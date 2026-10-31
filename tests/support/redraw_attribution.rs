//! Offscreen phase attribution. Ring events are fixtures, never host evidence.
use super::*;
use crate::model::{Workspace, WorkspaceSnapshot};
use masonry::imaging::render::{ImageBufferFormat, ImageBufferTarget, ImageRenderer};
use sophia_image_producer::{
    ResourceStatusKind, RingOfferState, RingProducer, RingProducerConfig, RingUpload,
};
use sophia_shell_protocol::shell_files::{RingControl, RingEventKind, RingOffer};
use std::hint::black_box;

const WIDTH: u32 = 1920;
const STATIC: &str = include_str!("../fixtures/redraw/static24.kdl");

#[derive(Clone, Copy, Debug)]
struct Span {
    cpu_ns: u64,
    wall_ns: u64,
}

fn thread_cpu_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes one initialized timespec; no borrowed state.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) },
        0
    );
    u64::try_from(ts.tv_sec).unwrap() * 1_000_000_000 + u64::try_from(ts.tv_nsec).unwrap()
}

fn measure<T>(operation: impl FnOnce() -> T) -> (T, Span) {
    let wall = Instant::now();
    let cpu = thread_cpu_ns();
    let value = operation();
    let cpu_ns = thread_cpu_ns() - cpu;
    (
        value,
        Span {
            cpu_ns,
            wall_ns: wall.elapsed().as_nanos().try_into().unwrap(),
        },
    )
}

fn model_for(height: u32) -> Model {
    assert!([24, 512].contains(&height));
    assert_eq!(STATIC.matches("height 24").count(), 1);
    let text = STATIC.replace("height 24", &format!("height {height}"));
    let (config, theme) = parse_shell_config(&text).unwrap();
    let mut model = Model::new(config, theme, 1, DateTime::<Utc>::UNIX_EPOCH.fixed_offset());
    model.epoch = 1;
    model.workspaces = WorkspaceSnapshot {
        epoch: 1,
        generation: 2,
        active_output: Some(1),
        entries: (1..=9)
            .map(|id| Workspace {
                id,
                output: 1,
                name: id.to_string(),
                active: id == 1,
                visible: false,
                urgent: false,
                action: Some(id + 10),
            })
            .collect(),
    };
    model
}

fn expected_checksum(height: u32) -> u64 {
    match height {
        24 => 0xa7bbb8c840a7303c,
        512 => 0x9a603a023eb2ab7c,
        _ => panic!("unsupported attribution geometry"),
    }
}

fn ready_ring_for(height: u32) -> RingProducer {
    let mut ring = RingProducer::new(
        RingProducerConfig {
            identity: RING,
            slots: 2,
            capacity: 0,
            max_resource_bytes: u64::from(WIDTH * height * 4),
            transfer_timeout_ms: 2000,
        },
        0,
    )
    .unwrap();
    assert!(ring.registration().is_some());
    ring.ring_event(ring_event(RingEventKind::RingAdmitted), 1)
        .unwrap();
    assert!(ring.source_packet().is_some());
    ring.source_sent().unwrap();
    ring.ring_event(ring_event(RingEventKind::RingReady), 2)
        .unwrap();
    ring
}

fn release(ring: &mut RingProducer, offer: RingOffer, now: u64) {
    ring.resource_status(
        offer.resource,
        offer.resource_generation,
        ResourceStatusKind::Accepted,
        now,
    )
    .unwrap();
    let mut event = ring_event(RingEventKind::SlotReleased);
    event.offer_id = offer.offer_id;
    event.slot = offer.slot;
    event.slot_generation = offer.slot_generation;
    event.resource = offer.resource;
    event.resource_generation = offer.resource_generation;
    ring.ring_event(event, now).unwrap();
    assert_eq!(
        ring.offer(offer.offer_id).unwrap().state,
        RingOfferState::Settled
    );
    ring.forget(offer.offer_id).unwrap();
}

struct Reply {
    fill: OwnedRingSlotFill,
    phases: [Span; 5],
    checksum: u64,
}

