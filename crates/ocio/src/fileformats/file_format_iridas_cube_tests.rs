// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Iridas `.cube` format: `tests/cpu/fileformats/FileFormatIridasCube_tests.cpp`
//! @ v2.5.2.

use super::*;
use crate::fileformats::test_utils::build_ops_test;
use ocio_ops::op_data::OpData;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(FileFormatIridasCube, format_info)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(1, format_info_vec.len());
    assert_eq!("iridas_cube", format_info_vec[0].name);
    assert_eq!("cube", format_info_vec[0].extension);
    assert_eq!(
        FormatCapabilities::READ | FormatCapabilities::BAKE,
        format_info_vec[0].capabilities
    );
}

/// Port of `ReadIridasCube` (FileFormatIridasCube_tests.cpp:26-37 @ v2.5.2).
fn read_iridas_cube(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file
    let tester = LocalFileFormat;
    let sample_name = b"Memory File";
    tester.read_file(&mut is, sample_name, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormatIridasCube, read_failure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_no_error = "LUT_3D_SIZE 2\n\
            DOMAIN_MIN 0.0 0.0 0.0\n\
            DOMAIN_MAX 1.0 1.0 1.0\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        assert!(read_iridas_cube(sample_no_error).is_ok());
    }
    {
        // Wrong LUT_3D_SIZE tag
        let sample_error = "LUT_3D_SIZE 2 2\n\
            DOMAIN_MIN 0.0 0.0 0.0\n\
            DOMAIN_MAX 1.0 1.0 1.0\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        check_throw_what(
            read_iridas_cube(sample_error),
            "Malformed 'LUT_3D_SIZE' tag",
        );
    }
    {
        // Wrong DOMAIN_MIN tag
        let sample_error = "LUT_3D_SIZE 2\n\
            DOMAIN_MIN 0.0 0.0\n\
            DOMAIN_MAX 1.0 1.0 1.0\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        check_throw_what(read_iridas_cube(sample_error), "Malformed 'DOMAIN_MIN' tag");
    }
    {
        // Wrong DOMAIN_MAX tag
        let sample_error = "LUT_3D_SIZE 2\n\
            DOMAIN_MIN 0.0 0.0 0.0\n\
            DOMAIN_MAX 1.0 1.0 1.0 1.0\n\
            0.0 0.0 0.0\n\
            1.0 0.0 0.0\n\
            0.0 1.0 0.0\n\
            1.0 1.0 0.0\n\
            0.0 0.0 1.0\n\
            1.0 0.0 1.0\n\
            0.0 1.0 1.0\n\
            1.0 1.0 1.0\n";

        check_throw_what(read_iridas_cube(sample_error), "Malformed 'DOMAIN_MAX' tag");
    }
    {
        // Unexpected tag
        let sample_error = "LUT_3D_SIZE 2\n\
            DOMAIN_MIN 0.0 0.0 0.0\n\
            DOMAIN_MAX 1.0 1.0 1.0\n\
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
            read_iridas_cube(sample_error),
            "Malformed color triples specified",
        );
    }
    {
        // Wrong number of entries
        let sample_error = "LUT_3D_SIZE 2\n\
            DOMAIN_MIN 0.0 0.0 0.0\n\
            DOMAIN_MAX 1.0 1.0 1.0\n\
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
            read_iridas_cube(sample_error),
            "Incorrect number of 3D LUT entries",
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatIridasCube, whitespace_handling)` @ v2.5.2.
#[test]
fn whitespace_handling() {
    // Rust's string continuation would drop the leading white space the test is about: the
    // lines are joined as written.
    let sample = [
        "# comment\n",
        "# comment with trailing space  \n",
        "# next up various forms of empty lines\n",
        "\n",
        "   \n",
        "   \t  \n",
        "# whitespace before keywords or after data should be supported\n",
        "  LUT_3D_SIZE \t 2  \t\n",
        "\t \tDOMAIN_MIN    0.25    0.5    0.75\n",
        "\n",
        "DOMAIN_MAX\t1.5\t2.5\t3.5\n",
        "0.0 0.0 0.0\n",
        "# comments in between data should be ignored\n",
        "   1.0    0.0 \t 0.0\n",
        "0.0 1.0 0.0    \n",
        "     1.0 1.0 0.0\n",
        "    \n",
        "0.0 0.0 1.0\n",
        "1.0 0.0 1.0\n",
        "0.0 1.0 1.0\n",
        "1.0 1.0 1.0\n",
        "   \n",
        "\n",
    ]
    .concat();

    let file = read_iridas_cube(&sample).unwrap();
    assert_eq!(file.domain_min[0], 0.25f32);
    assert_eq!(file.domain_min[2], 0.75f32);
    assert_eq!(file.domain_max[0], 1.5f32);
    assert_eq!(file.domain_max[2], 3.5f32);
    assert_eq!(file.lut3d.as_ref().unwrap().get_array().get_length(), 2);
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

/// Port of `OCIO_ADD_TEST(FileFormatIridasCube, load_1d_op)` @ v2.5.2.
#[test]
fn load_1d_op() {
    let file_name = "iridas_1d.cube";
    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 3);
    assert_eq!("<FileNoOp>", ops[0].get_info());
    assert_eq!("<MatrixOffsetOp>", ops[1].get_info());
    assert_eq!("<Lut1DOp>", ops[2].get_info());

    // `OCIO_CHECK_EQUAL(double, float)`: the float compares as a double.
    let (mat_array, mat_offsets) = matrix_of(&ops, 1);
    let expected = [
        0.25f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat_array[i], f64::from(e), "{i}");
    }

    let expected = [0.5f32, -1.0, 0.0, 0.0];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat_offsets[i], f64::from(e), "{i}");
    }

    let lut = match &**ops[2].data() {
        OpData::Lut1D(lut) => lut,
        other => panic!("not a 1D LUT: {other:?}"),
    };
    assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F32);

    let lut_array = lut.get_array();
    assert_eq!(lut_array.get_num_values(), 15);
    let lut_array = lut_array.get_values();

    let expected = [
        -1.0f32, -2.0, -3.0, 0.0, 0.1, 0.2, 0.4, 0.5, 0.6, 0.8, 0.9, 1.0, 1.0, 2.1, 3.2,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(lut_array[i], e, "{i}");
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatIridasCube, load_3d_op)` @ v2.5.2.
#[test]
fn load_3d_op() {
    let file_name = "iridas_3d.cube";
    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 3);
    assert_eq!("<FileNoOp>", ops[0].get_info());
    assert_eq!("<MatrixOffsetOp>", ops[1].get_info());
    assert_eq!("<Lut3DOp>", ops[2].get_info());

    // `OCIO_CHECK_EQUAL(double, float)`: the float compares as a double.
    let (mat_array, mat_offsets) = matrix_of(&ops, 1);
    let expected = [
        0.5f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat_array[i], f64::from(e), "{i}");
    }

    let expected = [0.0f32, -1.0, 0.0, 0.0];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(mat_offsets[i], f64::from(e), "{i}");
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

