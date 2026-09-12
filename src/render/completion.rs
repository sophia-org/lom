//! One deadline covers both the GPU poll and delivery of its map callback.
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

pub(super) fn wait_for_map(
    timeout: Duration,
    receiver: &Receiver<Result<(), String>>,
    poll: impl FnOnce(Duration) -> Result<(), String>,
) -> Result<(), String> {
    let start = Instant::now();
    if timeout.is_zero() {
        return Err("GPU readback deadline exhausted".into());
    }
    poll(timeout)?;
    receiver
        .recv_timeout(timeout.saturating_sub(start.elapsed()))
        .map_err(|error| format!("GPU readback callback: {error}"))?
}
