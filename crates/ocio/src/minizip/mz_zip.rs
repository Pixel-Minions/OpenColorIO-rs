// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's zip archive, opened for reading (`mz_zip.c`): the central directory, the
//! headers of its entries, and an entry's data.

use super::inflate::{Stop, inflate};
use super::mz_strm::{
    MZ_BUF_ERROR, MZ_END_OF_LIST, MZ_END_OF_STREAM, MZ_EXIST_ERROR, MZ_FORMAT_ERROR, MZ_OK,
    MZ_PARAM_ERROR, MZ_READ_ERROR, MZ_STREAM_ERROR, MZ_SUPPORT_ERROR, MemStream, Origin,
    SplitStream, Stream, mz_stream_find, mz_stream_find_reverse, mz_stream_read_int64,
    mz_stream_read_uint16, mz_stream_read_uint32, mz_stream_read_uint64, mz_stream_seek,
};

/// The signatures (mz_zip.c:59-66 @ 4.0.10).
const MZ_ZIP_MAGIC_LOCALHEADER: u32 = 0x0403_4b50;
const MZ_ZIP_MAGIC_CENTRALHEADER: u32 = 0x0201_4b50;
const MZ_ZIP_MAGIC_ENDHEADER: u32 = 0x0605_4b50;
const MZ_ZIP_MAGIC_ENDHEADER64: u32 = 0x0606_4b50;
const MZ_ZIP_MAGIC_ENDLOCHEADER64: u32 = 0x0706_4b50;
const MZ_ZIP_MAGIC_DATADESCRIPTOR: u32 = 0x0807_4b50;

/// The sizes of a central header and of the zip64 end locator (mz_zip.c:71-72 @ 4.0.10).
const MZ_ZIP_SIZE_CD_ITEM: i64 = 46;
const MZ_ZIP_SIZE_CD_LOCATOR64: i64 = 20;

/// The largest data descriptor (`MZ_ZIP_SIZE_MAX_DATA_DESCRIPTOR`, mz_zip.c:73 @ 4.0.10).
const MZ_ZIP_SIZE_MAX_DATA_DESCRIPTOR: i64 = 24;

/// How far back from the end of the file the end of the central directory is looked for
/// (`MZ_ZIP_EOCD_MAX_BACK`, mz_zip.c:78-80 @ 4.0.10).
const MZ_ZIP_EOCD_MAX_BACK: i64 = 1 << 20;

/// The extra fields minizip-ng reads (mz.h:88-94 @ 4.0.10). OpenColorIO's minizip-ng is built
/// without WinZip AES (`MZ_WZAES=OFF`, share/cmake/modules/install/Installminizip-ng.cmake:82
/// @ v2.5.2), so its AES field is skipped as unknown.
pub(crate) const MZ_ZIP_EXTENSION_ZIP64: u16 = 0x0001;
pub(crate) const MZ_ZIP_EXTENSION_NTFS: u16 = 0x000a;
pub(crate) const MZ_ZIP_EXTENSION_UNIX1: u16 = 0x000d;

/// The compression methods (mz.h:63-64 @ 4.0.10): OpenColorIO's minizip-ng is built with
/// deflate (zlib) only (Installminizip-ng.cmake:73-87 @ v2.5.2).
const MZ_COMPRESS_METHOD_STORE: u16 = 0;
const MZ_COMPRESS_METHOD_DEFLATE: u16 = 8;

/// `MZ_ZIP_FLAG_ENCRYPTED` (mz.h:77 @ 4.0.10).
const MZ_ZIP_FLAG_ENCRYPTED: u16 = 1;

/// `MZ_ZIP_FLAG_DATA_DESCRIPTOR` (mz.h:83 @ 4.0.10).
const MZ_ZIP_FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;

/// An entry's header, as minizip-ng reads it.
///
/// Port of `mz_zip_file` (mz_zip.h @ 4.0.10), with the fields reading reaches; `filename` and
/// `extrafield` are its pointers into the header's memory stream.
#[derive(Clone, Debug, Default)]
pub(crate) struct FileInfo {
    pub(crate) version_madeby: u16,
    pub(crate) version_needed: u16,
    pub(crate) flag: u16,
    pub(crate) compression_method: u16,
    pub(crate) crc: u32,
    pub(crate) compressed_size: i64,
    pub(crate) uncompressed_size: i64,
    pub(crate) filename_size: u16,
    pub(crate) extrafield_size: u16,
    pub(crate) comment_size: u16,
    pub(crate) disk_number: u32,
    pub(crate) internal_fa: u16,
    pub(crate) external_fa: u32,
    pub(crate) disk_offset: i64,
    /// The name: the C string the header's memory stream starts with.
    pub(crate) filename: Vec<u8>,
    /// The extra field's `extrafield_size` bytes, as far as the memory stream holds them.
    pub(crate) extrafield: Vec<u8>,
    /// The link name of a UNIX1 field: the C string at its place in the memory stream.
    pub(crate) linkname: Vec<u8>,
    /// The dates the NTFS and UNIX1 fields give (`time_t`). The modification date starts as
    /// the header's DOS date, which the port doesn't convert (`mktime`, of the local time
    /// zone): what reads these dates only asks whether all three are nonzero, and the creation
    /// date is nonzero only where an NTFS field set all three.
    pub(crate) modified_date: i64,
    pub(crate) accessed_date: i64,
    pub(crate) creation_date: i64,
}

/// Port of `mz_zip_ntfs_to_unix_time` (mz_zip.c:2742-2745 @ 4.0.10): the `uint64_t`
/// arithmetic, cast to `time_t`.
fn mz_zip_ntfs_to_unix_time(ntfs_time: u64) -> i64 {
    (ntfs_time.wrapping_sub(116_444_736_000_000_000) / 10_000_000) as i64
}

/// Port of `mz_stream_copy` (mz_strm.c:153-155, 161-185 @ 4.0.10): `len` bytes of `source`
/// into `target`, 16 KiB at a time; `MZ_STREAM_ERROR` when `source` ends or fails first.
fn mz_stream_copy(target: &mut MemStream, source: &mut dyn Stream, len: i32) -> i32 {
    let mut buf = [0u8; 16384];
    let mut len = len;
    while len > 0 {
        let bytes_to_copy = len.min(buf.len() as i32);
        let read = source.read(&mut buf[..bytes_to_copy as usize]);
        if read <= 0 {
            return MZ_STREAM_ERROR;
        }
        let written = target.write(&buf[..read as usize]);
        if written != read {
            return MZ_STREAM_ERROR;
        }
        len -= read;
    }
    MZ_OK
}

/// `err` of a read: its value, or the error it leaves in `err`.
macro_rules! read_into {
    ($err:ident, $read:expr) => {
        match $read {
            Ok(v) => v,
            Err(e) => {
                $err = e;
                Default::default()
            }
        }
    };
}

