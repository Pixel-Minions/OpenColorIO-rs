// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Context` and `ProcessorMetadata` with their iterators (chunk 6.3d) against the wheel:
//! - each class's surface, its nested iterator classes' included;
//! - through the same step scripts on both sides: `Context`'s constructors (every parameter,
//!   positional and keyword, pybind11's map caster: a dict, a dict subclass, keys and values as
//!   bytes, a key twice, an object with `items`, one whose `items()` isn't iterable, refusals),
//!   its variables (`__setitem__` with `None`, `__getitem__` ignoring case and its `KeyError`,
//!   `__contains__`, the iterators, negative indices, `StopIteration`), search paths, working
//!   directory, environment mode, cache IDs, `repr`, `__deepcopy__` (I-111), `loadEnvironment`
//!   of a variable no environment has, `resolveStringVar` and `resolveFileLocation` with and
//!   without a context collecting the variables used (another, `None`, itself when no variable
//!   is used); `ProcessorMetadata`'s files (a sorted set) and looks, its iterators.
//!
//! And: an environment mode outside the enumerators, which the wheel stores, is the port's error
//! (U-112), the wheel running the same steps without one; without the wheel, whose process dies
//! there, a context collecting the variables it uses into itself (U-122) and a null file name
//! or look (U-123) are the port's errors.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, by, compare, d, i, l, n, none, r, ra, s, t};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::json;

/// Each class has the wheel's surface, member for member, its iterator classes' included.
#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in ["Context", "ProcessorMetadata"] {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 6),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// The getters a context's state shows through.
const STATE: [&str; 6] = [
    "getCacheID",
    "getSearchPath",
    "getWorkingDir",
    "getEnvironmentMode",
    "__len__",
    "__repr__",
];

/// `Context`'s constructors and the map caster.
fn constructors_script() -> Script {
    let mut sc = Script::new();
    let c0 = sc.call(n("OCIO.Context"), vec![]);
    sc.eval(r(c0), &STATE);
    let c1 = sc.call(
        n("OCIO.Context"),
        vec![
            s("/work/dir"),
            l(vec![s("a"), s(""), s("b:c"), by(b"d\0e"), s("$X/f")]),
            d(vec![
                (s("X"), s("x")),
                (s("LONGER_NAME"), s("y")),
                (by(b"Z"), by(b"z\0tail")),
                (s(""), s("empty")),
            ]),
            n("OCIO.ENV_ENVIRONMENT_LOAD_ALL"),
        ],
    );
    sc.eval(r(c1), &STATE);
    let vars1 = sc.call(ra(c1, "getStringVars"), vec![]);
    sc.call(n("list"), vec![r(vars1)]);
    let c2 = sc.call_kw(
        n("OCIO.Context"),
        vec![],
        vec![
            (
                "stringVars",
                d(vec![(s("a"), by(b"z")), (by(b"a"), s("w"))]),
            ),
            ("environmentMode", n("OCIO.ENV_ENVIRONMENT_UNKNOWN")),
        ],
        Expect::Value,
    );
    sc.eval(r(c2), &STATE);
    let c3 = sc.call_kw(
        n("OCIO.Context"),
        vec![],
        vec![("searchPaths", t(vec![s("p1"), s("p2")]))],
        Expect::Value,
    );
    sc.eval(r(c3), &STATE);
    // A dict subclass is a dict; an object with `items` and `__getitem__` converts.
    let sub = sc.call(
        n("type"),
        vec![
            s("D"),
            t(vec![n("dict")]),
            d(vec![(s("__module__"), s("probe"))]),
        ],
    );
    let dsub = sc.call(r(sub), vec![d(vec![(s("K"), s("v"))])]);
    let c4 = sc.call(n("OCIO.Context"), vec![s(""), l(vec![]), r(dsub)]);
    sc.eval(r(c4), &["__repr__"]);
    let dq = sc.call(n("dict"), vec![d(vec![(s("Q"), s("q")), (s("R"), s("r"))])]);
    let items = sc.eval(ra(dq, "items"), &[]);
    let mapping = sc.call(
        n("type"),
        vec![
            s("M"),
            t(vec![n("object")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("items"), r(items)),
                (s("__getitem__"), n("operator.getitem")),
            ]),
        ],
    );
    let m = sc.call(r(mapping), vec![]);
    let c5 = sc.call_kw(
        n("OCIO.Context"),
        vec![],
        vec![("stringVars", r(m))],
        Expect::Value,
    );
    sc.eval(r(c5), &["__repr__"]);
    let bad = sc.call(
        n("type"),
        vec![
            s("Bad"),
            t(vec![n("object")]),
            d(vec![
                (s("__module__"), s("probe")),
                (s("items"), n("int")),
                (s("__getitem__"), n("operator.getitem")),
            ]),
        ],
    );
    let b = sc.call(r(bad), vec![]);
    sc.call_kw(
        n("OCIO.Context"),
        vec![],
        vec![("stringVars", r(b))],
        Expect::Raised,
    );
    let no_get = sc.call(
        n("type"),
        vec![
            s("N"),
            t(vec![n("object")]),
            d(vec![(s("__module__"), s("probe")), (s("items"), r(items))]),
        ],
    );
    let ng = sc.call(r(no_get), vec![]);
    sc.call_kw(
        n("OCIO.Context"),
        vec![],
        vec![("stringVars", r(ng))],
        Expect::Raised,
    );
    // Refusals.
    sc.raises(n("OCIO.Context"), vec![i(1)]);
    sc.raises(n("OCIO.Context"), vec![none()]);
    sc.raises(n("OCIO.Context"), vec![s(""), s("ab")]);
    sc.raises(n("OCIO.Context"), vec![s(""), l(vec![i(1)])]);
    sc.raises(
        n("OCIO.Context"),
        vec![s(""), l(vec![]), d(vec![(s("a"), none())])],
    );
    sc.raises(
        n("OCIO.Context"),
        vec![s(""), l(vec![]), d(vec![(i(1), s("a"))])],
    );
    sc.raises(
        n("OCIO.Context"),
        vec![s(""), l(vec![]), l(vec![t(vec![s("a"), s("b")])])],
    );
    sc.raises(n("OCIO.Context"), vec![s(""), l(vec![]), d(vec![]), i(1)]);
    sc.call_kw(
        n("OCIO.Context"),
        vec![],
        vec![("nope", s(""))],
        Expect::Raised,
    );
    sc
}

