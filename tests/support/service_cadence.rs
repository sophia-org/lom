//! `lom-cpu-cadence` over a real socket, peer and Vello CPU worker for both
//! arms. A small private quota drives the same redirty logic as the fixed
//! production quota, which only the binary entry selects.

use super::*;
use crate::render::RendererWorker;
use crate::service::tests::files_peer::{Message, Peer};
use sophia_shell_client::ShellClientOptions;
use sophia_shell_protocol::{
    ContentAllocationId, ContentCandidateOutcome, ContentGrant, ContentLogicalRect, ContentMargins,
    ContentOutputId, ContentPixelRect, ContentResourceReleased,
    SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE, SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER,
    SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS,
};
use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;

const GRANT: ContentGrant = ContentGrant {
    connection_epoch: crate::service::tests::files_wire::EPOCH,
    content_grant_epoch: 9,
};
const OUTPUTS: [ContentOutputId; 2] = [
    ContentOutputId {
        id: 2,
        generation: 3,
    },
    ContentOutputId {
        id: 3,
        generation: 4,
    },
];
const CAPABILITIES: u64 = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
    | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
    | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
    | sophia_shell_client::IMAGE_CAPABILITY_OWNED_RING;
/// The workspaces-only static panel: nothing but the cadence redirties it.
const STATIC: &str = r##"version 1
panel "main" {
    position "top"
    height 24
    renderer "cpu"
    margin 0 0 0 0
    popup-gap 5
    start {
        workspaces "workspaces"
    }
}
theme "minimal" {
    defaults {
        background "#1c1c1c"
        selected "#2d2d2d"
        foreground "#ffffff"
        active "#6699cc"
        urgent "#8f0a0a"
        font-size 13
        gap 13
        padding 7
    }
    module "clock" {
        bold #true
    }
}
"##;

static SOCKET_ID: AtomicU64 = AtomicU64::new(1);

/// What the scripted Session withholds until the test clears it.
#[derive(Default)]
struct Hold {
    presented: AtomicBool,
    released: AtomicBool,
}

