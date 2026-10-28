// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Config` (chunks 6.3f, 6.3g) against the wheel, through the same step scripts on both sides
//! (each step's value, exception, log and identity with the earlier steps' objects):
//! - creation, copies, versions, names, text (`serialize`, `str`, `repr`, cache IDs) and
//!   validation;
//! - the environment, search paths and the working directory; the current context, which is
//!   the config's own (an edit through it is the config's);
//! - color spaces, roles, displays and views, shared and virtual views, active lists, luma,
//!   looks, view transforms, named transforms: the getters return the config's own objects
//!   (the same Python object while it lives, an edit through it is the config's, deep into
//!   their transforms, without refreshing the config's caches), the adders copy;
//! - the file and viewing rules, the config's own as well;
//! - the 13 `getProcessor` overloads, whose processors come from the config's cache (the same
//!   object for the same request), and their errors;
//! - each of the 25 iterator classes: `len`, indexing, iteration and their errors;
//! - `GetCurrentConfig`, the same object while it lives and editable, and `SetCurrentConfig`,
//!   which copies;
//! - `archive` and `isArchivable`, the archives read back by the wheel.
//!
//! Monitor-dependent `instantiateDisplayFromMonitorName` calls belong to
//! `system_monitors_oracle.rs`, with real monitor names and upstream virtual-display configs.
//!
//! And, without the wheel, whose process dies there, `None` where upstream dereferences a null
//! pointer or makes a `std::string` of it is the port's error (U-123, U-124); and against the
//! wheel, a config whose own object holds a group that holds itself or a null transform: each
//! call on it is the port's error (U-111, U-110: owner decision 2026-10-22), the other steps,
//! the repair and the shared objects after it the wheel's.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, compare, d, f, fl, i, l, n, none, r, ra, s};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

/// A config of version 2.1 using every section the class reads.
const CONFIG: &str = r#"ocio_profile_version: 2.1

environment:
  SHOT: "042"
  SEQ: ab

search_path: [luts, "shots/$SHOT"]
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]
name: probe
description: A probe config.

roles:
  default: raw
  scene_linear: lin
  color_timing: log

file_rules:
  - !<Rule> {name: LogRule, colorspace: log, pattern: "*_log*", extension: "*"}
  - !<Rule> {name: Default, colorspace: raw}

viewing_rules:
  - !<Rule> {name: scene, colorspaces: [lin, log]}
  - !<Rule> {name: enc, encodings: [log]}

shared_views:
  - !<View> {name: shared1, view_transform: vt1, display_colorspace: <USE_DISPLAY_NAME>}
  - !<View> {name: shared2, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Film, view_transform: vt1, display_colorspace: disp, looks: +look1, rule: scene, description: A film view.}
    - !<Views> [shared2]
  disp:
    - !<View> {name: Log, colorspace: log}
    - !<Views> [shared1]

virtual_display:
  - !<View> {name: VRaw, colorspace: raw}
  - !<View> {name: VFilm, view_transform: vt1, display_colorspace: <USE_DISPLAY_NAME>, looks: look1, rule: enc, description: virtual}
  - !<Views> [shared2]

active_displays: [sRGB, disp]
active_views: [Film, Raw, Log, shared1]
inactive_colorspaces: [nt2, hidden]
default_view_transform: vt1

looks:
  - !<Look>
    name: look1
    process_space: log
    description: A look.
    transform: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<Look>
    name: look2
    process_space: lin
    inverse_transform: !<ExponentTransform> {value: [2, 2, 2, 1]}

view_transforms:
  - !<ViewTransform>
    name: vt1
    family: vts
    categories: [a, b]
    from_scene_reference: !<MatrixTransform> {offset: [0.01, 0, 0, 0]}
  - !<ViewTransform>
    name: vt2
    from_display_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0}

display_colorspaces:
  - !<ColorSpace>
    name: disp
    aliases: [display, d]
    family: display
    categories: [file-io]
    encoding: sdr-video
    from_display_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: utility
    categories: [file-io, working-space]
    isdata: true
    encoding: data
  - !<ColorSpace>
    name: lin
    aliases: [linear, scene-linear]
    family: scene/linear
    categories: [working-space]
    encoding: scene-linear
    bitdepth: 32f
  - !<ColorSpace>
    name: log
    aliases: [lg]
    family: scene/log
    encoding: log
    to_scene_reference: !<LogTransform> {base: 2, direction: inverse}
  - !<ColorSpace>
    name: hidden
    categories: [file-io]
    from_scene_reference: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}

named_transforms:
  - !<NamedTransform>
    name: nt1
    aliases: [n1]
    categories: [working-space]
    encoding: log
    transform: !<LogTransform> {base: 10}
  - !<NamedTransform>
    name: nt2
    inverse_transform: !<MatrixTransform> {offset: [0, 0.1, 0, 0]}
"#;

/// A new config from [`CONFIG`]: its step.
fn config(sc: &mut Script) -> usize {
    sc.call(n("OCIO.Config.CreateFromStream"), vec![s(CONFIG)])
}

/// `m(*args)` on step `obj`'s result, which succeeds.
fn call(sc: &mut Script, obj: usize, m: &str, args: Vec<Value>) -> usize {
    sc.call(ra(obj, m), args)
}

/// `m(*args)` on step `obj`'s result, which raises.
fn raises(sc: &mut Script, obj: usize, m: &str, args: Vec<Value>) -> usize {
    sc.raises(ra(obj, m), args)
}

/// `list(step)`.
fn list(sc: &mut Script, k: usize) -> usize {
    sc.call(n("list"), vec![r(k)])
}

fn creation_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    sc.repr(r(c));
    sc.call(n("str"), vec![r(c)]);
    call(&mut sc, c, "serialize", vec![]);
    call(&mut sc, c, "getCacheID", vec![]);
    let ctx = sc.call(n("OCIO.Context"), vec![]);
    call(&mut sc, c, "getCacheID", vec![r(ctx)]);
    raises(&mut sc, c, "getCacheID", vec![s("x")]);
    call(&mut sc, c, "validate", vec![]);
    for m in [
        "getMajorVersion",
        "getMinorVersion",
        "getName",
        "getDescription",
        "getFamilySeparator",
    ] {
        call(&mut sc, c, m, vec![]);
    }
    // Copies.
    let copy = sc.call(n("copy.deepcopy"), vec![r(c)]);
    sc.call(n("operator.is_"), vec![r(copy), r(c)]);
    call(&mut sc, copy, "setName", vec![s("copy")]);
    call(&mut sc, c, "getName", vec![]);
    call(&mut sc, copy, "getName", vec![]);
    raises(&mut sc, c, "__deepcopy__", vec![l(vec![])]);
    let copy2 = call(&mut sc, c, "__deepcopy__", vec![d(vec![])]);
    call(&mut sc, copy2, "serialize", vec![]);
    // The other creators.
    let raw = sc.call(n("OCIO.Config"), vec![]);
    call(&mut sc, raw, "serialize", vec![]);
    let raw2 = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    call(&mut sc, raw2, "serialize", vec![]);
    call(&mut sc, raw2, "validate", vec![]);
    let builtin = sc.call(
        n("OCIO.Config.CreateFromBuiltinConfig"),
        vec![s("ocio://default")],
    );
    call(&mut sc, builtin, "getName", vec![]);
    call(&mut sc, builtin, "getCacheID", vec![]);
    let builtin2 = sc.call(
        n("OCIO.Config.CreateFromBuiltinConfig"),
        vec![s("studio-config-v1.0.0_aces-v1.3_ocio-v2.1")],
    );
    call(&mut sc, builtin2, "getName", vec![]);
    sc.raises(n("OCIO.Config.CreateFromBuiltinConfig"), vec![s("nope")]);
    sc.raises(n("OCIO.Config.CreateFromBuiltinConfig"), vec![s("")]);
    for text in [
        "",
        "ocio_profile_version: 7\n",
        "a: [",
        "ocio_profile_version: 2\nroles: 3\n",
    ] {
        sc.raises(n("OCIO.Config.CreateFromStream"), vec![s(text)]);
    }
    sc.raises(n("OCIO.Config.CreateFromStream"), vec![none()]);
    for path in ["no/such/config.ocio", ""] {
        sc.raises(n("OCIO.Config.CreateFromFile"), vec![s(path)]);
    }
    sc.raises(n("OCIO.Config.CreateFromFile"), vec![none()]);
    sc.raises(n("OCIO.Config.CreateFromFile"), vec![i(3)]);
    sc.raises(n("OCIO.Config.CreateRaw"), vec![i(3)]);
    // Versions.
    let v = config(&mut sc);
    call(&mut sc, v, "setMinorVersion", vec![i(0)]);
    call(&mut sc, v, "getMinorVersion", vec![]);
    raises(&mut sc, v, "setMinorVersion", vec![i(9)]);
    raises(&mut sc, v, "setMajorVersion", vec![i(0)]);
    raises(&mut sc, v, "setMajorVersion", vec![i(3)]);
    call(&mut sc, v, "setMajorVersion", vec![i(1)]);
    call(&mut sc, v, "getMajorVersion", vec![]);
    call(&mut sc, v, "getMinorVersion", vec![]);
    raises(&mut sc, v, "serialize", vec![]);
    call(&mut sc, v, "setVersion", vec![i(2), i(0)]);
    raises(&mut sc, v, "setVersion", vec![i(2), i(7)]);
    raises(&mut sc, v, "setVersion", vec![i(-1), i(0)]);
    call(&mut sc, v, "upgradeToLatestVersion", vec![]);
    call(&mut sc, v, "getMajorVersion", vec![]);
    call(&mut sc, v, "getMinorVersion", vec![]);
    call(&mut sc, v, "serialize", vec![]);
    // Names.
    call(&mut sc, v, "setName", vec![s("n\u{e9}")]);
    raises(&mut sc, v, "setName", vec![none()]);
    call(&mut sc, v, "getName", vec![]);
    call(&mut sc, v, "setDescription", vec![s("two\nlines")]);
    call(&mut sc, v, "getDescription", vec![]);
    call(&mut sc, v, "setFamilySeparator", vec![s("-")]);
    call(&mut sc, v, "getFamilySeparator", vec![]);
    raises(&mut sc, v, "setFamilySeparator", vec![s("ab")]);
    raises(&mut sc, v, "setFamilySeparator", vec![s("\u{e9}")]);
    raises(&mut sc, v, "setFamilySeparator", vec![s("")]);
    call(&mut sc, v, "getFamilySeparator", vec![]);
    sc.repr(r(v));
    // Validation errors.
    let bad = config(&mut sc);
    call(&mut sc, bad, "setRole", vec![s("default"), s("nope")]);
    raises(&mut sc, bad, "validate", vec![]);
    sc
}

