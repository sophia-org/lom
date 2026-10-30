//! CPU direct-fill custody over the real SDK connection and ring peer. The
//! renderer is scripted; the loan, mapping, Offer and release are real.
use super::*;
use lom::service::{RenderIdentity, RenderImage};
use sophia_shell_client::OwnedRingSlotFill;
use sophia_shell_protocol::shell_files::ShellImageMetadata;
use std::sync::{Arc, Mutex, atomic::AtomicUsize};

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    Grant,
    Connection,
    Output,
    Allocation,
    Scale,
    Geometry,
    /// The render fails and drops its loan.
    Fail,
    /// The job outlives its deadline; the renderer keeps the loan.
    Timeout,
    /// The job never completes while the connection is lost.
    Park,
}

type Held = Arc<Mutex<Option<OwnedRingSlotFill>>>;

/// Applies `first` to the first job only; later jobs complete normally.
struct FillRenderer {
    first: Fault,
    jobs: Arc<AtomicUsize>,
    job: Option<(RenderIdentity, OwnedRingSlotFill, u32, u32)>,
    held: Held,
}

/// Straight RGBA with every alpha class, and the wire bytes it must become.
fn pattern(width: u32, height: u32) -> (Vec<u8>, Vec<u8>) {
    let rgba: Vec<u8> = (0..width * height * 4)
        .map(|i| (i.wrapping_mul(37) ^ (i >> 3)) as u8)
        .collect();
    let wire = ContentPixels::from_rgba8(width, height, rgba.clone())
        .unwrap()
        .bytes()
        .to_vec();
    (rgba, wire)
}

impl ContentRenderer for FillRenderer {
    fn submit(
        &mut self,
        _identity: RenderIdentity,
        _model: Model,
        _width: u32,
        _height: u32,
        _scale: f64,
    ) -> Result<(), String> {
        panic!("a ring-fill renderer must never receive a slice job")
    }

    fn uses_ring_fill(&self) -> bool {
        true
    }

    fn submit_fill(
        &mut self,
        identity: RenderIdentity,
        _model: Model,
        width: u32,
        height: u32,
        _scale: f64,
        fill: OwnedRingSlotFill,
    ) -> Result<(), (String, OwnedRingSlotFill)> {
        assert!(self.job.is_none(), "one render job at a time");
        self.jobs.fetch_add(1, Ordering::Relaxed);
        self.job = Some((identity, fill, width, height));
        Ok(())
    }

    fn poll(&mut self) -> Result<Option<RenderedContent>, String> {
        let first = self.jobs.load(Ordering::Relaxed) == 1;
        let fault = if first { self.first } else { Fault::None };
        if matches!(fault, Fault::Park) {
            return Ok(None);
        }
        let Some((mut identity, mut fill, mut width, height)) = self.job.take() else {
            return Ok(None);
        };
        let (_, wire) = pattern(width, height);
        // Write the converted wire bytes, as the worker's in-place conversion would.
        fill.pixels_mut().unwrap().copy_from_slice(&wire);
        match fault {
            Fault::None | Fault::Park => {}
            Fault::Grant => identity.grant.content_grant_epoch += 1,
            Fault::Connection => identity.grant.connection_epoch += 1,
            Fault::Output => identity.output.generation += 1,
            Fault::Allocation => identity.allocation.generation += 1,
            Fault::Scale => identity.scale_generation += 1,
            Fault::Geometry => width += 1,
            Fault::Fail => {
                drop(fill);
                return Err("injected render failure".into());
            }
            Fault::Timeout => {
                *self.held.lock().unwrap() = Some(fill);
                return Err(
                    "CPU worker deadline expired; in-flight resources remain quarantined".into(),
                );
            }
        }
        Ok(Some(RenderedContent {
            pixels: RenderImage::RingFill {
                fill,
                identity,
                width,
                height,
                checksum: 0,
            },
            targets: Vec::new(),
        }))
    }
}

