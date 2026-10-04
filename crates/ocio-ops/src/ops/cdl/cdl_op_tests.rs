// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/cdl/CDLOp_tests.cpp` @ v2.5.2: the tests of the op. Its test of
//! `CreateCDLTransform` (`create_transform`) needs the `CDLTransform`: it is in
//! crates/ocio/src/transforms/cdl_transform_tests.rs.
//!
//! The wheel is built with `OCIO_USE_SSE2`, so the apply tests keep the `#if OCIO_USE_SSE2`
//! error thresholds; they render with `getCPUOp(true)`.

use ocio_testkit::upstream::equal_with_safe_rel_error;

use super::*;
use crate::format_metadata::METADATA_ID;

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

/// `ApplyCDL` (tests/cpu/ops/cdl/CDLOp_tests.cpp:16-55 @ v2.5.2): builds the op from the
/// parameters (the constructor validates them), validates it, renders `input` in place with
/// `getCPUOp(true)` and compares with `reference`, a relative error with a minimum expected
/// value of 1 (so an absolute error below 1); NaN must stay NaN.
#[track_caller]
#[allow(clippy::too_many_arguments)]
fn apply_cdl(
    input: &mut [f32],
    reference: &[f32],
    slope: &[f64; 3],
    offset: &[f64; 3],
    power: &[f64; 3],
    saturation: f64,
    style: CdlOpStyle,
    error_threshold: f32,
) {
    let data = CdlOpData::new(
        style,
        ChannelParams::new(slope[0], slope[1], slope[2]),
        ChannelParams::new(offset[0], offset[1], offset[2]),
        ChannelParams::new(power[0], power[1], power[2]),
        saturation,
    )
    .unwrap();
    let mut cdl_op = cdl_op(data);

    cdl_op.validate().unwrap();

    let cpu = cdl_op.get_cpu_op(true).unwrap().expect("a renderer");
    cpu.apply(input);

    for (idx, (&value, &expected)) in input.iter().zip(reference).enumerate() {
        // Using rel error with a large minExpected value of 1 will transition from absolute
        // error for expected values < 1 and relative error for values > 1.
        assert!(
            equal_with_safe_rel_error(value, expected, error_threshold, 1.0f32),
            "Index: {idx} - Values: {value} and: {expected} - Threshold: {error_threshold}"
        );
    }
}

/// `CDL_DATA_1` (tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66 @ v2.5.2).
mod cdl_data_1 {
    pub(super) const SLOPE: [f64; 3] = [1.35, 1.1, 0.071];
    pub(super) const OFFSET: [f64; 3] = [0.05, -0.23, 0.11];
    pub(super) const POWER: [f64; 3] = [0.93, 0.81, 1.27];
    pub(super) const SATURATION: f64 = 1.23;
}

/// `CDL_DATA_2` (tests/cpu/ops/cdl/CDLOp_tests.cpp:428-434 @ v2.5.2).
mod cdl_data_2 {
    pub(super) const SLOPE: [f64; 3] = [1.15, 1.10, 0.9];
    pub(super) const OFFSET: [f64; 3] = [0.05, 0.02, 0.07];
    pub(super) const POWER: [f64; 3] = [1.2, 0.95, 1.13];
    pub(super) const SATURATION: f64 = 0.87;
}

/// `CDL_DATA_3` (tests/cpu/ops/cdl/CDLOp_tests.cpp:469-475 @ v2.5.2).
mod cdl_data_3 {
    pub(super) const SLOPE: [f64; 3] = [3.405, 1.0, 1.0];
    pub(super) const OFFSET: [f64; 3] = [-0.178, -0.178, -0.178];
    pub(super) const POWER: [f64; 3] = [1.095, 1.095, 1.095];
    pub(super) const SATURATION: f64 = 0.99;
}

/// `CreateCDLOp(ops, style, CDL_DATA_1 ..., saturation, direction)`.
fn create_data_1(ops: &mut OpVec, style: CdlOpStyle, saturation: f64, dir: TransformDirection) {
    use cdl_data_1::*;
    create_cdl_op_from_values(ops, style, &SLOPE, &OFFSET, &POWER, saturation, dir).unwrap();
}

