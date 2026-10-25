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
//! cache IDs).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, d, f, i, l, n, none, r, ra, s, t};
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

/// Starts the interpreter.
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
        "DisplayViewHelpers",
        "ColorSpaceMenuParameters",
        "ColorSpaceMenuHelper",
        "LegacyViewingPipeline",
        "MixingSlider",
        "MixingColorSpaceManager",
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

/// `DisplayViewHelpers`: the processors (their cache IDs, the same object asked twice), the
/// views added and removed, and their errors.
#[test]
fn display_view_helpers_behave_as_the_wheels() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "display view helpers");
    let get = "OCIO.DisplayViewHelpers.GetProcessor";
    let mut processors = Vec::new();
    for (args, kwargs) in [
        (
            vec![r(cfg)],
            vec![
                ("workingSpaceName", s("lin_1")),
                ("displayName", s("DISP_1")),
                ("viewName", s("VIEW_1")),
            ],
        ),
        (
            vec![r(cfg), none(), s("lin_1"), s("DISP_2"), s("VIEW_2")],
            vec![],
        ),
        (
            vec![r(cfg)],
            vec![
                ("workingSpaceName", s("lin_1")),
                ("displayName", s("DISP_1")),
                ("viewName", s("VIEW_1")),
                ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
            ],
        ),
    ] {
        let p = sc.call_kw(n(get), args, kwargs, Expect::Value);
        sc.unrepr(p);
        processors.push(p);
        sc.call(ra(p, "getCacheID"), vec![]);
    }
    let ctx = sc.call(ra(cfg, "getCurrentContext"), vec![]);
    sc.unrepr(ctx);
    let mat = sc.call(
        n("OCIO.MatrixTransform.Scale"),
        vec![l(vec![f(1.5), f(1.25), f(1.0), f(1.0)])],
    );
    let p = sc.call_kw(
        n(get),
        vec![r(cfg), r(ctx), s("lin_1"), s("DISP_1"), s("VIEW_1")],
        vec![("channelView", r(mat))],
        Expect::Value,
    );
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    let again = sc.call_kw(
        n(get),
        vec![r(cfg)],
        vec![
            ("workingSpaceName", s("lin_1")),
            ("displayName", s("DISP_1")),
            ("viewName", s("VIEW_1")),
        ],
        Expect::Value,
    );
    sc.unrepr(again);
    sc.call(n("operator.is_"), vec![r(again), r(processors[0])]);
    for args in [
        vec![r(cfg), none(), s("nope"), s("DISP_1"), s("VIEW_1")],
        vec![r(cfg), none(), s("lin_1"), s("DISP_9"), s("VIEW_1")],
        vec![r(cfg), none(), s("lin_1"), none(), s("VIEW_1")],
        vec![none(), none(), s("lin_1"), s("DISP_1"), s("VIEW_1")],
        vec![r(cfg), none(), s("lin_1"), s("DISP_1"), s("VIEW_1"), r(cfg)],
        vec![r(cfg), s("lin_1"), s("DISP_1"), s("VIEW_1")],
    ] {
        sc.raises(n(get), args);
    }
    let identity = sc.call(
        n("OCIO.DisplayViewHelpers.GetIdentityProcessor"),
        vec![r(cfg)],
    );
    sc.unrepr(identity);
    sc.call(ra(identity, "getCacheID"), vec![]);
    sc.raises(
        n("OCIO.DisplayViewHelpers.GetIdentityProcessor"),
        vec![none()],
    );
    // Views added and removed.
    let add = "OCIO.DisplayViewHelpers.AddDisplayView";
    sc.call_kw(
        n(add),
        vec![],
        vec![
            ("config", r(cfg)),
            ("displayName", s("DISP_1")),
            ("viewName", s("VIEW_5")),
            ("lookName", s("look_3")),
            ("colorSpaceName", s("view_5")),
            ("colorSpaceFamily", s("Display/family")),
            ("colorSpaceDescription", s("a description")),
            ("colorSpaceCategories", s("file-io")),
            ("transformFilePath", s("lut1d_green.ctf")),
            ("connectionColorSpaceName", s("lin_1")),
        ],
        Expect::Value,
    );
    sc.call(
        n(add),
        vec![
            r(cfg),
            s("DISP_NEW"),
            s("VIEW_NEW"),
            s(""),
            s("view_new\u{0}cut"),
            s(""),
            s(""),
            s(""),
            s("lut.clf"),
            s("lin_2"),
        ],
    );
    sc.call(ra(cfg, "serialize"), vec![]);
    // A color space that exists, an empty path, a connection color space that doesn't.
    for (cs, path, connection) in [
        ("view_5", "lut.clf", "lin_1"),
        ("view_51", "", "lin_1"),
        ("view_51", "lut.clf", "nope"),
    ] {
        sc.call_kw(
            n(add),
            vec![],
            vec![
                ("config", r(cfg)),
                ("displayName", s("DISP_1")),
                ("viewName", s("VIEW_6")),
                ("colorSpaceName", s(cs)),
                ("transformFilePath", s(path)),
                ("connectionColorSpaceName", s(connection)),
            ],
            Expect::Raised,
        );
    }
    sc.raises(
        n(add),
        vec![
            r(cfg),
            none(),
            s("v"),
            s(""),
            s(""),
            s(""),
            s(""),
            s(""),
            s("f"),
            s("lin_1"),
        ],
    );
    let remove = "OCIO.DisplayViewHelpers.RemoveDisplayView";
    sc.call(n(remove), vec![r(cfg), s("DISP_1"), s("VIEW_5")]);
    sc.call_kw(
        n(remove),
        vec![],
        vec![
            ("config", r(cfg)),
            ("displayName", s("DISP_NEW")),
            ("viewName", s("VIEW_NEW")),
        ],
        Expect::Value,
    );
    sc.raises(n(remove), vec![r(cfg), s("DISP_1"), s("VIEW_5")]);
    sc.raises(n(remove), vec![r(cfg), none(), s("VIEW_1")]);
    sc.call(ra(cfg, "serialize"), vec![]);
    run(&[sc]);
}

const PIPELINE_GETTERS: [&str; 3] = ["getLooksOverrideEnabled", "getLooksOverride", "__repr__"];

/// The pipeline: its transforms (its own copies, the same objects while they live, changed in
/// place through its getters), the looks override, `repr` and the processors, with their
/// errors (a transform whose direction is outside the enumerators among them, I-261).
#[test]
fn legacy_viewing_pipeline_behaves_as_the_wheels() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "pipeline");
    let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
    sc.eval(r(vp), &PIPELINE_GETTERS);
    sc.raises(n("OCIO.LegacyViewingPipeline"), vec![i(1)]);
    let none_proc = sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    let _ = none_proc;
    let dvt = sc.call_kw(
        n("OCIO.DisplayViewTransform"),
        vec![],
        vec![
            ("src", s("in_1")),
            ("display", s("DISP_2")),
            ("view", s("VIEW_2")),
        ],
        Expect::Value,
    );
    sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
    let held = sc.call(ra(vp, "getDisplayViewTransform"), vec![]);
    sc.call(n("operator.is_"), vec![r(held), r(dvt)]);
    let held2 = sc.call(ra(vp, "getDisplayViewTransform"), vec![]);
    sc.call(n("operator.is_"), vec![r(held), r(held2)]);
    sc.eval(r(dvt), &["getLooksBypass"]);
    sc.eval(r(held), &["getLooksBypass", "getSrc"]);
    sc.eval(r(vp), &PIPELINE_GETTERS);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    // The copy changed in place: the pipeline's.
    sc.call(ra(held, "setLooksBypass"), vec![b(false)]);
    sc.eval(r(vp), &PIPELINE_GETTERS);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    // The other transforms.
    let mat = sc.call(
        n("OCIO.MatrixTransform.Scale"),
        vec![l(vec![f(1.1), f(1.2), f(1.1), f(1.0)])],
    );
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_RED_MOD_03")],
    );
    let expo = sc.call(
        n("OCIO.ExponentTransform"),
        vec![l(vec![f(1.25), f(1.25), f(1.25), f(1.0)])],
    );
    let grp = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(grp, "appendTransform"), vec![r(expo)]);
    let timing = sc.call(n("OCIO.MatrixTransform"), vec![]);
    for (set, get, t) in [
        ("setChannelView", "getChannelView", mat),
        ("setLinearCC", "getLinearCC", ff),
        ("setDisplayCC", "getDisplayCC", grp),
        ("setColorTimingCC", "getColorTimingCC", timing),
    ] {
        sc.eval(r(vp), &[get]);
        sc.call(ra(vp, set), vec![r(t)]);
        let got = sc.call(ra(vp, get), vec![]);
        sc.call(n("operator.is_"), vec![r(got), r(t)]);
        let again = sc.call(ra(vp, get), vec![]);
        sc.call(n("operator.is_"), vec![r(got), r(again)]);
        sc.call(n("type"), vec![r(got)]);
        sc.raises(ra(vp, set), vec![]);
        sc.raises(ra(vp, set), vec![b(false)]);
        sc.raises(ra(vp, set), vec![n("OCIO.TRANSFORM_DIR_FORWARD")]);
    }
    sc.eval(r(vp), &PIPELINE_GETTERS);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    sc.call(ra(vp, "setColorTimingCC"), vec![none()]);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    // Changed in place through the getters; the group's copy shares its children.
    let cv = sc.call(ra(vp, "getChannelView"), vec![]);
    sc.call(
        ra(cv, "setOffset"),
        vec![l(vec![f(0.125), f(0.25), f(0.5), f(0.0)])],
    );
    sc.call(
        ra(expo, "setValue"),
        vec![l(vec![f(2.0), f(2.0), f(2.0), f(1.0)])],
    );
    sc.eval(r(vp), &PIPELINE_GETTERS);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    let ctx = sc.call(ra(cfg, "getCurrentContext"), vec![]);
    sc.unrepr(ctx);
    let p = sc.call_kw(
        ra(vp, "getProcessor"),
        vec![],
        vec![("config", r(cfg)), ("context", r(ctx))],
        Expect::Value,
    );
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    sc.raises(ra(vp, "getProcessor"), vec![none()]);
    sc.raises(ra(vp, "getProcessor"), vec![]);
    // Looks override.
    sc.call(ra(vp, "setLooksOverrideEnabled"), vec![b(true)]);
    sc.call(ra(vp, "setLooksOverride"), vec![s("look_3")]);
    sc.eval(r(vp), &PIPELINE_GETTERS);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    sc.call(ra(vp, "setLooksOverride"), vec![s("nope")]);
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.call(ra(vp, "setLooksOverrideEnabled"), vec![none()]);
    for bad in [vec![], vec![n("OCIO.TRANSFORM_DIR_FORWARD")], vec![s("no")]] {
        sc.raises(ra(vp, "setLooksOverrideEnabled"), bad);
    }
    for bad in [vec![none()], vec![], vec![b(false)]] {
        sc.raises(ra(vp, "setLooksOverride"), bad);
    }
    sc.eval(r(vp), &PIPELINE_GETTERS);
    // An invalid transform, and a direction outside the enumerators.
    sc.call(ra(vp, "setLooksOverride"), vec![s("")]);
    let bad_dir = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    sc.call(ra(cv, "setDirection"), vec![r(bad_dir)]);
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.call(
        ra(cv, "setDirection"),
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
    );
    let held_dvt = sc.call(ra(vp, "getDisplayViewTransform"), vec![]);
    sc.call(ra(held_dvt, "setSrc"), vec![s("")]);
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.call(ra(held_dvt, "setSrc"), vec![s("in_1")]);
    for set in [
        "setChannelView",
        "setLinearCC",
        "setColorTimingCC",
        "setDisplayCC",
        "setDisplayViewTransform",
    ] {
        sc.call(ra(vp, set), vec![none()]);
    }
    sc.eval(r(vp), &PIPELINE_GETTERS);
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    run(&[sc]);
}

