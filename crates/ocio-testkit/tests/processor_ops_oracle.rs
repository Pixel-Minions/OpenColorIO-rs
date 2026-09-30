// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `processor_ops` command (`oracle/ocio_oracle/processor_ops.py`, chunk O1.1)
//! against the wheel itself: every transform class a processor's ops turn back into comes out
//! with the values it went in with; the bit depths and flags reach `getOptimizedProcessor`;
//! getters that need arguments are listed, and their values are elsewhere in the dump; every
//! error path raises where the command says; its refusals; and replies that don't depend on the
//! run.
//!
//! **Error paths**, by stage (paths relative to `upstream/OpenColorIO/src/OpenColorIO` @ v2.5.2):
//! - `config`, `transform`, `processor`: as in `cpu_apply`.
//! - `optimize`: `getOptimizedProcessor` with UINT14, UINT32 or UNKNOWN and ops to optimize
//!   (`IsFloatBitDepth`, BitDepthUtils.cpp:90-117, from OpOptimizers.cpp:764-768). An empty
//!   processor doesn't raise: `optimizeForBitdepth` skips an empty list (OpOptimizers.cpp:762).
//! - `group`, `optimized_group`: `createGroupTransform` raises for an op without a transform
//!   (Transform.cpp:374-382), and every op a processor can hold has one.

use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth;
use ocio_testkit::oracle::{BatchCall, Response};
use ocio_testkit::processor_ops::{Dump, Dumped, ProcessorOpsReply, ProcessorOpsRequest};
use serde_json::{Value, json};

/// Runs `requests` in one oracle process; each must succeed.
fn run(requests: &[ProcessorOpsRequest]) -> Vec<ProcessorOpsReply> {
    let calls: Vec<BatchCall<'_>> = requests.iter().map(ProcessorOpsRequest::call).collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            ProcessorOpsReply::from_response(r.unwrap_or_else(|e| panic!("call {i}: {e}")))
        })
        .collect()
}

fn group(children: Vec<Value>) -> Value {
    json!({"transform": {"class": "GroupTransform", "children": children}})
}

fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|v| v.to_bits()).collect()
}

/// A getter's floats, as bits.
fn getter_bits(dump: &Dump, name: &str) -> Vec<u64> {
    bits(&dump.getter(name).f64s())
}

/// A LUT's entries (RGB triples), set entry by entry.
fn lut1d_entries(n: usize) -> Vec<[f32; 3]> {
    (0..n)
        .map(|i| {
            let x = i as f32 / (n - 1) as f32;
            [x * x, x * 0.5, x * 0.25 + 0.125]
        })
        .collect()
}

