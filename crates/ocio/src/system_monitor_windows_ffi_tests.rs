// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Win64 ABI layouts from the Windows SDK's wingdi.h/winnt.h definitions. The independent
//! platform-header probe target/monitor_layout.cpp prints the same size/alignment/offsets;
//! run it with MSVC before gating this platform-sensitive chunk (P9-3).

use super::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn display_configuration_structures_match_the_windows_sdk() {
    // Windows SDK10.0.22000.0: winnt.h LUID; wingdi.h:2739-2748,2846-2871,2931-3010,3057-3102.
    assert_eq!((size_of::<Luid>(), align_of::<Luid>()), (8, 4));
    assert_eq!(
        (size_of::<PathSourceInfo>(), align_of::<PathSourceInfo>()),
        (20, 4)
    );
    assert_eq!(
        (size_of::<PathTargetInfo>(), align_of::<PathTargetInfo>()),
        (48, 4)
    );
    assert_eq!((size_of::<PathInfo>(), align_of::<PathInfo>()), (72, 4));
    assert_eq!((size_of::<ModeInfo>(), align_of::<ModeInfo>()), (64, 8));
    assert_eq!(
        (
            size_of::<DeviceInfoHeader>(),
            align_of::<DeviceInfoHeader>()
        ),
        (20, 4)
    );
    assert_eq!(
        (
            size_of::<TargetDeviceName>(),
            align_of::<TargetDeviceName>()
        ),
        (420, 4)
    );
    assert_eq!(
        (size_of::<DisplayDevice>(), align_of::<DisplayDevice>()),
        (840, 4)
    );
    assert_eq!(offset_of!(PathInfo, target_info), 20);
    assert_eq!(offset_of!(PathInfo, flags), 68);
    assert_eq!(offset_of!(PathTargetInfo, id), 8);
    assert_eq!(offset_of!(PathTargetInfo, refresh_rate), 28);
    assert_eq!(offset_of!(PathTargetInfo, target_available), 40);
    assert_eq!(offset_of!(ModeInfo, mode), 16);
    assert_eq!(offset_of!(DeviceInfoHeader, adapter_id), 8);
    assert_eq!(offset_of!(DeviceInfoHeader, id), 16);
    assert_eq!(offset_of!(TargetDeviceName, flags), 20);
    assert_eq!(
        offset_of!(TargetDeviceName, monitor_friendly_device_name),
        36
    );
    assert_eq!(offset_of!(TargetDeviceName, monitor_device_path), 164);
    assert_eq!(offset_of!(DisplayDevice, device_name), 4);
    assert_eq!(offset_of!(DisplayDevice, device_string), 68);
    assert_eq!(offset_of!(DisplayDevice, state_flags), 324);
    assert_eq!(offset_of!(DisplayDevice, device_id), 328);
    assert_eq!(offset_of!(DisplayDevice, device_key), 584);
    assert_eq!(QUERY_FLAGS, 18);
    assert_eq!(ERROR_INSUFFICIENT_BUFFER, 122);
    assert_eq!(DISPLAY_DEVICE_ACTIVE, 1);
    assert_eq!(DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, 1);
}
