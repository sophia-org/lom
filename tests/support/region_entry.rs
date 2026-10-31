//! Actual entry startup, owner loop and writer; the peer checks delivered pixels.
use super::*;

#[test]
#[ignore = "subprocess entry; run by exact_region_entry_records"]
fn region_entry_child() {
    let entry = match std::env::var("LOM_REGION_ENTRY_TEST").unwrap().as_str() {
        "full" => Entry::CpuRegionsFull,
        "regions" => Entry::CpuRegions,
        "full-cadence" => Entry::CpuRegionCadenceFull,
        "regions-cadence" => Entry::CpuRegionCadence,
        _ => panic!("unknown entry"),
    };
    let stop = AtomicBool::new(false);
    let (wake, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let worker = start_renderer(
        entry,
        1,
        false,
        crate::config::Renderer::Cpu,
        &stop,
        wake.as_fd(),
    )
    .unwrap();
    crate::service::region_record_session_for_tests(worker, |service, stop| {
        drive(entry, service, stop).unwrap();
    });
}

#[test]
fn exact_region_entry_records() {
    for role in ["full", "regions", "full-cadence", "regions-cadence"] {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ls-{}-{nanos}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let log = fs::File::create(root.join("log")).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cli::serve::tests::region_entry::region_entry_child",
                "--ignored",
                "--nocapture",
            ])
            .env_clear()
            .env("TMPDIR", &root)
            .env("LOM_REGION_ENTRY_TEST", role)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut process = Process { child, root };
        wait_until(|| process.child.try_wait().unwrap().is_some());
        let text = fs::read_to_string(process.root.join("log")).unwrap();
        assert!(process.child.wait().unwrap().success(), "{role}: {text}");
        let rows: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with("lom_"))
            .collect();
        if let Some(role) = role.strip_suffix("-cadence") {
            assert_eq!(rows.len(), 7, "{text}");
            assert_eq!(
                rows[0],
                format!(
                    "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used diagnostic=cpu-region-cadence role={role}"
                )
            );
            assert_eq!(
                rows[1],
                format!(
                    "lom_region_cadence schema=1 source=fixture role={role} quota=2120 fixtures=01,02"
                )
            );
            assert_eq!(
                rows[2],
                "lom_cadence schema=1 mode=alternate-after-presented quota=2120 diagnostic=cpu-region-cadence"
            );
            assert_eq!(
                rows[3],
                "lom_panel_candidate schema=2 status=presented connection_epoch=41 content_grant_epoch=9 output=2 candidate_generation=1 presentation_epoch=31 indicator_generation=0 width=1920 height=24 bytes=184320 checksum=unavailable raster_source=rendered"
            );
            let mut sample = rows[4].to_owned();
            let mut times = Vec::new();
            for key in [
                "eligible_render_attempt_ns",
                "ring_commit_and_offer_queued_ns",
                "presented_observed_ns",
            ] {
                let field = rows[4]
                    .split_whitespace()
                    .find(|s| s.starts_with(&format!("{key}=")))
                    .unwrap();
                times.push(field.split_once('=').unwrap().1.parse::<u64>().unwrap());
                sample = sample.replace(field, &format!("{key}=TIME"));
            }
            assert!(times[0] > 0 && times[0] <= times[1] && times[1] <= times[2]);
            let deferrals = rows[4]
                .split_whitespace()
                .find(|s| s.starts_with("fill_deferrals="))
                .unwrap();
            let count = deferrals.split_once('=').unwrap().1.parse::<u64>().unwrap();
            assert_eq!(deferrals, format!("fill_deferrals={count}"));
            sample = sample.replace(deferrals, "fill_deferrals=COUNT");
            assert_eq!(
                sample,
                "lom_cadence_sample schema=1 sequence=1 candidate_generation=1 eligible_render_attempt_ns=TIME ring_commit_and_offer_queued_ns=TIME presented_observed_ns=TIME renders_started=1 renders_completed=1 uploads=1 raster_reuses=0 fill_deferrals=COUNT upload_deferrals=0"
            );
            assert_eq!(
                rows[5],
                format!(
                    "lom_region_cadence_sample schema=1 source=fixture role={role} sequence=1 model=0 placements=1 uploads=1 upload_bytes=184320 retained=0 regions_disabled=0"
                )
            );
            assert!(
                rows[6].starts_with(
                    "lom_cadence_final schema=1 status=refused quota=2120 presented=1 "
                )
            );
            continue;
        }
        assert_eq!(rows, [
            format!("lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used diagnostic=cpu-region-sequence role={role}"),
            format!("lom_region_sequence schema=1 source=fixture role={role} phases=9 hold_ms=10000"),
            format!("lom_region_phase schema=1 event=installed source=fixture role={role} phase=0 name=initial"),
            "lom_panel_candidate schema=2 status=presented connection_epoch=41 content_grant_epoch=9 output=2 candidate_generation=1 presentation_epoch=31 indicator_generation=0 width=1920 height=24 bytes=184320 checksum=unavailable raster_source=rendered".into(),
            format!("lom_region_phase schema=1 event=presented source=fixture role={role} phase=0 name=initial candidate_generation=1 placements=1 uploads=1 upload_bytes=184320 regions_disabled=0"),
            format!("lom_region_final schema=1 source=fixture role={role} status=refused phases=9 presented=1"),
        ], "{text}");
    }
}

#[test]
fn sequence_entries_refuse_gpu_copy_and_dma_before_connecting() {
    use crate::config::Renderer::{Cpu, CpuCopyReference, Gpu};
    let (mut config, _) =
        parse_shell_config(include_str!("../../examples/minimal/shell.kdl")).unwrap();
    for entry in [
        Entry::CpuRegions,
        Entry::CpuRegionsFull,
        Entry::CpuRegionCadence,
        Entry::CpuRegionCadenceFull,
    ] {
        for renderer in [Cpu, CpuCopyReference, Gpu] {
            config.renderer = renderer;
            assert_eq!(resolve(entry, None, None, &config).is_ok(), renderer == Cpu);
            config.dma_buf = true;
            assert!(resolve(entry, None, None, &config).is_err());
            config.dma_buf = false;
        }
    }
}
