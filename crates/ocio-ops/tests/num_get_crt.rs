// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The conversion half of `num_get`'s floating-point reading against this platform's C
//! runtime, the library both wheels call for it: on Windows, the fields MSVC's `num_get`
//! gathers (`num_get::msvc_parse_fp`) converted by the port's UCRT `strtod`/`strtof` and by the
//! UCRT itself; on Linux, the fields libstdc++'s stage 2 gathers
//! (`num_get::libstdcxx_extract_float`) converted by the port's glibc `strtod`/`strtof` and by
//! glibc. The value's bits, the end and `errno == ERANGE` must agree. The gathering itself is
//! checked against the wheels (`crates/ocio/tests/yaml_cpp_convert_oracle.rs`).

use ocio_ops::utils::num_get;
use ocio_testkit::crt::{ERANGE, strtod_c, strtof_c};

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Inputs to gather fields from: pieces of decimal and hexadecimal numbers, exponents at the
/// edges of both formats, and long runs of digits (past MSVC's 768 significant digits).
fn inputs() -> Vec<Vec<u8>> {
    const PIECES: &[&str] = &[
        "0",
        "1",
        "5",
        "8",
        "9",
        "00",
        "123",
        "999999999",
        "0000000000",
        ".",
        "e",
        "E",
        "e-",
        "e+",
        "-",
        "+",
        "0x",
        "0X",
        "p",
        "P",
        "p-",
        "a",
        "f",
        "F",
        "38",
        "39",
        "45",
        "46",
        "149",
        "150",
        "307",
        "308",
        "309",
        "323",
        "324",
        "325",
        "1022",
        "1074",
        "1075",
        "1100",
        "4200",
        "4.9",
        "2.2250738585072014",
        "1.401298464324817",
        "3.4028",
        "1.7976931348623157",
    ];
    let mut rng = Rng(0xC0FF_EE15_600D_BEEF);
    let mut out: Vec<Vec<u8>> = (0..20000)
        .map(|_| {
            let len = 1 + rng.below(7);
            (0..len)
                .flat_map(|_| PIECES[rng.below(PIECES.len())].bytes())
                .collect()
        })
        .collect();
    for digit in *b"0159f" {
        for len in [700, 767, 768, 769, 800, 1200] {
            let run = vec![digit; len];
            out.push([b"1".as_slice(), &run].concat());
            out.push([b"0.".as_slice(), &run].concat());
            out.push([b"0x1".as_slice(), &run, b"p-4000"].concat());
            out.push([b"1.".as_slice(), &run, b"e-1100"].concat());
            out.push([b"0.".as_slice(), &run, b"1e300"].concat());
        }
    }
    out
}

/// The fields this platform's `num_get` gathers from the inputs.
fn fields() -> Vec<Vec<u8>> {
    inputs()
        .iter()
        .filter_map(|input| {
            if cfg!(windows) {
                num_get::msvc_parse_fp(input).0
            } else {
                Some(num_get::libstdcxx_extract_float(input).0)
            }
        })
        .collect()
}

#[test]
fn doubles_convert_as_the_c_runtime_converts_them() {
    for field in fields() {
        let (value, end, erange) = if cfg!(windows) {
            ocio_ops::utils::number_utils::ucrt::strtod_field(&field)
        } else {
            ocio_ops::utils::number_utils::glibc::strtod(&field)
        };
        let crt = strtod_c(&field);
        let label = String::from_utf8_lossy(&field);
        assert_eq!(crt.value.to_bits(), value.to_bits(), "{label}");
        assert_eq!(crt.end, end, "{label}");
        assert_eq!(crt.errno == ERANGE, erange, "{label}");
    }
}

#[test]
fn floats_convert_as_the_c_runtime_converts_them() {
    for field in fields() {
        let (value, end, erange) = if cfg!(windows) {
            ocio_ops::utils::number_utils::ucrt::strtof_field(&field)
        } else {
            ocio_ops::utils::number_utils::glibc::strtof(&field)
        };
        let crt = strtof_c(&field);
        let label = String::from_utf8_lossy(&field);
        assert_eq!(crt.value.to_bits(), value.to_bits(), "{label}");
        assert_eq!(crt.end, end, "{label}");
        assert_eq!(crt.errno == ERANGE, erange, "{label}");
    }
}
