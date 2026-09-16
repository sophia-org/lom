//! Pure module projections shared by views and behavioral tests.

use crate::{
    config::ModuleConfig,
    model::{Model, Workspace},
};
use chrono::{Datelike, Days, NaiveDate};

/// Return name-sorted workspace entries after this module's filtering.
pub fn workspaces<'a>(model: &'a Model, config: &ModuleConfig) -> Vec<(&'a Workspace, String)> {
    let mut entries: Vec<_> = model
        .workspaces
        .entries
        .iter()
        .filter(|entry| {
            (config.all_monitors || entry.output == model.output)
                && !config.hidden.contains(&entry.name)
        })
        .map(|entry| {
            (
                entry,
                config.names.get(&entry.name).unwrap_or(&entry.name).clone(),
            )
        })
        .collect();
    entries.sort_by(
        |(left, a), (right, b)| match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b).then(left.id.cmp(&right.id)),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            _ => a.cmp(b).then(left.id.cmp(&right.id)),
        },
    );
    entries
}

/// Six Monday-first calendar rows; adjacent-month dates are retained for layout.
pub fn calendar(month: NaiveDate) -> Vec<Option<NaiveDate>> {
    let first = month.with_day(1).expect("first day exists");
    let start =
        first.checked_sub_days(Days::new(u64::from(first.weekday().num_days_from_monday())));
    (0..42)
        .map(|offset| start.and_then(|date| date.checked_add_days(Days::new(offset))))
        .collect()
}

/// Workspace appearance, shared by the view and visual invalidation.
pub fn workspace_colors(
    entry: &Workspace,
    style: &crate::config::Style,
) -> (crate::config::Color, crate::config::Color) {
    let fill = if entry.urgent {
        style.urgent
    } else if entry.active || entry.visible {
        style.selected
    } else {
        style.background
    };
    (fill, if entry.active { style.active } else { fill })
}

/// Compare only panel-visible inputs; publication identities are interaction
/// state. Module filtering, labels and colors are the same projections as UI.
pub fn same_panel_pixels(left: &Model, right: &Model) -> bool {
    use crate::config::ModuleKind;
    if left.config != right.config
        || left.theme != right.theme
        || left.generation != right.generation
        || left.output != right.output
        || left.epoch != right.epoch
        || left.popout != right.popout
        || left.month != right.month
        || left.fixture != right.fixture
        || left.fixture_values != right.fixture_values
    {
        return false;
    }
    left.config
        .modules
        .iter()
        .enumerate()
        .all(|(index, config)| match config.kind {
            ModuleKind::Workspaces => {
                let style = left.theme.resolve(config);
                let project = |model: &Model| {
                    workspaces(model, config)
                        .into_iter()
                        .map(|(entry, label)| {
                            (
                                label,
                                workspace_colors(entry, &style),
                                entry.action.is_some_and(|id| id != 0),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                project(left) == project(right)
            }
            ModuleKind::Clock => {
                left.time.format(&config.format).to_string()
                    == right.time.format(&config.format).to_string()
                    && (left.popout != Some(left.module_id(index))
                        || (left.time.date_naive() == right.time.date_naive()
                            && left.time.format(&config.format_popup).to_string()
                                == right.time.format(&config.format_popup).to_string()))
            }
            _ => true,
        })
}
