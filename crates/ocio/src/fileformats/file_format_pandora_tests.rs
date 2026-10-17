// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Pandora formats: `tests/cpu/fileformats/FileFormatPandora_tests.cpp` @
//! v2.5.2.

use super::*;
use crate::fileformats::test_utils::build_ops_test;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::BitDepth;
use ocio_testkit::upstream::{check_close, check_throw_what};

/// Port of `OCIO_ADD_TEST(FileFormatPandora, format_info)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(2, format_info_vec.len());
    assert_eq!("pandora_mga", format_info_vec[0].name);
    assert_eq!("mga", format_info_vec[0].extension);
    assert_eq!(FormatCapabilities::READ, format_info_vec[0].capabilities);
    assert_eq!("pandora_m3d", format_info_vec[1].name);
    assert_eq!("m3d", format_info_vec[1].extension);
    assert_eq!(FormatCapabilities::READ, format_info_vec[1].capabilities);
}

/// Port of `ReadPandora` (FileFormatPandora_tests.cpp:30-39 @ v2.5.2).
fn read_pandora(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file.
    let tester = LocalFileFormat;
    let sample_name = b"Memory File";
    tester.read_file(&mut is, sample_name, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormatPandora, read_failure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_no_error = [
            "channel 3d\n",
            "in 8\n",
            "out 256\n",
            "format lut\n",
            "values red green blue\n",
            "0 0     0   0\n",
            "1 0     0 255\n",
            "2 0   255   0\n",
            "3 0   255 255\n",
            "4 255   0   0\n",
            "5 255   0 255\n",
            "6 255 255   0\n",
            "7 255 255 255\n",
        ]
        .concat();

        assert!(read_pandora(&sample_no_error).is_ok());
    }
    {
        // Wrong channel tag
        let sample_error = [
            "channel 2d\n",
            "in 8\n",
            "out 256\n",
            "format lut\n",
            "values red green blue\n",
            "0 0     0   0\n",
            "1 0     0 255\n",
            "2 0   255   0\n",
            "3 0   255 255\n",
            "4 255   0   0\n",
            "5 255   0 255\n",
            "6 255 255   0\n",
            "7 255 255 255\n",
        ]
        .concat();

        check_throw_what(
            read_pandora(&sample_error),
            "Only 3D LUTs are currently supported",
        );
    }
    {
        // No value spec (LUT will not be read)
        let sample_error = [
            "channel 3d\n",
            "in 8\n",
            "out 256\n",
            "format lut\n",
            "0 0     0   0\n",
            "1 0     0 255\n",
            "2 0   255   0\n",
            "3 0   255 255\n",
            "4 255   0   0\n",
            "5 255   0 255\n",
            "6 255 255   0\n",
            "7 255 255 255\n",
        ]
        .concat();

        check_throw_what(
            read_pandora(&sample_error),
            "Incorrect number of 3D LUT entries",
        );
    }
    {
        // Wrong entry
        let sample_error = [
            "channel 3d\n",
            "in 8\n",
            "out 256\n",
            "format lut\n",
            "values red green blue\n",
            "0 0     0   0\n",
            "1 0     0 255\n",
            "2 0   255   0\n",
            "3 0   255 255\n",
            "4 WRONG 255   0   0\n",
            "5 255   0 255\n",
            "6 255 255   0\n",
            "7 255 255 255\n",
        ]
        .concat();

        check_throw_what(read_pandora(&sample_error), "Expected to find 4 integers");
    }
    {
        // Wrong number of entries
        let sample_error = [
            "channel 3d\n",
            "in 8\n",
            "out 256\n",
            "format lut\n",
            "values red green blue\n",
            "0 0     0   0\n",
            "1 0     0 255\n",
            "2 0   255   0\n",
            "3 0   255 255\n",
            "4 255   0   0\n",
            "5 255   0 255\n",
            "6 255 255   0\n",
            "7 255 255   0\n",
            "8 255 255 255\n",
        ]
        .concat();

        check_throw_what(
            read_pandora(&sample_error),
            "Incorrect number of 3D LUT entries",
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatPandora, load_op)` @ v2.5.2.
#[test]
fn load_op() {
    let file_name = "pandora_3d.m3d";
    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 2);
    assert_eq!("<FileNoOp>", ops[0].get_info());
    assert_eq!("<Lut3DOp>", ops[1].get_info());

    let lut = match &**ops[1].data() {
        OpData::Lut3D(lut) => lut,
        other => panic!("not a 3D LUT: {other:?}"),
    };
    assert_eq!(lut.get_file_output_bit_depth(), BitDepth::Uint8);

    let lut_array = lut.get_array();
    assert_eq!(lut_array.get_num_values(), 24);
    let lut_array = lut_array.get_values();
    let error = 1e-7f32;
    let expected = [
        0.0f32, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.8, 0.0, 0.0, 0.8, 0.8, 1.0, 0.0, 0.0, 1.0, 0.0,
        1.0, 1.0, 1.0, 0.0, 1.2, 1.0, 1.2,
    ];
    for (i, &e) in expected.iter().enumerate() {
        check_close(lut_array[i], e, error);
    }
}
