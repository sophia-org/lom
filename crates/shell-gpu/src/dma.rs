//! Producer-completed LINEAR XRGB8888 DMA-BUF export on the granted render
//! node. The GPU draws the frame, a queue-family release hands the image to
//! `VK_QUEUE_FAMILY_FOREIGN_EXT`, and the descriptor exists only after that
//! release's fence signals. Nothing here proves native import or presentation.
//!
//! Host synchronization: `vkQueueSubmit` on wgpu's raw queue is sound only
//! because every queue submission happens inside a `&mut GpuPreview` call.
//! A `DmaFrame` dropped on another thread only destroys its texture.
mod reuse;

use super::dma_layout::{self, Slot};
use super::dma_timing::{DmaTiming, Stage, Timer};
use ash::vk;
use imaging::record::Scene;
use masonry_imaging::TextureRenderer;
use std::os::fd::{FromRawFd, OwnedFd};
use std::time::{Duration, Instant};
use wgpu::hal::api::Vulkan;

/// One budget covers the draw and the release, like the readback deadline.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(2);
const HANDLE: vk::ExternalMemoryHandleTypeFlags = vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;
/// DRM XR24 is little-endian B, G, R, X: Vulkan's B8G8R8A8 with X ignored.
const FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;
/// DRM_FORMAT_MOD_LINEAR. RADV exports DMA-BUF images only with
/// DRM-modifier tiling, so LINEAR is requested as this explicit modifier.
const LINEAR: u64 = 0;
/// Device extensions wgpu 28 does not enable itself; `open_device` adds them.
const ADDED: [&std::ffi::CStr; 2] = [
    ash::ext::queue_family_foreign::NAME,
    ash::ext::image_drm_format_modifier::NAME,
];
const EXTENSIONS: [&std::ffi::CStr; 4] = [
    ash::ext::queue_family_foreign::NAME,
    ash::ext::image_drm_format_modifier::NAME,
    ash::khr::external_memory_fd::NAME,
    ash::ext::external_memory_dma_buf::NAME,
];

/// One completed, released frame. Its GPU allocation lives as long as this
/// value; keep it unchanged until Sophia's matching SourceReleased.
pub struct DmaFrame {
    /// Taken only by `Drop`, so its destruction can be timed.
    texture: Option<wgpu::Texture>,
    /// The texture's raw image, for the reuse queue-family barriers.
    image: vk::Image,
    fd: OwnedFd,
    width: u32,
    height: u32,
    layout: dma_layout::Layout,
    _slot: Slot,
    timer: Timer,
}

impl Drop for DmaFrame {
    fn drop(&mut self) {
        let mark = self.timer.start();
        drop(self.texture.take());
        self.timer.record(Stage::Destroy, mark);
    }
}

const _: fn() = || {
    fn send<T: Send>() {}
    send::<DmaFrame>();
};

impl DmaFrame {
    /// Frame width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }
    /// Frame height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Driver-chosen byte offset of plane 0.
    pub fn offset(&self) -> u32 {
        self.layout.offset
    }
    /// Driver-chosen row pitch of plane 0, at least `width * 4`.
    pub fn stride(&self) -> u32 {
        self.layout.stride
    }
    /// Exact descriptor size measured with `lseek`, as Sophia measures it.
    pub fn source_bytes(&self) -> u64 {
        self.layout.source_bytes
    }
    /// A new close-on-exec descriptor for the same DMA-BUF.
    pub fn dup_fd(&self) -> Result<OwnedFd, String> {
        self.fd
            .try_clone()
            .map_err(|error| format!("DMA-BUF descriptor duplicate: {error}"))
    }
}

/// An export whose GPU completion is unknown. Every object it may still use
/// stays alive and is never destroyed; the renderer refuses new work.
pub(super) struct Stalled {
    _source: Option<wgpu::Texture>,
    _target: Option<wgpu::Texture>,
    /// A reused frame, kept whole so it never drops or records a destroy.
    _frame: Option<DmaFrame>,
    _release: Option<Box<Release>>,
}

/// Command pool and fence of one submitted release.
struct Release {
    device: ash::Device,
    pool: vk::CommandPool,
    fence: vk::Fence,
}