const MIXING_GETTERS: [&str; 4] = [
    "getSelectedMixingSpaceIdx",
    "getSelectedMixingEncodingIdx",
    "isPerceptuallyUniform",
    "__repr__",
];

/// The manager: its mixing spaces and encodings (I-396's `__next__`), their selection, the
/// processors, the slider (the same object), the config (its later changes seen, `refresh`),
/// and the color picking role.
#[test]
fn mixing_helpers_behave_as_the_wheels() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "mixing");
    let m = sc.call(n("OCIO.MixingColorSpaceManager"), vec![r(cfg)]);
    sc.eval(r(m), &MIXING_GETTERS);
    for args in [vec![], vec![none()], vec![i(1)]] {
        sc.raises(n("OCIO.MixingColorSpaceManager"), args);
    }
    let kw = sc.call_kw(
        n("OCIO.MixingColorSpaceManager"),
        vec![],
        vec![("config", r(cfg))],
        Expect::Value,
    );
    sc.repr(r(kw));
    // The mixing spaces.
    let spaces = sc.call(ra(m, "getMixingSpaces"), vec![]);
    sc.unrepr(spaces);
    iterator(&mut sc, spaces, 2, Expect::Raised);
    let spaces2 = sc.call(ra(m, "getMixingSpaces"), vec![]);
    sc.unrepr(spaces2);
    sc.call(n("list"), vec![r(spaces2)]);
    // The encodings: `next` of their iterator is refused, the method walks a space iterator.
    let encodings = sc.call(ra(m, "getMixingEncodings"), vec![]);
    sc.unrepr(encodings);
    sc.call(n("len"), vec![r(encodings)]);
    for k in [0, 1] {
        sc.call(ra(encodings, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(encodings, "__getitem__"), vec![i(-1)]);
    sc.raises(ra(encodings, "__getitem__"), vec![i(2)]);
    let same = sc.call(n("iter"), vec![r(encodings)]);
    sc.call(n("operator.is_"), vec![r(same), r(encodings)]);
    sc.raises(n("next"), vec![r(encodings)]);
    sc.raises(n("list"), vec![r(encodings)]);
    let walked = sc.call(ra(m, "getMixingSpaces"), vec![]);
    sc.unrepr(walked);
    let next = "OCIO.MixingColorSpaceManager.MixingEncodingIterator.__next__";
    sc.call(n(next), vec![r(walked)]);
    sc.call(n(next), vec![r(walked)]);
    sc.raises(n(next), vec![r(walked)]);
    sc.raises(n(next), vec![r(encodings)]);
    // The selections.
    sc.call(ra(m, "setSelectedMixingSpaceIdx"), vec![i(1)]);
    sc.eval(r(m), &MIXING_GETTERS);
    sc.raises(ra(m, "setSelectedMixingSpaceIdx"), vec![i(2)]);
    sc.raises(ra(m, "setSelectedMixingSpaceIdx"), vec![i(-1)]);
    sc.raises(ra(m, "setSelectedMixingSpaceIdx"), vec![none()]);
    sc.call(ra(m, "setSelectedMixingSpace"), vec![s("Rendering Space")]);
    sc.raises(ra(m, "setSelectedMixingSpace"), vec![s("nope")]);
    sc.raises(ra(m, "setSelectedMixingSpace"), vec![none()]);
    sc.call(ra(m, "setSelectedMixingEncoding"), vec![s("HSV")]);
    sc.eval(r(m), &MIXING_GETTERS);
    sc.raises(ra(m, "setSelectedMixingEncoding"), vec![s("HS")]);
    for bad in [none(), i(1), n("OCIO.TRANSFORM_DIR_FORWARD")] {
        sc.raises(ra(m, "setSelectedMixingEncoding"), vec![bad]);
    }
    sc.call(ra(m, "setSelectedMixingEncodingIdx"), vec![i(0)]);
    sc.raises(ra(m, "setSelectedMixingEncodingIdx"), vec![i(2)]);
    for bad in [none(), s("test")] {
        sc.raises(ra(m, "setSelectedMixingEncodingIdx"), vec![bad]);
    }
    // The processors.
    let mut first = None;
    for (space, encoding, dir) in [
        (0, 0, "OCIO.TRANSFORM_DIR_FORWARD"),
        (1, 0, "OCIO.TRANSFORM_DIR_FORWARD"),
        (1, 1, "OCIO.TRANSFORM_DIR_INVERSE"),
        (0, 1, "OCIO.TRANSFORM_DIR_FORWARD"),
    ] {
        sc.call(ra(m, "setSelectedMixingSpaceIdx"), vec![i(space)]);
        sc.call(ra(m, "setSelectedMixingEncodingIdx"), vec![i(encoding)]);
        let p = sc.call_kw(
            ra(m, "getProcessor"),
            vec![],
            vec![
                ("workingSpaceName", s("lin_1")),
                ("displayName", s("DISP_1")),
                ("viewName", s("VIEW_1")),
                ("direction", n(dir)),
            ],
            Expect::Value,
        );
        sc.unrepr(p);
        sc.call(ra(p, "getCacheID"), vec![]);
        first.get_or_insert(p);
    }
    let again = sc.call(
        ra(m, "getProcessor"),
        vec![s("lin_1"), s("DISP_1"), s("VIEW_1")],
    );
    sc.unrepr(again);
    sc.call(ra(again, "getCacheID"), vec![]);
    let again2 = sc.call(
        ra(m, "getProcessor"),
        vec![s("lin_1"), s("DISP_1"), s("VIEW_1")],
    );
    sc.unrepr(again2);
    sc.call(n("operator.is_"), vec![r(again), r(again2)]);
    sc.call(ra(m, "setSelectedMixingSpaceIdx"), vec![i(1)]);
    for args in [
        vec![s("lin_1"), none(), s("VIEW_1")],
        vec![s("lin_1"), i(666), s("VIEW_1")],
        vec![s("lin_1"), s("DISP_1")],
        vec![s("lin_1"), s(""), s("VIEW_1")],
        vec![s(""), s("DISP_1"), s("VIEW_1")],
        vec![s("not found"), s("DISP_1"), s("VIEW_1")],
    ] {
        sc.raises(ra(m, "getProcessor"), args);
    }
    // The slider: the manager's own, the same object.
    let slider = sc.call(ra(m, "getSlider"), vec![]);
    let slider2 = sc.call_kw(
        ra(m, "getSlider"),
        vec![],
        vec![
            ("sliderMixingMinEdge", f(0.0)),
            ("sliderMixingMaxEdge", f(1.0)),
        ],
        Expect::Value,
    );
    sc.call(n("operator.is_"), vec![r(slider), r(slider2)]);
    let getters = ["getSliderMinEdge", "getSliderMaxEdge", "__repr__"];
    sc.eval(r(slider), &getters);
    for space in [0, 1] {
        sc.call(ra(m, "setSelectedMixingSpaceIdx"), vec![i(space)]);
        for (min, max) in [(0.0, 1.0), (-0.25, 0.75), (0.5, 0.5), (0.125, 8.0)] {
            sc.call(ra(slider, "setSliderMinEdge"), vec![f(min)]);
            sc.call(ra(slider, "setSliderMaxEdge"), vec![f(max)]);
            sc.eval(r(slider), &getters);
            for v in [0.0, 0.1, 0.5, 0.9, 1.0, -0.5, 2.0] {
                sc.call_kw(
                    ra(slider, "sliderToMixing"),
                    vec![],
                    vec![("sliderUnits", f(v))],
                    Expect::Value,
                );
                sc.call_kw(
                    ra(slider, "mixingToSlider"),
                    vec![],
                    vec![("mixingUnits", f(v))],
                    Expect::Value,
                );
            }
        }
        sc.call(ra(m, "getSlider"), vec![f(0.25), f(4.0)]);
        sc.eval(r(slider), &getters);
    }
    sc.raises(n("OCIO.MixingSlider"), vec![]);
    sc.raises(ra(slider, "setSliderMinEdge"), vec![none()]);
    sc.raises(ra(m, "getSlider"), vec![f(0.0)]);
    sc.raises(ra(m, "getSlider"), vec![none(), f(1.0)]);
    sc.repr(r(m));
    // A description doesn't reset the cache ID the config computed (the manager's first repr).
    sc.call(
        ra(cfg, "setDescription"),
        vec![s("mixing, described again")],
    );
    sc.repr(r(m));
    // The config: its changes are the manager's (processors, repr); `refresh` takes the color
    // picking role.
    sc.call(ra(m, "setSelectedMixingSpaceIdx"), vec![i(1)]);
    sc.call_kw(
        ra(cfg, "addDisplayView"),
        vec![],
        vec![
            ("display", s("DISP_1")),
            ("view", s("VIEW_1")),
            ("colorSpaceName", s("log_1")),
        ],
        Expect::Value,
    );
    let p = sc.call(
        ra(m, "getProcessor"),
        vec![s("lin_1"), s("DISP_1"), s("VIEW_1")],
    );
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    sc.call(ra(m, "setSelectedMixingSpaceIdx"), vec![i(0)]);
    sc.call(
        ra(cfg, "setRole"),
        vec![n("OCIO.ROLE_COLOR_PICKING"), s("log_1")],
    );
    sc.eval(r(m), &MIXING_GETTERS);
    let p = sc.call(
        ra(m, "getProcessor"),
        vec![s("lin_1"), s("DISP_1"), s("VIEW_1")],
    );
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    sc.call(ra(m, "refresh"), vec![r(cfg)]);
    sc.eval(r(m), &MIXING_GETTERS);
    let spaces3 = sc.call(ra(m, "getMixingSpaces"), vec![]);
    sc.unrepr(spaces3);
    sc.call(n("list"), vec![r(spaces3)]);
    sc.call(ra(m, "setSelectedMixingEncodingIdx"), vec![i(1)]);
    let p = sc.call(
        ra(m, "getProcessor"),
        vec![s("lin_1"), s("DISP_1"), s("VIEW_1")],
    );
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    sc.eval(r(slider), &getters);
    sc.raises(ra(m, "refresh"), vec![none()]);
    let other = config(&mut sc, "mixing, other");
    sc.call_kw(
        ra(m, "refresh"),
        vec![],
        vec![("config", r(other))],
        Expect::Value,
    );
    sc.eval(r(m), &MIXING_GETTERS);
    let _ = first;
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

/// A direction outside the enumerators, which the wheel's display view helper hands to its
/// display view transform, whose validation refuses it: the port's error (U-112).
#[test]
fn a_display_view_direction_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "display view direction outside");
    let dir = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let refused = sc.call_kw(
        n("OCIO.DisplayViewHelpers.GetProcessor"),
        vec![r(cfg)],
        vec![
            ("workingSpaceName", s("lin_1")),
            ("displayName", s("DISP_1")),
            ("viewName", s("VIEW_1")),
            ("direction", r(dir)),
        ],
        Expect::Raised,
    );
    outside_enumerators(&sc, &[], &[refused]);
}

