// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from minizip-ng 4.2.2 (see `mod.rs` for its notice): an altered version.

//! Port of minizip-ng 4.2.2's `test/test_path.cc` `symlink_target_base` (test/test_path.cc:
//! 69-112), verbatim: its seven cases, with `\` turned into `/` where `\` isn't a separator
//! (Linux), as the test does; `EXPECT_EQ(err, MZ_OK)` for the safe ones and `EXPECT_NE` for
//! the others. Plus the port's refusal where upstream writes past its buffer (U-730).

use super::*;

/// `symlink_base_tests` (test/test_path.cc:80-92): link path, target, base path, safe.
const SYMLINK_BASE_TESTS: [(&str, &str, &str, bool); 7] = [
    // In-bounds targets are allowed
    (r"base\link", "inside.txt", "base", true),
    (r"base\sub\link", r"..\inside.txt", "base", true),
    (r"base\link", r"..\base", "base", true),
    // Targets that resolve outside the base are rejected
    (r"base\link", r"..\out.txt", "base", false),
    (r"base\a\b\link", r"..\..\..\out.txt", "base", false),
    // Absolute targets are rejected
    (r"base\link", r"\etc\passwd", "base", false),
    // A sibling directory sharing the base name prefix is not in base
    (r"base\link", r"..\base_evil\x", "base", false),
];

/// Port of `TEST_P(symlink_target_base, os)` (test/test_path.cc:98-112) @ minizip-ng 4.2.2.
#[test]
fn symlink_target_base() {
    for (link_path, target, base_path, safe) in SYMLINK_BASE_TESTS {
        let convert = |s: &str| {
            if mz_os_is_dir_separator(b'\\') {
                s.to_owned()
            } else {
                s.replace('\\', "/")
            }
        };
        let (link_path, target, base_path) =
            (convert(link_path), convert(target), convert(base_path));
        let err = mz_path_is_symlink_target_safe(
            link_path.as_bytes(),
            target.as_bytes(),
            base_path.as_bytes(),
        );
        if safe {
            assert_eq!(err, MZ_OK, "{link_path} -> {target} in {base_path}");
        } else {
            assert_ne!(err, MZ_OK, "{link_path} -> {target} in {base_path}");
        }
    }
}

/// A link whose directory has 1,024 bytes or more, which upstream's `strncpy` copies past its
/// 1,024-byte buffer (undefined behaviour), is refused with `MZ_EXIST_ERROR`, the port's
/// deliberate error (docs/improvements.md, U-730), whatever its target.
#[test]
fn symlink_target_of_a_long_directory_is_refused() {
    for len in [1024, 1025, 4000] {
        let dir = format!("b/{}", "d".repeat(len - 2));
        assert_eq!(dir.len(), len);
        let link = format!("{dir}/link");
        assert_eq!(
            mz_path_is_symlink_target_safe(link.as_bytes(), b"inside.txt", b"b"),
            MZ_EXIST_ERROR,
            "{len}"
        );
    }
}
