// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Iridas `.itx` format: `tests/cpu/fileformats/FileFormatIridasItx_tests.cpp`
//! @ v2.5.2.

use super::*;
use crate::fileformats::test_utils::build_ops_test;
use ocio_ops::op_data::OpData;
use ocio_testkit::upstream::check_throw_what;

/// Port of `ReadIridasItx` (FileFormatIridasItx_tests.cpp:15-24 @ v2.5.2).
fn read_iridas_itx(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file
    let tester = LocalFileFormat;
    let sample_name = b"Memory File";
    tester.read_file(&mut is, sample_name, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormatIridasItx, read_failure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_no_error = "LUT_3D_SIZE 2\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        assert!(read_iridas_itx(sample_no_error).is_ok());
    }
    {
        // Wrong LUT_3D_SIZE tag
        let sample_error = "LUT_3D_SIZE 2 2\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        check_throw_what(read_iridas_itx(sample_error), "Malformed LUT_3D_SIZE tag");
    }
    {
        // Unexpected tag
        let sample_error = "LUT_3D_SIZE 2\n\
            WRONG_TAG\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        check_throw_what(
            read_iridas_itx(sample_error),
            "Malformed color triples specified",
        );
    }
    {
        // Wrong number of entries
        let sample_error = "LUT_3D_SIZE 2\n\
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

        check_throw_what(
            read_iridas_itx(sample_error),
            "Incorrect number of 3D LUT entries",
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatIridasItx, load_3d_op)` @ v2.5.2.
#[test]
fn load_3d_op() {
    let file_name = "iridas_3d.itx";
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
