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

/// A zip archive's entry `name`: its external attributes, from the central directory.
#[cfg(windows)]
fn zip_external_attributes(zip: &[u8], name: &[u8]) -> u32 {
    let u16_at = |p: usize| usize::from(u16::from_le_bytes([zip[p], zip[p + 1]]));
    let u32_at = |p: usize| u32::from_le_bytes(zip[p..p + 4].try_into().unwrap());
    let end = (0..zip.len() - 3)
        .rev()
        .find(|&p| u32_at(p) == 0x0605_4b50)
        .expect("an end of central directory record");
    let mut p = u32_at(end + 16) as usize;
    for _ in 0..u16_at(end + 10) {
        assert_eq!(u32_at(p), 0x0201_4b50);
        let name_len = u16_at(p + 28);
        if &zip[p + 46..p + 46 + name_len] == name {
            return u32_at(p + 38);
        }
        p += 46 + name_len + u16_at(p + 30) + u16_at(p + 32);
    }
    panic!("no entry {}", String::from_utf8_lossy(name));
}

/// Each letter is its own attribute, and every letter counts. `ociocheck` can't tell them apart,
/// since all three refuse a write; `ocioarchive` records what it finds in the archive
/// (FILE_ATTRIBUTE_READONLY 1, FILE_ATTRIBUTE_HIDDEN 2, FILE_ATTRIBUTE_SYSTEM 4).
#[cfg(windows)]
#[test]
fn attributes_are_the_letters_given() {
    let config = b"ocio_profile_version: 2\n\nsearch_path: .\n\nroles:\n  default: raw\n\n\
                   colorspaces:\n  - !<ColorSpace>\n    name: raw\n  - !<ColorSpace>\n    \
                   name: l\n    from_scene_reference: !<FileTransform> {src: l.spi1d}\n";
    let lut = b"Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n";
    for (letters, expected) in [
        ("r", 1),
        ("h", 2),
        ("s", 4),
        ("hs", 6),
        ("sh", 6),
        ("hr", 3),
        ("rs", 5),
        ("hsr", 7),
    ] {
        let r = run(
            json!({"tool": "ocioarchive", "argv": ["o", "--iconfig", "a.ocio"],
                   "cwd": cwd(&format!("letters_{letters}")),
                   "files": [["a.ocio", 0], ["l.spi1d", 1]],
                   "attributes": [["l.spi1d", letters]]}),
            &[config, lut],
        );
        assert_eq!(exit_code(&r), 0, "{letters}");
        let files = r.result["files"].as_array().unwrap();
        assert_eq!(files.len(), 1, "{letters}: {files:?}");
        assert_eq!(files[0]["path"], "o.ocioz");
        let zip = &r.blobs[files[0]["contents"].as_u64().unwrap() as usize];
        let attributes = zip_external_attributes(zip, b"l.spi1d");
        assert_eq!(attributes & 7, expected, "{letters}");
    }
}

/// What `attributes` refuses.
#[cfg(windows)]
#[test]
fn attributes_refusals() {
    let dir = cwd("attributes_refused");
    let request = |attributes: Value| {
        json!({"tool": "ociocheck", "argv": [], "cwd": dir, "files": [["a", 0]],
               "attributes": attributes})
    };
    for (attributes, expected) in [
        (json!("h"), "must be a list"),
        (json!({}), "must be a list"),
        (json!(1), "must be a list"),
        (json!(["a"]), "bad entry of attributes"),
        (json!([["a"]]), "bad entry of attributes"),
        (json!([["a", "h", "s"]]), "bad entry of attributes"),
        (json!([["b", "h"]]), "bad entry of attributes"),
        (json!([["a", ""]]), "bad entry of attributes"),
        (json!([["a", "x"]]), "bad entry of attributes"),
        (json!([["a", "hx"]]), "bad entry of attributes"),
        (json!([["a", "H"]]), "bad entry of attributes"),
        (json!([["a", 1]]), "bad entry of attributes"),
    ] {
        let error = refused(request(attributes.clone()), &[b"x"]);
        assert!(error.contains(expected), "{attributes}: {error}");
        assert!(!std::path::Path::new(&dir).exists(), "{attributes}");
    }
}

/// A file whose attributes the system won't set stops the run: `NUL` is a device, which takes the
/// input's bytes and has no attributes.
#[cfg(windows)]
#[test]
fn attributes_that_cannot_be_set_stop_the_run() {
    let error = refused(
        json!({"tool": "ociocheck", "argv": [], "cwd": cwd("attributes_device"),
               "files": [["NUL", 0]], "attributes": [["NUL", "h"]]}),
        &[b"x"],
    );
    assert!(error.contains("WinError"), "{error}");
}

