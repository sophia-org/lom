//! Opt-in cumulative CPU and wall time of DMA-BUF export stages. Off until the
//! owner enables it; the owner reads the totals, so nothing here is printed.
//! Thread CPU is `CLOCK_THREAD_CPUTIME_ID`: it includes kernel time spent in
//! this thread's system calls and driver ioctls, and excludes other threads.
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Cumulative totals of one completed export stage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StageTiming {
    /// Completed occurrences of the stage; a failed stage is not counted.
    pub count: u64,
    /// Calling-thread CPU time, user plus kernel, in nanoseconds.
    pub thread_cpu_ns: u64,
    /// Monotonic wall time, including any GPU wait, in nanoseconds.
    pub wall_ns: u64,
}

/// Cumulative DMA-BUF export timing since timing was last enabled.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DmaTiming {
    /// Admission, retention slot, exportable image, dedicated memory and its
    /// wgpu registration.
    pub setup: StageTiming,
    /// Vello render, export blit, submission and the bounded wait for the draw.
    pub draw: StageTiming,
    /// Foreign queue-family release: recording, submission and its fence wait.
    pub release: StageTiming,
    /// DMA-BUF descriptor export, size query and layout check.
    pub export: StageTiming,
    /// Dropping an exported frame's wgpu texture only. Closing its descriptor
    /// and releasing its retention slot follow and are excluded. wgpu may
    /// defer Vulkan destruction to a later device poll, which then counts in
    /// that export's draw stage.
    pub destroy: StageTiming,
}

#[derive(Clone, Copy)]
pub(super) enum Stage {
    Setup,
    Draw,
    Release,
    Export,
    Destroy,
}

#[derive(Default)]
struct Cell {
    count: AtomicU64,
    thread_cpu_ns: AtomicU64,
    wall_ns: AtomicU64,
}

impl Cell {
    fn totals(&self) -> StageTiming {
        StageTiming {
            count: self.count.load(Ordering::Relaxed),
            thread_cpu_ns: self.thread_cpu_ns.load(Ordering::Relaxed),
            wall_ns: self.wall_ns.load(Ordering::Relaxed),
        }
    }
}

/// Shared by the renderer and its exported frames; disabled is `None`.
#[derive(Clone, Default)]
pub(super) struct Timer(Option<Arc<[Cell; 5]>>);

/// The start of one stage, taken and recorded on the same thread.
pub(super) struct Mark {
    wall: Instant,
    thread_cpu_ns: u64,
}

impl Timer {
    /// Enabling an enabled timer keeps its totals; enabling again after
    /// disabling starts from zero.
    pub(super) fn set_enabled(&mut self, enabled: bool) {
        match (enabled, self.0.is_some()) {
            (true, false) => self.0 = Some(Arc::default()),
            (false, true) => self.0 = None,
            _ => {}
        }
    }

    pub(super) fn start(&self) -> Option<Mark> {
        self.0.as_ref().map(|_| Mark {
            wall: Instant::now(),
            thread_cpu_ns: thread_cpu_ns(),
        })
    }

    pub(super) fn record(&self, stage: Stage, mark: Option<Mark>) {
        let (Some(cells), Some(mark)) = (&self.0, mark) else {
            return;
        };
        let cell = &cells[stage as usize];
        let wall = u64::try_from(mark.wall.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let cpu = thread_cpu_ns().saturating_sub(mark.thread_cpu_ns);
        cell.count.fetch_add(1, Ordering::Relaxed);
        cell.thread_cpu_ns.fetch_add(cpu, Ordering::Relaxed);
        cell.wall_ns.fetch_add(wall, Ordering::Relaxed);
    }

    pub(super) fn totals(&self) -> Option<DmaTiming> {
        self.0.as_ref().map(|cells| DmaTiming {
            setup: cells[Stage::Setup as usize].totals(),
            draw: cells[Stage::Draw as usize].totals(),
            release: cells[Stage::Release as usize].totals(),
            export: cells[Stage::Export as usize].totals(),
            destroy: cells[Stage::Destroy as usize].totals(),
        })
    }
}

fn thread_cpu_ns() -> u64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    u64::try_from(now.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::try_from(now.tv_nsec).unwrap_or(0))
}
