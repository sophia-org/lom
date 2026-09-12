use super::{read, required};
use crate::{
    config::{ModuleKind, PanelConfig, Position, Theme},
    render::GpuPreview,
    runtime::parse_fixture,
    ui::PreviewDriver,
    update::{Msg, update},
};
use std::{collections::BTreeMap, ffi::OsString};

pub(super) fn run(
    config: PanelConfig,
    theme: Theme,
    options: BTreeMap<String, OsString>,
) -> Result<(), String> {
    let fixture_path = required(&options, "--fixture")?;
    let output = required(&options, "--output")?;
    let length = options.get("--width").map_or(Ok(1280u32), |v| {
        v.to_str()
            .ok_or("width must be UTF-8")
            .and_then(|s| s.parse().map_err(|_| "width must be an integer"))
    })?;
    let scale = options.get("--scale").map_or(Ok(1.0f64), |v| {
        v.to_str()
            .ok_or("scale must be UTF-8")
            .and_then(|s| s.parse().map_err(|_| "scale must be a number"))
    })?;
    if !(1..=8192).contains(&length) || !scale.is_finite() || !(0.5..=4.0).contains(&scale) {
        return Err("preview width must be 1..8192 and scale 0.5..4".into());
    }
    let model = parse_fixture(&read(&fixture_path)?, config, theme)
        .map_err(|e| format!("{}:{e}", fixture_path.display()))?;
    let thickness = (f64::from(model.config.height) * scale).ceil() as u32;
    let (width, height) = if matches!(model.config.position, Position::Left | Position::Right) {
        (thickness, length)
    } else {
        (length, thickness)
    };
    let mut panel = PreviewDriver::new(model.clone(), width, height, scale, false)?;
    let mut calendar = model.clone();
    let calendar = if let Some(index) = calendar
        .config
        .modules
        .iter()
        .position(|m| m.kind == ModuleKind::Clock)
    {
        let id = calendar.module_id(index);
        update(&mut calendar, Msg::ToggleCalendar(id));
        Some(PreviewDriver::new(
            calendar,
            (340.0 * scale).ceil() as u32,
            (280.0 * scale).ceil() as u32,
            scale,
            true,
        )?)
    } else {
        None
    };
    // Reserve a new artifact directory before GPU initialization; never replace prior evidence.
    std::fs::create_dir(&output).map_err(|e| format!("output must be a new directory: {e}"))?;
    let result = (|| {
        let mut gpu = GpuPreview::new()?;
        gpu.write_png(&mut panel.scene(), &output.join("panel.png"))?;
        if let Some(mut calendar) = calendar {
            gpu.write_png(&mut calendar.scene(), &output.join("calendar.png"))?;
        }
        let info = gpu.adapter();
        let report = format!(
            "Lom {}\nsource: explicit synthetic fixture\nrenderer: Vello GPU / Vulkan\nadapter: {}\ndevice-type: {:?}\ntoolkit: b81d8d7a631849def6eeab282561439b963862e5\npanel: {}x{} physical pixels\nscale: {}\nplacement: local allocation preview; position/margins/gap remain desired Engine placement\nSophia connected: false\nnative presentation: untested\n",
            env!("CARGO_PKG_VERSION"),
            info.name,
            info.device_type,
            width,
            height,
            scale
        );
        std::fs::write(output.join("evidence.txt"), report).map_err(|e| e.to_string())?;
        println!(
            "fixture GPU preview written to {}; no native presentation",
            output.display()
        );
        Ok::<_, String>(())
    })();
    if let Err(error) = &result {
        let _ = std::fs::write(output.join("failure.txt"), error);
    }
    result
}
