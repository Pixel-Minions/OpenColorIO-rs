// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ColorSpace` and `ColorSpaceSet` with their iterators (chunk 6.3e) against the wheel:
//! - each class's surface (`ColorSpaceSet.__hash__` None after `__eq__`);
//! - through the same step scripts on both sides: `ColorSpace`'s three constructors (every
//!   parameter, positional and keyword, their errors), every getter and setter (`None`,
//!   `bytes`, a NUL, aliases and the name, the interop ID's rules, the interchange attributes,
//!   categories ignoring case and spaces), the iterators (a negative index: `None` for a
//!   category, `''` for an alias), the transforms (the same object each `getTransform`, a
//!   change through it seen in the color space and its `repr`, a group's children shared, a
//!   direction outside the enumerators, `None`), copies;
//! - `ColorSpaceSet`: adding copies (replacing by name, the alias conflicts), the set's own
//!   color spaces returned (identity, changes seen by the set), names and aliases ignoring case,
//!   removal (a set of one color space removing its own, I-281), a set added to itself, the
//!   operators (`-`, `|`, `&`, `==`, `!=` between sets of different sizes, `NotImplemented` for
//!   anything but a set, `None`'s cases the wheel survives), the iterators, copies.
//!
//! And, without the wheel: an enum value outside its enumerators that the wheel stores is the
//! port's error (U-112; the wheel runs the same steps); `None` where the wheel dereferences a
//! null set or color space, and a set of two or more removing its own color spaces, are the
//! port's errors (U-124: the wheel's process dies).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, f, fl, i, l, n, none, r, ra, s, t};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::json;

/// Each class has the wheel's surface, member for member.
#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in ["ColorSpace", "ColorSpaceSet"] {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 15),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// The getters a color space's state shows through.
const STATE: [&str; 13] = [
    "getName",
    "getFamily",
    "getEncoding",
    "getEqualityGroup",
    "getDescription",
    "getInteropID",
    "getInterchangeAttributes",
    "getBitDepth",
    "isData",
    "getReferenceSpaceType",
    "getAllocation",
    "getAllocationVars",
    "__repr__",
];

/// A step whose outcome the test expects, by whether it raises.
fn step(sc: &mut Script, target: serde_json::Value, args: Vec<serde_json::Value>, raises: bool) {
    let expect = if raises {
        Expect::Raised
    } else {
        Expect::Value
    };
    sc.step(json!({ "call": target, "args": args }), expect);
}

/// The aliases and categories of the color space `cs`, as lists.
fn lists(sc: &mut Script, cs: usize) {
    let aliases = sc.call(ra(cs, "getAliases"), vec![]);
    sc.call(n("list"), vec![r(aliases)]);
    let categories = sc.call(ra(cs, "getCategories"), vec![]);
    sc.call(n("list"), vec![r(categories)]);
}

