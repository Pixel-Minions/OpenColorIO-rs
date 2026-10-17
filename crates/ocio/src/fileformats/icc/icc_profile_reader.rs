// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Translated from SampleICC 1.2.6 (`ext/sampleicc/src/include/iccProfileReader.h` and the
// parts of `icProfileHeader.h` it uses, as OpenColorIO 2.5.2 carries them), under the ICC
// Software License, Version 0.2 (THIRD-PARTY-NOTICES.md):
//
// Copyright (c) 2003-2010 The International Color Consortium. All rights reserved.

//! The part of SampleICC that OCIO's ICC reader uses: big-endian reads, the profile's header
//! and tag table, and the readers of the `XYZ `, `curv`, `para`, `desc` and `mluc` tag types.
//! A port of `ext/sampleicc/src/include/iccProfileReader.h` @ OCIO v2.5.2 (owner decision
//! P4-4), with the constants of `icProfileHeader.h` it reads.
//!
//! The reads go through [`InputStream`] as upstream's go through a binary `std::istream`: a
//! read that fails leaves the stream failed, and a later `seekg` doesn't move it, so later tags
//! fail to load too.

use crate::fileformats::input_stream::InputStream;

/// `icMagicNumber`: `'acsp'`.
pub const IC_MAGIC_NUMBER: u32 = 0x6163_7370;

/// Tag signatures (`icTagSignature`) the reader uses.
pub const IC_SIG_BLUE_COLORANT_TAG: u32 = 0x6258_595A;
/// `'bTRC'`.
pub const IC_SIG_BLUE_TRC_TAG: u32 = 0x6254_5243;
/// `'gXYZ'`.
pub const IC_SIG_GREEN_COLORANT_TAG: u32 = 0x6758_595A;
/// `'gTRC'`.
pub const IC_SIG_GREEN_TRC_TAG: u32 = 0x6754_5243;
/// `'desc'`.
pub const IC_SIG_PROFILE_DESCRIPTION_TAG: u32 = 0x6465_7363;
/// `'dscm'`, Apple's custom tag.
pub const IC_SIG_PROFILE_DESCRIPTION_ML_TAG: u32 = 0x6473_636d;
/// `'rXYZ'`.
pub const IC_SIG_RED_COLORANT_TAG: u32 = 0x7258_595A;
/// `'rTRC'`.
pub const IC_SIG_RED_TRC_TAG: u32 = 0x7254_5243;

/// Tag type signatures (`icTagTypeSignature`): `'curv'`.
pub const IC_SIG_CURVE_TYPE: u32 = 0x6375_7276;
/// `'mluc'`.
pub const IC_SIG_MULTI_LOCALIZED_UNICODE_TYPE: u32 = 0x6D6C_7563;
/// `'para'`.
pub const IC_SIG_PARAMETRIC_CURVE_TYPE: u32 = 0x7061_7261;
/// `'desc'`, removed in version 4.
pub const IC_SIG_TEXT_DESCRIPTION_TYPE: u32 = 0x6465_7363;
/// `'XYZ '`.
pub const IC_SIG_XYZ_ARRAY_TYPE: u32 = 0x5859_5A20;

/// `icSigXYZData`: `'XYZ '`.
pub const IC_SIG_XYZ_DATA: u32 = 0x5859_5A20;
/// `icSigRgbData`: `'RGB '`.
pub const IC_SIG_RGB_DATA: u32 = 0x5247_4220;

/// The profile classes (`icProfileClassSignature`) `Validate` accepts.
const IC_PROFILE_CLASSES: [u32; 7] = [
    0x7363_6E72, // icSigInputClass 'scnr'
    0x6D6E_7472, // icSigDisplayClass 'mntr'
    0x7072_7472, // icSigOutputClass 'prtr'
    0x6C69_6E6B, // icSigLinkClass 'link'
    0x7370_6163, // icSigColorSpaceClass 'spac'
    0x6162_7374, // icSigAbstractClass 'abst'
    0x6E6D_636C, // icSigNamedColorClass 'nmcl'
];

