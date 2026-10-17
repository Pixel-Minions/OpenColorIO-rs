// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the CLF/CTF reader: `tests/cpu/fileformats/FileFormatCTF_tests.cpp` @ v2.5.2, the
//! tests the ported part of the reader reaches (WP 4.5b-4.5d: the parser, the Matrix and
//! Range readers). The others come with the operators' readers.

use std::ffi::c_ulong;

use super::*;
use crate::fileformats::ctf::ctf_transform::{
    CTF_PROCESS_LIST_VERSION_1_2, CTF_PROCESS_LIST_VERSION_1_3, CTF_PROCESS_LIST_VERSION_1_4,
    CtfVersion, get_elements_values,
};
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use ocio_ops::bit_depth_utils::get_bit_depth_max_value;
use ocio_ops::format_metadata::{METADATA_DESCRIPTION, METADATA_INFO};
use ocio_ops::math_utils::convert_half_bits_to_float;
use ocio_ops::op_data::OpDataType;
use ocio_ops::open_color_types::{BitDepth, Interpolation, Lut1DHueAdjust, TransformDirection};
use ocio_ops::ops::cdl::cdl_op_data::{CdlOpData, CdlOpStyle, ChannelParams};
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::log::log_op_data::{
    LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LINEAR_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE,
    LogOpData,
};
use ocio_ops::ops::lut1d::lut1d_op_data::Lut1DOpData;
use ocio_ops::ops::lut3d::lut3d_op_data::Lut3DOpData;
use ocio_ops::ops::matrix::matrix_op_data::{MatrixArray, MatrixOpData};
use ocio_ops::ops::range::range_op_data::RangeOpData;
use ocio_testkit::upstream::{check_close, check_throw_what};

/// Port of `LoadCLFFile` (FileFormatCTF_tests.cpp:25-29 @ v2.5.2).
fn load_clf_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |stream, path| {
        LocalFileFormat.read_file(stream, path)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, missing_file)` @ v2.5.2.
#[test]
fn missing_file() {
    // Test LoadCLFFile helper function with missing file.
    let ctf_file = "xxxxxxxxxxxxxxxxx.xxxxx";
    check_throw_what(load_clf_file(ctf_file), "Error opening test file.");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, wrong_format)` @ v2.5.2.
#[test]
fn wrong_format() {
    let ctf_file = "logtolin_8to8.lut";
    let cached_file = load_clf_file(ctf_file);
    assert!(cached_file.is_err());
    check_throw_what(cached_file, "not a CTF/CLF file.");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, binary_file)` @ v2.5.2.
#[test]
fn binary_file() {
    let ctf_file = "clf/illegal/image_png.clf";
    check_throw_what(load_clf_file(ctf_file), "is not a CTF/CLF file.");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, process_list_invalid_version)` @ v2.5.2.
#[test]
fn process_list_invalid_version() {
    let ctf_file = "process_list_invalid_version.ctf";
    check_throw_what(load_clf_file(ctf_file), "is not a valid version");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, clf_process_list_bad_version)` @ v2.5.2.
#[test]
fn clf_process_list_bad_version() {
    let file_name = "clf/illegal/process_list_bad_version.clf";
    check_throw_what(load_clf_file(file_name), "is not a valid version");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, process_list_higher_version)` @ v2.5.2.
#[test]
fn process_list_higher_version() {
    let ctf_file = "process_list_higher_version.ctf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Unsupported transform file version",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, clf_process_list_higher_version)` @ v2.5.2.
#[test]
fn clf_process_list_higher_version() {
    let ctf_file = "clf/illegal/process_list_higher_version.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Unsupported transform file version",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, process_list_missing)` @ v2.5.2.
#[test]
fn process_list_missing() {
    let ctf_file = "clf/illegal/process_list_missing.clf";
    check_throw_what(load_clf_file(ctf_file), "is not a CTF/CLF file.");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_missing)` @ v2.5.2.
#[test]
fn transform_missing() {
    let ctf_file = "clf/illegal/transform_missing.clf";
    check_throw_what(load_clf_file(ctf_file), "is not a CTF/CLF file.");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_missing_id)` @ v2.5.2.
#[test]
fn transform_missing_id() {
    let ctf_file = "clf/illegal/transform_missing_id.clf";
    check_throw_what(load_clf_file(ctf_file), "Required attribute 'id'");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_empty)` @ v2.5.2.
#[test]
fn transform_empty() {
    let ctf_file = "clf/illegal/transform_empty.clf";
    check_throw_what(load_clf_file(ctf_file), "No color operator");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_id_empty)` @ v2.5.2.
#[test]
fn transform_id_empty() {
    let ctf_file = "clf/illegal/transform_id_empty.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Required attribute 'id' does not have a value",
    );
}

/// The matrix of an op data, which must be one (`dynamic_pointer_cast<const MatrixOpData>`).
fn as_matrix(op: &OpData) -> &MatrixOpData {
    match op {
        OpData::Matrix(matrix) => matrix,
        _ => panic!("not a matrix: {:?}", op.get_type()),
    }
}

/// The range of an op data, which must be one (`dynamic_pointer_cast<const RangeOpData>`).
fn as_range(op: &OpData) -> &RangeOpData {
    match op {
        OpData::Range(range) => range,
        _ => panic!("not a range: {:?}", op.get_type()),
    }
}

/// Checks the 16 values of a matrix's array.
#[track_caller]
fn check_values(array: &MatrixArray, values: [f64; 16]) {
    assert_eq!(array.get_values().len() as c_ulong, array.get_num_values());
    for (i, value) in values.into_iter().enumerate() {
        assert_eq!(array.get_values()[i], value, "value {i}");
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix4x4)` @ v2.5.2.
#[test]
fn matrix4x4() {
    let ctf_file = "matrix_example4x4.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    assert!(CTF_PROCESS_LIST_VERSION_1_2 == ctf_version);

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_matrix = as_matrix(&op_list[0]);

    assert!(cached_file.transform.get_input_descriptor() == b"XYZ");
    assert!(cached_file.transform.get_output_descriptor() == b"RGB");

    assert_eq!(p_matrix.get_file_input_bit_depth(), BitDepth::F32);
    assert_eq!(p_matrix.get_file_output_bit_depth(), BitDepth::F32);

    // In file, matrix is defined by a 4x4 array.
    let array = p_matrix.get_array();
    assert_eq!(array.get_length(), 4);
    assert_eq!(array.get_num_color_components(), 4);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * array.get_length()
    );

    check_values(
        array,
        [
            3.24,
            -1.537,
            -0.49850,
            0.0,
            -0.96930,
            1.876,
            0.04156,
            0.0,
            0.05560,
            -0.204,
            // Validate double precision can be read both matrix and ...
            1.123456789012,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ],
    );

    let offsets = p_matrix.get_offsets();
    // ... offset
    assert_eq!(offsets[0], 0.987654321098);
    assert_eq!(offsets[1], 0.2);
    assert_eq!(offsets[2], 0.3);
    assert_eq!(offsets[3], 0.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_with_offset)` @ v2.5.2.
#[test]
fn matrix_with_offset() {
    let ctf_file = "matrix_offsets_example.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    // Note that the ProcessList does not have a version attribute and therefore defaults to
    // 1.2. The "4x4x3" Array syntax is only allowed in versions 1.2 or earlier.
    let ctf_version = cached_file.transform.get_ctf_version();
    assert_eq!(CTF_PROCESS_LIST_VERSION_1_2, ctf_version);

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_matrix = as_matrix(&op_list[0]);

    let array = p_matrix.get_array();
    assert_eq!(array.get_length(), 4);
    assert_eq!(array.get_num_color_components(), 4);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * array.get_length()
    );

    check_values(
        array,
        [
            3.24, -1.537, -0.49850, 0.0, -0.96930, 1.876, 0.04156, 0.0, 0.05560, -0.204, 1.0573,
            0.0, 0.0, 0.0, 0.0, 1.0,
        ],
    );

    assert_eq!(p_matrix.get_offsets()[0], 1.0);
    assert_eq!(p_matrix.get_offsets()[1], 2.0);
    assert_eq!(p_matrix.get_offsets()[2], 3.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_with_offset_1_3)` @ v2.5.2.
#[test]
fn matrix_with_offset_1_3() {
    // Matrix 4 4 3 only valid up to version 1.2.
    let ctf_file = "matrix_offsets_example_1_3.ctf";
    check_throw_what(load_clf_file(ctf_file), "Illegal array dimensions 4 4 3");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_3x3)` @ v2.5.2.
#[test]
fn matrix_1_3_3x3() {
    // Version 1.3, array 3x3x3: matrix with no alpha and no offsets.
    let ctf_file = "matrix_example_1_3_3x3.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    assert!(CTF_PROCESS_LIST_VERSION_1_3 == ctf_version);

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_matrix = as_matrix(&op_list[0]);

    assert!(cached_file.transform.get_input_descriptor() == b"XYZ");
    assert!(cached_file.transform.get_output_descriptor() == b"RGB");

    assert_eq!(p_matrix.get_file_input_bit_depth(), BitDepth::Uint10);
    assert_eq!(p_matrix.get_file_output_bit_depth(), BitDepth::Uint10);

    // 3x3 array gets extended to 4x4.
    let array = p_matrix.get_array();
    assert_eq!(array.get_length(), 4);
    assert_eq!(array.get_num_color_components(), 4);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * array.get_length()
    );

    check_values(
        array,
        [
            3.24, -1.537, -0.49850, 0.0, -0.96930, 1.876, 0.04156, 0.0, 0.05560, -0.204, 1.0573,
            0.0, 0.0, 0.0, 0.0, 1.0,
        ],
    );

    let offsets = p_matrix.get_offsets();
    assert_eq!(offsets[1], 0.0);
    assert_eq!(offsets[2], 0.0);
    assert_eq!(offsets[3], 0.0);
    assert_eq!(offsets[0], 0.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_4x4)` @ v2.5.2.
#[test]
fn matrix_1_3_4x4() {
    // Version 1.3, array 4x4x4, matrix with alpha and no offsets.
    let ctf_file = "matrix_example_1_3_4x4.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    assert!(CTF_PROCESS_LIST_VERSION_1_3 == ctf_version);

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_matrix = as_matrix(&op_list[0]);

    let array = p_matrix.get_array();
    assert_eq!(array.get_length(), 4);
    assert_eq!(array.get_num_color_components(), 4);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * array.get_length()
    );

    check_values(
        array,
        [
            3.24, -1.537, -0.49850, -0.1, -0.96930, 1.876, 0.04156, -0.2, 0.05560, -0.204, 1.0573,
            -0.3, 0.11, 0.22, 0.33, 0.4,
        ],
    );

    let offsets = p_matrix.get_offsets();
    assert_eq!(offsets[0], 0.0);
    assert_eq!(offsets[1], 0.0);
    assert_eq!(offsets[2], 0.0);
    assert_eq!(offsets[3], 0.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_offsets)` @ v2.5.2.
#[test]
fn matrix_1_3_offsets() {
    // Version 1.3, array 3x4x3: matrix only with offsets and no alpha.
    let ctf_file = "matrix_example_1_3_offsets.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    assert!(CTF_PROCESS_LIST_VERSION_1_3 == ctf_version);

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_matrix = as_matrix(&op_list[0]);

    let array = p_matrix.get_array();
    assert_eq!(array.get_length(), 4);
    assert_eq!(array.get_num_color_components(), 4);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * array.get_length()
    );

    check_values(
        array,
        [
            3.24,
            -1.537,
            -0.49850,
            f64::from(0.0f32),
            -0.96930,
            1.876,
            0.04156,
            0.0,
            0.05560,
            -0.204,
            1.0573,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ],
    );

    let offsets = p_matrix.get_offsets();
    assert_eq!(offsets[0], 0.1);
    assert_eq!(offsets[1], 0.2);
    assert_eq!(offsets[2], 0.3);
    assert_eq!(offsets[3], 0.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_1_3_alpha_offsets)` @ v2.5.2.
#[test]
fn matrix_1_3_alpha_offsets() {
    // Version 1.3, array 4x5x4: matrix with alpha and offsets.
    let ctf_file = "matrix_example_1_3_alpha_offsets.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    assert!(CTF_PROCESS_LIST_VERSION_1_3 == ctf_version);

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_matrix = as_matrix(&op_list[0]);

    let array = p_matrix.get_array();
    assert_eq!(array.get_length(), 4);
    assert_eq!(array.get_num_color_components(), 4);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * array.get_length()
    );

    check_values(
        array,
        [
            3.24, -1.537, -0.49850, 0.6, -0.96930, 1.876, 0.04156, 0.7, 0.05560, -0.204, 1.0573,
            0.8, 1.2, 1.3, 1.4, 1.5,
        ],
    );

    let offsets = p_matrix.get_offsets();
    assert_eq!(offsets[0], 0.1);
    assert_eq!(offsets[1], 0.2);
    assert_eq!(offsets[2], 0.3);
    assert_eq!(offsets[3], 0.4);
}

/// Port of `CheckIdentity` (FileFormatCTF_tests.cpp:547-563 @ v2.5.2).
#[track_caller]
fn check_identity(ctf: &str) {
    // Load file
    let mut ctf_stream = InputStream::from_bytes(ctf.as_bytes());
    let cached_file = LocalFileFormat.read_file(&mut ctf_stream, b"").unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let mat = as_matrix(&file_ops[0]);
    assert!(mat.is_identity().unwrap());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_identity)` @ v2.5.2.
#[test]
fn matrix_identity() {
    // Pre version 1.3 matrix parsing.

    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none">
    <Description>RGB matrix Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="3 3 3">
4.0029325513196481 0 0
0 4.0029325513196481 0
0 0 4.0029325513196481
        </Array>
    </Matrix>
</ProcessList>
"#,
    );

    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.2">
    <Description>RGB matrix + offset Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="4 4 3">
4.0029325513196481 0 0 0
0 4.0029325513196481 0 0
0 0 4.0029325513196481 0
0 0                  0 0
        </Array>
    </Matrix>
</ProcessList>
"#,
    );

    // Version 1.3 and onward matrix parsing.

    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.3">
    <Description>RGB matrix Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="3 3 3">
