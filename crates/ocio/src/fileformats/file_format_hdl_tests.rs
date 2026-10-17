// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Houdini LUT format: `tests/cpu/fileformats/FileFormatHDL_tests.cpp` @ v2.5.2.
//! Its `bake_1d`, `bake_1d_shaper`, `bake_3d`, `bake_3d_1d` and `look_test` bake, and come
//! with the baker (WP 4.8).

use super::*;

/// Reads `text` as upstream's tests do (`tester.read(stream, "", INTERP_DEFAULT)`).
fn read(text: &str) -> CachedFileHdl {
    let mut stream = InputStream::from_bytes(text);
    LocalFileFormat
        .read_file(&mut stream, Interpolation::Default)
        .unwrap()
}

/// Checks a 3D LUT against `cube`, in Houdini's red-fastest order.
fn check_cube(lut3d: &Lut3DOpData, cube: &[f32]) {
    let lut_array = lut3d.get_array();
    let lut_size = lut_array.get_length() as usize;
    assert_eq!(2, lut_size);
    let lut_array = lut_array.get_values();

    for b in 0..lut_size {
        for g in 0..lut_size {
            for r in 0..lut_size {
                // OpData::Lut3D Array index: blue changes fastest.
                let array_idx = 3 * ((r * lut_size + g) * lut_size + b);

                // Houdini order, red changes fastest.
                let ocio_idx = 3 * ((b * lut_size + g) * lut_size + r);

                assert_eq!(lut_array[array_idx], cube[ocio_idx]);
                assert_eq!(lut_array[array_idx + 1], cube[ocio_idx + 1]);
                assert_eq!(lut_array[array_idx + 2], cube[ocio_idx + 2]);
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatHDL, read_1d)` @ v2.5.2.
#[test]
fn read_1d() {
    let strebuf = [
        "Version\t\t1\n",
        "Format\t\tany\n",
        "Type\t\tC\n",
        "From\t\t0.1 3.2\n",
        "To\t\t0 1\n",
        "Black\t\t0\n",
        "White\t\t0.99\n",
        "Length\t\t9\n",
        "LUT:\n",
        "RGB {\n",
        "\t0\n",
        "\t0.000977517\n",
        "\t0.00195503\n",
        "\t0.00293255\n",
        "\t0.00391007\n",
        "\t0.00488759\n",
        "\t0.0058651\n",
        "\t0.999022\n",
        "\t1.67 }\n",
    ]
    .concat();

    //
    let from_min = 0.1f32;
    let from_max = 3.2f32;
    let to_min = 0.0f32;
    let to_max = 1.0f32;
    let black = 0.0f32;
    let white = 0.99f32;
    let lut1d = [
        0.0f32,
        0.000977517,
        0.00195503,
        0.00293255,
        0.00391007,
        0.00488759,
        0.0058651,
        0.999022,
        1.67,
    ];

    // Load file
    let lut = read(&strebuf);
    let lut1d_data = lut.lut1d.as_ref().expect("a 1D LUT");

    assert_eq!(lut1d_data.get_file_output_bit_depth(), BitDepth::F32);

    //
    assert_eq!(to_min, lut.to_min);
    assert_eq!(to_max, lut.to_max);
    assert_eq!(black, lut.hdlblack);
    assert_eq!(white, lut.hdlwhite);

    // Check 1D data
    assert_eq!(from_min, lut.from_min);
    assert_eq!(from_max, lut.from_max);

    let lut_array = lut1d_data.get_array();
    assert_eq!(9, lut_array.get_length());
    let values = lut_array.get_values();

    for (i, &v) in lut1d.iter().enumerate() {
        assert_eq!(v, values[i * 3]);
        assert_eq!(v, values[i * 3 + 1]);
        assert_eq!(v, values[i * 3 + 2]);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatHDL, read_3d)` @ v2.5.2.
#[test]
// Upstream's literal, as written.
#[allow(clippy::excessive_precision)]
fn read_3d() {
    let strebuf = [
        "Version         2\n",
        "Format      any\n",
        "Type        3D\n",
        "From        0.2 0.9\n",
        "To      0.001 0.999\n",
        "Black       0.002\n",
        "White       0.98\n",
        "Length      2\n",
        "LUT:\n",
        " {\n",
        " 0 0 0\n",
        " 0 0 0\n",
        " 0 0.390735 2.68116e-28\n",
        " 0 0.390735 0\n",
        " 0 0 0\n",
        " 0 0 0.599397\n",
        " 0 0.601016 0\n",
        " 0 0.601016 0.917034\n",
        " }\n",
    ]
    .concat();

    // Load file
    let lut = read(&strebuf);
    let lut3d = lut.lut3d.as_ref().expect("a 3D LUT");

    // from_min & from_max are only stored when there is a 1D LUT.
    let to_min = 0.001f32;
    let to_max = 0.999f32;
    let black = 0.002f32;
    let white = 0.98f32;
    let cube = [
        0.0f32,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.390735,
        2.68116e-28,
        0.0,
        0.390735,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.599397,
        0.0,
        0.601016,
        0.0,
        0.0,
        0.601016,
        0.917034,
    ];

    //
    assert_eq!(to_min, lut.to_min);
    assert_eq!(to_max, lut.to_max);
    assert_eq!(black, lut.hdlblack);
    assert_eq!(white, lut.hdlwhite);

    // Check cube data
    check_cube(lut3d, &cube);
}

/// Port of `OCIO_ADD_TEST(FileFormatHDL, read_3d_1d)` @ v2.5.2.
#[test]
fn read_3d_1d() {
    let strebuf = [
        "Version         3\n",
        "Format      any\n",
        "Type        3D+1D\n",
        "From        0.005478 14.080103\n",
        "To      0 1\n",
        "Black       0\n",
        "White       1\n",
        "Length      2 10\n",
        "LUT:\n",
        "Pre {\n",
        "    0.994922\n",
        "    0.995052\n",
        "    0.995181\n",
        "    0.995310\n",
        "    0.995439\n",
        "    0.995568\n",
        "    0.995697\n",
        "    0.995826\n",
        "    0.995954\n",
        "    0.996082\n",
        "}\n",
        "3D {\n",
        "    0.093776 0.093776 0.093776\n",
        "    0.105219 0.093776 0.093776\n",
        "    0.118058 0.093776 0.093776\n",
        "    0.132463 0.093776 0.093776\n",
        "    0.148626 0.093776 0.093776\n",
        "    0.166761 0.093776 0.093776\n",
        "    0.187109 0.093776 0.093776\n",
        "    0.209939 0.093776 0.093776\n",
        "}\n",
    ]
    .concat();

    //
    let from_min = 0.005478f32;
    let from_max = 14.080103f32;
    let to_min = 0.0f32;
    let to_max = 1.0f32;
    let black = 0.0f32;
    let white = 1.0f32;
    let prelut = [
        0.994922f32,
        0.995052,
        0.995181,
        0.995310,
        0.995439,
        0.995568,
        0.995697,
        0.995826,
        0.995954,
        0.996082,
    ];
    let cube = [
        0.093776f32,
        0.093776,
        0.093776,
        0.105219,
        0.093776,
        0.093776,
        0.118058,
        0.093776,
        0.093776,
        0.132463,
        0.093776,
        0.093776,
        0.148626,
        0.093776,
        0.093776,
        0.166761,
        0.093776,
        0.093776,
        0.187109,
        0.093776,
        0.093776,
        0.209939,
        0.093776,
        0.093776,
    ];

    // Load file
    let lut = read(&strebuf);
    let lut1d = lut.lut1d.as_ref().expect("a prelut");
    let lut3d = lut.lut3d.as_ref().expect("a 3D LUT");

    assert_eq!(lut1d.get_file_output_bit_depth(), BitDepth::F32);

    //
    assert_eq!(to_min, lut.to_min);
    assert_eq!(to_max, lut.to_max);
    assert_eq!(black, lut.hdlblack);
    assert_eq!(white, lut.hdlwhite);

    // Check prelut data
    assert_eq!(from_min, lut.from_min);
    assert_eq!(from_max, lut.from_max);
    let pre_lut_array = lut1d.get_array();
    assert_eq!(10, pre_lut_array.get_length());
    let values = pre_lut_array.get_values();

    for (i, &v) in prelut.iter().enumerate() {
        assert_eq!(v, values[3 * i]);
        assert_eq!(v, values[3 * i + 1]);
        assert_eq!(v, values[3 * i + 2]);
    }

    // check cube data
    check_cube(lut3d, &cube);
}

/// A `3D+1D` file whose `Length` line gives the cube size alone has no prelut size: an error
/// in the port, where upstream reads past the sizes it read (docs/improvements.md, U-71).
#[test]
fn a_3d_1d_length_without_a_prelut_size_is_an_error() {
    let text = "Version 3\nFormat any\nType 3D+1D\nFrom 0 1\nTo 0 1\nBlack 0\nWhite 1\n\
                Length 2\nLUT:\nPre {\n0 1\n}\n3D {\n0 0 0\n}\n";
    let mut stream = InputStream::from_bytes(text);
    let error = LocalFileFormat
        .read_file(&mut stream, Interpolation::Default)
        .unwrap_err();
    assert!(error.to_string().contains("needs a prelut size"), "{error}");
}
