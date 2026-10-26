// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! PyMergeConfigs.cpp @ v2.5.2: the entire binding surface, all parameter fields and
//! strategies, shared parameter/config identity, editable copies, checked null pointers,
//! OCIOM chains and in-memory merges, compared through identical scripts against the wheel.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, d, i, n, none, r, ra, s};
use serde_json::Value;

const GETTERS: &[&str] = &[
    "getBaseConfigName",
    "getInputConfigName",
    "getOutputName",
    "getDefaultStrategy",
    "getInputFamilyPrefix",
    "getBaseFamilyPrefix",
    "isInputFirst",
    "isErrorOnConflict",
    "isAvoidDuplicates",
    "isAdjustInputReferenceSpace",
    "getName",
    "getDescription",
    "getNumEnvironmentVars",
    "getSearchPath",
    "getActiveDisplays",
    "getActiveViews",
    "getInactiveColorSpaces",
    "getRoles",
    "getFileRules",
    "getDisplayViews",
    "getViewTransforms",
    "getLooks",
    "getColorspaces",
    "getNamedTransforms",
    "__repr__",
];
const STRATEGIES: &[&str] = &[
    "STRATEGY_PREFER_INPUT",
    "STRATEGY_PREFER_BASE",
    "STRATEGY_INPUT_ONLY",
    "STRATEGY_BASE_ONLY",
    "STRATEGY_REMOVE",
    "STRATEGY_UNSPECIFIED",
];

#[test]
fn the_merge_classes_and_helpers_have_the_wheels_surface() {
    let wheel = common::wheel_surface();
    for name in [
        "ConfigMergingParameters",
        "ConfigMerger",
        "ConfigMergingHelpers",
    ] {
        let mut theirs = common::wheel_member(&wheel, name);
        assert!(
            theirs["members"].as_array().is_some_and(|m| !m.is_empty()),
            "{name}"
        );
        // An embedded module has no extension-module __file__, as in apphelpers_oracle.
        if let Some(names) = theirs
            .get_mut("runtime_names")
            .and_then(Value::as_array_mut)
        {
            let before = names.len();
            names.retain(|name| name != "__file__");
            assert_eq!(names.len(), before - 1);
        }
        assert_eq!(common::port_member(name), theirs, "{name}");
    }
}

