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
use ocio_testkit::oracle::f32_to_bytes;
use ocio_testkit::processor_ops::{Dump, Dumped, ProcessorOpsReply, ProcessorOpsRequest};
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

/// The values of a 5-entry 1D LUT, red, green and blue per entry, with -0.0, a subnormal, a
/// large value and an infinity among them.
fn lut1d_values() -> Vec<f32> {
    let mut values: Vec<f32> = (0..3u8)
        .flat_map(|i| {
            let x = f32::from(i) * 0.25;
            [x * x, -x, 1.0 - x]
        })
        .collect();
    values.extend([-0.0, f32::from_bits(1), 3.0e38, f32::INFINITY, 0.1, -7.5]);
    values
}

/// The values of a 2x2x2 3D LUT, entry `(r, g, b)` at `3 * ((r * 2 + g) * 2 + b)`, as
/// `Lut3DTransform.setData` reads them (PyLut3DTransform.cpp:76-104 @ v2.5.2).
fn lut3d_values() -> Vec<f32> {
    (0..24u8).map(|i| f32::from(i) / 23.0 - 0.125).collect()
}

/// A Lut1DTransform made entry by entry with `setValue`, the values by their bits.
fn lut1d_by_values(values: &[f32]) -> Value {
    let mut calls = vec![json!(["setLength", values.len() / 3])];
    for (i, rgb) in values.chunks(3).enumerate() {
        calls.push(json!([
            "setValue",
            i,
            f64_spec(f64::from(rgb[0])),
            f64_spec(f64::from(rgb[1])),
            f64_spec(f64::from(rgb[2]))
        ]));
    }
    json!({"class": "Lut1DTransform", "calls": calls})
}

/// A Lut3DTransform of grid size 2 made entry by entry with `setValue`.
fn lut3d_by_values(values: &[f32]) -> Value {
    let mut calls = vec![json!(["setGridSize", 2])];
    for (i, rgb) in values.chunks(3).enumerate() {
        let (r, g, b) = (i / 4, i / 2 % 2, i % 2);
        calls.push(json!([
            "setValue",
            r,
            g,
            b,
            f64_spec(f64::from(rgb[0])),
            f64_spec(f64::from(rgb[1])),
            f64_spec(f64::from(rgb[2]))
        ]));
    }
    json!({"class": "Lut3DTransform", "calls": calls})
}

/// A LUT transform of `class` whose values come from the spec's blob `blob` with `setData`,
/// in `shape` if any.
fn lut_by_blob(class: &str, blob: usize, shape: Option<&[usize]>) -> Value {
    let mut data = json!({"blob": blob, "dtype": "float32"});
    if let Some(shape) = shape {
        data["shape"] = json!(shape);
    }
    json!({"class": class, "calls": [["setData", data]]})
}

/// A LUT's values passed as a blob reach `setData` whole: the processor's
/// `createGroupTransform()` gives them back, bit for bit, in the LUT's `getData()`, for a 1D
/// LUT and for a 3D LUT given flat or in its `[2, 2, 2, 3]` shape.
#[test]
fn a_lut_takes_its_values_from_a_blob() {
    let cases = [
        (lut_by_blob("Lut1DTransform", 0, None), lut1d_values()),
        (lut_by_blob("Lut3DTransform", 0, None), lut3d_values()),
        (
            lut_by_blob("Lut3DTransform", 0, Some(&[2, 2, 2, 3])),
            lut3d_values(),
        ),
    ];
    for (spec, values) in cases {
        let bytes = f32_to_bytes(&values);
        let response = Oracle::get().call(
            "processor_ops",
            json!({"transform": spec, "optimization": "OPTIMIZATION_NONE"}),
            &[&bytes],
        );
        let reply = ProcessorOpsReply::from_response(response);
        assert_eq!(
            reply.processor().classes(),
            [spec["class"].as_str().unwrap()],
            "{spec}"
        );
        let group = &reply.processor().group;
        let data = group.children[0].getter("getData").f32s();
        let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&data), bits(&values), "{spec}");
    }
}

