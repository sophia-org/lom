use super::*;
#[path = "region_session.rs"]
mod session;

#[test]
fn integer_thirds_cover_each_pixel_exactly_once_on_both_axes() {
    for (width, height, position) in [
        (1920, 24, Position::Top),
        (1920, 512, Position::Bottom),
        (24, 1080, Position::Left),
        (512, 1080, Position::Right),
    ] {
        let regions = thirds(width, height, (1, 1), position).unwrap();
        let mut covered = vec![0u8; (width * height) as usize];
        for region in regions {
            for y in region.y as u32..region.y as u32 + region.height {
                for x in region.x as u32..region.x as u32 + region.width {
                    covered[(y * width + x) as usize] += 1;
                }
            }
        }
        assert!(covered.iter().all(|n| *n == 1));
    }
}

#[test]
fn fractional_seams_scale_and_invalid_dimensions_keep_full_frame() {
    for (width, height, scale, position) in [
        (1366, 24, (1, 1), Position::Top),
        (24, 1079, (1, 1), Position::Left),
        (1920, 24, (5, 4), Position::Top),
        (1920, 24, (0, 0), Position::Top),
        (0, 24, (1, 1), Position::Top),
        (u32::MAX, 24, (1, 1), Position::Top),
    ] {
        assert!(thirds(width, height, scale, position).is_none());
    }
}

#[test]
fn retained_region_updates_match_full_pixels_across_model_changes_and_fallback() {
    for height in [24, 512] {
        let mut session = session::Session::new(height);
        let model = session.service.panels[0].model.clone();
        let first = session.frame(&model);
        assert_eq!(first.uploads, 1);
        let model = session.set(1, 1);
        let established = session.frame(&model);
        assert_eq!(established.resources.len(), 3);
        assert_eq!(established.uploads, 3);
        let model = session.set(2, 1);
        let clock = session.frame(&model);
        assert_eq!(
            (clock.uploads, clock.upload_bytes),
            (1, 640 * height as usize * 4)
        );
        assert_eq!(&clock.resources[..2], &established.resources[..2]);
        session.service.panels[0].model.workspaces.entries[0].action = Some(900);
        session.service.panels[0].interaction_dirty = true;
        let model = session.service.panels[0].model.clone();
        let retargeted = session.frame(&model);
        assert_eq!(retargeted.uploads, 0);
        assert_eq!(retargeted.resources, clock.resources);
        let model = session.set(2, 2);
        let workspace = session.frame(&model);
        assert_eq!(
            (workspace.uploads, workspace.upload_bytes),
            (1, 640 * height as usize * 4)
        );
        assert_eq!(&workspace.resources[1..], &clock.resources[1..]);
        let model = session.set(60, 2);
        let minute = session.frame(&model);
        assert_eq!(minute.uploads, 2);
        assert_eq!(minute.resources[0], workspace.resources[0]);
        let clock_id = {
            let m = &session.service.panels[0].model;
            m.module_id(
                m.config
                    .modules
                    .iter()
                    .position(|c| c.kind == ModuleKind::Clock)
                    .unwrap(),
            )
        };
        session.service.panels[0].model.popout = Some(clock_id);
        let model = session.set(61, 2);
        assert_eq!(session.frame(&model).resources.len(), 1);
        session.service.panels[0].model.popout = None;
        let model = session.set(62, 2);
        assert_eq!(session.frame(&model).resources.len(), 1);
        let model = session.set(63, 2);
        assert_eq!(session.frame(&model).uploads, 3);
        session.service.renderer.fault_at = Some(session.service.renderer.partial_completions + 2);
        let model = session.set(120, 2);
        let fallback = session.frame(&model);
        assert_eq!(fallback.resources.len(), 1);
        assert!(session.service.panels[0].regions_disabled);
        let model = session.set(121, 2);
        assert_eq!(
            session.frame(&model).upload_bytes,
            1920 * height as usize * 4
        );
    }
}

#[test]
fn vertical_updates_and_nondivisible_geometry_match_the_full_panel() {
    for position in [Position::Left, Position::Right] {
        let mut session = session::Session::geometry(24, 1080, position);
        let model = session.service.panels[0].model.clone();
        session.frame(&model);
        let model = session.set(1, 1);
        assert_eq!(session.frame(&model).uploads, 3);
        let model = session.set(2, 1);
        let frame = session.frame(&model);
        assert_eq!((frame.uploads, frame.upload_bytes), (1, 24 * 360 * 4));
    }
    let mut session = session::Session::geometry(1366, 24, Position::Top);
    let model = session.service.panels[0].model.clone();
    session.frame(&model);
    for second in [1, 2] {
        let model = session.set(second, 1);
        let frame = session.frame(&model);
        assert_eq!((frame.uploads, frame.upload_bytes), (1, 1366 * 24 * 4));
    }
}

