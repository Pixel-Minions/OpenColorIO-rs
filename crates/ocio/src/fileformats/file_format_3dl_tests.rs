// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the `.3dl` format: `tests/cpu/fileformats/FileFormat3DL_tests.cpp` @ v2.5.2. Its
//! `bake` comes with the baker (WP 4.8).

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use ocio_testkit::upstream::check_throw_what;

/// Port of `LoadLutFile` (FileFormat3DL_tests.cpp:13-17 @ v2.5.2).
fn load_lut_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |stream, _path| {
        LocalFileFormat.read_file(stream, Interpolation::Default)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormat3DL, format_info)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(2, format_info_vec.len());
    assert_eq!("flame", format_info_vec[0].name);
    assert_eq!("lustre", format_info_vec[1].name);
    assert_eq!("3dl", format_info_vec[0].extension);
    assert_eq!("3dl", format_info_vec[1].extension);
    assert_eq!(
        FormatCapabilities::READ | FormatCapabilities::BAKE,
        format_info_vec[0].capabilities
    );
    assert_eq!(
        FormatCapabilities::READ | FormatCapabilities::BAKE,
        format_info_vec[1].capabilities
    );
}

/// Port of `OCIO_ADD_TEST(FileFormat3DL, get_likely_lut_bitdepth)` @ v2.5.2.
#[test]
fn get_likely_lut_bitdepth() {
    assert_eq!(get_likely_lut_bit_depth(-1), -1);

    assert_eq!(get_likely_lut_bit_depth(0), 8);
    assert_eq!(get_likely_lut_bit_depth(1), 8);
    assert_eq!(get_likely_lut_bit_depth(255), 8);
    assert_eq!(get_likely_lut_bit_depth(256), 8);
    assert_eq!(get_likely_lut_bit_depth(511), 8);

    assert_eq!(get_likely_lut_bit_depth(512), 10);
    assert_eq!(get_likely_lut_bit_depth(1023), 10);
    assert_eq!(get_likely_lut_bit_depth(1024), 10);
    assert_eq!(get_likely_lut_bit_depth(2047), 10);

    assert_eq!(get_likely_lut_bit_depth(2048), 12);
    assert_eq!(get_likely_lut_bit_depth(4095), 12);
    assert_eq!(get_likely_lut_bit_depth(4096), 12);
    assert_eq!(get_likely_lut_bit_depth(8191), 12);

    assert_eq!(get_likely_lut_bit_depth(16383), 16);

    assert_eq!(get_likely_lut_bit_depth(65535), 16);
    assert_eq!(get_likely_lut_bit_depth(65536), 16);
    assert_eq!(get_likely_lut_bit_depth(131071), 16);

    assert_eq!(get_likely_lut_bit_depth(131072), 16);
}

/// Port of `OCIO_ADD_TEST(FileFormat3DL, load)` @ v2.5.2.
#[test]
fn load() {
    // Discreet 3D LUT file
    let discree3_dt_lut = "discreet-3d-lut.3dl";

    let lut_file = load_lut_file(discree3_dt_lut).unwrap();

    assert!(lut_file.lut1d.is_none());
    let lut3d = lut_file.lut3d.as_ref().expect("a 3D LUT");

    assert_eq!(BitDepth::Uint12, lut3d.get_file_output_bit_depth());
    assert_eq!(17, lut3d.get_grid_size());

    let scale = get_bit_depth_max_value(BitDepth::Uint12).unwrap() as f32;
    // File and lut are using the same order.
    // 41: 54 323 597
    let lut_array = lut3d.get_array().get_values();
    assert_eq!(54.0f32, scale * lut_array[41 * 3]);
    assert_eq!(323.0f32, scale * lut_array[41 * 3 + 1]);
    assert_eq!(597.0f32, scale * lut_array[41 * 3 + 2]);

    // 4591: 4025 3426 0
    assert_eq!(4025.0f32, scale * lut_array[4591 * 3]);
    assert_eq!(3426.0f32, scale * lut_array[4591 * 3 + 1]);
    assert_eq!(0.0f32, scale * lut_array[4591 * 3 + 2]);

    let discree3_dt_lut_fail = "error_truncated_file.3dl";

    check_throw_what(
        load_lut_file(discree3_dt_lut_fail),
        "Cannot infer 3D LUT size",
    );
}

/// Port of `Read3dl` (FileFormat3DL_tests.cpp:184-194 @ v2.5.2).
fn read3dl(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file
    let tester = LocalFileFormat;
    tester.read_file(&mut is, Interpolation::Default)
}

/// Port of `OCIO_ADD_TEST(FileFormat3DL, parse_1d)` @ v2.5.2.
#[test]
fn parse_1d() {
    {
        // Rounding down test.
        let no3dlut = "#Tokens required by applications - do not edit\n\
            \n\
            3DMESH\n\
            Mesh 4 10\n\
            0 63 127 191 255 319 383 447 511 575 639 703 767 831 895 959 1023\n";

        let lut_file = read3dl(no3dlut).unwrap();

        assert!(lut_file.lut1d.is_none());
        assert!(lut_file.lut3d.is_none());
    }
    {
        // Rounding up test.
        let no3dlut = "#Tokens required by applications - do not edit\n\
            \n\
            3DMESH\n\
            Mesh 4 10\n\
            0 64 128 192 256 320 384 448 512 576 640 704 768 832 896 960 1023\n";

        let lut_file = read3dl(no3dlut).unwrap();

        assert!(lut_file.lut1d.is_none());
    }
    {
        // Not an identity test.
        let no3dlut = "#Tokens required by applications - do not edit\n\
            \n\
            3DMESH\n\
            Mesh 4 10\n\
            0 64 128 192 256 320 384 448 512 576 640 704 768 832 896 960 1020\n";

        let lut_file = read3dl(no3dlut).unwrap();

        let lut1d = lut_file.lut1d.as_ref().expect("a 1D LUT");
        assert_eq!(lut1d.get_file_output_bit_depth(), BitDepth::Uint10);
    }
}
