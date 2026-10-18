// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's guard around the bakes' buffers (docs/improvements.md, U-77). Its error for
//! sizes whose `int` wraps to a positive value, where upstream writes past its buffer, is the
//! port's decision; the wheels' errors for a negative wrap ("vector too long" on Windows,
//! "vector::_M_default_append" or "std::bad_alloc" on Linux) are checked against the wheel in
//! `crates/ocio/tests/bake_formats_oracle.rs`.

use super::*;
use ocio_ops::exception::ExceptionKind;

/// A size that fits an `int` is a zeroed buffer of that many floats.
#[test]
fn a_size_that_fits_is_a_buffer() {
    for linux in [LinuxResize::Checked, LinuxResize::Unchecked] {
        let buffer = bake_buffer(cube_values(2), linux).unwrap();
        assert_eq!(buffer.len(), 2 * 2 * 2 * 3);
        assert!(buffer.iter().all(|v| v.to_bits() == 0));
    }
}

/// Sizes whose `int` wraps to a positive value: the cube size 1290 (`1290^3 * 3` wraps to
/// `1290^3 * 3 - 2^32`) and the 1D size `2^32 / 3 + 1` (`* 3` wraps to 2). Upstream writes past
/// its buffer; the port's error, on both platforms.
#[test]
fn sizes_that_wrap_to_a_positive_value_are_errors() {
    let one_d = i128::from((1i64 << 32) / 3 + 1) * 3;
    for count in [cube_values(1290), one_d] {
        assert!(count > i128::from(i32::MAX));
        assert!((count as u32 as i32) > 0);
        for linux in [LinuxResize::Checked, LinuxResize::Unchecked] {
            let e = bake_buffer(count, linux).unwrap_err();
            assert_eq!(e.kind(), ExceptionKind::Exception);
            assert_eq!(
                e.what(),
                format!("The LUT's {count} values overflow the int that sizes its buffer.")
                    .as_bytes()
            );
        }
    }
}

/// Sizes whose `int` wraps to a negative value: a length error on Windows; on Linux a length
/// error where libstdc++ checks the size, `std::bad_alloc` where it doesn't (the texts are
/// checked against the wheels by the oracle tests).
#[test]
fn sizes_that_wrap_to_a_negative_value_are_errors() {
    let one_d = i128::from(715_827_883) * 3;
    for count in [cube_values(895), cube_values(1291), one_d] {
        assert!((count as u32 as i32) < 0);
        for linux in [LinuxResize::Checked, LinuxResize::Unchecked] {
            let e = bake_buffer(count, linux).unwrap_err();
            let expected = if cfg!(target_os = "windows") || linux == LinuxResize::Checked {
                ExceptionKind::LengthError
            } else {
                ExceptionKind::BadAlloc
            };
            assert_eq!(e.kind(), expected);
        }
    }
}
