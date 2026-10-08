// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Discreet 1D LUT format (`.lut`), which the Discreet (now Autodesk) creative finishing
//! products such as Flame and Smoke used: one table of 256 integers, or a `LUT: <tables>
//! <length> [<depth>]` header and 1, 3 or 4 tables. A port of
//! `src/OpenColorIO/fileformats/FileFormatDiscreet1DL.cpp` @ v2.5.2.
//!
//! The reader's buffers are C strings: a line is what `istream::getline` stored, up to its
//! first NUL.

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::bit_depth_utils::get_bit_depth_max_value;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::imath_half::half_to_float;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut1d::lut1d_op_data::HalfFlags;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::pystring::os_path;
use ocio_ops::utils::string_utils::{c_str, lower};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut1d, log_warning_interpolation_not_used};
use crate::fileformats::input_stream::InputStream;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// The size of the reader's line buffer, its NUL included.
const LINE_SIZE: usize = 200;

/// Tabs as spaces, then the spaces at both ends removed.
///
/// Port of `ReplaceTabsAndStripSpaces` (FileFormatDiscreet1DL.cpp:35-71 @ v2.5.2), on the C
/// string `string_to_strip` (whose length the 200-byte buffer keeps within `short`'s range).
pub(crate) fn replace_tabs_and_strip_spaces(string_to_strip: &mut Vec<u8>) {
    for c in string_to_strip.iter_mut() {
        if *c == 9 {
            // TAB
            *c = b' ';
        }
    }
    // Find Last Non Blank, and chop the end.
    let end = string_to_strip
        .iter()
        .rposition(|&c| c != b' ')
        .map_or(0, |i| i + 1);
    string_to_strip.truncate(end);
    // Find First Non Blank, and copy.
    let start = string_to_strip
        .iter()
        .position(|&c| c != b' ')
        .unwrap_or(string_to_strip.len());
    string_to_strip.drain(..start);
}

/// The last character removed when it is a line feed or a carriage return.
///
/// Port of `StripEndNewLine` (FileFormatDiscreet1DL.cpp:73-83 @ v2.5.2).
pub(crate) fn strip_end_new_line(string_to_strip: &mut Vec<u8>) {
    // Test for line feed and CR (both windows and linux)
    if let Some(&last) = string_to_strip.last()
        && (last == 10 || last == 13)
    {
        string_to_strip.pop();
    }
}

/// The bit depths of the format: `IM_LutBitsPerChannel`.
///
/// Port of `Lut1dUtils::IM_LutBitsPerChannel` (FileFormatDiscreet1DL.cpp:93-102 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImLutBitsPerChannel {
    Unknown,
    Bits8,
    Bits10,
    Bits12,
    Bits16,
    Half,
    Float,
}

/// Port of `Lut1dUtils::GetBitDepth` (FileFormatDiscreet1DL.cpp:104-125 @ v2.5.2).
fn get_bit_depth(discreet_bit_depth: ImLutBitsPerChannel) -> BitDepth {
    match discreet_bit_depth {
        ImLutBitsPerChannel::Unknown => BitDepth::Unknown,
        ImLutBitsPerChannel::Bits8 => BitDepth::Uint8,
        ImLutBitsPerChannel::Bits10 => BitDepth::Uint10,
        ImLutBitsPerChannel::Bits12 => BitDepth::Uint12,
        ImLutBitsPerChannel::Bits16 => BitDepth::Uint16,
        ImLutBitsPerChannel::Half => BitDepth::F16,
        ImLutBitsPerChannel::Float => BitDepth::F32,
    }
}

/// The bit depth of a table size: 256, 1024, 4096 or 65536 (half with `is_float`).
///
/// Port of `Lut1dUtils::IMLutTableSizeToBitDepth` (FileFormatDiscreet1DL.cpp:188-209 @
/// v2.5.2).
fn im_lut_table_size_to_bit_depth(table_size: i32, is_float: bool) -> ImLutBitsPerChannel {
    match table_size {
        256 => ImLutBitsPerChannel::Bits8,
        1024 => ImLutBitsPerChannel::Bits10,
        4096 => ImLutBitsPerChannel::Bits12,
        65536 => {
            if is_float {
                ImLutBitsPerChannel::Half
            } else {
                ImLutBitsPerChannel::Bits16
            }
        }
        _ => ImLutBitsPerChannel::Unknown,
    }
}

