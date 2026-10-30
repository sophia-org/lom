//! Owner custody of CPU direct-fill ring loans: acquire before the job,
//! recheck freshness before commit, and abort stale results explicitly.
use super::*;
use crate::protocol::ContentPixels;
use sophia_shell_client::{ImageAdapterError, ImageChannelState, OwnedRingSlotFill, RingUpload};

/// Everything the owner lent one payload for, captured at acquire and
/// compared field for field at completion and again immediately before
/// commit. The commit publishes `upload`, never metadata rebuilt later.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct FillTarget {
    identity: RenderIdentity,
    scale: (u32, u32),
    current_slot: Option<usize>,
    slot: usize,
    resource: (u64, u64),
    pub(super) upload: RingUpload,
}

impl<R: ContentRenderer> ShellService<R> {
    /// Acquire one exact payload and queue the sole render job into it.
    /// Ok(false) means the ring or its single loan is not yet available; the
    /// panel stays dirty and nothing is spent.
    pub(super) fn start_fill(
        &mut self,
        index: usize,
        model: Model,
        scale: f64,
    ) -> Result<bool, String> {
        let adapter = self
            .connection
            .image_adapter()
            .ok_or("image channel was not enabled")?;
        match adapter.channel() {
            Some(ImageChannelState::Ready) => {}
            Some(ImageChannelState::Closed) => {
                return Err("image channel closed before Ready".into());
            }
            _ => return Ok(false),
        }
        let target = self.fill_target(index);
        let (width, height) = (target.upload.width, target.upload.height);
        let bytes = ContentPixels::byte_len(width, height)?;
        let fill = match self
            .connection
            .image_acquire_ring_fill(bytes, Instant::now())
        {
            Ok(fill) => fill,
            Err(ImageAdapterError::Busy | ImageAdapterError::Again) => return Ok(false),
            Err(error) => return Err(format!("owned ring fill acquisition refused: {error:?}")),
        };
        if let Err((error, mut fill)) =
            self.renderer
                .submit_fill(target.identity, model, width, height, scale, fill)
        {
            self.abort_fill(&mut fill)?;
            return Err(error);
        }
        self.rendering_fill = Some(target);
        Ok(true)
    }

    /// Admit a completed render for presentation, or abort a stale fill with
    /// no Offer, resource ID or generation spent and render the panel again.
    /// The snapshot stays in `rendering_fill` for begin_presentation.
    pub(super) fn accept_render(
        &mut self,
        index: usize,
        mut content: RenderedContent,
    ) -> Result<Option<RenderedContent>, String> {
        let target = self.rendering_fill.take();
        let RenderImage::RingFill {
            fill,
            identity,
            width,
            height,
            ..
        } = &mut content.pixels
        else {
            if target.is_some() {
                return Err("owned ring fill job completed without its loan".into());
            }
            return Ok(Some(content));
        };
        let target = target.ok_or("owned ring fill completed without an owner job")?;
        if self.fill_unchanged(index, &target, *identity, *width, *height)
            && self.panels[index].resources[target.slot].state == ResourceState::Free
        {
            self.rendering_fill = Some(target);
            return Ok(Some(content));
        }
        self.abort_fill(fill)?;
        self.rendering_model = None;
        let panel = &mut self.panels[index];
        panel.dirty = true;
        panel.interaction_dirty = true;
        Ok(None)
    }

    /// The owner's snapshot of what a fill is for, from current state.
    fn fill_target(&self, index: usize) -> FillTarget {
        let panel = &self.panels[index];
        let slot = panel.current_slot.map_or(0, |slot| 1 - slot);
        let resource = panel.resources[slot];
        FillTarget {
            identity: RenderIdentity {
                grant: self.limits.grant,
                output: panel.output.output,
                allocation: panel.allocation.allocation,
                scale_generation: panel.allocation.scale_generation,
            },
            scale: (
                panel.allocation.scale_numerator,
                panel.allocation.scale_denominator,
            ),
            current_slot: panel.current_slot,
            slot,
            resource: (resource.id, resource.generation),
            upload: RingUpload {
                // The same peek advance_resource_start spends after Offer.
                resource: if resource.id == 0 {
                    self.next_resource
                } else {
                    resource.id
                },
                resource_generation: resource.generation,
                allocation: panel.allocation.allocation.id,
                allocation_generation: panel.allocation.allocation.generation,
                output: panel.output.output.id,
                output_generation: panel.output.output.generation,
                topology_generation: self.facts.facts_generation,
                width: panel.allocation.pixel.width,
                height: panel.allocation.pixel.height,
            },
        }
    }

    /// The owner's snapshot still describes the panel and connection, and the
    /// worker reports the same identity and geometry it was lent.
    pub(super) fn fill_unchanged(
        &self,
        index: usize,
        target: &FillTarget,
        identity: RenderIdentity,
        width: u32,
        height: u32,
    ) -> bool {
        self.fill_target(index) == *target
            && target.identity.grant.connection_epoch == self.connection.connection_epoch()
            && identity == target.identity
            && (width, height) == (target.upload.width, target.upload.height)
    }

    /// Return an unpublished loan; a refused abort is an internal custody fault.
    pub(super) fn abort_fill(&mut self, fill: &mut OwnedRingSlotFill) -> Result<(), String> {
        self.connection
            .image_abort_ring_fill(fill)
            .map_err(|error| format!("owned ring fill abort refused: {error:?}"))
    }
}

#[cfg(test)]
#[path = "../../tests/support/service_fill_snapshot.rs"]
mod tests;
