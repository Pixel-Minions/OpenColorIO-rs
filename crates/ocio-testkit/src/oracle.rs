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
//! sources and lock file, and this machine's OS and CPU. Set `OCIO_RS_ORACLE_NO_CACHE=1` to
//! bypass the cache. Environment variables named `OCIO` or `OCIO_*` are never passed to the
//! oracle, so its results don't depend on the caller's environment.
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
    /// responses, in order. Identical input blobs are sent once. With `cache`, the whole batch
    /// is one cache entry. Panics on a protocol error.
    #[track_caller]
    pub fn batch(&self, calls: &[BatchCall<'_>], cache: bool) -> Vec<Response> {
        // Deduplicate by address first (the usual case: one probe buffer, many calls), then
        // by content, so the request doesn't depend on where the buffers live.
        let mut by_address: HashMap<(usize, usize), usize> = HashMap::new();
        let mut by_content: HashMap<u128, Vec<usize>> = HashMap::new();
        let mut blobs: Vec<&[u8]> = Vec::new();
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
        let args = json!({ "calls": entries });
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
            .map(|entry| {
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
                Response {
                    result: entry["result"].clone(),
                    blobs,
                }
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
        let header = json!({
            "cmd": cmd,
            "args": args,
            "blobs": blobs.iter().map(|b| b.len()).collect::<Vec<_>>(),
        });
        let request = frame(&header, blobs);

        let cache_file = self.cache_dir.as_ref().filter(|_| cache).map(|dir| {
            let mut h = Xxh3::new();
            h.update(&self.identity.to_le_bytes());
            h.update(&request);
            dir.join(format!("{:032x}.bin", h.digest128()))
        });
        if let Some(file) = &cache_file
            && let Ok(bytes) = std::fs::read(file)
            && let Ok(response) = parse_response(&bytes)
        {
            return Ok(response);
        }

        let bytes = self.run(&request)?;
        let response = parse_response(&bytes)?;
        if let Some(file) = &cache_file {
            write_atomically(file, &bytes);
        }
        Ok(response)
    }

    fn run(&self, request: &[u8]) -> Result<Vec<u8>, String> {
        let mut command = Command::new(&self.python);
        command
            .args(["-X", "utf8", "-m", "ocio_oracle"])
            .current_dir(paths::oracle_dir())
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env_remove("PYTHONPATH")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, _) in std::env::vars_os() {
            let key_str = key.to_string_lossy();
            if key_str == "OCIO" || key_str.starts_with("OCIO_") {
                command.env_remove(&key);
            }
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", self.python.display()))?;

        // Write on a separate thread so a large response can't deadlock a large request.
        let mut stdin = child.stdin.take().expect("piped stdin");
        let request_len = request.len();
        let request = request.to_vec();
        let writer = std::thread::spawn(move || stdin.write_all(&request));
        let mut stdout = Vec::new();
        child
            .stdout
            .take()
            .expect("piped stdout")
            .read_to_end(&mut stdout)
            .map_err(|e| e.to_string())?;
        let mut stderr = String::new();
        child
            .stderr
            .take()
            .expect("piped stderr")
            .read_to_string(&mut stderr)
            .ok();
        let status = child.wait().map_err(|e| e.to_string())?;
        let written = writer.join();
        if !status.success() {
            return Err(format!("oracle exited with {status}\n{stderr}"));
        }
        // A request that didn't reach the oracle whole makes it fail with an EOFError; say why.
        match written {
            Ok(Ok(())) => Ok(stdout),
            Ok(Err(e)) => Err(format!(
                "writing the {}-byte request to the oracle failed: {e} ({:?})\n{stderr}",
                request_len,
                e.kind()
            )),
            Err(_) => Err("the thread writing the oracle request panicked".to_string()),
        }
    }
}

/// Whether `python` exists and starts.
fn python_runs(python: &Path) -> bool {
    python.is_file()
        && Command::new(python)
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
    h.update(machine_description().as_bytes());
    Ok(h.digest128())
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

/// OS, architecture, CPU model and the SIMD features OCIO's `CPUInfo` looks at.
pub fn machine_description() -> String {
    let mut s = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    if let Some(cpu) = cpu_model() {
        s.push_str(" cpu=");
        s.push_str(&cpu);
    }
    #[cfg(target_arch = "x86_64")]
    {
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
    use super::*;

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
