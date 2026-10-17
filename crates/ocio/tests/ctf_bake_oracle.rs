// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF and CTF bakes against the wheel (`bake`), byte for byte: a 1D LUT without
//! crosstalk (after a Range from a shaper space's range), a 3D LUT with crosstalk, a 1D shaper
//! (half-domain by default, else of the shaper size after a Range) and a 3D LUT with crosstalk
//! and a shaper space; the default sizes, the metadata the writer keeps, the format's names,
//! the LUTs' size limits, and targets that give NaNs, infinities, negative and large values.
//! The values come from the CPU processors, so this target is one of the `cpu-tests`.

mod bake_common;

use bake_common::{BakeCase, Call, Metadata, check_bakes};

use Call::*;

/// The CLF's and the CTF's format names.
const CLF: &str = "Academy/ASC Common LUT Format";
const CTF: &str = "Color Transform Format";

/// Color spaces for every kind of value: per-channel transforms, saturations (crosstalk),
/// values past `float`'s range, NaNs, negatives, and shapers of every range.
const CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: lin

file_rules:
  - !<Rule> {name: Default, colorspace: lin}

displays:
  disp:
    - !<View> {name: log, colorspace: log2}
    - !<View> {name: sat, colorspace: sat}
    - !<View> {name: looked, colorspace: log2, looks: brighten}

looks:
  - !<Look>
    name: brighten
    process_space: lin
    transform: !<ExponentTransform> {value: [1.1, 0.9, 1.2, 1]}
  - !<Look>
    name: satlook
    process_space: lin
    transform: !<CDLTransform> {sat: 1.5}

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
  name: sat_nans
  isdata: false
  from_reference: !<GroupTransform>
    children:
      - !<CDLTransform> {sat: 0.5}
      - !<MatrixTransform> {offset: [.nan, -.inf, 0.25, 0]}

- !<ColorSpace>
  name: sat_big
  isdata: false
  from_reference: !<GroupTransform>
    children:
      - !<CDLTransform> {sat: 1.5, style: noclamp}
      - !<MatrixTransform> {matrix: [1e30, 0, 0, 0, 0, -3e20, 0, 0, 0, 0, 1e-30, 0, 0, 0, 0, 1]}

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
  name: shaper
  isdata: false
  from_reference: !<MatrixTransform> {matrix: [0.8, 0, 0, 0, 0, 0.8, 0, 0, 0, 0, 0.8, 0, 0, 0, 0, 1], offset: [0.1, 0.1, 0.1, 0]}

- !<ColorSpace>
  name: negative
  isdata: false
  from_reference: !<MatrixTransform> {matrix: [-1, 0, 0, 0, 0, -1000, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1], offset: [-0.5, 0.25, -1e-7, 0]}
"#;

fn case(label: &str, calls: &[Call]) -> BakeCase {
    BakeCase::new(label, Some(CONFIG), calls)
}

/// The per-channel targets, which bake a 1D LUT.
const PER_CHANNEL: [&str; 7] = [
    "lin",
    "log2",
    "gamma",
    "big",
    "nans",
    "nans_green",
    "negative",
];

/// The targets with crosstalk, which bake a 3D LUT.
const CROSSTALK: [&str; 3] = ["sat", "sat_nans", "sat_big"];

/// The shaper spaces: ranges inside, around and outside 0 to 1, exactly 0 to 1 (no Range),
/// and of NaNs.
const SHAPERS: [&str; 8] = [
    "lin",
    "log2",
    "shaper",
    "double",
    "upper_half",
    "negative",
    "nans",
    "nans_green",
];

/// 1D LUTs: every per-channel target, with and without a shaper space (a Range from its
/// range), and the default size.
fn one_d() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for format in [CLF, CTF] {
        for target in PER_CHANNEL {
            for size in [2, 3, 10] {
                cases.push(case(
                    &format!("{format} 1D to {target}, {size}"),
                    &[
                        InputSpace("lin"),
                        TargetSpace(target),
                        CubeSize(size),
                        Format(format),
                    ],
                ));
            }
        }
        for target in ["log2", "gamma", "nans"] {
            for shaper in SHAPERS {
                cases.push(case(
                    &format!("{format} 1D to {target}, shaper {shaper}"),
                    &[
                        InputSpace("lin"),
                        TargetSpace(target),
                        ShaperSpace(shaper),
                        ShaperSize(7),
                        CubeSize(5),
                        Format(format),
                    ],
                ));
            }
        }
        cases.push(case(
            &format!("{format} 1D of the default size"),
            &[InputSpace("lin"), TargetSpace("log2"), Format(format)],
        ));
        cases.push(case(
            &format!("{format} 1D from log"),
            &[
                InputSpace("log2"),
                TargetSpace("lin"),
                CubeSize(17),
                Format(format),
            ],
        ));
    }
    cases
}

/// 3D LUTs: every target with crosstalk, of several sizes.
fn three_d() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for format in [CLF, CTF] {
        for target in CROSSTALK {
            for size in [2, 3, 5] {
                cases.push(case(
                    &format!("{format} 3D to {target}, {size}"),
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
            &format!("{format} 3D with a shaper size and no shaper space"),
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                ShaperSize(5),
                CubeSize(2),
                Format(format),
            ],
        ));
    }
    cases.push(case(
        "CLF 3D of the default size",
        &[InputSpace("lin"), TargetSpace("sat"), Format(CLF)],
    ));
    cases
}

