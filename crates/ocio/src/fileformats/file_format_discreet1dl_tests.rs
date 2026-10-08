// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Discreet 1D LUT format: `tests/cpu/fileformats/FileFormatDiscreet1DL_tests.cpp`
//! @ v2.5.2.

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use ocio_ops::format_metadata::METADATA_NAME;
use ocio_ops::imath_half::float_to_half;

/// Port of `TestToolsStripBlank` (FileFormatDiscreet1DL_tests.cpp:15-24 @ v2.5.2): the text,
/// as `snprintf` copies it into a 200-byte buffer, stripped.
fn test_tools_strip_blank(string_to_strip_char: &str, string_result: &str) {
    let mut string_to_strip = string_to_strip_char.as_bytes().to_vec();
    string_to_strip.truncate(199);
    replace_tabs_and_strip_spaces(&mut string_to_strip);
    assert_eq!(string_result.as_bytes(), string_to_strip.as_slice());
}

/// Port of `TestToolsStripEndNewLine` (FileFormatDiscreet1DL_tests.cpp:26-36 @ v2.5.2).
fn test_tools_strip_end_new_line(string_to_strip_char: &str, string_result: &str) {
    let mut string_to_strip = string_to_strip_char.as_bytes().to_vec();
    string_to_strip.truncate(199);
    strip_end_new_line(&mut string_to_strip);
    assert_eq!(string_result.as_bytes(), string_to_strip.as_slice());
}

/// Port of `OCIO_ADD_TEST(FileFormatD1DL, test_string_util)` @ v2.5.2.
#[test]
fn test_string_util() {
    test_tools_strip_blank("this is a test", "this is a test");
    test_tools_strip_blank("   this is a test      ", "this is a test");
    test_tools_strip_blank(" \t  this\tis a test    \t  ", "this is a test");
    test_tools_strip_blank("\t \t  this is a  test    \t  \t", "this is a  test");
    test_tools_strip_blank("\t \t  this\nis a\t\ttest    \t  \t", "this\nis a  test");
    test_tools_strip_blank("", "");

    test_tools_strip_end_new_line("", "");
    test_tools_strip_end_new_line("\n", "");
    test_tools_strip_end_new_line("\r", "");
    test_tools_strip_end_new_line("a\n", "a");
    test_tools_strip_end_new_line("b\r", "b");
    test_tools_strip_end_new_line("\na", "\na");
    test_tools_strip_end_new_line("\rb", "\rb");
}

/// Port of `LoadLutFile` (FileFormatDiscreet1DL_tests.cpp:58-62 @ v2.5.2).
fn load_lut_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |stream, path| {
        LocalFileFormat.read_file(stream, path, Interpolation::Default)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatD1DL, test_lut1d_8i_8i)` @ v2.5.2.
#[test]
fn test_lut1d_8i_8i() {
    let discreet_lut = "logtolin_8to8.lut";
    let lut_file = load_lut_file(discreet_lut).unwrap();
    let lut1d = &lut_file.lut1d;

    assert_eq!(lut1d.get_id(), b"");
    assert_eq!(
        lut1d
            .get_format_metadata()
            .get_attribute_value_string(Some(METADATA_NAME)),
        b""
    );

    assert_eq!(lut1d.get_interpolation(), Interpolation::Default);
    assert_eq!(lut1d.get_file_output_bit_depth(), BitDepth::Uint8);

    assert!(!lut1d.is_input_half_domain());
    assert!(!lut1d.is_output_raw_halfs());

    assert_eq!(lut1d.get_array().get_length(), 256);
    assert_eq!(lut1d.get_array().get_num_values(), 256 * 3);
    assert_eq!(lut1d.get_array().get_num_color_components(), 3);

    // Select some samples to verify the LUT was fully read.
    let sample_interval = 13;
    let expected_sample_values: [f32; 60] = [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 22.0, 25.0, 30.0, 33.0,
        37.0, 43.0, 48.0, 52.0, 59.0, 64.0, 70.0, 78.0, 85.0, 92.0, 101.0, 109.0, 117.0, 129.0,
        138.0, 148.0, 161.0, 173.0, 185.0, 201.0, 214.0, 229.0, 248.0, 255.0, 255.0, 255.0, 255.0,
        255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0,
        255.0, 255.0, 255.0,
    ];

    let lut1d_values = lut1d.get_array().get_values();
    for (ei, li) in (0..lut1d_values.len()).step_by(sample_interval).enumerate() {
        assert_eq!(lut1d_values[li] * 255.0f32, expected_sample_values[ei]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatD1DL, test_lut1d_12i_16f)` @ v2.5.2.
