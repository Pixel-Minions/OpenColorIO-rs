// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Nuke `.vf` format: `tests/cpu/fileformats/FileFormatVF_tests.cpp` @ v2.5.2.

use super::*;
use crate::fileformats::test_utils::build_ops_test;
use ocio_ops::op_data::OpData;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(FileFormatVF, format_info)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(1, format_info_vec.len());
    assert_eq!("nukevf", format_info_vec[0].name);
    assert_eq!("vf", format_info_vec[0].extension);
    assert_eq!(FormatCapabilities::READ, format_info_vec[0].capabilities);
}

/// Port of `ReadVF` (FileFormatVF_tests.cpp:30-39 @ v2.5.2).
fn read_vf(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file.
    let tester = LocalFileFormat;
    let sample_name = b"Memory File";
    tester.read_file(&mut is, sample_name, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormatVF, read_failure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_no_error = [
            "#Inventor V2.1 ascii\n",
            "grid_size 2 2 2\n",
            "global_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1 \n",
            "data\n",
            "0 0 0\n",
            "0 0 1\n",
            "0 1 0\n",
            "0 1 1\n",
            "1 0 0\n",
            "1 0 1\n",
            "1 1 0\n",
            "1 1 1\n",
        ]
        .concat();

        assert!(read_vf(&sample_no_error).is_ok());
    }
    {
        // Too much data
        let sample_error = [
            "#Inventor V2.1 ascii\n",
            "grid_size 2 2 2\n",
            "global_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1 \n",
            "data\n",
            "0 0 0\n",
            "0 0 1\n",
            "0 1 0\n",
            "0 1 1\n",
            "1 0 0\n",
            "1 0 1\n",
            "1 1 0\n",
            "1 1 0\n",
            "1 1 1\n",
        ]
        .concat();

        check_throw_what(read_vf(&sample_error), "Incorrect number of 3D LUT entries");
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatVF, load_ops)` @ v2.5.2.
#[test]
fn load_ops() {
    let vf_file_name = "nuke_3d.vf";
    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(
        &mut ops,
        vf_file_name,
        &context,
        TransformDirection::Forward,
    )
    .unwrap();

    assert_eq!(ops.len(), 3);
    assert_eq!("<FileNoOp>", ops[0].get_info());
    assert_eq!("<MatrixOffsetOp>", ops[1].get_info());
    assert_eq!("<Lut3DOp>", ops[2].get_info());

    let mat_array = match &**ops[1].data() {
        OpData::Matrix(mat) => mat.get_array().get_values().clone(),
        other => panic!("not a matrix: {other:?}"),
    };
    // `OCIO_CHECK_EQUAL(double, float)`: the float compares as a double.
    let expected = [
        2.0f32, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat_array[i], f64::from(e), "{i}");
    }

    let lut = match &**ops[2].data() {
        OpData::Lut3D(lut) => lut,
        other => panic!("not a 3D LUT: {other:?}"),
    };
    assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F32);

    let lut_array = lut.get_array();
    assert_eq!(lut_array.get_num_values(), 24);
    let lut_array = lut_array.get_values();
    let expected = [
        0.0f32, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 2.0, 0.0, 0.0, 2.0, 2.0, 2.0, 0.0, 0.0, 2.0, 0.0,
        2.0, 2.0, 2.0, 0.0, 2.0, 2.0, 2.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(lut_array[i], e, "{i}");
    }
}