fn environment_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    let names = call(&mut sc, c, "getEnvironmentVarNames", vec![]);
    list(&mut sc, names);
    call(&mut sc, c, "getEnvironmentVarDefault", vec![s("SHOT")]);
    call(&mut sc, c, "getEnvironmentVarDefault", vec![s("nope")]);
    call(&mut sc, c, "addEnvironmentVar", vec![s("X"), s("1")]);
    call(&mut sc, c, "addEnvironmentVar", vec![s("SEQ"), none()]);
    list(&mut sc, names);
    call(&mut sc, c, "getEnvironmentMode", vec![]);
    call(
        &mut sc,
        c,
        "setEnvironmentMode",
        vec![n("OCIO.ENV_ENVIRONMENT_LOAD_PREDEFINED")],
    );
    call(&mut sc, c, "getEnvironmentMode", vec![]);
    call(&mut sc, c, "loadEnvironment", vec![]);
    call(&mut sc, c, "getCacheID", vec![]);
    // The current context is the config's own.
    let ctx = call(&mut sc, c, "getCurrentContext", vec![]);
    let ctx2 = call(&mut sc, c, "getCurrentContext", vec![]);
    sc.repr(r(ctx2));
    call(&mut sc, ctx, "setStringVar", vec![s("SHOT"), s("007")]);
    call(&mut sc, ctx2, "__getitem__", vec![s("SHOT")]);
    call(&mut sc, c, "getEnvironmentVarDefault", vec![s("SHOT")]);
    call(&mut sc, c, "serialize", vec![]);
    call(&mut sc, c, "clearEnvironmentVars", vec![]);
    list(&mut sc, names);
    call(&mut sc, c, "getCurrentContext", vec![]);
    sc.repr(r(ctx));
    // Search paths and the working directory.
    call(&mut sc, c, "getSearchPath", vec![]);
    let paths = call(&mut sc, c, "getSearchPaths", vec![]);
    list(&mut sc, paths);
    call(&mut sc, c, "setSearchPath", vec![s("a:b;c")]);
    call(&mut sc, c, "getSearchPath", vec![]);
    list(&mut sc, paths);
    call(&mut sc, c, "addSearchPath", vec![s("d")]);
    call(&mut sc, c, "addSearchPath", vec![s("")]);
    call(&mut sc, c, "getSearchPath", vec![]);
    call(&mut sc, c, "setSearchPath", vec![none()]);
    call(&mut sc, c, "getSearchPath", vec![]);
    call(&mut sc, c, "addSearchPath", vec![s("e")]);
    call(&mut sc, c, "clearSearchPaths", vec![]);
    list(&mut sc, paths);
    call(&mut sc, c, "getWorkingDir", vec![]);
    call(&mut sc, c, "setWorkingDir", vec![s("some/dir")]);
    call(&mut sc, c, "getWorkingDir", vec![]);
    call(&mut sc, c, "setWorkingDir", vec![none()]);
    call(&mut sc, c, "getWorkingDir", vec![]);
    call(&mut sc, c, "getCacheID", vec![]);
    sc
}

