// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `write_transform` command (`oracle/ocio_oracle/write_api.py`), against the
//! wheel and upstream's tests: a group written as CLF is upstream's expected text; a
//! processor's group writes the same, with its optimization flags; the file overload writes the text in text mode; text
//! that isn't UTF-8 comes back as its bytes; refusals report their stage; the requests it can't
//! read are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

fn call(args: Value) -> Value {
    Oracle::get().call("write_transform", args, &[]).result
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The matrix of `OCIO_ADD_TEST(CTFTransform, matrix3x3_clf)`
/// (tests/cpu/fileformats/FileFormatCTF_tests.cpp:5653-5687 @ v2.5.2).
fn matrix3x3() -> Value {
    let m = [
        1. / 3.,
        10. / 3.,
        100. / 3.,
        0.,
        3.,
        4.,
        5.,
        0.,
        6.,
        7.,
        8.,
        0.,
        0.,
        0.,
        0.,
        1.,
    ];
    json!({"class": "MatrixTransform", "calls": [
        ["setFileInputBitDepth", {"enum": "BIT_DEPTH_UINT10"}],
        ["setFileOutputBitDepth", {"enum": "BIT_DEPTH_UINT10"}],
        ["setMatrix", m],
    ]})
}

/// Upstream's expected text of that test.
const MATRIX3X3_CLF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="UID42">
    <Matrix inBitDepth="10i" outBitDepth="10i">
        <Array dim="3 3">
  0.333333333333333    3.33333333333333    33.3333333333333
                  3                   4                   5
                  6                   7                   8
        </Array>
    </Matrix>
</ProcessList>
"#;

/// The text with the platform's text-mode line ends: what a `std::ofstream` writes.
fn text_mode(text: &[u8]) -> Vec<u8> {
    if cfg!(windows) {
        let mut out = Vec::new();
        for &b in text {
            if b == b'\n' {
                out.push(b'\r');
            }
            out.push(b);
        }
        out
    } else {
        text.to_vec()
    }
}

/// Port of the checks of `OCIO_ADD_TEST(CTFTransform, matrix3x3_clf)` @ v2.5.2, through the
/// binding: the group with the id `UID42` written as CLF is upstream's text; the file
/// overload writes it in text mode.
#[test]
fn a_group_written_as_clf_is_upstreams_text() {
    let result = call(json!({
        "group": {"class": "GroupTransform", "children": [matrix3x3()]},
        "metadata": [["__setitem__", "id", "UID42"]],
        "format": "Academy/ASC Common LUT Format",
    }));
    let text = bytes(&result["text"]);
    assert_eq!(String::from_utf8(text.clone()).unwrap(), MATRIX3X3_CLF);
    assert_eq!(
        unhex(result["file"].as_str().unwrap()),
        text_mode(&text),
        "{result}"
    );
    // A transform that isn't a group is written as the one transform of a new group.
    let single = call(json!({
        "group": matrix3x3(),
        "metadata": [["__setitem__", "id", "UID42"]],
        "format": "Academy/ASC Common LUT Format",
    }));
    assert_eq!(single["text"], result["text"]);
}

/// A processor's group (here `OPTIMIZATION_NONE` keeps the one matrix) writes as the group
/// does.
#[test]
fn a_processors_group_writes_the_same() {
    let group = call(json!({
        "group": matrix3x3(),
        "metadata": [["__setitem__", "id", "UID42"]],
        "format": "Academy/ASC Common LUT Format",
    }));
    let processor = call(json!({
        "processor": {"transform": matrix3x3(), "optimization": "OPTIMIZATION_NONE"},
        "metadata": [["__setitem__", "id", "UID42"]],
        "format": "Academy/ASC Common LUT Format",
    }));
    assert_eq!(processor["text"], group["text"], "{processor}");
}

/// The processor's flags are honoured: written with `OPTIMIZATION_NONE` and with
/// `OPTIMIZATION_DEFAULT`, a group of two matrices writes as many `<Matrix>` elements as the
/// optimized processor of the same flags has transforms (`processor_ops`), and the two flags
/// differ.
#[test]
fn a_processors_flags_are_honoured() {
    let scale = |k: f64| {
        json!({"class": "MatrixTransform", "calls": [
            ["setMatrix", [k, 0., 0., 0., 0., k, 0., 0., 0., 0., k, 0., 0., 0., 0., 1.]],
            ["setOffset", [0.125, 0.25, 0.5, 0.]],
        ]})
    };
    let group = json!({"class": "GroupTransform", "children": [scale(2.), scale(0.75)]});
    let mut counts = Vec::new();
    for flags in ["OPTIMIZATION_NONE", "OPTIMIZATION_DEFAULT"] {
        let written = call(json!({
            "processor": {"transform": group, "optimization": flags},
            "format": "Academy/ASC Common LUT Format",
        }));
        let text = String::from_utf8(bytes(&written["text"])).unwrap();
        let matrices = text.matches("<Matrix ").count();
        let ops = Oracle::get()
            .call(
                "processor_ops",
                json!({"transform": group, "optimization": flags}),
                &[],
            )
            .result;
        let children = ops["optimized"]["group"]["children"]
            .as_array()
            .unwrap_or_else(|| panic!("{ops}"))
            .len();
        assert_eq!(matrices, children, "{flags}: {text} {ops}");
        counts.push(matrices);
    }
    assert_ne!(counts[0], counts[1], "{counts:?}");
}

/// Text that isn't UTF-8 comes back as its bytes; a format the registry doesn't have is the
/// library's exception, at the write.
#[test]
fn undecodable_text_and_refusals() {
    let result = call(json!({
        "group": matrix3x3(),
        "metadata": [["setName", {"bytes": "61ff62"}]],
        "format": "Color Transform Format",
    }));
    let undecodable = unhex(result["text"]["undecodable"].as_str().unwrap());
    assert!(undecodable.windows(3).any(|w| w == b"a\xffb"), "{result}");
    let file = unhex(result["file"].as_str().unwrap());
    assert!(file.windows(3).any(|w| w == b"a\xffb"), "{result}");

    let missing = call(json!({"group": matrix3x3(), "format": "no such format"}));
    assert_eq!(missing["stage"], "write", "{missing}");
    assert_eq!(missing["exception"]["type"], "Exception", "{missing}");
}

/// Requests it can't read are refused.
#[test]
fn bad_requests_are_refused() {
    let group = matrix3x3();
    for args in [
        json!({"format": "ColorCorrection"}),
        json!({"group": group, "processor": {"transform": group}, "format": "ColorCorrection"}),
        json!({"group": group, "format": 1}),
        json!({"group": group, "format": "ColorCorrection", "extra": 1}),
        json!({"processor": {"transform": group, "bad": 1}, "format": "ColorCorrection"}),
        json!({"group": group, "format": "ColorCorrection", "metadata": [["remove"]]}),
        json!({"group": group, "format": "ColorCorrection", "metadata": "id"}),
    ] {
        assert!(
            Oracle::get()
                .try_call("write_transform", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
