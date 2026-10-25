// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Baker` (chunk 6.3i, PyBaker.cpp @ v2.5.2) against the wheel:
//! - the class's surface, its iterator class's included;
//! - through the same step scripts on both sides: the getters and setters (`None`, `bytes`, a
//!   name cut at a NUL, keywords, refused arguments), `setDisplayView`'s error, `setFormat`'s;
//!   the `FormatIterator` (length, indices past the end and negative, `next` to the end);
//!   `bake()`'s settings errors and each format's errors; the text each of the 12 formats
//!   bakes, from a target space and from a display and view, with metadata and a shaper
//!   space; the constructor's overloads; the config `setConfig` copies and `__deepcopy__`
//!   shares, a change through it seen by the bakers; the format metadata `getFormatMetadata`
//!   references, a change through it baked;
//! - `bake(fileName)`: the file's bytes, read back with `numpy.fromfile` (a text-mode stream: CR
//!   LF line ends on Windows); a bake that fails leaves the file created and empty; a file that
//!   can't be opened makes no error;
//! - and where the wheel's process dies (U-201): `setFormat(None)`, `setConfig(None)` and the
//!   constructor given `None` for its config are the port's error.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, by, compare, d, i, l, n, none, r, ra, s};
use ocio_testkit::oracle_values::bytes;
use serde_json::Value;

/// The config of upstream's `BakerTest.py` (tests/python/BakerTest.py:11-32 @ v2.5.2), with a
/// look, a matrix color space (channel crosstalk), a second display and an inactive view.
const PROFILE: &str = "ocio_profile_version: 1

strictparsing: false

displays:
  TestDisplay:
    - !<View> {name: TestView, colorspace: test}
    - !<View> {name: Other, colorspace: lnh}
  Second:
    - !<View> {name: Mtx, colorspace: mtx}

active_views: [TestView, Mtx]

looks:
  - !<Look>
    name: warm
    process_space: lnh
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}

colorspaces:

  - !<ColorSpace>
    name: lnh
    bitdepth: 16f
    isdata: false
    allocation: lg2

  - !<ColorSpace>
    name: test
    bitdepth: 8ui
    isdata: false
    allocation: uniform
    to_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}

  - !<ColorSpace>
    name: mtx
    bitdepth: 32f
    isdata: false
    allocation: uniform
    to_reference: !<MatrixTransform> {matrix: [0.9, 0.1, 0, 0, 0, 1, 0, 0, 0, 0.2, 0.8, 0, 0, 0, 0, 1]}
";

/// The 12 formats that bake, in the wheel's order (probed: `list(OCIO.Baker.getFormats())`).
const FORMATS: [&str; 12] = [
    "flame",
    "lustre",
    "Academy/ASC Common LUT Format",
    "Color Transform Format",
    "cinespace",
    "houdini",
    "iridas_cube",
    "iridas_itx",
    "resolve_cube",
    "spi1d",
    "spi3d",
    "truelight",
];

/// Each class has the wheel's surface, member for member, its iterator class's included.
#[test]
fn the_class_has_the_wheels_surface() {
    let report = common::wheel_surface();
    let theirs = common::wheel_member(&report, "Baker");
    let ours = common::port_member("Baker");
    let members = theirs["members"].as_array().unwrap();
    assert!(members.len() > 20, "{theirs}");
    assert!(
        members
            .iter()
            .any(|m| bytes(&m["name"]) == b"FormatIterator"),
        "{theirs}"
    );
    assert_eq!(ours, theirs);
}

fn config(sc: &mut Script) -> usize {
    sc.call(n("OCIO.Config.CreateFromStream"), vec![s(PROFILE)])
}

const GETTERS: [&str; 10] = [
    "getFormat",
    "getInputSpace",
    "getShaperSpace",
    "getLooks",
    "getDisplay",
    "getView",
    "getTargetSpace",
    "getShaperSize",
    "getCubeSize",
    "getConfig",
];

fn getters(sc: &mut Script, b: usize) {
    for g in GETTERS {
        sc.call(ra(b, g), vec![]);
    }
}

