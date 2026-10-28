// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! PySystemMonitors.cpp/.h @ v2.5.2: exact surface, tuples, singleton reads,
//! int indexing/errors, strict calls and the iterator's independent wrapper lifetime.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare_with, f, i, n, none, r, ra, s};
use ocio_testkit::Oracle;
use serde_json::{Value, json};

/// Observe this machine's current monitors live, without reusing a response from before a
/// monitor/profile change. This is the existing py_eval command and its usual report format.
fn wheel_live(scripts: &[Script]) -> Vec<Value> {
    let scripts: Vec<_> = scripts.iter().map(Script::json).collect();
    let result = Oracle::get()
        .call_uncached("py_eval", json!({ "scripts": scripts }), &[])
        .result;
    let reports = result["scripts"].as_array().expect("live script reports");
    assert_eq!(reports.len(), scripts.len());
    reports
        .iter()
        .map(|report| {
            assert!(report.get("crashed").is_none(), "{report}");
            let mut report = report.clone();
            report.as_object_mut().unwrap().remove("interpreter");
            report
        })
        .collect()
}

fn compare_live(scripts: &[Script]) {
    compare_with(scripts, &wheel_live(scripts));
}

#[test]
fn the_monitor_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    let theirs = common::wheel_member(&report, "SystemMonitors");
    // PySystemMonitors.cpp:22-57 @ v2.5.2 declares these parent and nested members.
    // py_surface_walk._members encodes each name as bytes, including the nested class.
    let named = |member: &Value, name: &str| {
        ocio_testkit::oracle_values::bytes(&member["name"]) == name.as_bytes()
    };
    let parent = theirs["members"].as_array().expect("wheel parent members");
    for name in ["MonitorIterator", "__init__", "getMonitors"] {
        assert!(parent.iter().any(|member| named(member, name)), "{name}");
    }
    let iterator = parent
        .iter()
        .find(|member| named(member, "MonitorIterator"))
        .expect("wheel nested MonitorIterator");
    let members = iterator["members"]
        .as_array()
        .expect("wheel iterator members");
    for name in ["__len__", "__getitem__", "__iter__", "__next__"] {
        assert!(members.iter().any(|member| named(member, name)), "{name}");
    }
    assert_eq!(common::port_member("SystemMonitors"), theirs);
}

/// Discover the machine's actual list, rather than assume particular Windows monitor names.
/// Linux's empty list is required; Windows must exercise successful tuples on this machine.
fn monitor_count() -> usize {
    let mut sc = Script::new();
    let monitors = sc.call(n("OCIO.SystemMonitors"), vec![]);
    let it = sc.call(ra(monitors, "getMonitors"), vec![]);
    let len = sc.call(n("len"), vec![r(it)]);
    let report = wheel_live(&[sc]);
    let count = report[0]["steps"][len]["value"]["int"]
        .as_str()
        .expect("the wheel's integer length")
        .parse()
        .expect("monitor count");
    #[cfg(windows)]
    assert!(
        count > 0,
        "P9-4 requires this Windows machine to list a monitor"
    );
    #[cfg(not(windows))]
    assert_eq!(count, 0, "SystemMonitor.cpp's Linux list is empty");
    count
}

#[test]
fn the_monitor_iterator_behaves_as_the_wheels() {
    let count = monitor_count();
    let mut sc = Script::new();
    let owner = sc.call(n("OCIO.SystemMonitors"), vec![]);
    let other = sc.call(n("OCIO.SystemMonitors"), vec![]);
    sc.call(n("operator.is_"), vec![r(owner), r(other)]);
    let it = sc.call(ra(owner, "getMonitors"), vec![]);
    let fresh = sc.call(ra(owner, "getMonitors"), vec![]);
    sc.call(n("operator.is_"), vec![r(it), r(fresh)]);
    sc.call(n("len"), vec![r(it)]);
    sc.call(n("list"), vec![r(fresh)]);
    let from_other = sc.call(ra(other, "getMonitors"), vec![]);
    sc.call(n("list"), vec![r(from_other)]);
    for index in 0..count {
        sc.call(ra(it, "__getitem__"), vec![i(index as i128)]);
    }
    for index in [
        -1,
        i32::MIN as i128,
        count as i128,
        count as i128 + 1,
        i32::MAX as i128,
    ] {
        sc.raises(ra(it, "__getitem__"), vec![i(index)]);
    }
    for value in [
        i(i32::MAX as i128 + 1),
        i(i32::MIN as i128 - 1),
        f(0.0),
        s("0"),
        by(b"0"),
        none(),
    ] {
        sc.raises(ra(it, "__getitem__"), vec![value]);
    }
    let same = sc.call(ra(it, "__iter__"), vec![]);
    sc.call(n("operator.is_"), vec![r(same), r(it)]);
    for _ in 0..count {
        sc.call(ra(it, "__next__"), vec![]);
        sc.call(n("len"), vec![r(it)]);
    }
    sc.raises(ra(it, "__next__"), vec![]);
    sc.raises(ra(it, "__next__"), vec![]);
    sc.call(n("list"), vec![r(it)]);
    if count > 0 {
        // Indexing is independent of the cursor, including after exhaustion.
        sc.call(ra(it, "__getitem__"), vec![i(0)]);
    }
    compare_live(&[sc]);
}

