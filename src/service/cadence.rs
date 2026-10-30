//! Diagnostic-only `lom-cpu-cadence` driver. After each exact Presented of a
//! rendered candidate the one static panel is redirtied, until a fixed quota;
//! pacing stays with the unchanged release and `render_ready` flow. Times are
//! owner-thread CLOCK_MONOTONIC nanoseconds: neither host receipt nor pure
//! raster time.

use super::*;

/// Rendered candidates one cadence run presents; deliberately not configurable.
pub(crate) const QUOTA: u64 = 2120;

/// One candidate's owner-thread observations; 0 means not yet observed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Timing {
    /// The first eligible render attempt: after `render_ready` and the model
    /// clone, before the fill acquire or job submit; kept through Busy retries.
    pub(super) eligible_render_attempt: u64,
    /// The ring upload or fill commit returned its queued Offer.
    pub(super) ring_commit_and_offer_queued: u64,
    /// The first validated exact Presented outcome.
    pub(super) presented_observed: u64,
}

pub(super) struct Cadence {
    quota: u64,
    presented: u64,
    /// The next job's first eligible attempt, until a presentation takes it.
    attempt: u64,
    last: Timing,
    fill_deferrals: u64,
    upload_deferrals: u64,
}

fn monotonic_ns() -> Result<u64, String> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out-pointer for the duration of the call.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } != 0 {
        return Err("CLOCK_MONOTONIC unavailable".into());
    }
    (ts.tv_sec as u64)
        .checked_mul(1_000_000_000)
        .and_then(|ns| ns.checked_add(ts.tv_nsec as u64))
        .ok_or_else(|| "CLOCK_MONOTONIC overflow".into())
}

impl<R: ContentRenderer> ShellService<R> {
    /// Start the cadence before the first service turn. It owns the work
    /// counters from zero, so counters already enabled are refused, never reset.
    pub(crate) fn enable_diagnostic_cadence(&mut self) -> Result<(), String> {
        self.enable_cadence(QUOTA)
    }

    fn enable_cadence(&mut self, quota: u64) -> Result<(), String> {
        if self.panels.len() != 1 {
            return Err("lom-cpu-cadence requires exactly one output panel".into());
        }
        if self.statistics.is_some() || self.cadence.is_some() {
            return Err("lom-cpu-cadence refuses work statistics already enabled".into());
        }
        if self.rendering_panel.is_some()
            || !self.presentations.is_empty()
            || self.panels[0].presented.is_some()
        {
            return Err("lom-cpu-cadence must start before the first render".into());
        }
        self.statistics = Some(WorkStatistics::default());
        self.cadence = Some(Cadence {
            quota,
            presented: 0,
            attempt: 0,
            last: Timing::default(),
            fill_deferrals: 0,
            upload_deferrals: 0,
        });
        Ok(())
    }

    pub(super) fn cadence_attempt(&mut self) -> Result<(), String> {
        if let Some(cadence) = &mut self.cadence
            && cadence.attempt == 0
        {
            cadence.attempt = monotonic_ns()?;
        }
        Ok(())
    }

    /// Count a Busy direct-fill acquire or ring upload; the attempt is kept.
    pub(super) fn cadence_deferred(&mut self, fill: bool) {
        if let Some(cadence) = &mut self.cadence {
            let count = if fill {
                &mut cadence.fill_deferrals
            } else {
                &mut cadence.upload_deferrals
            };
            *count = count.saturating_add(1);
        }
    }

    /// The timing a new rendered presentation carries from its attempt.
    pub(super) fn cadence_job(&mut self) -> Timing {
        Timing {
            eligible_render_attempt: self
                .cadence
                .as_mut()
                .map_or(0, |cadence| std::mem::take(&mut cadence.attempt)),
            ..Timing::default()
        }
    }

    pub(super) fn cadence_stamp(&self, at: &mut u64) -> Result<(), String> {
        if self.cadence.is_some() && *at == 0 {
            *at = monotonic_ns()?;
        }
        Ok(())
    }

    /// After `lom_panel_candidate`: refuse any extra or reused work, emit the
    /// sample, then redirty while the quota is unmet.
    pub(super) fn cadence_presented(
        &mut self,
        panel: usize,
        candidate: u64,
        timing: Timing,
    ) -> Result<(), String> {
        let Some(cadence) = &mut self.cadence else {
            return Ok(());
        };
        let stats = self.statistics.ok_or("lom-cpu-cadence lost its counters")?;
        if self.panels.len() != 1 || cadence.presented >= cadence.quota {
            return Err("lom-cpu-cadence refused: a candidate beyond its one-panel quota".into());
        }
        let sequence = cadence.presented + 1;
        if [
            stats.renders_started,
            stats.renders_completed,
            stats.uploads,
        ] != [sequence; 3]
            || stats.raster_reuses != 0
        {
            return Err(format!(
                "lom-cpu-cadence refused: candidate {sequence} shows extra or reused work"
            ));
        }
        let Timing {
            eligible_render_attempt: t0,
            ring_commit_and_offer_queued: t1,
            presented_observed: t2,
        } = timing;
        if t0 == 0 || t0 > t1 || t1 > t2 || t0 <= cadence.last.presented_observed {
            return Err(format!(
                "lom-cpu-cadence refused: candidate {sequence} timing is incomplete or out of order"
            ));
        }
        cadence.presented = sequence;
        cadence.last = timing;
        println!(
            "lom_cadence_sample schema=1 sequence={sequence} candidate_generation={candidate} eligible_render_attempt_ns={t0} ring_commit_and_offer_queued_ns={t1} presented_observed_ns={t2} renders_started={} renders_completed={} uploads={} raster_reuses={} fill_deferrals={} upload_deferrals={}",
            stats.renders_started,
            stats.renders_completed,
            stats.uploads,
            stats.raster_reuses,
            cadence.fill_deferrals,
            cadence.upload_deferrals,
        );
        if sequence < cadence.quota {
            self.panels[panel].dirty = true;
        }
        Ok(())
    }

    /// The shutdown record, so work started after the last sample is visible.
    pub(crate) fn cadence_final(&self) -> Option<String> {
        let cadence = self.cadence.as_ref()?;
        let stats = self.statistics.unwrap_or_default();
        let complete = cadence.presented == cadence.quota
            && [
                stats.renders_started,
                stats.renders_completed,
                stats.uploads,
            ] == [cadence.quota; 3]
            && stats.raster_reuses == 0
            && self.rendering_panel.is_none()
            && self.presentations.is_empty()
            && cadence.attempt == 0;
        Some(format!(
            "lom_cadence_final schema=1 status={} quota={} presented={} renders_started={} renders_completed={} uploads={} raster_reuses={} fill_deferrals={} upload_deferrals={} rendering_in_flight={} pending_presentations={}",
            if complete { "complete" } else { "refused" },
            cadence.quota,
            cadence.presented,
            stats.renders_started,
            stats.renders_completed,
            stats.uploads,
            stats.raster_reuses,
            cadence.fill_deferrals,
            cadence.upload_deferrals,
            u8::from(self.rendering_panel.is_some()),
            self.presentations.len(),
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/support/service_cadence.rs"]
mod tests;
