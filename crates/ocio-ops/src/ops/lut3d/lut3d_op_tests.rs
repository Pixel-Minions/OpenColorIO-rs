// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut3d/Lut3DOp_tests.cpp` @ v2.5.2: the tests of the op that need no
//! file, inverse renderer or composition. `cpu_renderer_cloned`, `cpu_renderer_inverse` and
//! `cpu_renderer_lut3d_with_nan` read files (Phase 4); `Lut3D/create_transform` and
//! `Lut3DTransform/build_op` come with the transform (WP 2.2c); `performance_check` is
//! commented out upstream.

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::open_color_types::Interpolation;
use crate::ops::matrix::matrix_op::{create_min_max_op_f32, create_offset_op};

/// Port of `OCIO_ADD_TEST(Lut3DOp, inverse_comparison_check)` @ v2.5.2.
#[test]
fn inverse_comparison_check() {
    let lut_a = Lut3DOpData::new(32).unwrap();
    let lut_b = Lut3DOpData::new(16).unwrap();

    let mut ops = OpVec::new();
    create_lut3d_op(&mut ops, lut_a.clone(), TransformDirection::Forward);
    create_lut3d_op(&mut ops, lut_a, TransformDirection::Inverse);
    // Add Matrix and LUT.
    create_min_max_op_f32(&mut ops, 0.5f32, 1.0f32, TransformDirection::Forward).unwrap();
    create_lut3d_op(&mut ops, lut_b.clone(), TransformDirection::Forward);
    // Add LUT and Matrix.
    create_lut3d_op(&mut ops, lut_b, TransformDirection::Inverse);
    create_min_max_op_f32(&mut ops, 0.5f32, 1.0f32, TransformDirection::Inverse).unwrap();

    assert_eq!(ops.len(), 6);

    let op1 = &ops[1];
    let op3 = &ops[3];
    let op4 = &ops[4];
    let op4_cloned = op4.clone_op().unwrap();

    assert!(ops[0].is_same_type(op1));
    assert!(ops[0].is_same_type(op3));
    assert!(ops[0].is_same_type(&op4_cloned));

    assert!(ops[0].is_inverse(op1));
    assert!(!ops[0].is_inverse(op3));
    assert!(!ops[0].is_inverse(op4));
    assert!(ops[3].is_inverse(op4));
}

