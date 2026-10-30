//! The owner's acquire-time snapshot is rechecked at completion and again
//! immediately before commit. Each case mutates one owner field between the
//! two stages over a real SDK loan and ring peer; the renderer is scripted.

use super::*;
use crate::service::tests::{files_peer, files_wire};
use sophia_shell_client::{IMAGE_CAPABILITY_OWNED_RING, ShellClientOptions};
use sophia_shell_protocol::{
    ContentAllocationId, ContentGrant, ContentLogicalRect, ContentMargins, ContentOutputFacts,
    ContentOutputId, ContentPixelRect, ContentReason, SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE,
    SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER, TransactionId,
};
use std::os::unix::net::UnixListener;
use std::sync::atomic::AtomicU64;

static SOCKET_ID: AtomicU64 = AtomicU64::new(1);

const GRANT: ContentGrant = ContentGrant {
    connection_epoch: files_wire::EPOCH,
    content_grant_epoch: 9,
};
const OUTPUT: ContentOutputId = ContentOutputId {
    id: 2,
    generation: 3,
};

/// Holds each lent payload until the test releases its completion.
#[derive(Default)]
struct Lender {
    job: Option<(RenderIdentity, OwnedRingSlotFill, u32, u32)>,
    jobs: usize,
    release: bool,
}

impl ContentRenderer for Lender {
    fn submit(
        &mut self,
        _identity: RenderIdentity,
        _model: Model,
        _width: u32,
        _height: u32,
        _scale: f64,
    ) -> Result<(), String> {
        Err("a ring-fill renderer takes no slice job".into())
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
        self.jobs += 1;
        self.job = Some((identity, fill, width, height));
        Ok(())
    }

