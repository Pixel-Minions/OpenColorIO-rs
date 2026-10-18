// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Resolve `.cube` format:
//! `tests/cpu/fileformats/FileFormatResolveCube_tests.cpp` @ v2.5.2.

use super::*;
use crate::fileformats::test_utils::build_ops_test;
use ocio_ops::op_data::OpData;
use ocio_testkit::upstream::check_str_float_vec_close;

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

/// The lines of `text`, as `StringUtils::SplitByLines` gives them.
fn lines(text: &[u8]) -> Vec<String> {
    ocio_ops::utils::string_utils::split_by_lines(text)
        .into_iter()
        .map(|l| String::from_utf8(l).unwrap())
        .collect()
}

/// A line and its floats, as `StringFloatVecClose` reads them.
fn floats(line: &str) -> (&str, Option<Vec<f32>>) {
    let parts = ocio_ops::utils::string_utils::split_by_white_spaces(line.as_bytes());
    let mut values = Vec::new();
    let ok = ocio_ops::parse_utils::string_vec_to_float_vec(&mut values, &parts);
    (line, ok.then_some(values))
}

/// The config of the bake tests: an `input` reference space and a `target` space.
fn input_target_config() -> crate::Config {
    let mut config = crate::Config::new().unwrap();
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("input");
        cs.set_family("input");
        config.add_color_space(&cs).unwrap();
        config
            .set_role(ocio_ops::parse_utils::ROLE_REFERENCE, Some(cs.name()))
            .unwrap();
    }
    config
}

