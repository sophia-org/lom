use crate::{
    config::{Color as ModelColor, ModuleKind, Position, Region, Style},
    model::Model,
    modules,
    update::Msg,
};
use chrono::Datelike;
use masonry::{
    kurbo::Axis,
    layout::AsUnit,
    layout::{Dim, UnitPoint},
    parley::style::FontWeight,
    peniko::Color,
    properties::{LineBreaking, Padding},
};
use std::collections::VecDeque;
use xilem_masonry::{
    AnyWidgetView, WidgetView,
    style::Style as _,
    view::{
        MainAxisAlignment, ZStackExt as _, button, flex, flex_col, flex_row, label, portal,
        sized_box, zstack,
    },
};

/// UI owner state. Callbacks enqueue semantic messages; hosts drain before rebuilding.
pub struct UiState {
    /// The application model, kept separate from toolkit state.
    pub model: Model,
    messages: VecDeque<Msg>,
    overflow: bool,
}
impl UiState {
    /// Create an adapter state with a finite message queue.
    pub fn new(model: Model) -> Self {
        Self {
            model,
            messages: VecDeque::new(),
            overflow: false,
        }
    }
    /// Take pending widget messages. Saturation is an explicit preview failure.
    pub fn take_messages(&mut self) -> Result<Vec<Msg>, &'static str> {
        if self.overflow {
            return Err("widget message queue exceeded 64 entries");
        }
        Ok(self.messages.drain(..).collect())
    }
    fn queue(&mut self, message: Msg) {
        if self.messages.len() == 64 {
            self.overflow = true;
        } else {
            self.messages.push_back(message);
        }
    }
}
pub(crate) fn color(value: ModelColor) -> Color {
    Color::from_rgb8(value.0[0], value.0[1], value.0[2])
}
type BoxView = Box<AnyWidgetView<UiState>>;
fn text(value: String, style: &Style) -> impl WidgetView<UiState> + use<> {
    label(value)
        .font("DejaVu Sans Mono")
        .text_size(style.font_size as f32)
        .weight(if style.bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        })
        .line_break_mode(LineBreaking::Clip)
        .color(color(style.foreground))
}
fn control(
    value: String,
    style: &Style,
    message: Msg,
    height: f64,
) -> impl WidgetView<UiState, Widget = masonry::widgets::Button> + use<> {
    button(text(value, style), move |state: &mut UiState| {
        state.queue(message.clone())
    })
    .padding(Padding::horizontal(f64::from(style.padding).px()))
    .height(height.px())
    .corner_radius(0.px())
    .border_width(0.px())
}