/// Reads an entry's header (a central one, or a local one with `local`) from `stream` into
/// `file_info`, keeping its name, extra field and comment in `file_extra_stream`, which the
/// caller keeps from header to header: what a header leaves past its own data stays, and a
/// misaligned extra field can read it.
///
/// Port of `mz_zip_entry_read_header` (mz_zip.c:202-479 @ 4.0.10), without WinZip AES and
/// PKWARE encryption (not built in OpenColorIO's minizip-ng); the dates it reads aren't kept.
pub(crate) fn mz_zip_entry_read_header(
    stream: &mut dyn Stream,
    local: bool,
    file_info: &mut FileInfo,
    file_extra_stream: &mut MemStream,
) -> i32 {
    *file_info = FileInfo::default();
    let mut err;

    // Check the magic
    let magic = match mz_stream_read_uint32(stream) {
        Ok(magic) => {
            err = MZ_OK;
            magic
        }
        Err(e) => {
            err = e;
            0
        }
    };
    if err == MZ_END_OF_STREAM
        || magic == MZ_ZIP_MAGIC_ENDHEADER
        || magic == MZ_ZIP_MAGIC_ENDHEADER64
    {
        err = MZ_END_OF_LIST;
    } else if (local && magic != MZ_ZIP_MAGIC_LOCALHEADER)
        || (!local && magic != MZ_ZIP_MAGIC_CENTRALHEADER)
    {
        err = MZ_FORMAT_ERROR;
    }

    // Read header fields
    if err == MZ_OK {
        if !local {
            file_info.version_madeby = read_into!(err, mz_stream_read_uint16(stream));
        }
        if err == MZ_OK {
            file_info.version_needed = read_into!(err, mz_stream_read_uint16(stream));
        }
        if err == MZ_OK {
            file_info.flag = read_into!(err, mz_stream_read_uint16(stream));
        }
        if err == MZ_OK {
            file_info.compression_method = read_into!(err, mz_stream_read_uint16(stream));
        }
        if err == MZ_OK {
            // dos_date
            let _: u32 = read_into!(err, mz_stream_read_uint32(stream));
        }
        if err == MZ_OK {
            file_info.crc = read_into!(err, mz_stream_read_uint32(stream));
        }
        if err == MZ_OK {
            let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
            file_info.compressed_size = i64::from(value32);
        }
        if err == MZ_OK {
            let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
            file_info.uncompressed_size = i64::from(value32);
        }
        if err == MZ_OK {
            file_info.filename_size = read_into!(err, mz_stream_read_uint16(stream));
        }
        if err == MZ_OK {
            file_info.extrafield_size = read_into!(err, mz_stream_read_uint16(stream));
        }
        if !local {
            if err == MZ_OK {
                file_info.comment_size = read_into!(err, mz_stream_read_uint16(stream));
            }
            if err == MZ_OK {
                let value16: u16 = read_into!(err, mz_stream_read_uint16(stream));
                file_info.disk_number = u32::from(value16);
            }
            if err == MZ_OK {
                file_info.internal_fa = read_into!(err, mz_stream_read_uint16(stream));
            }
            if err == MZ_OK {
                file_info.external_fa = read_into!(err, mz_stream_read_uint32(stream));
            }
            if err == MZ_OK {
                let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
                file_info.disk_offset = i64::from(value32);
            }
        }
    }

    if err == MZ_OK {
        err = mz_stream_seek(file_extra_stream, 0, Origin::Set);
    }

    // Copy variable length data to memory stream for later retrieval
    if err == MZ_OK && file_info.filename_size > 0 {
        err = mz_stream_copy(
            file_extra_stream,
            stream,
            i32::from(file_info.filename_size),
        );
    }
    file_extra_stream.write_uint8(0);
    let extrafield_pos = file_extra_stream.tell();

    if err == MZ_OK && file_info.extrafield_size > 0 {
        err = mz_stream_copy(
            file_extra_stream,
            stream,
            i32::from(file_info.extrafield_size),
        );
    }
    file_extra_stream.write_uint8(0);

    let comment_pos = file_extra_stream.tell();
    if err == MZ_OK && file_info.comment_size > 0 {
        err = mz_stream_copy(file_extra_stream, stream, i32::from(file_info.comment_size));
    }
    file_extra_stream.write_uint8(0);

    let linkname_pos = file_extra_stream.tell();
    // Overwrite if we encounter UNIX1 extra block
    file_extra_stream.write_uint8(0);
    let _ = comment_pos;

    if err == MZ_OK && file_info.extrafield_size > 0 {
        // Seek to and parse the extra field
        err = mz_stream_seek(file_extra_stream, extrafield_pos, Origin::Set);

        let extrafield_size = u32::from(file_info.extrafield_size);
        let mut field_pos: u32 = 0;
        while err == MZ_OK && field_pos + 4 <= extrafield_size {
            // mz_zip_extrafield_read
            let field_type = match mz_stream_read_uint16(file_extra_stream) {
                Ok(v) => v,
                Err(e) => {
                    err = e;
                    break;
                }
            };
            let mut field_length = match mz_stream_read_uint16(file_extra_stream) {
                Ok(v) => v,
                Err(e) => {
                    err = e;
                    break;
                }
            };
            field_pos += 4;

            // Don't allow field length to exceed size of remaining extrafield
            if u32::from(field_length) > extrafield_size - field_pos {
                field_length = (extrafield_size - field_pos) as u16;
            }

            if field_type == MZ_ZIP_EXTENSION_ZIP64 && field_length >= 8 {
                // Read ZIP64 extra field
                if err == MZ_OK && file_info.uncompressed_size == i64::from(u32::MAX) {
                    file_info.uncompressed_size =
                        read_into!(err, mz_stream_read_int64(file_extra_stream));
                    if file_info.uncompressed_size < 0 {
                        err = MZ_FORMAT_ERROR;
                    }
                }
                if err == MZ_OK && file_info.compressed_size == i64::from(u32::MAX) {
                    file_info.compressed_size =
                        read_into!(err, mz_stream_read_int64(file_extra_stream));
                    if file_info.compressed_size < 0 {
                        err = MZ_FORMAT_ERROR;
                    }
                }
                if err == MZ_OK && file_info.disk_offset == i64::from(u32::MAX) {
                    file_info.disk_offset =
                        read_into!(err, mz_stream_read_int64(file_extra_stream));
                    if file_info.disk_offset < 0 {
                        err = MZ_FORMAT_ERROR;
                    }
                }
                if err == MZ_OK && file_info.disk_number == u32::from(u16::MAX) {
                    file_info.disk_number =
                        read_into!(err, mz_stream_read_uint32(file_extra_stream));
                }
            } else if field_type == MZ_ZIP_EXTENSION_NTFS && field_length > 4 {
                // Read NTFS extra field
                if err == MZ_OK {
                    let _reserved: u32 = read_into!(err, mz_stream_read_uint32(file_extra_stream));
                }
                let mut field_length_read: u32 = 4;

                while err == MZ_OK && field_length_read + 4 <= u32::from(field_length) {
                    let ntfs_attrib_id: u16 =
                        read_into!(err, mz_stream_read_uint16(file_extra_stream));
                    let mut ntfs_attrib_size: u16 = 0;
                    if err == MZ_OK {
                        ntfs_attrib_size =
                            read_into!(err, mz_stream_read_uint16(file_extra_stream));
                    }
                    field_length_read += 4;

                    if err == MZ_OK && ntfs_attrib_id == 0x01 && ntfs_attrib_size == 24 {
                        let ntfs_time: u64 =
                            read_into!(err, mz_stream_read_uint64(file_extra_stream));
                        file_info.modified_date = mz_zip_ntfs_to_unix_time(ntfs_time);

                        if err == MZ_OK {
                            let ntfs_time: u64 =
                                read_into!(err, mz_stream_read_uint64(file_extra_stream));
                            file_info.accessed_date = mz_zip_ntfs_to_unix_time(ntfs_time);
                        }
                        if err == MZ_OK {
                            let ntfs_time: u64 =
                                read_into!(err, mz_stream_read_uint64(file_extra_stream));
                            file_info.creation_date = mz_zip_ntfs_to_unix_time(ntfs_time);
                        }
                    } else if err == MZ_OK
                        && field_length_read + u32::from(ntfs_attrib_size)
                            <= u32::from(field_length)
                    {
                        err = mz_stream_seek(
                            file_extra_stream,
                            i64::from(ntfs_attrib_size),
                            Origin::Cur,
                        );
                    }

                    field_length_read += u32::from(ntfs_attrib_size);
                }
            } else if field_type == MZ_ZIP_EXTENSION_UNIX1 && field_length >= 12 {
                // Read UNIX1 extra field
                if err == MZ_OK {
                    let value32: u32 = read_into!(err, mz_stream_read_uint32(file_extra_stream));
                    if err == MZ_OK && file_info.accessed_date == 0 {
                        file_info.accessed_date = i64::from(value32);
                    }
                }
                if err == MZ_OK {
                    let value32: u32 = read_into!(err, mz_stream_read_uint32(file_extra_stream));
                    if err == MZ_OK && file_info.modified_date == 0 {
                        file_info.modified_date = i64::from(value32);
                    }
                }
                if err == MZ_OK {
                    let _: u16 = read_into!(err, mz_stream_read_uint16(file_extra_stream));
                }
                if err == MZ_OK {
                    let _: u16 = read_into!(err, mz_stream_read_uint16(file_extra_stream));
                }

                // Copy linkname to end of file extra stream so we can return null terminated
                // string
                let linkname_size = field_length - 12;
                if err == MZ_OK && linkname_size > 0 {
                    let mut linkname = vec![0u8; usize::from(linkname_size)];
                    if file_extra_stream.read(&mut linkname) != i32::from(linkname_size) {
                        err = MZ_READ_ERROR;
                    }
                    if err == MZ_OK {
                        let saved_pos = file_extra_stream.tell();

                        file_extra_stream.seek(linkname_pos, Origin::Set);
                        file_extra_stream.write(&linkname);
                        file_extra_stream.write_uint8(0);

                        file_extra_stream.seek(saved_pos, Origin::Set);
                    }
                }
            } else if field_length > 0 {
                err = mz_stream_seek(file_extra_stream, i64::from(field_length), Origin::Cur);
            }

            field_pos += u32::from(field_length);
        }
    }

    // Get pointers to variable length data
    let start = file_extra_stream.bytes_at(0);
    file_info.filename =
        start[..start.iter().position(|&b| b == 0).unwrap_or(start.len())].to_vec();
    let extra = file_extra_stream.bytes_at(extrafield_pos as usize);
    file_info.extrafield =
        extra[..extra.len().min(usize::from(file_info.extrafield_size))].to_vec();
    let link = file_extra_stream.bytes_at(linkname_pos as usize);
    file_info.linkname = link[..link.iter().position(|&b| b == 0).unwrap_or(link.len())].to_vec();

    err
}

