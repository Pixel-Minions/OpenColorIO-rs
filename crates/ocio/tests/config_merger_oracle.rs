// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ConfigMerger` against the wheel, through the oracle's `object_calls` (chunk 9.2b):
//!
//! - sequences of calls on a merger, on the parameters it holds (added, then changed through
//!   the caller's handle or through `getParams`: one shared object, owner decision P9-2) and on
//!   copies of it (`copy.deepcopy`, `createEditableCopy`: a deep copy whose parameters still
//!   share their overrides, docs/improvements.md I-386), and after each call every getter of
//!   each merger, the text of each of its parameters, and its OCIOM text (`serialize()` and
//!   `repr()`), or the writer's exception;
//! - the OCIOM text of parameters whose strings aren't UTF-8 or hold YAML's special characters,
//!   and of lists the writer can't split ("Error building YAML: ...");
//! - `None` parameters, which the wheel holds and returns (its copy, merge and writer then
//!   crash: the port's errors, docs/improvements.md U-176, are checked on the port alone);
//! - where the merger finds a merge's base and input configs (`ConfigMerger::Impl::loadConfig`):
//!   files along the search paths and the working directory, built-in configs, and earlier
//!   merges' outputs. The wheel shows it through `mergeConfigs` (9.2j): its exception when it
//!   finds no config ("Could not load the base or the input config") or names a merge whose
//!   output doesn't exist yet (`std::out_of_range`, docs/improvements.md I-387), and otherwise
//!   the merged config's working directory, which is the base config's.

mod common;

use std::sync::Arc;

use common::app_helpers::{Rng, capture};
use ocio::{ConfigMerger, ConfigMergingParameters, ExceptionKind, MergeStrategies};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes_arg, hex, log};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------------------------
// Values

/// A string the port returned, as the wheel's call reports it: the `str`, or, for bytes that
/// aren't UTF-8, the binding's `UnicodeDecodeError` with the bytes.
fn string_entry(v: &[u8]) -> Value {
    match std::str::from_utf8(v) {
        Ok(s) => json!({"result": s}),
        Err(_) => json!({"undecodable": hex(v), "stage": "call"}),
    }
}

/// What a call that returns a string gave: the string, or the exception.
fn text_entry(r: ocio::Result<Vec<u8>>) -> Value {
    match r {
        Ok(v) => string_entry(&v),
        Err(e) => exception_entry(&e),
    }
}

fn exception_entry(e: &ocio::Exception) -> Value {
    let kind = match e.kind() {
        ExceptionKind::Exception => "Exception",
        ExceptionKind::OutOfRange => "IndexError",
        k => panic!("unexpected kind {k:?}"),
    };
    json!({"exception": {"type": kind, "message": bytes_arg(e.what())}, "stage": "call"})
}

/// The wheel's entry without its log, and the log.
fn split_log(entry: &Value) -> (Value, Vec<Vec<u8>>) {
    let log = log(&entry["log"]);
    let mut entry = entry.clone();
    entry.as_object_mut().expect("an entry").remove("log");
    (entry, log)
}

// ---------------------------------------------------------------------------------------------
// Sequences of calls on mergers

/// A setter of the parameters (the ones the OCIOM text shows).
#[derive(Debug, Clone)]
enum ParamOp {
    Base(Vec<u8>),
    Input(Vec<u8>),
    Output(Vec<u8>),
    Name(Vec<u8>),
    InputPrefix(Vec<u8>),
    SearchPath(Vec<u8>),
    Env(Vec<u8>, Vec<u8>),
    ActiveViews(Vec<u8>),
    Inactive(Vec<u8>),
    InputFirst(bool),
    Default(usize),
    Looks(usize),
}

const STRATEGIES: [(MergeStrategies, &str); 6] = [
    (MergeStrategies::PreferInput, "STRATEGY_PREFER_INPUT"),
    (MergeStrategies::PreferBase, "STRATEGY_PREFER_BASE"),
    (MergeStrategies::InputOnly, "STRATEGY_INPUT_ONLY"),
    (MergeStrategies::BaseOnly, "STRATEGY_BASE_ONLY"),
    (MergeStrategies::Remove, "STRATEGY_REMOVE"),
    (MergeStrategies::Unspecified, "STRATEGY_UNSPECIFIED"),
];