#[test]
fn every_parameter_and_its_copy_behave_as_the_wheels() {
    let mut sc = Script::new();
    sc.raises(n("OCIO.ConfigMergingParameters"), vec![]);
    let p = sc.call(n("OCIO.ConfigMergingParameters.Create"), vec![]);
    sc.eval(r(p), GETTERS);
    for (setter, getter) in [
        ("setBaseConfigName", "getBaseConfigName"),
        ("setInputConfigName", "getInputConfigName"),
        ("setOutputName", "getOutputName"),
        ("setInputFamilyPrefix", "getInputFamilyPrefix"),
        ("setBaseFamilyPrefix", "getBaseFamilyPrefix"),
        ("setName", "getName"),
        ("setDescription", "getDescription"),
        ("setSearchPath", "getSearchPath"),
        ("setActiveDisplays", "getActiveDisplays"),
        ("setActiveViews", "getActiveViews"),
        ("setInactiveColorSpaces", "getInactiveColorSpaces"),
    ] {
        for value in [s("first:second"), by(b"nul\0suffix"), s("\u{00e9} value")] {
            sc.call(ra(p, setter), vec![value]);
            sc.call(ra(p, getter), vec![]);
        }
        if ["setSearchPath", "setInactiveColorSpaces"].contains(&setter) {
            sc.call(ra(p, setter), vec![none()]);
            sc.call(ra(p, getter), vec![]);
        }
        sc.raises(ra(p, setter), vec![i(4)]);
    }
    for (setter, getter) in [
        ("setInputFirst", "isInputFirst"),
        ("setErrorOnConflict", "isErrorOnConflict"),
        ("setAvoidDuplicates", "isAvoidDuplicates"),
        (
            "setAdjustInputReferenceSpace",
            "isAdjustInputReferenceSpace",
        ),
    ] {
        for value in [b(false), b(true), i(0), i(5), none()] {
            sc.call(ra(p, setter), vec![value]);
            sc.call(ra(p, getter), vec![]);
        }
    }
    for (setter, getter) in [
        ("setDefaultStrategy", "getDefaultStrategy"),
        ("setRoles", "getRoles"),
        ("setFileRules", "getFileRules"),
        ("setDisplayViews", "getDisplayViews"),
        ("setViewTransforms", "getViewTransforms"),
        ("setLooks", "getLooks"),
        ("setColorspaces", "getColorspaces"),
        ("setNamedTransforms", "getNamedTransforms"),
    ] {
        for strategy in STRATEGIES {
            sc.call(
                ra(p, setter),
                vec![n(&format!("OCIO.ConfigMergingParameters.{strategy}"))],
            );
            sc.call(ra(p, getter), vec![]);
            sc.repr(r(p));
        }
        sc.raises(ra(p, setter), vec![i(1)]);
        sc.raises(ra(p, setter), vec![none()]);
    }
    sc.call(ra(p, "addEnvironmentVar"), vec![s("Z"), s("last")]);
    sc.call(ra(p, "addEnvironmentVar"), vec![s("A"), s("first")]);
    for index in [-1, 0, 1, 2] {
        sc.call(ra(p, "getEnvironmentVar"), vec![i(index)]);
        sc.call(ra(p, "getEnvironmentVarValue"), vec![i(index)]);
    }
    sc.call(ra(p, "addEnvironmentVar"), vec![s("A"), none()]);
    sc.call(ra(p, "addSearchPath"), vec![s("more:path")]);
    sc.eval(r(p), GETTERS);
    let copy = sc.call(n("copy.deepcopy"), vec![r(p)]);
    sc.call(n("operator.is_"), vec![r(copy), r(p)]);
    sc.call(ra(p, "setName"), vec![s("only original")]);
    sc.call(ra(p, "setSearchPath"), vec![s("shared override")]);
    sc.eval(r(copy), GETTERS);
    sc.eval(r(p), GETTERS);
    sc.raises(ra(p, "__deepcopy__"), vec![none()]);
    sc.call(ra(p, "__deepcopy__"), vec![d(vec![])]);
    compare(&[sc]);
}

