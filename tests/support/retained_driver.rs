use super::*;

fn ids(
    widget: WidgetRef<'_, dyn masonry::core::Widget>,
    result: &mut Vec<masonry::core::WidgetId>,
) {
    result.push(widget.id());
    for child in widget.children() {
        ids(child, result);
    }
}

#[test]
fn clock_reconciliation_keeps_the_existing_root_and_widget_identities() {
    let (config, theme) =
        crate::config::parse_shell_config(include_str!("../../examples/minimal/shell.kdl"))
            .unwrap();
    let model = Model::new(
        config,
        theme,
        1,
        chrono::DateTime::<chrono::Utc>::UNIX_EPOCH.fixed_offset(),
    );
    let mut driver = PreviewDriver::new(model.clone(), 800, 48, 1.0, false).unwrap();
    driver.scene();
    let mut before = Vec::new();
    ids(driver.root.get_layer_root(0), &mut before);
    let mut changed = model;
    changed.time += chrono::Duration::seconds(1);
    driver.reconcile_model(changed.clone());
    driver.scene();
    let mut after = Vec::new();
    ids(driver.root.get_layer_root(0), &mut after);
    assert_eq!(after, before);
    assert_eq!(driver.model().time, changed.time);
}
