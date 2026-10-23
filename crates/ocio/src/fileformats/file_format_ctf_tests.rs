// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the CLF/CTF reader and writer: `tests/cpu/fileformats/FileFormatCTF_tests.cpp` @
//! v2.5.2 (WP 4.5b-4.5i, 4.6a-4.6e).

use std::ffi::c_ulong;

use super::*;
use crate::fileformats::ctf::ctf_transform::{
    CTF_PROCESS_LIST_VERSION_1_2, CTF_PROCESS_LIST_VERSION_1_3, CTF_PROCESS_LIST_VERSION_1_4,
    CtfVersion, get_elements_values,
};
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::{get_test_files_dir, load_test_file};
use crate::processor::Processor;
use crate::transform::Transform;
use crate::transforms::cdl_transform::CdlTransform;
use crate::transforms::exponent_transform::ExponentTransform;
use crate::transforms::exponent_with_linear_transform::ExponentWithLinearTransform;
use crate::transforms::fixed_function_transform::FixedFunctionTransform;
use crate::transforms::log_affine_transform::LogAffineTransform;
use crate::transforms::log_camera_transform::LogCameraTransform;
use crate::transforms::log_transform::LogTransform;
use crate::transforms::lut1d_transform::Lut1DTransform;
use crate::transforms::lut3d_transform::Lut3DTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::transforms::range_transform::{RangeStyle, RangeTransform};
use ocio_formats::fileformats::xmlutils::xml_reader_helper::{
    METADATA_SAT_DESCRIPTION, METADATA_SOP_DESCRIPTION,
};
use ocio_ops::bit_depth_utils::get_bit_depth_max_value;
use ocio_ops::format_metadata::{METADATA_DESCRIPTION, METADATA_ID, METADATA_INFO, METADATA_NAME};
use ocio_ops::imath_half::{float_to_half, half_to_float};
use ocio_ops::logging::{get_logging_level, set_logging_level};
use ocio_ops::math_utils::convert_half_bits_to_float;
use ocio_ops::op_data::OpDataType;
use ocio_ops::open_color_types::{
    BitDepth, CdlStyle, FixedFunctionStyle, GradingStyle, HsyTransformStyle, HueCurveType,
    Interpolation, LoggingLevel, Lut1DHueAdjust, NegativeStyle, RgbCurveType, TransformDirection,
};
use ocio_ops::ops::cdl::cdl_op_data::{CdlOpData, CdlOpStyle, ChannelParams};
use ocio_ops::ops::exposurecontrast::exposure_contrast_op_data::{
    ExposureContrastOpData, ExposureContrastOpStyle,
};
use ocio_ops::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle,
};
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op_data::GradingHueCurveOpData;
use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_ops::ops::gradingprimary::grading_primary_op_data::GradingPrimaryOpData;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::GradingControlPoint;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::default_lin_curve;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone};
use ocio_ops::ops::gradingtone::grading_tone_op_data::GradingToneOpData;
use ocio_ops::ops::log::log_op_data::{
    LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LINEAR_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE,
    LogOpData,
};
use ocio_ops::ops::lut1d::lut1d_op_data::Lut1DOpData;
use ocio_ops::ops::lut3d::lut3d_op_data::Lut3DOpData;
use ocio_ops::ops::matrix::matrix_op_data::{MatrixArray, MatrixOpData};
use ocio_ops::ops::range::range_op_data::RangeOpData;
use ocio_ops::ops::reference::reference_op_data::{ReferenceOpData, ReferenceStyle};
use ocio_ops::utils::string_utils::trim;
use ocio_testkit::compare::assert_text_eq;
use ocio_testkit::upstream::{check_close, check_str_float_vec_close, check_throw_what};

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

/// Runs `f` at the warning logging level, and gives the lines OCIO logged on this thread
/// meanwhile, right-trimmed and split by lines.
///
/// Port of the tests' use of `LogGuard` with `SetLoggingLevel(LOGGING_LEVEL_WARNING)` and
/// `StringUtils::SplitByLines(StringUtils::RightTrim(guard.output()))`
/// (tests/cpu/UnitTestLogUtils.cpp:17-67 @ v2.5.2).
fn warning_lines<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    let (out, log) = crate::test_env::capture_log(|| {
        let level = get_logging_level();
        set_logging_level(LoggingLevel::Warning);
        let out = f();
        set_logging_level(level);
        out
    });
    let output = String::from_utf8_lossy(&log.concat()).into_owned();
    let lines = output.trim_end().split('\n').map(str::to_owned).collect();
    (out, lines)
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, difficult_syntax)` @ v2.5.2.
#[test]
fn difficult_syntax() {
    // This file contains a lot of unusual (but still legal) ways of writing the XML. It is
    // intended to stress test that the XML parsing is working robustly.

    let ctf_file = "clf/difficult_syntax.clf";

    let cached_file = load_clf_file(ctf_file).unwrap();

    let clf_version = cached_file.transform.get_clf_version();
    let ver = CtfVersion::new(3, 0, 0);
    assert_eq!(clf_version, ver);

    assert_eq!(cached_file.transform.get_id(), b"id1");

    assert_eq!(cached_file.transform.get_descriptions().len(), 2);
    assert_eq!(
        cached_file.transform.get_descriptions()[0],
        b"This is the ProcessList description."
    );
    assert_eq!(
        cached_file.transform.get_descriptions()[1],
        b"yet 'another' \"valid\" desc"
    );

    let info = cached_file.transform.get_info_metadata();
    assert_eq!(info.get_element_name(), METADATA_INFO);
    let items = info.get_children_elements();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get_element_name(), b"Stuff");
    assert_eq!(
        items[0].get_element_value(),
        b"This is a \"difficult\" but 'legal' color transform file."
    );

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 2);
    {
        let p_matrix = as_matrix(&op_list[0]);

        assert_eq!(op_list[0].get_id(), b"'mat-25'");
        assert_eq!(op_list[0].get_name(), b"\"quote\"");

        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 1);
        assert_eq!(desc[0], b"third array dim value is ignored");

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
                -0.4985,
                0.0,
                -0.96930,
                1.876,
                0.04156,
                0.0,
                0.0556,
                -0.204,
                0.105730e+1,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
        );
    }
    {
        let p_lut = as_lut1d(&op_list[1]);

        assert_eq!(op_list[1].get_name(), b"a multi-line  name");

        let desc = descriptions(&op_list[1]);
        assert_eq!(desc.len(), 3);
        // the string here uses opt-dash
        assert_eq!(desc[0], "the n\u{2013}dash description".as_bytes());
        assert_eq!(desc[1], b"another valid description element    ");
        assert_eq!(desc[2], b"& another <valid> desc");

        let array2 = p_lut.get_array();
        assert_eq!(array2.get_length(), 128);
        assert_eq!(array2.get_num_color_components(), 3);
        assert_eq!(
            array2.get_num_values(),
            array2.get_length() * p_lut.get_array().get_max_color_components()
        );

        assert_eq!(array2.get_values().len(), 384);
        let values = array2.get_values();
        assert_eq!(values[0], 0.0f32);
        assert_eq!(values[1], 0.0f32);
        assert_eq!(values[2], 0.0f32);
        assert_eq!(values[3], 0.06780f32);
        assert_eq!(values[4], 0.06441f32);
        assert_eq!(values[5], 0.06102f32);
        assert_eq!(values[6], 0.09965f32);
        assert_eq!(values[378], 0.99562f32);
        assert_eq!(values[379], 0.94584f32);
        assert_eq!(values[380], 0.89606f32);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, difficult_xml_unknown_elements)` @ v2.5.2.
#[test]
fn difficult_xml_unknown_elements() {
    let cached_file;

    {
        let error_outputs: [&str; 11] = [
            "(10): Unrecognized element 'Ignore' where its parent is 'ProcessList' (8): Unknown element",
            "(22): Unrecognized attribute 'id' of 'Array'",
            "(22): Unrecognized attribute 'foo' of 'Array'",
            "(27): Unrecognized element 'ProcessList' where its parent is 'ProcessList' (8): The Transform already exists",
            "(30): Unrecognized element 'Array' where its parent is 'Matrix' (16): Only one Array allowed per op",
            "(37): Unrecognized element 'just_ignore' where its parent is 'ProcessList' (8): Unknown element",
            "(69): Unrecognized element 'just_ignore' where its parent is 'Description' (66)",
            "(70): Unrecognized element 'just_ignore' where its parent is 'just_ignore' (69)",
            "(75): Unrecognized element 'Matrix' where its parent is 'LUT1D' (", // Line number is expat library version dependent.
            "(76): Unrecognized element 'Description' where its parent is 'Matrix' (75)",
            "(77): Unrecognized element 'Array' where its parent is 'Matrix' (75)",
        ];

        let ctf_file = "difficult_test1_v1.ctf";
        let (result, parts) = warning_lines(|| load_clf_file(ctf_file));
        cached_file = result.unwrap();

        assert_eq!(parts.len(), 11);

        for (part, error_output) in parts.iter().zip(error_outputs) {
            assert!(
                part.contains(error_output),
                "{part:?} holds {error_output:?}"
            );
        }
    }

    // Defaults to 1.2
    let ctf_version = cached_file.transform.get_ctf_version();
    assert!(CTF_PROCESS_LIST_VERSION_1_2 == ctf_version);

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 2);

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
            -0.4985,
            0.0,
            -0.96930,
            1.876,
            0.04156,
            0.0,
            0.0556,
            -0.204,
            0.105730e+1,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ],
    );

    let p_lut = as_lut1d(&op_list[1]);

    let array2 = p_lut.get_array();
    assert_eq!(array2.get_length(), 17);
    assert_eq!(array2.get_num_color_components(), 3);
    assert_eq!(
        array2.get_num_values(),
        array2.get_length() * p_lut.get_array().get_max_color_components()
    );

    assert_eq!(array2.get_values().len(), 51);
    let values = array2.get_values();
    assert_eq!(values[0], 0.0f32);
    assert_eq!(values[1], 0.0f32);
    assert_eq!(values[2], 0.0f32);
    assert_eq!(values[3], 0.28358f32);
    assert_eq!(values[4], 0.28358f32);
    assert_eq!(values[5], 0.28358f32);
    assert_eq!(values[6], 0.38860f32);
    assert_eq!(values[45], 0.97109f32);
    assert_eq!(values[46], 0.97109f32);
    assert_eq!(values[47], 0.97109f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, unknown_elements)` @ v2.5.2.
#[test]
fn unknown_elements() {
    let cached_file;

    {
        let error_outputs: [&str; 3] = [
            "(34): Unrecognized element 'B' where its parent is 'ProcessList' (2): Unknown element",
            "(34): Unrecognized element 'C' where its parent is 'B' (34)",
            "(36): Unrecognized element 'A' where its parent is 'Description' (36)",
        ];

        // NB: This file has some added unknown elements A, B, and C as a test.
        let ctf_file = "clf/illegal/unknown_elements.clf";
        let (result, parts) = warning_lines(|| load_clf_file(ctf_file));
        cached_file = result.unwrap();

        assert_eq!(parts.len(), 3);

        for (part, error_output) in parts.iter().zip(error_outputs) {
            assert!(
                part.contains(error_output),
                "{part:?} holds {error_output:?}"
            );
        }
    }

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 4);

    let p_matrix = as_matrix(&op_list[0]);

    let a1 = p_matrix.get_array();
    assert_eq!(a1.get_length(), 4);
    assert_eq!(a1.get_num_color_components(), 4);
    assert_eq!(a1.get_num_values(), a1.get_length() * a1.get_length());

    assert_eq!(a1.get_values().len() as c_ulong, a1.get_num_values());
    assert_eq!(a1.get_values()[0], 3.24);
    assert_eq!(a1.get_values()[4], -0.96930);
    assert_eq!(a1.get_values()[10], 1.0573);

    let p_lut1 = as_lut1d(&op_list[1]);

    let a2 = p_lut1.get_array();
    assert_eq!(a2.get_length(), 17);
    assert_eq!(a2.get_num_color_components(), 3);
    assert_eq!(
        a2.get_num_values(),
        a2.get_length() * p_lut1.get_array().get_max_color_components()
    );

    assert_eq!(a2.get_values().len() as c_ulong, a2.get_num_values());
    assert_eq!(a2.get_values()[3], 0.28358f32);
    assert_eq!(a2.get_values()[4], 0.28358f32);
    assert_eq!(a2.get_values()[5], 100.0f32);
    assert_eq!(a2.get_values()[50], 1.0f32);

    let p_lut2 = as_lut1d(&op_list[2]);
    assert_eq!(p_lut2.get_file_output_bit_depth(), BitDepth::Uint10);

    let array = p_lut2.get_array();
    assert_eq!(array.get_length(), 32);
    assert_eq!(array.get_num_color_components(), 1);
    assert_eq!(
        array.get_num_values(),
        array.get_length() * p_lut2.get_array().get_max_color_components()
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

    let p_lut3 = as_lut3d(&op_list[3]);
    assert_eq!(p_lut3.get_file_output_bit_depth(), BitDepth::Uint10);

    let a3 = p_lut3.get_array();
    assert_eq!(a3.get_length(), 3);
    assert_eq!(a3.get_num_color_components(), 3);
    assert_eq!(
        a3.get_num_values(),
        a3.get_length()
            * a3.get_length()
            * a3.get_length()
            * p_lut3.get_array().get_max_color_components()
    );

    assert_eq!(a3.get_values().len() as c_ulong, a3.get_num_values());
    let values = a3.get_values();
    assert_eq!(values[0], 0.0f32);
    assert_eq!(values[1], 30.0f32 / 1023.0f32);
    assert_eq!(values[2], 33.0f32 / 1023.0f32);
    assert_eq!(values[3], 0.0f32);
    assert_eq!(values[4], 0.0f32);
    assert_eq!(values[5], 133.0f32 / 1023.0f32);

    assert_eq!(values[78], 1023.0f32 / 1023.0f32);
    assert_eq!(values[79], 1023.0f32 / 1023.0f32);
    assert_eq!(values[80], 1023.0f32 / 1023.0f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, clf3_index_map)` @ v2.5.2.
#[test]
fn clf3_index_map() {
    // Same as previous, but setting compCLFversion=3.0.
    let ctf_file = "clf/illegal/indexMap_test2.clf";
    let warning = "Element 'IndexMap' is not valid since CLF 3 (or CTF 2)";

    let (result, parts) = warning_lines(|| load_clf_file(ctf_file));
    result.unwrap();
    assert!(parts.join("\n").contains(warning));
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, multiple_ops)` @ v2.5.2.
#[test]
fn multiple_ops() {
    let ctf_file = "clf/multiple_ops.clf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 9);

    {
        // Op 0 == CDL.
        let cdl_op_data = as_cdl(&op_list[0]);
        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 1);
        assert_eq!(desc[0], b"scene 1 exterior look");
        assert_eq!(cdl_op_data.get_style(), CdlOpStyle::V1_2Rev);
        assert!(*cdl_op_data.get_slope_params() == ChannelParams::new(1.1, 1., 0.8));
        assert!(*cdl_op_data.get_offset_params() == ChannelParams::new(-0.01, 0., 0.05));
        assert!(*cdl_op_data.get_power_params() == ChannelParams::new(1.05, 1.15, 0.8));
        assert_eq!(cdl_op_data.get_saturation(), 0.85);
    }
    {
        // Op 1 == Lut1D.
        let l1_op_data = as_lut1d(&op_list[1]);
        assert_eq!(l1_op_data.get_file_output_bit_depth(), BitDepth::Uint12);
        let desc = descriptions(&op_list[1]);
        assert_eq!(desc.len(), 0);
        assert_eq!(l1_op_data.get_array().get_length(), 65);
    }
    {
        // Op 2 == Range.
        // Check that the noClamp style Range became a Matrix.
        let mat_op_data = as_matrix(&op_list[2]);
        assert_eq!(mat_op_data.get_file_input_bit_depth(), BitDepth::Uint12);
        assert_eq!(mat_op_data.get_file_output_bit_depth(), BitDepth::Uint10);

        let out_scale = get_bit_depth_max_value(BitDepth::Uint10).unwrap();
        let mat_scale = out_scale / get_bit_depth_max_value(BitDepth::Uint12).unwrap();
        let array = mat_op_data.get_array();
        assert_eq!(array.get_length(), 4);
        assert_eq!(array.get_num_color_components(), 4);
        assert_eq!(
            array.get_num_values(),
            array.get_length() * array.get_length()
        );

        let scalef: f32 = (1200.0f32 - 20.0f32) / (3760.0f32 - 256.0f32);
        let offsetf: f32 = 20.0f32 - scalef * 256.0f32;
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
    {
        // Op 3 == Range with Clamp.
        let range_op_data = as_range(&op_list[3]);
        assert_eq!(range_op_data.get_file_input_bit_depth(), BitDepth::Uint10);
        assert_eq!(range_op_data.get_file_output_bit_depth(), BitDepth::Uint10);
    }
    {
        // Op 4 == Range with Clamp.
        // A range without style defaults to clamp.
        let range_op_data = as_range(&op_list[4]);
        assert_eq!(range_op_data.get_file_input_bit_depth(), BitDepth::Uint10);
        assert_eq!(range_op_data.get_file_output_bit_depth(), BitDepth::Uint10);
    }
    {
        // Op 5 == Log.
        as_log(&op_list[5]);
    }
    {
        // Op 6 == Matrix with offset.
        let mat_op_data2 = as_matrix(&op_list[6]);
        let array2 = mat_op_data2.get_array();
        assert_eq!(array2.get_length(), 4);
        assert_eq!(array2.get_values()[2], -0.2);
        let offsets2 = mat_op_data2.get_offsets();
        assert_eq!(offsets2[1], -0.11);
    }
    {
        // Op 7 == Exponent.
        as_gamma(&op_list[7]);
    }
    {
        // Op 8 == Lut3D.
        as_lut3d(&op_list[8]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, clf_examples)` @ v2.5.2.
#[test]
fn clf_examples() {
    {
        let ctf_file = "clf/lut1d_example.clf";
        let cached_file = load_clf_file(ctf_file).unwrap();
        let transform = &cached_file.transform;
        assert_eq!(transform.get_name(), b"transform example lut1d");
        assert_eq!(transform.get_id(), b"exlut1");
        assert_eq!(transform.get_descriptions().len(), 1);
        assert_eq!(
            transform.get_descriptions()[0],
            b"1D LUT with legal out of range values"
        );
        let op_list = transform.get_ops();
        assert_eq!(op_list.len(), 1);
        assert_eq!(op_list[0].get_type(), OpDataType::Lut1D);
        assert_eq!(op_list[0].get_name(), b"65valueLut");
        assert_eq!(op_list[0].get_id(), b"lut-23");
        let lut = as_lut1d(&op_list[0]);
        assert_eq!(lut.get_file_output_bit_depth(), BitDepth::Uint12);
        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 2);
        assert_eq!(
            desc[0],
            b"Note that the bit-depth does not constrain the legal range of values."
        );
        assert_eq!(desc[1], b"Formula: flipud(1.25 - 1.5 * x^2.2)");
    }

    {
        let ctf_file = "clf/lut3d_identity_12i_16f.clf";
        let cached_file = load_clf_file(ctf_file).unwrap();
        let transform = &cached_file.transform;
        assert_eq!(transform.get_name(), b"transform example lut3d");
        assert_eq!(transform.get_id(), b"exlut2");
        assert_eq!(transform.get_descriptions().len(), 1);
        assert_eq!(transform.get_descriptions()[0], b" 3D LUT example ");
        let op_list = transform.get_ops();
        assert_eq!(op_list.len(), 1);
        assert_eq!(op_list[0].get_name(), b"identity");
        assert_eq!(op_list[0].get_id(), b"lut-24");
        let lut = as_lut3d(&op_list[0]);
        assert_eq!(lut.get_interpolation(), Interpolation::Tetrahedral);
        assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F16);
        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 1);
        assert_eq!(desc[0], b" 3D LUT ");
    }

    {
        let ctf_file = "clf/matrix_3x4_example.clf";
        let cached_file = load_clf_file(ctf_file).unwrap();
        let transform = &cached_file.transform;
        assert_eq!(transform.get_name(), b"transform example matrix");
        assert_eq!(transform.get_id(), b"exmat1");
        assert_eq!(transform.get_descriptions().len(), 2);
        assert_eq!(transform.get_descriptions()[0], b" Matrix example ");
        assert_eq!(transform.get_descriptions()[1], b" Used by unit tests ");
        let op_list = transform.get_ops();
        assert_eq!(op_list.len(), 1);
        assert_eq!(op_list[0].get_name(), b"colorspace conversion");
        assert_eq!(op_list[0].get_id(), b"mat-25");
        let mat = as_matrix(&op_list[0]);
        assert_eq!(mat.get_file_input_bit_depth(), BitDepth::Uint10);
        assert_eq!(mat.get_file_output_bit_depth(), BitDepth::Uint12);
        let desc = descriptions(&op_list[0]);
        assert_eq!(desc.len(), 1);
        assert_eq!(desc[0], b" 3x4 Matrix , 4th column is offset ");

        // In file, matrix is defined by a 4x4 array.
        let array = mat.get_array();
        assert_eq!(array.get_length(), 4);
        assert_eq!(array.get_num_color_components(), 4);
        assert_eq!(
            array.get_num_values(),
            array.get_length() * array.get_length()
        );

        let oscale = get_bit_depth_max_value(BitDepth::Uint12).unwrap();
        let scale = oscale / get_bit_depth_max_value(BitDepth::Uint10).unwrap();

        // Check matrix ...
        let values = array.get_values();
        assert_eq!(values.len() as c_ulong, array.get_num_values());
        assert_eq!(values[0] * scale, 3.60);
        assert_eq!(values[1] * scale, 0.10);
        assert_eq!(values[2] * scale, -0.20);
        assert_eq!(values[3], 0.0);

        assert_eq!(values[4] * scale, 0.20);
        assert_eq!(values[5] * scale, 3.50);
        assert_eq!(values[6] * scale, 0.10);
        assert_eq!(values[7], 0.0);

        assert_eq!(values[8] * scale, 0.10);
        assert_eq!(values[9] * scale, -0.30);
        assert_eq!(values[10] * scale, 3.40);
        assert_eq!(values[11], 0.0);

        assert_eq!(values[12], 0.0);
        assert_eq!(values[13], 0.0);
        assert_eq!(values[14], 0.0);
        assert_eq!(values[15], 1.0);

        let offsets = mat.get_offsets();
        // ... offsets.
        assert_eq!(offsets[0] * oscale, 0.30);
        assert_eq!(offsets[1] * oscale, -0.05);
        assert_eq!(offsets[2] * oscale, -0.40);
        assert_eq!(offsets[3], 0.0);
    }

    {
        // Test two-entries IndexMap support.
        let ctf_file = "indexMap_test_clfv2.clf";
        let cached_file = load_clf_file(ctf_file).unwrap();
        let transform = &cached_file.transform;
        assert_eq!(transform.get_name(), b"transform example lut IndexMap");
        assert_eq!(transform.get_id(), b"exlut3");
        assert_eq!(transform.get_descriptions().len(), 1);
        assert_eq!(
            transform.get_descriptions()[0],
            b" IndexMap LUT example from spec "
        );
        let op_list = transform.get_ops();
        assert_eq!(op_list.len(), 2);
        let p_r = as_range(&op_list[0]);
        assert_eq!(p_r.get_file_input_bit_depth(), BitDepth::Uint10);
        assert_eq!(p_r.get_file_output_bit_depth(), BitDepth::Uint10);
        assert_eq!(p_r.get_min_in_value(), 64. / 1023.);
        assert_eq!(p_r.get_max_in_value(), 940. / 1023.);
        assert_eq!(p_r.get_min_out_value(), 0. / 1023.);
        assert_eq!(p_r.get_max_out_value(), 1023. / 1023.);

        assert_eq!(op_list[1].get_name(), b"IndexMap LUT");
        assert_eq!(op_list[1].get_id(), b"lut-26");
        let lut = as_lut1d(&op_list[1]);
        assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F16);
        let desc = descriptions(&op_list[1]);
        assert_eq!(desc.len(), 1);
        assert_eq!(desc[0], b" 1D LUT with IndexMap ");
    }
}

