use crate::{
    config::{
        ConfigError, ModuleKind, PanelConfig, Theme,
        parse::{args, boolean, children, document, error, leaf, number, string, unique},
    },
    model::{Model, Workspace, WorkspaceSnapshot},
    update::{Msg, update},
};
use chrono::DateTime;
use std::collections::{BTreeMap, BTreeSet};

/// Parse an explicitly diagnostic KDL observation set. Never contacts a data source.
pub fn parse_fixture(
    source: &str,
    config: PanelConfig,
    theme: Theme,
) -> Result<Model, ConfigError> {
    let doc = document(source)?;
    let mut seen = BTreeSet::new();
    let mut values = BTreeMap::new();
    let mut entries = Vec::new();
    let mut output = None;
    let mut active_output = None;
    let mut time = None;
    let mut ids = BTreeSet::new();
    for node in doc.nodes() {
        match node.name().value() {
            "workspace" => {
                let vals = args(source, node, 3)?;
                let id = vals[0]
                    .as_integer()
                    .filter(|v| (1..=u64::MAX as i128).contains(v))
                    .ok_or_else(|| error(source, node, "expected positive indicator id"))?
                    as u64;
                let output = vals[1]
                    .as_integer()
                    .filter(|v| (1..=u64::MAX as i128).contains(v))
                    .ok_or_else(|| error(source, node, "expected positive output id"))?
                    as u64;
                let name = vals[2]
                    .as_string()
                    .ok_or_else(|| error(source, node, "expected workspace name"))?
                    .to_owned();
                if !ids.insert(id) || ids.len() > 256 {
                    return Err(error(
                        source,
                        node,
                        "duplicate indicator or more than 256 entries",
                    ));
                }
                let mut entry = Workspace {
                    id,
                    output,
                    name,
                    active: false,
                    visible: false,
                    urgent: false,
                    action: None,
                };
                let mut fields = BTreeSet::new();
                for child in children(source, node)? {
                    unique(source, child, &mut fields)?;
                    match child.name().value() {
                        "active" => entry.active = boolean(source, child)?,
                        "visible" => entry.visible = boolean(source, child)?,
                        "urgent" => entry.urgent = boolean(source, child)?,
                        "action" => {
                            entry.action = Some(number(source, child, 1, u64::MAX as i128)? as u64)
                        }
                        _ => return Err(error(source, child, "unknown indicator fixture field")),
                    }
                }
                entries.push(entry);
            }
            "value" => {
                leaf(source, node)?;
                let vals = args(source, node, 2)?;
                let name = vals[0]
                    .as_string()
                    .ok_or_else(|| error(source, node, "expected module name"))?;
                let text = vals[1]
                    .as_string()
                    .ok_or_else(|| error(source, node, "expected fixture text"))?;
                if !config.modules.iter().any(|m| {
                    m.name == name
                        && matches!(
                            m.kind,
                            ModuleKind::Focused
                                | ModuleKind::Battery
                                | ModuleKind::SysInfo
                                | ModuleKind::Tray
                        )
                }) {
                    return Err(error(
                        source,
                        node,
                        "fixture value must name a configured data presentation",
                    ));
                }
                if values.insert(name.to_owned(), text.to_owned()).is_some() {
                    return Err(error(source, node, "duplicate fixture value"));
                }
            }
            key => {
                unique(source, node, &mut seen)?;
                match key {
                    "version" => {
                        number(source, node, 1, 1)?;
                    }
                    "output" => output = Some(number(source, node, 1, u64::MAX as i128)? as u64),
                    "active-output" => {
                        active_output = Some(number(source, node, 1, u64::MAX as i128)? as u64)
                    }
                    "time" => {
                        time = Some(
                            DateTime::parse_from_rfc3339(&string(source, node)?).map_err(|_| {
                                error(source, node, "expected RFC3339 time with explicit offset")
                            })?,
                        )
                    }
                    _ => return Err(error(source, node, "unknown fixture node")),
                }
            }
        }
    }
    let missing = || ConfigError {
        message: "fixture requires version 1, output and time".into(),
        line: 1,
        column: 1,
    };
    if !seen.contains("version") {
        return Err(missing());
    }
    let mut model = Model::new(
        config,
        theme,
        output.ok_or_else(missing)?,
        time.ok_or_else(missing)?,
    );
    model.fixture = true;
    model.fixture_values = values;
    update(&mut model, Msg::Connected(1));
    update(
        &mut model,
        Msg::Workspaces(WorkspaceSnapshot {
            epoch: 1,
            generation: 1,
            active_output,
            entries,
        }),
    );
    Ok(model)
}