fn worker(incoming: Receiver<(RenderJob, bool)>, done: SyncSender<Reply>) {
    let mut hosts = RetainedHosts::default();
    let mut renderer = VelloCpuRenderer::new(1, 1);
    while let Ok((mut job, check_reference)) = incoming.recv() {
        let mut fill = job.fill.take().unwrap();
        let ((mut scene, targets), prepare) = measure(|| hosts.scene(job).unwrap());
        let pixels = fill.pixels_mut().unwrap();
        let (_, raster) = measure(|| {
            renderer
                .render_source_into(
                    &mut scene.scene,
                    ImageBufferTarget {
                        data: &mut *pixels,
                        width: scene.width,
                        height: scene.height,
                        bytes_per_row: scene.width as usize * 4,
                        format: ImageBufferFormat::Rgba8Unorm,
                    },
                )
                .unwrap()
        });
        let (_, convert) = measure(|| crate::protocol::rgba8_to_premultiplied_bgra(pixels));
        let (checksum, checksum_span) =
            measure(|| crate::service::pixel_checksum(black_box(&*pixels)));
        assert_eq!(
            checksum,
            expected_checksum(scene.height),
            "saved scene checksum differs"
        );
        if check_reference {
            // Same private production function, not a second conversion implementation.
            let mut reference = vec![0xa5; pixels.len()];
            crate::render::cpu::render_into(&mut renderer, &mut scene, &mut reference).unwrap();
            assert_eq!(pixels, reference);
        }
        let (_, dispose) = measure(|| drop((scene, targets)));
        done.send(Reply {
            fill,
            phases: [prepare, raster, convert, checksum_span, dispose],
            checksum,
        })
        .unwrap();
    }
}

fn run(height: u32, warmup: usize, samples: usize, emit: bool) {
    let model = model_for(height);
    let mut ring = ready_ring_for(height);
    let (send, incoming) = mpsc::sync_channel(1);
    let (done, receive) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || worker(incoming, done));
    let mut retained: Option<RingOffer> = None;
    let mut rows = Vec::new();
    for index in 0..warmup + samples {
        let now = 10 + index as u64 * 10;
        let (fill, acquire) = measure(|| {
            ring.acquire_owned_slot((WIDTH * height * 4) as usize, now)
                .unwrap()
        });
        let (model, clone) = measure(|| model.clone());
        send.send((
            RenderJob {
                identity: identity(),
                fill: Some(fill),
                model,
                width: WIDTH,
                height,
                scale: 1.0,
            },
            index == 0,
        ))
        .unwrap();
        let Reply {
            mut fill,
            phases,
            checksum,
        } = receive.recv_timeout(Duration::from_secs(30)).unwrap();
        let upload = RingUpload {
            width: WIDTH,
            height,
            resource: index as u64 + 1,
            ..ring_upload()
        };
        let ((id, wire), commit) =
            measure(|| ring.commit_owned_slot(&mut fill, upload, now + 1).unwrap());
        assert!(fill.pixels_mut().is_err());
        let RingControl::Offer(offer) = RingControl::decode(&wire).unwrap() else {
            panic!("expected ring Offer");
        };
        assert_eq!(id, index as u64 + 1);
        assert_eq!((offer.width, offer.height), (WIDTH, height));
        if let Some(old) = retained.replace(offer) {
            assert_ne!(old.slot, offer.slot, "retained slot reused before release");
            assert!(matches!(
                ring.acquire_owned_slot((WIDTH * height * 4) as usize, now + 2),
                Err(sophia_image_producer::ProducerError::Busy)
            ));
            release(&mut ring, old, now + 3);
        }
        if index >= warmup {
            rows.push((index - warmup, acquire, clone, phases, commit, checksum));
        }
    }
    release(
        &mut ring,
        retained.unwrap(),
        10 + (warmup + samples) as u64 * 10,
    );
    drop(send);
    worker.join().unwrap();
    assert_eq!(ring.offers().count(), 0);
    if emit {
        for (index, acquire, clone, phases, commit, checksum) in rows {
            for (phase, owner, span) in [
                ("acquire", "owner", acquire),
                ("model_clone", "owner", clone),
                ("scene", "worker", phases[0]),
                ("raster_rgba", "worker", phases[1]),
                ("convert_bgra", "worker", phases[2]),
                ("checksum", "worker", phases[3]),
                ("scene_drop", "worker", phases[4]),
                ("producer_commit", "owner", commit),
            ] {
                println!(
                    "lom_redraw_phase schema=1 height={height} index={index} phase={phase} thread={owner} cpu_ns={} wall_ns={} checksum={checksum:016x}",
                    span.cpu_ns, span.wall_ns
                );
            }
        }
    }
}

