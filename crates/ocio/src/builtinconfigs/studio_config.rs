// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The registration of the Studio built-in configs: `StudioConfig.cpp` and `StudioConfig.h`
//! (src/OpenColorIO/builtinconfigs @ v2.5.2).

use super::builtin_config_registry::BuiltinConfigRegistry;
use super::studio::{
    STUDIO_CONFIG_V100_ACES_V13_OCIO_V21, STUDIO_CONFIG_V210_ACES_V13_OCIO_V23,
    STUDIO_CONFIG_V220_ACES_V13_OCIO_V24, STUDIO_CONFIG_V400_ACES_V20_OCIO_V25,
};

/// Adds the Studio configs to `registry`; only the latest is recommended.
///
/// Port of `STUDIOCONFIG::Register` (src/OpenColorIO/builtinconfigs/StudioConfig.cpp:18-49 @
/// v2.5.2).
pub(crate) fn register(registry: &mut BuiltinConfigRegistry) {
    // If a new built-in config is added, do not forget to update the
    // LATEST_STUDIO_BUILTIN_CONFIG_URI variable (in BuiltinConfigRegistry.cpp).

    registry.add_builtin(
        b"studio-config-v1.0.0_aces-v1.3_ocio-v2.1",
        b"Academy Color Encoding System - Studio Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
        STUDIO_CONFIG_V100_ACES_V13_OCIO_V21,
        false,
    );

    registry.add_builtin(
        b"studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
        b"Academy Color Encoding System - Studio Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
        STUDIO_CONFIG_V210_ACES_V13_OCIO_V23,
        false,
    );

    registry.add_builtin(
        b"studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
        b"Academy Color Encoding System - Studio Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
        STUDIO_CONFIG_V220_ACES_V13_OCIO_V24,
        false,
    );

    registry.add_builtin(
        b"studio-config-v4.0.0_aces-v2.0_ocio-v2.5",
        b"Academy Color Encoding System - Studio Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
        STUDIO_CONFIG_V400_ACES_V20_OCIO_V25,
        true,
    );
}