#[test]
fn test_lut1d_12i_16f() {
    let discreet_lut1216fp = "Test_12to16fp.lut";
    let lut_file = load_lut_file(discreet_lut1216fp).unwrap();
    let lut1d = &lut_file.lut1d;

    assert_eq!(lut1d.get_id(), b"");
    assert_eq!(
        lut1d
            .get_format_metadata()
            .get_attribute_value_string(Some(METADATA_NAME)),
        b""
    );

    assert_eq!(lut1d.get_interpolation(), Interpolation::Default);
    assert_eq!(lut1d.get_file_output_bit_depth(), BitDepth::F16);

    assert!(!lut1d.is_input_half_domain());
    assert!(!lut1d.is_output_raw_halfs());

    assert_eq!(lut1d.get_array().get_length(), 4096);
    assert_eq!(lut1d.get_array().get_num_values(), 4096 * 3);
    assert_eq!(lut1d.get_array().get_num_color_components(), 3);

    // Select some samples to verify the LUT was fully read.
    let sample_interval = 207;
    let expected_sample_values: [u16; 60] = [
        0, 12546, 13171, 13491, 13705, 13898, 14074, 14238, 14365, 14438, 14507, 14574, 14638,
        14700, 14760, 14818, 14875, 14930, 14983, 15037, 15094, 15156, 15222, 15294, 15366, 15408,
        15453, 15501, 15553, 15609, 15669, 15733, 15802, 15876, 15954, 16038, 16128, 16224, 16327,
        16410, 16468, 16530, 16596, 16667, 16741, 16821, 16905, 16995, 17090, 17191, 17298, 17410,
        17470, 17534, 17602, 17673, 17749, 17829, 17914, 18003,
    ];

    let lut1d_values = lut1d.get_array().get_values();
    for (ei, li) in (0..lut1d_values.len()).step_by(sample_interval).enumerate() {
        assert_eq!(float_to_half(lut1d_values[li]), expected_sample_values[ei]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatD1DL, test_lut1d_16f_16f)` @ v2.5.2.
#[test]
fn test_lut1d_16f_16f() {
    let discreet_lut16fp16fp = "photo_default_16fpto16fp.lut";
    let lut_file = load_lut_file(discreet_lut16fp16fp).unwrap();
    let lut1d = &lut_file.lut1d;

    assert_eq!(lut1d.get_interpolation(), Interpolation::Default);
    assert_eq!(lut1d.get_file_output_bit_depth(), BitDepth::F16);

    assert!(lut1d.is_input_half_domain());
    assert!(!lut1d.is_output_raw_halfs());

    assert_eq!(lut1d.get_array().get_length(), 65536);
    assert_eq!(lut1d.get_array().get_num_values(), 65536 * 3);
    assert_eq!(lut1d.get_array().get_num_color_components(), 3);

    // Select some samples to verify the LUT was fully read.
    let sample_interval = 3277;
    let expected_sample_values: [f32; 60] = [
        0.0, 242.0, 554.0, 1265.0, 2463.0, 3679.0, 4918.0, 6234.0, 7815.0, 9945.0, 11918.0,
        13222.0, 14063.0, 14616.0, 14958.0, 15176.0, 15266.0, 15349.0, 15398.0, 15442.0, 15488.0,
        15536.0, 15586.0, 15637.0, 15690.0, 15745.0, 15802.0, 15862.0, 15923.0, 15987.0, 32770.0,
        33862.0, 34954.0, 36047.0, 37139.0, 38231.0, 39324.0, 40416.0, 41508.0, 42601.0, 43693.0,
        44785.0, 45878.0, 46970.0, 48062.0, 49155.0, 50247.0, 51339.0, 52432.0, 53524.0, 54616.0,
        55709.0, 56801.0, 57893.0, 58986.0, 60078.0, 61170.0, 62263.0, 63355.0, 64447.0,
    ];

    let lut1d_values = lut1d.get_array().get_values();
    for (ei, li) in (0..lut1d_values.len()).step_by(sample_interval).enumerate() {
        // `OCIO_CHECK_EQUAL(half(...).bits(), float)`: the bits compared as a float.
        assert_eq!(
            f32::from(float_to_half(lut1d_values[li])),
            expected_sample_values[ei]
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatD1DL, test_lut1d_16f_12i)` @ v2.5.2.
#[test]
fn test_lut1d_16f_12i() {
    let discreet_lut16fp12 = "Test_16fpto12.lut";
    let lut_file = load_lut_file(discreet_lut16fp12).unwrap();
    let lut1d = &lut_file.lut1d;

    assert_eq!(lut1d.get_interpolation(), Interpolation::Default);
    assert_eq!(lut1d.get_file_output_bit_depth(), BitDepth::Uint12);

    assert!(lut1d.is_input_half_domain());
    assert!(!lut1d.is_output_raw_halfs());

    assert_eq!(lut1d.get_array().get_length(), 65536);
    assert_eq!(lut1d.get_array().get_num_values(), 65536 * 3);
    assert_eq!(lut1d.get_array().get_num_color_components(), 3);

    // Select some samples to verify the LUT was fully read.
    let sample_interval = 3277;
    let expected_sample_values: [f32; 60] = [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 10.0, 36.0, 130.0, 466.0, 1585.0, 2660.0,
        3595.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0, 4095.0,
        4095.0, 4095.0, 4095.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];

    let lut1d_values = lut1d.get_array().get_values();
    for (ei, li) in (0..lut1d_values.len()).step_by(sample_interval).enumerate() {
        assert_eq!(lut1d_values[li] * 4095.0f32, expected_sample_values[ei]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatD1DL, test_bad_file)` @ v2.5.2.
#[test]
fn test_bad_file() {
    // Bad file.
    let truncated_lut = "error_truncated_file.lut";
    assert!(load_lut_file(truncated_lut).is_err());
}
