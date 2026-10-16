// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The formats' bakes against the wheel (`bake`), byte for byte: each format's sizes (the
//! default included), shapers, metadata, the format name as the baker was given it, and
//! targets that give NaNs, infinities, negative and large values, written as each platform's
//! iostreams spell them. The values come from the CPU processors, so this target is one of
//! the `cpu-tests`.

mod bake_common;

use bake_common::{BakeCase, Call, Metadata, check_bakes};

use Call::*;

/// Color spaces for every kind of value: per-channel transforms (the 1D formats take no
/// crosstalk), a saturation, values past `float`'s range, NaNs, negatives.
const CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: lin

file_rules:
  - !<Rule> {name: Default, colorspace: lin}

displays:
  disp:
    - !<View> {name: log, colorspace: log2}
    - !<View> {name: sat, colorspace: sat}

looks:
  - !<Look>
    name: brighten
    process_space: lin
    transform: !<ExponentTransform> {value: [1.1, 0.9, 1.2, 1]}

colorspaces:
- !<ColorSpace>
  name: lin
  isdata: false

- !<ColorSpace>
  name: log2
  isdata: false
  from_reference: !<GroupTransform>
    children:
      - !<MatrixTransform> {matrix: [5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 5.55556, 0, 0, 0, 0, 1]}
      - !<LogTransform> {base: 2}
      - !<MatrixTransform> {offset: [6.5, 6.5, 6.5, 0]}
      - !<MatrixTransform> {matrix: [0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 0.076923, 0, 0, 0, 0, 1]}

- !<ColorSpace>
  name: gamma
  isdata: false
  from_reference: !<ExponentTransform> {value: [2.2, 1.8, 0.45, 1], direction: inverse}

- !<ColorSpace>
  name: sat
  isdata: false
  from_reference: !<CDLTransform> {slope: [1.2, 0.9, 1.1], offset: [0.01, -0.02, 0.03], sat: 0.5}

- !<ColorSpace>
  name: big
  isdata: false
  from_reference: !<GroupTransform>
    children:
      - !<MatrixTransform> {matrix: [1e30, 0, 0, 0, 0, -3e20, 0, 0, 0, 0, 1e-30, 0, 0, 0, 0, 1]}
      - !<ExponentTransform> {value: [2, 2, 2, 1]}

- !<ColorSpace>
  name: nans
  isdata: false
  from_reference: !<MatrixTransform> {offset: [.nan, 0.25, .inf, 0]}

- !<ColorSpace>
  name: nans_green
  isdata: false
  to_reference: !<MatrixTransform> {offset: [-0.75, .nan, 0.5, 0]}

- !<ColorSpace>
  name: double
  isdata: false
  to_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}

- !<ColorSpace>
  name: upper_half
  isdata: false
  to_reference: !<MatrixTransform> {matrix: [0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1], offset: [0.5, 0.5, 0.5, 0]}

- !<ColorSpace>
  name: lin_lg2
  isdata: false
  allocation: lg2
  allocationvars: [-8, 5, 0.00390625]

- !<ColorSpace>
  name: lin_lg2_two
  isdata: false
  allocation: lg2
  allocationvars: [-4, 2]

- !<ColorSpace>
  name: lin_uniform_vars
  isdata: false
  allocation: uniform
  allocationvars: [-0.5, 2]

- !<ColorSpace>
  name: negative
  isdata: false
  from_reference: !<MatrixTransform> {matrix: [-1, 0, 0, 0, 0, -1000, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1], offset: [-0.5, 0.25, -1e-7, 0]}
"#;

fn case(label: &str, calls: &[Call]) -> BakeCase {
    BakeCase::new(label, Some(CONFIG), calls)
}

/// The per-channel targets, which every format bakes.
const PER_CHANNEL: [&str; 7] = [
    "lin",
    "log2",
    "gamma",
    "big",
    "nans",
    "nans_green",
    "negative",
];

