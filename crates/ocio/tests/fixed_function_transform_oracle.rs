// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `FixedFunctionTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, the style (`FixedFunctionStyleToString`)
//!   and, for a transform with parameters, each with 6 significant digits, or 16 after a
//!   MatrixTransform in the same group (I-73); its validation, every message of the data's
//!   with the transform's prefix; `equals()` between transforms that differ in each part, NaN
//!   parameters included;
//! - the errors of its constructor (its data's validation without the prefix, and the two
//!   styles upstream doesn't implement) and of `setStyle`, by their messages;
//! - the raw config's processor of each valid transform in both directions:
//!   `BuildFixedFunctionOp`, the op's cache ID (in the processor's), `isInverse` (a transform
//!   and its inverse in a group, which the optimizer removes at the levels that pair inverses),
//!   and `CreateFixedFunctionTransform` through `createGroupTransform()`, every getter.
//!
//! The transforms are built as the binding builds them: the constructor
//! `FixedFunctionTransform(style, params, direction)`, which validates
//! (src/bindings/python/transforms/PyFixedFunctionTransform.cpp:26-42 @ v2.5.2), or, for
//! parameters it would refuse, the constructor of a style without parameters followed by the
//! setters, which don't validate.

mod common;

use common::transforms::{
    Case, check_optimized_processors, check_processors_dirs, check_text, direction_spec,
    fixed_function_style_name, group, setter_errors, special_doubles,
};
use ocio::{FixedFunctionStyle, FixedFunctionTransform, MatrixTransform, TransformDirection};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use FixedFunctionStyle::*;
use TransformDirection::{Forward, Inverse};

/// The style, as a spec value.
fn style_spec(style: FixedFunctionStyle) -> Value {
    json!({"enum": fixed_function_style_name(style)})
}

/// Parameters, as a spec value: each by its bits.
fn params_spec(params: &[f64]) -> Value {
    Value::Array(params.iter().map(|&v| f64_spec(v)).collect())
}

/// The binding's constructor, `FixedFunctionTransform(style, params, direction)`: `Create`,
/// then `setDirection` and `validate`.
fn constructed(style: FixedFunctionStyle, params: &[f64], dir: TransformDirection) -> Case {
    let mut port = FixedFunctionTransform::new(style, params).expect("a valid transform");
    port.set_direction(dir);
    port.validate().expect("a valid transform");
    Case::new(
        format!("{style:?} {params:?} {dir:?}"),
        json!({"class": "FixedFunctionTransform",
            "args": {"style": style_spec(style), "params": params_spec(params),
                "direction": direction_spec(dir)}}),
        port,
    )
}

/// The constructor of `ACES_GLOW_03`, then `setDirection`, `setStyle` and `setParams`, none of
/// which validates.
fn set_up(style: FixedFunctionStyle, params: &[f64], dir: TransformDirection) -> Case {
    let mut port = FixedFunctionTransform::new(AcesGlow03, &[]).expect("a valid transform");
    port.set_direction(dir);
    port.set_style(style).expect("an implemented style");
    port.set_params(params);
    Case::new(
        format!("set up {style:?} {params:?} {dir:?}"),
        json!({"class": "FixedFunctionTransform",
            "args": {"style": style_spec(AcesGlow03)},
            "calls": [["setDirection", direction_spec(dir)], ["setStyle", style_spec(style)],
                ["setParams", params_spec(params)]]}),
        port,
    )
}

/// Valid parameters of each style that takes some: upstream's test values
/// (tests/cpu/ops/fixedfunction/FixedFunctionOpData_tests.cpp:71, 169, 219-220, 270-271,
/// FixedFunctionOpCPU_tests.cpp:564-569, 773, 866 @ v2.5.2).
fn valid_params(style: FixedFunctionStyle) -> Vec<f64> {
    match style {
        AcesGamutComp13 => vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
        Rec2100Surround => vec![2.0],
        LinToDoubleLog => vec![
            10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
        ],
        LinToGammaLog => vec![
            0.0,
            0.25,
            0.5,
            1.0,
            0.0,
            2.718,
            0.17883277,
            0.807825590164,
            1.0,
            -0.07116723,
        ],
        AcesOutputTransform20 | AcesGamutCompress20 => {
            vec![
                100.0, 0.6400, 0.3300, 0.3000, 0.6000, 0.1500, 0.0600, 0.3127, 0.3290,
            ]
        }
        AcesRgbToJmh20 => vec![
            0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
        ],
        AcesTonescaleCompress20 => vec![1000.0],
        _ => Vec::new(),
    }
}

