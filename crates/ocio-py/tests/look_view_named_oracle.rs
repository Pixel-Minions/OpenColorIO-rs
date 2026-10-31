// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Look`, `NamedTransform` and `ViewTransform` with their iterators (chunk 6.3e) against the
//! wheel:
//! - each class's surface;
//! - through the same step scripts on both sides: the constructors (every parameter, positional
//!   and keyword, their refusals), every getter and setter (`None` where the wheel takes it and
//!   where `.none(false)` refuses it, `bytes`, a NUL), aliases and categories with their
//!   iterators, interchange attributes, the transforms (the same object each get, a change
//!   through it seen by its owner and its `repr`, a group's children shared, copies, a
//!   direction outside the enumerators: none for a view transform, the named transform's
//!   error), `NamedTransform.GetTransform` (the transform itself, or a copy of the other
//!   inverted, `None`'s error), copies;
//!
//! and, without the wheel: the reference space outside its enumerators (U-112; the wheel runs
//! the same steps), `None` where the wheel copies a null transform (U-124: its process dies).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, fl, i, l, n, none, r, ra, s, t};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::json;

/// Each class has the wheel's surface, member for member.
#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in ["Look", "NamedTransform", "ViewTransform"] {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 10),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// A step whose outcome the test expects, by whether it raises.
fn step(sc: &mut Script, target: serde_json::Value, args: Vec<serde_json::Value>, raises: bool) {
    let expect = if raises {
        Expect::Raised
    } else {
        Expect::Value
    };
    sc.step(json!({ "call": target, "args": args }), expect);
}

/// A matrix transform with an offset, a CDL whose slope prints with the precision a matrix
/// before it leaves (I-73), and a group of the CDL: the steps' indices.
fn transforms(sc: &mut Script) -> (usize, usize, usize) {
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(m, "setOffset"), vec![fl(&[0.1, 0.2, 0.3, 0.4])]);
    let cdl = sc.call(n("OCIO.CDLTransform"), vec![]);
    sc.call(ra(cdl, "setSlope"), vec![fl(&[0.1234567891234, 2.0, 3.0])]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(cdl)])]);
    (m, cdl, g)
}

/// A fixed function transform set into the step's result `owner` (by `set`), then made invalid
/// through the object `get` returns: the wheel prints the owner as it is, and its copy
/// (deepcopy's) refuses it.
fn invalid_held(
    sc: &mut Script,
    owner: usize,
    get: &str,
    args: Vec<serde_json::Value>,
    set: impl FnOnce(&mut Script, usize),
) {
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_03")],
    );
    set(sc, ff);
    let held = sc.call(ra(owner, get), args);
    sc.call(ra(held, "setParams"), vec![fl(&[3.0])]);
    sc.repr(r(owner));
    sc.raises(n("copy.deepcopy"), vec![r(owner)]);
}

const LOOK_STATE: [&str; 5] = [
    "getName",
    "getProcessSpace",
    "getDescription",
    "getInterchangeAttributes",
    "__repr__",
];

