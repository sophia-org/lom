//! Caller-owned bounded GPU readback. No upstream indefinite poll is used.
use super::completion::wait_for_map;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// A prototype recovery budget, not presentation pacing.
const READBACK_TIMEOUT: Duration = Duration::from_secs(2);

/// A failed/timeout job retains its resources and blocks new jobs. An elapsed
/// deadline does not mean the GPU stopped referencing the texture.
pub(super) struct PendingReadback {
    _texture: wgpu::Texture,
    buffer: wgpu::Buffer,
    receiver: Receiver<Result<(), String>>,
    submission: wgpu::SubmissionIndex,
    row_bytes: u32,
    padded_row_bytes: u32,
    height: u32,
}

impl PendingReadback {
    pub(super) fn submit(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Self {
        let row_bytes = width * 4;
        let padded_row_bytes = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Lom bounded GPU readback"),
            size: u64::from(padded_row_bytes) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Lom readback copy"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result.map_err(|error| error.to_string()));
            });
        Self {
            _texture: texture,
            buffer,
            receiver,
            submission,
            row_bytes,
            padded_row_bytes,
            height,
        }
    }

    pub(super) fn finish(&self, device: &wgpu::Device) -> Result<Vec<u8>, String> {
        wait_for_map(READBACK_TIMEOUT, &self.receiver, |timeout| {
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(self.submission.clone()),
                    timeout: Some(timeout),
                })
                .map(|_| ())
                .map_err(|error| format!("GPU readback poll: {error}"))
        })?;
        let mapped = self.buffer.slice(..).get_mapped_range();
        let bytes = self.row_bytes as usize * self.height as usize;
        let mut packed = Vec::new();
        packed
            .try_reserve_exact(bytes)
            .map_err(|error| format!("GPU readback CPU storage: {error}"))?;
        for row in mapped.chunks_exact(self.padded_row_bytes as usize) {
            packed.extend_from_slice(&row[..self.row_bytes as usize]);
        }
        drop(mapped);
        self.buffer.unmap();
        Ok(packed)
    }
}