fn color_spaces_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    // The config's own color space, by its name and its aliases.
    let lin = call(&mut sc, c, "getColorSpace", vec![s("lin")]);
    let alias = call(&mut sc, c, "getColorSpace", vec![s("LINEAR")]);
    sc.repr(r(alias));
    call(&mut sc, c, "getColorSpace", vec![s("nope")]);
    call(&mut sc, c, "getColorSpace", vec![s("scene_linear")]);
    call(&mut sc, lin, "setDescription", vec![s("edited")]);
    let tr = call(&mut sc, c, "getColorSpace", vec![s("log")]);
    let logt = call(
        &mut sc,
        tr,
        "getTransform",
        vec![n("OCIO.COLORSPACE_DIR_TO_REFERENCE")],
    );
    call(&mut sc, logt, "setBase", vec![f(3.0)]);
    call(&mut sc, c, "serialize", vec![]);
    let all = call(&mut sc, c, "getColorSpaces", vec![]);
    list(&mut sc, all);
    let cat = call(&mut sc, c, "getColorSpaces", vec![s("file-io")]);
    call(&mut sc, cat, "getColorSpaceNames", vec![]);
    call(&mut sc, c, "getColorSpaces", vec![s("")]);
    call(&mut sc, c, "getColorSpaces", vec![none()]);
    for (search, vis) in [
        ("OCIO.SEARCH_REFERENCE_SPACE_ALL", "OCIO.COLORSPACE_ALL"),
        (
            "OCIO.SEARCH_REFERENCE_SPACE_SCENE",
            "OCIO.COLORSPACE_ACTIVE",
        ),
        (
            "OCIO.SEARCH_REFERENCE_SPACE_DISPLAY",
            "OCIO.COLORSPACE_INACTIVE",
        ),
        (
            "OCIO.SEARCH_REFERENCE_SPACE_ALL",
            "OCIO.COLORSPACE_INACTIVE",
        ),
    ] {
        let k = call(&mut sc, c, "getColorSpaces", vec![n(search), n(vis)]);
        list(&mut sc, k);
        let k = call(&mut sc, c, "getColorSpaceNames", vec![n(search), n(vis)]);
        list(&mut sc, k);
    }
    let names = call(&mut sc, c, "getColorSpaceNames", vec![]);
    list(&mut sc, names);
    for name in ["lg", "LIN", "display", "scene_linear", "nope", ""] {
        call(&mut sc, c, "getCanonicalName", vec![s(name)]);
    }
    // Adding copies; removing.
    let cs = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("added")), ("aliases", l(vec![s("ad")]))],
        Expect::Value,
    );
    call(&mut sc, c, "addColorSpace", vec![r(cs)]);
    let got = call(&mut sc, c, "getColorSpace", vec![s("ad")]);
    sc.call(n("operator.is_"), vec![r(got), r(cs)]);
    call(&mut sc, cs, "setDescription", vec![s("after")]);
    call(&mut sc, got, "getDescription", vec![]);
    let clash = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("x")), ("aliases", l(vec![s("lin")]))],
        Expect::Value,
    );
    raises(&mut sc, c, "addColorSpace", vec![r(clash)]);
    call(&mut sc, c, "removeColorSpace", vec![s("added")]);
    call(&mut sc, c, "removeColorSpace", vec![s("nope")]);
    call(&mut sc, got, "getName", vec![]);
    list(&mut sc, names);
    // Linearity, use, parsing.
    for (name, space) in [
        ("lin", "OCIO.REFERENCE_SPACE_SCENE"),
        ("log", "OCIO.REFERENCE_SPACE_SCENE"),
        ("disp", "OCIO.REFERENCE_SPACE_DISPLAY"),
        ("raw", "OCIO.REFERENCE_SPACE_SCENE"),
    ] {
        call(&mut sc, c, "isColorSpaceLinear", vec![s(name), n(space)]);
    }
    raises(
        &mut sc,
        c,
        "isColorSpaceLinear",
        vec![s("nope"), n("OCIO.REFERENCE_SPACE_SCENE")],
    );
    for name in ["log", "hidden", "lin", "nope"] {
        call(&mut sc, c, "isColorSpaceUsed", vec![s(name)]);
    }
    for text in ["a_log.exr", "a_lin_disp.exr", "plain.exr", ""] {
        call(&mut sc, c, "parseColorSpaceFromString", vec![s(text)]);
    }
    call(&mut sc, c, "isStrictParsingEnabled", vec![]);
    call(&mut sc, c, "setStrictParsingEnabled", vec![b(false)]);
    call(&mut sc, c, "isStrictParsingEnabled", vec![]);
    call(
        &mut sc,
        c,
        "parseColorSpaceFromString",
        vec![s("plain.exr")],
    );
    // Inactive color spaces.
    call(&mut sc, c, "getInactiveColorSpaces", vec![]);
    for name in ["hidden", "HIDDEN", "nt2", "lin", ""] {
        call(&mut sc, c, "isInactiveColorSpace", vec![s(name)]);
    }
    call(&mut sc, c, "setInactiveColorSpaces", vec![s("lin, log")]);
    call(&mut sc, c, "getInactiveColorSpaces", vec![]);
    list(&mut sc, names);
    call(&mut sc, c, "setInactiveColorSpaces", vec![none()]);
    call(&mut sc, c, "getInactiveColorSpaces", vec![]);
    list(&mut sc, names);
    call(&mut sc, c, "serialize", vec![]);
    call(&mut sc, c, "clearColorSpaces", vec![]);
    list(&mut sc, names);
    call(&mut sc, lin, "getName", vec![]);
    call(&mut sc, c, "getColorSpace", vec![s("lin")]);
    sc
}

fn displays_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    // Roles.
    call(&mut sc, c, "setRole", vec![s("extra"), s("lin")]);
    call(&mut sc, c, "hasRole", vec![s("extra")]);
    call(&mut sc, c, "hasRole", vec![s("nope")]);
    call(&mut sc, c, "getRoleColorSpace", vec![s("EXTRA")]);
    call(&mut sc, c, "getRoleColorSpace", vec![s("nope")]);
    let roles = call(&mut sc, c, "getRoleNames", vec![]);
    list(&mut sc, roles);
    let pairs = call(&mut sc, c, "getRoles", vec![]);
    list(&mut sc, pairs);
    call(&mut sc, c, "setRole", vec![s("extra"), none()]);
    list(&mut sc, roles);
    raises(&mut sc, c, "setRole", vec![s(""), s("lin")]);
    // Displays and views.
    for m in ["getDefaultDisplay", "getDefaultViewTransformName"] {
        call(&mut sc, c, m, vec![]);
    }
    let displays = call(&mut sc, c, "getDisplays", vec![]);
    list(&mut sc, displays);
    let all = call(&mut sc, c, "getDisplaysAll", vec![]);
    list(&mut sc, all);
    for display in ["sRGB", "disp", "nope"] {
        call(&mut sc, c, "getDefaultView", vec![s(display)]);
        call(&mut sc, c, "getDefaultView", vec![s(display), s("lin")]);
        let k = call(&mut sc, c, "getViews", vec![s(display)]);
        list(&mut sc, k);
        let k = call(&mut sc, c, "getViews", vec![s(display), s("log")]);
        list(&mut sc, k);
        for ty in ["OCIO.VIEW_SHARED", "OCIO.VIEW_DISPLAY_DEFINED"] {
            let k = call(&mut sc, c, "getViews", vec![n(ty), s(display)]);
            list(&mut sc, k);
        }
    }
    for (display, view) in [
        ("sRGB", "Film"),
        ("sRGB", "Raw"),
        ("disp", "shared1"),
        ("sRGB", "shared2"),
        ("nope", "Film"),
        ("sRGB", "nope"),
    ] {
        for m in [
            "getDisplayViewTransformName",
            "getDisplayViewColorSpaceName",
            "getDisplayViewLooks",
            "getDisplayViewRule",
            "getDisplayViewDescription",
            "hasView",
            "isViewShared",
        ] {
            call(&mut sc, c, m, vec![s(display), s(view)]);
        }
    }
    call(
        &mut sc,
        c,
        "addDisplayView",
        vec![s("new"), s("v"), s("lin"), s("")],
    );
    sc.call_kw(
        ra(c, "addDisplayView"),
        vec![s("new"), s("w"), s("vt1"), s("disp")],
        vec![
            ("looks", s("look1")),
            ("ruleName", s("scene")),
            ("description", s("d")),
        ],
        Expect::Value,
    );
    raises(
        &mut sc,
        c,
        "addDisplayView",
        vec![s(""), s("v"), s("lin"), s("")],
    );
    raises(
        &mut sc,
        c,
        "addDisplayView",
        vec![s("new"), s("x"), s(""), s("")],
    );
    call(
        &mut sc,
        c,
        "addSharedView",
        vec![s("s3"), s(""), s("lin"), s(""), s(""), s("")],
    );
    raises(
        &mut sc,
        c,
        "addSharedView",
        vec![s(""), s(""), s("lin"), s(""), s(""), s("")],
    );
    let shared = call(&mut sc, c, "getSharedViews", vec![]);
    list(&mut sc, shared);
    call(&mut sc, c, "addDisplaySharedView", vec![s("new"), s("s3")]);
    raises(&mut sc, c, "addDisplaySharedView", vec![s("new"), s("s3")]);
    let views = call(&mut sc, c, "getViews", vec![s("new")]);
    list(&mut sc, views);
    call(&mut sc, c, "removeDisplayView", vec![s("new"), s("v")]);
    raises(&mut sc, c, "removeDisplayView", vec![s("new"), s("nope")]);
    list(&mut sc, views);
    raises(&mut sc, c, "removeSharedView", vec![s("nope")]);
    call(&mut sc, c, "removeSharedView", vec![s("s3")]);
    list(&mut sc, shared);
    let other = config(&mut sc);
    call(
        &mut sc,
        other,
        "addDisplayView",
        vec![s("sRGB"), s("Raw2"), s("raw"), s("")],
    );
    for (display, view) in [("sRGB", "Raw"), ("sRGB", "Film"), ("sRGB", "nope")] {
        sc.call(
            n("OCIO.Config.AreViewsEqual"),
            vec![r(c), r(other), s(display), s(view)],
        );
    }
    call(&mut sc, c, "serialize", vec![]);
    call(&mut sc, c, "clearSharedViews", vec![]);
    list(&mut sc, shared);
    // The virtual display.
    for view in ["VRaw", "VFilm", "shared2", "nope"] {
        call(&mut sc, c, "hasVirtualView", vec![s(view)]);
        call(&mut sc, c, "isVirtualViewShared", vec![s(view)]);
        for m in [
            "getVirtualDisplayViewTransformName",
            "getVirtualDisplayViewColorSpaceName",
            "getVirtualDisplayViewLooks",
            "getVirtualDisplayViewRule",
            "getVirtualDisplayViewDescription",
        ] {
            call(&mut sc, c, m, vec![s(view)]);
        }
    }
    for ty in ["OCIO.VIEW_SHARED", "OCIO.VIEW_DISPLAY_DEFINED"] {
        let k = call(&mut sc, c, "getVirtualDisplayViews", vec![n(ty)]);
        list(&mut sc, k);
    }
    call(
        &mut sc,
        c,
        "addVirtualDisplayView",
        vec![s("V3"), s(""), s("lin")],
    );
    sc.call_kw(
        ra(c, "addVirtualDisplayView"),
        vec![s("V4"), s("vt1"), s("disp")],
        vec![("looks", s("look2")), ("description", s("four"))],
        Expect::Value,
    );
    raises(
        &mut sc,
        c,
        "addVirtualDisplayView",
        vec![s(""), s(""), s("lin")],
    );
    call(
        &mut sc,
        c,
        "addVirtualDisplaySharedView",
        vec![s("shared1")],
    );
    raises(
        &mut sc,
        c,
        "addVirtualDisplaySharedView",
        vec![s("shared1")],
    );
    call(&mut sc, c, "removeVirtualDisplayView", vec![s("VRaw")]);
    call(&mut sc, c, "removeVirtualDisplayView", vec![s("nope")]);
    let vv = call(
        &mut sc,
        c,
        "getVirtualDisplayViews",
        vec![n("OCIO.VIEW_DISPLAY_DEFINED")],
    );
    list(&mut sc, vv);
    for view in ["VFilm", "V3", "nope"] {
        sc.call(
            n("OCIO.Config.AreVirtualViewsEqual"),
            vec![r(c), r(other), s(view)],
        );
    }
    raises(
        &mut sc,
        c,
        "instantiateDisplayFromICCProfile",
        vec![s("no/such.icc")],
    );
    raises(&mut sc, c, "instantiateDisplayFromICCProfile", vec![s("")]);
    call(&mut sc, c, "isDisplayTemporary", vec![s("sRGB")]);
    call(&mut sc, c, "isDisplayTemporary", vec![s("nope")]);
    raises(&mut sc, c, "isDisplayTemporary", vec![i(0)]);
    call(&mut sc, c, "setDisplayTemporary", vec![s("sRGB"), b(true)]);
    call(&mut sc, c, "isDisplayTemporary", vec![s("sRGB")]);
    call(&mut sc, c, "setDisplayTemporary", vec![s("nope"), b(true)]);
    call(&mut sc, c, "serialize", vec![]);
    call(&mut sc, c, "clearVirtualDisplay", vec![]);
    list(&mut sc, vv);
    // Active displays and views.
    for (get, set, add, remove, clear, num) in [
        (
            "getActiveDisplays",
            "setActiveDisplays",
            "addActiveDisplay",
            "removeActiveDisplay",
            "clearActiveDisplays",
            "getNumActiveDisplays",
        ),
        (
            "getActiveViews",
            "setActiveViews",
            "addActiveView",
            "removeActiveView",
            "clearActiveViews",
            "getNumActiveViews",
        ),
    ] {
        call(&mut sc, c, get, vec![]);
        call(&mut sc, c, num, vec![]);
        call(&mut sc, c, set, vec![s("a, \"b, c\", d")]);
        call(&mut sc, c, get, vec![]);
        call(&mut sc, c, num, vec![]);
        call(&mut sc, c, add, vec![s("e")]);
        call(&mut sc, c, add, vec![s("e")]);
        raises(&mut sc, c, add, vec![s("")]);
        call(&mut sc, c, remove, vec![s("a")]);
        raises(&mut sc, c, remove, vec![s("a")]);
        call(&mut sc, c, get, vec![]);
        call(&mut sc, c, clear, vec![]);
        call(&mut sc, c, get, vec![]);
        call(&mut sc, c, num, vec![]);
        call(&mut sc, c, set, vec![s("")]);
    }
    let displays2 = call(&mut sc, c, "getDisplays", vec![]);
    list(&mut sc, displays2);
    // Luma.
    call(&mut sc, c, "getDefaultLumaCoefs", vec![]);
    call(
        &mut sc,
        c,
        "setDefaultLumaCoefs",
        vec![fl(&[0.25, 0.5, 0.125])],
    );
    call(&mut sc, c, "getDefaultLumaCoefs", vec![]);
    raises(&mut sc, c, "setDefaultLumaCoefs", vec![fl(&[0.25, 0.5])]);
    raises(&mut sc, c, "setDefaultLumaCoefs", vec![none()]);
    call(&mut sc, c, "serialize", vec![]);
    call(&mut sc, c, "clearDisplays", vec![]);
    list(&mut sc, displays2);
    list(&mut sc, all);
    sc
}

