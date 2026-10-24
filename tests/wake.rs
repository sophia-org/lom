//! Device-free checks of the production worker notification primitive.
#[path = "../src/wake.rs"]
mod wake;
use std::os::fd::AsFd;
use std::sync::{Arc, mpsc};
use std::time::Duration;

#[test]
fn completion_before_wait_and_between_clear_and_receive_cannot_be_lost() {
    let wake = Arc::new(wake::Wake::new().unwrap());
    let (sender, receiver) = mpsc::sync_channel(1);
    let sender = wake::Sender::new(sender, wake.clone());
    for value in [1, 2] {
        wake.clear().unwrap();
        sender.send(value).unwrap();
        let mut fds = [wake::interest(wake.as_fd(), libc::POLLIN)];
        assert!(wake::wait(&mut fds, Some(Duration::ZERO)).unwrap());
        assert_eq!(receiver.try_recv().unwrap(), value);
        wake.clear().unwrap();
        assert!(!wake::wait(&mut fds, Some(Duration::ZERO)).unwrap());
    }
}

#[test]
fn worker_exit_wakes_after_disconnecting_the_channel() {
    let wake = Arc::new(wake::Wake::new().unwrap());
    let (sender, receiver) = mpsc::sync_channel::<()>(1);
    let sender = wake::Sender::new(sender, wake.clone());
    let thread = std::thread::spawn(move || drop(sender));
    let mut fds = [wake::interest(wake.as_fd(), libc::POLLIN)];
    assert!(wake::wait(&mut fds, Some(Duration::from_secs(2))).unwrap());
    wake.clear().unwrap();
    assert_eq!(receiver.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    thread.join().unwrap();
}

#[test]
fn no_event_waits_for_the_deadline_without_spurious_readiness() {
    let wake = wake::Wake::new().unwrap();
    let mut fds = [wake::interest(wake.as_fd(), libc::POLLIN)];
    let started = std::time::Instant::now();
    let deadline = Duration::from_millis(10);
    assert!(!wake::wait(&mut fds, Some(deadline)).unwrap());
    assert!(started.elapsed() >= deadline);
}

#[test]
fn an_idle_wait_has_no_periodic_timeout_and_wakes_on_completion() {
    let wake = Arc::new(wake::Wake::new().unwrap());
    let reader = wake.clone();
    let (finished, result) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let mut fds = [wake::interest(reader.as_fd(), libc::POLLIN)];
        finished.send(wake::wait(&mut fds, None)).unwrap();
    });
    assert_eq!(
        result.recv_timeout(Duration::from_millis(20)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    wake.notify().unwrap();
    assert!(
        result
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap()
    );
    thread.join().unwrap();
}
