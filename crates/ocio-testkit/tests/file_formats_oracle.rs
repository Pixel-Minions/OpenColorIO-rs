// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `file_formats` command (`oracle/ocio_oracle/file_formats_api.py`), against the
//! wheel and upstream's tests: the read, bake and write lists have upstream's counts and hold
//! the (name, extension) pairs upstream's tests look for; extensions are supported as upstream's
//! tests say; the requests it can't read are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::paths::upstream_dir;
use serde_json::{Value, json};

fn call(args: Value) -> Value {
    Oracle::get().call("file_formats", args, &[]).result
}

/// A list of the result as (name, extension) pairs.
fn pairs(list: &Value) -> Vec<(String, String)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let text = |v: &Value| String::from_utf8(bytes(v)).unwrap();
            (text(&p[0]), text(&p[1]))
        })
        .collect()
}

/// The value of a `FILEFORMAT_*` constant of `transforms/FileTransform.h` @ v2.5.2, which
/// upstream's tests name.
fn format_name(constant: &str) -> String {
    let path = upstream_dir().join("src/OpenColorIO/transforms/FileTransform.h");
    let source = std::fs::read_to_string(path).unwrap();
    let line = source
        .lines()
        .find(|l| l.contains(&format!("char {constant}[]")))
        .unwrap();
    let start = line.find('"').unwrap() + 1;
    let end = start + line[start..].find('"').unwrap();
    line[start..end].to_string()
}

/// The counts and pairs of `OCIO_ADD_TEST(FileTransform, all_formats)`
/// (tests/cpu/transforms/FileTransform_tests.cpp @ v2.5.2): 24 read formats, 12 bake formats,
/// 5 write formats, and each `FormatExtensionFoundByName(extension, name)` pair in the read
/// list (but one, below).
#[test]
fn the_lists_hold_upstreams_formats() {
    let result = call(json!({}));
    let read = pairs(&result["read"]);
    let bake = pairs(&result["bake"]);
    let write = pairs(&result["write"]);
    assert_eq!(read.len(), 24, "{result}");
    assert_eq!(bake.len(), 12, "{result}");
    assert_eq!(write.len(), 5, "{result}");
    let clf = format_name("FILEFORMAT_CLF");
    let ctf = format_name("FILEFORMAT_CTF");
    let expected: Vec<(&str, &str)> = vec![
        ("3dl", "flame"),
        ("3dl", "lustre"),
        ("cc", "ColorCorrection"),
        ("ccc", "ColorCorrectionCollection"),
        ("cdl", "ColorDecisionList"),
        ("clf", &clf),
        ("ctf", &ctf),
        ("csp", "cinespace"),
        ("cub", "truelight"),
        ("cube", "iridas_cube"),
        ("cube", "resolve_cube"),
        ("itx", "iridas_itx"),
        ("icc", "International Color Consortium profile"),
        // Upstream's `("icm", "International Color Consortium profile")` looks the name up as
        // a format, whose extensions include `icm`; the binding's list pairs each extension
        // with the name its format declares for it, so it has no such pair.
        ("look", "iridas_look"),
        ("lut", "houdini"),
        ("lut", "Discreet 1D LUT"),
        ("m3d", "pandora_m3d"),
        ("mga", "pandora_mga"),
        ("spi1d", "spi1d"),
        ("spi3d", "spi3d"),
        ("spimtx", "spimtx"),
        ("vf", "nukevf"),
    ];
    for (extension, name) in expected {
        assert!(
            read.iter().any(|(n, e)| n == name && e == extension),
            "({name}, {extension}) is not in {read:?}"
        );
    }
    // Every bake and write format is a read format too.
    for pair in bake.iter().chain(&write) {
        assert!(read.contains(pair), "{pair:?} is not in {read:?}");
    }
    assert!(result["log"].as_array().unwrap().is_empty(), "{result}");
}

/// `OCIO_ADD_TEST(FileTransform, is_format_extension_supported)` @ v2.5.2, through the binding.
#[test]
fn extensions_are_supported_as_upstreams_test_says() {
    let cases = [
        ("foo", false),
        ("bar", false),
        (".", false),
        ("cdl", true),
        (".cdl", true),
        ("Cdl", true),
        (".Cdl", true),
        ("3dl", true),
        (".3dl", true),
    ];
    let extensions: Vec<&str> = cases.iter().map(|(e, _)| *e).collect();
    let result = call(json!({ "extensions": extensions }));
    let supported = result["supported"].as_array().unwrap();
    assert_eq!(supported.len(), cases.len());
    for ((extension, expected), got) in cases.iter().zip(supported) {
        assert_eq!(got["result"], json!(expected), "{extension}: {got}");
    }
}

/// An extension given as bytes reaches the library as they are.
#[test]
fn extensions_as_bytes() {
    let result = call(json!({"extensions": [{"bytes": "43444c"}, {"bytes": "ff"}]}));
    let supported = result["supported"].as_array().unwrap();
    assert_eq!(supported[0]["result"], json!(true), "{result}");
    assert_eq!(supported[1]["result"], json!(false), "{result}");
}

/// Requests it can't read are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"extension": []}),
        json!({"extensions": "cdl"}),
        json!({"extensions": [1]}),
        json!({"extensions": [{"bytes": "zz"}]}),
    ] {
        assert!(
            Oracle::get()
                .try_call("file_formats", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
