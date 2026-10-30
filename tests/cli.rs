//! CLI behavior of configuration and preview diagnostics, without a desktop or credentials.

use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static PANEL: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "diagnostic-cpu-reference")]
const REFERENCE: &str = env!("CARGO_BIN_EXE_lom-cpu-reference");
#[cfg(feature = "diagnostic-cpu-reference")]
const CADENCE: &str = env!("CARGO_BIN_EXE_lom-cpu-cadence");

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lom"))
        .args(arguments)
        .env_clear()
        .output()
        .expect("the built Lom CLI should run without a session environment")
}

#[test]
fn help_succeeds_and_names_the_protected_service() {
    for flag in ["--help", "-h"] {
        let result = run(&[flag]);
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        let text = String::from_utf8(result.stdout).expect("help is UTF-8");
        assert!(text.contains("Usage: lom"));
        assert!(text.contains("--serve is the persistent protected Sophia content-shell"));
    }
}

#[test]
fn version_matches_the_package() {
    for flag in ["--version", "-V"] {
        let result = run(&[flag]);
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        assert_eq!(
            result.stdout,
            format!("lom {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
        );
    }
}

#[test]
fn default_startup_requires_an_explicit_command() {
    let result = run(&[]);
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    let text = String::from_utf8(result.stderr).expect("diagnostic is UTF-8");
    assert!(text.contains("no command supplied"));
}

#[test]
fn protected_service_requires_supervisor_supplied_inputs_before_gpu_setup() {
    for arguments in [
        &["--serve"][..],
        &["--serve", "--image-source", "owned-ring"],
        &["--serve", "--image-source", "dma-buf"],
    ] {
        let result = run(arguments);
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        let text = String::from_utf8(result.stderr).expect("diagnostic is UTF-8");
        assert!(text.contains("SOPHIA_SHELL_9P_SOCKET is required"));
    }
}

#[test]
fn invalid_image_source_fails_before_service_startup() {
    let result = run(&["--serve", "--image-source", "automatic"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("--image-source must be owned-ring or dma-buf")
    );
}

#[test]
fn protected_service_refuses_retired_transport_before_loading_config() {
    for retired in ["", "/retired"] {
        for files in [None, Some("/files")] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_lom"));
            command
                .arg("--serve")
                .env_clear()
                .env("SOPHIA_SHELL_SOCKET", retired);
            if let Some(files) = files {
                command.env("SOPHIA_SHELL_9P_SOCKET", files);
            }
            let result = command.output().unwrap();
            assert_eq!(result.status.code(), Some(2));
            assert!(
                String::from_utf8(result.stderr)
                    .unwrap()
                    .contains("SOPHIA_SHELL_SOCKET is unsupported")
            );
        }
    }
}

#[test]
fn unknown_or_extra_arguments_are_rejected() {
    for arguments in [
        &["--unknown"][..],
        &["--help", "extra"],
        &["--version", "extra"],
    ] {
        let result = run(arguments);
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8(result.stderr)
                .expect("diagnostic is UTF-8")
                .contains("unsupported arguments")
        );
    }
}

