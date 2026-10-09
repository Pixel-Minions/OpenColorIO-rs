// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms of displays: a port of `RegisterAll`
//! (src/OpenColorIO/transforms/builtins/Displays.cpp:177-556 @ v2.5.2), each entry's style and
//! description in upstream's order. The SDR displays build their ops (a Matrix or Scale op,
//! then a Gamma op); the PQ and HLG curves are half-domain LUTs (the wheel is built with
//! `OCIO_LUT_SUPPORT`), and the HLG display adds the Rec.2100 surround fixed function.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::fixedfunction::fixed_function_op::create_fixed_function_op;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpStyle;
use ocio_ops::ops::gamma::gamma_op::create_gamma_op;
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::matrix::matrix_op::{create_matrix_op_from_array, create_scale_op};

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, OpCreator,
};
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Primaries, build_conversion_matrix_from_xyz_d65, p3_d60, p3_d65, p3_dci,
    rec709, rec2020,
};
use crate::transforms::builtins::op_helpers::create_half_lut;

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

/// SMPTE ST-2084 (PQ), as `double`s: `ST_2084` (Displays.cpp:30-37 @ v2.5.2).
mod st_2084 {
    pub(super) const M1: f64 = 0.25 * 2610. / 4096.;
    pub(super) const M2: f64 = 128. * 2523. / 4096.;
    pub(super) const C2: f64 = 32. * 2413. / 4096.;
    pub(super) const C3: f64 = 32. * 2392. / 4096.;
    pub(super) const C1: f64 = C3 - C2 + 1.;
}

/// C++ `std::max(a, b)`: `(a < b) ? b : a`.
fn std_max(a: f64, b: f64) -> f64 {
    if a < b { b } else { a }
}

/// PQ to linear nits/100, mirrored about 0, as a half-domain LUT.
///
/// Port of `ST_2084::GeneratePQToLinearOps` (Displays.cpp:39-54 @ v2.5.2).
fn generate_pq_to_linear_ops(ops: &mut OpVec) -> Result<()> {
    use st_2084::*;
    let generate_lut_values = |input: f64| -> f32 {
        let n = input.abs(); // mirror about 0
        let x = n.powf(1. / M2);
        let mut l = (std_max(0., x - C1) / (C2 - C3 * x)).powf(1. / M1);
        // L is in nits/10000, convert to nits/100.
        l *= 100.;

        l.copysign(input) as f32
    };

    create_half_lut(ops, generate_lut_values)
}

/// Linear nits/100 to PQ, mirrored about 0, as a half-domain LUT.
///
/// Port of `ST_2084::GenerateLinearToPQOps` (Displays.cpp:56-70 @ v2.5.2).
fn generate_linear_to_pq_ops(ops: &mut OpVec) -> Result<()> {
    use st_2084::*;
    let generate_lut_values = |input: f64| -> f32 {
        // Input is in nits/100, convert to [0,1], where 1 is 10000 nits.
        let l = (input * 0.01).abs();
        let y = l.powf(M1);
        let ratpoly = (C1 + C2 * y) / (1. + C3 * y);
        let n = std_max(0., ratpoly).powf(M2);

        n.copysign(input) as f32
    };

    create_half_lut(ops, generate_lut_values)
}

/// ITU-R BT.2100 HLG, as `double`s: `HLG` (Displays.cpp:91-102 @ v2.5.2). `c0` and `c` are
/// computed at run time, as upstream's `static const`s are.
mod hlg {
    pub(super) const LW: f64 = 1000.;
    pub(super) const E_MAX: f64 = 3.;

    pub(super) const A: f64 = 0.17883277;
    pub(super) const B: f64 = (1. - 4. * A) * E_MAX / 12.;
    pub(super) fn c0() -> f64 {
        0.5 - A * (4. * A).ln()
    }
    pub(super) fn c() -> f64 {
        (12. / E_MAX).ln() * A + c0()
    }
    pub(super) const E_SCALE: f64 = 3. / E_MAX;
    pub(super) const E_BREAK: f64 = E_MAX / 12.;
}

/// HLG to linear, mirrored about 0, as a half-domain LUT.
///
/// Port of `HLG::GenerateHLGToLinearOps` (Displays.cpp:106-121 @ v2.5.2).
fn generate_hlg_to_linear_ops(ops: &mut OpVec) -> Result<()> {
    use hlg::*;
    let c = c();
    let generate_lut_values = |in_: f64| -> f32 {
        let e_prime = in_.abs(); // mirror about 0
        let out = if e_prime < 0.5 {
            e_prime * e_prime / E_SCALE
        } else {
            B + ((e_prime - c) / A).exp()
        };
        out.copysign(in_) as f32
    };

    create_half_lut(ops, generate_lut_values)
}

