use super::{MONO_BOLD, MONO_REGULAR, UiState, calendar_view, panel_view};
use crate::{
    model::Model,
    update::{Effect, Msg, update},
};
use masonry::{
    app::{RenderRoot, RenderRootOptions, WindowSizePolicy},
    core::DefaultProperties,
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
        let (layers, _) = self.root.redraw();
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
        PreviewScene {
            scene,
            width: self.width,
            height: self.height,
        }
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
