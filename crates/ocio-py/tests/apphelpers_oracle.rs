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

use std::sync::Once;

use common::script::{Expect, Script, b, by, compare, d, f, i, l, n, none, r, ra, s, t};
use pyo3::prelude::*;
use pyo3::types::PyCFunction;
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

static STAND_IN: Once = Once::new();

/// Starts the interpreter, with `Processor.getCacheID` a stand-in while the module lacks it.
fn start() {
    common::start_python();
    STAND_IN.call_once(|| {
        Python::attach(|py| {
            let ocio = py.import("PyOpenColorIO").unwrap();
            let processor = ocio.getattr("Processor").unwrap();
            if !processor.hasattr("getCacheID").unwrap() {
                let f = PyCFunction::new_closure(py, Some(c"getCacheID"), None, |args, _| {
                    let p = ocio_py::embed::processor_from_python(&args.get_item(0)?)?;
                    p.cache_id()
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
                })
                .unwrap();
                let method = py
                    .eval(c"lambda f: lambda self: f(self)", None, None)
                    .unwrap()
                    .call1((f,))
                    .unwrap();
                processor.setattr("getCacheID", method).unwrap();
            }
        });
    });
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

def menu_logs():
    cfg = OCIO.Config.CreateRaw()
    count[0] += 1
    # A role naming no color space: the config doesn't validate, which the menu logs.
    cfg.setRole('scene_linear', 'missing_%d_%d' % (RUN, count[0]))
    params = OCIO.ColorSpaceMenuParameters(cfg)
    params.setAppCategories('categories from the test')
    seen = []
    OCIO.SetLoggingFunction(lambda message: seen.append(outcome(params.getAppCategories)))
    try:
        done = outcome(lambda: type(OCIO.ColorSpaceMenuHelper(params)).__name__)
    finally:
        OCIO.ResetToDefaultLoggingFunction()
    return done, seen
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
#[test]
fn a_callback_reads_the_pipeline_that_makes_a_processor() {
    guarded(|| {
        start();
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
/// (MixingHelpers.cpp:150-154 @ v2.5.2).
#[test]
fn a_callback_reads_the_manager_that_makes_a_processor() {
    guarded(|| {
        start();
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

/// A menu of a config that doesn't validate logs a warning (ColorSpaceHelpers.cpp:385-395 @ v2.5.2),
/// and the logging function reads the menu's parameters: the port's parameters aren't locked
/// while the library makes the menu.
#[test]
fn a_logging_function_reads_the_parameters_of_a_menu_being_made() {
    guarded(|| {
        start();
        Python::attach(|py| {
            let globals = helper_globals(py);
            let got = eval_in(&globals, c"menu_logs()");
            let (done, seen) = (got.get_item(0).unwrap(), got.get_item(1).unwrap());
            assert_py_eq(&globals, &done, "('ok', 'ColorSpaceMenuHelper')");
            // Other tests of this process may log meanwhile: every message reads the same.
            assert!(seen.len().unwrap() > 0);
            for read in seen.try_iter().unwrap() {
                assert_py_eq(
                    &globals,
                    &read.unwrap(),
                    "('ok', 'categories from the test')",
                );
            }
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

/// A menu made before anything else computed its config's cache ID; then a color space the
/// config hands out is changed in place (its description), which doesn't reset the config's
/// cache ID: the menu's and the parameters' text keep it.
#[test]
fn a_menu_made_first_keeps_its_configs_cache_id() {
    let mut sc = Script::new();
    let cfg = config(&mut sc, "menu first");
    let p = sc.call(n("OCIO.ColorSpaceMenuParameters"), vec![r(cfg)]);
    let h = sc.call(n("OCIO.ColorSpaceMenuHelper"), vec![r(p)]);
    let cs = sc.call(ra(cfg, "getColorSpace"), vec![s("lin_1")]);
    sc.call(ra(cs, "setDescription"), vec![s("changed in place")]);
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
/// sets the pipeline's looks override (to the path it loads): the wheel's call works on the
/// pipeline the loader changes, and so does the port's (its lock isn't held while the library
/// builds the processor). The looks override isn't enabled, so the processor is the same.
#[test]
fn a_lut_loader_changes_the_pipeline_that_makes_a_processor() {
    let mut sc = Script::new();
    let px = proxy_of(&mut sc, &PROXY_CONFIG.replace("SRC", "lut_pipeline.spi1d"));
    let cfg = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(px)]);
    let vp = sc.call(n("OCIO.LegacyViewingPipeline"), vec![]);
    load_calls(&mut sc, px, ra(vp, "setLooksOverride"));
    let dvt = sc.call_kw(
        n("OCIO.DisplayViewTransform"),
        vec![],
        vec![("src", s("raw")), ("display", s("d")), ("view", s("v"))],
        Expect::Value,
    );
    sc.call(ra(vp, "setDisplayViewTransform"), vec![r(dvt)]);
    let ft = sc.call(n("OCIO.FileTransform"), vec![s("lut_pipeline.spi1d")]);
    sc.call(ra(vp, "setLinearCC"), vec![r(ft)]);
    sc.eval(r(vp), &PIPELINE_GETTERS);
    let p = sc.call(ra(vp, "getProcessor"), vec![r(cfg)]);
    sc.unrepr(p);
    sc.call(ra(p, "getCacheID"), vec![]);
    sc.eval(r(vp), &PIPELINE_GETTERS);
    guarded(move || run(&[sc]));
}

/// As [`a_lut_loader_changes_the_pipeline_that_makes_a_processor`], for a mixing manager
/// whose color picking role's color space reads the LUT, and a loader that selects a mixing
/// space by the path it loads (which isn't one: the loader raises, and so does the call).
#[test]
fn a_lut_loader_uses_the_manager_that_makes_a_processor() {
    let mut sc = Script::new();
    let config = PROXY_CONFIG
        .replace("SRC", "lut_mixing.spi1d")
        .replace("scene_linear: raw", "scene_linear: raw, color_picking: lut");
    let px = proxy_of(&mut sc, &config);
    let cfg = sc.call(n("OCIO.Config.CreateFromConfigIOProxy"), vec![r(px)]);
    let m = sc.call(n("OCIO.MixingColorSpaceManager"), vec![r(cfg)]);
    load_calls(&mut sc, px, ra(m, "setSelectedMixingSpace"));
    sc.eval(r(m), &MIXING_GETTERS);
    sc.raises(ra(m, "getProcessor"), vec![s("raw"), s("d"), s("v")]);
    sc.eval(r(m), &MIXING_GETTERS);
    guarded(move || run(&[sc]));
}
