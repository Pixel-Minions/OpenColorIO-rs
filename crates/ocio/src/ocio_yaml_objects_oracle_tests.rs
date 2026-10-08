// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The loaders of the config's objects (color spaces, looks, view transforms, named transforms)
//! against the wheel, through the oracle's `config_calls` (O3.1).
//!
//! Each case is the YAML text of one object, put in its section of a small config
//! ([`config_text`]) after a color space `raw` (the `default` role's). The wheel loads the
//! config and gives the object by its name; the port parses the same text, makes the object as
//! upstream's config loader does (OCIOYaml.cpp:4778-4935 @ v2.5.2: a color space of its
//! section's reference space, a view transform of the one its keys show) and loads the case's
//! node into it. They must agree on the error (with `OCIOYaml::Read`'s wrapper, as in
//! `ocio_yaml_oracle_tests.rs`), or on the object's getters (strings as bytes, floats by their
//! bits, transforms by their text); and on the warnings logged. The cases give each object a
//! name no other object has, so that the config adds it as it is.

use ocio_ops::open_color_types::{Allocation, BitDepth};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, hex, log};
use serde_json::{Value, json};

use super::oracle_tests::capture_log;
use super::*;
use crate::yaml_cpp::parse::load;

/// The text of `OCIOYaml::Read`'s error for a config read from a stream.
const READ_ERROR: &[u8] = b"Error: Loading the OCIO profile failed. ";

/// The section of a config an object goes in.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    ColorSpaces,
    DisplayColorSpaces,
    Looks,
    ViewTransforms,
    NamedTransforms,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Section::ColorSpaces => "colorspaces",
            Section::DisplayColorSpaces => "display_colorspaces",
            Section::Looks => "looks",
            Section::ViewTransforms => "view_transforms",
            Section::NamedTransforms => "named_transforms",
        }
    }
}

/// A case: its section, the config's version, the object's text and the name the wheel finds
/// it by.
struct Case {
    section: Section,
    version: &'static str,
    text: &'static [u8],
    name: &'static str,
}

fn case(section: Section, text: &'static [u8], name: &'static str) -> Case {
    Case {
        section,
        version: "2",
        text,
        name,
    }
}

/// A case in a version 2.5 config: the config refuses interchange attributes before 2.5
/// (`Config::validate`, which a config made from a stream runs).
fn case25(section: Section, text: &'static [u8], name: &'static str) -> Case {
    Case {
        version: "2.5",
        ..case(section, text, name)
    }
}

/// The config of a case: its object is the first of its section (the second of `colorspaces`,
/// after `raw`, and a blank line), on line 6.
fn config_text(c: &Case) -> Vec<u8> {
    let mut text = format!(
        "ocio_profile_version: {}\nroles: {{default: raw}}\ncolorspaces:\n  - !<ColorSpace> \
         {{name: raw}}\n",
        c.version
    )
    .into_bytes();
    if c.section != Section::ColorSpaces {
        text.extend_from_slice(format!("{}:\n", c.section.key()).as_bytes());
    } else {
        text.extend_from_slice(b"\n");
    }
    text.extend_from_slice(b"  - ");
    text.extend_from_slice(c.text);
    text.push(b'\n');
    text
}

/// The object the port loads.
enum Object {
    ColorSpace(ColorSpace),
    Look(Look),
    ViewTransform(ViewTransform),
    NamedTransform(NamedTransform),
}