/// The stream the central directory is read from (`cd_stream`): the archive's, or the
/// archive's memory stream (`cd_mem_stream`), holding a central directory minizip-ng
/// decompressed (`mz_zip_reader_unzip_cd`) or rebuilt (`mz_zip_recover_cd`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CdStream {
    Archive,
    Memory,
}

/// An archive opened for reading.
///
/// Port of `mz_zip` (mz_zip.c:84-118 @ 4.0.10), with the fields reading reaches.
#[derive(Debug)]
pub(crate) struct Zip {
    pub(crate) stream: SplitStream,
    pub(crate) cd_stream: CdStream,
    pub(crate) cd_mem_stream: MemStream,
    /// Whether `cd_mem_stream` was opened (`mz_stream_mem_is_open`: it has a buffer).
    pub(crate) cd_mem_open: bool,
    file_info_stream: MemStream,
    local_file_info_stream: MemStream,
    pub(crate) file_info: FileInfo,
    local_file_info: FileInfo,
    entry_scanned: bool,
    entry: Option<Entry>,
    cd_start_pos: i64,
    cd_current_pos: i64,
    cd_offset: i64,
    cd_size: i64,
    number_entry: u64,
    disk_number_with_cd: u32,
    disk_offset_shift: i64,
    cd_signature: u32,
    recover: bool,
}

impl Zip {
    /// The stream the central directory is read from.
    fn cd(&mut self) -> &mut dyn Stream {
        match self.cd_stream {
            CdStream::Archive => &mut self.stream,
            CdStream::Memory => &mut self.cd_mem_stream,
        }
    }

    /// Opens the archive on `stream` for reading, with recovery of a central directory it
    /// can't read when `recover`.
    ///
    /// Port of `mz_zip_create`, `mz_zip_set_recover` and `mz_zip_open` (mz_zip.c:1408-1413,
    /// 1589-1595, 1424-1505 @ 4.0.10) for reading.
    pub(crate) fn open(stream: SplitStream, recover: bool) -> Result<Zip, i32> {
        let mut zip = Zip {
            stream,
            cd_stream: CdStream::Archive,
            cd_mem_stream: MemStream::create(),
            cd_mem_open: false,
            file_info_stream: MemStream::create(),
            local_file_info_stream: MemStream::create(),
            file_info: FileInfo::default(),
            local_file_info: FileInfo::default(),
            entry_scanned: false,
            entry: None,
            cd_start_pos: 0,
            cd_current_pos: 0,
            cd_offset: 0,
            cd_size: 0,
            number_entry: 0,
            disk_number_with_cd: 0,
            disk_offset_shift: 0,
            cd_signature: 0,
            recover,
        };
        let mut err = zip.read_cd();
        if err != MZ_OK && zip.recover && zip.recover_cd() == MZ_OK {
            err = MZ_OK;
        }
        zip.cd_start_pos = zip.cd_offset;
        if err != MZ_OK {
            return Err(err);
        }
        Ok(zip)
    }