/// Produce a complete panel view from current state. Dimensions are local preview facts.
pub fn panel_view(model: &Model, width: f64, height: f64) -> impl WidgetView<UiState> + use<> {
    let vertical = matches!(model.config.position, Position::Left | Position::Right);
    let axis = if vertical {
        Axis::Vertical
    } else {
        Axis::Horizontal
    };
    let group_width = if vertical { width } else { width / 3.0 };
    let group_height = if vertical { height / 3.0 } else { height };
    let groups: Vec<BoxView> = [Region::Start, Region::Center, Region::End]
        .into_iter()
        .map(|region| {
            let mut first = true;
            let children: Vec<BoxView> = model
                .config
                .modules
                .iter()
                .enumerate()
                .filter(|(_, config)| config.region == region)
                .filter_map(|(index, config)| {
                    let style = model.theme.resolve(config);
                    let id = model.module_id(index);
                    let view: BoxView = match config.kind {
                        ModuleKind::Workspaces => {
                            let entries: Vec<BoxView> = modules::workspaces(model, config)
                                .into_iter()
                                .map(|(entry, title)| {
                                    let fill = if entry.urgent {
                                        style.urgent
                                    } else if entry.active || entry.visible {
                                        style.selected
                                    } else {
                                        style.background
                                    };
                                    let underline = if entry.active { style.active } else { fill };
                                    let content: BoxView =
                                        if let Some(action) = entry.action.filter(|a| *a != 0) {
                                            control(
                                                title,
                                                &style,
                                                Msg::ActivateWorkspace {
                                                    owner: id,
                                                    epoch: model.workspaces.epoch,
                                                    generation: model.workspaces.generation,
                                                    indicator: entry.id,
                                                    action,
                                                },
                                                f64::from(model.config.height),
                                            )
                                            .boxed()
                                        } else {
                                            sized_box(text(title, &style))
                                                .padding(Padding::horizontal(
                                                    f64::from(style.padding).px(),
                                                ))
                                                .height(f64::from(model.config.height).px())
                                                .boxed()
                                        };
                                    let line = sized_box(label(""))
                                        .dims((Dim::Stretch, 1.px()))
                                        .background_color(color(underline));
                                    zstack((content, line.alignment(UnitPoint::BOTTOM)))
                                        .background_color(color(fill))
                                        .boxed()
                                })
                                .collect();
                            flex(axis, entries).gap(0.px()).boxed()
                        }
                        ModuleKind::Clock => control(
                            model.time.format(&config.format).to_string(),
                            &style,
                            Msg::ToggleCalendar(id),
                            f64::from(model.config.height),
                        )
                        .background_color(color(style.background))
                        .boxed(),
                        ModuleKind::Label => text(config.text.clone(), &style).boxed(),
                        _ => {
                            let observation = if model.fixture {
                                model.fixture_values.get(&config.name)
                            } else {
                                None
                            };
                            if observation.is_none() && config.visible_when_available {
                                return None;
                            }
                            text(
                                observation.cloned().unwrap_or_else(|| {
                                    format!("{} unavailable", config.kind.name())
                                }),
                                &style,
                            )
                            .boxed()
                        }
                    };
                    // Gaps belong to individual modules so kind/instance overrides remain meaningful.
                    let spaced = !first && region == Region::End;
                    first = false;
                    Some(
                        sized_box(view)
                            .padding(if spaced {
                                Padding::left(f64::from(style.gap).px())
                            } else {
                                Padding::ZERO
                            })
                            .background_color(color(style.background))
                            .boxed(),
                    )
                })
                .collect();
            let alignment = match region {
                Region::Start => MainAxisAlignment::Start,
                Region::Center => MainAxisAlignment::Center,
                Region::End => MainAxisAlignment::End,
            };
            portal(
                flex(axis, children)
                    .main_axis_alignment(alignment)
                    .gap(0.px()),
            )
            .constrain_horizontal(true)
            .constrain_vertical(true)
            .must_fill(true)
            .dims((group_width.px(), group_height.px()))
            .boxed()
        })
        .collect();
    flex(axis, groups)
        .gap(0.px())
        .dims((width.px(), height.px()))
        .background_color(color(model.theme.defaults.background))
}

/// Calendar content for the current local popout intent; no allocation is implied.
pub fn calendar_view(model: &Model) -> impl WidgetView<UiState> + use<> {
    let mut rows: Vec<BoxView> = Vec::new();
    if let Some(id) = model.popout.filter(|id| model.owns(*id)) {
        let config = &model.config.modules[id.index];
        let style = model.theme.resolve(config);
        let mut header_style = style.clone();
        header_style.font_size *= 2;
        rows.push(
            text(
                model.time.format(&config.format_popup).to_string(),
                &header_style,
            )
            .boxed(),
        );
        rows.push(
            flex_row((
                control("‹".into(), &style, Msg::CalendarStep(id, -1), 24.0),
                text(model.month.format("%B %Y").to_string(), &style),
                control("›".into(), &style, Msg::CalendarStep(id, 1), 24.0),
                control("×".into(), &style, Msg::Dismiss(id), 24.0),
            ))
            .gap(4.px())
            .boxed(),
        );
        let mut headings: Vec<BoxView> = Vec::new();
        if config.show_week_numbers {
            headings.push(sized_box(text("Wk".into(), &style)).width(30.px()).boxed());
        }
        for name in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
            headings.push(sized_box(text(name.into(), &style)).width(30.px()).boxed());
        }
        rows.push(flex_row(headings).gap(0.px()).boxed());
        for week in modules::calendar(model.month).chunks(7) {
            let mut cells: Vec<BoxView> = Vec::new();
            if config.show_week_numbers {
                cells.push(
                    sized_box(text(
                        week[0].map_or(String::new(), |d| d.iso_week().week().to_string()),
                        &style,
                    ))
                    .dims((30.px(), 24.px()))
                    .boxed(),
                );
            }
            for date in week {
                let shown = date.filter(|date| date.month() == model.month.month());
                let today = shown == Some(model.time.date_naive());
                cells.push(
                    sized_box(text(
                        shown.map_or(String::new(), |date| date.day().to_string()),
                        &style,
                    ))
                    .dims((30.px(), 24.px()))
                    .background_color(color(if today {
                        style.active
                    } else {
                        style.background
                    }))
                    .boxed(),
                );
            }
            rows.push(flex_row(cells).gap(0.px()).boxed());
        }
    }
    flex_col(rows)
        .gap(4.px())
        .padding(10.px())
        .background_color(color(model.theme.defaults.background))
}
