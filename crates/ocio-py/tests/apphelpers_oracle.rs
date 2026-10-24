// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The app helpers of chunk 6.7c against the wheel: the submodules `ColorSpaceHelpers` and
//! `DisplayViewHelpers`, and the classes `ColorSpaceMenuParameters`, `ColorSpaceMenuHelper`,
//! `LegacyViewingPipeline`, `MixingSlider` and `MixingColorSpaceManager` with their iterator
//! classes: their surface (py_surface_walk.py's against the wheel's py_surface), and their
//! behaviour through the same step scripts on both sides: constructors (positional, keyword,
//! refused), every getter and setter, the iterators (indices past the end and negative, `next`
//! to the end), `repr`, the config the parameters, menus and managers hold (the very Python
//! object; its later changes seen by `repr` and the processors), the transforms a pipeline
//! holds (its own copies, changed in place through its getters), the manager's slider (the
//! same object), the encoding iterator's `__next__` (I-396), and the processors made (their
//! cache IDs; until the port binds `Processor.getCacheID`, chunk 6.3h, the port's interpreter
//! gets a stand-in reading the library's processor through the embedding API).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, compare, i, n, none, r, ra, s};
use pyo3::prelude::*;
use serde_json::Value;

/// upstream's tests/python/UnitTestUtils.py `SAMPLE_CONFIG`, the config of their tests.
fn sample_config() -> String {
    let path = ocio_testkit::paths::upstream_dir().join("tests/python/UnitTestUtils.py");
    let text = std::fs::read_to_string(path).unwrap();
    let start = text.find("SAMPLE_CONFIG = \"\"\"").unwrap() + "SAMPLE_CONFIG = \"\"\"".len();
    let len = text[start..].find("\"\"\"").unwrap();
    let config = text[start..start + len].to_string();
    assert!(config.contains("DISP_1") && !config.contains('\\'));
    config
}

/// A config of [`sample_config`] whose description is `tag`, so that its cache ID is this
/// script's alone: the library keeps the menus it made for the life of the process, by their
/// parameters' text (which holds the config's cache ID), and the port's scripts share a
/// process where the wheel runs each in its own.
fn config(sc: &mut Script, tag: &str) -> usize {
    let c = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(&sample_config())]);
    sc.call(ra(c, "setDescription"), vec![s(tag)]);
    c
}

fn start() {
    common::start_python();
}

fn run(scripts: &[Script]) {
    start();
    compare(scripts);
}

/// Each class and submodule has the wheel's surface, member for member.
#[test]
fn the_app_helpers_have_the_wheels_surface() {
    let report = common::wheel_surface();
    // The submodules get the module's `__file__` (pybind11's `def_submodule`), which the port's
    // module embedded in the test binary has none of (an extension module's, in target/py-port,
    // has: the probes of chunk 6.7c).
    start();
    let embedded_file = Python::attach(|py| {
        py.import("PyOpenColorIO")
            .unwrap()
            .hasattr("__file__")
            .unwrap()
    });
    assert!(!embedded_file);
    for c in [
        "ColorSpaceHelpers",
        "ColorSpaceMenuParameters",
        "ColorSpaceMenuHelper",
    ] {
        let mut theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| !m.is_empty()),
            "{c}: {theirs}"
        );
        if let Some(names) = theirs
            .get_mut("runtime_names")
            .and_then(Value::as_array_mut)
        {
            let before = names.len();
            names.retain(|n| n != "__file__");
            assert_eq!(names.len(), before - 1, "{c}");
        }
        assert_eq!(ours, theirs, "{c}");
    }
}

