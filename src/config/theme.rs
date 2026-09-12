use std::collections::{BTreeMap, BTreeSet};

use super::parse::{args, boolean, children, document, error, number, string, unique};
use super::{ConfigError, ModuleConfig, ModuleKind};

/// An opaque sRGB color; no toolkit objects enter configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub [u8; 3]);
impl Color {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.len() != 7 || !value.starts_with('#') || !value.is_ascii() {
            return None;
        }
        Some(Self([
            u8::from_str_radix(&value[1..3], 16).ok()?,
            u8::from_str_radix(&value[3..5], 16).ok()?,
            u8::from_str_radix(&value[5..7], 16).ok()?,
        ]))
    }
}

/// Resolved presentation values, independent of widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Base fill.
    pub background: Color,
    /// Text color.
    pub foreground: Color,
    /// Selected/hover fill.
    pub selected: Color,
    /// Active workspace underline/today fill.
    pub active: Color,
    /// Urgent workspace fill.
    pub urgent: Color,
    /// Font size in logical pixels.
    pub font_size: u32,
    /// Gap between modules in logical pixels.
    pub gap: u32,
    /// Button horizontal padding in logical pixels.
    pub padding: u32,
    /// Use the bundled bold face.
    pub bold: bool,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            background: Color([28; 3]),
            foreground: Color([255; 3]),
            selected: Color([45; 3]),
            active: Color([102, 153, 204]),
            urgent: Color([143, 10, 10]),
            font_size: 13,
            gap: 13,
            padding: 7,
            bold: false,
        }
    }
}

/// Sparse style patch; unset fields retain earlier values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleOverride {
    /// Background replacement.
    pub background: Option<Color>,
    /// Text replacement.
    pub foreground: Option<Color>,
    /// Selected/hover replacement.
    pub selected: Option<Color>,
    /// Active underline replacement.
    pub active: Option<Color>,
    /// Urgency fill replacement.
    pub urgent: Option<Color>,
    /// Font size replacement.
    pub font_size: Option<u32>,
    /// Module gap replacement.
    pub gap: Option<u32>,
    /// Button padding replacement.
    pub padding: Option<u32>,
    /// Weight replacement.
    pub bold: Option<bool>,
}
impl StyleOverride {
    fn apply(&self, style: &mut Style) {
        if let Some(v) = self.background {
            style.background = v;
        }
        if let Some(v) = self.foreground {
            style.foreground = v;
        }
        if let Some(v) = self.selected {
            style.selected = v;
        }
        if let Some(v) = self.active {
            style.active = v;
        }
        if let Some(v) = self.urgent {
            style.urgent = v;
        }
        if let Some(v) = self.font_size {
            style.font_size = v;
        }
        if let Some(v) = self.gap {
            style.gap = v;
        }
        if let Some(v) = self.padding {
            style.padding = v;
        }
        if let Some(v) = self.bold {
            style.bold = v;
        }
    }
}

/// Explicit theme cascade: defaults, module kind, then instance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Theme {
    /// Document identity.
    pub name: String,
    /// Base values.
    pub defaults: Style,
    /// Kind-specific patches.
    pub modules: BTreeMap<ModuleKind, StyleOverride>,
    /// Configured instance patches.
    pub instances: BTreeMap<String, StyleOverride>,
}
impl Theme {
    /// Resolve a module without modifying the theme.
    pub fn resolve(&self, module: &ModuleConfig) -> Style {
        let mut style = self.defaults.clone();
        if let Some(patch) = self.modules.get(&module.kind) {
            patch.apply(&mut style);
        }
        if let Some(patch) = self.instances.get(&module.name) {
            patch.apply(&mut style);
        }
        style
    }
}

/// Parse a KDL v2 theme; unknown properties never silently disappear.
pub fn parse_theme(source: &str) -> Result<Theme, ConfigError> {
    let doc = document(source)?;
    let nodes = doc.nodes();
    if nodes.len() != 2
        || nodes[0].name().value() != "version"
        || number(source, &nodes[0], 1, 1)? != 1
        || nodes[1].name().value() != "theme"
    {
        return Err(ConfigError {
            message: "expected version 1 followed by one theme".into(),
            line: 1,
            column: 1,
        });
    }
    let node = &nodes[1];
    let name = args(source, node, 1)?[0]
        .as_string()
        .ok_or_else(|| error(source, node, "expected theme name"))?
        .to_owned();
    let mut theme = Theme {
        name,
        ..Theme::default()
    };
    let mut defaults_seen = false;
    for child in children(source, node)? {
        match child.name().value() {
            "defaults" => {
                args(source, child, 0)?;
                if defaults_seen {
                    return Err(error(source, child, "duplicate defaults"));
                }
                defaults_seen = true;
                patch(source, child)?.apply(&mut theme.defaults);
            }
            "module" | "instance" => {
                let name = args(source, child, 1)?[0]
                    .as_string()
                    .ok_or_else(|| error(source, child, "expected override name"))?;
                let patch = patch(source, child)?;
                let duplicate = if child.name().value() == "module" {
                    let kind = ModuleKind::parse(name)
                        .ok_or_else(|| error(source, child, "unsupported module kind"))?;
                    theme.modules.insert(kind, patch).is_some()
                } else {
                    theme.instances.insert(name.into(), patch).is_some()
                };
                if duplicate {
                    return Err(error(source, child, "duplicate style override"));
                }
            }
            _ => return Err(error(source, child, "unknown theme node")),
        }
    }
    Ok(theme)
}
fn patch(source: &str, node: &kdl::KdlNode) -> Result<StyleOverride, ConfigError> {
    let mut patch = StyleOverride::default();
    let mut seen = BTreeSet::new();
    for child in children(source, node)? {
        unique(source, child, &mut seen)?;
        match child.name().value() {
            "background" | "foreground" | "selected" | "active" | "urgent" => {
                let color = Color::parse(&string(source, child)?)
                    .ok_or_else(|| error(source, child, "expected #rrggbb color"))?;
                match child.name().value() {
                    "background" => patch.background = Some(color),
                    "foreground" => patch.foreground = Some(color),
                    "selected" => patch.selected = Some(color),
                    "active" => patch.active = Some(color),
                    _ => patch.urgent = Some(color),
                }
            }
            "font-size" => patch.font_size = Some(number(source, child, 6, 96)? as u32),
            "gap" => patch.gap = Some(number(source, child, 0, 128)? as u32),
            "padding" => patch.padding = Some(number(source, child, 0, 128)? as u32),
            "bold" => patch.bold = Some(boolean(source, child)?),
            _ => return Err(error(source, child, "unknown theme property")),
        }
    }
    Ok(patch)
}
