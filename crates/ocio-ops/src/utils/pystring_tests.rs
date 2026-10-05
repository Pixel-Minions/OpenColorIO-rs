// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pystring 1.1.4's own tests of the functions the port has (upstream/pystring/test.cpp),
//! copied: the string functions, `abspath` and `os_path`, both variants.

use super::os_path::*;
use super::*;

/// A `split` case: the string, the separator, `maxsplit` and the parts.
type SplitCase = (&'static [u8], &'static [u8], i64, &'static [&'static [u8]]);

/// An `os.path` function that splits a path in two.
type SplitFn = fn(&[u8]) -> (Vec<u8>, Vec<u8>);

/// Port of `PYSTRING_ADD_TEST(pystring, endswith)` (upstream/pystring/test.cpp:12-34) @ pystring v1.1.4.
#[test]
fn pystring_endswith() {
    assert!(endswith(b"", b"", 0, MAX_32BIT_INT));
    assert!(!endswith(b"", b"a", 0, MAX_32BIT_INT));
    assert!(endswith(b"a", b"", 0, MAX_32BIT_INT));
    assert!(!endswith(b"", b".mesh", 0, MAX_32BIT_INT));
    assert!(!endswith(b"help", b".mesh", 0, MAX_32BIT_INT));
    assert!(!endswith(b"help", b".mesh", 0, MAX_32BIT_INT));
    assert!(!endswith(b"help", b".mesh", 1, MAX_32BIT_INT));
    assert!(!endswith(b"help", b".mesh", 1, 2));
    assert!(!endswith(b"help", b".mesh", 1, 1));
    assert!(!endswith(b"help", b".mesh", 1, -1));
    assert!(!endswith(b"help", b".mesh", -1, MAX_32BIT_INT));
    assert!(endswith(b".mesh", b".mesh", 0, MAX_32BIT_INT));
    assert!(endswith(b"a.mesh", b".mesh", 0, MAX_32BIT_INT));
    assert!(endswith(b"a.", b".", 0, MAX_32BIT_INT));
    assert!(endswith(b"abcdef", b"ef", 0, MAX_32BIT_INT));
    assert!(endswith(b"abcdef", b"cdef", 0, MAX_32BIT_INT));
    assert!(endswith(b"abcdef", b"cdef", 2, MAX_32BIT_INT));
    assert!(!endswith(b"abcdef", b"cdef", 3, MAX_32BIT_INT));
    assert!(!endswith(b"abcdef", b"cdef", 2, 3));
    assert!(endswith(b"abcdef", b"cdef", -10, MAX_32BIT_INT));
}

/// Port of `PYSTRING_ADD_TEST(pystring, find)` (upstream/pystring/test.cpp:36-67) @ pystring v1.1.4.
#[test]
fn pystring_find() {
    assert_eq!(find(b"", b"", 0, MAX_32BIT_INT), 0);
    assert_eq!(find(b"", b"a", 0, MAX_32BIT_INT), -1);
    assert_eq!(find(b"a", b"", 0, MAX_32BIT_INT), 0);
    assert_eq!(find(b"a", b"a", 0, MAX_32BIT_INT), 0);
    assert_eq!(find(b"abcdef", b"", 0, MAX_32BIT_INT), 0);
    assert_eq!(find(b"abcdef", b"", -1, MAX_32BIT_INT), 5);
    assert_eq!(find(b"abcdef", b"", -2, MAX_32BIT_INT), 4);
    assert_eq!(find(b"abcdef", b"", -5, MAX_32BIT_INT), 1);
    assert_eq!(find(b"abcdef", b"", -6, MAX_32BIT_INT), 0);
    assert_eq!(find(b"abcdef", b"", -7, MAX_32BIT_INT), 0);
    assert_eq!(find(b"abcdef", b"def", 0, MAX_32BIT_INT), 3);
    assert_eq!(find(b"abcdef", b"def", 3, MAX_32BIT_INT), 3);
    assert_eq!(find(b"abcdef", b"def", 4, MAX_32BIT_INT), -1);
    assert_eq!(find(b"abcdef", b"def", -5, MAX_32BIT_INT), 3);
    assert_eq!(find(b"abcdef", b"def", -1, MAX_32BIT_INT), -1);
    assert_eq!(find(b"abcabcabc", b"bc", -2, MAX_32BIT_INT), 7);
    assert_eq!(find(b"abcabcabc", b"bc", -1, MAX_32BIT_INT), -1);
    assert_eq!(find(b"abcabcabc", b"bc", 0, MAX_32BIT_INT), 1);
    assert_eq!(find(b"abcabcabc", b"bc", 1, MAX_32BIT_INT), 1);
    assert_eq!(find(b"abcabcabc", b"bc", 2, MAX_32BIT_INT), 4);
    assert_eq!(find(b"abcabcabc", b"bc", 4, MAX_32BIT_INT), 4);
    assert_eq!(find(b"abcabcabc", b"bc", 7, MAX_32BIT_INT), 7);
    assert_eq!(find(b"abcabcabc", b"bc", 4, 3), -1);
    assert_eq!(find(b"abcabcabc", b"bc", 4, 4), -1);
    assert_eq!(find(b"abcabcabc", b"bc", 4, 5), -1);
    assert_eq!(find(b"abcabcabc", b"bc", 4, -1), 4);
    assert_eq!(find(b"abcabcabc", b"bc", 4, 6), 4);
}

