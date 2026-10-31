//! Semantic changes in the fixed start/center/end view groups. This is not
//! permission to tile: geometry, actual opacity and retained custody are separate.
//! Containment comes from the groups' fixed clipping portals. Text-shape and
//! label restrictions are conservative policy, not proof of resolved layout;
//! future sub-group or target reuse must compare actual layout output.
use super::{workspace_colors, workspaces};
use crate::{
    config::{ModuleKind, Region},
    model::{Model, Workspace},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PanelChanges {
    Unchanged,
    Regions([bool; 3]),
    Full,
}

/// Compare the referenced raster's model directly with the next model. Today
/// reuse references Presented; a future tile ledger may reference an in-flight
/// resident set. Intermediate observations must not erase unpresented changes.
pub(crate) fn panel_changes(left: &Model, right: &Model) -> PanelChanges {
    // Adding model state must prompt a review of its pixel effects.
    let Model {
        config,
        theme,
        generation,
        output,
        epoch,
        workspaces: _,
        time: _,
        popout,
        month,
        fixture_values,
        fixture,
    } = left;
    if config != &right.config
        || theme != &right.theme
        || *generation != right.generation
        || *output != right.output
        || *epoch != right.epoch
        || *popout != right.popout
        || *month != right.month
        || *fixture != right.fixture
        || fixture_values != &right.fixture_values
    {
        return PanelChanges::Full;
    }
    let mut changed = [false; 3];
    for (index, config) in left.config.modules.iter().enumerate() {
        let differs = match config.kind {
            ModuleKind::Workspaces => {
                let style = left.theme.resolve(config);
                let project = |model: &Model| {
                    workspaces(model, config)
                        .into_iter()
                        .map(|(entry, label)| {
                            let Workspace {
                                id: _,
                                output: _,
                                name: _,
                                active: _,
                                visible: _,
                                urgent: _,
                                action,
                            } = entry;
                            (
                                label,
                                workspace_colors(entry, &style),
                                action.is_some_and(|id| id != 0),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                let before = project(left);
                let after = project(right);
                if before == after {
                    continue;
                }
                if before.len() != after.len()
                    || before
                        .iter()
                        .zip(&after)
                        .any(|(a, b)| a.0 != b.0 || a.2 != b.2)
                {
                    return PanelChanges::Full;
                }
                true
            }
            ModuleKind::Clock => {
                if left.popout == Some(left.module_id(index))
                    && (left.time.date_naive() != right.time.date_naive()
                        || left.time.format(&config.format_popup).to_string()
                            != right.time.format(&config.format_popup).to_string())
                {
                    return PanelChanges::Full;
                }
                let before = left.time.format(&config.format).to_string();
                let after = right.time.format(&config.format).to_string();
                if before == after {
                    continue;
                }
                // shortcut: only fixed-length numeric clock text; widen after
                // layout and glyph-boundary proofs for other formats.
                if before.len() != after.len()
                    || !before
                        .bytes()
                        .chain(after.bytes())
                        .all(|byte| byte.is_ascii_digit() || b":-/ .".contains(&byte))
                {
                    return PanelChanges::Full;
                }
                true
            }
            ModuleKind::Label
            | ModuleKind::Focused
            | ModuleKind::Battery
            | ModuleKind::SysInfo
            | ModuleKind::Tray => false,
        };
        if differs {
            if left.popout.is_some() {
                return PanelChanges::Full;
            }
            changed[match config.region {
                Region::Start => 0,
                Region::Center => 1,
                Region::End => 2,
            }] = true;
        }
    }
    if changed == [false; 3] {
        PanelChanges::Unchanged
    } else {
        PanelChanges::Regions(changed)
    }
}

#[cfg(test)]
#[path = "../../tests/support/panel_changes.rs"]
mod tests;