/// The port's load of a case: the error message as the wheel reports it, or the object.
fn port_load(c: &Case) -> (Result<Object, Vec<u8>>, Vec<Vec<u8>>) {
    capture_log(|| {
        let failed = |what: Vec<u8>| Err([READ_ERROR, &what].concat());
        let doc = match load(&config_text(c)) {
            Ok(doc) => doc,
            Err(e) => return failed(LoadError::from(e).what()),
        };
        let index = if c.section == Section::ColorSpaces {
            1
        } else {
            0
        };
        let node = doc
            .get(c.section.key())
            .and_then(|n| n.get(index as usize))
            .expect("the case's node");
        let loaded = match c.section {
            Section::ColorSpaces | Section::DisplayColorSpaces => {
                let reference = if c.section == Section::ColorSpaces {
                    ReferenceSpaceType::Scene
                } else {
                    ReferenceSpaceType::Display
                };
                let mut cs = ColorSpace::with_reference_space(reference);
                let major = c.version.split('.').next().unwrap().parse().unwrap();
                load_color_space(&node, &mut cs, major).map(|()| Object::ColorSpace(cs))
            }
            Section::Looks => {
                let mut look = Look::new();
                load_look(&node, &mut look).map(|()| Object::Look(look))
            }
            Section::ViewTransforms => peek_view_transform_reference_space(&node).and_then(|rst| {
                let mut vt = ViewTransform::new(rst);
                load_view_transform(&node, &mut vt).map(|()| Object::ViewTransform(vt))
            }),
            Section::NamedTransforms => {
                let mut nt = NamedTransform::new();
                load_named_transform(&node, &mut nt).map(|()| Object::NamedTransform(nt))
            }
        };
        loaded.or_else(|e| failed(e.what()))
    })
}

fn s(v: &[u8]) -> Value {
    json!({"bytes": hex(v)})
}

fn enum_(name: &str) -> Value {
    json!({"enum": name})
}

fn strings(values: impl Iterator<Item = Vec<u8>>) -> Value {
    Value::Array(values.map(|v| s(&v)).collect())
}

fn dict(map: &std::collections::BTreeMap<Vec<u8>, Vec<u8>>) -> Value {
    json!({"dict": map.iter().map(|(k, v)| json!([s(k), s(v)])).collect::<Vec<_>>()})
}

/// A transform as the oracle writes it, but by its text only.
fn transform_text(t: Option<&Transform>) -> Value {
    match t {
        None => Value::Null,
        Some(t) => s(&t.to_bytes()),
    }
}

fn bit_depth_name(b: BitDepth) -> &'static str {
    match b {
        BitDepth::Unknown => "BIT_DEPTH_UNKNOWN",
        BitDepth::Uint8 => "BIT_DEPTH_UINT8",
        BitDepth::Uint10 => "BIT_DEPTH_UINT10",
        BitDepth::Uint12 => "BIT_DEPTH_UINT12",
        BitDepth::Uint14 => "BIT_DEPTH_UINT14",
        BitDepth::Uint16 => "BIT_DEPTH_UINT16",
        BitDepth::Uint32 => "BIT_DEPTH_UINT32",
        BitDepth::F16 => "BIT_DEPTH_F16",
        BitDepth::F32 => "BIT_DEPTH_F32",
    }
}

fn reference_name(r: ReferenceSpaceType) -> &'static str {
    match r {
        ReferenceSpaceType::Scene => "REFERENCE_SPACE_SCENE",
        ReferenceSpaceType::Display => "REFERENCE_SPACE_DISPLAY",
    }
}

