// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the caches: a port of `tests/cpu/Caching_tests.cpp` @ v2.5.2, and the string hash
//! the caches key their entries with, against the Linux wheel's C++ library.

use std::sync::Arc;

use super::*;
use crate::test_env::EnvGuard;

/// A dummy struct to test the cache classes.
#[derive(Debug, Default)]
struct Data {
    _status: bool,
}

type DataRcPtr = Arc<Data>;

fn key(name: &str) -> String {
    name.to_string()
}

/// Port of `OCIO_ADD_TEST(Caching, generic_cache)` @ v2.5.2.
#[test]
fn generic_cache() {
    // A unit test to check the GenericCache class.
    let env = EnvGuard::new();

    {
        let cache: GenericCache<String, DataRcPtr> = GenericCache::new();
        assert!(cache.is_enabled());

        let entry1 = Arc::new(Data::default());
        {
            let mut m = cache.lock().expect("an enabled cache");
            m.entries().insert(key("entry1"), entry1.clone());
        }
        assert!(cache.exists(&key("entry1")));
        {
            let mut m = cache.lock().expect("an enabled cache");
            assert!(Arc::ptr_eq(&m.entries()[&key("entry1")], &entry1));
        }

        // Some faulty checks.
        assert!(!cache.exists(&key("entry2")));

        // Flush the cache and check the content.
        cache.clear();
        assert!(!cache.exists(&key("entry1")));
        assert!(!cache.exists(&key("entry2")));
    }

    {
        // Disable all the caches.
        env.set(&[(OCIO_DISABLE_ALL_CACHES, "1")]);

        let cache: GenericCache<String, DataRcPtr> = GenericCache::new();
        assert!(!cache.is_enabled());

        // Upstream writes the entry to its dummy entry; the port's lock gives no entries.
        assert!(cache.lock().is_none());

        assert!(!cache.exists(&key("entry1")));
        env.set(&[]);
    }

    {
        // Disable processor caches i.e. no impact on the generic cache.
        env.set(&[(OCIO_DISABLE_PROCESSOR_CACHES, "1")]);

        let cache: GenericCache<String, DataRcPtr> = GenericCache::new();
        assert!(cache.is_enabled());

        let entry1 = Arc::new(Data::default());
        cache
            .lock()
            .expect("an enabled cache")
            .entries()
            .insert(key("entry1"), entry1);

        assert!(cache.exists(&key("entry1")));
        env.set(&[]);
    }
}

/// The parts of upstream's `Caching/processor_cache` test (tests/cpu/Caching_tests.cpp:110-159
/// @ v2.5.2) before its config: the test's last part reads a config with a `FileTransform`,
/// which comes with the config reader (Phase 3), and the test's port marker with it.
#[test]
fn processor_cache_without_the_config() {
    // A unit test to check the ProcessorCache class.
    let env = EnvGuard::new();

    {
        let cache: ProcessorCache<String, DataRcPtr> = ProcessorCache::new();
        assert!(cache.is_enabled());
    }

    // Test that the content of the cache stays the same after disabling and enabling the cache.
    {
        let cache: ProcessorCache<String, DataRcPtr> = ProcessorCache::new();
        assert!(cache.is_enabled());

        let entry1 = Arc::new(Data::default());
        cache
            .lock()
            .expect("an enabled cache")
            .entries()
            .insert(key("entry1"), entry1);

        cache.enable(false);

        // Expecting failure because the cache is disabled.
        assert!(!cache.exists(&key("entry1")));

        cache.enable(true);

        // The data with the key "entry1" still exists after enabling the cache.
        assert!(cache.exists(&key("entry1")));
    }

    {
        // Disable all the caches.
        env.set(&[(OCIO_DISABLE_ALL_CACHES, "1")]);

        let cache1: ProcessorCache<String, DataRcPtr> = ProcessorCache::new();
        assert!(!cache1.is_enabled());

        let cache2: GenericCache<String, DataRcPtr> = GenericCache::new();
        assert!(!cache2.is_enabled());
    }

    {
        // Only disable the processor caches so the other caches are still enabled.
        env.set(&[(OCIO_DISABLE_PROCESSOR_CACHES, "1")]);

        let cache1: ProcessorCache<String, DataRcPtr> = ProcessorCache::new();
        assert!(!cache1.is_enabled());

        // But the generic cache is still enabled.
        let cache2: GenericCache<String, DataRcPtr> = GenericCache::new();
        assert!(cache2.is_enabled());
    }
}

/// A cache reads the environment when it is created, and only then (`m_envDisableAllCaches` is
/// `const`, Caching.h:88): a variable set later doesn't disable it, and once disabled by the
/// environment, `enable(true)` doesn't enable it. A variable set to the empty string is
/// present (`Platform::isEnvPresent`).
#[test]
fn caches_read_the_environment_once() {
    let env = EnvGuard::new();

    let enabled: ProcessorCache<u64, DataRcPtr> = ProcessorCache::new();
    env.set(&[(OCIO_DISABLE_PROCESSOR_CACHES, "")]);
    let disabled: ProcessorCache<u64, DataRcPtr> = ProcessorCache::new();
    env.set(&[]);

    assert!(enabled.is_enabled());
    assert!(!disabled.is_enabled());
    disabled.enable(true);
    assert!(!disabled.is_enabled());
    assert!(disabled.lock().is_none());
}

/// Texts of every length up to 40 bytes, every byte value, and two keys of the caches' form,
/// to check the hashes' 8-byte blocks and their tails.
fn texts() -> Vec<Vec<u8>> {
    let mut texts: Vec<Vec<u8>> = (0..=40usize)
        .map(|len| {
            (0..len)
                .map(|i| (i as u8).wrapping_mul(37).wrapping_add(len as u8))
                .collect()
        })
        .collect();
    texts.push((0..=255u8).collect());
    texts.push(b"88-1".to_vec());
    texts.push(b"<GroupTransform direction=forward, transforms=>0".to_vec());
    texts
}

/// The digest of [`texts`], pinned so the inputs can't change unnoticed.
#[test]
fn hash_inputs_are_pinned() {
    let mut all = Vec::new();
    for text in texts() {
        all.extend_from_slice(&(text.len() as u32).to_le_bytes());
        all.extend_from_slice(&text);
    }
    let digest = all.iter().enumerate().fold(0u64, |acc, (i, &b)| {
        acc.wrapping_mul(31).wrapping_add(u64::from(b) ^ i as u64)
    });
    assert_eq!((all.len(), digest), PINNED_TEXTS);
}

/// See [`hash_inputs_are_pinned`].
const PINNED_TEXTS: (usize, u64) = (1304, 1_295_993_977_883_512_475);

/// libstdc++'s `std::_Hash_bytes`, the Linux wheel's `std::hash<std::string>`, against the
/// library itself, with the seed `std::hash` passes and others.
#[cfg(target_os = "linux")]
#[test]
fn libstdcxx_hash_matches_the_library() {
    for text in texts() {
        for seed in [0xc70f_6907, 0, 1, u64::MAX] {
            assert_eq!(
                libstdcxx_hash_bytes(&text, seed),
                ocio_testkit::crt::libstdcxx_hash_bytes(&text, seed),
                "{text:?}, seed {seed:#x}"
            );
        }
        assert_eq!(
            std_hash_string(&text),
            ocio_testkit::crt::libstdcxx_hash_bytes(&text, 0xc70f_6907),
            "{text:?}"
        );
    }
}
