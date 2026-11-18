// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.2.2 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's system functions that reading and writing an archive reach (`mz_os.c`,
//! `mz_os_posix.c`, `mz_os_win32.c`).

use ocio_ops::utils::string_utils::c_str;

#[cfg(windows)]
use super::mz_strm::MZ_SYMLINK_ERROR;
use super::mz_strm::{MZ_EXIST_ERROR, MZ_INTERNAL_ERROR, MZ_OK, MZ_PARAM_ERROR};

/// Whether `path` matches `wildcard`, where a `*` in `wildcard` matches any run of bytes
/// (slashes included), `/` and `\` match each other, and with `ignore_case`, letters match
/// either case. Both end at their first NUL.
///
/// minizip-ng compares the bytes through the C runtime's `tolower` in the process's locale,
/// which Python sets at startup; the port folds `A`-`Z` only, whatever the locale (deviation
/// D-4, as [`ocio_ops::platform::strcasecmp`]).
///
/// Port of `mz_path_compare_wc` (mz_os.c:86-127 @ 4.2.2), which returns `MZ_OK` for a match
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
/// Port of `mz_os_file_exists` (mz_os_posix.c:184-191, `stat`; mz_os_win32.c:208-225,
/// `GetFileAttributesW` of the UTF-16 conversion @ 4.2.2).
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
/// Port of `mz_os_is_dir` (mz_os_posix.c:314-323, mz_os_win32.c:501-520 @ 4.2.2).
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
/// Port of `mz_os_is_symlink` (mz_os_posix.c:325-334, mz_os_win32.c:522-561 @ 4.2.2).
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
/// Port of `mz_os_get_file_size` (mz_os_posix.c:193-206, mz_os_win32.c:227-251 @ 4.2.2).
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
/// Port of `mz_os_get_file_date` (mz_os_posix.c:208-238, mz_os_win32.c:268-291 @ 4.2.2), with
/// `mz_os_file_to_unix_time` (mz_os_win32.c:253-258): `GetFileAttributesExW`'s times.
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
        // Not all systems allow stat'ing a file with / appended (mz_path_remove_slash)
        let mut name = c_str(path).to_vec();
        while name.last().is_some_and(|&c| mz_os_is_dir_separator(c)) {
            name.pop();
        }
        let m = std::fs::metadata(system_path(&name)).ok()?;
        // Creation date not supported
        Some((m.mtime(), m.atime(), 0))
    }
}

/// The attributes of the file `path`, following a link, as the system gives them: on Windows
/// `GetFileAttributesW`'s (`INVALID_FILE_ATTRIBUTES` where it fails), and for a reparse point
/// those of what it leads to, where it can be opened (`CreateFile2` with
/// `FILE_FLAG_BACKUP_SEMANTICS` and `GetFileInformationByHandle`, which `std::fs::metadata`
/// calls too); on Linux `stat`'s `st_mode` (0 where it fails).
///
/// Port of `mz_os_get_file_attribs` (mz_os_posix.c:253-262, mz_os_win32.c:333-378 @ 4.2.2).
/// OpenColorIO's writer never reaches the reparse point branch: it skips links first
/// ([`mz_os_is_symlink`], every reparse point on Windows). `mz_os_get_link_attribs`, the
/// attributes of a link itself (mz_os_posix.c:336-345, mz_os_win32.c:543-561), is called only
/// by a writer that stores links, which OpenColorIO's doesn't: it isn't ported.
pub(crate) fn mz_os_get_file_attribs(path: &[u8]) -> u32 {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let attributes = file_attributes(path);
        // If target is a reparse point, open with default flags to get attributes
        if attributes != u32::MAX
            && attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            && let Ok(m) = std::fs::metadata(system_path(path))
        {
            return m.file_attributes();
        }
        attributes
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(system_path(path)).map_or(0, |m| m.mode())
    }
}

