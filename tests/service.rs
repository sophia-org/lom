//! Real-socket lifecycle coverage for the persistent content service.

use lom::{
    config::parse_shell_config,
    model::Model,
    protocol::ContentPixels,
    service::{ContentRenderer, ShellService},
};
use sophia_protocol::{
    ContentAllocationId, ContentAllocationResult, ContentFramePermit, ContentGrant,
    ContentLogicalRect, ContentMargins, ContentOutputFacts, ContentOutputFactsEntry,
    ContentOutputId, ContentPixelRect, ContentReason, ContentResourceId, ContentResourceStatus,
    OutputId, POLICY_INDICATOR_STATE_ACTIVE, SOPHIA_IPC_HEADER_LEN,
    SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE, SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER,
    SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS, ShellContentRecord, ShellIndicator,
    ShellIndicatorSnapshot, ShellV1ClientHello, ShellV1ServerWelcome, TransactionId, decode_frame,
    decode_shell_content_frame, decode_shell_v1_client_hello_frame, encode_shell_content_frame,
    encode_shell_indicator_snapshot, encode_shell_v1_server_welcome_frame,
};
use sophia_shell_client::{ShellClientOptions, ShellConnection};
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static SOCKET_ID: AtomicU64 = AtomicU64::new(1);
const GRANT: ContentGrant = ContentGrant {
    connection_epoch: 7,
    content_grant_epoch: 9,
};
const OUTPUT: ContentOutputId = ContentOutputId {
    id: 2,
    generation: 3,
};

struct FixedRenderer {
    calls: usize,
    completed: Option<ContentPixels>,
    started: Option<std::sync::mpsc::Sender<()>>,
    release: std::sync::mpsc::Receiver<()>,
    gate_first: bool,
}
impl ContentRenderer for FixedRenderer {
    fn submit(&mut self, model: Model, width: u32, height: u32, scale: f64) -> Result<(), String> {
        assert_eq!((width, height, scale), (8, 48, 2.0));
        self.calls += 1;
        if let Some(started) = self.started.take() {
            started.send(()).unwrap();
        }
        let expected_generation = 5 + self.calls as u64;
        assert_eq!(model.workspaces.generation, expected_generation);
        assert_eq!(model.workspaces.active_output, Some(OUTPUT.id));
        assert_eq!(
            model.workspaces.entries[0].name,
            if self.calls == 1 { "one" } else { "two" }
        );
        assert!(model.workspaces.entries[0].active);
        self.completed = Some(ContentPixels::from_rgba8(
            width,
            height,
            vec![255; (width * height * 4) as usize],
        )?);
        Ok(())
    }

    fn poll(&mut self) -> Result<Option<ContentPixels>, String> {
        if self.gate_first {
            if self.release.try_recv().is_err() {
                return Ok(None);
            }
            self.gate_first = false;
        }
        Ok(self.completed.take())
    }
}

#[test]
fn persistent_service_negotiates_allocates_uploads_and_waits_for_native_presentation() {
    let path = std::env::temp_dir().join(format!(
        "lom-serve-{}-{}.sock",
        std::process::id(),
        SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    let (done_sender, done_receiver) = std::sync::mpsc::channel();
    let (started_sender, started_receiver) = std::sync::mpsc::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        serve_one(
            listener.accept().unwrap().0,
            done_receiver,
            started_receiver,
            release_sender,
        )
    });
    let capabilities = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS;
    let connection = ShellConnection::connect(
        &path,
        ShellClientOptions {
            minimum_revision: 6,
            maximum_revision: 6,
            required_capabilities: capabilities,
            handshake_timeout: Duration::from_secs(2),
        },
    )
    .unwrap();
    let (config, theme) =
        parse_shell_config(include_str!("../examples/minimal/shell.kdl")).unwrap();
    let mut service = ShellService::new(
        connection,
        config,
        theme,
        48,
        FixedRenderer {
            calls: 0,
            completed: None,
            started: Some(started_sender),
            release: release_receiver,
            gate_first: true,
        },
    )
    .unwrap();
    let mut presented = 0;
    while presented < 2 {
        presented += service.step().unwrap();
    }
    done_sender.send(()).unwrap();
    server.join().unwrap();
    let _ = std::fs::remove_file(path);
}

