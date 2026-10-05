// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the look list parsing: `tests/cpu/LookParse_tests.cpp` @ v2.5.2.

use super::*;
use ocio_ops::open_color_types::TransformDirection::{Forward, Inverse};

/// Port of `OCIO_ADD_TEST(LookParse, parse)` @ v2.5.2.
#[test]
fn parse() {
    let mut r = LookParseResult::default();

    {
        let options = r.parse(b"").unwrap();
        assert_eq!(options.len(), 0);
        assert!(options.is_empty());
    }

    {
        let options = r.parse(b"  ").unwrap();
        assert_eq!(options.len(), 0);
        assert!(options.is_empty());
    }

    {
        let options = r.parse(b"cc").unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert!(!options.is_empty());
    }

    {
        let options = r.parse(b"+cc").unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert!(!options.is_empty());
    }

    {
        let options = r.parse(b"  +cc").unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert!(!options.is_empty());
    }

    {
        let options = r.parse(b"  +cc   ").unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert!(!options.is_empty());
    }

    {
        let options = r.parse(b"+cc,-di").unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].len(), 2);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert_eq!(options[0][1].name, b"di");
        assert_eq!(options[0][1].dir, Inverse);
        assert!(!options.is_empty());
    }

    {
        let options = r.parse(b"  +cc ,  -di").unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].len(), 2);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert_eq!(options[0][1].name, b"di");
        assert_eq!(options[0][1].dir, Inverse);
        assert!(!options.is_empty());
    }

    {
        let options = r.parse(b"  +cc :  -di").unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].len(), 2);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert_eq!(options[0][1].name, b"di");
        assert_eq!(options[0][1].dir, Inverse);
        assert!(!options.is_empty());
    }

    {
        let options = r.parse(b"+cc, -di |-cc").unwrap();
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].len(), 2);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert_eq!(options[0][1].name, b"di");
        assert_eq!(options[0][1].dir, Inverse);
        assert_eq!(options[1].len(), 1);
        assert!(!options.is_empty());
        assert_eq!(options[1][0].name, b"cc");
        assert_eq!(options[1][0].dir, Inverse);
    }

    {
        let options = r.parse(b"+cc, -di |-cc|   ").unwrap();
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].len(), 2);
        assert_eq!(options[0][0].name, b"cc");
        assert_eq!(options[0][0].dir, Forward);
        assert_eq!(options[0][1].name, b"di");
        assert_eq!(options[0][1].dir, Inverse);
        assert_eq!(options[1].len(), 1);
        assert!(!options.is_empty());
        assert_eq!(options[1][0].name, b"cc");
        assert_eq!(options[1][0].dir, Inverse);
        assert_eq!(options[2].len(), 1);
        assert_eq!(options[2][0].name, b"");
        assert_eq!(options[2][0].dir, Forward);
    }
}

/// Port of `OCIO_ADD_TEST(LookParse, reverse)` @ v2.5.2.
#[test]
fn reverse() {
    let mut r = LookParseResult::default();

    {
        r.parse(b"+cc, -di |-cc|   ").unwrap();
        r.reverse();
        let options = r.options();

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].len(), 2);
        assert_eq!(options[0][1].name, b"cc");
        assert_eq!(options[0][1].dir, Inverse);
        assert_eq!(options[0][0].name, b"di");
        assert_eq!(options[0][0].dir, Forward);
        assert_eq!(options[1].len(), 1);
        assert!(!options.is_empty());
        assert_eq!(options[1][0].name, b"cc");
        assert_eq!(options[1][0].dir, Forward);
        assert_eq!(options[2].len(), 1);
        assert_eq!(options[2][0].name, b"");
        assert_eq!(options[2][0].dir, Inverse);
    }
}
