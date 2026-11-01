// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the multilocalized unicode part of `src/cmsnamed.c` @ lcms2.17, as far as the tool's
//! description and copyright reach it.
//!
//! C keeps `wchar_t`s in the pool: 2 bytes on Windows, 4 on Linux. `Type_MLU_Write` writes each
//! as 16 bits and converts the lengths and offsets back, so the file doesn't show it. Here the
//! pool holds the 16 bits that get written, and lengths and offsets count them.
//!
//! The named color lists of the file aren't ported: a profile's `clrt` tag is read and the list
//! is duplicated into the transform, but nothing looks at it (`Type_ColorantTable_Read`).

/// `_cmsMLUentry`.
#[derive(Debug, Clone)]
pub struct MluEntry {
    pub language: u16,
    pub country: u16,
    /// Where the string starts in the pool.
    pub str_w: u32,
    /// How long it is.
    pub len: u32,
}

/// `cmsMLU`.
#[derive(Debug, Clone, Default)]
pub struct Mlu {
    pub entries: Vec<MluEntry>,
    pub pool: Vec<u16>,
}

/// Port of `SearchMLUEntry` (src/cmsnamed.c:118-135 @ lcms2.17).
fn search_mlu_entry(mlu: &Mlu, language_code: u16, country_code: u16) -> bool {
    mlu.entries
        .iter()
        .any(|entry| entry.country == country_code && entry.language == language_code)
}

/// Port of `AddMLUBlock` (src/cmsnamed.c:139-179 @ lcms2.17).
fn add_mlu_block(mlu: &mut Mlu, block: &[u16], language_code: u16, country_code: u16) -> bool {
    // Only one ASCII string
    if search_mlu_entry(mlu, language_code, country_code) {
        return false; // Only one  is allowed!
    }

    let offset = mlu.pool.len() as u32;

    // Set the entry
    mlu.pool.extend_from_slice(block);

    mlu.entries.push(MluEntry {
        language: language_code,
        country: country_code,
        str_w: offset,
        len: block.len() as u32,
    });

    true
}

/// Port of `strTo16` (src/cmsnamed.c:183-195 @ lcms2.17).
fn str_to_16(str_: &[u8; 2]) -> u16 {
    ((str_[0] as u16) << 8) | str_[1] as u16
}

/// Port of `cmsMLUsetASCII` (src/cmsnamed.c:336-364 @ lcms2.17). `ascii_string` is a C string, as the
/// command line gave it.
///
/// `(wchar_t) ASCIIString[i]` goes through a signed `char`, on both platforms, so a byte over
/// 0x7F takes its sign: 0xC3 becomes 0xFFC3 in the 16 bits that are written.
pub fn cms_mlu_set_ascii(
    mlu: &mut Mlu,
    language_code: &[u8; 2],
    country_code: &[u8; 2],
    ascii_string: &[u8],
) -> bool {
    let lang = str_to_16(language_code);
    let cntry = str_to_16(country_code);

    // len == 0 would prevent operation, so we set a empty string pointing to zero
    if ascii_string.is_empty() {
        return add_mlu_block(mlu, &[0], lang, cntry);
    }

    let wstr: Vec<u16> = ascii_string
        .iter()
        .map(|&c| c as i8 as i16 as u16)
        .collect();

    add_mlu_block(mlu, &wstr, lang, cntry)
}