/// A look-up table descriptor: `numtables` tables of `length` 16-bit entries.
///
/// Port of `Lut1dUtils::IMLutStruct` (FileFormatDiscreet1DL.cpp:128-142, 211-240 @ v2.5.2) and
/// `IMLutAlloc` (FileFormatDiscreet1DL.cpp:254-297): the tables are vectors (upstream's
/// `malloc` leaves them uninitialized; the reader writes every entry it reads before it
/// succeeds).
struct ImLutStruct {
    numtables: i32,
    length: i32,
    src_bit_depth: ImLutBitsPerChannel,
    target_bit_depth: ImLutBitsPerChannel,
    tables: Vec<Vec<u16>>,
}

/// Port of `Lut1dUtils::IMLutAlloc` (FileFormatDiscreet1DL.cpp:254-297 @ v2.5.2); `num` and
/// `length` are already checked (1, 3 or 4 tables of 1 to 65536 entries).
fn im_lut_alloc(num: i32, length: i32) -> ImLutStruct {
    // On import, we never supported LUTs with 16bit integer input.
    // (16bit integer input was interpreted as 12bit.)
    // On export, 16bit input is necessarily float.
    let src16bit_depth_is_float = true;
    ImLutStruct {
        numtables: num,
        length,
        src_bit_depth: im_lut_table_size_to_bit_depth(length, src16bit_depth_is_float),
        // targetBitDepth will be set appropriately for conversion LUTs in IMLutGet
        target_bit_depth: im_lut_table_size_to_bit_depth(length, false),
        tables: vec![vec![0; length as usize]; num as usize],
    }
}

/// The reader's statuses.
///
/// Port of `Lut1dUtils`'s `IMLUT_*` codes (FileFormatDiscreet1DL.cpp:144-151 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImLutStatus {
    UnexpectedEof,
    Syntax,
}

/// The message of a status.
///
/// Port of `Lut1dUtils::IMLutErrorStr` (FileFormatDiscreet1DL.cpp:607-625 @ v2.5.2).
fn im_lut_error_str(errnum: ImLutStatus) -> &'static str {
    match errnum {
        ImLutStatus::UnexpectedEof => "Premature EOF reading LUT file",
        ImLutStatus::Syntax => "Syntax error reading LUT file",
    }
}

/// `std::stoi(text)` on a C string that starts with a digit: its leading digits as an `int`,
/// or `None` where `std::stoi` throws (`strtol`'s `ERANGE`, or a `long` outside `int`'s
/// range): any number above 2^31 - 1.
fn stoi(text: &[u8]) -> Option<i32> {
    let digits = text.iter().take_while(|c| c.is_ascii_digit()).count();
    let mut value: i64 = 0;
    for &d in &text[..digits] {
        value = value * 10 + i64::from(d - b'0');
        if value > i64::from(i32::MAX) {
            return None;
        }
    }
    i32::try_from(value).ok()
}

/// Loads the lines of a table into `ptable` from `ptablestart`, until it is full; blank lines
/// pass. A line that isn't a number, or a number `std::stoi` refuses, is a syntax error (its
/// text in `error_line`); a line that ends the stream, or doesn't fit the buffer, a premature
/// end.
///
/// Port of `tableLoad` (FileFormatDiscreet1DL.cpp:299-344 @ v2.5.2).
fn table_load(
    istream: &mut InputStream,
    ptable: &mut [u16],
    length: i32,
    ptablestart: i32,
    line: &mut i32,
    error_line: &mut Vec<u8>,
) -> std::result::Result<(), ImLutStatus> {
    let mut in_string = Vec::new();
    let mut count = ptablestart;
    while istream.good() {
        *line += 1;
        istream.getline(&mut in_string, LINE_SIZE);
        if !istream.good() {
            return Err(ImLutStatus::UnexpectedEof);
        }
        let len = c_str(&in_string).len();
        in_string.truncate(len);
        replace_tabs_and_strip_spaces(&mut in_string);
        strip_end_new_line(&mut in_string);

        if in_string.first().is_some_and(u8::is_ascii_digit) {
            match stoi(&in_string) {
                Some(v) => {
                    ptable[count as usize] = v as u16;
                    count += 1;
                }
                None => {
                    *error_line = in_string;
                    return Err(ImLutStatus::Syntax);
                }
            }
            if count >= length {
                break;
            }
        } else if !in_string.is_empty() {
            *error_line = in_string;
            return Err(ImLutStatus::Syntax);
        }
    }
    Ok(())
}

