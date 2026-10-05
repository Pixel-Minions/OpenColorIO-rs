// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's logging against the wheel's.
//!
//! - **Warnings from a config.** The wheel logs a warning for each unknown key of a transform
//!   in a config: "Unknown key in LogTransform: '<key>'." (`LogUnknownKeyWarning`,
//!   src/OpenColorIO/OCIOYaml.cpp:248-257, called at 2918-2921 @ v2.5.2). A YAML key can hold
//!   any character, so these keys make messages with line breaks, carriage returns, empty
//!   lines, spaces around the breaks, a non-ASCII character and a NUL. The oracle loads such a
//!   config (`config_serialize`) and returns what its logging function received
//!   (`captured_log`); the port logs the same messages.
//! - **`LogMessage`** at every level, for every level setting, with messages whose white space,
//!   line breaks, control characters and NULs exercise the trim, the line cutting and the C
//!   strings (`log_message`).
//! - **`LoggingLevelFromString` and `LoggingLevelToString`** (`logging_level_strings`).
//! - **`OCIO_LOGGING_LEVEL`**, read once per process: the oracle runs each case in a new Python
//!   process with the variable set (`logging_environment`), and this test runs each in a new
//!   process of itself, which gives the port the value through `platform::set_env_provider`.
//!   The levels, the lines the logging function receives, the messages logged after
//!   `ResetToDefaultLoggingFunction`, the refusal of `SetLoggingFunction(None)` and the raw
//!   stderr bytes must be the same.
//!
//! The port never reads this process's own `OCIO_LOGGING_LEVEL`: the tests run with an empty
//! environment provider, installed before the first logging call.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};

use ocio_ops::logging::{
    get_logging_level, log_message, log_warning, reset_to_default_logging_function,
    set_logging_function, set_logging_level,
};
use ocio_ops::open_color_types::{
    LoggingLevel, logging_level_from_string, logging_level_to_string,
};
use ocio_ops::platform::{MapEnv, set_env_provider};
use ocio_testkit::{Oracle, assert_bytes_eq};
use serde_json::{Value, json};

static LOGGING: Mutex<()> = Mutex::new(());

/// The logging state, for one test at a time. The first call installs an empty environment
/// provider for the rest of the process, before anything reads `OCIO_LOGGING_LEVEL`.
fn logging_state() -> MutexGuard<'static, ()> {
    static EMPTY_ENVIRONMENT: Once = Once::new();
    EMPTY_ENVIRONMENT.call_once(|| set_env_provider(Some(Arc::new(MapEnv::default()))));
    LOGGING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The lines the logging function receives while `run` runs.
fn capture<T>(run: impl FnOnce() -> T) -> (Vec<Vec<u8>>, T) {
    let lines = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let sink = Arc::clone(&lines);
    set_logging_function(Some(Arc::new(move |line: &[u8]| {
        sink.lock().unwrap().push(line.to_vec());
    })))
    .unwrap();
    let result = run();
    reset_to_default_logging_function();
    let lines = lines.lock().unwrap().clone();
    (lines, result)
}

/// The wheel's lines, as bytes.
fn lines(value: &Value) -> Vec<Vec<u8>> {
    value
        .as_array()
        .expect("lines")
        .iter()
        .map(|line| line.as_str().expect("a line").as_bytes().to_vec())
        .collect()
}

const LEVEL_NAMES: [(&str, LoggingLevel); 5] = [
    ("LOGGING_LEVEL_NONE", LoggingLevel::None),
    ("LOGGING_LEVEL_WARNING", LoggingLevel::Warning),
    ("LOGGING_LEVEL_INFO", LoggingLevel::Info),
    ("LOGGING_LEVEL_DEBUG", LoggingLevel::Debug),
    ("LOGGING_LEVEL_UNKNOWN", LoggingLevel::Unknown),
];

fn level(name: &str) -> LoggingLevel {
    LEVEL_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no level {name}"))
        .1
}

fn level_name(level: LoggingLevel) -> &'static str {
    LEVEL_NAMES.iter().find(|(_, l)| *l == level).unwrap().0
}

/// The unknown keys, in the order the transform lists them. Their order and bytes are the
/// test's input.
const KEYS: &[&str] = &[
    "a\nb",
    "c\r\nd",
    "e\n\nf",
    "g \n h",
    "\n",
    "tab\there",
    "\u{e9}t\u{e9}",
    "x\0y",
];

