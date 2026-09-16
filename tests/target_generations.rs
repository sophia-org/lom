//! Passive button-lifetime controls, independent of display and GPU state.

use lom::{config::parse_shell_config, model::Model, ui::ContentTargetLayout, update::Msg};

// Compile the private passive target registry without exposing a test API.
#[path = "../src/service/target_generations.rs"]
mod target_generations;
use lom::{model, ui, update};

#[test]
fn target_lifetimes_ignore_publication_but_not_semantics_or_removal() {
    let (config, theme) =
        parse_shell_config(include_str!("../examples/minimal/shell.kdl")).unwrap();
    let mut model = Model::new(
        config,
        theme,
        1,
        "2026-09-16T00:00:00+00:00".parse().unwrap(),
    );
    model.epoch = 1;
    model.workspaces = lom::model::WorkspaceSnapshot {
        epoch: 1,
        generation: 1,
        active_output: Some(1),
        entries: vec![lom::model::Workspace {
            id: 1,
            output: 1,
            name: "one".into(),
            active: false,
            visible: false,
            urgent: false,
            action: Some(10),
        }],
    };
    let mut registry = target_generations::TargetGenerations::default();
    let make = |m: &Model| ContentTargetLayout {
        target_generation: 0,
        message: Msg::ActivateWorkspace {
            owner: m.module_id(0),
            epoch: m.workspaces.epoch,
            generation: m.workspaces.generation,
            indicator: m.workspaces.entries[0].id,
            action: m.workspaces.entries[0].action.unwrap(),
        },
        indicator: m.workspaces.entries[0].id,
        action: m.workspaces.entries[0].action.unwrap(),
        generation: m.workspaces.generation,
        x: 0,
        y: 0,
        width: 20,
        height: 20,
    };
    let mut targets = vec![make(&model)];
    registry.prepare(&model, &mut targets, 64).unwrap();
    let first = targets[0].target_generation;
    model.workspaces.generation += 20;
    let mut targets = vec![make(&model)];
    registry.prepare(&model, &mut targets, 64).unwrap();
    assert_eq!(targets[0].target_generation, first);
    model.workspaces.entries[0].name.push('x');
    registry.prepare(&model, &mut targets, 64).unwrap();
    let renamed = targets[0].target_generation;
    assert!(renamed > first);
    registry.prepare(&model, &mut [], 64).unwrap();
    registry.prepare(&model, &mut targets, 64).unwrap();
    assert!(targets[0].target_generation > renamed);
    let restored = targets[0].target_generation;
    targets[0].x += 1;
    registry.prepare(&model, &mut targets, 64).unwrap();
    assert!(targets[0].target_generation > restored);
    assert!(registry.prepare(&model, &mut targets, 0).is_err());
}
