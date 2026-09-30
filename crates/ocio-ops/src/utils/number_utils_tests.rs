// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/utils/NumberUtils_tests.cpp` @ v2.5.2. Upstream runs these on whichever
//! branch of NumberUtils.h the build compiled; the port checks every branch it has.

use super::*;

const FLAVORS: [Flavor; 1] = [Flavor::Strtod];

/// `TEST_FROM_CHARS` of `from_chars_float`: the whole string converts.
#[track_caller]
fn from_chars_ok(flavor: Flavor, text: &str, val: &mut f32) {
    let res = from_chars_f32(flavor, text.as_bytes(), text.len(), val);
    assert_eq!(res.ec, Errc::Ok, "{flavor:?} {text:?}");
    assert_eq!(res.ptr, text.len(), "{flavor:?} {text:?}");
}

/// Port of `OCIO_ADD_TEST(NumberUtils, from_chars_float)` @ v2.5.2.
#[test]
fn from_chars_float() {
    for flavor in FLAVORS {
        let mut val = 0.0f32;

        // regular numbers
        from_chars_ok(flavor, "-7", &mut val);
        assert_eq!(val, -7.0f32);
        from_chars_ok(flavor, "1.5", &mut val);
        assert_eq!(val, 1.5f32);
        from_chars_ok(flavor, "-17.25", &mut val);
        assert_eq!(val, -17.25f32);
        from_chars_ok(flavor, "-.75", &mut val);
        assert_eq!(val, -0.75f32);
        from_chars_ok(flavor, "11.", &mut val);
        assert_eq!(val, 11.0f32);
        // exponent notation
        from_chars_ok(flavor, "1e3", &mut val);
        assert_eq!(val, 1000.0f32);
        from_chars_ok(flavor, "1e+2", &mut val);
        assert_eq!(val, 100.0f32);
        from_chars_ok(flavor, "50e-2", &mut val);
        assert_eq!(val, 0.5f32);
        from_chars_ok(flavor, "-1.5e2", &mut val);
        assert_eq!(val, -150.0f32);
        // whitespace/prefix handling
        from_chars_ok(flavor, "+57.125", &mut val);
        assert_eq!(val, 57.125f32);
        from_chars_ok(flavor, "  \t 123.5", &mut val);
        assert_eq!(val, 123.5f32);
        // special values
        from_chars_ok(flavor, "-infinity", &mut val);
        assert_eq!(val, f32::NEG_INFINITY);
        from_chars_ok(flavor, "nan", &mut val);
        assert!(val.is_nan());
        // hex format should be parsed
        from_chars_ok(flavor, "0x42", &mut val);
        assert_eq!(val, 66.0f32);
        from_chars_ok(flavor, "0x42ab.c", &mut val);
        assert_eq!(val, 17067.75f32);

        // valid numbers with trailing non-number chars should stop there
        let text = "-7.5ab";
        let res = from_chars_f32(flavor, text.as_bytes(), text.len(), &mut val);
        assert_eq!(res.ptr, 4, "{flavor:?}");
        assert_eq!(val, -7.5f32);
        let text = "infinitya";
        let res = from_chars_f32(flavor, text.as_bytes(), text.len(), &mut val);
        assert_eq!(res.ptr, 8, "{flavor:?}");
        assert_eq!(val, f32::INFINITY);
        let text = "0x18g";
        let res = from_chars_f32(flavor, text.as_bytes(), text.len(), &mut val);
        assert_eq!(res.ptr, 4, "{flavor:?}");
        assert_eq!(val, 24.0f32);
    }
}

/// Port of `OCIO_ADD_TEST(NumberUtils, from_chars_float_failures)` @ v2.5.2.
#[test]
fn from_chars_float_failures() {
    for flavor in FLAVORS {
        let mut val = 7.5f32;
        for text in ["", "ab", "   ", "---", "e3", "_x", "+."] {
            let res = from_chars_f32(flavor, text.as_bytes(), text.len(), &mut val);
            assert_eq!(val, 7.5f32, "{flavor:?} {text:?}");
            assert_eq!(res.ec, Errc::InvalidArgument, "{flavor:?} {text:?}");
        }
    }
}
