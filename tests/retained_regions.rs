//! Device-free region feasibility: replay the real scene into smaller targets.
//! These byte comparisons do not submit resources or prove native composition.
use chrono::{DateTime, Duration, Utc};
use imaging_vello_cpu::VelloCpuRenderer;
use lom::{
    config::parse_shell_config,
    model::{Model, Workspace, WorkspaceSnapshot},
    protocol::ContentPixels,
    ui::{PreviewDriver, PreviewScene},
};
use masonry::imaging::{
    Painter,
    kurbo::{Affine, Rect},
    peniko::Color,
    record::{Scene, replay_transformed},
    render::{ImageBufferFormat, ImageBufferTarget, ImageRenderer},
};

#[path = "support/retained_region_screen.rs"]
mod screen;

fn model(height: u32) -> Model {
    let text = include_str!("fixtures/redraw/static24.kdl")
        .replace("height 24", &format!("height {height}"))
        .replace(
            "    start {",
            "    end { clock \"clock\" { format \"%H:%M:%S\"; }; }\n    start {",
        );
    let (config, theme) = parse_shell_config(&text).unwrap();
    let mut model = Model::new(config, theme, 1, DateTime::<Utc>::UNIX_EPOCH.fixed_offset());
    model.epoch = 1;
    model.workspaces = WorkspaceSnapshot {
        epoch: 1,
        generation: 2,
        active_output: Some(1),
        entries: (1..=9)
            .map(|id| Workspace {
                id,
                output: 1,
                name: id.to_string(),
                active: id == 1,
                visible: false,
                urgent: false,
                action: Some(id + 10),
            })
            .collect(),
    };
    model
}

fn render(scene: &Scene, x: u32, y: u32, width: u32, height: u32) -> Vec<u8> {
    let mut translated = Scene::new();
    replay_transformed(
        scene,
        &mut translated,
        Affine::translate((-f64::from(x), -f64::from(y))),
    );
    // The full-frame oracle paints the original scene, not our replay helper.
    let mut source = if x == 0 && y == 0 { scene } else { &translated };
    let mut bytes = vec![0xa5; ContentPixels::byte_len(width, height).unwrap()];
    VelloCpuRenderer::new(1, 1)
        .render_source_into(
            &mut source,
            ImageBufferTarget {
                data: &mut bytes,
                width,
                height,
                bytes_per_row: width as usize * 4,
                format: ImageBufferFormat::Rgba8Unorm,
            },
        )
        .unwrap();
    ContentPixels::from_rgba8(width, height, bytes)
        .unwrap()
        .bytes()
        .to_vec()
}

fn equal(actual: &[u8], expected: &[u8], context: &str) {
    assert_eq!(actual.len(), expected.len());
    let mismatches: Vec<_> = actual
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .take(8)
        .collect();
    assert!(
        mismatches.is_empty(),
        "{context}: first differing bytes {mismatches:?}"
    );
}

fn crop(full: &[u8], width: u32, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
    let mut result = Vec::new();
    for row in y..y + h {
        let start = ((row * width + x) * 4) as usize;
        result.extend_from_slice(&full[start..start + w as usize * 4]);
    }
    result
}

fn verify_regions(scene: PreviewScene, columns: &[u32], rows: &[u32]) -> Vec<u8> {
    assert_eq!(columns[0], 0);
    assert_eq!(*columns.last().unwrap(), scene.width);
    assert_eq!(rows[0], 0);
    assert_eq!(*rows.last().unwrap(), scene.height);
    let full = render(&scene.scene, 0, 0, scene.width, scene.height);
    for ys in rows.windows(2) {
        for xs in columns.windows(2) {
            let (w, h) = (xs[1] - xs[0], ys[1] - ys[0]);
            let tile = render(&scene.scene, xs[0], ys[0], w, h);
            equal(
                &tile,
                &crop(&full, scene.width, xs[0], ys[0], w, h),
                &format!(
                    "{}x{} tile ({},{}) {w}x{h}",
                    scene.width, scene.height, xs[0], ys[0]
                ),
            );
        }
    }
    full
}

