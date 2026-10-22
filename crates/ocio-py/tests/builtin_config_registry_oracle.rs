// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `BuiltinConfigRegistry` and its two iterators (chunk 6.3f) against the wheel: the class's
//! surface (py_surface_walk.py's `_class` against the wheel's py_surface), and its behaviour
//! through the same step scripts on both sides: `len`, iteration to `StopIteration`, each
//! config's text by name (the bytes, ignoring case, cut at a NUL) and the refused names,
//! `in` (ignoring case, the whole string), both iterators' `len`, indexing without an index
//! check (negative, one past the end, past `int`), their tuples (the default config's flag),
//! and calls on instances of another class.
#![cfg(feature = "python")]

mod common;

use common::script::{Script, by, compare, i, n, none, r, ra, s};

/// The class has the wheel's surface, member for member.
#[test]
fn the_class_has_the_wheels_surface() {
    let report = common::wheel_surface();
    let theirs = common::wheel_member(&report, "BuiltinConfigRegistry");
    let ours = common::port_member("BuiltinConfigRegistry");
    assert!(
        theirs["members"].as_array().is_some_and(|m| m.len() > 5),
        "{theirs}"
    );
    assert_eq!(ours, theirs);
}

/// The names of the wheel's built-in configs, which the script asks for one by one (the
/// script also lists them through the iterator, so a name that changes fails there too).
const NAMES: [&str; 8] = [
    "cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
    "cg-config-v2.1.0_aces-v1.3_ocio-v2.3",
    "cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
    "cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
    "studio-config-v1.0.0_aces-v1.3_ocio-v2.1",
    "studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
    "studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
    "studio-config-v4.0.0_aces-v2.0_ocio-v2.5",
];

fn registry_script() -> Script {
    let mut sc = Script::new();
    let reg = sc.call(n("OCIO.BuiltinConfigRegistry"), vec![]);
    sc.call(n("len"), vec![r(reg)]);
    sc.call(n("list"), vec![r(reg)]);
    for name in NAMES {
        sc.call(ra(reg, "__getitem__"), vec![s(name)]);
        sc.call(ra(reg, "__contains__"), vec![s(name)]);
    }
    for name in [
        "CG-CONFIG-V4.0.0_ACES-V2.0_OCIO-V2.5",
        "cg-config-v4.0.0_aces-v2.0_ocio-v2.5\u{0}junk",
    ] {
        sc.call(ra(reg, "__getitem__"), vec![s(name)]);
        sc.call(ra(reg, "__contains__"), vec![s(name)]);
    }
    for name in [
        "nope",
        "",
        "default",
        "cg-config-latest",
        "ocio://default",
        "caf\u{e9}",
        "\u{0}",
    ] {
        sc.raises(ra(reg, "__getitem__"), vec![s(name)]);
        sc.call(ra(reg, "__contains__"), vec![s(name)]);
    }
    sc.call(
        ra(reg, "__getitem__"),
        vec![by(b"cg-config-v1.0.0_aces-v1.3_ocio-v2.1")],
    );
    sc.raises(ra(reg, "__getitem__"), vec![i(0)]);
    sc.raises(ra(reg, "__getitem__"), vec![none()]);
    sc.raises(ra(reg, "__contains__"), vec![i(0)]);
    // The two iterators.
    for make in ["__iter__", "getBuiltinConfigs"] {
        let it = sc.call(ra(reg, make), vec![]);
        sc.call(n("type"), vec![r(it)]);
        sc.call(n("len"), vec![r(it)]);
        for k in [0, 3, 7] {
            sc.call(ra(it, "__getitem__"), vec![i(k)]);
        }
        for k in [8, -1, 100, -2147483648] {
            sc.raises(ra(it, "__getitem__"), vec![i(k)]);
        }
        sc.raises(ra(it, "__getitem__"), vec![i(2147483648)]);
        sc.raises(ra(it, "__getitem__"), vec![s("0")]);
        let same = sc.call(ra(it, "__iter__"), vec![]);
        sc.call(n("operator.is_"), vec![r(same), r(it)]);
        sc.call(ra(it, "__next__"), vec![]);
        sc.call(n("list"), vec![r(it)]);
        sc.raises(ra(it, "__next__"), vec![]);
        sc.call(ra(it, "__getitem__"), vec![i(1)]);
    }
    // Calls on an instance of another class.
    let ctx = sc.call(n("OCIO.Context"), vec![]);
    for m in ["__len__", "__iter__", "getBuiltinConfigs"] {
        sc.raises(n(&format!("OCIO.BuiltinConfigRegistry.{m}")), vec![r(ctx)]);
    }
    let names = sc.call(ra(reg, "__iter__"), vec![]);
    sc.raises(
        n("OCIO.BuiltinConfigRegistry.BuiltinConfigIterator.__len__"),
        vec![r(names)],
    );
    sc.raises(n("OCIO.BuiltinConfigRegistry"), vec![i(1)]);
    sc
}

/// The class behaves as the wheel's.
#[test]
fn the_class_behaves_as_the_wheels() {
    compare(&[registry_script()]);
}