/// `Look`.
fn look_script() -> Script {
    let mut sc = Script::new();
    let (m, cdl, g) = transforms(&mut sc);
    let l0 = sc.call(n("OCIO.Look"), vec![]);
    sc.eval(r(l0), &LOOK_STATE);
    sc.call(ra(l0, "getTransform"), vec![]);
    let l1 = sc.call(
        n("OCIO.Look"),
        vec![s("look"), s("ps"), r(m), r(g), s("desc")],
    );
    sc.eval(r(l1), &LOOK_STATE);
    let t1 = sc.call(ra(l1, "getTransform"), vec![]);
    let t2 = sc.call(ra(l1, "getTransform"), vec![]);
    sc.call(n("operator.is_"), vec![r(t1), r(t2)]);
    sc.call(n("operator.is_"), vec![r(t1), r(m)]);
    sc.call(ra(t1, "setOffset"), vec![fl(&[1.0, 2.0, 3.0, 4.0])]);
    let inv = sc.call(ra(l1, "getInverseTransform"), vec![]);
    let inv0 = sc.call(ra(inv, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(inv0), r(cdl)]);
    sc.call(ra(cdl, "setOffset"), vec![fl(&[0.5, 0.5, 0.5])]);
    sc.repr(r(l1));
    let l2 = sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![
            ("name", by(b"kw\0x")),
            ("inverseTransform", r(m)),
            ("transform", none()),
        ],
        Expect::Value,
    );
    sc.eval(r(l2), &LOOK_STATE);
    sc.raises(n("OCIO.Look"), vec![i(1)]);
    sc.raises(n("OCIO.Look"), vec![none()]);
    sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("transform", i(1))],
        Expect::Raised,
    );
    sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("bad", s("x"))],
        Expect::Raised,
    );
    for (setter, v, raises) in [
        ("setName", s("n"), false),
        ("setName", none(), true),
        ("setProcessSpace", by(b"p\0q"), false),
        ("setProcessSpace", none(), true),
        ("setDescription", s("d"), false),
        ("setDescription", none(), true),
        ("setTransform", r(g), false),
        ("setTransform", i(3), true),
        ("setInverseTransform", r(m), false),
    ] {
        step(&mut sc, ra(l0, setter), vec![v], raises);
        sc.eval(r(l0), &LOOK_STATE);
    }
    for (m_, args, raises) in [
        (
            "setInterchangeAttribute",
            vec![s("amf_transform_ids"), s("a")],
            false,
        ),
        ("setInterchangeAttribute", vec![s("nope"), s("a")], true),
        (
            "getInterchangeAttribute",
            vec![s("AMF_TRANSFORM_IDS")],
            false,
        ),
        ("getInterchangeAttribute", vec![none()], true),
        (
            "setInterchangeAttribute",
            vec![s("amf_transform_ids"), none()],
            false,
        ),
    ] {
        step(&mut sc, ra(l0, m_), args, raises);
    }
    sc.eval(r(l0), &LOOK_STATE);
    // Copies.
    let copy = sc.call(n("copy.deepcopy"), vec![r(l1)]);
    let ct = sc.call(ra(copy, "getTransform"), vec![]);
    sc.call(n("operator.is_"), vec![r(ct), r(t1)]);
    let cinv = sc.call(ra(copy, "getInverseTransform"), vec![]);
    let cinv0 = sc.call(ra(cinv, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(cinv0), r(cdl)]);
    sc.eval(r(copy), &LOOK_STATE);
    sc.raises(ra(l1, "__deepcopy__"), vec![i(0)]);
    // A fixed function transform its copy refuses.
    let bad = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_03")],
    );
    sc.call(ra(bad, "setParams"), vec![fl(&[1.0])]);
    sc.raises(ra(l1, "setTransform"), vec![r(bad)]);
    sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("transform", r(bad))],
        Expect::Raised,
    );
    invalid_held(&mut sc, l0, "getInverseTransform", vec![], |sc, ff| {
        sc.call(ra(l0, "setInverseTransform"), vec![r(ff)]);
    });
    sc
}

const NT_STATE: [&str; 5] = [
    "getName",
    "getFamily",
    "getDescription",
    "getEncoding",
    "__repr__",
];

/// The aliases and categories of the step's result `x`, as lists.
fn lists(sc: &mut Script, x: usize) {
    let aliases = sc.call(ra(x, "getAliases"), vec![]);
    sc.call(n("list"), vec![r(aliases)]);
    let categories = sc.call(ra(x, "getCategories"), vec![]);
    sc.call(n("list"), vec![r(categories)]);
}