/// A config whose view `vv` shows the color space `dyn` (its transform set by the script).
const DYNAMIC_VIEW_CONFIG: &str = "ocio_profile_version: 2\nroles: {default: raw, scene_linear: raw}\n\
     displays: {dd: [!<View> {name: vv, colorspace: dyn}]}\n\
     colorspaces: [!<ColorSpace> {name: raw}, !<ColorSpace> {name: dyn}]\n";

/// The same direction once the config's processor cache holds the view's inverse processor,
/// whose cache key is the same (the direction's text is `inverse`, ParseUtils.cpp:133-138): the
/// wheel's cache answers without validating, and the pipeline the helper builds refuses the
/// direction (`LegacyViewingPipelineImpl::validate`'s text), or, for a view whose processor
/// has a dynamic exposure and gamma and without a channel view, the helper returns the cached
/// processor (DisplayViewHelpers.cpp:54-88). The port's error (U-112) in each case.
#[test]
fn a_display_view_direction_outside_the_enumerators_after_the_inverse_is_the_ports_error() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "display view direction outside, cached");
    let dir = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let get = |sc: &mut Script, cfg: usize, names: [&str; 3], dir: Value, expect: Expect| {
        sc.call_kw(
            n("OCIO.DisplayViewHelpers.GetProcessor"),
            vec![r(cfg)],
            vec![
                ("workingSpaceName", s(names[0])),
                ("displayName", s(names[1])),
                ("viewName", s(names[2])),
                ("direction", dir),
            ],
            expect,
        )
    };
    let sample = ["lin_1", "DISP_1", "VIEW_1"];
    let first = get(&mut sc, cfg, sample, r(dir), Expect::Raised);
    let p = get(
        &mut sc,
        cfg,
        sample,
        n("OCIO.TRANSFORM_DIR_INVERSE"),
        Expect::Value,
    );
    sc.unrepr(p);
    let cached = get(&mut sc, cfg, sample, r(dir), Expect::Raised);
    // A view whose processor is dynamic.
    let dyn_cfg = sc.call(
        n("OCIO.Config.CreateFromStream"),
        vec![s(DYNAMIC_VIEW_CONFIG)],
    );
    let ec = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![("style", n("OCIO.EXPOSURE_CONTRAST_LINEAR"))],
        Expect::Value,
    );
    sc.call(ra(ec, "makeExposureDynamic"), vec![]);
    sc.call(ra(ec, "makeGammaDynamic"), vec![]);
    let cs = sc.call(ra(dyn_cfg, "getColorSpace"), vec![s("dyn")]);
    sc.call(
        ra(cs, "setTransform"),
        vec![r(ec), n("OCIO.COLORSPACE_DIR_FROM_REFERENCE")],
    );
    sc.call(ra(dyn_cfg, "addColorSpace"), vec![r(cs)]);
    let names = ["raw", "dd", "vv"];
    let dyn_first = get(&mut sc, dyn_cfg, names, r(dir), Expect::Raised);
    let p = get(
        &mut sc,
        dyn_cfg,
        names,
        n("OCIO.TRANSFORM_DIR_INVERSE"),
        Expect::Value,
    );
    sc.unrepr(p);
    let returned = get(&mut sc, dyn_cfg, names, r(dir), Expect::Value);
    sc.unrepr(returned);
    outside_enumerators(&sc, &[returned], &[first, cached, dyn_first]);
    // The wheel's texts: the pipeline's for the cached processor only.
    let theirs = common::wheel(&[sc.json()]).remove(0);
    for (k, pipeline) in [(first, false), (cached, true), (dyn_first, false)] {
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &theirs["steps"][k]["raised"]["message"],
        ))
        .unwrap();
        assert_eq!(
            message.starts_with("LegacyViewingPipeline is not valid: "),
            pipeline,
            "{message}"
        );
        assert!(message.contains("DisplayViewTransform"), "{message}");
    }
}

/// A direction outside the enumerators, with which the wheel's manager makes a processor: the
/// port's error (U-112).
#[test]
fn a_mixing_direction_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "mixing direction outside");
    let dir = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let m = sc.call(n("OCIO.MixingColorSpaceManager"), vec![r(cfg)]);
    let stored = sc.call(
        ra(m, "getProcessor"),
        vec![s("lin_1"), s("DISP_1"), s("VIEW_1"), r(dir)],
    );
    outside_enumerators(&sc, &[stored], &[]);
}

// ---------------------------------------------------------------------------------------------
// Python code the library runs while a helper's call works on it

/// How long a guarded test may take (a pass takes seconds).
const LIMIT: std::time::Duration = std::time::Duration::from_secs(120);

/// Runs `test` on a thread of its own and fails if it panics or takes longer than [`LIMIT`]:
/// a regression can leave a thread waiting on a helper's lock with the GIL held, which no
/// Python timeout interrupts (as `config_wait_cycles.rs`).
fn guarded(test: impl FnOnce() + Send + 'static) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(LIMIT) {
        Ok(Ok(())) => {}
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => panic!("no end within {LIMIT:?}: a helper's lock waits for itself"),
    }
}

/// Configs read through a proxy whose LUT loader calls `read()` and records what it returned
/// (or raised), and the helpers' calls that load a LUT through it, in this thread and with
/// another thread reading the helper while the loader waits for it. Each config's LUT has a
/// name and a hash of its own (`RUN` tells this test's from another's), so no file cache
/// answers for the proxy. Every wait has a timeout.
const HELPER_CALLBACKS: &std::ffi::CStr = c"
import threading
Y = '''ocio_profile_version: 2
roles: {default: raw, scene_linear: raw, color_picking: lut}
displays: {d: [!<View> {name: v, colorspace: raw}]}
colorspaces:
  - !<ColorSpace> {name: raw}
  - !<ColorSpace> {name: lut, from_scene_reference: !<FileTransform> {src: NAME.spi1d}}
'''
LUT = list(b'Version 1\\nFrom 0 1\\nLength 2\\nComponents 1\\n{\\n0\\n0.5\\n}\\n')
count = [0]

def outcome(f):
    try:
        return ('ok', f())
    except Exception as e:
        return (type(e).__name__, str(e))

class Proxy(OCIO.PyConfigIOProxy):
    def __init__(s, read):
        super().__init__()
        count[0] += 1
        s.name = 'helper_%d_%d' % (RUN, count[0])
        s.read = read
        s.seen = []
    def getConfigData(s):
        return Y.replace('NAME', s.name)
    def getLutData(s, path):
        s.seen.append(outcome(s.read))
        return LUT
    def getFastLutFileHash(s, path):
        return 'h' + s.name

def pipeline(read):
    p = OCIO.LegacyViewingPipeline()
    px = Proxy(lambda: read(p))
    cfg = OCIO.Config.CreateFromConfigIOProxy(px)
    p.setDisplayViewTransform(OCIO.DisplayViewTransform(src='raw', display='d', view='v'))
    p.setLinearCC(OCIO.FileTransform(src=px.name + '.spi1d'))
    p.setLooksOverride('looks from the test')
    p.setLooksOverrideEnabled(False)
    return p, px, cfg

def manager(read):
    holder = []
    px = Proxy(lambda: read(holder[0]))
    cfg = OCIO.Config.CreateFromConfigIOProxy(px)
    m = OCIO.MixingColorSpaceManager(cfg)
    holder.append(m)
    m.setSelectedMixingEncodingIdx(1)
    return m, px, cfg

def pipeline_reads():
    p, px, cfg = pipeline(lambda p: (p.getLooksOverride(), p.getLooksOverrideEnabled(),
                                     type(repr(p)).__name__, type(p.getLinearCC()).__name__))
    done = outcome(lambda: type(p.getProcessor(cfg)).__name__)
    return done, px.seen

def manager_reads():
    m, px, cfg = manager(lambda m: (m.getSelectedMixingEncodingIdx(),
                                    type(m.getSlider()).__name__,
                                    type(repr(m)).__name__, len(m.getMixingSpaces())))
    done = outcome(lambda: type(m.getProcessor('raw', 'd', 'v')).__name__)
    return done, px.seen

def two_threads(make, read, call):
    entered, answered = threading.Event(), threading.Event()
    def wait():
        entered.set()
        return answered.wait(20)
    obj, px, cfg = make(lambda o: wait())
    got = {}
    def other():
        entered.wait(20)
        got['read'] = outcome(lambda: read(obj))
        answered.set()
    t = threading.Thread(target=other, daemon=True)
    t.start()
    done = outcome(lambda: type(call(obj, cfg)).__name__)
    t.join(30)
    return done, px.seen, got.get('read'), t.is_alive()