/// The getter calls on the object `o` (each a call name and its arguments) with the port's
/// values, as the oracle writes them; a transform getter's value is its text.
fn getters(object: &Object) -> Vec<(&'static str, Vec<Value>, Value)> {
    match object {
        Object::ColorSpace(cs) => vec![
            ("getName", vec![], s(cs.name())),
            ("getFamily", vec![], s(cs.family())),
            ("getEqualityGroup", vec![], s(cs.equality_group())),
            ("getDescription", vec![], s(cs.description())),
            ("getEncoding", vec![], s(cs.encoding())),
            ("getInteropID", vec![], s(cs.interop_id())),
            ("getBitDepth", vec![], enum_(bit_depth_name(cs.bit_depth()))),
            ("isData", vec![], json!(cs.is_data())),
            (
                "getAllocation",
                vec![],
                enum_(match cs.allocation() {
                    Allocation::Unknown => "ALLOCATION_UNKNOWN",
                    Allocation::Uniform => "ALLOCATION_UNIFORM",
                    Allocation::Lg2 => "ALLOCATION_LG2",
                }),
            ),
            (
                "getAllocationVars",
                vec![],
                Value::Array(
                    cs.allocation_vars()
                        .iter()
                        .map(|&v| json!({"f64": f64::from(v).to_bits()}))
                        .collect(),
                ),
            ),
            (
                "getReferenceSpaceType",
                vec![],
                enum_(reference_name(cs.reference_space_type())),
            ),
            (
                "getAliases",
                vec![],
                strings((0..cs.num_aliases()).map(|i| cs.alias(i).to_vec())),
            ),
            (
                "getCategories",
                vec![],
                strings((0..cs.num_categories()).map(|i| cs.category(i).unwrap().to_vec())),
            ),
            (
                "getInterchangeAttributes",
                vec![],
                dict(cs.interchange_attributes()),
            ),
            (
                "getTransform",
                vec![enum_("COLORSPACE_DIR_TO_REFERENCE")],
                transform_text(cs.transform(ColorSpaceDirection::ToReference)),
            ),
            (
                "getTransform",
                vec![enum_("COLORSPACE_DIR_FROM_REFERENCE")],
                transform_text(cs.transform(ColorSpaceDirection::FromReference)),
            ),
        ],
        Object::Look(look) => vec![
            ("getName", vec![], s(look.name())),
            ("getProcessSpace", vec![], s(look.process_space())),
            ("getDescription", vec![], s(look.description())),
            (
                "getInterchangeAttributes",
                vec![],
                dict(look.interchange_attributes()),
            ),
            ("getTransform", vec![], transform_text(look.transform())),
            (
                "getInverseTransform",
                vec![],
                transform_text(look.inverse_transform()),
            ),
        ],
        Object::ViewTransform(vt) => vec![
            ("getName", vec![], s(vt.name())),
            ("getFamily", vec![], s(vt.family())),
            ("getDescription", vec![], s(vt.description())),
            (
                "getReferenceSpaceType",
                vec![],
                enum_(reference_name(vt.reference_space_type())),
            ),
            (
                "getCategories",
                vec![],
                strings((0..vt.num_categories()).map(|i| vt.category(i).unwrap().to_vec())),
            ),
            (
                "getInterchangeAttributes",
                vec![],
                dict(vt.interchange_attributes()),
            ),
            (
                "getTransform",
                vec![enum_("VIEWTRANSFORM_DIR_TO_REFERENCE")],
                transform_text(vt.transform(ViewTransformDirection::ToReference)),
            ),
            (
                "getTransform",
                vec![enum_("VIEWTRANSFORM_DIR_FROM_REFERENCE")],
                transform_text(vt.transform(ViewTransformDirection::FromReference)),
            ),
        ],
        Object::NamedTransform(nt) => vec![
            ("getName", vec![], s(nt.name())),
            ("getFamily", vec![], s(nt.family())),
            ("getDescription", vec![], s(nt.description())),
            ("getEncoding", vec![], s(nt.encoding())),
            (
                "getAliases",
                vec![],
                strings((0..nt.num_aliases()).map(|i| nt.alias(i).to_vec())),
            ),
            (
                "getCategories",
                vec![],
                strings((0..nt.num_categories()).map(|i| nt.category(i).unwrap().to_vec())),
            ),
            (
                "getTransform",
                vec![enum_("TRANSFORM_DIR_FORWARD")],
                transform_text(nt.transform(TransformDirection::Forward)),
            ),
            (
                "getTransform",
                vec![enum_("TRANSFORM_DIR_INVERSE")],
                transform_text(nt.transform(TransformDirection::Inverse)),
            ),
        ],
    }
}

