//! Explicit offscreen GPU diagnostics. Nothing here proves native presentation.

use crate::protocol::ContentPixels;
use crate::ui::PreviewScene;
use masonry_imaging::TextureRenderer;
use std::path::Path;

mod completion;
mod readback;
use readback::PendingReadback;

/// Reusable Vello GPU renderer for sequential, bounded preview images.
pub struct GpuPreview {
    renderer: masonry_imaging::vello::Renderer,
    adapter: wgpu::AdapterInfo,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pending: Option<PendingReadback>,
}
impl GpuPreview {
    /// Initialize Vulkan without a surface. Software adapters are explicitly refused.
    ///
    /// Call only for an explicitly requested GPU preview, never during configuration validation.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| format!("no Vulkan GPU adapter: {e}"))?;
        let info = adapter.get_info();
        if matches!(info.device_type, wgpu::DeviceType::Cpu) {
            return Err("software adapter refused: GPU preview has no CPU fallback".into());
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Lom explicit offscreen preview"),
            ..Default::default()
        }))
        .map_err(|e| format!("GPU device request: {e}"))?;
        let renderer = masonry_imaging::vello::new_target_renderer(device.clone(), queue.clone())
            .map_err(|e| e.to_string())?;
        Ok(Self {
            renderer,
            adapter: info,
            device,
            queue,
            pending: None,
        })
    }
    /// Adapter metadata for diagnostic evidence, not native acceptance.
    pub fn adapter(&self) -> &wgpu::AdapterInfo {
        &self.adapter
    }
    /// Render one content-sized scene and return owned wire pixels. This is a
    /// readback building block, not a grant, upload or presentation operation.
    /// Native callers must establish GPU admission before creating the renderer.
    pub fn readback_content(&mut self, scene: &mut PreviewScene) -> Result<ContentPixels, String> {
        ContentPixels::byte_len(scene.width, scene.height)?;
        let bytes = self.read_rgba(scene)?;
        ContentPixels::from_rgba8(scene.width, scene.height, bytes)
    }
    /// Render and read back one PNG. No concurrent frames or accepted native resources exist here.
    pub fn write_png(&mut self, scene: &mut PreviewScene, path: &Path) -> Result<(), String> {
        let bytes = self.read_rgba(scene)?;
        image::save_buffer_with_format(
            path,
            &bytes,
            scene.width,
            scene.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())
    }

    fn read_rgba(&mut self, scene: &mut PreviewScene) -> Result<Vec<u8>, String> {
        if self.pending.is_some() {
            return Err(
                "GPU readback previously failed; renderer retains that job and refuses new work"
                    .into(),
            );
        }
        if scene.width == 0
            || scene.height == 0
            || scene.width > 8192
            || scene.height > 4096
            || u64::from(scene.width) * u64::from(scene.height) > 8 * 1024 * 1024
        {
            return Err("preview exceeds the 8M pixel output bound".into());
        }
        let texture = self
            .renderer
            .render_source_texture(&mut scene.scene, scene.width, scene.height)
            .map_err(|e| e.to_string())?;
        self.pending = Some(PendingReadback::submit(
            &self.device,
            &self.queue,
            texture,
            scene.width,
            scene.height,
        ));
        let bytes = self
            .pending
            .as_ref()
            .expect("submitted readback")
            .finish(&self.device)?;
        self.pending = None;
        Ok(bytes)
    }
}
