//! Real file/ring transport, production CPU worker and independently composed
//! peer resources. This is not a native compositor or scanout test.
use super::*;
use crate::service::tests::{
    files_peer::{Message, Peer},
    files_wire,
};
use crate::{
    config::parse_shell_config,
    model::{Workspace, WorkspaceSnapshot},
    render::RendererWorker,
    ui::PreviewDriver,
};
use sophia_shell_client::{IMAGE_CAPABILITY_OWNED_RING, ShellClientOptions};
use sophia_shell_protocol::*;
use std::{
    collections::BTreeMap,
    os::unix::net::UnixListener,
    sync::{atomic::AtomicU64, mpsc},
};

const GRANT: ContentGrant = ContentGrant {
    connection_epoch: files_wire::EPOCH,
    content_grant_epoch: 9,
};
const OUTPUT: ContentOutputId = ContentOutputId {
    id: 2,
    generation: 3,
};
static NEXT: AtomicU64 = AtomicU64::new(1);

pub(super) struct Worker {
    real: RendererWorker,
    partial: bool,
    pub(super) partial_completions: usize,
    pub(super) fault_at: Option<usize>,
}

impl ContentRenderer for Worker {
    fn submit(
        &mut self,
        _: RenderIdentity,
        _: Model,
        _: u32,
        _: u32,
        _: f64,
    ) -> Result<(), String> {
        Err("ring only".into())
    }
    fn uses_ring_fill(&self) -> bool {
        true
    }
    fn supports_regions(&self) -> bool {
        true
    }
    fn submit_fill(
        &mut self,
        id: RenderIdentity,
        model: Model,
        w: u32,
        h: u32,
        scale: f64,
        fill: sophia_shell_client::OwnedRingSlotFill,
    ) -> Result<(), (String, sophia_shell_client::OwnedRingSlotFill)> {
        self.partial = false;
        self.real.submit_fill(id, model, w, h, scale, fill)
    }
    fn submit_region(
        &mut self,
        id: RenderIdentity,
        model: Model,
        geometry: (u32, u32, f64),
        region: ContentPixelRect,
        fill: sophia_shell_client::OwnedRingSlotFill,
    ) -> Result<(), (String, sophia_shell_client::OwnedRingSlotFill)> {
        self.partial = (region.width, region.height) != (geometry.0, geometry.1);
        self.real.submit_region(id, model, geometry, region, fill)
    }
    fn clear_completion_wake(&mut self) -> Result<(), String> {
        self.real.clear_completion_wake()
    }
    fn poll(&mut self) -> Result<Option<RenderedContent>, String> {
        let Some(mut content) = self.real.poll()? else {
            return Ok(None);
        };
        if self.partial {
            self.partial_completions += 1;
            if self.fault_at == Some(self.partial_completions) {
                let RenderImage::RingFill { fill, opaque, .. } = &mut content.pixels else {
                    unreachable!()
                };
                fill.pixels_mut().unwrap()[..4].fill(0);
                *opaque = false;
            }
        }
        Ok(Some(content))
    }
}

#[derive(Debug)]
pub(super) struct Frame {
    pub(super) generation: u64,
    pub(super) pixels: Vec<u8>,
    pub(super) resources: Vec<(u64, u64)>,
    pub(super) upload_bytes: usize,
    pub(super) uploads: usize,
}

pub(super) enum Control {
    HoldSlots(bool),
    HoldResources(bool),
    Barrier(mpsc::Sender<()>),
    Stop,
}

pub(super) struct Session {
    pub(super) service: ShellService<Worker>,
    pub(super) commands: mpsc::Sender<Control>,
    frames: mpsc::Receiver<Frame>,
    server: Option<std::thread::JoinHandle<()>>,
    path: PathBuf,
    width: u32,
    height: u32,
}

