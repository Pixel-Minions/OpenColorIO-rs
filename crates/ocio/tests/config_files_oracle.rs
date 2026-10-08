// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Configs read from files and from `$OCIO` against the wheel, live on this platform (WP
//! 3.10b): `Config::CreateFromFile` and `Config::CreateFromEnv`, the error byte for byte or
//! the config's `serialize()`, and what they log.
//!
//! Both sides read the same files, by the same absolute paths, so the messages that name them
//! are the same. The files the tests write are named by their content, so the oracle's cache
//! (keyed on the request) never answers for another content.
//!
//! The files of fewer than four bytes are where the two platforms differ: yaml-cpp's detection
//! of the encoding puts back up to three bytes after the end of the file, which MSVC's file
//! stream can't (an empty document on Windows), and libstdc++'s can.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ocio::Config;
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, hex, log};
use serde_json::{Value, json};

static LOGGING: Mutex<()> = Mutex::new(());

/// `f()` with the environment `env` and nothing else, and what OCIO logged meanwhile.
fn captured<T>(env: &[(&str, &str)], f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(PoisonError::into_inner);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        sink.lock().unwrap().push(m.to_vec());
    })))
    .unwrap();
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries(env))));
    let result = f();
    set_thread_env_provider(None);
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (result, log)
}

/// A config's outcome: its `serialize()` text, or its error.
type Outcome = Result<Vec<u8>, Vec<u8>>;

fn show(outcome: &Outcome) -> String {
    match outcome {
        Ok(text) => format!("a config of {} bytes", text.len()),
        Err(e) => format!("error {:?}", String::from_utf8_lossy(e)),
    }
}

/// The bytes of a config source or call: its result, or the bytes the binding couldn't
/// decode.
fn wheel_bytes(v: &Value, key: &str) -> Vec<u8> {
    match v.get("undecodable") {
        Some(h) => bytes(&json!({"bytes": h})),
        None => bytes(&v[key]),
    }
}

/// The wheel's outcome and log of a `config_calls` request that serializes the config.
fn wheel_outcome(w: &Value) -> (Outcome, Vec<Vec<u8>>) {
    let config = &w["config"];
    let outcome = if config.is_null() {
        Ok(wheel_bytes(&w["calls"][0], "result"))
    } else if config.get("undecodable").is_some() {
        Err(wheel_bytes(config, "undecodable"))
    } else {
        Err(bytes(&config["exception"]["message"]))
    };
    (outcome, log(&w["config_log"]))
}

/// A config source of the oracle, and the environment it reads.
struct Case {
    label: String,
    source: Value,
    env: Vec<(String, String)>,
}

