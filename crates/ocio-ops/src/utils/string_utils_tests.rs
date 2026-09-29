// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/utils/StringUtils_tests.cpp` @ v2.5.2.

use super::*;

/// Port of `OCIO_ADD_TEST(StringUtils, cases)` @ v2.5.2.
#[test]
fn cases() {
    let reference = "lOwEr 1*& ctfG";

    assert_eq!(lower(reference), "lower 1*& ctfg");
    assert_eq!(upper(reference), "LOWER 1*& CTFG");
    assert_eq!(lower_c_str(None), "");
    assert_eq!(upper_c_str(None), "");
}

/// Port of `OCIO_ADD_TEST(StringUtils, trim)` @ v2.5.2.
#[test]
fn trim_() {
    let reference = " \t\n lOwEr 1*& ctfG \n\n ";

    assert_eq!(left_trim(reference), "lOwEr 1*& ctfG \n\n ");
    assert_eq!(right_trim(reference), " \t\n lOwEr 1*& ctfG");
    assert_eq!(trim(reference), "lOwEr 1*& ctfG");

    // Test that no assert happens when the Trim argument is not an unsigned char (see issue
    // #1874).
    let reference2 = [0xffu8, 0xfe, 0xfd];
    let _ = trim_bytes(&reference2);
}

/// Port of `OCIO_ADD_TEST(StringUtils, split)` @ v2.5.2.
#[test]
fn split_() {
    let reference = " \t\n lOwEr 1*& ctfG \n\n ";

    {
        let results = split(reference, b'O');
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], " \t\n l");
        assert_eq!(results[1], "wEr 1*& ctfG \n\n ");
    }

    // Test to validate the former pystring::split() behavior.
    {
        let results = split("", b',');
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "");
    }

    // Test to validate the former pystring::split() behavior.
    {
        let results = split(",", b',');
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "");
        assert_eq!(results[1], "");
    }

    {
        let results = split_by_lines(reference);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], " \t");
        assert_eq!(results[1], " lOwEr 1*& ctfG ");
        assert_eq!(results[2], "");
        assert_eq!(results[3], " ");
    }

    {
        let results = split_by_lines("\n");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "");
    }

    // Test to validate the former pystring::splitlines() behavior.
    {
        let results = split_by_lines("");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "");
    }

    // Something important to notice and preserve.
    {
        // Note: StringUtils::Split() is mainly used to parse some string content enumerating
        // a list of substrings (i.e. separator could be a space, comma, etc). In that use
        // case, a string like ",," must return three entries. Refer to 'looks' parsing for
        // example. However, StringUtils::SplitByLines() is mainly used to read some file
        // content where "xx\n" only means one string equal to "xx".
        let content = "\n";
        let res1 = split(content, b'\n');
        let res2 = split_by_lines(content);

        assert_eq!(res1.len(), 2);
        assert_eq!(res2.len(), 1);
    }
}

/// Port of `OCIO_ADD_TEST(StringUtils, searches)` @ v2.5.2.
#[test]
fn searches() {
    let reference = "lOwEr 1*& ctfG";

    assert!(starts_with(reference, "lOwEr"));
    assert!(!starts_with(reference, "wEr"));
    assert!(!starts_with(reference, "LOwEr"));

    assert!(ends_with(reference, "ctfG"));
    assert!(!ends_with(reference, "ctf"));
    assert!(!ends_with(reference, "CtfG"));
}

/// Port of `OCIO_ADD_TEST(StringUtils, replace)` @ v2.5.2.
#[test]
fn replace_() {
    let mut reference = String::from("lOwEr 1*& ctfG");

    reference = replace(&reference, "wEr", "12345");
    assert_eq!(reference, "lO12345 1*& ctfG");

    reference = replace(&reference, "345 1*", "ABC");
    assert_eq!(reference, "lO12ABC& ctfG");

    // Test a not existing subbstring.
    reference = replace(&reference, "ZY", "TO");
    assert_eq!(reference, "lO12ABC& ctfG");

    assert!(replace_in_place(&mut reference, "ct", "TO"));
    assert_eq!(reference, "lO12ABC& TOfG");

    assert!(!replace_in_place(&mut reference, "12345", "TO"));
    assert_eq!(reference, "lO12ABC& TOfG");
}

/// Port of `OCIO_ADD_TEST(StringUtils, split_whitespaces)` @ v2.5.2.
#[test]
fn split_whitespaces() {
    let reference = "10.0 9. 1 er\t1e-5f";

    let res1 = split_by_white_spaces(reference);
    assert_eq!(res1.len(), 5);
    assert_eq!(res1[0], "10.0");
    assert_eq!(res1[1], "9.");
    assert_eq!(res1[2], "1");
    assert_eq!(res1[3], "er");
    assert_eq!(res1[4], "1e-5f");
}

/// Port of `OCIO_ADD_TEST(StringUtils, find)` @ v2.5.2.
#[test]
fn find_() {
    let reference = "10.0 9. 1 er\t1e-5f";

    assert_eq!(Some(0), find(reference, "1"));
    assert_eq!(Some(12), find(reference, "\t"));

    assert_eq!(None, find(reference, "TO"));
    assert_eq!(None, find(reference, "9.1"));

    assert_eq!(Some(13), reverse_find(reference, "1"));
    assert_eq!(Some(17), reverse_find(reference, "f"));

    assert_eq!(None, reverse_find(reference, "TO"));
}

/// Port of `OCIO_ADD_TEST(StringUtils, remove_contain)` @ v2.5.2.
#[test]
fn remove_contain() {
    let reference = "1,\t2, 3, 4,5,      6";

    let mut res = split(reference, b',');

    {
        assert_eq!(res.len(), 6);
        trim_vec(&mut res);
        let values: StringVec = ["1", "2", "3", "4", "5", "6"].map(String::from).to_vec();
        assert!(res == values);

        let s = join(&res, b',');
        assert_eq!(s, "1, 2, 3, 4, 5, 6");
    }

    {
        assert!(contain(&res, "3"));
        assert!(contain(&res, "6"));

        assert!(!contain(&res, "9"));

        assert!(remove(&mut res, "3"));
        assert_eq!(res.len(), 5);
        assert!(!contain(&res, "3"));
    }
    {
        // Validate that Contain requires a full-match, not just a partial match.
        let values: StringVec = ["2 ", " 2 ", " 2", "2,", "2\n"].map(String::from).to_vec();
        assert!(contain(&values, " 2 "));
        assert!(!contain(&values, "2"));
    }
}
