// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The OCIOM reader (chunk 9.2c) against the wheel, through the oracle's `object_calls`: each
//! OCIOM text is the file `m.ociom` of a request, which `ConfigMerger.CreateFromFile` reads;
//! the port reads the same bytes with [`read`] and the same path. Each case compares the
//! exception, or the merger: its version, search paths, working directory (the request's
//! directory for the wheel, the process's for the port: the path is relative), the getters of
//! each merge's parameters and the OCIOM text the writer gives back (`serialize()`); and the
//! log (the warnings of unknown `params` sections).
//!
//! The texts: upstream's two OCIOM files, then generated ones: every key at each level,
//! unknown keys, repeated keys, values of each YAML kind where a key takes another, every
//! strategy's spelling and others, versions `std::stoi` reads or refuses, null values, empty
//! files and sections, names that aren't UTF-8, and YAML that doesn't parse.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes_arg, hex, log};
use serde_json::{Value, json};

use super::*;
use crate::test_env::{EnvGuard, capture_log};

/// The path both sides read: relative, so that the messages that name it are the same.
const PATH: &str = "m.ociom";

/// A string the port returned, as the wheel's call reports it.
fn string_entry(v: &[u8]) -> Value {
    match std::str::from_utf8(v) {
        Ok(s) => json!({"result": s}),
        Err(_) => json!({"undecodable": hex(v), "stage": "call"}),
    }
}

/// The wheel's entry without its log, and the log.
fn split_log(entry: &Value) -> (Value, Vec<Vec<u8>>) {
    let log = log(&entry["log"]);
    let mut entry = entry.clone();
    entry.as_object_mut().expect("an entry").remove("log");
    (entry, log)
}

/// The parameters' getters the wheel's dump of a `ConfigMergingParameters` calls (every `get*`
/// and `is*` method without arguments), with the port's.
fn params_getters(p: &ConfigMergingParameters) -> Vec<(&'static str, Value)> {
    let s = |v: Vec<u8>| -> Value {
        match String::from_utf8(v) {
            Ok(s) => Value::String(s),
            Err(e) => json!({"not utf-8": hex(e.as_bytes())}),
        }
    };
    let e = |m: MergeStrategies| {
        let name = match m {
            MergeStrategies::PreferInput => "STRATEGY_PREFER_INPUT",
            MergeStrategies::PreferBase => "STRATEGY_PREFER_BASE",
            MergeStrategies::InputOnly => "STRATEGY_INPUT_ONLY",
            MergeStrategies::BaseOnly => "STRATEGY_BASE_ONLY",
            MergeStrategies::Remove => "STRATEGY_REMOVE",
            MergeStrategies::Unspecified => "STRATEGY_UNSPECIFIED",
        };
        json!({"enum": name})
    };
    vec![
        ("getActiveDisplays", s(p.active_displays())),
        ("getActiveViews", s(p.active_views())),
        ("getBaseConfigName", s(p.base_config_name())),
        ("getBaseFamilyPrefix", s(p.base_family_prefix())),
        ("getColorspaces", e(p.colorspaces())),
        ("getDefaultStrategy", e(p.default_strategy())),
        ("getDescription", s(p.description())),
        ("getDisplayViews", e(p.display_views())),
        ("getFileRules", e(p.file_rules())),
        ("getInactiveColorSpaces", s(p.inactive_color_spaces())),
        ("getInputConfigName", s(p.input_config_name())),
        ("getInputFamilyPrefix", s(p.input_family_prefix())),
        ("getLooks", e(p.looks())),
        ("getName", s(p.name())),
        ("getNamedTransforms", e(p.named_transforms())),
        ("getNumEnvironmentVars", json!(p.num_environment_vars())),
        ("getOutputName", s(p.output_name())),
        ("getRoles", e(p.roles())),
        ("getSearchPath", s(p.search_path())),
        ("getViewTransforms", e(p.view_transforms())),
        (
            "isAdjustInputReferenceSpace",
            json!(p.is_adjust_input_reference_space()),
        ),
        ("isAvoidDuplicates", json!(p.is_avoid_duplicates())),
        ("isErrorOnConflict", json!(p.is_error_on_conflict())),
        ("isInputFirst", json!(p.is_input_first())),
    ]
}

