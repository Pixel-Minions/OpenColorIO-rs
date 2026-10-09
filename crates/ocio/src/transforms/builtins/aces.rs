// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ACES built-in transforms: a port of `ACES::RegisterAll`
//! (src/OpenColorIO/transforms/builtins/ACES.cpp:576-1437 @ v2.5.2), each entry's style and
//! description in upstream's order, the ACES 2.0 output transforms' table included, and the
//! ops each entry builds.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::math_utils::std_pow;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{GradingStyle, TransformDirection};
use ocio_ops::ops::fixedfunction::fixed_function_op::create_fixed_function_op;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpStyle;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op::create_grading_rgb_curve_op;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_ops::ops::log::log_op::{create_log_op, create_log_op_from_base};
use ocio_ops::ops::log::log_op_data::LogOpData;
use ocio_ops::ops::matrix::matrix_op::{
    create_matrix_op_from_array, create_matrix_op_from_m44, create_scale_offset_op, create_scale_op,
};
use ocio_ops::ops::matrix::matrix_op_data::Offsets;
use ocio_ops::ops::range::range_op::create_range_op_from_values;
use ocio_ops::ops::range::range_op_data::RangeOpData;

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, OpCreator,
};
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, Primaries, aces_ap0, aces_ap1, build_conversion_matrix,
    build_conversion_matrix_to_xyz_d65, build_vonkries_adapt, cie_xyz_illum_e, p3_d60, p3_d65,
    rec709, rec709_d60, rec2020, rec2020_d60, rgb2xyz_from_xy, whitepoint,
};
use crate::transforms::builtins::op_helpers::{
    create_half_lut, create_lut, interpolate_1d, try_create_half_lut,
};

/// An entry's op creator.
fn creator(f: fn(&mut OpVec) -> Result<()>) -> OpCreator {
    Arc::new(f)
}

