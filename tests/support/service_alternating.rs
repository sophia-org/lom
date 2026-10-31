use super::*;
use crate::service::regions::tests::session;

#[test]
fn alternating_cadence_composites_match_qualified_models_in_both_directions() {
    for height in [24, 512] {
        let mut reference = Vec::new();
        for full in [true, false] {
            let mut session = session::Session::new(height);
            session.service.enable_alternating(full, 6).unwrap();
            assert!(!session.service.live_model_inputs());
            assert!(session.service.enable_region_cadence(full).is_err());
            assert!(session.service.enable_region_sequence(full).is_err());
            for n in 1..=6 {
                let model = session.service.panels[0].model.clone();
                let frame = session.frame(&model);
                assert_eq!(frame.generation, n);
                let tiled = !full && n > 1;
                assert_eq!(frame.resources.len(), if tiled { 3 } else { 1 });
                assert_eq!(frame.uploads, if tiled && n == 2 { 3 } else { 1 });
                let bytes = if !full && n > 2 { 640 } else { 1920 } * height as usize * 4;
                assert_eq!(frame.upload_bytes, bytes);
                // The same turn may already start the next job after Presented.
                let stats = session
                    .service
                    .cadence
                    .as_ref()
                    .unwrap()
                    .alternating
                    .as_ref()
                    .unwrap()
                    .previous_stats;
                let jobs = n + if !full && n >= 2 { 2 } else { 0 };
                assert_eq!(
                    [
                        stats.renders_started,
                        stats.renders_completed,
                        stats.uploads
                    ],
                    [jobs; 3]
                );
                assert_eq!(stats.raster_reuses, 0);
                if full {
                    reference.push(frame.pixels);
                } else {
                    assert_eq!(frame.pixels, reference[n as usize - 1]);
                }
                assert!(!session.service.panels[0].regions_disabled);
            }
            assert_ne!(reference[0], reference[1]);
            assert_eq!(reference[0], reference[2]);
            assert_eq!(reference[1], reference[3]);
            assert!(
                session
                    .service
                    .cadence_final()
                    .unwrap()
                    .contains("status=complete quota=6 presented=6")
            );
            assert!(!session.service.panels[0].dirty);
            for _ in 0..10 {
                session.service.step().unwrap();
            }
            assert_eq!(session.service.next_candidate_generation, 7);
        }
    }
}

#[test]
fn alternating_cadence_refuses_contaminated_models_and_disabled_regions() {
    for disabled in [true, false] {
        let mut session = session::Session::new(24);
        session.service.enable_alternating(false, 3).unwrap();
        if disabled {
            session.service.panels[0].regions_disabled = true;
        } else {
            session.service.panels[0].model.time += chrono::Duration::days(1);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let error = loop {
            if let Err(error) = session.service.step() {
                break error;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert!(error.contains("contaminated, extra or disabled"), "{error}");
        assert!(
            session
                .service
                .cadence_final()
                .unwrap()
                .contains("status=refused")
        );
    }
}

#[test]
fn alternating_cadence_refuses_retention_in_the_wrong_positions() {
    let mut session = session::Session::new(24);
    session.service.enable_alternating(false, 3).unwrap();
    for _ in 0..2 {
        let model = session.service.panels[0].model.clone();
        session.frame(&model);
    }
    // Retention count stays two, but the Start and Center identities are wrong.
    session
        .service
        .cadence
        .as_mut()
        .unwrap()
        .alternating
        .as_mut()
        .unwrap()
        .previous
        .swap(0, 1);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Err(error) = session.service.step() {
            assert_eq!(error, "region cadence must replace only the End tile");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(
        session
            .service
            .cadence_final()
            .unwrap()
            .contains("status=refused")
    );
}

#[test]
fn alternating_cadence_ignores_live_feeds_and_refuses_content_input() {
    let mut session = session::Session::new(24);
    session.service.enable_alternating(false, 1).unwrap();
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
    assert_eq!(session.service.panels[0].model, model);
    assert!(!session.service.panels[0].dirty);
    let panel = &session.service.panels[0];
    let presented = panel.presented.as_ref().unwrap();
    session.control(session::Control::Action(ContentAction {
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
    }));
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Err(error) = session.service.step() {
            assert_eq!(error, "fixture sequence refuses content input");
            break;
        }
        assert!(Instant::now() < deadline);
    }
}
