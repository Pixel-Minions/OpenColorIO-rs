// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `object_calls` command (`oracle/ocio_oracle/object_api.py`), against the wheel
//! itself: value objects built from object specs reach the calls and come back exactly, the
//! helper modules' functions and the app helpers' classes run on the request's config, the
//! processors the calls make are written out as `processor_ops` writes them, in the order
//! asked, a constructor's exception stops before the calls, and bad requests are refused.

use ocio_testkit::Oracle;
use serde_json::{Value, json};

const STUDIO: &str = "studio-config-v2.1.0_aces-v1.3_ocio-v2.3";

fn call(args: Value) -> Value {
    Oracle::get().call("object_calls", args, &[]).result
}

fn calls(response: &Value) -> &Vec<Value> {
    assert_eq!(response["config"], Value::Null, "{response}");
    response["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

/// A GradingPrimary whose saturation has the bits `bits`.
fn primary(bits: u64) -> Value {
    json!({"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}],
           "attrs": [["saturation", {"f64": bits}]]})
}

/// A value object's fields come back exactly: two saturations a bit apart are told apart, in
/// the objects and in the calls' values; a B-spline's control points by their bits.
#[test]
fn value_objects_come_back_exactly() {
    let bits = 1.3f64.to_bits();
    // Values a C float holds exactly: the control points are floats.
    let points = [0.0, 0.0, 0.5, 0.625f64, 1.0, 1.0];
    let response = call(json!({
        "objects": {
            "a": primary(bits),
            "b": primary(bits + 1),
            "curve": {"class": "GradingBSplineCurve", "args": [points]},
        },
        "calls": [
            {"new": "GradingPrimaryTransform", "args": [{"ref": "b"}], "as": "t"},
            {"call": "getValue", "on": "t"},
            {"dump": "a"},
        ],
    }));
    let saturation = |v: &Value| v["properties"]["saturation"].clone();
    let objects = &response["objects"];
    assert_eq!(
        saturation(&objects["a"]["result"]),
        json!({"f64": bits}),
        "{response}"
    );
    assert_eq!(
        saturation(&objects["b"]["result"]),
        json!({"f64": bits + 1}),
        "{response}"
    );
    let c = calls(&response);
    assert_eq!(
        saturation(&c[1]["result"]),
        json!({"f64": bits + 1}),
        "{response}"
    );
    assert_eq!(c[2]["result"], objects["a"]["result"], "{response}");
    let control = objects["curve"]["result"]["getters"]["getControlPoints"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"));
    let written: Vec<Value> = control
        .iter()
        .flat_map(|p| [p["properties"]["x"].clone(), p["properties"]["y"].clone()])
        .collect();
    let given: Vec<Value> = points.iter().map(|v| json!({"f64": v.to_bits()})).collect();
    assert_eq!(written, given, "{response}");
}

/// The processors the calls store are written out as processor_ops writes the same processor,
/// unoptimized, in the order `processors` names them, each with its log; a processor a call
/// returns is written the same way.
#[test]
fn processors_are_written_as_processor_ops_writes_them() {
    // Two matrices, which the optimizer combines: the optimized processor differs.
    let matrices = json!({"class": "GroupTransform", "children": [
        {"class": "MatrixTransform", "args": {"offset": [0.25, 0.0, 0.0, 0.0]}},
        {"class": "MatrixTransform", "args": {"offset": [0.0, 0.5, 0.0, 0.0]}},
    ]});
    let exposure = json!({"class": "ExposureContrastTransform", "args": {"exposure": 0.5},
                          "calls": [["makeExposureDynamic"]]});
    let response = call(json!({
        "calls": [
            {"call": "getProcessor", "args": [{"transform": matrices}], "as": "m"},
            {"call": "getProcessor", "args": [{"transform": exposure}], "as": "e"},
        ],
        "processors": ["e", "m"],
    }));
    let ops = |transform: &Value| {
        Oracle::get()
            .call("processor_ops", json!({"transform": transform}), &[])
            .result
    };
    let (m, e) = (ops(&matrices), ops(&exposure));
    assert_ne!(m["processor"], m["optimized"], "{m}");
    let processors = &response["processors"];
    assert_eq!(processors[0]["processor"], e["processor"], "{response}");
    assert_eq!(processors[1]["processor"], m["processor"], "{response}");
    assert!(processors[1]["log"].is_array(), "{response}");
    let c = calls(&response);
    assert_eq!(c[0]["result"]["processor"], m["processor"], "{response}");
}

/// The helper modules' functions and the app helpers' classes run on the request's config: a
/// color space ColorSpaceHelpers adds is the config's, and the menu, the display view helpers,
/// the mixing manager (its encodings included) and its slider, and the legacy viewing pipeline
/// give their results.
#[test]
fn the_app_helpers_run_on_the_config() {
    let lut = "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n";
    let response = call(json!({
        "config": {"builtin": STUDIO},
        "files": {"lut.spi1d": lut},
        "calls": [
            {"call": "AddColorSpace", "on": "ColorSpaceHelpers",
             "kwargs": {"config": {"ref": "config"}, "name": "added",
                        "transformFilePath": "lut.spi1d", "categories": "file-io",
                        "connectionColorSpaceName": "ACEScg"}},
            {"call": "getColorSpace", "args": ["added"]},
            {"new": "ColorSpaceMenuParameters", "args": [{"ref": "config"}], "as": "params"},
            {"new": "ColorSpaceMenuHelper", "args": [{"ref": "params"}], "as": "menu"},
            {"call": "getNumColorSpaces", "on": "menu"},
            {"call": "getName", "on": "menu", "args": [0]},
            {"call": "GetIdentityProcessor", "on": "DisplayViewHelpers",
             "args": [{"ref": "config"}], "as": "identity"},
            {"new": "MixingColorSpaceManager", "args": [{"ref": "config"}], "as": "mixing"},
            {"call": "getSlider", "on": "mixing", "as": "slider"},
            {"call": "mixingToSlider", "on": "slider", "args": [{"f64": 0.25f64.to_bits()}]},
            {"new": "LegacyViewingPipeline", "as": "pipeline"},
            {"call": "getLooksOverrideEnabled", "on": "pipeline"},
        ],
        "processors": ["identity"],
    }));
    let c = calls(&response);
    for (i, entry) in c.iter().enumerate() {
        assert!(entry.get("exception").is_none(), "call {i}: {response}");
    }
    assert_eq!(c[1]["result"]["class"], json!("ColorSpace"), "{response}");
    assert!(c[4]["result"].as_u64().is_some_and(|n| n > 0), "{response}");
    assert!(
        c[5]["result"].as_str().is_some_and(|s| !s.is_empty()),
        "{response}"
    );
    let encodings = &c[7]["result"]["getters"]["getMixingEncodings"];
    assert!(
        encodings.as_array().is_some_and(|e| !e.is_empty()),
        "{response}"
    );
    // Every encoding, as the iterator's indices read them (iterating it raises, I-216).
    let indexed = call(json!({
        "config": {"builtin": STUDIO},
        "calls": [
            {"new": "MixingColorSpaceManager", "args": [{"ref": "config"}], "as": "mixing"},
            {"call": "getMixingEncodings", "on": "mixing", "as": "encodings"},
            {"call": "__len__", "on": "encodings"},
            {"call": "__getitem__", "on": "encodings", "args": [0]},
            {"call": "__getitem__", "on": "encodings", "args": [1]},
        ],
    }));
    let i = calls(&indexed);
    assert_eq!(i[2]["result"], json!(2), "{indexed}");
    assert_eq!(
        *encodings,
        json!([i[3]["result"], i[4]["result"]]),
        "{indexed}"
    );
    assert!(c[9]["result"]["f64"].is_u64(), "{response}");
    assert_eq!(c[11]["result"], json!(false), "{response}");
    assert!(
        response["processors"][0]["processor"]["cache_id"].is_string(),
        "{response}"
    );
}

/// Where the library raised: building a call's arguments, the call, or writing its value out
/// (a version 1 config with a view transform can't serialize); "as" still stores a value whose
/// write failed. A config is written by serialize(), and the request's objects before the
/// calls change them.
#[test]
fn stages_writes_and_order() {
    let bad_curve = json!({"object": {"class": "GradingBSplineCurve", "args": [[0.0, 0.0, 0.5]]}});
    let view_transform = json!({"referenceSpace": {"enum": "REFERENCE_SPACE_SCENE"},
                                "name": "vt",
                                "fromReference": {"transform": {"class": "MatrixTransform"}}});
    let response = call(json!({
        "objects": {"curve": {"class": "GradingBSplineCurve", "args": [[0.0, 0.0, 0.5, 0.5, 1.0, 1.0]]}},
        "calls": [
            {"dump": "config"},
            {"call": "serialize"},
            {"new": "GradingRGBCurveTransform", "args": [bad_curve]},
            {"call": "getProcessor", "args": ["nowhere", "nothing"]},
            {"call": "setMajorVersion", "args": [1]},
            {"new": "ViewTransform", "kwargs": view_transform, "as": "vt"},
            {"call": "addViewTransform", "args": [{"ref": "vt"}]},
            {"copy": "config", "as": "v1"},
            {"call": "getMajorVersion", "on": "v1"},
            {"call": "getMajorVersion"},
            {"call": "setSlopes", "on": "curve",
             "args": [[{"f64": 0.5f64.to_bits()}, {"f64": 0.25f64.to_bits()},
                       {"f64": 0.125f64.to_bits()}]]},
            {"dump": "curve"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(c[0]["result"]["class"], json!("Config"), "{response}");
    let serialized = c[1]["result"]
        .as_str()
        .unwrap_or_else(|| panic!("{response}"));
    let hex: String = serialized.bytes().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        c[0]["result"]["serialize"],
        json!({"bytes": hex}),
        "{response}"
    );
    assert_eq!(c[2]["stage"], json!("arguments"), "{response}");
    assert!(c[2]["exception"]["message"].is_object(), "{response}");
    assert_eq!(c[3]["stage"], json!("call"), "{response}");
    assert!(c[3]["exception"]["message"].is_object(), "{response}");
    assert_eq!(c[7]["stage"], json!("write"), "{response}");
    assert!(c[7]["exception"]["message"].is_object(), "{response}");
    assert_eq!(c[8]["result"], c[9]["result"], "{response}");
    // The object as it was made, before setSlopes.
    assert_ne!(
        response["objects"]["curve"]["result"], c[11]["result"],
        "{response}"
    );
}

/// OCIO's log goes where it was logged: loading the config to config_log, and the same load
/// in a call to that call's log, as config_calls reports them.
#[test]
fn the_log_goes_where_it_was_logged() {
    let yaml = "ocio_profile_version: 2

roles:
  default: raw

colorspaces:
  -                 !<ColorSpace>
    name: raw
    bogus: 1
";
    let args = json!({
        "config": {"yaml": yaml},
        "calls": [
            {"call": "getName"},
            {"call": "CreateFromStream", "on": "Config", "args": [yaml]},
        ],
    });
    let response = call(args.clone());
    let reference = Oracle::get().call("config_calls", args, &[]).result;
    let config_log = &response["config_log"];
    assert!(
        config_log.as_array().is_some_and(|l| !l.is_empty()),
        "{response}"
    );
    assert_eq!(*config_log, reference["config_log"], "{response}");
    let c = calls(&response);
    assert_eq!(c[0]["log"], json!([]), "{response}");
    assert_eq!(c[1]["log"], *config_log, "{response}");
}

/// An exception a value object's constructor raises is the config's: no call runs.
#[test]
fn a_constructor_exception_stops_before_the_calls() {
    let response = call(json!({
        "objects": {"curve": {"class": "GradingBSplineCurve", "args": [[0.0, 0.0, 0.5]]}},
        "calls": [{"call": "getCacheID"}],
    }));
    assert!(
        response["config"]["exception"]["message"].is_object(),
        "{response}"
    );
    assert_eq!(response["calls"], json!([]), "{response}");
}

/// A spec of `depth` groups, each holding the next.
fn nested_groups(depth: usize) -> Value {
    let mut spec = json!({"class": "GroupTransform"});
    for _ in 0..depth {
        spec = json!({"class": "GroupTransform", "children": [spec]});
    }
    spec
}

/// Requests the command can't run exactly are refused, with the oracle's own reason where it
/// gives one.
#[test]
fn bad_requests_are_refused() {
    let refused = |args: &Value| {
        Oracle::get()
            .try_call("object_calls", args.clone(), &[])
            .map(|r| r.result)
            .unwrap_err()
    };
    let bad = [
        json!({"unknown": 1}),
        json!({"processors": "p"}),
        json!({"processors": ["p"]}),
        json!({"objects": {"config": primary(0)}}),
        json!({"objects": {"x": {"class": "NoSuchClass"}}}),
        json!({"objects": {"x": {"class": "ExposureContrastTransform"}}}),
        json!({"objects": ["x"]}),
        // Malformed specs: whatever Python raises reading them refuses the request.
        json!({"objects": {"x": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}],
                                 "calls": [[]]}}}),
        json!({"objects": {"x": {"class": "GradingPrimary", "factory": []}}}),
        json!({"calls": [{"call": "getProcessor",
                          "args": [{"object": {"class": "GradingPrimary", "calls": [[]]}}]}]}),
        json!({"calls": [{"call": "NoSuchFunction", "on": "DisplayViewHelpers"}]}),
        json!({"calls": [{"dump": "nothing"}]}),
        // Malformed transform specs in arguments, an object nested in one, and a group too deep
        // for Python's recursion (1,000 levels: JSON reads them, the spec's builder can't): the
        // request's errors, not the library's.
        json!({"calls": [{"call": "getProcessor",
                          "args": [{"transform": {"class": "MatrixTransform", "calls": [[]]}}]}]}),
        json!({"calls": [{"call": "getProcessor", "args": [{"transform": {
            "class": "GradingPrimaryTransform",
            "args": [{"object": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}],
                                 "calls": [[]]}}]}}]}]}),
        json!({"calls": [{"new": "GroupTransform", "args": [[{"transform": {
            "class": "MatrixTransform", "calls": [[]]}}]]}]}),
        json!({"calls": [{"call": "getProcessor", "args": [{"transform": nested_groups(1000)}]}]}),
        // No overload takes the arguments (pybind11's TypeError).
        json!({"calls": [{"call": "getProcessor", "args": [1, 2, 3, 4, 5]}]}),
        // A value too deep for checks.dump to write out: the oracle's own error while writing.
        json!({"calls": [{"new": "GroupTransform", "args": [[{"transform": nested_groups(10)}]]}]}),
    ];
    for args in &bad {
        refused(args);
    }
    // A stored object that isn't a Processor.
    let error = refused(&json!({
        "objects": {"p": primary(0)},
        "processors": ["p"],
    }));
    assert!(error.contains("isn't a Processor"), "{error}");
}