/// A `target` color space whose saturation of 0.5 causes channel crosstalk.
fn saturated_target() -> crate::ColorSpace {
    let mut cs = crate::ColorSpace::new();
    cs.set_name("target");
    cs.set_family("target");

    // Set saturation to cause channel crosstalk, making a 3D LUT
    let mut transform1 = crate::CdlTransform::new();
    transform1.set_sat(f64::from(0.5f32));
    cs.set_transform(
        Some(&crate::Transform::from(transform1)),
        ocio_ops::open_color_types::ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cs
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, bake_1d)` @ v2.5.2.
#[test]
fn bake_1d() {
    let mut config = input_target_config();
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("target");
        cs.set_family("target");
        config.add_color_space(&cs).unwrap();
    }

    let bout = "LUT_1D_SIZE 2\n\
                0.000000 0.000000 0.000000\n\
                1.000000 1.000000 1.000000\n";

    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("resolve_cube").unwrap();
    baker.set_input_space("input");
    baker.set_target_space("target");
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = lines(&output);
    let resvec = lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        assert_eq!(osvec[i], resvec[i]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, bake_1d_shaper)` @ v2.5.2.
#[test]
fn bake_1d_shaper() {
    let my_profile = r#"
        ocio_profile_version: 1

        colorspaces:
        - !<ColorSpace>
          name : Raw
          isdata : false

        - !<ColorSpace>
          name: Log2
          isdata: false
          from_reference: !<GroupTransform>
            children:
              - !<MatrixTransform> {matrix: [5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 1]}
              - !<LogTransform> {base: 2}
              - !<MatrixTransform> {offset: [6.5, 6.5, 6.5, 0]}
              - !<MatrixTransform> {matrix: [0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 1]}
    "#;

    let config = crate::Config::create_from_stream(my_profile.as_bytes()).unwrap();

    {
        // Lin to Log
        let bout = "LUT_1D_SIZE 10\n\
                    LUT_1D_INPUT_RANGE 0.001989 16.291878\n\
                    0.000000 0.000000 0.000000\n\
                    0.756268 0.756268 0.756268\n\
                    0.833130 0.833130 0.833130\n\
                    0.878107 0.878107 0.878107\n\
                    0.910023 0.910023 0.910023\n\
                    0.934780 0.934780 0.934780\n\
                    0.955010 0.955010 0.955010\n\
                    0.972114 0.972114 0.972114\n\
                    0.986931 0.986931 0.986931\n\
                    1.000000 1.000000 1.000000\n";

        let mut baker = Baker::new();
        baker.set_config(&config);
        baker.set_format("resolve_cube").unwrap();
        baker.set_input_space("Raw");
        baker.set_target_space("Log2");
        baker.set_shaper_space("Log2");
        baker.set_cube_size(10);
        let mut output = Vec::new();
        baker.bake(&mut output).unwrap();

        //
        let osvec = lines(&output);
        let resvec = lines(bout.as_bytes());
        assert_eq!(osvec.len(), resvec.len());
        for i in 0..resvec.len() {
            assert_eq!(osvec[i], resvec[i]);
        }
    }

    {
        // Log to Lin
        let bout = "LUT_1D_SIZE 10\n\
                    0.001989 0.001989 0.001989\n\
                    0.005413 0.005413 0.005413\n\
                    0.014731 0.014731 0.014731\n\
                    0.040091 0.040091 0.040091\n\
                    0.109110 0.109110 0.109110\n\
                    0.296951 0.296951 0.296951\n\
                    0.808177 0.808177 0.808177\n\
                    2.199522 2.199522 2.199522\n\
                    5.986179 5.986179 5.986179\n\
                    16.291878 16.291878 16.291878\n";

        let mut baker = Baker::new();
        baker.set_config(&config);
        baker.set_format("resolve_cube").unwrap();
        baker.set_input_space("Log2");
        baker.set_target_space("Raw");
        baker.set_cube_size(10);
        let mut output = Vec::new();
        baker.bake(&mut output).unwrap();

        //
        let osvec = lines(&output);
        let resvec = lines(bout.as_bytes());
        assert_eq!(osvec.len(), resvec.len());
        for i in 0..resvec.len() {
            if i == 0 {
                assert_eq!(osvec[i], resvec[i]);
            } else {
                check_str_float_vec_close(floats(&osvec[i]), floats(&resvec[i]), 1e-5f32);
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, bake_3d)` @ v2.5.2.
#[test]
fn bake_3d() {
    let mut config = input_target_config();
    config.add_color_space(&saturated_target()).unwrap();

    let bout = "# OpenColorIO Test Line 1\n\
                # OpenColorIO Test Line 2\n\
                \n\
                LUT_3D_SIZE 2\n\
                0.000000 0.000000 0.000000\n\
                0.606300 0.106300 0.106300\n\
                0.357600 0.857600 0.357600\n\
                0.963900 0.963900 0.463900\n\
                0.036100 0.036100 0.536100\n\
                0.642400 0.142400 0.642400\n\
                0.393700 0.893700 0.893700\n\
                1.000000 1.000000 1.000000\n";

    let mut baker = Baker::new();
    baker.set_config(&config);
    baker
        .format_metadata_mut()
        .add_child_element(
            Some(ocio_ops::format_metadata::METADATA_DESCRIPTION),
            Some(b"OpenColorIO Test Line 1"),
        )
        .unwrap();
    baker
        .format_metadata_mut()
        .add_child_element(
            Some(ocio_ops::format_metadata::METADATA_DESCRIPTION),
            Some(b"OpenColorIO Test Line 2"),
        )
        .unwrap();
    baker.set_format("resolve_cube").unwrap();
    baker.set_input_space("input");
    baker.set_target_space("target");
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = lines(&output);
    let resvec = lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        if i <= 3 {
            assert_eq!(osvec[i], resvec[i]);
        } else {
            check_str_float_vec_close(floats(&osvec[i]), floats(&resvec[i]), 1e-5f32);
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatResolveCube, bake_1d_3d)` @ v2.5.2.
#[test]
fn bake_1d_3d() {
    let mut config = input_target_config();
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("shaper");
        cs.set_family("shaper");
        let mut transform1 = crate::ExponentTransform::new();
        let test: [f64; 4] = [2.2, 2.2, 2.2, 1.0];
        transform1.set_value(&test);
        cs.set_transform(
            Some(&crate::Transform::from(transform1)),
            ocio_ops::open_color_types::ColorSpaceDirection::ToReference,
        )
        .unwrap();
        config.add_color_space(&cs).unwrap();
    }
    config.add_color_space(&saturated_target()).unwrap();

    let bout = "LUT_1D_SIZE 10\n\
                LUT_1D_INPUT_RANGE 0.000000 1.000000\n\
                LUT_3D_SIZE 2\n\
                0.000000 0.000000 0.000000\n\
                0.368344 0.368344 0.368344\n\
                0.504760 0.504760 0.504760\n\
                0.606913 0.606913 0.606913\n\
                0.691699 0.691699 0.691699\n\
                0.765539 0.765539 0.765539\n\
                0.831684 0.831684 0.831684\n\
                0.892049 0.892049 0.892049\n\
                0.947870 0.947870 0.947870\n\
                1.000000 1.000000 1.000000\n\
                0.000000 0.000000 0.000000\n\
                0.606300 0.106300 0.106300\n\
                0.357600 0.857600 0.357600\n\
                0.963900 0.963900 0.463900\n\
                0.036100 0.036100 0.536100\n\
                0.642400 0.142400 0.642400\n\
                0.393700 0.893700 0.893700\n\
                1.000000 1.000000 1.000000\n";

    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("resolve_cube").unwrap();
    baker.set_input_space("input");
    baker.set_shaper_space("shaper");
    baker.set_target_space("target");
    baker.set_shaper_size(10);
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = lines(&output);
    let resvec = lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        if i <= 2 {
            assert_eq!(osvec[i], resvec[i]);
        } else {
            check_str_float_vec_close(floats(&osvec[i]), floats(&resvec[i]), 1e-5f32);
        }
    }
}