/// ACES AP1 to CIE XYZ with a D65 white, Bradford adaptation.
///
/// Port of `AP1_to_CIE_XYZ_D65::GenerateOps` (ACES.cpp:30-35 @ v2.5.2).
fn ap1_to_cie_xyz_d65(ops: &mut OpVec) -> Result<()> {
    let matrix =
        build_conversion_matrix_to_xyz_d65(&aces_ap1::PRIMARIES, AdaptationMethod::Bradford)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// The ACEScct log curve to linear: an inverse camera log with a linear break.
///
/// Port of `ACEScct_to_LINEAR` (ACES.cpp:40-55 @ v2.5.2).
fn acescct_to_linear() -> Result<LogOpData> {
    const LIN_SIDE_SLOPE: f64 = 1.;
    const LIN_SIDE_OFFSET: f64 = 0.;
    const LOG_SIDE_SLOPE: f64 = 1. / 17.52;
    const LOG_SIDE_OFFSET: f64 = 9.72 / 17.52;
    const LIN_SIDE_BREAK: f64 = 0.0078125;
    const BASE: f64 = 2.;

    let params = vec![
        LOG_SIDE_SLOPE,
        LOG_SIDE_OFFSET,
        LIN_SIDE_SLOPE,
        LIN_SIDE_OFFSET,
        LIN_SIDE_BREAK,
    ];
    LogOpData::from_channel_params(
        BASE,
        params.clone(),
        params.clone(),
        params,
        TransformDirection::Inverse,
    )
}

/// AP1 to AP0, without adaptation: ACEScg to ACES2065-1.
fn ap1_to_ap0(ops: &mut OpVec) -> Result<()> {
    let matrix = build_conversion_matrix(
        &aces_ap1::PRIMARIES,
        &aces_ap0::PRIMARIES,
        AdaptationMethod::None,
    )?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// Port of `ACES_AP0_to_CIE_XYZ_D65_BFD_Functor` (ACES.cpp:579-586 @ v2.5.2). The CIE XYZ space
/// has its conventional normalization (to illuminant E): AP0's neutral `[1, 1, 1]` maps to D65's
/// XYZ.
fn aces_ap0_to_cie_xyz_d65_bfd(ops: &mut OpVec) -> Result<()> {
    let matrix =
        build_conversion_matrix_to_xyz_d65(&aces_ap0::PRIMARIES, AdaptationMethod::Bradford)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// Port of `ACES_AP1_to_LINEAR_REC709_BFD_Functor` (ACES.cpp:603-608 @ v2.5.2).
fn aces_ap1_to_linear_rec709_bfd(ops: &mut OpVec) -> Result<()> {
    let matrix = build_conversion_matrix(
        &aces_ap1::PRIMARIES,
        &rec709::PRIMARIES,
        AdaptationMethod::Bradford,
    )?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// Port of `ACEScct_LOG_to_LIN_Functor` (ACES.cpp:615-619 @ v2.5.2).
fn acescct_log_to_lin(ops: &mut OpVec) -> Result<()> {
    create_log_op(ops, acescct_to_linear()?, TransformDirection::Forward)
}

/// Port of `ACEScct_to_ACES2065_1_Functor` (ACES.cpp:626-634 @ v2.5.2).
fn acescct_to_aces2065_1(ops: &mut OpVec) -> Result<()> {
    create_log_op(ops, acescct_to_linear()?, TransformDirection::Forward)?;
    ap1_to_ap0(ops)
}

/// Port of `ACEScg_to_ACES2065_1_Functor` (ACES.cpp:691-696 @ v2.5.2).
fn acescg_to_aces2065_1(ops: &mut OpVec) -> Result<()> {
    ap1_to_ap0(ops)
}

/// ACESproxy 10i: its code range to log2 values, then linear, then AP1 to AP0.
///
/// Port of `ACESproxy10i_to_ACES2065_1_Functor` (ACES.cpp:703-715 @ v2.5.2).
fn acesproxy10i_to_aces2065_1(ops: &mut OpVec) -> Result<()> {
    create_range_op_from_values(
        ops,
        64. / 1023.,
        940. / 1023.,
        ((64. - 425.) / 50.) - 2.5,
        ((940. - 425.) / 50.) - 2.5,
        TransformDirection::Forward,
    )?;

    create_log_op_from_base(ops, 2., TransformDirection::Inverse);

    ap1_to_ap0(ops)
}

/// ADX's Channel Independent Density to Relative Log Exposure, as `{in, out}` pairs.
///
/// Port of `ADX_to_ACES::nonuniform_LUT` (ACES.cpp:60-73 @ v2.5.2).
const ADX_LUT_SIZE: usize = 11;
#[rustfmt::skip]
const ADX_NONUNIFORM_LUT: [f64; ADX_LUT_SIZE * 2] = [
    -0.190000000000000, -6.000000000000000,
     0.010000000000000, -2.721718645000000,
     0.028000000000000, -2.521718645000000,
     0.054000000000000, -2.321718645000000,
     0.095000000000000, -2.121718645000000,
     0.145000000000000, -1.921718645000000,
     0.220000000000000, -1.721718645000000,
     0.300000000000000, -1.521718645000000,
     0.400000000000000, -1.321718645000000,
     0.500000000000000, -1.121718645000000,
     0.600000000000000, -0.926545676714876,
];

/// ADX Channel Dependent Density to ACES2065-1: a matrix to Channel Independent Density, a
/// half-domain LUT to Relative Log Exposure, an inverse log to Relative Exposure, a matrix to
/// ACES.
///
/// Port of `ADX_to_ACES::GenerateOps` (ACES.cpp:76-137 @ v2.5.2).
fn adx_to_aces(ops: &mut OpVec) -> Result<()> {
    // Note that in CTL, the matrices are stored transposed.
    #[rustfmt::skip]
    const CDD_TO_CID: [f64; 4 * 4] = [
        0.75573,  0.22197,  0.02230,  0.,
        0.05901,  0.96928, -0.02829,  0.,
        0.16134,  0.07406,  0.76460,  0.,
        0.,       0.,       0.,       1.,
    ];

    // Convert Channel Dependent Density values into Channel Independent Density values.
    create_matrix_op_from_m44(ops, &CDD_TO_CID, TransformDirection::Forward);

    let lut = &ADX_NONUNIFORM_LUT;
    // `Interpolate1D` throws for an input no pair brackets, which the half-domain LUT's inputs
    // (NaN codes as 0) never are; its error leaves the ops as they were, as upstream's throw out
    // of `CreateHalfLut` does.
    let generate_lut_values = |in_: f64| -> Result<f32> {
        let mut out;

        if in_ < lut[0] {
            // Lower bound i.e. in < nonuniform_LUT[0, 0].
            // Extrapolate to ease conversion to LUT1D.
            let slope = (lut[3] - lut[1]) / (lut[2] - lut[0]);

            out = lut[1] - slope * (lut[0] - in_);

            if out < -10. {
                out = -10.;
            }
        } else if in_ <= lut[(ADX_LUT_SIZE - 1) * 2] {
            out = interpolate_1d(ADX_LUT_SIZE, lut, in_)?;
        } else {
            // Upper bound i.e. in > nonuniform_LUT[lutSize-1, 0].
            let ref_pt = (7120. - 1520.) / 8000. * (100. / 55.) - 0.18f64.log10();

            out = (100. / 55.) * in_ - ref_pt;

            if out > 4.8162678 {
                out = 4.8162678; // log10(HALF_MAX)
            }
        }

        Ok(out as f32)
    };

    // Convert Channel Independent Density values to Relative Log Exposure values.
    try_create_half_lut(ops, generate_lut_values)?;

    // Convert Relative Log Exposure values to Relative Exposure values.
    create_log_op_from_base(ops, 10., TransformDirection::Inverse);

    #[rustfmt::skip]
    const EXP_TO_ACES: [f64; 4 * 4] = [
        0.72286,  0.12630,  0.15084,  0.,
        0.11923,  0.76418,  0.11659,  0.,
        0.01427,  0.08213,  0.90359,  0.,
        0.,       0.,       0.,       1.,
    ];

    // Convert Relative Exposure values to ACES values.
    create_matrix_op_from_m44(ops, &EXP_TO_ACES, TransformDirection::Forward);
    Ok(())
}

/// ACEScc: a range to [0, 1], a 4096-entry LUT of the ACEScc curve over [-0.36, 1.5], AP1 to
/// AP0, and a clamp at 0.
///
/// Port of `ACEScc_to_ACES2065_1_Functor` (ACES.cpp:641-684 @ v2.5.2).
fn acescc_to_aces2065_1(ops: &mut OpVec) -> Result<()> {
    let generate_lut_values = |input: f64| -> f32 {
        // The functor input will be [0,1].  Remap this to a wider domain to better capture
        // the full extent of ACEScc.
        const IN_MIN: f64 = -0.36;
        const IN_MAX: f64 = 1.50;
        let in_ = input * (IN_MAX - IN_MIN) + IN_MIN;

        let out = if in_ < ((9.72 - 15.0) / 17.52) {
            (std_pow(2., in_ * 17.52 - 9.72) - std_pow(2., -16.)) * 2.0
        } else {
            std_pow(2., in_ * 17.52 - 9.72)
        };
        // The CTL clamps at HALF_MAX, but it's better to avoid a slope discontinuity in a LUT.

        out as f32
    };

    // Allow the LUT to work over a wider input range to better capture the ACEScc extent.
    create_range_op_from_values(ops, -0.36, 1.5, 0.00, 1.0, TransformDirection::Forward)?;

    create_lut(ops, 4096, generate_lut_values)?;

    ap1_to_ap0(ops)?;

    // This helps when the transform is inverted to match the CTL, which clamps incoming
    // ACES2065-1 values.
    create_range_op_from_values(
        ops,
        0.00,
        RangeOpData::empty_value(), // don't clamp high end,
        0.00,
        RangeOpData::empty_value(), // don't clamp high end
        TransformDirection::Forward,
    )
}

/// ADX10: its codes to Channel Dependent Density, then to ACES.
///
/// Port of `ADX10_to_ACES2065_1_Functor` (ACES.cpp:722-735 @ v2.5.2).
fn adx10_to_aces2065_1(ops: &mut OpVec) -> Result<()> {
    const SCALE: f64 = 1023. / 500.;
    const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];

    const OFFSET: f64 = -95. / 500.;
    const OFFSET4: [f64; 4] = [OFFSET, OFFSET, OFFSET, 0.];

    // Convert ADX10 values to Channel Dependent Density values.
    create_scale_offset_op(ops, &SCALE4, &OFFSET4, TransformDirection::Forward);

    // Convert to ACES2065-1.
    adx_to_aces(ops)
}

/// ADX16: its codes to Channel Dependent Density, then to ACES.
///
/// Port of `ADX16_to_ACES2065_1_Functor` (ACES.cpp:742-755 @ v2.5.2).
fn adx16_to_aces2065_1(ops: &mut OpVec) -> Result<()> {
    const SCALE: f64 = 65535. / 8000.;
    const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];

    const OFFSET: f64 = -1520. / 8000.;
    const OFFSET4: [f64; 4] = [OFFSET, OFFSET, OFFSET, 0.];

    // Convert ADX16 values to Channel Dependent Density values.
    create_scale_offset_op(ops, &SCALE4, &OFFSET4, TransformDirection::Forward);

    // Convert to ACES2065-1.
    adx_to_aces(ops)
}

/// The ACES 1.3 reference gamut compression, applied in ACES2065-1: AP0 to AP1, the fixed
/// function with the reference parameters, and back.
///
/// Port of `GAMUT_COMP_13_Functor` (ACES.cpp:780-791 @ v2.5.2).
fn gamut_comp_13(ops: &mut OpVec) -> Result<()> {
    let matrix = build_conversion_matrix(
        &aces_ap0::PRIMARIES,
        &aces_ap1::PRIMARIES,
        AdaptationMethod::None,
    )?;

    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);

    create_fixed_function_op(
        ops,
        FixedFunctionOpStyle::AcesGamutComp13Fwd,
        &vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
    )?;

    create_matrix_op_from_array(ops, &matrix, TransformDirection::Inverse);
    Ok(())
}

/// The LMT that desaturates blue hues to reduce clipping artifacts: a matrix (stored
/// transposed in the CTL).
///
/// Port of `BLUE_LIGHT_FIX_Functor` (ACES.cpp:762-773 @ v2.5.2).
fn blue_light_fix(ops: &mut OpVec) -> Result<()> {
    const BLUE_LIGHT_FIX: [f64; 16] = [
        0.9404372683,
        -0.0183068787,
        0.0778696104,
        0., //
        0.0083786969,
        0.8286599939,
        0.1629613092,
        0., //
        0.0005471261,
        -0.0008833746,
        1.0003362486,
        0., //
        0.,
        0.,
        0.,
        1.,
    ];
    create_matrix_op_from_m44(ops, &BLUE_LIGHT_FIX, TransformDirection::Forward);
    Ok(())
}

/// The ACES 2.0 output transforms' ops.
///
/// Port of `ACES2_OUTPUT` (ACES.cpp:500-566 @ v2.5.2).
mod aces2_output {
    use super::*;

    /// C++ `std::max(a, b)`: `(a < b) ? b : a`.
    fn std_max(a: f64, b: f64) -> f64 {
        if a < b { b } else { a }
    }

    /// A clamp to AP1 (from AP0 and back), the ACES 2.0 output transform fixed function, a
    /// clamp to the normalized peak, the white scaling, the linear scale, and the limiting
    /// primaries to CIE XYZ (D65 white).
    ///
    /// The clamp's upper bound is in `float` throughout: `std::log` of a `float` and the
    /// unqualified `log(10000.f / 100.f)` are both the `float` overload (the Linux wheel calls
    /// `logf` and divides by `4.6051702f` with `divss` at 0x52562a).
    ///
    /// Port of `ACES2_OUTPUT::Generate_output_transform` (ACES.cpp:502-564 @ v2.5.2).
    pub(super) fn generate_output_transform(
        ops: &mut OpVec,
        peak_luminance: f32,
        limiting_pri: &Primaries,
        encoding_pri: &Primaries,
        linear_scale: f32,
        scale_white: bool,
    ) -> Result<()> {
        // Clamp to AP1
        let matrix_to_ap1 = build_conversion_matrix(
            &aces_ap0::PRIMARIES,
            &aces_ap1::PRIMARIES,
            AdaptationMethod::None,
        )?;
        create_matrix_op_from_array(ops, &matrix_to_ap1, TransformDirection::Forward);

        let upper_bound: f32 =
            8. * (128. + 768. * ((peak_luminance / 100.).ln() / (10000f32 / 100.).ln()));
        create_range_op_from_values(
            ops,
            0.,
            f64::from(upper_bound),
            0.,
            f64::from(upper_bound),
            TransformDirection::Forward,
        )?;

        create_matrix_op_from_array(ops, &matrix_to_ap1, TransformDirection::Inverse);

        // Display rendering
        create_fixed_function_op(
            ops,
            FixedFunctionOpStyle::AcesOutputTransform20Fwd,
            &vec![
                f64::from(peak_luminance),
                limiting_pri.red.xy[0],
                limiting_pri.red.xy[1],
                limiting_pri.grn.xy[0],
                limiting_pri.grn.xy[1],
                limiting_pri.blu.xy[0],
                limiting_pri.blu.xy[1],
                limiting_pri.wht.xy[0],
                limiting_pri.wht.xy[1],
            ],
        )?;

        // Post transform clamp
        let norm_peak_luminance = f64::from(peak_luminance / 100.);
        create_range_op_from_values(
            ops,
            0.,
            norm_peak_luminance,
            0.,
            norm_peak_luminance,
            TransformDirection::Forward,
        )?;

        // White point simulation
        if scale_white {
            let matrix_lim_to_out =
                build_conversion_matrix(limiting_pri, encoding_pri, AdaptationMethod::None)?;

            let mut white = Offsets::new(1., 1., 1., 0.);
            white = matrix_lim_to_out.inner_offsets(&white);

            let scale = 1. / std_max(std_max(white[0], white[1]), white[2]);
            let scale4 = [scale, scale, scale, 1.];
            create_scale_op(ops, &scale4, TransformDirection::Forward);
        }

        // Linear scale factor
        if linear_scale != 1. {
            let scale = f64::from(linear_scale);
            let scale4 = [scale, scale, scale, 1.];
            create_scale_op(ops, &scale4, TransformDirection::Forward);
        }

        let matrix_to_xyz =
            build_conversion_matrix_to_xyz_d65(limiting_pri, AdaptationMethod::None)?;
        create_matrix_op_from_array(ops, &matrix_to_xyz, TransformDirection::Forward);
        Ok(())
    }
}

/// An ACES 2.0 output transform of the table: its style, its description and the parameters
/// of its ops.
///
/// Port of `ACES2OutputTransform` (ACES.cpp:1122-1131 @ v2.5.2).
struct Aces2OutputTransform {
    /// `name`.
    name: &'static [u8],
    /// `desc`.
    desc: &'static [u8],
    /// `peak_luminance`.
    peak_luminance: f32,
    /// `limiting_primaries`.
    limiting_primaries: Primaries,
    /// `encoding_primaries`.
    encoding_primaries: Primaries,
    /// `linear_scale`.
    linear_scale: f32,
    /// `scale_white`.
    scale_white: bool,
}

/// `aces2_output_transforms` (ACES.cpp:1133-1419 @ v2.5.2): D65, then D60 simulated.
const ACES2_OUTPUT_TRANSFORMS: [Aces2OutputTransform; 31] = [
    // ACES.cpp:1138
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709",
        peak_luminance: 100.,
        limiting_primaries: rec709::PRIMARIES,
        encoding_primaries: rec709::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1147
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR P3-D65",
        peak_luminance: 100.,
        limiting_primaries: p3_d65::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1156
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 108 nit HDR P3-D65",
        peak_luminance: 225., // = 108 * (100/48);
        limiting_primaries: p3_d65::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 0.48,
        scale_white: false,
    },
    // ACES.cpp:1165
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 300 nit HDR P3-D65",
        peak_luminance: 625., // = 300 * (100/48);
        limiting_primaries: p3_d65::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 0.48,
        scale_white: false,
    },
    // ACES.cpp:1174
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR P3-D65",
        peak_luminance: 500.,
        limiting_primaries: p3_d65::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1183
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR P3-D65",
        peak_luminance: 1000.,
        limiting_primaries: p3_d65::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1192
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR P3-D65",
        peak_luminance: 2000.,
        limiting_primaries: p3_d65::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1201
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR P3-D65",
        peak_luminance: 4000.,
        limiting_primaries: p3_d65::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1210
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR Rec2020",
        peak_luminance: 500.,
        limiting_primaries: rec2020::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1219
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR Rec2020",
        peak_luminance: 1000.,
        limiting_primaries: rec2020::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1228
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR Rec2020",
        peak_luminance: 2000.,
        limiting_primaries: rec2020::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1237
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR Rec2020",
        peak_luminance: 4000.,
        limiting_primaries: rec2020::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1249
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC709-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in Rec709",
        peak_luminance: 100.,
        limiting_primaries: rec709_d60::PRIMARIES,
        encoding_primaries: rec709::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1258
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in P3-D65",
        peak_luminance: 100.,
        limiting_primaries: rec709_d60::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1267
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR Rec709 simulating D60 white in Rec2020",
        peak_luminance: 100.,
        limiting_primaries: rec709_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1276
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR P3-D60 simulating D60 white in P3-D65",
        peak_luminance: 100.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1285
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-XYZ-E_2.0",
        desc: b"Component of ACES 2 Output Transforms for 100 nit SDR P3-D60 simulating D60 white in XYZ-E",
        peak_luminance: 100.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: cie_xyz_illum_e::PRIMARIES,
        linear_scale: 1.,
        scale_white: false,
    },
    // ACES.cpp:1294
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 108 nit HDR P3-D60 simulating D60 white in P3-D65",
        peak_luminance: 225., // = 108 * (100/48);
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 0.48,
        scale_white: true,
    },
    // ACES.cpp:1303
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D60-in-XYZ-E_2.0",
        desc: b"Component of ACES 2 Output Transforms for 300 nit HDR P3-D60 simulating D60 white in XYZ-E",
        peak_luminance: 625., // = 300 * (100/48);
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: cie_xyz_illum_e::PRIMARIES,
        linear_scale: 0.48,
        scale_white: true,
    },
    // ACES.cpp:1312
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR P3-D60 simulating D60 white in P3-D65",
        peak_luminance: 500.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1321
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR P3-D60 simulating D60 white in P3-D65",
        peak_luminance: 1000.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1330
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR P3-D60 simulating D60 white in P3-D65",
        peak_luminance: 2000.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1339
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR P3-D60 simulating D60 white in P3-D65",
        peak_luminance: 4000.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: p3_d65::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1348
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR P3-D60 simulating D60 white in Rec2020",
        peak_luminance: 500.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1357
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR P3-D60 simulating D60 white in Rec2020",
        peak_luminance: 1000.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1366
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR P3-D60 simulating D60 white in Rec2020",
        peak_luminance: 2000.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1375
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR P3-D60 simulating D60 white in Rec2020",
        peak_luminance: 4000.,
        limiting_primaries: p3_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1384
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 500 nit HDR Rec2020 simulating D60 white in Rec2020",
        peak_luminance: 500.,
        limiting_primaries: rec2020_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1393
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 1000 nit HDR Rec2020 simulating D60 white in Rec2020",
        peak_luminance: 1000.,
        limiting_primaries: rec2020_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1402
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 2000 nit HDR Rec2020 simulating D60 white in Rec2020",
        peak_luminance: 2000.,
        limiting_primaries: rec2020_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
    // ACES.cpp:1411
    Aces2OutputTransform {
        name: b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020-D60-in-REC2020-D65_2.0",
        desc: b"Component of ACES 2 Output Transforms for 4000 nit HDR Rec2020 simulating D60 white in Rec2020",
        peak_luminance: 4000.,
        limiting_primaries: rec2020_d60::PRIMARIES,
        encoding_primaries: rec2020::PRIMARIES,
        linear_scale: 1.,
        scale_white: true,
    },
];