/// The getter of the config that finds a case's object by its name.
fn finder(section: Section) -> &'static str {
    match section {
        Section::ColorSpaces | Section::DisplayColorSpaces => "getColorSpace",
        Section::Looks => "getLook",
        Section::ViewTransforms => "getViewTransform",
        Section::NamedTransforms => "getNamedTransform",
    }
}

/// A result as the port writes it: a string the binding couldn't decode as its bytes, and a
/// transform by its text.
fn normalized(call: &Value) -> Value {
    match (call.get("result"), call.get("undecodable")) {
        (Some(result), _) => match result.get("repr") {
            Some(repr) => match repr.get("undecodable") {
                Some(h) => json!({"bytes": h}),
                None => repr.clone(),
            },
            None => result.clone(),
        },
        (None, Some(h)) => json!({"bytes": h}),
        _ => call.clone(),
    }
}

/// Checks each case against the wheel; panics listing every case that differs.
fn check(cases: &[Case]) {
    let ported: Vec<_> = cases.iter().map(port_load).collect();

    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .zip(&ported)
        .map(|(c, (loaded, _))| {
            let mut calls = vec![json!({"call": finder(c.section), "args": [c.name], "as": "o"})];
            if let Ok(object) = loaded {
                for (name, args, _) in getters(object) {
                    calls.push(json!({"call": name, "on": "o", "args": args}));
                }
            }
            BatchCall {
                cmd: "config_calls",
                args: json!({"config": {"yaml": {"bytes": hex(&config_text(c))}},
                             "calls": calls}),
                blobs: Vec::new(),
            }
        })
        .collect();
    let wheel: Vec<Value> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| r.expect("config_calls").result)
        .collect();

    let mut failures = Vec::new();
    for ((c, (loaded, port_log)), w) in cases.iter().zip(&ported).zip(&wheel) {
        let label = String::from_utf8_lossy(c.text);
        let wheel_log = log(&w["config_log"]);
        if &wheel_log != port_log {
            failures.push(format!(
                "{label}: log\n  wheel {:?}\n  port  {:?}",
                wheel_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>(),
                port_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>()
            ));
        }
        let config = &w["config"];
        match loaded {
            Err(message) => {
                let wheel_message = match config.get("undecodable") {
                    Some(h) => bytes(&json!({"bytes": h})),
                    None if config.is_null() => Vec::new(),
                    None => bytes(&config["exception"]["message"]),
                };
                if &wheel_message != message {
                    failures.push(format!(
                        "{label}: error\n  wheel {}\n  port  {}",
                        String::from_utf8_lossy(&wheel_message),
                        String::from_utf8_lossy(message)
                    ));
                }
            }
            Ok(object) => {
                if !config.is_null() {
                    failures.push(format!("{label}: the wheel fails: {config}"));
                    continue;
                }
                let results = w["calls"].as_array().unwrap();
                for ((name, _, port), call) in getters(object).iter().zip(&results[1..]) {
                    let wheel = normalized(call);
                    if &wheel != port {
                        failures.push(format!("{label}: {name}\n  wheel {wheel}\n  port  {port}"));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

use Section::{ColorSpaces, DisplayColorSpaces, Looks, NamedTransforms, ViewTransforms};

/// The color space's keys, their values and errors, its descriptions without their trailing
/// newlines, its interchange attributes, and its transforms by reference space and version.
#[test]
fn color_spaces_load_as_in_the_wheel() {
    let mut cases =
        vec![
        case(ColorSpaces, b"!<ColorSpace> {name: cs}", "cs"),
        case(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, family: f, equalitygroup: g, bitdepth: 32f, isdata: true, \
              encoding: scene-linear, allocation: lg2, allocationvars: [-8, 5, 0.00390625], \
              aliases: [a1, a2], categories: [c1, c2], interop_id: lin_ap1_scene}",
            "cs",
        ),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, description: one line}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, description: \"two\\n\\n\"}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, description: \"\\n\\n\"}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, description: \"\\n\"}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, description: \"a\\nb\\n\"}", "cs"),
        case(
            ColorSpaces,
            b"!<ColorSpace>\n    name: cs\n    description: |\n      line 1\n      line 2\n\n",
            "cs",
        ),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, bitdepth: foo}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, bitdepth: [8ui]}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, isdata: maybe}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, aliases: a}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, aliases: [[a]]}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, categories: []}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, aliases: [cs, CS, \"\"]}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, interop_id: \"bad id\"}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, allocationvars: []}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, allocationvars: 1}", "cs"),
        case25(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, interchange: {amf_transform_ids: \"a\\nb\\n\\n\", \
              icc_profile_name: p}}",
            "cs",
        ),
        case25(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, interchange: {foo: 1, amf_transform_ids: x}}",
            "cs",
        ),
        case25(ColorSpaces,
 b"!<ColorSpace> {name: cs, interchange: {foo: }}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, interchange: [a]}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, interchange: x}", "cs"),
        case25(ColorSpaces,
 b"!<ColorSpace> {name: cs, interchange: {[a]: b}}", "cs"),
        case25(ColorSpaces,
 b"!<ColorSpace> {name: cs, interchange: {a: [b]}}", "cs"),
        case(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, to_scene_reference: !<LogTransform> {base: 10}, \
              from_scene_reference: !<MatrixTransform> {offset: [1, 2, 3, 4]}}",
            "cs",
        ),
        case(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, to_reference: !<LogTransform> {}, from_reference: \
              !<RangeTransform> {}}",
            "cs",
        ),
        case(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, to_display_reference: !<LogTransform> {}}",
            "cs",
        ),
        case(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, from_display_reference: !<LogTransform> {}}",
            "cs",
        ),
        case(
            DisplayColorSpaces,
            b"!<ColorSpace> {name: cs, to_display_reference: !<LogTransform> {}, \
              from_display_reference: !<MatrixTransform> {}}",
            "cs",
        ),
        case(
            DisplayColorSpaces,
            b"!<ColorSpace> {name: cs, to_scene_reference: !<LogTransform> {}}",
            "cs",
        ),
        case(
            DisplayColorSpaces,
            b"!<ColorSpace> {name: cs, from_reference: !<LogTransform> {}}",
            "cs",
        ),
        case(
            ColorSpaces,
            b"!<ColorSpace> {name: cs, to_scene_reference: !<FixedFunctionTransform> \
              {style: ACES_Glow03, params: [1]}}",
            "cs",
        ),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, foo: 1, bar: [2]}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, family: f, family: g}", "cs"),
        case(ColorSpaces, b"!<ColorSpace> [cs]", "cs"),
        case(ColorSpaces, b"!<ColorSpace> {name: \"c\\0s\"}", "c"),
        case(ColorSpaces, b"!<ColorSpace> {name: cs, family: \xe9}", "cs"),
    ];
    // Version 1: the scene keys are unknown.
    for text in [
        &b"!<ColorSpace> {name: cs, to_scene_reference: !<LogTransform> {}}"[..],
        b"!<ColorSpace> {name: cs, to_reference: !<LogTransform> {}, from_scene_reference: \
          !<LogTransform> {}}",
    ] {
        cases.push(Case {
            section: ColorSpaces,
            version: "1",
            text,
            name: "cs",
        });
    }
    check(&cases);
}

