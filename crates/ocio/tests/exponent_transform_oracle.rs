// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `ExponentTransform` and `ExponentWithLinearTransform` against the wheel
//! (`common::transforms`):
//! - their text (`repr()`, `str()`), the values with 6 significant digits, or 16 after a
//!   MatrixTransform in the same group (I-73), and the negative style; their validation; the
//!   styles each refuses while the spec is built (`setNegativeStyle`), with upstream's message;
//!   `equals()` between transforms that differ in each part;
//! - the raw config's processor of each, in both directions: `BuildExponentOp` and
//!   `BuildExponentWithLinearOp` make Gamma ops, and `CreateGammaTransform` makes either
//!   class of them, through `createGroupTransform()`;
//! - a version 1 config's processor of each ExponentTransform: `BuildExponentOp` makes an
//!   Exponent op of the values whatever the style, and `CreateExponentTransform` makes the
//!   transform of it.

mod common;

use std::sync::Arc;

use common::transforms::{
    Case, check_processors, check_processors_in, check_text, direction_spec, group,
    negative_style_spec, special_doubles,
};
use ocio::{
    Config, ExponentTransform, ExponentWithLinearTransform, MatrixTransform, NegativeStyle,
    Transform, TransformDirection,
};
use ocio_testkit::transform_text::{Built, TransformTextRequest, f64_spec};
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// A version 1 config with one color space.
const V1_CONFIG: &str = "ocio_profile_version: 1\n\
roles: {default: raw}\n\
colorspaces:\n  \
- !<ColorSpace> {name: raw}\n";

/// Doubles as spec values.
fn f64s(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|&v| f64_spec(v)).collect())
}

/// A setter call of a transform, in the spec and on the port.
enum Call {
    Value([f64; 4]),
    Offset([f64; 4]),
    Direction(TransformDirection),
    Style(NegativeStyle),
}

impl Call {
    fn spec(&self, value_setter: &str) -> Value {
        match self {
            Call::Value(v) => json!([value_setter, f64s(v)]),
            Call::Offset(v) => json!(["setOffset", f64s(v)]),
            Call::Direction(dir) => json!(["setDirection", direction_spec(*dir)]),
            Call::Style(style) => json!(["setNegativeStyle", negative_style_spec(*style)]),
        }
    }
}

/// An ExponentTransform after `calls`, or the error of the first call that raises.
fn exponent(label: &str, calls: &[Call]) -> (String, Value, ocio::Result<Transform>) {
    let mut port = ExponentTransform::new();
    let mut result = Ok(());
    for call in calls {
        if result.is_err() {
            break;
        }
        match call {
            Call::Value(v) => port.set_value(v),
            Call::Offset(_) => unreachable!("an exponent has no offset"),
            Call::Direction(dir) => port.set_direction(*dir),
            Call::Style(style) => result = port.set_negative_style(*style),
        }
    }
    let spec = json!({"class": "ExponentTransform",
        "calls": calls.iter().map(|c| c.spec("setValue")).collect::<Vec<_>>()});
    (label.to_string(), spec, result.map(|()| port.into()))
}

/// An ExponentWithLinearTransform after `calls`, or the error of the first call that raises.
fn with_linear(label: &str, calls: &[Call]) -> (String, Value, ocio::Result<Transform>) {
    let mut port = ExponentWithLinearTransform::new();
    let mut result = Ok(());
    for call in calls {
        if result.is_err() {
            break;
        }
        match call {
            Call::Value(v) => port.set_gamma(v),
            Call::Offset(v) => port.set_offset(v),
            Call::Direction(dir) => port.set_direction(*dir),
            Call::Style(style) => result = port.set_negative_style(*style),
        }
    }
    let spec = json!({"class": "ExponentWithLinearTransform",
        "calls": calls.iter().map(|c| c.spec("setGamma")).collect::<Vec<_>>()});
    (label.to_string(), spec, result.map(|()| port.into()))
}

const STYLES: [NegativeStyle; 4] = [
    NegativeStyle::Clamp,
    NegativeStyle::Mirror,
    NegativeStyle::PassThru,
    NegativeStyle::Linear,
];

