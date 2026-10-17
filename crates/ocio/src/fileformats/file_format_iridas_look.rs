// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Iridas .look format: an XML file whose `<LUT>` section holds a 3D LUT as hex-encoded
//! little-endian 32-bit floats, red fastest. A port of
//! `src/OpenColorIO/fileformats/FileFormatIridasLook.cpp` @ v2.5.2.
//!
//! ```text
//! <?xml version="1.0" ?>
//! <look>
//!   <shaders>
//!     # anything in here is useless to us
//!   </shaders>
//!   <LUT>
//!     <size>"8"</size> # Size of 3D LUT
//!     <data>"
//!       0000008000000080000000802CF52E3D2DF52E3D2DF52E3D2CF5AE3D2DF5AE3D
//!       # ...cut...
//!       5A216A3F5A216A3FAD10753FAD10753FAD10753F0000803F0000803F0000803F"
//!     </data>
//!   </LUT>
//! </look>
//! ```
//!
//! The data is read by removing all spaces, quotes and line feeds, then taking 8 characters at
//! a time as the bytes of a little-endian float: "0000003F" is 0.5, "0000803F" 1.0.
//!
//! Upstream's `XMLParserHelper` is expat's user data, and its static handlers reach it through
//! a pointer. Here its state is shared (`Rc<RefCell<State>>`) between the parser and the
//! closures that are expat's handlers; a handler's exception is its `Err`, which ends
//! `XML_Parse` and is returned as is, as upstream's C++ exceptions propagate.

use std::any::Any;
use std::cell::RefCell;
use std::ffi::c_ulong;
use std::rc::Rc;
use std::sync::Arc;

use ocio_formats::expat::xmlparse::{ParseError, Parser, XmlError, XmlStatus, xml_error_string};
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op::create_lut3d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_long};
use ocio_ops::utils::pystring;
use ocio_ops::utils::string_utils::replace_in_place;
use ocio_ops::{Exception, Result};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut3d, log_warning_interpolation_not_used};
use crate::fileformats::input_stream::InputStream;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// Converts a hex ASCII digit to its value: `None` (upstream's `false`, with 0 stored) for any
/// other character.
///
/// Port of `hexasciitoint` (FileFormatIridasLook.cpp:69-91 @ v2.5.2).
pub fn hexasciitoint(character: u8) -> Option<u8> {
    if (48..=57).contains(&character) {
        // [0-9]
        Some(character - 48)
    } else if (65..=70).contains(&character) {
        // [A-F]
        Some(10 + character - 65)
    } else if (97..=102).contains(&character) {
        // [a-f]
        Some(10 + character - 97)
    } else {
        None
    }
}

/// Converts 8 hex ASCII digits, the little-endian bytes of an `f32` as the Iridas format
/// writes them, to the float: "AD10753F" is 0.9572857022285461f on all architectures. `None`
/// when one isn't a hex digit.
///
/// Port of `hexasciitofloat` (FileFormatIridasLook.cpp:93-130 @ v2.5.2), its little-endian
/// branch (the reference platforms are little endian).
pub fn hexasciitofloat(ascii: &[u8; 8]) -> Option<f32> {
    // Convert all ASCII numbers to their numerical representations
    let mut asciinums = [0u8; 8];
    for i in 0..8 {
        asciinums[i] = hexasciitoint(ascii[i])?;
    }

    // Since incoming values are little endian, and we're on little endian
    // preserve the byte order
    let fvalbytes = [
        asciinums[1] | (asciinums[0] << 4),
        asciinums[3] | (asciinums[2] << 4),
        asciinums[5] | (asciinums[4] << 4),
        asciinums[7] | (asciinums[6] << 4),
    ];
    Some(f32::from_le_bytes(fvalbytes))
}

/// The parser's state, which expat's handlers change.
///
/// The members of `XMLParserHelper` (FileFormatIridasLook.cpp:441-451 @ v2.5.2) but the
/// parser.
#[derive(Debug, Default)]
struct State {
    /// `m_lineNumber`.
    line_number: u32,
    /// `m_fileName`.
    file_name: Vec<u8>,
    /// `m_ignoring`: the depth inside elements that are skipped.
    ignoring: i32,
    /// `m_inLook`.
    in_look: bool,
    /// `m_inLut`.
    in_lut: bool,
    /// `m_inMask`.
    in_mask: bool,
    /// `m_size`.
    size: bool,
    /// `m_data`.
    data: bool,
    /// `m_lutSize`.
    lut_size: i32,
    /// `m_lutString`.
    lut_string: Vec<u8>,
}

