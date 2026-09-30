// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `transform_text` command (`oracle/ocio_oracle/transform_text.py`, chunk O1.4)
//! against the wheel itself: every transform class prints itself, and its text follows its
//! values; `validate()` reports what the transform holds, and a constructor that validates is
//! reported apart; upstream's tests' validation messages come through; `equals()` follows the
//! values for the classes the binding gives it, and is absent for the others; the command's
//! refusals; and replies that don't depend on the run.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::transform_text::{Built, TransformTextReply, TransformTextRequest};
use serde_json::{Value, json};

/// Runs `request`; it must succeed.
fn run(request: &TransformTextRequest) -> TransformTextReply {
    let calls = [request.call()];
    let response = Oracle::get()
        .batch(&calls, true)
        .remove(0)
        .unwrap_or_else(|e| panic!("{e}"));
    TransformTextReply::from_response(response)
}

/// One transform of every class the binding has, each with values of its own, and each a
/// second time with a value its text shows changed (or its direction inverted).
fn every_class() -> Vec<(Value, Value)> {
    let inverse = |mut spec: Value| {
        let calls = spec["calls"].as_array().cloned().unwrap_or_default();
        let mut calls = calls;
        calls.push(json!(["setDirection", {"enum": "TRANSFORM_DIR_INVERSE"}]));
        spec["calls"] = json!(calls);
        spec
    };
    let pair = |a: Value, b: Value| (a, b);
    let lut3d = |v: f64| {
        json!({"class": "Lut3DTransform", "args": {"gridSize": 2},
            "calls": [["setValue", 1, 0, 1, v, 0.2, 0.1]]})
    };
    vec![
        pair(
            json!({"class": "AllocationTransform"}),
            inverse(json!({"class": "AllocationTransform"})),
        ),
        pair(
            json!({"class": "BuiltinTransform", "args": {"style": "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD"}}),
            json!({"class": "BuiltinTransform", "args": {"style": "UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD"}}),
        ),
        pair(
            json!({"class": "CDLTransform", "args": {"slope": [1.1, 0.9, 1.3], "sat": 0.77}}),
            json!({"class": "CDLTransform", "args": {"slope": [1.1, 0.9, 1.3], "sat": 0.78}}),
        ),
        pair(
            json!({"class": "ColorSpaceTransform", "args": {"src": "a", "dst": "b"}}),
            json!({"class": "ColorSpaceTransform", "args": {"src": "a", "dst": "c"}}),
        ),
        pair(
            json!({"class": "DisplayViewTransform", "args": {"src": "a", "display": "d", "view": "v"}}),
            json!({"class": "DisplayViewTransform", "args": {"src": "a", "display": "d", "view": "w"}}),
        ),
        pair(
            json!({"class": "ExponentTransform", "args": {"value": [2.2, 2.4, 1.8, 1.0]}}),
            json!({"class": "ExponentTransform", "args": {"value": [2.2, 2.4, 1.8, 1.5]}}),
        ),
        pair(
            json!({"class": "ExponentWithLinearTransform",
                "args": {"gamma": [2.4, 2.2, 2.0, 1.0], "offset": [0.055, 0.099, 0.1, 0.0]}}),
            json!({"class": "ExponentWithLinearTransform",
                "args": {"gamma": [2.4, 2.2, 2.0, 1.0], "offset": [0.055, 0.099, 0.2, 0.0]}}),
        ),
        pair(
            json!({"class": "ExposureContrastTransform", "args": {"exposure": 0.5}}),
            json!({"class": "ExposureContrastTransform", "args": {"exposure": 0.25}}),
        ),
        pair(
            json!({"class": "FileTransform", "args": {"src": "a.cube"}}),
            json!({"class": "FileTransform", "args": {"src": "b.cube"}}),
        ),
        pair(
            json!({"class": "FixedFunctionTransform",
                "args": {"style": {"enum": "FIXED_FUNCTION_ACES_GLOW_03"}}}),
            json!({"class": "FixedFunctionTransform",
                "args": {"style": {"enum": "FIXED_FUNCTION_ACES_GLOW_10"}}}),
        ),
        pair(
            json!({"class": "GradingHueCurveTransform", "args": {"style": {"enum": "GRADING_LOG"}}}),
            json!({"class": "GradingHueCurveTransform", "args": {"style": {"enum": "GRADING_LIN"}}}),
        ),
        pair(
            json!({"class": "GradingPrimaryTransform", "args": {"style": {"enum": "GRADING_LOG"}}}),
            json!({"class": "GradingPrimaryTransform", "args": {"style": {"enum": "GRADING_VIDEO"}}}),
        ),
        pair(
            json!({"class": "GradingRGBCurveTransform", "args": {"style": {"enum": "GRADING_LOG"}}}),
            json!({"class": "GradingRGBCurveTransform", "args": {"style": {"enum": "GRADING_LIN"}}}),
        ),
        pair(
            json!({"class": "GradingToneTransform", "args": {"style": {"enum": "GRADING_LOG"}}}),
            json!({"class": "GradingToneTransform", "args": {"style": {"enum": "GRADING_VIDEO"}}}),
        ),
        pair(
            json!({"class": "GroupTransform", "children": [{"class": "LogTransform"}]}),
            json!({"class": "GroupTransform", "children": [
                {"class": "LogTransform", "args": {"base": 10.0}}]}),
        ),
        pair(
            json!({"class": "LogAffineTransform", "args": {"logSideSlope": [0.3, 0.29, 0.31]}}),
            json!({"class": "LogAffineTransform", "args": {"logSideSlope": [0.3, 0.29, 0.32]}}),
        ),
        pair(
            json!({"class": "LogCameraTransform", "args": {"linSideBreak": [0.01, 0.02, 0.03]}}),
            json!({"class": "LogCameraTransform", "args": {"linSideBreak": [0.01, 0.02, 0.04]}}),
        ),
        pair(
            json!({"class": "LogTransform", "args": {"base": 3.0}}),
            json!({"class": "LogTransform", "args": {"base": 3.5}}),
        ),
        pair(
            json!({"class": "LookTransform", "args": {"src": "a", "dst": "b", "looks": "l"}}),
            json!({"class": "LookTransform", "args": {"src": "a", "dst": "b", "looks": "m"}}),
        ),
        // A LUT prints its range, not its entries: the second one's reaches further.
        pair(
            json!({"class": "Lut1DTransform", "args": {"length": 3},
                "calls": [["setValue", 1, 0.3, 0.2, 0.1]]}),
            json!({"class": "Lut1DTransform", "args": {"length": 3},
                "calls": [["setValue", 1, 1.5, 0.2, 0.1]]}),
        ),
        pair(lut3d(0.3), lut3d(1.5)),
        pair(
            json!({"class": "MatrixTransform", "args": {"offset": [0.1, 0.2, 0.3, 0.4]}}),
            json!({"class": "MatrixTransform", "args": {"offset": [0.1, 0.2, 0.3, 0.5]}}),
        ),
        pair(
            json!({"class": "RangeTransform", "args": {"minInValue": -0.5, "maxInValue": 2.0,
                "minOutValue": 0.1, "maxOutValue": 3.0}}),
            json!({"class": "RangeTransform", "args": {"minInValue": -0.5, "maxInValue": 2.0,
                "minOutValue": 0.1, "maxOutValue": 3.5}}),
        ),
    ]
}

