//! Diagnostic CPU reference startup: one binary serves both Vello CPU arms.

use crate::{config::Renderer, render::RendererWorker};
use std::os::fd::BorrowedFd;
use std::sync::atomic::AtomicBool;

/// Exact source-selected startup record; not admission evidence.
pub(super) fn record(renderer: Renderer) -> Result<&'static str, String> {
    match renderer {
        Renderer::Cpu => Ok(
            "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used diagnostic=cpu-reference",
        ),
        Renderer::CpuCopyReference => Ok(
            "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-copy gpu=not-used diagnostic=cpu-reference",
        ),
        Renderer::Gpu => Err(super::serve::REFUSE_GPU.into()),
    }
}

/// Start the resolved arm; neither reads a GPU grant.
pub(super) fn start(
    renderer: Renderer,
    stop: &AtomicBool,
    wake: BorrowedFd<'_>,
) -> Result<RendererWorker, String> {
    let record = record(renderer)?;
    let worker = if renderer == Renderer::Cpu {
        RendererWorker::start_cpu_until_stopped(stop, wake)?
    } else {
        RendererWorker::start_cpu_copy_reference_until_stopped(stop, wake)?
    };
    println!("{record}");
    Ok(worker)
}