4.0029325513196481 0 0
0 4.0029325513196481 0
0 0 4.0029325513196481
        </Array>
    </Matrix>
</ProcessList>
"#,
    );

    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.3">
    <Description>RGBA matrix Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="4 4 4">
4.0029325513196481 0 0 0
0 4.0029325513196481 0 0
0 0 4.0029325513196481 0
0 0 0 4.0029325513196481
        </Array>
    </Matrix>
</ProcessList>
"#,
    );

    check_identity(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="none" version="1.3">
    <Description>RGB matrix + offset Identity, 10i to 12i</Description>
    <Matrix inBitDepth="10i" outBitDepth="12i">
        <Array dim="3 4 3">
4.0029325513196481 0 0 0
0 4.0029325513196481 0 0
0 0 4.0029325513196481 0
        </Array>
    </Matrix>
</ProcessList>
"#,
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_windows_eol)` @ v2.5.2.
#[test]
fn matrix_windows_eol() {
    // This file uses windows end of line character and does not start with the ?xml header.
    let ctf_file = "clf/matrix_windows.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();
    assert_eq!(cached_file.transform.get_id(), b"42");
    assert_eq!(op_list.len(), 1);
    assert_eq!(op_list[0].get_type(), OpDataType::Matrix);
    assert_eq!(op_list[0].get_id(), b"");
    assert_eq!(op_list[0].get_name(), b"identity matrix");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_no_newlines)` @ v2.5.2.