/// The interpolation of the group's only transform, a 3D LUT.
fn lut3d_interpolation(config: &Config, file_transform: &FileTransform) -> Interpolation {
    let proc = config
        .processor(&Transform::File(file_transform.clone()))
        .unwrap();
    let group = proc.create_group_transform().unwrap();
    assert_eq!(group.num_transforms(), 1);
    match group.transform(0).unwrap() {
        Transform::Lut3D(lut3d) => lut3d.interpolation(),
        other => panic!("not a 3D LUT: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, lut_interpolation_option)` @ v2.5.2.
#[test]
fn lut_interpolation_option() {
    // Create empty Config to use.
    let mut config = Config::new().unwrap();
    config.set_major_version(2).unwrap();

    // CLF file containing a LUT3D that does not specifiy an interpolation.
    let file_path = format!("{}/clf/lut3d_17x17x17_10i_12i.clf", get_test_files_dir());

    let mut file_transform = FileTransform::new();
    file_transform.set_direction(TransformDirection::Forward);
    file_transform.set_src(&file_path);

    // Get the processor corresponding to the transform with default interpolation. LUT3D is
    // using default interpolation.
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Default
    );

    // For LUTs that do not specify an interpolation, if the FileTransform interpolation may be
    // used, it is.
    file_transform.set_interpolation(Interpolation::Best);
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Best
    );

    // If the FileTranform interpolation is not supported by the LUT, it is ignored and default
    // interpolation is used.
    file_transform.set_interpolation(Interpolation::Cubic);
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Default
    );

    // CTF file containing a LUT3D that specifies tetrahedral interpolation.
    let file_path2 = format!("{}/lut3d_example_Inv.ctf", get_test_files_dir());

    file_transform.set_src(&file_path2);

    // Whatever the file transform interpolation, LUT will keep its interpolation.
    file_transform.set_interpolation(Interpolation::Default);
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Tetrahedral
    );

    // (INTERP_LINEAR is trilinear for LUT3D.)
    file_transform.set_interpolation(Interpolation::Linear);
    assert_eq!(
        lut3d_interpolation(&config, &file_transform),
        Interpolation::Tetrahedral
    );
}

/// The fixed function of a reader's op.
fn as_fixed_function(op: &OpData) -> &FixedFunctionOpData {
    match op {
        OpData::FixedFunction(func) => func,
        _ => panic!("not a FixedFunction"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_deprecated_ops_file)` @ v2.5.2.
#[test]
fn load_deprecated_ops_file() {
    let file_name = "deprecated_ops.ctf";
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 3);

    // Test ACES RedMod03 (deprecated) conversion to the modern representation.
    {
        let func = as_fixed_function(&file_ops[0]);
        assert_eq!(func.style(), FixedFunctionOpStyle::AcesRedMod03Inv);
        func.validate().unwrap();
        assert!(func.params().is_empty());
    }

    // Test ACES Surround (deprecated) conversion to the modern representation.
    {
        let func = as_fixed_function(&file_ops[1]);
        assert_eq!(func.style(), FixedFunctionOpStyle::Rec2100SurroundFwd);
        func.validate().unwrap();

        let params = vec![1.2];
        assert!(*func.params() == params);
    }

    // Test Function (deprecated) conversion to the modern representation.
    {
        let func = as_fixed_function(&file_ops[2]);
        assert_eq!(func.style(), FixedFunctionOpStyle::HsvToRgb);
        func.validate().unwrap();
        assert!(func.params().is_empty());
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_fixed_function_file)` @ v2.5.2.
#[test]
fn load_fixed_function_file() {
    let file_name = "fixed_function.ctf";
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 2);

    // Test FixedFunction with the REC2100_SURROUND_FWD style.
    {
        let func = as_fixed_function(&file_ops[0]);
        assert_eq!(func.style(), FixedFunctionOpStyle::Rec2100SurroundFwd);
        func.validate().unwrap();

        let params = vec![0.8];
        assert!(*func.params() == params);
    }

    // Test FixedFunction with the HSV_to_RGB style.
    {
        let func = as_fixed_function(&file_ops[1]);
        assert_eq!(func.style(), FixedFunctionOpStyle::HsvToRgb);
        func.validate().unwrap();
        assert!(func.params().is_empty());
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_ff_fail_version)` @ v2.5.2.
#[test]
fn load_ff_fail_version() {
    let mut strebuf = String::new();
    strebuf.push_str("<?xml version='1.0' encoding='UTF-8'?>\n");
    strebuf.push_str("<ProcessList id='none' version='1.5'>\n");
    strebuf.push_str("    <FixedFunction inBitDepth='8i' outBitDepth='32f' ");
    strebuf.push_str("params = '0.8' ");
    strebuf.push_str("style = 'Rec2100SurroundFwd' />\n");
    strebuf.push_str("</ProcessList>\n");

    check_throw_what(
        parse_string(&strebuf),
        "CTF file version '1.5' does not support operator 'FixedFunction'",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_ff_fail_params)` @ v2.5.2.
#[test]
fn load_ff_fail_params() {
    let mut strebuf = String::new();
    strebuf.push_str("<?xml version='1.0' encoding='UTF-8'?>\n");
    strebuf.push_str("<ProcessList id='none' version='2'>\n");
    strebuf.push_str("    <FixedFunction inBitDepth='8i' outBitDepth='32f' ");
    strebuf.push_str("params = '0.8 2.0' ");
    strebuf.push_str("style = 'Rec2100SurroundFwd' />\n");
    strebuf.push_str("</ProcessList>\n");

    check_throw_what(
        parse_string(&strebuf),
        "must have one parameter but 2 found",
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_ff_fail_style)` @ v2.5.2.
#[test]
fn load_ff_fail_style() {
    let mut strebuf = String::new();
    strebuf.push_str("<?xml version='1.0' encoding='UTF-8'?>\n");
    strebuf.push_str("<ProcessList id='none' version='2.0'>\n");
    strebuf.push_str(
        "    <FixedFunction inBitDepth='16i' outBitDepth='32f' style='UnknownStyle' />\n",
    );
    strebuf.push_str("</ProcessList>\n");

    check_throw_what(parse_string(&strebuf), "Unknown FixedFunction style");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_ff_aces_fail_gamma_param)` @ v2.5.2. Upstream's
/// `LogGuard` is the crate's `test_env::capture_log`.
#[test]
fn load_ff_aces_fail_gamma_param() {
    let mut strebuf = String::new();
    strebuf.push_str("<?xml version='1.0' encoding='UTF-8'?>\n");
    strebuf.push_str("<ProcessList id='none' version='1.5'>\n");
    strebuf.push_str("    <ACES inBitDepth='16i' outBitDepth='32f' style='Surround'>\n");
    strebuf.push_str("        <ACESParams wrongParam='1.2' />\n");
    strebuf.push_str("    </ACES>\n");
    strebuf.push_str("</ProcessList>\n");

    let (result, _log) = crate::test_env::capture_log(|| parse_string(&strebuf));
    check_throw_what(result, "Missing required parameter");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_ff_aces_fail_gamma_twice)` @ v2.5.2.
#[test]
fn load_ff_aces_fail_gamma_twice() {
    let mut strebuf = String::new();
    strebuf.push_str("<?xml version='1.0' encoding='UTF-8'?>\n");
    strebuf.push_str("<ProcessList id='none' version='1.5'>\n");
    strebuf.push_str("    <ACES inBitDepth='16i' outBitDepth='32f' style='Surround'>\n");
    strebuf.push_str("        <ACESParams gamma='1.2' />\n");
    strebuf.push_str("        <ACESParams gamma='1.4' />\n");
    strebuf.push_str("    </ACES>\n");
    strebuf.push_str("</ProcessList>\n");

    check_throw_what(parse_string(&strebuf), "only 1 gamma parameter");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_ff_aces_fail_missing_param)` @ v2.5.2.
#[test]
fn load_ff_aces_fail_missing_param() {
    let mut strebuf = String::new();
    strebuf.push_str("<?xml version='1.0' encoding='UTF-8'?>\n");
    strebuf.push_str("<ProcessList id='none' version='1.5'>\n");
    strebuf.push_str("    <ACES inBitDepth='16i' outBitDepth='32f' style='Surround'>\n");
    strebuf.push_str("    </ACES>\n");
    strebuf.push_str("</ProcessList>\n");

    check_throw_what(parse_string(&strebuf), "must have one parameter");
}

/// The grading primary of a reader's op.
fn as_grading_primary(op: &OpData) -> &GradingPrimaryOpData {
    match op {
        OpData::GradingPrimary(gp) => gp,
        _ => panic!("not a GradingPrimary"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_primary_log)` @ v2.5.2.
#[test]
fn load_grading_primary_log() {
    let ctf_log = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="log">
        <Brightness rgb="0.1 0 0" master="0" />
        <Contrast rgb="1 1.1 1" master="1" />
        <Gamma rgb="1 1 1" master="1.1" />
        <Saturation master="1.1" />
        <Pivot contrast="-0.1" black="0.1" white="1.1" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="logRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="16f" style="log">
        <Brightness rgb="0.1 0 0" master="0" />
        <Gamma rgb="1 1 1" master="1.1" />
        <Saturation master="1.1" />
        <Pivot black="0.1" />
        <Clamp white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="log">
        <Brightness rgb="-0.25 0.5 7.62939453125e-06" master="1" />
        <Contrast rgb="1.25 1.5 -1.25" master="103125e-5" />
        <Gamma rgb="0.75 0.625 1.25" master="0.9e+1" />
        <Pivot contrast="0.75" />
        <Saturation master="1.03125" />
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;

    // Load file.
    let cached_file = parse_string(ctf_log).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 4);
    let grading_primary0 = as_grading_primary(&file_ops[0]);
    assert_eq!(GradingStyle::Log, grading_primary0.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary0.get_direction()
    );
    let vals0 = grading_primary0.get_value();
    assert_eq!(GradingRgbm::new(0.1, 0., 0., 0.), vals0.brightness);
    assert_eq!(GradingRgbm::new(1., 1.1, 1., 1.), vals0.contrast);
    assert_eq!(GradingRgbm::new(1., 1., 1., 1.1), vals0.gamma);
    assert_eq!(1.1, vals0.saturation);
    assert_eq!(-0.1, vals0.pivot);
    assert_eq!(0.1, vals0.pivot_black);
    assert_eq!(1.1, vals0.pivot_white);
    assert_eq!(0., vals0.clamp_black);
    assert_eq!(1., vals0.clamp_white);
    assert!(!grading_primary0.is_dynamic());

    let grading_primary1 = as_grading_primary(&file_ops[1]);
    assert_eq!(GradingStyle::Log, grading_primary1.get_style());
    assert_eq!(
        TransformDirection::Inverse,
        grading_primary1.get_direction()
    );
    let def_log = GradingPrimary::new(GradingStyle::Log);
    assert_eq!(def_log, grading_primary1.get_value());
    assert!(grading_primary1.is_dynamic());

    let grading_primary2 = as_grading_primary(&file_ops[2]);
    assert_eq!(GradingStyle::Log, grading_primary2.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary2.get_direction()
    );
    let vals2 = grading_primary2.get_value();
    assert_eq!(GradingRgbm::new(0.1, 0., 0., 0.), vals2.brightness);
    assert_eq!(def_log.contrast, vals2.contrast);
    assert_eq!(GradingRgbm::new(1., 1., 1., 1.1), vals2.gamma);
    assert_eq!(1.1, vals2.saturation);
    assert_eq!(def_log.pivot, vals2.pivot);
    assert_eq!(0.1, vals2.pivot_black);
    assert_eq!(def_log.pivot_white, vals2.pivot_white);
    assert_eq!(def_log.clamp_black, vals2.clamp_black);
    assert_eq!(1., vals2.clamp_white);
    assert!(!grading_primary2.is_dynamic());

    let grading_primary3 = as_grading_primary(&file_ops[3]);
    assert_eq!(GradingStyle::Log, grading_primary3.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary3.get_direction()
    );
    let vals3 = grading_primary3.get_value();
    assert_eq!(
        GradingRgbm::new(-0.25, 0.5, 7.62939453125e-06, 1.),
        vals3.brightness
    );
    assert_eq!(
        GradingRgbm::new(1.25, 1.5, -1.25, 103125e-5),
        vals3.contrast
    );
    assert_eq!(GradingRgbm::new(0.75, 0.625, 1.25, 0.9e+1), vals3.gamma);
    assert_eq!(1.03125, vals3.saturation);
    assert_eq!(0.75, vals3.pivot);
    assert_eq!(def_log.pivot_black, vals3.pivot_black);
    assert_eq!(def_log.pivot_white, vals3.pivot_white);
    assert_eq!(def_log.clamp_black, vals3.clamp_black);
    assert_eq!(def_log.clamp_white, vals3.clamp_white);
    assert!(grading_primary3.is_dynamic());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_primary_lin)` @ v2.5.2.
#[test]
fn load_grading_primary_lin() {
    let ctf_lin = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="linear">
        <Offset rgb="0.1 0 0" master="0" />
        <Exposure rgb="0 0 0" master="0.1" />
        <Contrast rgb="1 1.1 1" master="1" />
        <Saturation master="1.1" />
        <Pivot contrast="0.28" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="linearRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="16f" style="linear">
        <Offset rgb="0.1 0 0" master="0" />
        <Exposure rgb="0 0 0" master="0.1" />
        <Saturation master="1.1" />
        <Clamp white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="linear">
        <Offset rgb="0.25 -0.015625 +.03125" master="0" />
        <Exposure rgb="-0.25 0.5 7.62939453125e-06" master="1" />
        <Contrast rgb="0.75 0.625 1.25" master=".9e+1" />
        <Pivot contrast="1.25" />
        <Saturation master="1.125" />
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;

    // Load file.
    let cached_file = parse_string(ctf_lin).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 4);
    let grading_primary0 = as_grading_primary(&file_ops[0]);
    assert_eq!(GradingStyle::Lin, grading_primary0.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary0.get_direction()
    );
    let vals0 = grading_primary0.get_value();
    assert_eq!(GradingRgbm::new(0.1, 0., 0., 0.), vals0.offset);
    assert_eq!(GradingRgbm::new(0., 0., 0., 0.1), vals0.exposure);
    assert_eq!(GradingRgbm::new(1., 1.1, 1., 1.), vals0.contrast);
    assert_eq!(1.1, vals0.saturation);
    assert_eq!(0.28, vals0.pivot);
    assert_eq!(0., vals0.clamp_black);
    assert_eq!(1., vals0.clamp_white);
    assert!(!grading_primary0.is_dynamic());

    let grading_primary1 = as_grading_primary(&file_ops[1]);
    assert_eq!(GradingStyle::Lin, grading_primary1.get_style());
    assert_eq!(
        TransformDirection::Inverse,
        grading_primary1.get_direction()
    );
    let def_lin = GradingPrimary::new(GradingStyle::Lin);
    assert_eq!(def_lin, grading_primary1.get_value());
    assert!(grading_primary1.is_dynamic());

    let grading_primary2 = as_grading_primary(&file_ops[2]);
    assert_eq!(GradingStyle::Lin, grading_primary2.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary2.get_direction()
    );
    let vals2 = grading_primary2.get_value();
    assert_eq!(GradingRgbm::new(0.1, 0., 0., 0.), vals0.offset);
    assert_eq!(GradingRgbm::new(0., 0., 0., 0.1), vals0.exposure);
    assert_eq!(def_lin.contrast, vals2.contrast);
    assert_eq!(1.1, vals2.saturation);
    assert_eq!(def_lin.pivot, vals2.pivot);
    assert_eq!(def_lin.clamp_black, vals2.clamp_black);
    assert_eq!(1., vals2.clamp_white);
    assert!(!grading_primary2.is_dynamic());

    let grading_primary3 = as_grading_primary(&file_ops[3]);
    assert_eq!(GradingStyle::Lin, grading_primary3.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary3.get_direction()
    );
    let vals3 = grading_primary3.get_value();
    assert_eq!(GradingRgbm::new(0.25, -0.015625, 0.03125, 0.), vals3.offset);
    assert_eq!(
        GradingRgbm::new(-0.25, 0.5, 7.62939453125e-06, 1.),
        vals3.exposure
    );
    assert_eq!(GradingRgbm::new(0.75, 0.625, 1.25, 0.9e+1), vals3.contrast);
    assert_eq!(1.125, vals3.saturation);
    assert_eq!(1.25, vals3.pivot);
    assert_eq!(def_lin.clamp_black, vals3.clamp_black);
    assert_eq!(def_lin.clamp_white, vals3.clamp_white);
    assert!(grading_primary3.is_dynamic());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_primary_video)` @ v2.5.2.
#[test]
fn load_grading_primary_video() {
    let ctf_video = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="video">
        <Lift rgb="0.1 0 0" master="0" />
        <Gamma rgb="1 1 1" master="1.1" />
        <Gain rgb="1 1.1 1" master="1" />
        <Offset rgb="0 0.1 0" master="0" />
        <Saturation master="1.1" />
        <Pivot black="0.1" white="1.1" />
        <Clamp black="0" white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="32f" style="videoRev">
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="32f" outBitDepth="16f" style="video">
        <Lift rgb="0.1 0 0" master="0" />
        <Gain rgb="1 1.1 1" master="1" />
        <Pivot black="0.1" />
        <Clamp white="1" />
    </GradingPrimary>
    <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
        <Lift rgb="0.25 -.015625 .03125" master="0" />
        <Gamma rgb=".75 .625 1.25" master=".09e2" />
        <Gain rgb="-0.25 00.500 7.62939453125e-06" master="1" />
        <Offset rgb="02.5 +0.5 -.125" master="1" />
        <Pivot black="-0.25" white="12" />
        <Saturation master="1" />
        <Clamp black="0.5" white="1.5" />
        <DynamicParameter param="PRIMARY" />
    </GradingPrimary>
</ProcessList>
"#;

    // Load file.
    let cached_file = parse_string(ctf_video).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 4);
    let grading_primary0 = as_grading_primary(&file_ops[0]);
    assert_eq!(GradingStyle::Video, grading_primary0.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary0.get_direction()
    );
    let vals0 = grading_primary0.get_value();
    assert_eq!(GradingRgbm::new(0.1, 0., 0., 0.), vals0.lift);
    assert_eq!(GradingRgbm::new(1., 1., 1., 1.1), vals0.gamma);
    assert_eq!(GradingRgbm::new(1., 1.1, 1., 1.), vals0.gain);
    assert_eq!(GradingRgbm::new(0., 0.1, 0., 0.), vals0.offset);
    assert_eq!(1.1, vals0.saturation);
    assert_eq!(0.1, vals0.pivot_black);
    assert_eq!(1.1, vals0.pivot_white);
    assert_eq!(0., vals0.clamp_black);
    assert_eq!(1., vals0.clamp_white);
    assert!(!grading_primary0.is_dynamic());

    let grading_primary1 = as_grading_primary(&file_ops[1]);
    assert_eq!(GradingStyle::Video, grading_primary1.get_style());
    assert_eq!(
        TransformDirection::Inverse,
        grading_primary1.get_direction()
    );
    let def_video = GradingPrimary::new(GradingStyle::Video);
    assert_eq!(def_video, grading_primary1.get_value());
    assert!(grading_primary1.is_dynamic());

    let grading_primary2 = as_grading_primary(&file_ops[2]);
    assert_eq!(GradingStyle::Video, grading_primary2.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary2.get_direction()
    );
    let vals2 = grading_primary2.get_value();
    assert_eq!(GradingRgbm::new(0.1, 0., 0., 0.), vals2.lift);
    assert_eq!(def_video.gamma, vals2.gamma);
    assert_eq!(GradingRgbm::new(1., 1.1, 1., 1.), vals2.gain);
    assert_eq!(def_video.offset, vals2.offset);
    assert_eq!(def_video.saturation, vals2.saturation);
    assert_eq!(0.1, vals2.pivot_black);
    assert_eq!(def_video.pivot_white, vals2.pivot_white);
    assert_eq!(def_video.clamp_black, vals2.clamp_black);
    assert_eq!(1., vals2.clamp_white);

    let grading_primary3 = as_grading_primary(&file_ops[3]);
    assert_eq!(GradingStyle::Video, grading_primary3.get_style());
    assert_eq!(
        TransformDirection::Forward,
        grading_primary3.get_direction()
    );
    let vals3 = grading_primary3.get_value();
    assert_eq!(GradingRgbm::new(0.25, -0.015625, 0.03125, 0.), vals3.lift);
    assert_eq!(GradingRgbm::new(0.75, 0.625, 1.25, 0.9e+1), vals3.gamma);
    assert_eq!(
        GradingRgbm::new(-0.25, 0.500, 7.62939453125e-06, 1.),
        vals3.gain
    );
    assert_eq!(GradingRgbm::new(02.5, 0.5, -0.125, 1.), vals3.offset);
    assert_eq!(1., vals3.saturation);
    assert_eq!(-0.25, vals3.pivot_black);
    assert_eq!(12., vals3.pivot_white);
    assert_eq!(0.5, vals3.clamp_black);
    assert_eq!(1.5, vals3.clamp_white);
    assert!(grading_primary3.is_dynamic());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_primary_errors)` @ v2.5.2.
#[test]
fn load_grading_primary_errors() {
    // Wrong version.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.8" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "CTF file version '1.8' does not support operator 'GradingPrimary'",
    );

    // Master attribute for pivot instead of contrast.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <Lift rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="1.0 1.0 1.0" master="1" />
      <Gain rgb="0.25 0.5 0.25" master="1" />
      <Offset rgb="0.25 0.5 0.25" master="1" />
      <Saturation master="1" />
      <Pivot master="1">
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Illegal attribute for 'Pivot': 'master'",
    );

    // Brightness does not have 3 rgb values.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="log">
      <Brightness rgb="0.25 0.5" master="1" />
      <Contrast rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.25 0.5 0.25" master="1" />
      <Pivot contrast="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Illegal number of 'rgb' values for 'Brightness'",
    );

    // Gamma values should be above lower bound.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <Lift rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.0 0.0 0.0" master="1" />
      <Gain rgb="0.25 0.5 0.25" master="1" />
      <Offset rgb="0.25 0.5 0.25" master="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "GradingPrimary gamma '<r=0, g=0, b=0, m=1>' are below lower bound (0.01)",
    );

    // Brightness does not have a master value.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="log">
      <Brightness rgb="0.25 0.5 0.25" />
      <Contrast rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.25 0.5 0.25" master="1" />
      <Pivot contrast="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Missing 'master' attribute for 'Brightness'",
    );

    // Missing style attribute.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
   <GradingPrimary inBitDepth="16f" outBitDepth="32f">
      <Brightness rgb="0.25 0.5 0.25" master=".1" />
      <Contrast rgb="0.25 0.5 0.25" master="1" />
      <Gamma rgb="0.25 0.5 0.25" master="1" />
      <Pivot contrast="1" />
      <Saturation master="1" />
      <DynamicParameter param="PRIMARY" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );

    // Unsupported dynamic parameter for GradingPrimary.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
   <GradingPrimary inBitDepth="16f" outBitDepth="32f" style="video">
      <DynamicParameter param="CONTRAST" />
   </GradingPrimary>
