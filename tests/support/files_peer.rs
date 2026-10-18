//! File-contract peer for the service assertions. Only 9P bytes cross the
//! socket. Whole candidates are split into the SDK's neutral records for the
//! existing lifecycle assertions; upload bytes come from actual slot writes.

use super::files_wire::{EPOCH, Wire};
use sophia_shell_protocol::{shell_files::*, *};
use std::{
    os::unix::net::UnixStream,
    sync::mpsc,
    time::{Duration, Instant},
};

pub enum Message {
    Content(TransactionId, Box<ShellContentRecord>),
    Activation(TransactionId, ShellIndicatorActivation),
    /// One sealed Source verified by the peer (bytes checked there).
    Image(ShellImageMetadata),
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
    static ENDPOINT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let endpoint_name = format!(
        "lom-image-{}-{}.sock",
        std::process::id(),
        ENDPOINT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let endpoint_path = std::env::temp_dir().join(&endpoint_name);
    let image_listener = seqpacket_listener(&endpoint_path);
    wire.object("image-channel", 41, discovery_text(&endpoint_name));
    wire.object("image-limits", 42, image_limits_record());
    wire.object("image-formats", 43, image_formats_record());
    let mut image_socket: Option<std::os::fd::OwnedFd> = None;
    let mut hello_seen = false;
    let mut next_offer = 0u64;
    let mut admit_queue: Vec<(TransactionId, ShellImageMetadata, bool)> = Vec::new();
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
            let kind = decode_shell_file_record_at_revision(bytes, ShellFileClass::Candidate, 9)
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
                        (9, 9, capabilities)
                    );
                    let body = encode_shell_file_negotiated_body(ShellFileNegotiated {
                        welcome: ShellV1ServerWelcome {
                            selected_revision: 9,
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
                ShellFileKind::ImageBegin => {
                    let record =
                        decode_shell_file_record_at_revision(bytes, ShellFileClass::Candidate, 9)
                            .unwrap();
                    let mut metadata = ShellImageMetadata::decode(record.body).unwrap();
                    assert_eq!(
                        metadata.admission_epoch,
                        super::GRANT.content_grant_epoch,
                        "admission epoch must name the content grant, not the connection"
                    );
                    next_offer += 1;
                    metadata.offer_id = next_offer;
                    admit_queue.push((
                        TransactionId::from_raw(record.header.submission_id),
                        metadata,
                        false,
                    ));
                    continue;
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
        // Drive the sealed endpoint: Hello, ChannelReady,
        // ImageAdmitted, then verify each Source before Accepted.
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            if image_socket.is_none() && fd_ready(image_listener.as_raw_fd()) {
                let fd = unsafe {
                    libc::accept4(
                        image_listener.as_raw_fd(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        libc::SOCK_CLOEXEC,
                    )
                };
                assert!(fd >= 0, "image endpoint accept");
                image_socket = Some(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) });
            }
            if let Some(socket) = &image_socket {
                let raw = socket.as_raw_fd();
                if !hello_seen {
                    if let Some((packet, fds)) = recv_packet(raw) {
                        assert_eq!(packet.len(), ShellImagePacket::HELLO_BYTES);
                        assert!(fds.is_empty(), "Hello carries no descriptors");
                        hello_seen = true;
                        let body = ShellChannelReady {
                            channel_generation: 3,
                            granted_source_kinds: 1,
                        }
                        .encode()
                        .unwrap();
                        wire.event_image(ShellFileKind::ChannelReady, &body);
                    }
                } else {
                    if let Some(entry) = admit_queue.iter_mut().find(|(_, _, sent)| !sent) {
                        entry.2 = true;
                        let body = entry.1.encode().unwrap();
                        wire.event_image(ShellFileKind::ImageAdmitted, &body);
                    }
                    if let Some((packet, fds)) = recv_packet(raw) {
                        assert_eq!(packet.len(), 200, "Source packet size");
                        assert_eq!(fds.len(), 1, "Source carries one descriptor");
                        let ShellImagePacket::Source { metadata, .. } =
                            ShellImagePacket::decode(&packet).unwrap()
                        else {
                            panic!("expected Source packet");
                        };
                        assert!(
                            admit_queue
                                .iter()
                                .any(|(_, m, sent)| *sent && m.offer_id == metadata.offer_id),
                            "Source names an admitted offer"
                        );
                        admit_queue.retain(|(_, m, _)| m.offer_id != metadata.offer_id);
                        let mut bytes = vec![0u8; metadata.declared_bytes as usize];
                        let read = unsafe {
                            libc::pread(
                                fds[0].as_raw_fd(),
                                bytes.as_mut_ptr() as *mut libc::c_void,
                                bytes.len(),
                                0,
                            )
                        };
                        assert_eq!(read as usize, bytes.len(), "sealed bytes readable");
                        messages.send(Message::Image(metadata)).unwrap();
                        emit(
                            &mut wire,
                            TransactionId::from_raw(metadata.offer_id),
                            ShellContentRecord::ResourceStatus(ContentResourceStatus {
                                grant: super::GRANT,
                                resource: ContentResourceId {
                                    id: metadata.resource,
                                    generation: metadata.resource_generation,
                                },
                                status: 2,
                                reason: 0,
                                next_ordinal: 0,
                                admitted_bytes: metadata.declared_bytes,
                            }),
                        );
                        let body = ShellSourceReleased {
                            offer_id: metadata.offer_id,
                            resource: metadata.resource,
                            resource_generation: metadata.resource_generation,
                            disposition: ShellSourceDisposition::Retained,
                        }
                        .encode()
                        .unwrap();
                        wire.event_image(ShellFileKind::SourceReleased, &body);
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_micros(100));
    }
}

/// Bind a SEQPACKET listener for the sealed image endpoint.
fn seqpacket_listener(path: &std::path::Path) -> std::os::fd::OwnedFd {
    use std::os::fd::FromRawFd;
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0);
        assert!(fd >= 0, "seqpacket socket");
        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let bytes = c_path.as_bytes();
        assert!(bytes.len() < addr.sun_path.len());
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            addr.sun_path.as_mut_ptr() as *mut u8,
            bytes.len(),
        );
        let len = (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len()) as u32;
        assert_eq!(
            libc::bind(fd, &addr as *const _ as *const libc::sockaddr, len),
            0,
            "seqpacket bind"
        );
        assert_eq!(libc::listen(fd, 4), 0, "seqpacket listen");
        std::os::fd::OwnedFd::from_raw_fd(fd)
    }
}

fn fd_ready(fd: std::os::fd::RawFd) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pfd, 1, 0) > 0 && pfd.revents & libc::POLLIN != 0 }
}

