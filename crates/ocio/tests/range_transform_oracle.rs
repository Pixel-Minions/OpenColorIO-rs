// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `RangeTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the style when it doesn't clamp, the bounds that are set,
//!   with 6 significant digits, or 16 after a MatrixTransform in the same group (I-73); its
//!   validation, every message of the data's and the transform's own (I-75); `equals()`
//!   between ranges that differ in each part, and within the data's tolerance in both orders;
//! - the raw config's processor of each, in both directions: `BuildRangeOp` (a Range op, or a
//!   Matrix op when the range doesn't clamp), then `CreateRangeTransform` or
//!   `CreateMatrixTransform` through `createGroupTransform()`.

mod common;

use common::transforms::{
    BIT_DEPTHS, Case, bit_depth_spec, check_processors, check_text, direction_spec, group,
    special_doubles,
};
use ocio::{BitDepth, MatrixTransform, RangeStyle, RangeTransform, TransformDirection};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// A range transform's parameters: `None` leaves a bound unset (the binding has no way to pass
/// one to the constructor without validating it).
#[derive(Debug, Clone, Copy)]
struct Params {
    min_in: Option<f64>,
    max_in: Option<f64>,
    min_out: Option<f64>,
    max_out: Option<f64>,
    style: RangeStyle,
    dir: TransformDirection,
    depths: (BitDepth, BitDepth),
}

impl Default for Params {
    fn default() -> Self {
        Params {
            min_in: None,
            max_in: None,
            min_out: None,
            max_out: None,
            style: RangeStyle::Clamp,
            dir: Forward,
            depths: (BitDepth::Unknown, BitDepth::Unknown),
        }
    }
}

impl Params {
    /// Both bounds set.
    fn bounds(min_in: f64, max_in: f64, min_out: f64, max_out: f64) -> Params {
        Params {
            min_in: Some(min_in),
            max_in: Some(max_in),
            min_out: Some(min_out),
            max_out: Some(max_out),
            ..Params::default()
        }
    }
}

/// The style, as the binding names it.
fn style_spec(style: RangeStyle) -> Value {
    json!({"enum": match style {
        RangeStyle::Clamp => "RANGE_CLAMP",
        RangeStyle::NoClamp => "RANGE_NO_CLAMP",
    }})
}

/// The case of `params`, built through the setters (the binding's constructor validates).
fn range_case(label: impl Into<String>, params: Params) -> Case {
    let mut port = RangeTransform::new();
    let mut calls = Vec::new();
    for (value, setter, set) in [
        (
            params.min_in,
            "setMinInValue",
            RangeTransform::set_min_in_value as fn(&mut RangeTransform, f64),
        ),
        (
            params.max_in,
            "setMaxInValue",
            RangeTransform::set_max_in_value,
        ),
        (
            params.min_out,
            "setMinOutValue",
            RangeTransform::set_min_out_value,
        ),
        (
            params.max_out,
            "setMaxOutValue",
            RangeTransform::set_max_out_value,
        ),
    ] {
        if let Some(value) = value {
            set(&mut port, value);
            calls.push(json!([setter, f64_spec(value)]));
        }
    }
    port.set_style(params.style);
    port.set_direction(params.dir);
    port.set_file_input_bit_depth(params.depths.0);
    port.set_file_output_bit_depth(params.depths.1);
    calls.extend([
        json!(["setStyle", style_spec(params.style)]),
        json!(["setDirection", direction_spec(params.dir)]),
        json!(["setFileInputBitDepth", bit_depth_spec(params.depths.0)]),
        json!(["setFileOutputBitDepth", bit_depth_spec(params.depths.1)]),
    ]);
    Case::new(
        label,
        json!({"class": "RangeTransform", "calls": calls}),
        port,
    )
}

/// A range case unset with `unset*` after its bounds were set: a NaN bound and an unset one are
/// the same.
fn unset_case(label: &str, params: Params, unset: &[usize]) -> Case {
    let mut case = range_case(label, params);
    let names = [
        "unsetMinInValue",
        "unsetMaxInValue",
        "unsetMinOutValue",
        "unsetMaxOutValue",
    ];
    let ocio::Transform::Range(port) = &mut case.port else {
        unreachable!()
    };
    let calls = case.spec["calls"].as_array_mut().unwrap();
    for &i in unset {
        calls.push(json!([names[i]]));
        match i {
            0 => port.unset_min_in_value(),
            1 => port.unset_max_in_value(),
            2 => port.unset_min_out_value(),
            _ => port.unset_max_out_value(),
        }
    }
    case
}