    fn poll(&mut self) -> Result<Option<RenderedContent>, String> {
        if !std::mem::take(&mut self.release) {
            return Ok(None);
        }
        let (identity, fill, width, height) = self.job.take().ok_or("no lent job")?;
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

struct Harness<R = Lender> {
    service: ShellService<R>,
    done: std::sync::mpsc::Sender<()>,
    /// The bytes of every Offer the peer read.
    offers: std::sync::mpsc::Receiver<Vec<u8>>,
    server: std::thread::JoinHandle<()>,
    path: std::path::PathBuf,
}

fn harness() -> Harness {
    harness_with(Lender::default())
}

/// One output, allocated and Ready; the peer reports every Offer it reads.
fn harness_with<R: ContentRenderer>(renderer: R) -> Harness<R> {
    let path = std::env::temp_dir().join(format!(
        "lom-fill-snapshot-{}-{}.sock",
        std::process::id(),
        SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let capabilities = SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | IMAGE_CAPABILITY_OWNED_RING;
    let (done, wait) = std::sync::mpsc::channel::<()>();
    let (offered, offers) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let peer = files_peer::Peer::new(listener.accept().unwrap().0, capabilities);
        peer.content(
            TransactionId::from_raw(2),
            ShellContentRecord::OutputFacts(ContentOutputFacts {
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
        );
        let files_peer::Message::Content(transaction, record) = peer.receive() else {
            panic!("expected allocation request");
        };
        let ShellContentRecord::AllocationRequest(request) = *record else {
            panic!("expected allocation request");
        };
        peer.content(
            transaction,
            ShellContentRecord::AllocationResult(ContentAllocationResult {
                grant: GRANT,
                allocation_request_id: request.allocation_request_id,
                status: 1,
                reason: ContentReason::None as u16,
                output: OUTPUT,
                allocation: ContentAllocationId {
                    id: 11,
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
        while wait.try_recv().is_err() {
            if let Some(files_peer::Message::Image(_, pixels)) =
                peer.try_receive(Duration::from_millis(20))
            {
                offered.send(pixels).unwrap();
            }
        }
    });
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
        crate::config::parse_shell_config(include_str!("../../examples/minimal/shell.kdl"))
            .unwrap();
    let service = ShellService::new(
        connection,
        config,
        theme,
        48,
        renderer,
        path.parent().unwrap().to_path_buf(),
    )
    .unwrap();
    Harness {
        service,
        done,
        offers,
        server,
        path,
    }
}

impl<R: ContentRenderer> Harness<R> {
    fn until(&mut self, what: &str, ready: impl Fn(&ShellService<R>) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready(&self.service) {
            assert!(Instant::now() < deadline, "{what} stalled");
            self.service.step().unwrap();
            std::thread::yield_now();
        }
    }

    /// Offers the peer read.
    fn finish(self) -> usize {
        self.done.send(()).unwrap();
        drop(self.service);
        self.server.join().unwrap();
        let _ = std::fs::remove_file(&self.path);
        self.offers.try_iter().count()
    }
}

impl Harness {
    /// Lend the first payload, as the scheduler does once the ring is Ready.
    fn lent(&mut self) {
        self.until("first loan", |service| service.renderer.job.is_some());
    }

    /// What a spent resource ID or slot generation would change.
    fn spend(&self) -> (u64, [(u64, u64); 2]) {
        let slots = self.service.panels[0]
            .resources
            .map(|slot| (slot.id, slot.generation));
        (self.service.next_resource, slots)
    }

    /// The single loan was aborted, not dropped: a dropped loan faults the
    /// ring and the next acquire is refused.
    fn loan_returned(&mut self) -> bool {
        let len = ContentPixels::byte_len(8, 48).unwrap();
        let Ok(mut fill) = self
            .service
            .connection
            .image_acquire_ring_fill(len, Instant::now())
        else {
            return false;
        };
        self.service
            .connection
            .image_abort_ring_fill(&mut fill)
            .is_ok()
    }
}

type Mutation = (&'static str, fn(&mut ShellService<Lender>));

/// One owner field per case; none is reported by the worker.
const SNAPSHOT: [Mutation; 12] = [
    ("content grant", |s| s.limits.grant.content_grant_epoch += 1),
    ("connection epoch", |s| s.limits.grant.connection_epoch += 1),
    ("output", |s| s.panels[0].output.output.generation += 1),
    ("allocation", |s| {
        s.panels[0].allocation.allocation.generation += 1
    }),
    ("scale generation", |s| {
        s.panels[0].allocation.scale_generation += 1
    }),
    ("scale fraction", |s| {
        s.panels[0].allocation.scale_numerator += 1
    }),
    ("geometry", |s| s.panels[0].allocation.pixel.width -= 1),
    ("topology", |s| s.facts.facts_generation += 1),
    // None and Some(1) both target slot 0; the original current slot is bound.
    ("current slot", |s| s.panels[0].current_slot = Some(1)),
    ("resource id", |s| s.panels[0].resources[0].id = 7),
    ("resource generation", |s| {
        s.panels[0].resources[0].generation += 1
    }),
    ("peeked resource id", |s| s.next_resource += 1),
];

#[test]
fn a_fill_stale_at_completion_is_aborted_and_rendered_again() {
    let retiring: Mutation = ("slot state", |s| {
        s.panels[0].resources[0].state = ResourceState::Retiring
    });
    for (name, mutate) in SNAPSHOT.into_iter().chain([retiring]) {
        let mut harness = harness();
        harness.lent();
        mutate(&mut harness.service);
        let spent = harness.spend();
        harness.service.renderer.release = true;
        harness.service.step().unwrap();
        assert!(harness.service.presentations.is_empty(), "{name}");
        assert_eq!(harness.spend(), spent, "{name}: nothing spent");
        // The aborted loan is lent again at once, or (when the slot is no
        // longer free) is still acquirable.
        if harness.service.renderer.job.is_some() {
            assert_eq!(harness.service.renderer.jobs, 2, "{name}");
        } else {
            assert!(harness.service.panels[0].dirty, "{name}");
            assert!(harness.loan_returned(), "{name}");
        }
        assert_eq!(harness.finish(), 0, "{name}: no Offer");
    }
}

#[test]
fn a_fill_stale_before_commit_is_aborted_and_terminal() {
    let freed: Mutation = ("slot state", |s| {
        s.panels[0].resources[0].state = ResourceState::Free
    });
    let bytes: Mutation = ("staged bytes", |s| s.panels[0].resources[0].bytes += 4);
    for (name, mutate) in SNAPSHOT.into_iter().chain([freed, bytes]) {
        let mut harness = harness();
        harness.lent();
        harness.service.renderer.release = true;
        // Completion accepts and stages; the commit waits for the next turn.
        harness.service.step().unwrap();
        assert_eq!(harness.service.presentations.len(), 1, "{name}");
        mutate(&mut harness.service);
        let spent = harness.spend();
        let error = harness.service.step().unwrap_err();
        assert!(error.contains("stale before commit"), "{name}: {error}");
        assert_eq!(harness.spend(), spent, "{name}: nothing spent");
        assert!(harness.loan_returned(), "{name}: aborted before return");
        assert_eq!(harness.finish(), 0, "{name}: no Offer");
    }
}

#[test]
fn a_presentation_deadline_before_commit_aborts_the_loan_and_is_terminal() {
    let mut harness = harness();
    harness.lent();
    harness.service.renderer.release = true;
    harness.service.step().unwrap();
    harness.service.presentations[0].deadline = Instant::now();
    let spent = harness.spend();
    let error = harness.service.step().unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    assert_eq!(harness.spend(), spent);
    assert!(harness.loan_returned());
    assert_eq!(harness.finish(), 0);
}

#[test]
fn an_unchanged_snapshot_commits_its_expected_metadata_once() {
    let mut harness = harness();
    harness.lent();
    harness.service.renderer.release = true;
    harness.service.step().unwrap();
    let (next, _) = harness.spend();
    harness.until("commit", |service| service.next_resource == next + 1);
    assert_eq!(harness.service.panels[0].resources[0].id, next);
    // The Offer reaches the peer as later turns flush the image channel.
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.offers.try_recv().is_err() {
        assert!(Instant::now() < deadline, "peer never read the Offer");
        harness.service.step().unwrap();
        std::thread::yield_now();
    }
    assert_eq!(harness.finish(), 0, "exactly one Offer");
}

/// Real workers over the real SDK connection and ring peer: the diagnostic
/// copy arm's existing slice upload reaches the peer with the direct fill's
/// exact bytes.
#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn the_real_copy_reference_worker_offers_the_direct_fill_peer_bytes() {
    use crate::render::RendererWorker;
    use std::os::fd::AsFd;
    fn offered(worker: RendererWorker) -> Vec<u8> {
        assert!(!worker.uses_dma_buf());
        let mut harness = harness_with(worker);
        let deadline = Instant::now() + Duration::from_secs(10);
        let pixels = loop {
            if let Ok(pixels) = harness.offers.try_recv() {
                break pixels;
            }
            assert!(Instant::now() < deadline, "the peer never read an Offer");
            harness.service.step().unwrap();
            std::thread::yield_now();
        };
        harness.finish();
        pixels
    }
    let stop = std::sync::atomic::AtomicBool::new(false);
    let (wake, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let direct = RendererWorker::start_cpu_until_stopped(&stop, wake.as_fd()).unwrap();
    assert!(direct.uses_ring_fill());
    let direct = offered(direct);
    let copy = RendererWorker::start_cpu_copy_reference_until_stopped(&stop, wake.as_fd()).unwrap();
    assert!(!copy.uses_ring_fill(), "the copy arm uses the slice upload");
    let copy = offered(copy);
    assert_eq!(direct.len(), ContentPixels::byte_len(8, 48).unwrap());
    assert!(direct.iter().any(|byte| *byte != 0), "rendered, not blank");
    assert_eq!(copy, direct);
}