/// spi1d: 1D LUTs of every size, with and without a shaper.
fn spi1d() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for target in PER_CHANNEL {
        for size in [2, 3, 10] {
            cases.push(case(
                &format!("spi1d to {target}, {size}"),
                &[
                    InputSpace("lin"),
                    TargetSpace(target),
                    CubeSize(size),
                    Format("spi1d"),
                ],
            ));
        }
        for shaper in ["log2", "gamma", "big", "nans", "nans_green", "negative"] {
            cases.push(case(
                &format!("spi1d to {target}, shaper {shaper}"),
                &[
                    InputSpace("lin"),
                    TargetSpace(target),
                    ShaperSpace(shaper),
                    ShaperSize(3),
                    CubeSize(7),
                    Format("spi1d"),
                ],
            ));
        }
    }
    cases.push(case(
        "spi1d of the default size",
        &[InputSpace("lin"), TargetSpace("log2"), Format("spi1d")],
    ));
    cases.push(case(
        "spi1d from log",
        &[
            InputSpace("log2"),
            TargetSpace("lin"),
            CubeSize(17),
            Format("spi1d"),
        ],
    ));
    cases.push(case(
        "spi1d with a display and view",
        &[
            InputSpace("lin"),
            DisplayView("disp", "log"),
            CubeSize(5),
            Format("spi1d"),
        ],
    ));
    cases.push(
        case(
            "spi1d with metadata",
            &[
                InputSpace("lin"),
                TargetSpace("log2"),
                CubeSize(4),
                Format("spi1d"),
            ],
        )
        .with_metadata(&[Metadata::Child("Desc", "a description")]),
    );
    cases.push(case(
        "spi1d with a look",
        &[
            InputSpace("lin"),
            TargetSpace("log2"),
            Looks("brighten"),
            CubeSize(6),
            Format("spi1d"),
        ],
    ));
    cases.push(case(
        "spi1d with a look and a display and view",
        &[
            InputSpace("lin"),
            DisplayView("disp", "log"),
            Looks("+brighten"),
            CubeSize(6),
            Format("spi1d"),
        ],
    ));
    cases.push(case(
        "spi1d named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("log2"),
            CubeSize(4),
            Format("SPI1D"),
        ],
    ));
    cases
}

/// spi3d: cubes of every size.
fn spi3d() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for target in PER_CHANNEL.iter().chain(["sat"].iter()) {
        for size in [2, 3, 5] {
            cases.push(case(
                &format!("spi3d to {target}, {size}"),
                &[
                    InputSpace("lin"),
                    TargetSpace(target),
                    CubeSize(size),
                    Format("spi3d"),
                ],
            ));
        }
    }
    cases.push(case(
        "spi3d of the default size",
        &[InputSpace("lin"), TargetSpace("sat"), Format("spi3d")],
    ));
    cases.push(case(
        "spi3d with a shaper size",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            ShaperSize(1),
            CubeSize(2),
            Format("spi3d"),
        ],
    ));
    cases.push(
        case(
            "spi3d with metadata",
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                CubeSize(2),
                Format("spi3d"),
            ],
        )
        .with_metadata(&[
            Metadata::Child("Desc", "a description"),
            Metadata::Name("a name"),
            Metadata::Id("an id"),
        ]),
    );
    cases.push(case(
        "spi3d named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("Spi3D"),
        ],
    ));
    cases
}

