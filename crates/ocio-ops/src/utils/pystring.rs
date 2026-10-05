// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pystring 1.1.4 (`upstream/pystring`, the library OpenColorIO 2.5.2 is built with): the string
//! functions OCIO and pystring's `os::path` call, and `os::path`'s `splitdrive`, `isabs`,
//! `abspath`, `join`, `split`, `basename`, `dirname`, `normpath` and `splitext`.
//!
//! pystring compiles `os::path` as `nt` where `WINDOWS` is defined (`_WIN32`, `_MSC_VER`, ...:
//! pystring.cpp:17-21) and as `posix` elsewhere, so the Windows wheel joins with `\` and the
//! Linux wheel with `/`. Both variants are ported (`*_nt`, `*_posix`); the unsuffixed functions
//! pick the one the platform's wheel compiles.
//!
//! Strings are bytes, as `std::string` holds them. Indices and counts are pystring's `int`s,
//! here `i64`: pystring's arithmetic on them never leaves `int`'s range for strings shorter
//! than 2^31 bytes, and the port computes with the true lengths of longer ones (pystring would
//! wrap them: docs/improvements.md, I-115).

/// `MAX_32BIT_INT` (pystring.h:30), the default `end` of the functions that take one: past any
/// string's end.
pub const MAX_32BIT_INT: i64 = 2_147_483_647;

/// pystring's `forward_slash`.
const FORWARD_SLASH: &[u8] = b"/";
/// pystring's `double_back_slash`, which is one backslash (pystring.cpp:34).
const BACK_SLASH: &[u8] = b"\\";
const DOT: &[u8] = b".";
const DOUBLE_DOT: &[u8] = b"..";
const COLON: &[u8] = b":";

/// `::isspace` in the "C" locale.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn len(s: &[u8]) -> i64 {
    i64::try_from(s.len()).expect("a length")
}

fn at(i: i64) -> usize {
    usize::try_from(i).expect("an index")
}

/// pystring's `ADJUST_INDICES` (pystring.cpp:42-53 @ v1.1.4): `start` and `end` as Python
/// slices read them, clamped to `[0, len]` at the ends that need it.
fn adjust_indices(start: i64, end: i64, len: i64) -> (i64, i64) {
    let mut end = end;
    if end > len {
        end = len;
    } else if end < 0 {
        end += len;
        if end < 0 {
            end = 0;
        }
    }
    let mut start = start;
    if start < 0 {
        start += len;
        if start < 0 {
            start = 0;
        }
    }
    (start, end)
}

/// `std::string::find(sub, pos)`.
fn std_find(s: &[u8], sub: &[u8], pos: usize) -> Option<usize> {
    if pos > s.len() || sub.len() > s.len() - pos {
        return None;
    }
    (pos..=s.len() - sub.len()).find(|&i| &s[i..i + sub.len()] == sub)
}

/// `std::string::rfind(sub, pos)`.
fn std_rfind(s: &[u8], sub: &[u8], pos: usize) -> Option<usize> {
    if sub.len() > s.len() {
        return None;
    }
    let last = (s.len() - sub.len()).min(pos);
    (0..=last).rev().find(|&i| &s[i..i + sub.len()] == sub)
}

/// The bytes of `s` from `start` to `end`, as Python's `s[start:end]`.
///
/// Port of `pystring::slice` (pystring.cpp:819-824 @ v1.1.4).
pub fn slice(s: &[u8], start: i64, end: i64) -> Vec<u8> {
    let (start, end) = adjust_indices(start, end, len(s));
    if start >= end {
        return Vec::new();
    }
    s[at(start)..at(end)].to_vec()
}

/// The first position of `sub` in `s[start:end]`, or -1.
///
/// Port of `pystring::find` (pystring.cpp:830-845 @ v1.1.4).
pub fn find(s: &[u8], sub: &[u8], start: i64, end: i64) -> i64 {
    let (start, end) = adjust_indices(start, end, len(s));
    match std_find(s, sub, at(start)) {
        Some(r) if len(&s[..r]) + len(sub) <= end => len(&s[..r]),
        _ => -1,
    }
}