#[test]
fn matrix_no_newlines() {
    let ctf_file = "clf/matrix_no_newlines.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    assert_eq!(op_list[0].get_type(), OpDataType::Matrix);
    let p_matrix = as_matrix(&op_list[0]);
    let array = p_matrix.get_array();

    // `array.getValues()[i] * scale` is a double times a float, `3.6f` and `1e-6f` promoted.
    let scale = f64::from(4095.0f32 / 1023.0f32);
    let close = |i: usize, expected: f32| {
        check_close(
            array.get_values()[i] * scale,
            f64::from(expected),
            f64::from(1e-6f32),
        );
    };
    close(0, 3.6);
    close(1, 0.1);
    close(2, -0.2);
    close(3, 0.0);
    close(4, 0.2);
    close(5, 3.5);
    close(6, 0.1);
    close(7, 0.0);
    close(8, 0.1);
    close(9, -0.3);
    close(10, 3.4);
    close(11, 0.0);

    let offsets = p_matrix.get_offsets();
    let oscale = f64::from(4095.0f32);
    let close_offset = |i: usize, expected: f32| {
        check_close(offsets[i] * oscale, f64::from(expected), f64::from(1e-6f32));
    };
    close_offset(0, 0.3);
    close_offset(1, -0.05);
    close_offset(2, -0.4);
    close_offset(3, 0.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, check_utf8)` @ v2.5.2.
#[test]
fn check_utf8() {
    let ctf_file = "clf/matrix_example_utf8.clf";

    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let mut desc_list = Vec::new();
    get_elements_values(
        op_list[0].get_format_metadata().get_children_elements(),
        TAG_DESCRIPTION,
        &mut desc_list,
    );
    assert_eq!(desc_list.len(), 1);
    let desc = &desc_list[0];
    let utf8_test: &[u8] = b"\xE6\xA8\x99\xE6\xBA\x96\xE8\x90\xAC\xE5\x9C\x8B\xE7\xA2\xBC";
    assert_eq!(desc.as_slice(), utf8_test);
    let utf8_test_wrong: &[u8] = b"\xE5\xA8\x99\xE6\xBA\x96\xE8\x90\xAC\xE5\x9C\x8B\xE7\xA2\xBC";
    assert_ne!(desc.as_slice(), utf8_test_wrong);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, info_example)` @ v2.5.2.
#[test]
fn info_example() {
    let ctf_file = "clf/info_example.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let transform = &cached_file.transform;
    assert_eq!(transform.get_descriptions().len(), 2);
    assert_eq!(
        transform.get_descriptions()[0],
        b"Example of using the Info element"
    );
    assert_eq!(transform.get_descriptions()[1], b"A second description");
    assert_eq!(transform.get_input_descriptor(), b"input desc");
    assert_eq!(transform.get_output_descriptor(), b"output desc");

    // Ensure ops were not affected by metadata parsing.
    let op_list = transform.get_ops();
    assert_eq!(op_list.len(), 1);

    let p_matrix = as_matrix(&op_list[0]);
    assert_eq!(p_matrix.get_name(), b"identity");

    assert_eq!(p_matrix.get_file_input_bit_depth(), BitDepth::F32);
    assert_eq!(p_matrix.get_file_output_bit_depth(), BitDepth::Uint12);

    let info = transform.get_info_metadata();

    // Check element values.
    assert_eq!(info.get_element_name(), METADATA_INFO);
    let items = info.get_children_elements();
    assert_eq!(items.len(), 6);
    assert_eq!(items[0].get_element_name(), b"Copyright");
    assert_eq!(
        items[0].get_element_value(),
        b"Copyright Contributors to the OpenColorIO Project."
    );
    assert_eq!(items[1].get_element_name(), b"AppRelease");
    assert_eq!(items[1].get_element_value(), b"2020.0.63");
    assert_eq!(items[2].get_element_name(), b"Revision");
    assert_eq!(items[2].get_element_value(), b"1");

    assert_eq!(items[3].get_element_name(), b"Category");
    assert_eq!(items[3].get_element_value(), b"");
    let cat_items = items[3].get_children_elements();
    assert_eq!(cat_items.len(), 1);
    assert_eq!(cat_items[0].get_element_name(), b"Tags");
    let tags_items = cat_items[0].get_children_elements();
    assert_eq!(tags_items.len(), 2);
    assert_eq!(tags_items[0].get_element_name(), b"SceneLinearWorkingSpace");
    assert_eq!(tags_items[0].get_element_value(), b"");
    assert_eq!(tags_items[1].get_element_name(), b"Input");
    assert_eq!(tags_items[1].get_element_value(), b"");

    assert_eq!(items[4].get_element_name(), b"InputColorSpace");
    assert_eq!(items[4].get_element_value(), b"");
    let ic_items = items[4].get_children_elements();
    assert_eq!(ic_items.len(), 4);
    assert_eq!(ic_items[0].get_element_name(), METADATA_DESCRIPTION);
    assert_eq!(
        ic_items[0].get_element_value(),
        b"Input color space description"
    );
    assert_eq!(ic_items[1].get_element_name(), b"ImageState");
    assert_eq!(ic_items[1].get_element_value(), b"video");
    assert_eq!(ic_items[2].get_element_name(), b"ShortName");
    assert_eq!(ic_items[2].get_element_value(), b"no_version");
    assert_eq!(ic_items[3].get_element_name(), b"ID");
    assert_eq!(
        ic_items[3].get_element_value(),
        b"387b23d1-f1ce-3f69-8544-e5601f45f78b"
    );

    assert_eq!(items[5].get_element_name(), b"OutputColorSpace");
    assert_eq!(items[5].get_element_value(), b"");
    let oc_items = items[5].get_children_elements();
    assert_eq!(oc_items.len(), 3);
    let attribs = items[5].get_attributes();
    assert_eq!(attribs.len(), 2);
    assert_eq!(attribs[0].0, b"att1");
    assert_eq!(attribs[0].1, b"test1");
    assert_eq!(attribs[1].0, b"att2");
    assert_eq!(attribs[1].1, b"test2");
    assert_eq!(oc_items[0].get_element_name(), b"ImageState");
    assert_eq!(oc_items[0].get_element_value(), b"scene");
    assert_eq!(oc_items[1].get_element_name(), b"ShortName");
    assert_eq!(oc_items[1].get_element_value(), b"ACES");
    assert_eq!(oc_items[2].get_element_name(), b"ID");
    assert_eq!(oc_items[2].get_element_value(), b"1");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, process_list_valid_version)` @ v2.5.2.
#[test]
fn process_list_valid_version() {
    let ctf_file = "process_list_valid_version.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    assert_eq!(ctf_version, CTF_PROCESS_LIST_VERSION_1_4);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, process_list_version_revision)` @ v2.5.2.
#[test]
fn process_list_version_revision() {
    let ctf_file = "process_list_version_revision.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    let ver = CtfVersion::new(1, 3, 10);
    assert_eq!(ctf_version, ver);
    assert!(CTF_PROCESS_LIST_VERSION_1_3 < ctf_version);
    assert!(ctf_version < CTF_PROCESS_LIST_VERSION_1_4);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, process_list_no_version)` @ v2.5.2.
#[test]
fn process_list_no_version() {
    let ctf_file = "process_list_no_version.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let ctf_version = cached_file.transform.get_ctf_version();
    assert_eq!(ctf_version, CTF_PROCESS_LIST_VERSION_1_2);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, info_element_version_test)` @ v2.5.2.
#[test]
fn info_element_version_test() {
    // VALID - No Version.
    {
        let ctf_file = "info_version_without.ctf";
        load_clf_file(ctf_file).unwrap();
    }
    // VALID - Minor Version.
    {
        let ctf_file = "info_version_valid_minor.ctf";
        load_clf_file(ctf_file).unwrap();
    }
    // INVALID - Invalid Version.
    {
        let ctf_file = "info_version_invalid.ctf";
        check_throw_what(
            load_clf_file(ctf_file),
            "Invalid Info element version attribute",
        );
    }
    // INVALID - Unsupported Version.
    {
        let ctf_file = "info_version_unsupported.ctf";
        check_throw_what(
            load_clf_file(ctf_file),
            "Unsupported Info element version attribute",
        );
    }
    // INVALID - Empty Version.
    {
        let ctf_file = "info_version_empty.ctf";
        check_throw_what(
            load_clf_file(ctf_file),
            "Invalid Info element version attribute",
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_element_end_missing)` @ v2.5.2.
#[test]
fn transform_element_end_missing() {
    let ctf_file = "clf/illegal/transform_element_end_missing.clf";
    check_throw_what(load_clf_file(ctf_file), "no element found");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, array_missing_values)` @ v2.5.2.
#[test]
fn array_missing_values() {
    let ctf_file = "clf/illegal/array_missing_values.clf";
    check_throw_what(load_clf_file(ctf_file), "Expected 3x3 Array values");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, array_bad_value)` @ v2.5.2.
#[test]
fn array_bad_value() {
    let ctf_file = "clf/illegal/array_bad_value.clf";
    check_throw_what(load_clf_file(ctf_file), "Illegal values");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, array_bad_dimension)` @ v2.5.2.
#[test]
fn array_bad_dimension() {
    let ctf_file = "clf/illegal/array_bad_dimension.clf";
    check_throw_what(load_clf_file(ctf_file), "Illegal array dimensions");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, array_too_many_values)` @ v2.5.2.
#[test]
fn array_too_many_values() {
    let ctf_file = "clf/illegal/array_too_many_values.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Expected 3x3 Array, found too many values",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, matrix_end_missing)` @ v2.5.2.
#[test]
fn matrix_end_missing() {
    let ctf_file = "clf/illegal/matrix_end_missing.clf";
    check_throw_what(load_clf_file(ctf_file), "no closing tag for 'Matrix'");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_bad_outdepth)` @ v2.5.2.
#[test]
fn transform_bad_outdepth() {
    let ctf_file = "clf/illegal/transform_bad_outdepth.clf";
    check_throw_what(load_clf_file(ctf_file), "outBitDepth unknown value");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_end_missing)` @ v2.5.2.
#[test]
fn transform_end_missing() {
    let ctf_file = "clf/illegal/transform_element_end_missing.clf";
    check_throw_what(load_clf_file(ctf_file), "no element found");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_corrupted_tag)` @ v2.5.2.
#[test]
fn transform_corrupted_tag() {
    let ctf_file = "clf/illegal/transform_corrupted_tag.clf";
    check_throw_what(load_clf_file(ctf_file), "no closing tag");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_default)` @ v2.5.2.
#[test]
fn range_default() {
    // If style is not present, it defaults to clamp.
    let ctf_file = "clf/range.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_r = as_range(&op_list[0]);

    assert_eq!(p_r.get_file_input_bit_depth(), BitDepth::Uint16);
    assert_eq!(p_r.get_file_output_bit_depth(), BitDepth::Uint16);
    // NB: All exactly representable as float.
    assert_eq!(p_r.get_min_in_value(), 16320. / 65535.);
    assert_eq!(p_r.get_max_in_value(), 32640. / 65535.);
    assert_eq!(p_r.get_min_out_value(), 16320. / 65535.);
    assert_eq!(p_r.get_max_out_value(), 32640. / 65535.);

    assert!(!p_r.min_is_empty());
    assert!(!p_r.max_is_empty());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_test1_clamp)` @ v2.5.2.
#[test]
fn range_test1_clamp() {
    // Style == clamp.
    let ctf_file = "clf/range_test1_clamp.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_r = as_range(&op_list[0]);

    assert_eq!(p_r.get_file_input_bit_depth(), BitDepth::Uint8);
    assert_eq!(p_r.get_file_output_bit_depth(), BitDepth::F32);
    // NB: All exactly representable as float.
    assert_eq!(p_r.get_min_in_value(), 16. / 255.);
    assert_eq!(p_r.get_max_in_value(), 240. / 255.);
    assert_eq!(p_r.get_min_out_value(), -0.5);
    assert_eq!(p_r.get_max_out_value(), 2.);

    assert!(!p_r.min_is_empty());
    assert!(!p_r.max_is_empty());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_test1_noclamp)` @ v2.5.2.
#[test]
fn range_test1_noclamp() {
    // Style == noClamp.
    let ctf_file = "clf/range_test1_noclamp.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);

    // Check that the noClamp style Range became a Matrix.
    let mat_op_data = as_matrix(&op_list[0]);
    assert_eq!(mat_op_data.get_file_input_bit_depth(), BitDepth::Uint8);
    assert_eq!(mat_op_data.get_file_output_bit_depth(), BitDepth::F32);

    let out_scale = get_bit_depth_max_value(BitDepth::F32).unwrap();
    let mat_scale = out_scale / get_bit_depth_max_value(BitDepth::Uint8).unwrap();
    let array = mat_op_data.get_array();
    assert_eq!(array.get_length(), 4);
    assert_eq!(array.get_num_color_components(), 4);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * array.get_length()
    );

    let scalef: f32 = (1.05f32 + 0.05f32) / (272.0f32 + 16.0f32);
    let offsetf: f32 = -0.05f32 - scalef * -16.0f32;
    let prec: f32 = 10000.0;
    let scale = (prec * scalef) as i32;
    let offset = (prec * offsetf) as i32;

    assert!(mat_op_data.is_diagonal());

    // Check values on the diagonal.
    assert_eq!(array.get_values().len() as c_ulong, array.get_num_values());
    let prec = f64::from(prec);
    assert_eq!((prec * array.get_values()[0] * mat_scale) as i32, scale);
    assert_eq!((prec * array.get_values()[5] * mat_scale) as i32, scale);
    assert_eq!((prec * array.get_values()[10] * mat_scale) as i32, scale);
    assert_eq!(array.get_values()[15], 1.0);

    // Check the offsets.
    let offsets = mat_op_data.get_offsets();
    assert_eq!((prec * offsets[0] * out_scale) as i32, offset);
    assert_eq!((prec * offsets[1] * out_scale) as i32, offset);
    assert_eq!((prec * offsets[2] * out_scale) as i32, offset);
    assert_eq!(offsets[3], f64::from(0.0f32));
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_test2)` @ v2.5.2.
#[test]
fn range_test2() {
    // Style == clamp.
    let ctf_file = "clf/range_test2.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_r = as_range(&op_list[0]);

    assert_eq!(p_r.get_file_input_bit_depth(), BitDepth::F32);
    assert_eq!(p_r.get_file_output_bit_depth(), BitDepth::F16);
    assert_eq!(p_r.get_min_in_value(), 0.1);
    assert_eq!(p_r.get_min_out_value(), 0.1);
    assert!(p_r.max_is_empty());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_nonmatching_clamp)` @ v2.5.2.
#[test]
fn range_nonmatching_clamp() {
    let ctf_file = "clf/illegal/range_nonmatching_clamp.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "In and out minimum limits must be equal",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_empty)` @ v2.5.2.
#[test]
fn range_empty() {
    let ctf_file = "clf/illegal/range_empty.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "At least minimum or maximum limits must be set",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_bad_noclamp)` @ v2.5.2.
#[test]
fn range_bad_noclamp() {
    let ctf_file = "clf/illegal/range_bad_noclamp.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Non-clamping Range min & max values have to be set",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, range_bad_values)` @ v2.5.2.
#[test]
fn range_bad_values() {
    let ctf_file = "clf/illegal/range_bad_values.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Range maxInValue is too close to minInValue",
    );
}

/// The gamma of an op data, which must be one (`dynamic_pointer_cast<const GammaOpData>`).
fn as_gamma(op: &OpData) -> &GammaOpData {
    match op {
        OpData::Gamma(gamma) => gamma,
        _ => panic!("not a gamma: {:?}", op.get_type()),
    }
}

/// The log of an op data, which must be one (`dynamic_pointer_cast<const LogOpData>`).
fn as_log(op: &OpData) -> &LogOpData {
    match op {
        OpData::Log(log) => log,
        _ => panic!("not a log: {:?}", op.get_type()),
    }
}

/// The CDL of an op data, which must be one (`dynamic_pointer_cast<const CDLOpData>`).
fn as_cdl(op: &OpData) -> &CdlOpData {
    match op {
        OpData::Cdl(cdl) => cdl,
        _ => panic!("not a CDL: {:?}", op.get_type()),
    }
}

/// The op's descriptions (`GetElementsValues` of its metadata's children).
fn descriptions(op: &OpData) -> Vec<Vec<u8>> {
    let mut desc = Vec::new();
    get_elements_values(
        op.get_format_metadata().get_children_elements(),
        TAG_DESCRIPTION,
        &mut desc,
    );
    desc
}

/// Port of `ParseString` (FileFormatCTF_tests.cpp:996-1007 @ v2.5.2).
fn parse_string(str: &str) -> Result<LocalCachedFile> {
    let mut ctf = InputStream::from_bytes(str.as_bytes());

    // Parse stream.
    LocalFileFormat.read_file(&mut ctf, b"")
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_test1)` @ v2.5.2.
#[test]
fn gamma_test1() {
    let ctf_file = "gamma_test1.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    assert_eq!(cached_file.transform.get_id(), b"id");

    assert_eq!(cached_file.transform.get_descriptions().len(), 1);
    assert_eq!(cached_file.transform.get_descriptions()[0], b"2.4 gamma");

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::BasicFwd);

    let params = vec![2.4];

    assert!(p_g.red_params() == &params);
    assert!(p_g.green_params() == &params);
    assert!(p_g.blue_params() == &params);
    // Version of the ctf is less than 1.5, so alpha must be identity.
    assert!(GammaOpData::is_identity_parameters(
        p_g.alpha_params(),
        p_g.style()
    ));

    assert!(!p_g.are_all_components_equal());
    assert!(p_g.is_non_channel_dependent()); // RGB are equal, A is an identity
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_test2)` @ v2.5.2.
#[test]
fn gamma_test2() {
    let ctf_file = "gamma_test2.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::BasicRev);
    let params_r = vec![2.4];
    let params_g = vec![2.35];
    let params_b = vec![2.2];

    assert!(p_g.red_params() == &params_r);
    assert!(p_g.green_params() == &params_g);
    assert!(p_g.blue_params() == &params_b);
    assert!(GammaOpData::is_identity_parameters(
        p_g.alpha_params(),
        p_g.style()
    ));

    assert!(!p_g.are_all_components_equal());
    assert!(!p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_test3)` @ v2.5.2.
#[test]
fn gamma_test3() {
    let ctf_file = "gamma_test3.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::MoncurveFwd);
    let params = vec![1. / 0.45, 0.099];

    // This is a precision test to ensure we can recreate a double that is exactly equal to
    // 1/0.45, which is required to implement rec 709 exactly.
    assert!(p_g.red_params() == &params);
    assert!(p_g.green_params() == &params);
    assert!(p_g.blue_params() == &params);
    assert!(GammaOpData::is_identity_parameters(
        p_g.alpha_params(),
        p_g.style()
    ));

    assert!(!p_g.are_all_components_equal());
    assert!(p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_test4)` @ v2.5.2.
#[test]
fn gamma_test4() {
    let ctf_file = "gamma_test4.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::MoncurveRev);
    let params_r = vec![2.2, 0.001];
    let params_g = vec![2.4, 0.01];
    let params_b = vec![2.6, 0.1];

    assert!(p_g.red_params() == &params_r);
    assert!(p_g.green_params() == &params_g);
    assert!(p_g.blue_params() == &params_b);
    assert!(GammaOpData::is_identity_parameters(
        p_g.alpha_params(),
        p_g.style()
    ));

    assert!(!p_g.are_all_components_equal());
    assert!(!p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_test5)` @ v2.5.2.
#[test]
fn gamma_test5() {
    // This test is for an old (< 1.5) transform file that contains an invalid GammaParams for
    // the A channel.
    let ctf_file = "gamma_test5.ctf";
    check_throw_what(load_clf_file(ctf_file), "Invalid channel");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_test6)` @ v2.5.2.
#[test]
fn gamma_test6() {
    // This test is for an old (< 1.5) transform file that contains a single GammaParams with
    // identity values:
    // - R, G and B set to identity parameters (identity test).
    // - A set to identity.
    let ctf_file = "gamma_test6.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::MoncurveFwd);
    assert!(p_g.are_all_components_equal());
    assert!(p_g.is_non_channel_dependent());
    assert!(p_g.is_identity().unwrap());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test1)` @ v2.5.2.
#[test]
fn gamma_alpha_test1() {
    // This test is for a new (>= 1.5) transform file that contains a single GammaParams:
    // - R, G and B set to same parameters.
    // - A set to identity.
    let ctf_file = "gamma_alpha_test1.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::BasicFwd);

    let params = vec![2.4];

    assert!(p_g.red_params() == &params);
    assert!(p_g.green_params() == &params);
    assert!(p_g.blue_params() == &params);
    assert!(GammaOpData::is_identity_parameters(
        p_g.alpha_params(),
        p_g.style()
    ));

    assert!(!p_g.are_all_components_equal());
    assert!(p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test2)` @ v2.5.2.
#[test]
fn gamma_alpha_test2() {
    // This test is for a new (>= 1.5) transform file that contains a different GammaParams
    // for every channel:
    // - R, G, B and A set to different parameters.
    let ctf_file = "gamma_alpha_test2.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::BasicRev);

    let params_r = vec![2.4];
    let params_g = vec![2.35];
    let params_b = vec![2.2];
    let params_a = vec![2.5];

    assert!(p_g.red_params() == &params_r);
    assert!(p_g.green_params() == &params_g);
    assert!(p_g.blue_params() == &params_b);
    assert!(p_g.alpha_params() == &params_a);

    assert!(!p_g.are_all_components_equal());
    assert!(!p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test3)` @ v2.5.2.
#[test]
fn gamma_alpha_test3() {
    // This test is for a new (>= 1.5) transform file that contains a single GammaParams:
    // - R, G and B set to same parameters (precision test).
    // - A set to identity.
    let ctf_file = "gamma_alpha_test3.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::MoncurveFwd);

    let params = vec![1. / 0.45, 0.099];

    assert!(p_g.red_params() == &params);
    assert!(p_g.green_params() == &params);
    assert!(p_g.blue_params() == &params);
    assert!(GammaOpData::is_identity_parameters(
        p_g.alpha_params(),
        p_g.style()
    ));

    assert!(!p_g.are_all_components_equal());
    assert!(p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test4)` @ v2.5.2.
#[test]
fn gamma_alpha_test4() {
    // This test is for a new (>= 1.5) transform file that contains a different GammaParams
    // for every channel:
    // - R, G, B and A set to different parameters (attributes order test).
    let ctf_file = "gamma_alpha_test4.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::MoncurveRev);

    let params_r = vec![2.2, 0.001];
    let params_g = vec![2.4, 0.01];
    let params_b = vec![2.6, 0.1];
    let params_a = vec![2.0, 0.0001];

    assert!(p_g.red_params() == &params_r);
    assert!(p_g.green_params() == &params_g);
    assert!(p_g.blue_params() == &params_b);
    assert!(p_g.alpha_params() == &params_a);

    assert!(!p_g.are_all_components_equal());
    assert!(!p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test5)` @ v2.5.2.
#[test]
fn gamma_alpha_test5() {
    // This test is for a new (>= 1.5) transform file that contains a GammaParams with no
    // channel specified:
    // - R, G and B set to same parameters.
    // and a GammaParams for the A channel:
    // - A set to different parameters.
    let ctf_file = "gamma_alpha_test5.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_g = as_gamma(&op_list[0]);

    assert_eq!(p_g.style(), GammaStyle::MoncurveFwd);

    let params = vec![1. / 0.45, 0.099];
    let params_a = vec![1.7, 0.33];

    assert!(p_g.red_params() == &params);
    assert!(p_g.green_params() == &params);
    assert!(p_g.blue_params() == &params);
    assert!(p_g.alpha_params() == &params_a);

    assert!(!p_g.are_all_components_equal());
    assert!(!p_g.is_non_channel_dependent());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, gamma_alpha_test6)` @ v2.5.2.
#[test]
fn gamma_alpha_test6() {
    // This test is for an new (>= 1.5) transform file that contains an invalid GammaParams
    // for the A channel (missing offset attribute).
    let ctf_file = "gamma_alpha_test6.ctf";
    check_throw_what(load_clf_file(ctf_file), "Missing required offset parameter");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exponent_bad_value)` @ v2.5.2.
#[test]
fn exponent_bad_value() {
    // The moncurve style requires a gamma value >= 1.
    let ctf_file = "clf/illegal/exponent_bad_value.clf";
    check_throw_what(load_clf_file(ctf_file), "is less than lower bound");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exponent_bad_param)` @ v2.5.2.
#[test]
fn exponent_bad_param() {
    // The basic style cannot use offset.
    let ctf_file = "clf/illegal/exponent_bad_param.clf";
    check_throw_what(load_clf_file(ctf_file), "Illegal offset parameter");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exponent_all_styles)` @ v2.5.2.
#[test]
fn exponent_all_styles() {
    // Note: This is somewhat repetitive of the CTF Gamma tests above, but it is worth having
    // both due to changes in the format over time (e.g. moncurveFwd->monCurveFwd, and
    // gamma->exponent), and the fact that CLF and early CTF does not support alpha.
    let file_name = "clf/exponent_all_styles.clf";
    let cached_file = load_clf_file(file_name).unwrap();
    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 12);

    {
        // Op 0 == basicFwd.
        let op_data = as_gamma(&op_list[0]);
        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 1);
        assert_eq!(
            desc[0],
            b"If there is only one Params, use it for R, G, and B."
        );
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert_eq!(op_data.style(), GammaStyle::BasicFwd);
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
        assert!(op_data.is_alpha_component_identity());
        let params = vec![2.4];
        assert!(op_data.red_params() == &params);
    }
    {
        // Op 1 == basicRev.
        let op_data = as_gamma(&op_list[1]);
        assert_eq!(op_list[1].get_id(), b"a1");
        assert_eq!(op_list[1].get_name(), b"gamma");
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert_eq!(op_data.style(), GammaStyle::BasicRev);
        assert!(!op_data.is_non_channel_dependent());
        assert!(op_data.is_alpha_component_identity());
        let params_r = vec![2.4];
        let params_g = vec![2.35];
        let params_b = vec![2.2];
        assert!(op_data.red_params() == &params_r);
        assert!(op_data.green_params() == &params_g);
        assert!(op_data.blue_params() == &params_b);
    }
    {
        // Op 2 == monCurveFwd.
        let op_data = as_gamma(&op_list[2]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert_eq!(op_data.style(), GammaStyle::MoncurveFwd);
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
        assert!(op_data.is_alpha_component_identity());
        let params = vec![1. / 0.45, 0.099];
        assert!(op_data.red_params() == &params);
    }
    {
        // Op 3 == monCurveRev.
        let op_data = as_gamma(&op_list[3]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert_eq!(op_data.style(), GammaStyle::MoncurveRev);
        assert!(!op_data.is_non_channel_dependent());
        assert!(op_data.is_alpha_component_identity());
        let params_r = vec![2.2, 0.001];
        let params_g = vec![2.4, 0.01];
        let params_b = vec![2.6, 0.1];
        assert!(op_data.red_params() == &params_r);
        assert!(op_data.green_params() == &params_g);
        assert!(op_data.blue_params() == &params_b);
    }
    {
        // Op 4 == monCurveFwd.
        let op_data = as_gamma(&op_list[4]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert_eq!(op_data.style(), GammaStyle::MoncurveFwd);
        assert!(op_data.are_all_components_equal());
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
        assert!(op_data.is_alpha_component_identity());
        assert!(GammaOpData::is_identity_parameters(
            op_data.red_params(),
            op_data.style()
        ));
    }
    {
        // Op 5 == basicMirrorFwd.
        let op_data = as_gamma(&op_list[5]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert_eq!(op_data.style(), GammaStyle::BasicMirrorFwd);
        assert!(!op_data.are_all_components_equal());
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
        assert!(op_data.is_alpha_component_identity());
    }
    {
        // Op 6 == basicMirrorRev.
        let op_data = as_gamma(&op_list[6]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert_eq!(op_data.style(), GammaStyle::BasicMirrorRev);
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
    }
    {
        // Op 7 == basicPassThruFwd.
        let op_data = as_gamma(&op_list[7]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert_eq!(op_data.style(), GammaStyle::BasicPassThruFwd);
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
    }
    {
        // Op 8 == basicPassThruRev.
        let op_data = as_gamma(&op_list[8]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert_eq!(op_data.style(), GammaStyle::BasicPassThruRev);
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
    }
    {
        // Op 9 == monCurveMirrorFwd.
        let op_data = as_gamma(&op_list[9]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert_eq!(op_data.style(), GammaStyle::MoncurveMirrorFwd);
        assert!(op_data.is_non_channel_dependent()); // RGB are equal, A is an identity
    }
    {
        // Op 10 == monCurveMirrorRev.
        let op_data = as_gamma(&op_list[10]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert_eq!(op_data.style(), GammaStyle::MoncurveMirrorRev);
        assert!(!op_data.is_non_channel_dependent());
        let params_r = vec![3.0, 0.16];
        assert!(op_data.red_params() == &params_r);
        assert!(GammaOpData::is_identity_parameters(
            op_data.green_params(),
            op_data.style()
        ));
        assert!(GammaOpData::is_identity_parameters(
            op_data.blue_params(),
            op_data.style()
        ));
    }
    {
        // Op 11 == Range.
        as_range(&op_list[11]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, clf2_exponent_parse)` @ v2.5.2.
#[test]
fn clf2_exponent_parse() {
    let gamma_clf2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="2" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <ExponentParams gamma="2.6" />
    </Exponent>
</ProcessList>
"#;

    check_throw_what(
        parse_string(gamma_clf2),
        "CLF file version '2' does not support operator 'Exponent'",
    );

    let gamma_clf_alpha = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <ExponentParams gamma="2.6" />
        <ExponentParams channel="A" gamma="1.7" offset="0.33" />
    </Exponent>
</ProcessList>
"#;

    check_throw_what(parse_string(gamma_clf_alpha), "Invalid channel: A");

    let gamma_ctf_mirror_1_7 = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.7" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicMirrorRev">
        <ExponentParams gamma="2.6" />
    </Exponent>
</ProcessList>
"#;

    check_throw_what(
        parse_string(gamma_ctf_mirror_1_7),
        "Style not handled: 'basicMirrorRev'",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_clamp_fwd)` @ v2.5.2.
#[test]
fn cdl_clamp_fwd() {
    let ctf_file = "clf/cdl_clamp_fwd.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(cached_file.transform.get_input_descriptor(), b"inputDesc");
    assert_eq!(cached_file.transform.get_output_descriptor(), b"outputDesc");
    assert_eq!(op_list.len(), 1);
    let p_cdl = as_cdl(&op_list[0]);

    assert_eq!(p_cdl.get_id(), b"look 1");
    assert_eq!(p_cdl.get_name(), b"cdl");

    let descriptions = descriptions(&op_list[0]);

    assert_eq!(descriptions.len(), 1);
    assert_eq!(descriptions[0], b"ASC CDL operation");

    assert_eq!(p_cdl.get_style(), CdlOpStyle::V1_2Fwd);
    let style_name = CdlOpData::get_style_name(p_cdl.get_style());
    assert_eq!(style_name, "Fwd");

    assert!(*p_cdl.get_slope_params() == ChannelParams::new(1.35, 1.1, 0.71));
    assert!(*p_cdl.get_offset_params() == ChannelParams::new(0.05, -0.23, 0.11));
    assert!(*p_cdl.get_power_params() == ChannelParams::new(0.93, 0.81, 1.27));
    assert_eq!(p_cdl.get_saturation(), 1.239);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_style)` @ v2.5.2.
#[test]
fn cdl_missing_style() {
    let ctf_file = "clf/cdl_missing_style.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_cdl = as_cdl(&op_list[0]);

    // Note: Default for CLF is different from OCIO default.
    assert_eq!(p_cdl.get_style(), CdlOpStyle::V1_2Fwd);

    assert!(*p_cdl.get_slope_params() == ChannelParams::new(1.35, 1.1, 0.71));
    assert!(*p_cdl.get_offset_params() == ChannelParams::new(0.05, -0.23, 0.11));
    assert!(*p_cdl.get_power_params() == ChannelParams::new(0.93, 0.81, 1.27));
    assert_eq!(p_cdl.get_saturation(), 1.239);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_all_styles)` @ v2.5.2.
#[test]
fn cdl_all_styles() {
    let ctf_file = "clf/cdl_all_styles.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 4);

    assert_eq!(as_cdl(&op_list[0]).get_style(), CdlOpStyle::V1_2Fwd);
    assert_eq!(as_cdl(&op_list[1]).get_style(), CdlOpStyle::V1_2Rev);
    assert_eq!(as_cdl(&op_list[2]).get_style(), CdlOpStyle::NoClampFwd);
    assert_eq!(as_cdl(&op_list[3]).get_style(), CdlOpStyle::NoClampRev);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_bad_slope)` @ v2.5.2.
#[test]
fn cdl_bad_slope() {
    let ctf_file = "clf/illegal/cdl_bad_slope.clf";
    check_throw_what(load_clf_file(ctf_file), "SOPNode: 3 values required");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_bad_sat)` @ v2.5.2.
#[test]
fn cdl_bad_sat() {
    let ctf_file = "clf/illegal/cdl_bad_sat.clf";
    check_throw_what(load_clf_file(ctf_file), "SatNode: non-single value");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_bad_power)` @ v2.5.2.
#[test]
fn cdl_bad_power() {
    let ctf_file = "clf/illegal/cdl_bad_power.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "CDLOpData: Invalid 'power' 0 should be greater than 0.",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_slope)` @ v2.5.2.
#[test]
fn cdl_missing_slope() {
    let ctf_file = "clf/illegal/cdl_missing_slope.clf";
    check_throw_what(load_clf_file(ctf_file), "Required node 'Slope' is missing");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_offset)` @ v2.5.2.
#[test]
fn cdl_missing_offset() {
    let ctf_file = "clf/illegal/cdl_missing_offset.clf";
    check_throw_what(load_clf_file(ctf_file), "Required node 'Offset' is missing");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_power)` @ v2.5.2.
#[test]
fn cdl_missing_power() {
    let ctf_file = "clf/illegal/cdl_missing_power.clf";
    check_throw_what(load_clf_file(ctf_file), "Required node 'Power' is missing");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_bad_style)` @ v2.5.2.
#[test]
fn cdl_bad_style() {
    let ctf_file = "clf/illegal/cdl_bad_style.clf";
    check_throw_what(load_clf_file(ctf_file), "Unknown style for CDL");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_sop)` @ v2.5.2.
#[test]
fn cdl_missing_sop() {
    let ctf_file = "clf/cdl_missing_sop.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_cdl = as_cdl(&op_list[0]);

    assert!(*p_cdl.get_slope_params() == ChannelParams::splat(1.0));
    assert!(*p_cdl.get_offset_params() == ChannelParams::splat(0.0));
    assert!(*p_cdl.get_power_params() == ChannelParams::splat(1.0));
    assert_eq!(p_cdl.get_saturation(), 1.239);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_missing_sat)` @ v2.5.2.
#[test]
fn cdl_missing_sat() {
    let ctf_file = "clf/cdl_missing_sat.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_cdl = as_cdl(&op_list[0]);

    assert!(*p_cdl.get_slope_params() == ChannelParams::new(1.35, 1.1, 0.71));
    assert!(*p_cdl.get_offset_params() == ChannelParams::new(0.05, -0.23, 0.11));
    assert!(*p_cdl.get_power_params() == ChannelParams::new(0.93, 0.81, 1.27));
    assert_eq!(p_cdl.get_saturation(), 1.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, cdl_various_in_ctf)` @ v2.5.2.
#[test]
fn cdl_various_in_ctf() {
    // When CDL was added to the CLF spec in v2, the style names were changed. Test that both
    // the new and old style names work.
    let ctf_file = "cdl_various.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 8);

    let styles = [
        CdlOpStyle::V1_2Fwd,
        CdlOpStyle::V1_2Fwd,
        CdlOpStyle::V1_2Rev,
        CdlOpStyle::V1_2Rev,
        CdlOpStyle::NoClampFwd,
        CdlOpStyle::NoClampFwd,
        CdlOpStyle::NoClampRev,
        CdlOpStyle::NoClampRev,
    ];
    for (op, style) in op_list.iter().zip(styles) {
        assert_eq!(as_cdl(op).get_style(), style);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_all_styles)` @ v2.5.2.
#[test]
fn log_all_styles() {
    let file_name = "clf/log_all_styles.clf";
    let cached_file = load_clf_file(file_name).unwrap();
    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 11);
    let error = 1e-9;

    {
        // Op 0 == antiLog2.
        let op_data = as_log(&op_list[0]);
        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 1);
        assert_eq!(desc[0], b"AntiLog2 logarithm operation");
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert!(op_data.is_log2());
    }
    {
        // Op 1 == log2.
        let op_data = as_log(&op_list[1]);
        assert_eq!(op_list[1].get_id(), b"a1");
        assert_eq!(op_list[1].get_name(), b"logarithm");
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert!(op_data.is_log2());
        assert!(!op_data.is_log10());
        assert!(!op_data.is_camera());
    }
    {
        // Op 2 == linToLog.
        let op_data = as_log(&op_list[2]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert!(!op_data.is_log2());
        assert!(!op_data.is_log10());
        assert!(!op_data.is_camera());
        assert!(op_data.all_components_equal());
        let param = op_data.red_params();
        assert_eq!(param.len(), 4);
        check_close(param[LOG_SIDE_SLOPE], 0.29325513196, error);
        check_close(param[LOG_SIDE_OFFSET], 0.66959921799, error);
        check_close(param[LIN_SIDE_SLOPE], 0.98920224838, error);
        check_close(param[LIN_SIDE_OFFSET], 0.01079775162, error);
        assert_eq!(op_data.base(), 10.);
    }
    {
        // Op 3 == antiLog10.
        let op_data = as_log(&op_list[3]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert!(!op_data.is_log2());
        assert!(op_data.is_log10());
    }
    {
        // Op 4 == log10.
        let op_data = as_log(&op_list[4]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert!(!op_data.is_log2());
        assert!(op_data.is_log10());
    }
    {
        // Op 5 == logToLin.
        let op_data = as_log(&op_list[5]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert!(!op_data.is_log2());
        assert!(!op_data.is_log10());
        assert!(!op_data.is_camera());
        assert!(op_data.all_components_equal());
        let param = op_data.red_params();
        assert_eq!(param.len(), 4);
        check_close(param[LOG_SIDE_SLOPE], 0.29325513196, error);
        check_close(param[LOG_SIDE_OFFSET], 0.66959921799, error);
        check_close(param[LIN_SIDE_SLOPE], 0.98920224838, error);
        check_close(param[LIN_SIDE_OFFSET], 0.01079775162, error);
        assert_eq!(op_data.base(), 10.);
    }
    {
        // Op 6 == cameraLinToLog.
        let op_data = as_log(&op_list[6]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert!(!op_data.is_log2());
        assert!(!op_data.is_log10());
        assert!(op_data.is_camera());
        assert!(op_data.all_components_equal());
        let param = op_data.red_params();
        assert_eq!(param.len(), 5);
        check_close(param[LOG_SIDE_SLOPE], 0.05707762557, error);
        check_close(param[LOG_SIDE_OFFSET], 0.55479452050, error);
        check_close(param[LIN_SIDE_SLOPE], 1., error);
        check_close(param[LIN_SIDE_OFFSET], 0., error);
        check_close(param[LIN_SIDE_BREAK], 0.00781250000, error);
        // Default base value is 2.
        assert_eq!(op_data.base(), 2.);
    }
    {
        // Op 7 == cameraLogToLin.
        let op_data = as_log(&op_list[7]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert!(!op_data.is_log2());
        assert!(!op_data.is_log10());
        assert!(op_data.is_camera());
        assert!(op_data.all_components_equal());
        let param = op_data.red_params();
        assert_eq!(param.len(), 5);
        check_close(param[LOG_SIDE_SLOPE], 0.05707762557, error);
        check_close(param[LOG_SIDE_OFFSET], 0.55479452050, error);
        check_close(param[LIN_SIDE_SLOPE], 1., error);
        check_close(param[LIN_SIDE_OFFSET], 0., error);
        check_close(param[LIN_SIDE_BREAK], 0.00781250000, error);
        assert_eq!(op_data.base(), 2.);
    }
    {
        // Op 8 == cameraLogToLin.
        let op_data = as_log(&op_list[8]);
        assert_eq!(op_data.direction(), TransformDirection::Inverse);
        assert!(!op_data.is_log2());
        assert!(!op_data.is_log10());
        assert!(op_data.is_camera());
        assert!(op_data.all_components_equal());
        let param = op_data.red_params();
        assert_eq!(param.len(), 6);
        check_close(param[LOG_SIDE_SLOPE], 0.25562072336, error);
        check_close(param[LOG_SIDE_OFFSET], 0.41055718475, error);
        check_close(param[LIN_SIDE_SLOPE], 5.26315789474, error);
        check_close(param[LIN_SIDE_OFFSET], 0.05263157895, error);
        check_close(param[LIN_SIDE_BREAK], 0.01125000000, error);
        check_close(param[LINEAR_SLOPE], 6.62194371178, error);
        assert_eq!(op_data.base(), 10.);
    }
    {
        // Op 9 == linToLog.
        let op_data = as_log(&op_list[9]);
        assert_eq!(op_data.direction(), TransformDirection::Forward);
        assert!(!op_data.is_log2());
        assert!(!op_data.is_log10());
        assert!(!op_data.is_camera());
        assert!(!op_data.all_components_equal());
        {
            let param = op_data.red_params();
            assert_eq!(param.len(), 4);
            assert_eq!(param[LOG_SIDE_SLOPE], 0.3);
            assert_eq!(param[LOG_SIDE_OFFSET], 0.6);
            assert_eq!(param[LIN_SIDE_SLOPE], 0.9);
            assert_eq!(param[LIN_SIDE_OFFSET], 0.05);
            assert_eq!(op_data.base(), 8.);
        }
        {
            let param = op_data.green_params();
            assert_eq!(param.len(), 4);
            assert_eq!(param[LOG_SIDE_SLOPE], 0.25);
            assert_eq!(param[LOG_SIDE_OFFSET], 0.4);
            assert_eq!(param[LIN_SIDE_SLOPE], 5.0);
            assert_eq!(param[LIN_SIDE_OFFSET], 0.05);
            assert_eq!(op_data.base(), 8.);
        }
        {
            let param = op_data.blue_params();
            assert_eq!(param.len(), 4);
            assert_eq!(param[LOG_SIDE_SLOPE], 0.28);
            assert_eq!(param[LOG_SIDE_OFFSET], 0.5);
            assert_eq!(param[LIN_SIDE_SLOPE], 2.0);
            assert_eq!(param[LIN_SIDE_OFFSET], 0.1);
            assert_eq!(op_data.base(), 8.);
        }
    }
    {
        // Op 10 == Range.
        as_range(&op_list[10]);
    }
}

/// The checks of `log_logtolin` and `log_logtolinv2`.
#[track_caller]
fn check_log_to_lin(file_name: &str) {
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let log = as_log(&file_ops[0]);

    assert_eq!(log.direction(), TransformDirection::Inverse);
    assert!(!log.is_log2());
    assert!(!log.is_log10());
    assert!(log.all_components_equal());
    let param = log.red_params();
    assert_eq!(param.len(), 4);
    let error = 1e-9;
    // This file uses the original CTF/Cineon style params, verify they are converted properly
    // to the new OCIO style params.
    check_close(param[LOG_SIDE_SLOPE], 0.29325513196, error);
    check_close(param[LOG_SIDE_OFFSET], 0.66959921799, error);
    check_close(param[LIN_SIDE_SLOPE], 0.98969709693, error);
    check_close(param[LIN_SIDE_OFFSET], 0.01030290307, error);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_logtolin)` @ v2.5.2.
#[test]
fn log_logtolin() {
    check_log_to_lin("log_logtolin.ctf");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_logtolinv2)` @ v2.5.2.
#[test]
fn log_logtolinv2() {
    // Same as previous test, but CTF version set to 2.
    check_log_to_lin("log_logtolinv2.ctf");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_lintolog_3chan)` @ v2.5.2.
#[test]
fn log_lintolog_3chan() {
    let file_name = "log_lintolog_3chan.ctf";
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let log = as_log(&file_ops[0]);

    assert_eq!(log.direction(), TransformDirection::Forward);
    assert!(!log.all_components_equal());

    let r_param = log.red_params();
    assert_eq!(r_param.len(), 4);
    let error = 1e-9;
    // This file uses the original CTF/Cineon style params, verify they are converted properly
    // to the new OCIO style params.
    check_close(r_param[LOG_SIDE_SLOPE], 0.244379276637, error);
    check_close(r_param[LOG_SIDE_OFFSET], 0.665689149560, error);
    check_close(r_param[LIN_SIDE_SLOPE], 1.111637101285, error);
    check_close(r_param[LIN_SIDE_OFFSET], -0.000473391157, error);

    let g_param = log.green_params();
    assert_eq!(g_param.len(), 4);
    check_close(g_param[LOG_SIDE_SLOPE], 0.293255131964, error);
    check_close(g_param[LOG_SIDE_OFFSET], 0.666666666667, error);
    check_close(g_param[LIN_SIDE_SLOPE], 0.991514003046, error);
    check_close(g_param[LIN_SIDE_OFFSET], 0.008485996954, error);

    let b_param = log.blue_params();
    assert_eq!(b_param.len(), 4);
    check_close(b_param[LOG_SIDE_SLOPE], 0.317693059628, error);
    check_close(b_param[LOG_SIDE_OFFSET], 0.667644183773, error);
    check_close(b_param[LIN_SIDE_SLOPE], 1.236287104632, error);
    check_close(b_param[LIN_SIDE_OFFSET], 0.010970316295, error);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_bad_style)` @ v2.5.2.
#[test]
fn log_bad_style() {
    let file_name = "clf/illegal/log_bad_style.clf";
    check_throw_what(load_clf_file(file_name), "is invalid");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_bad_version)` @ v2.5.2.
#[test]
fn log_bad_version() {
    let file_name = "clf/illegal/log_bad_version.clf";
    check_throw_what(
        load_clf_file(file_name),
        "CLF file version '2' does not support operator 'Log'",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_bad_param)` @ v2.5.2.
#[test]
fn log_bad_param() {
    let file_name = "clf/illegal/log_bad_param.clf";
    check_throw_what(
        load_clf_file(file_name),
        "Parameter 'linSideBreak' is only allowed for style",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_missing_breakpnt)` @ v2.5.2.
#[test]
fn log_missing_breakpnt() {
    let file_name = "clf/illegal/log_missing_breakpnt.clf";
    check_throw_what(
        load_clf_file(file_name),
        "Parameter 'linSideBreak' should be defined for style",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_ocio_params_channels)` @ v2.5.2.
#[test]
fn log_ocio_params_channels() {
    // NB: The blue channel is missing and will use default values. Base can be specified in
    // any channel but has to be specified.
    let strebuf = "<?xml version='1.0' encoding='UTF-8'?>\n\
                   <ProcessList id='none' version='2'>\n\
                   <Log inBitDepth='10i' outBitDepth='16f' style='linToLog'>\n\
                   <LogParams channel='R' linSideSlope='1.1' linSideOffset='0.1' \
                   logSideSlope='0.9' logSideOffset='0.2' base='10.0' />\n\
                   <LogParams channel='G' logSideSlope='0.9' logSideOffset='0.23456' />\n\
                   </Log>\n\
                   </ProcessList>\n";

    let cached_file = parse_string(strebuf).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let log = as_log(&file_ops[0]);
    assert_eq!(log.base(), 10.0);
    assert!(!log.all_components_equal());
    let r_params = log.red_params();
    assert_eq!(r_params[LIN_SIDE_SLOPE], 1.1);
    assert_eq!(r_params[LIN_SIDE_OFFSET], 0.1);
    assert_eq!(r_params[LOG_SIDE_SLOPE], 0.9);
    assert_eq!(r_params[LOG_SIDE_OFFSET], 0.2);
    let g_params = log.green_params();
    assert_eq!(g_params[LIN_SIDE_SLOPE], 1.0);
    assert_eq!(g_params[LIN_SIDE_OFFSET], 0.0);
    assert_eq!(g_params[LOG_SIDE_SLOPE], 0.9);
    assert_eq!(g_params[LOG_SIDE_OFFSET], 0.23456);
    let b_params = log.blue_params();
    assert_eq!(b_params[LIN_SIDE_SLOPE], 1.0);
    assert_eq!(b_params[LIN_SIDE_OFFSET], 0.0);
    assert_eq!(b_params[LOG_SIDE_SLOPE], 1.0);
    assert_eq!(b_params[LOG_SIDE_OFFSET], 0.0);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_ocio_params_base_missmatch)` @ v2.5.2.
#[test]
fn log_ocio_params_base_missmatch() {
    let strebuf = "<?xml version='1.0' encoding='UTF-8'?>\n\
                   <ProcessList id='none' version='2'>\n\
                   <Log inBitDepth='32f' outBitDepth='32f' style='linToLog'>\n\
                   <LogParams channel='R' linSideSlope='1.1' base='2.0'/>\n\
                   <LogParams channel='G' linSideSlope='1.2' base='2.5'/>\n\
                   </Log>\n\
                   </ProcessList>\n";

    check_throw_what(parse_string(strebuf), "base has to be the same");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, log_default_params)` @ v2.5.2.
#[test]
fn log_default_params() {
    let strebuf = "<?xml version='1.0' encoding='UTF-8'?>\n\
                   <ProcessList id='none' version='2'>\n\
                   <Log inBitDepth='32f' outBitDepth='32f' style='linToLog' />\n\
                   <Log inBitDepth='32f' outBitDepth='32f' style='cameraLinToLog'>\n\
                   <LogParams linSideBreak='0.1'/>\n\
                   </Log>\n\
                   </ProcessList>\n";

    let cached_file = parse_string(strebuf).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 2);
    let log = as_log(&file_ops[0]);
    // Default value is 2.
    assert_eq!(log.base(), 2.0);
    let red_params = log.red_params();
    assert_eq!(red_params.len(), 4);
    assert_eq!(red_params[LIN_SIDE_SLOPE], 1.);
    assert_eq!(red_params[LIN_SIDE_OFFSET], 0.);
    assert_eq!(red_params[LOG_SIDE_SLOPE], 1.);
    assert_eq!(red_params[LOG_SIDE_OFFSET], 0.);

    let log = as_log(&file_ops[1]);
    // Default value is 2.
    assert_eq!(log.base(), 2.0);
    let green_params = log.green_params();
    assert_eq!(green_params.len(), 5);
    assert_eq!(green_params[LIN_SIDE_SLOPE], 1.);
    assert_eq!(green_params[LIN_SIDE_OFFSET], 0.);
    assert_eq!(green_params[LOG_SIDE_SLOPE], 1.);
    assert_eq!(green_params[LOG_SIDE_OFFSET], 0.);
    assert_eq!(green_params[LIN_SIDE_BREAK], 0.1);
}

/// The LUT of an op data, which must be one (`dynamic_pointer_cast<const Lut1DOpData>`).
fn as_lut1d(op: &OpData) -> &Lut1DOpData {
    match op {
        OpData::Lut1D(lut) => lut,
        _ => panic!("not a Lut1D: {:?}", op.get_type()),
    }
}

/// The 16 values of the matrix of `lut1d_inv` and `3by1d_lut`.
const XYZ_TO_RGB: [f64; 16] = [
    3.24, -1.537, -0.49850, 0.0, -0.96930, 1.876, 0.04156, 0.0, 0.05560, -0.204, 1.0573, 0.0, 0.0,
    0.0, 0.0, 1.0,
];

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut_1d)` @ v2.5.2.
#[test]
fn lut_1d() {
    {
        let ctf_file = "lut1d_32_10i_10i.ctf";
        let cached_file = load_clf_file(ctf_file).unwrap();
        let transform = &cached_file.transform;
        assert_eq!(transform.get_name(), b"1d-lut example");
        assert_eq!(transform.get_id(), b"9843a859-e41e-40a8-a51c-840889c3774e");
        assert_eq!(transform.get_descriptions().len(), 1);
        assert_eq!(transform.get_descriptions()[0], b"Apply a 1/2.2 gamma.");
        assert_eq!(transform.get_input_descriptor(), b"RGB");
        assert_eq!(transform.get_output_descriptor(), b"RGB");
        let op_list = transform.get_ops();
        assert_eq!(op_list.len(), 1);

        let p_lut = as_lut1d(&op_list[0]);

        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 1);

        assert!(!p_lut.is_input_half_domain());
        assert!(!p_lut.is_output_raw_halfs());
        assert_eq!(p_lut.get_hue_adjust(), Lut1DHueAdjust::None);

        assert_eq!(p_lut.get_file_output_bit_depth(), BitDepth::Uint10);
        assert!(op_list[0].get_name() == b"1d-lut example op");

        // LUT is defined with a 32x1 array. Array is extended to 32x3 by duplicating the
        // available component.
        let array = p_lut.get_array();
        assert_eq!(array.get_length(), 32);
        assert_eq!(array.get_num_color_components(), 1);
        assert_eq!(
            array.get_num_values(),
            array.get_length() * p_lut.get_array().get_max_color_components()
        );

        assert_eq!(array.get_values().len(), 96);
        let values = array.get_values();
        assert_eq!(values[0], 0.0f32);
        assert_eq!(values[1], 0.0f32);
        assert_eq!(values[2], 0.0f32);
        assert_eq!(values[3], 215.0f32 / 1023.0f32);
        assert_eq!(values[4], 215.0f32 / 1023.0f32);
        assert_eq!(values[5], 215.0f32 / 1023.0f32);
        assert_eq!(values[6], 294.0f32 / 1023.0f32);
        // and many more
        assert_eq!(values[92], 1008.0f32 / 1023.0f32);
        assert_eq!(values[93], 1023.0f32 / 1023.0f32);
        assert_eq!(values[94], 1023.0f32 / 1023.0f32);
        assert_eq!(values[95], 1023.0f32 / 1023.0f32);
    }

    // Test the hue adjust attribute.
    {
        let ctf_file = "lut1d_hue_adjust_test.ctf";
        let cached_file = load_clf_file(ctf_file).unwrap();

        let op_list = cached_file.transform.get_ops();
        assert_eq!(op_list.len(), 1);
        let p_lut = as_lut1d(&op_list[0]);
        assert_eq!(p_lut.get_hue_adjust(), Lut1DHueAdjust::Dw3);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_hue_adjust_invalid_style)` @ v2.5.2.
#[test]
fn lut1d_hue_adjust_invalid_style() {
    let ctf_file = "lut1d_hue_adjust_invalid_style.ctf";
    check_throw_what(load_clf_file(ctf_file), "Illegal 'hueAdjust' attribute");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut_3by1d_with_nan_infinity)` @ v2.5.2.
#[test]
fn lut_3by1d_with_nan_infinity() {
    let ctf_file = "lut3by1d_nan_infinity_example.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_lut1d = as_lut1d(&op_list[0]);

    let array = p_lut1d.get_array();

    assert_eq!(array.get_values().len() as c_ulong, array.get_num_values());
    let values = array.get_values();
    assert!(values[0].is_nan());
    assert!(values[1].is_nan());
    assert!(values[2].is_nan());
    assert!(values[3].is_nan());
    assert!(values[4].is_nan());
    assert_eq!(values[5], f32::INFINITY);
    assert_eq!(values[6], f32::INFINITY);
    assert_eq!(values[7], f32::INFINITY);
    assert_eq!(values[8], -f32::INFINITY);
    assert_eq!(values[9], -f32::INFINITY);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_half_domain_set_false)` @ v2.5.2.
#[test]
fn lut1d_half_domain_set_false() {
    // Should throw an exception because the 'half_domain' tag was found but set to something
    // other than 'true'.
    let ctf_file = "clf/illegal/lut1d_half_domain_set_false.clf";
    check_throw_what(load_clf_file(ctf_file), "Illegal 'halfDomain' attribute");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_raw_half_set_false)` @ v2.5.2.
#[test]
fn lut1d_raw_half_set_false() {
    // Should throw an exception because the 'raw_halfs' tag was found but set to something
    // other than 'true'.
    let ctf_file = "clf/illegal/lut1d_raw_half_set_false.clf";
    check_throw_what(load_clf_file(ctf_file), "Illegal 'rawHalfs' attribute");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_half_domain_raw_half_set)` @ v2.5.2.
#[test]
fn lut1d_half_domain_raw_half_set() {
    let ctf_file = "clf/lut1d_half_domain_raw_half_set.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_lut1d = as_lut1d(&op_list[0]);

    assert!(p_lut1d.is_input_half_domain());
    assert!(p_lut1d.is_output_raw_halfs());

    let values = p_lut1d.get_array().get_values();
    assert_eq!(values[0], convert_half_bits_to_float(44646));
    assert_eq!(values[3], convert_half_bits_to_float(44637));
    assert_eq!(values[6], convert_half_bits_to_float(44634));
    assert_eq!(values[9], convert_half_bits_to_float(44631));
    assert_eq!(values[12], convert_half_bits_to_float(44629));
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_half_domain_missing_values)` @ v2.5.2.
#[test]
fn lut1d_half_domain_missing_values() {
    let ctf_file = "clf/illegal/lut1d_half_domain_missing_values.clf";
    // This should fail with invalid entries exception because the number of entries in the
    // op is not 65536 (required when using half domain).
    check_throw_what(load_clf_file(ctf_file), "65536 required for halfDomain");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, 3by1d_lut)` @ v2.5.2.
#[test]
fn three_by_1d_lut() {
    let ctf_file = "clf/xyz_to_rgb.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 3);
    let p_matrix = as_matrix(&op_list[0]);

    let a1 = p_matrix.get_array();
    assert_eq!(a1.get_length(), 4);
    assert_eq!(a1.get_num_color_components(), 4);
    assert_eq!(a1.get_num_values(), a1.get_length() * a1.get_length());
    check_values(a1, XYZ_TO_RGB);

    as_range(&op_list[1]);

    let p_lut = as_lut1d(&op_list[2]);
    assert_eq!(p_lut.get_direction(), TransformDirection::Forward);
    assert_eq!(p_lut.get_file_output_bit_depth(), BitDepth::F32);

    let a2 = p_lut.get_array();
    assert_eq!(a2.get_length(), 128);
    assert_eq!(a2.get_num_color_components(), 3);
    assert_eq!(
        a2.get_num_values(),
        a2.get_length() * p_lut.get_array().get_max_color_components()
    );

    assert_eq!(a2.get_values().len() as c_ulong, a2.get_num_values());
    let values = a2.get_values();
    assert_eq!(values[0], 0.0f32);
    assert_eq!(values[1], 0.0f32);
    assert_eq!(values[2], 0.0f32);
    assert_eq!(values[3], 0.06780f32);

    assert_eq!(values[21], 0.19986f32);
    assert_eq!(values[22], 0.18986f32);
    assert_eq!(values[23], 0.17987f32);

    assert_eq!(values[48], 0.31636f32);
    assert_eq!(values[49], 0.30054f32);
    assert_eq!(values[50], 0.28472f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_long_lut)` @ v2.5.2.
#[test]
fn lut1d_long_lut() {
    let ctf_file = "clf/lut1d_long.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let p_lut = as_lut1d(&op_list[0]);

    let a = p_lut.get_array();
    assert_eq!(a.get_length(), 131072);
    assert_eq!(a.get_num_color_components(), 1);
    assert_eq!(
        a.get_num_values(),
        a.get_length() * p_lut.get_array().get_max_color_components()
    );
    assert_eq!(a.get_values()[393215], 1.293f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_inv)` @ v2.5.2.
#[test]
fn lut1d_inv() {
    let ctf_file = "lut1d_inv.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 2);

    let p_matrix = as_matrix(&op_list[0]);

    let a1 = p_matrix.get_array();
    assert_eq!(a1.get_length(), 4);
    assert_eq!(a1.get_num_color_components(), 4);
    assert_eq!(a1.get_num_values(), a1.get_length() * a1.get_length());
    check_values(a1, XYZ_TO_RGB);

    let p_lut = as_lut1d(&op_list[1]);
    assert_eq!(p_lut.get_file_output_bit_depth(), BitDepth::F32);
    assert_eq!(p_lut.get_direction(), TransformDirection::Inverse);

    let a2 = p_lut.get_array();
    assert_eq!(a2.get_num_color_components(), 3);

    assert_eq!(a2.get_length(), 17);
    assert_eq!(
        a2.get_num_values(),
        a2.get_length() * a2.get_max_color_components()
    );

    let error = 1e-6f32;
    assert_eq!(a2.get_values().len() as c_ulong, a2.get_num_values());

    let values = a2.get_values();
    check_close(values[0], 0.0f32, error);
    check_close(values[1], 0.0f32, error);
    check_close(values[2], 0.0f32, error);
    check_close(values[3], 0.28358f32, error);

    check_close(values[21], 0.68677f32, error);
    check_close(values[22], 0.68677f32, error);
    check_close(values[23], 0.68677f32, error);

    check_close(values[48], 1.0f32, error);
    check_close(values[49], 1.0f32, error);
    check_close(values[50], 1.0f32, error);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut1d_inv_scaling)` @ v2.5.2.
#[test]
fn lut1d_inv_scaling() {
    // Validate that the InverseLUT1D array values are scaled based on inBitDepth. (The
    // previous example had inBitDepth=32f, so it does not validate that.)

    let ctf_file = "lut1d_inverse_halfdom_slog_fclut.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);

    let p_lut = as_lut1d(&op_list[0]);
    // For an InverseLUT1D, the file "out" depth is actually taken from inBitDepth.
    assert_eq!(p_lut.get_file_output_bit_depth(), BitDepth::Uint16);
    assert_eq!(p_lut.get_direction(), TransformDirection::Inverse);

    let a2 = p_lut.get_array();
    assert_eq!(a2.get_num_color_components(), 1);

    assert_eq!(a2.get_length(), 65536);
    assert_eq!(
        a2.get_num_values(),
        a2.get_length() * a2.get_max_color_components()
    );

    let error = 1e-6f32;
    assert_eq!(a2.get_values().len() as c_ulong, a2.get_num_values());

    // Input value 17830 scaled by 65535.
    check_close(a2.get_values()[0], 0.27206836f32, error);
    // Input value 55070 scaled by 65535.
    check_close(a2.get_values()[31743 * 3], 0.84031434f32, error);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, invlut1d_clf)` @ v2.5.2.
#[test]
fn invlut1d_clf() {
    let clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UIDLUT42">
    <InverseLUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
        <Array dim="16 3">
   0    1    2
   3    4    5
   6    7    8
   9   10   11
  12   13   14
  15   16   17
  18   19   20
  21   22   23
  24   25   26
  27   28   29
  30   31   32
  33   34   35
  36   37   38
  39   40   41
  42   43   44
  45   46   47
        </Array>
    </InverseLUT1D>
</ProcessList>
"#;

    check_throw_what(
        parse_string(clf),
        "CLF file version '3' does not support operator 'InverseLUT1D'",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, indexMap_test)` @ v2.5.2.
#[test]
fn index_map_test() {
    let ctf_file = "indexMap_test.ctf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Only two entry IndexMaps are supported",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, indexMap_test1_clfv2)` @ v2.5.2.
#[test]
fn index_map_test1_clfv2() {
    // IndexMaps were allowed in CLF v2 (were removed in v3).
    let ctf_file = "indexMap_test1_clfv2.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 2);
    let p_r = as_range(&op_list[0]);

    // Check that the indexMap caused a Range to be inserted.
    assert_eq!(p_r.get_min_in_value() * 1023., 64.5);
    assert_eq!(p_r.get_max_in_value() * 1023., 940.);
    assert_eq!(p_r.get_min_out_value() * 1023.0, 132.0); // 4*1023/31
    assert_eq!(p_r.get_max_out_value() * 1023.0, 1089.0); // 33*1023/31
    assert_eq!(p_r.get_file_input_bit_depth(), BitDepth::Uint10);
    assert_eq!(p_r.get_file_output_bit_depth(), BitDepth::Uint10);

    // Check the LUT is ok.
    let p_l = as_lut1d(&op_list[1]);
    assert_eq!(p_l.get_array().get_length(), 32);
    assert_eq!(p_l.get_file_output_bit_depth(), BitDepth::Uint12);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, indexMap_test3)` @ v2.5.2.
#[test]
fn index_map_test3() {
    let ctf_file = "indexMap_test3.ctf";
    check_throw_what(load_clf_file(ctf_file), "Only one IndexMap allowed per LUT");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, indexMap_test4_clfv2)` @ v2.5.2.
#[test]
fn index_map_test4_clfv2() {
    let ctf_file = "indexMap_test4_clfv2.clf";
    check_throw_what(
        load_clf_file(ctf_file),
        "Only two entry IndexMaps are supported",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_missing_inbitdepth)` @ v2.5.2.
#[test]
fn transform_missing_inbitdepth() {
    let ctf_file = "clf/illegal/transform_missing_inbitdepth.clf";
    check_throw_what(load_clf_file(ctf_file), "inBitDepth is missing");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_missing_outbitdepth)` @ v2.5.2.
#[test]
fn transform_missing_outbitdepth() {
    let ctf_file = "clf/illegal/transform_missing_outbitdepth.clf";
    check_throw_what(load_clf_file(ctf_file), "outBitDepth is missing");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, transform_with_bitdepth_mismatch)` @ v2.5.2.
#[test]
fn transform_with_bitdepth_mismatch() {
    // Even though we normalize the bit-depths after reading, any mismatches in the file are
    // an indication of improper/unreliable formatting and an exception should be thrown.
    let ctf_file = "clf/illegal/transform_bitdepth_mismatch.clf";
    check_throw_what(load_clf_file(ctf_file), "Bit-depth mismatch");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, inverse_of_id_test)` @ v2.5.2.
#[test]
fn inverse_of_id_test() {
    let ctf_file = "clf/inverseOf_id_test.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    assert!(cached_file.transform.get_inverse_of_id() == b"inverseOfIdTest");
}

/// The LUT of an op data, which must be one (`dynamic_pointer_cast<const Lut3DOpData>`).
fn as_lut3d(op: &OpData) -> &Lut3DOpData {
    match op {
        OpData::Lut3D(lut) => lut,
        _ => panic!("not a Lut3D: {:?}", op.get_type()),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut3d)` @ v2.5.2.
#[test]
fn lut3d() {
    let ctf_file = "clf/lut3d_17x17x17_10i_12i.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);

    let p_lut = as_lut3d(&op_list[0]);
    assert_eq!(p_lut.get_direction(), TransformDirection::Forward);
    assert_eq!(p_lut.get_file_output_bit_depth(), BitDepth::Uint12);
    // Interpolation is not defined in the file.
    assert_eq!(p_lut.get_interpolation(), Interpolation::Default);

    let array = p_lut.get_array();
    assert_eq!(array.get_length(), 17);
    assert_eq!(array.get_num_color_components(), 3);
    assert_eq!(
        array.get_num_values(),
        array.get_length()
            * array.get_length()
            * array.get_length()
            * p_lut.get_array().get_max_color_components()
    );

    assert_eq!(array.get_values().len() as c_ulong, array.get_num_values());
    let tol = 2e-8f32;
    let values = array.get_values();
    check_close(values[0], 0.0f32 / 4095.0f32, tol);
    check_close(values[1], 12.0f32 / 4095.0f32, tol);
    check_close(values[2], 13.0f32 / 4095.0f32, tol);

    check_close(values[18], 0.0f32 / 4095.0f32, tol);
    check_close(values[19], 203.0f32 / 4095.0f32, tol);
    check_close(values[20], 399.0f32 / 4095.0f32, tol);

    check_close(values[30], 54.0f32 / 4095.0f32, tol);
    check_close(values[31], 490.0f32 / 4095.0f32, tol);
    check_close(values[32], 987.0f32 / 4095.0f32, tol);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut3d_inv)` @ v2.5.2.
#[test]
fn lut3d_inv() {
    let ctf_file = "lut3d_example_Inv.ctf";

    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);

    let p_lut = as_lut3d(&op_list[0]);

    // For an InverseLUT3D, the file "out" depth is set by the inBitDepth of the file.
    assert_eq!(p_lut.get_file_output_bit_depth(), BitDepth::Uint12);

    assert_eq!(p_lut.get_interpolation(), Interpolation::Tetrahedral);
    assert_eq!(p_lut.get_direction(), TransformDirection::Inverse);

    let array = p_lut.get_array();
    assert_eq!(array.get_num_color_components(), 3);
    assert_eq!(
        array.get_num_values(),
        array.get_length()
            * array.get_length()
            * array.get_length()
            * array.get_max_color_components()
    );
    assert_eq!(array.get_values().len() as c_ulong, array.get_num_values());

    // Validate that the array was scaled by the inBitDepth of the file.
    assert_eq!(array.get_length(), 17);
    let values = array.get_values();
    check_close(values[0], 25.0f32 / 4095.0f32, 1e-8f32);
    check_close(values[1], 30.0f32 / 4095.0f32, 1e-8f32);
    assert_eq!(values[2], 33.0f32 / 4095.0f32);

    check_close(values[18], 26.0f32 / 4095.0f32, 1e-8f32);
    assert_eq!(values[19], 308.0f32 / 4095.0f32);
    assert_eq!(values[20], 580.0f32 / 4095.0f32);

    assert_eq!(values[30], 0.0f32);
    assert_eq!(values[31], 586.0f32 / 4095.0f32);
    assert_eq!(values[32], 1350.0f32 / 4095.0f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut3d_unequal_size)` @ v2.5.2.
#[test]
fn lut3d_unequal_size() {
    let file_name = "clf/illegal/lut3d_unequal_size.clf";
    check_throw_what(load_clf_file(file_name), "Illegal array dimensions 2 2 3 3");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, tabluation_support)` @ v2.5.2.
#[test]
fn tabluation_support() {
    // This clf file contains tabulations used as delimiters for a series of numbers.
    let ctf_file = "clf/tabulation_support.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();
    assert_eq!(
        cached_file.transform.get_id(),
        b"e0a0ae4b-adc2-4c25-ad70-fa6f31ba219d"
    );
    assert_eq!(op_list.len(), 1);

    let p_l = as_lut3d(&op_list[0]);

    assert_eq!(p_l.get_file_output_bit_depth(), BitDepth::Uint10);
    assert_eq!(p_l.get_interpolation(), Interpolation::Tetrahedral);

    let array = p_l.get_array();
    assert_eq!(array.get_length(), 3);
    assert_eq!(array.get_num_color_components(), 3);
    assert_eq!(array.get_num_values(), 81);
    assert_eq!(array.get_values().len(), 81);

    let scale = get_bit_depth_max_value(BitDepth::Uint10).unwrap() as f32;
    let values = array.get_values();
    assert_eq!(values[0] * scale, -60.0f32);
    assert_eq!(values[1] * scale, 5.0f32);
    assert_eq!(values[2] * scale, 75.0f32);

    assert_eq!(values[3] * scale, -10.0f32);
    check_close(values[4] * scale, 50.0f32, 1e-5f32);
    check_close(values[5] * scale, 400.0f32, 1e-4f32);

    assert_eq!(values[6] * scale, 0.0f32);
    check_close(values[7] * scale, 100.0f32, 1e-4f32);
    assert_eq!(values[8] * scale, 1200.0f32);

    assert_eq!(values[9] * scale, -40.0f32);
    assert_eq!(values[10] * scale, 500.0f32);
    assert_eq!(values[11] * scale, -30.0f32);

    assert_eq!(values[3 * 26] * scale, 1110.0f32);
    assert_eq!(values[3 * 26 + 1] * scale, 900.0f32);
    assert_eq!(values[3 * 26 + 2] * scale, 1200.0f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, indexMap_test2_clfv2)` @ v2.5.2.
#[test]
fn index_map_test2_clfv2() {
    let ctf_file = "indexMap_test2_clfv2.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 2);
    let p_r = as_range(&op_list[0]);
    assert_eq!(p_r.get_min_in_value(), f64::from(-0.1f32));
    assert_eq!(p_r.get_max_in_value(), f64::from(19.0f32));
    assert_eq!(p_r.get_min_out_value(), f64::from(0.0f32));
    assert_eq!(p_r.get_max_out_value(), f64::from(1.0f32));
    assert_eq!(p_r.get_file_input_bit_depth(), BitDepth::F32);
    assert_eq!(p_r.get_file_output_bit_depth(), BitDepth::F32);

    // Check the LUT is ok.
    let p_l = as_lut3d(&op_list[1]);
    assert_eq!(p_l.get_array().get_length(), 2);
    assert_eq!(p_l.get_file_output_bit_depth(), BitDepth::Uint10);
}
