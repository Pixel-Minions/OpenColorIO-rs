// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsio0.c` @ lcms2.17: the file and null stream handlers, the profile structure,
//! its header, and reading and writing tags.
//!
//! `cmsReadTag` hands out a pointer the profile keeps, and here an `Rc`. The handlers for memory
//! streams, raw tags and tag deletion aren't reached.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::rc::Rc;

use super::cmserr::{cms_signal_error, cms_tag_signature_2_string};
use super::cmsnamed::Mlu;
use super::cmsplugin::*;
use super::cmstypes::*;
use super::cmswtpnt::cms_d50_xyz;
use super::types::*;
use crate::stdio::{create_file, path_from_bytes};

/// `sizeof(cmsICCHeader)`.
const ICC_HEADER_SIZE: usize = 128;

/// What the stream of a `cmsIOHANDLER` is.
#[derive(Debug)]
enum Stream {
    /// `FILENULL`.
    Null { pointer: u32 },
    /// A `FILE*` open with "rb". The file is read when it opens: the C runtime's buffering
    /// doesn't show, and a profile's values are read one at a time.
    Reader { data: Vec<u8>, pos: usize },
    /// A `FILE*` open with "wb".
    Writer { file: BufWriter<File>, pos: u32 },
}

/// `cmsIOHANDLER`.
#[derive(Debug)]
pub struct IoHandler {
    stream: Stream,
    pub used_space: u32,
    pub reported_size: u32,
}

impl IoHandler {
    /// Port of `cmsOpenIOhandlerFromNULL` (src/cmsio0.c:100-131 @ lcms2.17).
    pub fn null() -> IoHandler {
        IoHandler {
            stream: Stream::Null { pointer: 0 },
            used_space: 0,
            reported_size: 0,
        }
    }

    /// Port of the "r" case of `cmsOpenIOhandlerFromFile` (src/cmsio0.c:378-474 @ lcms2.17).
    pub fn open_read(file_name: &[u8]) -> Option<IoHandler> {
        let quoted = |format: &[&[u8]]| -> Vec<u8> { format.concat() };

        let Ok(mut file) = File::open(path_from_bytes(file_name)) else {
            cms_signal_error(
                CMS_ERROR_FILE,
                &quoted(&[b"File '", file_name, b"' not found"]),
            );
            return None;
        };

        // cmsfilelength (src/cmserr.c:52-68 @ lcms2.17)
        let length = file
            .seek(SeekFrom::End(0))
            .map(|end| end as i64)
            .unwrap_or(-1);
        // `long` has 32 bits on Windows, where ftell fails past it; (cmsInt32Number) cuts it
        // elsewhere.
        let file_len = if cfg!(windows) && length > i32::MAX as i64 {
            -1
        } else {
            length as i32
        };
        if file_len < 0 {
            cms_signal_error(
                CMS_ERROR_FILE,
                &quoted(&[b"Cannot get size of file '", file_name, b"'"]),
            );
            return None;
        }

        let mut data = Vec::new();
        // A directory opens on Linux, and then reading it fails: nothing to read.
        if file.seek(SeekFrom::Start(0)).is_err() || file.read_to_end(&mut data).is_err() {
            data.clear();
        }

        Some(IoHandler {
            stream: Stream::Reader { data, pos: 0 },
            used_space: 0,
            reported_size: file_len as u32,
        })
    }

    /// Port of the "w" case of `cmsOpenIOhandlerFromFile` (src/cmsio0.c:378-474 @ lcms2.17).
    pub fn open_write(file_name: &[u8]) -> Option<IoHandler> {
        let Ok(file) = create_file(file_name) else {
            cms_signal_error(
                CMS_ERROR_FILE,
                &[b"Couldn't create '", file_name, b"'"].concat(),
            );
            return None;
        };
        Some(IoHandler {
            stream: Stream::Writer {
                file: BufWriter::with_capacity(1 << 20, file),
                pos: 0,
            },
            used_space: 0,
            reported_size: 0,
        })
    }