fn looks_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    // Looks.
    let look = call(&mut sc, c, "getLook", vec![s("look1")]);
    call(&mut sc, c, "getLook", vec![s("LOOK1")]);
    call(&mut sc, c, "getLook", vec![s("nope")]);
    let mt = call(&mut sc, look, "getTransform", vec![]);
    call(&mut sc, mt, "setOffset", vec![fl(&[0.5, 0.5, 0.5, 0.0])]);
    call(&mut sc, look, "setDescription", vec![s("edited")]);
    let names = call(&mut sc, c, "getLookNames", vec![]);
    list(&mut sc, names);
    let looks = call(&mut sc, c, "getLooks", vec![]);
    list(&mut sc, looks);
    let new = sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("name", s("look3")), ("processSpace", s("lin"))],
        Expect::Value,
    );
    call(&mut sc, c, "addLook", vec![r(new)]);
    let got = call(&mut sc, c, "getLook", vec![s("look3")]);
    sc.call(n("operator.is_"), vec![r(got), r(new)]);
    let renamed = sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("name", s("LOOK1")), ("processSpace", s("raw"))],
        Expect::Value,
    );
    call(&mut sc, c, "addLook", vec![r(renamed)]);
    call(&mut sc, c, "getLook", vec![s("look1")]);
    call(&mut sc, look, "getProcessSpace", vec![]);
    let unnamed = sc.call(n("OCIO.Look"), vec![]);
    raises(&mut sc, c, "addLook", vec![r(unnamed)]);
    list(&mut sc, names);
    // View transforms.
    let vt = call(&mut sc, c, "getViewTransform", vec![s("vt1")]);
    call(&mut sc, c, "getViewTransform", vec![s("VT1")]);
    call(&mut sc, c, "getViewTransform", vec![s("nope")]);
    call(&mut sc, vt, "addCategory", vec![s("c")]);
    let vt_names = call(&mut sc, c, "getViewTransformNames", vec![]);
    list(&mut sc, vt_names);
    let vts = call(&mut sc, c, "getViewTransforms", vec![]);
    list(&mut sc, vts);
    call(&mut sc, c, "getDefaultSceneToDisplayViewTransform", vec![]);
    call(&mut sc, c, "setDefaultViewTransformName", vec![s("vt2")]);
    call(&mut sc, c, "getDefaultViewTransformName", vec![]);
    call(&mut sc, c, "getDefaultSceneToDisplayViewTransform", vec![]);
    call(&mut sc, c, "setDefaultViewTransformName", vec![s("nope")]);
    call(&mut sc, c, "getDefaultSceneToDisplayViewTransform", vec![]);
    let bare_vt = sc.call_kw(
        n("OCIO.ViewTransform"),
        vec![],
        vec![("name", s("vt4"))],
        Expect::Value,
    );
    raises(&mut sc, c, "addViewTransform", vec![r(bare_vt)]);
    let m0 = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let new_vt = sc.call_kw(
        n("OCIO.ViewTransform"),
        vec![],
        vec![
            ("name", s("vt3")),
            ("referenceSpace", n("OCIO.REFERENCE_SPACE_DISPLAY")),
            ("toReference", r(m0)),
        ],
        Expect::Value,
    );
    call(&mut sc, c, "addViewTransform", vec![r(new_vt)]);
    let got_vt = call(&mut sc, c, "getViewTransform", vec![s("vt3")]);
    sc.call(n("operator.is_"), vec![r(got_vt), r(new_vt)]);
    let unnamed = sc.call(n("OCIO.ViewTransform"), vec![]);
    raises(&mut sc, c, "addViewTransform", vec![r(unnamed)]);
    // Named transforms.
    let nt = call(&mut sc, c, "getNamedTransform", vec![s("n1")]);
    call(&mut sc, c, "getNamedTransform", vec![s("NT1")]);
    call(&mut sc, c, "getNamedTransform", vec![s("nope")]);
    call(&mut sc, c, "getNamedTransform", vec![s("nt2")]);
    let lt = call(
        &mut sc,
        nt,
        "getTransform",
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
    );
    call(&mut sc, lt, "setBase", vec![f(4.0)]);
    for vis in [
        "OCIO.NAMEDTRANSFORM_ALL",
        "OCIO.NAMEDTRANSFORM_ACTIVE",
        "OCIO.NAMEDTRANSFORM_INACTIVE",
    ] {
        let k = call(&mut sc, c, "getNamedTransformNames", vec![n(vis)]);
        list(&mut sc, k);
        let k = call(&mut sc, c, "getNamedTransforms", vec![n(vis)]);
        list(&mut sc, k);
    }
    let nt_names = call(&mut sc, c, "getNamedTransformNames", vec![]);
    list(&mut sc, nt_names);
    let nts = call(&mut sc, c, "getNamedTransforms", vec![]);
    list(&mut sc, nts);
    let m1 = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let new_nt = sc.call_kw(
        n("OCIO.NamedTransform"),
        vec![],
        vec![("name", s("nt3")), ("forwardTransform", r(m1))],
        Expect::Value,
    );
    call(&mut sc, c, "addNamedTransform", vec![r(new_nt)]);
    let got_nt = call(&mut sc, c, "getNamedTransform", vec![s("nt3")]);
    sc.call(n("operator.is_"), vec![r(got_nt), r(new_nt)]);
    let clash = sc.call_kw(
        n("OCIO.NamedTransform"),
        vec![],
        vec![
            ("name", s("x")),
            ("aliases", l(vec![s("lin")])),
            ("forwardTransform", r(m1)),
        ],
        Expect::Value,
    );
    raises(&mut sc, c, "addNamedTransform", vec![r(clash)]);
    let empty = sc.call_kw(
        n("OCIO.NamedTransform"),
        vec![],
        vec![("name", s("y"))],
        Expect::Value,
    );
    raises(&mut sc, c, "addNamedTransform", vec![r(empty)]);
    call(&mut sc, c, "serialize", vec![]);
    call(&mut sc, c, "clearLooks", vec![]);
    call(&mut sc, c, "clearViewTransforms", vec![]);
    call(&mut sc, c, "clearNamedTransforms", vec![]);
    list(&mut sc, names);
    list(&mut sc, vt_names);
    list(&mut sc, nt_names);
    call(&mut sc, look, "getName", vec![]);
    call(&mut sc, c, "serialize", vec![]);
    sc
}

