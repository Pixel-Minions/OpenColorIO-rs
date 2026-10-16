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

#[test]
fn spi1d_bakes_as_in_the_wheel() {
    check_bakes(&spi1d());
}

#[test]
fn spi3d_bakes_as_in_the_wheel() {
    check_bakes(&spi3d());
}
