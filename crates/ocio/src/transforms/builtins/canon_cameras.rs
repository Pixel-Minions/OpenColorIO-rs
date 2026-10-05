// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of Canon cameras: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/CanonCameras.cpp:146-197 @ v2.5.2), each entry's style and
//! description in upstream's order. Their ops are not ported yet: each creator returns an
//! error until its builder lands (WP 3.2, and `p3-after-p2` for the fixed functions).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// Registers the 4 built-in transforms of Canon cameras.
///
/// Port of `CAMERA::CANON::RegisterAll` (CanonCameras.cpp:146-197 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // CanonCameras.cpp:158
    registry.add_builtin(
        b"CANON_CLOG2-CGAMUT_to_ACES2065-1",
        Some(b"Convert Canon Log 2 Cinema Gamut to ACES2065-1"),
        not_ported_yet(b"CANON_CLOG2-CGAMUT_to_ACES2065-1"),
    );
    // CanonCameras.cpp:168
    registry.add_builtin(
        b"CURVE - CANON_CLOG2_to_LINEAR",
        Some(b"Convert Canon Log 2 to linear"),
        not_ported_yet(b"CURVE - CANON_CLOG2_to_LINEAR"),
    );
    // CanonCameras.cpp:183
    registry.add_builtin(
        b"CANON_CLOG3-CGAMUT_to_ACES2065-1",
        Some(b"Convert Canon Log 3 Cinema Gamut to ACES2065-1"),
        not_ported_yet(b"CANON_CLOG3-CGAMUT_to_ACES2065-1"),
    );
    // CanonCameras.cpp:193
    registry.add_builtin(
        b"CURVE - CANON_CLOG3_to_LINEAR",
        Some(b"Convert Canon Log 3 to linear"),
        not_ported_yet(b"CURVE - CANON_CLOG3_to_LINEAR"),
    );
}
