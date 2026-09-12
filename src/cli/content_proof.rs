use std::path::PathBuf;
use std::time::{Duration, Instant};

use sophia_protocol::{
    ContentReason, ContentResourceBegin, ContentResourceChunk, ContentResourceEnd,
    ContentResourceId, ContentResourceRetire, SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE,
    SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER, ShellContentRecord, TransactionId,
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
            rendered_scale_numerator: 1,
            rendered_scale_denominator: 1,
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
        "lom_content_transport schema=1 status=complete revision={} bytes={} resource=1/1 native_presentation=false",
        connection.welcome().selected_revision,
        pixels.bytes().len()
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