/// `ColorSpace`'s constructors.
fn constructors_script() -> Script {
    let mut sc = Script::new();
    let c0 = sc.call(n("OCIO.ColorSpace"), vec![]);
    sc.eval(r(c0), &STATE);
    let c1 = sc.call(
        n("OCIO.ColorSpace"),
        vec![n("OCIO.REFERENCE_SPACE_DISPLAY")],
    );
    sc.eval(r(c1), &STATE);
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let range = sc.call(n("OCIO.RangeTransform"), vec![]);
    sc.call(ra(range, "setMinInValue"), vec![f(0.1234567891234)]);
    let c2 = sc.call(
        n("OCIO.ColorSpace"),
        vec![
            n("OCIO.REFERENCE_SPACE_SCENE"),
            s("full"),
            l(vec![s("alias1"), s("FULL"), s("alias2"), s("alias1")]),
            s("fam"),
            s("enc"),
            s("eq"),
            s("desc"),
            n("OCIO.BIT_DEPTH_F32"),
            b(true),
            n("OCIO.ALLOCATION_LG2"),
            fl(&[-8.0, 5.0, 0.00390625]),
            r(m),
            r(range),
            l(vec![s("cat1"), s(" Cat2 "), s("CAT1")]),
            s("ns:id"),
        ],
    );
    sc.eval(r(c2), &STATE);
    lists(&mut sc, c2);
    let t2 = sc.call(
        ra(c2, "getTransform"),
        vec![n("OCIO.COLORSPACE_DIR_TO_REFERENCE")],
    );
    sc.call(n("operator.is_"), vec![r(t2), r(m)]);
    let c3 = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![
            ("name", by(b"kw\0cut")),
            ("aliases", t(vec![s("kw")])),
            ("description", s("multi\nline")),
            ("allocationVars", l(vec![i(1), i(2)])),
            ("fromReference", r(m)),
            ("isData", i(1)),
        ],
        Expect::Value,
    );
    sc.eval(r(c3), &STATE);
    lists(&mut sc, c3);
    for (kwargs, raises) in [
        (vec![("allocationVars", fl(&[1.0]))], true),
        (vec![("allocationVars", fl(&[1.0, 2.0, 3.0, 4.0]))], true),
        (vec![("interopID", s("Bad ID"))], true),
        (vec![("interopID", s(":x"))], true),
        (vec![("name", none())], true),
        (vec![("toReference", i(1))], true),
        (vec![("toReference", none())], false),
        (vec![("categories", s("ab"))], true),
        (vec![("bitDepth", i(1))], true),
        (vec![("nope", i(1))], true),
        (
            vec![
                ("referenceSpace", n("OCIO.REFERENCE_SPACE_DISPLAY")),
                ("name", s("d")),
            ],
            false,
        ),
    ] {
        let expect = if raises {
            Expect::Raised
        } else {
            Expect::Value
        };
        let c = sc.call_kw(n("OCIO.ColorSpace"), vec![], kwargs, expect);
        if !raises {
            sc.eval(r(c), &STATE);
        }
    }
    sc.raises(n("OCIO.ColorSpace"), vec![i(0)]);
    sc.raises(n("OCIO.ColorSpace"), vec![none()]);
    sc
}