impl ParamOp {
    /// The wheel's call on the parameters stored as `on`.
    fn call(&self, on: &str) -> Value {
        let strategy =
            |i: usize| json!({"enum": format!("ConfigMergingParameters.{}", STRATEGIES[i].1)});
        let (name, args) = match self {
            ParamOp::Base(v) => ("setBaseConfigName", json!([bytes_arg(v)])),
            ParamOp::Input(v) => ("setInputConfigName", json!([bytes_arg(v)])),
            ParamOp::Output(v) => ("setOutputName", json!([bytes_arg(v)])),
            ParamOp::Name(v) => ("setName", json!([bytes_arg(v)])),
            ParamOp::InputPrefix(v) => ("setInputFamilyPrefix", json!([bytes_arg(v)])),
            ParamOp::SearchPath(v) => ("setSearchPath", json!([bytes_arg(v)])),
            ParamOp::Env(n, v) => ("addEnvironmentVar", json!([bytes_arg(n), bytes_arg(v)])),
            ParamOp::ActiveViews(v) => ("setActiveViews", json!([bytes_arg(v)])),
            ParamOp::Inactive(v) => ("setInactiveColorSpaces", json!([bytes_arg(v)])),
            ParamOp::InputFirst(b) => ("setInputFirst", json!([b])),
            ParamOp::Default(s) => ("setDefaultStrategy", json!([strategy(*s)])),
            ParamOp::Looks(s) => ("setLooks", json!([strategy(*s)])),
        };
        json!({"call": name, "on": on, "args": args})
    }

    /// The port's call; its outcome as the wheel's entry.
    fn apply(&self, p: &ConfigMergingParameters) -> Value {
        let r = match self {
            ParamOp::Base(v) => {
                p.set_base_config_name(v);
                Ok(())
            }
            ParamOp::Input(v) => {
                p.set_input_config_name(v);
                Ok(())
            }
            ParamOp::Output(v) => {
                p.set_output_name(v);
                Ok(())
            }
            ParamOp::Name(v) => {
                p.set_name(v);
                Ok(())
            }
            ParamOp::InputPrefix(v) => {
                p.set_input_family_prefix(v);
                Ok(())
            }
            ParamOp::SearchPath(v) => {
                p.set_search_path(v);
                Ok(())
            }
            ParamOp::Env(n, v) => {
                p.add_environment_var(n, Some(v));
                Ok(())
            }
            ParamOp::ActiveViews(v) => p.set_active_views(v),
            ParamOp::Inactive(v) => {
                p.set_inactive_color_spaces(v);
                Ok(())
            }
            ParamOp::InputFirst(b) => {
                p.set_input_first(*b);
                Ok(())
            }
            ParamOp::Default(s) => {
                p.set_default_strategy(STRATEGIES[*s].0);
                Ok(())
            }
            ParamOp::Looks(s) => {
                p.set_looks(STRATEGIES[*s].0);
                Ok(())
            }
        };
        match r {
            Ok(()) => json!({"result": null}),
            Err(e) => exception_entry(&e),
        }
    }
}

/// A call of a sequence.
#[derive(Debug, Clone)]
enum Op {
    SetSearchPath(Vec<u8>),
    AddSearchPath(Vec<u8>),
    SetWorkingDir(Vec<u8>),
    SetVersion(u32, u32),
    /// New parameters `p<k>`, set up, then added.
    AddParams(usize, Vec<ParamOp>),
    /// `p<k>` added again (the same object twice).
    AddAgain(usize),
    /// A change through the caller's handle `p<k>`.
    Change(usize, ParamOp),
    /// A change through `getParams(i)` of the merger.
    ChangeVia(i32, ParamOp),
    /// `copy = copy.deepcopy(merger)`.
    Copy,
}

/// A call on the merger, or on its copy.
#[derive(Debug, Clone)]
struct Step {
    on_copy: bool,
    op: Op,
}