/// Port of `OCIO_ADD_TEST(GenerateIdentityLut3D, throw_lut)` @ v2.5.2.
#[test]
fn throw_lut() {
    const LUT_SIZE: i32 = 3;
    let mut lut = vec![0.0f32; (LUT_SIZE * LUT_SIZE * LUT_SIZE * 3) as usize];

    check_throw_what(
        generate_identity_lut3d(&mut lut, LUT_SIZE, 2, Lut3DOrder::FastRed),
        "less than 3 channels",
    );

    // Get3DLutEdgeLenFromNumPixels with not cubic size.
    check_throw_what(
        get_3d_lut_edge_len_from_num_pixels(10),
        "Cannot infer 3D LUT size",
    );
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, create_op)` @ v2.5.2.
#[test]
fn create_op() {
    let lut = Lut3DOpData::new(3).unwrap();

    let mut ops = OpVec::new();
    create_lut3d_op(&mut ops, lut, TransformDirection::Inverse);
    assert_eq!(ops.len(), 1);

    // Inverse is fine.
    ops.finalize().unwrap();
}

/// Port of `OCIO_ADD_TEST(Lut3DOp, cache_id)` @ v2.5.2.
#[test]
fn cache_id() {
    let mut ops = OpVec::new();
    for _ in 0..2 {
        let lut = Lut3DOpData::new(3).unwrap();
        create_lut3d_op(&mut ops, lut, TransformDirection::Forward);
    }

    assert_eq!(ops.len(), 2);

    ops.validate().unwrap();

    let cache_id0 = ops[0].get_cache_id().unwrap();
    let cache_id1 = ops[1].get_cache_id().unwrap();
    assert!(!cache_id0.is_empty());
    // Identical LUTs have the same cacheID.
    assert_eq!(cache_id0, cache_id1);
}

/// Port of `OCIO_ADD_TEST(Lut3DOp, edge_len_from_num_pixels)` @ v2.5.2.
#[test]
fn edge_len_from_num_pixels() {
    check_throw_what(
        get_3d_lut_edge_len_from_num_pixels(10),
        "Cannot infer 3D LUT size",
    );
    let mut expected_res = 33;
    let res =
        get_3d_lut_edge_len_from_num_pixels(expected_res * expected_res * expected_res).unwrap();
    assert_eq!(res, expected_res);

    expected_res = 1290; // Maximum value such that v^3 is still an int.
    let res =
        get_3d_lut_edge_len_from_num_pixels(expected_res * expected_res * expected_res).unwrap();
    assert_eq!(res, expected_res);
}

/// Port of `OCIO_ADD_TEST(Lut3DOpStruct, lut3d_order)` @ v2.5.2.
#[test]
fn lut3d_order() {
    const LUT_SIZE: i32 = 3;
    let mut lut = vec![0.0f32; (LUT_SIZE * LUT_SIZE * LUT_SIZE * 3) as usize];

    generate_identity_lut3d(&mut lut, LUT_SIZE, 3, Lut3DOrder::FastRed).unwrap();

    // First 3 values have red changing.
    assert_eq!(lut[0], 0.0f32);
    assert_eq!(lut[3], 0.5f32);
    assert_eq!(lut[6], 1.0f32);
    // Blue is all 0.
    assert_eq!(lut[2], 0.0f32);
    assert_eq!(lut[5], 0.0f32);
    assert_eq!(lut[8], 0.0f32);
    // Last 3 values have red changing.
    assert_eq!(lut[72], 0.0f32);
    assert_eq!(lut[75], 0.5f32);
    assert_eq!(lut[78], 1.0f32);
    // Blue is all 1.
    assert_eq!(lut[74], 1.0f32);
    assert_eq!(lut[77], 1.0f32);
    assert_eq!(lut[80], 1.0f32);

    generate_identity_lut3d(&mut lut, LUT_SIZE, 3, Lut3DOrder::FastBlue).unwrap();

    // First 3 values have blue changing.
    assert_eq!(lut[2], 0.0f32);
    assert_eq!(lut[5], 0.5f32);
    assert_eq!(lut[8], 1.0f32);
    // Red is all 0.
    assert_eq!(lut[0], 0.0f32);
    assert_eq!(lut[3], 0.0f32);
    assert_eq!(lut[6], 0.0f32);
    // Last 3 values have blue changing.
    assert_eq!(lut[74], 0.0f32);
    assert_eq!(lut[77], 0.5f32);
    assert_eq!(lut[80], 1.0f32);
    // Red is all 1.
    assert_eq!(lut[72], 1.0f32);
    assert_eq!(lut[75], 1.0f32);
    assert_eq!(lut[78], 1.0f32);
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, lut_order)` @ v2.5.2.
#[test]
fn lut_order() {
    let p_lb = Lut3DOpData::new(3).unwrap();
    let values = p_lb.get_array().get_values();

    // First 3 values have blue changing.
    assert_eq!(values[2], 0.0f32);
    assert_eq!(values[5], 0.5f32);
    assert_eq!(values[8], 1.0f32);
    // Red is all 0.
    assert_eq!(values[0], 0.0f32);
    assert_eq!(values[3], 0.0f32);
    assert_eq!(values[6], 0.0f32);
    // Last 3 values have blue changing.
    assert_eq!(values[74], 0.0f32);
    assert_eq!(values[77], 0.5f32);
    assert_eq!(values[80], 1.0f32);
    // Red is all 1.
    assert_eq!(values[72], 1.0f32);
    assert_eq!(values[75], 1.0f32);
    assert_eq!(values[78], 1.0f32);
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, lut_combine)` @ v2.5.2.
#[test]
fn lut_combine() {
    let lut_data1 = Lut3DOpData::new(3).unwrap();
    let lut_data2 = Lut3DOpData::new(5).unwrap();

    let mut ops = OpVec::new();
    create_lut3d_op(&mut ops, lut_data1.clone(), TransformDirection::Forward);
    create_lut3d_op(&mut ops, lut_data2.clone(), TransformDirection::Forward);
    create_lut3d_op(&mut ops, lut_data1, TransformDirection::Inverse);
    create_lut3d_op(&mut ops, lut_data2, TransformDirection::Inverse);
    let offset = [1.1, -1.3, 0.3, -1.0];
    create_offset_op(&mut ops, &offset, TransformDirection::Forward);

    assert_eq!(ops.len(), 5);

    let lut_fwd1 = &ops[0];
    let lut_fwd2 = &ops[1];
    let lut_inv1 = &ops[2];
    let lut_inv2 = &ops[3];
    let mat = &ops[4];

    // LUT 3D can combine with other LUT 3D.
    assert!(lut_fwd1.can_combine_with(lut_fwd2).unwrap());
    assert!(lut_fwd1.can_combine_with(lut_inv1).unwrap());
    assert!(lut_inv1.can_combine_with(lut_inv2).unwrap());
    assert!(lut_inv1.can_combine_with(lut_fwd1).unwrap());

    // LUT 3D can't combine with other ops (like matrix).
    assert!(!lut_fwd1.can_combine_with(mat).unwrap());
    assert!(!lut_inv1.can_combine_with(mat).unwrap());
    assert!(!mat.can_combine_with(lut_fwd1).unwrap());
    assert!(!mat.can_combine_with(lut_inv1).unwrap());
}

/// Upstream's op shares `lutData` with the test, which changes it between renders; here the
/// op owns its data, so the test makes the op again after each change.
///
/// Port of `OCIO_ADD_TEST(Lut3DOp, cpu_renderer_lut3d)` @ v2.5.2.
#[test]
fn cpu_renderer_lut3d() {
    // By default, this constructor creates an 'identity LUT'.
    let mut lut_data = Lut3DOpData::with_interpolation(Interpolation::Linear, 33).unwrap();

    let op = |data: &Lut3DOpData| {
        let mut ops = OpVec::new();
        create_lut3d_op(&mut ops, data.clone(), TransformDirection::Forward);
        ops[0].clone()
    };
    let mut lut = op(&lut_data);

    lut.validate().unwrap();
    lut.finalize().unwrap();
    assert!(!lut_data.is_identity());
    assert!(!lut.is_no_op().unwrap());

    // Use an input value exactly at a grid point so the output value is
    // just the grid value, regardless of interpolation.
    let step = 1.0f32 / (lut_data.get_array().get_length() as f32 - 1.0f32);

    let mut my_image: [f32; 8] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, step, 1.0];

    {
        lut.apply(&mut my_image).unwrap();

        assert_eq!(my_image[0], 0.0f32);
        assert_eq!(my_image[1], 0.0f32);
        assert_eq!(my_image[2], 0.0f32);
        assert_eq!(my_image[3], 0.0f32);

        assert_eq!(my_image[4], 0.0f32);
        assert_eq!(my_image[5], 0.0f32);
        assert_eq!(my_image[6], step);
        assert_eq!(my_image[7], 1.0f32);
    }

    // No more an 'identity LUT 3D'.
    let arbitrary_val = 0.123456f32;
    lut_data.get_array_mut()[5] = arbitrary_val;
    lut = op(&lut_data);

    lut.validate().unwrap();
    lut.finalize().unwrap();
    assert!(!lut_data.is_identity());
    assert!(!lut.is_no_op().unwrap());

    {
        lut.apply(&mut my_image).unwrap();

        assert_eq!(my_image[0], 0.0f32);
        assert_eq!(my_image[1], 0.0f32);
        assert_eq!(my_image[2], 0.0f32);
        assert_eq!(my_image[3], 0.0f32);

        assert_eq!(my_image[4], 0.0f32);
        assert_eq!(my_image[5], 0.0f32);
        assert_eq!(my_image[6], arbitrary_val);
        assert_eq!(my_image[7], 1.0f32);
    }

    // Change interpolation.
    lut_data.set_interpolation(Interpolation::Tetrahedral);
    lut = op(&lut_data);
    lut.validate().unwrap();
    lut.finalize().unwrap();
    assert!(!lut_data.is_identity());
    assert!(!lut.is_no_op().unwrap());
    my_image[6] = step;
    {
        lut.apply(&mut my_image).unwrap();

        assert_eq!(my_image[0], 0.0f32);
        assert_eq!(my_image[1], 0.0f32);
        assert_eq!(my_image[2], 0.0f32);
        assert_eq!(my_image[3], 0.0f32);

        assert_eq!(my_image[4], 0.0f32);
        assert_eq!(my_image[5], 0.0f32);
        assert_eq!(my_image[6], arbitrary_val);
        assert_eq!(my_image[7], 1.0f32);
    }
}
