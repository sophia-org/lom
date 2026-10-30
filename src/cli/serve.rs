//! Protected production entry point; protocol ownership lives in `service`.

use crate::{
    config::parse_shell_config,
    render::{GpuGrant, RendererWorker},
    runtime::validate_theme,
    service::{ContentRenderer, ShellService},
};
use sophia_shell_client::ShellClientOptions;
use sophia_shell_protocol::{
    SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT, SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE,
    SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER, SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION,
    SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS,
};
use std::os::fd::AsFd;
use std::{path::Path, time::Duration};

pub(super) fn run(dma: Option<bool>, cpu: bool) -> Result<(), String> {
    run_with_renderer(dma, cpu, |epoch, dma, stop| {
        if cpu {
            // Explicit CPU mode reads no GPU grant and never falls back to GPU.
            // The record states the source-selected renderer, not admission:
            // an inherited grant may exist but is not used.
            let renderer = RendererWorker::start_cpu_until_stopped(&stop.flag, stop.wake.as_fd())?;
            println!(
                "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used"
            );
            return Ok(renderer);
        }
        let grant = GpuGrant::from_environment(epoch)?;
        let (renderer, admission) =
            RendererWorker::start_until_stopped(grant, dma, &stop.flag, stop.wake.as_fd())?;
        println!("{}", admission.record("lom")?);
        Ok(renderer)
    })
}

fn run_with_renderer<R: ContentRenderer>(
    dma: Option<bool>,
    cpu: bool,
    renderer: impl FnOnce(u64, bool, &super::shutdown::Shutdown) -> Result<R, String>,
) -> Result<(), String> {
    let stop = super::shutdown::Shutdown::install()?;
    let result = serve(&stop, dma, cpu, renderer);
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
    dma: Option<bool>,
    cpu: bool,
    renderer: impl FnOnce(u64, bool, &super::shutdown::Shutdown) -> Result<R, String>,
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
    let dma = dma.unwrap_or(config.dma_buf);
    if cpu && dma {
        return Err("--renderer cpu requires the owned-ring image source, not dma-buf".into());
    }
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
        | SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION
        | sophia_shell_client::IMAGE_CAPABILITY_OWNED_RING
        | if dma {
            sophia_shell_client::IMAGE_CAPABILITY_DMA_BUF
        } else {
            0
        };
    // The owned ring requires r10 and its capability; there is no
    // sealed or byte fallback. The image endpoint directory is the parent of the
    // exact role socket path.
    let endpoint_dir = Path::new(&socket)
        .parent()
        .map(|parent| parent.to_path_buf())
        .ok_or("shell socket path has no parent directory")?;
    let options = ShellClientOptions {
        minimum_revision: 10,
        maximum_revision: 10,
        required_capabilities: capabilities,
        handshake_timeout: Duration::from_secs(5),
    };
    stop.check()?;
    let (connection, _generation_channel) =
        super::authentication::connect(socket, options, || stop.requested())
            .map_err(|error| format!("shell negotiation failed: {error}"))?;
    stop.check()?;
    println!(
        "lom_shell_transport schema=1 wire=9p2000.L revision={} epoch={}",
        connection.welcome().selected_revision,
        connection.connection_epoch()
    );
    let renderer = renderer(connection.connection_epoch(), dma, stop)?;
    stop.check()?;
    let mut service = ShellService::new_until_stopped(
        connection,
        config,
        theme,
        allowance,
        renderer,
        (
            stop.flag.clone(),
            Some(stop.wake.try_clone().map_err(|error| error.to_string())?),
        ),
        endpoint_dir,
    )?;
    let mut reporter = super::work_statistics::Reporter::new(&mut service);
    let result = (|| {
        while !stop.requested() {
            let presented = service.step()?;
            reporter.emit(&service, false)?;
            if presented == 0 {
                service.wait_for_work_until(reporter.deadline())?;
            }
        }
        Ok(())
    })();
    reporter.emit(&service, true)?;
    result
}

#[cfg(test)]
#[path = "../../tests/support/shutdown.rs"]
mod tests;