/// `len`, items (past the end and negative: `negative`), `iter`, and `next` to the end of the
/// iterator that step `it` is.
fn iterator(sc: &mut Script, it: usize, len: i128, negative: Expect) {
    sc.call(n("len"), vec![r(it)]);
    for k in [0, 1, len - 1] {
        sc.call(ra(it, "__getitem__"), vec![i(k)]);
    }
    // A negative index passes the iterator's check, and is a `size_t` past the last item.
    sc.step(
        serde_json::json!({ "call": ra(it, "__getitem__"), "args": [i(-1)] }),
        negative,
    );
    sc.raises(ra(it, "__getitem__"), vec![i(len)]);
    sc.raises(ra(it, "__getitem__"), vec![none()]);
    let same = sc.call(n("iter"), vec![r(it)]);
    sc.call(n("operator.is_"), vec![r(same), r(it)]);
    for _ in 0..len {
        sc.call(n("next"), vec![r(it)]);
    }
    sc.raises(n("next"), vec![r(it)]);
}

const PARAMETER_GETTERS: [&str; 10] = [
    "getRole",
    "getAppCategories",
    "getUserCategories",
    "getEncodings",
    "getSearchReferenceSpaceType",
    "getIncludeRoles",
    "getIncludeColorSpaces",
    "getIncludeNamedTransforms",
    "getTreatNoCategoryAsAny",
    "__repr__",
];

/// The parameters: constructors, getters, setters, the added color spaces, the config.
#[test]
fn menu_parameters_behave_as_the_wheels() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "menu parameters");
    let p = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    sc.eval(r(p), &PARAMETER_GETTERS);
    let got = sc.call(ra(p, "getConfig"), vec![]);
    sc.unrepr(got);
    sc.call(n("operator.is_"), vec![r(got), r(cfg)]);
    let kw = sc.call_kw(
        n("OCIO.ColorSpaceMenuParameters"),
        vec![],
        vec![
            ("config", r(cfg)),
            ("role", s("role")),
            ("appCategories", s("categories")),
            ("userCategories", s("user")),
            ("encodings", s("video")),
            (
                "searchReferenceSpaceType",
                n("OCIO.SEARCH_REFERENCE_SPACE_DISPLAY"),
            ),
            ("includeRoles", b(true)),
            ("includeColorSpaces", b(false)),
            ("includeNamedTransforms", b(true)),
            ("treatNoCategoryAsAny", b(false)),
        ],
        Expect::Value,
    );
    sc.eval(r(kw), &PARAMETER_GETTERS);
    let positional = sc.call(
        n("OCIO.ColorSpaceMenuParameters"),
        vec![
            r(cfg),
            s("r\u{0}x"),
            b(false),
            n("OCIO.SEARCH_REFERENCE_SPACE_SCENE"),
            i(1),
            none(),
            s("a"),
            s("e"),
            s("u"),
            b(true),
        ],
    );
    sc.eval(r(positional), &PARAMETER_GETTERS);
    for args in [
        vec![],
        vec![none()],
        vec![i(1)],
        vec![r(cfg), none()],
        vec![r(cfg), s("r"), s("x")],
        vec![r(cfg), s("r"), b(true), i(1)],
    ] {
        sc.raises(n("OCIO.ColorSpaceMenuParameters"), args);
    }
    sc.call_kw(
        n("OCIO.ColorSpaceMenuParameters"),
        vec![r(cfg)],
        vec![("role", none())],
        Expect::Raised,
    );
    // Setters.
    for (set, v) in [
        ("setRole", s("x")),
        ("setAppCategories", s("cat1, cat2")),
        ("setUserCategories", s("user1")),
        ("setEncodings", s("film")),
        (
            "setSearchReferenceSpaceType",
            n("OCIO.SEARCH_REFERENCE_SPACE_DISPLAY"),
        ),
        ("setIncludeRoles", b(false)),
        ("setIncludeColorSpaces", b(false)),
        ("setIncludeNamedTransforms", b(false)),
        ("setTreatNoCategoryAsAny", b(false)),
    ] {
        sc.call(ra(p, set), vec![v]);
        // A `bool` takes `None` (false), a string and an enum don't.
        if set.starts_with("setInclude") || set == "setTreatNoCategoryAsAny" {
            sc.call(ra(p, set), vec![none()]);
        } else {
            sc.raises(ra(p, set), vec![none()]);
        }
        sc.eval(r(p), &PARAMETER_GETTERS);
    }
    for set in [
        "setIncludeRoles",
        "setIncludeColorSpaces",
        "setIncludeNamedTransforms",
        "setTreatNoCategoryAsAny",
    ] {
        sc.call(ra(p, set), vec![]);
    }
    sc.raises(ra(p, "setRole"), vec![]);
    sc.call_kw(
        ra(p, "setUserCategories"),
        vec![],
        vec![("categories", s("kw"))],
        Expect::Value,
    );
    sc.eval(r(p), &PARAMETER_GETTERS);
    // The added color spaces.
    for name in ["lin_1", "lin_1", "", "raw", "nope"] {
        sc.call(ra(p, "addColorSpace"), vec![s(name)]);
    }
    sc.raises(ra(p, "addColorSpace"), vec![none()]);
    let it = sc.call(ra(p, "getAddedColorSpaces"), vec![]);
    sc.unrepr(it);
    iterator(&mut sc, it, 3, Expect::Value);
    let it2 = sc.call(ra(p, "getAddedColorSpaces"), vec![]);
    sc.unrepr(it2);
    sc.call(n("list"), vec![r(it2)]);
    sc.repr(r(p));
    sc.call(ra(p, "clearAddedColorSpaces"), vec![]);
    sc.call(n("len"), vec![r(it)]);
    sc.repr(r(p));
    // The config: the object given; its changes are the parameters'.
    // (A role resets the config's cache ID; a description doesn't.)
    sc.call(ra(cfg, "setRole"), vec![s("parameters_role"), s("raw")]);
    sc.repr(r(p));
    let other = config(&mut sc, "menu parameters, other");
    sc.call(ra(p, "setConfig"), vec![r(other)]);
    let got = sc.call(ra(p, "getConfig"), vec![]);
    sc.unrepr(got);
    sc.call(n("operator.is_"), vec![r(got), r(other)]);
    sc.raises(ra(p, "setConfig"), vec![none()]);
    sc.raises(ra(p, "setConfig"), vec![]);
    sc.repr(r(p));
    run(&[sc]);
}

