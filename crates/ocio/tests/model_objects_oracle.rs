// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config's model objects against the wheel, through the oracle's `config_calls`: the
//! `ColorSpace`, `Look`, `ViewTransform`, `NamedTransform` and `ColorSpaceSet` built by the same
//! calls on both sides. Each setter's error, if any, is compared with the wheel's exception
//! message (bytes); then the object's `repr()` (upstream's `operator<<`) with `to_bytes()`, and its
//! getters with the port's.
//!
//! The binding's `setAllocationVars` takes 2 or 3 variables, so the cases set 2 or 3.

use ocio::{
    Allocation, BitDepth, ColorSpace, ColorSpaceDirection, ColorSpaceSet, LogTransform, Look,
    MatrixTransform, NamedTransform, RangeTransform, ReferenceSpaceType, Transform,
    TransformDirection, ViewTransform, ViewTransformDirection,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, exception, log};
use serde_json::{Value, json};

/// An object's `repr()`, as bytes.
trait Repr {
    fn repr(&self) -> Vec<u8>;
}

macro_rules! repr {
    ($($T:ty),*) => {$(
        impl Repr for $T {
            fn repr(&self) -> Vec<u8> {
                self.to_bytes()
            }
        }
    )*};
}
repr!(ColorSpace, Look, NamedTransform, ViewTransform);

/// The port's side of a call: its error message, if it fails.
type PortCall<T> = Box<dyn Fn(&mut T) -> Option<Vec<u8>>>;

/// One call on the object, as the oracle makes it and as the port does.
struct Op<T> {
    call: Value,
    port: PortCall<T>,
}

fn op<T>(call: Value, port: impl Fn(&mut T) -> Option<Vec<u8>> + 'static) -> Op<T> {
    Op {
        call,
        port: Box::new(port),
    }
}

/// A case: the constructor's arguments and the calls.
struct Case<T> {
    label: String,
    new_args: Value,
    port: T,
    ops: Vec<Op<T>>,
}

/// A string the wheel returned: `{"bytes": hex}`.
#[track_caller]
fn text(v: &Value) -> Vec<u8> {
    bytes(v)
}

fn texts(v: &Value) -> Vec<Vec<u8>> {
    v.as_array()
        .unwrap_or_else(|| panic!("not a list: {v}"))
        .iter()
        .map(text)
        .collect()
}

fn f32_list(v: &Value) -> Vec<u32> {
    v.as_array()
        .unwrap_or_else(|| panic!("not a list: {v}"))
        .iter()
        .map(|x| (f64::from_bits(x["f64"].as_u64().unwrap()) as f32).to_bits())
        .collect()
}

fn dict(v: &Value) -> Vec<(Vec<u8>, Vec<u8>)> {
    v["dict"]
        .as_array()
        .unwrap_or_else(|| panic!("not a dict: {v}"))
        .iter()
        .map(|kv| (text(&kv[0]), text(&kv[1])))
        .collect()
}

/// A transform the wheel returned (`{"class", "repr"}`), as its `repr()` bytes, or `None`.
fn transform_repr(v: &Value) -> Option<Vec<u8>> {
    if v.is_null() {
        return None;
    }
    Some(bytes(&v["repr"]))
}

