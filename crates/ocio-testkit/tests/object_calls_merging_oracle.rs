// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What chunk O9.1 adds to the oracle's `object_calls` (`oracle/ocio_oracle/object_api.py`,
//! `config_api.py`, `spec.py`), against the wheel itself: the `ConfigMergingHelpers`
//! submodule's functions (`MergeConfigs`, `MergeColorSpace`) run on the request's configs, and
//! an enum value only a class exports (`ConfigMergingParameters.STRATEGY_PREFER_INPUT`: the
//! module doesn't export `MergeStrategies`' values) is named with its class, in a call's
//! arguments, in an object spec and in `config_calls`; names that aren't such a member are
//! refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

/// `MergeStrategies`' values, as the binding names them (`PyMergeConfigs.cpp` @ v2.5.2).
const STRATEGIES: [&str; 6] = [
    "STRATEGY_PREFER_INPUT",
    "STRATEGY_PREFER_BASE",
    "STRATEGY_INPUT_ONLY",
    "STRATEGY_BASE_ONLY",
    "STRATEGY_REMOVE",
    "STRATEGY_UNSPECIFIED",
];

/// A version 2 config with one scene-referred color space, `name`, its reference role and
/// default role.
fn config_yaml(name: &str) -> String {
    format!(
        "ocio_profile_version: 2\n\nroles:\n  default: {name}\n  scene_linear: {name}\n  \
         aces_interchange: {name}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: {name}\n    \
         isdata: false\n"
    )
}

/// The input config: `config_yaml("input_space")` and a color space `input_offset`, an offset
/// from the reference, which no color space of the base is equivalent to.
fn input_yaml() -> String {
    format!(
        "{}  - !<ColorSpace>\n    name: input_offset\n    isdata: false\n    \
         to_scene_reference: !<MatrixTransform> {{offset: [0.25, 0, 0, 0]}}\n",
        config_yaml("input_space")
    )
}

fn call(args: Value) -> Value {
    Oracle::get().call("object_calls", args, &[]).result
}

