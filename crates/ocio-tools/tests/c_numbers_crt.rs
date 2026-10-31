// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio_tools::c_numbers` against the platform's C runtime (`ocio_testkit::crt`), which is the
//! reference for these functions (CLAUDE.md, rule 2): the same text gives the same bits.

use ocio_testkit::crt::{atof_c, atoi_c, strtof_c};
use ocio_tools::c_numbers::{atof, atoi, strtof};

/// What can begin a number, what can stop one, and the spellings that aren't digits.
const INPUTS: &[&str] = &[
    "",
    " ",
    "\t\n\x0b\x0c\r 7",
    "-",
    "+",
    "--1",
    "+-1",
    "abc",
    "1abc",
    "12 34",
    "-0",
    "+0",
    "0",
    "00012",
    "1",
    "-1",
    "+1",
    ".",
    "-.",
    ".5",
    "-.5",
    "5.",
    "1.5",
    "0.18",
    "1e",
    "1e+",
    "1e-",
    "1e5",
    "1E5",
    "1e+5",
    "1e-5",
    "1.5e3x",
    ".e5",
    "e5",
    "1e400",
    "-1e400",
    "1e-400",
    "-1e-400",
    "1e-46",
    "1e-45",
    "1.4e-45",
    "1e39",
    "3.4028235e38",
    "3.4028237e38",
    "1.17549435e-38",
    "4.9e-324",
    "2.4e-324",
    "1.7976931348623157e308",
    "1.7976931348623159e308",
    "0.1",
    "0.2",
    "0.3",
    "0.30000000000000004",
    "16777217",
    "16777219",
    "33554435",
    "9007199254740993",
    "9007199254740995",
    "1.00000000000000011102230246251565404236316680908203125",
    "1.00000000000000033306690738754696212708950042724609375",
    "2147483647",
    "2147483648",
    "-2147483648",
    "-2147483649",
    "4294967295",
    "4294967296",
    "4294967297",
    "-4294967297",
    "9223372036854775807",
    "9223372036854775808",
    "-9223372036854775808",
    "-9223372036854775809",
    "18446744073709551616",
    "99999999999999999999999999999999",
    "-99999999999999999999999999999999",
    "0x",
    "0x1",
    "0X1",
    "-0x10",
    "0x1p3",
    "0x1P3",
    "0x1.8p1",
    "0x.8p1",
    "0x.p1",
    "0x1p",
    "0x1p+",
    "0xg",
    "0x1fffffffffffffp0",
    "0x1ffffffp0",
    "0x3fffffffffffffp0",
    "0x1p-1074",
    "0x1p-1075",
    "0x1p-149",
    "0x1p-150",
    "0x1p1024",
    "0x1p128",
    "inf",
    "INF",
    "Inf",
    "-inf",
    "+inf",
    "infinity",
    "INFINITY",
    "-Infinity",
    "infinit",
    "infin",
    "in",
    "i",
    "infx",
    "nan",
    "NAN",
    "NaN",
    "-nan",
    "+nan",
    "na",
    "n",
    "nanx",
    "nan(",
    "nan()",
    "nan(1)",
    "nan(0x7)",
    "nan(0x400000)",
    "nan(abc_1)",
    "-nan(abc)",
    "nan(abc",
    "nan(a b)",
    "nan(snan)",
    "-nan(snan)",
    "NAN(SNAN)",
    "nan(ind)",
    "-nan(ind)",
    "NaN(IND)",
    "nan(snan",
    "nan(ind",
    "nan(snanx)",
    "nan(indx)",
    "1,5",
    "1_000",
    "\u{e9}",
    "1\u{e9}",
];

#[test]
fn the_platform_gives_the_same_numbers() {
    assert!(INPUTS.len() > 100);
    let mut mismatches = Vec::new();
    for input in INPUTS {
        let s = input.as_bytes();
        let (expected, actual) = (atoi_c(s), atoi(s));
        if expected != actual {
            mismatches.push(format!("atoi({input:?}): {expected}, port {actual}"));
        }
        let (expected, actual) = (atof_c(s), atof(s));
        if expected.to_bits() != actual.to_bits() {
            mismatches.push(format!(
                "atof({input:?}): {:016x}, port {:016x}",
                expected.to_bits(),
                actual.to_bits()
            ));
        }
        let (expected, actual) = (strtof_c(s).value, strtof(s));
        if expected.to_bits() != actual.to_bits() {
            mismatches.push(format!(
                "strtof({input:?}): {:08x}, port {:08x}",
                expected.to_bits(),
                actual.to_bits()
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

/// The inputs of the list, each with a suffix: what follows a number doesn't change it, on
/// either platform.
#[test]
fn what_follows_a_number_is_ignored_the_same_way() {
    let mut mismatches = Vec::new();
    for input in INPUTS {
        for suffix in [" 1", "x", "e", "p1", ")", "(", "."] {
            let text = format!("{input}{suffix}");
            let s = text.as_bytes();
            if atoi_c(s) != atoi(s) {
                mismatches.push(format!("atoi({text:?})"));
            }
            if atof_c(s).to_bits() != atof(s).to_bits() {
                mismatches.push(format!("atof({text:?})"));
            }
            if strtof_c(s).value.to_bits() != strtof(s).to_bits() {
                mismatches.push(format!("strtof({text:?})"));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