/// The classes the binding gives `equals()`, each taking a transform of its own class.
const WITH_EQUALS: [&str; 12] = [
    "CDLTransform",
    "ExponentTransform",
    "ExponentWithLinearTransform",
    "ExposureContrastTransform",
    "FixedFunctionTransform",
    "LogAffineTransform",
    "LogCameraTransform",
    "LogTransform",
    "Lut1DTransform",
    "Lut3DTransform",
    "MatrixTransform",
    "RangeTransform",
];

/// Every class prints itself, `str()` as `repr()`, and its text follows its values: the two
/// transforms of a class, which differ in a value the text shows, print differently, and the
/// same spec built twice prints the same. `equals()` agrees: true for the same spec, false for the other
/// one, for each class the binding gives it; absent for the others, and across classes.
#[test]
fn every_class_prints_itself_and_compares_by_value() {
    let classes = every_class();
    let mut transforms = Vec::new();
    for (a, b) in &classes {
        transforms.extend([a.clone(), a.clone(), b.clone()]);
    }
    let mut pairs = Vec::new();
    for k in 0..classes.len() {
        let (a, again, b) = (3 * k, 3 * k + 1, 3 * k + 2);
        pairs.extend([(a, a), (a, again), (a, b), (b, a)]);
        // Across classes.
        pairs.push((a, (3 * k + 3) % (3 * classes.len())));
    }
    let reply = run(&TransformTextRequest { transforms, pairs });
    assert_eq!(reply.transforms.len(), 3 * classes.len());
    for (k, (a, _)) in classes.iter().enumerate() {
        let class = a["class"].as_str().unwrap();
        let [first, again, other] = [0, 1, 2].map(|i| reply.transforms[3 * k + i].text());
        assert_eq!(first.class, class);
        assert_eq!(first.str, first.repr, "{class}");
        assert!(
            first.repr.starts_with('<') && first.repr.ends_with('>'),
            "{}",
            first.repr
        );
        assert_eq!(first.repr, again.repr, "{class}: the same spec twice");
        assert_ne!(first.repr, other.repr, "{class}: a value changed");
        let equality = &reply.pairs[5 * k..5 * k + 5];
        if WITH_EQUALS.contains(&class) {
            assert_eq!(
                equality,
                [Some(true), Some(true), Some(false), Some(false), None],
                "{class}"
            );
        } else {
            assert_eq!(equality, [None; 5], "{class}");
        }
    }
    // The 23 classes of the binding, each once.
    let mut names: Vec<&str> = classes
        .iter()
        .map(|(a, _)| a["class"].as_str().unwrap())
        .collect();
    names.dedup();
    assert_eq!(names.len(), 23);
}

