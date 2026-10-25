// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `MixingColorSpaceManager` and `MixingSlider` against the wheel, through the oracle's
//! `object_calls`.
//!
//! - Generated sequences of a manager's calls over upstream's app helpers test config, the
//!   pipeline config of `legacy_viewing_pipeline_oracle.rs`, the raw config and every built-in
//!   config (some have a color picking role): the selections by index and by name (valid or
//!   not), refreshes with copies of the config whose color picking role names one of its color
//!   spaces (or aliases) or is set to `""`, the processors (cache ID, flags, metadata,
//!   `createGroupTransform()`, or the exception; the log; and the pixels of their CPU
//!   processors at extreme values, `common::app_helpers`), the mixing spaces and encodings,
//!   the selected indices, whether the space is perceptually uniform, the texts (`repr`), and
//!   the slider's edges and conversions.
//! - The slider's conversions over dense grids and specials (NaNs with payloads of their own,
//!   the infinities, the extremes of `f32`, subnormals, the break points), for every pair of a
//!   list of edges, in a linear and a perceptually uniform mixing space.
//! - A color picking role that names no color space: the wheel's process dies, the port
//!   returns an error (docs/improvements.md, U-152).
//!
//! The slider calls the C runtime's `log10f` and `powf`, whose results may depend on the CPU,
//! and the processors' kernels do: this target is in the `cpu-tests` alias.

mod common;

use std::sync::Arc;

use common::app_helpers::{
    DynamicSet, EXPOSURE_CONTRAST_GAMMA, Reached, Rng, apply_calls, capture,
    compare_processor_call, f, report, text,
};
use ocio::{BuiltinConfigRegistry, Config, MixingColorSpaceManager, Processor, TransformDirection};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::Response;
use ocio_testkit::oracle_values::{bytes, hex, log};
use serde_json::{Value, json};

/// The config of upstream's app helpers tests (tests/cpu/apphelpers/configs.data:5-173 @
/// v2.5.2).
const CATEGORY_TEST_CONFIG: &str = include_str!("data/apphelpers_category_test_config.ocio");

/// The config of the pipeline's paths (see `legacy_viewing_pipeline_oracle.rs`).
const PIPELINE_CONFIG: &str = include_str!("data/apphelpers_pipeline_config.ocio");

const ROLE: &str = "color_picking";

/// Where a config comes from, on both sides.
#[derive(Clone)]
enum Source {
    Yaml(&'static str),
    Builtin(String),
    Raw,
}

impl Source {
    fn port(&self) -> Arc<Config> {
        match self {
            Source::Yaml(text) => Config::create_from_stream(text.as_bytes()).unwrap(),
            Source::Builtin(name) => Config::create_from_builtin_config(name).unwrap(),
            Source::Raw => Config::create_raw().unwrap(),
        }
    }