/// Port of `OCIO_ADD_TEST(CDLOp, computed_identifier)` @ v2.5.2.
#[test]
fn computed_identifier() {
    use cdl_data_1::*;
    let fwd = TransformDirection::Forward;
    let mut ops = OpVec::new();

    create_data_1(&mut ops, CdlOpStyle::V1_2Fwd, SATURATION, fwd);
    assert_eq!(ops.len(), 1);

    create_data_1(&mut ops, CdlOpStyle::V1_2Fwd, SATURATION, fwd);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 2);

    let id0 = ops[0].get_cache_id().unwrap();
    let id1 = ops[1].get_cache_id().unwrap();
    assert_eq!(id0, id1);

    let slope = ChannelParams::new(SLOPE[0], SLOPE[1], SLOPE[2]);
    let offset = ChannelParams::new(OFFSET[0], OFFSET[1], OFFSET[2]);
    let power = ChannelParams::new(POWER[0], POWER[1], POWER[2]);

    let mut cdl_data =
        CdlOpData::new(CdlOpStyle::V1_2Fwd, slope, offset, power, SATURATION).unwrap();
    cdl_data
        .get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"1"))
        .unwrap();
    assert_eq!(cdl_data.get_id(), b"1");

    create_cdl_op(&mut ops, cdl_data, fwd);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 3);

    let id2 = ops[2].get_cache_id().unwrap();

    assert!(id0 != id2);
    assert!(id1 != id2);

    // `CDL_DATA_1::saturation + 0.002f`: the float promoted to double.
    let saturation_2 = SATURATION + f64::from(0.002f32);

    create_data_1(&mut ops, CdlOpStyle::V1_2Fwd, saturation_2, fwd);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 4);

    let id3 = ops[3].get_cache_id().unwrap();

    assert!(id0 != id3);
    assert!(id1 != id3);
    assert!(id2 != id3);

    create_data_1(&mut ops, CdlOpStyle::V1_2Fwd, saturation_2, fwd);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 5);

    let id4 = ops[4].get_cache_id().unwrap();

    assert!(id0 != id4);
    assert!(id1 != id4);
    assert!(id2 != id4);
    assert!(id3 == id4);

    create_data_1(&mut ops, CdlOpStyle::NoClampFwd, saturation_2, fwd);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 6);

    let id5 = ops[5].get_cache_id().unwrap();

    assert!(id3 != id5);
    assert!(id4 != id5);
}

/// Port of `OCIO_ADD_TEST(CDLOp, is_inverse)` @ v2.5.2.
#[test]
fn is_inverse() {
    use TransformDirection::{Forward, Inverse};
    let sat = cdl_data_1::SATURATION;
    let mut ops = OpVec::new();

    create_data_1(&mut ops, CdlOpStyle::V1_2Fwd, sat, Forward);
    assert_eq!(ops.len(), 1);

    create_data_1(&mut ops, CdlOpStyle::V1_2Fwd, sat, Inverse);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 2);

    let op0 = ops[0].clone();
    let op1 = ops[1].clone();

    assert!(ops[0].is_inverse(&op1));
    assert!(ops[1].is_inverse(&op0));

    create_data_1(&mut ops, CdlOpStyle::V1_2Fwd, 1.30, Inverse);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 3);
    let op2 = ops[2].clone();

    assert!(!ops[0].is_inverse(&op2));
    assert!(!ops[1].is_inverse(&op2));
    assert!(!ops[2].is_inverse(&op0));
    assert!(!ops[2].is_inverse(&op1));

    create_data_1(&mut ops, CdlOpStyle::V1_2Rev, 1.30, Inverse);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 4);
    let op3 = ops[3].clone();

    assert!(ops[2].is_inverse(&op3));

    create_data_1(&mut ops, CdlOpStyle::V1_2Rev, 1.30, Forward);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 5);
    let op4 = ops[4].clone();

    assert!(!ops[2].is_inverse(&op4));
    assert!(ops[3].is_inverse(&op4));

    create_data_1(&mut ops, CdlOpStyle::NoClampFwd, 1.30, Forward);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 6);
    let op5 = ops[5].clone();

    assert!(!ops[2].is_inverse(&op5));
    assert!(!ops[3].is_inverse(&op5));
    assert!(!ops[4].is_inverse(&op5));

    create_data_1(&mut ops, CdlOpStyle::NoClampFwd, 1.30, Inverse);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 7);
    let op6 = ops[6].clone();

    assert!(!ops[2].is_inverse(&op6));
    assert!(!ops[3].is_inverse(&op6));
    assert!(!ops[4].is_inverse(&op6));
    assert!(ops[5].is_inverse(&op6));
}

