// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `object_calls` command (`oracle/ocio_oracle/object_api.py`), against the wheel
//! itself: value objects built from object specs reach the calls, the helper modules' functions
//! and the app helpers' classes run on the request's config, the processors the calls make are
//! written out as `processor_ops` writes them, a constructor's exception stops before the
//! calls, and bad requests are refused.

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

/// The text of a `{"bytes": hex}` result.
fn text(value: &Value) -> String {
    let hex = value["bytes"].as_str().unwrap_or_else(|| panic!("{value}"));
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

/// A value object's attributes, set from its spec, reach the calls: its repr() differs from a
/// default one's, and a dynamic grading transform made with it holds it.
#[test]
fn value_objects_reach_the_calls() {
    let response = call(json!({
        "objects": {
            "set": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}],
                    "attrs": [["saturation", {"f64": 1.3f64.to_bits()}]]},
            "default": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}]},
        },
        "calls": [
            {"call": "__repr__", "on": "set"},
            {"call": "__repr__", "on": "default"},
            {"new": "GradingPrimaryTransform", "args": [{"ref": "set"}], "as": "t"},
            {"call": "getValue", "on": "t", "as": "value"},
            {"call": "__repr__", "on": "value"},
        ],
    }));
    let c = calls(&response);
    let set = text(&c[0]["result"]);
    assert_ne!(set, text(&c[1]["result"]), "{response}");
    assert_eq!(text(&c[4]["result"]), set, "{response}");
}

/// A processor a call stores is written out as processor_ops writes the same processor.
#[test]
fn processors_are_written_as_processor_ops_writes_them() {
    let transform = json!({"class": "ExposureContrastTransform", "args": {"exposure": 0.5},
                           "calls": [["makeExposureDynamic"]]});
    let response = call(json!({
        "calls": [{"call": "getProcessor", "args": [{"transform": transform}], "as": "p"}],
        "processors": ["p"],
    }));
    let ops = Oracle::get()
        .call("processor_ops", json!({"transform": transform}), &[])
        .result;
    assert_eq!(
        response["processors"][0], ops["processor"],
        "{response}\n{ops}"
    );
}

/// The helper modules' functions and the app helpers' classes run on the request's config: a
/// color space ColorSpaceHelpers adds is the config's, and the menu, the display view helpers,
/// the mixing manager and its slider, and the legacy viewing pipeline give their results.
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
    assert!(!text(&c[5]["result"]).is_empty(), "{response}");
    assert!(c[9]["result"]["f64"].is_u64(), "{response}");
    assert_eq!(c[11]["result"], json!(false), "{response}");
    assert!(
        response["processors"][0]["cache_id"].is_string(),
        "{response}"
    );
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

/// Requests the command can't run exactly are refused.
#[test]
fn bad_requests_are_refused() {
    let bad = [
        json!({"unknown": 1}),
        json!({"processors": "p"}),
        json!({"processors": ["p"]}),
        json!({"calls": [{"call": "getCacheID", "as": "id"}], "processors": ["id"]}),
        json!({"objects": {"config": {"class": "GradingPrimary",
                                      "args": [{"enum": "GRADING_LOG"}]}}}),
        json!({"objects": {"x": {"class": "NoSuchClass"}}}),
        json!({"objects": {"x": {"class": "ExposureContrastTransform"}}}),
        json!({"objects": ["x"]}),
        json!({"calls": [{"call": "NoSuchFunction", "on": "DisplayViewHelpers"}]}),
    ];
    for args in bad {
        assert!(
            Oracle::get()
                .try_call("object_calls", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