/// The names of a directory's entries, in the order the system lists them, or `None` where
/// it can't be opened. The names are UTF-8 on Windows (`WideCharToMultiByte`), the bytes on
/// Linux; `.` and `..`, which minizip-ng lists too, are left out (OpenColorIO skips them).
///
/// Port of `mz_os_open_dir`, `mz_os_read_dir` and `mz_os_close_dir` (mz_os_posix.c:284-300,
/// mz_os_win32.c:422-495 @ 4.2.2). On Windows it lists as minizip-ng does, through
/// `FindFirstFileW` and `FindNextFileW` (`os_windows_ffi`, owner decision 2026-10-17):
/// - `mz_os_open_dir` builds the pattern in a buffer of 320 bytes: the path cut to 319 bytes
///   (`strncpy`), a separator where `mz_path_append_slash` has room (a path of 317 bytes or
///   fewer), and a `*` where `mz_path_combine` has room, converted from UTF-8 as
///   `MultiByteToWideChar` converts (a character the cut splits is U+FFFD). From 318 bytes the
///   pattern isn't the directory's listing: `path*` lists the parent's entries whose long or
///   8.3 short name starts with the directory's own name, and the cut path the one whose name
///   is the cut name (docs/improvements.md, I-221). OpenColorIO then looks for each name found
///   in the directory itself.
/// - `mz_os_read_dir` converts each name, `.` and `..` included, into the same `d_name` of 256
///   bytes, which the byte `end` follows (0, and 1 once the last entry is read), then the
///   structure's padding; `malloc` left all of it uninitialized. A name of more than 255
///   UTF-8 bytes doesn't fit with its NUL: `WideCharToMultiByte` writes the whole characters
///   that fit in the 256 bytes and fails, and the bytes after them stay as earlier entries
///   wrote them. OpenColorIO reads `d_name` as a C string: the cut, then those bytes up to a
///   NUL, or up to `end`'s 0 where the 256 bytes hold none (docs/improvements.md, I-222). Where
///   the read reaches a byte no entry wrote, or `end`'s 1 after the last entry and the padding,
///   the name read depends on memory nothing wrote, and the port gives the entry's name as
///   `Err` (docs/improvements.md, U-79).
pub(crate) fn mz_os_list_dir(path: &[u8]) -> Option<Vec<Result<Vec<u8>, Vec<u8>>>> {
    let path = c_str(path);
    #[cfg(windows)]
    {
        // fixed_path[320]: strncpy of 319 bytes, the separator, then the "*".
        let mut fixed_path = path[..path.len().min(319)].to_vec();
        let _ = mz_path_append_slash(&mut fixed_path, 320);
        mz_path_combine(&mut fixed_path, b"*", 320);
        let mut pattern = ocio_ops::platform::utf8_to_utf16_lossy(&fixed_path);
        if pattern.len() >= 260 {
            // Past MAX_PATH the wheel's call works because Python is a long-path-aware process
            // (and Windows allows long paths); this process may not be, so the port gives the
            // system the same path in its long form: made absolute as Windows normalizes it
            // (GetFullPathNameW), behind `\\?\`.
            use std::os::windows::ffi::{OsStrExt, OsStringExt};
            let absolute = std::path::absolute(std::ffi::OsString::from_wide(&pattern)).ok()?;
            let absolute: Vec<u16> = absolute.as_os_str().encode_wide().collect();
            let backslash = u16::from(b'\\');
            pattern = if absolute.starts_with(&[backslash, backslash, u16::from(b'?')]) {
                absolute
            } else if absolute.starts_with(&[backslash, backslash]) {
                r"\\?\UNC"
                    .encode_utf16()
                    .chain(absolute[1..].iter().copied())
                    .collect()
            } else {
                r"\\?\".encode_utf16().chain(absolute).collect()
            };
        }
        let listing = super::os_windows_ffi::find_files(&pattern)?;
        let last = listing.names.len().saturating_sub(1);
        // d_name across the listing: `None` where no entry has written yet.
        let mut d_name: [Option<u8>; 256] = [None; 256];
        let mut names = Vec::new();
        for (i, wide) in listing.names.iter().enumerate() {
            let name = ocio_ops::platform::utf16_to_utf8_lossy(wide);
            // WideCharToMultiByte: the name and its NUL where they fit, else the whole
            // characters that fit in the 256 bytes.
            let written = if name.len() < 256 {
                name.len() + 1
            } else {
                let s = String::from_utf8_lossy(&name).into_owned();
                let mut cut = 0;
                for (at, c) in s.char_indices() {
                    if at + c.len_utf8() > 256 {
                        break;
                    }
                    cut = at + c.len_utf8();
                }
                cut
            };
            for (byte, value) in d_name.iter_mut().zip(name.iter().chain(&[0])).take(written) {
                *byte = Some(*value);
            }
            // The C string OpenColorIO reads: up to d_name's first NUL, else up to `end`'s 0;
            // `None` where it reaches a byte nothing wrote, or `end`'s 1 and the padding.
            let mut read = Vec::new();
            let mut ended = false;
            for byte in d_name {
                match byte {
                    Some(0) => {
                        ended = true;
                        break;
                    }
                    Some(value) => read.push(value),
                    None => break,
                }
            }
            let at_end = listing.ended && i == last;
            let read = (ended || (read.len() == 256 && !at_end)).then_some(read);
            if read.as_deref() == Some(b".") || read.as_deref() == Some(b"..") {
                continue;
            }
            names.push(read.ok_or(name));
        }
        Some(names)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut names = Vec::new();
        for entry in std::fs::read_dir(system_path(path)).ok()? {
            let Ok(entry) = entry else {
                break;
            };
            names.push(Ok(entry.file_name().as_bytes().to_vec()));
        }
        Some(names)
    }
}

