// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp` @ v2.5.2: the tests of the forward
//! LUT, of the inverse's set-up and of composition. `make_fast_from_inverse_*` read files
//! (Phase 4).

use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::format_metadata::{METADATA_DESCRIPTION, METADATA_ID};

/// Port of `OCIO_ADD_TEST(Lut1DOpData, get_lut_ideal_size)` @ v2.5.2.
#[test]
fn get_lut_ideal_size() {
    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::Uint8).unwrap(),
        256
    );
    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::Uint16).unwrap(),
        65536
    );

    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::F16).unwrap(),
        65536
    );
    assert_eq!(
        Lut1DOpData::get_lut_ideal_size(BitDepth::F32).unwrap(),
        65536
    );
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, constructor)` @ v2.5.2.
#[test]
fn constructor() {
    let lut = Lut1DOpData::new(2).unwrap();

    assert_eq!(lut.get_type(), OpDataType::Lut1D);
    assert!(!lut.is_no_op());
    assert!(lut.is_identity());
    assert_eq!(lut.get_array().get_length(), 2);
    assert_eq!(lut.get_interpolation(), Interpolation::Default);
    lut.validate().unwrap();

    check_throw_what(Lut1DOpData::new(0), "at least 2");
    check_throw_what(Lut1DOpData::new(1), "at least 2");
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let mut l = Lut1DOpData::new(17).unwrap();
    l.set_interpolation(Interpolation::Linear);

    assert_eq!(l.get_interpolation(), Interpolation::Linear);
    assert!(!l.is_no_op());
    assert!(l.is_identity());
    l.validate().unwrap();

    assert_eq!(l.get_hue_adjust(), Lut1DHueAdjust::None);
    l.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    assert_eq!(l.get_hue_adjust(), Lut1DHueAdjust::Dw3);

    // Note: Hue adjust does not affect identity status.
    assert!(l.is_identity());
    l.finalize().unwrap();
    assert_eq!(l.get_array().get_num_color_components(), 1);

    // Restore the number of components
    l.get_array_mut().set_num_color_components(3);
    l.get_array_mut()[1] = 1.0f32;
    assert!(!l.is_no_op());
    assert!(!l.is_identity());
    l.validate().unwrap();

    l.set_interpolation(Interpolation::Best);
    assert_eq!(l.get_interpolation(), Interpolation::Best);

    assert_eq!(l.get_array().get_length(), 17);
    assert_eq!(l.get_array().get_num_values(), 17 * 3);
    assert_eq!(l.get_array().get_num_color_components(), 3);

    check_throw_what(l.get_array_mut().resize(0, 3), "at least 2");
    check_throw_what(l.get_array_mut().resize(1, 3), "at least 2");

    l.get_array_mut().resize(65, 3).unwrap();

    assert_eq!(l.get_array().get_length(), 65);
    assert_eq!(l.get_array().get_num_values(), 65 * 3);
    assert_eq!(l.get_array().get_num_color_components(), 3);
    l.validate().unwrap();

    l.finalize().unwrap();
    assert_eq!(l.get_array().get_num_color_components(), 3);

    // Restore value.
    l.get_array_mut()[1] = 0.0f32;

    l.finalize().unwrap();
    // Finalize sets numColorComponents to 1 if the three channels are equal.
    assert_eq!(l.get_array().get_num_color_components(), 1);

    //
    // Number of components using NAN.
    //

    // Reset number of components.
    l.get_array_mut().set_num_color_components(3);

    l.get_array_mut()[0] = f32::NAN;
    l.get_array_mut()[1] = f32::NAN;
    l.get_array_mut()[2] = 0.0;

    l.finalize().unwrap();
    assert_eq!(l.get_array().get_num_color_components(), 3);

    l.get_array_mut()[2] = f32::NAN;
    l.finalize().unwrap();
    assert_eq!(l.get_array().get_num_color_components(), 1);
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, is_identity)` @ v2.5.2.
#[test]
fn is_identity() {
    let mut l1 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 1024, false).unwrap();

    assert!(l1.is_identity());

    // The tolerance will be 1e-5.
    let last_id = l1.get_array().get_values().len() - 1;
    let first = l1.get_array()[0];
    let last = l1.get_array()[last_id];

    l1.get_array_mut()[0] = first + 0.9e-5f32;
    l1.get_array_mut()[last_id] = last + 0.9e-5f32;
    assert!(l1.is_identity());

    l1.get_array_mut()[0] = first + 1.1e-5f32;
    l1.get_array_mut()[last_id] = last;
    assert!(!l1.is_identity());

    l1.get_array_mut()[0] = first;
    l1.get_array_mut()[last_id] = last + 1.1e-5f32;
    assert!(!l1.is_identity());

    let mut l2 = Lut1DOpData::with_half_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();

    let id2 = 31700 * 3;
    let first2 = l2.get_array()[0];
    let last2 = l2.get_array()[id2];

    // (float)half(1) - (float)half(0) = 5.96046448e-08f
    const ERROR_0: f32 = 5.96046448e-08f32;
    // (float)half(31701) - (float)half(31700) = 32.0f
    const ERROR_31700: f32 = 32.0f32;

    assert!(l2.is_identity());

    l2.get_array_mut()[0] = first2 + ERROR_0;
    l2.get_array_mut()[id2] = last2 + ERROR_31700;

    assert!(l2.is_identity());

    l2.get_array_mut()[0] = first2 + 2.0 * ERROR_0;
    l2.get_array_mut()[id2] = last2;

    assert!(!l2.is_identity());

    l2.get_array_mut()[0] = first2;
    l2.get_array_mut()[id2] = last2 + 2.0 * ERROR_31700;

    assert!(!l2.is_identity());
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, clone)` @ v2.5.2.
#[test]
fn clone() {
    let mut reference = Lut1DOpData::new(20).unwrap();
    reference.get_array_mut()[1] = 0.5f32;
    reference.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    let p_clone = reference.clone();

    assert!(!p_clone.is_no_op());
    assert!(!p_clone.is_identity());
    p_clone.validate().unwrap();
    assert!(p_clone.get_array() == reference.get_array());
    assert_eq!(p_clone.get_hue_adjust(), Lut1DHueAdjust::Dw3);
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, equality_test)` @ v2.5.2.
#[test]
fn equality_test() {
    let mut l1 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    let mut l2 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    l2.set_interpolation(Interpolation::Nearest);

    // LUT 1D only implements 1 style of interpolation.
    assert!(l1 == l2);

    let l3 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 65536, false).unwrap();

    assert!(!(l1 == l3) && !(l3 == l2));

    let mut l4 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 1024, false).unwrap();

    assert!(l1 == l4);

    l1.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    assert!(!(l1 == l4));

    // Inversion quality does not affect forward ops equality.
    l4.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    assert!(l1 == l4);

    // Inversion quality does not affect inverse ops equality. Even so applying the ops could
    // lead to small differences.
    let l5 = l1.inverse();
    let l6 = l4.inverse();

    assert!(l5 == l6);
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, channel)` @ v2.5.2.
#[test]
fn channel() {
    let mut l1 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 17, false).unwrap();

    let l2 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 20, false).unwrap();

    // False: identity.
    assert!(!l1.has_channel_crosstalk());
    assert!(l1.may_compose(&l2));

    l1.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    // True: hue restore is on, it's an identity LUT, but this is not tested for efficiency.
    assert!(l1.has_channel_crosstalk());

    assert!(!l1.may_compose(&l2));

    assert!(!l2.may_compose(&l1));

    l1.set_hue_adjust(Lut1DHueAdjust::None).unwrap();
    l1.get_array_mut()[1] = 3.0f32;
    // False: non-identity.
    assert!(!l1.has_channel_crosstalk());

    l1.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    // True: non-identity w/hue restore.
    assert!(l1.has_channel_crosstalk());
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, interpolation)` @ v2.5.2.
#[test]
fn interpolation() {
    let mut l = Lut1DOpData::new(17).unwrap();

    l.set_interpolation(Interpolation::Linear);
    assert_eq!(l.get_interpolation(), Interpolation::Linear);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    l.validate().unwrap();

    l.set_interpolation(Interpolation::Best);
    assert_eq!(l.get_interpolation(), Interpolation::Best);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    l.validate().unwrap();

    l.set_interpolation(Interpolation::Cubic);
    assert_eq!(l.get_interpolation(), Interpolation::Cubic);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    check_throw_what(l.validate(), "does not support interpolation algorithm");

    l.set_interpolation(Interpolation::Default);
    assert_eq!(l.get_interpolation(), Interpolation::Default);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    l.validate().unwrap();

    // TODO: INTERP_NEAREST is currently implemented as INTERP_LINEAR.
    l.set_interpolation(Interpolation::Nearest);
    assert_eq!(l.get_interpolation(), Interpolation::Nearest);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    l.validate().unwrap();

    // Invalid interpolation type do not get translated by getConcreteInterpolation.
    l.set_interpolation(Interpolation::Unknown);
    assert_eq!(l.get_interpolation(), Interpolation::Unknown);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    check_throw_what(l.validate(), "does not support interpolation algorithm");

    l.set_interpolation(Interpolation::Tetrahedral);
    assert_eq!(l.get_interpolation(), Interpolation::Tetrahedral);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    check_throw_what(l.validate(), " does not support interpolation algorithm");
}