/// The last position of `sub` in `s[start:end]`, or -1.
///
/// Port of `pystring::rfind` (pystring.cpp:858-870 @ v1.1.4).
pub fn rfind(s: &[u8], sub: &[u8], start: i64, end: i64) -> i64 {
    let (start, end) = adjust_indices(start, end, len(s));
    match std_rfind(s, sub, at(end)) {
        Some(r) if len(&s[..r]) >= start && len(&s[..r]) + len(sub) <= end => len(&s[..r]),
        _ => -1,
    }
}

/// Port of `_string_tailmatch` (pystring.cpp:390-425 @ v1.1.4): whether `s[start:end]` starts
/// (`direction` < 0) or ends with `sub`.
fn tailmatch(s: &[u8], sub: &[u8], start: i64, end: i64, direction: i32) -> bool {
    let slen = len(sub);
    let (mut start, end) = adjust_indices(start, end, len(s));
    if direction < 0 {
        // startswith
        if start + slen > len(s) {
            return false;
        }
    } else {
        // endswith
        if end - start < slen || start > len(s) {
            return false;
        }
        if end - slen > start {
            start = end - slen;
        }
    }
    if end - start >= slen {
        return &s[at(start)..at(start) + sub.len()] == sub;
    }
    false
}

/// Port of `pystring::startswith` (pystring.cpp:438-445 @ v1.1.4).
pub fn startswith(s: &[u8], prefix: &[u8], start: i64, end: i64) -> bool {
    tailmatch(s, prefix, start, end, -1)
}

/// Port of `pystring::endswith` (pystring.cpp:428-435 @ v1.1.4).
pub fn endswith(s: &[u8], suffix: &[u8], start: i64, end: i64) -> bool {
    tailmatch(s, suffix, start, end, 1)
}

/// Port of `split_whitespace` (pystring.cpp:72-100 @ v1.1.4).
fn split_whitespace(s: &[u8], maxsplit: i64) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    let mut maxsplit = maxsplit;
    let (mut i, mut j) = (0, 0);
    while i < s.len() {
        while i < s.len() && is_space(s[i]) {
            i += 1;
        }
        j = i;
        while i < s.len() && !is_space(s[i]) {
            i += 1;
        }
        if j < i {
            if maxsplit <= 0 {
                break;
            }
            maxsplit -= 1;
            result.push(s[j..i].to_vec());
            while i < s.len() && is_space(s[i]) {
                i += 1;
            }
            j = i;
        }
    }
    if j < s.len() {
        result.push(s[j..].to_vec());
    }
    result
}

/// `s` split on `sep` (on runs of white space when `sep` is empty), at most `maxsplit` times
/// (no limit when negative).
///
/// Port of `pystring::split` (pystring.cpp:144-177 @ v1.1.4).
pub fn split(s: &[u8], sep: &[u8], maxsplit: i64) -> Vec<Vec<u8>> {
    let mut maxsplit = if maxsplit < 0 {
        MAX_32BIT_INT
    } else {
        maxsplit
    };
    if sep.is_empty() {
        return split_whitespace(s, maxsplit);
    }
    let mut result = Vec::new();
    let (mut i, mut j, n) = (0usize, 0usize, sep.len());
    while i + n <= s.len() {
        if s[i] == sep[0] && &s[i..i + n] == sep {
            if maxsplit <= 0 {
                break;
            }
            maxsplit -= 1;
            result.push(s[j..i].to_vec());
            i += n;
            j = i;
        } else {
            i += 1;
        }
    }
    result.push(s[j..].to_vec());
    result
}

/// Which ends `do_strip` strips.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Strip {
    Left,
    Right,
    Both,
}

/// Port of `do_strip` (pystring.cpp:231-296 @ v1.1.4): white space when `chars` is empty.
fn do_strip(s: &[u8], striptype: Strip, chars: &[u8]) -> Vec<u8> {
    let strips = |c: u8| {
        if chars.is_empty() {
            is_space(c)
        } else {
            chars.contains(&c)
        }
    };
    let mut i = 0;
    if striptype != Strip::Right {
        while i < s.len() && strips(s[i]) {
            i += 1;
        }
    }
    let mut j = s.len();
    if striptype != Strip::Left {
        while j > i && strips(s[j - 1]) {
            j -= 1;
        }
    }
    s[i..j].to_vec()
}

/// Port of `pystring::strip` (pystring.cpp:343-346 @ v1.1.4).
pub fn strip(s: &[u8], chars: &[u8]) -> Vec<u8> {
    do_strip(s, Strip::Both, chars)
}

