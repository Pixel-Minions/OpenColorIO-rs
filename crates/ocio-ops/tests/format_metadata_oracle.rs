// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `FormatMetadataImpl` against the wheel.
//!
//! - `format_metadata_ops`: the oracle makes calls on a `MatrixTransform`'s metadata (an
//!   element named `ROOT`), and the port makes the same calls on `FormatMetadataImpl::root()`.
//!   The errors, `repr()` (upstream's `operator<<`, the port's `write_to`) and every node's
//!   getters must be the same. Python passes each string to OCIO as a C string, up to its first
//!   NUL; the port receives the whole string, and must cut it the same way.
//! - `format_metadata_combine`: the wheel's optimizer composes two `MatrixTransform`s, and the
//!   composed op's metadata is the first one's combined with the second one's
//!   (`MatrixOpData::compose`, src/OpenColorIO/ops/matrix/MatrixOpData.cpp:684-686 @ v2.5.2).
//!   The port combines the same two elements with `FormatMetadataImpl::combine`.
//!
//! The calls and elements are the tests' inputs; the expected results come from the wheel.

use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_testkit::Oracle;
use serde_json::{Value, json};

fn op(name: &str, path: &[i32], args: Value) -> Value {
    let mut op = args;
    op["op"] = json!(name);
    op["path"] = json!(path);
    op
}

fn add_attribute(path: &[i32], name: &str, value: &str) -> Value {
    op("add_attribute", path, json!({"name": name, "value": value}))
}

fn set_name(path: &[i32], name: &str) -> Value {
    op("set_name", path, json!({ "name": name }))
}

fn set_id(path: &[i32], id: &str) -> Value {
    op("set_id", path, json!({ "id": id }))
}

fn add_child(path: &[i32], name: &str, value: &str) -> Value {
    op(
        "add_child_element",
        path,
        json!({"name": name, "value": value}),
    )
}

fn set_element_name(path: &[i32], name: &str) -> Value {
    op("set_element_name", path, json!({ "name": name }))
}

fn set_element_value(path: &[i32], value: &str) -> Value {
    op("set_element_value", path, json!({ "value": value }))
}

fn clear(path: &[i32]) -> Value {
    op("clear", path, json!({}))
}

fn get_child(path: &[i32], index: i32) -> Value {
    op("get_child_element", path, json!({ "index": index }))
}