/// `MZ_PATH_SLASH_PLATFORM` (mz_os.h:38-44 @ 4.2.2).
const MZ_PATH_SLASH_PLATFORM: u8 = if cfg!(windows) { b'\\' } else { b'/' };

/// Whether `c` separates directories: `\` and `/` on Windows; on Linux only `/`, since
/// `mz_os_posix.c` defaults `MZ_PRESERVE_NATIVE_STRUCTURE` to 1 (mz_os_posix.c:41-43) and a
/// backslash is then part of a name.
///
/// Port of `mz_os_is_dir_separator` (mz_os_posix.c:302-312, mz_os_win32.c:497-499 @ 4.2.2).
pub(crate) fn mz_os_is_dir_separator(c: u8) -> bool {
    if cfg!(windows) {
        c == b'\\' || c == b'/'
    } else {
        c == b'/'
    }
}

/// Appends the platform's slash to `path` (a buffer of `max_path` bytes, its C string's NUL
/// included) unless it ends with a separator: `MZ_BUF_ERROR` where there's no room.
///
/// Port of `mz_path_append_slash` (mz_os.c:45-54 @ 4.2.2). An empty `path` reads before its
/// start upstream (`path[path_len - 1]`); its callers never pass one.
fn mz_path_append_slash(path: &mut Vec<u8>, max_path: usize) -> i32 {
    if path.len() + 2 >= max_path {
        return super::mz_strm::MZ_BUF_ERROR;
    }
    if path.last().is_some_and(|&c| !mz_os_is_dir_separator(c)) {
        path.push(MZ_PATH_SLASH_PLATFORM);
    }
    MZ_OK
}

/// Joins `join` to `path` (a buffer of `max_path` bytes) with the platform's slash, cutting
/// what doesn't fit.
///
/// Port of `mz_path_combine` (mz_os.c:24-43 @ 4.2.2).
pub(crate) fn mz_path_combine(path: &mut Vec<u8>, join: &[u8], max_path: usize) {
    let join = c_str(join);
    if path.is_empty() {
        path.extend_from_slice(&join[..join.len().min(max_path - 1)]);
    } else {
        let _ = mz_path_append_slash(path, max_path);
        if max_path > path.len() {
            let room = max_path - path.len() - 1;
            path.extend_from_slice(&join[..join.len().min(room)]);
        }
    }
}

