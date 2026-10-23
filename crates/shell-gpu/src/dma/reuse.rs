//! Redraw of an exported frame's own LINEAR image once Sophia finished with it.
//! The image, memory, descriptor, layout and retention slot are kept; only
//! the draw and the two queue-family barriers repeat.
//!
//! Three encoders go to wgpu in one `Queue::submit`: a raw foreign return, the
//! blit, and a raw hand-off. wgpu 28 forbids mixing raw and wgpu commands in
//! one encoder, keeps the submitted order, and puts each buffer's own
//! transitions just before it, so the return precedes every wgpu use. Raw
//! encoders register no resource lifetime: the frame stays held until
//! wgpu's poll proves the submission complete.
use super::{DmaFrame, EXPORT_TIMEOUT, HANDOFF_STAGES, Stalled, handoff, remaining};
use crate::dma_timing::Stage;
use ash::vk;
use imaging::record::Scene;
use masonry_imaging::TextureRenderer;
use std::time::Instant;
use wgpu::hal::api::Vulkan;

impl crate::GpuPreview {
    /// Draw `scene` into `frame`'s existing DMA-BUF and hand it back to the
    /// foreign queue family, with the fresh export's completion guarantee
    /// and no new allocation or descriptor. The frame keeps its size, layout
    /// and descriptor, so a consumer sees the same buffer with new content.
    ///
    /// A refusal before drawing drops `frame`. An error returned after the
    /// Vello render keeps the frame and the rendered source here and refuses
    /// all new work, as an export with unknown completion does.
    ///
    /// # Panics
    ///
    /// wgpu validation and device errors while encoding or submitting go to
    /// wgpu's uncaptured-error handler, which panics by default. Such a panic
    /// bypasses retention, so recovery and the frame's lifetime are not
    /// guaranteed. The fresh export has the same exposure.
    ///
    /// # Safety
    ///
    /// Every foreign access to the frame's memory must have finished, and no
    /// queued or re-sendable use of it may remain: no descriptor from
    /// [`DmaFrame::dup_fd`] that could be sent again is still held.
    ///
    /// With the Sophia SDK, the concrete proof is the frame's last offer
    /// reported Accepted and then released as capture-complete (disposition
    /// 2), which Sophia sends only after its read of the buffer signalled,
    /// followed by `image_forget` for that offer. A controlled consumer may
    /// give its own proof; `examples/dma_peer.rs` uses its receiver's DONE
    /// status 0 instead.
    ///
    /// The GPU overwrites the buffer without waiting on any implicit fence,
    /// so nothing here can protect a read still in flight.
    pub unsafe fn render_dma_buf_reusing(
        &mut self,
        scene: &mut Scene,
        frame: DmaFrame,
    ) -> Result<DmaFrame, String> {
        let mark = self.dma_timing.start();
        let (width, height) = (frame.width, frame.height);
        self.admit(width, height)?;
        if !self.frames.owns(&frame._slot) {
            return Err("DMA-BUF frame was exported by another renderer".into());
        }
        let deadline = Instant::now() + EXPORT_TIMEOUT;
        let (device, family) = {
            // SAFETY: borrowed only to read this device's raw handles.
            let hal =
                unsafe { self.device.as_hal::<Vulkan>() }.ok_or("wgpu device has no Vulkan HAL")?;
            (hal.raw_device().clone(), hal.queue_family_index())
        };
        let target = frame
            .texture
            .as_ref()
            .ok_or("DMA-BUF frame has no texture")?
            .create_view(&wgpu::TextureViewDescriptor::default());
        let source = self
            .renderer
            .render_source_texture(scene, width, height)
            .map_err(|error| error.to_string())?;
        // The Vello draw may be pending from here on, so every failure keeps
        // both the source and the whole frame.
        let submitted = self.encode_reuse(&device, family, frame.image, &source, &target);
        let result = submitted.and_then(|draw| {
            self.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(draw),
                    timeout: Some(remaining(deadline)),
                })
                .map_err(|error| format!("GPU DMA-BUF reuse: {error}"))
        });
        if let Err(error) = result {
            self.stalled = Some(Stalled {
                _source: Some(source),
                _target: None,
                _frame: Some(frame),
                _release: None,
            });
            return Err(error);
        }
        drop(source);
        self.dma_timing.record(Stage::Reuse, mark);
        Ok(frame)
    }

    /// Record the return, blit and hand-off, and submit them together.
    fn encode_reuse(
        &mut self,
        device: &ash::Device,
        family: u32,
        image: vk::Image,
        source: &wgpu::Texture,
        target: &wgpu::TextureView,
    ) -> Result<wgpu::SubmissionIndex, String> {
        let acquire = self.raw_barrier(
            device,
            foreign_return(image, family),
            RETURN_STAGES,
            "Shell DMA-BUF reuse return",
        )?;
        let blitter = self.blitter.get_or_insert_with(|| {
            wgpu::util::TextureBlitter::new(&self.device, wgpu::TextureFormat::Bgra8Unorm)
        });
        let mut blit = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Shell DMA-BUF reuse blit"),
            });
        blitter.copy(
            &self.device,
            &mut blit,
            &source.create_view(&wgpu::TextureViewDescriptor::default()),
            target,
        );
        let release = self.raw_barrier(
            device,
            handoff(image, family),
            HANDOFF_STAGES,
            "Shell DMA-BUF reuse hand-off",
        )?;
        Ok(self.queue.submit([acquire, blit.finish(), release]))
    }

    /// A wgpu command buffer holding only one raw image barrier.
    fn raw_barrier(
        &self,
        device: &ash::Device,
        barrier: vk::ImageMemoryBarrier<'static>,
        (src, dst): (vk::PipelineStageFlags, vk::PipelineStageFlags),
        label: &str,
    ) -> Result<wgpu::CommandBuffer, String> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        // SAFETY: this encoder is raw-only: nothing else records into it, the
        // buffer is neither ended nor destroyed here, and wgpu ends it at
        // `finish`. The image outlives the submission (see module docs).
        let recorded = unsafe {
            encoder.as_hal_mut::<Vulkan, _, _>(|hal| {
                hal.map(|hal| {
                    device.cmd_pipeline_barrier(
                        hal.raw_handle(),
                        src,
                        dst,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier],
                    );
                })
            })
        };
        recorded.ok_or_else(|| format!("{label}: wgpu encoder has no Vulkan HAL"))?;
        Ok(encoder.finish())
    }
}

/// Nothing local precedes the return; Sophia's signalled read, observed
/// before the caller's call, orders the foreign side.
const RETURN_STAGES: (vk::PipelineStageFlags, vk::PipelineStageFlags) = (
    vk::PipelineStageFlags::TOP_OF_PIPE,
    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
);

/// Take the image back from completed foreign access. Its premise is the
/// layout the hand-off left for the importer, GENERAL; this is not the
/// matching half of an internal release/acquire pair. It restores the
/// COLOR_ATTACHMENT_OPTIMAL layout wgpu's tracker still records from the
/// last blit, so wgpu's own transitions for the blit stay correct.
fn foreign_return(image: vk::Image, family: u32) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        )
        .old_layout(vk::ImageLayout::GENERAL)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
        .dst_queue_family_index(family)
        .image(image)
        .subresource_range(super::COLOR)
}
