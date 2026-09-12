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