/// Runs the cases of `class` in one request and compares each through `compare`, which gets
/// the port's object and the wheel's dump and returns the differences.
fn check<T: Repr>(
    class: &str,
    mut cases: Vec<Case<T>>,
    compare: impl Fn(&T, &Value) -> Vec<String>,
) {
    let mut calls = Vec::new();
    let mut spans = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let name = format!("o{k}");
        let start = calls.len();
        calls.push(json!({"new": class, "args": case.new_args, "as": name}));
        for o in &case.ops {
            let mut call = o.call.clone();
            call["on"] = json!(name);
            calls.push(call);
        }
        calls.push(json!({"dump": name}));
        spans.push(start);
    }
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "raw", "calls": calls}),
            &[],
        )
        .result;
    assert_logs_empty(&response);
    let results = response["calls"].as_array().expect("the calls' results");
    let mut failures = Vec::new();
    for (case, &start) in cases.iter_mut().zip(&spans) {
        assert!(
            results[start].get("result").is_some(),
            "{}: {}",
            case.label,
            results[start]
        );
        for (i, o) in case.ops.iter().enumerate() {
            let wheel = results[start + 1 + i]
                .get("exception")
                .map(|_| exception(&results[start + 1 + i]).1);
            let port = (o.port)(&mut case.port);
            if wheel != port {
                failures.push(format!(
                    "{}: {}: wheel {:?}, port {:?}",
                    case.label,
                    o.call,
                    wheel.as_deref().map(String::from_utf8_lossy),
                    port.as_deref().map(String::from_utf8_lossy)
                ));
            }
        }
        let dump = &results[start + 1 + case.ops.len()]["result"];
        let repr = bytes(&dump["repr"]);
        let port = case.port.repr();
        if repr != port {
            failures.push(format!(
                "{}: repr\n  wheel {:?}\n  port  {port:?}",
                case.label,
                String::from_utf8_lossy(&repr)
            ));
        }
        for diff in compare(&case.port, &dump["getters"]) {
            failures.push(format!("{}: {diff}", case.label));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// OCIO logs nothing while the config is made, nor during any call.
#[track_caller]
fn assert_logs_empty(response: &Value) {
    assert!(log(&response["config_log"]).is_empty(), "{response}");
    for call in response["calls"].as_array().expect("the calls' results") {
        assert!(log(&call["log"]).is_empty(), "{call}");
    }
}

/// Pushes a difference when the wheel's and the port's values differ.
fn diff<V: PartialEq + std::fmt::Debug>(out: &mut Vec<String>, what: &str, wheel: V, port: V) {
    if wheel != port {
        out.push(format!("{what}: wheel {wheel:?}, port {port:?}"));
    }
}

/// Names that print in their own way: empty, ending at a NUL, non-ASCII, holding the text's
/// separators, different only in case.
const NAMES: [&str; 8] = [
    "",
    "lin_rec709",
    "a\0b",
    "caf\u{e9} \u{2014}",
    "x, y=z>",
    "LIN_REC709",
    "  spaced  ",
    "ACEScg",
];

fn s(v: &str) -> Value {
    json!(v)
}

fn enum_spec(name: &str) -> Value {
    json!({"enum": name})
}

/// Transforms of different classes, the matrix one changing the stream's precision (I-73).
fn transforms() -> Vec<(Value, Transform)> {
    let mut matrix = MatrixTransform::new();
    matrix.set_offset(&[0.1, 1.0 / 3.0, 0.0, 0.0]);
    let mut range = RangeTransform::new();
    range.set_min_in_value(0.123_456_789_123_4);
    let log = LogTransform::new();
    vec![
        (
            json!({"class": "MatrixTransform",
                "calls": [["setOffset", [0.1, 1.0 / 3.0, 0.0, 0.0]]]}),
            matrix.into(),
        ),
        (
            json!({"class": "RangeTransform",
                "calls": [["setMinInValue", 0.123_456_789_123_4]]}),
            range.into(),
        ),
        (json!({"class": "LogTransform"}), log.into()),
    ]
}

fn color_space_cases() -> Vec<Case<ColorSpace>> {
    let n = NAMES.len();
    let mut cases = Vec::new();
    let transforms = transforms();
    for k in 0..n {
        let reference = if k % 2 == 0 {
            ReferenceSpaceType::Scene
        } else {
            ReferenceSpaceType::Display
        };
        let ref_name = match reference {
            ReferenceSpaceType::Scene => "REFERENCE_SPACE_SCENE",
            ReferenceSpaceType::Display => "REFERENCE_SPACE_DISPLAY",
        };
        let name = NAMES[k];
        let other = NAMES[(k + 1) % n];
        let third = NAMES[(k + 4) % n];
        let mut ops: Vec<Op<ColorSpace>> = vec![
            op(
                json!({"call": "setName", "args": [s(name)]}),
                move |c: &mut ColorSpace| {
                    c.set_name(name);
                    None
                },
            ),
            op(
                json!({"call": "addAlias", "args": [s(other)]}),
                move |c: &mut ColorSpace| {
                    c.add_alias(other);
                    None
                },
            ),
            op(
                json!({"call": "addAlias", "args": [s(third)]}),
                move |c: &mut ColorSpace| {
                    c.add_alias(third);
                    None
                },
            ),
            op(
                json!({"call": "addAlias", "args": [s(name)]}),
                move |c: &mut ColorSpace| {
                    c.add_alias(name);
                    None
                },
            ),
            op(
                json!({"call": "setFamily", "args": [s(other)]}),
                move |c: &mut ColorSpace| {
                    c.set_family(other);
                    None
                },
            ),
            op(
                json!({"call": "setEqualityGroup", "args": [s(third)]}),
                move |c: &mut ColorSpace| {
                    c.set_equality_group(third);
                    None
                },
            ),
            op(
                json!({"call": "setDescription", "args": [s(name)]}),
                move |c: &mut ColorSpace| {
                    c.set_description(name);
                    None
                },
            ),
            op(
                json!({"call": "setEncoding", "args": [s(third)]}),
                move |c: &mut ColorSpace| {
                    c.set_encoding(third);
                    None
                },
            ),
            op(
                json!({"call": "addCategory", "args": [s(other)]}),
                move |c: &mut ColorSpace| {
                    c.add_category(other);
                    None
                },
            ),
            op(
                json!({"call": "addCategory", "args": [s(" file-io ")]}),
                |c: &mut ColorSpace| {
                    c.add_category(" file-io ");
                    None
                },
            ),
            op(
                json!({"call": "addCategory", "args": [s("FILE-IO")]}),
                |c: &mut ColorSpace| {
                    c.add_category("FILE-IO");
                    None
                },
            ),
        ];
        let interop = [
            "srgb_p3d65_scene",
            "ns:cs",
            "a:b:c",
            ":x",
            "Upper",
            "",
            "x\0Y",
            "ok|()",
        ][k % 8];
        ops.push(op(
            json!({"call": "setInteropID", "args": [s(interop)]}),
            move |c: &mut ColorSpace| c.set_interop_id(interop).err().map(|e| e.what().to_vec()),
        ));
        let attr = ["amf_transform_ids", "ICC_PROFILE_NAME", "unknown"][k % 3];
        ops.push(op(
            json!({"call": "setInterchangeAttribute", "args": [s(attr), s(other)]}),
            move |c: &mut ColorSpace| {
                c.set_interchange_attribute(attr, other)
                    .err()
                    .map(|e| e.what().to_vec())
            },
        ));
        ops.push(op(
            json!({"call": "setInterchangeAttribute", "args": [s("icc_profile_name"), s("p\nq")]}),
            |c: &mut ColorSpace| {
                c.set_interchange_attribute("icc_profile_name", "p\nq")
                    .err()
                    .map(|e| e.what().to_vec())
            },
        ));
        let depth = ["BIT_DEPTH_F16", "BIT_DEPTH_UNKNOWN", "BIT_DEPTH_UINT10"][k % 3];
        let depth_port = [BitDepth::F16, BitDepth::Unknown, BitDepth::Uint10][k % 3];
        ops.push(op(
            json!({"call": "setBitDepth", "args": [enum_spec(depth)]}),
            move |c: &mut ColorSpace| {
                c.set_bit_depth(depth_port);
                None
            },
        ));
        ops.push(op(
            json!({"call": "setIsData", "args": [k % 3 == 0]}),
            move |c: &mut ColorSpace| {
                c.set_is_data(k % 3 == 0);
                None
            },
        ));
        if k % 4 != 3 {
            let vars: Vec<f32> = [
                vec![0.0f32, 1.0],
                vec![-8.0, 5.0, 0.003_906_25],
                vec![f32::NAN, f32::INFINITY, 1.0 / 3.0],
            ][k % 3]
                .clone();
            let alloc = [Allocation::Lg2, Allocation::Uniform, Allocation::Unknown][k % 3];
            let alloc_name = ["ALLOCATION_LG2", "ALLOCATION_UNIFORM", "ALLOCATION_UNKNOWN"][k % 3];
            let specs: Vec<Value> = vars
                .iter()
                .map(|&v| json!({"f64": f64::from(v).to_bits()}))
                .collect();
            ops.push(op(
                json!({"call": "setAllocation", "args": [enum_spec(alloc_name)]}),
                move |c: &mut ColorSpace| {
                    c.set_allocation(alloc);
                    None
                },
            ));
            ops.push(op(
                json!({"call": "setAllocationVars", "args": [specs]}),
                move |c: &mut ColorSpace| {
                    c.set_allocation_vars(&vars);
                    None
                },
            ));
        }
        for (j, dir) in [
            (
                ColorSpaceDirection::ToReference,
                "COLORSPACE_DIR_TO_REFERENCE",
            ),
            (
                ColorSpaceDirection::FromReference,
                "COLORSPACE_DIR_FROM_REFERENCE",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            if (k + j) % 3 == 2 {
                continue;
            }
            let (spec, port) = transforms[(k + j) % transforms.len()].clone();
            ops.push(op(
                json!({"call": "setTransform", "args": [{"transform": spec}, enum_spec(dir.1)]}),
                move |c: &mut ColorSpace| {
                    c.set_transform(Some(&port), dir.0);
                    None
                },
            ));
        }
        if k == 5 {
            ops.push(op(
                json!({"call": "removeAlias", "args": [s("lin_rec709")]}),
                |c: &mut ColorSpace| {
                    c.remove_alias("lin_rec709");
                    None
                },
            ));
            ops.push(op(
                json!({"call": "removeCategory", "args": [s("file-IO")]}),
                |c: &mut ColorSpace| {
                    c.remove_category("file-IO");
                    None
                },
            ));
        }
        cases.push(Case {
            label: format!("ColorSpace {k}"),
            new_args: json!([enum_spec(ref_name)]),
            port: ColorSpace::with_reference_space(reference),
            ops,
        });
    }
    cases
}

fn compare_color_space(c: &ColorSpace, g: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff(&mut out, "getName", text(&g["getName"]), c.name().to_vec());
    let aliases: Vec<Vec<u8>> = (0..c.num_aliases()).map(|i| c.alias(i).to_vec()).collect();
    diff(&mut out, "getAliases", texts(&g["getAliases"]), aliases);
    let categories: Vec<Vec<u8>> = (0..c.num_categories())
        .map(|i| c.category(i).unwrap().to_vec())
        .collect();
    diff(
        &mut out,
        "getCategories",
        texts(&g["getCategories"]),
        categories,
    );
    diff(
        &mut out,
        "getFamily",
        text(&g["getFamily"]),
        c.family().to_vec(),
    );
    diff(
        &mut out,
        "getEqualityGroup",
        text(&g["getEqualityGroup"]),
        c.equality_group().to_vec(),
    );
    diff(
        &mut out,
        "getDescription",
        text(&g["getDescription"]),
        c.description().to_vec(),
    );
    diff(
        &mut out,
        "getEncoding",
        text(&g["getEncoding"]),
        c.encoding().to_vec(),
    );
    diff(
        &mut out,
        "getInteropID",
        text(&g["getInteropID"]),
        c.interop_id().to_vec(),
    );
    diff(
        &mut out,
        "isData",
        g["isData"].as_bool().unwrap(),
        c.is_data(),
    );
    let vars: Vec<u32> = c.allocation_vars().iter().map(|v| v.to_bits()).collect();
    diff(
        &mut out,
        "getAllocationVars",
        f32_list(&g["getAllocationVars"]),
        vars,
    );
    let attrs: Vec<(Vec<u8>, Vec<u8>)> = c
        .interchange_attributes()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    diff(
        &mut out,
        "getInterchangeAttributes",
        dict(&g["getInterchangeAttributes"]),
        attrs,
    );
    out
}

#[test]
fn color_spaces_match_the_wheel() {
    check("ColorSpace", color_space_cases(), compare_color_space);
}

fn look_cases() -> Vec<Case<Look>> {
    let n = NAMES.len();
    let transforms = transforms();
    (0..n)
        .map(|k| {
            let name = NAMES[k];
            let other = NAMES[(k + 2) % n];
            let mut ops: Vec<Op<Look>> = vec![
                op(
                    json!({"call": "setName", "args": [s(name)]}),
                    move |l: &mut Look| {
                        l.set_name(name);
                        None
                    },
                ),
                op(
                    json!({"call": "setProcessSpace", "args": [s(other)]}),
                    move |l: &mut Look| {
                        l.set_process_space(other);
                        None
                    },
                ),
                op(
                    json!({"call": "setDescription", "args": [s(other)]}),
                    move |l: &mut Look| {
                        l.set_description(other);
                        None
                    },
                ),
            ];
            let attr = ["amf_transform_ids", "AMF_Transform_IDs", "icc_profile_name"][k % 3];
            ops.push(op(
                json!({"call": "setInterchangeAttribute", "args": [s(attr), s(name)]}),
                move |l: &mut Look| {
                    l.set_interchange_attribute(attr, name)
                        .err()
                        .map(|e| e.what().to_vec())
                },
            ));
            if k % 3 != 2 {
                let (spec, port) = transforms[k % 3].clone();
                ops.push(op(
                    json!({"call": "setTransform", "args": [{"transform": spec}]}),
                    move |l: &mut Look| {
                        l.set_transform(&port);
                        None
                    },
                ));
            }
            if k % 2 == 1 {
                let (spec, port) = transforms[(k + 1) % 3].clone();
                ops.push(op(
                    json!({"call": "setInverseTransform", "args": [{"transform": spec}]}),
                    move |l: &mut Look| {
                        l.set_inverse_transform(&port);
                        None
                    },
                ));
            }
            Case {
                label: format!("Look {k}"),
                new_args: json!([]),
                port: Look::new(),
                ops,
            }
        })
        .collect()
}

fn compare_look(l: &Look, g: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff(&mut out, "getName", text(&g["getName"]), l.name().to_vec());
    diff(
        &mut out,
        "getProcessSpace",
        text(&g["getProcessSpace"]),
        l.process_space().to_vec(),
    );
    diff(
        &mut out,
        "getDescription",
        text(&g["getDescription"]),
        l.description().to_vec(),
    );
    diff(
        &mut out,
        "getTransform",
        transform_repr(&g["getTransform"]),
        l.transform().map(|t| t.to_bytes()),
    );
    diff(
        &mut out,
        "getInverseTransform",
        transform_repr(&g["getInverseTransform"]),
        l.inverse_transform().map(|t| t.to_bytes()),
    );
    out
}

#[test]
fn looks_match_the_wheel() {
    check("Look", look_cases(), compare_look);
}

fn view_transform_cases() -> Vec<Case<ViewTransform>> {
    let n = NAMES.len();
    let transforms = transforms();
    (0..n)
        .map(|k| {
            let reference = if k % 2 == 0 {
                ReferenceSpaceType::Scene
            } else {
                ReferenceSpaceType::Display
            };
            let ref_name = match reference {
                ReferenceSpaceType::Scene => "REFERENCE_SPACE_SCENE",
                ReferenceSpaceType::Display => "REFERENCE_SPACE_DISPLAY",
            };
            let name = NAMES[k];
            let other = NAMES[(k + 3) % n];
            let mut ops: Vec<Op<ViewTransform>> = vec![
                op(
                    json!({"call": "setName", "args": [s(name)]}),
                    move |v: &mut ViewTransform| {
                        v.set_name(name);
                        None
                    },
                ),
                op(
                    json!({"call": "setFamily", "args": [s(other)]}),
                    move |v: &mut ViewTransform| {
                        v.set_family(other);
                        None
                    },
                ),
                op(
                    json!({"call": "setDescription", "args": [s(name)]}),
                    move |v: &mut ViewTransform| {
                        v.set_description(name);
                        None
                    },
                ),
                op(
                    json!({"call": "addCategory", "args": [s(other)]}),
                    move |v: &mut ViewTransform| {
                        v.add_category(other);
                        None
                    },
                ),
                op(
                    json!({"call": "setInterchangeAttribute",
                        "args": [s("amf_transform_ids"), s(other)]}),
                    move |v: &mut ViewTransform| {
                        v.set_interchange_attribute("amf_transform_ids", other)
                            .err()
                            .map(|e| e.what().to_vec())
                    },
                ),
            ];
            for (j, dir) in [
                (ViewTransformDirection::ToReference, "VIEWTRANSFORM_DIR_TO_REFERENCE"),
                (ViewTransformDirection::FromReference, "VIEWTRANSFORM_DIR_FROM_REFERENCE"),
            ]
            .into_iter()
            .enumerate()
            {
                if (k + j) % 3 == 1 {
                    continue;
                }
                let (spec, port) = transforms[(k + 2 * j) % transforms.len()].clone();
                ops.push(op(
                    json!({"call": "setTransform", "args": [{"transform": spec}, enum_spec(dir.1)]}),
                    move |v: &mut ViewTransform| {
                        v.set_transform(Some(&port), dir.0);
                        None
                    },
                ));
            }
            Case {
                label: format!("ViewTransform {k}"),
                new_args: json!([enum_spec(ref_name)]),
                port: ViewTransform::new(reference),
                ops,
            }
        })
        .collect()
}

fn compare_view_transform(v: &ViewTransform, g: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff(&mut out, "getName", text(&g["getName"]), v.name().to_vec());
    diff(
        &mut out,
        "getFamily",
        text(&g["getFamily"]),
        v.family().to_vec(),
    );
    let categories: Vec<Vec<u8>> = (0..v.num_categories())
        .map(|i| v.category(i).unwrap().to_vec())
        .collect();
    diff(
        &mut out,
        "getCategories",
        texts(&g["getCategories"]),
        categories,
    );
    out
}

#[test]
fn view_transforms_match_the_wheel() {
    check(
        "ViewTransform",
        view_transform_cases(),
        compare_view_transform,
    );
}

fn named_transform_cases() -> Vec<Case<NamedTransform>> {
    let n = NAMES.len();
    let transforms = transforms();
    (0..n)
        .map(|k| {
            let name = NAMES[k];
            let other = NAMES[(k + 1) % n];
            let third = NAMES[(k + 5) % n];
            let mut ops: Vec<Op<NamedTransform>> = vec![
                op(
                    json!({"call": "setName", "args": [s(name)]}),
                    move |t: &mut NamedTransform| {
                        t.set_name(name);
                        None
                    },
                ),
                op(
                    json!({"call": "addAlias", "args": [s(other)]}),
                    move |t: &mut NamedTransform| {
                        t.add_alias(other);
                        None
                    },
                ),
                op(
                    json!({"call": "addAlias", "args": [s(third)]}),
                    move |t: &mut NamedTransform| {
                        t.add_alias(third);
                        None
                    },
                ),
                op(
                    json!({"call": "setFamily", "args": [s(third)]}),
                    move |t: &mut NamedTransform| {
                        t.set_family(third);
                        None
                    },
                ),
                op(
                    json!({"call": "setDescription", "args": [s(other)]}),
                    move |t: &mut NamedTransform| {
                        t.set_description(other);
                        None
                    },
                ),
                op(
                    json!({"call": "setEncoding", "args": [s(name)]}),
                    move |t: &mut NamedTransform| {
                        t.set_encoding(name);
                        None
                    },
                ),
                op(
                    json!({"call": "addCategory", "args": [s(other)]}),
                    move |t: &mut NamedTransform| {
                        t.add_category(other);
                        None
                    },
                ),
                op(
                    json!({"call": "addCategory", "args": [s("input")]}),
                    |t: &mut NamedTransform| {
                        t.add_category("input");
                        None
                    },
                ),
            ];
            for (j, dir) in [
                (TransformDirection::Forward, "TRANSFORM_DIR_FORWARD"),
                (TransformDirection::Inverse, "TRANSFORM_DIR_INVERSE"),
            ]
            .into_iter()
            .enumerate()
            {
                if (k + j) % 3 == 0 {
                    continue;
                }
                let (spec, port) = transforms[(k + j) % transforms.len()].clone();
                ops.push(op(
                    json!({"call": "setTransform", "args": [{"transform": spec}, enum_spec(dir.1)]}),
                    move |t: &mut NamedTransform| {
                        t.set_transform(Some(&port), dir.0);
                        None
                    },
                ));
            }
            Case {
                label: format!("NamedTransform {k}"),
                new_args: json!([]),
                port: NamedTransform::new(),
                ops,
            }
        })
        .collect()
}

fn compare_named_transform(t: &NamedTransform, g: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff(&mut out, "getName", text(&g["getName"]), t.name().to_vec());
    let aliases: Vec<Vec<u8>> = (0..t.num_aliases()).map(|i| t.alias(i).to_vec()).collect();
    diff(&mut out, "getAliases", texts(&g["getAliases"]), aliases);
    diff(
        &mut out,
        "getEncoding",
        text(&g["getEncoding"]),
        t.encoding().to_vec(),
    );
    out
}

#[test]
fn named_transforms_match_the_wheel() {
    check(
        "NamedTransform",
        named_transform_cases(),
        compare_named_transform,
    );
}

/// A set built by adding color spaces whose names and aliases collide in every way: each add's
/// error, then the names in order and each name's lookups.
#[test]
fn color_space_sets_match_the_wheel() {
    let spaces: [(&str, &[&str]); 9] = [
        ("cs1", &["one", "First"]),
        ("cs2", &[]),
        ("ONE", &[]),
        ("cs3", &["CS2"]),
        ("CS1", &["one", "uno"]),
        ("", &[]),
        ("cs4", &["first"]),
        ("cs5", &["cs5", "five"]),
        ("caf\u{e9}", &["CAF\u{e9}"]),
    ];
    let mut calls = vec![json!({"new": "ColorSpaceSet", "as": "set"})];
    let mut port = ColorSpaceSet::new();
    let mut port_errors = Vec::new();
    for (k, (name, aliases)) in spaces.iter().enumerate() {
        let cs = format!("cs{k}");
        calls.push(json!({"new": "ColorSpace", "as": cs}));
        calls.push(json!({"call": "setName", "on": cs, "args": [name]}));
        let mut c = ColorSpace::new();
        c.set_name(name);
        for alias in *aliases {
            calls.push(json!({"call": "addAlias", "on": cs, "args": [alias]}));
            c.add_alias(alias);
        }
        calls.push(json!({"call": "addColorSpace", "on": "set", "args": [{"ref": cs}]}));
        port_errors.push(port.add_color_space(&c).err().map(|e| e.what().to_vec()));
    }
    let lookups = [
        "cs1",
        "one",
        "FIRST",
        "uno",
        "cs2",
        "Cs2",
        "cs3",
        "five",
        "CAF\u{e9}",
        "",
        "none",
    ];
    for name in lookups {
        calls.push(json!({"call": "getColorSpace", "on": "set", "args": [name]}));
        calls.push(json!({"call": "hasColorSpace", "on": "set", "args": [name]}));
    }
    calls.push(json!({"call": "removeColorSpace", "on": "set", "args": ["one"]}));
    calls.push(json!({"call": "removeColorSpace", "on": "set", "args": ["cs2"]}));
    calls.push(json!({"call": "getColorSpaceNames", "on": "set"}));
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "raw", "calls": calls}),
            &[],
        )
        .result;
    assert_logs_empty(&response);
    let results = response["calls"].as_array().expect("the calls' results");
    let mut wheel_errors = Vec::new();
    let mut lookup_results = Vec::new();
    let mut i = 1;
    for (_, aliases) in &spaces {
        i += 2 + aliases.len();
        wheel_errors.push(
            results[i]
                .get("exception")
                .map(|_| exception(&results[i]).1),
        );
        i += 1;
    }
    assert_eq!(wheel_errors, port_errors);
    for name in lookups {
        lookup_results.push((
            results[i]["result"].get("repr").map(bytes),
            results[i + 1]["result"].as_bool().unwrap(),
        ));
        i += 2;
        assert_eq!(
            *lookup_results.last().unwrap(),
            (
                port.color_space(name).map(ColorSpace::to_bytes),
                port.has_color_space(name)
            ),
            "{name:?}"
        );
    }
    port.remove_color_space("one");
    port.remove_color_space("cs2");
    let port_names: Vec<Vec<u8>> = (0..port.num_color_spaces())
        .map(|k| port.color_space_name_by_index(k).unwrap().to_vec())
        .collect();
    assert_eq!(texts(&results[i + 2]["result"]), port_names);
}

