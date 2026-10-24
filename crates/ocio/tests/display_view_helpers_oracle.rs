// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `DisplayViewHelpers` against the wheel, through the oracle's `object_calls`.
//!
//! - `GetProcessor`: generated (working space, display, view, channel view, direction,
//!   context) cases over upstream's app helpers test config, the pipeline config of
//!   `legacy_viewing_pipeline_oracle.rs`, a config whose views already have dynamic exposure,
//!   contrast or gamma, and every built-in config: the processor (cache ID, flags, metadata,
//!   `createGroupTransform()`) or exception, the log, and the pixels of its CPU processors with
//!   the dynamic properties it has set away from their defaults.
//! - `GetIdentityProcessor`, the same way.
//! - `AddDisplayView` and `RemoveDisplayView`: generated cases on copies of configs, with
//!   active lists set or not and `OCIO_ACTIVE_DISPLAYS` / `OCIO_ACTIVE_VIEWS` set or not: the
//!   exception or none, the log, the config's text afterwards (`serialize`, also after an
//!   exception: upstream keeps what it changed before), and the processor of a pair added.
//!
//! The results depend on the CPU (the processors' kernels): this target is in the `cpu-tests`
//! alias.

mod common;

use std::sync::Arc;

use common::app_helpers::{
    EXPOSURE_CONTRAST_GAMMA, Reached, Rng, apply_calls, capture, compare_processor_call, fs,
    report, text,
};
use ocio::display_view_helpers::{
    add_display_view, identity_processor, processor, remove_display_view,
};
use ocio::{
    BuiltinConfigRegistry, Config, Context, MatrixTransform, Processor, TransformDirection,
};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, log};
use serde_json::{Value, json};

/// The config of upstream's app helpers tests (tests/cpu/apphelpers/configs.data:5-173 @
/// v2.5.2).
const CATEGORY_TEST_CONFIG: &str = include_str!("data/apphelpers_category_test_config.ocio");

/// The config of the pipeline's paths (see `legacy_viewing_pipeline_oracle.rs`).
const PIPELINE_CONFIG: &str = include_str!("data/apphelpers_pipeline_config.ocio");

/// A config whose views already have an ExposureContrastTransform whose exposure, contrast or
/// gamma is dynamic (a value missing in the YAML is dynamic), in a color space or a look.
const DYNAMIC_CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw
  scene_linear: lin

displays:
  disp:
    - !<View> {name: all_dynamic, colorspace: ec_all}
    - !<View> {name: exposure_only, colorspace: ec_exposure}
    - !<View> {name: gamma_only, colorspace: ec_gamma}
    - !<View> {name: plain, colorspace: srgb}
    - !<View> {name: looked, colorspace: srgb, looks: grade}

active_displays: []
active_views: []

looks:
  - !<Look>
    name: grade
    process_space: log
    transform: !<ExposureContrastTransform> {style: log, contrast: 1.1, gamma: 1}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin

  - !<ColorSpace>
    name: log
    to_scene_reference: !<LogTransform> {base: 2, direction: inverse}

  - !<ColorSpace>
    name: srgb
    from_scene_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1], direction: inverse}

  - !<ColorSpace>
    name: ec_all
    from_scene_reference: !<ExposureContrastTransform> {style: video, pivot: 1}

  - !<ColorSpace>
    name: ec_exposure
    from_scene_reference: !<ExposureContrastTransform> {style: linear, gamma: 1.2, pivot: 0.18}

  - !<ColorSpace>
    name: ec_gamma
    from_scene_reference: !<ExposureContrastTransform> {style: video, exposure: 0.1, contrast: 1.1}
"#;

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

fn lut1d_green() -> String {
    ocio_testkit::paths::upstream_dir()
        .join("tests/data/files/lut1d_green.ctf")
        .to_string_lossy()
        .replace('\\', "/")
}

