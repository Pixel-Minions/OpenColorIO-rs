// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/range/RangeOpData_tests.cpp` @ v2.5.2.

use ocio_testkit::upstream::{check_close, check_throw_what, floats_differ};

use super::*;

/// Port of `OCIO_ADD_TEST(RangeOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    {
        let mut r = RangeOpData::new();

        assert!(is_nan(r.get_min_in_value() as f32));
        assert!(is_nan(r.get_max_in_value() as f32));
        assert!(is_nan(r.get_min_out_value() as f32));
        assert!(is_nan(r.get_max_out_value() as f32));

        // Empty range is not valid.
        assert!(!r.is_no_op());
        assert!(r.is_identity());
        check_throw_what(r.validate(), "At least minimum or maximum limits");

        let min_val = 1.0;
        let max_val = 10.0;
        r.set_min_in_value(min_val);
        r.set_max_in_value(max_val);
        r.set_min_out_value(2.0 * min_val);
        r.set_max_out_value(2.0 * max_val);

        assert_eq!(r.get_min_in_value(), min_val);
        assert_eq!(r.get_max_in_value(), max_val);
        assert_eq!(r.get_min_out_value(), 2.0 * min_val);
        assert_eq!(r.get_max_out_value(), 2.0 * max_val);

        assert_eq!(r.get_type(), OpDataType::Range);
    }

    {
        let g_error = 1e-7f32;

        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();

        assert_eq!(range.get_min_in_value(), 0.);
        assert_eq!(range.get_max_in_value(), 1.);
        assert_eq!(range.get_min_out_value(), 0.5);
        assert_eq!(range.get_max_out_value(), 1.5);

        range.set_min_in_value(-0.05432);
        range.validate().unwrap();
        assert_eq!(range.get_min_in_value(), -0.05432);

        range.set_max_in_value(1.05432);
        range.validate().unwrap();
        assert_eq!(range.get_max_in_value(), 1.05432);

        range.set_min_out_value(0.05432);
        range.validate().unwrap();
        assert_eq!(range.get_min_out_value(), 0.05432);

        range.set_max_out_value(2.05432);
        range.validate().unwrap();
        assert_eq!(range.get_max_out_value(), 2.05432);

        // `OCIO_CHECK_CLOSE(double, double, float)`: the tolerance is promoted to double.
        check_close(range.get_scale(), 1.804012123, f64::from(g_error));
        check_close(range.get_offset(), 0.1523139385, f64::from(g_error));
    }

    {
        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();

        assert_eq!(range.get_direction(), TransformDirection::Forward);
        assert_eq!(range.get_file_input_bit_depth(), BitDepth::Unknown);
        assert_eq!(range.get_file_output_bit_depth(), BitDepth::Unknown);

        // Set file bit-depth and verify.
        range.set_file_input_bit_depth(BitDepth::Uint8);
        range.set_file_output_bit_depth(BitDepth::F32);

        assert_eq!(range.get_file_input_bit_depth(), BitDepth::Uint8);
        assert_eq!(range.get_file_output_bit_depth(), BitDepth::F32);

        // Changing direction does not change values.
        range.set_direction(TransformDirection::Inverse);

        assert_eq!(range.get_direction(), TransformDirection::Inverse);
        assert_eq!(range.get_file_input_bit_depth(), BitDepth::Uint8);
        assert_eq!(range.get_file_output_bit_depth(), BitDepth::F32);

        assert_eq!(range.get_min_in_value(), 0.);
        assert_eq!(range.get_max_in_value(), 1.);
        assert_eq!(range.get_min_out_value(), 0.5);
        assert_eq!(range.get_max_out_value(), 1.5);

        // The getAsForward swaps the bit-depths and the values.
        let r = range.get_as_forward().unwrap();
        assert_eq!(r.get_direction(), TransformDirection::Forward);
        assert_eq!(r.get_file_input_bit_depth(), BitDepth::F32);
        assert_eq!(r.get_file_output_bit_depth(), BitDepth::Uint8);

        assert_eq!(r.get_min_in_value(), 0.5);
        assert_eq!(r.get_max_in_value(), 1.5);
        assert_eq!(r.get_min_out_value(), 0.);
        assert_eq!(r.get_max_out_value(), 1.);
    }
}

