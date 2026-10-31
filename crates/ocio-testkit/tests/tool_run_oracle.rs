// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `tool_run` command (`oracle/ocio_oracle/tools_api.py`) against the wheel's real
//! tools: what it passes to a tool, what it reports back, and what it refuses.
//!
//! The expected texts are upstream's, from the tools' sources: `src/apps/ociowrite/main.cpp`
//! and `src/apps/ociocheck/main.cpp` @ v2.5.2. Every call is uncached, so a change of the
//! command can't pass on old responses.

use std::path::PathBuf;

use ocio_testkit::Oracle;
use ocio_testkit::oracle::Response;
use serde_json::{Value, json};

/// A directory of this test, which `tool_run` needs empty or missing.
fn cwd(name: &str) -> String {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("tool_run_oracle")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    dir.to_str().unwrap().to_string()
}

fn run(args: Value, blobs: &[&[u8]]) -> Response {
    Oracle::get().call_uncached("tool_run", args, blobs)
}

fn refused(args: Value, blobs: &[&[u8]]) -> String {
    Oracle::get()
        .try_call("tool_run", args, blobs)
        .expect_err("the request should be refused")
}

fn exit_code(r: &Response) -> i64 {
    assert_eq!(r.result["crashed"], Value::Null, "{}", r.result);
    assert_eq!(r.result["timed_out"], false, "{}", r.result);
    r.result["exit_code"].as_i64().unwrap()
}

fn stdout(r: &Response) -> &[u8] {
    &r.blobs[r.result["stdout"].as_u64().unwrap() as usize]
}

fn stderr(r: &Response) -> &[u8] {
    &r.blobs[r.result["stderr"].as_u64().unwrap() as usize]
}