/// The items of the menu `h`: names, UI names, descriptions, families and hierarchy levels.
fn items(sc: &mut Script, h: usize, indices: &[i128]) {
    sc.eval(r(h), &["getNumColorSpaces", "__repr__"]);
    for &k in indices {
        for get in ["getName", "getUIName", "getDescription", "getFamily"] {
            sc.call(ra(h, get), vec![i(k)]);
        }
        let levels = sc.call(ra(h, "getHierarchyLevels"), vec![i(k)]);
        sc.unrepr(levels);
        sc.call(n("list"), vec![r(levels)]);
    }
}

/// The menus: of the parameters' each kind, their items and iterators, lookups, refusals, and
/// the config they share with the parameters that made them.
#[test]
fn menus_behave_as_the_wheels() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "menus");
    let p = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    let h = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(p)]);
    items(&mut sc, h, &[0, 1, 4, 15, 16, 100]);
    let levels = sc.call(ra(h, "getHierarchyLevels"), vec![i(4)]);
    sc.unrepr(levels);
    iterator(&mut sc, levels, 3, Expect::Value);
    sc.call_kw(
        ra(h, "getHierarchyLevels"),
        vec![],
        vec![("index", i(0))],
        Expect::Value,
    );
    for get in ["getName", "getHierarchyLevels"] {
        sc.raises(ra(h, get), vec![i(-1)]);
        sc.raises(ra(h, get), vec![none()]);
    }
    for (get, name) in [
        ("getIndexFromName", "in_1"),
        ("getIndexFromName", "nope"),
        ("getIndexFromUIName", "view_2"),
        ("getIndexFromUIName", "nope"),
        ("getNameFromUIName", "raw"),
        ("getNameFromUIName", "nope"),
        ("getUINameFromName", "lin_2"),
        ("getUINameFromName", "nope"),
    ] {
        sc.call(ra(h, get), vec![s(name)]);
        sc.raises(ra(h, get), vec![none()]);
    }
    for args in [vec![], vec![none()], vec![r(cfg)]] {
        sc.raises(n("OCIO.ColorSpaceMenuHelper"), args);
    }
    let kw = sc.call_kw(
        n("OCIO.ColorSpaceMenuHelper"),
        vec![],
        vec![("parameters", r(p))],
        Expect::Value,
    );
    sc.repr(r(kw));
    // A role, the roles, categories, encodings, added color spaces.
    for kwargs in [
        vec![("role", s("default"))],
        vec![
            ("role", s("non-existing")),
            ("appCategories", s("working-space")),
        ],
        vec![("includeRoles", b(true))],
        vec![
            ("includeRoles", b(true)),
            ("appCategories", s("file-io, working-space")),
            ("treatNoCategoryAsAny", b(false)),
        ],
        vec![
            ("appCategories", s("working-space, file-io")),
            ("treatNoCategoryAsAny", b(false)),
            ("encodings", s("sdr-video")),
            (
                "searchReferenceSpaceType",
                n("OCIO.SEARCH_REFERENCE_SPACE_SCENE"),
            ),
        ],
        vec![("includeNamedTransforms", b(true))],
    ] {
        let mut all = vec![("config", r(cfg))];
        all.extend(kwargs);
        let pk = sc.call_kw(
            n("OCIO.ColorSpaceMenuParameters"),
            vec![],
            all,
            Expect::Value,
        );
        let hk = sc.step(
            serde_json::json!({ "call": n("OCIO.ColorSpaceMenuHelper"), "args": [r(pk)] }),
            Expect::Value,
        );
        items(&mut sc, hk, &[0, 3, 10, 16, 19]);
    }
    // Roles need the color spaces.
    let no_spaces = sc.call_kw(
        n("OCIO.ColorSpaceMenuParameters"),
        vec![r(cfg)],
        vec![("includeColorSpaces", b(false)), ("includeRoles", b(true))],
        Expect::Value,
    );
    sc.raises(n("OCIO.ColorSpaceMenuHelper"), vec![r(no_spaces)]);
    // Added color spaces.
    let pa = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    sc.call(ra(pa, "setAppCategories"), vec![s("file-io")]);
    sc.call(ra(pa, "setTreatNoCategoryAsAny"), vec![b(false)]);
    sc.call(ra(pa, "addColorSpace"), vec![s("view_1")]);
    sc.call(ra(pa, "addColorSpace"), vec![s("raw")]);
    let ha = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(pa)]);
    items(&mut sc, ha, &[0, 4, 5, 6]);
    sc.call(ra(pa, "addColorSpace"), vec![s("nope")]);
    sc.raises(n("OCIO.ColorSpaceMenuHelper"), vec![r(pa)]);
    // The config's later changes: the menu's text writes its cache ID now. A description
    // doesn't reset the cache ID the config computed when the menus were made.
    sc.call(ra(cfg, "setDescription"), vec![s("menus, described again")]);
    sc.repr(r(h));
    sc.repr(r(p));
    sc.call(ra(cfg, "setRole"), vec![s("menus_role"), s("raw")]);
    sc.repr(r(h));
    sc.repr(r(p));
    run(&[sc]);
}