/// Receive one packet plus any descriptors; `None` when none is ready.
fn recv_packet(fd: std::os::fd::RawFd) -> Option<(Vec<u8>, Vec<std::os::fd::OwnedFd>)> {
    use std::os::fd::FromRawFd;
    if !fd_ready(fd) {
        return None;
    }
    let mut buf = [0u8; 512];
    let mut control = [0u8; 128];
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr() as *mut libc::c_void,
        iov_len: buf.len(),
    };
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = control.len();
    let n = unsafe { libc::recvmsg(fd, &mut msg, libc::MSG_CMSG_CLOEXEC) };
    if n <= 0 {
        return None;
    }
    let mut fds = Vec::new();
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let count = ((*cmsg).cmsg_len - libc::CMSG_LEN(0) as usize) / 4;
                let data = libc::CMSG_DATA(cmsg) as *const i32;
                for i in 0..count {
                    fds.push(std::os::fd::OwnedFd::from_raw_fd(*data.add(i)));
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }
    Some((buf[..n as usize].to_vec(), fds))
}

fn discovery_text(endpoint: &str) -> Vec<u8> {
    ShellImageDiscovery {
        connection_epoch: EPOCH,
        channel_generation: 3,
        endpoint: endpoint.to_owned(),
        source_kinds: 1,
        max_offers: 4,
        max_packets_per_turn: 1,
        hello_timeout_ms: 250,
        validation_timeout_ms: 2_000,
        validation_chunk_bytes: 65_536,
        capture_interest_timeout_ms: 500,
        formats_generation: 11,
        limits_generation: 9,
    }
    .encode()
    .unwrap()
    .into_bytes()
}

fn image_limits_record() -> Vec<u8> {
    let body = ShellImageLimits {
        channel_generation: 3,
        limits_generation: 9,
        max_resource_bytes: 4_194_304,
        max_sealed_backing_bytes: 16_777_216,
        max_dma_source_bytes: 0,
        max_renderer_allocation_bytes: 0,
        max_renderer_entries: 0,
        max_offers: 4,
        max_validation_jobs: 4,
        max_capture_jobs: 0,
        max_admitted_outputs: 16,
        max_format_rows_per_output: 1,
        max_width: 8192,
        max_height: 4096,
        validation_timeout_ms: 2_000,
        capture_interest_timeout_ms: 500,
        validation_chunk_bytes: 65_536,
        max_packets_per_turn: 1,
    }
    .encode()
    .unwrap();
    encode_shell_file_record_at_revision(object_header(ShellFileKind::ImageLimits), &body, 9)
        .unwrap()
}

fn image_formats_record() -> Vec<u8> {
    let body = ShellImageFormats {
        channel_generation: 3,
        formats_generation: 11,
        limits_generation: 9,
        rows: Vec::new(),
    }
    .encode()
    .unwrap();
    encode_shell_file_record_at_revision(object_header(ShellFileKind::ImageFormats), &body, 9)
        .unwrap()
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
