// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The wide Windows calls made by SystemMonitor_windows.cpp @ v2.5.2. Owner decision P9-3
//! permits this single FFI file (no additional dependency). All buffers are initialized,
//! pointer lifetimes cover each call, and every returned DC is closed by its owner.
#![allow(unsafe_code)]

use std::ffi::c_void;

/// winerror.h and wingdi.h: QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE.
pub(crate) const QUERY_FLAGS: u32 = 0x2 | 0x10;
pub(crate) const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
pub(crate) const DISPLAY_DEVICE_ACTIVE: u32 = 0x1;
pub(crate) const DISPLAY_DEVICE_ATTACHED_TO_DESKTOP: u32 = 0x1;

/// winnt.h LUID, used by the display configuration structures in wingdi.h.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Luid {
    pub(crate) low: u32,
    pub(crate) high: i32,
}

/// wingdi.h DISPLAYCONFIG_PATH_SOURCE_INFO. Its mode-index union is one UINT32.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct PathSourceInfo {
    adapter_id: Luid,
    id: u32,
    mode_info: u32,
    status_flags: u32,
}

/// wingdi.h DISPLAYCONFIG_PATH_TARGET_INFO. Enum storage and its index union are UINT32;
/// BOOL is a 32-bit signed integer, and DISPLAYCONFIG_RATIONAL is two UINT32s.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct PathTargetInfo {
    pub(crate) adapter_id: Luid,
    pub(crate) id: u32,
    mode_info: u32,
    output_technology: u32,
    rotation: u32,
    scaling: u32,
    refresh_rate: [u32; 2],
    scan_line_ordering: u32,
    target_available: i32,
    status_flags: u32,
}

/// wingdi.h DISPLAYCONFIG_PATH_INFO.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct PathInfo {
    source_info: PathSourceInfo,
    pub(crate) target_info: PathTargetInfo,
    flags: u32,
}

/// wingdi.h DISPLAYCONFIG_MODE_INFO. The unread mode union's largest member is
/// DISPLAYCONFIG_TARGET_MODE (48 bytes, aligned as UINT64 pixelRate). All its bits are valid
/// in this opaque storage; the port never interprets a mode or a C enum as a Rust enum.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct ModeInfo {
    info_type: u32,
    id: u32,
    adapter_id: Luid,
    mode: [u64; 6],
}

/// wingdi.h DISPLAYCONFIG_DEVICE_INFO_HEADER.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct DeviceInfoHeader {
    type_: u32,
    size: u32,
    adapter_id: Luid,
    id: u32,
}

/// wingdi.h DISPLAYCONFIG_TARGET_DEVICE_NAME, including its UINT32 flags union.
#[repr(C)]
pub(crate) struct TargetDeviceName {
    header: DeviceInfoHeader,
    pub(crate) flags: u32,
    output_technology: u32,
    edid_manufacture_id: u16,
    edid_product_code_id: u16,
    connector_instance: u32,
    pub(crate) monitor_friendly_device_name: [u16; 64],
    monitor_device_path: [u16; 128],
}

/// wingdi.h DISPLAY_DEVICEW (CCHDEVICENAME =32, MAX_PATH =260).
#[repr(C)]
pub(crate) struct DisplayDevice {
    cb: u32,
    pub(crate) device_name: [u16; 32],
    pub(crate) device_string: [u16; 128],
    pub(crate) state_flags: u32,
    device_id: [u16; 128],
    device_key: [u16; 128],
}