    fn wheel(&self) -> Value {
        match self {
            Source::Yaml(text) => json!({"yaml": text}),
            Source::Builtin(name) => json!({"builtin": name}),
            Source::Raw => json!("raw"),
        }
    }
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

/// A float argument, as the binding takes it: a Python float the binding converts to a C
/// float (exact for the widened `f32`).
fn f32_arg(v: f32) -> Value {
    f(f64::from(v))
}

/// A NaN of `f32` with these bits.
fn nan(bits: u32) -> f32 {
    let v = f32::from_bits(bits);
    assert!(v.is_nan());
    v
}

// ---------------------------------------------------------------------------------------------
// The operations

/// One step of a case, on the manager (and its slider) of the case.
#[derive(Debug, Clone)]
enum Op {
    /// `setSelectedMixingSpaceIdx`.
    SpaceIdx(u64),
    /// `setSelectedMixingSpace`.
    Space(String),
    /// `setSelectedMixingEncodingIdx`.
    EncodingIdx(u64),
    /// `setSelectedMixingEncoding`.
    Encoding(String),
    /// `refresh` with the case's config (`None`), or with a copy of it whose color picking
    /// role is set to this name.
    Refresh(Option<String>),
    /// `getProcessor(working, display, view, direction)`.
    Processor(String, String, String, TransformDirection),
    /// `getSlider(min, max)`.
    Range(f32, f32),
    /// The slider's `setSliderMinEdge`.
    SetMin(f32),
    /// The slider's `setSliderMaxEdge`.
    SetMax(f32),
    /// The slider's `sliderToMixing` and `mixingToSlider` of the value.
    Convert(f32),
    /// The slider's edges (`getSliderMinEdge`, `getSliderMaxEdge`) and text.
    Slider,
    /// The manager's mixing spaces, encodings, selected indices, whether it is perceptually
    /// uniform, and its text.
    State,
}

/// What the port gave for one wheel call.
enum Port {
    /// A call that returns nothing, or raises.
    Unit(ocio::Result<()>),
    Usize(usize),
    Bool(bool),
    Names(Vec<Vec<u8>>),
    Text(ocio::Result<Vec<u8>>),
    Float(f32),
    /// A call whose result is only stored ("as"): it must return.
    Stored,
}

/// The wheel's call `results[at]` against the port's `port`; a problem is returned.
fn compare(wheel: &Value, port: &Port) -> Option<String> {
    let exception = wheel.get("exception");
    let result = wheel.get("result");
    let ok = match port {
        Port::Unit(Ok(())) => exception.is_none() && result == Some(&Value::Null),
        Port::Unit(Err(e)) | Port::Text(Err(e)) => {
            result.is_none() && exception.is_some_and(|x| bytes(&x["message"]) == e.what())
        }
        Port::Usize(n) => result.and_then(Value::as_u64) == Some(*n as u64),
        Port::Bool(b) => result.and_then(Value::as_bool) == Some(*b),
        Port::Names(names) => result.and_then(Value::as_array).is_some_and(|list| {
            list.len() == names.len()
                && list
                    .iter()
                    .zip(names)
                    .all(|(w, p)| w.as_str().map(str::as_bytes) == Some(p.as_slice()))
        }),
        Port::Text(Ok(t)) => result.and_then(Value::as_str).map(str::as_bytes) == Some(t),
        Port::Float(x) => result.and_then(|r| r["f64"].as_u64()) == Some(f64::from(*x).to_bits()),
        Port::Stored => exception.is_none() && result.is_some(),
    };
    let port_text = match port {
        Port::Unit(r) => format!("{:?}", r.as_ref().map_err(|e| e.message())),
        Port::Text(r) => format!(
            "{:?}",
            r.as_ref()
                .map(|t| String::from_utf8_lossy(t).into_owned())
                .map_err(|e| e.message())
        ),
        Port::Usize(n) => n.to_string(),
        Port::Bool(b) => b.to_string(),
        Port::Names(n) => format!("{:?}", n.iter().map(|s| text(s)).collect::<Vec<_>>()),
        Port::Float(x) => format!("{x:?} ({:#x})", f64::from(*x).to_bits()),
        Port::Stored => "stored".to_string(),
    };
    (!ok).then(|| format!("wheel {wheel}, port {port_text}"))
}

/// The dynamic properties of `proc`'s CPU processor, set away from their defaults before the
/// pixels are applied.
fn dynamic_sets(proc: &Processor) -> Vec<DynamicSet> {
    EXPOSURE_CONTRAST_GAMMA
        .iter()
        .copied()
        .filter(|(t, _, _)| proc.has_dynamic_property(*t))
        .collect()
}

/// A processor call: its index, the port's outcome and log, and its CPU processors' calls.
struct ProcCall {
    at: usize,
    made: ocio::Result<Arc<Processor>>,
    log: Vec<Vec<u8>>,
    applies: Vec<common::app_helpers::Applied>,
}

/// What a case expects: per wheel call (by index) the port's outcome and a label.
#[derive(Default)]
struct Plan {
    checks: Vec<(usize, Port, String)>,
    procs: Vec<(ProcCall, String)>,
    /// The indices of the calls whose log must be empty (the port's side never logs there).
    quiet: Vec<usize>,
}

/// Appends the wheel's calls of `ops` on the manager `m` and its slider `s` (made by the
/// caller) to `calls`, runs the same on `manager` (of the config `config`), and records what
/// the port gave in `plan`. `tag` makes the case's object names unique.
#[allow(clippy::too_many_arguments)]
fn run_ops(
    tag: &str,
    ops: &[Op],
    config: &Arc<Config>,
    manager: &mut MixingColorSpaceManager,
    m: &str,
    s: &str,
    calls: &mut Vec<Value>,
    plan: &mut Plan,
) {
    for (i, op) in ops.iter().enumerate() {
        let label = format!("{tag} op {i} {op:?}");
        match op {
            Op::SpaceIdx(idx) => {
                plan.checks.push((
                    calls.len(),
                    Port::Unit(manager.set_selected_mixing_space_idx(*idx as usize)),
                    label,
                ));
                calls.push(json!({"call": "setSelectedMixingSpaceIdx", "on": m, "args": [idx]}));
            }
            Op::Space(name) => {
                plan.checks.push((
                    calls.len(),
                    Port::Unit(manager.set_selected_mixing_space(name)),
                    label,
                ));
                calls.push(json!({"call": "setSelectedMixingSpace", "on": m, "args": [name]}));
            }
            Op::EncodingIdx(idx) => {
                plan.checks.push((
                    calls.len(),
                    Port::Unit(manager.set_selected_mixing_encoding_idx(*idx as usize)),
                    label,
                ));
                calls.push(json!({"call": "setSelectedMixingEncodingIdx", "on": m, "args": [idx]}));
            }
            Op::Encoding(name) => {
                plan.checks.push((
                    calls.len(),
                    Port::Unit(manager.set_selected_mixing_encoding(name)),
                    label,
                ));
                calls.push(json!({"call": "setSelectedMixingEncoding", "on": m, "args": [name]}));
            }
            Op::Refresh(role) => {
                let (cfg_ref, port_config) = match role {
                    None => ("config".to_string(), Arc::clone(config)),
                    Some(name) => {
                        let cfg = format!("{tag}_cfg{i}");
                        let mut copy = (**config).clone();
                        let set = copy.set_role(ROLE, Some(name.as_bytes()));
                        plan.checks
                            .push((calls.len() + 1, Port::Unit(set), label.clone()));
                        calls.push(json!({"copy": "config", "as": cfg}));
                        calls.push(json!({"call": "setRole", "on": cfg, "args": [ROLE, name]}));
                        (cfg, Arc::new(copy))
                    }
                };
                let (refreshed, port_log) = capture(|| manager.refresh(port_config));
                assert!(port_log.is_empty(), "{label}: {port_log:?}");
                plan.quiet.push(calls.len());
                plan.checks
                    .push((calls.len(), Port::Unit(refreshed), label));
                calls.push(json!({"call": "refresh", "on": m, "args": [{"ref": cfg_ref}]}));
            }
            Op::Processor(working, display, view, dir) => {
                let proc = format!("{tag}_proc{i}");
                let at = calls.len();
                calls.push(json!({"call": "getProcessor", "on": m,
                    "args": [working, display, view, {"enum": dir_name(*dir)}], "as": proc}));
                let (made, port_log) = capture(|| manager.processor(working, display, view, *dir));
                let applies = match &made {
                    Ok(p) => apply_calls(calls, &proc, p, &dynamic_sets(p)),
                    Err(_) => Vec::new(),
                };
                plan.procs.push((
                    ProcCall {
                        at,
                        made,
                        log: port_log,
                        applies,
                    },
                    label,
                ));
            }
            Op::Range(min, max) => {
                manager.slider_with_range(*min, *max);
                plan.quiet.push(calls.len());
                plan.checks.push((calls.len(), Port::Stored, label));
                calls.push(json!({"call": "getSlider", "on": m,
                    "args": [f32_arg(*min), f32_arg(*max)]}));
            }
            Op::SetMin(v) => {
                manager.slider().set_slider_min_edge(*v);
                plan.quiet.push(calls.len());
                plan.checks.push((calls.len(), Port::Unit(Ok(())), label));
                calls.push(json!({"call": "setSliderMinEdge", "on": s, "args": [f32_arg(*v)]}));
            }
            Op::SetMax(v) => {
                manager.slider().set_slider_max_edge(*v);
                plan.quiet.push(calls.len());
                plan.checks.push((calls.len(), Port::Unit(Ok(())), label));
                calls.push(json!({"call": "setSliderMaxEdge", "on": s, "args": [f32_arg(*v)]}));
            }
            Op::Convert(v) => {
                let slider = manager.slider();
                for (name, out) in [
                    ("sliderToMixing", slider.slider_to_mixing(*v)),
                    ("mixingToSlider", slider.mixing_to_slider(*v)),
                ] {
                    plan.quiet.push(calls.len());
                    plan.checks
                        .push((calls.len(), Port::Float(out), format!("{label} {name}")));
                    calls.push(json!({"call": name, "on": s, "args": [f32_arg(*v)]}));
                }
            }
            Op::Slider => {
                let slider = manager.slider();
                for (name, port) in [
                    ("getSliderMinEdge", Port::Float(slider.slider_min_edge())),
                    ("getSliderMaxEdge", Port::Float(slider.slider_max_edge())),
                    ("__repr__", Port::Text(Ok(slider.to_bytes()))),
                ] {
                    plan.quiet.push(calls.len());
                    plan.checks
                        .push((calls.len(), port, format!("{label} {name}")));
                    calls.push(json!({"call": name, "on": s}));
                }
            }
            Op::State => {
                let spaces: Vec<Vec<u8>> = (0..manager.num_mixing_spaces())
                    .map(|i| manager.mixing_space_ui_name(i).unwrap().to_vec())
                    .collect();
                let encodings: Vec<Vec<u8>> = (0..manager.num_mixing_encodings())
                    .map(|i| manager.mixing_encoding_name(i).unwrap().to_vec())
                    .collect();
                // Past the last index, an error (the binding's iterators check the index
                // first, so the wheel can't be asked for the message).
                assert!(
                    manager
                        .mixing_space_ui_name(manager.num_mixing_spaces())
                        .is_err()
                );
                assert!(
                    manager
                        .mixing_encoding_name(manager.num_mixing_encodings())
                        .is_err()
                );
                let (repr, repr_log) = capture(|| manager.to_bytes());
                assert!(repr_log.is_empty(), "{label}: {repr_log:?}");
                for (name, port) in [
                    ("getMixingSpaces", Port::Names(spaces)),
                    ("getMixingEncodings", Port::Names(encodings)),
                    (
                        "getSelectedMixingSpaceIdx",
                        Port::Usize(manager.selected_mixing_space_idx()),
                    ),
                    (
                        "getSelectedMixingEncodingIdx",
                        Port::Usize(manager.selected_mixing_encoding_idx()),
                    ),
                    (
                        "isPerceptuallyUniform",
                        Port::Bool(manager.is_perceptually_uniform()),
                    ),
                    ("__repr__", Port::Text(repr)),
                ] {
                    plan.quiet.push(calls.len());
                    plan.checks
                        .push((calls.len(), port, format!("{label} {name}")));
                    calls.push(json!({"call": name, "on": m}));
                }
            }
        }
    }
}

/// What a test's requests reached.
#[derive(Default)]
struct Totals {
    reached: Reached,
    /// Calls compared, those that raised on both sides, and NaN results compared.
    checks: usize,
    raised: usize,
    nans: usize,
    /// The repr texts of the managers with a color picking role.
    color_picking: usize,
}

/// Sends one request of `calls` on the config of `source` and compares it with `plan`; the
/// mismatches go to `failures`.
fn check_request(
    label: &str,
    source: &Source,
    calls: Vec<Value>,
    plan: Plan,
    totals: &mut Totals,
    failures: &mut Vec<String>,
) {
    let n = calls.len();
    let response: Response = Oracle::get().call(
        "object_calls",
        json!({"config": source.wheel(), "calls": calls}),
        &[],
    );
    let result = &response.result;
    assert_eq!(result["config"], Value::Null, "{label}: {result}");
    let results = result["calls"].as_array().expect("calls");
    assert_eq!(results.len(), n, "{label}");

    for at in &plan.quiet {
        let wheel_log = log(&results[*at]["log"]);
        if !wheel_log.is_empty() {
            failures.push(format!("{label}: call {at} logged {wheel_log:?}"));
        }
    }
    for (at, port, what) in &plan.checks {
        totals.checks += 1;
        if results[*at].get("exception").is_some() {
            totals.raised += 1;
        }
        if let Port::Float(x) = port
            && x.is_nan()
        {
            totals.nans += 1;
        }
        if let Port::Text(Ok(t)) = port
            && t.ends_with(b", colorPicking")
        {
            totals.color_picking += 1;
        }
        if let Some(problem) = compare(&results[*at], port) {
            failures.push(format!("{label} {what}: {problem}"));
        }
    }
    for (
        ProcCall {
            at,
            made,
            log: port_log,
            applies,
        },
        what,
    ) in plan.procs
    {
        let mut problems = Vec::new();
        compare_processor_call(
            &response,
            results,
            at,
            &made,
            &port_log,
            applies,
            &mut totals.reached,
            &mut problems,
        );
        if !problems.is_empty() {
            failures.push(format!("{label} {what}: {}", problems.join("\n    ")));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Generated manager cases

/// The names a config's cases take.
struct Tokens {
    /// Working spaces: the color spaces and roles, and names the config doesn't have.
    working: Vec<String>,
    /// (display, view) pairs, with ones the config doesn't have.
    display_views: Vec<(String, String)>,
    /// What the color picking role is set to: the config's color spaces and aliases, and `""`.
    picks: Vec<String>,
}

fn tokens(config: &Config) -> Tokens {
    let mut working: Vec<String> = (0..config.num_color_spaces())
        .map(|i| text(config.color_space_name_by_index(i)))
        .collect();
    let mut picks = working.clone();
    for i in 0..config.num_color_spaces() {
        let cs = config
            .color_space(config.color_space_name_by_index(i))
            .unwrap();
        for a in 0..cs.num_aliases() {
            picks.push(text(cs.alias(a)));
        }
    }
    picks.push(String::new());
    working.extend((0..config.num_roles()).map(|i| text(config.role_name(i))));
    working.push("no_such_space".to_string());
    working.push(String::new());
    let mut display_views = Vec::new();
    for d in 0..config.num_displays() {
        let display = text(config.display(d));
        for v in 0..config.num_views(&display) {
            display_views.push((display.clone(), text(config.view(&display, v))));
        }
        display_views.push((display.clone(), "no_such_view".to_string()));
    }
    display_views.push(("no_such_display".to_string(), "no_such_view".to_string()));
    display_views.push((String::new(), String::new()));
    Tokens {
        working,
        display_views,
        picks,
    }
}

/// The slider's edges and values the generated cases take.
fn slider_values() -> Vec<f32> {
    vec![
        0.0,
        -0.0,
        1.0,
        -0.2,
        5.0,
        0.18,
        0.5,
        0.1,
        3.0,
        -1.0,
        0.379232,
        0.801448,
        0.42426407,
        1e-40,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        nan(0x7fc0_0101),
        nan(0xffc0_0202),
        100.0,
    ]
}

fn gen_ops(tokens: &Tokens, rng: &mut Rng, n: usize) -> Vec<Op> {
    let values = slider_values();
    let space_names = [
        "Rendering Space",
        "Display Space",
        "display space",
        "DisplaySpace",
        "",
        "Display Space\u{0}trailing",
        "color_picking (lin_1)",
    ];
    let encoding_names = ["RGB", "HSV", "HS", "hsv", "", "HSV\u{0}trailing"];
    let indices = [0, 1, 0, 1, 2, 3, u64::MAX];
    let mut ops = vec![Op::State];
    for _ in 0..n {
        let op = match rng.below(14) {
            0 | 1 => Op::SpaceIdx(rng.pick(&indices)),
            2 => {
                let name = if rng.below(3) == 0 {
                    format!("{ROLE} ({})", rng.pick(&tokens.picks))
                } else {
                    rng.pick(&space_names).to_string()
                };
                Op::Space(name)
            }
            3 => Op::EncodingIdx(rng.pick(&indices)),
            4 => Op::Encoding(rng.pick(&encoding_names).to_string()),
            5 => Op::Refresh((rng.below(4) != 0).then(|| rng.pick(&tokens.picks))),
            6..=8 => {
                let (display, view) = rng.pick(&tokens.display_views);
                let dir = if rng.below(3) == 0 {
                    TransformDirection::Inverse
                } else {
                    TransformDirection::Forward
                };
                Op::Processor(rng.pick(&tokens.working), display, view, dir)
            }
            9 => Op::Range(rng.pick(&values), rng.pick(&values)),
            10 => {
                if rng.below(2) == 0 {
                    Op::SetMin(rng.pick(&values))
                } else {
                    Op::SetMax(rng.pick(&values))
                }
            }
            11 => Op::Slider,
            12 => Op::Convert(rng.pick(&values)),
            _ => Op::State,
        };
        ops.push(op);
    }
    ops.push(Op::Slider);
    ops.push(Op::State);
    ops
}

/// Runs `cases` (each a sequence of operations on a manager of its own) on the config of
/// `source`, `per_request` cases a request; the mismatches go to `failures`.
fn check_cases(
    test: &str,
    source: &Source,
    cases: &[Vec<Op>],
    per_request: usize,
    totals: &mut Totals,
    failures: &mut Vec<String>,
) {
    for (chunk_index, chunk) in cases.chunks(per_request).enumerate() {
        let label = format!("{test} #{chunk_index}");
        // A config per request, as the wheel's (its processor cache starts empty).
        let config = source.port();
        let mut calls = Vec::new();
        let mut plan = Plan::default();
        for (k, ops) in chunk.iter().enumerate() {
            let tag = format!("c{k}");
            let (m, s) = (format!("{tag}_m"), format!("{tag}_s"));
            let mut manager = MixingColorSpaceManager::new(Arc::clone(&config)).unwrap();
            plan.checks
                .push((calls.len(), Port::Stored, format!("{label} {tag} new")));
            calls.push(
                json!({"new": "MixingColorSpaceManager", "args": [{"ref": "config"}],
                "as": m}),
            );
            plan.checks.push((
                calls.len(),
                Port::Stored,
                format!("{label} {tag} getSlider"),
            ));
            calls.push(json!({"call": "getSlider", "on": m, "as": s}));
            run_ops(
                &format!("{label} {tag}"),
                ops,
                &config,
                &mut manager,
                &m,
                &s,
                &mut calls,
                &mut plan,
            );
        }
        check_request(&label, source, calls, plan, totals, failures);
    }
}

/// Generated cases on upstream's test config, the pipeline config and the raw config.
#[test]
fn managers_match_the_wheel() {
    let mut failures = Vec::new();
    let mut totals = Totals::default();
    for (test, source, seed, n) in [
        (
            "category",
            Source::Yaml(CATEGORY_TEST_CONFIG),
            0x31c0_0001,
            60,
        ),
        ("pipeline", Source::Yaml(PIPELINE_CONFIG), 0x31c0_0002, 60),
        ("raw", Source::Raw, 0x31c0_0003, 20),
    ] {
        let config = source.port();
        let tokens = tokens(&config);
        let mut rng = Rng(seed);
        let cases: Vec<Vec<Op>> = (0..n).map(|_| gen_ops(&tokens, &mut rng, 24)).collect();
        check_cases(test, &source, &cases, 10, &mut totals, &mut failures);
    }
    report("mixing_helpers_managers", &failures);

    let r = &totals.reached;
    assert!(
        r.made >= 300 && r.refused >= 50,
        "{} made, {} refused",
        r.made,
        r.refused
    );
    assert_eq!(r.pixels, 2 * r.made, "pixels compared");
    assert!(r.groups.len() >= 10, "{} group shapes", r.groups.len());
    for fragment in [
        "Color space 'no_such_space' could not be found.",
        "DisplayViewTransform: empty display name.",
    ] {
        assert!(
            r.messages.iter().any(|m| m.contains(fragment)),
            "no processor raised {fragment:?}: {:#?}",
            r.messages
        );
    }
    assert!(
        totals.checks >= 6_000 && totals.raised >= 400 && totals.nans >= 100,
        "{} checks, {} raised, {} NaNs",
        totals.checks,
        totals.raised,
        totals.nans
    );
    assert!(totals.color_picking >= 50, "{}", totals.color_picking);
}

/// Generated cases on every built-in config (some have a color picking role).
#[test]
fn builtin_config_managers_match_the_wheel() {
    let mut failures = Vec::new();
    let mut totals = Totals::default();
    let registry = BuiltinConfigRegistry::get();
    for i in 0..registry.num_builtin_configs() {
        let name = text(registry.builtin_config_name(i).unwrap());
        let source = Source::Builtin(name.clone());
        let config = source.port();
        let tokens = tokens(&config);
        let mut rng = Rng(0x31c0_0100 + i as u64);
        let cases: Vec<Vec<Op>> = (0..6).map(|_| gen_ops(&tokens, &mut rng, 16)).collect();
        check_cases(&name, &source, &cases, 6, &mut totals, &mut failures);
    }
    report("mixing_helpers_builtin", &failures);
    let r = &totals.reached;
    assert!(r.made >= 40, "{} made", r.made);
    assert_eq!(r.pixels, 2 * r.made, "pixels compared");
    assert!(totals.color_picking >= 20, "{}", totals.color_picking);
}

// ---------------------------------------------------------------------------------------------
// The slider's conversions

/// The edges of the slider grid: every ordered pair of them is a range.
fn edges() -> Vec<f32> {
    vec![
        0.0,
        -0.0,
        1.0,
        -0.2,
        5.0,
        0.18,
        0.5,
        0.505,
        0.42426407,
        100.0,
        1e-40,
        -1e-40,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        nan(0x7fc0_0011),
        nan(0xffc0_0022),
    ]
}

/// The special values every range converts: each with a NaN payload of its own.
fn specials() -> Vec<f32> {
    let mut v = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        0.1,
        3.0,
        -0.1,
        2.0,
        10.0,
        1e10,
        -1e10,
        0.01,
        0.379232,
        0.801448,
        0.037927,
        0.907444,
        0.8338642,
        f32::MIN_POSITIVE,
        1e-40,
        -1e-40,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        nan(0x7fc0_0033),
        nan(0xffc0_0044),
        nan(0x7fe5_5555),
    ];
    // Inputs where the C runtime's `powf` isn't the correctly rounded `x * x` or `sqrt`
    // (docs/improvements.md, I-337), found by scanning every float: slider units whose
    // perceptual value in the range (0, 1) has `powf(x, 2) != x * x` on Windows (the square of
    // `perceptualToLinear`), mixing values with `powf(x, 0.5) != sqrt(x)` on Windows, and
    // mixing values with `powf(x, 0.5) != sqrtf(x)` on Linux (glibc, Rocky Linux 9).
    v.extend(
        [
            0x3c44_8128u32,
            0x3c44_9480,
            0x3c44_acb0,
            0x3c23_dc25,
            0x3c23_ee81,
            0x3c00_5972,
            0x3c00_66c3,
            0x3c00_68ea,
        ]
        .map(f32::from_bits),
    );
    // The break points and their neighbours.
    for b in [0.18f32, 0.42426407] {
        for d in [-2i32, -1, 0, 1, 2] {
            v.push(f32::from_bits(b.to_bits().wrapping_add_signed(d)));
        }
    }
    v
}

/// A dense grid: `n` steps over [lo, hi].
fn grid(lo: f32, hi: f32, n: usize) -> Vec<f32> {
    (0..=n)
        .map(|i| lo + (hi - lo) * (i as f32 / n as f32))
        .collect()
}

/// The manager's call that selects a perceptually uniform mixing space (the display space) or
/// not (the rendering space).
fn select(uniform: bool) -> Op {
    Op::SpaceIdx(u64::from(uniform))
}

/// Runs `ranges` x `values`, in each kind of mixing space, `per_request` ranges a request.
fn check_slider(
    test: &str,
    ranges: &[(f32, f32)],
    values: &[f32],
    per_request: usize,
    totals: &mut Totals,
    failures: &mut Vec<String>,
) {
    let source = Source::Yaml(CATEGORY_TEST_CONFIG);
    for uniform in [false, true] {
        for (chunk_index, chunk) in ranges.chunks(per_request).enumerate() {
            let label = format!("{test} uniform {uniform} #{chunk_index}");
            let config = source.port();
            let mut manager = MixingColorSpaceManager::new(Arc::clone(&config)).unwrap();
            let mut calls = vec![
                json!({"new": "MixingColorSpaceManager", "args": [{"ref": "config"}], "as": "m"}),
                json!({"call": "getSlider", "on": "m", "as": "s"}),
            ];
            let mut plan = Plan::default();
            let mut ops = vec![select(uniform)];
            for &(min, max) in chunk {
                ops.push(Op::Range(min, max));
                ops.push(Op::Slider);
                ops.extend(values.iter().map(|&v| Op::Convert(v)));
            }
            run_ops(
                &label,
                &ops,
                &config,
                &mut manager,
                "m",
                "s",
                &mut calls,
                &mut plan,
            );
            check_request(&label, &source, calls, plan, totals, failures);
        }
    }
}

/// Every ordered pair of edges, with the special values.
#[test]
fn slider_specials_match_the_wheel() {
    let mut failures = Vec::new();
    let mut totals = Totals::default();
    let edges = edges();
    let ranges: Vec<(f32, f32)> = edges
        .iter()
        .flat_map(|&a| edges.iter().map(move |&b| (a, b)))
        .collect();
    check_slider(
        "specials",
        &ranges,
        &specials(),
        40,
        &mut totals,
        &mut failures,
    );
    report("mixing_helpers_slider_specials", &failures);
    let expected = 2 * (ranges.len().div_ceil(40) + ranges.len() * (4 + 2 * specials().len()));
    assert_eq!(totals.checks, expected, "checks");
    assert!(totals.nans >= 10_000, "{} NaNs", totals.nans);
}

/// Ordinary ranges, with dense grids of slider and mixing units.
#[test]
fn slider_grids_match_the_wheel() {
    let mut failures = Vec::new();
    let mut totals = Totals::default();
    let ranges = [
        (0.0, 1.0),
        (-0.2, 5.0),
        (0.0, 100.0),
        (0.18, 0.5),
        (1.0, 0.0),
    ];
    let mut values = grid(-0.5, 1.5, 2000);
    values.extend(grid(-1.0, 10.0, 2000));
    values.extend((-400..=400).map(|e| 10f32.powf(e as f32 / 20.0)));
    check_slider("grids", &ranges, &values, 1, &mut totals, &mut failures);
    report("mixing_helpers_slider_grids", &failures);
    let expected = 2 * (ranges.len() + ranges.len() * (4 + 2 * values.len()));
    assert_eq!(totals.checks, expected, "checks");
}

// ---------------------------------------------------------------------------------------------
// U-152

/// A py_eval string.
fn s(v: &str) -> Value {
    json!({"str": hex(v.as_bytes())})
}

fn name(v: &str) -> Value {
    json!({"name": v})
}

/// A py_eval script: the category config from its text, its color picking role set to `role`
/// (on the config itself, or on a copy for a manager made before, `refresh`), then the
/// manager and its text.
fn script(role: &str, refresh: bool) -> Value {
    let mut steps = vec![
        json!({"call": name("OCIO.Config.CreateFromStream"), "args": [s(CATEGORY_TEST_CONFIG)]}),
    ];
    if refresh {
        // A manager with other selections and slider edges than its defaults, refreshed.
        let one = json!({"int": "1"});
        let float = |v: f64| json!({"float": format!("{:#018x}", v.to_bits())});
        steps.push(json!({"call": name("OCIO.MixingColorSpaceManager"), "args": [{"ref": 0}]}));
        steps.push(json!({"call": {"ref": 1, "attr": "setSelectedMixingSpaceIdx"}, "args": [one]}));
        steps.push(
            json!({"call": {"ref": 1, "attr": "setSelectedMixingEncodingIdx"},
            "args": [one]}),
        );
        steps.push(json!({"call": {"ref": 1, "attr": "getSlider"},
            "args": [float(-0.5), float(2.0)]}));
        steps.push(json!({"call": name("copy.deepcopy"), "args": [{"ref": 0}]}));
        steps.push(json!({"call": {"ref": 5, "attr": "setRole"}, "args": [s(ROLE), s(role)]}));
        steps.push(json!({"call": {"ref": 1, "attr": "refresh"}, "args": [{"ref": 5}]}));
        steps.push(json!({"call": name("repr"), "args": [{"ref": 1}]}));
    } else {
        steps.push(json!({"call": {"ref": 0, "attr": "setRole"}, "args": [s(ROLE), s(role)]}));
        steps.push(json!({"call": name("OCIO.MixingColorSpaceManager"), "args": [{"ref": 0}]}));
        steps.push(json!({"call": name("repr"), "args": [{"ref": 2}]}));
    }
    json!({"steps": steps})
}

/// A color picking role that names no color space (a name the config doesn't have, another
/// role's name): the wheel's process dies when it makes the manager of that config, or
/// refreshes one with it (docs/improvements.md, U-152); the port returns an error, and a
/// manager it refreshes is left as it was. A role naming a color space works on both.
#[test]
fn a_color_picking_role_naming_no_color_space_is_refused() {
    let roles = ["no_such_space", "scene_linear", "lin_1", "log_1"];
    let mut scripts = Vec::new();
    for role in roles {
        for refresh in [false, true] {
            scripts.push(script(role, refresh));
        }
    }
    let report = Oracle::get()
        .call("py_eval", json!({"scripts": scripts}), &[])
        .result;
    let reports = report["scripts"].as_array().expect("scripts");
    assert_eq!(reports.len(), 2 * roles.len());

    let base = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    let mut refused = 0;
    for (k, role) in roles.iter().enumerate() {
        let mut config = (*base).clone();
        config.set_role(ROLE, Some(role.as_bytes())).unwrap();
        let config = Arc::new(config);
        let made = MixingColorSpaceManager::new(Arc::clone(&config)).map(|m| m.to_bytes().unwrap());

        let mut manager = MixingColorSpaceManager::new(Arc::clone(&base)).unwrap();
        manager.set_selected_mixing_space_idx(1).unwrap();
        manager.set_selected_mixing_encoding_idx(1).unwrap();
        manager.slider_with_range(-0.5, 2.0);
        let before = manager.to_bytes().unwrap();
        let refreshed = manager.refresh(Arc::clone(&config));
        if refreshed.is_err() {
            // Refused: the manager is as it was.
            assert_eq!(manager.to_bytes().unwrap(), before, "role {role:?}");
        }
        let refreshed = refreshed.map(|()| manager.to_bytes().unwrap());

        for (j, port) in [made, refreshed].into_iter().enumerate() {
            let wheel = &reports[2 * k + j];
            let what = format!("role {role:?}, refresh {}: {wheel}", j == 1);
            let crashed = wheel.get("crashed");
            match port {
                Err(e) => {
                    refused += 1;
                    let message = e.to_string();
                    let names = format!("'{ROLE}' refers to the color space '{role}'");
                    assert!(
                        message.contains("U-152") && message.contains(&names),
                        "{what}: {message}"
                    );
                    assert!(
                        crashed.is_some_and(|c| c["returncode"].as_i64().is_some_and(|r| r != 0)),
                        "{what}"
                    );
                }
                Ok(text) => {
                    assert!(crashed.is_none(), "{what}");
                    let steps = wheel["steps"].as_array().expect("steps");
                    assert!(steps.iter().all(|s| s.get("raised").is_none()), "{what}");
                    let last = steps.last().expect("a step");
                    assert_eq!(
                        bytes(&last["value"]["str"]),
                        text,
                        "{what}: the manager's text"
                    );
                }
            }
        }
    }
    assert_eq!(
        refused, 4,
        "the two roles naming no color space, made and refreshed"
    );
}

/// Generated managers of upstream's test config files
/// (`common::app_helpers::upstream_test_configs`), each read from its text.
#[test]
fn upstream_test_config_managers_match_the_wheel() {
    let mut failures = Vec::new();
    let mut totals = Totals::default();
    for (k, (name, path)) in common::app_helpers::upstream_test_configs()
        .into_iter()
        .enumerate()
    {
        let text: &'static str = std::fs::read_to_string(&path).expect("the config").leak();
        let source = Source::Yaml(text);
        let config = source.port();
        let tokens = tokens(&config);
        let mut rng = Rng(0x31c0_0200 + k as u64);
        let cases: Vec<Vec<Op>> = (0..6).map(|_| gen_ops(&tokens, &mut rng, 16)).collect();
        check_cases(&name, &source, &cases, 6, &mut totals, &mut failures);
    }
    report("mixing_helpers_upstream_configs", &failures);
    let r = &totals.reached;
    assert!(r.made >= 40, "{} made", r.made);
    assert_eq!(r.pixels, 2 * r.made, "pixels compared");
    assert!(totals.color_picking >= 10, "{}", totals.color_picking);
}
