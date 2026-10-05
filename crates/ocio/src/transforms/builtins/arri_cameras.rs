// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of ARRI cameras: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/ArriCameras.cpp:75-108 @ v2.5.2), each entry's style and
//! description in upstream's order. Their ops are not ported yet: each creator returns an
//! error until its builder lands (WP 3.2, and `p3-after-p2` for the fixed functions).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// Registers the 2 built-in transforms of ARRI cameras.
///
/// Port of `CAMERA::ARRI::RegisterAll` (ArriCameras.cpp:75-108 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // ArriCameras.cpp:88
    registry.add_builtin(
        b"ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1",
        Some(b"Convert ARRI ALEXA LogC (EI800) ALEXA Wide Gamut to ACES2065-1"),
        not_ported_yet(b"ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1"),
    );
    // ArriCameras.cpp:104
    registry.add_builtin(
        b"ARRI_LOGC4_to_ACES2065-1",
        Some(b"Convert ARRI LogC4 to ACES2065-1"),
        not_ported_yet(b"ARRI_LOGC4_to_ACES2065-1"),
    );
}