/// Scripted Session: present every frame, release each retired resource, and
/// report the peer-read Offers until `done` or the client disconnects.
fn present_fills(
    stream: UnixStream,
    done: std::sync::mpsc::Receiver<()>,
    images: std::sync::mpsc::Sender<(ShellImageMetadata, Vec<u8>)>,
    release: Option<std::sync::mpsc::Receiver<()>>,
) {
    let expected = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
        | sophia_shell_client::IMAGE_CAPABILITY_OWNED_RING;
    let mut stream = Peer::new(stream, expected);
    if release.is_some() {
        stream.hold_slot_releases(true);
    }
    initialize_two_outputs(&mut stream);
    let mut candidate = None;
    while done.try_recv().is_err() {
        if release
            .as_ref()
            .is_some_and(|release| release.try_recv().is_ok())
        {
            stream.hold_slot_releases(false);
        }
        let Some(message) = stream.try_receive(Duration::from_millis(20)) else {
            continue;
        };
        let (transaction, record) = match message {
            Message::Image(metadata, pixels) => {
                images.send((metadata, pixels)).unwrap();
                continue;
            }
            Message::Content(transaction, record) => (transaction, *record),
            Message::Activation(..) => panic!("unexpected activation"),
        };
        match record {
            ShellContentRecord::FrameDemand(demand) => send_tx(
                &mut stream,
                transaction,
                ShellContentRecord::FramePermit(ContentFramePermit {
                    grant: GRANT,
                    output: demand.output,
                    demand_id: demand.demand_id,
                    permit_id: demand.demand_id + 20,
                    state: 1,
                    reason: 0,
                    ttl_ms: 250,
                    max_candidate_bytes: 8192,
                }),
            ),
            ShellContentRecord::CandidateBegin(begin) => candidate = Some((transaction, begin)),
            ShellContentRecord::CandidateChunk(_) => {}
            ShellContentRecord::CandidateEnd(end) => {
                let (origin, begin) = candidate.take().unwrap();
                assert_eq!(end.candidate_generation, begin.candidate_generation);
                for kind in [1, 2] {
                    send_tx(
                        &mut stream,
                        origin,
                        ShellContentRecord::CandidateOutcome(
                            sophia_shell_protocol::ContentCandidateOutcome {
                                grant: GRANT,
                                candidate_generation: begin.candidate_generation,
                                output: begin.output,
                                kind,
                                reason: 0,
                                presentation_epoch: if kind == 2 {
                                    begin.candidate_generation + 30
                                } else {
                                    0
                                },
                                work_area_generation: 8 + begin.candidate_generation,
                                wm_commit_generation: 9 + begin.candidate_generation,
                            },
                        ),
                    );
                }
            }
            ShellContentRecord::ResourceRetire(retire) => send_tx(
                &mut stream,
                transaction,
                ShellContentRecord::ResourceReleased(
                    sophia_shell_protocol::ContentResourceReleased {
                        grant: GRANT,
                        resource: retire.resource,
                        reason: 0,
                    },
                ),
            ),
            other => panic!("unexpected record: {other:?}"),
        }
    }
}

struct Session<R> {
    service: ShellService<R>,
    images: std::sync::mpsc::Receiver<(ShellImageMetadata, Vec<u8>)>,
    done: std::sync::mpsc::Sender<()>,
    release: Option<std::sync::mpsc::Sender<()>>,
    server: std::thread::JoinHandle<()>,
    path: std::path::PathBuf,
}

fn session<R: ContentRenderer>(renderer: R) -> Session<R> {
    session_with(renderer, false)
}

/// `hold`: the peer sends Accepted but withholds SlotReleased until released.
fn session_with<R: ContentRenderer>(renderer: R, hold: bool) -> Session<R> {
    let path = std::env::temp_dir().join(format!(
        "lom-fill-{}-{}.sock",
        std::process::id(),
        SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    let (done, done_receiver) = std::sync::mpsc::channel();
    let (image_sender, images) = std::sync::mpsc::channel();
    let (release, release_receiver) = std::sync::mpsc::channel();
    let release_receiver = hold.then_some(release_receiver);
    let server = std::thread::spawn(move || {
        present_fills(
            listener.accept().unwrap().0,
            done_receiver,
            image_sender,
            release_receiver,
        )
    });
    let capabilities = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
        | sophia_shell_client::IMAGE_CAPABILITY_OWNED_RING;
    let connection = ShellConnection::connect_files(
        &path,
        ShellClientOptions {
            minimum_revision: 10,
            maximum_revision: 10,
            required_capabilities: capabilities,
            handshake_timeout: Duration::from_secs(2),
        },
    )
    .unwrap();
    let (config, theme) =
        parse_shell_config(include_str!("../../examples/minimal/shell.kdl")).unwrap();
    let service = ShellService::new(
        connection,
        config,
        theme,
        48,
        renderer,
        path.parent().unwrap().to_path_buf(),
    )
    .unwrap();
    Session {
        service,
        images,
        done,
        release: hold.then_some(release),
        server,
        path,
    }
}

impl<R: ContentRenderer> Session<R> {
    /// Step until `count` Offers were read by the peer.
    fn offers(&mut self, count: usize) -> Vec<(ShellImageMetadata, Vec<u8>)> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut offers = Vec::new();
        while offers.len() < count {
            assert!(std::time::Instant::now() < deadline, "fill offers stalled");
            self.service.step().unwrap();
            offers.extend(self.images.try_iter());
            std::thread::yield_now();
        }
        offers
    }

    /// Step until the service fails terminally.
    fn failure(&mut self) -> String {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            assert!(std::time::Instant::now() < deadline, "service never failed");
            if let Err(error) = self.service.step() {
                return error;
            }
            std::thread::yield_now();
        }
    }

    fn finish(self) -> Vec<(ShellImageMetadata, Vec<u8>)> {
        let _ = self.done.send(());
        drop(self.service);
        self.server.join().unwrap();
        let _ = std::fs::remove_file(&self.path);
        self.images.try_iter().collect()
    }
}