/// Each case against the wheel: the outcome and the log of making the config.
fn check(cases: &[Case], port: impl Fn(&Case) -> Result<Arc<Config>, ocio::Exception>) {
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|case| {
            let env: serde_json::Map<String, Value> = case
                .env
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            BatchCall {
                cmd: "config_calls",
                args: json!({"config": case.source, "env": env,
                             "calls": [{"call": "serialize"}]}),
                blobs: Vec::new(),
            }
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for (case, w) in cases.iter().zip(wheel) {
        let (wheel, wheel_log) = wheel_outcome(&w.expect("config_calls").result);
        let env: Vec<(&str, &str)> = case
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let (port, port_log) = captured(&env, || {
            port(case)
                .and_then(|config| config.serialize())
                .map_err(|e| e.what().to_vec())
        });
        if wheel != port {
            failures.push(format!(
                "{}: wheel {}, port {}",
                case.label,
                show(&wheel),
                show(&port)
            ));
        }
        if wheel_log != port_log {
            failures.push(format!(
                "{}: log\n  wheel {:?}\n  port  {:?}",
                case.label,
                wheel_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>(),
                port_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The directory of the files the tests write.
fn files_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_files_oracle");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `content` to a file named by it; its absolute path, with `/` separators.
fn write_file(content: &[u8]) -> String {
    // Short contents name their file; longer ones by their length and FNV-1a hash.
    let name = if content.len() <= 16 {
        hex(content)
    } else {
        format!(
            "{}_{:016x}",
            content.len(),
            ocio::caching::msvc_fnv1a(content)
        )
    };
    let path = files_dir().join(format!("f_{name}.ocio"));
    std::fs::write(&path, content).unwrap();
    path.to_str().unwrap().replace('\\', "/")
}

/// A `CreateFromFile` case of the file `path`.
fn file_case(label: String, path: &str) -> Case {
    Case {
        label,
        source: json!({"file": path}),
        env: Vec::new(),
    }
}

fn create_from_file(case: &Case) -> Result<Arc<Config>, ocio::Exception> {
    Config::create_from_file(case.source["file"].as_str().unwrap())
}

/// Every file of up to three bytes of a byte order mark's bytes, a zero, a letter and a line
/// feed, and the files of four of a zero, `FE`, `FF` and a letter: the files whose first bytes
/// yaml-cpp reads as the start of an encoding's mark and puts back.
#[test]
fn tiny_files_read_as_in_the_wheel() {
    const BYTES: [u8; 8] = [0x00, 0xEF, 0xBB, 0xBF, 0xFE, 0xFF, b'a', b'\n'];
    const FOUR: [u8; 4] = [0x00, 0xFE, 0xFF, b'a'];
    let mut contents: Vec<Vec<u8>> = vec![Vec::new()];
    for len in 1..=3 {
        let mut next = Vec::new();
        for prefix in contents.iter().filter(|c| c.len() == len - 1) {
            for b in BYTES {
                let mut c = prefix.clone();
                c.push(b);
                next.push(c);
            }
        }
        contents.extend(next);
    }
    for a in FOUR {
        for b in FOUR {
            for c in FOUR {
                for d in FOUR {
                    contents.push(vec![a, b, c, d]);
                }
            }
        }
    }
    let cases: Vec<Case> = contents
        .iter()
        .map(|content| file_case(format!("file {}", hex(content)), &write_file(content)))
        .collect();
    check(&cases, create_from_file);
}

/// Configs read from files: upstream's test configs (`.ocio` and `.yaml`), a file that doesn't
/// exist, a directory, an empty name, built-in configs' URIs, and a config whose search path
/// and working directory come from its file's directory.
#[test]
fn config_files_read_as_in_the_wheel() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/OpenColorIO/tests/data/files/configs")
        .canonicalize()
        .unwrap();
    let mut files = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("ocio" | "yaml")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    let slashed = |p: &Path| {
        let p = p.to_str().unwrap().replace('\\', "/");
        // Windows' canonical paths start with the verbatim prefix, which the wheel's paths
        // don't have.
        p.strip_prefix("//?/").map(str::to_string).unwrap_or(p)
    };
    let mut cases: Vec<Case> = files
        .iter()
        .map(|f| file_case(slashed(f.strip_prefix(&root).unwrap()), &slashed(f)))
        .collect();

    let dir = files_dir();
    let dir = slashed(&dir);
    cases.push(file_case(
        "a file that doesn't exist".into(),
        &format!("{dir}/no such file.ocio"),
    ));
    cases.push(file_case("a directory".into(), &dir));
    cases.push(file_case("an empty name".into(), ""));
    for uri in [
        "ocio://default",
        "ocio://cg-config-latest",
        "x ocio://studio-config-latest",
        "ocio://unknown",
        "ocio://",
    ] {
        cases.push(file_case(uri.into(), uri));
    }
    let with_search_path = write_file(
        b"ocio_profile_version: 2\nsearch_path: luts\nroles: {default: raw}\ncolorspaces:\n  \
          - !<ColorSpace> {name: raw}\ndisplays:\n  d:\n    - !<View> {name: v, colorspace: \
          raw}\n",
    );
    cases.push(file_case("a search path".into(), &with_search_path));
    let bad = write_file(b"ocio_profile_version: 2\nroles: {default: raw\n");
    cases.push(file_case("a file the reader refuses".into(), &bad));

    check(&cases, create_from_file);
}

/// `CreateFromEnv`: `$OCIO` unset or empty (the raw config, and an info message), a file, a
/// built-in config's URI, and names that don't resolve.
#[test]
fn configs_from_the_environment_as_in_the_wheel() {
    let config = write_file(
        b"ocio_profile_version: 2\nroles: {default: raw}\ncolorspaces:\n  - !<ColorSpace> \
          {name: raw}\ndisplays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n",
    );
    let env_case = |label: &str, value: Option<&str>| Case {
        label: label.into(),
        source: json!("env"),
        env: value
            .map(|v| vec![("OCIO".to_string(), v.to_string())])
            .unwrap_or_default(),
    };
    let cases = vec![
        env_case("unset", None),
        env_case("empty", Some("")),
        env_case("a file", Some(&config)),
        env_case("ocio://default", Some("ocio://default")),
        env_case("ocio://thedefault", Some("ocio://thedefault")),
        env_case("a file that doesn't exist", Some("no such file.ocio")),
    ];
    check(&cases, |_| Config::create_from_env());
}
