// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in configs against the wheel, live on this platform (WP 3.10a): the registry's
//! names, names for user interfaces, flags and texts (the texts' line ends are the platform's,
//! I-148), `ResolveConfigPath`, and `Config::CreateFromBuiltinConfig` for names that resolve
//! and names that don't: the error byte for byte, or the config's `serialize()`.

use ocio::{BuiltinConfigRegistry, Config, resolve_config_path};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, hex};
use serde_json::{Value, json};
use std::sync::Arc;

/// The bytes a call or a config source gave: its result, or the bytes the binding couldn't
/// decode.
fn wheel_bytes(v: &Value, key: &str) -> Vec<u8> {
    match v.get("undecodable") {
        Some(h) => bytes(&json!({"bytes": h})),
        None => bytes(&v[key]),
    }
}

/// The registry: names, names for user interfaces, recommended flags and texts, in order.
#[test]
fn the_registry_holds_the_wheels_configs() {
    let names = Oracle::get()
        .call("builtin_config_names", json!({}), &[])
        .result;
    let names = names.as_array().unwrap();
    let registry = BuiltinConfigRegistry::get();
    assert_eq!(registry.num_builtin_configs(), names.len());
    for (i, wheel) in names.iter().enumerate() {
        let name = wheel["name"].as_str().unwrap();
        assert_eq!(
            registry.builtin_config_name(i).unwrap(),
            name.as_bytes(),
            "name {i}"
        );
        assert_eq!(
            registry.builtin_config_ui_name(i).unwrap(),
            wheel["ui_name"].as_str().unwrap().as_bytes(),
            "{name}: UI name"
        );
        assert_eq!(
            registry.is_builtin_config_recommended(i).unwrap(),
            wheel["recommended"].as_bool().unwrap(),
            "{name}: recommended"
        );
        let source = Oracle::get().call("builtin_config_source", json!({"name": name}), &[]);
        let text = registry.builtin_config(i).unwrap();
        assert!(
            text == source.blobs[0].as_slice(),
            "{name}: the text differs from the wheel's ({} bytes, the wheel's {})",
            text.len(),
            source.blobs[0].len()
        );
    }
}

/// Paths and URIs for `ResolveConfigPath` and `CreateFromBuiltinConfig`: upstream's, other
/// cases of the names and the prefix, URIs inside other text, and spaces around them.
fn paths() -> Vec<Vec<u8>> {
    let mut paths: Vec<Vec<u8>> = [
        "ocio://default",
        "ocio://cg-config-latest",
        "ocio://studio-config-latest",
        "default",
        "cg-config-latest",
        "studio-config-latest",
        "studio-config-latest.ocio",
        "/usr/local/share/aces.ocio",
        "C:\\myconfig\\config.ocio",
        "",
        "ocio://not-a-builtin",
        "ocio:default",
        "ocio://DEFAULT",
        "OCIO://default",
        "Ocio://cg-config-latest",
        "ocio://Studio-Config-Latest",
        "x ocio://default",
        "ocio://default y",
        "ocio:// default",
        "ocio://",
        "ocio://ocio://default",
        "ocio:// ocio://cg-config-latest",
        "ocio://default\tocio://studio-config-latest",
        "ocio://default\x0b",
        "ocio://\x0cdefault",
        "ocio://default\u{a0}",
        "I-do-not-exist",
        "ocio://I-do-not-exist",
        "thedefault",
        "CG-CONFIG-V1.0.0_ACES-V1.3_OCIO-V2.1",
        "ocio://studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
        "cg-config-v4.0.0_aces-v2.0_ocio-v2.5 ",
    ]
    .iter()
    .map(|p| p.as_bytes().to_vec())
    .collect();
    paths.push(b"ocio://default\0ignored".to_vec());
    paths.push(b"ocio://\xffdefault".to_vec());
    paths
}

/// `ResolveConfigPath` of each path.
#[test]
fn config_paths_resolve_as_in_the_wheel() {
    let paths = paths();
    let calls: Vec<Value> = paths
        .iter()
        .map(|p| json!({"call": "ResolveConfigPath", "on": "OCIO", "args": [{"bytes": hex(p)}]}))
        .collect();
    let wheel = Oracle::get()
        .call(
            "config_calls",
            json!({"config": "raw", "calls": calls}),
            &[],
        )
        .result;
    let mut failures = Vec::new();
    for (path, w) in paths.iter().zip(wheel["calls"].as_array().unwrap()) {
        let wheel = wheel_bytes(w, "result");
        let port = resolve_config_path(path);
        if wheel != port {
            failures.push(format!(
                "{:?}: wheel {:?}, port {:?}",
                String::from_utf8_lossy(path),
                String::from_utf8_lossy(&wheel),
                String::from_utf8_lossy(port)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `CreateFromBuiltinConfig` of each path, and of each name of the registry: the error, or the
/// config's text.
#[test]
fn builtin_configs_are_created_as_in_the_wheel() {
    let mut names = paths();
    let registry = BuiltinConfigRegistry::get();
    for i in 0..registry.num_builtin_configs() {
        names.push(registry.builtin_config_name(i).unwrap().to_vec());
    }
    let calls: Vec<BatchCall<'_>> = names
        .iter()
        .map(|name| BatchCall {
            cmd: "config_calls",
            args: json!({"config": {"builtin": {"bytes": hex(name)}},
                         "calls": [{"call": "serialize"}]}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);
    // The port reads the same, empty, environment as the wheel.
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
    let mut failures = Vec::new();
    for (name, w) in names.iter().zip(wheel) {
        let w = w.expect("config_calls").result;
        let config = &w["config"];
        let wheel = if config.is_null() {
            Ok(wheel_bytes(&w["calls"][0], "result"))
        } else if config.get("undecodable").is_some() {
            Err(wheel_bytes(config, "undecodable"))
        } else {
            Err(bytes(&config["exception"]["message"]))
        };
        let port = Config::create_from_builtin_config(name)
            .and_then(|c| c.serialize())
            .map_err(|e| e.what().to_vec());
        if wheel != port {
            let show = |r: &Result<Vec<u8>, Vec<u8>>| match r {
                Ok(text) => format!("a config of {} bytes", text.len()),
                Err(e) => format!("error {:?}", String::from_utf8_lossy(e)),
            };
            failures.push(format!(
                "{:?}: wheel {}, port {}",
                String::from_utf8_lossy(name),
                show(&wheel),
                show(&port)
            ));
        }
    }
    set_thread_env_provider(None);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