</ProcessList>
"#,
        ),
        "Dynamic parameter 'CONTRAST' is not supported in 'GradingPrimary'",
    );
}

/// The grading RGB curves of a reader's op.
fn as_grading_rgb_curve(op: &OpData) -> &GradingRgbCurveOpData {
    match op {
        OpData::GradingRgbCurve(curves) => curves,
        _ => panic!("not a GradingRGBCurve"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_rgbcurves_lin)` @ v2.5.2.
#[test]
fn load_grading_rgbcurves_lin() {
    let ctf_lin = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDGradingCurves">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                         -7 -6
                          0 0
                          7 7
            </ControlPoints>
        </Red>
        <Master>
            <ControlPoints>
                         -7 -7
                          0 0
                          7 7
                         16 10
            </ControlPoints>
        </Master>
    </GradingRGBCurve>
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear" bypassLinToLog="true">
        <DynamicParameter param="RGB_CURVE" />
    </GradingRGBCurve>
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="video">
        <Master>
            <ControlPoints>
                          0 0 1 1
                        1.5 1.4
            </ControlPoints>
            <Slopes> 0 0.7 1.1 </Slopes>
        </Master>
    </GradingRGBCurve>
</ProcessList>
"#;

    // Load file.
    let cached_file = parse_string(ctf_lin).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 3);
    let grading_curves0 = as_grading_rgb_curve(&file_ops[0]);
    assert_eq!(grading_curves0.get_style(), GradingStyle::Lin);
    assert!(!grading_curves0.get_bypass_lin_to_log());
    assert!(!grading_curves0.is_dynamic());
    let curves = grading_curves0.get_value();
    assert_eq!(
        curves
            .curve(RgbCurveType::Master)
            .unwrap()
            .num_control_points(),
        4
    );
    assert!(*curves.curve(RgbCurveType::Blue).unwrap() == default_lin_curve());
    assert!(*curves.curve(RgbCurveType::Red).unwrap() != default_lin_curve());

    let grading_curves1 = as_grading_rgb_curve(&file_ops[1]);
    assert_eq!(grading_curves1.get_style(), GradingStyle::Lin);
    assert!(grading_curves1.get_bypass_lin_to_log());
    assert!(grading_curves1.is_dynamic());
    let curves = grading_curves1.get_value();
    assert!(*curves.curve(RgbCurveType::Red).unwrap() == default_lin_curve());
    assert!(
        curves
            .curve(RgbCurveType::Red)
            .unwrap()
            .slopes_are_default()
    );

    let grading_curves2 = as_grading_rgb_curve(&file_ops[2]);
    assert_eq!(grading_curves2.get_style(), GradingStyle::Video);
    assert!(!grading_curves2.get_bypass_lin_to_log());
    assert!(!grading_curves2.is_dynamic());
    let curves = grading_curves2.get_value();
    let master = curves.curve(RgbCurveType::Master).unwrap();
    assert_eq!(master.num_control_points(), 3);
    assert_eq!(
        *master.control_point(0).unwrap(),
        GradingControlPoint::new(0.0f32, 0.0f32)
    );
    assert_eq!(
        *master.control_point(1).unwrap(),
        GradingControlPoint::new(1.0f32, 1.0f32)
    );
    assert_eq!(
        *master.control_point(2).unwrap(),
        GradingControlPoint::new(1.5f32, 1.4f32)
    );
    assert!(!master.slopes_are_default());
    assert_eq!(master.slope(0).unwrap(), 0.0f32);
    assert_eq!(master.slope(1).unwrap(), 0.7f32);
    assert_eq!(master.slope(2).unwrap(), 1.1f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_rgbcurves_log)` @ v2.5.2.
#[test]
fn load_grading_rgbcurves_log() {
    let ctf_log = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
   <GradingRGBCurve inBitDepth="16f" outBitDepth="32f" style="logRev">
      <Red>
         <ControlPoints>
            0.015625 0
            0.5 0.5
            2 2
         </ControlPoints>
      </Red>
      <Green>
         <ControlPoints>
            0.015625 0.1
            2.5 0.5
            3.5 1.5
         </ControlPoints>
      </Green>
      <Blue>
         <ControlPoints>
            -4 -4
            4.5 0.5
            5 3
         </ControlPoints>
      </Blue>
      <Master>
         <ControlPoints>
            11 11 12.5 11.5 13.5 12.5 26.5 15
         </ControlPoints>
      </Master>
      <DynamicParameter param="RGB_CURVE" />
   </GradingRGBCurve>
</ProcessList>"#;

    // Load file.
    let cached_file = parse_string(ctf_log).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let grading_curves0 = as_grading_rgb_curve(&file_ops[0]);
    assert_eq!(grading_curves0.get_style(), GradingStyle::Log);
    assert_eq!(grading_curves0.get_direction(), TransformDirection::Inverse);
    assert!(!grading_curves0.get_bypass_lin_to_log());
    assert!(grading_curves0.is_dynamic());
    let curves = grading_curves0.get_value();
    let red = curves.curve(RgbCurveType::Red).unwrap();
    assert_eq!(red.num_control_points(), 3);
    assert_eq!(red.control_point(0).unwrap().x, 0.015625f32);
    assert_eq!(red.control_point(0).unwrap().y, 0.0f32);
    assert_eq!(red.control_point(1).unwrap().x, 0.5f32);
    assert_eq!(red.control_point(1).unwrap().y, 0.5f32);
    assert_eq!(red.control_point(2).unwrap().x, 2.0f32);
    assert_eq!(red.control_point(2).unwrap().y, 2.0f32);

    let master = curves.curve(RgbCurveType::Master).unwrap();
    assert_eq!(master.num_control_points(), 4);
    assert_eq!(master.control_point(0).unwrap().x, 11.0f32);
    assert_eq!(master.control_point(0).unwrap().y, 11.0f32);
    assert_eq!(master.control_point(1).unwrap().x, 12.5f32);
    assert_eq!(master.control_point(1).unwrap().y, 11.5f32);
    assert_eq!(master.control_point(2).unwrap().x, 13.5f32);
    assert_eq!(master.control_point(2).unwrap().y, 12.5f32);
    assert_eq!(master.control_point(3).unwrap().x, 26.5f32);
    assert_eq!(master.control_point(3).unwrap().y, 15.0f32);
}

/// The grading hue curves of a reader's op.
fn as_grading_hue_curve(op: &OpData) -> &GradingHueCurveOpData {
    match op {
        OpData::GradingHueCurve(curves) => curves,
        _ => panic!("not a GradingHueCurve"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_huecurves_log)` @ v2.5.2.
#[test]
fn load_grading_huecurves_log() {
    let ctf_log = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="empty">
   <GradingHueCurve inBitDepth="16f" outBitDepth="32f" style="logRev" hsyTransform="none">
      <HueHue>
         <ControlPoints>
            0.015625 0
            0.5 0.6
            0.9 0.8
         </ControlPoints>
      </HueHue>
      <HueSat>
         <ControlPoints>
            0.015625 1
            0.5 0.5
            0.9 1.5
         </ControlPoints>
      </HueSat>
      <HueLum>
         <ControlPoints>
            0.1, 1.5, 0.2, 0.7, 0.4, 1.4, 0.5, 0.8, 0.8, 0.5
         </ControlPoints>
      </HueLum>
      <LumSat>
         <ControlPoints>
           -0.1  1.0
            0.5  1.5
            1.0  0.9
            1.1  1.2
         </ControlPoints>
      </LumSat>
      <SatSat>
         <ControlPoints>
            0., 0.1, 0.5, 0.45, 1., 1.1
         </ControlPoints>
      </SatSat>
      <LumLum>
         <ControlPoints>
            0.  -0.0005
            0.5  0.3
            1.  0.9
         </ControlPoints>
      </LumLum>
      <SatLum>
         <ControlPoints>
            0., 1.2, 0.6, 0.8, 0.9, 1.1
         </ControlPoints>
      </SatLum>
      <HueFx>
         <ControlPoints>
            0.2, 0.05, .4, -0.09, .6, -0.2, .8, 0.05, 0.99, -0.02
         </ControlPoints>
      </HueFx>
      <DynamicParameter param="HUE_CURVE" />
   </GradingHueCurve>
</ProcessList>"#;

    // Load file.
    let cached_file = parse_string(ctf_log).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let grading_curves0 = as_grading_hue_curve(&file_ops[0]);
    assert_eq!(grading_curves0.get_style(), GradingStyle::Log);
    assert_eq!(grading_curves0.get_direction(), TransformDirection::Inverse);
    assert_eq!(grading_curves0.get_rgb_to_hsy(), HsyTransformStyle::None);
    assert!(grading_curves0.is_dynamic());
    let curves = grading_curves0.get_value();
    let hh = curves.curve(HueCurveType::HueHue).unwrap();
    assert_eq!(hh.num_control_points(), 3);
    assert_eq!(hh.control_point(0).unwrap().x, 0.015625f32);
    assert_eq!(hh.control_point(0).unwrap().y, 0.0f32);
    assert_eq!(hh.control_point(1).unwrap().x, 0.5f32);
    assert_eq!(hh.control_point(1).unwrap().y, 0.6f32);
    assert_eq!(hh.control_point(2).unwrap().x, 0.9f32);
    assert_eq!(hh.control_point(2).unwrap().y, 0.8f32);

    let ls = curves.curve(HueCurveType::LumSat).unwrap();
    assert_eq!(ls.num_control_points(), 4);
    assert_eq!(ls.control_point(0).unwrap().x, -0.1f32);
    assert_eq!(ls.control_point(0).unwrap().y, 1.0f32);
    assert_eq!(ls.control_point(1).unwrap().x, 0.5f32);
    assert_eq!(ls.control_point(1).unwrap().y, 1.5f32);
    assert_eq!(ls.control_point(2).unwrap().x, 1.0f32);
    assert_eq!(ls.control_point(2).unwrap().y, 0.9f32);
    assert_eq!(ls.control_point(3).unwrap().x, 1.1f32);
    assert_eq!(ls.control_point(3).unwrap().y, 1.2f32);

    let sl = curves.curve(HueCurveType::SatLum).unwrap();
    assert_eq!(sl.num_control_points(), 3);
    assert_eq!(sl.control_point(0).unwrap().x, 0.0f32);
    assert_eq!(sl.control_point(0).unwrap().y, 1.2f32);
    assert_eq!(sl.control_point(1).unwrap().x, 0.6f32);
    assert_eq!(sl.control_point(1).unwrap().y, 0.8f32);
    assert_eq!(sl.control_point(2).unwrap().x, 0.9f32);
    assert_eq!(sl.control_point(2).unwrap().y, 1.1f32);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, load_grading_curves_errors)` @ v2.5.2.
#[test]
fn load_grading_curves_errors() {
    // Wrong version.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.8" id="empty">
  <GradingRGBCurve inBitDepth="16f" outBitDepth="32f" style="video">
      <DynamicParameter param="RGB_CURVE" />
   </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "CTF file version '1.8' does not support operator 'GradingRGBCurve'",
    );

    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.0" id="empty">
  <GradingHueCurve inBitDepth="16f" outBitDepth="32f" style="video">
  </GradingHueCurve>
</ProcessList>
"#,
        ),
        "CTF file version '2' does not support operator 'GradingHueCurve'",
    );

    // Missing style attribute.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingRGBCurve inBitDepth="16f" outBitDepth="32f">
      <DynamicParameter param="RGB_CURVE" />
   </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );

    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="empty">
  <GradingHueCurve inBitDepth="16f" outBitDepth="32f">
  </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );

    // Wrong dynamic property param.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
  <GradingRGBCurve inBitDepth="16f" outBitDepth="32f">
      <DynamicParameter param="PRIMARY" />
   </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Required attribute 'style' is missing",
    );

    // Odd number of values for control points.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                         -7 -6 0 0 7
            </ControlPoints>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Control points element: odd number of values",
    );

    // Not enough control points.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                         0 1
            </ControlPoints>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "There must be at least 2 control points",
    );

    // Control points don't have increasing x.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                 -7 -6 0 0 -1 7
            </ControlPoints>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Control point at index 2 has a x coordinate '-1' that is less than previous control \
         point x coordinate '0'",
    );

    // Number of slopes matches control points.
    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Red>
            <ControlPoints>
                 -7 -6 0 0 1 7
            </ControlPoints>
            <Slopes> 1 1 1 1 </Slopes>
        </Red>
    </GradingRGBCurve>
</ProcessList>
"#,
        ),
        "Number of slopes must match number of control points",
    );

    // Wrong curve: warning is logged.
    let (result, parts) = warning_lines(|| {
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="empty">
    <GradingRGBCurve inBitDepth="32f" outBitDepth="32f" style="linear">
        <Grn>
            <ControlPoints>
                         0 1 1 2 2 3
            </ControlPoints>
        </Grn>
    </GradingRGBCurve>
</ProcessList>
"#,
        )
    });
    assert!(result.is_ok());

    assert_eq!(parts.len(), 2);
    assert!(parts[0].contains("Unrecognized element 'Grn'"));
    assert!(parts[1].contains("Unrecognized element 'ControlPoints'"));

    check_throw_what(
        parse_string(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.5" id="UIDGradingCurves">
    <GradingHueCurve inBitDepth="32f" outBitDepth="32f" style="linear" hsyTransform="hsy1">
        <DynamicParameter param="HUE_CURVE" />
    </GradingHueCurve>
</ProcessList>
"#,
        ),
        "Unknown hsyTransform value: 'hsy1'",
    );
}

/// The grading tone of a reader's op.
fn as_grading_tone(op: &OpData) -> &GradingToneOpData {
    match op {
        OpData::GradingTone(gt) => gt,
        _ => panic!("not a GradingTone"),
    }
}

/// Port of `OCIO_ADD_TEST(CTFTransform, load_grading_tone)` @ v2.5.2.
#[test]
fn load_grading_tone() {
    let ctf_log = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDEC42">
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="log">
        <Blacks rgb="1.1 1 1" master="1" start="0" width="0.98765432198" />
        <Shadows rgb="1 1.1 1" master="1" start="1" pivot="0" />
        <Midtones rgb="1 1 1" master="1.1" center="0" width="1" />
        <Highlights rgb="1 1 1" master="1.1" start="0" pivot="1" />
        <Whites rgb="1 1 1.1" master="1" start="0" width="1" />
        <SContrast master="1.1" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="logRev">
        <DynamicParameter param="TONE" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="linear">
        <Blacks rgb="1.1 1 1" master="1" start="0" width="1" />
        <Shadows rgb="1 1.1 1" master="1" start="1" pivot="0" />
        <Whites rgb="1 1 1.1" master="1" start="0" width="1" />
        <SContrast master="1.1" />
        <DynamicParameter param="TONE" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="linearRev">
        <SContrast master="1.12345678912" />
    </GradingTone>
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="video" />
    <GradingTone inBitDepth="32f" outBitDepth="32f" style="videoRev">
        <Shadows rgb="1 1 1.12345678912" master="1" start="0.6" pivot="0" />
    </GradingTone>
</ProcessList>
"#;

    // Load file.
    let cached_file = parse_string(ctf_log).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 6);

    let grading_tone0 = as_grading_tone(&file_ops[0]);
    assert_eq!(GradingStyle::Log, grading_tone0.get_style());
    assert_eq!(TransformDirection::Forward, grading_tone0.get_direction());
    let vals0 = grading_tone0.get_value();
    assert_eq!(
        GradingRgbmsw::new(1.1, 1., 1., 1., 0., 0.98765432198),
        vals0.blacks
    );
    assert_eq!(GradingRgbmsw::new(1., 1.1, 1., 1., 1., 0.), vals0.shadows);
    assert_eq!(GradingRgbmsw::new(1., 1., 1., 1.1, 0., 1.), vals0.midtones);
    assert_eq!(
        GradingRgbmsw::new(1., 1., 1., 1.1, 0., 1.),
        vals0.highlights
    );
    assert_eq!(GradingRgbmsw::new(1., 1., 1.1, 1., 0., 1.), vals0.whites);
    assert_eq!(1.1, vals0.scontrast);
    assert!(!grading_tone0.is_dynamic());

    let grading_tone1 = as_grading_tone(&file_ops[1]);
    assert_eq!(GradingStyle::Log, grading_tone1.get_style());
    assert_eq!(TransformDirection::Inverse, grading_tone1.get_direction());
    let vals1 = grading_tone1.get_value();
    let def_vals_log = GradingTone::new(grading_tone1.get_style());
    assert_eq!(def_vals_log, vals1);
    assert!(grading_tone1.is_dynamic());

    let grading_tone2 = as_grading_tone(&file_ops[2]);
    assert_eq!(GradingStyle::Lin, grading_tone2.get_style());
    assert_eq!(TransformDirection::Forward, grading_tone2.get_direction());
    let vals2 = grading_tone2.get_value();
    assert_eq!(GradingRgbmsw::new(1.1, 1., 1., 1., 0., 1.), vals2.blacks);
    assert_eq!(GradingRgbmsw::new(1., 1.1, 1., 1., 1., 0.), vals2.shadows);
    let def_vals_lin = GradingTone::new(grading_tone2.get_style());
    assert_eq!(def_vals_lin.midtones, vals2.midtones);
    assert_eq!(def_vals_lin.highlights, vals2.highlights);
    assert_eq!(GradingRgbmsw::new(1., 1., 1.1, 1., 0., 1.), vals2.whites);
    assert_eq!(1.1, vals2.scontrast);
    assert!(grading_tone2.is_dynamic());

    let grading_tone3 = as_grading_tone(&file_ops[3]);
    assert_eq!(GradingStyle::Lin, grading_tone3.get_style());
    assert_eq!(TransformDirection::Inverse, grading_tone3.get_direction());
    let vals3 = grading_tone3.get_value();
    let mut expected3 = GradingTone::new(grading_tone3.get_style());
    expected3.scontrast = 1.12345678912;
    assert_eq!(expected3, vals3);
    assert!(!grading_tone3.is_dynamic());

    let grading_tone4 = as_grading_tone(&file_ops[4]);
    assert_eq!(GradingStyle::Video, grading_tone4.get_style());
    assert_eq!(TransformDirection::Forward, grading_tone4.get_direction());
    let vals4 = grading_tone4.get_value();
    let expected4 = GradingTone::new(grading_tone4.get_style());
    assert_eq!(expected4, vals4);
    assert!(!grading_tone4.is_dynamic());

    let grading_tone5 = as_grading_tone(&file_ops[5]);
    assert_eq!(GradingStyle::Video, grading_tone5.get_style());
    assert_eq!(TransformDirection::Inverse, grading_tone5.get_direction());
    let vals5 = grading_tone5.get_value();
    let mut expected5 = GradingTone::new(grading_tone5.get_style());
    expected5.shadows.blue = 1.12345678912;
    assert_eq!(expected5, vals5);
    assert!(!grading_tone5.is_dynamic());
}

