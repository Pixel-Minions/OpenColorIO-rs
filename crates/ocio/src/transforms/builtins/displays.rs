// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of displays: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/Displays.cpp:177-556 @ v2.5.2), each entry's style and
//! description in upstream's order. The SDR displays build their ops (a Matrix or Scale op,
//! then a Gamma op); the PQ and HLG curves and displays, built on fixed functions or half-domain
//! LUTs, return an error until `p3-after-p2`.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::gamma::gamma_op::create_gamma_op;
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::matrix::matrix_op::{create_matrix_op_from_array, create_scale_op};

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, OpCreator, not_ported_yet,
};
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Primaries, build_conversion_matrix_from_xyz_d65, p3_d60, p3_d65, p3_dci,
    rec709, rec2020,
};

/// An entry's op creator.
fn creator(f: fn(&mut OpVec) -> Result<()>) -> OpCreator {
    Arc::new(f)
}

/// Appends a Gamma op of `style`, its R, G and B parameters `rgb_params` and its alpha ones
/// `alpha_params`, forward.
fn gamma(ops: &mut OpVec, style: GammaStyle, rgb_params: &[f64], alpha_params: &[f64]) {
    let gamma_data = GammaOpData::new(
        style,
        rgb_params.to_vec(),
        rgb_params.to_vec(),
        rgb_params.to_vec(),
        alpha_params.to_vec(),
    );
    create_gamma_op(ops, gamma_data, TransformDirection::Forward);
}

/// Appends CIE XYZ (D65 white) to `primaries` with the adaptation `method`, then a Gamma op:
/// the functors of the SDR displays (Displays.cpp:182-194, 216-227, 249-260, 282-293, 315-326,
/// 334-345, 367-378 @ v2.5.2).
fn matrix_then_gamma(
    ops: &mut OpVec,
    primaries: &Primaries,
    method: AdaptationMethod,
    style: GammaStyle,
    rgb_params: &[f64],
    alpha_params: &[f64],
) -> Result<()> {
    let matrix = build_conversion_matrix_from_xyz_d65(primaries, method)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);

    gamma(ops, style, rgb_params, alpha_params);
    Ok(())
}

/// CIE XYZ (D65 white) to DCDM: a scale, then a gamma of 2.6.
///
/// Port of `CIE_XYZ_D65_to_DCDM_D65_Functor` (Displays.cpp:386-397 @ v2.5.2).
fn cie_xyz_d65_to_dcdm_d65(ops: &mut OpVec) -> Result<()> {
    let scale = 48.0 / 52.37;
    let scale4 = [scale, scale, scale, 1.];
    create_scale_op(ops, &scale4, TransformDirection::Forward);

    gamma(ops, GammaStyle::BasicRev, &[2.6], &[1.0]);
    Ok(())
}

/// CIE XYZ (D65 white) to Apple Display P3: P3-D65, then the sRGB curve mirrored below 0 (as
/// macOS's extended Display P3 reflects it around 0).
///
/// Port of `CIE_XYZ_D65_to_DisplayP3_Functor` (Displays.cpp:405-429 @ v2.5.2).
fn cie_xyz_d65_to_display_p3(ops: &mut OpVec) -> Result<()> {
    matrix_then_gamma(
        ops,
        &p3_d65::PRIMARIES,
        AdaptationMethod::None,
        GammaStyle::MoncurveMirrorRev,
        &[2.4, 0.055],
        &[1.0, 0.0],
    )
}

/// Registers the 23 built-in transforms of displays.
///
/// Port of `DISPLAY::RegisterAll` (Displays.cpp:177-556 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // Displays.cpp:201
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.709, clamp neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec709::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicRev,
                &[2.4],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:210
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.709, mirror neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec709::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicMirrorRev,
                &[2.4],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:234
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.2020, clamp neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec2020::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicRev,
                &[2.4],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:243
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Rec.1886/Rec.2020, mirror neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec2020::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicMirrorRev,
                &[2.4],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:267
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709",
        Some(b"Convert CIE XYZ (D65 white) to Gamma2.2, Rec.709, clamp neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec709::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicRev,
                &[2.2],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:276
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Gamma2.2, Rec.709, mirror neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec709::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicMirrorRev,
                &[2.2],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:300
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_sRGB",
        Some(b"Convert CIE XYZ (D65 white) to sRGB (piecewise EOTF)"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec709::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::MoncurveRev,
                &[2.4, 0.055],
                &[1.0, 0.0],
            )
        }),
    );
    // Displays.cpp:309
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to sRGB (piecewise EOTF), mirror neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &rec709::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::MoncurveMirrorRev,
                &[2.4, 0.055],
                &[1.0, 0.0],
            )
        }),
    );
    // Displays.cpp:328
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-DCI (DCI white with Bradford adaptation)"),
        creator(|ops| matrix_then_gamma(ops, &p3_dci::PRIMARIES, AdaptationMethod::Bradford, GammaStyle::BasicRev, &[2.6], &[1.0])),
    );
    // Displays.cpp:352
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D65, clamp neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &p3_d65::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicRev,
                &[2.6],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:361
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D65, mirror neg. values"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &p3_d65::PRIMARIES,
                AdaptationMethod::None,
                GammaStyle::BasicMirrorRev,
                &[2.6],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:380
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D60-BFD",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6, P3-D60 (Bradford adaptation)"),
        creator(|ops| {
            matrix_then_gamma(
                ops,
                &p3_d60::PRIMARIES,
                AdaptationMethod::Bradford,
                GammaStyle::BasicRev,
                &[2.6],
                &[1.0],
            )
        }),
    );
    // Displays.cpp:399
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
        Some(b"Convert CIE XYZ (D65 white) to Gamma 2.6 (D65 white in XYZ-E encoding)"),
        creator(cie_xyz_d65_to_dcdm_d65),
    );
    // Displays.cpp:431
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_DisplayP3",
        Some(b"Convert CIE XYZ (D65 white) to Apple Display P3, mirror neg. values"),
        creator(cie_xyz_d65_to_display_p3),
    );
    // Displays.cpp:437
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
        Some(b"Convert CIE XYZ (D65 white) to Apple Display P3 (HDR), mirror neg. values"),
        creator(cie_xyz_d65_to_display_p3),
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
