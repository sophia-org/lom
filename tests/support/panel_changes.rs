use super::*;
use crate::{
    config::parse_shell_config,
    model::{Workspace, WorkspaceSnapshot},
    protocol::ContentPixels,
    ui::PreviewDriver,
};
use chrono::{DateTime, Duration, Utc};
use imaging_vello_cpu::VelloCpuRenderer;
use masonry::imaging::render::{ImageBufferFormat, ImageBufferTarget, ImageRenderer};

fn model(height: u32) -> Model {
    let text = include_str!("../fixtures/redraw/static24.kdl")
        .replace("height 24", &format!("height {height}"))
        .replace("    start {", "    center { clock \"minutes\" { format \"%H:%M\"; }; }\n    end { clock \"clock\" { format \"%H:%M:%S\"; }; }\n    start {");
    let (config, theme) = parse_shell_config(&text).unwrap();
    let mut model = Model::new(config, theme, 1, DateTime::<Utc>::UNIX_EPOCH.fixed_offset());
    model.epoch = 1;
    model.workspaces = WorkspaceSnapshot {
        epoch: 1,
        generation: 1,
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

fn render(driver: &mut PreviewDriver) -> Vec<u8> {
    let mut scene = driver.scene();
    let mut rgba = vec![0xa5; (scene.width * scene.height * 4) as usize];
    VelloCpuRenderer::new(1, 1)
        .render_source_into(
            &mut scene.scene,
            ImageBufferTarget {
                data: &mut rgba,
                width: scene.width,
                height: scene.height,
                bytes_per_row: scene.width as usize * 4,
                format: ImageBufferFormat::Rgba8Unorm,
            },
        )
        .unwrap();
    ContentPixels::from_rgba8(scene.width, scene.height, rgba)
        .unwrap()
        .bytes()
        .to_vec()
}

fn activate(model: &mut Model, id: u64) {
    model.workspaces.generation += 1;
    for entry in &mut model.workspaces.entries {
        entry.active = entry.id == id;
    }
}

#[test]
fn predicted_groups_match_changed_pixels_at_1920_horizontal_scale_one() {
    for height in [24, 512] {
        let mut before = model(height);
        let mut driver = PreviewDriver::new(before.clone(), 1920, height, 1.0, false).unwrap();
        let mut pixels = render(&mut driver);
        for (seconds, workspace, expected) in [
            (1, 1, [false, false, true]),
            (1, 2, [true, false, false]),
            (60, 3, [true, true, true]),
            (61, 3, [false, false, true]),
        ] {
            let mut after = before.clone();
            after.time = DateTime::<Utc>::UNIX_EPOCH.fixed_offset() + Duration::seconds(seconds);
            activate(&mut after, workspace);
            assert_eq!(
                panel_changes(&before, &after),
                PanelChanges::Regions(expected)
            );
            assert!(!crate::modules::same_panel_pixels(&before, &after));
            driver.reconcile_model(after.clone());
            let next = render(&mut driver);
            assert!(next.chunks_exact(4).all(|p| p[3] == 255));
            let mut actual = [false; 3];
            for (index, (a, b)) in pixels.chunks_exact(4).zip(next.chunks_exact(4)).enumerate() {
                if a != b {
                    actual[(index % 1920) / 640] = true;
                }
            }
            assert_eq!(
                actual, expected,
                "h{height} t{seconds} workspace{workspace}"
            );
            pixels = next;
            before = after;
        }
    }
}

#[test]
fn publication_authority_and_filtered_observations_do_not_dirty_pixels() {
    let before = model(24);
    let mut after = before.clone();
    after.workspaces.epoch += 1;
    after.workspaces.generation += 1;
    after.workspaces.active_output = Some(2);
    for entry in &mut after.workspaces.entries {
        entry.action = Some(entry.id + 100);
    }
    let mut foreign = after.workspaces.entries[0].clone();
    foreign.id = 100;
    foreign.output = 2;
    foreign.name = "foreign".into();
    after.workspaces.entries.push(foreign);
    after.workspaces.entries.reverse();
    assert_eq!(panel_changes(&before, &after), PanelChanges::Unchanged);
    assert!(crate::modules::same_panel_pixels(&before, &after));
    let mut driver = PreviewDriver::new(before, 1920, 24, 1.0, false).unwrap();
    let pixels = render(&mut driver);
    driver.reconcile_model(after);
    assert_eq!(pixels, render(&mut driver));
}

#[test]
fn shape_and_incarnation_changes_require_full_redraw() {
    let before = model(24);
    let changes: &[fn(&mut Model)] = &[
        |m| m.workspaces.entries[0].name = "a workspace name much wider than the old label".into(),
        |m| {
            m.workspaces.entries.pop();
        },
        |m| m.workspaces.entries[0].action = None,
        |m| m.config.modules.reverse(),
        |m| m.config.height += 1,
        |m| m.theme.defaults.font_size += 1,
        |m| m.generation += 1,
        |m| m.epoch += 1,
        |m| m.output += 1,
        |m| m.fixture = true,
        |m| {
            m.fixture_values
                .insert("observation".into(), "changed".into());
        },
        |m| m.popout = Some(m.module_id(0)),
        |m| m.month += Duration::days(31),
    ];
    for (index, change) in changes.iter().enumerate() {
        let mut after = before.clone();
        change(&mut after);
        assert_eq!(
            panel_changes(&before, &after),
            PanelChanges::Full,
            "case{index}"
        );
        assert!(!crate::modules::same_panel_pixels(&before, &after));
    }
    for format in ["%-S", "%a", "%S秒"] {
        let mut before = model(24);
        let clock = before
            .config
            .modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::Clock)
            .unwrap();
        clock.format = format.into();
        before.time += Duration::seconds(9);
        let mut after = before.clone();
        after.time += if format == "%a" {
            Duration::days(1)
        } else {
            Duration::seconds(1)
        };
        assert_eq!(
            panel_changes(&before, &after),
            PanelChanges::Full,
            "{format}"
        );
    }
}

#[test]
fn coalesced_changes_are_relative_to_the_presented_model() {
    let presented = model(24);
    let mut clock = presented.clone();
    clock.time += Duration::seconds(1);
    let mut latest = clock.clone();
    activate(&mut latest, 2);
    assert_eq!(
        panel_changes(&clock, &latest),
        PanelChanges::Regions([true, false, false])
    );
    assert_eq!(
        panel_changes(&presented, &latest),
        PanelChanges::Regions([true, false, true])
    );
    assert_eq!(
        panel_changes(&presented, &presented),
        PanelChanges::Unchanged
    );
    let mut renamed = presented.clone();
    renamed.workspaces.entries[0].name = "renamed".into();
    assert_eq!(panel_changes(&presented, &renamed), PanelChanges::Full);
    renamed.workspaces.entries[0].name = presented.workspaces.entries[0].name.clone();
    assert_eq!(panel_changes(&presented, &renamed), PanelChanges::Unchanged);
}

#[test]
fn changes_with_an_open_popout_require_full_redraw() {
    let mut before = model(24);
    let clock = before
        .config
        .modules
        .iter()
        .position(|m| m.kind == ModuleKind::Clock)
        .unwrap();
    before.popout = Some(before.module_id(clock));
    let mut workspace = before.clone();
    activate(&mut workspace, 2);
    assert_eq!(panel_changes(&before, &workspace), PanelChanges::Full);
    let mut clock = before.clone();
    clock.time += Duration::seconds(1);
    assert_eq!(panel_changes(&before, &clock), PanelChanges::Full);
    clock.time = before.time + Duration::days(1);
    assert_eq!(panel_changes(&before, &clock), PanelChanges::Full);
}

#[test]
fn module_region_and_space_padded_numeric_clock_are_classified() {
    for (index, region) in [Region::Start, Region::Center, Region::End]
        .into_iter()
        .enumerate()
    {
        for kind in [ModuleKind::Clock, ModuleKind::Workspaces] {
            let mut before = model(24);
            before.config.modules.retain(|m| m.kind == kind);
            before.config.modules.truncate(1);
            before.config.modules[0].region = region;
            before.config.modules[0].format = "%e".into();
            let mut after = before.clone();
            if kind == ModuleKind::Clock {
                after.time += Duration::days(1);
            } else {
                activate(&mut after, 2);
            }
            let mut expected = [false; 3];
            expected[index] = true;
            assert_eq!(
                panel_changes(&before, &after),
                PanelChanges::Regions(expected)
            );
        }
    }
}
