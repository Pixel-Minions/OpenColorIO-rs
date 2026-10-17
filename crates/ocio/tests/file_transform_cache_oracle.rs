// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The caches of file transforms against the wheel:
//! - a file transform whose path holds a context variable, processed with two contexts that
//!   give the variable two values, in one config (the processor cache keys on the used
//!   context's cache ID), each processor the wheel's (`config_processor`);
//! - the file cache, in one process: a file read once is served from the cache after it
//!   changes, a file that failed to read fails with the same error after it changes, both
//!   until `ClearAllCaches` (`config_calls`, the wheel writing the files with
//!   `Config::serialize`, the port writing the same bytes).
//!
//! The tests of this file share the process's caches, so each holds a lock while it runs.

mod common;

use std::sync::Mutex;

use common::lut_files::{file, file_case, write_files};
use common::transforms::{ProcessorAndGroup, compare_reply};
use ocio::{
    BitDepth, Config, Context, FileTransform, Interpolation, OptimizationFlags, Transform,
    TransformDirection,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::Response;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::processor_ops::ProcessorOpsReply;
use serde_json::{Value, json};

static CACHES: Mutex<()> = Mutex::new(());

/// A spi1d LUT of 4 entries, `last` its last.
fn spi1d(last: &str) -> String {
    format!("Version 1\nFrom 0 1\nLength 4\nComponents 1\n{{\n0\n0.25\n0.5\n{last}\n}}\n")
}

/// The port's processor of `transform` in `context`, and its optimized processor (F32, no
/// optimization), each with its group; or the error and its stage, as `processor_ops` names
/// them.
fn port_processors_in(
    config: &Config,
    context: &Context,
    transform: &Transform,
) -> Result<(ProcessorAndGroup, ProcessorAndGroup), (&'static str, ocio::Exception)> {
    let processor = config
        .processor_with_context(context, transform, TransformDirection::Forward)
        .map_err(|e| ("processor", e))?;
    let group = processor
        .create_group_transform()
        .map_err(|e| ("group", e))?;
    let optimized = processor
        .optimized_processor_with_bit_depths(BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE)
        .map_err(|e| ("optimize", e))?;
    let optimized_group = optimized
        .create_group_transform()
        .map_err(|e| ("optimized_group", e))?;
    Ok(((processor, group), (optimized, optimized_group)))
}

/// `$VAR/lut.spi1d` with `VAR` naming two directories, in one config: two processors, each
/// the wheel's for its context.
#[test]
fn a_variable_in_the_path_gives_a_processor_per_context() {
    let _caches = CACHES.lock().unwrap_or_else(|e| e.into_inner());
    let dirs = [
        write_files(&[file("lut.spi1d", spi1d("1"))]),
        write_files(&[file("lut.spi1d", spi1d("0.75"))]),
    ];
    let case = file_case(
        "$VAR",
        "$VAR/lut.spi1d",
        Interpolation::Default,
        TransformDirection::Forward,
    );
    let config = Config::create_raw().unwrap();
    for dir in &dirs {
        let response = Oracle::get().call(
            "with_files",
            json!({"files": {}, "command": "config_processor", "args": {
                "config": "raw",
                "context": {"base": "current",
                            "calls": [{"call": "setStringVar", "args": ["VAR", dir]}]},
                "overload": {"transform_direction": [case.spec, "TRANSFORM_DIR_FORWARD"]},
                "ops": {"optimization": "OPTIMIZATION_NONE"},
            }}),
            &[],
        );
        let ops = Response {
            result: response.result["ops"].clone(),
            blobs: response.blobs.clone(),
        };
        assert!(ops.result.is_object(), "{}", response.result);
        let reply = ProcessorOpsReply::from_response(ops);
        let mut context = (*config.current_context().get()).clone();
        context.set_string_var("VAR", Some(dir.as_bytes()));
        let port = port_processors_in(&config, &context, &case.port);
        if let Some(failure) = compare_reply(&reply, &port) {
            panic!("VAR={dir}: {failure}");
        }
    }
}

/// The bytes a C++ `std::ofstream` writes for `text`: `\n` as `\r\n` on Windows (text mode).
fn ofstream_bytes(text: &[u8]) -> Vec<u8> {
    if cfg!(windows) {
        let mut out = Vec::new();
        for &b in text {
            if b == b'\n' {
                out.push(b'\r');
            }
            out.push(b);
        }
        out
    } else {
        text.to_vec()
    }
}

/// A file read once is served from the file cache after it changes, and a file that failed
/// to read fails with its first error after it changes, until `ClearAllCaches`: the wheel in
/// one process (`config_calls`), the port in this one, with the same files.
#[test]
fn the_file_cache_keeps_reads_and_failures_until_cleared() {
    let _caches = CACHES.lock().unwrap_or_else(|e| e.into_inner());
    let numbers = "1 0 0 0 0 1 0 0 0 0 1 0\n";
    let dir = write_files(&[
        file("m.spimtx", numbers),
        file(
            "label.txt",
            "the_file_cache_keeps_reads_and_failures_until_cleared",
        ),
    ]);
    let path = format!("{dir}/m.spimtx");
    // The test changes the file: it starts from its first contents.
    std::fs::write(&path, numbers).unwrap();
    let yaml = "ocio_profile_version: 2\nroles: {default: raw}\n\
                colorspaces:\n  - !<ColorSpace> {name: raw}\n";
    let get_processor = || json!({"call": "getProcessor", "args": [{"ref": "ft"}], "as": "p"});
    let calls = json!([
        {"call": "ClearAllCaches", "on": "OCIO"},
        {"new": "FileTransform", "as": "ft"},
        {"call": "setSrc", "on": "ft", "args": [path]},
        get_processor(),
        {"call": "getCacheID", "on": "p"},
        // 5: the file changes: the config's text.
        {"call": "serialize", "args": [path]},
        {"call": "serialize"},
        {"call": "clearProcessorCache"},
        get_processor(),
        {"call": "getCacheID", "on": "p"},
        // 10: from empty caches, the changed file fails.
        {"call": "ClearAllCaches", "on": "OCIO"},
        {"call": "clearProcessorCache"},
        get_processor(),
        // 13: the file changes again; its first failure stays.
        {"call": "setDescription", "args": ["one two three four"]},
        {"call": "serialize", "args": [path]},
        {"call": "serialize"},
        {"call": "clearProcessorCache"},
        get_processor(),
        // 18: from empty caches, the new failure.
        {"call": "ClearAllCaches", "on": "OCIO"},
        {"call": "clearProcessorCache"},
        get_processor(),
    ]);
    let result = Oracle::get()
        .call(
            "config_calls",
            json!({"config": {"yaml": yaml}, "calls": calls}),
            &[],
        )
        .result;
    let call = |i: usize| -> &Value { &result["calls"][i] };
    let text = |i: usize| -> Vec<u8> { bytes(&call(i)["result"]) };
    let error = |i: usize| -> Vec<u8> { bytes(&call(i)["exception"]["message"]) };

    // The port, the same steps.
    std::fs::write(&path, numbers).unwrap();
    ocio::clear_all_caches();
    let config = Config::create_raw().unwrap();
    let mut file_transform = FileTransform::new();
    file_transform.set_src(&path);
    let transform = Transform::File(file_transform);
    let processor = || config.processor(&transform);
    let cache_id = |p: ocio::Result<std::sync::Arc<ocio::Processor>>| -> Vec<u8> {
        p.unwrap().cache_id().unwrap().into_bytes()
    };
    let message = |p: ocio::Result<std::sync::Arc<ocio::Processor>>| -> Vec<u8> {
        p.map(|_| ()).unwrap_err().what().to_vec()
    };

    assert_eq!(cache_id(processor()), text(4), "{result}");
    std::fs::write(&path, ofstream_bytes(&text(6))).unwrap();
    config.clear_processor_cache();
    assert_eq!(cache_id(processor()), text(9), "{result}");
    assert_eq!(text(9), text(4), "{result}");

    ocio::clear_all_caches();
    config.clear_processor_cache();
    assert_eq!(message(processor()), error(12), "{result}");
    std::fs::write(&path, ofstream_bytes(&text(15))).unwrap();
    config.clear_processor_cache();
    assert_eq!(message(processor()), error(17), "{result}");
    assert_eq!(error(17), error(12), "{result}");

    ocio::clear_all_caches();
    config.clear_processor_cache();
    assert_eq!(message(processor()), error(20), "{result}");
    assert_ne!(error(20), error(12), "{result}");
}
