// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio::Config`'s own state against the wheel's, through the oracle's `config_calls`: the
//! same calls on a new config (`Config()`, `Config::Create`) on both sides, in the same
//! environment, each call's result or exception compared byte for byte, then the config's
//! getters and its context's `repr()` (upstream's `operator<<`).
//!
//! The oracle's process holds exactly the request's variables, set one by one; the port reads
//! a `MapEnv` the same variables are set in, through OCIO's `Setenv`, on the test's own thread.

use std::sync::Arc;

use ocio::Config;
use ocio_ops::open_color_types::EnvironmentMode;
use ocio_ops::platform::{MapEnv, set_thread_env_provider, setenv};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes_arg, hex, log};
use serde_json::{Value, json};

/// An environment: each variable's name and value.
type Env<'a> = &'a [(&'a str, &'a [u8])];

/// The port's side of a call: its outcome, as the oracle writes a call's (`{"result": value}`,
/// `{"exception": {"type", "message"}}` or `{"undecodable": hex}`).
type PortCall = Box<dyn Fn(&mut Config) -> Value>;

/// A call on the config: the wheel's (a `config_calls` call) and the port's.
struct Step {
    call: Value,
    port: PortCall,
}

fn step(call: Value, port: impl Fn(&mut Config) -> Value + 'static) -> Step {
    Step {
        call,
        port: Box::new(port),
    }
}

/// A string the library returned, as the binding gives it: `{"bytes": hex}` when it is UTF-8,
/// else `{"undecodable": hex}` for the whole call.
fn text_out(s: &[u8]) -> Value {
    match std::str::from_utf8(s) {
        Ok(_) => json!({"result": bytes_arg(s)}),
        Err(_) => json!({"undecodable": hex(s)}),
    }
}

/// A list of strings, element by element (an element the binding can't decode is
/// `{"undecodable": hex}`).
fn texts_out(list: &[Vec<u8>]) -> Value {
    let elements: Vec<Value> = list
        .iter()
        .map(|s| match std::str::from_utf8(s) {
            Ok(_) => bytes_arg(s),
            Err(_) => json!({"undecodable": hex(s)}),
        })
        .collect();
    json!({ "result": elements })
}

/// What a call that returns nothing gives, or its exception.
fn unit_out(r: ocio::Result<()>) -> Value {
    match r {
        Ok(()) => json!({"result": null}),
        Err(e) => match std::str::from_utf8(e.what()) {
            Ok(_) => json!({"exception": {"type": "Exception", "message": bytes_arg(e.what())}}),
            Err(_) => json!({"undecodable": hex(e.what())}),
        },
    }
}

fn mode_name(mode: EnvironmentMode) -> &'static str {
    match mode {
        EnvironmentMode::Unknown => "ENV_ENVIRONMENT_UNKNOWN",
        EnvironmentMode::LoadPredefined => "ENV_ENVIRONMENT_LOAD_PREDEFINED",
        EnvironmentMode::LoadAll => "ENV_ENVIRONMENT_LOAD_ALL",
    }
}

fn mode(m: EnvironmentMode) -> Value {
    json!({"enum": mode_name(m)})
}

fn arg(s: &[u8]) -> Value {
    bytes_arg(s)
}

// ---------------------------------------------------------------------------------------------
// Calls

fn set_major_version(v: u32) -> Step {
    step(json!({"call": "setMajorVersion", "args": [v]}), move |c| {
        unit_out(c.set_major_version(v))
    })
}

fn set_minor_version(v: u32) -> Step {
    step(json!({"call": "setMinorVersion", "args": [v]}), move |c| {
        unit_out(c.set_minor_version(v))
    })
}

fn set_version(major: u32, minor: u32) -> Step {
    step(
        json!({"call": "setVersion", "args": [major, minor]}),
        move |c| unit_out(c.set_version(major, minor)),
    )
}

fn set_name(name: &[u8]) -> Step {
    let name = name.to_vec();
    step(json!({"call": "setName", "args": [arg(&name)]}), move |c| {
        c.set_name(&name);
        json!({"result": null})
    })
}

fn set_description(description: &[u8]) -> Step {
    let description = description.to_vec();
    step(
        json!({"call": "setDescription", "args": [arg(&description)]}),
        move |c| {
            c.set_description(&description);
            json!({"result": null})
        },
    )
}

fn set_family_separator(separator: u8) -> Step {
    step(
        json!({"call": "setFamilySeparator", "args": [arg(&[separator])]}),
        move |c| unit_out(c.set_family_separator(separator)),
    )
}

