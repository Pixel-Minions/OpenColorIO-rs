// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `config_calls` command (`oracle/ocio_oracle/config_api.py`), against the wheel
//! itself: the environment of a request is exactly the one it gives and the process's own comes
//! back after it; files land in the request's directory, which is the working directory;
//! strings come back as their bytes, those the binding can't decode included, element by
//! element; the log is OCIO's own, as bytes, per call; doubles and arrays come back exact;
//! exceptions are reported per call; the dump reaches every getter; and the requests it can't
//! run exactly are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, bytes_arg, exception, hex, log, result, result_bytes};
use serde_json::{Value, json};

/// A config of one color space, with no `environment:` section (so its context loads the whole
/// environment) and the extra lines given.
fn yaml(extra: &str) -> String {
    format!(
        "ocio_profile_version: 2\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    \
         name: raw\n{extra}"
    )
}

fn yaml_bytes(extra: &[u8]) -> Vec<u8> {
    [yaml("").as_bytes(), extra].concat()
}

fn call(args: Value) -> Value {
    Oracle::get().call("config_calls", args, &[]).result
}

/// The calls of a response whose config was made.
fn calls(response: &Value) -> &Vec<Value> {
    assert_eq!(response["config"], Value::Null, "{response}");
    response["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

/// The bytes of each `[name, value]` pair of a list of pairs.
fn pairs(list: &Value) -> Vec<(Vec<u8>, Vec<u8>)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), bytes(&p[1])))
        .collect()
}

/// The context of a config that loads the whole environment holds exactly the request's
/// variables: none of the oracle's own reach it.
#[test]
fn the_environment_is_exactly_the_requests() {
    let response = call(json!({
        "config": {"yaml": yaml("")},
        "env": {"OCIO_RS_PROBE_B": "2", "OCIO_RS_PROBE_A": "one"},
        "calls": [
            {"call": "getCurrentContext", "as": "context"},
            {"call": "getStringVars", "on": "context"},
            {"call": "getEnvironmentMode"},
            {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["PATH"]},
            {"call": "GetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_PROBE_A"]},
        ],
    }));
    let c = calls(&response);
    let mut vars = pairs(result(&c[1]));
    vars.sort();
    assert_eq!(
        vars,
        [
            (b"OCIO_RS_PROBE_A".to_vec(), b"one".to_vec()),
            (b"OCIO_RS_PROBE_B".to_vec(), b"2".to_vec())
        ]
    );
    assert_eq!(*result(&c[2]), json!({"enum": "ENV_ENVIRONMENT_LOAD_ALL"}));
    assert_eq!(*result(&c[3]), json!(false));
    assert_eq!(result_bytes(&c[4]), b"one");
}

/// In one oracle process, a request's variables are gone in the next request, and the
/// process's own are back once a request ends (the command checks that through OCIO and fails
/// otherwise): what a later command of a batch sees doesn't depend on the earlier ones.
#[test]
fn the_process_environment_comes_back() {
    let with = json!({"config": "raw", "env": {"OCIO_RS_PROBE": "x"}, "calls": []});
    let probe = json!({"config": "raw", "calls": [
        {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["OCIO_RS_PROBE"]},
    ]});
    let calls_ = [with.clone(), probe, with];
    let batch: Vec<BatchCall<'_>> = calls_
        .iter()
        .map(|a| BatchCall {
            cmd: "config_calls",
            args: a.clone(),
            blobs: vec![],
        })
        .collect();
    let out: Vec<_> = Oracle::get()
        .batch(&batch, false)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")))
        .collect();
    assert_eq!(*result(&calls(&out[1].result)[0]), json!(false));
}

/// Files land in a new directory, the request's working directory: a relative path finds
/// them, and the config's working directory is the file's directory under it.
#[test]
fn files_land_in_the_working_directory() {
    let response = call(json!({
        "config": {"file": "sub/config.ocio"},
        "files": {"sub/config.ocio": yaml("")},
        "calls": [{"call": "getWorkingDir"}],
    }));
    let dir = response["dir"].as_str().unwrap();
    let working = String::from_utf8(result_bytes(&calls(&response)[0])).unwrap();
    assert!(working.starts_with(dir), "{working} is not under {dir}");
    assert!(working.ends_with("sub"), "{working}");
}

