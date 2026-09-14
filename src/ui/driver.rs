use super::{MONO_BOLD, MONO_REGULAR, UiState, calendar_view, panel_view};
use crate::{
    model::Model,
    update::{Effect, Msg, update},
};
use masonry::{
    app::{RenderRoot, RenderRootOptions, WindowSizePolicy},
    core::{DefaultProperties, WidgetRef},
    imaging::{Painter, record::Scene},
    kurbo::{Affine, Rect},
    peniko::Blob,
};
use std::sync::Arc;
use xilem_masonry::{
    MasonryRoot, ViewCtx,
    core::{ProxyError, RawProxy, SendMessage, View, ViewId},
};

/// A locally rendered scene and its physical dimensions; never a presented candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentTargetLayout {
    /// Exact TEA message retained with the presented target.
    pub message: Msg,
    /// Indicator identity bound to this button.
    pub indicator: u64,
    /// Authorized WM action identity.
    pub action: u64,
    /// Indicator publication generation.
    pub generation: u64,
    /// Physical left edge in the allocation.
    pub x: i32,
    /// Physical top edge in the allocation.
    pub y: i32,
    /// Physical target width.
    pub width: u32,
    /// Physical target height.
    pub height: u32,
}

/// A rendered scene and the target geometry produced by that same layout pass.
pub struct PreviewScene {
    /// Retained imaging commands from Masonry layout and paint.
    pub scene: Scene,
    /// Physical pixel width.
    pub width: u32,
    /// Physical pixel height.
    pub height: u32,
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

/// Finite, windowless preview host. It owns Xilem reconciliation, not native input.
pub struct PreviewDriver {
    state: UiState,
    view: MasonryRoot<UiState>,
    view_state: <MasonryRoot<UiState> as View<UiState, (), ViewCtx>>::ViewState,
    context: ViewCtx,
    root: RenderRoot,
    width: u32,
    height: u32,
    scale: f64,
    calendar: bool,
}
impl PreviewDriver {
    /// Build without connecting to a display, querying system fonts, or initializing a GPU.
    pub fn new(
        model: Model,
        width: u32,
        height: u32,
        scale: f64,
        calendar: bool,
    ) -> Result<Self, String> {
        if !(1..=8192).contains(&width)
            || !(1..=4096).contains(&height)
            || u64::from(width) * u64::from(height) > 8 * 1024 * 1024
            || !scale.is_finite()
            || !(0.5..=4.0).contains(&scale)
        {
            return Err(
                "preview extent/scale exceeds local bounds (8192×4096, 8M pixels, scale 0.5–4)"
                    .into(),
            );
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|e| e.to_string())?;
        let mut context = ViewCtx::new(Arc::new(NoAsync), Arc::new(runtime));
        let mut state = UiState::new(model);
        let view = make_view(&state.model, width, height, scale, calendar);
        let (element, view_state) = view.build(&mut context, &mut state);
        let mut root = RenderRoot::new(
            element.0.new_widget,
            |_| {},
            RenderRootOptions {
                default_properties: Arc::new(DefaultProperties::new()),
                use_system_fonts: false,
                size_policy: WindowSizePolicy::User,
                size: (width, height).into(),
                scale_factor: scale,
                test_font: Some(Blob::new(Arc::new(MONO_REGULAR.to_vec()))),
            },
        );
        root.register_fonts(Blob::new(Arc::new(MONO_BOLD.to_vec())));
        Ok(Self {
            state,
            view,
            view_state,
            context,
            root,
            width,
            height,
            scale,
            calendar,
        })
    }
    /// Read the current semantic state without exposing the widget tree.
    pub fn model(&self) -> &Model {
        &self.state.model
    }
    /// Apply an observation and reconcile the existing tree. Effects are returned, never executed.
    pub fn apply(&mut self, message: Msg) -> Vec<Effect> {
        let effects = update(&mut self.state.model, message);
        let view = make_view(
            &self.state.model,
            self.width,
            self.height,
            self.scale,
            self.calendar,
        );
        view.rebuild(
            &self.view,
            &mut self.view_state,
            &mut self.context,
            &mut self.root,
            &mut self.state,
        );
        self.view = view;
        effects
    }
    /// Produce a bounded, clipped physical scene using actual Masonry paint output.
    pub fn scene(&mut self) -> PreviewScene {
        self.scene_and_targets().0
    }

