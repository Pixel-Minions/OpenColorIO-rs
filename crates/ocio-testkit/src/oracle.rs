// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The live oracle: the official `opencolorio==2.5.2` wheel, run on this machine.
//!
//! Each call starts `python -m ocio_oracle` in the oracle's own virtual environment (created
//! from `oracle/uv.lock` inside the target directory, so Windows and the Rocky Linux 9
//! container never share one) and exchanges one framed message each way:
//!
//! ```text
//! u32 little-endian header length | header (UTF-8 JSON) | blob 0 | blob 1 | ...
//! ```
//!
//! The request header is `{"cmd": str, "args": object, "blobs": [len, ...]}`. The response
//! header is `{"ok": true, "result": any, "blobs": [len, ...]}` or
//! `{"ok": false, "error": str}`. An OCIO exception that a command is probing for is part of
//! its `result`, not a protocol error.
//!
//! Responses are cached under `<target>/oracle-cache`, keyed by the request, the oracle's
//! sources and lock file, upstream's test files (which requests name by path), and this
//! machine's OS and CPU ([`machine_description`]). Set
//! `OCIO_RS_ORACLE_NO_CACHE=1` to bypass the cache. Environment variables named `OCIO`,
//! `OCIO_*` or `PYTHON*` are never passed to the oracle's processes ([`isolate`]), so their
//! results and output don't depend on the caller's environment.
//!
//! Under Intel SDE (`scripts/sde.sh`), the test process and the oracle it starts both see the
//! emulated CPU, so the wheel and the port dispatch to the SIMD kernels of that CPU. On
//! Windows, importing `PyOpenColorIO` calls `platform.system()`, which under SDE, where the
//! WMI query fails, falls back to starting `cmd /c ver`; Pin's injection into that child now
//! and then crashed the oracle (`0xC0000005`) before it read the request. The oracle's package
//! (`oracle/ocio_oracle/__init__.py`) now answers that version from `sys.getwindowsversion()`
//! without starting `ver`, before anything imports `PyOpenColorIO`. A crash before the request
//! is read is still started again, at most twice; it can't hide a bug of the wheel, since no
//! command ran ([`crashed_unread`]).
//!
//! Starting the oracle costs about a third of a second; a small `cpu_apply` inside it, about
//! a tenth of a millisecond. [`Oracle::batch`] runs many calls in one process (the `batch`
//! command), and the battery (`crate::battery`) sends all its calls that way.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde_json::{Value, json};
use xxhash_rust::xxh3::Xxh3;

use crate::paths;

/// A response from the oracle.
#[derive(Debug, Clone)]
pub struct Response {
    /// The command's JSON result.
    pub result: Value,
    /// Binary outputs, in the order the command produced them.
    pub blobs: Vec<Vec<u8>>,
}

impl Response {
    /// Blob `index` as little-endian `f32` values.
    pub fn blob_f32(&self, index: usize) -> Vec<f32> {
        bytes_to_f32(&self.blobs[index])
    }

    /// Blob `index` as UTF-8 text.
    pub fn blob_text(&self, index: usize) -> &str {
        std::str::from_utf8(&self.blobs[index]).expect("oracle blob is not UTF-8")
    }
}

/// One call of an [`Oracle::batch`].
#[derive(Debug, Clone)]
pub struct BatchCall<'a> {
    /// The command.
    pub cmd: &'a str,
    /// Its JSON arguments.
    pub args: Value,
    /// Its binary inputs.
    pub blobs: Vec<&'a [u8]>,
}

/// Handle to the oracle environment. Use [`Oracle::get`].
#[derive(Debug)]
pub struct Oracle {
    python: PathBuf,
    identity: u128,
    cache_dir: Option<PathBuf>,
}

