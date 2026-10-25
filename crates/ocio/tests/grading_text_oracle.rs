// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The text of the ExposureContrast and grading transforms and of the grading values, against
//! the wheel's, byte for byte (card p5-m3-parity, the M3 exit's text sweep).
//!
//! The `grading_text` fixtures (`oracle/ocio_oracle/grading_text.py`, chunk O5.3) hold, per
//! class, generated cases (every style, direction and dynamic flag, each numeric field set to
//! a NaN, -0.0 and +Inf in every style, and doubles and floats that print in their own way)
//! and what the wheel says about each: its `repr()` (upstream's `operator<<`), what
//! `validate()` raises, its CTF text (`GroupTransform::write`) or what writing raised, what
//! building it raised (a setter's refusal); and `Config::serialize()` of a config holding
//! every transform the wheel built, their YAML. The port builds the same cases from the same
//! specs (`common::grading_specs`), and each must give the same text, or the same refusal with
//! the same message.
//!
//! The fixtures are the cases whose text is the same on both platforms. The cases holding a
//! value that the platforms' C++ libraries spell differently (a negative or signalling NaN,
//! I-7) are in the fixtures for their YAML only, which yaml-cpp writes `.nan` for any NaN;
//! [`spelled_cases_match_the_wheel_on_this_platform`] compares their `repr()` and CTF text
//! with the wheel's on this platform, live.

mod common;

use common::grading_specs::{self, Object};
use ocio::{ColorSpace, ColorSpaceDirection, Config, GroupTransform, Transform};
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values;
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use ocio_testkit::{Oracle, assert_text_eq, fixtures};
use serde_json::{Value, json};

/// The fixture's classes, and the least number of cases each must have, of those the wheel
/// built and printed, and of those it refused (so that no comparison below is vacuous).
const CLASSES: [(&str, usize, usize, usize); 6] = [
    ("ExposureContrastTransform", 180, 150, 0),
    ("GradingPrimaryTransform", 400, 350, 20),
    ("GradingToneTransform", 350, 150, 150),
    ("GradingRGBCurveTransform", 120, 80, 20),
    ("GradingHueCurveTransform", 170, 130, 20),
    ("values", 150, 140, 0),
];

/// The CTF format's name, as the fixtures write it.
const CTF: &str = "Color Transform Format";

/// A case of a fixture: its label, whether it is a transform's, its spec, and whether it holds
/// a value the platforms spell differently.
struct Case {
    label: String,
    transform: bool,
    spec: Value,
    spelled: bool,
}

fn read_cases(class: &str) -> (Vec<Case>, Vec<Value>) {
    let specs: Vec<Value> = serde_json::from_str(&fixtures::read_text(&format!(
        "grading_text/{class}/spec.json"
    )))
    .unwrap();
    let texts: Vec<Value> = serde_json::from_str(&fixtures::read_text(&format!(
        "grading_text/{class}/text.json"
    )))
    .unwrap();
    assert_eq!(specs.len(), texts.len(), "{class}: a text per case");
    let cases: Vec<Case> = specs
        .iter()
        .zip(&texts)
        .map(|(s, t)| {
            assert_eq!(s["label"], t["label"], "{class}: the same case");
            Case {
                label: s["label"].as_str().unwrap().to_string(),
                transform: match s["kind"].as_str() {
                    Some("transform") => true,
                    Some("object") => false,
                    other => panic!("kind {other:?}"),
                },
                spec: s["spec"].clone(),
                spelled: s["spelled"].as_bool().unwrap(),
            }
        })
        .collect();
    (cases, texts)
}

/// What the port built from a case's spec.
enum Port {
    Transform(Box<Transform>),
    Object(Box<Object>),
}

fn build(case: &Case) -> ocio::Result<Port> {
    if case.transform {
        grading_specs::transform(&case.spec).map(|t| Port::Transform(Box::new(t)))
    } else {
        grading_specs::object(&case.spec).map(|o| Port::Object(Box::new(o)))
    }
}

/// An exception's message from the wheel's `{"type", "message"}`; the type is OCIO's.
fn message(exception: &Value) -> &str {
    assert_eq!(exception["type"], "Exception", "{exception}");
    exception["message"].as_str().unwrap()
}

/// The port's CTF text of `transform` (a group holding it, written with the raw config), or
/// the message of what writing raised.
fn port_ctf(transform: &Transform) -> Result<String, String> {
    let mut group = GroupTransform::new();
    group.append_transform(transform.clone());
    let mut written = Vec::new();
    group
        .write(&Config::create_raw().unwrap(), CTF, &mut written)
        .map_err(|e| e.message().to_string())?;
    Ok(String::from_utf8(written).unwrap())
}