/// The ACES 1.x output transforms' building blocks.
///
/// Port of `ACES_OUTPUT` (ACES.cpp:141-498 @ v2.5.2).
mod aces_output {
    use super::*;

    /// The RRT's glow, red modifier, clamps around AP0 to AP1, and saturation matrix.
    ///
    /// Port of `ACES_OUTPUT::Generate_RRT_preamble_ops` (ACES.cpp:144-172 @ v2.5.2).
    pub(super) fn generate_rrt_preamble_ops(ops: &mut OpVec) -> Result<()> {
        create_fixed_function_op(ops, FixedFunctionOpStyle::AcesGlow10Fwd, &vec![])?;

        create_fixed_function_op(ops, FixedFunctionOpStyle::AcesRedMod10Fwd, &vec![])?;

        create_range_op_from_values(
            ops,
            0.,
            RangeOpData::empty_value(), // don't clamp high end
            0.,
            RangeOpData::empty_value(), // don't clamp high end
            TransformDirection::Forward,
        )?;

        let matrix = build_conversion_matrix(
            &aces_ap0::PRIMARIES,
            &aces_ap1::PRIMARIES,
            AdaptationMethod::None,
        )?;
        create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);

        create_range_op_from_values(
            ops,
            0.,
            RangeOpData::empty_value(), // don't clamp high end
            0.,
            RangeOpData::empty_value(), // don't clamp high end
            TransformDirection::Forward,
        )?;