/// The range cases: upstream's values, the default, one-sided ranges, every style, direction
/// and bit depth, the special doubles as bounds, and each of the validation's errors.
fn range_cases() -> Vec<Case> {
    let mut cases = vec![Case::new(
        "the default",
        json!({"class": "RangeTransform"}),
        RangeTransform::new(),
    )];
    // tests/cpu/transforms/RangeTransform_tests.cpp:30-58 @ v2.5.2.
    let upstream = Params {
        depths: (BitDepth::Uint8, BitDepth::Uint10),
        ..Params::bounds(-1.5, -0.5, 1.5, 4.5)
    };
    for style in [RangeStyle::Clamp, RangeStyle::NoClamp] {
        for dir in [Forward, Inverse] {
            cases.push(range_case(
                format!("upstream's, {style:?}, {dir:?}"),
                Params {
                    style,
                    dir,
                    ..upstream
                },
            ));
            for (label, params) in [
                (
                    "min only",
                    Params {
                        min_in: Some(-0.5),
                        min_out: Some(-0.5),
                        ..Params::default()
                    },
                ),
                (
                    "max only",
                    Params {
                        max_in: Some(1.25),
                        max_out: Some(1.25),
                        ..Params::default()
                    },
                ),
                (
                    "a min in only",
                    Params {
                        min_in: Some(0.0),
                        ..Params::default()
                    },
                ),
                (
                    "a min out only",
                    Params {
                        min_out: Some(0.0),
                        max_in: Some(1.0),
                        max_out: Some(1.0),
                        ..Params::default()
                    },
                ),
                (
                    "a max out only",
                    Params {
                        max_out: Some(1.0),
                        min_in: Some(0.0),
                        min_out: Some(0.0),
                        ..Params::default()
                    },
                ),
                (
                    "a max in only",
                    Params {
                        max_in: Some(1.0),
                        ..Params::default()
                    },
                ),
                ("min in above max in", Params::bounds(1.0, 0.0, 0.0, 1.0)),
                ("min out above max out", Params::bounds(0.0, 1.0, 1.0, 0.0)),
                (
                    "different one-sided bounds",
                    Params {
                        min_in: Some(0.1),
                        min_out: Some(0.2),
                        ..Params::default()
                    },
                ),
                (
                    "different max-only bounds",
                    Params {
                        max_in: Some(0.9),
                        max_out: Some(0.8),
                        ..Params::default()
                    },
                ),
                (
                    "input bounds too close",
                    Params::bounds(0.5, 0.5000005, 0.0, 1.0),
                ),
                ("a constant", Params::bounds(0.0, 1.0, 0.5, 0.5)),
                (
                    "one-sided bounds within the tolerance",
                    Params {
                        min_in: Some(1e-3),
                        min_out: Some(9.995e-4),
                        ..Params::default()
                    },
                ),
                (
                    "one-sided bounds within the tolerance, the other order",
                    Params {
                        min_in: Some(9.995e-4),
                        min_out: Some(1e-3),
                        ..Params::default()
                    },
                ),
            ] {
                cases.push(range_case(
                    format!("{label}, {style:?}, {dir:?}"),
                    Params {
                        style,
                        dir,
                        ..params
                    },
                ));
            }
        }
    }
    for (i, &depth) in BIT_DEPTHS.iter().enumerate() {
        cases.push(range_case(
            format!("bit depths {depth:?}"),
            Params {
                depths: (depth, BIT_DEPTHS[(i + 5) % BIT_DEPTHS.len()]),
                ..Params::bounds(0.0, 1.0, 0.0, 1.0)
            },
        ));
    }
    let specials = special_doubles();
    for (k, &v) in specials.iter().enumerate() {
        let w = specials[(k + 9) % specials.len()];
        for (label, params) in [
            ("min", Params::bounds(v, 1.0, w, 2.0)),
            ("max", Params::bounds(-1.0, v, 0.0, w)),
            (
                "one-sided",
                Params {
                    min_in: Some(v),
                    min_out: Some(v),
                    ..Params::default()
                },
            ),
            (
                "one-sided, different",
                Params {
                    max_in: Some(v),
                    max_out: Some(w),
                    ..Params::default()
                },
            ),
        ] {
            for style in [RangeStyle::Clamp, RangeStyle::NoClamp] {
                cases.push(range_case(
                    format!("special {k} ({v:e}) as {label}, {style:?}"),
                    Params { style, ..params },
                ));
            }
        }
    }
    cases.push(unset_case(
        "upstream's, its bounds unset",
        upstream,
        &[0, 1, 2, 3],
    ));
    cases.push(unset_case("upstream's, its min in unset", upstream, &[0]));
    cases
}

