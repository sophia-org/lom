//! Ordered transport observations and exact presented input ownership.
use super::*;

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn observe(&mut self) -> Result<(), String> {
        self.custody.observe(&self.connection)?;
        self.connection
            .poll_io()
            .map_err(|error| format!("shell I/O: {error}"))?;
        self.custody.observe(&self.connection)?;
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((_, snapshot)) = self
                .connection
                .take_indicators()
                .map_err(|error| format!("indicator receive failed: {error}"))?
            else {
                break;
            };
            if snapshot.connection_epoch != self.connection.connection_epoch() {
                return Err("indicator snapshot belongs to a different connection".into());
            }
            self.latest_indicators = Some(snapshot.clone());
            if !self.live_model_inputs() {
                continue;
            }
            let workspaces = workspace_snapshot(&snapshot);
            for panel in &mut self.panels {
                let mut next = panel.model.clone();
                next.workspaces = workspaces.clone();
                next.epoch = workspaces.epoch;
                panel.dirty |= !crate::modules::same_panel_pixels(&panel.model, &next);
                panel.interaction_dirty |= panel.model.workspaces != next.workspaces;
                panel.model = next;
            }
        }
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((_, outcome)) = self
                .connection
                .take_indicator_activation_outcome()
                .map_err(|error| format!("indicator outcome receive failed: {error}"))?
            else {
                break;
            };
            self.lifecycle.finish_action(outcome.event_id);
            if !self.pending_activation_events.remove(&outcome.event_id) {
                continue;
            }
            if outcome.status != ShellIndicatorActivationStatus::Accepted {
                eprintln!(
                    "lom_workspace_activation schema=1 status=rejected event_id={} reason={}",
                    outcome.event_id, outcome.reason
                );
            }
        }
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((transaction, record)) = self
                .connection
                .take_content()
                .map_err(|error| format!("shell content receive failed: {error}"))?
            else {
                break;
            };
            self.observe_content(transaction, record)?;
        }
        #[cfg(feature = "experimental-frame-transactions")]
        if self.frame_mode() {
            self.observe_frame_journal()?;
        }
        Ok(())
    }

    pub(super) fn observe_content(
        &mut self,
        transaction: TransactionId,
        record: ShellContentRecord,
    ) -> Result<(), String> {
        let dispatch = self
            .lifecycle
            .dispatch(transaction, record)
            .map_err(|error| format!("ordered content lifecycle: {error:?}"))?;
        let record = dispatch.record;
        if let ShellContentRecord::Action(action) = record {
            if !self.live_model_inputs() {
                return Err("fixture sequence refuses content input".into());
            }
            if dispatch.action != Some(ContentActionDispatch::Cancelled) {
                self.handle_content_action(
                    action,
                    dispatch.action == Some(ContentActionDispatch::Eligible),
                )?;
            }
            return Ok(());
        }
        if let ShellContentRecord::ResourceReleased(released) = &record {
            self.release_resource(released)?;
            return Ok(());
        }
        if let ShellContentRecord::CandidateOutcome(outcome) = &record
            && outcome.kind == CANDIDATE_PRESENTED
        {
            self.install_presented_targets(outcome)?;
        }
        if let ShellContentRecord::OutputFacts(facts) = &record {
            if facts != &self.facts {
                return Err(
                    "content output facts changed; restart required for fresh allocations".into(),
                );
            }
            return Ok(());
        }
        if self.pending_content.len() == MAX_PENDING_CONTENT {
            return Err("shell content response queue saturated".into());
        }
        self.pending_content.push_back((transaction, record));
        Ok(())
    }
}
