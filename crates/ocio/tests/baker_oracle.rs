// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The baker's settings and their checks against the wheel (`bake`): the getters after the
//! setters, `setFormat`'s refusals, and every error `Baker::bake` raises before a format
//! bakes, in upstream's order: the format, the config, the input space, the target space or
//! the display and view (active, inactive, shared), the 1D formats' crosstalk, the cube and
//! shaper sizes, the shaper's support and its crosstalk, and the processors' own errors.

mod bake_common;

use bake_common::{BakeCase, Call, Metadata, check_bakes};

use Call::*;

/// The config of upstream's `Baker_tests.cpp` `baking_validation` (Baker_tests.cpp:247-327
/// @ v2.5.2), with a role and an alias for the lookups.
const CONFIG: &str = r#"ocio_profile_version: 2

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
  aliases: [raw_alias]
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

fn case(label: &str, calls: &[Call]) -> BakeCase {
    BakeCase::new(label, Some(CONFIG), calls)
}

/// The settings' cases.
fn cases() -> Vec<BakeCase> {
    vec![
        // The getters, and the names cut at a NUL.
        case(
            "the getters",
            &[
                InputSpace("Raw"),
                ShaperSpace("Log"),
                Looks("foo, +bar"),
                TargetSpace("Gamma22"),
                DisplayView("sRGB", "Film"),
                ShaperSize(7),
                CubeSize(1),
                Format("spi3d"),
            ],
        ),
        case(
            "names cut at a NUL",
            &[
                InputSpace("Raw\0x"),
                ShaperSpace("Log\0x"),
                Looks("foo\0x"),
                TargetSpace("Gamma22\0x"),
                DisplayView("sRGB\0x", "Film\0x"),
                ShaperSize(-5),
                CubeSize(-1),
                Format("spi3d\0x"),
            ],
        ),
        // setFormat.
        case("a format in capitals", &[Format("SPI3D"), CubeSize(0)]),
        case("a format by another name", &[Format("flame"), CubeSize(0)]),
        case("an unknown format", &[Format("nope")]),
        case("an empty format", &[Format("")]),
        case("a format that doesn't bake", &[Format("ColorDecisionList")]),
        case("the .look format", &[Format("iridas_look")]),
        case(
            "a format that doesn't bake, then one",
            &[Format("spi3d"), Format("cdl")],
        ),
        // The format.
        case("no format", &[InputSpace("Raw"), TargetSpace("Gamma22")]),
        // The input space.
        case(
            "no input space",
            &[TargetSpace("Gamma22"), Format("cinespace")],
        ),
        case("no input space and no target", &[Format("cinespace")]),
        case(
            "no target or display and view",
            &[InputSpace("Raw"), Format("cinespace")],
        ),
        case(
            "a display without a view",
            &[
                InputSpace("Raw"),
                DisplayView("sRGB", ""),
                Format("cinespace"),
            ],
        ),
        case(
            "a view without a display",
            &[
                InputSpace("Raw"),
                DisplayView("", "Film"),
                Format("cinespace"),
            ],
        ),
        case(
            "both a target and a display and view",
            &[
                InputSpace("Raw"),
                TargetSpace("Gamma22"),
                DisplayView("sRGB", "Film"),
                Format("cinespace"),
            ],
        ),
        case(
            "an unknown input space",
            &[
                InputSpace("Invalid"),
                DisplayView("sRGB", "Film"),
                Format("cinespace"),
            ],
        ),
        case(
            "a named transform as input",
            &[
                InputSpace("Log2NT"),
                TargetSpace("sRGB"),
                Format("cinespace"),
            ],
        ),
        case(
            "an unknown target space",
            &[
                InputSpace("Raw"),
                TargetSpace("Invalid"),
                Format("cinespace"),
            ],
        ),
        case(
            "a named transform as target",
            &[
                InputSpace("sRGB"),
                TargetSpace("Log2NT"),
                Format("cinespace"),
            ],
        ),
        // The display and view.
        case(
            "an unknown display",
            &[
                InputSpace("Raw"),
                DisplayView("Invalid", "Film"),
                Format("cinespace"),
            ],
        ),
        case(
            "a display in other case",
            &[
                InputSpace("Raw"),
                DisplayView("srgb", "Film"),
                Format("cinespace"),
            ],
        ),
        case(
            "an unknown view",
            &[
                InputSpace("Raw"),
                DisplayView("sRGB", "Invalid"),
                Format("cinespace"),
            ],
        ),
        case(
            "an unknown view of an inactive display",
            &[
                InputSpace("Raw"),
                DisplayView("sRGBInactive", "Nope"),
                Format("cinespace"),
            ],
        ),
        case(
            "a view in other case",
            &[
                InputSpace("Raw"),
                DisplayView("sRGB", "film"),
                Format("cinespace"),
            ],
        ),
        // The 1D formats refuse crosstalk: a matrix, a saturation; a role and an alias as
        // input.
        case(
            "crosstalk in a 1D format",
            &[
                InputSpace("Raw"),
                DisplayView("sRGB", "Film"),
                Format("spi1d"),
            ],
        ),
        case(
            "crosstalk in a 1D format, a role as input",
            &[
                InputSpace("scene_linear"),
                TargetSpace("sRGB"),
                Format("spi1d"),
            ],
        ),
        case(
            "crosstalk in a 1D format, an alias as input",
            &[
                InputSpace("raw_alias"),
                TargetSpace("Saturation"),
                Format("spi1d"),
            ],
        ),
        case(
            "crosstalk in a 1D format from a look",
            &[
                InputSpace("Raw"),
                TargetSpace("Raw"),
                Looks("foo"),
                Format("spi1d"),
            ],
        ),
        case(
            "an unknown look in a 1D format",
            &[
                InputSpace("Raw"),
                TargetSpace("Raw"),
                Looks("nope"),
                Format("spi1d"),
            ],
        ),
        case(
            "an unknown look with a display in a 1D format",
            &[
                InputSpace("Raw"),
                DisplayView("sRGB", "Raw"),
                Looks("nope"),
                Format("spi1d"),
            ],
        ),
        // The cube size.
        for_cube_size(1),
        for_cube_size(0),
        for_cube_size(-2),
        for_cube_size(i32::MIN),
        // The shaper.
        case(
            "a shaper size of 1",
            &[
                InputSpace("Raw"),
                TargetSpace("sRGB"),
                ShaperSpace("Log"),
                ShaperSize(1),
                Format("resolve_cube"),
            ],
        ),
        case(
            "a shaper size of 0 for a named transform",
            &[
                InputSpace("Raw"),
                TargetSpace("sRGB"),
                ShaperSpace("Log2NT"),
                ShaperSize(0),
                Format("houdini"),
            ],
        ),
        case(
            "a shaper size of 1 with a 1D format",
            &[
                InputSpace("Raw"),
                TargetSpace("Log"),
                ShaperSpace("Log"),
                ShaperSize(1),
                Format("spi1d"),
            ],
        ),
        case(
            "a shaper with a format that takes none",
            &[
                InputSpace("Raw"),
                TargetSpace("sRGB"),
                ShaperSpace("Log"),
                Format("iridas_itx"),
            ],
        ),
        case(
            "a shaper with a format in capitals that takes none",
            &[
                InputSpace("Raw"),
                TargetSpace("sRGB"),
                ShaperSpace("Log"),
                Format("IRIDAS_ITX"),
            ],
        ),
        case(
            "a shaper with crosstalk",
            &[
                InputSpace("Raw"),
                TargetSpace("sRGB"),
                ShaperSpace("Saturation"),
                Format("cinespace"),
            ],
        ),
        case(
            "an unknown shaper space",
            &[
                InputSpace("Raw"),
                TargetSpace("sRGB"),
                ShaperSpace("Nope"),
                Format("cinespace"),
            ],
        ),
        case(
            "a cube size of 1 and a shaper size of 1",
            &[
                InputSpace("Raw"),
                TargetSpace("sRGB"),
                ShaperSpace("Log"),
                ShaperSize(1),
                CubeSize(1),
                Format("resolve_cube"),
            ],
        ),
        // A baker without a config: the format is looked up first.
        BakeCase::without_config("no config, no format", &[]),
        BakeCase::without_config("no config, spi1d", &[Format("spi1d")]),
        BakeCase::without_config("no config, Flame", &[Format("Flame")]),
        BakeCase::without_config(
            "no config, the spaces set",
            &[InputSpace("lin"), TargetSpace("log"), Format("cinespace")],
        ),
        // The raw config.
        BakeCase::new(
            "the raw config",
            None,
            &[InputSpace("raw"), TargetSpace("nope"), Format("spi3d")],
        ),
        // Metadata alone doesn't change the checks.
        case("metadata", &[InputSpace("Raw"), Format("spi3d")]).with_metadata(&[
            Metadata::Child("Desc", "some text"),
            Metadata::Name("name"),
            Metadata::Id("id"),
        ]),
    ]
}