    /// Rebuilds the central directory from the local headers, into the archive's memory
    /// stream, where minizip-ng can't read it (the archive's reader asks for recovery).
    ///
    /// Port of `mz_zip_recover_cd` (mz_zip.c:1236-1406 @ 4.0.10).
    fn recover_cd(&mut self) -> i32 {
        let descriptor_magic = MZ_ZIP_MAGIC_DATADESCRIPTOR.to_le_bytes();
        let local_header_magic = MZ_ZIP_MAGIC_LOCALHEADER.to_le_bytes();
        let central_header_magic = MZ_ZIP_MAGIC_CENTRALHEADER.to_le_bytes();
        let mut number_entry: u64 = 0;
        let mut next_header_pos: i64 = 0;
        let mut compressed_end_pos: i64 = 0;
        let mut disk_number_with_cd: u32 = 0;
        let mut err = MZ_OK;
        let mut zip64 = false;
        let mut eof = false;

        // Determine if we are on a split disk or not
        self.stream.set_disk_number(0);
        if self.stream.tell() < 0 {
            self.stream.set_disk_number(-1);
            mz_stream_seek(&mut self.stream, 0, Origin::Set);
        } else {
            disk_number_with_cd = 1;
        }

        let mut local_file_info_stream = MemStream::create();

        if !self.cd_mem_open {
            self.cd_mem_stream = MemStream::create();
            self.cd_mem_open = true;
        }

        match mz_stream_find(&mut self.stream, &local_header_magic, i64::MAX) {
            Ok(pos) => next_header_pos = pos,
            Err(e) => err = e,
        }

        while err == MZ_OK && !eof {
            // Get current offset and disk number for central dir record
            let disk_offset = self.stream.tell();
            let mut disk_number = self.stream.disk_number();

            // Read local headers
            let mut local_file_info = FileInfo::default();
            err = mz_zip_entry_read_header(
                &mut self.stream,
                true,
                &mut local_file_info,
                &mut local_file_info_stream,
            );
            if err != MZ_OK {
                break;
            }

            local_file_info.disk_offset = disk_offset;
            if disk_number < 0 {
                disk_number = 0;
            }
            local_file_info.disk_number = disk_number as u32;

            let compressed_pos = self.stream.tell();

            if local_file_info.compressed_size > 0 {
                mz_stream_seek(
                    &mut self.stream,
                    local_file_info.compressed_size,
                    Origin::Cur,
                );
            }

            loop {
                // Search for the next local header
                let found = mz_stream_find(&mut self.stream, &local_header_magic, i64::MAX);
                err = match found {
                    Ok(pos) => {
                        next_header_pos = pos;
                        MZ_OK
                    }
                    Err(e) => e,
                };

                if err == MZ_EXIST_ERROR {
                    mz_stream_seek(&mut self.stream, compressed_pos, Origin::Set);

                    // Search for central dir if no local header found
                    let found = mz_stream_find(&mut self.stream, &central_header_magic, i64::MAX);
                    err = match found {
                        Ok(pos) => {
                            next_header_pos = pos;
                            MZ_OK
                        }
                        Err(e) => e,
                    };

                    if err == MZ_EXIST_ERROR {
                        // Get end of stream if no central header found
                        mz_stream_seek(&mut self.stream, 0, Origin::End);
                        next_header_pos = self.stream.tell();
                    }

                    eof = true;
                }

                if local_file_info.flag & MZ_ZIP_FLAG_DATA_DESCRIPTOR != 0
                    || local_file_info.compressed_size == 0
                {
                    // Search backwards for the descriptor, seeking too far back will be
                    // incorrect if compressed size is small
                    let found = mz_stream_find_reverse(
                        &mut self.stream,
                        &descriptor_magic,
                        MZ_ZIP_SIZE_MAX_DATA_DESCRIPTOR,
                    );
                    // Its error, as the descriptor's below, is the loop's `err` until the
                    // header is written, which replaces it.
                    if let Ok(descriptor_pos) = found {
                        if mz_zip_extrafield_contains(
                            &local_file_info.extrafield,
                            i32::from(local_file_info.extrafield_size),
                            MZ_ZIP_EXTENSION_ZIP64,
                        ) == MZ_OK
                        {
                            zip64 = true;
                        }

                        if let Ok((crc32, compressed_size, uncompressed_size)) =
                            mz_zip_entry_read_descriptor(&mut self.stream, zip64)
                        {
                            if local_file_info.crc == 0 {
                                local_file_info.crc = crc32;
                            }
                            if local_file_info.compressed_size == 0 {
                                local_file_info.compressed_size = compressed_size;
                            }
                            if local_file_info.uncompressed_size == 0 {
                                local_file_info.uncompressed_size = uncompressed_size;
                            }
                        }

                        compressed_end_pos = descriptor_pos;
                    } else if eof {
                        compressed_end_pos = next_header_pos;
                    } else if local_file_info.flag & MZ_ZIP_FLAG_DATA_DESCRIPTOR != 0 {
                        // Wrong local file entry found, keep searching
                        next_header_pos += 1;
                        mz_stream_seek(&mut self.stream, next_header_pos, Origin::Set);
                        continue;
                    }
                } else {
                    compressed_end_pos = next_header_pos;
                }

                break;
            }

            let compressed_size = compressed_end_pos - compressed_pos;

            if compressed_size > i64::from(u32::MAX) {
                // Update sizes if 4GB file is written with no ZIP64 support
                if local_file_info.uncompressed_size < i64::from(u32::MAX) {
                    local_file_info.compressed_size = compressed_size;
                    local_file_info.uncompressed_size = 0;
                }
            }

            // Rewrite central dir with local headers and offsets
            err = mz_zip_entry_write_header(&mut self.cd_mem_stream, false, &local_file_info);
            if err == MZ_OK {
                number_entry += 1;
            }

            err = mz_stream_seek(&mut self.stream, next_header_pos, Origin::Set);
        }

        if number_entry == 0 {
            return err;
        }

        // Set new upper seek boundary for central dir mem stream
        let disk_offset = self.cd_mem_stream.tell();
        self.cd_mem_stream.set_buffer_limit(disk_offset as usize);

        // Set new central directory info
        self.set_cd_stream(0, CdStream::Memory);
        self.number_entry = number_entry;
        self.disk_number_with_cd = disk_number_with_cd;

        MZ_OK
    }

    /// Port of `mz_zip_search_eocd` (mz_zip.c:131-147 @ 4.0.10).
    fn search_eocd(&mut self) -> Result<i64, i32> {
        let err = mz_stream_seek(&mut self.stream, 0, Origin::End);
        if err != MZ_OK {
            return Err(err);
        }
        let file_size = self.stream.tell();
        let mut max_back = MZ_ZIP_EOCD_MAX_BACK;
        if max_back <= 0 || max_back > file_size {
            max_back = file_size;
        }
        mz_stream_find_reverse(
            &mut self.stream,
            &MZ_ZIP_MAGIC_ENDHEADER.to_le_bytes(),
            max_back,
        )
    }

    /// Port of `mz_zip_search_zip64_eocd` (mz_zip.c:150-188 @ 4.0.10).
    fn search_zip64_eocd(&mut self, end_central_offset: i64) -> Result<i64, i32> {
        let stream = &mut self.stream;
        // Zip64 end of central directory locator
        let mut err = mz_stream_seek(
            stream,
            end_central_offset - MZ_ZIP_SIZE_CD_LOCATOR64,
            Origin::Set,
        );
        let mut offset: i64 = 0;
        // Read locator signature
        if err == MZ_OK {
            let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
            if value32 != MZ_ZIP_MAGIC_ENDLOCHEADER64 {
                err = MZ_FORMAT_ERROR;
            }
        }
        // Number of the disk with the start of the zip64 end of  central directory
        if err == MZ_OK {
            let _: u32 = read_into!(err, mz_stream_read_uint32(stream));
        }
        // Relative offset of the zip64 end of central directory record8
        if err == MZ_OK {
            let value: u64 = read_into!(err, mz_stream_read_uint64(stream));
            offset = value as i64;
        }
        // Total number of disks
        if err == MZ_OK {
            let _: u32 = read_into!(err, mz_stream_read_uint32(stream));
        }
        // Goto end of central directory record
        if err == MZ_OK {
            err = mz_stream_seek(stream, offset, Origin::Set);
        }
        // The signature
        if err == MZ_OK {
            let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
            if value32 != MZ_ZIP_MAGIC_ENDHEADER64 {
                err = MZ_FORMAT_ERROR;
            }
        }

        if err == MZ_OK { Ok(offset) } else { Err(err) }
    }

