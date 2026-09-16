//! Persistent, display-independent Sophia content lifecycle.

mod presentation;
mod scheduler;
use presentation::PendingPresentation;

use crate::{
    config::{ModuleKind, PanelConfig, Position, Theme},
    model::{Model, Workspace, WorkspaceSnapshot},
    protocol::ContentPixels,
    ui::ContentTargetLayout,
};
use chrono::{DateTime, FixedOffset, Utc};
use sophia_protocol::{
    ContentAction, ContentActionAck, ContentAllocationId, ContentAllocationRequest,
    ContentAllocationResult, ContentCandidateBegin, ContentCandidateChunk, ContentCandidateEnd,
    ContentFrameDemand, ContentFramePermit, ContentMargins, ContentOutputFacts,
    ContentOutputFactsEntry, ContentPixelRect, ContentPlacement, ContentReason,
    ContentResourceBegin, ContentResourceChunk, ContentResourceEnd, ContentResourceId,
    ContentResourceRetire, ContentSurface, ContentTarget, POLICY_INDICATOR_STATE_ACTIVE,
    POLICY_INDICATOR_STATE_URGENT, POLICY_INDICATOR_STATE_VISIBLE_ELSEWHERE, ShellContentRecord,
    ShellIndicatorActivation, ShellIndicatorActivationStatus, ShellIndicatorSnapshot,
    TransactionId,
};
use sophia_shell_client::{ContentActionDispatch, ContentLifecycle, ShellConnection};
use std::{
    collections::{BTreeSet, VecDeque},
    time::{Duration, Instant, SystemTime},
};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const IDLE_POLL: Duration = Duration::from_millis(4);
const MAX_OBSERVATIONS_PER_TURN: usize = 64;
const MAX_PENDING_CONTENT: usize = 64;
const PANEL_ROLE: u16 = 1;
const CREATE_ALLOCATION: u16 = 1;
const RESOURCE_ACCEPTED: u16 = 2;
const PERMIT_GRANTED: u16 = 1;
const CANDIDATE_PRESENTED: u16 = 2;

/// Pixels and action targets resolved by one immutable layout pass.
#[derive(Debug, PartialEq)]
pub struct RenderedContent {
    /// Complete panel pixels.
    pub pixels: ContentPixels,
    /// Workspace hit regions from the same Masonry layout.
    pub targets: Vec<ContentTargetLayout>,
}

/// Exact lifetime of one acknowledged allocation's retained UI host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderIdentity {
    /// Admitted connection and content grant.
    pub grant: sophia_protocol::ContentGrant,
    /// Logical output and its topology generation.
    pub output: sophia_protocol::ContentOutputId,
    /// Engine-owned allocation and generation.
    pub allocation: ContentAllocationId,
    /// Scale identity from the acknowledged allocation.
    pub scale_generation: u64,
}