/// `ColorSpace`'s getters and setters, iterators and copies.
fn methods_script() -> Script {
    let mut sc = Script::new();
    let cs = sc.call(n("OCIO.ColorSpace"), vec![]);
    for (setter, v, raises) in [
        ("setName", s("name"), false),
        ("setName", by(b"bytes\0name"), false),
        ("setName", none(), true),
        ("setFamily", s("fam"), false),
        ("setFamily", none(), false),
        ("setEncoding", s("scene-linear"), false),
        ("setEqualityGroup", by(b"eq"), false),
        ("setDescription", s("d\0x"), false),
        ("setDescription", i(1), true),
        ("setInteropID", s("ok_id"), false),
        ("setInteropID", s("UPPER"), true),
        ("setInteropID", s("a:b:c"), true),
        ("setInteropID", s("a:"), true),
        ("setInteropID", s("ns:cs-1.0"), false),
        ("setInteropID", none(), false),
        ("setBitDepth", n("OCIO.BIT_DEPTH_UINT10"), false),
        ("setBitDepth", i(3), true),
        ("setIsData", b(true), false),
        ("setIsData", none(), false),
        ("setAllocation", n("OCIO.ALLOCATION_LG2"), false),
        ("setAllocationVars", fl(&[0.5, 1e40]), false),
        ("setAllocationVars", fl(&[]), true),
        ("setAllocationVars", l(vec![s("a"), s("b")]), true),
    ] {
        step(&mut sc, ra(cs, setter), vec![v], raises);
        sc.eval(r(cs), &STATE);
    }
    // Aliases.
    sc.call(ra(cs, "setName"), vec![s("Name")]);
    for (m, v, raises) in [
        ("addAlias", s("a1"), false),
        ("addAlias", s("NAME"), false),
        ("addAlias", s("A1"), false),
        ("addAlias", s(""), false),
        ("addAlias", none(), true),
        ("addAlias", s("a2"), false),
        ("hasAlias", s("A2"), false),
        ("hasAlias", s("name"), false),
        ("hasAlias", none(), false),
        ("removeAlias", s("A1"), false),
        ("removeAlias", s("a1"), false),
        ("removeAlias", none(), true),
        ("addAlias", s("a3"), false),
        ("setName", s("a3"), false),
    ] {
        step(&mut sc, ra(cs, m), vec![v], raises);
        lists(&mut sc, cs);
    }
    let aliases = sc.call(ra(cs, "getAliases"), vec![]);
    sc.call(n("type"), vec![r(aliases)]);
    for k in [0, -1, -5] {
        sc.call(ra(aliases, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(aliases, "__getitem__"), vec![i(1)]);
    sc.call(ra(cs, "addAlias"), vec![s("late")]);
    sc.call(n("len"), vec![r(aliases)]);
    sc.call(n("next"), vec![r(aliases)]);
    sc.call(ra(cs, "clearAliases"), vec![]);
    sc.raises(n("next"), vec![r(aliases)]);
    // Categories.
    for (m, v) in [
        ("addCategory", s(" Cat ")),
        ("addCategory", s("cat")),
        ("addCategory", s("")),
        ("addCategory", none()),
        ("addCategory", s("other")),
        ("hasCategory", s("CAT")),
        ("hasCategory", s(" other ")),
        ("hasCategory", none()),
        ("removeCategory", s("OTHER ")),
        ("removeCategory", none()),
        ("addCategory", s("third")),
    ] {
        sc.call(ra(cs, m), vec![v]);
        lists(&mut sc, cs);
    }
    let cats = sc.call(ra(cs, "getCategories"), vec![]);
    for k in [0, 1, -1, -3] {
        sc.call(ra(cats, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(cats, "__getitem__"), vec![i(2)]);
    sc.call(ra(cs, "clearCategories"), vec![]);
    sc.call(n("len"), vec![r(cats)]);
    // Interchange attributes.
    for (m, args, raises) in [
        (
            "setInterchangeAttribute",
            vec![s("AMF_transform_ids"), s("x")],
            false,
        ),
        (
            "setInterchangeAttribute",
            vec![s("icc_profile_name"), s("p")],
            false,
        ),
        ("setInterchangeAttribute", vec![s("unknown"), s("v")], true),
        ("setInterchangeAttribute", vec![none(), s("v")], true),
        (
            "getInterchangeAttribute",
            vec![s("amf_TRANSFORM_ids")],
            false,
        ),
        ("getInterchangeAttribute", vec![s("nope")], true),
        ("getInterchangeAttribute", vec![none()], true),
        (
            "setInterchangeAttribute",
            vec![s("icc_profile_name"), none()],
            false,
        ),
        (
            "getInterchangeAttribute",
            vec![s("icc_profile_name")],
            false,
        ),
    ] {
        step(&mut sc, ra(cs, m), args, raises);
    }
    sc.eval(r(cs), &STATE);
    // Copies.
    let copy = sc.call(n("copy.deepcopy"), vec![r(cs)]);
    sc.call(n("operator.is_"), vec![r(copy), r(cs)]);
    sc.eval(r(copy), &STATE);
    sc.call(ra(copy, "setName"), vec![s("copy")]);
    sc.eval(r(cs), &["getName"]);
    sc.raises(ra(cs, "__deepcopy__"), vec![l(vec![])]);
    sc
}

/// `ColorSpace`'s transforms.
fn transforms_script() -> Script {
    let mut sc = Script::new();
    let cs = sc.call(n("OCIO.ColorSpace"), vec![]);
    sc.call(ra(cs, "setName"), vec![s("cs")]);
    let to_dir = n("OCIO.COLORSPACE_DIR_TO_REFERENCE");
    let from_dir = n("OCIO.COLORSPACE_DIR_FROM_REFERENCE");
    sc.call(ra(cs, "getTransform"), vec![to_dir.clone()]);
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(cs, "setTransform"), vec![r(m), to_dir.clone()]);
    let t1 = sc.call(ra(cs, "getTransform"), vec![to_dir.clone()]);
    let t2 = sc.call(ra(cs, "getTransform"), vec![to_dir.clone()]);
    sc.call(n("operator.is_"), vec![r(t1), r(t2)]);
    sc.call(n("operator.is_"), vec![r(t1), r(m)]);
    sc.call(ra(t1, "setOffset"), vec![fl(&[1.0, 2.0, 3.0, 4.0])]);
    sc.repr(r(cs));
    sc.call(ra(m, "setOffset"), vec![fl(&[9.0, 9.0, 9.0, 9.0])]);
    sc.repr(r(cs));
    // A group: its copy shares its children.
    let child = sc.call(n("OCIO.CDLTransform"), vec![]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(child)])]);
    sc.call(ra(cs, "setTransform"), vec![r(g), from_dir.clone()]);
    let g2 = sc.call(ra(cs, "getTransform"), vec![from_dir.clone()]);
    sc.call(n("operator.is_"), vec![r(g2), r(g)]);
    let g2c = sc.call(ra(g2, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(g2c), r(child)]);
    sc.call(
        ra(child, "setSlope"),
        vec![fl(&[0.1234567891234, 2.0, 3.0])],
    );
    sc.repr(r(cs));
    // Copies of the color space: their transforms copied, a group's children still shared.
    let copy = sc.call(n("copy.deepcopy"), vec![r(cs)]);
    let ct = sc.call(ra(copy, "getTransform"), vec![to_dir.clone()]);
    sc.call(n("operator.is_"), vec![r(ct), r(t1)]);
    let cg = sc.call(ra(copy, "getTransform"), vec![from_dir.clone()]);
    let cgc = sc.call(ra(cg, "__getitem__"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(cgc), r(child)]);
    sc.repr(r(copy));
    // Directions outside the enumerators: none read, none stored (the copy is still made).
    let odd = sc.call(n("OCIO.ColorSpaceDirection"), vec![i(5)]);
    sc.call(ra(cs, "getTransform"), vec![r(odd)]);
    sc.call(ra(cs, "setTransform"), vec![r(m), r(odd)]);
    sc.repr(r(cs));
    let bad = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_03")],
    );
    sc.call(ra(bad, "setParams"), vec![fl(&[1.0])]);
    sc.raises(ra(cs, "setTransform"), vec![r(bad), r(odd)]);
    sc.raises(ra(cs, "setTransform"), vec![r(bad), to_dir.clone()]);
    sc.repr(r(cs));
    // None removes.
    sc.call(ra(cs, "setTransform"), vec![none(), to_dir.clone()]);
    sc.call(ra(cs, "getTransform"), vec![to_dir.clone()]);
    sc.raises(ra(cs, "setTransform"), vec![r(m)]);
    sc.raises(ra(cs, "setTransform"), vec![i(1), to_dir.clone()]);
    sc.raises(ra(cs, "getTransform"), vec![i(0)]);
    sc.repr(r(cs));
    // A fixed function transform made invalid through the object getTransform returns: the
    // wheel prints it as it is (its copy, deepcopy's, refuses it).
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_03")],
    );
    sc.call(ra(cs, "setTransform"), vec![r(ff), from_dir.clone()]);
    let held = sc.call(ra(cs, "getTransform"), vec![from_dir.clone()]);
    sc.call(ra(held, "setParams"), vec![fl(&[1.0, 2.0])]);
    sc.repr(r(cs));
    sc.raises(n("copy.deepcopy"), vec![r(cs)]);
    sc
}

