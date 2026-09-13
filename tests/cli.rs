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
    let result = run(&["--serve"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    let text = String::from_utf8(result.stderr).expect("diagnostic is UTF-8");
    assert!(text.contains("SOPHIA_SHELL_SOCKET is required"));
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