/// Groups that show a MatrixTransform's precision on the ranges after it (I-73): a range
/// before and after a matrix, in nested groups too.
fn group_cases(ranges: &[Case]) -> Vec<Case> {
    let range = range_case(
        "a range of many digits",
        Params::bounds(0.1234567891234, 1.0 / 3.0, -2.0 / 3.0, 123_456_789.125),
    );
    let one_sided = range_case(
        "a one-sided range",
        Params {
            max_in: Some(1.0 / 7.0),
            max_out: Some(1.0 / 7.0),
            ..Params::default()
        },
    );
    let matrix = Case::new(
        "a matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    let inner = group(
        "a group with a matrix",
        Inverse,
        std::slice::from_ref(&matrix),
    );
    vec![
        group(
            "a range, a matrix, the range",
            Forward,
            &[
                range.clone(),
                matrix.clone(),
                range.clone(),
                one_sided.clone(),
            ],
        ),
        group(
            "a nested matrix, then ranges",
            Forward,
            &[
                range.clone(),
                inner.clone(),
                one_sided.clone(),
                range.clone(),
            ],
        ),
        group(
            "ranges in a group after a matrix",
            Inverse,
            &[
                matrix,
                group("ranges", Forward, &[one_sided, range.clone()]),
            ],
        ),
        group("ranges only", Forward, &[range, ranges[1].clone()]),
    ]
}

#[test]
fn text_validation_and_equality_match_the_wheel() {
    let mut cases = range_cases();
    let n = cases.len();
    // Each range case built again.
    let copies: Vec<Case> = cases.clone();
    cases.extend(copies);
    let mut pairs = Vec::new();
    for i in 0..n {
        pairs.extend([(i, i), (i, (i + 1) % n), (i, i + n)]);
    }
    // Ranges that differ from one in one part each, and bounds within the data's tolerance.
    let base = Params::bounds(-0.5, 2.0, 0.1, 3.0);
    let variants = [
        ("the base", base),
        (
            "other file bit depths",
            Params {
                depths: (BitDepth::F32, BitDepth::Uint8),
                ..base
            },
        ),
        (
            "no clamp",
            Params {
                style: RangeStyle::NoClamp,
                ..base
            },
        ),
        (
            "inverse",
            Params {
                dir: Inverse,
                ..base
            },
        ),
        ("another min in", Params::bounds(-0.4, 2.0, 0.1, 3.0)),
        ("another max out", Params::bounds(-0.5, 2.0, 0.1, 3.5)),
        (
            "a min in 1e-7 away",
            Params::bounds(-0.5000001, 2.0, 0.1, 3.0),
        ),
        (
            "a max out 1e-5 away",
            Params::bounds(-0.5, 2.0, 0.1, 3.00001),
        ),
        (
            "max only",
            Params {
                max_in: Some(2.0),
                max_out: Some(2.0),
                ..Params::default()
            },
        ),
        (
            "max only, within the tolerance",
            Params {
                max_in: Some(2.0000001),
                max_out: Some(2.0000001),
                ..Params::default()
            },
        ),
        (
            "min only, near 1e-3",
            Params {
                min_in: Some(9.995e-4),
                min_out: Some(9.995e-4),
                ..Params::default()
            },
        ),
        (
            "min only, 1e-3",
            Params {
                min_in: Some(1e-3),
                min_out: Some(1e-3),
                ..Params::default()
            },
        ),
    ];
    let first = cases.len();
    for (label, params) in variants {
        cases.push(range_case(label, params));
    }
    for i in first..cases.len() {
        for j in first..cases.len() {
            if i != j {
                pairs.push((i, j));
            }
        }
    }
    // Groups have no equals() in the binding, and a range compares only with a range.
    let groups = group_cases(&cases);
    let g = cases.len();
    cases.extend(groups);
    let matrix = cases.len();
    cases.push(Case::new(
        "a matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    ));
    pairs.extend([(g, g), (g, 1), (1, matrix), (matrix, 1)]);
    check_text(&cases, &pairs);
}

#[test]
fn processors_match_the_wheel() {
    let mut cases = range_cases();
    let groups = group_cases(&cases);
    cases.extend(groups);
    check_processors(&cases);
}
