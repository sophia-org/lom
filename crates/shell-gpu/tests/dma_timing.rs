//! Exercise the production export-timing accumulator without opening a GPU.
#[path = "../src/dma_timing.rs"]
mod dma_timing;
use dma_timing::{DmaTiming, Stage, Timer};
use std::time::{Duration, Instant};

fn busy(at_least: Duration) {
    let start = Instant::now();
    while start.elapsed() < at_least {
        std::hint::black_box(());
    }
}

#[test]
fn timing_is_off_until_enabled_and_records_nothing() {
    let timer = Timer::default();
    let mark = timer.start();
    assert!(mark.is_none());
    timer.record(Stage::Draw, mark);
    assert_eq!(timer.totals(), None);
}

#[test]
fn completed_stages_accumulate_thread_cpu_and_wall_time() {
    let mut timer = Timer::default();
    timer.set_enabled(true);
    for _ in 0..2 {
        let mark = timer.start();
        busy(Duration::from_millis(5));
        timer.record(Stage::Draw, mark);
    }
    let mark = timer.start();
    std::thread::sleep(Duration::from_millis(5));
    timer.record(Stage::Release, mark);
    let totals = timer.totals().unwrap();
    assert_eq!(totals.draw.count, 2);
    // Spinning burns this thread's CPU; preemption may still take some wall time.
    assert!(totals.draw.thread_cpu_ns > 0, "{totals:?}");
    assert!(totals.draw.wall_ns >= 10_000_000, "{totals:?}");
    // Sleeping takes wall time, not CPU time.
    assert_eq!(totals.release.count, 1);
    assert!(totals.release.wall_ns >= 5_000_000, "{totals:?}");
    assert!(
        totals.release.thread_cpu_ns < totals.release.wall_ns,
        "{totals:?}"
    );
    // A stage that never completed records nothing.
    assert_eq!(totals.setup, Default::default());
}

#[test]
fn a_reuse_counts_only_in_its_own_stage() {
    let mut timer = Timer::default();
    timer.set_enabled(true);
    timer.record(Stage::Reuse, timer.start());
    let totals = timer.totals().unwrap();
    assert_eq!(totals.reuse.count, 1);
    assert_eq!(
        DmaTiming {
            reuse: Default::default(),
            ..totals
        },
        DmaTiming::default(),
        "no fresh stage is counted for a reused frame"
    );
}

#[test]
fn a_frame_clone_shares_totals_until_timing_is_reset() {
    let mut timer = Timer::default();
    timer.set_enabled(true);
    let frame = timer.clone();
    frame.record(Stage::Destroy, frame.start());
    assert_eq!(timer.totals().unwrap().destroy.count, 1);
    // Enabling again keeps the totals.
    timer.set_enabled(true);
    assert_eq!(timer.totals().unwrap().destroy.count, 1);
    // Disabling detaches; re-enabling starts from zero, and an older
    // frame's late record does not reach the new totals.
    timer.set_enabled(false);
    assert_eq!(timer.totals(), None);
    timer.set_enabled(true);
    frame.record(Stage::Destroy, frame.start());
    assert_eq!(timer.totals(), Some(DmaTiming::default()));
}