// The expected values below were calculated via an independent ASC CDL implementation.
// Note that the error thresholds are higher for the SSE version because of the use of a much
// faster, but somewhat less accurate, implementation of the power function.
// TODO: The NaN and Inf handling is probably not ideal, as shown by the tests below, and could
// be improved.

/// The input of apply_clamp_fwd (CDLOp_tests.cpp:276-286 @ v2.5.2).
const INPUT_FWD: [f32; 40] = [
    QNAN, QNAN, QNAN, 0.0, //
    0.0, 0.0, 0.0, QNAN, //
    INF, INF, INF, INF, //
    -INF, -INF, -INF, -INF, //
    0.3278, 0.01, 1.0, 0.0, //
    0.25, 0.5, 0.75, 1.0, //
    1.25, 1.5, 1.75, 0.75, //
    -0.2, 0.5, 1.4, 0.0, //
    -0.25, -0.5, -0.75, 0.25, //
    0.0, 0.8, 0.99, 0.5,
];

/// Port of `OCIO_ADD_TEST(CDLOp, apply_clamp_fwd)` @ v2.5.2.
#[test]
fn apply_clamp_fwd() {
    use cdl_data_1::*;
    let mut input_32f = INPUT_FWD;

    let expected_32f: [f32; 40] = [
        0.0, 0.0, 0.0, 0.0, //
        0.071827, 0.0, 0.070533, QNAN, //
        1.0, 1.0, 1.0, INF, //
        0.0, 0.0, 0.0, -INF, //
        0.609399, 0.000000, 0.113130, 0.0, //
        0.422056, 0.401466, 0.035820, 1.0, //
        1.000000, 1.000000, 0.000000, 0.75, //
        0.000000, 0.421096, 0.101225, 0.0, //
        0.000000, 0.000000, 0.031735, 0.25, //
        0.000000, 0.746748, 0.018691, 0.5,
    ];

    apply_cdl(
        &mut input_32f,
        &expected_32f,
        &SLOPE,
        &OFFSET,
        &POWER,
        SATURATION,
        CdlOpStyle::V1_2Fwd,
        4e-6,
    );
}

/// The input of the reverse apply tests (CDLOp_tests.cpp:315-325 @ v2.5.2).
const INPUT_REV: [f32; 40] = [
    QNAN, QNAN, QNAN, 0.0, //
    0.0, 0.0, 0.0, QNAN, //
    INF, INF, INF, INF, //
    -INF, -INF, -INF, -INF, //
    0.609399, 0.100000, 0.113130, 0.0, //
    0.001000, 0.746748, 0.018691, 0.5, //
    0.422056, 0.401466, 0.035820, 1.0, //
    -0.25, -0.5, -0.75, 0.25, //
    1.25, 1.5, 1.75, 0.75, //
    -0.2, 0.5, 1.4, 0.0,
];

