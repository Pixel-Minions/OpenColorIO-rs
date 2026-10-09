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
use ocio_ops::op_data::OpDataType;
use ocio_ops::open_color_types::BitDepth;
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