/// `addColorSpaces` adds the other set's color spaces in order and stops at the first one it
/// refuses: the error, and the names it added before.
#[test]
fn add_color_spaces_stops_at_the_first_refusal() {
    let spaces: [(&str, &str, &[&str]); 4] = [
        ("a", "cs1", &["x"]),
        ("b", "cs2", &[]),
        ("b", "cs3", &["X"]),
        ("b", "cs4", &[]),
    ];
    let mut calls = vec![
        json!({"new": "ColorSpaceSet", "as": "a"}),
        json!({"new": "ColorSpaceSet", "as": "b"}),
    ];
    let mut port_a = ColorSpaceSet::new();
    let mut port_b = ColorSpaceSet::new();
    for (k, (set, name, aliases)) in spaces.iter().enumerate() {
        let cs = format!("c{k}");
        calls.push(json!({"new": "ColorSpace", "as": cs}));
        calls.push(json!({"call": "setName", "on": cs, "args": [name]}));
        let mut c = ColorSpace::new();
        c.set_name(name);
        for alias in *aliases {
            calls.push(json!({"call": "addAlias", "on": cs, "args": [alias]}));
            c.add_alias(alias);
        }
        calls.push(json!({"call": "addColorSpace", "on": set, "args": [{"ref": cs}]}));
        let port = if *set == "a" {
            &mut port_a
        } else {
            &mut port_b
        };
        port.add_color_space(&c).unwrap();
    }
    calls.push(json!({"call": "addColorSpaces", "on": "a", "args": [{"ref": "b"}]}));
    calls.push(json!({"call": "getColorSpaceNames", "on": "a"}));
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "raw", "calls": calls}),
            &[],
        )
        .result;
    assert_logs_empty(&response);
    let results = response["calls"].as_array().expect("the calls' results");
    let n = results.len();
    let error = port_a.add_color_spaces(&port_b).unwrap_err();
    assert_eq!(exception(&results[n - 2]).1, error.what());
    let names: Vec<Vec<u8>> = (0..port_a.num_color_spaces())
        .map(|k| port_a.color_space_name_by_index(k).unwrap().to_vec())
        .collect();
    assert_eq!(texts(&results[n - 1]["result"]), names);
}

