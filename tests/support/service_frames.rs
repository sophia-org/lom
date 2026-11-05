//! The real Lom CPU worker and file client, with an independent scripted frame owner.
use super::{session::Session, *};
use crate::service::tests::files_peer::Peer;
use sophia_shell_protocol::ContentCandidateOutcome;
use sophia_shell_protocol::shell_files::*;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Script {
    pub(super) action_before: Option<bool>,
    key: Option<ShellFrameStreamKey>,
    commit: Option<ShellFrameCommit>,
    last_stream: u64,
}

impl Script {
    pub(super) fn pending(&self) -> bool {
        self.commit.is_some()
    }

    pub(super) fn receive(
        &mut self,
        peer: &Peer,
        record: ShellFrameRecord,
        parts: &mut VecDeque<(TransactionId, ShellContentRecord)>,
    ) {
        match record {
            ShellFrameRecord::Open { transaction, key } => {
                assert!(self.key.is_none());
                assert!(key.stream_id > self.last_stream);
                self.last_stream = key.stream_id;
                self.key = Some(key);
                peer.frame(ShellFrameRecord::Opened {
                    transaction,
                    key,
                    ttl_millis: 250,
                });
            }
            ShellFrameRecord::Close { transaction, key } => {
                assert_eq!(self.key, Some(key));
                assert!(self.commit.is_none());
                self.key = None;
                peer.frame(ShellFrameRecord::Closed {
                    transaction,
                    key,
                    reason: ContentReason::Stale as u16,
                });
            }
            ShellFrameRecord::Commit(commit) => {
                let key = self.key.expect("commit only on open stream");
                assert!(self.commit.is_none(), "one frame credit");
                assert_eq!(commit.candidate.pacing_permit, key.stream_id);
                assert_eq!(commit.candidate.grant, key.grant);
                assert_eq!(commit.candidate.output, key.output);
                assert_eq!(commit.candidate.facts_generation, key.facts_generation);
                parts.extend(commit.candidate.parts().map(|r| (commit.transaction, r)));
                self.commit = Some(commit);
            }
            record => panic!("client sent {record:?}"),
        }
    }

    pub(super) fn present(
        &mut self,
        peer: &Peer,
        transaction: TransactionId,
        outcome: ContentCandidateOutcome,
        parts: &mut VecDeque<(TransactionId, ShellContentRecord)>,
    ) -> bool {
        let Some(commit) = self.commit.take() else {
            return false;
        };
        assert_eq!(commit.transaction, transaction);
        assert_eq!(
            commit.candidate.candidate_generation,
            outcome.candidate_generation
        );
        let key = self.key.unwrap();
        let action_before = self.action_before.take();
        let action = action_before.map(|_| {
            let target = &commit.candidate.targets[0];
            ContentAction {
                grant: key.grant,
                output: key.output,
                allocation: key.allocation,
                candidate_generation: outcome.candidate_generation,
                presentation_epoch: outcome.presentation_epoch,
                interaction_generation: commit.candidate.interaction_generation,
                target_id: target.target_id,
                target_generation: target.target_generation,
                action_id: target.action_id,
                event_id: 301,
                kind: 1,
                reason: 0,
            }
        });
        let mut prepared = outcome.clone();
        prepared.kind = 1;
        prepared.presentation_epoch = 0;
        for (outcome, credit_expires_msec) in [(prepared, 0), (outcome, 250)] {
            if outcome.kind == 2 && action_before == Some(true) {
                peer.content(
                    TransactionId::from_raw(301),
                    ShellContentRecord::Action(action.clone().unwrap()),
                );
            }
            peer.frame(ShellFrameRecord::Outcome {
                transaction,
                outcome,
                stream_id: key.stream_id,
                allocation: key.allocation,
                facts_generation: key.facts_generation,
                credit_expires_msec,
            });
        }
        if action_before == Some(false) {
            peer.content(
                TransactionId::from_raw(301),
                ShellContentRecord::Action(action.unwrap()),
            );
        }
        if let Some(retire) = commit.retire {
            parts.push_back((
                retire.transaction,
                ShellContentRecord::ResourceRetire(ContentResourceRetire {
                    grant: key.grant,
                    resource: retire.resource,
                }),
            ));
        }
        true
    }
}

