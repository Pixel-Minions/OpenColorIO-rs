// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! S1 (PLAN.md §3): the port reads each built-in config's text as OCIO 2.5.2 serializes it,
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

/// The `serialize.ocio` fixtures of the built-in configs.
fn serialized_configs() -> Vec<String> {
    let paths: Vec<String> = fixtures::list("builtin_configs/")
        .into_iter()
        .filter(|p| p.ends_with("/serialize.ocio"))
        .collect();
    assert_eq!(paths.len(), 8, "the eight built-in configs: {paths:?}");
    paths
}

#[test]
fn builtin_configs_read_and_write_byte_identically() {
    for path in serialized_configs() {
        let expected = fixtures::read_text(&path);
        set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
        let config = Config::create_from_stream(expected.as_bytes());
        set_thread_env_provider(None);
        let config = config.unwrap_or_else(|e| panic!("{path}: {e}"));
        let text = config.serialize().unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_text_eq(&path, &expected, std::str::from_utf8(&text).unwrap());
        let cache_id = config.cache_id().unwrap();
        let cache_id_path = path.replace("/serialize.ocio", "/cache_id.txt");
        assert_text_eq(
            &cache_id_path,
            &fixtures::read_text(&cache_id_path),
            &cache_id,
        );
    }
}