/// Port of `OCIO_ADD_TEST(CDLOp, apply_clamp_rev)` @ v2.5.2.
#[test]
fn apply_clamp_rev() {
    use cdl_data_1::*;
    let mut input_32f = INPUT_REV;

    let expected_32f: [f32; 40] = [
        0.0, 0.209091, 0.0, 0.0, //
        0.0, 0.209091, 0.0, QNAN, //
        0.703713, 1.0, 1.0, INF, //
        0.0, 0.209091, 0.0, -INF, //
        0.340710, 0.275726, 1.000000, 0.0, //
        0.025902, 0.801895, 1.000000, 0.5, //
        0.250000, 0.500000, 0.750006, 1.0, //
        0.000000, 0.209091, 0.000000, 0.25, //
        0.703704, 1.000000, 1.000000, 0.75, //
        0.012206, 0.582944, 1.000000, 0.0,
    ];

    apply_cdl(
        &mut input_32f,
        &expected_32f,
        &SLOPE,
        &OFFSET,
        &POWER,
        SATURATION,
        CdlOpStyle::V1_2Rev,
        9e-6,
    );
}

/// Port of `OCIO_ADD_TEST(CDLOp, apply_noclamp_fwd)` @ v2.5.2.
#[test]
fn apply_noclamp_fwd() {
    use cdl_data_1::*;
    // CDLOp_tests.cpp:354-364 @ v2.5.2.
    let mut input_32f: [f32; 40] = [
        QNAN, QNAN, QNAN, 0.0, //
        0.0, 0.0, 0.0, QNAN, //
        INF, INF, INF, INF, //
        -INF, -INF, -INF, -INF, //
        0.3278, 0.01, 1.0, 0.0, //
        0.0, 0.8, 0.99, 0.5, //
        0.25, 0.5, 0.75, 1.0, //
        -0.25, -0.5, -0.75, 0.25, //
        1.25, 1.5, 1.75, 0.75, //
        -0.2, 0.5, 1.4, 0.0,
    ];

    let expected_32f: [f32; 40] = [
        0.0, 0.0, 0.0, 0.0, //
        0.109661, -0.249088, 0.108368, QNAN, //
        QNAN, QNAN, QNAN, INF, //
        QNAN, QNAN, QNAN, -INF, //
        0.645424, -0.260548, 0.149154, 0.0, //
        -0.045094, 0.746748, 0.018691, 0.5, //
        0.422056, 0.401466, 0.035820, 1.0, //
        -0.211694, -0.817469, 0.174100, 0.25, //
        1.753162, 1.331130, -0.108181, 0.75, //
        -0.327485, 0.431854, 0.111983, 0.0,
    ];

    apply_cdl(
        &mut input_32f,
        &expected_32f,
        &SLOPE,
        &OFFSET,
        &POWER,
        SATURATION,
        CdlOpStyle::NoClampFwd,
        2e-5,
    );
}

/// Port of `OCIO_ADD_TEST(CDLOp, apply_noclamp_rev)` @ v2.5.2.
#[test]
fn apply_noclamp_rev() {
    use cdl_data_1::*;
    let mut input_32f = INPUT_REV;

    let expected_32f: [f32; 40] = [
        -0.037037, 0.209091, -1.549296, 0.0, //
        -0.037037, 0.209091, -1.549296, QNAN, //
        -0.037037, 0.209091, -1.549296, INF, //
        -0.037037, 0.209091, -1.549296, -INF, //
        0.340710, 0.275726, 1.294827, 0.0, //
        0.025902, 0.801895, 1.022221, 0.5, //
        0.250000, 0.500000, 0.750006, 1.0, //
        -0.251989, -0.239488, -11.361812, 0.25, //
        0.937160, 1.700692, 19.807237, 0.75, //
        -0.099839, 0.580528, 14.880301, 0.0,
    ];

    apply_cdl(
        &mut input_32f,
        &expected_32f,
        &SLOPE,
        &OFFSET,
        &POWER,
        SATURATION,
        CdlOpStyle::NoClampRev,
        3e-5,
    );
}

