//! Exercise the production DMA-BUF layout and retention rules without a GPU.
//! Nothing here proves driver support, the queue-family release or import.
#[path = "../src/dma_layout.rs"]
mod dma_layout;
use dma_layout::{Frames, Layout, MAX_LIVE_FRAMES, checked_layout};

#[test]
fn a_tight_or_padded_linear_plane_is_accepted_with_its_measured_size() {
    assert_eq!(
        checked_layout(200, 20, 0, 800, 16_000),
        Ok(Layout {
            offset: 0,
            stride: 800,
            source_bytes: 16_000,
        })
    );
    // Driver row padding and a plane offset: span 1024 * 19 + 64 + 800.
    let padded = checked_layout(200, 20, 64, 1024, 20_320).unwrap();
    assert_eq!((padded.offset, padded.stride), (64, 1024));
}

#[test]
fn a_plane_that_overruns_its_descriptor_is_refused() {
    assert!(checked_layout(200, 20, 0, 800, 15_999).is_err());
    assert!(checked_layout(200, 20, 64, 1024, 20_319).is_err());
}

#[test]
fn narrow_oversized_or_empty_layouts_are_refused() {
    assert!(
        checked_layout(200, 20, 0, 799, u64::MAX).is_err(),
        "stride < w*4"
    );
    assert!(
        checked_layout(200, 20, 0, 1 << 32, u64::MAX).is_err(),
        "stride > u32"
    );
    assert!(
        checked_layout(200, 20, 1 << 32, 800, u64::MAX).is_err(),
        "offset > u32"
    );
    assert!(checked_layout(0, 20, 0, 800, u64::MAX).is_err());
    assert!(checked_layout(200, 0, 0, 800, u64::MAX).is_err());
    // The largest u32 stride and offset cannot overflow the u64 span.
    assert!(
        checked_layout(
            8192,
            4096,
            u64::from(u32::MAX),
            u64::from(u32::MAX),
            u64::MAX
        )
        .is_ok()
    );
}

#[test]
fn at_most_four_frames_are_retained_and_a_dropped_frame_frees_its_slot() {
    let frames = Frames::default();
    let mut live: Vec<_> = (0..MAX_LIVE_FRAMES)
        .map(|_| frames.acquire().expect("free slot"))
        .collect();
    assert!(
        frames.acquire().is_none(),
        "a fifth frame waits for a release"
    );
    live.pop();
    let again = frames.acquire().expect("slot returned on drop");
    assert!(frames.acquire().is_none());
    drop((live, again));
    assert!(frames.clone().acquire().is_some(), "clones share one bound");
}