/// Bytes go to the library as they are; a string the binding can't decode comes back as its
/// bytes, from a whole call (`undecodable`) and from each element of a list on its own.
#[test]
fn undecodable_strings_come_back_as_bytes() {
    let config = yaml_bytes(b"  - !<ColorSpace>\n    name: a\xffb\n    aliases: [x\xfey, ok]\n");
    let response = call(json!({
        "config": {"yaml": bytes_arg(&config)},
        "calls": [
            {"call": "getColorSpace", "args": ["raw"], "as": "cs"},
            {"call": "setFamily", "on": "cs", "args": [{"bytes": "61ff62"}]},
            {"call": "getFamily", "on": "cs"},
            {"call": "getColorSpaceNames", "args": [{"enum": "SEARCH_REFERENCE_SPACE_ALL"},
                                                     {"enum": "COLORSPACE_ALL"}]},
            {"call": "getColorSpace", "args": [{"bytes": "61ff62"}], "as": "odd"},
            {"call": "getAliases", "on": "odd"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(c[2]["undecodable"], "61ff62", "{response}");
    assert_eq!(
        *result(&c[3]),
        json!([{"bytes": hex(b"raw")}, {"undecodable": "61ff62"}])
    );
    assert!(
        c[4]["result"]["repr"]["undecodable"]
            .as_str()
            .unwrap()
            .contains("61ff62"),
        "{response}"
    );
    assert_eq!(
        *result(&c[5]),
        json!([{"undecodable": "78fe79"}, {"bytes": hex(b"ok")}])
    );
}

/// The log is OCIO's own messages as bytes, those that aren't UTF-8 included: a warning while
/// loading goes to config_log, one in a call to that call's log only.
#[test]
fn the_log_is_bytes_per_call() {
    let config = yaml_bytes(b"    b\xffd: 1\n");
    let response = call(json!({
        "config": {"yaml": bytes_arg(&config)},
        "calls": [
            {"call": "getName"},
            {"call": "CreateFromStream", "on": "Config", "args": [bytes_arg(&config)]},
            {"call": "getName"},
        ],
    }));
    let config_log = log(&response["config_log"]);
    assert_eq!(config_log.len(), 1, "{response}");
    assert!(
        config_log[0].starts_with(b"[OpenColorIO Warning]: "),
        "{response}"
    );
    assert!(
        config_log[0].windows(3).any(|w| w == b"b\xffd"),
        "{response}"
    );
    assert!(config_log[0].ends_with(b"\n"));
    let c = calls(&response);
    assert_eq!(log(&c[0]["log"]), Vec::<Vec<u8>>::new());
    assert_eq!(log(&c[1]["log"]), config_log);
    assert_eq!(log(&c[2]["log"]), Vec::<Vec<u8>>::new());
}

/// Doubles come back with all their bits (0.1 isn't a float), and arrays whole, as bytes.
#[test]
fn doubles_and_arrays_are_exact() {
    let tenth = 0.1f64.to_bits();
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"new": "MatrixTransform", "kwargs": {"offset": [{"f64": tenth}, 0, 0, 0]}, "as": "m"},
            {"call": "getOffset", "on": "m"},
            {"new": "Lut1DTransform", "args": [4096, false], "as": "lut"},
            {"call": "getData", "on": "lut"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(result(&c[1])[0], json!({"f64": tenth}));
    assert_ne!(f64::from(0.1f32).to_bits(), tenth);
    let array = &result(&c[3])["array"];
    assert_eq!(array["dtype"], "float32");
    assert_eq!(array["shape"], json!([4096 * 3]));
    assert_eq!(array["bytes"].as_str().unwrap().len(), 4096 * 3 * 4 * 2);
}

/// What the library raises is reported per call with its log, and nothing is stored for it,
/// a transform's constructor included; a source that raises reports the exception and runs no
/// call.
#[test]
fn exceptions_are_reported_per_call() {
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"call": "getProcessor", "args": ["nope", "raw"], "as": "p"},
            {"call": "getMajorVersion"},
            {"call": "getProcessor", "args": [
                {"transform": {"class": "LogTransform", "args": {"base": 1.0}}}]},
        ],
    }));
    let c = calls(&response);
    let (kind, message) = exception(&c[0]);
    assert_eq!(kind, "Exception");
    assert!(!message.is_empty());
    assert!(c[0]["log"].is_array());
    assert!(c[1]["result"].is_number());
    assert_eq!(exception(&c[2]).0, "Exception", "{response}");
    let failed = call(json!({"config": {"yaml": "not: a config"}, "calls": [{"dump": "config"}]}));
    assert_eq!(
        failed["config"]["exception"]["type"], "Exception",
        "{failed}"
    );
    assert_eq!(failed["calls"], json!([]));
}

