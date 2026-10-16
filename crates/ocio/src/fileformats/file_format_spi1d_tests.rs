// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the spi1d format: `tests/cpu/fileformats/FileFormatSpi1D_tests.cpp` @ v2.5.2.

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use ocio_testkit::upstream::{check_str_float_vec_close, check_throw_what};

/// Port of `OCIO_ADD_TEST(FileFormatSpi1D, format_info)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(1, format_info_vec.len());
    assert_eq!("spi1d", format_info_vec[0].name);
    assert_eq!("spi1d", format_info_vec[0].extension);
    assert_eq!(
        FormatCapabilities::READ | FormatCapabilities::BAKE,
        format_info_vec[0].capabilities
    );
}

/// Port of `LoadLutFile` (FileFormatSpi1D_tests.cpp:26-30 @ v2.5.2).
fn load_lut_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |stream, _path| {
        LocalFileFormat.read_file(stream, Interpolation::Default)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatSpi1D, test)` @ v2.5.2.
#[test]
// Upstream's literal, as written.
#[allow(clippy::excessive_precision)]
fn test() {
    let spi1d_file = "cpf.spi1d";
    let cached_file = load_lut_file(spi1d_file).unwrap();

    assert_eq!(cached_file.lut.get_file_output_bit_depth(), BitDepth::F32);

    assert_eq!(0.0f32, cached_file.from_min);
    assert_eq!(1.0f32, cached_file.from_max);

    let lut_array = cached_file.lut.get_array();
    assert_eq!(2048, lut_array.get_length());
    let lut_array = lut_array.get_values();

    assert_eq!(0.0f32, lut_array[0]);
    assert_eq!(0.0f32, lut_array[1]);
    assert_eq!(0.0f32, lut_array[2]);

    assert_eq!(4.511920005404118f32, lut_array[1970 * 3]);
    assert_eq!(4.511920005404118f32, lut_array[1970 * 3 + 1]);
    assert_eq!(4.511920005404118f32, lut_array[1970 * 3 + 2]);
}

/// Port of `ReadSpi1d` (FileFormatSpi1D_tests.cpp:57-67 @ v2.5.2).
fn read_spi1d(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file.
    let tester = LocalFileFormat;
    tester.read_file(&mut is, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormatSpi1D, read_failure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_no_error = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            \n\
            1.0\n\
            }\n";

        assert!(read_spi1d(sample_no_error).is_ok());
    }
    {
        // Version missing.
        let sample_error = "From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Could not find 'Version' Tag");
    }
    {
        // Version is not 1.
        let sample_error = "Version 2\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Only format version 1 supported");
    }
    {
        // Version can't be scanned.
        let sample_error = "Version A\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Invalid 'Version' Tag");
    }
    {
        // Version case is wrong.
        let sample_error = "VERSION 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Could not find 'Version' Tag");
    }
    {
        // From does not specify 2 floats.
        let sample_error = "Version 1\n\
            From 0.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Invalid 'From' Tag");
    }
    {
        // Length is missing.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Could not find 'Length' Tag");
    }
    {
        // Length can't be read.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Length A\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Invalid 'Length' Tag");
    }
    {
        // Component is missing.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Could not find 'Components' Tag");
    }
    {
        // Component can't be read.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components A\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Invalid 'Components' Tag");
    }
    {
        // Component not 1 or 2 or 3.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 4\n\
            {\n\
            0.0\n\
            1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Components must be [1,2,3]");
    }
    {
        // LUT too short.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Not enough entries found");
    }
    {
        // LUT too long.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            0.0\n\
            0.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Too many entries found");
    }
    {
        // Components==1 but two components specified in LUT.
        let sample_error = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.0 1.0\n\
            }\n";

        check_throw_what(read_spi1d(sample_error), "Malformed LUT line");
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatSpi1D, identity)` @ v2.5.2.
#[test]
fn identity() {
    {
        let sample_lut = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.000007\n\
            }\n";

        let parsed_lut = read_spi1d(sample_lut).unwrap();
        assert!(parsed_lut.lut.is_identity());
    }
    {
        let sample_lut = "Version 1\n\
            From 0.0 1.0\n\
            Length 2\n\
            Components 1\n\
            {\n\
            0.0\n\
            1.00001\n\
            }\n";

        let parsed_lut = read_spi1d(sample_lut).unwrap();
        assert!(!parsed_lut.lut.is_identity());
    }
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

/// Port of `OCIO_ADD_TEST(FileFormatSpi1D, bake_1d)` @ v2.5.2.
#[test]
fn bake_1d() {
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
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("target");
        cs.set_family("target");
        config.add_color_space(&cs).unwrap();
    }

    let bout = "Version 1\n\
                From 0.000000 1.000000\n\
                Length 2\n\
                Components 3\n\
                {\n    \
                0.000000 0.000000 0.000000\n    \
                1.000000 1.000000 1.000000\n\
                }\n";

    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("spi1d").unwrap();
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

/// Port of `OCIO_ADD_TEST(FileFormatSpi1D, bake_1d_shaper)` @ v2.5.2.
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
        let bout = "Version 1\n\
                    From 0.001989 16.291878\n\
                    Length 10\n\
                    Components 3\n\
                    {\n    \
                    0.000000 0.000000 0.000000\n    \
                    0.756268 0.756268 0.756268\n    \
                    0.833130 0.833130 0.833130\n    \
                    0.878107 0.878107 0.878107\n    \
                    0.910023 0.910023 0.910023\n    \
                    0.934780 0.934780 0.934780\n    \
                    0.955010 0.955010 0.955010\n    \
                    0.972114 0.972114 0.972114\n    \
                    0.986931 0.986931 0.986931\n    \
                    1.000000 1.000000 1.000000\n\
                    }\n";

        let mut baker = Baker::new();
        baker.set_config(&config);
        baker.set_format("spi1d").unwrap();
        baker.set_input_space("Raw");
        baker.set_target_space("Log2");
        // The ShaperSpace is used here to derive the range of the LUT.
        // This is needed because the range [0, 1] will not cover the full
        // extent of the log space.
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
        let bout = "Version 1\n\
                    From 0.000000 1.000000\n\
                    Length 10\n\
                    Components 3\n\
                    {\n    \
                    0.001989 0.001989 0.001989\n    \
                    0.005413 0.005413 0.005413\n    \
                    0.014731 0.014731 0.014731\n    \
                    0.040091 0.040091 0.040091\n    \
                    0.109110 0.109110 0.109110\n    \
                    0.296951 0.296951 0.296951\n    \
                    0.808177 0.808177 0.808177\n    \
                    2.199522 2.199522 2.199522\n    \
                    5.986179 5.986179 5.986179\n    \
                    16.291878 16.291878 16.291878\n\
                    }\n";

        let mut baker = Baker::new();
        baker.set_config(&config);
        baker.set_format("spi1d").unwrap();
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
            if i <= 5 || i >= 15 {
                assert_eq!(osvec[i], resvec[i]);
            } else {
                check_str_float_vec_close(floats(&osvec[i]), floats(&resvec[i]), 1e-5f32);
            }
        }
    }
}
