// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the library writes to stderr, with and without `set_stderr_sink`, which the tools set.
//!
//! Without a sink the bytes go to stderr as they are: the expected text is upstream's
//! (`InitLogging` and the default logging function, src/OpenColorIO/Logging.cpp:30-68 @ v2.5.2),
//! with `\n` line ends on every platform, and `logging_oracle.rs` compares the same stderr with
//! the wheel's. With a sink, the same bytes reach the sink and stderr stays empty.
//!
//! `OCIO_LOGGING_LEVEL` is read once per process and the sink is global, so each case runs in a
//! new process of this test, given by `CASE`.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

use ocio_ops::logging::{get_logging_level, log_warning, set_stderr_sink};
use ocio_ops::platform::{MapEnv, set_env_provider};

const CASE: &str = "OCIO_RS_LOGGING_STDERR_SINK_CASE";
const SINK: &str = "sink received: ";

const DEBUG_LINE: &str = "[OpenColorIO Debug]: Using OpenColorIO version: 2.5.2\n";
const INVALID_LINE: &str = "[OpenColorIO Warning]: Invalid $OCIO_LOGGING_LEVEL specified. \
                            Options: none (0), warning (1), info (2), debug (3)\n";
const MESSAGE_LINE: &str = "[OpenColorIO Warning]: a message\n";

/// The child: a level, with a sink or without, and one message by the default function.
fn run_case(level: &str, with_sink: bool) {
    let vars = BTreeMap::from([("OCIO_LOGGING_LEVEL".to_string(), level.to_string())]);
    set_env_provider(Some(Arc::new(MapEnv::from(vars))));
    let received = Arc::new(Mutex::new(Vec::<u8>::new()));
    if with_sink {
        let sink = Arc::clone(&received);
        set_stderr_sink(Arc::new(move |text: &[u8]| {
            sink.lock().unwrap().extend_from_slice(text);
        }));
    }
    get_logging_level();
    log_warning("a message");
    if with_sink {
        let received = received.lock().unwrap();
        println!(
            "{SINK}{}",
            String::from_utf8_lossy(&received).escape_debug()
        );
    }
}

/// Runs this test's child for `case`: its stdout and stderr.
fn run_child(case: &str) -> (String, Vec<u8>) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child", "--nocapture"])
        .env(CASE, case)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    (String::from_utf8(output.stdout).unwrap(), output.stderr)
}

/// Not a test of its own: what [`run_child`] runs.
#[test]
fn child() {
    if let Ok(case) = std::env::var(CASE) {
        let (level, sink) = case.split_once(' ').unwrap();
        run_case(level, sink == "sink");
    }
}

#[test]
fn without_a_sink_the_bytes_go_to_stderr_as_they_are() {
    for (level, first) in [
        ("debug", DEBUG_LINE),
        ("bogus", INVALID_LINE),
        ("warning", ""),
    ] {
        let (stdout, stderr) = run_child(&format!("{level} none"));
        assert!(!stdout.contains(SINK), "{stdout}");
        assert_eq!(
            String::from_utf8(stderr).unwrap(),
            format!("{first}{MESSAGE_LINE}"),
            "{level}"
        );
    }
}

#[test]
fn a_sink_receives_those_bytes_and_stderr_nothing() {
    for (level, first) in [
        ("debug", DEBUG_LINE),
        ("bogus", INVALID_LINE),
        ("warning", ""),
    ] {
        let (stdout, stderr) = run_child(&format!("{level} sink"));
        assert_eq!(stderr, b"", "{level}");
        let expected = format!("{first}{MESSAGE_LINE}");
        let expected = format!("{SINK}{}", expected.escape_debug());
        assert!(stdout.contains(&expected), "{level}: {stdout}");
    }
}