/// Making a menu computes its config's cache ID, which the config keeps until a change that
/// resets it (a description doesn't): the menu's text has that one.
#[test]
fn a_menu_keeps_the_cache_id_its_config_computed() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "kept cache ID");
    let p = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    let h = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(p)]);
    sc.call(
        ra(cfg, "setDescription"),
        vec![s("kept cache ID, described again")],
    );
    sc.repr(r(h));
    sc.repr(r(p));
    sc.call(ra(cfg, "getCacheID"), vec![]);
    run(&[sc]);
}

/// A menu the library made earlier for parameters of the same text (of another config whose
/// text is the same) shares the config of the parameters that made it first.
#[test]
fn a_menu_made_again_shares_the_first_config() {
    let mut sc = Script::new();
    let first = config(&mut sc, "menu made again");
    let second = config(&mut sc, "menu made again");
    let p1 = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(first)]);
    let h1 = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(p1)]);
    let p2 = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(second)]);
    let h2 = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(p2)]);
    // A role resets the config's cache ID (a description doesn't).
    sc.call(ra(first, "setRole"), vec![s("again_first"), s("raw")]);
    sc.repr(r(h1));
    sc.repr(r(h2));
    sc.repr(r(p2));
    sc.call(ra(second, "setRole"), vec![s("again_second"), s("lin_1")]);
    sc.repr(r(h2));
    sc.repr(r(p2));
    run(&[sc]);
}