/// `key` as a YAML double-quoted scalar.
fn yaml_quoted(key: &str) -> String {
    let mut out = String::from("\"");
    for c in key.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A config whose one transform has every key of [`KEYS`].
fn config_yaml() -> String {
    let keys: Vec<String> = KEYS
        .iter()
        .map(|key| format!("{}: 1", yaml_quoted(key)))
        .collect();
    format!(
        "ocio_profile_version: 2\n\
         \n\
         roles:\n  default: raw\n\
         \n\
         colorspaces:\n\
         \x20 - !<ColorSpace>\n\
         \x20   name: raw\n\
         \x20   from_scene_reference: !<LogTransform> {{base: 2, {}}}\n",
        keys.join(", ")
    )
}

#[test]
fn unknown_key_warnings_match_the_wheel() {
    let _state = logging_state();
    let response = Oracle::get().call(
        "config_serialize",
        json!({ "config": { "yaml": config_yaml() } }),
        &[],
    );
    assert!(
        response.result.get("exception").is_none(),
        "the wheel refused the config: {}",
        response.result
    );
    let wheel = lines(&response.result["log"]);

    // The wheel ran at its default level.
    let (port, ()) = capture(|| {
        for key in KEYS {
            log_warning(format!("Unknown key in LogTransform: '{key}'."));
        }
    });
    assert_eq!(
        port,
        wheel,
        "port:\n{:#?}\nwheel:\n{:#?}",
        port.iter()
            .map(|l| String::from_utf8_lossy(l))
            .collect::<Vec<_>>(),
        wheel
            .iter()
            .map(|l| String::from_utf8_lossy(l))
            .collect::<Vec<_>>()
    );
}

/// The messages of the `LogMessage` test: white space and line breaks at either end and
/// inside, control characters, non-ASCII text, and NULs.
const MESSAGES: &[&str] = &[
    "Dummy message",
    "",
    "   ",
    "a\n",
    "a\n\n\n",
    "\na",
    "\n\na",
    "a\r\nb",
    "a\rb",
    "a\r",
    "a \n b",
    "a\t\n",
    "a\x0b",
    "a\x7f",
    "a\u{a0}",
    "a\0b",
    "x\x01",
    "\x1f",
    "a\n \nb",
    "a\n\tb\t",
    " lead",
    "\u{e9}t\u{e9}\n\u{65e5}",
    "l1\nl2\nl3",
    "a\x0c\x0d",
    "\r\n",
    "\0",
];

#[test]
fn log_message_matches_the_wheel() {
    let _state = logging_state();
    let names: Vec<&str> = LEVEL_NAMES.iter().map(|(name, _)| *name).collect();
    let response = Oracle::get().call(
        "log_message",
        json!({"settings": names, "levels": names, "messages": MESSAGES}),
        &[],
    );
    let wheel = response.result["settings"].as_array().expect("settings");
    assert_eq!(wheel.len(), LEVEL_NAMES.len());

    let previous = get_logging_level();
    for (&(setting, _), wheel) in LEVEL_NAMES.iter().zip(wheel) {
        set_logging_level(level(setting));
        assert_eq!(
            logging_level_to_string(get_logging_level()),
            wheel["level"],
            "the level after setting {setting}"
        );
        let calls = wheel["calls"].as_array().expect("calls");
        let mut index = 0;
        for &(message_level, _) in &LEVEL_NAMES {
            for message in MESSAGES {
                let wheel = &calls[index];
                index += 1;
                let (port, result) =
                    capture(|| log_message(level(message_level), message.as_bytes()));
                let what = format!("{setting}, LogMessage({message_level}, {message:?})");
                assert_eq!(port, lines(&wheel["lines"]), "lines of {what}");
                let exception = match result {
                    Ok(()) => Value::Null,
                    Err(e) => json!({"type": "Exception", "message": e.message()}),
                };
                assert_eq!(exception, wheel["exception"], "exception of {what}");
            }
        }
        assert_eq!(index, calls.len());
    }
    set_logging_level(previous);
}

/// Strings for `LoggingLevelFromString`: every name in various cases, the digits, and strings
/// that are close to them.
const LEVEL_STRINGS: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "4",
    "-1",
    "none",
    "NONE",
    "None",
    "nOnE",
    "warning",
    "WARNING",
    "Warning",
    "info",
    "INFO",
    "Info",
    "debug",
    "DEBUG",
    "Debug",
    "unknown",
    "UNKNOWN",
    "",
    " 1",
    "1 ",
    "01",
    "+1",
    "1.0",
    "info ",
    " info",
    "\u{130}nfo",
    "\u{131}nfo",
    "debug\0x",
    "de\0bug",
    "\u{e9}",
    "warn",
    "error",
    "0x1",
    "3\n",
];

#[test]
fn logging_level_strings_match_the_wheel() {
    let names: Vec<&str> = LEVEL_NAMES.iter().map(|(name, _)| *name).collect();
    let response = Oracle::get().call(
        "logging_level_strings",
        json!({"from_string": LEVEL_STRINGS, "to_string": names}),
        &[],
    );
    let from_string: Vec<&str> = LEVEL_STRINGS
        .iter()
        .map(|s| level_name(logging_level_from_string(Some(s.as_bytes()))))
        .collect();
    assert_eq!(json!(from_string), response.result["from_string"]);
    let to_string: Vec<&str> = LEVEL_NAMES
        .iter()
        .map(|(_, level)| logging_level_to_string(*level))
        .collect();
    assert_eq!(json!(to_string), response.result["to_string"]);
}

