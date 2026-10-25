// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ConfigMergingParameters` against the wheel, through the oracle's `object_calls` (chunk
//! 9.2a): sequences of setter calls on a parameters object and on copies of it, and after each
//! one every getter of each object (the environment overrides at indices from -1 past their
//! end) and its text (`repr()`, upstream's `operator<<`), each with its log.
//!
//! The values are strings with commas, spaces, quotes, colons, `""`, a NUL (the binding's
//! `const char *` ends there), UTF-8 and bytes that aren't UTF-8 (the wheel's getters then
//! raise `UnicodeDecodeError`, which the oracle reports with the bytes); lists of active
//! displays and views that open a quote they don't close (the setter's exception); every
//! strategy for every section and as the default; and environment overrides added, replaced
//! and removed (`None`). A copy (`copy.deepcopy`, `createEditableCopy`) shares its overrides
//! with the original (docs/improvements.md, I-386): the sequences change both after copying.

mod common;

use std::sync::Arc;

use common::app_helpers::{Rng, capture};
use ocio::{ConfigMergingParameters, MergeStrategies};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes_arg, hex, log};
use serde_json::{Value, json};

/// The strategies, with the binding's names (`PyMergeConfigs.cpp:174-186` @ v2.5.2).
const STRATEGIES: [(MergeStrategies, &str); 6] = [
    (MergeStrategies::PreferInput, "STRATEGY_PREFER_INPUT"),
    (MergeStrategies::PreferBase, "STRATEGY_PREFER_BASE"),
    (MergeStrategies::InputOnly, "STRATEGY_INPUT_ONLY"),
    (MergeStrategies::BaseOnly, "STRATEGY_BASE_ONLY"),
    (MergeStrategies::Remove, "STRATEGY_REMOVE"),
    (MergeStrategies::Unspecified, "STRATEGY_UNSPECIFIED"),
];

/// The string setters.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Field {
    Base,
    Input,
    Output,
    Name,
    Description,
    InputPrefix,
    BasePrefix,
    SearchPath,
    AddSearchPath,
    ActiveDisplays,
    ActiveViews,
    Inactive,
}

const FIELDS: [Field; 12] = [
    Field::Base,
    Field::Input,
    Field::Output,
    Field::Name,
    Field::Description,
    Field::InputPrefix,
    Field::BasePrefix,
    Field::SearchPath,
    Field::AddSearchPath,
    Field::ActiveDisplays,
    Field::ActiveViews,
    Field::Inactive,
];

impl Field {
    fn setter(self) -> &'static str {
        match self {
            Field::Base => "setBaseConfigName",
            Field::Input => "setInputConfigName",
            Field::Output => "setOutputName",
            Field::Name => "setName",
            Field::Description => "setDescription",
            Field::InputPrefix => "setInputFamilyPrefix",
            Field::BasePrefix => "setBaseFamilyPrefix",
            Field::SearchPath => "setSearchPath",
            Field::AddSearchPath => "addSearchPath",
            Field::ActiveDisplays => "setActiveDisplays",
            Field::ActiveViews => "setActiveViews",
            Field::Inactive => "setInactiveColorSpaces",
        }
    }

    fn set(self, p: &ConfigMergingParameters, v: &[u8]) -> ocio::Result<()> {
        match self {
            Field::Base => p.set_base_config_name(v),
            Field::Input => p.set_input_config_name(v),
            Field::Output => p.set_output_name(v),
            Field::Name => p.set_name(v),
            Field::Description => p.set_description(v),
            Field::InputPrefix => p.set_input_family_prefix(v),
            Field::BasePrefix => p.set_base_family_prefix(v),
            Field::SearchPath => p.set_search_path(v),
            Field::AddSearchPath => p.add_search_path(v),
            Field::ActiveDisplays => return p.set_active_displays(v),
            Field::ActiveViews => return p.set_active_views(v),
            Field::Inactive => p.set_inactive_color_spaces(v),
        }
        Ok(())
    }
}

/// The flags.
const FLAGS: [&str; 4] = [
    "setInputFirst",
    "setErrorOnConflict",
    "setAvoidDuplicates",
    "setAdjustInputReferenceSpace",
];

fn set_flag(p: &ConfigMergingParameters, which: usize, on: bool) {
    match which {
        0 => p.set_input_first(on),
        1 => p.set_error_on_conflict(on),
        2 => p.set_avoid_duplicates(on),
        _ => p.set_adjust_input_reference_space(on),
    }
}

