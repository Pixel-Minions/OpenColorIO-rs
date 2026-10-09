// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the CLF/CTF reader: `tests/cpu/fileformats/FileFormatCTF_tests.cpp` @ v2.5.2, the
//! tests the ported part of the reader reaches (WP 4.5b). The others come with the
//! operators' readers.

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use ocio_testkit::upstream::check_throw_what;

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