/// Port of `PYSTRING_ADD_TEST(pystring, rfind)` (upstream/pystring/test.cpp:69-99) @ pystring v1.1.4.
#[test]
fn pystring_rfind() {
    assert_eq!(rfind(b"", b"", 0, MAX_32BIT_INT), 0);
    assert_eq!(rfind(b"", b"a", 0, MAX_32BIT_INT), -1);
    assert_eq!(rfind(b"a", b"", 0, MAX_32BIT_INT), 1);
    assert_eq!(rfind(b"a", b"a", 0, MAX_32BIT_INT), 0);
    assert_eq!(rfind(b"abcdef", b"", 0, MAX_32BIT_INT), 6);
    assert_eq!(rfind(b"abcdef", b"", 0, 1), 1);
    assert_eq!(rfind(b"abcdef", b"", 0, 5), 5);
    assert_eq!(rfind(b"abcdef", b"", 0, -1), 5);
    assert_eq!(rfind(b"abcdef", b"", 0, -3), 3);
    assert_eq!(rfind(b"abcdef", b"def", 0, MAX_32BIT_INT), 3);
    assert_eq!(rfind(b"abcdef", b"def", 3, MAX_32BIT_INT), 3);
    assert_eq!(rfind(b"abcdef", b"def", 4, MAX_32BIT_INT), -1);
    assert_eq!(rfind(b"abcdef", b"def", -5, MAX_32BIT_INT), 3);
    assert_eq!(rfind(b"abcdef", b"def", -1, MAX_32BIT_INT), -1);
    assert_eq!(rfind(b"abcabcabc", b"bc", -2, MAX_32BIT_INT), 7);
    assert_eq!(rfind(b"abcabcabc", b"bc", -1, MAX_32BIT_INT), -1);
    assert_eq!(rfind(b"abcabcabc", b"bc", 0, MAX_32BIT_INT), 7);
    assert_eq!(rfind(b"abcabcabc", b"bc", 1, MAX_32BIT_INT), 7);
    assert_eq!(rfind(b"abcabcabc", b"bc", 4, MAX_32BIT_INT), 7);
    assert_eq!(rfind(b"abcabcabc", b"bc", 7, MAX_32BIT_INT), 7);
    assert_eq!(rfind(b"abcabcabc", b"bc", 4, -5), -1);
    assert_eq!(rfind(b"abcabcabc", b"bc", 4, -10), -1);
    assert_eq!(rfind(b"abcabcabc", b"bc", 4, 20), 7);
    assert_eq!(rfind(b"abcabcabc", b"abc", 6, 8), -1);
}

/// Port of `PYSTRING_ADD_TEST(pystring, replace)` (upstream/pystring/test.cpp:101-109) @ pystring v1.1.4.
#[test]
fn pystring_replace() {
    assert_eq!(replace(b"abcdef", b"foo", b"bar", -1), b"abcdef");
    assert_eq!(replace(b"abcdef", b"ab", b"cd", -1), b"cdcdef");
    assert_eq!(replace(b"abcdef", b"ab", b"", -1), b"cdef");
    assert_eq!(replace(b"abcabc", b"ab", b"", -1), b"cc");
    assert_eq!(replace(b"abcdef", b"", b"", -1), b"abcdef");
    assert_eq!(replace(b"abcdef", b"", b".", -1), b".a.b.c.d.e.f.");
}