/// The dump of a built-in config reaches every getter but the two that take what no config
/// holds (a file path, a processor's arguments), and lists them; its lookups find each color
/// space by name, alias and role.
#[test]
fn the_dump_reaches_every_getter() {
    let response = call(json!({
        "config": {"builtin": "studio-config-v4.0.0_aces-v2.0_ocio-v2.5"},
        "calls": [{"dump": "config"}],
    }));
    let dump = result(&calls(&response)[0]);
    assert_eq!(
        dump["not_dumped"],
        json!(["getColorSpaceFromFilepath", "getProcessor"])
    );
    for key in [
        "getters",
        "color_space_names",
        "color_spaces",
        "categories",
        "roles",
        "lookups",
        "environment",
        "displays",
        "shared_views",
        "virtual_display",
        "looks",
        "view_transforms",
        "default_scene_to_display",
        "named_transform_names",
        "named_transforms",
        "current_context",
        "file_rules",
        "viewing_rules",
        "serialize",
        "validate",
    ] {
        assert!(dump.get(key).is_some(), "the dump has no {key}");
    }
    let space = &dump["color_spaces"][0]["dump"];
    assert_eq!(space["class"], "ColorSpace");
    assert_eq!(space["uncalled"], json!([]));
    assert!(bytes(&dump["serialize"]).starts_with(b"ocio_profile_version"));
    // Each name, alias and role in the lookups, each found by getColorSpace.
    let lookups = dump["lookups"].as_array().unwrap();
    let found: Vec<(Vec<u8>, Vec<u8>)> = lookups
        .iter()
        .filter(|l| !l["getColorSpace"].is_null())
        .map(|l| (bytes(&l["name"]), bytes(&l["getColorSpace"])))
        .collect();
    assert!(found.contains(&(b"ACEScg".to_vec(), b"ACEScg".to_vec())));
    assert!(found.contains(&(b"lin_ap1".to_vec(), b"ACEScg".to_vec())));
    assert!(found.contains(&(b"scene_linear".to_vec(), b"ACEScg".to_vec())));
}