/// Port of `OCIO_ADD_TEST(CDLOp, apply_clamp_fwd_2)` @ v2.5.2.
#[test]
fn apply_clamp_fwd_2() {
    use cdl_data_2::*;
    let mut input_32f: [f32; 28] = [
        QNAN, QNAN, QNAN, 0.0, //
        0.0, 0.0, 0.0, QNAN, //
        INF, INF, INF, INF, //
        -INF, -INF, -INF, -INF, //
        0.65, 0.55, 0.20, 0.0, //
        0.41, 0.81, 0.39, 0.5, //
        0.25, 0.50, 0.75, 1.0,
    ];

    let expected_32f: [f32; 28] = [
        0.0, 0.0, 0.0, 0.0, //
        0.027379, 0.024645, 0.046585, QNAN, //
        1.0, 1.0, 1.0, INF, //
        0.0, 0.0, 0.0, -INF, //
        0.745644, 0.639197, 0.264149, 0.0, //
        0.499594, 0.897554, 0.428591, 0.5, //
        0.305035, 0.578779, 0.692558, 1.0,
    ];

    apply_cdl(
        &mut input_32f,
        &expected_32f,
        &SLOPE,
        &OFFSET,
        &POWER,
        SATURATION,
        CdlOpStyle::V1_2Fwd,
        7e-6,
    );
}

/// The input of the `_3` apply tests (CDLOp_tests.cpp:481-505 @ v2.5.2).
const INPUT_3: [f32; 80] = [
    QNAN, QNAN, QNAN, 0.0, //
    0.0, 0.0, 0.0, QNAN, //
    INF, INF, INF, INF, //
    -INF, -INF, -INF, -INF, //
    //
    0.02, 0.0, 0.0, 0.0, //
    0.17, 0.0, 0.0, 0.0, //
    0.65, 0.0, 0.0, 0.0, //
    0.97, 0.0, 0.0, 0.0, //
    //
    0.02, 0.13, 0.0, 0.0, //
    0.17, 0.13, 0.0, 0.0, //
    0.65, 0.13, 0.0, 0.0, //
    0.97, 0.13, 0.0, 0.0, //
    //
    0.02, 0.23, 0.0, 0.0, //
    0.17, 0.23, 0.0, 0.0, //
    0.65, 0.23, 0.0, 0.0, //
    0.97, 0.23, 0.0, 0.0, //
    //
    0.02, 0.13, 0.23, 0.0, //
    0.17, 0.13, 0.23, 0.0, //
    0.65, 0.13, 0.23, 0.0, //
    0.97, 0.13, 0.23, 0.0,
];

/// Port of `OCIO_ADD_TEST(CDLOp, apply_clamp_fwd_3)` @ v2.5.2.
#[test]
fn apply_clamp_fwd_3() {
    use cdl_data_3::*;
    let mut input_32f = INPUT_3;

    let expected_32f: [f32; 80] = [
        0.000000, 0.000000, 0.000000, 0.0, //
        0.000000, 0.000000, 0.000000, QNAN, //
        1.0, 1.0, 1.0, INF, //
        0.0, 0.0, 0.0, -INF, //
        //
        0.000000, 0.000000, 0.000000, 0.0, //
        0.364613, 0.000781, 0.000781, 0.0, //
        0.992126, 0.002126, 0.002126, 0.0, //
        0.992126, 0.002126, 0.002126, 0.0, //
        //
        0.000000, 0.000000, 0.000000, 0.0, //
        0.364613, 0.000781, 0.000781, 0.0, //
        0.992126, 0.002126, 0.002126, 0.0, //
        0.992126, 0.002126, 0.002126, 0.0, //
        //
        0.000281, 0.039155, 0.0002808, 0.0, //
        0.364894, 0.039936, 0.0010621, 0.0, //
        0.992407, 0.041281, 0.0024068, 0.0, //
        0.992407, 0.041281, 0.0024068, 0.0, //
        //
        0.000028, 0.000028, 0.0389023, 0.0, //
        0.364641, 0.000810, 0.0396836, 0.0, //
        0.992154, 0.002154, 0.0410283, 0.0, //
        0.992154, 0.002154, 0.0410283, 0.0,
    ];

    apply_cdl(
        &mut input_32f,
        &expected_32f,
        &SLOPE,
        &OFFSET,
        &POWER,
        SATURATION,
        CdlOpStyle::V1_2Fwd,
        2e-5,
    );
}

