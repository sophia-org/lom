//! One scene builder shared by legacy candidates and whole SFT commits.
use super::*;

pub(super) struct Scene {
    pub(super) begin: ContentCandidateBegin,
    pub(super) chunk: ContentCandidateChunk,
    pub(super) end: ContentCandidateEnd,
}

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn scene(
        &self,
        panel_index: usize,
        resources: RasterSet,
        candidate: u64,
        pacing_permit: u64,
        targets: &[ContentTargetLayout],
    ) -> Result<Scene, String> {
        let panel = &self.panels[panel_index];
        let reservation_extent = match self.config.position {
            Position::Top | Position::Bottom => panel.allocation.pixel.height,
            Position::Left | Position::Right => panel.allocation.pixel.width,
        };
        let surface = ContentSurface {
            allocation: panel.allocation.allocation,
            scale_generation: panel.allocation.scale_generation,
            role: PANEL_ROLE,
            edge: edge(self.config.position),
            margins: panel.allocation.margins,
            reservation_extent,
            parent_surface_index: u16::MAX,
            anchor_parent_rect: ContentPixelRect::default(),
        };
        let placements: Vec<_> = resources
            .placements()
            .map(|placement| {
                let resource = panel.resources[placement.slot];
                ContentPlacement {
                    resource: ContentResourceId {
                        id: resource.id,
                        generation: resource.generation,
                    },
                    surface_index: 0,
                    destination_x_px: placement.rect.x,
                    destination_y_px: placement.rect.y,
                }
            })
            .collect();
        let placement_count = placements.len() as u32;
        let target_count = u32::try_from(targets.len()).map_err(|_| "target count overflow")?;
        let begin = ContentCandidateBegin {
            grant: self.limits.grant,
            candidate_generation: candidate,
            output: panel.output.output,
            facts_generation: self.facts.facts_generation,
            pacing_permit,
            interaction_generation: 1,
            surface_count: 1,
            placement_count,
            target_count,
        };
        let chunk = ContentCandidateChunk {
            grant: self.limits.grant,
            candidate_generation: candidate,
            chunk_ordinal: 0,
            surfaces: vec![surface],
            placements,
            targets: targets
                .iter()
                .map(|target| ContentTarget {
                    surface_index: 0,
                    action_kind: 1,
                    target_id: target.indicator,
                    target_generation: target.target_generation,
                    action_id: target.action,
                    bounds_px: ContentPixelRect {
                        x: target.x,
                        y: target.y,
                        width: target.width,
                        height: target.height,
                    },
                })
                .collect(),
        };
        let end = ContentCandidateEnd {
            grant: self.limits.grant,
            candidate_generation: candidate,
            surface_count: 1,
            placement_count,
            target_count,
        };
        Ok(Scene { begin, chunk, end })
    }
}