/// Iridas cube and itx: cubes of every size; the cube writes the metadata's children as
/// comments.
fn cube_and_itx() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for format in ["iridas_cube", "iridas_itx"] {
        for target in PER_CHANNEL.iter().chain(["sat"].iter()) {
            for size in [2, 3] {
                cases.push(case(
                    &format!("{format} to {target}, {size}"),
                    &[
                        InputSpace("lin"),
                        TargetSpace(target),
                        CubeSize(size),
                        Format(format),
                    ],
                ));
            }
        }
        cases.push(case(
            &format!("{format} of the default size"),
            &[InputSpace("lin"), TargetSpace("sat"), Format(format)],
        ));
        cases.push(
            case(
                &format!("{format} with metadata"),
                &[
                    InputSpace("lin"),
                    TargetSpace("gamma"),
                    CubeSize(2),
                    Format(format),
                ],
            )
            .with_metadata(&[
                Metadata::Child("Description", "first"),
                Metadata::Child("Other", "second\nline"),
                Metadata::Child("Empty", ""),
                Metadata::Name("a name"),
                Metadata::Id("an id"),
            ]),
        );
        cases.push(
            case(
                &format!("{format} with one metadata child"),
                &[
                    InputSpace("lin"),
                    TargetSpace("gamma"),
                    CubeSize(2),
                    Format(format),
                ],
            )
            .with_metadata(&[Metadata::Child("Description", "only")]),
        );
        cases.push(case(
            &format!("{format} with a display and view"),
            &[
                InputSpace("lin"),
                DisplayView("disp", "sat"),
                Looks("brighten"),
                CubeSize(4),
                Format(format),
            ],
        ));
    }
    cases.push(case(
        "iridas_cube named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("Iridas_Cube"),
        ],
    ));
    cases.push(case(
        "iridas_itx named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("IRIDAS_ITX"),
        ],
    ));
    cases
}

/// Resolve cube: a 1D LUT without crosstalk (its input range written when not 0 to 1), a 3D
/// LUT with crosstalk, a 1D shaper and a 3D LUT with crosstalk and a shaper space.
fn resolve() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for target in PER_CHANNEL {
        for size in [2, 3] {
            cases.push(case(
                &format!("resolve_cube 1D to {target}, {size}"),
                &[
                    InputSpace("lin"),
                    TargetSpace(target),
                    CubeSize(size),
                    Format("resolve_cube"),
                ],
            ));
        }
        for shaper in [
            "log2",
            "nans",
            "nans_green",
            "negative",
            "double",
            "upper_half",
        ] {
            cases.push(case(
                &format!("resolve_cube 1D to {target}, shaper {shaper}"),
                &[
                    InputSpace("lin"),
                    TargetSpace(target),
                    ShaperSpace(shaper),
                    ShaperSize(9),
                    CubeSize(5),
                    Format("resolve_cube"),
                ],
            ));
        }
    }
    for size in [2, 3] {
        cases.push(case(
            &format!("resolve_cube 3D, {size}"),
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                CubeSize(size),
                Format("resolve_cube"),
            ],
        ));
    }
    for shaper in [
        "lin",
        "log2",
        "gamma",
        "big",
        "nans",
        "nans_green",
        "negative",
    ] {
        for (shaper_size, cube_size) in [(2, 2), (5, 3)] {
            cases.push(case(
                &format!("resolve_cube 1D+3D, shaper {shaper}, {shaper_size} and {cube_size}"),
                &[
                    InputSpace("lin"),
                    TargetSpace("sat"),
                    ShaperSpace(shaper),
                    ShaperSize(shaper_size),
                    CubeSize(cube_size),
                    Format("resolve_cube"),
                ],
            ));
        }
    }
    cases.push(case(
        "resolve_cube 1D with a shaper smaller than the LUT",
        &[
            InputSpace("lin"),
            TargetSpace("gamma"),
            ShaperSpace("double"),
            ShaperSize(3),
            CubeSize(6),
            Format("resolve_cube"),
        ],
    ));
    cases.push(case(
        "resolve_cube 1D of the default size",
        &[
            InputSpace("lin"),
            TargetSpace("gamma"),
            Format("resolve_cube"),
        ],
    ));
    cases.push(case(
        "resolve_cube 3D of the default size",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            Format("resolve_cube"),
        ],
    ));
    cases.push(case(
        "resolve_cube 1D+3D of the default sizes",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            ShaperSpace("log2"),
            Format("resolve_cube"),
        ],
    ));
    cases.push(
        case(
            "resolve_cube with metadata",
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                ShaperSpace("gamma"),
                ShaperSize(3),
                CubeSize(2),
                Format("resolve_cube"),
            ],
        )
        .with_metadata(&[
            Metadata::Child("Description", "first"),
            Metadata::Child("Other", "second"),
        ]),
    );
    cases.push(case(
        "resolve_cube with a look and a display and view",
        &[
            InputSpace("lin"),
            DisplayView("disp", "sat"),
            Looks("brighten"),
            ShaperSpace("log2"),
            ShaperSize(4),
            CubeSize(3),
            Format("resolve_cube"),
        ],
    ));
    cases.push(case(
        "resolve_cube named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("Resolve_Cube"),
        ],
    ));
    cases
}

