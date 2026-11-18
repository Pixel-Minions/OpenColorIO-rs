// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.9.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.9.0's `test/fptostring_test.cpp`, verbatim: `FpToString(v)` and
//! `FpToString(v, precision)` are `fp!(v)` and `fp!(v, precision)`, a `float` literal `1.5f`
//! is `1.5_f32`, and `convert_with_stringstream` (test/fptostring_test.cpp:12-20) is the
//! platform C runtime's `printf("%.<precision>g")`, the conversion `std::stringstream <<`
//! makes on both platforms (CLAUDE.md rule 2), never the port's own stream.

use super::{Fp, fp_to_string};
use ocio_testkit::crt;

/// A `float` or a `double`, as `FpToString`'s overloads take them.
trait IntoFp: Copy {
    fn into_fp(self) -> Fp;
    fn as_double(self) -> f64;
}

impl IntoFp for f32 {
    fn into_fp(self) -> Fp {
        Fp::F32(self)
    }
    fn as_double(self) -> f64 {
        f64::from(self)
    }
}

impl IntoFp for f64 {
    fn into_fp(self) -> Fp {
        Fp::F64(self)
    }
    fn as_double(self) -> f64 {
        self
    }
}

/// `FpToString(v, precision = 0)`.
fn fp_to_string_of<T: IntoFp>(v: T, precision: usize) -> String {
    fp_to_string(v.into_fp(), precision)
}

/// `convert_with_stringstream(v, precision = 0)`: a classic-locale `std::stringstream`, with
/// `std::setprecision(precision)` when `precision > 0` (its default precision is 6), which
/// formats like `%.<precision>g` (a `float` as the `double` it converts to).
fn convert_with_stringstream<T: IntoFp>(v: T, precision: usize) -> String {
    let precision = if precision > 0 { precision } else { 6 };
    crt::format_f64(&format!("%.{precision}g"), v.as_double())
}

macro_rules! fp {
    ($v:expr) => {
        fp_to_string_of($v, 0)
    };
    ($v:expr, $p:expr) => {
        fp_to_string_of($v, $p)
    };
}

macro_rules! stringstream {
    ($v:expr) => {
        convert_with_stringstream($v, 0)
    };
    ($v:expr, $p:expr) => {
        convert_with_stringstream($v, $p)
    };
}