type StateRc = Rc<RefCell<State>>;

impl State {
    /// The error `error` with the file's name and the line being parsed.
    ///
    /// Port of `XMLParserHelper::Throw` (FileFormatIridasLook.cpp:236-244 @ v2.5.2).
    fn throw(&self, error: &[u8]) -> Exception {
        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str("Error parsing Iridas Look file (");
        os.put_c_str(&self.file_name);
        os.put_str("). ");
        os.put_str("Error is: ");
        os.put_c_str(error);
        os.put_str(". At line (");
        os.put_u32(self.line_number);
        os.put_str(")");
        Exception::new(os.into_bytes())
    }
}

/// The .look parser: expat with the Iridas handlers.
///
/// Port of `XMLParserHelper` (FileFormatIridasLook.cpp:132-452 @ v2.5.2).
pub struct XmlParserHelper {
    state: StateRc,
    /// `m_parser`.
    parser: Parser<'static, Exception>,
}

impl std::fmt::Debug for XmlParserHelper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XmlParserHelper")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl XmlParserHelper {
    /// A parser of the file `file_name`, which names it in messages.
    ///
    /// Port of `XMLParserHelper::XMLParserHelper` (FileFormatIridasLook.cpp:139-156 @
    /// v2.5.2).
    pub fn new(file_name: &[u8]) -> XmlParserHelper {
        let state = Rc::new(RefCell::new(State {
            file_name: file_name.to_vec(),
            ..State::default()
        }));
        let mut parser = Parser::new();
        let start_state = state.clone();
        let end_state = state.clone();
        parser.set_element_handler(
            Some(Box::new(move |name, _atts| {
                start_element_handler(&start_state, name)
            })),
            Some(Box::new(move |name| end_element_handler(&end_state, name))),
        );
        let data_state = state.clone();
        parser.set_character_data_handler(Some(Box::new(move |s| {
            character_data_handler(&data_state, s)
        })));
        XmlParserHelper { state, parser }
    }

    /// Parses `istream` line by line.
    ///
    /// Port of `XMLParserHelper::Parse(std::istream &)` (FileFormatIridasLook.cpp:162-174 @
    /// v2.5.2).
    pub fn parse(&mut self, istream: &mut InputStream) -> Result<()> {
        let mut line = Vec::new();
        self.state.borrow_mut().line_number = 0;
        while istream.good() {
            istream.getline(&mut line);
            line.push(b'\n');
            self.state.borrow_mut().line_number += 1;

            self.parse_buffer(&line, !istream.good())?;
        }
        Ok(())
    }

    /// Parses one buffer, the last one when `last_line`.
    ///
    /// Port of `XMLParserHelper::Parse(const std::string &, bool)`
    /// (FileFormatIridasLook.cpp:175-193 @ v2.5.2).
    fn parse_buffer(&mut self, buffer: &[u8], last_line: bool) -> Result<()> {
        let status = match self.parser.parse(buffer, last_line) {
            Ok(status) => status,
            Err(ParseError::Handler(e)) => return Err(e),
        };
        if status == XmlStatus::Error {
            let expat_error_code = self.parser.error_code();
            let state = self.state.borrow();
            if expat_error_code == XmlError::TagMismatch {
                return Err(state.throw(b"XML parsing error (unbalanced element tags)"));
            } else {
                let mut error = b"XML parsing error: ".to_vec();
                error.extend_from_slice(
                    xml_error_string(self.parser.error_code())
                        .unwrap_or("")
                        .as_bytes(),
                );
                return Err(state.throw(&error));
            }
        }
        Ok(())
    }

    /// The LUT's grid size, and its values decoded from the `data` text, in the file's order.
    ///
    /// Port of `XMLParserHelper::getLut` (FileFormatIridasLook.cpp:195-233 @ v2.5.2).
    pub fn get_lut(&self) -> Result<(i32, Vec<f32>)> {
        let state = self.state.borrow();
        if !state.lut_string.len().is_multiple_of(8) {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Error parsing Iridas Look file (");
            os.put_c_str(&state.file_name);
            os.put_str("). ");
            os.put_str("Number of characters in 'data' must be multiple of 8. ");
            os.put_u64(state.lut_string.len() as u64);
            os.put_str(" elements found.");
            return Err(Exception::new(os.into_bytes()));
        }

        let lut_size = state.lut_size;
        // `int` arithmetic: at most 3 * 129^3.
        let expacted_vector_size = 3 * (lut_size * lut_size * lut_size);
        let mut lut = Vec::with_capacity(expacted_vector_size.max(0) as usize);

        let ascii = &state.lut_string;
        for i in 0..(ascii.len() / 8) as u32 {
            let start = 8 * i as usize;
            let digits: &[u8; 8] = ascii[start..start + 8].try_into().expect("8 characters");
            let Some(fval) = hexasciitofloat(digits) else {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("Error parsing Iridas Look file (");
                os.put_c_str(&state.file_name);
                os.put_str("). ");
                os.put_str("Non-hex characters found in 'data' block ");
                os.put_str("at index '");
                os.put_u32(8 * i);
                os.put_str("'.");
                return Err(Exception::new(os.into_bytes()));
            };
            lut.push(fval);
        }

        if expacted_vector_size != lut.len() as i32 {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Error parsing Iridas Look file (");
            os.put_c_str(&state.file_name);
            os.put_str("). ");
            os.put_str("Incorrect number of lut3d entries. ");
            os.put_str("Found ");
            os.put_u64(lut.len() as u64);
            os.put_str(" values, expected ");
            os.put_i32(expacted_vector_size);
            os.put_str(".");
            return Err(Exception::new(os.into_bytes()));
        }

        Ok((lut_size, lut))
    }
}

/// Start the parsing of one element.
///
/// Port of `XMLParserHelper::StartElementHandler` (FileFormatIridasLook.cpp:246-329 @
/// v2.5.2). expat never gives an empty name, so its "Internal error" can't happen.
fn start_element_handler(state: &StateRc, name: &[u8]) -> Result<()> {
    let mut p = state.borrow_mut();
    if name.is_empty() {
        return Err(p.throw(b"Internal error"));
    }

    if p.ignoring > 0 {
        p.ignoring += 1;

        if p.in_mask {
            // Non-empty mask
            return Err(p.throw(b"Cannot load .look LUT containing mask"));
        }
    } else if name == b"look" {
        if p.in_look {
            return Err(p.throw(b"<look> node can not be inside a <look> node"));
        } else {
            p.in_look = true;
        }
    } else if !p.in_look {
        return Err(p.throw(b"Expecting root node to be a look node"));
    } else if !p.in_lut {
        if name == b"LUT" {
            p.in_lut = true;
        } else if name == b"mask" {
            p.in_mask = true;
            p.ignoring += 1;
        } else {
            p.ignoring += 1;
        }
    } else if name == b"size" {
        p.size = true;
    } else if name == b"data" {
        p.data = true;
    }
    Ok(())
}

/// End the parsing of one element.
///
/// Port of `XMLParserHelper::EndElementHandler` (FileFormatIridasLook.cpp:331-392 @ v2.5.2).
/// expat never gives an empty name, so its "XML internal parsing error." can't happen.
fn end_element_handler(state: &StateRc, name: &[u8]) -> Result<()> {
    let mut p = state.borrow_mut();
    if name.is_empty() {
        return Err(Exception::new("XML internal parsing error."));
    }

    if p.ignoring > 0 {
        p.ignoring -= 1;
    } else if p.size {
        if name != b"size" {
            return Err(p.throw(b"Expecting <size> end"));
        }
        // reading size
        p.size = false;
    } else if p.data {
        if name != b"data" {
            return Err(p.throw(b"Expecting <data> end"));
        }
        // reading data
        p.data = false;
    } else if p.in_lut {
        if name != b"LUT" {
            return Err(p.throw(b"Expecting <LUT> end"));
        }
        // reading lut finished
        p.in_lut = false;
    } else if p.in_look {
        if name != b"look" {
            return Err(p.throw(b"Expecting <look> end"));
        }
        // reading look finished
        p.in_look = false;
    } else if p.in_mask {
        if name != b"mask" {
            return Err(p.throw(b"Expecting <mask> end"));
        }
        // reading mask finished
        p.in_mask = false;
    }
    Ok(())
}

/// Handle of strings within an element: the LUT's size, and the text of its data without
/// spaces, quotes and line feeds.
///
/// Port of `XMLParserHelper::CharacterDataHandler` (FileFormatIridasLook.cpp:394-438 @
/// v2.5.2).
fn character_data_handler(state: &StateRc, s: &[u8]) -> Result<()> {
    let mut p = state.borrow_mut();

    let len = s.len();
    if len == 0 {
        return Ok(());
    }
    // `!*s`: the text starts with a NUL, which expat never gives.
    if s[0] == 0 {
        return Err(p.throw(b"XML parsing error: attribute illegal"));
    }
    // Parsing a single new line. This is valid.
    if len == 1 && s[0] == b'\n' {
        return Ok(());
    }

    if p.size {
        let size_raw = s;
        let size_clean = pystring::strip(size_raw, b"'\" "); // strip quotes and space

        let mut size_3d: i64 = 0;

        let result = from_chars_long(Flavor::NATIVE, &size_clean, size_clean.len(), &mut size_3d);

        if result.ec != Errc::Ok {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Invalid LUT size value: '");
            os.put_bytes(size_raw);
            os.put_str("'. Expected quoted integer");
            return Err(p.throw(&os.into_bytes()));
        }
        if size_3d < 2 || size_3d > i64::from(MAX_3D_LUT_LENGTH) {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Invalid LUT size value: '");
            os.put_bytes(size_raw);
            os.put_str("'. Expected value between 2 and ");
            os.put_u32(MAX_3D_LUT_LENGTH);
            return Err(p.throw(&os.into_bytes()));
        }
        p.lut_size = size_3d as i32;
    } else if p.data {
        // TODO: Possible parsing improvement: string copies
        //       could be avoided.
        // Remove spaces, quotes and newlines
        let mut what = s.to_vec();
        replace_in_place(&mut what, b" ", b"");
        replace_in_place(&mut what, b"\"", b"");
        replace_in_place(&mut what, b"'", b"");
        replace_in_place(&mut what, b"\n", b"");
        // Append to lut string (cap at max possible: Max3DLUTLength^3 entries * 3 floats * 8 hex chars)
        let max = MAX_3D_LUT_LENGTH as usize;
        if p.lut_string.len() + what.len() > max * max * max * 3 * 8 {
            return Err(p.throw(b"Too many characters in 'data' block"));
        }
        p.lut_string.extend_from_slice(&what);
    }
    Ok(())
}

/// A .look file's 3D LUT.
///
/// Port of `LocalCachedFile` (FileFormatIridasLook.cpp:483-492 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut3D`.
    pub lut3d: Lut3DOpData,
}

