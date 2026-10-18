// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `dynamic_apply` command (`oracle/ocio_oracle/dynamic_api.py`), against the
//! wheel itself: a value set on a dynamic property applies as the same value fixed in the
//! transform does (`cpu_apply`), a GPU description's uniform reads the value set on its
//! property, steps fail one at a time, setup failures stop before the steps, and bad requests
//! are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::{Response, f32_to_bytes};
use serde_json::{Value, json};

/// Pixels with a value of each sign and size, and an alpha.
fn pixels() -> Vec<u8> {
    f32_to_bytes(&[
        0.5, 0.25, 0.125, 1.0, -0.25, 2.0, 0.0, 0.5, 1.0e-3, 100.0, 0.75, 0.25,
    ])
}

/// One packed RGBA float image over request blob 0, of `pixels()`.
fn image_args() -> Value {
    json!({
        "buffers": [{"blob": 0}],
        "images": [{"kind": "packed", "data": {"buffer": 0}, "width": 3, "height": 1,
                    "num_channels": 4}],
    })
}

fn f64_spec(value: f64) -> Value {
    json!({"f64": value.to_bits()})
}

/// `base` with `extra`'s keys added.
fn with(base: Value, extra: Value) -> Value {
    let mut base = base;
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

fn dynamic_apply(args: Value) -> Response {
    Oracle::get().call("dynamic_apply", args, &[&pixels()])
}

/// The bytes `cpu_apply` gives for `transform` on `pixels()`.
fn fixed_apply(transform: Value) -> Vec<u8> {
    let response = Oracle::get().call("cpu_apply", json!({"transform": transform}), &[&pixels()]);
    assert!(
        response.result.get("exception").is_none(),
        "{}",
        response.result
    );
    response.blobs[0].clone()
}

fn steps(response: &Response) -> &Vec<Value> {
    response.result["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", response.result))
}

/// The bytes of the buffer an apply step returned.
fn applied<'a>(response: &'a Response, step: &Value) -> &'a [u8] {
    let index = step["result"]["buffers"][0]
        .as_u64()
        .unwrap_or_else(|| panic!("{step}"));
    &response.blobs[index as usize]
}

/// Each exposure set on a dynamic ExposureContrastTransform's CPU property applies as the
/// same exposure fixed in the transform, and reads back as it was set.
#[test]
fn a_set_exposure_applies_as_a_fixed_one() {
    let exposures = [1.0, -0.5, 2.5];
    let mut step_list = vec![json!({"get": "DYNAMIC_PROPERTY_EXPOSURE", "from": "cpu", "as": "e"})];
    for exposure in exposures {
        step_list.push(json!({"set": "e", "value": f64_spec(exposure)}));
        step_list.push(json!({"read": "e"}));
        // Each apply runs on the original pixels: a fresh image would be another buffer, so
        // apply from buffer 0 into buffer 1.
        step_list.push(json!({"apply": [0, 1]}));
    }
    let response = dynamic_apply(json!({
        "transform": {"class": "ExposureContrastTransform", "args": {"exposure": 0.0},
                      "calls": [["makeExposureDynamic"]]},
        "buffers": [{"blob": 0}, {"size": pixels().len(), "fill": [0]}],
        "images": [
            {"kind": "packed", "data": {"buffer": 0}, "width": 3, "height": 1, "num_channels": 4},
            {"kind": "packed", "data": {"buffer": 1}, "width": 3, "height": 1, "num_channels": 4},
        ],
        "steps": step_list,
    }));
    let s = steps(&response);
    assert_eq!(
        response.result["is_dynamic"]["cpu"],
        json!(true),
        "{}",
        response.result
    );
    assert_eq!(s[0]["result"]["has"], json!(true), "{}", response.result);
    for (i, exposure) in exposures.into_iter().enumerate() {
        let read = &s[2 + 3 * i];
        assert_eq!(read["result"]["value"], f64_spec(exposure), "{read}");
        let apply = &s[3 + 3 * i];
        // The apply step returns every buffer: the source, then the destination.
        let index = apply["result"]["buffers"][1].as_u64().unwrap() as usize;
        let expected = fixed_apply(json!({"class": "ExposureContrastTransform",
                                          "args": {"exposure": exposure}}));
        assert_eq!(response.blobs[index], expected, "exposure {exposure}");
    }
}