/// Port of yaml-cpp 0.9.0 `TEST(FpToStringTest, conversion_double)` (test/fptostring_test.cpp).
#[test]
fn conversion_double() {
    // Issue motivating FpToString function,
    // https://github.com/jbeder/yaml-cpp/issues/1289
    // Original problem at hand:
    assert_eq!("34.34", fp!(34.34));
    assert_eq!("56.56", fp!(56.56));
    assert_eq!("12.12", fp!(12.12));
    assert_eq!("78.78", fp!(78.78));
    // Special challenge with rounding
    // https://github.com/jbeder/yaml-cpp/issues/1289#issuecomment-2211705536
    assert_eq!("1.54743e+26", fp!(1.5474250491e+26_f32));
    assert_eq!(
        stringstream!(1.5474250491e+26_f32),
        fp!(1.5474250491e+26_f32)
    );
    assert_eq!("1.5474251e+26", fp!(1.5474250491e+26_f32, 8));
    // prints the same way as std::stringstream
    assert_eq!(stringstream!(1.), fp!(1.));
    assert_eq!(stringstream!(1e0), fp!(1e0));
    assert_eq!(stringstream!(1e1), fp!(1e1));
    assert_eq!(stringstream!(1e2), fp!(1e2));
    assert_eq!(stringstream!(1e3), fp!(1e3));
    assert_eq!(stringstream!(1e4), fp!(1e4));
    assert_eq!(stringstream!(1e5), fp!(1e5));
    assert_eq!(stringstream!(1e6), fp!(1e6));
    assert_eq!(stringstream!(1e7), fp!(1e7));
    assert_eq!(stringstream!(1e8), fp!(1e8));
    assert_eq!(stringstream!(1e9), fp!(1e9));
    // Print by default values below 1e6 without scientific notation
    assert_eq!("1", fp!(1.));
    assert_eq!("1", fp!(1e0));
    assert_eq!("10", fp!(1e1));
    assert_eq!("100", fp!(1e2));
    assert_eq!("1000", fp!(1e3));
    assert_eq!("10000", fp!(1e4));
    assert_eq!("100000", fp!(1e5));
    assert_eq!("1e+06", fp!(1e6));
    assert_eq!("1e+07", fp!(1e7));
    assert_eq!("1e+08", fp!(1e8));
    assert_eq!("1e+09", fp!(1e9));
    // prints the same way as std::stringstream
    assert_eq!(stringstream!(1.), fp!(1.));
    assert_eq!(stringstream!(1e-0), fp!(1e-0));
    assert_eq!(stringstream!(1e-1), fp!(1e-1));
    assert_eq!(stringstream!(1e-2), fp!(1e-2));
    assert_eq!(stringstream!(1e-3), fp!(1e-3));
    assert_eq!(stringstream!(1e-4), fp!(1e-4));
    assert_eq!(stringstream!(1e-5), fp!(1e-5));
    assert_eq!(stringstream!(1e-6), fp!(1e-6));
    assert_eq!(stringstream!(1e-7), fp!(1e-7));
    assert_eq!(stringstream!(1e-8), fp!(1e-8));
    assert_eq!(stringstream!(1e-9), fp!(1e-9));
    // Print by default values above 1e-5 without scientific notation
    assert_eq!("1", fp!(1.));
    assert_eq!("1", fp!(1e-0));
    assert_eq!("0.1", fp!(1e-1));
    assert_eq!("0.01", fp!(1e-2));
    assert_eq!("0.001", fp!(1e-3));
    assert_eq!("0.0001", fp!(1e-4));
    assert_eq!("1e-05", fp!(1e-5));
    assert_eq!("1e-06", fp!(1e-6));
    assert_eq!("1e-07", fp!(1e-7));
    assert_eq!("1e-08", fp!(1e-8));
    assert_eq!("1e-09", fp!(1e-9));
    // changing precision has the same effect as std::stringstream
    assert_eq!(stringstream!(123., 1), fp!(123., 1));
    assert_eq!(stringstream!(1234567., 7), fp!(1234567., 7));
    assert_eq!(stringstream!(12345.67, 7), fp!(12345.67, 7));
    assert_eq!(stringstream!(1234567e-9, 7), fp!(1234567e-9, 7));
    assert_eq!(stringstream!(1234567e-9, 1), fp!(1234567e-9, 1));
    // known example that is difficult to round
    assert_eq!("1", fp!(0.9999, 2));
    assert_eq!("-1", fp!(-0.9999, 2));
    // some more random tests
    assert_eq!("1.25", fp!(1.25));
    assert_eq!("34.34", fp!(34.34));
    assert_eq!("1e+20", fp!(1e+20));
    assert_eq!("1.1e+20", fp!(1.1e+20));
    assert_eq!("1e-20", fp!(1e-20));
    assert_eq!("1.1e-20", fp!(1.1e-20));
    assert_eq!("1e-20", fp!(0.1e-19));
    assert_eq!("1.1e-20", fp!(0.11e-19));
    assert_eq!("19", fp!(18.9, 2));
    assert_eq!("20", fp!(19.9, 2));
    assert_eq!("2e+01", fp!(19.9, 1));
    assert_eq!("1.2e-05", fp!(1.234e-5, 2));
    assert_eq!("1.3e-05", fp!(1.299e-5, 2));
    assert_eq!("-1", fp!(-1.));
    assert_eq!("-1.25", fp!(-1.25));
    assert_eq!("-34.34", fp!(-34.34));
    assert_eq!("-1e+20", fp!(-1e+20));
    assert_eq!("-1.1e+20", fp!(-1.1e+20));
    assert_eq!("-1e-20", fp!(-1e-20));
    assert_eq!("-1.1e-20", fp!(-1.1e-20));
    assert_eq!("-1e-20", fp!(-0.1e-19));
    assert_eq!("-1.1e-20", fp!(-0.11e-19));
    assert_eq!("-19", fp!(-18.9, 2));
    assert_eq!("-20", fp!(-19.9, 2));
    assert_eq!("-2e+01", fp!(-19.9, 1));
    assert_eq!("-1.2e-05", fp!(-1.234e-5, 2));
    assert_eq!("-1.3e-05", fp!(-1.299e-5, 2));
}

