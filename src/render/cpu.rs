//! Explicit Vello CPU rendering into an exact owned-ring payload.

use crate::{protocol::ContentPixels, ui::PreviewScene};
use imaging_vello_cpu::VelloCpuRenderer;
use masonry::imaging::render::{ImageBufferFormat, ImageBufferTarget, ImageRenderer};
use masonry::imaging::{
    kurbo::Affine,
    record::{Scene, replay_transformed},
};
use sophia_shell_protocol::ContentPixelRect;

/// Replay a physical, integer-aligned sub-target of the full resolved scene.
/// Eligibility belongs to the owner; this validates bounds and reports actual
/// output opacity before any loan can be offered.
pub(crate) fn render_region_into(
    renderer: &mut VelloCpuRenderer,
    scene: &mut PreviewScene,
    region: ContentPixelRect,
    target: &mut [u8],
) -> Result<bool, String> {
    if region.x < 0
        || region.y < 0
        || region.width == 0
        || region.height == 0
        || (region.x as u32)
            .checked_add(region.width)
            .is_none_or(|v| v > scene.width)
        || (region.y as u32)
            .checked_add(region.height)
            .is_none_or(|v| v > scene.height)
    {
        return Err("CPU region is outside its resolved panel".into());
    }
    if region.x == 0
        && region.y == 0
        && region.width == scene.width
        && region.height == scene.height
    {
        render_opaque_into(renderer, scene, target)
    } else {
        let mut translated = Scene::new();
        replay_transformed(
            &scene.scene,
            &mut translated,
            Affine::translate((-f64::from(region.x), -f64::from(region.y))),
        );
        let mut tile = PreviewScene {
            scene: translated,
            width: region.width,
            height: region.height,
        };
        render_opaque_into(renderer, &mut tile, target)
    }
}

/// Render one retained scene as straight RGBA8, then convert it in place to
/// the wire's premultiplied BGRA8. `target` is exactly width * height * 4.
pub(crate) fn render_into(
    renderer: &mut VelloCpuRenderer,
    scene: &mut PreviewScene,
    target: &mut [u8],
) -> Result<(), String> {
    render_opaque_into(renderer, scene, target).map(|_| ())
}

fn render_opaque_into(
    renderer: &mut VelloCpuRenderer,
    scene: &mut PreviewScene,
    target: &mut [u8],
) -> Result<bool, String> {
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
    Ok(crate::protocol::rgba8_to_premultiplied_bgra_opaque(target))
}

/// Diagnostic copy reference: the same renderer into a new owned image, then
/// the existing readback conversion for the slice upload.
#[cfg(feature = "diagnostic-cpu-reference")]
pub(crate) fn render_copy(
    renderer: &mut VelloCpuRenderer,
    scene: &mut PreviewScene,
) -> Result<ContentPixels, String> {
    let image = renderer
        .render_source(&mut scene.scene, scene.width, scene.height)
        .map_err(|error| format!("Vello CPU render: {error:?}"))?;
    ContentPixels::from_rgba8(scene.width, scene.height, image.data)
}

#[cfg(test)]
#[path = "../../tests/support/render_cpu.rs"]
mod tests;
