// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ACES built-in transforms: a port of `ACES::RegisterAll`
//! (src/OpenColorIO/transforms/builtins/ACES.cpp:576-1437 @ v2.5.2), each entry's style and
//! description in upstream's order, the ACES 2.0 output transforms' table included. The
//! entries build their ops, but the ACES 1.x and 2.0 output transforms, which return an error
//! until `p3-after-p2` ports them.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::fixedfunction::fixed_function_op::create_fixed_function_op;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpStyle;
use ocio_ops::ops::log::log_op::{create_log_op, create_log_op_from_base};
use ocio_ops::ops::log::log_op_data::LogOpData;
use ocio_ops::ops::matrix::matrix_op::{
    create_matrix_op_from_array, create_matrix_op_from_m44, create_scale_offset_op,
};
use ocio_ops::ops::range::range_op::create_range_op_from_values;
use ocio_ops::ops::range::range_op_data::RangeOpData;

use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, OpCreator, not_ported_yet,
};
use crate::transforms::builtins::color_matrix_helpers::{
    AdaptationMethod, aces_ap0, aces_ap1, build_conversion_matrix,
    build_conversion_matrix_to_xyz_d65, rec709,
};
use crate::transforms::builtins::op_helpers::{create_half_lut, create_lut, interpolate_1d};

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
    // (NaN codes as 0) never are; the first such error is returned after the LUT is built.
    let interpolation_error = std::cell::RefCell::new(None);
    let generate_lut_values = |in_: f64| -> f32 {
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
            out = interpolate_1d(ADX_LUT_SIZE, lut, in_).unwrap_or_else(|e| {
                interpolation_error.borrow_mut().get_or_insert(e);
                0.
            });
        } else {
            // Upper bound i.e. in > nonuniform_LUT[lutSize-1, 0].
            let ref_pt = (7120. - 1520.) / 8000. * (100. / 55.) - 0.18f64.log10();

            out = (100. / 55.) * in_ - ref_pt;

            if out > 4.8162678 {
                out = 4.8162678; // log10(HALF_MAX)
            }
        }

        out as f32
    };

    // Convert Channel Independent Density values to Relative Log Exposure values.
    create_half_lut(ops, generate_lut_values)?;
    if let Some(e) = interpolation_error.into_inner() {
        return Err(e);
    }

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
            (2f64.powf(in_ * 17.52 - 9.72) - 2f64.powf(-16.)) * 2.0
        } else {
            2f64.powf(in_ * 17.52 - 9.72)
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