/// A grading primary value set on a dynamic GradingPrimaryTransform applies as the same value
/// fixed in the transform.
#[test]
fn a_set_grading_primary_applies_as_a_fixed_one() {
    let value = json!({"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}],
                       "attrs": [["saturation", f64_spec(1.3)], ["pivot", f64_spec(-0.1)]]});
    let response = dynamic_apply(with(
        image_args(),
        json!({
            "transform": {"class": "GradingPrimaryTransform",
                          "args": {"style": {"enum": "GRADING_LOG"}, "dynamic": true}},
            "steps": [
                {"get": "DYNAMIC_PROPERTY_GRADING_PRIMARY", "from": "cpu", "as": "p"},
                {"set": "p", "value": {"object": value}},
                {"apply": [0]},
            ],
        }),
    ));
    let s = steps(&response);
    assert_eq!(s[1]["result"], Value::Null, "{}", response.result);
    let expected = fixed_apply(json!({"class": "GradingPrimaryTransform",
                                      "args": {"values": {"object": value},
                                               "style": {"enum": "GRADING_LOG"}}}));
    assert_eq!(applied(&response, &s[2]), expected.as_slice());
}

/// The GPU description's own property: before a set, its uniform holds the value the property
/// reads; after, the value set.
#[test]
fn the_gpu_uniforms_read_the_value_set() {
    let response = dynamic_apply(json!({
        "transform": {"class": "ExposureContrastTransform", "args": {"exposure": 0.25},
                      "calls": [["makeExposureDynamic"]]},
        "gpu": {"shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}},
        "steps": [
            {"get": "DYNAMIC_PROPERTY_EXPOSURE", "from": "gpu", "as": "g"},
            {"uniforms": true},
            {"set": "g", "value": f64_spec(-3.0)},
            {"uniforms": true},
        ],
    }));
    let s = steps(&response);
    let initial = &s[0]["result"]["value"]["f64"];
    let doubles = |step: &Value| -> Vec<Value> {
        step["result"]
            .as_array()
            .unwrap_or_else(|| panic!("{step}"))
            .iter()
            .filter(|u| u["type"] == json!("UNIFORM_DOUBLE"))
            .map(|u| u["value"].clone())
            .collect()
    };
    assert!(doubles(&s[1]).contains(initial), "{}", response.result);
    assert!(
        doubles(&s[3]).contains(&json!((-3.0f64).to_bits())),
        "{}",
        response.result
    );
    assert!(!doubles(&s[3]).contains(initial), "{}", response.result);
    // The extraction's description is reported too.
    assert!(
        response.result["shader"]["text"].is_u64(),
        "{}",
        response.result
    );
}

/// A step OCIO refuses reports its exception, and the steps after it still run; a property
/// whose get failed is unbound for the steps that name it.
#[test]
fn steps_fail_one_at_a_time() {
    let primary = json!({"object": {"class": "GradingPrimary",
                                    "args": [{"enum": "GRADING_LOG"}]}});
    let response = dynamic_apply(with(
        image_args(),
        json!({
            "transform": {"class": "ExposureContrastTransform",
                          "calls": [["makeExposureDynamic"]]},
            "steps": [
                {"get": "DYNAMIC_PROPERTY_EXPOSURE", "from": "cpu", "as": "e"},
                {"set": "e", "call": "setGradingPrimary", "value": primary},
                {"get": "DYNAMIC_PROPERTY_CONTRAST", "from": "cpu", "as": "c"},
                {"read": "c"},
                {"set": "c", "value": f64_spec(1.0)},
                {"read": "e", "call": "getGradingPrimary"},
                {"read": "e"},
                {"apply": [0]},
            ],
        }),
    ));
    let s = steps(&response);
    assert!(
        s[1]["exception"]["message"].is_string(),
        "{}",
        response.result
    );
    assert!(
        s[2]["exception"]["message"].is_string(),
        "{}",
        response.result
    );
    assert_eq!(
        s[3]["result"],
        json!({"unbound": true}),
        "{}",
        response.result
    );
    assert_eq!(
        s[4]["result"],
        json!({"unbound": true}),
        "{}",
        response.result
    );
    assert!(
        s[5]["exception"]["message"].is_string(),
        "{}",
        response.result
    );
    assert_eq!(s[6]["result"]["type"], json!("DYNAMIC_PROPERTY_EXPOSURE"));
    // The failed set changed nothing: the apply is the transform's own.
    let expected = fixed_apply(json!({"class": "ExposureContrastTransform"}));
    assert_eq!(applied(&response, &s[7]), expected.as_slice());
}

/// A failure before the steps (here a color space the config doesn't have) is reported with
/// its stage, and no step runs.
#[test]
fn a_setup_failure_stops_before_the_steps() {
    let response = dynamic_apply(json!({
        "src": "nowhere", "dst": "nothing",
        "steps": [{"get": "DYNAMIC_PROPERTY_EXPOSURE", "from": "cpu", "as": "e"}],
    }));
    assert_eq!(
        response.result["stage"],
        json!("processor"),
        "{}",
        response.result
    );
    assert!(response.result["exception"]["message"].is_string());
    assert!(
        response.result.get("steps").is_none(),
        "{}",
        response.result
    );
}

/// Requests the command can't run exactly are refused.
#[test]
fn bad_requests_are_refused() {
    let transform = json!({"class": "ExposureContrastTransform",
                           "calls": [["makeExposureDynamic"]]});
    let get = json!({"get": "DYNAMIC_PROPERTY_EXPOSURE", "from": "cpu", "as": "e"});
    let bad = [
        json!({"transform": transform, "unknown": 1}),
        json!({"transform": transform, "steps": [{"jump": 1}]}),
        json!({"transform": transform, "steps": [{"get": "DYNAMIC_PROPERTY_EXPOSURE",
                                                  "read": "e", "from": "cpu", "as": "e"}]}),
        json!({"transform": transform, "steps": [{"read": "e"}]}),
        json!({"transform": transform, "steps": [{"read": "e"}, get]}),
        json!({"transform": transform, "steps": [{"get": "DYNAMIC_PROPERTY_NONE",
                                                  "from": "cpu", "as": "e"}]}),
        json!({"transform": transform, "steps": [{"get": "DYNAMIC_PROPERTY_EXPOSURE",
                                                  "from": "gpu", "as": "e"}]}),
        json!({"transform": transform, "steps": [{"get": "DYNAMIC_PROPERTY_EXPOSURE",
                                                  "from": "image", "as": "e"}]}),
        json!({"transform": transform, "steps": [get, {"set": "e"}]}),
        json!({"transform": transform, "steps": [get, {"set": "e", "call": "setFoo",
                                                       "value": f64_spec(1.0)}]}),
        json!({"transform": transform, "steps": [get, {"read": "e", "call": "getFoo"}]}),
        json!({"transform": transform, "steps": [{"uniforms": true}]}),
        json!({"transform": transform, "steps": [{"apply": [0]}]}),
        json!({"transform": transform, "gpu": {"shader": {}, "extra": 1}}),
        // An image reaching past its buffer: image_apply's checks.
        json!({"transform": transform, "buffers": [{"size": 16}],
               "images": [{"kind": "packed", "data": {"buffer": 0}, "width": 2,
                           "height": 1, "num_channels": 4}],
               "steps": [{"apply": [0]}]}),
    ];
    for args in bad {
        assert!(
            Oracle::get()
                .try_call("dynamic_apply", args.clone(), &[&pixels()])
                .is_err(),
            "{args}"
        );
    }
}