/// `validate()` reports what a transform holds: values a setter put in, which no constructor
/// checked, raise; the same transforms with valid values don't. A constructor that validates
/// is reported as the spec's exception, and its pairs have no equality.
#[test]
fn validation_follows_the_values() {
    let cases: Vec<(&str, Value, bool)> = vec![
        (
            "a negative exponent",
            json!({"class": "ExponentTransform", "calls": [["setValue", [-1.0, 1.0, 1.0, 1.0]]]}),
            false,
        ),
        (
            "a valid exponent",
            json!({"class": "ExponentTransform", "calls": [["setValue", [2.0, 1.0, 1.0, 1.0]]]}),
            true,
        ),
        (
            "a log side slope of 0",
            json!({"class": "LogAffineTransform", "calls": [["setLogSideSlopeValue", [0.0, 1.0, 1.0]]]}),
            false,
        ),
        (
            "a log side slope of 1",
            json!({"class": "LogAffineTransform", "calls": [["setLogSideSlopeValue", [1.0, 1.0, 1.0]]]}),
            true,
        ),
        (
            "a range without limits",
            json!({"class": "RangeTransform"}),
            false,
        ),
        (
            "a range with a limit",
            json!({"class": "RangeTransform", "calls": [["setMinInValue", 0.0], ["setMinOutValue", 0.0]]}),
            true,
        ),
        (
            "a color space transform without names",
            json!({"class": "ColorSpaceTransform"}),
            false,
        ),
        (
            "a color space transform with names",
            json!({"class": "ColorSpaceTransform", "args": {"src": "a", "dst": "b"}}),
            true,
        ),
        (
            "a file transform without a path",
            json!({"class": "FileTransform"}),
            false,
        ),
    ];
    let mut transforms: Vec<Value> = cases.iter().map(|(_, spec, _)| spec.clone()).collect();
    // A constructor that validates.
    transforms
        .push(json!({"class": "LogAffineTransform", "args": {"linSideSlope": [0.0, 1.0, 1.0]}}));
    let refused = transforms.len() - 1;
    let reply = run(&TransformTextRequest {
        transforms,
        pairs: vec![(refused, refused), (0, refused)],
    });
    for ((label, _, valid), built) in cases.iter().zip(&reply.transforms) {
        let text = built.text();
        match &text.validate {
            None => assert!(valid, "{label}: validates"),
            Some(raised) => {
                assert!(!valid, "{label}: {raised:?}");
                assert_eq!(raised.kind, "Exception", "{label}");
                assert!(!raised.message.is_empty(), "{label}");
            }
        }
    }
    match &reply.transforms[refused] {
        Built::Raised(raised) => {
            assert_eq!(raised.kind, "Exception");
            assert!(
                raised.message.contains("linear side slope cannot be 0"),
                "{raised:?}"
            );
        }
        other => panic!("the constructor validates: {other:?}"),
    }
    assert_eq!(reply.pairs, vec![None, None]);
}

