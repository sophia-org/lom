//! Exercise checksum selection through the actual binary-entry renderer dispatch.
use super::*;
use crate::service::{RenderIdentity, RenderImage};
use sophia_image_producer::{RingProducer, RingProducerConfig};
use sophia_shell_protocol::shell_files::{RingEvent, RingEventKind, RingIdentity};

#[test]
fn only_diagnostic_entries_hash_the_same_cpu_pixels() {
    let entries = [
        (Entry::Ordinary, false),
        #[cfg(feature = "diagnostic-cpu-reference")]
        (Entry::CpuReference, true),
        #[cfg(feature = "diagnostic-cpu-reference")]
        (Entry::CpuCadence, true),
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
