// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the SampleICC subset: `types` and `endian` of
//! `tests/cpu/fileformats/FileFormatICC_tests.cpp` @ v2.5.2.

use super::*;

/// Port of `OCIO_ADD_TEST(FileFormatICC, types)` @ v2.5.2.
#[test]
fn types() {
    // Test types used
    assert_eq!(1, std::mem::size_of::<u8>());
    assert_eq!(2, std::mem::size_of::<u16>());
    assert_eq!(4, std::mem::size_of::<u32>());

    assert_eq!(4, std::mem::size_of::<i32>());

    assert_eq!(4, std::mem::size_of::<IcS15Fixed16Number>());
}

/// Port of `OCIO_ADD_TEST(FileFormatICC, endian)` @ v2.5.2.
#[test]
fn endian() {
    let mut test = [0x11u8, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];

    swap32_array(&mut test, 2);

    assert_eq!(test[0], 0x44);
    assert_eq!(test[1], 0x33);
    assert_eq!(test[2], 0x22);
    assert_eq!(test[3], 0x11);

    assert_eq!(test[4], 0x88);
    assert_eq!(test[5], 0x77);
    assert_eq!(test[6], 0x66);
    assert_eq!(test[7], 0x55);

    swap16_array(&mut test, 4);

    assert_eq!(test[0], 0x33);
    assert_eq!(test[1], 0x44);

    assert_eq!(test[2], 0x11);
    assert_eq!(test[3], 0x22);

    assert_eq!(test[4], 0x77);
    assert_eq!(test[5], 0x88);

    assert_eq!(test[6], 0x55);
    assert_eq!(test[7], 0x66);

    swap64_array(&mut test, 1);

    assert_eq!(test[7], 0x33);
    assert_eq!(test[6], 0x44);
    assert_eq!(test[5], 0x11);
    assert_eq!(test[4], 0x22);
    assert_eq!(test[3], 0x77);
    assert_eq!(test[2], 0x88);
    assert_eq!(test[1], 0x55);
    assert_eq!(test[0], 0x66);
}