/// The strings the sequences set: ordinary names, YAML's special characters, lists, the empty
/// string, colons (search paths split on them), UTF-8 that isn't ASCII.
fn strings() -> Vec<Vec<u8>> {
    [
        "",
        "base.ocio",
        "input.ocio",
        "Merge_1",
        "with: colon",
        "- dash",
        "'quoted'",
        "\"dq\"",
        "#hash",
        "a:b::c:",
        "/abs/path:rel",
        "C:/drive",
        "spaced  ",
        "caf\u{e9} \u{4e2d}",
        "null",
        "true",
        "a, b, \"c, d\"",
        "x\ny",
        // NUL: the setters take a `const char *`, which ends there.
        "a\0b",
        "\0x",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect()
}

fn param_op(rng: &mut Rng, strings: &[Vec<u8>]) -> ParamOp {
    let s = rng.pick(strings);
    match rng.below(12) {
        0 => ParamOp::Base(s),
        1 => ParamOp::Input(s),
        2 => ParamOp::Output(s),
        3 => ParamOp::Name(s),
        4 => ParamOp::InputPrefix(s),
        5 => ParamOp::SearchPath(s),
        6 => ParamOp::Env(rng.pick(&[b"A".to_vec(), b"B".to_vec(), b"Zz".to_vec()]), s),
        7 => ParamOp::ActiveViews(s),
        8 => ParamOp::Inactive(s),
        9 => ParamOp::InputFirst(rng.below(2) == 0),
        10 => ParamOp::Default(rng.below(STRATEGIES.len())),
        _ => ParamOp::Looks(rng.below(STRATEGIES.len())),
    }
}

fn random_sequence(seed: u64, len: usize) -> Vec<Step> {
    let mut rng = Rng(seed);
    let strings = strings();
    let mut steps = Vec::new();
    let mut params = 0;
    let mut copied = false;
    for _ in 0..len {
        let op = match rng.below(12) {
            0 => Op::SetSearchPath(rng.pick(&strings)),
            1 => Op::AddSearchPath(rng.pick(&strings)),
            2 => Op::SetWorkingDir(rng.pick(&strings)),
            3 => Op::SetVersion(rng.below(3) as u32, rng.below(12) as u32),
            4 | 5 => {
                let ops = (0..rng.below(4))
                    .map(|_| param_op(&mut rng, &strings))
                    .collect();
                params += 1;
                Op::AddParams(params - 1, ops)
            }
            6 if params > 0 => Op::AddAgain(rng.below(params)),
            7 | 8 if params > 0 => Op::Change(rng.below(params), param_op(&mut rng, &strings)),
            9 | 10 => Op::ChangeVia(
                rng.below(params + 2) as i32 - 1,
                param_op(&mut rng, &strings),
            ),
            11 if !copied => {
                copied = true;
                Op::Copy
            }
            _ => Op::SetSearchPath(rng.pick(&strings)),
        };
        let on_copy = copied && !matches!(op, Op::Copy) && rng.below(2) == 0;
        steps.push(Step { on_copy, op });
    }
    steps
}

/// A sequence that covers each call: search paths set, added (empty ones too) and split on
/// colons, the working directory, versions, parameters added, added twice, changed through each
/// handle, a copy changed with its original, and lists the writer can't split.
fn coverage_sequence() -> Vec<Step> {
    let op = |op| Step { on_copy: false, op };
    let on_copy = |op| Step { on_copy: true, op };
    let s = |t: &str| t.as_bytes().to_vec();
    vec![
        op(Op::SetSearchPath(s(""))),
        op(Op::AddSearchPath(s(""))),
        op(Op::AddSearchPath(s("/usr/local/configs"))),
        op(Op::SetSearchPath(s("a:b::c:"))),
        op(Op::AddSearchPath(s("."))),
        op(Op::SetWorkingDir(s("/work dir"))),
        // NUL ends each string (`const char *`), whatever follows it.
        op(Op::SetSearchPath(s("p\0:q"))),
        op(Op::AddSearchPath(s("r\0s"))),
        op(Op::AddSearchPath(s("\0t"))),
        op(Op::SetWorkingDir(s("w\0d"))),
        op(Op::SetSearchPath(s("a:b::c:"))),
        op(Op::SetWorkingDir(s("/work dir"))),
        op(Op::SetVersion(1, 0)),
        op(Op::SetVersion(2, 13)),
        op(Op::AddParams(
            0,
            vec![
                ParamOp::Base(s("base0.ocio")),
                ParamOp::Input(s("input0.ocio")),
                ParamOp::Output(s("Merge1")),
                ParamOp::Name(s("my merge")),
                ParamOp::InputPrefix(s("abc")),
                ParamOp::SearchPath(s("abc:def")),
                ParamOp::Env(s("test"), s("valueOther")),
                ParamOp::Env(s("test1"), s("value123")),
                ParamOp::Env(s("empty"), s("")),
                ParamOp::ActiveViews(s("V1, V2")),
                ParamOp::Inactive(s("I1, I2")),
                ParamOp::Default(2),
                ParamOp::Looks(3),
            ],
        )),
        op(Op::AddParams(1, vec![])),
        op(Op::Change(1, ParamOp::Output(s("second")))),
        op(Op::ChangeVia(1, ParamOp::Base(s("Merge1")))),
        op(Op::ChangeVia(0, ParamOp::InputFirst(false))),
        op(Op::ChangeVia(5, ParamOp::Name(s("nobody")))),
        op(Op::AddAgain(0)),
        op(Op::Change(0, ParamOp::Default(5))),
        op(Op::Change(0, ParamOp::Looks(5))),
        op(Op::Copy),
        on_copy(Op::ChangeVia(0, ParamOp::Name(s("copy name")))),
        on_copy(Op::ChangeVia(0, ParamOp::SearchPath(s("shared")))),
        on_copy(Op::ChangeVia(2, ParamOp::Env(s("shared"), s("yes")))),
        on_copy(Op::SetSearchPath(s("copy:only"))),
        on_copy(Op::AddParams(2, vec![ParamOp::Output(s("copy's"))])),
        op(Op::Change(0, ParamOp::Output(s("renamed")))),
        op(Op::Change(1, ParamOp::ActiveViews(s("x, 'y")))),
        // Lists the writer can't split: an inactive list isn't checked when set.
        op(Op::Change(1, ParamOp::Inactive(s("\"x, y")))),
        op(Op::Change(1, ParamOp::Inactive(s("x, y")))),
        op(Op::Change(1, ParamOp::ActiveViews(s("\"open, v")))),
        on_copy(Op::Change(2, ParamOp::Inactive(s("ok, \"bad")))),
    ]
}

/// The calls that observe the merger `on`, given how many parameters it holds.
fn observe_calls(on: &str, num_params: usize) -> Vec<Value> {
    let mut calls = vec![
        json!({"call": "getNumSearchPaths", "on": on}),
        json!({"call": "getWorkingDir", "on": on}),
        json!({"call": "getMajorVersion", "on": on}),
        json!({"call": "getMinorVersion", "on": on}),
        json!({"call": "getNumConfigMergingParameters", "on": on}),
        json!({"call": "getNumMergedConfigs", "on": on}),
        json!({"call": "getMergedConfig", "on": on}),
        json!({"call": "getMergedConfig", "on": on, "args": [0]}),
        json!({"call": "getMergedConfig", "on": on, "args": [-1]}),
        json!({"call": "getParams", "on": on, "args": [-1]}),
        json!({"call": "getParams", "on": on, "args": [num_params]}),
    ];
    for i in -1..=SEARCH_INDICES_PAST {
        calls.push(json!({"call": "getSearchPath", "on": on, "args": [i]}));
    }
    for i in 0..num_params {
        calls.push(json!({"call": "getParams", "on": on, "args": [i], "as": "got"}));
        calls.push(json!({"call": "__repr__", "on": "got"}));
    }
    calls.push(json!({"call": "serialize", "on": on}));
    calls.push(json!({"call": "__repr__", "on": on}));
    calls
}

/// The search path indices past which each observation reads (the sequences make fewer).
const SEARCH_INDICES_PAST: i32 = 9;

/// The port's entries for [`observe_calls`]; `None` where the wheel's entry is the parameters
/// object written out (checked apart).
fn observe(m: &ConfigMerger) -> Vec<Option<Value>> {
    let n = m.num_config_merging_parameters();
    let mut out = vec![
        Some(json!({"result": m.num_search_paths()})),
        Some(string_entry(m.working_dir())),
        Some(json!({"result": m.major_version()})),
        Some(json!({"result": m.minor_version()})),
        Some(json!({"result": n})),
        Some(json!({"result": m.num_merged_configs()})),
        Some(json!({"result": m.last_merged_config().map(|_| "a config")})),
        Some(json!({"result": m.merged_config(0).map(|_| "a config")})),
        Some(json!({"result": m.merged_config(-1).map(|_| "a config")})),
        Some(json!({"result": m.params(-1).map(|_| "params")})),
        Some(json!({"result": m.params(n).map(|_| "params")})),
    ];
    for i in -1..=SEARCH_INDICES_PAST {
        out.push(Some(string_entry(&m.search_path(i))));
    }
    for i in 0..n {
        out.push(None);
        out.push(Some(string_entry(&m.params(i).expect("params").to_bytes())));
    }
    out.push(Some(text_entry(m.serialize())));
    out.push(Some(text_entry(m.serialize())));
    out
}

/// The wheel's calls of a step on `on` (`merger` or `copy`), with the handles `p<k>`.
fn step_calls(step: &Step) -> Vec<Value> {
    let on = if step.on_copy { "copy" } else { "merger" };
    match &step.op {
        Op::SetSearchPath(v) => vec![json!({"call": "setSearchPath", "on": on,
                                            "args": [bytes_arg(v)]})],
        Op::AddSearchPath(v) => vec![json!({"call": "addSearchPath", "on": on,
                                            "args": [bytes_arg(v)]})],
        Op::SetWorkingDir(v) => vec![json!({"call": "setWorkingDir", "on": on,
                                            "args": [bytes_arg(v)]})],
        Op::SetVersion(a, b) => vec![json!({"call": "setVersion", "on": on, "args": [a, b]})],
        Op::AddParams(k, ops) => {
            let name = format!("p{k}");
            let mut calls =
                vec![json!({"call": "Create", "on": "ConfigMergingParameters", "as": name})];
            calls.extend(ops.iter().map(|op| op.call(&name)));
            calls.push(json!({"call": "addParams", "on": on, "args": [{"ref": name}]}));
            calls
        }
        Op::AddAgain(k) => {
            vec![json!({"call": "addParams", "on": on, "args": [{"ref": format!("p{k}")}]})]
        }
        Op::Change(k, op) => vec![op.call(&format!("p{k}"))],
        Op::ChangeVia(i, op) => vec![
            json!({"call": "getParams", "on": on, "args": [i], "as": "via"}),
            // A call on None (out of range) is refused by the oracle: only in range.
            op.call("via"),
        ],
        Op::Copy => vec![json!({"copy": "merger", "as": "copy"})],
    }
}

/// The port's state of a sequence: the merger, its copy, and the handles `p<k>`.
struct PortState {
    merger: ConfigMerger,
    copy: Option<ConfigMerger>,
    handles: Vec<Option<ConfigMergingParameters>>,
}

impl PortState {
    /// The port's entries of a step's calls (`None` for those that return an object).
    fn run(&mut self, step: &Step) -> Vec<Option<Value>> {
        if let Op::Copy = step.op {
            self.copy = Some(self.merger.create_editable_copy().expect("the copy"));
            return vec![None];
        }
        let target = if step.on_copy {
            self.copy.as_mut().expect("a copy")
        } else {
            &mut self.merger
        };
        match &step.op {
            Op::SetSearchPath(v) => {
                target.set_search_path(v);
                vec![Some(json!({"result": null}))]
            }
            Op::AddSearchPath(v) => {
                target.add_search_path(v);
                vec![Some(json!({"result": null}))]
            }
            Op::SetWorkingDir(v) => {
                target.set_working_dir(v);
                vec![Some(json!({"result": null}))]
            }
            Op::SetVersion(a, b) => {
                target.set_version(*a, *b);
                vec![Some(json!({"result": null}))]
            }
            Op::AddParams(k, ops) => {
                let p = ConfigMergingParameters::new().expect("Create");
                let mut out = vec![None];
                out.extend(ops.iter().map(|op| Some(op.apply(&p))));
                target.add_params(Some(p.clone()));
                out.push(Some(json!({"result": null})));
                if self.handles.len() <= *k {
                    self.handles.resize(*k + 1, None);
                }
                self.handles[*k] = Some(p);
                out
            }
            Op::AddAgain(k) => {
                target.add_params(self.handles[*k].clone());
                vec![Some(json!({"result": null}))]
            }
            Op::Change(k, op) => vec![Some(op.apply(self.handles[*k].as_ref().expect("p")))],
            Op::ChangeVia(i, op) => {
                let p = target.params(*i).expect("in range");
                vec![None, Some(op.apply(&p))]
            }
            Op::Copy => unreachable!(),
        }
    }
}

/// The number of parameters each merger holds after each step, from the plan.
fn plan_counts(steps: &[Step]) -> Vec<(usize, Option<usize>)> {
    let mut merger = 0;
    let mut copy = None;
    let mut out = Vec::new();
    for step in steps {
        let count = if step.on_copy {
            copy.as_mut().expect("a copy")
        } else {
            &mut merger
        };
        match step.op {
            Op::AddParams(..) | Op::AddAgain(_) => *count += 1,
            Op::Copy => copy = Some(merger),
            _ => {}
        }
        out.push((merger, copy));
    }
    out
}

/// What the sequences covered.
#[derive(Debug, Default)]
struct Seen {
    calls: usize,
    serialize_errors: usize,
    undecodable: usize,
    params_reprs: usize,
}

/// Runs the sequences on both sides and compares every call's entry and log.
fn compare_sequences(sequences: &[Vec<Step>]) -> Seen {
    // The oracle's environment is empty: so is the port's here (`Config::Create`, for the
    // parameters' override configs).
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&[u8], &[u8]>(&[]))));

    // Steps that change parameters out of range would be refused by the oracle (a call on
    // None): the plans keep `ChangeVia` in range.
    let sequences: Vec<Vec<Step>> = sequences
        .iter()
        .map(|steps| {
            let counts = plan_counts(steps);
            steps
                .iter()
                .enumerate()
                .filter(|(i, step)| match step.op {
                    Op::ChangeVia(index, _) => {
                        let before = if *i == 0 { (0, None) } else { counts[i - 1] };
                        let n = if step.on_copy {
                            before.1.unwrap_or(0)
                        } else {
                            before.0
                        };
                        index >= 0 && (index as usize) < n
                    }
                    _ => true,
                })
                .map(|(_, s)| s.clone())
                .collect()
        })
        .collect();

    let requests: Vec<Value> = sequences
        .iter()
        .map(|steps| {
            let counts = plan_counts(steps);
            let mut calls = vec![json!({"call": "Create", "on": "ConfigMerger", "as": "merger"})];
            calls.extend(observe_calls("merger", 0));
            for (step, (n, copy)) in steps.iter().zip(&counts) {
                calls.extend(step_calls(step));
                calls.extend(observe_calls("merger", *n));
                if let Some(c) = copy {
                    calls.extend(observe_calls("copy", *c));
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
        let wheel = response["calls"].as_array().expect("the calls");
        let mut at = 0;
        let mut compare =
            |label: &str, port: Vec<Option<Value>>, logged: Vec<Vec<u8>>, seen: &mut Seen| {
                let mut wheel_log = Vec::new();
                for entry in port {
                    let w = wheel
                        .get(at)
                        .unwrap_or_else(|| panic!("sequence {n}: {label}: no entry"));
                    at += 1;
                    let (w_entry, w_log) = split_log(w);
                    wheel_log.extend(w_log);
                    seen.calls += 1;
                    match entry {
                        Some(entry) => {
                            assert_eq!(w_entry, entry, "sequence {n}: {label}");
                            if entry["exception"]["message"].is_object() {
                                seen.serialize_errors += 1;
                            }
                            if entry.get("undecodable").is_some() {
                                seen.undecodable += 1;
                            }
                        }
                        // An object: the parameters (or a merger), written out with its getters;
                        // a string that isn't UTF-8 among them stops the writing ("write"), and
                        // the object is stored all the same.
                        None => assert!(
                            w_entry["result"]["class"].is_string()
                                || (w_entry["stage"] == json!("write")
                                    && w_entry["undecodable"].is_string()),
                            "sequence {n}: {label}: {w_entry}"
                        ),
                    }
                }
                assert_eq!(wheel_log, logged, "sequence {n}: {label}: the log");
            };

        let (merger, logged) = capture(ConfigMerger::new);
        compare("Create", vec![None], logged, &mut seen);
        let mut state = PortState {
            merger,
            copy: None,
            handles: Vec::new(),
        };
        let (port, logged) = capture(|| observe(&state.merger));
        compare("after Create", port, logged, &mut seen);
        for (s, step) in steps.iter().enumerate() {
            let label = format!("step {s} {step:?}");
            let (port, logged) = capture(|| state.run(step));
            compare(&label, port, logged, &mut seen);
            let (port, logged) = capture(|| observe(&state.merger));
            seen.params_reprs += state.merger.num_config_merging_parameters() as usize;
            compare(&format!("{label}: merger"), port, logged, &mut seen);
            if let Some(copy) = &state.copy {
                let (port, logged) = capture(|| observe(copy));
                compare(&format!("{label}: copy"), port, logged, &mut seen);
            }
        }
        assert_eq!(at, wheel.len(), "sequence {n}: entries left over");
    }
    set_thread_env_provider(None);
    seen
}

/// The merger's calls, its parameters' and its OCIOM text, as the wheel's.
#[test]
fn mergers_match_the_wheel() {
    let mut sequences = vec![coverage_sequence()];
    for seed in 1..=8u64 {
        sequences.push(random_sequence(0x51_7cc1_b727_220a ^ seed, 30));
    }
    let seen = compare_sequences(&sequences);
    // Not vacuous: many calls, the writer's errors, parameters' texts read through the merger.
    assert!(seen.calls > 5_000, "{seen:?}");
    assert!(seen.serialize_errors >= 6, "{seen:?}");
    assert!(seen.params_reprs >= 100, "{seen:?}");
}

/// Strings that aren't UTF-8 in every field the OCIOM text holds: the writer's text, bytes for
/// bytes (the wheel's `serialize()` then raises `UnicodeDecodeError`, which the oracle reports
/// with the bytes).
#[test]
fn ociom_text_of_bytes_matches_the_wheel() {
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&[u8], &[u8]>(&[]))));
    let bad = |t: &str| [t.as_bytes(), b" \xff\xfe"].concat();
    let ops = vec![
        ParamOp::Output(bad("out")),
        ParamOp::Base(bad("base")),
        ParamOp::Input(bad("input")),
        ParamOp::Name(bad("name")),
        ParamOp::InputPrefix(bad("prefix")),
        ParamOp::SearchPath(bad("sp")),
        ParamOp::Env(bad("ENV"), bad("value")),
        ParamOp::ActiveViews(bad("v1, v2")),
        ParamOp::Inactive(bad("i1, i2")),
    ];
    let mut cases = Vec::new();
    // Each field alone, then all of them.
    for op in &ops {
        cases.push(vec![op.clone()]);
    }
    cases.push(ops.clone());
    let mut calls = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let m = format!("m{k}");
        let p = format!("p{k}");
        calls.push(json!({"call": "Create", "on": "ConfigMerger", "as": m}));
        calls.push(json!({"call": "Create", "on": "ConfigMergingParameters", "as": p}));
        calls.extend(case.iter().map(|op| op.call(&p)));
        calls.push(json!({"call": "addSearchPath", "on": m, "args": [bytes_arg(&bad("path"))]}));
        calls.push(json!({"call": "addParams", "on": m, "args": [{"ref": p}]}));
        calls.push(json!({"call": "serialize", "on": m}));
    }
    let response = Oracle::get()
        .call("object_calls", json!({"calls": calls}), &[])
        .result;
    let wheel = response["calls"].as_array().expect("the calls");
    let mut at = 0;
    let mut undecodable = 0;
    for case in &cases {
        let mut m = ConfigMerger::new();
        let p = ConfigMergingParameters::new().expect("Create");
        // Create, Create, the setters, addSearchPath, addParams.
        at += 2;
        for op in case {
            assert_eq!(split_log(&wheel[at]).0, op.apply(&p), "{op:?}");
            at += 1;
        }
        m.add_search_path(bad("path"));
        m.add_params(Some(p));
        at += 2;
        let (port, logged) = capture(|| text_entry(m.serialize()));
        let (w, w_log) = split_log(&wheel[at]);
        at += 1;
        assert_eq!(w, port, "{case:?}");
        assert_eq!(w_log, logged, "{case:?}");
        undecodable += usize::from(port.get("undecodable").is_some());
    }
    assert_eq!(at, wheel.len());
    assert_eq!(undecodable, cases.len());
    set_thread_env_provider(None);
}

