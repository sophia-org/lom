//! Startup-scoped GPU grant validation and exact Vulkan adapter selection.

use std::fs::File;
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
use std::path::PathBuf;

use super::drm::DrmRenderIdentity;

const GPU_MODE_ENV: &str = "SOPHIA_SHELL_GPU_MODE";
const GPU_GRANT_EPOCH_ENV: &str = "SOPHIA_SHELL_GPU_GRANT_EPOCH";
const GPU_RENDER_NODE_ENV: &str = "SOPHIA_SHELL_GPU_RENDER_NODE";
const GPU_DEVICE_MAJOR_ENV: &str = "SOPHIA_SHELL_GPU_DEVICE_MAJOR";
const GPU_DEVICE_MINOR_ENV: &str = "SOPHIA_SHELL_GPU_DEVICE_MINOR";
const GPU_PCI_BUS_ID_ENV: &str = "SOPHIA_SHELL_GPU_PCI_BUS_ID";
const GPU_PCI_VENDOR_ID_ENV: &str = "SOPHIA_SHELL_GPU_PCI_VENDOR_ID";
const GPU_PCI_DEVICE_ID_ENV: &str = "SOPHIA_SHELL_GPU_PCI_DEVICE_ID";

/// Exact startup authority for one GPU worker and shell connection epoch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuGrant {
    /// The shell connection epoch that owns this process-scoped grant.
    pub epoch: u64,
    /// Private render-node path exposed by Sophia.
    pub render_node: PathBuf,
    /// Kernel character-device major number observed by Sophia.
    pub device_major: u32,
    /// Kernel character-device minor number observed by Sophia.
    pub device_minor: u32,
    /// Optional PCI bus diagnostic supplied by Sophia.
    pub pci_bus_id: Option<String>,
    /// Optional PCI vendor diagnostic supplied by Sophia.
    pub pci_vendor_id: Option<u32>,
    /// Optional PCI device diagnostic supplied by Sophia.
    pub pci_device_id: Option<u32>,
}

impl GpuGrant {
    /// Read and validate Sophia's process-scoped direct-GPU grant.
    pub fn from_environment(connection_epoch: u64) -> Result<Self, String> {
        Self::from_lookup(connection_epoch, |key| std::env::var(key).ok())
    }

    fn from_lookup(
        connection_epoch: u64,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        if lookup(GPU_MODE_ENV).as_deref() != Some("direct") {
            return Err("Sophia did not grant direct GPU access to this shell process".into());
        }
        let epoch = number(&lookup, GPU_GRANT_EPOCH_ENV)?;
        if epoch == 0 || epoch != connection_epoch {
            return Err("GPU grant epoch does not match the shell connection".into());
        }
        let render_node = PathBuf::from(
            lookup(GPU_RENDER_NODE_ENV).ok_or("GPU grant omitted its render-node path")?,
        );
        let device_major = u32::try_from(number(&lookup, GPU_DEVICE_MAJOR_ENV)?)
            .map_err(|_| "GPU device major exceeds u32")?;
        let device_minor = u32::try_from(number(&lookup, GPU_DEVICE_MINOR_ENV)?)
            .map_err(|_| "GPU device minor exceeds u32")?;
        let render_name = render_node
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("GPU grant render-node path has no UTF-8 basename")?;
        if render_node.parent() != Some(std::path::Path::new("/dev/dri"))
            || render_name != format!("renderD{device_minor}")
        {
            return Err("GPU grant render-node path disagrees with its kernel minor".into());
        }
        let metadata = std::fs::metadata(&render_node)
            .map_err(|error| format!("GPU render-node metadata: {error}"))?;
        if !metadata.file_type().is_char_device()
            || rustix::fs::major(metadata.rdev()) != device_major
            || rustix::fs::minor(metadata.rdev()) != device_minor
        {
            return Err("GPU grant does not name the exposed character device".into());
        }
        if visible_dri_entries(std::path::Path::new("/dev/dri"))?.as_slice() != [render_name] {
            return Err("GPU grant domain exposes more than its selected private device".into());
        }
        let pci = pci_diagnostics(&lookup)?;
        Ok(Self {
            epoch,
            render_node,
            device_major,
            device_minor,
            pci_bus_id: pci.bus_id,
            pci_vendor_id: pci.vendor_id,
            pci_device_id: pci.device_id,
        })
    }