impl Oracle {
    /// The oracle for this process. The first call creates or updates its environment.
    ///
    /// Panics with setup instructions if the oracle can't run (for example, uv is missing).
    pub fn get() -> &'static Oracle {
        static ORACLE: OnceLock<Oracle> = OnceLock::new();
        ORACLE.get_or_init(|| Oracle::setup().unwrap_or_else(|e| panic!("oracle setup: {e}")))
    }

    fn setup() -> Result<Oracle, String> {
        let oracle_dir = paths::oracle_dir();
        let venv = paths::target_dir().join("oracle-venv");
        let python = if cfg!(windows) {
            venv.join("Scripts").join("python.exe")
        } else {
            venv.join("bin").join("python")
        };
        // A target directory restored from a build cache, or a moved uv Python install, can
        // leave an environment whose interpreter no longer runs; uv refuses to reuse it.
        // Rebuild it from the lock file instead.
        if venv.exists() && !python_runs(&python) {
            std::fs::remove_dir_all(&venv)
                .map_err(|e| format!("could not remove the broken {}: {e}", venv.display()))?;
        }
        let sync = || {
            Command::new(uv_program())
                .arg("sync")
                .arg("--project")
                .arg(&oracle_dir)
                .args(["--frozen", "--quiet", "--no-install-project"])
                .env("UV_PROJECT_ENVIRONMENT", &venv)
                .env_remove("VIRTUAL_ENV")
                .stdin(Stdio::null())
                .status()
                .map_err(|e| {
                    format!("could not run `uv` ({e}); install it: https://docs.astral.sh/uv/")
                })
        };
        let mut status = sync()?;
        if !status.success() && venv.exists() {
            // One retry from scratch, for any other kind of stale environment.
            let _ = std::fs::remove_dir_all(&venv);
            status = sync()?;
        }
        if !status.success() {
            return Err(format!(
                "`uv sync` for {} failed: {status}",
                oracle_dir.display()
            ));
        }
        let cache_dir = if std::env::var_os("OCIO_RS_ORACLE_NO_CACHE").is_some() {
            None
        } else {
            Some(paths::target_dir().join("oracle-cache"))
        };
        Ok(Oracle {
            python,
            identity: identity(&oracle_dir)?,
            cache_dir,
        })
    }

    /// The oracle environment's Python interpreter.
    pub fn python(&self) -> &Path {
        &self.python
    }

    /// A command that runs the oracle environment's Python as the oracle runs: from the oracle's
    /// directory, so `ocio_oracle` imports, in the caller's environment less what [`isolate`]
    /// removes. Every process of the oracle starts this way.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.python);
        command.current_dir(paths::oracle_dir());
        isolate(&mut command);
        command
    }

    /// Runs `script` in the oracle's environment, as the oracle runs (from its directory, so
    /// `ocio_oracle` imports, and without the caller's `OCIO` variables), with `args`, and
    /// returns the lines it prints. For tests that read the wheel independently of a command,
    /// or reach a check no request can. Panics if the script fails. Nothing is cached.
    #[track_caller]
    pub fn run_script(&self, script: &str, args: &[String]) -> Vec<String> {
        let mut command = self.command();
        command.args(["-X", "utf8", "-c", script]).args(args);
        let output = command.output().expect("the oracle's Python runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        stdout.lines().map(str::to_string).collect()
    }

    /// Runs `cmd` with JSON `args` and binary inputs. Panics on a protocol error.
    #[track_caller]
    pub fn call(&self, cmd: &str, args: Value, blobs: &[&[u8]]) -> Response {
        match self.try_call(cmd, args, blobs) {
            Ok(r) => r,
            Err(e) => panic!("oracle `{cmd}` failed: {e}"),
        }
    }

    /// Runs `cmd` with JSON `args` and binary inputs.
    pub fn try_call(&self, cmd: &str, args: Value, blobs: &[&[u8]]) -> Result<Response, String> {
        self.try_call_with(cmd, args, blobs, true)
    }

    /// Runs `cmd` without the response cache: for large one-off requests (the battery's
    /// exhaustive tier), whose responses would only fill the disk. Panics on a protocol error.
    #[track_caller]
    pub fn call_uncached(&self, cmd: &str, args: Value, blobs: &[&[u8]]) -> Response {
        match self.try_call_with(cmd, args, blobs, false) {
            Ok(r) => r,
            Err(e) => panic!("oracle `{cmd}` failed: {e}"),
        }
    }

    /// Runs `calls` in one oracle process with the `batch` command and returns their
    /// responses, in order: a call that raised in the oracle gives `Err` with its traceback,
    /// and the other calls still run. Identical input blobs are sent once. With `cache`, the
    /// whole batch is one cache entry, but never a batch with a call that raised: the failure
    /// may be transient (a `MemoryError`), and a cached one would replay on every later run.
    /// Panics on a protocol error.
    #[track_caller]
    pub fn batch(&self, calls: &[BatchCall<'_>], cache: bool) -> Vec<Result<Response, String>> {
        let (args, blobs) = batch_request(calls);
        let response = match self.try_call_with("batch", args, &blobs, cache) {
            Ok(r) => r,
            Err(e) => panic!("oracle `batch` of {} calls failed: {e}", calls.len()),
        };
        let results = response
            .result
            .as_array()
            .unwrap_or_else(|| panic!("oracle `batch` returned {}", response.result));
        assert_eq!(results.len(), calls.len(), "oracle `batch` result count");
        let mut out_blobs: Vec<Option<Vec<u8>>> = response.blobs.into_iter().map(Some).collect();
        results
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                if let Some(error) = entry.get("error") {
                    return Err(format!(
                        "oracle call {i} of {} (`{}`) failed: {}",
                        calls.len(),
                        calls[i].cmd,
                        error.as_str().unwrap_or_default()
                    ));
                }
                let blobs = entry["blobs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|i| {
                        let i = i.as_u64().expect("blob index") as usize;
                        out_blobs[i]
                            .take()
                            .expect("each response blob belongs to one call")
                    })
                    .collect();
                Ok(Response {
                    result: entry["result"].clone(),
                    blobs,
                })
            })
            .collect()
    }

    fn try_call_with(
        &self,
        cmd: &str,
        args: Value,
        blobs: &[&[u8]],
        cache: bool,
    ) -> Result<Response, String> {
        let request = request(cmd, args, blobs);
        let cache_file = self.cache_file(&request).filter(|_| cache);
        if let Some(file) = &cache_file
            && let Ok(bytes) = std::fs::read(file)
            && let Ok(response) = parse_response(&bytes)
            && cacheable(cmd, &response)
        {
            return Ok(response);
        }

        let output = self.run(&request)?;
        let response = response_of(&output)?;
        if let Some(file) = &cache_file
            && cacheable(cmd, &response)
        {
            write_atomically(file, &output.stdout);
        }
        Ok(response)
    }

    /// Where the response to `request` is cached, unless the cache is off.
    fn cache_file(&self, request: &[u8]) -> Option<PathBuf> {
        self.cache_dir.as_ref().map(|dir| {
            let mut h = Xxh3::new();
            h.update(&self.identity.to_le_bytes());
            h.update(request);
            dir.join(format!("{:032x}.bin", h.digest128()))
        })
    }

    fn run(&self, request: &[u8]) -> Result<Output, String> {
        let mut command = self.command();
        command.args(["-X", "utf8", "-m", "ocio_oracle"]);
        exchange(command, request)
    }
}

/// Removes from `command`'s environment the variables named `OCIO` or `OCIO_*`, which would
/// change what OCIO does, and every `PYTHON*` variable, which would change what Python does or
/// writes (`PYTHONPATH`, `PYTHONHOME`, `PYTHONVERBOSE`, `PYTHONWARNINGS`, ...); then sets
/// `PYTHONDONTWRITEBYTECODE=1`, so the oracle leaves no `__pycache__` in the checkout.
pub fn isolate(command: &mut Command) -> &mut Command {
    isolate_from(command, std::env::vars_os().map(|(key, _)| key))
}

/// [`isolate`], for the variable names `names`. Names compare in ASCII upper case: Windows
/// looks variables up whatever their case (`pythonoptimize` sets Python's `-O` there), and on
/// Linux removing a lower-case name changes nothing the oracle reads.
fn isolate_from(
    command: &mut Command,
    names: impl IntoIterator<Item = std::ffi::OsString>,
) -> &mut Command {
    for key in names {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if name == "OCIO" || name.starts_with("OCIO_") || name.starts_with("PYTHON") {
            command.env_remove(&key);
        }
    }
    command.env("PYTHONDONTWRITEBYTECODE", "1")
}

