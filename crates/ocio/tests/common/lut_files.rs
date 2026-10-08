// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! LUT files for the readers' oracle tests: each set of files is written once, under
//! `<target>/lut-files/<hash of the set>/`, where the wheel and the port read the same paths
//! (the oracle runs on this machine). The directory's name covers the files' names and bytes,
//! so the oracle's cached replies, which key on the paths, always describe these bytes.
//!
//! Each request runs through `with_files`, which clears OCIO's caches first: the wheel's file
//! cache hands a file's LUT to the ops of a processor, whose finalization changes it, so what a
//! later processor reads from the cache depends on the processors made before it (reported to
//! the owner, 2026-10-08). From an empty cache the wheel's result is a function of the request.

use std::path::PathBuf;

use ocio::{FileTransform, Interpolation, TransformDirection};
use serde_json::{Value, json};

use super::transforms::{Case, direction_name, interpolation_name};

/// One entry of a set of files.
#[derive(Debug, Clone)]
pub(crate) enum Entry {
    /// A file and its bytes.
    File(String, Vec<u8>),
    /// A directory.
    Dir(String),
}

/// A file of `name` holding `bytes`.
pub(crate) fn file(name: &str, bytes: impl Into<Vec<u8>>) -> Entry {
    Entry::File(name.to_owned(), bytes.into())
}

/// The directory of the set `entries`, written if needed, as a path with `/` separators (not
/// a Windows verbatim path).
pub(crate) fn write_files(entries: &[Entry]) -> String {
    let mut key = Vec::new();
    for entry in entries {
        match entry {
            Entry::File(name, bytes) => {
                key.extend_from_slice(b"F");
                key.extend_from_slice(&(name.len() as u64).to_le_bytes());
                key.extend_from_slice(name.as_bytes());
                key.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                key.extend_from_slice(bytes);
            }
            Entry::Dir(name) => {
                key.extend_from_slice(b"D");
                key.extend_from_slice(&(name.len() as u64).to_le_bytes());
                key.extend_from_slice(name.as_bytes());
            }
        }
    }
    let hash = xxhash_rust::xxh3::xxh3_128(&key);
    let dir: PathBuf = ocio_testkit::paths::target_dir()
        .join("lut-files")
        .join(format!("{hash:032x}"));
    for entry in entries {
        match entry {
            Entry::File(name, bytes) => {
                let path = dir.join(name);
                std::fs::create_dir_all(path.parent().expect("a parent")).unwrap();
                if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                    // Written aside then renamed, as parallel tests may write the same set.
                    let temp = dir.join(format!(
                        "{name}.{}.{:?}.tmp",
                        std::process::id(),
                        std::thread::current().id()
                    ));
                    std::fs::write(&temp, bytes).unwrap();
                    std::fs::rename(&temp, &path).unwrap();
                }
            }
            Entry::Dir(name) => std::fs::create_dir_all(dir.join(name)).unwrap(),
        }
    }
    let dir = dir.to_str().expect("a UTF-8 path").replace('\\', "/");
    dir.trim_start_matches("//?/").to_owned()
}

/// A case of the file transform of `src`, with the interpolation `interp` and the direction
/// `dir`, as the oracle builds it and as the port does.
pub(crate) fn file_case(
    label: impl Into<String>,
    src: &str,
    interp: Interpolation,
    dir: TransformDirection,
) -> Case {
    let spec: Value = json!({
        "class": "FileTransform",
        "calls": [
            ["setSrc", src],
            ["setInterpolation", {"enum": interpolation_name(interp)}],
            ["setDirection", {"enum": direction_name(dir)}],
        ],
    });
    let mut port = FileTransform::new();
    port.set_src(src);
    port.set_interpolation(interp);
    port.set_direction(dir);
    Case::new(label, spec, port)
}

