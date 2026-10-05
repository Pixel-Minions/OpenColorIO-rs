// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `builtin_transform_names` command (`oracle/ocio_oracle/builtin_transforms.py`),
//! against the wheel itself and upstream's tests: the registry comes back whole and in order,
//! a style set on a transform reports what the registry holds for it, an unknown style reports
//! the exception, and the requests it can't read are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, exception};
use ocio_testkit::paths::upstream_dir;
use serde_json::{Value, json};

fn call(args: Value) -> Value {
    Oracle::get()
        .call("builtin_transform_names", args, &[])
        .result
}

/// The styles of `UnitTestValues` (tests/cpu/transforms/BuiltinTransform_tests.cpp @ v2.5.2),
/// upstream's table of every built-in transform, which its test `validate` checks against the
/// registry's count (line 766).
fn upstream_styles() -> Vec<String> {
    let path = upstream_dir().join("tests/cpu/transforms/BuiltinTransform_tests.cpp");
    let source = std::fs::read_to_string(path).unwrap();
    let start = source.find("AllValues UnitTestValues").unwrap();
    let end = start + source[start..].find("\n};").unwrap();
    source[start..end]
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("{ \""))
        .map(|l| l[..l.find('"').unwrap()].to_string())
        .collect()
}

/// The registry in order: its first two entries are those upstream's test `access` pins
/// (BuiltinTransform_tests.cpp:52-64), and its styles are upstream's table's, as many; each
/// style set on a transform reports the registry's description for it.
#[test]
fn the_registry_in_order() {
    let first = call(json!({}));
    let builtins = first["builtins"].as_array().unwrap();
    let entries: Vec<(Vec<u8>, Vec<u8>)> = builtins
        .iter()
        .map(|e| (bytes(&e[0]), bytes(&e[1])))
        .collect();
    // Port of the checks of `OCIO_ADD_TEST(BuiltinTransform, access)` @ v2.5.2.
    assert_eq!(entries[0].0, b"IDENTITY");
    assert_eq!(entries[1].0, b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD");
    assert_eq!(
        entries[1].1,
        b"Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation"
    );
    let mut upstream = upstream_styles();
    assert_eq!(first["count"].as_u64().unwrap(), upstream.len() as u64);
    assert_eq!(builtins.len(), upstream.len());
    let mut styles: Vec<String> = entries
        .iter()
        .map(|(s, _)| String::from_utf8(s.clone()).unwrap())
        .collect();
    styles.sort();
    upstream.sort();
    assert_eq!(styles, upstream);

    let given: Vec<Value> = builtins.iter().map(|e| e[0].clone()).collect();
    let second = call(json!({"styles": given}));
    assert_eq!(second["builtins"], first["builtins"]);
    for ((style, description), set) in entries.iter().zip(second["styles"].as_array().unwrap()) {
        assert_eq!(bytes(&set["getStyle"]), *style, "{set}");
        assert_eq!(bytes(&set["getDescription"]), *description, "{set}");
        assert!(
            bytes(&set["repr"]).starts_with(b"<BuiltinTransform"),
            "{set}"
        );
    }
}

/// A style the registry doesn't hold reports what setStyle raised; a message the binding can't
/// decode comes back as its bytes.
#[test]
fn an_unknown_style_raises() {
    let result = call(json!({"styles": ["not a style", {"bytes": "ff"}]}));
    let styles = result["styles"].as_array().unwrap();
    assert_eq!(exception(&styles[0]).0, "Exception", "{result}");
    let bytes = styles[1]["undecodable"].as_str().unwrap();
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