/// `attributes` names inputs only: a path out of `cwd` is refused, and that file keeps its
/// attributes.
#[cfg(windows)]
#[test]
fn attributes_stay_inside_the_working_directory() {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 2;
    let dir = cwd("attributes_outside");
    let outside = PathBuf::from(&dir).with_file_name("attributes_outside.txt");
    std::fs::write(&outside, "kept").unwrap();
    let hidden = |path: &PathBuf| {
        std::fs::metadata(path).unwrap().file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
    };
    assert!(!hidden(&outside));
    let error = refused(
        json!({"tool": "ociocheck", "argv": [], "cwd": dir, "files": [["a", 0]],
               "attributes": [["../attributes_outside.txt", "h"]]}),
        &[b"x"],
    );
    assert!(error.contains("bad entry of attributes"), "{error}");
    assert!(!hidden(&outside));
}

/// The wheel's tools with a pipe nobody reads, on both dispositions of SIGPIPE: SIGPIPE's number
/// is 13 on Linux, and a shell reports it as 128 + 13.
#[cfg(unix)]
#[test]
fn sigpipe_reaches_a_tool() {
    for (closed, tool, argv) in [
        ("stdout", "ociocheck", vec!["--help"]),
        ("stderr", "ociowrite", vec!["--file", "o.txt"]),
    ] {
        let r = run(
            json!({"tool": tool, "argv": argv, "cwd": cwd("sigpipe"), "closed_pipe": closed}),
            &[],
        );
        assert_eq!(r.result["exit_code"], -13, "{}", r.result);
        assert_eq!(r.result["crashed"]["returncode"], -13);
        let r = run(
            json!({"tool": tool, "argv": argv, "cwd": cwd("sigpipe"), "closed_pipe": closed,
                   "sigpipe_ignored": true}),
            &[],
        );
        assert_eq!(exit_code(&r), 1);
    }
}

/// `tool | head -c 3`, with more output than a pipe holds, so that timing doesn't decide it.
#[cfg(unix)]
#[test]
fn head_closes_the_pipe() {
    let r = run(
        json!({"tool": "ociobakelut", "cwd": cwd("head"), "head_bytes": 3,
               "argv": ["--iconfig", "a.ocio", "--inputspace", "raw", "--outputspace", "raw",
                        "--format", "spi3d", "--cubesize", "33", "--stdout"],
               "files": [["a.ocio", 0]]}),
        &[CONFIG.as_bytes()],
    );
    assert_eq!(exit_code(&r), 141);
    assert_eq!(stdout(&r), b"");
}

#[cfg(unix)]
#[test]
fn pipe_keys_refusals() {
    let dir = cwd("pipe_refused");
    for (extra, expected) in [
        (json!({"closed_pipe": "stdin"}), "closed_pipe"),
        (json!({"closed_pipe": 1}), "closed_pipe"),
        (json!({"sigpipe_ignored": 1}), "sigpipe_ignored"),
        (json!({"head_bytes": 0}), "head_bytes"),
        (json!({"head_bytes": "3"}), "head_bytes"),
        (
            json!({"closed_pipe": "stdout", "merge_stderr": true}),
            "merge_stderr",
        ),
        (
            json!({"head_bytes": 3, "merge_stderr": true}),
            "merge_stderr",
        ),
        (
            json!({"head_bytes": 3, "closed_pipe": "stdout"}),
            "head_bytes",
        ),
        (
            json!({"head_bytes": 3, "sigpipe_ignored": true}),
            "head_bytes",
        ),
    ] {
        let mut args = json!({"tool": "ociocheck", "argv": [], "cwd": dir});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let error = refused(args, &[]);
        assert!(error.contains(expected), "{extra}: {error}");
    }
}

/// Windows has no SIGPIPE.
#[cfg(windows)]
#[test]
fn pipe_keys_are_refused() {
    for extra in [
        json!({"closed_pipe": "stdout"}),
        json!({"sigpipe_ignored": true}),
        json!({"head_bytes": 3}),
    ] {
        let mut args = json!({"tool": "ociocheck", "argv": [], "cwd": cwd("pipe_keys")});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let error = refused(args, &[]);
        assert!(error.contains("Linux"), "{extra}: {error}");
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