/// `None` parameters: the wheel holds them, counts them and returns them (`getParams`). Its
/// copy, its merge and its writer dereference them and the process dies (probed on Windows,
/// 2026-10-23), so those calls run on the port alone, which refuses them with an error
/// (docs/improvements.md, U-176).
#[test]
fn none_parameters() {
    let response = Oracle::get()
        .call(
            "object_calls",
            json!({"calls": [
                {"call": "Create", "on": "ConfigMerger", "as": "merger"},
                {"call": "Create", "on": "ConfigMergingParameters", "as": "p"},
                {"call": "addParams", "on": "merger", "args": [{"ref": "p"}]},
                {"call": "addParams", "on": "merger", "args": [null]},
                {"call": "getNumConfigMergingParameters", "on": "merger"},
                {"call": "getParams", "on": "merger", "args": [1]},
                {"call": "getParams", "on": "merger", "args": [2]},
                {"call": "getParams", "on": "merger", "args": [0]},
            ]}),
            &[],
        )
        .result;
    let wheel = response["calls"].as_array().expect("the calls");
    let mut m = ConfigMerger::new();
    m.add_params(Some(ConfigMergingParameters::new().expect("Create")));
    m.add_params(None);
    assert_eq!(wheel[2]["result"], Value::Null, "{response}");
    assert_eq!(wheel[3]["result"], Value::Null, "{response}");
    assert_eq!(
        wheel[4]["result"],
        json!(m.num_config_merging_parameters()),
        "{response}"
    );
    assert_eq!(wheel[5]["result"], Value::Null, "{response}");
    assert!(m.params(1).is_none());
    assert_eq!(wheel[6]["result"], Value::Null, "{response}");
    assert!(m.params(2).is_none());
    assert_eq!(
        wheel[7]["result"]["class"],
        json!("ConfigMergingParameters"),
        "{response}"
    );
    assert!(m.params(0).is_some());

    // The port's errors where the wheel's process dies.
    let u176 = |call: &str| {
        format!(
            "{call}: the merge parameters at index 1 are None, a null pointer the wheel \
             dereferences (its process dies; docs/improvements.md, U-176)"
        )
    };
    assert_eq!(m.serialize().unwrap_err().to_string(), u176("serialize"));
    assert_eq!(
        m.create_editable_copy().unwrap_err().to_string(),
        u176("createEditableCopy")
    );
}