    /// `io->Read(io, Buffer, size, count)`: `FileRead` (src/cmsio0.c:319-330 @ lcms2.17). The elements
    /// read, or 0 and an error if that's not all of them. `buffer` holds `size * count` bytes.
    pub fn read(&mut self, buffer: &mut [u8], size: u32, count: u32) -> u32 {
        let Stream::Reader { data, pos } = &mut self.stream else {
            return 0;
        };
        let wanted = size as usize * count as usize;
        let got = wanted.min(data.len().saturating_sub(*pos));
        if got > 0 {
            buffer[..got].copy_from_slice(&data[*pos..*pos + got]);
            *pos += got;
        }
        let n_readed = if size == 0 {
            0
        } else {
            (got / size as usize) as u32
        };

        if n_readed != count {
            let text = format!(
                "Read error. Got {} bytes, block should be of {} bytes",
                n_readed.wrapping_mul(size) as i32,
                count.wrapping_mul(size) as i32,
            );
            cms_signal_error(CMS_ERROR_FILE, text.as_bytes());
            return 0;
        }

        n_readed
    }

    /// `io->Seek(io, offset)`: `NULLSeek` (src/cmsio0.c:59-66 @ lcms2.17) and `FileSeek` (src/cmsio0.c:333-343 @ lcms2.17). `(long) offset` is negative from 2^31 up on Windows, where
    /// `long` has 32 bits, and `fseek` refuses it.
    pub fn seek(&mut self, offset: u32) -> bool {
        let refused = cfg!(windows) && offset > i32::MAX as u32;
        let done = match &mut self.stream {
            Stream::Null { pointer } => {
                *pointer = offset;
                return true;
            }
            Stream::Reader { pos, .. } => {
                if !refused {
                    *pos = offset as usize;
                }
                !refused
            }
            Stream::Writer { file, pos } => {
                let moved = !refused && file.seek(SeekFrom::Start(offset as u64)).is_ok();
                if moved {
                    *pos = offset;
                }
                moved
            }
        };
        if !done {
            cms_signal_error(CMS_ERROR_FILE, b"Seek error; probably corrupted file");
        }
        done
    }

    /// `io->Tell(io)`: `NULLTell` (src/cmsio0.c:68-73 @ lcms2.17) and `FileTell` (src/cmsio0.c:346-356 @ lcms2.17).
    pub fn tell(&self) -> u32 {
        match &self.stream {
            Stream::Null { pointer } => *pointer,
            Stream::Reader { pos, .. } => *pos as u32,
            Stream::Writer { pos, .. } => *pos,
        }
    }

    /// `io->Write(io, size, Buffer)`: `NULLWrite` (src/cmsio0.c:75-87 @ lcms2.17) and `FileWrite` (src/cmsio0.c:359-366 @ lcms2.17). Only the first keeps a true count: `FileWrite` also counts
    /// what is written twice, the directories that are filled in later.
    pub fn write(&mut self, buffer: &[u8]) -> bool {
        let size = buffer.len() as u32;
        match &mut self.stream {
            Stream::Null { pointer } => {
                *pointer = pointer.wrapping_add(size);
                if *pointer > self.used_space {
                    self.used_space = *pointer;
                }
                true
            }
            Stream::Reader { .. } => false,
            Stream::Writer { file, pos } => {
                if size == 0 {
                    return true; // We allow to write 0 bytes, but nothing is written
                }

                self.used_space = self.used_space.wrapping_add(size);
                *pos = pos.wrapping_add(size);
                file.write_all(buffer).is_ok()
            }
        }
    }

    /// `io->Close(io)`: `FileClose` (src/cmsio0.c:369-375 @ lcms2.17).
    fn close(self) -> bool {
        match self.stream {
            Stream::Writer { mut file, .. } => file.flush().is_ok(),
            _ => true,
        }
    }
}

/// What a tag holds. `TagPtrs` points to one of these.
#[derive(Debug)]
pub enum TagData {
    /// `cmsCIEXYZ`.
    Xyz(CieXyz),
    /// `cmsCIExyYTRIPLE`.
    Chromaticity(CieXyYTriple),
    /// `cmsToneCurve`.
    Curve(super::cmsgamma::ToneCurve),
    /// `cmsPipeline`.
    Pipeline(super::cmslut::Pipeline),
    /// `cmsMLU`.
    Mlu(Mlu),
    /// An array of doubles.
    S15Fixed16(Vec<f64>),
    /// `cmsNAMEDCOLORLIST`, which nobody looks at.
    ColorantTable,
}

