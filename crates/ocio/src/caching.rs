// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The caches of processors: a port of `src/OpenColorIO/Caching.h` @ v2.5.2, and the key
//! upstream gives them, `std::hash<std::string>` of a text, which each wheel's C++ library
//! computes its own way.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use ocio_ops::platform::is_env_present;

/// `OCIO_DISABLE_ALL_CACHES`: disables every cache, the processors' and the files'.
///
/// Port of `OCIO_DISABLE_ALL_CACHES` (src/OpenColorIO/Caching.cpp:16 @ v2.5.2).
pub const OCIO_DISABLE_ALL_CACHES: &str = "OCIO_DISABLE_ALL_CACHES";

/// `OCIO_DISABLE_PROCESSOR_CACHES`: disables the processors' caches.
///
/// Port of `OCIO_DISABLE_PROCESSOR_CACHES` (src/OpenColorIO/Caching.cpp:17 @ v2.5.2).
pub const OCIO_DISABLE_PROCESSOR_CACHES: &str = "OCIO_DISABLE_PROCESSOR_CACHES";

/// `OCIO_DISABLE_CACHE_FALLBACK`: disables the config's reuse of a cached processor of the same
/// cache ID.
///
/// Port of `OCIO_DISABLE_CACHE_FALLBACK` (src/OpenColorIO/Caching.cpp:18 @ v2.5.2).
pub const OCIO_DISABLE_CACHE_FALLBACK: &str = "OCIO_DISABLE_CACHE_FALLBACK";

/// `std::hash<std::string>{}(text)`, as each wheel's C++ library computes it: MSVC's FNV-1a
/// (64 bits) on Windows, libstdc++'s `_Hash_bytes` with the seed `0xc70f6907` on Linux. The
/// caches key their entries with it, so two texts with the same hash share an entry, on one
/// platform and not the other.
pub fn std_hash_string(text: &[u8]) -> u64 {
    if cfg!(windows) {
        msvc_fnv1a(text)
    } else {
        libstdcxx_hash_bytes(text, 0xc70f_6907)
    }
}

/// MSVC's `_Fnv1a_append_bytes` from `_FNV_offset_basis` (`<type_traits>`, the 64-bit
/// constants), which `std::hash<std::string>` calls on the string's bytes.
pub fn msvc_fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;
    let mut value = OFFSET_BASIS;
    for &b in bytes {
        value ^= u64::from(b);
        value = value.wrapping_mul(PRIME);
    }
    value
}

/// libstdc++'s `std::_Hash_bytes` for an 8-byte `size_t` (libstdc++-v3/libsupc++/
/// hash_bytes.cc), which `std::hash<std::string>` calls with the seed `0xc70f6907`.
pub fn libstdcxx_hash_bytes(bytes: &[u8], seed: u64) -> u64 {
    const MUL: u64 = (0xc6a4_a793u64 << 32) + 0x5bd1_e995;
    let shift_mix = |v: u64| v ^ (v >> 47);

    let len = bytes.len();
    let len_aligned = len & !0x7;
    let mut hash = seed ^ (len as u64).wrapping_mul(MUL);
    for chunk in bytes[..len_aligned].chunks(8) {
        let load = u64::from_le_bytes(chunk.try_into().expect("8 bytes"));
        let data = shift_mix(load.wrapping_mul(MUL)).wrapping_mul(MUL);
        hash ^= data;
        hash = hash.wrapping_mul(MUL);
    }
    if len & 0x7 != 0 {
        // `load_bytes`: the remaining bytes, the last one most significant.
        let data = bytes[len_aligned..]
            .iter()
            .rev()
            .fold(0u64, |acc, &b| (acc << 8) + u64::from(b));
        hash ^= data;
        hash = hash.wrapping_mul(MUL);
    }
    hash = shift_mix(hash).wrapping_mul(MUL);
    shift_mix(hash)
}

/// A cache of entries under a key, which the environment can disable when it is created, and
/// its owner at any time.
///
/// Port of `GenericCache` (src/OpenColorIO/Caching.h:24-94 @ v2.5.2). Upstream's lock and
/// `operator[]` become [`GenericCache::lock`], which returns the locked entries, or `None` when
/// the cache is disabled (where upstream returns its `dummy` entry, which nothing reads).
#[derive(Debug)]
pub struct GenericCache<K, V> {
    /// `m_envDisableAllCaches`: the environment disables the cache.
    env_disable_all_caches: bool,
    /// `m_enabled` and `m_entries`, under `m_mutex`.
    state: Mutex<CacheState<K, V>>,
}

