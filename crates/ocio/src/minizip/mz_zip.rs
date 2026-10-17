// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's zip archive, opened for reading (`mz_zip.c`): the central directory, the
//! headers of its entries, and an entry's data.

use super::inflate::{Stop, inflate};
use super::mz_strm::{
    MZ_BUF_ERROR, MZ_END_OF_LIST, MZ_END_OF_STREAM, MZ_FORMAT_ERROR, MZ_OK, MZ_PARAM_ERROR,
    MZ_READ_ERROR, MZ_STREAM_ERROR, MZ_SUPPORT_ERROR, MemStream, Origin, SplitStream, Stream,
    mz_stream_find_reverse, mz_stream_read_int64, mz_stream_read_uint16, mz_stream_read_uint32,
    mz_stream_read_uint64, mz_stream_seek,
};

/// The signatures (mz_zip.c:59-66 @ 4.0.10).
const MZ_ZIP_MAGIC_LOCALHEADER: u32 = 0x0403_4b50;
const MZ_ZIP_MAGIC_CENTRALHEADER: u32 = 0x0201_4b50;
const MZ_ZIP_MAGIC_ENDHEADER: u32 = 0x0605_4b50;
const MZ_ZIP_MAGIC_ENDHEADER64: u32 = 0x0606_4b50;
const MZ_ZIP_MAGIC_ENDLOCHEADER64: u32 = 0x0706_4b50;

/// The sizes of a central header and of the zip64 end locator (mz_zip.c:71-72 @ 4.0.10).
const MZ_ZIP_SIZE_CD_ITEM: i64 = 46;
const MZ_ZIP_SIZE_CD_LOCATOR64: i64 = 20;

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
                        // The modification, access and creation times.
                        let _: u64 = read_into!(err, mz_stream_read_uint64(file_extra_stream));
                        if err == MZ_OK {
                            let _: u64 = read_into!(err, mz_stream_read_uint64(file_extra_stream));
                        }
                        if err == MZ_OK {
                            let _: u64 = read_into!(err, mz_stream_read_uint64(file_extra_stream));
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
                // Read UNIX1 extra field: the access and modification times, the user and
                // group ids.
                if err == MZ_OK {
                    let _: u32 = read_into!(err, mz_stream_read_uint32(file_extra_stream));
                }
                if err == MZ_OK {
                    let _: u32 = read_into!(err, mz_stream_read_uint32(file_extra_stream));
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

    err
}

/// An archive opened for reading.
///
/// Port of `mz_zip` (mz_zip.c:84-118 @ 4.0.10), with the fields reading reaches.
#[derive(Debug)]
pub(crate) struct Zip {
    pub(crate) stream: SplitStream,
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
    /// The stream the central directory is read from (`cd_stream`): the archive's.
    fn cd(&mut self) -> &mut dyn Stream {
        &mut self.stream
    }

    /// Opens the archive on `stream` for reading, with recovery of a central directory it
    /// can't read when `recover`.
    ///
    /// Port of `mz_zip_create`, `mz_zip_set_recover` and `mz_zip_open` (mz_zip.c:1408-1413,
    /// 1589-1595, 1424-1505 @ 4.0.10) for reading.
    pub(crate) fn open(stream: SplitStream, recover: bool) -> Result<Zip, i32> {
        let mut zip = Zip {
            stream,
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

    /// Rebuilds the central directory from the local headers.
    ///
    /// `mz_zip_recover_cd` (mz_zip.c:1236-1406 @ 4.0.10) isn't ported yet: it fails, as it
    /// does where it finds no entry.
    fn recover_cd(&mut self) -> i32 {
        MZ_FORMAT_ERROR
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

        self.stream.set_disk_number(-1);

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