/// Variables, search paths, the iterators, the environment mode, copies.
fn methods_script() -> Script {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Context"), vec![]);
    sc.call(ra(c, "__setitem__"), vec![s("TeSt"), s("foobar")]);
    sc.call(ra(c, "setStringVar"), vec![by(b"Bar"), s("Foo")]);
    sc.call(ra(c, "__setitem__"), vec![s("NUL\0x"), s("v\0w")]);
    sc.call(ra(c, "__setitem__"), vec![none(), s("ignored")]);
    sc.call(ra(c, "__setitem__"), vec![s(""), s("ignored")]);
    sc.call(ra(c, "__setitem__"), vec![s("GONE"), s("1")]);
    sc.call(ra(c, "__setitem__"), vec![s("GONE"), none()]);
    sc.call(ra(c, "setStringVar"), vec![s("bar"), s("lower")]);
    sc.raises(ra(c, "__setitem__"), vec![i(1), s("x")]);
    sc.eval(r(c), &STATE);
    for name in [
        "TeSt", "test", "TEST", "NUL", "bar", "Bar", "GONE", "nope", "", "NUL\0x",
    ] {
        let expect = if ["GONE", "nope", "", "NUL\0x"].contains(&name) {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(
            json!({ "call": ra(c, "__getitem__"), "args": [s(name)] }),
            expect,
        );
        sc.call(ra(c, "__contains__"), vec![s(name)]);
    }
    sc.raises(ra(c, "__getitem__"), vec![by(b"\xff")]);
    sc.raises(ra(c, "__getitem__"), vec![none()]);
    sc.raises(ra(c, "__contains__"), vec![none()]);
    // The iterators, which see later changes.
    let names = sc.call(n("iter"), vec![r(c)]);
    sc.call(n("type"), vec![r(names)]);
    sc.call(n("len"), vec![r(names)]);
    for k in [0, 3, -1, -9] {
        sc.call(ra(names, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(names, "__getitem__"), vec![i(4)]);
    sc.call(n("next"), vec![r(names)]);
    let it2 = sc.call(n("iter"), vec![r(names)]);
    sc.call(n("operator.is_"), vec![r(it2), r(names)]);
    let vars = sc.call(ra(c, "getStringVars"), vec![]);
    sc.call(n("type"), vec![r(vars)]);
    for k in [0, 3, -1] {
        sc.call(ra(vars, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(vars, "__getitem__"), vec![i(4)]);
    sc.call(ra(c, "__setitem__"), vec![s("LATE"), s("late")]);
    sc.call(n("len"), vec![r(vars)]);
    sc.call(n("list"), vec![r(vars)]);
    sc.raises(n("next"), vec![r(vars)]);
    sc.call(n("list"), vec![r(names)]);
    // A value that isn't UTF-8.
    sc.call(ra(c, "__setitem__"), vec![s("BIN"), by(b"\xfe")]);
    sc.raises(ra(c, "__getitem__"), vec![s("BIN")]);
    sc.raises(n("repr"), vec![r(c)]);
    sc.call(ra(c, "clearStringVars"), vec![]);
    sc.call(n("len"), vec![r(c)]);
    sc.eval(r(c), &STATE);
    // Search paths.
    let paths = sc.call(ra(c, "getSearchPaths"), vec![]);
    sc.call(n("type"), vec![r(paths)]);
    sc.call(n("len"), vec![r(paths)]);
    sc.call(ra(c, "setSearchPath"), vec![s("a:b::c")]);
    sc.call(n("len"), vec![r(paths)]);
    sc.call(n("list"), vec![r(paths)]);
    sc.call(ra(c, "addSearchPath"), vec![s("D:\\Second\\Path\\")]);
    sc.call(ra(c, "addSearchPath"), vec![s("")]);
    sc.call(ra(c, "addSearchPath"), vec![none()]);
    sc.call(ra(c, "addSearchPath"), vec![s("x\0y")]);
    sc.eval(r(c), &STATE);
    for k in [0, 4, -1] {
        sc.call(ra(paths, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(paths, "__getitem__"), vec![i(6)]);
    sc.call(ra(c, "setSearchPath"), vec![none()]);
    sc.eval(r(c), &STATE);
    sc.call(ra(c, "clearSearchPaths"), vec![]);
    sc.call(n("len"), vec![r(paths)]);
    sc.call(ra(c, "setWorkingDir"), vec![s("/dir/123")]);
    sc.eval(r(c), &STATE);
    sc.call(ra(c, "setWorkingDir"), vec![none()]);
    sc.raises(ra(c, "setWorkingDir"), vec![i(3)]);
    sc.eval(r(c), &STATE);
    // The environment mode.
    for mode in [
        "OCIO.ENV_ENVIRONMENT_LOAD_ALL",
        "OCIO.ENV_ENVIRONMENT_UNKNOWN",
        "OCIO.ENV_ENVIRONMENT_LOAD_PREDEFINED",
    ] {
        sc.call(ra(c, "setEnvironmentMode"), vec![n(mode)]);
        sc.eval(r(c), &STATE);
    }
    sc.raises(ra(c, "setEnvironmentMode"), vec![i(1)]);
    sc.raises(ra(c, "setEnvironmentMode"), vec![none()]);
    // loadEnvironment updates the variables the environment has: here none.
    sc.call(
        ra(c, "__setitem__"),
        vec![s("OCIO_RS_NO_SUCH_VARIABLE_1"), s("kept")],
    );
    sc.call(ra(c, "loadEnvironment"), vec![]);
    let vars2 = sc.call(ra(c, "getStringVars"), vec![]);
    sc.call(n("list"), vec![r(vars2)]);
    // Copies: everything but the environment mode (I-111).
    sc.call(
        ra(c, "setEnvironmentMode"),
        vec![n("OCIO.ENV_ENVIRONMENT_LOAD_ALL")],
    );
    sc.call(ra(c, "setSearchPath"), vec![s("testing123:testing456")]);
    let copy = sc.call(n("copy.deepcopy"), vec![r(c)]);
    sc.call(n("operator.is_"), vec![r(copy), r(c)]);
    sc.eval(r(copy), &STATE);
    sc.call(ra(copy, "__setitem__"), vec![s("ONLY"), s("copy")]);
    sc.call(n("len"), vec![r(c)]);
    let copy2 = sc.call(ra(c, "__deepcopy__"), vec![d(vec![])]);
    sc.call(n("type"), vec![r(copy2)]);
    sc.raises(ra(c, "__deepcopy__"), vec![l(vec![])]);
    sc.raises(n("OCIO.Context.__len__"), vec![n("OCIO.ProcessorMetadata")]);
    sc
}

/// `resolveStringVar` and `resolveFileLocation`.
fn resolve_script() -> Script {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Context"), vec![]);
    sc.call(ra(c, "__setitem__"), vec![s("TEST1"), s("foobar")]);
    sc.call(ra(c, "__setitem__"), vec![s("A"), s("a")]);
    sc.call(ra(c, "__setitem__"), vec![s("AB"), s("ab")]);
    for text in [
        "/foo/${TEST1}/bar",
        "/foo/$TEST1/bar",
        "%TEST1%",
        "$AB$A",
        "${NOPE}",
        "",
        "plain",
        "$TEST1\0$A",
    ] {
        sc.call(ra(c, "resolveStringVar"), vec![s(text)]);
    }
    sc.call(ra(c, "resolveStringVar"), vec![none()]);
    sc.raises(ra(c, "resolveStringVar"), vec![by(b"\xff$A")]);
    sc.raises(ra(c, "resolveStringVar"), vec![i(1)]);
    // Collecting the variables used: another context, None, the context itself (none used).
    let used = sc.call(n("OCIO.Context"), vec![]);
    sc.call(ra(c, "resolveStringVar"), vec![s("$AB/${TEST1}"), r(used)]);
    sc.eval(r(used), &STATE);
    sc.call(ra(c, "resolveStringVar"), vec![s("$AB/${TEST1}"), r(used)]);
    sc.call(ra(c, "resolveStringVar"), vec![s("$A"), none()]);
    sc.call_kw(
        ra(c, "resolveStringVar"),
        vec![],
        vec![("string", s("$A")), ("usedContextVars", r(used))],
        Expect::Value,
    );
    sc.eval(r(used), &STATE);
    sc.call(ra(c, "resolveStringVar"), vec![s("plain"), r(c)]);
    sc.raises(ra(c, "resolveStringVar"), vec![s("$A"), i(1)]);
    sc.raises(ra(c, "resolveStringVar"), vec![s("$A"), r(c), r(c)]);
    // Files that don't exist, relative and absolute; the variables used.
    sc.call(ra(c, "setWorkingDir"), vec![s("/no/such/dir")]);
    sc.call(ra(c, "setSearchPath"), vec![s("sp1:${TEST1}/sp2")]);
    sc.raises(ra(c, "resolveFileLocation"), vec![s("test.lut")]);
    sc.raises(ra(c, "resolveFileLocation"), vec![s("$A/test.lut")]);
    sc.raises(ra(c, "resolveFileLocation"), vec![s("/no/such/file.lut")]);
    sc.raises(ra(c, "resolveFileLocation"), vec![s("")]);
    let used2 = sc.call(n("OCIO.Context"), vec![]);
    sc.raises(ra(c, "resolveFileLocation"), vec![s("$AB.lut"), r(used2)]);
    sc.eval(r(used2), &STATE);
    sc.raises(ra(c, "resolveFileLocation"), vec![s("x.lut"), none()]);
    sc.raises(ra(c, "resolveFileLocation"), vec![s("x.lut"), r(c)]);
    sc.raises(ra(c, "resolveFileLocation"), vec![i(1)]);
    sc
}

/// `ProcessorMetadata`.
fn processor_metadata_script() -> Script {
    let mut sc = Script::new();
    let m = sc.call(n("OCIO.ProcessorMetadata"), vec![]);
    sc.raises(n("OCIO.ProcessorMetadata"), vec![i(1)]);
    let files = sc.call(ra(m, "getFiles"), vec![]);
    let looks = sc.call(ra(m, "getLooks"), vec![]);
    sc.call(n("type"), vec![r(files)]);
    sc.call(n("type"), vec![r(looks)]);
    sc.call(n("len"), vec![r(files)]);
    for f in ["b.lut", "a.lut", "b.lut", "A.lut", "", "c\0d"] {
        sc.call(ra(m, "addFile"), vec![s(f)]);
    }
    sc.call(ra(m, "addFile"), vec![by(b"z.cube")]);
    sc.raises(ra(m, "addFile"), vec![i(1)]);
    for look in ["look2", "look1", "look2", "", "x\0y"] {
        sc.call(ra(m, "addLook"), vec![s(look)]);
    }
    sc.call_kw(
        ra(m, "addLook"),
        vec![],
        vec![("look", s("kw"))],
        Expect::Value,
    );
    sc.call(n("len"), vec![r(files)]);
    sc.call(n("list"), vec![r(files)]);
    sc.call(n("len"), vec![r(looks)]);
    sc.call(n("list"), vec![r(looks)]);
    for k in [0, 4, -1] {
        sc.call(ra(files, "__getitem__"), vec![i(k)]);
        sc.call(ra(looks, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(files, "__getitem__"), vec![i(6)]);
    sc.raises(ra(looks, "__getitem__"), vec![i(6)]);
    sc.raises(n("next"), vec![r(files)]);
    let it = sc.call(ra(m, "getLooks"), vec![]);
    sc.call(n("next"), vec![r(it)]);
    let it2 = sc.call(ra(it, "__iter__"), vec![]);
    sc.call(n("operator.is_"), vec![r(it2), r(it)]);
    sc.call(ra(m, "addFile"), vec![s("bin\u{e9}")]);
    sc.call(ra(m, "addLook"), vec![by(b"\xff")]);
    sc.raises(n("list"), vec![r(it)]);
    sc.raises(n("copy.deepcopy"), vec![r(m)]);
    // The default repr, whose address the step interpreter masks (P6-7).
    sc.eval(r(m), &[]);
    sc
}

#[test]
fn context_and_processor_metadata_behave_as_the_wheels() {
    compare(&[
        constructors_script(),
        methods_script(),
        resolve_script(),
        processor_metadata_script(),
    ]);
}

/// An environment mode outside the enumerators, which the wheel's context stores, is the
/// port's error (U-112): `setEnvironmentMode` and the constructor. The wheel runs the same
/// steps without an error.
#[test]
fn an_environment_mode_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let c = sc.call(n("OCIO.Context"), vec![]);
    // -1 in the Windows wheel, whose enum is an `int`; the same bits as the Linux wheel's
    // `unsigned int`, which refuses a negative number (I-225).
    let minus_one: i128 = if cfg!(windows) { -1 } else { (1 << 32) - 1 };
    for v in [3, 7, minus_one, 2147483647] {
        let mode = sc.call(n("OCIO.EnvironmentMode"), vec![i(v)]);
        refused.push(sc.call(ra(c, "setEnvironmentMode"), vec![r(mode)]));
        refused.push(sc.call_kw(
            n("OCIO.Context"),
            vec![],
            vec![("environmentMode", r(mode))],
            Expect::Value,
        ));
    }
    assert_eq!(refused.len(), 8);
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
            assert_eq!(step["raised"]["type"], "builtins.ValueError", "step {k}");
        } else {
            assert_eq!(step, wheel_step, "step {k}");
        }
    }
}

/// Runs `code` against the port's module, in globals of its own.
fn run(py: Python<'_>, globals: &Bound<'_, PyDict>, code: &str) -> PyResult<()> {
    let code = std::ffi::CString::new(code).unwrap();
    py.run(&code, Some(globals), None)
}

/// Where the wheel's process dies, the port's error: a context collecting the variables it uses
/// into itself (U-122), a null file name or look (U-123).
#[test]
fn where_the_wheel_dies_the_port_raises() {
    common::start_python();
    Python::attach(|py| {
        let cases: &[(&str, &str, &str)] = &[
            ("c.resolveStringVar('$A', c)", "U-122", "RuntimeError"),
            ("c.resolveStringVar('${A}x', c)", "U-122", "RuntimeError"),
            (
                "c.resolveFileLocation('$A.lut', c)",
                "U-122",
                "RuntimeError",
            ),
            ("c.resolveFileLocation(None)", "U-123", "ValueError"),
            (
                "c.resolveFileLocation(None, OCIO.Context())",
                "U-123",
                "ValueError",
            ),
            ("c.resolveFileLocation(None, None)", "U-123", "ValueError"),
            (
                "OCIO.ProcessorMetadata().addFile(None)",
                "U-123",
                "ValueError",
            ),
            (
                "OCIO.ProcessorMetadata().addLook(None)",
                "U-123",
                "ValueError",
            ),
        ];
        for (code, entry, kind) in cases {
            let globals = PyDict::new(py);
            run(
                py,
                &globals,
                "import PyOpenColorIO as OCIO\nc = OCIO.Context()\nc['A'] = 'a'\n\
                 c.setWorkingDir('/no/such/dir')",
            )
            .unwrap();
            let err = run(py, &globals, code).expect_err(code);
            let text = err.to_string();
            assert!(
                text.starts_with(kind) && text.contains(&format!("docs/improvements.md, {entry}")),
                "{code}: {text}"
            );
        }
        // The controls: the same calls short of what kills the wheel.
        let globals = PyDict::new(py);
        run(
            py,
            &globals,
            "import PyOpenColorIO as OCIO\nc = OCIO.Context()\nc['A'] = 'a'\n\
             assert c.resolveStringVar('$A', OCIO.Context()) == 'a'\n\
             assert c.resolveStringVar('plain', c) == 'plain'\n\
             m = OCIO.ProcessorMetadata()\nm.addFile('')\nm.addLook('')\n\
             assert len(m.getFiles()) == 1 and len(m.getLooks()) == 1",
        )
        .unwrap_or_else(|e| panic!("{e}"));
    });
}