/// Port of `PYSTRING_ADD_TEST(pystring, slice)` (upstream/pystring/test.cpp:111-137) @ pystring v1.1.4.
#[test]
fn pystring_slice() {
    assert_eq!(slice(b"", 0, MAX_32BIT_INT), b"");
    assert_eq!(slice(b"", 1, MAX_32BIT_INT), b"");
    assert_eq!(slice(b"", -1, MAX_32BIT_INT), b"");
    assert_eq!(slice(b"", -1, 2), b"");
    assert_eq!(slice(b"abcdef", 0, MAX_32BIT_INT), b"abcdef");
    assert_eq!(slice(b"abcdef", 0, MAX_32BIT_INT), b"abcdef");
    assert_eq!(slice(b"abcdef", 1, MAX_32BIT_INT), b"bcdef");
    assert_eq!(slice(b"abcdef", 2, MAX_32BIT_INT), b"cdef");
    assert_eq!(slice(b"abcdef", 2, 2), b"");
    assert_eq!(slice(b"abcdef", 2, 3), b"c");
    assert_eq!(slice(b"abcdef", 2, 1), b"");
    assert_eq!(slice(b"abcdef", 2, -1), b"cde");
    assert_eq!(slice(b"abcdef", 2, -2), b"cd");
    assert_eq!(slice(b"abcdef", 2, -3), b"c");
    assert_eq!(slice(b"abcdef", 2, -4), b"");
    assert_eq!(slice(b"abcdef", 2, -5), b"");
    assert_eq!(slice(b"abcdef", -1, MAX_32BIT_INT), b"f");
    assert_eq!(slice(b"abcdef", -2, MAX_32BIT_INT), b"ef");
    assert_eq!(slice(b"abcdef", -99, MAX_32BIT_INT), b"abcdef");
    assert_eq!(slice(b"abcdef", -99, -98), b"");
    assert_eq!(slice(b"abcdef", -2, 3), b"");
    assert_eq!(slice(b"abcdef", -2, 10), b"ef");
    assert_eq!(slice(b"abcdef", -1, MAX_32BIT_INT), b"f");
    assert_eq!(slice(b"abcdef", 0, -1), b"abcde");
}

/// Port of `PYSTRING_ADD_TEST(pystring, startswith)` (upstream/pystring/test.cpp:476-487) @ pystring v1.1.4.
#[test]
fn pystring_startswith() {
    assert!(startswith(b"", b"", 0, MAX_32BIT_INT));
    assert!(!startswith(b"", b"a", 0, MAX_32BIT_INT));
    assert!(startswith(b"a", b"", 0, MAX_32BIT_INT));
    assert!(startswith(b"abc", b"ab", 0, MAX_32BIT_INT));
    assert!(startswith(b"abc", b"abc", 0, MAX_32BIT_INT));
    assert!(!startswith(b"abc", b"abcd", 0, MAX_32BIT_INT));
    assert!(startswith(b"abcdef", b"abc", 0, MAX_32BIT_INT));
    assert!(!startswith(b"abcdef", b"abc", 1, MAX_32BIT_INT));
    assert!(startswith(b"abcdef", b"bc", 1, MAX_32BIT_INT));
}

/// Port of `PYSTRING_ADD_TEST(pystring, strip)` (upstream/pystring/test.cpp:489-498) @ pystring v1.1.4.
#[test]
fn pystring_strip() {
    assert_eq!(strip(b"", b""), b"");
    assert_eq!(strip(b"a", b""), b"a");
    assert_eq!(strip(b"a ", b""), b"a");
    assert_eq!(strip(b" a", b""), b"a");
    assert_eq!(strip(b"\n a ", b""), b"a");
    assert_eq!(strip(b"\r\n a \r\n", b""), b"a");
    assert_eq!(strip(b"\r\n a \r\n\t", b""), b"a");
}

/// Port of `PYSTRING_ADD_TEST(pystring, abspath)` (upstream/pystring/test.cpp:518-527) @ pystring v1.1.4.
#[test]
fn pystring_abspath() {
    assert_eq!(abspath_posix(b"", b"/net"), b"/net");
    assert_eq!(
        abspath_posix(b"../jeremys", b"/net/soft_scratch/users/stevel"),
        b"/net/soft_scratch/users/jeremys"
    );
    assert_eq!(
        abspath_posix(b"../../../../tmp/a", b"/net/soft_scratch/users/stevel"),
        b"/tmp/a"
    );
    assert_eq!(abspath_nt(b"", b"c:\\net"), b"c:\\net");
    assert_eq!(
        abspath_nt(b"..\\jeremys", b"c:\\net\\soft_scratch\\users\\stevel"),
        b"c:\\net\\soft_scratch\\users\\jeremys"
    );
    assert_eq!(
        abspath_nt(
            b"..\\..\\..\\..\\tmp\\a",
            b"c:\\net\\soft_scratch\\users\\stevel"
        ),
        b"c:\\tmp\\a"
    );
}

