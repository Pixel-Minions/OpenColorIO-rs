// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The process's fixed list of monitors and ICC profiles (`SystemMonitor.cpp` @ v2.5.2).

use std::sync::OnceLock;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::utils::string_utils::c_str;

#[derive(Debug)]
pub(crate) struct MonitorInfo {
    pub(crate) name: Vec<u8>,
    pub(crate) profile: Vec<u8>,
}

/// The operating system's active monitors that have an ICC profile. The list is populated
/// once; subsequent hardware changes are not seen, as in OCIO 2.5.2 (owner decision P9-3).
#[derive(Debug)]
pub struct SystemMonitors {
    monitors: Vec<MonitorInfo>,
}

impl SystemMonitors {
    /// Port of `SystemMonitors::Get` (src/OpenColorIO/SystemMonitor.cpp:56-78 @ v2.5.2).
    #[doc(alias = "Get")]
    pub fn get() -> &'static Self {
        static MONITORS: OnceLock<SystemMonitors> = OnceLock::new();
        MONITORS.get_or_init(|| {
            #[cfg(windows)]
            let monitors = {
                let mut monitors = Vec::new();
                if let Err(error) = crate::system_monitor_windows::get_all_monitors(&mut monitors) {
                    ocio_ops::logging::log_debug(error.what());
                }
                monitors
            };
            // Linux's getAllMonitors is empty (SystemMonitor.cpp:42-46). macOS is a later
            // platform, owner decision P9-5; its separate source is deliberately unported.
            #[cfg(not(windows))]
            let monitors = Vec::new();
            SystemMonitors { monitors }
        })
    }

    /// Port of `SystemMonitorsImpl::isSupported` (src/OpenColorIO/SystemMonitor.cpp:80-83
    /// @ v2.5.2).
    #[doc(alias = "isSupported")]
    pub fn is_supported(&self) -> bool {
        !self.monitors.is_empty()
    }

    /// Port of `SystemMonitorsImpl::getNumMonitors` (src/OpenColorIO/SystemMonitor.cpp:85-88
    /// @ v2.5.2).
    #[doc(alias = "getNumMonitors")]
    pub fn num_monitors(&self) -> usize {
        self.monitors.len()
    }

    /// Port of `SystemMonitorsImpl::getMonitorName` (src/OpenColorIO/SystemMonitor.cpp:
    /// 90-101 @ v2.5.2).
    #[doc(alias = "getMonitorName")]
    pub fn monitor_name(&self, index: usize) -> Result<&[u8]> {
        self.monitors
            .get(index)
            .map(|monitor| monitor.name.as_slice())
            .ok_or_else(|| self.index_error(index))
    }

    /// Port of `SystemMonitorsImpl::getProfileFilepath` (src/OpenColorIO/SystemMonitor.cpp:
    /// 103-114 @ v2.5.2), including the same "monitor name" error as getMonitorName.
    #[doc(alias = "getProfileFilepath")]
    pub fn profile_filepath(&self, index: usize) -> Result<&[u8]> {
        self.monitors
            .get(index)
            .map(|monitor| monitor.profile.as_slice())
            .ok_or_else(|| self.index_error(index))
    }

    /// The common exception in the two index getters (SystemMonitor.cpp:94-98,107-111).
    fn index_error(&self, index: usize) -> Exception {
        Exception::new(format!(
            "Invalid index for the monitor name {index} where the number of monitors is {}.",
            self.monitors.len()
        ))
    }

    /// Port of `SystemMonitorsImpl::GetICCProfileFromMonitorName`
    /// (src/OpenColorIO/SystemMonitor.cpp:116-129 @ v2.5.2): byte-exact, case-sensitive strcmp.
    pub(crate) fn icc_profile_from_monitor_name(name: &[u8]) -> Result<&'static [u8]> {
        let name = c_str(name);
        for monitor in &Self::get().monitors {
            if monitor.name.as_slice() == name {
                return Ok(&monitor.profile);
            }
        }
        Err(Exception::new(
            [b"The monitor name '".as_slice(), name, b"' does not exist."].concat(),
        ))
    }
}