        #[rustfmt::skip]
        const RRT_SAT_MAT: [f64; 4 * 4] = [
            0.970889148671, 0.026963270632, 0.002147580696, 0.,
            0.010889148671, 0.986963270632, 0.002147580696, 0.,
            0.010889148671, 0.026963270632, 0.962147580696, 0.,
            0., 0., 0., 1.,
        ];
        create_matrix_op_from_m44(ops, &RRT_SAT_MAT, TransformDirection::Forward);
        Ok(())
    }

    /// A log-style RGB curve op whose master curve is the B-spline through `points` with
    /// `slopes`, the other three curves the identity: one of the CTL's quadratic B-spline
    /// shapers. Upstream repeats this block for each shaper.
    ///
    /// Port of the shaper blocks of `ACES_OUTPUT::Generate_tonecurve_ops` and
    /// `ACES_OUTPUT::Generate_hdr_tonecurve_ops` (ACES.cpp:179-200, 203-232, 266-285 @ v2.5.2).
    fn create_shaper_op(ops: &mut OpVec, points: &[(f32, f32)], slopes: &[f32]) -> Result<()> {
        let points: Vec<GradingControlPoint> = points
            .iter()
            .map(|&(x, y)| GradingControlPoint::new(x, y))
            .collect();
        let mut curve = GradingBSplineCurve::with_points(&points);
        for (i, &slope) in slopes.iter().enumerate() {
            curve.set_slope(i, slope)?;
        }
        let m = curve;
        let identity = GradingBSplineCurve::with_points(&[
            GradingControlPoint::new(0., 0.),
            GradingControlPoint::new(1., 1.),
        ]);
        let z = identity;
        let gc = GradingRgbCurveOpData::with_curves(GradingStyle::Log, &z, &z, &z, &m)?;

        create_grading_rgb_curve_op(ops, gc, TransformDirection::Forward);
        Ok(())
    }

    /// The SDR tone curve: to log 10, the RRT and SDR ODT shapers, back to linear, and the
    /// cinema white and black.
    ///
    /// Port of `ACES_OUTPUT::Generate_tonecurve_ops` (ACES.cpp:174-249 @ v2.5.2).
    pub(super) fn generate_tonecurve_ops(ops: &mut OpVec) -> Result<()> {
        // Convert to Log space.
        create_log_op_from_base(ops, 10., TransformDirection::Forward);

        // Apply RRT shaper using the same quadratic B-spline as the CTL.
        create_shaper_op(
            ops,
            &[
                (-5.26017743, -4.),
                (-3.75502745, -3.57868829),
                (-2.24987747, -1.82131329),
                (-0.74472749, 0.68124124),
                (1.06145248, 2.87457742),
                (2.86763245, 3.83406206),
                (4.67381243, 4.),
            ],
            &[0., 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.],
        )?;

        // Apply SDR ODT shaper using the same quadratic B-spline as the CTL.
        create_shaper_op(
            ops,
            &[
                (-2.54062362, -1.69897000),
                (-2.08035721, -1.58843500),
                (-1.62009080, -1.35350000),
                (-1.15982439, -1.04695000),
                (-0.69955799, -0.65640000),
                (-0.23929158, -0.22141000),
                (0.22097483, 0.22814402),
                (0.68124124, 0.68124124),
                (1.01284632, 0.99142189),
                (1.34445140, 1.25800000),
                (1.67605648, 1.44995000),
                (2.00766156, 1.55910000),
                (2.33926665, 1.62260000),
                (2.67087173, 1.66065457),
                (3.00247681, 1.68124124),
            ],
            &[
                0., 0.4803088, 0.5405565, 0.79149813, 0.9055625, 0.98460368, 0.96884766, 1.,
                0.87078346, 0.73702127, 0.42068113, 0.23763206, 0.14535362, 0.08416378, 0.04,
            ],
        )?;

        // Undo the logarithm.
        create_log_op_from_base(ops, 10., TransformDirection::Inverse);

        // Apply Cinema White/Black correction.
        {
            const CINEMA_WHITE: f64 = 48.;
            // Note: ACESlib.ODT_Common.ctl claims that using pow10(log10(0.02) for black
            // improves performance at 0, but that does not seem to be the case here, 0 input
            // currently gives about 4e-11 XYZ output either way.
            const CINEMA_BLACK: f64 = 0.02;
            const SCALE: f64 = 1. / (CINEMA_WHITE - CINEMA_BLACK);
            const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];
            const OFFSET: f64 = -CINEMA_BLACK * SCALE;
            const OFFSET4: [f64; 4] = [OFFSET, OFFSET, OFFSET, 0.];

            create_scale_offset_op(ops, &SCALE4, &OFFSET4, TransformDirection::Forward);
        }
        Ok(())
    }

    /// The dark to dim surround and the 48 to 100 nit desaturation.
    ///
    /// Port of `ACES_OUTPUT::Generate_video_adjustment_ops` (ACES.cpp:251-265 @ v2.5.2).
    pub(super) fn generate_video_adjustment_ops(ops: &mut OpVec) -> Result<()> {
        // Surround correction for cinema to video.
        create_fixed_function_op(ops, FixedFunctionOpStyle::AcesDarkToDim10Fwd, &vec![])?;

        // Desat to compensate 48 nit to 100 nit brightness.
        #[rustfmt::skip]
        const DESAT_100_NITS: [f64; 4 * 4] = [
            0.949056010175, 0.047185723607, 0.003758266219, 0.,
            0.019056010175, 0.977185723607, 0.003758266219, 0.,
            0.019056010175, 0.047185723607, 0.933758266219, 0.,
            0., 0., 0., 1.,
        ];
        create_matrix_op_from_m44(ops, &DESAT_100_NITS, TransformDirection::Forward);
        Ok(())
    }

    /// The HDR tone curve for a peak of `y_max` nits (1000, 2000, 4000 or 108; no shaper for
    /// another): to log 10, the RRT shaper, back to linear, and the white and black.
    ///
    /// Port of `ACES_OUTPUT::Generate_hdr_tonecurve_ops` (ACES.cpp:267-384 @ v2.5.2).
    pub(super) fn generate_hdr_tonecurve_ops(ops: &mut OpVec, y_max: f64) -> Result<()> {
        // Convert to Log space.
        create_log_op_from_base(ops, 10., TransformDirection::Forward);

        // Apply RRT shaper using the same quadratic B-spline as the CTL.
        if y_max == 1000. {
            create_shaper_op(
                ops,
                &[
                    (-5.60050155, -4.00000000),
                    (-4.09535157, -3.57868829),
                    (-2.59020159, -1.82131329),
                    (-1.08505161, 0.68124124),
                    (0.22347059, 2.22673503),
                    (1.53199279, 2.87906206),
                    (2.84051500, 3.00000000),
                ],
                &[0., 0.55982688, 1.77532247, 1.55, 0.81219728, 0.1848466, 0.],
            )?;
        } else if y_max == 2000. {
            create_shaper_op(
                ops,
                &[
                    (-5.59738488, -4.00000000),
                    (-4.09223490, -3.57868829),
                    (-2.58708492, -1.82131329),
                    (-1.08193494, 0.68124124),
                    (0.37639718, 2.42130131),
                    (1.83472930, 3.16609199),
                    (3.29306142, 3.30103000),
                ],
                &[0., 0.55982688, 1.77532247, 1.55, 0.83637009, 0.18505799, 0.],
            )?;
        } else if y_max == 4000. {
            create_shaper_op(
                ops,
                &[
                    (-5.59503319, -4.00000000),
                    (-4.08988322, -3.57868829),
                    (-2.58473324, -1.82131329),
                    (-1.07958326, 0.68124124),
                    (0.52855878, 2.61625839),
                    (2.13670081, 3.45351273),
                    (3.74484285, 3.60205999),
                ],
                &[0., 0.55982688, 1.77532247, 1.55, 0.85652519, 0.18474395, 0.],
            )?;
        } else if y_max == 108. {
            create_shaper_op(
                ops,
                &[
                    (-5.37852506, -4.00000000),
                    (-3.87337508, -3.57868829),
                    (-2.36822510, -1.82131329),
                    (-0.86307513, 0.68124124),
                    (-0.03557710, 1.60464482),
                    (0.79192092, 1.96008059),
                    (1.61941895, 2.03342376),
                ],
                &[0., 0.55982688, 1.77532247, 1.55, 0.68179646, 0.17726487, 0.],
            )?;
        }

        // Undo the logarithm.
        create_log_op_from_base(ops, 10., TransformDirection::Inverse);

        // Apply Cinema White/Black correction.
        {
            let y_min = 0.0001;
            let scale = 1. / (y_max - y_min);
            let scale4 = [scale, scale, scale, 1.];
            let offset = -y_min * scale;
            let offset4 = [offset, offset, offset, 0.];

            create_scale_offset_op(ops, &scale4, &offset4, TransformDirection::Forward);
        }
        Ok(())
    }

    /// AP1 to the limiting primaries (Bradford), a clamp to [0, 1], and to CIE XYZ.
    ///
    /// Port of `ACES_OUTPUT::Generate_sdr_primary_clamp_ops` (ACES.cpp:386-400 @ v2.5.2).
    pub(super) fn generate_sdr_primary_clamp_ops(
        ops: &mut OpVec,
        limit_primaries: &Primaries,
    ) -> Result<()> {
        let matrix1 = build_conversion_matrix(
            &aces_ap1::PRIMARIES,
            limit_primaries,
            AdaptationMethod::Bradford,
        )?;
        create_matrix_op_from_array(ops, &matrix1, TransformDirection::Forward);

        create_range_op_from_values(ops, 0., 1., 0., 1., TransformDirection::Forward)?;

        let matrix2 = rgb2xyz_from_xy(limit_primaries)?;
        create_matrix_op_from_array(ops, &matrix2, TransformDirection::Forward);
        Ok(())
    }

    /// AP1 to the limiting primaries (no adaptation), a clamp to [0, 1], to CIE XYZ, and D60
    /// to D65 (Bradford).
    ///
    /// Port of `ACES_OUTPUT::Generate_hdr_primary_clamp_ops` (ACES.cpp:402-420 @ v2.5.2).
    pub(super) fn generate_hdr_primary_clamp_ops(
        ops: &mut OpVec,
        limit_primaries: &Primaries,
    ) -> Result<()> {
        let matrix1 = build_conversion_matrix(
            &aces_ap1::PRIMARIES,
            limit_primaries,
            AdaptationMethod::None,
        )?;
        create_matrix_op_from_array(ops, &matrix1, TransformDirection::Forward);

        create_range_op_from_values(ops, 0., 1., 0., 1., TransformDirection::Forward)?;

        let matrix2 = rgb2xyz_from_xy(limit_primaries)?;
        create_matrix_op_from_array(ops, &matrix2, TransformDirection::Forward);

        let matrix3 = build_vonkries_adapt(
            &whitepoint::d60_xyz(),
            &whitepoint::d65_xyz(),
            AdaptationMethod::Bradford,
        )?;
        create_matrix_op_from_array(ops, &matrix3, TransformDirection::Forward);
        Ok(())
    }

    /// A scale by `nit_level / 100`: the PQ curve takes nits / 100.
    ///
    /// Port of `ACES_OUTPUT::Generate_nit_normalization_ops` (ACES.cpp:422-429 @ v2.5.2).
    pub(super) fn generate_nit_normalization_ops(ops: &mut OpVec, nit_level: f64) {
        // The PQ curve expects nits / 100 as input.  Unnormalize 1.0 to the nit level for the
        // transform and then renormalize to put 100 nits at 1.0.
        let scale = nit_level * 0.01;
        let scale4 = [scale, scale, scale, 1.];
        create_scale_op(ops, &scale4, TransformDirection::Forward);
    }

    /// The roll-off of the white to `new_wht`, as the half-domain LUT both roll-offs build.
    ///
    /// Port of the `GenerateLutValues` lambdas of `ACES_OUTPUT::Generate_roll_white_d60_ops`
    /// and `ACES_OUTPUT::Generate_roll_white_d65_ops` (ACES.cpp:433-458, 467-492 @ v2.5.2),
    /// which differ only in `new_wht`.
    fn roll_white(new_wht: f64, in_: f64) -> f32 {
        let width = 0.5;
        let x0 = -1.0;
        let x1 = x0 + width;
        let y0 = -new_wht;
        let y1 = x1;
        let m1 = x1 - x0;
        let a = y0 - y1 + m1;
        let b = 2. * (y1 - y0) - m1;
        let c = y0;
        let t = (-in_ - x0) / (x1 - x0);
        let out = if t < 0.0 {
            -(t * b + c)
        } else if t > 1.0 {
            in_
        } else {
            -((t * a + b) * t + c)
        };
        out as f32
    }

    /// Port of `ACES_OUTPUT::Generate_roll_white_d60_ops` (ACES.cpp:431-462 @ v2.5.2).
    pub(super) fn generate_roll_white_d60_ops(ops: &mut OpVec) -> Result<()> {
        create_half_lut(ops, |in_| roll_white(0.918, in_))
    }

    /// Port of `ACES_OUTPUT::Generate_roll_white_d65_ops` (ACES.cpp:464-496 @ v2.5.2).
    pub(super) fn generate_roll_white_d65_ops(ops: &mut OpVec) -> Result<()> {
        create_half_lut(ops, |in_| roll_white(0.908, in_))
    }
}