impl CachedFile for LocalCachedFile {}

/// The Iridas .look format.
///
/// Port of `LocalFileFormat` (FileFormatIridasLook.cpp:494-512 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

impl LocalFileFormat {
    /// Reads the file: its XML, then the LUT of its `<LUT>` section.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatIridasLook.cpp:523-551 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        let mut parser = XmlParserHelper::new(file_name);
        parser.parse(istream)?;

        // TODO: There is a LUT1D section in some .look files,
        // which we could use if available. Need to check
        // assumption that it is only written for 1D transforms,
        // and it matches the desired output

        // Validate LUT sizes, and create cached file object
        let (grid_size, raw) = parser.get_lut()?;

        // `std::make_shared<Lut3DOpData>(gridSize)`: the size is 0 (no `<size>`) or in
        // [2, 129].
        let mut lut3d = Lut3DOpData::new(grid_size as c_ulong)?;
        if Lut3DOpData::is_valid_interpolation(interp) {
            lut3d.set_interpolation(interp);
        }
        lut3d.set_file_output_bit_depth(BitDepth::F32);
        lut3d.set_array_from_red_fastest_order(&raw)?;

        Ok(LocalCachedFile { lut3d })
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatIridasLook.cpp:514-521 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "iridas_look",
            "look",
            FormatCapabilities::READ,
            FormatBakeCapabilities::NONE,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatIridasLook.cpp:523-551 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Appends the file's 3D LUT, with the file transform's interpolation where valid for it.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatIridasLook.cpp:554-585 @ v2.5.2).
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
            return Err(Exception::new(
                "Cannot build Iridas .look Op. Invalid cache type.",
            ));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_interp = file_transform.interpolation();

        let mut file_interp_used = false;
        let lut3d = handle_lut3d(Some(&cached_file.lut3d), file_interp, &mut file_interp_used)
            .expect("the file's LUT");

        if !file_interp_used {
            log_warning_interpolation_not_used(file_interp, file_transform);
        }

        create_lut3d_op(ops, lut3d, new_dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_iridas_look_tests.rs"]
mod tests;