fn serve_one(
    mut stream: UnixStream,
    done: std::sync::mpsc::Receiver<()>,
    render_started: std::sync::mpsc::Receiver<()>,
    render_release: std::sync::mpsc::Sender<()>,
) {
    let hello = read_frame(&mut stream);
    let ShellV1ClientHello {
        minimum_revision,
        maximum_revision,
        required_capabilities,
    } = decode_shell_v1_client_hello_frame(&hello).unwrap();
    assert_eq!((minimum_revision, maximum_revision), (6, 6));
    let expected = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS;
    assert_eq!(required_capabilities, expected);
    stream
        .write_all(
            &encode_shell_v1_server_welcome_frame(ShellV1ServerWelcome {
                selected_revision: 6,
                capabilities: expected,
                connection_epoch: GRANT.connection_epoch,
                max_descriptors: 16,
                max_label_bytes: 32,
                max_pending_activations: 16,
            })
            .unwrap(),
        )
        .unwrap();
    let mut startup = encode_shell_content_frame(
        TransactionId::from_raw(0),
        &ShellContentRecord::Limits(sophia_protocol::ContentLimits::prototype(GRANT)),
    )
    .unwrap();
    startup.extend(
        encode_shell_indicator_snapshot(
            TransactionId::from_raw(3),
            &ShellIndicatorSnapshot {
                connection_epoch: GRANT.connection_epoch,
                generation: 6,
                active_output: Some(OutputId::from_raw(OUTPUT.id)),
                statuses: Vec::new(),
                indicators: vec![ShellIndicator {
                    output: OutputId::from_raw(OUTPUT.id),
                    indicator: 14,
                    action: 15,
                    slot: 0,
                    state_bits: POLICY_INDICATOR_STATE_ACTIVE,
                    label: "one".into(),
                }],
            },
        )
        .unwrap()
        .into_iter()
        .flatten(),
    );
    startup.extend(
        encode_shell_content_frame(
            TransactionId::from_raw(2),
            &ShellContentRecord::OutputFacts(ContentOutputFacts {
                grant: GRANT,
                facts_generation: 4,
                outputs: vec![ContentOutputFactsEntry {
                    output: OUTPUT,
                    local_width: 4,
                    local_height: 32,
                    scale_numerator: 2,
                    scale_denominator: 1,
                    scale_generation: 5,
                }],
            }),
        )
        .unwrap(),
    );
    stream.write_all(&startup).unwrap();

    let (transaction, ShellContentRecord::AllocationRequest(request)) = receive(&mut stream) else {
        panic!("expected allocation request");
    };
    assert_eq!((request.desired_width, request.desired_height), (4, 24));
    let allocation = ContentAllocationId {
        id: 11,
        generation: 1,
    };
    send_tx(
        &mut stream,
        transaction,
        ShellContentRecord::AllocationResult(ContentAllocationResult {
            grant: GRANT,
            allocation_request_id: request.allocation_request_id,
            status: 1,
            reason: ContentReason::None as u16,
            output: OUTPUT,
            allocation,
            parent: ContentAllocationId::default(),
            scale_generation: 5,
            logical: ContentLogicalRect {
                x: 0,
                y: 0,
                width: 4,
                height: 24,
            },
            pixel: ContentPixelRect {
                x: 0,
                y: 0,
                width: 8,
                height: 48,
            },
            scale_numerator: 2,
            scale_denominator: 1,
            allowed_reservation_extent: 48,
            margins: ContentMargins::default(),
            acknowledged_anchor: ContentPixelRect::default(),
        }),
    );

    render_started.recv_timeout(Duration::from_secs(2)).unwrap();
    for frame in encode_shell_indicator_snapshot(
        TransactionId::from_raw(50),
        &ShellIndicatorSnapshot {
            connection_epoch: GRANT.connection_epoch,
            generation: 7,
            active_output: Some(OutputId::from_raw(OUTPUT.id)),
            statuses: Vec::new(),
            indicators: vec![ShellIndicator {
                output: OutputId::from_raw(OUTPUT.id),
                indicator: 14,
                action: 15,
                slot: 0,
                state_bits: POLICY_INDICATOR_STATE_ACTIVE,
                label: "two".into(),
            }],
        },
    )
    .unwrap()
    {
        stream.write_all(&frame).unwrap();
    }
    render_release.send(()).unwrap();
    let first = serve_frame(&mut stream, allocation, 1, 22, 31);
    let second = serve_frame(&mut stream, allocation, 2, 23, 32);
    assert_ne!(first, second, "a live resource cannot be overwritten");
    let (retire_tx, ShellContentRecord::ResourceRetire(retire)) = receive(&mut stream) else {
        panic!("expected resource retirement after its successor presented");
    };
    assert_eq!(retire.resource, first);
    send_tx(
        &mut stream,
        retire_tx,
        ShellContentRecord::ResourceReleased(sophia_protocol::ContentResourceReleased {
            grant: GRANT,
            resource: first,
            reason: ContentReason::None as u16,
        }),
    );
    done.recv_timeout(Duration::from_secs(2)).unwrap();
}