/// A line of the tag arrays of `_cmsICCPROFILE`.
#[derive(Debug)]
pub struct Tag {
    pub name: u32,
    /// The tag to which is linked (0=none).
    pub linked: u32,
    /// Size on disk.
    pub size: u32,
    pub offset: u32,
    pub ptr: Option<Rc<TagData>>,
    /// The signature of the handler, or 0 for `NULL`.
    pub type_handler: u32,
}

/// `_cmsICCPROFILE`.
#[derive(Debug)]
pub struct Profile {
    pub io: Option<IoHandler>,
    pub created: DateTime,
    pub cmm: u32,
    pub version: u32,
    pub device_class: u32,
    pub color_space: u32,
    pub pcs: u32,
    pub rendering_intent: u32,
    pub platform: u32,
    pub flags: u32,
    pub manufacturer: u32,
    pub model: u32,
    pub attributes: u64,
    pub creator: u32,
    pub profile_id: [u8; 16],
    pub tags: Vec<Tag>,
}

/// Port of `cmsCreateProfilePlaceholder` (src/cmsio0.c:526-569 @ lcms2.17).
pub fn cms_create_profile_placeholder() -> Profile {
    Profile {
        io: None,
        created: cms_get_time(),
        cmm: LCMS_SIGNATURE,
        // Set default version
        version: 0x02100000,
        device_class: SIG_DISPLAY_CLASS,
        color_space: 0,
        pcs: 0,
        rendering_intent: 0,
        // Set default platform
        platform: if cfg!(windows) {
            SIG_MICROSOFT
        } else {
            SIG_MACINTOSH
        },
        flags: 0,
        manufacturer: 0,
        model: 0,
        attributes: 0,
        creator: LCMS_SIGNATURE,
        profile_id: [0; 16],
        tags: Vec::new(),
    }
}

/// Port of `SearchOneTag` (src/cmsio0.c:601-613 @ lcms2.17).
fn search_one_tag(profile: &Profile, sig: u32) -> i32 {
    profile
        .tags
        .iter()
        .position(|tag| tag.name == sig)
        .map_or(-1, |i| i as i32)
}

/// Port of `_cmsSearchTag` (src/cmsio0.c:617-643 @ lcms2.17).
pub fn cms_search_tag(icc: &Profile, mut sig: u32, follow_links: bool) -> i32 {
    loop {
        // Search for given tag in ICC profile directory
        let n = search_one_tag(icc, sig);
        if n < 0 {
            return -1; // Not found
        }

        if !follow_links {
            return n; // Found, don't follow links
        }

        // Is this a linked tag?
        let linked_sig = icc.tags[n as usize].linked;

        // Yes, follow link
        if linked_sig == 0 {
            return n;
        }
        sig = linked_sig;
    }
}

/// Port of `_cmsNewTag` (src/cmsio0.c:678-704 @ lcms2.17). A built-in profile has a few tags, so
/// `MAX_TABLE_TAG` isn't reached.
fn cms_new_tag(icc: &mut Profile, sig: u32) -> usize {
    // Search for the tag
    let i = cms_search_tag(icc, sig, false);
    if i >= 0 {
        // Already exists? delete it
        icc.tags[i as usize].ptr = None;
        return i as usize;
    }

    // No, make a new one
    icc.tags.push(Tag {
        name: 0,
        linked: 0,
        size: 0,
        offset: 0,
        ptr: None,
        type_handler: 0,
    });
    icc.tags.len() - 1
}

/// Port of `cmsIsTag` (src/cmsio0.c:708-712 @ lcms2.17).
pub fn cms_is_tag(profile: &Profile, sig: u32) -> bool {
    cms_search_tag(profile, sig, false) >= 0
}

/// Port of `CompatibleTypes` (src/cmsio0.c:717-733 @ lcms2.17).
fn compatible_types(desc1: Option<TagDescriptor>, desc2: Option<TagDescriptor>) -> bool {
    let (Some(desc1), Some(desc2)) = (desc1, desc2) else {
        return false;
    };

    desc1.supported_types == desc2.supported_types && desc1.elem_count == desc2.elem_count
}

