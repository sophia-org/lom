//! CLI behavior of the scaffold, without a desktop or credentials.

use std::process::{Command, Output};

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lom"))
        .args(arguments)
        .env_clear()
        .output()
        .expect("the built Lom CLI should run without a session environment")
}

#[test]
fn help_succeeds_and_states_the_scaffold_limit() {
    for flag in ["--help", "-h"] {
        let result = run(&[flag]);
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        let text = String::from_utf8(result.stdout).expect("help is UTF-8");
        assert!(text.contains("Usage: lom"));
        assert!(text.contains("Native shell startup is not implemented yet."));
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
fn default_startup_refuses_to_claim_a_running_shell() {
    let result = run(&[]);
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    let text = String::from_utf8(result.stderr).expect("diagnostic is UTF-8");
    assert!(text.contains("native shell startup is not implemented"));
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
