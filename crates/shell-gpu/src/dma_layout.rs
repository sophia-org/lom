//! Device-free rules for exported LINEAR XRGB8888 frames: the single-plane
//! byte span a consumer may read, and the bound on frames held for Sophia.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Sophia's shared capture job bound; more retained frames cannot be admitted.
pub(super) const MAX_LIVE_FRAMES: usize = 4;

/// Plane 0 of one exported frame, checked against the descriptor's real size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Layout {
    pub(super) offset: u32,
    pub(super) stride: u32,
    pub(super) source_bytes: u64,
}

/// The driver chooses offset and row pitch; the span the Sophia SDK and
/// Session check (`stride * (height - 1) + offset + width * 4`) must fit.
pub(super) fn checked_layout(
    width: u32,
    height: u32,
    offset: u64,
    row_pitch: u64,
    source_bytes: u64,
) -> Result<Layout, String> {
    let row = u64::from(width) * 4;
    let rows = u64::from(height)
        .checked_sub(1)
        .filter(|_| width != 0)
        .ok_or("exported frame has no pixels")?;
    let stride = u32::try_from(row_pitch).map_err(|_| "exported row pitch exceeds u32")?;
    let offset = u32::try_from(offset).map_err(|_| "exported plane offset exceeds u32")?;
    if u64::from(stride) < row {
        return Err("exported row pitch is narrower than the XRGB8888 row".into());
    }
    let span = u64::from(stride)
        .checked_mul(rows)
        .and_then(|span| span.checked_add(u64::from(offset)))
        .and_then(|span| span.checked_add(row))
        .ok_or("exported plane span overflows")?;
    if span > source_bytes {
        return Err(format!(
            "exported plane span {span} exceeds the {source_bytes}-byte DMA-BUF"
        ));
    }
    Ok(Layout {
        offset,
        stride,
        source_bytes,
    })
}

/// Frames still owned by callers; each holds one slot until it drops.
#[derive(Clone, Default)]
pub(super) struct Frames(Arc<AtomicUsize>);

/// One retained frame's share of [`MAX_LIVE_FRAMES`].
pub(super) struct Slot(Arc<AtomicUsize>);

impl Frames {
    pub(super) fn acquire(&self) -> Option<Slot> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                (live < MAX_LIVE_FRAMES).then_some(live + 1)
            })
            .ok()
            .map(|_| Slot(Arc::clone(&self.0)))
    }

    /// Whether `slot` was acquired from this renderer's bound.
    pub(super) fn owns(&self, slot: &Slot) -> bool {
        Arc::ptr_eq(&self.0, &slot.0)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
