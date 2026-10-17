// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's system functions that reading and writing an archive reach (`mz_os.c`,
//! `mz_os_posix.c`, `mz_os_win32.c`).

use ocio_ops::utils::string_utils::c_str;

use super::mz_strm::{MZ_EXIST_ERROR, MZ_OK};

/// Whether `path` matches `wildcard`, where a `*` in `wildcard` matches any run of bytes
/// (slashes included), `/` and `\` match each other, and with `ignore_case`, letters match
/// either case. Both end at their first NUL.
///
/// minizip-ng compares the bytes through the C runtime's `tolower` in the process's locale,
/// which Python sets at startup; the port folds `A`-`Z` only, whatever the locale (deviation
/// D-4, as [`ocio_ops::platform::strcasecmp`]).
///
/// Port of `mz_path_compare_wc` (mz_os.c:86-127 @ 4.0.10), which returns `MZ_OK` for a match
/// and `MZ_EXIST_ERROR` otherwise.
pub(crate) fn mz_path_compare_wc(path: &[u8], wildcard: &[u8], ignore_case: bool) -> bool {
    compare_wc(c_str(path), c_str(wildcard), ignore_case)
}

/// The byte at `i` of a C string, 0 past its end.
fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

fn compare_wc(path: &[u8], wildcard: &[u8], ignore_case: bool) -> bool {
    let (mut p, mut w) = (0usize, 0usize);
    while at(path, p) != 0 {
        match at(wildcard, w) {
            b'*' => {
                if at(wildcard, w + 1) == 0 {
                    return true;
                }

                while at(path, p) != 0 {
                    if compare_wc(&path[p..], &wildcard[w + 1..], ignore_case) {
                        return true;
                    }

                    p += 1;
                }

                return false;
            }
            c => {
                let pc = at(path, p);
                // Ignore differences in path slashes on platforms
                let slashes = (pc == b'\\' && c == b'/') || (pc == b'/' && c == b'\\');
                if !slashes {
                    if ignore_case {
                        if !pc.eq_ignore_ascii_case(&c) {
                            return false;
                        }
                    } else if pc != c {
                        return false;
                    }
                }
            }
        }

        p += 1;
        w += 1;
    }

    let c = at(wildcard, w);
    !(c != 0 && c != b'*')
}

/// `MZ_OK` when something exists at `path` (a C string), a directory included;
/// `MZ_EXIST_ERROR` otherwise.
///
/// Port of `mz_os_file_exists` (mz_os_posix.c:174-181, `stat`; mz_os_win32.c:176-193,
/// `GetFileAttributesW` of the UTF-16 conversion @ 4.0.10).
pub(crate) fn mz_os_file_exists(path: &[u8]) -> i32 {
    let path = c_str(path);
    #[cfg(windows)]
    let found = {
        use std::os::windows::ffi::OsStringExt;
        let wide = ocio_ops::platform::utf8_to_utf16_lossy(path);
        std::fs::metadata(std::ffi::OsString::from_wide(&wide)).is_ok()
    };
    #[cfg(not(windows))]
    let found = {
        use std::os::unix::ffi::OsStrExt;
        std::fs::metadata(std::ffi::OsStr::from_bytes(path)).is_ok()
    };
    if found { MZ_OK } else { MZ_EXIST_ERROR }
}

/// The path minizip-ng gives the system for `path` (a C string): its UTF-16 conversion on
/// Windows (`mz_os_unicode_string_create`, mz_os_win32.c), the bytes on Linux.
pub(crate) fn system_path(path: &[u8]) -> std::path::PathBuf {
    let path = c_str(path);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let wide = ocio_ops::platform::utf8_to_utf16_lossy(path);
        std::ffi::OsString::from_wide(&wide).into()
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(path).to_os_string().into()
    }
}

/// `GetFileAttributesW` of `path`: the attributes of the file, or of a link itself, or
/// `INVALID_FILE_ATTRIBUTES` (0xFFFFFFFF).
#[cfg(windows)]
fn file_attributes(path: &[u8]) -> u32 {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(system_path(path)).map_or(u32::MAX, |m| m.file_attributes())
}