/// `path` without `.` and `..` and repeated separators, as a buffer of `max_output` bytes
/// holds it (`output`, which keeps what earlier calls left in it): `..` doesn't go above the
/// start. An empty `path` is `MZ_INTERNAL_ERROR`.
///
/// Port of `mz_path_resolve` (mz_os.c:129-200 @ 4.2.2). A `..` at the start makes it read the
/// output's first byte, which neither this call nor an earlier one may have written (memory
/// `malloc` left uninitialized). Its value decides whether the result of a name that is only
/// `..`s is empty or a separator, which extracts the same way; the port takes it for one that
/// isn't a separator (docs/improvements.md, U-78).
pub(crate) fn mz_path_resolve(path: &[u8], output: &mut Vec<u8>, max_output: i32) -> i32 {
    let path = c_str(path);
    let at = |i: usize| path.get(i).copied().unwrap_or(0);
    let mut max_output = max_output;
    if max_output <= 0 {
        return MZ_PARAM_ERROR;
    }

    // What the buffer holds: the bytes this call writes from the start, over what earlier
    // calls left.
    let mut source = 0usize;
    let mut target = 0usize;

    while at(source) != 0 && max_output > 1 {
        let mut check = source;
        if mz_os_is_dir_separator(at(check)) {
            check += 1;
        }

        if source == 0 || target == 0 || check != source {
            // Skip double paths
            if mz_os_is_dir_separator(at(check)) {
                source += 1;
                continue;
            }
            if at(check) == b'.' {
                check += 1;

                // Remove . if at end of string and not at the beginning
                if at(check) == 0 && source != 0 && target != 0 {
                    // Copy last slash
                    write_at(output, target, at(source));
                    target += 1;
                    max_output -= 1;
                    source = check;
                    continue;
                }
                // Remove . if not at end of string
                else if mz_os_is_dir_separator(at(check)) {
                    source = check;
                    // Skip slash if at beginning of string
                    if target == 0 && at(source) != 0 {
                        source += 1;
                    }
                    continue;
                }
                // Go to parent directory ..
                else if at(check) == b'.' {
                    check += 1;
                    if at(check) == 0 || mz_os_is_dir_separator(at(check)) {
                        source = check;

                        // Search backwards for previous slash or the start of the output
                        // string
                        if target != 0 {
                            target -= 1;
                            loop {
                                // Every byte past the start was written by this call.
                                if target == 0 || mz_os_is_dir_separator(output[target]) {
                                    break;
                                }
                                target -= 1;
                                max_output += 1;
                                if target == 0 {
                                    break;
                                }
                            }
                        }

                        if target == 0 && at(source) != 0 {
                            source += 1;
                        }
                        // Upstream reads the byte before it tests the source; the byte matters
                        // only where the source ends. A byte nothing wrote yet is taken for
                        // one that isn't a separator: its value doesn't change the outcome
                        // (docs/improvements.md, U-78).
                        if at(source) == 0
                            && output
                                .get(target)
                                .is_some_and(|&c| mz_os_is_dir_separator(c))
                        {
                            target += 1;
                        }

                        write_at(output, target, 0);
                        continue;
                    }
                }
            }
        }

        write_at(output, target, at(source));
        source += 1;
        target += 1;
        max_output -= 1;
    }

    write_at(output, target, 0);

    if path.is_empty() {
        return MZ_INTERNAL_ERROR;
    }

    MZ_OK
}

/// Writes `byte` at `index` of the buffer `output`, growing it as needed.
fn write_at(output: &mut Vec<u8>, index: usize, byte: u8) {
    if output.len() <= index {
        output.resize(index + 1, 0);
    }
    output[index] = byte;
}

/// `path` without its last component (and the separator before it).
///
/// Port of `mz_path_remove_filename` (mz_os.c:202-221 @ 4.2.2).
pub(crate) fn mz_path_remove_filename(path: &mut Vec<u8>) {
    let mut i = path.len();
    while i > 1 {
        i -= 1;
        if mz_os_is_dir_separator(path[i]) {
            path.truncate(i);
            return;
        }
    }
    path.clear();
}

/// Makes the directory `path` (an existing one is fine).
///
/// Port of `mz_os_make_dir` (mz_os_posix.c:264-273, mode 0755; mz_os_win32.c:397-420, which
/// doesn't try to make a drive letter @ 4.2.2).
pub(crate) fn mz_os_make_dir(path: &[u8]) -> i32 {
    let path = c_str(path);
    if cfg!(windows) && !path.is_empty() && path.len() <= 3 && path.get(1) == Some(&b':') {
        return if mz_os_is_dir(path) {
            MZ_OK
        } else {
            MZ_EXIST_ERROR
        };
    }
    #[cfg(unix)]
    let builder = {
        let mut builder = std::fs::DirBuilder::new();
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o755);
        builder
    };
    #[cfg(not(unix))]
    let builder = std::fs::DirBuilder::new();
    match builder.create(system_path(path)) {
        Ok(()) => MZ_OK,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => MZ_OK,
        Err(_) => MZ_INTERNAL_ERROR,
    }
}

