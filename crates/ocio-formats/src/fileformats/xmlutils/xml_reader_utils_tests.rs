// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/xmlutils/XMLReaderUtils_tests.cpp` (@ v2.5.2).
//!
//! Upstream passes `std::string::c_str()` and C string literals: the port's slices end where
//! their terminating null is.

use ocio_testkit::upstream::{check_equal, check_throw_what};

use super::*;

/// `OCIO_CHECK_NO_THROW(OCIO::ParseNumber(str, start, end, value))`.
#[track_caller]
fn parse_ok<T: XmlNumber>(s: &[u8], start: usize, end: usize, value: &mut T) {
    if let Err(e) = parse_number(s, start, end, value) {
        panic!("OCIO_CHECK_NO_THROW failed: {e}");
    }
}

/// Port of `OCIO_ADD_TEST(XMLReaderHelper, string_to_float)` @ v2.5.2.
#[test]
fn string_to_float() {
    let mut value = 0.0f32;
    let s = b"12345";
    let len = s.len();

    parse_ok(s, 0, len, &mut value);
    check_equal(value, 12345.0f32);
}

/// Port of `OCIO_ADD_TEST(XMLReaderHelper, string_to_float_failure)` @ v2.5.2.
#[test]
fn string_to_float_failure() {
    let mut value = 0.0f32;
    let s = b"ABDSCSGFDS";
    let len = s.len();

    check_throw_what(parse_number(s, 0, len, &mut value), "can not be parsed");

    let str1 = b"10 ";
    let len1 = str1.len();

    check_throw_what(
        parse_number(str1, 0, len1, &mut value),
        "followed by unexpected characters",
    );

    // 2 characters are parsed and this is the length required.
    parse_ok(str1, 0, 2, &mut value);

    let str2 = b"12345";
    let len2 = str2.len();
    // All characters are parsed and this is more than the required length.
    // The string to double function might not stop at a given length,
    // but we detect if it did read too many characters.
    match parse_number(str2, 0, len2 - 2, &mut value) {
        // At this point value should be 123 if underlying implementation
        // properly stops at given length.
        Ok(()) => check_equal(value, 123.0f32),
        Err(ex) => {
            // If underlying implementation scans past the end, exception should be thrown.
            let what = ex.to_string();
            assert!(what.contains("followed by unexpected characters"));
        }
    }

    let str3 = b"123XX";
    let len3 = str3.len();
    // Strtod will stop after parsing 123 and this happens to be the
    // exact length that is required to be parsed.
    parse_ok(str3, 0, len3 - 2, &mut value);
}

/// Port of `OCIO_ADD_TEST(XMLReaderHelper, get_numbers)` @ v2.5.2.
#[test]
fn get_numbers_test() {
    let s = b"  1.0 , 2.0     3.0,4";
    let len = s.len();

    let values = get_numbers::<f32>(s, len).expect("OCIO_CHECK_NO_THROW");
    assert_eq!(values.len(), 4);
    check_equal(values[0], 1.0f32);
    check_equal(values[1], 2.0f32);
    check_equal(values[2], 3.0f32);
    check_equal(values[3], 4.0f32);

    // Same test without a null terminated string:
    // Copy the string into a buffer that will not be null terminated.
    // Add a delimiter at the end of the buffer.
    let mut buffer = vec![0u8; len + 1];
    buffer[..len].copy_from_slice(s);
    buffer[len] = b'\n';
    let values = get_numbers::<f32>(&buffer, len).expect("OCIO_CHECK_NO_THROW");
    assert_eq!(values.len(), 4);
    check_equal(values[0], 1.0f32);
    check_equal(values[1], 2.0f32);
    check_equal(values[2], 3.0f32);
    check_equal(values[3], 4.0f32);

    // Testing with more values.
    let str1 = b"inf, -infinity 1.0, -2.0 0x42 nan  , -nan 5.0";
    let len1 = str1.len();

    let values = get_numbers::<f32>(str1, len1).expect("OCIO_CHECK_NO_THROW");
    assert_eq!(values.len(), 8);
    assert!(values[0].is_infinite());
    assert!(values[1].is_infinite());
    check_equal(values[2], 1.0f32);
    check_equal(values[3], -2.0f32);
    check_equal(values[4], 66.0f32); // i.e. 0x42
    assert!(values[5].is_nan());
    assert!(values[6].is_nan());
    check_equal(values[7], 5.0f32);

    // It is valid to start with delimiters.
    let str2 = b",  ,, , 0 2.0 \n \t 3.0 0.1e+1";
    let len2 = str2.len();

    let values = get_numbers::<f32>(str2, len2).expect("OCIO_CHECK_NO_THROW");
    assert_eq!(values.len(), 4);
    check_equal(values[0], 0.0f32);
    check_equal(values[1], 2.0f32);
    check_equal(values[2], 3.0f32);
    check_equal(values[3], 1.0f32);

    // Error: text is not a number.
    let str3 = b"  0   error 2.0 3.0";
    let len3 = str3.len();

    check_throw_what(get_numbers::<f32>(str3, len3), "can not be parsed");

    // Error: number is not separated from text.
    let str4 = b"0   1.0error 2.0 3.0";
    let len4 = str4.len();

    check_throw_what(
        get_numbers::<f32>(str4, len4),
        "followed by unexpected characters",
    );
}

