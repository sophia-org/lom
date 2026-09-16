use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationPhase {
    ResourceUnqueued,
    ResourceBegun,
    ResourceUploading,
    ResourceUploaded,
    ResourceAccepted,
    DemandSent,
    PermitGranted,
    CandidateSubmitted,
    RetireNeeded,
    Presented,
}

pub(super) struct PendingPresentation {
    pub(super) panel: usize,
    slot_index: usize,
    resource: ContentResourceId,
    old: Option<ContentResourceId>,
    candidate: u64,
    demand_id: u64,
    upload_transaction: TransactionId,
    next_chunk: u32,
    permit: Option<ContentFramePermit>,
    content: Option<RenderedContent>,
    targets: Vec<ContentTargetLayout>,
    raster: RasterSummary,
    raster_reused: bool,
    phase: PresentationPhase,
    deadline: Instant,
    presentation_epoch: u64,
    indicator_revision: u64,
    model: Model,
}

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn begin_presentation(
        &mut self,
        panel: usize,
        content: RenderedContent,
    ) -> Result<(), String> {
        if self
            .presentations
            .iter()
            .any(|pending| pending.panel == panel)
        {
            return Err("a content presentation is already in flight".into());
        }
        let (slot_index, slot, old) = {
            let panel = &self.panels[panel];
            let slot_index = panel.current_slot.map_or(0, |slot| 1 - slot);
            (
                slot_index,
                panel.resources[slot_index],
                panel.current_slot.map(|index| panel.resources[index]),
            )
        };
        let resource = ContentResourceId {
            id: slot.id,
            generation: slot.generation,
        };
        let old = old.map(|slot| ContentResourceId {
            id: slot.id,
            generation: slot.generation,
        });
        let candidate = 0;
        let pixels = &content.pixels;
        self.panels[panel].resources[slot_index] = ResourceSlot {
            id: slot.id,
            generation: slot.generation,
            bytes: pixels.bytes().len() as u64,
            state: ResourceState::Staging,
            release_deadline: None,
        };
        let upload_transaction = self.transaction()?;
        self.presentations.push(PendingPresentation {
            panel,
            slot_index,
            resource,
            old,
            candidate,
            demand_id: 0,
            upload_transaction,
            next_chunk: 0,
            permit: None,
            raster: RasterSummary {
                width: pixels.width(),
                height: pixels.height(),
                bytes: pixels.bytes().len(),
                checksum: pixel_checksum(pixels.bytes()),
            },
            targets: content.targets.clone(),
            content: Some(content),
            raster_reused: false,
            phase: PresentationPhase::ResourceUnqueued,
            deadline: Instant::now() + RESPONSE_TIMEOUT,
            presentation_epoch: 0,
            indicator_revision: self.rendering_revision,
            model: self
                .rendering_model
                .take()
                .ok_or("render completion lost its originating model")?,
        });
        Ok(())
    }

    pub(super) fn refresh_interaction(&mut self, index: usize) -> Result<(), String> {
        let panel = &self.panels[index];
        if panel.dirty
            || !panel.interaction_dirty
            || self.rendering_panel == Some(index)
            || self
                .presentations
                .iter()
                .any(|pending| pending.panel == index)
            || self.presentations.len() >= self.limits.max_pending_candidates_total as usize
        {
            return Ok(());
        }
        let Some(presented) = panel.presented.as_ref() else {
            return Ok(());
        };
        let Some(targets) =
            crate::ui::retarget_unchanged_panel(&presented.model, &panel.model, &presented.targets)
        else {
            self.panels[index].dirty = true;
            return Ok(());
        };
        if targets == presented.targets {
            self.panels[index].interaction_dirty = false;
            return Ok(());
        }
        let slot_index = panel
            .current_slot
            .ok_or("presented panel lost its resource")?;
        let slot = panel.resources[slot_index];
        if slot.state != ResourceState::Resident {
            return Err("interaction refresh lost resident pixels".into());
        }
        self.presentations.push(PendingPresentation {
            panel: index,
            slot_index,
            resource: ContentResourceId {
                id: slot.id,
                generation: slot.generation,
            },
            old: None,
            candidate: 0,
            demand_id: 0,
            upload_transaction: TransactionId::from_raw(0),
            next_chunk: 0,
            permit: None,
            content: None,
            raster_reused: true,
            targets,
            raster: presented.raster,
            phase: PresentationPhase::ResourceAccepted,
            deadline: Instant::now() + RESPONSE_TIMEOUT,
            presentation_epoch: 0,
            indicator_revision: panel.model.workspaces.generation,
            model: panel.model.clone(),
        });
        self.panels[index].interaction_dirty = false;
        Ok(())
    }

    pub(super) fn advance_presentation(
        &mut self,
        mut pending: PendingPresentation,
    ) -> Result<bool, String> {
        if Instant::now() >= pending.deadline {
            return Err(format!("content {:?} response timed out", pending.phase));
        }
        match pending.phase {
            PresentationPhase::ResourceUnqueued => self.advance_resource_start(&mut pending)?,
            PresentationPhase::ResourceAccepted => self.advance_demand_enqueue(&mut pending)?,
            PresentationPhase::PermitGranted => self.advance_candidate_enqueue(&mut pending)?,
            PresentationPhase::RetireNeeded => self.advance_retire_enqueue(&mut pending)?,
            PresentationPhase::ResourceBegun => self.advance_resource_begin(&mut pending)?,
            PresentationPhase::ResourceUploading => self.advance_resource_chunks(&mut pending)?,
            PresentationPhase::ResourceUploaded => self.advance_resource_upload(&mut pending)?,
            PresentationPhase::DemandSent => self.advance_demand(&mut pending)?,
            PresentationPhase::CandidateSubmitted => self.advance_candidate(&mut pending)?,
            PresentationPhase::Presented => false,
        };
        if pending.phase == PresentationPhase::Presented {
            self.finish_presentation(pending)?;
            return Ok(true);
        }
        self.presentations.push(pending);
        Ok(false)
    }

    fn advance_resource_start(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        let pixels = &pending
            .content
            .as_ref()
            .ok_or("upload lost its raster")?
            .pixels;
        let resource = ContentResourceId {
            id: if pending.resource.id == 0 {
                self.next_resource
            } else {
                pending.resource.id
            },
            generation: pending.resource.generation,
        };
        let allocation = &self.panels[pending.panel].allocation;
        let begin = ShellContentRecord::ResourceBegin(ContentResourceBegin {
            grant: self.limits.grant,
            resource,
            width_px: pixels.width(),
            height_px: pixels.height(),
            rendered_scale_numerator: allocation.scale_numerator,
            rendered_scale_denominator: allocation.scale_denominator,
            pixel_format: 1,
            chunk_count: pixels
                .chunks(self.limits.max_frame_payload, self.limits.max_chunk_bytes)?
                .count() as u32,
            total_bytes: pixels.bytes().len() as u64,
        });
        match self
            .connection
            .enqueue_content(pending.upload_transaction, &begin)
        {
            Ok(()) => {}
            Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
            Err(error) => return Err(format!("resource begin failed: {error}")),
        }
        if pending.resource.id == 0 {
            self.next_resource = self
                .next_resource
                .checked_add(1)
                .ok_or("resource identity exhausted")?;
        }
        pending.resource = resource;
        self.panels[pending.panel].resources[pending.slot_index].id = resource.id;
        pending.phase = PresentationPhase::ResourceBegun;
        pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
        Ok(true)
    }

    fn advance_resource_begin(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        let Some(ShellContentRecord::ResourceStatus(status)) =
            self.take_pending(|record| matches!(record, ShellContentRecord::ResourceStatus(value) if value.resource == pending.resource))
        else {
            return Ok(false);
        };
        if status.status != 1 || status.reason != ContentReason::None as u16 {
            return Err(format!("resource begin was rejected: {}", status.reason));
        }
        pending.phase = PresentationPhase::ResourceUploading;
        self.advance_resource_chunks(pending)
    }

    fn advance_resource_chunks(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        let pixels = &pending
            .content
            .as_ref()
            .ok_or("upload lost its raster")?
            .pixels;
        let chunk_count = pixels
            .chunks(self.limits.max_frame_payload, self.limits.max_chunk_bytes)?
            .count() as u32;
        // At most four whole-row chunks (under 256 KiB) per output turn.
        for chunk in pixels
            .chunks(self.limits.max_frame_payload, self.limits.max_chunk_bytes)?
            .skip(pending.next_chunk as usize)
            .take(self.upload_chunks_left)
        {
            match self.connection.enqueue_content(
                pending.upload_transaction,
                &ShellContentRecord::ResourceChunk(ContentResourceChunk {
                    grant: self.limits.grant,
                    resource: pending.resource,
                    ordinal: chunk.ordinal,
                    offset: chunk.offset,
                    bytes: chunk.bytes.to_vec(),
                }),
            ) {
                Ok(()) => {
                    pending.next_chunk += 1;
                    self.upload_chunks_left -= 1;
                }
                Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
                Err(error) => return Err(format!("resource chunk failed: {error}")),
            }
        }
        if pending.next_chunk != chunk_count {
            return Ok(false);
        }
        match self.connection.enqueue_content(
            pending.upload_transaction,
            &ShellContentRecord::ResourceEnd(ContentResourceEnd {
                grant: self.limits.grant,
                resource: pending.resource,
                total_bytes: pixels.bytes().len() as u64,
                chunk_count,
            }),
        ) {
            Ok(()) => {}
            Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
            Err(error) => return Err(format!("resource end failed: {error}")),
        }
        pending.phase = PresentationPhase::ResourceUploaded;
        pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
        Ok(true)
    }

    fn advance_resource_upload(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        let Some(ShellContentRecord::ResourceStatus(status)) =
            self.take_pending(|record| matches!(record, ShellContentRecord::ResourceStatus(value) if value.resource == pending.resource))
        else {
            return Ok(false);
        };
        if status.status != RESOURCE_ACCEPTED || status.reason != ContentReason::None as u16 {
            return Err(format!("resource was rejected: {}", status.reason));
        }
        self.panels[pending.panel].resources[pending.slot_index].state = ResourceState::Resident;
        pending.content = None;
        pending.phase = PresentationPhase::ResourceAccepted;
        self.advance_demand_enqueue(pending)
    }

    fn advance_demand_enqueue(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        let panel = &self.panels[pending.panel];
        let record = ShellContentRecord::FrameDemand(ContentFrameDemand {
            grant: self.limits.grant,
            output: panel.output.output,
            allocation: panel.allocation.allocation,
            demand_id: self.next_demand,
            reason: 1,
        });
        let transaction = self.transaction()?;
        match self.connection.enqueue_content(transaction, &record) {
            Ok(()) => {}
            Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
            Err(error) => return Err(format!("frame demand failed: {error}")),
        }
        pending.demand_id = self.next_demand;
        self.next_demand = self
            .next_demand
            .checked_add(1)
            .ok_or("demand identity exhausted")?;
        pending.phase = PresentationPhase::DemandSent;
        pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
        Ok(true)
    }

    fn advance_demand(&mut self, pending: &mut PendingPresentation) -> Result<bool, String> {
        let output = self.panels[pending.panel].output.output;
        let Some(ShellContentRecord::FramePermit(permit)) = self.take_pending(|record| {
            matches!(record, ShellContentRecord::FramePermit(value) if value.output == output && value.demand_id == pending.demand_id)
        }) else {
            return Ok(false);
        };
        if permit.state != PERMIT_GRANTED || permit.reason != ContentReason::None as u16 {
            return Err(format!("frame demand was refused: {}", permit.reason));
        }
        pending.permit = Some(permit);
        pending.phase = PresentationPhase::PermitGranted;
        self.advance_candidate_enqueue(pending)
    }

    fn advance_candidate_enqueue(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        let candidate = self.next_candidate_generation;
        if !self.submit_candidate(
            pending.panel,
            pending.resource,
            candidate,
            pending.permit.clone().ok_or("candidate lost its permit")?,
            &pending.targets,
        )? {
            return Ok(false);
        }
        pending.candidate = candidate;
        pending.permit = None;
        self.next_candidate_generation = candidate
            .checked_add(1)
            .ok_or("candidate generation exhausted")?;
        pending.phase = PresentationPhase::CandidateSubmitted;
        pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
        Ok(true)
    }

    pub(super) fn install_presented_targets(
        &mut self,
        outcome: &sophia_protocol::ContentCandidateOutcome,
    ) -> Result<(), String> {
        let pending = self
            .presentations
            .iter()
            .find(|pending| pending.candidate == outcome.candidate_generation)
            .ok_or("Presented has no pending candidate")?;
        if pending.candidate != outcome.candidate_generation
            || self.panels[pending.panel].output.output != outcome.output
        {
            return Err("Presented names a different pending candidate".into());
        }
        self.panels[pending.panel].presented = Some(PresentedPanel {
            model: pending.model.clone(),
            candidate_generation: pending.candidate,
            presentation_epoch: outcome.presentation_epoch,
            targets: pending.targets.clone(),
            raster: pending.raster,
        });
        Ok(())
    }

    fn advance_candidate(&mut self, pending: &mut PendingPresentation) -> Result<bool, String> {
        let output = self.panels[pending.panel].output.output;
        let Some(ShellContentRecord::CandidateOutcome(outcome)) = self.take_pending(|record| {
            matches!(record, ShellContentRecord::CandidateOutcome(value) if value.output == output && value.candidate_generation == pending.candidate)
        }) else {
            return Ok(false);
        };
        if outcome.kind == 3 {
            return Err(format!(
                "content candidate was rejected: {}",
                outcome.reason
            ));
        }
        if outcome.kind != CANDIDATE_PRESENTED {
            return Ok(true);
        }
        if outcome.reason != ContentReason::None as u16 || outcome.presentation_epoch == 0 {
            return Err("presented content candidate carried an invalid outcome".into());
        }
        pending.presentation_epoch = outcome.presentation_epoch;
        pending.phase = PresentationPhase::RetireNeeded;
        self.advance_retire_enqueue(pending)
    }

    fn advance_retire_enqueue(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        if let Some(old) = pending.old {
            let old_bytes = self.panels[pending.panel].resources[1 - pending.slot_index].bytes;
            let retiring: u64 = self
                .panels
                .iter()
                .flat_map(|panel| &panel.resources)
                .filter(|slot| slot.state == ResourceState::Retiring)
                .map(|slot| slot.bytes)
                .sum();
            if retiring + old_bytes > self.limits.max_retiring_bytes {
                return Ok(false);
            }
            let transaction = self.transaction()?;
            match self.connection.enqueue_content(
                transaction,
                &ShellContentRecord::ResourceRetire(ContentResourceRetire {
                    grant: self.limits.grant,
                    resource: old,
                }),
            ) {
                Ok(()) => {}
                Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
                Err(error) => return Err(format!("resource retire failed: {error}")),
            }
            let slot = &mut self.panels[pending.panel].resources[1 - pending.slot_index];
            slot.state = ResourceState::Retiring;
            slot.release_deadline = Some(Instant::now() + RESPONSE_TIMEOUT);
        }
        pending.phase = PresentationPhase::Presented;
        Ok(true)
    }

    fn finish_presentation(&mut self, pending: PendingPresentation) -> Result<(), String> {
        let panel = &mut self.panels[pending.panel];
        let raster = pending.raster;
        let indicator_generation = pending.indicator_revision;
        println!(
            "lom_panel_candidate schema=1 status=presented connection_epoch={} content_grant_epoch={} output={} candidate_generation={} presentation_epoch={} indicator_generation={} width={} height={} bytes={} checksum={:016x} raster_source={}",
            self.limits.grant.connection_epoch,
            self.limits.grant.content_grant_epoch,
            panel.output.output.id,
            pending.candidate,
            pending.presentation_epoch,
            indicator_generation,
            raster.width,
            raster.height,
            raster.bytes,
            raster.checksum,
            if pending.raster_reused {
                "reused"
            } else {
                "rendered"
            },
        );
        panel.current_slot = Some(pending.slot_index);
        panel.presented = Some(PresentedPanel {
            model: pending.model,
            candidate_generation: pending.candidate,
            presentation_epoch: pending.presentation_epoch,
            targets: pending.targets,
            raster: pending.raster,
        });
        Ok(())
    }

    fn take_pending(
        &mut self,
        predicate: impl Fn(&ShellContentRecord) -> bool,
    ) -> Option<ShellContentRecord> {
        let at = self
            .pending_content
            .iter()
            .position(|(_, record)| predicate(record))?;
        self.pending_content.remove(at).map(|(_, record)| record)
    }

    fn submit_candidate(
        &mut self,
        panel_index: usize,
        resource: ContentResourceId,
        candidate: u64,
        permit: ContentFramePermit,
        targets: &[ContentTargetLayout],
    ) -> Result<bool, String> {
        let panel = &self.panels[panel_index];
        let reservation_extent = match self.config.position {
            Position::Top | Position::Bottom => panel.allocation.pixel.height,
            Position::Left | Position::Right => panel.allocation.pixel.width,
        };
        let surface = ContentSurface {
            allocation: panel.allocation.allocation,
            scale_generation: panel.allocation.scale_generation,
            role: PANEL_ROLE,
            edge: edge(self.config.position),
            margins: panel.allocation.margins,
            reservation_extent,
            parent_surface_index: u16::MAX,
            anchor_parent_rect: ContentPixelRect::default(),
        };
        let target_count = u32::try_from(targets.len()).map_err(|_| "target count overflow")?;
        let begin = ShellContentRecord::CandidateBegin(ContentCandidateBegin {
            grant: self.limits.grant,
            candidate_generation: candidate,
            output: panel.output.output,
            facts_generation: self.facts.facts_generation,
            pacing_permit: permit.permit_id,
            interaction_generation: 1,
            surface_count: 1,
            placement_count: 1,
            target_count,
        });
        let chunk = ShellContentRecord::CandidateChunk(ContentCandidateChunk {
            grant: self.limits.grant,
            candidate_generation: candidate,
            chunk_ordinal: 0,
            surfaces: vec![surface],
            placements: vec![ContentPlacement {
                resource,
                surface_index: 0,
                destination_x_px: 0,
                destination_y_px: 0,
            }],
            targets: targets
                .iter()
                .map(|target| ContentTarget {
                    surface_index: 0,
                    action_kind: 1,
                    target_id: target.indicator,
                    target_generation: target.generation,
                    action_id: target.action,
                    bounds_px: ContentPixelRect {
                        x: target.x,
                        y: target.y,
                        width: target.width,
                        height: target.height,
                    },
                })
                .collect(),
        });
        let end = ShellContentRecord::CandidateEnd(ContentCandidateEnd {
            grant: self.limits.grant,
            candidate_generation: candidate,
            surface_count: 1,
            placement_count: 1,
            target_count,
        });
        let transaction = self.transaction()?;
        match self.connection.enqueue_candidate(
            &mut self.lifecycle,
            transaction,
            &[begin, chunk, end],
        ) {
            Ok(()) => {}
            Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
            Err(error) => return Err(format!("candidate outbox admission: {error}")),
        }
        Ok(true)
    }
}
