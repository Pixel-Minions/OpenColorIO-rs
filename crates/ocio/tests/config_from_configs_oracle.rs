// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Config::GetProcessorFromConfigs`, its 8 overloads, against the wheel, through the oracle's
//! `object_calls` (WP 9.1b).
//!
//! Generated cases over pairs of configs: upstream's test configs of
//! `get_processor_from_two_configs`, generated ones (interchange roles present, missing, and
//! naming a missing color space; data, inactive, display-referred and aliased spaces; views
//! with a view transform, `<USE_DISPLAY_NAME>` and a data space; a dynamic property on each
//! side; a context variable in a color space transform), every built-in config, and upstream's
//! configs of `tests/data/files/configs` (context variables in file paths and search paths).
//! Each case picks an overload, names (color spaces, roles, aliases, missing ones and `""`),
//! interchange names, a display and view, a direction, and for the overloads with contexts
//! each side's context: the config's current one, a copy with a variable changed, or a new
//! one. Compared: the processor (cache ID, flags, metadata, `createGroupTransform()`) or the
//! exception's message, the log, and the pixels of its CPU processors (default and
//! `OPTIMIZATION_NONE`) at extreme values, NaNs and infinities, with its dynamic properties
//! set away from their defaults.
//!
//! The results depend on the CPU (the processors' kernels): this target is in the `cpu-tests`
//! alias.

mod common;

use std::sync::Arc;

use common::app_helpers::{
    EXPOSURE_CONTRAST_GAMMA, Reached, Rng, apply_calls, capture, compare_processor_call, pixels,
    report, text,
};
use ocio::{
    BuiltinConfigRegistry, ColorSpaceVisibility, Config, Context, DynamicPropertyType, Processor,
    ProcessorCacheFlags, SearchReferenceSpaceType, TransformDirection,
};
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::image_desc::PackedImageDesc;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::Response;
use ocio_testkit::oracle_values::log;
use ocio_testkit::processor_ops::ProcessorOpsReply;
use serde_json::{Value, json};

/// The source config of upstream's `get_processor_from_two_configs` (Config_tests.cpp:6580-6637
/// @ v2.5.2).
const SIMPLE_CONFIG1: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw1
  aces_interchange: aces1
  cie_xyz_d65_interchange: display1

displays:
  displayname:
    - !<View> {name: view1, colorspace: displaytest1}
    - !<View> {name: view2, view_transform: vt1, display_colorspace: display2}
    - !<View> {name: view3, colorspace: data_space}

view_transforms:
  - !<ViewTransform>
    name: vt1
    from_scene_reference: !<RangeTransform> {min_in_value: 0., min_out_value: 0.}

colorspaces:
  - !<ColorSpace>
    name: raw1
    allocation: uniform

  - !<ColorSpace>
    name: test1
    allocation: uniform
    to_scene_reference: !<MatrixTransform> {offset: [0.01, 0.02, 0.03, 0]}

  - !<ColorSpace>
    name: displaytest1
    allocation: uniform
    to_scene_reference: !<LogTransform> {base: 2}

  - !<ColorSpace>
    name: aces1
    allocation: uniform
    from_scene_reference: !<ExponentTransform> {value: [1.101, 1.202, 1.303, 1.404]}

  - !<ColorSpace>
    name: data_space
    isdata: true

display_colorspaces:
  - !<ColorSpace>
    name: display1
    allocation: uniform
    from_display_reference: !<CDLTransform> {slope: [1, 2, 1]}

  - !<ColorSpace>
    name: display2
    allocation: uniform
    from_display_reference: !<FixedFunctionTransform> {style: ACES_RedMod03}

"#;

/// The destination config of upstream's `get_processor_from_two_configs`
/// (Config_tests.cpp:6639-6676 @ v2.5.2).
const SIMPLE_CONFIG2: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw2
  aces_interchange: aces2
  cie_xyz_d65_interchange: display3
  test_role: test2

colorspaces:
  - !<ColorSpace>
    name: raw2
    allocation: uniform

  - !<ColorSpace>
    name: test2
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: aces2
    allocation: uniform
    to_scene_reference: !<RangeTransform> {min_in_value: -0.0109, max_in_value: 1.0505, min_out_value: 0.0009, max_out_value: 2.5001}