/// A py_eval string.
fn s(v: &str) -> Value {
    json!({"str": hex(v.as_bytes())})
}

/// A py_eval script that sets the process's `OCIO_ACTIVE_DISPLAYS` (through `os.environ`,
/// which the script's names don't list: `__import__`, from the builtins the `copy` module
/// holds), then copies (`copy.deepcopy`) a merger holding `params` `None` parameters. Its last
/// step is the copy.
fn copy_script(value: &str, params: usize) -> Value {
    let mut steps = vec![
        json!({"call": {"name": "operator.getitem"},
               "args": [{"name": "copy.__builtins__"}, s("__import__")]}),
        json!({"call": {"ref": 0}, "args": [s("os")]}),
        json!({"eval": {"ref": 1, "attr": "environ"}}),
        json!({"setitem": {"ref": 2}, "index": s("OCIO_ACTIVE_DISPLAYS"), "value": s(value)}),
        json!({"call": {"name": "OCIO.ConfigMerger.Create"}}),
    ];
    for _ in 0..params {
        steps.push(json!({"call": {"ref": 4, "attr": "addParams"},
                          "args": [{"none": null}]}));
    }
    steps.push(json!({"call": {"name": "copy.deepcopy"}, "args": [{"ref": 4}]}));
    json!({"steps": steps})
}

