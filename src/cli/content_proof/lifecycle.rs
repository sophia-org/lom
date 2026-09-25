//! Diagnostic wire lifecycle; presentation is accepted only from host receipts.
use super::*;
use sophia_protocol::{
    ContentActionAck, ContentAllocationResult, ContentLimits, ContentOutputFacts,
};

pub(super) fn finish(
    connection: &mut ShellConnection,
    limits: &ContentLimits,
    facts: &ContentOutputFacts,
    parent: &ContentAllocationResult,
    resource: ContentResourceId,
) -> Result<(), String> {
    let epoch = presented(connection, parent, 1)?;
    let mut request = ContentAllocationRequest {
        grant: limits.grant,
        output: parent.output,
        allocation_request_id: 2,
        operation: 1,
        role: 2,
        edge: 1,
        prior: ContentAllocationId::default(),
        parent: parent.allocation,
        parent_presentation_epoch: epoch,
        anchor_parent_rect: ContentPixelRect {
            x: 3,
            y: 4,
            width: 2,
            height: 1,
        },
        desired_width: 16,
        desired_height: 8,
        margins: ContentMargins::default(),
    };
    request.parent_presentation_epoch = epoch + 1;
    send(
        connection,
        TransactionId::from_raw(21),
        ShellContentRecord::AllocationRequest(request.clone()),
    )?;
    let ShellContentRecord::AllocationResult(stale) = receive(connection)? else {
        return Err("stale-parent refusal missing".into());
    };
    if stale.grant != limits.grant
        || stale.allocation_request_id != 2
        || stale.status != 2
        || stale.reason != ContentReason::Stale as u16
    {
        return Err("stale parent receipt was not refused".into());
    }
    request.allocation_request_id = 3;
    request.parent_presentation_epoch = epoch;
    send(
        connection,
        TransactionId::from_raw(22),
        ShellContentRecord::AllocationRequest(request),
    )?;
    let ShellContentRecord::AllocationResult(popup) = receive(connection)? else {
        return Err("popout allocation missing".into());
    };
    if popup.grant != limits.grant
        || popup.output != parent.output
        || popup.parent != parent.allocation
        || popup.status != 1
        || popup.reason != 0
        || popup.allocation_request_id != 3
        || popup.pixel
            != (ContentPixelRect {
                x: 3,
                y: 5,
                width: 16,
                height: 8,
            })
    {
        return Err("popout changed exact parent/physical placement".into());
    }
    candidate(connection, limits, facts, parent, Some(&popup), resource, 2)?;
    let epoch = presented(connection, parent, 2)?;
    action(connection, &popup, 2, epoch, 1)?;
    candidate(connection, limits, facts, parent, Some(&popup), resource, 3)?;
    let epoch = presented(connection, parent, 3)?;
    action(connection, &popup, 3, epoch, 2)?;
    // ACK is receipt, never evidence that the peer has withdrawn its pixels.
    invalidated(connection, &popup)?;
    candidate(connection, limits, facts, parent, None, resource, 4)?;
    presented(connection, parent, 4)?;
    invalidated(connection, parent)?;
    println!(
        "lom_content_lifecycle candidates=4 action=1 outside_dismissal=1 parent_loss=1 native_acceptance=false"
    );
    Ok(())
}

fn presented(
    connection: &mut ShellConnection,
    parent: &ContentAllocationResult,
    generation: u64,
) -> Result<u64, String> {
    let mut epoch = 0;
    for kind in [1, 2] {
        let ShellContentRecord::CandidateOutcome(outcome) = receive(connection)? else {
            return Err("candidate outcome missing".into());
        };
        if outcome.grant != parent.grant
            || outcome.output != parent.output
            || outcome.candidate_generation != generation
            || outcome.kind != kind
            || outcome.reason != 0
            || (kind == 2) != (outcome.presentation_epoch != 0)
        {
            return Err("candidate outcome identity/phase changed".into());
        }
        epoch = outcome.presentation_epoch;
    }
    Ok(epoch)
}

fn invalidated(
    connection: &mut ShellConnection,
    allocation: &ContentAllocationResult,
) -> Result<(), String> {
    let ShellContentRecord::AllocationResult(result) = receive(connection)? else {
        return Err("allocation loss missing".into());
    };
    if result.grant != allocation.grant
        || result.output != allocation.output
        || result.allocation != allocation.allocation
        || result.status != 4
        || result.reason != ContentReason::AllocationLost as u16
    {
        return Err("allocation loss identity changed".into());
    }
    Ok(())
}