/// The strategy setters: the default, then each section's.
const SECTIONS: [&str; 8] = [
    "setDefaultStrategy",
    "setRoles",
    "setFileRules",
    "setDisplayViews",
    "setViewTransforms",
    "setLooks",
    "setColorspaces",
    "setNamedTransforms",
];

fn set_strategy(p: &ConfigMergingParameters, section: usize, s: MergeStrategies) {
    match section {
        0 => p.set_default_strategy(s),
        1 => p.set_roles(s),
        2 => p.set_file_rules(s),
        3 => p.set_display_views(s),
        4 => p.set_view_transforms(s),
        5 => p.set_looks(s),
        6 => p.set_colorspaces(s),
        _ => p.set_named_transforms(s),
    }
}

/// A setter call.
#[derive(Debug, Clone)]
enum Op {
    Str(Field, Vec<u8>),
    Flag(usize, bool),
    Strategy(usize, usize),
    Env(Vec<u8>, Option<Vec<u8>>),
    /// `copy = copy.deepcopy(params)`: `createEditableCopy`.
    Copy,
}

/// A step: an operation on the parameters (or on their copy).
#[derive(Debug, Clone)]
struct Step {
    on_copy: bool,
    op: Op,
}

/// The string values the steps set.
fn strings() -> Vec<Vec<u8>> {
    vec![
        b"".to_vec(),
        b"a".to_vec(),
        b"base.ocio".to_vec(),
        b"with, comma".to_vec(),
        b"  spaced out  ".to_vec(),
        b"D1, 'D 2'".to_vec(),
        b"\"x, y\", z".to_vec(),
        b"a:b::c:".to_vec(),
        b"/usr/local:.".to_vec(),
        b"ab\0cd".to_vec(),
        "caf\u{e9} \u{2014} \u{4e2d}".as_bytes().to_vec(),
        b"bad \xff\xfe utf-8".to_vec(),
        b"a, 'b".to_vec(),
        b"\"open, c".to_vec(),
        b"i1 , i2 ,, i3".to_vec(),
        b"null".to_vec(),
    ]
}

/// The environment variable names and values the steps add.
fn env_names() -> Vec<Vec<u8>> {
    vec![
        b"X".to_vec(),
        b"Y".to_vec(),
        b"".to_vec(),
        b"a b".to_vec(),
        b"Z\xff".to_vec(),
        b"LONG_NAME".to_vec(),
    ]
}

/// A sequence covering every setter: each string on each field, each flag both ways, each
/// strategy on each section, environment overrides added, replaced, emptied and removed, and a
/// copy changed with the original.
fn coverage_sequence() -> Vec<Step> {
    let mut steps = Vec::new();
    let op = |op| Step { on_copy: false, op };
    for (i, field) in FIELDS.iter().enumerate() {
        for (j, s) in strings().into_iter().enumerate() {
            // Every string on the string fields; the lists see them all too.
            if (i + j) % 2 == 0 || matches!(field, Field::ActiveDisplays | Field::ActiveViews) {
                steps.push(op(Op::Str(*field, s)));
            }
        }
    }
    for flag in 0..FLAGS.len() {
        steps.push(op(Op::Flag(flag, false)));
        steps.push(op(Op::Flag(flag, true)));
    }
    for section in 0..SECTIONS.len() {
        for strategy in 0..STRATEGIES.len() {
            steps.push(op(Op::Strategy(section, strategy)));
        }
    }
    // Sections unspecified under every default strategy.
    for section in 1..SECTIONS.len() {
        steps.push(op(Op::Strategy(section, 5)));
    }
    for strategy in 0..STRATEGIES.len() {
        steps.push(op(Op::Strategy(0, strategy)));
    }
    steps.push(op(Op::Env(b"X".to_vec(), Some(b"1".to_vec()))));
    steps.push(op(Op::Env(b"A".to_vec(), Some(b"".to_vec()))));
    steps.push(op(Op::Env(b"X".to_vec(), Some(b"2".to_vec()))));
    steps.push(op(Op::Env(b"".to_vec(), Some(b"ignored".to_vec()))));
    steps.push(op(Op::Env(b"B\0C".to_vec(), Some(b"v\0w".to_vec()))));
    steps.push(op(Op::Env(b"Z".to_vec(), Some(b"bad \xff".to_vec()))));
    steps.push(op(Op::Env(b"X".to_vec(), None)));
    steps.push(op(Op::Env(b"missing".to_vec(), None)));
    steps.push(op(Op::Copy));
    for on_copy in [true, false] {
        steps.push(Step {
            on_copy,
            op: Op::Str(Field::Name, b"copy name".to_vec()),
        });
        steps.push(Step {
            on_copy,
            op: Op::Str(Field::SearchPath, b"shared:path".to_vec()),
        });
        steps.push(Step {
            on_copy,
            op: Op::Env(b"SHARED".to_vec(), Some(b"yes".to_vec())),
        });
        steps.push(Step {
            on_copy,
            op: Op::Str(Field::ActiveViews, b"v1, v2".to_vec()),
        });
        steps.push(Step {
            on_copy,
            op: Op::Strategy(2, 3),
        });
        steps.push(Step {
            on_copy,
            op: Op::Flag(1, on_copy),
        });
    }
    steps
}

