//! File-contract peer for the service assertions. Only 9P bytes cross the
//! socket. Whole candidates are split into the SDK's neutral records for the
//! existing lifecycle assertions; upload bytes come from actual slot writes.

use super::files_wire::{EPOCH, Wire};
use sophia_shell_protocol::{shell_files::*, *};
use std::{
    collections::HashMap,
    os::unix::net::UnixStream,
    sync::mpsc,
    time::{Duration, Instant},
};

pub enum Message {
    Content(TransactionId, Box<ShellContentRecord>),
    Activation(TransactionId, ShellIndicatorActivation),
}

enum Command {
    Content(TransactionId, ShellContentRecord),
    Indicators(TransactionId, ShellIndicatorSnapshot),
    Activation(TransactionId, ShellIndicatorActivationOutcome),
    Stop,
}

pub struct Peer {
    command: mpsc::Sender<Command>,
    messages: mpsc::Receiver<Message>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    pub fn new(stream: UnixStream, capabilities: u64) -> Self {
        Self::with_refusal(stream, capabilities, None)
    }

    pub fn with_refusal(
        stream: UnixStream,
        capabilities: u64,
        refuse: Option<ShellFileKind>,
    ) -> Self {
        let (command, commands) = mpsc::channel();
        let (messages, received) = mpsc::channel();
        let worker =
            std::thread::spawn(move || run(stream, capabilities, refuse, commands, messages));
        Self {
            command,
            messages: received,
            worker: Some(worker),
        }
    }

    pub fn content(&self, tx: TransactionId, record: ShellContentRecord) {
        self.command.send(Command::Content(tx, record)).unwrap();
    }

    pub fn indicators(&self, tx: TransactionId, snapshot: ShellIndicatorSnapshot) {
        self.command
            .send(Command::Indicators(tx, snapshot))
            .unwrap();
    }

    pub fn activation(&self, tx: TransactionId, outcome: ShellIndicatorActivationOutcome) {
        self.command.send(Command::Activation(tx, outcome)).unwrap();
    }

    pub fn receive(&self) -> Message {
        self.messages.recv_timeout(Duration::from_secs(3)).unwrap()
    }

