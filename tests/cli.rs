//! CLI behavior of configuration and preview diagnostics, without a desktop or credentials.

use std::process::{Command, Output};

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

/// Serve with the minimal configuration plus `settings` in its panel, stopping
/// before any connection: no socket server or allowance is supplied.
fn serve_with_panel(settings: &str, arguments: &[&str]) -> Output {
    let source = include_str!("../examples/minimal/shell.kdl").replace(
        "panel \"main\" {",
        &format!("panel \"main\" {{\n {settings}"),
    );
    let path = std::env::temp_dir().join(format!(
        "lom-cli-renderer-{}-{}.kdl",
        std::process::id(),
        arguments.join("_").replace('-', "")
    ));
    std::fs::write(&path, source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_lom"))
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
