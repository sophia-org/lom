//! Explicit offscreen GPU diagnostics. Nothing here proves native presentation.

use crate::protocol::ContentPixels;
use crate::ui::PreviewScene;
use masonry_imaging::TextureRenderer;
use std::path::Path;

mod admission;
mod completion;
mod drm;
mod readback;
mod worker;
pub use admission::GpuGrant;
use readback::PendingReadback;
pub use worker::RendererWorker;

/// Auditable identity of the GPU worker admitted for one shell connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuAdmissionEvidence {
    /// Shell connection epoch named by Sophia's startup grant.
    pub grant_epoch: u64,
    /// Fixed private render node visible inside the shell domain.
    pub render_node: std::path::PathBuf,
    /// Kernel character-device identity observed inside the domain.
    pub device_major: u32,
    /// Kernel character-device identity observed inside the domain.
    pub device_minor: u32,
    /// Optional PCI bus diagnostic supplied by Sophia.
    pub pci_bus_id: Option<String>,
    /// PCI vendor identifier supplied by Sophia.
    pub pci_vendor_id: Option<u32>,
    /// PCI device identifier supplied by Sophia.
    pub pci_device_id: Option<u32>,
    /// DRM render-node identity reported by the selected Vulkan adapter.
    pub adapter_render_major: u32,
    /// DRM render-node identity reported by the selected Vulkan adapter.
    pub adapter_render_minor: u32,
    /// Vulkan adapter selected after applying the exact grant.
    pub adapter: wgpu::AdapterInfo,
    /// Sorted entries visible in the private `/dev/dri` directory.
    pub visible_dri_entries: Vec<String>,
}

impl GpuAdmissionEvidence {
    fn collect(
        grant: &GpuGrant,
        adapter: &wgpu::AdapterInfo,
        adapter_drm: drm::DrmRenderIdentity,
    ) -> Result<Self, String> {
        if !adapter_drm.has_render
            || adapter_drm.major != grant.device_major
            || adapter_drm.minor != grant.device_minor
        {
            return Err("selected Vulkan adapter no longer matches the GPU grant".into());
        }
        let visible_dri_entries = admission::visible_dri_entries(std::path::Path::new("/dev/dri"))?;
        let expected = grant
            .render_node
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("GPU grant render-node path has no UTF-8 basename")?;
        if visible_dri_entries.as_slice() != [expected] {
            return Err(format!(
                "GPU domain exposes unexpected DRM devices: {}",
                visible_dri_entries.join(",")
            ));
        }
        Ok(Self {
            grant_epoch: grant.epoch,
            render_node: grant.render_node.clone(),
            device_major: grant.device_major,
            device_minor: grant.device_minor,
            pci_bus_id: grant.pci_bus_id.clone(),
            pci_vendor_id: grant.pci_vendor_id,
            pci_device_id: grant.pci_device_id,
            adapter_render_major: adapter_drm.major,
            adapter_render_minor: adapter_drm.minor,
            adapter: adapter.clone(),
            visible_dri_entries,
        })
    }

    /// Stable diagnostic record used by isolated and native acceptance gates.
    pub fn record(&self) -> String {
        format!(
            "lom_gpu_admission schema=2 status=ready grant_epoch={} render_node={} device_major={} device_minor={} selection_method=drm_dev_t adapter_render_major={} adapter_render_minor={} adapter_has_render=true pci_bus_id={} pci_vendor_id={} pci_device_id={} backend={:?} device_type={:?} adapter_name={:?} driver={:?} visible_dri_entries={}",
            self.grant_epoch,
            self.render_node.display(),
            self.device_major,
            self.device_minor,
            self.adapter_render_major,
            self.adapter_render_minor,
            self.pci_bus_id.as_deref().unwrap_or("none"),
            self.pci_vendor_id
                .map(|value| format!("{value:04x}"))
                .as_deref()
                .unwrap_or("none"),
            self.pci_device_id
                .map(|value| format!("{value:04x}"))
                .as_deref()
                .unwrap_or("none"),
            self.adapter.backend,
            self.adapter.device_type,
            self.adapter.name,
            self.adapter.driver,
            self.visible_dri_entries.join(","),
        )
    }
}