fn rules_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    let fr = call(&mut sc, c, "getFileRules", vec![]);
    let fr2 = call(&mut sc, c, "getFileRules", vec![]);
    sc.repr(r(fr2));
    for path in ["a_log_b.exr", "plain.exr", "", "dir/x_log.dpx"] {
        call(&mut sc, c, "getColorSpaceFromFilepath", vec![s(path)]);
        call(&mut sc, c, "filepathOnlyMatchesDefaultRule", vec![s(path)]);
    }
    call(
        &mut sc,
        fr,
        "insertRule",
        vec![i(0), s("Linear"), s("lin"), s("*lin*"), s("*")],
    );
    call(
        &mut sc,
        c,
        "getColorSpaceFromFilepath",
        vec![s("a_lin.exr")],
    );
    call(&mut sc, c, "serialize", vec![]);
    let other = sc.call(n("OCIO.FileRules"), vec![]);
    call(&mut sc, other, "setDefaultRuleColorSpace", vec![s("log")]);
    call(&mut sc, c, "setFileRules", vec![r(other)]);
    let fr3 = call(&mut sc, c, "getFileRules", vec![]);
    sc.call(n("operator.is_"), vec![r(fr3), r(other)]);
    call(&mut sc, other, "setDefaultRuleColorSpace", vec![s("raw")]);
    call(
        &mut sc,
        c,
        "getColorSpaceFromFilepath",
        vec![s("a_lin.exr")],
    );
    sc.repr(r(fr));
    raises(&mut sc, c, "getColorSpaceFromFilepath", vec![none()]);
    let vr = call(&mut sc, c, "getViewingRules", vec![]);
    call(&mut sc, c, "getViewingRules", vec![]);
    call(&mut sc, vr, "insertRule", vec![i(0), s("more")]);
    call(&mut sc, vr, "addColorSpace", vec![i(0), s("raw")]);
    sc.repr(r(vr));
    call(&mut sc, c, "serialize", vec![]);
    let views = call(&mut sc, c, "getViews", vec![s("sRGB"), s("raw")]);
    list(&mut sc, views);
    let other_vr = sc.call(n("OCIO.ViewingRules"), vec![]);
    call(&mut sc, c, "setViewingRules", vec![r(other_vr)]);
    let vr2 = call(&mut sc, c, "getViewingRules", vec![]);
    sc.call(n("operator.is_"), vec![r(vr2), r(other_vr)]);
    call(&mut sc, c, "serialize", vec![]);
    sc
}

