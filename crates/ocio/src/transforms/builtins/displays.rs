// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of displays: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/Displays.cpp:177-556 @ v2.5.2), each entry's style and
//! description in upstream's order. Their ops are not ported yet: each creator returns an
//! error until its builder lands (WP 3.2, and `p3-after-p2` for the fixed functions).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// Registers the 23 built-in transforms of displays.
///
/// Port of `DISPLAY::RegisterAll` (Displays.cpp:177-556 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // Displays.cpp:201
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.709, clamp neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709"),
    );
    // Displays.cpp:210
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.709, mirror neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS"),
    );
    // Displays.cpp:234
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.2020, clamp neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020"),
    );
    // Displays.cpp:243
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.2020, mirror neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS"),
    );
    // Displays.cpp:267
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709",
        Some(b"Convert CIE XYZ (D65 white) to Gamma2.2, Rec.709, clamp neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709"),
    );
    // Displays.cpp:276
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Gamma2.2, Rec.709, mirror neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS"),
    );
    // Displays.cpp:300
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_sRGB",
        Some(b"Convert CIE XYZ (D65 white) to sRGB (piecewise EOTF)"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_sRGB"),
    );
    // Displays.cpp:309
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to sRGB (piecewise EOTF), mirror neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS"),
    );
    // Displays.cpp:328
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-DCI (DCI white with Bradford adaptation)"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD"),
    );
    // Displays.cpp:352
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D65, clamp neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65"),
    );
    // Displays.cpp:361
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D65, mirror neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS"),
    );
    // Displays.cpp:380
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D60-BFD",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D60 (Bradford adaptation)"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D60-BFD"),
    );
    // Displays.cpp:399
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6 (D65 white in XYZ-E encoding)"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_DCDM-D65"),
    );
    // Displays.cpp:431
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_DisplayP3",
        Some(b"Convert CIE XYZ (D65 white) to Apple Display P3, mirror neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_DisplayP3"),
    );
    // Displays.cpp:437
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
        Some(b"Convert CIE XYZ (D65 white) to Apple Display P3 (HDR), mirror neg. values"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR"),
    );
    // Displays.cpp:448
    registry.add_builtin(
        b"CURVE - ST-2084_to_LINEAR",
        Some(b"Convert SMPTE ST-2084 (PQ) full-range to linear nits/100"),
        not_ported_yet(b"CURVE - ST-2084_to_LINEAR"),
    );
    // Displays.cpp:459
    registry.add_builtin(
        b"CURVE - LINEAR_to_ST-2084",
        Some(b"Convert linear nits/100 to SMPTE ST-2084 (PQ) full-range"),
        not_ported_yet(b"CURVE - LINEAR_to_ST-2084"),
    );
    // Displays.cpp:474
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ",
        Some(b"Convert CIE XYZ (D65 white) to Rec.2100-PQ"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ"),
    );
    // Displays.cpp:489
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65",
        Some(b"Convert CIE XYZ (D65 white) to ST-2084 (PQ), P3-D65 primaries"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65"),
    );
    // Displays.cpp:500
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65",
        Some(b"Convert CIE XYZ (D65 white) to ST-2084 (PQ) (D65 white in XYZ-E encoding)"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65"),
    );
    // Displays.cpp:511
    registry.add_builtin(
        b"CURVE - HLG-OETF-INVERSE",
        Some(b"Apply ITU-R BT.2100 (HLG) OETF inverse, scaled with HLG 0.42 at 18% grey"),
        not_ported_yet(b"CURVE - HLG-OETF-INVERSE"),
    );
    // Displays.cpp:522
    registry.add_builtin(
        b"CURVE - HLG-OETF",
        Some(b"Apply ITU-R BT.2100 (HLG) OETF, scaled with 18% grey at HLG 0.42"),
        not_ported_yet(b"CURVE - HLG-OETF"),
    );
    // Displays.cpp:551
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.2100-HLG-1000nit",
        Some(b"Convert CIE XYZ (D65 white) to Rec.2100-HLG, 1000 nit"),
        not_ported_yet(b"DISPLAY - CIE-XYZ-D65_to_REC.2100-HLG-1000nit"),
    );
}