    /// Reads the end of the central directory, and finds the central directory.
    ///
    /// Port of `mz_zip_read_cd` (mz_zip.c:947-1100 @ 4.0.10); the comment is read and not
    /// kept.
    fn read_cd(&mut self) -> i32 {
        let mut eocd_pos64: i64 = 0;
        let mut err;

        // Read and cache central directory records
        let mut eocd_pos = match self.search_eocd() {
            Ok(pos) => {
                err = MZ_OK;
                pos
            }
            Err(e) => {
                err = e;
                -1
            }
        };
        if err == MZ_OK {
            let stream = &mut self.stream;
            let mut value16: u16 = 0;
            // The signature, already checked
            let _: u32 = read_into!(err, mz_stream_read_uint32(stream));
            // Number of this disk
            if err == MZ_OK {
                value16 = read_into!(err, mz_stream_read_uint16(stream));
            }
            // Number of the disk with the start of the central directory
            if err == MZ_OK {
                value16 = read_into!(err, mz_stream_read_uint16(stream));
            }
            self.disk_number_with_cd = u32::from(value16);
            // Total number of entries in the central dir on this disk
            if err == MZ_OK {
                value16 = read_into!(err, mz_stream_read_uint16(stream));
            }
            self.number_entry = u64::from(value16);
            // Total number of entries in the central dir
            if err == MZ_OK {
                value16 = read_into!(err, mz_stream_read_uint16(stream));
            }
            let number_entry_cd = u64::from(value16);
            // When recover is enabled, we can ignore incorrect number of entries
            if number_entry_cd != self.number_entry && !self.recover {
                err = MZ_FORMAT_ERROR;
            }
            // Size of the central directory
            if err == MZ_OK {
                let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
                if err == MZ_OK {
                    self.cd_size = i64::from(value32);
                }
            }
            // Offset of start of central directory with respect to the starting disk number
            if err == MZ_OK {
                let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
                if err == MZ_OK {
                    self.cd_offset = i64::from(value32);
                }
            }
            // Zip file global comment length; the comment, which isn't critical, isn't kept
            if err == MZ_OK {
                let comment_size: u16 = read_into!(err, mz_stream_read_uint16(stream));
                if err == MZ_OK && comment_size > 0 {
                    let mut comment = vec![0u8; usize::from(comment_size)];
                    let _ = stream.read(&mut comment);
                }
            }

            if err == MZ_OK
                && (number_entry_cd == u64::from(u16::MAX) || self.cd_offset == i64::from(u32::MAX))
            {
                // Format should be Zip64, as the central directory or file size is too large
                if let Ok(pos64) = self.search_zip64_eocd(eocd_pos) {
                    eocd_pos64 = pos64;
                    eocd_pos = eocd_pos64;

                    let stream = &mut self.stream;
                    err = mz_stream_seek(stream, eocd_pos, Origin::Set);
                    // The signature, already checked
                    if err == MZ_OK {
                        let _: u32 = read_into!(err, mz_stream_read_uint32(stream));
                    }
                    // Size of zip64 end of central directory record
                    if err == MZ_OK {
                        let _: u64 = read_into!(err, mz_stream_read_uint64(stream));
                    }
                    // Version made by
                    if err == MZ_OK {
                        let _version_madeby: u16 = read_into!(err, mz_stream_read_uint16(stream));
                    }
                    // Version needed to extract
                    if err == MZ_OK {
                        let _: u16 = read_into!(err, mz_stream_read_uint16(stream));
                    }
                    // Number of this disk
                    if err == MZ_OK {
                        let _: u32 = read_into!(err, mz_stream_read_uint32(stream));
                    }
                    // Number of the disk with the start of the central directory
                    if err == MZ_OK {
                        self.disk_number_with_cd = read_into!(err, mz_stream_read_uint32(stream));
                    }
                    // Total number of entries in the central directory on this disk
                    if err == MZ_OK {
                        self.number_entry = read_into!(err, mz_stream_read_uint64(stream));
                    }
                    // Total number of entries in the central directory
                    let mut number_entry_cd64: u64 = 0;
                    if err == MZ_OK {
                        number_entry_cd64 = read_into!(err, mz_stream_read_uint64(stream));
                    }
                    if self.number_entry != number_entry_cd64 {
                        err = MZ_FORMAT_ERROR;
                    }
                    // Size of the central directory
                    if err == MZ_OK {
                        self.cd_size = read_into!(err, mz_stream_read_int64(stream));
                        if self.cd_size < 0 {
                            err = MZ_FORMAT_ERROR;
                        }
                    }
                    // Offset of start of central directory with respect to the starting disk
                    // number
                    if err == MZ_OK {
                        self.cd_offset = read_into!(err, mz_stream_read_int64(stream));
                        if self.cd_offset < 0 {
                            err = MZ_FORMAT_ERROR;
                        }
                    }
                } else if self.number_entry == u64::from(u16::MAX)
                    || number_entry_cd != self.number_entry
                    || self.cd_size == i64::from(u16::MAX)
                    || self.cd_offset == i64::from(u32::MAX)
                {
                    err = MZ_FORMAT_ERROR;
                }
            }
        }

        if err == MZ_OK {
            // Verify central directory signature exists at offset
            err = mz_stream_seek(&mut self.stream, self.cd_offset, Origin::Set);
            if err == MZ_OK {
                self.cd_signature = read_into!(err, mz_stream_read_uint32(&mut self.stream));
            }
            if err == MZ_OK && self.cd_signature != MZ_ZIP_MAGIC_CENTRALHEADER {
                // If cd exists in large file and no zip-64 support, error for recover
                if eocd_pos > i64::from(u32::MAX) && eocd_pos64 == 0 {
                    err = MZ_FORMAT_ERROR;
                }
                // If cd not found attempt to seek backward to find it
                if err == MZ_OK {
                    err = mz_stream_seek(&mut self.stream, eocd_pos - self.cd_size, Origin::Set);
                }
                if err == MZ_OK {
                    self.cd_signature = read_into!(err, mz_stream_read_uint32(&mut self.stream));
                }
                if err == MZ_OK && self.cd_signature == MZ_ZIP_MAGIC_CENTRALHEADER {
                    // If found compensate for incorrect locations
                    let value64i = self.cd_offset;
                    self.cd_offset = eocd_pos - self.cd_size;
                    // Assume disk has prepended data
                    self.disk_offset_shift = self.cd_offset - value64i;
                }
            }
        }

        if err == MZ_OK {
            if eocd_pos < self.cd_offset {
                // End of central dir should always come after central dir
                err = MZ_FORMAT_ERROR;
            } else if (eocd_pos as u64) < (self.cd_offset as u64).wrapping_add(self.cd_size as u64)
            {
                // Truncate size of cd if incorrect size or offset provided
                self.cd_size = eocd_pos - self.cd_offset;
            }
        }

        err
    }