fn action(
    connection: &mut ShellConnection,
    popup: &ContentAllocationResult,
    generation: u64,
    epoch: u64,
    kind: u16,
) -> Result<(), String> {
    let ShellContentRecord::Action(action) = receive(connection)? else {
        return Err("action missing".into());
    };
    let target = if kind == 2 {
        (0, 0, 0)
    } else {
        (2, generation, 2)
    };
    if action.grant != popup.grant
        || action.output != popup.output
        || action.allocation != popup.allocation
        || action.candidate_generation != generation
        || action.presentation_epoch != epoch
        || action.interaction_generation != generation
        || action.kind != kind
        || action.reason != 0
        || action.event_id != generation - 1
        || (action.target_id, action.target_generation, action.action_id) != target
    {
        return Err("action identity changed".into());
    }
    send(
        connection,
        TransactionId::from_raw(20),
        ShellContentRecord::ActionAck(ContentActionAck {
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
            disposition: 1,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn candidate(
    connection: &mut ShellConnection,
    limits: &ContentLimits,
    facts: &ContentOutputFacts,
    parent: &ContentAllocationResult,
    popup: Option<&ContentAllocationResult>,
    resource: ContentResourceId,
    generation: u64,
) -> Result<(), String> {
    send(
        connection,
        TransactionId::from_raw(9),
        ShellContentRecord::FrameDemand(ContentFrameDemand {
            grant: limits.grant,
            output: parent.output,
            allocation: ContentAllocationId::default(),
            demand_id: generation,
            reason: 1,
        }),
    )?;
    let ShellContentRecord::FramePermit(permit) = receive(connection)? else {
        return Err("permit missing".into());
    };
    if permit.grant != limits.grant
        || permit.output != parent.output
        || permit.demand_id != generation
        || permit.state != 1
        || permit.permit_id == 0
    {
        return Err("permit identity changed".into());
    }
    let rows: Vec<_> = std::iter::once(parent).chain(popup).collect();
    let count = rows.len() as u32;
    send(
        connection,
        TransactionId::from_raw(10),
        ShellContentRecord::CandidateBegin(ContentCandidateBegin {
            grant: limits.grant,
            candidate_generation: generation,
            output: parent.output,
            facts_generation: facts.facts_generation,
            pacing_permit: permit.permit_id,
            interaction_generation: generation,
            surface_count: count,
            placement_count: count,
            target_count: count,
        }),
    )?;
    let mut surfaces = Vec::new();
    let mut placements = Vec::new();
    let mut targets = Vec::new();
    for (index, allocation) in rows.into_iter().enumerate() {
        let is_popup = index == 1;
        surfaces.push(ContentSurface {
            allocation: allocation.allocation,
            scale_generation: allocation.scale_generation,
            role: if is_popup { 2 } else { 1 },
            edge: 1,
            margins: allocation.margins,
            reservation_extent: if is_popup {
                0
            } else {
                allocation.allowed_reservation_extent.min(24)
            },
            parent_surface_index: if is_popup { 0 } else { u16::MAX },
            anchor_parent_rect: if is_popup {
                ContentPixelRect {
                    x: 3,
                    y: 4,
                    width: 2,
                    height: 1,
                }
            } else {
                ContentPixelRect::default()
            },
        });
        placements.push(ContentPlacement {
            resource,
            surface_index: index as u16,
            destination_x_px: if is_popup { 0 } else { 3 },
            destination_y_px: if is_popup { 0 } else { 4 },
        });
        targets.push(ContentTarget {
            surface_index: index as u16,
            action_kind: 1,
            target_id: index as u64 + 1,
            target_generation: generation,
            action_id: index as u64 + 1,
            bounds_px: ContentPixelRect {
                x: if is_popup { 0 } else { 3 },
                y: if is_popup { 0 } else { 4 },
                width: 2,
                height: 1,
            },
        });
    }
    send(
        connection,
        TransactionId::from_raw(11),
        ShellContentRecord::CandidateChunk(ContentCandidateChunk {
            grant: limits.grant,
            candidate_generation: generation,
            chunk_ordinal: 0,
            surfaces,
            placements,
            targets,
        }),
    )?;
    send(
        connection,
        TransactionId::from_raw(12),
        ShellContentRecord::CandidateEnd(ContentCandidateEnd {
            grant: limits.grant,
            candidate_generation: generation,
            surface_count: count,
            placement_count: count,
            target_count: count,
        }),
    )
}
