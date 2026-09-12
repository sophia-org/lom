//! Validated KDL configuration and typed styling.

pub(crate) mod parse;
mod theme;

pub use parse::{ConfigError, parse_config};
pub use theme::{Color, Style, StyleOverride, Theme, parse_theme};

/// Position of the desired panel; placement remains Engine-owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// Top edge.
    Top,
    /// Bottom edge.
    Bottom,
    /// Left edge.
    Left,
    /// Right edge.
    Right,
}

/// Ordered region within a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Left/top group.
    Start,
    /// Geometrically centered group.
    Center,
    /// Right/bottom group.
    End,
}

/// Compiled module presentations in the first tranche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleKind {
    /// Static text.
    Label,
    /// Sophia workspace indicators.
    Workspaces,
    /// Time and calendar.
    Clock,
    /// Focused title, fixture-backed until an authorized feed exists.
    Focused,
    /// Battery observation, fixture-backed.
    Battery,
    /// System observation, fixture-backed.
    SysInfo,
    /// Status items, fixture-backed.
    Tray,
}

impl ModuleKind {
    /// Stable KDL spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Workspaces => "workspaces",
            Self::Clock => "clock",
            Self::Focused => "focused",
            Self::Battery => "battery",
            Self::SysInfo => "sys-info",
            Self::Tray => "tray",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        [
            Self::Label,
            Self::Workspaces,
            Self::Clock,
            Self::Focused,
            Self::Battery,
            Self::SysInfo,
            Self::Tray,
        ]
        .into_iter()
        .find(|kind| kind.name() == value)
    }
}

/// Validated module settings. Names are unique within a panel.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleConfig {
    /// Stable configured name.
    pub name: String,
    /// Presentation type.
    pub kind: ModuleKind,
    /// Ordered region.
    pub region: Region,
    /// Static label content.
    pub text: String,
    /// Chrono/strftime clock format.
    pub format: String,
    /// Popout clock format.
    pub format_popup: String,
    /// Calendar ISO week numbers.
    pub show_week_numbers: bool,
    /// Workspace name replacements.
    pub names: std::collections::BTreeMap<String, String>,
    /// Workspace names to omit.
    pub hidden: Vec<String>,
    /// Include indicators from other outputs.
    pub all_monitors: bool,
    /// Hide the fixture-backed module when its observation is absent.
    pub visible_when_available: bool,
}

/// One desired panel. Sizes are logical, not acknowledged native allocations.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelConfig {
    /// Configured identity.
    pub name: String,
    /// Desired edge.
    pub position: Position,
    /// Logical thickness.
    pub height: u32,
    /// Logical top, right, bottom, left margins.
    pub margins: [i32; 4],
    /// Logical gap from panel to popout.
    pub popup_gap: u32,
    /// Ordered module configuration.
    pub modules: Vec<ModuleConfig>,
}
