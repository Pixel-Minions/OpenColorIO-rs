// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/utils/StringUtils_tests.cpp` @ v2.5.2. C++ string literals are byte
//! strings, so they become `b"..."` here.

use super::*;

/// A `StringVec` from literals.
fn string_vec(list: &[&str]) -> StringVec {
    list.iter().map(|s| s.as_bytes().to_vec()).collect()
}

/// Port of `OCIO_ADD_TEST(StringUtils, cases)` @ v2.5.2.
#[test]
fn cases() {
    let reference = b"lOwEr 1*& ctfG";

    assert_eq!(lower(reference), b"lower 1*& ctfg");
    assert_eq!(upper(reference), b"LOWER 1*& CTFG");
    assert_eq!(lower_c_str(None), b"");
    assert_eq!(upper_c_str(None), b"");
}

/// Port of `OCIO_ADD_TEST(StringUtils, trim)` @ v2.5.2.
#[test]
fn trim_() {
    let reference = b" \t\n lOwEr 1*& ctfG \n\n ";

    assert_eq!(left_trim(reference), b"lOwEr 1*& ctfG \n\n ");
    assert_eq!(right_trim(reference), b" \t\n lOwEr 1*& ctfG");
    assert_eq!(trim(reference), b"lOwEr 1*& ctfG");

    // Test that no assert happens when the Trim argument is not an unsigned char (see issue
    // #1874).
    let reference2 = [0xffu8, 0xfe, 0xfd];
    let _ = trim(&reference2);
}

/// Port of `OCIO_ADD_TEST(StringUtils, split)` @ v2.5.2.
#[test]
fn split_() {
    let reference = b" \t\n lOwEr 1*& ctfG \n\n ";

    {
        let results = split(reference, b'O');
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], b" \t\n l");
        assert_eq!(results[1], b"wEr 1*& ctfG \n\n ");
    }

    // Test to validate the former pystring::split() behavior.
    {
        let results = split(b"", b',');
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], b"");
    }

    // Test to validate the former pystring::split() behavior.
    {
        let results = split(b",", b',');
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], b"");
        assert_eq!(results[1], b"");
    }

    {
        let results = split_by_lines(reference);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], b" \t");
        assert_eq!(results[1], b" lOwEr 1*& ctfG ");
        assert_eq!(results[2], b"");
        assert_eq!(results[3], b" ");
    }

    {
        let results = split_by_lines(b"\n");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], b"");
    }

    // Test to validate the former pystring::splitlines() behavior.
    {
        let results = split_by_lines(b"");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], b"");
    }

    // Something important to notice and preserve.
    {
        // Note: StringUtils::Split() is mainly used to parse some string content enumerating
        // a list of substrings (i.e. separator could be a space, comma, etc). In that use
        // case, a string like ",," must return three entries. Refer to 'looks' parsing for
        // example. However, StringUtils::SplitByLines() is mainly used to read some file
        // content where "xx\n" only means one string equal to "xx".
        let content = b"\n";
        let res1 = split(content, b'\n');
        let res2 = split_by_lines(content);

        assert_eq!(res1.len(), 2);
        assert_eq!(res2.len(), 1);
    }
}

/// Port of `OCIO_ADD_TEST(StringUtils, searches)` @ v2.5.2.
#[test]
fn searches() {
    let reference = b"lOwEr 1*& ctfG";

    assert!(starts_with(reference, b"lOwEr"));
    assert!(!starts_with(reference, b"wEr"));
    assert!(!starts_with(reference, b"LOwEr"));

    assert!(ends_with(reference, b"ctfG"));
    assert!(!ends_with(reference, b"ctf"));
    assert!(!ends_with(reference, b"CtfG"));
}

/// Port of `OCIO_ADD_TEST(StringUtils, replace)` @ v2.5.2.
#[test]
fn replace_() {
    let mut reference = b"lOwEr 1*& ctfG".to_vec();

    reference = replace(&reference, b"wEr", b"12345");
    assert_eq!(reference, b"lO12345 1*& ctfG");

    reference = replace(&reference, b"345 1*", b"ABC");
    assert_eq!(reference, b"lO12ABC& ctfG");

    // Test a not existing subbstring.
    reference = replace(&reference, b"ZY", b"TO");
    assert_eq!(reference, b"lO12ABC& ctfG");

    assert!(replace_in_place(&mut reference, b"ct", b"TO"));
    assert_eq!(reference, b"lO12ABC& TOfG");

    assert!(!replace_in_place(&mut reference, b"12345", b"TO"));
    assert_eq!(reference, b"lO12ABC& TOfG");
}

/// Port of `OCIO_ADD_TEST(StringUtils, split_whitespaces)` @ v2.5.2.
#[test]
fn split_whitespaces() {
    let reference = b"10.0 9. 1 er\t1e-5f";

    let res1 = split_by_white_spaces(reference);
    assert_eq!(res1.len(), 5);
    assert_eq!(res1[0], b"10.0");
    assert_eq!(res1[1], b"9.");
    assert_eq!(res1[2], b"1");
    assert_eq!(res1[3], b"er");
    assert_eq!(res1[4], b"1e-5f");
}

/// Port of `OCIO_ADD_TEST(StringUtils, find)` @ v2.5.2.
#[test]
fn find_() {
    let reference = b"10.0 9. 1 er\t1e-5f";

    assert_eq!(Some(0), find(reference, b"1"));
    assert_eq!(Some(12), find(reference, b"\t"));

    assert_eq!(None, find(reference, b"TO"));
    assert_eq!(None, find(reference, b"9.1"));

    assert_eq!(Some(13), reverse_find(reference, b"1"));
    assert_eq!(Some(17), reverse_find(reference, b"f"));

    assert_eq!(None, reverse_find(reference, b"TO"));
}

/// Port of `OCIO_ADD_TEST(StringUtils, remove_contain)` @ v2.5.2.
#[test]
fn remove_contain() {
    let reference = b"1,\t2, 3, 4,5,      6";

    let mut res = split(reference, b',');

    {
        assert_eq!(res.len(), 6);
        trim_vec(&mut res);
        let values = string_vec(&["1", "2", "3", "4", "5", "6"]);
        assert!(res == values);

        let s = join(&res, b',');
        assert_eq!(s, b"1, 2, 3, 4, 5, 6");
    }

    {
        assert!(contain(&res, b"3"));
        assert!(contain(&res, b"6"));

        assert!(!contain(&res, b"9"));

        assert!(remove(&mut res, b"3"));
        assert_eq!(res.len(), 5);
        assert!(!contain(&res, b"3"));
    }
    {
        // Validate that Contain requires a full-match, not just a partial match.
        let values = string_vec(&["2 ", " 2 ", " 2", "2,", "2\n"]);
        assert!(contain(&values, b" 2 "));
        assert!(!contain(&values, b"2"));
    }
}