fn renderer(first: Fault) -> (FillRenderer, Arc<AtomicUsize>, Held) {
    let jobs = Arc::new(AtomicUsize::new(0));
    let held = Held::default();
    let renderer = FillRenderer {
        first,
        jobs: jobs.clone(),
        job: None,
        held: held.clone(),
    };
    (renderer, jobs, held)
}

#[test]
fn committed_fills_reach_the_peer_byte_for_byte_and_share_one_loan() {
    let (renderer, jobs, _) = renderer(Fault::None);
    let mut session = session(renderer);
    let offers = session.offers(2);
    let (_, wire) = pattern(8, 48);
    for (index, (metadata, pixels)) in offers.iter().enumerate() {
        let id = index as u64 + 1;
        assert_eq!((metadata.offer_id, metadata.resource), (id, id));
        assert_eq!((metadata.width, metadata.height), (8, 48));
        assert_eq!(pixels, &wire, "peer reads exactly the filled payload");
    }
    // Two outputs, one loan; a clock minute boundary may add a later frame.
    // The Accepted-only hold below proves the wait for the exact release.
    assert!(jobs.load(Ordering::Relaxed) >= 2);
    session.finish();
}

#[test]
fn accepted_alone_lends_no_second_job_until_the_exact_slot_release() {
    let (renderer, jobs, _) = renderer(Fault::None);
    let mut session = session_with(renderer, true);
    let first = session.offers(1);
    assert_eq!(first[0].0.offer_id, 1);
    // Accepted has arrived; SlotReleased is withheld. The second output is
    // dirty, but the single loan stays Busy: no job, Offer or ID is spent.
    let held = std::time::Instant::now() + Duration::from_millis(500);
    while std::time::Instant::now() < held {
        session.service.step().unwrap();
        assert!(session.images.try_recv().is_err(), "Offer before release");
        std::thread::yield_now();
    }
    assert_eq!(jobs.load(Ordering::Relaxed), 1, "job before release");
    session.release.take().unwrap().send(()).unwrap();
    let second = session.offers(1);
    assert_eq!((second[0].0.offer_id, second[0].0.resource), (2, 2));
    assert!(jobs.load(Ordering::Relaxed) >= 2);
    session.finish();
}

#[test]
fn stale_fills_are_aborted_without_spending_an_offer_or_resource() {
    for fault in [
        Fault::Grant,
        Fault::Connection,
        Fault::Output,
        Fault::Allocation,
        Fault::Scale,
        Fault::Geometry,
    ] {
        let (renderer, jobs, _) = renderer(fault);
        let mut session = session(renderer);
        let offers = session.offers(2);
        let ids: Vec<_> = offers
            .iter()
            .map(|(metadata, _)| (metadata.offer_id, metadata.resource))
            .collect();
        assert_eq!(ids, [(1, 1), (2, 2)], "{fault:?}");
        assert!(
            jobs.load(Ordering::Relaxed) >= 3,
            "{fault:?}: stale job re-rendered"
        );
        session.finish();
    }
}

#[test]
fn a_failed_render_is_terminal_and_publishes_nothing() {
    let (renderer, _, _) = renderer(Fault::Fail);
    let mut session = session(renderer);
    assert!(session.failure().contains("injected render failure"));
    assert!(session.finish().is_empty());
}

#[test]
fn a_timed_out_job_keeps_its_loan_writable_after_the_service_ends() {
    let (renderer, _, held) = renderer(Fault::Timeout);
    let mut session = session(renderer);
    assert!(session.failure().contains("remain quarantined"));
    assert!(session.finish().is_empty());
    let mut fill = held.lock().unwrap().take().expect("renderer kept the loan");
    // Neither the timeout nor dropping the connection reclaimed the storage.
    fill.pixels_mut().unwrap().fill(0x5a);
}

#[test]
fn disconnect_while_a_job_holds_its_loan_publishes_nothing() {
    let (renderer, jobs, _) = renderer(Fault::Park);
    let mut session = session(renderer);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while jobs.load(Ordering::Relaxed) == 0 {
        assert!(std::time::Instant::now() < deadline, "no job started");
        session.service.step().unwrap();
        std::thread::yield_now();
    }
    session.done.send(()).unwrap();
    let error = session.failure();
    assert!(!error.is_empty());
    assert!(session.finish().is_empty());
}

#[test]
fn the_real_cpu_worker_presents_every_output_through_owned_fills() {
    // No GPU grant is read or needed for the explicit CPU renderer.
    let mut session = session(lom::render::RendererWorker::start_cpu().unwrap());
    let offers = session.offers(2);
    for (index, (metadata, pixels)) in offers.iter().enumerate() {
        let id = index as u64 + 1;
        assert_eq!((metadata.offer_id, metadata.resource), (id, id));
        assert_eq!(pixels.len(), 8 * 48 * 4);
        assert!(pixels.iter().any(|byte| *byte != 0), "rendered, not blank");
    }
    session.finish();
}
