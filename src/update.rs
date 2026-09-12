//! Deterministic TEA transitions and correlated effect descriptions.

use crate::{
    config::ModuleKind,
    model::{Model, ModuleId, WorkspaceSnapshot},
};
use chrono::{DateTime, Datelike, FixedOffset, Months};

/// Semantic input from an adapter or retained action binding.
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// Replace all configured module owners; delayed results cannot target replacements.
    ReplaceConfiguration(crate::config::PanelConfig),
    /// Allocation was refused or the parent was withdrawn; clears matching local intent.
    PopoutRejected(ModuleId),
    /// Establish a new source epoch; clears stale source observations.
    Connected(u64),
    /// Withdraw source and dependent interaction rights.
    Disconnected(u64),
    /// Complete indicator publication.
    Workspaces(WorkspaceSnapshot),
    /// Clock observation for the current owner.
    Time(ModuleId, DateTime<FixedOffset>),
    /// Toggle local calendar intent.
    ToggleCalendar(ModuleId),
    /// Previous/next month, with the originating module identity.
    CalendarStep(ModuleId, i32),
    /// Outside dismissal or explicit close, already target-resolved by its caller.
    Dismiss(ModuleId),
    /// Activation bound to the exact observed publication, never a widget index.
    ActivateWorkspace {
        /// Module owner.
        owner: ModuleId,
        /// Connection epoch.
        epoch: u64,
        /// Publication generation.
        generation: u64,
        /// Indicator identity.
        indicator: u64,
        /// Granted action.
        action: u64,
    },
}

/// Side effects for a driver to execute; reducer never performs them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Request an anchored calendar allocation; does not establish visibility.
    RequestPopout(ModuleId),
    /// Withdraw an existing/desired calendar allocation.
    WithdrawPopout(ModuleId),
    /// Authorized semantic indicator activation, revalidated by the real driver.
    ActivateWorkspace {
        /// Connection epoch.
        epoch: u64,
        /// Complete snapshot generation.
        generation: u64,
        /// Indicator identity.
        indicator: u64,
        /// Action identity.
        action: u64,
    },
}

/// Reduce one ordered message. Rejected stale work produces no effect.
pub fn update(model: &mut Model, message: Msg) -> Vec<Effect> {
    match message {
        Msg::ReplaceConfiguration(config) => {
            let Some(generation) = model.generation.checked_add(1) else {
                return Vec::new();
            };
            let effects = close(model);
            model.generation = generation;
            model.config = config;
            model.fixture_values.clear();
            effects
        }
        Msg::PopoutRejected(id) if model.popout == Some(id) && model.owns(id) => {
            model.popout = None;
            Vec::new()
        }
        Msg::Connected(epoch) if epoch != 0 && epoch > model.epoch => {
            let effects = close(model);
            model.epoch = epoch;
            model.workspaces = WorkspaceSnapshot {
                epoch,
                ..WorkspaceSnapshot::default()
            };
            effects
        }
        Msg::Disconnected(epoch) if epoch == model.epoch => {
            let effects = close(model);
            model.workspaces = WorkspaceSnapshot::default();
            // Retain the high-water epoch so an old connection cannot reconnect.
            effects
        }
        Msg::Workspaces(snapshot)
            if snapshot.epoch == model.epoch
                && snapshot.epoch != 0
                && model.workspaces.epoch == snapshot.epoch
                && snapshot.generation > model.workspaces.generation =>
        {
            let mut ids = std::collections::BTreeSet::new();
            if snapshot.entries.len() <= 256
                && snapshot.entries.iter().all(|entry| ids.insert(entry.id))
            {
                model.workspaces = snapshot;
            }
            Vec::new()
        }
        Msg::Time(id, time) if is_kind(model, id, ModuleKind::Clock) => {
            model.time = time;
            Vec::new()
        }
        Msg::ToggleCalendar(id) if is_kind(model, id, ModuleKind::Clock) => {
            if model.popout == Some(id) {
                return close(model);
            }
            let mut effects = close(model);
            model.month = model
                .time
                .date_naive()
                .with_day(1)
                .expect("first day exists");
            model.popout = Some(id);
            effects.push(Effect::RequestPopout(id));
            effects
        }
        Msg::Dismiss(id) if model.owns(id) && model.popout == Some(id) => close(model),
        Msg::CalendarStep(id, step)
            if model.owns(id) && model.popout == Some(id) && matches!(step, -1 | 1) =>
        {
            let next = if step < 0 {
                model.month.checked_sub_months(Months::new(1))
            } else {
                model.month.checked_add_months(Months::new(1))
            };
            if let Some(next) = next {
                model.month = next;
            }
            Vec::new()
        }
        Msg::ActivateWorkspace {
            owner,
            epoch,
            generation,
            indicator,
            action,
        } if is_kind(model, owner, ModuleKind::Workspaces) => {
            let snapshot = &model.workspaces;
            if epoch != 0
                && epoch == snapshot.epoch
                && generation == snapshot.generation
                && snapshot.entries.iter().any(|entry| {
                    entry.id == indicator && entry.action == Some(action) && action != 0
                })
            {
                vec![Effect::ActivateWorkspace {
                    epoch,
                    generation,
                    indicator,
                    action,
                }]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}
fn is_kind(model: &Model, id: ModuleId, kind: ModuleKind) -> bool {
    model.owns(id) && model.config.modules[id.index].kind == kind
}
fn close(model: &mut Model) -> Vec<Effect> {
    model
        .popout
        .take()
        .map(Effect::WithdrawPopout)
        .into_iter()
        .collect()
}
