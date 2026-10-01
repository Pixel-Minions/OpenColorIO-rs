// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config: a port of the parts of `src/OpenColorIO/Config.cpp` @ v2.5.2 that the transforms
//! need so far. `Config::CreateRaw`'s full state and `getProcessor` come with WP 1.8g; the rest
//! of the config (YAML, color spaces, displays, looks) is Phase 3.

use std::sync::Arc;

use crate::context::Context;

/// A config: so far the version the op builders read (`BuildCDLOp` and `BuildExponentOp` build
/// version 1 configs' ops differently) and the current context.
///
/// Port of `Config` and `Config::Impl` (include/OpenColorIO/OpenColorIO.h,
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

    /// Port of `Config::getMajorVersion`.
    #[doc(alias = "getMajorVersion")]
    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Port of `Config::getMinorVersion`.
    #[doc(alias = "getMinorVersion")]
    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// Port of `Config::getCurrentContext`.
    #[doc(alias = "getCurrentContext")]
    pub fn current_context(&self) -> &Arc<Context> {
        &self.context
    }
}