/// The names of the set `css` (a step's result) now, through a new iterator.
fn names_now(sc: &mut Script, css: usize) {
    let it = sc.call(ra(css, "getColorSpaceNames"), vec![]);
    sc.call(n("list"), vec![r(it)]);
}

/// `ColorSpaceSet`.
fn set_script() -> Script {
    let mut sc = Script::new();
    let css = sc.call(n("OCIO.ColorSpaceSet"), vec![]);
    let mk = |sc: &mut Script, name: &str, aliases: Vec<&str>| {
        sc.call_kw(
            n("OCIO.ColorSpace"),
            vec![],
            vec![
                ("name", s(name)),
                ("aliases", l(aliases.into_iter().map(s).collect())),
            ],
            Expect::Value,
        )
    };
    let a = mk(&mut sc, "A", vec!["a1"]);
    let b_ = mk(&mut sc, "B", vec!["b1", "b2"]);
    let empty = sc.call(n("OCIO.ColorSpace"), vec![]);
    sc.call(ra(css, "addColorSpace"), vec![r(a)]);
    sc.call(ra(css, "addColorSpace"), vec![r(b_)]);
    sc.raises(ra(css, "addColorSpace"), vec![r(empty)]);
    let a_alias = mk(&mut sc, "a1", vec![]);
    sc.raises(ra(css, "addColorSpace"), vec![r(a_alias)]);
    let clash = mk(&mut sc, "C", vec!["B2"]);
    sc.raises(ra(css, "addColorSpace"), vec![r(clash)]);
    let a_again = mk(&mut sc, "a", vec!["a1", "x"]);
    sc.call(ra(css, "addColorSpace"), vec![r(a_again)]);
    sc.raises(ra(css, "addColorSpace"), vec![i(1)]);
    let names = sc.call(ra(css, "getColorSpaceNames"), vec![]);
    sc.call(n("list"), vec![r(names)]);
    // The set's own color spaces.
    let got = sc.call(ra(css, "getColorSpace"), vec![s("B1")]);
    sc.call(n("operator.is_"), vec![r(got), r(b_)]);
    let got2 = sc.call(ra(css, "getColorSpace"), vec![s("b")]);
    sc.call(n("operator.is_"), vec![r(got), r(got2)]);
    let spaces = sc.call(ra(css, "getColorSpaces"), vec![]);
    let got3 = sc.call(ra(spaces, "__getitem__"), vec![i(1)]);
    sc.call(n("operator.is_"), vec![r(got3), r(got)]);
    sc.call(ra(got, "setName"), vec![s("Renamed")]);
    names_now(&mut sc, css);
    for name in ["renamed", "B", "x", "", "A1"] {
        sc.call(ra(css, "hasColorSpace"), vec![s(name)]);
        sc.call(ra(css, "getColorSpace"), vec![s(name)]);
    }
    sc.call(ra(css, "hasColorSpace"), vec![none()]);
    sc.call(ra(css, "getColorSpace"), vec![none()]);
    for k in [0, -1] {
        sc.call(ra(names, "__getitem__"), vec![i(k)]);
        sc.call(ra(spaces, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(names, "__getitem__"), vec![i(2)]);
    sc.call(n("next"), vec![r(spaces)]);
    // The operators.
    let other = sc.call(n("OCIO.ColorSpaceSet"), vec![]);
    let c = mk(&mut sc, "C", vec![]);
    sc.call(ra(other, "addColorSpace"), vec![r(c)]);
    sc.call(ra(other, "addColorSpace"), vec![r(a)]);
    for op in ["__sub__", "__or__", "__and__"] {
        let res = sc.call(ra(css, op), vec![r(other)]);
        let res_names = sc.call(ra(res, "getColorSpaceNames"), vec![]);
        sc.call(n("list"), vec![r(res_names)]);
        let res0 = sc.call(ra(res, "getColorSpace"), vec![s("A")]);
        sc.call(n("operator.is_"), vec![r(res0), r(a)]);
        sc.call(ra(css, op), vec![i(1)]);
        sc.call(ra(css, op), vec![s("x")]);
    }
    for op in ["operator.sub", "operator.or_", "operator.and_"] {
        sc.raises(n(op), vec![r(css), i(1)]);
    }
    let empty_set = sc.call(n("OCIO.ColorSpaceSet"), vec![]);
    let diff = sc.call(ra(empty_set, "__sub__"), vec![none()]);
    sc.call(n("type"), vec![r(diff)]);
    for op in ["__eq__", "__ne__"] {
        sc.call(ra(css, op), vec![r(css)]);
        sc.call(ra(css, op), vec![r(other)]);
        sc.call(ra(css, op), vec![none()]);
        sc.call(ra(css, op), vec![i(5)]);
        sc.call(ra(css, op), vec![]);
    }
    sc.call(n("operator.eq"), vec![r(css), none()]);
    sc.call(n("operator.ne"), vec![r(css), r(other)]);
    sc.call(n("OCIO.ColorSpaceSet.__eq__"), vec![i(5), r(css)]);
    sc.raises(n("hash"), vec![r(css)]);
    let same_names = sc.call(n("OCIO.ColorSpaceSet"), vec![]);
    let a_lower = mk(&mut sc, "a", vec![]);
    sc.call(ra(same_names, "addColorSpace"), vec![r(a_lower)]);
    let renamed = mk(&mut sc, "RENAMED", vec![]);
    sc.call(ra(same_names, "addColorSpace"), vec![r(renamed)]);
    sc.call(n("operator.eq"), vec![r(css), r(same_names)]);
    // Sets of different sizes, each name of the smaller in the larger: on either side.
    let subset = sc.call(n("OCIO.ColorSpaceSet"), vec![]);
    let a_only = mk(&mut sc, "A", vec![]);
    sc.call(ra(subset, "addColorSpace"), vec![r(a_only)]);
    for (lhs, rhs) in [(css, subset), (subset, css)] {
        for op in ["__eq__", "__ne__"] {
            sc.call(ra(lhs, op), vec![r(rhs)]);
        }
        sc.call(n("operator.eq"), vec![r(lhs), r(rhs)]);
        sc.call(n("operator.ne"), vec![r(lhs), r(rhs)]);
    }
    // A set added to itself: each color space replaced by a copy.
    let before = sc.call(ra(css, "getColorSpace"), vec![s("A")]);
    sc.call(ra(css, "addColorSpaces"), vec![r(css)]);
    let after = sc.call(ra(css, "getColorSpace"), vec![s("A")]);
    sc.call(n("operator.is_"), vec![r(before), r(after)]);
    sc.call(ra(css, "addColorSpaces"), vec![r(other)]);
    names_now(&mut sc, css);
    // Removing.
    sc.call(ra(css, "removeColorSpace"), vec![s("a1")]);
    sc.call(ra(css, "removeColorSpace"), vec![s("RENAMED")]);
    sc.call(ra(css, "removeColorSpace"), vec![none()]);
    names_now(&mut sc, css);
    sc.call(ra(css, "removeColorSpaces"), vec![r(other)]);
    names_now(&mut sc, css);
    let copy = sc.call(n("copy.deepcopy"), vec![r(other)]);
    let copied = sc.call(ra(copy, "getColorSpace"), vec![s("C")]);
    let original = sc.call(ra(other, "getColorSpace"), vec![s("C")]);
    sc.call(n("operator.is_"), vec![r(copied), r(original)]);
    sc.call(ra(other, "clearColorSpaces"), vec![]);
    let left = sc.call(ra(other, "getColorSpaces"), vec![]);
    sc.call(n("len"), vec![r(left)]);
    // A set of one color space removing its own (I-281; with more, U-124): emptied, with its
    // color space held from Python and without.
    for hold in [true, false] {
        let single = sc.call(n("OCIO.ColorSpaceSet"), vec![]);
        let only = mk(&mut sc, "Only", vec!["o1"]);
        sc.call(ra(single, "addColorSpace"), vec![r(only)]);
        let held = hold.then(|| sc.call(ra(single, "getColorSpace"), vec![s("only")]));
        sc.call(ra(single, "removeColorSpaces"), vec![r(single)]);
        let rest = sc.call(ra(single, "getColorSpaces"), vec![]);
        sc.call(n("len"), vec![r(rest)]);
        names_now(&mut sc, single);
        sc.call(ra(single, "hasColorSpace"), vec![s("o1")]);
        if let Some(held) = held {
            sc.call(ra(held, "getName"), vec![]);
        }
        sc.call(ra(single, "removeColorSpaces"), vec![r(single)]);
        sc.call(ra(single, "addColorSpace"), vec![r(only)]);
        names_now(&mut sc, single);
    }
    sc
}

#[test]
fn color_spaces_behave_as_the_wheels() {
    compare(&[
        constructors_script(),
        methods_script(),
        transforms_script(),
        set_script(),
    ]);
}

/// An enum value outside its enumerators, which the wheel's color space stores, is the port's
/// error (U-112); the wheel runs the same steps without one.
#[test]
fn an_enum_value_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let cs = sc.call(n("OCIO.ColorSpace"), vec![]);
    let rs = sc.call(n("OCIO.ReferenceSpaceType"), vec![i(2)]);
    refused.push(sc.call(n("OCIO.ColorSpace"), vec![r(rs)]));
    refused.push(sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("referenceSpace", r(rs)), ("name", s("x"))],
        Expect::Value,
    ));
    let bd = sc.call(n("OCIO.BitDepth"), vec![i(42)]);
    refused.push(sc.call(ra(cs, "setBitDepth"), vec![r(bd)]));
    refused.push(sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("bitDepth", r(bd))],
        Expect::Value,
    ));
    let al = sc.call(n("OCIO.Allocation"), vec![i(9)]);
    refused.push(sc.call(ra(cs, "setAllocation"), vec![r(al)]));
    refused.push(sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("allocation", r(al))],
        Expect::Value,
    ));
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), sc.steps.len());
    assert_eq!(ours.len(), sc.steps.len());
    for (k, (step, wheel_step)) in ours.iter().zip(theirs).enumerate() {
        assert!(wheel_step.get("value").is_some(), "step {k}: {wheel_step}");
        if refused.contains(&k) {
            let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                &step["raised"]["message"],
            ))
            .unwrap();
            assert!(message.contains("U-112"), "step {k}: {step}");
        } else {
            assert_eq!(step, wheel_step, "step {k}");
        }
    }
    assert_eq!(refused.len(), 6);
}

