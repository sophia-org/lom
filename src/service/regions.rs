//! Bounded raster sets and conservative physical region planning.
use super::*;
use crate::modules::{PanelChanges, panel_changes};

pub(super) const RESOURCE_SLOTS: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Placement {
    pub(super) slot: usize,
    pub(super) rect: ContentPixelRect,
}

/// At most three non-overlapping resources make one complete panel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RasterSet(pub(super) [Option<Placement>; 3]);

impl RasterSet {
    pub(super) fn placements(&self) -> impl Iterator<Item = Placement> + '_ {
        self.0.iter().flatten().copied()
    }

    pub(super) fn contains(&self, slot: usize) -> bool {
        self.placements().any(|placement| placement.slot == slot)
    }
}

pub(super) struct UpdatePlan {
    pub(super) next: RasterSet,
    pub(super) jobs: VecDeque<Placement>,
    pub(super) old: VecDeque<usize>,
    pub(super) tiled: bool,
    pub(super) inspect_opacity: bool,
}

pub(super) fn full_rect(width: u32, height: u32) -> ContentPixelRect {
    ContentPixelRect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

/// shortcut: exact integer thirds at scale one; fractional seams stay full
/// until outward pixel bounds and their rendering equivalence are qualified.
pub(super) fn thirds(
    width: u32,
    height: u32,
    scale: (u32, u32),
    position: Position,
) -> Option<[ContentPixelRect; 3]> {
    if width == 0
        || height == 0
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || scale.0 == 0
        || scale.0 != scale.1
    {
        return None;
    }
    let horizontal = matches!(position, Position::Top | Position::Bottom);
    let extent = if horizontal { width } else { height };
    if !extent.is_multiple_of(3) {
        return None;
    }
    Some(std::array::from_fn(|i| {
        if horizontal {
            ContentPixelRect {
                x: (i as u32 * width / 3) as i32,
                y: 0,
                width: width / 3,
                height,
            }
        } else {
            ContentPixelRect {
                x: 0,
                y: (i as u32 * height / 3) as i32,
                width,
                height: height / 3,
            }
        }
    }))
}

impl Panel {
    /// Plan from the exact current raster model, never accumulated observations.
    /// Reservation and negotiated aggregate budgets are checked by the owner.
    pub(super) fn update_plan(&self, supports_regions: bool) -> Option<UpdatePlan> {
        let full = full_rect(self.allocation.pixel.width, self.allocation.pixel.height);
        let regions = thirds(
            full.width,
            full.height,
            (
                self.allocation.scale_numerator,
                self.allocation.scale_denominator,
            ),
            self.model.config.position,
        );
        let eligible = supports_regions
            && !self.regions_disabled
            && self.model.popout.is_none()
            && regions.is_some();
        let changes = self
            .presented
            .as_ref()
            .map(|p| panel_changes(&p.model, &self.model));
        let tiled = eligible
            && self.presented.as_ref().is_some_and(|p| p.raster.opaque)
            && matches!(changes, Some(PanelChanges::Regions(_)));
        let mut next = RasterSet::default();
        let mut requested = Vec::new();
        if tiled {
            let regions = regions.expect("eligible geometry");
            let PanelChanges::Regions(changed) = changes.expect("presented model") else {
                unreachable!()
            };
            let retained_layout = self.current.placements().count() == 3
                && self
                    .current
                    .placements()
                    .zip(regions)
                    .all(|(p, r)| p.rect == r);
            for (index, rect) in regions.into_iter().enumerate() {
                if retained_layout && !changed[index] {
                    next.0[index] = self.current.0[index];
                } else {
                    requested.push((index, rect));
                }
            }
        } else {
            requested.push((0, full));
        }
        let mut free = self
            .resources
            .iter()
            .enumerate()
            .take(if supports_regions { RESOURCE_SLOTS } else { 2 })
            .filter(|(_, resource)| resource.state == ResourceState::Free)
            .map(|(i, _)| i);
        let mut jobs = VecDeque::new();
        for (index, rect) in requested {
            let placement = Placement {
                slot: free.next()?,
                rect,
            };
            next.0[index] = Some(placement);
            jobs.push_back(placement);
        }
        let old = self
            .current
            .placements()
            .filter(|p| !next.contains(p.slot))
            .map(|p| p.slot)
            .collect();
        Some(UpdatePlan {
            next,
            jobs,
            old,
            tiled,
            inspect_opacity: eligible,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/support/service_regions.rs"]
mod tests;
