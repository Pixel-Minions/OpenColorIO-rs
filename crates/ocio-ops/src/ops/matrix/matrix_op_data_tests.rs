// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/matrix/MatrixOpData_tests.cpp` @ v2.5.2.

use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;

/// Port of `OCIO_ADD_TEST(MatrixOpData, empty)` @ v2.5.2.
#[test]
fn empty() {
    let mut m = MatrixOpData::new();
    assert!(m.is_no_op().unwrap());
    assert!(m.is_unity_diagonal());
    assert!(m.is_diagonal());
    m.validate().unwrap();
    assert_eq!(m.get_type(), OpDataType::Matrix);

    assert_eq!(m.get_array().get_length(), 4);
    assert_eq!(m.get_array().get_num_values(), 16);
    assert_eq!(m.get_array().get_num_color_components(), 4);

    m.get_array_mut().resize(3, 3);

    assert_eq!(m.get_array().get_num_values(), 9);
    assert_eq!(m.get_array().get_length(), 3);
    assert_eq!(m.get_array().get_num_color_components(), 3);
    m.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let mut m = MatrixOpData::new();
    assert!(m.is_no_op().unwrap());
    assert!(m.is_unity_diagonal());
    assert!(m.is_diagonal());
    assert!(m.is_identity().unwrap());
    m.validate().unwrap();

    // `1 + 1e-5f` is a float sum, passed as a double.
    m.set_array_value(15, f64::from(1.0f32 + 1e-5f32));

    assert!(!m.is_no_op().unwrap());
    assert!(!m.is_unity_diagonal());
    assert!(m.is_diagonal());
    assert!(!m.is_identity().unwrap());
    m.validate().unwrap();

    m.set_array_value(1, f64::from(1e-5f32));
    m.set_array_value(15, f64::from(1.0f32));

    assert!(!m.is_no_op().unwrap());
    assert!(!m.is_unity_diagonal());
    assert!(!m.is_diagonal());
    assert!(!m.is_identity().unwrap());
    m.validate().unwrap();

    assert_eq!(m.get_file_input_bit_depth(), BitDepth::Unknown);
    assert_eq!(m.get_file_output_bit_depth(), BitDepth::Unknown);
    m.set_file_input_bit_depth(BitDepth::Uint10);
    m.set_file_output_bit_depth(BitDepth::Uint8);
    assert_eq!(m.get_file_input_bit_depth(), BitDepth::Uint10);
    assert_eq!(m.get_file_output_bit_depth(), BitDepth::Uint8);

    let m1 = m.clone();
    assert_eq!(m1.get_file_input_bit_depth(), BitDepth::Uint10);
    assert_eq!(m1.get_file_output_bit_depth(), BitDepth::Uint8);

    #[allow(unused_assignments)]
    let mut m2 = MatrixOpData::new();
    m2 = m.clone();
    assert_eq!(m2.get_file_input_bit_depth(), BitDepth::Uint10);
    assert_eq!(m2.get_file_output_bit_depth(), BitDepth::Uint8);
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, offsets)` @ v2.5.2.
#[test]
fn offsets() {
    let mut m = MatrixOpData::new();
    assert!(m.is_no_op().unwrap());
    assert!(m.is_unity_diagonal());
    assert!(m.is_diagonal());
    assert!(!m.has_offsets());
    m.validate().unwrap();

    m.set_offset_value(2, f64::from(1.0f32)).unwrap();
    assert!(!m.is_no_op().unwrap());
    assert!(m.is_unity_diagonal());
    assert!(m.is_diagonal());
    assert!(m.has_offsets());
    m.validate().unwrap();
    assert_eq!(m.get_offsets()[2], f64::from(1.0f32));
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, offsets4)` @ v2.5.2.
#[test]
fn offsets4() {
    let mut m = MatrixOpData::new();
    assert!(m.is_no_op().unwrap());
    assert!(m.is_unity_diagonal());
    assert!(m.is_diagonal());
    assert!(!m.has_offsets());
    m.validate().unwrap();

    m.set_offset_value(3, f64::from(-1e-6f32)).unwrap();
    assert!(!m.is_no_op().unwrap());
    assert!(m.is_unity_diagonal());
    assert!(m.is_diagonal());
    assert!(m.has_offsets());
    m.validate().unwrap();
    assert_eq!(m.get_offsets()[3], f64::from(-1e-6f32));
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, diagonal)` @ v2.5.2.
#[test]
fn diagonal() {
    let mut p_m = MatrixOpData::create_diagonal_matrix(0.5);

    assert!(p_m.is_diagonal());
    assert!(!p_m.has_offsets());
    p_m.validate().unwrap();
    assert_eq!(p_m.get_array().get_values()[0], 0.5);
    assert_eq!(p_m.get_array().get_values()[5], 0.5);
    assert_eq!(p_m.get_array().get_values()[10], 0.5);
    assert_eq!(p_m.get_array().get_values()[15], 0.5);
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, has_alpha)` @ v2.5.2.
#[test]
fn has_alpha() {
    let mut mat = MatrixOpData::new();
    assert!(!mat.has_alpha().unwrap());

    // MATRIX_TEST_HAS_ALPHA(id, val)
    let mut test_has_alpha = |id: usize, val: f64| {
        mat.get_array_mut()[id] = val + 0.001;
        assert!(mat.has_alpha().unwrap());
        mat.get_array_mut()[id] = val;
        assert!(!mat.has_alpha().unwrap());
    };
    test_has_alpha(3, 0.0);
    test_has_alpha(7, 0.0);
    test_has_alpha(11, 0.0);
    test_has_alpha(12, 0.0);
    test_has_alpha(13, 0.0);
    test_has_alpha(14, 0.0);
    test_has_alpha(15, 1.0);

    mat.get_offsets_mut()[3] = 0.001;
    assert!(mat.has_alpha().unwrap());
    mat.get_offsets_mut()[3] = 0.0;
    assert!(!mat.has_alpha().unwrap());
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, clone)` @ v2.5.2.
#[test]
fn clone() {
    let mut reference = MatrixOpData::new();
    reference.set_offset_value(2, f64::from(1.0f32)).unwrap();
    reference.set_array_value(0, f64::from(2.0f32));

    let mut p_clone = reference.clone();

    assert!(!p_clone.is_no_op().unwrap());
    assert!(!p_clone.is_unity_diagonal());
    assert!(p_clone.is_diagonal());
    p_clone.validate().unwrap();
    assert_eq!(p_clone.get_type(), OpDataType::Matrix);
    assert_eq!(p_clone.get_offsets()[0], f64::from(0.0f32));
    assert_eq!(p_clone.get_offsets()[1], f64::from(0.0f32));
    assert_eq!(p_clone.get_offsets()[2], f64::from(1.0f32));
    assert_eq!(p_clone.get_offsets()[3], f64::from(0.0f32));
    assert!(p_clone.get_array().equals(reference.get_array()));
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, clone_offsets4)` @ v2.5.2.
#[test]
fn clone_offsets4() {
    let mut reference = MatrixOpData::new();
    reference.set_offset_value(0, f64::from(1.0f32)).unwrap();
    reference.set_offset_value(1, f64::from(2.0f32)).unwrap();
    reference.set_offset_value(2, f64::from(3.0f32)).unwrap();
    reference.set_offset_value(3, f64::from(4.0f32)).unwrap();
    reference.set_array_value(0, f64::from(2.0f32));

    let mut p_clone = reference.clone();

    assert!(!p_clone.is_no_op().unwrap());
    assert!(!p_clone.is_unity_diagonal());
    assert!(p_clone.is_diagonal());
    p_clone.validate().unwrap();
    assert_eq!(p_clone.get_type(), OpDataType::Matrix);
    assert_eq!(p_clone.get_offsets()[0], f64::from(1.0f32));
    assert_eq!(p_clone.get_offsets()[1], f64::from(2.0f32));
    assert_eq!(p_clone.get_offsets()[2], f64::from(3.0f32));
    assert_eq!(p_clone.get_offsets()[3], f64::from(4.0f32));
    assert!(p_clone.get_array().equals(reference.get_array()));
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, test_construct)` @ v2.5.2.
#[test]
fn test_construct() {
    let mut mat_op = MatrixOpData::new();

    assert_eq!(mat_op.get_id(), b"");
    assert_eq!(mat_op.get_type(), OpDataType::Matrix);
    assert!(
        mat_op
            .get_format_metadata()
            .get_children_elements()
            .is_empty()
    );
    assert_eq!(mat_op.get_offsets()[0], f64::from(0.0f32));
    assert_eq!(mat_op.get_offsets()[1], f64::from(0.0f32));
    assert_eq!(mat_op.get_offsets()[2], f64::from(0.0f32));
    assert_eq!(mat_op.get_offsets()[3], f64::from(0.0f32));
    assert_eq!(mat_op.get_array().get_length(), 4);
    assert_eq!(mat_op.get_array().get_num_color_components(), 4);
    assert_eq!(mat_op.get_array().get_num_values(), 16);
    let val = mat_op.get_array().get_values();
    assert_eq!(val.len(), 16);
    let expected = [
        1.0f32, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(val[i], f64::from(e), "value {i}");
    }

    mat_op.validate().unwrap();

    mat_op.get_array_mut().resize(3, 3); // validate() will resize to 4x4

    assert_eq!(mat_op.get_array().get_num_values(), 9);
    assert_eq!(mat_op.get_array().get_length(), 3);
    assert_eq!(mat_op.get_array().get_num_color_components(), 3);

    mat_op.validate().unwrap();

    assert_eq!(mat_op.get_array().get_num_values(), 16);
    assert_eq!(mat_op.get_array().get_length(), 4);
    assert_eq!(mat_op.get_array().get_num_color_components(), 4);
}

/// Checks `result`'s matrix and offsets, exactly.
#[track_caller]
fn check_composition(result: &MatrixOpData, aim: &[f64; 16], aim_offs: &[f64; 4]) {
    let new_coeff = result.get_array().get_values();

    // Size check.
    assert_eq!(new_coeff.len(), 16);

    // Coefficient check.
    for i in 0..new_coeff.len() {
        assert_eq!(aim[i], new_coeff[i], "coefficient {i}");
    }

    // Offset check.
    let dim = result.get_array().get_length();
    assert_eq!(dim, 4);
    for (i, &aim_off) in aim_offs.iter().enumerate().take(dim as usize) {
        assert_eq!(aim_off, result.get_offsets()[i], "offset {i}");
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, composition)` @ v2.5.2.
#[test]
fn composition() {
    // Compose 2 forward matrices.
    {
        // Create two test ops.
        let mtx_a: [f64; 16] = [
            1., 2., 3., 4., //
            4., 5., 6., 7., //
            7., 8., 9., 10., //
            11., 12., 13., 14.,
        ];
        let offs_a: [f64; 4] = [10., 11., 12., 13.];

        let mut m_a = MatrixOpData::new();
        m_a.set_file_input_bit_depth(BitDepth::Uint8);
        m_a.set_file_output_bit_depth(BitDepth::F16);

        m_a.set_rgba(&mtx_a);
        m_a.set_rgba_offsets(&offs_a);

        let mtx_b: [f64; 16] = [
            21., 22., 23., 24., //
            24., 25., 26., 27., //
            27., 28., 29., 30., //
            31., 32., 33., 34.,
        ];
        let offs_b: [f32; 4] = [30., 31., 32., 33.];

        let mut m_b = MatrixOpData::new();
        m_b.set_file_input_bit_depth(BitDepth::F16);
        m_b.set_file_output_bit_depth(BitDepth::Uint10);

        m_b.set_rgba(&mtx_b);
        m_b.set_rgba_offsets(&offs_b);

        // Correct results.
        let aim: [f64; 16] = [
            534., 624., 714., 804., //
            603., 705., 807., 909., //
            672., 786., 900., 1014., //
            764., 894., 1024., 1154.,
        ];
        let aim_offs: [f64; 4] = [1040. + 30., 1178. + 31., 1316. + 32., 1500. + 33.];

        // Compose.
        let result = m_a.compose(&m_b).unwrap();

        // Check bit-depths copied correctly.
        assert_eq!(result.get_file_input_bit_depth(), BitDepth::Uint8);
        assert_eq!(result.get_file_output_bit_depth(), BitDepth::Uint10);

        check_composition(&result, &aim, &aim_offs);
    }

    // Compose inverse with forward.
    {
        let mtx_a: [f64; 16] = [
            2.0, 0.0, 0.0, 0.0, //
            0.0, 4.0, 0.0, 0.0, //
            0.0, 0.0, 0.5, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let offs_a: [f64; 4] = [1.0, 2.0, 0.0, 0.5];

        let mut m_a = MatrixOpData::new();
        m_a.set_rgba(&mtx_a);
        m_a.set_rgba_offsets(&offs_a);
        m_a.set_direction(TransformDirection::Inverse);

        let mtx_b: [f64; 16] = [
            2.0, 0.0, 0.0, 0.0, //
            0.0, 1.5, 0.0, 0.0, //
            0.0, 0.0, 3.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let offs_b: [f64; 4] = [2.0, 4.0, 0.0, 0.5];

        let mut m_b = MatrixOpData::new();
        m_b.set_rgba(&mtx_b);
        m_b.set_rgba_offsets(&offs_b);

        // Correct results.
        let aim: [f64; 16] = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 0.375, 0.0, 0.0, //
            0.0, 0.0, 6.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let aim_offs: [f64; 4] = [1.0, 3.25, 0.0, 0.0];

        // Compose.
        let m_a_const = m_a.get_as_forward().unwrap();
        let result = m_a_const.compose(&m_b).unwrap();

        check_composition(&result, &aim, &aim_offs);
    }

    // Compose forward with inverse.
    {
        let mtx_a: [f64; 16] = [
            2.0, 0.0, 0.0, 0.0, //
            0.0, 4.0, 0.0, 0.0, //
            0.0, 0.0, 0.5, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let offs_a: [f64; 4] = [1.0, 2.0, 0.0, 0.5];

        let mut m_a = MatrixOpData::new();
        m_a.set_rgba(&mtx_a);
        m_a.set_rgba_offsets(&offs_a);

        let mtx_b: [f64; 16] = [
            2.0, 0.0, 0.0, 0.0, //
            0.0, 0.25, 0.0, 0.0, //
            0.0, 0.0, 4.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let offs_b: [f64; 4] = [2.0, 4.0, 0.0, 0.5];

        let mut m_b = MatrixOpData::new();
        m_b.set_rgba(&mtx_b);
        m_b.set_rgba_offsets(&offs_b);
        m_b.set_direction(TransformDirection::Inverse);

        // Correct results.
        let aim: [f64; 16] = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 16.0, 0.0, 0.0, //
            0.0, 0.0, 0.125, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let aim_offs: [f64; 4] = [-0.5, -8.0, 0.0, 0.0];

        // Compose.
        let m_b_const = m_b.get_as_forward().unwrap();
        let result = m_a.compose(&m_b_const).unwrap();

        check_composition(&result, &aim, &aim_offs);
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, equality)` @ v2.5.2.
#[test]
fn equality() {
    let mut m1 = MatrixOpData::new();
    m1.set_array_value(0, 2.);

    let mut m2 = MatrixOpData::new();
    m2.set_id(b"invalid_u_id_test");
    m2.set_array_value(0, 2.);

    // id is part of metadata. FormatMetadataImpl is ignored for ==.
    assert!(m1 == m2);

    // File bit-depth is ignored for ==.
    m1.set_file_input_bit_depth(BitDepth::Uint8);
    assert!(m1 == m2);

    let mut m3 = MatrixOpData::new();
    m3.set_array_value(0, 6.);

    assert!(!(m1 == m3));

    let mut m4 = MatrixOpData::new();
    m4.set_array_value(0, 2.);

    assert!(m1 == m4);

    m4.set_offset_value(3, f64::from(1e-5f32)).unwrap();

    assert!(!(m1 == m4));
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, rgb)` @ v2.5.2.
#[test]
fn rgb() {
    let mut m = MatrixOpData::new();

    let rgb: [f32; 9] = [0., 1., 2., 3., 4., 5., 6., 7., 8.];
    m.set_rgb(&rgb);

    let v = m.get_array().get_values();
    assert_eq!(v[0], f64::from(rgb[0]));
    assert_eq!(v[1], f64::from(rgb[1]));
    assert_eq!(v[2], f64::from(rgb[2]));
    assert_eq!(v[3], f64::from(0.0f32));

    assert_eq!(v[4], f64::from(rgb[3]));
    assert_eq!(v[5], f64::from(rgb[4]));
    assert_eq!(v[6], f64::from(rgb[5]));
    assert_eq!(v[7], f64::from(0.0f32));

    assert_eq!(v[8], f64::from(rgb[6]));
    assert_eq!(v[9], f64::from(rgb[7]));
    assert_eq!(v[10], f64::from(rgb[8]));
    assert_eq!(v[11], f64::from(0.0f32));

    assert_eq!(v[12], f64::from(0.0f32));
    assert_eq!(v[13], f64::from(0.0f32));
    assert_eq!(v[14], f64::from(0.0f32));
    assert_eq!(v[15], f64::from(1.0f32));
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, rgba)` @ v2.5.2.
#[test]
fn rgba() {
    let mut matrix = MatrixOpData::new();

    // Upstream initializes 15 of the 16 values; the last is 0.
    let rgba: [f32; 16] = [
        0., 1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12., 13., 15., 0.,
    ];
    matrix.set_rgba(&rgba);

    let v = matrix.get_array().get_values();
    for i in 0..16 {
        assert_eq!(v[i], f64::from(rgba[i]));
    }

    assert!(!matrix.is_no_op().unwrap());
    assert!(matrix.has_channel_crosstalk());
    assert!(!matrix.is_diagonal());
    assert!(!matrix.is_identity().unwrap());
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, matrixInverse_identity)` @ v2.5.2.
#[test]
fn matrix_inverse_identity() {
    let mut ref_matrix_op = MatrixOpData::new();

    ref_matrix_op.set_file_input_bit_depth(BitDepth::F32);
    ref_matrix_op.set_file_output_bit_depth(BitDepth::Uint12);
    assert_eq!(BitDepth::F32, ref_matrix_op.get_file_input_bit_depth());
    assert_eq!(BitDepth::Uint12, ref_matrix_op.get_file_output_bit_depth());

    ref_matrix_op.set_direction(TransformDirection::Inverse);
    assert_eq!(BitDepth::F32, ref_matrix_op.get_file_input_bit_depth());
    assert_eq!(BitDepth::Uint12, ref_matrix_op.get_file_output_bit_depth());

    assert!(ref_matrix_op.is_no_op().unwrap());
    assert!(!ref_matrix_op.has_channel_crosstalk());
    assert!(ref_matrix_op.is_diagonal());
    assert!(ref_matrix_op.is_identity().unwrap());
    assert!(!ref_matrix_op.has_offsets());

    // Get inverse of reference matrix operation.
    let fwd_matrix_op = ref_matrix_op.get_as_forward().unwrap();
    assert_eq!(fwd_matrix_op.get_direction(), TransformDirection::Forward);

    // The getAsForward swaps the bit-depths.
    assert_eq!(
        fwd_matrix_op.get_file_input_bit_depth(),
        ref_matrix_op.get_file_output_bit_depth()
    );
    assert_eq!(
        fwd_matrix_op.get_file_output_bit_depth(),
        ref_matrix_op.get_file_input_bit_depth()
    );

    // But still be an identity matrix.
    assert!(fwd_matrix_op.is_diagonal());
    assert!(fwd_matrix_op.is_identity().unwrap());
    assert!(!fwd_matrix_op.has_offsets());
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, matrixInverse_singular)` @ v2.5.2.
#[test]
fn matrix_inverse_singular() {
    let mut singular_matrix_op = MatrixOpData::new();

    // Set singular matrix values.
    let mat: [f32; 16] = [
        1.0, 0., 0., 0.2, //
        0.0, 0., 0., 0.0, //
        0.0, 0., 0., 0.0, //
        0.2, 0., 0., 1.0,
    ];

    singular_matrix_op.set_rgba(&mat);
    singular_matrix_op.set_direction(TransformDirection::Inverse);

    assert!(!singular_matrix_op.is_no_op().unwrap());
    assert!(singular_matrix_op.has_channel_crosstalk());
    assert!(!singular_matrix_op.is_unity_diagonal());
    assert!(!singular_matrix_op.is_diagonal());
    assert!(!singular_matrix_op.is_identity().unwrap());
    assert!(!singular_matrix_op.has_offsets());

    // Get inverse of singular matrix operation.
    check_throw_what(
        singular_matrix_op.get_as_forward(),
        "Singular Matrix can't be inverted",
    );
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    let mut ref_matrix_op = MatrixOpData::new();

    // Set arbitrary matrix and offset values.
    let matrix: [f32; 16] = [
        0.9, 0.8, -0.7, 0.6, //
        -0.4, 0.5, 0.3, 0.2, //
        0.1, -0.2, 0.4, 0.3, //
        -0.5, 0.6, 0.7, 0.8,
    ];

    let offsets: [f32; 4] = [-0.1, 0.2, -0.3, 0.4];

    ref_matrix_op.set_rgba(&matrix);
    ref_matrix_op.set_rgba_offsets(&offsets);

    assert!(!ref_matrix_op.is_no_op().unwrap());
    assert!(ref_matrix_op.has_channel_crosstalk());
    assert!(!ref_matrix_op.is_diagonal());
    assert!(!ref_matrix_op.is_identity().unwrap());

    let fwd_matrix_op = ref_matrix_op.get_as_forward().unwrap();
    assert!(ref_matrix_op == fwd_matrix_op);

    ref_matrix_op.set_direction(TransformDirection::Inverse);

    assert!(!ref_matrix_op.is_no_op().unwrap());
    assert!(ref_matrix_op.has_channel_crosstalk());
    assert!(!ref_matrix_op.is_diagonal());
    assert!(!ref_matrix_op.is_identity().unwrap());

    // Get inverse of reference matrix operation.
    let inv_matrix_op = ref_matrix_op.get_as_forward().unwrap();

    let expected_matrix: [f32; 16] = [
        0.75,
        3.5,
        3.5,
        -2.75,
        0.546296296296297,
        3.90740740740741,
        1.31481481481482,
        -1.87962962962963,
        0.12037037037037,
        4.75925925925926,
        4.01851851851852,
        -2.78703703703704,
        -0.0462962962962963,
        -4.90740740740741,
        -2.31481481481482,
        3.37962962962963,
    ];

    let expected_offsets: [f32; 4] = [
        1.525,
        0.419444444444445,
        1.38055555555556,
        -1.06944444444444,
    ];

    let inv_values = inv_matrix_op.get_array().get_values();
    let inv_offsets = inv_matrix_op.get_offsets().get_values();

    // Check matrix coeffs.
    for i in 0..16 {
        check_close(
            inv_values[i],
            f64::from(expected_matrix[i]),
            f64::from(1e-6f32),
        );
    }

    // Check matrix offsets.
    for i in 0..4 {
        check_close(
            inv_offsets[i],
            f64::from(expected_offsets[i]),
            f64::from(1e-6f32),
        );
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOpData, channel_crosstalk)` @ v2.5.2.
#[test]
fn channel_crosstalk() {
    let mut ref_matrix_op = MatrixOpData::new();

    assert!(ref_matrix_op.is_no_op().unwrap());
    assert!(ref_matrix_op.is_diagonal());
    assert!(ref_matrix_op.is_identity().unwrap());

    assert!(!ref_matrix_op.has_channel_crosstalk());

    let offsets: [f32; 4] = [-0.1, 0.2, -0.3, 0.4];
    ref_matrix_op.set_rgba_offsets(&offsets);
    // False: with offsets.
    assert!(!ref_matrix_op.has_channel_crosstalk());

    let matrix: [f32; 16] = [
        0.9, 0.0, 0.0, 0.0, //
        0.0, 0.5, 0.0, 0.0, //
        0.0, 0.0, -0.4, 0.0, //
        0.0, 0.0, 0.0, 0.8,
    ];
    ref_matrix_op.set_rgba(&matrix);
    // False: with diagonal.
    assert!(!ref_matrix_op.has_channel_crosstalk());

    let matrix2: [f32; 16] = [
        1.0,
        0.0,
        0.0,
        0.0, //
        0.0,
        1.0,
        0.0,
        0.0, //
        0.0,
        0.0,
        1.0,
        0.000000001, //
        0.0,
        0.0,
        0.0,
        1.0,
    ];
    ref_matrix_op.set_rgba(&matrix2);
    // True: with off-diagonal.
    assert!(ref_matrix_op.has_channel_crosstalk());
}