#[derive(Debug)]
struct CacheState<K, V> {
    enabled: bool,
    entries: BTreeMap<K, V>,
}

impl<K: Ord, V> Default for GenericCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord, V> GenericCache<K, V> {
    /// An enabled cache, unless `OCIO_DISABLE_ALL_CACHES` is set.
    ///
    /// Port of `GenericCache::GenericCache()` (Caching.h:38-41 @ v2.5.2).
    pub fn new() -> Self {
        Self::with_disable(false)
    }

    /// An enabled cache, unless `OCIO_DISABLE_ALL_CACHES` is set or `disable_caches`.
    ///
    /// Port of `GenericCache::GenericCache(bool)` (Caching.h:83-86 @ v2.5.2).
    fn with_disable(disable_caches: bool) -> Self {
        GenericCache {
            env_disable_all_caches: is_env_present(OCIO_DISABLE_ALL_CACHES) || disable_caches,
            state: Mutex::new(CacheState {
                enabled: true,
                entries: BTreeMap::new(),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, CacheState<K, V>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Port of `GenericCache::clear` (Caching.h:45-50 @ v2.5.2).
    pub fn clear(&self) {
        self.state().entries.clear();
    }

    /// Port of `GenericCache::enable` (Caching.h:52-57 @ v2.5.2).
    pub fn enable(&self, enable: bool) {
        self.state().enabled = enable;
    }

    /// Port of `GenericCache::isEnabled` (Caching.h:59 @ v2.5.2).
    #[doc(alias = "isEnabled")]
    pub fn is_enabled(&self) -> bool {
        !self.env_disable_all_caches && self.state().enabled
    }

    /// Whether the cache is enabled and has an entry under `key`.
    ///
    /// Upstream's `exists` doesn't lock: its callers hold the lock ("To only use when lock is
    /// on"). This one takes the lock itself, so it must not be called while a [`CacheGuard`] of
    /// the same cache is held (a `std` mutex isn't reentrant); only the tests call it.
    ///
    /// Port of `GenericCache::exists` (Caching.h:64-69 @ v2.5.2).
    pub fn exists(&self, key: &K) -> bool {
        let state = self.state();
        !self.env_disable_all_caches && state.enabled && state.entries.contains_key(key)
    }

    /// The entries, locked, when the cache is enabled.
    ///
    /// Port of `GenericCache::lock` and `operator[]` (Caching.h:61-77 @ v2.5.2).
    pub fn lock(&self) -> Option<CacheGuard<'_, K, V>> {
        let guard = self.state();
        if self.env_disable_all_caches || !guard.enabled {
            return None;
        }
        Some(CacheGuard(guard))
    }
}

/// The locked entries of an enabled [`GenericCache`], in key order (upstream's `std::map`).
#[derive(Debug)]
pub struct CacheGuard<'a, K, V>(MutexGuard<'a, CacheState<K, V>>);

impl<K, V> CacheGuard<'_, K, V> {
    /// The entries.
    pub fn entries(&mut self) -> &mut BTreeMap<K, V> {
        &mut self.0.entries
    }
}

/// The cache of the processors a processor or a config makes, which `OCIO_DISABLE_ALL_CACHES` or
/// `OCIO_DISABLE_PROCESSOR_CACHES` disables.
///
/// Port of `ProcessorCache` (src/OpenColorIO/Caching.h:96-109 @ v2.5.2).
#[derive(Debug)]
pub struct ProcessorCache<K, V>(GenericCache<K, V>);

impl<K: Ord, V> Default for ProcessorCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord, V> ProcessorCache<K, V> {
    /// An enabled cache, unless `OCIO_DISABLE_ALL_CACHES` or `OCIO_DISABLE_PROCESSOR_CACHES` is
    /// set.
    ///
    /// Port of `ProcessorCache::ProcessorCache` (Caching.h:103-106 @ v2.5.2).
    pub fn new() -> Self {
        ProcessorCache(GenericCache::with_disable(is_env_present(
            OCIO_DISABLE_PROCESSOR_CACHES,
        )))
    }
}

impl<K, V> std::ops::Deref for ProcessorCache<K, V> {
    type Target = GenericCache<K, V>;

    fn deref(&self) -> &GenericCache<K, V> {
        &self.0
    }
}

#[cfg(test)]
#[path = "caching_tests.rs"]
mod tests;
