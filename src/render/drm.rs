//! Vulkan adapter identity derived from `VK_EXT_physical_device_drm`.

/// Exact render-node identity reported by one Vulkan physical device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DrmRenderIdentity {
    pub has_render: bool,
    pub major: u32,
    pub minor: u32,
}

/// Inspect the same wgpu adapter that will later create the rendering device.
pub(super) fn adapter_render_identity(
    adapter: &wgpu::Adapter,
) -> Result<Option<DrmRenderIdentity>, String> {
    // SAFETY: the returned guard is borrowed only for this read-only physical-
    // device query. The adapter remains alive and no HAL resource is destroyed.
    let hal = unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }
        .ok_or("wgpu Vulkan adapter has no Vulkan HAL identity")?;
    if !hal
        .physical_device_capabilities()
        .supports_extension(ash::ext::physical_device_drm::NAME)
    {
        return Ok(None);
    }
    let mut drm = ash::vk::PhysicalDeviceDrmPropertiesEXT::default();
    let mut properties = ash::vk::PhysicalDeviceProperties2::default().push_next(&mut drm);
    // SAFETY: the chained property structures remain valid and uniquely
    // borrowed for this read-only Vulkan query.
    unsafe {
        hal.shared_instance()
            .raw_instance()
            .get_physical_device_properties2(hal.raw_physical_device(), &mut properties);
    }
    identity_from_properties(
        drm.has_render == ash::vk::TRUE,
        drm.render_major,
        drm.render_minor,
    )
    .map(Some)
}

fn identity_from_properties(
    has_render: bool,
    render_major: i64,
    render_minor: i64,
) -> Result<DrmRenderIdentity, String> {
    let major = u32::try_from(render_major)
        .map_err(|_| "Vulkan render-node major is negative or exceeds u32")?;
    let minor = u32::try_from(render_minor)
        .map_err(|_| "Vulkan render-node minor is negative or exceeds u32")?;
    Ok(DrmRenderIdentity {
        has_render,
        major,
        minor,
    })
}

#[cfg(test)]
#[path = "../../tests/support/render_drm.rs"]
mod tests;