/// Port of `OCIO_ADD_TEST(CTFTransform, load_grading_tone_errors)` @ v2.5.2. Upstream's test
/// is empty (`// TODO`).
#[test]
fn load_grading_tone_errors() {
    // TODO
}

/// The reference of a reader's op.
fn as_reference(op: &OpData) -> &ReferenceOpData {
    match op {
        OpData::Reference(reference) => reference,
        _ => panic!("not a Reference"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, reference_load_alias)` @ v2.5.2.
#[test]
fn reference_load_alias() {
    let file_name = "reference_alias.ctf";
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let op = &file_ops[0];
    let reference = as_reference(op);
    assert_eq!(op.get_name(), b"name");
    assert_eq!(op.get_id(), b"uuid");
    assert_eq!(reference.get_reference_style(), ReferenceStyle::Alias);
    assert_eq!(reference.get_path(), b"");
    assert_eq!(reference.get_alias(), b"alias");
    assert_eq!(reference.get_direction(), TransformDirection::Forward);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, reference_load_path)` @ v2.5.2.
#[test]
fn reference_load_path() {
    let file_name = "reference_path_missing_file.ctf";
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let reference = as_reference(&file_ops[0]);
    assert_eq!(reference.get_reference_style(), ReferenceStyle::Path);
    assert_eq!(reference.get_path(), b"toto/toto.ctf");
    assert_eq!(reference.get_alias(), b"");
    assert_eq!(reference.get_direction(), TransformDirection::Inverse);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, reference_load_multiple)` @ v2.5.2.
#[test]
fn reference_load_multiple() {
    // File contains 2 references, 1 range and 1 reference.
    let file_name = "references_some_inverted.ctf";
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 4);
    let ref0 = as_reference(&file_ops[0]);
    assert_eq!(ref0.get_reference_style(), ReferenceStyle::Path);
    assert_eq!(ref0.get_path(), b"matrix_example_1_3_offsets.ctf");
    assert_eq!(ref0.get_direction(), TransformDirection::Forward);

    let ref1 = as_reference(&file_ops[1]);
    assert_eq!(ref1.get_reference_style(), ReferenceStyle::Path);
    assert_eq!(ref1.get_path(), b"clf/xyz_to_rgb.clf");
    assert_eq!(ref1.get_direction(), TransformDirection::Inverse);

    as_range(&file_ops[2]);

    let ref3 = as_reference(&file_ops[3]);
    assert_eq!(ref3.get_reference_style(), ReferenceStyle::Path);
    assert_eq!(ref3.get_path(), b"clf/cdl_clamp_fwd.clf");
    // Note: This tests that the "inverted" attribute set to anything other than true does not
    // result in an inverted transform.
    assert_eq!(ref3.get_direction(), TransformDirection::Forward);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, reference_load_path_utf8)` @ v2.5.2.
#[test]
fn reference_load_path_utf8() {
    let file_name = "reference_utf8.ctf";
    let cached_file = load_clf_file(file_name).unwrap();
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let reference = as_reference(&file_ops[0]);
    assert_eq!(reference.get_reference_style(), ReferenceStyle::Path);
    assert_eq!(
        reference.get_path(),
        b"\xE6\xA8\x99\xE6\xBA\x96\xE8\x90\xAC\xE5\x9C\x8B\xE7\xA2\xBC"
    );
    assert_eq!(reference.get_alias(), b"");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, reference_load_alias_path)` @ v2.5.2.
#[test]
fn reference_load_alias_path() {
    let file_name = "reference_alias_path.ctf";
    // Can't have alias and path at the same time.
    check_throw_what(
        load_clf_file(file_name),
        "alias & path attributes for Reference should not be both defined",
    );
}

/// The exposure and contrast of a reader's op.
fn as_exposure_contrast(op: &OpData) -> &ExposureContrastOpData {
    match op {
        OpData::ExposureContrast(ec) => ec,
        _ => panic!("not an ExposureContrast"),
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_video)` @ v2.5.2.
#[test]
fn exposure_contrast_video() {
    let ctf_file = "exposure_contrast_video.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();

    assert_eq!(op_list.len(), 2);

    let p_ec = as_exposure_contrast(&op_list[0]);

    assert_eq!(p_ec.get_style(), ExposureContrastOpStyle::Video);

    assert_eq!(p_ec.get_exposure(), -1.0);
    assert_eq!(p_ec.get_contrast(), 1.5);
    assert_eq!(p_ec.get_pivot(), 0.5);

    assert!(p_ec.is_dynamic());
    assert!(p_ec.get_exposure_property().is_dynamic());
    assert!(p_ec.get_contrast_property().is_dynamic());
    assert!(!p_ec.get_gamma_property().is_dynamic());

    let p_ec_rev = as_exposure_contrast(&op_list[1]);
    assert!(!p_ec_rev.is_dynamic());

    assert_eq!(p_ec_rev.get_style(), ExposureContrastOpStyle::VideoRev);
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_log)` @ v2.5.2.
#[test]
fn exposure_contrast_log() {
    let ctf_file = "exposure_contrast_log.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();

    assert_eq!(op_list.len(), 2);

    let p_ec = as_exposure_contrast(&op_list[0]);

    assert_eq!(p_ec.get_style(), ExposureContrastOpStyle::Logarithmic);

    assert_eq!(p_ec.get_exposure(), -1.5);
    assert_eq!(p_ec.get_contrast(), 0.5);
    assert_eq!(p_ec.get_gamma(), 1.2);
    assert_eq!(p_ec.get_pivot(), 0.18);

    assert!(p_ec.is_dynamic());
    assert!(p_ec.get_exposure_property().is_dynamic());
    assert!(p_ec.get_contrast_property().is_dynamic());
    assert!(!p_ec.get_gamma_property().is_dynamic());

    let p_ec_rev = as_exposure_contrast(&op_list[1]);

    assert_eq!(
        p_ec_rev.get_style(),
        ExposureContrastOpStyle::LogarithmicRev
    );
    assert!(p_ec_rev.is_dynamic());
    assert!(!p_ec_rev.get_exposure_property().is_dynamic());
    assert!(!p_ec_rev.get_contrast_property().is_dynamic());
    assert!(p_ec_rev.get_gamma_property().is_dynamic());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_linear)` @ v2.5.2.
#[test]
fn exposure_contrast_linear() {
    let ctf_file = "exposure_contrast_linear.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();

    assert_eq!(op_list.len(), 2);

    let p_ec = as_exposure_contrast(&op_list[0]);

    assert_eq!(p_ec.get_style(), ExposureContrastOpStyle::Linear);

    assert_eq!(p_ec.get_exposure(), 0.65);
    assert_eq!(p_ec.get_contrast(), 1.2);
    assert_eq!(p_ec.get_gamma(), 0.5);
    assert_eq!(p_ec.get_pivot(), 1.0);

    assert!(p_ec.is_dynamic());
    assert!(p_ec.get_exposure_property().is_dynamic());
    assert!(p_ec.get_contrast_property().is_dynamic());
    assert!(p_ec.get_gamma_property().is_dynamic());

    let p_ec_rev = as_exposure_contrast(&op_list[1]);

    assert_eq!(p_ec_rev.get_style(), ExposureContrastOpStyle::LinearRev);
    assert!(!p_ec_rev.is_dynamic());
    assert!(!p_ec_rev.get_exposure_property().is_dynamic());
    assert!(!p_ec_rev.get_contrast_property().is_dynamic());
    assert!(!p_ec_rev.get_gamma_property().is_dynamic());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_no_gamma)` @ v2.5.2.
#[test]
fn exposure_contrast_no_gamma() {
    let ctf_file = "exposure_contrast_no_gamma.ctf";
    let cached_file = load_clf_file(ctf_file).unwrap();
    let op_list = cached_file.transform.get_ops();

    assert_eq!(op_list.len(), 1);

    let p_ec = as_exposure_contrast(&op_list[0]);

    assert_eq!(p_ec.get_style(), ExposureContrastOpStyle::Video);

    assert_eq!(p_ec.get_exposure(), 0.2);
    assert_eq!(p_ec.get_contrast(), 0.65);
    assert_eq!(p_ec.get_pivot(), 0.23);

    assert_eq!(p_ec.get_gamma(), 1.0);

    assert!(!p_ec.is_dynamic());
    assert!(!p_ec.get_exposure_property().is_dynamic());
    assert!(!p_ec.get_contrast_property().is_dynamic());
    assert!(!p_ec.get_gamma_property().is_dynamic());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, exposure_contrast_failures)` @ v2.5.2.
#[test]
fn exposure_contrast_failures() {
    let ec_bad_style = "exposure_contrast_bad_style.ctf";
    check_throw_what(
        load_clf_file(ec_bad_style),
        "Unknown exposure contrast style",
    );

    let ec_missing_param = "exposure_contrast_missing_param.ctf";
    check_throw_what(load_clf_file(ec_missing_param), "exposure missing");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, attribute_float_parse_extra_values)` @ v2.5.2.
#[test]
fn attribute_float_parse_extra_values() {
    // Test attribute float parsing will throw if extra values are present (using E/C for
    // this test).
    let ctf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="empty" version="1.7">
   <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="log">
      <ECParams exposure="-1.5 1.2" contrast="0.5" gamma="1.2" pivot="0.18" />
   </ExposureContrast>
</ProcessList>
"#;

    // Load file.
    check_throw_what(parse_string(ctf), "Expecting 1 value, found 2 values");
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, attribute_float_parse_leading_spaces)` @ v2.5.2.
#[test]
fn attribute_float_parse_leading_spaces() {
    // Test attribute float parsing will not fail if extra leading white space is present
    // (using E/C for this test).
    let ctf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList id="empty" version="1.7">
   <ExposureContrast inBitDepth="32f" outBitDepth="32f" style="log">
      <ECParams exposure="    -1.5 " contrast="0.5" gamma="1.2" pivot="0.18" />
   </ExposureContrast>
</ProcessList>
"#;

    // Load file.
    let cached_file = parse_string(ctf).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let ec = as_exposure_contrast(&file_ops[0]);

    assert_eq!(ec.get_exposure(), -1.5);
}

/// The processor of a file transform of the test file `file_name`, from an empty config.
///
/// Port of `CreateFileTransform` and `GetFileTransformProcessor` (tests/cpu/UnitTestUtils.cpp:
/// 38-72 @ v2.5.2).
fn get_file_transform_processor(file_name: &str) -> Result<std::sync::Arc<Processor>> {
    let file_path = format!("{}/{file_name}", get_test_files_dir());

    // Create a FileTransform
    let mut file_transform = FileTransform::new();
    file_transform.set_src(file_path);

    // Create empty Config to use.
    let config = Config::new()?;
    // Get the processor corresponding to the transform.
    config.processor(&Transform::File(file_transform))
}

/// Port of `WriteGroupCTF` (FileFormatCTF_tests.cpp:3795-3799 @ v2.5.2).
fn write_group_ctf(group: &GroupTransform, output_transform: &mut Vec<u8>) -> Result<()> {
    let cfg = Config::create_raw()?;
    group.write(&cfg, FILEFORMAT_CTF, output_transform)
}

/// Port of `WriteGroupCLF` (FileFormatCTF_tests.cpp:3801-3805 @ v2.5.2).
fn write_group_clf(group: &GroupTransform, output_transform: &mut Vec<u8>) -> Result<()> {
    let cfg = Config::create_raw()?;
    group.write(&cfg, FILEFORMAT_CLF, output_transform)
}

/// The group's transform `index`, a matrix.
fn matrix_transform_mut(group: &mut GroupTransform, index: i32) -> &mut MatrixTransform {
    match group.transform_mut(index).unwrap() {
        Transform::Matrix(mat) => mat,
        other => panic!("not a matrix: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(CTFTransform, load_edit_save_matrix)` @ v2.5.2.
#[test]
fn load_edit_save_matrix() {
    let ctf_file = "clf/matrix_example.clf";
    let processor = get_file_transform_processor(ctf_file).unwrap();
    let mut group = processor.create_group_transform().unwrap();

    group
        .format_metadata_mut()
        .add_attribute(
            Some(crate::fileformats::ctf::ctf_reader_utils::ATTR_INVERSE_OF),
            Some(b"added inverseOf"),
        )
        .unwrap();
    group
        .format_metadata_mut()
        .add_attribute(Some(b"Unknown"), Some(b"not saved"))
        .unwrap();
    group
        .format_metadata_mut()
        .add_child_element(Some(b"Unknown"), Some(b"not saved"))
        .unwrap();
    group
        .format_metadata_mut()
        .add_child_element(Some(METADATA_INFO), Some(b"Preserved"))
        .unwrap();
    let index_info = group.format_metadata().get_num_children_elements() - 1;
    let info = group
        .format_metadata_mut()
        .get_child_element_mut(index_info)
        .unwrap();
    info.add_attribute(Some(b"attrib"), Some(b"value")).unwrap();
    info.add_child_element(Some(b"Child"), Some(b"Preserved"))
        .unwrap();

    assert_eq!(group.num_transforms(), 1);
    let mat_trans = matrix_transform_mut(&mut group, 0);

    // Validate how escape characters are saved.
    let short_name: &[u8] = br#"A ' short ' " name"#;
    let description1: &[u8] = br#"A " short " description with a ' inside"#;
    let description2: &[u8] = br#"<test"'&>"#;
    mat_trans
        .format_metadata_mut()
        .add_child_element(Some(METADATA_DESCRIPTION), Some(description1))
        .unwrap();
    let index_desc = mat_trans.format_metadata().get_num_children_elements() - 1;
    let desc = mat_trans
        .format_metadata_mut()
        .get_child_element_mut(index_desc)
        .unwrap();
    desc.add_attribute(Some(b"Unknown"), Some(b"not saved"))
        .unwrap();
    mat_trans
        .format_metadata_mut()
        .add_child_element(Some(METADATA_DESCRIPTION), Some(description2))
        .unwrap();

    mat_trans.format_metadata_mut().set_name(Some(short_name));

    let offset = [0.1, 1.2, 2.3456789123456, 0.0];
    mat_trans.set_offset(&offset);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Output matrix array as '3 4 3'.
    let expected_ctf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="matrix example" inverseOf="added inverseOf">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Info attrib="value">
    Preserved
        <Child>Preserved</Child>
    </Info>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" name="A &apos; short &apos; &quot; name" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Description>A &quot; short &quot; description with a &apos; inside</Description>
        <Description>&lt;test&quot;&apos;&amp;&gt;</Description>
        <Array dim="3 4 3">
          0.4123908          0.35758434          0.18048079                 0.1
         0.21263901          0.71516868          0.07219232                 1.2
         0.01933082          0.01191948          0.95053215     2.3456789123456
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected_ctf.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected_ctf,
        std::str::from_utf8(&output_transform).unwrap(),
    );

    // Read the stream back.
    let mut input_transform = InputStream::from_bytes(output_transform.clone());

    let cached_file = LocalFileFormat
        .read_file(&mut input_transform, b"")
        .unwrap();

    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let op = &file_ops[0];
    as_matrix(op);
    let md = op.get_format_metadata();
    assert_eq!(md.get_num_attributes(), 2);
    assert_eq!(METADATA_ID, md.get_attribute_name(0));
    assert_eq!(METADATA_NAME, md.get_attribute_name(1));
    assert_eq!(short_name, md.get_attribute_value(1));
    assert_eq!(md.get_num_children_elements(), 4);
    let desc0 = md.get_child_element(0).unwrap();
    assert_eq!(METADATA_DESCRIPTION, desc0.get_element_name());
    assert_eq!(b"Legacy matrix", desc0.get_element_value());
    let desc1 = md.get_child_element(2).unwrap();
    assert_eq!(METADATA_DESCRIPTION, desc1.get_element_name());
    assert_eq!(description1, desc1.get_element_value());
    let desc2 = md.get_child_element(3).unwrap();
    assert_eq!(METADATA_DESCRIPTION, desc2.get_element_name());
    assert_eq!(description2, desc2.get_element_value());
}

/// Writes `transform` as the one transform of a group, and reads it back.
///
/// Port of `WriteRead` (FileFormatCTF_tests.cpp:5040-5056 @ v2.5.2).
fn write_read(transform: Transform) -> LocalCachedFile {
    try_write_read(transform).unwrap()
}

/// [`write_read`], or the error of writing or reading.
fn try_write_read(transform: Transform) -> Result<LocalCachedFile> {
    let mut group = GroupTransform::new();
    group.append_transform(transform);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform)?;

    let mut input_transform = InputStream::from_bytes(output_transform.clone());

    LocalFileFormat.read_file(&mut input_transform, b"")
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_matrix)` @ v2.5.2.
#[test]
fn save_matrix() {
    let mut mat_transform = MatrixTransform::new();
    let offset4 = [0.123456789123, 0.11, 0.111, 0.2];
    mat_transform.set_offset(&offset4);
    mat_transform.set_direction(TransformDirection::Forward);

    let cached_file = write_read(Transform::Matrix(mat_transform));
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let mat = as_matrix(&file_ops[0]);
    assert_eq!(mat.get_offset_value(0).unwrap(), offset4[0]);
    assert_eq!(mat.get_offset_value(1).unwrap(), offset4[1]);
    assert_eq!(mat.get_offset_value(2).unwrap(), offset4[2]);
    assert_eq!(mat.get_offset_value(3).unwrap(), offset4[3]);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, load_save_matrix)` @ v2.5.2.
#[test]
fn load_save_matrix() {
    let ctf_file = "clf/matrix_example.clf";
    let processor = get_file_transform_processor(ctf_file).unwrap();
    let group = processor.create_group_transform().unwrap();

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Output matrix array as '3 3 3'.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="matrix example">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Array dim="3 3 3">
          0.4123908          0.35758434          0.18048079
         0.21263901          0.71516868          0.07219232
         0.01933082          0.01191948          0.95053215
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected,
        std::str::from_utf8(&output_transform).unwrap(),
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_matrix_444)` @ v2.5.2.
#[test]
fn save_matrix_444() {
    let config = Config::new().unwrap();

    let mut mat = MatrixTransform::new();
    let m = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0.5, 0.5, 0., 1.,
    ];
    mat.set_matrix(&m);
    let processor = config.processor(&Transform::Matrix(mat)).unwrap();

    let group = processor.create_group_transform().unwrap();

    assert_eq!(group.num_transforms(), 1);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Output matrix array as '4 4 4'.
    let output = String::from_utf8_lossy(&output_transform);
    assert!(output.contains("\"4 4 4\""));
}

/// Port of `OCIO_ADD_TEST(CTFTransform, load_edit_save_matrix_clf)` @ v2.5.2.
#[test]
fn load_edit_save_matrix_clf() {
    let ctf_file = "clf/matrix_example.clf";
    let processor = get_file_transform_processor(ctf_file).unwrap();
    let mut group = processor.create_group_transform().unwrap();
    assert_eq!(group.num_transforms(), 1);
    let mat_trans = matrix_transform_mut(&mut group, 0);
    let new_description: &[u8] = b"Added description";
    mat_trans
        .format_metadata_mut()
        .add_child_element(Some(METADATA_DESCRIPTION), Some(new_description))
        .unwrap();
    let offset = [0.1, 1.2, 2.3, 0.0];
    mat_trans.set_offset(&offset);

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="matrix example">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Description>Added description</Description>
        <Array dim="3 4">
          0.4123908          0.35758434          0.18048079                 0.1
         0.21263901          0.71516868          0.07219232                 1.2
         0.01933082          0.01191948          0.95053215                 2.3
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected_clf.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected_clf,
        std::str::from_utf8(&output_transform).unwrap(),
    );

    let offset_alpha = [0.1, 1.2, 2.3, 0.9];
    matrix_transform_mut(&mut group, 0).set_offset(&offset_alpha);

    let mut output_transform_ctf = Vec::new();
    write_group_ctf(&group, &mut output_transform_ctf).unwrap();

    let expected_ctf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="b5cc7aed-d405-4d8b-b64b-382b2341a378" name="matrix example">
    <Description>Basic matrix example using CLF v2 dim syntax</Description>
    <InputDescriptor>RGB</InputDescriptor>
    <OutputDescriptor>XYZ</OutputDescriptor>
    <Matrix id="c61daf06-539f-4254-81fc-9800e6d02a37" inBitDepth="32f" outBitDepth="32f">
        <Description>Legacy matrix</Description>
        <Description>Note that dim=&quot;3 3 3&quot; should be supported for CLF v2 compatibility</Description>
        <Description>Added description</Description>
        <Array dim="4 5 4">
          0.4123908          0.35758434          0.18048079                   0                 0.1
         0.21263901          0.71516868          0.07219232                   0                 1.2
         0.01933082          0.01191948          0.95053215                   0                 2.3
                  0                   0                   0                   1                 0.9
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected_ctf.len(), output_transform_ctf.len());
    assert_text_eq(
        "written",
        expected_ctf,
        std::str::from_utf8(&output_transform_ctf).unwrap(),
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, matrix3x3_clf)` @ v2.5.2.
#[test]
fn matrix3x3_clf() {
    let mut mat = MatrixTransform::new();
    mat.set_file_input_bit_depth(BitDepth::Uint10);
    mat.set_file_output_bit_depth(BitDepth::Uint10);

    let m = [
        1. / 3.,
        10. / 3.,
        100. / 3.,
        0.,
        3.,
        4.,
        5.,
        0.,
        6.,
        7.,
        8.,
        0.,
        0.,
        0.,
        0.,
        1.,
    ];
    mat.set_matrix(&m);

    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UID42"))
        .unwrap();
    group.append_transform(Transform::Matrix(mat));

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    // In/out bit-depth equal, matrix not scaled.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Matrix inBitDepth="10i" outBitDepth="10i">
        <Array dim="3 3">
  0.333333333333333    3.33333333333333    33.3333333333333
                  3                   4                   5
                  6                   7                   8
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected,
        std::str::from_utf8(&output_transform).unwrap(),
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, matrix_offset_alpha_ctf)` @ v2.5.2.
#[test]
fn matrix_offset_alpha_ctf() {
    let mut mat = MatrixTransform::new();
    mat.set_file_input_bit_depth(BitDepth::Uint10);
    mat.set_file_output_bit_depth(BitDepth::Uint10);

    let m = [
        1., 10., 20., 0.5, 3., 4., 5., 0.9, 6., 7., 8., 1.1, 2., 30., 11., 1.,
    ];
    mat.set_matrix(&m);

    let o = [0.1, 0.2, 0.3, 1.0];
    mat.set_offset(&o);

    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UID42"))
        .unwrap();
    group.append_transform(Transform::Matrix(mat));

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Note that offset is scale by 1023 (for output bit-depth).
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Matrix inBitDepth="10i" outBitDepth="10i">
        <Array dim="4 5 4">
                  1                  10                  20                 0.5               102.3
                  3                   4                   5                 0.9               204.6
                  6                   7                   8                 1.1               306.9
                  2                  30                  11                   1                1023
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected,
        std::str::from_utf8(&output_transform).unwrap(),
    );

    // Alpha not handled by CLF.
    check_throw_what(
        write_group_clf(&group, &mut output_transform),
        "Transform uses the 'Matrix with alpha component' op which cannot be written as CLF",
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, matrix_offset_alpha_bitdepth_ctf)` @ v2.5.2.
#[test]
fn matrix_offset_alpha_bitdepth_ctf() {
    let mut mat = MatrixTransform::new();
    mat.set_file_input_bit_depth(BitDepth::Uint8);
    mat.set_file_output_bit_depth(BitDepth::Uint12);

    let m = [
        255. / 4095.,
        0.,
        0.,
        0.,
        0.,
        510. / 4095.,
        0.,
        0.,
        0.,
        0.,
        51. / 91.,
        0.,
        0.,
        0.,
        0.,
        51. / 182.,
    ];
    mat.set_matrix(&m);

    let o = [0.01, 0.02, 0.03, 0.001];
    mat.set_offset(&o);

    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UID42"))
        .unwrap();
    group.append_transform(Transform::Matrix(mat));

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Matrix scale following input bit-depth.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Matrix inBitDepth="8i" outBitDepth="12i">
        <Array dim="4 5 4">
                  1                   0                   0                   0               40.95
                  0                   2                   0                   0                81.9
                  0                   0                   9                   0              122.85
                  0                   0                   0                 4.5               4.095
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected,
        std::str::from_utf8(&output_transform).unwrap(),
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, matrix_offset_alpha_inverse_ctf)` @ v2.5.2.
#[test]
fn matrix_offset_alpha_inverse_ctf() {
    let mut mat = MatrixTransform::new();
    mat.set_file_input_bit_depth(BitDepth::F16);
    mat.set_file_output_bit_depth(BitDepth::F32);

    let m = [
        2., 0., 0., 0., 0., 4., 0., 0., 0., 0., 8., 0., 0., 0., 0., 1.,
    ];
    mat.set_matrix(&m);

    let o = [0.1, 0.2, 0.3, 1.0];
    mat.set_offset(&o);

    mat.set_direction(TransformDirection::Inverse);

    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UID42"))
        .unwrap();
    group.append_transform(Transform::Matrix(mat));

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Matrix inBitDepth="32f" outBitDepth="16f">
        <Array dim="4 5 4">
                0.5                   0                   0                   0               -0.05
                  0                0.25                   0                   0               -0.05
                  0                   0               0.125                   0             -0.0375
                  0                   0                   0                   1                  -1
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected,
        std::str::from_utf8(&output_transform).unwrap(),
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, no_ops_ctf)` @ v2.5.2.
#[test]
fn no_ops_ctf() {
    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UIDEC42"))
        .unwrap();

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDEC42">
    <Matrix inBitDepth="32f" outBitDepth="32f">
        <Array dim="3 3 3">
                  1                   0                   0
                  0                   1                   0
                  0                   0                   1
        </Array>
    </Matrix>
</ProcessList>
"#;

    assert_eq!(expected.len(), output_transform.len());
    assert_text_eq(
        "written",
        expected,
        std::str::from_utf8(&output_transform).unwrap(),
    );
}

/// `OCIO_CHECK_EQUAL(expected.size(), output.size()); OCIO_CHECK_EQUAL(expected, output);`
#[track_caller]
fn check_written(expected: &str, output: &[u8]) {
    assert_eq!(expected.len(), output.len());
    assert_text_eq("written", expected, std::str::from_utf8(output).unwrap());
}

/// A group of `transform` whose id is `id`.
fn group_of(id: &[u8], transform: impl Into<Transform>) -> GroupTransform {
    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(id))
        .unwrap();
    group.append_transform(transform.into());
    group
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_cdl)` @ v2.5.2.
#[test]
fn save_cdl() {
    let mut cdl_transform = CdlTransform::new();
    cdl_transform.set_direction(TransformDirection::Forward);
    let slope = [1.1, 1.2, 1.3];
    cdl_transform.set_slope(&slope);
    let offset = [2.1, 2.2, 2.3];
    cdl_transform.set_offset(&offset);
    let power = [3.1, 3.2, 3.3];
    cdl_transform.set_power(&power);
    let sat = 0.7;
    cdl_transform.set_sat(sat);
    let md = cdl_transform.format_metadata_mut();
    md.add_attribute(Some(METADATA_ID), Some(b"test-cdl-1"))
        .unwrap();
    for (name, value) in [
        (METADATA_DESCRIPTION, b"CDL description 1".as_slice()),
        (METADATA_DESCRIPTION, b"CDL description 2"),
        (METADATA_INPUT_DESCRIPTION, b"Input"),
        (METADATA_VIEWING_DESCRIPTION, b"Viewing"),
        (METADATA_SOP_DESCRIPTION, b"SOP description 1"),
        (METADATA_SOP_DESCRIPTION, b"SOP description 2"),
        (METADATA_SAT_DESCRIPTION, b"Sat description 1"),
        (METADATA_SAT_DESCRIPTION, b"Sat description 2"),
    ] {
        md.add_child_element(Some(name), Some(value)).unwrap();
    }

    let cached_file = write_read(cdl_transform.into());
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let op = &file_ops[0];
    let cdl = as_cdl(op);
    assert_eq!(op.get_id(), b"test-cdl-1");
    let metadata = op.get_format_metadata();
    assert_eq!(metadata.get_num_children_elements(), 8);
    for (i, name) in [
        METADATA_DESCRIPTION,
        METADATA_DESCRIPTION,
        METADATA_INPUT_DESCRIPTION,
        METADATA_VIEWING_DESCRIPTION,
        METADATA_SOP_DESCRIPTION,
        METADATA_SOP_DESCRIPTION,
        METADATA_SAT_DESCRIPTION,
        METADATA_SAT_DESCRIPTION,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            name,
            metadata
                .get_child_element(i as i32)
                .unwrap()
                .get_element_name()
        );
    }
    let params = cdl.get_slope_params().get_rgb();
    assert_eq!(params[0], slope[0]);
    assert_eq!(params[1], slope[1]);
    assert_eq!(params[2], slope[2]);
    let params = cdl.get_offset_params().get_rgb();
    assert_eq!(params[0], offset[0]);
    assert_eq!(params[1], offset[1]);
    assert_eq!(params[2], offset[2]);
    let params = cdl.get_power_params().get_rgb();
    assert_eq!(params[0], power[0]);
    assert_eq!(params[1], power[1]);
    assert_eq!(params[2], power[2]);
    let val = cdl.get_saturation();
    assert_eq!(val, sat);
}

/// Port of `TestSaveLog` (FileFormatCTF_tests.cpp:5142-5155 @ v2.5.2).
#[track_caller]
fn test_save_log(base: f64) {
    let mut log_t = LogTransform::new();
    log_t.set_base(base);

    let cached_file = write_read(log_t.into());
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let log = as_log(&file_ops[0]);
    assert_eq!(log.base(), base);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_log)` @ v2.5.2.
#[test]
fn save_log() {
    test_save_log(2.0);
    test_save_log(10.0);
    test_save_log(8.0);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_log_affine)` @ v2.5.2.
#[test]
fn save_log_affine() {
    let mut log_t = LogAffineTransform::new();
    let base = 8.0;
    log_t.set_base(base);
    let vals = [0.9, 1.1, 1.2];
    log_t.set_lin_side_slope_value(&vals);

    let cached_file = write_read(log_t.into());
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let log = as_log(&file_ops[0]);
    assert_eq!(log.base(), base);
    assert_eq!(log.red_params()[LIN_SIDE_SLOPE], vals[0]);
    assert_eq!(log.green_params()[LIN_SIDE_SLOPE], vals[1]);
    assert_eq!(log.blue_params()[LIN_SIDE_SLOPE], vals[2]);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_log_camera)` @ v2.5.2.
#[test]
fn save_log_camera() {
    let vals_break = [0.4, 0.5, 0.6];
    let mut log_t = LogCameraTransform::new(&vals_break);
    let base = 8.0;
    log_t.set_base(base);
    let vals = [0.9, 1.1, 1.2];
    log_t.set_lin_side_slope_value(&vals);
    let vals_ls = [1.2, 1.3, 1.4];
    log_t.set_linear_slope_value(&vals_ls).unwrap();

    let cached_file = write_read(log_t.into());
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let log = as_log(&file_ops[0]);
    assert_eq!(log.base(), base);
    assert_eq!(log.red_params()[LIN_SIDE_SLOPE], vals[0]);
    assert_eq!(log.green_params()[LIN_SIDE_SLOPE], vals[1]);
    assert_eq!(log.blue_params()[LIN_SIDE_SLOPE], vals[2]);
    assert_eq!(log.red_params()[LIN_SIDE_BREAK], vals_break[0]);
    assert_eq!(log.green_params()[LIN_SIDE_BREAK], vals_break[1]);
    assert_eq!(log.blue_params()[LIN_SIDE_BREAK], vals_break[2]);
    assert_eq!(log.red_params()[LINEAR_SLOPE], vals_ls[0]);
    assert_eq!(log.green_params()[LINEAR_SLOPE], vals_ls[1]);
    assert_eq!(log.blue_params()[LINEAR_SLOPE], vals_ls[2]);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_range)` @ v2.5.2.
#[test]
fn save_range() {
    let mut range_t = RangeTransform::new();
    range_t.set_direction(TransformDirection::Forward);
    range_t.set_min_in_value(0.0);
    range_t.set_max_in_value(0.5);
    range_t.set_min_out_value(0.5);
    range_t.set_max_out_value(1.5);

    let cached_file = write_read(range_t.clone().into());
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let range = as_range(&file_ops[0]);
    assert_eq!(range.get_min_in_value(), range_t.min_in_value());
    assert_eq!(range.get_max_in_value(), range_t.max_in_value());
    assert_eq!(range.get_min_out_value(), range_t.min_out_value());
    assert_eq!(range.get_max_out_value(), range_t.max_out_value());
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_group)` @ v2.5.2.
#[test]
fn save_group() {
    let mut range_t = RangeTransform::new();
    range_t.set_direction(TransformDirection::Forward);
    range_t.set_min_in_value(0.0f32 as f64);
    range_t.set_max_in_value(0.5f32 as f64);
    range_t.set_min_out_value(0.5f32 as f64);
    range_t.set_max_out_value(1.5f32 as f64);

    let mut mat_t = MatrixTransform::new();
    let offset4 = [0.123456789123, 0.11, 0.111, 0.2];
    mat_t.set_offset(&offset4);
    mat_t.set_direction(TransformDirection::Forward);

    let mut group_t = GroupTransform::new();
    group_t.append_transform(range_t.into());
    group_t.append_transform(mat_t.into());

    let cached_file = write_read(group_t.into());
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 2);
    as_range(&file_ops[0]);
    as_matrix(&file_ops[1]);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, legacy_cdl)` @ v2.5.2.
#[test]
fn legacy_cdl() {
    // Create empty legacy Config to use.
    let mut config = Config::new().unwrap();
    config.set_major_version(1).unwrap();

    let mut cdl = CdlTransform::new();
    let sop = [1.0, 1.1, 1.2, 0.2, 0.3, 0.4, 3.1, 3.2, 3.3];
    cdl.set_sop(&sop);
    cdl.set_sat(2.1);

    // Need to specify an id so that it does not get generated.
    let group = group_of(b"cdl0", cdl);

    let mut output_transform = Vec::new();
    group
        .write(&config, FILEFORMAT_CTF, &mut output_transform)
        .unwrap();

    // For OCIO v1, an ASC CDL was implemented as a Matrix/Gamma/Matrix rather than as a
    // dedicated op as in v2 and onward.
    let expected = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<ProcessList version=\"1.3\" id=\"cdl0\">\n",
        "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n",
        "        <Array dim=\"3 4 3\">\n",
        "                  1                   0                   0                 0.2\n",
        "                  0                 1.1                   0                 0.3\n",
        "                  0                   0                 1.2                 0.4\n",
        "        </Array>\n",
        "    </Matrix>\n",
        "    <Gamma inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"basicFwd\">\n",
        "        <GammaParams channel=\"R\" gamma=\"3.1\" />\n",
        "        <GammaParams channel=\"G\" gamma=\"3.2\" />\n",
        "        <GammaParams channel=\"B\" gamma=\"3.3\" />\n",
        "    </Gamma>\n",
        // Output matrix array as '3 3 3'.
        "    <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\">\n",
        "        <Array dim=\"3 3 3\">\n",
        "            1.86614            -0.78672            -0.07942\n",
        "           -0.23386             1.31328            -0.07942\n",
        "           -0.23386            -0.78672             2.02058\n",
        "        </Array>\n",
        "    </Matrix>\n",
        "</ProcessList>\n"
    );

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, cdl_clf)` @ v2.5.2.
#[test]
fn cdl_clf() {
    let mut cdl = CdlTransform::new();
    let sop = [1.0, 1.1, 1.2, 0.2, 0.3, 0.4, 3.1, 3.2, 3.3];
    cdl.set_sop(&sop);
    cdl.set_sat(2.1);
    let md = cdl.format_metadata_mut();
    md.add_attribute(Some(METADATA_NAME), Some(b"TestCDL"))
        .unwrap();
    md.add_attribute(Some(METADATA_ID), Some(b"CDL42")).unwrap();

    for (name, value) in [
        (METADATA_DESCRIPTION, b"CDL node for unit test".as_slice()),
        (METADATA_DESCRIPTION, b"Adding another description"),
        (METADATA_INPUT_DESCRIPTION, b"Input"),
        (METADATA_VIEWING_DESCRIPTION, b"Viewing"),
        (METADATA_SOP_DESCRIPTION, b"SOP description 1"),
        (METADATA_SOP_DESCRIPTION, b"SOP description 2"),
        (METADATA_SAT_DESCRIPTION, b"Sat description 1"),
        (METADATA_SAT_DESCRIPTION, b"Sat description 2"),
    ] {
        md.add_child_element(Some(name), Some(value)).unwrap();
    }

    let mut group = GroupTransform::new();

    // Need to specify an id so that it does not get generated.
    let gmd = group.format_metadata_mut();
    gmd.add_attribute(Some(METADATA_ID), Some(b"cdl1")).unwrap();
    gmd.add_child_element(Some(METADATA_DESCRIPTION), Some(b"ProcessList description"))
        .unwrap();
    gmd.add_child_element(Some(METADATA_DESCRIPTION), Some(b"======================="))
        .unwrap();

    group.append_transform(cdl.into());

    let gmd = group.format_metadata_mut();
    gmd.add_child_element(Some(METADATA_INFO), Some(b""))
        .unwrap();
    let info = gmd.get_child_element_mut(2).unwrap();
    info.add_child_element(Some(b"Release"), Some(b"2019"))
        .unwrap();
    info.add_child_element(Some(b"Directors"), Some(b""))
        .unwrap();
    let sub = info.get_child_element_mut(1).unwrap();
    for (i, last_name) in [b"Cronenberg".as_slice(), b"Lynch", b"Fincher", b"Lean"]
        .into_iter()
        .enumerate()
    {
        sub.add_child_element(Some(b"Director"), Some(b"")).unwrap();
        let sub_sub = sub.get_child_element_mut(i as i32).unwrap();
        sub_sub
            .add_attribute(Some(b"FirstName"), Some(b"David"))
            .unwrap();
        sub_sub
            .add_attribute(Some(b"LastName"), Some(last_name))
            .unwrap();
    }

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="cdl1">
    <Description>ProcessList description</Description>
    <Description>=======================</Description>
    <Info>
        <Release>2019</Release>
        <Directors>
            <Director FirstName="David" LastName="Cronenberg"></Director>
            <Director FirstName="David" LastName="Lynch"></Director>
            <Director FirstName="David" LastName="Fincher"></Director>
            <Director FirstName="David" LastName="Lean"></Director>
        </Directors>
    </Info>
    <ASC_CDL id="CDL42" name="TestCDL" inBitDepth="32f" outBitDepth="32f" style="FwdNoClamp">
        <Description>CDL node for unit test</Description>
        <Description>Adding another description</Description>
        <InputDescription>Input</InputDescription>
        <ViewingDescription>Viewing</ViewingDescription>
        <SOPNode>
            <Description>SOP description 1</Description>
            <Description>SOP description 2</Description>
            <Slope>1 1.1 1.2</Slope>
            <Offset>0.2 0.3 0.4</Offset>
            <Power>3.1 3.2 3.3</Power>
        </SOPNode>
        <SatNode>
            <Description>Sat description 1</Description>
            <Description>Sat description 2</Description>
            <Saturation>2.1</Saturation>
        </SatNode>
    </ASC_CDL>
</ProcessList>
"#;

    check_written(expected, &output_transform);

    // Now test if the read is working.

    let mut ctf_stream = InputStream::from_bytes(expected.as_bytes().to_vec());

    let cached_file = LocalFileFormat.read_file(&mut ctf_stream, b"").unwrap();

    let op_list = cached_file.transform.get_ops();
    assert_eq!(op_list.len(), 1);
    let cdl_data = as_cdl(&op_list[0]);
    assert_eq!(cdl_data.get_slope_params().get_rgb()[0], 1.);
    assert_eq!(cdl_data.get_slope_params().get_rgb()[1], 1.1);
    assert_eq!(cdl_data.get_slope_params().get_rgb()[2], 1.2);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, cdl_ctf)` @ v2.5.2.
#[test]
fn cdl_ctf() {
    let mut cdl = CdlTransform::new();
    cdl.set_style(CdlStyle::Asc);

    let sop = [1.0, 1.1, 1.2, 0.2, 0.3, 0.4, 3.1, 3.2, 3.3];
    cdl.set_sop(&sop);
    cdl.set_sat(2.1);

    // Need to specify an id so that it does not get generated.
    let group = group_of(b"cdl2", cdl);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<ProcessList version=\"1.7\" id=\"cdl2\">\n",
        "    <ASC_CDL inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"Fwd\">\n",
        "        <SOPNode>\n",
        "            <Slope>1 1.1 1.2</Slope>\n",
        "            <Offset>0.2 0.3 0.4</Offset>\n",
        "            <Power>3.1 3.2 3.3</Power>\n",
        "        </SOPNode>\n",
        "        <SatNode>\n",
        "            <Saturation>2.1</Saturation>\n",
        "        </SatNode>\n",
        "    </ASC_CDL>\n",
        "</ProcessList>\n"
    );

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, range_ctf)` @ v2.5.2.
#[test]
fn range_ctf() {
    // Non-clamping range are converted to matrix.
    let mut range = RangeTransform::new();
    range.set_style(RangeStyle::NoClamp);
    range.set_min_in_value(0.1);
    range.set_max_in_value(0.9);
    range.set_min_out_value(0.0);
    range.set_max_out_value(1.2);
    let md = range.format_metadata_mut();
    md.add_child_element(
        Some(METADATA_DESCRIPTION),
        Some(b"Range node for unit test"),
    )
    .unwrap();
    md.add_attribute(Some(METADATA_NAME), Some(b"TestRange"))
        .unwrap();
    md.add_attribute(Some(METADATA_ID), Some(b"Range42"))
        .unwrap();

    // Need to specify an id so that it does not get generated.
    let mut group = group_of(b"mat0", range);
    let gmd = group.format_metadata_mut();
    gmd.add_child_element(Some(METADATA_INPUT_DESCRIPTOR), Some(b"Input descriptor"))
        .unwrap();
    gmd.add_child_element(Some(METADATA_OUTPUT_DESCRIPTOR), Some(b"Output descriptor"))
        .unwrap();

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<ProcessList version=\"1.3\" id=\"mat0\">\n",
        "    <InputDescriptor>Input descriptor</InputDescriptor>\n",
        "    <OutputDescriptor>Output descriptor</OutputDescriptor>\n",
        "    <Matrix id=\"Range42\" name=\"TestRange\" inBitDepth=\"32f\" outBitDepth=\"32f\">\n",
        "        <Description>Range node for unit test</Description>\n",
        "        <Array dim=\"3 4 3\">\n",
        "                1.5                   0                   0               -0.15\n",
        "                  0                 1.5                   0               -0.15\n",
        "                  0                   0                 1.5               -0.15\n",
        "        </Array>\n",
        "    </Matrix>\n",
        "</ProcessList>\n"
    );

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, range1_clf)` @ v2.5.2.
#[test]
fn range1_clf() {
    // Forward clamping range with all 4 values set and with metadata.
    let mut range = RangeTransform::new();
    range.set_file_input_bit_depth(BitDepth::Uint8);
    range.set_style(RangeStyle::Clamp);
    range.set_min_in_value(16.0 / 255.0);
    range.set_max_in_value(235. / 255.0);
    range.set_min_out_value(-0.5);
    range.set_max_out_value(2.1);
    let md = range.format_metadata_mut();
    md.add_child_element(
        Some(METADATA_DESCRIPTION),
        Some(b"Range node for unit test"),
    )
    .unwrap();
    md.add_attribute(Some(METADATA_NAME), Some(b"TestRange"))
        .unwrap();
    md.add_attribute(Some(METADATA_ID), Some(b"Range42"))
        .unwrap();

    let mut group = group_of(b"UID42", range);
    let gmd = group.format_metadata_mut();
    gmd.add_child_element(Some(METADATA_INPUT_DESCRIPTOR), Some(b"Input descriptor"))
        .unwrap();
    gmd.add_child_element(Some(METADATA_OUTPUT_DESCRIPTOR), Some(b"Output descriptor"))
        .unwrap();

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    let expected = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<ProcessList compCLFversion=\"3\" id=\"UID42\">\n",
        "    <InputDescriptor>Input descriptor</InputDescriptor>\n",
        "    <OutputDescriptor>Output descriptor</OutputDescriptor>\n",
        "    <Range id=\"Range42\" name=\"TestRange\" inBitDepth=\"8i\" outBitDepth=\"32f\">\n",
        "        <Description>Range node for unit test</Description>\n",
        "        <minInValue> 16 </minInValue>\n",
        "        <maxInValue> 235 </maxInValue>\n",
        "        <minOutValue> -0.5 </minOutValue>\n",
        "        <maxOutValue> 2.1 </maxOutValue>\n",
        "    </Range>\n",
        "</ProcessList>\n"
    );

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, range2_clf)` @ v2.5.2.
#[test]
fn range2_clf() {
    // Forward clamping range with just minValues set.
    let mut range = RangeTransform::new();
    range.set_file_input_bit_depth(BitDepth::Uint10);
    range.set_file_output_bit_depth(BitDepth::Uint8);
    range.set_min_in_value(0.1);
    range.set_min_out_value(0.1);
    range
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"Range42"))
        .unwrap();

    let group = group_of(b"UID42", range);

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    let expected = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<ProcessList compCLFversion=\"3\" id=\"UID42\">\n",
        "    <Range id=\"Range42\" inBitDepth=\"10i\" outBitDepth=\"8i\">\n",
        "        <minInValue> 102.3 </minInValue>\n",
        "        <minOutValue> 25.5 </minOutValue>\n",
        "    </Range>\n",
        "</ProcessList>\n"
    );

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, range3_clf)` @ v2.5.2.
#[test]
fn range3_clf() {
    // Forward clamping range with just minValues set.

    // This will only do bit-depth conversion (with a clamp at 0).
    let mut range = RangeTransform::new();
    range.set_file_input_bit_depth(BitDepth::F16);
    range.set_file_output_bit_depth(BitDepth::Uint12);
    range.set_min_in_value(0.);
    range.set_min_out_value(0.);
    range
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"Range42"))
        .unwrap();

    let group = group_of(b"UID42", range);

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Range id="Range42" inBitDepth="16f" outBitDepth="12i">
        <minInValue> 0 </minInValue>
        <minOutValue> 0 </minOutValue>
    </Range>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, range4_clf)` @ v2.5.2.