fn add_environment_var(name: &[u8], value: Option<&[u8]>) -> Step {
    let name = name.to_vec();
    let value = value.map(<[u8]>::to_vec);
    let value_arg = value.as_deref().map_or(Value::Null, arg);
    step(
        json!({"call": "addEnvironmentVar", "args": [arg(&name), value_arg]}),
        move |c| {
            c.add_environment_var(&name, value.as_deref());
            json!({"result": null})
        },
    )
}

fn get_environment_var_default(name: &[u8]) -> Step {
    let name = name.to_vec();
    step(
        json!({"call": "getEnvironmentVarDefault", "args": [arg(&name)]}),
        move |c| text_out(c.environment_var_default(&name)),
    )
}

fn clear_environment_vars() -> Step {
    step(json!({"call": "clearEnvironmentVars"}), |c| {
        c.clear_environment_vars();
        json!({"result": null})
    })
}

fn set_environment_mode(m: EnvironmentMode) -> Step {
    step(
        json!({"call": "setEnvironmentMode", "args": [mode(m)]}),
        move |c| {
            c.set_environment_mode(m);
            json!({"result": null})
        },
    )
}

fn load_environment() -> Step {
    step(json!({"call": "loadEnvironment"}), |c| {
        c.load_environment();
        json!({"result": null})
    })
}

fn set_search_path(path: &[u8]) -> Step {
    let path = path.to_vec();
    step(
        json!({"call": "setSearchPath", "args": [arg(&path)]}),
        move |c| {
            c.set_search_path(&path);
            json!({"result": null})
        },
    )
}

fn add_search_path(path: &[u8]) -> Step {
    let path = path.to_vec();
    step(
        json!({"call": "addSearchPath", "args": [arg(&path)]}),
        move |c| {
            c.add_search_path(&path);
            json!({"result": null})
        },
    )
}

fn clear_search_paths() -> Step {
    step(json!({"call": "clearSearchPaths"}), |c| {
        c.clear_search_paths();
        json!({"result": null})
    })
}

fn set_working_dir(dir: &[u8]) -> Step {
    let dir = dir.to_vec();
    step(
        json!({"call": "setWorkingDir", "args": [arg(&dir)]}),
        move |c| {
            c.set_working_dir(&dir);
            json!({"result": null})
        },
    )
}

/// The config's getters, then its context's `repr()`.
fn getters() -> Vec<Step> {
    vec![
        step(
            json!({"call": "getMajorVersion"}),
            |c| json!({"result": c.major_version()}),
        ),
        step(
            json!({"call": "getMinorVersion"}),
            |c| json!({"result": c.minor_version()}),
        ),
        step(json!({"call": "getName"}), |c| text_out(c.name())),
        step(json!({"call": "getDescription"}), |c| {
            text_out(c.description())
        }),
        step(json!({"call": "getFamilySeparator"}), |c| {
            text_out(&[c.family_separator()])
        }),
        step(json!({"call": "getEnvironmentVarNames"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_environment_vars())
                .map(|i| c.environment_var_name_by_index(i).to_vec())
                .collect();
            texts_out(&names)
        }),
        step(
            json!({"call": "getEnvironmentMode"}),
            |c| json!({"result": mode(c.environment_mode())}),
        ),
        step(json!({"call": "getSearchPath"}), |c| {
            text_out(c.search_path())
        }),
        step(json!({"call": "getSearchPaths"}), |c| {
            let paths: Vec<Vec<u8>> = (0..c.num_search_paths())
                .map(|i| c.search_path_with_index(i).to_vec())
                .collect();
            texts_out(&paths)
        }),
        step(json!({"call": "getWorkingDir"}), |c| {
            text_out(c.working_dir())
        }),
        step(json!({"call": "getCurrentContext", "as": "context"}), |c| {
            let repr = c.current_context().to_bytes();
            let repr = match std::str::from_utf8(&repr) {
                Ok(_) => bytes_arg(&repr),
                Err(_) => json!({"undecodable": hex(&repr)}),
            };
            json!({"result": {"class": "Context", "repr": repr}})
        }),
    ]
}

/// Copies the config (`copy.deepcopy`: `createEditableCopy`); the steps after it act on the
/// copy, on both sides.
const COPY: &str = "copy";

// ---------------------------------------------------------------------------------------------
// The check

/// Runs `steps` (each followed by the getters) on a new config in the environment `env`, on
/// both sides, and compares every outcome. `steps` may hold `None`: a copy of the config.
fn check(label: &str, env: &[(&str, &[u8])], steps: Vec<Option<Step>>) {
    check_source(label, "new", env, steps);
}