/// `icLanguageCodeEnglish`: `'en'`.
const IC_LANGUAGE_CODE_ENGLISH: u16 = 0x656E;
/// `icCountryCodeUSA`: `'US'`.
const IC_COUNTRY_CODE_USA: u16 = 0x5553;
/// `icCountryCodeUnitedKingdom`: `'UK'`.
const IC_COUNTRY_CODE_UNITED_KINGDOM: u16 = 0x554B;

/// `icS15Fixed16Number`.
pub type IcS15Fixed16Number = i32;

/// `icXYZNumber`.
///
/// Port of `icXYZNumber` (icProfileHeader.h:830-834 @ OCIO v2.5.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IcXyzNumber {
    /// `X`.
    pub x: IcS15Fixed16Number,
    /// `Y`.
    pub y: IcS15Fixed16Number,
    /// `Z`.
    pub z: IcS15Fixed16Number,
}

/// `icDateTimeNumber`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IcDateTimeNumber {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hours: u16,
    pub minutes: u16,
    pub seconds: u16,
}

/// The profile's header, as `ReadInfo` reads it.
///
/// Port of `icHeader` (icProfileHeader.h:1402-1421 @ OCIO v2.5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IcHeader {
    pub size: u32,
    pub cmm_id: u32,
    pub version: u32,
    pub device_class: u32,
    pub color_space: u32,
    pub pcs: u32,
    pub date: IcDateTimeNumber,
    pub magic: u32,
    pub platform: u32,
    pub flags: u32,
    pub manufacturer: u32,
    pub model: u32,
    pub attributes: u64,
    pub rendering_intent: u32,
    pub illuminant: IcXyzNumber,
    pub creator: u32,
    pub profile_id: [u8; 16],
    pub reserved: [u8; 28],
}

/// An entry of the tag table.
///
/// Port of `icTag` (icProfileHeader.h:1380-1386 @ OCIO v2.5.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IcTag {
    pub sig: u32,
    pub offset: u32,
    pub size: u32,
}

/// Swaps the bytes of each 8-byte value of `buf`, for `num` values.
///
/// Port of `SampleICC::Swap64Array` (iccProfileReader.h:99-112 @ OCIO v2.5.2).
pub fn swap64_array(buf: &mut [u8], num: i32) {
    for chunk in buf
        .as_chunks_mut::<8>()
        .0
        .iter_mut()
        .take(num.max(0) as usize)
    {
        chunk.reverse();
    }
}

/// Swaps the bytes of each 4-byte value of `buf`, for `num` values.
///
/// Port of `SampleICC::Swap32Array` (iccProfileReader.h:114-125 @ OCIO v2.5.2).
pub fn swap32_array(buf: &mut [u8], num: i32) {
    for chunk in buf
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .take(num.max(0) as usize)
    {
        chunk.reverse();
    }
}

/// Swaps the bytes of each 2-byte value of `buf`, for `num` values.
///
/// Port of `SampleICC::Swap16Array` (iccProfileReader.h:127-137 @ OCIO v2.5.2).
pub fn swap16_array(buf: &mut [u8], num: i32) {
    for chunk in buf
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .take(num.max(0) as usize)
    {
        chunk.reverse();
    }
}

/// An S15.16 number as a float, through a double.
///
/// Port of `SampleICC::icFtoD` (iccProfileReader.h:139-142 @ OCIO v2.5.2).
pub fn ic_f_to_d(num: IcS15Fixed16Number) -> f32 {
    (f64::from(num) / 65536.0) as f32
}

