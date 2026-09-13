use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write as _;

use kdl::{KdlDocument, KdlNode, KdlValue};

use super::{ModuleConfig, ModuleKind, PanelConfig, Position, Region};

/// Configuration error with one-based source position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// Human-readable boundary failure.
    pub message: String,
    /// One-based line.
    pub line: usize,
    /// One-based column.
    pub column: usize,
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}
impl std::error::Error for ConfigError {}

pub(crate) fn error(source: &str, node: &KdlNode, message: impl Into<String>) -> ConfigError {
    at(source, node.span().offset(), message)
}
fn at(source: &str, offset: usize, message: impl Into<String>) -> ConfigError {
    let prefix = &source[..offset.min(source.len())];
    ConfigError {
        message: message.into(),
        line: prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
        column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
    }
}
pub(crate) fn document(source: &str) -> Result<KdlDocument, ConfigError> {
    if source.len() > 256 * 1024 {
        return Err(at(source, 0, "document exceeds 256 KiB"));
    }
    source.parse::<KdlDocument>().map_err(|err| {
        at(
            source,
            err.diagnostics.first().map_or(0, |d| d.span.offset()),
            err.to_string(),
        )
    })
}
pub(crate) fn args<'a>(
    source: &str,
    node: &'a KdlNode,
    count: usize,
) -> Result<Vec<&'a KdlValue>, ConfigError> {
    if node.ty().is_some()
        || node.entries().len() != count
        || node
            .entries()
            .iter()
            .any(|e| e.name().is_some() || e.ty().is_some())
    {
        return Err(error(
            source,
            node,
            format!(
                "{} expects {count} positional arguments without type annotations",
                node.name().value()
            ),
        ));
    }
    Ok(node.entries().iter().map(|e| e.value()).collect())
}
pub(crate) fn leaf(source: &str, node: &KdlNode) -> Result<(), ConfigError> {
    if node.children().is_some() {
        return Err(error(source, node, "unexpected child nodes"));
    }
    Ok(())
}
pub(crate) fn string(source: &str, node: &KdlNode) -> Result<String, ConfigError> {
    leaf(source, node)?;
    args(source, node, 1)?[0]
        .as_string()
        .map(str::to_owned)
        .ok_or_else(|| error(source, node, "expected string"))
}
pub(crate) fn number(
    source: &str,
    node: &KdlNode,
    min: i128,
    max: i128,
) -> Result<i128, ConfigError> {
    leaf(source, node)?;
    args(source, node, 1)?[0]
        .as_integer()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| error(source, node, format!("expected integer in {min}..={max}")))
}
pub(crate) fn boolean(source: &str, node: &KdlNode) -> Result<bool, ConfigError> {
    leaf(source, node)?;
    args(source, node, 1)?[0]
        .as_bool()
        .ok_or_else(|| error(source, node, "expected #true or #false"))
}
pub(crate) fn children<'a>(source: &str, node: &'a KdlNode) -> Result<&'a [KdlNode], ConfigError> {
    node.children()
        .map(|d| d.nodes())
        .ok_or_else(|| error(source, node, "expected child block"))
}
pub(crate) fn unique(
    source: &str,
    node: &KdlNode,
    seen: &mut BTreeSet<String>,
) -> Result<(), ConfigError> {
    if !seen.insert(node.name().value().into()) {
        return Err(error(source, node, "duplicate setting"));
    }
    Ok(())
}

/// Parse the version-1 panel document using KDL v2.
pub fn parse_config(source: &str) -> Result<PanelConfig, ConfigError> {
    let doc = document(source)?;
    if doc.nodes().len() != 2
        || doc.nodes()[0].name().value() != "version"
        || number(source, &doc.nodes()[0], 1, 1)? != 1
        || doc.nodes()[1].name().value() != "panel"
    {
        return Err(at(source, 0, "expected version 1 followed by one panel"));
    }
    parse_panel(source, &doc.nodes()[1])
}