/// Port of `ACES2065_1_to_CIE_XYZ_cinema_1_0_Functor` (ACES.cpp:803-810 @ v2.5.2).
fn cinema_1_0(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    ap1_to_cie_xyz_d65(ops)
}

/// Port of `ACES2065_1_to_CIE_XYZ_video_1_0_Functor` (ACES.cpp:818-827 @ v2.5.2).
fn video_1_0(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    aces_output::generate_video_adjustment_ops(ops)?;

    ap1_to_cie_xyz_d65(ops)
}

/// Port of `ACES2065_1_to_CIE_XYZ_cinema_rec709lim_1_1_Functor` (ACES.cpp:835-842 @ v2.5.2).
fn cinema_rec709lim_1_1(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    aces_output::generate_sdr_primary_clamp_ops(ops, &rec709::PRIMARIES)
}

/// Port of `ACES2065_1_to_CIE_XYZ_video_rec709lim_1_1_Functor` (ACES.cpp:850-859 @ v2.5.2).
fn video_rec709lim_1_1(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    aces_output::generate_video_adjustment_ops(ops)?;

    aces_output::generate_sdr_primary_clamp_ops(ops, &rec709::PRIMARIES)
}

/// Port of `ACES2065_1_to_CIE_XYZ_video_p3lim_1_1_Functor` (ACES.cpp:867-876 @ v2.5.2).
fn video_p3lim_1_1(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    aces_output::generate_video_adjustment_ops(ops)?;

    aces_output::generate_sdr_primary_clamp_ops(ops, &p3_d65::PRIMARIES)
}