/// Every transform to check: the defaults, upstream's values, every style before and after the
/// direction, the special doubles as values and offsets.
fn all() -> Vec<(String, Value, ocio::Result<Transform>)> {
    let mut out = vec![
        exponent("the default exponent", &[]),
        with_linear("the default exponent with linear", &[]),
    ];
    // tests/cpu/transforms/ExponentTransform_tests.cpp and ExponentWithLinearTransform_tests.cpp
    // @ v2.5.2.
    out.push(exponent(
        "upstream's",
        &[Call::Value([1., 2.1234567, 1., 1.])],
    ));
    out.push(with_linear(
        "upstream's",
        &[
            Call::Value([1., 2.1234567, 1., 1.]),
            Call::Offset([0., 0.1234567, 0., 0.]),
        ],
    ));
    for style in STYLES {
        for dir in [Forward, Inverse] {
            let value = Call::Value([2.2, 2.4, 1.8, 1.0]);
            out.push(exponent(
                &format!("{style:?} then {dir:?}"),
                &[Call::Style(style), Call::Direction(dir), value],
            ));
            out.push(exponent(
                &format!("{dir:?} then {style:?}"),
                &[Call::Direction(dir), Call::Style(style)],
            ));
            out.push(with_linear(
                &format!("{style:?} then {dir:?}"),
                &[
                    Call::Style(style),
                    Call::Direction(dir),
                    Call::Value([2.4, 2.2, 2.0, 1.0]),
                    Call::Offset([0.055, 0.099, 0.1, 0.0]),
                ],
            ));
            out.push(with_linear(
                &format!("{dir:?} then {style:?}"),
                &[Call::Direction(dir), Call::Style(style)],
            ));
        }
    }
    let specials = special_doubles();
    for start in (0..specials.len()).step_by(4) {
        let v = [0, 1, 2, 3].map(|i| specials[(start + i) % specials.len()]);
        let w = [0, 1, 2, 3].map(|i| specials[(start + 7 + i) % specials.len()]);
        for dir in [Forward, Inverse] {
            out.push(exponent(
                &format!("specials from {start}, {dir:?}"),
                &[Call::Value(v), Call::Direction(dir)],
            ));
            out.push(exponent(
                &format!("specials from {start}, mirror, {dir:?}"),
                &[
                    Call::Value(v),
                    Call::Style(NegativeStyle::Mirror),
                    Call::Direction(dir),
                ],
            ));
            out.push(with_linear(
                &format!("specials from {start}, {dir:?}"),
                &[Call::Value(v), Call::Offset(w), Call::Direction(dir)],
            ));
            out.push(with_linear(
                &format!("specials from {start} as offsets, {dir:?}"),
                &[
                    Call::Value([2.4, 2.2, 2.0, 1.0]),
                    Call::Offset(v),
                    Call::Direction(dir),
                ],
            ));
        }
    }
    // Values at the edges of the data's validation, one channel at a time.
    for (k, value) in [0.01, 0.0099999, 100.0, 100.0001, 0.0, -1.0]
        .into_iter()
        .enumerate()
    {
        let mut v = [1.0; 4];
        v[k % 4] = value;
        out.push(exponent(&format!("value {value}"), &[Call::Value(v)]));
        out.push(with_linear(&format!("gamma {value}"), &[Call::Value(v)]));
        out.push(with_linear(
            &format!("offset {value}"),
            &[Call::Value([2.0; 4]), Call::Offset(v)],
        ));
    }
    out
}

/// The cases the wheel builds, and the specs it refuses with the port's errors.
fn split() -> (Vec<Case>, Vec<(String, Value, String)>) {
    let mut built = Vec::new();
    let mut refused = Vec::new();
    for (label, spec, port) in all() {
        match port {
            Ok(port) => built.push(Case::new(label, spec, port)),
            Err(e) => refused.push((label, spec, e.message().to_string())),
        }
    }
    (built, refused)
}

/// Groups that show a MatrixTransform's precision on the exponents after it (I-73).
fn group_cases(cases: &[Case]) -> Vec<Case> {
    let matrix = Case::new(
        "a matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    let digits = |t: (String, Value, ocio::Result<Transform>)| {
        Case::new(t.0, t.1, t.2.expect("a valid transform"))
    };
    let exp = digits(exponent(
        "many digits",
        &[Call::Value([1.0 / 3.0, 2.123456789, 0.1, 1.0])],
    ));
    let lin = digits(with_linear(
        "many digits",
        &[
            Call::Value([2.4000000001, 1.0 / 7.0, 2.0, 1.0]),
            Call::Offset([0.0550000001, 0.0, 1.0 / 3.0, 0.0]),
        ],
    ));
    vec![
        group(
            "exponents around a matrix",
            Forward,
            &[
                exp.clone(),
                lin.clone(),
                matrix.clone(),
                exp.clone(),
                lin.clone(),
            ],
        ),
        group(
            "exponents after a nested matrix",
            Inverse,
            &[
                group("a matrix", Forward, std::slice::from_ref(&matrix)),
                exp,
                lin,
            ],
        ),
        group("the first two", Forward, &cases[..2]),
    ]
}

#[test]
fn text_validation_and_equality_match_the_wheel() {
    let (mut cases, _) = split();
    let n = cases.len();
    let copies = cases.clone();
    cases.extend(copies);
    let mut pairs = Vec::new();
    for i in 0..n {
        pairs.extend([(i, i), (i, (i + 1) % n), (i, i + n)]);
    }
    let groups = group_cases(&cases);
    let g = cases.len();
    cases.extend(groups);
    pairs.extend([(g, g), (g, 0), (0, 1), (1, 0)]);
    check_text(&cases, &pairs);
}

#[test]
fn refused_styles_raise_the_wheels_messages() {
    let (_, refused) = split();
    assert!(refused.len() >= 6, "{}", refused.len());
    let reply = TransformTextRequest {
        transforms: refused.iter().map(|(_, spec, _)| spec.clone()).collect(),
        pairs: Vec::new(),
    }
    .run();
    let mut failures = Vec::new();
    for ((label, _, port), built) in refused.iter().zip(&reply.transforms) {
        match built {
            Built::Raised(raised) if raised.message == *port => {}
            other => failures.push(format!("{label}: wheel {other:?}, port {port:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn processors_match_the_wheel() {
    let (mut cases, _) = split();
    let groups = group_cases(&cases);
    cases.extend(groups);
    check_processors(&cases);
}

#[test]
fn version_1_processors_match_the_wheel() {
    let (cases, _) = split();
    let cases: Vec<Case> = cases
        .into_iter()
        .filter(|case| matches!(case.port, Transform::Exponent(_)))
        .collect();
    let mut config = Config::create_raw().unwrap();
    Arc::get_mut(&mut config)
        .unwrap()
        .set_major_version(1)
        .unwrap();
    check_processors_in(&cases, Some(&json!({"yaml": V1_CONFIG})), &config);
}
