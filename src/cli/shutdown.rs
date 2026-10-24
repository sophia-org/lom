//! Process signals request cancellation; ordinary Rust code owns cleanup.

use signal_hook::{
    SigId,
    consts::{SIGINT, SIGTERM},
};
use std::os::unix::net::UnixStream;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct Shutdown {
    pub flag: Arc<AtomicBool>,
    pub wake: UnixStream,
    handlers: Vec<SigId>,
}

impl Shutdown {
    pub fn install() -> Result<Self, String> {
        let (wake, writer) = UnixStream::pair().map_err(|error| error.to_string())?;
        wake.set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        writer
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let mut stop = Self {
            flag: Arc::new(AtomicBool::new(false)),
            wake,
            handlers: Vec::new(),
        };
        for signal in [SIGTERM, SIGINT] {
            stop.handlers.push(
                signal_hook::flag::register(signal, stop.flag.clone())
                    .map_err(|error| format!("install shutdown signal: {error}"))?,
            );
            stop.handlers.push(
                signal_hook::low_level::pipe::register(
                    signal,
                    writer.try_clone().map_err(|error| error.to_string())?,
                )
                .map_err(|error| format!("install shutdown wake: {error}"))?,
            );
        }
        Ok(stop)
    }

    pub fn requested(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.requested() {
            Err("shell stopping".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}