/// Reusable Vello GPU renderer for sequential, bounded preview images.
pub struct GpuPreview {
    renderer: masonry_imaging::vello::Renderer,
    adapter: wgpu::AdapterInfo,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pending: Option<PendingReadback>,
    adapter_drm: Option<drm::DrmRenderIdentity>,
    _render_node: Option<std::fs::File>,
}
impl GpuPreview {
    /// Initialize Vulkan without a surface. Software adapters are explicitly refused.
    ///
    /// Call only for an explicitly requested GPU preview, never during configuration validation.
    pub fn new(grant: &GpuGrant) -> Result<Self, String> {
        let render_node = grant.open_render_node()?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let mut adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
        let infos = adapters
            .iter()
            .map(wgpu::Adapter::get_info)
            .collect::<Vec<_>>();
        let drm = adapters
            .iter()
            .map(drm::adapter_render_identity)
            .collect::<Result<Vec<_>, _>>()?;
        let selected = admission::select_adapter(grant, &infos, &drm)?;
        let adapter = adapters.swap_remove(selected);
        let adapter_drm = drm[selected].ok_or("selected Vulkan adapter omitted DRM identity")?;
        Self::from_adapter(adapter, Some(adapter_drm), Some(render_node))
    }

    /// Initialize an explicit offscreen diagnostic outside a Sophia shell.
    pub fn new_diagnostic() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|error| format!("no Vulkan GPU adapter: {error}"))?;
        if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
            return Err("software adapter refused: GPU preview has no CPU fallback".into());
        }
        Self::from_adapter(adapter, None, None)
    }

    fn from_adapter(
        adapter: wgpu::Adapter,
        adapter_drm: Option<drm::DrmRenderIdentity>,
        render_node: Option<std::fs::File>,
    ) -> Result<Self, String> {
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Lom explicit offscreen preview"),
            ..Default::default()
        }))
        .map_err(|error| format!("GPU device request: {error}"))?;
        let renderer = masonry_imaging::vello::new_target_renderer(device.clone(), queue.clone())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            renderer,
            adapter: info,
            device,
            queue,
            pending: None,
            adapter_drm,
            _render_node: render_node,
        })
    }
    /// Adapter metadata for diagnostic evidence, not native acceptance.
    pub fn adapter(&self) -> &wgpu::AdapterInfo {
        &self.adapter
    }
    fn adapter_drm(&self) -> Option<drm::DrmRenderIdentity> {
        self.adapter_drm
    }
    /// Render one content-sized scene and return owned wire pixels. This is a
    /// readback building block, not a grant, upload or presentation operation.
    /// Native callers must establish GPU admission before creating the renderer.
    pub fn readback_content(&mut self, scene: &mut PreviewScene) -> Result<ContentPixels, String> {
        ContentPixels::byte_len(scene.width, scene.height)?;
        let bytes = self.read_rgba(scene)?;
        ContentPixels::from_rgba8(scene.width, scene.height, bytes)
    }
    /// Render and read back one PNG. No concurrent frames or accepted native resources exist here.
    pub fn write_png(&mut self, scene: &mut PreviewScene, path: &Path) -> Result<(), String> {
        let bytes = self.read_rgba(scene)?;
        image::save_buffer_with_format(
            path,
            &bytes,
            scene.width,
            scene.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())
    }

    fn read_rgba(&mut self, scene: &mut PreviewScene) -> Result<Vec<u8>, String> {
        if self.pending.is_some() {
            return Err(
                "GPU readback previously failed; renderer retains that job and refuses new work"
                    .into(),
            );
        }
        if scene.width == 0
            || scene.height == 0
            || scene.width > 8192
            || scene.height > 4096
            || u64::from(scene.width) * u64::from(scene.height) > 8 * 1024 * 1024
        {
            return Err("preview exceeds the 8M pixel output bound".into());
        }
        let texture = self
            .renderer
            .render_source_texture(&mut scene.scene, scene.width, scene.height)
            .map_err(|e| e.to_string())?;
        self.pending = Some(PendingReadback::submit(
            &self.device,
            &self.queue,
            texture,
            scene.width,
            scene.height,
        ));
        let bytes = self
            .pending
            .as_ref()
            .expect("submitted readback")
            .finish(&self.device)?;
        self.pending = None;
        Ok(bytes)
    }
}
