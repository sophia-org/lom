//! Command-line diagnostics, explicitly separated from native shell startup.

mod authentication;
mod content_proof;
#[cfg(feature = "diagnostic-cpu-reference")]
mod cpu_reference;
mod preview;
mod serve;
mod shutdown;
mod work_statistics;

#[cfg(all(test, feature = "diagnostic-cpu-reference"))]
pub(crate) use serve::drive_for_tests;

use crate::{
    config::{PanelConfig, Renderer, Theme, parse_config, parse_theme},
    runtime,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Help displayed without loading configuration, fonts or a GPU.
pub const HELP: &str = "Lom — native Sophia shell components\n\nUsage: lom [--help | --version]\n       lom check-config --config FILE --theme FILE\n       lom preview --config FILE --theme FILE --fixture FILE --output NEW_DIRECTORY [--width 1280] [--scale 1]\n       lom content-proof --socket FILE\n       lom --serve [--image-source owned-ring|dma-buf] [--renderer gpu|cpu]\n\npreview explicitly initializes a Vulkan GPU and writes panel/calendar PNGs.\ncontent-proof exercises fixed diagnostic pixels over an explicitly admitted Sophia 9P socket; it does not present a panel.\n--serve is the persistent protected Sophia content-shell entry point. The panel's renderer setting, or an overriding --renderer, selects gpu (default) or cpu; cpu renders with Vello CPU into owned-ring slots without a GPU grant and is never a fallback.\n";

/// Run one diagnostic command; the caller prints any returned boundary failure.
pub fn run(arguments: Vec<OsString>) -> Result<(), String> {
    if arguments.len() == 1 && (arguments[0] == "--help" || arguments[0] == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if arguments.len() == 1 && (arguments[0] == "--version" || arguments[0] == "-V") {
        println!("lom {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let Some(command) = arguments.first().and_then(|v| v.to_str()) else {
        return Err("no command supplied; use --help".into());
    };
    let allowed = match command {
        "check-config" => &["--config", "--theme"][..],
        "content-proof" => &["--socket"][..],
        "--serve" => &["--image-source", "--renderer"][..],
        "preview" => &[
            "--config",
            "--theme",
            "--fixture",
            "--output",
            "--width",
            "--scale",
        ][..],
        _ => return Err("unsupported arguments; use --help".into()),
    };
    let options = options(&arguments[1..], allowed)?;
    if command == "content-proof" {
        return content_proof::run(required(&options, "--socket")?);
    }
    if command == "--serve" {
        let dma = match options.get("--image-source") {
            None => None,
            Some(value) if value == "owned-ring" => Some(false),
            Some(value) if value == "dma-buf" => Some(true),
            _ => return Err("--image-source must be owned-ring or dma-buf".into()),
        };
        let renderer = match options.get("--renderer") {
            None => None,
            Some(value) if value == "gpu" => Some(Renderer::Gpu),
            Some(value) if value == "cpu" => Some(Renderer::Cpu),
            _ => return Err("--renderer must be gpu or cpu".into()),
        };
        return serve::run(serve::Entry::Ordinary, dma, renderer);
    }
    let (config, theme) = load(
        &required(&options, "--config")?,
        &required(&options, "--theme")?,
    )?;
    if command == "check-config" {
        println!("configuration valid: {}", config.name);
        for status in runtime::availability(&config) {
            println!("{status}");
        }
        println!("native presentation: unavailable; no live connection attempted");
        Ok(())
    } else {
        preview::run(config, theme, options)
    }
}

/// Diagnostic `lom-cpu-reference` entry. It accepts exactly `--serve`; the
/// protected panel's `renderer` selects `cpu` or `cpu-copy-reference`.
#[cfg(feature = "diagnostic-cpu-reference")]
pub fn run_cpu_reference(arguments: Vec<OsString>) -> Result<(), String> {
    if arguments.len() != 1 || arguments[0] != "--serve" {
        return Err("lom-cpu-reference accepts exactly --serve".into());
    }
    serve::run(serve::Entry::CpuReference, None, None)
}

/// Diagnostic `lom-cpu-cadence` entry: the `lom-cpu-reference` arms, redirtied
/// after each exact Presented to a fixed quota. It accepts exactly `--serve`.
#[cfg(feature = "diagnostic-cpu-reference")]
pub fn run_cpu_cadence(arguments: Vec<OsString>) -> Result<(), String> {
    if arguments.len() != 1 || arguments[0] != "--serve" {
        return Err("lom-cpu-cadence accepts exactly --serve".into());
    }
    serve::run(serve::Entry::CpuCadence, None, None)
}

/// Fixed diagnostic cadence with ordinary CPU direct fill and no checksum.
/// Accepts exactly `--serve`; GPU and copy-reference selections are refused.
#[cfg(feature = "diagnostic-cpu-reference")]
pub fn run_cpu_cadence_no_checksum(arguments: Vec<OsString>) -> Result<(), String> {
    if arguments.len() != 1 || arguments[0] != "--serve" {
        return Err("lom-cpu-cadence-no-checksum accepts exactly --serve".into());
    }
    serve::run(serve::Entry::CpuCadenceNoChecksum, None, None)
}

fn options(arguments: &[OsString], allowed: &[&str]) -> Result<BTreeMap<String, OsString>, String> {
    if !arguments.len().is_multiple_of(2) {
        return Err("unsupported arguments: expected flag/value pairs".into());
    }
    let mut options = BTreeMap::new();
    for pair in arguments.chunks_exact(2) {
        let key = pair[0].to_str().ok_or("option name must be UTF-8")?;
        if !allowed.contains(&key) || options.insert(key.to_owned(), pair[1].clone()).is_some() {
            return Err(format!(
                "unsupported arguments: unknown or duplicate option {key}"
            ));
        }
    }
    Ok(options)
}
fn required(options: &BTreeMap<String, OsString>, key: &str) -> Result<PathBuf, String> {
    options
        .get(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {key}"))
}
fn load(config: &Path, theme: &Path) -> Result<(PanelConfig, Theme), String> {
    let config = parse_config(&read(config)?).map_err(|e| format!("{}:{e}", config.display()))?;
    let theme = parse_theme(&read(theme)?).map_err(|e| format!("{}:{e}", theme.display()))?;
    runtime::validate_theme(&config, &theme)?;
    Ok((config, theme))
}
fn read(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > 256 * 1024 {
        return Err(format!("{}: document exceeds 256 KiB", path.display()));
    }
    String::from_utf8(bytes).map_err(|_| format!("{}: expected UTF-8", path.display()))
}
