use super::*;
use std::collections::BTreeMap;

fn adapter(pci: &str, device_type: wgpu::DeviceType) -> wgpu::AdapterInfo {
    wgpu::AdapterInfo {
        name: "test".into(),
        vendor: 1,
        device: 2,
        device_type,
        device_pci_bus_id: pci.into(),
        driver: "test".into(),
        driver_info: "test".into(),
        backend: wgpu::Backend::Vulkan,
        subgroup_min_size: 1,
        subgroup_max_size: 1,
        transient_saves_memory: false,
    }
}

#[test]
fn grant_refuses_denied_stale_and_non_device_environments() {
    let mut values = BTreeMap::from([
        (GPU_MODE_ENV, "denied".to_owned()),
        (GPU_GRANT_EPOCH_ENV, "7".to_owned()),
        (GPU_RENDER_NODE_ENV, "/dev/null".to_owned()),
        (GPU_DEVICE_MAJOR_ENV, "1".to_owned()),
        (GPU_DEVICE_MINOR_ENV, "3".to_owned()),
    ]);
    let parse = |values: &BTreeMap<&str, String>, epoch| {
        GpuGrant::from_lookup(epoch, |key| values.get(key).cloned())
    };
    assert!(parse(&values, 7).is_err());
    values.insert(GPU_MODE_ENV, "direct".into());
    assert!(parse(&values, 8).is_err());
    values.insert(GPU_RENDER_NODE_ENV, "/dev/dri/renderD128".into());
    assert!(parse(&values, 7).is_err());
}

#[test]
fn adapter_selection_is_exact_unique_and_never_cpu() {
    let grant = GpuGrant {
        epoch: 7,
        render_node: "/dev/dri/renderD128".into(),
        device_major: 226,
        device_minor: 128,
        pci_bus_id: Some("0000:01:00.0".into()),
    };
    let adapters = [
        adapter("0000:02:00.0", wgpu::DeviceType::DiscreteGpu),
        adapter("0000:01:00.0", wgpu::DeviceType::IntegratedGpu),
    ];
    assert_eq!(select_adapter(&grant, &adapters), Ok(1));
    assert!(
        select_adapter(
            &grant,
            &[
                adapter("0000:01:00.0", wgpu::DeviceType::IntegratedGpu),
                adapter("0000:01:00.0", wgpu::DeviceType::DiscreteGpu),
            ],
        )
        .is_err()
    );
    assert!(select_adapter(&grant, &[adapter("0000:01:00.0", wgpu::DeviceType::Cpu)]).is_err());
}