/// The parameters each case reads at most (`getParams(0..PARAMS)`), and the search paths.
const PARAMS: i32 = 4;
const SEARCH_PATHS: i32 = 4;

/// The wheel's calls of a case.
fn calls() -> Vec<Value> {
    let mut calls = vec![
        // An empty merger first, so that the calls after a refused file act on it.
        json!({"call": "Create", "on": "ConfigMerger", "as": "m"}),
        json!({"call": "CreateFromFile", "on": "ConfigMerger", "args": [PATH], "as": "m"}),
        json!({"call": "getMajorVersion", "on": "m"}),
        json!({"call": "getMinorVersion", "on": "m"}),
        json!({"call": "getNumSearchPaths", "on": "m"}),
        json!({"call": "getWorkingDir", "on": "m"}),
        json!({"call": "getNumConfigMergingParameters", "on": "m"}),
        json!({"call": "serialize", "on": "m"}),
    ];
    for i in 0..SEARCH_PATHS {
        calls.push(json!({"call": "getSearchPath", "on": "m", "args": [i]}));
    }
    for i in 0..PARAMS {
        calls.push(json!({"call": "getParams", "on": "m", "args": [i]}));
    }
    calls
}

/// What a case covered.
#[derive(Debug, Default)]
struct Seen {
    read: usize,
    refused: usize,
    warned: usize,
    params: usize,
}

/// Reads each text on both sides and compares.
fn compare(texts: &[Vec<u8>]) -> Seen {
    let _env = EnvGuard::new();
    let requests: Vec<Value> = texts
        .iter()
        .map(|t| json!({"files": {PATH: bytes_arg(t)}, "calls": calls()}))
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
    assert_eq!(responses.len(), texts.len());

    let cwd = std::env::current_dir().expect("the working directory");
    let cwd = cwd.to_str().expect("a UTF-8 path").as_bytes().to_vec();
    let mut seen = Seen::default();
    for (n, (text, response)) in texts.iter().zip(responses).enumerate() {
        let label = format!("case {n}: {:?}", String::from_utf8_lossy(text));
        let response = response.unwrap_or_else(|e| panic!("{label}: {e}")).result;
        let wheel = response["calls"].as_array().expect("the calls");
        let (empty, _) = split_log(&wheel[0]);
        assert_eq!(
            empty["result"]["class"],
            json!("ConfigMerger"),
            "{label}: {empty}"
        );
        let (created, created_log) = split_log(&wheel[1]);
        let (port, logged) = capture_log(|| read(IStream::file(text), PATH.as_bytes()));
        assert_eq!(created_log, logged, "{label}: the log");
        seen.warned += usize::from(!logged.is_empty());
        let merger = match port {
            Err(e) => {
                assert_eq!(
                    created,
                    json!({"exception": {"type": "Exception", "message": bytes_arg(e.what())},
                           "stage": "call"}),
                    "{label}"
                );
                seen.refused += 1;
                // The other calls act on the empty merger.
                continue;
            }
            Ok(m) => m,
        };
        seen.read += 1;
        assert!(
            created["result"]["class"] == json!("ConfigMerger")
                || created["stage"] == json!("write"),
            "{label}: {created}"
        );
        let entry = |i: usize| split_log(&wheel[i + 1]).0;
        assert_eq!(
            entry(1),
            json!({"result": merger.major_version()}),
            "{label}"
        );
        assert_eq!(
            entry(2),
            json!({"result": merger.minor_version()}),
            "{label}"
        );
        assert_eq!(
            entry(3),
            json!({"result": merger.num_search_paths()}),
            "{label}"
        );
        // The working directory: the directory of the file, the request's for the wheel, the
        // process's for the port; or none.
        let wd = merger.working_dir();
        if wd.is_empty() {
            assert_eq!(entry(4), json!({"result": ""}), "{label}");
        } else {
            assert_eq!(wd, cwd.as_slice(), "{label}");
            assert_eq!(entry(4), json!({"result": response["dir"]}), "{label}");
        }
        assert_eq!(
            entry(5),
            json!({"result": merger.num_config_merging_parameters()}),
            "{label}"
        );
        let serialized = match merger.serialize() {
            Ok(text) => string_entry(&text),
            Err(e) => json!({"exception": {"type": "Exception", "message": bytes_arg(e.what())},
                             "stage": "call"}),
        };
        assert_eq!(entry(6), serialized, "{label}");
        for i in 0..SEARCH_PATHS {
            let w = entry(7 + i as usize);
            assert_eq!(
                w,
                string_entry(&merger.search_path(i)),
                "{label}: search path {i}"
            );
        }
        for i in 0..PARAMS {
            let w = entry((7 + SEARCH_PATHS + i) as usize);
            match merger.params(i) {
                None => assert_eq!(w, json!({"result": null}), "{label}: params {i}"),
                Some(p) => {
                    seen.params += 1;
                    let getters = params_getters(&p);
                    if getters.iter().any(|(_, v)| v.get("not utf-8").is_some()) {
                        // The dump stops at the first string that isn't UTF-8 (in name order).
                        let first = getters
                            .iter()
                            .find_map(|(_, v)| v.get("not utf-8"))
                            .expect("one");
                        assert_eq!(w["stage"], json!("write"), "{label}: params {i}: {w}");
                        assert_eq!(&w["undecodable"], first, "{label}: params {i}");
                        continue;
                    }
                    let wg = &w["result"]["getters"];
                    assert_eq!(
                        wg.as_object().map(|o| o.len()),
                        Some(getters.len()),
                        "{label}: params {i}: {w}"
                    );
                    for (name, v) in getters {
                        assert_eq!(wg[name], v, "{label}: params {i}: {name}");
                    }
                }
            }
        }
        assert_eq!(wheel.len(), calls().len(), "{label}");
    }
    seen
}

