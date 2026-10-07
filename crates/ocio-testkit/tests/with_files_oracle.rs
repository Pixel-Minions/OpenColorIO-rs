// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `with_files` command (`oracle/ocio_oracle/files_api.py`): a command run next
//! to given files gives what it gives on the same files written by the test itself, with the
//! directory's path written as `$FILES`; files can come as text, bytes or blobs; `$FILES` is
//! replaced in strings and in `{"bytes": hex}` values, both ways; the requests it can't read
//! are refused.

use std::path::{Path, PathBuf};

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

/// A small `.spi1d` LUT: the test's input.
const SPI1D: &str = "Version 1\nFrom 0.0 1.0\nLength 3\nComponents 1\n{\n0.0\n0.5\n1.0\n}\n";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A `FileTransform` spec reading `src`.
fn file_transform(src: &str) -> Value {
    json!({"class": "FileTransform", "calls": [["setSrc", src]]})
}

/// A new directory of the test's own, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let dir =
            std::env::temp_dir().join(format!("ocio-rs-with-files-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        TempDir(std::fs::canonicalize(&dir).unwrap())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The path as the oracle's Python writes it: without the `\\?\` prefix `canonicalize` adds on
/// Windows.
fn plain(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// `value` with each spelling of `root` (as is, and with `/` for `\`) replaced by `$FILES` in
/// strings: what `with_files` does to its command's results.
fn restored(value: &Value, root: &str) -> Value {
    let roots = [root.to_string(), root.replace('\\', "/")];
    match value {
        Value::String(s) => {
            let mut s = s.clone();
            for r in &roots {
                s = s.replace(r.as_str(), "$FILES");
            }
            Value::String(s)
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| restored(v, root)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), restored(v, root)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `processor_ops` of a LUT through `with_files` is `processor_ops` of the same LUT the test
/// wrote, its directory written as `$FILES`; a file given as a blob is the same file.
#[test]
fn a_command_next_to_files_gives_what_it_gives_on_those_files() {
    let dir = TempDir::new("same");
    std::fs::write(dir.path().join("a").join("lut.spi1d"), SPI1D).unwrap();
    let root = plain(dir.path());
    let direct_src = format!("{root}/a/lut.spi1d");
    let direct = Oracle::get().call(
        "processor_ops",
        json!({"transform": file_transform(&direct_src), "optimization": "OPTIMIZATION_NONE"}),
        &[],
    );
    assert!(
        direct.result.get("exception").is_none(),
        "{}",
        direct.result
    );

    let args = json!({"transform": file_transform("$FILES/a/lut.spi1d"),
                      "optimization": "OPTIMIZATION_NONE"});
    let as_text = Oracle::get().call(
        "with_files",
        json!({"files": {"a/lut.spi1d": SPI1D}, "command": "processor_ops", "args": args}),
        &[],
    );
    assert_eq!(as_text.result, restored(&direct.result, &root));
    assert_eq!(as_text.blobs, direct.blobs);
    let files = &as_text.result["processor"]["processor_metadata"]["getters"]["getFiles"];
    assert!(files.to_string().contains("$FILES"), "{files}");

    let as_bytes = Oracle::get().call(
        "with_files",
        json!({"files": {"a/lut.spi1d": {"bytes": hex(SPI1D.as_bytes())}},
               "command": "processor_ops", "args": args}),
        &[],
    );
    assert_eq!(as_bytes.result, as_text.result);

    let extra: &[u8] = b"not a file";
    let as_blob = Oracle::get().call(
        "with_files",
        json!({"files": {"a/lut.spi1d": {"blob": 0}}, "file_blobs": 1,
               "command": "processor_ops", "args": args}),
        &[SPI1D.as_bytes(), extra],
    );
    assert_eq!(as_blob.result, as_text.result);
    assert_eq!(as_blob.blobs, as_text.blobs);
}

/// A missing file's message names it with `$FILES`, as `ExceptionMissingFile`.
#[test]
fn a_missing_file_is_named_with_files() {
    let result = Oracle::get()
        .call(
            "with_files",
            json!({"files": {}, "command": "processor_ops",
                   "args": {"transform": file_transform("$FILES/missing.spi1d")}}),
            &[],
        )
        .result;
    assert_eq!(
        result["exception"]["type"], "ExceptionMissingFile",
        "{result}"
    );
    let message = result["exception"]["message"].as_str().unwrap();
    assert!(message.contains("'$FILES/missing.spi1d'"), "{message}");
}

/// `$FILES` in `{"bytes": hex}` arguments becomes the path, and the path in `{"bytes": hex}`
/// results becomes `$FILES` again; the command's other blobs reach it after the files'.
#[test]
fn files_in_bytes_both_ways() {
    let src = b"$FILES/a/lut.spi1d";
    let calls = json!([
        {"new": "FileTransform", "as": "ft"},
        {"call": "setSrc", "on": "ft", "args": [{"bytes": hex(src)}]},
        {"call": "getSrc", "on": "ft"},
    ]);
    let result = Oracle::get()
        .call(
            "with_files",
            json!({"files": {"a/lut.spi1d": SPI1D}, "command": "config_calls",
                   "args": {"config": "raw", "calls": calls}}),
            &[],
        )
        .result;
    let got = &result["calls"][2]["result"];
    assert_eq!(bytes(got), src.to_vec(), "{result}");
}

/// Requests it can't read are refused.
#[test]
fn bad_requests_are_refused() {
    let ok_args = json!({"transform": file_transform("$FILES/a.spi1d")});
    for (args, blobs) in [
        (
            json!({"files": {}, "command": "no_such_command", "args": {}}),
            0,
        ),
        (json!({"files": {}, "command": "with_files", "args": {}}), 0),
        (json!({"files": {}, "command": "batch", "args": {}}), 0),
        (
            json!({"files": {"../a.spi1d": SPI1D}, "command": "processor_ops", "args": ok_args}),
            0,
        ),
        (
            json!({"files": {"a.spi1d": 1}, "command": "processor_ops", "args": ok_args}),
            0,
        ),
        (
            json!({"files": {"a.spi1d": {"blob": 0}}, "command": "processor_ops", "args": ok_args}),
            1,
        ),
        (
            json!({"files": {"a.spi1d": {"blob": 1}}, "file_blobs": 1,
                "command": "processor_ops", "args": ok_args}),
            1,
        ),
        (
            json!({"files": {}, "file_blobs": 2, "command": "processor_ops", "args": ok_args}),
            1,
        ),
        (
            json!({"files": {}, "file_blobs": true, "command": "processor_ops", "args": ok_args}),
            1,
        ),
        (
            json!({"files": [], "command": "processor_ops", "args": ok_args}),
            0,
        ),
        (
            json!({"files": {}, "command": "processor_ops", "args": ok_args, "extra": 1}),
            0,
        ),
    ] {
        let blob: &[u8] = SPI1D.as_bytes();
        let blobs: Vec<&[u8]> = vec![blob; blobs];
        assert!(
            Oracle::get()
                .try_call("with_files", args.clone(), &blobs)
                .is_err(),
            "{args}"
        );
    }
}