/// Port of `OCIO_ADD_TEST(FileFormatIridasCube, no_shaper)` @ v2.5.2.
#[test]
fn no_shaper() {
    // check baker output
    let mut config = crate::Config::new().unwrap();
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("lnf");
        cs.set_family("lnf");
        config.add_color_space(&cs).unwrap();
        config
            .set_role(ocio_ops::parse_utils::ROLE_REFERENCE, Some(cs.name()))
            .unwrap();
    }
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("target");
        cs.set_family("target");
        config.add_color_space(&cs).unwrap();
    }

    let bout = "# Alexa conversion LUT, logc2video. Full in/full out.\n\
                # created by alexalutconv (2.11)\n\
                \n\
                LUT_3D_SIZE 2\n\
                0.000000 0.000000 0.000000\n\
                1.000000 0.000000 0.000000\n\
                0.000000 1.000000 0.000000\n\
                1.000000 1.000000 0.000000\n\
                0.000000 0.000000 1.000000\n\
                1.000000 0.000000 1.000000\n\
                0.000000 1.000000 1.000000\n\
                1.000000 1.000000 1.000000\n";

    let mut baker = Baker::new();
    baker.set_config(&config);

    baker
        .format_metadata_mut()
        .add_child_element(
            Some(ocio_ops::format_metadata::METADATA_DESCRIPTION),
            Some(b"Alexa conversion LUT, logc2video. Full in/full out."),
        )
        .unwrap();
    baker
        .format_metadata_mut()
        .add_child_element(
            Some(ocio_ops::format_metadata::METADATA_DESCRIPTION),
            Some(b"created by alexalutconv (2.11)"),
        )
        .unwrap();
    baker.set_format("iridas_cube").unwrap();
    baker.set_input_space("lnf");
    baker.set_target_space("target");
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = ocio_ops::utils::string_utils::split_by_lines(&output);
    let resvec = ocio_ops::utils::string_utils::split_by_lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        assert_eq!(
            String::from_utf8_lossy(&osvec[i]),
            String::from_utf8_lossy(&resvec[i])
        );
    }
}
