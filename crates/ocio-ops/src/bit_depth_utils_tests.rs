// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/BitDepthUtils_tests.cpp` @ v2.5.2, plus the one value of
//! `GetBitdepthFromMaxValue` that upstream's tests check. The wheel's side is in
//! `tests/bit_depth_utils_oracle.rs`.

use super::*;

/// `OCIO_CHECK_THROW_WHAT(S, OCIO::Exception, W)`: `S` throws an `OCIO::Exception` (which
/// `ExceptionMissingFile` also is) whose `what()` contains `W`; an empty `what()` or `W`
/// fails.
///
/// Port of `OCIO_CHECK_THROW_WHAT` (tests/testutils/UnitTest.h:237-250 @ v2.5.2).
#[track_caller]
fn check_throw_what<T: Debug>(result: Result<T>, what: &str) {
    match result {
        Ok(value) => panic!("OCIO::Exception is expected to be thrown, got Ok({value:?})"),
        Err(ex) => assert!(
            !what.is_empty() && !ex.message().is_empty() && ex.message().contains(what),
            "OCIO::Exception was thrown with \"{}\". Expecting to contain \"{what}\"",
            ex.message()
        ),
    }
}

/// Port of `OCIO_ADD_TEST(BitDepthUtils, get_bitdepth_max_value)` @ v2.5.2.
#[test]
fn get_bitdepth_max_value() {
    assert_eq!(get_bit_depth_max_value(BitDepth::Uint8).unwrap(), 255.0);
    assert_eq!(get_bit_depth_max_value(BitDepth::Uint16).unwrap(), 65535.0);

    assert_eq!(get_bit_depth_max_value(BitDepth::F16).unwrap(), 1.0);
    assert_eq!(get_bit_depth_max_value(BitDepth::F32).unwrap(), 1.0);
}

/// Port of `OCIO_ADD_TEST(BitDepthUtils, is_float_bitdepth)` @ v2.5.2.
#[test]
fn is_float_bitdepth() {
    assert!(!is_float_bit_depth(BitDepth::Uint8).unwrap());
    assert!(!is_float_bit_depth(BitDepth::Uint10).unwrap());
    assert!(!is_float_bit_depth(BitDepth::Uint12).unwrap());
    assert!(!is_float_bit_depth(BitDepth::Uint16).unwrap());

    assert!(is_float_bit_depth(BitDepth::F16).unwrap());
    assert!(is_float_bit_depth(BitDepth::F32).unwrap());

    check_throw_what(is_float_bit_depth(BitDepth::Uint14), "not supported");

    check_throw_what(is_float_bit_depth(BitDepth::Uint32), "not supported");
}

/// Port of `OCIO_ADD_TEST(BitDepthUtils, get_channel_size)` @ v2.5.2. `half` is Imath's type
/// upstream; `half::f16` stands for it here.
#[test]
fn get_channel_size() {
    // The unsigned result compared with a size_t, as C++ promotes it.
    let size = |bit_depth| get_channel_size_in_bytes(bit_depth).map(|s| s as usize);

    assert_eq!(size(BitDepth::Uint8).unwrap(), size_of::<u8>());

    assert_eq!(size(BitDepth::F16).unwrap(), size_of::<half::f16>());

    check_throw_what(
        get_channel_size_in_bytes(BitDepth::Uint14),
        "Bit depth is not supported: 14ui.",
    );
}

/// `GetBitdepthFromMaxValue` reads a Pandora file's `out 256` as 8-bit: upstream's
/// `FileFormatPandora/load_op` test (tests/cpu/fileformats/FileFormatPandora_tests.cpp:149-165
/// @ v2.5.2) checks `getFileOutputBitDepth() == BIT_DEPTH_UINT8` for
/// `tests/data/files/pandora_3d.m3d`, whose line 3 is `out 256`, and the Pandora reader sets
/// that bit depth to `GetBitdepthFromMaxValue(out)`
/// (src/OpenColorIO/fileformats/FileFormatPandora.cpp:277-278 @ v2.5.2).
#[test]
fn bitdepth_from_max_value_of_the_pandora_test_file() {
    assert_eq!(get_bitdepth_from_max_value(256), BitDepth::Uint8);
}
