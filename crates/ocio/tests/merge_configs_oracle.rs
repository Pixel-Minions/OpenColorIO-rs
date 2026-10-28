// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The merge sweep of chunk 9.2j (`docs/cards/phase9.md`, WP 9.2): whole merges against the
//! wheel, through the oracle's `object_calls`, each merged config compared by `serialize()`
//! byte for byte, with the merge's log and any exception.
//!
//! - `ConfigMergingHelpers.MergeConfigs` over upstream's merge configs
//!   (`tests/data/files/configs/mergeconfigs/`, either way round), an OCIOZ archive, and pairs
//!   of built-in configs: every strategy (6) for each section (7) and as the default, each with
//!   the input-first, error-on-conflict, avoid-duplicates and adjust-reference-space options
//!   (16 combinations in the `full` tier, one per strategy, in turn, in `quick`), and with the
//!   overrides (name, description, search path, environment, active displays and views,
//!   inactive color spaces) and family prefixes;
//! - `ConfigMerger.mergeConfigs` over upstream's OCIOM files (chains of merges, an earlier
//!   merge's output a later merge's base), every merged config, with each color spaces
//!   strategy;
//! - `ConfigMergingHelpers.MergeColorSpace` of each color space of the input configs;
//! - the configs that are `None` (null pointers the functions check).
//!
//! The duplicate search and the reference space converters apply CPU processors to test values
//! (ConfigUtils), so this target belongs to the `cpu-tests` alias.
//!
//! Each test starts with empty port caches, before any config setup, matching the fresh
//! Python process of its wheel batch. The guard lasts through the whole test. Archive file
//! hashes and parsed LUT files intentionally share filename-only caches (I-209;
//! PathUtils.cpp:64-115 and FileTransform.cpp:798-888 @ v2.5.2). Their warm behavior remains
//! within each test and is covered by `ocioz_read_oracle::two_archives_in_one_process`.

mod common;

use std::sync::{Arc, Mutex, PoisonError};

use common::app_helpers::capture;
use ocio::{
    Config, ConfigMerger, ConfigMergingParameters, ExceptionKind, MergeStrategies,
    config_merging_helpers,
};
use ocio_testkit::Oracle;
use ocio_testkit::battery::Tier;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, log};
use serde_json::{Value, json};

/// Serialize the empty-cache boundary across this file's five tests.
static CACHES: Mutex<()> = Mutex::new(());

