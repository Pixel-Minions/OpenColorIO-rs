// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Houdini LUT format: `tests/cpu/fileformats/FileFormatHDL_tests.cpp` @ v2.5.2.

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

use ocio_ops::utils::string_utils::{split_by_lines, split_by_white_spaces, trim};
use ocio_testkit::upstream::check_str_float_vec_close;

/// A line and its floats, as `StringFloatVecClose` reads them.
fn floats(line: &[u8]) -> (String, Option<Vec<f32>>) {
    let parts = split_by_white_spaces(line);
    let mut values = Vec::new();
    let ok = ocio_ops::parse_utils::string_vec_to_float_vec(&mut values, &parts);
    (
        String::from_utf8_lossy(line).into_owned(),
        ok.then_some(values),
    )
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

/// A color space `name` of `transform` in `dir`.
fn color_space(
    name: &str,
    transform: crate::Transform,
    dir: ocio_ops::open_color_types::ColorSpaceDirection,
) -> crate::ColorSpace {
    let mut cs = crate::ColorSpace::new();
    cs.set_name(name);
    cs.set_family(name);
    cs.set_transform(Some(&transform), dir).unwrap();
    cs
}

/// An exponent transform of `value` on the color channels.
fn exponent(value: f64) -> crate::Transform {
    let mut transform1 = crate::ExponentTransform::new();
    let test: [f64; 4] = [value, value, value, 1.0];
    transform1.set_value(&test);
    crate::Transform::from(transform1)
}

/// A CDL of saturation `sat`.
fn saturation(sat: f64) -> crate::Transform {
    let mut transform1 = crate::CdlTransform::new();
    transform1.set_sat(sat);
    crate::Transform::from(transform1)
}

use ocio_ops::open_color_types::ColorSpaceDirection::{FromReference, ToReference};

/// Port of `OCIO_ADD_TEST(FileFormatHDL, bake_1d)` @ v2.5.2.
#[test]
fn bake_1d() {
    let mut config = lnf_config();

    // Add target space
    {
        let mut transform1 = crate::CdlTransform::new();
        let rgb: [f64; 3] = [0.1, 0.1, 0.1];
        transform1.set_offset(&rgb);
        let cs = color_space("target", crate::Transform::from(transform1), FromReference);
        config.add_color_space(&cs).unwrap();
    }

    let bout = concat!(
        "Version\t\t1\n",
        "Format\t\tany\n",
        "Type\t\tRGB\n",
        "From\t\t0.000000 1.000000\n",
        "To\t\t0.000000 1.000000\n",
        "Black\t\t0.000000\n",
        "White\t\t1.000000\n",
        "Length\t\t10\n",
        "LUT:\n",
        "R {\n",
        "\t0.100000\n",
        "\t0.211111\n",
        "\t0.322222\n",
        "\t0.433333\n",
        "\t0.544444\n",
        "\t0.655556\n",
        "\t0.766667\n",
        "\t0.877778\n",
        "\t0.988889\n",
        "\t1.100000\n",
        " }\n",
        "G {\n",
        "\t0.100000\n",
        "\t0.211111\n",
        "\t0.322222\n",
        "\t0.433333\n",
        "\t0.544444\n",
        "\t0.655556\n",
        "\t0.766667\n",
        "\t0.877778\n",
        "\t0.988889\n",
        "\t1.100000\n",
        " }\n",
        "B {\n",
        "\t0.100000\n",
        "\t0.211111\n",
        "\t0.322222\n",
        "\t0.433333\n",
        "\t0.544444\n",
        "\t0.655556\n",
        "\t0.766667\n",
        "\t0.877778\n",
        "\t0.988889\n",
        "\t1.100000\n",
        " }\n",
    );

    //
    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("houdini").unwrap();
    baker.set_input_space("lnf");
    baker.set_target_space("target");
    baker.set_cube_size(10); // FIXME: Misusing the cube size to set the 1D LUT size
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = split_by_lines(&output);
    let resvec = split_by_lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..std::cmp::min(osvec.len(), resvec.len()) {
        assert_eq!(
            String::from_utf8_lossy(trim(&osvec[i])),
            String::from_utf8_lossy(trim(&resvec[i]))
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatHDL, bake_1d_shaper)` @ v2.5.2.
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

    let config = crate::Config::create_from_stream(my_profile.as_bytes()).unwrap();

    {
        // Lin to Log
        let mut baker = Baker::new();
        baker.set_config(&config);
        baker.set_format("houdini").unwrap();
        baker.set_input_space("Raw");
        baker.set_target_space("Log2");
        baker.set_shaper_space("Log2");
        baker.set_cube_size(10);
        let mut output_hdl = Vec::new();
        baker.bake(&mut output_hdl).unwrap();

        let expected_hdl = concat!(
            "Version\t\t1\n",
            "Format\t\tany\n",
            "Type\t\tRGB\n",
            "From\t\t0.001989 16.291878\n",
            "To\t\t0.000000 1.000000\n",
            "Black\t\t0.000000\n",
            "White\t\t1.000000\n",
            "Length\t\t10\n",
            "LUT:\n",
            "R {\n",
            "\t0.000000\n",
            "\t0.756268\n",
            "\t0.833130\n",
            "\t0.878107\n",
            "\t0.910023\n",
            "\t0.934780\n",
            "\t0.955010\n",
            "\t0.972114\n",
            "\t0.986931\n",
            "\t1.000000\n",
            "}\n",
            "G {\n",
            "\t0.000000\n",
            "\t0.756268\n",
            "\t0.833130\n",
            "\t0.878107\n",
            "\t0.910023\n",
            "\t0.934780\n",
            "\t0.955010\n",
            "\t0.972114\n",
            "\t0.986931\n",
            "\t1.000000\n",
            "}\n",
            "B {\n",
            "\t0.000000\n",
            "\t0.756268\n",
            "\t0.833130\n",
            "\t0.878107\n",
            "\t0.910023\n",
            "\t0.934780\n",
            "\t0.955010\n",
            "\t0.972114\n",
            "\t0.986931\n",
            "\t1.000000\n",
            "}\n",
        );

        assert_eq!(expected_hdl.len(), output_hdl.len());
        assert_eq!(expected_hdl, String::from_utf8_lossy(&output_hdl));
    }

    {
        // Log to Lin
        let mut baker = Baker::new();
        baker.set_config(&config);
        baker.set_format("houdini").unwrap();
        baker.set_input_space("Log2");
        baker.set_target_space("Raw");
        baker.set_cube_size(10);
        let mut output_hdl = Vec::new();
        baker.bake(&mut output_hdl).unwrap();

        let expected_hdl = concat!(
            "Version\t\t1\n",
            "Format\t\tany\n",
            "Type\t\tRGB\n",
            "From\t\t0.000000 1.000000\n",
            "To\t\t0.000000 1.000000\n",
            "Black\t\t0.000000\n",
            "White\t\t1.000000\n",
            "Length\t\t10\n",
            "LUT:\n",
            "R {\n",
            "\t0.001989\n",
            "\t0.005413\n",
            "\t0.014731\n",
            "\t0.040091\n",
            "\t0.109110\n",
            "\t0.296951\n",
            "\t0.808177\n",
            "\t2.199522\n",
            "\t5.986179\n",
            "\t16.291878\n",
            "}\n",
            "G {\n",
            "\t0.001989\n",
            "\t0.005413\n",
            "\t0.014731\n",
            "\t0.040091\n",
            "\t0.109110\n",
            "\t0.296951\n",
            "\t0.808177\n",
            "\t2.199522\n",
            "\t5.986179\n",
            "\t16.291878\n",
            "}\n",
            "B {\n",
            "\t0.001989\n",
            "\t0.005413\n",
            "\t0.014731\n",
            "\t0.040091\n",
            "\t0.109110\n",
            "\t0.296951\n",
            "\t0.808177\n",
            "\t2.199522\n",
            "\t5.986179\n",
            "\t16.291878\n",
            "}\n",
        );

        let osvec = split_by_lines(expected_hdl.as_bytes());
        let resvec = split_by_lines(&output_hdl);
        assert_eq!(osvec.len(), resvec.len());
        for i in 0..resvec.len() {
            if (10..=19).contains(&i) || (22..=31).contains(&i) || (34..=43).contains(&i) {
                let (a, b) = (floats(&osvec[i]), floats(&resvec[i]));
                check_str_float_vec_close((&a.0, a.1), (&b.0, b.1), 1e-5f32);
            } else {
                assert_eq!(osvec[i], resvec[i]);
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatHDL, bake_3d)` @ v2.5.2.
#[test]
fn bake_3d() {
    let mut config = crate::Config::new().unwrap();

    // Set luma coef's to simple values
    {
        let luma_coef: [f64; 3] = [0.333, 0.333, 0.333];
        config.set_default_luma_coefs(&luma_coef);
    }

    // Add lnf space
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("lnf");
        cs.set_family("lnf");
        config.add_color_space(&cs).unwrap();
        config
            .set_role(ocio_ops::parse_utils::ROLE_REFERENCE, Some(cs.name()))
            .unwrap();
    }

    // Add target space
    // Set saturation to cause channel crosstalk, making a 3D LUT
    let cs = color_space("target", saturation(f64::from(0.5f32)), FromReference);
    config.add_color_space(&cs).unwrap();

    let bout = concat!(
        "Version\t\t2\n",
        "Format\t\tany\n",
        "Type\t\t3D\n",
        "From\t\t0.000000 1.000000\n",
        "To\t\t0.000000 1.000000\n",
        "Black\t\t0.000000\n",
        "White\t\t1.000000\n",
        "Length\t\t2\n",
        "LUT:\n",
        " {\n",
        "\t0.000000 0.000000 0.000000\n",
        "\t0.606300 0.106300 0.106300\n",
        "\t0.357600 0.857600 0.357600\n",
        "\t0.963900 0.963900 0.463900\n",
        "\t0.036100 0.036100 0.536100\n",
        "\t0.642400 0.142400 0.642400\n",
        "\t0.393700 0.893700 0.893700\n",
        "\t1.000000 1.000000 1.000000\n",
        " }\n",
    );

    //
    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("houdini").unwrap();
    baker.set_input_space("lnf");
    baker.set_target_space("target");
    baker.set_cube_size(2);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = split_by_lines(&output);
    let resvec = split_by_lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..std::cmp::min(osvec.len(), resvec.len()) {
        assert_eq!(
            String::from_utf8_lossy(trim(&osvec[i])),
            String::from_utf8_lossy(trim(&resvec[i]))
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatHDL, bake_3d_1d)` @ v2.5.2.
#[test]
fn bake_3d_1d() {
    // check baker output
    let mut config = crate::Config::new().unwrap();

    // Set luma coef's to simple values
    {
        let luma_coef: [f64; 3] = [0.333, 0.333, 0.333];
        config.set_default_luma_coefs(&luma_coef);
    }

    // Add lnf space
    {
        let mut cs = crate::ColorSpace::new();
        cs.set_name("lnf");
        cs.set_family("lnf");
        config.add_color_space(&cs).unwrap();
        config
            .set_role(ocio_ops::parse_utils::ROLE_REFERENCE, Some(cs.name()))
            .unwrap();
    }

    // Add shaper space
    config
        .add_color_space(&color_space("shaper", exponent(2.6), ToReference))
        .unwrap();

    // Add target space
    // Set saturation to cause channel crosstalk, making a 3D LUT
    config
        .add_color_space(&color_space("target", saturation(0.5), FromReference))
        .unwrap();

    let bout = concat!(
        "Version\t\t3\n",
        "Format\t\tany\n",
        "Type\t\t3D+1D\n",
        "From\t\t0.000000 1.000000\n",
        "To\t\t0.000000 1.000000\n",
        "Black\t\t0.000000\n",
        "White\t\t1.000000\n",
        "Length\t\t2 10\n",
        "LUT:\n",
        "Pre {\n",
        "\t0.000000\n",
        "\t0.429520\n",
        "\t0.560744\n",
        "\t0.655378\n",
        "\t0.732057\n",
        "\t0.797661\n",
        "\t0.855604\n",
        "\t0.907865\n",
        "\t0.955710\n",
        "\t1.000000\n",
        "}\n",
        "3D {\n",
        "\t0.000000 0.000000 0.000000\n",
        "\t0.606300 0.106300 0.106300\n",
        "\t0.357600 0.857600 0.357600\n",
        "\t0.963900 0.963900 0.463900\n",
        "\t0.036100 0.036100 0.536100\n",
        "\t0.642400 0.142400 0.642400\n",
        "\t0.393700 0.893700 0.893700\n",
        "\t1.000000 1.000000 1.000000\n",
        "}\n",
    );

    //
    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("houdini").unwrap();
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

    // TODO: Get this working on osx
    /*
    for(unsigned int i = 0; i < std::min(osvec.size(), resvec.size()); ++i)
        OCIO_CHECK_EQUAL(StringUtils::Trim(osvec[i]), StringUtils::Trim(resvec[i]));
    */
}

/// Port of `OCIO_ADD_TEST(FileFormatHDL, look_test)` @ v2.5.2.
#[test]
fn look_test() {
    // Note this sets up a Look with the same parameters as the Bake3D1D test
    // however it uses a different shaper space, to ensure we catch that case.
    // Also ensure we detect the effects of the desaturation by using a 3 cubed
    // LUT, which will thus test colour values other than the corner points of
    // the cube

    let mut config = lnf_config();

    // Add shaper space
    config
        .add_color_space(&color_space("shaper", exponent(2.2), ToReference))
        .unwrap();

    // Add Look process space
    config
        .add_color_space(&color_space("look_process", exponent(2.6), ToReference))
        .unwrap();

    // Add Look process space
    {
        let mut look = crate::Look::new();
        look.set_name("look");
        look.set_process_space("look_process");

        // Set saturation to cause channel crosstalk, making a 3D LUT
        look.set_transform(&saturation(f64::from(0.5f32))).unwrap();
        config.add_look(&look).unwrap();
    }

    let bout = concat!(
        "Version\t\t3\n",
        "Format\t\tany\n",
        "Type\t\t3D+1D\n",
        "From\t\t0.000000 1.000000\n",
        "To\t\t0.000000 1.000000\n",
        "Black\t\t0.000000\n",
        "White\t\t1.000000\n",
        "Length\t\t3 10\n",
        "LUT:\n",
        "Pre {\n",
        "\t0.000000\n",
        "\t0.368344\n",
        "\t0.504760\n",
        "\t0.606913\n",
        "\t0.691699\n",
        "\t0.765539\n",
        "\t0.831684\n",
        "\t0.892049\n",
        "\t0.947870\n",
        "\t1.000000\n",
        "}\n",
        "3D {\n",
        "\t0.000000 0.000000 0.000000\n",
        "\t0.276787 0.035360 0.035360\n",
        "\t0.553575 0.070720 0.070720\n",
        "\t0.148309 0.416989 0.148309\n",
        "\t0.478739 0.478739 0.201718\n",
        "\t0.774120 0.528900 0.245984\n",
        "\t0.296618 0.833978 0.296618\n",
        "\t0.650361 0.902354 0.355417\n",
        "\t0.957478 0.957478 0.403436\n",
        "\t0.009867 0.009867 0.239325\n",
        "\t0.296368 0.049954 0.296368\n",
        "\t0.575308 0.086766 0.343137\n",
        "\t0.166161 0.437812 0.437812\n",
        "\t0.500000 0.500000 0.500000\n",
        "\t0.796987 0.550484 0.550484\n",
        "\t0.316402 0.857106 0.607391\n",
        "\t0.672631 0.925760 0.672631\n",
        "\t0.981096 0.981096 0.725386\n",
        "\t0.019735 0.019735 0.478650\n",
        "\t0.312132 0.062101 0.541651\n",
        "\t0.592736 0.099909 0.592736\n",
        "\t0.180618 0.454533 0.695009\n",
        "\t0.517061 0.517061 0.761560\n",
        "\t0.815301 0.567796 0.815301\n",
        "\t0.332322 0.875624 0.875624\n",
        "\t0.690478 0.944497 0.944497\n",
        "\t1.000000 1.000000 1.000000\n",
        "}\n",
    );

    //
    let mut baker = Baker::new();
    baker.set_config(&config);
    baker.set_format("houdini").unwrap();
    baker.set_input_space("lnf");
    baker.set_shaper_space("shaper");
    baker.set_target_space("shaper");
    baker.set_looks("look");
    baker.set_shaper_size(10);
    baker.set_cube_size(3);
    let mut output = Vec::new();
    baker.bake(&mut output).unwrap();

    //
    let osvec = split_by_lines(&output);
    let resvec = split_by_lines(bout.as_bytes());
    assert_eq!(osvec.len(), resvec.len());
    for i in 0..std::cmp::min(osvec.len(), resvec.len()) {
        assert_eq!(
            String::from_utf8_lossy(trim(&osvec[i])),
            String::from_utf8_lossy(trim(&resvec[i]))
        );
    }
}
