// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! The one system call of minizip-ng's that safe Rust can't make as minizip-ng makes it:
//! `CreateSymbolicLinkW` with minizip-ng's own flags. `std`'s links pass
//! `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`, which makes links where the wheel's call
//! fails (an unelevated process in Windows' developer mode).
//!
//! This module is the one exception to `unsafe` outside the SIMD modules, `ocio-py` and the
//! test kit's C runtime reference (owner decision 2026-10-17; `cargo xtask guards` names this
//! file).
#![allow(unsafe_code)]

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

/// `SYMBOLIC_LINK_FLAG_DIRECTORY` (winbase.h).
const SYMBOLIC_LINK_FLAG_DIRECTORY: u32 = 0x1;

#[link(name = "kernel32")]
unsafe extern "system" {
    /// `BOOLEAN CreateSymbolicLinkW(LPCWSTR lpSymlinkFileName, LPCWSTR lpTargetFileName,
    /// DWORD dwFlags)` (winbase.h): `BOOLEAN` is an `unsigned char`.
    fn CreateSymbolicLinkW(
        symlink_file_name: *const u16,
        target_file_name: *const u16,
        flags: u32,
    ) -> u8;
}

/// The NUL-terminated UTF-16 of `path`.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Makes `path` a symbolic link to `target`, a directory link with `directory`, as minizip-ng
/// calls `CreateSymbolicLinkW`: with `SYMBOLIC_LINK_FLAG_DIRECTORY` or no flag at all. Whether
/// the system made it.
///
/// Port of the call in `mz_os_make_symlink` (mz_os_win32.c:483-530 @ 4.0.10, the call at 516-520),
/// which looks `CreateSymbolicLinkW` up in kernel32 (every Windows the port runs on has it).
pub(crate) fn create_symbolic_link(path: &Path, target: &Path, directory: bool) -> bool {
    let (path, target) = (wide(path), wide(target));
    let flags = if directory {
        SYMBOLIC_LINK_FLAG_DIRECTORY
    } else {
        0
    };
    // SAFETY: `path` and `target` are NUL-terminated UTF-16 strings that live until the call
    // returns; the function only reads them, and takes the flags by value.
    unsafe { CreateSymbolicLinkW(path.as_ptr(), target.as_ptr(), flags) != 0 }
}