/// Each command's arguments for a transform spec.
type Args = fn(&Value) -> Value;

/// Every command that takes a transform spec passes it the request blobs that follow its own:
/// a LUT made from a blob gives each command the same response as the same LUT made entry by
/// entry, where the command's own blobs come first. A command that handed the spec the wrong
/// blobs would build another LUT (from its pixels) or refuse the request.
#[test]
fn every_command_passes_the_spec_its_blobs() {
    let lut1d = f32_to_bytes(&lut1d_values()[..9]);
    let lut3d = f32_to_bytes(&lut3d_values());
    let pixels = f32_to_bytes(&[0.0, 0.3, 0.6, 1.0, 0.9, 0.5, 0.25, 0.75]);
    let rgb = f32_to_bytes(&[0.1, 0.2, 0.3, 0.7, 0.8, 0.9]);
    let luts = [
        (
            lut1d_by_values(&lut1d_values()[..9]),
            lut_by_blob("Lut1DTransform", 0, None),
            &lut1d,
        ),
        (
            lut3d_by_values(&lut3d_values()),
            lut_by_blob("Lut3DTransform", 0, Some(&[2, 2, 2, 3])),
            &lut3d,
        ),
    ];
    let requests: [(&str, Args, Vec<&[u8]>); 8] = [
        ("cpu_apply", |t| json!({"transform": t}), vec![&pixels]),
        (
            "image_apply",
            |t| {
                json!({
                    "transform": t,
                    "buffers": [{"blob": 0}, {"size": 32}],
                    "images": [
                        {"kind": "packed", "data": {"buffer": 0}, "width": 2, "height": 1,
                         "num_channels": 4},
                        {"kind": "packed", "data": {"buffer": 1}, "width": 2, "height": 1,
                         "num_channels": 4},
                    ],
                    "apply": [0, 1],
                })
            },
            vec![&pixels],
        ),
        (
            "image_apply_rgb",
            |t| json!({"transform": t, "call": "applyRGB"}),
            vec![&rgb],
        ),
        ("processor_debug_log", |t| json!({"transform": t}), vec![]),
        ("gpu_shader", |t| json!({"transform": t}), vec![]),
        ("processor_ops", |t| json!({"transform": t}), vec![]),
        (
            "processor_cache",
            |t| {
                json!({"cases": [{"env": {}, "steps": [
                    ["config"],
                    ["processor", "p", t, "TRANSFORM_DIR_FORWARD"],
                    ["cpu", "c", "p", "BIT_DEPTH_F32", "BIT_DEPTH_F32", null],
                ]}]})
            },
            vec![],
        ),
        ("transform_text", |t| json!({"transforms": [t]}), vec![]),
    ];
    let mut calls = Vec::new();
    for (by_values, by_blob, lut) in &luts {
        for (cmd, args, own) in &requests {
            calls.push(BatchCall {
                cmd,
                args: args(by_values),
                blobs: own.clone(),
            });
            let mut blobs = own.clone();
            blobs.push(lut.as_slice());
            calls.push(BatchCall {
                cmd,
                args: args(by_blob),
                blobs,
            });
        }
    }
    let results = Oracle::get().batch(&calls, false);
    for (pair, call) in results.chunks(2).zip(calls.chunks(2)) {
        let what = format!("{} of {}", call[1].cmd, call[1].args);
        let (by_values, by_blob) = (
            pair[0].as_ref().unwrap_or_else(|e| panic!("{what}: {e}")),
            pair[1].as_ref().unwrap_or_else(|e| panic!("{what}: {e}")),
        );
        let text = by_values.result.to_string();
        assert!(
            !text.contains("exception") && !text.contains("\"type\""),
            "{what}: the wheel refused the LUT: {text}"
        );
        assert_eq!(by_blob.result, by_values.result, "{what}");
        assert_eq!(by_blob.blobs, by_values.blobs, "{what}");
    }
}