    /// Reads the central header at the current position.
    ///
    /// Port of `mz_zip_goto_next_entry_int` (mz_zip.c:2349-2366 @ 4.0.10).
    fn goto_next_entry_int(&mut self) -> i32 {
        self.entry_scanned = false;

        if let CdStream::Archive = self.cd_stream {
            self.stream.set_disk_number(-1);
        }

        let pos = self.cd_current_pos;
        let mut err = mz_stream_seek(self.cd(), pos, Origin::Set);
        if err == MZ_OK {
            let mut file_info = FileInfo::default();
            let mut extra = std::mem::take(&mut self.file_info_stream);
            err = mz_zip_entry_read_header(self.cd(), false, &mut file_info, &mut extra);
            self.file_info_stream = extra;
            self.file_info = file_info;
        }
        if err == MZ_OK {
            self.entry_scanned = true;
        }
        err
    }

    /// Port of `mz_zip_goto_first_entry` (mz_zip.c:2391-2400 @ 4.0.10).
    pub(crate) fn goto_first_entry(&mut self) -> i32 {
        self.cd_current_pos = self.cd_start_pos;
        self.goto_next_entry_int()
    }

    /// Port of `mz_zip_goto_next_entry` (mz_zip.c:2402-2412 @ 4.0.10).
    pub(crate) fn goto_next_entry(&mut self) -> i32 {
        self.cd_current_pos += MZ_ZIP_SIZE_CD_ITEM
            + i64::from(self.file_info.filename_size)
            + i64::from(self.file_info.extrafield_size)
            + i64::from(self.file_info.comment_size);

        self.goto_next_entry_int()
    }

    /// Whether an entry was scanned (`entry_scanned`).
    pub(crate) fn entry_scanned(&self) -> bool {
        self.entry_scanned
    }

    /// Port of `mz_zip_set_cd_stream` (mz_zip.c:1615-1623 @ 4.0.10).
    pub(crate) fn set_cd_stream(&mut self, cd_start_pos: i64, cd_stream: CdStream) {
        self.cd_offset = 0;
        self.cd_stream = cd_stream;
        self.cd_start_pos = cd_start_pos;
    }

    /// Port of `mz_zip_entry_is_open` (mz_zip.c:1865-1872 @ 4.0.10).
    pub(crate) fn entry_is_open(&self) -> bool {
        self.entry.is_some()
    }

    /// Port of `mz_zip_entry_seek_local_header` (mz_zip.c:2235-2256 @ 4.0.10) for reading.
    fn entry_seek_local_header(&mut self) -> i32 {
        let mut disk_number = self.file_info.disk_number;

        // The disk size is 0 when reading.
        if disk_number == self.disk_number_with_cd {
            disk_number = u32::MAX;
        }

        self.stream.set_disk_number(i64::from(disk_number));

        // Guard against seek overflows
        if self.disk_offset_shift > 0
            && self.file_info.disk_offset > i64::MAX - self.disk_offset_shift
        {
            return MZ_FORMAT_ERROR;
        }

        let offset = self.file_info.disk_offset + self.disk_offset_shift;
        mz_stream_seek(&mut self.stream, offset, Origin::Set)
    }

    /// Opens the scanned entry for reading, without a password.
    ///
    /// Port of `mz_zip_entry_read_open` (mz_zip.c:1874-1913 @ 4.0.10) with `raw` 0 and no
    /// password, through `mz_zip_entry_open_int` (mz_zip.c:1682-1863): without PKWARE and AES
    /// decryption (not built in OpenColorIO's minizip-ng), an encrypted entry's data is read as
    /// it is stored.
    pub(crate) fn entry_read_open(&mut self) -> i32 {
        if !self.entry_scanned {
            return MZ_PARAM_ERROR;
        }

        let mut err = self.entry_seek_local_header();
        if err == MZ_OK {
            let mut local = FileInfo::default();
            err = mz_zip_entry_read_header(
                &mut self.stream,
                true,
                &mut local,
                &mut self.local_file_info_stream,
            );
            self.local_file_info = local;
        }

        if err == MZ_FORMAT_ERROR && self.disk_offset_shift > 0 {
            // Perhaps we didn't compensated correctly for incorrect cd offset
            let mut err_shift =
                mz_stream_seek(&mut self.stream, self.file_info.disk_offset, Origin::Set);
            if err_shift == MZ_OK {
                let mut local = FileInfo::default();
                err_shift = mz_zip_entry_read_header(
                    &mut self.stream,
                    true,
                    &mut local,
                    &mut self.local_file_info_stream,
                );
                self.local_file_info = local;
            }
            if err_shift == MZ_OK {
                self.disk_offset_shift = 0;
                err = err_shift;
            }
        }

        if err == MZ_OK {
            err = self.entry_open_int();
        }

        err
    }

    /// Port of `mz_zip_entry_open_int` (mz_zip.c:1682-1863 @ 4.0.10) for reading, without a
    /// password, `raw` 0, and only the store and deflate methods.
    fn entry_open_int(&mut self) -> i32 {
        let method = self.file_info.compression_method;
        if method != MZ_COMPRESS_METHOD_STORE && method != MZ_COMPRESS_METHOD_DEFLATE {
            return MZ_SUPPORT_ERROR;
        }

        // The raw stream of the stored or encrypted data is limited to its compressed size;
        // zlib reads past it otherwise.
        let limited =
            method == MZ_COMPRESS_METHOD_STORE || self.file_info.flag & MZ_ZIP_FLAG_ENCRYPTED != 0;
        let max_total_in = if limited {
            self.file_info.compressed_size
        } else {
            0
        };

        self.entry = Some(Entry {
            deflate: method == MZ_COMPRESS_METHOD_DEFLATE,
            max_total_in,
            total_in: 0,
            // OpenColorIO's readers ask for at most the entry's size, as a C `int32_t`, and
            // one more read past it (of 64 KiB at most).
            bound: (self.file_info.uncompressed_size as i32).max(0) as usize + 65535,
            inflated: None,
            total_out: 0,
        });
        MZ_OK
    }

