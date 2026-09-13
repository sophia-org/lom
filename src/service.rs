//! Persistent, display-independent Sophia content lifecycle.

use crate::{
    config::{ModuleKind, PanelConfig, Position, Theme},
    model::{Model, Workspace, WorkspaceSnapshot},
    protocol::ContentPixels,
};
use chrono::{DateTime, FixedOffset, Utc};
use sophia_protocol::{
    ContentAllocationId, ContentAllocationRequest, ContentAllocationResult, ContentCandidateBegin,
    ContentCandidateChunk, ContentCandidateEnd, ContentFrameDemand, ContentFramePermit,
    ContentMargins, ContentOutputFacts, ContentOutputFactsEntry, ContentPixelRect,
    ContentPlacement, ContentReason, ContentResourceBegin, ContentResourceChunk,
    ContentResourceEnd, ContentResourceId, ContentResourceRetire, ContentSurface,
    POLICY_INDICATOR_STATE_ACTIVE, POLICY_INDICATOR_STATE_URGENT,
    POLICY_INDICATOR_STATE_VISIBLE_ELSEWHERE, ShellContentRecord, ShellIndicatorSnapshot,
    TransactionId,
};
use sophia_shell_client::ShellConnection;
use std::{
    collections::VecDeque,
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
const CANDIDATE_REJECTED: u16 = 3;

/// Produces one complete panel resource from passive application state.
pub trait ContentRenderer {
    /// Queue exactly one render for the acknowledged physical allocation.
    fn submit(&mut self, model: Model, width: u32, height: u32, scale: f64) -> Result<(), String>;

    /// Poll the sole in-flight render without blocking protocol ownership.
    fn poll(&mut self) -> Result<Option<ContentPixels>, String>;
}

struct Panel {
    output: ContentOutputFactsEntry,
    allocation: ContentAllocationResult,
    model: Model,
    current_slot: Option<usize>,
    resources: [ResourceSlot; 2],
    candidate_generation: u64,
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
    dirty: bool,
    last_second: u64,
    latest_indicators: Option<ShellIndicatorSnapshot>,
    pending_content: VecDeque<ShellContentRecord>,
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
            dirty: true,
            last_second: unix_second(),
            latest_indicators: None,
            pending_content: VecDeque::new(),
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
        if let Some(index) = self.rendering_panel {
            let Some(pixels) = self.renderer.poll()? else {
                return Ok(0);
            };
            self.rendering_panel = None;
            self.present(index, pixels)?;
            return Ok(1);
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
            let base = u64::try_from(index)
                .map_err(|_| "output index overflow")?
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .ok_or("resource identity exhausted")?;
            self.panels.push(Panel {
                output,
                allocation,
                model,
                current_slot: None,
                resources: [
                    ResourceSlot {
                        id: base,
                        generation: 1,
                    },
                    ResourceSlot {
                        id: base + 1,
                        generation: 1,
                    },
                ],
                candidate_generation: 1,
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

    fn present(&mut self, index: usize, pixels: ContentPixels) -> Result<(), String> {
        let (slot_index, slot, old, candidate) = {
            let panel = &self.panels[index];
            let slot_index = panel.current_slot.map_or(0, |slot| 1 - slot);
            (
                slot_index,
                panel.resources[slot_index],
                panel.current_slot.map(|old| panel.resources[old]),
                panel.candidate_generation,
            )
        };
        let resource = ContentResourceId {
            id: slot.id,
            generation: slot.generation,
        };
        self.upload(resource, &pixels, index)?;
        let demand_id = self.next_demand;
        self.next_demand = demand_id
            .checked_add(1)
            .ok_or("frame demand identity exhausted")?;
        let output = self.panels[index].output.output;
        let allocation = self.panels[index].allocation.allocation;
        self.send(ShellContentRecord::FrameDemand(ContentFrameDemand {
            grant: self.limits.grant,
            output,
            allocation,
            demand_id,
            reason: 1,
        }))?;
        let permit = self.wait_permit(output, demand_id)?;
        self.submit_candidate(index, resource, candidate, permit)?;
        self.wait_presented(output, candidate)?;
        if let Some(old) = old {
            let old = ContentResourceId {
                id: old.id,
                generation: old.generation,
            };
            self.send(ShellContentRecord::ResourceRetire(ContentResourceRetire {
                grant: self.limits.grant,
                resource: old,
            }))?;
            self.wait_released(old)?;
            let old_index = 1 - slot_index;
            self.panels[index].resources[old_index].generation = self.panels[index].resources
                [old_index]
                .generation
                .checked_add(1)
                .ok_or("resource generation exhausted")?;
        }
        let panel = &mut self.panels[index];
        panel.current_slot = Some(slot_index);
        panel.candidate_generation = candidate
            .checked_add(1)
            .ok_or("candidate generation exhausted")?;
        Ok(())
    }

    fn upload(
        &mut self,
        resource: ContentResourceId,
        pixels: &ContentPixels,
        panel: usize,
    ) -> Result<(), String> {
        let chunk_count = u32::try_from(
            pixels
                .chunks(self.limits.max_frame_payload, self.limits.max_chunk_bytes)?
                .count(),
        )
        .map_err(|_| "resource chunk count exceeds the protocol")?;
        let transaction = self.transaction()?;
        self.connection
            .send_content(
                transaction,
                &ShellContentRecord::ResourceBegin(ContentResourceBegin {
                    grant: self.limits.grant,
                    resource,
                    width_px: pixels.width(),
                    height_px: pixels.height(),
                    rendered_scale_numerator: self.panels[panel].allocation.scale_numerator,
                    rendered_scale_denominator: self.panels[panel].allocation.scale_denominator,
                    pixel_format: 1,
                    chunk_count,
                    total_bytes: pixels.bytes().len() as u64,
                }),
            )
            .map_err(|error| format!("resource begin failed: {error}"))?;
        self.wait_resource_status(resource, 1)?;
        for chunk in pixels.chunks(self.limits.max_frame_payload, self.limits.max_chunk_bytes)? {
            self.connection
                .send_content(
                    transaction,
                    &ShellContentRecord::ResourceChunk(ContentResourceChunk {
                        grant: self.limits.grant,
                        resource,
                        ordinal: chunk.ordinal,
                        offset: chunk.offset,
                        bytes: chunk.bytes.to_vec(),
                    }),
                )
                .map_err(|error| format!("resource chunk failed: {error}"))?;
        }
        self.connection
            .send_content(
                transaction,
                &ShellContentRecord::ResourceEnd(ContentResourceEnd {
                    grant: self.limits.grant,
                    resource,
                    total_bytes: pixels.bytes().len() as u64,
                    chunk_count,
                }),
            )
            .map_err(|error| format!("resource end failed: {error}"))?;
        self.wait_resource_status(resource, RESOURCE_ACCEPTED)
    }

    fn wait_resource_status(
        &mut self,
        resource: ContentResourceId,
        expected: u16,
    ) -> Result<(), String> {
        loop {
            if let ShellContentRecord::ResourceStatus(status) = self.wait_record()?
                && status.resource == resource
            {
                if status.status == expected && status.reason == ContentReason::None as u16 {
                    return Ok(());
                }
                if status.reason != ContentReason::None as u16 || status.status >= 3 {
                    return Err(format!("resource was rejected: {}", status.reason));
                }
            }
        }
    }

    fn wait_permit(
        &mut self,
        output: sophia_protocol::ContentOutputId,
        demand_id: u64,
    ) -> Result<ContentFramePermit, String> {
        loop {
            if let ShellContentRecord::FramePermit(permit) = self.wait_record()?
                && permit.output == output
                && permit.demand_id == demand_id
            {
                if permit.state == PERMIT_GRANTED && permit.reason == ContentReason::None as u16 {
                    return Ok(permit);
                }
                return Err(format!("frame demand was refused: {}", permit.reason));
            }
        }
    }

    fn submit_candidate(
        &mut self,
        panel_index: usize,
        resource: ContentResourceId,
        candidate: u64,
        permit: ContentFramePermit,
    ) -> Result<(), String> {
        let panel = &self.panels[panel_index];
        let reservation_extent = match self.config.position {
            Position::Top | Position::Bottom => panel.allocation.pixel.height,
            Position::Left | Position::Right => panel.allocation.pixel.width,
        };
        let surface = ContentSurface {
            allocation: panel.allocation.allocation,
            scale_generation: panel.allocation.scale_generation,
            role: PANEL_ROLE,
            edge: edge(self.config.position),
            margins: panel.allocation.margins,
            reservation_extent,
            parent_surface_index: u16::MAX,
            anchor_parent_rect: ContentPixelRect::default(),
        };
        let begin = ShellContentRecord::CandidateBegin(ContentCandidateBegin {
            grant: self.limits.grant,
            candidate_generation: candidate,
            output: panel.output.output,
            facts_generation: self.facts.facts_generation,
            pacing_permit: permit.permit_id,
            interaction_generation: candidate,
            surface_count: 1,
            placement_count: 1,
            target_count: 0,
        });
        let chunk = ShellContentRecord::CandidateChunk(ContentCandidateChunk {
            grant: self.limits.grant,
            candidate_generation: candidate,
            chunk_ordinal: 0,
            surfaces: vec![surface],
            placements: vec![ContentPlacement {
                resource,
                surface_index: 0,
                destination_x_px: 0,
                destination_y_px: 0,
            }],
            targets: Vec::new(),
        });
        let end = ShellContentRecord::CandidateEnd(ContentCandidateEnd {
            grant: self.limits.grant,
            candidate_generation: candidate,
            surface_count: 1,
            placement_count: 1,
            target_count: 0,
        });
        for record in [begin, chunk, end] {
            self.send(record)?;
        }
        Ok(())
    }

    fn wait_presented(
        &mut self,
        output: sophia_protocol::ContentOutputId,
        candidate: u64,
    ) -> Result<(), String> {
        loop {
            if let ShellContentRecord::CandidateOutcome(outcome) = self.wait_record()?
                && outcome.output == output
                && outcome.candidate_generation == candidate
            {
                if outcome.kind == CANDIDATE_PRESENTED
                    && outcome.reason == ContentReason::None as u16
                    && outcome.presentation_epoch != 0
                {
                    return Ok(());
                }
                if outcome.kind == CANDIDATE_REJECTED {
                    return Err(format!(
                        "content candidate was rejected: {}",
                        outcome.reason
                    ));
                }
            }
        }
    }

    fn wait_released(&mut self, resource: ContentResourceId) -> Result<(), String> {
        loop {
            if let ShellContentRecord::ResourceReleased(released) = self.wait_record()?
                && released.resource == resource
            {
                return if released.reason == ContentReason::None as u16 {
                    Ok(())
                } else {
                    Err(format!("resource release failed: {}", released.reason))
                };
            }
        }
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
