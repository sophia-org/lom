//! Real FIFO/resource ownership with supplied rendering and native outcomes.
use super::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

struct CountedRenderer {
    calls: Arc<Mutex<Vec<u64>>>,
    ready: Option<RenderedContent>,
}
impl ContentRenderer for CountedRenderer {
    fn submit(
        &mut self,
        identity: lom::service::RenderIdentity,
        model: Model,
        width: u32,
        height: u32,
        _: f64,
    ) -> Result<(), String> {
        self.calls.lock().unwrap().push(identity.output.id);
        let entry = model
            .workspaces
            .entries
            .iter()
            .find(|entry| entry.output == model.output)
            .unwrap();
        self.ready = Some(RenderedContent {
            pixels: ContentPixels::from_rgba8(
                width,
                height,
                vec![if entry.active { 255 } else { 128 }; (width * height * 4) as usize],
            )?,
            targets: vec![ContentTargetLayout {
                message: Msg::ActivateWorkspace {
                    owner: model.module_id(0),
                    epoch: model.epoch,
                    generation: model.workspaces.generation,
                    indicator: entry.id,
                    action: entry.action.unwrap(),
                },
                indicator: entry.id,
                action: entry.action.unwrap(),
                generation: model.workspaces.generation,
                x: 0,
                y: 0,
                width,
                height,
            }],
        });
        Ok(())
    }
    fn poll(&mut self) -> Result<Option<RenderedContent>, String> {
        Ok(self.ready.take())
    }
}
fn indicators(stream: &mut UnixStream, generation: u64) {
    for frame in encode_shell_indicator_snapshot(
        TransactionId::from_raw(90 + generation),
        &ShellIndicatorSnapshot {
            connection_epoch: GRANT.connection_epoch,
            generation,
            active_output: Some(OutputId::from_raw(OUTPUT.id)),
            statuses: Vec::new(),
            indicators: [OUTPUT, SECOND_OUTPUT]
                .into_iter()
                .map(|output| ShellIndicator {
                    output: OutputId::from_raw(output.id),
                    indicator: output.id,
                    action: output.id + 10,
                    slot: output.id as u32,
                    state_bits: if output == OUTPUT && generation >= 2 {
                        POLICY_INDICATOR_STATE_ACTIVE
                    } else {
                        0
                    },
                    label: output.id.to_string(),
                })
                .collect(),
        },
    )
    .unwrap()
    {
        stream.write_all(&frame).unwrap();
    }
}

