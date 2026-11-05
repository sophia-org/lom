//! Default-off whole-frame admission; existing owners retain images and input meaning.
use super::*;
use sophia_shell_client::{
    ClientContentCandidate, FrameStream, FrameStreamState, ShellClientError,
};
use sophia_shell_protocol::{
    ContentCandidate,
    shell_files::{ShellFrameCommit, ShellFrameRecord, ShellFrameStreamKey},
};

#[derive(Default)]
pub(in crate::service) struct FrameOutput {
    pub(in crate::service) stream: FrameStream,
    legacy: bool,
}

pub(super) struct PendingFrame {
    metadata: Option<ClientContentCandidate>,
    refused: bool,
}

impl<R: ContentRenderer> ShellService<R> {
    pub(in crate::service) fn frame_mode(&self) -> bool {
        self.connection.welcome().selected_revision == 13
    }

    pub(in crate::service) fn frame_key(
        &self,
        panel: usize,
        stream_id: u64,
    ) -> ShellFrameStreamKey {
        ShellFrameStreamKey {
            grant: self.limits.grant,
            output: self.panels[panel].output.output,
            allocation: self.panels[panel].allocation.allocation,
            facts_generation: self.facts.facts_generation,
            stream_id,
        }
    }

    fn eligible_frame(&self, pending: &PendingPresentation) -> bool {
        pending.uploads == 0
            && pending.update.jobs.len() <= 1
            && pending.update.old.len() <= 1
            && (!self.renderer.supports_regions() || pending.update.tiled)
            && pending
                .content
                .as_ref()
                .is_none_or(|c| c.pixels.byte_len() <= 4 * 1024 * 1024)
    }

    fn frame_ready(&mut self, index: usize) -> Result<bool, String> {
        match self.panels[index].frame.stream.state() {
            FrameStreamState::Unknown => Err("frame custody is unknown".into()),
            FrameStreamState::Opening | FrameStreamState::Closing => Ok(false),
            FrameStreamState::Open => Ok(self.panels[index].frame.stream.has_credit()),
            FrameStreamState::Closed => {
                let transaction = self.transaction()?;
                let key = self.frame_key(index, transaction.raw());
                let result = self.custody.reserve(1).and_then(|()| {
                    self.panels[index]
                        .frame
                        .stream
                        .open(&mut self.connection, transaction, key)
                });
                match result {
                    Ok(admission) => {
                        self.custody.record(admission);
                        self.progressed = true;
                        Ok(false)
                    }
                    Err(ShellClientError::QueueSaturated) => Ok(false),
                    Err(e) => Err(format!("frame Open: {e}")),
                }
            }
        }
    }

    fn drain_frame(&mut self, index: usize) -> Result<bool, String> {
        match self.panels[index].frame.stream.state() {
            FrameStreamState::Closed => Ok(true),
            FrameStreamState::Unknown => Err("frame drain lost custody".into()),
            FrameStreamState::Opening | FrameStreamState::Closing => Ok(false),
            FrameStreamState::Open => {
                let transaction = self.transaction()?;
                let result = self.custody.reserve(1).and_then(|()| {
                    self.panels[index]
                        .frame
                        .stream
                        .close(&mut self.connection, transaction)
                });
                match result {
                    Ok(admission) => {
                        self.custody.record(admission);
                        self.progressed = true;
                        Ok(false)
                    }
                    Err(ShellClientError::QueueSaturated) => Ok(false),
                    Err(e) => Err(format!("frame Close: {e}")),
                }
            }
        }
    }

    fn select_frame(&mut self, pending: &PendingPresentation) -> Result<Option<bool>, String> {
        if !self.frame_mode() {
            return Ok(None);
        }
        let index = pending.panel;
        let eligible = self.eligible_frame(pending);
        let frame = &mut self.panels[index].frame;
        // Establish the unchanged region plan before Open; an active unsupported update drains permanently.
        if !eligible && frame.stream.key().is_some() {
            frame.legacy = true;
        }
        if frame.legacy {
            return self
                .drain_frame(index)
                .map(|drained| if drained { None } else { Some(false) });
        }
        if !eligible {
            return Ok(None);
        }
        self.frame_ready(index).map(Some)
    }

