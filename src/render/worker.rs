//! One bounded GPU worker keeps the protocol owner responsive during rendering.

use super::{GpuAdmissionEvidence, GpuGrant, GpuPreview};
use crate::model::Model;
use crate::service::{ContentRenderer, RenderIdentity, RenderImage, RenderedContent};
use crate::ui::PreviewDriver;
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

/// Capacity-one renderer executor. The worker alone owns Vello and wgpu.
pub struct RendererWorker {
    wake: Arc<crate::wake::Wake>,
    dma: bool,
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

    fn start_source(
        grant: GpuGrant,
        dma: bool,
        stop: Option<(&AtomicBool, BorrowedFd<'_>)>,
    ) -> Result<(Self, GpuAdmissionEvidence), String> {
        let (requests, incoming) = mpsc::sync_channel::<RenderJob>(1);
        let (completed, completions) = mpsc::sync_channel(1);
        let (ready, readiness) = mpsc::sync_channel(1);
        let wake =
            Arc::new(crate::wake::Wake::new().map_err(|error| format!("worker wake: {error}"))?);
        let completed = crate::wake::Sender::new(completed, wake.clone());
        let ready = crate::wake::Sender::new(ready, wake.clone());
        std::thread::Builder::new()
            .name("lom-gpu".into())
            .spawn(move || {
                let mut renderer = match GpuPreview::new(&grant) {
                    Ok(renderer) => {
                        let evidence = renderer.evidence(&grant);
                        if ready.send(evidence).is_err() {
                            return;
                        }
                        renderer
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let mut hosts = RetainedHosts::default();
                while let Ok(job) = incoming.recv() {
                    let result = hosts.scene(job).and_then(|(mut scene, targets)| {
                        let pixels = if dma {
                            renderer.dma_content(&mut scene).map(RenderImage::Dma)
                        } else {
                            renderer.readback_content(&mut scene).map(RenderImage::Cpu)
                        }?;
                        Ok(RenderedContent { pixels, targets })
                    });
                    if completed.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("start GPU worker: {error}"))?;
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
                    return Err("GPU worker stopped during startup".into());
                }
                Err(TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err("GPU worker startup timed out".into());
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
                requests,
                completions,
                submitted_at: None,
                timeout: GPU_JOB_TIMEOUT,
            },
            evidence,
        ))
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
                Err(TryRecvError::Disconnected) => return Err("GPU worker stopped".into()),
                Ok(_) => return Err("GPU worker completed an unowned job".into()),
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
        if self.submitted_at.is_some() {
            return Err("GPU worker already owns a render job".into());
        }
        match self.requests.try_send(RenderJob {
            identity,
            model,
            width,
            height,
            scale,
        }) {
            Ok(()) => {
                self.submitted_at = Some(Instant::now());
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err("GPU worker request capacity is exhausted".into()),
            Err(TrySendError::Disconnected(_)) => Err("GPU worker stopped".into()),
        }
    }

    fn poll(&mut self) -> Result<Option<RenderedContent>, String> {
        let Some(started) = self.submitted_at else {
            return Err("GPU worker has no render job to poll".into());
        };
        match self.completions.try_recv() {
            Ok(result) => {
                self.submitted_at = None;
                result.map(Some)
            }
            Err(TryRecvError::Empty) if started.elapsed() < self.timeout => Ok(None),
            Err(TryRecvError::Empty) => {
                Err("GPU worker deadline expired; in-flight resources remain quarantined".into())
            }
            Err(TryRecvError::Disconnected) => Err("GPU worker stopped".into()),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/render_worker.rs"]
mod tests;