/// Upstream's OCIOM test files.
fn upstream_files() -> Vec<Vec<u8>> {
    ["parser_test.ociom", "parser_test_no_overrides.ociom"]
        .iter()
        .map(|f| {
            let path = format!(
                "{}/configs/mergeconfigs/{f}",
                crate::fileformats::test_utils::get_test_files_dir()
            );
            std::fs::read(path).expect("upstream's test file")
        })
        .collect()
}

/// A merge entry with `body` (indented by 4) under `merge`.
fn merge_doc(body: &str) -> String {
    format!("ociom_version: 1.0\nsearch_path: [a]\nmerge:\n  M:\n{body}")
}

/// A merge's `options` with the one entry `entry`.
fn options(entry: &str) -> String {
    merge_doc(&format!("    base: b.ocio\n    options:\n      {entry}\n"))
}

/// A merge's `overrides` with the one entry `entry`.
fn overrides(entry: &str) -> String {
    merge_doc(&format!("    overrides:\n      {entry}\n"))
}

/// A merge's `params` with the one entry `entry`.
fn params(entry: &str) -> String {
    merge_doc(&format!("    params:\n      {entry}\n"))
}

fn generated() -> Vec<Vec<u8>> {
    let mut docs: Vec<String> = vec![
        // Whole documents.
        String::new(),
        "# only a comment\n".into(),
        "ociom_version: 1.0\n".into(),
        "merge:\n".into(),
        "merge: {}\n".into(),
        "merge: []\n".into(),
        "merge: scalar\n".into(),
        "- a\n- b\n".into(),
        "just a scalar\n".into(),
        "ociom_version: 1.0\nociom_version: 1.0\n".into(),
        "unknown: 1\nother: [1, 2]\n".into(),
        "a: [\n".into(),
        "{a: b}: c\n".into(),
        "~: x\nmerge: {}\n".into(),
        "search_path: ~\n".into(),
        "merge:\n  M1:\n  M2: scalar\n  M3: {}\n".into(),
        "merge:\n  M1:\n    - base\n".into(),
        "merge:\n  M1: {base: a}\n  M1: {base: b}\n".into(),
        "merge:\n  M1: {base: a}\n  m1: {base: b}\n".into(),
        "merge:\n  ~: {base: a}\n  2: {input: x}\n".into(),
        "merge:\n  M1: {base: a, base: b, input: c, other: d}\n".into(),
        "merge:\n  M1: {base: ~, input: }\n".into(),
        "merge:\n  M1: {base: [a], input: b}\n".into(),
        "merge:\n  M1: {base: {a: b}}\n".into(),
        "merge:\n  M1: {base: 'quoted', input: \"dq\", options: ~, overrides: ~, params: ~}\n"
            .into(),
        "merge:\n  A: {base: x}\n  B: {base: y}\n  C: {base: z}\n".into(),
        "search_path: a:b::c\nmerge: {}\n".into(),
        "search_path: [a, '', /abs, ' sp ']\n".into(),
        "search_path: []\n".into(),
        "search_path: {a: b}\n".into(),
        "search_path: ''\n".into(),
        "search_path: C:/drive:x\n".into(),
        "merge:\n  M: {base: b}\nociom_version: 1.0\nsearch_path: [late]\n".into(),
        "merge:\n  M: {base: b}\nmerge:\n  N: {}\n".into(),
    ];
    // Versions.
    for v in [
        "1",
        "1.0",
        "1.1",
        "2",
        "2.0",
        "0",
        "0.0",
        "0.1",
        "1.0.0",
        "''",
        "abc",
        "1.",
        ".0",
        "+1",
        "' 1'",
        "-1",
        "-0",
        "1.0abc",
        "1.-0",
        "99999999999",
        "4294967297",
        "'1.0 '",
        "{a: 1}",
        "[1, 0]",
        "1.00",
        "01.0",
        "1.0x5",
        "'\t1.0'",
        "1e0",
    ] {
        docs.push(format!("ociom_version: {v}\nmerge:\n  M: {{base: b}}\n"));
    }
    // Options.
    for key in [
        "input_first",
        "error_on_conflict",
        "avoid_duplicates",
        "adjust_input_reference_space",
        "assume_common_reference_space",
    ] {
        for v in [
            "true", "false", "yes", "No", "on", "OFF", "1", "y", "maybe", "~", "[true]",
        ] {
            docs.push(options(&format!("{key}: {v}")));
        }
    }
    for v in [
        "PreferInput",
        "PreferBase",
        "InputOnly",
        "BaseOnly",
        "Remove",
        "Unspecified",
        "preferinput",
        "'PreferInput '",
        "Unknown",
        "''",
        "~",
        "[PreferBase]",
    ] {
        docs.push(options(&format!("default_strategy: {v}")));
    }
    for entry in [
        "input_family_prefix: abc",
        "input_family_prefix: ~",
        "input_family_prefix: [a]",
        "base_family_prefix: 'a/b'",
        "base_family_prefix: 12",
        "unknown_option: 1",
        "input_frist: false",
        "{input_first: false, input_first: true}",
    ] {
        docs.push(options(entry));
    }
    docs.push(merge_doc("    options: [input_first]\n"));
    docs.push(merge_doc("    options: scalar\n"));
    docs.push(merge_doc(
        "    options:\n      input_first: false\n      input_first: true\n",
    ));
    docs.push(merge_doc(
        "    options:\n      assume_common_reference_space: true\n      \
         adjust_input_reference_space: true\n",
    ));
    // Overrides.
    for entry in [
        "name: my merge",
        "name: ~",
        "name: [a]",
        "name: ''",
        "description: \"multi\\nline\"",
        "description: {a: b}",
        "search_path: abc:def",
        "search_path: [a, b]",
        "search_path: []",
        "search_path: {a: b}",
        "search_path: ''",
        "environment: {A: 1, B: ~, C: '', A: 2}",
        "environment: {}",
        "environment: [A]",
        "environment: scalar",
        "environment: {A: [1]}",
        "environment: {'': x}",
        "active_displays: [D1, D2]",
        "active_displays: ['D, 1', D2]",
        "active_displays: ['\"D1', D2]",
        "active_displays: []",
        "active_displays: D1",
        "active_displays: [[D1]]",
        "active_views: [V1, 'V 2']",
        "active_views: ['\"open, v']",
        "active_views: x",
        "inactive_colorspaces: [I1, I2]",
        "inactive_colorspaces: ['\"I1', I2]",
        "inactive_colorspaces: []",
        "inactive_colorspaces: {a: b}",
        "unknown: x",
    ] {
        docs.push(overrides(entry));
    }
    docs.push(merge_doc("    overrides:\n      name: a\n      name: b\n"));
    docs.push(merge_doc("    overrides: [name]\n"));
    // Params.
    for section in [
        "roles",
        "file_rules",
        "display-views",
        "view_transforms",
        "looks",
        "colorspaces",
        "named_transforms",
    ] {
        for v in ["PreferBase", "Remove", "Unspecified", "remove", "~"] {
            docs.push(params(&format!("{section}: {{strategy: {v}}}")));
        }
    }
    for entry in [
        "roles: PreferBase",
        "roles: [PreferBase]",
        "roles: {}",
        "roles: {other: x}",
        "roles: {other: x, strategy: BaseOnly}",
        "roles: {strategy: BaseOnly, strategy: InputOnly}",
        "roles: {strategy: [BaseOnly]}",
        "roles: {other: [x], strategy: BaseOnly}",
        "roles: ~",
        "display_views: {strategy: Remove}",
        "named_transform: {strategy: Remove}",
        "unknown: 1",
    ] {
        docs.push(params(entry));
    }
    docs.push(merge_doc(
        "    params:\n      unknown_a: 1\n      looks: {strategy: Remove}\n      unknown_b: ~\n",
    ));
    docs.push(merge_doc(
        "    params:\n      looks: {strategy: Remove}\n      looks: {strategy: Remove}\n",
    ));
    docs.push(merge_doc("    params: [looks]\n"));
    docs.push(merge_doc("    params: scalar\n"));
    // A parse error after a warning is logged.
    docs.push(merge_doc(
        "    params:\n      unknown: 1\n      roles: {strategy: Nope}\n",
    ));
    let mut texts: Vec<Vec<u8>> = docs.into_iter().map(String::into_bytes).collect();
    // Bytes that aren't UTF-8, and a NUL.
    texts.push(b"merge:\n  \xff\xfe: {base: \xfe}\n".to_vec());
    texts.push(b"merge:\n  M: {base: a\0b}\n".to_vec());
    texts.push(b"merge:\n  M: {overrides: {environment: {\xc3\xa9: v\xff}}}\n".to_vec());
    texts.push(b"\xef\xbb\xbfmerge:\n  M: {base: bom}\n".to_vec());
    texts.push(b"merge:\r\n  M: {base: crlf}\r\n".to_vec());
    texts
}