/// Whether the link `link_path`'s target `target` stays inside `base_path`: `MZ_OK`, or
/// `MZ_EXIST_ERROR` for a target that starts with a separator or resolves (lexically, with
/// [`mz_path_resolve`]) to a path outside `base_path`. The check is on the text: on Windows a
/// target with a drive letter (`C:\x`) is joined to the link's directory like a relative one
/// and passes (docs/improvements.md, I-952); `base_path` is compared as it is spelled.
///
/// Port of `mz_path_is_symlink_target_safe` (mz_os.c:280-341 @ 4.2.2). Upstream copies the
/// link's directory into a buffer of 1,024 bytes with `strncpy` and writes its NUL after it,
/// past the buffer when the directory has 1,024 bytes or more: the port returns
/// `MZ_EXIST_ERROR` there (docs/improvements.md, U-730).
pub(crate) fn mz_path_is_symlink_target_safe(
    link_path: &[u8],
    target: &[u8],
    base_path: &[u8],
) -> i32 {
    let (link_path, target, base_path) = (c_str(link_path), c_str(target), c_str(base_path));
    let max_path = 1024usize;

    // Absolute symlink targets are not allowed
    if target.first().is_some_and(|&c| mz_os_is_dir_separator(c)) {
        return MZ_EXIST_ERROR;
    }

    let mut base_len = base_path.len();

    // Remove trailing slash from base_path for comparison
    while base_len > 0 && mz_os_is_dir_separator(base_path[base_len - 1]) {
        base_len -= 1;
    }

    // Find parent directory length by scanning backwards past filename and trailing slashes
    let mut parent_len = link_path.len();
    while parent_len > 0 && !mz_os_is_dir_separator(link_path[parent_len - 1]) {
        parent_len -= 1;
    }
    while parent_len > 0 && mz_os_is_dir_separator(link_path[parent_len - 1]) {
        parent_len -= 1;
    }
    // strncpy and the NUL after it would write past `combined` (U-730).
    if parent_len >= max_path {
        return MZ_EXIST_ERROR;
    }

    // Combine parent + target
    let mut combined = Vec::new();
    if parent_len > 0 {
        combined.extend_from_slice(&link_path[..parent_len]);
        let _ = mz_path_append_slash(&mut combined, max_path);
    }
    let room = max_path - combined.len() - 1;
    combined.extend_from_slice(&target[..target.len().min(room)]);

    // Resolve the combined path to eliminate .. (into a buffer calloc zeroed)
    let mut resolved = Vec::new();
    if mz_path_resolve(&combined, &mut resolved, max_path as i32) != MZ_OK {
        return MZ_EXIST_ERROR;
    }
    let resolved = c_str(&resolved);

    // Check that resolved path stays within base_path
    if resolved.len() < base_len
        || resolved[..base_len] != base_path[..base_len]
        || (resolved.len() > base_len && !mz_os_is_dir_separator(resolved[base_len]))
    {
        return MZ_EXIST_ERROR;
    }
    MZ_OK
}