/// Upstream's MergeConfigsTest.py and MergeConfigsHelpers_tests.cpp use this OCIOM chain.
fn chain_file() -> String {
    ocio_testkit::paths::upstream_dir()
        .join("tests/data/files/configs/mergeconfigs/merged2/merged.ociom")
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn mergers_share_parameters_and_outputs_as_the_wheels() {
    let mut sc = Script::new();
    sc.raises(n("OCIO.ConfigMerger"), vec![]);
    let m = sc.call(n("OCIO.ConfigMerger.Create"), vec![]);
    sc.eval(
        r(m),
        &[
            "getNumSearchPaths",
            "getWorkingDir",
            "getNumConfigMergingParameters",
            "getMajorVersion",
            "getMinorVersion",
            "getNumMergedConfigs",
            "getMergedConfig",
            "serialize",
            "__repr__",
        ],
    );
    for index in [-1, 0, 1] {
        sc.call(ra(m, "getSearchPath"), vec![i(index)]);
        sc.call(ra(m, "getParams"), vec![i(index)]);
        sc.call(ra(m, "getMergedConfig"), vec![i(index)]);
    }
    sc.call(ra(m, "setSearchPath"), vec![s("one:two")]);
    sc.call(ra(m, "addSearchPath"), vec![s("three")]);
    sc.call(ra(m, "setWorkingDir"), vec![s("working")]);
    sc.call(ra(m, "setVersion"), vec![i(2), i(3)]);
    let p = sc.call(n("OCIO.ConfigMergingParameters.Create"), vec![]);
    sc.call(ra(m, "addParams"), vec![r(p)]);
    sc.call(ra(m, "addParams"), vec![r(p)]);
    let got = sc.call(ra(m, "getParams"), vec![i(0)]);
    let again = sc.call(ra(m, "getParams"), vec![i(1)]);
    sc.call(n("operator.is_"), vec![r(got), r(p)]);
    sc.call(n("operator.is_"), vec![r(got), r(again)]);
    sc.call(ra(p, "setBaseConfigName"), vec![s("base")]);
    sc.repr(r(m));
    let copy = sc.call(n("copy.deepcopy"), vec![r(m)]);
    let copied_params = sc.call(ra(copy, "getParams"), vec![i(0)]);
    sc.call(n("operator.is_"), vec![r(copied_params), r(p)]);
    sc.call(ra(p, "setSearchPath"), vec![s("shared")]);
    sc.repr(r(copy));
    sc.raises(n("OCIO.ConfigMerger.CreateFromFile"), vec![none()]);
    sc.raises(n("OCIO.ConfigMerger.CreateFromFile"), vec![s("")]);
    let loaded = sc.call(
        n("OCIO.ConfigMerger.CreateFromFile"),
        vec![s(&chain_file())],
    );
    let result = sc.call(ra(loaded, "mergeConfigs"), vec![]);
    sc.eval(r(result), &["getNumMergedConfigs", "serialize", "__repr__"]);
    let last = sc.call(ra(result, "getMergedConfig"), vec![]);
    let indexed = sc.call(ra(result, "getMergedConfig"), vec![i(1)]);
    sc.call(n("operator.is_"), vec![r(last), r(indexed)]);
    sc.call(
        ra(last, "setDescription"),
        vec![s("changed through Python")],
    );
    let copied = sc.call(n("copy.deepcopy"), vec![r(result)]);
    let c = sc.call(ra(copied, "getMergedConfig"), vec![]);
    sc.eval(r(c), &["getDescription", "serialize"]);
    let same = sc.call(ra(result, "getMergedConfig"), vec![]);
    sc.call(n("operator.is_"), vec![r(same), r(last)]);
    compare(&[sc]);
}

#[test]
fn in_memory_merges_and_checked_nulls_are_the_wheels() {
    let mut sc = Script::new();
    let p = sc.call(n("OCIO.ConfigMergingParameters.Create"), vec![]);
    let base = sc.call(
        n("OCIO.Config.CreateFromBuiltinConfig"),
        vec![s("cg-config-latest")],
    );
    let input = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    for args in [
        vec![r(p), none(), r(input)],
        vec![r(p), r(base), none()],
        vec![none(), r(base), r(input)],
    ] {
        sc.raises(n("OCIO.ConfigMergingHelpers.MergeConfigs"), args);
    }
    for args in [
        vec![r(p), none(), none()],
        vec![none(), none(), none()],
        vec![r(p), r(base), none()],
    ] {
        sc.raises(n("OCIO.ConfigMergingHelpers.MergeColorSpace"), args);
    }
    let cs = sc.call_kw(
        n("OCIO.ColorSpace"),
        vec![],
        vec![("name", s("my_cs"))],
        Expect::Value,
    );
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_XYZ_TO_xyY")],
    );
    sc.call(
        ra(cs, "setTransform"),
        vec![r(ff), n("OCIO.COLORSPACE_DIR_TO_REFERENCE")],
    );
    let merged = sc.call(
        n("OCIO.ConfigMergingHelpers.MergeColorSpace"),
        vec![r(p), r(base), r(cs)],
    );
    sc.eval(r(merged), &["serialize"]);
    sc.raises(
        n("OCIO.ConfigMergingHelpers.MergeConfigs"),
        vec![r(p), i(3), r(input)],
    );
    compare(&[sc]);
}

