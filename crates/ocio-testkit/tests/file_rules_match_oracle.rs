// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `file_rules_match` command (`oracle/ocio_oracle/file_rules_api.py`), against
//! the wheel itself: rules are inserted in order before the default rule, refused ones report
//! the library's message, each path reports the rule that matched it consistently with
//! filepathOnlyMatchesDefaultRule, an isolated case runs in a process of its own and reports
//! what the same case reports in the oracle's process, the rules are set on the request's
//! config, and the requests it can't read exactly are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, exception};
use serde_json::{Value, json};

fn cases(args: Value) -> Vec<Value> {
    let response = Oracle::get().call("file_rules_match", args, &[]);
    response.result["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", response.result))
        .clone()
}

fn case() -> Value {
    json!({
        "rules": [
            {"name": "glob", "colorspace": "cs_glob", "pattern": "*", "extension": "exr"},
            {"name": "broken", "colorspace": "x", "regex": "a(b"},
            {"name": "regex", "colorspace": "cs_regex", "regex": ".*_[0-9]+\\.dpx$"},
            {"path_search": true},
        ],
        "default": "cs_default",
        "paths": ["shot.exr", "plate_0012.dpx", "image.png", {"bytes": "ff2e657872"}],
    })
}

/// Rules land in order before the default rule; the refused one reports the library's message
/// and is left out; each path's rule and color space are reported, its rule index agreeing with
/// filepathOnlyMatchesDefaultRule.
#[test]
fn rules_and_matches() {
    let all = cases(json!({"cases": [case()]}));
    let result = &all[0];
    let inserted = result["inserted"].as_array().unwrap();
    assert_eq!(inserted[0], Value::Null);
    let (kind, message) = exception(&inserted[1]);
    assert_eq!(kind, "Exception", "{result}");
    assert!(
        message.starts_with(b"File rules: invalid regular expression 'a(b'"),
        "{result}"
    );
    assert_eq!(inserted[2], Value::Null);
    assert_eq!(inserted[3], Value::Null);
    assert_eq!(result["default"], Value::Null);
    let rules = &result["file_rules"];
    assert_eq!(rules["getters"]["getNumEntries"], 4, "{rules}");
    let names: Vec<Vec<u8>> = rules["keyed"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k[0] == "getName")
        .map(|k| bytes(&k[2]))
        .collect();
    assert_eq!(
        names,
        [
            b"glob".to_vec(),
            b"regex".to_vec(),
            b"ColorSpaceNamePathSearch".to_vec(),
            b"Default".to_vec()
        ]
    );
    let paths = result["paths"].as_array().unwrap();
    let rule_of = |i: usize| paths[i]["rule"].as_u64().unwrap();
    assert_eq!(
        [rule_of(0), rule_of(1), rule_of(2), rule_of(3)],
        [0, 1, 3, 0],
        "{result}"
    );
    assert_eq!(bytes(&paths[0]["colorspace"]), b"cs_glob");
    assert_eq!(bytes(&paths[1]["colorspace"]), b"cs_regex");
    assert_eq!(bytes(&paths[2]["colorspace"]), b"cs_default");
    for path in paths {
        assert_eq!(
            path["only_default"],
            json!(path["rule"].as_u64().unwrap() == 3),
            "{path}"
        );
    }
}

/// An isolated case runs in a process of its own and reports what the oracle's own process
/// reports.
#[test]
fn an_isolated_case_reports_the_same() {
    let mut isolated = case();
    isolated["isolated"] = json!(true);
    let all = cases(json!({"cases": [case(), isolated]}));
    assert_ne!(all[0]["pid"], all[1]["pid"]);
    let without_pid = |v: &Value| {
        let mut v = v.clone();
        v.as_object_mut().unwrap().remove("pid");
        v
    };
    assert_eq!(without_pid(&all[0]), without_pid(&all[1]));
}

/// The rules are set on the config the request gives, whose color space names the path
/// search finds.
#[test]
fn the_path_search_uses_the_config() {
    let yaml = "ocio_profile_version: 2\nroles:\n  default: lin\ncolorspaces:\n  \
                - !<ColorSpace>\n    name: lin\n  - !<ColorSpace>\n    name: log\n";
    let all = cases(json!({
        "config": {"yaml": yaml},
        "cases": [{"rules": [{"path_search": true}], "paths": ["plate_log.dpx", "plate.dpx"]}],
    }));
    let paths = all[0]["paths"].as_array().unwrap();
    assert_eq!(paths[0]["rule"], 0, "{paths:?}");
    assert_eq!(bytes(&paths[0]["colorspace"]), b"log");
    assert_eq!(paths[1]["rule"], 1, "{paths:?}");
}

/// Requests it can't read exactly are refused, isolated ones too.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"cases": [{"rule": []}]}),
        json!({"cases": [{"rules": [{"name": "a", "colorspace": "b"}]}]}),
        json!({"cases": [{"rules": [{"name": "a", "colorspace": "b", "regex": "x",
                                     "pattern": "*"}]}]}),
        json!({"cases": [{"rules": [{"path_search": 1}]}]}),
        json!({"cases": [{"paths": [1]}]}),
        json!({"cases": [{"paths": "x"}]}),
        json!({"cases": [{"isolated": "yes"}]}),
        json!({"cases": [{"paths": [1], "isolated": true}]}),
        json!({"config": {"file": 1}, "cases": [{"isolated": true}]}),
        json!({"config": "nope", "cases": []}),
        json!({"cases": {}}),
        json!({"cases": [], "other": 1}),
    ] {
        assert!(
            Oracle::get()
                .try_call("file_rules_match", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
