//! Warm raster-only screen. No ring, upload, Session or native cost is measured.
use super::*;
use masonry::imaging::{PaintSink, record::ValidateError, render::RenderSource};
use std::hint::black_box;

struct Region<'a> {
    scene: &'a Scene,
    x: u32,
}

impl RenderSource for Region<'_> {
    fn validate(&self) -> Result<(), ValidateError> {
        self.scene.validate()
    }

    fn paint_into(&mut self, sink: &mut dyn PaintSink) {
        replay_transformed(
            self.scene,
            sink,
            Affine::translate((-f64::from(self.x), 0.0)),
        );
    }
}

fn cpu_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the local timespec is writable and lives through clock_gettime.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) },
        0
    );
    u64::try_from(ts.tv_sec).unwrap() * 1_000_000_000 + u64::try_from(ts.tv_nsec).unwrap()
}

fn raster(
    renderer: &mut VelloCpuRenderer,
    scene: &Scene,
    width: u32,
    height: u32,
    bytes: &mut [u8],
    region: bool,
) {
    let target = ImageBufferTarget {
        data: bytes,
        width,
        height,
        bytes_per_row: width as usize * 4,
        format: ImageBufferFormat::Rgba8Unorm,
    };
    if region {
        renderer
            .render_source_into(&mut Region { scene, x: 1280 }, target)
            .unwrap();
    } else {
        let mut source = scene;
        renderer.render_source_into(&mut source, target).unwrap();
    }
}

#[test]
#[ignore = "explicit release-only warm raster screen; not native performance evidence"]
fn compare_full_frame_with_changed_clock_region() {
    if cfg!(debug_assertions) {
        panic!("use --release for timing");
    }
    for height in [24, 512] {
        let mut state = model(height);
        let mut driver = PreviewDriver::new(state.clone(), 1920, height, 1.0, false).unwrap();
        let first = driver.scene();
        state.time += Duration::seconds(1);
        driver.reconcile_model(state);
        let second = driver.scene();
        let scenes = [&first.scene, &second.scene];
        let mut oracle = VelloCpuRenderer::new(1, 1);
        let reference: Vec<_> = scenes
            .iter()
            .map(|scene| {
                let mut bytes = vec![0; (1920 * height * 4) as usize];
                raster(&mut oracle, scene, 1920, height, &mut bytes, false);
                bytes
            })
            .collect();
        assert_ne!(reference[0], reference[1], "clock must change pixels");
        for (block, region) in [false, true, true, false].into_iter().enumerate() {
            let width = if region { 640 } else { 1920 };
            let mut bytes = vec![0xa5; (width * height * 4) as usize];
            let expected: Vec<_> = reference
                .iter()
                .map(|full| {
                    if region {
                        crop(full, 1920, 1280, 0, 640, height)
                    } else {
                        full.clone()
                    }
                })
                .collect();
            let mut renderer = VelloCpuRenderer::new(1, 1);
            let mut times = Vec::new();
            for iteration in 0..320 {
                let index = iteration % 2;
                let start = cpu_ns();
                raster(
                    &mut renderer,
                    black_box(scenes[index]),
                    width,
                    height,
                    black_box(&mut bytes),
                    region,
                );
                let elapsed = cpu_ns().checked_sub(start).unwrap();
                equal(&bytes, &expected[index], "warm raster changed region");
                if iteration >= 120 {
                    times.push(elapsed);
                }
            }
            let mean = times.iter().sum::<u64>() / times.len() as u64;
            times.sort_unstable();
            println!(
                "lom_region_screen height={height} block={block} region={region} bytes={} trials={} mean_thread_ns={mean} p50_thread_ns={} scope=warm-raster-only",
                bytes.len(),
                times.len(),
                times[times.len() / 2]
            );
        }
    }
}
