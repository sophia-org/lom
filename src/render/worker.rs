//! One bounded render worker keeps the protocol owner responsive during rendering.

use super::{GpuAdmissionEvidence, GpuGrant, GpuPreview};
use crate::model::Model;
use crate::service::{ContentRenderer, RenderIdentity, RenderImage, RenderedContent};
use crate::ui::PreviewDriver;
use imaging_vello_cpu::VelloCpuRenderer;
use sophia_shell_client::OwnedRingSlotFill;
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const GPU_JOB_TIMEOUT: Duration = Duration::from_secs(2);

struct RenderJob {
    identity: RenderIdentity,
    /// The owner's ring loan for CPU direct fill; never present for GPU jobs.
    fill: Option<OwnedRingSlotFill>,
    model: Model,
    width: u32,
    height: u32,
    scale: f64,
}

type HostKey = (u64, u64, u64, u64, u64, u64, u64);

#[derive(Default)]
struct RetainedHosts {
    hosts: std::collections::BTreeMap<HostKey, (PreviewDriver, u32, u32, u64)>,
}

impl RetainedHosts {
    fn scene(
        &mut self,
        job: RenderJob,
    ) -> Result<(crate::ui::PreviewScene, Vec<crate::ui::ContentTargetLayout>), String> {
        let identity = job.identity;
        let key = (
            identity.grant.connection_epoch,
            identity.grant.content_grant_epoch,
            identity.output.id,
            identity.output.generation,
            identity.allocation.id,
            identity.allocation.generation,
            identity.scale_generation,
        );
        // A replacement allocation/grant cannot inherit another host's widget
        // state. Unaffected outputs keep their existing Xilem/Masonry roots.
        self.hosts
            .retain(|old, _| (old.0, old.1) == (key.0, key.1) && (old.2 != key.2 || *old == key));
        if let Some((host, width, height, scale)) = self.hosts.get_mut(&key) {
            if (*width, *height, *scale) != (job.width, job.height, job.scale.to_bits()) {
                return Err("allocation geometry changed without a new generation".into());
            }
            host.reconcile_model(job.model);
            return Ok(host.scene_and_targets());
        }
        if self.hosts.len() >= 16 {
            return Err("retained UI host limit exhausted".into());
        }
        let mut host = PreviewDriver::new(job.model, job.width, job.height, job.scale, false)?;
        let scene = host.scene_and_targets();
        self.hosts
            .insert(key, (host, job.width, job.height, job.scale.to_bits()));
        Ok(scene)
    }
}

fn label(cpu: bool) -> &'static str {
    if cpu { "CPU" } else { "GPU" }
}

fn quarantined(cpu: bool) -> String {
    format!(
        "{} worker deadline expired; in-flight resources remain quarantined",
        label(cpu)
    )
}

#[expect(
    clippy::large_enum_variant,
    reason = "one per worker thread; boxing would add an indirection to the GPU path"
)]
enum Backend {
    Gpu(GpuPreview, bool),
    Cpu(VelloCpuRenderer),
}

/// A failed job drops its loan here, faulting the ring before the completion
/// is sent and the owner woken; nothing is published or reused.
fn render(
    backend: &mut Backend,
    hosts: &mut RetainedHosts,
    mut job: RenderJob,
) -> Result<RenderedContent, String> {
    let fill = job.fill.take();
    let identity = job.identity;
    let (mut scene, targets) = hosts.scene(job)?;
    let pixels = match (backend, fill) {
        (Backend::Gpu(renderer, true), None) => RenderImage::Dma(renderer.dma_content(&mut scene)?),
        (Backend::Gpu(renderer, false), None) => {
            RenderImage::Cpu(renderer.readback_content(&mut scene)?)
        }
        (Backend::Cpu(renderer), Some(mut fill)) => {
            let pixels = fill
                .pixels_mut()
                .map_err(|error| format!("owned ring fill: {error:?}"))?;
            super::cpu::render_into(renderer, &mut scene, pixels)?;
            let checksum = crate::service::pixel_checksum(pixels);
            RenderImage::RingFill {
                fill,
                identity,
                width: scene.width,
                height: scene.height,
                checksum,
            }
        }
        _ => return Err("render job does not match the worker backend".into()),
    };
    Ok(RenderedContent { pixels, targets })
}

