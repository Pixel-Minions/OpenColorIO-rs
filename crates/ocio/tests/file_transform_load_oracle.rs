// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Loading a file transform's file, against the wheel (`config_processor`): the errors of
//! files no format reads (no format for the extension, every format tried), of files that
//! can't be found or opened, and of an empty path, through `Config::getProcessor` on an empty
//! config, as type and bytes; with debug logging, the error and the log of the load
//! (`processor_debug_log`). The readers' own errors come with the readers (WP 4.2-4.9).

use std::sync::{Arc, Mutex};

use ocio::{Config, ExceptionKind, FileTransform, Transform};
use ocio_ops::logging::{
    LoggingFunction, get_logging_level, reset_to_default_logging_function, set_logging_function,
    set_logging_level,
};
use ocio_ops::open_color_types::LoggingLevel;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

/// The logging level and function are the process's: each test holds this while it runs.
static LOGGING: Mutex<()> = Mutex::new(());

/// Upstream's test files, as a path with `/` separators (not a Windows verbatim path).
fn test_files_dir() -> String {
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    let dir = dir.to_str().expect("a UTF-8 path").replace('\\', "/");
    dir.trim_start_matches("//?/").to_owned()
}

/// The port's error for the processor of a file transform of `src`, as (type, message).
fn port_error(src: &str) -> (String, Vec<u8>) {
    let mut file_transform = FileTransform::new();
    file_transform.set_src(src);
    let config = Config::new().unwrap();
    let error = config
        .processor(&Transform::File(file_transform))
        .map(|_| ())
        .expect_err(src);
    let kind = match error.kind() {
        ExceptionKind::MissingFile => "ExceptionMissingFile",
        _ => "Exception",
    };
    (kind.to_owned(), error.what().to_vec())
}

/// The wheel's errors for the processors of file transforms of `srcs`, as (type, message).
fn wheel_errors(srcs: &[String]) -> Vec<(String, Vec<u8>)> {
    let args: Vec<Value> = srcs
        .iter()
        .map(|src| {
            json!({
                "config": "new",
                "overload": {"transform": [{"class": "FileTransform",
                                            "calls": [["setSrc", src]]}]},
            })
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = args
        .iter()
        .map(|a| BatchCall {
            cmd: "config_processor",
            args: a.clone(),
            blobs: Vec::new(),
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .zip(srcs)
        .map(|(response, src)| {
            let response = response.unwrap_or_else(|e| panic!("{src}: {e}"));
            let exception = &response.result["processor"]["exception"];
            assert!(exception.is_object(), "{src}: {}", response.result);
            (
                exception["type"].as_str().unwrap().to_owned(),
                bytes(&exception["message"]),
            )
        })
        .collect()
}

fn check(srcs: &[String]) {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let wheel = wheel_errors(srcs);
    for (src, wheel) in srcs.iter().zip(wheel) {
        let port = port_error(src);
        assert_eq!(
            (port.0.as_str(), port.1.escape_ascii().to_string()),
            (wheel.0.as_str(), wheel.1.escape_ascii().to_string()),
            "{src}"
        );
    }
}

/// Files of an extension no format reads, which every format fails to read in the wheel too:
/// the message without a format's error.
#[test]
fn files_no_format_reads_fail_as_in_the_wheel() {
    let dir = test_files_dir();
    check(&[
        format!("{dir}/rgb-cmy.jpg"),
        format!("{dir}/error_unknown_format.txt"),
        format!("{dir}/./rgb-cmy.jpg"),
        format!("{dir}/clf/../rgb-cmy.jpg"),
    ]);
}

/// Missing files, absolute and relative, with and without a format for their extension, an
/// unresolved context variable, an empty path, and directories: a directory of an extension
/// no format reads fails every format, on Windows because it can't be opened, on Linux
/// because it can't be read.
#[test]
fn files_that_cant_be_found_or_opened_fail_as_in_the_wheel() {
    let dir = test_files_dir();
    let mut srcs = vec![
        format!("{dir}/missing.file"),
        format!("{dir}/missing.cube"),
        format!("{dir}/missing"),
        "missing.cube".to_owned(),
        "$MISSING_OCIO_VARIABLE/lut.cube".to_owned(),
        String::new(),
        format!("{dir}/clf"),
        format!("{dir}/clf/"),
        dir.clone(),
    ];
    // On Windows a directory doesn't open, so each format of its extension fails to open it;
    // on Linux the readers read it.
    if cfg!(windows) {
        srcs.push(format!("{dir}/clf/illegal"));
        srcs.push(format!("{dir}/clf/illegal/../../clf"));
    }
    check(&srcs);
}

/// The loader's part of a debug log: from its first line (`**`) on, each line up to the
/// error of the format that failed, which is the reader's (the readers' errors come with the
/// readers, WP 4.2-4.9).
fn loader_log(log: &[Vec<u8>]) -> Vec<String> {
    let start = log
        .iter()
        .position(|m| m.as_slice() == b"[OpenColorIO Debug]: **\n")
        .expect("the loader's first line");
    log[start..]
        .iter()
        .map(|m| {
            let text = m.escape_ascii().to_string();
            match text.find(":  ") {
                Some(i) if text.contains(" format ") => text[..i + 3].to_owned(),
                _ => text,
            }
        })
        .collect()
}

/// With debug logging, the load logs the file it opens and each format that fails, and its
/// error points to that log: the error, and the log as far as the loader writes it. Each file
/// is loaded once (the files are cached with their error), and none of these is loaded by the
/// other tests.
#[test]
fn debug_logging_logs_each_format_tried_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = test_files_dir();
    let srcs = [
        format!("{dir}/configs/ocioz_archive_configs/empty.ocioz"),
        format!("{dir}/configs/mergeconfigs/base_config.yaml"),
        format!("{dir}/configs"),
    ];
    let calls: Vec<BatchCall<'_>> = srcs
        .iter()
        .map(|src| BatchCall {
            cmd: "processor_debug_log",
            args: json!({"transform": {"class": "FileTransform", "calls": [["setSrc", src]]}}),
            blobs: Vec::new(),
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let messages = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let sink = messages.clone();
    let function: LoggingFunction = Arc::new(move |m: &[u8]| {
        sink.lock().unwrap().push(m.to_vec());
    });
    let level = get_logging_level();
    for (src, result) in srcs.iter().zip(results) {
        let wheel = result.unwrap_or_else(|e| panic!("{src}: {e}")).result;
        assert_eq!(wheel["stage"], "processor", "{src}: {wheel}");
        let wheel_log: Vec<Vec<u8>> = wheel["processor"]
            .as_array()
            .expect("the processor's log")
            .iter()
            .map(|m| m.as_str().expect("a message").as_bytes().to_vec())
            .collect();
        let wheel_error = wheel["exception"]["message"]
            .as_str()
            .expect("a message")
            .as_bytes()
            .to_vec();

        let mut file_transform = FileTransform::new();
        file_transform.set_src(src);
        let config = Config::create_raw().unwrap();
        messages.lock().unwrap().clear();
        set_logging_function(Some(function.clone())).unwrap();
        set_logging_level(LoggingLevel::Debug);
        let built = config.processor(&Transform::File(file_transform));
        set_logging_level(level);
        reset_to_default_logging_function();
        let port_error = built.map(|_| ()).expect_err(src);
        let port_log = messages.lock().unwrap().clone();

        assert_eq!(
            port_error.what().escape_ascii().to_string(),
            wheel_error.escape_ascii().to_string(),
            "{src}"
        );
        assert_eq!(loader_log(&port_log), loader_log(&wheel_log), "{src}");
    }
}