/// Reads `num` bytes into `buf`; `num` when the stream is still good after, else 0 (and
/// nothing when it isn't good before).
///
/// Port of `SampleICC::Read8` (iccProfileReader.h:144-156 @ OCIO v2.5.2).
pub fn read8(istream: &mut InputStream, buf: &mut [u8], num: i32) -> i32 {
    if !istream.good() {
        return 0;
    }
    istream.read(buf, num as usize);
    if !istream.good() {
        return 0;
    }
    num
}

/// Reads `num` big-endian 32-bit values.
///
/// Port of `SampleICC::Read32` (iccProfileReader.h:166-172 @ OCIO v2.5.2), for one value.
pub fn read32(istream: &mut InputStream, value: &mut u32) -> bool {
    let mut buf = value.to_ne_bytes();
    let num = read8(istream, &mut buf, 4) >> 2;
    swap32_array(&mut buf, num);
    *value = u32::from_ne_bytes(buf);
    num == 1
}

/// [`read32`] of a signed value.
pub fn read32_signed(istream: &mut InputStream, value: &mut i32) -> bool {
    let mut v = *value as u32;
    let ok = read32(istream, &mut v);
    *value = v as i32;
    ok
}

/// Reads one big-endian 16-bit value.
///
/// Port of `SampleICC::Read16` (iccProfileReader.h:174-180 @ OCIO v2.5.2), for one value.
pub fn read16(istream: &mut InputStream, value: &mut u16) -> bool {
    let mut buf = value.to_ne_bytes();
    let num = read8(istream, &mut buf, 2) >> 1;
    swap16_array(&mut buf, num);
    *value = u16::from_ne_bytes(buf);
    num == 1
}

/// Reads one big-endian 64-bit value.
///
/// Port of `SampleICC::Read64` (iccProfileReader.h:158-164 @ OCIO v2.5.2), for one value.
pub fn read64(istream: &mut InputStream, value: &mut u64) -> bool {
    let mut buf = value.to_ne_bytes();
    let num = read8(istream, &mut buf, 8) >> 3;
    swap64_array(&mut buf, num);
    *value = u64::from_ne_bytes(buf);
    num == 1
}

/// Reads `count` bytes, in pieces so that what a profile claims can't allocate more than the
/// file holds: the bytes when the stream had them all and is still good, else `None` (the
/// stream then failed at its end, as one `istream::read` of `count` bytes leaves it).
fn read_bytes(istream: &mut InputStream, count: u64) -> Option<Vec<u8>> {
    if !istream.good() {
        return None;
    }
    const PIECE: u64 = 1 << 16;
    let mut out = Vec::new();
    let mut left = count;
    let mut piece = vec![0u8; PIECE as usize];
    while left > 0 {
        let n = left.min(PIECE) as usize;
        istream.read(&mut piece, n);
        out.extend_from_slice(&piece[..istream.gcount()]);
        if !istream.good() {
            return None;
        }
        left -= n as u64;
    }
    Some(out)
}

/// A tag's reader.
///
/// Port of `SampleICC::IccTypeReader` and its subclasses (iccProfileReader.h:200-612 @ OCIO
/// v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub enum IccTypeReader {
    /// `IccTextDescriptionTypeReader`: the text.
    TextDescription(Vec<u8>),
    /// `IccMultiLocalizedUnicodeTypeReader`: the one string it picks.
    MultiLocalizedUnicode(Vec<u8>),
    /// `IccXYZArrayTypeReader`: the one XYZ.
    XyzArray(IcXyzNumber),
    /// `IccParametricCurveTypeReader`: the function type and its parameters.
    ParametricCurve {
        /// `mFunctionType`.
        function_type: u16,
        /// `mParam`, `mnNumParam` of them.
        params: Vec<IcS15Fixed16Number>,
    },
    /// `IccCurveTypeReader`: the entries over 65535.
    Curve(Vec<f32>),
}

/// The kinds of tag types the reader knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeKind {
    TextDescription,
    MultiLocalizedUnicode,
    XyzArray,
    ParametricCurve,
    Curve,
}