/// Every transform class a processor's ops turn back into, unoptimized, comes out of the
/// processor and of the optimized processor with the class, the order and the values it went in
/// with: floats bit for bit, enums by name, LUT entries from their blob, grading values as
/// objects with their properties. Nothing changes without optimization, cache ID included.
#[test]
fn every_class_comes_back_with_its_values() {
    let matrix = [
        1.0, 2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 1.0,
    ];
    let offset = [0.1, 0.2, 0.3, 0.4];
    let lut1d = lut1d_entries(4);
    let lut1d_calls: Vec<Value> = lut1d
        .iter()
        .enumerate()
        .map(|(i, v)| json!(["setValue", i, v[0], v[1], v[2]]))
        .collect();
    let lut3d: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            let i = i as f32;
            [i / 7.0, (7.0 - i) / 7.0, i * i / 49.0]
        })
        .collect();
    let lut3d_calls: Vec<Value> = lut3d
        .iter()
        .enumerate()
        .map(|(i, v)| json!(["setValue", i / 4, i / 2 % 2, i % 2, v[0], v[1], v[2]]))
        .collect();
    let children = vec![
        json!({"class": "MatrixTransform", "args": {"matrix": matrix, "offset": offset}}),
        json!({"class": "RangeTransform", "args": {"minInValue": -0.5, "maxInValue": 2.0,
            "minOutValue": 0.1, "maxOutValue": 3.0}}),
        json!({"class": "ExponentTransform", "args": {"value": [2.2, 2.4, 1.8, 1.0]}}),
        json!({"class": "ExponentWithLinearTransform",
            "args": {"gamma": [2.4, 2.2, 2.0, 1.0], "offset": [0.055, 0.099, 0.1, 0.0]}}),
        json!({"class": "LogTransform", "args": {"base": 3.0}}),
        json!({"class": "LogAffineTransform",
            "args": {"logSideSlope": [0.3, 0.29, 0.31], "linSideOffset": [0.01, 0.02, 0.03]},
            "calls": [["setBase", 10.0]]}),
        json!({"class": "LogCameraTransform", "args": {"linSideBreak": [0.01, 0.02, 0.03]},
            "calls": [["setBase", 2.0]]}),
        json!({"class": "CDLTransform", "args": {"slope": [1.1, 0.9, 1.3],
            "offset": [0.01, -0.02, 0.03], "power": [1.2, 0.8, 1.0], "sat": 0.77}}),
        json!({"class": "ExposureContrastTransform",
            "args": {"exposure": 0.5, "contrast": 1.25, "gamma": 1.1, "pivot": 0.2},
            "calls": [["makeExposureDynamic"]]}),
        json!({"class": "FixedFunctionTransform",
            "args": {"style": {"enum": "FIXED_FUNCTION_ACES_GLOW_03"}}}),
        json!({"class": "GradingPrimaryTransform", "args": {"style": {"enum": "GRADING_LOG"}}}),
        json!({"class": "GradingToneTransform", "args": {"style": {"enum": "GRADING_VIDEO"}}}),
        json!({"class": "GradingHueCurveTransform", "args": {"style": {"enum": "GRADING_LOG"}}}),
        json!({"class": "GradingRGBCurveTransform", "args": {"style": {"enum": "GRADING_LIN"}}}),
        json!({"class": "Lut1DTransform", "args": {"length": lut1d.len()}, "calls": lut1d_calls}),
        json!({"class": "Lut3DTransform", "args": {"gridSize": 2}, "calls": lut3d_calls}),
    ];
    let classes: Vec<String> = children
        .iter()
        .map(|c| c["class"].as_str().unwrap().to_string())
        .collect();
    let mut request = ProcessorOpsRequest::new(group(children));
    request.optimization = Some(json!("OPTIMIZATION_NONE"));
    let reply = &run(&[request])[0];
    let (processor, optimized) = (reply.processor(), reply.optimized());
    assert_eq!(processor.classes(), classes);
    assert_eq!(processor.group, optimized.group);
    assert_eq!(processor.cache_id, optimized.cache_id);
    let t = &optimized.group.children;
    assert_eq!(t[0].getter("getMatrix").f64s(), matrix.to_vec());
    assert_eq!(getter_bits(&t[0], "getOffset"), bits(&offset));
    for (name, value) in [
        ("getMinInValue", -0.5),
        ("getMaxInValue", 2.0),
        ("getMinOutValue", 0.1),
        ("getMaxOutValue", 3.0),
    ] {
        assert_eq!(
            t[1].getter(name).f64().to_bits(),
            f64::to_bits(value),
            "{name}"
        );
    }
    assert_eq!(t[1].getter("hasMinInValue"), &Dumped::Bool(true));
    assert_eq!(getter_bits(&t[2], "getValue"), bits(&[2.2, 2.4, 1.8, 1.0]));
    assert_eq!(getter_bits(&t[3], "getGamma"), bits(&[2.4, 2.2, 2.0, 1.0]));
    assert_eq!(
        getter_bits(&t[3], "getOffset"),
        bits(&[0.055, 0.099, 0.1, 0.0])
    );
    assert_eq!(t[4].getter("getBase").f64().to_bits(), 3.0f64.to_bits());
    assert_eq!(t[5].getter("getBase").f64().to_bits(), 10.0f64.to_bits());
    assert_eq!(
        getter_bits(&t[5], "getLogSideSlopeValue"),
        bits(&[0.3, 0.29, 0.31])
    );
    assert_eq!(
        getter_bits(&t[5], "getLinSideOffsetValue"),
        bits(&[0.01, 0.02, 0.03])
    );
    assert_eq!(
        getter_bits(&t[6], "getLinSideBreakValue"),
        bits(&[0.01, 0.02, 0.03])
    );
    assert_eq!(getter_bits(&t[7], "getSlope"), bits(&[1.1, 0.9, 1.3]));
    assert_eq!(getter_bits(&t[7], "getOffset"), bits(&[0.01, -0.02, 0.03]));
    assert_eq!(getter_bits(&t[7], "getPower"), bits(&[1.2, 0.8, 1.0]));
    assert_eq!(t[7].getter("getSat").f64().to_bits(), 0.77f64.to_bits());
    for (name, value) in [
        ("getExposure", 0.5),
        ("getContrast", 1.25),
        ("getGamma", 1.1),
        ("getPivot", 0.2),
    ] {
        assert_eq!(
            t[8].getter(name).f64().to_bits(),
            f64::to_bits(value),
            "{name}"
        );
    }
    assert_eq!(t[8].getter("isExposureDynamic"), &Dumped::Bool(true));
    assert_eq!(t[8].getter("isContrastDynamic"), &Dumped::Bool(false));
    assert_eq!(
        t[9].getter("getStyle").name(),
        "FIXED_FUNCTION_ACES_GLOW_03"
    );
    assert_eq!(t[10].getter("getStyle").name(), "GRADING_LOG");
    let primary = t[10].getter("getValue").object();
    assert_eq!(primary.class, "GradingPrimary");
    let brightness = primary.property("brightness").object();
    assert_eq!(brightness.class, "GradingRGBM");
    for channel in ["red", "green", "blue", "master"] {
        brightness.property(channel).f64();
    }
    assert_eq!(t[11].getter("getStyle").name(), "GRADING_VIDEO");
    assert_eq!(t[13].getter("getStyle").name(), "GRADING_LIN");
    assert_eq!(t[14].getter("getLength"), &Dumped::Int(4));
    let data: Vec<u32> = t[14]
        .getter("getData")
        .f32s()
        .iter()
        .map(|v| v.to_bits())
        .collect();
    let given: Vec<u32> = lut1d.concat().iter().map(|v| v.to_bits()).collect();
    assert_eq!(data, given, "the 1D LUT's entries");
    assert_eq!(t[15].getter("getGridSize"), &Dumped::Int(2));
    let data: Vec<u32> = t[15]
        .getter("getData")
        .f32s()
        .iter()
        .map(|v| v.to_bits())
        .collect();
    // The entries in one of the two orders a 3D LUT can hold them: blue fastest, as `lut3d`
    // holds them, or red fastest.
    let red_fastest: Vec<[f32; 3]> = (0..8)
        .map(|k| lut3d[k % 2 * 4 + k / 2 % 2 * 2 + k / 4])
        .collect();
    let entries = |e: &[[f32; 3]]| -> Vec<u32> { e.concat().iter().map(|v| v.to_bits()).collect() };
    assert!(
        data == entries(&lut3d) || data == entries(&red_fastest),
        "the 3D LUT's entries"
    );
    for transform in t {
        assert_eq!(
            transform.getter("getDirection").name(),
            "TRANSFORM_DIR_FORWARD",
            "{}",
            transform.class
        );
    }
}

