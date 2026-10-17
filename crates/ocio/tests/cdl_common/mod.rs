// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the CDL readers' oracle tests share: the comparison of the warnings the wheel and the
//! port log while they read a file.

use std::sync::{Arc, Mutex};

use ocio::{BitDepth, Config, OptimizationFlags, Transform, TransformDirection};
use ocio_ops::logging::{LoggingFunction, reset_to_default_logging_function, set_logging_function};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::processor_ops::{ProcessorOpsReply, ProcessorOpsRequest};
use serde_json::json;

use crate::common::lut_files::{file, write_files};
use crate::common::transforms::{Case, port_processors};

/// The logging function is the process's: the tests that capture the log hold this.
static LOGGING: Mutex<()> = Mutex::new(());

/// What the wheel and the port log (at the default logging level) while the raw config
/// builds the forward processor of each case, message by message. Both read the files anew:
/// the wheel from empty caches (`with_files`), the port after `clear_all_caches`. The port's
/// messages are those logged on this thread; others go to stderr, as by default.
pub(crate) fn check_logs(cases: &[Case]) {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let cases: Vec<Case> = cases.iter().map(relocated).collect();
    let cases = cases.as_slice();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|c| {
            let mut request = ProcessorOpsRequest::new(
                json!({"transform": c.spec, "direction": "TRANSFORM_DIR_FORWARD"}),
            );
            request.optimization = Some(json!("OPTIMIZATION_NONE"));
            BatchCall {
                cmd: "with_files",
                args: json!({"files": {}, "command": "processor_ops", "args": request.args()}),
                blobs: Vec::new(),
            }
        })
        .collect();
    let replies = Oracle::get().batch(&calls, true);

    // Only this thread's messages: other tests of the binary may log meanwhile.
    let me = std::thread::current().id();
    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = messages.clone();
    let function: LoggingFunction = Arc::new(move |m: &[u8]| {
        if std::thread::current().id() == me {
            sink.lock()
                .unwrap()
                .push(String::from_utf8_lossy(m).into_owned());
        } else {
            eprint!("{}", String::from_utf8_lossy(m));
        }
    });
    let mut failures = Vec::new();
    for (case, reply) in cases.iter().zip(replies) {
        let reply = ProcessorOpsReply::from_response(reply.unwrap_or_else(|e| panic!("{e}")));
        let wheel: Vec<String> = reply.result["log"]
            .as_array()
            .expect("the log")
            .iter()
            .map(|m| m.as_str().expect("a message").to_owned())
            .collect();
        let config = Config::create_raw().unwrap();
        ocio::clear_all_caches();
        messages.lock().unwrap().clear();
        set_logging_function(Some(function.clone())).unwrap();
        let _ = port_processors(
            &config,
            &case.port,
            TransformDirection::Forward,
            (BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE),
        );
        reset_to_default_logging_function();
        let port = messages.lock().unwrap().clone();
        if port != wheel {
            failures.push(format!(
                "{}:\n  wheel {wheel:?}\n  port  {port:?}",
                case.label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `case` reading a copy of its file in a directory of its own: no other test of the binary
/// loads it, so the port's read can't come from a load another test cached after
/// `clear_all_caches`.
fn relocated(case: &Case) -> Case {
    let Transform::File(transform) = &case.port else {
        panic!("{}: a file transform", case.label);
    };
    let src = String::from_utf8(transform.src().to_vec()).expect("a UTF-8 path");
    let name = src.rsplit('/').next().expect("a file name").to_owned();
    let bytes = std::fs::read(&src).unwrap_or_else(|e| panic!("{src}: {e}"));
    let dir = write_files(&[file(&name, bytes), file(".logs", b"".as_slice())]);
    let new_src = format!("{dir}/{name}");

    let mut port = transform.clone();
    port.set_src(&new_src);
    let mut spec = case.spec.clone();
    for call in spec["calls"].as_array_mut().expect("the calls") {
        if call[0] == "setSrc" {
            call[1] = json!(new_src);
        }
    }
    Case::new(case.label.clone(), spec, port)
}
