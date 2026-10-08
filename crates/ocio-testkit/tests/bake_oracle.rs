// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `bake` command (`oracle/ocio_oracle/bake_api.py`), against the wheel and
//! upstream's test `OCIO_ADD_TEST(Baker, bake_3dlut)` (tests/cpu/Baker_tests.cpp @ v2.5.2): the
//! cinespace and Resolve cube LUTs it bakes are upstream's, the getters report the setters, the
//! file overload writes the text in text mode, refusals report their stage, and the requests it
//! can't read are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::upstream::check_close;
use serde_json::{Value, json};

fn call(args: Value) -> Value {
    Oracle::get().call("bake", args, &[]).result
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The text with the platform's text-mode line ends: what a `std::ofstream` writes.
fn text_mode(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in text {
        if cfg!(windows) && b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}

/// `myProfile` of `bake_3dlut` (Baker_tests.cpp:38-85).
const PROFILE: &str = "ocio_profile_version: 2\n\nfile_rules:\n  - !<Rule> {name: Default, colorspace: lnh}\n\ndisplays:\n  display1:\n    - !<View> {name: view1, colorspace: gamma22}\n    - !<View> {name: view2, looks: satlook, colorspace: gamma22}\n\nlooks:\n  - !<Look>\n    name : contrastlook\n    process_space : lnh\n    transform : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}\n  - !<Look>\n    name : satlook\n    process_space : lnh\n    transform : !<CDLTransform> {sat: 2}\n\ncolorspaces:\n  - !<ColorSpace>\n    name : lnh\n    bitdepth : 16f\n    isdata : false\n    allocation : lg2\n\n  - !<ColorSpace>\n    name : gamma22\n    bitdepth : 8ui\n    isdata : false\n    allocation : uniform\n    to_reference : !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}\n\nnamed_transforms:\n- !<NamedTransform>\n  name: logcnt\n  transform: !<LogCameraTransform>\n    log_side_slope:  0.247189638318671\n    log_side_offset: 0.385536998692443\n    lin_side_slope:  5.55555555555556\n    lin_side_offset: 0.0522722750251688\n    lin_side_break:  0.0105909904954696\n    base: 10\n    direction: inverse\n\n";

/// `expectedLut` of `bake_3dlut` (Baker_tests.cpp:87-115).
const EXPECTED_CSP: &str = "CSPLUTV100\n3D\n\nBEGIN METADATA\nthis is some metadata!\nEND METADATA\n\n4\n0.000977 0.039373 1.587401 64.000000\n0.000000 0.333333 0.666667 1.000000\n4\n0.000977 0.039373 1.587401 64.000000\n0.000000 0.333333 0.666667 1.000000\n4\n0.000977 0.039373 1.587401 64.000000\n0.000000 0.333333 0.666667 1.000000\n\n2 2 2\n0.042823 0.042823 0.042823\n6.622026 0.042823 0.042823\n0.042823 6.622026 0.042823\n6.622026 6.622026 0.042823\n0.042823 0.042823 6.622026\n6.622026 0.042823 6.622026\n0.042823 6.622026 6.622026\n6.622026 6.622026 6.622026\n\n";

/// `expectedCube` of `bake_3dlut` (Baker_tests.cpp:181-193).
const EXPECTED_CUBE: &str = "LUT_1D_SIZE 10\n0.000000 0.000000 0.000000\n0.111111 0.111111 0.111111\n0.222222 0.222222 0.222222\n0.333333 0.333333 0.333333\n0.444444 0.444444 0.444444\n0.555556 0.555556 0.555556\n0.666667 0.666667 0.666667\n0.777778 0.777778 0.777778\n0.888889 0.888889 0.888889\n1.000000 1.000000 1.000000\n";

/// `StringUtils::SplitByLines`: the text's lines.
fn lines(s: &str) -> Vec<&str> {
    s.split('\n').collect()
}

/// Port of the checks of the first block of `OCIO_ADD_TEST(Baker, bake_3dlut)` @ v2.5.2, through
/// the binding: the getters report the setters, and the cinespace LUT's first 7 lines are
/// upstream's text, its numbers upstream's within its tolerance (`CompareFloats`, 1e-5).
#[test]
fn the_cinespace_lut_is_upstreams() {
    let result = call(json!({
        "config": {"yaml": PROFILE},
        "metadata": [["addChildElement", "Desc", "this is some metadata!"]],
        "calls": [
            ["setFormat", "cinespace"],
            ["setInputSpace", "lnh"],
            ["setLooks", "foo, +bar"],
            ["setLooks", ""],
            ["setTargetSpace", "gamma22"],
            ["setShaperSize", 4],
            ["setCubeSize", 2],
        ],
    }));
    let getters = &result["getters"];
    assert_eq!(bytes(&getters["getFormat"]), b"cinespace", "{result}");
    assert_eq!(bytes(&getters["getInputSpace"]), b"lnh");
    assert_eq!(bytes(&getters["getLooks"]), b"");
    assert_eq!(bytes(&getters["getTargetSpace"]), b"gamma22");
    assert_eq!(getters["getShaperSize"], 4);
    assert_eq!(getters["getCubeSize"], 2);

    let text = String::from_utf8(bytes(&result["text"])).unwrap();
    let (expected, got) = (lines(EXPECTED_CSP), lines(&text));
    assert_eq!(expected.len(), got.len(), "{text}");
    for (i, (e, g)) in expected.iter().zip(&got).enumerate() {
        if i > 6 {
            let numbers = |s: &str| -> Vec<f32> {
                s.split_whitespace().map(|t| t.parse().unwrap()).collect()
            };
            let (e, g) = (numbers(e), numbers(g));
            assert_eq!(e.len(), g.len());
            for (x, y) in e.iter().zip(&g) {
                check_close(*x, *y, 1e-5f32);
            }
        } else {
            assert_eq!(e, g);
        }
    }
    assert_eq!(
        unhex(result["file"].as_str().unwrap()),
        text_mode(text.as_bytes())
    );
}

/// The second block of `bake_3dlut`: the Resolve cube through a display and view, upstream's
/// text exactly.
#[test]
fn the_resolve_cube_is_upstreams() {
    let result = call(json!({
        "config": {"yaml": PROFILE},
        "calls": [
            ["setFormat", "resolve_cube"],
            ["setInputSpace", "lnh"],
            ["setLooks", "contrastlook"],
            ["setDisplayView", "display1", "view1"],
            ["setCubeSize", 10],
        ],
    }));
    assert_eq!(bytes(&result["text"]), EXPECTED_CUBE.as_bytes(), "{result}");
    assert_eq!(bytes(&result["getters"]["getDisplay"]), b"display1");
    assert_eq!(bytes(&result["getters"]["getView"]), b"view1");
}

/// The checks of `bake_3dlut` on the format list (Baker_tests.cpp:165-167), through
/// `file_formats`: 12 formats, `cinespace` at index 4, `3dl` at index 1.
#[test]
fn the_bake_formats_are_upstreams() {
    let result = Oracle::get().call("file_formats", json!({}), &[]).result;
    let bake = result["bake"].as_array().unwrap();
    assert_eq!(bake.len(), 12);
    assert_eq!(bytes(&bake[4][0]), b"cinespace");
    assert_eq!(bytes(&bake[1][1]), b"3dl");
}

/// A format the baker doesn't know is refused by its setter; a bake without the spaces it
/// needs is refused by `bake`.
#[test]
fn refusals_report_their_stage() {
    let unknown = call(json!({"config": {"yaml": PROFILE}, "calls": [["setFormat", "nope"]]}));
    assert_eq!(unknown["stage"], "setters", "{unknown}");
    assert_eq!(unknown["exception"]["type"], "Exception", "{unknown}");
    let incomplete = call(json!({"config": {"yaml": PROFILE},
                                 "calls": [["setFormat", "cinespace"]]}));
    assert_eq!(incomplete["stage"], "bake", "{incomplete}");
}

/// Requests it can't read are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"calls": [["bake"]]}),
        json!({"calls": [["setCubeSize", true]]}),
        json!({"calls": [["setCubeSize", 1.5]]}),
        json!({"calls": "setFormat"}),
        json!({"metadata": [["addAttribute", "a", "b"]]}),
        json!({"extra": 1}),
    ] {
        assert!(
            Oracle::get().try_call("bake", args.clone(), &[]).is_err(),
            "{args}"
        );
    }
}
