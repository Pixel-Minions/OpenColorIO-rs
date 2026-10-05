// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ACES built-in transforms: a port of `ACES::RegisterAll`
//! (src/OpenColorIO/transforms/builtins/ACES.cpp:576-1437 @ v2.5.2), each entry's style and
//! description in upstream's order, the ACES 2.0 output transforms' table included. Their ops
//! are not ported yet: each creator returns an error until its builder lands (WP 3.2g, and
//! `p3-after-p2` for the ACES 1.x and 2.0 output transforms).

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, not_ported_yet,
};

/// An ACES 2.0 output transform of the table: its style and description. The parameters of
/// its ops (peak luminance, limiting and encoding primaries, linear scale, white scaling) come
/// with them.
///
/// Port of `ACES2OutputTransform` (ACES.cpp:1122-1131 @ v2.5.2), without its parameters yet.
struct Aces2OutputTransform {
    /// `name`.
    name: &'static [u8],
    /// `desc`.
    desc: &'static [u8],
}

/// `aces2_output_transforms` (ACES.cpp:1133-1419 @ v2.5.2): D65, then D60 simulated.
const ACES2_OUTPUT_TRANSFORMS: [Aces2OutputTransform; 31] = [
    // ACES.cpp:1138
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709",
    },
    // ACES.cpp:1147
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR P3-D65",
    },
    // ACES.cpp:1156
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 108 nit HDR P3-D65",
    },
    // ACES.cpp:1165
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 300 nit HDR P3-D65",
    },
    // ACES.cpp:1174
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR P3-D65",
    },
    // ACES.cpp:1183
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR P3-D65",
    },
    // ACES.cpp:1192
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR P3-D65",
    },
    // ACES.cpp:1201
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR P3-D65",
    },
    // ACES.cpp:1210
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR Rec2020",
    },
    // ACES.cpp:1219
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR Rec2020",
    },
    // ACES.cpp:1228
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR Rec2020",
    },
    // ACES.cpp:1237
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR Rec2020",
    },
    // ACES.cpp:1249
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC709-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in Rec709",
    },
    // ACES.cpp:1258
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in P3-D65",
    },
    // ACES.cpp:1267
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1276
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR P3-D60 simulating D60 white in P3-D65",
    },
    // ACES.cpp:1285
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-XYZ-E_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR P3-D60 simulating D60 white in XYZ-E",
    },
    // ACES.cpp:1294
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 108 nit HDR P3-D60 simulating D60 white in P3-D65",
    },
    // ACES.cpp:1303
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D60-in-XYZ-E_2.0",
        desc: b"Component of ACES 2 Output Transforms for 300 nit HDR P3-D60 simulating D60 white in XYZ-E",
    },
    // ACES.cpp:1312
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR P3-D60 simulating D60 white in P3-D65",
    },
    // ACES.cpp:1321
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR P3-D60 simulating D60 white in P3-D65",
    },
    // ACES.cpp:1330
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR P3-D60 simulating D60 white in P3-D65",
    },
    // ACES.cpp:1339
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR P3-D60 simulating D60 white in P3-D65",
    },
    // ACES.cpp:1348
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR P3-D60 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1357
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR P3-D60 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1366
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR P3-D60 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1375
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR P3-D60 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1384
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR Rec2020 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1393
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR Rec2020 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1402
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR Rec2020 simulating D60 white in Rec2020",
    },
    // ACES.cpp:1411
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR Rec2020 simulating D60 white in Rec2020",
    },
];

