use super::*;

#[cfg(feature = "experimental-frame-transactions")]
mod frames;
#[cfg(feature = "experimental-frame-transactions")]
pub(super) use frames::FrameOutput;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationPhase {
    AwaitRender,
    ResourceUnqueued,
    ImageOffered,
    ResourceAccepted,
    DemandSent,
    PermitGranted,
    CandidateSubmitted,
    RetireNeeded,
    Presented,
}

pub(super) struct PendingPresentation {
    #[cfg(feature = "experimental-frame-transactions")]
    frame: Option<frames::PendingFrame>,
    pub(super) panel: usize,
    slot_index: usize,
    resource: ContentResourceId,
    update: UpdatePlan,
    discard: bool,
    uploads: u32,
    upload_bytes: u64,
    candidate: u64,
    demand_id: u64,
    offer: Option<u64>,
    accepted: bool,
    permit: Option<ContentFramePermit>,
    content: Option<RenderedContent>,
    /// The owner's acquire-time snapshot for a direct fill.
    fill: Option<super::ring_fill::FillTarget>,
    targets: Vec<ContentTargetLayout>,
    targets_prepared: bool,
    raster: RasterSummary,
    raster_reused: bool,
    phase: PresentationPhase,
    pub(super) deadline: Instant,
    presentation_epoch: u64,
    indicator_revision: u64,
    model: Model,
    #[cfg(feature = "diagnostic-cpu-reference")]
    timing: super::cadence::Timing,
}

