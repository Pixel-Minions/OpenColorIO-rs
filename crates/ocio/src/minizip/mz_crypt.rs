// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's CRC-32 (`mz_crypt.c`).

/// The table of the CRC-32 of the zip format (polynomial 0xEDB88320, reflected), built as
/// `crc32_table` (mz_crypt.c:51-80 @ 4.0.10) lists it.
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// `value` updated with the CRC-32 of `buf`.
///
/// Port of `mz_crypt_crc32_update` (mz_crypt.c:35-89 @ 4.0.10): OpenColorIO's minizip-ng calls
/// zlib's `crc32`, which computes the same CRC as the table version ported here.
pub(crate) fn mz_crypt_crc32_update(value: u32, buf: &[u8]) -> u32 {
    let mut value = !value;
    for &b in buf {
        value = (value >> 8) ^ CRC32_TABLE[((value ^ u32::from(b)) & 0xff) as usize];
    }
    !value
}
