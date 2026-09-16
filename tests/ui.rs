//! Offline regression evidence for the Minimal port.

mod support;

use imaging_vello_cpu::VelloCpuRenderer;
use lom::{
    ui::{PreviewDriver, UiState, panel_view},
    update::{Effect, Msg, update},
};
use masonry::{
    core::{DefaultProperties, PointerButton},
    imaging::render::ImageRenderer,
    widgets::Button,
};
use masonry_testing::{TestHarness, TestHarnessParams};
use std::sync::Arc;
use xilem_masonry::{
    ViewCtx, WidgetView,
    core::{
        DynMessage, MessageCtx, ProxyError, RawProxy, SendMessage, View, ViewId, ViewPathTracker,
    },
};

fn raster(driver: &mut PreviewDriver) -> image::RgbaImage {
    let mut scene = driver.scene();
    scene.scene.validate().unwrap();
    let mut renderer = VelloCpuRenderer::new(scene.width as u16, scene.height as u16);
    let image = renderer
        .render_source(&mut scene.scene, scene.width, scene.height)
        .unwrap();
    image::RgbaImage::from_raw(image.width, image.height, image.data).unwrap()
}

fn snapshot(name: &str, image: &image::RgbaImage) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("tests/snapshots").join(format!("{name}.png"));
    if std::env::var_os("LOM_UPDATE_SNAPSHOTS").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        image.save(&path).unwrap();
    }
    let expected = image::open(&path)
        .expect("review and create missing snapshot explicitly")
        .to_rgba8();
    if image != &expected {
        let directory = root.join(".artifacts/snapshots");
        std::fs::create_dir_all(&directory).unwrap();
        image
            .save(directory.join(format!("{name}.actual.png")))
            .unwrap();
    }
    assert_eq!(image.dimensions(), expected.dimensions(), "{name}");
    let differing = image
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(differing, 0, "{name}: see .artifacts/snapshots");
}

#[test]
fn minimal_images_use_real_xilem_masonry_scenes() {
    let model = support::model();
    for (name, width, height, scale) in [
        ("minimal", 1280, 24, 1.0),
        ("narrow", 480, 24, 1.0),
        ("fractional", 1600, 30, 1.25),
    ] {
        let mut driver = PreviewDriver::new(model.clone(), width, height, scale, false).unwrap();
        let image = raster(&mut driver);
        assert_eq!(image.get_pixel(width / 2, 0).0, [28, 28, 28, 255]);
        assert!(
            image.pixels().any(|p| p.0[0] > 100),
            "text must be rendered"
        );
        assert!(
            image.pixels().any(|p| p.0 == [102, 153, 204, 255]),
            "active underline must render"
        );
        assert!(
            image.pixels().any(|p| p.0 == [143, 10, 10, 255]),
            "urgent fill must render"
        );
        snapshot(name, &image);
    }
}

#[test]
fn only_active_workspace_has_an_underline_after_reconciliation() {
    let model = support::model();
    let style = model.theme.resolve(&model.config.modules[0]);
    let mut next = model.workspaces.clone();
    let mut driver = PreviewDriver::new(model, 1280, 24, 1.0, false).unwrap();
    let (_, targets) = driver.scene_and_targets();
    let active = targets.iter().find(|t| t.indicator == 1).unwrap();
    let elsewhere = targets.iter().find(|t| t.indicator == 2).unwrap();
    let sample = |image: &image::RgbaImage, target: &lom::ui::ContentTargetLayout| {
        image.get_pixel(target.x as u32 + target.width / 2, 23).0
    };
    let rgba = |c: lom::config::Color| [c.0[0], c.0[1], c.0[2], 255];
    let before = raster(&mut driver);
    assert_eq!(sample(&before, active), rgba(style.active));
    assert_eq!(sample(&before, elsewhere), rgba(style.selected));
    next.generation += 1;
    next.entries[0].active = false;
    next.entries[0].visible = true;
    next.entries[1].active = true;
    driver.apply(Msg::Workspaces(next));
    let after = raster(&mut driver);
    assert_eq!(sample(&after, active), rgba(style.selected));
    assert_eq!(sample(&after, elsewhere), rgba(style.active));
    let (_, updated) = driver.scene_and_targets();
    assert_eq!(
        targets
            .iter()
            .map(|t| (t.indicator, t.action))
            .collect::<Vec<_>>(),
        updated
            .iter()
            .map(|t| (t.indicator, t.action))
            .collect::<Vec<_>>()
    );
}

#[test]
fn calendar_and_week_numbers_have_reviewable_snapshots() {
    let mut model = support::model();
    let id = model.module_id(5);
    update(&mut model, Msg::ToggleCalendar(id));
    let mut driver = PreviewDriver::new(model.clone(), 340, 280, 1.0, true).unwrap();
    snapshot("calendar", &raster(&mut driver));
    model.config.modules[5].show_week_numbers = true;
    let mut driver = PreviewDriver::new(model, 425, 350, 1.25, true).unwrap();
    snapshot("calendar-weeks", &raster(&mut driver));
}