/// A random sequence, with a copy somewhere.
fn random_sequence(seed: u64, len: usize) -> Vec<Step> {
    let mut rng = Rng(seed);
    let strings = strings();
    let names = env_names();
    let mut copied = false;
    let mut steps = Vec::new();
    for _ in 0..len {
        let op = match rng.below(10) {
            0..=4 => Op::Str(FIELDS[rng.below(FIELDS.len())], rng.pick(&strings)),
            5 => Op::Flag(rng.below(FLAGS.len()), rng.below(2) == 0),
            6 | 7 => Op::Strategy(rng.below(SECTIONS.len()), rng.below(STRATEGIES.len())),
            8 => Op::Env(
                rng.pick(&names),
                rng.maybe(strings.len(), 4).map(|i| strings[i].clone()),
            ),
            _ => Op::Copy,
        };
        if matches!(op, Op::Copy) {
            copied = true;
        }
        let on_copy = copied && rng.below(2) == 0 && !matches!(op, Op::Copy);
        steps.push(Step { on_copy, op });
    }
    steps
}

/// A string argument.
fn arg(v: &[u8]) -> Value {
    bytes_arg(v)
}

/// The wheel's calls of a step.
fn step_calls(step: &Step) -> Vec<Value> {
    let on = if step.on_copy { "copy" } else { "params" };
    match &step.op {
        Op::Str(field, v) => vec![json!({"call": field.setter(), "on": on, "args": [arg(v)]})],
        Op::Flag(which, flag) => vec![json!({"call": FLAGS[*which], "on": on, "args": [flag]})],
        Op::Strategy(section, strategy) => {
            let name = format!("ConfigMergingParameters.{}", STRATEGIES[*strategy].1);
            vec![json!({"call": SECTIONS[*section], "on": on, "args": [{"enum": name}]})]
        }
        Op::Env(name, value) => {
            let value = value.as_deref().map_or(Value::Null, arg);
            vec![json!({"call": "addEnvironmentVar", "on": on, "args": [arg(name), value]})]
        }
        Op::Copy => vec![json!({"copy": "params", "as": "copy"})],
    }
}

/// The string getters, with the port's.
type StrGetter = fn(&ConfigMergingParameters) -> Vec<u8>;

const STRING_GETTERS: [(&str, StrGetter); 11] = [
    (
        "getBaseConfigName",
        ConfigMergingParameters::base_config_name,
    ),
    (
        "getInputConfigName",
        ConfigMergingParameters::input_config_name,
    ),
    ("getOutputName", ConfigMergingParameters::output_name),
    ("getName", ConfigMergingParameters::name),
    ("getDescription", ConfigMergingParameters::description),
    (
        "getInputFamilyPrefix",
        ConfigMergingParameters::input_family_prefix,
    ),
    (
        "getBaseFamilyPrefix",
        ConfigMergingParameters::base_family_prefix,
    ),
    ("getSearchPath", ConfigMergingParameters::search_path),
    (
        "getActiveDisplays",
        ConfigMergingParameters::active_displays,
    ),
    ("getActiveViews", ConfigMergingParameters::active_views),
    (
        "getInactiveColorSpaces",
        ConfigMergingParameters::inactive_color_spaces,
    ),
];

type BoolGetter = fn(&ConfigMergingParameters) -> bool;