/// Port of `_validatedVersion` (src/cmsio0.c:741-758 @ lcms2.17), on the big-endian bytes.
fn validated_version(mut bytes: [u8; 4]) -> [u8; 4] {
    if bytes[0] > 0x09 {
        bytes[0] = 0x09;
    }
    let mut temp1 = bytes[1] & 0xf0;
    let mut temp2 = bytes[1] & 0x0f;
    if temp1 > 0x90 {
        temp1 = 0x90;
    }
    if temp2 > 0x09 {
        temp2 = 0x09;
    }
    bytes[1] = temp1 | temp2;
    bytes[2] = 0;
    bytes[3] = 0;
    bytes
}

/// Port of `validDeviceClass` (src/cmsio0.c:761-781 @ lcms2.17).
fn valid_device_class(cl: u32) -> bool {
    if cl == 0 {
        return true; // We allow zero because older lcms versions defaulted to that.
    }

    matches!(
        cl,
        SIG_INPUT_CLASS
            | SIG_DISPLAY_CLASS
            | SIG_OUTPUT_CLASS
            | SIG_LINK_CLASS
            | SIG_ABSTRACT_CLASS
            | SIG_COLOR_SPACE_CLASS
            | SIG_NAMED_COLOR_CLASS
    )
}

/// Port of `_cmsReadHeader` (src/cmsio0.c:784-906 @ lcms2.17).
fn cms_read_header(icc: &mut Profile, io: &mut IoHandler) -> bool {
    // Read the header
    let mut header = [0u8; ICC_HEADER_SIZE];
    if io.read(&mut header, ICC_HEADER_SIZE as u32, 1) != 1 {
        return false;
    }
    let field = |at: usize| [header[at], header[at + 1], header[at + 2], header[at + 3]];
    let number = |at: usize| u32::from_be_bytes(field(at));

    // Validate file as an ICC profile
    if number(36) != CMS_MAGIC_NUMBER {
        cms_signal_error(
            CMS_ERROR_BAD_SIGNATURE,
            b"not an ICC profile, invalid signature",
        );
        return false;
    }

    // Adjust endianness of the used parameters
    icc.cmm = number(4);
    icc.device_class = number(12);
    icc.color_space = number(16);
    icc.pcs = number(20);

    icc.rendering_intent = number(64);
    icc.platform = number(40);
    icc.flags = number(44);
    icc.manufacturer = number(48);
    icc.model = number(52);
    icc.creator = number(80);

    icc.attributes = ((number(56) as u64) << 32) | number(60) as u64;
    icc.version = u32::from_be_bytes(validated_version(field(8)));

    if icc.version > 0x5000000 {
        let text = format!("Unsupported profile version '0x{:x}'", icc.version);
        cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, text.as_bytes());
        return false;
    }

    if !valid_device_class(icc.device_class) {
        let text = format!("Unsupported device class '0x{:x}'", icc.device_class);
        cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, text.as_bytes());
        return false;
    }

    // Get size as reported in header
    let mut header_size = number(0);

    // Make sure HeaderSize is lower than profile size
    if header_size >= io.reported_size {
        header_size = io.reported_size;
    }

    // Read tag directory
    let Some(tag_count) = cms_read_u32(io) else {
        return false;
    };
    if tag_count > MAX_TABLE_TAG as u32 {
        let text = format!("Too many tags ({})", tag_count as i32);
        cms_signal_error(CMS_ERROR_RANGE, text.as_bytes());
        return false;
    }

    // Read tag directory
    icc.tags.clear();
    for _ in 0..tag_count {
        let Some(sig) = cms_read_u32(io) else {
            return false;
        };
        let Some(offset) = cms_read_u32(io) else {
            return false;
        };
        let Some(size) = cms_read_u32(io) else {
            return false;
        };

        // Perform some sanity check. Offset + size should fall inside file.
        if size == 0 || offset == 0 {
            continue;
        }
        if offset.wrapping_add(size) > header_size || offset.wrapping_add(size) < offset {
            continue;
        }

        let mut tag = Tag {
            name: sig,
            linked: 0,
            size,
            offset,
            ptr: None,
            type_handler: 0,
        };

        // Search for links
        for other in &icc.tags {
            if other.offset == offset && other.size == size {
                // Check types.
                if compatible_types(
                    cms_get_tag_descriptor(other.name),
                    cms_get_tag_descriptor(sig),
                ) {
                    tag.linked = other.name;
                }
            }
        }

        icc.tags.push(tag);
    }

    for i in 0..icc.tags.len() {
        for j in 0..icc.tags.len() {
            // Tags cannot be duplicate
            if i != j && icc.tags[i].name == icc.tags[j].name {
                cms_signal_error(CMS_ERROR_RANGE, b"Duplicate tag found");
                return false;
            }
        }
    }

    true
}