    pub fn no_message(&self) {
        assert!(matches!(
            self.messages.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.command.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

fn object_header(kind: ShellFileKind) -> ShellFileHeader {
    ShellFileHeader {
        kind,
        connection_epoch: EPOCH,
        submission_id: 0,
        sequence: 0,
    }
}

fn run(
    stream: UnixStream,
    capabilities: u64,
    refuse: Option<ShellFileKind>,
    commands: mpsc::Receiver<Command>,
    messages: mpsc::Sender<Message>,
) {
    let mut wire = Wire::new(stream);
    let limits = ContentLimits::prototype(super::GRANT);
    wire.object(
        "limits",
        15,
        encode_shell_file_limits(object_header(ShellFileKind::Limits), limits.clone()).unwrap(),
    );
    let mut slots = HashMap::new();
    let mut negotiated = false;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline, "scripted file peer deadline");
        while negotiated && let Ok(command) = commands.try_recv() {
            match command {
                Command::Stop => return,
                Command::Content(_, ShellContentRecord::Limits(value)) => assert_eq!(value, limits),
                Command::Content(tx, record) => emit(&mut wire, tx, record),
                Command::Indicators(transaction, snapshot) => {
                    let generation = snapshot.generation;
                    let bytes = encode_shell_file_indicators(
                        object_header(ShellFileKind::Indicators),
                        &ShellFileIndicators {
                            transaction,
                            snapshot,
                        },
                    )
                    .unwrap();
                    wire.object("indicators", 100 + generation, bytes);
                    wire.announce(ShellFileKind::Indicators, generation, 100 + generation);
                }
                Command::Activation(transaction, outcome) => {
                    let body = encode_shell_file_indicator_activation_outcome_body(
                        &ShellFileIndicatorActivationOutcome {
                            transaction,
                            outcome,
                        },
                    )
                    .unwrap();
                    wire.event(ShellFileKind::IndicatorActivationOutcome, &body);
                }
            }
        }
        if !wire.pump() {
            return;
        }
        while let Some(submission) = wire.submissions.pop_front() {
            let bytes = &submission.bytes;
            let kind = decode_shell_file_record(bytes, ShellFileClass::Candidate)
                .unwrap()
                .header
                .kind;
            if Some(kind) == refuse {
                wire.refuse(&submission);
                continue;
            }
            wire.accept(&submission, kind);
            let value = match kind {
                ShellFileKind::Negotiate => {
                    let hello = decode_shell_file_negotiate(bytes).unwrap();
                    assert_eq!(
                        (
                            hello.minimum_revision,
                            hello.maximum_revision,
                            hello.required_capabilities
                        ),
                        (6, 6, capabilities)
                    );
                    let body = encode_shell_file_negotiated_body(ShellFileNegotiated {
                        welcome: ShellV1ServerWelcome {
                            selected_revision: 6,
                            connection_epoch: EPOCH,
                            capabilities,
                            max_descriptors: 16,
                            max_label_bytes: 32,
                            max_pending_activations: 16,
                        },
                        limits_published: true,
                    })
                    .unwrap();
                    wire.event(ShellFileKind::Negotiated, &body);
                    negotiated = true;
                    continue;
                }
                ShellFileKind::AllocationRequest => {
                    decode_shell_file_allocation_request(bytes).unwrap()
                }
                ShellFileKind::ResourceBegin => {
                    let v = decode_shell_file_resource_begin(bytes).unwrap();
                    let ShellContentRecord::ResourceBegin(begin) = &v.record else {
                        unreachable!()
                    };
                    let name = format!("upload/{}", v.slot);
                    wire.uploads.insert(name.clone(), Vec::new());
                    slots.insert(begin.resource.id, name);
                    ShellFileTransactionRecord {
                        transaction: v.transaction,
                        record: v.record,
                    }
                }
                ShellFileKind::ResourceEnd => {
                    let v = decode_shell_file_resource_end(bytes).unwrap();
                    let ShellContentRecord::ResourceEnd(end) = &v.record else {
                        unreachable!()
                    };
                    let data = wire
                        .uploads
                        .remove(&slots.remove(&end.resource.id).unwrap())
                        .unwrap();
                    assert_eq!(data.len() as u64, end.total_bytes);
                    assert!(
                        end.total_bytes <= 137 || wire.short_writes > 0,
                        "control must exercise partial slot writes"
                    );
                    messages
                        .send(Message::Content(
                            v.transaction,
                            Box::new(ShellContentRecord::ResourceChunk(ContentResourceChunk {
                                grant: end.grant,
                                resource: end.resource,
                                ordinal: 0,
                                offset: 0,
                                bytes: data,
                            })),
                        ))
                        .unwrap();
                    v
                }
                ShellFileKind::ResourceRetire => decode_shell_file_resource_retire(bytes).unwrap(),
                ShellFileKind::FrameDemand | ShellFileKind::ActionAck => {
                    decode_shell_file_transaction(bytes, kind).unwrap()
                }
                ShellFileKind::Candidate => {
                    let v = decode_shell_file_candidate(bytes).unwrap();
                    for record in v.candidate.parts() {
                        messages
                            .send(Message::Content(v.transaction, Box::new(record)))
                            .unwrap();
                    }
                    continue;
                }
                ShellFileKind::IndicatorActivate => {
                    let v = decode_shell_file_indicator_activate(bytes).unwrap();
                    messages
                        .send(Message::Activation(v.transaction, v.activation))
                        .unwrap();
                    continue;
                }
                _ => panic!("unexpected transaction {kind:?}"),
            };
            messages
                .send(Message::Content(value.transaction, Box::new(value.record)))
                .unwrap();
        }
        std::thread::sleep(Duration::from_micros(100));
    }
}

fn emit(wire: &mut Wire, transaction: TransactionId, record: ShellContentRecord) {
    if let ShellContentRecord::OutputFacts(facts) = &record {
        let generation = facts.facts_generation;
        let bytes = encode_shell_file_outputs(
            object_header(ShellFileKind::Outputs),
            &ShellFileTransactionRecord {
                transaction,
                record,
            },
        )
        .unwrap();
        wire.object("outputs", 200 + generation, bytes);
        wire.announce(ShellFileKind::Outputs, generation, 200 + generation);
        return;
    }
    let record = ShellFileTransactionRecord {
        transaction,
        record,
    };
    let (kind, body) = match &record.record {
        ShellContentRecord::AllocationResult(_) => (
            ShellFileKind::AllocationResult,
            encode_shell_file_allocation_result_body(&record).unwrap(),
        ),
        ShellContentRecord::ResourceStatus(_) => (
            ShellFileKind::ResourceStatus,
            encode_shell_file_resource_status_body(&record).unwrap(),
        ),
        ShellContentRecord::ResourceReleased(_) => (
            ShellFileKind::ResourceReleased,
            encode_shell_file_resource_released_body(&record).unwrap(),
        ),
        _ => encode_shell_file_transaction_body(&record).unwrap(),
    };
    wire.event(kind, &body);
}
