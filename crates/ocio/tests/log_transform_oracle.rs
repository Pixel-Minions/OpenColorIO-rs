// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `LogTransform`, `LogAffineTransform` and `LogCameraTransform` against the wheel
//! (`common::transforms`):
//! - their text (`repr()`, `str()`), the numbers with 6 significant digits, or 16 after a
//!   MatrixTransform in the same group (I-73), the linear slope only when it is set; their
//!   validation; `equals()` between transforms that differ in each part;
//! - the raw config's processor of each, in both directions: `BuildLogOp` makes a Log op, and
//!   `CreateLogTransform` makes the class its parameters call for (a simple log comes back as a
//!   `LogTransform`), through `createGroupTransform()`.

mod common;

use common::transforms::{
    Case, check_processors, check_text, direction_spec, group, special_doubles,
};
use ocio::{
    LogAffineTransform, LogCameraTransform, LogTransform, MatrixTransform, Transform,
    TransformDirection,
};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// Three doubles as a spec value.
fn f64s(values: &[f64; 3]) -> Value {
    Value::Array(values.iter().map(|&v| f64_spec(v)).collect())
}

/// The parameters of a log transform, the ones each class takes.
#[derive(Debug, Clone, Copy)]
struct Params {
    base: f64,
    log_side_slope: [f64; 3],
    log_side_offset: [f64; 3],
    lin_side_slope: [f64; 3],
    lin_side_offset: [f64; 3],
    lin_side_break: [f64; 3],
    linear_slope: Option<[f64; 3]>,
    dir: TransformDirection,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            base: 2.0,
            log_side_slope: [1.0; 3],
            log_side_offset: [0.0; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
            lin_side_break: [0.1; 3],
            linear_slope: None,
            dir: Forward,
        }
    }
}

/// A LogTransform of `p`'s base and direction.
fn log_case(label: &str, p: Params) -> Case {
    let mut port = LogTransform::new();
    port.set_base(p.base);
    port.set_direction(p.dir);
    Case::new(
        format!("log, {label}"),
        json!({"class": "LogTransform", "calls": [
            ["setBase", f64_spec(p.base)],
            ["setDirection", direction_spec(p.dir)],
        ]}),
        port,
    )
}

/// The calls that set the affine parameters.
fn affine_calls(p: &Params) -> Vec<Value> {
    vec![
        json!(["setBase", f64_spec(p.base)]),
        json!(["setLogSideSlopeValue", f64s(&p.log_side_slope)]),
        json!(["setLogSideOffsetValue", f64s(&p.log_side_offset)]),
        json!(["setLinSideSlopeValue", f64s(&p.lin_side_slope)]),
        json!(["setLinSideOffsetValue", f64s(&p.lin_side_offset)]),
        json!(["setDirection", direction_spec(p.dir)]),
    ]
}

/// A LogAffineTransform of `p`, built through the setters (the binding's constructor
/// validates).
fn affine_case(label: &str, p: Params) -> Case {
    let mut port = LogAffineTransform::new();
    port.set_base(p.base);
    port.set_log_side_slope_value(&p.log_side_slope);
    port.set_log_side_offset_value(&p.log_side_offset);
    port.set_lin_side_slope_value(&p.lin_side_slope);
    port.set_lin_side_offset_value(&p.lin_side_offset);
    port.set_direction(p.dir);
    Case::new(
        format!("affine, {label}"),
        json!({"class": "LogAffineTransform", "calls": affine_calls(&p)}),
        port,
    )
}

/// A LogCameraTransform of `p`: the break through the constructor (the binding's doesn't
/// validate), the rest through the setters.
fn camera_case(label: &str, p: Params) -> Case {
    let mut port = LogCameraTransform::new(&p.lin_side_break);
    port.set_base(p.base);
    port.set_log_side_slope_value(&p.log_side_slope);
    port.set_log_side_offset_value(&p.log_side_offset);
    port.set_lin_side_slope_value(&p.lin_side_slope);
    port.set_lin_side_offset_value(&p.lin_side_offset);
    port.set_direction(p.dir);
    let mut calls = affine_calls(&p);
    if let Some(slope) = p.linear_slope {
        port.set_linear_slope_value(&slope).unwrap();
        calls.push(json!(["setLinearSlopeValue", f64s(&slope)]));
    }
    Case::new(
        format!("camera, {label}"),
        json!({"class": "LogCameraTransform",
            "args": {"linSideBreak": f64s(&p.lin_side_break)}, "calls": calls}),
        port,
    )
}

/// Every class with `p`.
fn each_class(label: &str, p: Params) -> Vec<Case> {
    vec![
        log_case(label, p),
        affine_case(label, p),
        camera_case(label, p),
    ]
}