/// Port of `_cmsWriteHeader` (src/cmsio0.c:909-979 @ lcms2.17).
fn cms_write_header(icc: &Profile, io: &mut IoHandler, used_space: u32) -> bool {
    let mut header = [0u8; ICC_HEADER_SIZE];
    let mut put = |at: usize, value: u32| header[at..at + 4].copy_from_slice(&value.to_be_bytes());

    put(0, used_space);
    put(4, icc.cmm);
    put(8, icc.version);

    put(12, icc.device_class);
    put(16, icc.color_space);
    put(20, icc.pcs);

    put(36, CMS_MAGIC_NUMBER);

    put(40, icc.platform);

    put(44, icc.flags);
    put(48, icc.manufacturer);
    put(52, icc.model);

    put(56, (icc.attributes >> 32) as u32);
    put(60, icc.attributes as u32);

    // Rendering intent in the header (for embedded profiles)
    put(64, icc.rendering_intent);

    // Illuminant is always D50
    put(68, cms_double_to_15_fixed16(cms_d50_xyz().x) as u32);
    put(72, cms_double_to_15_fixed16(cms_d50_xyz().y) as u32);
    put(76, cms_double_to_15_fixed16(cms_d50_xyz().z) as u32);

    put(80, icc.creator);

    //   NOTE: in v4 Timestamp must be in UTC rather than in local time
    header[24..36].copy_from_slice(&cms_encode_date_time_number(&icc.created));

    // Set profile ID. Endianness is always big endian
    header[84..100].copy_from_slice(&icc.profile_id);

    // Dump the header
    if !io.write(&header) {
        return false;
    }

    // Saves Tag directory

    // Store number of tags
    if !cms_write_u32(io, icc.tags.len() as u32) {
        return false;
    }

    for tag in &icc.tags {
        let mut entry = [0u8; 12];
        entry[..4].copy_from_slice(&tag.name.to_be_bytes());
        entry[4..8].copy_from_slice(&tag.offset.to_be_bytes());
        entry[8..].copy_from_slice(&tag.size.to_be_bytes());

        if !io.write(&entry) {
            return false;
        }
    }

    true
}

/// Port of `BaseToBase` (src/cmsio0.c:1118-1136 @ lcms2.17).
fn base_to_base(mut in_: u32, base_in: u32, base_out: u32) -> u32 {
    let mut buff = [0u32; 100];
    let mut len = 0;
    while in_ > 0 && len < 100 {
        buff[len] = in_ % base_in;
        in_ /= base_in;
        len += 1;
    }

    let mut out = 0u32;
    for digit in buff[..len].iter().rev() {
        out = out.wrapping_mul(base_out).wrapping_add(*digit);
    }

    out
}

/// Port of `cmsSetProfileVersion` (src/cmsio0.c:1138-1145 @ lcms2.17).
pub fn cms_set_profile_version(icc: &mut Profile, version: f64) {
    // 4.2 -> 0x4200000

    icc.version = base_to_base((version * 100.0 + 0.5).floor() as u32, 10, 16) << 16;
}

/// Port of `cmsGetProfileVersion` (src/cmsio0.c:1147-1153 @ lcms2.17).
pub fn cms_get_profile_version(icc: &Profile) -> f64 {
    let n = icc.version >> 16;

    base_to_base(n, 16, 10) as f64 / 100.0
}

/// Port of `cmsOpenProfileFromFile` (src/cmsio0.c:1231-1234 @ lcms2.17), with "r".
pub fn cms_open_profile_from_file(icc_profile: &[u8]) -> Option<Profile> {
    let mut new_icc = cms_create_profile_placeholder();

    let mut io = IoHandler::open_read(icc_profile)?;

    if !cms_read_header(&mut new_icc, &mut io) {
        return None;
    }
    new_icc.io = Some(io);
    Some(new_icc)
}