/// Port of `OCIO_ADD_TEST(RangeOpData, range_identity)` @ v2.5.2.
#[test]
fn range_identity() {
    let empty = RangeOpData::empty_value();

    let r1 = RangeOpData::with_values(0., 1., 0., 1.).unwrap();
    assert!(r1.clamps_to_lut_domain());
    assert!(r1.is_identity());
    assert!(!r1.is_clamp_negs());

    let r2 = RangeOpData::with_values(0.1, 1.2, -0.5, 2.).unwrap();
    assert!(!r2.clamps_to_lut_domain());
    assert!(!r2.is_identity());
    assert!(!r2.is_clamp_negs());

    let r3 = RangeOpData::with_values(-0.1, 1.0, -0.5, 2.).unwrap();
    assert!(!r3.clamps_to_lut_domain());
    assert!(!r3.is_identity());
    assert!(!r3.is_clamp_negs());

    let r4 = RangeOpData::with_values(0., 1., 0.01, 1.).unwrap();
    assert!(r4.clamps_to_lut_domain());
    assert!(!r4.is_identity());
    assert!(!r4.is_clamp_negs());

    let r5 = RangeOpData::with_values(0.1, 1., -0.01, 1.).unwrap();
    assert!(r5.clamps_to_lut_domain());
    assert!(!r5.is_identity());
    assert!(!r5.is_clamp_negs());

    let r6 = RangeOpData::with_values(-0.1, 1.1, -0.1, 1.1).unwrap();
    assert!(!r6.clamps_to_lut_domain());
    assert!(r6.is_identity());
    assert!(!r6.is_clamp_negs());

    let r7 = RangeOpData::with_values(0., empty, 0., empty).unwrap();
    assert!(!r7.clamps_to_lut_domain());
    assert!(r7.is_identity());
    assert!(r7.is_clamp_negs());

    let r8 = RangeOpData::with_values(empty, 1., empty, 1.).unwrap();
    assert!(!r8.clamps_to_lut_domain());
    assert!(r8.is_identity());
    assert!(!r8.is_clamp_negs());
}

/// Port of `OCIO_ADD_TEST(RangeOpData, identity)` @ v2.5.2.
#[test]
fn identity() {
    let empty = RangeOpData::empty_value();

    let r4 = RangeOpData::with_values(0., empty, 0., empty).unwrap();
    assert!(r4.is_identity());
    assert!(!r4.is_no_op());
    assert!(!r4.has_channel_crosstalk());
    assert!(!r4.scales());
    assert!(!r4.min_is_empty());
    assert!(r4.max_is_empty());

    let r5 = RangeOpData::with_values(0., 1., 0., 1.).unwrap();
    assert!(!r5.scales());
    assert!(r5.is_identity());
    assert!(!r5.has_channel_crosstalk());
    assert!(!r5.is_no_op());
    assert!(!r5.min_is_empty());
    assert!(!r5.max_is_empty());

    let r6 = RangeOpData::with_values(0., 1., -1., 1.).unwrap();
    assert!(!r6.is_identity());
    assert!(!r6.is_no_op());
    assert!(!r6.has_channel_crosstalk());
    assert!(!r6.min_is_empty());
    assert!(!r6.max_is_empty());
    assert_eq!(r6.get_min_out_value(), -1.);
    assert_eq!(r6.get_max_out_value(), 1.);
    assert!(r6.scales());
}

/// Port of `OCIO_ADD_TEST(RangeOpData, equality)` @ v2.5.2.
#[test]
fn equality() {
    let r1 = RangeOpData::with_values(0., 1., -1., 1.).unwrap();

    let r2 = RangeOpData::with_values(0.123, 1., -1., 1.).unwrap();

    assert!(!(r1 == r2));

    let r3 = RangeOpData::with_values(0., 0.99, -1., 1.).unwrap();

    assert!(!(r1 == r3));

    let r4 = RangeOpData::with_values(0., 1., -12., 1.).unwrap();

    assert!(!(r1 == r4));

    let r5 = RangeOpData::with_values(0., 1., -1., 1.).unwrap();

    assert!(r5 == r1);
}

