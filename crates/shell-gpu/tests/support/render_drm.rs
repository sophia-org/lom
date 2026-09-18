use super::*;

#[test]
fn vulkan_drm_properties_require_representable_kernel_device_numbers() {
    assert_eq!(
        identity_from_properties(true, 226, 129),
        Ok(DrmRenderIdentity {
            has_render: true,
            major: 226,
            minor: 129,
        })
    );
    assert!(identity_from_properties(true, -1, 129).is_err());
    assert!(identity_from_properties(true, 226, -1).is_err());
    assert!(identity_from_properties(true, i64::from(u32::MAX) + 1, 129).is_err());
    assert!(identity_from_properties(true, 226, i64::from(u32::MAX) + 1).is_err());
}