/// Port of yaml-cpp 0.9.0 `TEST(FpToStringTest, conversion_float)` (test/fptostring_test.cpp).
#[test]
fn conversion_float() {
    // Issue motivating FpToString function,
    // https://github.com/jbeder/yaml-cpp/issues/1289
    // Original problem at hand:
    assert_eq!("34.34", fp!(34.34_f32));
    assert_eq!("56.56", fp!(56.56_f32));
    assert_eq!("12.12", fp!(12.12_f32));
    assert_eq!("78.78", fp!(78.78_f32));
    // prints the same way as std::stringstream
    assert_eq!(stringstream!(1.0_f32), fp!(1.0_f32));
    assert_eq!(stringstream!(1e0_f32), fp!(1e0_f32));
    assert_eq!(stringstream!(1e1_f32), fp!(1e1_f32));
    assert_eq!(stringstream!(1e2_f32), fp!(1e2_f32));
    assert_eq!(stringstream!(1e3_f32), fp!(1e3_f32));
    assert_eq!(stringstream!(1e4_f32), fp!(1e4_f32));
    assert_eq!(stringstream!(1e5_f32), fp!(1e5_f32));
    assert_eq!(stringstream!(1e6_f32), fp!(1e6_f32));
    assert_eq!(stringstream!(1e7_f32), fp!(1e7_f32));
    assert_eq!(stringstream!(1e8_f32), fp!(1e8_f32));
    assert_eq!(stringstream!(1e9_f32), fp!(1e9_f32));
    // Print by default values below 1e6 without scientific notation
    assert_eq!("1", fp!(1.0_f32));
    assert_eq!("1", fp!(1e0_f32));
    assert_eq!("10", fp!(1e1_f32));
    assert_eq!("100", fp!(1e2_f32));
    assert_eq!("1000", fp!(1e3_f32));
    assert_eq!("10000", fp!(1e4_f32));
    assert_eq!("100000", fp!(1e5_f32));
    assert_eq!("1e+06", fp!(1e6_f32));
    assert_eq!("1e+07", fp!(1e7_f32));
    assert_eq!("1e+08", fp!(1e8_f32));
    assert_eq!("1e+09", fp!(1e9_f32));
    // prints the same way as std::stringstream
    assert_eq!(stringstream!(1.0_f32), fp!(1.0_f32));
    assert_eq!(stringstream!(1e-0_f32), fp!(1e-0_f32));
    assert_eq!(stringstream!(1e-1_f32), fp!(1e-1_f32));
    assert_eq!(stringstream!(1e-2_f32), fp!(1e-2_f32));
    assert_eq!(stringstream!(1e-3_f32), fp!(1e-3_f32));
    assert_eq!(stringstream!(1e-4_f32), fp!(1e-4_f32));
    assert_eq!(stringstream!(1e-5_f32), fp!(1e-5_f32));
    assert_eq!(stringstream!(1e-6_f32), fp!(1e-6_f32));
    assert_eq!(stringstream!(1e-7_f32), fp!(1e-7_f32));
    assert_eq!(stringstream!(1e-8_f32), fp!(1e-8_f32));
    assert_eq!(stringstream!(1e-9_f32), fp!(1e-9_f32));
    // Print by default values above 1e-5 without scientific notation
    assert_eq!("1", fp!(1.0_f32));
    assert_eq!("1", fp!(1e-0_f32));
    assert_eq!("0.1", fp!(1e-1_f32));
    assert_eq!("0.01", fp!(1e-2_f32));
    assert_eq!("0.001", fp!(1e-3_f32));
    assert_eq!("0.0001", fp!(1e-4_f32));
    assert_eq!("1e-05", fp!(1e-5_f32));
    assert_eq!("1e-06", fp!(1e-6_f32));
    assert_eq!("1e-07", fp!(1e-7_f32));
    assert_eq!("1e-08", fp!(1e-8_f32));
    assert_eq!("1e-09", fp!(1e-9_f32));
    // changing precision has the same effect as std::stringstream
    assert_eq!(stringstream!(123.0_f32, 1), fp!(123.0_f32, 1));
    assert_eq!(stringstream!(1234567.0_f32, 7), fp!(1234567.0_f32, 7));
    assert_eq!(stringstream!(12345.67_f32, 7), fp!(12345.67_f32, 7));
    assert_eq!(stringstream!(1234567e-9_f32, 7), fp!(1234567e-9_f32, 7));
    assert_eq!(stringstream!(1234567e-9_f32, 1), fp!(1234567e-9_f32, 1));
    // known example that is difficult to round
    assert_eq!("1", fp!(0.9999_f32, 2));
    assert_eq!("-1", fp!(-0.9999_f32, 2));
    // some more random tests
    assert_eq!("1.25", fp!(1.25_f32));
    assert_eq!("34.34", fp!(34.34_f32));
    assert_eq!("1e+20", fp!(1e+20_f32));
    assert_eq!("1.1e+20", fp!(1.1e+20_f32));
    assert_eq!("1e-20", fp!(1e-20_f32));
    assert_eq!("1.1e-20", fp!(1.1e-20_f32));
    assert_eq!("1e-20", fp!(0.1e-19_f32));
    assert_eq!("1.1e-20", fp!(0.11e-19_f32));
    assert_eq!("19", fp!(18.9_f32, 2));
    assert_eq!("20", fp!(19.9_f32, 2));
    assert_eq!("2e+01", fp!(19.9_f32, 1));
    assert_eq!("1.2e-05", fp!(1.234e-5_f32, 2));
    assert_eq!("1.3e-05", fp!(1.299e-5_f32, 2));
    assert_eq!("-1", fp!(-1.0_f32));
    assert_eq!("-1.25", fp!(-1.25_f32));
    assert_eq!("-34.34", fp!(-34.34_f32));
    assert_eq!("-1e+20", fp!(-1e+20_f32));
    assert_eq!("-1.1e+20", fp!(-1.1e+20_f32));
    assert_eq!("-1e-20", fp!(-1e-20_f32));
    assert_eq!("-1.1e-20", fp!(-1.1e-20_f32));
    assert_eq!("-1e-20", fp!(-0.1e-19_f32));
    assert_eq!("-1.1e-20", fp!(-0.11e-19_f32));
    assert_eq!("-19", fp!(-18.9_f32, 2));
    assert_eq!("-20", fp!(-19.9_f32, 2));
    assert_eq!("-2e+01", fp!(-19.9_f32, 1));
    assert_eq!("-1.2e-05", fp!(-1.234e-5_f32, 2));
    assert_eq!("-1.3e-05", fp!(-1.299e-5_f32, 2));
}

/// Port of yaml-cpp 0.9.0 `TEST(FpToStringTest, vulnerability_stack_buffer_overflow)` (test/fptostring_test.cpp).
#[test]
fn vulnerability_stack_buffer_overflow() {
    assert_eq!(
        fp!(1.0e100, 200),
        "10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    );
}