/// Port of `PYSTRING_ADD_TEST(pystring_os_path, isabs)` (upstream/pystring/test.cpp:539-553) @ pystring v1.1.4.
#[test]
fn os_path_isabs() {
    assert!(isabs_posix(b"/Users/test"));
    assert!(!isabs_posix(b"\\Users\\test"));
    assert!(!isabs_posix(b"../Users"));
    assert!(!isabs_posix(b"Users"));
    assert!(isabs_nt(b"C:\\Users\\test"));
    assert!(isabs_nt(b"C:/Users"));
    assert!(isabs_nt(b"/Users"));
    assert!(!isabs_nt(b"../Users"));
    assert!(!isabs_nt(b"..\\Users"));
}

/// Port of `PYSTRING_ADD_TEST(pystring_os_path, normpath)` (upstream/pystring/test.cpp:593-617) @ pystring v1.1.4.
#[test]
fn os_path_normpath() {
    assert_eq!(normpath_posix(b"A//B"), b"A/B");
    assert_eq!(normpath_posix(b"A/./B"), b"A/B");
    assert_eq!(normpath_posix(b"A/foo/../B"), b"A/B");
    assert_eq!(normpath_posix(b"/A//B"), b"/A/B");
    assert_eq!(normpath_posix(b"//A//B"), b"//A/B");
    assert_eq!(normpath_posix(b"///A//B"), b"/A/B");
    assert_eq!(normpath_posix(b"../A"), b"../A");
    assert_eq!(normpath_posix(b"../A../"), b"../A..");
    assert_eq!(normpath_posix(b"FOO/../A../././B"), b"A../B");
    assert_eq!(normpath_nt(b""), b".");
    assert_eq!(normpath_nt(b"A"), b"A");
    assert_eq!(normpath_nt(b"A./B"), b"A.\\B");
    assert_eq!(normpath_nt(b"C:\\"), b"C:\\");
    assert_eq!(normpath_nt(b"C:\\A"), b"C:\\A");
    assert_eq!(normpath_nt(b"C:/A"), b"C:\\A");
    assert_eq!(normpath_nt(b"C:/A..\\"), b"C:\\A..");
    assert_eq!(normpath_nt(b"C:/A..\\..\\"), b"C:\\");
    assert_eq!(normpath_nt(b"C:\\\\A"), b"C:\\A");
    assert_eq!(normpath_nt(b"C:\\\\\\A\\\\B"), b"C:\\A\\B");
}

/// Port of `PYSTRING_ADD_TEST(pystring, split)` (upstream/pystring/test.cpp:139-276) @ pystring v1.1.4: each case's size and parts.
#[test]
fn pystring_split() {
    let cases: &[SplitCase] = &[
        (b"", b"/", 1, &[b""]),
        (b"/", b"/", 1, &[b"", b""]),
        (b" ", b" ", 1, &[b"", b""]),
        (b" /", b"/", 1, &[b" ", b""]),
        (b" //", b"/", 1, &[b" ", b"/"]),
        (b"a  ", b" ", 1, &[b"a", b" "]),
        (b"//as//rew//gdf", b"//", -1, &[b"", b"as", b"rew", b"gdf"]),
        (b"/root", b"/", 1, &[b"", b"root"]),
        (b"/root/world", b"/", 0, &[b"/root/world"]),
        (b"/root/world", b"/", 1, &[b"", b"root/world"]),
        (b"/root/world", b"/", 2, &[b"", b"root", b"world"]),
        (b"/root/world", b"/", -1, &[b"", b"root", b"world"]),
    ];
    for (s, sep, maxsplit, parts) in cases {
        let result = super::split(s, sep, *maxsplit);
        assert_eq!(result, parts.to_vec(), "{s:?} {sep:?} {maxsplit}");
    }
}

/// Port of `PYSTRING_ADD_TEST(pystring_os_path, splitdrive)` (upstream/pystring/test.cpp:529-537)
/// @ pystring v1.1.4.
#[test]
fn os_path_splitdrive() {
    assert_eq!(
        splitdrive_posix(b"/Users/test"),
        (b"".to_vec(), b"/Users/test".to_vec())
    );
    assert_eq!(
        splitdrive_nt(b"C:\\Users\\test"),
        (b"C:".to_vec(), b"\\Users\\test".to_vec())
    );
    assert_eq!(
        splitdrive_nt(b"\\Users\\test"),
        (b"".to_vec(), b"\\Users\\test".to_vec())
    );
}