/// What an oracle process that exited successfully wrote.
#[derive(Debug)]
struct Output {
    /// The framed response.
    stdout: Vec<u8>,
    /// Anything else it printed, as text.
    stderr: String,
}

/// The response in `output`. A response whose framing is broken (fewer or more bytes than its
/// header declares) is an error that carries the oracle's stderr: the process exited
/// successfully, so its stderr is the only trace of what went wrong while it wrote.
fn response_of(output: &Output) -> Result<Response, String> {
    let (header, blobs) = parse_frame(&output.stdout).map_err(|e| {
        format!(
            "{e}: the oracle exited successfully after writing {} bytes to stdout; its \
             stderr:\n{}",
            output.stdout.len(),
            output.stderr
        )
    })?;
    response_from_frame(&header, blobs)
}

/// Starts `command`, writes `request` to its stdin, and returns its output once it exits
/// successfully, starting it again (at most [`SPAWN_RETRIES`] times) when it crashed as the
/// oracle crashes under Intel SDE on Windows before it reads a request ([`crashed_unread`]).
/// The final error says how many attempts were made, with each attempt's message and stderr;
/// each retry also prints a line to the process's stderr.
fn exchange(mut command: Command, request: &[u8]) -> Result<Output, String> {
    let mut failures = Vec::new();
    loop {
        match exchange_once(&mut command, request) {
            Ok(output) => return Ok(output),
            Err(failure) => {
                let retry = failure.crashed_unread && failures.len() < SPAWN_RETRIES;
                failures.push(failure.message);
                if !retry {
                    break;
                }
                // Straight to the process's stderr, past the test harness's capture, so that
                // CI logs show every retry, of passing tests too.
                let _ = writeln!(
                    std::io::stderr(),
                    "ocio-testkit: the oracle crashed before reading its request (attempt {} \
                     of {}, exit code 0xc0000005); starting it again",
                    failures.len(),
                    SPAWN_RETRIES + 1
                );
            }
        }
    }
    if failures.len() == 1 {
        return Err(failures.pop().unwrap_or_default());
    }
    let attempts: Vec<String> = failures
        .iter()
        .enumerate()
        .map(|(i, message)| format!("attempt {}: {message}", i + 1))
        .collect();
    Err(format!(
        "the oracle failed after {} attempts (it crashed before reading the request, and was \
         started again):\n{}",
        failures.len(),
        attempts.join("\n")
    ))
}

/// How many times [`exchange`] starts the oracle again after [`crashed_unread`].
const SPAWN_RETRIES: usize = 2;

/// Windows' `STATUS_ACCESS_VIOLATION` (0xC0000005), as [`std::process::ExitStatus::code`]
/// gives it.
const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;

/// Whether an oracle process that failed crashed before it read its whole request, the way it
/// does under Intel SDE on Windows: on Windows only, writing the request failed, the process
/// exited with `STATUS_ACCESS_VIOLATION`, and it wrote nothing to stdout.
///
/// The cause (seen with `faulthandler`, 2026-10-01): `PyOpenColorIO/__init__.py:10` calls
/// `platform.system()`. Natively, Python 3.13 asks WMI for the Windows version; under SDE the
/// WMI query raises `OSError('not supported')`, so `platform._syscmd_ver` starts `cmd /c ver`,
/// and Pin, injecting itself into that child of an instrumented process, now and then crashes
/// the oracle. The crash comes before the wheel's native module is even loaded.
///
/// Starting the oracle again can't hide a bug of the wheel: the oracle reads the whole request
/// before it runs any command (`serve_one`, `oracle/ocio_oracle/__main__.py`, which reads the
/// header and every blob before it looks the command up), so a request it didn't take whole
/// ran nothing. A crash after the request is read leaves the write complete, and is reported
/// at once; a crash at every start, such as one in the wheel's import, is reported after the
/// last attempt.
fn crashed_unread(
    write_failed: bool,
    code: Option<i32>,
    stdout_empty: bool,
    windows: bool,
) -> bool {
    windows && write_failed && code == Some(STATUS_ACCESS_VIOLATION) && stdout_empty
}

/// One attempt of [`exchange`] that failed.
struct Failure {
    /// What went wrong, with the oracle's stderr.
    message: String,
    /// [`crashed_unread`] holds.
    crashed_unread: bool,
}

/// Starts `command`, writes `request` to its stdin, and returns its output once it exits
/// successfully. Each pipe has its own thread or loop, so the process never waits on a full
/// pipe: the request is written on a thread (a large response can't deadlock a large
/// request), stderr is read on another (a process that writes more than a pipe holds to
/// stderr before it closes stdout can't hang), and stdout is read here.
fn exchange_once(command: &mut Command, request: &[u8]) -> Result<Output, Failure> {
    let fail = |message: String| Failure {
        message,
        crashed_unread: false,
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .spawn()
        .map_err(|e| fail(format!("could not start {program}: {e}")))?;

    // Both directions move at most PIPE_PIECE bytes per call: on Windows, one pipe read or
    // write of several megabytes can fail when the system is short of kernel memory, and a
    // failed write left the oracle with an empty request ("EOFError: expected 4 bytes, got 0").
    let mut stdin = child.stdin.take().expect("piped stdin");
    let request_len = request.len();
    let request = request.to_vec();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for piece in request.chunks(PIPE_PIECE) {
            stdin.write_all(piece)?;
        }
        Ok(())
    });
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    let stderr_reader = std::thread::spawn(move || read_in_pieces(&mut stderr_pipe));
    let stdout = read_in_pieces(&mut child.stdout.take().expect("piped stdout"));
    let stderr = stderr_reader
        .join()
        .ok()
        .and_then(Result::ok)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    let status = child.wait().map_err(|e| fail(e.to_string()))?;
    let stdout = stdout.map_err(|e| fail(format!("reading the oracle's response failed: {e}")))?;
    let written = writer
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::other("the writer thread panicked")));
    // The exit status says how the oracle died when it stopped reading early: under Intel
    // SDE on Windows, it has exited mid-request without writing anything to stderr
    // (crashed_unread).
    if let Err(e) = written {
        return Err(Failure {
            message: format!(
                "writing the {request_len}-byte request to the oracle failed: {e}; the oracle \
                 exited with {status}\n{stderr}"
            ),
            crashed_unread: crashed_unread(true, status.code(), stdout.is_empty(), cfg!(windows)),
        });
    }
    if !status.success() {
        return Err(fail(format!("oracle exited with {status}\n{stderr}")));
    }
    Ok(Output { stdout, stderr })
}

