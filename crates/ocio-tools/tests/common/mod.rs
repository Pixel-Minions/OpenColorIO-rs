// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The sweeps of the tools: each case runs in the wheel's tool (the oracle's `tool_run`) and in
//! the port's, in one directory, with the same arguments, environment and input files, and
//! stdout, stderr, the exit code and the files written must be the same bytes.
//!
//! Nothing is normalised. Absolute paths compare because both runs use the same directory; the
//! version banner is `2.5.2` on both sides; these tools print no timings.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use ocio_testkit::oracle::{ALLOWED_VARIABLES, BatchCall, Oracle};
use ocio_testkit::paths;
use serde_json::{Value, json};

/// How many cases run at a time in the port's tool.
const THREADS: usize = 6;

/// One command line.
#[derive(Debug, Clone)]
pub(crate) struct Case {
    pub(crate) label: String,
    pub(crate) argv: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) files: Vec<(String, Vec<u8>)>,
}

impl Case {
    pub(crate) fn new(label: impl Into<String>, argv: &[&str]) -> Case {
        Case {
            label: label.into(),
            argv: argv.iter().map(|s| s.to_string()).collect(),
            env: Vec::new(),
            files: Vec::new(),
        }
    }

    pub(crate) fn env(mut self, name: &str, value: impl Into<String>) -> Case {
        self.env.push((name.to_string(), value.into()));
        self
    }

    pub(crate) fn file(mut self, path: &str, contents: impl Into<Vec<u8>>) -> Case {
        self.files.push((path.to_string(), contents.into()));
        self
    }
}

/// What a run gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub(crate) exit_code: i64,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) files: BTreeMap<String, Vec<u8>>,
}

/// A file of upstream's test data, by its absolute path.
pub(crate) fn test_file(relative: &str) -> String {
    let path = paths::upstream_dir()
        .join("tests/data/files")
        .join(relative);
    assert!(path.exists(), "{}", path.display());
    path.to_str().unwrap().replace('\\', "/")
}

/// The contents of a file of upstream's test data.
pub(crate) fn test_file_bytes(relative: &str) -> Vec<u8> {
    std::fs::read(test_file(relative)).unwrap()
}

/// Case `index`'s directory: one path for both runs, the same on every run of the test.
fn case_dir(group: &str, index: usize) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tool_sweeps")
        .join(group)
        .join(index.to_string())
}

/// Runs `cases` in the wheel's `tool`.
fn wheel_outcomes(group: &str, tool: &str, cases: &[Case]) -> Vec<Outcome> {
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let cwd = case_dir(group, index);
            let _ = std::fs::remove_dir_all(&cwd);
            std::fs::create_dir_all(cwd.parent().unwrap()).unwrap();
            let files: Vec<Value> = case
                .files
                .iter()
                .enumerate()
                .map(|(blob, (path, _))| json!([path, blob]))
                .collect();
            let env: BTreeMap<&str, &str> = case
                .env
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            BatchCall {
                cmd: "tool_run",
                args: json!({
                    "tool": tool, "argv": case.argv, "cwd": cwd.to_str().unwrap(),
                    "env": env, "files": files,
                }),
                blobs: case
                    .files
                    .iter()
                    .map(|(_, bytes)| bytes.as_slice())
                    .collect(),
            }
        })
        .collect();
    // Never from the cache: a file's inode can reach the output (`CacheID`, on Linux), and a
    // Rocky Linux 9 container has its own inodes for the checkout.
    Oracle::get()
        .batch(&calls, false)
        .into_iter()
        .zip(cases)
        .map(|(response, case)| {
            let response = response.unwrap_or_else(|e| panic!("{}: {e}", case.label));
            let result = &response.result;
            let blob = |value: &Value| response.blobs[value.as_u64().unwrap() as usize].clone();
            let files = result["files"].as_array().unwrap();
            let outcome = Outcome {
                exit_code: result["exit_code"].as_i64().unwrap_or(i64::MIN),
                stdout: blob(&result["stdout"]),
                stderr: blob(&result["stderr"]),
                files: files
                    .iter()
                    .map(|file| {
                        (
                            file["path"].as_str().unwrap().to_string(),
                            blob(&file["contents"]),
                        )
                    })
                    .collect(),
            };
            assert_eq!(result["timed_out"], false, "{}", case.label);
            outcome
        })
        .collect()
}

/// Every file under `dir`, by its relative path with `/`.
fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(root, &path, found);
        } else {
            let relative = path.strip_prefix(root).unwrap().to_str().unwrap();
            found.insert(relative.replace('\\', "/"), std::fs::read(&path).unwrap());
        }
    }
}