/// 1D shapers and 3D LUTs: every shaper space, of a shaper size (a Range from its range), and
/// the default half-domain shaper.
fn shaper_and_cube() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for format in [CLF, CTF] {
        for target in CROSSTALK {
            for shaper in SHAPERS {
                for shaper_size in [3, 10] {
                    cases.push(case(
                        &format!("{format} 1D+3D to {target}, shaper {shaper} of {shaper_size}"),
                        &[
                            InputSpace("lin"),
                            TargetSpace(target),
                            ShaperSpace(shaper),
                            ShaperSize(shaper_size),
                            CubeSize(2),
                            Format(format),
                        ],
                    ));
                }
            }
        }
        for shaper in ["shaper", "log2", "nans"] {
            cases.push(case(
                &format!("{format} 1D+3D to sat, half-domain shaper {shaper}"),
                &[
                    InputSpace("lin"),
                    TargetSpace("sat"),
                    ShaperSpace(shaper),
                    CubeSize(2),
                    Format(format),
                ],
            ));
        }
    }
    cases
}

/// The metadata the writer keeps (descriptions, the first input and output descriptors, the
/// info, the name and id) and drops; display, view and looks; the formats' names as the
/// registry finds them but the bake doesn't take them.
fn others() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for format in [CLF, CTF] {
        for (target, size) in [("log2", 3), ("sat", 2)] {
            cases.push(
                case(
                    &format!("{format} to {target} with metadata"),
                    &[
                        InputSpace("lin"),
                        TargetSpace(target),
                        CubeSize(size),
                        Format(format),
                    ],
                )
                .with_metadata(&[
                    Metadata::Id("an id"),
                    Metadata::Name("a name"),
                    Metadata::Child("Description", "first <line> & \"quoted\" 'text'"),
                    Metadata::Child("Description", "second"),
                    Metadata::Child("Anything", "not saved"),
                    Metadata::Child("InputDescriptor", "input"),
                    Metadata::Child("InputDescriptor", "only the first is saved"),
                    Metadata::Child("OutputDescriptor", "output"),
                    Metadata::Child("Info", "info"),
                ]),
            );
        }
        cases.push(case(
            &format!("{format} with a display, a view and looks"),
            &[
                InputSpace("lin"),
                DisplayView("disp", "looked"),
                Looks("+satlook"),
                CubeSize(3),
                Format(format),
            ],
        ));
        cases.push(case(
            &format!("{format} with a display and view"),
            &[
                InputSpace("lin"),
                DisplayView("disp", "log"),
                CubeSize(4),
                Format(format),
            ],
        ));
    }
    for name in [
        "academy/asc common lut format",
        "COLOR TRANSFORM FORMAT",
        "clf",
        "ctf",
    ] {
        cases.push(case(
            &format!("format named {name}"),
            &[
                InputSpace("lin"),
                TargetSpace("log2"),
                CubeSize(2),
                Format(name),
            ],
        ));
    }
    cases
}

/// The LUTs' size limits, which the ops' data check once the values are baked: a 3D LUT of
/// more than 129 per side, a 1D LUT or shaper of more than 1,048,576 entries.
fn limits() -> Vec<BakeCase> {
    let mut cases = Vec::new();
    for format in [CLF, CTF] {
        cases.push(case(
            &format!("{format} 3D of 130"),
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                CubeSize(130),
                Format(format),
            ],
        ));
        cases.push(case(
            &format!("{format} 1D of 1048577"),
            &[
                InputSpace("lin"),
                TargetSpace("log2"),
                CubeSize(1_048_577),
                Format(format),
            ],
        ));
        cases.push(case(
            &format!("{format} shaper of 1048577"),
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                ShaperSpace("log2"),
                ShaperSize(1_048_577),
                CubeSize(2),
                Format(format),
            ],
        ));
    }
    cases
}

#[test]
fn one_d_bakes_as_in_the_wheel() {
    check_bakes(&one_d());
}

#[test]
fn three_d_bakes_as_in_the_wheel() {
    check_bakes(&three_d());
}

#[test]
fn shaper_and_cube_bakes_as_in_the_wheel() {
    check_bakes(&shaper_and_cube());
}

#[test]
fn other_bakes_as_in_the_wheel() {
    check_bakes(&others());
}

#[test]
fn size_limits_as_in_the_wheel() {
    check_bakes(&limits());
}

/// Sizes whose `int` buffer size overflows to a negative value in the Windows wheel, which
/// then refuses them before it writes anything ("vector too long", docs/improvements.md,
/// U-77): the smallest such cube (895) and 1D LUT (715,827,883). Only Windows, as in
/// `bake_formats_oracle.rs`.
#[cfg(target_os = "windows")]
#[test]
fn overflowing_sizes_fail_as_in_the_windows_wheel() {
    let mut cases = Vec::new();
    for format in [CLF, CTF] {
        cases.push(case(
            &format!("{format} of cube size 895"),
            &[
                InputSpace("lin"),
                TargetSpace("sat"),
                CubeSize(895),
                Format(format),
            ],
        ));
        cases.push(case(
            &format!("{format} 1D of size 715827883"),
            &[
                InputSpace("lin"),
                TargetSpace("gamma"),
                CubeSize(715_827_883),
                Format(format),
            ],
        ));
    }
    check_bakes(&cases);
}
