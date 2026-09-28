//! Protected production entry point; protocol ownership lives in `service`.

use crate::{
    config::parse_shell_config,
    render::{GpuGrant, RendererWorker},
    runtime::validate_theme,
    service::ShellService,
};
use sophia_shell_client::{ShellClientOptions, ShellConnection};
use sophia_shell_protocol::{
    SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT, SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE,
    SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER, SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION,
    SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS,
};
use std::{path::Path, time::Duration};

pub(super) fn run() -> Result<(), String> {
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
    let connection = ShellConnection::connect_files(socket, options)
        .map_err(|error| format!("shell negotiation failed: {error}"))?;
    println!(
        "lom_shell_transport schema=1 wire=9p2000.L revision={} epoch={}",
        connection.welcome().selected_revision,
        connection.connection_epoch()
    );
    let grant = GpuGrant::from_environment(connection.connection_epoch())?;
    let (renderer, admission) = RendererWorker::start(grant)?;
    println!("{}", admission.record("lom")?);
    let mut service = ShellService::new(connection, config, theme, allowance, renderer)?;
    loop {
        if service.step()? == 0 {
            service.wait_for_work()?;
        }
    }
}
