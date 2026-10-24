// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Where the merger finds a merge's base and input configs (`ConfigMerger::Impl::loadConfig`,
//! chunk 9.2b) against the wheel, through the oracle's `object_calls`: files along the search
//! paths and the working directory, built-in configs, and earlier merges' outputs. The wheel
//! shows it through `mergeConfigs` (9.2j): its exception when it finds no config ("Could not
//! load the base or the input config") or a merge names a merge whose output doesn't exist yet
//! (`std::out_of_range`, docs/improvements.md I-387), and otherwise the merged config's
//! working directory, which is the base config's (the merge starts from a copy of the base,
//! and no section merger changes the working directory). The logs of `mergeConfigs` are the
//! merge's (9.2j), and aren't compared here.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ocio_ops::exception::ExceptionKind;
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes_arg;
use serde_json::{Value, json};

use super::*;

/// What the wheel's `mergeConfigs` raised, as the port's error gives it.
fn exception_json(e: &Exception) -> Value {
    let kind = match e.kind() {
        ExceptionKind::Exception => "Exception",
        ExceptionKind::OutOfRange => "IndexError",
        k => panic!("unexpected kind {k:?}"),
    };
    json!({"type": kind, "message": bytes_arg(e.what())})
}

/// A config with one color space, `name`, and `description`.
fn small_config(description: &str) -> String {
    format!(
        "ocio_profile_version: 2\n\ndescription: {description}\n\nroles:\n  default: lin\n  \
         scene_linear: lin\n\ndisplays:\n  disp:\n    - !<View> {{name: v, colorspace: lin}}\n\n\
         colorspaces:\n  - !<ColorSpace>\n    name: lin\n"
    )
}

/// The files of the request directory: configs at the top, in `sub` and in `other`, a file
/// that isn't a config in `broken` (with a valid one of the same name in `sub`), a directory
/// named like a config.
fn files() -> Vec<(&'static str, String)> {
    vec![
        ("a.ocio", small_config("top a")),
        ("sub/a.ocio", small_config("sub a")),
        ("sub/b.ocio", small_config("sub b")),
        ("other/c.ocio", small_config("other c")),
        ("other/b.ocio", small_config("other b")),
        (
            "broken/a.ocio",
            "ocio_profile_version: 2\ncolorspaces: [\n".to_string(),
        ),
        ("broken/b.ocio", "not: a config\n".to_string()),
        ("dir.ocio/inside.ocio", small_config("inside")),
        // A file named like a built-in config: found along the search paths before it.
        ("cg-config-latest", small_config("file cg")),
    ]
}

