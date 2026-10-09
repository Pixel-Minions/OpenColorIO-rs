// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Truelight `.cub` format:
//! `tests/cpu/fileformats/FileFormatTruelight_tests.cpp` @ v2.5.2.

use super::*;
use ocio_ops::open_color_types::OptimizationFlags;
use ocio_testkit::upstream::check_close;

/// Reads `text` as upstream's tests do (`tester.read(stream, "", INTERP_DEFAULT)`).
fn read(text: &str) -> LocalCachedFile {
    let mut stream = InputStream::from_bytes(text);
    LocalFileFormat
        .read_file(&mut stream, Interpolation::Default)
        .unwrap()
}

/// The file's ops (its shaper, then its 3D LUT), finalized and optimized with the default
/// flags, applied to the 3 pixels of `data`, then compared with `result` within 1e-6, as
/// upstream's tests do.
fn apply_and_check(lut: &LocalCachedFile, data: &mut [f32; 12], result: &[f32; 12]) {
    let mut ops = OpVec::new();
    if let Some(lut1d) = &lut.lut1d {
        create_lut1d_op(&mut ops, lut1d.clone(), TransformDirection::Forward);
    }
    if let Some(lut3d) = &lut.lut3d {
        create_lut3d_op(&mut ops, lut3d.clone(), TransformDirection::Forward);
    }

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    // Apply the result
    for op in ops.iter() {
        op.apply(data).unwrap();
    }

    for i in 0..4 * 3 {
        check_close(data[i], result[i], 1.0e-6);
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatTruelight, shaper_and_lut_3d)` @ v2.5.2.
#[test]
fn shaper_and_lut_3d() {
    // This lowers the red channel by 0.5, other channels are unaffected.
    let luttext = [
        "# Truelight Cube v2.0\n",
        "# iDims 3\n",
        "# oDims 3\n",
        "# width 3 3 3\n",
        "# lutLength 5\n",
        "# InputLUT\n",
        " 0.000000 0.000000 0.000000\n",
        " 0.500000 0.500000 0.500000\n",
        " 1.000000 1.000000 1.000000\n",
        " 1.500000 1.500000 1.500000\n",
        " 2.000000 2.000000 2.000000\n",
        "\n",
        "# Cube\n",
        " 0.000000 0.000000 0.000000\n",
        " 0.250000 0.000000 0.000000\n",
        " 0.500000 0.000000 0.000000\n",
        " 0.000000 0.500000 0.000000\n",
        " 0.250000 0.500000 0.000000\n",
        " 0.500000 0.500000 0.000000\n",
        " 0.000000 1.000000 0.000000\n",
        " 0.250000 1.000000 0.000000\n",
        " 0.500000 1.000000 0.000000\n",
        " 0.000000 0.000000 0.500000\n",
        " 0.250000 0.000000 0.500000\n",
        " 0.500000 0.000000 0.500000\n",
        " 0.000000 0.500000 0.500000\n",
        " 0.250000 0.500000 0.500000\n",
        " 0.500000 0.500000 0.500000\n",
        " 0.000000 1.000000 0.500000\n",
        " 0.250000 1.000000 0.500000\n",
        " 0.500000 1.000000 0.500000\n",
        " 0.000000 0.000000 1.000000\n",
        " 0.250000 0.000000 1.000000\n",
        " 0.500000 0.000000 1.000000\n",
        " 0.000000 0.500000 1.000000\n",
        " 0.250000 0.500000 1.000000\n",
        " 0.500000 0.500000 1.000000\n",
        " 0.000000 1.000000 1.000000\n",
        " 0.250000 1.000000 1.000000\n",
        " 0.500000 1.000000 1.000000\n",
        "\n",
        "# end\n",
        "\n",
        "# Truelight profile\n",
        "title{madeup on some display}\n",
        "print{someprint}\n",
        "display{some}\n",
        "cubeFile{madeup.cube}\n",
        "\n",
        " # This last line confirms 'end' tag is obeyed\n",
        " 1.23456 1.23456 1.23456\n",
    ]
    .concat();

    // Read file
    let lut = read(&luttext);

    assert!(lut.lut1d.is_some());
    assert!(lut.lut3d.is_some());
    assert_eq!(
        lut.lut1d.as_ref().unwrap().get_file_output_bit_depth(),
        BitDepth::F32
    );
    assert_eq!(
        lut.lut3d.as_ref().unwrap().get_file_output_bit_depth(),
        BitDepth::F32
    );

    let mut data = [
        0.1f32, 0.2, 0.3, 0.0, 1.0, 0.5, 0.123456, 0.0, -1.0, 1.5, 0.5, 0.0,
    ];

    let result = [
        0.05f32, 0.2, 0.3, 0.0, 0.50, 0.5, 0.123456, 0.0, 0.0, 1.0, 0.5, 0.0,
    ];

    apply_and_check(&lut, &mut data, &result);
}

/// Port of `OCIO_ADD_TEST(FileFormatTruelight, shaper)` @ v2.5.2.
#[test]
fn shaper() {
    let luttext = [
        "# Truelight Cube v2.0\n",
        "# lutLength 11\n",
        "# iDims 3\n",
        "\n",
        "\n",
        "# InputLUT\n",
        " 0.000 0.000 -0.000\n",
        " 0.200 0.010 -0.100\n",
        " 0.400 0.040 -0.200\n",
        " 0.600 0.090 -0.300\n",
        " 0.800 0.160 -0.400\n",
        " 1.000 0.250 -0.500\n",
        " 1.200 0.360 -0.600\n",
        " 1.400 0.490 -0.700\n",
        " 1.600 0.640 -0.800\n",
        " 1.800 0.820 -0.900\n",
        " 2.000 1.000 -1.000\n",
        "\n\n\n",
        "# end\n",
    ]
    .concat();

    // Read file
    let lut = read(&luttext);

    assert!(lut.lut1d.is_some());
    assert!(lut.lut3d.is_none());

    let mut data = [
        0.1f32, 0.2, 0.3, 0.0, 1.0, 0.5, 0.123456, 0.0, -1.0, 1.5, 0.5, 0.0,
    ];

    let result = [
        0.2f32, 0.04, -0.3, 0.0, 2.0, 0.25, -0.123456, 0.0, 0.0, 1.0, -0.5, 0.0,
    ];

    apply_and_check(&lut, &mut data, &result);
}

/// Port of `OCIO_ADD_TEST(FileFormatTruelight, lut_3d)` @ v2.5.2.
#[test]
fn lut_3d() {
    // This lowers the red channel by 0.5, other channels are unaffected.
    let luttext = [
        "# Truelight Cube v2.0\n",
        "# iDims 3\n",
        "# oDims 3\n",
        "# width 3 3 3\n",
        "\n\n\n",
        "# Cube\n",
        " 0.000000 0.000000 0.000000\n",
        " 0.250000 0.000000 0.000000\n",
        " 0.500000 0.000000 0.000000\n",
        " 0.000000 0.500000 0.000000\n",
        " 0.250000 0.500000 0.000000\n",
        " 0.500000 0.500000 0.000000\n",
        " 0.000000 1.000000 0.000000\n",
        " 0.250000 1.000000 0.000000\n",
        " 0.500000 1.000000 0.000000\n",
        " 0.000000 0.000000 0.500000\n",
        " 0.250000 0.000000 0.500000\n",
        " 0.500000 0.000000 0.500000\n",
        " 0.000000 0.500000 0.500000\n",
        " 0.250000 0.500000 0.500000\n",
        " 0.500000 0.500000 0.500000\n",
        " 0.000000 1.000000 0.500000\n",
        " 0.250000 1.000000 0.500000\n",
        " 0.500000 1.000000 0.500000\n",
        " 0.000000 0.000000 1.000000\n",
        " 0.250000 0.000000 1.000000\n",
        " 0.500000 0.000000 1.000000\n",
        " 0.000000 0.500000 1.000000\n",
        " 0.250000 0.500000 1.000000\n",
        " 0.500000 0.500000 1.000000\n",
        " 0.000000 1.000000 1.000000\n",
        " 0.250000 1.000000 1.000000\n",
        " 0.500000 1.000000 1.000000\n",
        "\n",
        "# end\n",
    ]
    .concat();

    // Read file
    let lut = read(&luttext);

    assert!(lut.lut1d.is_none());
    assert!(lut.lut3d.is_some());

    let mut data = [
        0.1f32, 0.2, 0.3, 0.0, 1.0, 0.5, 0.123456, 0.0, -1.0, 1.5, 0.5, 0.0,
    ];

    let result = [
        0.05f32, 0.2, 0.3, 0.0, 0.50, 0.5, 0.123456, 0.0, 0.0, 1.0, 0.5, 0.0,
    ];

    apply_and_check(&lut, &mut data, &result);
}
