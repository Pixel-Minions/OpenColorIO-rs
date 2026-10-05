// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ports of `tests/cpu/PathUtils_tests.cpp` @ v2.5.2.

use super::*;

/// A custom compute hash function for testing: the hash of the name with a suffix, as text.
///
/// Port of `CustomComputeHash` (PathUtils_tests.cpp:16-21 @ v2.5.2), which streams
/// `std::hash<std::string>`; the test only compares its results with each other and with the
/// default function's, so Rust's `DefaultHasher` (fixed keys) stands in for it.
fn custom_compute_hash(filename: &[u8]) -> Vec<u8> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    [filename, b"this is custom hash".as_slice()]
        .concat()
        .hash(&mut hasher);
    hasher.finish().to_string().into_bytes()
}

/// Sets the custom hash function, and restores the default one when dropped.
///
/// Port of `ComputeHashGuard` (PathUtils_tests.cpp:26-47 @ v2.5.2).
struct ComputeHashGuard;

impl ComputeHashGuard {
    fn new() -> ComputeHashGuard {
        set_compute_hash_function(custom_compute_hash);
        ComputeHashGuard
    }
}

impl Drop for ComputeHashGuard {
    fn drop(&mut self) {
        reset_compute_hash_function();
    }
}

fn test_file(name: &str) -> Vec<u8> {
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    // Without the verbatim prefix that canonicalize gives on Windows, which would keep `/`.
    let dir = dir.to_string_lossy().into_owned();
    let mut path = dir
        .strip_prefix(r"\\?\")
        .unwrap_or(&dir)
        .as_bytes()
        .to_vec();
    path.push(b'/');
    path.extend_from_slice(name.as_bytes());
    path
}

/// Port of `OCIO_ADD_TEST(PathUtils, compute_hash)` @ v2.5.2.
#[test]
fn compute_hash() {
    let _lock = hash_function_lock();

    let file1 = test_file("lut1d_4.spi1d");
    let file2 = test_file("lut1d_5.spi1d");

    assert_eq!(hash_function()(&file1), hash_function()(&file1));

    assert_ne!(hash_function()(&file1), hash_function()(&file2));

    let result1 = hash_function()(&file1);
    let result2 = hash_function()(&file2);
    let result3;
    let result4;

    {
        let _tester = ComputeHashGuard::new();

        assert_ne!(hash_function()(&file1), result1);

        assert_ne!(hash_function()(&file2), result2);

        assert_eq!(hash_function()(&file1), hash_function()(&file1));

        result3 = hash_function()(&file1);

        result4 = hash_function()(&file2);
    }

    assert_ne!(result3, hash_function()(&file1));

    assert_ne!(result4, hash_function()(&file2));
}