/// Registers the 59 ACES built-in transforms: 28 one by one, then the ACES 2.0
/// output transforms of the table.
///
/// Port of `ACES::RegisterAll` (ACES.cpp:576-1437 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // ACES.cpp:588
    registry.add_builtin(
        b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
        Some(b"Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation"),
        not_ported_yet(b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD"),
    );
    // ACES.cpp:598
    registry.add_builtin(
        b"UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD",
        Some(b"Convert ACES AP1 primaries to CIE XYZ with a D65 white point with Bradford adaptation"),
        not_ported_yet(b"UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD"),
    );
    // ACES.cpp:610
    registry.add_builtin(
        b"UTILITY - ACES-AP1_to_LINEAR-REC709_BFD",
        Some(b"Convert ACES AP1 primaries to linear Rec.709 primaries with Bradford adaptation"),
        not_ported_yet(b"UTILITY - ACES-AP1_to_LINEAR-REC709_BFD"),
    );
    // ACES.cpp:621
    registry.add_builtin(
        b"CURVE - ACEScct-LOG_to_LINEAR",
        Some(b"Apply the log-to-lin curve used in ACEScct"),
        not_ported_yet(b"CURVE - ACEScct-LOG_to_LINEAR"),
    );
    // ACES.cpp:636
    registry.add_builtin(
        b"ACEScct_to_ACES2065-1",
        Some(b"Convert ACEScct to ACES2065-1"),
        not_ported_yet(b"ACEScct_to_ACES2065-1"),
    );
    // ACES.cpp:686
    registry.add_builtin(
        b"ACEScc_to_ACES2065-1",
        Some(b"Convert ACEScc to ACES2065-1"),
        not_ported_yet(b"ACEScc_to_ACES2065-1"),
    );
    // ACES.cpp:698
    registry.add_builtin(
        b"ACEScg_to_ACES2065-1",
        Some(b"Convert ACEScg to ACES2065-1"),
        not_ported_yet(b"ACEScg_to_ACES2065-1"),
    );
    // ACES.cpp:717
    registry.add_builtin(
        b"ACESproxy10i_to_ACES2065-1",
        Some(b"Convert ACESproxy 10i to ACES2065-1"),
        not_ported_yet(b"ACESproxy10i_to_ACES2065-1"),
    );
    // ACES.cpp:737
    registry.add_builtin(
        b"ADX10_to_ACES2065-1",
        Some(b"Convert ADX10 to ACES2065-1"),
        not_ported_yet(b"ADX10_to_ACES2065-1"),
    );
    // ACES.cpp:757
    registry.add_builtin(
        b"ADX16_to_ACES2065-1",
        Some(b"Convert ADX16 to ACES2065-1"),
        not_ported_yet(b"ADX16_to_ACES2065-1"),
    );
    // ACES.cpp:775
    registry.add_builtin(
        b"ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX",
        Some(b"LMT for desaturating blue hues to reduce clipping artifacts"),
        not_ported_yet(b"ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX"),
    );
    // ACES.cpp:793
    registry.add_builtin(
        b"ACES-LMT - ACES 1.3 Reference Gamut Compression",
        Some(b"LMT (applied in ACES2065-1) to compress scene-referred values from common cameras into the AP1 gamut"),
        not_ported_yet(b"ACES-LMT - ACES 1.3 Reference Gamut Compression"),
    );
    // ACES.cpp:812
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA_1.0",
        Some(b"Component of ACES Output Transforms for SDR cinema"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA_1.0"),
    );
    // ACES.cpp:829
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0",
        Some(b"Component of ACES Output Transforms for SDR D65 video"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0"),
    );
    // ACES.cpp:844
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-REC709lim_1.1",
        Some(b"Component of ACES Output Transforms for SDR cinema"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-REC709lim_1.1"),
    );
    // ACES.cpp:861
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-REC709lim_1.1",
        Some(b"Component of ACES Output Transforms for SDR D65 video"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-REC709lim_1.1"),
    );
    // ACES.cpp:878
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for SDR D65 video"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1"),
    );
    // ACES.cpp:904
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-D65_1.1",
        Some(b"Component of ACES Output Transforms for SDR D65 cinema simulating D60 white"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-D65_1.1"),
    );
    // ACES.cpp:932
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-D60sim-D65_1.0",
        Some(b"Component of ACES Output Transforms for SDR D65 video simulating D60 white"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-D60sim-D65_1.0"),
    );
    // ACES.cpp:964
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-DCI_1.0",
        Some(b"Component of ACES Output Transforms for SDR DCI cinema simulating D60 white"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-DCI_1.0"),
    );
    // ACES.cpp:994
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D65sim-DCI_1.1",
        Some(b"Component of ACES Output Transforms for SDR DCI cinema simulating D65 white"),
        not_ported_yet(b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D65sim-DCI_1.1"),
    );
    // ACES.cpp:1011
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-REC2020lim_1.1",
        Some(b"Component of ACES Output Transforms for 1000 nit HDR D65 video"),
        not_ported_yet(
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-REC2020lim_1.1",
        ),
    );
    // ACES.cpp:1028
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 1000 nit HDR D65 video"),
        not_ported_yet(
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-P3lim_1.1",
        ),
    );
    // ACES.cpp:1045
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-REC2020lim_1.1",
        Some(b"Component of ACES Output Transforms for 2000 nit HDR D65 video"),
        not_ported_yet(
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-REC2020lim_1.1",
        ),
    );
    // ACES.cpp:1062
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 2000 nit HDR D65 video"),
        not_ported_yet(
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-P3lim_1.1",
        ),
    );
    // ACES.cpp:1079
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-REC2020lim_1.1",
        Some(b"Component of ACES Output Transforms for 4000 nit HDR D65 video"),
        not_ported_yet(
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-REC2020lim_1.1",
        ),
    );
    // ACES.cpp:1096
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 4000 nit HDR D65 video"),
        not_ported_yet(
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-P3lim_1.1",
        ),
    );
    // ACES.cpp:1113
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-CINEMA-108nit-7.2nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 108 nit HDR D65 cinema"),
        not_ported_yet(
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-CINEMA-108nit-7.2nit-P3lim_1.1",
        ),
    );

    for tr in &ACES2_OUTPUT_TRANSFORMS {
        registry.add_builtin(tr.name, Some(tr.desc), not_ported_yet(tr.name));
    }
}