/// Blob value specs that don't describe an array of the spec's blobs are refused: an index
/// out of range, a bool or a string for an index, no dtype, an unknown key, a dtype that isn't
/// a NumPy type of fixed size, a blob that isn't a whole number of entries, and a shape the
/// blob doesn't fill or that isn't a list of non-negative integers.
#[test]
fn bad_blob_specs_are_refused() {
    let lut = f32_to_bytes(&lut3d_values());
    let cases = [
        (
            json!({"blob": 1, "dtype": "float32"}),
            "blob 1 isn't one of the spec's 1 blobs",
        ),
        (
            json!({"blob": true, "dtype": "float32"}),
            "blob True isn't one",
        ),
        (
            json!({"blob": "0", "dtype": "float32"}),
            "blob '0' isn't one",
        ),
        (json!({"blob": 0}), "takes blob, dtype and optionally shape"),
        (
            json!({"blob": 0, "dtype": "float32", "size": 3}),
            "takes blob, dtype and optionally shape",
        ),
        (
            json!({"blob": 0, "dtype": "object"}),
            "of fixed size, not 'object'",
        ),
        (json!({"blob": 0, "dtype": 4}), "of fixed size, not 4"),
        (
            json!({"blob": 0, "dtype": "V7"}),
            "blob 0 has 96 bytes, not a whole number of",
        ),
        (
            json!({"blob": 0, "dtype": "float64", "shape": [24]}),
            "blob 0 has 12 float64 entries, which don't fill the shape [24]",
        ),
        (
            json!({"blob": 0, "dtype": "float32", "shape": [2, 2, 2, 2]}),
            "don't fill the shape [2, 2, 2, 2]",
        ),
        (
            json!({"blob": 0, "dtype": "float32", "shape": [24, -1]}),
            "list of non-negative integers, not [24, -1]",
        ),
        (
            json!({"blob": 0, "dtype": "float32", "shape": 24}),
            "list of non-negative integers, not 24",
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(data, _)| BatchCall {
            cmd: "transform_text",
            args: json!({"transforms": [{"class": "Lut3DTransform", "calls": [["setData", data]]}]}),
            blobs: vec![&lut],
        })
        .collect();
    for ((data, fragment), result) in cases.iter().zip(Oracle::get().batch(&calls, false)) {
        let error = result.expect_err(&format!("{data}: refused"));
        assert!(error.contains(fragment), "{data}: {error}");
    }
}

/// `values` (f32s) as spec values by their bits.
fn f32_specs(values: &[f32]) -> Vec<Value> {
    values.iter().map(|&v| f64_spec(f64::from(v))).collect()
}

/// A GradingBSplineCurve object spec from control points `[x0, y0, x1, y1, ...]` (the
/// binding's constructor from a list, positional), with `setSlopes(slopes)` if any.
fn curve_spec(points: &[f32], slopes: Option<&[f32]>) -> Value {
    let mut spec = json!({"class": "GradingBSplineCurve", "args": [f32_specs(points)]});
    if let Some(slopes) = slopes {
        spec["calls"] = json!([["setSlopes", f32_specs(slopes)]]);
    }
    json!({ "object": spec })
}

/// A GradingBSplineCurve as `checks.dump` writes it out: its control points `[x0, y0, ...]`
/// and its slopes, by their bits.
fn curve_bits(curve: &Dump) -> (Vec<u64>, Vec<u64>) {
    let points = match curve.getter("getControlPoints") {
        Dumped::List(points) => points
            .iter()
            .flat_map(|p| {
                let p = p.object();
                [p.property("x").f64(), p.property("y").f64()]
            })
            .map(f64::to_bits)
            .collect(),
        other => panic!("not a list of control points: {other:?}"),
    };
    let slopes = curve
        .getter("getSlopes")
        .f64s()
        .into_iter()
        .map(f64::to_bits)
        .collect();
    (points, slopes)
}

/// The bits of `values` widened to doubles, as the binding returns C floats.
fn widened_bits(values: &[f32]) -> Vec<u64> {
    values.iter().map(|&v| f64::from(v).to_bits()).collect()
}

/// Object specs build the value objects a GradingRGBCurveTransform takes: GradingBSplineCurves
/// from a list of control points (positional), with `setSlopes` called on one; a
/// GradingRGBCurve from them by keyword arguments, and with its `master` property set through
/// "attrs". The processor's `createGroupTransform()` gives every control point and slope back,
/// bit for bit, on the curves the spec set.
#[test]
fn a_grading_curve_transform_takes_value_objects() {
    let red = [0.0, 0.1, 0.5, 0.6, 1.0, 1.2f32];
    let red_slopes = [1.0, 0.8, 1.5f32];
    let green = [-0.25, -0.5, 0.125, 0.3, 0.75, 0.7, 2.0, 1.9f32];
    let master = [0.0, 0.0, 0.3, 0.2, 1.0, 1.0f32];
    let rgb_curve = json!({"object": {
        "class": "GradingRGBCurve",
        "args": {"red": curve_spec(&red, Some(&red_slopes)), "green": curve_spec(&green, None)},
        "attrs": [["master", curve_spec(&master, None)]],
    }});
    let spec = json!({"class": "GradingRGBCurveTransform",
        "args": {"values": rgb_curve, "style": {"enum": "GRADING_LIN"}}});
    let mut request = ProcessorOpsRequest::new(json!({ "transform": spec }));
    request.optimization = Some(json!("OPTIMIZATION_NONE"));
    let reply = request.run();
    assert_eq!(
        reply.processor().classes(),
        ["GradingRGBCurveTransform"],
        "{}",
        reply.result
    );
    let transform = &reply.processor().group.children[0];
    assert_eq!(transform.getter("getStyle").name(), "GRADING_LIN");
    let values = transform.getter("getValue").object();
    assert_eq!(values.class, "GradingRGBCurve");
    let curve = |name: &str| curve_bits(values.property(name).object());
    assert_eq!(
        curve("red"),
        (widened_bits(&red), widened_bits(&red_slopes))
    );
    assert_eq!(curve("green").0, widened_bits(&green));
    assert_eq!(curve("master").0, widened_bits(&master));
}

/// Object specs that the oracle can't build as asked are refused: a transform's class (a
/// transform spec builds those), no class, an unknown key, both args and a factory, attrs
/// that aren't [name, value] pairs, and an attribute the class doesn't have (the binding's
/// AttributeError).
#[test]
fn bad_object_specs_are_refused() {
    let curve = curve_spec(&[0.0, 0.0, 1.0, 1.0], None);
    let cases = [
        (
            json!({"class": "LogTransform"}),
            "an instance of a class that isn't a transform's, not 'LogTransform'",
        ),
        (
            json!({"args": [0.5, 0.5]}),
            "an object spec takes class and optionally",
        ),
        (
            json!({"class": "GradingControlPoint", "children": []}),
            "an object spec takes class and optionally",
        ),
        (
            json!({"class": "GradingRGBCurve", "args": {"red": curve}, "factory": ["Create"]}),
            "an object spec takes args or a factory, not both",
        ),
        (
            json!({"class": "GradingRGBCurve", "attrs": [["red"]]}),
            "attrs are [name, value] pairs, not ['red']",
        ),
        (
            json!({"class": "GradingRGBCurve", "attrs": [["redd", curve]]}),
            "has no attribute 'redd'",
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(object, _)| BatchCall {
            cmd: "transform_text",
            args: json!({"transforms": [{"class": "GradingRGBCurveTransform",
                "args": {"values": {"object": object}}}]}),
            blobs: Vec::new(),
        })
        .collect();
    for ((object, fragment), result) in cases.iter().zip(Oracle::get().batch(&calls, false)) {
        let error = result.expect_err(&format!("{object}: refused"));
        assert!(error.contains(fragment), "{object}: {error}");
    }
}