/// The lookup domains of the bake (`MakeLookupDomain`): one entry per code for integer
/// depths, which a lookup with that depth may use and no other; a half domain for 16f, with
/// NaN codes at 0, which only 16f may look up.
///
/// Hand-derived from `MakeLookupDomain`, `fill` and `mayLookup` (Lut1DOpData.cpp:45-85,
/// 491-526 @ v2.5.2); the wheel's baked LUTs, which start from these domains, are compared
/// value for value in `tests/lut1d_bake_oracle.rs`.
#[test]
fn lookup_domains() {
    for (depth, length) in [
        (BitDepth::Uint8, 256),
        (BitDepth::Uint10, 1024),
        (BitDepth::Uint12, 4096),
        (BitDepth::Uint16, 65536),
    ] {
        let lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
        assert!(!lut.is_input_half_domain());
        assert_eq!(lut.get_array().get_length(), length);
        assert!(lut.may_lookup(depth).unwrap());
        assert!(!lut.may_lookup(BitDepth::F16).unwrap());
        assert!(!lut.may_lookup(BitDepth::F32).unwrap());
        assert!(lut.is_identity());
        lut.validate().unwrap();
    }
    let half = Lut1DOpData::make_lookup_domain(BitDepth::F16).unwrap();
    assert!(half.is_input_half_domain());
    assert_eq!(half.get_array().get_length(), 65536);
    assert!(half.may_lookup(BitDepth::F16).unwrap());
    assert!(!half.may_lookup(BitDepth::Uint16).unwrap());
    // A NaN code (0x7e00) maps to 0, an infinity (0x7c00) to itself.
    assert_eq!(half.get_array()[0x7e00 * 3], 0.0);
    assert_eq!(half.get_array()[0x7c00 * 3], f32::INFINITY);
    half.validate().unwrap();
}