/// Upstream's test files directory (`GetTestFilesDir`) joined with `rel`, with forward
/// slashes.
fn test_file(rel: &str) -> String {
    ocio_testkit::paths::upstream_dir()
        .join("tests/data/files")
        .join(rel)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Where a config comes from.
#[derive(Debug, Clone)]
enum Source {
    /// A file of upstream's test files.
    File(&'static str),
    /// A built-in config.
    Builtin(&'static str),
}

impl Source {
    /// The wheel's call that makes the config, stored as `name`.
    fn call(&self, name: &str) -> Value {
        match self {
            Source::File(rel) => {
                json!({"call": "CreateFromFile", "on": "Config", "args": [test_file(rel)], "as": name})
            }
            Source::Builtin(uri) => {
                json!({"call": "CreateFromBuiltinConfig", "on": "Config", "args": [uri], "as": name})
            }
        }
    }

    /// The port's config.
    fn port(&self) -> Arc<Config> {
        match self {
            Source::File(rel) => Config::create_from_file(test_file(rel)).expect("a config"),
            Source::Builtin(uri) => Config::create_from_builtin_config(uri).expect("a config"),
        }
    }
}

/// The base and input configs of the merges.
const PAIRS: &[(&str, Source, Source)] = &[
    (
        "base_config and input_config",
        Source::File("configs/mergeconfigs/base_config.yaml"),
        Source::File("configs/mergeconfigs/input_config.yaml"),
    ),
    (
        "input_config and base_config",
        Source::File("configs/mergeconfigs/input_config.yaml"),
        Source::File("configs/mergeconfigs/base_config.yaml"),
    ),
    (
        "color spaces configs",
        Source::File("configs/mergeconfigs/base_colorspaces_config.yaml"),
        Source::File("configs/mergeconfigs/input_colorspaces_config.yaml"),
    ),
    (
        "merged1",
        Source::File("configs/mergeconfigs/merged1/base1.ocio"),
        Source::File("configs/mergeconfigs/merged1/input1.ocio"),
    ),
    (
        "merged1 the other way",
        Source::File("configs/mergeconfigs/merged1/input1.ocio"),
        Source::File("configs/mergeconfigs/merged1/base1.ocio"),
    ),
    (
        "merged2",
        Source::File("configs/mergeconfigs/merged2/base.ocio"),
        Source::File("configs/mergeconfigs/merged2/input.ocio"),
    ),
    (
        "merged3",
        Source::File("configs/mergeconfigs/merged3/base.ocio"),
        Source::File("configs/mergeconfigs/merged3/input.ocio"),
    ),
    (
        "an archive and merged3's input",
        Source::File("configs/context_test1/context_test1_linux.ocioz"),
        Source::File("configs/mergeconfigs/merged3/input.ocio"),
    ),
    (
        "two CG configs",
        Source::Builtin("ocio://cg-config-v2.2.0_aces-v1.3_ocio-v2.4"),
        Source::Builtin("ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5"),
    ),
    (
        "a studio and a CG config",
        Source::Builtin("ocio://studio-config-v4.0.0_aces-v2.0_ocio-v2.5"),
        Source::Builtin("ocio://cg-config-v1.0.0_aces-v1.3_ocio-v2.1"),
    ),
    (
        "the latest CG config and merged1's input",
        Source::Builtin("ocio://cg-config-latest"),
        Source::File("configs/mergeconfigs/merged1/input1.ocio"),
    ),
];

/// A setter of a strategy of [`ConfigMergingParameters`].
type StrategySetter = fn(&ConfigMergingParameters, MergeStrategies);

/// The default strategy and the sections' strategies: the wheel's setter and the port's.
const SECTIONS: [(&str, StrategySetter); 8] = [
    (
        "setDefaultStrategy",
        ConfigMergingParameters::set_default_strategy,
    ),
    ("setRoles", ConfigMergingParameters::set_roles),
    ("setFileRules", ConfigMergingParameters::set_file_rules),
    (
        "setDisplayViews",
        ConfigMergingParameters::set_display_views,
    ),
    (
        "setViewTransforms",
        ConfigMergingParameters::set_view_transforms,
    ),
    ("setLooks", ConfigMergingParameters::set_looks),
    ("setColorspaces", ConfigMergingParameters::set_colorspaces),
    (
        "setNamedTransforms",
        ConfigMergingParameters::set_named_transforms,
    ),
];

/// The strategies, by the wheel's names.
const STRATEGIES: [(&str, MergeStrategies); 6] = [
    ("STRATEGY_PREFER_INPUT", MergeStrategies::PreferInput),
    ("STRATEGY_PREFER_BASE", MergeStrategies::PreferBase),
    ("STRATEGY_INPUT_ONLY", MergeStrategies::InputOnly),
    ("STRATEGY_BASE_ONLY", MergeStrategies::BaseOnly),
    ("STRATEGY_REMOVE", MergeStrategies::Remove),
    ("STRATEGY_UNSPECIFIED", MergeStrategies::Unspecified),
];

/// `{"enum": "ConfigMergingParameters.<name>"}`.
fn strategy_arg(name: &str) -> Value {
    json!({"enum": format!("ConfigMergingParameters.{name}")})
}

/// The options of a merge: one strategy set (an index into [`SECTIONS`] and one into
/// [`STRATEGIES`]), the four options (bits of `options`: input first, error on conflict, avoid
/// duplicates, adjust the input reference space), and whether the overrides and family
/// prefixes are set.
#[derive(Debug, Clone, Copy)]
struct Opts {
    section: usize,
    strategy: usize,
    options: usize,
    overrides: bool,
}

impl Opts {
    fn input_first(&self) -> bool {
        self.options & 1 != 0
    }
    fn error_on_conflict(&self) -> bool {
        self.options & 2 != 0
    }
    fn avoid_duplicates(&self) -> bool {
        self.options & 4 != 0
    }
    fn adjust(&self) -> bool {
        self.options & 8 != 0
    }
}

/// The overrides a case sets: the wheel's setter and its argument.
const OVERRIDES: &[(&str, &str)] = &[
    ("setName", "merged name"),
    ("setDescription", "merged description"),
    ("setSearchPath", "ovr1:luts"),
    ("setActiveDisplays", "DISP_1, sRGB - Display"),
    (
        "setActiveViews",
        "VIEW_1, ACES 2.0 - SDR 100 nits (Rec.709)",
    ),
    ("setInactiveColorSpaces", "ACES2065-1, missing, srgb_tx"),
    ("setInputFamilyPrefix", "In/"),
    ("setBaseFamilyPrefix", "Base/Prefix/"),
];

/// The environment the overrides set.
const ENVIRONMENT_OVERRIDE: &[(&str, &str)] = &[("SHOT", "shot2"), ("OVR", "value")];

/// The wheel's calls that make the parameters `p` of `opts`.
fn params_calls(opts: &Opts) -> Vec<Value> {
    let mut calls = vec![
        json!({"call": "Create", "on": "ConfigMergingParameters", "as": "p"}),
        json!({"call": "setInputFirst", "on": "p", "args": [opts.input_first()]}),
        json!({"call": "setErrorOnConflict", "on": "p", "args": [opts.error_on_conflict()]}),
        json!({"call": "setAvoidDuplicates", "on": "p", "args": [opts.avoid_duplicates()]}),
        json!({"call": "setAdjustInputReferenceSpace", "on": "p", "args": [opts.adjust()]}),
        json!({"call": SECTIONS[opts.section].0, "on": "p",
               "args": [strategy_arg(STRATEGIES[opts.strategy].0)]}),
    ];
    if opts.overrides {
        for (setter, value) in OVERRIDES {
            calls.push(json!({"call": setter, "on": "p", "args": [value]}));
        }
        for (name, value) in ENVIRONMENT_OVERRIDE {
            calls.push(json!({"call": "addEnvironmentVar", "on": "p", "args": [name, value]}));
        }
    }
    calls
}

/// The port's parameters of `opts`.
fn port_params(opts: &Opts) -> ConfigMergingParameters {
    let p = ConfigMergingParameters::new().expect("Create");
    p.set_input_first(opts.input_first());
    p.set_error_on_conflict(opts.error_on_conflict());
    p.set_avoid_duplicates(opts.avoid_duplicates());
    p.set_adjust_input_reference_space(opts.adjust());
    (SECTIONS[opts.section].1)(&p, STRATEGIES[opts.strategy].1);
    if opts.overrides {
        p.set_name(OVERRIDES[0].1);
        p.set_description(OVERRIDES[1].1);
        p.set_search_path(OVERRIDES[2].1);
        p.set_active_displays(OVERRIDES[3].1)
            .expect("setActiveDisplays");
        p.set_active_views(OVERRIDES[4].1).expect("setActiveViews");
        p.set_inactive_color_spaces(OVERRIDES[5].1);
        p.set_input_family_prefix(OVERRIDES[6].1);
        p.set_base_family_prefix(OVERRIDES[7].1);
        for (name, value) in ENVIRONMENT_OVERRIDE {
            p.add_environment_var(name, Some(value.as_bytes()));
        }
    }
    p
}

/// The wheel's exception type of a port's error.
fn exception_type(e: &ocio::Exception) -> &'static str {
    match e.kind() {
        ExceptionKind::Exception => "Exception",
        ExceptionKind::MissingFile => "ExceptionMissingFile",
        kind => panic!("a merge error of kind {kind:?}: {e}"),
    }
}

/// An outcome, as both sides report it: the serialized config, or the exception's type and
/// message; with the call's log.
#[derive(Debug, PartialEq)]
enum Outcome {
    Text(Vec<u8>),
    Raised(String, Vec<u8>),
}

/// The outcome of the wheel's call that returns a config (`call`), whose `serialize()` is
/// `serialize` (the call after it).
#[track_caller]
fn wheel_outcome(call: &Value, serialize: &Value) -> Outcome {
    match call.get("exception") {
        Some(e) => Outcome::Raised(
            e["type"].as_str().expect("a type").to_string(),
            bytes(&e["message"]),
        ),
        None => Outcome::Text(
            serialize["result"]
                .as_str()
                .unwrap_or_else(|| panic!("serialize: {serialize}"))
                .as_bytes()
                .to_vec(),
        ),
    }
}

/// The port's outcome of a merge.
fn port_outcome(result: ocio::Result<Config>) -> Outcome {
    match result.and_then(|c| c.serialize()) {
        Ok(text) => Outcome::Text(text),
        Err(e) => Outcome::Raised(exception_type(&e).to_string(), e.what().to_vec()),
    }
}

/// Runs the wheel's requests in one batch: each request's calls.
fn wheel(requests: &[Vec<Value>]) -> Vec<Vec<Value>> {
    let args: Vec<Value> = requests.iter().map(|c| json!({"calls": c})).collect();
    let batch: Vec<BatchCall<'_>> = args
        .iter()
        .map(|args| BatchCall {
            cmd: "object_calls",
            args: args.clone(),
            blobs: Vec::new(),
        })
        .collect();
    let responses = Oracle::get().batch(&batch, true);
    assert_eq!(responses.len(), requests.len());
    responses
        .into_iter()
        .map(|r| {
            r.unwrap_or_else(|e| panic!("{e}")).result["calls"]
                .as_array()
                .expect("the calls")
                .clone()
        })
        .collect()
}

/// What the comparisons saw: mismatches (written to a file when the test fails), and counts
/// for the minimum-count checks.
#[derive(Default)]
struct Seen {
    mismatches: Vec<String>,
    texts: std::collections::BTreeSet<Vec<u8>>,
    raised: usize,
    logged: usize,
}

impl Seen {
    /// Compares a case's outcome and log on both sides.
    fn compare(&mut self, label: &str, wheel: (Outcome, Vec<u8>), port: (Outcome, Vec<u8>)) {
        let mut problems = Vec::new();
        if wheel.1 != port.1 {
            problems.push(format!(
                "log:\n--- wheel\n{}\n--- port\n{}",
                String::from_utf8_lossy(&wheel.1),
                String::from_utf8_lossy(&port.1)
            ));
        }
        if !port.1.is_empty() {
            self.logged += 1;
        }
        match (&wheel.0, &port.0) {
            (Outcome::Text(w), Outcome::Text(p)) => {
                if w != p {
                    problems.push(format!(
                        "serialize:\n--- wheel\n{}\n--- port\n{}",
                        String::from_utf8_lossy(w),
                        String::from_utf8_lossy(p)
                    ));
                }
                self.texts.insert(p.clone());
            }
            (w, p) => {
                if w != p {
                    problems.push(format!("outcome: wheel {w:?}, port {p:?}"));
                } else {
                    self.raised += 1;
                }
            }
        }
        if !problems.is_empty() {
            self.mismatches
                .push(format!("{label}: {}", problems.join("\n")));
        }
    }

    /// Fails with the mismatches, written to a file under the target directory too.
    #[track_caller]
    fn assert_no_mismatch(&self, name: &str, cases: usize) {
        if self.mismatches.is_empty() {
            return;
        }
        let path = std::env::temp_dir().join(format!("merge_configs_oracle_{name}.txt"));
        let _ = std::fs::write(&path, self.mismatches.join("\n\n"));
        panic!(
            "{} of {cases} cases differ (all in {}):\n{}",
            self.mismatches.len(),
            path.display(),
            self.mismatches[..self.mismatches.len().min(3)].join("\n\n")
        );
    }
}

/// The options of the merges of a pair: each strategy of each section and of the default,
/// with every combination of the four options (`full`) or one, in turn (`quick`); then each
/// default strategy with the overrides.
fn pair_opts(tier: Tier) -> Vec<Opts> {
    let mut out = Vec::new();
    let mut turn = 0;
    for section in 0..SECTIONS.len() {
        for strategy in 0..STRATEGIES.len() {
            let options: Vec<usize> = if tier == Tier::Quick {
                turn += 1;
                vec![turn % 16]
            } else {
                (0..16).collect()
            };
            for options in options {
                out.push(Opts {
                    section,
                    strategy,
                    options,
                    overrides: false,
                });
            }
        }
    }
    for strategy in 0..STRATEGIES.len() {
        out.push(Opts {
            section: 0,
            strategy,
            options: 15 - strategy,
            overrides: true,
        });
    }
    out
}

/// `MergeConfigs` over the pairs of configs and the options of [`pair_opts`].
#[test]
fn merges_match_the_wheel() {
    let _cache_guard = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    ocio::clear_all_caches();
    let tier = Tier::current();
    let mut cases = Vec::new();
    for (pair, (label, _, _)) in PAIRS.iter().enumerate() {
        for opts in pair_opts(tier) {
            cases.push((pair, *label, opts));
        }
    }

    let requests: Vec<Vec<Value>> = cases
        .iter()
        .map(|(pair, _, opts)| {
            let (_, base, input) = &PAIRS[*pair];
            let mut calls = vec![base.call("merged"), base.call("base"), input.call("input")];
            calls.extend(params_calls(opts));
            calls.extend([
                json!({"call": "MergeConfigs", "on": "ConfigMergingHelpers",
                       "args": [{"ref": "p"}, {"ref": "base"}, {"ref": "input"}], "as": "merged"}),
                json!({"call": "serialize", "on": "merged"}),
            ]);
            calls
        })
        .collect();
    let responses = wheel(&requests);

    let configs: Vec<(Arc<Config>, Arc<Config>)> = PAIRS
        .iter()
        .map(|(_, base, input)| (base.port(), input.port()))
        .collect();

    let mut seen = Seen::default();
    for ((pair, label, opts), calls) in cases.iter().zip(&responses) {
        for call in &calls[..calls.len() - 2] {
            assert!(call.get("result").is_some(), "{label} {opts:?}: {call}");
        }
        let merge = &calls[calls.len() - 2];
        let wheel_side = (
            wheel_outcome(merge, &calls[calls.len() - 1]),
            log(&merge["log"]).concat(),
        );

        let (base, input) = &configs[*pair];
        let params = port_params(opts);
        let (result, logged) = capture(|| {
            config_merging_helpers::merge_configs(Some(&params), Some(base), Some(input))
        });
        let port_side = (port_outcome(result), logged.concat());
        seen.compare(&format!("{label} {opts:?}"), wheel_side, port_side);
    }
    seen.assert_no_mismatch("merges", cases.len());

    // Merged configs of every pair, merges that raise (conflicts with error on conflict) and
    // merges that log.
    assert!(
        seen.texts.len() >= 150,
        "{} different merged configs",
        seen.texts.len()
    );
    assert!(seen.raised >= 30, "{} merges raised", seen.raised);
    assert!(seen.logged >= 200, "{} merges logged", seen.logged);
}

/// The OCIOM files of upstream's merge tests.
const OCIOM_FILES: &[&str] = &[
    "configs/mergeconfigs/merged1/merged1.ociom",
    "configs/mergeconfigs/merged2/merged.ociom",
    "configs/mergeconfigs/merged3/merged.ociom",
    "configs/mergeconfigs/merged4/merged.ociom",
];

/// `ConfigMerger.mergeConfigs` over upstream's OCIOM files, as they are and with each color
/// spaces strategy set on every merge: every merged config, or the exception.
#[test]
fn ociom_merges_match_the_wheel() {
    let _cache_guard = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    ocio::clear_all_caches();
    // (file, the color spaces strategy set, if any)
    let mut cases: Vec<(&str, Option<usize>)> = Vec::new();
    for file in OCIOM_FILES {
        cases.push((file, None));
        for strategy in 0..STRATEGIES.len() {
            cases.push((file, Some(strategy)));
        }
    }
    // The merges each file holds (upstream's files: merged2 holds two).
    let num_merges = |file: &str| if file.contains("merged2") { 2 } else { 1 };

    let requests: Vec<Vec<Value>> = cases
        .iter()
        .map(|(file, strategy)| {
            let mut calls = vec![json!({"call": "CreateFromFile", "on": "ConfigMerger",
                                        "args": [test_file(file)], "as": "m"})];
            for i in 0..num_merges(file) {
                calls.push(json!({"call": "getParams", "on": "m", "args": [i],
                                  "as": format!("p{i}")}));
                if let Some(s) = strategy {
                    calls.push(json!({"call": "setColorspaces", "on": format!("p{i}"),
                                      "args": [strategy_arg(STRATEGIES[*s].0)]}));
                }
            }
            calls.push(json!({"call": "mergeConfigs", "on": "m", "as": "merged"}));
            calls.push(json!({"call": "getNumMergedConfigs", "on": "merged"}));
            for i in 0..num_merges(file) {
                calls.push(
                    json!({"call": "getMergedConfig", "on": "merged", "args": [i],
                                  "as": format!("c{i}")}),
                );
                calls.push(json!({"call": "serialize", "on": format!("c{i}")}));
            }
            calls
        })
        .collect();
    let responses = wheel(&requests);

    let mut seen = Seen::default();
    for ((file, strategy), calls) in cases.iter().zip(&responses) {
        let label = format!("{file} {strategy:?}");
        let merger = ConfigMerger::from_file(test_file(file)).expect("CreateFromFile");
        let n = num_merges(file);
        if let Some(s) = strategy {
            for i in 0..n {
                merger
                    .params(i as i32)
                    .expect("getParams")
                    .set_colorspaces(STRATEGIES[*s].1);
            }
        }
        let (result, logged) = capture(|| merger.merge_configs());
        let at = 1 + n * (1 + usize::from(strategy.is_some()));
        let merge = &calls[at];
        let wheel_log = log(&merge["log"]).concat();
        match (merge.get("exception"), result) {
            (Some(_), Err(e)) => {
                let wheel_side = (wheel_outcome(merge, &Value::Null), wheel_log);
                let port_side = (
                    Outcome::Raised(exception_type(&e).to_string(), e.what().to_vec()),
                    logged.concat(),
                );
                seen.compare(&label, wheel_side, port_side);
            }
            (None, Ok(merged)) => {
                assert_eq!(
                    calls[at + 1]["result"],
                    json!(merged.num_merged_configs()),
                    "{label}: getNumMergedConfigs"
                );
                for i in 0..n {
                    let wheel_side = (
                        wheel_outcome(&calls[at + 2 + 2 * i], &calls[at + 3 + 2 * i]),
                        if i == 0 {
                            wheel_log.clone()
                        } else {
                            Vec::new()
                        },
                    );
                    let config = merged.merged_config(i as i32).expect("a merged config");
                    let port_side = (
                        Outcome::Text(config.serialize().expect("serialize")),
                        if i == 0 { logged.concat() } else { Vec::new() },
                    );
                    seen.compare(&format!("{label} merge {i}"), wheel_side, port_side);
                }
            }
            (w, p) => seen
                .mismatches
                .push(format!("{label}: outcome: wheel {w:?}, port {p:?}")),
        }
    }
    seen.assert_no_mismatch("ociom", cases.len());
    assert!(
        seen.texts.len() >= 12,
        "{} different merged configs",
        seen.texts.len()
    );
    assert!(seen.logged >= 10, "{} merges logged", seen.logged);
}

/// `MergeColorSpace` of each color space of the pairs' input configs into their base configs,
/// with the default parameters and with the color spaces strategy `PreferBase`, and the
/// configs or color spaces that are `None`.
#[test]
fn single_color_space_merges_match_the_wheel() {
    let _cache_guard = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    ocio::clear_all_caches();
    // (pair, color space index, PreferBase, which argument is None: 0 none, 1 base, 2 cs)
    let mut cases = Vec::new();
    let configs: Vec<(Arc<Config>, Arc<Config>)> = PAIRS[..8]
        .iter()
        .map(|(_, base, input)| (base.port(), input.port()))
        .collect();
    for (pair, (_, input)) in configs.iter().enumerate() {
        let n = input.num_color_spaces_with(
            ocio::SearchReferenceSpaceType::All,
            ocio::ColorSpaceVisibility::All,
        );
        for i in 0..n {
            for prefer_base in [false, true] {
                cases.push((pair, i, prefer_base, 0));
            }
        }
        cases.push((pair, 0, false, 1));
        cases.push((pair, 0, false, 2));
    }

    let requests: Vec<Vec<Value>> = cases
        .iter()
        .map(|(pair, i, prefer_base, none)| {
            let (_, base, input) = &PAIRS[*pair];
            let mut calls = vec![
                base.call("merged"),
                base.call("base"),
                input.call("input"),
                json!({"call": "Create", "on": "ConfigMergingParameters", "as": "p"}),
            ];
            if *prefer_base {
                calls.push(json!({"call": "setColorspaces", "on": "p",
                                  "args": [strategy_arg("STRATEGY_PREFER_BASE")]}));
            }
            // (The name is an input of both sides: the binding has no getColorSpaceNameByIndex.)
            let name = configs[*pair].1.color_space_name_by_index_with(
                ocio::SearchReferenceSpaceType::All,
                ocio::ColorSpaceVisibility::All,
                *i,
            );
            calls.push(json!({"call": "getColorSpace", "on": "input",
                              "args": [String::from_utf8_lossy(name)], "as": "cs"}));
            let base_arg = if *none == 1 {
                Value::Null
            } else {
                json!({"ref": "base"})
            };
            let cs_arg = if *none == 2 {
                Value::Null
            } else {
                json!({"ref": "cs"})
            };
            calls.extend([
                json!({"call": "MergeColorSpace", "on": "ConfigMergingHelpers",
                       "args": [{"ref": "p"}, base_arg, cs_arg], "as": "merged"}),
                json!({"call": "serialize", "on": "merged"}),
            ]);
            calls
        })
        .collect();
    let responses = wheel(&requests);

    let mut seen = Seen::default();
    for ((pair, i, prefer_base, none), calls) in cases.iter().zip(&responses) {
        let label = format!(
            "{} cs {i} prefer base {prefer_base} none {none}",
            PAIRS[*pair].0
        );
        for call in &calls[..calls.len() - 2] {
            assert!(call.get("result").is_some(), "{label}: {call}");
        }
        assert!(
            !calls[calls.len() - 3]["result"].is_null(),
            "{label}: getColorSpace"
        );
        let merge = &calls[calls.len() - 2];
        let wheel_side = (
            wheel_outcome(merge, &calls[calls.len() - 1]),
            log(&merge["log"]).concat(),
        );

        let (base, input) = &configs[*pair];
        let params = ConfigMergingParameters::new().expect("Create");
        if *prefer_base {
            params.set_colorspaces(MergeStrategies::PreferBase);
        }
        let name = input.color_space_name_by_index_with(
            ocio::SearchReferenceSpaceType::All,
            ocio::ColorSpaceVisibility::All,
            *i,
        );
        let cs = input.color_space(name);
        let base_arg = if *none == 1 { None } else { Some(&**base) };
        let cs_arg = if *none == 2 { None } else { cs };
        let (result, logged) =
            capture(|| config_merging_helpers::merge_color_space(&params, base_arg, cs_arg));
        let port_side = (port_outcome(result), logged.concat());
        seen.compare(&label, wheel_side, port_side);
    }
    seen.assert_no_mismatch("single", cases.len());
    assert!(
        seen.texts.len() >= 60,
        "{} different merged configs",
        seen.texts.len()
    );
    assert!(seen.raised >= 16, "{} merges raised", seen.raised);
    assert!(seen.logged >= 30, "{} merges logged", seen.logged);
}

/// `MergeConfigs` with configs or parameters that are `None`: the functions' own checks.
#[test]
fn merges_of_none_match_the_wheel() {
    let _cache_guard = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    ocio::clear_all_caches();
    // (params, base, input) given or None.
    let cases = [
        (true, false, true),
        (true, true, false),
        (true, false, false),
        (false, true, true),
    ];
    let source = &PAIRS[0];
    let requests: Vec<Vec<Value>> = cases
        .iter()
        .map(|(p, b, i)| {
            let arg = |given: bool, name: &str| {
                if given {
                    json!({"ref": name})
                } else {
                    Value::Null
                }
            };
            vec![
                source.1.call("merged"),
                source.1.call("base"),
                source.2.call("input"),
                json!({"call": "Create", "on": "ConfigMergingParameters", "as": "p"}),
                json!({"call": "MergeConfigs", "on": "ConfigMergingHelpers",
                       "args": [arg(*p, "p"), arg(*b, "base"), arg(*i, "input")],
                       "as": "merged"}),
                json!({"call": "serialize", "on": "merged"}),
            ]
        })
        .collect();
    let responses = wheel(&requests);

    let (base, input) = (source.1.port(), source.2.port());
    let mut seen = Seen::default();
    for ((p, b, i), calls) in cases.iter().zip(&responses) {
        let merge = &calls[calls.len() - 2];
        assert!(merge.get("exception").is_some(), "{p} {b} {i}: {merge}");
        let wheel_side = (
            wheel_outcome(merge, &calls[calls.len() - 1]),
            log(&merge["log"]).concat(),
        );
        let params = ConfigMergingParameters::new().expect("Create");
        let (result, logged) = capture(|| {
            config_merging_helpers::merge_configs(
                p.then_some(&params),
                b.then_some(&*base),
                i.then_some(&*input),
            )
        });
        let port_side = (port_outcome(result), logged.concat());
        seen.compare(&format!("{p} {b} {i}"), wheel_side, port_side);
    }
    seen.assert_no_mismatch("none", cases.len());
    assert_eq!(seen.raised, cases.len());
}

/// A merge whose reference space converter can't find a LUT file: the wheel reports the
/// `ExceptionMissingFile` as a plain `Exception` (docs/improvements.md, I-437), and so does the
/// port.
#[test]
fn a_missing_file_in_a_merge_is_a_plain_exception() {
    let _cache_guard = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    ocio::clear_all_caches();
    const BASE: &str = "ocio_profile_version: 2\n\nroles:\n  default: raw\n  \
                        aces_interchange: ACES2065-1\n\ncolorspaces:\n  - !<ColorSpace>\n    \
                        name: raw\n    isdata: true\n  - !<ColorSpace>\n    name: ACES2065-1\n";
    const INPUT: &str = "ocio_profile_version: 2\n\nsearch_path: nowhere\n\nroles:\n  \
                         default: raw\n  aces_interchange: lut_space\n\ncolorspaces:\n  - \
                         !<ColorSpace>\n    name: raw\n    isdata: true\n  - !<ColorSpace>\n    \
                         name: lut_space\n    to_scene_reference: !<FileTransform> {src: \
                         missing.clf}\n";
    let calls = vec![
        json!({"call": "CreateFromStream", "on": "Config", "args": [BASE], "as": "merged"}),
        json!({"call": "CreateFromStream", "on": "Config", "args": [BASE], "as": "base"}),
        json!({"call": "CreateFromStream", "on": "Config", "args": [INPUT], "as": "input"}),
        json!({"call": "Create", "on": "ConfigMergingParameters", "as": "p"}),
        json!({"call": "MergeConfigs", "on": "ConfigMergingHelpers",
               "args": [{"ref": "p"}, {"ref": "base"}, {"ref": "input"}], "as": "merged"}),
        json!({"call": "serialize", "on": "merged"}),
    ];
    let responses = wheel(&[calls]);
    let calls = &responses[0];
    for call in &calls[..4] {
        assert!(call.get("result").is_some(), "{call}");
    }
    let merge = &calls[4];
    let wheel_side = (wheel_outcome(merge, &calls[5]), log(&merge["log"]).concat());
    assert!(
        matches!(&wheel_side.0, Outcome::Raised(kind, _) if kind == "Exception"),
        "{wheel_side:?}"
    );

    let base = Config::create_from_stream(BASE.as_bytes()).expect("the base config");
    let input = Config::create_from_stream(INPUT.as_bytes()).expect("the input config");
    let params = ConfigMergingParameters::new().expect("Create");
    let (result, logged) =
        capture(|| config_merging_helpers::merge_configs(Some(&params), Some(&base), Some(&input)));
    let port_side = (port_outcome(result), logged.concat());
    let mut seen = Seen::default();
    seen.compare("a missing file", wheel_side, port_side);
    seen.assert_no_mismatch("missing", 1);
    assert_eq!(seen.raised, 1);
}
