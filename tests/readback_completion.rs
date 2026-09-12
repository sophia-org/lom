//! Exercise the production deadline function without opening a GPU.
#[path = "../src/render/completion.rs"]
mod completion;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_successful_poll_without_a_callback_is_not_completion() {
    let (_sender, receiver) = mpsc::channel();
    assert!(completion::wait_for_map(Duration::from_millis(1), &receiver, |_| Ok(())).is_err());
}

#[test]
fn poll_failure_and_failed_mapping_never_count_as_completion() {
    let (sender, receiver) = mpsc::channel();
    sender.send(Ok(())).unwrap();
    assert_eq!(
        completion::wait_for_map(Duration::from_secs(2), &receiver, |_| Err(
            "device timeout".into()
        )),
        Err("device timeout".into())
    );
    let (sender, receiver) = mpsc::channel();
    sender.send(Err("map failure".into())).unwrap();
    assert_eq!(
        completion::wait_for_map(Duration::from_secs(2), &receiver, |_| Ok(())),
        Err("map failure".into())
    );
}

#[test]
fn actual_callback_completes_the_finite_poll_budget() {
    let (sender, receiver) = mpsc::channel();
    completion::wait_for_map(Duration::from_secs(2), &receiver, |budget| {
        assert_eq!(budget, Duration::from_secs(2));
        sender.send(Ok(())).unwrap();
        Ok(())
    })
    .unwrap();
    assert!(
        completion::wait_for_map(Duration::ZERO, &receiver, |_| panic!("expired job polled"))
            .is_err()
    );
}
