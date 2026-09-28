//! The production serve owner with a GPU-free renderer, in a separate PID 1.
use super::*;
use sophia_shell_protocol::*;
use std::{
    fs,
    io::Read,
    os::unix::net::UnixListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Instant,
};

#[expect(
    dead_code,
    reason = "shared service fixture also covers activation and resource uploads"
)]
#[path = "files_peer.rs"]
mod files_peer;
#[path = "files_wire.rs"]
mod files_wire;
const GRANT: ContentGrant = ContentGrant {
    connection_epoch: files_wire::EPOCH,
    content_grant_epoch: 9,
};
const OUTPUT: ContentOutputId = ContentOutputId {
    id: 2,
    generation: 3,
};
const CHILD: &str = "cli::serve::tests::shutdown_child";

struct Renderer(PathBuf);
impl Drop for Renderer {
    fn drop(&mut self) {
        fs::write(self.0.join("dropped"), "yes").unwrap();
    }
}
impl ContentRenderer for Renderer {
    fn submit(
        &mut self,
        _: crate::service::RenderIdentity,
        _: crate::model::Model,
        _: u32,
        _: u32,
        _: f64,
    ) -> Result<(), String> {
        fs::write(self.0.join("rendering"), "yes").unwrap();
        Ok(())
    }
    fn poll(&mut self) -> Result<Option<crate::service::RenderedContent>, String> {
        Ok(None)
    }
}

#[test]
#[ignore = "subprocess entry; run by the shutdown tests"]
fn shutdown_child() {
    let root = PathBuf::from(std::env::var_os("LOM_SHUTDOWN_TEST").expect("parent fixture"));
    assert_eq!(std::process::id(), 1);
    run_with_renderer(|_| {
        fs::write(root.join("renderer"), "yes").unwrap();
        Ok(Renderer(root.clone()))
    })
    .unwrap();
    fs::write(root.join("returned"), "yes").unwrap();
}