/// Produces one complete panel resource from passive application state.
pub trait ContentRenderer {
    /// Queue exactly one render for the acknowledged physical allocation.
    fn submit(
        &mut self,
        identity: RenderIdentity,
        model: Model,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<(), String>;

    /// Poll the sole in-flight render without blocking protocol ownership.
    fn poll(&mut self) -> Result<Option<RenderedContent>, String>;
}

struct Panel {
    output: ContentOutputFactsEntry,
    allocation: ContentAllocationResult,
    model: Model,
    dirty: bool,
    interaction_dirty: bool,
    current_slot: Option<usize>,
    presented: Option<PresentedPanel>,
    resources: [ResourceSlot; 2],
}

#[derive(Clone, Debug)]
struct PresentedPanel {
    model: Model,
    candidate_generation: u64,
    presentation_epoch: u64,
    targets: Vec<ContentTargetLayout>,
    raster: RasterSummary,
}

#[derive(Clone, Copy, Debug)]
struct RasterSummary {
    width: u32,
    height: u32,
    bytes: usize,
    checksum: u64,
}

#[derive(Clone, Copy)]
struct ResourceSlot {
    id: u64,
    generation: u64,
    bytes: u64,
    state: ResourceState,
    release_deadline: Option<Instant>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ResourceState {
    Free,
    Staging,
    Resident,
    Retiring,
}

/// One persistent, display-independent Sophia content client.
///
/// The owner serializes observations, uploads, pacing and retirement. The
/// supplied renderer owns toolkit/GPU policy and returns immutable wire pixels.
pub struct ShellService<R> {
    connection: ShellConnection,
    config: PanelConfig,
    theme: Theme,
    renderer: R,
    limits: sophia_protocol::ContentLimits,
    facts: ContentOutputFacts,
    panels: Vec<Panel>,
    next_transaction: u64,
    next_demand: u64,
    next_candidate_generation: u64,
    next_resource: u64,
    upload_chunks_left: usize,
    last_second: u64,
    latest_indicators: Option<ShellIndicatorSnapshot>,
    pending_content: VecDeque<(TransactionId, ShellContentRecord)>,
    lifecycle: ContentLifecycle,
    rendering_revision: u64,
    rendering_model: Option<Model>,
    activation_high_water: u64,
    pending_activation_events: BTreeSet<u64>,
    presentations: Vec<PendingPresentation>,
    rendering_panel: Option<usize>,
    next_panel: usize,
}

impl<R: ContentRenderer> ShellService<R> {
    /// Establish the initial limits, output facts and panel allocations.
    pub fn new(
        mut connection: ShellConnection,
        config: PanelConfig,
        theme: Theme,
        allowance: u32,
        renderer: R,
    ) -> Result<Self, String> {
        let (_, ShellContentRecord::Limits(limits)) = receive_content(&mut connection)? else {
            return Err("first content record was not ContentLimits".into());
        };
        limits
            .validate()
            .map_err(|error| format!("invalid content limits: {error:?}"))?;
        let (facts_transaction, ShellContentRecord::OutputFacts(facts)) =
            receive_content(&mut connection)?
        else {
            return Err("ContentLimits was not followed by ContentOutputFacts".into());
        };
        if facts.outputs.is_empty() {
            return Err("content service published no outputs".into());
        }
        let pending_capacity = limits.max_pending_candidates_total as usize;
        let mut lifecycle = ContentLifecycle::new(limits.clone())
            .map_err(|error| format!("content lifecycle: {error:?}"))?;
        lifecycle
            .dispatch(
                facts_transaction,
                ShellContentRecord::OutputFacts(facts.clone()),
            )
            .map_err(|error| format!("initial output facts: {error:?}"))?;
        let mut service = Self {
            connection,
            config,
            theme,
            renderer,
            lifecycle,
            rendering_revision: 0,
            rendering_model: None,
            limits,
            facts,
            panels: Vec::new(),
            next_transaction: 1,
            next_demand: 1,
            next_candidate_generation: 1,
            next_resource: 1,
            upload_chunks_left: 4,
            last_second: unix_second(),
            latest_indicators: None,
            pending_content: VecDeque::new(),
            activation_high_water: 0,
            pending_activation_events: BTreeSet::new(),
            presentations: Vec::with_capacity(pending_capacity),
            rendering_panel: None,
            next_panel: 0,
        };
        service.allocate_panels(allowance)?;
        Ok(service)
    }

    /// Make one bounded observation turn and, when dirty, present one complete
    /// replacement on every output. Returns the number of outputs presented.
    pub fn step(&mut self) -> Result<usize, String> {
        self.service_turn()
    }
    fn transaction(&mut self) -> Result<TransactionId, String> {
        let value = self.next_transaction;
        self.next_transaction = value
            .checked_add(1)
            .ok_or("transaction identity exhausted")?;
        Ok(TransactionId::from_raw(value))
    }

    fn send(&mut self, record: ShellContentRecord) -> Result<(), String> {
        let transaction = self.transaction()?;
        self.connection
            .enqueue_content(transaction, &record)
            .map_err(|error| format!("content send failed: {error}"))
    }

    fn allocate_panels(&mut self, allowance: u32) -> Result<(), String> {
        let outputs = self.facts.outputs.clone();
        for (index, output) in outputs.into_iter().enumerate() {
            let request_id = u64::try_from(index + 1).map_err(|_| "too many outputs")?;
            let (desired_width, desired_height) = match self.config.position {
                Position::Top | Position::Bottom => (output.local_width, self.config.height),
                Position::Left | Position::Right => (self.config.height, output.local_height),
            };
            self.send(ShellContentRecord::AllocationRequest(
                ContentAllocationRequest {
                    grant: self.limits.grant,
                    output: output.output,
                    allocation_request_id: request_id,
                    operation: CREATE_ALLOCATION,
                    role: PANEL_ROLE,
                    edge: edge(self.config.position),
                    prior: ContentAllocationId::default(),
                    parent: ContentAllocationId::default(),
                    parent_presentation_epoch: 0,
                    anchor_parent_rect: ContentPixelRect::default(),
                    desired_width,
                    desired_height,
                    margins: margins(self.config.margins)?,
                },
            ))?;
            let allocation = loop {
                let record = self.wait_record()?;
                if let ShellContentRecord::AllocationResult(result) = record
                    && result.allocation_request_id == request_id
                {
                    break result;
                }
            };
            if allocation.status != 1 || allocation.reason != ContentReason::None as u16 {
                return Err(format!(
                    "panel allocation {request_id} was refused: {}",
                    allocation.reason
                ));
            }
            let panel_extent = match self.config.position {
                Position::Top | Position::Bottom => allocation.pixel.height,
                Position::Left | Position::Right => allocation.pixel.width,
            };
            if panel_extent > allowance {
                return Err("acknowledged panel exceeds the admitted physical allowance".into());
            }
            if allocation.allowed_reservation_extent < panel_extent {
                return Err(
                    "Engine acknowledged less reservation than the rendered panel thickness".into(),
                );
            }
            let mut model = Model::new(
                self.config.clone(),
                self.theme.clone(),
                output.output.id,
                observed_time(),
            );
            if let Some(snapshot) = &self.latest_indicators {
                model.workspaces = workspace_snapshot(snapshot);
                model.epoch = snapshot.connection_epoch;
            }
            self.panels.push(Panel {
                output,
                allocation,
                model,
                dirty: true,
                interaction_dirty: true,
                current_slot: None,
                presented: None,
                resources: [ResourceSlot {
                    id: 0,
                    generation: 1,
                    bytes: 0,
                    state: ResourceState::Free,
                    release_deadline: None,
                }; 2],
            });
        }
        Ok(())
    }