/// Every implemented style.
const STYLES: [FixedFunctionStyle; 21] = [
    AcesRedMod03,
    AcesRedMod10,
    AcesGlow03,
    AcesGlow10,
    AcesDarkToDim10,
    Rec2100Surround,
    RgbToHsv,
    XyzToXyy,
    XyzToUvy,
    XyzToLuv,
    AcesGamutComp13,
    LinToPq,
    LinToGammaLog,
    LinToDoubleLog,
    AcesOutputTransform20,
    AcesRgbToJmh20,
    AcesTonescaleCompress20,
    AcesGamutCompress20,
    RgbToHsyLin,
    RgbToHsyLog,
    RgbToHsyVid,
];

/// The styles whose CPU renderers are ported (2.3b, 2.3c1, 2.3c2, 2.3d1, 2.4e1, 2.4e2): the
/// processors' cases.
const RENDERED: [FixedFunctionStyle; 20] = [
    AcesRedMod03,
    AcesRedMod10,
    AcesGlow03,
    AcesGlow10,
    AcesDarkToDim10,
    AcesGamutComp13,
    AcesOutputTransform20,
    AcesRgbToJmh20,
    AcesTonescaleCompress20,
    AcesGamutCompress20,
    Rec2100Surround,
    RgbToHsv,
    XyzToXyy,
    XyzToUvy,
    XyzToLuv,
    RgbToHsyLin,
    RgbToHsyLog,
    RgbToHsyVid,
    LinToGammaLog,
    LinToDoubleLog,
];

/// Transforms of every style in both directions, valid and set up with other parameters, and
/// the special doubles as parameters; groups whose MatrixTransform changes the stream's
/// precision for the transforms after it.
fn text_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for style in STYLES {
        for dir in [Forward, Inverse] {
            cases.push(constructed(style, &valid_params(style), dir));
            cases.push(set_up(style, &[], dir));
            cases.push(set_up(style, &[1.5, -2.25, 0.1], dir));
        }
    }
    for v in special_doubles() {
        let mut params = valid_params(AcesGamutComp13);
        params[6] = v;
        cases.push(set_up(AcesGamutComp13, &params, Inverse));
        cases.push(set_up(Rec2100Surround, &[v], Forward));
        cases.push(set_up(AcesOutputTransform20, &[v; 9], Forward));
    }
    let matrix = Case::new(
        "matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    let ff = set_up(LinToGammaLog, &valid_params(LinToGammaLog), Forward);
    cases.push(group(
        "ff after a matrix",
        Forward,
        &[matrix.clone(), ff.clone()],
    ));
    cases.push(group("ff before a matrix", Inverse, &[ff, matrix]));
    cases
}

#[test]
fn text_validation_and_equality_match_the_wheel() {
    let cases = text_cases();
    // Every pair of the first transforms (both directions of each style), and each transform
    // with itself and its neighbours.
    let mut pairs = Vec::new();
    let head = 6 * STYLES.len();
    for i in (0..head).step_by(3) {
        for j in (0..head).step_by(3) {
            pairs.push((i, j));
        }
    }
    for i in 0..cases.len() {
        pairs.push((i, i));
        if i + 1 < cases.len() {
            pairs.push((i, i + 1));
        }
    }
    // Only the FixedFunctionTransforms have `equals`.
    pairs.retain(|&(i, j)| {
        cases[i].spec["class"] == "FixedFunctionTransform"
            && cases[j].spec["class"] == "FixedFunctionTransform"
    });
    check_text(&cases, &pairs);
}

