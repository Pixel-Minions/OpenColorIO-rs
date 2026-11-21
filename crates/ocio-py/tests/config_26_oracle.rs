// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config methods, statics, iterator, enum and submodule the 2.6.0 binding adds
//! (PyConfig.cpp:340-343, 360-363, 381-390, 431-443, 463-468, 500-511, 528-541, 1345-1366;
//! PyTypes.cpp:864-870; apphelpers/PyConfigCompatibilityHelpers.cpp @ v2.6.0) against the
//! wheel, through the same step scripts on both sides: results, `None` for each `const char *`
//! (the library's null pointer, which each of these answers without dereferencing it), names
//! cut by a NUL, keywords, the aliases' caster (a `str` and `None` refused), the iterator's
//! protocol and index errors, the statics' null configs, and the submodule's `.none(false)`
//! and enum parameter.
//!
//! A `ConfigCompatibility` value other than 0 (`OCIO.ConfigCompatibility(1)`) is outside the
//! enum's values, which the wheel stores and the port refuses (U-112): not compared here.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, compare, i, l, n, none, r, ra, s, t};

const BUILTIN: &str = "ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5";

/// A config of version 2.6 with a data space, a scene space with an alias, a display space
/// with an alias, a display whose view has aliases (one holding a NUL) and a shared view.
fn config(sc: &mut Script) -> usize {
    let c = sc.call(n("OCIO.Config"), vec![]);
    sc.call(ra(c, "setVersion"), vec![i(2), i(6)]);
    sc.call(ra(c, "setName"), vec![s("My Studio")]);
    let raw = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("raw")), ("isData", b(true))],
        Expect::Value,
    );
    sc.call(ra(c, "addColorSpace"), vec![r(raw)]);
    let lin = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("lin")), ("aliases", l(vec![s("linear")]))],
        Expect::Value,
    );
    sc.call(ra(c, "addColorSpace"), vec![r(lin)]);
    let dcs = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![
            ("referenceSpace", n("OCIO.REFERENCE_SPACE_DISPLAY")),
            ("name", s("sRGB - Display")),
            ("aliases", l(vec![s("sRGB")])),
            ("description", s("d")),
        ],
        Expect::Value,
    );
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(
        ra(dcs, "setTransform"),
        vec![r(m), n("OCIO.COLORSPACE_DIR_FROM_REFERENCE")],
    );
    sc.call(ra(c, "addColorSpace"), vec![r(dcs)]);
    sc.call_kw(
        ra(c, "addDisplayView"),
        vec![s("sRGB - Display"), s("v")],
        vec![
            ("viewTransform", s("")),
            ("displayColorSpaceName", s("sRGB - Display")),
            ("looks", s("")),
            ("ruleName", s("")),
            ("description", s("")),
            ("aliases", l(vec![s("v_old"), s("a\u{0}b")])),
        ],
        Expect::Value,
    );
    sc.call_kw(
        ra(c, "addSharedView"),
        vec![s("sh"), s(""), s("raw")],
        vec![("aliases", t(vec![s("s1"), s("s2")]))],
        Expect::Value,
    );
    sc.call(
        ra(c, "addDisplaySharedView"),
        vec![s("sRGB - Display"), s("sh")],
    );
    c
}

