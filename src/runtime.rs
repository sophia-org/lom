//! Explicit diagnostic input and effect boundaries; no live service grants.

mod fixture;
pub use fixture::parse_fixture;

use crate::config::{ModuleKind, PanelConfig, Theme};

/// Report unavailable integrations without implying that configuration grants them.
pub fn availability(config: &PanelConfig) -> Vec<String> {
    config
        .modules
        .iter()
        .map(|module| {
            let status = match module.kind {
                ModuleKind::Label => "local text presentation",
                ModuleKind::Clock => "clock/calendar presentation; explicit time observations",
                ModuleKind::Workspaces => "indicator presentation; live Sophia adapter pending",
                _ => "fixture presentation only; authorized data adapter pending",
            };
            format!("{} ({}): {status}", module.name, module.kind.name())
        })
        .collect()
}

/// Validate references crossing the independently parsed configuration/theme boundary.
pub fn validate_theme(config: &PanelConfig, theme: &Theme) -> Result<(), String> {
    for name in theme.instances.keys() {
        if !config.modules.iter().any(|module| &module.name == name) {
            return Err(format!("theme instance {name:?} has no configured module"));
        }
    }
    Ok(())
}