impl Release {
    /// Only after its fence signalled; a stalled release is never destroyed.
    fn destroy(self) {
        // SAFETY: the fence signalled, so the command buffer is complete.
        unsafe {
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}

enum Released {
    Done,
    /// No release was submitted, but the draw may still be pending.
    Refused(String),
    Stalled(Option<Box<Release>>, String),
}

struct Target {
    texture: wgpu::Texture,
    image: vk::Image,
    memory: vk::DeviceMemory,
    offset: u64,
    row_pitch: u64,
}

/// Open the device, adding `VK_EXT_queue_family_foreign` and
/// `VK_EXT_image_drm_format_modifier` when the adapter supports them and
/// their dependencies (Vulkan 1.2 core image format lists). wgpu's
/// `request_device` cannot add extensions.
pub(super) fn open_device(
    adapter: &wgpu::Adapter,
    desc: &wgpu::DeviceDescriptor<'_>,
) -> Result<(wgpu::Device, wgpu::Queue), String> {
    let open = {
        // SAFETY: the guard is borrowed only to open a device from this same
        // adapter; the callback only adds a supported extension.
        let hal =
            unsafe { adapter.as_hal::<Vulkan>() }.ok_or("wgpu Vulkan adapter has no Vulkan HAL")?;
        let capabilities = hal.physical_device_capabilities();
        // SAFETY: read-only query of this adapter's physical device.
        let api = unsafe {
            hal.shared_instance()
                .raw_instance()
                .get_physical_device_properties(hal.raw_physical_device())
        }
        .api_version;
        let added: Vec<_> = if api >= vk::API_VERSION_1_2 {
            ADDED
                .into_iter()
                .filter(|name| capabilities.supports_extension(name))
                .collect()
        } else {
            Vec::new()
        };
        let callback: Box<wgpu::hal::vulkan::CreateDeviceCallback<'_>> = Box::new(move |args| {
            for name in added {
                if !args.extensions.contains(&name) {
                    args.extensions.push(name);
                }
            }
        });
        // SAFETY: the callback neither removes features nor adds unsupported ones.
        unsafe {
            hal.open_with_callback(desc.required_features, &desc.memory_hints, Some(callback))
        }
        .map_err(|error| format!("GPU device request: {error}"))?
    };
    // SAFETY: `open` came from this adapter with exactly `desc.required_features`.
    unsafe { adapter.create_device_from_hal(open, desc) }
        .map_err(|error| format!("GPU device request: {error}"))
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

impl super::GpuPreview {
    /// Enable or disable per-stage DMA-BUF export timing; it starts disabled.
    /// Re-enabling after disabling starts again from zero.
    pub fn set_dma_timing_enabled(&mut self, enabled: bool) {
        self.dma_timing.set_enabled(enabled);
    }

    /// Cumulative export timing, or `None` while timing is disabled. A frame
    /// exported while enabled records its destruction even if timing was
    /// disabled since. Each total is a separate relaxed atomic read, not a
    /// coherent snapshot while frames drop on other threads; read it on the
    /// thread that drops the frames, after the drop.
    pub fn dma_timing(&self) -> Option<DmaTiming> {
        self.dma_timing.totals()
    }