def pipeline_two_threads():
    return two_threads(pipeline, lambda p: p.getLooksOverride(), lambda p, c: p.getProcessor(c))

def manager_two_threads():
    return two_threads(manager, lambda m: m.getSelectedMixingEncodingIdx(),
                       lambda m, c: m.getProcessor('raw', 'd', 'v'))

";

/// The globals [`HELPER_CALLBACKS`] runs in: the module and `RUN` (a number of this call's own).
fn helper_globals(py: Python<'_>) -> Bound<'_, pyo3::types::PyDict> {
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let globals = pyo3::types::PyDict::new(py);
    globals
        .set_item("OCIO", py.import("PyOpenColorIO").unwrap())
        .unwrap();
    globals
        .set_item(
            "RUN",
            RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        )
        .unwrap();
    py.run(HELPER_CALLBACKS, Some(&globals), None)
        .unwrap_or_else(|e| panic!("{e}"));
    globals
}

/// The value of `expr` in `globals`.
fn eval_in<'py>(
    globals: &Bound<'py, pyo3::types::PyDict>,
    expr: &std::ffi::CStr,
) -> Bound<'py, PyAny> {
    globals
        .py()
        .eval(expr, Some(globals), None)
        .unwrap_or_else(|e| panic!("{expr:?}: {e}"))
}

/// `value == expected` in Python (`expected` a Python expression of the test's own values).
fn assert_py_eq(
    globals: &Bound<'_, pyo3::types::PyDict>,
    value: &Bound<'_, PyAny>,
    expected: &str,
) {
    let code = std::ffi::CString::new(expected).unwrap();
    let expected_value = eval_in(globals, &code);
    assert!(
        value.eq(&expected_value).unwrap(),
        "{} != {expected}",
        value.repr().unwrap()
    );
}

/// A pipeline's processor loads a LUT through the config's proxy, whose loader reads the
/// pipeline (its looks override, `repr`, a transform): the wheel's call works on the pipeline
/// it reads (`LegacyViewingPipelineImpl::getProcessor` takes no lock), and so does the port's,
/// in this thread and in another one while the loader waits for it. The values read are the
/// test's own.
///
/// The pipeline reads its LUT once, as in the wheel's process: its linear CC's processor
/// (LegacyViewingPipeline.cpp:264) puts it in the library's file cache, from which the
/// pipeline's processor (:380) has it. The test runs in its turn ([`common::turn`]): another
/// test's script clearing that cache (`proxy_of`'s `ClearAllCaches`) between the two makes the
/// second read the file again.
#[test]
fn a_callback_reads_the_pipeline_that_makes_a_processor() {
    guarded(|| {
        start();
        let _turn = common::turn();
        Python::attach(|py| {
            let globals = helper_globals(py);
            let got = eval_in(&globals, c"pipeline_reads()");
            let read = "('ok', ('looks from the test', False, 'str', 'FileTransform'))";
            assert_py_eq(&globals, &got, &format!("(('ok', 'Processor'), [{read}])"));
            let got = eval_in(&globals, c"pipeline_two_threads()");
            assert_py_eq(
                &globals,
                &got,
                "(('ok', 'Processor'), [('ok', True)], ('ok', 'looks from the test'), False)",
            );
        });
    });
}

/// As [`a_callback_reads_the_pipeline_that_makes_a_processor`], for a mixing manager whose
/// color picking role's color space reads a LUT (`MixingColorSpaceManagerImpl::getProcessor`):
/// the encoding the test selected, the slider, and one mixing space, the color picking role's
/// (MixingHelpers.cpp:150-154 @ v2.5.2). In its turn too ([`common::turn`]).
#[test]
fn a_callback_reads_the_manager_that_makes_a_processor() {
    guarded(|| {
        start();
        let _turn = common::turn();
        Python::attach(|py| {
            let globals = helper_globals(py);
            let got = eval_in(&globals, c"manager_reads()");
            assert_py_eq(
                &globals,
                &got,
                "(('ok', 'Processor'), [('ok', (1, 'MixingSlider', 'str', 1))])",
            );
            let got = eval_in(&globals, c"manager_two_threads()");
            assert_py_eq(
                &globals,
                &got,
                "(('ok', 'Processor'), [('ok', True)], ('ok', 1), False)",
            );
        });
    });
}

// ---------------------------------------------------------------------------------------------
// The verifier's cases (6.7c)

/// `ACES_GAMUT_COMP_13`'s parameters in upstream's tests (FixedFunctionTransformTest.py:98).
fn gamut_comp_params() -> Value {
    l([1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2]
        .into_iter()
        .map(f)
        .collect())
}

/// A pipeline without a display view transform refuses to make a processor before it looks at
/// its other transforms (`LegacyViewingPipelineImpl::validate`), one of which Python made
/// invalid in place; with one, the invalid transform is the error.
#[test]
fn a_pipeline_without_a_display_view_transform_is_refused_first() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "pipeline without a display view transform");
    let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![
            n("OCIO.FIXED_FUNCTION_ACES_GAMUT_COMP_13"),
            gamut_comp_params(),
        ],
    );
    sc.call(ra(vp, "setLinearCC"), vec![r(ff)]);
    let held = sc.call(ra(vp, "getLinearCC"), vec![]);
    sc.call(ra(held, "setParams"), vec![l(vec![f(1.0)])]);
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    let dvt = sc.call_kw(
        n("OCIO.DisplayViewTransform"),
        vec![],
        vec![
            ("src", s("in_1")),
            ("display", s("DISP_2")),
            ("view", s("VIEW_2")),
        ],
        Expect::Value,
    );
    sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    run(&[sc]);
}

/// The context a pipeline's processor resolves its files in is the one given: a new context
/// whose `LUT` names the file.
#[test]
fn a_pipeline_resolves_files_in_the_context_given() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "pipeline context");
    let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
    let dvt = sc.call_kw(
        n("OCIO.DisplayViewTransform"),
        vec![],
        vec![
            ("src", s("in_1")),
            ("display", s("DISP_2")),
            ("view", s("VIEW_2")),
        ],
        Expect::Value,
    );
    sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
    let ft = sc.call(n("OCIO.FileTransform"), vec![s("$LUT.spi1d")]);
    sc.call(ra(vp, "setLinearCC"), vec![r(ft)]);
    let ctx = sc.call(n("OCIO.Context"), vec![]);
    sc.step(
        serde_json::json!({ "setitem": r(ctx), "index": s("LUT"), "value": s("from_ctx") }),
        Expect::Value,
    );
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg), r(ctx)]);
    sc.call_kw(
        ra(vp, "getProcessor"),
        vec![],
        vec![("config", r(cfg)), ("context", r(ctx))],
        Expect::Raised,
    );
    // The config's own context has no `LUT`.
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    run(&[sc]);
}

/// A display view transform that bypasses its looks, for a view whose looks the config
/// doesn't have: the pipeline doesn't apply them, and makes the processor.
#[test]
fn a_pipeline_bypasses_the_missing_looks_of_its_view() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "pipeline looks bypass");
    sc.call_kw(
        ra(cfg, "addDisplayView"),
        vec![],
        vec![
            ("display", s("DISP_1")),
            ("view", s("VIEW_LOOKS")),
            ("colorSpaceName", s("view_1")),
            ("looks", s("missing_look")),
        ],
        Expect::Value,
    );
    let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
    let dvt = sc.call_kw(
        n("OCIO.DisplayViewTransform"),
        vec![],
        vec![
            ("src", s("lin_1")),
            ("display", s("DISP_1")),
            ("view", s("VIEW_LOOKS")),
        ],
        Expect::Value,
    );
    sc.call(ra(dvt, "setLooksBypass"), vec![b(true)]);
    sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    // Without the bypass, the missing look is the error.
    sc.call(ra(dvt, "setLooksBypass"), vec![b(false)]);
    sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
    sc.raises(ra(vp, "getProcessor"), vec![r(cfg)]);
    run(&[sc]);
}

/// A menu made as the first computation of its config's cache ID; then a color space the
/// config hands out is changed in place (its description), which doesn't reset the config's
/// cache ID: the menu's text and the config keep it (the verifier's M33, M18). The step
/// interpreter writes each step's `repr`, and the parameters' and the menu's compute the cache
/// ID: a role resets it after the parameters are made, and another thread makes the menu,
/// which the library keeps (by its parameters' text) and gives again to the step that asks
/// for it after the change.
#[test]
fn a_menu_made_first_keeps_its_configs_cache_id() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "menu first");
    let p = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    sc.call(ra(cfg, "setRole"), vec![s("menu_first_role"), s("raw")]);
    let thread = sc.call_kw(
        n("threading.Thread"),
        vec![],
        vec![
            ("target", n("OCIO.ColorSpaceMenuHelper")),
            ("args", t(vec![r(p)])),
        ],
        Expect::Value,
    );
    sc.unrepr(thread);
    sc.call(ra(thread, "start"), vec![]);
    sc.call(ra(thread, "join"), vec![]);
    let cs = sc.call(ra(cfg, "getColorSpace"), vec![s("lin_1")]);
    sc.call(ra(cs, "setDescription"), vec![s("changed in place")]);
    let h = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(p)]);
    sc.repr(r(h));
    sc.repr(r(p));
    sc.call(ra(cfg, "getCacheID"), vec![]);
    sc.call(ra(h, "getDescription"), vec![i(0)]);
    run(&[sc]);
}

/// A color picking role: one mixing space and two encodings; the encodings' `__next__` walks a
/// space iterator over the encodings (I-396).
#[test]
fn the_encodings_next_walks_a_space_iterator_of_one_space() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "mixing color picking");
    sc.call(
        ra(cfg, "setRole"),
        vec![n("OCIO.ROLE_COLOR_PICKING"), s("log_1")],
    );
    let m = sc.call(n("OCIO.MixingColorSpaceManager"), vec![r(cfg)]);
    let walked = sc.call(ra(m, "getMixingSpaces"), vec![]);
    sc.unrepr(walked);
    sc.call(n("len"), vec![r(walked)]);
    let next = "OCIO.MixingColorSpaceManager.MixingEncodingIterator.__next__";
    sc.call(n(next), vec![r(walked)]);
    sc.call(n(next), vec![r(walked)]);
    sc.raises(n(next), vec![r(walked)]);
    let spaces = sc.call(ra(m, "getMixingSpaces"), vec![]);
    sc.unrepr(spaces);
    sc.call(n("list"), vec![r(spaces)]);
    let encodings = sc.call(ra(m, "getMixingEncodings"), vec![]);
    sc.unrepr(encodings);
    sc.call(n("len"), vec![r(encodings)]);
    run(&[sc]);
}

