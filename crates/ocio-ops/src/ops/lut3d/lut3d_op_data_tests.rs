// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut3d/Lut3DOpData_tests.cpp` @ v2.5.2: the tests of the data. The
//! tests that read files wait for the file readers (Phase 4: `compose`, `compose_2`,
//! `inv_lut3d_lut_size`).

use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;

/// Port of `OCIO_ADD_TEST(Lut3DOpData, empty)` @ v2.5.2.
#[test]
fn empty() {
    let l = Lut3DOpData::new(2).unwrap();
    l.validate().unwrap();
    assert!(!l.is_identity());
    assert!(!l.is_no_op());
    assert_eq!(l.get_type(), OpDataType::Lut3D);
    assert_eq!(l.get_direction(), TransformDirection::Forward);
    assert!(l.has_channel_crosstalk());
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let mut interpol = Interpolation::Linear;

    let mut l = Lut3DOpData::with_interpolation(interpol, 33).unwrap();
    l.get_format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"uid"))
        .unwrap();

    assert_eq!(l.get_interpolation(), interpol);

    l.get_array_mut()[0] = 1.0f32;

    assert!(!l.is_identity());
    l.validate().unwrap();

    interpol = Interpolation::Tetrahedral;
    l.set_interpolation(interpol);
    assert_eq!(l.get_interpolation(), interpol);

    assert_eq!(l.get_array().get_length(), 33);
    assert_eq!(l.get_array().get_num_values(), 33 * 33 * 33 * 3);
    assert_eq!(l.get_array().get_num_color_components(), 3);

    l.get_array_mut().resize(17, 3).unwrap();

    assert_eq!(l.get_array().get_length(), 17);
    assert_eq!(l.get_array().get_num_values(), 17 * 17 * 17 * 3);
    assert_eq!(l.get_array().get_num_color_components(), 3);
    l.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, clone)` @ v2.5.2.
#[test]
fn clone() {
    let mut reference = Lut3DOpData::new(33).unwrap();
    reference.get_array_mut()[1] = 0.1f32;

    let p_clone = reference.clone();

    assert!(!p_clone.is_no_op());
    assert!(!p_clone.is_identity());
    p_clone.validate().unwrap();
    assert!(p_clone.get_array() == reference.get_array());
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, not_supported_length)` @ v2.5.2.
#[test]
fn not_supported_length() {
    Lut3DOpData::new(c_ulong::from(MAX_3D_LUT_LENGTH)).unwrap();
    check_throw_what(
        Lut3DOpData::new(c_ulong::from(MAX_3D_LUT_LENGTH + 1)),
        "must not be greater",
    );
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, equality)` @ v2.5.2.
#[test]
fn equality() {
    let l1 = Lut3DOpData::with_interpolation(Interpolation::Linear, 33).unwrap();

    let l2 = Lut3DOpData::with_interpolation(Interpolation::Best, 33).unwrap();

    assert!(!(l1 == l2));

    let l3 = Lut3DOpData::with_interpolation(Interpolation::Linear, 33).unwrap();

    assert!(l1 == l3);
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, interpolation)` @ v2.5.2.
#[test]
fn interpolation() {
    let mut l = Lut3DOpData::new(2).unwrap();

    l.set_interpolation(Interpolation::Linear);
    assert_eq!(l.get_interpolation(), Interpolation::Linear);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    l.validate().unwrap();

    l.set_interpolation(Interpolation::Cubic);
    assert_eq!(l.get_interpolation(), Interpolation::Cubic);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    check_throw_what(
        l.validate(),
        "does not support interpolation algorithm: cubic",
    );

    l.set_interpolation(Interpolation::Tetrahedral);
    assert_eq!(l.get_interpolation(), Interpolation::Tetrahedral);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Tetrahedral);
    l.validate().unwrap();

    l.set_interpolation(Interpolation::Default);
    assert_eq!(l.get_interpolation(), Interpolation::Default);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    l.validate().unwrap();

    l.set_interpolation(Interpolation::Best);
    assert_eq!(l.get_interpolation(), Interpolation::Best);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Tetrahedral);
    l.validate().unwrap();

    // NB: INTERP_NEAREST is currently implemented as INTERP_LINEAR.
    l.set_interpolation(Interpolation::Nearest);
    assert_eq!(l.get_interpolation(), Interpolation::Nearest);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    l.validate().unwrap();

    // Invalid interpolation type are implemented as INTERP_LINEAR
    // but can not be used because validation throws.
    l.set_interpolation(Interpolation::Unknown);
    assert_eq!(l.get_interpolation(), Interpolation::Unknown);
    assert_eq!(l.get_concrete_interpolation(), Interpolation::Linear);
    check_throw_what(
        l.validate(),
        "does not support interpolation algorithm: unknown.",
    );
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, is_inverse)` @ v2.5.2.
#[test]
fn is_inverse() {
    // Create forward LUT.
    let mut l1_nc = Lut3DOpData::with_interpolation(Interpolation::Linear, 5).unwrap();
    // Set some metadata.
    l1_nc.get_format_metadata_mut().set_name(Some(b"Forward"));
    // Make it not an identity.
    let values = l1_nc.get_array_mut().get_values_mut();
    values[0] = 20.0f32;
    assert!(!l1_nc.is_identity());

    // Create an inverse LUT with same basics.
    let l1 = l1_nc;
    let mut l2_nc = l1.inverse();
    // Change metadata.
    l2_nc.get_format_metadata_mut().set_name(Some(b"Inverse"));
    let l2 = l2_nc;
    // Inverse and forward.
    assert!(!(l1 == l2));

    // Back to forward.
    let l3 = l2.inverse();
    assert!(l3 == l1);

    // Check isInverse.
    assert!(l1.is_inverse(&l2));
    assert!(l2.is_inverse(&l1));
}

/// Upstream's LUTs are shared pointers, cloned for each composition; here each composition
/// takes references to the data.
///
/// Port of `OCIO_ADD_TEST(Lut3DOpData, compose_inverse_luts)` @ v2.5.2.
#[test]
fn compose_inverse_luts() {
    let lut_ref = Lut3DOpData::new(5).unwrap();
    let mut lut = lut_ref.clone();

    let lut_values = lut.get_array_mut().get_values_mut();
    for val in lut_values.iter_mut() {
        *val *= *val;
    }

    let lut_fwd1 = lut.clone();
    let lut_fwd2 = lut_fwd1.clone();

    // Forward + forward.
    let comp_lut_fwd_fwd = Lut3DOpData::compose(&lut_fwd1, &lut_fwd2).unwrap();
    assert_eq!(
        comp_lut_fwd_fwd.get_direction(),
        TransformDirection::Forward
    );

    // Inverse + inverse.
    lut.set_direction(TransformDirection::Inverse);
    let lut_inv1 = lut.clone();
    let _lut_inv2 = lut_inv1.clone();
    let comp_lut_inv_inv = Lut3DOpData::compose(&lut_inv1, &lut_inv1).unwrap();
    assert_eq!(
        comp_lut_inv_inv.get_direction(),
        TransformDirection::Inverse
    );

    assert!(comp_lut_fwd_fwd.get_array().get_values() == comp_lut_inv_inv.get_array().get_values());

    // Forward + inverse.
    let comp_lut_fwd_inv = Lut3DOpData::compose(&lut_fwd1, &lut_inv1).unwrap();
    assert_eq!(
        comp_lut_fwd_inv.get_direction(),
        TransformDirection::Forward
    );

    assert!(comp_lut_fwd_inv.get_array().get_values() == lut_ref.get_array().get_values());

    // Inverse + forward.
    let comp_lut_inv_fwd = Lut3DOpData::compose(&lut_inv1, &lut_fwd1).unwrap();
    assert_eq!(
        comp_lut_inv_fwd.get_direction(),
        TransformDirection::Forward
    );

    const TOL: f32 = 1e-5f32;
    for i in 0..comp_lut_inv_fwd.get_array().get_values().len() / 3 {
        check_close(
            comp_lut_inv_fwd.get_array().get_values()[i * 3],
            lut_ref.get_array().get_values()[i * 3],
            TOL,
        );
    }
}