/// 3DL as flame and lustre: cubes of 12-bit codes after a 10-bit shaper of any size (a
/// negative one is refused as the C++ library refuses the vector, I-200).
fn tdl() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for format in ["flame", "lustre"] {
        for target in PER_CHANNEL.iter().chain(["sat"].iter()) {
            cases.push(case(
                &format!("{format} to {target}, 3"),
                &[
                    InputSpace("lin"),
                    TargetSpace(target),
                    CubeSize(3),
                    Format(format),
                ],
            ));
        }
        for size in [2, 5, 9, 17] {
            cases.push(case(
                &format!("{format} of size {size}"),
                &[
                    InputSpace("lin"),
                    TargetSpace("gamma"),
                    CubeSize(size),
                    Format(format),
                ],
            ));
        }
        cases.push(case(
            &format!("{format} of the default size"),
            &[InputSpace("lin"), TargetSpace("sat"), Format(format)],
        ));
        for shaper in [-5, 0, 1, 2, 7] {
            cases.push(case(
                &format!("{format} with a shaper size of {shaper}"),
                &[
                    InputSpace("lin"),
                    TargetSpace("sat"),
                    ShaperSize(shaper),
                    CubeSize(2),
                    Format(format),
                ],
            ));
        }
    }
    cases.push(case(
        "lustre of size 33",
        &[
            InputSpace("lin"),
            TargetSpace("gamma"),
            CubeSize(33),
            Format("lustre"),
        ],
    ));
    cases.push(case(
        "lustre of size 65",
        &[
            InputSpace("lin"),
            TargetSpace("lin"),
            CubeSize(65),
            Format("lustre"),
        ],
    ));
    cases.push(case(
        "flame named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("Flame"),
        ],
    ));
    cases
}

/// Truelight: a cube after an identity shaper over its indices.
fn truelight() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for target in PER_CHANNEL.iter().chain(["sat"].iter()) {
        cases.push(case(
            &format!("truelight to {target}"),
            &[
                InputSpace("lin"),
                TargetSpace(target),
                ShaperSize(4),
                CubeSize(3),
                Format("truelight"),
            ],
        ));
    }
    for (shaper, cube) in [(-1, 2), (0, 2), (1, 2), (2, 2), (7, 4), (3, 9)] {
        cases.push(case(
            &format!("truelight with shaper {shaper} and cube {cube}"),
            &[
                InputSpace("lin"),
                TargetSpace("gamma"),
                ShaperSize(shaper),
                CubeSize(cube),
                Format("truelight"),
            ],
        ));
    }
    cases.push(case(
        "truelight of the default sizes",
        &[InputSpace("lin"), TargetSpace("sat"), Format("truelight")],
    ));
    cases.push(case(
        "truelight named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("TrueLight"),
        ],
    ));
    cases
}

/// Houdini: a 1D LUT without crosstalk, a 3D LUT with it, a prelut and a 3D LUT with a shaper.
fn houdini() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for target in PER_CHANNEL {
        cases.push(case(
            &format!("houdini 1D to {target}"),
            &[
                InputSpace("lin"),
                TargetSpace(target),
                CubeSize(4),
                Format("houdini"),
            ],
        ));
        cases.push(case(
            &format!("houdini 1D to {target}, shaper log2"),
            &[
                InputSpace("lin"),
                TargetSpace(target),
                ShaperSpace("log2"),
                CubeSize(4),
                Format("houdini"),
            ],
        ));
    }
    for size in [2, 3] {
        cases.push(case(
            &format!("houdini 3D, {size}"),
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                CubeSize(size),
                Format("houdini"),
            ],
        ));
    }
    for shaper in [
        "lin",
        "log2",
        "gamma",
        "big",
        "nans",
        "nans_green",
        "negative",
        "double",
    ] {
        cases.push(case(
            &format!("houdini 3D+1D, shaper {shaper}"),
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                ShaperSpace(shaper),
                ShaperSize(5),
                CubeSize(2),
                Format("houdini"),
            ],
        ));
    }
    cases.push(case(
        "houdini 1D of the default size",
        &[InputSpace("lin"), TargetSpace("gamma"), Format("houdini")],
    ));
    cases.push(case(
        "houdini 3D+1D of the default sizes",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            ShaperSpace("log2"),
            Format("houdini"),
        ],
    ));
    cases.push(case(
        "houdini with a look and a display and view",
        &[
            InputSpace("lin"),
            DisplayView("disp", "sat"),
            Looks("brighten"),
            ShaperSpace("gamma"),
            ShaperSize(3),
            CubeSize(2),
            Format("houdini"),
        ],
    ));
    cases.push(case(
        "houdini named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("Houdini"),
        ],
    ));
    cases
}