const BOOL_GETTERS: [(&str, BoolGetter); 4] = [
    ("isInputFirst", ConfigMergingParameters::is_input_first),
    (
        "isErrorOnConflict",
        ConfigMergingParameters::is_error_on_conflict,
    ),
    (
        "isAvoidDuplicates",
        ConfigMergingParameters::is_avoid_duplicates,
    ),
    (
        "isAdjustInputReferenceSpace",
        ConfigMergingParameters::is_adjust_input_reference_space,
    ),
];

type StrategyGetter = fn(&ConfigMergingParameters) -> MergeStrategies;

const STRATEGY_GETTERS: [(&str, StrategyGetter); 8] = [
    (
        "getDefaultStrategy",
        ConfigMergingParameters::default_strategy,
    ),
    ("getRoles", ConfigMergingParameters::roles),
    ("getFileRules", ConfigMergingParameters::file_rules),
    ("getDisplayViews", ConfigMergingParameters::display_views),
    (
        "getViewTransforms",
        ConfigMergingParameters::view_transforms,
    ),
    ("getLooks", ConfigMergingParameters::looks),
    ("getColorspaces", ConfigMergingParameters::colorspaces),
    (
        "getNamedTransforms",
        ConfigMergingParameters::named_transforms,
    ),
];

/// The environment indices each observation reads: past both ends of any list the sequences
/// make (they add at most 6 names).
const ENV_INDICES: std::ops::RangeInclusive<i32> = -1..=7;

/// The wheel's calls that observe the object `on`: every getter, then `repr()`.
fn observe_calls(on: &str) -> Vec<Value> {
    let mut calls = Vec::new();
    let names = STRING_GETTERS
        .iter()
        .map(|g| g.0)
        .chain(BOOL_GETTERS.iter().map(|g| g.0))
        .chain(STRATEGY_GETTERS.iter().map(|g| g.0))
        .chain(["getNumEnvironmentVars"]);
    for name in names {
        calls.push(json!({"call": name, "on": on}));
    }
    for i in ENV_INDICES {
        calls.push(json!({"call": "getEnvironmentVar", "on": on, "args": [i]}));
        calls.push(json!({"call": "getEnvironmentVarValue", "on": on, "args": [i]}));
    }
    calls.push(json!({"call": "__repr__", "on": on}));
    calls
}

/// A string the port returned, as the wheel's call reports it: the `str`, or, for bytes that
/// aren't UTF-8, the binding's `UnicodeDecodeError` with the bytes.
fn string_entry(v: &[u8]) -> Value {
    match std::str::from_utf8(v) {
        Ok(s) => json!({"result": s}),
        Err(_) => json!({"undecodable": hex(v), "stage": "call"}),
    }
}

/// The port's entries of [`observe_calls`] (without their logs).
fn observe(p: &ConfigMergingParameters) -> Vec<Value> {
    let mut out = Vec::new();
    for (_, g) in STRING_GETTERS {
        out.push(string_entry(&g(p)));
    }
    for (_, g) in BOOL_GETTERS {
        out.push(json!({"result": g(p)}));
    }
    for (_, g) in STRATEGY_GETTERS {
        let s = g(p);
        let name = STRATEGIES.iter().find(|x| x.0 == s).unwrap().1;
        out.push(json!({"result": {"enum": name}}));
    }
    out.push(json!({"result": p.num_environment_vars()}));
    for i in ENV_INDICES {
        out.push(string_entry(&p.environment_var(i)));
        out.push(string_entry(&p.environment_var_value(i)));
    }
    out.push(string_entry(&p.to_bytes()));
    out
}

/// The port's entry of a step's call (without its log); `None` for the copy, whose entry is
/// the copied object written out.
fn run_step(
    step: &Step,
    params: &ConfigMergingParameters,
    copy: &mut Option<ConfigMergingParameters>,
) -> Option<Value> {
    let target = if step.on_copy {
        copy.as_ref().expect("a copy").clone()
    } else {
        params.clone()
    };
    let outcome = match &step.op {
        Op::Str(field, v) => field.set(&target, v),
        Op::Flag(which, on) => {
            set_flag(&target, *which, *on);
            Ok(())
        }
        Op::Strategy(section, strategy) => {
            set_strategy(&target, *section, STRATEGIES[*strategy].0);
            Ok(())
        }
        Op::Env(name, value) => {
            target.add_environment_var(name, value.as_deref());
            Ok(())
        }
        Op::Copy => {
            *copy = Some(params.create_editable_copy().expect("the copy"));
            return None;
        }
    };
    Some(match outcome {
        Ok(()) => json!({"result": null}),
        Err(e) => json!({"exception": {"type": "Exception", "message": bytes_arg(e.what())},
                         "stage": "call"}),
    })
}