/// Port of `ACES2065_1_to_CIE_XYZ_cinema_d60sim_1_1_Functor` (ACES.cpp:884-902 @ v2.5.2).
fn cinema_d60sim_1_1(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    create_range_op_from_values(
        ops,
        RangeOpData::empty_value(),
        1.0, // don't clamp low end
        RangeOpData::empty_value(),
        1.0, // don't clamp low end
        TransformDirection::Forward,
    )?;

    const SCALE: f64 = 0.964;
    const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];
    create_scale_op(ops, &SCALE4, TransformDirection::Forward);

    let matrix = rgb2xyz_from_xy(&aces_ap1::PRIMARIES)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// Port of `ACES2065_1_to_CIE_XYZ_video_d60sim_1_0_Functor` (ACES.cpp:910-930 @ v2.5.2).
fn video_d60sim_1_0(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    create_range_op_from_values(
        ops,
        RangeOpData::empty_value(),
        1.0, // don't clamp low end
        RangeOpData::empty_value(),
        1.0, // don't clamp low end
        TransformDirection::Forward,
    )?;

    const SCALE: f64 = 0.955;
    const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];
    create_scale_op(ops, &SCALE4, TransformDirection::Forward);

    aces_output::generate_video_adjustment_ops(ops)?;

    let matrix = rgb2xyz_from_xy(&aces_ap1::PRIMARIES)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);
    Ok(())
}