    /// Render a bounded scene into a new LINEAR XRGB8888 DMA-BUF on the
    /// granted render node. Returns only after the GPU finished drawing and
    /// released the image to a foreign queue family; there is no CPU
    /// readback. At most four frames may be retained. A frame whose
    /// completion is unknown stays owned here and blocks all new work.
    pub fn render_dma_buf(
        &mut self,
        scene: &mut Scene,
        width: u32,
        height: u32,
    ) -> Result<DmaFrame, String> {
        let mark = self.dma_timing.start();
        self.admit(width, height)?;
        if self.adapter_drm.is_none() {
            return Err("diagnostic renderer has no GPU grant for DMA-BUF export".into());
        }
        let slot = self
            .frames
            .acquire()
            .ok_or("four exported DMA-BUF frames are still retained")?;
        let deadline = Instant::now() + EXPORT_TIMEOUT;
        let target = self.export_target(width, height)?;
        self.dma_timing.record(Stage::Setup, mark);
        let mark = self.dma_timing.start();
        let source = self
            .renderer
            .render_source_texture(scene, width, height)
            .map_err(|error| error.to_string())?;
        let blitter = self.blitter.get_or_insert_with(|| {
            wgpu::util::TextureBlitter::new(&self.device, wgpu::TextureFormat::Bgra8Unorm)
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Shell DMA-BUF export blit"),
            });
        blitter.copy(
            &self.device,
            &mut encoder,
            &source.create_view(&wgpu::TextureViewDescriptor::default()),
            &target
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        let submission = self.queue.submit([encoder.finish()]);
        self.dma_timing.record(Stage::Draw, mark);
        let mark = self.dma_timing.start();
        // The draw may still be pending on every failure, so the source and
        // target are kept with whatever part of the release was submitted.
        let (release, error) = match self.release(target.image, submission, deadline) {
            Released::Done => (None, None),
            Released::Refused(error) => (None, Some(error)),
            Released::Stalled(release, error) => (release, Some(error)),
        };
        if let Some(error) = error {
            self.stalled = Some(Stalled {
                _source: Some(source),
                _target: Some(target.texture),
                _frame: None,
                _release: release,
            });
            return Err(error);
        }
        drop(source);
        self.dma_timing.record(Stage::Release, mark);
        let mark = self.dma_timing.start();
        let fd = self.export_fd(target.memory)?;
        let source_bytes = rustix::fs::seek(&fd, rustix::fs::SeekFrom::End(0))
            .map_err(|error| format!("DMA-BUF size: {error}"))?;
        let layout = dma_layout::checked_layout(
            width,
            height,
            target.offset,
            target.row_pitch,
            source_bytes,
        )?;
        self.dma_timing.record(Stage::Export, mark);
        Ok(DmaFrame {
            texture: Some(target.texture),
            image: target.image,
            fd,
            width,
            height,
            layout,
            _slot: slot,
            timer: self.dma_timing.clone(),
        })
    }

    /// A dedicated, exportable LINEAR image wrapped as a wgpu render target.
    fn export_target(&self, width: u32, height: u32) -> Result<Target, String> {
        // SAFETY: the guard is borrowed for raw object creation on this device;
        // every created object is destroyed here or owned by wgpu-hal.
        let hal =
            unsafe { self.device.as_hal::<Vulkan>() }.ok_or("wgpu device has no Vulkan HAL")?;
        if let Some(missing) = EXTENSIONS
            .iter()
            .find(|name| !hal.enabled_device_extensions().contains(name))
        {
            return Err(format!("DMA-BUF export needs {missing:?}"));
        }
        let shared = hal.shared_instance();
        let instance = shared.raw_instance();
        let physical = hal.raw_physical_device();
        let raw = hal.raw_device();
        // SAFETY: read-only query of the device's own physical device.
        let device_api = unsafe { instance.get_physical_device_properties(physical) }.api_version;
        if shared.instance_api_version() < vk::API_VERSION_1_1 || device_api < vk::API_VERSION_1_2 {
            return Err("DMA-BUF export needs a Vulkan 1.2 device".into());
        }
        supported(instance, physical, width, height)?;
        let mut external = vk::ExternalMemoryImageCreateInfo::default().handle_types(HANDLE);
        let modifiers = [LINEAR];
        let mut modifier =
            vk::ImageDrmFormatModifierListCreateInfoEXT::default().drm_format_modifiers(&modifiers);
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(FORMAT)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external)
            .push_next(&mut modifier);
        // SAFETY: the preflight accepted exactly these parameters.
        let image = unsafe { raw.create_image(&info, None) }
            .map_err(|error| format!("DMA-BUF image: {error}"))?;
        let memory = match allocate(instance, physical, raw, image) {
            Ok(memory) => memory,
            Err(error) => {
                // SAFETY: the image was never bound or used.
                unsafe { raw.destroy_image(image, None) };
                return Err(error);
            }
        };
        // SAFETY: dedicated memory allocated for this image's requirements.
        if let Err(error) = unsafe { raw.bind_image_memory(image, memory, 0) } {
            // SAFETY: neither object was used by the GPU.
            unsafe {
                raw.destroy_image(image, None);
                raw.free_memory(memory, None);
            }
            return Err(format!("DMA-BUF bind: {error}"));
        }
        let mut chosen = vk::ImageDrmFormatModifierPropertiesEXT::default();
        // SAFETY: the image was created with DRM-modifier tiling.
        let queried = unsafe {
            ash::ext::image_drm_format_modifier::Device::new(instance, raw)
                .get_image_drm_format_modifier_properties(image, &mut chosen)
        };
        if queried.is_err() || chosen.drm_format_modifier != LINEAR {
            // SAFETY: neither object was used by the GPU.
            unsafe {
                raw.destroy_image(image, None);
                raw.free_memory(memory, None);
            }
            return Err("DMA-BUF image did not keep the LINEAR modifier".into());
        }
        // SAFETY: DRM-modifier images expose their layout per memory plane.
        let layout = unsafe {
            raw.get_image_subresource_layout(
                image,
                vk::ImageSubresource {
                    aspect_mask: vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
                    mip_level: 0,
                    array_layer: 0,
                },
            )
        };
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        // SAFETY: the image matches this descriptor; wgpu-hal takes ownership
        // of the image and its dedicated memory and destroys them after use.
        let hal_texture = unsafe {
            hal.texture_from_raw(
                image,
                &wgpu::hal::TextureDescriptor {
                    label: Some("Shell DMA-BUF export"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUses::COLOR_TARGET,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                None,
                wgpu::hal::vulkan::TextureMemory::Dedicated(memory),
            )
        };
        drop(hal);
        // SAFETY: created on this device with this descriptor. Its contents
        // are undefined until the blit, which covers every pixel.
        let texture = unsafe {
            self.device.create_texture_from_hal::<Vulkan>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("Shell DMA-BUF export"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
            )
        };
        Ok(Target {
            texture,
            image,
            memory,
            offset: layout.offset,
            row_pitch: layout.row_pitch,
        })
    }

    /// Release the drawn image to the foreign queue family, submitted right
    /// after wgpu's draw on the same queue with no wait between. The barrier's
    /// first scope covers that earlier submission, so the release fence proves
    /// both finished; wgpu is then polled for its own bookkeeping. wgpu 28
    /// leaves a render target it last drew in COLOR_ATTACHMENT_OPTIMAL; GENERAL
    /// is the layout a non-Vulkan importer of a LINEAR DMA-BUF can assume.
    fn release(
        &self,
        image: vk::Image,
        draw: wgpu::SubmissionIndex,
        deadline: Instant,
    ) -> Released {
        // SAFETY: borrowed only to read raw handles of this device and queue.
        let Some(hal) = (unsafe { self.device.as_hal::<Vulkan>() }) else {
            return Released::Refused("wgpu device has no Vulkan HAL".into());
        };
        // SAFETY: as above, for the queue wgpu submitted the draw on.
        let Some(queue) = (unsafe { self.queue.as_hal::<Vulkan>() }).map(|q| q.as_raw()) else {
            return Released::Refused("wgpu queue has no Vulkan HAL".into());
        };
        let family = hal.queue_family_index();
        let device = hal.raw_device().clone();
        drop(hal);
        let refused = |device: &ash::Device, pool, error: vk::Result| {
            // SAFETY: nothing from this pool was submitted.
            unsafe { device.destroy_command_pool(pool, None) };
            Released::Refused(format!("DMA-BUF release recording: {error}"))
        };
        // SAFETY: a transient pool on the queue's own family.
        let pool = match unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT)
                    .queue_family_index(family),
                None,
            )
        } {
            Ok(pool) => pool,
            Err(error) => return Released::Refused(format!("DMA-BUF release pool: {error}")),
        };
        // SAFETY: one primary buffer from the pool just created.
        let command = match unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        } {
            Ok(buffers) => buffers[0],
            Err(error) => return refused(&device, pool, error),
        };
        let barrier = handoff(image, family);
        // SAFETY: recording into a fresh buffer; the image is alive and was
        // last used by wgpu's submission just made on this queue family.
        let recorded = unsafe {
            device
                .begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .and_then(|()| {
                    device.cmd_pipeline_barrier(
                        command,
                        HANDOFF_STAGES.0,
                        HANDOFF_STAGES.1,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier],
                    );
                    device.end_command_buffer(command)
                })
        };
        if let Err(error) = recorded {
            return refused(&device, pool, error);
        }
        // SAFETY: a fresh, unsignalled fence on this device.
        let fence = match unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) } {
            Ok(fence) => fence,
            Err(error) => return refused(&device, pool, error),
        };
        let release = Release {
            device,
            pool,
            fence,
        };
        let commands = [command];
        // SAFETY: host access to the queue is exclusive (see module docs).
        if let Err(error) = unsafe {
            release.device.queue_submit(
                queue,
                &[vk::SubmitInfo::default().command_buffers(&commands)],
                release.fence,
            )
        } {
            // A failed submit may still have reached the device; hold it all.
            return Released::Stalled(
                Some(Box::new(release)),
                format!("DMA-BUF release submit: {error}"),
            );
        }
        let nanos = u64::try_from(remaining(deadline).as_nanos()).unwrap_or(u64::MAX);
        // SAFETY: waiting on our own fence.
        match unsafe {
            release
                .device
                .wait_for_fences(&[release.fence], true, nanos)
        } {
            Ok(()) => {
                release.destroy();
                match self.device.poll(wgpu::PollType::Wait {
                    submission_index: Some(draw),
                    timeout: Some(remaining(deadline)),
                }) {
                    Ok(_) => Released::Done,
                    Err(error) => Released::Stalled(None, format!("GPU DMA-BUF draw: {error}")),
                }
            }
            Err(error) => Released::Stalled(
                Some(Box::new(release)),
                format!("DMA-BUF release fence: {error}"),
            ),
        }
    }

    fn export_fd(&self, memory: vk::DeviceMemory) -> Result<OwnedFd, String> {
        // SAFETY: borrowed only to load the extension's device function.
        let hal =
            unsafe { self.device.as_hal::<Vulkan>() }.ok_or("wgpu device has no Vulkan HAL")?;
        let fds = ash::khr::external_memory_fd::Device::new(
            hal.shared_instance().raw_instance(),
            hal.raw_device(),
        );
        // SAFETY: `memory` is alive, owned by the target texture, and was
        // allocated exportable as a DMA-BUF.
        let raw = unsafe {
            fds.get_memory_fd(
                &vk::MemoryGetFdInfoKHR::default()
                    .memory(memory)
                    .handle_type(HANDLE),
            )
        }
        .map_err(|error| format!("DMA-BUF export: {error}"))?;
        // SAFETY: vkGetMemoryFdKHR transfers a new descriptor to the caller.
        Ok(unsafe { OwnedFd::from_raw_fd(raw) })
    }
}