/// Scripted Session: Permit every demand, report Accepted at once and
/// Presented and ResourceReleased unless held, and forward the slot bytes.
fn present(
    stream: UnixStream,
    outputs: usize,
    hold: Arc<Hold>,
    done: mpsc::Receiver<()>,
    images: mpsc::Sender<Vec<u8>>,
) {
    let peer = Peer::new(stream, CAPABILITIES);
    let send = |transaction, record| peer.content(transaction, record);
    send(
        TransactionId::from_raw(0),
        ShellContentRecord::Limits(sophia_shell_protocol::ContentLimits::prototype(GRANT)),
    );
    send(
        TransactionId::from_raw(2),
        ShellContentRecord::OutputFacts(ContentOutputFacts {
            grant: GRANT,
            facts_generation: 4,
            outputs: OUTPUTS[..outputs]
                .iter()
                .map(|&output| ContentOutputFactsEntry {
                    output,
                    local_width: 4,
                    local_height: 32,
                    scale_numerator: 2,
                    scale_denominator: 1,
                    scale_generation: 5,
                })
                .collect(),
        }),
    );
    let (mut candidate, mut presented, mut released) = (None, Vec::new(), Vec::new());
    while done.try_recv().is_err() {
        if !hold.presented.load(Ordering::SeqCst) {
            for (transaction, outcome) in presented.drain(..) {
                send(transaction, ShellContentRecord::CandidateOutcome(outcome));
            }
        }
        if !hold.released.load(Ordering::SeqCst) {
            for (transaction, record) in released.drain(..) {
                send(transaction, ShellContentRecord::ResourceReleased(record));
            }
        }
        let Some(message) = peer.try_receive(Duration::from_millis(5)) else {
            continue;
        };
        let (transaction, record) = match message {
            Message::Image(_, pixels) => {
                images.send(pixels).unwrap();
                continue;
            }
            Message::Content(transaction, record) => (transaction, *record),
            Message::Activation(..) => panic!("unexpected activation"),
        };
        match record {
            ShellContentRecord::AllocationRequest(request) => {
                let index = OUTPUTS.iter().position(|o| *o == request.output).unwrap();
                send(
                    transaction,
                    ShellContentRecord::AllocationResult(ContentAllocationResult {
                        grant: GRANT,
                        allocation_request_id: request.allocation_request_id,
                        status: 1,
                        reason: ContentReason::None as u16,
                        output: request.output,
                        allocation: ContentAllocationId {
                            id: 11 + index as u64,
                            generation: 1,
                        },
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
            }
            ShellContentRecord::FrameDemand(demand) => send(
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
                let outcome = |kind| ContentCandidateOutcome {
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
                };
                send(origin, ShellContentRecord::CandidateOutcome(outcome(1)));
                presented.push((origin, outcome(2)));
            }
            ShellContentRecord::ResourceRetire(retire) => released.push((
                transaction,
                ContentResourceReleased {
                    grant: GRANT,
                    resource: retire.resource,
                    reason: 0,
                },
            )),
            other => panic!("unexpected record: {other:?}"),
        }
    }
}

struct Session<R> {
    service: ShellService<R>,
    hold: Arc<Hold>,
    images: mpsc::Receiver<Vec<u8>>,
    done: mpsc::Sender<()>,
    server: std::thread::JoinHandle<()>,
    directory: PathBuf,
}

fn session<R: ContentRenderer>(renderer: R, outputs: usize) -> Session<R> {
    session_until_stopped(renderer, outputs, (Arc::new(AtomicBool::new(false)), None))
}

fn session_until_stopped<R: ContentRenderer>(
    renderer: R,
    outputs: usize,
    stop: (Arc<AtomicBool>, Option<UnixStream>),
) -> Session<R> {
    // A fresh exclusive directory per session: a reused PID never meets a
    // stale socket, and only this fixture's own directory is removed.
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lom-cadence-{}-{nanos}-{}",
        std::process::id(),
        SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("shell.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let hold = Arc::new(Hold::default());
    let (done, done_receiver) = mpsc::channel();
    let (image_sender, images) = mpsc::channel();
    let peer_hold = hold.clone();
    let server = std::thread::spawn(move || {
        let stream = listener.accept().unwrap().0;
        present(stream, outputs, peer_hold, done_receiver, image_sender)
    });
    let connection = ShellConnection::connect_files(
        &path,
        ShellClientOptions {
            minimum_revision: 10,
            maximum_revision: 10,
            required_capabilities: CAPABILITIES,
            handshake_timeout: Duration::from_secs(2),
        },
    )
    .unwrap();
    let (config, theme) = crate::config::parse_shell_config(STATIC).unwrap();
    let service = ShellService::new_until_stopped(
        connection,
        config,
        theme,
        48,
        renderer,
        stop,
        // The peer binds its image endpoint in the temporary directory.
        std::env::temp_dir(),
    )
    .unwrap();
    Session {
        service,
        hold,
        images,
        done,
        server,
        directory,
    }
}

impl<R: ContentRenderer> Session<R> {
    fn cadence(&self) -> &super::Cadence {
        self.service.cadence.as_ref().unwrap()
    }

    fn stats(&self) -> WorkStatistics {
        self.service.statistics.unwrap()
    }

    /// Step until `done` holds, collecting each newly presented timing.
    fn until(&mut self, done: impl Fn(&ShellService<R>) -> bool) -> Vec<(u64, Timing)> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut timings = Vec::new();
        while !done(&self.service) {
            assert!(Instant::now() < deadline, "cadence stalled");
            self.service.step().unwrap();
            if let Some(cadence) = &self.service.cadence
                && cadence.presented > timings.len() as u64
            {
                let candidate = self.service.panels[0].presented.as_ref().unwrap();
                timings.push((candidate.candidate_generation, cadence.last));
            }
            std::thread::yield_now();
        }
        timings
    }

    /// Keep stepping without a goal: nothing may start that is not allowed.
    fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            self.service.step().unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn failure(&mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "service never failed");
            if let Err(error) = self.service.step() {
                return error;
            }
            std::thread::yield_now();
        }
    }

    fn finish(self) -> Vec<Vec<u8>> {
        let _ = self.done.send(());
        drop(self.service);
        self.server.join().unwrap();
        std::fs::remove_dir_all(&self.directory).unwrap();
        self.images.try_iter().collect()
    }
}

fn presented(count: u64) -> impl Fn(&ShellService<RendererWorker>) -> bool {
    move |service| service.cadence.as_ref().unwrap().presented == count
}

/// Reuse the real peer for a subprocess stdout test of CLI dispatch and drive.
pub(crate) fn cpu_record_session_for_tests(
    worker: RendererWorker,
    run: impl FnOnce(
        &mut ShellService<RendererWorker>,
        fn(&ShellService<RendererWorker>) -> bool,
    ) -> Result<(), String>,
) -> Vec<u8> {
    let mut session = session(worker, 1);
    let result = run(&mut session.service, |service| {
        service.panels[0].presented.is_some() && service.presentations.is_empty()
    });
    let images = session.finish();
    result.unwrap();
    assert_eq!(images.len(), 1);
    images.into_iter().next().unwrap()
}

fn complete(final_record: &str, quota: u64) {
    let prefix = format!(
        "lom_cadence_final schema=1 status=complete quota={quota} presented={quota} renders_started={quota} renders_completed={quota} uploads={quota} raster_reuses=0 "
    );
    assert!(final_record.starts_with(&prefix), "{final_record}");
    assert!(
        final_record.ends_with(" rendering_in_flight=0 pending_presentations=0"),
        "{final_record}"
    );
}

#[test]
fn both_arms_repeat_identical_bytes_to_the_quota_and_then_stay_connected() {
    let stop = AtomicBool::new(false);
    let (wake, _wake_peer) = UnixStream::pair().unwrap();
    let mut arms = Vec::new();
    for copy in [false, true] {
        let worker = if copy {
            RendererWorker::start_cpu_copy_reference_until_stopped(&stop, wake.as_fd())
        } else {
            RendererWorker::start_cpu_until_stopped(&stop, wake.as_fd(), true)
        };
        let mut session = session(worker.unwrap(), 1);
        assert_eq!(session.service.renderer.uses_ring_fill(), !copy);
        session.service.enable_cadence(3).unwrap();
        let timings = session.until(presented(3));
        // Candidate identity and owner-thread order, each attempt after the
        // previous exact Presented.
        let mut previous = 0;
        for (index, (candidate, timing)) in timings.iter().enumerate() {
            assert_eq!(*candidate, index as u64 + 1);
            assert!(timing.eligible_render_attempt > previous, "{timing:?}");
            assert!(timing.eligible_render_attempt <= timing.ring_commit_and_offer_queued);
            assert!(timing.ring_commit_and_offer_queued <= timing.presented_observed);
            previous = timing.presented_observed;
        }
        assert_eq!(timings.len(), 3);
        session.settle();
        // The quota stops redirtying; the connection stays up for settlement.
        assert!(!session.service.panels[0].dirty);
        assert_eq!(session.stats().renders_started, 3);
        complete(&session.service.cadence_final().unwrap(), 3);
        session.service.step().unwrap();
        let images = session.finish();
        assert_eq!(images.len(), 3, "copy={copy}");
        assert!(images.iter().all(|pixels| *pixels == images[0]));
        assert!(
            images[0].iter().any(|byte| *byte != 0),
            "rendered, not blank"
        );
        arms.push(images[0].clone());
    }
    assert_eq!(arms[0], arms[1], "both arms present the same bytes");
}

#[test]
fn nothing_redirties_before_the_exact_presented_or_bypasses_the_release() {
    let mut session = session(RendererWorker::start_cpu().unwrap(), 1);
    session.service.enable_cadence(3).unwrap();
    session.hold.presented.store(true, Ordering::SeqCst);
    session.hold.released.store(true, Ordering::SeqCst);
    session.settle();
    // Candidate 1 waits for Presented: no sample, no redirty, no second job.
    assert_eq!(session.cadence().presented, 0);
    assert_eq!(session.stats().renders_started, 1);
    assert!(!session.service.panels[0].dirty);
    session.hold.presented.store(false, Ordering::SeqCst);
    session.until(presented(2));
    session.settle();
    // Candidate 2 retired slot 0; the redirtied panel waits for its release.
    assert_eq!(session.cadence().presented, 2);
    assert_eq!(session.stats().renders_started, 2);
    assert!(session.service.panels[0].dirty);
    session.hold.released.store(false, Ordering::SeqCst);
    session.until(presented(3));
    session.settle();
    complete(&session.service.cadence_final().unwrap(), 3);
    assert_eq!(session.finish().len(), 3);
}

#[test]
fn the_static_service_still_presents_one_candidate_without_the_cadence() {
    let mut session = session(RendererWorker::start_cpu().unwrap(), 1);
    session.service.set_work_statistics_enabled(true);
    session.until(|service| service.panels[0].presented.is_some());
    session.settle();
    assert_eq!(session.stats().renders_started, 1);
    assert!(!session.service.panels[0].dirty);
    assert_eq!(session.service.cadence_final(), None);
    assert_eq!(session.finish().len(), 1);
}

#[test]
fn the_production_quota_is_fixed_and_counters_are_never_reset_or_shared() {
    assert_eq!(QUOTA, 2120);
    let mut session = session(RendererWorker::start_cpu().unwrap(), 1);
    session.service.set_work_statistics_enabled(true);
    let error = session.service.enable_diagnostic_cadence().unwrap_err();
    assert!(error.contains("statistics already enabled"), "{error}");
    session.service.set_work_statistics_enabled(false);
    session.service.enable_diagnostic_cadence().unwrap();
    assert_eq!(session.cadence().quota, 2120);
    assert_eq!(session.stats(), WorkStatistics::default());
    assert!(session.service.enable_diagnostic_cadence().is_err());
    session.finish();

    let mut late = self::session(RendererWorker::start_cpu().unwrap(), 1);
    late.until(|service| service.rendering_panel.is_some());
    let error = late.service.enable_cadence(3).unwrap_err();
    assert!(error.contains("before the first render"), "{error}");
    late.finish();

    let mut two = self::session(RendererWorker::start_cpu().unwrap(), 2);
    let error = two.service.enable_cadence(3).unwrap_err();
    assert!(error.contains("exactly one output panel"), "{error}");
    two.finish();
}

#[test]
fn lost_counters_and_extra_work_refuse_instead_of_sampling() {
    let mut lost = session(RendererWorker::start_cpu().unwrap(), 1);
    lost.service.enable_cadence(3).unwrap();
    lost.service.statistics = None;
    assert!(lost.failure().contains("lost its counters"));
    let record = lost.service.cadence_final().unwrap();
    assert!(
        record.starts_with("lom_cadence_final schema=1 status=refused quota=3 presented=0 "),
        "{record}"
    );
    lost.finish();

    // An unsolicited job before the sample breaks the one-render equality.
    let mut extra = session(RendererWorker::start_cpu().unwrap(), 1);
    extra.service.enable_cadence(3).unwrap();
    extra.service.statistics.as_mut().unwrap().renders_started = 1;
    assert!(
        extra
            .failure()
            .contains("candidate 1 shows extra or reused work")
    );
    extra.finish();

    // After the quota, unfinished work is in the final record, and a
    // presented one is refused rather than sampled.
    let mut after = session(RendererWorker::start_cpu().unwrap(), 1);
    after.service.enable_cadence(2).unwrap();
    after.until(presented(2));
    after.settle();
    complete(&after.service.cadence_final().unwrap(), 2);
    after.hold.presented.store(true, Ordering::SeqCst);
    after.service.panels[0].dirty = true;
    after.until(|service| !service.presentations.is_empty());
    let record = after.service.cadence_final().unwrap();
    assert!(
        record.starts_with(
            "lom_cadence_final schema=1 status=refused quota=2 presented=2 renders_started=3 renders_completed=3 uploads="
        ),
        "{record}"
    );
    assert!(record.ends_with(" pending_presentations=1"), "{record}");
    after.hold.presented.store(false, Ordering::SeqCst);
    assert!(after.failure().contains("beyond its one-panel quota"));
    after.finish();
}

/// The CLI owner loop itself, not the private enable: the cadence entry keeps
/// counters enabled across the reporter and repeats with samples, while the
/// reference entry still presents once without them.
#[test]
fn the_serve_owner_loop_enables_the_cadence_after_the_reporter() {
    assert!(std::env::var_os("SOPHIA_SHELL_WORK_STATS").is_none());
    for cadence in [Some(true), Some(false), None] {
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, service_wake) = UnixStream::pair().unwrap();
        let mut session = session_until_stopped(
            RendererWorker::start_cpu().unwrap(),
            1,
            (stop.clone(), Some(service_wake)),
        );
        // A stalled loop ends with the stop error instead of hanging.
        let (finished, deadline) = mpsc::channel::<()>();
        let watchdog_stop = stop.clone();
        let watchdog = std::thread::spawn(move || {
            if deadline.recv_timeout(Duration::from_secs(10)).is_err() {
                watchdog_stop.store(true, Ordering::SeqCst);
                let _ = std::io::Write::write_all(&mut &wake, &[1]);
            }
        });
        let goal = |service: &ShellService<RendererWorker>| match &service.cadence {
            Some(cadence) => cadence.presented >= 3,
            None => cadence.is_none() && service.panels[0].presented.is_some(),
        };
        let driven = crate::cli::drive_for_tests(cadence, &mut session.service, goal);
        // The watchdog may already have timed out and closed the receiver.
        let _ = finished.send(());
        watchdog.join().unwrap();
        driven.unwrap();
        if cadence.is_some() {
            let state = session.cadence();
            assert_eq!((state.quota, state.presented), (QUOTA, 3));
            assert_eq!(session.stats().renders_started, 3);
            assert!(session.service.panels[0].dirty, "redirtied below the quota");
        } else {
            session.settle();
            assert!(session.service.cadence.is_none());
            assert_eq!(session.service.statistics, None);
            let presented = session.service.panels[0].presented.as_ref().unwrap();
            assert_eq!(presented.candidate_generation, 1);
            assert!(!session.service.panels[0].dirty);
        }
        session.finish();
    }
}