/// The reader of a tag type signature, or `None` for one SampleICC doesn't read here.
///
/// Port of `SampleICC::IccTypeReader::Create` (iccProfileReader.h:614-628 @ OCIO v2.5.2).
fn create(sig_type: u32) -> Option<TypeKind> {
    if IC_SIG_XYZ_ARRAY_TYPE == sig_type {
        Some(TypeKind::XyzArray)
    } else if IC_SIG_PARAMETRIC_CURVE_TYPE == sig_type {
        Some(TypeKind::ParametricCurve)
    } else if IC_SIG_CURVE_TYPE == sig_type {
        Some(TypeKind::Curve)
    } else if IC_SIG_TEXT_DESCRIPTION_TYPE == sig_type {
        Some(TypeKind::TextDescription)
    } else if IC_SIG_MULTI_LOCALIZED_UNICODE_TYPE == sig_type {
        Some(TypeKind::MultiLocalizedUnicode)
    } else {
        None
    }
}

/// The text up to its first NUL.
fn up_to_nul(mut text: Vec<u8>) -> Vec<u8> {
    if let Some(pos) = text.iter().position(|&c| c == 0) {
        text.truncate(pos);
    }
    text
}

/// Port of `IccTextDescriptionTypeReader::Read` (iccProfileReader.h:225-282 @ OCIO v2.5.2).
/// A text size past what the file holds fails the read without allocating it (upstream
/// allocates the size first: docs/improvements.md, U-70).
fn read_text_description(istream: &mut InputStream, size: u32) -> Option<IccTypeReader> {
    // Note that tag size include sig that has already been read.
    if 4 + 4 + 4 > size {
        return None;
    }
    if !istream.good() {
        return None;
    }
    let mut reserved = 0u32;
    if !read32(istream, &mut reserved) {
        return None;
    }
    // Read the size of the string.
    let mut text_size = 0u32;
    if !read32(istream, &mut text_size) {
        return None;
    }
    // Read the string itself.
    let mut text = Vec::new();
    if text_size != 0 {
        // `Read8(istream, buf, textSize)`: the size as an `icInt32Number`. `istream::read` of
        // a negative count reads nothing; MSVC's leaves the stream good, so `Read8` returns
        // the count, which is `textSize`, and the text (`textSize + 1` NULs, or none for
        // 2^32 - 1) is empty; libstdc++'s fails the stream (I-166).
        let read = if (text_size as i32) < 0 {
            if cfg!(windows) {
                Some(Vec::new())
            } else {
                read_bytes(istream, u64::MAX)
            }
        } else {
            read_bytes(istream, u64::from(text_size))
        };
        // Removes extra '\0' if any.
        text = up_to_nul(read?);
    }
    Some(IccTypeReader::TextDescription(text))
}