/// As [`check`], on the config of `source`: `"new"` (`Config()`) or `"raw"`
/// (`Config.CreateRaw()`).
fn check_source(label: &str, source: &str, env: &[(&str, &[u8])], steps: Vec<Option<Step>>) {
    let mut sequence: Vec<Option<Step>> = Vec::new();
    sequence.extend(getters().into_iter().map(Some));
    for s in steps {
        sequence.push(s);
        sequence.extend(getters().into_iter().map(Some));
    }

    let mut calls = Vec::new();
    let mut on = "config".to_string();
    for s in &sequence {
        match s {
            Some(s) => {
                let mut call = s.call.clone();
                if call.get("on").is_none() {
                    call["on"] = json!(on);
                }
                calls.push(call);
            }
            None => {
                calls.push(json!({"copy": on, "as": COPY}));
                on = COPY.to_string();
            }
        }
    }
    let env_json: serde_json::Map<String, Value> = env
        .iter()
        .map(|(k, v)| {
            let value = match std::str::from_utf8(v) {
                Ok(text) => json!(text),
                Err(_) => arg(v),
            };
            (k.to_string(), value)
        })
        .collect();
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": source, "env": env_json, "calls": calls}),
        &[],
    );
    let wheel = &response.result;
    assert!(log(&wheel["config_log"]).is_empty(), "{label}: {wheel}");

    set_thread_env_provider(Some(Arc::new(MapEnv::default())));
    for (name, value) in env {
        setenv(name, value).expect("a request's variable");
    }
    let made = match source {
        "new" => Config::new(),
        "raw" => Ok(Arc::try_unwrap(Config::create_raw()).expect("a config of its own")),
        _ => panic!("unknown source {source}"),
    };
    let mut failures = Vec::new();
    let mut config = match made {
        Ok(config) => {
            assert_eq!(
                wheel["config"],
                Value::Null,
                "{label}: the wheel refused the config"
            );
            config
        }
        Err(e) => {
            set_thread_env_provider(None);
            assert_eq!(
                wheel["config"],
                unit_out(Err(e)),
                "{label}: the port refused the config"
            );
            return;
        }
    };
    let results = wheel["calls"].as_array().expect("the calls' results");
    assert_eq!(results.len(), sequence.len(), "{label}");
    for (i, (s, w)) in sequence.iter().zip(results).enumerate() {
        assert!(log(&w["log"]).is_empty(), "{label}, call {i}: {w}");
        let mut w = w.clone();
        w.as_object_mut().expect("a call's outcome").remove("log");
        match s {
            Some(s) => {
                let port = (s.port)(&mut config);
                if port != w {
                    failures.push(format!(
                        "{label}, call {i} {}: wheel {w}, port {port}",
                        s.call
                    ));
                }
            }
            None => {
                config = config.clone();
                assert!(w.get("result").is_some(), "{label}, call {i}: {w}");
            }
        }
    }
    set_thread_env_provider(None);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// Cases

#[test]
fn a_new_config_matches_the_wheel() {
    check("defaults", &[], vec![]);
}

#[test]
fn versions_match_the_wheel() {
    check(
        "versions",
        &[],
        vec![
            Some(set_minor_version(6)),
            Some(set_minor_version(0)),
            Some(set_minor_version(5)),
            Some(set_major_version(1)),
            Some(set_minor_version(1)),
            Some(set_minor_version(0)),
            Some(set_major_version(0)),
            Some(set_major_version(3)),
            Some(set_major_version(u32::MAX)),
            Some(set_version(2, 9)),
            Some(set_version(1, 0)),
            Some(set_version(3, 4)),
            Some(set_version(2, 3)),
            Some(set_version(1, 1)),
            Some(set_version(2, 0)),
            None,
            Some(set_minor_version(4)),
        ],
    );
}

#[test]
fn name_description_and_family_separator_match_the_wheel() {
    let mut steps = vec![
        Some(set_name(b"my config")),
        Some(set_description(b"line 1\nline 2")),
        Some(set_name(b"a\0b")),
        Some(set_description(b"\xff\xfe")),
        Some(set_description(b"")),
        Some(set_name(b"\xc3\xa9t\xc3\xa9")),
        None,
        Some(set_name(b"")),
    ];
    for separator in [
        b' ', 0, b'/', 0x7f, 0x1f, b'~', b'!', 31, 32, 126, 127, 0x80, 0xe9, 0xff, b'\n',
    ] {
        steps.push(Some(set_family_separator(separator)));
    }
    steps.push(None);
    check("name, description and family separator", &[], steps);
}