/// The submodules are in `sys.modules` under their full names, the module's attributes
/// (pybind11's `def_submodule`); the wheel's have the extension's `__file__`, which the port's
/// `def_submodule` gives them from the module it is given (the module embedded in this test
/// has none).
#[test]
fn the_submodules_are_registered_as_the_wheels() {
    start();
    let mut sc = Script::new();
    for sub in ["ColorSpaceHelpers", "DisplayViewHelpers"] {
        let name = sc.eval(n(&format!("OCIO.{sub}.__name__")), &[]);
        let got = sc.call(n("threading._sys.modules.get"), vec![r(name)]);
        sc.unrepr(got);
        sc.call(n("operator.is_"), vec![r(got), n(&format!("OCIO.{sub}"))]);
    }
    compare(&[sc]);
    // The wheel's `__file__`: the extension's.
    let mut sc = Script::new();
    let mut same = Vec::new();
    for sub in ["ColorSpaceHelpers", "DisplayViewHelpers"] {
        same.push(sc.call(
            n("operator.eq"),
            vec![
                n(&format!("OCIO.{sub}.__file__")),
                n("OCIO.PyOpenColorIO.__file__"),
            ],
        ));
    }
    let theirs = common::wheel(&[sc.json()]).remove(0);
    for k in same {
        assert_eq!(theirs["steps"][k]["value"]["bool"], true, "{theirs}");
    }
    // The port's, of a module with a `__file__`.
    Python::attach(|py| {
        let parent = pyo3::types::PyModule::new(py, "ocio_rs_submodule_probe").unwrap();
        let file = pyo3::types::PyString::new(py, "probe file");
        parent.setattr("__file__", &file).unwrap();
        let sub = ocio_py::pybind11::def_submodule(&parent, "ProbeSubmodule").unwrap();
        assert!(sub.getattr("__file__").unwrap().is(&file));
        assert!(parent.getattr("ProbeSubmodule").unwrap().is(&sub));
        let modules = py.import("sys").unwrap().getattr("modules").unwrap();
        let name = sub.name().unwrap();
        assert!(modules.get_item(&name).unwrap().is(&sub));
    });
}

/// The first control point of the curve whose control points step `points` iterates set to
/// (1.5, 0.5), past the next one: the curve no longer validates.
fn move_point(sc: &mut Script, points: usize) {
    let point = sc.call(n("OCIO.GradingControlPoint"), vec![f(1.5), f(0.5)]);
    sc.step(
        serde_json::json!({ "setitem": r(points), "index": i(0), "value": r(point) }),
        Expect::Value,
    );
}

/// The wheel's process dies where a pipeline's setter copies a transform that doesn't validate
/// (a fixed function's parameters, a curve, changed in place; U-151): the port's setters raise
/// the copy's error, the wheel's text for the same transform made with those values, and the
/// pipeline keeps the transform it had.
#[test]
fn a_pipeline_given_an_invalid_transform_raises() {
    start();
    // Each script gives the pipeline a valid transform, makes the other one and changes it in
    // place, then calls the setter: the wheel dies at the last step.
    let mut scripts = Vec::new();
    for set in [
        "setLinearCC",
        "setColorTimingCC",
        "setChannelView",
        "setDisplayCC",
    ] {
        for curve in [false, true] {
            let mut sc = Script::new();
            let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
            let old = sc.call(n("OCIO.MatrixTransform"), vec![]);
            sc.call(ra(vp, set), vec![r(old)]);
            // The pipeline's copy, the same object while the pipeline holds it.
            let held = sc.call(ra(vp, &set.replacen("set", "get", 1)), vec![]);
            let bad = if curve {
                let t = sc.call(n("OCIO.GradingRGBCurveTransform"), vec![]);
                let v = sc.call(ra(t, "getValue"), vec![]);
                let red = sc.call(ra(v, "red.getControlPoints"), vec![]);
                move_point(&mut sc, red);
                t
            } else {
                let t = sc.call(
                    n("OCIO.FixedFunctionTransform"),
                    vec![
                        n("OCIO.FIXED_FUNCTION_ACES_GAMUT_COMP_13"),
                        gamut_comp_params(),
                    ],
                );
                sc.call(ra(t, "setParams"), vec![l(vec![f(1.0)])]);
                t
            };
            sc.call(ra(vp, set), vec![r(bad)]);
            scripts.push((sc, vp, held, set, curve));
        }
    }
    let json: Vec<Value> = scripts.iter().map(|(sc, ..)| sc.json()).collect();
    let result = ocio_testkit::Oracle::get()
        .call("py_eval", serde_json::json!({ "scripts": json }), &[])
        .result;
    let reports = result["scripts"].as_array().unwrap();
    assert_eq!(reports.len(), scripts.len());
    for (k, report) in reports.iter().enumerate() {
        let code = report["crashed"]["returncode"].as_i64();
        assert!(code.is_some_and(|c| c != 0), "script {k}: {report}");
    }
    // The steps before the setter succeed in the wheel; the wheel's error for the values.
    let mut shortened = Vec::new();
    let mut errors = Vec::new();
    for (sc, ..) in &scripts {
        let mut steps = sc.json();
        steps["steps"].as_array_mut().unwrap().pop();
        shortened.push(steps);
    }
    let mut sc = Script::new();
    let ff = sc.raises(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GAMUT_COMP_13"), l(vec![f(1.0)])],
    );
    let t = sc.call(n("OCIO.GradingRGBCurveTransform"), vec![]);
    let v = sc.call(ra(t, "getValue"), vec![]);
    let red = sc.call(ra(v, "red.getControlPoints"), vec![]);
    move_point(&mut sc, red);
    let curve = sc.raises(ra(t, "setValue"), vec![r(v)]);
    shortened.push(sc.json());
    let theirs = common::wheel(&shortened);
    for report in &theirs[..scripts.len()] {
        for step in report["steps"].as_array().unwrap() {
            assert!(step.get("value").is_some(), "{step}");
        }
    }
    for k in [ff, curve] {
        errors.push(theirs[scripts.len()]["steps"][k]["raised"].clone());
        assert!(
            errors.last().unwrap().is_object(),
            "{}",
            theirs[scripts.len()]
        );
    }
    for (sc, vp, held, set, curve) in &scripts {
        let get = set.replacen("set", "get", 1);
        let (setter, kept) = (sc.steps.len() - 1, sc.steps.len() + 1);
        let mut json = sc.json();
        let steps = json["steps"].as_array_mut().unwrap();
        steps.push(serde_json::json!({ "call": ra(*vp, &get), "args": [] }));
        steps.push(
            serde_json::json!({ "call": n("operator.is_"), "args": [r(setter + 1), r(*held)] }),
        );
        let ours = common::port(&json);
        let steps = ours["steps"].as_array().unwrap();
        let setter = &steps[setter];
        assert_eq!(
            setter["raised"],
            errors[usize::from(*curve)],
            "{set}, curve {curve}: {setter}"
        );
        assert_eq!(steps[kept]["value"]["bool"], true, "{set}: {}", steps[kept]);
    }
}

/// A config read through a proxy: its color space `lut` reads the file `SRC`.
const PROXY_CONFIG: &str = "ocio_profile_version: 2\nroles: {default: raw, scene_linear: raw}\n\
     displays: {d: [!<View> {name: v, colorspace: raw}]}\n\
     colorspaces:\n  - !<ColorSpace> {name: raw}\n  \
     - !<ColorSpace> {name: lut, from_scene_reference: !<FileTransform> {src: SRC}}\n";

/// The LUT the proxies give.
const LUT_TEXT: &[u8] = b"Version 1\nFrom 0 1\nLength 2\nComponents 1\n{\n0\n0.5\n}\n";

/// A subclass of `PyConfigIOProxy` and an instance of it whose config is `config`: the
/// instance's step (as `callbacks_oracle.rs`).
fn proxy_of(sc: &mut Script, config: &str) -> usize {
    // The port's tests share a process, and the library's file cache.
    sc.call(n("OCIO.ClearAllCaches"), vec![]);
    let cls = sc.call(
        n("type"),
        vec![
            s("P"),
            t(vec![n("OCIO.PyConfigIOProxy")]),
            d(vec![(s("__module__"), s("probe"))]),
        ],
    );
    let obj = sc.call(r(cls), vec![]);
    let it = sc.call(n("iter"), vec![l(vec![s(config)])]);
    let hash = sc.call(n("str"), vec![s("h%.0s")]);
    for (name, value) in [
        ("getConfigData", ra(it, "__next__")),
        ("getFastLutFileHash", ra(hash, "__mod__")),
    ] {
        sc.step(
            serde_json::json!({ "setattr": r(obj), "name": name, "value": value }),
            Expect::Value,
        );
    }
    obj
}

/// Gives the proxy `px` a LUT loader that calls `init(path)` (a method of a helper) and returns
/// [`LUT_TEXT`]: a class (the step interpreter defines no functions) of lists whose `__init__`
/// is `init`, and whose length, items and iterator are those of the text's bytes.
fn load_calls(sc: &mut Script, px: usize, init: Value) {
    let data = sc.call(n("list"), vec![by(LUT_TEXT)]);
    let class = sc.call(
        n("type"),
        vec![
            s("LutData"),
            t(vec![n("list")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("__init__"), init),
                (s("__len__"), ra(data, "__len__")),
                (s("__getitem__"), ra(data, "__getitem__")),
                (s("__iter__"), ra(data, "__iter__")),
            ]),
        ],
    );
    sc.step(
        serde_json::json!({ "setattr": r(px), "name": "getLutData", "value": r(class) }),
        Expect::Value,
    );
}

