//! Coalesced worker notifications and deadline-bounded owner waits.
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::sync::{Arc, mpsc::SyncSender};
use std::time::Duration;

pub(crate) struct Wake(OwnedFd);

impl Wake {
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: eventfd has no pointer arguments; ownership transfers on success.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    pub(crate) fn notify(&self) -> io::Result<()> {
        let value = 1u64;
        loop {
            // SAFETY: value is a live eight-byte eventfd counter.
            let n = unsafe { libc::write(self.0.as_raw_fd(), (&value as *const u64).cast(), 8) };
            if n == 8 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()), // Already readable.
                _ => return Err(error),
            }
        }
    }

    pub(crate) fn clear(&self) -> io::Result<()> {
        let mut value = 0u64;
        loop {
            // SAFETY: value is writable for the eventfd's eight-byte read.
            let n = unsafe { libc::read(self.0.as_raw_fd(), (&mut value as *mut u64).cast(), 8) };
            if n == 8 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()),
                _ => return Err(error),
            }
        }
    }
}

impl AsFd for Wake {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

/// Drop the sender before notifying, including on a worker panic.
pub(crate) struct Sender<T> {
    sender: Option<SyncSender<T>>,
    wake: Arc<Wake>,
}

impl<T> Sender<T> {
    pub(crate) fn new(sender: SyncSender<T>, wake: Arc<Wake>) -> Self {
        Self {
            sender: Some(sender),
            wake,
        }
    }
    pub(crate) fn send(&self, value: T) -> Result<(), String> {
        self.sender
            .as_ref()
            .expect("live sender")
            .send(value)
            .map_err(|_| "worker receiver closed".to_string())?;
        self.wake
            .notify()
            .map_err(|error| format!("worker wake: {error}"))
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        drop(self.sender.take());
        let _ = self.wake.notify();
    }
}

pub(crate) fn interest(fd: BorrowedFd<'_>, events: i16) -> libc::pollfd {
    libc::pollfd {
        fd: fd.as_raw_fd(),
        events,
        revents: 0,
    }
}

/// Returns true for readiness or interruption, false for an expired timeout.
pub(crate) fn wait(fds: &mut [libc::pollfd], timeout: Option<Duration>) -> Result<bool, String> {
    let time = timeout.map(|duration| libc::timespec {
        tv_sec: duration.as_secs().min(i64::MAX as u64) as libc::time_t,
        tv_nsec: duration.subsec_nanos().into(),
    });
    // SAFETY: fds and the optional timeout remain valid for this synchronous call.
    let result = unsafe {
        libc::ppoll(
            fds.as_mut_ptr(),
            fds.len() as libc::nfds_t,
            time.as_ref().map_or(std::ptr::null(), |value| value),
            std::ptr::null(),
        )
    };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            return Ok(true);
        }
        return Err(format!("owner readiness wait: {error}"));
    }
    if fds.iter().any(|fd| fd.revents & libc::POLLNVAL != 0) {
        return Err("owner readiness descriptor became invalid".into());
    }
    Ok(result != 0)
}