/// cineSpace: a cube after the shaper space's prelut, or after one made from the input
/// space's allocation; the metadata's children in its block.
fn cinespace() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for target in PER_CHANNEL.iter().chain(["sat"].iter()) {
        cases.push(case(
            &format!("cinespace to {target}"),
            &[
                InputSpace("lin"),
                TargetSpace(target),
                CubeSize(2),
                Format("cinespace"),
            ],
        ));
    }
    for shaper in [
        "lin",
        "log2",
        "gamma",
        "big",
        "nans",
        "nans_green",
        "negative",
        "double",
    ] {
        for size in [2, 5] {
            cases.push(case(
                &format!("cinespace, shaper {shaper}, {size}"),
                &[
                    InputSpace("lin"),
                    TargetSpace("sat"),
                    ShaperSpace(shaper),
                    ShaperSize(size),
                    CubeSize(2),
                    Format("cinespace"),
                ],
            ));
        }
    }
    for input in ["lin_lg2", "lin_lg2_two", "lin_uniform_vars", "log2"] {
        for shaper_size in [-1, 0, 1, 4] {
            cases.push(case(
                &format!("cinespace from {input}, shaper size {shaper_size}"),
                &[
                    InputSpace(input),
                    TargetSpace("sat"),
                    ShaperSize(shaper_size),
                    CubeSize(2),
                    Format("cinespace"),
                ],
            ));
        }
    }
    cases.push(case(
        "cinespace of the default sizes",
        &[InputSpace("lin"), TargetSpace("sat"), Format("cinespace")],
    ));
    cases.push(case(
        "cinespace of the default sizes with a shaper",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            ShaperSpace("log2"),
            Format("cinespace"),
        ],
    ));
    cases.push(
        case(
            "cinespace with metadata",
            &[
                InputSpace("lin"),
                TargetSpace("gamma"),
                CubeSize(2),
                Format("cinespace"),
            ],
        )
        .with_metadata(&[
            Metadata::Child("Description", "first"),
            Metadata::Child("Other", "END METADATA"),
            Metadata::Child("Empty", ""),
        ]),
    );
    cases.push(case(
        "cinespace named in capitals",
        &[
            InputSpace("lin"),
            TargetSpace("sat"),
            CubeSize(2),
            Format("CineSpace"),
        ],
    ));
    cases
}

#[test]
fn spi1d_bakes_as_in_the_wheel() {
    check_bakes(&spi1d());
}

#[test]
fn spi3d_bakes_as_in_the_wheel() {
    check_bakes(&spi3d());
}

#[test]
fn cube_and_itx_bakes_as_in_the_wheel() {
    check_bakes(&cube_and_itx());
}

#[test]
fn resolve_bakes_as_in_the_wheel() {
    check_bakes(&resolve());
}

#[test]
fn tdl_bakes_as_in_the_wheel() {
    check_bakes(&tdl());
}

#[test]
fn truelight_bakes_as_in_the_wheel() {
    check_bakes(&truelight());
}

#[test]
fn houdini_bakes_as_in_the_wheel() {
    check_bakes(&houdini());
}

#[test]
fn cinespace_bakes_as_in_the_wheel() {
    check_bakes(&cinespace());
}
