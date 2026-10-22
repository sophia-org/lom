//! Bounded native smoke for producer-completed DMA-BUF export. Run only in
//! an authorized GPU qualification, in a domain whose `/dev/dri` holds just
//! the granted render node, for example (node 128, under `timeout 30`):
//!
//! ```text
//! bwrap --ro-bind / / --dev /dev --tmpfs /dev/dri \
//!   --dev-bind /dev/dri/renderD128 /dev/dri/renderD128 --tmpfs /tmp \
//!   --unshare-all --die-with-parent --new-session -- \
//!   env SOPHIA_SHELL_GPU_MODE=direct SOPHIA_SHELL_GPU_GRANT_EPOCH=1 \
//!   SOPHIA_SHELL_GPU_RENDER_NODE=/dev/dri/renderD128 \
//!   SOPHIA_SHELL_GPU_DEVICE_MAJOR=226 SOPHIA_SHELL_GPU_DEVICE_MINOR=128 \
//!   timeout 30 <built dma_smoke example>
//! ```
//!
//! Every GPU wait is bounded by the library's 2 s deadline. It proves
//! allocation, draw, foreign release, export and the retention bound on one
//! driver. It does not prove Sophia import, presentation or pixel content.
use imaging::record::Scene;
use sophia_shell_gpu::{DmaFrame, GpuGrant, GpuPreview};
use std::time::Instant;

fn main() -> Result<(), String> {
    let grant = GpuGrant::from_environment(1)?;
    let mut gpu = GpuPreview::new(&grant)?;
    println!("{}", gpu.evidence(&grant)?.record("dma_smoke")?);
    let mut frames = Vec::new();
    for _ in 0..4 {
        frames.push(render(&mut gpu, 200, 20)?);
    }
    if gpu.render_dma_buf(&mut Scene::new(), 200, 20).is_ok() {
        return Err("a fifth retained frame was admitted".into());
    }
    frames.remove(0);
    // A released slot admits a different size; padding is the driver's.
    frames.push(render(&mut gpu, 1920, 64)?);
    gpu.read_rgba(&mut Scene::new(), 16, 16)
        .map_err(|error| format!("readback after export: {error}"))?;
    drop(frames);
    println!("dma_smoke status=ok frames=5 refused_fifth=true readback_after=true");
    Ok(())
}

fn render(gpu: &mut GpuPreview, width: u32, height: u32) -> Result<DmaFrame, String> {
    let start = Instant::now();
    let frame = gpu.render_dma_buf(&mut Scene::new(), width, height)?;
    let elapsed = start.elapsed();
    let duplicate = frame.dup_fd()?;
    let size = rustix::fs::seek(&duplicate, rustix::fs::SeekFrom::End(0))
        .map_err(|error| format!("duplicate size: {error}"))?;
    let span = u64::from(frame.stride()) * u64::from(height - 1)
        + u64::from(frame.offset())
        + u64::from(width) * 4;
    if (frame.width(), frame.height()) != (width, height)
        || frame.stride() < width * 4
        || span > frame.source_bytes()
        || size != frame.source_bytes()
    {
        return Err("exported frame layout is inconsistent".into());
    }
    println!(
        "dma_smoke_frame schema=1 width={width} height={height} offset={} stride={} source_bytes={} span={span} elapsed_us={}",
        frame.offset(),
        frame.stride(),
        frame.source_bytes(),
        elapsed.as_micros(),
    );
    Ok(frame)
}
