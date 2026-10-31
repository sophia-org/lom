use super::*;
use crate::{config::parse_shell_config, model::Model, ui::PreviewDriver};
use chrono::{DateTime, Utc};

fn model() -> Model {
    let (config, theme) =
        parse_shell_config(include_str!("../../examples/minimal/shell.kdl")).unwrap();
    Model::new(config, theme, 1, DateTime::<Utc>::UNIX_EPOCH.fixed_offset())
}

/// Render `scene` both ways with the same Vello CPU backend: the owned
/// image plus the existing readback conversion, and the direct target.
fn assert_direct_equals_reference(mut scene: PreviewScene) -> Vec<u8> {
    let (width, height) = (scene.width, scene.height);
    let image = VelloCpuRenderer::new(1, 1)
        .render_source(&mut scene.scene, width, height)
        .unwrap();
    let straight = image.data.clone();
    let reference = ContentPixels::from_rgba8(width, height, image.data).unwrap();
    // A dirty slot proves the renderer overwrites every byte.
    let mut target = vec![0xa5; ContentPixels::byte_len(width, height).unwrap()];
    render_into(&mut VelloCpuRenderer::new(1, 1), &mut scene, &mut target).unwrap();
    assert_eq!(target, reference.bytes(), "{width}x{height}");
    // The diagnostic copy arm is exactly this reference path.
    #[cfg(feature = "diagnostic-cpu-reference")]
    {
        let copy = super::render_copy(&mut VelloCpuRenderer::new(1, 1), &mut scene).unwrap();
        assert_eq!(copy.bytes(), target, "copy {width}x{height}");
    }
    straight
}

/// Real panel and calendar scenes from the retained Xilem/Masonry driver.
#[test]
fn direct_target_bytes_equal_the_same_renderer_through_the_slice_conversion() {
    for (width, height, scale, popup) in [
        (1280, 24, 1.0, false),
        (1600, 30, 1.25, false),
        (340, 280, 1.0, true),
        (425, 350, 1.25, true),
    ] {
        let mut driver = PreviewDriver::new(model(), width, height, scale, popup).unwrap();
        assert_direct_equals_reference(driver.scene());
    }
}

/// Lom themes are opaque, so translucency and partial-alpha antialiasing come
/// from a recorded imaging scene with fractional edges.
#[test]
fn translucent_scene_bytes_equal_the_same_renderer_through_the_slice_conversion() {
    use masonry::imaging::{Painter, kurbo::Rect, peniko::Color, record::Scene};
    let mut scene = Scene::new();
    {
        let mut painter = Painter::new(&mut scene);
        painter.fill_rect(
            Rect::new(0.0, 0.0, 40.0, 16.0),
            Color::from_rgba8(200, 40, 90, 128),
        );
        painter.fill_rect(
            Rect::new(10.5, 3.25, 30.0, 12.75),
            Color::from_rgba8(20, 180, 250, 77),
        );
    }
    let straight = assert_direct_equals_reference(PreviewScene {
        scene,
        width: 48,
        height: 20,
    });
    let alphas: std::collections::BTreeSet<u8> =
        straight.chunks_exact(4).map(|pixel| pixel[3]).collect();
    assert!(alphas.contains(&0), "transparent pixels");
    assert!(
        alphas
            .iter()
            .filter(|alpha| **alpha != 0 && **alpha != 255)
            .count()
            >= 3,
        "several partial alphas: {alphas:?}"
    );
}

#[test]
fn a_target_of_any_other_length_is_refused_before_rendering() {
    let mut driver = PreviewDriver::new(model(), 64, 24, 1.0, false).unwrap();
    let mut scene = driver.scene();
    let mut renderer = VelloCpuRenderer::new(1, 1);
    for len in [64 * 24 * 4 - 4, 64 * 24 * 4 + 4] {
        let mut target = vec![7u8; len];
        assert!(render_into(&mut renderer, &mut scene, &mut target).is_err());
        assert!(target.iter().all(|byte| *byte == 7));
    }
}

#[test]
fn physical_regions_equal_full_frame_crops_and_report_actual_opacity() {
    for (width, height, vertical) in [(1920, 24, false), (1920, 512, false), (24, 1080, true)] {
        let mut model = model();
        model.config.position = if vertical {
            crate::config::Position::Left
        } else {
            crate::config::Position::Top
        };
        let mut driver = PreviewDriver::new(model, width, height, 1.0, false).unwrap();
        let mut scene = driver.scene();
        let mut renderer = VelloCpuRenderer::new(1, 1);
        let mut full = vec![0; (width * height * 4) as usize];
        render_into(&mut renderer, &mut scene, &mut full).unwrap();
        assert!(full.chunks_exact(4).all(|p| p[3] == 255));
        for index in 0..3 {
            let region = if vertical {
                ContentPixelRect {
                    x: 0,
                    y: (index * height / 3) as i32,
                    width,
                    height: height / 3,
                }
            } else {
                ContentPixelRect {
                    x: (index * width / 3) as i32,
                    y: 0,
                    width: width / 3,
                    height,
                }
            };
            let mut tile = vec![0xa5; (region.width * region.height * 4) as usize];
            assert!(render_region_into(&mut renderer, &mut scene, region, &mut tile).unwrap());
            let expected: Vec<_> = (region.y as u32..region.y as u32 + region.height)
                .flat_map(|y| {
                    let start = ((y * width + region.x as u32) * 4) as usize;
                    full[start..start + region.width as usize * 4]
                        .iter()
                        .copied()
                })
                .collect();
            assert_eq!(tile, expected, "{width}x{height} region {index}");
        }
    }
    let mut empty = PreviewScene {
        scene: Scene::new(),
        width: 12,
        height: 12,
    };
    let region = ContentPixelRect {
        x: 4,
        y: 0,
        width: 4,
        height: 12,
    };
    assert!(
        !render_region_into(
            &mut VelloCpuRenderer::new(1, 1),
            &mut empty,
            region,
            &mut [255; 4 * 12 * 4]
        )
        .unwrap()
    );
}

#[test]
fn regions_refuse_out_of_bounds_and_wrong_payload_lengths() {
    let mut scene = PreviewScene {
        scene: Scene::new(),
        width: 12,
        height: 12,
    };
    let mut renderer = VelloCpuRenderer::new(1, 1);
    for region in [
        ContentPixelRect {
            x: -1,
            y: 0,
            width: 4,
            height: 12,
        },
        ContentPixelRect {
            x: 10,
            y: 0,
            width: 4,
            height: 12,
        },
        ContentPixelRect {
            x: 0,
            y: 10,
            width: 12,
            height: 4,
        },
        ContentPixelRect {
            x: 0,
            y: 0,
            width: 0,
            height: 12,
        },
        ContentPixelRect {
            x: i32::MAX,
            y: 0,
            width: u32::MAX,
            height: 1,
        },
        ContentPixelRect {
            x: 0,
            y: 0,
            width: 4,
            height: 12,
        },
    ] {
        let mut target = vec![7; 4 * 12 * 4 - 1];
        assert!(render_region_into(&mut renderer, &mut scene, region, &mut target).is_err());
        assert!(target.iter().all(|b| *b == 7));
    }
}
