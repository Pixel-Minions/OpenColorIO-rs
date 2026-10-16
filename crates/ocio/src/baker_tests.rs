// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the baker: `tests/cpu/Baker_tests.cpp` @ v2.5.2.

use super::*;
use ocio_ops::parse_utils::string_vec_to_float_vec;
use ocio_ops::utils::string_utils::{split_by_lines, split_by_white_spaces, trim};
use ocio_testkit::upstream::{check_close, check_throw_what};

/// The floats of two lines are close.
///
/// Port of `CompareFloats` (Baker_tests.cpp:17-33 @ v2.5.2).
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

/// Port of `OCIO_ADD_TEST(Baker, bake_3dlut)` @ v2.5.2.
#[test]
fn bake_3dlut() {
    let my_profile = concat!(
        "ocio_profile_version: 2\n",
        "\n",
        "file_rules:\n",
        "  - !<Rule> {name: Default, colorspace: lnh}\n",
        "\n",
        "displays:\n",
        "  display1:\n",
        "    - !<View> {name: view1, colorspace: gamma22}\n",
        "    - !<View> {name: view2, looks: satlook, colorspace: gamma22}\n",
        "\n",
        "looks:\n",
        "  - !<Look>\n",
        "    name : contrastlook\n",
        "    process_space : lnh\n",
        "    transform : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}\n",
        "  - !<Look>\n",
        "    name : satlook\n",
        "    process_space : lnh\n",
        "    transform : !<CDLTransform> {sat: 2}\n",
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name : lnh\n",
        "    bitdepth : 16f\n",
        "    isdata : false\n",
        "    allocation : lg2\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name : gamma22\n",
        "    bitdepth : 8ui\n",
        "    isdata : false\n",
        "    allocation : uniform\n",
        "    to_reference : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}\n",
        "\n",
        "named_transforms:\n",
        "- !<NamedTransform>\n",
        "  name: logcnt\n",
        "  transform: !<LogCameraTransform>\n",
        "    log_side_slope:  0.247189638318671\n",
        "    log_side_offset: 0.385536998692443\n",
        "    lin_side_slope:  5.55555555555556\n",
        "    lin_side_offset: 0.0522722750251688\n",
        "    lin_side_break:  0.0105909904954696\n",
        "    base: 10\n",
        "    direction: inverse\n",
        "\n",
    );

    let expected_lut = concat!(
        "CSPLUTV100\n",
        "3D\n",
        "\n",
        "BEGIN METADATA\n",
        "this is some metadata!\n",
        "END METADATA\n",
        "\n",
        "4\n",
        "0.000977 0.039373 1.587401 64.000000\n",
        "0.000000 0.333333 0.666667 1.000000\n",
        "4\n",
        "0.000977 0.039373 1.587401 64.000000\n",
        "0.000000 0.333333 0.666667 1.000000\n",
        "4\n",
        "0.000977 0.039373 1.587401 64.000000\n",
        "0.000000 0.333333 0.666667 1.000000\n",
        "\n",
        "2 2 2\n",
        "0.042823 0.042823 0.042823\n",
        "6.622026 0.042823 0.042823\n",
        "0.042823 6.622026 0.042823\n",
        "6.622026 6.622026 0.042823\n",
        "0.042823 0.042823 6.622026\n",
        "6.622026 0.042823 6.622026\n",
        "0.042823 6.622026 6.622026\n",
        "6.622026 6.622026 6.622026\n",
        "\n",
    );

    let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(config.num_named_transforms(), 1);

    {
        let mut bake = Baker::new();
        bake.set_config(&config);
        let cfg2 = bake.config().unwrap();
        assert_eq!(cfg2.num_color_spaces(), 2);

        let test_string = b"this is some metadata!";
        bake.format_metadata_mut()
            .add_child_element(Some(b"Desc"), Some(test_string))
            .unwrap();
        let data = bake.format_metadata();
        assert_eq!(data.get_num_children_elements(), 1);
        assert_eq!(
            test_string.as_slice(),
            data.get_child_element(0).unwrap().get_element_value()
        );

        bake.set_format("cinespace").unwrap();
        assert_eq!(b"cinespace", bake.format());
        bake.set_input_space("lnh");
        assert_eq!(b"lnh", bake.input_space());
        bake.set_looks("foo, +bar");
        assert_eq!(b"foo, +bar", bake.looks());
        bake.set_looks("");
        bake.set_target_space("gamma22");
        assert_eq!(b"gamma22", bake.target_space());
        bake.set_shaper_size(4);
        assert_eq!(4, bake.shaper_size());
        bake.set_cube_size(2);
        assert_eq!(2, bake.cube_size());
        let mut os = Vec::new();
        bake.bake(&mut os).unwrap();

        let osvec = split_by_lines(expected_lut.as_bytes());
        let resvec = split_by_lines(&os);
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

        assert_eq!(12, Baker::num_formats());
        assert_eq!(b"cinespace", Baker::format_name_by_index(4));
        assert_eq!(b"3dl", Baker::format_extension_by_index(1));
    }

    {
        let mut bake = Baker::new();
        bake.set_config(&config);
        bake.set_format("resolve_cube").unwrap();
        bake.set_input_space("lnh");
        bake.set_looks("contrastlook");
        bake.set_display_view("display1", "view1");
        bake.set_cube_size(10);
        let mut os = Vec::new();
        bake.bake(&mut os).unwrap();

        let expected_cube = r#"LUT_1D_SIZE 10
0.000000 0.000000 0.000000
0.111111 0.111111 0.111111
0.222222 0.222222 0.222222
0.333333 0.333333 0.333333
0.444444 0.444444 0.444444
0.555556 0.555556 0.555556
0.666667 0.666667 0.666667
0.777778 0.777778 0.777778
0.888889 0.888889 0.888889
1.000000 1.000000 1.000000
"#;

        assert_eq!(expected_cube.len(), os.len());
        assert_eq!(expected_cube, String::from_utf8_lossy(&os));
    }

    {
        let mut bake = Baker::new();
        bake.set_config(&config);
        bake.set_format("resolve_cube").unwrap();
        bake.set_input_space("lnh");
        bake.set_shaper_space("logcnt");
        bake.set_display_view("display1", "view2");
        bake.set_shaper_size(10);
        bake.set_cube_size(2);
        let mut os = Vec::new();
        bake.bake(&mut os).unwrap();

        let expected_cube = r#"LUT_1D_SIZE 10
LUT_1D_INPUT_RANGE -0.017290 55.080036
LUT_3D_SIZE 2
0.000000 0.000000 0.000000
0.763998 0.763998 0.763998
0.838479 0.838479 0.838479
0.882030 0.882030 0.882030
0.912925 0.912925 0.912925
0.936887 0.936887 0.936887
0.956464 0.956464 0.956464
0.973016 0.973016 0.973016
0.987354 0.987354 0.987354
1.000000 1.000000 1.000000
0.000000 0.000000 0.000000
8.054426 0.000000 0.000000
0.000000 6.931791 0.000000
6.384501 6.384501 0.000000
0.000000 0.000000 8.336130
7.904850 0.000000 7.904850
0.000000 6.751890 6.751890
6.185304 6.185304 6.185304
"#;

        assert_eq!(expected_cube.len(), os.len());
        assert_eq!(expected_cube, String::from_utf8_lossy(&os));
    }
}