/// Requests the command refuses: a key it doesn't know, and pairs that don't name two of the
/// transforms.
#[test]
fn unknown_keys_and_bad_pairs_are_refused() {
    let one = json!([{"class": "LogTransform"}]);
    let cases = [
        (
            json!({"transforms": one, "pair": [[0, 0]]}),
            "unknown keys ['pair']",
        ),
        (
            json!({"transforms": one, "pairs": [[0, 1]]}),
            "doesn't name two of the 1 transforms",
        ),
        (
            json!({"transforms": one, "pairs": [[-1, 0]]}),
            "doesn't name two of the 1 transforms",
        ),
        (
            json!({"transforms": one, "pairs": [[0]]}),
            "doesn't name two of the 1 transforms",
        ),
        // Python takes true for 1 and false for 0; the command doesn't.
        (
            json!({"transforms": [{"class": "LogTransform"}, {"class": "LogTransform"}],
                "pairs": [[true, false]]}),
            "[True, False] doesn't name two of the 2 transforms",
        ),
        (
            json!({"transforms": one, "pairs": [[0.0, 0]]}),
            "doesn't name two of the 1 transforms",
        ),
        (
            json!({"transforms": {"class": "LogTransform"}}),
            "transforms must be a list",
        ),
        (
            json!({"transforms": one, "pairs": null}),
            "pairs must be a list, not None",
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(args, _)| BatchCall {
            cmd: "transform_text",
            args: args.clone(),
            blobs: Vec::new(),
        })
        .collect();
    for ((args, fragment), result) in cases.iter().zip(Oracle::get().batch(&calls, false)) {
        let error = result.expect_err(&format!("{args}: refused"));
        assert!(error.contains(fragment), "{args}: {error}");
    }
}

/// Upstream's transform tests' validation messages, run on the wheel through the command: each
/// reported message holds upstream's expected text, for the fixed functions the whole message.
/// The transforms are built the way upstream's tests build them, through setters: the binding's
/// constructors validate before a direction applies.
/// - FixedFunctionTransform_tests.cpp:37-70 @ v2.5.2, from `validate()`;
/// - ExponentTransform_tests.cpp:145-146 and ExponentWithLinearTransform_tests.cpp:58-61
///   @ v2.5.2, from `setNegativeStyle`, which raises while the transform is built;
/// - Lut1DTransform_tests.cpp:88-89 @ v2.5.2, from `validate()`.
#[test]
fn upstream_validation_messages_come_through() {
    let set = |name: &str, value: Value| json!([name, value]);
    let e = |name: &str| json!({"enum": name});
    let fixed = |calls: Vec<Value>| {
        json!({"class": "FixedFunctionTransform",
            "args": {"style": e("FIXED_FUNCTION_ACES_RED_MOD_03")}, "calls": calls})
    };
    let inverse = set("setDirection", e("TRANSFORM_DIR_INVERSE"));
    let one = set("setParams", json!([1.0]));
    let cases: Vec<(Value, &str)> = vec![
        (
            fixed(vec![
                inverse.clone(),
                set("setStyle", e("FIXED_FUNCTION_ACES_GAMUT_COMP_13")),
            ]),
            "The style 'ACES_GamutComp13 (Inverse)' must have seven parameters but 0 found.",
        ),
        (
            fixed(vec![
                inverse.clone(),
                set("setStyle", e("FIXED_FUNCTION_REC2100_SURROUND")),
            ]),
            "The style 'REC2100_Surround (Inverse)' must have one parameter but 0 found.",
        ),
        (
            fixed(vec![
                inverse.clone(),
                one.clone(),
                set("setStyle", e("FIXED_FUNCTION_ACES_DARK_TO_DIM_10")),
            ]),
            "The style 'ACES_DarkToDim10 (Inverse)' must have zero parameters but 1 found.",
        ),
        (
            fixed(vec![
                inverse.clone(),
                one,
                set("setStyle", e("FIXED_FUNCTION_RGB_TO_HSV")),
            ]),
            "The style 'RGB_TO_HSV' must have zero parameters but 1 found.",
        ),
        (
            json!({"class": "ExponentTransform",
                "calls": [set("setNegativeStyle", e("NEGATIVE_LINEAR"))]}),
            "Linear negative extrapolation is not valid for basic exponent style",
        ),
        (
            json!({"class": "ExponentWithLinearTransform",
                "calls": [set("setNegativeStyle", e("NEGATIVE_PASS_THRU"))]}),
            "Pass thru negative extrapolation is not valid for MonCurve",
        ),
        (
            json!({"class": "ExponentWithLinearTransform",
                "calls": [set("setNegativeStyle", e("NEGATIVE_CLAMP"))]}),
            "Clamp negative extrapolation is not valid",
        ),
        (
            json!({"class": "Lut1DTransform", "args": {"length": 3},
                "calls": [inverse, set("setInputHalfDomain", json!(true))]}),
            "65536 required for halfDomain 1D LUT",
        ),
    ];
    let request = TransformTextRequest {
        transforms: cases.iter().map(|(spec, _)| spec.clone()).collect(),
        pairs: Vec::new(),
    };
    let reply = run(&request);
    assert_eq!(reply.transforms.len(), cases.len());
    for (built, (spec, expected)) in reply.transforms.iter().zip(&cases) {
        let message = match built {
            Built::Text(text) => {
                &text
                    .validate
                    .as_ref()
                    .unwrap_or_else(|| panic!("{spec}: valid"))
                    .message
            }
            Built::Raised(exception) => &exception.message,
        };
        assert!(message.contains(expected), "{spec}: {message}");
    }
}

/// A request's reply is the same on every run, alone or in a batch.
#[test]
fn replies_are_the_same_on_every_run() {
    let transforms: Vec<Value> = every_class().into_iter().map(|(a, _)| a).collect();
    let request = TransformTextRequest {
        pairs: (0..transforms.len()).map(|i| (i, i)).collect(),
        transforms,
    };
    let calls = [request.call()];
    let runs: Vec<_> = (0..2).map(|_| Oracle::get().batch(&calls, false)).collect();
    let alone = Oracle::get().call_uncached(calls[0].cmd, calls[0].args.clone(), &[]);
    for run in &runs {
        let batched = run[0].as_ref().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(batched.result, alone.result);
    }
}
