//! Protected production entry point; protocol ownership lives in `service`.

use crate::{
    config::{PanelConfig, Renderer, parse_shell_config},
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
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::atomic::AtomicBool;
use std::{path::Path, time::Duration};

/// The binary that entered `--serve`; never inferred from argv[0] or the
/// environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Entry {
    /// `lom --serve`: GPU or the production CPU direct fill.
    Ordinary,
    /// Explicit default-off SFT; CPU owned-ring only, same scene and service.
    #[cfg(feature = "experimental-frame-transactions")]
    FrameTransactions,
    /// `lom-cpu-reference --serve`: only the Vello CPU direct and copy arms.
    #[cfg(feature = "diagnostic-cpu-reference")]
    CpuReference,
    /// `lom-cpu-cadence --serve`: the same arms, redirtied to a fixed quota.
    #[cfg(feature = "diagnostic-cpu-reference")]
    CpuCadence,
    /// Fixed cadence using ordinary CPU direct fill without diagnostic hashing.
    #[cfg(feature = "diagnostic-cpu-reference")]
    CpuCadenceNoChecksum,
    #[cfg(feature = "diagnostic-cpu-reference")]
    CpuRegions,
    #[cfg(feature = "diagnostic-cpu-reference")]
    CpuRegionsFull,
    #[cfg(feature = "diagnostic-cpu-reference")]
    CpuRegionCadence,
    #[cfg(feature = "diagnostic-cpu-reference")]
    CpuRegionCadenceFull,
}

#[cfg(feature = "diagnostic-cpu-reference")]
pub(super) const REFUSE_GPU: &str = "lom-cpu-reference refuses the GPU renderer";
#[cfg(feature = "diagnostic-cpu-reference")]
pub(super) const REFUSE_GPU_CADENCE: &str = "lom-cpu-cadence refuses the GPU renderer";

impl Entry {
    fn revision(self) -> u16 {
        #[cfg(feature = "experimental-frame-transactions")]
        if self == Self::FrameTransactions {
            return 13;
        }
        10
    }

    #[cfg(feature = "diagnostic-cpu-reference")]
    fn region_sequence(self) -> Option<bool> {
        match self {
            Self::CpuRegions => Some(false),
            Self::CpuRegionsFull => Some(true),
            _ => None,
        }
    }

    #[cfg(feature = "diagnostic-cpu-reference")]
    fn region_cadence(self) -> Option<bool> {
        match self {
            Self::CpuRegionCadence => Some(false),
            Self::CpuRegionCadenceFull => Some(true),
            _ => None,
        }
    }

    fn admit(self, renderer: Renderer) -> Result<(), String> {
        let (allowed, refusal): (&[Renderer], &str) = match self {
            Entry::Ordinary => (
                &[Renderer::Gpu, Renderer::Cpu],
                "the cpu-copy-reference renderer requires lom-cpu-reference",
            ),
            #[cfg(feature = "experimental-frame-transactions")]
            Entry::FrameTransactions => (&[Renderer::Cpu], "SFT requires the cpu renderer"),
            #[cfg(feature = "diagnostic-cpu-reference")]
            Entry::CpuReference => (&[Renderer::Cpu, Renderer::CpuCopyReference], REFUSE_GPU),
            #[cfg(feature = "diagnostic-cpu-reference")]
            Entry::CpuCadence => (
                &[Renderer::Cpu, Renderer::CpuCopyReference],
                REFUSE_GPU_CADENCE,
            ),
            #[cfg(feature = "diagnostic-cpu-reference")]
            Entry::CpuCadenceNoChecksum => (
                &[Renderer::Cpu],
                "lom-cpu-cadence-no-checksum requires the cpu renderer",
            ),
            #[cfg(feature = "diagnostic-cpu-reference")]
            Entry::CpuRegions
            | Entry::CpuRegionsFull
            | Entry::CpuRegionCadence
            | Entry::CpuRegionCadenceFull => (
                &[Renderer::Cpu],
                "region sequence requires the cpu renderer",
            ),
        };
        if allowed.contains(&renderer) {
            Ok(())
        } else {
            Err(refusal.into())
        }
    }
}

pub(super) fn run(
    entry: Entry,
    dma: Option<bool>,
    renderer: Option<Renderer>,
) -> Result<(), String> {
    run_with_renderer(entry, dma, renderer, |epoch, dma, renderer, stop| {
        start_renderer(entry, epoch, dma, renderer, &stop.flag, stop.wake.as_fd())
    })
}

