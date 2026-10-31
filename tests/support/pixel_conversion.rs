use super::rgba8_to_premultiplied_bgra;

#[test]
fn every_channel_and_alpha_matches_the_wire_rounding_rule() {
    let mut rgba = Vec::new();
    let mut expected = Vec::new();
    for alpha in 0..=255u32 {
        for channel in 0..=255u32 {
            let [red, green, blue] = [channel, 255 - channel, channel ^ 0x55];
            rgba.extend([red as u8, green as u8, blue as u8, alpha as u8]);
            expected.extend([
                ((blue * alpha + 127) / 255) as u8,
                ((green * alpha + 127) / 255) as u8,
                ((red * alpha + 127) / 255) as u8,
                alpha as u8,
            ]);
        }
    }
    rgba8_to_premultiplied_bgra(&mut rgba);
    assert_eq!(rgba, expected);
}

#[test]
fn opaque_translucent_and_transparent_pixels_can_alternate() {
    let mut pixels = [201, 77, 19, 255, 201, 77, 19, 128, 201, 77, 19, 0];
    rgba8_to_premultiplied_bgra(&mut pixels);
    assert_eq!(pixels, [19, 77, 201, 255, 10, 39, 101, 128, 0, 0, 0, 0]);
    rgba8_to_premultiplied_bgra(&mut []);
}