/// Port of `OCIO_ADD_TEST(RangeOpData, validation)` @ v2.5.2.
#[test]
fn validation() {
    {
        let mut r = RangeOpData::new();

        r.set_min_in_value(16.);
        r.set_max_in_value(235.);
        // Leave min output empty.
        r.set_max_out_value(2.);

        check_throw_what(r.validate(), "must be both set or both missing");
    }

    {
        let mut r = RangeOpData::new();

        r.set_min_in_value(0.0);
        r.set_min_out_value(0.00001);

        check_throw_what(r.validate(), "In and out minimum limits must be equal");
    }

    {
        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();
        range.validate().unwrap();

        range.unset_min_in_value();
        check_throw_what(
            range.validate(),
            "In and out minimum limits must be both set or both missing",
        );
    }

    {
        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();
        range.validate().unwrap();

        range.unset_min_in_value();
        range.unset_min_out_value();
        check_throw_what(range.validate(), "In and out maximum limits must be equal");
        range.set_max_in_value(range.get_max_out_value());
        range.validate().unwrap();
    }

    {
        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();
        range.validate().unwrap();

        range.unset_max_in_value();
        check_throw_what(
            range.validate(),
            "In and out maximum limits must be both set or both missing",
        );
    }

    {
        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();
        range.validate().unwrap();

        range.unset_max_in_value();
        range.unset_max_out_value();
        check_throw_what(range.validate(), "In and out minimum limits must be equal");
        range.set_min_in_value(range.get_min_out_value());
        range.validate().unwrap();
    }

    {
        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();
        range.validate().unwrap();

        range.set_max_in_value(-125.);
        check_throw_what(
            range.validate(),
            "Range maximum input value is less than minimum input value",
        );
    }

    {
        let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();
        range.validate().unwrap();

        range.set_max_out_value(-125.);
        check_throw_what(
            range.validate(),
            "Range maximum output value is less than minimum output value",
        );
    }
}

/// Port of the `checkInverse` helper of `RangeOpData_tests.cpp` (lines 301-350 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
#[track_caller]
fn check_inverse(
    fwd_min_in: f64,
    fwd_max_in: f64,
    fwd_min_out: f64,
    fwd_max_out: f64,
    rev_min_in: f64,
    rev_max_in: f64,
    rev_min_out: f64,
    rev_max_out: f64,
) {
    let ref_op = RangeOpData::with_direction(
        fwd_min_in,
        fwd_max_in,
        fwd_min_out,
        fwd_max_out,
        TransformDirection::Inverse,
    )
    .unwrap();

    let inv_op = ref_op.get_as_forward().unwrap();

    // The min/max values should be swapped.
    if rev_min_in.is_nan() {
        assert!(inv_op.get_min_in_value().is_nan());
    } else {
        assert_eq!(inv_op.get_min_in_value(), rev_min_in);
    }
    if rev_max_in.is_nan() {
        assert!(inv_op.get_max_in_value().is_nan());
    } else {
        assert_eq!(inv_op.get_max_in_value(), rev_max_in);
    }
    if rev_min_out.is_nan() {
        assert!(inv_op.get_min_out_value().is_nan());
    } else {
        assert_eq!(inv_op.get_min_out_value(), rev_min_out);
    }
    if rev_max_out.is_nan() {
        assert!(inv_op.get_max_out_value().is_nan());
    } else {
        assert_eq!(inv_op.get_max_out_value(), rev_max_out);
    }

    // Check that the computation would be correct.
    // (Doing this in lieu of renderer testing.)

    let fwd_scale = ref_op.get_scale() as f32;
    let fwd_offset = ref_op.get_offset() as f32;
    let rev_scale = inv_op.get_scale() as f32;
    let rev_offset = inv_op.get_offset() as f32;

    // Want in == (in * fwdScale + fwdOffset) * revScale + revOffset
    // in == in * fwdScale * revScale + fwdOffset * revScale + revOffset
    // in == in * 1. + 0.
    assert!(!floats_differ(1.0f32, fwd_scale * rev_scale, 10, false));

    // The above is correct but we lose too much precision in the subtraction so rearrange the
    // compare as follows to allow a tighter tolerance.
    assert!(!floats_differ(
        fwd_offset * rev_scale,
        -rev_offset,
        500,
        false
    ));
}

/// Port of `OCIO_ADD_TEST(RangeOpData, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    let empty = RangeOpData::empty_value();

    // Results in scale != 1 and offset != 0.
    check_inverse(0.064, 0.940, 0.032, 0.235, 0.032, 0.235, 0.064, 0.940);

    // Note: All the following result in clipping only.

    check_inverse(empty, 0.235, empty, 0.235, empty, 0.235, empty, 0.235);

    check_inverse(0.64, empty, 0.64, empty, 0.64, empty, 0.64, empty);
}