#[test]
fn slot_release_serializes_offers_and_observations_keep_the_update_model() {
    let mut session = session::Session::new(24);
    let model = session.service.panels[0].model.clone();
    session.frame(&model);
    session.control(session::Control::HoldSlots(true));
    let first = session.set(1, 1);
    session.until(|s| s.next_resource == 3);
    for _ in 0..100 {
        assert_eq!(session.service.step().unwrap(), 0);
    }
    assert_eq!(session.service.renderer.partial_completions, 1);
    assert_eq!(
        session.service.panels[0]
            .resources
            .iter()
            .filter(|r| r.state == ResourceState::Reserved)
            .count(),
        2
    );
    let latest = session.set(60, 2);
    session.control(session::Control::HoldSlots(false));
    assert_eq!(session.frame(&first).uploads, 3);
    assert_eq!(session.frame(&latest).uploads, 3);
}

#[test]
fn retiring_resources_are_never_borrowed_for_the_next_update() {
    let mut session = session::Session::new(24);
    let model = session.service.panels[0].model.clone();
    session.frame(&model);
    let model = session.set(1, 1);
    session.frame(&model);
    session.until(|s| {
        s.panels[0]
            .resources
            .iter()
            .all(|r| r.state != ResourceState::Retiring)
    });
    session.control(session::Control::HoldResources(true));
    let model = session.set(60, 2);
    assert_eq!(session.frame(&model).uploads, 3);
    assert_eq!(
        session.service.panels[0]
            .resources
            .iter()
            .filter(|r| r.state == ResourceState::Retiring)
            .count(),
        3
    );
    let next = session.set(61, 2);
    let ids = session.service.next_resource;
    let renders = session.service.renderer.partial_completions;
    for _ in 0..100 {
        assert_eq!(session.service.step().unwrap(), 0);
    }
    assert_eq!(session.service.next_resource, ids);
    assert_eq!(session.service.renderer.partial_completions, renders);
    session.control(session::Control::HoldResources(false));
    assert_eq!(session.frame(&next).uploads, 1);
}

#[test]
fn admission_reserves_the_whole_update_before_any_offer() {
    let mut session = session::Session::new(24);
    let model = session.service.panels[0].model.clone();
    session.frame(&model);
    let model = session.set(1, 1);
    let budget = session.service.limits.max_resident_bytes;
    session.service.limits.max_resident_bytes = 1920 * 24 * 4 + 2 * 640 * 24 * 4;
    for _ in 0..100 {
        assert_eq!(session.service.step().unwrap(), 0);
    }
    assert_eq!(session.service.next_resource, 2);
    assert_eq!(session.service.renderer.partial_completions, 0);
    assert!(
        session.service.panels[0]
            .resources
            .iter()
            .all(|r| matches!(r.state, ResourceState::Free | ResourceState::Resident))
    );
    session.service.limits.max_resident_bytes = budget;
    assert_eq!(session.frame(&model).uploads, 3);
}

#[test]
fn an_unoffered_nonopaque_region_frees_reservations_without_spending_generations() {
    let mut session = session::Session::new(24);
    let model = session.service.panels[0].model.clone();
    session.frame(&model);
    let before = session.service.panels[0].resources;
    let current = session.service.panels[0].current;
    session.service.renderer.fault_at = Some(1);
    let model = session.set(1, 1);
    session.until(|s| s.panels[0].regions_disabled);
    assert_eq!(session.service.next_resource, 2);
    assert_eq!(session.service.panels[0].current, current);
    for (old, new) in before.iter().zip(session.service.panels[0].resources) {
        assert_eq!(
            (old.id, old.generation, old.bytes),
            (new.id, new.generation, new.bytes)
        );
        assert!(old.state == new.state);
    }
    assert_eq!(session.frame(&model).uploads, 1);
}

#[test]
fn a_nonopaque_second_tile_retires_only_the_accepted_unpublished_tile() {
    let mut session = session::Session::new(24);
    let initial = session.service.panels[0].model.clone();
    session.frame(&initial);
    let current = session.service.panels[0].current;
    session.control(session::Control::HoldResources(true));
    session.service.renderer.fault_at = Some(2);
    let model = session.set(1, 1);
    session.until(|s| s.panels[0].regions_disabled);
    assert_eq!(
        session.service.next_resource, 3,
        "only the first tile was offered"
    );
    assert_eq!(session.service.panels[0].current, current);
    assert_eq!(session.service.next_candidate_generation, 2);
    let slots = &session.service.panels[0].resources;
    let accepted = slots.iter().position(|r| r.id == 2).unwrap();
    assert!(slots[accepted].state == ResourceState::Accepted);
    assert_eq!(slots[accepted].generation, 1);
    assert!(
        slots
            .iter()
            .filter(|r| r.id == 0)
            .all(|r| r.state == ResourceState::Free && r.generation == 1)
    );
    session.until(|s| s.panels[0].resources[accepted].state == ResourceState::Retiring);
    assert_eq!(session.service.panels[0].current, current);
    assert_eq!(
        session.service.next_candidate_generation, 2,
        "discard has no candidate"
    );
    session.control(session::Control::HoldResources(false));
    session.until(|s| s.panels[0].resources[accepted].generation == 2);
    assert!(session.service.panels[0].resources[accepted].state == ResourceState::Free);
    let full = session.frame(&model);
    assert_eq!(full.resources.len(), 1);
    assert_eq!(
        full.uploads, 2,
        "discarded accepted tile plus full replacement"
    );
    assert!(session.service.panels[0].regions_disabled);
}