/// A named transform's transform set to null is removed: `getTransform` gives `None` again,
/// and the text no longer shows it.
#[test]
fn a_named_transform_set_to_null_has_no_transform() {
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "raw", "calls": [
                {"new": "NamedTransform", "as": "n"},
                {"call": "setName", "on": "n", "args": ["NewName"]},
                {"call": "setTransform", "on": "n", "args": [
                    {"transform": {"class": "MatrixTransform"}}, {"enum": "TRANSFORM_DIR_FORWARD"}]},
                {"call": "setTransform", "on": "n", "args": [
                    null, {"enum": "TRANSFORM_DIR_FORWARD"}]},
                {"call": "getTransform", "on": "n", "args": [{"enum": "TRANSFORM_DIR_FORWARD"}]},
                {"call": "__repr__", "on": "n"},
            ]}),
            &[],
        )
        .result;
    assert_logs_empty(&response);
    let results = response["calls"].as_array().expect("the calls' results");
    let mut nt = NamedTransform::new();
    nt.set_name("NewName");
    let matrix: Transform = MatrixTransform::new().into();
    nt.set_transform(Some(&matrix), TransformDirection::Forward);
    nt.set_transform(None, TransformDirection::Forward);
    assert!(results[3].get("result").is_some(), "{}", results[3]);
    assert_eq!(results[4]["result"], Value::Null);
    assert!(nt.transform(TransformDirection::Forward).is_none());
    assert_eq!(bytes(&results[5]["result"]), nt.to_bytes());
}