    /// Reads up to `buf.len()` bytes of the open entry's data: how many it read (0 at its
    /// end), or an error.
    ///
    /// Port of `mz_zip_entry_read` (mz_zip.c:2031-2054 @ 4.0.10), through `mz_stream_raw_read`
    /// (mz_strm.c:442-460) and `mz_stream_zlib_read` (mz_strm_zlib.c:116-193); see
    /// [`inflate`].
    pub(crate) fn entry_read(&mut self, buf: &mut [u8]) -> i32 {
        let Some(entry) = self.entry.as_mut() else {
            return MZ_PARAM_ERROR;
        };
        if buf.is_empty() {
            return MZ_PARAM_ERROR;
        }

        if self.file_info.compressed_size == 0 {
            return 0;
        }

        // Read entire entry even if uncompressed_size = 0, otherwise
        // aes encryption validation will fail if compressed_size > 0
        if !entry.deflate {
            let mut bytes_to_read = buf.len() as i64;
            if entry.max_total_in > 0 && bytes_to_read > entry.max_total_in - entry.total_in {
                bytes_to_read = entry.max_total_in - entry.total_in;
            }
            let read = self.stream.read(&mut buf[..bytes_to_read.max(0) as usize]);
            if read > 0 {
                entry.total_in += i64::from(read);
            }
            return read;
        }

        if entry.inflated.is_none() {
            // The input zlib reads, 32767 bytes at a time, up to the limit when there is one;
            // a read that fails ends it, as the end of the stream does.
            let mut input = Vec::new();
            let mut chunk = vec![0u8; i16::MAX as usize];
            loop {
                let mut bytes_to_read = chunk.len() as i64;
                if entry.max_total_in > 0 {
                    bytes_to_read = bytes_to_read.min(entry.max_total_in - input.len() as i64);
                }
                let read = self
                    .stream
                    .read(&mut chunk[..bytes_to_read.max(0) as usize]);
                if read <= 0 {
                    break;
                }
                input.extend_from_slice(&chunk[..read as usize]);
            }
            entry.inflated = Some(inflate(&input, entry.bound));
        }
        let (out, stop) = entry.inflated.as_ref().expect("inflated");
        let p = entry.total_out;
        if p + buf.len() <= out.len() {
            buf.copy_from_slice(&out[p..p + buf.len()]);
            entry.total_out += buf.len();
            return buf.len() as i32;
        }
        if *stop == Stop::End {
            let n = out.len() - p;
            buf[..n].copy_from_slice(&out[p..]);
            entry.total_out += n;
            return n as i32;
        }
        MZ_BUF_ERROR
    }

    /// Closes the open entry. Without the out parameters, nothing is read or checked.
    ///
    /// Port of `mz_zip_entry_close` (mz_zip.c:2268-2285 @ 4.0.10), through
    /// `mz_zip_entry_read_close` (mz_zip.c:2070-2134) with no out parameter.
    pub(crate) fn entry_close(&mut self) -> i32 {
        if self.entry.take().is_none() {
            return MZ_PARAM_ERROR;
        }
        MZ_OK
    }
}

/// The open entry's streams: the raw stream of its data, or the zlib stream over it.
#[derive(Debug)]
struct Entry {
    deflate: bool,
    /// The raw stream's limit (`MZ_STREAM_PROP_TOTAL_IN_MAX`), 0 for none.
    max_total_in: i64,
    total_in: i64,
    /// How many bytes of the deflate stream are decoded.
    bound: usize,
    /// The deflate stream decoded: the bytes before it ends or fails, and how it stopped.
    inflated: Option<(Vec<u8>, Stop)>,
    total_out: usize,
}

/// Port of `mz_zip_extrafield_find` (mz_zip.c:2583-2612 @ 4.0.10): positions `stream` after
/// the header of the first field of `type_` and returns its length.
pub(crate) fn mz_zip_extrafield_find(
    stream: &mut dyn Stream,
    type_: u16,
    max_seek: i32,
) -> Result<u16, i32> {
    let mut max_seek = max_seek;
    if max_seek < 4 {
        return Err(MZ_EXIST_ERROR);
    }

    while let Ok(field_type) = mz_stream_read_uint16(stream) {
        let Ok(field_length) = mz_stream_read_uint16(stream) else {
            break;
        };

        if type_ == field_type {
            return Ok(field_length);
        }

        max_seek -= i32::from(field_length) - 4;
        if max_seek < 0 {
            return Err(MZ_EXIST_ERROR);
        }

        if mz_stream_seek(stream, i64::from(field_length), Origin::Cur) != MZ_OK {
            break;
        }
    }

    Err(MZ_EXIST_ERROR)
}

/// Reads a data descriptor: its CRC and its compressed and uncompressed sizes, 8 bytes each
/// with `zip64`.
///
/// Port of `mz_zip_entry_read_descriptor` (mz_zip.c:481-520 @ 4.0.10).
fn mz_zip_entry_read_descriptor(
    stream: &mut dyn Stream,
    zip64: bool,
) -> Result<(u32, i64, i64), i32> {
    let mut err;
    let value32: u32 = match mz_stream_read_uint32(stream) {
        Ok(v) => {
            err = MZ_OK;
            v
        }
        Err(e) => {
            err = e;
            0
        }
    };
    if value32 != MZ_ZIP_MAGIC_DATADESCRIPTOR {
        err = MZ_FORMAT_ERROR;
    }
    let mut crc32 = 0;
    if err == MZ_OK {
        crc32 = read_into!(err, mz_stream_read_uint32(stream));
    }
    let mut sizes = [0i64; 2];
    for size in &mut sizes {
        if err == MZ_OK {
            // If zip 64 extension is enabled then read as 8 byte
            let value64 = if !zip64 {
                let value32: u32 = read_into!(err, mz_stream_read_uint32(stream));
                i64::from(value32)
            } else {
                let value64: i64 = read_into!(err, mz_stream_read_int64(stream));
                if value64 < 0 {
                    err = MZ_FORMAT_ERROR;
                }
                value64
            };
            if err == MZ_OK {
                *size = value64;
            }
        }
    }
    if err == MZ_OK {
        Ok((crc32, sizes[0], sizes[1]))
    } else {
        Err(err)
    }
}

/// Whether the extra field `extrafield` has a field of `type_`: `MZ_OK`, or an error.
///
/// Port of `mz_zip_extrafield_contains` (mz_zip.c:2614-2630 @ 4.0.10).
fn mz_zip_extrafield_contains(extrafield: &[u8], extrafield_size: i32, type_: u16) -> i32 {
    if extrafield_size == 0 {
        return MZ_PARAM_ERROR;
    }
    let mut file_extra_stream = MemStream::over(extrafield);
    match mz_zip_extrafield_find(&mut file_extra_stream, type_, extrafield_size) {
        Ok(_) => MZ_OK,
        Err(e) => e,
    }
}

/// Whether a central header needs the zip64 extra field, for an entry whose `zip64` setting
/// is `MZ_ZIP64_AUTO` (as a rebuilt entry's is).
///
/// Port of `mz_zip_entry_needs_zip64` (mz_zip.c:551-592 @ 4.0.10) for a central header.
fn mz_zip_entry_needs_zip64(file_info: &FileInfo) -> bool {
    let max_uncompressed_size = i64::from(u32::MAX);
    let mut needs_zip64 = file_info.uncompressed_size >= max_uncompressed_size
        || file_info.compressed_size >= i64::from(u32::MAX);
    // Disk offset and number only used in central directory header
    needs_zip64 |= file_info.disk_offset >= i64::from(u32::MAX)
        || file_info.disk_number >= u32::from(u16::MAX);
    needs_zip64
}