/// The logging level and function are the process's: a test that captures the log holds this
/// while it runs.
pub(crate) static LOGGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What the wheel and the port log while the raw config builds the processor of each case in
/// the direction `dir`, and its optimized processor (`processor_ops`, at the default logging
/// level), message by message.
pub(crate) fn check_logs(cases: &[Case], dir: TransformDirection) {
    use std::sync::{Arc, Mutex};

    use ocio::{BitDepth, Config, OptimizationFlags};
    use ocio_ops::logging::{
        LoggingFunction, reset_to_default_logging_function, set_logging_function,
    };
    use ocio_testkit::processor_ops::ProcessorOpsReply;

    use super::transforms::port_processors;

    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let requests: Vec<(usize, TransformDirection)> = (0..cases.len()).map(|k| (k, dir)).collect();
    let replies = wheel_replies(cases, &requests);

    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = messages.clone();
    let function: LoggingFunction = Arc::new(move |m: &[u8]| {
        sink.lock()
            .unwrap()
            .push(String::from_utf8(m.to_vec()).expect("a UTF-8 message"));
    });
    let mut failures = Vec::new();
    for (case, reply) in cases.iter().zip(replies) {
        let reply: ProcessorOpsReply = reply;
        let wheel: Vec<String> = reply.result["log"]
            .as_array()
            .expect("the log")
            .iter()
            .map(|m| m.as_str().expect("a message").to_owned())
            .collect();
        let config = Config::create_raw().unwrap();
        messages.lock().unwrap().clear();
        set_logging_function(Some(function.clone())).unwrap();
        let _ = port_processors(
            &config,
            &case.port,
            dir,
            (BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE),
        );
        reset_to_default_logging_function();
        let port = messages.lock().unwrap().clone();
        if port != wheel {
            failures.push(format!("{}: wheel {wheel:?}\n  port  {port:?}", case.label));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The wheel's `processor_ops` replies for the processors of `cases[k]` in the direction `dir`,
/// for each `(k, dir)` of `requests` (the raw config, no optimization), each request run from
/// empty caches (`with_files`).
fn wheel_replies(
    cases: &[Case],
    requests: &[(usize, TransformDirection)],
) -> Vec<ocio_testkit::processor_ops::ProcessorOpsReply> {
    use ocio_testkit::Oracle;
    use ocio_testkit::oracle::BatchCall;
    use ocio_testkit::processor_ops::{ProcessorOpsReply, ProcessorOpsRequest};

    let calls: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|&(k, dir)| {
            let mut request = ProcessorOpsRequest::new(
                json!({"transform": cases[k].spec, "direction": direction_name(dir)}),
            );
            request.optimization = Some(json!("OPTIMIZATION_NONE"));
            BatchCall {
                cmd: "with_files",
                args: json!({"files": {}, "command": "processor_ops", "args": request.args()}),
                blobs: Vec::new(),
            }
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| ProcessorOpsReply::from_response(r.unwrap_or_else(|e| panic!("{e}"))))
        .collect()
}

/// The raw config's processors of `cases` in both directions, the wheel's (from empty caches)
/// against the port's: as `transforms::check_processors` compares them.
pub(crate) fn check_file_processors(cases: &[Case]) {
    use ocio::{BitDepth, Config, OptimizationFlags};

    use super::transforms::{compare_reply, port_processors};

    let requests: Vec<(usize, TransformDirection)> = (0..cases.len())
        .flat_map(|k| {
            [
                (k, TransformDirection::Forward),
                (k, TransformDirection::Inverse),
            ]
        })
        .collect();
    let replies = wheel_replies(cases, &requests);
    let config = Config::create_raw().unwrap();
    let mut failures = Vec::new();
    for (&(k, dir), reply) in requests.iter().zip(replies) {
        let case = &cases[k];
        let port = port_processors(
            &config,
            &case.port,
            dir,
            (BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE),
        );
        if let Some(failure) = compare_reply(&reply, &port) {
            failures.push(format!("{} ({dir:?}): {failure}", case.label));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
