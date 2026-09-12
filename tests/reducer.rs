//! Offline regression evidence for the Minimal port.

mod support;
use chrono::{DateTime, Datelike, NaiveDate};
use lom::{
    model::ModuleId,
    modules::{calendar, workspaces},
    update::{Effect, Msg, update},
};

#[test]
fn calendar_open_navigate_tick_dismiss_and_reopen() {
    let mut model = support::model();
    let id = model.module_id(5);
    assert_eq!(
        update(&mut model, Msg::ToggleCalendar(id)),
        vec![Effect::RequestPopout(id)]
    );
    update(&mut model, Msg::CalendarStep(id, 1));
    assert_eq!(model.month.month(), 10);
    update(
        &mut model,
        Msg::Time(
            id,
            DateTime::parse_from_rfc3339("2026-09-13T00:00:00Z").unwrap(),
        ),
    );
    assert_eq!(
        model.month.month(),
        10,
        "tick must not reset calendar navigation"
    );
    assert_eq!(
        update(&mut model, Msg::Dismiss(id)),
        vec![Effect::WithdrawPopout(id)]
    );
    assert!(update(&mut model, Msg::Dismiss(id)).is_empty());
    update(&mut model, Msg::ToggleCalendar(id));
    assert_eq!(model.month.month(), 9);
    update(&mut model, Msg::PopoutRejected(id));
    assert_eq!(model.popout, None);
}

#[test]
fn calendar_handles_leap_year_and_year_rollover() {
    let february = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
    let days = calendar(february);
    assert_eq!(days.len(), 42);
    assert_eq!(days[0].unwrap().weekday().num_days_from_monday(), 0);
    assert_eq!(
        days.iter().flatten().filter(|day| day.month() == 2).count(),
        29
    );
    let mut model = support::model();
    let id = model.module_id(5);
    update(&mut model, Msg::ToggleCalendar(id));
    model.month = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
    update(&mut model, Msg::CalendarStep(id, 1));
    assert_eq!(model.month, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    update(&mut model, Msg::CalendarStep(id, -1));
    assert_eq!(model.month.year(), 2026);
}

#[test]
fn stale_module_results_cannot_update_a_replacement() {
    let mut model = support::model();
    let id = model.module_id(5);
    let config = model.config.clone();
    update(&mut model, Msg::ToggleCalendar(id));
    assert_eq!(
        update(&mut model, Msg::ReplaceConfiguration(config)),
        vec![Effect::WithdrawPopout(id)]
    );
    let before = model.clone();
    for msg in [
        Msg::ToggleCalendar(id),
        Msg::Time(
            id,
            DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap(),
        ),
        Msg::Dismiss(id),
        Msg::CalendarStep(id, -1),
    ] {
        assert!(update(&mut model, msg).is_empty());
    }
    assert_eq!(model, before);
    let new_id = model.module_id(5);
    assert_ne!(new_id, id);
    assert_eq!(
        update(&mut model, Msg::ToggleCalendar(new_id)),
        vec![Effect::RequestPopout(new_id)]
    );
}

#[test]
fn retained_workspace_message_never_retargets_a_reordered_entry() {
    let mut model = support::model();
    let owner = model.module_id(0);
    let activation = Msg::ActivateWorkspace {
        owner,
        epoch: 1,
        generation: 1,
        indicator: 1,
        action: 11,
    };
    assert_eq!(
        update(&mut model, activation.clone()),
        vec![Effect::ActivateWorkspace {
            epoch: 1,
            generation: 1,
            indicator: 1,
            action: 11
        }]
    );
    model.workspaces.entries.reverse();
    assert_eq!(update(&mut model, activation.clone()).len(), 1);
    model
        .workspaces
        .entries
        .iter_mut()
        .find(|entry| entry.id == 1)
        .unwrap()
        .action = None;
    assert!(update(&mut model, activation.clone()).is_empty());
    update(&mut model, Msg::Disconnected(1));
    assert!(update(&mut model, activation.clone()).is_empty());
    update(&mut model, Msg::Connected(2));
    assert!(update(&mut model, activation).is_empty());
}

#[test]
fn snapshot_ordering_and_disconnect_reject_old_publications() {
    let mut model = support::model();
    let old = model.workspaces.clone();
    let mut newer = old.clone();
    newer.generation += 1;
    newer.active_output = Some(2);
    update(&mut model, Msg::Workspaces(newer.clone()));
    update(&mut model, Msg::Workspaces(old));
    assert_eq!(model.workspaces, newer);
    update(&mut model, Msg::Disconnected(1));
    update(&mut model, Msg::Workspaces(newer));
    assert!(model.workspaces.entries.is_empty());
    update(&mut model, Msg::Connected(1));
    assert_eq!(model.workspaces.epoch, 0);
}

#[test]
fn reducer_replay_is_deterministic_and_wrong_module_actions_are_inert() {
    let mut a = support::model();
    let mut b = a.clone();
    let id = a.module_id(5);
    for msg in [
        Msg::ToggleCalendar(id),
        Msg::CalendarStep(id, -1),
        Msg::CalendarStep(id, -1),
        Msg::Dismiss(id),
        Msg::ToggleCalendar(ModuleId {
            index: 0,
            generation: 1,
        }),
    ] {
        assert_eq!(update(&mut a, msg.clone()), update(&mut b, msg));
        assert_eq!(a, b);
    }
    assert_eq!(a.popout, None);
}

#[test]
fn workspace_name_sorting_mapping_and_output_filtering() {
    let mut model = support::model();
    model.workspaces.entries[0].name = "10".into();
    model.workspaces.entries[1].name = "2".into();
    model.workspaces.entries[2].output = 2;
    let mut config = model.config.modules[0].clone();
    config.hidden.push("4".into());
    config.names.insert("10".into(), "web".into());
    assert_eq!(
        workspaces(&model, &config)
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<_>>(),
        ["2", "web"]
    );
    config.all_monitors = true;
    assert_eq!(workspaces(&model, &config).len(), 3);
}