/// The cases: the defaults, upstream's values, the special doubles in every parameter, values
/// the data's validation refuses, and the linear slope set, in both directions.
fn cases() -> Vec<Case> {
    let mut out = vec![
        Case::new(
            "log, default",
            json!({"class": "LogTransform"}),
            LogTransform::new(),
        ),
        Case::new(
            "affine, default",
            json!({"class": "LogAffineTransform"}),
            LogAffineTransform::new(),
        ),
        Case::new(
            "camera, default",
            json!({"class": "LogCameraTransform", "args": {"linSideBreak": [0.1, 0.1, 0.1]}}),
            LogCameraTransform::new(&[0.1; 3]),
        ),
    ];
    // tests/cpu/transforms/LogAffineTransform_tests.cpp and LogCameraTransform_tests.cpp @
    // v2.5.2.
    let upstream = Params {
        base: 3.0,
        log_side_slope: [1.4, 1.5, 1.6],
        log_side_offset: [0.4, 0.5, 0.6],
        lin_side_slope: [1.1, 1.2, 1.3],
        lin_side_offset: [0.1, 0.2, 0.3],
        lin_side_break: [0.01, 0.02, 0.03],
        linear_slope: Some([1.0, 1.1, 1.2]),
        dir: Forward,
    };
    for dir in [Forward, Inverse] {
        out.extend(each_class(
            &format!("upstream's, {dir:?}"),
            Params { dir, ..upstream },
        ));
        out.push(camera_case(
            &format!("upstream's without a linear slope, {dir:?}"),
            Params {
                linear_slope: None,
                dir,
                ..upstream
            },
        ));
        for base in [10.0, 1.0, 0.0, -2.0, 1e-300, 0.5] {
            out.extend(each_class(
                &format!("base {base}, {dir:?}"),
                Params {
                    base,
                    dir,
                    ..Params::default()
                },
            ));
        }
    }
    let specials = special_doubles();
    for (k, &v) in specials.iter().enumerate() {
        let w = specials[(k + 5) % specials.len()];
        let dir = if k % 2 == 0 { Forward } else { Inverse };
        let base = Params { dir, ..upstream };
        let label = |name: &str| format!("special {k} ({v:e}) as {name}, {dir:?}");
        out.extend(each_class(&label("base"), Params { base: v, ..base }));
        out.extend(each_class(
            &label("log side slope"),
            Params {
                log_side_slope: [v, 1.0, w],
                ..base
            },
        ));
        out.extend(each_class(
            &label("log side offset"),
            Params {
                log_side_offset: [w, v, 0.5],
                ..base
            },
        ));
        out.extend(each_class(
            &label("lin side slope"),
            Params {
                lin_side_slope: [1.0, w, v],
                ..base
            },
        ));
        out.extend(each_class(
            &label("lin side offset"),
            Params {
                lin_side_offset: [v, v, w],
                ..base
            },
        ));
        out.push(camera_case(
            &label("lin side break"),
            Params {
                lin_side_break: [v, 0.5, w],
                ..base
            },
        ));
        out.push(camera_case(
            &label("linear slope"),
            Params {
                linear_slope: Some([w, v, 1.0]),
                ..base
            },
        ));
    }
    out
}

/// Groups that show a MatrixTransform's precision on the logs after it (I-73).
fn group_cases() -> Vec<Case> {
    let digits = Params {
        base: 1.0 / 3.0 + 2.0,
        log_side_slope: [0.123456789, 1.0, 2.0 / 3.0],
        log_side_offset: [1.0 / 7.0, 0.5, 0.0],
        lin_side_slope: [1.1, 1.2345678, 1.3],
        lin_side_offset: [0.1, 0.2, 1.0 / 3.0],
        lin_side_break: [0.01, 1.0 / 9.0, 0.03],
        linear_slope: Some([1.0 / 3.0, 1.1, 1.2]),
        dir: Forward,
    };
    let logs = each_class("many digits", digits);
    let matrix = Case::new(
        "a matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    let mut around = logs.clone();
    around.push(matrix.clone());
    around.extend(logs.clone());
    let mut nested = vec![group("a matrix", Inverse, std::slice::from_ref(&matrix))];
    nested.extend(logs);
    vec![
        group("logs around a matrix", Forward, &around),
        group("logs after a nested matrix", Inverse, &nested),
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
        pairs.extend([(i, i), (i, (i + 1) % n), (i, (i + 3) % n), (i, i + n)]);
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

/// The binding's camera log reports no linear slope after `unsetLinearSlopeValue`, as the
/// port's does: its text and its processor.
#[test]
fn an_unset_linear_slope_matches_the_wheel() {
    let mut port = LogCameraTransform::new(&[0.2; 3]);
    port.set_linear_slope_value(&[1.5, 1.0, 0.5]).unwrap();
    port.unset_linear_slope_value();
    let cases = vec![Case::new(
        "camera, slope unset",
        json!({"class": "LogCameraTransform", "args": {"linSideBreak": [0.2, 0.2, 0.2]},
            "calls": [["setLinearSlopeValue", [1.5, 1.0, 0.5]], ["unsetLinearSlopeValue"]]}),
        Transform::from(port),
    )];
    check_text(&cases, &[(0, 0)]);
    check_processors(&cases);
}