pub(crate) fn parse_panel(source: &str, node: &KdlNode) -> Result<PanelConfig, ConfigError> {
    let name = args(source, node, 1)?[0]
        .as_string()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error(source, node, "expected nonempty panel name"))?
        .to_owned();
    let mut panel = PanelConfig {
        name,
        position: Position::Bottom,
        height: 42,
        margins: [0; 4],
        popup_gap: 5,
        modules: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    let mut names = BTreeSet::new();
    for child in children(source, node)? {
        unique(source, child, &mut seen)?;
        match child.name().value() {
            "position" => {
                panel.position = match string(source, child)?.as_str() {
                    "top" => Position::Top,
                    "bottom" => Position::Bottom,
                    "left" => Position::Left,
                    "right" => Position::Right,
                    _ => return Err(error(source, child, "expected top, bottom, left or right")),
                }
            }
            "height" => panel.height = number(source, child, 1, 512)? as u32,
            "popup-gap" => panel.popup_gap = number(source, child, 0, 512)? as u32,
            "margin" => {
                leaf(source, child)?;
                for (index, value) in args(source, child, 4)?.iter().enumerate() {
                    panel.margins[index] = value
                        .as_integer()
                        .filter(|v| (-512..=512).contains(v))
                        .ok_or_else(|| {
                        error(source, child, "margins must be integers in -512..=512")
                    })? as i32;
                }
            }
            "start" | "center" | "end" => {
                args(source, child, 0)?;
                let region = match child.name().value() {
                    "start" => Region::Start,
                    "center" => Region::Center,
                    _ => Region::End,
                };
                for module in children(source, child)? {
                    let config = parse_module(source, module, region)?;
                    if !names.insert(config.name.clone()) {
                        return Err(error(source, module, "duplicate module identity"));
                    }
                    panel.modules.push(config);
                    if panel.modules.len() > 64 {
                        return Err(error(source, module, "at most 64 modules are supported"));
                    }
                }
            }
            _ => return Err(error(source, child, "unknown or unsupported panel setting")),
        }
    }
    Ok(panel)
}

fn parse_module(source: &str, node: &KdlNode, region: Region) -> Result<ModuleConfig, ConfigError> {
    let kind = ModuleKind::parse(node.name().value())
        .ok_or_else(|| error(source, node, "unsupported module type"))?;
    let name = args(source, node, 1)?[0]
        .as_string()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error(source, node, "expected nonempty module name"))?
        .to_owned();
    let mut config = ModuleConfig {
        name,
        kind,
        region,
        text: String::new(),
        format: "%d/%m/%Y %H:%M".into(),
        format_popup: "%H:%M:%S".into(),
        show_week_numbers: false,
        names: BTreeMap::new(),
        hidden: Vec::new(),
        all_monitors: false,
        visible_when_available: false,
    };
    let mut seen = BTreeSet::new();
    for child in node.children().map_or(&[][..], |d| d.nodes()) {
        unique(source, child, &mut seen)?;
        match (kind, child.name().value()) {
            (ModuleKind::Label, "text") => config.text = string(source, child)?,
            (ModuleKind::Clock, "format" | "format-popup") => {
                let value = string(source, child)?;
                let sample = chrono::DateTime::parse_from_rfc3339("2026-09-12T12:30:00Z")
                    .expect("fixed validation timestamp is valid");
                let mut formatted = String::new();
                if chrono::format::StrftimeItems::new(&value)
                    .any(|item| item == chrono::format::Item::Error)
                    || write!(&mut formatted, "{}", sample.format(&value)).is_err()
                {
                    return Err(error(source, child, "invalid clock format"));
                }
                if child.name().value() == "format" {
                    config.format = value;
                } else {
                    config.format_popup = value;
                }
            }
            (ModuleKind::Clock, "show-week-numbers") => {
                config.show_week_numbers = boolean(source, child)?
            }
            (ModuleKind::Workspaces, "all-monitors") => {
                config.all_monitors = boolean(source, child)?
            }
            (ModuleKind::Workspaces, "hidden") => {
                leaf(source, child)?;
                for value in args(source, child, child.entries().len())? {
                    config.hidden.push(
                        value
                            .as_string()
                            .ok_or_else(|| error(source, child, "hidden takes workspace names"))?
                            .into(),
                    );
                }
            }
            (ModuleKind::Workspaces, "name-map") => {
                args(source, child, 0)?;
                for entry in children(source, child)? {
                    if entry.name().value() != "name" {
                        return Err(error(source, entry, "expected name source replacement"));
                    }
                    leaf(source, entry)?;
                    let values = args(source, entry, 2)?;
                    let from = values[0]
                        .as_string()
                        .ok_or_else(|| error(source, entry, "expected name"))?;
                    let to = values[1]
                        .as_string()
                        .ok_or_else(|| error(source, entry, "expected replacement"))?;
                    if config.names.insert(from.into(), to.into()).is_some() {
                        return Err(error(source, entry, "duplicate name mapping"));
                    }
                }
            }
            (
                ModuleKind::Battery | ModuleKind::Focused | ModuleKind::SysInfo | ModuleKind::Tray,
                "visible-when",
            ) => {
                if string(source, child)? != "available" {
                    return Err(error(
                        source,
                        child,
                        "only available is supported; scripts are not executed",
                    ));
                }
                config.visible_when_available = true;
            }
            _ => {
                return Err(error(
                    source,
                    child,
                    "unknown or unsupported module setting",
                ));
            }
        }
    }
    Ok(config)
}
