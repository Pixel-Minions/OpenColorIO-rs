// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `FormatMetadata` and its iterators (chunk 6.3d) against the wheel:
//! - the class's surface, its four nested iterator classes' included;
//! - through the same step scripts on both sides: the metadata of each transform class that has
//!   some; attributes (`__setitem__`, `__getitem__` and its `KeyError`, `__contains__`, names
//!   ignoring case, `setName`, `setID`, the iterators by index, negative indices and
//!   `StopIteration`), the casters (`bytes`, a NUL, `None`, an `int`); element names and values
//!   and their errors (the root's); child elements (nested, `repr`, `clear`, invalid names);
//!   the identity of the objects the iterators return, which changes exactly when the wheel's
//!   vector of child elements reallocates (MSVC's and libstdc++'s growth differ), a reference
//!   to a slot that `clear` emptied and a later child filled; the CDL's first SOP description
//!   added and erased among other child elements; a transform's copy, a group's child.
//!
//! And, without the wheel (whose process reads freed memory there): a reference whose element
//! the wheel freed is the port's error (U-121): after its vector reallocated, after `clear`,
//! after the CDL's description was erased before it, an iterator or child of a transform that
//! died.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, by, compare, fl, i, l, n, none, r, ra, s};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// The class has the wheel's surface, member for member, its iterator classes' included.
#[test]
fn the_class_has_the_wheels_surface() {
    let report = common::wheel_surface();
    let theirs = common::wheel_member(&report, "FormatMetadata");
    let ours = common::port_member("FormatMetadata");
    assert!(
        theirs["members"].as_array().is_some_and(|m| m.len() > 20),
        "{theirs}"
    );
    assert_eq!(ours, theirs);
}

/// The classes whose instances have metadata (the log camera's made with its one argument).
const WITH_METADATA: [&str; 9] = [
    "OCIO.MatrixTransform",
    "OCIO.RangeTransform",
    "OCIO.GroupTransform",
    "OCIO.CDLTransform",
    "OCIO.ExponentTransform",
    "OCIO.ExponentWithLinearTransform",
    "OCIO.LogTransform",
    "OCIO.LogAffineTransform",
    "OCIO.LogCameraTransform",
];

/// Child `k` of the element `md` (a step's result), through a new child element iterator.
fn child(sc: &mut Script, md: usize, k: i128) -> usize {
    let kids = sc.call(ra(md, "getChildElements"), vec![]);
    sc.call(ra(kids, "__getitem__"), vec![i(k)])
}

/// Each class's metadata: its root, by default; the same object twice.
fn defaults_script() -> Script {
    let mut sc = Script::new();
    for class in WITH_METADATA {
        let t = if class == "OCIO.LogCameraTransform" {
            sc.call(n(class), vec![fl(&[0.1, 0.2, 0.3])])
        } else {
            sc.call(n(class), vec![])
        };
        let md = sc.call(ra(t, "getFormatMetadata"), vec![]);
        sc.eval(
            r(md),
            &[
                "getElementName",
                "getElementValue",
                "getName",
                "getID",
                "__len__",
                "__repr__",
            ],
        );
        let md2 = sc.call(ra(t, "getFormatMetadata"), vec![]);
        sc.call(n("operator.is_"), vec![r(md), r(md2)]);
        sc.call(n("list"), vec![r(md)]);
        let kids = sc.call(ra(md, "getChildElements"), vec![]);
        sc.call(n("list"), vec![r(kids)]);
    }
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_03")],
    );
    let md = sc.call(ra(ff, "getFormatMetadata"), vec![]);
    sc.eval(r(md), &["getElementName", "__repr__"]);
    sc
}