impl Default for DisplayDevice {
    /// Port of the ZeroMemory/cb initialization (SystemMonitor_windows.cpp:106-108 @ v2.5.2).
    fn default() -> Self {
        Self {
            cb: size_of::<Self>() as u32,
            device_name: [0; 32],
            device_string: [0; 128],
            state_flags: 0,
            device_id: [0; 128],
            device_key: [0; 128],
        }
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetDisplayConfigBufferSizes(flags: u32, paths: *mut u32, modes: *mut u32) -> i32;
    fn QueryDisplayConfig(
        flags: u32,
        num_paths: *mut u32,
        paths: *mut PathInfo,
        num_modes: *mut u32,
        modes: *mut ModeInfo,
        topology: *mut u32,
    ) -> i32;
    fn DisplayConfigGetDeviceInfo(header: *mut DeviceInfoHeader) -> i32;
    fn EnumDisplayDevicesW(
        device: *const u16,
        number: u32,
        display: *mut DisplayDevice,
        flags: u32,
    ) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateDCW(
        driver: *const u16,
        device: *const u16,
        output: *const u16,
        mode: *const c_void,
    ) -> isize;
    fn GetICMProfileW(dc: isize, length: *mut u32, filename: *mut u16) -> i32;
    fn DeleteDC(dc: isize) -> i32;
}

/// Port of the GetDisplayConfigBufferSizes call (SystemMonitor_windows.cpp:42 @ v2.5.2).
pub(crate) fn display_config_buffer_sizes() -> std::result::Result<(u32, u32), i32> {
    let (mut paths, mut modes) = (0, 0);
    // SAFETY: both output pointers refer to live initialized UINT32s.
    let result = unsafe { GetDisplayConfigBufferSizes(QUERY_FLAGS, &mut paths, &mut modes) };
    if result == 0 {
        Ok((paths, modes))
    } else {
        Err(result)
    }
}

/// Port of QueryDisplayConfig (SystemMonitor_windows.cpp:50 @ v2.5.2).
pub(crate) fn query_display_config(
    paths: &mut [PathInfo],
    modes: &mut [ModeInfo],
) -> (i32, u32, u32) {
    let (mut path_count, mut mode_count) = (paths.len() as u32, modes.len() as u32);
    let paths_ptr = if paths.is_empty() {
        std::ptr::null_mut()
    } else {
        paths.as_mut_ptr()
    };
    let modes_ptr = if modes.is_empty() {
        std::ptr::null_mut()
    } else {
        modes.as_mut_ptr()
    };
    // SAFETY: the slices' lengths are exactly the UINT32 counts obtained from the OS; both
    // arrays are initialized and writable for those lengths. QDC_ONLY_ACTIVE_PATHS takes
    // no topology output. The pointers and counts live until the call returns.
    let result = unsafe {
        QueryDisplayConfig(
            QUERY_FLAGS,
            &mut path_count,
            paths_ptr,
            &mut mode_count,
            modes_ptr,
            std::ptr::null_mut(),
        )
    };
    (result, path_count, mode_count)
}

/// Port of DisplayConfigGetDeviceInfo (SystemMonitor_windows.cpp:67-82 @ v2.5.2).
pub(crate) fn target_device_name(target: &PathTargetInfo) -> Option<TargetDeviceName> {
    let mut name = TargetDeviceName {
        header: DeviceInfoHeader {
            type_: 2,
            size: size_of::<TargetDeviceName>() as u32,
            adapter_id: target.adapter_id,
            id: target.id,
        },
        flags: 0,
        output_technology: 0,
        edid_manufacture_id: 0,
        edid_product_code_id: 0,
        connector_instance: 0,
        monitor_friendly_device_name: [0; 64],
        monitor_device_path: [0; 128],
    };
    // SAFETY: the pointer derives from the whole initialized, writable target-name object,
    // retaining its full-object provenance for the size-based output. repr(C) puts header
    // first, and header.size describes the whole object. Type is GET_TARGET_NAME, and the
    // adapter/id came from the OS.
    (unsafe { DisplayConfigGetDeviceInfo((&raw mut name).cast::<DeviceInfoHeader>()) } == 0)
        .then_some(name)
}

fn terminated(text: &[u16]) -> Vec<u16> {
    text.iter()
        .copied()
        .take_while(|&unit| unit != 0)
        .chain(Some(0))
        .collect()
}

/// Port of EnumDisplayDevicesW (SystemMonitor_windows.cpp:113,132 @ v2.5.2).
pub(crate) fn enum_display_devices(
    device: Option<&[u16]>,
    number: u32,
    display: &mut DisplayDevice,
) -> bool {
    let device = device.map(terminated);
    // SAFETY: optional device is NUL-terminated and lives through the call. `display` is a
    // complete initialized DISPLAY_DEVICEW with cb set to its size, writable until return.
    unsafe {
        EnumDisplayDevicesW(
            device.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            number,
            display,
            0,
        ) != 0
    }
}

/// Owns the HDC returned by CreateDCW, released exactly once with DeleteDC.
pub(crate) struct DisplayDc(isize);

impl DisplayDc {
    /// Port of CreateDCW (SystemMonitor_windows.cpp:123 @ v2.5.2).
    pub(crate) fn create(device: &[u16]) -> Option<Self> {
        let device = terminated(device);
        // SAFETY: device is a live NUL-terminated UTF-16 string; all other arguments are
        // null as in upstream. A non-null returned HDC is uniquely owned by this object.
        let dc = unsafe {
            CreateDCW(
                std::ptr::null(),
                device.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if dc == 0 { None } else { Some(Self(dc)) }
    }

    /// Port of GetICMProfileW (SystemMonitor_windows.cpp:160 @ v2.5.2): MAX_PATH=260, while
    /// the allocated buffer holds MAX_PATH+1 UTF-16 units as in upstream.
    pub(crate) fn icm_profile(&self) -> Option<[u16; 261]> {
        let mut path = [0; 261];
        let mut length = 260;
        // SAFETY: this HDC is still owned, length begins at260, and path is writable for
        // 261 initialized UTF-16 units. GetICMProfileW writes a terminated name on success.
        (unsafe { GetICMProfileW(self.0, &mut length, path.as_mut_ptr()) } != 0).then_some(path)
    }
}

impl Drop for DisplayDc {
    /// Port of DeleteDC (SystemMonitor_windows.cpp:177 @ v2.5.2).
    fn drop(&mut self) {
        // SAFETY: this is the live HDC created above, closed exactly once by its owner.
        unsafe { DeleteDC(self.0) };
    }
}

#[cfg(test)]
#[path = "system_monitor_windows_ffi_tests.rs"]
mod tests;