#[test]
fn range4_clf() {
    // Inverse clamping range with all 4 values set.
    let mut range = RangeTransform::new();
    range.set_file_input_bit_depth(BitDepth::F16);
    range.set_file_output_bit_depth(BitDepth::Uint12);
    // Set inverse direction.
    range.set_direction(TransformDirection::Inverse);
    range.set_min_in_value(0.);
    range.set_min_out_value(0.5);
    range.set_max_in_value(1.0);
    range.set_max_out_value(1.0);
    range
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"Range42"))
        .unwrap();

    let group = group_of(b"UID42", range);

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    // Range is saved in the forward direction.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Range id="Range42" inBitDepth="12i" outBitDepth="16f">
        <minInValue> 2047.5 </minInValue>
        <maxInValue> 4095 </maxInValue>
        <minOutValue> 0 </minOutValue>
        <maxOutValue> 1 </maxOutValue>
    </Range>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, exponent_ctf)` @ v2.5.2.
#[test]
fn exponent_ctf() {
    let mut exp = ExponentWithLinearTransform::new();
    let gamma = [1.1, 1.2, 1.3, 1.0];
    exp.set_gamma(&gamma);
    let offset = [0.1, 0.2, 0.1, 0.0];
    exp.set_offset(&offset);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<ProcessList version=\"1.3\" id=\"UID42\">\n",
        "    <Gamma inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"monCurveFwd\">\n",
        "        <GammaParams channel=\"R\" gamma=\"1.1\" offset=\"0.1\" />\n",
        "        <GammaParams channel=\"G\" gamma=\"1.2\" offset=\"0.2\" />\n",
        "        <GammaParams channel=\"B\" gamma=\"1.3\" offset=\"0.1\" />\n",
        "    </Gamma>\n",
        "</ProcessList>\n"
    );

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma1_ctf)` @ v2.5.2.
#[test]
fn gamma1_ctf() {
    let mut exp = ExponentTransform::new();
    exp.set_direction(TransformDirection::Inverse);
    let gamma = [2.6, 2.6, 2.6, 1.0];
    exp.set_value(&gamma);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Identity alpha. Transform written as version 1.3.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <GammaParams gamma="2.6" />
    </Gamma>
</ProcessList>
"#;

    check_written(expected, &output_transform);

    let mut output_transform_clf = Vec::new();
    write_group_clf(&group, &mut output_transform_clf).unwrap();

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;

    check_written(expected_clf, &output_transform_clf);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma1_mirror_ctf)` @ v2.5.2.
#[test]
fn gamma1_mirror_ctf() {
    let mut exp = ExponentTransform::new();
    exp.set_direction(TransformDirection::Inverse);
    exp.set_negative_style(NegativeStyle::Mirror).unwrap();
    let gamma = [2.6, 2.6, 2.6, 1.0];
    exp.set_value(&gamma);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Identity alpha. Transform written as version 2 because of new style.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicMirrorRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;

    check_written(expected, &output_transform);

    let mut output_transform_clf = Vec::new();
    write_group_clf(&group, &mut output_transform_clf).unwrap();

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicMirrorRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;

    check_written(expected_clf, &output_transform_clf);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma1_pass_thru_ctf)` @ v2.5.2.
