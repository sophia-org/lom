//! Fixture entries admit only their fixed CPU configuration before connecting.
#[cfg(feature = "diagnostic-cpu-reference")]
use std::process::Command;

#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn region_entries_accept_only_serve_and_the_fixed_configuration() {
    let root = std::env::temp_dir().join(format!("lom-region-cli-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let config = root.join("panel.kdl");
    let source = include_str!("fixtures/redraw/static24.kdl").replace(
        "    start {",
        "    center { clock \"minutes\" { format \"%H:%M\"; }; }\n    end { clock \"clock\" { format \"%H:%M:%S\"; }; }\n    start {",
    );
    for binary in [
        env!("CARGO_BIN_EXE_lom-cpu-regions"),
        env!("CARGO_BIN_EXE_lom-cpu-regions-full"),
        env!("CARGO_BIN_EXE_lom-cpu-region-cadence"),
        env!("CARGO_BIN_EXE_lom-cpu-region-cadence-full"),
    ] {
        for args in [
            &[][..],
            &["--help"],
            &["--serve", "--renderer", "cpu"],
            &["--serve", "extra"],
        ] {
            let result = Command::new(binary)
                .args(args)
                .env_clear()
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(2));
            assert!(result.stdout.is_empty());
            assert!(
                String::from_utf8(result.stderr)
                    .unwrap()
                    .contains("accepts exactly --serve")
            );
        }
        for (text, stats, refusal) in [
            (
                source.clone(),
                false,
                "SOPHIA_SHELL_BAR_THICKNESS is required",
            ),
            (
                source.clone(),
                true,
                "region sequence refuses SOPHIA_SHELL_WORK_STATS",
            ),
            (
                source.replace("renderer \"cpu\"", "renderer \"gpu\""),
                false,
                "requires the cpu renderer",
            ),
            (
                source.replace("renderer \"cpu\"", "renderer \"cpu-copy-reference\""),
                false,
                "requires the cpu renderer",
            ),
            (
                source.replace("height 24", "height 32"),
                false,
                "fixed CPU top-panel",
            ),
            (
                source.replace("%H:%M:%S", "%S"),
                false,
                "fixed CPU top-panel",
            ),
            (
                source.replace("position \"top\"", "position \"left\""),
                false,
                "fixed CPU top-panel",
            ),
            (
                source.replace(
                    "renderer \"cpu\"",
                    "renderer \"cpu\"\n image-source \"dma-buf\"",
                ),
                false,
                "requires the owned-ring",
            ),
        ] {
            std::fs::write(&config, text).unwrap();
            let mut command = Command::new(binary);
            command
                .arg("--serve")
                .env_clear()
                .env("SOPHIA_SHELL_CONFIG", &config)
                .env("SOPHIA_SHELL_9P_SOCKET", root.join("not-created.sock"));
            if stats {
                command.env("SOPHIA_SHELL_WORK_STATS", "0");
            }
            let result = command.output().unwrap();
            assert_eq!(result.status.code(), Some(2));
            assert!(result.stdout.is_empty());
            let error = String::from_utf8(result.stderr).unwrap();
            assert!(error.contains(refusal), "{error}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
