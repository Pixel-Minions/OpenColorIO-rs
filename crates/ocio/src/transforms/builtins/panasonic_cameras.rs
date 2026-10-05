// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Panasonic cameras: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/PanasonicCameras.cpp:58-75 @ v2.5.2), each entry's style and
//! description in upstream's order. Their ops are not ported yet: each creator returns an
//! error until its builder lands (WP 3.2, and `p3-after-p2` for the fixed functions).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// Registers the 1 built-in transforms of Panasonic cameras.
///
/// Port of `CAMERA::PANASONIC::RegisterAll` (PanasonicCameras.cpp:58-75 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // PanasonicCameras.cpp:71
    registry.add_builtin(
        b"PANASONIC_VLOG-VGAMUT_to_ACES2065-1",
        Some(b"Convert Panasonic Varicam V-Log V-Gamut to ACES2065-1"),
        not_ported_yet(b"PANASONIC_VLOG-VGAMUT_to_ACES2065-1"),
    );
}