/// Port of `ACES2065_1_to_CIE_XYZ_cinema_d60sim_dci_1_0_Functor` (ACES.cpp:938-962 @ v2.5.2).
fn cinema_d60sim_dci_1_0(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    aces_output::generate_roll_white_d60_ops(ops)?;

    create_range_op_from_values(
        ops,
        RangeOpData::empty_value(),
        0.918, // don't clamp low end
        RangeOpData::empty_value(),
        0.918, // don't clamp low end
        TransformDirection::Forward,
    )?;

    const SCALE: f64 = 0.96;
    const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];
    create_scale_op(ops, &SCALE4, TransformDirection::Forward);

    let matrix = rgb2xyz_from_xy(&aces_ap1::PRIMARIES)?;
    create_matrix_op_from_array(ops, &matrix, TransformDirection::Forward);

    let matrix2 = build_vonkries_adapt(
        &whitepoint::dci_xyz(),
        &whitepoint::d65_xyz(),
        AdaptationMethod::Bradford,
    )?;
    create_matrix_op_from_array(ops, &matrix2, TransformDirection::Forward);
    Ok(())
}

/// Port of `ACES2065_1_to_CIE_XYZ_cinema_d65sim_dci_1_1_Functor` (ACES.cpp:970-992 @ v2.5.2).
fn cinema_d65sim_dci_1_1(ops: &mut OpVec) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_tonecurve_ops(ops)?;

    aces_output::generate_roll_white_d65_ops(ops)?;

    create_range_op_from_values(
        ops,
        RangeOpData::empty_value(),
        0.908, // don't clamp low end
        RangeOpData::empty_value(),
        0.908, // don't clamp low end
        TransformDirection::Forward,
    )?;

    const SCALE: f64 = 0.9575;
    const SCALE4: [f64; 4] = [SCALE, SCALE, SCALE, 1.];
    create_scale_op(ops, &SCALE4, TransformDirection::Forward);

    ap1_to_cie_xyz_d65(ops)?;

    let matrix2 = build_vonkries_adapt(
        &whitepoint::dci_xyz(),
        &whitepoint::d65_xyz(),
        AdaptationMethod::Bradford,
    )?;
    create_matrix_op_from_array(ops, &matrix2, TransformDirection::Forward);
    Ok(())
}

/// The HDR video and cinema outputs: the RRT preamble, the tone curve of `nits`, the clamp to
/// `limit_primaries`, and the nit normalization.
///
/// Port of the HDR functors (`ACES2065_1_to_CIE_XYZ_hdr_video_1000nits_rec2020lim_1_1_Functor`
/// and the six after it, ACES.cpp:1000-1111 @ v2.5.2), which differ only in these two.
fn hdr_output(ops: &mut OpVec, nits: f64, limit_primaries: &Primaries) -> Result<()> {
    aces_output::generate_rrt_preamble_ops(ops)?;

    aces_output::generate_hdr_tonecurve_ops(ops, nits)?;

    aces_output::generate_hdr_primary_clamp_ops(ops, limit_primaries)?;

    aces_output::generate_nit_normalization_ops(ops, nits);
    Ok(())
}

/// An HDR output's op creator.
fn hdr_creator(nits: f64, limit_primaries: Primaries) -> OpCreator {
    Arc::new(move |ops: &mut OpVec| hdr_output(ops, nits, &limit_primaries))
}

