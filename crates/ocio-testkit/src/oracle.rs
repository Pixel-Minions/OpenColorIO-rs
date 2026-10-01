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
//! emulated CPU, so the wheel and the port dispatch to the SIMD kernels of that CPU.
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

        let bytes = self.run(&request)?;
        let response = parse_response(&bytes)?;
        if let Some(file) = &cache_file
            && cacheable(cmd, &response)
        {
            write_atomically(file, &bytes);
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

    fn run(&self, request: &[u8]) -> Result<Vec<u8>, String> {
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
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name == "OCIO" || name.starts_with("OCIO_") || name.starts_with("PYTHON") {
            command.env_remove(&key);
        }
    }
    command.env("PYTHONDONTWRITEBYTECODE", "1")
}

/// Starts `command`, writes `request` to its stdin, and returns its stdout once it exits
/// successfully. Each pipe has its own thread or loop, so the process never waits on a full
/// pipe: the request is written on a thread (a large response can't deadlock a large
/// request), stderr is read on another (a process that writes more than a pipe holds to
/// stderr before it closes stdout can't hang), and stdout is read here.
fn exchange(mut command: Command, request: &[u8]) -> Result<Vec<u8>, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start {program}: {e}"))?;

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
    let status = child.wait().map_err(|e| e.to_string())?;
    let stdout = stdout.map_err(|e| format!("reading the oracle's response failed: {e}"))?;
    let written = writer
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::other("the writer thread panicked")));
    // The exit status says how the oracle died when it stopped reading early: under Intel
    // SDE on Windows, it has exited mid-request without writing anything to stderr.
    if let Err(e) = written {
        return Err(format!(
            "writing the {request_len}-byte request to the oracle failed: {e}; the oracle \
             exited with {status}\n{stderr}"
        ));
    }
    if !status.success() {
        return Err(format!("oracle exited with {status}\n{stderr}"));
    }
    Ok(stdout)
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
    hash_test_data(&mut h, &paths::upstream_dir().join("tests").join("data"));
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

fn parse_response(bytes: &[u8]) -> Result<Response, String> {
    let len_bytes: [u8; 4] = bytes
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .ok_or("oracle response is shorter than its frame header")?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    let header_bytes = bytes
        .get(4..4 + len)
        .ok_or("oracle response header is truncated")?;
    let header: Value = serde_json::from_slice(header_bytes).map_err(|e| e.to_string())?;
    if header["ok"] != Value::Bool(true) {
        return Err(header["error"]
            .as_str()
            .unwrap_or("unknown oracle error")
            .to_string());
    }
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
    Ok(Response {
        result: header["result"].clone(),
        blobs,
    })
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
    /// gives its stdout back: stderr is drained while stdout is read.
    #[test]
    fn a_large_stderr_does_not_block_the_exchange() {
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
        let stdout = super::exchange(command, &request).expect("the exchange");
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
}
