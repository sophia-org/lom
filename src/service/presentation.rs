use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationPhase {
    ResourceBegun,
    ResourceUploaded,
    DemandSent,
    CandidateSubmitted,
    ResourceRetiring,
}

pub(super) struct PendingPresentation {
    panel: usize,
    slot_index: usize,
    resource: ContentResourceId,
    old: Option<ContentResourceId>,
    candidate: u64,
    demand_id: u64,
    upload_transaction: TransactionId,
    content: RenderedContent,
    phase: PresentationPhase,
    deadline: Instant,
    presentation_epoch: u64,
}

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn begin_presentation(
        &mut self,
        panel: usize,
        content: RenderedContent,
    ) -> Result<(), String> {
        if self.presentation.is_some() {
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
        let candidate = self.next_candidate_generation;
        let demand_id = self.next_demand;
        self.next_demand = demand_id
            .checked_add(1)
            .ok_or("frame demand identity exhausted")?;
        let upload_transaction = self.transaction()?;
        let pixels = &content.pixels;
        let chunk_count = u32::try_from(
            pixels
                .chunks(self.limits.max_frame_payload, self.limits.max_chunk_bytes)?
                .count(),
        )
        .map_err(|_| "resource chunk count exceeds the protocol")?;
        let allocation = &self.panels[panel].allocation;
        self.connection
            .send_content(
                upload_transaction,
                &ShellContentRecord::ResourceBegin(ContentResourceBegin {
                    grant: self.limits.grant,
                    resource,
                    width_px: pixels.width(),
                    height_px: pixels.height(),
                    rendered_scale_numerator: allocation.scale_numerator,
                    rendered_scale_denominator: allocation.scale_denominator,
                    pixel_format: 1,
                    chunk_count,
                    total_bytes: pixels.bytes().len() as u64,
                }),
            )
            .map_err(|error| format!("resource begin failed: {error}"))?;
        self.presentation = Some(PendingPresentation {
            panel,
            slot_index,
            resource,
            old,
            candidate,
            demand_id,
            upload_transaction,
            content,
            phase: PresentationPhase::ResourceBegun,
            deadline: Instant::now() + RESPONSE_TIMEOUT,
            presentation_epoch: 0,
        });
        Ok(())
    }

    pub(super) fn advance_presentation(&mut self) -> Result<bool, String> {
        let Some(mut pending) = self.presentation.take() else {
            return Ok(false);
        };
        if Instant::now() >= pending.deadline {
            return Err(format!("content {:?} response timed out", pending.phase));
        }
        match pending.phase {
            PresentationPhase::ResourceBegun => self.advance_resource_begin(&mut pending)?,
            PresentationPhase::ResourceUploaded => self.advance_resource_upload(&mut pending)?,
            PresentationPhase::DemandSent => self.advance_demand(&mut pending)?,
            PresentationPhase::CandidateSubmitted => self.advance_candidate(&mut pending)?,
            PresentationPhase::ResourceRetiring => self.advance_retirement(&mut pending)?,
        };
        if pending.phase == PresentationPhase::ResourceRetiring
            && pending.old.is_none()
            && pending.presentation_epoch != 0
        {
            self.finish_presentation(pending)?;
            return Ok(true);
        }
        self.presentation = Some(pending);
        Ok(false)
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
        let pixels = &pending.content.pixels;
        let chunks = pixels
            .chunks(self.limits.max_frame_payload, self.limits.max_chunk_bytes)?
            .collect::<Vec<_>>();
        let chunk_count =
            u32::try_from(chunks.len()).map_err(|_| "resource chunk count overflow")?;
        for chunk in chunks {
            self.connection
                .send_content(
                    pending.upload_transaction,
                    &ShellContentRecord::ResourceChunk(ContentResourceChunk {
                        grant: self.limits.grant,
                        resource: pending.resource,
                        ordinal: chunk.ordinal,
                        offset: chunk.offset,
                        bytes: chunk.bytes.to_vec(),
                    }),
                )
                .map_err(|error| format!("resource chunk failed: {error}"))?;
        }
        self.connection
            .send_content(
                pending.upload_transaction,
                &ShellContentRecord::ResourceEnd(ContentResourceEnd {
                    grant: self.limits.grant,
                    resource: pending.resource,
                    total_bytes: pixels.bytes().len() as u64,
                    chunk_count,
                }),
            )
            .map_err(|error| format!("resource end failed: {error}"))?;
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
        let panel = &self.panels[pending.panel];
        self.send(ShellContentRecord::FrameDemand(ContentFrameDemand {
            grant: self.limits.grant,
            output: panel.output.output,
            allocation: panel.allocation.allocation,
            demand_id: pending.demand_id,
            reason: 1,
        }))?;
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
        self.submit_candidate(
            pending.panel,
            pending.resource,
            pending.candidate,
            permit,
            &pending.content.targets,
        )?;
        pending.phase = PresentationPhase::CandidateSubmitted;
        pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
        Ok(true)
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
        if let Some(old) = pending.old {
            self.send(ShellContentRecord::ResourceRetire(ContentResourceRetire {
                grant: self.limits.grant,
                resource: old,
            }))?;
            pending.phase = PresentationPhase::ResourceRetiring;
            pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
            return Ok(true);
        }
        pending.phase = PresentationPhase::ResourceRetiring;
        Ok(true)
    }

    fn advance_retirement(&mut self, pending: &mut PendingPresentation) -> Result<bool, String> {
        let Some(old) = pending.old else {
            return Ok(false);
        };
        let Some(ShellContentRecord::ResourceReleased(released)) = self.take_pending(|record| {
            matches!(record, ShellContentRecord::ResourceReleased(value) if value.resource == old)
        }) else {
            return Ok(false);
        };
        if released.reason != ContentReason::None as u16 {
            return Err(format!("resource release failed: {}", released.reason));
        }
        pending.old = None;
        Ok(true)
    }

    fn finish_presentation(&mut self, pending: PendingPresentation) -> Result<(), String> {
        let panel = &mut self.panels[pending.panel];
        if panel.current_slot.is_some() {
            let old_index = 1 - pending.slot_index;
            panel.resources[old_index].generation = panel.resources[old_index]
                .generation
                .checked_add(1)
                .ok_or("resource generation exhausted")?;
        }
        let pixels = &pending.content.pixels;
        let indicator_generation = self
            .latest_indicators
            .as_ref()
            .map_or(0, |snapshot| snapshot.generation);
        println!(
            "lom_panel_candidate schema=1 status=presented output={} candidate_generation={} indicator_generation={} width={} height={} bytes={} checksum={:016x}",
            panel.output.output.id,
            pending.candidate,
            indicator_generation,
            pixels.width(),
            pixels.height(),
            pixels.bytes().len(),
            pixel_checksum(pixels.bytes()),
        );
        panel.current_slot = Some(pending.slot_index);
        panel.presented = Some(PresentedPanel {
            candidate_generation: pending.candidate,
            presentation_epoch: pending.presentation_epoch,
            targets: pending.content.targets,
        });
        self.next_candidate_generation = pending
            .candidate
            .checked_add(1)
            .ok_or("candidate generation exhausted")?;
        Ok(())
    }

    fn take_pending(
        &mut self,
        predicate: impl Fn(&ShellContentRecord) -> bool,
    ) -> Option<ShellContentRecord> {
        let at = self.pending_content.iter().position(predicate)?;
        self.pending_content.remove(at)
    }

    fn submit_candidate(
        &mut self,
        panel_index: usize,
        resource: ContentResourceId,
        candidate: u64,
        permit: ContentFramePermit,
        targets: &[ContentTargetLayout],
    ) -> Result<(), String> {
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
        for record in [begin, chunk, end] {
            self.send(record)?;
        }
        Ok(())
    }
}