/// The bit depths and flags reach `getOptimizedProcessor`: each changes what the optimizer makes
/// of the same processor, whose own cache ID and ops stay the same.
#[test]
fn bit_depths_and_flags_reach_the_optimizer() {
    // An identity matrix, which optimization removes; a matrix and a log, which an integer
    // input bakes into a LUT; a closing clamp, which an integer output makes redundant.
    let spec = group(vec![
        json!({"class": "MatrixTransform"}),
        json!({"class": "MatrixTransform", "args": {"offset": [0.125, -0.25, 0.0625, 0.5]}}),
        json!({"class": "LogTransform", "args": {"base": 2.0}}),
        json!({"class": "RangeTransform", "args": {"minInValue": 0.0, "maxInValue": 1.0,
            "minOutValue": 0.0, "maxOutValue": 1.0}}),
    ]);
    let request = |in_bd: Option<BitDepth>, out_bd: Option<BitDepth>, flags: Option<&str>| {
        let mut request = ProcessorOpsRequest::new(spec.clone());
        request.in_bitdepth = in_bd;
        request.out_bitdepth = out_bd;
        request.optimization = flags.map(|f| json!(f));
        request
    };
    let replies = run(&[
        request(None, None, None),
        request(None, None, Some("OPTIMIZATION_DEFAULT")),
        request(None, None, Some("OPTIMIZATION_NONE")),
        request(Some(BitDepth::Uint8), None, None),
        request(None, Some(BitDepth::Uint8), None),
    ]);
    let processor = replies[0].processor();
    for reply in &replies[1..] {
        assert_eq!(reply.processor(), processor, "the processor itself");
    }
    // The defaults are F32, F32 and OPTIMIZATION_DEFAULT.
    assert_eq!(replies[0].optimized(), replies[1].optimized());
    let optimized: Vec<_> = replies.iter().map(|r| r.optimized()).collect();
    for (i, a) in optimized.iter().enumerate().skip(1) {
        for b in &optimized[i + 1..] {
            assert_ne!(a.cache_id, b.cache_id);
            assert_ne!(a.group, b.group);
        }
    }
    // Without optimization, the processor's ops.
    assert_eq!(optimized[2].group, processor.group);
}

