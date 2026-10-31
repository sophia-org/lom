//! A fixed two-model fixture workload; rendering and resource custody are unchanged.
use super::*;

pub(in crate::service) struct Alternating {
    pub(in crate::service) full: bool,
    models: [Model; 2],
    previous: [Option<ContentResourceId>; 3],
    previous_stats: WorkStatistics,
    pub(super) validated: u64,
}

#[cfg(test)]
#[path = "../../../tests/support/service_alternating.rs"]
mod tests;

impl Alternating {
    pub(super) fn jobs(&self, candidate: u64) -> u64 {
        candidate + if !self.full && candidate >= 2 { 2 } else { 0 }
    }
}

impl<R: ContentRenderer> ShellService<R> {
    pub(crate) fn enable_region_cadence(&mut self, full: bool) -> Result<(), String> {
        self.enable_alternating(full, QUOTA)
    }

    fn enable_alternating(&mut self, full: bool, quota: u64) -> Result<(), String> {
        if self.sequence.is_some()
            || !self.renderer.supports_regions()
            || !self.renderer.uses_ring_fill()
            || self.renderer.uses_dma_buf()
        {
            return Err("region cadence requires a fresh CPU region owner".into());
        }
        let models = self.fixture_models(&[
            include_str!("../../../diagnostics/regions/01.kdl"),
            include_str!("../../../diagnostics/regions/02.kdl"),
        ])?;
        let models: [Model; 2] = models.try_into().map_err(|_| "two fixtures required")?;
        self.enable_cadence(quota)?;
        self.panels[0].model = models[0].clone();
        self.panels[0].dirty = true;
        self.panels[0].interaction_dirty = true;
        self.cadence.as_mut().unwrap().alternating = Some(Alternating {
            full,
            models,
            previous: [None; 3],
            previous_stats: WorkStatistics::default(),
            validated: 0,
        });
        println!(
            "lom_region_cadence schema=1 source=fixture role={} quota={quota} fixtures=01,02",
            if full { "full" } else { "regions" }
        );
        Ok(())
    }

    pub(super) fn alternating_presented(
        &mut self,
        panel: usize,
        candidate: u64,
        sequence: u64,
    ) -> Result<(), String> {
        let Some(cadence) = &mut self.cadence else {
            return Ok(());
        };
        let Some(alternating) = &mut cadence.alternating else {
            return Ok(());
        };
        let current = &mut self.panels[panel];
        let stats = self.statistics.ok_or("region cadence lost counters")?;
        let index = ((sequence - 1) % 2) as usize;
        if panel != 0
            || candidate != sequence
            || current.regions_disabled
            || current
                .presented
                .as_ref()
                .is_none_or(|p| p.model != alternating.models[index])
        {
            return Err("region cadence refused a contaminated, extra or disabled update".into());
        }
        let ids = current.current.0.map(|placement| {
            placement.map(|p| {
                let resource = current.resources[p.slot];
                ContentResourceId {
                    id: resource.id,
                    generation: resource.generation,
                }
            })
        });
        let placements = ids.iter().flatten().count();
        let retained = ids
            .iter()
            .flatten()
            .filter(|id| alternating.previous.contains(&Some(**id)))
            .count();
        let uploads = stats
            .uploads
            .checked_sub(alternating.previous_stats.uploads)
            .ok_or("region cadence upload counter regressed")?;
        let bytes = stats
            .upload_bytes
            .checked_sub(alternating.previous_stats.upload_bytes)
            .ok_or("region cadence byte counter regressed")?;
        let full_bytes = u64::from(current.allocation.pixel.width)
            * u64::from(current.allocation.pixel.height)
            * 4;
        let expected = if alternating.full || sequence == 1 {
            (1, 0, 1, full_bytes)
        } else if sequence == 2 {
            (3, 0, 3, full_bytes)
        } else {
            (3, 2, 1, full_bytes / 3)
        };
        if (placements, retained, uploads, bytes) != expected {
            return Err("region cadence placement/upload prediction failed".into());
        }
        if !alternating.full
            && sequence > 2
            && (ids[..2] != alternating.previous[..2] || ids[2] == alternating.previous[2])
        {
            return Err("region cadence must replace only the End tile".into());
        }
        println!(
            "lom_region_cadence_sample schema=1 source=fixture role={} sequence={sequence} model={index} placements={placements} uploads={uploads} upload_bytes={bytes} retained={retained} regions_disabled={}",
            if alternating.full { "full" } else { "regions" },
            u8::from(current.regions_disabled)
        );
        alternating.previous = ids;
        alternating.previous_stats = stats;
        alternating.validated = sequence;
        if sequence < cadence.quota {
            current.model = alternating.models[(sequence % 2) as usize].clone();
            current.interaction_dirty = true;
        }
        Ok(())
    }
}