/// Port of `pystring::lstrip` (pystring.cpp:351-354 @ v1.1.4).
pub fn lstrip(s: &[u8], chars: &[u8]) -> Vec<u8> {
    do_strip(s, Strip::Left, chars)
}

/// Port of `pystring::rstrip` (pystring.cpp:359-362 @ v1.1.4).
pub fn rstrip(s: &[u8], chars: &[u8]) -> Vec<u8> {
    do_strip(s, Strip::Right, chars)
}

/// The strings of `seq` joined with `sep`.
///
/// Port of `pystring::join` (pystring.cpp:367-382 @ v1.1.4).
pub fn join(sep: &[u8], seq: &[Vec<u8>]) -> Vec<u8> {
    let Some((first, rest)) = seq.split_first() else {
        return Vec::new();
    };
    let mut result = first.clone();
    for item in rest {
        result.extend_from_slice(sep);
        result.extend_from_slice(item);
    }
    result
}

/// `s` with the first `count` occurrences of `old` replaced by `new` (all when `count` is
/// negative); an empty `old` matches before each byte and at the end.
///
/// Port of `pystring::replace` (pystring.cpp:952-987 @ v1.1.4).
pub fn replace(s: &[u8], old: &[u8], new: &[u8], count: i64) -> Vec<u8> {
    let mut sofar = 0;
    let mut s = s.to_vec();
    let mut cursor = find(&s, old, 0, MAX_32BIT_INT);
    while cursor != -1 && cursor <= len(&s) {
        if count > -1 && sofar >= count {
            break;
        }
        s.splice(at(cursor)..at(cursor) + old.len(), new.iter().copied());
        cursor += len(new);
        if !old.is_empty() {
            cursor = find(&s, old, cursor, MAX_32BIT_INT);
        } else {
            cursor += 1;
        }
        sofar += 1;
    }
    s
}

/// `s` repeated `n` times (empty for `n` <= 0).
///
/// Port of `pystring::mul` (pystring.cpp:1032-1043 @ v1.1.4).
pub fn mul(s: &[u8], n: i64) -> Vec<u8> {
    if n <= 0 {
        return Vec::new();
    }
    s.repeat(at(n))
}

/// pystring's `os::path`.
pub mod os_path {
    use super::*;

    /// Port of `splitdrive_nt` (pystring.cpp:1061-1075 @ v1.1.4): (drive, path).
    pub fn splitdrive_nt(p: &[u8]) -> (Vec<u8>, Vec<u8>) {
        if p.len() >= 2 && p[1] == b':' {
            (slice(p, 0, 2), slice(p, 2, MAX_32BIT_INT))
        } else {
            (Vec::new(), p.to_vec())
        }
    }

    /// Port of `splitdrive_posix` (pystring.cpp:1078-1083 @ v1.1.4).
    pub fn splitdrive_posix(p: &[u8]) -> (Vec<u8>, Vec<u8>) {
        (Vec::new(), p.to_vec())
    }

    /// Port of `isabs_nt` (pystring.cpp:1102-1108 @ v1.1.4).
    pub fn isabs_nt(path: &[u8]) -> bool {
        let (_, pathspec) = splitdrive_nt(path);
        match pathspec.first() {
            None => false,
            Some(&c) => c == b'/' || c == b'\\',
        }
    }

    /// Port of `isabs_posix` (pystring.cpp:1110-1113 @ v1.1.4).
    pub fn isabs_posix(s: &[u8]) -> bool {
        startswith(s, FORWARD_SLASH, 0, MAX_32BIT_INT)
    }

    /// Port of `abspath_nt` (pystring.cpp:1129-1134 @ v1.1.4).
    pub fn abspath_nt(path: &[u8], cwd: &[u8]) -> Vec<u8> {
        let mut p = path.to_vec();
        if !isabs_nt(&p) {
            p = join_nt(cwd, &p);
        }
        normpath_nt(&p)
    }

    /// Port of `abspath_posix` (pystring.cpp:1136-1141 @ v1.1.4).
    pub fn abspath_posix(path: &[u8], cwd: &[u8]) -> Vec<u8> {
        let mut p = path.to_vec();
        if !isabs_posix(&p) {
            p = join_posix(cwd, &p);
        }
        normpath_posix(&p)
    }