display_colorspaces:
  - !<ColorSpace>
    name: display3
    allocation: uniform
    from_display_reference: !<ExponentTransform> {value: 2.4}

  - !<ColorSpace>
    name: display4
    allocation: uniform
    from_display_reference: !<LogTransform> {base: 5}
"#;

/// The config without interchange roles of upstream's `get_processor_from_two_configs`
/// (Config_tests.cpp:6845-6869 @ v2.5.2).
const SIMPLE_CONFIG3: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw

colorspaces:
  - !<ColorSpace>
    name: raw
    allocation: uniform

  - !<ColorSpace>
    name: test
    allocation: uniform
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

display_colorspaces:
  - !<ColorSpace>
    name: display5
    allocation: uniform
    from_display_reference: !<ExponentTransform> {value: 2.4}
"#;

/// A config of everything the overloads branch on: both interchange roles, a data space (also
/// a view's), an inactive space, aliases, a view transform, a `<USE_DISPLAY_NAME>` view, an
/// ExposureContrastTransform whose gamma is dynamic (missing in the YAML), and a color space
/// transform from the context variable `CS`.
const EDGE_CONFIG: &str = r#"ocio_profile_version: 2

environment:
  CS: lin_b

roles:
  default: raw
  aces_interchange: lin_a
  cie_xyz_d65_interchange: xyz
  scene_linear: lin_b

inactive_colorspaces: [lin_inactive]

displays:
  disp:
    - !<View> {name: plain, colorspace: curve}
    - !<View> {name: shown, view_transform: vt, display_colorspace: xyz}
    - !<View> {name: use_name, view_transform: vt, display_colorspace: <USE_DISPLAY_NAME>}
    - !<View> {name: data, colorspace: raw}
    - !<View> {name: ec, colorspace: ec_dyn}
    - !<View> {name: missing, colorspace: no_such_space}
  other:
    - !<View> {name: shown, view_transform: vt, display_colorspace: disp}

view_transforms:
  - !<ViewTransform>
    name: vt
    from_scene_reference: !<ExponentTransform> {value: [1.8, 1.9, 2.0, 1]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin_a
    aliases: [reference_alias]

  - !<ColorSpace>
    name: lin_b
    to_scene_reference: !<MatrixTransform> {matrix: [0.6, 0.3, 0.1, 0, 0.2, 0.7, 0.1, 0, 0.0, 0.1, 0.9, 0, 0, 0, 0, 1]}

  - !<ColorSpace>
    name: lin_inactive
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0, -0.1, 0]}

  - !<ColorSpace>
    name: curve
    aliases: [curve_alias]
    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [2.4, 2.4, 2.4, 1], offset: [0.055, 0.055, 0.055, 0], direction: inverse}

  - !<ColorSpace>
    name: ctx
    to_scene_reference: !<ColorSpaceTransform> {src: $CS, dst: lin_a}

  - !<ColorSpace>
    name: ec_dyn
    from_scene_reference: !<ExposureContrastTransform> {style: linear, exposure: 0.25, contrast: 1.1, pivot: 0.18}

display_colorspaces:
  - !<ColorSpace>
    name: xyz
    from_display_reference: !<CDLTransform> {slope: [1.1, 0.9, 1], power: [1.2, 1, 0.8]}

  - !<ColorSpace>
    name: disp
    from_display_reference: !<LogTransform> {base: 10}

  - !<ColorSpace>
    name: display_data
    isdata: true
"#;

/// [`EDGE_CONFIG`] with its interchange roles naming color spaces it doesn't have.
fn ghost_config() -> String {
    EDGE_CONFIG
        .replace("aces_interchange: lin_a", "aces_interchange: ghost_scene")
        .replace(
            "cie_xyz_d65_interchange: xyz",
            "cie_xyz_d65_interchange: ghost_display",
        )
}

/// [`EDGE_CONFIG`] with a data space as its scene-referred interchange space.
fn data_interchange_config() -> String {
    EDGE_CONFIG.replace("aces_interchange: lin_a", "aces_interchange: raw")
}

/// Where a config comes from, on both sides.
#[derive(Clone, Debug)]
enum Source {
    Yaml(String),
    Builtin(String),
    File(String),
}

impl Source {
    /// The port's config, and what it logged while loading it.
    fn port(&self) -> (Arc<Config>, Vec<Vec<u8>>) {
        capture(|| match self {
            Source::Yaml(text) => Config::create_from_stream(text.as_bytes()).unwrap(),
            Source::Builtin(name) => Config::create_from_builtin_config(name).unwrap(),
            Source::File(path) => Config::create_from_file(path).unwrap(),
        })
    }

