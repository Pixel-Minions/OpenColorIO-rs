// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's transform specs (`oracle/ocio_oracle/spec.py`), against the wheel itself:
//! - a double by its bits, `{"f64": bits}` ([`f64_spec`]): the transform holds exactly those
//!   bits, NaNs of either sign and any payload, signalling ones included, the infinities and
//!   -0.0; a value passed by its bits prints as the same value passed as a JSON number; and
//!   anything but an unsigned 64-bit integer is refused;
//! - a transform built by a static factory of its class (`"factory"`), and the specs that
//!   misuse it refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::processor_ops::ProcessorOpsRequest;
use ocio_testkit::transform_text::{TransformTextRequest, f64_spec};
use serde_json::{Value, json};

/// Doubles JSON can't hold or loses: quiet and signalling NaNs of both signs, one with a
/// payload, the infinities, -0.0; and the extremes of the finite ones.
fn specials() -> Vec<f64> {
    [
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0xfff0_0000_0000_0001,
        0x7ff4_0000_0000_beef,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x7fef_ffff_ffff_ffff,
        0x8010_0000_0000_0000,
        0x3fb9_9999_9999_999a,
    ]
    .map(f64::from_bits)
    .to_vec()
}

/// A MatrixTransform holds the bits it was given, in its matrix and its offsets: the
/// processor's `createGroupTransform()` reports them through `getMatrix()` and `getOffset()`
/// (a C double comes back exact).
#[test]
fn a_transform_holds_the_bits_it_was_given() {
    let specials = specials();
    let matrix: Vec<f64> = (0..16).map(|i| specials[i % specials.len()]).collect();
    let offset: Vec<f64> = (0..4).map(|i| specials[(i + 5) % specials.len()]).collect();
    let spec = json!({"class": "MatrixTransform", "calls": [
        ["setMatrix", matrix.iter().map(|&v| f64_spec(v)).collect::<Vec<_>>()],
        ["setOffset", offset.iter().map(|&v| f64_spec(v)).collect::<Vec<_>>()],
    ]});
    let mut request = ProcessorOpsRequest::new(json!({"transform": spec}));
    request.optimization = Some(json!("OPTIMIZATION_NONE"));
    let reply = request.run();
    let group = &reply.processor().group;
    assert_eq!(group.children.len(), 1, "{}", reply.result);
    let child = &group.children[0];
    let bits = |values: Vec<f64>| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(child.getter("getMatrix").f64s()), bits(matrix));
    assert_eq!(bits(child.getter("getOffset").f64s()), bits(offset));
}

/// A double passed by its bits prints as the same double passed as a JSON number.
#[test]
fn bits_print_as_the_number() {
    let values = [0.1, -2.5, 1e300, 5e-324, 1.0 / 3.0];
    let spec = |v: Value| json!({"class": "MatrixTransform", "calls": [["setOffset", [v, 0.0, 0.0, 0.0]]]});
    let mut transforms = Vec::new();
    for v in values {
        transforms.push(spec(json!(v)));
        transforms.push(spec(f64_spec(v)));
    }
    let reply = TransformTextRequest {
        transforms,
        pairs: Vec::new(),
    }
    .run();
    for (k, v) in values.iter().enumerate() {
        let (number, bits) = (
            reply.transforms[2 * k].text(),
            reply.transforms[2 * k + 1].text(),
        );
        assert_eq!(number.repr, bits.repr, "{v}");
    }
}

/// Bits that aren't an unsigned 64-bit integer are refused: negative, too large, fractional,
/// a bool, a string; and a value spec with more keys than `f64` is unknown.
#[test]
fn bad_bits_are_refused() {
    let cases = [
        (json!(-1), "not -1"),
        (
            json!(18_446_744_073_709_551_616.0_f64),
            "not 1.8446744073709552e+19",
        ),
        (json!(1.5), "not 1.5"),
        (json!(true), "not True"),
        (json!("1"), "not '1'"),
    ];
    let mut calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(bits, _)| BatchCall {
            cmd: "transform_text",
            args: json!({"transforms": [{"class": "MatrixTransform",
                "calls": [["setOffset", [{"f64": bits}, 0.0, 0.0, 0.0]]]}]}),
            blobs: Vec::new(),
        })
        .collect();
    calls.push(BatchCall {
        cmd: "transform_text",
        args: json!({"transforms": [{"class": "MatrixTransform",
            "calls": [["setOffset", [{"f64": 0, "enum": "x"}, 0.0, 0.0, 0.0]]]}]}),
        blobs: Vec::new(),
    });
    let results = Oracle::get().batch(&calls, false);
    for ((bits, fragment), result) in cases.iter().zip(&results) {
        let error = result.as_ref().expect_err(&format!("{bits}: refused"));
        assert!(
            error.contains("f64 bits must be an unsigned 64-bit integer")
                && error.contains(fragment),
            "{bits}: {error}"
        );
    }
    let error = results[cases.len()].as_ref().expect_err("refused");
    assert!(error.contains("unknown value spec"), "{error}");
}

/// `"factory"` builds the transform with the class's static factory: `MatrixTransform.Scale`
/// gives the diagonal matrix of its scale (the processor reports it), where the constructor
/// would give the identity; "calls" apply after it.
#[test]
fn a_factory_builds_the_transform() {
    let scale = [2.0, 0.5, f64::NAN, -0.0];
    let spec = json!({"class": "MatrixTransform",
        "factory": ["Scale", scale.iter().map(|&v| f64_spec(v)).collect::<Vec<_>>()],
        "calls": [["setOffset", [0.25, 0.0, 0.0, 0.0]]]});
    let mut request = ProcessorOpsRequest::new(json!({"transform": spec}));
    request.optimization = Some(json!("OPTIMIZATION_NONE"));
    let reply = request.run();
    let child = &reply.processor().group.children[0];
    let matrix = child.getter("getMatrix").f64s();
    for (i, value) in matrix.iter().enumerate() {
        let expected = if i % 5 == 0 { scale[i / 5] } else { 0.0 };
        assert_eq!(value.to_bits(), expected.to_bits(), "{i}: {matrix:?}");
    }
    assert_eq!(child.getter("getOffset").f64s(), [0.25, 0.0, 0.0, 0.0]);
}

/// A spec with both "args" and "factory", and a factory that returns something else than a
/// transform of the class, are refused.
#[test]
fn bad_factories_are_refused() {
    let cases = [
        (
            json!({"class": "MatrixTransform", "args": {"offset": [0.0, 0.0, 0.0, 0.0]},
                "factory": ["Identity"]}),
            "takes args or a factory, not both",
        ),
        (
            json!({"class": "GroupTransform", "factory": ["GetWriteFormats"]}),
            "GroupTransform.GetWriteFormats returned a WriteFormatIterator, not a GroupTransform",
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(spec, _)| BatchCall {
            cmd: "transform_text",
            args: json!({"transforms": [spec]}),
            blobs: Vec::new(),
        })
        .collect();
    for ((spec, fragment), result) in cases.iter().zip(Oracle::get().batch(&calls, false)) {
        let error = result.expect_err(&format!("{spec}: refused"));
        assert!(error.contains(fragment), "{spec}: {error}");
    }
}