/// Port of `OCIO_ADD_TEST(CDLOp, apply_noclamp_fwd_3)` @ v2.5.2.
#[test]
fn apply_noclamp_fwd_3() {
    use cdl_data_3::*;
    let mut input_32f = INPUT_3;

    let expected_32f: [f32; 80] = [
        0.0, 0.0, 0.0, 0.0, //
        -0.178000, -0.178000, -0.178000, QNAN, //
        QNAN, QNAN, QNAN, INF, //
        QNAN, QNAN, QNAN, -INF, //
        //
        -0.110436, -0.177855, -0.177855, 0.0, //
        0.363211, -0.176840, -0.176840, 0.0, //
        2.158845, -0.172992, -0.172992, 0.0, //
        3.453254, -0.170219, -0.170219, 0.0, //
        //
        -0.109506, -0.048225, -0.176925, 0.0, //
        0.364141, -0.047210, -0.175910, 0.0, //
        2.159774, -0.043363, -0.172063, 0.0, //
        3.454184, -0.040589, -0.169289, 0.0, //
        //
        -0.108882, 0.038793, -0.176301, 0.0, //
        0.364765, 0.039808, -0.175286, 0.0, //
        2.160399, 0.043655, -0.171438, 0.0, //
        3.454808, 0.046429, -0.168665, 0.0, //
        //
        -0.109350, -0.048069, 0.038325, 0.0, //
        0.364298, -0.047054, 0.039340, 0.0, //
        2.159931, -0.043206, 0.043188, 0.0, //
        3.454341, -0.040432, 0.045962, 0.0,
    ];

    apply_cdl(
        &mut input_32f,
        &expected_32f,
        &SLOPE,
        &OFFSET,
        &POWER,
        SATURATION,
        CdlOpStyle::NoClampFwd,
        5e-6,
    );
}

// ---------------------------------------------------------------------------------------------
// The port's own checks.

/// A CDL op never combines, and `combineWith` refuses with upstream's message (CDLOp.cpp:
/// 102-117 @ v2.5.2); `CreateCDLOp` inverts the data for an inverse direction and keeps its
/// metadata (CDLOp.cpp:170-181), and the constructor of the parameters validates them.
#[test]
fn never_combines_and_create_inverts() {
    let mut ops = OpVec::new();
    let mut data = CdlOpData::new(
        CdlOpStyle::V1_2Fwd,
        ChannelParams::new(1.35, 1.1, 0.071),
        ChannelParams::new(0.05, -0.23, 0.11),
        ChannelParams::new(0.93, 0.81, 1.27),
        1.23,
    )
    .unwrap();
    data.get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"cdl"))
        .unwrap();
    create_cdl_op(&mut ops, data.clone(), TransformDirection::Forward);
    create_cdl_op(&mut ops, data.clone(), TransformDirection::Inverse);
    let (op0, op1) = (ops[0].clone(), ops[1].clone());
    assert!(!op0.can_combine_with(&op1).unwrap());
    assert!(op0.is_same_type(&op1));
    assert_eq!(
        op0.combine_with(&mut ops, &op1).unwrap_err().message(),
        "CDLOp: canCombineWith must be checked before calling combineWith."
    );
    let OpData::Cdl(inverted) = &**op1.data() else {
        panic!("a CDL op")
    };
    assert!(inverted.equals(&data.inverse()));
    assert_eq!(inverted.get_id(), b"cdl");
    let refused = create_cdl_op_from_values(
        &mut ops,
        CdlOpStyle::NoClampFwd,
        &[1.0; 3],
        &[0.0; 3],
        &[0.0, 1.0, 1.0],
        1.0,
        TransformDirection::Forward,
    );
    assert!(refused.is_err());
    assert_eq!(ops.len(), 2);
}