/// A merger's search paths: set with `setSearchPath`, or added one by one.
#[derive(Debug, Clone)]
enum Paths {
    None,
    Set(&'static str),
    Add(Vec<&'static str>),
}

/// A case: the merger's working directory (relative to the request directory), its search
/// paths, and its merges' base and input names and output names. Only the first merge's
/// configs are looked for: the others give output names to refer to.
#[derive(Debug, Clone)]
struct LoadCase {
    working_dir: &'static str,
    paths: Paths,
    merges: Vec<(&'static str, &'static str, &'static str)>,
}

fn load_cases() -> Vec<LoadCase> {
    let paths = [
        Paths::None,
        Paths::Add(vec!["sub"]),
        Paths::Add(vec!["broken", "sub"]),
        Paths::Add(vec![" sub/ ", "other"]),
        Paths::Add(vec!["other", "sub"]),
        Paths::Add(vec!["nonexistent", "./other//"]),
        Paths::Set("sub:other"),
        Paths::Set(""),
        Paths::Add(vec!["broken"]),
        Paths::Add(vec!["../x", "sub/../other"]),
    ];
    let names = [
        "a.ocio",
        "b.ocio",
        "c.ocio",
        "sub/b.ocio",
        // A lexical `..` through a directory that doesn't exist (the normalized path on Linux).
        "nodir/../a.ocio",
        "dir.ocio",
        "missing.ocio",
        "",
        "cg-config-latest",
        "ocio://default",
        "CG-Config-Latest",
        "merged",
        "MERGED",
    ];
    let mut cases = Vec::new();
    for working_dir in ["", "sub"] {
        for p in &paths {
            for base in names {
                // The input: one that loads from most search paths, or one that doesn't.
                for input in ["sub/b.ocio", base] {
                    cases.push(LoadCase {
                        working_dir,
                        paths: p.clone(),
                        merges: vec![(base, input, "merged")],
                    });
                }
            }
        }
    }
    // A merge naming a later merge's output (by its name in another case), or its own; an
    // input naming one after a base that loads.
    for (base, input) in [
        ("later", "a.ocio"),
        ("LATER", "a.ocio"),
        ("a.ocio", "later"),
        ("first", "a.ocio"),
        ("missing.ocio", "Later"),
        ("later", "missing.ocio"),
    ] {
        cases.push(LoadCase {
            working_dir: "",
            paths: Paths::None,
            merges: vec![
                (base, input, "first"),
                ("a.ocio", "a.ocio", "later"),
                ("a.ocio", "a.ocio", "later"),
            ],
        });
    }
    cases
}

/// The wheel's calls of a case: the merger, then `mergeConfigs`, and the merged config's
/// working directory when it merges.
fn load_calls(case: &LoadCase) -> Vec<Value> {
    let mut calls = vec![
        // A merge that succeeds first, stored as "out", so that the calls after a merge that
        // fails act on it (and are then not compared).
        json!({"call": "Create", "on": "ConfigMerger", "as": "dummy"}),
        json!({"call": "Create", "on": "ConfigMergingParameters", "as": "dp"}),
        json!({"call": "setBaseConfigName", "on": "dp", "args": ["a.ocio"]}),
        json!({"call": "setInputConfigName", "on": "dp", "args": ["a.ocio"]}),
        json!({"call": "setAdjustInputReferenceSpace", "on": "dp", "args": [false]}),
        json!({"call": "addParams", "on": "dummy", "args": [{"ref": "dp"}]}),
        json!({"call": "mergeConfigs", "on": "dummy", "as": "out"}),
        json!({"call": "Create", "on": "ConfigMerger", "as": "m"}),
        json!({"call": "setWorkingDir", "on": "m", "args": [case.working_dir]}),
    ];
    match &case.paths {
        Paths::None => {}
        Paths::Set(p) => calls.push(json!({"call": "setSearchPath", "on": "m", "args": [p]})),
        Paths::Add(list) => {
            for p in list {
                calls.push(json!({"call": "addSearchPath", "on": "m", "args": [p]}));
            }
        }
    }
    for (k, (base, input, output)) in case.merges.iter().enumerate() {
        let p = format!("p{k}");
        calls.extend([
            json!({"call": "Create", "on": "ConfigMergingParameters", "as": p}),
            json!({"call": "setBaseConfigName", "on": p, "args": [base]}),
            json!({"call": "setInputConfigName", "on": p, "args": [input]}),
            json!({"call": "setOutputName", "on": p, "args": [output]}),
            // Without the reference-space heuristics, which these small configs fail.
            json!({"call": "setAdjustInputReferenceSpace", "on": p, "args": [false]}),
            json!({"call": "addParams", "on": "m", "args": [{"ref": p}]}),
        ]);
    }
    calls.extend([
        json!({"call": "mergeConfigs", "on": "m", "as": "out"}),
        json!({"call": "getMergedConfig", "on": "out", "args": [0], "as": "merged"}),
        json!({"call": "getWorkingDir", "on": "merged"}),
        json!({"call": "getDescription", "on": "merged"}),
    ]);
    calls
}

/// A path under `root` as text relative to it ("" for `root` itself), with `/` separators.
fn relative(path: &[u8], root: &Path) -> String {
    let path = String::from_utf8(path.to_vec()).expect("UTF-8 paths");
    let root = root.to_str().expect("UTF-8 paths");
    let rest = path
        .strip_prefix(root)
        .unwrap_or_else(|| panic!("{path} isn't under {root}"));
    rest.trim_start_matches(['/', '\\']).replace('\\', "/")
}

/// Writes the files under a new directory of the test's own.
fn write_files() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ocio-rs-config-merger-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    for (path, content) in files() {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().expect("a parent")).expect("the directory");
        std::fs::write(full, content).expect("the file");
    }
    root
}

/// Where the merger looks for the first merge's base and input configs, as the wheel's
/// `mergeConfigs` shows it: the `std::out_of_range` (`IndexError`) of a merge's output that
/// doesn't exist yet, "Could not load the base or the input config" when either isn't found,
/// and otherwise the base config found, by its working directory (relative to the files'
/// directory, `""` for a built-in config). The port's `load_config` runs on a merger with the
/// same calls, the files in a directory of its own.
#[test]
fn configs_are_found_as_the_wheel_finds_them() {
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&[u8], &[u8]>(&[]))));
    let files_json: serde_json::Map<String, Value> = files()
        .into_iter()
        .map(|(p, c)| (p.to_string(), Value::String(c)))
        .collect();
    let cases = load_cases();
    let requests: Vec<Value> = cases
        .iter()
        .map(|case| json!({"files": files_json, "calls": load_calls(case)}))
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
    assert_eq!(responses.len(), cases.len());

    let root = write_files();
    let mut outcomes = [0usize; 4];
    for (case, response) in cases.iter().zip(responses) {
        let response = response.unwrap_or_else(|e| panic!("{case:?}: {e}")).result;
        let wheel_root = PathBuf::from(response["dir"].as_str().expect("the directory"));
        let calls = response["calls"].as_array().expect("the calls");
        let merge = &calls[calls.len() - 4];
        assert_eq!(
            calls[6]["result"]["class"],
            json!("ConfigMerger"),
            "{case:?}: the first merge"
        );
        let working_dir = &calls[calls.len() - 2];
        let description = &calls[calls.len() - 1];

        let mut m = ConfigMerger::new();
        let wd = if case.working_dir.is_empty() {
            root.clone()
        } else {
            root.join(case.working_dir)
        };
        m.set_working_dir(wd.to_str().expect("UTF-8 paths"));
        match &case.paths {
            Paths::None => {}
            Paths::Set(p) => m.set_search_path(p),
            Paths::Add(list) => list.iter().for_each(|p| m.add_search_path(p)),
        }
        for (base, input, output) in &case.merges {
            let p = ConfigMergingParameters::new().expect("Create");
            p.set_base_config_name(base);
            p.set_input_config_name(input);
            p.set_output_name(output);
            m.add_params(Some(p));
        }
        let (base, input, _) = case.merges[0];
        let loaded = m
            .load_config(base.as_bytes())
            .and_then(|b| Ok((b, m.load_config(input.as_bytes())?)));
        match loaded {
            Err(e) => {
                outcomes[0] += 1;
                assert_eq!(e.kind(), ExceptionKind::OutOfRange, "{case:?}");
                assert_eq!(merge["exception"], exception_json(&e), "{case:?}: {merge}");
            }
            Ok((None, _)) | Ok((_, None)) => {
                outcomes[1] += 1;
                assert_eq!(
                    merge["exception"],
                    json!({"type": "Exception",
                           "message": bytes_arg(b"Could not load the base or the input config")}),
                    "{case:?}: {merge}"
                );
            }
            Ok((Some(base_cfg), Some(input_cfg))) => {
                // The merged config's description is the input's (the general section's
                // default strategy, PreferInput, SectionMerger.cpp:116-143 @ v2.5.2).
                let port_description = String::from_utf8(input_cfg.description().to_vec())
                    .expect("UTF-8 descriptions");
                // Loaded: the wheel merged (or failed in the merge itself, which 9.2j
                // checks), and the merged config is the base's copy.
                if merge.get("exception").is_some() {
                    let message = &merge["exception"]["message"];
                    assert_ne!(
                        message,
                        &bytes_arg(b"Could not load the base or the input config"),
                        "{case:?}"
                    );
                    assert_ne!(merge["exception"]["type"], json!("IndexError"), "{case:?}");
                    outcomes[3] += 1;
                    continue;
                }
                outcomes[2] += 1;
                let port_wd = base_cfg.working_dir();
                let port_wd = if port_wd.is_empty() {
                    String::new()
                } else {
                    relative(&port_wd, &root)
                };
                let wheel_wd = working_dir["result"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{case:?}: {working_dir}"));
                let wheel_wd = if wheel_wd.is_empty() {
                    String::new()
                } else {
                    relative(wheel_wd.as_bytes(), &wheel_root)
                };
                assert_eq!(wheel_wd, port_wd, "{case:?}");
                assert_eq!(
                    description,
                    &json!({"result": port_description, "log": description["log"]}),
                    "{case:?}"
                );
            }
        }
    }
    set_thread_env_provider(None);
    let _ = std::fs::remove_dir_all(&root);
    // Every outcome is reached: out of range, not found, found (files in several directories
    // and built-in configs).
    assert!(outcomes[0] >= 5, "{outcomes:?}");
    assert!(outcomes[1] >= 40, "{outcomes:?}");
    assert!(outcomes[2] >= 80, "{outcomes:?}");
}

/// Parameters that are `None`: where the merger looks for a merge's output among them, upstream
/// dereferences the null pointer (`m_mergeParams.at(i)->getOutputName()`), and the port refuses
/// it (docs/improvements.md, U-176). The wheel's merge never gets there (its copy of the merger
/// dies first), so this is the port's alone. Parameters before the `None` that name the value
/// are found first, as upstream's loop finds them.
#[test]
fn none_parameters_in_the_search() {
    let mut m = ConfigMerger::new();
    let p = ConfigMergingParameters::new().expect("Create");
    p.set_output_name("first");
    m.add_params(Some(p));
    m.add_params(None);
    let e = m.load_config(b"not a config").unwrap_err();
    assert_eq!(
        e.to_string(),
        "mergeConfigs: the merge parameters at index 1 are None, a null pointer the wheel \
         dereferences (its process dies; docs/improvements.md, U-176)"
    );
    let e = m.load_config(b"FIRST").unwrap_err();
    assert_eq!(e.kind(), ExceptionKind::OutOfRange);
}
