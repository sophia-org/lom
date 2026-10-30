//! Explicit Vello CPU rendering into an exact owned-ring payload.

use crate::{protocol::ContentPixels, ui::PreviewScene};
use imaging_vello_cpu::VelloCpuRenderer;
use masonry::imaging::render::{ImageBufferFormat, ImageBufferTarget, ImageRenderer};

/// Render one retained scene as straight RGBA8, then convert it in place to
/// the wire's premultiplied BGRA8. `target` is exactly width * height * 4.
pub(crate) fn render_into(
    renderer: &mut VelloCpuRenderer,
    scene: &mut PreviewScene,
    target: &mut [u8],
) -> Result<(), String> {
    if target.len() != ContentPixels::byte_len(scene.width, scene.height)? {
        return Err("CPU render target is not exactly width * height * 4".into());
    }
    renderer
        .render_source_into(
            &mut scene.scene,
            ImageBufferTarget {
                data: target,
                width: scene.width,
                height: scene.height,
                bytes_per_row: scene.width as usize * 4,
                format: ImageBufferFormat::Rgba8Unorm,
            },
        )
        .map_err(|error| format!("Vello CPU render: {error:?}"))?;
    crate::protocol::rgba8_to_premultiplied_bgra(target);
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/render_cpu.rs"]
mod tests;