/// A copy of a merger holding `None` parameters, under an environment `Config::Create` refuses
/// (an unclosed quote in `OCIO_ACTIVE_DISPLAYS`): upstream's `param->createEditableCopy()`
/// makes the new parameters (`ConfigMergingParameters::Create`) before it reads the null
/// pointer, so the wheel raises the environment's error, and the port must too (the error of
/// docs/improvements.md, U-176, comes after it). A merger without parameters copies under the
/// same environment. The environment is set inside the wheel's script, because the oracle's
/// config (`object_calls`) is made from it first and would be refused.
#[test]
fn a_copy_makes_its_parameters_before_it_reads_the_null() {
    const BAD: &str = "\"open, c";
    let report = Oracle::get()
        .call(
            "py_eval",
            json!({"scripts": [copy_script(BAD, 1), copy_script(BAD, 0)]}),
            &[],
        )
        .result;
    let reports = report["scripts"].as_array().expect("scripts");
    assert_eq!(reports.len(), 2, "{report}");
    // Every step before the copy succeeded: the environment was set, the merger made.
    for r in reports {
        let steps = r["steps"].as_array().expect("steps");
        let (_, before) = steps.split_last().expect("steps");
        assert!(before.iter().all(|st| st.get("raised").is_none()), "{r}");
    }

    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[(
        "OCIO_ACTIVE_DISPLAYS",
        BAD,
    )]))));

    // With a null: the environment's error, as the wheel's.
    let wheel = reports[0]["steps"]
        .as_array()
        .expect("steps")
        .last()
        .expect("a step");
    let raised = &wheel["raised"];
    assert_eq!(
        raised["mro"][0],
        json!("PyOpenColorIO.PyOpenColorIO.Exception"),
        "{wheel}"
    );
    assert_eq!(
        raised["message"],
        bytes_arg(
            b"The string '\"open, c' is not correctly formatted. It is missing a closing quote."
        ),
        "{wheel}"
    );
    let mut m = ConfigMerger::new();
    m.add_params(None);
    let e = m.create_editable_copy().expect_err("the null parameters");
    assert_eq!(e.kind(), ExceptionKind::Exception);
    assert_eq!(raised["message"], bytes_arg(e.what()));

    // Without parameters: nothing is made, and the copy succeeds on both sides.
    let wheel = reports[1]["steps"]
        .as_array()
        .expect("steps")
        .last()
        .expect("a step");
    assert!(wheel.get("raised").is_none(), "{wheel}");
    assert!(ConfigMerger::new().create_editable_copy().is_ok());

    set_thread_env_provider(None);
}