fn config_methods() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    // generateLocalIDForColorSpace and findColorSpaceForID.
    sc.call(ra(c, "generateLocalIDForColorSpace"), vec![s("lin")]);
    sc.call(ra(c, "generateLocalIDForColorSpace"), vec![s("lin\u{0}x")]);
    for name in [none(), s(""), s("nope")] {
        sc.raises(ra(c, "generateLocalIDForColorSpace"), vec![name]);
    }
    for id in [
        s("lin"),
        none(),
        s(""),
        s("my_studio:local:linear"),
        s("x:lin"),
        s("local:lin"),
        s("other:local:lin"),
    ] {
        let found = sc.call(ra(c, "findColorSpaceForID"), vec![id]);
        sc.call(n("repr"), vec![r(found)]);
    }
    // The display and view names.
    for display in [s("sRGB"), none(), s("sRGB - Display"), s("sRGB\u{0}x")] {
        sc.call(ra(c, "getCanonicalDisplayName"), vec![display.clone()]);
        sc.call(ra(c, "getDisplayDescription"), vec![display]);
    }
    for (display, view) in [
        (s("sRGB - Display"), s("v_old")),
        (none(), none()),
        (s("sRGB - Display"), s("v\u{0}zz")),
        (s("sRGB - Display"), s("SH")),
        (s(""), s("sh")),
        (s("sRGB"), s("v_old")),
    ] {
        sc.call(
            ra(c, "getCanonicalViewName"),
            vec![display.clone(), view.clone()],
        );
        sc.call(
            ra(c, "getResolvedDisplayViewColorSpaceName"),
            vec![display, view],
        );
    }
    for (display, view, alias) in [
        (s("sRGB - Display"), s("v"), s("V_OLD")),
        (none(), s("sh"), s("s1")),
        (s(""), s("sh"), none()),
        (s("sRGB - Display"), s("v"), s("a")),
        (s("sRGB - Display"), s("sh"), s("s2")),
        (s("nope"), s("v"), s("v_old")),
    ] {
        sc.call(ra(c, "hasDisplayViewAlias"), vec![display, view, alias]);
    }
    // use_display_aliases, and the names it lets through.
    sc.call(ra(c, "getUseDisplayAliases"), vec![]);
    sc.call(ra(c, "setUseDisplayAliases"), vec![b(true)]);
    sc.call(ra(c, "getUseDisplayAliases"), vec![]);
    sc.call(ra(c, "getCanonicalDisplayName"), vec![s("sRGB")]);
    sc.call(ra(c, "getDisplayDescription"), vec![s("sRGB")]);
    sc.call(
        ra(c, "getResolvedDisplayViewColorSpaceName"),
        vec![s("sRGB"), s("v_old")],
    );
    sc.call(ra(c, "setUseDisplayAliases"), vec![i(0)]);
    sc.call(ra(c, "getUseDisplayAliases"), vec![]);
    sc.call(ra(c, "setUseDisplayAliases"), vec![none()]);
    sc.call_kw(
        ra(c, "setUseDisplayAliases"),
        vec![],
        vec![("enabled", b(true))],
        Expect::Value,
    );
    // The aliases' caster and the views' checks.
    for aliases in [s("ab"), none()] {
        sc.call_kw(
            ra(c, "addDisplayView"),
            vec![s("d2"), s("v"), s(""), s("raw")],
            vec![("aliases", aliases)],
            Expect::Raised,
        );
    }
    sc.call_kw(
        ra(c, "addSharedView"),
        vec![],
        vec![
            ("view", s("sh2")),
            ("viewTransformName", s("")),
            ("colorSpaceName", s("raw")),
            ("aliases", l(vec![s("x")])),
        ],
        Expect::Value,
    );
    sc.raises(
        ra(c, "addDisplayView"),
        vec![
            s("sRGB - Display"),
            s("w"),
            s(""),
            s("raw"),
            s(""),
            s(""),
            s(""),
            l(vec![s("v_old")]),
        ],
    );
    sc.raises(
        ra(c, "addSharedView"),
        vec![
            s("sh3"),
            s(""),
            s("raw"),
            s(""),
            s(""),
            s(""),
            l(vec![s("s1")]),
        ],
    );
    sc.call(ra(c, "serialize"), vec![]);
    sc
}