/// Capacity-one renderer executor. The worker alone owns Vello and wgpu, or
/// the explicitly selected Vello CPU renderer; neither falls back to the other.
pub struct RendererWorker {
    wake: Arc<crate::wake::Wake>,
    dma: bool,
    cpu: bool,
    requests: SyncSender<RenderJob>,
    completions: Receiver<Result<RenderedContent, String>>,
    submitted_at: Option<Instant>,
    timeout: Duration,
}

impl RendererWorker {
    /// Validate the admitted adapter and initialize the worker before serving.
    pub fn start(grant: GpuGrant) -> Result<(Self, GpuAdmissionEvidence), String> {
        Self::start_source(grant, false, None)
    }

    /// Explicit DMA capture worker; no readback fallback on refusal.
    pub fn start_dma(grant: GpuGrant) -> Result<(Self, GpuAdmissionEvidence), String> {
        Self::start_source(grant, true, None)
    }

    pub(crate) fn start_until_stopped(
        grant: GpuGrant,
        dma: bool,
        stop: &AtomicBool,
        fd: BorrowedFd<'_>,
    ) -> Result<(Self, GpuAdmissionEvidence), String> {
        Self::start_source(grant, dma, Some((stop, fd)))
    }

    /// Explicit Vello CPU worker filling owner-acquired ring loans. It reads
    /// no GPU grant and is never selected as a fallback.
    pub fn start_cpu() -> Result<Self, String> {
        Self::start_backend(false, true, None, || {
            Ok((Backend::Cpu(VelloCpuRenderer::new(1, 1)), None))
        })
        .map(|(worker, _)| worker)
    }

    pub(crate) fn start_cpu_until_stopped(
        stop: &AtomicBool,
        fd: BorrowedFd<'_>,
    ) -> Result<Self, String> {
        Self::start_backend(false, true, Some((stop, fd)), || {
            Ok((Backend::Cpu(VelloCpuRenderer::new(1, 1)), None))
        })
        .map(|(worker, _)| worker)
    }