/// `bake->bake(os)`, its text cleared first.
fn bake_into(bake: &Baker) -> Result<()> {
    let mut os = Vec::new();
    bake.bake(&mut os)
}

/// Port of `OCIO_ADD_TEST(Baker, baking_validation)` @ v2.5.2.
#[test]
fn baking_validation() {
    let my_profile = r#"
        ocio_profile_version: 2

        strictparsing: false

        roles:
          scene_linear: Raw

        file_rules:
          - !<Rule> {name: Default, colorspace: Raw}

        shared_views:
          - !<View> {name: Raw, colorspace: Raw}
          - !<View> {name: RawInactive, colorspace: Raw}

        displays:
          sRGB:
            - !<Views> [Raw, RawInactive]
            - !<View> {name: Film, colorspace: sRGB}
            - !<View> {name: FilmInactive, colorspace: sRGB}
          sRGBInactive:
            - !<Views> [Raw, RawInactive]
            - !<View> {name: Film, colorspace: sRGB}
            - !<View> {name: FilmInactive, colorspace: sRGB}

        active_displays: [sRGB]
        active_views: [Film, Raw]

        looks:
        - !<Look>
          name : foo
          process_space : Raw
          transform : !<CDLTransform> {sat: 2}

        colorspaces:
        - !<ColorSpace>
          name : Raw
          isdata : false

        - !<ColorSpace>
          name : RawInactive
          isdata : false

        - !<ColorSpace>
          name : Log
          isdata : false
          to_reference: !<LogTransform> {}

        - !<ColorSpace>
          name : Saturation
          isdata : false
          to_reference: !<CDLTransform> {sat: 0.5}

        - !<ColorSpace>
          name : Log2sRGB
          isdata : false
          to_reference: !<GroupTransform>
            children:
              - !<LogTransform> {base: 2, direction: inverse}
              - !<MatrixTransform> {matrix: [3.2409, -1.5373, -0.4986, 0, -0.9692, 1.8759, 0.0415, 0, 0.0556, -0.2039, 1.0569, 0, 0, 0, 0, 1 ], direction: inverse}

        - !<ColorSpace>
          name : sRGB
          isdata : false
          from_reference: !<GroupTransform>
            children:
              - !<MatrixTransform> {matrix: [3.2409, -1.5373, -0.4986, 0, -0.9692, 1.8759, 0.0415, 0, 0.0556, -0.2039, 1.0569, 0, 0, 0, 0, 1 ]}
              - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

        - !<ColorSpace>
          name : Gamma22
          isdata : false
          from_reference : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1], direction: inverse}

        named_transforms:

        - !<NamedTransform>
          name: Log2NT
          transform: !<LogTransform> {base: 2}

        inactive_colorspaces: [RawInactive]
    "#;

    let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
    config.validate().unwrap();

    // Missing configuration.
    let mut bake = Baker::new();
    bake.set_format("cinespace").unwrap();

    check_throw_what(bake_into(&bake), "No OCIO config has been set.");

    // Missing input space
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_target_space("Gamma22");
    bake.set_format("cinespace").unwrap();

    check_throw_what(bake_into(&bake), "No input space has been set.");

    // Missing target space and display / view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_format("cinespace").unwrap();

    check_throw_what(
        bake_into(&bake),
        "No display / view or target colorspace has been set.",
    );

    // Setting both target space and display / view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_target_space("Gamma22");
    bake.set_display_view("sRGB", "Film");
    bake.set_format("cinespace").unwrap();

    check_throw_what(
        bake_into(&bake),
        "Cannot use both display / view and target colorspace.",
    );

    // Setting looks with display / view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Film");
    bake.set_looks("foo");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Invalid input space.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Invalid");
    bake.set_display_view("sRGB", "Film");
    bake.set_format("cinespace").unwrap();

    check_throw_what(
        bake_into(&bake),
        "Could not find input colorspace 'Invalid'.",
    );

    // Inactive input space.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("RawInactive");
    bake.set_display_view("sRGB", "Film");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Invalid target space.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_target_space("Invalid");
    bake.set_format("cinespace").unwrap();

    check_throw_what(
        bake_into(&bake),
        "Could not find target colorspace 'Invalid'.",
    );

    // Invalid display.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("Invalid", "Film");
    bake.set_format("cinespace").unwrap();

    check_throw_what(bake_into(&bake), "Could not find display 'Invalid'.");

    // Invalid view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Invalid");
    bake.set_format("cinespace").unwrap();

    check_throw_what(bake_into(&bake), "Could not find view 'Invalid'.");

    // Inactive display.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("sRGBInactive", "Film");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Shared view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Raw");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Inactive view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "FilmInactive");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Inactive shared view.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("sRGBInactive", "RawInactive");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Baking 1D LUT with Crosstalk.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_display_view("sRGB", "Film");
    bake.set_format("spi1d").unwrap();

    check_throw_what(
        bake_into(&bake),
        "The format 'spi1d' does not support transformations with channel crosstalk.",
    );

    // Cube Size < 2.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_cube_size(1);
    bake.set_format("cinespace").unwrap();

    check_throw_what(bake_into(&bake), "Cube size must be at least 2 if set.");

    // Shaper Size < 2.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_shaper_space("Log");
    bake.set_shaper_size(1);
    bake.set_format("resolve_cube").unwrap();

    check_throw_what(
        bake_into(&bake),
        "A shaper space 'Log' has been specified, so the shaper size must be 2 or larger.",
    );

    // Using shaper with unsupported format.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_shaper_space("Log");
    bake.set_format("iridas_itx").unwrap();

    check_throw_what(
        bake_into(&bake),
        "The format 'iridas_itx' does not support shaper space.",
    );

    // Using shaper space with Crosstalk.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Raw");
    bake.set_target_space("sRGB");
    bake.set_shaper_space("Saturation");
    bake.set_format("cinespace").unwrap();

    check_throw_what(
        bake_into(&bake),
        "The specified shaper space, 'Saturation' has channel crosstalk, which \
         is not appropriate for shapers. Please select an alternate shaper \
         space or omit this option.",
    );

    // Using shaper space without Crosstalk (after optimization).
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("sRGB");
    bake.set_target_space("Raw");
    bake.set_shaper_space("Log2sRGB");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Using NamedTransform as shaper space.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("sRGB");
    bake.set_target_space("Raw");
    bake.set_shaper_space("Log2NT");
    bake.set_format("cinespace").unwrap();

    bake_into(&bake).unwrap();

    // Using NamedTransform as input space is not supported.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("Log2NT");
    bake.set_target_space("sRGB");
    bake.set_format("cinespace").unwrap();

    check_throw_what(
        bake_into(&bake),
        "Could not find input colorspace 'Log2NT'.",
    );

    // Using NamedTransform as target space is not supported.
    let mut bake = Baker::new();
    bake.set_config(&config);
    bake.set_input_space("sRGB");
    bake.set_target_space("Log2NT");
    bake.set_format("cinespace").unwrap();

    check_throw_what(
        bake_into(&bake),
        "Could not find target colorspace 'Log2NT'.",
    );
}
