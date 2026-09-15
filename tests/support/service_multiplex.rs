use super::*;
use std::collections::{BTreeMap, BTreeSet};

// A private wire peer, not a native renderer. Interleave independent output
// lifecycles so a sequential fake server cannot hide client serialization.
pub(super) fn exchange(stream: &mut UnixStream) {
    let mut resources = BTreeMap::new();
    let mut first_use = 0;
    let mut candidate = None;
    let mut generations = 0;
    let mut outputs = BTreeSet::new();
    let mut retired = Vec::new();
    loop {
        let (transaction, record) = receive(stream);
        match record {
            ShellContentRecord::ResourceBegin(begin) => {
                assert_eq!(begin.resource.id, first_use + 1);
                first_use += 1;
                assert_eq!(begin.resource.generation, 1);
                assert_eq!(begin.total_bytes, 1536);
                resources.insert(begin.resource.id, (transaction, begin.clone(), 0usize));
                send_tx(
                    stream,
                    transaction,
                    ShellContentRecord::ResourceStatus(ContentResourceStatus {
                        grant: GRANT,
                        resource: begin.resource,
                        status: 1,
                        reason: 0,
                        next_ordinal: 0,
                        admitted_bytes: begin.total_bytes,
                    }),
                );
            }
            ShellContentRecord::ResourceChunk(chunk) => {
                let (_, begin, bytes) = resources.get_mut(&chunk.resource.id).unwrap();
                assert_eq!(chunk.resource, begin.resource);
                assert_eq!(chunk.offset as usize, *bytes);
                *bytes += chunk.bytes.len();
            }
            ShellContentRecord::ResourceEnd(end) => {
                let (origin, begin, bytes) = &resources[&end.resource.id];
                assert_eq!(*bytes as u64, begin.total_bytes);
                send_tx(
                    stream,
                    *origin,
                    ShellContentRecord::ResourceStatus(ContentResourceStatus {
                        grant: GRANT,
                        resource: end.resource,
                        status: 2,
                        reason: 0,
                        next_ordinal: end.chunk_count,
                        admitted_bytes: end.total_bytes,
                    }),
                );
            }
            ShellContentRecord::FrameDemand(demand) => {
                send_tx(
                    stream,
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
                );
            }
            ShellContentRecord::CandidateBegin(begin) => {
                assert!(candidate.is_none());
                assert_eq!(begin.candidate_generation, generations + 1);
                candidate = Some((transaction, begin));
            }
            ShellContentRecord::CandidateChunk(chunk) => {
                let (_, begin) = candidate.as_ref().unwrap();
                assert_eq!(chunk.candidate_generation, begin.candidate_generation);
                assert_eq!(chunk.surfaces[0].reservation_extent, 48);
                assert_eq!(chunk.targets.len(), 0);
                assert!(resources.contains_key(&chunk.placements[0].resource.id));
            }
            ShellContentRecord::CandidateEnd(end) => {
                let (origin, begin) = candidate.take().unwrap();
                assert_eq!(end.candidate_generation, begin.candidate_generation);
                generations += 1;
                outputs.insert(begin.output.id);
                for kind in [1, 2] {
                    send_tx(
                        stream,
                        origin,
                        ShellContentRecord::CandidateOutcome(
                            sophia_protocol::ContentCandidateOutcome {
                                grant: GRANT,
                                candidate_generation: generations,
                                output: begin.output,
                                kind,
                                reason: 0,
                                presentation_epoch: if kind == 2 { generations + 30 } else { 0 },
                                work_area_generation: 8 + generations,
                                wm_commit_generation: 9 + generations,
                            },
                        ),
                    );
                }
                if generations == 2 {
                    assert_eq!(outputs.len(), 2);
                    outputs.clear();
                    for frame in encode_shell_indicator_snapshot(
                        TransactionId::from_raw(70),
                        &ShellIndicatorSnapshot {
                            connection_epoch: GRANT.connection_epoch,
                            generation: 1,
                            active_output: None,
                            statuses: Vec::new(),
                            indicators: Vec::new(),
                        },
                    )
                    .unwrap()
                    {
                        stream.write_all(&frame).unwrap();
                    }
                }
                if generations == 4 {
                    assert_eq!(outputs.len(), 2);
                }
            }
            ShellContentRecord::ResourceRetire(retire) => {
                retired.push((transaction, retire));
            }
            other => panic!("unexpected multiplexed record: {other:?}"),
        }
        // Withhold every old release until BOTH outputs have replaced their
        // frame. The former global retirement wait deadlocks this scenario.
        if generations == 4 && retired.len() == 2 {
            for (transaction, retire) in retired {
                send_tx(
                    stream,
                    transaction,
                    ShellContentRecord::ResourceReleased(
                        sophia_protocol::ContentResourceReleased {
                            grant: GRANT,
                            resource: retire.resource,
                            reason: 0,
                        },
                    ),
                );
            }
            break;
        }
    }
}