    /// Return pixels and workspace targets from one resolved Masonry layout.
    pub fn scene_and_targets(&mut self) -> (PreviewScene, Vec<ContentTargetLayout>) {
        let (layers, _) = self.root.redraw();
        let targets = self.workspace_targets();
        let mut logical = Scene::new();
        layers.replay_into(&mut logical);
        let mut scene = Scene::new();
        {
            let mut painter = Painter::new(&mut scene);
            painter.fill_rect(
                Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height)),
                super::views::color(self.state.model.theme.defaults.background),
            );
        }
        masonry::imaging::record::replay_transformed(
            &logical,
            &mut scene,
            Affine::scale(self.scale),
        );
        (
            PreviewScene {
                scene,
                width: self.width,
                height: self.height,
            },
            targets,
        )
    }

    fn workspace_targets(&self) -> Vec<ContentTargetLayout> {
        let meanings = button_meanings(&self.state.model);
        let mut buttons = Vec::new();
        collect_buttons(self.root.get_layer_root(0), &mut buttons);
        let mut targets = meanings
            .into_iter()
            .zip(buttons)
            .filter_map(|(meaning, bounds)| {
                let (message, indicator, action, generation) = meaning?;
                let x0 = (bounds.x0 * self.scale).floor().max(0.0);
                let y0 = (bounds.y0 * self.scale).floor().max(0.0);
                let x1 = (bounds.x1 * self.scale).ceil().min(f64::from(self.width));
                let y1 = (bounds.y1 * self.scale).ceil().min(f64::from(self.height));
                (x1 > x0 && y1 > y0).then_some(ContentTargetLayout {
                    message,
                    indicator,
                    action,
                    generation,
                    x: x0 as i32,
                    y: y0 as i32,
                    width: (x1 - x0) as u32,
                    height: (y1 - y0) as u32,
                })
            })
            .collect::<Vec<_>>();
        trim_snapped_overlap(
            &mut targets,
            matches!(
                self.state.model.config.position,
                crate::config::Position::Left | crate::config::Position::Right
            ),
        );
        targets
    }
}
impl Drop for PreviewDriver {
    fn drop(&mut self) {
        self.view
            .teardown(&mut self.view_state, &mut self.context, &mut self.root);
    }
}
fn make_view(
    model: &Model,
    width: u32,
    height: u32,
    scale: f64,
    calendar: bool,
) -> MasonryRoot<UiState> {
    if calendar {
        MasonryRoot::new(calendar_view(model))
    } else {
        MasonryRoot::new(panel_view(
            model,
            f64::from(width) / scale,
            f64::from(height) / scale,
        ))
    }
}

fn collect_buttons(widget: WidgetRef<'_, dyn masonry::core::Widget>, bounds: &mut Vec<Rect>) {
    if widget.downcast::<masonry::widgets::Button>().is_some() {
        bounds.push(widget.ctx().bounding_box());
    }
    for child in widget.children() {
        collect_buttons(child, bounds);
    }
}

fn button_meanings(model: &Model) -> Vec<Option<(Msg, u64, u64, u64)>> {
    let mut meanings = Vec::new();
    for region in [
        crate::config::Region::Start,
        crate::config::Region::Center,
        crate::config::Region::End,
    ] {
        for (index, config) in model
            .config
            .modules
            .iter()
            .enumerate()
            .filter(|(_, config)| config.region == region)
        {
            match config.kind {
                crate::config::ModuleKind::Workspaces => {
                    let owner = model.module_id(index);
                    meanings.extend(
                        crate::modules::workspaces(model, config)
                            .into_iter()
                            .filter_map(|(entry, _)| {
                                entry.action.filter(|action| *action != 0).map(|action| {
                                    Some((
                                        Msg::ActivateWorkspace {
                                            owner,
                                            epoch: model.workspaces.epoch,
                                            generation: model.workspaces.generation,
                                            indicator: entry.id,
                                            action,
                                        },
                                        entry.id,
                                        action,
                                        model.workspaces.generation,
                                    ))
                                })
                            }),
                    );
                }
                crate::config::ModuleKind::Clock => meanings.push(None),
                _ => {}
            }
        }
    }
    meanings
}

fn trim_snapped_overlap(targets: &mut [ContentTargetLayout], vertical: bool) {
    for index in 1..targets.len() {
        let (before, after) = targets.split_at_mut(index);
        let prior = &before[index - 1];
        let current = &mut after[0];
        if vertical {
            let prior_end = prior.y.saturating_add_unsigned(prior.height);
            if current.y < prior_end {
                let old_end = current.y.saturating_add_unsigned(current.height);
                current.y = prior_end;
                current.height = old_end.saturating_sub(prior_end) as u32;
            }
        } else {
            let prior_end = prior.x.saturating_add_unsigned(prior.width);
            if current.x < prior_end {
                let old_end = current.x.saturating_add_unsigned(current.width);
                current.x = prior_end;
                current.width = old_end.saturating_sub(prior_end) as u32;
            }
        }
    }
}
