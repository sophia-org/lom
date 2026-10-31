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

use crate::service::tests::{files_peer, files_wire};
#[path = "checksum_entry.rs"]
mod checksum_entry;
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
    run_with_renderer(Entry::Ordinary, None, None, |_, _, _, _| {
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
                "child status: {:?}; child log: {}",
                self.child.try_wait(),
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
/// The host's merged-usr link, so the ELF interpreter path resolves as it
/// does outside: Ubuntu links /lib64 to usr/lib64, Void and Arch to usr/lib.
fn host_link(path: &str, merged: &str) -> PathBuf {
    fs::read_link(path).unwrap_or_else(|_| merged.into())
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
        .args(["--ro-bind", "/usr", "/usr", "--symlink"])
        .arg(host_link("/bin", "usr/bin"))
        .args(["/bin", "--symlink"])
        .arg(host_link("/lib", "usr/lib"))
        .args(["/lib", "--symlink"])
        .arg(host_link("/lib64", "usr/lib"))
        .args([
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
        | sophia_shell_client::IMAGE_CAPABILITY_OWNED_RING
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

/// Renderer selection: the panel setting, its argument overrides, and the
/// CPU and DMA-BUF refusal, decided before any connection or renderer.
#[test]
fn an_explicit_argument_overrides_the_panel_renderer_and_image_source() {
    use crate::config::Renderer::{Cpu, Gpu};
    let (mut config, _) =
        crate::config::parse_shell_config(include_str!("../../examples/minimal/shell.kdl"))
            .unwrap();
    for (panel, panel_dma, renderer, dma, expected) in [
        (Gpu, false, None, None, Some((false, Gpu))),
        (Cpu, false, None, None, Some((false, Cpu))),
        (Cpu, false, Some(Gpu), None, Some((false, Gpu))),
        (Gpu, false, Some(Cpu), None, Some((false, Cpu))),
        (Gpu, true, None, None, Some((true, Gpu))),
        (Cpu, true, None, None, None),
        (Cpu, true, Some(Gpu), None, Some((true, Gpu))),
        (Cpu, true, None, Some(false), Some((false, Cpu))),
        (Gpu, false, Some(Cpu), Some(true), None),
    ] {
        (config.renderer, config.dma_buf) = (panel, panel_dma);
        let resolved = resolve(Entry::Ordinary, dma, renderer, &config);
        assert_eq!(
            resolved.as_ref().ok(),
            expected.as_ref(),
            "{panel:?} {panel_dma} {renderer:?} {dma:?}: {resolved:?}"
        );
        if expected.is_none() {
            assert!(
                resolved
                    .unwrap_err()
                    .contains("requires the owned-ring image source")
            );
        }
    }
}

/// The resolved CPU renderer starts without the GPU grant the GPU renderer
/// requires in the same environment.
#[test]
fn the_cpu_renderer_starts_without_the_grant_the_gpu_renderer_requires() {
    use crate::config::Renderer::{Cpu, Gpu};
    assert!(
        GpuGrant::from_environment(1).is_err(),
        "this control needs an environment without a GPU grant"
    );
    let stop = std::sync::atomic::AtomicBool::new(false);
    let (wake, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let worker = start_renderer(Entry::Ordinary, 1, false, Cpu, &stop, wake.as_fd()).unwrap();
    assert!(
        worker.uses_ring_fill(),
        "the CPU worker fills owned ring slots"
    );
    assert!(start_renderer(Entry::Ordinary, 1, false, Gpu, &stop, wake.as_fd()).is_err());
}

/// Entry admission after the argument and panel merge: the ordinary binary
/// refuses the copy reference, the diagnostic binary refuses GPU, and CPU
/// with DMA-BUF stays refused for both. All of it precedes any connection.
#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn each_entry_admits_only_its_resolved_renderers() {
    use crate::config::Renderer::{Cpu, CpuCopyReference as Copy, Gpu};
    use Entry::{CpuCadence as Cadence, CpuReference as Reference, Ordinary};
    let ordinary = "the cpu-copy-reference renderer requires lom-cpu-reference";
    let dma = "requires the owned-ring image source";
    let (mut config, _) =
        crate::config::parse_shell_config(include_str!("../../examples/minimal/shell.kdl"))
            .unwrap();
    for (entry, panel, panel_dma, renderer, expected) in [
        (Ordinary, Copy, false, None, Err(ordinary)),
        (Ordinary, Cpu, false, Some(Copy), Err(ordinary)),
        (Ordinary, Copy, false, Some(Cpu), Ok(Cpu)),
        (Ordinary, Copy, false, Some(Gpu), Ok(Gpu)),
        (Ordinary, Copy, true, None, Err(dma)),
        (Reference, Gpu, false, None, Err(REFUSE_GPU)),
        (Reference, Cpu, false, Some(Gpu), Err(REFUSE_GPU)),
        (Reference, Gpu, true, None, Err(REFUSE_GPU)),
        (Reference, Cpu, false, None, Ok(Cpu)),
        (Reference, Copy, false, None, Ok(Copy)),
        (Reference, Gpu, false, Some(Cpu), Ok(Cpu)),
        (Reference, Gpu, false, Some(Copy), Ok(Copy)),
        (Reference, Cpu, true, None, Err(dma)),
        (Reference, Copy, true, None, Err(dma)),
        (Cadence, Gpu, false, None, Err(REFUSE_GPU_CADENCE)),
        (Cadence, Cpu, false, Some(Gpu), Err(REFUSE_GPU_CADENCE)),
        (Cadence, Cpu, false, None, Ok(Cpu)),
        (Cadence, Copy, false, None, Ok(Copy)),
        (Cadence, Gpu, false, Some(Copy), Ok(Copy)),
        (Cadence, Cpu, true, None, Err(dma)),
        (Cadence, Copy, true, None, Err(dma)),
    ] {
        (config.renderer, config.dma_buf) = (panel, panel_dma);
        let resolved = resolve(entry, None, renderer, &config);
        match expected {
            Ok(renderer) => assert_eq!(resolved, Ok((false, renderer)), "{entry:?} {panel:?}"),
            Err(refusal) => assert!(
                resolved
                    .as_ref()
                    .is_err_and(|error| error.contains(refusal)),
                "{entry:?} {panel:?} {panel_dma} {renderer:?}: {resolved:?}"
            ),
        }
    }
}

/// One diagnostic binary starts both CPU arms, without a GPU grant, with its
/// exact records; the ordinary entry starts neither the copy nor a fallback.
#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn the_diagnostic_entry_starts_both_cpu_arms_with_exact_records() {
    use crate::cli::cpu_reference::{cadence_record, record};
    use crate::config::Renderer::{Cpu, CpuCopyReference, Gpu};
    assert!(GpuGrant::from_environment(1).is_err());
    assert_eq!(
        record(Cpu).unwrap(),
        "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used diagnostic=cpu-reference"
    );
    assert_eq!(
        record(CpuCopyReference).unwrap(),
        "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-copy gpu=not-used diagnostic=cpu-reference"
    );
    let stop = std::sync::atomic::AtomicBool::new(false);
    let (wake, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    assert_eq!(
        cadence_record(),
        "lom_cadence schema=1 mode=redirty-after-presented quota=2120 diagnostic=cpu-reference"
    );
    let start = |entry, renderer| start_renderer(entry, 1, false, renderer, &stop, wake.as_fd());
    for entry in [Entry::CpuReference, Entry::CpuCadence] {
        assert!(start(entry, Cpu).unwrap().uses_ring_fill());
        assert!(!start(entry, CpuCopyReference).unwrap().uses_ring_fill());
    }
    for (entry, renderer) in [
        (Entry::CpuReference, Gpu),
        (Entry::CpuCadence, Gpu),
        (Entry::Ordinary, CpuCopyReference),
    ] {
        assert!(start(entry, renderer).is_err(), "{entry:?} {renderer:?}");
    }
}
