//! Command-line diagnostics, explicitly separated from native shell startup.

mod content_proof;
mod preview;

use crate::{
    config::{PanelConfig, Theme, parse_config, parse_theme},
    runtime,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Help displayed without loading configuration, fonts or a GPU.
pub const HELP: &str = "Lom — native Sophia shell components\n\nUsage: lom [--help | --version]\n       lom check-config --config FILE --theme FILE\n       lom preview --config FILE --theme FILE --fixture FILE --output NEW_DIRECTORY [--width 1280] [--scale 1]\n       lom content-proof --socket FILE\n\npreview explicitly initializes a Vulkan GPU and writes panel/calendar PNGs.\ncontent-proof exercises fixed diagnostic pixels over an explicitly admitted Sophia socket; it does not present a panel.\nNative shell startup is not implemented yet.\n";

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
        return Err("native shell startup is not implemented yet; use --help".into());
    };
    let allowed = match command {
        "check-config" => &["--config", "--theme"][..],
        "content-proof" => &["--socket"][..],
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