#[test]
fn sft_establishes_regions_then_copies_retains_and_drains_without_changing_pixels() {
    let mut session = Session::sft(24);
    let model = session.service.panels[0].model.clone();
    let first = session.frame(&model);
    assert!(!first.stream);
    assert_eq!(first.uploads, 1);
    let model = session.set(1, 1);
    let established = session.frame(&model);
    assert!(!established.stream);
    assert_eq!(established.uploads, 3);
    let model = session.set(2, 1);
    let copied = session.frame(&model);
    assert!(copied.stream);
    assert_eq!(copied.uploads, 1);
    assert_eq!(&copied.resources[..2], &established.resources[..2]);
    session.service.panels[0].model.workspaces.entries[0].action = Some(900);
    session.service.panels[0].interaction_dirty = true;
    let model = session.service.panels[0].model.clone();
    let retained = session.frame(&model);
    assert!(retained.stream);
    assert_eq!(retained.uploads, 0);
    assert_eq!(retained.resources, copied.resources);
    let model = session.set(60, 1);
    let fallback = session.frame(&model);
    assert!(!fallback.stream);
    assert_eq!(fallback.uploads, 2);
    assert_eq!(
        session.service.panels[0].frame.stream.state(),
        sophia_shell_client::FrameStreamState::Closed
    );
    let model = session.set(61, 1);
    assert!(
        !session.frame(&model).stream,
        "unsupported active updates stay legacy on this binding"
    );
}

fn established() -> Session {
    let mut session = Session::sft(24);
    let model = session.service.panels[0].model.clone();
    session.frame(&model);
    let model = session.set(1, 1);
    session.frame(&model);
    session
}

#[test]
fn sft_full_panel_worker_submits_the_first_frame_without_legacy_establishment() {
    for height in [24, 512] {
        let mut session = Session::sft(height);
        session.service.renderer.regions = false;
        let model = session.service.panels[0].model.clone();
        let first = session.frame(&model);
        assert!(first.stream);
        assert_eq!(
            (first.uploads, first.upload_bytes),
            (1, 1920 * height as usize * 4)
        );
        let model = session.set(1, 1);
        let second = session.frame(&model);
        assert!(second.stream);
        assert_eq!(
            (second.uploads, second.upload_bytes),
            (1, 1920 * height as usize * 4)
        );
        assert_ne!(first.resources, second.resources);
    }
}

#[test]
fn sft_presented_does_not_release_the_source_loan_or_predecessor() {
    use super::session::Control;
    let mut session = established();
    session.control(Control::HoldSlots(true));
    session.control(Control::HoldResources(true));
    let model = session.set(2, 1);
    session.until(|service| {
        service.panels[0]
            .presented
            .as_ref()
            .unwrap()
            .candidate_generation
            == 3
    });
    assert_eq!(
        session.service.presentations.len(),
        1,
        "Presented must not settle source custody"
    );
    assert!(
        session.service.panels[0]
            .resources
            .iter()
            .any(|r| r.state == ResourceState::Staging)
    );
    for _ in 0..8 {
        assert_eq!(session.service.step().unwrap(), 0);
    }
    assert!(
        session.service.panels[0]
            .resources
            .iter()
            .any(|r| r.state == ResourceState::Retiring)
    );
    assert!(session.service.panels[0].frame.stream.has_credit());
    session.control(Control::HoldSlots(false));
    assert!(session.frame(&model).stream);
    assert!(
        session.service.panels[0]
            .resources
            .iter()
            .any(|r| r.state == ResourceState::Retiring)
    );
    session.control(Control::HoldResources(false));
    session.until(|s| {
        s.panels[0]
            .resources
            .iter()
            .all(|r| r.state != ResourceState::Retiring)
    });
}

#[test]
fn sft_actions_follow_exact_presented_order_in_the_same_journal() {
    use super::session::Control;
    for before in [true, false] {
        let mut session = established();
        session.control(Control::FrameAction(before));
        session.set(2, 1);
        session.until(|service| service.activation_high_water == 301);
        let deadline = Instant::now() + Duration::from_secs(2);
        let ack = loop {
            if let Ok(ack) = session.action_acks.try_recv() {
                break ack;
            }
            assert!(Instant::now() < deadline, "action was not acknowledged");
            session.service.step().unwrap();
        };
        assert_eq!(ack.candidate_generation, 3);
        assert_eq!(ack.disposition, if before { 2 } else { 1 });
    }
}

#[test]
fn sft_explicit_unadmitted_refusal_keeps_the_display_and_falls_back() {
    let mut session = established();
    session.control(super::session::Control::RefuseNextFrame);
    let model = session.set(2, 1);
    let frame = session.frame(&model);
    assert_eq!(frame.generation, 4, "refused identity is spent");
    assert!(!frame.stream);
    assert_eq!(frame.uploads, 1, "refused copy was not admitted");
    assert_eq!(
        session.service.panels[0].frame.stream.state(),
        sophia_shell_client::FrameStreamState::Closed
    );
}