/// Upstream's OCIOM files and the generated ones, read as the wheel reads them.
#[test]
fn ociom_files_read_as_the_wheel_reads_them() {
    let mut texts = upstream_files();
    texts.extend(generated());
    let seen = compare(&texts);
    // Not vacuous: both outcomes, many of each; warnings; parameters read.
    assert!(seen.read >= 100, "{seen:?}");
    assert!(seen.refused >= 60, "{seen:?}");
    assert!(seen.warned >= 3, "{seen:?}");
    assert!(seen.params >= 100, "{seen:?}");
}

/// `ConfigMerger::from_file`'s own errors: an empty path, a file that doesn't exist, a
/// directory (which Windows doesn't open, and which libstdc++'s stream opens and then fails to
/// read); and a file read through it. The paths are relative, to the request's directory for
/// the wheel and the process's for the port: both hold `src` and `src/x`, and no `nope.ociom`.
#[test]
fn files_open_as_the_wheel_opens_them() {
    let _env = EnvGuard::new();
    assert!(std::path::Path::new("src").is_dir());
    assert!(!std::path::Path::new("nope.ociom").exists());
    let paths = ["", "nope.ociom", "src", "src/"];
    let calls: Vec<Value> = paths
        .iter()
        .map(|p| json!({"call": "CreateFromFile", "on": "ConfigMerger", "args": [p]}))
        .collect();
    let response = Oracle::get()
        .call(
            "object_calls",
            json!({"files": {"src/x": "x"}, "calls": calls}),
            &[],
        )
        .result;
    let wheel = response["calls"].as_array().expect("the calls");
    assert_eq!(wheel.len(), paths.len());
    for (path, w) in paths.iter().zip(wheel) {
        let e = ConfigMerger::from_file(path).expect_err("refused");
        let kind = if e.is_missing_file() {
            "ExceptionMissingFile"
        } else {
            "Exception"
        };
        assert_eq!(
            split_log(w).0,
            json!({"exception": {"type": kind, "message": bytes_arg(e.what())},
                   "stage": "call"}),
            "{path:?}"
        );
    }
}