/// Registers the 59 ACES built-in transforms: 28 one by one, then the ACES 2.0
/// output transforms of the table.
///
/// Port of `ACES::RegisterAll` (ACES.cpp:576-1437 @ v2.5.2).
pub(crate) fn register_all(registry: &mut BuiltinTransformRegistry) {
    // ACES.cpp:588
    registry.add_builtin(
        b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
        Some(b"Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation"),
        creator(aces_ap0_to_cie_xyz_d65_bfd),
    );
    // ACES.cpp:598
    registry.add_builtin(
        b"UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD",
        Some(b"Convert ACES AP1 primaries to CIE XYZ with a D65 white point with Bradford adaptation"),
        creator(ap1_to_cie_xyz_d65),
    );
    // ACES.cpp:610
    registry.add_builtin(
        b"UTILITY - ACES-AP1_to_LINEAR-REC709_BFD",
        Some(b"Convert ACES AP1 primaries to linear Rec.709 primaries with Bradford adaptation"),
        creator(aces_ap1_to_linear_rec709_bfd),
    );
    // ACES.cpp:621
    registry.add_builtin(
        b"CURVE - ACEScct-LOG_to_LINEAR",
        Some(b"Apply the log-to-lin curve used in ACEScct"),
        creator(acescct_log_to_lin),
    );
    // ACES.cpp:636
    registry.add_builtin(
        b"ACEScct_to_ACES2065-1",
        Some(b"Convert ACEScct to ACES2065-1"),
        creator(acescct_to_aces2065_1),
    );
    // ACES.cpp:686
    registry.add_builtin(
        b"ACEScc_to_ACES2065-1",
        Some(b"Convert ACEScc to ACES2065-1"),
        creator(acescc_to_aces2065_1),
    );
    // ACES.cpp:698
    registry.add_builtin(
        b"ACEScg_to_ACES2065-1",
        Some(b"Convert ACEScg to ACES2065-1"),
        creator(acescg_to_aces2065_1),
    );
    // ACES.cpp:717
    registry.add_builtin(
        b"ACESproxy10i_to_ACES2065-1",
        Some(b"Convert ACESproxy 10i to ACES2065-1"),
        creator(acesproxy10i_to_aces2065_1),
    );
    // ACES.cpp:737
    registry.add_builtin(
        b"ADX10_to_ACES2065-1",
        Some(b"Convert ADX10 to ACES2065-1"),
        creator(adx10_to_aces2065_1),
    );
    // ACES.cpp:757
    registry.add_builtin(
        b"ADX16_to_ACES2065-1",
        Some(b"Convert ADX16 to ACES2065-1"),
        creator(adx16_to_aces2065_1),
    );
    // ACES.cpp:775
    registry.add_builtin(
        b"ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX",
        Some(b"LMT for desaturating blue hues to reduce clipping artifacts"),
        creator(blue_light_fix),
    );
    // ACES.cpp:793
    registry.add_builtin(
        b"ACES-LMT - ACES 1.3 Reference Gamut Compression",
        Some(b"LMT (applied in ACES2065-1) to compress scene-referred values from common cameras into the AP1 gamut"),
        creator(gamut_comp_13),
    );
    // ACES.cpp:812
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA_1.0",
        Some(b"Component of ACES Output Transforms for SDR cinema"),
        creator(cinema_1_0),
    );
    // ACES.cpp:829
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0",
        Some(b"Component of ACES Output Transforms for SDR D65 video"),
        creator(video_1_0),
    );
    // ACES.cpp:844
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-REC709lim_1.1",
        Some(b"Component of ACES Output Transforms for SDR cinema"),
        creator(cinema_rec709lim_1_1),
    );
    // ACES.cpp:861
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-REC709lim_1.1",
        Some(b"Component of ACES Output Transforms for SDR D65 video"),
        creator(video_rec709lim_1_1),
    );
    // ACES.cpp:878
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for SDR D65 video"),
        creator(video_p3lim_1_1),
    );
    // ACES.cpp:904
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-D65_1.1",
        Some(b"Component of ACES Output Transforms for SDR D65 cinema simulating D60 white"),
        creator(cinema_d60sim_1_1),
    );
    // ACES.cpp:932
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-D60sim-D65_1.0",
        Some(b"Component of ACES Output Transforms for SDR D65 video simulating D60 white"),
        creator(video_d60sim_1_0),
    );
    // ACES.cpp:964
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-DCI_1.0",
        Some(b"Component of ACES Output Transforms for SDR DCI cinema simulating D60 white"),
        creator(cinema_d60sim_dci_1_0),
    );
    // ACES.cpp:994
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D65sim-DCI_1.1",
        Some(b"Component of ACES Output Transforms for SDR DCI cinema simulating D65 white"),
        creator(cinema_d65sim_dci_1_1),
    );
    // ACES.cpp:1011
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-REC2020lim_1.1",
        Some(b"Component of ACES Output Transforms for 1000 nit HDR D65 video"),
        hdr_creator(1000., rec2020::PRIMARIES),
    );
    // ACES.cpp:1028
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 1000 nit HDR D65 video"),
        hdr_creator(1000., p3_d65::PRIMARIES),
    );
    // ACES.cpp:1045
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-REC2020lim_1.1",
        Some(b"Component of ACES Output Transforms for 2000 nit HDR D65 video"),
        hdr_creator(2000., rec2020::PRIMARIES),
    );
    // ACES.cpp:1062
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 2000 nit HDR D65 video"),
        hdr_creator(2000., p3_d65::PRIMARIES),
    );
    // ACES.cpp:1079
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-REC2020lim_1.1",
        Some(b"Component of ACES Output Transforms for 4000 nit HDR D65 video"),
        hdr_creator(4000., rec2020::PRIMARIES),
    );
    // ACES.cpp:1096
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 4000 nit HDR D65 video"),
        hdr_creator(4000., p3_d65::PRIMARIES),
    );
    // ACES.cpp:1113
    registry.add_builtin(
        b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-CINEMA-108nit-7.2nit-P3lim_1.1",
        Some(b"Component of ACES Output Transforms for 108 nit HDR D65 cinema"),
        hdr_creator(108., p3_d65::PRIMARIES),
    );

    for tr in &ACES2_OUTPUT_TRANSFORMS {
        let functor: OpCreator = Arc::new(move |ops: &mut OpVec| {
            aces2_output::generate_output_transform(
                ops,
                tr.peak_luminance,
                &tr.limiting_primaries,
                &tr.encoding_primaries,
                tr.linear_scale,
                tr.scale_white,
            )
        });

        registry.add_builtin(tr.name, Some(tr.desc), functor);
    }
}