fn calls(response: &Value) -> &Vec<Value> {
    assert_eq!(response["config"], Value::Null, "{response}");
    response["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

/// The hex of `bytes`, as the oracle writes strings.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `ConfigMergingHelpers.MergeConfigs` and `MergeColorSpace` run on the request's configs: the
/// merged config holds the input's color space that no base space is equivalent to (and the
/// base's beside the added one), it is written by `serialize()` as the same config's own
/// `serialize` call returns it, its warnings are its call's log, and a merge the library
/// refuses is reported at stage "call", storing nothing under "as".
#[test]
fn the_merging_helpers_run_on_the_configs() {
    let offset = json!({"transform": {"class": "MatrixTransform",
                                      "args": {"offset": [0.25, 0.0, 0.0, 0.0]}}});
    let response = call(json!({
        "config": {"yaml": config_yaml("base_space")},
        "calls": [
            {"call": "Create", "on": "ConfigMergingParameters", "as": "params"},
            {"call": "CreateFromStream", "on": "Config", "args": [input_yaml()],
             "as": "input"},
            {"call": "MergeConfigs", "on": "ConfigMergingHelpers",
             "args": [{"ref": "params"}, {"ref": "config"}, {"ref": "input"}], "as": "merged"},
            {"call": "getColorSpaceNames", "on": "merged",
             "args": [{"enum": "SEARCH_REFERENCE_SPACE_ALL"}, {"enum": "COLORSPACE_ALL"}]},
            {"call": "serialize", "on": "merged"},
            {"new": "ColorSpace", "kwargs": {"name": "added_space", "toReference": offset},
             "as": "cs"},
            {"call": "MergeColorSpace", "on": "ConfigMergingHelpers",
             "kwargs": {"params": {"ref": "params"}, "baseConfig": {"ref": "config"},
                        "colorspace": {"ref": "cs"}},
             "as": "with_cs"},
            {"call": "getColorSpaceNames", "on": "with_cs",
             "args": [{"enum": "SEARCH_REFERENCE_SPACE_ALL"}, {"enum": "COLORSPACE_ALL"}]},
        ],
    }));
    let c = calls(&response);
    for (i, entry) in c.iter().enumerate() {
        assert!(entry.get("result").is_some(), "call {i}: {response}");
    }
    assert_eq!(c[2]["result"]["class"], json!("Config"), "{response}");
    // The merge's warnings (the input's roles override the base's) are its call's log.
    assert!(
        c[2]["log"].as_array().is_some_and(|l| !l.is_empty()),
        "{response}"
    );
    let names = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap_or_else(|| panic!("{response}"))
            .iter()
            .map(|n| {
                n.as_str()
                    .unwrap_or_else(|| panic!("{response}"))
                    .to_owned()
            })
            .collect()
    };
    let merged = names(&c[3]["result"]);
    assert!(merged.contains(&"input_offset".to_owned()), "{response}");
    let serialized = c[4]["result"]
        .as_str()
        .unwrap_or_else(|| panic!("{response}"));
    assert_eq!(
        c[2]["result"]["serialize"],
        json!({"bytes": hex(serialized.as_bytes())}),
        "{response}"
    );
    assert_eq!(c[6]["result"]["class"], json!("Config"), "{response}");
    let with_cs = names(&c[7]["result"]);
    assert!(
        with_cs.contains(&"base_space".to_owned()) && with_cs.contains(&"added_space".to_owned()),
        "{response}"
    );

    // A merge the library refuses (a base config without a color space for the reference: the
    // raw config has no reference roles), at stage "call"; "as" stores nothing, so a call on
    // it is refused.
    let refused = json!({
        "config": {"builtin": "cg-config-v2.1.0_aces-v1.3_ocio-v2.3"},
        "calls": [
            {"call": "Create", "on": "ConfigMergingParameters", "as": "params"},
            {"call": "CreateRaw", "on": "Config", "as": "raw"},
            {"call": "MergeConfigs", "on": "ConfigMergingHelpers",
             "args": [{"ref": "params"}, {"ref": "config"}, {"ref": "raw"}], "as": "merged"},
        ],
    });
    let response = call(refused.clone());
    let c = calls(&response);
    assert_eq!(c[2]["stage"], json!("call"), "{response}");
    assert_eq!(c[2]["exception"]["type"], json!("Exception"), "{response}");
    assert!(c[2].get("result").is_none(), "{response}");
    let mut stored = refused;
    stored["calls"]
        .as_array_mut()
        .expect("the calls")
        .push(json!({"call": "getName", "on": "merged"}));
    let error = Oracle::get()
        .try_call("object_calls", stored, &[])
        .map(|r| r.result)
        .unwrap_err();
    assert!(error.contains("no object is stored as 'merged'"), "{error}");
}

/// Each of `MergeStrategies`' values, named with its class, reaches the library as that value:
/// set as the default strategy then read back, in a call's arguments, in an object spec's
/// calls (`spec.enum_member`) and through `config_calls` (`value_in`, shared). A value of the
/// module (`TRANSFORM_DIR_INVERSE`) is still named alone. Values come back by their short name
/// (`value_out` and `checks.dump` write `{"enum": "STRATEGY_REMOVE"}`), while a request names a
/// class's value with its class.
#[test]
fn enum_values_named_with_their_class() {
    for name in STRATEGIES {
        let qualified = format!("ConfigMergingParameters.{name}");
        let response = call(json!({
            "objects": {"spec_params": {
                "class": "ConfigMergingParameters", "factory": ["Create"],
                "calls": [["setDefaultStrategy", {"enum": qualified}]]}},
            "calls": [
                {"call": "Create", "on": "ConfigMergingParameters", "as": "params"},
                {"call": "setDefaultStrategy", "on": "params", "args": [{"enum": qualified}]},
                {"call": "getDefaultStrategy", "on": "params"},
                {"call": "getDefaultStrategy", "on": "spec_params"},
                {"new": "LogTransform",
                 "kwargs": {"direction": {"enum": "TRANSFORM_DIR_INVERSE"}}, "as": "log"},
                {"call": "getDirection", "on": "log"},
            ],
        }));
        let c = calls(&response);
        assert_eq!(c[2]["result"], json!({"enum": name}), "{response}");
        assert_eq!(c[3]["result"], json!({"enum": name}), "{response}");
        assert_eq!(
            response["objects"]["spec_params"]["result"]["getters"]["getDefaultStrategy"],
            json!({"enum": name}),
            "{response}"
        );
        assert_eq!(
            c[5]["result"],
            json!({"enum": "TRANSFORM_DIR_INVERSE"}),
            "{response}"
        );

        let response = Oracle::get()
            .call(
                "config_calls",
                json!({"config": "raw", "calls": [
                    {"call": "Create", "on": "ConfigMergingParameters", "as": "params"},
                    {"call": "setDefaultStrategy", "on": "params", "args": [{"enum": qualified}]},
                    {"call": "getDefaultStrategy", "on": "params"},
                ]}),
                &[],
            )
            .result;
        assert_eq!(
            response["calls"][2]["result"],
            json!({"enum": name}),
            "{response}"
        );
    }
}

/// Names that aren't an enum member a class of the binding exports are refused, in a call's
/// arguments (`value_in`), in an object spec (`spec.enum_member`) and in `config_calls`: the
/// unqualified name of a value the module doesn't export, the nested enum's type, a static
/// method, dunders, a class without that value (a module value qualified by a class that
/// doesn't export it: no fallback to the module), a class that isn't the binding's (Python's
/// `Exception`, the module's type), a submodule, a member named through another member, a
/// path of three names, and empty parts. The submodule's names can't be taken by a request's
/// object or a call's "as", and it has no function but its own.
#[test]
fn bad_enum_names_and_submodule_uses_are_refused() {
    let names = [
        "STRATEGY_PREFER_INPUT",
        "ConfigMergingParameters.MergeStrategies",
        "ConfigMergingParameters.Create",
        "ConfigMergingParameters.__class__",
        "ConfigMergingParameters.__init__",
        "Config.STRATEGY_PREFER_INPUT",
        "Config.TRANSFORM_DIR_FORWARD",
        "NoSuchClass.STRATEGY_PREFER_INPUT",
        "Exception.STRATEGY_PREFER_INPUT",
        "Exception.args",
        "__class__.__name__",
        "ConfigMergingHelpers.MergeConfigs",
        "TRANSFORM_DIR_FORWARD.TRANSFORM_DIR_INVERSE",
        "ConfigMergingParameters.MergeStrategies.STRATEGY_REMOVE",
        "ConfigMergingParameters.STRATEGY_REMOVE.name",
        "ConfigMergingParameters.",
        ".STRATEGY_REMOVE",
        ".",
    ];
    let mut requests: Vec<(&str, Value, &str)> = Vec::new();
    for name in names {
        let value = json!({"enum": name});
        requests.push((
            "object_calls",
            json!({"calls": [
                {"call": "Create", "on": "ConfigMergingParameters", "as": "params"},
                {"call": "setDefaultStrategy", "on": "params", "args": [value]},
            ]}),
            "unknown enum value",
        ));
        requests.push((
            "object_calls",
            json!({"objects": {"p": {
                "class": "ConfigMergingParameters", "factory": ["Create"],
                "calls": [["setDefaultStrategy", value]]}}}),
            "isn't a member of a PyOpenColorIO enum",
        ));
        requests.push((
            "config_calls",
            json!({"config": "raw", "calls": [
                {"call": "Create", "on": "ConfigMergingParameters", "as": "params"},
                {"call": "setDefaultStrategy", "on": "params", "args": [value]},
            ]}),
            "unknown enum value",
        ));
    }
    let submodule = [
        (
            json!({"objects": {"ConfigMergingHelpers": {
                "class": "ConfigMergingParameters", "factory": ["Create"]}}}),
            "without the names",
        ),
        (
            json!({"calls": [{"call": "getName", "as": "ConfigMergingHelpers"}]}),
            "can't store as",
        ),
        (
            json!({"calls": [{"call": "NoSuchFunction", "on": "ConfigMergingHelpers"}]}),
            "has no method",
        ),
    ];
    for (args, reason) in submodule {
        requests.push(("object_calls", args, reason));
    }
    let batch: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|(cmd, args, _)| BatchCall {
            cmd,
            args: args.clone(),
            blobs: vec![],
        })
        .collect();
    let replies = Oracle::get().batch(&batch, true);
    assert_eq!(replies.len(), requests.len());
    for ((cmd, args, reason), reply) in requests.iter().zip(&replies) {
        match reply {
            Ok(r) => panic!("{cmd} {args} was run: {}", r.result),
            Err(error) => assert!(error.contains(reason), "{cmd} {args}: {error}"),
        }
    }
}