/// `uid` (tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp:504 @ v2.5.2).
const UID: &[u8] = b"uid";

/// Port of `OCIO_ADD_TEST(Lut1DOpData, inverse_hueadjust)` @ v2.5.2.
#[test]
fn inverse_hueadjust() {
    let mut ref_lut1d = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    ref_lut1d
        .get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(UID))
        .unwrap();

    ref_lut1d.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    // Get inverse of reference lut1d operation.
    let inv_lut1d = ref_lut1d.inverse();

    assert_eq!(inv_lut1d.get_hue_adjust(), Lut1DHueAdjust::Dw3);
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, is_inverse)` @ v2.5.2.
#[test]
fn is_inverse() {
    // Create forward LUT.
    let mut l1 = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 5, false).unwrap();
    l1.get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(UID))
        .unwrap();

    // Make it not an identity.
    let values = l1.get_array_mut().get_values_mut();
    values[0] = 20.0f32;
    assert!(!l1.is_identity());

    // Create an inverse LUT with same basics.
    let l2 = l1.inverse();

    assert!(!(l1 == l2));

    // Check isInverse.
    assert!(l1.is_inverse(&l2));
    assert!(l2.is_inverse(&l1));
}

/// Port of `SetLutArray` (tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp:542-567 @ v2.5.2).
fn set_lut_array(op: &mut Lut1DOpData, dimension: c_ulong, channels: c_ulong, data: &[f32]) {
    let ref_array = op.get_array_mut();
    ref_array.resize(dimension, channels).unwrap();
    // The data allocated for the array is dimension * getMaxColorComponents(),
    // not dimension * channels.

    let max_channels = ref_array.get_max_color_components();
    let values = ref_array.get_values_mut();
    if channels == max_channels {
        let n = (dimension * channels) as usize;
        values[..n].copy_from_slice(&data[..n]);
    } else {
        // Set the red component, fill other with zero values.
        let max_channels = max_channels as usize;
        for i in 0..dimension as usize {
            values[i * max_channels] = data[i];
            values[i * max_channels + 1] = 0.0f32;
            values[i * max_channels + 2] = 0.0f32;
        }
    }
}

/// Port of `CheckInverse_IncreasingEffectiveDomain` (tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp:
/// 569-602 @ v2.5.2): `exp` holds each channel's `(increasing, start, end)`.
fn check_inverse_increasing_effective_domain(
    dimension: c_ulong,
    channels: c_ulong,
    fwd_array_data: &[f32],
    exp: [(bool, c_ulong, c_ulong); 3],
) {
    let mut ref_lut1d_op = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 1024, false).unwrap();
    ref_lut1d_op
        .get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(UID))
        .unwrap();

    set_lut_array(&mut ref_lut1d_op, dimension, channels, fwd_array_data);

    ref_lut1d_op.set_direction(TransformDirection::Inverse);
    ref_lut1d_op.validate().unwrap();
    ref_lut1d_op.finalize().unwrap();

    let properties = [
        *ref_lut1d_op.get_red_properties(),
        *ref_lut1d_op.get_green_properties(),
        *ref_lut1d_op.get_blue_properties(),
    ];
    for (props, (increasing, start, end)) in properties.iter().zip(exp) {
        assert_eq!(props.is_increasing, increasing);
        assert_eq!(props.start_domain, start);
        assert_eq!(props.end_domain, end);
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, inverse_increasing_effective_domain)` @ v2.5.2.
#[test]
fn inverse_increasing_effective_domain() {
    {
        #[rustfmt::skip]
        let fwd_data: [f32; 30] = [
            0.1, 0.8, 0.1,    // 0
            0.1, 0.7, 0.1,
            0.1, 0.6, 0.1,    // 2
            0.2, 0.5, 0.1,    // 3
            0.3, 0.4, 0.2,
            0.4, 0.3, 0.3,
            0.5, 0.1, 0.4,    // 6
            0.6, 0.1, 0.5,    // 7
            0.7, 0.1, 0.5,
            0.8, 0.1, 0.5,    // 9
        ];

        check_inverse_increasing_effective_domain(
            10,
            3,
            &fwd_data,
            [
                (true, 2, 9),  // increasing, flat [0, 2]
                (false, 0, 6), // decreasing, flat [6, 9]
                (true, 3, 7),  // increasing, flat [0, 3] and [7, 9]
            ],
        );
    }

    {
        #[rustfmt::skip]
        let fwd_data: [f32; 10] = [
            0.3, // 0
            0.3,
            0.3, // 2
            0.4,
            0.5,
            0.6,
            0.7,
            0.8, // 7
            0.8,
            0.8, // 9
        ];

        check_inverse_increasing_effective_domain(
            10,
            1,
            &fwd_data,
            [
                (true, 2, 7), // increasing, flat [0->2] and [7->9]
                (true, 2, 7),
                (true, 2, 7),
            ],
        );
    }

    {
        let fwd_data = [0.5f32; 10];

        check_inverse_increasing_effective_domain(
            10,
            1,
            &fwd_data,
            [(false, 0, 0), (false, 0, 0), (false, 0, 0)],
        );
    }

    {
        #[rustfmt::skip]
        let fwd_data: [f32; 10] = [
            0.8, // 0
            0.9, // reversal
            0.8, // 2
            0.5,
            0.4,
            0.3,
            0.2,
            0.1, // 7
            0.1,
            0.2, // reversal
        ];

        check_inverse_increasing_effective_domain(
            10,
            1,
            &fwd_data,
            [(false, 2, 7), (false, 2, 7), (false, 2, 7)],
        );
    }
}

