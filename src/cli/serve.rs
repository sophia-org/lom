//! Protected production entry point; protocol ownership lives in `service`.

use crate::{
    config::parse_shell_config,
    model::Model,
    protocol::ContentPixels,
    render::GpuPreview,
    runtime::validate_theme,
    service::{ContentRenderer, ShellService},
    ui::PreviewDriver,
};
use sophia_protocol::{
    SOPHIA_SHELL_CAPABILITY_CONTENT_SURFACE, SOPHIA_SHELL_CAPABILITY_DESCRIPTOR_SWITCHER,
    SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS,
};
use sophia_shell_client::{ShellClientOptions, ShellConnection};
use std::{path::Path, time::Duration};

const IDLE_POLL: Duration = Duration::from_millis(4);

pub(super) fn run() -> Result<(), String> {
    let socket = std::env::var_os("SOPHIA_SHELL_SOCKET")
        .ok_or("SOPHIA_SHELL_SOCKET is required for --serve")?;
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
        | SOPHIA_SHELL_CAPABILITY_VIEW_INDICATORS;
    let connection = ShellConnection::connect(
        socket,
        ShellClientOptions {
            minimum_revision: 6,
            maximum_revision: 6,
            required_capabilities: capabilities,
            handshake_timeout: Duration::from_secs(5),
        },
    )
    .map_err(|error| format!("shell negotiation failed: {error}"))?;
    let mut service =
        ShellService::new(connection, config, theme, allowance, VelloRenderer::new()?)?;
    loop {
        if service.step()? == 0 {
            std::thread::sleep(IDLE_POLL);
        }
    }
}

struct VelloRenderer(GpuPreview);
impl VelloRenderer {
    fn new() -> Result<Self, String> {
        GpuPreview::new().map(Self)
    }
}
impl ContentRenderer for VelloRenderer {
    fn render(
        &mut self,
        model: Model,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<ContentPixels, String> {
        let mut driver = PreviewDriver::new(model, width, height, scale, false)?;
        self.0.readback_content(&mut driver.scene())
    }
}