fn processors_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    let ctx = sc.call(n("OCIO.Context"), vec![]);
    let cur = call(&mut sc, c, "getCurrentContext", vec![]);
    let lin = call(&mut sc, c, "getColorSpace", vec![s("lin")]);
    let log = call(&mut sc, c, "getColorSpace", vec![s("log")]);
    let nt = call(&mut sc, c, "getNamedTransform", vec![s("nt1")]);
    // Each overload; the same request twice gives the cached processor.
    let p = call(&mut sc, c, "getProcessor", vec![s("lin"), s("log")]);
    sc.call(n("type"), vec![r(p)]);
    call(&mut sc, c, "getProcessor", vec![s("lin"), s("log")]);
    call(&mut sc, c, "getProcessor", vec![s("linear"), s("lg")]);
    call(&mut sc, c, "getProcessor", vec![r(lin), r(log)]);
    call(&mut sc, c, "getProcessor", vec![r(ctx), r(lin), r(log)]);
    // The same color space as source and destination (the config's own, and one of its own).
    call(&mut sc, c, "getProcessor", vec![r(lin), r(lin)]);
    call(&mut sc, c, "getProcessor", vec![r(ctx), r(lin), r(lin)]);
    call(&mut sc, c, "getProcessor", vec![s("lin"), s("lin")]);
    let own = sc.call(n("OCIO.ColorSpace"), vec![]);
    call(&mut sc, own, "setName", vec![s("log")]);
    call(&mut sc, c, "getProcessor", vec![r(own), r(own)]);
    call(&mut sc, c, "getProcessor", vec![r(cur), r(own), r(own)]);
    call(&mut sc, c, "getProcessor", vec![r(cur), s("lin"), s("log")]);
    call(&mut sc, c, "getProcessor", vec![r(ctx), s("lin"), s("log")]);
    let dir_fwd = n("OCIO.TRANSFORM_DIR_FORWARD");
    let dir_inv = n("OCIO.TRANSFORM_DIR_INVERSE");
    call(
        &mut sc,
        c,
        "getProcessor",
        vec![s("lin"), s("sRGB"), s("Film"), dir_fwd.clone()],
    );
    call(
        &mut sc,
        c,
        "getProcessor",
        vec![s("lin"), s("sRGB"), s("Film"), dir_inv.clone()],
    );
    call(
        &mut sc,
        c,
        "getProcessor",
        vec![r(ctx), s("lin"), s("disp"), s("shared1"), dir_fwd.clone()],
    );
    call(&mut sc, c, "getProcessor", vec![r(nt), dir_fwd.clone()]);
    call(
        &mut sc,
        c,
        "getProcessor",
        vec![r(ctx), r(nt), dir_inv.clone()],
    );
    call(&mut sc, c, "getProcessor", vec![s("n1"), dir_fwd.clone()]);
    call(
        &mut sc,
        c,
        "getProcessor",
        vec![r(ctx), s("nt2"), dir_inv.clone()],
    );
    let mt = sc.call(n("OCIO.MatrixTransform"), vec![]);
    call(&mut sc, c, "getProcessor", vec![r(mt)]);
    call(&mut sc, c, "getProcessor", vec![r(mt), dir_inv.clone()]);
    call(
        &mut sc,
        c,
        "getProcessor",
        vec![r(ctx), r(mt), dir_fwd.clone()],
    );
    let cst = sc.call(n("OCIO.ColorSpaceTransform"), vec![s("lin"), s("log")]);
    call(&mut sc, c, "getProcessor", vec![r(cst)]);
    let dvt = sc.call(
        n("OCIO.DisplayViewTransform"),
        vec![s("lin"), s("sRGB"), s("Film")],
    );
    call(&mut sc, c, "getProcessor", vec![r(dvt)]);
    let lt = sc.call(
        n("OCIO.LookTransform"),
        vec![s("lin"), s("log"), s("look1, -look2")],
    );
    call(&mut sc, c, "getProcessor", vec![r(lt)]);
    let group = sc.call(n("OCIO.GroupTransform"), vec![]);
    call(&mut sc, group, "appendTransform", vec![r(cst)]);
    call(&mut sc, c, "getProcessor", vec![r(group)]);
    // Their errors.
    raises(&mut sc, c, "getProcessor", vec![s("nope"), s("log")]);
    raises(&mut sc, c, "getProcessor", vec![s("lin"), s("")]);
    raises(&mut sc, c, "getProcessor", vec![none(), s("log")]);
    raises(&mut sc, c, "getProcessor", vec![s("lin"), none()]);
    raises(&mut sc, c, "getProcessor", vec![none(), r(log)]);
    raises(&mut sc, c, "getProcessor", vec![none(), r(lin), r(log)]);
    raises(&mut sc, c, "getProcessor", vec![none(), s("lin"), s("log")]);
    raises(
        &mut sc,
        c,
        "getProcessor",
        vec![s("lin"), s("nope"), s("Film"), dir_fwd.clone()],
    );
    raises(
        &mut sc,
        c,
        "getProcessor",
        vec![s("lin"), s("sRGB"), s("nope"), dir_fwd.clone()],
    );
    // The display view transform is validated before the context is checked.
    for (src, display, view) in [
        ("", "sRGB", "Film"),
        ("lin", "", "Film"),
        ("lin", "sRGB", ""),
    ] {
        raises(
            &mut sc,
            c,
            "getProcessor",
            vec![none(), s(src), s(display), s(view), dir_fwd.clone()],
        );
        raises(
            &mut sc,
            c,
            "getProcessor",
            vec![s(src), s(display), s(view), dir_inv.clone()],
        );
    }
    raises(
        &mut sc,
        c,
        "getProcessor",
        vec![none(), s("lin"), s("sRGB"), s("Film"), dir_fwd.clone()],
    );
    raises(&mut sc, c, "getProcessor", vec![s("nope"), dir_fwd.clone()]);
    raises(&mut sc, c, "getProcessor", vec![none(), dir_fwd.clone()]);
    raises(&mut sc, c, "getProcessor", vec![none()]);
    raises(&mut sc, c, "getProcessor", vec![none(), dir_fwd.clone()]);
    let bad = sc.call(n("OCIO.ColorSpaceTransform"), vec![s("lin"), s("nope")]);
    raises(&mut sc, c, "getProcessor", vec![r(bad)]);
    raises(&mut sc, c, "getProcessor", vec![]);
    raises(&mut sc, c, "getProcessor", vec![i(1)]);
    // The cache.
    call(
        &mut sc,
        c,
        "setProcessorCacheFlags",
        vec![n("OCIO.PROCESSOR_CACHE_OFF")],
    );
    call(&mut sc, c, "getProcessor", vec![s("lin"), s("log")]);
    call(&mut sc, c, "getProcessor", vec![s("lin"), s("log")]);
    call(
        &mut sc,
        c,
        "setProcessorCacheFlags",
        vec![n("OCIO.PROCESSOR_CACHE_DEFAULT")],
    );
    call(&mut sc, c, "getProcessor", vec![s("lin"), s("log")]);
    call(&mut sc, c, "clearProcessorCache", vec![]);
    call(&mut sc, c, "getProcessor", vec![s("lin"), s("log")]);
    // An edit through a color space the config owns doesn't refresh the cache, as upstream.
    call(&mut sc, log, "setDescription", vec![s("changed")]);
    call(&mut sc, c, "getProcessor", vec![s("lin"), s("log")]);
    sc
}

/// The iterators' methods: `len`, indexing (in range, past the end, negative), `iter`, `next`
/// and a list of what's left.
fn iterate(sc: &mut Script, it: usize) {
    sc.call(n("len"), vec![r(it)]);
    sc.call(ra(it, "__getitem__"), vec![i(0)]);
    sc.raises(ra(it, "__getitem__"), vec![i(99)]);
    sc.call(ra(it, "__getitem__"), vec![i(-1)]);
    sc.raises(ra(it, "__getitem__"), vec![s("a")]);
    sc.call(n("iter"), vec![r(it)]);
    sc.call(n("next"), vec![r(it)]);
    sc.call(n("list"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    sc.call(n("list"), vec![r(it)]);
    sc.call(n("type"), vec![r(it)]);
}

fn iterators_script() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    let iterators: Vec<(&str, Vec<Value>)> = vec![
        ("getEnvironmentVarNames", vec![]),
        ("getSearchPaths", vec![]),
        (
            "getColorSpaceNames",
            vec![
                n("OCIO.SEARCH_REFERENCE_SPACE_ALL"),
                n("OCIO.COLORSPACE_ALL"),
            ],
        ),
        (
            "getColorSpaces",
            vec![
                n("OCIO.SEARCH_REFERENCE_SPACE_SCENE"),
                n("OCIO.COLORSPACE_ALL"),
            ],
        ),
        ("getColorSpaceNames", vec![]),
        ("getColorSpaces", vec![]),
        ("getRoleNames", vec![]),
        ("getRoles", vec![]),
        ("getDisplays", vec![]),
        ("getDisplaysAll", vec![]),
        ("getSharedViews", vec![]),
        (
            "getVirtualDisplayViews",
            vec![n("OCIO.VIEW_DISPLAY_DEFINED")],
        ),
        ("getViews", vec![s("sRGB")]),
        ("getViews", vec![s("sRGB"), s("lin")]),
        ("getViews", vec![n("OCIO.VIEW_SHARED"), s("sRGB")]),
        ("getActiveDisplays", vec![]),
        ("getActiveViews", vec![]),
        ("getLookNames", vec![]),
        ("getLooks", vec![]),
        ("getViewTransformNames", vec![]),
        ("getViewTransforms", vec![]),
        ("getNamedTransformNames", vec![n("OCIO.NAMEDTRANSFORM_ALL")]),
        (
            "getNamedTransforms",
            vec![n("OCIO.NAMEDTRANSFORM_INACTIVE")],
        ),
        ("getNamedTransformNames", vec![]),
        ("getNamedTransforms", vec![]),
    ];
    let mut made = Vec::new();
    for (m, args) in iterators {
        let it = call(&mut sc, c, m, args);
        made.push(it);
        iterate(&mut sc, it);
    }
    // An iterator follows the config's changes, and keeps it alive.
    let it = call(&mut sc, c, "getLookNames", vec![]);
    let look = sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("name", s("look9"))],
        Expect::Value,
    );
    call(&mut sc, c, "addLook", vec![r(look)]);
    sc.call(n("list"), vec![r(it)]);
    let cs_it = call(&mut sc, c, "getColorSpaces", vec![]);
    sc.call(n("len"), vec![r(cs_it)]);
    let first = sc.call(ra(cs_it, "__getitem__"), vec![i(0)]);
    call(&mut sc, c, "getColorSpace", vec![s("disp")]);
    call(&mut sc, first, "getName", vec![]);
    // An empty config's.
    let raw = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    for m in [
        "getLooks",
        "getSharedViews",
        "getActiveViews",
        "getNamedTransforms",
    ] {
        let it = call(&mut sc, raw, m, vec![]);
        sc.call(n("len"), vec![r(it)]);
        sc.raises(ra(it, "__getitem__"), vec![i(0)]);
        sc.raises(n("next"), vec![r(it)]);
    }
    sc
}