    /// The wheel's call that loads the config, stored as `name`.
    fn wheel(&self, name: &str) -> Value {
        let (call, arg) = match self {
            Source::Yaml(text) => ("CreateFromStream", text),
            Source::Builtin(name) => ("CreateFromBuiltinConfig", name),
            Source::File(path) => ("CreateFromFile", path),
        };
        json!({"call": call, "on": "Config", "args": [arg], "as": name})
    }

    fn label(&self) -> String {
        match self {
            Source::Yaml(text) => format!("yaml#{:016x}", fxhash(text.as_bytes())),
            Source::Builtin(name) => name.clone(),
            Source::File(path) => path.rsplit('/').take(2).collect::<Vec<_>>().join("<"),
        }
    }
}

/// FNV-1a, to label generated configs.
fn fxhash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

/// A config of upstream's tests (tests/data/files/configs), by its absolute path.
fn upstream_config(relative: &str) -> Source {
    let path = ocio_testkit::paths::upstream_dir()
        .join("tests/data/files/configs")
        .join(relative);
    Source::File(path.to_string_lossy().replace('\\', "/"))
}

/// The context a case gives one side.
#[derive(Clone, Debug)]
enum ContextChoice {
    /// The config's current context.
    Current,
    /// A copy of the config's current context with the variable set.
    Edited(&'static str, &'static str),
    /// A new context (`Context()`) with the variable set.
    Fresh(&'static str, &'static str),
}

impl ContextChoice {
    fn port(&self, config: &Config) -> Arc<Context> {
        match self {
            ContextChoice::Current => config.current_context().get(),
            ContextChoice::Edited(name, value) => {
                let mut context = (*config.current_context().get()).clone();
                context.set_string_var(name, Some(value.as_bytes()));
                Arc::new(context)
            }
            ContextChoice::Fresh(name, value) => {
                let mut context = Context::new();
                context.set_string_var(name, Some(value.as_bytes()));
                Arc::new(context)
            }
        }
    }

    /// Appends the wheel's calls that make the context, stored as `name`, of the config stored
    /// as `config`.
    fn wheel(&self, calls: &mut Vec<Value>, config: &str, name: &str) {
        match self {
            ContextChoice::Current => {
                calls.push(json!({"call": "getCurrentContext", "on": config, "as": name}));
            }
            ContextChoice::Edited(var, value) => {
                let current = format!("{name}_current");
                calls.push(json!({"call": "getCurrentContext", "on": config, "as": current}));
                calls.push(json!({"copy": current, "as": name}));
                calls.push(json!({"call": "setStringVar", "on": name, "args": [var, value]}));
            }
            ContextChoice::Fresh(var, value) => {
                calls.push(json!({"new": "Context", "as": name}));
                calls.push(json!({"call": "setStringVar", "on": name, "args": [var, value]}));
            }
        }
    }
}

/// One call of `GetProcessorFromConfigs`: the overload (1 to 8, upstream's order) and its
/// arguments.
#[derive(Clone, Debug)]
struct Case {
    overload: usize,
    src_name: String,
    src_interchange: String,
    dst_name: String,
    display: String,
    view: String,
    dst_interchange: String,
    dir: TransformDirection,
    src_context: ContextChoice,
    dst_context: ContextChoice,
}

/// The names a case picks from in `config`: every color space (inactive ones included), every
/// role, every alias, names cut by a NUL, a missing name and `""`.
fn names(config: &Config) -> Vec<String> {
    let mut out: Vec<String> = (0..config
        .num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All))
        .map(|i| {
            text(config.color_space_name_by_index_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All,
                i,
            ))
        })
        .collect();
    let mut aliases = Vec::new();
    for name in &out {
        let cs = config
            .color_space(name)
            .expect("a color space of the config");
        aliases.extend((0..cs.num_aliases()).map(|i| text(cs.alias(i))));
    }
    out.extend(aliases);
    out.extend((0..config.num_roles()).map(|i| text(config.role_name(i))));
    // Names cut by a NUL, as the library reads a C string: a color space's, and a missing one.
    if let Some(first) = out.first().cloned() {
        out.push(format!("{first}\0no_such_space"));
    }
    out.push("no_such_space\0".to_string());
    out.push("no_such_space".to_string());
    out.push(String::new());
    out
}

