//! Opt-in owner-thread counters; the CLI owns emission.

/// Cumulative work counts, excluding construction before statistics are enabled.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkStatistics {
    /// Calls to the bounded service turn.
    pub service_turns: u64,
    /// Actual readiness waits, excluding immediately runnable work.
    pub waits: u64,
    /// Waits that returned because their timeout expired.
    pub wait_expirations: u64,
    /// Successfully queued render jobs.
    pub renders_started: u64,
    /// Successfully received render results.
    pub renders_completed: u64,
    /// Successfully admitted image offers, not enqueue attempts.
    pub uploads: u64,
    /// Declared image bytes in those offers; not measured physical copies.
    pub upload_bytes: u64,
    /// Candidates successfully enqueued against an existing raster without a render.
    pub raster_reuses: u64,
}

impl<R: super::ContentRenderer> super::ShellService<R> {
    /// Enable diagnostic counters; enabling from disabled starts at zero.
    pub fn set_work_statistics_enabled(&mut self, enabled: bool) {
        if enabled {
            self.statistics.get_or_insert_with(Default::default);
        } else {
            self.statistics = None;
        }
    }

    /// Snapshot the counters on the service owner thread.
    pub fn work_statistics(&self) -> Option<WorkStatistics> {
        self.statistics
    }
}