    fn observe(&mut self) -> Result<(), String> {
        self.connection
            .poll_io()
            .map_err(|error| format!("shell I/O: {error}"))?;
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((_, snapshot)) = self
                .connection
                .take_indicators()
                .map_err(|error| format!("indicator receive failed: {error}"))?
            else {
                break;
            };
            if snapshot.connection_epoch != self.connection.connection_epoch() {
                return Err("indicator snapshot belongs to a different connection".into());
            }
            self.latest_indicators = Some(snapshot.clone());
            let workspaces = workspace_snapshot(&snapshot);
            for panel in &mut self.panels {
                let mut next = panel.model.clone();
                next.workspaces = workspaces.clone();
                next.epoch = workspaces.epoch;
                panel.dirty |= !crate::modules::same_panel_pixels(&panel.model, &next);
                panel.interaction_dirty |= panel.model.workspaces != next.workspaces;
                panel.model = next;
            }
        }
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((_, outcome)) = self
                .connection
                .take_indicator_activation_outcome()
                .map_err(|error| format!("indicator outcome receive failed: {error}"))?
            else {
                break;
            };
            self.lifecycle.finish_action(outcome.event_id);
            if !self.pending_activation_events.remove(&outcome.event_id) {
                continue;
            }
            if outcome.status != ShellIndicatorActivationStatus::Accepted {
                eprintln!(
                    "lom_workspace_activation schema=1 status=rejected event_id={} reason={}",
                    outcome.event_id, outcome.reason
                );
            }
        }
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((transaction, record)) = self
                .connection
                .take_content()
                .map_err(|error| format!("shell content receive failed: {error}"))?
            else {
                break;
            };
            let dispatch = self
                .lifecycle
                .dispatch(transaction, record)
                .map_err(|error| format!("ordered content lifecycle: {error:?}"))?;
            let record = dispatch.record;
            if let ShellContentRecord::Action(action) = record {
                if dispatch.action != Some(ContentActionDispatch::Cancelled) {
                    self.handle_content_action(
                        action,
                        dispatch.action == Some(ContentActionDispatch::Eligible),
                    )?;
                }
                continue;
            }
            if let ShellContentRecord::ResourceReleased(released) = &record {
                self.release_resource(released)?;
                continue;
            }
            if let ShellContentRecord::CandidateOutcome(outcome) = &record
                && outcome.kind == CANDIDATE_PRESENTED
            {
                self.install_presented_targets(outcome)?;
            }
            if let ShellContentRecord::OutputFacts(facts) = &record {
                if facts != &self.facts {
                    return Err(
                        "content output facts changed; restart required for fresh allocations"
                            .into(),
                    );
                }
                continue;
            }
            if self.pending_content.len() == MAX_PENDING_CONTENT {
                return Err("shell content response queue saturated".into());
            }
            self.pending_content.push_back((transaction, record));
        }
        Ok(())
    }

    fn handle_content_action(
        &mut self,
        action: ContentAction,
        eligible: bool,
    ) -> Result<(), String> {
        let mut message = None;
        if eligible
            && action.kind == 1
            && action.reason == ContentReason::None as u16
            && action.event_id != 0
            && action.event_id > self.activation_high_water
            && action.grant == self.limits.grant
        {
            message = self.panels.iter().find_map(|panel| {
                let presented = panel.presented.as_ref()?;
                if panel.output.output != action.output
                    || panel.allocation.allocation != action.allocation
                    || presented.candidate_generation != action.candidate_generation
                    || presented.presentation_epoch != action.presentation_epoch
                    || action.interaction_generation != 1
                {
                    return None;
                }
                presented
                    .targets
                    .iter()
                    .find(|target| {
                        target.indicator == action.target_id
                            && target.generation == action.target_generation
                            && target.action == action.action_id
                    })
                    .map(|target| target.message.clone())
            });
        }
        let accepted = if let Some(message) = message {
            let panel = self
                .panels
                .iter_mut()
                .find(|panel| panel.output.output == action.output)
                .ok_or("content action output disappeared")?;
            let mut observed = panel
                .presented
                .as_ref()
                .ok_or("presented action lost its model")?
                .model
                .clone();
            crate::update::update(&mut observed, message)
                .into_iter()
                .any(|effect| {
                    matches!(effect,
                    crate::update::Effect::ActivateWorkspace { epoch, generation, indicator, action: id }
                        if epoch == self.connection.connection_epoch()
                            && generation == action.target_generation
                            && indicator == action.target_id
                            && id == action.action_id)
                })
        } else {
            false
        };
        let ack = ContentActionAck {
            grant: action.grant,
            output: action.output,
            candidate_generation: action.candidate_generation,
            presentation_epoch: action.presentation_epoch,
            interaction_generation: action.interaction_generation,
            allocation: action.allocation,
            target_id: action.target_id,
            target_generation: action.target_generation,
            action_id: action.action_id,
            event_id: action.event_id,
            disposition: if accepted { 1 } else { 2 },
        };
        if action.grant == self.limits.grant && action.event_id > self.activation_high_water {
            self.activation_high_water = action.event_id;
        }
        let ack_transaction = self.transaction()?;
        let activation_transaction = self.transaction()?;
        let activation = ShellIndicatorActivation {
            connection_epoch: self.connection.connection_epoch(),
            snapshot_generation: action.target_generation,
            output: sophia_protocol::OutputId::from_raw(action.output.id),
            indicator: action.target_id,
            action: action.action_id,
            event_id: action.event_id,
        };
        self.connection
            .enqueue_indicator_action_response(
                ack_transaction,
                &ack,
                accepted.then_some((activation_transaction, &activation)),
            )
            .map_err(|error| format!("action response admission: {error}"))?;
        if !accepted {
            return Ok(());
        }
        self.pending_activation_events.insert(action.event_id);
        Ok(())
    }

    fn wait_record(&mut self) -> Result<ShellContentRecord, String> {
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        loop {
            self.observe()?;
            if let Some((_, record)) = self.pending_content.pop_front() {
                return Ok(record);
            }
            if Instant::now() >= deadline {
                return Err("shell content response timed out".into());
            }
            std::thread::sleep(IDLE_POLL);
        }
    }
}