/// The interchange names a case picks from: the interchange roles' names, the scene-referred
/// color spaces that have no transform (and are no data), and [`names`].
fn interchange_names(config: &Config) -> Vec<String> {
    let mut out = vec![
        "aces_interchange".to_string(),
        "cie_xyz_d65_interchange".to_string(),
    ];
    out.extend((0..config.num_roles()).map(|i| text(config.role_color_space_by_index(i))));
    out.extend(names(config));
    out
}

/// The (display, view) pairs a case picks from: every view of every display, active or not,
/// a missing view of each, and a missing display.
fn display_views(config: &Config) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for d in 0..config.num_displays_all() {
        let display = text(config.display_all(d));
        for t in [ocio::ViewType::DisplayDefined, ocio::ViewType::Shared] {
            for v in 0..config.num_views_of_type(t, &display) {
                pairs.push((display.clone(), text(config.view_of_type(t, &display, v))));
            }
        }
        pairs.push((display, "no_such_view".to_string()));
    }
    pairs.push(("no_such_display".to_string(), "no_such_view".to_string()));
    pairs
}

/// The contexts a case picks from for a side.
const CONTEXTS: [ContextChoice; 6] = [
    ContextChoice::Current,
    ContextChoice::Current,
    ContextChoice::Edited("CS", "lin_inactive"),
    ContextChoice::Edited("SHOT", "shot2"),
    ContextChoice::Fresh("CS", "curve"),
    ContextChoice::Fresh("SHOT", "shot1"),
];

