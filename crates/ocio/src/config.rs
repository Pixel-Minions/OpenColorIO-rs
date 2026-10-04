// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config: a port of the parts of `src/OpenColorIO/Config.cpp` @ v2.5.2 that the transforms
//! need so far. `Config::CreateRaw`'s full state and `getProcessor` come with WP 1.8g; the rest
//! of the config (YAML, color spaces, displays, looks) is Phase 3.

use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};

use crate::context::Context;

/// A config: so far the version the op builders read (`BuildCDLOp` and `BuildExponentOp` build
/// version 1 configs' ops differently) and the current context.
///
/// Port of `Config` and `Config::Impl` (include/OpenColorIO/OpenColorIO.h:285,
/// src/OpenColorIO/Config.cpp @ v2.5.2), in part.
#[derive(Debug, Clone)]
pub struct Config {
    /// `m_majorVersion`.
    major_version: u32,
    /// `m_minorVersion`.
    minor_version: u32,
    /// `m_context`.
    context: Arc<Context>,
}

impl Config {
    /// The raw config: version 2.0, its one color space `raw`, its roles and its display.
    /// So far only the version and the context; the rest of its state comes with WP 1.8g,
    /// built directly until the YAML reader (Phase 3.3) parses upstream's profile.
    ///
    /// Port of `Config::CreateRaw` (src/OpenColorIO/Config.cpp:74-92, 1127-1133 @ v2.5.2), in
    /// part.
    #[doc(alias = "CreateRaw")]
    pub fn create_raw() -> Arc<Config> {
        Arc::new(Config {
            major_version: 2,
            minor_version: 0,
            context: Arc::new(Context::new()),
        })
    }

    /// Port of `Config::getMajorVersion` (src/OpenColorIO/Config.cpp:1280-1283 @ v2.5.2).
    #[doc(alias = "getMajorVersion")]
    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Sets the major version, and the minor version to the last one this release supports for
    /// it: "The version is <v> where supported versions start at 1 and end at 2." outside them.
    /// Upstream also resets the config's cache IDs; the port's config has none yet (WP 1.8g).
    ///
    /// Port of `Config::setMajorVersion` (src/OpenColorIO/Config.cpp:1285-1304 @ v2.5.2), with
    /// `FirstSupportedMajorVersion`, `LastSupportedMajorVersion` and `LastSupportedMinorVersion`
    /// (Config.cpp:245-251).
    #[doc(alias = "setMajorVersion")]
    pub fn set_major_version(&mut self, version: u32) -> Result<()> {
        /// `FirstSupportedMajorVersion`.
        const FIRST_SUPPORTED_MAJOR_VERSION: u32 = 1;
        /// `LastSupportedMajorVersion`: `OCIO_VERSION_MAJOR`.
        const LAST_SUPPORTED_MAJOR_VERSION: u32 = 2;
        /// `LastSupportedMinorVersion`: for each major version, the most recent minor.
        const LAST_SUPPORTED_MINOR_VERSION: [u32; 2] = [0, 5];

        if !(FIRST_SUPPORTED_MAJOR_VERSION..=LAST_SUPPORTED_MAJOR_VERSION).contains(&version) {
            return Err(Exception::new(format!(
                "The version is {version} where supported versions start at \
                 {FIRST_SUPPORTED_MAJOR_VERSION} and end at {LAST_SUPPORTED_MAJOR_VERSION}."
            )));
        }
        self.major_version = version;
        self.minor_version = LAST_SUPPORTED_MINOR_VERSION[(version - 1) as usize];
        Ok(())
    }

    /// Port of `Config::getMinorVersion` (src/OpenColorIO/Config.cpp:1306-1309 @ v2.5.2).
    #[doc(alias = "getMinorVersion")]
    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// Port of `Config::getCurrentContext` (src/OpenColorIO/Config.cpp:2161-2164 @ v2.5.2).
    #[doc(alias = "getCurrentContext")]
    pub fn current_context(&self) -> &Arc<Context> {
        &self.context
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
