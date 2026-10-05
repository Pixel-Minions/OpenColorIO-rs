// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's transform specs (`oracle/ocio_oracle/spec.py`), against the wheel itself:
//! - a double by its bits, `{"f64": bits}` ([`f64_spec`]): the transform holds exactly those
//!   bits, NaNs of either sign and any payload, signalling ones included, the infinities and
//!   -0.0; a value passed by its bits prints as the same value passed as a JSON number; and
//!   anything but an unsigned 64-bit integer is refused;
//! - a transform built by a static factory of its class (`"factory"`), and the specs that
//!   misuse it refused;
//! - a LUT's values taken from a request blob (`{"blob": i, "dtype": name}`), bit for bit, NaN
//!   payloads included, from the blob the spec names, in every command, after the command's
//!   own blobs; the blob specs that don't describe an array refused;
//! - what the binding raises building a spec reported alike by every command, and what the
//!   oracle refuses, or Python raises (a RuntimeError subclass), refusing the request;
//! - value objects (`{"object": spec}`), their "attrs" set before their "calls", and the
//!   object specs that misuse them refused.

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

/// The values of a 7-entry 1D LUT, red, green and blue per entry, with -0.0, a subnormal, a
/// large value, an infinity and NaNs among them: quiet and signalling, of both signs, with
/// payloads, which a blob must carry bit for bit (NumPy or the binding converting the values
/// would quiet the signalling ones or drop the payloads).
fn lut1d_values() -> Vec<f32> {
    let mut values: Vec<f32> = (0..3u8)
        .flat_map(|i| {
            let x = f32::from(i) * 0.25;
            [x * x, -x, 1.0 - x]
        })
        .collect();
    values.extend([-0.0, f32::from_bits(1), 3.0e38, f32::INFINITY, 0.1, -7.5]);
    values.extend([0x7f80_0001, 0x7fc1_2345, 0xff80_0001, 0x7fbf_ffff].map(f32::from_bits));
    values.extend([0.5, -1.0]);
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

/// Every command that takes a transform spec, with its arguments for one and its own blobs,
/// which come before the spec's.
fn commands() -> Vec<(&'static str, Args, Vec<Vec<u8>>)> {
    let pixels = f32_to_bytes(&[0.0, 0.3, 0.6, 1.0, 0.9, 0.5, 0.25, 0.75]);
    let rgb = f32_to_bytes(&[0.1, 0.2, 0.3, 0.7, 0.8, 0.9]);
    vec![
        (
            "cpu_apply",
            |t| json!({"transform": t}),
            vec![pixels.clone()],
        ),
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
            vec![pixels],
        ),
        (
            "image_apply_rgb",
            |t| json!({"transform": t, "call": "applyRGB"}),
            vec![rgb],
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
    ]
}

/// Every command that takes a transform spec passes it the request blobs that follow its own:
/// a LUT made from a blob gives each command the same response as the same LUT made entry by
/// entry, where the command's own blobs come first; and so does a GroupTransform of a 1D LUT
/// from blob 0 and a 3D LUT from blob 1, so every blob reaches the spec, each by its index. A
/// command that handed the spec the wrong blobs (its pixels, or only the last blob) would build
/// another LUT or refuse the request.
#[test]
fn every_command_passes_the_spec_its_blobs() {
    let lut1d = f32_to_bytes(&lut1d_values()[..9]);
    let lut3d = f32_to_bytes(&lut3d_values());
    let luts: [(Value, Value, Vec<&[u8]>); 3] = [
        (
            lut1d_by_values(&lut1d_values()[..9]),
            lut_by_blob("Lut1DTransform", 0, None),
            vec![&lut1d],
        ),
        (
            lut3d_by_values(&lut3d_values()),
            lut_by_blob("Lut3DTransform", 0, Some(&[2, 2, 2, 3])),
            vec![&lut3d],
        ),
        (
            json!({"class": "GroupTransform", "children": [
                lut1d_by_values(&lut1d_values()[..9]),
                lut3d_by_values(&lut3d_values()),
            ]}),
            json!({"class": "GroupTransform", "children": [
                lut_by_blob("Lut1DTransform", 0, None),
                lut_by_blob("Lut3DTransform", 1, Some(&[2, 2, 2, 3])),
            ]}),
            vec![&lut1d, &lut3d],
        ),
    ];
    let commands = commands();
    let requests: Vec<(&str, Args, Vec<&[u8]>)> = commands
        .iter()
        .map(|(cmd, args, own)| (*cmd, *args, own.iter().map(Vec::as_slice).collect()))
        .collect();
    let mut calls = Vec::new();
    for (by_values, by_blob, spec_blobs) in &luts {
        for (cmd, args, own) in &requests {
            calls.push(BatchCall {
                cmd,
                args: args(by_values),
                blobs: own.clone(),
            });
            let mut blobs = own.clone();
            blobs.extend(spec_blobs);
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
/// a NumPy type of fixed size or that is big-endian (the blobs are little-endian), a blob that
/// isn't a whole number of entries, and a shape the blob doesn't fill or that isn't a list of
/// non-negative integers. With two blobs, the indices Python would take for one of them, -1
/// (from the end) and `true` (1), are refused too.
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
            json!({"blob": 0, "dtype": ">f4"}),
            "little-endian, as the blobs are, not '>f4'",
        ),
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
    let two_blobs = [
        (
            json!({"blob": -1, "dtype": "float32"}),
            "blob -1 isn't one of the spec's 2 blobs",
        ),
        (
            json!({"blob": true, "dtype": "float32"}),
            "blob True isn't one of the spec's 2 blobs",
        ),
    ];
    let calls: Vec<BatchCall<'_>> = two_blobs
        .iter()
        .map(|(data, _)| BatchCall {
            cmd: "transform_text",
            args: json!({"transforms": [{"class": "Lut3DTransform", "calls": [["setData", data]]}]}),
            blobs: vec![&lut, &lut],
        })
        .collect();
    for ((data, fragment), result) in two_blobs.iter().zip(Oracle::get().batch(&calls, false)) {
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

/// What a command reports for the transform spec it was given, the binding's exception and
/// the stage, wherever the command puts them.
fn reported_exception(cmd: &str, result: &Value) -> (Value, Value) {
    match cmd {
        "transform_text" => (
            result["transforms"][0]["exception"].clone(),
            result["transforms"][0]["stage"].clone(),
        ),
        "processor_cache" => {
            assert_eq!(result[0]["returncode"], 0, "{result}");
            (result[0]["report"]["steps"][1].clone(), json!("transform"))
        }
        _ => (result["exception"].clone(), result["stage"].clone()),
    }
}

/// Every command reports what the binding raises building a transform spec the same way:
/// a Lut3DTransform given a blob in a shape `setData` can't take, `[3, 8]`, raises the same
/// RuntimeError, message for message, at the "transform" stage, in each command's report.
/// (processor_cache reports it as the processor step's exception; its case stops there,
/// since a later step on the processor it didn't make would stop the new process.)
#[test]
fn every_command_reports_the_bindings_exception_alike() {
    let lut = f32_to_bytes(&lut3d_values());
    let spec = lut_by_blob("Lut3DTransform", 0, Some(&[3, 8]));
    let commands = commands();
    let calls: Vec<BatchCall<'_>> = commands
        .iter()
        .map(|(cmd, args, own)| {
            let mut blobs: Vec<&[u8]> = own.iter().map(Vec::as_slice).collect();
            blobs.push(&lut);
            let args = if *cmd == "processor_cache" {
                json!({"cases": [{"env": {}, "steps": [
                    ["config"],
                    ["processor", "p", spec, "TRANSFORM_DIR_FORWARD"],
                ]}]})
            } else {
                args(&spec)
            };
            BatchCall { cmd, args, blobs }
        })
        .collect();
    let mut first: Option<Value> = None;
    for (call, result) in calls.iter().zip(Oracle::get().batch(&calls, false)) {
        let result = result.unwrap_or_else(|e| panic!("{}: {e}", call.cmd));
        let (exception, stage) = reported_exception(call.cmd, &result.result);
        assert_eq!(stage, "transform", "{}: {}", call.cmd, result.result);
        assert_eq!(
            exception["type"], "RuntimeError",
            "{}: {exception}",
            call.cmd
        );
        let message = exception["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("failed to calculate grid size from shape (3, 8)"),
            "{}: {exception}",
            call.cmd
        );
        let first = first.get_or_insert_with(|| exception.clone());
        assert_eq!(&exception, first, "{}", call.cmd);
    }
}

/// A transform spec the oracle itself refuses (a blob value spec naming a blob the request
/// doesn't have) refuses the request, whatever the command: it is never reported as if the
/// wheel had raised.
#[test]
fn every_command_refuses_a_spec_the_oracle_refuses() {
    let lut = f32_to_bytes(&lut3d_values());
    let spec = lut_by_blob("Lut3DTransform", 1, None);
    let commands = commands();
    let calls: Vec<BatchCall<'_>> = commands
        .iter()
        .map(|(cmd, args, own)| {
            let mut blobs: Vec<&[u8]> = own.iter().map(Vec::as_slice).collect();
            blobs.push(&lut);
            BatchCall {
                cmd,
                args: args(&spec),
                blobs,
            }
        })
        .collect();
    for (call, result) in calls.iter().zip(Oracle::get().batch(&calls, false)) {
        let error = result.expect_err(call.cmd);
        assert!(
            error.contains("blob 1 isn't one of the spec's 1 blobs"),
            "{}: {error}",
            call.cmd
        );
    }
}

/// A RuntimeError of Python's own refuses the request in every command: it is never reported
/// as if the wheel had raised, as the binding's RuntimeError (pybind11's std::runtime_error)
/// is. A value spec nested deeper than Python's recursion limit raises RecursionError, a
/// subclass of RuntimeError, while the oracle builds the spec.
#[test]
fn every_command_refuses_pythons_own_runtime_errors() {
    let mut deep = json!(3);
    for _ in 0..1500 {
        deep = json!([deep]);
    }
    let spec = json!({"class": "Lut1DTransform", "calls": [["setLength", deep]]});
    let commands = commands();
    let calls: Vec<BatchCall<'_>> = commands
        .iter()
        .map(|(cmd, args, own)| BatchCall {
            cmd,
            args: args(&spec),
            blobs: own.iter().map(Vec::as_slice).collect(),
        })
        .collect();
    for (call, result) in calls.iter().zip(Oracle::get().batch(&calls, false)) {
        let error = result.expect_err(call.cmd);
        assert!(error.contains("RecursionError"), "{}: {error}", call.cmd);
    }
}

/// processor_cache gives every case the request's blobs, each by its index: in each of two
/// cases, a 3D LUT made from blob 0 or from blob 1 makes a processor of the same cache ID as
/// the same LUT made entry by entry, and the two LUTs' IDs differ.
#[test]
fn processor_cache_gives_every_case_the_blobs() {
    let values = lut3d_values();
    let other: Vec<f32> = values.iter().rev().copied().collect();
    let (lut, other_lut) = (f32_to_bytes(&values), f32_to_bytes(&other));
    let shape: &[usize] = &[2, 2, 2, 3];
    let processor =
        |name: &str, spec: Value| json!(["processor", name, spec, "TRANSFORM_DIR_FORWARD"]);
    let steps = |order: [usize; 4]| {
        let all = [
            processor("v", lut3d_by_values(&values)),
            processor("b", lut_by_blob("Lut3DTransform", 0, Some(shape))),
            processor("w", lut3d_by_values(&other)),
            processor("o", lut_by_blob("Lut3DTransform", 1, Some(shape))),
        ];
        let mut steps = vec![json!(["config"])];
        steps.extend(order.iter().map(|&i| all[i].clone()));
        json!({"env": {}, "steps": steps})
    };
    let reply = Oracle::get().call(
        "processor_cache",
        json!({"cases": [steps([0, 1, 2, 3]), steps([3, 2, 1, 0])]}),
        &[&lut, &other_lut],
    );
    let cases = reply.result.as_array().expect("cases");
    assert_eq!(cases.len(), 2);
    for case in cases {
        assert_eq!(case["returncode"], 0, "{case}");
        let report = &case["report"];
        assert_eq!(
            report["steps"],
            json!([null, null, null, null, null]),
            "{case}"
        );
        let ids = &report["cache_ids"];
        assert_eq!(ids["b"], ids["v"], "{case}");
        assert_eq!(ids["o"], ids["w"], "{case}");
        assert_ne!(ids["v"], ids["w"], "{case}");
        assert_eq!(ids, &cases[0]["report"]["cache_ids"], "{case}");
    }
}

/// A spec reads the blob it names, through a GroupTransform's children and through a list of
/// transforms (GroupTransform's constructor): of two LUTs of the same size but other values,
/// from blobs 1 and 0, the processor's `createGroupTransform()` gives back blob 1's values
/// first and blob 0's second, bit for bit.
#[test]
fn a_spec_reads_the_blob_it_names() {
    let values = lut1d_values();
    let other: Vec<f32> = values.iter().rev().copied().collect();
    let (first, second) = (
        lut_by_blob("Lut1DTransform", 1, None),
        lut_by_blob("Lut1DTransform", 0, None),
    );
    let groups = [
        json!({"class": "GroupTransform", "children": [first, second]}),
        json!({"class": "GroupTransform", "args": {"transforms": [
            {"transform": first}, {"transform": second},
        ]}}),
    ];
    let bytes = [f32_to_bytes(&values), f32_to_bytes(&other)];
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    for spec in groups {
        let response = Oracle::get().call(
            "processor_ops",
            json!({"transform": spec, "optimization": "OPTIMIZATION_NONE"}),
            &[&bytes[0], &bytes[1]],
        );
        let reply = ProcessorOpsReply::from_response(response);
        assert_eq!(
            reply.processor().classes(),
            ["Lut1DTransform", "Lut1DTransform"],
            "{spec}"
        );
        let children = &reply.processor().group.children;
        let data = |i: usize| bits(&children[i].getter("getData").f32s());
        assert_eq!(data(0), bits(&other), "{spec}");
        assert_eq!(data(1), bits(&values), "{spec}");
    }
}

/// image_apply's own blobs are the ones its buffers name, up to the last one named, and the
/// transform spec's follow them: with a buffer naming only blob 1 (blob 0 another LUT's
/// values, which the spec must not read), with two buffers naming the same blob, and with no
/// buffer naming a blob, a LUT from the spec's blob 0 gives the response of the same LUT made
/// entry by entry.
#[test]
fn image_apply_splits_its_blobs_from_the_specs() {
    let values = &lut1d_values()[..9];
    let other: Vec<f32> = values.iter().rev().copied().collect();
    let (lut, decoy) = (f32_to_bytes(values), f32_to_bytes(&other));
    let pixels = f32_to_bytes(&[0.0, 0.3, 0.6, 1.0, 0.9, 0.5, 0.25, 0.75]);
    let image = |buffer: usize| {
        json!({"kind": "packed", "data": {"buffer": buffer}, "width": 2, "height": 1,
               "num_channels": 4})
    };
    let quarter = [0x00, 0x00, 0x80, 0x3e];
    let cases: [(Value, Vec<&[u8]>); 3] = [
        (json!([{"blob": 1}, {"size": 32}]), vec![&decoy, &pixels]),
        (json!([{"blob": 0}, {"blob": 0}]), vec![&pixels]),
        (json!([{"size": 32, "fill": quarter}, {"size": 32}]), vec![]),
    ];
    let mut calls = Vec::new();
    for (buffers, own) in &cases {
        let args = |transform: Value| {
            json!({"transform": transform, "buffers": buffers, "images": [image(0), image(1)],
                   "apply": [0, 1]})
        };
        calls.push(BatchCall {
            cmd: "image_apply",
            args: args(lut1d_by_values(values)),
            blobs: own.clone(),
        });
        let mut blobs = own.clone();
        blobs.push(&lut);
        calls.push(BatchCall {
            cmd: "image_apply",
            args: args(lut_by_blob("Lut1DTransform", 0, None)),
            blobs,
        });
    }
    let results = Oracle::get().batch(&calls, false);
    for (pair, call) in results.chunks(2).zip(calls.chunks(2)) {
        let what = &call[1].args["buffers"];
        let (by_values, by_blob) = (
            pair[0].as_ref().unwrap_or_else(|e| panic!("{what}: {e}")),
            pair[1].as_ref().unwrap_or_else(|e| panic!("{what}: {e}")),
        );
        assert!(
            by_values.result.get("exception").is_none(),
            "{what}: {}",
            by_values.result
        );
        assert_eq!(by_blob.result, by_values.result, "{what}");
        assert_eq!(by_blob.blobs, by_values.blobs, "{what}");
    }
}

/// An object spec sets its "attrs" before it makes its "calls": a GradingPrimary whose gamma
/// "attrs" set to 0 makes `validate(GRADING_LOG)` raise (GradingPrimary.cpp:69-79 @ v2.5.2),
/// which validating the defaults first wouldn't; without that call, the same object builds a
/// GradingPrimaryTransform of the linear style, which takes a gamma of 0.
#[test]
fn an_object_spec_sets_its_attrs_before_its_calls() {
    let zero = json!({"object": {"class": "GradingRGBM", "args": [0.0, 0.0, 0.0, 0.0]}});
    let spec = |calls: Value| {
        json!({"class": "GradingPrimaryTransform", "args": {
            "values": {"object": {"class": "GradingPrimary",
                "args": [{"enum": "GRADING_LIN"}],
                "attrs": [["gamma", zero]], "calls": calls}},
            "style": {"enum": "GRADING_LIN"}}})
    };
    let response = Oracle::get().call(
        "transform_text",
        json!({"transforms": [spec(json!([["validate", {"enum": "GRADING_LOG"}]])),
                              spec(json!([]))]}),
        &[],
    );
    let transforms = &response.result["transforms"];
    assert_eq!(transforms[0]["stage"], "transform", "{transforms}");
    let message = transforms[0]["exception"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        message.starts_with("GradingPrimary gamma '"),
        "{transforms}"
    );
    assert!(transforms[1].get("exception").is_none(), "{transforms}");
}