/// Port of `IccMultiLocalizedUnicodeTypeReader::Read` (iccProfileReader.h:298-436 @ OCIO
/// v2.5.2): the records' strings are read where each record ends, not at their offsets (as
/// upstream does). A string past what the file holds fails the read without allocating it
/// (U-70).
fn read_multi_localized_unicode(istream: &mut InputStream, size: u32) -> Option<IccTypeReader> {
    // Note that tag size include sig that has already been read.
    if 4 + 4 * 3 > size {
        return None;
    }
    if !istream.good() {
        return None;
    }
    let (mut reserved, mut n_num_rec, mut n_rec_size) = (0u32, 0u32, 0u32);
    if !read32(istream, &mut reserved)
        || !read32(istream, &mut n_num_rec)
        || !read32(istream, &mut n_rec_size)
    {
        return None;
    }
    // Recognized version name records are 12 bytes each.
    if n_rec_size != 12 {
        return None;
    }

    let mut found_country_usa = Vec::new();
    let mut found_country_uk = Vec::new();
    let mut found_language_en = Vec::new();
    let mut found_first_entry = Vec::new();

    let mut i: u32 = 0;
    while i < n_num_rec {
        // `(4 * sizeof(icUInt32Number) + (i+1) * 12) > size`: `(i+1) * 12` in 32 bits (it
        // wraps), the sum in `size_t`.
        if 16 + u64::from((i + 1).wrapping_mul(12)) > u64::from(size) {
            return None;
        }
        let (mut n_language_code, mut n_region_code) = (0u16, 0u16);
        let (mut n_length, mut n_offset) = (0u32, 0u32);
        if !read16(istream, &mut n_language_code)
            || !read16(istream, &mut n_region_code)
            || !read32(istream, &mut n_length)
            || !read32(istream, &mut n_offset)
        {
            return None;
        }
        // `(nOffset + nLength) > size`, in 32 bits.
        if n_offset.wrapping_add(n_length) > size {
            return None;
        }
        let n_num_char = n_length / 2;
        // `Read16(istream, &unicodeStr[0], nNumChar)`: `nNumChar << 1` bytes as an
        // `icInt32Number`, which wraps for 2^30 characters or more (I-165).
        let bytes = (n_num_char as i32).wrapping_shl(1);
        if bytes < 0 {
            // `istream::read` of a negative count reads nothing: MSVC's leaves the stream
            // good, libstdc++'s fails it (I-166). `Read16` then returns `bytes >> 1`, never
            // `nNumChar`.
            if !cfg!(windows) {
                let _ = read_bytes(istream, u64::MAX);
            }
            return None;
        }
        let raw = read_bytes(istream, bytes as u64)?;
        if (raw.len() / 2) as u32 != n_num_char {
            return None;
        }
        // As only the English language is supported the Basic Latin character set is
        // enough for all the English characters.
        let text: Vec<u8> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c) as u8)
            .collect();
        // Removes extra '\0' if any.
        let text = up_to_nul(text);

        // As the order of the (country, language) is unknown, read all the strings before
        // selecting the right one.
        if n_region_code == IC_COUNTRY_CODE_USA {
            // As soon as the US is found stop.
            found_country_usa = text;
            break;
        }
        if n_region_code == IC_COUNTRY_CODE_UNITED_KINGDOM && found_country_uk.is_empty() {
            found_country_uk = text.clone();
        }
        if n_language_code == IC_LANGUAGE_CODE_ENGLISH && found_language_en.is_empty() {
            found_language_en = text.clone();
        }
        if i == 0 {
            found_first_entry = text;
        }
        i += 1;
    }

    let text = if !found_country_usa.is_empty() {
        found_country_usa
    } else if !found_country_uk.is_empty() {
        found_country_uk
    } else if !found_language_en.is_empty() {
        found_language_en
    } else {
        found_first_entry
    };
    Some(IccTypeReader::MultiLocalizedUnicode(text))
}

/// Port of `IccXYZArrayTypeReader::Read` (iccProfileReader.h:449-474 @ OCIO v2.5.2).
fn read_xyz_array(istream: &mut InputStream, size: u32) -> Option<IccTypeReader> {
    // Tag size include sig that has already been read.
    if 4 + 4 + 12 > size || !istream.good() {
        return None;
    }
    let size_comp = (size - 8) / 12;
    // only used to read single XYZ
    if size_comp != 1 {
        return None;
    }
    let mut res = 0u32;
    if !read32(istream, &mut res) {
        return None;
    }
    // `Read32(istream, &mXYZ, 3)`: the three numbers in one read.
    let mut buf = [0u8; 12];
    let num = read8(istream, &mut buf, 12) >> 2;
    if num != 3 {
        return None;
    }
    let value = |i: usize| i32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
    Some(IccTypeReader::XyzArray(IcXyzNumber {
        x: value(0),
        y: value(4),
        z: value(8),
    }))
}

