//! Display-free checks for the GPU-readback to CPU-wire representation.
use lom::protocol::ContentPixels;
use sophia_shell_protocol::*;

#[test]
fn channel_order_alpha_and_rounding_match_the_cpu_wire_contract() {
    let image = ContentPixels::from_rgba8(
        4,
        1,
        vec![
            255, 0, 0, 255, 255, 128, 64, 128, 255, 255, 255, 0, 127, 128, 255, 1,
        ],
    )
    .unwrap();
    assert_eq!(
        image.bytes(),
        &[0, 0, 255, 255, 32, 64, 128, 128, 0, 0, 0, 0, 1, 1, 0, 1]
    );
    assert_eq!(image.width(), 4);
    assert_eq!(image.height(), 1);
}

#[test]
fn whole_row_chunks_match_the_adr_examples_and_reconstruct_every_byte() {
    for (width, height, expected) in [
        (2560, 32, vec![61440, 61440, 61440, 61440, 61440, 20480]),
        (120, 32, vec![15360]),
        (8192, 128, vec![32768; 128]),
    ] {
        let image =
            ContentPixels::from_rgba8(width, height, vec![255; (width * height * 4) as usize])
                .unwrap();
        let chunks: Vec<_> = image.chunks(65488).unwrap().collect();
        assert_eq!(
            chunks.iter().map(|c| c.bytes.len()).collect::<Vec<_>>(),
            expected
        );
        let mut offset = 0;
        for (ordinal, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.ordinal, ordinal as u32);
            assert_eq!(chunk.offset, offset);
            assert_eq!(chunk.bytes.len() % (width as usize * 4), 0);
            offset += chunk.bytes.len() as u64;
        }
        assert_eq!(offset, image.bytes().len() as u64);
        let reconstructed: Vec<_> = chunks
            .iter()
            .flat_map(|c| c.bytes.iter().copied())
            .collect();
        assert_eq!(reconstructed, image.bytes());
    }
}

#[test]
fn dimension_product_length_and_negotiated_row_limits_are_joint_constraints() {
    assert!(ContentPixels::byte_len(8192, 4096).is_err());
    assert_eq!(ContentPixels::byte_len(8192, 128).unwrap(), 4194304);
    assert!(ContentPixels::byte_len(0, 1).is_err());
    assert!(ContentPixels::from_rgba8(1, 1, vec![255; 8]).is_err());
    let image = ContentPixels::from_rgba8(8192, 1, vec![0; 32768]).unwrap();
    // One byte short of a whole row cannot carry it; the row itself fits.
    assert!(image.chunks(32767).is_err());
    assert!(image.chunks(32768).is_ok());
}

/// The canonical upload chunk is `max_chunk_bytes` (t268). On every Limits
/// object the SDK accepts, the earlier `min(max_frame_payload - 48,
/// max_chunk_bytes)` is the same value, so each chunk sequence is unchanged:
/// this checks that equivalence and the SDK's own layout at whole-row
/// boundaries, for one chunk under every valid frame cap. Limits the SDK
/// refuses are its concern, not this adapter's.
#[test]
fn canonical_chunks_match_the_sdk_layout_under_every_valid_frame_cap() {
    let grant = ContentGrant {
        connection_epoch: 1,
        content_grant_epoch: 1,
    };
    let mut checked = 0;
    for chunk in [32768, 40000, 65488] {
        let mut frames = vec![chunk + 48, chunk + 49, 65536];
        frames.retain(|frame| *frame <= 65536);
        frames.dedup();
        // Widths where whole rows exactly fill the chunk, one pixel past
        // that, one row per chunk, and small rasters.
        let exact = [chunk / 4, chunk / 8, chunk / 16]
            .into_iter()
            .filter(|w| chunk % (w * 4) == 0);
        let widths: Vec<u32> = exact
            .flat_map(|w| [w, w + 1])
            .chain([1, 7, 120, 2560, 8192])
            .filter(|w| (1..=8192).contains(w))
            .collect();
        for width in widths {
            let height = 32;
            let image =
                ContentPixels::from_rgba8(width, height, vec![255; (width * height * 4) as usize])
                    .unwrap();
            let rows = chunk / (width * 4);
            let lengths: Vec<_> = image
                .chunks(chunk)
                .unwrap()
                .map(|c| c.bytes.len())
                .collect();
            assert!(lengths.iter().all(|len| len % (width as usize * 4) == 0));
            assert_eq!(lengths[0], (rows.min(height) * width * 4) as usize);
            for frame in &frames {
                let mut limits = ContentLimits::prototype(grant);
                limits.max_frame_payload = *frame;
                limits.max_chunk_bytes = chunk;
                assert_eq!(limits.validate(), Ok(()), "frame {frame} chunk {chunk}");
                // The earlier frame-derived expression gives the same rows.
                assert_eq!((frame - 48).min(chunk) / (width * 4), rows);
                let begin = ContentResourceBegin {
                    grant,
                    resource: ContentResourceId {
                        id: 1,
                        generation: 1,
                    },
                    width_px: width,
                    height_px: height,
                    rendered_scale_numerator: 1,
                    rendered_scale_denominator: 1,
                    pixel_format: 1,
                    chunk_count: lengths.len() as u32,
                    total_bytes: image.bytes().len() as u64,
                };
                let layout = begin.layout(&limits).unwrap();
                assert_eq!(layout.rows_per_chunk, rows);
                assert_eq!(layout.chunk_count, lengths.len() as u32);
                checked += 1;
            }
        }
    }
    // Control: every chunk size and frame cap was actually exercised.
    assert!(checked >= 20, "only {checked} layouts");
}