    pub(super) fn frame_before_upload(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<Option<bool>, String> {
        match self.select_frame(pending)? {
            Some(true) => self.submit_frame(pending, true).map(Some),
            value => Ok(value),
        }
    }

    pub(super) fn frame_before_demand(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<Option<bool>, String> {
        match self.select_frame(pending)? {
            Some(true) => self.submit_frame(pending, false).map(Some),
            value => Ok(value),
        }
    }

    fn submit_frame(
        &mut self,
        pending: &mut PendingPresentation,
        copying: bool,
    ) -> Result<bool, String> {
        if !pending.targets_prepared {
            self.panels[pending.panel].target_generations.prepare(
                &pending.model,
                &mut pending.targets,
                self.limits.max_candidate_targets as usize,
            )?;
            pending.targets_prepared = true;
        }
        let index = pending.panel;
        let key = self.panels[index]
            .frame
            .stream
            .key()
            .ok_or("frame has no binding")?;
        let generation = self.next_candidate_generation;
        let scene = self.scene(
            index,
            pending.update.next,
            generation,
            key.stream_id,
            &pending.targets,
        )?;
        let mut candidate = ContentCandidate {
            grant: scene.begin.grant,
            output: scene.begin.output,
            candidate_generation: generation,
            facts_generation: scene.begin.facts_generation,
            pacing_permit: key.stream_id,
            interaction_generation: scene.begin.interaction_generation,
            surfaces: scene.chunk.surfaces,
            placements: scene.chunk.placements,
            targets: scene.chunk.targets,
        };
        if copying {
            let target = pending.fill.ok_or("frame copy lost its fill snapshot")?;
            for (placement, slot) in candidate
                .placements
                .iter_mut()
                .zip(pending.update.next.placements())
            {
                if slot.slot == pending.slot_index {
                    placement.resource = ContentResourceId {
                        id: target.upload.resource,
                        generation: target.upload.resource_generation,
                    };
                }
            }
        }
        let retire = if let Some(&slot) = pending.update.old.front() {
            let old = self.panels[index].resources[slot];
            if old.state != ResourceState::Resident || pending.update.next.contains(slot) {
                return Err("frame predecessor is not the excluded resident resource".into());
            }
            let retiring: u64 = self
                .panels
                .iter()
                .flat_map(|p| &p.resources)
                .filter(|s| s.state == ResourceState::Retiring)
                .map(|s| s.bytes)
                .sum();
            if retiring + old.bytes > self.limits.max_retiring_bytes {
                return Ok(false);
            }
            Some(sophia_shell_protocol::shell_files::ShellFrameRetire {
                transaction: self.transaction()?,
                resource: ContentResourceId {
                    id: old.id,
                    generation: old.generation,
                },
            })
        } else {
            None
        };
        let transaction = self.transaction()?;
        let mut record = ShellFrameRecord::Commit(ShellFrameCommit {
            transaction,
            candidate,
            image: None,
            retire,
        });
        let header = sophia_shell_protocol::shell_files::ShellFileHeader {
            kind: record.kind(),
            connection_epoch: self.limits.grant.connection_epoch,
            submission_id: 1,
            sequence: 0,
        };
        let bytes =
            sophia_shell_protocol::shell_files::shell_frame_record_bytes(header, &record, 13);
        if bytes.is_err()
            || bytes.is_ok_and(|n| {
                n + usize::from(copying) * 152 > self.limits.max_candidate_bytes.min(8192) as usize
            })
        {
            self.panels[index].frame.legacy = true;
            return Ok(true);
        }
        if let Err(e) = self.custody.reserve(1) {
            return match e {
                ShellClientError::QueueSaturated => Ok(false),
                e => Err(format!("frame custody: {e}")),
            };
        }
        let result = if copying {
            let target = pending.fill.ok_or("frame copy lost its snapshot")?;
            let content = pending.content.as_ref().ok_or("frame copy lost pixels")?;
            let RenderImage::RingFill {
                identity,
                width,
                height,
                region,
                ..
            } = &content.pixels
            else {
                return Err("frame copy requires an owned loan".into());
            };
            let slot = self.panels[index].resources[pending.slot_index];
            if !self.fill_unchanged(index, &target, *identity, *width, *height)
                || *region != target.region
                || slot.state != ResourceState::Staging
                || slot.bytes != content.pixels.byte_len() as u64
            {
                return Err("frame copy became stale before admission".into());
            }
            let RenderImage::RingFill { fill, .. } = &mut pending
                .content
                .as_mut()
                .ok_or("frame copy lost pixels")?
                .pixels
            else {
                return Err("frame copy lost its owned loan".into());
            };
            self.panels[index]
                .frame
                .stream
                .submit_ring_fill(
                    &mut self.connection,
                    &mut record,
                    fill,
                    target.upload,
                    Instant::now(),
                )
                .map(|(a, o)| (a, Some(o)))
        } else {
            self.panels[index]
                .frame
                .stream
                .submit(&mut self.connection, &record)
                .map(|a| (a, None))
        };
        let (admission, offer) = match result {
            Ok(value) => value,
            Err(
                ShellClientError::QueueSaturated
                | ShellClientError::Image(
                    sophia_shell_client::ImageAdapterError::Again
                    | sophia_shell_client::ImageAdapterError::Busy,
                ),
            ) => return Ok(false),
            Err(e) => return Err(format!("frame admission: {e}")),
        };
        self.custody.record(admission);
        if let Some(offer) = offer {
            let upload = pending
                .fill
                .ok_or("admitted frame lost its source snapshot")?
                .upload;
            let byte_len = pending
                .content
                .as_ref()
                .ok_or("admitted frame lost its pixels")?
                .pixels
                .byte_len();
            self.record_image_offer(
                pending,
                ContentResourceId {
                    id: upload.resource,
                    generation: upload.resource_generation,
                },
                offer,
                byte_len,
            )?;
        }
        let ShellFrameRecord::Commit(frame) = record else {
            return Err("frame admission changed the record kind".into());
        };
        pending.frame = Some(PendingFrame {
            metadata: Some(ClientContentCandidate {
                transaction,
                begin: scene.begin,
                surfaces: frame.candidate.surfaces,
                targets: frame.candidate.targets,
            }),
            refused: false,
        });
        pending.candidate = generation;
        pending.phase = PresentationPhase::CandidateSubmitted;
        pending.deadline = Instant::now() + RESPONSE_TIMEOUT;
        self.next_candidate_generation = generation
            .checked_add(1)
            .ok_or("candidate generation exhausted")?;
        if pending.raster_reused
            && let Some(stats) = &mut self.statistics
        {
            stats.raster_reuses = stats.raster_reuses.saturating_add(1);
        }
        Ok(true)
    }

    pub(in crate::service) fn observe_frame_journal(&mut self) -> Result<(), String> {
        use sophia_shell_client::JournalRecord;
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some(event) = self.connection.journal_event() else {
                break;
            };
            match event.record {
                JournalRecord::Content(tx, value) => {
                    let value = value.clone();
                    self.observe_content(tx, value)?;
                }
                JournalRecord::Frame(value) => {
                    let value = value.clone();
                    self.observe_frame(value)?;
                }
                JournalRecord::ObjectPublished(_) => {} // Snapshot fetching stays SDK-owned.
                JournalRecord::IndicatorOutcome(_, value) => {
                    let value = *value;
                    self.lifecycle.finish_action(value.event_id);
                    if self.pending_activation_events.remove(&value.event_id)
                        && value.status != ShellIndicatorActivationStatus::Accepted
                    {
                        eprintln!(
                            "lom_workspace_activation schema=1 status=rejected event_id={} reason={}",
                            value.event_id, value.reason
                        );
                    }
                }
                _ => return Err("unexpected event in the SFT panel journal".into()),
            }
            self.connection
                .consume_journal_event()
                .map_err(|e| format!("SFT journal consume: {e}"))?;
            self.progressed = true;
            self.connection
                .poll_io()
                .map_err(|e| format!("SFT journal progress: {e}"))?;
        }
        Ok(())
    }

    fn observe_frame(&mut self, record: ShellFrameRecord) -> Result<(), String> {
        let output = match &record {
            ShellFrameRecord::Opened { key, .. } | ShellFrameRecord::Closed { key, .. } => {
                key.output
            }
            ShellFrameRecord::Refused { output, .. } => *output,
            ShellFrameRecord::Outcome { outcome, .. } => outcome.output,
            _ => return Err("request arrived in the frame journal".into()),
        };
        let index = self
            .panels
            .iter()
            .position(|p| p.output.output == output)
            .ok_or("frame names no panel")?;
        self.panels[index]
            .frame
            .stream
            .observe(&record)
            .map_err(|e| format!("frame observation: {e}"))?;
        match record {
            ShellFrameRecord::Outcome {
                transaction,
                outcome,
                ..
            } => {
                let pending = self
                    .presentations
                    .iter_mut()
                    .find(|p| p.panel == index && p.candidate == outcome.candidate_generation)
                    .ok_or("frame outcome has no pending scene")?;
                let frame = pending
                    .frame
                    .as_mut()
                    .ok_or("frame outcome names a legacy scene")?;
                if let Some(metadata) = frame.metadata.take() {
                    // First admitted outcome precedes every action for this scene in this FIFO.
                    self.lifecycle
                        .register(metadata)
                        .map_err(|e| format!("frame input lifecycle: {e:?}"))?;
                }
                if outcome.kind == CANDIDATE_PRESENTED
                    && let Some(slot) = pending.update.old.pop_front()
                {
                    let old = &mut self.panels[index].resources[slot];
                    if old.state != ResourceState::Resident || pending.update.next.contains(slot) {
                        return Err("frame retirement lost the exact predecessor".into());
                    }
                    old.state = ResourceState::Retiring;
                    old.release_deadline = Some(Instant::now() + RESPONSE_TIMEOUT);
                }
                self.observe_content(transaction, ShellContentRecord::CandidateOutcome(outcome))?;
            }
            ShellFrameRecord::Refused {
                candidate_generation,
                request_kind,
                ..
            } => {
                use sophia_shell_protocol::shell_files::ShellFileKind;
                match request_kind {
                    ShellFileKind::FrameCommit => {
                        let pending = self
                            .presentations
                            .iter_mut()
                            .find(|p| p.panel == index && p.candidate == candidate_generation)
                            .ok_or("frame refusal has no pending scene")?;
                        pending
                            .frame
                            .as_mut()
                            .ok_or("frame refusal names legacy scene")?
                            .refused = true;
                        self.panels[index].frame.legacy = true;
                    }
                    ShellFileKind::FrameStreamOpen => self.panels[index].frame.legacy = true,
                    ShellFileKind::FrameStreamClose => {} // A Closed/terminal already in flight still drains.
                    _ => return Err("unexpected frame refusal".into()),
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn advance_frame_progress(
        &mut self,
        pending: &mut PendingPresentation,
    ) -> Result<bool, String> {
        if pending.frame.as_ref().is_some_and(|f| f.refused) {
            if let Some(offer) = pending.offer {
                let info = self
                    .connection
                    .image_ring_offer(offer)
                    .ok_or("refused frame lost source custody")?;
                match info.state {
                    sophia_shell_client::RingOfferState::Pending => return Ok(false),
                    sophia_shell_client::RingOfferState::Refused => {}
                    _ => return Err("unadmitted frame source did not refuse exactly".into()),
                }
                self.connection
                    .image_forget(offer)
                    .map_err(|e| format!("refused frame source: {e:?}"))?;
                let slot = &mut self.panels[pending.panel].resources[pending.slot_index];
                if slot.state != ResourceState::Staging {
                    return Err("refused frame changed a live resource".into());
                }
                slot.generation = slot
                    .generation
                    .checked_add(1)
                    .ok_or("resource generation exhausted")?;
                slot.state = ResourceState::Free;
                slot.bytes = 0;
            }
            self.free_reserved(pending.panel, &pending.update);
            pending.discard = true;
            pending.phase = PresentationPhase::Presented;
            return Ok(true);
        }
        let source_progress = if pending.offer.is_some() {
            self.advance_image_offered(pending)?
        } else {
            false
        };
        if pending.phase == PresentationPhase::RetireNeeded {
            return if pending.offer.is_none() {
                self.advance_retire_enqueue(pending)
            } else {
                Ok(source_progress)
            };
        }
        self.advance_candidate(pending)
            .map(|p| p || source_progress)
    }
}