/// Port of `CheckInverse_Flatten` (tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp:678-702 @
/// v2.5.2).
fn check_inverse_flatten(
    dimension: c_ulong,
    channels: c_ulong,
    fwd_array_data: &[f32],
    exp_inv_array_data: &[f32],
) {
    let mut ref_lut1d_op = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 65536, false).unwrap();
    ref_lut1d_op
        .get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(UID))
        .unwrap();

    set_lut_array(&mut ref_lut1d_op, dimension, channels, fwd_array_data);

    ref_lut1d_op.set_direction(TransformDirection::Inverse);
    ref_lut1d_op.validate().unwrap();
    ref_lut1d_op.finalize().unwrap();

    let inv_values = ref_lut1d_op.get_array().get_values();

    for i in 0..(dimension * channels) as usize {
        assert_eq!(inv_values[i], exp_inv_array_data[i]);
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, inverse_flatten_test)` @ v2.5.2.
#[test]
fn inverse_flatten_test() {
    #[rustfmt::skip]
    let fwd_data: [f32; 30] = [
        0.10, 0.90, 0.25,    // 0
        0.20, 0.80, 0.30,
        0.30, 0.70, 0.40,
        0.40, 0.60, 0.50,
        0.35, 0.50, 0.60,    // 4
        0.30, 0.55, 0.50,    // 5
        0.45, 0.60, 0.40,    // 6
        0.50, 0.65, 0.30,    // 7
        0.60, 0.45, 0.20,    // 8
        0.70, 0.50, 0.10,    // 9
    ];
    // red is increasing, with a reversal [4, 5]
    // green is decreasing, with reversals [4, 5] and [9]
    // blue is decreasing, with reversals [0, 8]

    #[rustfmt::skip]
    let exp_inv_data: [f32; 30] = [
        0.10, 0.90, 0.25,
        0.20, 0.80, 0.25,
        0.30, 0.70, 0.25,
        0.40, 0.60, 0.25,
        0.40, 0.50, 0.25,
        0.40, 0.50, 0.25,
        0.45, 0.50, 0.25,
        0.50, 0.50, 0.25,
        0.60, 0.45, 0.20,
        0.70, 0.45, 0.10,
    ];

    check_inverse_flatten(10, 3, &fwd_data, &exp_inv_data);
}

/// Port of `SetLutArrayHalf` (tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp:732-803 @ v2.5.2).
fn set_lut_array_half(op: &mut Lut1DOpData, channels: c_ulong) {
    let dimension: c_ulong = 65536;
    let ref_array = op.get_array_mut();
    ref_array.resize(dimension, channels).unwrap();
    // The data allocated for the array is dimension * getMaxColorComponents(),
    // not dimension * channels.

    let max_channels = ref_array.get_max_color_components() as usize;
    let values = ref_array.get_values_mut();
    for j in 0..channels as usize {
        for i in 0u32..65536 {
            let mut f = crate::math_utils::convert_half_bits_to_float(i as u16);
            if j == 0 {
                if i < 32768 {
                    f = 2.0f32 * f - 0.1f32;
                } else {
                    // neg domain overlaps pos with reversal
                    f = 3.0f32 * f + 0.1f32;
                }
                if (25000..32760).contains(&i) {
                    // flat spot at positive end
                    f = 10000.0f32;
                }
                if i >= 60000 {
                    // flat spot at neg end
                    f = -10000.0f32;
                }
                if i > 15000 && i < 20000 {
                    // reversal in positive side
                    f = 0.5f32;
                }
                if i > 50000 && i < 55000 {
                    // reversal in negative side
                    f = -2.0f32;
                }
            } else if j == 1 {
                if i < 32768 {
                    // decreasing function
                    f = -0.5f32 * f + 0.02f32;
                } else {
                    // gap between pos & neg at zero
                    f = -0.4f32 * f + 0.05f32;
                }
                if (25000..32760).contains(&i) {
                    // flat spot at positive end
                    f = -400.0f32;
                }
                if i >= 60000 {
                    // flat spot at neg end
                    f = 2000.0f32;
                }
                if i > 15000 && i < 20000 {
                    // reversal in positive side
                    f = -0.1f32;
                }
                if i > 50000 && i < 55000 {
                    // reversal in negative side
                    f = 1.4f32;
                }
            } else if j == 2 {
                if i < 32768 {
                    f = f.powf(1.5f32);
                } else {
                    f = -(-f).powf(0.9f32);
                }
                if i <= 11878 || (32768..=44646).contains(&i) {
                    // flat spot around zero
                    f = -0.01f32;
                }
            }
            values[i as usize * max_channels + j] = f;
        }
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, inverse_half_domain)` @ v2.5.2.
#[test]
fn inverse_half_domain() {
    let half_flags = HalfFlags::INPUT_HALF_CODE;
    let mut ref_lut1d_op = Lut1DOpData::with_half_flags(half_flags, 65536, false).unwrap();
    ref_lut1d_op
        .get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(UID))
        .unwrap();

    set_lut_array_half(&mut ref_lut1d_op, 3);

    ref_lut1d_op.set_direction(TransformDirection::Inverse);
    ref_lut1d_op.validate().unwrap();
    ref_lut1d_op.finalize().unwrap();

    let red_properties = *ref_lut1d_op.get_red_properties();
    let green_properties = *ref_lut1d_op.get_green_properties();
    let blue_properties = *ref_lut1d_op.get_blue_properties();

    let inv_values = ref_lut1d_op.get_array().get_values();

    // Check increasing/decreasing and start/end domain.
    assert!(red_properties.is_increasing);
    assert_eq!(red_properties.start_domain, 0);
    assert_eq!(red_properties.end_domain, 25000);
    assert_eq!(red_properties.neg_start_domain, 44100); // -0.2/3 (flattened to remove overlap)
    assert_eq!(red_properties.neg_end_domain, 60000);

    assert!(!green_properties.is_increasing);
    assert_eq!(green_properties.start_domain, 0);
    assert_eq!(green_properties.end_domain, 25000);
    assert_eq!(green_properties.neg_start_domain, 32768);
    assert_eq!(green_properties.neg_end_domain, 60000);

    assert!(blue_properties.is_increasing);
    assert_eq!(blue_properties.start_domain, 11878);
    assert_eq!(blue_properties.end_domain, 31743); // see note in Lut1DOpData.cpp
    assert_eq!(blue_properties.neg_start_domain, 44646);
    assert_eq!(blue_properties.neg_end_domain, 64511);

    // Check reversals are removed.
    let half_bits = |v: f32| float_to_half(v);
    assert_eq!(half_bits(inv_values[16000 * 3]), 15922); // halfToFloat(15000) * 2 - 0.1
    assert_eq!(half_bits(inv_values[52000 * 3]), 51567); // halfToFloat(50000) * 3 + 0.1
    assert_eq!(half_bits(inv_values[16000 * 3 + 1]), 46662); // halfToFloat(15000) * -0.5 + 0.02
    assert_eq!(half_bits(inv_values[52000 * 3 + 1]), 15885); // halfToFloat(50000) * -0.4 + 0.05

    let mut reversal = false;
    for i in 1..31745 {
        if inv_values[i * 3] < inv_values[(i - 1) * 3] {
            // increasing red
            reversal = true;
        }
    }
    assert!(!reversal);
    // Check no overlap at +0 and -0.
    assert!(inv_values[0] >= inv_values[32768 * 3]);
    reversal = false;
    for i in 1..31745 {
        if inv_values[i * 3 + 1] > inv_values[(i - 1) * 3 + 1] {
            // decreasing grn
            reversal = true;
        }
    }
    assert!(!reversal);
    assert!(inv_values[1] <= inv_values[32768 * 3 + 1]);
    reversal = false;
    for i in 1..31745 {
        if inv_values[i * 3 + 2] < inv_values[(i - 1) * 3 + 2] {
            // increasing blu
            reversal = true;
        }
    }
    assert!(!reversal);
    assert!(inv_values[2] >= inv_values[32768 * 3 + 2]);
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, lut_1d_compose)` @ v2.5.2.
#[test]
fn lut_1d_compose() {
    let mut lut1 = Lut1DOpData::new(10).unwrap();

    lut1.get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"lut1"))
        .unwrap();
    lut1.get_format_metadata_mut()
        .add_child_element(Some(METADATA_DESCRIPTION), Some(b"description of 'lut1'"))
        .unwrap();
    lut1.get_array_mut().resize(8, 3).unwrap();
    {
        let values = lut1.get_array_mut().get_values_mut();

        values[0] = 0.0f32;
        values[1] = 0.0f32;
        values[2] = 0.002333f32;
        values[3] = 0.0f32;
        values[4] = 0.291341f32;
        values[5] = 0.015624f32;
        values[6] = 0.106521f32;
        values[7] = 0.334331f32;
        values[8] = 0.462431f32;
        values[9] = 0.515851f32;
        values[10] = 0.474151f32;
        values[11] = 0.624611f32;
        values[12] = 0.658791f32;
        values[13] = 0.527381f32;
        values[14] = 0.685071f32;
        values[15] = 0.908501f32;
        values[16] = 0.707951f32;
        values[17] = 0.886331f32;
        values[18] = 0.926671f32;
        values[19] = 0.846431f32;
        values[20] = 1.0f32;
        values[21] = 1.0f32;
        values[22] = 1.0f32;
        values[23] = 1.0f32;
    }

    let mut lut2 = Lut1DOpData::new(10).unwrap();

    lut2.get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"lut2"))
        .unwrap();
    lut2.get_format_metadata_mut()
        .add_child_element(Some(METADATA_DESCRIPTION), Some(b"description of 'lut2'"))
        .unwrap();
    lut2.get_array_mut().resize(8, 3).unwrap();
    {
        let values = lut2.get_array_mut().get_values_mut();

        values[0] = 0.0f32;
        values[1] = 0.0f32;
        values[2] = 0.0023303f32;
        values[3] = 0.0f32;
        values[4] = 0.0029134f32;
        values[5] = 0.015624f32;
        values[6] = 0.00010081f32;
        values[7] = 0.0059806f32;
        values[8] = 0.023362f32;
        values[9] = 0.0045628f32;
        values[10] = 0.024229f32;
        values[11] = 0.05822f32;
        values[12] = 0.0082598f32;
        values[13] = 0.033831f32;
        values[14] = 0.074063f32;
        values[15] = 0.028595f32;
        values[16] = 0.075003f32;
        values[17] = 0.13552f32;
        values[18] = 0.69154f32;
        values[19] = 0.9213f32;
        values[20] = 1.0f32;
        values[21] = 0.76038f32;
        values[22] = 1.0f32;
        values[23] = 1.0f32;
    }

    let lut1_c = &lut1;
    let lut2_c = &lut2;

    {
        let result = Lut1DOpData::compose(lut1_c, lut2_c, ComposeMethod::ResampleNo).unwrap();

        assert_eq!(result.get_format_metadata().get_num_attributes(), 1);
        assert_eq!(
            result.get_format_metadata().get_attribute_name(0),
            METADATA_ID
        );
        assert_eq!(
            result.get_format_metadata().get_attribute_value(0),
            b"lut1 + lut2"
        );
        assert_eq!(result.get_format_metadata().get_num_children_elements(), 2);
        let desc1 = result.get_format_metadata().get_child_element(0).unwrap();
        assert_eq!(desc1.get_element_name(), METADATA_DESCRIPTION);
        assert_eq!(desc1.get_element_value(), b"description of 'lut1'");
        let desc2 = result.get_format_metadata().get_child_element(1).unwrap();
        assert_eq!(desc2.get_element_name(), METADATA_DESCRIPTION);
        assert_eq!(desc2.get_element_value(), b"description of 'lut2'");

        let values = result.get_array().get_values();

        assert_eq!(result.get_array().get_length(), 8);

        check_close(values[0], 0.0f32, 1e-6f32);
        check_close(values[1], 0.0f32, 1e-6f32);
        check_close(values[2], 0.00254739914f32, 1e-6f32);

        check_close(values[3], 0.0f32, 1e-6f32);
        check_close(values[4], 0.00669934973f32, 1e-6f32);
        check_close(values[5], 0.00378420483f32, 1e-6f32);

        check_close(values[6], 0.0f32, 1e-6f32);
        check_close(values[7], 0.0121908365f32, 1e-6f32);
        check_close(values[8], 0.0619750582f32, 1e-6f32);

        check_close(values[9], 0.00682150759f32, 1e-6f32);
        check_close(values[10], 0.0272925831f32, 1e-6f32);
        check_close(values[11], 0.096942015f32, 1e-6f32);

        check_close(values[12], 0.0206955168f32, 1e-6f32);
        check_close(values[13], 0.0308703855f32, 1e-6f32);
        check_close(values[14], 0.12295182f32, 1e-6f32);

        check_close(values[15], 0.716288447f32, 1e-6f32);
        check_close(values[16], 0.0731772855f32, 1e-6f32);
        check_close(values[17], 1.0f32, 1e-6f32);

        check_close(values[18], 0.725044191f32, 1e-6f32);
        check_close(values[19], 0.857842028f32, 1e-6f32);
        check_close(values[20], 1.0f32, 1e-6f32);
    }

    {
        let result = Lut1DOpData::compose(lut1_c, lut2_c, ComposeMethod::ResampleBig).unwrap();

        let values = result.get_array().get_values();

        assert_eq!(result.get_array().get_length(), 65536);

        check_close(values[0], 0.0f32, 1e-6f32);
        check_close(values[1], 0.0f32, 1e-6f32);
        check_close(values[2], 0.00254739914f32, 1e-6f32);

        check_close(values[3], 0.0f32, 1e-6f32);
        check_close(values[4], 6.34463504e-07f32, 1e-6f32);
        check_close(values[5], 0.00254753046f32, 1e-6f32);

        check_close(values[6], 0.0f32, 1e-6f32);
        check_close(values[7], 1.26915984e-06f32, 1e-6f32);
        check_close(values[8], 0.00254766271f32, 1e-6f32);

        check_close(values[9], 0.0f32, 1e-6f32);
        check_close(values[10], 1.90362334e-06f32, 1e-6f32);
        check_close(values[11], 0.00254779495f32, 1e-6f32);

        check_close(values[12], 0.0f32, 1e-6f32);
        check_close(values[13], 2.53855251e-06f32, 1e-6f32);
        check_close(values[14], 0.0025479272f32, 1e-6f32);

        check_close(values[15], 0.0f32, 1e-6f32);
        check_close(values[16], 3.17324884e-06f32, 1e-6f32);
        check_close(values[17], 0.00254805945f32, 1e-6f32);

        check_close(values[300], 0.0f32, 1e-6f32);
        check_close(values[301], 6.3463347e-05f32, 1e-6f32);
        check_close(values[302], 0.00256060902f32, 1e-6f32);

        check_close(values[900], 0.0f32, 1e-6f32);
        check_close(values[901], 0.000190390972f32, 1e-6f32);
        check_close(values[902], 0.00258703064f32, 1e-6f32);

        check_close(values[2700], 0.0f32, 1e-6f32);
        check_close(values[2701], 0.000571172219f32, 1e-6f32);
        check_close(values[2702], 0.00266629551f32, 1e-6f32);
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, lut_1d_compose_sc)` @ v2.5.2.
#[test]
fn lut_1d_compose_sc() {
    let mut lut1 = Lut1DOpData::new(2).unwrap();

    lut1.get_array_mut().resize(2, 3).unwrap();
    {
        let values = lut1.get_array_mut().get_values_mut();
        values[0] = 64.0f32;
        values[1] = 64.0f32;
        values[2] = 64.0f32;
        values[3] = 196.0f32;
        values[4] = 196.0f32;
        values[5] = 196.0f32;
    }
    lut1.scale(1.0f32 / 255.0f32);

    let mut lut2 = Lut1DOpData::new(2).unwrap();

    lut2.get_array_mut().resize(32, 3).unwrap();
    {
        let values = lut2.get_array_mut().get_values_mut();

        values[0] = 0.0000000f32;
        values[1] = 0.0000000f32;
        values[2] = 0.0023303f32;
        values[3] = 0.0000000f32;
        values[4] = 0.0001869f32;
        values[5] = 0.0052544f32;
        values[6] = 0.0000000f32;
        values[7] = 0.0010572f32;
        values[8] = 0.0096338f32;
        values[9] = 0.0000000f32;
        values[10] = 0.0029134f32;
        values[11] = 0.0156240f32;
        values[12] = 0.0001008f32;
        values[13] = 0.0059806f32;
        values[14] = 0.0233620f32;
        values[15] = 0.0007034f32;
        values[16] = 0.0104480f32;
        values[17] = 0.0329680f32;
        values[18] = 0.0021120f32;
        values[19] = 0.0164810f32;
        values[20] = 0.0445540f32;
        values[21] = 0.0045628f32;
        values[22] = 0.0242290f32;
        values[23] = 0.0582200f32;
        values[24] = 0.0082598f32;
        values[25] = 0.0338310f32;
        values[26] = 0.0740630f32;
        values[27] = 0.0133870f32;
        values[28] = 0.0454150f32;
        values[29] = 0.0921710f32;
        values[30] = 0.0201130f32;
        values[31] = 0.0591010f32;
        values[32] = 0.1126300f32;
        values[33] = 0.0285950f32;
        values[34] = 0.0750030f32;
        values[35] = 0.1355200f32;
        values[36] = 0.0389830f32;
        values[37] = 0.0932290f32;
        values[38] = 0.1609100f32;
        values[39] = 0.0514180f32;
        values[40] = 0.1138800f32;
        values[41] = 0.1888800f32;
        values[42] = 0.0660340f32;
        values[43] = 0.1370600f32;
        values[44] = 0.2195000f32;
        values[45] = 0.0829620f32;
        values[46] = 0.1628600f32;
        values[47] = 0.2528300f32;
        values[48] = 0.1023300f32;
        values[49] = 0.1913800f32;
        values[50] = 0.2889500f32;
        values[51] = 0.1242500f32;
        values[52] = 0.2227000f32;
        values[53] = 0.3279000f32;
        values[54] = 0.1488500f32;
        values[55] = 0.2569100f32;
        values[56] = 0.3697600f32;
        values[57] = 0.1762300f32;
        values[58] = 0.2940900f32;
        values[59] = 0.4145900f32;
        values[60] = 0.2065200f32;
        values[61] = 0.3343300f32;
        values[62] = 0.4624300f32;
        values[63] = 0.2398200f32;
        values[64] = 0.3777000f32;
        values[65] = 0.5133400f32;
        values[66] = 0.2762200f32;
        values[67] = 0.4242800f32;
        values[68] = 0.5673900f32;
        values[69] = 0.3158500f32;
        values[70] = 0.4741500f32;
        values[71] = 0.6246100f32;
        values[72] = 0.3587900f32;
        values[73] = 0.5273800f32;
        values[74] = 0.6850700f32;
        values[75] = 0.4051500f32;
        values[76] = 0.5840400f32;
        values[77] = 0.7488100f32;
        values[78] = 0.4550200f32;
        values[79] = 0.6442100f32;
        values[80] = 0.8158800f32;
        values[81] = 0.5085000f32;
        values[82] = 0.7079500f32;
        values[83] = 0.8863300f32;
        values[84] = 0.5656900f32;
        values[85] = 0.7753400f32;
        values[86] = 0.9602100f32;
        values[87] = 0.6266700f32;
        values[88] = 0.8464300f32;
        values[89] = 1.0000000f32;
        values[90] = 0.6915400f32;
        values[91] = 0.9213000f32;
        values[92] = 1.0000000f32;
        values[93] = 0.7603800f32;
        values[94] = 1.0000000f32;
        values[95] = 1.0000000f32;
    }

    let lut1_c = &lut1;
    let lut2_c = &lut2;

    {
        let l_comp = Lut1DOpData::compose(lut1_c, lut2_c, ComposeMethod::ResampleNo).unwrap();

        assert_eq!(l_comp.get_array().get_length(), 2);
        check_close(l_comp.get_array().get_values()[0], 0.00744791f32, 1e-6f32);
        check_close(l_comp.get_array().get_values()[1], 0.03172233f32, 1e-6f32);
        check_close(l_comp.get_array().get_values()[2], 0.07058375f32, 1e-6f32);
        check_close(l_comp.get_array().get_values()[3], 0.3513808f32, 1e-6f32);
        check_close(l_comp.get_array().get_values()[4], 0.51819527f32, 1e-6f32);
        check_close(l_comp.get_array().get_values()[5], 0.67463773f32, 1e-6f32);
    }

    {
        let l_comp = Lut1DOpData::compose(lut1_c, lut2_c, ComposeMethod::ResampleBig).unwrap();

        assert_eq!(l_comp.get_array().get_length(), 65536);
        check_close(l_comp.get_array().get_values()[0], 0.00744791f32, 1e-6f32);
        check_close(l_comp.get_array().get_values()[1], 0.03172233f32, 1e-6f32);
        check_close(l_comp.get_array().get_values()[2], 0.07058375f32, 1e-6f32);
        check_close(
            l_comp.get_array().get_values()[98688],
            0.0991418f32,
            1e-6f32,
        );
        check_close(
            l_comp.get_array().get_values()[98689],
            0.1866853f32,
            1e-6f32,
        );
        check_close(
            l_comp.get_array().get_values()[98690],
            0.2830042f32,
            1e-6f32,
        );
        check_close(
            l_comp.get_array().get_values()[196605],
            0.3513808f32,
            1e-6f32,
        );
        check_close(
            l_comp.get_array().get_values()[196606],
            0.51819527f32,
            1e-6f32,
        );
        check_close(
            l_comp.get_array().get_values()[196607],
            0.67463773f32,
            1e-6f32,
        );
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, compose_inverse_luts)` @ v2.5.2.
#[test]
fn compose_inverse_luts() {
    let lut_ref = Lut1DOpData::new(17).unwrap();
    let mut lut = Lut1DOpData::new(17).unwrap();

    for val in lut.get_array_mut().get_values_mut().iter_mut() {
        *val *= *val;
    }

    let lut_fwd1 = lut.clone();
    let lut_fwd2 = lut_fwd1.clone();

    // Forward + forward.
    let comp_lut_fwd_fwd =
        Lut1DOpData::compose(&lut_fwd1, &lut_fwd2, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(
        comp_lut_fwd_fwd.get_direction(),
        TransformDirection::Forward
    );

    // Inverse + inverse.
    let mut lut_inv1_non_const = lut.inverse();
    lut_inv1_non_const.finalize().unwrap();
    let lut_inv1 = lut_inv1_non_const;
    let mut lut_inv2_non_const = lut.inverse();
    lut_inv2_non_const.finalize().unwrap();
    let lut_inv2 = lut_inv2_non_const;
    let comp_lut_inv_inv =
        Lut1DOpData::compose(&lut_inv1, &lut_inv2, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(
        comp_lut_inv_inv.get_direction(),
        TransformDirection::Inverse
    );

    assert!(comp_lut_fwd_fwd.get_array().get_values() == comp_lut_inv_inv.get_array().get_values());

    // Forward + inverse.
    let comp_lut_fwd_inv =
        Lut1DOpData::compose(&lut_fwd1, &lut_inv1, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(
        comp_lut_fwd_inv.get_direction(),
        TransformDirection::Forward
    );

    assert!(comp_lut_fwd_inv.get_array().get_values() == lut_ref.get_array().get_values());

    // Inverse + forward.
    let comp_lut_inv_fwd =
        Lut1DOpData::compose(&lut_inv1, &lut_fwd1, ComposeMethod::ResampleNo).unwrap();
    assert_eq!(
        comp_lut_inv_fwd.get_direction(),
        TransformDirection::Forward
    );

    assert!(comp_lut_inv_fwd.is_input_half_domain());
    assert_eq!(comp_lut_inv_fwd.get_array().get_length(), 65536);
    assert_eq!(comp_lut_inv_fwd.get_array()[14336 * 3], 0.5f32);
}