/// Port of `PYSTRING_ADD_TEST(pystring_os_path, join)` (upstream/pystring/test.cpp:555-591) @
/// pystring v1.1.4.
#[test]
fn os_path_join() {
    assert_eq!(join_posix(b"a", b"b"), b"a/b");
    assert_eq!(join_posix(b"/a", b"b"), b"/a/b");
    assert_eq!(join_posix(b"/a", b"/b"), b"/b");
    assert_eq!(join_posix(b"a", b"/b"), b"/b");
    assert_eq!(join_posix(b"//a", b"b"), b"//a/b");
    assert_eq!(join_posix(b"//a", b"b//"), b"//a/b//");
    assert_eq!(join_posix(b"../a", b"/b"), b"/b");
    assert_eq!(join_posix(b"../a", b"b"), b"../a/b");

    let mut paths: Vec<Vec<u8>> = Vec::new();
    assert_eq!(join_posix_all(&paths), b"");
    paths.push(b"/a".to_vec());
    assert_eq!(join_posix_all(&paths), b"/a");
    paths.push(b"b".to_vec());
    assert_eq!(join_posix_all(&paths), b"/a/b");
    paths.push(b"c/".to_vec());
    assert_eq!(join_posix_all(&paths), b"/a/b/c/");
    paths.push(b"d".to_vec());
    assert_eq!(join_posix_all(&paths), b"/a/b/c/d");
    paths.push(b"/e".to_vec());
    assert_eq!(join_posix_all(&paths), b"/e");

    assert_eq!(join_nt(b"c:", b"/a"), b"c:/a");
    assert_eq!(join_nt(b"c:/", b"/a"), b"c:/a");
    assert_eq!(join_nt(b"c:/a", b"/b"), b"/b");
    assert_eq!(join_nt(b"c:", b"d:/"), b"d:/");
    assert_eq!(join_nt(b"c:/", b"d:/"), b"d:/");
    assert_eq!(join_nt(b"a", b"b"), b"a\\b");
    assert_eq!(join_nt(b"\\a", b"b"), b"\\a\\b");
    assert_eq!(join_nt(b"c:\\a", b"b"), b"c:\\a\\b");
    assert_eq!(join_nt(b"c:\\a", b"c:\\b"), b"c:\\b");
    assert_eq!(join_nt(b"..\\a", b"b"), b"..\\a\\b");
}

/// Port of `PYSTRING_ADD_TEST(pystring_os_path, split)` (upstream/pystring/test.cpp:619-644) @
/// pystring v1.1.4.
#[test]
fn os_path_split() {
    let check = |f: SplitFn, path: &[u8], head: &[u8], tail: &[u8]| {
        assert_eq!(f(path), (head.to_vec(), tail.to_vec()), "{path:?}");
    };
    check(split_posix, b"", b"", b"");
    check(split_posix, b"/", b"/", b"");
    check(split_posix, b"a", b"", b"a");
    check(split_posix, b"a/", b"a", b"");
    check(split_posix, b"/a", b"/", b"a");
    check(split_posix, b"/a/b/", b"/a/b", b"");
    check(split_posix, b"/a/b", b"/a", b"b");
    check(split_posix, b"/a/b//", b"/a/b", b"");
    check(split_posix, b"/a/b/////////////", b"/a/b", b"");

    check(split_nt, b"", b"", b"");
    check(split_nt, b"\\", b"\\", b"");
    check(split_nt, b"a", b"", b"a");
    check(split_nt, b"a\\", b"a", b"");
    check(split_nt, b"c:\\a", b"c:\\", b"a");
    check(split_nt, b"c:\\a\\b", b"c:\\a", b"b");
    check(split_nt, b"c:\\a\\b\\", b"c:\\a\\b", b"");
    check(split_nt, b"D:\\dir\\\\", b"D:\\dir", b"");
}

/// Port of `PYSTRING_ADD_TEST(pystring_os_path, splitext)` (upstream/pystring/test.cpp:646-660)
/// @ pystring v1.1.4.
#[test]
fn os_path_splitext() {
    let check = |path: &[u8], root: &[u8], ext: &[u8]| {
        assert_eq!(splitext_nt(path), (root.to_vec(), ext.to_vec()), "{path:?}");
    };
    check(b"", b"", b"");
    check(b".", b".", b"");
    check(b".foo", b".foo", b"");
    check(b".foo.", b".foo", b".");
    check(b".foo.e", b".foo", b".e");
    check(b"c", b"c", b"");
    check(b"a_b.c", b"a_b", b".c");
    check(b"c:\\a.b.c", b"c:\\a.b", b".c");
    check(b"c:\\a_b.c", b"c:\\a_b", b".c");
}