fn server(
    listener: UnixListener,
    width: u32,
    height: u32,
    controls: mpsc::Receiver<Control>,
    frames: mpsc::Sender<Frame>,
) {
    let caps = SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | IMAGE_CAPABILITY_OWNED_RING;
    let peer = Peer::new(listener.accept().unwrap().0, caps);
    peer.content(
        TransactionId::from_raw(2),
        ShellContentRecord::OutputFacts(ContentOutputFacts {
            grant: GRANT,
            facts_generation: 4,
            outputs: vec![ContentOutputFactsEntry {
                output: OUTPUT,
                local_width: width,
                local_height: height,
                scale_numerator: 1,
                scale_denominator: 1,
                scale_generation: 5,
            }],
        }),
    );
    let Message::Content(tx, request) = peer.receive() else {
        panic!("allocation")
    };
    let ShellContentRecord::AllocationRequest(request) = *request else {
        panic!("allocation request")
    };
    peer.content(
        tx,
        ShellContentRecord::AllocationResult(ContentAllocationResult {
            grant: GRANT,
            allocation_request_id: request.allocation_request_id,
            status: 1,
            reason: 0,
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
                width,
                height,
            },
            pixel: full_rect(width, height),
            scale_numerator: 1,
            scale_denominator: 1,
            allowed_reservation_extent: width.min(height),
            margins: ContentMargins::default(),
            acknowledged_anchor: ContentPixelRect::default(),
        }),
    );
    let mut accepted = BTreeMap::new();
    let mut high_water = BTreeMap::new();
    let mut greatest = 0;
    let mut current = Vec::new();
    let mut held: Vec<(TransactionId, ContentResourceId)> = Vec::new();
    let mut hold_resources = false;
    let mut begin = None;
    let mut placements = Vec::new();
    let mut uploaded = (0, 0);
    loop {
        for control in controls.try_iter() {
            match control {
                Control::Stop => return,
                Control::Barrier(done) => {
                    peer.synchronize();
                    done.send(()).unwrap();
                }
                Control::HoldSlots(hold) => peer.hold_slot_releases(hold),
                Control::HoldResources(hold) => {
                    hold_resources = hold;
                    if !hold {
                        for (tx, resource) in held.drain(..) {
                            accepted.remove(&(resource.id, resource.generation));
                            peer.content(
                                tx,
                                ShellContentRecord::ResourceReleased(ContentResourceReleased {
                                    grant: GRANT,
                                    resource,
                                    reason: 0,
                                }),
                            );
                        }
                    }
                }
            }
        }
        let Some(message) = peer.try_receive(Duration::from_millis(1)) else {
            continue;
        };
        let (tx, record) = match message {
            Message::Image(metadata, pixels) => {
                let key = (metadata.resource, metadata.resource_generation);
                assert!(!accepted.contains_key(&key));
                if let Some(previous) = high_water.insert(key.0, key.1) {
                    assert_eq!(key.1, previous + 1);
                } else {
                    assert!(key.0 > greatest);
                    assert_eq!(key.1, 1);
                    greatest = key.0;
                }
                assert!(greatest <= RESOURCE_SLOTS as u64, "identity leak");
                uploaded.0 += 1;
                uploaded.1 += pixels.len();
                accepted.insert(key, (metadata.width, metadata.height, pixels));
                assert!(accepted.len() <= RESOURCE_SLOTS);
                continue;
            }
            Message::Content(tx, record) => (tx, *record),
            Message::Activation(..) => panic!("unexpected activation"),
        };
        match record {
            ShellContentRecord::FrameDemand(demand) => peer.content(
                tx,
                ShellContentRecord::FramePermit(ContentFramePermit {
                    grant: GRANT,
                    output: OUTPUT,
                    demand_id: demand.demand_id,
                    permit_id: demand.demand_id + 20,
                    state: 1,
                    reason: 0,
                    ttl_ms: 250,
                    max_candidate_bytes: 8192,
                }),
            ),
            ShellContentRecord::CandidateBegin(row) => {
                assert!(begin.is_none());
                begin = Some((tx, row));
                placements.clear();
            }
            ShellContentRecord::CandidateChunk(row) => placements.extend(row.placements),
            ShellContentRecord::CandidateEnd(end) => {
                let (tx, begin) = begin.take().unwrap();
                assert_eq!(begin.candidate_generation, end.candidate_generation);
                assert_eq!(begin.placement_count, placements.len() as u32);
                assert_eq!(end.placement_count, begin.placement_count);
                let mut pixels = vec![0xa5; (width * height * 4) as usize];
                let mut coverage = vec![0u8; (width * height) as usize];
                current.clear();
                for placement in &placements {
                    let key = (placement.resource.id, placement.resource.generation);
                    current.push(key);
                    let (w, h, bytes) = accepted
                        .get(&key)
                        .expect("placement requires accepted bytes");
                    let (x, y) = (
                        placement.destination_x_px as u32,
                        placement.destination_y_px as u32,
                    );
                    assert!(x + w <= width && y + h <= height);
                    for row in 0..*h {
                        let src = (row * w * 4) as usize;
                        let dst = ((y + row) * width + x) as usize;
                        pixels[dst * 4..dst * 4 + *w as usize * 4]
                            .copy_from_slice(&bytes[src..src + *w as usize * 4]);
                        for n in &mut coverage[dst..dst + *w as usize] {
                            *n += 1;
                        }
                    }
                }
                assert!(
                    coverage.iter().all(|n| *n == 1),
                    "whole-panel coverage without overlaps"
                );
                frames
                    .send(Frame {
                        generation: begin.candidate_generation,
                        pixels,
                        resources: current.clone(),
                        uploads: uploaded.0,
                        upload_bytes: uploaded.1,
                    })
                    .unwrap();
                uploaded = (0, 0);
                peer.content(
                    tx,
                    ShellContentRecord::CandidateOutcome(ContentCandidateOutcome {
                        grant: GRANT,
                        candidate_generation: begin.candidate_generation,
                        output: OUTPUT,
                        kind: 2,
                        reason: 0,
                        presentation_epoch: begin.candidate_generation + 30,
                        work_area_generation: begin.candidate_generation + 8,
                        wm_commit_generation: begin.candidate_generation + 9,
                    }),
                );
            }
            ShellContentRecord::ResourceRetire(row) => {
                let key = (row.resource.id, row.resource.generation);
                assert!(
                    !current.contains(&key),
                    "retire only after excluding Presented"
                );
                assert!(accepted.contains_key(&key));
                assert!(
                    !held
                        .iter()
                        .any(|(_, r): &(_, ContentResourceId)| r == &row.resource),
                    "duplicate retire"
                );
                if hold_resources {
                    held.push((tx, row.resource));
                } else {
                    accepted.remove(&key);
                    peer.content(
                        tx,
                        ShellContentRecord::ResourceReleased(ContentResourceReleased {
                            grant: GRANT,
                            resource: row.resource,
                            reason: 0,
                        }),
                    );
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

impl Session {
    pub(super) fn control(&self, command: Control) {
        let (send, receive) = mpsc::channel();
        self.commands.send(command).unwrap();
        self.commands.send(Control::Barrier(send)).unwrap();
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    pub(super) fn new(height: u32) -> Self {
        Self::geometry(1920, height, Position::Top)
    }

    pub(super) fn geometry(width: u32, height: u32, position: Position) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lom-regions-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let (commands, controls) = mpsc::channel();
        let (send, frames) = mpsc::channel();
        let server = std::thread::spawn(move || server(listener, width, height, controls, send));
        let caps = SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
            | SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
            | IMAGE_CAPABILITY_OWNED_RING;
        let connection = ShellConnection::connect_files(
            &path,
            ShellClientOptions {
                minimum_revision: 10,
                maximum_revision: 10,
                required_capabilities: caps,
                handshake_timeout: Duration::from_secs(2),
            },
        )
        .unwrap();
        let position = match position {
            Position::Top => "top",
            Position::Bottom => "bottom",
            Position::Left => "left",
            Position::Right => "right",
        };
        let extent = width.min(height);
        let source=include_str!("../fixtures/redraw/static24.kdl").replace("height 24",&format!("height {extent}"))
            .replace("position \"top\"", &format!("position \"{position}\""))
            .replace("    start {","    center { clock \"minutes\" { format \"%H:%M\"; }; }\n    end { clock \"clock\" { format \"%H:%M:%S\"; }; }\n    start {");
        let (config, theme) = parse_shell_config(&source).unwrap();
        let renderer = Worker {
            real: RendererWorker::start_cpu().unwrap(),
            partial: false,
            partial_completions: 0,
            fault_at: None,
        };
        let mut service = ShellService::new(
            connection,
            config,
            theme,
            extent,
            renderer,
            path.parent().unwrap().into(),
        )
        .unwrap();
        let now = DateTime::<Utc>::UNIX_EPOCH.fixed_offset();
        service.observed_clock = Some(now);
        service.last_second = 0;
        let model = &mut service.panels[0].model;
        model.time = now;
        model.epoch = files_wire::EPOCH;
        model.workspaces = WorkspaceSnapshot {
            epoch: files_wire::EPOCH,
            generation: 1,
            active_output: Some(OUTPUT.id),
            entries: (1..=9)
                .map(|id| Workspace {
                    id,
                    output: OUTPUT.id,
                    name: id.to_string(),
                    active: id == 1,
                    visible: false,
                    urgent: false,
                    action: Some(id + 10),
                })
                .collect(),
        };
        Self {
            service,
            commands,
            frames,
            server: Some(server),
            path,
            width,
            height,
        }
    }

    pub(super) fn set(&mut self, seconds: i64, active: u64) -> Model {
        let now = DateTime::<Utc>::UNIX_EPOCH.fixed_offset() + chrono::Duration::seconds(seconds);
        self.service.observed_clock = Some(now);
        let panel = &mut self.service.panels[0];
        panel.model.time = now;
        panel.model.workspaces.generation += 1;
        for entry in &mut panel.model.workspaces.entries {
            entry.active = entry.id == active;
        }
        panel.dirty = true;
        panel.interaction_dirty = true;
        panel.model.clone()
    }

    pub(super) fn until(&mut self, ready: impl Fn(&ShellService<Worker>) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready(&self.service) {
            assert!(Instant::now() < deadline, "region service stalled");
            self.service.step().unwrap();
            std::thread::yield_now();
        }
    }

    pub(super) fn frame(&mut self, expected: &Model) -> Frame {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "region presentation stalled");
            if self.service.step().unwrap() > 0 {
                break;
            }
            std::thread::yield_now();
        }
        let frame = self.frames.recv_timeout(Duration::from_secs(1)).unwrap();
        let presented = self.service.panels[0].presented.as_ref().unwrap();
        assert_eq!(frame.generation, presented.candidate_generation);
        assert_eq!(&presented.model, expected);
        let mut scene = PreviewDriver::new(expected.clone(), self.width, self.height, 1.0, false)
            .unwrap()
            .scene();
        use masonry::imaging::render::ImageRenderer;
        let image = imaging_vello_cpu::VelloCpuRenderer::new(1, 1)
            .render_source(&mut scene.scene, self.width, self.height)
            .unwrap();
        let full = crate::protocol::ContentPixels::from_rgba8(self.width, self.height, image.data)
            .unwrap();
        assert_eq!(
            frame.pixels,
            full.bytes(),
            "peer composite versus full resolved update model"
        );
        frame
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.commands.send(Control::Stop);
        if let Some(server) = self.server.take() {
            let result = server.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
        let _ = std::fs::remove_file(&self.path);
    }
}
