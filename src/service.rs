//! Persistent, display-independent Sophia content lifecycle.

mod presentation;
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
use sophia_shell_client::ShellConnection;
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

/// Produces one complete panel resource from passive application state.
pub trait ContentRenderer {
    /// Queue exactly one render for the acknowledged physical allocation.
    fn submit(&mut self, model: Model, width: u32, height: u32, scale: f64) -> Result<(), String>;

    /// Poll the sole in-flight render without blocking protocol ownership.
    fn poll(&mut self) -> Result<Option<RenderedContent>, String>;
}

struct Panel {
    output: ContentOutputFactsEntry,
    allocation: ContentAllocationResult,
    model: Model,
    current_slot: Option<usize>,
    presented: Option<PresentedPanel>,
    resources: [ResourceSlot; 2],
}

#[derive(Clone, Debug)]
struct PresentedPanel {
    candidate_generation: u64,
    presentation_epoch: u64,
    targets: Vec<ContentTargetLayout>,
}

#[derive(Clone, Copy)]
struct ResourceSlot {
    id: u64,
    generation: u64,
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
    dirty: bool,
    last_second: u64,
    latest_indicators: Option<ShellIndicatorSnapshot>,
    pending_content: VecDeque<ShellContentRecord>,
    activation_high_water: u64,
    pending_activation_events: BTreeSet<u64>,
    presentation: Option<PendingPresentation>,
    rendering_panel: Option<usize>,
    next_panel: Option<usize>,
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
        let ShellContentRecord::Limits(limits) = receive_content(&mut connection)? else {
            return Err("first content record was not ContentLimits".into());
        };
        limits
            .validate()
            .map_err(|error| format!("invalid content limits: {error:?}"))?;
        let ShellContentRecord::OutputFacts(facts) = receive_content(&mut connection)? else {
            return Err("ContentLimits was not followed by ContentOutputFacts".into());
        };
        if facts.outputs.is_empty() {
            return Err("content service published no outputs".into());
        }
        let mut service = Self {
            connection,
            config,
            theme,
            renderer,
            limits,
            facts,
            panels: Vec::new(),
            next_transaction: 1,
            next_demand: 1,
            next_candidate_generation: 1,
            dirty: true,
            last_second: unix_second(),
            latest_indicators: None,
            pending_content: VecDeque::new(),
            activation_high_water: 0,
            pending_activation_events: BTreeSet::new(),
            presentation: None,
            rendering_panel: None,
            next_panel: None,
        };
        service.allocate_panels(allowance)?;
        Ok(service)
    }

    /// Make one bounded observation turn and, when dirty, present one complete
    /// replacement on every output. Returns the number of outputs presented.
    pub fn step(&mut self) -> Result<usize, String> {
        self.observe()?;
        let second = unix_second();
        if second != self.last_second {
            self.last_second = second;
            let now = observed_time();
            let mut observed_clock = false;
            for panel in &mut self.panels {
                if panel
                    .model
                    .config
                    .modules
                    .iter()
                    .any(|module| module.kind == ModuleKind::Clock)
                {
                    panel.model.time = now;
                    observed_clock = true;
                }
            }
            self.dirty |= observed_clock;
        }
        if self.presentation.is_some() {
            return self.advance_presentation().map(usize::from);
        }
        self.service_pending_actions()?;
        if let Some(index) = self.rendering_panel {
            let Some(content) = self.renderer.poll()? else {
                return Ok(0);
            };
            self.rendering_panel = None;
            self.begin_presentation(index, content)?;
            return Ok(0);
        }
        if self.next_panel.is_none() && self.dirty {
            self.dirty = false;
            self.next_panel = Some(0);
        }
        let Some(index) = self.next_panel else {
            return Ok(0);
        };
        if index == self.panels.len() {
            self.next_panel = None;
            return Ok(0);
        }
        let panel = &self.panels[index];
        let scale = f64::from(panel.allocation.scale_numerator)
            / f64::from(panel.allocation.scale_denominator);
        self.renderer.submit(
            panel.model.clone(),
            panel.allocation.pixel.width,
            panel.allocation.pixel.height,
            scale,
        )?;
        self.rendering_panel = Some(index);
        self.next_panel = Some(index + 1);
        Ok(0)
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
            .send_content(transaction, &record)
            .map_err(|error| format!("content send failed: {error}"))
    }

    fn allocate_panels(&mut self, allowance: u32) -> Result<(), String> {
        let outputs = self.facts.outputs.clone();
        let output_count =
            u64::try_from(outputs.len()).map_err(|_| "output count exceeds resource identity")?;
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
            let primary = u64::try_from(index + 1).map_err(|_| "output index overflow")?;
            let alternate = output_count
                .checked_add(primary)
                .ok_or("resource identity exhausted")?;
            // New resource IDs are admitted in panel iteration order. Keep each
            // panel's alternate slot after every panel's primary slot so first
            // use is globally monotonic under the grant's resource high-water.
            self.panels.push(Panel {
                output,
                allocation,
                model,
                current_slot: None,
                presented: None,
                resources: [
                    ResourceSlot {
                        id: primary,
                        generation: 1,
                    },
                    ResourceSlot {
                        id: alternate,
                        generation: 1,
                    },
                ],
            });
        }
        Ok(())
    }

    fn observe(&mut self) -> Result<(), String> {
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((_, snapshot)) = self
                .connection
                .poll_indicators()
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
                panel.model.workspaces = workspaces.clone();
                panel.model.epoch = workspaces.epoch;
            }
            self.dirty = true;
        }
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some((_, outcome)) = self
                .connection
                .poll_indicator_activation_outcome()
                .map_err(|error| format!("indicator outcome receive failed: {error}"))?
            else {
                break;
            };
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
            let Some((_, record)) = self
                .connection
                .poll_content()
                .map_err(|error| format!("shell content receive failed: {error}"))?
            else {
                break;
            };
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
            self.pending_content.push_back(record);
        }
        Ok(())
    }

    fn service_pending_actions(&mut self) -> Result<(), String> {
        for _ in 0..MAX_OBSERVATIONS_PER_TURN {
            let Some(at) = self
                .pending_content
                .iter()
                .position(|record| matches!(record, ShellContentRecord::Action(_)))
            else {
                break;
            };
            let Some(ShellContentRecord::Action(action)) = self.pending_content.remove(at) else {
                unreachable!("position selected an action");
            };
            self.handle_content_action(action)?;
        }
        Ok(())
    }

    fn handle_content_action(&mut self, action: ContentAction) -> Result<(), String> {
        let mut message = None;
        if action.kind == 1
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
            crate::update::update(&mut panel.model, message)
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
        self.send(ShellContentRecord::ActionAck(ContentActionAck {
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
        }))?;
        if action.grant == self.limits.grant && action.event_id > self.activation_high_water {
            self.activation_high_water = action.event_id;
        }
        if !accepted {
            return Ok(());
        }
        let transaction = self.transaction()?;
        self.connection
            .send_indicator_activation(
                transaction,
                &ShellIndicatorActivation {
                    connection_epoch: self.connection.connection_epoch(),
                    snapshot_generation: action.target_generation,
                    output: sophia_protocol::OutputId::from_raw(action.output.id),
                    indicator: action.target_id,
                    action: action.action_id,
                    event_id: action.event_id,
                },
            )
            .map_err(|error| format!("indicator activation send failed: {error}"))?;
        self.pending_activation_events.insert(action.event_id);
        Ok(())
    }

    fn wait_record(&mut self) -> Result<ShellContentRecord, String> {
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        loop {
            self.observe()?;
            if let Some(record) = self.pending_content.pop_front() {
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

fn receive_content(connection: &mut ShellConnection) -> Result<ShellContentRecord, String> {
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    loop {
        match connection.poll_content() {
            Ok(Some((_, record))) => return Ok(record),
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