    /// Port of `join_nt(paths)` (pystring.cpp:1157-1244 @ v1.1.4).
    pub fn join_nt_all(paths: &[Vec<u8>]) -> Vec<u8> {
        let Some((first, rest)) = paths.split_first() else {
            return Vec::new();
        };
        let mut path = first.clone();
        for b in rest {
            let mut b_nts = false;
            if path.is_empty() {
                b_nts = true;
            } else if isabs_nt(b) {
                // This probably wipes out path so far.  However, it's more complicated if path
                // begins with a drive letter (pystring.cpp:1176-1184).
                if (path.len() >= 2 && path[1] != b':') || (b.len() >= 2 && b[1] == b':') {
                    // Path doesnt start with a drive letter
                    b_nts = true;
                } else if path.len() > 3
                    || (path.len() == 3
                        && !endswith(&path, FORWARD_SLASH, 0, MAX_32BIT_INT)
                        && !endswith(&path, BACK_SLASH, 0, MAX_32BIT_INT))
                {
                    // Else path has a drive letter, and b doesn't but is absolute.
                    b_nts = true;
                }
            }

            if b_nts {
                path = b.clone();
            } else if endswith(&path, FORWARD_SLASH, 0, MAX_32BIT_INT)
                || endswith(&path, BACK_SLASH, 0, MAX_32BIT_INT)
            {
                // Join, and ensure there's a separator.
                if startswith(b, FORWARD_SLASH, 0, MAX_32BIT_INT)
                    || startswith(b, BACK_SLASH, 0, MAX_32BIT_INT)
                {
                    path.extend(slice(b, 1, MAX_32BIT_INT));
                } else {
                    path.extend_from_slice(b);
                }
            } else if endswith(&path, COLON, 0, MAX_32BIT_INT) {
                path.extend_from_slice(b);
            } else if !b.is_empty() {
                if startswith(b, FORWARD_SLASH, 0, MAX_32BIT_INT)
                    || startswith(b, BACK_SLASH, 0, MAX_32BIT_INT)
                {
                    path.extend_from_slice(b);
                } else {
                    path.extend_from_slice(BACK_SLASH);
                    path.extend_from_slice(b);
                }
            } else {
                // path is not empty and does not end with a backslash, but b is empty.
                path.extend_from_slice(BACK_SLASH);
            }
        }
        path
    }

    /// Port of `join_nt(a, b)` (pystring.cpp:1247-1253 @ v1.1.4).
    pub fn join_nt(a: &[u8], b: &[u8]) -> Vec<u8> {
        join_nt_all(&[a.to_vec(), b.to_vec()])
    }

    /// Port of `join_posix(paths)` (pystring.cpp:1261-1285 @ v1.1.4).
    pub fn join_posix_all(paths: &[Vec<u8>]) -> Vec<u8> {
        let Some((first, rest)) = paths.split_first() else {
            return Vec::new();
        };
        let mut path = first.clone();
        for b in rest {
            if startswith(b, FORWARD_SLASH, 0, MAX_32BIT_INT) {
                path = b.clone();
            } else if path.is_empty() || endswith(&path, FORWARD_SLASH, 0, MAX_32BIT_INT) {
                path.extend_from_slice(b);
            } else {
                path.extend_from_slice(FORWARD_SLASH);
                path.extend_from_slice(b);
            }
        }
        path
    }

    /// Port of `join_posix(a, b)` (pystring.cpp:1288-1294 @ v1.1.4).
    pub fn join_posix(a: &[u8], b: &[u8]) -> Vec<u8> {
        join_posix_all(&[a.to_vec(), b.to_vec()])
    }

    /// Port of `split_nt` (pystring.cpp:1324-1356 @ v1.1.4): (head, tail).
    pub fn split_nt(path: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let (d, p) = splitdrive_nt(path);
        // set i to index beyond p's last slash
        let mut i = p.len();
        while i > 0 && p[i - 1] != b'\\' && p[i - 1] != b'/' {
            i -= 1;
        }
        let mut head = p[..i].to_vec();
        let tail = p[i..].to_vec();
        // remove trailing slashes from head, unless it's all slashes
        let mut head2 = head.clone();
        while matches!(head2.last(), Some(b'/' | b'\\')) {
            head2.pop();
        }
        if !head2.is_empty() {
            head = head2;
        }
        let mut out = d;
        out.extend(head);
        (out, tail)
    }

