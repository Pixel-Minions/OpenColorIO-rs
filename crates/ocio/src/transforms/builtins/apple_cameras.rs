// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Apple cameras: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/AppleCameras.cpp:96-122 @ v2.5.2), each entry's style and
//! description in upstream's order. Their ops are not ported yet: each creator returns an
//! error until its builder lands (WP 3.2, and `p3-after-p2` for the fixed functions).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// Registers the 2 built-in transforms of Apple cameras.
///
/// Port of `CAMERA::APPLE::RegisterAll` (AppleCameras.cpp:96-122 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // AppleCameras.cpp:108
    registry.add_builtin(
        b"APPLE_LOG_to_ACES2065-1",
        Some(b"Convert Apple Log to ACES2065-1"),
        not_ported_yet(b"APPLE_LOG_to_ACES2065-1"),
    );
    // AppleCameras.cpp:118
    registry.add_builtin(
        b"CURVE - APPLE_LOG_to_LINEAR",
        Some(b"Convert Apple Log to linear"),
        not_ported_yet(b"CURVE - APPLE_LOG_to_LINEAR"),
    );
}
