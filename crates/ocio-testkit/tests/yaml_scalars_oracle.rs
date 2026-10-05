// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `yaml_scalars` command (`oracle/ocio_oracle/yaml_scalars.py`), against the
//! wheel itself: each spelling lands at its field's documented line of the config text the
//! result carries, every field reads a value back at its own width (doubles as 64 bits, floats
//! as 32), the conversions of one type agree between its fields, warnings land in the log as
//! bytes, and the requests it can't read exactly are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, log};
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

fn yaml_text(entry: &Value) -> String {
    String::from_utf8(bytes(&entry["yaml"])).unwrap()
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
        let yaml = yaml_text(entry);
        let found = yaml.lines().find(|l| l.trim_start() == *line);
        let found = found.unwrap_or_else(|| panic!("{field}: no line {line:?} in {yaml:?}"));
        assert_eq!(found.len() - found.trim_start().len(), *column, "{field}");
        assert!(yaml.starts_with("ocio_profile_version: "), "{field}");
        assert!(yaml.ends_with('\n'), "{field}");
    }
    assert_eq!(bytes(&all[6][0]["value"]), b"fam");
    let aliases: Vec<Vec<u8>> = all[8][0]["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(bytes)
        .collect();
    assert_eq!(aliases, [b"x".to_vec(), b"y".to_vec()]);
}

/// Each field reads at its own width: doubles (base, luma) keep the bits a float would lose,
/// floats (allocationvars) come back as 32-bit values.
#[test]
fn each_width_is_its_own() {
    let all = entries(json!([
        {"field": "base", "spellings": ["0.1"]},
        {"field": "luma", "spellings": ["[0.1, 0.2, 0.3]"]},
        {"field": "allocationvars", "spellings": ["[0.1, 1]"]},
    ]));
    let tenth = all[0][0]["value"]["f64"].as_u64().unwrap();
    assert_eq!(all[1][0]["value"][0]["f64"].as_u64().unwrap(), tenth);
    // 0.1 as a double: its low 29 bits are not those of a float's.
    let as_double = f64::from_bits(tenth);
    assert_ne!(f64::from(as_double as f32).to_bits(), tenth);
    let float = all[2][0]["value"][0]["f32"].as_u64().unwrap();
    assert!(float < 1 << 32);
    assert_eq!(u64::from((as_double as f32).to_bits()), float);
}

/// A spelling one field refuses reports the line it is on.
#[test]
fn a_refused_spelling_reports_its_line() {
    let all = entries(json!([{"field": "base", "spellings": ["not a number"]}]));
    let entry = &all[0][0];
    let yaml = yaml_text(entry);
    let line = yaml
        .lines()
        .position(|l| l.contains("base: not a number"))
        .unwrap()
        + 1;
    let message = String::from_utf8(bytes(&entry["exception"]["message"])).unwrap();
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

/// Bytes reach the parser as they are; a value the binding can't decode comes back as its
/// bytes, and a warning about bytes that aren't UTF-8 lands in the log as they are.
#[test]
fn bytes_pass_through() {
    let all = entries(json!([
        {"field": "family", "spellings": [{"bytes": "61ff62"}]},
        {"field": "family", "spellings": [{"bytes": "780a2020202062ff643a2031"}]},
    ]));
    let entry = &all[0][0];
    assert_eq!(entry["undecodable"], "61ff62", "{entry}");
    assert!(
        entry["yaml"]["bytes"]
            .as_str()
            .unwrap()
            .contains("66616d696c793a2061ff62"),
        "{entry}"
    );
    let warned = &all[1][0];
    let lines = log(&warned["log"]);
    assert_eq!(lines.len(), 1, "{warned}");
    assert!(lines[0].starts_with(b"[OpenColorIO Warning]: "), "{warned}");
    assert!(lines[0].windows(3).any(|w| w == b"b\xffd"), "{warned}");
}

/// Requests it can't read exactly are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"cases": [{"field": "nope", "spellings": ["1"]}]}),
        json!({"cases": [{"field": "base", "spellings": [1]}]}),
        json!({"cases": [{"field": "base", "spellings": "1"}]}),
        json!({"cases": [{"field": "base", "spelling": ["1"]}]}),
        json!({"cases": [{"field": "base", "spellings": [{"bytes": "zz"}]}]}),
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
