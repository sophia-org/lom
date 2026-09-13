use super::*;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

fn adapter(
    pci: &str,
    vendor: u32,
    device: u32,
    device_type: wgpu::DeviceType,
) -> wgpu::AdapterInfo {
    wgpu::AdapterInfo {
        name: "test".into(),
        vendor,
        device,
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

fn drm(major: u32, minor: u32) -> Option<DrmRenderIdentity> {
    Some(DrmRenderIdentity {
        has_render: true,
        major,
        minor,
    })
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
        pci_vendor_id: Some(0x1002),
        pci_device_id: Some(0x744c),
    };
    let adapters = [
        adapter(
            "0000:02:00.0",
            0x1002,
            0x164e,
            wgpu::DeviceType::DiscreteGpu,
        ),
        adapter(
            "0000:01:00.0",
            0x1002,
            0x744c,
            wgpu::DeviceType::IntegratedGpu,
        ),
    ];
    assert_eq!(
        select_adapter(&grant, &adapters, &[drm(226, 129), drm(226, 128)]),
        Ok(1)
    );
    assert_eq!(
        select_adapter(
            &grant,
            &[adapter(
                "0000:ff:00.0",
                0xffff,
                0xffff,
                wgpu::DeviceType::DiscreteGpu,
            )],
            &[drm(226, 128)],
        ),
        Ok(0),
        "PCI diagnostics must neither authorize nor veto an exact DRM identity"
    );
    assert!(
        select_adapter(
            &grant,
            &[
                adapter(
                    "0000:01:00.0",
                    0x1002,
                    0x744c,
                    wgpu::DeviceType::IntegratedGpu
                ),
                adapter(
                    "0000:01:00.0",
                    0x1002,
                    0x744c,
                    wgpu::DeviceType::DiscreteGpu
                ),
            ],
            &[drm(226, 128), drm(226, 128)],
        )
        .is_err()
    );
    assert!(
        select_adapter(
            &grant,
            &[adapter(
                "0000:01:00.0",
                0x1002,
                0x744c,
                wgpu::DeviceType::Cpu
            )],
            &[drm(226, 128)],
        )
        .is_err()
    );

    assert_eq!(
        select_adapter(
            &grant,
            &[adapter("", 0x1002, 0x744c, wgpu::DeviceType::DiscreteGpu)],
            &[drm(226, 128)],
        ),
        Ok(0)
    );
    assert!(
        select_adapter(
            &grant,
            &[adapter("", 0x1002, 0x164e, wgpu::DeviceType::DiscreteGpu)],
            &[drm(226, 129)],
        )
        .is_err()
    );
    assert!(
        select_adapter(
            &grant,
            &[adapter(
                "0000:01:00.0",
                0x1002,
                0x744c,
                wgpu::DeviceType::DiscreteGpu,
            )],
            &[None],
        )
        .is_err()
    );
    assert!(
        select_adapter(
            &grant,
            &[adapter(
                "0000:01:00.0",
                0x1002,
                0x744c,
                wgpu::DeviceType::DiscreteGpu,
            )],
            &[Some(DrmRenderIdentity {
                has_render: false,
                major: 226,
                minor: 128,
            })],
        )
        .is_err()
    );
    assert!(select_adapter(&grant, &adapters, &[drm(226, 128)]).is_err());
}

#[test]
fn optional_pci_diagnostics_are_independent_bounded_and_hexadecimal() {
    let values = BTreeMap::from([
        (GPU_PCI_BUS_ID_ENV.to_owned(), "0000:03:00.0".to_owned()),
        (GPU_PCI_VENDOR_ID_ENV.to_owned(), "1002".to_owned()),
        (GPU_PCI_DEVICE_ID_ENV.to_owned(), "744c".to_owned()),
    ]);
    assert_eq!(
        pci_diagnostics(&|key: &str| values.get(key).cloned()).unwrap(),
        PciDiagnostics {
            bus_id: Some("0000:03:00.0".to_owned()),
            vendor_id: Some(0x1002),
            device_id: Some(0x744c),
        }
    );

    let mut partial = values.clone();
    partial.remove(GPU_PCI_DEVICE_ID_ENV);
    assert_eq!(
        pci_diagnostics(&|name| partial.get(name).cloned()).unwrap(),
        PciDiagnostics {
            bus_id: Some("0000:03:00.0".into()),
            vendor_id: Some(0x1002),
            device_id: None,
        },
        "diagnostic absence must not invalidate exact DRM authorization"
    );

    for (key, value) in [
        (GPU_PCI_VENDOR_ID_ENV, Some("not-hex")),
        (GPU_PCI_DEVICE_ID_ENV, Some("10000")),
    ] {
        let mut malformed = values.clone();
        match value {
            Some(value) => {
                malformed.insert(key.to_owned(), value.to_owned());
            }
            None => {
                malformed.remove(key);
            }
        }
        assert!(pci_diagnostics(&|name| malformed.get(name).cloned()).is_err());
    }
}

#[test]
fn private_dri_inventory_is_sorted_and_complete() {
    let directory = std::env::temp_dir().join(format!(
        "lom-render-admission-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("renderD128"), []).unwrap();
    std::fs::write(directory.join("card0"), []).unwrap();
    assert_eq!(
        visible_dri_entries(&directory).unwrap(),
        ["card0".to_owned(), "renderD128".to_owned()]
    );
    std::fs::remove_dir_all(directory).unwrap();
}
