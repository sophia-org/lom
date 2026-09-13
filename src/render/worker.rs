//! One bounded GPU worker keeps the protocol owner responsive during rendering.

use super::{GpuGrant, GpuPreview};
use crate::model::Model;
use crate::protocol::ContentPixels;
use crate::service::ContentRenderer;
use crate::ui::PreviewDriver;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::{Duration, Instant};

const GPU_JOB_TIMEOUT: Duration = Duration::from_secs(2);

struct RenderJob {
    model: Model,
    width: u32,
    height: u32,
    scale: f64,
}

/// Capacity-one renderer executor. The worker alone owns Vello and wgpu.
pub struct RendererWorker {
    requests: SyncSender<RenderJob>,
    completions: Receiver<Result<ContentPixels, String>>,
    submitted_at: Option<Instant>,
    timeout: Duration,
}

impl RendererWorker {
    /// Validate the admitted adapter and initialize the worker before serving.
    pub fn start(grant: GpuGrant) -> Result<Self, String> {
        let (requests, incoming) = mpsc::sync_channel::<RenderJob>(1);
        let (completed, completions) = mpsc::sync_channel(1);
        let (ready, readiness) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("lom-gpu".into())
            .spawn(move || {
                let mut renderer = match GpuPreview::new(&grant) {
                    Ok(renderer) => {
                        let _ = ready.send(Ok(()));
                        renderer
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                while let Ok(job) = incoming.recv() {
                    let result =
                        PreviewDriver::new(job.model, job.width, job.height, job.scale, false)
                            .and_then(|mut driver| renderer.readback_content(&mut driver.scene()));
                    if completed.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("start GPU worker: {error}"))?;
        readiness
            .recv_timeout(GPU_JOB_TIMEOUT)
            .map_err(|error| format!("GPU worker startup: {error}"))??;
        Ok(Self {
            requests,
            completions,
            submitted_at: None,
            timeout: GPU_JOB_TIMEOUT,
        })
    }
}

impl ContentRenderer for RendererWorker {
    fn submit(&mut self, model: Model, width: u32, height: u32, scale: f64) -> Result<(), String> {
        if self.submitted_at.is_some() {
            return Err("GPU worker already owns a render job".into());
        }
        match self.requests.try_send(RenderJob {
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

    fn poll(&mut self) -> Result<Option<ContentPixels>, String> {
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
