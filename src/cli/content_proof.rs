use std::path::PathBuf;
use std::time::{Duration, Instant};

use sophia_protocol::{
    ContentAllocationId, ContentAllocationRequest, ContentCandidateBegin, ContentCandidateChunk,
    ContentCandidateEnd, ContentFrameDemand, ContentMargins, ContentPixelRect, ContentPlacement,
    ContentReason, ContentResourceBegin, ContentResourceChunk, ContentResourceEnd,
    ContentResourceId, ContentResourceRetire, ContentSurface, ContentTarget,
    SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE, SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER,
    ShellContentRecord, TransactionId,
};
use sophia_shell_client::{ShellClientOptions, ShellConnection};

use crate::protocol::ContentPixels;

pub(super) fn run(socket: PathBuf) -> Result<(), String> {
    let capabilities =
        SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE;
    let mut connection = ShellConnection::connect(
        socket,
        ShellClientOptions {
            minimum_revision: 5,
            maximum_revision: 6,
            required_capabilities: capabilities,
            handshake_timeout: Duration::from_secs(5),
        },
    )
    .map_err(|error| format!("content negotiation failed: {error}"))?;
    let ShellContentRecord::Limits(limits) = receive(&mut connection)? else {
        return Err("first content record was not ContentLimits".into());
    };
    let ShellContentRecord::OutputFacts(facts) = receive(&mut connection)? else {
        return Err("ContentLimits was not followed by output facts".into());
    };
    let [output] = facts.outputs.as_slice() else {
        return Err("conformance host did not publish one exact output".into());
    };
    let output = output.clone();
    send(
        &mut connection,
        TransactionId::from_raw(3),
        ShellContentRecord::AllocationRequest(ContentAllocationRequest {
            grant: limits.grant,
            output: output.output,
            allocation_request_id: 1,
            operation: 1,
            role: 1,
            edge: 1,
            prior: ContentAllocationId::default(),
            parent: ContentAllocationId::default(),
            parent_presentation_epoch: 0,
            anchor_parent_rect: ContentPixelRect::default(),
            desired_width: output.local_width,
            desired_height: 32,
            margins: ContentMargins::default(),
        }),
    )?;
    let ShellContentRecord::AllocationResult(allocation) = receive(&mut connection)? else {
        return Err("output facts were not followed by an allocation result".into());
    };
    if allocation.status != 1
        || allocation.reason != ContentReason::None as u16
        || allocation.output != output.output
        || allocation.logical.width != output.local_width
        || allocation.logical.height != 32
    {
        return Err("panel allocation was not granted with the requested dimensions".into());
    }
    let pixels = ContentPixels::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128])?;
    let chunks = pixels
        .chunks(limits.max_frame_payload, limits.max_chunk_bytes)?
        .collect::<Vec<_>>();
    let chunk_count = chunks.len() as u32;
    let resource = ContentResourceId {
        id: 1,
        generation: 1,
    };
    let upload = TransactionId::from_raw(1);
    send(
        &mut connection,
        upload,
        ShellContentRecord::ResourceBegin(ContentResourceBegin {
            grant: limits.grant,
            resource,
            width_px: pixels.width(),
            height_px: pixels.height(),
            rendered_scale_numerator: allocation.scale_numerator,
            rendered_scale_denominator: allocation.scale_denominator,
            pixel_format: 1,
            chunk_count,
            total_bytes: pixels.bytes().len() as u64,
        }),
    )?;
    for chunk in chunks {
        send(
            &mut connection,
            upload,
            ShellContentRecord::ResourceChunk(ContentResourceChunk {
                grant: limits.grant,
                resource,
                ordinal: chunk.ordinal,
                offset: chunk.offset,
                bytes: chunk.bytes.to_vec(),
            }),
        )?;
    }
    send(
        &mut connection,
        upload,
        ShellContentRecord::ResourceEnd(ContentResourceEnd {
            grant: limits.grant,
            resource,
            total_bytes: pixels.bytes().len() as u64,
            chunk_count,
        }),
    )?;
    let statuses = [receive(&mut connection)?, receive(&mut connection)?];
    if !matches!(&statuses[0], ShellContentRecord::ResourceStatus(status) if status.status == 1)
        || !matches!(&statuses[1], ShellContentRecord::ResourceStatus(status) if status.status == 2)
    {
        return Err("resource did not pass admitted then accepted states".into());
    }
    send(
        &mut connection,
        TransactionId::from_raw(9),
        ShellContentRecord::FrameDemand(ContentFrameDemand {
            grant: limits.grant,
            output: output.output,
            allocation: ContentAllocationId::default(),
            demand_id: 1,
            reason: 1,
        }),
    )?;
    let ShellContentRecord::FramePermit(permit) = receive(&mut connection)? else {
        return Err("accepted resource was not followed by a frame permit".into());
    };
    if permit.state != 1 || permit.permit_id == 0 {
        return Err("frame permit was not a fresh grant".into());
    }
    for (transaction, record) in [
        (
            10,
            ShellContentRecord::CandidateBegin(ContentCandidateBegin {
                grant: limits.grant,
                candidate_generation: 1,
                output: permit.output,
                facts_generation: facts.facts_generation,
                pacing_permit: permit.permit_id,
                interaction_generation: 1,
                surface_count: 1,
                placement_count: 1,
                target_count: 1,
            }),
        ),
        (
            11,
            ShellContentRecord::CandidateChunk(ContentCandidateChunk {
                grant: limits.grant,
                candidate_generation: 1,
                chunk_ordinal: 0,
                surfaces: vec![ContentSurface {
                    allocation: allocation.allocation,
                    scale_generation: allocation.scale_generation,
                    role: 1,
                    edge: 1,
                    margins: allocation.margins,
                    reservation_extent: allocation.allowed_reservation_extent.min(24),
                    parent_surface_index: u16::MAX,
                    anchor_parent_rect: ContentPixelRect::default(),
                }],
                placements: vec![ContentPlacement {
                    resource,
                    surface_index: 0,
                    destination_x_px: 3,
                    destination_y_px: 4,
                }],
                targets: vec![ContentTarget {
                    surface_index: 0,
                    action_kind: 1,
                    target_id: 1,
                    target_generation: 1,
                    action_id: 1,
                    bounds_px: ContentPixelRect {
                        x: 3,
                        y: 4,
                        width: 2,
                        height: 1,
                    },
                }],
            }),
        ),
        (
            12,
            ShellContentRecord::CandidateEnd(ContentCandidateEnd {
                grant: limits.grant,
                candidate_generation: 1,
                surface_count: 1,
                placement_count: 1,
                target_count: 1,
            }),
        ),
    ] {
        send(
            &mut connection,
            TransactionId::from_raw(transaction),
            record,
        )?;
    }
    let ShellContentRecord::CandidateOutcome(outcome) = receive(&mut connection)? else {
        return Err("candidate did not receive a terminal outcome".into());
    };
    if outcome.candidate_generation != 1
        || outcome.output != permit.output
        || outcome.kind != 3
        || outcome.reason != ContentReason::RendererFailed as u16
        || outcome.presentation_epoch != 0
    {
        return Err("headless candidate did not take the exact renderer-failure outcome".into());
    }
    send(
        &mut connection,
        TransactionId::from_raw(2),
        ShellContentRecord::ResourceRetire(ContentResourceRetire {
            grant: limits.grant,
            resource,
        }),
    )?;
    let ShellContentRecord::ResourceReleased(released) = receive(&mut connection)? else {
        return Err("accepted resource did not receive release".into());
    };
    if released.resource != resource || released.reason != ContentReason::None as u16 {
        return Err("resource release identity or reason changed".into());
    }
    println!(
        "lom_content_transport schema=1 status=complete revision={} allocation=granted bytes={} resource=1/1 candidate=accepted renderer_outcome={} native_presentation=false",
        connection.welcome().selected_revision,
        pixels.bytes().len(),
        ContentReason::RendererFailed as u16,
    );
    Ok(())
}

fn send(
    connection: &mut ShellConnection,
    transaction: TransactionId,
    record: ShellContentRecord,
) -> Result<(), String> {
    connection
        .send_content(transaction, &record)
        .map_err(|error| format!("content send failed: {error}"))
}

fn receive(connection: &mut ShellConnection) -> Result<ShellContentRecord, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match connection.poll_content() {
            Ok(Some((_, record))) => return Ok(record),
            Ok(None) if Instant::now() < deadline => std::thread::yield_now(),
            Ok(None) => return Err("content response timed out".into()),
            Err(error) => return Err(format!("content receive failed: {error}")),
        }
    }
}