/// The one color subresource of an export image.
const COLOR: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

/// Source and destination stages of the foreign hand-off barrier.
const HANDOFF_STAGES: (vk::PipelineStageFlags, vk::PipelineStageFlags) = (
    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
);

/// Release a drawn image from `family` to the foreign queue family, from
/// wgpu's COLOR_ATTACHMENT_OPTIMAL to the GENERAL layout an importer assumes.
fn handoff(image: vk::Image, family: u32) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_access_mask(vk::AccessFlags::empty())
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_queue_family_index(family)
        .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
        .image(image)
        .subresource_range(COLOR)
}

/// LINEAR B8G8R8A8 color attachments exportable as DMA-BUF, at this size.
fn supported(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let mut external_info =
        vk::PhysicalDeviceExternalImageFormatInfo::default().handle_type(HANDLE);
    let mut modifier = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
        .drm_format_modifier(LINEAR)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let info = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(FORMAT)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .push_next(&mut external_info)
        .push_next(&mut modifier);
    let mut external = vk::ExternalImageFormatProperties::default();
    let mut properties = vk::ImageFormatProperties2::default().push_next(&mut external);
    // SAFETY: the chained structures stay valid for this read-only query.
    unsafe {
        instance.get_physical_device_image_format_properties2(physical, &info, &mut properties)
    }
    .map_err(|error| format!("LINEAR XRGB8888 DMA-BUF export unsupported: {error}"))?;
    let extent = properties.image_format_properties.max_extent;
    if extent.width < width || extent.height < height {
        return Err("LINEAR XRGB8888 DMA-BUF export is smaller than the frame".into());
    }
    if !external
        .external_memory_properties
        .external_memory_features
        .contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
    {
        return Err("LINEAR XRGB8888 images are not exportable as DMA-BUF".into());
    }
    Ok(())
}

/// Dedicated exportable memory, device-local when the image allows it.
fn allocate(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    image: vk::Image,
) -> Result<vk::DeviceMemory, String> {
    // SAFETY: read-only queries of a live image and its physical device.
    let (requirements, types) = unsafe {
        (
            device.get_image_memory_requirements(image),
            instance.get_physical_device_memory_properties(physical),
        )
    };
    let allowed =
        |&(index, _): &(usize, &vk::MemoryType)| requirements.memory_type_bits & (1 << index) != 0;
    let mut candidates = types
        .memory_types_as_slice()
        .iter()
        .enumerate()
        .filter(allowed);
    let index = candidates
        .clone()
        .find(|(_, kind)| {
            kind.property_flags
                .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .or_else(|| candidates.next())
        .map(|(index, _)| index as u32)
        .ok_or("no memory type can back the DMA-BUF image")?;
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let mut export = vk::ExportMemoryAllocateInfo::default().handle_types(HANDLE);
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(index)
        .push_next(&mut export)
        .push_next(&mut dedicated);
    // SAFETY: a valid dedicated, exportable allocation for this image.
    unsafe { device.allocate_memory(&info, None) }
        .map_err(|error| format!("DMA-BUF memory: {error}"))
}