/// What one sequence covered.
#[derive(Debug, Default)]
struct Seen {
    calls: usize,
    exceptions: usize,
    undecodable: usize,
    copied_steps: usize,
}

/// The wheel's entries of one request, read in order.
struct Entries<'a> {
    sequence: usize,
    wheel: &'a [Value],
    at: usize,
}

impl<'a> Entries<'a> {
    /// The next entry, for the call `what`.
    fn next(&mut self, what: &str) -> &'a Value {
        let entry = self
            .wheel
            .get(self.at)
            .unwrap_or_else(|| panic!("sequence {}: no entry for {what}", self.sequence));
        self.at += 1;
        entry
    }

    /// Compares the port's observation of `p` with the wheel's next entries: each value, and
    /// the logs (empty on both sides).
    fn check_observation(&mut self, label: &str, p: &ConfigMergingParameters, seen: &mut Seen) {
        let n = self.sequence;
        let (port, logged) = capture(|| observe(p));
        assert_eq!(
            logged,
            Vec::<Vec<u8>>::new(),
            "sequence {n}: {label}: the port logged"
        );
        let calls = observe_calls("x");
        assert_eq!(port.len(), calls.len());
        for (entry, call) in port.into_iter().zip(calls) {
            let w = self.next(label);
            let w_log = log(&w["log"]);
            let mut w_entry = w.clone();
            w_entry.as_object_mut().expect("an entry").remove("log");
            assert_eq!(w_entry, entry, "sequence {n}: {label}: {call}");
            assert!(
                w_log.is_empty(),
                "sequence {n}: {label}: {call} logged {w_log:?}"
            );
            if entry.get("undecodable").is_some() {
                seen.undecodable += 1;
            }
            seen.calls += 1;
        }
    }
}

/// Runs the sequences on both sides, and compares every call's entry and log.
fn compare_sequences(sequences: &[Vec<Step>]) -> Seen {
    // The oracle's environment is empty: so is the port's here, as OCIO reads it for the
    // override config (`Config::Create`).
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&[u8], &[u8]>(&[]))));

    let requests: Vec<Value> = sequences
        .iter()
        .map(|steps| {
            let mut calls = vec![json!({"call": "Create", "on": "ConfigMergingParameters",
                                        "as": "params"})];
            calls.extend(observe_calls("params"));
            let mut copied = false;
            for step in steps {
                calls.extend(step_calls(step));
                copied |= matches!(step.op, Op::Copy);
                calls.extend(observe_calls("params"));
                if copied {
                    calls.extend(observe_calls("copy"));
                }
            }
            json!({"calls": calls})
        })
        .collect();
    let batch: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|args| BatchCall {
            cmd: "object_calls",
            args: args.clone(),
            blobs: Vec::new(),
        })
        .collect();
    let responses = Oracle::get().batch(&batch, true);
    assert_eq!(responses.len(), sequences.len());

    let mut seen = Seen::default();
    for (n, (steps, response)) in sequences.iter().zip(responses).enumerate() {
        let response = response
            .unwrap_or_else(|e| panic!("sequence {n}: {e}"))
            .result;
        assert_eq!(response["config"], Value::Null, "sequence {n}: {response}");
        let mut wheel = Entries {
            sequence: n,
            wheel: response["calls"].as_array().expect("the calls"),
            at: 0,
        };

        let (params, logged) = capture(|| ConfigMergingParameters::new().expect("Create"));
        let created = wheel.next("Create");
        assert_eq!(
            created["result"]["class"],
            json!("ConfigMergingParameters"),
            "sequence {n}: {created}"
        );
        assert_eq!(log(&created["log"]), logged, "sequence {n}: Create's log");
        wheel.check_observation("after Create", &params, &mut seen);

        let mut copy: Option<ConfigMergingParameters> = None;
        for (s, step) in steps.iter().enumerate() {
            let label = format!("step {s} {step:?}");
            let (port, logged) = capture(|| run_step(step, &params, &mut copy));
            let w = wheel.next(&label);
            assert_eq!(log(&w["log"]), logged, "sequence {n}: {label}: the log");
            match port {
                Some(entry) => {
                    let mut w_entry = w.clone();
                    w_entry.as_object_mut().expect("an entry").remove("log");
                    assert_eq!(w_entry, entry, "sequence {n}: {label}");
                    if entry.get("exception").is_some() {
                        seen.exceptions += 1;
                    }
                }
                // The copy, written out with its getters: a string that isn't UTF-8 among
                // them stops the writing (stage "write"), and the copy is stored all the same.
                None => assert!(
                    w["result"]["class"] == json!("ConfigMergingParameters")
                        || (w["stage"] == json!("write") && w["undecodable"].is_string()),
                    "sequence {n}: {label}: {w}"
                ),
            }
            if step.on_copy {
                seen.copied_steps += 1;
            }
            wheel.check_observation(&format!("{label}, params"), &params, &mut seen);
            if let Some(c) = &copy {
                wheel.check_observation(&format!("{label}, copy"), c, &mut seen);
            }
        }
        assert_eq!(
            wheel.at,
            wheel.wheel.len(),
            "sequence {n}: entries left over"
        );
    }
    set_thread_env_provider(None);
    seen
}