/// `validate()`'s outcome as the fixtures write it: null, the exception, or "no validate".
fn compare_validate(label: &str, wheel: &Value, port: Option<ocio::Result<()>>) -> Option<String> {
    let same = match (wheel, &port) {
        (Value::String(s), None) => s == "no validate",
        (Value::Null, Some(Ok(()))) => true,
        (Value::Object(_), Some(Err(e))) => message(wheel) == e.message(),
        _ => false,
    };
    (!same).then(|| format!("{label}: validate: wheel {wheel}, port {port:?}"))
}

/// Counts of what one class compared.
#[derive(Default, Debug)]
struct Counts {
    cases: usize,
    printed: usize,
    refused: usize,
    ctf: usize,
    ctf_refused: usize,
    spelled: usize,
    yaml: usize,
}

/// One class's fixture against the port; returns what it compared and the failures.
fn check_class(class: &str) -> (Counts, Vec<String>) {
    let (cases, texts) = read_cases(class);
    let mut counts = Counts {
        cases: cases.len(),
        ..Counts::default()
    };
    let mut failures = Vec::new();
    let mut built = Vec::new();
    for (case, text) in cases.iter().zip(&texts) {
        let label = &case.label;
        let port = build(case);
        let port = match (text.get("exception"), port) {
            (Some(exception), Err(e)) => {
                counts.refused += 1;
                if message(exception) != e.message() {
                    failures.push(format!(
                        "{label}: refused with\n  wheel {}\n  port  {}",
                        message(exception),
                        e.message()
                    ));
                }
                continue;
            }
            (Some(exception), Ok(_)) => {
                failures.push(format!(
                    "{label}: the wheel refused it ({exception}), not the port"
                ));
                continue;
            }
            (None, Err(e)) => {
                failures.push(format!("{label}: the port refused it: {e}"));
                continue;
            }
            (None, Ok(port)) => port,
        };
        // A spelled case has no repr or CTF in the fixture, and every other case has both.
        assert_eq!(
            text.get("repr").is_none(),
            case.spelled,
            "{label}: repr iff not spelled"
        );
        assert_eq!(
            text.get("ctf").is_none(),
            case.spelled || !case.transform,
            "{label}: ctf iff a transform not spelled"
        );
        counts.spelled += usize::from(case.spelled);
        let (repr, validate) = match &port {
            Port::Transform(t) => (t.to_string(), Some(t.validate())),
            Port::Object(o) => (o.repr(), o.validate()),
        };
        if let Some(wheel) = text.get("repr") {
            counts.printed += 1;
            if wheel.as_str().unwrap() != repr {
                failures.push(format!("{label}: repr\n  wheel {wheel}\n  port  {repr:?}"));
            }
        }
        failures.extend(compare_validate(label, &text["validate"], validate));
        if let Port::Transform(t) = port {
            if let Some(wheel) = text.get("ctf") {
                let port = port_ctf(&t);
                match (wheel.get("text"), wheel.get("exception"), &port) {
                    (Some(w), None, Ok(p)) if w.as_str().unwrap() == p => counts.ctf += 1,
                    (None, Some(e), Err(p)) if message(e) == p => counts.ctf_refused += 1,
                    _ => failures.push(format!("{label}: CTF\n  wheel {wheel}\n  port  {port:?}")),
                }
            }
            built.push(*t);
        }
    }
    if class != "values" {
        counts.yaml = built.len();
        let mut config = Config::new().unwrap();
        for (i, t) in built.iter().enumerate() {
            let mut cs = ColorSpace::new();
            cs.set_name(format!("t{i:04}"));
            cs.set_transform(Some(t), ColorSpaceDirection::ToReference)
                .unwrap();
            config.add_color_space(&cs).unwrap();
        }
        let port = String::from_utf8(config.serialize().unwrap()).unwrap();
        let wheel = fixtures::read_text(&format!("grading_text/{class}/serialize.ocio"));
        if port != wheel {
            if failures.is_empty() {
                // The first difference, line by line.
                assert_text_eq(&format!("{class} serialize.ocio"), &wheel, &port);
            }
            failures.push(format!("{class}: serialize.ocio differs"));
        }
    }
    let log = fixtures::read_text(&format!("grading_text/{class}/log.txt"));
    assert!(log.is_empty(), "{class}: the wheel logged {log}");
    (counts, failures)
}

