//! Button lifetimes are independent of indicator publication revisions.
use crate::{model::Model, ui::ContentTargetLayout, update::Msg};

#[derive(Default)]
pub(super) struct TargetGenerations {
    next: u64,
    previous: Vec<(ContentTargetLayout, String)>,
}

impl TargetGenerations {
    /// The panel owns one allocation/scale incarnation. Keep only the latest
    /// prepared complete target set; removal/reappearance cannot recycle IDs.
    pub(super) fn prepare(
        &mut self,
        model: &Model,
        targets: &mut [ContentTargetLayout],
        maximum: usize,
    ) -> Result<(), String> {
        if targets.len() > maximum {
            return Err("target generation capacity exceeded".into());
        }
        let mut next = Vec::with_capacity(targets.len());
        for target in targets {
            let label = model
                .workspaces
                .entries
                .iter()
                .find(|e| e.id == target.indicator)
                .ok_or("target lost its indicator")?
                .name
                .clone();
            target.target_generation = if let Some((old, _)) = self
                .previous
                .iter()
                .find(|(old, name)| name == &label && same_meaning(old, target))
            {
                old.target_generation
            } else {
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or("target generation exhausted")?;
                self.next
            };
            next.push((target.clone(), label));
        }
        self.previous = next;
        Ok(())
    }
}

fn same_meaning(a: &ContentTargetLayout, b: &ContentTargetLayout) -> bool {
    let same_owner = match (&a.message, &b.message) {
        (
            Msg::ActivateWorkspace {
                owner: ao,
                epoch: ae,
                indicator: ai,
                action: aa,
                ..
            },
            Msg::ActivateWorkspace {
                owner: bo,
                epoch: be,
                indicator: bi,
                action: ba,
                ..
            },
        ) => ao == bo && ae == be && ai == bi && aa == ba,
        _ => false,
    };
    same_owner
        && a.indicator == b.indicator
        && a.action == b.action
        && a.x == b.x
        && a.y == b.y
        && a.width == b.width
        && a.height == b.height
}