    /// Port of `split_posix` (pystring.cpp:1359-1370 @ v1.1.4): (head, tail).
    pub fn split_posix(p: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let i = rfind(p, FORWARD_SLASH, 0, MAX_32BIT_INT) + 1;
        let mut head = slice(p, 0, i);
        let tail = slice(p, i, MAX_32BIT_INT);
        if !head.is_empty() && head != mul(FORWARD_SLASH, len(&head)) {
            head = rstrip(&head, FORWARD_SLASH);
        }
        (head, tail)
    }

    /// Port of `basename_nt` (pystring.cpp:1386-1391 @ v1.1.4).
    pub fn basename_nt(path: &[u8]) -> Vec<u8> {
        split_nt(path).1
    }

    /// Port of `basename_posix` (pystring.cpp:1393-1398 @ v1.1.4).
    pub fn basename_posix(path: &[u8]) -> Vec<u8> {
        split_posix(path).1
    }

    /// Port of `dirname_nt` (pystring.cpp:1409-1414 @ v1.1.4).
    pub fn dirname_nt(path: &[u8]) -> Vec<u8> {
        split_nt(path).0
    }

    /// Port of `dirname_posix` (pystring.cpp:1416-1421 @ v1.1.4).
    pub fn dirname_posix(path: &[u8]) -> Vec<u8> {
        split_posix(path).0
    }