#[test]
fn interaction_refresh_reuses_pixels_and_stays_clickable_with_old_release_held() {
    let path = std::env::temp_dir().join(format!(
        "lom-refresh-{}-{}.sock",
        std::process::id(),
        SOCKET_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let (drained_tx, drained_rx) = std::sync::mpsc::channel();
    let caps = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
        | SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT
        | SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION;
    let server = std::thread::spawn(move || {
        let mut stream = listener.accept().unwrap().0;
        initialize_two_outputs(&mut stream, caps);
        indicators(&mut stream, 1);
        exchange(&mut stream);
        drained_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    });
    let connection = ShellConnection::connect(
        &path,
        ShellClientOptions {
            minimum_revision: 6,
            maximum_revision: 6,
            required_capabilities: caps,
            handshake_timeout: Duration::from_secs(2),
        },
    )
    .unwrap();
    let (mut config, theme) =
        parse_shell_config(include_str!("../../examples/minimal/shell.kdl")).unwrap();
    config
        .modules
        .retain(|module| module.kind == lom::config::ModuleKind::Workspaces);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut service = ShellService::new(
        connection,
        config,
        theme,
        48,
        CountedRenderer {
            calls: calls.clone(),
            ready: None,
        },
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while drained_rx.try_recv().is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "interaction refresh stalled behind old retirement"
        );
        service.step().unwrap();
        std::thread::yield_now();
    }
    let rendered = calls.lock().unwrap();
    assert_eq!(rendered.iter().filter(|id| **id == OUTPUT.id).count(), 2);
    assert_eq!(
        rendered
            .iter()
            .filter(|id| **id == SECOND_OUTPUT.id)
            .count(),
        1
    );
    done_tx.send(()).unwrap();
    server.join().unwrap();
    std::fs::remove_file(path).unwrap();
}

fn exchange(stream: &mut UnixStream) {
    let mut resources = BTreeMap::new();
    let mut candidate = None;
    let mut published = BTreeMap::new();
    let mut placed = BTreeMap::new();
    let mut generations = 0;
    let mut retired = None;
    let mut refreshed = false;
    let mut acknowledgements = 0;
    let mut activations = 0;
    let mut next_event = 100;
    loop {
        let frame = read_frame(stream);
        if let Ok((transaction, activation)) = decode_shell_indicator_activation(&frame) {
            assert_eq!(activation.action, activation.indicator + 10);
            assert!((1..=3).contains(&activation.snapshot_generation));
            activations += 1;
            stream
                .write_all(
                    &encode_shell_indicator_activation_outcome(
                        transaction,
                        &ShellIndicatorActivationOutcome {
                            connection_epoch: GRANT.connection_epoch,
                            snapshot_generation: activation.snapshot_generation,
                            event_id: activation.event_id,
                            status: ShellIndicatorActivationStatus::Accepted,
                            reason: 0,
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
        } else {
            let (transaction, record) = decode_shell_content_frame(&frame).unwrap();
            match record {
                ShellContentRecord::ResourceBegin(begin) => {
                    assert!(
                        resources
                            .insert(begin.resource.id, (transaction, begin.clone()))
                            .is_none()
                    );
                    assert!(
                        resources.len() <= 3,
                        "unchanged output must not upload another raster"
                    );
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
                ShellContentRecord::ResourceChunk(_) => {}
                ShellContentRecord::ResourceEnd(end) => {
                    let (tx, begin) = &resources[&end.resource.id];
                    send_tx(
                        stream,
                        *tx,
                        ShellContentRecord::ResourceStatus(ContentResourceStatus {
                            grant: GRANT,
                            resource: end.resource,
                            status: 2,
                            reason: 0,
                            next_ordinal: end.chunk_count,
                            admitted_bytes: begin.total_bytes,
                        }),
                    );
                }
                ShellContentRecord::FrameDemand(demand) => send_tx(
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
                ),
                ShellContentRecord::CandidateBegin(begin) => {
                    assert!(candidate.is_none());
                    candidate = Some((transaction, begin, None));
                }
                ShellContentRecord::CandidateChunk(chunk) => {
                    let (_, begin, detail) = candidate.as_mut().unwrap();
                    let resource = chunk.placements[0].resource;
                    let target = chunk.targets[0].clone();
                    assert_eq!(
                        target.target_generation,
                        if generations < 2 {
                            1
                        } else if generations < 4 {
                            2
                        } else {
                            3
                        }
                    );
                    if (begin.output == SECOND_OUTPUT || target.target_generation == 3)
                        && let Some(previous) = placed.get(&begin.output.id)
                    {
                        assert_eq!(
                            *previous, resource,
                            "interaction-only update replaced raster"
                        );
                    }
                    placed.insert(begin.output.id, resource);
                    *detail = Some((chunk.surfaces[0].allocation, target));
                }
                ShellContentRecord::CandidateEnd(end) => {
                    let (origin, begin, detail) = candidate.take().unwrap();
                    generations += 1;
                    assert_eq!(end.candidate_generation, generations);
                    let (allocation, target) = detail.unwrap();
                    if begin.output == SECOND_OUTPUT && target.target_generation == 2 {
                        // A new candidate is submitted but not Presented. The
                        // previous model/targets must still handle its event.
                        let (old_candidate, old_allocation, old_target): &(
                            u64,
                            ContentAllocationId,
                            sophia_protocol::ContentTarget,
                        ) = &published[&begin.output.id];
                        send(
                            stream,
                            99,
                            ShellContentRecord::Action(ContentAction {
                                grant: GRANT,
                                output: begin.output,
                                candidate_generation: *old_candidate,
                                presentation_epoch: *old_candidate + 30,
                                interaction_generation: 1,
                                allocation: *old_allocation,
                                target_id: old_target.target_id,
                                target_generation: old_target.target_generation,
                                action_id: old_target.action_id,
                                event_id: next_event,
                                kind: 1,
                                reason: 0,
                            }),
                        );
                        next_event += 1;
                    }
                    published.insert(begin.output.id, (generations, allocation, target.clone()));
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
                                    presentation_epoch: if kind == 2 {
                                        generations + 30
                                    } else {
                                        0
                                    },
                                    work_area_generation: 8,
                                    wm_commit_generation: 9,
                                },
                            ),
                        );
                    }
                    if generations > 2 {
                        send(
                            stream,
                            100 + generations,
                            ShellContentRecord::Action(ContentAction {
                                grant: GRANT,
                                output: begin.output,
                                candidate_generation: generations,
                                presentation_epoch: generations + 30,
                                interaction_generation: 1,
                                allocation,
                                target_id: target.target_id,
                                target_generation: target.target_generation,
                                action_id: target.action_id,
                                event_id: next_event,
                                kind: 1,
                                reason: 0,
                            }),
                        );
                        next_event += 1;
                    }
                    if generations == 2 {
                        indicators(stream, 2);
                    }
                }
                ShellContentRecord::ResourceRetire(value) => {
                    assert!(
                        retired.replace((transaction, value)).is_none(),
                        "reused pixels must not be retired"
                    );
                }
                ShellContentRecord::ActionAck(ack) => {
                    assert_eq!(ack.disposition, 1);
                    acknowledgements += 1;
                }
                other => panic!("unexpected refresh record: {other:?}"),
            }
        }
        if generations == 4 && retired.is_some() && !refreshed {
            // Keep the actual old resource unreleased across both outputs'
            // next interaction-only candidate and exact action responses.
            indicators(stream, 3);
            refreshed = true;
        }
        if generations == 6 && activations == 5 && acknowledgements == 5 {
            assert_eq!(resources.len(), 3);
            let (tx, old) = retired.unwrap();
            send_tx(
                stream,
                tx,
                ShellContentRecord::ResourceReleased(sophia_protocol::ContentResourceReleased {
                    grant: GRANT,
                    resource: old.resource,
                    reason: 0,
                }),
            );
            break;
        }
    }
}
