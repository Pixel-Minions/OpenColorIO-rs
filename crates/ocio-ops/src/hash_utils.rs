// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Cache ID hashing: a port of `src/OpenColorIO/HashUtils.cpp` @ v2.6.0.

use std::fmt::Write as _;

/// Port of `CacheIDHash` (HashUtils.cpp:20-29 @ v2.6.0): XXH3-128 of `data`, printed as
/// zero-padded hex, the low 64 bits first and then the high 64 bits.
pub fn cache_id_hash(data: &[u8]) -> String {
    let hash = xxhash_rust::xxh3::xxh3_128(data);
    let low64 = hash as u64;
    let high64 = (hash >> 64) as u64;
    let mut out = String::with_capacity(32);
    let _ = write!(out, "{low64:016x}{high64:016x}");
    out
}

/// Port of `CacheIDHashUUID` (HashUtils.cpp:31-51 @ v2.6.0): XXH3-128 of `data` as 32
/// zero-padded hex digits, the high 64 bits first and then the low 64 bits (the reverse of
/// [`cache_id_hash`]), split 8-4-4-4-12 by hyphens as a UUID is.
pub fn cache_id_hash_uuid(data: &[u8]) -> String {
    let hash = xxhash_rust::xxh3::xxh3_128(data);
    let low64 = hash as u64;
    let high64 = (hash >> 64) as u64;
    let mut hex = String::with_capacity(32);
    let _ = write!(hex, "{high64:016x}{low64:016x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::cache_id_hash;
    use ocio_testkit::fixtures;

    /// The config cache ID is `CacheIDHash(serialize()) + ":" + CacheIDHash(file references)`
    /// (Config.cpp:5250-5310 @ v2.5.2); built-in configs reference no files.
    #[test]
    fn builtin_config_cache_ids() {
        let serialized = fixtures::list("builtin_configs/")
            .into_iter()
            .filter(|p| p.ends_with("/serialize.ocio"))
            .collect::<Vec<_>>();
        assert_eq!(serialized.len(), 10);
        for path in serialized {
            let text = fixtures::read(&path);
            let expected = fixtures::read_text(&path.replace("serialize.ocio", "cache_id.txt"));
            let actual = format!("{}:{}", cache_id_hash(&text), cache_id_hash(b""));
            assert_eq!(actual, expected, "{path}");
        }
    }
}