/// The getters that need arguments are listed, and the values they would give are elsewhere in
/// the dump: a LUT's entries in `getData`, a curve's slopes in its curves' `getSlopes`.
#[test]
fn getters_that_need_arguments_are_listed() {
    let spec = group(vec![
        json!({"class": "Lut1DTransform", "args": {"length": 3},
            "calls": [["setValue", 1, 0.3, 0.2, 0.1]]}),
        json!({"class": "Lut3DTransform", "args": {"gridSize": 2},
            "calls": [["setValue", 1, 0, 0, 0.3, 0.2, 0.1]]}),
        json!({"class": "GradingRGBCurveTransform", "args": {"style": {"enum": "GRADING_LOG"}}}),
    ]);
    let mut request = ProcessorOpsRequest::new(spec);
    request.optimization = Some(json!("OPTIMIZATION_NONE"));
    let reply = &run(&[request])[0];
    let t = &reply.processor().group.children;
    assert_eq!(t[0].uncalled, vec!["getValue"]);
    assert_eq!(t[0].getter("getData").f32s().len(), 3 * 3);
    assert_eq!(t[1].uncalled, vec!["getValue"]);
    assert_eq!(t[1].getter("getData").f32s().len(), 2 * 2 * 2 * 3);
    assert_eq!(t[2].uncalled, vec!["getSlope"]);
    let curves = t[2].getter("getValue").object();
    for channel in ["red", "green", "blue", "master"] {
        let curve = curves.property(channel).object();
        let points = match curve.getter("getControlPoints") {
            Dumped::List(points) => points.len(),
            other => panic!("{other:?}"),
        };
        assert_eq!(curve.getter("getSlopes").f64s().len(), points, "{channel}");
    }
}

/// Where a request raises and a fragment of the message.
type Expected = (&'static str, &'static str);

