//! Protected production entry point; protocol ownership lives in `service`.

use crate::{
    config::parse_shell_config,
    render::{GpuGrant, RendererWorker},
    runtime::validate_theme,
    service::{ContentRenderer, ShellService},
};
use sophia_shell_client::{ShellClientOptions, ShellConnection};
use sophia_shell_protocol::{
    SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT, SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE,
    SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER, SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION,
    SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS,
};
use std::{path::Path, time::Duration};

pub(super) fn run() -> Result<(), String> {
    run_with_renderer(|epoch| {
        let grant = GpuGrant::from_environment(epoch)?;
        let (renderer, admission) = RendererWorker::start(grant)?;
        println!("{}", admission.record("lom")?);
        Ok(renderer)
    })
}

fn run_with_renderer<R: ContentRenderer>(
    renderer: impl FnOnce(u64) -> Result<R, String>,
) -> Result<(), String> {
    let stop = super::shutdown::Shutdown::install()?;
    let result = serve(&stop, renderer);
    if stop.requested() {
        // Dropping the connection revokes this owner's content and leaves any
        // ambiguous submission with its original owner; nothing is replayed.
        println!("lom_shell_shutdown schema=1 reason=signal");
        Ok(())
    } else {
        result
    }
}

fn serve<R: ContentRenderer>(
    stop: &super::shutdown::Shutdown,
    renderer: impl FnOnce(u64) -> Result<R, String>,
) -> Result<(), String> {
    if std::env::var_os("SOPHIA_SHELL_SOCKET").is_some() {
        return Err("SOPHIA_SHELL_SOCKET is unsupported; use SOPHIA_SHELL_9P_SOCKET".into());
    }
    let socket = std::env::var_os("SOPHIA_SHELL_9P_SOCKET")
        .filter(|path| !path.is_empty())
        .ok_or("SOPHIA_SHELL_9P_SOCKET is required for --serve")?;
    let config_path = std::env::var_os("SOPHIA_SHELL_CONFIG")
        .ok_or("SOPHIA_SHELL_CONFIG is required for --serve")?;
    let source = super::read(Path::new(&config_path))?;
    let (config, theme) = parse_shell_config(&source)
        .map_err(|error| format!("{}:{error}", Path::new(&config_path).display()))?;
    validate_theme(&config, &theme)?;
    let allowance = std::env::var("SOPHIA_SHELL_BAR_THICKNESS")
        .map_err(|_| "SOPHIA_SHELL_BAR_THICKNESS is required for --serve")?
        .parse::<u32>()
        .map_err(|_| "SOPHIA_SHELL_BAR_THICKNESS must be an integer")?;
    if allowance == 0 {
        return Err("admitted shell allowance must be positive".into());
    }
    let capabilities = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
        | SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION;
    let options = ShellClientOptions {
        minimum_revision: 6,
        maximum_revision: 6,
        required_capabilities: capabilities,
        handshake_timeout: Duration::from_secs(5),
    };
    stop.check()?;
    let connection = ShellConnection::connect_files(socket, options)
        .map_err(|error| format!("shell negotiation failed: {error}"))?;
    stop.check()?;
    println!(
        "lom_shell_transport schema=1 wire=9p2000.L revision={} epoch={}",
        connection.welcome().selected_revision,
        connection.connection_epoch()
    );
    let renderer = renderer(connection.connection_epoch())?;
    stop.check()?;
    let mut service = ShellService::new_until_stopped(
        connection,
        config,
        theme,
        allowance,
        renderer,
        stop.flag.clone(),
    )?;
    while !stop.requested() {
        if service.step()? == 0 {
            service.wait_for_work()?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/shutdown.rs"]
mod tests;
