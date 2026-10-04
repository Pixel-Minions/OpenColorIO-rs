// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `CDLTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`), the numbers with 6 significant digits, or 16 after a
//!   MatrixTransform in the same group (I-73), and the style; its validation; `equals()` between
//!   CDLs that differ in each part, and within the data's tolerance;
//! - its ID and first SOP description, which live in its metadata, as the processor's
//!   `createGroupTransform()` reports them;
//! - the raw config's processor of each, in both directions: `BuildCDLOp` makes a CDL op, and
//!   `CreateCDLTransform` the transform of it;
//! - a version 1 config's processor of each: `BuildCDLOp` makes a scale and offset, an exponent
//!   and a saturation (Matrix and Exponent ops, whatever the style), which come back as Matrix
//!   and Exponent transforms.

mod common;

use std::sync::Arc;

use common::transforms::{
    Case, check_processors, check_processors_in, check_text, direction_spec, group, special_doubles,
};
use ocio::{CdlStyle, CdlTransform, Config, MatrixTransform, TransformDirection};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// A version 1 config with one color space.
const V1_CONFIG: &str = "ocio_profile_version: 1\n\
roles: {default: raw}\n\
colorspaces:\n  \
- !<ColorSpace> {name: raw}\n";

/// Three doubles as a spec value.
fn f64s(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|&v| f64_spec(v)).collect())
}

/// A CDL's parameters.
#[derive(Debug, Clone)]
struct Params {
    slope: [f64; 3],
    offset: [f64; 3],
    power: [f64; 3],
    sat: f64,
    style: CdlStyle,
    dir: TransformDirection,
    id: &'static str,
    description: &'static str,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            slope: [1.0; 3],
            offset: [0.0; 3],
            power: [1.0; 3],
            sat: 1.0,
            style: CdlStyle::NoClamp,
            dir: Forward,
            id: "",
            description: "",
        }
    }
}

/// The case of `p`, built through the setters (the binding's constructor validates); `sop`
/// sets the parameters with `setSOP` instead of one by one.
fn cdl_case(label: &str, p: &Params, sop: bool) -> Case {
    let mut port = CdlTransform::new();
    let mut calls = Vec::new();
    if sop {
        let mut vec9 = [0.0; 9];
        vec9[..3].copy_from_slice(&p.slope);
        vec9[3..6].copy_from_slice(&p.offset);
        vec9[6..].copy_from_slice(&p.power);
        port.set_sop(&vec9);
        calls.push(json!(["setSOP", f64s(&vec9)]));
    } else {
        port.set_slope(&p.slope);
        port.set_offset(&p.offset);
        port.set_power(&p.power);
        calls.push(json!(["setSlope", f64s(&p.slope)]));
        calls.push(json!(["setOffset", f64s(&p.offset)]));
        calls.push(json!(["setPower", f64s(&p.power)]));
    }
    port.set_sat(p.sat);
    // The style is set in the current direction: before the direction, and after it.
    port.set_style(p.style);
    port.set_direction(p.dir);
    port.set_style(p.style);
    port.set_id(p.id.as_bytes());
    port.set_first_sop_description(p.description.as_bytes());
    let style = match p.style {
        CdlStyle::Asc => "CDL_ASC",
        CdlStyle::NoClamp => "CDL_NO_CLAMP",
    };
    calls.extend([
        json!(["setSat", f64_spec(p.sat)]),
        json!(["setStyle", {"enum": style}]),
        json!(["setDirection", direction_spec(p.dir)]),
        json!(["setStyle", {"enum": style}]),
        json!(["setID", p.id]),
        json!(["setFirstSOPDescription", p.description]),
    ]);
    Case::new(
        format!("{label}{}", if sop { ", by SOP" } else { "" }),
        json!({"class": "CDLTransform", "calls": calls}),
        port,
    )
}