/// `NamedTransform`.
fn named_transform_script() -> Script {
    let mut sc = Script::new();
    let (m, cdl, g) = transforms(&mut sc);
    let fwd = n("OCIO.TRANSFORM_DIR_FORWARD");
    let inv = n("OCIO.TRANSFORM_DIR_INVERSE");
    let nt0 = sc.call(n("OCIO.NamedTransform"), vec![]);
    sc.eval(r(nt0), &NT_STATE);
    let nt1 = sc.call(
        n("OCIO.NamedTransform"),
        vec![
            s("nt"),
            l(vec![s("a1"), s("NT"), s("a2")]),
            s("fam"),
            s("desc"),
            r(m),
            r(g),
            l(vec![s(" c1 "), s("C1"), s("c2")]),
        ],
    );
    sc.eval(r(nt1), &NT_STATE);
    lists(&mut sc, nt1);
    let t1 = sc.call(ra(nt1, "getTransform"), vec![fwd.clone()]);
    sc.call(n("operator.is_"), vec![r(t1), r(m)]);
    let t2 = sc.call(ra(nt1, "getTransform"), vec![fwd.clone()]);
    sc.call(n("operator.is_"), vec![r(t1), r(t2)]);
    sc.call(ra(t1, "setOffset"), vec![fl(&[1.0, 2.0, 3.0, 4.0])]);
    let ti = sc.call(ra(nt1, "getTransform"), vec![inv.clone()]);
    let ti0 = sc.call(ra(ti, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(ti0), r(cdl)]);
    sc.repr(r(nt1));
    let odd = sc.call(n("OCIO.TransformDirection"), vec![i(7)]);
    sc.raises(ra(nt1, "getTransform"), vec![r(odd)]);
    sc.raises(ra(nt1, "setTransform"), vec![r(m), r(odd)]);
    sc.raises(ra(nt1, "setTransform"), vec![none(), r(odd)]);
    // GetTransform: the transform itself, or a copy of the other inverted.
    let gt = sc.call(
        n("OCIO.NamedTransform.GetTransform"),
        vec![r(nt1), fwd.clone()],
    );
    sc.call(n("operator.is_"), vec![r(gt), r(t1)]);
    sc.raises(
        n("OCIO.NamedTransform.GetTransform"),
        vec![none(), fwd.clone()],
    );
    sc.raises(n("OCIO.NamedTransform.GetTransform"), vec![r(nt1), r(odd)]);
    sc.raises(
        n("OCIO.NamedTransform.GetTransform"),
        vec![i(1), fwd.clone()],
    );
    sc.call(ra(nt1, "setTransform"), vec![none(), inv.clone()]);
    let gi = sc.call(
        n("OCIO.NamedTransform.GetTransform"),
        vec![r(nt1), inv.clone()],
    );
    sc.call(n("operator.is_"), vec![r(gi), r(t1)]);
    sc.eval(r(gi), &["getDirection", "__repr__"]);
    sc.call(ra(nt1, "getTransform"), vec![inv.clone()]);
    sc.call(ra(nt1, "setTransform"), vec![r(g), inv.clone()]);
    sc.call(ra(nt1, "setTransform"), vec![none(), fwd.clone()]);
    let gf = sc.call_kw(
        n("OCIO.NamedTransform.GetTransform"),
        vec![],
        vec![("transform", r(nt1)), ("direction", fwd.clone())],
        Expect::Value,
    );
    sc.eval(r(gf), &["getDirection", "__repr__"]);
    // A direction outside the enumerators on the transform: inverted to forward.
    let m2 = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(m2, "setDirection"), vec![r(odd)]);
    sc.call(ra(nt1, "setTransform"), vec![r(m2), inv.clone()]);
    let gf2 = sc.call(
        n("OCIO.NamedTransform.GetTransform"),
        vec![r(nt1), fwd.clone()],
    );
    sc.eval(r(gf2), &["getDirection"]);
    // Setters.
    for (setter, v, raises) in [
        ("setName", s("a2"), false),
        ("setName", none(), true),
        ("setFamily", by(b"f"), false),
        ("setFamily", none(), true),
        ("setDescription", s("d\0e"), false),
        ("setEncoding", s("log"), false),
        ("setEncoding", none(), true),
        ("addAlias", s("x"), false),
        ("addAlias", s("A2"), false),
        ("addAlias", none(), true),
        ("removeAlias", s("X"), false),
        ("removeAlias", none(), true),
        ("hasAlias", s("A1"), false),
        ("hasAlias", none(), false),
        ("addCategory", s("c3"), false),
        ("addCategory", none(), false),
        ("hasCategory", s("C3"), false),
        ("hasCategory", none(), false),
        ("removeCategory", s(" c1"), false),
    ] {
        step(&mut sc, ra(nt1, setter), vec![v], raises);
        sc.eval(r(nt1), &NT_STATE);
        lists(&mut sc, nt1);
    }
    let aliases = sc.call(ra(nt1, "getAliases"), vec![]);
    let cats = sc.call(ra(nt1, "getCategories"), vec![]);
    for k in [0, -1] {
        sc.call(ra(aliases, "__getitem__"), vec![i(k)]);
        sc.call(ra(cats, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(aliases, "__getitem__"), vec![i(9)]);
    sc.raises(ra(cats, "__getitem__"), vec![i(9)]);
    sc.call(ra(nt1, "clearAliases"), vec![]);
    sc.call(ra(nt1, "clearCategories"), vec![]);
    sc.call(n("len"), vec![r(aliases)]);
    sc.call(n("len"), vec![r(cats)]);
    // Constructors' refusals, keywords.
    let nt2 = sc.call_kw(
        n("OCIO.NamedTransform"),
        vec![],
        vec![
            ("inverseTransform", r(m)),
            ("aliases", t(vec![s("z")])),
            ("name", s("z")),
        ],
        Expect::Value,
    );
    sc.eval(r(nt2), &NT_STATE);
    lists(&mut sc, nt2);
    sc.call_kw(
        n("OCIO.NamedTransform"),
        vec![],
        vec![("name", none())],
        Expect::Raised,
    );
    sc.call_kw(
        n("OCIO.NamedTransform"),
        vec![],
        vec![("family", none())],
        Expect::Raised,
    );
    sc.raises(n("OCIO.NamedTransform"), vec![i(1)]);
    // Copies.
    let copy = sc.call(n("copy.deepcopy"), vec![r(nt1)]);
    let ct = sc.call(ra(copy, "getTransform"), vec![inv.clone()]);
    sc.call(n("operator.is_"), vec![r(ct), r(m2)]);
    sc.eval(r(copy), &NT_STATE);
    invalid_held(&mut sc, nt1, "getTransform", vec![fwd.clone()], |sc, ff| {
        sc.call(ra(nt1, "setTransform"), vec![r(ff), fwd.clone()]);
    });
    sc
}

const VT_STATE: [&str; 6] = [
    "getName",
    "getFamily",
    "getDescription",
    "getReferenceSpaceType",
    "getInterchangeAttributes",
    "__repr__",
];

/// `ViewTransform`.
fn view_transform_script() -> Script {
    let mut sc = Script::new();
    let (m, cdl, g) = transforms(&mut sc);
    let to = n("OCIO.VIEWTRANSFORM_DIR_TO_REFERENCE");
    let from = n("OCIO.VIEWTRANSFORM_DIR_FROM_REFERENCE");
    let vd = sc.call(n("OCIO.ViewTransform"), vec![]);
    sc.eval(r(vd), &VT_STATE);
    let v0 = sc.call(
        n("OCIO.ViewTransform"),
        vec![n("OCIO.REFERENCE_SPACE_DISPLAY")],
    );
    sc.eval(r(v0), &VT_STATE);
    let v1 = sc.call(
        n("OCIO.ViewTransform"),
        vec![
            n("OCIO.REFERENCE_SPACE_SCENE"),
            s("vt"),
            s("fam"),
            s("desc"),
            r(m),
            r(g),
            l(vec![s("c1"), s("c2")]),
        ],
    );
    sc.eval(r(v1), &VT_STATE);
    let t1 = sc.call(ra(v1, "getTransform"), vec![to.clone()]);
    sc.call(n("operator.is_"), vec![r(t1), r(m)]);
    sc.call(ra(t1, "setOffset"), vec![fl(&[1.0, 2.0, 3.0, 4.0])]);
    let tf = sc.call(ra(v1, "getTransform"), vec![from.clone()]);
    let tf0 = sc.call(ra(tf, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(tf0), r(cdl)]);
    sc.repr(r(v1));
    let odd = sc.call(n("OCIO.ViewTransformDirection"), vec![i(4)]);
    sc.call(ra(v1, "getTransform"), vec![r(odd)]);
    sc.call(ra(v1, "setTransform"), vec![r(m), r(odd)]);
    sc.call(ra(v1, "setTransform"), vec![none(), to.clone()]);
    sc.eval(r(v1), &VT_STATE);
    let v2 = sc.call_kw(
        n("OCIO.ViewTransform"),
        vec![],
        vec![
            ("name", s("kw")),
            ("fromReference", r(m)),
            ("toReference", none()),
        ],
        Expect::Value,
    );
    sc.eval(r(v2), &VT_STATE);
    sc.call_kw(
        n("OCIO.ViewTransform"),
        vec![],
        vec![("name", none())],
        Expect::Raised,
    );
    sc.raises(n("OCIO.ViewTransform"), vec![i(0)]);
    for (setter, v) in [
        ("setName", s("n")),
        ("setName", none()),
        ("setFamily", by(b"f\0g")),
        ("setFamily", none()),
        ("setDescription", s("d")),
        ("setDescription", none()),
        ("addCategory", s("C1")),
        ("addCategory", s("c3")),
        ("addCategory", none()),
        ("hasCategory", s("c3 ")),
        ("removeCategory", s("C2")),
        ("removeCategory", none()),
    ] {
        sc.call(ra(v1, setter), vec![v]);
        sc.eval(r(v1), &VT_STATE);
    }
    let cats = sc.call(ra(v1, "getCategories"), vec![]);
    sc.call(n("list"), vec![r(cats)]);
    for k in [0, 1, -1] {
        sc.call(ra(cats, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(cats, "__getitem__"), vec![i(2)]);
    sc.call(ra(v1, "clearCategories"), vec![]);
    sc.call(n("len"), vec![r(cats)]);
    for (m_, args, raises) in [
        (
            "setInterchangeAttribute",
            vec![s("AMF_transform_ids"), s("p")],
            false,
        ),
        (
            "setInterchangeAttribute",
            vec![s("icc_profile_name"), s("p")],
            true,
        ),
        ("setInterchangeAttribute", vec![none(), s("p")], true),
        (
            "getInterchangeAttribute",
            vec![s("amf_transform_ids")],
            false,
        ),
        ("getInterchangeAttribute", vec![s("x")], true),
    ] {
        step(&mut sc, ra(v1, m_), args, raises);
    }
    let copy = sc.call(n("copy.deepcopy"), vec![r(v1)]);
    let ct = sc.call(ra(copy, "getTransform"), vec![from.clone()]);
    sc.call(n("operator.is_"), vec![r(ct), r(tf)]);
    sc.eval(r(copy), &VT_STATE);
    sc.raises(ra(v1, "setIsData"), vec![b(true)]);
    invalid_held(&mut sc, v1, "getTransform", vec![to.clone()], |sc, ff| {
        sc.call(ra(v1, "setTransform"), vec![r(ff), to.clone()]);
    });
    sc
}

#[test]
fn looks_named_and_view_transforms_behave_as_the_wheels() {
    compare(&[
        look_script(),
        named_transform_script(),
        view_transform_script(),
    ]);
}

/// The reference space outside its enumerators, which the wheel's view transform stores, is the
/// port's error (U-112); the wheel runs the same steps.
#[test]
fn a_reference_space_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let rs = sc.call(n("OCIO.ReferenceSpaceType"), vec![i(3)]);
    let refused = [
        sc.call(n("OCIO.ViewTransform"), vec![r(rs)]),
        sc.call_kw(
            n("OCIO.ViewTransform"),
            vec![],
            vec![("referenceSpace", r(rs)), ("name", s("x"))],
            Expect::Value,
        ),
    ];
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), 3);
    for k in refused {
        assert!(theirs[k].get("value").is_some(), "{}", theirs[k]);
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &ours[k]["raised"]["message"],
        ))
        .unwrap();
        assert!(message.contains("U-112"), "{}", ours[k]);
    }
}