fn alias_iterator() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    for (display, view) in [
        (s("sRGB - Display"), s("v")),
        (s(""), s("sh")),
        (s("sRGB - Display\u{0}x"), s("v")),
        (s("sRGB - Display"), s("nope")),
    ] {
        let it = sc.call(ra(c, "getDisplayViewAliases"), vec![display, view]);
        sc.call(n("list"), vec![r(it)]);
    }
    let it = sc.call(
        ra(c, "getDisplayViewAliases"),
        vec![s("sRGB - Display"), s("v")],
    );
    sc.call(n("len"), vec![r(it)]);
    sc.call(ra(it, "__getitem__"), vec![i(1)]);
    sc.call(ra(it, "__getitem__"), vec![i(-1)]);
    sc.raises(ra(it, "__getitem__"), vec![i(5)]);
    sc.call(n("next"), vec![r(it)]);
    sc.call(n("next"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    sc.call(ra(it, "__iter__"), vec![]);
    sc.raises(ra(c, "getDisplayViewAliases"), vec![none(), s("v")]);
    sc.call_kw(
        ra(c, "getDisplayViewAliases"),
        vec![],
        vec![("display", s("")), ("view", s("sh"))],
        Expect::Value,
    );
    sc.call(n("repr"), vec![n("OCIO.Config.DisplayViewAliasIterator")]);
    sc
}

fn statics_and_helpers() -> Script {
    let mut sc = Script::new();
    let c = config(&mut sc);
    let bc = sc.call(n("OCIO.Config.CreateFromFile"), vec![s(BUILTIN)]);
    let locate = n("OCIO.Config.LocateBuiltinColorSpace");
    sc.raises(locate.clone(), vec![none(), s("lin"), r(bc)]);
    sc.raises(locate.clone(), vec![r(c), s("lin"), none()]);
    sc.raises(locate.clone(), vec![r(bc), none(), r(bc)]);
    sc.raises(locate.clone(), vec![r(bc), s(""), r(bc)]);
    sc.raises(locate.clone(), vec![r(bc), s("\u{0}x"), r(bc)]);
    sc.raises(locate.clone(), vec![r(bc), s("nope\u{0}x"), r(bc)]);
    sc.call(locate.clone(), vec![r(bc), s("ACEScg"), r(bc)]);
    sc.call(locate.clone(), vec![r(bc), s("lin_ap1\u{0}x"), r(bc)]);
    sc.call_kw(
        locate.clone(),
        vec![],
        vec![
            ("srcConfig", r(bc)),
            ("srcColorSpaceName", s("lin_ap1")),
            ("builtinConfig", r(bc)),
        ],
        Expect::Value,
    );
    sc.call(locate.clone(), vec![r(c), s("raw"), r(bc)]);
    sc.raises(locate.clone(), vec![r(c), s("lin"), r(bc)]);
    sc.call(locate.clone(), vec![r(bc), s("sRGB - Display"), r(bc)]);
    // The submodule.
    let check = n("OCIO.ConfigCompatibilityHelpers.CheckCompatibility");
    sc.call(
        check.clone(),
        vec![r(c), n("OCIO.CONFIG_HDR_DISPLAY_SUPPORT_26")],
    );
    sc.call(
        check.clone(),
        vec![
            r(bc),
            n("OCIO.ConfigCompatibility.CONFIG_HDR_DISPLAY_SUPPORT_26"),
        ],
    );
    sc.raises(
        check.clone(),
        vec![none(), n("OCIO.CONFIG_HDR_DISPLAY_SUPPORT_26")],
    );
    sc.raises(check.clone(), vec![r(bc), i(0)]);
    sc.raises(check.clone(), vec![r(bc), none()]);
    sc.call_kw(
        check,
        vec![],
        vec![
            ("config", r(bc)),
            ("compatibility", n("OCIO.CONFIG_HDR_DISPLAY_SUPPORT_26")),
        ],
        Expect::Value,
    );
    sc.call(
        n("repr"),
        vec![n("OCIO.ConfigCompatibility.CONFIG_HDR_DISPLAY_SUPPORT_26")],
    );
    sc.call(n("int"), vec![n("OCIO.CONFIG_HDR_DISPLAY_SUPPORT_26")]);
    sc.call(
        n("repr"),
        vec![n("OCIO.ConfigCompatibilityHelpers.__name__")],
    );
    sc
}

/// The 2.6.0 config methods, iterator, statics and submodule, as the wheel's.
#[test]
fn the_26_config_bindings_behave_as_the_wheels() {
    compare(&[config_methods(), alias_iterator(), statics_and_helpers()]);
}
