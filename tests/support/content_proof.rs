//! The diagnostic executable also uses the file SDK; no GPU is initialized.
use super::*;

#[test]
fn serve_reports_the_file_transport_before_requiring_a_gpu_grant() {
    let path = std::env::temp_dir().join(format!(
        "lom-serve-{}-{}.sock",
        std::process::id(),
        SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    let (done, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let _peer = Peer::new(
            listener.accept().unwrap().0,
            SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
                | SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
                | SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT
                | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
                | SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION,
        );
        wait.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lom"))
        .arg("--serve")
        .env_clear()
        .env("SOPHIA_SHELL_9P_SOCKET", &path)
        .env(
            "SOPHIA_SHELL_CONFIG",
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/minimal/shell.kdl"),
        )
        .env("SOPHIA_SHELL_BAR_THICKNESS", "48")
        .output()
        .unwrap();
    done.send(()).unwrap();
    server.join().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "lom_shell_transport schema=1 wire=9p2000.L revision=6 epoch=41\n"
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("Sophia did not grant direct GPU access")
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn diagnostic_uploads_on_9p_and_reports_only_renderer_failure() {
    let path = std::env::temp_dir().join(format!(
        "lom-proof-{}-{}.sock",
        std::process::id(),
        SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    let (done, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut peer = Peer::new(
            listener.accept().unwrap().0,
            SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE | SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER,
        );
        send(
            &mut peer,
            2,
            ShellContentRecord::OutputFacts(ContentOutputFacts {
                grant: GRANT,
                facts_generation: 4,
                outputs: vec![ContentOutputFactsEntry {
                    output: OUTPUT,
                    local_width: 800,
                    local_height: 600,
                    scale_numerator: 1,
                    scale_denominator: 1,
                    scale_generation: 5,
                }],
            }),
        );
        let (tx, ShellContentRecord::AllocationRequest(request)) = receive(&mut peer) else {
            panic!("allocation")
        };
        assert_eq!((request.desired_width, request.desired_height), (800, 32));
        let allocation = ContentAllocationId {
            id: 11,
            generation: 1,
        };
        send_tx(
            &mut peer,
            tx,
            ShellContentRecord::AllocationResult(ContentAllocationResult {
                grant: GRANT,
                allocation_request_id: request.allocation_request_id,
                status: 1,
                reason: 0,
                output: OUTPUT,
                allocation,
                parent: ContentAllocationId::default(),
                scale_generation: 5,
                logical: ContentLogicalRect {
                    x: 0,
                    y: 0,
                    width: 800,
                    height: 32,
                },
                pixel: ContentPixelRect {
                    x: 0,
                    y: 0,
                    width: 800,
                    height: 32,
                },
                scale_numerator: 1,
                scale_denominator: 1,
                allowed_reservation_extent: 32,
                margins: ContentMargins::default(),
                acknowledged_anchor: ContentPixelRect::default(),
            }),
        );
        let (tx, ShellContentRecord::ResourceBegin(begin)) = receive(&mut peer) else {
            panic!("begin")
        };
        assert_eq!(
            (begin.width_px, begin.height_px, begin.total_bytes),
            (2, 1, 8)
        );
        send_tx(
            &mut peer,
            tx,
            ShellContentRecord::ResourceStatus(ContentResourceStatus {
                grant: GRANT,
                resource: begin.resource,
                status: 1,
                reason: 0,
                next_ordinal: 0,
                admitted_bytes: 8,
            }),
        );
        let (_, ShellContentRecord::ResourceChunk(chunk)) = receive(&mut peer) else {
            panic!("bytes")
        };
        assert_eq!(chunk.bytes.len(), 8);
        let (_, ShellContentRecord::ResourceEnd(end)) = receive(&mut peer) else {
            panic!("end")
        };
        assert_eq!(end.resource, begin.resource);
        send_tx(
            &mut peer,
            tx,
            ShellContentRecord::ResourceStatus(ContentResourceStatus {
                grant: GRANT,
                resource: begin.resource,
                status: 2,
                reason: 0,
                next_ordinal: 1,
                admitted_bytes: 8,
            }),
        );
        let (tx, ShellContentRecord::FrameDemand(demand)) = receive(&mut peer) else {
            panic!("demand")
        };
        send_tx(
            &mut peer,
            tx,
            ShellContentRecord::FramePermit(ContentFramePermit {
                grant: GRANT,
                output: OUTPUT,
                demand_id: demand.demand_id,
                permit_id: 22,
                state: 1,
                reason: 0,
                ttl_ms: 250,
                max_candidate_bytes: 8192,
            }),
        );
        let (tx, ShellContentRecord::CandidateBegin(candidate)) = receive(&mut peer) else {
            panic!("candidate")
        };
        let (_, ShellContentRecord::CandidateChunk(chunk)) = receive(&mut peer) else {
            panic!("candidate rows")
        };
        let (_, ShellContentRecord::CandidateEnd(_)) = receive(&mut peer) else {
            panic!("candidate end")
        };
        assert_eq!(chunk.placements[0].resource, begin.resource);
        send_tx(
            &mut peer,
            tx,
            ShellContentRecord::CandidateOutcome(sophia_shell_protocol::ContentCandidateOutcome {
                grant: GRANT,
                candidate_generation: candidate.candidate_generation,
                output: OUTPUT,
                kind: 3,
                reason: ContentReason::RendererFailed as u16,
                presentation_epoch: 0,
                work_area_generation: 0,
                wm_commit_generation: 0,
            }),
        );
        let (tx, ShellContentRecord::ResourceRetire(retire)) = receive(&mut peer) else {
            panic!("retire")
        };
        assert_eq!(retire.resource, begin.resource);
        send_tx(
            &mut peer,
            tx,
            ShellContentRecord::ResourceReleased(sophia_shell_protocol::ContentResourceReleased {
                grant: GRANT,
                resource: retire.resource,
                reason: 0,
            }),
        );
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lom"))
        .args(["content-proof", "--socket"])
        .arg(&path)
        .env_clear()
        .output()
        .unwrap();
    done.send(()).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("native_presentation=false")
    );
    std::fs::remove_file(path).unwrap();
}