/// The look's keys, its transforms (an invalid fixed function refused), descriptions and
/// interchange attributes.
#[test]
fn looks_load_as_in_the_wheel() {
    check(&[
        case(Looks, b"!<Look> {name: l, process_space: raw}", "l"),
        case(
            Looks,
            b"!<Look> {name: l, process_space: raw, transform: !<CDLTransform> {slope: [1, 2, \
              3]}, inverse_transform: !<MatrixTransform> {}, description: \"d\\n\"}",
            "l",
        ),
        case(Looks, b"!<Look> {name: l, description: \"\\n\"}", "l"),
        case25(
            Looks,
            b"!<Look> {name: l, interchange: {amf_transform_ids: a, foo: b}}",
            "l",
        ),
        case(Looks, b"!<Look> {name: l, interchange: 1}", "l"),
        case(
            Looks,
            b"!<Look> {name: l, transform: !<FixedFunctionTransform> {style: REC2100_Surround}}",
            "l",
        ),
        case(Looks, b"!<Look> {name: l, transform: 5}", "l"),
        case(Looks, b"!<Look> {name: l, foo: 1}", "l"),
        case(Looks, b"!<Look> {name: l, name: m}", "l"),
        case(Looks, b"!<Look> {name: [l]}", "l"),
    ]);
}