/// The most bytes one read or write moves through a pipe to or from the oracle.
const PIPE_PIECE: usize = 64 * 1024;

/// Reads `stream` to its end, at most [`PIPE_PIECE`] bytes per read.
fn read_in_pieces(stream: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut piece = vec![0u8; PIPE_PIECE];
    loop {
        match stream.read(&mut piece) {
            Ok(0) => return Ok(out),
            Ok(n) => out.extend_from_slice(&piece[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// Whether `python` exists and starts.
fn python_runs(python: &Path) -> bool {
    python.is_file()
        && isolate(&mut Command::new(python))
            .args(["-c", "pass"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

fn uv_program() -> PathBuf {
    std::env::var_os("OCIO_RS_UV").map_or_else(|| PathBuf::from("uv"), PathBuf::from)
}

/// Hash of everything that determines the oracle's answers on this machine.
fn identity(oracle_dir: &Path) -> Result<u128, String> {
    identity_with(
        oracle_dir,
        &paths::upstream_dir().join("tests").join("data"),
    )
}

/// [`identity`], with upstream's test files in `test_data`.
fn identity_with(oracle_dir: &Path, test_data: &Path) -> Result<u128, String> {
    let mut files = Vec::new();
    for name in ["pyproject.toml", "uv.lock", ".python-version"] {
        files.push(oracle_dir.join(name));
    }
    collect_files(&oracle_dir.join("ocio_oracle"), &mut files);
    files.sort();

    let mut h = Xxh3::new();
    for file in &files {
        let rel = file.strip_prefix(oracle_dir).unwrap_or(file);
        h.update(rel.to_string_lossy().replace('\\', "/").as_bytes());
        let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
        h.update(&(bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    hash_test_data(&mut h, test_data);
    h.update(machine_description().as_bytes());
    Ok(h.digest128())
}

/// Hashes upstream's test files (`upstream/OpenColorIO/tests/data`), which requests name by
/// path (a CLF to load, a config's search path), into the cache key: each file's path, length
/// and bytes, in path order. Without the submodule checked out, the key differs from any with
/// it, so a response made then (an `ExceptionMissingFile`) is never replayed once the files
/// are there. About 260 files, 33 MB, hashed once per process.
fn hash_test_data(h: &mut Xxh3, data: &Path) {
    let mut files = Vec::new();
    collect_files(data, &mut files);
    files.sort();
    h.update(b"upstream test data");
    h.update(&(files.len() as u64).to_le_bytes());
    for file in &files {
        let rel = file.strip_prefix(data).unwrap_or(file);
        h.update(rel.to_string_lossy().replace('\\', "/").as_bytes());
        match std::fs::read(file) {
            Ok(bytes) => {
                h.update(&(bytes.len() as u64).to_le_bytes());
                h.update(&bytes);
            }
            Err(_) => h.update(b"unreadable"),
        }
    }
}

/// Every file under `dir`, at any depth, except Python's bytecode caches (`__pycache__`),
/// which the interpreter may write and which don't change what the oracle does.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "__pycache__") {
                collect_files(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

/// OS, architecture, CPU model, and what OCIO's `CPUInfo` reads: the CPUID vendor and
/// signature (family, model and stepping, which decide the "slow" flags such as AVX2
/// slow-gather on Haswell) and the SIMD features.
///
/// The CPUID values are what this process sees. Under Intel SDE they describe the emulated CPU,
/// while the OS still reports the host's CPU model. That keeps the cache of each emulated CPU
/// apart, even for two CPUs with the same features (Haswell and Skylake).
pub fn machine_description() -> String {
    let mut s = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    if let Some(cpu) = cpu_model() {
        s.push_str(" cpu=");
        s.push_str(&cpu);
    }
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;
        let leaf0 = __cpuid(0);
        let vendor: Vec<u8> = [leaf0.ebx, leaf0.edx, leaf0.ecx]
            .iter()
            .flat_map(|r| r.to_le_bytes())
            .collect();
        s.push_str(&format!(
            " cpuid={}:{:08x}",
            String::from_utf8_lossy(&vendor),
            __cpuid(1).eax
        ));

        let features = [
            ("sse2", std::arch::is_x86_feature_detected!("sse2")),
            ("sse3", std::arch::is_x86_feature_detected!("sse3")),
            ("ssse3", std::arch::is_x86_feature_detected!("ssse3")),
            ("sse4.1", std::arch::is_x86_feature_detected!("sse4.1")),
            ("sse4.2", std::arch::is_x86_feature_detected!("sse4.2")),
            ("avx", std::arch::is_x86_feature_detected!("avx")),
            ("avx2", std::arch::is_x86_feature_detected!("avx2")),
            ("fma", std::arch::is_x86_feature_detected!("fma")),
            ("f16c", std::arch::is_x86_feature_detected!("f16c")),
            ("avx512f", std::arch::is_x86_feature_detected!("avx512f")),
            ("avx512dq", std::arch::is_x86_feature_detected!("avx512dq")),
            ("avx512bw", std::arch::is_x86_feature_detected!("avx512bw")),
            ("avx512vl", std::arch::is_x86_feature_detected!("avx512vl")),
        ];
        s.push_str(" features=");
        let names: Vec<_> = features
            .iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| *n)
            .collect();
        s.push_str(&names.join(","));
    }
    s
}

fn cpu_model() -> Option<String> {
    if cfg!(windows) {
        return std::env::var("PROCESSOR_IDENTIFIER").ok();
    }
    let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let field = |name: &str| {
        info.lines()
            .find(|l| l.split(':').next().is_some_and(|k| k.trim() == name))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string())
    };
    Some(format!(
        "{} family {} model {}",
        field("model name")?,
        field("cpu family").unwrap_or_default(),
        field("model").unwrap_or_default()
    ))
}

/// The framed request for `cmd` with `args` and `blobs`.
fn request(cmd: &str, args: Value, blobs: &[&[u8]]) -> Vec<u8> {
    let header = json!({
        "cmd": cmd,
        "args": args,
        "blobs": blobs.iter().map(|b| b.len()).collect::<Vec<_>>(),
    });
    frame(&header, blobs)
}

/// The `batch` command's arguments for `calls`, and its blobs. Identical blobs are sent once,
/// deduplicated by address first (the usual case: one probe buffer, many calls), then by
/// content, so the request doesn't depend on where the buffers live.
fn batch_request<'a>(calls: &[BatchCall<'a>]) -> (Value, Vec<&'a [u8]>) {
    let mut by_address: HashMap<(usize, usize), usize> = HashMap::new();
    let mut by_content: HashMap<u128, Vec<usize>> = HashMap::new();
    let mut blobs: Vec<&'a [u8]> = Vec::new();
    let mut entries = Vec::with_capacity(calls.len());
    for call in calls {
        let mut indices = Vec::with_capacity(call.blobs.len());
        for &blob in &call.blobs {
            let address = (blob.as_ptr() as usize, blob.len());
            let index = *by_address.entry(address).or_insert_with(|| {
                let same = by_content
                    .entry(xxhash_rust::xxh3::xxh3_128(blob))
                    .or_default();
                if let Some(&i) = same.iter().find(|&&i| blobs[i] == blob) {
                    return i;
                }
                blobs.push(blob);
                same.push(blobs.len() - 1);
                blobs.len() - 1
            });
            indices.push(index);
        }
        entries.push(json!({"cmd": call.cmd, "args": call.args, "blobs": indices}));
    }
    (json!({ "calls": entries }), blobs)
}

/// Whether a response may be cached, or replayed from the cache: every one but a batch in
/// which a call raised, whose failure may be transient. (A command that raises outside a
/// batch fails the whole call, and a failed call is never cached.)
fn cacheable(cmd: &str, response: &Response) -> bool {
    cmd != "batch"
        || !response
            .result
            .as_array()
            .is_some_and(|calls| calls.iter().any(|call| call.get("error").is_some()))
}

fn frame(header: &Value, blobs: &[&[u8]]) -> Vec<u8> {
    let header = serde_json::to_vec(header).expect("JSON header");
    let len = u32::try_from(header.len()).expect("header under 4 GiB");
    let mut out =
        Vec::with_capacity(4 + header.len() + blobs.iter().map(|b| b.len()).sum::<usize>());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&header);
    for blob in blobs {
        out.extend_from_slice(blob);
    }
    out
}

/// The response framed in `bytes`, or what is wrong with it: its framing, or the error the
/// oracle reported.
fn parse_response(bytes: &[u8]) -> Result<Response, String> {
    let (header, blobs) = parse_frame(bytes)?;
    response_from_frame(&header, blobs)
}

/// The response of a well-framed `header` and its `blobs`, or the error the oracle reported.
fn response_from_frame(header: &Value, blobs: Vec<Vec<u8>>) -> Result<Response, String> {
    if header["ok"] != Value::Bool(true) {
        return Err(header["error"]
            .as_str()
            .unwrap_or("unknown oracle error")
            .to_string());
    }
    Ok(Response {
        result: header["result"].clone(),
        blobs,
    })
}

/// The header and blobs of a framed response, or what is wrong with its framing: a header
/// that is cut short or isn't JSON, or fewer or more bytes than its blob sizes add up to.
fn parse_frame(bytes: &[u8]) -> Result<(Value, Vec<Vec<u8>>), String> {
    let len_bytes: [u8; 4] = bytes
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .ok_or("oracle response is shorter than its frame header")?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    let header_bytes = bytes
        .get(4..4 + len)
        .ok_or("oracle response header is truncated")?;
    let header: Value = serde_json::from_slice(header_bytes).map_err(|e| e.to_string())?;
    let mut offset = 4 + len;
    let mut blobs = Vec::new();
    for size in header["blobs"].as_array().into_iter().flatten() {
        let size = size.as_u64().ok_or("blob size is not an integer")? as usize;
        let blob = bytes
            .get(offset..offset + size)
            .ok_or("oracle response blob is truncated")?;
        blobs.push(blob.to_vec());
        offset += size;
    }
    if offset != bytes.len() {
        return Err(format!(
            "{} trailing bytes after the oracle response",
            bytes.len() - offset
        ));
    }
    Ok((header, blobs))
}

fn write_atomically(file: &Path, bytes: &[u8]) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, file).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// `f32` values as little-endian bytes, for oracle blobs.
pub fn f32_to_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Little-endian bytes as `f32` values.
pub fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    assert!(
        bytes.len().is_multiple_of(4),
        "blob length {} is not a multiple of 4",
        bytes.len()
    );
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {

    /// The test data's hash, as the cache key takes it.
    fn data_hash(dir: &Path) -> u128 {
        let mut h = Xxh3::new();
        hash_test_data(&mut h, dir);
        h.digest128()
    }

    /// Test data that is missing, added, or changed in its bytes or path gives another key, so
    /// a response made without upstream's files (or with others) is never replayed.
    #[test]
    fn the_cache_key_follows_the_test_data() {
        let dir = paths::target_dir().join(format!("oracle_key_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let missing = data_hash(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let empty = data_hash(&dir);
        std::fs::write(dir.join("sub").join("a.clf"), b"one").unwrap();
        let one = data_hash(&dir);
        std::fs::write(dir.join("sub").join("a.clf"), b"two").unwrap();
        let two = data_hash(&dir);
        std::fs::rename(dir.join("sub").join("a.clf"), dir.join("sub").join("b.clf")).unwrap();
        let renamed = data_hash(&dir);
        std::fs::write(dir.join("sub").join("b.clf"), b"two").unwrap();
        assert_eq!(data_hash(&dir), renamed);
        std::fs::remove_dir_all(&dir).unwrap();
        let keys = [missing, one, two, renamed];
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a, b);
            }
        }
        // An empty directory and a missing one have no file either way.
        assert_eq!(missing, empty);
    }

    /// A process that writes more to stderr than a pipe holds before it closes stdout still
    /// gives its stdout back: stderr is drained while stdout is read. A hang fails the test
    /// after two minutes rather than stalling the run.
    #[test]
    fn a_large_stderr_does_not_block_the_exchange() {
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            large_stderr_exchange();
            let _ = done.send(());
        });
        match finished.recv_timeout(std::time::Duration::from_secs(120)) {
            Ok(()) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("the exchange didn't return in 120 s: blocked on stderr")
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("the exchange failed (its panic is printed above)")
            }
        }
    }

    /// The oracle is started again only after a crash before it read its request, and only on
    /// Windows: writing failed, the exit code is `STATUS_ACCESS_VIOLATION`, stdout is empty.
    #[test]
    fn only_a_crash_before_the_request_is_read_is_retried() {
        let av = Some(super::STATUS_ACCESS_VIOLATION);
        assert!(super::crashed_unread(true, av, true, true));
        // Never off Windows.
        assert!(!super::crashed_unread(true, av, true, false));
        // The request was taken whole: a command may have run.
        assert!(!super::crashed_unread(false, av, true, true));
        // Another exit code, or a response begun.
        assert!(!super::crashed_unread(true, Some(1), true, true));
        assert!(!super::crashed_unread(true, None, true, true));
        assert!(!super::crashed_unread(true, av, false, true));
    }

    /// A fake oracle for the retry tests: it counts its starts in `counter`, and for the first
    /// `crashes` starts exits with `STATUS_ACCESS_VIOLATION`, without reading stdin (`unread`)
    /// or after reading all of it (`read`); later starts echo the request reversed.
    #[cfg(windows)]
    fn fake_oracle(counter: &Path, crashes: u32, mode: &str) -> Command {
        let mut command = super::Oracle::get().command();
        command.args([
            "-c",
            "import os, sys\n\
             path, crashes, mode = sys.argv[1], int(sys.argv[2]), sys.argv[3]\n\
             n = (int(open(path).read()) if os.path.exists(path) else 0) + 1\n\
             open(path, 'w').write(str(n))\n\
             data = sys.stdin.buffer.read() if mode == 'read' else None\n\
             if n <= crashes:\n\
             \x20   sys.stderr.write(f'crash at start {n}\\n')\n\
             \x20   sys.stderr.flush()\n\
             \x20   os._exit(0xC0000005 - 2**32)\n\
             data = data if data is not None else sys.stdin.buffer.read()\n\
             sys.stdout.buffer.write(data[::-1])\n",
        ]);
        command.arg(counter).arg(crashes.to_string()).arg(mode);
        command
    }

    /// A request larger than any pipe holds, so a process that exits without reading it makes
    /// the write fail.
    #[cfg(windows)]
    fn large_request() -> Vec<u8> {
        (0..=255u8).cycle().take(4 << 20).collect()
    }

    /// A counter file of its own for each test, absent.
    #[cfg(windows)]
    fn counter(name: &str) -> PathBuf {
        let path = paths::target_dir().join(format!("oracle-retry-{name}.txt"));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[cfg(windows)]
    fn starts(counter: &Path) -> String {
        std::fs::read_to_string(counter).expect("the counter")
    }

    /// The process exits with `STATUS_ACCESS_VIOLATION` (as Windows reports it) before it
    /// reads the request, once: it is started again, and the second start answers.
    #[cfg(windows)]
    #[test]
    fn a_crash_before_the_request_is_read_is_retried() {
        let counter = counter("once");
        let request = large_request();
        let stdout = super::exchange(fake_oracle(&counter, 1, "unread"), &request)
            .expect("the second start answers")
            .stdout;
        assert_eq!(stdout, request.iter().rev().copied().collect::<Vec<u8>>());
        assert_eq!(starts(&counter), "2");
    }

    /// A process that crashes at every start is started 3 times (2 retries), and the error says
    /// so, with every attempt's stderr.
    #[cfg(windows)]
    #[test]
    fn a_crash_at_every_start_gives_the_attempt_count() {
        let counter = counter("always");
        let error = super::exchange(fake_oracle(&counter, 99, "unread"), &large_request())
            .expect_err("every start crashes");
        assert_eq!(starts(&counter), "3");
        assert!(
            error.starts_with("the oracle failed after 3 attempts"),
            "{error}"
        );
        for n in 1..=3 {
            assert!(
                error.contains(&format!("attempt {n}: writing the")),
                "{error}"
            );
            assert!(error.contains(&format!("crash at start {n}")), "{error}");
        }
        assert!(error.contains("0xc0000005"), "{error}");
    }

    /// A process that reads the whole request and then crashes may have run a command: it is
    /// not started again.
    #[cfg(windows)]
    #[test]
    fn a_crash_after_the_request_is_read_is_not_retried() {
        let counter = counter("read");
        let error = super::exchange(fake_oracle(&counter, 99, "read"), &large_request())
            .expect_err("the start crashes");
        assert_eq!(starts(&counter), "1");
        assert!(error.starts_with("oracle exited with"), "{error}");
        assert!(!error.contains("attempts"), "{error}");
    }

    /// A process that exits successfully with a response shorter than its header declares, as
    /// the oracle did when writing a blob failed under memory pressure (it then appended an
    /// error frame and exited with 0): the error says the blob is truncated and that the
    /// process exited successfully, and carries its stderr.
    #[test]
    fn a_truncated_response_reports_the_oracle_stderr() {
        let mut command = super::Oracle::get().command();
        command.args([
            "-c",
            "import json, struct, sys\n\
             sys.stdin.buffer.read()\n\
             header = json.dumps({'ok': True, 'result': None, 'blobs': [1 << 20]}).encode()\n\
             sys.stdout.buffer.write(struct.pack('<I', len(header)) + header + bytes(1000))\n\
             error = json.dumps({'ok': False, 'error': 'OSError', 'blobs': []}).encode()\n\
             sys.stdout.buffer.write(struct.pack('<I', len(error)) + error)\n\
             sys.stderr.write('the write of blob 0 failed\\n')\n",
        ]);
        let output = super::exchange(command, b"request").expect("the process exits with 0");
        let error = super::response_of(&output).expect_err("the response is truncated");
        assert!(
            error.starts_with("oracle response blob is truncated: the oracle exited successfully"),
            "{error}"
        );
        assert!(error.contains("the write of blob 0 failed"), "{error}");
    }

    /// Runs the oracle (`python -m ocio_oracle`) on pipes it sees through a recorder: each read
    /// and write goes to the real stdin or stdout, and the sizes are recorded. With
    /// `fail_after`, the first write that would take stdout past that many bytes raises
    /// `OSError` (errno 22, as Python reports a failed `WriteFile`), as one write does when the
    /// system is short of memory; later writes go through. At exit it prints `pipes: <largest
    /// read> <largest write> <bytes written> <writes after the failure>` to stderr.
    fn recorded_oracle(fail_after: Option<usize>) -> Command {
        let mut command = super::Oracle::get().command();
        command.args([
            "-X",
            "utf8",
            "-c",
            "import atexit, io, os, runpy, sys\n\
             limit = int(sys.argv[1]) if len(sys.argv) > 1 else None\n\
             stats = {'read': 0, 'write': 0, 'sent': 0, 'after': 0, 'failed': False}\n\
             class Pipe(io.RawIOBase):\n\
             \x20   def __init__(self, fd): self.fd = fd\n\
             \x20   def readable(self): return self.fd == 0\n\
             \x20   def writable(self): return self.fd == 1\n\
             \x20   def readinto(self, b):\n\
             \x20       stats['read'] = max(stats['read'], len(b))\n\
             \x20       data = os.read(self.fd, len(b))\n\
             \x20       b[:len(data)] = data\n\
             \x20       return len(data)\n\
             \x20   def write(self, b):\n\
             \x20       stats['write'] = max(stats['write'], len(b))\n\
             \x20       if stats['failed']:\n\
             \x20           stats['after'] += 1\n\
             \x20       elif limit is not None and stats['sent'] + len(b) > limit:\n\
             \x20           stats['failed'] = True\n\
             \x20           raise OSError(22, 'Invalid argument')\n\
             \x20       n = os.write(self.fd, b)\n\
             \x20       stats['sent'] += n\n\
             \x20       return n\n\
             atexit.register(lambda: sys.__stderr__.write(\n\
             \x20   'pipes: %(read)d %(write)d %(sent)d %(after)d\\n' % stats))\n\
             # Held here too, as sys.__stdout__ holds the real one: the oracle replaces\n\
             # sys.stdout, and that must not close the pipe.\n\
             pipes = [io.TextIOWrapper(io.BufferedReader(Pipe(0))),\n\
             \x20        io.TextIOWrapper(io.BufferedWriter(Pipe(1)))]\n\
             sys.stdin, sys.stdout = pipes\n\
             sys.argv = ['ocio_oracle']\n\
             runpy.run_module('ocio_oracle', run_name='__main__', alter_sys=True)\n",
        ]);
        if let Some(limit) = fail_after {
            command.arg(limit.to_string());
        }
        command
    }

    /// The recorder's `pipes:` line in `stderr`: the largest read and write, the bytes
    /// written, and the writes after the failure.
    fn pipe_stats(stderr: &str) -> [usize; 4] {
        let line = stderr
            .lines()
            .find_map(|l| l.strip_prefix("pipes: "))
            .unwrap_or_else(|| panic!("no pipe statistics in:\n{stderr}"));
        let values: Vec<usize> = line.split(' ').map(|v| v.parse().unwrap()).collect();
        values.try_into().unwrap()
    }

    /// A `cpu_apply` request and response of 1 MiB of pixels each, many pipe pieces long.
    fn large_cpu_apply() -> Vec<u8> {
        let pixels = f32_to_bytes(&[0.25f32, 0.5, 1.0, 1.0].repeat(1 << 16));
        let args = json!({"transform": {"class": "LogTransform", "args": {"base": 2.0}}});
        request("cpu_apply", args, &[&pixels])
    }

    /// The oracle reads its request and writes its response at most [`PIPE_PIECE`] bytes per
    /// call, as the caller does: one pipe read or write of megabytes can fail on Windows when
    /// the system is short of memory.
    #[test]
    fn the_oracle_moves_at_most_a_pipe_piece_per_call() {
        let output =
            super::exchange(recorded_oracle(None), &large_cpu_apply()).expect("a response");
        let response = super::response_of(&output).expect("the response");
        assert_eq!(response.blobs[0].len(), 1 << 20);
        let [read, write, sent, _] = pipe_stats(&output.stderr);
        assert!(read <= PIPE_PIECE, "a read of {read} bytes");
        assert!(write <= PIPE_PIECE, "a write of {write} bytes");
        assert_eq!(sent, output.stdout.len());
    }

    /// When writing the response fails after part of it went out (a blob, here), the oracle
    /// writes nothing more and exits with an error and the traceback on stderr. It used to
    /// append an error frame and exit with 0, which the caller read as a truncated blob
    /// ("oracle response blob is truncated").
    #[test]
    fn the_oracle_fails_without_a_second_frame_when_its_response_is_cut() {
        let limit = 100_000;
        let error = match super::exchange(recorded_oracle(Some(limit)), &large_cpu_apply()) {
            Err(error) => error,
            Ok(output) => panic!(
                "the oracle exited successfully: {:?}",
                super::response_of(&output).map(|r| r.result)
            ),
        };
        // `ExitStatus` displays as "exit code: 1" on Windows and "exit status: 1" on Linux.
        let exit = if cfg!(windows) {
            "exit code: 1"
        } else {
            "exit status: 1"
        };
        assert!(
            error.starts_with(&format!("oracle exited with {exit}\n")),
            "{error}"
        );
        assert!(
            error.contains("OSError: [Errno 22] Invalid argument"),
            "{error}"
        );
        let [_, _, sent, after] = pipe_stats(&error);
        assert!(sent <= limit && sent > 0, "{sent} bytes written");
        assert_eq!(after, 0, "the oracle kept writing after the failure");
    }

    fn large_stderr_exchange() {
        let mut command = super::Oracle::get().command();
        command.args([
            "-c",
            "import sys\n\
             request = sys.stdin.buffer.read()\n\
             sys.stderr.write('e' * (1 << 20))\n\
             sys.stderr.flush()\n\
             sys.stdout.buffer.write(request[::-1])\n",
        ]);
        let request: Vec<u8> = (0..=255u8).cycle().take(200_000).collect();
        let stdout = super::exchange(command, &request)
            .expect("the exchange")
            .stdout;
        let reversed: Vec<u8> = request.iter().rev().copied().collect();
        assert_eq!(stdout, reversed);
    }

    use super::*;

    /// A batch in which a call raised is never cached, and never replayed from the cache: the
    /// failure may be transient (a `MemoryError`), so the next run must ask the oracle again. A
    /// batch without failures is cached as before. (With `OCIO_RS_ORACLE_NO_CACHE` set there is
    /// no cache to check.)
    #[test]
    fn a_batch_with_a_failed_call_is_never_cached() {
        let oracle = Oracle::get();
        let pixels = f32_to_bytes(&[0.25, 0.5, 1.0, 1.0]);
        let log =
            |base: f64| json!({"transform": {"class": "LogTransform", "args": {"base": base}}});
        let calls = |args: Vec<Value>| -> Vec<BatchCall<'_>> {
            args.into_iter()
                .map(|args| BatchCall {
                    cmd: "cpu_apply",
                    args,
                    blobs: vec![pixels.as_slice()],
                })
                .collect()
        };
        let cache_file = |calls: &[BatchCall<'_>]| {
            let (args, blobs) = batch_request(calls);
            oracle.cache_file(&request("batch", args, &blobs))
        };

        // A call that raises: the batch isn't written to the cache.
        let failing = calls(vec![
            log(2.0),
            json!({"transform": {"class": "NoSuchTransform"}}),
        ]);
        let Some(failing_file) = cache_file(&failing) else {
            return;
        };
        let _ = std::fs::remove_file(&failing_file);
        let results = oracle.batch(&failing, true);
        assert!(results[0].is_ok() && results[1].is_err(), "{results:?}");
        assert!(
            !failing_file.exists(),
            "a batch with a failed call was cached"
        );

        // A cached batch with failed calls (an earlier version wrote them) isn't replayed: the
        // oracle runs again, and its good response replaces the entry.
        let passing = calls(vec![log(2.0), log(10.0)]);
        let passing_file = cache_file(&passing).expect("the cache is on");
        let stale = json!({
            "ok": true,
            "result": [
                {"error": "MemoryError (a stale entry)", "call": 0, "blobs": []},
                {"error": "MemoryError (a stale entry)", "call": 1, "blobs": []},
            ],
            "blobs": [],
        });
        write_atomically(&passing_file, &frame(&stale, &[]));
        let results = oracle.batch(&passing, true);
        assert!(results.iter().all(Result::is_ok), "{results:?}");
        let cached = parse_response(&std::fs::read(&passing_file).expect("cached")).unwrap();
        assert!(cacheable("batch", &cached), "{}", cached.result);
    }

    /// The cache identity covers every file of the oracle's package, whatever its extension,
    /// and not Python's bytecode caches.
    #[test]
    fn the_identity_covers_every_file_but_bytecode() {
        let dir = paths::target_dir().join(format!("testkit-identity-{}", std::process::id()));
        let package = dir.join("ocio_oracle");
        std::fs::create_dir_all(package.join("__pycache__")).unwrap();
        for (name, text) in [
            ("pyproject.toml", "p"),
            ("uv.lock", "l"),
            (".python-version", "3.13"),
        ] {
            std::fs::write(dir.join(name), text).unwrap();
        }
        std::fs::write(package.join("commands.py"), "c").unwrap();
        std::fs::write(package.join("table.json"), "[1]").unwrap();
        std::fs::write(package.join("__pycache__").join("commands.pyc"), "b").unwrap();

        let before = identity(&dir).unwrap();
        std::fs::write(package.join("__pycache__").join("commands.pyc"), "b2").unwrap();
        assert_eq!(
            identity(&dir).unwrap(),
            before,
            "bytecode changed the identity"
        );
        std::fs::write(package.join("table.json"), "[2]").unwrap();
        let data_changed = identity(&dir).unwrap();
        std::fs::write(package.join("table.json"), "[1]").unwrap();
        std::fs::write(package.join("commands.py"), "c2").unwrap();
        let code_changed = identity(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(
            data_changed, before,
            "a data file didn't change the identity"
        );
        assert_ne!(code_changed, before, "a module didn't change the identity");
    }

    /// The cache identity takes upstream's test files: the same oracle with and without a test
    /// file, or with other bytes in it, has another identity.
    #[test]
    fn the_identity_covers_upstream_test_data() {
        let dir = paths::target_dir().join(format!("testkit-identity-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ocio_oracle")).unwrap();
        for name in ["pyproject.toml", "uv.lock", ".python-version"] {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        let data = dir.join("data");
        let missing = identity_with(&dir, &data).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("a.clf"), "one").unwrap();
        let one = identity_with(&dir, &data).unwrap();
        std::fs::write(data.join("a.clf"), "two").unwrap();
        let two = identity_with(&dir, &data).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(missing, one, "a test file didn't change the identity");
        assert_ne!(one, two, "a test file's bytes didn't change the identity");
    }

    /// `isolate` removes `OCIO`, `OCIO_*` and `PYTHON*` in any case (Windows finds a variable
    /// whatever its case), keeps the others, and sets `PYTHONDONTWRITEBYTECODE`.
    #[test]
    fn isolate_removes_ocio_and_python_variables_in_any_case() {
        let names = [
            "OCIO",
            "OCIO_LOGGING_LEVEL",
            "ocio_lut_cache",
            "PYTHONVERBOSE",
            "pythonoptimize",
            "PythonPath",
            "OCIOX",
            "PATH",
            "MY_PYTHON",
        ];
        let mut command = Command::new("x");
        isolate_from(&mut command, names.iter().map(std::ffi::OsString::from));
        let envs: Vec<(String, Option<String>)> = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        let removed = |name: &str| envs.iter().any(|(k, v)| k == name && v.is_none());
        for name in &names[..6] {
            assert!(removed(name), "{name} was kept: {envs:?}");
        }
        for name in &names[6..] {
            assert!(!removed(name), "{name} was removed: {envs:?}");
        }
        assert!(
            envs.iter()
                .any(|(k, v)| k == "PYTHONDONTWRITEBYTECODE" && v.as_deref() == Some("1")),
            "{envs:?}"
        );
    }
}
