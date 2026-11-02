// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Copying a config reads the environment, and can throw (docs/improvements.md, I-134):
//! `Config::createEditableCopy` (Config.cpp:1352-1357 @ v2.5.2) makes a new config with
//! `Config::Create`, whose `Impl` constructor reads `OCIO_ACTIVE_DISPLAYS` and
//! `OCIO_ACTIVE_VIEWS`, then overwrites what it read with the copied config's. So every call that
//! copies a config raises the environment's error once one of those lists is malformed, whatever
//! the config holds, and a copy made under a valid environment keeps the config's lists.
//!
//! The scripts change the process's environment: in the wheel each runs in a process of its own,
//! and here they run in this test binary, one at a time, and take their variables out again.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, compare, n, r, ra, s};
use serde_json::json;

const DISPLAYS: &str = "OCIO_ACTIVE_DISPLAYS";
const VIEWS: &str = "OCIO_ACTIVE_VIEWS";
const INACTIVE: &str = "OCIO_INACTIVE_COLORSPACES";

/// A config with two displays of two views, and two color spaces.
const CONFIG: &str = "ocio_profile_version: 2.5\n\nroles:\n  default: raw\n\nfile_rules:\n  - \
     !<Rule> {name: Default, colorspace: raw}\n\ndisplays:\n  d1:\n    - !<View> {name: v1, \
     colorspace: raw}\n    - !<View> {name: v2, colorspace: raw}\n  d2:\n    - !<View> {name: v1, \
     colorspace: raw}\n    - !<View> {name: v2, colorspace: raw}\n\ncolorspaces:\n  - \
     !<ColorSpace> {name: raw, isdata: true}\n  - !<ColorSpace> {name: other, isdata: true}\n";

/// `os.environ`, through the builtins the `copy` module holds: the scripts' names don't list
/// `os`. These steps' results aren't compared: the environment is each process's own.
fn environ(sc: &mut Script) -> usize {
    let import = sc.call(
        n("operator.getitem"),
        vec![n("copy.__builtins__"), s("__import__")],
    );
    let os = sc.call(r(import), vec![s("os")]);
    let environ = sc.eval(ra(os, "environ"), &[]);
    for step in [import, os, environ] {
        sc.uncompare(step);
    }
    environ
}

fn set(sc: &mut Script, environ: usize, name: &str, value: &str) {
    sc.step(
        json!({"setitem": r(environ), "index": s(name), "value": s(value)}),
        Expect::Value,
    );
}

fn unset(sc: &mut Script, environ: usize, name: &str) {
    sc.call(ra(environ, "pop"), vec![s(name)]);
}

fn config(sc: &mut Script) -> usize {
    sc.call(n("OCIO.Config.CreateFromStream"), vec![s(CONFIG)])
}

/// Reads of a config that show which environment lists it holds.
fn lists(sc: &mut Script, config: usize) {
    for getter in ["getActiveDisplays", "getActiveViews", "getCacheID"] {
        sc.call(ra(config, getter), vec![]);
    }
    let displays = sc.call(ra(config, "getDisplays"), vec![]);
    sc.call(n("list"), vec![r(displays)]);
    let names = sc.call(ra(config, "getColorSpaceNames"), vec![]);
    sc.call(n("list"), vec![r(names)]);
}

/// Each call of the binding that copies a config, under the malformed `value` of `name`: each
/// raises what `Config()` raises and leaves what it would have set as it was, and each goes
/// through once the variable is gone.
fn malformed(name: &str, value: &str) -> Script {
    let mut sc = Script::new();
    let environ = environ(&mut sc);
    let config = config(&mut sc);
    let baker = sc.call(n("OCIO.Baker"), vec![]);
    let params = sc.call(n("OCIO.ConfigMergingParameters.Create"), vec![]);
    let raw = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.call(n("OCIO.SetCurrentConfig"), vec![r(raw)]);
    set(&mut sc, environ, name, value);
    sc.raises(n("OCIO.Config"), vec![]);
    sc.raises(n("copy.deepcopy"), vec![r(config)]);
    sc.raises(ra(config, "__deepcopy__"), vec![json!({"dict": []})]);
    sc.raises(n("OCIO.SetCurrentConfig"), vec![r(config)]);
    sc.raises(ra(baker, "setConfig"), vec![r(config)]);
    sc.raises(
        n("OCIO.Baker"),
        vec![r(config), s("spi3d"), s("raw"), s("raw")],
    );
    sc.raises(
        n("OCIO.ConfigMergingHelpers.MergeConfigs"),
        vec![r(params), r(config), r(config)],
    );
    unset(&mut sc, environ, name);
    sc.call(ra(baker, "getConfig"), vec![]);
    let current = sc.call(n("OCIO.GetCurrentConfig"), vec![]);
    sc.call(ra(current, "getCacheID"), vec![]);
    let copy = sc.call(n("copy.deepcopy"), vec![r(config)]);
    lists(&mut sc, copy);
    sc.call(n("OCIO.SetCurrentConfig"), vec![r(config)]);
    let current = sc.call(n("OCIO.GetCurrentConfig"), vec![]);
    sc.call(ra(current, "getCacheID"), vec![]);
    sc.call(ra(baker, "setConfig"), vec![r(config)]);
    let held = sc.call(ra(baker, "getConfig"), vec![]);
    sc.call(ra(held, "getCacheID"), vec![]);
    // Past the copy, the merge's own error: this config has no reference role.
    sc.raises(
        n("OCIO.ConfigMergingHelpers.MergeConfigs"),
        vec![r(params), r(config), r(config)],
    );
    sc
}

#[test]
fn a_copy_raises_what_a_new_config_raises() {
    let scripts: Vec<Script> = [DISPLAYS, VIEWS]
        .into_iter()
        .flat_map(|name| {
            ["\"open, c", "a, \"open, c", "\"open:c"]
                .into_iter()
                .map(move |value| malformed(name, value))
        })
        .collect();
    assert_eq!(scripts.len(), 6);
    compare(&scripts);
}

/// A copy made under another valid environment holds the lists of the config it copies, not what
/// `Config::Create` read on the way, and a list that doesn't split raises nothing.
#[test]
fn a_copy_keeps_the_configs_lists() {
    let mut sc = Script::new();
    let environ = environ(&mut sc);
    set(&mut sc, environ, DISPLAYS, "d1");
    set(&mut sc, environ, VIEWS, "v1");
    set(&mut sc, environ, INACTIVE, "other");
    let config = config(&mut sc);
    lists(&mut sc, config);
    set(&mut sc, environ, DISPLAYS, "d2");
    set(&mut sc, environ, VIEWS, "v2");
    set(&mut sc, environ, INACTIVE, "raw");
    let copy = sc.call(n("copy.deepcopy"), vec![r(config)]);
    lists(&mut sc, copy);
    set(&mut sc, environ, DISPLAYS, "\"open");
    let copy = sc.call(n("copy.deepcopy"), vec![r(config)]);
    lists(&mut sc, copy);
    for name in [DISPLAYS, VIEWS, INACTIVE] {
        unset(&mut sc, environ, name);
    }
    compare(&[sc]);
}