/// Every setter and getter of `ConfigMergingParameters`, and its text, as the wheel's: the
/// coverage sequence and random ones.
#[test]
fn parameters_match_the_wheel() {
    let mut sequences = vec![coverage_sequence()];
    for seed in 1..=6u64 {
        sequences.push(random_sequence(0x9e37_79b9_7f4a_7c15 ^ seed, 40));
    }
    let seen = compare_sequences(&sequences);
    // Not vacuous: thousands of calls, exceptions (the unclosed quotes), strings that aren't
    // UTF-8, and steps on a copy.
    assert!(seen.calls > 10_000, "{seen:?}");
    assert!(seen.exceptions >= 4, "{seen:?}");
    assert!(seen.undecodable >= 10, "{seen:?}");
    assert!(seen.copied_steps >= 6, "{seen:?}");
}

/// `ConfigMergingParameters.Create()` fails as `Config::Create` does, for an environment list of
/// active displays or views that opens a double quote it doesn't close before a comma
/// (`ConfigMergingParameters::Impl::Impl` makes its override config with `Config::Create`,
/// MergeConfigsHelpers.h:71 @ v2.5.2).
///
/// `object_calls` makes its config (`Config.CreateRaw()`, through `Config::Create`) in the
/// request's environment before any call, so in such an environment the wheel's request fails
/// there, with `Config::Create`'s exception: the text `Create` raises. In an environment the
/// config accepts, the wheel makes the parameters.
#[test]
fn create_fails_on_the_environment_as_the_wheel() {
    let mut failed = 0;
    let mut made = 0;
    for (name, value) in [
        ("OCIO_ACTIVE_DISPLAYS", "\"open, c"),
        ("OCIO_ACTIVE_VIEWS", "\"v1, v2"),
        ("OCIO_ACTIVE_DISPLAYS", "a, 'b"),
    ] {
        let response = Oracle::get()
            .call(
                "object_calls",
                json!({"env": {name: value},
                       "calls": [{"call": "Create", "on": "ConfigMergingParameters",
                                  "as": "params"}]}),
                &[],
            )
            .result;
        set_thread_env_provider(Some(Arc::new(MapEnv::from_entries(&[(name, value)]))));
        let port = ConfigMergingParameters::new();
        set_thread_env_provider(None);
        match port {
            Ok(_) => {
                assert_eq!(
                    response["config"],
                    Value::Null,
                    "{name}={value}: {response}"
                );
                assert_eq!(
                    response["calls"][0]["result"]["class"],
                    json!("ConfigMergingParameters"),
                    "{name}={value}: {response}"
                );
                made += 1;
            }
            Err(e) => {
                failed += 1;
                assert_eq!(
                    response["config"]["exception"],
                    json!({"type": "Exception", "message": bytes_arg(e.what())}),
                    "{name}={value}: {response}"
                );
                assert_eq!(response["calls"], json!([]), "{response}");
            }
        }
    }
    // Both outcomes are reached.
    assert_eq!((failed, made), (2, 1));
}