/// A pipeline's processor loads its linear CC's LUT through the config's proxy, whose loader
/// changes the pipeline (its looks override, set to the path it loads) or one of its transforms
/// in place (the source of its display view transform's copy): the wheel's call reads the
/// pipeline again after the loader (LegacyViewingPipeline.cpp:220-330), and makes the
/// processor; the port's raises D-12's error, and the change stands, as in the wheel.
#[test]
fn a_lut_loader_that_changes_the_pipeline_is_d12() {
    start();
    for (k, change) in ["looks", "transform"].into_iter().enumerate() {
        let mut sc = Script::new();
        let lut = format!("lut_pipeline_{k}.spi1d");
        let px = proxy_of(&mut sc, &PROXY_CONFIG.replace("SRC", &lut));
        let cfg = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(px)]);
        let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
        let dvt = sc.call_kw(
            n("OCIO.DisplayViewTransform"),
            vec![],
            vec![("src", s("raw")), ("display", s("d")), ("view", s("v"))],
            Expect::Value,
        );
        sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
        let held = sc.call(ra(vp, "getDisplayViewTransform"), vec![]);
        let init = match change {
            "looks" => ra(vp, "setLooksOverride"),
            _ => ra(held, "setSrc"),
        };
        load_calls(&mut sc, px, init);
        let ft = sc.call(n("OCIO.FileTransform"), vec![s(&lut)]);
        sc.call(ra(vp, "setLinearCC"), vec![r(ft)]);
        sc.eval(r(vp), &PIPELINE_GETTERS);
        let made = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
        sc.eval(r(vp), &PIPELINE_GETTERS);
        sc.eval(r(held), &["getSrc"]);
        let json = sc.json();
        let theirs = common::wheel(std::slice::from_ref(&json)).remove(0);
        let ours = guarded_port(json);
        let (theirs, ours) = (
            theirs["steps"].as_array().unwrap(),
            ours["steps"].as_array().unwrap(),
        );
        assert_eq!(theirs.len(), sc.steps.len());
        assert_eq!(ours.len(), sc.steps.len());
        for (k, (t, o)) in theirs.iter().zip(ours).enumerate() {
            if k == made {
                assert!(t.get("value").is_some(), "{change}: {t}");
                assert_eq!(
                    o["raised"]["type"], "builtins.RuntimeError",
                    "{change}: {o}"
                );
                let message =
                    String::from_utf8(ocio_testkit::oracle_values::bytes(&o["raised"]["message"]))
                        .unwrap();
                assert_eq!(
                    message,
                    "LegacyViewingPipeline: a callback changed the pipeline while the library \
                     made its processor, which OCIO 2.5.2 reads again after that callback and \
                     the port can't follow (docs/deviations.md, D-12)"
                );
            } else {
                assert!(t.get("value").is_some(), "{change}, step {k}: {t}");
                assert_eq!(t, o, "{change}, step {k}");
            }
        }
    }
}

/// The port's report of `script`, run on a thread of its own ([`guarded`]).
fn guarded_port(script: Value) -> Value {
    let (tx, rx) = std::sync::mpsc::channel();
    guarded(move || {
        let _ = tx.send(common::port(&script));
    });
    rx.recv().unwrap()
}

/// As [`a_lut_loader_that_changes_the_pipeline_is_d12`], for a mixing manager whose color
/// picking role's color space reads the LUT: a loader that selects the mixing encoding named by
/// the path it loads (`HSV`), whose processor the wheel makes in that encoding (it reads the
/// encoding after the loader, MixingHelpers.cpp:338), one that changes the config's
/// description, and one that selects a mixing space by the path (which isn't one: the loader
/// raises, and so does the call). The port's manager is read again after the loader too.
#[test]
fn a_lut_loader_changes_the_manager_that_makes_a_processor() {
    let mut sc = Script::new();
    for (src, select, expect) in [
        ("HSV", "setSelectedMixingEncoding", Expect::Value),
        ("lut_mixing_2.spi1d", "setDescription", Expect::Value),
        ("lut_mixing.spi1d", "setSelectedMixingSpace", Expect::Raised),
    ] {
        let config = PROXY_CONFIG
            .replace("SRC", src)
            .replace("scene_linear: raw", "scene_linear: raw, color_picking: lut");
        let px = proxy_of(&mut sc, &config);
        let cfg = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(px)]);
        let m = sc.call(n("OCIO.MixingColorSpaceManager"), vec![r(cfg)]);
        let init = match select {
            "setDescription" => ra(cfg, select),
            _ => ra(m, select),
        };
        load_calls(&mut sc, px, init);
        sc.eval(r(m), &MIXING_GETTERS);
        let p = sc.step(
            serde_json::json!({ "call": ra(m, "getProcessor"), "args": [s("raw"), s("d"), s("v")] }),
            expect,
        );
        if expect == Expect::Value {
            sc.unrepr(p);
            sc.call(ra(p, "getCacheID"), vec![]);
        }
        sc.eval(r(m), &MIXING_GETTERS);
        sc.call(ra(cfg, "getDescription"), vec![]);
    }
    guarded(move || run(&[sc]));
}

// ---------------------------------------------------------------------------------------------
// D-12: what a LUT loader does to the pipeline whose processor loads the LUT (verifier 6.7c,
// second round)

/// D-12's error (docs/deviations.md).
const D12: &str = "LegacyViewingPipeline: a callback changed the pipeline while the library made \
                   its processor, which OCIO 2.5.2 reads again after that callback and the port \
                   can't follow (docs/deviations.md, D-12)";

/// What a LUT loader's call does to the pipeline whose processor loads the LUT.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Effect {
    /// It reads the pipeline's transforms: the port makes the wheel's processor.
    Read,
    /// It changes one in place: the wheel makes its processor, the port raises D-12's error.
    Change,
}

/// A call of a step: its callable and arguments.
type Call = (Value, Vec<Value>);

/// Gives the proxy `px` a LUT loader that returns [`LUT_TEXT`] in an object (a list) whose
/// `__del__` is `on_del`, which runs once the library has read the LUT, during its call: as
/// [`load_calls`], with a callable of no arguments.
fn load_then(sc: &mut Script, px: usize, on_del: Value) {
    let data = sc.call(n("list"), vec![by(LUT_TEXT)]);
    let class = sc.call(
        n("type"),
        vec![
            s("LutData"),
            t(vec![n("list")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("__del__"), on_del),
                (s("__len__"), ra(data, "__len__")),
                (s("__getitem__"), ra(data, "__getitem__")),
                (s("__iter__"), ra(data, "__iter__")),
            ]),
        ],
    );
    sc.step(
        serde_json::json!({ "setattr": r(px), "name": "getLutData", "value": r(class) }),
        Expect::Value,
    );
}

/// A callable of no arguments that calls `target(*args, **kwargs)` once (the step interpreter
/// defines no functions): the `run` of a `threading.Thread` never started. With `check`, the
/// same call is made first, as a step of its own (its arguments are right). The second
/// element is a call to make after the processor: after `run`, the thread has no `_target`
/// (`Thread.run` deletes it).
fn call_once(
    sc: &mut Script,
    target: Value,
    args: Vec<Value>,
    kwargs: Vec<(&str, Value)>,
    check: bool,
) -> (Value, Call) {
    if check {
        sc.call_kw(target.clone(), args.clone(), kwargs.clone(), Expect::Value);
    }
    let kwargs = kwargs.into_iter().map(|(k, v)| (s(k), v)).collect();
    let th = sc.call_kw(
        n("threading.Thread"),
        vec![],
        vec![("target", target), ("args", t(args)), ("kwargs", d(kwargs))],
        Expect::Value,
    );
    // A thread's repr holds its number and whether it is a daemon (the port's tests run
    // these scripts in threads of their own).
    sc.unrepr(th);
    (ra(th, "run"), (n("hasattr"), vec![r(th), s("_target")]))
}

/// A pipeline (a display view transform and a linear CC whose LUT the config's proxy loads,
/// the config's role `color_timing` defined) whose other transforms `setup` sets; `setup` returns the callable the loader calls once the
/// library has the LUT ([`load_then`]) and the calls to make after the processor. The wheel's
/// processor is made in every case; the port's too, the same (each step's report equal, the
/// cache ID included), when the loader only reads, and D-12's error when it changes the
/// pipeline or its transforms (the other steps equal to the wheel's).
fn loader_on_pipeline(
    label: &str,
    effect: Effect,
    setup: impl FnOnce(&mut Script, usize) -> (Value, Vec<Call>),
) {
    let mut sc = Script::new();
    let lut = format!("lut_pipeline_{label}.spi1d");
    // A color timing CC that isn't an identity needs the role.
    let config = PROXY_CONFIG
        .replace("SRC", &lut)
        .replace("scene_linear: raw", "scene_linear: raw, color_timing: raw");
    let px = proxy_of(&mut sc, &config);
    let cfg = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(px)]);
    let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
    let dvt = sc.call_kw(
        n("OCIO.DisplayViewTransform"),
        vec![],
        vec![("src", s("raw")), ("display", s("d")), ("view", s("v"))],
        Expect::Value,
    );
    sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
    let (on_del, after) = setup(&mut sc, vp);
    load_then(&mut sc, px, on_del);
    let ft = sc.call(n("OCIO.FileTransform"), vec![s(&lut)]);
    sc.call(ra(vp, "setLinearCC"), vec![r(ft)]);
    sc.eval(r(vp), &PIPELINE_GETTERS);
    let made = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    let id = (effect == Effect::Read).then(|| sc.call(ra(made, "getCacheID"), vec![]));
    sc.eval(r(vp), &PIPELINE_GETTERS);
    for (target, args) in after {
        sc.call(target, args);
    }
    let json = sc.json();
    let theirs = common::wheel(std::slice::from_ref(&json)).remove(0);
    let ours = guarded_port(json);
    let (theirs, ours) = (
        theirs["steps"].as_array().unwrap(),
        ours["steps"].as_array().unwrap(),
    );
    assert_eq!(theirs.len(), sc.steps.len(), "{label}");
    assert_eq!(ours.len(), sc.steps.len(), "{label}");
    if let Some(id) = id {
        assert!(
            theirs[id]["value"]["str"].is_object(),
            "{label}: {}",
            theirs[id]
        );
    }
    for (k, (t, o)) in theirs.iter().zip(ours).enumerate() {
        // Every step of these scripts succeeds in the wheel.
        assert!(t.get("value").is_some(), "{label}, step {k}: {t}");
        if k == made && effect == Effect::Change {
            assert_eq!(o["raised"]["type"], "builtins.RuntimeError", "{label}: {o}");
            let message =
                String::from_utf8(ocio_testkit::oracle_values::bytes(&o["raised"]["message"]))
                    .unwrap();
            assert_eq!(message, D12, "{label}");
        } else {
            let (mut t, mut o) = (t.clone(), o.clone());
            if sc.unrepr.contains(&k) {
                for v in [&mut t, &mut o] {
                    if let Some(value) = v.get_mut("value").and_then(Value::as_object_mut) {
                        value.remove("repr");
                    }
                }
            }
            assert_eq!(t, o, "{label}, step {k}");
        }
    }
}