/// Set in a new process of this test: the `OCIO_LOGGING_LEVEL` case it runs.
const ENVIRONMENT_CASE: &str = "OCIO_RS_LOGGING_ENVIRONMENT_CASE";

/// What precedes that process's report, on the line libtest starts for the test.
const REPORT: &str = "logging environment report: ";

/// The port's side of a `logging_environment` case, in a new process of this test: the steps
/// of the oracle's process, with the variable given through the environment provider.
fn run_environment_case(case: OsString) {
    let case: Value = serde_json::from_str(case.to_str().expect("UTF-8")).expect("a case");
    let mut vars = BTreeMap::new();
    if let Some(value) = case["env"].as_str() {
        vars.insert("OCIO_LOGGING_LEVEL".to_string(), value.to_string());
    }
    set_env_provider(Some(Arc::new(MapEnv::from(vars))));

    let received = Arc::new(Mutex::new(Vec::<String>::new()));
    if case["custom_function"].as_bool().expect("custom_function") {
        let sink = Arc::clone(&received);
        set_logging_function(Some(Arc::new(move |line: &[u8]| {
            sink.lock()
                .unwrap()
                .push(String::from_utf8(line.to_vec()).expect("UTF-8"));
        })))
        .unwrap();
    }
    let first = logging_level_to_string(get_logging_level());
    if let Some(name) = case["set_level"].as_str() {
        set_logging_level(level(name));
    }
    let after = logging_level_to_string(get_logging_level());
    let log = |key: &str| {
        for message in case[key].as_array().expect("messages") {
            log_message(
                level(message[0].as_str().expect("a level")),
                message[1].as_str().expect("a message").as_bytes(),
            )
            .unwrap();
        }
    };
    log("messages");
    reset_to_default_logging_function();
    log("messages_after_reset");
    let null_function = set_logging_function(None)
        .err()
        .map(|e| e.message().to_string());
    reset_to_default_logging_function();
    let received = received.lock().unwrap().clone();
    println!(
        "{REPORT}{}",
        json!({"first": first, "after": after, "received": received,
               "null_function": null_function})
    );
}

#[test]
fn logging_environment_matches_the_wheel() {
    if let Some(case) = std::env::var_os(ENVIRONMENT_CASE) {
        run_environment_case(case);
        return;
    }
    let values = [
        None,
        Some(""),
        Some("0"),
        Some("none"),
        Some("1"),
        Some("warning"),
        Some("info"),
        Some("debug"),
        Some("DEBUG"),
        Some("3"),
        Some("bogus"),
        Some(" 1"),
        Some("unknown"),
    ];
    let mut cases = Vec::new();
    for value in values {
        for custom_function in [false, true] {
            cases.push(json!({
                "env": value,
                "custom_function": custom_function,
                "set_level": "LOGGING_LEVEL_NONE",
                "messages": [["LOGGING_LEVEL_WARNING", "w1\nw2"], ["LOGGING_LEVEL_DEBUG", "d1"]],
                "messages_after_reset": [["LOGGING_LEVEL_WARNING", "after reset"]],
            }));
        }
    }
    let response = Oracle::get().call("logging_environment", json!({ "cases": cases }), &[]);
    let wheel = response.result.as_array().expect("a list");
    assert_eq!(wheel.len(), cases.len());

    let this_test = std::env::current_exe().expect("the test's path");
    for (case, wheel) in cases.iter().zip(wheel) {
        assert_eq!(wheel["returncode"], 0, "the wheel's process for {case}");
        let output = Command::new(&this_test)
            .args([
                "logging_environment_matches_the_wheel",
                "--exact",
                "--nocapture",
                "--test-threads",
                "1",
            ])
            .env(ENVIRONMENT_CASE, case.to_string())
            // The port must read the variable through the provider only.
            .env_remove("OCIO_LOGGING_LEVEL")
            .output()
            .expect("a new process of this test");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "the port's process for {case} failed: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = stdout
            .lines()
            .find_map(|line| line.split_once(REPORT).map(|(_, report)| report))
            .unwrap_or_else(|| panic!("no report for {case}: {stdout}"));
        let report: Value = serde_json::from_str(report).expect("a report");
        assert_eq!(report, wheel["report"], "{case}");
        let stderr = &response.blobs[wheel["stderr"].as_u64().expect("a blob") as usize];
        assert_bytes_eq(&format!("stderr of {case}"), stderr, &output.stderr);
    }
}