/// Writes `file_info`'s central header to `stream`.
///
/// Port of `mz_zip_entry_write_header` (mz_zip.c:594-919 @ 4.0.10) for a central header of an
/// entry minizip-ng rebuilt from its local header (no comment, no masking, no AES; its external
/// attributes, 0, aren't a directory's). The DOS date and the NTFS times are written as zeros:
/// minizip-ng reads them back without using them (`mz_zip_entry_read_header`).
fn mz_zip_entry_write_header(stream: &mut MemStream, local: bool, file_info: &FileInfo) -> i32 {
    let w16 = |stream: &mut MemStream, v: u16| stream.write(&v.to_le_bytes());
    let w32 = |stream: &mut MemStream, v: u32| stream.write(&v.to_le_bytes());
    let w64 = |stream: &mut MemStream, v: u64| stream.write(&v.to_le_bytes());
    debug_assert!(!local);

    // Determine if zip64 extra field is necessary
    let zip64 = mz_zip_entry_needs_zip64(file_info);

    let mut extrafield_size: u16 = 0;
    let mut field_length_zip64: u16 = 0;
    let mut field_length_ntfs: u16 = 0;
    let mut field_length_unix1: u16 = 0;

    // Start calculating extra field sizes
    if zip64 {
        // Both compressed and uncompressed sizes must be included (at least in local header)
        field_length_zip64 = 8 + 8;
        if file_info.disk_offset >= i64::from(u32::MAX) {
            field_length_zip64 += 8;
        }

        extrafield_size = extrafield_size.wrapping_add(4 + field_length_zip64);
    }

    // Calculate extra field size and check for duplicates
    let mut file_extra_stream = MemStream::over(&file_info.extrafield);
    if file_info.extrafield_size > 0 {
        while let Ok(field_type) = mz_stream_read_uint16(&mut file_extra_stream) {
            let Ok(field_length) = mz_stream_read_uint16(&mut file_extra_stream) else {
                break;
            };

            // Prefer our zip64, ntfs, unix1 extension over incoming
            if field_type != MZ_ZIP_EXTENSION_ZIP64
                && field_type != MZ_ZIP_EXTENSION_NTFS
                && field_type != MZ_ZIP_EXTENSION_UNIX1
            {
                extrafield_size = extrafield_size.wrapping_add(4).wrapping_add(field_length);
            }

            if file_extra_stream.seek(i64::from(field_length), Origin::Cur) != MZ_OK {
                break;
            }
        }
    }

    // NTFS timestamps
    if file_info.modified_date != 0 && file_info.accessed_date != 0 && file_info.creation_date != 0
    {
        field_length_ntfs = 8 + 8 + 8 + 4 + 2 + 2;
        extrafield_size = extrafield_size.wrapping_add(4 + field_length_ntfs);
    }

    // Unix1 symbolic links
    let linkname_size = file_info.linkname.len() as u16;
    if linkname_size != 0 {
        field_length_unix1 = 12u16.wrapping_add(linkname_size);
        extrafield_size = extrafield_size
            .wrapping_add(4)
            .wrapping_add(field_length_unix1);
    }

    w32(stream, MZ_ZIP_MAGIC_CENTRALHEADER);
    w16(stream, file_info.version_madeby);

    // Calculate version needed to extract
    let mut version_needed = file_info.version_needed;
    if version_needed == 0 {
        version_needed = 20;
        if zip64 {
            version_needed = 45;
        }
    }
    w16(stream, version_needed);
    w16(stream, file_info.flag);
    w16(stream, file_info.compression_method);
    // The DOS date.
    w32(stream, 0);

    // mz_zip_entry_write_crc_sizes (mz_zip.c:522-549), unmasked
    w32(stream, file_info.crc);
    // For backwards-compatibility with older zip applications we set all sizes to UINT32_MAX
    // when zip64 is needed, instead of only setting sizes larger than UINT32_MAX.
    if zip64 {
        w32(stream, u32::MAX);
        w32(stream, u32::MAX);
    } else {
        w32(stream, file_info.compressed_size as u32);
        w32(stream, file_info.uncompressed_size as u32);
    }

    let filename = &file_info.filename;
    let filename_length = filename.len() as u16;
    w16(stream, filename_length);
    w16(stream, extrafield_size);

    // No comment.
    w16(stream, 0);
    w16(stream, file_info.disk_number as u16);
    w16(stream, file_info.internal_fa);
    w32(stream, file_info.external_fa);
    if file_info.disk_offset >= i64::from(u32::MAX) {
        w32(stream, u32::MAX);
    } else {
        w32(stream, file_info.disk_offset as u32);
    }

    // Ensure all slashes are written as forward slashes according to 4.4.17.1 (on Windows
    // only, `#if defined(_WIN32)`).
    let name = &filename[..usize::from(filename_length).min(filename.len())];
    if cfg!(windows) {
        let forward: Vec<u8> = name
            .iter()
            .map(|&b| if b == b'\\' { b'/' } else { b })
            .collect();
        stream.write(&forward);
    } else {
        stream.write(name);
    }

    // Write ZIP64 extra field first so we can update sizes later if data descriptor not used
    if zip64 {
        w16(stream, MZ_ZIP_EXTENSION_ZIP64);
        w16(stream, field_length_zip64);
        w64(stream, file_info.uncompressed_size as u64);
        w64(stream, file_info.compressed_size as u64);
        if file_info.disk_offset >= i64::from(u32::MAX) {
            w64(stream, file_info.disk_offset as u64);
        }
        if file_info.disk_number >= u32::from(u16::MAX) {
            w32(stream, file_info.disk_number);
        }
    }
    // Write NTFS extra field
    if field_length_ntfs > 0 {
        w16(stream, MZ_ZIP_EXTENSION_NTFS);
        w16(stream, field_length_ntfs);
        w32(stream, 0);
        w16(stream, 0x01);
        w16(stream, field_length_ntfs - 8);
        // The modification, access and creation times.
        for _ in 0..3 {
            w64(stream, 0);
        }
    }
    // Write UNIX extra block extra field
    if field_length_unix1 > 0 {
        w16(stream, MZ_ZIP_EXTENSION_UNIX1);
        w16(stream, field_length_unix1);
        w32(stream, file_info.accessed_date as u32);
        w32(stream, file_info.modified_date as u32);
        // User id, group id
        w16(stream, 0);
        w16(stream, 0);
        stream.write(&file_info.linkname[..usize::from(linkname_size)]);
    }

    let mut err = MZ_OK;
    if file_info.extrafield_size > 0 {
        let mut err_mem = file_extra_stream.seek(0, Origin::Set);
        while err == MZ_OK && err_mem == MZ_OK {
            let field_type = match mz_stream_read_uint16(&mut file_extra_stream) {
                Ok(v) => v,
                Err(_) => break,
            };
            let field_length = match mz_stream_read_uint16(&mut file_extra_stream) {
                Ok(v) => v,
                Err(_) => break,
            };

            // Prefer our zip 64, ntfs, unix1 extensions over incoming
            if field_type == MZ_ZIP_EXTENSION_ZIP64
                || field_type == MZ_ZIP_EXTENSION_NTFS
                || field_type == MZ_ZIP_EXTENSION_UNIX1
            {
                err_mem = file_extra_stream.seek(i64::from(field_length), Origin::Cur);
                continue;
            }

            w16(stream, field_type);
            w16(stream, field_length);
            err = mz_stream_copy(stream, &mut file_extra_stream, i32::from(field_length));
        }
    }

    err
}
