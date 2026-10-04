// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transforms' text and validation against the wheel, through the oracle's
//! `transform_text`: `str()` and `repr()` print upstream's `operator<<`
//! (src/bindings/python/PyUtils.h `defRepr` @ v2.5.2), which [`Transform`]'s `Display` ports,
//! byte for byte; `validate()` raises upstream's message.
//!
//! So far the group transform, the one class in the port: groups of groups, nested, in both
//! directions. Each class adds its cases as it lands.

use ocio::{GroupTransform, Transform, TransformDirection};
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use serde_json::{Value, json};

/// A group: its direction and its children.
#[derive(Debug, Clone)]
struct Group {
    dir: TransformDirection,
    children: Vec<Group>,
}

impl Group {
    fn spec(&self) -> Value {
        let dir = match self.dir {
            TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
            TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
        };
        json!({
            "class": "GroupTransform",
            "calls": [["setDirection", {"enum": dir}]],
            "children": self.children.iter().map(Group::spec).collect::<Vec<_>>(),
        })
    }

    fn port(&self) -> Transform {
        let mut group = GroupTransform::new();
        group.set_direction(self.dir);
        for child in &self.children {
            group.append_transform(child.port());
        }
        group.into()
    }
}

fn cases() -> Vec<Group> {
    use TransformDirection::{Forward as F, Inverse as I};
    let leaf = |dir| Group {
        dir,
        children: Vec::new(),
    };
    let node = |dir, children| Group { dir, children };
    vec![
        leaf(F),
        leaf(I),
        node(F, vec![leaf(I)]),
        node(I, vec![leaf(F), leaf(I), leaf(F)]),
        node(
            F,
            vec![node(I, vec![leaf(F), node(F, vec![leaf(I)])]), leaf(F)],
        ),
    ]
}

#[test]
fn group_text_and_validation_match_the_wheel() {
    let cases = cases();
    let reply = TransformTextRequest {
        transforms: cases.iter().map(Group::spec).collect(),
        pairs: Vec::new(),
    }
    .run();

    let mut failures = Vec::new();
    for (case, built) in cases.iter().zip(&reply.transforms) {
        let Built::Text(text) = built else {
            panic!("{case:?}: the wheel raised building it: {built:?}");
        };
        let port = case.port();
        let port_text = port.to_string();
        let port_validate = port.validate().err().map(|e| e.message().to_string());
        let wheel_validate = text.validate.as_ref().map(|e| e.message.clone());
        if port_text != text.repr || port_text != text.str || port_validate != wheel_validate {
            failures.push(format!(
                "{case:?}\n  wheel {:?} {wheel_validate:?}\n  port  {port_text:?} {port_validate:?}",
                text.repr
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A child index outside the group raises upstream's message, as the binding's `__getitem__`
/// (`GroupTransform::getTransform`) raises it while the spec is built.
#[test]
fn an_index_outside_the_group_raises_the_wheels_message() {
    let indices = [-1, 0, 1, 7, i32::MAX, i32::MIN];
    let reply = TransformTextRequest {
        transforms: indices
            .iter()
            .map(|i| json!({"class": "GroupTransform", "calls": [["__getitem__", i]]}))
            .collect(),
        pairs: Vec::new(),
    }
    .run();
    for (&index, built) in indices.iter().zip(&reply.transforms) {
        let Built::Raised(raised) = built else {
            panic!("{index}: the wheel built it: {built:?}");
        };
        let port = GroupTransform::new().transform(index).unwrap_err();
        assert_eq!(port.message(), raised.message, "{index}");
    }
}