/// Reads lines until one that isn't blank or a comment (`#`), which is left in `in_string`;
/// whether the stream is still good.
///
/// Port of `FindNonComment` (FileFormatDiscreet1DL.cpp:346-367 @ v2.5.2).
fn find_non_comment(istream: &mut InputStream, line: &mut i32, in_string: &mut Vec<u8>) -> bool {
    let mut read_on = true;
    while istream.good() && read_on {
        read_on = false;
        istream.getline(in_string, LINE_SIZE);
        *line += 1;
        let len = c_str(in_string).len();
        in_string.truncate(len);
        replace_tabs_and_strip_spaces(in_string);
        strip_end_new_line(in_string);
        if in_string.is_empty() || in_string[0] == b'#' {
            read_on = true;
        }
    }
    istream.good()
}

/// The bit depth after the first "to" in the file's name (lower case): `8`, `10`, `12`,
/// `16`, `16f`, `32f`.
///
/// Port of `Lut1dUtils::IMLutGetBitDepthFromFileName` (FileFormatDiscreet1DL.cpp:536-605 @
/// v2.5.2).
fn im_lut_get_bit_depth_from_file_name(file_name: &[u8]) -> ImLutBitsPerChannel {
    if file_name.is_empty() {
        return ImLutBitsPerChannel::Unknown;
    }
    let lower_file_name = lower(file_name);
    // `strstr` on the C string.
    let lower_file_name = c_str(&lower_file_name);

    // Get the export depth from the LUT name.  Look for a bit depth
    // after the "to" string. (ex: 12to10log).
    if let Some(at) = lower_file_name.windows(2).position(|w| w == b"to") {
        // Skip the "to"; past the end, the C string's NUL.
        let ch = |i: usize| lower_file_name.get(at + 2 + i).copied().unwrap_or(0);
        let first_char = ch(0);
        if first_char == b'8' {
            return ImLutBitsPerChannel::Bits8;
        } else if first_char == b'1' {
            let second_char = ch(1);
            if second_char == b'0' {
                return ImLutBitsPerChannel::Bits10;
            } else if second_char == b'2' {
                return ImLutBitsPerChannel::Bits12;
            } else if second_char == b'6' {
                // check for 16fp
                let third_char = ch(2);
                if third_char == b'f' || third_char == b'F' {
                    return ImLutBitsPerChannel::Half;
                } else {
                    return ImLutBitsPerChannel::Bits16;
                }
            }
        } else if first_char == b'3' {
            let second_char = ch(1);
            if second_char == b'2' {
                let third_char = ch(2);
                if third_char == b'f' || third_char == b'F' {
                    return ImLutBitsPerChannel::Float;
                }
            }
        }
    }
    ImLutBitsPerChannel::Unknown
}

