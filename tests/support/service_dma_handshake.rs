//! A DMA renderer selects the DMA image adapter, so Session's capture grant
//! (ChannelReady source kinds 6) reaches Ready instead of a ring-only fault.
//! Handshake only: the renderer never produces a frame and no GPU is opened.

use super::*;
use sophia_shell_client::{IMAGE_CAPABILITY_DMA_BUF, IMAGE_CAPABILITY_OWNED_RING};
use sophia_shell_client::{ImageChannelState, ShellClientOptions};
use sophia_shell_protocol::{
    ContentAllocationId, ContentGrant, ContentLogicalRect, ContentMargins, ContentOutputFacts,
    ContentOutputId, ContentPixelRect, ContentReason, SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE,
    SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER, TransactionId,
};
use std::os::unix::net::UnixListener;

#[allow(dead_code)]
#[path = "files_peer.rs"]
pub(crate) mod files_peer;
#[allow(dead_code)]
#[path = "files_wire.rs"]
pub(crate) mod files_wire;

const GRANT: ContentGrant = ContentGrant {
    connection_epoch: files_wire::EPOCH,
    content_grant_epoch: 9,
};
const OUTPUT: ContentOutputId = ContentOutputId {
    id: 2,
    generation: 3,
};

struct PendingDmaRenderer;

impl ContentRenderer for PendingDmaRenderer {
    fn uses_dma_buf(&self) -> bool {
        true
    }

    fn submit(
        &mut self,
        _identity: RenderIdentity,
        _model: Model,
        _width: u32,
        _height: u32,
        _scale: f64,
    ) -> Result<(), String> {
        Ok(())
    }

    fn poll(&mut self) -> Result<Option<RenderedContent>, String> {
        Ok(None)
    }
}

#[test]
fn a_dma_renderer_reaches_ready_on_a_capture_granted_image_channel() {
    let path = std::env::temp_dir().join(format!("lom-dma-ready-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let capabilities = SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | IMAGE_CAPABILITY_OWNED_RING
        | IMAGE_CAPABILITY_DMA_BUF;
    let (done, wait) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let peer = files_peer::Peer::with_dma(listener.accept().unwrap().0, capabilities);
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
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
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
    let mut service = ShellService::new(
        connection,
        config,
        theme,
        48,
        PendingDmaRenderer,
        path.parent().unwrap().to_path_buf(),
    )
    .unwrap();
    let channel = |service: &ShellService<PendingDmaRenderer>| {
        service
            .connection
            .image_adapter()
            .and_then(|adapter| adapter.channel())
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while channel(&service) != Some(ImageChannelState::Ready) {
        // The ring-only adapter faults on granted kinds 6 and reports Closed.
        assert_ne!(channel(&service), Some(ImageChannelState::Closed));
        assert!(
            Instant::now() < deadline,
            "image channel did not reach Ready"
        );
        service.step().unwrap();
        service.wait_for_work().unwrap();
    }
    done.send(()).unwrap();
    drop(service);
    server.join().unwrap();
    let _ = std::fs::remove_file(path);
}
