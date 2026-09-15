use super::*;

impl<R: ContentRenderer> ShellService<R> {
    pub(super) fn service_turn(&mut self) -> Result<usize, String> {
        self.observe()?;
        self.upload_chunks_left = 4;
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
        if second != self.last_second {
            self.last_second = second;
            let now = observed_time();
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
        if let Some(index) = self.rendering_panel
            && let Some(content) = self.renderer.poll()?
        {
            self.rendering_panel = None;
            self.begin_presentation(index, content)?;
        }
        if self.rendering_panel.is_none() {
            let count = self.panels.len();
            for offset in 0..count {
                let index = (self.next_panel + offset) % count;
                if !self.render_ready(index)? {
                    continue;
                }
                let panel = &mut self.panels[index];
                let scale = f64::from(panel.allocation.scale_numerator)
                    / f64::from(panel.allocation.scale_denominator);
                self.rendering_revision = panel.model.workspaces.generation;
                self.rendering_model = Some(panel.model.clone());
                self.renderer.submit(
                    RenderIdentity {
                        grant: self.limits.grant,
                        output: panel.output.output,
                        allocation: panel.allocation.allocation,
                        scale_generation: panel.allocation.scale_generation,
                    },
                    panel.model.clone(),
                    panel.allocation.pixel.width,
                    panel.allocation.pixel.height,
                    scale,
                )?;
                panel.dirty = false;
                self.rendering_panel = Some(index);
                self.next_panel = (index + 1) % count;
                break;
            }
        }
        Ok(presented)
    }

    pub(super) fn release_resource(
        &mut self,
        released: &sophia_protocol::ContentResourceReleased,
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

    fn render_ready(&self, index: usize) -> Result<bool, String> {
        let panel = &self.panels[index];
        if !panel.dirty
            || self
                .presentations
                .iter()
                .any(|pending| pending.panel == index)
        {
            return Ok(false);
        }
        let slot = &panel.resources[panel.current_slot.map_or(0, |slot| 1 - slot)];
        if slot.state != ResourceState::Free {
            return Ok(false);
        }
        let bytes =
            ContentPixels::byte_len(panel.allocation.pixel.width, panel.allocation.pixel.height)?
                as u64;
        if bytes > self.limits.max_resource_bytes
            || bytes > self.limits.max_staging_bytes
            || bytes > self.limits.max_resident_bytes
        {
            return Err("panel cannot fit the negotiated resource budget".into());
        }
        let mut staging = 0u64;
        let mut resident_credit = 0u64;
        let mut open = 0usize;
        let mut live = 0usize;
        for slot in self.panels.iter().flat_map(|panel| &panel.resources) {
            if slot.state != ResourceState::Free {
                live += 1;
            }
            if slot.state == ResourceState::Staging {
                staging += slot.bytes;
                open += 1;
            }
            if matches!(slot.state, ResourceState::Staging | ResourceState::Resident) {
                resident_credit += slot.bytes;
            }
        }
        Ok(staging + bytes <= self.limits.max_staging_bytes
            && resident_credit + bytes <= self.limits.max_resident_bytes
            && open < self.limits.max_open_transfers as usize
            && live < self.limits.max_live_resources as usize
            && self.presentations.len() < self.limits.max_pending_candidates_total as usize
            && (slot.id != 0 || self.next_resource <= u64::from(self.limits.max_resource_ids)))
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