/// Reads a table file: an old one (a first line that is a number: one table of 256), or a
/// `LUT:` header and its tables; the target depth from the header, else the file's name;
/// nothing but blank lines and comments after the tables. On an error, the status, the line
/// it happened at, and for a syntax error the line's text.
///
/// Port of `Lut1dUtils::IMLutGet` (FileFormatDiscreet1DL.cpp:369-534 @ v2.5.2).
fn im_lut_get(
    istream: &mut InputStream,
    file_name: &[u8],
    line: &mut i32,
    error_line: &mut Vec<u8>,
) -> std::result::Result<ImLutStruct, ImLutStatus> {
    let mut in_string = Vec::new();
    let mut depth_scaled = ImLutBitsPerChannel::Unknown;
    *line = 0;

    // Find first line that is not blank or a comment:
    if !find_non_comment(istream, line, &mut in_string) {
        return Err(ImLutStatus::UnexpectedEof);
    }

    let mut lut;
    let tablestart;
    if in_string.first().is_some_and(u8::is_ascii_digit) {
        // Old format LUT file:  1 table of 256 entries.
        lut = im_lut_alloc(1, 256);

        // Load first table value.
        match stoi(&in_string) {
            Some(v) => lut.tables[0][0] = v as u16,
            None => {
                *error_line = in_string;
                return Err(ImLutStatus::Syntax);
            }
        }
        tablestart = 1;
    } else {
        let mut numtables: i32 = 0;
        let mut length: i32 = 0;
        let mut dst_depth_s = [0u8; 16];
        let nummatched = sscanf(
            ScanCrt::NATIVE,
            &in_string,
            b"%*s %d %d %15s",
            &mut [
                ScanArg::Int(&mut numtables),
                ScanArg::Int(&mut length),
                ScanArg::Str(&mut dst_depth_s),
            ],
        );
        // `std::string subStr(InString, 5)`: the buffer's first 5 bytes; a shorter line has
        // its NUL among them, so its lower case can't be "lut: " (docs/improvements.md, I-164).
        let sub_str_matches = in_string.len() >= 5 && lower(&in_string[..5]) == b"lut: ";
        if nummatched < 2
            || !sub_str_matches
            || (numtables != 1 && numtables != 3 && numtables != 4)
            || length <= 0
            || length > 65536
        {
            *error_line = in_string;
            return Err(ImLutStatus::Syntax);
        }

        // New format LUT file:  "numtables" tables, each of
        // "length" entries. Optional dstDepth.
        if nummatched > 2 {
            // Optional dstDepth was specified. Validate it.
            let mut dst_depth: i32 = 0;
            let mut float_c: u8 = b' ';
            sscanf(
                ScanCrt::NATIVE,
                &dst_depth_s,
                b"%d%c",
                &mut [ScanArg::Int(&mut dst_depth), ScanArg::Char(&mut float_c)],
            );

            // Currently when Smoke exports a 16f output depth it uses "65536f"
            // as the third token. However it is likely that earlier versions either
            // wrote only two tokens or wrote the third token without the "f". In that
            // case we may wrongly interpret a 16f outDepth as 16i. We may want to
            // investigate this further at some point.
            depth_scaled =
                im_lut_table_size_to_bit_depth(dst_depth, float_c == b'f' || float_c == b'F');
            if depth_scaled == ImLutBitsPerChannel::Unknown {
                *error_line = in_string;
                return Err(ImLutStatus::Syntax);
            }
        }

        lut = im_lut_alloc(numtables, length);
        tablestart = 0;
    }

    for i in 0..lut.numtables {
        let length = lut.length;
        table_load(
            istream,
            &mut lut.tables[i as usize],
            length,
            tablestart,
            line,
            error_line,
        )?;
    }

    if lut.numtables == 1 {
        lut.numtables = 3;
        let table = lut.tables[0].clone();
        lut.tables.push(table.clone());
        lut.tables.push(table);
    }

    if depth_scaled == ImLutBitsPerChannel::Unknown {
        depth_scaled = im_lut_get_bit_depth_from_file_name(file_name);
    }

    if ImLutBitsPerChannel::Unknown != depth_scaled {
        lut.target_bit_depth = depth_scaled;
    }

    // If there are any more lines in the file that are not blank
    // or comments, it's a syntax error:
    if find_non_comment(istream, line, &mut in_string) {
        *error_line = in_string;
        return Err(ImLutStatus::Syntax);
    }

    Ok(lut)
}

/// A Discreet 1D LUT file's LUT.
///
/// Port of `LocalCachedFile` (FileFormatDiscreet1DL.cpp:627-652 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut1D`.
    pub lut1d: Lut1DOpData,
}

impl LocalCachedFile {
    /// A LUT of `dimension` entries, with a half domain for an F16 input depth, the
    /// interpolation when a 1D LUT takes it, and the output depth.
    ///
    /// Port of `LocalCachedFile::LocalCachedFile` (FileFormatDiscreet1DL.cpp:631-648 @
    /// v2.5.2).
    fn new(
        in_bit_depth: BitDepth,
        out_bit_depth: BitDepth,
        dimension: c_ulong,
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        let half_flags = if in_bit_depth == BitDepth::F16 {
            HalfFlags::INPUT_HALF_CODE
        } else {
            HalfFlags::STANDARD
        };

        let mut lut1d = Lut1DOpData::with_half_flags(half_flags, dimension, false)?;
        if Lut1DOpData::is_valid_interpolation(interp) {
            lut1d.set_interpolation(interp);
        }

        lut1d.set_file_output_bit_depth(out_bit_depth);
        Ok(LocalCachedFile { lut1d })
    }
}