/// The dynamic properties of `proc`'s CPU processor, set away from their defaults before the
/// pixels are applied.
fn dynamic_sets(proc: &Processor) -> Vec<common::app_helpers::DynamicSet> {
    EXPOSURE_CONTRAST_GAMMA
        .iter()
        .copied()
        .filter(|(t, _, _)| proc.has_dynamic_property(*t))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// GetProcessor

/// The channel views: a matrix that reads the alpha, upstream's red view, a scale, and the
/// identity.
fn channel_views() -> Vec<(&'static str, [f64; 16])> {
    let mut alpha = [0.; 16];
    alpha[3] = 1.;
    alpha[7] = 1.;
    alpha[11] = 1.;
    alpha[15] = 1.;
    let mut red = [0.; 16];
    red[0] = 1.;
    let mut scale = [0.; 16];
    scale[0] = 1.1;
    scale[5] = 1.2;
    scale[10] = 0.9;
    scale[15] = 1.;
    let mut identity = [0.; 16];
    for i in 0..4 {
        identity[i * 5] = 1.;
    }
    vec![
        ("alpha", alpha),
        ("red", red),
        ("scale", scale),
        ("identity", identity),
    ]
}

#[derive(Debug, Clone)]
struct ProcessorCase {
    working: String,
    display: String,
    view: String,
    channel_view: Option<usize>,
    dir: TransformDirection,
    context: bool,
}

fn processor_cases(config: &Config, seed: u64, n: usize) -> Vec<ProcessorCase> {
    let mut sources: Vec<String> = (0..config.num_color_spaces())
        .map(|i| text(config.color_space_name_by_index(i)))
        .collect();
    sources.extend((0..config.num_roles()).map(|i| text(config.role_name(i))));
    sources.push("no_such_space".to_string());
    let mut pairs = Vec::new();
    for d in 0..config.num_displays() {
        let display = text(config.display(d));
        for v in 0..config.num_views(&display) {
            pairs.push((display.clone(), text(config.view(&display, v))));
        }
        pairs.push((display, "no_such_view".to_string()));
    }
    let mut rng = Rng(seed);
    let num_views = channel_views().len();
    (0..n)
        .map(|_| {
            let (display, view) = rng.pick(&pairs);
            ProcessorCase {
                working: rng.pick(&sources),
                display,
                view,
                channel_view: rng.maybe(num_views, 2),
                dir: if rng.below(3) == 0 {
                    TransformDirection::Inverse
                } else {
                    TransformDirection::Forward
                },
                context: rng.below(2) == 0,
            }
        })
        .collect()
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

/// Compares `GetProcessor` for `cases` on the config of `source`, `per_request` a request.
fn check_processors(
    test: &str,
    source: &Source,
    cases: &[ProcessorCase],
    per_request: usize,
    failures: &mut Vec<String>,
) -> Reached {
    let mut reached = Reached::default();
    let views = channel_views();
    for (chunk_index, chunk) in cases.chunks(per_request).enumerate() {
        let label = format!("{test} #{chunk_index}");
        let config = source.port();
        let context: Arc<Context> = config.current_context().get();
        let mut calls = vec![json!({"call": "getCurrentContext", "as": "ctx"})];
        let mut plan = Vec::new();
        for (k, case) in chunk.iter().enumerate() {
            let proc_name = format!("proc{k}");
            let mut kwargs = json!({
                "config": {"ref": "config"},
                "workingSpaceName": case.working,
                "displayName": case.display,
                "viewName": case.view,
                "direction": {"enum": dir_name(case.dir)},
            });
            let mut matrix = MatrixTransform::new();
            if let Some(i) = case.channel_view {
                matrix.set_matrix(&views[i].1);
                kwargs["channelView"] = json!({"transform": {"class": "MatrixTransform", "calls": [["setMatrix", fs(&views[i].1)]]}});
            }
            if case.context {
                kwargs["context"] = json!({"ref": "ctx"});
            }
            let at = calls.len();
            calls.push(json!({"call": "GetProcessor", "on": "DisplayViewHelpers",
                "kwargs": kwargs, "as": proc_name}));
            let (made, port_log) = capture(|| {
                processor(
                    &config,
                    case.context.then_some(&*context),
                    &case.working,
                    &case.display,
                    &case.view,
                    case.channel_view.map(|_| &matrix),
                    case.dir,
                )
            });
            let applies = match &made {
                Ok(proc) => apply_calls(&mut calls, &proc_name, proc, &dynamic_sets(proc)),
                Err(_) => Vec::new(),
            };
            plan.push((k, case, at, made, port_log, applies));
        }
        let response = Oracle::get().call(
            "object_calls",
            json!({"config": source.wheel(), "calls": calls}),
            &[],
        );
        let result = &response.result;
        assert_eq!(result["config"], Value::Null, "{label}: {result}");
        let results = result["calls"].as_array().expect("calls");
        assert_eq!(results.len(), calls.len(), "{label}");
        for (k, case, at, made, port_log, applies) in plan {
            let mut problems = Vec::new();
            compare_processor_call(
                &response,
                results,
                at,
                &made,
                &port_log,
                applies,
                &mut reached,
                &mut problems,
            );
            if !problems.is_empty() {
                failures.push(format!(
                    "{label} case {k} {case:?}: {}",
                    problems.join("\n    ")
                ));
            }
        }
    }
    reached
}

/// Generated `GetProcessor` cases of upstream's test config, the pipeline config and the
/// dynamic config.
#[test]
fn processors_match_the_wheel() {
    let mut failures = Vec::new();
    let mut all = Reached::default();
    for (test, source, seed, n, min_made) in [
        (
            "category",
            Source::Yaml(CATEGORY_TEST_CONFIG),
            0xd15b_0001,
            120,
            50,
        ),
        (
            "pipeline",
            Source::Yaml(PIPELINE_CONFIG),
            0xd15b_0002,
            120,
            40,
        ),
        (
            "dynamic",
            Source::Yaml(DYNAMIC_CONFIG),
            0xd15b_0003,
            120,
            50,
        ),
    ] {
        let config = source.port();
        let r = check_processors(
            test,
            &source,
            &processor_cases(&config, seed, n),
            40,
            &mut failures,
        );
        assert!(
            r.made >= min_made && r.refused >= n / 20,
            "{test}: {} made, {} refused",
            r.made,
            r.refused
        );
        all.add(r);
    }
    report("display_view_helpers_processors", &failures);
    assert!(all.groups.len() >= 40, "{} group shapes", all.groups.len());
    assert_eq!(all.pixels, 2 * all.made, "pixels compared");
    // The early return (both controls there, no channel view): the display view transform's
    // processor alone, with an ExposureContrastTransform of the config and no other.
    assert!(
        all.groups
            .iter()
            .any(|g| g == &["ExposureContrastTransform".to_string()]),
        "{:#?}",
        all.groups
    );
}

/// Generated `GetProcessor` cases of every built-in config.
#[test]
fn builtin_config_processors_match_the_wheel() {
    let mut failures = Vec::new();
    let registry = BuiltinConfigRegistry::get();
    for i in 0..registry.num_builtin_configs() {
        let name = text(registry.builtin_config_name(i).unwrap());
        let source = Source::Builtin(name.clone());
        let config = source.port();
        let r = check_processors(
            &name,
            &source,
            &processor_cases(&config, 0xd15b_0100 + i as u64, 30),
            30,
            &mut failures,
        );
        assert!(r.made >= 10, "{name}: {} made", r.made);
        assert_eq!(r.pixels, 2 * r.made, "{name}: pixels compared");
    }
    report("display_view_helpers_builtin_processors", &failures);
}

/// `GetIdentityProcessor` of the raw config and upstream's test config.
#[test]
fn identity_processors_match_the_wheel() {
    let mut failures = Vec::new();
    let mut reached = Reached::default();
    for source in [Source::Raw, Source::Yaml(CATEGORY_TEST_CONFIG)] {
        let config = source.port();
        let mut calls = vec![
            json!({"call": "GetIdentityProcessor", "on": "DisplayViewHelpers",
            "args": [{"ref": "config"}], "as": "proc"}),
        ];
        let (made, port_log) = capture(|| identity_processor(&config));
        let mut applies = Vec::new();
        if let Ok(proc) = &made {
            applies = apply_calls(&mut calls, "proc", proc, &[]);
            applies.extend(apply_calls(&mut calls, "proc", proc, &dynamic_sets(proc)));
        }
        let response = Oracle::get().call(
            "object_calls",
            json!({"config": source.wheel(), "calls": calls}),
            &[],
        );
        let results = response.result["calls"].as_array().expect("calls");
        assert_eq!(results.len(), calls.len());
        let mut problems = Vec::new();
        compare_processor_call(
            &response,
            results,
            0,
            &made,
            &port_log,
            applies,
            &mut reached,
            &mut problems,
        );
        failures.extend(problems);
    }
    report("display_view_helpers_identity", &failures);
    assert_eq!((reached.made, reached.pixels), (2, 8));
}

// ---------------------------------------------------------------------------------------------
// AddDisplayView and RemoveDisplayView

/// What a case does to a copy of the config: set its active lists, then add pairs, then
/// remove pairs.
#[derive(Debug, Clone)]
struct EditCase {
    active_displays: Option<&'static str>,
    active_views: Option<&'static str>,
    adds: Vec<Add>,
    removes: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct Add {
    display: String,
    view: String,
    look: String,
    name: String,
    family: String,
    description: String,
    categories: String,
    file: String,
    connection: String,
}

/// The names a config's edits take.
struct EditTokens {
    displays: Vec<String>,
    views: Vec<String>,
    pairs: Vec<(String, String)>,
    color_spaces: Vec<String>,
    looks: Vec<String>,
    categories: Vec<String>,
}

fn edit_tokens(config: &Config) -> EditTokens {
    let mut displays = Vec::new();
    let mut views = Vec::new();
    let mut pairs = Vec::new();
    for d in 0..config.num_displays() {
        let display = text(config.display(d));
        for v in 0..config.num_views(&display) {
            let view = text(config.view(&display, v));
            views.push(view.clone());
            pairs.push((display.clone(), view));
        }
        displays.push(display);
    }
    let mut color_spaces: Vec<String> = (0..config.num_color_spaces())
        .map(|i| text(config.color_space_name_by_index(i)))
        .collect();
    color_spaces.extend((0..config.num_roles()).map(|i| text(config.role_name(i))));
    let mut categories = Vec::new();
    for i in 0..config.num_color_spaces() {
        let cs = config
            .color_space(config.color_space_name_by_index(i))
            .unwrap();
        for c in 0..cs.num_categories() {
            categories.push(text(cs.category(c).unwrap()));
        }
    }
    categories.sort();
    categories.dedup();
    let looks = (0..config.num_looks())
        .map(|i| text(config.look_name_by_index(i)))
        .collect();
    EditTokens {
        displays,
        views,
        pairs,
        color_spaces,
        looks,
        categories,
    }
}

fn edit_cases(t: &EditTokens, seed: u64, n: usize) -> Vec<EditCase> {
    let mut rng = Rng(seed);
    let file = lut1d_green();
    (0..n)
        .map(|k| {
            let num_adds = [0, 1, 1, 1, 2][rng.below(5)];
            let mut adds = Vec::new();
            for a in 0..num_adds {
                let display = match rng.below(8) {
                    0 => String::new(),
                    1 | 2 => "NEW_DISPLAY".to_string(),
                    _ => rng.pick(&t.displays),
                };
                let view = match rng.below(10) {
                    0 => String::new(),
                    1 | 2 => rng.pick(&t.views),
                    _ => format!("NEW_VIEW_{a}"),
                };
                let look = match rng.below(4) {
                    0 if !t.looks.is_empty() => rng.pick(&t.looks),
                    1 => "missing_look".to_string(),
                    _ => String::new(),
                };
                let name = match rng.below(10) {
                    0 => String::new(),
                    1 | 2 => rng.pick(&t.color_spaces),
                    _ => format!("new_cs_{k}_{a}"),
                };
                let categories = match rng.below(5) {
                    0 => String::new(),
                    1 => "cat1, cat2".to_string(),
                    _ if !t.categories.is_empty() => {
                        let a = rng.pick(&t.categories);
                        let b = rng.pick(&t.categories);
                        format!(" {a} ,{} , other", b.to_uppercase())
                    }
                    _ => "file-io".to_string(),
                };
                let file = match rng.below(8) {
                    0 => String::new(),
                    1 => "no_such_lut.ctf".to_string(),
                    _ => file.clone(),
                };
                let connection = match rng.below(10) {
                    0 => String::new(),
                    1 => "no_such_space".to_string(),
                    _ => rng.pick(&t.color_spaces),
                };
                adds.push(Add {
                    display,
                    view,
                    look,
                    name,
                    family: ["", "Family", "A/B"][rng.below(3)].to_string(),
                    description: ["", "A description."][rng.below(2)].to_string(),
                    categories,
                    file,
                    connection,
                });
            }
            let mut removes = Vec::new();
            for _ in 0..[0, 1, 1, 2][rng.below(4)] {
                let (display, view) = match (rng.below(7), adds.first()) {
                    (0, _) => ("no_such_display".to_string(), "no_such_view".to_string()),
                    (1, _) => (rng.pick(&t.displays), "no_such_view".to_string()),
                    (2, _) => (String::new(), rng.pick(&t.views)),
                    (3 | 4, Some(add)) => (add.display.clone(), add.view.clone()),
                    _ => rng.pick(&t.pairs),
                };
                // The names compare ignoring case: a pair spelled in another case.
                let (display, view) = match rng.below(5) {
                    0 => (display.to_lowercase(), view),
                    1 => (display, view.to_lowercase()),
                    _ => (display, view),
                };
                removes.push((display, view));
            }
            let active_displays = [
                None,
                None,
                Some(""),
                Some("DISP_1"),
                Some("DISP_2, disp_1"),
                Some("\" \""),
                Some(","),
            ][rng.below(7)];
            let active_views = [
                None,
                None,
                Some(""),
                Some("VIEW_3, VIEW_2"),
                Some("view_1"),
                Some("\" \""),
                Some(","),
            ][rng.below(7)];
            EditCase {
                active_displays,
                active_views,
                adds,
                removes,
            }
        })
        .collect()
}

/// The environments of the edit requests: none, or the active lists from the environment.
const ENVS: [&[(&str, &str)]; 4] = [
    &[],
    &[("OCIO_ACTIVE_DISPLAYS", "DISP_1")],
    &[("OCIO_ACTIVE_VIEWS", "VIEW_3, VIEW_2")],
    &[("OCIO_ACTIVE_DISPLAYS", " "), ("OCIO_ACTIVE_VIEWS", "")],
];

/// Restores the process environment of this thread when dropped.
struct Unset;

impl Drop for Unset {
    fn drop(&mut self) {
        set_thread_env_provider(None);
    }
}

/// A call's outcome on the port's side: `Ok(())` or the exception, and the log.
type Outcome = (ocio::Result<()>, Vec<Vec<u8>>);

/// A case's steps (the call's index, its label and the port's outcome), then the index of its
/// serialize call and the port's text.
type CasePlan = (
    usize,
    Vec<(usize, String, Outcome)>,
    usize,
    ocio::Result<Vec<u8>>,
);

/// Compares the edits `cases` on copies of the config of `source`, in the environment `env`.
fn check_edits(
    test: &str,
    source: &Source,
    env: &[(&str, &str)],
    cases: &[EditCase],
    failures: &mut Vec<String>,
    reached: &mut Reached,
) {
    let _env = {
        let entries: Vec<(&str, &[u8])> = env.iter().map(|(k, v)| (*k, v.as_bytes())).collect();
        set_thread_env_provider(Some(Arc::new(MapEnv::from_entries(&entries))));
        Unset
    };
    let label = format!("{test} env {env:?}");
    let original = source.port();
    let mut calls = Vec::new();
    // Per case and step: the call's index, its label and the port's outcome; then the
    // serialize call's index and the port's text.
    let mut plan: Vec<CasePlan> = Vec::new();
    let mut processors = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let cfg = format!("cfg{k}");
        let mut config = (*original).clone();
        calls.push(json!({"copy": "config", "as": cfg}));
        let mut steps = Vec::new();
        for (setter, list) in [
            ("setActiveDisplays", case.active_displays),
            ("setActiveViews", case.active_views),
        ] {
            if let Some(list) = list {
                let outcome = capture(|| match setter {
                    "setActiveDisplays" => config.set_active_displays(list),
                    _ => config.set_active_views(list),
                });
                steps.push((calls.len(), format!("{setter}({list:?})"), outcome));
                calls.push(json!({"call": setter, "on": cfg, "args": [list]}));
            }
        }
        for (a, add) in case.adds.iter().enumerate() {
            let outcome = capture(|| {
                add_display_view(
                    &mut config,
                    &add.display,
                    &add.view,
                    &add.look,
                    &add.name,
                    &add.family,
                    &add.description,
                    &add.categories,
                    &add.file,
                    &add.connection,
                )
            });
            let added = outcome.0.is_ok();
            steps.push((calls.len(), format!("AddDisplayView {add:?}"), outcome));
            calls.push(
                json!({"call": "AddDisplayView", "on": "DisplayViewHelpers", "kwargs": {
                    "config": {"ref": cfg},
                    "displayName": add.display,
                    "viewName": add.view,
                    "lookName": add.look,
                    "colorSpaceName": add.name,
                    "colorSpaceFamily": add.family,
                    "colorSpaceDescription": add.description,
                    "colorSpaceCategories": add.categories,
                    "transformFilePath": add.file,
                    "connectionColorSpaceName": add.connection,
                }}),
            );
            if added {
                // The processor of the new pair, from the default role.
                let proc_name = format!("proc{k}_{a}");
                let at = calls.len();
                calls.push(json!({"call": "GetProcessor", "on": "DisplayViewHelpers",
                    "kwargs": {"config": {"ref": cfg}, "workingSpaceName": "default",
                        "displayName": add.display, "viewName": add.view},
                    "as": proc_name}));
                let (made, port_log) = capture(|| {
                    processor(
                        &config,
                        None,
                        "default",
                        &add.display,
                        &add.view,
                        None,
                        TransformDirection::Forward,
                    )
                });
                let applies = match &made {
                    Ok(proc) => apply_calls(&mut calls, &proc_name, proc, &dynamic_sets(proc)),
                    Err(_) => Vec::new(),
                };
                processors.push((
                    format!("{label} case {k} add {a}"),
                    at,
                    made,
                    port_log,
                    applies,
                ));
            }
        }
        for (display, view) in &case.removes {
            let outcome = capture(|| remove_display_view(&mut config, display, view));
            steps.push((
                calls.len(),
                format!("RemoveDisplayView({display:?}, {view:?})"),
                outcome,
            ));
            calls.push(
                json!({"call": "RemoveDisplayView", "on": "DisplayViewHelpers",
                "args": [{"ref": cfg}, display, view]}),
            );
        }
        let serialize_at = calls.len();
        calls.push(json!({"call": "serialize", "on": cfg}));
        plan.push((k, steps, serialize_at, config.serialize()));
    }

    let mut args = json!({"config": source.wheel(), "calls": calls});
    if !env.is_empty() {
        args["env"] = env
            .iter()
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect::<serde_json::Map<_, _>>()
            .into();
    }
    let response = Oracle::get().call("object_calls", args, &[]);
    let result = &response.result;
    assert_eq!(result["config"], Value::Null, "{label}: {result}");
    let results = result["calls"].as_array().expect("calls");
    assert_eq!(results.len(), calls.len(), "{label}");

    for (k, steps, serialize_at, port_text) in plan {
        let mut problems = Vec::new();
        for (at, step, (port, port_log)) in steps {
            let wheel = &results[at];
            match (wheel.get("exception"), &port) {
                (Some(e), Err(port)) => {
                    reached.refused += 1;
                    let message = bytes(&e["message"]);
                    reached
                        .messages
                        .insert(String::from_utf8_lossy(&message).into_owned());
                    if message != port.what() {
                        problems.push(format!(
                            "{step}: messages {:?} != {:?}",
                            String::from_utf8_lossy(&message),
                            String::from_utf8_lossy(port.what())
                        ));
                    }
                }
                (None, Ok(())) if wheel.get("result").is_some() => reached.made += 1,
                _ => problems.push(format!("{step}: wheel {wheel}, port {port:?}")),
            }
            let wheel_log = log(&wheel["log"]);
            if wheel_log != port_log {
                problems.push(format!("{step}: log {wheel_log:?} != {port_log:?}"));
            }
        }
        let wheel_text = &results[serialize_at];
        match (&port_text, wheel_text["result"].as_str()) {
            (Ok(port), Some(wheel)) if port.as_slice() == wheel.as_bytes() => {}
            _ => problems.push(format!(
                "serialize: wheel {wheel_text}, port {:?}",
                port_text.as_ref().map(|t| String::from_utf8_lossy(t))
            )),
        }
        if !problems.is_empty() {
            failures.push(format!(
                "{label} case {k} {:?}: {}",
                cases[k],
                problems.join("\n    ")
            ));
        }
    }
    let mut processor_reached = Reached::default();
    for (name, at, made, port_log, applies) in processors {
        let mut problems = Vec::new();
        compare_processor_call(
            &response,
            results,
            at,
            &made,
            &port_log,
            applies,
            &mut processor_reached,
            &mut problems,
        );
        if !problems.is_empty() {
            failures.push(format!("{name}: {}", problems.join("\n    ")));
        }
    }
    assert_eq!(
        processor_reached.pixels,
        2 * processor_reached.made,
        "{label}: pixels compared"
    );
    reached.pixels += processor_reached.pixels;
    reached.groups.extend(processor_reached.groups);
}

/// Generated `AddDisplayView` and `RemoveDisplayView` cases on copies of upstream's test
/// config, the pipeline config and a built-in config, in each environment.
#[test]
fn edits_match_the_wheel() {
    let mut failures = Vec::new();
    let mut reached = Reached::default();
    for (test, source, seed) in [
        ("category", Source::Yaml(CATEGORY_TEST_CONFIG), 0xd15b_1001),
        ("pipeline", Source::Yaml(PIPELINE_CONFIG), 0xd15b_1002),
        (
            "cg",
            Source::Builtin("cg-config-v2.2.0_aces-v1.3_ocio-v2.4".to_string()),
            0xd15b_1003,
        ),
    ] {
        let tokens = edit_tokens(&source.port());
        for (e, env) in ENVS.iter().enumerate() {
            let cases = edit_cases(&tokens, seed + 16 * e as u64, 40);
            check_edits(test, &source, env, &cases, &mut failures, &mut reached);
        }
    }
    report("display_view_helpers_edits", &failures);

    for fragment in [
        "Invalid display name.",
        "Missing color space for '' and '",
        "Invalid view name.",
        "already exists.",
        "Connection color space name 'no_such_space' does not exist.",
        "Connection color space name '' does not exist.",
        "Forbidden to add an active display as 'OCIO_ACTIVE_DISPLAYS' controls the active list.",
        "Forbidden to add an active view as 'OCIO_ACTIVE_VIEWS' controls the active list.",
        "Forbidden to remove an active display as 'OCIO_ACTIVE_DISPLAYS' controls the active list.",
        "Forbidden to remove an active view as 'OCIO_ACTIVE_VIEWS' controls the active list.",
    ] {
        assert!(
            reached.messages.iter().any(|m| m.contains(fragment)),
            "no case raised {fragment:?}: {:#?}",
            reached.messages
        );
    }
    assert!(
        reached.made >= 300 && reached.refused >= 100,
        "{} done, {} refused",
        reached.made,
        reached.refused
    );
    assert!(
        reached.pixels >= 100,
        "{} pixel comparisons",
        reached.pixels
    );
}

/// A view added with a connection color space takes the connection's transform from the
/// reference when it has one, else the inverse of its transform to the reference: `cam` has
/// both (an exponent from the scene reference and a matrix to it, which are not each other's
/// inverse), `log` only the transform to, `srgb` only the transform from.
#[test]
fn the_connection_color_space_gives_its_from_reference_transform_first() {
    let source = Source::Yaml(PIPELINE_CONFIG);
    let file = lut1d_green();
    let cases: Vec<EditCase> = ["cam", "log", "srgb"]
        .into_iter()
        .map(|connection| EditCase {
            active_displays: None,
            active_views: None,
            adds: vec![Add {
                display: "d_srgb".to_string(),
                view: format!("NEW_VIEW_{connection}"),
                look: String::new(),
                name: format!("new_cs_{connection}"),
                family: String::new(),
                description: String::new(),
                categories: String::new(),
                file: file.clone(),
                connection: connection.to_string(),
            }],
            removes: Vec::new(),
        })
        .collect();
    let mut failures = Vec::new();
    let mut reached = Reached::default();
    check_edits(
        "connection",
        &source,
        &[],
        &cases,
        &mut failures,
        &mut reached,
    );
    report("display_view_helpers_connection", &failures);
    // Each add worked on both sides (and so did its processor).
    assert_eq!(
        (reached.made, reached.refused),
        (3, 0),
        "{:?}",
        reached.messages
    );
    assert_eq!(reached.pixels, 6);
}

/// Generated `GetProcessor` cases of upstream's test config files
/// (`common::app_helpers::upstream_test_configs`), each read from its text.
#[test]
fn upstream_test_config_processors_match_the_wheel() {
    let mut failures = Vec::new();
    let mut all = Reached::default();
    let mut swept = 0;
    for (k, (name, path)) in common::app_helpers::upstream_test_configs()
        .into_iter()
        .enumerate()
    {
        let text: &'static str = std::fs::read_to_string(&path).expect("the config").leak();
        let source = Source::Yaml(text);
        let config = source.port();
        // The cases are (display, view) pairs: a config without displays has none.
        if config.num_displays() == 0 {
            continue;
        }
        swept += 1;
        let r = check_processors(
            &name,
            &source,
            &processor_cases(&config, 0xd15b_0200 + k as u64, 30),
            30,
            &mut failures,
        );
        assert!(
            r.made + r.refused >= 30,
            "{name}: {} made, {} refused",
            r.made,
            r.refused
        );
        all.add(r);
    }
    report("display_view_helpers_upstream_configs", &failures);
    assert_eq!(swept, 5, "the configs with displays");
    // Most cases of these configs are refused (views of view transforms or files they lack).
    assert!(
        all.made >= 20 && all.refused >= 60,
        "{} made, {} refused",
        all.made,
        all.refused
    );
    assert_eq!(all.pixels, 2 * all.made, "pixels compared");
}