/// The getters and setters, the iterator class.
fn settings_script() -> Script {
    let mut sc = Script::new();
    let b = sc.call(n("OCIO.Baker"), vec![]);
    getters(&mut sc, b);
    // setFormat: as given (any case), bytes, cut at a NUL, by keyword; refused names.
    for name in [s("CineSpace"), by(b"spi1d"), s("spi3d\u{0}tail")] {
        sc.call(ra(b, "setFormat"), vec![name]);
        sc.call(ra(b, "getFormat"), vec![]);
    }
    sc.raises(ra(b, "setFormat"), vec![s("nope")]);
    sc.raises(ra(b, "setFormat"), vec![s("ColorDecisionList")]);
    sc.raises(ra(b, "setFormat"), vec![s("")]);
    sc.call(ra(b, "getFormat"), vec![]);
    sc.raises(ra(b, "setFormat"), vec![i(1)]);
    sc.raises(ra(b, "setFormat"), vec![]);
    sc.call_kw(
        ra(b, "setFormat"),
        vec![],
        vec![("formatName", s("flame"))],
        Expect::Value,
    );
    sc.call(ra(b, "getFormat"), vec![]);
    // The text setters: None is the empty string.
    for (set, get, param) in [
        ("setInputSpace", "getInputSpace", "inputSpace"),
        ("setShaperSpace", "getShaperSpace", "shaperSpace"),
        ("setLooks", "getLooks", "looks"),
        ("setTargetSpace", "getTargetSpace", "targetSpace"),
    ] {
        sc.call(ra(b, set), vec![s("a b\u{0}c")]);
        sc.call(ra(b, get), vec![]);
        sc.call(ra(b, set), vec![none()]);
        sc.call(ra(b, get), vec![]);
        sc.call_kw(ra(b, set), vec![], vec![(param, by(b"x"))], Expect::Value);
        sc.call(ra(b, get), vec![]);
        sc.raises(ra(b, set), vec![i(2)]);
        sc.raises(ra(b, set), vec![s("x"), s("y")]);
    }
    // setDisplayView: both or the library's error.
    sc.call(ra(b, "setDisplayView"), vec![s("d"), s("v")]);
    sc.call(ra(b, "getDisplay"), vec![]);
    sc.call(ra(b, "getView"), vec![]);
    sc.raises(ra(b, "setDisplayView"), vec![none(), s("v2")]);
    sc.raises(ra(b, "setDisplayView"), vec![s("d2"), none()]);
    sc.raises(ra(b, "setDisplayView"), vec![none(), none()]);
    sc.raises(ra(b, "setDisplayView"), vec![s("d2")]);
    sc.call(ra(b, "getDisplay"), vec![]);
    sc.call_kw(
        ra(b, "setDisplayView"),
        vec![],
        vec![("view", s("v3")), ("display", s("d3\u{0}x"))],
        Expect::Value,
    );
    sc.call(ra(b, "getDisplay"), vec![]);
    sc.call(ra(b, "getView"), vec![]);
    // The sizes.
    for (set, get, param) in [
        ("setShaperSize", "getShaperSize", "shaperSize"),
        ("setCubeSize", "getCubeSize", "cubeSize"),
    ] {
        sc.call(ra(b, set), vec![i(4)]);
        sc.call(ra(b, get), vec![]);
        sc.call(ra(b, set), vec![i(-7)]);
        sc.call(ra(b, get), vec![]);
        sc.call(ra(b, set), vec![common::script::b(true)]);
        sc.call(ra(b, get), vec![]);
        sc.call_kw(ra(b, set), vec![], vec![(param, i(2))], Expect::Value);
        sc.call(ra(b, get), vec![]);
        sc.raises(ra(b, set), vec![i(1 << 31)]);
        sc.raises(ra(b, set), vec![i(-(1 << 31) - 1)]);
        sc.raises(ra(b, set), vec![common::script::f(2.0)]);
        sc.raises(ra(b, set), vec![none()]);
        sc.call(ra(b, set), vec![i(-(1 << 31))]);
        sc.call(ra(b, get), vec![]);
    }
    getters(&mut sc, b);
    // The iterator, from the class and from an instance.
    let it = sc.call(n("OCIO.Baker.getFormats"), vec![]);
    sc.call(n("len"), vec![r(it)]);
    for k in [0, 1, 4, 11] {
        sc.call(ra(it, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(it, "__getitem__"), vec![i(12)]);
    // A negative index passes the check (PyUtils.h) and names no format.
    sc.call(ra(it, "__getitem__"), vec![i(-1)]);
    sc.call(ra(it, "__getitem__"), vec![i(-1 << 31)]);
    sc.raises(ra(it, "__getitem__"), vec![s("0")]);
    let again = sc.call(n("iter"), vec![r(it)]);
    sc.call(n("operator.is_"), vec![r(again), r(it)]);
    sc.call(n("list"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    let it2 = sc.call(ra(b, "getFormats"), vec![]);
    for _ in 0..3 {
        sc.call(n("next"), vec![r(it2)]);
    }
    sc.call(n("list"), vec![r(it2)]);
    sc.call(n("type"), vec![r(it2)]);
    sc
}

/// `bake()`'s settings errors, and each format's errors.
fn errors_script() -> Script {
    let mut sc = Script::new();
    let cfg = config(&mut sc);
    let b = sc.call(n("OCIO.Baker"), vec![]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setFormat"), vec![s("cinespace")]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setConfig"), vec![r(cfg)]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setInputSpace"), vec![s("lnh")]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setTargetSpace"), vec![s("test")]);
    sc.call(
        ra(b, "setDisplayView"),
        vec![s("TestDisplay"), s("TestView")],
    );
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setTargetSpace"), vec![s("")]);
    sc.call(ra(b, "setDisplayView"), vec![s("nope"), s("TestView")]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setDisplayView"), vec![s("TestDisplay"), s("nope")]);
    sc.raises(ra(b, "bake"), vec![]);
    // A view of another display, and an inactive one.
    sc.call(ra(b, "setDisplayView"), vec![s("TestDisplay"), s("Mtx")]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setCubeSize"), vec![i(2)]);
    sc.call(ra(b, "setShaperSize"), vec![i(3)]);
    sc.call(ra(b, "setDisplayView"), vec![s("TestDisplay"), s("Other")]);
    sc.call(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setDisplayView"), vec![s(""), s("")]);
    sc.call(ra(b, "setTargetSpace"), vec![s("nope")]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setTargetSpace"), vec![s("test")]);
    sc.call(ra(b, "setInputSpace"), vec![s("nope")]);
    sc.raises(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setInputSpace"), vec![s("lnh")]);
    sc.call(ra(b, "bake"), vec![]);
    // Each format: a look or a shaper space that isn't there; a 1D format and crosstalk.
    for format in FORMATS {
        sc.call(ra(b, "setFormat"), vec![s(format)]);
        sc.call(ra(b, "setLooks"), vec![s("nope")]);
        sc.raises(ra(b, "bake"), vec![]);
        sc.call(ra(b, "setLooks"), vec![s("")]);
        sc.call(ra(b, "setShaperSpace"), vec![s("nope")]);
        sc.raises(ra(b, "bake"), vec![]);
        sc.call(ra(b, "setShaperSpace"), vec![s("")]);
        sc.call(ra(b, "setTargetSpace"), vec![s("mtx")]);
        // Only spi1d bakes a 1D LUT of nothing else (FORMAT_BAKE_CAPABILITY_1DLUT).
        let expect = if format == "spi1d" {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(
            serde_json::json!({ "call": ra(b, "bake"), "args": [] }),
            expect,
        );
        sc.call(ra(b, "setTargetSpace"), vec![s("test")]);
    }
    // A cube size the format refuses.
    sc.call(ra(b, "setFormat"), vec![s("spi3d")]);
    sc.call(ra(b, "setCubeSize"), vec![i(1)]);
    sc.step(
        serde_json::json!({ "call": ra(b, "bake"), "args": [] }),
        Expect::Raised,
    );
    sc
}

/// What each format bakes: from a target space, with metadata, a look and a shaper space; and
/// from a display and view.
fn bake_script() -> Script {
    let mut sc = Script::new();
    let cfg = config(&mut sc);
    let b = sc.call(n("OCIO.Baker"), vec![]);
    sc.call(ra(b, "setConfig"), vec![r(cfg)]);
    sc.call(ra(b, "setInputSpace"), vec![s("lnh")]);
    sc.call(ra(b, "setTargetSpace"), vec![s("test")]);
    sc.call(ra(b, "setShaperSize"), vec![i(4)]);
    sc.call(ra(b, "setCubeSize"), vec![i(2)]);
    // Upstream's test's bake (BakerTest.py:122-137 @ v2.5.2).
    sc.call(ra(b, "setFormat"), vec![s("cinespace")]);
    sc.call(ra(b, "bake"), vec![]);
    for format in FORMATS {
        sc.call(ra(b, "setFormat"), vec![s(format)]);
        sc.call(ra(b, "bake"), vec![]);
    }
    let md = sc.call(ra(b, "getFormatMetadata"), vec![]);
    sc.call(
        ra(md, "addChildElement"),
        vec![s("Description"), s("hello")],
    );
    sc.call(
        ra(md, "addChildElement"),
        vec![s("Description"), s("world")],
    );
    sc.call(
        ra(md, "addChildElement"),
        vec![s("InputDescriptor"), s("in")],
    );
    sc.call(ra(md, "__setitem__"), vec![s("name"), s("baked")]);
    sc.repr(r(md));
    sc.call(ra(b, "setLooks"), vec![s("warm")]);
    for format in FORMATS {
        sc.call(ra(b, "setFormat"), vec![s(format)]);
        sc.call(ra(b, "bake"), vec![]);
    }
    sc.call(ra(b, "setShaperSpace"), vec![s("lnh")]);
    for format in FORMATS {
        sc.call(ra(b, "setFormat"), vec![s(format)]);
        let refuses = [
            "flame",
            "lustre",
            "iridas_cube",
            "iridas_itx",
            "spi3d",
            "truelight",
        ]
        .contains(&format);
        let expect = if refuses {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(
            serde_json::json!({ "call": ra(b, "bake"), "args": [] }),
            expect,
        );
    }
    // From a display and view.
    let dv = sc.call(n("OCIO.Baker"), vec![]);
    sc.call(ra(dv, "setConfig"), vec![r(cfg)]);
    sc.call(ra(dv, "setInputSpace"), vec![s("lnh")]);
    sc.call(
        ra(dv, "setDisplayView"),
        vec![s("TestDisplay"), s("TestView")],
    );
    sc.call(ra(dv, "setShaperSize"), vec![i(5)]);
    sc.call(ra(dv, "setCubeSize"), vec![i(3)]);
    for format in FORMATS {
        sc.call(ra(dv, "setFormat"), vec![s(format)]);
        sc.call(ra(dv, "bake"), vec![]);
    }
    // The formats' default sizes, for the 1D ones.
    let def = sc.call(n("OCIO.Baker"), vec![]);
    sc.call(ra(def, "setConfig"), vec![r(cfg)]);
    sc.call(ra(def, "setInputSpace"), vec![s("lnh")]);
    sc.call(ra(def, "setTargetSpace"), vec![s("test")]);
    for format in ["spi1d", "resolve_cube"] {
        sc.call(ra(def, "setFormat"), vec![s(format)]);
        sc.call(ra(def, "bake"), vec![]);
    }
    sc
}

/// The config `setConfig` copies, which `__deepcopy__` shares; the metadata `getFormatMetadata`
/// references, which `__deepcopy__` copies.
fn identity_script() -> Script {
    let mut sc = Script::new();
    let cfg = config(&mut sc);
    let b = sc.call(n("OCIO.Baker"), vec![]);
    sc.call(ra(b, "setConfig"), vec![r(cfg)]);
    let c1 = sc.call(ra(b, "getConfig"), vec![]);
    sc.call(n("operator.is_"), vec![r(c1), r(cfg)]);
    let c2 = sc.call(ra(b, "getConfig"), vec![]);
    sc.call(n("operator.is_"), vec![r(c1), r(c2)]);
    sc.call(n("operator.eq"), vec![r(c1), r(c2)]);
    sc.call(ra(b, "setInputSpace"), vec![s("lnh")]);
    sc.call(ra(b, "setTargetSpace"), vec![s("test")]);
    sc.call(ra(b, "setFormat"), vec![s("spi3d")]);
    sc.call(ra(b, "setCubeSize"), vec![i(2)]);
    let md = sc.call(ra(b, "getFormatMetadata"), vec![]);
    let md2 = sc.call(ra(b, "getFormatMetadata"), vec![]);
    sc.call(n("operator.is_"), vec![r(md), r(md2)]);
    sc.call(ra(md, "addChildElement"), vec![s("Description"), s("one")]);
    let copy = sc.call(n("copy.deepcopy"), vec![r(b)]);
    sc.call(n("operator.is_"), vec![r(copy), r(b)]);
    sc.call(n("type"), vec![r(copy)]);
    let cc = sc.call(ra(copy, "getConfig"), vec![]);
    sc.call(n("operator.is_"), vec![r(cc), r(c1)]);
    let cmd = sc.call(ra(copy, "getFormatMetadata"), vec![]);
    sc.call(n("operator.is_"), vec![r(cmd), r(md)]);
    sc.repr(r(cmd));
    sc.call(ra(md, "addChildElement"), vec![s("Description"), s("two")]);
    sc.repr(r(md));
    sc.repr(r(cmd));
    getters(&mut sc, copy);
    sc.call(ra(copy, "setFormat"), vec![s("resolve_cube")]);
    sc.call(ra(b, "getFormat"), vec![]);
    sc.call(ra(copy, "getFormat"), vec![]);
    sc.raises(ra(b, "__deepcopy__"), vec![l(vec![])]);
    sc.raises(ra(b, "__deepcopy__"), vec![]);
    let copy2 = sc.call(ra(b, "__deepcopy__"), vec![d(vec![])]);
    sc.call(ra(copy2, "getFormat"), vec![]);
    sc.call(ra(b, "bake"), vec![]);
    sc.call(ra(copy, "bake"), vec![]);
    // A change through the baker's config shows in both bakers; one through the config given
    // doesn't.
    let cs = sc.call(ra(c1, "getColorSpace"), vec![s("test")]);
    let e = sc.call(
        n("OCIO.ExponentTransform"),
        vec![common::script::fl(&[1.8, 1.8, 1.8, 1.0])],
    );
    sc.call(
        ra(cs, "setTransform"),
        vec![r(e), n("OCIO.COLORSPACE_DIR_TO_REFERENCE")],
    );
    sc.call(ra(b, "bake"), vec![]);
    sc.call(ra(copy, "bake"), vec![]);
    let given = sc.call(ra(cfg, "getColorSpace"), vec![s("test")]);
    let e2 = sc.call(
        n("OCIO.ExponentTransform"),
        vec![common::script::fl(&[3.0, 3.0, 3.0, 1.0])],
    );
    sc.call(
        ra(given, "setTransform"),
        vec![r(e2), n("OCIO.COLORSPACE_DIR_TO_REFERENCE")],
    );
    sc.call(ra(b, "bake"), vec![]);
    // A config changed after the baker's copy, set again; a color space added to the baker's.
    sc.call(ra(b, "setConfig"), vec![r(cfg)]);
    let c3 = sc.call(ra(b, "getConfig"), vec![]);
    sc.call(n("operator.is_"), vec![r(c3), r(c1)]);
    sc.call(ra(b, "bake"), vec![]);
    sc.call(ra(copy, "bake"), vec![]);
    let added = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("added")), ("toReference", r(e))],
        Expect::Value,
    );
    sc.call(ra(c3, "addColorSpace"), vec![r(added)]);
    sc.call(ra(b, "setTargetSpace"), vec![s("added")]);
    sc.call(ra(b, "bake"), vec![]);
    sc.call(ra(copy, "setTargetSpace"), vec![s("added")]);
    sc.raises(ra(copy, "bake"), vec![]);
    // The environment mode of the baker's config.
    sc.call(
        ra(c3, "setEnvironmentMode"),
        vec![n("OCIO.ENV_ENVIRONMENT_LOAD_ALL")],
    );
    sc.call(ra(c3, "getEnvironmentMode"), vec![]);
    sc.call(ra(b, "bake"), vec![]);
    sc.call(ra(b, "setConfig"), vec![r(c3)]);
    let c4 = sc.call(ra(b, "getConfig"), vec![]);
    sc.call(ra(c4, "getEnvironmentMode"), vec![]);
    sc
}

/// The constructor's overloads.
fn constructor_script() -> Script {
    let mut sc = Script::new();
    let cfg = config(&mut sc);
    let b = sc.call(
        n("OCIO.Baker"),
        vec![r(cfg), s("cinespace"), s("lnh"), s("test")],
    );
    getters(&mut sc, b);
    let all = sc.call(
        n("OCIO.Baker"),
        vec![
            r(cfg),
            s("spi1d"),
            s("lnh"),
            s("test"),
            s("warm"),
            i(3),
            s("lnh"),
            i(5),
        ],
    );
    getters(&mut sc, all);
    sc.call(ra(all, "bake"), vec![]);
    let kw = sc.call_kw(
        n("OCIO.Baker"),
        vec![],
        vec![
            ("shaperSize", i(4)),
            ("targetSpace", s("test")),
            ("cubeSize", i(2)),
            ("inputSpace", s("lnh")),
            ("format", s("Color Transform Format")),
            ("config", r(cfg)),
        ],
        Expect::Value,
    );
    getters(&mut sc, kw);
    sc.call(ra(kw, "bake"), vec![]);
    let cut = sc.call(
        n("OCIO.Baker"),
        vec![
            r(cfg),
            s("cinespace\u{0}x"),
            s("lnh\u{0}y"),
            by(b"test"),
            s("warm\u{0}z"),
        ],
    );
    getters(&mut sc, cut);
    // An empty look and shaper space are not set; a space that isn't there is kept.
    let empty = sc.call_kw(
        n("OCIO.Baker"),
        vec![r(cfg), s("spi3d"), s(""), s("nope")],
        vec![("looks", s("")), ("shaperSpace", s(""))],
        Expect::Value,
    );
    getters(&mut sc, empty);
    sc.raises(
        n("OCIO.Baker"),
        vec![r(cfg), s("nope"), s("lnh"), s("test")],
    );
    sc.raises(
        n("OCIO.Baker"),
        vec![r(cfg), s("cinespace"), none(), s("test")],
    );
    sc.raises(n("OCIO.Baker"), vec![r(cfg), s("cinespace"), s("lnh")]);
    sc.raises(n("OCIO.Baker"), vec![s("x")]);
    sc.raises(
        n("OCIO.Baker"),
        vec![r(cfg), s("cinespace"), s("lnh"), s("test"), s(""), s("2")],
    );
    sc.raises(
        n("OCIO.Baker"),
        vec![s("cfg"), s("cinespace"), s("lnh"), s("test")],
    );
    // A keyword the constructor doesn't have.
    sc.call_kw(
        n("OCIO.Baker"),
        vec![r(cfg), s("cinespace"), s("lnh"), s("test")],
        vec![("cubesize", i(2))],
        Expect::Raised,
    );
    sc
}

/// The getters, setters, iterator, errors, bakes, identities and constructors behave as the
/// wheel's.
#[test]
fn the_class_behaves_as_the_wheels() {
    common::start_python();
    compare(&[
        settings_script(),
        errors_script(),
        bake_script(),
        identity_script(),
        constructor_script(),
    ]);
}

/// `numpy.fromfile(name, dtype="u1")`: the file's bytes.
fn read(sc: &mut Script, name: &str, expect: Expect) -> usize {
    sc.call_kw(
        n("numpy.fromfile"),
        vec![s(name)],
        vec![("dtype", s("u1"))],
        expect,
    )
}

fn file_script() -> Script {
    let mut sc = Script::new();
    let cfg = config(&mut sc);
    let b = sc.call_kw(
        n("OCIO.Baker"),
        vec![r(cfg), s("cinespace"), s("lnh"), s("test")],
        vec![("cubeSize", i(2)), ("shaperSize", i(4))],
        Expect::Value,
    );
    let md = sc.call(ra(b, "getFormatMetadata"), vec![]);
    sc.call(
        ra(md, "addChildElement"),
        vec![s("Description"), s("to a file")],
    );
    sc.call(ra(b, "bake"), vec![s("out.csp")]);
    read(&mut sc, "out.csp", Expect::Value);
    sc.call_kw(
        ra(b, "bake"),
        vec![],
        vec![("fileName", s("kw.csp"))],
        Expect::Value,
    );
    read(&mut sc, "kw.csp", Expect::Value);
    sc.call(ra(b, "bake"), vec![by(b"bytes.csp")]);
    read(&mut sc, "bytes.csp", Expect::Value);
    sc.call(ra(b, "bake"), vec![s("cut.csp\u{0}tail")]);
    read(&mut sc, "cut.csp", Expect::Value);
    // A file that can't be opened.
    sc.call(ra(b, "bake"), vec![s("missing/out.csp")]);
    read(&mut sc, "missing/out.csp", Expect::Raised);
    // A bake that fails: the file is created, empty; then over a file written before.
    let bad = sc.call(n("OCIO.Baker"), vec![]);
    sc.raises(ra(bad, "bake"), vec![s("bad.csp")]);
    read(&mut sc, "bad.csp", Expect::Value);
    sc.call(ra(bad, "setFormat"), vec![s("spi1d")]);
    sc.raises(ra(bad, "bake"), vec![s("out.csp")]);
    read(&mut sc, "out.csp", Expect::Value);
    // Other formats.
    for (format, file) in [("spi3d", "x.spi3d"), ("Color Transform Format", "x.ctf")] {
        sc.call(ra(b, "setFormat"), vec![s(format)]);
        sc.call(ra(b, "bake"), vec![s(file)]);
        read(&mut sc, file, Expect::Value);
    }
    // Refused arguments.
    sc.raises(ra(b, "bake"), vec![s("x"), s("y")]);
    sc.raises(ra(b, "bake"), vec![i(1)]);
    sc.raises(ra(b, "bake"), vec![none()]);
    sc
}

/// `bake(fileName)` writes the file as the wheel's: in each side's working directory (the
/// oracle runs each script in a new one; this test makes one and works in it, and no other test
/// of this binary reads a relative path).
#[test]
fn baking_to_a_file_as_the_wheel() {
    let dir = std::env::temp_dir().join(format!("ocio-py-baker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();
    compare(&[file_script()]);
    std::env::set_current_dir(std::env::temp_dir()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The text of a step's exception, from its report.
fn message(step: &Value) -> String {
    String::from_utf8(bytes(&step["raised"]["message"])).unwrap()
}

/// Where the wheel's process dies (a null config dereferenced, a `std::string` made of a null
/// format name), the port raises U-201's error; the steps before run in the wheel.
#[test]
fn where_the_wheels_process_dies_the_port_raises() {
    let mut scripts = Vec::new();
    for case in 0..3 {
        let mut sc = Script::new();
        let cfg = config(&mut sc);
        let b = sc.call(n("OCIO.Baker"), vec![]);
        sc.call(ra(b, "setFormat"), vec![s("cinespace")]);
        sc.call(ra(b, "setConfig"), vec![r(cfg)]);
        match case {
            0 => sc.raises(ra(b, "setFormat"), vec![none()]),
            1 => sc.raises(ra(b, "setConfig"), vec![none()]),
            _ => sc.raises(
                n("OCIO.Baker"),
                vec![none(), s("cinespace"), s("lnh"), s("test")],
            ),
        };
        scripts.push(sc);
    }
    let json: Vec<Value> = scripts.iter().map(Script::json).collect();
    let result = ocio_testkit::Oracle::get()
        .call("py_eval", serde_json::json!({ "scripts": json }), &[])
        .result;
    let reports = result["scripts"].as_array().unwrap();
    assert_eq!(reports.len(), 3);
    let mut shortened = Vec::new();
    for (k, report) in reports.iter().enumerate() {
        // libstdc++ may throw for a null `std::string` (case 0) where MSVC's reads address 0.
        let crashed = report["crashed"]["returncode"]
            .as_i64()
            .is_some_and(|c| c != 0);
        let raised_on_linux = cfg!(not(windows))
            && k == 0
            && report["steps"]
                .as_array()
                .and_then(|s| s.last())
                .is_some_and(|s| s.get("raised").is_some());
        assert!(crashed || raised_on_linux, "script {k}: {report}");
        let mut steps = scripts[k].json();
        steps["steps"].as_array_mut().unwrap().pop();
        shortened.push(steps);
    }
    let theirs = common::wheel(&shortened);
    assert_eq!(theirs.len(), 3);
    for report in &theirs {
        let steps = report["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 4, "{report}");
        for step in steps {
            assert!(step.get("value").is_some(), "{step}");
        }
    }
    // The port: its error, and the baker unchanged (its format and config as set before).
    let messages = [
        "Baker.setFormat(formatName): None",
        "Baker.setConfig(config): None",
        "Baker(config): None",
    ];
    for (k, sc) in scripts.iter_mut().enumerate() {
        sc.call(ra(1, "getFormat"), vec![]);
        sc.call(ra(1, "getConfig"), vec![]);
        let ours = common::port(&sc.json());
        let steps = ours["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 7, "{ours}");
        for step in steps[..4].iter().chain(&steps[5..]) {
            assert!(step.get("value").is_some(), "script {k}: {step}");
        }
        let last = &steps[4];
        assert_eq!(
            last["raised"]["type"], "builtins.ValueError",
            "script {k}: {last}"
        );
        assert!(
            message(last).starts_with(messages[k])
                && message(last).ends_with("(its process dies; docs/improvements.md, U-201)"),
            "script {k}: {last}"
        );
        assert_eq!(bytes(&steps[5]["value"]["str"]), b"cinespace", "{ours}");
        assert_eq!(
            steps[6]["value"]["type"], "PyOpenColorIO.PyOpenColorIO.Config",
            "{ours}"
        );
    }
}
