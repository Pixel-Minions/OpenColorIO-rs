// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! S1 (PLAN.md §3): the port reads each built-in config's text as OCIO 2.6.0 serializes it,
//! writes it again byte for byte, and gives its cache ID.
//!
//! Each `serialize.ocio` fixture (the wheel's `Config.serialize()`) is read by the port
//! (`Config::create_from_stream`) and serialized again (`Config::serialize`); the text must
//! equal the fixture byte for byte, and the config's cache ID (`Config::cache_id`) the
//! wheel's (`cache_id.txt`).

use std::sync::Arc;

use ocio::Config;
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::{assert_text_eq, fixtures};

/// The built-in configs OCIO 2.6.0 registers: `CGCONFIG::Register`
/// (src/OpenColorIO/builtinconfigs/CGConfig.cpp:23-56 @ v2.6.0) and `STUDIOCONFIG::Register`
/// (src/OpenColorIO/builtinconfigs/StudioConfig.cpp:23-56 @ v2.6.0).
const BUILTIN_CONFIGS: [&str; 10] = [
    "cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
    "cg-config-v2.1.0_aces-v1.3_ocio-v2.3",
    "cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
    "cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
    "cg-config-v5.0.0_aces-v2.1_ocio-v2.6",
    "studio-config-v1.0.0_aces-v1.3_ocio-v2.1",
    "studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
    "studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
    "studio-config-v4.0.0_aces-v2.0_ocio-v2.5",
    "studio-config-v5.0.0_aces-v2.1_ocio-v2.6",
];

/// The `serialize.ocio` fixtures of the built-in configs: one for each config upstream
/// registers, and no other.
fn serialized_configs() -> Vec<String> {
    let mut paths: Vec<String> = fixtures::list("builtin_configs/")
        .into_iter()
        .filter(|p| p.ends_with("/serialize.ocio"))
        .collect();
    paths.sort();
    let mut expected: Vec<String> = BUILTIN_CONFIGS
        .iter()
        .map(|name| format!("builtin_configs/{name}/serialize.ocio"))
        .collect();
    expected.sort();
    assert_eq!(paths, expected, "the ten built-in configs");
    paths
}

#[test]
fn builtin_configs_read_and_write_byte_identically() {
    let paths = serialized_configs();
    let mut checked = 0;
    for path in &paths {
        let expected = fixtures::read_text(path);
        set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
        let config = Config::create_from_stream(expected.as_bytes());
        set_thread_env_provider(None);
        let config = config.unwrap_or_else(|e| panic!("{path}: {e}"));
        let text = config.serialize().unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_text_eq(path, &expected, std::str::from_utf8(&text).unwrap());
        let cache_id = config.cache_id().unwrap();
        let cache_id_path = path.replace("/serialize.ocio", "/cache_id.txt");
        assert_text_eq(
            &cache_id_path,
            &fixtures::read_text(&cache_id_path),
            &cache_id,
        );
        checked += 1;
    }
    assert_eq!(checked, BUILTIN_CONFIGS.len());
}