/// Port of `SaveTags` (src/cmsio0.c:1303-1416 @ lcms2.17). Every tag of the one profile the tool saves
/// holds data, and `Icc` has no original file: the blind copy of a tag isn't reached.
fn save_tags(icc: &mut Profile, io: &mut IoHandler) -> bool {
    let version = cms_get_profile_version(icc);

    for i in 0..icc.tags.len() {
        let begin = io.used_space;
        icc.tags[i].offset = begin;

        let Some(data) = icc.tags[i].ptr.clone() else {
            continue;
        };

        let name = icc.tags[i].name;
        let Some(descriptor) = cms_get_tag_descriptor(name) else {
            continue; // Unsupported, ignore it
        };

        let type_ = decide_type(name, version, &data, &descriptor);

        if !cms_write_type_base(io, type_) {
            return false;
        }

        if !type_write(type_, io, &data, descriptor.elem_count) {
            let mut text = b"Couldn't write type '".to_vec();
            text.extend_from_slice(&cms_tag_signature_2_string(type_));
            text.push(b'\'');
            cms_signal_error(CMS_ERROR_WRITE, &text);
            return false;
        }

        icc.tags[i].size = io.used_space.wrapping_sub(begin);

        // Align to 32 bit boundary.
        if !cms_write_alignment(io) {
            return false;
        }
    }

    true
}

/// Port of `cmsSaveProfileToIOhandler` (src/cmsio0.c:1446-1498 @ lcms2.17).
pub fn cms_save_profile_to_io_handler(icc: &mut Profile, io: &mut IoHandler) -> u32 {
    let mut null = IoHandler::null();

    // Pass #1 does compute offsets

    if !cms_write_header(icc, &mut null, 0) {
        return 0;
    }
    if !save_tags(icc, &mut null) {
        return 0;
    }

    let used_space = null.used_space;

    // Pass #2 does save to iohandler

    if !cms_write_header(icc, io, used_space) {
        return 0;
    }
    if !save_tags(icc, io) {
        return 0;
    }

    used_space
}

/// Port of `cmsSaveProfileToFile` (src/cmsio0.c:1502-1517 @ lcms2.17).
pub fn cms_save_profile_to_file(icc: &mut Profile, file_name: &[u8]) -> bool {
    let Some(mut io) = IoHandler::open_write(file_name) else {
        return false;
    };

    let mut rc = cms_save_profile_to_io_handler(icc, &mut io) != 0;
    rc &= io.close();

    if !rc {
        // remove() is C99 per 7.19.4.1
        let _ = std::fs::remove_file(path_from_bytes(file_name)); // We have to IGNORE return value in this case
    }
    rc
}

/// Port of `IsTypeSupported` (src/cmsio0.c:1618-1632 @ lcms2.17).
fn is_type_supported(tag_descriptor: &TagDescriptor, type_: u32) -> bool {
    tag_descriptor.supported_types.contains(&type_)
}

/// Port of `cmsReadTag` (src/cmsio0.c:1636-1767 @ lcms2.17).
pub fn cms_read_tag(icc: &mut Profile, sig: u32) -> Option<Rc<TagData>> {
    let n = cms_search_tag(icc, sig, true);
    if n < 0 {
        // Not found, return NULL
        return None;
    }
    let n = n as usize;

    let result = read_tag_at(icc, sig, n);
    if result.is_none() {
        icc.tags[n].ptr = None;
    }
    result
}

