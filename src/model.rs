//! Passive application records. No toolkit, clock reads, or device state.

use crate::config::{PanelConfig, Theme};
use chrono::{DateTime, FixedOffset, NaiveDate};
use std::collections::BTreeMap;

/// A module slot together with its replacement generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleId {
    /// Index in the configuration's module table.
    pub index: usize,
    /// Configuration incarnation, advanced on replacement.
    pub generation: u64,
}

/// An observed workspace with protocol identity separate from its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    /// Opaque indicator identity.
    pub id: u64,
    /// Authorized output identity.
    pub output: u64,
    /// Display name.
    pub name: String,
    /// Selected on this output.
    pub active: bool,
    /// Visible on another output.
    pub visible: bool,
    /// Requires attention.
    pub urgent: bool,
    /// Available activation action; absent is not activatable.
    pub action: Option<u64>,
}

/// Latest complete indicator observation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    /// Connection incarnation.
    pub epoch: u64,
    /// Publication generation within that connection.
    pub generation: u64,
    /// Focused output, including outputs without indicators.
    pub active_output: Option<u64>,
    /// Complete indicator set.
    pub entries: Vec<Workspace>,
}

/// Single-owner application state.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// Desired configuration.
    pub config: PanelConfig,
    /// Typed styles.
    pub theme: Theme,
    /// Local module lifetime generation.
    pub generation: u64,
    /// Output this panel represents, provided by the driver or fixture.
    pub output: u64,
    /// Current connection; zero means no authorized source.
    pub epoch: u64,
    /// Latest indicator snapshot.
    pub workspaces: WorkspaceSnapshot,
    /// Explicitly observed wall-clock value; never read inside update/view.
    pub time: DateTime<FixedOffset>,
    /// Local calendar/popout intent; this is not native visibility.
    pub popout: Option<ModuleId>,
    /// Calendar month, always its first day.
    pub month: NaiveDate,
    /// Explicit fixture presentation strings, keyed by configured identity.
    pub fixture_values: BTreeMap<String, String>,
    /// Marks the source of the preview; never treated as live observations.
    pub fixture: bool,
}
impl Model {
    /// Create an unconnected model with an explicit time observation.
    pub fn new(
        config: PanelConfig,
        theme: Theme,
        output: u64,
        time: DateTime<FixedOffset>,
    ) -> Self {
        use chrono::Datelike;
        let month = time.date_naive().with_day(1).expect("first day exists");
        Self {
            config,
            theme,
            output,
            time,
            month,
            generation: 1,
            epoch: 0,
            workspaces: WorkspaceSnapshot::default(),
            popout: None,
            fixture_values: BTreeMap::new(),
            fixture: false,
        }
    }
    /// Identity of a currently configured module.
    pub fn module_id(&self, index: usize) -> ModuleId {
        ModuleId {
            index,
            generation: self.generation,
        }
    }
    /// Validate an asynchronous or retained owner.
    pub fn owns(&self, id: ModuleId) -> bool {
        id.generation == self.generation && id.index < self.config.modules.len()
    }
}
