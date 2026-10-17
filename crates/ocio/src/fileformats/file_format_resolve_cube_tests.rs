// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Resolve `.cube` format:
//! `tests/cpu/fileformats/FileFormatResolveCube_tests.cpp` @ v2.5.2. Its `bake_1d`,
//! `bake_1d_shaper`, `bake_3d` and `bake_1d_3d` come with the baker (WP 4.8).

use super::*;
use crate::fileformats::test_utils::build_ops_test;
use ocio_ops::op_data::OpData;

/// Port of `ReadResolveCube` (FileFormatResolveCube_tests.cpp:15-26 @ v2.5.2).
fn read_resolve_cube(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file
    let tester = LocalFileFormat;
    let sample_name = b"Memory File";
    tester.read_file(&mut is, sample_name, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, format_info)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(1, format_info_vec.len());
    assert_eq!("resolve_cube", format_info_vec[0].name);
    assert_eq!("cube", format_info_vec[0].extension);
    assert_eq!(
        FormatCapabilities::READ | FormatCapabilities::BAKE,
        format_info_vec[0].capabilities
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, read_1d)` @ v2.5.2.
#[test]
fn read_1d() {
    let sample = "LUT_1D_SIZE 2\n\
        LUT_1D_INPUT_RANGE 0.0 1.0\n\
        0.0 0.0 0.0\n\
        1.0 0.0 0.0\n";

    assert!(read_resolve_cube(sample).is_ok());
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, read_3d)` @ v2.5.2.
#[test]
fn read_3d() {
    let sample = "LUT_3D_SIZE 2\n\
        LUT_3D_INPUT_RANGE 0.0 1.0\n\
        0.0 0.0 0.0\n\
        1.0 0.0 0.0\n\
        0.0 1.0 0.0\n\
        1.0 1.0 0.0\n\
        0.0 0.0 1.0\n\
        1.0 0.0 1.0\n\
        0.0 1.0 1.0\n\
        1.0 1.0 1.0\n";

    assert!(read_resolve_cube(sample).is_ok());
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, read_1d_3d)` @ v2.5.2.
#[test]
fn read_1d_3d() {
    let sample = "LUT_1D_SIZE 6\n\
        LUT_1D_INPUT_RANGE 0.0 1.0\n\
        LUT_3D_SIZE 3\n\
        LUT_3D_INPUT_RANGE 0.0 1.0\n\
        1.0 1.0 1.0\n\
        0.8 0.8 0.8\n\
        0.6 0.6 0.6\n\
        0.4 0.4 0.4\n\
        0.2 0.2 0.2\n\
        0.0 0.0 0.0\n\
        1.0 1.0 1.0\n\
        0.5 1.0 1.0\n\
        0.0 1.0 1.0\n\
        1.0 0.5 1.0\n\
        0.5 0.5 1.0\n\
        0.0 0.5 1.0\n\
        1.0 0.0 1.0\n\
        0.5 0.0 1.0\n\
        0.0 0.0 1.0\n\
        1.0 1.0 0.5\n\
        0.5 1.0 0.5\n\
        0.0 1.0 0.5\n\
        1.0 0.5 0.5\n\
        0.5 0.5 0.5\n\
        0.0 0.5 0.5\n\
        1.0 0.0 0.5\n\
        0.5 0.0 0.5\n\
        0.0 0.0 0.5\n\
        1.0 1.0 0.0\n\
        0.5 1.0 0.0\n\
        0.0 1.0 0.0\n\
        1.0 0.5 0.0\n\
        0.5 0.5 0.0\n\
        0.0 0.5 0.0\n\
        1.0 0.0 0.0\n\
        0.5 0.0 0.0\n\
        0.0 0.0 0.0\n";

    assert!(read_resolve_cube(sample).is_ok());
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, read_default_range)` @ v2.5.2.
#[test]
fn read_default_range() {
    let sample_1d = "LUT_1D_SIZE 2\n\
        0.0 0.0 0.0\n\
        1.0 0.0 0.0\n";

    assert!(read_resolve_cube(sample_1d).is_ok());

    let sample_3d = "LUT_3D_SIZE 2\n\
        0.0 0.0 0.0\n\
        1.0 0.0 0.0\n\
        0.0 1.0 0.0\n\
        1.0 1.0 0.0\n\
        0.0 0.0 1.0\n\
        1.0 0.0 1.0\n\
        0.0 1.0 1.0\n\
        1.0 1.0 1.0\n";

    assert!(read_resolve_cube(sample_3d).is_ok());

    let sample_1d3d = "LUT_1D_SIZE 2\n\
        LUT_3D_SIZE 2\n\
        0.0 0.0 0.0\n\
        1.0 1.0 1.0\n\
        0.0 0.0 0.0\n\
        1.0 0.0 0.0\n\
        0.0 1.0 0.0\n\
        1.0 1.0 0.0\n\
        0.0 0.0 1.0\n\
        1.0 0.0 1.0\n\
        0.0 1.0 1.0\n\
        1.0 1.0 1.0\n";

    assert!(read_resolve_cube(sample_1d3d).is_ok());
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, read_failure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Wrong LUT_3D_SIZE tag
        let sample_error = "LUT_3D_SIZE 2 2\n\
            LUT_3D_INPUT_RANGE 0.0 1.0\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        assert!(read_resolve_cube(sample_error).is_err());
    }
    {
        // Wrong LUT_3D_INPUT_RANGE tag
        let sample_error = "LUT_3D_SIZE 2\n\
            LUT_3D_INPUT_RANGE 0.0 1.0 2.0\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        assert!(read_resolve_cube(sample_error).is_err());
    }
    {
        // Comment after header
        let sample_error = "LUT_3D_SIZE 2\n\
            LUT_3D_INPUT_RANGE 0.0 1.0 2.0\n\
            # Malformed comment\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        assert!(read_resolve_cube(sample_error).is_err());
    }
    {
        // Unexpected tag
        let sample_error = "LUT_3D_SIZE 2\n\
            LUT_3D_INPUT_RANGE 0.0 1.0 2.0\n\
            WRONG_TAG\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        assert!(read_resolve_cube(sample_error).is_err());
    }
    {
        // Wrong number of entries
        let sample_error = "LUT_3D_SIZE 2\n\
            LUT_3D_INPUT_RANGE 0.0 1.0 2.0\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            0.0 1.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        assert!(read_resolve_cube(sample_error).is_err());
    }
}