fn current_config_script() -> Script {
    let mut sc = Script::new();
    let raw = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.call(n("OCIO.SetCurrentConfig"), vec![r(raw)]);
    let g = sc.call(n("OCIO.GetCurrentConfig"), vec![]);
    sc.call(n("OCIO.GetCurrentConfig"), vec![]);
    sc.call(n("operator.is_"), vec![r(g), r(raw)]);
    call(&mut sc, g, "setName", vec![s("current")]);
    let g2 = sc.call(n("OCIO.GetCurrentConfig"), vec![]);
    call(&mut sc, g2, "getName", vec![]);
    call(&mut sc, raw, "getName", vec![]);
    let c = config(&mut sc);
    sc.call(n("OCIO.SetCurrentConfig"), vec![r(c)]);
    call(&mut sc, c, "setName", vec![s("after")]);
    let g3 = sc.call(n("OCIO.GetCurrentConfig"), vec![]);
    call(&mut sc, g3, "getName", vec![]);
    let cs = call(&mut sc, g3, "getColorSpace", vec![s("lin")]);
    call(&mut sc, cs, "setDescription", vec![s("current's")]);
    let g4 = sc.call(n("OCIO.GetCurrentConfig"), vec![]);
    call(&mut sc, g4, "getColorSpace", vec![s("lin")]);
    call(&mut sc, g4, "serialize", vec![]);
    sc.raises(n("OCIO.SetCurrentConfig"), vec![i(1)]);
    sc.raises(n("OCIO.SetCurrentConfig"), vec![]);
    // A transform's processor through the current config: GroupTransform.write without one.
    let group = sc.call(n("OCIO.GroupTransform"), vec![]);
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(group, "appendTransform"), vec![r(m)]);
    sc.call_kw(
        ra(group, "write"),
        vec![],
        vec![("formatName", s("Color Transform Format"))],
        Expect::Value,
    );
    let back = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.call(n("OCIO.SetCurrentConfig"), vec![r(back)]);
    sc
}

/// `Config` behaves as the wheel's: creation, text, versions.
#[test]
fn creation_and_text_as_the_wheels() {
    compare(&[creation_script()]);
}

/// The environment, search paths and the current context, as the wheel's.
#[test]
fn environment_and_search_paths_as_the_wheels() {
    compare(&[environment_script()]);
}

/// Color spaces as the wheel's, the config's own objects included.
#[test]
fn color_spaces_as_the_wheels() {
    compare(&[color_spaces_script()]);
}

/// Roles, displays, views, the virtual display, active lists, luma, as the wheel's.
#[test]
fn displays_and_views_as_the_wheels() {
    compare(&[displays_script()]);
}

/// Looks, view transforms, named transforms, as the wheel's.
#[test]
fn looks_and_transforms_as_the_wheels() {
    compare(&[looks_script()]);
}

/// The file and viewing rules, the config's own, as the wheel's.
#[test]
fn rules_as_the_wheels() {
    compare(&[rules_script()]);
}

/// The 13 `getProcessor` overloads, their cache and errors, as the wheel's.
#[test]
fn processors_as_the_wheels() {
    compare(&[processors_script()]);
}

/// The 25 iterator classes, as the wheel's.
#[test]
fn iterators_as_the_wheels() {
    compare(&[iterators_script()]);
}

/// The current config, as the wheel's (the only script of this binary that sets it).
#[test]
fn the_current_config_as_the_wheels() {
    compare(&[current_config_script()]);
}

/// Where the wheel dereferences a null pointer or makes a `std::string` of it, the port's
/// error (U-123, U-124: the wheel's process dies, probed in subprocesses); the controls run.
/// In a turn of its own ([`common::turn`]): the controls set the current config, which
/// [`the_current_config_as_the_wheels`]'s script sets and reads.
#[test]
fn where_the_wheels_process_dies_the_port_raises() {
    let _turn = common::turn();
    Python::attach(|py| {
        for (code, entry) in [
            ("c.addColorSpace(None)", "U-124"),
            ("c.addLook(None)", "U-124"),
            ("c.addViewTransform(None)", "U-124"),
            ("c.setFileRules(None)", "U-124"),
            ("c.setViewingRules(None)", "U-124"),
            ("OCIO.Config.AreViewsEqual(None, c, 'a', 'b')", "U-124"),
            ("OCIO.Config.AreViewsEqual(c, None, 'a', 'b')", "U-124"),
            ("OCIO.Config.AreVirtualViewsEqual(c, None, 'b')", "U-124"),
            ("OCIO.SetCurrentConfig(None)", "U-124"),
            (
                "c.getProcessor(OCIO.NamedTransform(), OCIO.TRANSFORM_DIR_FORWARD)",
                "U-124",
            ),
            (
                "c.getProcessor(None, OCIO.NamedTransform(), OCIO.TRANSFORM_DIR_FORWARD)",
                "U-124",
            ),
            ("OCIO.Config.CreateFromBuiltinConfig(None)", "U-123"),
            ("c.getColorSpace(None)", "U-123"),
            ("c.getCanonicalName(None)", "U-123"),
            (
                "c.isColorSpaceLinear(None, OCIO.REFERENCE_SPACE_SCENE)",
                "U-123",
            ),
            ("c.isInactiveColorSpace(None)", "U-123"),
            ("c.setActiveDisplays(None)", "U-123"),
            ("c.setActiveViews(None)", "U-123"),
            ("c.archive(None)", "U-123"),
        ] {
            let globals = PyDict::new(py);
            let src = format!("import PyOpenColorIO as OCIO\nc = OCIO.Config.CreateRaw()\n{code}");
            let src = std::ffi::CString::new(src).unwrap();
            let err = py.run(&src, Some(&globals), None).expect_err(code);
            let text = err.to_string();
            assert!(
                text.starts_with("ValueError")
                    && text.contains(&format!("docs/improvements.md, {entry})")),
                "{code}: {text}"
            );
        }
        let src = std::ffi::CString::new(
            "import PyOpenColorIO as OCIO\nc = OCIO.Config.CreateRaw()\n\
             cs = OCIO.ColorSpace(name='a')\nc.addColorSpace(cs)\n\
             assert c.getColorSpace('a').getName() == 'a'\n\
             assert OCIO.Config.AreViewsEqual(c, c, 'x', 'y') is False\n\
             assert c.getCanonicalName('a') == 'a'\nc.setActiveDisplays('')\n\
             OCIO.SetCurrentConfig(c)\nOCIO.SetCurrentConfig(OCIO.Config.CreateRaw())",
        )
        .unwrap();
        py.run(&src, None, None).unwrap_or_else(|e| panic!("{e}"));
    });
}

/// Values the wheel keeps outside an enum's enumerators are the port's error (U-112); the same
/// steps succeed in the wheel.
#[test]
fn an_enum_value_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let mt = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let dir = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let space = sc.call(n("OCIO.ReferenceSpaceType"), vec![i(5)]);
    let mode = sc.call(n("OCIO.EnvironmentMode"), vec![i(9)]);
    let steps = [
        sc.call(ra(c, "getProcessor"), vec![r(mt), r(dir)]),
        sc.call(ra(c, "isColorSpaceLinear"), vec![s("raw"), r(space)]),
        sc.call(ra(c, "setEnvironmentMode"), vec![r(mode)]),
    ];
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    for &k in &steps {
        assert!(
            theirs["steps"][k].get("value").is_some(),
            "{}",
            theirs["steps"][k]
        );
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &ours["steps"][k]["raised"]["message"],
        ))
        .unwrap();
        assert!(message.contains("U-112"), "{}", ours["steps"][k]);
    }
}

