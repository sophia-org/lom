use super::*;

#[test]
fn both_sequence_roles_present_the_same_nine_snapshots_with_the_predicted_uploads() {
    for height in [24, 512] {
        let mut reference = Vec::new();
        for full in [true, false] {
            let mut session = session::Session::new(height);
            session.service.enable_region_sequence(full).unwrap();
            assert!(!session.service.live_model_inputs());
            assert!(session.service.enable_region_sequence(full).is_err());
            let expected_tiles = [1, 3, 1, 1, 2, 1, 1, 3, 1];
            for phase in 0..9 {
                let model = session.service.panels[0].model.clone();
                assert!(model.fixture);
                assert_eq!(model.workspaces.generation, 0);
                assert!(model.workspaces.entries.iter().all(|w| w.action.is_none()));
                let frame = session.frame(&model);
                assert_eq!(frame.generation, phase as u64 + 1);
                if full {
                    assert_eq!(
                        (frame.uploads, frame.upload_bytes),
                        (1, 1920 * height as usize * 4)
                    );
                    reference.push(frame.pixels);
                } else {
                    assert_eq!(frame.pixels, reference[phase]);
                    let tiles = expected_tiles[phase];
                    assert_eq!(frame.uploads, tiles);
                    let bytes = if [0, 5, 6].contains(&phase) {
                        1920 * height as usize * 4
                    } else {
                        tiles * 640 * height as usize * 4
                    };
                    assert_eq!(frame.upload_bytes, bytes);
                    assert!(!session.service.panels[0].regions_disabled);
                }
                if phase < 8 {
                    let deadline = session.service.sequence.as_ref().unwrap().next_at.unwrap();
                    session
                        .service
                        .advance_sequence(deadline - Duration::from_nanos(1))
                        .unwrap();
                    assert_eq!(
                        session.service.panels[0].model, model,
                        "hold stays immutable"
                    );
                    session.service.advance_sequence(deadline).unwrap();
                }
            }
            assert!(
                session
                    .service
                    .region_sequence_final()
                    .unwrap()
                    .contains("status=complete phases=9 presented=9")
            );
            assert!(session.service.sequence.as_ref().unwrap().next_at.is_none());
            for pair in reference.windows(2) {
                assert_ne!(pair[0], pair[1], "adjacent phases must differ");
            }
        }
    }
}

#[test]
fn sequence_configuration_and_extra_candidates_fail_closed() {
    let mut session = session::Session::new(24);
    let original = session.service.config.clone();
    session.service.config.modules[1].format = "%S".into();
    assert!(session.service.enable_region_sequence(false).is_err());
    session.service.config = original;
    session.service.enable_region_sequence(false).unwrap();
    let model = session.service.panels[0].model.clone();
    session.frame(&model);
    assert!(
        session
            .service
            .sequence_presented(0, 1, 1, 184320, false)
            .is_err()
    );
    assert!(
        session
            .service
            .region_sequence_final()
            .unwrap()
            .contains("status=refused")
    );
}

#[test]
fn live_observations_do_not_change_the_fixture_and_content_input_refuses() {
    for full in [true, false] {
        let mut session = session::Session::new(24);
        session.service.enable_region_sequence(full).unwrap();
        let model = session.service.panels[0].model.clone();
        session.frame(&model);
        session.service.observed_clock = Some(model.time + chrono::Duration::days(1));
        session.control(session::Control::Indicators(17));
        session.until(|service| {
            service
                .latest_indicators
                .as_ref()
                .is_some_and(|s| s.generation == 17)
        });
        let panel = &session.service.panels[0];
        assert_eq!(panel.model, model);
        assert!(!panel.dirty && !panel.interaction_dirty);
        assert_eq!(session.service.next_candidate_generation, 2);
        let presented = panel.presented.as_ref().unwrap();
        // Even an unsolicited action cannot enter the fixture-owned model.
        let action = ContentAction {
            grant: session.service.limits.grant,
            output: panel.output.output,
            allocation: panel.allocation.allocation,
            candidate_generation: presented.candidate_generation,
            presentation_epoch: presented.presentation_epoch,
            interaction_generation: 1,
            target_id: 1,
            target_generation: 1,
            action_id: 1,
            event_id: 1,
            kind: 1,
            reason: 0,
        };
        session.control(session::Control::Action(action));
        let deadline = Instant::now() + Duration::from_secs(2);
        let error = loop {
            if let Err(error) = session.service.step() {
                break error;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert_eq!(error, "fixture sequence refuses content input");
        assert_eq!(session.service.panels[0].model, model);
        assert_eq!(session.service.next_candidate_generation, 2);
    }
}
