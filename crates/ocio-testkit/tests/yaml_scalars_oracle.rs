// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `yaml_scalars` command (`oracle/ocio_oracle/yaml_scalars.py`), against the
//! wheel itself: each spelling lands at its field's documented line of the config text the
//! result carries, every field reads a value back, the conversions of one type agree between
//! its fields, and the requests it can't read exactly are refused.

use ocio_testkit::Oracle;
use serde_json::{Value, json};

/// Per case, per spelling, the command's entries.
fn entries(cases: Value) -> Vec<Vec<Value>> {
    let response = Oracle::get().call("yaml_scalars", json!({"cases": cases}), &[]);
    response.result["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", response.result))
        .iter()
        .map(|c| c.as_array().unwrap().clone())
        .collect()
}

/// The 1-based line of the config text that holds `needle`.
fn line_of(yaml: &str, needle: &str) -> usize {
    yaml.lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} is not in {yaml:?}"))
        + 1
}

/// Each field puts the spelling after its key at the documented column, and the config text
/// is in the result; each reads its value back.
#[test]
fn each_field_reads_its_value() {
    let fields = [
        ("strictparsing", "true", "strictparsing: true", 0),
        ("isdata", "false", "isdata: false", 4),
        (
            "ocio_profile_version",
            "2.1",
            "ocio_profile_version: 2.1",
            0,
        ),
        ("luma", "[0.25, 0.5, 0.25]", "luma: [0.25, 0.5, 0.25]", 0),
        ("base", "10", "base: 10", 6),
        ("allocationvars", "[0, 1]", "allocationvars: [0, 1]", 4),
        ("family", "fam", "family: fam", 4),
        ("description", "words", "description: words", 4),
        ("aliases", "[x, y]", "aliases: [x, y]", 4),
    ];
    let cases: Vec<Value> = fields
        .iter()
        .map(|(field, spelling, _, _)| json!({"field": field, "spellings": [spelling]}))
        .collect();
    let all = entries(Value::Array(cases));
    for ((field, _, line, column), entry) in fields.iter().zip(&all) {
        let entry = &entry[0];
        assert!(entry.get("value").is_some(), "{field}: {entry}");
        let yaml = entry["yaml"].as_str().unwrap();
        let found = yaml.lines().find(|l| l.trim_start() == *line);
        let found = found.unwrap_or_else(|| panic!("{field}: no line {line:?} in {yaml:?}"));
        assert_eq!(found.len() - found.trim_start().len(), *column, "{field}");
        assert!(yaml.starts_with("ocio_profile_version: "), "{field}");
        assert!(yaml.ends_with('\n'), "{field}");
    }
}

/// A spelling one field refuses reports the line it is on.
#[test]
fn a_refused_spelling_reports_its_line() {
    let all = entries(json!([{"field": "base", "spellings": ["not a number"]}]));
    let entry = &all[0][0];
    let yaml = entry["yaml"].as_str().unwrap();
    let line = line_of(yaml, "base: not a number");
    let message = entry["exception"]["message"].as_str().unwrap();
    assert!(message.contains(&format!("At line {line},")), "{message}");
}

/// The double fields read a spelling the same way: as a scalar and as a list element.
#[test]
fn the_double_fields_agree() {
    let spellings = ["0.1", "1e10", "-0", ".inf", "0x10", "1e400", "7abc"];
    let scalar: Vec<Value> = spellings.iter().map(|s| json!(s)).collect();
    let listed: Vec<Value> = spellings
        .iter()
        .map(|s| json!(format!("[{s}, 0, 0]")))
        .collect();
    let all = entries(json!([
        {"field": "base", "spellings": scalar},
        {"field": "luma", "spellings": listed},
    ]));
    for ((s, base), luma) in spellings.iter().zip(&all[0]).zip(&all[1]) {
        match (base.get("value"), luma.get("value")) {
            (Some(b), Some(l)) => assert_eq!(*b, l[0], "{s}"),
            (None, None) => {}
            _ => panic!("{s}: {base} and {luma}"),
        }
    }
}

/// Bytes reach the parser as they are, and a value the binding can't decode comes back as
/// its bytes; the config text comes back as bytes when it isn't UTF-8.
#[test]
fn bytes_pass_through() {
    let all = entries(json!([{"field": "family", "spellings": [{"bytes": "61ff62"}]}]));
    let entry = &all[0][0];
    assert_eq!(entry["exception"]["bytes"], "61ff62", "{entry}");
    assert!(
        entry["yaml"]["bytes"]
            .as_str()
            .unwrap()
            .contains("66616d696c793a2061ff62"),
        "{entry}"
    );
}

/// Requests it can't read exactly are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"cases": [{"field": "nope", "spellings": ["1"]}]}),
        json!({"cases": [{"field": "base", "spellings": [1]}]}),
        json!({"cases": [{"field": "base", "spellings": "1"}]}),
        json!({"cases": [{"field": "base", "spelling": ["1"]}]}),
        json!({"cases": {"field": "base"}}),
        json!({"case": []}),
    ] {
        assert!(
            Oracle::get()
                .try_call("yaml_scalars", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