#[test]
fn strict_calls_and_a_temporary_parent_are_the_wheels() {
    let count = monitor_count();
    let mut sc = Script::new();
    sc.raises(n("OCIO.SystemMonitors"), vec![i(1)]);
    sc.call_kw(
        n("OCIO.SystemMonitors"),
        vec![],
        vec![("unused", i(1))],
        Expect::Raised,
    );
    sc.raises(n("OCIO.SystemMonitors.MonitorIterator"), vec![]);
    let owner = sc.call(n("OCIO.SystemMonitors"), vec![]);
    sc.raises(ra(owner, "getMonitors"), vec![i(0)]);
    sc.call_kw(
        ra(owner, "getMonitors"),
        vec![],
        vec![("unused", i(0))],
        Expect::Raised,
    );
    let context = sc.call(n("OCIO.Context"), vec![]);
    for value in [none(), i(0), r(context)] {
        sc.raises(n("OCIO.SystemMonitors.getMonitors"), vec![value.clone()]);
        for method in ["__len__", "__iter__", "__next__"] {
            sc.raises(
                n(&format!("OCIO.SystemMonitors.MonitorIterator.{method}")),
                vec![value.clone()],
            );
        }
        sc.raises(
            n("OCIO.SystemMonitors.MonitorIterator.__getitem__"),
            vec![value, i(0)],
        );
    }
    let it = sc.call(ra(owner, "getMonitors"), vec![]);
    for method in ["__len__", "__iter__", "__next__"] {
        sc.raises(ra(it, method), vec![i(0)]);
    }
    sc.raises(ra(it, "__getitem__"), vec![]);
    sc.raises(ra(it, "__getitem__"), vec![i(0), i(1)]);
    sc.call_kw(
        ra(it, "__getitem__"),
        vec![],
        vec![("i", i(0))],
        Expect::Raised,
    );
    let item_outcome = if count > 0 {
        Expect::Value
    } else {
        Expect::Raised
    };
    sc.call_kw(
        ra(it, "__getitem__"),
        vec![],
        vec![("arg0", i(0))],
        Expect::Raised,
    );
    sc.step(
        json!({"call":ra(it,"__getitem__"),"args":[b(false)]}),
        item_outcome,
    );

    // The step interpreter keeps results alive. A factory instead makes the parent a
    // temporary that dies before the returned iterator is inspected (PyIterator<T> copies T).
    let builtins = sc.call(
        n("copy._reconstruct.__globals__.get"),
        vec![s("__builtins__")],
    );
    let exec = sc.call(ra(builtins, "get"), vec![s("exec")]);
    let globals = sc.eval(
        json!({"dict":[[s("Monitors"),n("OCIO.SystemMonitors")]]}),
        &[],
    );
    sc.call(
        r(exec),
        vec![
            s("def make():\n    return Monitors().getMonitors()\n"),
            r(globals),
        ],
    );
    let make = sc.call(ra(globals, "get"), vec![s("make")]);
    let temporary = sc.call(r(make), vec![]);
    sc.call(n("len"), vec![r(temporary)]);
    sc.call(n("list"), vec![r(temporary)]);
    sc.raises(ra(temporary, "__next__"), vec![]);
    compare_live(&[sc]);
}

// Exact Config_tests.cpp:8690-8749 and9067-9109 @ v2.5.2 inputs.
const MONITOR_CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}
  - !<View> {name: sview2, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}
    - !<Views> [sview1]

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<View> {name: Film, view_transform: display_vt, display_colorspace: <USE_DISPLAY_NAME>}
  - !<Views> [sview2]

active_displays: []
active_views: []

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: true
    allocation: uniform