#[test]
fn gamma1_pass_thru_ctf() {
    let mut exp = ExponentTransform::new();
    exp.set_direction(TransformDirection::Inverse);
    exp.set_negative_style(NegativeStyle::PassThru).unwrap();
    let gamma = [2.6, 2.6, 2.6, 1.0];
    exp.set_value(&gamma);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Identity alpha. Transform written as version 2.0 because of new style.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicPassThruRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;

    check_written(expected, &output_transform);

    let mut output_transform_clf = Vec::new();
    write_group_clf(&group, &mut output_transform_clf).unwrap();

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="basicPassThruRev">
        <ExponentParams exponent="2.6" />
    </Exponent>
</ProcessList>
"#;

    check_written(expected_clf, &output_transform_clf);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma2_ctf)` @ v2.5.2.
#[test]
fn gamma2_ctf() {
    let mut exp = ExponentWithLinearTransform::new();
    exp.set_direction(TransformDirection::Inverse);
    let gamma = [2.4, 2.2, 2.0, 1.8];
    exp.set_gamma(&gamma);
    let offset = [0.1, 0.2, 0.4, 0.8];
    exp.set_offset(&offset);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Non-identity alpha. Transform written as version 1.5.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.5" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="monCurveRev">
        <GammaParams channel="R" gamma="2.4" offset="0.1" />
        <GammaParams channel="G" gamma="2.2" offset="0.2" />
        <GammaParams channel="B" gamma="2" offset="0.4" />
        <GammaParams channel="A" gamma="1.8" offset="0.8" />
    </Gamma>
</ProcessList>
"#;

    check_written(expected, &output_transform);

    // CLF does not allow alpha channel.
    let mut output_transform_clf = Vec::new();
    check_throw_what(
        write_group_clf(&group, &mut output_transform_clf),
        "Transform uses the 'Gamma with alpha component' op which cannot be written as CLF",
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma3_ctf)` @ v2.5.2.
#[test]
fn gamma3_ctf() {
    let mut exp = ExponentWithLinearTransform::new();
    let gamma = [2.42, 2.42, 2.42, 1.0];
    exp.set_gamma(&gamma);
    let offset = [0.099, 0.099, 0.099, 0.0];
    exp.set_offset(&offset);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Identity alpha.  Transform written as version 1.3.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="monCurveFwd">
        <GammaParams gamma="2.42" offset="0.099" />
    </Gamma>
</ProcessList>
"#;

    check_written(expected, &output_transform);

    // CLF does not allow alpha channel.
    let mut output_transform_clf = Vec::new();
    write_group_clf(&group, &mut output_transform_clf).unwrap();

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Exponent inBitDepth="32f" outBitDepth="32f" style="monCurveFwd">
        <ExponentParams exponent="2.42" offset="0.099" />
    </Exponent>
</ProcessList>
"#;

    check_written(expected_clf, &output_transform_clf);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma4_ctf)` @ v2.5.2.
#[test]
fn gamma4_ctf() {
    let mut exp = ExponentTransform::new();
    let gamma = [2.6, 2.5, 2.4, 2.2];
    exp.set_value(&gamma);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Non-identity alpha.  Transform written as version 1.5.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.5" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="basicFwd">
        <GammaParams channel="R" gamma="2.6" />
        <GammaParams channel="G" gamma="2.5" />
        <GammaParams channel="B" gamma="2.4" />
        <GammaParams channel="A" gamma="2.2" />
    </Gamma>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma5_ctf)` @ v2.5.2.
#[test]
fn gamma5_ctf() {
    let mut exp = ExponentWithLinearTransform::new();
    let gamma = [1. / 0.45, 1. / 0.45, 1. / 0.45, 1. / 0.45];
    exp.set_gamma(&gamma);
    let offset = [0.099, 0.099, 0.099, 0.099];
    exp.set_offset(&offset);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // Non-identity alpha.  Transform written as version 1.5.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.5" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="monCurveFwd">
        <GammaParams channel="R" gamma="2.22222222222222" offset="0.099" />
        <GammaParams channel="G" gamma="2.22222222222222" offset="0.099" />
        <GammaParams channel="B" gamma="2.22222222222222" offset="0.099" />
        <GammaParams channel="A" gamma="2.22222222222222" offset="0.099" />
    </Gamma>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, gamma6_ctf)` @ v2.5.2.
#[test]
fn gamma6_ctf() {
    let mut exp = ExponentTransform::new();
    let gamma = [2.4, 2.5, 2.6, 1.0];
    exp.set_value(&gamma);

    let group = group_of(b"UID42", exp);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    // R,G,B channels different, but alpha is identity. Transform written as version 1.3.
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <Gamma inBitDepth="32f" outBitDepth="32f" style="basicFwd">
        <GammaParams channel="R" gamma="2.4" />
        <GammaParams channel="G" gamma="2.5" />
        <GammaParams channel="B" gamma="2.6" />
    </Gamma>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, log_lin_to_log_ctf)` @ v2.5.2.
#[test]
fn log_lin_to_log_ctf() {
    let mut log_t = LogAffineTransform::new();

    let base = 2.0;
    log_t.set_base(base);
    let lins = [0.9, 1.1, 1.2];
    log_t.set_lin_side_slope_value(&lins);
    let lino = [0.1, 0.2, 0.3];
    log_t.set_lin_side_offset_value(&lino);
    let logs = [1.3, 1.4, 1.5];
    log_t.set_log_side_slope_value(&logs);
    let logo = [0.4, 0.5, 0.6];
    log_t.set_log_side_offset_value(&logo);

    let group = group_of(b"UIDLOG42", log_t);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDLOG42">
    <Log inBitDepth="32f" outBitDepth="32f" style="linToLog">
        <LogParams channel="R" base="2" linSideSlope="0.9" linSideOffset="0.1" logSideSlope="1.3" logSideOffset="0.4" />
        <LogParams channel="G" base="2" linSideSlope="1.1" linSideOffset="0.2" logSideSlope="1.4" logSideOffset="0.5" />
        <LogParams channel="B" base="2" linSideSlope="1.2" linSideOffset="0.3" logSideSlope="1.5" logSideOffset="0.6" />
    </Log>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, log_log_to_lin_ctf)` @ v2.5.2.
#[test]
fn log_log_to_lin_ctf() {
    let mut log_t = LogAffineTransform::new();
    log_t.set_direction(TransformDirection::Inverse);
    let base = 2.0;
    log_t.set_base(base);
    let vals = [0.9, 0.9, 0.9];
    log_t.set_lin_side_slope_value(&vals);

    let group = group_of(b"UIDLOG42", log_t);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDLOG42">
    <Log inBitDepth="32f" outBitDepth="32f" style="logToLin">
        <LogParams base="2" linSideSlope="0.9" linSideOffset="0" logSideSlope="1" logSideOffset="0" />
    </Log>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, log_antilog2_ctf)` @ v2.5.2.