/// Start the resolved renderer. CPU reads no GPU grant and never falls back to
/// GPU; its record states the source-selected renderer, not admission: an
/// inherited grant may exist but is not used.
fn start_renderer(
    entry: Entry,
    epoch: u64,
    dma: bool,
    renderer: Renderer,
    stop: &AtomicBool,
    wake: BorrowedFd<'_>,
) -> Result<RendererWorker, String> {
    #[cfg(feature = "diagnostic-cpu-reference")]
    if let Some(full) = entry.region_sequence().or(entry.region_cadence()) {
        entry.admit(renderer)?;
        let diagnostic = if entry.region_cadence().is_some() {
            "cpu-region-cadence"
        } else {
            "cpu-region-sequence"
        };
        let worker = RendererWorker::start_cpu_until_stopped(stop, wake, false)?;
        println!(
            "lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used diagnostic={diagnostic} role={}",
            if full { "full" } else { "regions" }
        );
        return Ok(worker);
    }
    #[cfg(feature = "diagnostic-cpu-reference")]
    if entry == Entry::CpuCadenceNoChecksum {
        entry.admit(renderer)?;
        let worker = RendererWorker::start_cpu_until_stopped(stop, wake, false)?;
        println!("{}", super::cpu_reference::record(renderer)?);
        println!("lom_checksum_policy schema=1 checksum=unavailable diagnostic=cpu-cadence");
        return Ok(worker);
    }
    #[cfg(feature = "diagnostic-cpu-reference")]
    if matches!(entry, Entry::CpuReference | Entry::CpuCadence) {
        entry.admit(renderer)?;
        return super::cpu_reference::start(renderer, stop, wake);
    }
    entry.admit(renderer)?;
    if renderer == Renderer::Cpu {
        let worker = RendererWorker::start_cpu_until_stopped(stop, wake, false)?;
        println!("lom_renderer schema=1 renderer=vello-cpu source=owned-ring-fill gpu=not-used");
        return Ok(worker);
    }
    let grant = GpuGrant::from_environment(epoch)?;
    let (worker, admission) = RendererWorker::start_until_stopped(grant, dma, stop, wake)?;
    println!("{}", admission.record("lom")?);
    Ok(worker)
}

/// An explicit argument overrides the protected panel's setting. CPU never
/// pairs with DMA-BUF, so that combination fails instead of falling back. The
/// entry's renderer refusal is also decided here, before any connection.
fn resolve(
    entry: Entry,
    dma: Option<bool>,
    renderer: Option<Renderer>,
    config: &PanelConfig,
) -> Result<(bool, Renderer), String> {
    let dma = dma.unwrap_or(config.dma_buf);
    let renderer = renderer.unwrap_or(config.renderer);
    if renderer != Renderer::Gpu && dma {
        return Err("the CPU renderer requires the owned-ring image source, not dma-buf".into());
    }
    entry.admit(renderer)?;
    Ok((dma, renderer))
}