"#;

const ACTIVE_MONITOR_CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<Views> [sview1]

active_displays: [sRGB]
active_views: [view]

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

/// Observe the whole serialized config and active display/view lists at each boundary.
fn monitor_config_snapshot(sc: &mut Script, cfg: usize) -> usize {
    sc.call(ra(cfg, "getActiveDisplays"), vec![]);
    sc.call(ra(cfg, "getActiveViews"), vec![]);
    let displays = sc.call(ra(cfg, "getDisplays"), vec![]);
    sc.call(n("list"), vec![r(displays)]);
    let views = sc.call(ra(cfg, "getViews"), vec![s("sRGB")]);
    sc.call(n("list"), vec![r(views)]);
    sc.call(ra(cfg, "serialize"), vec![])
}

/// Config_tests.cpp:8686-9061,9063-9147 and PyConfig.cpp:548-577 @ v2.5.2:
/// actual monitor names, conversion/keyword dispatch and temporary/permanent display state.
#[test]
fn configs_from_real_monitors_match_the_wheels() {
    let count = monitor_count();
    let mut scripts = Vec::new();
    let mut validation_checks = Vec::new();
    let mut checkpoint_count = 0;
    for (text, validation_reason) in [
        (
            MONITOR_CONFIG,
            "can not use '<USE_DISPLAY_NAME>' keyword for the color space name.",
        ),
        (
            ACTIVE_MONITOR_CONFIG,
            "The search_path must not be an empty string if there are FileTransforms.",
        ),
    ] {
        for monitor_index in 0..count {
            // Each route starts from the exact upstream config, not a previous mutation.
            // Names come from each implementation's live iterator; that tuple is compared too.
            for route in 0..4 {
                let mut sc = Script::new();
                let monitors = sc.call(n("OCIO.SystemMonitors"), vec![]);
                let iterator = sc.call(ra(monitors, "getMonitors"), vec![]);
                let pair = sc.call(ra(iterator, "__getitem__"), vec![i(monitor_index as i128)]);
                let name = sc.call(n("operator.getitem"), vec![r(pair), i(0)]);
                let name = match route {
                    2 => sc.call(ra(name, "encode"), vec![s("utf-8")]),
                    3 => sc.call(n("operator.add"), vec![r(name), s("\0ignored suffix")]),
                    _ => name,
                };
                let cfg = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(text)]);
                sc.call(ra(cfg, "validate"), vec![]);
                monitor_config_snapshot(&mut sc, cfg);
                let index = if route == 1 {
                    sc.call_kw(
                        ra(cfg, "instantiateDisplayFromMonitorName"),
                        vec![],
                        vec![("monitorName", r(name))],
                        Expect::Value,
                    )
                } else {
                    sc.call(ra(cfg, "instantiateDisplayFromMonitorName"), vec![r(name)])
                };
                // Compare this successful call before the state trace dereferences its index.
                // The checkpoint copies the complete prefix and all report metadata; its
                // final result is deliberately not consumed by another expression.
                scripts.push(Script {
                    steps: sc.steps.clone(),
                    expect: sc.expect.clone(),
                    unrepr: sc.unrepr.clone(),
                    unlog: sc.unlog.clone(),
                    uncompared: sc.uncompared.clone(),
                });
                checkpoint_count += 1;
                // This is the active-list index returned by instantiateDisplay. The Python
                // temporary APIs resolve the resulting display NAME among all displays.
                let displays = sc.call(ra(cfg, "getDisplays"), vec![]);
                let display = sc.call(n("operator.getitem"), vec![r(displays), r(index)]);
                let views = sc.call(ra(cfg, "getViews"), vec![r(display)]);
                sc.call(n("list"), vec![r(views)]);
                sc.call(ra(cfg, "isDisplayTemporary"), vec![r(display)]);
                let color_space = sc.call(ra(cfg, "getColorSpace"), vec![r(display)]);
                let profile = sc.call(
                    ra(color_space, "getTransform"),
                    vec![n("OCIO.COLORSPACE_DIR_FROM_REFERENCE")],
                );
                sc.call(ra(profile, "getSrc"), vec![]);
                monitor_config_snapshot(&mut sc, cfg);
                sc.call_kw(
                    ra(cfg, "setDisplayTemporary"),
                    vec![],
                    vec![("display", r(display)), ("isTemporary", b(false))],
                    Expect::Value,
                );
                sc.call(ra(cfg, "isDisplayTemporary"), vec![r(display)]);
                let serialized = monitor_config_snapshot(&mut sc, cfg);
                let permanent = sc.call(n("OCIO.Config.CreateFromStream"), vec![r(serialized)]);
                // I-452: copied Film hits the token guard (Config.cpp:612-621). I-453:
                // Raw-only ACTIVE instead reaches the empty-search-path guard after the
                // ICC FileTransform was added (Config.cpp:1974-2033 @ v2.5.2).
                let validation_step = sc.step(
                    json!({"call": ra(permanent, "validate"), "args": []}),
                    Expect::Raised,
                );
                sc.call(ra(permanent, "isDisplayTemporary"), vec![r(display)]);
                monitor_config_snapshot(&mut sc, permanent);
                sc.call(ra(cfg, "setDisplayTemporary"), vec![r(display), b(true)]);
                sc.call(ra(cfg, "isDisplayTemporary"), vec![r(display)]);
                monitor_config_snapshot(&mut sc, cfg);
                validation_checks.push((scripts.len(), validation_step, validation_reason));
                scripts.push(sc);
            }
        }
        // The core same-machine contract uses these missing/empty names. Wrong argument
        // types and None exercise the Python conversion/error path as well as the library.
        for name in [
            s("OCIO missing monitor"),
            s(""),
            by(b"OCIO missing monitor"),
            none(),
            i(0),
            f(0.0),
            b(false),
            json!({"list": []}),
        ] {
            let mut sc = Script::new();
            let cfg = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(text)]);
            sc.call(ra(cfg, "validate"), vec![]);
            monitor_config_snapshot(&mut sc, cfg);
            sc.raises(ra(cfg, "instantiateDisplayFromMonitorName"), vec![name]);
            monitor_config_snapshot(&mut sc, cfg);
            scripts.push(sc);
        }
        let mut sc = Script::new();
        let cfg = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(text)]);
        sc.call(ra(cfg, "validate"), vec![]);
        monitor_config_snapshot(&mut sc, cfg);
        sc.raises(ra(cfg, "instantiateDisplayFromMonitorName"), vec![]);
        monitor_config_snapshot(&mut sc, cfg);
        sc.raises(
            ra(cfg, "instantiateDisplayFromMonitorName"),
            vec![s("OCIO missing monitor"), i(0)],
        );
        monitor_config_snapshot(&mut sc, cfg);
        sc.call_kw(
            ra(cfg, "instantiateDisplayFromMonitorName"),
            vec![],
            vec![("monitorName", s("OCIO missing monitor"))],
            Expect::Raised,
        );
        monitor_config_snapshot(&mut sc, cfg);
        sc.call_kw(
            ra(cfg, "instantiateDisplayFromMonitorName"),
            vec![],
            vec![("monitor_name", s("OCIO missing monitor"))],
            Expect::Raised,
        );
        monitor_config_snapshot(&mut sc, cfg);
        sc.call_kw(
            ra(cfg, "instantiateDisplayFromMonitorName"),
            vec![s("OCIO missing monitor")],
            vec![("monitorName", s("OCIO missing monitor"))],
            Expect::Raised,
        );
        monitor_config_snapshot(&mut sc, cfg);
        sc.raises(
            n("OCIO.Config.instantiateDisplayFromMonitorName"),
            vec![none(), s("OCIO missing monitor")],
        );
        monitor_config_snapshot(&mut sc, cfg);
        scripts.push(sc);
    }
    let reports = wheel_live(&scripts);
    assert_eq!(checkpoint_count, 2 * 4 * count);
    assert_eq!(validation_checks.len(), 2 * 4 * count);
    assert_eq!(scripts.len(), 2 * (2 * 4 * count + 8 + 1));
    for (script, step, reason) in validation_checks {
        let raised = reports[script]["steps"][step]
            .get("raised")
            .expect("the wheel rejects this persisted config for the source-defined reason");
        let message = ocio_testkit::oracle_values::bytes(&raised["message"]);
        assert!(
            message
                .windows(reason.len())
                .any(|fragment| fragment == reason.as_bytes()),
            "script {script} step {step}: expected reason {reason:?}, got {raised}"
        );
    }
    // Keep one complete reference batch, but compare each port report before evaluating the
    // next script. A checkpoint can then show Value/Raised inequality directly instead of a
    // dependent state trace aborting on a reference to the unexpectedly raised call.
    for (script, report) in scripts.iter().zip(&reports) {
        compare_with(std::slice::from_ref(script), std::slice::from_ref(report));
    }
}
