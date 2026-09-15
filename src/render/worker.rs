//! One bounded GPU worker keeps the protocol owner responsive during rendering.

use super::{GpuAdmissionEvidence, GpuGrant, GpuPreview};
use crate::model::Model;
use crate::service::{ContentRenderer, RenderIdentity, RenderedContent};
use crate::ui::PreviewDriver;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
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
    requests: SyncSender<RenderJob>,
    completions: Receiver<Result<RenderedContent, String>>,
    submitted_at: Option<Instant>,
    timeout: Duration,
}

impl RendererWorker {
    /// Validate the admitted adapter and initialize the worker before serving.
    pub fn start(grant: GpuGrant) -> Result<(Self, GpuAdmissionEvidence), String> {
        let (requests, incoming) = mpsc::sync_channel::<RenderJob>(1);
        let (completed, completions) = mpsc::sync_channel(1);
        let (ready, readiness) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("lom-gpu".into())
            .spawn(move || {
                let mut renderer = match GpuPreview::new(&grant) {
                    Ok(renderer) => {
                        let evidence = renderer
                            .adapter_drm()
                            .ok_or_else(|| {
                                "admitted Vulkan renderer omitted DRM identity".to_owned()
                            })
                            .and_then(|drm| {
                                GpuAdmissionEvidence::collect(&grant, renderer.adapter(), drm)
                            });
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
                        renderer
                            .readback_content(&mut scene)
                            .map(|pixels| RenderedContent { pixels, targets })
                    });
                    if completed.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("start GPU worker: {error}"))?;
        let evidence = readiness
            .recv_timeout(GPU_JOB_TIMEOUT)
            .map_err(|error| format!("GPU worker startup: {error}"))??;
        Ok((
            Self {
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