/// `FILE_ATTRIBUTE_DIRECTORY` and `FILE_ATTRIBUTE_REPARSE_POINT`.
#[cfg(windows)]
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Whether `path` is a directory (following a link on Linux; a link to a directory is one on
/// Windows, where `GetFileAttributesW` reports the link's own attributes).
///
/// Port of `mz_os_is_dir` (mz_os_posix.c:304-313, mz_os_win32.c:441-460 @ 4.0.10).
pub(crate) fn mz_os_is_dir(path: &[u8]) -> bool {
    #[cfg(windows)]
    {
        let attribs = file_attributes(path);
        attribs != u32::MAX && attribs & FILE_ATTRIBUTE_DIRECTORY != 0
    }
    #[cfg(not(windows))]
    {
        std::fs::metadata(system_path(path)).is_ok_and(|m| m.is_dir())
    }
}

/// Whether `path` is a symbolic link (on Windows, any reparse point).
///
/// Port of `mz_os_is_symlink` (mz_os_posix.c:315-324, mz_os_win32.c:462-481 @ 4.0.10).
pub(crate) fn mz_os_is_symlink(path: &[u8]) -> bool {
    #[cfg(windows)]
    {
        let attribs = file_attributes(path);
        attribs != u32::MAX && attribs & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        std::fs::symlink_metadata(system_path(path)).is_ok_and(|m| m.file_type().is_symlink())
    }
}

/// The size of the file `path`: 0 for a directory or a file that can't be examined (on
/// Windows, one that can't be opened without sharing it).
///
/// Port of `mz_os_get_file_size` (mz_os_posix.c:183-196, mz_os_win32.c:195-219 @ 4.0.10).
pub(crate) fn mz_os_get_file_size(path: &[u8]) -> i64 {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // CreateFile2(path, GENERIC_READ, 0 (no sharing), OPEN_EXISTING, NULL)
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(system_path(path))
            .and_then(|f| f.metadata())
            .map_or(0, |m| m.len() as i64)
    }
    #[cfg(not(windows))]
    {
        match std::fs::metadata(system_path(path)) {
            Ok(m) if m.is_dir() => 0,
            Ok(m) => m.len() as i64,
            Err(_) => 0,
        }
    }
}

/// The modification, access and creation dates of the file `path` (`time_t`), or `None`
/// where they can't be read. Linux has no creation date (0).
///
/// Port of `mz_os_get_file_date` (mz_os_posix.c:198-228, mz_os_win32.c:236-259 @ 4.0.10), with
/// `mz_os_file_to_unix_time` (mz_os_win32.c:221-226): `GetFileAttributesExW`'s times.
pub(crate) fn mz_os_get_file_date(path: &[u8]) -> Option<(i64, i64, i64)> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let to_unix = |t: u64| (t.wrapping_sub(116_444_736_000_000_000) / 10_000_000) as i64;
        let m = std::fs::symlink_metadata(system_path(path)).ok()?;
        Some((
            to_unix(m.last_write_time()),
            to_unix(m.last_access_time()),
            to_unix(m.creation_time()),
        ))
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        // Not all systems allow stat'ing a file with / appended
        let mut name = c_str(path).to_vec();
        while name.last().is_some_and(|&c| c == b'/' || c == b'\\') {
            name.pop();
        }
        let m = std::fs::metadata(system_path(&name)).ok()?;
        // Creation date not supported
        Some((m.mtime(), m.atime(), 0))
    }
}

/// The attributes of the file `path`, as the system gives them: `GetFileAttributesW` on
/// Windows (`INVALID_FILE_ATTRIBUTES` where it fails), `lstat`'s `st_mode` on Linux (0 where it
/// fails).
///
/// Port of `mz_os_get_file_attribs` (mz_os_posix.c:243-252, mz_os_win32.c:301-318 @ 4.0.10).
pub(crate) fn mz_os_get_file_attribs(path: &[u8]) -> u32 {
    #[cfg(windows)]
    {
        file_attributes(path)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::symlink_metadata(system_path(path)).map_or(0, |m| m.mode())
    }
}

/// The names of a directory's entries, in the order the system lists them, or `None` where
/// it can't be opened. The names are UTF-8 on Windows (`WideCharToMultiByte`), the bytes on
/// Linux; `.` and `..`, which minizip-ng lists too, are left out (OpenColorIO skips them).
///
/// Port of `mz_os_open_dir`, `mz_os_read_dir` and `mz_os_close_dir` (mz_os_posix.c:274-290,
/// mz_os_win32.c:362-435 @ 4.0.10). On Windows `mz_os_open_dir` builds `FindFirstFileW`'s
/// pattern in a buffer of 320 bytes: a path that doesn't leave room for its separator and `*`
/// isn't listed here (`None`).
pub(crate) fn mz_os_list_dir(path: &[u8]) -> Option<Vec<Vec<u8>>> {
    let path = c_str(path);
    if cfg!(windows) && path.len() + 2 >= 319 {
        return None;
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(system_path(path)).ok()? {
        let Ok(entry) = entry else {
            break;
        };
        #[cfg(windows)]
        names.push(
            entry
                .file_name()
                .to_string_lossy()
                .into_owned()
                .into_bytes(),
        );
        #[cfg(not(windows))]
        {
            use std::os::unix::ffi::OsStrExt;
            names.push(entry.file_name().as_bytes().to_vec());
        }
    }
    Some(names)
}
