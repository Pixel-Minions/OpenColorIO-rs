// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! An application's callback exceptions through the library (`ocio_ops::foreign_exception`,
//! owner decision 2026-10-22, Q2 = U): they reach the caller as they were, except at the sites
//! that port a C++ `catch (std::exception &)` (which read their `what()`) or `catch (...)`
//! (which swallow them); the library's locks recover after one.
//!
//! The texts the sites build around `what()` are their own (OCIOYaml.cpp:5427-5438 @ v2.5.2,
//! "Error: Loading the OCIO profile failed. "); the Python module's tests compare the whole
//! behaviour with the wheel's.

use std::sync::Arc;

use ocio_ops::exception::Result;
use ocio_ops::foreign_exception::{ForeignException, raise, try_foreign};
use ocio_ops::logging::{log_message, reset_to_default_logging_function, set_logging_function};
use ocio_ops::open_color_types::LoggingLevel;

use crate::config::Config;
use crate::config_io_proxy::ConfigIoProxy;
use crate::fileformats::input_stream::OpenMode;
use crate::path_utils::{
    get_fast_file_hash, hash_function_change, reset_compute_hash_function,
    set_compute_hash_function,
};

fn boom() -> ForeignException {
    ForeignException::new("Callback: boom", Box::new("payload"))
}

/// Runs `f` with a logging function that raises [`boom`] for every message.
fn with_raising_logger<T>(f: impl FnOnce() -> T) -> T {
    // The logging function is global: hold the lock the library's log tests hold.
    let _lock = crate::test_env::logging_lock();
    set_logging_function(Some(Arc::new(|_: &[u8]| raise(boom())))).unwrap();
    let out = try_foreign(f);
    reset_to_default_logging_function();
    out.unwrap_or_else(|e| panic!("escaped: {e:?}"))
}

#[test]
fn a_logging_exception_reaches_the_caller_of_log_message() {
    let caught = with_raising_logger(|| try_foreign(|| log_message(LoggingLevel::Warning, b"x")));
    let e = caught.unwrap_err();
    assert_eq!(e.what(), b"Callback: boom");
    assert_eq!(*e.into_payload().downcast::<&str>().unwrap(), "payload");
}

#[test]
fn reading_a_config_reads_a_logging_exception_by_its_what() {
    // An unknown key: the reader logs a warning.
    let text = b"ocio_profile_version: 2\nnope: 1\nroles: {default: raw}\ncolorspaces:\n  - !<ColorSpace> {name: raw}\n";
    let result = with_raising_logger(|| Config::create_from_stream(text).map(|_| ()));
    assert_eq!(
        result.unwrap_err().what(),
        b"Error: Loading the OCIO profile failed. Callback: boom"
    );
}

#[test]
fn the_cache_id_swallows_a_hash_exception_and_the_hash_lock_recovers() {
    let _change = hash_function_change();
    let path = b"/foreign_exception_tests/no_such.clf";
    let text = format!(
        "ocio_profile_version: 2\nroles: {{default: raw}}\ncolorspaces:\n  - !<ColorSpace> \
         {{name: raw}}\n  - !<ColorSpace> {{name: f, to_reference: !<FileTransform> {{src: \
         {}}}}}\n",
        String::from_utf8_lossy(path)
    );
    let config = Config::create_from_stream(text.as_bytes()).unwrap();
    set_compute_hash_function(Arc::new(|_: &[u8]| raise(boom())));
    let context = config.current_context().get();
    // `catch (...)`: no unwind leaves getCacheID.
    let id = try_foreign(|| config.cache_id_with_context(Some(&context)));
    // The hash was marked ready before the function threw (GetFastFileHash): the path's
    // lock, poisoned by the unwind, is taken again and gives the empty hash, without calling
    // the function.
    let again = try_foreign(|| get_fast_file_hash(path, &context));
    reset_compute_hash_function();
    assert!(id.unwrap().is_ok());
    assert_eq!(again.unwrap().unwrap(), b"");
}

/// A proxy whose LUT data throws an application's exception.
struct RaisingProxy;

impl ConfigIoProxy for RaisingProxy {
    fn lut_data(&self, _filepath: &[u8]) -> Result<Vec<u8>> {
        raise(boom())
    }

    fn config_data(&self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    fn fast_lut_file_hash(&self, _filepath: &[u8]) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

#[test]
fn the_proxy_lut_exception_is_thrown_again_for_a_relative_path_only() {
    let mut config =
        Arc::try_unwrap(Config::create_raw().unwrap()).unwrap_or_else(|c| (*c).clone());
    config.set_config_io_proxy(Some(Arc::new(RaisingProxy)));
    let relative = try_foreign(|| {
        crate::transforms::file_transform::get_lut_data(&config, b"lut.clf", OpenMode::Text)
            .map(|_| ())
    });
    assert_eq!(relative.unwrap_err().what(), b"Callback: boom");
    // An absolute path falls back to the file system, whose file can't be opened.
    let absolute = try_foreign(|| {
        crate::transforms::file_transform::get_lut_data(
            &config,
            b"/foreign_exception_tests/none.clf",
            OpenMode::Text,
        )
        .map(|s| s.good())
    });
    assert!(!absolute.unwrap().unwrap());
}
