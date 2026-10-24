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
            let mut m = cache.lock().unwrap().expect("an enabled cache");
            m.entries().insert(key("entry1"), entry1.clone());
        }
        assert!(cache.exists(&key("entry1")));
        {
            let mut m = cache.lock().unwrap().expect("an enabled cache");
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
        assert!(cache.lock().unwrap().is_none());

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
            .unwrap()
            .expect("an enabled cache")
            .entries()
            .insert(key("entry1"), entry1);

        assert!(cache.exists(&key("entry1")));
        env.set(&[]);
    }
}

/// Port of `OCIO_ADD_TEST(Caching, processor_cache)` @ v2.5.2.
#[test]
fn processor_cache() {
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
            .unwrap()
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
    // The end of the guard's scope.
    env.set(&[]);

    // Test the processor cache reset.
    {
        use crate::fileformats::test_utils::get_test_files_dir;
        use crate::processor::ProcessorCacheFlags;
        use ocio_ops::open_color_types::TransformDirection;

        let config_text = "ocio_profile_version: 2

search_path: "
            .to_owned()
            + &get_test_files_dir()
            + "

environment: {CS3: lut1d_green.ctf}

roles:
  default: cs1

displays:
  disp1:
    - !<View> {name: view1, colorspace: cs3}

colorspaces:
  - !<ColorSpace>
    name: cs1

  - !<ColorSpace>
    name: cs2
    from_scene_reference: !<MatrixTransform> {offset: [0.11, 0.12, 0.13, 0]}

  - !<ColorSpace>
    name: cs3
    from_scene_reference: !<FileTransform> {src: $CS3}
";

        let config = crate::Config::create_from_stream(config_text.as_bytes()).unwrap();

        // Creating a editable config to clear the processor cache later in the test.
        let cfg = (*config).clone();

        {
            // Test that clearProcessorCache clears the Processor cache.

            // Create two processors and confirm that it is the same object as expected.
            let proc_a = cfg
                .processor_for_display_view("cs3", "disp1", "view1", TransformDirection::Forward)
                .unwrap();
            let proc_b = cfg
                .processor_for_display_view("cs3", "disp1", "view1", TransformDirection::Forward)
                .unwrap();

            // Comparing the address of both Processor objects to confirm if they are the same or
            // not.
            assert!(Arc::ptr_eq(&proc_a, &proc_b));

            cfg.clear_processor_cache();

            // Create a third processor and confirm that it is different from the previous two as
            // the processor cache was cleared.
            let proc_c = cfg
                .processor_for_display_view("cs3", "disp1", "view1", TransformDirection::Forward)
                .unwrap();

            assert!(!Arc::ptr_eq(&proc_c, &proc_a));
        }

        {
            // Test that disable and re-enable the cache, using setProcessorCacheFlags, does not
            // clear the Processor cache.

            let proc_a = cfg
                .processor_for_display_view("cs3", "disp1", "view1", TransformDirection::Forward)
                .unwrap();

            // Disable and re-enable the processor cache.
            cfg.set_processor_cache_flags(ProcessorCacheFlags::OFF);
            cfg.set_processor_cache_flags(ProcessorCacheFlags::ENABLED);

            // Confirm that the processor is the same.
            let proc_b = cfg
                .processor_for_display_view("cs3", "disp1", "view1", TransformDirection::Forward)
                .unwrap();
            assert!(Arc::ptr_eq(&proc_a, &proc_b));
        }
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
    assert!(disabled.lock().unwrap().is_none());
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

/// A cache counts the runs of `clear` and `enable`, the two `noexcept` functions upstream locks
/// its mutex in, and tells whether this thread holds its lock (U-184).
#[test]
fn a_cache_counts_the_locks_of_its_changes_and_knows_its_holder() {
    let cache: GenericCache<String, DataRcPtr> = GenericCache::new();
    assert_eq!(cache.locks_by_changes(), 0);
    cache.clear();
    assert_eq!(cache.locks_by_changes(), 1);
    cache.enable(false);
    cache.enable(true);
    assert_eq!(cache.locks_by_changes(), 3);
    // Taking the lock, or reading the cache, is not a change.
    assert!(!cache.is_held_by_this_thread());
    {
        let guard = cache.lock().unwrap().expect("an enabled cache");
        assert!(cache.is_held_by_this_thread());
        // Another cache of this thread is not held.
        let other: GenericCache<String, DataRcPtr> = GenericCache::new();
        assert!(!other.is_held_by_this_thread());
        // Another thread doesn't hold it.
        std::thread::scope(|s| {
            s.spawn(|| assert!(!cache.is_held_by_this_thread()));
        });
        drop(guard);
    }
    assert!(!cache.is_held_by_this_thread());
    assert_eq!(cache.locks_by_changes(), 3);
    // A disabled cache's lock is not held (upstream doesn't lock it then).
    cache.enable(false);
    assert!(cache.lock().unwrap().is_none());
    assert!(!cache.is_held_by_this_thread());
}