/// Sets the pipeline's display CC to a new transform of the class `class` (style log), and
/// returns the transform as the pipeline holds it.
fn display_cc(sc: &mut Script, vp: usize, class: &str) -> usize {
    let cc = sc.call(n(class), vec![n("OCIO.GradingStyle.GRADING_LOG")]);
    sc.call(ra(vp, "setDisplayCC"), vec![r(cc)]);
    sc.call(ra(vp, "getDisplayCC"), vec![])
}

/// Sets the pipeline's color timing CC to a group of a matrix, and returns the group and the
/// matrix as the pipeline holds them.
fn color_timing_group(sc: &mut Script, vp: usize) -> (usize, usize) {
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m)])]);
    sc.call(ra(vp, "setColorTimingCC"), vec![r(g)]);
    let g = sc.call(ra(vp, "getColorTimingCC"), vec![]);
    let child = sc.call(ra(g, "__getitem__"), vec![i(0)]);
    (g, child)
}

/// The null device, which `GroupTransform.write` writes to.
const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// A LUT loader that only reads the pipeline's transforms: a curve transform's `getValue`
/// (which makes the value object of its curves; the port raised D-12, verifier 6.7c F1), and
/// the `const` methods whose names aren't a getter's (`slopesAreDefault` of a transform and of
/// a curve, which the binding counted as a change, `__deepcopy__`, `equals`,
/// `GroupTransform.write`). The wheel and the port make the same processor.
#[test]
fn a_lut_loader_that_reads_the_pipeline_makes_its_processor() {
    start();
    for (label, class) in [
        ("rgb_value", "OCIO.GradingRGBCurveTransform"),
        ("hue_value", "OCIO.GradingHueCurveTransform"),
    ] {
        loader_on_pipeline(label, Effect::Read, |sc, vp| {
            let cc = display_cc(sc, vp, class);
            (ra(cc, "getValue"), vec![(n("repr"), vec![r(cc)])])
        });
    }
    for (label, class, curve) in [
        (
            "rgb_slopes",
            "OCIO.GradingRGBCurveTransform",
            "OCIO.RGBCurveType.RGB_RED",
        ),
        (
            "hue_slopes",
            "OCIO.GradingHueCurveTransform",
            "OCIO.HueCurveType.HUE_SAT",
        ),
    ] {
        loader_on_pipeline(label, Effect::Read, |sc, vp| {
            let cc = display_cc(sc, vp, class);
            let target = ra(cc, "slopesAreDefault");
            let (run, ran) = call_once(sc, target, vec![n(curve)], vec![], true);
            (run, vec![ran])
        });
    }
    loader_on_pipeline("curve_slopes", Effect::Read, |sc, vp| {
        let cc = display_cc(sc, vp, "OCIO.GradingRGBCurveTransform");
        let value = sc.call(ra(cc, "getValue"), vec![]);
        let target = ra(value, "red.slopesAreDefault");
        let (run, ran) = call_once(sc, target, vec![], vec![], true);
        (run, vec![ran])
    });
    loader_on_pipeline("deepcopy", Effect::Read, |sc, vp| {
        let cc = display_cc(sc, vp, "OCIO.GradingRGBCurveTransform");
        let target = ra(cc, "__deepcopy__");
        let (run, ran) = call_once(sc, target, vec![d(vec![])], vec![], true);
        (run, vec![ran])
    });
    loader_on_pipeline("equals", Effect::Read, |sc, vp| {
        let (_, child) = color_timing_group(sc, vp);
        let other = sc.call(n("OCIO.MatrixTransform"), vec![]);
        let (run, ran) = call_once(sc, ra(child, "equals"), vec![r(other)], vec![], true);
        (run, vec![ran])
    });
    loader_on_pipeline("write", Effect::Read, |sc, vp| {
        let (g, _) = color_timing_group(sc, vp);
        let raw = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
        let kwargs = vec![
            ("config", r(raw)),
            ("formatName", s("Color Transform Format")),
            ("fileName", s(NULL_DEVICE)),
        ];
        let (run, ran) = call_once(sc, ra(g, "write"), vec![], kwargs, true);
        (run, vec![ran])
    });
}

/// A LUT loader that changes the pipeline's transforms in place, as Python holds them: a
/// child appended to the group of its color timing CC, that group's child changed, a curve of
/// a curve transform's value changed (a control point, through the curve's iterator), and the
/// hue curve value's "draw curve only". The wheel reads them again after the loader and makes
/// its processor; the port raises D-12's error (verifier 6.7c F2), and the change stands.
#[test]
fn a_lut_loader_that_changes_the_pipelines_transforms_is_d12() {
    start();
    loader_on_pipeline("group_append", Effect::Change, |sc, vp| {
        let (g, _) = color_timing_group(sc, vp);
        let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
        let (run, ran) = call_once(sc, ra(g, "appendTransform"), vec![r(m)], vec![], false);
        (
            run,
            vec![ran, (n("len"), vec![r(g)]), (n("repr"), vec![r(g)])],
        )
    });
    loader_on_pipeline("group_child", Effect::Change, |sc, vp| {
        let (g, child) = color_timing_group(sc, vp);
        let offset = l(vec![f(0.125), f(0.0), f(0.0), f(0.0)]);
        let (run, ran) = call_once(sc, ra(child, "setOffset"), vec![offset], vec![], false);
        (run, vec![ran, (n("repr"), vec![r(g)])])
    });
    loader_on_pipeline("curve_point", Effect::Change, |sc, vp| {
        let cc = display_cc(sc, vp, "OCIO.GradingRGBCurveTransform");
        let value = sc.call(ra(cc, "getValue"), vec![]);
        let points = sc.call(ra(value, "red.getControlPoints"), vec![]);
        let point = sc.call(n("OCIO.GradingControlPoint"), vec![f(0.5), f(0.25)]);
        let target = ra(points, "__setitem__");
        let (run, ran) = call_once(sc, target, vec![i(1), r(point)], vec![], false);
        (run, vec![ran, (n("repr"), vec![r(cc)])])
    });
    loader_on_pipeline("hue_draw", Effect::Change, |sc, vp| {
        let cc = display_cc(sc, vp, "OCIO.GradingHueCurveTransform");
        let value = sc.call(ra(cc, "getValue"), vec![]);
        let target = ra(value, "setDrawCurveOnly");
        let (run, ran) = call_once(sc, target, vec![b(true)], vec![], false);
        (run, vec![ran, (n("repr"), vec![r(cc)])])
    });
}

// ---------------------------------------------------------------------------------------------
// U-182: a cache's mutex locked again by the thread that holds it (verifier 6.7c, second round)

/// Where a LUT loader makes a processor again while the library makes one that loads the LUT.
#[derive(Clone, Copy, Debug)]
enum Again {
    /// The mixing manager's own processor: upstream's config locks its processors' cache, which
    /// the outer call holds (Config.cpp:4830-4832 @ v2.5.2).
    Manager,
    /// A processor of another config over the same file: upstream's file cache locks the file's
    /// entry, which the outer load holds (FileTransform.cpp:837 @ v2.5.2).
    OtherConfig,
}

/// A mixing manager (its config's color picking role reads a LUT through the config's proxy)
/// makes a processor, or its config does, and the proxy's loader makes the processor `again`
/// does (the step interpreter defines no functions: the `run` of a thread never started, as
/// the loaded object's `__del__`, [`load_then`]), whose exception reaches
/// `sys.unraisablehook`, a list's `append` for the script. The script, and its step that
/// reads the exception.
fn relock_script(again: Again) -> (Script, usize) {
    let mut sc = Script::new();
    let lut = format!("lut_relock_{again:?}.spi1d");
    let config = PROXY_CONFIG
        .replace("SRC", &lut)
        .replace("scene_linear: raw", "scene_linear: raw, color_picking: lut");
    let px = proxy_of(&mut sc, &config);
    let cfg = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(px)]);
    let m = sc.call(n("OCIO.MixingColorSpaceManager"), vec![r(cfg)]);
    let (target, args) = match again {
        Again::Manager => (ra(m, "getProcessor"), vec![s("raw"), s("d"), s("v")]),
        Again::OtherConfig => {
            let other = proxy_of(&mut sc, &config);
            let other = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(other)]);
            (ra(other, "getProcessor"), vec![s("raw"), s("lut")])
        }
    };
    let (run, _) = call_once(&mut sc, target, args, vec![], false);
    let got = sc.call(n("list"), vec![]);
    sc.step(
        serde_json::json!({ "setattr": n("threading._sys"), "name": "unraisablehook",
                            "value": ra(got, "append") }),
        Expect::Value,
    );
    load_then(&mut sc, px, run);
    let made = match again {
        Again::Manager => sc.call(ra(m, "getProcessor"), vec![s("raw"), s("d"), s("v")]),
        Again::OtherConfig => sc.call(ra(cfg, "getProcessor"), vec![s("raw"), s("lut")]),
    };
    sc.call(ra(made, "getCacheID"), vec![]);
    sc.step(
        serde_json::json!({ "setattr": n("threading._sys"), "name": "unraisablehook",
                            "value": n("threading._sys.__unraisablehook__") }),
        Expect::Value,
    );
    sc.call(n("len"), vec![r(got)]);
    let first = sc.call(n("operator.getitem"), vec![r(got), i(0)]);
    // It names the object whose `__del__` raised, at its address.
    sc.uncompare(first);
    let raised = sc.eval(ra(first, "exc_value"), &[]);
    (sc, raised)
}