/// Linear to HLG, mirrored about 0, as a half-domain LUT.
///
/// Port of `HLG::GenerateLinearToHLGOps` (Displays.cpp:123-141 @ v2.5.2).
fn generate_linear_to_hlg_ops(ops: &mut OpVec) -> Result<()> {
    use hlg::*;
    let c = c();
    let generate_lut_values = |in_: f64| -> f32 {
        let e = in_.abs(); // mirror about 0
        let out = if e < E_BREAK {
            (e * E_SCALE).sqrt()
        } else {
            A * (e - B).ln() + c
        };
        out.copysign(in_) as f32
    };

    create_half_lut(ops, generate_lut_values)
}

/// CIE XYZ (D65 white) to `primaries` without adaptation, then linear to PQ: the PQ displays.
///
/// Port of `CIE_XYZ_D65_to_REC2100_PQ_Functor` and `CIE_XYZ_D65_to_ST2084_P3_D65_Functor`
/// (Displays.cpp:465-472, 480-487 @ v2.5.2).
fn matrix_then_pq(ops: &mut OpVec, primaries: &Primaries) -> Result<()> {
    let matrix = build_conversion_matrix_from_xyz_d65(primaries, AdaptationMethod::None)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);

    generate_linear_to_pq_ops(ops)
}

/// Port of `CIE_XYZ_D65_to_REC2100_HLG_1000nit_Functor` (Displays.cpp:529-549 @ v2.5.2).
fn cie_xyz_d65_to_rec2100_hlg_1000nit(ops: &mut OpVec) -> Result<()> {
    let matrix = build_conversion_matrix_from_xyz_d65(&rec2020::PRIMARIES, AdaptationMethod::None)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);

    let gamma = 1.2 + 0.42 * (hlg::LW / 1000.).log10();
    {
        const SCALE: f64 = 100.;
        const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];
        create_scale_op(ops, &SCALE4, TransformDirection::Forward);
    }
    {
        let scale = hlg::E_MAX.powf(gamma) / hlg::LW;
        let scale4 = [scale, scale, scale, 1.];
        create_scale_op(ops, &scale4, TransformDirection::Forward);
    }

    create_fixed_function_op(
        ops,
        FixedFunctionOpStyle::Rec2100SurroundFwd,
        &vec![1. / gamma],
    )?;

    generate_linear_to_hlg_ops(ops)
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
        creator(generate_pq_to_linear_ops),
    );
    // Displays.cpp:459
    registry.add_builtin(
        b"CURVE - LINEAR_to_ST-2084",
        Some(b"Convert linear nits/100 to SMPTE ST-2084 (PQ) full-range"),
        creator(generate_linear_to_pq_ops),
    );
    // Displays.cpp:474
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ",
        Some(b"Convert CIE XYZ (D65 white) to Rec.2100-PQ"),
        Arc::new(|ops: &mut OpVec| matrix_then_pq(ops, &rec2020::PRIMARIES)),
    );
    // Displays.cpp:489
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65",
        Some(b"Convert CIE XYZ (D65 white) to ST-2084 (PQ), P3-D65 primaries"),
        Arc::new(|ops: &mut OpVec| matrix_then_pq(ops, &p3_d65::PRIMARIES)),
    );
    // Displays.cpp:500
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65",
        Some(b"Convert CIE XYZ (D65 white) to ST-2084 (PQ) (D65 white in XYZ-E encoding)"),
        creator(generate_linear_to_pq_ops),
    );
    // Displays.cpp:511
    registry.add_builtin(
        b"CURVE - HLG-OETF-INVERSE",
        Some(b"Apply ITU-R BT.2100 (HLG) OETF inverse, scaled with HLG 0.42 at 18% grey"),
        creator(generate_hlg_to_linear_ops),
    );
    // Displays.cpp:522
    registry.add_builtin(
        b"CURVE - HLG-OETF",
        Some(b"Apply ITU-R BT.2100 (HLG) OETF, scaled with 18% grey at HLG 0.42"),
        creator(generate_linear_to_hlg_ops),
    );
    // Displays.cpp:551
    registry.add_builtin(
        b"DISPLAY - CIE-XYZ-D65_to_REC.2100-HLG-1000nit",
        Some(b"Convert CIE XYZ (D65 white) to Rec.2100-HLG, 1000 nit"),
        creator(cie_xyz_d65_to_rec2100_hlg_1000nit),
    );
}