fn pixel_checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn receive_content(
    connection: &mut ShellConnection,
) -> Result<(TransactionId, ShellContentRecord), String> {
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    loop {
        match connection.poll_content() {
            Ok(Some(record)) => return Ok(record),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(IDLE_POLL),
            Ok(None) => return Err("shell content response timed out".into()),
            Err(error) => return Err(format!("shell content receive failed: {error}")),
        }
    }
}

fn workspace_snapshot(snapshot: &ShellIndicatorSnapshot) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        epoch: snapshot.connection_epoch,
        generation: snapshot.generation,
        active_output: snapshot.active_output.map(|output| output.raw()),
        entries: snapshot
            .indicators
            .iter()
            .map(|entry| Workspace {
                id: entry.indicator,
                output: entry.output.raw(),
                name: entry.label.clone(),
                active: entry.state_bits & POLICY_INDICATOR_STATE_ACTIVE != 0,
                visible: entry.state_bits & POLICY_INDICATOR_STATE_VISIBLE_ELSEWHERE != 0,
                urgent: entry.state_bits & POLICY_INDICATOR_STATE_URGENT != 0,
                action: (entry.action != 0).then_some(entry.action),
            })
            .collect(),
    }
}

fn observed_time() -> DateTime<FixedOffset> {
    DateTime::<Utc>::from(SystemTime::now()).fixed_offset()
}

fn unix_second() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

fn edge(position: Position) -> u16 {
    match position {
        Position::Top => 1,
        Position::Right => 2,
        Position::Bottom => 3,
        Position::Left => 4,
    }
}

fn margins(values: [i32; 4]) -> Result<ContentMargins, String> {
    Ok(ContentMargins {
        top: i16::try_from(values[0]).map_err(|_| "top margin exceeds protocol range")?,
        right: i16::try_from(values[1]).map_err(|_| "right margin exceeds protocol range")?,
        bottom: i16::try_from(values[2]).map_err(|_| "bottom margin exceeds protocol range")?,
        left: i16::try_from(values[3]).map_err(|_| "left margin exceeds protocol range")?,
    })
}