#[test]
fn null_string_setters_raise_where_the_wheel_process_dies() {
    let mut scripts = Vec::new();
    for (class, setter, getter, entry) in [
        (
            "ConfigMergingParameters",
            "setBaseConfigName",
            "getBaseConfigName",
            "U-221",
        ),
        (
            "ConfigMergingParameters",
            "setInputConfigName",
            "getInputConfigName",
            "U-221",
        ),
        (
            "ConfigMergingParameters",
            "setOutputName",
            "getOutputName",
            "U-221",
        ),
        ("ConfigMergingParameters", "setName", "getName", "U-221"),
        (
            "ConfigMergingParameters",
            "setDescription",
            "getDescription",
            "U-221",
        ),
        (
            "ConfigMergingParameters",
            "setInputFamilyPrefix",
            "getInputFamilyPrefix",
            "U-221",
        ),
        (
            "ConfigMergingParameters",
            "setBaseFamilyPrefix",
            "getBaseFamilyPrefix",
            "U-221",
        ),
        ("ConfigMerger", "setWorkingDir", "getWorkingDir", "U-221"),
        (
            "ConfigMergingParameters",
            "setActiveDisplays",
            "getActiveDisplays",
            "U-123",
        ),
        (
            "ConfigMergingParameters",
            "setActiveViews",
            "getActiveViews",
            "U-123",
        ),
    ] {
        let mut sc = Script::new();
        let p = sc.call(n(&format!("OCIO.{class}.Create")), vec![]);
        sc.call(ra(p, setter), vec![s("before")]);
        sc.call(ra(p, getter), vec![]);
        sc.raises(ra(p, setter), vec![none()]);
        scripts.push((sc, getter, entry));
    }
    let json: Vec<Value> = scripts.iter().map(|(script, _, _)| script.json()).collect();
    let result = ocio_testkit::Oracle::get()
        .call("py_eval", serde_json::json!({"scripts": json}), &[])
        .result;
    let reports = result["scripts"].as_array().unwrap();
    assert_eq!(reports.len(), scripts.len());
    for ((mut sc, getter, entry), report) in scripts.into_iter().zip(reports) {
        let crashed = report["crashed"]["returncode"]
            .as_i64()
            .is_some_and(|code| code != 0);
        let linux_string_exception = cfg!(not(windows))
            && entry == "U-123"
            && report["steps"]
                .as_array()
                .and_then(|s| s.last())
                .is_some_and(|s| s.get("raised").is_some());
        assert!(crashed || linux_string_exception, "{report}");
        let mut before = sc.json();
        before["steps"].as_array_mut().unwrap().pop();
        let wheel = common::wheel(&[before]);
        let expected = &wheel[0]["steps"][2]["value"];
        assert!(expected.is_object(), "{wheel:?}");
        sc.call(ra(0, getter), vec![]);
        let port = common::port(&sc.json());
        let steps = port["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 5);
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &steps[3]["raised"]["message"],
        ))
        .unwrap();
        assert_eq!(steps[3]["raised"]["type"], "builtins.ValueError", "{port}");
        assert!(message.contains(entry), "{port}");
        assert_eq!(&steps[4]["value"], expected, "{port}");
    }
}