/// Every error path the header lists, with its stage and message.
#[test]
fn every_error_path_raises() {
    let with_depth = |spec: Value, depth: &str| {
        let mut args = spec;
        args["in_bitdepth"] = json!(depth);
        args
    };
    let cases: Vec<(&str, Value, Option<Expected>)> = vec![
        (
            "a config that doesn't parse",
            json!({"config": {"yaml": "ocio_profile_version: 2\ncolorspaces: 3\n"},
                "src": "a", "dst": "b"}),
            Some(("config", "")),
        ),
        (
            "a transform the binding's constructor refuses",
            json!({"transform": {"class": "LogAffineTransform",
                "args": {"linSideSlope": [0.0, 1.0, 1.0]}}}),
            Some(("transform", "linear side slope cannot be 0")),
        ),
        (
            "a color space the raw config doesn't have",
            json!({"src": "raw", "dst": "nowhere"}),
            Some(("processor", "nowhere")),
        ),
        (
            "UINT14 with ops to optimize",
            with_depth(
                json!({"transform": {"class": "LogTransform"}}),
                "BIT_DEPTH_UINT14",
            ),
            Some(("optimize", "Bit depth is not supported: 14ui.")),
        ),
        (
            "UINT32 with ops to optimize",
            with_depth(
                json!({"transform": {"class": "LogTransform"}}),
                "BIT_DEPTH_UINT32",
            ),
            Some(("optimize", "Bit depth is not supported: 32ui.")),
        ),
        (
            "UINT14 without an op",
            with_depth(
                json!({"transform": {"class": "GroupTransform"}}),
                "BIT_DEPTH_UINT14",
            ),
            None,
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(_, args, _)| BatchCall {
            cmd: "processor_ops",
            args: args.clone(),
            blobs: Vec::new(),
        })
        .collect();
    for ((label, _, expected), response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply =
            ProcessorOpsReply::from_response(response.unwrap_or_else(|e| panic!("{label}: {e}")));
        match (expected, reply.raised()) {
            (Some((stage, fragment)), Some(raised)) => {
                assert_eq!(raised.kind, "Exception", "{label}: {raised:?}");
                assert_eq!(raised.stage, *stage, "{label}: {raised:?}");
                assert!(raised.message.contains(fragment), "{label}: {raised:?}");
                assert!(reply.optimized.is_none(), "{label}");
            }
            (None, None) => assert!(reply.optimized().group.children.is_empty(), "{label}"),
            (expected, raised) => panic!("{label}: expected {expected:?}, got {raised:?}"),
        }
    }
}

/// Requests the command refuses: keys and bit depths it doesn't know.
#[test]
fn unknown_keys_and_bit_depths_are_refused() {
    let cases = [
        (
            json!({"transform": {"class": "LogTransform"}, "optimisation": "OPTIMIZATION_NONE"}),
            "unknown keys ['optimisation']",
        ),
        (
            json!({"transform": {"class": "LogTransform"}, "in_bitdepth": "BIT_DEPTH_UINT9"}),
            "in_bitdepth: unknown BitDepth 'BIT_DEPTH_UINT9'",
        ),
        (
            json!({"transform": {"class": "LogTransform"}, "out_bitdepth": "name"}),
            "out_bitdepth: unknown BitDepth 'name'",
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(args, _)| BatchCall {
            cmd: "processor_ops",
            args: args.clone(),
            blobs: Vec::new(),
        })
        .collect();
    for ((args, fragment), result) in cases.iter().zip(Oracle::get().batch(&calls, false)) {
        let error = result.expect_err(&format!("{args}: refused"));
        assert!(error.contains(fragment), "{args}: {error}");
    }
}

/// A request's reply is the same on every run, alone or in a batch, blobs included.
#[test]
fn replies_are_the_same_on_every_run() {
    let mut lut = ProcessorOpsRequest::new(group(vec![
        json!({"class": "Lut1DTransform", "args": {"length": 5},
            "calls": [["setValue", 2, 0.3, 0.2, 0.1]]}),
        json!({"class": "GradingToneTransform", "args": {"style": {"enum": "GRADING_LOG"}},
            "calls": [["makeDynamic"]]}),
    ]));
    lut.in_bitdepth = Some(BitDepth::Uint10);
    let requests = [lut];
    let calls: Vec<BatchCall<'_>> = requests.iter().map(ProcessorOpsRequest::call).collect();
    let runs: Vec<Vec<Result<Response, String>>> =
        (0..2).map(|_| Oracle::get().batch(&calls, false)).collect();
    for (i, call) in calls.iter().enumerate() {
        let alone = Oracle::get().call_uncached(call.cmd, call.args.clone(), &call.blobs);
        assert!(alone.result.get("exception").is_none(), "{}", alone.result);
        for run in &runs {
            let batched = run[i].as_ref().unwrap_or_else(|e| panic!("call {i}: {e}"));
            assert_eq!(batched.result, alone.result, "call {i}");
            assert_eq!(batched.blobs, alone.blobs, "call {i}");
        }
    }
}
