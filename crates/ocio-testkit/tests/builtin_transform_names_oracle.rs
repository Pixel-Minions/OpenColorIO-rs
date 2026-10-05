// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `builtin_transform_names` command (`oracle/ocio_oracle/builtin_transforms.py`),
//! against the wheel itself: the registry comes back whole and in order, a style set on a
//! transform reports what the registry holds for it, an unknown style reports the exception,
//! and the requests it can't read are refused.

use ocio_testkit::Oracle;
use serde_json::{Value, json};

fn call(args: Value) -> Value {
    Oracle::get()
        .call("builtin_transform_names", args, &[])
        .result
}

/// Every entry has a style and a description, the count is the registry's length, and each
/// style set on a transform reports the registry's description for it.
#[test]
fn the_registry_in_order() {
    let first = call(json!({}));
    let builtins = first["builtins"].as_array().unwrap();
    assert_eq!(first["count"].as_u64().unwrap(), builtins.len() as u64);
    for entry in builtins {
        assert!(entry[0].is_string() && entry[1].is_string(), "{entry}");
    }
    let styles: Vec<Value> = builtins.iter().map(|e| e[0].clone()).collect();
    let second = call(json!({"styles": styles}));
    assert_eq!(second["builtins"], first["builtins"]);
    for (entry, set) in builtins.iter().zip(second["styles"].as_array().unwrap()) {
        assert_eq!(set["getStyle"], entry[0], "{set}");
        assert_eq!(set["getDescription"], entry[1], "{set}");
        assert!(set["repr"].is_string(), "{set}");
    }
}

/// A style the registry doesn't hold reports what setStyle raised; a message the binding can't
/// decode comes back as its bytes.
#[test]
fn an_unknown_style_raises() {
    let result = call(json!({"styles": ["not a style", {"bytes": "ff"}]}));
    let styles = result["styles"].as_array().unwrap();
    assert_eq!(styles[0]["exception"]["type"], "Exception", "{result}");
    assert_eq!(
        styles[1]["exception"]["type"], "UnicodeDecodeError",
        "{result}"
    );
    let bytes = styles[1]["exception"]["bytes"].as_str().unwrap();
    assert!(bytes.contains("27ff27"), "{result}");
}

/// Requests it can't read are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"style": []}),
        json!({"styles": "ACEScct_to_ACES2065-1"}),
        json!({"styles": [1]}),
    ] {
        assert!(
            Oracle::get()
                .try_call("builtin_transform_names", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