/// The calls, one scenario per list. Each starts from a new `ROOT` element.
fn scenarios() -> Vec<Vec<Value>> {
    vec![
        // Attributes: their order, replacing one, and names that differ in case.
        vec![
            add_attribute(&[], "b", "1"),
            add_attribute(&[], "a", "2"),
            add_attribute(&[], "b", "3"),
        ],
        vec![add_attribute(&[], "Name", "a"), set_name(&[], "b")],
        vec![
            add_attribute(&[], "NAME", "a"),
            add_attribute(&[], "name", "b"),
            add_attribute(&[], "Name", "c"),
        ],
        vec![
            add_attribute(&[], "ID", "x"),
            set_id(&[], "y"),
            add_attribute(&[], "Id", "z"),
        ],
        vec![set_name(&[], ""), set_id(&[], "")],
        vec![set_name(&[], "n1"), set_name(&[], "n2"), set_id(&[], "i1")],
        vec![
            add_attribute(&[], "nAmE", "first"),
            add_attribute(&[], "name", "second"),
        ],
        vec![add_attribute(&[], "iD", "first"), set_id(&[], "second")],
        vec![
            add_attribute(&[], "name", "a"),
            set_name(&[], "b"),
            add_attribute(&[], "id", "c"),
            set_id(&[], "d"),
        ],
        // Empty names, NULs, non-ASCII text and characters XML would escape.
        vec![add_attribute(&[], "", "x")],
        vec![add_attribute(&[], "\0abc", "x")],
        vec![add_attribute(&[], "k\0z", "v\0w")],
        vec![
            add_attribute(&[], "k", ""),
            add_attribute(&[], "k2", "\u{e9}\u{65e5}"),
        ],
        vec![add_attribute(&[], "q", "<&>\"'\n\t")],
        // Children.
        vec![
            add_child(&[], "A", "va"),
            add_child(&[], "B", ""),
            add_child(&[1], "C", "vc"),
            add_attribute(&[1, 0], "x", "1"),
        ],
        vec![add_child(&[], "", "v")],
        vec![add_child(&[], "ROOT", "v")],
        vec![add_child(&[], "root", "v")],
        vec![add_child(&[], "ROOT\0x", "v")],
        vec![add_child(&[], "Root ", "v")],
        vec![add_child(&[], "A\0B", "v\0w")],
        vec![add_child(&[], "A", "<b>&amp;"), add_child(&[0], "Z", "\n")],
        vec![add_child(&[], "\u{e9}l", "\u{e9}")],
        // Renaming.
        vec![set_element_name(&[], "X")],
        vec![set_element_name(&[], "")],
        vec![set_element_name(&[], "ROOT")],
        vec![set_element_name(&[], "\0")],
        vec![add_child(&[], "A", ""), set_element_name(&[0], "B")],
        vec![add_child(&[], "A", ""), set_element_name(&[0], "")],
        vec![add_child(&[], "A", ""), set_element_name(&[0], "ROOT")],
        vec![add_child(&[], "A", ""), set_element_name(&[0], "root")],
        vec![add_child(&[], "A", ""), set_element_name(&[0], "ROOT\0z")],
        vec![add_child(&[], "A", ""), set_element_name(&[0], "\0")],
        vec![add_child(&[], "A", ""), set_element_name(&[0], "N\0M")],
        // Values.
        vec![set_element_value(&[], "v")],
        vec![set_element_value(&[], "")],
        vec![
            add_child(&[], "A", "old"),
            set_element_value(&[0], "new"),
            set_element_value(&[0], ""),
        ],
        vec![add_child(&[], "A", "old"), set_element_value(&[0], "x\0y")],
        // Clearing.
        vec![
            add_attribute(&[], "a", "1"),
            add_child(&[], "A", "x"),
            clear(&[]),
        ],
        vec![
            add_child(&[], "A", "x"),
            add_attribute(&[0], "a", "1"),
            add_child(&[0], "B", ""),
            clear(&[0]),
        ],
        // Child indices, and paths through them.
        vec![get_child(&[], 0)],
        vec![add_child(&[], "A", ""), get_child(&[], 1)],
        vec![add_child(&[], "A", ""), get_child(&[], -1)],
        vec![add_child(&[], "A", ""), get_child(&[], 0)],
        vec![add_child(&[], "A", ""), add_attribute(&[1], "a", "1")],
        vec![add_child(&[], "A", ""), add_attribute(&[-1], "a", "1")],
    ]
}

/// The element on `path`, through `getChildElement`.
fn node<'a>(
    root: &'a mut FormatMetadataImpl,
    path: &Value,
) -> ocio_ops::Result<&'a mut FormatMetadataImpl> {
    let mut node = root;
    for index in path.as_array().expect("a path") {
        node = node.get_child_element_mut(index.as_i64().expect("an index") as i32)?;
    }
    Ok(node)
}

