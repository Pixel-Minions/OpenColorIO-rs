// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the cineSpace format: `tests/cpu/fileformats/FileFormatCSP_tests.cpp` @ v2.5.2.
//! Its `complete_3d`, `shaper_hdr` and `no_shaper` bake, and come with the baker (WP 4.8).

use super::*;
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