impl PendingPresentation {
    pub(super) fn render_request(&self) -> Option<(regions::Placement, Model, bool)> {
        (self.phase == PresentationPhase::AwaitRender).then(|| {
            (
                *self.update.jobs.front().expect("awaiting planned region"),
                self.model.clone(),
                self.update.inspect_opacity,
            )
        })
    }
}

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn begin_presentation(
        &mut self,
        panel: usize,
        mut content: RenderedContent,
    ) -> Result<(), String> {
        let model = self
            .rendering_model
            .take()
            .ok_or("render completion lost its originating model")?;
        let fill = self.rendering_fill.take();
        if fill.is_some() != matches!(content.pixels, RenderImage::RingFill { .. }) {
            return Err("owned ring fill and its owner snapshot diverged".into());
        }
        let mut pending = if let Some(at) = self.presentations.iter().position(|p| p.panel == panel)
        {
            let pending = self.presentations.remove(at);
            if pending.phase != PresentationPhase::AwaitRender || pending.model != model {
                return Err("render completion does not belong to the immutable update".into());
            }
            if pending.targets != content.targets {
                return Err("region render changed the immutable update targets".into());
            }
            pending
        } else {
            let update = self
                .rendering_update
                .take()
                .ok_or("render completion lost its raster plan")?;
            let job = *update.jobs.front().ok_or("empty raster plan")?;
            let slot = self.panels[panel].resources[job.slot];
            #[cfg(feature = "diagnostic-cpu-reference")]
            let timing = self.cadence_job();
            let allocation = &self.panels[panel].allocation;
            PendingPresentation {
                #[cfg(feature = "experimental-frame-transactions")]
                frame: None,
                panel,
                slot_index: job.slot,
                resource: ContentResourceId {
                    id: slot.id,
                    generation: slot.generation,
                },
                update,
                discard: false,
                uploads: 0,
                upload_bytes: 0,
                candidate: 0,
                demand_id: 0,
                offer: None,
                accepted: false,
                permit: None,
                content: None,
                fill: None,
                targets: content.targets.clone(),
                targets_prepared: false,
                raster: RasterSummary {
                    width: allocation.pixel.width,
                    height: allocation.pixel.height,
                    bytes: crate::protocol::ContentPixels::byte_len(
                        allocation.pixel.width,
                        allocation.pixel.height,
                    )?,
                    checksum: None,
                    opaque: true,
                },
                raster_reused: false,
                phase: PresentationPhase::AwaitRender,
                deadline: Instant::now() + RESPONSE_TIMEOUT,
                presentation_epoch: 0,
                indicator_revision: self.rendering_revision,
                model,
                #[cfg(feature = "diagnostic-cpu-reference")]
                timing,
            }
        };
        let job = *pending
            .update
            .jobs
            .front()
            .ok_or("completion has no region job")?;
        if job.slot != pending.slot_index
            || content.pixels.width() != job.rect.width
            || content.pixels.height() != job.rect.height
        {
            return Err("render completion changed the planned geometry".into());
        }
        let opaque = matches!(content.pixels, RenderImage::RingFill { opaque: true, .. });
        if pending.update.tiled && !opaque {
            let RenderImage::RingFill { fill, .. } = &mut content.pixels else {
                return Err("region render lost its owned loan".into());
            };
            self.abort_fill(fill)?;
            self.free_reserved(panel, &pending.update);
            pending.update.old = pending
                .update
                .next
                .placements()
                .filter(|p| {
                    !self.panels[panel].current.contains(p.slot)
                        && self.panels[panel].resources[p.slot].state == ResourceState::Accepted
                })
                .map(|p| p.slot)
                .collect();
            pending.update.jobs.clear();
            pending.discard = true;
            pending.phase = PresentationPhase::RetireNeeded;
            self.panels[panel].regions_disabled = true;
        } else {
            if pending.update.inspect_opacity && !opaque {
                self.panels[panel].regions_disabled = true;
            }
            pending.raster.opaque &= opaque;
            if !pending.update.tiled {
                pending.raster.checksum = content.pixels.checksum();
            }
            let slot = &mut self.panels[panel].resources[job.slot];
            if slot.state != ResourceState::Reserved
                || slot.bytes != content.pixels.byte_len() as u64
            {
                return Err("render completion lost its budget reservation".into());
            }
            slot.state = ResourceState::Staging;
            pending.fill = fill;
            pending.content = Some(content);
            pending.phase = PresentationPhase::ResourceUnqueued;
        }
        self.presentations.push(pending);
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
            .current
            .placements()
            .next()
            .ok_or("presented panel lost its resources")?
            .slot;
        let slot = panel.resources[slot_index];
        if panel
            .current
            .placements()
            .any(|p| panel.resources[p.slot].state != ResourceState::Resident)
        {
            return Err("interaction refresh lost resident pixels".into());
        }
        self.presentations.push(PendingPresentation {
            #[cfg(feature = "experimental-frame-transactions")]
            frame: None,
            panel: index,
            slot_index,
            resource: ContentResourceId {
                id: slot.id,
                generation: slot.generation,
            },
            update: UpdatePlan {
                next: panel.current,
                jobs: VecDeque::new(),
                old: VecDeque::new(),
                tiled: panel.current.placements().count() == 3,
                inspect_opacity: false,
            },
            discard: false,
            uploads: 0,
            upload_bytes: 0,
            candidate: 0,
            demand_id: 0,
            offer: None,
            accepted: false,
            permit: None,
            content: None,
            fill: None,
            raster_reused: true,
            targets_prepared: false,
            targets,
            raster: presented.raster,
            phase: PresentationPhase::ResourceAccepted,
            deadline: Instant::now() + RESPONSE_TIMEOUT,
            presentation_epoch: 0,
            indicator_revision: panel.model.workspaces.generation,
            model: panel.model.clone(),
            #[cfg(feature = "diagnostic-cpu-reference")]
            timing: Default::default(),
        });
        self.panels[index].interaction_dirty = false;
        self.progressed = true;
        Ok(())
    }

    pub(super) fn advance_presentation(
        &mut self,
        mut pending: PendingPresentation,
    ) -> Result<bool, String> {
        let old_phase = pending.phase;
        let progressed = match self.advance_phase(&mut pending) {
            Ok(progressed) => progressed,
            Err(error) => {
                // An uncommitted loan is aborted before the terminal error.
                // A committed (inert) handle refuses the abort and is left to
                // settlement and teardown; nothing is rolled back.
                if let Some(RenderedContent {
                    pixels: RenderImage::RingFill { fill, .. },
                    ..
                }) = pending.content.as_mut()
                {
                    let _ = self.connection.image_abort_ring_fill(fill);
                }
                return Err(error);
            }
        };
        self.progressed |= progressed || pending.phase != old_phase;
        if pending.phase == PresentationPhase::Presented {
            if pending.discard {
                self.panels[pending.panel].dirty = true;
                self.panels[pending.panel].interaction_dirty = true;
                return Ok(false);
            }
            self.finish_presentation(pending)?;
            return Ok(true);
        }
        self.presentations.push(pending);
        Ok(false)
    }

    fn advance_phase(&mut self, pending: &mut PendingPresentation) -> Result<bool, String> {
        if Instant::now() >= pending.deadline {
            return Err(format!("content {:?} response timed out", pending.phase));
        }
        #[cfg(feature = "experimental-frame-transactions")]
        if pending.frame.is_some()
            && matches!(
                pending.phase,
                PresentationPhase::CandidateSubmitted | PresentationPhase::RetireNeeded
            )
        {
            return self.advance_frame_progress(pending);
        }
        match pending.phase {
            PresentationPhase::AwaitRender => Ok(false),
            PresentationPhase::ResourceUnqueued => self.advance_resource_start(pending),
            PresentationPhase::ResourceAccepted => self.advance_demand_enqueue(pending),
            PresentationPhase::PermitGranted => self.advance_candidate_enqueue(pending),
            PresentationPhase::RetireNeeded => self.advance_retire_enqueue(pending),
            PresentationPhase::ImageOffered => self.advance_image_offered(pending),
            PresentationPhase::DemandSent => self.advance_demand(pending),
            PresentationPhase::CandidateSubmitted => self.advance_candidate(pending),
            PresentationPhase::Presented => Ok(false),
        }
    }

    fn advance_resource_start(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        #[cfg(feature = "experimental-frame-transactions")]
        if let Some(progressed) = self.frame_before_upload(pending)? {
            return Ok(progressed);
        }
        // Neither transport may send a source before ChannelReady.
        let Some(adapter) = self.connection.image_adapter() else {
            return Err("image channel was not enabled".into());
        };
        match adapter.channel() {
            Some(sophia_shell_client::ImageChannelState::Ready) => {}
            Some(sophia_shell_client::ImageChannelState::Closed) => {
                return Err("image channel closed before Ready".into());
            }
            _ => return Ok(false),
        }
        let (width, height, byte_len) = {
            let pixels = &pending
                .content
                .as_ref()
                .ok_or("upload lost its raster")?
                .pixels;
            (pixels.width(), pixels.height(), pixels.byte_len())
        };
        let resource = ContentResourceId {
            id: if pending.resource.id == 0 {
                self.next_resource
            } else {
                pending.resource.id
            },
            generation: pending.resource.generation,
        };
        let panel = &self.panels[pending.panel];
        let upload = sophia_shell_client::RingUpload {
            resource: resource.id,
            resource_generation: resource.generation,
            allocation: panel.allocation.allocation.id,
            allocation_generation: panel.allocation.allocation.generation,
            output: panel.output.output.id,
            output_generation: panel.output.output.generation,
            topology_generation: self.facts.facts_generation,
            width,
            height,
        };
        let (index, slot) = (pending.panel, pending.slot_index);
        let fresh_slot = self.panels[index].resources[slot];
        let pixels = &mut pending
            .content
            .as_mut()
            .ok_or("upload lost its raster")?
            .pixels;
        let offered = match pixels {
            RenderImage::Cpu(pixels) => self
                .connection
                .image_upload_ring(upload, pixels.bytes(), Instant::now())
                .map_err(sophia_shell_client::ShellClientError::Image),
            RenderImage::Dma(frame) => {
                let metadata = sophia_shell_client::ProducerMetadata {
                    offer_id: 0,
                    resource: resource.id,
                    resource_generation: resource.generation,
                    admission_epoch: self.limits.grant.content_grant_epoch,
                    output_id: panel.output.output.id,
                    allocation_id: panel.allocation.allocation.id,
                    topology_generation: self.facts.facts_generation,
                    source_kind: 2,
                    source_format: u32::from_le_bytes(*b"XR24"),
                    width: frame.width(),
                    height: frame.height(),
                    stride: 0,
                    plane_count: 1,
                    descriptor_count: 1,
                    declared_bytes: byte_len as u64,
                    modifier: 0,
                };
                self.connection
                    .image_begin_dma_buf(
                        metadata,
                        frame.dup_fd()?,
                        frame.offset(),
                        frame.stride(),
                        Instant::now(),
                    )
                    .map(|admission| admission.offer)
                    .map_err(|error| sophia_shell_client::ShellClientError::Image(error.error))
            }
            RenderImage::RingFill {
                fill,
                identity,
                width,
                height,
                region,
                ..
            } => {
                // Recheck the acquire-time snapshot, then publish exactly it.
                // A stale or refused commit is terminal like every other
                // service error: advance_presentation aborts the loan first,
                // so no Offer or resource ID is spent, and the caller must
                // drop and restart the service. The ID is spent only after
                // the Offer below succeeds.
                let target = pending
                    .fill
                    .ok_or("owned ring fill lost its owner snapshot")?;
                if !self.fill_unchanged(index, &target, *identity, *width, *height)
                    || *region != target.region
                    || fresh_slot.state != ResourceState::Staging
                    || fresh_slot.bytes != byte_len as u64
                {
                    return Err("owned ring fill became stale before commit".into());
                }
                self.offer_completed_fill(index, target.upload, fill)
            }
        };
        match offered {
            Ok(offer) => self.record_image_offer(pending, resource, offer, byte_len),
            Err(error) => {
                use sophia_shell_client::ImageAdapterError;
                if matches!(
                    error,
                    sophia_shell_client::ShellClientError::Image(
                        ImageAdapterError::Busy | ImageAdapterError::Again
                    )
                ) {
                    // A ring slot or control write is busy; the
                    // pixels wait in the pending presentation.
                    #[cfg(feature = "diagnostic-cpu-reference")]
                    self.cadence_deferred(false);
                    return Ok(false);
                }
                Err(format!("owned-ring image upload refused: {error:?}"))
            }
        }
    }

    fn record_image_offer(
        &mut self,
        pending: &mut PendingPresentation,
        resource: ContentResourceId,
        offer: u64,
        byte_len: usize,
    ) -> Result<bool, String> {
        #[cfg(feature = "diagnostic-cpu-reference")]
        self.cadence_stamp(&mut pending.timing.ring_commit_and_offer_queued)?;
        if let Some(stats) = &mut self.statistics {
            stats.uploads = stats.uploads.saturating_add(1);
            stats.upload_bytes = stats.upload_bytes.saturating_add(byte_len as u64);
        }
        if pending.resource.id == 0 {
            self.next_resource = self
                .next_resource
                .checked_add(1)
                .ok_or("resource identity exhausted")?;
        }
        pending.resource = resource;
        pending.uploads += 1;
        pending.upload_bytes += byte_len as u64;
        self.panels[pending.panel].resources[pending.slot_index].id = resource.id;
        pending.offer = Some(offer);
        pending.phase = PresentationPhase::ImageOffered;
        pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
        Ok(true)
    }

    fn advance_image_offered(&mut self, pending: &mut PendingPresentation) -> Result<bool, String> {
        if !pending.accepted {
            let Some(ShellContentRecord::ResourceStatus(status)) =
                self.take_pending(|record| matches!(record, ShellContentRecord::ResourceStatus(value) if value.resource == pending.resource))
            else {
                return Ok(false);
            };
            if status.status != RESOURCE_ACCEPTED || status.reason != ContentReason::None as u16 {
                return Err(format!("image resource was rejected: {}", status.reason));
            }
            pending.accepted = true;
            pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
            return Ok(true);
        }
        let offer = pending.offer.ok_or("image offer was not recorded")?;
        if self.renderer.uses_dma_buf() {
            use sophia_image_producer::OfferState;
            let info = self
                .connection
                .image_adapter()
                .and_then(|a| a.dma_offer(offer))
                .ok_or("DMA offer vanished before settlement")?;
            match info.state {
                OfferState::Settled
                    if info.disposition == 2
                        && info.status
                            == Some(sophia_image_producer::ResourceStatusKind::Accepted) => {}
                OfferState::Settled | OfferState::Refused | OfferState::Unknown => {
                    return Err(format!(
                        "DMA source ended without capture completion: {:?}",
                        info.state
                    ));
                }
                _ => return Ok(false),
            }
            return self.finish_image_offer(pending, offer);
        }
        let info = self
            .connection
            .image_ring_offer(offer)
            .ok_or("image offer vanished before settlement")?;
        use sophia_shell_client::RingOfferState;
        match info.state {
            RingOfferState::Settled => {
                if info.status != Some(sophia_image_producer::ResourceStatusKind::Accepted) {
                    return Err("image settled without an Accepted status".into());
                }
                self.finish_image_offer(pending, offer)
            }
            RingOfferState::Refused | RingOfferState::Unknown => Err(format!(
                "image offer ended in {:?} before settlement",
                info.state
            )),
            RingOfferState::Pending => Ok(false),
        }
    }

    fn finish_image_offer(
        &mut self,
        pending: &mut PendingPresentation,
        offer: u64,
    ) -> Result<bool, String> {
        self.connection
            .image_forget(offer)
            .map_err(|error| format!("image forget: {error:?}"))?;
        self.panels[pending.panel].resources[pending.slot_index].state = ResourceState::Accepted;
        // Dropping a DMA frame is permitted only after CaptureComplete above.
        pending.content = None;
        pending.fill = None;
        pending.offer = None;
        pending
            .update
            .jobs
            .pop_front()
            .ok_or("settled image has no planned job")?;
        #[cfg(feature = "experimental-frame-transactions")]
        if pending.frame.is_some() {
            if !pending.update.jobs.is_empty() {
                return Err("SFT admitted more than one new image".into());
            }
            return Ok(true);
        }
        if let Some(job) = pending.update.jobs.front() {
            pending.slot_index = job.slot;
            let slot = self.panels[pending.panel].resources[job.slot];
            pending.resource = ContentResourceId {
                id: slot.id,
                generation: slot.generation,
            };
            pending.offer = None;
            pending.accepted = false;
            pending.phase = PresentationPhase::AwaitRender;
            pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
            Ok(true)
        } else {
            pending.phase = PresentationPhase::ResourceAccepted;
            self.advance_demand_enqueue(pending)
        }
    }

    fn advance_demand_enqueue(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        #[cfg(feature = "experimental-frame-transactions")]
        if let Some(progressed) = self.frame_before_demand(pending)? {
            return Ok(progressed);
        }
        let panel = &self.panels[pending.panel];
        let record = ShellContentRecord::FrameDemand(ContentFrameDemand {
            grant: self.limits.grant,
            output: panel.output.output,
            allocation: panel.allocation.allocation,
            demand_id: self.next_demand,
            reason: 1,
        });
        let transaction = self.transaction()?;
        match self.enqueue_content(transaction, &record) {
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
        pending.deadline = pending
            .deadline
            .min(Instant::now() + Duration::from_millis(u64::from(permit.ttl_ms)));
        pending.permit = Some(permit);
        pending.phase = PresentationPhase::PermitGranted;
        self.advance_candidate_enqueue(pending)
    }

    fn advance_candidate_enqueue(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        if !pending.targets_prepared {
            self.panels[pending.panel].target_generations.prepare(
                &pending.model,
                &mut pending.targets,
                self.limits.max_candidate_targets as usize,
            )?;
            pending.targets_prepared = true;
        }
        let candidate = self.next_candidate_generation;
        if !self.submit_candidate(
            pending.panel,
            pending.update.next,
            candidate,
            pending.permit.clone().ok_or("candidate lost its permit")?,
            &pending.targets,
        )? {
            return Ok(false);
        }
        pending.candidate = candidate;
        if pending.raster_reused
            && let Some(stats) = &mut self.statistics
        {
            stats.raster_reuses = stats.raster_reuses.saturating_add(1);
        }
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
        outcome: &sophia_shell_protocol::ContentCandidateOutcome,
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
        #[cfg(feature = "diagnostic-cpu-reference")]
        self.cadence_stamp(&mut pending.timing.presented_observed)?;
        pending.phase = PresentationPhase::RetireNeeded;
        #[cfg(feature = "experimental-frame-transactions")]
        if pending.frame.is_some() && pending.offer.is_some() {
            return Ok(true);
        }
        self.advance_retire_enqueue(pending)
    }

    fn advance_retire_enqueue(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        if let Some(&index) = pending.update.old.front() {
            let old = self.panels[pending.panel].resources[index];
            let expected = if pending.discard {
                ResourceState::Accepted
            } else {
                ResourceState::Resident
            };
            if old.state != expected || (!pending.discard && pending.update.next.contains(index)) {
                return Err("retirement does not exclude the exact prior resource".into());
            }
            let retiring: u64 = self
                .panels
                .iter()
                .flat_map(|p| &p.resources)
                .filter(|slot| slot.state == ResourceState::Retiring)
                .map(|slot| slot.bytes)
                .sum();
            if retiring + old.bytes > self.limits.max_retiring_bytes {
                return Ok(false);
            }
            let transaction = self.transaction()?;
            match self.enqueue_content(
                transaction,
                &ShellContentRecord::ResourceRetire(ContentResourceRetire {
                    grant: self.limits.grant,
                    resource: ContentResourceId {
                        id: old.id,
                        generation: old.generation,
                    },
                }),
            ) {
                Ok(()) => {}
                Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
                Err(error) => return Err(format!("resource retire failed: {error}")),
            }
            let slot = &mut self.panels[pending.panel].resources[index];
            slot.state = ResourceState::Retiring;
            slot.release_deadline = Some(Instant::now() + RESPONSE_TIMEOUT);
            pending.update.old.pop_front();
            // Only an enqueued record advances the retirement cursor.
            pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
            return Ok(true);
        }
        pending.phase = PresentationPhase::Presented;
        Ok(true)
    }

    fn finish_presentation(&mut self, pending: PendingPresentation) -> Result<(), String> {
        #[cfg(feature = "diagnostic-cpu-reference")]
        let cadence = (pending.panel, pending.candidate, pending.timing);
        let panel = &mut self.panels[pending.panel];
        let raster = pending.raster;
        let indicator_generation = pending.indicator_revision;
        println!(
            "lom_panel_candidate schema={} status=presented connection_epoch={} content_grant_epoch={} output={} candidate_generation={} presentation_epoch={} indicator_generation={} width={} height={} bytes={} checksum={} raster_source={}",
            if raster.checksum.is_some() { 1 } else { 2 },
            self.limits.grant.connection_epoch,
            self.limits.grant.content_grant_epoch,
            panel.output.output.id,
            pending.candidate,
            pending.presentation_epoch,
            indicator_generation,
            raster.width,
            raster.height,
            raster.bytes,
            raster
                .checksum
                .map_or_else(|| "unavailable".to_owned(), |value| format!("{value:016x}")),
            if pending.raster_reused {
                "reused"
            } else {
                "rendered"
            },
        );
        panel.current = pending.update.next;
        for placement in panel.current.placements() {
            let slot = &mut panel.resources[placement.slot];
            if !matches!(
                slot.state,
                ResourceState::Accepted | ResourceState::Resident
            ) {
                return Err("Presented lost an accepted raster resource".into());
            }
            slot.state = ResourceState::Resident;
        }
        if pending.update.tiled {
            println!(
                "lom_panel_regions schema=1 candidate_generation={} placements=3 uploads={} upload_bytes={} retained={}",
                pending.candidate,
                pending.uploads,
                pending.upload_bytes,
                3 - pending.uploads
            );
        }
        panel.presented = Some(PresentedPanel {
            model: pending.model,
            candidate_generation: pending.candidate,
            presentation_epoch: pending.presentation_epoch,
            targets: pending.targets,
            raster: pending.raster,
        });
        #[cfg(feature = "diagnostic-cpu-reference")]
        self.cadence_presented(cadence.0, cadence.1, cadence.2)?;
        #[cfg(feature = "diagnostic-cpu-reference")]
        self.sequence_presented(
            pending.panel,
            pending.candidate,
            pending.uploads,
            pending.upload_bytes,
            pending.raster_reused,
        )?;
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
        resources: RasterSet,
        candidate: u64,
        permit: ContentFramePermit,
        targets: &[ContentTargetLayout],
    ) -> Result<bool, String> {
        let scene = self.scene(panel_index, resources, candidate, permit.permit_id, targets)?;
        let begin = ShellContentRecord::CandidateBegin(scene.begin);
        let chunk = ShellContentRecord::CandidateChunk(scene.chunk);
        let end = ShellContentRecord::CandidateEnd(scene.end);
        let transaction = self.transaction()?;
        match self.enqueue_candidate(transaction, &[begin, chunk, end]) {
            Ok(()) => {}
            Err(sophia_shell_client::ShellClientError::QueueSaturated) => return Ok(false),
            Err(error) => return Err(format!("candidate outbox admission: {error}")),
        }
        Ok(true)
    }
}