/// A config whose names the binding can't decode dumps whole: the names come back as their
/// bytes, and the getters that take them are called with those bytes.
#[test]
fn the_dump_reads_undecodable_names() {
    let config = yaml_bytes(b"  - !<ColorSpace>\n    name: a\xffb\n    aliases: [x\xfey]\n");
    let response =
        call(json!({"config": {"yaml": bytes_arg(&config)}, "calls": [{"dump": "config"}]}));
    let dump = result(&calls(&response)[0]);
    let spaces = dump["color_spaces"].as_array().unwrap();
    assert_eq!(bytes(&spaces[1]["name"]), b"a\xffb");
    assert_eq!(spaces[1]["isColorSpaceUsed"], json!(false), "{}", spaces[1]);
    let lookup = dump["lookups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| bytes(&l["name"]) == b"x\xfey")
        .unwrap();
    assert_eq!(lookup["getColorSpace"]["undecodable"], "61ff62", "{lookup}");
}

/// Objects made by a call are stored and reached by later calls, constructors and copies
/// included, with values of every kind.
#[test]
fn objects_are_stored_and_reached() {
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"new": "Context", "kwargs": {"stringVars": {"map": [["V", "x"]]},
                                          "environmentMode": {"enum": "ENV_ENVIRONMENT_LOAD_PREDEFINED"}},
             "as": "context"},
            {"copy": "context", "as": "copy"},
            {"call": "setStringVar", "on": "copy", "args": ["V", "y"]},
            {"call": "resolveStringVar", "on": "context", "args": ["$V"]},
            {"call": "resolveStringVar", "on": "copy", "args": ["${V}"]},
            {"new": "ColorSpace", "kwargs": {"name": "made"}, "as": "cs"},
            {"call": "setTransform", "on": "cs", "args": [
                {"transform": {"class": "MatrixTransform"}}, {"enum": "COLORSPACE_DIR_TO_REFERENCE"}]},
            {"call": "addColorSpace", "args": [{"ref": "cs"}]},
            {"call": "getColorSpace", "args": ["made"], "as": "found"},
            {"call": "getTransform", "on": "found", "args": [{"enum": "COLORSPACE_DIR_TO_REFERENCE"}]},
            {"call": "setDefaultLumaCoefs", "args": [[{"f64": 0}, {"f64": 0x3ff0_0000_0000_0000u64}, 0.5]]},
            {"call": "getDefaultLumaCoefs"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(result_bytes(&c[3]), b"x");
    assert_eq!(result_bytes(&c[4]), b"y");
    assert_eq!(result(&c[9])["class"], "MatrixTransform");
    assert!(bytes(&result(&c[9])["repr"]).starts_with(b"<MatrixTransform"));
    assert_eq!(
        *result(&c[11]),
        json!([{"f64": 0}, {"f64": 0x3ff0_0000_0000_0000u64}, {"f64": 0x3fe0_0000_0000_0000u64}])
    );
}

/// Requests it can't run exactly are refused: unknown keys and calls, methods it may not
/// reach, GroupTransform.write without a config (the process's current config would outlive
/// the request), the variables OCIO reads once per process (in any case on Windows), file paths
/// outside the directory, bad value specs.
#[test]
fn bad_requests_are_refused() {
    let mut cases = vec![
        json!({"config": "raw", "unknown": 1}),
        json!({"config": "nope"}),
        json!({"config": {"yaml": 1}}),
        json!({"config": "raw", "calls": [{"frobnicate": 1}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "extra": 1}]}),
        json!({"config": "raw", "calls": [{"call": "_pybind11_conduit_v1_"}]}),
        json!({"config": "raw", "calls": [{"call": "noSuchMethod"}]}),
        json!({"config": "raw", "calls": [{"call": "SetCurrentConfig", "on": "OCIO",
                                           "args": [{"ref": "config"}]}]}),
        json!({"config": "raw", "calls": [{"call": "GetCurrentConfig", "on": "OCIO"}]}),
        json!({"config": "raw", "calls": [{"new": "GroupTransform", "as": "g"},
                                          {"call": "write", "on": "g",
                                           "args": ["Color Transform Format"]}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "on": "missing"}]}),
        json!({"config": "raw", "calls": [{"call": "setName", "args": [1]}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "args": [{"what": 1}]}]}),
        json!({"config": "raw", "calls": [{"new": "Context", "kwargs": {"stringVars": {"map": "x"}}}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "args": [{"f64": -1}]}]}),
        json!({"config": "raw", "calls": [{"call": "getProcessor", "args": [
            {"transform": {"class": "NoSuchTransform"}}]}]}),
        json!({"config": "raw", "env": {"OCIO_LOGGING_LEVEL": "debug"}}),
        json!({"config": "raw", "env": {"OCIO_DISABLE_ALL_CACHES": "1"}}),
        json!({"config": "raw", "env": ["x"]}),
        json!({"config": "raw", "files": {"../escape.ocio": "x"}}),
        json!({"config": "raw", "files": {"/abs.ocio": "x"}}),
    ];
    if cfg!(windows) {
        cases.push(json!({"config": "raw", "env": {"ocio_logging_level": "debug"}}));
        cases.push(json!({"config": "raw", "files": {"a\\..\\b": "x"}}));
    }
    for args in cases {
        assert!(
            Oracle::get()
                .try_call("config_calls", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}

/// On Linux a backslash is a file name's byte, not a separator.
#[cfg(target_os = "linux")]
#[test]
fn a_backslash_is_a_name_on_linux() {
    let response = call(json!({
        "config": {"file": "a\\b.ocio"},
        "files": {"a\\b.ocio": yaml("")},
        "calls": [{"call": "getWorkingDir"}],
    }));
    assert_eq!(
        result_bytes(&calls(&response)[0]),
        response["dir"].as_str().unwrap().as_bytes()
    );
}

/// ColorSpaceSet's operators are reached (`__or__`, `__and__`, `__sub__`, `__eq__`,
/// `__ne__`): the sets of upstream's `OCIO_ADD_TEST(ColorSpaceSet, operations_on_set)`
/// (tests/cpu/ColorSpaceSet_tests.cpp:205-316 @ v2.5.2), with its counts and names.
#[test]
fn color_space_set_operators_are_reached() {
    let mut requests = vec![json!({"new": "Config", "as": "c"})];
    for (name, categories) in [
        ("cs1", &[][..]),
        ("cs2", &["linear", "rendering"][..]),
        ("cs3", &["log", "rendering"][..]),
    ] {
        requests.push(json!({"new": "ColorSpace", "as": name}));
        requests.push(json!({"call": "setName", "on": name, "args": [name]}));
        for category in categories {
            requests.push(json!({"call": "addCategory", "on": name, "args": [category]}));
        }
        requests.push(json!({"call": "addColorSpace", "on": "c", "args": [{"ref": name}]}));
    }
    let set = |name: &str, category: Value| json!({"call": "getColorSpaces", "on": "c", "args": [category], "as": name});
    requests.push(set("css1", Value::Null));
    requests.push(set("css2", json!("linear")));
    requests.push(set("css3", json!("log")));
    requests.push(set("css5", json!("rendering")));
    let op = |op: &str, a: &str, b: &str, store: &str| json!({"call": op, "on": a, "args": [{"ref": b}], "as": store});
    let first = requests.len();
    let ops = [
        op("__or__", "css2", "css3", "u23"),
        op("__or__", "css1", "css2", "u12"),
        op("__and__", "css2", "css3", "i23"),
        op("__and__", "css2", "css1", "i21"),
        op("__sub__", "css1", "css3", "d13"),
        op("__sub__", "css1", "css2", "d12"),
        op("__sub__", "css1", "u23", "d1u"),
        op("__sub__", "css1", "css5", "d15"),
        op("__and__", "d15", "u23", "nested"),
        op("__eq__", "css2", "i21", "eq"),
        op("__ne__", "css2", "i21", "ne"),
    ];
    requests.extend(ops);
    let named = [
        "u23", "u12", "i23", "i21", "d13", "d12", "d1u", "css5", "nested",
    ];
    for set in named {
        requests.push(json!({"call": "getColorSpaceNames", "on": set}));
    }
    let response = call(json!({"config": "raw", "calls": requests}));
    let c = calls(&response);
    let names = |k: usize| -> Vec<Vec<u8>> {
        result(&c[first + 11 + k])
            .as_array()
            .unwrap()
            .iter()
            .map(bytes)
            .collect()
    };
    for k in 0..9 {
        assert_eq!(result(&c[first + k])["class"], "ColorSpaceSet", "{k}");
    }
    assert_eq!(names(0).len(), 2);
    assert_eq!(names(1).len(), 3);
    assert_eq!(names(2).len(), 0);
    assert_eq!(names(3), [b"cs2".to_vec()]);
    assert_eq!(names(4), [b"cs1".to_vec(), b"cs2".to_vec()]);
    assert_eq!(names(5), [b"cs1".to_vec(), b"cs3".to_vec()]);
    assert_eq!(names(6), [b"cs1".to_vec()]);
    assert_eq!(names(7), [b"cs2".to_vec(), b"cs3".to_vec()]);
    assert_eq!(names(8).len(), 0);
    let eq = result(&c[first + 9]).as_bool().unwrap();
    assert_eq!(*result(&c[first + 10]), json!(!eq));
}