/// `cmsReadTag` once the tag is found: `None` is its `goto Error`.
fn read_tag_at(icc: &mut Profile, sig: u32, n: usize) -> Option<Rc<TagData>> {
    // If the element is already in memory, return the pointer
    if let Some(ptr) = &icc.tags[n].ptr {
        // Sanity check
        let base_type = icc.tags[n].type_handler;
        if base_type == 0 {
            return None;
        }

        let tag_descriptor = cms_get_tag_descriptor(sig)?;

        if !is_type_supported(&tag_descriptor, base_type) {
            return None;
        }

        return Some(ptr.clone());
    }

    // We need to read it. Get the offset and size to the file
    let offset = icc.tags[n].offset;
    let tag_size = icc.tags[n].size;

    if tag_size < 8 {
        return None;
    }

    let Some(io) = icc.io.as_mut() else {
        // This is a built-in profile that has been manipulated, abort early
        cms_signal_error(
            CMS_ERROR_CORRUPTION_DETECTED,
            b"Corrupted built-in profile.",
        );
        return None;
    };

    // Seek to its location
    if !io.seek(offset) {
        return None;
    }

    // Search for support on this tag
    let Some(tag_descriptor) = cms_get_tag_descriptor(sig) else {
        let mut text = b"Unknown tag type '".to_vec();
        text.extend_from_slice(&cms_tag_signature_2_string(sig));
        text.extend_from_slice(b"' found.");

        // An unknown element was found.
        cms_signal_error(CMS_ERROR_UNKNOWN_EXTENSION, &text);
        return None; // Unsupported.
    };

    // if supported, get type and check if in list
    let base_type = cms_read_type_base(io);
    if base_type == 0 {
        return None;
    }

    if !is_type_supported(&tag_descriptor, base_type) {
        return None;
    }

    // Read the tag
    icc.tags[n].type_handler = base_type;

    let (data, elem_count) = match type_read(base_type, io) {
        Some(read) => read,
        None => {
            // The tag type is supported, but something wrong happened and we cannot read the
            // tag. let know the user about this (although it is just a warning)
            let mut text = b"Corrupted tag '".to_vec();
            text.extend_from_slice(&cms_tag_signature_2_string(sig));
            text.push(b'\'');
            cms_signal_error(CMS_ERROR_CORRUPTION_DETECTED, &text);
            return None;
        }
    };

    // This is a weird error that may be a symptom of something more serious, the number of
    // stored item is actually less than the number of required elements.
    if elem_count < tag_descriptor.elem_count {
        let mut text = b"'".to_vec();
        text.extend_from_slice(&cms_tag_signature_2_string(sig));
        text.extend_from_slice(
            format!(
                "' Inconsistent number of items: expected {}, got {}",
                tag_descriptor.elem_count, elem_count
            )
            .as_bytes(),
        );
        cms_signal_error(CMS_ERROR_CORRUPTION_DETECTED, &text);
        return None;
    }

    // Return the data
    let data = Rc::new(data);
    icc.tags[n].ptr = Some(data.clone());
    Some(data)
}

/// Port of `_cmsGetTagTrueType` (src/cmsio0.c:1771-1784 @ lcms2.17).
pub fn cms_get_tag_true_type(icc: &Profile, sig: u32) -> u32 {
    // Search for given tag in ICC profile directory
    let n = cms_search_tag(icc, sig, true);
    if n < 0 {
        return 0; // Not found, return NULL
    }

    // Get the handler. The true type is there
    icc.tags[n as usize].type_handler
}

/// Port of `cmsWriteTag` (src/cmsio0.c:1789-1901 @ lcms2.17). `data` is the duplicate `DupPtr` makes. The
/// tags and types the tool writes are all supported ones, and none of them fails to duplicate:
/// the function's error paths aren't reached.
pub fn cms_write_tag(icc: &mut Profile, sig: u32, data: TagData) -> bool {
    let i = cms_new_tag(icc, sig);

    // This is not a link
    icc.tags[i].linked = 0;

    // Get information about the TAG.
    let Some(tag_descriptor) = cms_get_tag_descriptor(sig) else {
        return false;
    };

    // Now we need to know which type to use. It depends on the version.
    let version = cms_get_profile_version(icc);

    let type_ = decide_type(sig, version, &data, &tag_descriptor);

    // Fill fields on icc structure
    let tag = &mut icc.tags[i];
    tag.type_handler = type_;
    tag.name = sig;
    tag.size = 0;
    tag.offset = 0;
    tag.ptr = Some(Rc::new(data));

    true
}

/// Port of `cmsLinkTag` (src/cmsio0.c:2065-2088 @ lcms2.17).
pub fn cms_link_tag(icc: &mut Profile, sig: u32, dest: u32) -> bool {
    let i = cms_new_tag(icc, sig);

    // Keep necessary information
    let tag = &mut icc.tags[i];
    tag.name = sig;
    tag.linked = dest;

    tag.ptr = None;
    tag.size = 0;
    tag.offset = 0;

    true
}