struct Process {
    child: Child,
    root: PathBuf,
}
impl Drop for Process {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "child log: {}",
                fs::read_to_string(self.root.join("log")).unwrap_or_default()
            );
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !condition() {
        assert!(Instant::now() < end, "shutdown fixture deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn start(name: &str) -> (Process, UnixListener) {
    let root = std::env::temp_dir().join(format!("lom-stop-{}-{name}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("shell.kdl"),
        include_str!("../../examples/minimal/shell.kdl"),
    )
    .unwrap();
    let listener = UnixListener::bind(root.join("socket")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let log = fs::File::create(root.join("log")).unwrap();
    let child = Command::new("bwrap")
        .args([
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--ro-bind",
            "/etc",
            "/etc",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/run",
            "--unshare-all",
            "--as-pid-1",
            "--die-with-parent",
            "--clearenv",
            "--info-fd",
            "2",
            "--bind",
        ])
        .arg(&root)
        .arg(&root)
        .arg("--ro-bind")
        .arg(std::env::current_exe().unwrap())
        .arg("/lom-test")
        .args(["--setenv", "LOM_SHUTDOWN_TEST"])
        .arg(&root)
        .args(["--setenv", "SOPHIA_SHELL_9P_SOCKET"])
        .arg(root.join("socket"))
        .args(["--setenv", "SOPHIA_SHELL_CONFIG"])
        .arg(root.join("shell.kdl"))
        .args(["--setenv", "SOPHIA_SHELL_BAR_THICKNESS", "48", "--"])
        .arg("/lom-test")
        .args(["--exact", CHILD, "--ignored", "--nocapture"])
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    (Process { child, root }, listener)
}
fn accept(listener: &UnixListener) -> std::os::unix::net::UnixStream {
    let mut stream = None;
    wait_until(|| match listener.accept() {
        Ok((value, _)) => {
            stream = Some(value);
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(e) => panic!("accept: {e}"),
    });
    stream.unwrap()
}
fn stop(process: &mut Process, signal: &str) -> Duration {
    let log = fs::read_to_string(process.root.join("log")).unwrap();
    let pid: u32 = log
        .lines()
        .find_map(|line| line.trim().strip_prefix("\"child-pid\":"))
        .expect(&log)
        .trim()
        .trim_end_matches(',')
        .parse()
        .unwrap();
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    assert_eq!(
        status
            .lines()
            .find(|l| l.starts_with("NSpid:"))
            .unwrap()
            .split_whitespace()
            .last(),
        Some("1")
    );
    let started = Instant::now();
    assert!(
        Command::new("kill")
            .args([signal, &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    wait_until(|| process.child.try_wait().unwrap().is_some());
    assert!(
        process.child.wait().unwrap().success(),
        "{}",
        fs::read_to_string(process.root.join("log")).unwrap()
    );
    assert!(process.root.join("returned").exists());
    assert!(
        fs::read_to_string(process.root.join("log"))
            .unwrap()
            .contains("lom_shell_shutdown schema=1 reason=signal")
    );
    started.elapsed()
}

#[test]
fn sigterm_during_negotiation_is_bounded_and_does_not_start_renderer() {
    let (mut process, listener) = start("handshake");
    let mut stream = accept(&listener);
    assert!(stop(&mut process, "-TERM") < Duration::from_secs(6));
    assert!(!process.root.join("renderer").exists());
    // Drain the initial request and prove the connection closed, without retry.
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut bytes = Vec::new();
    match stream.read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset),
    }
}

#[test]
fn sigterm_cancels_initial_facts_wait_and_drops_renderer() {
    let (mut process, listener) = start("facts");
    let stream = accept(&listener);
    let mut closed = stream.try_clone().unwrap();
    let peer = files_peer::Peer::new(stream, capabilities());
    wait_until(|| process.root.join("renderer").exists());
    assert!(stop(&mut process, "-TERM") < Duration::from_secs(2));
    assert!(process.root.join("dropped").exists());
    drop(peer);
    closed.set_nonblocking(false).unwrap();
    closed
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut bytes = Vec::new();
    match closed.read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset),
    }
}

fn capabilities() -> u64 {
    SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
        | SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION
}

#[test]
fn sigint_during_rendering_closes_service_and_drops_renderer() {
    stop_during_rendering("-INT", "render-int");
}

#[test]
fn sigterm_during_rendering_closes_service_and_drops_renderer() {
    stop_during_rendering("-TERM", "render-term");
}

fn stop_during_rendering(signal: &str, name: &str) {
    let (mut process, listener) = start(name);
    let peer = files_peer::Peer::new(accept(&listener), capabilities());
    peer.content(
        TransactionId::from_raw(2),
        ShellContentRecord::OutputFacts(ContentOutputFacts {
            grant: GRANT,
            facts_generation: 4,
            outputs: vec![ContentOutputFactsEntry {
                output: OUTPUT,
                local_width: 4,
                local_height: 32,
                scale_numerator: 2,
                scale_denominator: 1,
                scale_generation: 5,
            }],
        }),
    );
    let files_peer::Message::Content(tx, record) = peer.receive() else {
        panic!("allocation request");
    };
    let ShellContentRecord::AllocationRequest(request) = *record else {
        panic!("allocation request");
    };
    peer.content(
        tx,
        ShellContentRecord::AllocationResult(ContentAllocationResult {
            grant: GRANT,
            allocation_request_id: request.allocation_request_id,
            status: 1,
            reason: 0,
            output: OUTPUT,
            allocation: ContentAllocationId {
                id: 11,
                generation: 1,
            },
            parent: ContentAllocationId::default(),
            logical: ContentLogicalRect {
                x: 0,
                y: 0,
                width: 4,
                height: 24,
            },
            pixel: ContentPixelRect {
                x: 0,
                y: 0,
                width: 8,
                height: 48,
            },
            scale_numerator: 2,
            scale_denominator: 1,
            scale_generation: 5,
            allowed_reservation_extent: 48,
            margins: ContentMargins::default(),
            acknowledged_anchor: ContentPixelRect::default(),
        }),
    );
    wait_until(|| process.root.join("rendering").exists());
    assert!(stop(&mut process, signal) < Duration::from_secs(2));
    assert!(process.root.join("dropped").exists());
    drop(peer);
}