/// Where the wheel copies a null transform, the port's error (U-124); the controls run.
#[test]
fn where_the_wheel_copies_a_null_transform_the_port_raises() {
    common::start_python();
    Python::attach(|py| {
        for code in [
            "OCIO.Look().setTransform(None)",
            "OCIO.Look().setInverseTransform(None)",
            "OCIO.NamedTransform.GetTransform(OCIO.NamedTransform(), OCIO.TRANSFORM_DIR_FORWARD)",
            "OCIO.NamedTransform.GetTransform(OCIO.NamedTransform(), OCIO.TRANSFORM_DIR_INVERSE)",
        ] {
            let globals = PyDict::new(py);
            let c =
                std::ffi::CString::new(format!("import PyOpenColorIO as OCIO\n{code}")).unwrap();
            let err = py.run(&c, Some(&globals), None).expect_err(code);
            let text = err.to_string();
            assert!(
                text.starts_with("ValueError") && text.contains("docs/improvements.md, U-124"),
                "{code}: {text}"
            );
        }
        let c = std::ffi::CString::new(
            "import PyOpenColorIO as OCIO\nl = OCIO.Look()\n\
             l.setTransform(OCIO.MatrixTransform())\n\
             nt = OCIO.NamedTransform(inverseTransform=OCIO.MatrixTransform())\n\
             t = OCIO.NamedTransform.GetTransform(nt, OCIO.TRANSFORM_DIR_FORWARD)\n\
             assert t.getDirection() == OCIO.TRANSFORM_DIR_INVERSE",
        )
        .unwrap();
        py.run(&c, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}

/// Port of `OCIO_ADD_TEST(NamedTransform, basic)` @ v2.5.2.
/// Source: tests/cpu/NamedTransform_tests.cpp:12-62. Run every original assertion through
/// the existing production Python API, whose None argument represents the null shared pointer.
/// Expected text and exception fragments below are copied from the upstream C++ test.
#[test]
fn upstream_named_transform_basic_through_existing_public_api() {
    common::start_python();
    Python::attach(|py| {
        let code = std::ffi::CString::new(
            r#"import copy
import PyOpenColorIO as OCIO

nt = OCIO.NamedTransform()
assert nt is not None
assert nt.getName() == ""
assert nt.getTransform(OCIO.TRANSFORM_DIR_FORWARD) is None
assert nt.getTransform(OCIO.TRANSFORM_DIR_INVERSE) is None
nt.setName("NewName")
assert nt.getName() == "NewName"

mat = OCIO.MatrixTransform()
nt.setTransform(mat, OCIO.TRANSFORM_DIR_FORWARD)
forward = nt.getTransform(OCIO.TRANSFORM_DIR_FORWARD)
assert isinstance(forward, OCIO.MatrixTransform)
assert nt.getTransform(OCIO.TRANSFORM_DIR_FORWARD) is not mat
assert nt.getTransform(OCIO.TRANSFORM_DIR_INVERSE) is None
actual_forward = OCIO.NamedTransform.GetTransform(nt, OCIO.TRANSFORM_DIR_FORWARD)
assert isinstance(actual_forward, OCIO.MatrixTransform)
actual_inverse = OCIO.NamedTransform.GetTransform(nt, OCIO.TRANSFORM_DIR_INVERSE)
assert isinstance(actual_inverse, OCIO.MatrixTransform)
assert str(nt) == '<NamedTransform name=NewName,\n    forward=\n        <MatrixTransform direction=forward, fileindepth=unknown, fileoutdepth=unknown, matrix=[1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], offset=[0, 0, 0, 0]>>'

config = copy.deepcopy(OCIO.Config.CreateRaw())
def check_throw_what(value, fragment):
    try:
        config.addNamedTransform(value)
    except OCIO.Exception as error:
        assert fragment in str(error), str(error)
    else:
        raise AssertionError("expected OCIO.Exception: " + fragment)

invalid = None
check_throw_what(invalid, 'Named transform is null')
invalid = OCIO.NamedTransform()
check_throw_what(invalid, 'Named transform must have a non-empty name')
invalid.setName("name")
check_throw_what(invalid, 'Named transform must define at least one transform')
"#,
        )
        .unwrap();
        let globals = PyDict::new(py);
        py.run(&code, Some(&globals), None)
            .unwrap_or_else(|error| panic!("{error}"));
    });
}
