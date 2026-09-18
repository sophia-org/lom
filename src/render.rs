//! Lom's scene/protocol adapter over the shared client-side GPU owner.
use crate::{protocol::ContentPixels, ui::PreviewScene};
use std::path::Path;

mod worker;
pub use sophia_shell_gpu::{GpuAdmissionEvidence, GpuGrant};
pub use worker::RendererWorker;

/// Lom-specific pixel and diagnostic adaptation. The inner owner retains GPU
/// admission and any failed readback resources independently of protocol state.
pub struct GpuPreview(sophia_shell_gpu::GpuPreview);
impl GpuPreview {
    /// Initialize only against the explicit shell grant.
    pub fn new(grant: &GpuGrant) -> Result<Self, String> {
        sophia_shell_gpu::GpuPreview::new(grant).map(Self)
    }
    /// Explicit operator-requested offscreen diagnostic, never a native fallback.
    pub fn new_diagnostic() -> Result<Self, String> {
        sophia_shell_gpu::GpuPreview::new_diagnostic().map(Self)
    }
    /// Selected adapter information; not presentation evidence.
    pub fn adapter(&self) -> &wgpu::AdapterInfo {
        self.0.adapter()
    }
    fn evidence(&self, grant: &GpuGrant) -> Result<GpuAdmissionEvidence, String> {
        self.0.evidence(grant)
    }
    /// Read bounded pixels for the existing protocol resource owner.
    pub fn readback_content(&mut self, scene: &mut PreviewScene) -> Result<ContentPixels, String> {
        ContentPixels::byte_len(scene.width, scene.height)?;
        let bytes = self
            .0
            .read_rgba(&mut scene.scene, scene.width, scene.height)?;
        ContentPixels::from_rgba8(scene.width, scene.height, bytes)
    }
    /// Write an explicit diagnostic image without claiming native presentation.
    pub fn write_png(&mut self, scene: &mut PreviewScene, path: &Path) -> Result<(), String> {
        let bytes = self
            .0
            .read_rgba(&mut scene.scene, scene.width, scene.height)?;
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
}
