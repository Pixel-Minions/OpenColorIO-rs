// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The registration of the CG built-in configs: `CGConfig.cpp` and `CGConfig.h`
//! (src/OpenColorIO/builtinconfigs @ v2.5.2).

use super::builtin_config_registry::BuiltinConfigRegistry;
use super::cg::{
    CG_CONFIG_V100_ACES_V13_OCIO_V21, CG_CONFIG_V210_ACES_V13_OCIO_V23,
    CG_CONFIG_V220_ACES_V13_OCIO_V24, CG_CONFIG_V400_ACES_V20_OCIO_V25,
};

/// Adds the CG configs to `registry`; only the latest is recommended.
///
/// Port of `CGCONFIG::Register` (src/OpenColorIO/builtinconfigs/CGConfig.cpp:18-49 @ v2.5.2).
pub(crate) fn register(registry: &mut BuiltinConfigRegistry) {
    // If a new built-in config is added, do not forget to update the LATEST_CG_BUILTIN_CONFIG_URI
    // variable (in BuiltinConfigRegistry.cpp).

    registry.add_builtin(
        b"cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
        b"Academy Color Encoding System - CG Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
        CG_CONFIG_V100_ACES_V13_OCIO_V21,
        false,
    );

    registry.add_builtin(
        b"cg-config-v2.1.0_aces-v1.3_ocio-v2.3",
        b"Academy Color Encoding System - CG Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
        CG_CONFIG_V210_ACES_V13_OCIO_V23,
        false,
    );

    registry.add_builtin(
        b"cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
        b"Academy Color Encoding System - CG Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
        CG_CONFIG_V220_ACES_V13_OCIO_V24,
        false,
    );

    registry.add_builtin(
        b"cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
        b"Academy Color Encoding System - CG Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
        CG_CONFIG_V400_ACES_V20_OCIO_V25,
        true,
    );
}
