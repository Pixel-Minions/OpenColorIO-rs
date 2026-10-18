// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! The system calls of minizip-ng's that safe Rust can't make as minizip-ng makes them:
//! - `CreateSymbolicLinkW` with minizip-ng's own flags. `std`'s links pass
//!   `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`, which makes links where the wheel's call
//!   fails (an unelevated process in Windows' developer mode).
//! - `FindFirstFileW`, `FindNextFileW` and `FindClose` with minizip-ng's own pattern
//!   (`mz_os_open_dir`), which for a long path isn't the directory's listing: it matches the
//!   parent's entries by their long and their 8.3 short names, which `std` can't list.
//!   `GetLastError` tells how the listing ended.
//!
//! This module is the one exception to `unsafe` outside the SIMD modules, `ocio-py` and the
//! test kit's C runtime reference (owner decisions 2026-10-17: the link, then the listing;
//! `cargo xtask guards` names this file).
#![allow(unsafe_code)]

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

/// `SYMBOLIC_LINK_FLAG_DIRECTORY` (winbase.h).
const SYMBOLIC_LINK_FLAG_DIRECTORY: u32 = 0x1;

/// `WIN32_FIND_DATAW` (minwinbase.h): the attributes, the three `FILETIME`s (two `DWORD`s
/// each), the size, two reserved `DWORD`s, the long name and the 8.3 name.
#[repr(C)]
struct Win32FindDataW {
    file_attributes: u32,
    creation_time: [u32; 2],
    last_access_time: [u32; 2],
    last_write_time: [u32; 2],
    file_size_high: u32,
    file_size_low: u32,
    reserved0: u32,
    reserved1: u32,
    file_name: [u16; 260],
    alternate_file_name: [u16; 14],
}

/// `INVALID_HANDLE_VALUE` and `ERROR_NO_MORE_FILES` (winerror.h).
const INVALID_HANDLE_VALUE: isize = -1;
const ERROR_NO_MORE_FILES: u32 = 18;

#[link(name = "kernel32")]
unsafe extern "system" {
    /// `HANDLE FindFirstFileW(LPCWSTR lpFileName, LPWIN32_FIND_DATAW lpFindFileData)`.
    fn FindFirstFileW(file_name: *const u16, find_data: *mut Win32FindDataW) -> isize;
    /// `BOOL FindNextFileW(HANDLE hFindFile, LPWIN32_FIND_DATAW lpFindFileData)`.
    fn FindNextFileW(find_file: isize, find_data: *mut Win32FindDataW) -> i32;
    /// `BOOL FindClose(HANDLE hFindFile)`.
    fn FindClose(find_file: isize) -> i32;
    /// `DWORD GetLastError(void)`.
    fn GetLastError() -> u32;

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

/// What minizip-ng's listing of a pattern reads: each entry's long name (`cFileName`, UTF-16
/// without its NUL) in the order `FindFirstFileW` and `FindNextFileW` give them, and whether it
/// ended as `mz_os_read_dir` sets `end` (`ERROR_NO_MORE_FILES` after the last name). Where
/// `FindNextFileW` fails otherwise, `mz_os_read_dir` returns no entry for the name it was
/// reading, which is left out.
pub(crate) struct Listing {
    pub(crate) names: Vec<Vec<u16>>,
    pub(crate) ended: bool,
}

/// The listing of `pattern` (UTF-16, without its NUL) as minizip-ng reads it, or `None` where
/// `FindFirstFileW` fails (`mz_os_open_dir` returns no directory).
///
/// Port of the calls in `mz_os_open_dir`, `mz_os_read_dir` and `mz_os_close_dir`
/// (mz_os_win32.c:362-399, 401-421, 423-435 @ 4.0.10).
pub(crate) fn find_files(pattern: &[u16]) -> Option<Listing> {
    let pattern: Vec<u16> = pattern.iter().copied().chain(Some(0)).collect();
    let name = |data: &Win32FindDataW| {
        let len = data
            .file_name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(data.file_name.len());
        data.file_name[..len].to_vec()
    };
    let mut data = Win32FindDataW {
        file_attributes: 0,
        creation_time: [0; 2],
        last_access_time: [0; 2],
        last_write_time: [0; 2],
        file_size_high: 0,
        file_size_low: 0,
        reserved0: 0,
        reserved1: 0,
        file_name: [0; 260],
        alternate_file_name: [0; 14],
    };
    // SAFETY: `pattern` is a NUL-terminated UTF-16 string, and `data` a `WIN32_FIND_DATAW` the
    // function writes; both live until the call returns.
    let handle = unsafe { FindFirstFileW(pattern.as_ptr(), &mut data) };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut listing = Listing {
        names: Vec::new(),
        ended: false,
    };
    loop {
        let current = name(&data);
        // SAFETY: `handle` is the search `FindFirstFileW` opened and nothing closed yet, and
        // `data` a `WIN32_FIND_DATAW` the function writes.
        if unsafe { FindNextFileW(handle, &mut data) } == 0 {
            // SAFETY: a plain call that reads the thread's last error, which the failed
            // `FindNextFileW` just set.
            if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                listing.names.push(current);
                listing.ended = true;
            }
            break;
        }
        listing.names.push(current);
    }
    // SAFETY: `handle` is the search `FindFirstFileW` opened, closed once here.
    unsafe { FindClose(handle) };
    Some(listing)
}
