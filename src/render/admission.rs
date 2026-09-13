//! Startup-scoped GPU grant validation and exact Vulkan adapter selection.

use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
use std::path::PathBuf;

const GPU_MODE_ENV: &str = "SOPHIA_SHELL_GPU_MODE";
const GPU_GRANT_EPOCH_ENV: &str = "SOPHIA_SHELL_GPU_GRANT_EPOCH";
const GPU_RENDER_NODE_ENV: &str = "SOPHIA_SHELL_GPU_RENDER_NODE";
const GPU_DEVICE_MAJOR_ENV: &str = "SOPHIA_SHELL_GPU_DEVICE_MAJOR";
const GPU_DEVICE_MINOR_ENV: &str = "SOPHIA_SHELL_GPU_DEVICE_MINOR";
const GPU_PCI_BUS_ID_ENV: &str = "SOPHIA_SHELL_GPU_PCI_BUS_ID";

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
    /// PCI bus identity when the kernel device has one.
    pub pci_bus_id: Option<String>,
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
        if render_node != std::path::Path::new("/dev/dri/renderD128") {
            return Err("GPU grant render-node path is not the fixed private device".into());
        }
        let device_major = u32::try_from(number(&lookup, GPU_DEVICE_MAJOR_ENV)?)
            .map_err(|_| "GPU device major exceeds u32")?;
        let device_minor = u32::try_from(number(&lookup, GPU_DEVICE_MINOR_ENV)?)
            .map_err(|_| "GPU device minor exceeds u32")?;
        let metadata = std::fs::metadata(&render_node)
            .map_err(|error| format!("GPU render-node metadata: {error}"))?;
        if !metadata.file_type().is_char_device()
            || rustix::fs::major(metadata.rdev()) != device_major
            || rustix::fs::minor(metadata.rdev()) != device_minor
        {
            return Err("GPU grant does not name the exposed character device".into());
        }
        let pci_bus_id = lookup(GPU_PCI_BUS_ID_ENV)
            .map(|value| normalized_pci_bus_id(&value))
            .transpose()?;
        Ok(Self {
            epoch,
            render_node,
            device_major,
            device_minor,
            pci_bus_id,
        })
    }
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
) -> Result<usize, String> {
    let matching = adapters
        .iter()
        .enumerate()
        .filter(|(_, info)| {
            info.backend == wgpu::Backend::Vulkan
                && info.device_type != wgpu::DeviceType::Cpu
                && grant
                    .pci_bus_id
                    .as_ref()
                    .is_none_or(|identity| &info.device_pci_bus_id == identity)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [index] => Ok(*index),
        [] => Err("no Vulkan adapter matches Sophia's GPU grant".into()),
        _ => Err("Sophia's GPU grant resolves to multiple Vulkan adapters".into()),
    }
}

#[cfg(test)]
#[path = "../../tests/support/render_admission.rs"]
mod tests;