/// Whether a component of `path` below `base_path` is a link (on Windows, any reparse point)
/// whose target leads outside `base_path`: `MZ_EXIST_ERROR` then, or where such a link's
/// target can't be read; `MZ_OK` otherwise. Components that are `base_path` or one of its
/// parents, as spelled, aren't checked.
///
/// Port of `mz_dir_has_unsafe_symlink` (mz_os.c:343-420 @ 4.2.2). The buffer the targets are
/// read into is kept from one component to the next, as upstream's: where
/// [`mz_os_read_symlink`] leaves it as it was (a Windows reparse point that isn't a symbolic
/// link), the component is checked with the previous link's target, or an empty one.
pub(crate) fn mz_dir_has_unsafe_symlink(path: &[u8], base_path: &[u8]) -> i32 {
    let (path, base_path) = (c_str(path), c_str(base_path));
    if path.is_empty() {
        return MZ_PARAM_ERROR;
    }
    let max_path = 1024usize;
    let path_len = path.len();
    let mut base_len = base_path.len();

    // Remove trailing slash from base_path for comparison
    while base_len > 0 && mz_os_is_dir_separator(base_path[base_len - 1]) {
        base_len -= 1;
    }

    let mut symlink_target: Option<Vec<u8>> = None;
    let mut pos = 0usize;
    let mut err = MZ_OK;

    // Walk through each path component
    while err == MZ_OK && pos < path_len {
        // Copy separator if present
        if mz_os_is_dir_separator(path[pos]) {
            pos += 1;
        }
        // Copy next path component
        while pos < path_len && !mz_os_is_dir_separator(path[pos]) {
            pos += 1;
        }
        let check_path = &path[..pos];

        // Check if this existing path component is a symlink
        if !mz_os_is_symlink(check_path) {
            continue;
        }

        // Skip components at or above the base dir.
        let mut cmp_len = pos;
        if check_path
            .last()
            .is_some_and(|&c| mz_os_is_dir_separator(c))
        {
            cmp_len -= 1;
        }
        if cmp_len <= base_len
            && check_path[..cmp_len] == base_path[..cmp_len]
            // Verify that the prefix match is on a directory boundary.
            && (cmp_len == base_len || mz_os_is_dir_separator(base_path[cmp_len]))
        {
            continue;
        }

        // Allocate symlink target buffer on first use
        let target = symlink_target.get_or_insert_with(Vec::new);

        if mz_os_read_symlink(check_path, target, max_path) != MZ_OK {
            err = MZ_EXIST_ERROR;
            break;
        }

        // Reject the component if its symlink target escapes the base path
        err = mz_path_is_symlink_target_safe(check_path, target, base_path);
    }
    err
}

/// Makes the directory `path` and its parents.
///
/// Port of `mz_dir_make` (mz_os.c:422-457 @ 4.2.2).
pub(crate) fn mz_dir_make(path: &[u8]) -> i32 {
    let path = c_str(path);
    if path.is_empty() {
        return MZ_OK;
    }

    let mut current_dir = path.to_vec();
    // mz_path_remove_slash
    while current_dir
        .last()
        .is_some_and(|&c| mz_os_is_dir_separator(c))
    {
        current_dir.pop();
    }

    let mut err = mz_os_make_dir(&current_dir);
    if err != MZ_OK {
        let mut matched = 1;
        loop {
            while matched < current_dir.len() && !mz_os_is_dir_separator(current_dir[matched]) {
                matched += 1;
            }
            let hold = matched < current_dir.len();

            err = mz_os_make_dir(&current_dir[..matched.min(current_dir.len())]);
            if err != MZ_OK || !hold {
                break;
            }

            matched += 1;
        }
    }
    err
}

/// The `FILETIME` of `mz_os_unix_to_file_time` (mz_os_win32.c:261-266 @ 4.2.2):
/// 100-nanosecond intervals since 1601, in `uint64_t` arithmetic that wraps, so every `time_t`
/// gives one.
#[cfg(windows)]
fn mz_os_unix_to_file_time(unix_time: i64) -> u64 {
    (unix_time as u64)
        .wrapping_mul(10_000_000)
        .wrapping_add(116_444_736_000_000_000)
}

/// The system time of a `FILETIME` that is at most `0x7FFF_FFFF_FFFF_FFFF` (a `SystemTime`
/// holds its intervals since 1601 as a signed count).
#[cfg(windows)]
fn file_time_to_system_time(file_time: u64) -> Option<std::time::SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};
    let intervals = i128::from(file_time) - 116_444_736_000_000_000;
    let magnitude = intervals.unsigned_abs();
    let duration = Duration::new(
        (magnitude / 10_000_000) as u64,
        (magnitude % 10_000_000) as u32 * 100,
    );
    if intervals >= 0 {
        UNIX_EPOCH.checked_add(duration)
    } else {
        UNIX_EPOCH.checked_sub(duration)
    }
}

/// The system time of a `time_t` (Linux: every one is representable).
#[cfg(not(windows))]
fn time_t_to_system_time(unix_time: i64) -> Option<std::time::SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};
    if unix_time >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(unix_time as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(unix_time.unsigned_abs()))
    }
}

