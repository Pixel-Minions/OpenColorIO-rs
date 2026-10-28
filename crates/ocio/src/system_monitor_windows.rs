// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Windows monitor enumeration (SystemMonitor_windows.cpp @ v2.5.2). All OS calls are in the
//! owner-approved FFI file; this module preserves upstream's enumeration and naming order.

use ocio_ops::exception::{Exception, Result};
use ocio_ops::logging::log_debug;
use ocio_ops::platform::utf16_to_utf8_lossy;

use crate::system_monitor::MonitorInfo;
use crate::system_monitor_windows_ffi as os;

fn wide_string(text: &[u16]) -> &[u16] {
    &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())]
}

/// Port of `getAllMonitorsWithQueryDisplayConfig`
/// (src/OpenColorIO/SystemMonitor_windows.cpp:26-85 @ v2.5.2).
fn get_all_monitors_with_query_display_config() -> Result<Vec<Vec<u16>>> {
    let (mut paths, mut modes) = (Vec::new(), Vec::new());
    let result = loop {
        // Upstream ignores an error then uses uninitialized counts. Fail deliberately at
        // this boundary instead (U-306); SystemMonitors::get logs and retains its empty list.
        let (path_count, mode_count) = os::display_config_buffer_sizes().map_err(|_| {
            Exception::new("Problem obtaining monitor profile information from operating system.")
        })?;
        paths.resize(path_count as usize, os::PathInfo::default());
        modes.resize(mode_count as usize, os::ModeInfo::default());
        let (result, path_count, mode_count) = os::query_display_config(&mut paths, &mut modes);
        paths.resize(path_count as usize, os::PathInfo::default());
        modes.resize(mode_count as usize, os::ModeInfo::default());
        if result != os::ERROR_INSUFFICIENT_BUFFER {
            break result;
        }
    };
    let mut names = Vec::new();
    if result == 0 {
        for path in paths {
            if let Some(target) = os::target_device_name(&path.target_info) {
                names.push(if target.flags & 1 != 0 {
                    wide_string(&target.monitor_friendly_device_name).to_vec()
                } else {
                    Vec::new()
                });
            }
        }
    }
    Ok(names)
}

/// Port of `SystemMonitorsImpl::getAllMonitors`
/// (src/OpenColorIO/SystemMonitor_windows.cpp:98-194 @ v2.5.2).
pub(crate) fn get_all_monitors(monitors: &mut Vec<MonitorInfo>) -> Result<()> {
    monitors.clear();
    let friendly_names = get_all_monitors_with_query_display_config()?;
    let mut device = os::DisplayDevice::default();
    let mut number = 0_u32;
    while os::enum_display_devices(None, number, &mut device) {
        let device_name = wide_string(&device.device_name).to_vec();
        let active = os::DISPLAY_DEVICE_ACTIVE | os::DISPLAY_DEVICE_ATTACHED_TO_DESKTOP;
        if device.state_flags & active == active {
            if let Some(dc) = os::DisplayDc::create(&device_name) {
                device = os::DisplayDevice::default();
                os::enum_display_devices(Some(&device_name), 0, &mut device);
                // I-541: source indexes the query's active paths by EnumDisplayDevices'
                // adapter number. Preserve that cross-list indexing, including its fallback.
                let extra = friendly_names
                    .get(number as usize)
                    .filter(|n| !n.is_empty())
                    .map_or_else(|| wide_string(&device.device_string), Vec::as_slice);
                let stripped = if utf16_to_utf8_lossy(&device_name).starts_with(b"\\\\.\\DISPLAY") {
                    &device_name[4..]
                } else {
                    device_name.as_slice()
                };
                let mut display_name = stripped.to_vec();
                display_name.extend([u16::from(b','), u16::from(b' ')]);
                display_name.extend_from_slice(extra);
                let display_name = utf16_to_utf8_lossy(&display_name);
                if let Some(profile) = dc.icm_profile() {
                    monitors.push(MonitorInfo {
                        name: display_name,
                        profile: utf16_to_utf8_lossy(wide_string(&profile)),
                    });
                } else {
                    log_debug(
                        [
                            b"Unable to access the ICM profile for the monitor '".as_slice(),
                            &display_name,
                            b"'.",
                        ]
                        .concat(),
                    );
                }
            } else {
                log_debug(
                    [
                        b"Unable to access the monitor '".as_slice(),
                        &utf16_to_utf8_lossy(&device_name),
                        b"'.",
                    ]
                    .concat(),
                );
            }
        }
        device = os::DisplayDevice::default();
        number = number.wrapping_add(1);
    }
    Ok(())
}