/// The port's side of one call.
fn apply(root: &mut FormatMetadataImpl, op: &Value) -> ocio_ops::Result<()> {
    let string = |key: &str| Some(op[key].as_str().expect("a string").as_bytes());
    let node = node(root, &op["path"])?;
    match op["op"].as_str().expect("an operation") {
        "add_attribute" => node.add_attribute(string("name"), string("value")),
        "set_name" => {
            node.set_name(string("name"));
            Ok(())
        }
        "set_id" => {
            node.set_id(string("id"));
            Ok(())
        }
        "add_child_element" => node.add_child_element(string("name"), string("value")),
        "set_element_name" => node.set_element_name(string("name")),
        "set_element_value" => node.set_element_value(string("value")),
        "clear" => {
            node.clear();
            Ok(())
        }
        "get_child_element" => node
            .get_child_element(op["index"].as_i64().expect("an index") as i32)
            .map(|_| ()),
        other => panic!("unknown operation {other}"),
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

/// The getters the oracle reports, for `node` and its children.
fn tree(node: &FormatMetadataImpl) -> Value {
    json!({
        "element_name": text(node.get_element_name()),
        "element_value": text(node.get_element_value()),
        "attributes": (0..node.get_num_attributes())
            .map(|i| json!([text(node.get_attribute_name(i)), text(node.get_attribute_value(i))]))
            .collect::<Vec<_>>(),
        "name": text(node.get_name()),
        "id": text(node.get_id()),
        "children": (0..node.get_num_children_elements())
            .map(|i| tree(node.get_child_element(i).expect("a child")))
            .collect::<Vec<_>>(),
    })
}

fn repr(node: &FormatMetadataImpl) -> String {
    let mut out = Vec::new();
    node.write_to(&mut out);
    text(&out)
}

#[test]
fn calls_match_the_wheel() {
    let scenarios = scenarios();
    let response = Oracle::get().call(
        "format_metadata_ops",
        json!({ "scenarios": scenarios }),
        &[],
    );
    let wheel = response.result.as_array().expect("a list");
    assert_eq!(wheel.len(), scenarios.len());
    for (scenario, wheel) in scenarios.iter().zip(wheel) {
        let mut root = FormatMetadataImpl::root();
        let mut errors = Vec::new();
        for (i, op) in scenario.iter().enumerate() {
            if let Err(e) = apply(&mut root, op) {
                errors.push(json!([i, e.message()]));
            }
        }
        let scenario = Value::from(scenario.clone());
        assert_eq!(Value::from(errors), wheel["errors"], "errors of {scenario}");
        assert_eq!(repr(&root), wheel["repr"], "repr() of {scenario}");
        assert_eq!(tree(&root), wheel["tree"], "getters of {scenario}");
    }
}

/// `(first element's attributes, second element's attributes, second element's children)`.
/// The first element also gets a child `L`, and the second a child `R` before its own.
type CombineCase = (
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
);

const COMBINE_CASES: &[CombineCase] = &[
    // An empty value in the second element is skipped; an empty one in the first is replaced.
    (&[("kx", "1")], &[("zz", "")], &[]),
    (&[("kx", "")], &[("kx", "2")], &[]),
    // Names that match ignoring case are joined into the first element's attribute.
    (&[("Name", "a")], &[("NAME", "b"), ("name", "c")], &[]),
    (&[("id", "a")], &[("ID", "b")], &[]),
    (&[], &[("k", "v")], &[]),
    (
        &[("a", "1"), ("b", "2")],
        &[("B", "3"), ("c", "4"), ("A", "")],
        &[],
    ),
    // The second element's children follow the first's, in order.
    (&[("a", "1")], &[("a", "2")], &[("R2", "r2"), ("R3", "r3")]),
    // Two attributes of the first element match: the first of them is joined.
    (&[("Name", "a"), ("name", "b")], &[("NAME", "c")], &[]),
    (&[], &[], &[]),
];

#[test]
fn combine_matches_the_wheel() {
    let element = |attributes: &[(&str, &str)], children: Vec<(&str, &str)>| {
        json!({
            "attributes": attributes.iter().map(|(n, v)| json!([n, v])).collect::<Vec<_>>(),
            "children": children.iter().map(|(n, v)| json!([n, v])).collect::<Vec<_>>(),
        })
    };
    let cases: Vec<Value> = COMBINE_CASES
        .iter()
        .map(|(first, second, children)| {
            let mut second_children = vec![("R", "r")];
            second_children.extend_from_slice(children);
            json!({
                "first": element(first, vec![("L", "l")]),
                "second": element(second, second_children),
            })
        })
        .collect();
    let response = Oracle::get().call("format_metadata_combine", json!({ "cases": cases }), &[]);
    let wheel = response.result.as_array().expect("a list");
    assert_eq!(wheel.len(), cases.len());
    for (case, wheel) in cases.iter().zip(wheel) {
        let transforms = wheel["transforms"].as_array().expect("transforms");
        assert_eq!(transforms.len(), 1, "{case}: {wheel}");
        let build = |spec: &Value| {
            let mut element = FormatMetadataImpl::root();
            for pair in spec["attributes"].as_array().unwrap() {
                element
                    .add_attribute(
                        Some(pair[0].as_str().unwrap().as_bytes()),
                        Some(pair[1].as_str().unwrap().as_bytes()),
                    )
                    .unwrap();
            }
            for pair in spec["children"].as_array().unwrap() {
                element
                    .add_child_element(
                        Some(pair[0].as_str().unwrap().as_bytes()),
                        Some(pair[1].as_str().unwrap().as_bytes()),
                    )
                    .unwrap();
            }
            element
        };
        let mut combined = build(&case["first"]);
        combined.combine(&build(&case["second"])).unwrap();
        assert_eq!(repr(&combined), transforms[0]["repr"], "{case}");
    }
}