#[test]
fn constructor_and_setter_errors_match_the_wheel() {
    let mut checks: Vec<(String, Value, ocio::Result<()>)> = Vec::new();
    for style in [AcesGamutMap02, AcesGamutMap07] {
        checks.push((
            format!("constructor {style:?}"),
            json!({"class": "FixedFunctionTransform", "args": {"style": style_spec(style)}}),
            FixedFunctionTransform::new(style, &[]).map(|_| ()),
        ));
        checks.push((
            format!("setStyle {style:?}"),
            json!({"class": "FixedFunctionTransform", "args": {"style": style_spec(AcesGlow03)},
                "calls": [["setStyle", style_spec(style)]]}),
            FixedFunctionTransform::new(AcesGlow03, &[])
                .unwrap()
                .set_style(style),
        ));
    }
    // The binding's constructor validates the forward style's data: without the transform's
    // prefix, before it sets the direction.
    for style in STYLES {
        for params in [vec![], vec![1.5], valid_params(style), vec![0.5; 20]] {
            for dir in [Forward, Inverse] {
                let port = FixedFunctionTransform::new(style, &params).and_then(|mut t| {
                    t.set_direction(dir);
                    t.validate()
                });
                if port.is_err() {
                    checks.push((
                        format!("constructor {style:?} {params:?} {dir:?}"),
                        json!({"class": "FixedFunctionTransform",
                            "args": {"style": style_spec(style), "params": params_spec(&params),
                                "direction": direction_spec(dir)}}),
                        port,
                    ));
                }
            }
        }
    }
    let checks: Vec<(&str, Value, ocio::Result<()>)> = checks
        .iter()
        .map(|(label, spec, port)| (label.as_str(), spec.clone(), port.clone()))
        .collect();
    setter_errors(&checks);
}

/// The processors' cases: the rendered styles in both directions, with the processors in both
/// directions; each with its inverse in a group, which the optimizer removes.
fn processor_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for style in RENDERED {
        for dir in [Forward, Inverse] {
            cases.push(constructed(style, &valid_params(style), dir));
        }
        let fwd = constructed(style, &valid_params(style), Forward);
        let inv = constructed(style, &valid_params(style), Inverse);
        cases.push(group(
            &format!("{style:?} and its inverse"),
            Forward,
            &[fwd, inv],
        ));
    }
    // Parameters with more digits than the cache ID's 7 significant ones, and values of
    // every magnitude.
    for params in [
        [
            1.123456789,
            1.987654321,
            1.0011234567,
            0.1234567891,
            0.5,
            0.75,
            1.23456789,
        ],
        [
            65504.0,
            12345.678901,
            1.5e-3 + 1.001,
            1e-9,
            0.999,
            0.0,
            9876.54321,
        ],
    ] {
        cases.push(constructed(AcesGamutComp13, &params, Inverse));
    }
    // Two forward surrounds are inverses when `isInverse` finds `p0 == 1. / p1` in double
    // (FixedFunctionOpData.cpp:842-854 @ v2.5.2), which the optimizer then removes. Pairs
    // where that test, `p0 * p1 == 1` and `1 / p0 == p1` don't all agree, in both orders.
    for gammas in [
        [23.804083062918217, 0.04200959967064603],
        [6.562230635388913, 0.1523872072717442],
    ] {
        for [a, b] in [gammas, [gammas[1], gammas[0]]] {
            let first = constructed(Rec2100Surround, &[a], Forward);
            let second = constructed(Rec2100Surround, &[b], Forward);
            cases.push(group(
                &format!("surrounds {a} and {b}"),
                Forward,
                &[first, second],
            ));
        }
    }
    cases
}

#[test]
fn processors_match_the_wheel() {
    check_processors_dirs(&processor_cases(), &[Forward, Inverse]);
}

#[test]
fn optimized_processors_match_the_wheel() {
    let depths = [
        (Depth::Uint8, Depth::Uint16),
        (Depth::F16, Depth::F32),
        (Depth::F32, Depth::Uint10),
    ];
    check_optimized_processors(&processor_cases(), &depths);
}