/// `s` as the tool's text streams write it: `\r\n` for each `\n` on Windows.
fn text(s: &str) -> Vec<u8> {
    if cfg!(windows) {
        s.replace('\n', "\r\n").into_bytes()
    } else {
        s.as_bytes().to_vec()
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A small valid config. The wheel writes it back differently, which `--oconfig` shows.
const CONFIG: &str = "ocio_profile_version: 2\n\nroles:\n  default: raw\n\n\
     displays:\n  sRGB:\n    - !<View> {name: Raw, colorspace: raw}\n\n\
     colorspaces:\n  - !<ColorSpace>\n    name: raw\n";

#[test]
fn exit_code_and_stderr() {
    // ociowrite/main.cpp:66-70: without arguments, an empty error line on stderr, then the
    // usage on stdout and exit(1).
    let dir = cwd("exit");
    let r = run(json!({"tool": "ociowrite", "argv": [], "cwd": dir}), &[]);
    assert_eq!(exit_code(&r), 1);
    assert_eq!(stderr(&r), text("\n"));
    let intro = "ociowrite -- write a color transformation to a file\n\n";
    assert!(stdout(&r).starts_with(&text(intro)));
    assert_eq!(r.result["files"], json!([]));
    assert!(!std::path::Path::new(&dir).exists());
}

#[test]
fn zero_exit_code_and_argv_reach_the_tool() {
    // ociocheck/main.cpp:152, 203, 704.
    let dir = cwd("zero");
    let r = run(
        json!({"tool": "ociocheck", "argv": ["--iconfig", "in/a.ocio"], "cwd": dir,
               "files": [["in/a.ocio", 0]]}),
        &[CONFIG.as_bytes()],
    );
    assert_eq!(exit_code(&r), 0);
    let out = stdout(&r);
    assert!(contains(out, &text("Loading in/a.ocio\n")));
    // The tool ran in `cwd`, whatever CARGO_TARGET_DIR's separators: `in`'s directory is
    // this test's, in the system's spelling.
    let working = format!("{0}zero{0}in", std::path::MAIN_SEPARATOR);
    assert!(
        contains(out, b"Working Dir: ") && contains(out, &text(&format!("{working}\n"))),
        "{}",
        String::from_utf8_lossy(out)
    );
    assert!(out.ends_with(&text("Tests complete.\n\n")));
}

#[test]
fn environment_reaches_the_tool() {
    // ociocheck/main.cpp:155-159.
    let r = run(
        json!({"tool": "ociocheck", "argv": [], "cwd": cwd("env"),
               "env": {"OCIO": "a.ocio"}, "files": [["a.ocio", 0]]}),
        &[CONFIG.as_bytes()],
    );
    assert_eq!(exit_code(&r), 0);
    assert!(contains(stdout(&r), &text("Loading $OCIO a.ocio\n")));
}

#[test]
fn written_files_are_reported_and_unchanged_inputs_are_not() {
    let r = run(
        json!({"tool": "ociocheck",
               "argv": ["--iconfig", "a.ocio", "--oconfig", "out/b.ocio"],
               "cwd": cwd("files"),
               "files": [["a.ocio", 0], ["out/old.txt", 1]]}),
        &[CONFIG.as_bytes(), b"old"],
    );
    assert_eq!(exit_code(&r), 0);
    let files = r.result["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0]["path"], "out/b.ocio");
    let written = &r.blobs[files[0]["contents"].as_u64().unwrap() as usize];
    assert!(written.starts_with(&text("ocio_profile_version: 2\n")));
}

#[test]
fn a_changed_input_is_reported() {
    let r = run(
        json!({"tool": "ociocheck", "argv": ["--iconfig", "a.ocio", "--oconfig", "a.ocio"],
               "cwd": cwd("changed"), "files": [["a.ocio", 0]]}),
        &[CONFIG.as_bytes()],
    );
    assert_eq!(exit_code(&r), 0);
    let files = r.result["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0]["path"], "a.ocio");
    let written = &r.blobs[files[0]["contents"].as_u64().unwrap() as usize];
    assert_ne!(written.as_slice(), CONFIG.as_bytes());
}

/// `0xC0000409`, which a process that fails fast ends with on Windows, as the C++ runtime's
/// `std::terminate` does in the wheel's tools; the negative of SIGABRT's number on Linux.
const DIED_IN_TERMINATE: i64 = if cfg!(windows) { 0xC000_0409 } else { -6 };

#[test]
fn a_tool_that_dies_is_reported() {
    // ociobakelut/main.cpp:261: the config loads outside every `try`.
    let r = run(
        json!({"tool": "ociobakelut", "cwd": cwd("dies"),
               "argv": ["--iconfig", "not_there.ocio", "--inputspace", "a", "--outputspace", "b",
                        "--format", "spi1d", "out.lut"]}),
        &[],
    );
    assert_eq!(r.result["exit_code"], DIED_IN_TERMINATE, "{}", r.result);
    assert_eq!(r.result["crashed"]["returncode"], DIED_IN_TERMINATE);
    assert_eq!(r.result["timed_out"], false);
}

#[test]
fn a_normal_failure_is_not_a_crash() {
    let r = run(
        json!({"tool": "ociowrite", "argv": ["--help"], "cwd": cwd("not_a_crash")}),
        &[],
    );
    assert_eq!(exit_code(&r), 1);
}

#[test]
fn output_bytes_are_not_decoded() {
    // ociocheck/main.cpp:451: the color spaces' names, as the config has them: UTF-8, and a byte
    // that isn't.
    let config = [
        CONFIG.as_bytes(),
        b"  - !<ColorSpace>\n    name: caf\xc3\xa9\n",
        b"  - !<ColorSpace>\n    name: bad\xff\n",
    ]
    .concat();
    let r = run(
        json!({"tool": "ociocheck", "argv": ["--iconfig", "a.ocio"], "cwd": cwd("bytes"),
               "files": [["a.ocio", 0]]}),
        &[&config],
    );
    assert_eq!(exit_code(&r), 0);
    assert!(contains(stdout(&r), &text("caf\u{e9}\nbad")));
    assert!(contains(stdout(&r), b"bad\xff"));
}

#[test]
fn a_tool_that_runs_too_long_is_stopped() {
    let dir = cwd("timeout");
    let r = run(
        json!({"tool": "ociocheck", "argv": ["--iconfig", "ocio://default"], "cwd": dir,
               "timeout_seconds": 0.001}),
        &[],
    );
    assert_eq!(r.result["timed_out"], true, "{}", r.result);
    assert_eq!(r.result["exit_code"], Value::Null);
    assert_eq!(r.result["crashed"]["timeout_seconds"], 0.001);
    assert_eq!(r.result["files"], json!([]));
    assert!(!std::path::Path::new(&dir).exists());
}

#[test]
fn several_written_files_are_sorted() {
    // ocioarchive: `--extract` writes the archive's files, not in this order.
    let archive = ocio_testkit::paths::upstream_dir()
        .join("tests/data/files/configs/context_test1/context_test1_windows.ocioz");
    let archive = std::fs::read(archive).unwrap();
    let r = run(
        json!({"tool": "ocioarchive", "argv": ["--extract", "a.ocioz", "--dir", "out"],
               "cwd": cwd("sorted"), "files": [["a.ocioz", 0]]}),
        &[&archive],
    );
    assert_eq!(exit_code(&r), 0);
    let files = r.result["files"].as_array().unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert!(paths.len() >= 5, "{paths:?}");
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);
}

#[test]
fn stderr_can_share_the_stdout_pipe() {
    // ociobakelut/main.cpp:216-221, 399-401: stdout's text, then stderr's.
    let lut = ocio_testkit::paths::upstream_dir().join("tests/data/files/lut1d_1.spi1d");
    let r = run(
        json!({"tool": "ociobakelut", "cwd": cwd("merged"), "merge_stderr": true,
               "argv": ["--lut", lut.to_str().unwrap(), "--format", "nope", "--v", "out.lut"]}),
        &[],
    );
    assert_eq!(exit_code(&r), 1);
    assert_eq!(stderr(&r), b"");
    let out = stdout(&r);
    assert!(out.starts_with(b"[OpenColorIO DEBUG]: Specified Transform:"));
    assert!(out.ends_with(&text("See --help for more info.\n")));
}

/// A file's attributes reach the tool: `std::ofstream` refuses a hidden file on Windows, and
/// the command can still remove what it made.
#[cfg(windows)]
#[test]
fn attributes_are_set_on_input_files() {
    for letters in ["h", "s", "r", "hsr"] {
        let dir = cwd(&format!("attributes_{letters}"));
        let r = run(
            json!({"tool": "ociocheck", "cwd": dir,
                   "argv": ["--iconfig", "a.ocio", "--oconfig", "o.ocio"],
                   "files": [["a.ocio", 0], ["o.ocio", 1]],
                   "attributes": [["o.ocio", letters]]}),
            &[CONFIG.as_bytes(), b"old"],
        );
        assert_eq!(exit_code(&r), 0);
        let refused = text("Error opening o.ocio for writing.\n");
        assert!(contains(stdout(&r), &refused), "{letters}");
        assert_eq!(r.result["files"], json!([]), "{letters}");
        assert!(!std::path::Path::new(&dir).exists());
    }
}

/// `tool_run` gives a tool an empty stdin. No tool reads it, so this writes to it: /dev/stdin is
/// /dev/null then, where any size fits, where a pipe nobody reads, the command's own stdin
/// passed on, would never take this much.
#[cfg(unix)]
#[test]
fn stdin_is_empty() {
    let r = run(
        json!({"tool": "ociobakelut", "cwd": cwd("stdin"), "timeout_seconds": 60,
               "argv": ["--iconfig", "a.ocio", "--inputspace", "raw", "--outputspace", "raw",
                        "--format", "spi3d", "--cubesize", "33", "/dev/stdin"],
               "files": [["a.ocio", 0]]}),
        &[CONFIG.as_bytes()],
    );
    assert_eq!(exit_code(&r), 0);
}

#[test]
fn refusals() {
    let dir = cwd("refused");
    for (args, expected) in [
        (
            json!({"tool": "nope", "argv": [], "cwd": dir}),
            "must be a tool's name",
        ),
        (
            json!({"tool": "ociocheck;x", "argv": [], "cwd": dir}),
            "must be a tool's name",
        ),
        (
            json!({"tool": "OpenColorIO_2_5", "argv": [], "cwd": dir}),
            "must be a tool's name",
        ),
        (
            json!({"tool": 1, "argv": [], "cwd": dir}),
            "must be a tool's name",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "files": [["a\\b", 0]]}),
            "bad path",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "files": [["a:b", 0]]}),
            "bad path",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "timeout_seconds": 0}),
            "timeout_seconds",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "merge_stderr": 1}),
            "merge_stderr",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "attributes": [["a", "h"]]}),
            "attributes",
        ),
        (
            json!({"tool": "ocionope", "argv": [], "cwd": dir}),
            "no tool",
        ),
        (json!({"tool": "ociocheck", "cwd": dir}), "argv"),
        (
            json!({"tool": "ociocheck", "argv": [1], "cwd": dir}),
            "argv",
        ),
        (json!({"tool": "ociocheck", "argv": []}), "cwd"),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": "relative"}),
            "cwd",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "env": {"A": 1}}),
            "env",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "other": 1}),
            "unknown keys",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "files": [["a", 5]]}),
            "files",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "files": [["../a", 0]]}),
            "path",
        ),
        (
            json!({"tool": "ociocheck", "argv": [], "cwd": dir, "files": [["a", 0], ["a", 0]]}),
            "twice",
        ),
    ] {
        let error = refused(args.clone(), &[b"x"]);
        assert!(error.contains(expected), "{args}: {error}");
    }
    // A directory with something in it, which the command would remove.
    std::fs::create_dir_all(&dir).unwrap();
    let kept = PathBuf::from(&dir).join("kept.txt");
    std::fs::write(&kept, "kept").unwrap();
    let error = refused(json!({"tool": "ociocheck", "argv": [], "cwd": dir}), &[]);
    assert!(error.contains("empty"), "{error}");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), "kept");
}

/// Attributes are Windows's: elsewhere a request for them can't be met.
#[cfg(unix)]
#[test]
fn attributes_are_refused_elsewhere() {
    let error = refused(
        json!({"tool": "ociocheck", "argv": [], "cwd": cwd("attributes"),
               "files": [["a", 0]], "attributes": [["a", "h"]]}),
        &[b"x"],
    );
    assert!(error.contains("on Windows"), "{error}");
}