/// `ColorSpaceHelpers.AddColorSpace`.
#[test]
fn add_color_space_behaves_as_the_wheels() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "add color space");
    let add = "OCIO.ColorSpaceHelpers.AddColorSpace";
    sc.call(
        n(add),
        vec![r(cfg), s("added_1"), s("lut.clf"), s("file-io"), s("lin_1")],
    );
    sc.call(
        n(add),
        vec![
            r(cfg),
            s("added_2"),
            s("lut.clf"),
            s("unused-category"),
            s("log_1"),
        ],
    );
    sc.call(
        n(add),
        vec![r(cfg), s("added_3"), s("lut.clf"), none(), s("in_1")],
    );
    sc.call_kw(
        n(add),
        vec![],
        vec![
            ("config", r(cfg)),
            ("name", s("added_4")),
            ("transformFilePath", s("lut.csp")),
            ("connectionColorSpaceName", s("view_1")),
        ],
        Expect::Value,
    );
    sc.call(ra(cfg, "serialize"), vec![]);
    for args in [
        vec![r(cfg), s("raw"), s("lut.clf"), s(""), s("lin_1")],
        vec![r(cfg), s("added_5"), s("lut.clf"), s(""), s("")],
        vec![r(cfg), s("added_5"), s("lut.clf"), s(""), s("nope")],
        vec![r(cfg), s("added_5"), s(""), s(""), s("lin_1")],
        vec![r(cfg), none(), s("lut.clf"), s(""), s("lin_1")],
        vec![none(), s("added_5"), s("lut.clf"), s(""), s("lin_1")],
        vec![r(cfg), s("added_5"), s("lut.clf")],
    ] {
        sc.raises(n(add), args);
    }
    sc.call(ra(cfg, "serialize"), vec![]);
    run(&[sc]);
}

/// Runs `sc` on both sides: each step of `stored` succeeds in the wheel, each of `refused`
/// raises there, and each raises the port's ValueError naming U-112 (an enum value outside the
/// enumerators, which the port's library can't hold).
fn outside_enumerators(sc: &Script, stored: &[usize], refused: &[usize]) {
    start();
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    assert_eq!(theirs["steps"].as_array().unwrap().len(), sc.steps.len());
    assert!(!stored.is_empty() || !refused.is_empty());
    for &k in stored.iter().chain(refused) {
        assert_eq!(
            theirs["steps"][k].get("raised").is_some(),
            refused.contains(&k),
            "{}",
            theirs["steps"][k]
        );
        assert_eq!(
            ours["steps"][k]["raised"]["type"], "builtins.ValueError",
            "{}",
            ours["steps"][k]
        );
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &ours["steps"][k]["raised"]["message"],
        ))
        .unwrap();
        assert!(
            message.ends_with("(docs/improvements.md, U-112)"),
            "{message}"
        );
    }
}

/// A search reference type outside the enumerators, which the wheel's parameters keep (their
/// getter gives it back; a menu of them has no item): the port's error (U-112).
#[test]
fn a_search_reference_type_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "search reference outside");
    let search = sc.call(n("OCIO.SearchReferenceSpaceType"), vec![i(5)]);
    let p = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    let stored = [
        sc.call(ra(p, "setSearchReferenceSpaceType"), vec![r(search)]),
        sc.call_kw(
            n("OCIO.ColorSpaceMenuParameters"),
            vec![r(cfg)],
            vec![("searchReferenceSpaceType", r(search))],
            Expect::Value,
        ),
    ];
    outside_enumerators(&sc, &stored, &[]);
}
