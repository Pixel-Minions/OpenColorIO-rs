// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Sony cameras: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/SonyCameras.cpp:64-143 @ v2.5.2), each entry's style and
//! description in upstream's order. Their ops are not ported yet: each creator returns an
//! error until its builder lands (WP 3.2, and `p3-after-p2` for the fixed functions).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// Registers the 4 built-in transforms of Sony cameras.
///
/// Port of `CAMERA::SONY::RegisterAll` (SonyCameras.cpp:64-143 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // SonyCameras.cpp:77
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3 to ACES2065-1"),
        not_ported_yet(b"SONY_SLOG3-SGAMUT3_to_ACES2065-1"),
    );
    // SonyCameras.cpp:93
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3.Cine to ACES2065-1"),
        not_ported_yet(b"SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1"),
    );
    // SonyCameras.cpp:116
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3 for the Venice camera to ACES2065-1"),
        not_ported_yet(b"SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1"),
    );
    // SonyCameras.cpp:139
    registry.add_builtin(
        b"SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1",
        Some(b"Convert Sony S-Log3 S-Gamut3.Cine for the Venice camera to ACES2065-1"),
        not_ported_yet(b"SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1"),
    );
}