fn run_with_renderer<R: ContentRenderer>(
    entry: Entry,
    dma: Option<bool>,
    renderer: Option<Renderer>,
    start: impl FnOnce(u64, bool, Renderer, &super::shutdown::Shutdown) -> Result<R, String>,
) -> Result<(), String> {
    let stop = super::shutdown::Shutdown::install()?;
    let result = serve(&stop, entry, dma, renderer, start);
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
    entry: Entry,
    dma: Option<bool>,
    renderer: Option<Renderer>,
    start: impl FnOnce(u64, bool, Renderer, &super::shutdown::Shutdown) -> Result<R, String>,
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
    let (dma, renderer) = resolve(entry, dma, renderer, &config)?;
    #[cfg(feature = "diagnostic-cpu-reference")]
    if entry.region_sequence().is_some() || entry.region_cadence().is_some() {
        crate::service::sequence::validate_config(&config)?;
        if std::env::var_os("SOPHIA_SHELL_WORK_STATS").is_some() {
            return Err("region sequence refuses SOPHIA_SHELL_WORK_STATS".into());
        }
    }
    // The cadence owns the work counters from zero; its samples replace them.
    #[cfg(feature = "diagnostic-cpu-reference")]
    if matches!(entry, Entry::CpuCadence | Entry::CpuCadenceNoChecksum)
        && std::env::var_os("SOPHIA_SHELL_WORK_STATS").is_some()
    {
        return Err(match entry {
            Entry::CpuCadenceNoChecksum => {
                "lom-cpu-cadence-no-checksum refuses SOPHIA_SHELL_WORK_STATS"
            }
            _ => "lom-cpu-cadence refuses SOPHIA_SHELL_WORK_STATS",
        }
        .into());
    }
    let allowance = std::env::var("SOPHIA_SHELL_BAR_THICKNESS")
        .map_err(|_| "SOPHIA_SHELL_BAR_THICKNESS is required for --serve")?
        .parse::<u32>()
        .map_err(|_| "SOPHIA_SHELL_BAR_THICKNESS must be an integer")?;
    if allowance == 0 {
        return Err("admitted shell allowance must be positive".into());
    }
    let revision = entry.revision();
    let capabilities = SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER
        | SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE
        | SOPHIA_SHELL_CAPABILITY_CONTENT_DISCRETE_INPUT
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS
        | SOPHIA_SHELL_CAPABILITY_INDICATOR_ACTIVATION
        | sophia_shell_client::IMAGE_CAPABILITY_OWNED_RING
        | if revision == 13 {
            sophia_shell_protocol::shell_files::SHELL_FRAME_CAPABILITY
        } else {
            0
        }
        | if dma {
            sophia_shell_client::IMAGE_CAPABILITY_DMA_BUF
        } else {
            0
        };
    // Ordinary owned ring uses r10; only the explicit SFT entry requests r13. No
    // sealed or byte fallback. The image endpoint directory is the parent of the
    // exact role socket path.
    let endpoint_dir = Path::new(&socket)
        .parent()
        .map(|parent| parent.to_path_buf())
        .ok_or("shell socket path has no parent directory")?;
    let options = ShellClientOptions {
        minimum_revision: revision,
        maximum_revision: revision,
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
    let renderer = start(connection.connection_epoch(), dma, renderer, stop)?;
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
    drive(entry, &mut service, |_| stop.requested())
}

/// The owner loop. The reporter comes first: it disables counters when unset,
/// which the cadence must then find disabled and enable from zero.
fn drive<R: ContentRenderer>(
    entry: Entry,
    service: &mut ShellService<R>,
    stop: impl Fn(&ShellService<R>) -> bool,
) -> Result<(), String> {
    let mut reporter = super::work_statistics::Reporter::new(service);
    #[cfg(not(feature = "diagnostic-cpu-reference"))]
    let _ = entry;
    #[cfg(feature = "diagnostic-cpu-reference")]
    if matches!(entry, Entry::CpuCadence | Entry::CpuCadenceNoChecksum) {
        service.enable_diagnostic_cadence()?;
        println!("{}", super::cpu_reference::cadence_record());
    }
    #[cfg(feature = "diagnostic-cpu-reference")]
    if let Some(full) = entry.region_sequence() {
        service.enable_region_sequence(full)?;
    }
    #[cfg(feature = "diagnostic-cpu-reference")]
    if let Some(full) = entry.region_cadence() {
        service.enable_region_cadence(full)?;
        println!(
            "lom_cadence schema=1 mode=alternate-after-presented quota={} diagnostic=cpu-region-cadence",
            crate::service::CADENCE_QUOTA
        );
    }
    let result = (|| {
        while !stop(service) {
            let presented = service.step()?;
            reporter.emit(service, false)?;
            if presented == 0 {
                service.wait_for_work_until(reporter.deadline())?;
            }
        }
        Ok(())
    })();
    reporter.emit(service, true)?;
    #[cfg(feature = "diagnostic-cpu-reference")]
    if let Some(record) = service.cadence_final() {
        println!("{record}");
    }
    #[cfg(feature = "diagnostic-cpu-reference")]
    if let Some(record) = service.region_sequence_final() {
        println!("{record}");
    }
    result
}

/// The production owner loop for the service cadence tests, which own the
/// socket peer; `Some` selects cadence with the requested checksum policy.
#[cfg(all(test, feature = "diagnostic-cpu-reference"))]
pub(crate) fn drive_for_tests<R: ContentRenderer>(
    cadence: Option<bool>,
    service: &mut ShellService<R>,
    stop: impl Fn(&ShellService<R>) -> bool,
) -> Result<(), String> {
    let entry = match cadence {
        Some(true) => Entry::CpuCadence,
        Some(false) => Entry::CpuCadenceNoChecksum,
        None => Entry::CpuReference,
    };
    drive(entry, service, stop)
}

#[cfg(test)]
#[path = "../../tests/support/shutdown.rs"]
mod tests;
