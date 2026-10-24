//! Diagnostic emission adds a five-second deadline only when explicitly enabled.
use crate::service::{ContentRenderer, ShellService};
use std::time::{Duration, Instant};

pub(super) struct Reporter {
    next: Option<Instant>,
}

impl Reporter {
    pub(super) fn new<R: ContentRenderer>(service: &mut ShellService<R>) -> Self {
        let enabled = std::env::var_os("SOPHIA_SHELL_WORK_STATS").is_some_and(|value| value == "1");
        service.set_work_statistics_enabled(enabled);
        Self {
            next: enabled.then(Instant::now),
        }
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.next
    }

    pub(super) fn emit<R: ContentRenderer>(
        &mut self,
        service: &ShellService<R>,
        final_record: bool,
    ) -> Result<(), String> {
        let Some(next) = self.next else {
            return Ok(());
        };
        if !final_record && Instant::now() < next {
            return Ok(());
        }
        let stats = service
            .work_statistics()
            .expect("reporter enabled statistics");
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: time is a valid output for this synchronous clock read.
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
            return Err("work statistics monotonic clock unavailable".into());
        }
        let usec = (time.tv_sec as u64).saturating_mul(1_000_000) + time.tv_nsec as u64 / 1000;
        println!(
            "sophia_shell_work_stats schema=1 client=lom monotonic_usec={usec} final={} service_turns={} waits={} wait_expirations={} renders_started={} renders_completed={} uploads={} upload_bytes={} raster_reuses={}",
            u8::from(final_record),
            stats.service_turns,
            stats.waits,
            stats.wait_expirations,
            stats.renders_started,
            stats.renders_completed,
            stats.uploads,
            stats.upload_bytes,
            stats.raster_reuses
        );
        self.next = Some(Instant::now() + Duration::from_secs(5));
        Ok(())
    }
}