/// Port of `IccParametricCurveTypeReader::Read` (iccProfileReader.h:498-540 @ OCIO v2.5.2).
fn read_parametric_curve(istream: &mut InputStream, size: u32) -> Option<IccTypeReader> {
    // Tag size include sig that has already been read.
    let n_hdr_size: u32 = 4 + 4 + 2 * 2;
    if n_hdr_size > size {
        return None;
    }
    if n_hdr_size + 4 > size {
        return None;
    }
    if !istream.good() {
        return None;
    }
    let (mut res32, mut function_type, mut res16) = (0u32, 0u16, 0u16);
    if !read32(istream, &mut res32)
        || !read16(istream, &mut function_type)
        || !read16(istream, &mut res16)
    {
        return None;
    }
    let mn_num_param = ((size - n_hdr_size) / 4) as u16;
    let mut params = Vec::with_capacity(usize::from(mn_num_param));
    if mn_num_param != 0 {
        if u64::from(n_hdr_size) + u64::from(mn_num_param) * 4 > u64::from(size) {
            return None;
        }
        for _ in 0..mn_num_param {
            let mut p = 0i32;
            if !read32_signed(istream, &mut p) {
                return None;
            }
            params.push(p);
        }
    }
    Some(IccTypeReader::ParametricCurve {
        function_type,
        params,
    })
}

/// Port of `IccCurveTypeReader::Read` (iccProfileReader.h:569-602 @ OCIO v2.5.2) and
/// `Read16Float` (iccProfileReader.h:182-196).
fn read_curve(istream: &mut InputStream, size: u32) -> Option<IccTypeReader> {
    // Tag size include sig that has already been read.
    if 4 + 4 + 4 > size {
        return None;
    }
    if !istream.good() {
        return None;
    }
    let mut res32 = 0u32;
    if !read32(istream, &mut res32) {
        return None;
    }
    let mut size_data = 0u32;
    if !read32(istream, &mut size_data) {
        return None;
    }
    // ICC curve entries are indexed by 16-bit values; 65536 is the maximum.
    if size_data > 65536 {
        return None;
    }
    let mut curve = Vec::with_capacity(size_data as usize);
    for _ in 0..size_data {
        let mut tmp = 0u16;
        if !read16(istream, &mut tmp) {
            return None;
        }
        curve.push(f32::from(tmp) / 65535.0f32);
    }
    Some(IccTypeReader::Curve(curve))
}

impl IccTypeReader {
    /// Whether it is a parametric curve's reader.
    ///
    /// Port of `IccTypeReader::IsParametricCurve` (iccProfileReader.h:211, 496 @ OCIO v2.5.2).
    pub fn is_parametric_curve(&self) -> bool {
        matches!(self, IccTypeReader::ParametricCurve { .. })
    }
}

/// A tag of the table, and its reader once loaded.
///
/// Port of `SampleICC::IccTagElement` (iccProfileReader.h:630-636 @ OCIO v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct IccTagElement {
    /// `mTagInfo`.
    pub tag_info: IcTag,
    /// `mTagReader`.
    pub tag_reader: Option<IccTypeReader>,
}

/// A profile's header and tags.
///
/// Port of `SampleICC::IccContent` (iccProfileReader.h:639-807 @ OCIO v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct IccContent {
    /// `mHeader`.
    pub header: IcHeader,
    /// `mTags`.
    pub tags: Vec<IccTagElement>,
}

impl IccContent {
    /// The first tag of the signature.
    fn find_tag(&self, sig: u32) -> Option<usize> {
        self.tags.iter().position(|t| t.tag_info.sig == sig)
    }

    /// Port of `IccContent::HasTag` (iccProfileReader.h:689-692 @ OCIO v2.5.2).
    pub fn has_tag(&self, sig: u32) -> bool {
        self.find_tag(sig).is_some()
    }