#[test]
fn environment_variables_match_the_wheel() {
    let env: &[(&str, &[u8])] = &[("OCIO_TEST_A", b"from the environment"), ("HOME2", b"/h")];
    check(
        "environment variables",
        env,
        vec![
            Some(add_environment_var(b"B", Some(b"1"))),
            Some(add_environment_var(b"A", Some(b"2"))),
            Some(add_environment_var(b"a", Some(b"lower"))),
            Some(add_environment_var(b"C", Some(b""))),
            Some(add_environment_var(b"", Some(b"ignored"))),
            Some(add_environment_var(b"D\0E", Some(b"v\0w"))),
            Some(add_environment_var(b"\xc3\xa9", Some(b"\xff"))),
            Some(get_environment_var_default(b"A")),
            Some(get_environment_var_default(b"a")),
            Some(get_environment_var_default(b"C")),
            Some(get_environment_var_default(b"D")),
            Some(get_environment_var_default(b"unknown")),
            Some(get_environment_var_default(b"")),
            Some(get_environment_var_default(b"\xc3\xa9")),
            Some(add_environment_var(b"A", None)),
            Some(add_environment_var(b"unknown", None)),
            Some(add_environment_var(b"B", Some(b"1"))),
            Some(add_environment_var(b"OCIO_TEST_A", Some(b"default"))),
            Some(load_environment()),
            None,
            Some(add_environment_var(b"Z", Some(b"z"))),
            Some(set_environment_mode(EnvironmentMode::LoadAll)),
            Some(load_environment()),
            None,
            Some(load_environment()),
            Some(set_environment_mode(EnvironmentMode::LoadAll)),
            Some(load_environment()),
            Some(clear_environment_vars()),
            Some(set_environment_mode(EnvironmentMode::Unknown)),
            Some(load_environment()),
        ],
    );
}

#[test]
fn search_paths_and_working_dir_match_the_wheel() {
    check(
        "search paths",
        &[],
        vec![
            Some(set_search_path(b"a:b:c")),
            Some(add_search_path(b"")),
            Some(add_search_path(b"d")),
            Some(add_search_path(b"\0e")),
            Some(add_search_path(b"f:g")),
            Some(set_working_dir(b"/work/dir")),
            None,
            Some(set_search_path(b"")),
            Some(add_search_path(b"$VAR/luts")),
            Some(set_search_path(b"C:\\luts;D:\\other:x")),
            Some(clear_search_paths()),
            Some(set_working_dir(b"")),
            Some(set_working_dir(b"\xff")),
        ],
    );
}

/// The active displays and views of the environment, read when the config is made: the
/// config is refused when a list opens a quote it doesn't close before a separator.
#[test]
fn the_environment_lists_match_the_wheel() {
    let cases: &[(&str, Env)] = &[
        (
            "an unclosed quote in the active views",
            &[("OCIO_ACTIVE_VIEWS", b"\"a,b")],
        ),
        (
            "an unclosed quote in the active displays",
            &[("OCIO_ACTIVE_DISPLAYS", b"x:\"y")],
        ),
        (
            "both refused",
            &[
                ("OCIO_ACTIVE_DISPLAYS", b"\"d1,d2"),
                ("OCIO_ACTIVE_VIEWS", b"\"v1,v2"),
            ],
        ),
        ("an unclosed quote alone", &[("OCIO_ACTIVE_VIEWS", b"\"a")]),
        (
            "quoted lists",
            &[
                ("OCIO_ACTIVE_DISPLAYS", b" \"a,b\", c "),
                ("OCIO_ACTIVE_VIEWS", b"v1:v2"),
                ("OCIO_INACTIVE_COLORSPACES", b"  cs1, cs2  "),
            ],
        ),
        (
            "empty lists",
            &[
                ("OCIO_ACTIVE_DISPLAYS", b"   "),
                ("OCIO_ACTIVE_VIEWS", b""),
                ("OCIO_INACTIVE_COLORSPACES", b""),
            ],
        ),
        ("not UTF-8", &[("OCIO_ACTIVE_VIEWS", b"\"\xff,b")]),
    ];
    for (label, env) in cases {
        if cfg!(windows) && env.iter().any(|(_, v)| std::str::from_utf8(v).is_err()) {
            // The oracle sets Windows variables from text only.
            continue;
        }
        check(label, env, vec![]);
    }
}

/// The raw config: its version, and the whole environment in its context.
#[test]
fn the_raw_config_matches_the_wheel() {
    let env: &[(&str, &[u8])] = &[("OCIO_TEST_A", b"a"), ("PATH_LIKE", b"x:y")];
    check_source(
        "raw",
        "raw",
        env,
        vec![
            None,
            Some(add_environment_var(b"OCIO_TEST_A", Some(b"default"))),
            Some(load_environment()),
        ],
    );
}