/// U-182: the Windows wheel's C++ runtime throws `std::system_error` ("resource deadlock would
/// occur") where upstream locks a cache's `std::mutex` again on the thread that holds it, as
/// RuntimeError in the loader, and the outer call makes its processor: the port's is the same
/// (each step compared with the wheel). glibc's mutex waits for itself, and the Linux wheel's
/// process hangs (probed in Rocky Linux 9): there, without the wheel, the port's error, and the
/// outer processor. Each on a thread of its own with a time limit ([`guarded`]): a regression
/// waits for itself.
#[test]
fn a_lut_loader_that_makes_the_same_processor_again_is_u182() {
    for again in [Again::Manager, Again::OtherConfig] {
        let (sc, raised) = relock_script(again);
        if cfg!(windows) {
            let theirs = common::wheel(&[sc.json()]).remove(0);
            assert_eq!(
                theirs["steps"][raised]["value"]["type"], "builtins.RuntimeError",
                "{again:?}: {theirs}"
            );
            guarded(move || run(&[sc]));
            continue;
        }
        let ours = guarded_port(sc.json());
        let steps = ours["steps"].as_array().unwrap();
        assert_eq!(steps.len(), sc.steps.len(), "{again:?}");
        for (k, step) in steps.iter().enumerate() {
            assert!(step.get("value").is_some(), "{again:?}, step {k}: {step}");
        }
        let value = &steps[raised]["value"];
        assert_eq!(
            value["type"], "PyOpenColorIO.PyOpenColorIO.Exception",
            "{again:?}: {value}"
        );
        let text = String::from_utf8(ocio_testkit::oracle_values::bytes(&value["repr"])).unwrap();
        assert!(
            text.contains("(docs/improvements.md, U-182)."),
            "{again:?}: {text}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// U-184: a change of a config by the LUT loader of the processor being made of it (verifier
// 6.7c, third round)

/// What makes a processor whose LUT a loader reads through the config's proxy.
#[derive(Clone, Copy, Debug)]
enum Maker {
    /// `MixingColorSpaceManager.getProcessor`: the port works on a copy of the config.
    Manager,
    /// `Config.getProcessor`: the call holds the config, D-9's error for a callback's call.
    Config,
    /// `LegacyViewingPipeline.getProcessor`: as `Config`.
    Pipeline,
    /// `DisplayViewHelpers.GetProcessor`: as `Config`.
    DisplayView,
}

/// A change of the config `cfg` a loader makes: a call and the arguments it is given.
struct Change {
    name: &'static str,
    /// Whether upstream's call locks the config's cache of processors (`resetCacheIDs`,
    /// `clearProcessorCache`, `setProcessorCacheFlags`): the wheel's process ends.
    kills: bool,
    args: fn(&mut Script) -> Vec<Value>,
}

/// The changes of the config the tests try: four that lock the cache of processors, three that
/// do not (`setName` and `setDescription` store a string; `upgradeToLatestVersion` of a config
/// at the latest version does nothing).
fn changes() -> Vec<Change> {
    vec![
        Change {
            name: "setRole",
            kills: true,
            args: |_| vec![s("foo"), s("raw")],
        },
        Change {
            name: "clearProcessorCache",
            kills: true,
            args: |_| vec![],
        },
        Change {
            name: "setProcessorCacheFlags",
            kills: true,
            args: |_| vec![n("OCIO.PROCESSOR_CACHE_ENABLED")],
        },
        Change {
            name: "addColorSpace",
            kills: true,
            args: |sc| {
                let cs = sc.call_kw(
                    n("OCIO.ColorSpace"),
                    vec![],
                    vec![("name", s("zz"))],
                    Expect::Value,
                );
                vec![r(cs)]
            },
        },
        Change {
            name: "setName",
            kills: false,
            args: |_| vec![s("renamed")],
        },
        Change {
            name: "setDescription",
            kills: false,
            args: |_| vec![s("described")],
        },
        Change {
            name: "upgradeToLatestVersion",
            kills: false,
            args: |_| vec![],
        },
    ]
}

/// A script in which a processor is made by `maker` of a config whose proxy's loader (the
/// `__del__` of the object it returns, [`load_then`]) calls `change` on the config, with the
/// processors' cache of the config on or off. The loader is a `Thread.run` of a thread never
/// started ([`call_once`]), so an exception reaches `sys.unraisablehook`, whose reports go to
/// a list. The script, the step of the processor, the step that reads the list's length,
/// the config's name and description, and, when the change locks the cache that is on, the
/// list's first exception.
fn change_script(
    maker: Maker,
    change: &Change,
    cache_off: bool,
    tag: &str,
) -> (Script, usize, usize, Option<usize>) {
    let mut sc = Script::new();
    let lut = format!("lut_u184_{tag}.spi1d");
    let config = PROXY_CONFIG
        .replace("SRC", &lut)
        .replace("colorspace: raw}]}", "colorspace: lut}]}")
        .replace("scene_linear: raw", "scene_linear: raw, color_picking: lut");
    let px = proxy_of(&mut sc, &config);
    let cfg = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(px)]);
    if cache_off {
        sc.call(
            ra(cfg, "setProcessorCacheFlags"),
            vec![n("OCIO.PROCESSOR_CACHE_OFF")],
        );
    }
    let (made_by, made_args) = match maker {
        Maker::Manager => {
            let m = sc.call(n("OCIO.MixingColorSpaceManager"), vec![r(cfg)]);
            (ra(m, "getProcessor"), vec![s("raw"), s("d"), s("v")])
        }
        Maker::Config => (ra(cfg, "getProcessor"), vec![s("raw"), s("lut")]),
        Maker::Pipeline => {
            let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
            let dvt = sc.call_kw(
                n("OCIO.DisplayViewTransform"),
                vec![],
                vec![("src", s("raw")), ("display", s("d")), ("view", s("v"))],
                Expect::Value,
            );
            sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
            (ra(vp, "getProcessor"), vec![r(cfg)])
        }
        Maker::DisplayView => {
            let ctx = sc.call(ra(cfg, "getCurrentContext"), vec![]);
            (
                n("OCIO.DisplayViewHelpers.GetProcessor"),
                vec![r(cfg), r(ctx), s("raw"), s("d"), s("v")],
            )
        }
    };
    let args = (change.args)(&mut sc);
    let (run, _) = call_once(&mut sc, ra(cfg, change.name), args, vec![], false);
    let got = sc.call(n("list"), vec![]);
    sc.step(
        serde_json::json!({ "setattr": n("threading._sys"), "name": "unraisablehook",
                            "value": ra(got, "append") }),
        Expect::Value,
    );
    load_then(&mut sc, px, run);
    let made = sc.call(made_by, made_args);
    sc.call(ra(made, "getCacheID"), vec![]);
    sc.step(
        serde_json::json!({ "setattr": n("threading._sys"), "name": "unraisablehook",
                            "value": n("threading._sys.__unraisablehook__") }),
        Expect::Value,
    );
    let len = sc.call(n("len"), vec![r(got)]);
    sc.eval(r(cfg), &["getName", "getDescription"]);
    // The exception the loader's call ended with, when the change locks the cache.
    let raised = (change.kills && !cache_off).then(|| {
        let first = sc.call(n("operator.getitem"), vec![r(got), i(0)]);
        // It names the object whose `__del__` raised, at its address.
        sc.uncompare(first);
        sc.eval(ra(first, "exc_value"), &[])
    });
    (sc, made, len, raised)
}

const MAKERS: [Maker; 4] = [
    Maker::Manager,
    Maker::Config,
    Maker::Pipeline,
    Maker::DisplayView,
];

/// U-184: `Config::getProcessor` holds the config's cache of processors locked while it makes
/// a processor, and every change of the config that flushes the cache locks it again, from a
/// `noexcept` function: a LUT loader that changes the config (`setRole`, `addColorSpace`,
/// `clearProcessorCache`, `setProcessorCacheFlags`) ends the wheel's process, for each of the
/// four ways to make a processor of a config, as the Windows wheel's crash report from py_eval
/// shows (the Linux wheel's thread waits for itself: probed in Rocky Linux 9). The port's
/// manager makes its processors from a copy of the config, and the change raises U-184's error
/// in the loader; the config's, the pipeline's and `DisplayViewHelpers`' calls hold the
/// config, and the change raises D-9's error. The processor is made in every case.
#[test]
fn a_lut_loader_that_changes_the_config_of_the_processor_is_u184() {
    start();
    let mut cases = Vec::new();
    for maker in MAKERS {
        for change in changes().iter().filter(|c| c.kills) {
            let tag = format!("{maker:?}_{}", change.name);
            let (sc, made, _, raised) = change_script(maker, change, false, &tag);
            cases.push((maker, change.name, sc, made, raised.unwrap()));
        }
    }
    assert_eq!(cases.len(), 16);
    if cfg!(windows) {
        let json: Vec<Value> = cases.iter().map(|c| c.2.json()).collect();
        let result = ocio_testkit::Oracle::get()
            .call("py_eval", serde_json::json!({ "scripts": json }), &[])
            .result;
        let reports = result["scripts"].as_array().unwrap();
        assert_eq!(reports.len(), cases.len());
        for (report, (maker, change, ..)) in reports.iter().zip(&cases) {
            let code = report["crashed"]["returncode"].as_i64();
            assert!(code.is_some_and(|c| c != 0), "{maker:?} {change}: {report}");
        }
    }
    for (maker, change, sc, made, raised) in cases {
        let ours = guarded_port(sc.json());
        let steps = ours["steps"].as_array().unwrap();
        assert_eq!(steps.len(), sc.steps.len(), "{maker:?} {change}");
        for (k, step) in steps.iter().enumerate() {
            assert!(
                step.get("value").is_some(),
                "{maker:?} {change}, step {k}: {step}"
            );
        }
        assert!(steps[made].get("value").is_some());
        let value = &steps[raised]["value"];
        assert_eq!(value["type"], "builtins.RuntimeError", "{maker:?} {change}");
        let text = String::from_utf8(ocio_testkit::oracle_values::bytes(&value["repr"])).unwrap();
        let marker = match maker {
            Maker::Manager => "(docs/improvements.md, U-184)",
            _ => "(docs/deviations.md, D-9)",
        };
        assert!(text.contains(marker), "{maker:?} {change}: {text}");
    }
}

/// What the manager's loader leaves alone, the port leaves alone too: changes that don't lock
/// the cache of processors (`setName`, `setDescription`, `upgradeToLatestVersion` of a config
/// at the latest version), and every change when the config's cache is off (upstream's call
/// doesn't lock it then): the wheel makes the processor, and so does the port, each step equal.
#[test]
fn a_lut_loader_that_changes_the_config_without_the_cache_lock_is_as_the_wheels() {
    start();
    let mut scripts = Vec::new();
    for change in changes() {
        for cache_off in [false, true] {
            if change.kills && !cache_off {
                continue;
            }
            let tag = format!("quiet_{}_{cache_off}", change.name);
            let (sc, _, _, raised) = change_script(Maker::Manager, &change, cache_off, &tag);
            assert!(raised.is_none());
            scripts.push(sc);
        }
    }
    assert_eq!(scripts.len(), 10);
    // The loaders ran in the wheel: the config has the name or the description they gave it.
    let json: Vec<Value> = scripts.iter().map(Script::json).collect();
    let theirs = common::wheel(&json);
    let hex = |text: &str| text.bytes().map(|c| format!("{c:02x}")).collect::<String>();
    for (k, text) in [
        (4, "renamed"),
        (5, "renamed"),
        (6, "described"),
        (7, "described"),
    ] {
        let report = theirs[k].to_string();
        assert!(report.contains(&hex(text)), "script {k}: {report}");
    }
    guarded(move || run(&scripts));
}