/// A cube size the baker refuses.
fn for_cube_size(size: i32) -> BakeCase {
    case(
        &format!("a cube size of {size}"),
        &[
            InputSpace("Raw"),
            TargetSpace("sRGB"),
            CubeSize(size),
            Format("cinespace"),
        ],
    )
}

#[test]
fn the_settings_check_as_in_the_wheel() {
    check_bakes(&cases());
}

/// The baker's format queries list the wheel's `Baker.getFormats()`, in order.
#[test]
fn the_format_queries_match_the_wheel() {
    let result = ocio_testkit::Oracle::get()
        .call("file_formats", serde_json::json!({}), &[])
        .result;
    let wheel: Vec<(Vec<u8>, Vec<u8>)> = result["bake"]
        .as_array()
        .expect("the baker's formats")
        .iter()
        .map(|p| {
            (
                ocio_testkit::oracle_values::bytes(&p[0]),
                ocio_testkit::oracle_values::bytes(&p[1]),
            )
        })
        .collect();
    let port: Vec<(Vec<u8>, Vec<u8>)> = (0..ocio::Baker::num_formats())
        .map(|i| {
            (
                ocio::Baker::format_name_by_index(i).to_vec(),
                ocio::Baker::format_extension_by_index(i).to_vec(),
            )
        })
        .collect();
    assert_eq!(port, wheel);
}