/// Sets the file's dates: on Linux, its access and modification times (`utime`); on Windows,
/// those of the three that aren't 0 (`SetFileTime` of the three `FILETIME`s, the others as
/// `GetFileTime` read them).
///
/// `SetFileTime` (probed 2026-10-17) leaves a time whose `FILETIME` is 0, `0xFFFF_FFFF_FFFF_FFFF`
/// or `0xFFFF_FFFF_FFFF_FFFE` as it is, and refuses one past `0x7FFF_FFFF_FFFF_FFFF`
/// otherwise: the call then fails as a whole, and none of the times is set. A `time_t` after
/// the year 30828 or before 1601 wraps to such a `FILETIME` (from an NTFS time of 0, say);
/// the three special values aren't reachable from a `time_t`.
///
/// Port of `mz_os_set_file_date` (mz_os_posix.c:240-251, mz_os_win32.c:293-331 @ 4.2.2).
pub(crate) fn mz_os_set_file_date(path: &[u8], modified: i64, accessed: i64, creation: i64) -> i32 {
    let Ok(file) = std::fs::OpenOptions::new()
        .write(true)
        .open(system_path(path))
    else {
        return if cfg!(windows) {
            MZ_OK
        } else {
            MZ_INTERNAL_ERROR
        };
    };
    let mut times = std::fs::FileTimes::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTimesExt;
        for (which, date) in [(0, modified), (1, accessed), (2, creation)] {
            if date == 0 {
                continue;
            }
            let file_time = mz_os_unix_to_file_time(date);
            if matches!(file_time, 0 | u64::MAX | 0xFFFF_FFFF_FFFF_FFFE) {
                continue;
            }
            let Some(time) =
                file_time_to_system_time(file_time).filter(|_| file_time <= i64::MAX as u64)
            else {
                // SetFileTime refuses it, and sets none of the three. The filter states the
                // limit itself: that std's SystemTime also overflows past 2^63 100-ns ticks
                // is an implementation detail of std.
                return MZ_INTERNAL_ERROR;
            };
            times = match which {
                0 => times.set_modified(time),
                1 => times.set_accessed(time),
                _ => times.set_created(time),
            };
        }
    }
    #[cfg(not(windows))]
    {
        // Creation date not supported
        let _ = creation;
        let (Some(m), Some(a)) = (
            time_t_to_system_time(modified),
            time_t_to_system_time(accessed),
        ) else {
            return MZ_INTERNAL_ERROR;
        };
        times = times.set_modified(m).set_accessed(a);
    }
    if file.set_times(times).is_err() {
        return MZ_INTERNAL_ERROR;
    }
    MZ_OK
}

/// Sets the file's attributes: its mode on Linux (`chmod`); on Windows, where the port has no
/// `SetFileAttributesW`, only the read-only attribute (deviation D-6).
///
/// Port of `mz_os_set_file_attribs` (mz_os_posix.c:264-272, mz_os_win32.c:380-395 @ 4.2.2).
pub(crate) fn mz_os_set_file_attribs(path: &[u8], attributes: u32) -> i32 {
    let path = system_path(path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::set_permissions(&path, std::fs::Permissions::from_mode(attributes)).is_err() {
            return MZ_INTERNAL_ERROR;
        }
    }
    #[cfg(windows)]
    {
        let Ok(metadata) = std::fs::metadata(&path) else {
            return MZ_INTERNAL_ERROR;
        };
        let mut permissions = metadata.permissions();
        permissions.set_readonly(attributes & 0x01 != 0);
        if std::fs::set_permissions(&path, permissions).is_err() {
            return MZ_INTERNAL_ERROR;
        }
    }
    MZ_OK
}

