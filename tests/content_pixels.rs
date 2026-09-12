//! Display-free checks for the GPU-readback to CPU-wire representation.
use lom::protocol::ContentPixels;

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
        let chunks: Vec<_> = image.chunks(65536, 65488).unwrap().collect();
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
    assert!(image.chunks(65536, 32767).is_err());
    assert!(image.chunks(32768, 32768).is_err());
    assert!(image.chunks(u32::MAX, 65488).is_err());
    assert!(image.chunks(65536, 32768).is_ok());
}
