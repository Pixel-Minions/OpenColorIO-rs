// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the spimtx format: `tests/cpu/fileformats/FileFormatSpiMtx_tests.cpp` @ v2.5.2.

use super::*;
use crate::fileformats::input_stream::OpenMode;
use crate::fileformats::test_utils::load_test_file;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(FileFormatSpiMtx, FormatInfo)` @ v2.5.2.
#[test]
fn format_info() {
    let tester = LocalFileFormat;
    let format_info_vec = tester.format_info();

    assert_eq!(1, format_info_vec.len());
    assert_eq!("spimtx", format_info_vec[0].name);
    assert_eq!("spimtx", format_info_vec[0].extension);
    assert_eq!(FormatCapabilities::READ, format_info_vec[0].capabilities);
}

/// Port of `LoadLutFile` (FileFormatSpiMtx_tests.cpp:27-33 @ v2.5.2).
fn load_lut_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Text, |stream, path| {
        LocalFileFormat.read_file(stream, path)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatSpiMtx, Test)` @ v2.5.2.
#[test]
// Upstream's literals, as written.
#[allow(clippy::excessive_precision)]
fn test() {
    let spi_mtx_file = "camera_to_aces.spimtx";
    let cached_file = load_lut_file(spi_mtx_file).unwrap();

    assert_eq!(0.0, cached_file.offset4[0]);
    assert_eq!(0.0, cached_file.offset4[1]);
    assert_eq!(0.0, cached_file.offset4[2]);
    assert_eq!(0.0, cached_file.offset4[3]);

    assert_eq!(0.754338638f32, cached_file.m44[0] as f32);
    assert_eq!(0.133697046f32, cached_file.m44[1] as f32);
    assert_eq!(0.111968437f32, cached_file.m44[2] as f32);
    assert_eq!(0.0, cached_file.m44[3]);

    assert_eq!(0.021198141f32, cached_file.m44[4] as f32);
    assert_eq!(1.005410934f32, cached_file.m44[5] as f32);
    assert_eq!(-0.026610548f32, cached_file.m44[6] as f32);
    assert_eq!(0.0, cached_file.m44[7]);

    assert_eq!(-0.009756991f32, cached_file.m44[8] as f32);
    assert_eq!(0.004508563f32, cached_file.m44[9] as f32);
    assert_eq!(1.005253201f32, cached_file.m44[10] as f32);
    assert_eq!(0.0, cached_file.m44[11]);

    assert_eq!(0.0, cached_file.m44[12]);
    assert_eq!(0.0, cached_file.m44[13]);
    assert_eq!(0.0, cached_file.m44[14]);
    assert_eq!(1.0, cached_file.m44[15]);
}

/// Port of `ReadSpiMtx` (FileFormatSpiMtx_tests.cpp:68-79 @ v2.5.2).
fn read_spi_mtx(file_content: &str) -> Result<LocalCachedFile> {
    let mut is = InputStream::from_bytes(file_content);

    // Read file
    let tester = LocalFileFormat;
    let sample_name = b"Memory File";
    tester.read_file(&mut is, sample_name)
}

/// Port of `OCIO_ADD_TEST(FileFormatSpiMtx, ReadOffset)` @ v2.5.2.
#[test]
fn read_offset() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_file = "1 0 0 6553.5\n\
                           0 1 0 32767.5\n\
                           0 0 1 65535.0\n";

        let cached_file = read_spi_mtx(sample_file).unwrap();
        assert_eq!(0.1, cached_file.offset4[0]);
        assert_eq!(0.5, cached_file.offset4[1]);
        assert_eq!(1.0, cached_file.offset4[2]);
        assert_eq!(0.0, cached_file.offset4[3]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatSpiMtx, ReadFailure)` @ v2.5.2.
#[test]
fn read_failure() {
    {
        // Validate stream can be read with no error.
        // Then stream will be altered to introduce errors.
        let sample_no_error = "1.0 0.0 0.0 0.0\n\
                               0.0 1.0 0.0 0.0\n\
                               0.0 0.0 1.0 0.0\n";

        assert!(read_spi_mtx(sample_no_error).is_ok());
    }
    {
        // Wrong number of elements
        let sample_error = "1.0 0.0 0.0\n\
                            0.0 1.0 0.0\n\
                            0.0 0.0 1.0\n";

        check_throw_what(
            read_spi_mtx(sample_error),
            "File must contain 12 float entries",
        );
    }
    {
        // Some elements can' t be read as float
        let sample_error = "1.0 0.0 0.0 0.0\n\
                            0.0 error 0.0 0.0\n\
                            0.0 0.0 1.0 0.0\n";

        check_throw_what(
            read_spi_mtx(sample_error),
            "File must contain all float entries",
        );
    }
}