/// Port of `OCIO_ADD_TEST(XMLReaderHelper, trim)` @ v2.5.2.
#[test]
fn trim_test() {
    let original1 = b"    some text    ".to_vec();
    let original2 = b" \n \r some text  \t \x0b \x0c ".to_vec();
    {
        let mut value = original1.clone();
        trim(&mut value);
        assert_eq!(value, b"some text");
        value = original2.clone();
        trim(&mut value);
        assert_eq!(value, b"some text");
    }

    {
        let mut value = original1.clone();
        r_trim(&mut value);
        assert_eq!(value, b"    some text");
        value = original2.clone();
        r_trim(&mut value);
        assert_eq!(value, b" \n \r some text");
    }

    {
        let mut value = original1.clone();
        l_trim(&mut value);
        assert_eq!(value, b"some text    ");
        value = original2.clone();
        l_trim(&mut value);
        assert_eq!(value, b"some text  \t \x0b \x0c ");
    }
}

/// Port of `OCIO_ADD_TEST(XMLReaderHelper, parse_number)` @ v2.5.2.
#[test]
fn parse_number_test() {
    let mut data = 0.0f32;
    // (buffer, end, expected)
    let cases: [(&[u8], usize, f32); 24] = [
        (b"1 0", 1, 1.0),
        (b" 1 0", 2, 1.0),
        (b"1.0 0", 3, 1.0),
        (b"1.0000 0", 6, 1.0),
        (b"1.0", 3, 1.0),
        (b"1", 1, 1.0),
        (b"10.0e-1", 7, 1.0),
        (b"0.1e+1", 6, 1.0),
        (b"-1 0", 2, -1.0),
        (b"-1.0 0", 4, -1.0),
        (b"  -1.0", 6, -1.0),
        (b"   -1", 5, -1.0),
        (b" -10.0e-1", 9, -1.0),
        (b"-0.1e+1", 7, -1.0),
        (b"0.001", 5, 0.001),
        (b"-0.001", 6, -0.001),
        (b".001", 4, 0.001),
        (b"-.001", 5, -0.001),
        (b".01e-1", 6, 0.001),
        (b"-.01e-1", 7, -0.001),
        (b"-.01e-1,", 7, -0.001),
        (b"-.01e-1\n", 7, -0.001),
        (b"-.01e-1\t", 7, -0.001),
        (b"10E-1", 5, 1.0),
    ];
    for (buffer, end, expected) in cases {
        parse_ok(buffer, 0, end, &mut data);
        check_equal(data, expected);
    }
    {
        let buffer = b"-1.0000 0";
        let end = find_delim(buffer, buffer.len(), 0);
        parse_ok(buffer, 0, end, &mut data);
        check_equal(data, -1.0f32);
    }
    {
        let buffer = b"INF";
        parse_ok(buffer, 0, 3, &mut data);
        check_equal(data, f32::INFINITY);
    }
    {
        let buffer = b"INF 1.0 2.0";
        let mut pos = 0usize;
        let size = buffer.len();
        let mut next_pos = find_delim(buffer, size, pos);
        check_equal(next_pos, 3);
        parse_ok(buffer, pos, next_pos, &mut data);
        check_equal(data, f32::INFINITY);
        pos = find_next_token_start(buffer, size, next_pos);
        check_equal(pos, 4);
        next_pos = find_delim(buffer, size, pos);
        check_equal(next_pos, 7);
        parse_ok(buffer, pos, next_pos, &mut data);
        check_equal(data, 1.0f32);
        pos = find_next_token_start(buffer, size, next_pos);
        check_equal(pos, 8);
        next_pos = find_delim(buffer, size, pos);
        check_equal(next_pos, 11);
        parse_ok(buffer, pos, next_pos, &mut data);
        check_equal(data, 2.0f32);
    }
    for (buffer, expected) in [
        (b"INFINITY".as_slice(), f32::INFINITY),
        (b"-INF", -f32::INFINITY),
        (b"-INFINITY", -f32::INFINITY),
    ] {
        parse_ok(buffer, 0, buffer.len(), &mut data);
        check_equal(data, expected);
    }
    for buffer in [b"NAN".as_slice(), b"-NAN"] {
        parse_ok(buffer, 0, buffer.len(), &mut data);
        assert!(data.is_nan());
    }
    {
        let buffer = b"0.10E01";
        parse_ok(buffer, 0, buffer.len(), &mut data);
        check_equal(data, 1.0f32);
    }

    {
        let buffer = b"XY";
        check_throw_what(parse_number(buffer, 0, 2, &mut data), "can not be parsed");
    }
}

/// Port of `OCIO_ADD_TEST(XMLReaderHelper, find_sub_string)` @ v2.5.2.
#[test]
fn find_sub_string_test() {
    // (buffer, start, end)
    let cases: [(&[u8], usize, usize); 9] = [
        //012345678901234
        (b"   new order   ", 3, 12),
        (b"new order   ", 0, 9),
        (b"   new order", 3, 12),
        (b"new order", 0, 9),
        (b"", 0, 0),
        (b"      ", 0, 0),
        (b"   \t123    ", 4, 7),
        (b"1   \t \n \r", 0, 1),
        (b"\t", 0, 0),
    ];
    for (buffer, expected_start, expected_end) in cases {
        let (start, end) = find_sub_string(buffer, buffer.len());
        check_equal(start, expected_start);
        check_equal(end, expected_end);
    }
}
