use super::*;

fn model() -> Model {
    crate::runtime::parse_fixture(
        include_str!("../../examples/minimal/fixture.kdl"),
        crate::config::parse_config(include_str!("../../examples/minimal/config.kdl")).unwrap(),
        crate::config::parse_theme(include_str!("../../examples/minimal/theme.kdl")).unwrap(),
    )
    .unwrap()
}

#[test]
fn a_clock_tick_dirties_only_outputs_whose_visible_text_changes() {
    let mut minute = model();
    minute.time = DateTime::parse_from_rfc3339("2026-09-14T10:00:00Z").unwrap();
    for module in &mut minute.config.modules {
        if module.kind == ModuleKind::Clock {
            module.format = "%H:%M".into();
        }
    }
    let mut second = minute.clone();
    for module in &mut second.config.modules {
        if module.kind == ModuleKind::Clock {
            module.format = "%H:%M:%S".into();
        }
    }
    let mut no_clock = minute.clone();
    no_clock
        .config
        .modules
        .retain(|module| module.kind != ModuleKind::Clock);
    let now = DateTime::parse_from_rfc3339("2026-09-14T10:00:01Z").unwrap();
    assert!(!observe_clock(&mut minute, now));
    assert!(observe_clock(&mut second, now));
    assert!(!observe_clock(&mut no_clock, now));
    let next_minute = DateTime::parse_from_rfc3339("2026-09-14T10:01:00Z").unwrap();
    assert!(observe_clock(&mut minute, next_minute));
    assert!(!observe_clock(&mut minute, next_minute));
}