/// Runs `case` in the port's `program`, in the directory of the wheel's run.
fn port_outcome(group: &str, program: &str, index: usize, case: &Case) -> Outcome {
    let cwd = case_dir(group, index);
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).unwrap();
    for (path, contents) in &case.files {
        let path = cwd.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    let mut command = Command::new(program);
    command
        .args(&case.argv)
        .current_dir(&cwd)
        .env_clear()
        .stdin(Stdio::null());
    for name in ALLOWED_VARIABLES {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in &case.env {
        command.env(name, value);
    }
    let output = command.output().unwrap();
    let mut files = BTreeMap::new();
    walk(&cwd, &cwd, &mut files);
    for (path, contents) in &case.files {
        if files.get(path) == Some(contents) {
            files.remove(path);
        }
    }
    let _ = std::fs::remove_dir_all(&cwd);
    Outcome {
        exit_code: i64::from(output.status.code().unwrap_or(i32::MIN)),
        stdout: output.stdout,
        stderr: output.stderr,
        files,
    }
}

/// Runs `cases` in the port's `program`.
fn port_outcomes(group: &str, program: &str, cases: &[Case]) -> Vec<Outcome> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Outcome)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(case) = cases.get(index) else {
                        return;
                    };
                    let outcome = port_outcome(group, program, index, case);
                    done.lock().unwrap().push((index, outcome));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap();
    done.sort_by_key(|(index, _)| *index);
    assert_eq!(done.len(), cases.len());
    done.into_iter().map(|(_, outcome)| outcome).collect()
}

/// `bytes` as text, for a report.
fn show(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).escape_debug().to_string()
}

/// How one byte string differs from another, for a report.
fn describe(what: &str, wheel: &[u8], port: &[u8]) -> Option<String> {
    if wheel == port {
        return None;
    }
    let at = wheel
        .iter()
        .zip(port)
        .position(|(a, b)| a != b)
        .unwrap_or(wheel.len().min(port.len()));
    let start = at.saturating_sub(80);
    Some(format!(
        "  {what} differs at byte {at} ({} and {} bytes):\n    wheel: {}\n    port:  {}\n",
        wheel.len(),
        port.len(),
        show(&wheel[start..(at + 160).min(wheel.len())]),
        show(&port[start..(at + 160).min(port.len())]),
    ))
}

/// What differs between two runs, or nothing.
fn differences(wheel: &Outcome, port: &Outcome) -> String {
    let mut text = String::new();
    if wheel.exit_code != port.exit_code {
        text += &format!(
            "  exit code: wheel {}, port {}\n",
            wheel.exit_code, port.exit_code
        );
    }
    text += &describe("stdout", &wheel.stdout, &port.stdout).unwrap_or_default();
    text += &describe("stderr", &wheel.stderr, &port.stderr).unwrap_or_default();
    if !wheel.files.keys().eq(port.files.keys()) {
        text += &format!(
            "  files: wheel {:?}, port {:?}\n",
            wheel.files.keys().collect::<Vec<_>>(),
            port.files.keys().collect::<Vec<_>>()
        );
    }
    for (path, contents) in &wheel.files {
        if let Some(other) = port.files.get(path) {
            text += &describe(path, contents, other).unwrap_or_default();
        }
    }
    text
}

/// What the wheel's runs of a sweep gave: how a test checks that the cases do what they are for.
#[derive(Debug)]
pub(crate) struct Summary {
    pub(crate) outcomes: Vec<Outcome>,
}

impl Summary {
    /// The number of cases that ended with `code`.
    pub(crate) fn with_exit_code(&self, code: i64) -> usize {
        self.outcomes.iter().filter(|o| o.exit_code == code).count()
    }

    /// The number of cases that wrote a file.
    pub(crate) fn writing_files(&self) -> usize {
        self.outcomes.iter().filter(|o| !o.files.is_empty()).count()
    }
}

/// Runs `cases` on both sides, and fails on any difference. `program` is the port's tool. The
/// list of differences also goes to a file under the test's temporary directory.
pub(crate) fn sweep(group: &str, tool: &str, program: &str, cases: &[Case]) -> Summary {
    assert!(!cases.is_empty(), "{group}: no cases");
    let mut labels: Vec<&str> = cases.iter().map(|case| case.label.as_str()).collect();
    labels.sort_unstable();
    let total = labels.len();
    labels.dedup();
    assert_eq!(labels.len(), total, "{group}: two cases have one label");

    let wheel = wheel_outcomes(group, tool, cases);
    let port = port_outcomes(group, program, cases);
    assert_eq!(wheel.len(), cases.len());
    let mut report = String::new();
    let mut mismatches = 0;
    for (index, case) in cases.iter().enumerate() {
        // A case the wheel doesn't finish has no result to compare with: its own test handles it.
        assert_ne!(wheel[index].exit_code, i64::MIN, "{}", case.label);
        assert!(
            (0..=255).contains(&wheel[index].exit_code),
            "{}: the wheel died with {}: {}",
            case.label,
            wheel[index].exit_code,
            show(&wheel[index].stderr)
        );
        let text = differences(&wheel[index], &port[index]);
        if !text.is_empty() {
            mismatches += 1;
            report += &format!("{} {:?}\n{text}\n", case.label, case.argv);
        }
    }
    if mismatches > 0 {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{group}-mismatches.txt"));
        std::fs::write(&path, &report).unwrap();
        let shown: String = report.chars().take(6000).collect();
        panic!(
            "{group}: {mismatches} of {} cases differ (all of them in {}):\n{shown}",
            cases.len(),
            path.display()
        );
    }
    Summary { outcomes: wheel }
}

/// Runs one case on both sides, for a test that doesn't expect them to match: a case the wheel
/// doesn't survive, or that the port deliberately refuses. Returns (the wheel's, the port's).
pub(crate) fn run_both(group: &str, tool: &str, program: &str, case: &Case) -> (Outcome, Outcome) {
    let wheel = wheel_outcomes(group, tool, std::slice::from_ref(case)).remove(0);
    let port = port_outcome(group, program, 0, case);
    (wheel, port)
}