impl CachedFile for LocalCachedFile {}

/// The Discreet 1D LUT format.
///
/// Port of `LocalFileFormat` (FileFormatDiscreet1DL.cpp:655-673 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

impl LocalFileFormat {
    /// Reads a Discreet 1D LUT file; the depth hint comes from the name of `file_path`.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatDiscreet1DL.cpp:685-752 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_path: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        let mut errline = 0;
        let mut error_line = Vec::new();

        let (root, _extension) = os_path::splitext(file_path);
        let file_name = os_path::basename(&root);

        let discreet_lut1d = match im_lut_get(istream, &file_name, &mut errline, &mut error_line) {
            Ok(lut) => lut,
            Err(status) => {
                let mut os = Vec::new();
                os.extend_from_slice(b"Error parsing .lut file (");
                os.extend_from_slice(c_str(file_path));
                os.extend_from_slice(b") ");
                os.extend_from_slice(b"using Discreet 1D LUT reader. ");
                os.extend_from_slice(b"Error is: ");
                os.extend_from_slice(im_lut_error_str(status).as_bytes());
                if status == ImLutStatus::Syntax {
                    os.extend_from_slice(format!(" At line ({errline}): '").as_bytes());
                    os.extend_from_slice(&error_line);
                    os.extend_from_slice(b"'.");
                }
                return Err(Exception::new(os));
            }
        };

        let input_bd = get_bit_depth(discreet_lut1d.src_bit_depth);
        let output_bd = get_bit_depth(discreet_lut1d.target_bit_depth);

        let lut_size = discreet_lut1d.length;

        let mut cached_file =
            LocalCachedFile::new(input_bd, output_bd, lut_size as c_ulong, interp)?;

        let scale = get_bit_depth_max_value(output_bd)? as f32;

        let array = cached_file.lut1d.get_array_mut().get_values_mut();
        let src_table_limit = discreet_lut1d.numtables - 1;
        let mut p = 0;
        for i in 0..lut_size as usize {
            for j in 0..3 {
                let src_table = std::cmp::min(j, src_table_limit) as usize;
                let value = discreet_lut1d.tables[src_table][i];
                if discreet_lut1d.target_bit_depth == ImLutBitsPerChannel::Half {
                    // Convert raw half values to floats.
                    array[p] = half_to_float(value);
                } else {
                    array[p] = f32::from(value) / scale;
                }
                p += 1;
            }
        }

        Ok(cached_file)
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatDiscreet1DL.cpp:675-682 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "Discreet 1D LUT",
            "lut",
            FormatCapabilities::READ,
            FormatBakeCapabilities::NONE,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatDiscreet1DL.cpp:685-752 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_path: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_path, interp)?))
    }

    /// Appends the LUT's op in the direction `dir` combined with the transform's, with the
    /// transform's interpolation when a 1D LUT takes it, else a warning.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatDiscreet1DL.cpp:754-784 @ v2.5.2).
    fn build_file_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        untyped_cached_file: &CachedFileRcPtr,
        file_transform: &FileTransform,
        dir: TransformDirection,
    ) -> Result<()> {
        let any: &dyn Any = untyped_cached_file.as_ref();
        // This should never happen.
        let Some(cached_file) = any.downcast_ref::<LocalCachedFile>() else {
            return Err(Exception::new("Cannot build .lut Op. Invalid cache type."));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());
        let file_interp = file_transform.interpolation();

        let mut file_interp_used = false;
        let lut1d = handle_lut1d(Some(&cached_file.lut1d), file_interp, &mut file_interp_used)
            .expect("the file's LUT");

        if !file_interp_used {
            log_warning_interpolation_not_used(file_interp, file_transform);
        }

        create_lut1d_op(ops, lut1d, new_dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_discreet1dl_tests.rs"]
mod tests;