/// The view transform's reference space, found from its transform keys, its keys and errors.
#[test]
fn view_transforms_load_as_in_the_wheel() {
    check(&[
        case(
            ViewTransforms,
            b"!<ViewTransform> {name: vt, from_scene_reference: !<LogTransform> {}}",
            "vt",
        ),
        case25(
            ViewTransforms,
            b"!<ViewTransform> {name: vt, to_display_reference: !<LogTransform> {}, \
              from_display_reference: !<MatrixTransform> {}, family: f, categories: [a, b], \
              description: \"x\\n\\n\", interchange: {amf_transform_ids: i, bad: j}}",
            "vt",
        ),
        case(ViewTransforms, b"!<ViewTransform> {name: vt}", "vt"),
        case(
            ViewTransforms,
            b"!<ViewTransform> {name: vt, to_scene_reference: !<LogTransform> {}, \
              to_display_reference: !<LogTransform> {}}",
            "vt",
        ),
        case(
            ViewTransforms,
            b"!<ViewTransform> {name: vt, to_scene_reference: }",
            "vt",
        ),
        case(ViewTransforms, b"!<ViewTransform> [vt]", "vt"),
        case(
            ViewTransforms,
            b"!<ViewTransform> {[a]: 1, to_scene_reference: !<LogTransform> {}}",
            "vt",
        ),
        case(
            ViewTransforms,
            b"!<ViewTransform> {name: vt, from_scene_reference: !<LogTransform> {}, foo: 1, \
              name: x}",
            "vt",
        ),
        case(
            ViewTransforms,
            b"!<ViewTransform> {name: vt, from_scene_reference: !<LogTransform> {}, \
              categories: x}",
            "vt",
        ),
    ]);
}

/// The named transform's keys, its description with its trailing newlines (I-144), its
/// transforms and errors.
#[test]
fn named_transforms_load_as_in_the_wheel() {
    check(&[
        case(
            NamedTransforms,
            b"!<NamedTransform> {name: nt, transform: !<LogTransform> {}}",
            "nt",
        ),
        case(
            NamedTransforms,
            b"!<NamedTransform> {name: nt, aliases: [a, b], family: f, categories: [c], \
              encoding: log, description: \"d\\n\\n\", inverse_transform: !<MatrixTransform> \
              {}}",
            "nt",
        ),
        case(
            NamedTransforms,
            b"!<NamedTransform> {name: nt, transform: !<LogTransform> {}, description: \"\\n\"}",
            "nt",
        ),
        case(
            NamedTransforms,
            b"!<NamedTransform> {name: nt, transform: !<LogTransform> {}, foo: 1}",
            "nt",
        ),
        case(
            NamedTransforms,
            b"!<NamedTransform> {name: nt, transform: !<FixedFunctionTransform> \
              {style: ACES_Glow03, params: [1]}}",
            "nt",
        ),
        case(NamedTransforms, b"!<NamedTransform> [nt]", "nt"),
        case(
            NamedTransforms,
            b"!<NamedTransform> {name: nt, aliases: x, transform: !<LogTransform> {}}",
            "nt",
        ),
    ]);
}