#[test]
fn configuration_check_reports_real_availability_without_a_session() {
    let result = run(&[
        "check-config",
        "--config",
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/minimal/config.kdl"),
        "--theme",
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/minimal/theme.kdl"),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("configuration valid: main"));
    assert!(text.contains("fixture presentation only"));
    assert!(text.contains("live Sophia adapter pending"));
    assert!(text.contains("no live connection attempted"));
}

#[test]
fn invalid_preview_arguments_fail_before_any_gpu_initialization() {
    let result = run(&[
        "preview",
        "--config",
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/minimal/config.kdl"),
        "--theme",
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/minimal/theme.kdl"),
        "--fixture",
        "does-not-exist",
        "--output",
        "must-not-be-created",
        "--width",
        "0",
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("preview width")
    );
    assert!(!std::path::Path::new("must-not-be-created").exists());
}

#[test]
fn missing_duplicate_and_malformed_config_options_fail_closed() {
    for arguments in [
        &["check-config"][..],
        &["check-config", "--config", "missing", "--config", "second"],
        &["check-config", "--config"],
        &["preview", "--display", ":77"],
    ] {
        assert_eq!(run(arguments).status.code(), Some(2));
    }
}

#[test]
fn invalid_renderer_fails_before_service_startup() {
    let result = run(&["--serve", "--renderer", "automatic"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("--renderer must be gpu or cpu")
    );
}

#[test]
fn cpu_rendering_refuses_the_dma_buf_source_instead_of_falling_back() {
    let result = Command::new(env!("CARGO_BIN_EXE_lom"))
        .args(["--serve", "--renderer", "cpu", "--image-source", "dma-buf"])
        .env_clear()
        .env("SOPHIA_SHELL_9P_SOCKET", "/nonexistent/lom.sock")
        .env(
            "SOPHIA_SHELL_CONFIG",
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/minimal/shell.kdl"),
        )
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty(), "no renderer or transport started");
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("the CPU renderer requires the owned-ring image source")
    );
}

/// A protected configuration: the minimal one plus `settings` in its panel.
fn panel_config(settings: &str) -> std::path::PathBuf {
    let source = include_str!("../examples/minimal/shell.kdl").replace(
        "panel \"main\" {",
        &format!("panel \"main\" {{\n {settings}"),
    );
    let path = std::env::temp_dir().join(format!(
        "lom-cli-renderer-{}-{}.kdl",
        std::process::id(),
        PANEL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, source).unwrap();
    path
}

/// Serve with the minimal configuration plus `settings` in its panel, stopping
/// before any connection: no socket server or allowance is supplied.
fn serve_with_panel(settings: &str, arguments: &[&str]) -> Output {
    serve_binary_with_panel(env!("CARGO_BIN_EXE_lom"), settings, arguments)
}

fn serve_binary_with_panel(binary: &str, settings: &str, arguments: &[&str]) -> Output {
    let path = panel_config(settings);
    let result = Command::new(binary)
        .arg("--serve")
        .args(arguments)
        .env_clear()
        .env("SOPHIA_SHELL_9P_SOCKET", "/nonexistent/lom.sock")
        .env("SOPHIA_SHELL_CONFIG", &path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    result
}

#[test]
fn the_panel_renderer_is_selected_by_config_and_overridden_by_arguments() {
    // Passing resolution reaches the next supervisor input; a CPU and DMA-BUF
    // selection stops first. Which one occurs proves the resolved renderer.
    let passed = "SOPHIA_SHELL_BAR_THICKNESS is required";
    let refused = "the CPU renderer requires the owned-ring image source";
    let both = "renderer \"cpu\"\n image-source \"dma-buf\"";
    for (settings, arguments, expected) in [
        ("renderer \"cpu\"", &[][..], passed),
        (both, &[], refused),
        (both, &["--renderer", "gpu"], passed),
        (both, &["--image-source", "owned-ring"], passed),
        (
            "renderer \"gpu\"",
            &["--renderer", "cpu", "--image-source", "dma-buf"],
            refused,
        ),
        ("image-source \"dma-buf\"", &[], passed),
        ("image-source \"dma-buf\"", &["--renderer", "cpu"], refused),
    ] {
        let result = serve_with_panel(settings, arguments);
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(2), "{settings} {arguments:?}");
        assert!(result.stdout.is_empty(), "no renderer or transport started");
        assert!(
            stderr.contains(expected),
            "{settings} {arguments:?}: {stderr}"
        );
    }
}

#[test]
fn cpu_rendering_needs_the_same_supervisor_inputs_before_any_renderer() {
    for arguments in [
        &["--serve", "--renderer", "cpu"][..],
        &["--serve", "--renderer", "gpu"],
    ] {
        let result = run(arguments);
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8(result.stderr)
                .unwrap()
                .contains("SOPHIA_SHELL_9P_SOCKET is required")
        );
    }
}

#[test]
fn production_help_and_renderer_spellings_are_unchanged() {
    let result = run(&["--help"]);
    assert_eq!(String::from_utf8(result.stdout).unwrap(), lom::cli::HELP);
    assert!(!lom::cli::HELP.contains("reference"));
    let result = run(&["--serve", "--renderer", "cpu-copy-reference"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("--renderer must be gpu or cpu")
    );
}

#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn the_diagnostic_binaries_accept_exactly_serve() {
    for (binary, refusal, arguments) in [
        (REFERENCE, "lom-cpu-reference"),
        (CADENCE, "lom-cpu-cadence"),
    ]
    .into_iter()
    .flat_map(|(binary, name)| {
        [
            &[][..],
            &["--help"],
            &["--version"],
            &["--serve", "--serve"],
            &["--serve", "--renderer", "cpu"],
            &["--serve", "--image-source", "owned-ring"],
            &[
                "check-config",
                "--config",
                "panel.kdl",
                "--theme",
                "theme.kdl",
            ],
        ]
        .map(move |arguments| (binary, name, arguments))
    }) {
        let result = Command::new(binary)
            .args(arguments)
            .env_clear()
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "{arguments:?}");
        assert!(result.stdout.is_empty());
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(
            stderr.contains(&format!("{refusal} accepts exactly --serve")),
            "{arguments:?}: {stderr}"
        );
    }
}

#[cfg(feature = "diagnostic-cpu-reference")]
const ORDINARY_REFUSAL: &str = "the cpu-copy-reference renderer requires lom-cpu-reference";
#[cfg(feature = "diagnostic-cpu-reference")]
const GPU_REFUSAL: &str = "lom-cpu-reference refuses the GPU renderer";
#[cfg(feature = "diagnostic-cpu-reference")]
const CADENCE_GPU_REFUSAL: &str = "lom-cpu-cadence refuses the GPU renderer";
#[cfg(feature = "diagnostic-cpu-reference")]
const WORK_STATS_REFUSAL: &str = "lom-cpu-cadence refuses SOPHIA_SHELL_WORK_STATS";
#[cfg(feature = "diagnostic-cpu-reference")]
const DMA_REFUSAL: &str = "the CPU renderer requires the owned-ring image source";
#[cfg(feature = "diagnostic-cpu-reference")]
const COPY: &str = "renderer \"cpu-copy-reference\"";

#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn entry_selection_is_refused_before_the_allowance() {
    // An admitted selection reaches the next supervisor input; a refused one
    // stops first, so the error shows where the selection was decided.
    let passed = "SOPHIA_SHELL_BAR_THICKNESS is required";
    let lom = env!("CARGO_BIN_EXE_lom");
    let cpu_dma = "renderer \"cpu\"\n image-source \"dma-buf\"";
    let copy_dma = "renderer \"cpu-copy-reference\"\n image-source \"dma-buf\"";
    for (binary, settings, arguments, expected) in [
        (lom, COPY, &[][..], ORDINARY_REFUSAL),
        (lom, COPY, &["--renderer", "cpu"], passed),
        (lom, COPY, &["--renderer", "gpu"], passed),
        (lom, "renderer \"cpu\"", &[], passed),
        (REFERENCE, "", &[], GPU_REFUSAL),
        (REFERENCE, "renderer \"gpu\"", &[], GPU_REFUSAL),
        (REFERENCE, "renderer \"cpu\"", &[], passed),
        (REFERENCE, COPY, &[], passed),
        (REFERENCE, cpu_dma, &[], DMA_REFUSAL),
        (REFERENCE, copy_dma, &[], DMA_REFUSAL),
        (CADENCE, "", &[], CADENCE_GPU_REFUSAL),
        (CADENCE, "renderer \"gpu\"", &[], CADENCE_GPU_REFUSAL),
        (CADENCE, "renderer \"cpu\"", &[], passed),
        (CADENCE, COPY, &[], passed),
        (CADENCE, cpu_dma, &[], DMA_REFUSAL),
        (CADENCE, copy_dma, &[], DMA_REFUSAL),
    ] {
        let result = serve_binary_with_panel(binary, settings, arguments);
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(2), "{settings} {arguments:?}");
        assert!(result.stdout.is_empty(), "no renderer or transport started");
        assert!(
            stderr.contains(expected),
            "{binary} {settings} {arguments:?}: {stderr}"
        );
    }
}