#[test]
fn fixture_cases_match_the_wheel() {
    let mut report = Vec::new();
    for (class, min_cases, min_printed, min_refused) in CLASSES {
        let (counts, failures) = check_class(class);
        println!("{class}: {counts:?}");
        assert!(counts.cases >= min_cases, "{class}: {counts:?}");
        assert!(counts.printed >= min_printed, "{class}: {counts:?}");
        assert!(counts.refused >= min_refused, "{class}: {counts:?}");
        assert!(counts.spelled > 0, "{class}: {counts:?}");
        if class != "values" {
            assert!(counts.ctf >= min_printed, "{class}: {counts:?}");
            assert!(counts.yaml > counts.printed, "{class}: {counts:?}");
        }
        report.extend(failures);
    }
    if !report.is_empty() {
        let path = std::env::temp_dir().join("grading_text_failures.txt");
        std::fs::write(&path, report.join("\n")).unwrap();
        panic!(
            "{} failures (all in {}):\n{}",
            report.len(),
            path.display(),
            report[..report.len().min(20)].join("\n")
        );
    }
}

#[test]
fn spelled_cases_match_the_wheel_on_this_platform() {
    let mut transforms = Vec::new();
    let mut objects = Vec::new();
    for (class, ..) in CLASSES {
        let (cases, texts) = read_cases(class);
        for (case, text) in cases.into_iter().zip(texts) {
            if case.spelled && text.get("exception").is_none() {
                if case.transform {
                    transforms.push(case);
                } else {
                    objects.push(case);
                }
            }
        }
    }
    assert!(
        transforms.len() >= 25,
        "{} spelled transforms",
        transforms.len()
    );
    assert!(objects.len() >= 8, "{} spelled objects", objects.len());
    let mut failures = Vec::new();

    // repr() and validate() of the transforms, and their CTF text.
    let request = TransformTextRequest {
        transforms: transforms.iter().map(|c| c.spec.clone()).collect(),
        pairs: Vec::new(),
    };
    let reply = request.run();
    let writes: Vec<BatchCall<'_>> = transforms
        .iter()
        .map(|c| BatchCall {
            cmd: "write_transform",
            args: json!({"group": c.spec, "format": CTF}),
            blobs: Vec::new(),
        })
        .collect();
    let written = Oracle::get().batch(&writes, true);
    for ((case, built), write) in transforms.iter().zip(&reply.transforms).zip(written) {
        let label = &case.label;
        let port = grading_specs::transform(&case.spec).expect("the wheel built it");
        let Built::Text(text) = built else {
            panic!("{label}: the wheel refused it live: {built:?}");
        };
        if text.repr != port.to_string() {
            failures.push(format!(
                "{label}: repr\n  wheel {}\n  port  {port}",
                text.repr
            ));
        }
        let validate = match (&text.validate, port.validate()) {
            (None, Ok(())) => true,
            (Some(w), Err(p)) => w.message == p.message(),
            _ => false,
        };
        if !validate {
            failures.push(format!(
                "{label}: validate {:?} {:?}",
                text.validate,
                port.validate()
            ));
        }
        let result = write.unwrap().result;
        let wheel = match (result.get("text"), result.get("exception")) {
            (Some(t), None) => Ok(String::from_utf8(oracle_values::bytes(t)).unwrap()),
            (None, Some(e)) => Err(message(e).to_string()),
            _ => panic!("{label}: {result}"),
        };
        if wheel != port_ctf(&port) {
            failures.push(format!(
                "{label}: CTF\n  wheel {wheel:?}\n  port  {:?}",
                port_ctf(&port)
            ));
        }
    }

    // repr() of the grading values.
    let calls: Vec<BatchCall<'_>> = objects
        .iter()
        .map(|c| BatchCall {
            cmd: "object_calls",
            args: json!({"objects": {"o": c.spec}, "calls": [{"call": "__repr__", "on": "o"}]}),
            blobs: Vec::new(),
        })
        .collect();
    for (case, response) in objects.iter().zip(Oracle::get().batch(&calls, true)) {
        let result = response.unwrap().result;
        let wheel = result["calls"][0]["result"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: {result}", case.label));
        let port = grading_specs::object(&case.spec)
            .expect("the wheel built it")
            .repr();
        if wheel != port {
            failures.push(format!(
                "{}: repr\n  wheel {wheel}\n  port  {port}",
                case.label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