/// A logger reenters the merger, changes its current params and appends a merge while it is
/// executing. Upstream rereads the source list and the shared params after every callback.
#[test]
fn a_logging_callback_can_read_and_change_the_running_merger() {
    let mut sc = Script::new();
    let path = std::path::PathBuf::from(chain_file());
    let directory = path.parent().unwrap().to_string_lossy().replace('\\', "/");
    let m = sc.call(n("OCIO.ConfigMerger.Create"), vec![]);
    let p = sc.call(n("OCIO.ConfigMergingParameters.Create"), vec![]);
    sc.call(
        ra(p, "setBaseConfigName"),
        vec![s(&format!("{directory}/base.ocio"))],
    );
    sc.call(
        ra(p, "setInputConfigName"),
        vec![s(&format!("{directory}/input.ocio"))],
    );
    sc.call(ra(p, "setAdjustInputReferenceSpace"), vec![b(false)]);
    sc.call(ra(m, "addParams"), vec![r(p)]);
    // py_eval deliberately allows dunder attribute access. Use the already-imported copy
    // module's builtins to define a callback, without adding an oracle command.
    let builtins = sc.call(
        n("copy._reconstruct.__globals__.get"),
        vec![s("__builtins__")],
    );
    let exec = sc.call(ra(builtins, "get"), vec![s("exec")]);
    let globals = sc.eval(d(vec![(s("merger"), r(m)), (s("params"), r(p))]), &[]);
    sc.call(r(exec), vec![s("seen = []\ndef logger(message):\n    seen.append((merger.getNumConfigMergingParameters(), merger.getParams(0) is params, type(repr(merger)).__name__))\n    params.setInputFamilyPrefix('callback')\n    if len(seen) == 1:\n        extra = type(params).Create()\n        extra.setBaseConfigName('cg-config-latest')\n        extra.setInputConfigName('cg-config-latest')\n        merger.addParams(extra)\n"), r(globals)]);
    let logger = sc.call(ra(globals, "get"), vec![s("logger")]);
    sc.call(n("OCIO.SetLoggingFunction"), vec![r(logger)]);
    let result = sc.call(ra(m, "mergeConfigs"), vec![]);
    let prefix = common::wheel(&[sc.json()]);
    assert!(
        prefix[0]["steps"][result].get("value").is_some(),
        "{}",
        prefix[0]["steps"][result]
    );
    sc.eval(
        r(result),
        &[
            "getNumMergedConfigs",
            "getNumConfigMergingParameters",
            "serialize",
        ],
    );
    let first = sc.call(ra(result, "getMergedConfig"), vec![i(0)]);
    sc.eval(r(first), &["serialize"]);
    sc.call(ra(globals, "get"), vec![s("seen")]);
    compare(&[sc]);
}

/// C++'s unfixed MergeStrategies enum holds 0..7, but only 0..5 are enumerators.
/// The owner approved rejecting 6/7 under D-15 (2026-10-25), before changing the parameters.
#[test]
fn unnamed_merge_strategies_are_d15_and_leave_parameters_unchanged() {
    let mut cases = Vec::new();
    for value in [6, 7] {
        for (setter, getter) in [
            ("setDefaultStrategy", "getDefaultStrategy"),
            ("setRoles", "getRoles"),
            ("setFileRules", "getFileRules"),
            ("setDisplayViews", "getDisplayViews"),
            ("setViewTransforms", "getViewTransforms"),
            ("setLooks", "getLooks"),
            ("setColorspaces", "getColorspaces"),
            ("setNamedTransforms", "getNamedTransforms"),
        ] {
            let mut sc = Script::new();
            let params = sc.call(n("OCIO.ConfigMergingParameters.Create"), vec![]);
            let strategy = sc.call(
                n("OCIO.ConfigMergingParameters.MergeStrategies"),
                vec![i(value)],
            );
            let before = sc.call(ra(params, getter), vec![]);
            let set = sc.call(ra(params, setter), vec![r(strategy)]);
            let after = sc.call(ra(params, getter), vec![]);
            cases.push((sc, value, before, set, after));
        }
    }
    assert_eq!(cases.len(), 16);
    let reports = common::wheel(&cases.iter().map(|case| case.0.json()).collect::<Vec<_>>());
    assert_eq!(reports.len(), cases.len());
    for ((sc, value, before, set, after), wheel) in cases.into_iter().zip(reports) {
        let port = common::port(&sc.json());
        let theirs = wheel["steps"].as_array().unwrap();
        let ours = port["steps"].as_array().unwrap();
        assert_eq!(theirs.len(), 5);
        assert_eq!(ours.len(), 5);
        assert!(
            theirs.iter().all(|step| step.get("value").is_some()),
            "{wheel}"
        );
        assert_eq!(&ours[..set], &theirs[..set]);
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &ours[set]["raised"]["message"],
        ))
        .unwrap();
        assert_eq!(ours[set]["raised"]["type"], "builtins.ValueError");
        assert_eq!(
            message,
            format!(
                "PyOpenColorIO.PyOpenColorIO.ConfigMergingParameters.MergeStrategies({value}): a value of the enum that is none of its enumerators, which the wheel stores and the port's library can't hold (docs/deviations.md, D-15)"
            )
        );
        assert_eq!(theirs[after]["value"]["enum"][1], serde_json::json!(value));
        assert_eq!(ours[after], theirs[before]);
    }
}
