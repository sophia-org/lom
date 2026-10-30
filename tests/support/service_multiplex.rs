use super::*;
use std::collections::{BTreeMap, BTreeSet};

// A private wire peer, not a native renderer. Interleave independent output
// lifecycles so a sequential fake server cannot hide client serialization.
pub(super) fn exchange(stream: &mut Peer) {
    let mut resources = BTreeMap::new();
    let mut first_use = 0;
    let mut candidate = None;
    let mut generations = 0;
    let mut outputs = BTreeSet::new();
    let mut retired = Vec::new();
    loop {
        // Sealed images arrive as peer-verified offers, interleaved
        // with the content records below.
        let (transaction, record) = loop {
            match stream.receive() {
                Message::Image(metadata, _) => {
                    assert_eq!(metadata.resource, first_use + 1);
                    first_use += 1;
                    assert_eq!(metadata.resource_generation, 1);
                    assert_eq!(metadata.declared_bytes, 1536);
                    resources.insert(metadata.resource, metadata);
                }
                Message::Content(transaction, record) => break (transaction, *record),
                Message::Activation(..) => panic!("unexpected multiplexed activation"),
            }
        };
        match record {
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
                            sophia_shell_protocol::ContentCandidateOutcome {
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
                    stream.indicators(
                        TransactionId::from_raw(70),
                        ShellIndicatorSnapshot {
                            connection_epoch: GRANT.connection_epoch,
                            generation: 1,
                            active_output: None,
                            statuses: Vec::new(),
                            indicators: Vec::new(),
                        },
                    );
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
                        sophia_shell_protocol::ContentResourceReleased {
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
