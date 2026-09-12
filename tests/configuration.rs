//! Offline regression evidence for the Minimal port.

mod support;

use lom::{
    config::{Color, ModuleKind, Position, parse_config, parse_theme},
    runtime::{parse_fixture, validate_theme},
};

#[test]
fn minimal_preserves_arrangement_and_theme() {
    let model = support::model();
    assert_eq!(model.config.height, 24);
    assert_eq!(model.config.position, Position::Top);
    assert_eq!(
        model
            .config
            .modules
            .iter()
            .map(|m| m.kind)
            .collect::<Vec<_>>(),
        [
            ModuleKind::Workspaces,
            ModuleKind::Focused,
            ModuleKind::Battery,
            ModuleKind::SysInfo,
            ModuleKind::Tray,
            ModuleKind::Clock
        ]
    );
    assert_eq!(model.theme.defaults.background, Color([28; 3]));
    assert_eq!(model.theme.defaults.active, Color([102, 153, 204]));
    assert!(model.theme.resolve(&model.config.modules[5]).bold);
    assert!(model.fixture);
}

#[test]
fn defaults_match_ironbar_and_signed_margins_are_logical() {
    let config = parse_config("version 1\npanel \"main\" {\n margin -2 3 4 5\n}").unwrap();
    assert_eq!(config.height, 42);
    assert_eq!(config.position, Position::Bottom);
    assert_eq!(config.popup_gap, 5);
    assert_eq!(config.margins, [-2, 3, 4, 5]);
    assert!(config.modules.is_empty());
}

#[test]
fn invalid_kdl_values_and_unknown_features_are_not_ignored() {
    for inner in [
        "height 0",
        "height 513",
        "height 2.5",
        "height 24\nheight 25",
        "position \"floating\"",
        "margin 0 0 0 513",
        "autohide 200",
        "height size=24",
        "(number)height 24",
        "height 24 { bad; }",
    ] {
        let source = format!("version 1\npanel \"main\" {{\n{inner}\n}}");
        let error = parse_config(&source).expect_err(inner);
        assert_eq!(
            error.line,
            if inner.contains("\nheight") { 4 } else { 3 },
            "{inner}: {error}"
        );
        assert!(error.column > 0);
    }
    for source in [
        "version 2\npanel \"x\" {}",
        "panel \"x\" {}",
        "version 1\npanel \"x\" {}\nextra",
        "version 1\npanel \"x\" {\n",
    ] {
        assert!(parse_config(source).is_err(), "{source}");
    }
}

#[test]
fn module_owners_and_fields_are_validated() {
    for source in [
        "version 1\npanel \"x\" { start { label \"x\"; clock \"x\"; }; }",
        "version 1\npanel \"x\" { start { clock \"x\" { format \"%Q\"; }; }; }",
        "version 1\npanel \"x\" { start { battery \"x\" { show-if \"rm file\"; }; }; }",
        "version 1\npanel \"x\" { start { label \"x\" { format \"%Y\"; }; }; }",
        "version 1\npanel \"x\" { start { clock \"x\" { show-week-numbers \"true\"; }; }; }",
    ] {
        assert!(parse_config(source).is_err(), "{source}");
    }
}

#[test]
fn override_precedence_is_independent_of_document_order() {
    let config = parse_config(support::CONFIG).unwrap();
    let theme = parse_theme("version 1\ntheme \"custom\" {\n instance \"clock\" { font-size 19; bold #false; }\n module \"clock\" { font-size 16; bold #true; }\n defaults { font-size 12; }\n}").unwrap();
    let style = theme.resolve(&config.modules[5]);
    assert_eq!(style.font_size, 19);
    assert!(!style.bold);
    assert_eq!(theme.resolve(&config.modules[0]).font_size, 12);
    validate_theme(&config, &theme).unwrap();
}

#[test]
fn theme_typos_and_unbound_instances_fail() {
    for inner in [
        "defaults { background \"#xyzxyz\"; }",
        "defaults { background \"#fff\"; }",
        "defaults { radius 5; }",
        "module \"unknown\" {}",
        "defaults {}\ndefaults {}",
        "instance \"x\" {}\ninstance \"x\" {}",
    ] {
        assert!(
            parse_theme(&format!("version 1\ntheme \"t\" {{\n{inner}\n}}")).is_err(),
            "{inner}"
        );
    }
    let theme = parse_theme("version 1\ntheme \"t\" { instance \"typo\" {}; }").unwrap();
    assert!(validate_theme(&parse_config(support::CONFIG).unwrap(), &theme).is_err());
}

#[test]
fn unicode_and_fixture_absence_are_preserved() {
    let config = parse_config(
        "version 1\npanel \"лом\" { start { label \"日本語\" { text \"Привет 👋\"; }; }; }",
    )
    .unwrap();
    assert_eq!(config.modules[0].text, "Привет 👋");
    for bad in [
        "value \"typo\" \"x\"",
        "workspace 1 1 \"duplicate\" {}",
        "time \"bad\"",
        "extra 4",
    ] {
        let source = format!("{}\n{bad}", support::FIXTURE);
        assert!(
            parse_fixture(
                &source,
                parse_config(support::CONFIG).unwrap(),
                parse_theme(support::THEME).unwrap()
            )
            .is_err(),
            "{bad}"
        );
    }
    let model = parse_fixture(
        "version 1\noutput 1\nactive-output 2\ntime \"2026-09-12T00:00:00Z\"",
        parse_config(support::CONFIG).unwrap(),
        parse_theme(support::THEME).unwrap(),
    )
    .unwrap();
    assert_eq!(model.workspaces.active_output, Some(2));
    assert!(model.workspaces.entries.is_empty());
    assert!(model.fixture_values.is_empty());
}

#[test]
fn parsing_only_clock_formats_are_rejected_before_view_formatting() {
    let source = "version 1\npanel \"x\" { end { clock \"time\" { format \"%#z\"; }; }; }";
    assert!(parse_config(source).is_err());
}