/// The cases: the default, upstream's values in every style and direction, the special doubles
/// in each parameter, values the data's validation refuses, IDs and descriptions.
fn cases() -> Vec<Case> {
    let mut out = vec![Case::new(
        "the default",
        json!({"class": "CDLTransform"}),
        CdlTransform::new(),
    )];
    // `CDL_DATA_1` (tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66 @ v2.5.2).
    let upstream = Params {
        slope: [1.35, 1.1, 0.071],
        offset: [0.05, -0.23, 0.11],
        power: [0.93, 0.81, 1.27],
        sat: 1.23,
        id: "Test look: 01-A.",
        description: "SOP Desc",
        ..Params::default()
    };
    for style in [CdlStyle::Asc, CdlStyle::NoClamp] {
        for dir in [Forward, Inverse] {
            let p = Params {
                style,
                dir,
                ..upstream.clone()
            };
            let label = format!("upstream's, {style:?}, {dir:?}");
            out.push(cdl_case(&label, &p, false));
            out.push(cdl_case(&label, &p, true));
        }
    }
    let specials = special_doubles();
    for (k, &v) in specials.iter().enumerate() {
        let w = specials[(k + 5) % specials.len()];
        let dir = if k % 2 == 0 { Forward } else { Inverse };
        let style = if k % 3 == 0 {
            CdlStyle::Asc
        } else {
            CdlStyle::NoClamp
        };
        let base = Params {
            dir,
            style,
            ..upstream.clone()
        };
        let label = |name: &str| format!("special {k} ({v:e}) as {name}, {style:?}, {dir:?}");
        out.push(cdl_case(
            &label("slope"),
            &Params {
                slope: [v, 1.0, w],
                ..base.clone()
            },
            false,
        ));
        out.push(cdl_case(
            &label("offset"),
            &Params {
                offset: [w, v, 0.5],
                ..base.clone()
            },
            false,
        ));
        out.push(cdl_case(
            &label("power"),
            &Params {
                power: [1.0, w, v],
                ..base.clone()
            },
            false,
        ));
        out.push(cdl_case(
            &label("sat"),
            &Params {
                sat: v,
                ..base.clone()
            },
            false,
        ));
    }
    for (label, id, description) in [
        ("no id, no description", "", ""),
        ("an id with spaces", "id with spaces", ""),
        ("a description only", "", "the first SOP"),
        // A C string ends at its first NUL.
        (
            "a NUL in the id and the description",
            "an\0id",
            "the first\0SOP",
        ),
        ("a description that starts with a NUL", "", "\0first SOP"),
    ] {
        out.push(cdl_case(
            label,
            &Params {
                id,
                description,
                ..upstream.clone()
            },
            false,
        ));
    }
    out
}

/// Groups that show a MatrixTransform's precision on the CDLs after it (I-73).
fn group_cases() -> Vec<Case> {
    let digits = cdl_case(
        "many digits",
        &Params {
            slope: [1.0 / 3.0, 1.123456789, 0.5],
            offset: [0.0123456789, 0.0, -1.0 / 7.0],
            power: [2.0 / 3.0, 1.0, 1.25],
            sat: 1.0 / 3.0,
            ..Params::default()
        },
        false,
    );
    let matrix = Case::new(
        "a matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    vec![
        group(
            "CDLs around a matrix",
            Forward,
            &[digits.clone(), matrix.clone(), digits.clone()],
        ),
        group(
            "a CDL after a nested matrix",
            Inverse,
            &[
                group("a matrix", Forward, std::slice::from_ref(&matrix)),
                digits,
            ],
        ),
    ]
}

#[test]
fn text_validation_and_equality_match_the_wheel() {
    let mut cases = cases();
    let n = cases.len();
    let copies = cases.clone();
    cases.extend(copies);
    let mut pairs = Vec::new();
    for i in 0..n {
        pairs.extend([(i, i), (i, (i + 1) % n), (i, (i + 2) % n), (i, i + n)]);
    }
    // Parameters within the data's tolerance of 1e-9, and just past it; a saturation off by
    // 0.002 (CDLTransform_tests.cpp:30 @ v2.5.2).
    let first = cases.len();
    let base = Params::default();
    for (label, p) in [
        ("the base", base.clone()),
        (
            "a slope 1e-10 away",
            Params {
                slope: [1.0 + 1e-10, 1.0, 1.0],
                ..base.clone()
            },
        ),
        (
            "a slope 1e-8 away",
            Params {
                slope: [1.0 + 1e-8, 1.0, 1.0],
                ..base.clone()
            },
        ),
        (
            "an offset 1e-10 away",
            Params {
                offset: [0.0, -1e-10, 0.0],
                ..base.clone()
            },
        ),
        (
            "a power 1e-10 away",
            Params {
                power: [1.0, 1.0, 1.0 - 1e-10],
                ..base.clone()
            },
        ),
        (
            "a saturation 1e-10 away",
            Params {
                sat: 1.0 + 1e-10,
                ..base.clone()
            },
        ),
        (
            "a saturation 0.002 away",
            Params {
                sat: 1.0 + f64::from(0.002f32),
                ..base.clone()
            },
        ),
        (
            "ASC",
            Params {
                style: CdlStyle::Asc,
                ..base.clone()
            },
        ),
        (
            "another id",
            Params {
                id: "other",
                ..base.clone()
            },
        ),
    ] {
        cases.push(cdl_case(label, &p, false));
    }
    for i in first..cases.len() {
        for j in first..cases.len() {
            if i != j {
                pairs.push((i, j));
            }
        }
    }
    let g = cases.len();
    cases.extend(group_cases());
    pairs.extend([(g, g), (g, 0)]);
    check_text(&cases, &pairs);
}

#[test]
fn processors_match_the_wheel() {
    let mut cases = cases();
    cases.extend(group_cases());
    check_processors(&cases);
}

#[test]
fn version_1_processors_match_the_wheel() {
    let mut config = Config::create_raw();
    Arc::get_mut(&mut config)
        .unwrap()
        .set_major_version(1)
        .unwrap();
    check_processors_in(&cases(), Some(&json!({"yaml": V1_CONFIG})), &config);
}