#[test]
fn rebuild_changes_existing_scene_and_teardown_can_repeat() {
    let model = support::model();
    let mut snapshot = model.workspaces.clone();
    snapshot.generation += 1;
    snapshot.entries[0].active = false;
    snapshot.entries[1].active = true;
    let mut driver = PreviewDriver::new(model, 1280, 24, 1.0, false).unwrap();
    let before = raster(&mut driver);
    driver.apply(Msg::Workspaces(snapshot));
    assert_ne!(before, raster(&mut driver));
    let stable = raster(&mut driver);
    assert_eq!(stable, raster(&mut driver));
    for _ in 0..3 {
        let mut another = PreviewDriver::new(support::model(), 1280, 24, 1.0, false).unwrap();
        assert_eq!(raster(&mut another), before);
    }
}

#[test]
fn empty_focused_output_and_absent_battery_are_valid_views() {
    let mut model = support::model();
    model.workspaces.entries.clear();
    model.workspaces.active_output = Some(2);
    model.fixture_values.remove("battery");
    let mut driver = PreviewDriver::new(model, 1280, 24, 1.0, false).unwrap();
    snapshot("empty-focused-output", &raster(&mut driver));
}

#[test]
fn extreme_preview_dimensions_are_rejected_before_a_device_exists() {
    for (width, height, scale) in [
        (0, 24, 1.0),
        (9000, 24, 1.0),
        (8192, 4096, 1.0),
        (1280, 24, f64::NAN),
        (1280, 24, 10.0),
    ] {
        assert!(PreviewDriver::new(support::model(), width, height, scale, false).is_err());
    }
}

#[derive(Debug)]
struct NoAsync;
impl RawProxy for NoAsync {
    fn send_message(&self, _: Arc<[ViewId]>, message: SendMessage) -> Result<(), ProxyError> {
        Err(ProxyError::DriverFinished(message))
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

#[test]
fn actual_widget_click_emits_a_message_before_the_reducer_changes_state() {
    let mut model = support::model();
    model
        .config
        .modules
        .retain(|m| m.kind == lom::config::ModuleKind::Clock);
    let clock = model.module_id(0);
    let view = panel_view(&model, 1280.0, 24.0).boxed();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoAsync), Arc::new(runtime));
    let mut state = UiState::new(model);
    let (pod, mut view_state) = view.build(&mut ctx, &mut state);
    let mut harness = TestHarness::create_with(
        DefaultProperties::new(),
        pod.new_widget,
        TestHarnessParams::default().with_size((1280, 24)),
    );
    let mut buttons = Vec::new();
    harness.inspect_widgets(|widget| {
        if widget.downcast::<Button>().is_some() {
            buttons.push(widget.id());
        }
    });
    assert_eq!(buttons.len(), 1);
    harness.mouse_click_on(buttons[0], Some(PointerButton::Primary));
    let (action, source) = harness.pop_action_erased().expect("real button action");
    let path = ctx.get_id_path(source).unwrap().clone();
    let mut message = MessageCtx::new(std::mem::take(ctx.environment()), path, DynMessage(action));
    harness.edit_root_widget(|root| view.message(&mut view_state, &mut message, root, &mut state));
    let (environment, _, _) = message.finish();
    *ctx.environment() = environment;
    assert_eq!(state.model.popout, None, "view callback only enqueues");
    assert_eq!(
        state.take_messages().unwrap(),
        vec![Msg::ToggleCalendar(clock)]
    );
    assert_eq!(
        update(&mut state.model, Msg::ToggleCalendar(clock)),
        vec![Effect::RequestPopout(clock)]
    );
    assert!(state.take_messages().unwrap().is_empty());
    for _ in 0..65 {
        harness.mouse_click_on(buttons[0], Some(PointerButton::Primary));
        let (action, source) = harness.pop_action_erased().unwrap();
        let path = ctx.get_id_path(source).unwrap().clone();
        let mut message =
            MessageCtx::new(std::mem::take(ctx.environment()), path, DynMessage(action));
        harness
            .edit_root_widget(|root| view.message(&mut view_state, &mut message, root, &mut state));
        let (environment, _, _) = message.finish();
        *ctx.environment() = environment;
    }
    assert!(
        state.take_messages().is_err(),
        "queue saturation must be visible"
    );
}

#[test]
fn workspace_targets_come_from_the_same_masonry_layout_as_pixels() {
    let model = support::model();
    let expected = model
        .workspaces
        .entries
        .iter()
        .filter(|entry| entry.output == model.output && entry.action.is_some())
        .count();
    let mut driver = PreviewDriver::new(model, 1280, 24, 1.0, false).unwrap();
    let (scene, targets) = driver.scene_and_targets();
    assert_eq!((scene.width, scene.height), (1280, 24));
    assert_eq!(targets.len(), expected);
    assert!(targets.iter().all(|target| {
        target.x >= 0
            && target.y >= 0
            && target.width > 0
            && target.height > 0
            && target.x as u32 + target.width <= scene.width
            && target.y as u32 + target.height <= scene.height
    }));
    for pair in targets.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        assert!(left.x as u32 + left.width <= right.x as u32);
    }
    assert!(targets.iter().all(|target| matches!(
        target.message,
        Msg::ActivateWorkspace { indicator, action, .. }
            if indicator == target.indicator && action == target.action
    )));
}
