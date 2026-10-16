// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the cineSpace format: `tests/cpu/fileformats/FileFormatCSP_tests.cpp` @ v2.5.2.

use super::*;
use ocio_ops::utils::string_utils::split_by_lines;
use ocio_testkit::upstream::{check_close, check_throw_what};

/// Reads `text` as upstream's tests do (`tester.read(stream, "file.name", interp)`).
fn read_csp(text: &str, interp: Interpolation) -> Result<CachedFileCsp> {
    let mut stream = InputStream::from_bytes(text);
    LocalFileFormat.read_file(&mut stream, b"file.name", interp)
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, simple_1d)` @ v2.5.2.
#[test]
fn simple_1d() {
    let strebuf = [
        "CSPLUTV100\n",
        "1D\n",
        "\n",
        "BEGIN METADATA\n",
        "foobar\n",
        "END METADATA\n",
        "\n",
        "2\n",
        "0.0 1.0\n",
        "0.0 2.0\n",
        "6\n",
        "0.0 0.2 0.4 0.6 0.8 1.0\n",
        "0.0 0.4 0.8 1.2 1.6 2.0\n",
        "3\n",
        "0.0 0.1 1.0\n",
        "0.0 0.2 2.0\n",
        "\n",
        "6\n",
        "0.0 0.0 0.0\n",
        "0.2 0.3 0.1\n",
        "0.4 0.5 0.2\n",
        "0.5 0.6 0.3\n",
        "0.6 0.8 0.4\n",
        "1.0 0.9 0.5\n",
    ]
    .concat();

    let red = [0.0f32, 0.2, 0.4, 0.5, 0.6, 1.0];
    let green = [0.0f32, 0.3, 0.5, 0.6, 0.8, 0.9];
    let blue = [0.0f32, 0.1, 0.2, 0.3, 0.4, 0.5];

    // Read file.
    let csplut = read_csp(&strebuf, Interpolation::Default).unwrap();

    // Check metadata.
    assert_eq!(csplut.metadata, b"foobar\n");

    // Check prelut data.
    let prelut = csplut.prelut.as_ref().expect("a prelut");
    assert_eq!(prelut.get_file_output_bit_depth(), BitDepth::F32);

    let prelut_array = prelut.get_array();

    // Check prelut data (note: the spline is resampled into a 1D LUT).
    let length = prelut_array.get_length();
    let values = prelut_array.get_values();
    for i in (0..length).step_by(128) {
        let input = i as f32 / (length - 1) as f32;
        let output = values[i as usize * 3];
        check_close(input * 2.0f32, output, 1e-4);
    }

    // Check 1D data.
    let lut1d = csplut.lut1d.as_ref().expect("a 1D LUT");
    assert_eq!(lut1d.get_file_output_bit_depth(), BitDepth::F32);

    let lut_array = lut1d.get_array();
    let lutlength = lut_array.get_length() as usize;
    assert_eq!(lutlength, 6);
    let lut_array = lut_array.get_values();
    // Red.
    for i in 0..lutlength {
        assert_eq!(red[i], lut_array[i * 3]);
    }
    // Green.
    for i in 0..lutlength {
        assert_eq!(green[i], lut_array[i * 3 + 1]);
    }
    // Blue.
    for i in 0..lutlength {
        assert_eq!(blue[i], lut_array[i * 3 + 2]);
    }

    // Check 3D data.
    assert!(csplut.lut3d.is_none());
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, simple_3d)` @ v2.5.2.
#[test]
fn simple_3d() {
    let strebuf = [
        "CSPLUTV100\n",
        "3D\n",
        "\n",
        "BEGIN METADATA\n",
        "foobar\n",
        "END METADATA\n",
        "\n",
        "11\n",
        "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
        "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
        "6\n",
        "0.0 0.2       0.4 0.6 0.8 1.0\n",
        "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
        "5\n",
        "0.0 0.25       0.5 0.6 0.7\n",
        "0.0 0.25000001 0.5 0.6 0.7\n",
        "\n",
        "3 3 3\n",
        "0.0 0.0 0.0\n",
        "0.5 0.0 0.0\n",
        "1.0 0.0 0.0\n",
        "0.0 0.5 0.0\n",
        "0.5 0.5 0.0\n",
        "1.0 0.5 0.0\n",
        "0.0 1.0 0.0\n",
        "0.5 1.0 0.0\n",
        "1.0 1.0 0.0\n",
        "0.0 0.0 0.5\n",
        "0.5 0.0 0.5\n",
        "1.0 0.0 0.5\n",
        "0.0 0.5 0.5\n",
        "0.5 0.5 0.5\n",
        "1.0 0.5 0.5\n",
        "0.0 1.0 0.5\n",
        "0.5 1.0 0.5\n",
        "1.0 1.0 0.5\n",
        "0.0 0.0 1.0\n",
        "0.5 0.0 1.0\n",
        "1.0 0.0 1.0\n",
        "0.0 0.5 1.0\n",
        "0.5 0.5 1.0\n",
        "1.0 0.5 1.0\n",
        "0.0 1.0 1.0\n",
        "0.5 1.0 1.0\n",
        "1.0 1.0 1.0\n",
    ]
    .concat();

    let cube = [
        0.0f32, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.5, 0.0, 0.5,
        1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.5, 0.0, 1.0, 1.0, 0.5, 0.0, 0.0, 0.5, 0.0, 0.5, 0.5, 0.0,
        1.0, 0.5, 0.5, 0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 1.0, 0.5, 1.0, 0.0, 0.5, 1.0, 0.5, 0.5, 1.0,
        1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.5, 1.0, 0.0, 1.0, 1.0, 0.5, 0.0, 1.0, 0.5, 0.5, 1.0, 0.5,
        1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.5, 1.0, 1.0, 1.0,
    ];

    // Load file.
    let csplut = read_csp(&strebuf, Interpolation::Tetrahedral).unwrap();

    // Check metadata.
    assert_eq!(csplut.metadata, b"foobar\n");

    // Check prelut data.
    // As in & out preLut values are the same there is nothing to do.
    assert!(csplut.prelut.is_none());

    // Check cube data.
    let lut3d = csplut.lut3d.as_ref().expect("a 3D LUT");
    assert_eq!(lut3d.get_interpolation(), Interpolation::Tetrahedral);
    let lut_array = lut3d.get_array();
    let lutlength = lut_array.get_length() as usize;
    let values = lut_array.get_values();

    for i in 0..lutlength {
        assert_eq!(cube[i], values[i]);
    }

    // Check 1D data.
    assert!(csplut.lut1d.is_none());
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, less_strict_parse)` @ v2.5.2.
#[test]
fn less_strict_parse() {
    let strebuf = [
        " CspluTV100 malformed\n",
        "3D\n",
        "\n",
        " BegIN MEtadATA malformed malformed malfo\n",
        "foobar\n",
        "   end metadata malformed malformed m a l\n",
        "\n",
        "11\n",
        "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
        "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
        "6\n",
        "0.0 0.2       0.4 0.6 0.8 1.0\n",
        "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
        "5\n",
        "0.0 0.25       0.5 0.6 0.7\n",
        "0.0 0.25000001 0.5 0.6 0.7\n",
        "\n",
        "2 2 2\n",
        "0.100000 0.100000 0.100000\n",
        "1.100000 0.100000 0.100000\n",
        "0.100000 1.100000 0.100000\n",
        "1.100000 1.100000 0.100000\n",
        "0.100000 0.100000 1.100000\n",
        "1.100000 0.100000 1.100000\n",
        "0.100000 1.100000 1.100000\n",
        "1.100000 1.100000 1.100000\n",
    ]
    .concat();

    // Load file.
    let csplut = read_csp(&strebuf, Interpolation::Default).unwrap();

    // Check metadata.
    assert_eq!(csplut.metadata, b"foobar\n");

    // Check prelut data.
    // As in & out from the preLut are the same, there is nothing to do.
    assert!(csplut.prelut.is_none());
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, failures_1d)` @ v2.5.2.
#[test]
fn failures_1d() {
    {
        // Empty.
        let lut = String::new();

        check_throw_what(read_csp(&lut, Interpolation::Default), "file stream empty");
    }
    {
        // Wrong first line.
        let lut = ["CSPLUTV2000\n", "1D\n", "\n"].concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "expected 'CSPLUTV100'",
        );
    }
    {
        // Missing LUT.
        let lut = [
            "CSPLUTV100\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
        ]
        .concat();

        check_throw_what(read_csp(&lut, Interpolation::Default), "Require 1D or 3D");
    }
    {
        // Can't read prelut size.
        let lut = [
            "CSPLUTV100\n",
            "1D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "A\n",
            "0.0 1.0\n",
            "0.0 2.0\n",
            "6\n",
            "0.0 0.2 0.4 0.6 0.8 1.0\n",
            "0.0 0.4 0.8 1.2 1.6 2.0\n",
            "3\n",
            "0.0 0.1 1.0\n",
            "0.0 0.2 2.0\n",
            "\n",
            "6\n",
            "0.0 0.0 0.0\n",
            "0.2 0.3 0.1\n",
            "0.4 0.5 0.2\n",
            "0.5 0.6 0.3\n",
            "0.6 0.8 0.4\n",
            "1.0 0.9 0.5\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "Prelut does not specify valid dimension size",
        );
    }
    {
        // Prelut has too many points.
        let lut = [
            "CSPLUTV100\n",
            "1D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "2\n",
            "0.0 1.0 1.0\n",
            "0.0 2.0\n",
            "6\n",
            "0.0 0.2 0.4 0.6 0.8 1.0\n",
            "0.0 0.4 0.8 1.2 1.6 2.0\n",
            "3\n",
            "0.0 0.1 1.0\n",
            "0.0 0.2 2.0\n",
            "\n",
            "6\n",
            "0.0 0.0 0.0\n",
            "0.2 0.3 0.1\n",
            "0.4 0.5 0.2\n",
            "0.5 0.6 0.3\n",
            "0.6 0.8 0.4\n",
            "1.0 0.9 0.5\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "expected number of data points",
        );
    }
    {
        // Can't read a float in prelut.
        let lut = [
            "CSPLUTV100\n",
            "1D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "2\n",
            "0.0 notFloat\n",
            "0.0 2.0\n",
            "6\n",
            "0.0 0.2 0.4 0.6 0.8 1.0\n",
            "0.0 0.4 0.8 1.2 1.6 2.0\n",
            "3\n",
            "0.0 0.1 1.0\n",
            "0.0 0.2 2.0\n",
            "\n",
            "6\n",
            "0.0 0.0 0.0\n",
            "0.2 0.3 0.1\n",
            "0.4 0.5 0.2\n",
            "0.5 0.6 0.3\n",
            "0.6 0.8 0.4\n",
            "1.0 0.9 0.5\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "Prelut data is malformed",
        );
    }
    {
        // Bad number of LUT entries.
        let lut = [
            "CSPLUTV100\n",
            "1D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "2\n",
            "0.0 1.0\n",
            "0.0 2.0\n",
            "6\n",
            "0.0 0.2 0.4 0.6 0.8 1.0\n",
            "0.0 0.4 0.8 1.2 1.6 2.0\n",
            "3\n",
            "0.0 0.1 1.0\n",
            "0.0 0.2 2.0\n",
            "\n",
            "-6\n",
            "0.0 0.0 0.0\n",
            "0.2 0.3 0.1\n",
            "0.4 0.5 0.2\n",
            "0.5 0.6 0.3\n",
            "0.6 0.8 0.4\n",
            "1.0 0.9 0.5\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "1D LUT with invalid number of entries",
        );
    }
    {
        // Too many components on LUT entry.
        let lut = [
            "CSPLUTV100\n",
            "1D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "2\n",
            "0.0 1.0\n",
            "0.0 2.0\n",
            "6\n",
            "0.0 0.2 0.4 0.6 0.8 1.0\n",
            "0.0 0.4 0.8 1.2 1.6 2.0\n",
            "3\n",
            "0.0 0.1 1.0\n",
            "0.0 0.2 2.0\n",
            "\n",
            "6\n",
            "0.0 0.0 0.0 0.0\n",
            "0.2 0.3 0.1\n",
            "0.4 0.5 0.2\n",
            "0.5 0.6 0.3\n",
            "0.6 0.8 0.4\n",
            "1.0 0.9 0.5\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "must contain three numbers",
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, failures_3d)` @ v2.5.2.
#[test]
fn failures_3d() {
    {
        // Cube size has only 2 entries.
        let lut = [
            "CSPLUTV100\n",
            "3D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "11\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "6\n",
            "0.0 0.2       0.4 0.6 0.8 1.0\n",
            "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
            "5\n",
            "0.0 0.25       0.5 0.6 0.7\n",
            "0.0 0.25000001 0.5 0.6 0.7\n",
            "\n",
            "3 3\n",
            "0.0 0.0 0.0\n",
            "0.5 0.0 0.0\n",
            "1.0 0.0 0.0\n",
            "0.0 0.5 0.0\n",
            "0.5 0.5 0.0\n",
            "1.0 0.5 0.0\n",
            "0.0 1.0 0.0\n",
            "0.5 1.0 0.0\n",
            "1.0 1.0 0.0\n",
            "0.0 0.0 0.5\n",
            "0.5 0.0 0.5\n",
            "1.0 0.0 0.5\n",
            "0.0 0.5 0.5\n",
            "0.5 0.5 0.5\n",
            "1.0 0.5 0.5\n",
            "0.0 1.0 0.5\n",
            "0.5 1.0 0.5\n",
            "1.0 1.0 0.5\n",
            "0.0 0.0 1.0\n",
            "0.5 0.0 1.0\n",
            "1.0 0.0 1.0\n",
            "0.0 0.5 1.0\n",
            "0.5 0.5 1.0\n",
            "1.0 0.5 1.0\n",
            "0.0 1.0 1.0\n",
            "0.5 1.0 1.0\n",
            "1.0 1.0 1.0\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "couldn't read cube size",
        );
    }
    {
        // Cube sizes are not equal.
        let lut = [
            "CSPLUTV100\n",
            "3D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "11\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "6\n",
            "0.0 0.2       0.4 0.6 0.8 1.0\n",
            "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
            "5\n",
            "0.0 0.25       0.5 0.6 0.7\n",
            "0.0 0.25000001 0.5 0.6 0.7\n",
            "\n",
            "3 3 4\n",
            "0.0 0.0 0.0\n",
            "0.5 0.0 0.0\n",
            "1.0 0.0 0.0\n",
            "0.0 0.5 0.0\n",
            "0.5 0.5 0.0\n",
            "1.0 0.5 0.0\n",
            "0.0 1.0 0.0\n",
            "0.5 1.0 0.0\n",
            "1.0 1.0 0.0\n",
            "0.0 0.0 0.5\n",
            "0.5 0.0 0.5\n",
            "1.0 0.0 0.5\n",
            "0.0 0.5 0.5\n",
            "0.5 0.5 0.5\n",
            "1.0 0.5 0.5\n",
            "0.0 1.0 0.5\n",
            "0.5 1.0 0.5\n",
            "1.0 1.0 0.5\n",
            "0.0 0.0 1.0\n",
            "0.5 0.0 1.0\n",
            "1.0 0.0 1.0\n",
            "0.0 0.5 1.0\n",
            "0.5 0.5 1.0\n",
            "1.0 0.5 1.0\n",
            "0.0 1.0 1.0\n",
            "0.5 1.0 1.0\n",
            "1.0 1.0 1.0\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "nonuniform cube sizes",
        );
    }
    {
        // Cube size is not >0.
        let lut = [
            "CSPLUTV100\n",
            "3D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "11\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "6\n",
            "0.0 0.2       0.4 0.6 0.8 1.0\n",
            "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
            "5\n",
            "0.0 0.25       0.5 0.6 0.7\n",
            "0.0 0.25000001 0.5 0.6 0.7\n",
            "\n",
            "-3 -3 -3\n",
            "0.0 0.0 0.0\n",
            "0.5 0.0 0.0\n",
            "1.0 0.0 0.0\n",
            "0.0 0.5 0.0\n",
            "0.5 0.5 0.0\n",
            "1.0 0.5 0.0\n",
            "0.0 1.0 0.0\n",
            "0.5 1.0 0.0\n",
            "1.0 1.0 0.0\n",
            "0.0 0.0 0.5\n",
            "0.5 0.0 0.5\n",
            "1.0 0.0 0.5\n",
            "0.0 0.5 0.5\n",
            "0.5 0.5 0.5\n",
            "1.0 0.5 0.5\n",
            "0.0 1.0 0.5\n",
            "0.5 1.0 0.5\n",
            "1.0 1.0 0.5\n",
            "0.0 0.0 1.0\n",
            "0.5 0.0 1.0\n",
            "1.0 0.0 1.0\n",
            "0.0 0.5 1.0\n",
            "0.5 0.5 1.0\n",
            "1.0 0.5 1.0\n",
            "0.0 1.0 1.0\n",
            "0.5 1.0 1.0\n",
            "1.0 1.0 1.0\n",
        ]
        .concat();

        check_throw_what(read_csp(&lut, Interpolation::Default), "invalid cube size");
    }
    {
        // One LUT entry has 4 components.
        let lut = [
            "CSPLUTV100\n",
            "3D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "11\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "6\n",
            "0.0 0.2       0.4 0.6 0.8 1.0\n",
            "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
            "5\n",
            "0.0 0.25       0.5 0.6 0.7\n",
            "0.0 0.25000001 0.5 0.6 0.7\n",
            "\n",
            "3 3 3\n",
            "0.0 0.0 0.0\n",
            "0.5 0.0 0.0\n",
            "1.0 0.0 0.0\n",
            "0.0 0.5 0.0\n",
            "0.5 0.5 0.0 1.0\n",
            "1.0 0.5 0.0\n",
            "0.0 1.0 0.0\n",
            "0.5 1.0 0.0\n",
            "1.0 1.0 0.0\n",
            "0.0 0.0 0.5\n",
            "0.5 0.0 0.5\n",
            "1.0 0.0 0.5\n",
            "0.0 0.5 0.5\n",
            "0.5 0.5 0.5\n",
            "1.0 0.5 0.5\n",
            "0.0 1.0 0.5\n",
            "0.5 1.0 0.5\n",
            "1.0 1.0 0.5\n",
            "0.0 0.0 1.0\n",
            "0.5 0.0 1.0\n",
            "1.0 0.0 1.0\n",
            "0.0 0.5 1.0\n",
            "0.5 0.5 1.0\n",
            "1.0 0.5 1.0\n",
            "0.0 1.0 1.0\n",
            "0.5 1.0 1.0\n",
            "1.0 1.0 1.0\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "couldn't read cube row",
        );
    }
    {
        // One LUT entry has 2 components.
        let lut = [
            "CSPLUTV100\n",
            "3D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "11\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "6\n",
            "0.0 0.2       0.4 0.6 0.8 1.0\n",
            "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
            "5\n",
            "0.0 0.25       0.5 0.6 0.7\n",
            "0.0 0.25000001 0.5 0.6 0.7\n",
            "\n",
            "3 3 3\n",
            "0.0 0.0 0.0\n",
            "0.5 0.0 0.0\n",
            "1.0 0.0 0.0\n",
            "0.0 0.5 0.0\n",
            "0.5 0.5 0.0\n",
            "1.0 0.5 0.0\n",
            "0.0 1.0 0.0\n",
            "0.5 1.0 0.0\n",
            "1.0 1.0\n",
            "0.0 0.0 0.5\n",
            "0.5 0.0 0.5\n",
            "1.0 0.0 0.5\n",
            "0.0 0.5 0.5\n",
            "0.5 0.5 0.5\n",
            "1.0 0.5 0.5\n",
            "0.0 1.0 0.5\n",
            "0.5 1.0 0.5\n",
            "1.0 1.0 0.5\n",
            "0.0 0.0 1.0\n",
            "0.5 0.0 1.0\n",
            "1.0 0.0 1.0\n",
            "0.0 0.5 1.0\n",
            "0.5 0.5 1.0\n",
            "1.0 0.5 1.0\n",
            "0.0 1.0 1.0\n",
            "0.5 1.0 1.0\n",
            "1.0 1.0 1.0\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "couldn't read cube row",
        );
    }
    {
        // One LUT entry can't be converted to 3 floats.
        let lut = [
            "CSPLUTV100\n",
            "3D\n",
            "\n",
            "BEGIN METADATA\n",
            "foobar\n",
            "END METADATA\n",
            "\n",
            "11\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "0.0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1.0\n",
            "6\n",
            "0.0 0.2       0.4 0.6 0.8 1.0\n",
            "0.0 0.2000000 0.4 0.6 0.8 1.0\n",
            "5\n",
            "0.0 0.25       0.5 0.6 0.7\n",
            "0.0 0.25000001 0.5 0.6 0.7\n",
            "\n",
            "3 3 3\n",
            "0.0 0.0 0.0\n",
            "0.5 0.0 0.0\n",
            "1.0 0.0 0.0\n",
            "0.0 0.5 0.0\n",
            "0.5 0.5 0.0\n",
            "1.0 0.5 One\n",
            "0.0 1.0 0.0\n",
            "0.5 1.0 0.0\n",
            "1.0 1.0 0.0\n",
            "0.0 0.0 0.5\n",
            "0.5 0.0 0.5\n",
            "1.0 0.0 0.5\n",
            "0.0 0.5 0.5\n",
            "0.5 0.5 0.5\n",
            "1.0 0.5 0.5\n",
            "0.0 1.0 0.5\n",
            "0.5 1.0 0.5\n",
            "1.0 1.0 0.5\n",
            "0.0 0.0 1.0\n",
            "0.5 0.0 1.0\n",
            "1.0 0.0 1.0\n",
            "0.0 0.5 1.0\n",
            "0.5 0.5 1.0\n",
            "1.0 0.5 1.0\n",
            "0.0 1.0 1.0\n",
            "0.5 1.0 1.0\n",
            "1.0 1.0 1.0\n",
        ]
        .concat();

        check_throw_what(
            read_csp(&lut, Interpolation::Default),
            "couldn't read cube row",
        );
    }
}

/// A metadata block the file never closes is an error: upstream reads past the file's end
/// forever (docs/improvements.md, U-72). The block ending with the file, with or without a
/// line feed, and a block of a single line.
#[test]
fn a_metadata_block_never_closed_is_an_error() {
    for text in [
        "CSPLUTV100\n3D\nBEGIN METADATA\nfoobar\n",
        "CSPLUTV100\n3D\nBEGIN METADATA\nfoobar",
        "CSPLUTV100\n3D\nBEGIN METADATA",
        "CSPLUTV100\n3D\nBEGIN METADATA\n2\n0 1\n0 1\n2\n0 1\n0 1\n2\n0 1\n0 1\n2 2 2\n",
    ] {
        check_throw_what(
            read_csp(text, Interpolation::Default),
            "Metadata block of csp LUT file.name is not closed.",
        );
    }
}

/// The floats of two lines are close: upstream's `compareFloats`.
///
/// Port of `compareFloats` (FileFormatCSP_tests.cpp:14-30 @ v2.5.2).
fn compare_floats(floats1: &[u8], floats2: &[u8]) {
    // Number comparison.
    let strings1 = split_by_white_spaces(trim(floats1));
    let mut numbers1 = Vec::new();
    string_vec_to_float_vec(&mut numbers1, &strings1);

    let strings2 = split_by_white_spaces(trim(floats2));
    let mut numbers2 = Vec::new();
    string_vec_to_float_vec(&mut numbers2, &strings2);

    assert_eq!(numbers1.len(), numbers2.len());
    for j in 0..numbers1.len() {
        check_close(numbers1[j], numbers2[j], 1e-5f32);
    }
}

/// A config with the `lnf` reference space, as the bake tests start.
fn lnf_config() -> crate::Config {
    let mut config = crate::Config::new().unwrap();
    let mut cs = crate::ColorSpace::new();
    cs.set_name("lnf");
    cs.set_family("lnf");
    config.add_color_space(&cs).unwrap();
    config
        .set_role(ocio_ops::parse_utils::ROLE_REFERENCE, Some(cs.name()))
        .unwrap();
    config
}

/// A color space `name` of a CDL with the offset `rgb`, from the reference.
fn offset_space(name: &str, rgb: [f64; 3]) -> crate::ColorSpace {
    let mut cs = crate::ColorSpace::new();
    cs.set_name(name);
    cs.set_family(name);
    let mut transform1 = crate::CdlTransform::new();
    transform1.set_offset(&rgb);
    cs.set_transform(
        Some(&crate::Transform::from(transform1)),
        ocio_ops::open_color_types::ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cs
}

/// Adds a description child to the baker's metadata.
fn describe(baker: &mut Baker, text: &[u8]) {
    baker
        .format_metadata_mut()
        .add_child_element(
            Some(ocio_ops::format_metadata::METADATA_DESCRIPTION),
            Some(text),
        )
        .unwrap();
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, complete_3d)` @ v2.5.2.
#[test]
fn complete_3d() {
    // Check baker output.
    let mut config = lnf_config();
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("shaper");
        cs.set_family("shaper");
        let mut transform1 = crate::ExponentTransform::new();
        let test: [f64; 4] = [2.6, 2.6, 2.6, 1.0];
        transform1.set_value(&test);
        cs.set_transform(
            Some(&crate::Transform::from(transform1)),
            ocio_ops::open_color_types::ColorSpaceDirection::ToReference,
        )
        .unwrap();
        config.add_color_space(&cs).unwrap();
    }
    config
        .add_color_space(&offset_space("target", [0.1, 0.1, 0.1]))
        .unwrap();

    let bout = concat!(
        "CSPLUTV100\n",
        "3D\n",
        "\n",
        "BEGIN METADATA\n",
        "date: 2011:02:21 15:22:55\n",
        "Baked by OCIO\n",
        "END METADATA\n",
        "\n",
        "10\n",
        "0.000000 0.003303 0.020028 0.057476 0.121430 0.216916 0.348468 0.520265 0.736213 1.000000\n",
        "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000\n",
        "10\n",
        "0.000000 0.003303 0.020028 0.057476 0.121430 0.216916 0.348468 0.520265 0.736213 1.000000\n",
        "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000\n",
        "10\n",
        "0.000000 0.003303 0.020028 0.057476 0.121430 0.216916 0.348468 0.520265 0.736213 1.000000\n",
        "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000\n",
        "\n",
        "2 2 2\n",
        "0.100000 0.100000 0.100000\n",
        "1.100000 0.100000 0.100000\n",
        "0.100000 1.100000 0.100000\n",
        "1.100000 1.100000 0.100000\n",
        "0.100000 0.100000 1.100000\n",
        "1.100000 0.100000 1.100000\n",
        "0.100000 1.100000 1.100000\n",
        "1.100000 1.100000 1.100000\n",
        "\n",
    );

    let mut baker = Baker::new();
    baker.set_config(&config);
    describe(&mut baker, b"date: 2011:02:21 15:22:55");
    describe(&mut baker, b"Baked by OCIO");
    baker.set_format("cinespace").unwrap();
    baker.set_input_space("lnf");
    baker.set_shaper_space("shaper");
    baker.set_target_space("target");
    baker.set_shaper_size(10);
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = split_by_lines(&output);
    let resvec = split_by_lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        if i > 6 {
            // Number comparison.
            compare_floats(&osvec[i], &resvec[i]);
        } else {
            // text comparison
            assert_eq!(osvec[i], resvec[i]);
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, shaper_hdr)` @ v2.5.2.
#[test]
fn shaper_hdr() {
    // Check baker output.
    let mut config = lnf_config();
    config
        .add_color_space(&offset_space("lnf_tweak", [2.0, -2.0, 0.9]))
        .unwrap();
    config
        .add_color_space(&offset_space("target", [0.1, 0.1, 0.1]))
        .unwrap();

    let bout = concat!(
        "CSPLUTV100\n",
        "3D\n",
        "\n",
        "BEGIN METADATA\n",
        "date: 2011:02:21 15:22:55\n",
        "END METADATA\n",
        "\n",
        "10\n",
        "2.000000 2.111111 2.222222 2.333333 2.444444 2.555556 2.666667 2.777778 2.888889 3.000000\n",
        "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000\n",
        "10\n",
        "-2.000000 -1.888889 -1.777778 -1.666667 -1.555556 -1.444444 -1.333333 -1.222222 -1.111111 -1.000000\n",
        "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000\n",
        "10\n",
        "0.900000 1.011111 1.122222 1.233333 1.344444 1.455556 1.566667 1.677778 1.788889 1.900000\n",
        "0.000000 0.111111 0.222222 0.333333 0.444444 0.555556 0.666667 0.777778 0.888889 1.000000\n",
        "\n",
        "2 2 2\n",
        "0.100000 0.100000 0.100000\n",
        "1.100000 0.100000 0.100000\n",
        "0.100000 1.100000 0.100000\n",
        "1.100000 1.100000 0.100000\n",
        "0.100000 0.100000 1.100000\n",
        "1.100000 0.100000 1.100000\n",
        "0.100000 1.100000 1.100000\n",
        "1.100000 1.100000 1.100000\n",
        "\n",
    );

    let mut baker = Baker::new();
    baker.set_config(&config);
    describe(&mut baker, b"date: 2011:02:21 15:22:55");
    baker.set_format("cinespace").unwrap();
    baker.set_input_space("lnf_tweak");
    baker.set_shaper_space("lnf");
    baker.set_target_space("target");
    baker.set_shaper_size(10);
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = split_by_lines(&output);
    let resvec = split_by_lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        if i > 6 {
            // Number comparison.
            compare_floats(&osvec[i], &resvec[i]);
        } else {
            // text comparison
            assert_eq!(osvec[i], resvec[i]);
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatCSP, no_shaper)` @ v2.5.2.
#[test]
fn no_shaper() {
    // Check baker output.
    let mut config = lnf_config();
    config
        .add_color_space(&offset_space("target", [0.1, 0.1, 0.1]))
        .unwrap();

    let bout = concat!(
        "CSPLUTV100\n",
        "3D\n",
        "\n",
        "BEGIN METADATA\n",
        "date: 2011:02:21 15:22:55\n",
        "END METADATA\n",
        "\n",
        "2\n",
        "0.000000 1.000000\n",
        "0.000000 1.000000\n",
        "2\n",
        "0.000000 1.000000\n",
        "0.000000 1.000000\n",
        "2\n",
        "0.000000 1.000000\n",
        "0.000000 1.000000\n",
        "\n",
        "2 2 2\n",
        "0.100000 0.100000 0.100000\n",
        "1.100000 0.100000 0.100000\n",
        "0.100000 1.100000 0.100000\n",
        "1.100000 1.100000 0.100000\n",
        "0.100000 0.100000 1.100000\n",
        "1.100000 0.100000 1.100000\n",
        "0.100000 1.100000 1.100000\n",
        "1.100000 1.100000 1.100000\n",
        "\n",
    );

    let mut baker = Baker::new();
    baker.set_config(&config);
    describe(&mut baker, b"date: 2011:02:21 15:22:55");
    baker.set_format("cinespace").unwrap();
    baker.set_input_space("lnf");
    baker.set_target_space("target");
    baker.set_shaper_size(10);
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = split_by_lines(&output);
    let resvec = split_by_lines(bout.as_bytes());

    assert_eq!(osvec.len(), resvec.len());
    for i in 0..resvec.len() {
        assert_eq!(osvec[i], resvec[i]);
    }
}