/// Attributes.
fn attributes_script() -> Script {
    let mut sc = Script::new();
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let md = sc.call(ra(m, "getFormatMetadata"), vec![]);
    sc.call(ra(md, "__setitem__"), vec![s("att1"), s("val1")]);
    sc.call(ra(md, "__setitem__"), vec![by(b"Att2"), by(b"v\xc3\xa9")]);
    sc.call(ra(md, "__setitem__"), vec![s("a\0b"), s("c\0d")]);
    sc.call(ra(md, "__setitem__"), vec![s("ATT1"), s("upper")]);
    sc.call(ra(md, "__setitem__"), vec![s("att1"), s("replaced")]);
    sc.raises(ra(md, "__setitem__"), vec![s(""), s("x")]);
    sc.raises(ra(md, "__setitem__"), vec![none(), s("x")]);
    sc.raises(ra(md, "__setitem__"), vec![s("x"), none()]);
    sc.raises(ra(md, "__setitem__"), vec![s("x"), i(1)]);
    sc.call(n("len"), vec![r(md)]);
    sc.repr(r(md));
    for name in ["att1", "ATT1", "aTt2", "a", "a\0b", "nope", ""] {
        let expect = if ["nope", "a\0b", ""].contains(&name) {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(
            serde_json::json!({ "call": ra(md, "__getitem__"), "args": [s(name)] }),
            expect,
        );
        sc.call(ra(md, "__contains__"), vec![s(name)]);
    }
    sc.raises(ra(md, "__getitem__"), vec![by(b"x\0y")]);
    sc.raises(ra(md, "__getitem__"), vec![by(b"\xff")]);
    sc.raises(ra(md, "__getitem__"), vec![none()]);
    sc.raises(ra(md, "__contains__"), vec![none()]);
    sc.call(ra(md, "__contains__"), vec![by(b"att2")]);
    // The name and the id, which are attributes too.
    sc.call(ra(md, "setName"), vec![s("the name")]);
    sc.call(ra(md, "setID"), vec![s("the id")]);
    sc.call(ra(md, "__setitem__"), vec![s("NAME"), s("other")]);
    sc.call(ra(md, "getName"), vec![]);
    sc.call(ra(md, "getID"), vec![]);
    sc.raises(ra(md, "setName"), vec![none()]);
    sc.raises(ra(md, "setID"), vec![i(3)]);
    sc.call(ra(md, "setName"), vec![s("")]);
    sc.call(ra(md, "getName"), vec![]);
    // Iterators.
    let names = sc.call(n("iter"), vec![r(md)]);
    sc.call(n("type"), vec![r(names)]);
    sc.call(n("len"), vec![r(names)]);
    for k in [0, 1, 6, 7, -1, -7, 100] {
        let expect = if k >= 7 {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(
            serde_json::json!({ "call": ra(names, "__getitem__"), "args": [i(k)] }),
            expect,
        );
    }
    sc.raises(ra(names, "__getitem__"), vec![s("0")]);
    sc.call(n("next"), vec![r(names)]);
    sc.call(n("list"), vec![r(names)]);
    sc.raises(n("next"), vec![r(names)]);
    let it2 = sc.call(ra(names, "__iter__"), vec![]);
    sc.call(n("operator.is_"), vec![r(it2), r(names)]);
    let attrs = sc.call(ra(md, "getAttributes"), vec![]);
    sc.call(n("type"), vec![r(attrs)]);
    sc.call(n("len"), vec![r(attrs)]);
    for k in [0, 6, -1, 7] {
        let expect = if k >= 7 {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(
            serde_json::json!({ "call": ra(attrs, "__getitem__"), "args": [i(k)] }),
            expect,
        );
    }
    sc.call(n("next"), vec![r(attrs)]);
    // The iterators see later changes.
    sc.call(ra(md, "__setitem__"), vec![s("late"), s("1")]);
    sc.call(n("len"), vec![r(attrs)]);
    sc.call(n("list"), vec![r(attrs)]);
    sc.raises(n("next"), vec![r(attrs)]);
    // An attribute value that isn't UTF-8.
    sc.call(ra(md, "__setitem__"), vec![s("bad"), by(b"\xff")]);
    sc.raises(ra(md, "__getitem__"), vec![s("bad")]);
    sc.raises(n("repr"), vec![r(md)]);
    let attrs2 = sc.call(ra(md, "getAttributes"), vec![]);
    sc.raises(n("list"), vec![r(attrs2)]);
    sc.call(ra(md, "clear"), vec![]);
    sc.call(n("len"), vec![r(md)]);
    sc.call(n("len"), vec![r(attrs)]);
    sc.repr(r(md));
    sc
}

/// Element names, values, child elements.
fn children_script() -> Script {
    let mut sc = Script::new();
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let md = sc.call(ra(m, "getFormatMetadata"), vec![]);
    sc.raises(ra(md, "setElementName"), vec![s("X")]);
    sc.raises(ra(md, "setElementValue"), vec![s("X")]);
    sc.raises(ra(md, "setElementName"), vec![none()]);
    sc.raises(ra(md, "addChildElement"), vec![s(""), s("v")]);
    sc.raises(ra(md, "addChildElement"), vec![s("ROOT"), s("v")]);
    sc.raises(ra(md, "addChildElement"), vec![s("\0x"), s("v")]);
    sc.raises(ra(md, "addChildElement"), vec![s("C"), none()]);
    sc.raises(ra(md, "addChildElement"), vec![i(42), s("v")]);
    sc.call(ra(md, "addChildElement"), vec![s("C1"), s("V1")]);
    sc.call(ra(md, "addChildElement"), vec![by(b"C2"), s("")]);
    sc.call(ra(md, "addChildElement"), vec![s("C3"), s("V3\0tail")]);
    let kids = sc.call(ra(md, "getChildElements"), vec![]);
    sc.call(n("type"), vec![r(kids)]);
    sc.call(n("len"), vec![r(kids)]);
    let c1 = sc.call(ra(kids, "__getitem__"), vec![i(0)]);
    sc.call(n("type"), vec![r(c1)]);
    let c1b = child(&mut sc, md, 0);
    sc.call(n("operator.is_"), vec![r(c1), r(c1b)]);
    for k in [3, -1, 100] {
        sc.raises(ra(kids, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(kids, "__getitem__"), vec![none()]);
    sc.eval(r(c1), &["getElementName", "getElementValue", "__repr__"]);
    sc.call(ra(c1, "setElementName"), vec![s("C1x")]);
    sc.call(ra(c1, "setElementValue"), vec![s("V1x")]);
    sc.raises(ra(c1, "setElementName"), vec![s("ROOT")]);
    sc.raises(ra(c1, "setElementName"), vec![s("")]);
    sc.call(ra(c1, "addChildElement"), vec![s("C4"), s("V4")]);
    sc.call(ra(c1, "addChildElement"), vec![s("C5"), s("V5")]);
    let c5 = child(&mut sc, c1, 1);
    sc.call(ra(c5, "addChildElement"), vec![s("C6"), s("V6")]);
    sc.call(ra(c5, "__setitem__"), vec![s("k"), s("v")]);
    sc.repr(r(md));
    sc.repr(r(m));
    // clear keeps the name, drops the rest.
    sc.call(ra(c5, "clear"), vec![]);
    sc.eval(
        r(c5),
        &["getElementName", "getElementValue", "__len__", "__repr__"],
    );
    sc.repr(r(md));
    sc.call(ra(c1, "getID"), vec![]);
    // Iterating: each child, then StopIteration.
    let it = sc.call(ra(md, "getChildElements"), vec![]);
    let first = sc.call(n("next"), vec![r(it)]);
    sc.call(n("operator.is_"), vec![r(first), r(c1)]);
    sc.call(n("next"), vec![r(it)]);
    sc.call(n("next"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    let it2 = sc.call(ra(it, "__iter__"), vec![]);
    sc.call(n("operator.is_"), vec![r(it2), r(it)]);
    sc.call(n("len"), vec![r(it)]);
    // (The vector reallocates: c1 and c5 dangle from here on, U-121.)
    sc.call(ra(md, "addChildElement"), vec![s("C7"), s("")]);
    sc.call(n("len"), vec![r(it)]);
    sc.call(n("next"), vec![r(it)]);
    sc
}

/// Adds `count` children named `prefix<k>` to the element `md`.
fn add_children(sc: &mut Script, md: usize, prefix: &str, count: usize) {
    for k in 0..count {
        sc.call(
            ra(md, "addChildElement"),
            vec![s(&format!("{prefix}{k}")), s("v")],
        );
    }
}

/// A matrix transform and its metadata: the steps' indices.
fn matrix_metadata(sc: &mut Script) -> (usize, usize) {
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let md = sc.call(ra(m, "getFormatMetadata"), vec![]);
    (m, md)
}

// The objects the child iterators return are the same while the vector keeps its buffer, new
// ones once it reallocates, which the wheels' C++ runtimes do at different sizes (MSVC's grows a
// full vector by half: 1, 2, 3, 4, 6, 9, 13; libstdc++ doubles it: 1, 2, 4, 8, 16). A script
// keeps every result alive, and the wheel's allocator may put a later buffer at the address of
// one it freed, where pybind11 then finds the stale object (U-121): so each script reallocates
// a vector once at most while it holds an object of the old buffer, and allocates nothing
// after.

/// The root's vector of `count` children given one more: its first child's object before and
/// after.
fn growth_script(count: usize) -> Script {
    let mut sc = Script::new();
    let (_, md) = matrix_metadata(&mut sc);
    add_children(&mut sc, md, "C", count);
    let c0 = child(&mut sc, md, 0);
    let last = child(&mut sc, md, count as i128 - 1);
    add_children(&mut sc, md, "D", 1);
    let c0b = child(&mut sc, md, 0);
    let lastb = child(&mut sc, md, count as i128 - 1);
    sc.call(n("operator.is_"), vec![r(c0), r(c0b)]);
    sc.call(n("operator.is_"), vec![r(last), r(lastb)]);
    sc.call(n("operator.is_"), vec![r(c0), r(last)]);
    sc
}

/// A child's vector of `count` children given one more; then (with 4 children at the root,
/// a size at which both runtimes reallocate) the root's growth, which copies the child and
/// its vector.
fn nested_growth_script(count: usize) -> Script {
    let mut sc = Script::new();
    let (_, md) = matrix_metadata(&mut sc);
    add_children(&mut sc, md, "C", 4);
    let c3 = child(&mut sc, md, 3);
    add_children(&mut sc, c3, "G", count);
    let g0 = child(&mut sc, c3, 0);
    add_children(&mut sc, c3, "H", 1);
    let g0b = child(&mut sc, c3, 0);
    sc.call(n("operator.is_"), vec![r(g0), r(g0b)]);
    let c3b = child(&mut sc, md, 3);
    sc.call(n("operator.is_"), vec![r(c3), r(c3b)]);
    add_children(&mut sc, md, "D", 1);
    let c3c = child(&mut sc, md, 3);
    sc.call(n("operator.is_"), vec![r(c3c), r(c3)]);
    let g0c = child(&mut sc, c3c, 0);
    sc.call(n("operator.is_"), vec![r(g0c), r(g0b)]);
    sc.eval(r(g0c), &["getElementName", "__repr__"]);
    sc.repr(r(md));
    sc
}

/// A copy has its own metadata; a group's child is the transform given.
fn copy_script() -> Script {
    let mut sc = Script::new();
    let (m, md) = matrix_metadata(&mut sc);
    add_children(&mut sc, md, "C", 3);
    let copy = sc.call(n("copy.deepcopy"), vec![r(m)]);
    let cmd = sc.call(ra(copy, "getFormatMetadata"), vec![]);
    sc.call(n("operator.is_"), vec![r(cmd), r(md)]);
    sc.repr(r(cmd));
    sc.call(ra(cmd, "addChildElement"), vec![s("only"), s("copy")]);
    let kids = sc.call(ra(md, "getChildElements"), vec![]);
    sc.call(n("len"), vec![r(kids)]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m)])]);
    let g0 = sc.call(ra(g, "__getitem__"), vec![i(0)]);
    let gmd = sc.call(ra(g0, "getFormatMetadata"), vec![]);
    sc.call(n("operator.is_"), vec![r(gmd), r(md)]);
    sc
}

/// `clear`, then children in the emptied slots: the old objects read them.
fn clear_identity_script() -> Script {
    let mut sc = Script::new();
    let (_, md) = matrix_metadata(&mut sc);
    add_children(&mut sc, md, "C", 12);
    let c5 = child(&mut sc, md, 5);
    let c6 = child(&mut sc, md, 6);
    sc.call(ra(md, "clear"), vec![]);
    sc.call(n("len"), vec![r(md)]);
    add_children(&mut sc, md, "N", 7);
    sc.eval(r(c5), &["getElementName", "__repr__"]);
    let again = child(&mut sc, md, 6);
    sc.call(n("operator.is_"), vec![r(again), r(c6)]);
    sc.eval(r(c6), &["getElementName"]);
    sc.repr(r(md));
    sc
}

/// The CDL's first SOP description, a child element the transform adds and erases among
/// others: the references to the others follow the wheel's slots.
fn cdl_script() -> Script {
    let mut sc = Script::new();
    let cdl = sc.call(n("OCIO.CDLTransform"), vec![]);
    let md = sc.call(ra(cdl, "getFormatMetadata"), vec![]);
    sc.call(ra(md, "addChildElement"), vec![s("A"), s("a")]);
    sc.call(ra(cdl, "setFirstSOPDescription"), vec![s("desc")]);
    sc.call(ra(md, "addChildElement"), vec![s("B"), s("b")]);
    sc.call(ra(md, "addChildElement"), vec![s("C"), s("c")]);
    let b0 = child(&mut sc, md, 2);
    sc.call(ra(b0, "addChildElement"), vec![s("B1"), s("b1")]);
    let a0 = child(&mut sc, md, 0);
    sc.repr(r(md));
    sc.call(ra(cdl, "getFirstSOPDescription"), vec![]);
    sc.call(ra(cdl, "setFirstSOPDescription"), vec![s("other")]);
    sc.repr(r(md));
    // Erased: B and C move down a slot.
    sc.call(ra(cdl, "setFirstSOPDescription"), vec![s("")]);
    sc.repr(r(md));
    sc.eval(r(a0), &["getElementName"]);
    sc.eval(r(b0), &["getElementName", "__repr__"]);
    let b1 = child(&mut sc, md, 1);
    sc.eval(r(b1), &["getElementName", "__repr__"]);
    let b2 = child(&mut sc, md, 1);
    sc.call(n("operator.is_"), vec![r(b1), r(b2)]);
    // Added again, at the end.
    sc.call(ra(cdl, "setFirstSOPDescription"), vec![s("again")]);
    sc.repr(r(md));
    sc.call(n("len"), vec![r(md)]);
    sc.raises(ra(cdl, "setFirstSOPDescription"), vec![none()]);
    sc
}

/// A CDL whose first SOP description (child 0) is erased, the children after it copy-assigned
/// a slot down: `b` children under the element after it, `c` under the next. Then a reference
/// to the grandchild under slot 1 made before (`b` > 0), and one made after and a child added.
fn erase_script(b: usize, c: usize) -> Script {
    let mut sc = Script::new();
    let cdl = sc.call(n("OCIO.CDLTransform"), vec![]);
    let md = sc.call(ra(cdl, "getFormatMetadata"), vec![]);
    sc.call(ra(cdl, "setFirstSOPDescription"), vec![s("desc")]);
    add_children(&mut sc, md, "B", 1);
    add_children(&mut sc, md, "C", 1);
    let eb = child(&mut sc, md, 1);
    add_children(&mut sc, eb, "b", b);
    let ec = child(&mut sc, md, 2);
    add_children(&mut sc, ec, "c", c);
    let before = (b > 0 && c <= b).then(|| child(&mut sc, eb, 0));
    sc.call(ra(cdl, "setFirstSOPDescription"), vec![s("")]);
    sc.repr(r(md));
    let e1 = child(&mut sc, md, 1);
    sc.call(n("operator.is_"), vec![r(e1), r(eb)]);
    if c > 0 {
        let g = child(&mut sc, e1, 0);
        if let Some(before) = before {
            sc.call(n("operator.is_"), vec![r(g), r(before)]);
            sc.eval(r(before), &["getElementName"]);
        }
        add_children(&mut sc, e1, "x", 1);
        let g2 = child(&mut sc, e1, 0);
        sc.call(n("operator.is_"), vec![r(g), r(g2)]);
    }
    sc.repr(r(md));
    sc
}

#[test]
fn an_erased_description_moves_the_children_as_the_wheels() {
    let mut scripts = Vec::new();
    for (b, c) in [
        (0, 1),
        (1, 1),
        (1, 3),
        (2, 1),
        (4, 2),
        (3, 3),
        (2, 4),
        (4, 5),
    ] {
        scripts.push(erase_script(b, c));
    }
    compare(&scripts);
}

#[test]
fn format_metadata_behaves_as_the_wheels() {
    compare(&[
        defaults_script(),
        attributes_script(),
        children_script(),
        copy_script(),
        clear_identity_script(),
        cdl_script(),
    ]);
}

#[test]
fn child_objects_change_where_the_wheels_vectors_reallocate() {
    let mut scripts: Vec<Script> = (1..=13).map(growth_script).collect();
    scripts.extend((1..=9).map(nested_growth_script));
    compare(&scripts);
}

/// Runs `code` against the port's module, in globals of its own.
fn run(py: Python<'_>, globals: &Bound<'_, PyDict>, code: &str) -> PyResult<()> {
    let code = std::ffi::CString::new(code).unwrap();
    py.run(&code, Some(globals), None)
}

/// The port's error for each reference whose element the wheel freed (U-121); the same steps
/// short of the dangling read run.
#[test]
fn a_reference_to_freed_memory_is_the_ports_error() {
    common::start_python();
    Python::attach(|py| {
        let cases: &[(&str, &str)] = &[
            // The vector reallocates (its capacity 1, then 2).
            (
                "md = OCIO.MatrixTransform().getFormatMetadata()\n\
                 md.addChildElement('A', 'a')\n\
                 a = md.getChildElements()[0]\n\
                 md.addChildElement('B', 'b')",
                "a.getElementName()",
            ),
            // A grandchild, copied when its grandparent's vector reallocates.
            (
                "md = OCIO.MatrixTransform().getFormatMetadata()\n\
                 md.addChildElement('A', 'a')\n\
                 md.getChildElements()[0].addChildElement('G', 'g')\n\
                 g = md.getChildElements()[0].getChildElements()[0]\n\
                 md.addChildElement('B', 'b')",
                "g.getElementName()",
            ),
            // An iterator over a child, whose vector reallocated since.
            (
                "md = OCIO.MatrixTransform().getFormatMetadata()\n\
                 md.addChildElement('A', 'a')\n\
                 it = md.getChildElements()[0].getChildElements()\n\
                 md.addChildElement('B', 'b')",
                "len(it)",
            ),
            // clear destroys the child.
            (
                "md = OCIO.MatrixTransform().getFormatMetadata()\n\
                 md.addChildElement('A', 'a')\n\
                 a = md.getChildElements()[0]\n\
                 md.clear()",
                "a.getElementValue()",
            ),
            // ... and its children, whose slots a new child doesn't bring back.
            (
                "md = OCIO.MatrixTransform().getFormatMetadata()\n\
                 md.addChildElement('A', 'a')\n\
                 md.getChildElements()[0].addChildElement('G', 'g')\n\
                 g = md.getChildElements()[0].getChildElements()[0]\n\
                 md.clear()\n\
                 md.addChildElement('A', 'a')\n\
                 md.getChildElements()[0].addChildElement('G', 'g')",
                "g.getElementName()",
            ),
            // The CDL's description erased: the last slot is gone.
            (
                "cdl = OCIO.CDLTransform()\n\
                 md = cdl.getFormatMetadata()\n\
                 cdl.setFirstSOPDescription('d')\n\
                 md.addChildElement('B', 'b')\n\
                 last = md.getChildElements()[1]\n\
                 cdl.setFirstSOPDescription('')",
                "last.getElementName()",
            ),
            // The erased element's children.
            (
                "cdl = OCIO.CDLTransform()\n\
                 md = cdl.getFormatMetadata()\n\
                 cdl.setFirstSOPDescription('d')\n\
                 md.getChildElements()[0].addChildElement('G', 'g')\n\
                 md.addChildElement('B', 'b')\n\
                 g = md.getChildElements()[0].getChildElements()[0]\n\
                 cdl.setFirstSOPDescription('')",
                "g.getElementName()",
            ),
            // The last slot's element destroyed with its children: a new last child and
            // grandchild there are other objects.
            (
                "cdl = OCIO.CDLTransform()\n\
                 md = cdl.getFormatMetadata()\n\
                 cdl.setFirstSOPDescription('d')\n\
                 md.addChildElement('B', 'b')\n\
                 md.getChildElements()[1].addChildElement('G', 'g')\n\
                 g = md.getChildElements()[1].getChildElements()[0]\n\
                 cdl.setFirstSOPDescription('')\n\
                 md.addChildElement('N', 'n')\n\
                 md.getChildElements()[1].addChildElement('H', 'h')",
                "g.getElementName()",
            ),
            // A copy-assignment that destroyed the slot's child, with its children: a new child
            // and grandchild there are other objects.
            (
                "cdl = OCIO.CDLTransform()\n\
                 md = cdl.getFormatMetadata()\n\
                 cdl.setFirstSOPDescription('d')\n\
                 md.addChildElement('B', 'b')\n\
                 md.addChildElement('C', 'c')\n\
                 b = md.getChildElements()[1]\n\
                 b.addChildElement('B0', '')\n\
                 b.addChildElement('B1', '')\n\
                 b.getChildElements()[1].addChildElement('G', 'g')\n\
                 g = b.getChildElements()[1].getChildElements()[0]\n\
                 md.getChildElements()[2].addChildElement('C0', '')\n\
                 cdl.setFirstSOPDescription('')\n\
                 b = md.getChildElements()[1]\n\
                 b.addChildElement('N', '')\n\
                 b.getChildElements()[1].addChildElement('H', 'h')",
                "g.getElementName()",
            ),
            // A copy-assignment that reallocated the slot's vector.
            (
                "cdl = OCIO.CDLTransform()\n\
                 md = cdl.getFormatMetadata()\n\
                 cdl.setFirstSOPDescription('d')\n\
                 md.addChildElement('B', 'b')\n\
                 md.addChildElement('C', 'c')\n\
                 md.getChildElements()[1].addChildElement('G', 'g')\n\
                 for k in range(3): md.getChildElements()[2].addChildElement('H', 'h')\n\
                 g = md.getChildElements()[1].getChildElements()[0]\n\
                 cdl.setFirstSOPDescription('')",
                "g.getElementName()",
            ),
            // An iterator over a transform that died (the metadata object it came from too).
            (
                "it = OCIO.MatrixTransform().getFormatMetadata().getChildElements()\n\
                 gc.collect()",
                "len(it)",
            ),
            (
                "it = OCIO.MatrixTransform().getFormatMetadata().getAttributes()\n\
                 gc.collect()",
                "next(it)",
            ),
            // A child of a transform that died: it keeps its iterator alive, not the transform.
            (
                "md = OCIO.MatrixTransform().getFormatMetadata()\n\
                 md.addChildElement('A', 'a')\n\
                 a = md.getChildElements()[0]\n\
                 del md\n\
                 gc.collect()",
                "a.getElementName()",
            ),
        ];
        for (setup, read) in cases {
            let globals = PyDict::new(py);
            run(py, &globals, "import gc\nimport PyOpenColorIO as OCIO").unwrap();
            run(py, &globals, setup).unwrap_or_else(|e| panic!("{setup}: {e}"));
            let err = run(py, &globals, read).expect_err(read);
            let text = err.to_string();
            assert!(
                err.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py)
                    && text.contains("docs/improvements.md, U-121"),
                "{setup}\n{read}: {text}"
            );
        }
        // The controls: what keeps the element alive reads it.
        let globals = PyDict::new(py);
        run(
            py,
            &globals,
            "import gc\nimport PyOpenColorIO as OCIO\n\
             md = OCIO.MatrixTransform().getFormatMetadata()\n\
             gc.collect()\n\
             md.addChildElement('A', 'a')\n\
             a = md.getChildElements()[0]\n\
             gc.collect()\n\
             assert a.getElementName() == 'A'\n\
             m = OCIO.MatrixTransform()\n\
             it = m.getFormatMetadata().getChildElements()\n\
             gc.collect()\n\
             assert len(it) == 0",
        )
        .unwrap_or_else(|e| panic!("{e}"));
    });
}