/// Runs `sc` on both sides: each step of `port_errors` succeeds in the wheel and raises the
/// port's RuntimeError naming its register entry; every other step is the wheel's, each with
/// the outcome the script expects of the wheel.
fn compare_except(sc: &Script, port_errors: &[(usize, &str)]) {
    let theirs = common::wheel(&[sc.json()]).remove(0);
    let ours = common::port(&sc.json());
    let (theirs, ours) = (
        theirs["steps"].as_array().unwrap(),
        ours["steps"].as_array().unwrap(),
    );
    assert_eq!(theirs.len(), sc.steps.len());
    assert_eq!(ours.len(), sc.steps.len());
    assert!(!port_errors.is_empty());
    for (k, (a, b)) in ours.iter().zip(theirs).enumerate() {
        assert_eq!(
            b.get("raised").is_some(),
            sc.expect[k] == Expect::Raised,
            "step {k}: the wheel's outcome is not the expected one: {b}"
        );
        if let Some((_, entry)) = port_errors.iter().find(|(j, _)| *j == k) {
            assert!(b.get("value").is_some(), "step {k}: {b}");
            assert_eq!(
                a["raised"]["type"], "builtins.RuntimeError",
                "step {k}: {a}"
            );
            let message =
                String::from_utf8(ocio_testkit::oracle_values::bytes(&a["raised"]["message"]))
                    .unwrap();
            assert!(
                message.ends_with(&format!("(docs/improvements.md, {entry})")),
                "step {k}: {message}"
            );
        } else {
            // The wheel's results of those steps are among the objects a later step's can be.
            let mut b = b.clone();
            if let Some(same) = b.get_mut("same_as").and_then(Value::as_array_mut) {
                same.retain(|j| {
                    !port_errors
                        .iter()
                        .any(|(e, _)| Some(*e as u64) == j.as_u64())
                });
            }
            assert_eq!(a, &b, "step {k}: {}", sc.steps[k]);
        }
    }
}

/// The calls on a config whose own object can't be written back (a group of its color space
/// holding itself, U-111, or a null transform, U-110): owner decision 2026-10-22.
fn unwritable_script(null: bool) -> (Script, Vec<(usize, &'static str)>) {
    let entry = if null { "U-110" } else { "U-111" };
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let g0 = sc.call(n("OCIO.GroupTransform"), vec![]);
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(g0, "appendTransform"), vec![r(m)]);
    let to_ref = n("OCIO.COLORSPACE_DIR_TO_REFERENCE");
    for name in ["a", "b"] {
        let cs = sc.call_kw(
            n("OCIO.ColorSpace"),
            vec![],
            vec![("name", s(name))],
            Expect::Value,
        );
        if name == "a" {
            sc.call(ra(cs, "setTransform"), vec![r(g0), to_ref.clone()]);
        }
        call(&mut sc, c, "addColorSpace", vec![r(cs)]);
    }
    let look = sc.call_kw(
        n("OCIO.Look"),
        vec![],
        vec![("name", s("l"))],
        Expect::Value,
    );
    call(&mut sc, c, "addLook", vec![r(look)]);
    call(&mut sc, c, "getCacheID", vec![]);
    // The config's objects, held before, at and after the one that breaks.
    let b = call(&mut sc, c, "getColorSpace", vec![s("b")]);
    let a = call(&mut sc, c, "getColorSpace", vec![s("a")]);
    let raw = call(&mut sc, c, "getColorSpace", vec![s("raw")]);
    let lk = call(&mut sc, c, "getLook", vec![s("l")]);
    let g = call(&mut sc, a, "getTransform", vec![to_ref.clone()]);
    sc.call(n("type"), vec![r(g)]);
    if null {
        sc.call(ra(g, "appendTransform"), vec![none()]);
    } else {
        sc.call(ra(g, "appendTransform"), vec![r(g)]);
    }
    // Every call on the config raises, while the object stays as it is (the wheel's calls that
    // don't walk the color space's transforms succeed).
    let names = call(&mut sc, c, "getColorSpaceNames", vec![]);
    let mut errors = vec![sc.call(n("list"), vec![r(names)])];
    for (m, args) in [
        ("getCacheID", vec![]),
        ("getDescription", vec![]),
        ("getColorSpace", vec![s("b")]),
        ("getLook", vec![s("l")]),
        ("getSearchPath", vec![]),
        ("__repr__", vec![]),
    ] {
        errors.push(call(&mut sc, c, m, args));
    }
    // Changes through the objects meanwhile, then the repair.
    call(&mut sc, b, "setFamily", vec![s("fam_after")]);
    call(&mut sc, lk, "setDescription", vec![s("look's")]);
    call(&mut sc, raw, "setDescription", vec![s("raw's")]);
    let m2 = sc.call(n("OCIO.MatrixTransform"), vec![]);
    call(&mut sc, a, "setTransform", vec![r(m2), to_ref.clone()]);
    // The config's objects are the same objects, with the changes.
    for (name, held) in [("b", b), ("a", a), ("raw", raw)] {
        let cs = call(&mut sc, c, "getColorSpace", vec![s(name)]);
        sc.call(n("operator.is_"), vec![r(cs), r(held)]);
    }
    let back = call(&mut sc, c, "getLook", vec![s("l")]);
    sc.call(n("operator.is_"), vec![r(back), r(lk)]);
    let b2 = call(&mut sc, c, "getColorSpace", vec![s("b")]);
    call(&mut sc, b2, "getFamily", vec![]);
    let raw2 = call(&mut sc, c, "getColorSpace", vec![s("raw")]);
    call(&mut sc, raw2, "getDescription", vec![]);
    call(&mut sc, back, "getDescription", vec![]);
    let a2 = call(&mut sc, c, "getColorSpace", vec![s("a")]);
    let t = call(&mut sc, a2, "getTransform", vec![to_ref]);
    sc.call(n("type"), vec![r(t)]);
    call(&mut sc, c, "getCacheID", vec![]);
    let names = call(&mut sc, c, "getColorSpaceNames", vec![]);
    sc.call(n("list"), vec![r(names)]);
    call(&mut sc, c, "serialize", vec![]);
    (sc, errors.into_iter().map(|k| (k, entry)).collect())
}

/// A config whose own object can't be written back: every call on it raises U-111's or
/// U-110's error, where the wheel's succeed (owner decision 2026-10-22); once Python repairs
/// the object, the config's objects are the same Python objects, with what changed through
/// them meanwhile, and every other step is the wheel's.
#[test]
fn an_object_that_cant_be_written_back() {
    for null in [false, true] {
        let (sc, errors) = unwritable_script(null);
        compare_except(&sc, &errors);
    }
}

/// Arguments whose casters run Python code that uses the config (an `__index__` or `__bool__`
/// that is one of its bound methods: not a descriptor, it's called without the instance): the
/// wheel loads them before the call, so the config is free to use then.
fn argument_callbacks_script() -> Script {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let class = |sc: &mut Script, name: &str, slot: &str, method: &str| {
        let ty = sc.call(
            n("type"),
            vec![
                s(name),
                common::script::t(vec![]),
                d(vec![
                    (s("__module__"), s("probe")),
                    (s(slot), ra(c, method)),
                ]),
            ],
        );
        sc.call(r(ty), vec![])
    };
    let major = class(&mut sc, "Major", "__index__", "getMajorVersion");
    let strict = class(&mut sc, "Strict", "__bool__", "isStrictParsingEnabled");
    call(&mut sc, c, "setMinorVersion", vec![r(major)]);
    call(&mut sc, c, "getMinorVersion", vec![]);
    call(&mut sc, c, "setVersion", vec![r(major), r(major)]);
    call(&mut sc, c, "getMinorVersion", vec![]);
    call(
        &mut sc,
        c,
        "setDefaultLumaCoefs",
        vec![l(vec![r(major), f(0.5), r(major)])],
    );
    call(&mut sc, c, "getDefaultLumaCoefs", vec![]);
    call(&mut sc, c, "setStrictParsingEnabled", vec![r(strict)]);
    call(&mut sc, c, "isStrictParsingEnabled", vec![]);
    call(&mut sc, c, "setDisplayTemporary", vec![s("x"), r(strict)]);
    let names = call(&mut sc, c, "getColorSpaceNames", vec![]);
    sc.raises(ra(names, "__getitem__"), vec![r(major)]);
    sc
}

/// Python code an argument's caster runs may use the config, as in the wheel.
#[test]
fn arguments_load_before_the_config_is_locked() {
    compare(&[argument_callbacks_script()]);
}