/// `n` cases from `src` to `dst`, the overloads in turn, the rest drawn by `seed`.
fn cases(src: &Config, dst: &Config, seed: u64, n: usize) -> Vec<Case> {
    let mut rng = Rng(seed);
    let (src_names, dst_names) = (names(src), names(dst));
    let (src_inter, dst_inter) = (interchange_names(src), interchange_names(dst));
    let pairs = display_views(dst);
    (0..n)
        .map(|i| {
            let (display, view) = rng.pick(&pairs);
            Case {
                overload: 1 + i % 8,
                src_name: rng.pick(&src_names),
                src_interchange: rng.pick(&src_inter),
                dst_name: rng.pick(&dst_names),
                display,
                view,
                dst_interchange: rng.pick(&dst_inter),
                dir: if rng.below(2) == 0 {
                    TransformDirection::Inverse
                } else {
                    TransformDirection::Forward
                },
                src_context: rng.pick(&CONTEXTS),
                dst_context: rng.pick(&CONTEXTS),
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

/// The port's processor of `case`.
fn port_processor(
    case: &Case,
    src: &Config,
    dst: &Config,
    src_ctx: &Context,
    dst_ctx: &Context,
) -> ocio::Result<Arc<Processor>> {
    let c = case;
    match c.overload {
        1 => Config::processor_from_configs(src, &c.src_name, dst, &c.dst_name),
        2 => Config::processor_from_configs_with_contexts(
            src_ctx,
            src,
            &c.src_name,
            dst_ctx,
            dst,
            &c.dst_name,
        ),
        3 => Config::processor_from_configs_with_interchange(
            src,
            &c.src_name,
            &c.src_interchange,
            dst,
            &c.dst_name,
            &c.dst_interchange,
        ),
        4 => Config::processor_from_configs_with_interchange_with_contexts(
            src_ctx,
            src,
            &c.src_name,
            &c.src_interchange,
            dst_ctx,
            dst,
            &c.dst_name,
            &c.dst_interchange,
        ),
        5 => Config::processor_from_configs_to_display_view(
            src,
            &c.src_name,
            dst,
            &c.display,
            &c.view,
            c.dir,
        ),
        6 => Config::processor_from_configs_to_display_view_with_contexts(
            src_ctx,
            src,
            &c.src_name,
            dst_ctx,
            dst,
            &c.display,
            &c.view,
            c.dir,
        ),
        7 => Config::processor_from_configs_to_display_view_with_interchange(
            src,
            &c.src_name,
            &c.src_interchange,
            dst,
            &c.display,
            &c.view,
            &c.dst_interchange,
            c.dir,
        ),
        8 => Config::processor_from_configs_to_display_view_with_interchange_with_contexts(
            src_ctx,
            src,
            &c.src_name,
            &c.src_interchange,
            dst_ctx,
            dst,
            &c.display,
            &c.view,
            &c.dst_interchange,
            c.dir,
        ),
        _ => unreachable!("8 overloads"),
    }
}

/// The wheel's positional arguments of `case`, the configs stored as "src" and "dst" and the
/// contexts as `src_ctx` and `dst_ctx`.
fn wheel_args(case: &Case, src_ctx: &str, dst_ctx: &str) -> Value {
    let c = case;
    let (src, dst) = (json!({"ref": "src"}), json!({"ref": "dst"}));
    let (sc, dc) = (json!({"ref": src_ctx}), json!({"ref": dst_ctx}));
    let dir = json!({"enum": dir_name(c.dir)});
    match c.overload {
        1 => json!([src, c.src_name, dst, c.dst_name]),
        2 => json!([sc, src, c.src_name, dc, dst, c.dst_name]),
        3 => json!([
            src,
            c.src_name,
            c.src_interchange,
            dst,
            c.dst_name,
            c.dst_interchange
        ]),
        4 => json!([
            sc,
            src,
            c.src_name,
            c.src_interchange,
            dc,
            dst,
            c.dst_name,
            c.dst_interchange
        ]),
        5 => json!([src, c.src_name, dst, c.display, c.view, dir]),
        6 => json!([sc, src, c.src_name, dc, dst, c.display, c.view, dir]),
        7 => json!([
            src,
            c.src_name,
            c.src_interchange,
            dst,
            c.display,
            c.view,
            c.dst_interchange,
            dir
        ]),
        8 => json!([
            sc,
            src,
            c.src_name,
            c.src_interchange,
            dc,
            dst,
            c.display,
            c.view,
            c.dst_interchange,
            dir
        ]),
        _ => unreachable!("8 overloads"),
    }
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

/// What a test reached: [`Reached`], and the overloads that made a processor and that raised.
#[derive(Default)]
struct Coverage {
    reached: Reached,
    made_by: [usize; 8],
    refused_by: [usize; 8],
    /// The messages the wheel logged making the processors.
    logs: std::collections::BTreeSet<String>,
    /// Per case in order, the classes of the wheel's processor's group, or `None` where it
    /// raised.
    wheel_groups: Vec<Option<Vec<String>>>,
}

/// Compares `cases` from `src` to `dst` with the wheel, `per_request` a request.
fn check(
    label: &str,
    src: &Source,
    dst: &Source,
    cases: &[Case],
    per_request: usize,
    coverage: &mut Coverage,
    failures: &mut Vec<String>,
) {
    for (chunk_index, chunk) in cases.chunks(per_request).enumerate() {
        let label = format!("{label} #{chunk_index}");
        let (src_config, src_log) = src.port();
        let (dst_config, dst_log) = dst.port();
        let mut calls = vec![src.wheel("src"), dst.wheel("dst")];
        let mut plan = Vec::new();
        for (k, case) in chunk.iter().enumerate() {
            let (sc, dc) = (format!("sctx{k}"), format!("dctx{k}"));
            let first = calls.len();
            case.src_context.wheel(&mut calls, "src", &sc);
            case.dst_context.wheel(&mut calls, "dst", &dc);
            let src_ctx = case.src_context.port(&src_config);
            let dst_ctx = case.dst_context.port(&dst_config);
            let proc_name = format!("proc{k}");
            let at = calls.len();
            calls.push(json!({"call": "GetProcessorFromConfigs", "on": "Config",
                "args": wheel_args(case, &sc, &dc), "as": proc_name}));
            let (made, port_log) =
                capture(|| port_processor(case, &src_config, &dst_config, &src_ctx, &dst_ctx));
            let applies = match &made {
                Ok(proc) => apply_calls(&mut calls, &proc_name, proc, &dynamic_sets(proc)),
                Err(_) => Vec::new(),
            };
            plan.push((k, case, first, at, made, port_log, applies));
        }
        let response = Oracle::get().call("object_calls", json!({"calls": calls}), &[]);
        let result = &response.result;
        assert_eq!(result["config"], Value::Null, "{label}: {result}");
        let results = result["calls"].as_array().expect("calls");
        assert_eq!(results.len(), calls.len(), "{label}");
        // The configs load on both sides, with the same log.
        for (i, port_log) in [(0, &src_log), (1, &dst_log)] {
            assert!(
                results[i].get("result").is_some(),
                "{label}: {}",
                results[i]
            );
            assert_eq!(log(&results[i]["log"]), *port_log, "{label}: config {i}");
        }
        for (k, case, first, at, made, port_log, applies) in plan {
            // Making the contexts never raises.
            for call in &results[first..at] {
                assert!(call.get("result").is_some(), "{label}: {call}");
            }
            let mut problems = Vec::new();
            for message in log(&results[at]["log"]) {
                coverage
                    .logs
                    .insert(String::from_utf8_lossy(&message).into_owned());
            }
            coverage
                .wheel_groups
                .push(results[at].get("result").map(|r| {
                    ProcessorOpsReply::from_response(Response {
                        result: json!({"processor": r["processor"]}),
                        blobs: response.blobs.clone(),
                    })
                    .processor()
                    .classes()
                    .into_iter()
                    .map(String::from)
                    .collect()
                }));
            let before = (coverage.reached.made, coverage.reached.refused);
            compare_processor_call(
                &response,
                results,
                at,
                &made,
                &port_log,
                applies,
                &mut coverage.reached,
                &mut problems,
            );
            if coverage.reached.made > before.0 {
                coverage.made_by[case.overload - 1] += 1;
            }
            if coverage.reached.refused > before.1 {
                coverage.refused_by[case.overload - 1] += 1;
            }
            if !problems.is_empty() {
                failures.push(format!(
                    "{label} case {k} {case:?}: {}",
                    problems.join("\n    ")
                ));
            }
        }
    }
}

/// Generated cases between every ordered pair of upstream's three test configs and the
/// generated ones (interchange roles valid, naming missing spaces, naming a data space).
#[test]
fn processors_between_generated_configs_match_the_wheel() {
    let sources = [
        Source::Yaml(SIMPLE_CONFIG1.to_string()),
        Source::Yaml(SIMPLE_CONFIG2.to_string()),
        Source::Yaml(SIMPLE_CONFIG3.to_string()),
        Source::Yaml(EDGE_CONFIG.to_string()),
        Source::Yaml(ghost_config()),
        Source::Yaml(data_interchange_config()),
    ];
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    for (i, src) in sources.iter().enumerate() {
        for (j, dst) in sources.iter().enumerate() {
            let (src_config, _) = src.port();
            let (dst_config, _) = dst.port();
            let seed = 0x9b1b_0000 + (i * 16 + j) as u64;
            let label = format!("{i}->{j}");
            let cases = cases(&src_config, &dst_config, seed, 48);
            check(&label, src, dst, &cases, 24, &mut coverage, &mut failures);
        }
    }
    report("config_from_configs_generated", &failures);
    let r = &coverage.reached;
    assert_eq!(r.pixels, 2 * r.made, "pixels compared");
    for k in 0..8 {
        assert!(
            coverage.made_by[k] >= 20 && coverage.refused_by[k] >= 20,
            "overload {}: {} made, {} refused",
            k + 1,
            coverage.made_by[k],
            coverage.refused_by[k]
        );
    }
    assert!(r.groups.len() >= 60, "{} group shapes", r.groups.len());
    // An empty processor (a data space), and both interchange spaces.
    assert!(r.groups.contains(&Vec::new()), "{:#?}", r.groups);
    for message in [
        "is missing from the source and/or destination config.",
        "' refers to a color space that is missing in the source config.",
        "' refers to a color space that is missing in the destination config.",
        "' is missing in the source config.",
        "' is missing in the destination config.",
        "' that is missing in the source config.",
        "' that is missing in the destination config.",
        "Could not find source interchange color space",
        "Could not find destination interchange color space",
        "Could not find source color space",
        "Could not find destination color space",
        "display color space not found.",
    ] {
        assert!(
            r.messages.iter().any(|m| m.contains(message)),
            "no case raised {message:?}: {:#?}",
            r.messages
        );
    }
}

/// Explicit cases: the inverse direction of a data color space to a display and view
/// (docs/improvements.md, I-376), the same through a data interchange space, and two dynamic
/// properties of one type, one from each config, which the concatenation warns about.
#[test]
fn explicit_processors_match_the_wheel() {
    let edge = Source::Yaml(EDGE_CONFIG.to_string());

    let base = Case {
        overload: 7,
        src_name: "raw".to_string(),
        src_interchange: "lin_a".to_string(),
        dst_name: String::new(),
        display: "disp".to_string(),
        view: "plain".to_string(),
        dst_interchange: "lin_a".to_string(),
        dir: TransformDirection::Inverse,
        src_context: ContextChoice::Current,
        dst_context: ContextChoice::Current,
    };
    let mut cases = Vec::new();
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        cases.push(Case {
            dir,
            ..base.clone()
        });
        cases.push(Case {
            dir,
            src_name: "lin_b".to_string(),
            src_interchange: "raw".to_string(),
            ..base.clone()
        });
        cases.push(Case {
            dir,
            src_name: "ec_dyn".to_string(),
            view: "ec".to_string(),
            ..base.clone()
        });
    }
    cases.push(Case {
        overload: 3,
        src_name: "ec_dyn".to_string(),
        dst_name: "ec_dyn".to_string(),
        ..base.clone()
    });
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    check(
        "explicit",
        &edge,
        &edge,
        &cases,
        cases.len(),
        &mut coverage,
        &mut failures,
    );
    report("config_from_configs_explicit", &failures);
    // The cases reach the branches they are for, as the wheel's processors show: the forward
    // data space gives an empty processor, its inverse doesn't (I-376), the inverse through the
    // data interchange space does; the dynamic gamma on both sides is logged.
    let g = &coverage.wheel_groups;
    assert_eq!(g.len(), cases.len());
    assert!(g.iter().all(Option::is_some), "{g:?}");
    assert_eq!(g[0].as_deref(), Some(&[][..]), "{g:?}");
    assert!(g[3].as_ref().is_some_and(|c| !c.is_empty()), "{g:?}");
    assert_eq!(g[4].as_deref(), Some(&[][..]), "{g:?}");
    assert!(
        coverage
            .logs
            .iter()
            .any(|m| m.contains("dynamic property can only be there once")),
        "{:#?}",
        coverage.logs
    );
}

/// Generated cases between every built-in config and the default one, both ways, and from
/// upstream's first test config to each.
#[test]
fn processors_between_builtin_configs_match_the_wheel() {
    let registry = BuiltinConfigRegistry::get();
    let default = Source::Builtin("ocio://default".to_string());
    let simple = Source::Yaml(SIMPLE_CONFIG1.to_string());
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    for i in 0..registry.num_builtin_configs() {
        let name = text(registry.builtin_config_name(i).unwrap());
        let builtin = Source::Builtin(name.clone());
        for (k, (src, dst)) in [
            (&builtin, &default),
            (&default, &builtin),
            (&simple, &builtin),
        ]
        .into_iter()
        .enumerate()
        {
            let (src_config, _) = src.port();
            let (dst_config, _) = dst.port();
            let seed = 0x9b1b_1000 + (i * 4 + k) as u64;
            let label = format!("{name} {k}");
            let cases = cases(&src_config, &dst_config, seed, 16);
            check(&label, src, dst, &cases, 16, &mut coverage, &mut failures);
        }
    }
    report("config_from_configs_builtin", &failures);
    let r = &coverage.reached;
    assert_eq!(r.pixels, 2 * r.made, "pixels compared");
    assert!(
        r.made >= 100 && r.refused >= 50,
        "{} made, {} refused",
        r.made,
        r.refused
    );
}

/// Generated cases from and to upstream's configs of tests/data/files/configs (context
/// variables in search paths and file paths, the merge tests' configs), each with itself and
/// with upstream's first test config.
#[test]
fn processors_between_upstream_test_configs_match_the_wheel() {
    let simple = Source::Yaml(SIMPLE_CONFIG1.to_string());
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    let files = [
        "context_test1/config.ocio",
        "mergeconfigs/base_config.yaml",
        "mergeconfigs/input_config.yaml",
        "mergeconfigs/base_colorspaces_config.yaml",
        "mergeconfigs/input_colorspaces_config.yaml",
        "mergeconfigs/merged1/base1.ocio",
        "mergeconfigs/merged1/input1.ocio",
        "mergeconfigs/merged3/base.ocio",
        "mergeconfigs/merged3/input.ocio",
    ];
    for (i, file) in files.iter().enumerate() {
        let source = upstream_config(file);
        for (k, (src, dst)) in [(&source, &source), (&source, &simple), (&simple, &source)]
            .into_iter()
            .enumerate()
        {
            let (src_config, _) = src.port();
            let (dst_config, _) = dst.port();
            let seed = 0x9b1b_2000 + (i * 4 + k) as u64;
            let label = format!("{} {k}", src.label());
            let cases = cases(&src_config, &dst_config, seed, 24);
            check(&label, src, dst, &cases, 24, &mut coverage, &mut failures);
        }
    }
    report("config_from_configs_upstream", &failures);
    let r = &coverage.reached;
    assert_eq!(r.pixels, 2 * r.made, "pixels compared");
    assert!(
        r.made >= 60 && r.refused >= 60,
        "{} made, {} refused",
        r.made,
        r.refused
    );
}

/// The processor takes the source config's cache flags: with dynamic properties shared
/// (`PROCESSOR_CACHE_DEFAULT`) a second CPU processor of a dynamic processor is the first, so
/// it applies a dynamic gamma set on the first; without sharing (`PROCESSOR_CACHE_ENABLED`) or
/// without the cache (`PROCESSOR_CACHE_OFF`) it is a new one, with the default gamma. The
/// pixels of the second CPU processor, after the gamma is set on the first, are compared.
#[test]
fn the_source_config_cache_flags_match_the_wheel() {
    let edge = Source::Yaml(EDGE_CONFIG.to_string());
    let gamma = 1.7f64;
    let mut wheel_results = Vec::new();
    for (name, flags) in [
        ("PROCESSOR_CACHE_DEFAULT", ProcessorCacheFlags::DEFAULT),
        ("PROCESSOR_CACHE_ENABLED", ProcessorCacheFlags::ENABLED),
        ("PROCESSOR_CACHE_OFF", ProcessorCacheFlags::OFF),
    ] {
        let (src, _) = edge.port();
        let (dst, _) = edge.port();
        src.set_processor_cache_flags(flags);
        let port = (|| -> ocio::Result<Vec<f32>> {
            let proc = Config::processor_from_configs_with_interchange(
                &src, "ec_dyn", "lin_a", &dst, "lin_b", "lin_a",
            )?;
            let first = proc.default_cpu_processor()?;
            match first.get_dynamic_property(DynamicPropertyType::Gamma)? {
                DynamicPropertyRcPtr::Double(p) => p.set_value(gamma),
                other => panic!("gamma is no double: {other:?}"),
            }
            let second = proc.default_cpu_processor()?;
            let mut data = pixels();
            let width = data.len() / 4;
            {
                let mut img = PackedImageDesc::new(&mut data[..], width, 1, 4)?;
                second.apply(&mut img)?;
            }
            Ok(data)
        })()
        .unwrap();
        let pixel_values: Vec<Value> = pixels()
            .iter()
            .map(|&v| json!({"f64": f64::from(v).to_bits()}))
            .collect();
        let mut calls = vec![edge.wheel("src"), edge.wheel("dst")];
        calls.extend([
            json!({"call": "setProcessorCacheFlags", "on": "src", "args": [{"enum": name}]}),
            json!({"call": "GetProcessorFromConfigs", "on": "Config",
                   "args": [{"ref": "src"}, "ec_dyn", "lin_a", {"ref": "dst"}, "lin_b", "lin_a"],
                   "as": "proc"}),
            json!({"call": "getDefaultCPUProcessor", "on": "proc", "as": "first"}),
            json!({"call": "getDynamicProperty", "on": "first",
                   "args": [{"enum": "DYNAMIC_PROPERTY_GAMMA"}], "as": "dp"}),
            json!({"call": "setDouble", "on": "dp", "args": [{"f64": gamma.to_bits()}]}),
            json!({"call": "getDefaultCPUProcessor", "on": "proc", "as": "second"}),
            json!({"call": "applyRGBA", "on": "second", "args": [pixel_values]}),
        ]);
        let response = Oracle::get().call("object_calls", json!({"calls": calls}), &[]);
        let results = response.result["calls"].as_array().expect("calls").clone();
        assert_eq!(results.len(), calls.len(), "{name}");
        let applied = &results[results.len() - 1];
        let wheel_bits: Vec<u64> = applied["result"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: {applied}"))
            .iter()
            .map(|v| v["f64"].as_u64().expect("an f64"))
            .collect();
        let port_bits: Vec<u64> = port.iter().map(|&v| f64::from(v).to_bits()).collect();
        assert_eq!(wheel_bits, port_bits, "{name}");
        wheel_results.push(wheel_bits);
    }
    // The flags change what the wheel applies: shared, the gamma set on the first CPU
    // processor reaches the second; not shared, it doesn't.
    assert_ne!(wheel_results[0], wheel_results[1]);
    assert_eq!(wheel_results[1], wheel_results[2]);
}