    /// Port of `normpath_nt` (pystring.cpp:1438-1519 @ v1.1.4).
    pub fn normpath_nt(p: &[u8]) -> Vec<u8> {
        let path = replace(p, FORWARD_SLASH, BACK_SLASH, -1);
        let (mut prefix, mut path) = splitdrive_nt(&path);

        if prefix.is_empty() {
            // No drive letter - preserve initial backslashes
            while slice(&path, 0, 1) == BACK_SLASH {
                prefix.extend_from_slice(BACK_SLASH);
                path = slice(&path, 1, MAX_32BIT_INT);
            }
        } else if startswith(&path, BACK_SLASH, 0, MAX_32BIT_INT) {
            // We have a drive letter - collapse initial backslashes
            prefix.extend_from_slice(BACK_SLASH);
            path = lstrip(&path, BACK_SLASH);
        }

        let mut comps = super::split(&path, BACK_SLASH, -1);
        let mut i = 0usize;
        while i < comps.len() {
            if comps[i].is_empty() || comps[i] == DOT {
                comps.remove(i);
            } else if comps[i] == DOUBLE_DOT {
                if i > 0 && comps[i - 1] != DOUBLE_DOT {
                    comps.drain(i - 1..i + 1);
                    i -= 1;
                } else if i == 0 && endswith(&prefix, BACK_SLASH, 0, MAX_32BIT_INT) {
                    comps.remove(i);
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        // If the path is now empty, substitute '.'
        if prefix.is_empty() && comps.is_empty() {
            comps.push(DOT.to_vec());
        }
        let mut out = prefix;
        out.extend(super::join(BACK_SLASH, &comps));
        out
    }

    /// Port of `normpath_posix` (pystring.cpp:1522-1563 @ v1.1.4).
    pub fn normpath_posix(p: &[u8]) -> Vec<u8> {
        if p.is_empty() {
            return DOT.to_vec();
        }
        let mut initial_slashes = i64::from(startswith(p, FORWARD_SLASH, 0, MAX_32BIT_INT));
        // POSIX allows one or two initial slashes, but treats three or more as single slash.
        if initial_slashes != 0
            && startswith(p, b"//", 0, MAX_32BIT_INT)
            && !startswith(p, b"///", 0, MAX_32BIT_INT)
        {
            initial_slashes = 2;
        }
        let comps = super::split(p, FORWARD_SLASH, -1);
        let mut new_comps: Vec<Vec<u8>> = Vec::new();
        for comp in comps {
            if comp.is_empty() || comp == DOT {
                continue;
            }
            if comp != DOUBLE_DOT
                || (initial_slashes == 0 && new_comps.is_empty())
                || new_comps.last().is_some_and(|last| last == DOUBLE_DOT)
            {
                new_comps.push(comp);
            } else if !new_comps.is_empty() {
                new_comps.pop();
            }
        }
        let mut path = super::join(FORWARD_SLASH, &new_comps);
        if initial_slashes > 0 {
            let mut prefixed = mul(FORWARD_SLASH, initial_slashes);
            prefixed.extend(path);
            path = prefixed;
        }
        if path.is_empty() {
            return DOT.to_vec();
        }
        path
    }

    /// Port of `splitext_generic` (pystring.cpp:1584-1617 @ v1.1.4): (root, ext).
    ///
    /// It compares `slice(p, filenameIndex)`, the rest of the path, with ".", where Python's
    /// `ntpath._splitext` compares the one character `p[filenameIndex]`; ported as it is
    /// (docs/improvements.md, I-110).
    fn splitext_generic(p: &[u8], sep: &[u8], altsep: &[u8], extsep: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut sep_index = rfind(p, sep, 0, MAX_32BIT_INT);
        if !altsep.is_empty() {
            let altsep_index = rfind(p, altsep, 0, MAX_32BIT_INT);
            sep_index = sep_index.max(altsep_index);
        }
        let dot_index = rfind(p, extsep, 0, MAX_32BIT_INT);
        if dot_index > sep_index {
            // Skip all leading dots
            let mut filename_index = sep_index + 1;
            while filename_index < dot_index {
                if slice(p, filename_index, MAX_32BIT_INT) != extsep {
                    return (slice(p, 0, dot_index), slice(p, dot_index, MAX_32BIT_INT));
                }
                filename_index += 1;
            }
        }
        (p.to_vec(), Vec::new())
    }

    /// Port of `splitext_nt` (pystring.cpp:1620-1624 @ v1.1.4).
    pub fn splitext_nt(path: &[u8]) -> (Vec<u8>, Vec<u8>) {
        splitext_generic(path, BACK_SLASH, FORWARD_SLASH, DOT)
    }

    /// Port of `splitext_posix` (pystring.cpp:1626-1630 @ v1.1.4).
    pub fn splitext_posix(path: &[u8]) -> (Vec<u8>, Vec<u8>) {
        splitext_generic(path, FORWARD_SLASH, b"", DOT)
    }

    /// Port of `isabs` (pystring.cpp:1115-1122 @ v1.1.4): the platform's variant.
    pub fn isabs(path: &[u8]) -> bool {
        if cfg!(windows) {
            isabs_nt(path)
        } else {
            isabs_posix(path)
        }
    }

    /// Port of `join(path1, path2)` (pystring.cpp:1296-1303 @ v1.1.4): the platform's variant.
    pub fn join(a: &[u8], b: &[u8]) -> Vec<u8> {
        if cfg!(windows) {
            join_nt(a, b)
        } else {
            join_posix(a, b)
        }
    }

    /// Port of `split` (pystring.cpp:1372-1379 @ v1.1.4): the platform's variant.
    pub fn split(path: &[u8]) -> (Vec<u8>, Vec<u8>) {
        if cfg!(windows) {
            split_nt(path)
        } else {
            split_posix(path)
        }
    }

    /// Port of `basename` (pystring.cpp:1400-1407 @ v1.1.4): the platform's variant.
    pub fn basename(path: &[u8]) -> Vec<u8> {
        if cfg!(windows) {
            basename_nt(path)
        } else {
            basename_posix(path)
        }
    }

    /// Port of `dirname` (pystring.cpp:1423-1430 @ v1.1.4): the platform's variant.
    pub fn dirname(path: &[u8]) -> Vec<u8> {
        if cfg!(windows) {
            dirname_nt(path)
        } else {
            dirname_posix(path)
        }
    }

    /// Port of `normpath` (pystring.cpp:1566-1573 @ v1.1.4): the platform's variant.
    pub fn normpath(path: &[u8]) -> Vec<u8> {
        if cfg!(windows) {
            normpath_nt(path)
        } else {
            normpath_posix(path)
        }
    }

    /// Port of `splitext` (pystring.cpp:1632-1639 @ v1.1.4): the platform's variant.
    pub fn splitext(path: &[u8]) -> (Vec<u8>, Vec<u8>) {
        if cfg!(windows) {
            splitext_nt(path)
        } else {
            splitext_posix(path)
        }
    }
}

#[cfg(test)]
#[path = "pystring_tests.rs"]
mod tests;
