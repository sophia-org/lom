//! Readiness and real obligations determine when the owner next runs.
use super::*;
use std::os::fd::{AsFd, BorrowedFd};
use std::time::{SystemTime, UNIX_EPOCH};

impl<R: ContentRenderer> ShellService<R> {
    /// Maximum wait for an actual deadline; MAX means no timer is needed.
    pub fn idle_wait(&self) -> Duration {
        let now = Instant::now();
        let mut timeout = self
            .connection
            .wake_deadline()
            .into_iter()
            .chain(self.renderer.completion_deadline())
            .chain(self.presentations.iter().map(|pending| pending.deadline))
            .chain(
                self.panels
                    .iter()
                    .flat_map(|panel| &panel.resources)
                    .filter_map(|slot| slot.release_deadline),
            )
            .map(|deadline| deadline.saturating_duration_since(now))
            .min()
            .unwrap_or(Duration::MAX);
        if self.rendering_panel.is_some() && self.renderer.completion_fd().is_none() {
            timeout = timeout.min(IDLE_POLL);
        }
        if self.panels.iter().any(|panel| {
            panel
                .model
                .config
                .modules
                .iter()
                .any(|module| module.kind == ModuleKind::Clock)
        }) {
            let wall = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            timeout = timeout.min(if wall.as_secs() != self.last_second {
                Duration::ZERO
            } else {
                Duration::from_secs(1) - Duration::from_nanos(wall.subsec_nanos().into())
            });
        }
        timeout
    }

    /// Wait only after draining runnable work; readiness does not promise progress.
    pub fn wait_for_work(&mut self) -> Result<(), String> {
        self.wait_for_work_until(None)
    }

    pub(crate) fn wait_for_work_until(&mut self, deadline: Option<Instant>) -> Result<(), String> {
        if self.progressed {
            return Ok(());
        }
        self.wait_until(deadline)
    }

    pub(super) fn wait_until(&mut self, deadline: Option<Instant>) -> Result<(), String> {
        self.check_stop()?;
        let timeout = deadline.map_or(self.idle_wait(), |at| {
            self.idle_wait()
                .min(at.saturating_duration_since(Instant::now()))
        });
        let waited = connection_wait(
            &self.connection,
            self.stop_wake.as_ref().map(AsFd::as_fd),
            self.renderer.completion_fd(),
            (timeout != Duration::MAX).then_some(timeout),
        )?;
        if let Some(ready) = waited
            && let Some(stats) = &mut self.statistics
        {
            stats.waits = stats.waits.saturating_add(1);
            stats.wait_expirations = stats.wait_expirations.saturating_add(u64::from(!ready));
        }
        Ok(())
    }
}

pub(super) fn connection_wait(
    connection: &ShellConnection,
    stop: Option<BorrowedFd<'_>>,
    worker: Option<BorrowedFd<'_>>,
    timeout: Option<Duration>,
) -> Result<Option<bool>, String> {
    let interests = connection
        .io_interest()
        .map_err(|error| format!("shell interests: {error}"))?;
    if interests.immediate {
        return Ok(None);
    }
    let timeout = connection
        .wake_deadline()
        .map(|at| at.saturating_duration_since(Instant::now()))
        .into_iter()
        .chain(timeout)
        .min();
    // Four descriptors, with no allocation or idle POLLOUT subscription.
    let mut fds = [libc::pollfd {
        fd: -1,
        events: 0,
        revents: 0,
    }; 4];
    fds[0] = crate::wake::interest(
        connection.as_fd(),
        if interests.readable { libc::POLLIN } else { 0 }
            | if interests.writable { libc::POLLOUT } else { 0 },
    );
    if let Some(image) = connection
        .image_readiness()
        .map_err(|error| format!("image interests: {error}"))?
        && let Some(fd) = image.image_fd
    {
        fds[1] = crate::wake::interest(fd, image.image_events.0 as i16);
    }
    if let Some(fd) = worker {
        fds[2] = crate::wake::interest(fd, libc::POLLIN);
    }
    if let Some(fd) = stop {
        fds[3] = crate::wake::interest(fd, libc::POLLIN);
    }
    crate::wake::wait(&mut fds, timeout).map(Some)
}
