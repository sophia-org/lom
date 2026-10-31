use super::*;
use crate::protocol::ContentPixels;

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn service_turn(&mut self) -> Result<usize, String> {
        self.renderer.clear_completion_wake()?;
        self.progressed = false;
        self.observe()?;
        #[cfg(feature = "diagnostic-cpu-reference")]
        self.advance_sequence(Instant::now())?;
        if self
            .panels
            .iter()
            .flat_map(|panel| &panel.resources)
            .any(|slot| {
                slot.release_deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
            })
        {
            return Err("resource retirement response timed out".into());
        }
        let second = unix_second();
        #[cfg(test)]
        let second = self
            .observed_clock
            .map_or(second, |now| now.timestamp() as u64);
        if self.live_model_inputs() && second != self.last_second {
            self.last_second = second;
            let now = observed_time();
            #[cfg(test)]
            let now = self.observed_clock.unwrap_or(now);
            for panel in &mut self.panels {
                panel.dirty |= observe_clock(&mut panel.model, now);
            }
        }
        // Each output's wait advances independently. A withheld permit or
        // resource release must not suppress observations or another output.
        let mut presented = 0;
        // Rotate the bounded vector in place; never discard its reserved
        // allocation or service a requeued candidate twice in the same turn.
        for _ in 0..self.presentations.len() {
            let presentation = self.presentations.remove(0);
            presented += usize::from(self.advance_presentation(presentation)?);
        }
        // Interaction-only candidates do not need the GPU worker or a free
        // upload slot; they retain the exact current immutable raster.
        for index in 0..self.panels.len() {
            self.refresh_interaction(index)?;
        }
        if let Some(index) = self.rendering_panel
            && let Some(content) = self.renderer.poll()?
        {
            self.rendering_panel = None;
            if let Some(stats) = &mut self.statistics {
                stats.renders_completed = stats.renders_completed.saturating_add(1);
            }
            if let Some(content) = self.accept_render(index, content)? {
                self.begin_presentation(index, content)?;
            }
            self.progressed = true;
        }
        if self.rendering_panel.is_none() {
            let count = self.panels.len();
            for offset in 0..count {
                let index = (self.next_panel + offset) % count;
                let continuation = self
                    .presentations
                    .iter()
                    .find(|p| p.panel == index)
                    .and_then(|p| p.render_request());
                let (placement, model, inspect_opacity) = if let Some(request) = continuation {
                    request
                } else {
                    let Some(plan) = self.render_plan(index)? else {
                        continue;
                    };
                    let placement = *plan.jobs.front().ok_or("empty raster update")?;
                    let inspect_opacity = plan.inspect_opacity;
                    for job in &plan.jobs {
                        let slot = &mut self.panels[index].resources[job.slot];
                        slot.state = ResourceState::Reserved;
                        slot.bytes =
                            ContentPixels::byte_len(job.rect.width, job.rect.height)? as u64;
                    }
                    self.rendering_update = Some(plan);
                    (placement, self.panels[index].model.clone(), inspect_opacity)
                };
                if !self.render_credit(index, placement.slot) {
                    if let Some(plan) = self.rendering_update.take() {
                        self.free_reserved(index, &plan);
                    }
                    continue;
                }
                let panel = &self.panels[index];
                let scale = f64::from(panel.allocation.scale_numerator)
                    / f64::from(panel.allocation.scale_denominator);
                let identity = RenderIdentity {
                    grant: self.limits.grant,
                    output: panel.output.output,
                    allocation: panel.allocation.allocation,
                    scale_generation: panel.allocation.scale_generation,
                };
                #[cfg(feature = "diagnostic-cpu-reference")]
                self.cadence_attempt()?;
                if self.renderer.uses_ring_fill() {
                    if !self.start_fill(index, model.clone(), scale, placement, inspect_opacity)? {
                        if let Some(plan) = self.rendering_update.take() {
                            self.free_reserved(index, &plan);
                        }
                        #[cfg(feature = "diagnostic-cpu-reference")]
                        self.cadence_deferred(true);
                        break;
                    }
                } else {
                    self.renderer.submit(
                        identity,
                        model.clone(),
                        placement.rect.width,
                        placement.rect.height,
                        scale,
                    )?;
                }
                let panel = &mut self.panels[index];
                self.rendering_revision = model.workspaces.generation;
                self.rendering_model = Some(model);
                if let Some(stats) = &mut self.statistics {
                    stats.renders_started = stats.renders_started.saturating_add(1);
                }
                // Observations during a multi-offer update belong to the next
                // update; never clear them when starting a subsequent tile.
                if self.rendering_update.is_some() {
                    panel.dirty = false;
                    panel.interaction_dirty = false;
                }
                self.rendering_panel = Some(index);
                self.progressed = true;
                self.next_panel = (index + 1) % count;
                break;
            }
        }
        Ok(presented)
    }

    pub(super) fn release_resource(
        &mut self,
        released: &sophia_shell_protocol::ContentResourceReleased,
    ) -> Result<(), String> {
        let slot = self
            .panels
            .iter_mut()
            .flat_map(|panel| &mut panel.resources)
            .find(|slot| {
                slot.id == released.resource.id && slot.generation == released.resource.generation
            })
            .ok_or("ResourceReleased names no owned generation")?;
        if slot.state != ResourceState::Retiring || released.reason != 0 {
            return Err("ResourceReleased does not settle the exact retiring generation".into());
        }
        slot.generation = slot
            .generation
            .checked_add(1)
            .ok_or("resource generation exhausted")?;
        slot.state = ResourceState::Free;
        slot.bytes = 0;
        slot.release_deadline = None;
        Ok(())
    }

    pub(super) fn free_reserved(&mut self, index: usize, plan: &UpdatePlan) {
        for placement in plan.next.placements() {
            let slot = &mut self.panels[index].resources[placement.slot];
            if slot.state == ResourceState::Reserved {
                slot.state = ResourceState::Free;
                slot.bytes = 0;
            }
        }
    }

    fn region_rendering(&self) -> bool {
        #[cfg(feature = "diagnostic-cpu-reference")]
        if self.cadence.is_some() {
            return false;
        }
        self.renderer.supports_regions()
            && self.limits.max_candidate_placements >= 3
            && self.limits.max_candidate_bytes >= 8192
            && self.limits.max_live_resources >= RESOURCE_SLOTS as u32
            && self.panels.len() * RESOURCE_SLOTS <= self.limits.max_resource_ids as usize
    }

    fn render_plan(&self, index: usize) -> Result<Option<UpdatePlan>, String> {
        let panel = &self.panels[index];
        if !panel.dirty
            || self.presentations.iter().any(|p| p.panel == index)
            || self.presentations.len() >= self.limits.max_pending_candidates_total as usize
        {
            return Ok(None);
        }
        let full_only = false;
        #[cfg(feature = "diagnostic-cpu-reference")]
        let full_only = self.sequence.as_ref().map_or(full_only, |s| s.full);
        let Some(plan) = panel.update_plan(self.region_rendering(), full_only) else {
            return Ok(None);
        };
        let mut bytes = 0;
        let mut new_ids = 0;
        for job in &plan.jobs {
            let size = ContentPixels::byte_len(job.rect.width, job.rect.height)? as u64;
            if size > self.limits.max_resource_bytes || size > self.limits.max_staging_bytes {
                return Err("panel cannot fit the negotiated resource budget".into());
            }
            bytes += size;
            new_ids += u64::from(panel.resources[job.slot].id == 0);
        }
        if bytes > self.limits.max_resident_bytes {
            return Err("panel cannot fit the negotiated resource budget".into());
        }
        let mut credit = 0;
        let mut live = 0;
        for slot in self.panels.iter().flat_map(|panel| &panel.resources) {
            if slot.state != ResourceState::Free {
                live += 1;
            }
            if matches!(
                slot.state,
                ResourceState::Reserved
                    | ResourceState::Staging
                    | ResourceState::Accepted
                    | ResourceState::Resident
            ) {
                credit += slot.bytes;
            }
            if slot.id == 0
                && matches!(slot.state, ResourceState::Reserved | ResourceState::Staging)
            {
                new_ids += 1;
            }
        }
        let ids_fit = self
            .next_resource
            .checked_add(new_ids)
            .is_some_and(|next| next <= u64::from(self.limits.max_resource_ids) + 1);
        Ok((credit + bytes <= self.limits.max_resident_bytes
            && live + plan.jobs.len() <= self.limits.max_live_resources as usize
            && ids_fit)
            .then_some(plan))
    }

    fn render_credit(&self, index: usize, slot: usize) -> bool {
        let slot = &self.panels[index].resources[slot];
        if slot.state != ResourceState::Reserved {
            return false;
        }
        let mut staging = 0;
        let mut open = 0;
        for slot in self.panels.iter().flat_map(|panel| &panel.resources) {
            if slot.state == ResourceState::Staging {
                staging += slot.bytes;
                open += 1;
            }
        }
        staging + slot.bytes <= self.limits.max_staging_bytes
            && open < self.limits.max_open_transfers as usize
    }
}

fn observe_clock(model: &mut Model, now: DateTime<FixedOffset>) -> bool {
    let changed = model
        .config
        .modules
        .iter()
        .enumerate()
        .any(|(index, module)| {
            module.kind == ModuleKind::Clock
                && (model.time.format(&module.format).to_string()
                    != now.format(&module.format).to_string()
                    || (model.popout == Some(model.module_id(index))
                        && (model.time.date_naive() != now.date_naive()
                            || model.time.format(&module.format_popup).to_string()
                                != now.format(&module.format_popup).to_string())))
        });
    model.time = now;
    changed
}

#[cfg(test)]
#[path = "../../tests/support/service_clock.rs"]
mod tests;
