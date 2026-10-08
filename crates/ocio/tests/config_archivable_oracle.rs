// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Config::isArchivable` against the wheel, on this platform's paths (WP 3.10c): working
//! directories, search paths and file transforms' files, absolute or relative, with `..`, with
//! context variables, and Windows' drive and UNC forms, which only Windows takes as absolute.

use ocio::Config;
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::hex;
use serde_json::{Value, json};
use std::sync::Arc;

/// Paths for working directories, search paths and files.
const PATHS: &[&str] = &[
    "",
    ".",
    "..",
    "./..",
    "a/../..",
    "a/..",
    "a/../b",
    "..a",
    "...",
    "luts",
    "./luts",
    ".\\luts",
    "..\\luts",
    "luts\\..\\..",
    "$SHOT",
    "${SHOT}/luts",
    "%SHOT%",
    "luts/$SHOT",
    "a%b",
    "/",
    "/luts",
    "//server/share",
    "\\\\server\\share",
    "\\luts",
    "C:",
    "C:luts",
    "C:\\",
    "C:/luts",
    "c:\\$SHOT",
    "~/luts",
];

/// The YAML of a config whose color space reads the file `src`.
fn config_text(src: &str) -> Vec<u8> {
    let mut text = b"ocio_profile_version: 2\nroles: {default: raw}\ndisplays:\n  d:\n    - \
                     !<View> {name: v, colorspace: raw}\ncolorspaces:\n  - !<ColorSpace>\n    \
                     name: raw\n"
        .to_vec();
    if !src.is_empty() {
        text.extend_from_slice(b"    from_scene_reference: !<FileTransform> {src: \"");
        for c in src.bytes() {
            if c == b'\\' || c == b'"' {
                text.push(b'\\');
            }
            text.push(c);
        }
        text.extend_from_slice(b"\"}\n");
    }
    text
}

/// A case: the working directory, the search path and the file transform's file.
struct Case {
    working_dir: &'static str,
    search_path: &'static str,
    src: &'static str,
}

#[test]
fn configs_are_archivable_as_in_the_wheel() {
    let mut cases = Vec::new();
    for wd in PATHS {
        cases.push(Case {
            working_dir: wd,
            search_path: "",
            src: "",
        });
    }
    let wd = if cfg!(windows) { "C:\\work" } else { "/work" };
    for path in PATHS {
        cases.push(Case {
            working_dir: wd,
            search_path: path,
            src: "",
        });
        cases.push(Case {
            working_dir: wd,
            search_path: "",
            src: path,
        });
    }
    // Two search paths, the second refused.
    cases.push(Case {
        working_dir: wd,
        search_path: "luts:../luts",
        src: "",
    });

    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|case| BatchCall {
            cmd: "config_calls",
            args: json!({"config": {"yaml": {"bytes": hex(&config_text(case.src))}},
            "calls": [
                {"call": "setWorkingDir", "args": [case.working_dir]},
                {"call": "setSearchPath", "args": [case.search_path]},
                {"call": "isArchivable"},
            ]}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);

    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
    let mut failures = Vec::new();
    for (case, w) in cases.iter().zip(wheel) {
        let w = w.expect("config_calls").result;
        assert!(w["config"].is_null(), "{}: {}", case.src, w["config"]);
        let wheel: Value = w["calls"][2]["result"].clone();
        let mut config = (*Config::create_from_stream(&config_text(case.src)).unwrap()).clone();
        config.set_working_dir(case.working_dir);
        config.set_search_path(case.search_path);
        let port = json!(config.is_archivable());
        if wheel != port {
            failures.push(format!(
                "working dir {:?}, search path {:?}, file {:?}: wheel {wheel}, port {port}",
                case.working_dir, case.search_path, case.src
            ));
        }
    }
    set_thread_env_provider(None);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
