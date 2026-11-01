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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
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
    /// The wheel's tool ends here in `std::terminate`, and the port's must too.
    pub(crate) dies: bool,
    /// Windows attributes of input files: (path, letters of "hsr").
    pub(crate) attributes: Vec<(String, String)>,
    /// stderr goes into stdout's pipe: the bytes show the order they were written in.
    pub(crate) merged: bool,
    /// This stream, `stdout` or `stderr`, is a pipe nobody reads.
    pub(crate) closed_pipe: Option<&'static str>,
    /// The tool starts with SIGPIPE ignored.
    pub(crate) sigpipe_ignored: bool,
    /// The shell line `tool | head -c n`.
    pub(crate) head_bytes: Option<u32>,
    /// The files written are ICC profiles: their creation time isn't compared, see [`Case::icc`].
    pub(crate) icc: bool,
    /// Where the wheel leaves its memory, the port's deliberate error instead, see
    /// [`Case::stops`].
    pub(crate) stop: Option<Stop>,
}

/// When a case ends in a deliberate error of the port (CLAUDE.md, rule 1).
#[derive(Debug, Clone, Copy)]
pub(crate) enum Stop {
    /// Always: the wheel finishes, with bytes that come from memory nobody set.
    Always(&'static str),
    /// If the wheel's tool dies: whether it does is up to the case's profile.
    IfTheWheelDies(&'static str),
    /// If the wheel's tool dies, with the first message; or if the port says the second, that
    /// the wheel reads memory it doesn't own, which the wheel then does whether it dies or not.
    /// [`Summary::port_stops`] counts the second.
    Profile {
        dies: &'static str,
        reads_outside: &'static str,
    },
}

impl Case {
    pub(crate) fn new(label: impl Into<String>, argv: &[&str]) -> Case {
        Case {
            label: label.into(),
            argv: argv.iter().map(|s| s.to_string()).collect(),
            env: Vec::new(),
            files: Vec::new(),
            dies: false,
            attributes: Vec::new(),
            merged: false,
            closed_pipe: None,
            sigpipe_ignored: false,
            head_bytes: None,
            icc: false,
            stop: None,
        }
    }

    /// The case writes ICC profiles. Bytes 24 to 35 of a profile are its creation time (ICC.1
    /// section 7.2.1, `dateTimeNumber`; `_cmsWriteHeader` puts `Icc->Created` there): a test can't
    /// run both tools in one second, so that field alone is compared by being a date close to
    /// the wheel's. `creation_time_is_all_that_varies` shows on the wheel that nothing else does.
    pub(crate) fn icc(mut self) -> Case {
        self.icc = true;
        self
    }

    /// The wheel goes where the port can't follow (docs/improvements.md): the port must print
    /// what the wheel printed up to there, then the message, write nothing, and exit with 1. One
    /// stdout must start the other: what `std::cout` holds when the wheel's process dies is
    /// lost, and a wheel that goes on prints more.
    pub(crate) fn stops(mut self, stop: Stop) -> Case {
        self.stop = Some(stop);
        self
    }

    pub(crate) fn closed_pipe(mut self, stream: &'static str) -> Case {
        self.closed_pipe = Some(stream);
        self
    }

    pub(crate) fn sigpipe_ignored(mut self) -> Case {
        self.sigpipe_ignored = true;
        self
    }

    pub(crate) fn head_bytes(mut self, n: u32) -> Case {
        self.head_bytes = Some(n);
        self
    }

    pub(crate) fn attribute(mut self, path: &str, letters: &str) -> Case {
        self.attributes
            .push((path.to_string(), letters.to_string()));
        self
    }

    pub(crate) fn dies(mut self) -> Case {
        self.dies = true;
        self
    }

    pub(crate) fn merged(mut self) -> Case {
        self.merged = true;
        self
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
pub(crate) fn wheel_outcomes(group: &str, tool: &str, cases: &[Case]) -> Vec<Outcome> {
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
            let mut args = json!({
                "tool": tool, "argv": case.argv, "cwd": cwd.to_str().unwrap(),
                "env": env, "files": files, "merge_stderr": case.merged,
                "attributes": case.attributes,
            });
            if let Some(stream) = case.closed_pipe {
                args["closed_pipe"] = json!(stream);
            }
            if case.sigpipe_ignored {
                args["sigpipe_ignored"] = json!(true);
            }
            if let Some(n) = case.head_bytes {
                args["head_bytes"] = json!(n);
            }
            BatchCall {
                cmd: "tool_run",
                args,
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
    set_attributes(&cwd, &case.attributes, '+');
    let mut command = port_command(program, case);
    command.current_dir(&cwd).env_clear().stdin(Stdio::null());
    for name in ALLOWED_VARIABLES {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in &case.env {
        command.env(name, value);
    }
    if let Some(stream) = case.closed_pipe {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        if stream == "stdout" {
            command.stdout(writer);
        } else {
            command.stderr(writer);
        }
    }
    let output = if case.merged {
        merged_output(&mut command)
    } else {
        command.output().unwrap()
    };
    let mut files = BTreeMap::new();
    walk(&cwd, &cwd, &mut files);
    for (path, contents) in &case.files {
        if files.get(path) == Some(contents) {
            files.remove(path);
        }
    }
    set_attributes(&cwd, &case.attributes, '-');
    let _ = std::fs::remove_dir_all(&cwd);
    Outcome {
        exit_code: exit_code(output.status),
        stdout: output.stdout,
        stderr: output.stderr,
        files,
    }
}

/// What runs `case` in the port's `program`, with SIGPIPE as `Command` leaves it, the default,
/// unless the case says otherwise. A shell gives the two other starts without `unsafe`.
fn port_command(program: &str, case: &Case) -> Command {
    let mut command;
    if let Some(n) = case.head_bytes {
        const HEAD_LINE: &str = r#""${@:2}" | head -c "$1" > /dev/null; exit "${PIPESTATUS[0]}""#;
        command = Command::new("/bin/bash");
        command.args(["-c", HEAD_LINE, "bash", &n.to_string(), program]);
    } else if case.sigpipe_ignored {
        command = Command::new("/bin/sh");
        command.args(["-c", r#"trap "" PIPE; exec "$@""#, "sh", program]);
    } else {
        command = Command::new(program);
    }
    command.args(&case.argv);
    command
}

/// `attrib` on input files, the way to set hidden and system attributes without `unsafe`.
fn set_attributes(cwd: &Path, attributes: &[(String, String)], sign: char) {
    for (path, letters) in attributes {
        let mut attrib = Command::new("attrib");
        for letter in letters.chars() {
            attrib.arg(format!("{sign}{letter}"));
        }
        assert!(attrib.arg(cwd.join(path)).status().unwrap().success());
    }
}

/// The exit status as `subprocess` reports it: the negative of the signal on Linux, and on
/// Windows the unsigned value of an NTSTATUS failure code.
fn exit_code(status: std::process::ExitStatus) -> i64 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .map_or_else(|| -i64::from(status.signal().unwrap()), i64::from)
    }
    #[cfg(windows)]
    {
        i64::from(status.code().unwrap() as u32)
    }
}

/// `command`'s output with one pipe for stdout and stderr: a duplicate of the write end is
/// stderr, as `subprocess.STDOUT` does on the wheel's side.
fn merged_output(command: &mut Command) -> Output {
    let (mut reader, writer) = std::io::pipe().unwrap();
    command.stdout(writer.try_clone().unwrap()).stderr(writer);
    let mut child = command.spawn().unwrap();
    // Dropped here, or the read below never ends.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut stdout = Vec::new();
    reader.read_to_end(&mut stdout).unwrap();
    Output {
        status: child.wait().unwrap(),
        stdout,
        stderr: Vec::new(),
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

/// Where a profile's creation time is (ICC.1 section 7.2.1).
pub(crate) const ICC_CREATION_TIME: std::ops::Range<usize> = 24..36;

/// The seconds since 1970 of a `dateTimeNumber`, after checking that it is a date.
fn icc_creation_time(profile: &[u8]) -> Option<i64> {
    let field = profile.get(ICC_CREATION_TIME)?;
    let number = |i: usize| i64::from(u16::from_be_bytes([field[i], field[i + 1]]));
    let (year, month, day) = (number(0), number(2), number(4));
    let (hours, minutes, seconds) = (number(6), number(8), number(10));
    let valid = (2026..3000).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && (0..24).contains(&hours)
        && (0..60).contains(&minutes)
        && (0..61).contains(&seconds);
    // Days from a civil date: the proleptic Gregorian calendar.
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let days = 365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + day - 1;
    valid.then_some(days * 86400 + hours * 3600 + minutes * 60 + seconds)
}

/// `files` with the creation times out of the profiles, or how they are wrong: a port's must be a
/// date, not before the wheel's, by no more than a run of a sweep.
fn without_creation_time(
    wheel: &mut BTreeMap<String, Vec<u8>>,
    port: &mut BTreeMap<String, Vec<u8>>,
) -> String {
    let mut text = String::new();
    for (path, wheel_file) in wheel.iter_mut() {
        let Some(port_file) = port.get_mut(path) else {
            continue;
        };
        let times = (icc_creation_time(wheel_file), icc_creation_time(port_file));
        match times {
            (Some(wheel_time), Some(port_time))
                if (0..=3600).contains(&(port_time - wheel_time)) =>
            {
                wheel_file[ICC_CREATION_TIME].fill(0);
                port_file[ICC_CREATION_TIME].fill(0);
            }
            _ => text += &format!("  {path}: creation times {times:?}\n"),
        }
    }
    text
}

/// What is wrong with a deliberate error of the port, or nothing.
fn stop_differences(wheel: &Outcome, port: &Outcome, message: &str) -> String {
    let mut text = String::new();
    if port.exit_code != 1 {
        text += &format!("  the port ended with {}, not 1\n", port.exit_code);
    }
    if !port.files.is_empty() {
        text += &format!(
            "  the port wrote {:?}\n",
            port.files.keys().collect::<Vec<_>>()
        );
    }
    if !port.stdout.starts_with(&wheel.stdout) && !wheel.stdout.starts_with(&port.stdout) {
        text += &describe("stdout", &wheel.stdout, &port.stdout).unwrap_or_default();
    }
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    let expected = [&wheel.stderr[..], message.as_bytes(), eol.as_bytes()].concat();
    text += &describe("stderr", &expected, &port.stderr).unwrap_or_default();
    text
}

/// Whether `outcome`'s stderr ends with the line `message`.
fn said(outcome: &Outcome, message: &str) -> bool {
    let eol: &[u8] = if cfg!(windows) { b"\r\n" } else { b"\n" };
    outcome
        .stderr
        .strip_suffix(eol)
        .is_some_and(|text| text.ends_with(message.as_bytes()))
}

/// What differs between the runs of `case`, or nothing.
fn case_differences(case: &Case, wheel: &Outcome, port: &Outcome) -> String {
    let died = !(0..=255).contains(&wheel.exit_code);
    match case.stop {
        Some(Stop::Always(message)) => return stop_differences(wheel, port, message),
        Some(Stop::IfTheWheelDies(message)) if died => {
            return stop_differences(wheel, port, message);
        }
        Some(Stop::Profile { reads_outside, .. }) if said(port, reads_outside) => {
            return stop_differences(wheel, port, reads_outside);
        }
        Some(Stop::Profile { dies, .. }) if died => {
            return stop_differences(wheel, port, dies);
        }
        _ => {}
    }
    if !case.icc {
        return differences(wheel, port);
    }
    let (mut wheel, mut port) = (wheel.clone(), port.clone());
    let times = without_creation_time(&mut wheel.files, &mut port.files);
    times + &differences(&wheel, &port)
}

/// What the wheel's runs of a sweep gave: how a test checks that the cases do what they are for.
#[derive(Debug)]
pub(crate) struct Summary {
    pub(crate) outcomes: Vec<Outcome>,
    /// How many cases of [`Stop::Profile`] the port ended with its second message.
    pub(crate) port_stops: usize,
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
    let mut port_stops = 0;
    for (index, case) in cases.iter().enumerate() {
        let finished = (0..=255).contains(&wheel[index].exit_code);
        let either_way = matches!(
            case.stop,
            Some(Stop::IfTheWheelDies(_) | Stop::Profile { .. })
        );
        if let Some(Stop::Profile { reads_outside, .. }) = case.stop
            && said(&port[index], reads_outside)
        {
            port_stops += 1;
        }
        assert!(
            either_way || finished != case.dies,
            "{}: the wheel ended with {}: {}",
            case.label,
            wheel[index].exit_code,
            show(&wheel[index].stderr)
        );
        let text = case_differences(case, &wheel[index], &port[index]);
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
    Summary {
        outcomes: wheel,
        port_stops,
    }
}

/// Windows only: `argv` writes the file `output`, which exists with each combination of
/// attributes. The wheel refuses a hidden or a system file, and leaves it alone.
#[cfg(windows)]
pub(crate) fn output_attribute_cases(
    output: &str,
    argv: &[&str],
    base: impl Fn(Case) -> Case,
) -> Vec<Case> {
    ["h", "s", "r", "hs", "hr", "sr"]
        .iter()
        .map(|letters| {
            base(Case::new(format!("attribute_{letters}"), argv))
                .file(output, "old")
                .attribute(output, letters)
        })
        .collect()
}

/// Runs one case on both sides, for a test that doesn't expect them to match: a case the wheel
/// doesn't survive, or that the port deliberately refuses. Returns (the wheel's, the port's).
pub(crate) fn run_both(group: &str, tool: &str, program: &str, case: &Case) -> (Outcome, Outcome) {
    let wheel = wheel_outcomes(group, tool, std::slice::from_ref(case)).remove(0);
    let port = port_outcome(group, program, 0, case);
    (wheel, port)
}