fn serve_frame(
    stream: &mut UnixStream,
    allocation: ContentAllocationId,
    expected_candidate: u64,
    permit_id: u64,
    presentation_epoch: u64,
) -> ContentResourceId {
    let (upload_tx, ShellContentRecord::ResourceBegin(begin)) = receive(stream) else {
        panic!("expected resource begin");
    };
    assert_eq!(
        (begin.width_px, begin.height_px, begin.total_bytes),
        (8, 48, 1536)
    );
    send_tx(
        stream,
        upload_tx,
        ShellContentRecord::ResourceStatus(ContentResourceStatus {
            grant: GRANT,
            resource: begin.resource,
            status: 1,
            reason: ContentReason::None as u16,
            next_ordinal: 0,
            admitted_bytes: 1536,
        }),
    );
    let (_, ShellContentRecord::ResourceChunk(chunk)) = receive(stream) else {
        panic!("expected resource chunk only after transfer admission");
    };
    assert_eq!(chunk.bytes.len(), 1536);
    let (_, ShellContentRecord::ResourceEnd(end)) = receive(stream) else {
        panic!("expected resource end");
    };
    assert_eq!(end.resource, begin.resource);
    send_tx(
        stream,
        upload_tx,
        ShellContentRecord::ResourceStatus(ContentResourceStatus {
            grant: GRANT,
            resource: begin.resource,
            status: 2,
            reason: ContentReason::None as u16,
            next_ordinal: 1,
            admitted_bytes: 1536,
        }),
    );
    let (demand_tx, ShellContentRecord::FrameDemand(demand)) = receive(stream) else {
        panic!("expected frame demand");
    };
    assert_eq!(demand.allocation, allocation);
    send_tx(
        stream,
        demand_tx,
        ShellContentRecord::FramePermit(ContentFramePermit {
            grant: GRANT,
            output: OUTPUT,
            demand_id: demand.demand_id,
            permit_id,
            state: 1,
            reason: ContentReason::None as u16,
            ttl_ms: 250,
            max_candidate_bytes: 8192,
        }),
    );
    let (_, ShellContentRecord::CandidateBegin(candidate)) = receive(stream) else {
        panic!("expected candidate begin");
    };
    let (_, ShellContentRecord::CandidateChunk(chunk)) = receive(stream) else {
        panic!("expected candidate chunk");
    };
    let (_, ShellContentRecord::CandidateEnd(end)) = receive(stream) else {
        panic!("expected candidate end");
    };
    assert_eq!(candidate.candidate_generation, expected_candidate);
    assert_eq!(candidate.pacing_permit, permit_id);
    assert_eq!(candidate.target_count, 0);
    assert_eq!(chunk.surfaces[0].reservation_extent, 48);
    assert_eq!(chunk.placements[0].resource, begin.resource);
    assert_eq!(end.candidate_generation, candidate.candidate_generation);
    for (kind, epoch) in [(1, 0), (2, presentation_epoch)] {
        send(
            stream,
            60 + expected_candidate * 2 + u64::from(kind),
            ShellContentRecord::CandidateOutcome(sophia_protocol::ContentCandidateOutcome {
                grant: GRANT,
                candidate_generation: candidate.candidate_generation,
                output: OUTPUT,
                kind,
                reason: ContentReason::None as u16,
                presentation_epoch: epoch,
                work_area_generation: 8 + expected_candidate,
                wm_commit_generation: 9 + expected_candidate,
            }),
        );
    }
    begin.resource
}

fn send(stream: &mut UnixStream, transaction: u64, record: ShellContentRecord) {
    send_tx(stream, TransactionId::from_raw(transaction), record);
}

fn send_tx(stream: &mut UnixStream, transaction: TransactionId, record: ShellContentRecord) {
    stream
        .write_all(&encode_shell_content_frame(transaction, &record).unwrap())
        .unwrap();
}

fn receive(stream: &mut UnixStream) -> (TransactionId, ShellContentRecord) {
    decode_shell_content_frame(&read_frame(stream)).unwrap()
}

fn read_frame(stream: &mut UnixStream) -> Vec<u8> {
    let mut header = [0; SOPHIA_IPC_HEADER_LEN];
    stream.read_exact(&mut header).unwrap();
    let payload = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
    let mut frame = header.to_vec();
    frame.resize(SOPHIA_IPC_HEADER_LEN + payload, 0);
    stream
        .read_exact(&mut frame[SOPHIA_IPC_HEADER_LEN..])
        .unwrap();
    decode_frame(&frame).unwrap();
    frame
}