/// Makes `path` a symbolic link to `target_path`: `MZ_INTERNAL_ERROR` (Linux) or
/// `MZ_SYMLINK_ERROR` (Windows) where the system refuses.
///
/// Port of `mz_os_make_symlink` (mz_os_posix.c:347-355, mz_os_win32.c:563-610 @ 4.2.2). On
/// Windows it calls `CreateSymbolicLinkW` as upstream does (`os_windows_ffi`): a target that
/// ends with a separator (`mz_path_has_slash`, mz_os.c:69-74; an empty one too) makes a
/// directory link (`SYMBOLIC_LINK_FLAG_DIRECTORY`), and without
/// `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE` only a process with the privilege (an
/// elevated one) makes links.
pub(crate) fn mz_os_make_symlink(path: &[u8], target_path: &[u8]) -> i32 {
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(system_path(target_path), system_path(path)).is_err() {
            return MZ_INTERNAL_ERROR;
        }
        MZ_OK
    }
    #[cfg(windows)]
    {
        let target = c_str(target_path);
        let directory = target.last().is_none_or(|&c| mz_os_is_dir_separator(c));
        if !super::os_windows_ffi::create_symbolic_link(
            &system_path(path),
            &system_path(target),
            directory,
        ) {
            return MZ_SYMLINK_ERROR;
        }
        MZ_OK
    }
}

/// The target of the link `path` into `target_path`, a buffer of `max_target_path` bytes:
/// `MZ_OK`, or an error where it can't be read or (Linux) fills the buffer.
///
/// Linux: port of `mz_os_read_symlink` (mz_os_posix.c:357-372 @ 4.2.2): `readlink`
/// (`std::fs::read_link`, which calls it) of at most `max_target_path - 1` bytes; a target of
/// that length or more is `MZ_BUF_ERROR`, where upstream has written its first bytes without
/// a NUL (the callers then drop the buffer).
///
/// Windows: port of `mz_os_read_symlink` (mz_os_win32.c:612-714 @ 4.2.2), in safe Rust
/// (owner decision 2026-11-18, deviation D-22). Upstream reads the reparse point
/// (`FSCTL_GET_REPARSE_POINT`); for a symbolic link (`IO_REPARSE_TAG_SYMLINK`) it takes the
/// print name, at most `max_target_path` UTF-16 units, in UTF-8 (`WideCharToMultiByte`, a
/// lone surrogate as U+FFFD), cut to `max_target_path - 1` bytes, with a separator appended
/// when the target is a directory (`mz_os_is_dir` of the target as written, relative to the
/// process's directory); for any other reparse point (a junction, ...) it returns `MZ_OK`
/// and leaves the buffer as it was. The port takes the tag from `FindFirstFileW`
/// (`dwReserved0`) and the target from `std::fs::read_link`, which reads the substitute name
/// (D-22: the same text for every link `CreateSymbolicLinkW` makes).
pub(crate) fn mz_os_read_symlink(
    path: &[u8],
    target_path: &mut Vec<u8>,
    max_target_path: usize,
) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(target) = std::fs::read_link(system_path(path)) else {
            return MZ_EXIST_ERROR;
        };
        let target = target.as_os_str().as_bytes();
        if target.len() >= max_target_path - 1 {
            return super::mz_strm::MZ_BUF_ERROR;
        }
        target_path.clear();
        target_path.extend_from_slice(target);
        MZ_OK
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let system = system_path(path);
        let wide: Vec<u16> = system.as_os_str().encode_wide().collect();
        // The file can't be opened, or isn't a reparse point (DeviceIoControl fails).
        let Some(tag) = super::os_windows_ffi::reparse_tag(&wide) else {
            return MZ_INTERNAL_ERROR;
        };
        if tag != IO_REPARSE_TAG_SYMLINK {
            return MZ_OK;
        }
        let Ok(target) = std::fs::read_link(&system) else {
            return MZ_INTERNAL_ERROR;
        };
        let mut target: Vec<u16> = target.as_os_str().encode_wide().collect();
        target.truncate(max_target_path);
        let utf8 = ocio_ops::platform::utf16_to_utf8_lossy(&target);
        target_path.clear();
        target_path.extend_from_slice(&utf8[..utf8.len().min(max_target_path - 1)]);
        // Ensure directories have slash at the end so we can recreate them later
        if mz_os_is_dir(&utf8) {
            let _ = mz_path_append_slash(target_path, max_target_path);
        }
        MZ_OK
    }
}

/// `IO_REPARSE_TAG_SYMLINK` (winnt.h).
#[cfg(windows)]
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;

#[cfg(test)]
#[path = "mz_os_tests.rs"]
mod tests;