    pub(super) fn open_render_node(&self) -> Result<File, String> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.render_node)
            .map_err(|error| format!("open granted GPU render node: {error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("granted GPU render-node identity: {error}"))?;
        if !metadata.file_type().is_char_device()
            || rustix::fs::major(metadata.rdev()) != self.device_major
            || rustix::fs::minor(metadata.rdev()) != self.device_minor
        {
            return Err("opened GPU render node disagrees with Sophia's grant".into());
        }
        Ok(file)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct PciDiagnostics {
    bus_id: Option<String>,
    vendor_id: Option<u32>,
    device_id: Option<u32>,
}

fn pci_diagnostics(lookup: &impl Fn(&str) -> Option<String>) -> Result<PciDiagnostics, String> {
    let bus_id = lookup(GPU_PCI_BUS_ID_ENV)
        .map(|value| normalized_pci_bus_id(&value))
        .transpose()?;
    let vendor_id = optional_hex(lookup, GPU_PCI_VENDOR_ID_ENV)?;
    let device_id = optional_hex(lookup, GPU_PCI_DEVICE_ID_ENV)?;
    Ok(PciDiagnostics {
        bus_id,
        vendor_id,
        device_id,
    })
}

fn optional_hex(
    lookup: &impl Fn(&str) -> Option<String>,
    key: &str,
) -> Result<Option<u32>, String> {
    lookup(key)
        .map(|value| {
            let value = u32::from_str_radix(value.strip_prefix("0x").unwrap_or(&value), 16)
                .map_err(|_| format!("GPU grant {key} is not a hexadecimal integer"))?;
            (value <= u16::MAX.into())
                .then_some(value)
                .ok_or_else(|| format!("GPU grant {key} exceeds sixteen bits"))
        })
        .transpose()
}

fn number(lookup: &impl Fn(&str) -> Option<String>, key: &str) -> Result<u64, String> {
    lookup(key)
        .ok_or_else(|| format!("GPU grant omitted {key}"))?
        .parse()
        .map_err(|_| format!("GPU grant {key} is not an unsigned integer"))
}

fn normalized_pci_bus_id(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    if bytes.len() != 12
        || bytes[4] != b':'
        || bytes[7] != b':'
        || bytes[10] != b'.'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7 | 10) || byte.is_ascii_hexdigit())
    {
        return Err("GPU grant PCI bus identity is malformed".into());
    }
    Ok(value.to_ascii_lowercase())
}

pub(super) fn visible_dri_entries(path: &std::path::Path) -> Result<Vec<String>, String> {
    let mut entries = std::fs::read_dir(path)
        .map_err(|error| format!("GPU device directory {}: {error}", path.display()))?
        .map(|entry| {
            let entry = entry.map_err(|error| format!("GPU device directory entry: {error}"))?;
            entry
                .file_name()
                .into_string()
                .map_err(|_| "GPU device directory contains a non-UTF-8 entry".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    Ok(entries)
}

pub(super) fn select_adapter(
    grant: &GpuGrant,
    adapters: &[wgpu::AdapterInfo],
    drm: &[Option<DrmRenderIdentity>],
) -> Result<usize, String> {
    if adapters.len() != drm.len() {
        return Err("Vulkan adapter identity inventory is inconsistent".into());
    }
    let vulkan_non_cpu = adapters
        .iter()
        .filter(|info| {
            info.backend == wgpu::Backend::Vulkan && info.device_type != wgpu::DeviceType::Cpu
        })
        .count();
    let drm_extension = adapters
        .iter()
        .zip(drm)
        .filter(|(info, identity)| {
            info.backend == wgpu::Backend::Vulkan
                && info.device_type != wgpu::DeviceType::Cpu
                && identity.is_some()
        })
        .count();
    let has_render = adapters
        .iter()
        .zip(drm)
        .filter(|(info, identity)| {
            info.backend == wgpu::Backend::Vulkan
                && info.device_type != wgpu::DeviceType::Cpu
                && identity.is_some_and(|identity| identity.has_render)
        })
        .count();
    let matching = adapters
        .iter()
        .zip(drm)
        .enumerate()
        .filter(|(_, (info, identity))| {
            info.backend == wgpu::Backend::Vulkan
                && info.device_type != wgpu::DeviceType::Cpu
                && identity.is_some_and(|identity| {
                    identity.has_render
                        && identity.major == grant.device_major
                        && identity.minor == grant.device_minor
                })
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [index] => Ok(*index),
        [] => Err(format!(
            "no Vulkan adapter matches Sophia's GPU grant (enumerated={} vulkan_non_cpu={vulkan_non_cpu} drm_extension={drm_extension} has_render={has_render} dev_t_match=0)",
            adapters.len()
        )),
        _ => Err("Sophia's GPU grant resolves to multiple Vulkan adapters".into()),
    }
}

#[cfg(test)]
#[path = "../../tests/support/render_admission.rs"]
mod tests;