#[test]
fn log_antilog2_ctf() {
    let mut log_t = LogAffineTransform::new();
    log_t.set_direction(TransformDirection::Inverse);
    let base = 2.0;
    log_t.set_base(base);

    let group = group_of(b"UIDLOG42", log_t);

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDLOG42">
    <Log inBitDepth="32f" outBitDepth="32f" style="antiLog2">
    </Log>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// The processor of the test file `file_name`'s group written as CTF.
fn written_ctf_of_file(file_name: &str) -> String {
    let processor = get_file_transform_processor(file_name).unwrap();
    let group = processor.create_group_transform().unwrap();

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();
    String::from_utf8(output_transform).unwrap()
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut_1d_1component)` @ v2.5.2.
#[test]
fn save_lut_1d_1component() {
    let result = written_ctf_of_file("clf/lut1d_32f_example.clf");
    let expected = "<Array dim=\"65 1\">";
    assert!(result.contains(expected));
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut_1d_3components)` @ v2.5.2.
#[test]
fn save_lut_1d_3components() {
    let result = written_ctf_of_file("lut1d_green.ctf");
    let expected = "<Array dim=\"32 3\">";
    assert!(result.contains(expected));
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_invlut_1d_3components)` @ v2.5.2.
#[test]
fn save_invlut_1d_3components() {
    let result = written_ctf_of_file("lut1d_inv.ctf");
    let expected1 = "</InverseLUT1D>";
    assert!(result.contains(expected1));
    // Components are equal, so only 1 get saved.
    let expected2 = "<Array dim=\"17 1\">";
    assert!(result.contains(expected2));
}

/// Whether the half of `bits` is a NaN (`half::isNan`, Imath half.h @ v3.2.1).
fn half_is_nan(bits: u16) -> bool {
    (bits & 0x7c00) == 0x7c00 && (bits & 0x03ff) != 0
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut1d_halfdomain)` @ v2.5.2.
#[test]
fn save_lut1d_halfdomain() {
    let mut lut_t = Lut1DTransform::new();
    lut_t.set_file_output_bit_depth(BitDepth::Uint10);

    let size = Lut1DOpData::get_lut_ideal_size(BitDepth::F16).unwrap();
    lut_t.set_length(size).unwrap();
    lut_t.set_input_half_domain(true);

    for i in 0..size {
        let val = half_to_float(i as u16);
        lut_t.set_value(i, val, val, val).unwrap();
    }

    let cached_file = write_read(Transform::Lut1D(lut_t));
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let OpData::Lut1D(lut) = &*file_ops[0] else {
        panic!("not a Lut1D");
    };
    assert_eq!(lut.get_file_output_bit_depth(), BitDepth::Uint10);
    assert!(lut.is_input_half_domain());

    assert_eq!(lut.get_array().get_length(), size);

    let array = lut.get_array().get_values();
    for i in 0..size as usize {
        let expected = i as u16;
        let loaded_val = array[3 * i];
        let loaded = float_to_half(loaded_val);
        if half_is_nan(expected) {
            assert!(half_is_nan(loaded));
            assert!(array[3 * i + 1].is_nan());
            assert!(array[3 * i + 2].is_nan());
        } else {
            assert_eq!(loaded, expected);
            assert_eq!(loaded_val, array[3 * i + 1]);
            assert_eq!(loaded_val, array[3 * i + 2]);
        }
    }
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut1d_f16_raw)` @ v2.5.2. `HALF_MAX` and
/// `HALF_NRM_MIN` are Imath's (half.h @ v3.2.1), the halfs of bits 0x7bff and 0x0400.
#[test]
fn save_lut1d_f16_raw() {
    let mut lut_t = Lut1DTransform::new();
    lut_t.set_file_output_bit_depth(BitDepth::F16);

    lut_t.set_length(2).unwrap();
    lut_t.set_output_raw_halfs(true);

    let half = |f: f32| half_to_float(float_to_half(f));
    let values = [
        half(1.0f32 / 3.0f32),
        half_to_float(0x7bff),
        half_to_float(0x0400),
        half(1.0f32 / 7.0f32),
        half_to_float(0x7c00),
        -half_to_float(0x7c00),
    ];
    lut_t.set_value(0, values[0], values[1], values[2]).unwrap();
    lut_t.set_value(1, values[3], values[4], values[5]).unwrap();

    let cached_file = write_read(Transform::Lut1D(lut_t));
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let OpData::Lut1D(lut) = &*file_ops[0] else {
        panic!("not a Lut1D");
    };
    assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F16);

    assert_eq!(lut.get_array().get_length(), 2);

    let array = lut.get_array().get_values();
    for (i, value) in values.iter().enumerate() {
        assert_eq!(*value, array[i]);
    }
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut1d_f32)` @ v2.5.2.
#[test]
fn save_lut1d_f32() {
    let mut lut_t = Lut1DTransform::new();
    lut_t.set_file_output_bit_depth(BitDepth::F32);

    lut_t.set_length(8).unwrap();
    let values = [
        1.0f32 / 3.0f32,
        0.0000000000000001f32,
        0.9999999f32,
        0.0,
        f32::MAX,
        -f32::MIN_POSITIVE,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];

    for (i, &v) in values.iter().enumerate() {
        lut_t.set_value(i as c_ulong, v, v, v).unwrap();
    }

    let cached_file = write_read(Transform::Lut1D(lut_t));
    let file_ops = cached_file.transform.get_ops();
    assert_eq!(file_ops.len(), 1);
    let OpData::Lut1D(lut) = &*file_ops[0] else {
        panic!("not a Lut1D");
    };
    assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F32);

    assert_eq!(lut.get_array().get_length(), 8);

    let array = lut.get_array().get_values();
    for (i, value) in values.iter().enumerate() {
        assert_eq!(array[3 * i], *value);
    }
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut1d_interpolation)` @ v2.5.2. Its config,
/// created and set to version 2, isn't used.
#[test]
fn save_lut1d_interpolation() {
    let mut lut = Lut1DTransform::new();
    lut.set_interpolation(Interpolation::Default);

    let mut group = GroupTransform::new();
    group.append_transform(Transform::Lut1D(lut.clone()));

    let mut output_transform = Vec::new();
    write_group_clf(&group, &mut output_transform).unwrap();

    {
        let result = String::from_utf8(output_transform.clone()).unwrap();
        let expected = r#"<LUT1D inBitDepth="32f" outBitDepth="32f">"#;
        assert!(result.contains(expected));
    }

    output_transform.clear();
    let mut group = GroupTransform::new();

    lut.set_interpolation(Interpolation::Best);

    group.append_transform(Transform::Lut1D(lut.clone()));
    write_group_clf(&group, &mut output_transform).unwrap();

    {
        let result = String::from_utf8(output_transform.clone()).unwrap();
        let expected = r#"<LUT1D inBitDepth="32f" outBitDepth="32f" interpolation="linear">"#;
        assert!(result.contains(expected));
    }

    output_transform.clear();
    let mut group = GroupTransform::new();

    lut.set_interpolation(Interpolation::Linear);

    group.append_transform(Transform::Lut1D(lut.clone()));
    write_group_clf(&group, &mut output_transform).unwrap();

    {
        let result = String::from_utf8(output_transform.clone()).unwrap();
        let expected = r#"<LUT1D inBitDepth="32f" outBitDepth="32f" interpolation="linear">"#;
        assert!(result.contains(expected));
    }

    output_transform.clear();
    let mut group = GroupTransform::new();

    lut.set_interpolation(Interpolation::Cubic);
    group.append_transform(Transform::Lut1D(lut));
    check_throw_what(
        write_group_clf(&group, &mut output_transform),
        "1D LUT does not support interpolation algorithm: cubic",
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_invalid_lut_1d)` @ v2.5.2.
#[test]
fn save_invalid_lut_1d() {
    let mut lut_t = Lut1DTransform::new();
    lut_t.set_file_output_bit_depth(BitDepth::F32);

    lut_t.set_length(8).unwrap();
    lut_t.set_input_half_domain(true);

    check_throw_what(
        try_write_read(Transform::Lut1D(lut_t)),
        "65536 required for halfDomain 1D LUT",
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut_3d)` @ v2.5.2.
#[test]
fn save_lut_3d() {
    let result = written_ctf_of_file("clf/lut3d_identity_12i_16f.clf");
    let expected = "<Array dim=\"2 2 2 3\">";
    assert!(result.contains(expected));
}

/// A group of `transform` whose id is `UIDLUT42`, written as CTF or CLF.
fn write_lut_group(transform: Transform, clf: bool) -> Result<Vec<u8>> {
    let group = group_of(b"UIDLUT42", transform);
    let mut output_transform = Vec::new();
    if clf {
        write_group_clf(&group, &mut output_transform)?;
    } else {
        write_group_ctf(&group, &mut output_transform)?;
    }
    Ok(output_transform)
}

/// `getFormatMetadata().addAttribute(METADATA_NAME, name)` and `(METADATA_ID, id)`.
fn add_name_and_id(metadata: &mut ocio_ops::format_metadata::FormatMetadataImpl, name: &[u8]) {
    metadata
        .add_attribute(Some(METADATA_NAME), Some(name))
        .unwrap();
    metadata
        .add_attribute(Some(METADATA_ID), Some(b"lut01"))
        .unwrap();
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_clf)` @ v2.5.2.
#[test]
fn lut1d_clf() {
    let mut lut = Lut1DTransform::new();
    lut.set_interpolation(Interpolation::Linear);

    let output_transform = write_lut_group(Transform::Lut1D(lut), true).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UIDLUT42">
    <LUT1D inBitDepth="32f" outBitDepth="32f" interpolation="linear">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_inverse_clf)` @ v2.5.2.
#[test]
fn lut1d_inverse_clf() {
    let mut lut = Lut1DTransform::new();
    lut.set_direction(TransformDirection::Inverse);

    check_throw_what(
        write_lut_group(Transform::Lut1D(lut), true),
        "Transform uses the 'InverseLUT1D' op which cannot be written as CLF",
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_ctf)` @ v2.5.2.
#[test]
fn lut1d_ctf() {
    let mut lut = Lut1DTransform::new();
    lut.set_interpolation(Interpolation::Default);

    let output_transform = write_lut_group(Transform::Lut1D(lut), false).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_attributes_ctf)` @ v2.5.2.
#[test]
fn lut1d_attributes_ctf() {
    let mut lut = Lut1DTransform::new();
    add_name_and_id(lut.format_metadata_mut(), b"test-lut");
    lut.set_file_output_bit_depth(BitDepth::Uint10);
    lut.set_interpolation(Interpolation::Default);
    lut.set_input_half_domain(true);
    lut.set_output_raw_halfs(true);
    lut.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    lut.set_length(65536).unwrap();
    let [r, g, b] = lut.value(1000).unwrap();
    lut.set_value(1000, r * 1.001f32, g * 1.002f32, b * 1.003f32)
        .unwrap();

    let output_transform = write_lut_group(Transform::Lut1D(lut), false).unwrap();

    let input_transform = String::from_utf8(output_transform).unwrap();
    let mut lines = input_transform.split('\n');

    let mut line = lines.next().unwrap();
    assert_eq!(line, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");

    line = lines.next().unwrap();
    assert_eq!(line, r#"<ProcessList version="1.4" id="UIDLUT42">"#);

    line = lines.next().unwrap();
    assert_eq!(
        trim(line.as_bytes()),
        b"<LUT1D id=\"lut01\" name=\"test-lut\" inBitDepth=\"32f\" outBitDepth=\"10i\" \
          halfDomain=\"true\" rawHalfs=\"true\" hueAdjust=\"dw3\">"
    );

    line = lines.next().unwrap();
    assert_eq!(trim(line.as_bytes()), br#"<Array dim="65536 3">"#);

    for _ in 0..=1000 {
        line = lines.next().unwrap();
    }
    assert_eq!(trim(line.as_bytes()), b"11216 11218 11220");
}

/// The Lut1D of `lut1d_array_16x1_ctf`, `lut1d_array_16x3_ctf` and `lut1d_inverse_ctf`: 16
/// entries of 10i values counting by 3 from 0, or by 1 over the three channels.
fn lut1d_16(one_component: bool) -> Lut1DTransform {
    let mut lut = Lut1DTransform::new();
    lut.set_interpolation(Interpolation::Default);
    add_name_and_id(lut.format_metadata_mut(), b"test-lut");
    lut.set_file_output_bit_depth(BitDepth::Uint10);
    lut.set_length(16).unwrap();
    let mut rgb = 0.0f32;
    for i in 0..16 {
        if one_component {
            let val = rgb / 1023.0f32;
            lut.set_value(i, val, val, val).unwrap();
        } else {
            lut.set_value(
                i,
                rgb / 1023.0f32,
                (rgb + 1.0f32) / 1023.0f32,
                (rgb + 2.0f32) / 1023.0f32,
            )
            .unwrap();
        }
        rgb += 3.0f32;
    }
    lut
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_array_16x1_ctf)` @ v2.5.2.
#[test]
fn lut1d_array_16x1_ctf() {
    let lut = lut1d_16(true);

    let output_transform = write_lut_group(Transform::Lut1D(lut), false).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
        <Array dim="16 1">
   0
   3
   6
   9
  12
  15
  18
  21
  24
  27
  30
  33
  36
  39
  42
  45
        </Array>
    </LUT1D>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_array_16x3_ctf)` @ v2.5.2.
#[test]
fn lut1d_array_16x3_ctf() {
    let lut = lut1d_16(false);

    let output_transform = write_lut_group(Transform::Lut1D(lut), false).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
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
    </LUT1D>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_10i_ctf)` @ v2.5.2.
#[test]
fn lut1d_10i_ctf() {
    let mut lut = Lut1DTransform::new();
    lut.set_interpolation(Interpolation::Default);
    add_name_and_id(lut.format_metadata_mut(), b"test-lut");
    lut.set_file_output_bit_depth(BitDepth::Uint10);
    lut.set_length(3).unwrap();
    lut.set_value(
        1,
        511.0f32 / 1023.0f32,
        4011.12345f32 / 1023.0f32,
        -24.10297f32 / 1023.0f32,
    )
    .unwrap();

    let output_transform = write_lut_group(Transform::Lut1D(lut), false).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT1D id="lut01" name="test-lut" inBitDepth="32f" outBitDepth="10i">
        <Array dim="3 3">
   0    0    0
 511 4011.12 -24.103
   1023    1023    1023
        </Array>
    </LUT1D>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut1d_inverse_ctf)` @ v2.5.2.
#[test]
fn lut1d_inverse_ctf() {
    let mut lut = lut1d_16(false);

    lut.set_direction(TransformDirection::Inverse);

    let output_transform = write_lut_group(Transform::Lut1D(lut), false).unwrap();

    // Note the type of the node.
    //
    // For an InverseLUT1D, the scaling of array values is based on the inBitDepth.
    //
    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <InverseLUT1D id="lut01" name="test-lut" inBitDepth="10i" outBitDepth="32f">
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

    check_written(expected, &output_transform);
}

/// The Lut3D of `lut3d_array_ctf`, `lut3d_inverse_clf` and `lut3d_inverse_ctf`: a 3x3x3 grid
/// of 10i values counting by 1 over the channels.
fn lut3d_3() -> Lut3DTransform {
    let mut lut = Lut3DTransform::new();
    add_name_and_id(lut.format_metadata_mut(), b"test-lut3d");
    lut.set_file_output_bit_depth(BitDepth::Uint10);
    let gs: c_ulong = 3;
    lut.set_grid_size(gs).unwrap();
    let mut rgb = 0.0f32;
    for r in 0..gs {
        for g in 0..3 {
            for b in 0..3 {
                lut.set_value(
                    r,
                    g,
                    b,
                    rgb / 1023.0f32,
                    (rgb + 1.0f32) / 1023.0f32,
                    (rgb + 2.0f32) / 1023.0f32,
                )
                .unwrap();
                rgb += 3.0f32;
            }
        }
    }
    lut
}

/// The array of `lut3d_3`, as `lut3d_array_ctf` and `lut3d_inverse_ctf` expect it.
const LUT3D_3_ARRAY: &str = r#"        <Array dim="3 3 3 3">
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
  48   49   50
  51   52   53
  54   55   56
  57   58   59
  60   61   62
  63   64   65
  66   67   68
  69   70   71
  72   73   74
  75   76   77
  78   79   80
        </Array>
"#;

/// Port of `OCIO_ADD_TEST(CTFTransform, lut3d_array_ctf)` @ v2.5.2.
#[test]
fn lut3d_array_ctf() {
    let mut lut = Lut3DTransform::new();
    add_name_and_id(lut.format_metadata_mut(), b"test-lut3d");
    lut.set_file_output_bit_depth(BitDepth::Uint10);
    lut.set_interpolation(Interpolation::Tetrahedral);
    let gs: c_ulong = 3;
    lut.set_grid_size(gs).unwrap();
    let mut rgb = 0.0f32;
    for r in 0..gs {
        for g in 0..3 {
            for b in 0..3 {
                lut.set_value(
                    r,
                    g,
                    b,
                    rgb / 1023.0f32,
                    (rgb + 1.0f32) / 1023.0f32,
                    (rgb + 2.0f32) / 1023.0f32,
                )
                .unwrap();
                rgb += 3.0f32;
            }
        }
    }

    let output_transform = write_lut_group(Transform::Lut3D(lut), false).unwrap();

    let expected = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UIDLUT42">
    <LUT3D id="lut01" name="test-lut3d" inBitDepth="32f" outBitDepth="10i" interpolation="tetrahedral">
{LUT3D_3_ARRAY}    </LUT3D>
</ProcessList>
"#
    );

    check_written(&expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut3d_inverse_clf)` @ v2.5.2.
#[test]
fn lut3d_inverse_clf() {
    let mut lut = lut3d_3();

    lut.set_direction(TransformDirection::Inverse);

    check_throw_what(
        write_lut_group(Transform::Lut3D(lut), true),
        "Transform uses the 'InverseLUT3D' op which cannot be written as CLF",
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, lut3d_inverse_ctf)` @ v2.5.2.
#[test]
fn lut3d_inverse_ctf() {
    let mut lut = lut3d_3();

    lut.set_direction(TransformDirection::Inverse);

    let output_transform = write_lut_group(Transform::Lut3D(lut), false).unwrap();

    // Note the type of the node.
    //
    // For an InverseLUT3D, the scaling of array values is based on the inBitDepth.
    //
    let expected = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.6" id="UIDLUT42">
    <InverseLUT3D id="lut01" name="test-lut3d" inBitDepth="10i" outBitDepth="32f">
{LUT3D_3_ARRAY}    </InverseLUT3D>
</ProcessList>
"#
    );

    check_written(&expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, save_lut3d_interpolation)` @ v2.5.2. Its config,
/// created and set to version 2, isn't used.
#[test]
fn save_lut3d_interpolation() {
    let mut lut = Lut3DTransform::new();
    lut.set_interpolation(Interpolation::Default);

    let mut output_transform = Vec::new();
    for (interpolation, expected) in [
        (
            Interpolation::Default,
            r#"<LUT3D inBitDepth="32f" outBitDepth="32f">"#,
        ),
        // INTERP_BEST is not a valid CTF/CLF method, so write its concrete value, which is
        // tetrahedral.
        (
            Interpolation::Best,
            r#"<LUT3D inBitDepth="32f" outBitDepth="32f" interpolation="tetrahedral">"#,
        ),
        (
            Interpolation::Linear,
            r#"<LUT3D inBitDepth="32f" outBitDepth="32f" interpolation="trilinear">"#,
        ),
        (
            Interpolation::Tetrahedral,
            r#"<LUT3D inBitDepth="32f" outBitDepth="32f" interpolation="tetrahedral">"#,
        ),
    ] {
        output_transform.clear();
        lut.set_interpolation(interpolation);
        let mut group = GroupTransform::new();
        group.append_transform(Transform::Lut3D(lut.clone()));
        write_group_ctf(&group, &mut output_transform).unwrap();

        let result = String::from_utf8(output_transform.clone()).unwrap();
        assert!(result.contains(expected), "{interpolation:?}");
    }

    output_transform.clear();
    lut.set_interpolation(Interpolation::Cubic);
    let mut group = GroupTransform::new();
    group.append_transform(Transform::Lut3D(lut));
    check_throw_what(
        write_group_ctf(&group, &mut output_transform),
        "Lut3D does not support interpolation algorithm: cubic",
    );
}

/// Port of `OCIO_ADD_TEST(CTFTransform, bitdepth_ctf)` @ v2.5.2.
#[test]
fn bitdepth_ctf() {
    let mut mat = MatrixTransform::new();
    mat.set_file_input_bit_depth(BitDepth::Uint8);
    mat.set_file_output_bit_depth(BitDepth::Uint10);

    let mut lut = Lut1DTransform::new();
    lut.set_interpolation(Interpolation::Default);
    lut.set_file_output_bit_depth(BitDepth::Uint10);
    lut.set_length(3).unwrap();

    let exp = ExponentTransform::new();

    let mut range = RangeTransform::new();
    range.set_file_input_bit_depth(BitDepth::F16);
    range.set_file_output_bit_depth(BitDepth::Uint12);
    range.set_min_in_value(0.1);
    range.set_min_out_value(-0.1);
    range.set_max_in_value(0.9);
    range.set_max_out_value(1.1);

    let log = LogTransform::new();

    let mut invlut = Lut1DTransform::new();
    invlut.set_direction(TransformDirection::Inverse);
    invlut.set_file_output_bit_depth(BitDepth::Uint16);
    invlut.set_length(3).unwrap();

    let mut mat2 = MatrixTransform::new();
    mat2.set_file_input_bit_depth(BitDepth::Uint8);
    mat2.set_file_output_bit_depth(BitDepth::Uint10);
    mat2.set_direction(TransformDirection::Inverse);

    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UID42"))
        .unwrap();

    // Transforms are setup as follows:
    //
    // Matrix    fileIn =  8i, fileOut = 10i
    // Lut1D                   fileOut = 10i
    // Exponent
    // Range     fileIn = 16f, fileOut = 12i
    // Matrix    fileIn =  8i, fileOut = 10i
    // Log
    // InvLut1D                fileOut = 16i
    // InvMatrix fileIn =  8i, fileOut = 10i
    // InvLut1D                fileOut = 16i

    // First op keeps its in & out bit-depth.
    // <Matrix inBitDepth="8i" outBitDepth="10i">
    group.append_transform(Transform::Matrix(mat.clone()));

    // Previous op out bit-depth used for in bit-depth.
    // <LUT1D inBitDepth="10i" outBitDepth="10i">
    group.append_transform(Transform::Lut1D(lut));

    // Previous op out bit-depth used for in bit-depth.
    // And next op (range) in bit-depth used for out bit-depth.
    // <Exponent inBitDepth="10i" outBitDepth="16f">
    group.append_transform(Transform::Exponent(exp));

    // In bit-depth preserved and has been used for out bit-depth of previous op.
    // Next op is a matrix, but current op is range, first op out bit-depth
    // is preserved and overrides the next op's in in bit-depth.
    // <Range inBitDepth="16f" outBitDepth="12i">
    group.append_transform(Transform::Range(range));

    // Previous op out bit-depth used for in bit-depth.
    // <Matrix inBitDepth="12i" outBitDepth="10i">
    group.append_transform(Transform::Matrix(mat));

    // Previous op out bit-depth used for in bit-depth. Out depth is set by preference
    // of the next op.
    // <Log inBitDepth="10i" outBitDepth="16i">
    group.append_transform(Transform::Log(log));

    // Preferred in bit-depth is preserved. Out depth is set by the next op.
    // <InverseLUT1D inBitDepth="16i" outBitDepth="32f">
    group.append_transform(Transform::Lut1D(invlut.clone()));

    // Sets both its preferred in and out depth. Note that because the transform
    // direction is inverse, the mapping of file depths is swapped.
    // <Matrix inBitDepth="10i" outBitDepth="8i">
    group.append_transform(Transform::Matrix(mat2));

    // This time it doesn't get its preferred in depth, since the previous op has priority.
    // The array values are scaled accordingly.
    // <InverseLUT1D inBitDepth="8i" outBitDepth="32f">
    group.append_transform(Transform::Lut1D(invlut));

    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UID42">
    <Matrix inBitDepth="8i" outBitDepth="10i">
        <Array dim="3 3">
   4.01176470588235                   0                   0
                  0    4.01176470588235                   0
                  0                   0    4.01176470588235
        </Array>
    </Matrix>
    <LUT1D inBitDepth="10i" outBitDepth="10i">
        <Array dim="3 1">
   0
511.5
 1023
        </Array>
    </LUT1D>
    <Exponent inBitDepth="10i" outBitDepth="16f" style="basicFwd">
        <ExponentParams exponent="1" />
    </Exponent>
    <Range inBitDepth="16f" outBitDepth="12i">
        <minInValue> 0.1 </minInValue>
        <maxInValue> 0.9 </maxInValue>
        <minOutValue> -409.5 </minOutValue>
        <maxOutValue> 4504.5 </maxOutValue>
    </Range>
    <Matrix inBitDepth="12i" outBitDepth="10i">
        <Array dim="3 3">
   0.24981684981685                   0                   0
                  0    0.24981684981685                   0
                  0                   0    0.24981684981685
        </Array>
    </Matrix>
    <Log inBitDepth="10i" outBitDepth="16i" style="log2">
    </Log>
    <InverseLUT1D inBitDepth="16i" outBitDepth="10i">
        <Array dim="3 1">
    0
32767.5
  65535
        </Array>
    </InverseLUT1D>
    <Matrix inBitDepth="10i" outBitDepth="8i">
        <Array dim="3 3">
  0.249266862170088                   0                   0
                  0   0.249266862170088                   0
                  0                   0   0.249266862170088
        </Array>
    </Matrix>
    <InverseLUT1D inBitDepth="8i" outBitDepth="32f">
        <Array dim="3 1">
  0
127.5
  255
        </Array>
    </InverseLUT1D>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// The group of `ff` with the id `UIDFF42`, written as CTF.
fn write_ff_group(ff: &FixedFunctionTransform) -> Vec<u8> {
    let group = group_of(b"UIDFF42", Transform::FixedFunction(ff.clone()));
    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();
    output_transform
}

/// Port of `OCIO_ADD_TEST(CTFTransform, fixed_function_aces_gamut_comp_13_ctf)` @ v2.5.2.
#[test]
fn fixed_function_aces_gamut_comp_13_ctf() {
    let data = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesGamutComp13, &data).unwrap();

    let output_transform = write_ff_group(&ff);

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.1" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="GamutComp13Fwd" params="1.147 1.264 1.312 0.815 0.803 0.88 1.2">
    </FixedFunction>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, fixed_function_aces_gamut_comp_13_inverse_ctf)` @ v2.5.2.
#[test]
fn fixed_function_aces_gamut_comp_13_inverse_ctf() {
    let data = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    let mut ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesGamutComp13, &data).unwrap();
    ff.set_direction(TransformDirection::Inverse);

    let output_transform = write_ff_group(&ff);

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.1" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="GamutComp13Rev" params="1.147 1.264 1.312 0.815 0.803 0.88 1.2">
    </FixedFunction>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, fixed_function_rec2100_ctf)` @ v2.5.2.
#[test]
fn fixed_function_rec2100_ctf() {
    let val = 0.5;
    let ff = FixedFunctionTransform::new(FixedFunctionStyle::Rec2100Surround, &[val]).unwrap();

    let output_transform = write_ff_group(&ff);

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="Rec2100SurroundFwd" params="0.5">
    </FixedFunction>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, fixed_function_rec2100_inverse_ctf)` @ v2.5.2.
#[test]
fn fixed_function_rec2100_inverse_ctf() {
    let val = 0.5;
    let mut ff = FixedFunctionTransform::new(FixedFunctionStyle::Rec2100Surround, &[val]).unwrap();
    ff.set_direction(TransformDirection::Inverse);

    let output_transform = write_ff_group(&ff);

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="Rec2100SurroundRev" params="0.5">
    </FixedFunction>
</ProcessList>
"#;

    check_written(expected, &output_transform);
}

/// Port of `OCIO_ADD_TEST(CTFTransform, fixed_function_lin_to_gammalog_ctf)` @ v2.5.2.
#[test]
fn fixed_function_lin_to_gammalog_ctf() {
    let vals = [
        0.0,
        0.25,
        0.5,
        1.0,
        0.0,
        2.718,
        0.17883277,
        0.807825590164,
        1.0,
        -0.07116723,
    ];
    let mut ff = FixedFunctionTransform::new(FixedFunctionStyle::LinToGammaLog, &vals).unwrap();

    {
        let output_transform = write_ff_group(&ff);

        let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.4" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="Lin_TO_GammaLog" params="0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723">
    </FixedFunction>
</ProcessList>
"#;

        check_written(expected, &output_transform);
    }
    {
        ff.set_direction(TransformDirection::Inverse);
        let output_transform = write_ff_group(&ff);

        let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.4" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="GammaLog_TO_Lin" params="0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723">
    </FixedFunction>
</ProcessList>
"#;

        check_written(expected, &output_transform);
    }
}

/// Port of `OCIO_ADD_TEST(CTFTransform, fixed_function_lin_to_doublelog_ctf)` @ v2.5.2.
#[test]
fn fixed_function_lin_to_doublelog_ctf() {
    let vals = [
        10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
    ];
    let mut ff = FixedFunctionTransform::new(FixedFunctionStyle::LinToDoubleLog, &vals).unwrap();

    {
        let output_transform = write_ff_group(&ff);

        let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.4" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="Lin_TO_DoubleLog" params="10 0.25 0.5 -1 0 -1 1.25 1 1 1 0.5 1 0">
    </FixedFunction>
</ProcessList>
"#;

        check_written(expected, &output_transform);
    }
    {
        ff.set_direction(TransformDirection::Inverse);
        let output_transform = write_ff_group(&ff);

        let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="2.4" id="UIDFF42">
    <FixedFunction inBitDepth="32f" outBitDepth="32f" style="DoubleLog_TO_Lin" params="10 0.25 0.5 -1 0 -1 1.25 1 1 1 0.5 1 0">
    </FixedFunction>
</ProcessList>
"#;

        check_written(expected, &output_transform);
    }
}

/// Reads a CTF file of one FixedFunction of `style` (and `params`) in the CTF `vers`, then
/// writes the group of its op's transform: the text read and the text written are the same.
///
/// Port of `ValidateFixedFunctionStyle` (FileFormatCTF_tests.cpp:3807-3866 @ v2.5.2).
#[track_caller]
fn validate_fixed_function_style(style: FixedFunctionOpStyle, vers: &str, params: &str) {
    // Validate the load & save for any FixedFunction style without parameters.

    let mut ff_str = format!(
        "<FixedFunction inBitDepth=\"32f\" outBitDepth=\"32f\" style=\"{}\"",
        style.to_str(false)
    );

    if !params.is_empty() {
        ff_str.push_str(&format!(" params=\"{params}\""));
    }
    ff_str.push('>');

    let strebuf = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <ProcessList version=\"{vers}\" id=\"ABCD\">\n    {ff_str}\n    </FixedFunction>\n\
         </ProcessList>\n"
    );

    // Test parsing.
    let cached_file = parse_string(&strebuf).unwrap();
    let file_ops = cached_file.transform.get_ops();

    assert_eq!(file_ops.len(), 1);
    let OpData::FixedFunction(ff_op_data) = &*file_ops[0] else {
        panic!("not a FixedFunction");
    };
    assert_eq!(ff_op_data.style(), style);

    let cloned_op_data = ff_op_data.clone();
    let mut ops = OpVec::new();
    ocio_ops::ops::fixedfunction::fixed_function_op::create_fixed_function_op_from_data(
        &mut ops,
        cloned_op_data,
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"ABCD"))
        .unwrap();
    let const_op = ops.iter().next().unwrap();
    crate::transforms::fixed_function_transform::create_fixed_function_transform(
        &mut group, const_op,
    )
    .unwrap();
    assert_eq!(group.num_transforms(), 1);

    // Test serialization.
    let mut output_transform = Vec::new();
    write_group_ctf(&group, &mut output_transform).unwrap();

    assert_text_eq(
        "written",
        &strebuf,
        std::str::from_utf8(&output_transform).unwrap(),
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, ff_load_save_ctf)` @ v2.5.2.
#[test]
fn ff_load_save_ctf() {
    use FixedFunctionOpStyle::*;
    validate_fixed_function_style(AcesRedMod03Fwd, "2", "");
    validate_fixed_function_style(AcesRedMod03Inv, "2", "");
    validate_fixed_function_style(AcesRedMod10Fwd, "2", "");
    validate_fixed_function_style(AcesRedMod10Inv, "2", "");
    validate_fixed_function_style(AcesGlow03Fwd, "2", "");
    validate_fixed_function_style(AcesGlow03Inv, "2", "");
    validate_fixed_function_style(AcesGlow10Fwd, "2", "");
    validate_fixed_function_style(AcesGlow10Inv, "2", "");
    validate_fixed_function_style(AcesDarkToDim10Fwd, "2", "");
    validate_fixed_function_style(AcesDarkToDim10Inv, "2", "");
    validate_fixed_function_style(
        AcesGamutComp13Fwd,
        "2.1",
        "1.147 1.264 1.312 0.815 0.803 0.88 1.2",
    );
    validate_fixed_function_style(Rec2100SurroundFwd, "2", "1");
    validate_fixed_function_style(Rec2100SurroundInv, "2", "1");
    validate_fixed_function_style(RgbToHsv, "2", "");
    validate_fixed_function_style(HsvToRgb, "2", "");
    validate_fixed_function_style(XyzToXyy, "2", "");
    validate_fixed_function_style(XyyToXyz, "2", "");
    validate_fixed_function_style(XyzToUvy, "2", "");
    validate_fixed_function_style(UvyToXyz, "2", "");
    validate_fixed_function_style(XyzToLuv, "2", "");
    validate_fixed_function_style(LuvToXyz, "2", "");
    validate_fixed_function_style(LinToPq, "2.4", "");
    validate_fixed_function_style(PqToLin, "2.4", "");
    validate_fixed_function_style(
        LinToGammaLog,
        "2.4",
        "0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723",
    );
    validate_fixed_function_style(
        GammaLogToLin,
        "2.4",
        "0 0.25 0.5 1 0 2.718 0.17883277 0.807825590164 1 -0.07116723",
    );
    validate_fixed_function_style(
        LinToDoubleLog,
        "2.4",
        "10 0.25 0.5 -1 0 -1 1.25 1 1 1 0.5 1 0",
    );
    validate_fixed_function_style(
        DoubleLogToLin,
        "2.4",
        "10 0.25 0.5 -1 0 -1 1.25 1 1 1 0.5 1 0",
    );
    validate_fixed_function_style(
        AcesOutputTransform20Fwd,
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    validate_fixed_function_style(
        AcesOutputTransform20Inv,
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    validate_fixed_function_style(
        AcesRgbToJmh20,
        "2.4",
        "0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    validate_fixed_function_style(
        AcesJmhToRgb20,
        "2.4",
        "0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    validate_fixed_function_style(AcesTonescaleCompress20Fwd, "2.4", "1000");
    validate_fixed_function_style(AcesTonescaleCompress20Inv, "2.4", "1000");
    validate_fixed_function_style(
        AcesGamutCompress20Fwd,
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    validate_fixed_function_style(
        AcesGamutCompress20Inv,
        "2.4",
        "1000 0.68 0.32 0.265 0.69 0.15 0.06 0.3127 0.329",
    );
    validate_fixed_function_style(RgbToHsyLog, "2.5", "");
    validate_fixed_function_style(HsyLogToRgb, "2.5", "");
    validate_fixed_function_style(RgbToHsyLin, "2.5", "");
    validate_fixed_function_style(HsyLinToRgb, "2.5", "");
    validate_fixed_function_style(RgbToHsyVid, "2.5", "");
    validate_fixed_function_style(HsyVidToRgb, "2.5", "");
}

///////////////////////////////////////////////////////////////////////////////
//
// BAKER TESTS
//
///////////////////////////////////////////////////////////////////////////////

/// The lines of `text`, as `StringUtils::SplitByLines` splits them.
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

/// A color space `name` of the family `name`.
fn bake_color_space(name: &str) -> crate::ColorSpace {
    let mut cs = crate::ColorSpace::new();
    cs.set_name(name);
    cs.set_family(name);
    cs
}

/// The config of the bake tests: an `input` reference space.
fn bake_input_config() -> Config {
    let mut config = Config::new().unwrap();
    let cs = bake_color_space("input");
    config.add_color_space(&cs).unwrap();
    config
        .set_role(ocio_ops::parse_utils::ROLE_REFERENCE, Some(cs.name()))
        .unwrap();
    config
}

/// A `target` color space whose saturation of 0.5 (in the CDL style `style`, when given)
/// causes channel crosstalk.
fn saturated_bake_target(style: Option<CdlStyle>) -> crate::ColorSpace {
    let mut cs = bake_color_space("target");

    // Set saturation to cause channel crosstalk, making a 3D LUT
    let mut transform1 = CdlTransform::new();
    if let Some(style) = style {
        transform1.set_style(style);
    }
    transform1.set_sat(f64::from(0.5f32));
    cs.set_transform(
        Some(&Transform::from(transform1)),
        ocio_ops::open_color_types::ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cs
}

/// The bake of `baker`.
fn baked(baker: &crate::Baker) -> Vec<u8> {
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();
    output
}

/// The ops of the CLF/CTF `text`, read back.
fn read_back(text: &[u8]) -> Vec<OpDataRcPtr> {
    let mut is = InputStream::from_bytes(text);
    let tester = LocalFileFormat;
    let file = tester.read_file(&mut is, b"").unwrap();
    file.transform.get_ops().to_vec()
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, bake_1d)` @ v2.5.2.
#[test]
fn bake_1d() {
    let mut config = bake_input_config();
    config.add_color_space(&bake_color_space("target")).unwrap();

    let mut baker = crate::Baker::new();
    baker.set_config(&config);
    baker.set_format(FILEFORMAT_CLF).unwrap();
    baker.set_input_space("input");
    baker.set_target_space("target");
    baker
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UID42"))
        .unwrap();
    baker.set_cube_size(2);
    let output_clf = baked(&baker);

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;
    assert_eq!(expected_clf.len(), output_clf.len());
    assert_eq!(expected_clf.as_bytes(), output_clf.as_slice());

    baker.set_format(FILEFORMAT_CTF).unwrap();
    let output_ctf = baked(&baker);
    let expected_ctf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList version="1.3" id="UID42">
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 1">
          0
          1
        </Array>
    </LUT1D>
</ProcessList>
"#;
    assert_eq!(expected_ctf.len(), output_ctf.len());
    assert_eq!(expected_ctf.as_bytes(), output_ctf.as_slice());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, bake_1d_shaper)` @ v2.5.2.
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

    let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();

    {
        // Lin to Log
        let mut baker = crate::Baker::new();
        baker.set_config(&config);
        baker.set_format(FILEFORMAT_CLF).unwrap();
        baker.set_input_space("Raw");
        baker.set_target_space("Log2");
        baker.set_shaper_space("Log2");
        baker
            .format_metadata_mut()
            .add_attribute(Some(METADATA_ID), Some(b"UID42"))
            .unwrap();
        baker.set_cube_size(10);
        let output_ctf = baked(&baker);

        let op_list = read_back(&output_ctf);
        assert_eq!(op_list.len(), 2);

        let OpData::Range(range) = &*op_list[0] else {
            panic!("a range");
        };
        assert_eq!(range.get_min_in_value(), 0.00198873621411622);
        assert_eq!(range.get_max_in_value(), 16.291877746582);
        assert_eq!(range.get_min_out_value(), 0.);
        assert_eq!(range.get_max_out_value(), 1.);

        let OpData::Lut1D(lut) = &*op_list[1] else {
            panic!("a LUT1D");
        };
        assert_eq!(lut.get_array().get_length(), 10);
        check_close(lut.get_array()[0], 0., 1.0e-5f32);
        check_close(lut.get_array()[3], 0.7562682, 1.0e-5f32);
        check_close(lut.get_array()[6], 0.83313024, 1.0e-5f32);
        check_close(lut.get_array()[9], 0.87810701, 1.0e-5f32);
        check_close(lut.get_array()[12], 0.9100228, 1.0e-5f32);
        check_close(lut.get_array()[15], 0.93478036, 1.0e-5f32);
        check_close(lut.get_array()[18], 0.9550097, 1.0e-5f32);
        check_close(lut.get_array()[21], 0.97211391, 1.0e-5f32);
        check_close(lut.get_array()[24], 0.98693061, 1.0e-5f32);
        check_close(lut.get_array()[27], 1., 1.0e-5f32);
    }

    {
        // Log to Lin
        let mut baker = crate::Baker::new();
        baker.set_config(&config);
        baker.set_format(FILEFORMAT_CLF).unwrap();
        baker.set_input_space("Log2");
        baker.set_target_space("Raw");
        baker
            .format_metadata_mut()
            .add_attribute(Some(METADATA_ID), Some(b"UID42"))
            .unwrap();
        baker.set_cube_size(10);
        let output_ctf = baked(&baker);

        let op_list = read_back(&output_ctf);
        assert_eq!(op_list.len(), 1);

        let OpData::Lut1D(lut) = &*op_list[0] else {
            panic!("a LUT1D");
        };
        assert_eq!(lut.get_array().get_length(), 10);
        check_close(lut.get_array()[0], 0.0019887362, 1.0e-5f32);
        check_close(lut.get_array()[3], 0.0054125111, 1.0e-5f32);
        check_close(lut.get_array()[6], 0.014730596, 1.0e-5f32);
        check_close(lut.get_array()[9], 0.040090535, 1.0e-5f32);
        check_close(lut.get_array()[12], 0.10910972, 1.0e-5f32);
        check_close(lut.get_array()[15], 0.29695117, 1.0e-5f32);
        check_close(lut.get_array()[18], 0.80817699, 1.0e-5f32);
        check_close(lut.get_array()[21], 2.1995215, 1.0e-5f32);
        check_close(lut.get_array()[24], 5.9861789, 1.0e-5f32);
        check_close(lut.get_array()[27], 16.291878, 1.0e-5f32);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, bake_3d)` @ v2.5.2.
#[test]
fn bake_3d() {
    let mut config = bake_input_config();
    config
        .add_color_space(&saturated_bake_target(None))
        .unwrap();

    let mut baker = crate::Baker::new();
    baker.set_config(&config);
    let data = baker.format_metadata_mut();
    data.add_attribute(Some(METADATA_ID), Some(b"TestID"))
        .unwrap();
    data.add_child_element(Some(METADATA_DESCRIPTION), Some(b"OpenColorIO Test Line 1"))
        .unwrap();
    data.add_child_element(Some(METADATA_DESCRIPTION), Some(b"OpenColorIO Test Line 2"))
        .unwrap();
    data.add_child_element(Some(b"Anything"), Some(b"Not Saved"))
        .unwrap();
    data.add_child_element(Some(METADATA_INPUT_DESCRIPTOR), Some(b"Input descriptor"))
        .unwrap();
    data.add_child_element(
        Some(METADATA_INPUT_DESCRIPTOR),
        Some(b"Only first is saved"),
    )
    .unwrap();
    data.add_child_element(Some(METADATA_OUTPUT_DESCRIPTOR), Some(b"Output descriptor"))
        .unwrap();
    data.add_child_element(Some(METADATA_INFO), Some(b""))
        .unwrap();
    let info = data.get_child_element_mut(6).unwrap();
    info.add_attribute(Some(b"attrib1"), Some(b"val1")).unwrap();
    info.add_attribute(Some(b"attrib2"), Some(b"val2")).unwrap();
    info.add_child_element(Some(b"anything"), Some(b"is saved"))
        .unwrap();
    info.add_child_element(Some(b"anything"), Some(b"is also saved"))
        .unwrap();

    baker.set_format(FILEFORMAT_CLF).unwrap();
    baker.set_input_space("input");
    baker.set_target_space("target");
    baker.set_cube_size(2);
    let output = baked(&baker);

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="TestID">
    <Description>OpenColorIO Test Line 1</Description>
    <Description>OpenColorIO Test Line 2</Description>
    <InputDescriptor>Input descriptor</InputDescriptor>
    <OutputDescriptor>Output descriptor</OutputDescriptor>
    <Info attrib1="val1" attrib2="val2">
        <anything>is saved</anything>
        <anything>is also saved</anything>
    </Info>
    <LUT3D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 2 2 3">
          0           0           0
     0.0361      0.0361  0.53609997
     0.3576  0.85759997      0.3576
     0.3937      0.8937      0.8937
     0.6063      0.1063      0.1063
 0.64240003      0.1424  0.64239997
 0.96389997  0.96389997      0.4639
          1           1           1
        </Array>
    </LUT3D>
</ProcessList>
"#;
    assert_eq!(expected_clf.len(), output.len());
    assert_eq!(expected_clf.as_bytes(), output.as_slice());
}

/// Port of `OCIO_ADD_TEST(FileFormatCTF, bake_1d_3d)` @ v2.5.2.
#[test]
fn bake_1d_3d() {
    let mut config = bake_input_config();
    {
        let mut cs = bake_color_space("shaper");
        let mut transform1 = MatrixTransform::new();
        let mat: [f64; 16] = [
            0.8, 0., 0., 0., 0., 0.8, 0., 0., 0., 0., 0.8, 0., 0., 0., 0., 1.,
        ];
        transform1.set_matrix(&mat);
        let offset: [f64; 4] = [0.1, 0.1, 0.1, 0.];
        transform1.set_offset(&offset);
        cs.set_transform(
            Some(&Transform::from(transform1)),
            ocio_ops::open_color_types::ColorSpaceDirection::FromReference,
        )
        .unwrap();
        config.add_color_space(&cs).unwrap();
    }
    config
        .add_color_space(&saturated_bake_target(Some(CdlStyle::Asc)))
        .unwrap();

    let mut baker = crate::Baker::new();
    baker.set_config(&config);
    baker.set_format(FILEFORMAT_CLF).unwrap();
    baker
        .format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(b"UID42"))
        .unwrap();
    baker.set_input_space("input");
    baker.set_shaper_space("shaper");
    baker.set_target_space("target");
    baker.set_cube_size(2);
    let output = baked(&baker);

    let op_list = read_back(&output);
    assert_eq!(op_list.len(), 2);
    let OpData::Lut1D(shaper_lut) = &*op_list[0] else {
        panic!("a LUT1D");
    };
    assert!(shaper_lut.is_input_half_domain());
    let shaper_array = shaper_lut.get_array();
    // Calculate the index for 0.5 in a half-domain LUT1D. We'll test the value there.
    let h05bits = float_to_half(0.5f32);
    let index = usize::from(h05bits) * 3;
    let res = 0.5f32 * 0.8f32 + 0.1f32;

    check_close(shaper_array[index], res, 1e-5f32);
    assert_eq!(shaper_array[index], shaper_array[index + 1]);
    assert_eq!(shaper_array[index], shaper_array[index + 2]);

    let OpData::Lut3D(lut) = &*op_list[1] else {
        panic!("a LUT3D");
    };
    assert_eq!(lut.get_array().get_length(), 2);
    assert_eq!(lut.get_array()[0], 0.0f32);
    assert_eq!(lut.get_array()[1], 0.0f32);
    assert_eq!(lut.get_array()[2], 0.0f32);
    check_close(lut.get_array()[3], 0.0361f32, 1.0e-5f32);
    check_close(lut.get_array()[4], 0.0361f32, 1.0e-5f32);
    check_close(lut.get_array()[5], 0.5361f32, 1.0e-5f32);
    check_close(lut.get_array()[6], 0.3576f32, 1.0e-5f32);
    check_close(lut.get_array()[7], 0.8576f32, 1.0e-5f32);
    check_close(lut.get_array()[8], 0.3576f32, 1.0e-5f32);
    check_close(lut.get_array()[9], 0.3937f32, 1.0e-5f32);
    check_close(lut.get_array()[10], 0.8937f32, 1.0e-5f32);
    check_close(lut.get_array()[11], 0.8937f32, 1.0e-5f32);
    check_close(lut.get_array()[12], 0.6063f32, 1.0e-5f32);
    check_close(lut.get_array()[13], 0.1063f32, 1.0e-5f32);
    check_close(lut.get_array()[14], 0.1063f32, 1.0e-5f32);
    check_close(lut.get_array()[15], 0.6424f32, 1.0e-5f32);
    check_close(lut.get_array()[16], 0.1424f32, 1.0e-5f32);
    check_close(lut.get_array()[17], 0.6424f32, 1.0e-5f32);
    check_close(lut.get_array()[18], 0.9639f32, 1.0e-5f32);
    check_close(lut.get_array()[19], 0.9639f32, 1.0e-5f32);
    check_close(lut.get_array()[20], 0.4639f32, 1.0e-5f32);
    assert_eq!(lut.get_array()[21], 1.0f32);
    assert_eq!(lut.get_array()[22], 1.0f32);
    assert_eq!(lut.get_array()[23], 1.0f32);

    baker.set_shaper_size(10);
    let output1 = baked(&baker);

    let expected_clf = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Range inBitDepth="32f" outBitDepth="32f">
        <minInValue> -0.125 </minInValue>
        <maxInValue> 1.125 </maxInValue>
        <minOutValue> 0 </minOutValue>
        <maxOutValue> 1 </maxOutValue>
    </Range>
    <LUT1D inBitDepth="32f" outBitDepth="32f">
        <Array dim="10 1">
          0
 0.11111112
 0.22222224
 0.33333334
 0.44444448
 0.55555558
 0.66666675
 0.77777779
 0.88888896
          1
        </Array>
    </LUT1D>
    <LUT3D inBitDepth="32f" outBitDepth="32f">
        <Array dim="2 2 2 3">
          0           0           0
     0.0361      0.0361  0.53609997
     0.3576  0.85759997      0.3576
     0.3937      0.8937      0.8937
     0.6063      0.1063      0.1063
 0.64240003      0.1424  0.64239997
 0.96389997  0.96389997      0.4639
          1           1           1
        </Array>
    </LUT3D>
</ProcessList>
"#;

    let osvec = lines(&output1);
    let resvec = lines(expected_clf.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        if (10..=19).contains(&i) || (24..=31).contains(&i) {
            check_str_float_vec_close(floats(&osvec[i]), floats(&resvec[i]), 1e-5f32);
        } else {
            assert_eq!(osvec[i], resvec[i]);
        }
    }
}
