//! Exercise checksum selection through the actual binary-entry renderer dispatch.
use super::*;
use crate::service::{RenderIdentity, RenderImage};
use sophia_image_producer::{RingProducer, RingProducerConfig};
use sophia_shell_protocol::shell_files::{RingEvent, RingEventKind, RingIdentity};

#[test]
fn each_entry_selects_its_checksum_policy_for_the_same_cpu_pixels() {
    let entries = [
        (Entry::Ordinary, false),
        #[cfg(feature = "diagnostic-cpu-reference")]
        (Entry::CpuReference, true),
        #[cfg(feature = "diagnostic-cpu-reference")]
        (Entry::CpuCadence, true),
        #[cfg(feature = "diagnostic-cpu-reference")]
        (Entry::CpuCadenceNoChecksum, false),
    ];
    let mut baseline = None;
    for (entry, diagnostic) in entries {
        let identity = RingIdentity {
            connection_epoch: 1,
            content_grant_epoch: 1,
            channel_generation: 1,
            ring: 1,
            generation: 1,
        };
        let mut ring = RingProducer::new(
            RingProducerConfig {
                identity,
                slots: 1,
                capacity: 0,
                max_resource_bytes: 64 * 24 * 4,
                transfer_timeout_ms: 2000,
            },
            0,
        )
        .unwrap();
        assert!(ring.registration().is_some());
        let event = |kind| RingEvent {
            identity,
            kind,
            sequence: 1,
            offer_id: 0,
            slot: 0,
            slot_generation: 0,
            resource: 0,
            resource_generation: 0,
            reason: 0,
            changed_generation: false,
        };
        ring.ring_event(event(RingEventKind::RingAdmitted), 1)
            .unwrap();
        assert!(ring.source_packet().is_some());
        ring.source_sent().unwrap();
        ring.ring_event(event(RingEventKind::RingReady), 2).unwrap();
        let loan = ring.acquire_owned_slot(64 * 24 * 4, 3).unwrap();
        let stop = AtomicBool::new(false);
        let (wake, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let mut worker = start_renderer(
            entry,
            1,
            false,
            crate::config::Renderer::Cpu,
            &stop,
            wake.as_fd(),
        )
        .unwrap();
        let (config, theme) =
            parse_shell_config(include_str!("../../examples/minimal/shell.kdl")).unwrap();
        let model = crate::model::Model::new(
            config,
            theme,
            1,
            chrono::DateTime::UNIX_EPOCH.fixed_offset(),
        );
        let render_identity = RenderIdentity {
            grant: ContentGrant {
                connection_epoch: 1,
                content_grant_epoch: 1,
            },
            output: ContentOutputId {
                id: 1,
                generation: 1,
            },
            allocation: ContentAllocationId {
                id: 1,
                generation: 1,
            },
            scale_generation: 1,
        };
        worker
            .submit_fill(render_identity, model, 64, 24, 1.0, loan)
            .map_err(|(error, _)| error)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let image = loop {
            if let Some(content) = worker.poll().unwrap() {
                break content.pixels;
            }
            assert!(Instant::now() < deadline, "CPU entry never completed");
            std::thread::sleep(Duration::from_millis(1));
        };
        let RenderImage::RingFill {
            mut fill, checksum, ..
        } = image
        else {
            panic!("CPU entry must return a ring fill");
        };
        let bytes = fill.pixels_mut().unwrap().to_vec();
        assert_eq!(
            checksum,
            diagnostic.then(|| crate::service::pixel_checksum(&bytes)),
            "{entry:?}"
        );
        assert!(bytes.iter().any(|byte| *byte != 0));
        if let Some(expected) = &baseline {
            assert_eq!(&bytes, expected);
        } else {
            baseline = Some(bytes);
        }
        ring.abort_owned_slot(&mut fill).unwrap();
    }
}

#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
#[ignore = "subprocess entry; run by exact_entry_records"]
fn entry_records_child() {
    let entry = match std::env::var("LOM_ENTRY_RECORD_TEST").unwrap().as_str() {
        "ordinary" => Entry::Ordinary,
        "reference" => Entry::CpuReference,
        "cadence" => Entry::CpuCadence,
        "no-checksum" => Entry::CpuCadenceNoChecksum,
        _ => panic!("unknown test entry"),
    };
    let stop = AtomicBool::new(false);
    let (wake, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let worker = start_renderer(
        entry,
        1,
        false,
        crate::config::Renderer::Cpu,
        &stop,
        wake.as_fd(),
    )
    .unwrap();
    let pixels = crate::service::cpu_record_session_for_tests(worker, |service, stop| {
        drive(entry, service, stop)
    });
    let hash = pixels.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    println!("lom_test_received_pixels fnv64={hash:016x}");
}

#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn exact_entry_records() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for (name, hashed, cadence) in [
        ("ordinary", false, false),
        ("reference", true, false),
        ("cadence", true, true),
        ("no-checksum", false, true),
    ] {
        let root = std::env::temp_dir().join(format!("lr-{}-{nanos}-{name}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let log = std::fs::File::create(root.join("log")).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cli::serve::tests::checksum_entry::entry_records_child",
                "--ignored",
                "--nocapture",
            ])
            .env_clear()
            .env("TMPDIR", &root)
            .env("LOM_ENTRY_RECORD_TEST", name)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut process = Process { child, root };
        wait_until(|| process.child.try_wait().unwrap().is_some());
        let text = std::fs::read_to_string(process.root.join("log")).unwrap();
        assert!(process.child.wait().unwrap().success(), "{name}: {text}");
        let rows: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with("lom_"))
            .collect();
        let received = rows
            .iter()
            .find_map(|line| line.strip_prefix("lom_test_received_pixels fnv64="))
            .unwrap();
        assert_eq!(received.len(), 16);
        assert!(
            received
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        );
        let checksum = if hashed { received } else { "unavailable" };
        let candidate = format!(
            "lom_panel_candidate schema={} status=presented connection_epoch=41 content_grant_epoch=9 output=2 candidate_generation=1 presentation_epoch=31 indicator_generation=0 width=8 height=48 bytes=1536 checksum={checksum} raster_source=rendered",
            if hashed { 1 } else { 2 }
        );
        let renderer =
            "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used";
        let mut expected = vec![if name == "ordinary" {
            renderer.to_owned()
        } else {
            format!("{renderer} diagnostic=cpu-reference")
        }];
        if name == "no-checksum" {
            expected.push(
                "lom_checksum_policy schema=1 checksum=unavailable diagnostic=cpu-cadence".into(),
            );
        }
        if cadence {
            expected.push("lom_cadence schema=1 mode=redirty-after-presented quota=2120 diagnostic=cpu-reference".into());
        }
        expected.push(candidate);
        let selected: Vec<_> = rows
            .iter()
            .filter(|line| {
                [
                    "lom_renderer ",
                    "lom_checksum_policy ",
                    "lom_cadence ",
                    "lom_panel_candidate ",
                ]
                .iter()
                .any(|tag| line.starts_with(tag))
            })
            .copied()
            .collect();
        assert_eq!(selected, expected, "{name}: {text}");
        assert_eq!(
            rows.iter()
                .filter(|line| line.starts_with("lom_cadence_sample "))
                .count(),
            usize::from(cadence)
        );
    }
}