/// The matrix of the op `ops[index]`: its 16 values and 4 offsets.
fn matrix_of(ops: &OpVec, index: usize) -> (Vec<f64>, [f64; 4]) {
    match &**ops[index].data() {
        OpData::Matrix(mat) => (
            mat.get_array().get_values().clone(),
            *mat.get_offsets().get_values(),
        ),
        other => panic!("not a matrix: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, load_ops)` @ v2.5.2.
#[test]
fn load_ops() {
    let file_name = "resolve_1d3d.cube";
    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 5);
    assert_eq!("<FileNoOp>", ops[0].get_info());
    assert_eq!("<MatrixOffsetOp>", ops[1].get_info());
    assert_eq!("<Lut1DOp>", ops[2].get_info());
    assert_eq!("<MatrixOffsetOp>", ops[3].get_info());
    assert_eq!("<Lut3DOp>", ops[4].get_info());

    // `OCIO_CHECK_EQUAL(double, float)`: the float compares as a double.
    let (mat_array, mat_offsets) = matrix_of(&ops, 1);
    let expected = [
        0.25f32, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat_array[i], f64::from(e), "{i}");
    }

    let expected = [0.25f32, 0.25, 0.25, 0.0];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat_offsets[i], f64::from(e), "{i}");
    }

    let lut = match &**ops[2].data() {
        OpData::Lut1D(lut) => lut,
        other => panic!("not a 1D LUT: {other:?}"),
    };
    assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F32);

    let lut_array = lut.get_array();
    assert_eq!(lut_array.get_num_values(), 18);
    let lut_array = lut_array.get_values();

    let expected = [
        3.3f32, 3.4, 3.5, 3.0, 3.1, 3.2, 2.2, 2.3, 2.4, 2.1, 2.0, 2.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(lut_array[i], e, "{i}");
    }

    let (mat3_array, _) = matrix_of(&ops, 3);
    let expected = [
        0.25f32, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat3_array[i], f64::from(e), "{i}");
    }

    // Upstream reads these offsets from the first matrix (`mat->getOffsets()`).
    let mat3_offsets = mat_offsets;
    let expected = [0.25f32, 0.25, 0.25, 0.0];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat3_offsets[i], f64::from(e), "{i}");
    }

    let lut4 = match &**ops[4].data() {
        OpData::Lut3D(lut) => lut,
        other => panic!("not a 3D LUT: {other:?}"),
    };
    assert_eq!(lut4.get_file_output_bit_depth(), BitDepth::F32);

    let lut4_array = lut4.get_array();
    assert_eq!(lut4_array.get_num_values(), 81);
    let lut4_array = lut4_array.get_values();

    // File line 11 - R:0 - G:0 - B:0
    assert_eq!(lut4_array[0], 1.1f32);
    assert_eq!(lut4_array[1], 1.1f32);
    assert_eq!(lut4_array[2], 1.1f32);

    // File line 23 - R:0 - G:1 - B:1
    assert_eq!(lut4_array[12], 1.0f32);
    assert_eq!(lut4_array[13], 0.5f32);
    assert_eq!(lut4_array[14], 0.5f32);

    // File line 31 - R:2 - G:0 - B:2
    assert_eq!(lut4_array[60], 0.0f32);
    assert_eq!(lut4_array[61], 1.0f32);
    assert_eq!(lut4_array[62], 0.0f32);
}