/// Where the wheel dereferences a null set or color space, or erases from the vector it loops
/// over, the port's error (U-124); the controls run.
#[test]
fn where_the_wheel_dies_the_port_raises() {
    common::start_python();
    Python::attach(|py| {
        let setup = "import PyOpenColorIO as OCIO\ncss = OCIO.ColorSpaceSet()\n\
                     css.addColorSpace(OCIO.ColorSpace(name='a'))\n\
                     css.addColorSpace(OCIO.ColorSpace(name='b'))";
        for code in [
            "css - None",
            "css | None",
            "css & None",
            "OCIO.ColorSpaceSet() & None",
            "css.addColorSpace(None)",
            "css.addColorSpaces(None)",
            "css.removeColorSpaces(None)",
            "css.removeColorSpaces(css)",
        ] {
            let globals = PyDict::new(py);
            let run = |c: &str| {
                let c = std::ffi::CString::new(c).unwrap();
                py.run(&c, Some(&globals), None)
            };
            run(setup).unwrap();
            let err = run(code).expect_err(code);
            let text = err.to_string();
            assert!(
                text.starts_with("ValueError") && text.contains("docs/improvements.md, U-124"),
                "{code}: {text}"
            );
        }
        let globals = PyDict::new(py);
        let c = std::ffi::CString::new(format!(
            "{setup}\nassert len((OCIO.ColorSpaceSet() - None).getColorSpaces()) == 0\n\
             e = OCIO.ColorSpaceSet()\ne.removeColorSpaces(e)\n\
             css.removeColorSpaces(OCIO.ColorSpaceSet())\n\
             assert len(css.getColorSpaces()) == 2"
        ))
        .unwrap();
        py.run(&c, Some(&globals), None)
            .unwrap_or_else(|e| panic!("{e}"));
    });
}