#[test]
fn translated_real_panel_regions_match_full_frame_at_fractional_scales() {
    for (width, height, scale) in [(1920, 24, 1.0), (1920, 512, 1.0), (1919, 30, 1.25)] {
        let mut driver = PreviewDriver::new(model(height), width, height, scale, false).unwrap();
        let full = verify_regions(
            driver.scene(),
            &[0, 17, 127, 640, 1280, width],
            &[0, 7, height],
        );
        assert!(
            full.chunks_exact(4).all(|pixel| pixel[3] == 255),
            "real panel fixture must be opaque"
        );
    }
}

fn edge_scene(opaque: bool) -> Scene {
    let mut scene = Scene::new();
    {
        let mut painter = Painter::new(&mut scene);
        if opaque {
            painter.fill_rect(Rect::new(0., 0., 96., 32.), Color::from_rgb8(28, 28, 28));
        }
        painter.fill_rect(
            Rect::new(0.25, 0.5, 91.75, 30.5),
            Color::from_rgba8(201, 77, 19, 128),
        );
        painter.fill_rect(
            Rect::new(16.5, 3.25, 70.75, 22.25),
            Color::from_rgba8(20, 180, 250, 77),
        );
    }
    scene
}

#[test]
fn fractional_translucent_paint_on_an_opaque_background_matches_regions() {
    verify_regions(
        PreviewScene {
            scene: edge_scene(true),
            width: 96,
            height: 32,
        },
        &[0, 17, 64, 96],
        &[0, 7, 32],
    );
}

#[test]
#[ignore = "device-free boundary diagnosis"]
fn diagnose_translucent_boundaries() {
    for opaque in [false, true] {
        let scene = edge_scene(opaque);
        let full = render(&scene, 0, 0, 96, 32);
        println!(
            "region_probe opaque_background={opaque} min_alpha={} nonopaque_pixels={}",
            full.chunks_exact(4).map(|pixel| pixel[3]).min().unwrap(),
            full.chunks_exact(4).filter(|pixel| pixel[3] != 255).count()
        );
        for x in [1, 3, 4, 8, 16, 17, 32, 33, 64] {
            for y in [0, 1, 7, 8, 16] {
                let tile = render(&scene, x, y, 96 - x, 32 - y);
                let expected = crop(&full, 96, x, y, 96 - x, 32 - y);
                let n = tile.iter().zip(&expected).filter(|(a, b)| a != b).count();
                println!("region_probe opaque={opaque} x={x} y={y} differing_bytes={n}");
            }
        }
    }
}

#[test]
fn clock_and_workspace_updates_reconstruct_from_retained_regions() {
    for height in [24, 512] {
        let mut state = model(height);
        let mut driver = PreviewDriver::new(state.clone(), 1920, height, 1.0, false).unwrap();
        let mut scene = driver.scene();
        let mut tiles: Vec<_> = (0..3)
            .map(|i| render(&scene.scene, i * 640, 0, 640, height))
            .collect();
        for (seconds, workspace, changed) in [
            (1, 1, vec![2]),
            (1, 2, vec![0]),
            (60, 3, vec![0, 2]),
            (0, 1, vec![0, 2]),
        ] {
            state.time = DateTime::<Utc>::UNIX_EPOCH.fixed_offset() + Duration::seconds(seconds);
            state.workspaces.generation += 1;
            for entry in &mut state.workspaces.entries {
                entry.active = entry.id == workspace;
            }
            driver.reconcile_model(state.clone());
            scene = driver.scene();
            for index in changed {
                let replacement = render(&scene.scene, index as u32 * 640, 0, 640, height);
                assert_ne!(
                    replacement, tiles[index],
                    "fixture must change tile {index}"
                );
                tiles[index] = replacement;
            }
            let full = render(&scene.scene, 0, 0, 1920, height);
            assert!(full.chunks_exact(4).all(|pixel| pixel[3] == 255));
            for (i, tile) in tiles.iter().enumerate() {
                equal(
                    tile,
                    &crop(&full, 1920, i as u32 * 640, 0, 640, height),
                    &format!("retained h{height} t{seconds} workspace{workspace} tile{i}"),
                );
            }
        }
    }
}