/// Port of `OCIO_ADD_TEST(RangeOpData, compose)` @ v2.5.2.
#[test]
fn compose() {
    let empty = RangeOpData::empty_value();

    let r1 = RangeOpData::with_values(0., 1., 0., 1.).unwrap();
    let r2 = RangeOpData::with_values(0.1, 0.9, 0.1, 0.9).unwrap();

    let res = r1.compose(&r2).unwrap();
    assert_eq!(res.get_min_in_value(), 0.1);
    assert_eq!(res.get_max_in_value(), 0.9);
    assert_eq!(res.get_min_out_value(), 0.1);
    assert_eq!(res.get_max_out_value(), 0.9);

    let r3 = RangeOpData::with_values(0.1, 1.9, 0.1, 1.9).unwrap();
    let res = r1.compose(&r3).unwrap();
    assert_eq!(res.get_min_in_value(), 0.1);
    assert_eq!(res.get_max_in_value(), 1.0);
    assert_eq!(res.get_min_out_value(), 0.1);
    assert_eq!(res.get_max_out_value(), 1.0);

    let r4 = RangeOpData::with_values(0.1, 1.9, 0.2, 1.8).unwrap();
    let res = r1.compose(&r4).unwrap();
    assert_eq!(res.get_min_in_value(), 0.1);
    assert_eq!(res.get_max_in_value(), 1.0);
    assert_eq!(res.get_min_out_value(), 0.2);
    check_close(res.get_max_out_value(), 1.0, 1e-15);

    let r6 = RangeOpData::with_values(-1.0, 1.0, 0., 1.2).unwrap();
    let res = r1.compose(&r6).unwrap();
    assert_eq!(res.get_min_in_value(), 0.);
    assert_eq!(res.get_max_in_value(), 1.);
    assert_eq!(res.get_min_out_value(), 0.6);
    assert_eq!(res.get_max_out_value(), 1.2);

    let r7 = RangeOpData::with_values(empty, 0.5, empty, 0.5).unwrap();

    let res = r7.compose(&r4).unwrap();
    assert_eq!(res.get_min_in_value(), 0.1);
    assert_eq!(res.get_max_in_value(), 0.5);
    assert_eq!(res.get_min_out_value(), 0.2);
    check_close(
        res.get_max_out_value(),
        (0.5 * 1.6 + 0.2 * 1.8 - 0.1 * 1.6) / 1.8,
        1e-15,
    );

    let res = r4.compose(&r7).unwrap();
    assert_eq!(res.get_min_in_value(), 0.1);
    check_close(res.get_max_in_value(), 0.4375, 1e-15);
    assert_eq!(res.get_min_out_value(), 0.2);
    assert_eq!(res.get_max_out_value(), 0.5);

    let r8 = RangeOpData::with_values(0.5, empty, 0.5, empty).unwrap();

    let res = r8.compose(&r3).unwrap();
    assert_eq!(res.get_min_in_value(), 0.5);
    assert_eq!(res.get_max_in_value(), 1.9);
    assert_eq!(res.get_min_out_value(), 0.5);
    assert_eq!(res.get_max_out_value(), 1.9);

    let res = r4.compose(&r8).unwrap();
    check_close(res.get_min_in_value(), 0.4375, 1e-15);
    assert_eq!(res.get_max_in_value(), 1.9);
    assert_eq!(res.get_min_out_value(), 0.5);
    assert_eq!(res.get_max_out_value(), 1.8);

    let r9 = RangeOpData::with_values(1.1, 1.9, 1.2, 1.5).unwrap();
    let res = r1.compose(&r9).unwrap();
    assert_eq!(res.get_min_in_value(), 0.0);
    assert_eq!(res.get_max_in_value(), 1.0);
    assert_eq!(res.get_min_out_value(), 1.2);
    assert_eq!(res.get_max_out_value(), 1.2);

    let r10 = RangeOpData::with_values(-1.1, -0.1, 1.1, 1.9).unwrap();
    let res = r1.compose(&r10).unwrap();
    assert_eq!(res.get_min_in_value(), 0.);
    assert_eq!(res.get_max_in_value(), 1.);
    assert_eq!(res.get_min_out_value(), 1.9);
    assert_eq!(res.get_max_out_value(), 1.9);
}

/// Port of `OCIO_ADD_TEST(RangeOpData, computed_identifier)` @ v2.5.2.
#[test]
fn computed_identifier() {
    let mut range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();
    let mut id1 = range.get_cache_id();

    range.unset_max_in_value();
    range.unset_max_out_value();
    range.set_min_out_value(range.get_min_in_value());
    let id2 = range.get_cache_id();

    assert!(id1 != id2);
    id1 = range.get_cache_id();
    assert!(id1 == id2);
}
