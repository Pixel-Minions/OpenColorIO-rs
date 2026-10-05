// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of RED cameras: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/RedCameras.cpp:77-113 @ v2.5.2), each entry's style and
//! description in upstream's order. Their ops are not ported yet: each creator returns an
//! error until its builder lands (WP 3.2, and `p3-after-p2` for the fixed functions).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// Registers the 2 built-in transforms of RED cameras.
///
/// Port of `CAMERA::RED::RegisterAll` (RedCameras.cpp:77-113 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // RedCameras.cpp:92
    registry.add_builtin(
        b"RED_REDLOGFILM-RWG_to_ACES2065-1",
        Some(b"Convert RED LogFilm RED Wide Gamut to ACES2065-1"),
        not_ported_yet(b"RED_REDLOGFILM-RWG_to_ACES2065-1"),
    );
    // RedCameras.cpp:109
    registry.add_builtin(
        b"RED_LOG3G10-RWG_to_ACES2065-1",
        Some(b"Convert RED Log3G10 RED Wide Gamut to ACES2065-1"),
        not_ported_yet(b"RED_LOG3G10-RWG_to_ACES2065-1"),
    );
}
