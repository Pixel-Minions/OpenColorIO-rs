// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp` @ v2.5.2: the tests of the forward
//! LUT. Its tests of composition and inversion (`lut_1d_compose`, `lut_1d_compose_sc`,
//! `inverse_*`, `is_inverse`, `make_fast_from_inverse_*`, `compose_inverse_luts`) come with
//! Phase 2's Lut1D (WP 2.1).

use ocio_testkit::upstream::check_throw_what;

use super::*;

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