#[test]
fn split_matches_saved_scene_and_production_with_real_loans() {
    for height in [24, 512] {
        run(height, 1, 3, false);
    }
}

#[test]
fn optional_hash_preserves_both_saved_scenes_and_ring_custody() {
    for height in [24, 512] {
        let mut reference = None;
        for diagnostic in [false, true] {
            let mut ring = ready_ring_for(height);
            let mut hosts = RetainedHosts::default();
            let mut backend = Backend::Cpu(VelloCpuRenderer::new(1, 1), diagnostic);
            for frame in 0..3 {
                let fill = ring
                    .acquire_owned_slot((WIDTH * height * 4) as usize, 3 + frame)
                    .unwrap();
                let content = render(
                    &mut backend,
                    &mut hosts,
                    RenderJob {
                        identity: identity(),
                        fill: Some(fill),
                        model: model_for(height),
                        width: WIDTH,
                        height,
                        scale: 1.0,
                    },
                )
                .unwrap();
                let RenderImage::RingFill {
                    mut fill, checksum, ..
                } = content.pixels
                else {
                    panic!("CPU render must return its loan");
                };
                let bytes = fill.pixels_mut().unwrap();
                assert_eq!(
                    crate::service::pixel_checksum(bytes),
                    expected_checksum(height)
                );
                assert_eq!(checksum, diagnostic.then(|| expected_checksum(height)));
                if let Some(expected) = &reference {
                    assert_eq!(bytes, expected);
                } else {
                    reference = Some(bytes.to_vec());
                }
                ring.abort_owned_slot(&mut fill).unwrap();
                assert_eq!(ring.offers().count(), 0);
            }
        }
    }
}

#[test]
#[ignore = "offscreen attribution; run explicitly in release with no concurrent builds"]
fn measure_redraw_phases() {
    if cfg!(debug_assertions) {
        panic!("attribution requires --release");
    }
    for index in 0..200 {
        let (_, span) = measure(|| black_box(()));
        println!(
            "lom_redraw_timer schema=1 index={index} cpu_ns={} wall_ns={}",
            span.cpu_ns, span.wall_ns
        );
    }
    for height in [24, 512] {
        run(height, 120, 200, true);
    }
}

#[test]
#[ignore = "offscreen checksum policy; run explicitly in release with no concurrent builds"]
fn measure_checksum_policy() {
    if cfg!(debug_assertions) {
        panic!("attribution requires --release");
    }
    let mut rows = Vec::new();
    for height in [24, 512] {
        for (block, diagnostic) in [true, false, false, true].into_iter().enumerate() {
            let mut ring = ready_ring_for(height);
            let mut hosts = RetainedHosts::default();
            let mut backend = Backend::Cpu(VelloCpuRenderer::new(1, 1), diagnostic);
            for index in 0..320 {
                let fill = ring
                    .acquire_owned_slot((WIDTH * height * 4) as usize, 3 + index)
                    .unwrap();
                let job = RenderJob {
                    identity: identity(),
                    fill: Some(fill),
                    model: model_for(height),
                    width: WIDTH,
                    height,
                    scale: 1.0,
                };
                let (content, span) = measure(|| render(&mut backend, &mut hosts, job).unwrap());
                let RenderImage::RingFill {
                    mut fill, checksum, ..
                } = content.pixels
                else {
                    panic!("CPU render must return its loan");
                };
                // Validate both policies after the measured production call.
                assert_eq!(
                    crate::service::pixel_checksum(fill.pixels_mut().unwrap()),
                    expected_checksum(height)
                );
                assert_eq!(checksum, diagnostic.then(|| expected_checksum(height)));
                ring.abort_owned_slot(&mut fill).unwrap();
                if index >= 120 {
                    rows.push((height, block, diagnostic, index - 120, span));
                }
            }
        }
    }
    for (height, block, diagnostic, index, span) in rows {
        println!(
            "lom_checksum_policy schema=1 height={height} block={block} diagnostic={diagnostic} index={index} cpu_ns={} wall_ns={}",
            span.cpu_ns, span.wall_ns
        );
    }
}