/// With every supervisor input present, a refused selection never connects to
/// the listening shell socket; an admitted selection does (the control).
#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn a_refused_entry_selection_never_connects() {
    use std::os::unix::net::UnixListener;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    // A fresh directory per run: a reused PID never meets a stale socket, and
    // only this fixture's own directory is removed.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("lom-cli-connect-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let socket = directory.join("shell.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let serve = |binary: &str, settings: &str, work_stats: bool| {
        let config = panel_config(settings);
        let mut command = Command::new(binary);
        command.env_clear();
        if work_stats {
            // Any value is refused, even one that would disable the counters.
            command.env("SOPHIA_SHELL_WORK_STATS", "0");
        }
        let child = command
            .arg("--serve")
            .env("SOPHIA_SHELL_9P_SOCKET", &socket)
            .env("SOPHIA_SHELL_CONFIG", &config)
            .env("SOPHIA_SHELL_BAR_THICKNESS", "24")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        (child, config)
    };
    let cpu_dma = "renderer \"cpu\"\n image-source \"dma-buf\"";
    for (binary, settings, work_stats, refusal) in [
        (env!("CARGO_BIN_EXE_lom"), COPY, false, ORDINARY_REFUSAL),
        (REFERENCE, "renderer \"gpu\"", false, GPU_REFUSAL),
        (REFERENCE, cpu_dma, false, DMA_REFUSAL),
        (CADENCE, "renderer \"gpu\"", false, CADENCE_GPU_REFUSAL),
        (CADENCE, cpu_dma, false, DMA_REFUSAL),
        (CADENCE, COPY, true, WORK_STATS_REFUSAL),
    ] {
        let (child, config) = serve(binary, settings, work_stats);
        let result = child.wait_with_output().unwrap();
        std::fs::remove_file(config).unwrap();
        assert_eq!(result.status.code(), Some(2), "{settings}");
        assert!(result.stdout.is_empty(), "no transport or renderer record");
        assert!(String::from_utf8(result.stderr).unwrap().contains(refusal));
        let error = listener.accept().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock, "{settings}");
    }
    let (child, config) = serve(REFERENCE, COPY, false);
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "the control never connected");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("{error}"),
        }
    };
    drop(stream);
    let result = child.wait_with_output().unwrap();
    std::fs::remove_file(config).unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        !result.status.success(),
        "the dropped peer ends the control"
    );
    assert!(result.stdout.is_empty(), "no transport was negotiated");
}

/// `check-config` stays binary-neutral: with the feature, the ordinary binary
/// validates a copy-reference panel that its `--serve` refuses; without it,
/// the same document is refused.
#[test]
fn check_config_parses_the_copy_reference_only_with_its_feature() {
    let source = include_str!("../examples/minimal/config.kdl").replace(
        "panel \"main\" {",
        "panel \"main\" {\n renderer \"cpu-copy-reference\"",
    );
    let path = std::env::temp_dir().join(format!(
        "lom-cli-check-copy-{}-{}.kdl",
        std::process::id(),
        PANEL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, source).unwrap();
    let result = run(&[
        "check-config",
        "--config",
        path.to_str().unwrap(),
        "--theme",
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/minimal/theme.kdl"),
    ]);
    std::fs::remove_file(path).unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    #[cfg(feature = "diagnostic-cpu-reference")]
    {
        assert!(result.status.success(), "{stderr}");
        assert!(
            String::from_utf8(result.stdout)
                .unwrap()
                .contains("configuration valid: main")
        );
    }
    #[cfg(not(feature = "diagnostic-cpu-reference"))]
    {
        assert_eq!(result.status.code(), Some(2));
        assert!(stderr.contains("expected gpu or cpu"), "{stderr}");
    }
}
