//! Explicit offscreen GPU diagnostics. Nothing here proves native presentation.

use crate::ui::PreviewScene;
use masonry_imaging::ImageRenderer;
use std::path::Path;

/// Reusable Vello GPU renderer for sequential, bounded preview images.
pub struct GpuPreview {
    renderer: masonry_imaging::vello::Renderer,
    adapter: wgpu::AdapterInfo,
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
        let renderer = masonry_imaging::vello::new_target_renderer(device, queue)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            renderer,
            adapter: info,
        })
    }
    /// Adapter metadata for diagnostic evidence, not native acceptance.
    pub fn adapter(&self) -> &wgpu::AdapterInfo {
        &self.adapter
    }
    /// Render and read back one PNG. No concurrent frames or accepted native resources exist here.
    pub fn write_png(&mut self, scene: &mut PreviewScene, path: &Path) -> Result<(), String> {
        if scene.width == 0
            || scene.height == 0
            || u64::from(scene.width) * u64::from(scene.height) > 8 * 1024 * 1024
        {
            return Err("preview exceeds the 8M pixel output bound".into());
        }
        let image = self
            .renderer
            .render_source(&mut scene.scene, scene.width, scene.height)
            .map_err(|e| e.to_string())?;
        image::save_buffer_with_format(
            path,
            &image.data,
            image.width,
            image.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())
    }
}