    /// Port of `IccContent::isMatrixShaper` (iccProfileReader.h:650-658 @ OCIO v2.5.2).
    pub fn is_matrix_shaper(&self) -> bool {
        self.has_tag(IC_SIG_RED_COLORANT_TAG)
            && self.has_tag(IC_SIG_GREEN_COLORANT_TAG)
            && self.has_tag(IC_SIG_BLUE_COLORANT_TAG)
            && self.has_tag(IC_SIG_RED_TRC_TAG)
            && self.has_tag(IC_SIG_GREEN_TRC_TAG)
            && self.has_tag(IC_SIG_BLUE_TRC_TAG)
    }

    /// The reader of the first tag of the signature, loaded the first time: the stream moves
    /// to the tag's offset, reads its type signature, and the type's reader reads the tag;
    /// `None` when there is no such tag, the type isn't one SampleICC reads here, or the read
    /// fails (a later call tries again).
    ///
    /// Port of `IccContent::LoadTag` (iccProfileReader.h:694-726 @ OCIO v2.5.2).
    pub fn load_tag(&mut self, istream: &mut InputStream, sig: u32) -> Option<&IccTypeReader> {
        let index = self.find_tag(sig)?;
        if self.tags[index].tag_reader.is_none() {
            let info = self.tags[index].tag_info;
            istream.seekg(u64::from(info.offset));
            if istream.good() {
                let mut sig_type = 0u32;
                if read32(istream, &mut sig_type)
                    && let Some(kind) = create(sig_type)
                {
                    // Read; remember tag if read ok.
                    self.tags[index].tag_reader = match kind {
                        TypeKind::TextDescription => read_text_description(istream, info.size),
                        TypeKind::MultiLocalizedUnicode => {
                            read_multi_localized_unicode(istream, info.size)
                        }
                        TypeKind::XyzArray => read_xyz_array(istream, info.size),
                        TypeKind::ParametricCurve => read_parametric_curve(istream, info.size),
                        TypeKind::Curve => read_curve(istream, info.size),
                    };
                }
            }
        }
        self.tags[index].tag_reader.as_ref()
    }

    /// The critical issues of the header and the tag table, as text.
    ///
    /// Port of `IccContent::Validate` (iccProfileReader.h:728-772 @ OCIO v2.5.2). The class
    /// prints as an `int`, as an enum whose enumerators all fit one promotes; the rendering
    /// intent, an `icUInt32Number` in the header, as an `unsigned int`.
    pub fn validate(&self) -> Result<(), Vec<u8>> {
        if !IC_PROFILE_CLASSES.contains(&self.header.device_class) {
            return Err(format!(
                "Unknown profile class: {}. ",
                self.header.device_class as i32
            )
            .into_bytes());
        }
        if self.header.rendering_intent > 3 {
            return Err(format!(
                "Unknown rendering intent: {}. ",
                self.header.rendering_intent
            )
            .into_bytes());
        }
        if self.tags.is_empty() {
            return Err(b"No tags present. ".to_vec());
        }
        Ok(())
    }

    /// [`validate`](Self::validate), then the model OCIO reads: matrix/TRC, the XYZ PCS, RGB.
    ///
    /// Port of `IccContent::ValidateForOCIO` (iccProfileReader.h:774-806 @ OCIO v2.5.2).
    pub fn validate_for_ocio(&self) -> Result<(), Vec<u8>> {
        self.validate()?;
        // Only the Matrix/TRC Model is supported for now
        if !self.is_matrix_shaper() {
            return Err(b"Only Matrix/TRC Model is supported. ".to_vec());
        }
        // Matrix/TRC profiles only use the XYZ PCS
        if self.header.pcs != IC_SIG_XYZ_DATA {
            return Err(b"Unsupported ICC profile connection space. ".to_vec());
        }
        if self.header.color_space != IC_SIG_RGB_DATA {
            return Err(b"Unsupported ICC device color space. ".to_vec());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "icc_profile_reader_tests.rs"]
mod tests;