    fn start_source(
        grant: GpuGrant,
        dma: bool,
        stop: Option<(&AtomicBool, BorrowedFd<'_>)>,
    ) -> Result<(Self, GpuAdmissionEvidence), String> {
        let (worker, evidence) = Self::start_backend(dma, false, stop, move || {
            let renderer = GpuPreview::new(&grant)?;
            let evidence = renderer.evidence(&grant)?;
            Ok((Backend::Gpu(renderer, dma), Some(evidence)))
        })?;
        Ok((
            worker,
            evidence.ok_or("GPU worker returned no admission evidence")?,
        ))
    }

    fn start_backend(
        dma: bool,
        cpu: bool,
        stop: Option<(&AtomicBool, BorrowedFd<'_>)>,
        backend: impl FnOnce() -> Result<(Backend, Option<GpuAdmissionEvidence>), String>
        + Send
        + 'static,
    ) -> Result<(Self, Option<GpuAdmissionEvidence>), String> {
        let (requests, incoming) = mpsc::sync_channel::<RenderJob>(1);
        let (completed, completions) = mpsc::sync_channel(1);
        let (ready, readiness) = mpsc::sync_channel(1);
        let wake =
            Arc::new(crate::wake::Wake::new().map_err(|error| format!("worker wake: {error}"))?);
        let completed = crate::wake::Sender::new(completed, wake.clone());
        let ready = crate::wake::Sender::new(ready, wake.clone());
        std::thread::Builder::new()
            .name(if cpu { "lom-cpu" } else { "lom-gpu" }.into())
            .spawn(move || {
                let mut backend = match backend() {
                    Ok((backend, evidence)) => {
                        if ready.send(Ok(evidence)).is_err() {
                            return;
                        }
                        backend
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let mut hosts = RetainedHosts::default();
                while let Ok(job) = incoming.recv() {
                    // The result, including any loan, is published before the wake.
                    let result = render(&mut backend, &mut hosts, job);
                    if completed.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("start {} worker: {error}", label(cpu)))?;
        let deadline = Instant::now() + GPU_JOB_TIMEOUT;
        let evidence = loop {
            wake.clear()
                .map_err(|error| format!("worker startup wake: {error}"))?;
            if stop.is_some_and(|(flag, _)| flag.load(Ordering::Relaxed)) {
                return Err("shell stopping".into());
            }
            match readiness.try_recv() {
                Ok(result) => break result?,
                Err(TryRecvError::Disconnected) => {
                    return Err(format!("{} worker stopped during startup", label(cpu)));
                }
                Err(TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err(format!("{} worker startup timed out", label(cpu)));
            }
            let mut fds = vec![crate::wake::interest(wake.as_fd(), libc::POLLIN)];
            if let Some((_, fd)) = stop {
                fds.push(crate::wake::interest(fd, libc::POLLIN));
            }
            crate::wake::wait(
                &mut fds,
                Some(deadline.saturating_duration_since(Instant::now())),
            )?;
        };
        Ok((
            Self {
                wake,
                dma,
                cpu,
                requests,
                completions,
                submitted_at: None,
                timeout: GPU_JOB_TIMEOUT,
            },
            evidence,
        ))
    }
}

impl RendererWorker {
    /// A refused job, and any loan in it, returns to the owner unchanged.
    fn queue(&mut self, job: RenderJob) -> Result<(), (String, Box<RenderJob>)> {
        if self.submitted_at.is_some() {
            return Err((
                format!("{} worker already owns a render job", label(self.cpu)),
                Box::new(job),
            ));
        }
        match self.requests.try_send(job) {
            Ok(()) => {
                self.submitted_at = Some(Instant::now());
                Ok(())
            }
            Err(TrySendError::Full(job)) => Err((
                format!("{} worker request capacity is exhausted", label(self.cpu)),
                Box::new(job),
            )),
            Err(TrySendError::Disconnected(job)) => {
                Err((format!("{} worker stopped", label(self.cpu)), Box::new(job)))
            }
        }
    }
}

impl ContentRenderer for RendererWorker {
    fn completion_fd(&self) -> Option<BorrowedFd<'_>> {
        Some(self.wake.as_fd())
    }

    fn completion_deadline(&self) -> Option<Instant> {
        self.submitted_at.map(|started| started + self.timeout)
    }

    fn clear_completion_wake(&mut self) -> Result<(), String> {
        self.wake
            .clear()
            .map_err(|error| format!("worker wake: {error}"))?;
        if self.submitted_at.is_none() {
            match self.completions.try_recv() {
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    return Err(format!("{} worker stopped", label(self.cpu)));
                }
                Ok(_) => {
                    return Err(format!(
                        "{} worker completed an unowned job",
                        label(self.cpu)
                    ));
                }
            }
        }
        Ok(())
    }

    fn uses_dma_buf(&self) -> bool {
        self.dma
    }

    fn submit(
        &mut self,
        identity: RenderIdentity,
        model: Model,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<(), String> {
        if self.cpu {
            return Err("CPU worker renders only into owned ring fills".into());
        }
        self.queue(RenderJob {
            identity,
            fill: None,
            model,
            width,
            height,
            scale,
        })
        .map_err(|(error, _)| error)
    }

    fn uses_ring_fill(&self) -> bool {
        self.cpu
    }

    fn submit_fill(
        &mut self,
        identity: RenderIdentity,
        model: Model,
        width: u32,
        height: u32,
        scale: f64,
        fill: OwnedRingSlotFill,
    ) -> Result<(), (String, OwnedRingSlotFill)> {
        if !self.cpu {
            return Err(("GPU worker does not fill ring slots".into(), fill));
        }
        self.queue(RenderJob {
            identity,
            fill: Some(fill),
            model,
            width,
            height,
            scale,
        })
        .map_err(|(error, job)| (error, job.fill.expect("queued fill returns with its job")))
    }

    fn poll(&mut self) -> Result<Option<RenderedContent>, String> {
        let Some(started) = self.submitted_at else {
            return Err(format!(
                "{} worker has no render job to poll",
                label(self.cpu)
            ));
        };
        // A late CPU fill is never received for commit; its loan remains
        // quarantined, possibly already queued in the completion receiver.
        // GPU keeps its existing receive-first order.
        if self.cpu && started.elapsed() >= self.timeout {
            return Err(quarantined(true));
        }
        match self.completions.try_recv() {
            Ok(result) => {
                self.submitted_at = None;
                result.map(Some)
            }
            Err(TryRecvError::Empty) if started.elapsed() < self.timeout => Ok(None),
            Err(TryRecvError::Empty) => Err(quarantined(self.cpu)),
            Err(TryRecvError::Disconnected) => Err(format!("{} worker stopped", label(self.cpu))),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/render_worker.rs"]
mod tests;
