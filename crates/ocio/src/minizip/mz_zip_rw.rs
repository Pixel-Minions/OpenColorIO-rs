// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's archive reader (`mz_zip_rw.c`), as OpenColorIO uses it: an archive file
//! opened with recovery on (the reader's default), its entries one after the other, and an
//! entry's contents saved to a buffer.

use ocio_ops::utils::string_utils::c_str;

use super::mz_strm::{
    MZ_BUF_ERROR, MZ_EXIST_ERROR, MZ_OK, MZ_PARAM_ERROR, MZ_STREAM_ERROR, MZ_WRITE_ERROR,
    MemStream, Origin, SplitStream, mz_stream_read_uint64, mz_stream_seek,
};
use super::mz_zip::{CdStream, FileInfo, MZ_ZIP_EXTENSION_CDCD, Zip, mz_zip_extrafield_find};

/// The name of a zipped central directory's entry (`MZ_ZIP_CD_FILENAME`, mz_zip_rw.c:28 @
/// 4.0.10).
const MZ_ZIP_CD_FILENAME: &[u8] = b"__cdcd__";

/// The size of the reader's buffer (`uint8_t buffer[UINT16_MAX]`, mz_zip_rw.c:55 @ 4.0.10).
const READER_BUFFER_SIZE: usize = u16::MAX as usize;

/// An archive reader.
///
/// Port of `mz_zip_reader` (mz_zip_rw.c:32-62 @ 4.0.10), with the fields OpenColorIO's use
/// reaches: no pattern, password, callbacks or signature.
#[derive(Debug)]
pub(crate) struct Reader {
    zip: Zip,
    /// Whether `file_info` points at the archive's current entry.
    has_file_info: bool,
}

impl Reader {
    /// Opens the archive file `path` (a C string), or fails as minizip-ng does.
    ///
    /// Port of `mz_zip_reader_create` and `mz_zip_reader_open_file` (mz_zip_rw.c:1018-1025,
    /// 99-129 @ 4.0.10), through `mz_zip_reader_open` (mz_zip_rw.c:75-97), which also
    /// unzips a zipped central directory (`mz_zip_reader_unzip_cd`), whatever that gives.
    pub(crate) fn open_file(path: &[u8]) -> Result<Reader, i32> {
        let stream = SplitStream::open(c_str(path))?;
        // The reader turns recovery on (reader->recover = 1).
        let zip = Zip::open(stream, true)?;
        let mut reader = Reader {
            zip,
            has_file_info: false,
        };
        let _ = reader.unzip_cd();
        Ok(reader)
    }

    /// Decompresses the central directory when the first entry holds it, zipped.
    ///
    /// Port of `mz_zip_reader_unzip_cd` (mz_zip_rw.c:243-304 @ 4.0.10).
    fn unzip_cd(&mut self) -> i32 {
        let err = self.goto_first_entry();
        if err != MZ_OK {
            return err;
        }
        let cd_info = match self.entry_get_info() {
            Ok(info) => info.clone(),
            Err(err) => return err,
        };

        if c_str(&cd_info.filename) != MZ_ZIP_CD_FILENAME {
            return self.goto_first_entry();
        }

        let err = self.entry_open();
        if err != MZ_OK {
            return err;
        }

        let mut file_extra_stream = MemStream::over(&cd_info.extrafield);
        let number_entry =
            match mz_zip_extrafield_find(&mut file_extra_stream, MZ_ZIP_EXTENSION_CDCD, i32::MAX)
                .and_then(|_| mz_stream_read_uint64(&mut file_extra_stream))
            {
                Ok(number_entry) => number_entry,
                Err(err) => return err,
            };

        let zip = &mut self.zip;
        if !zip.cd_mem_open {
            zip.cd_mem_stream = MemStream::create();
            zip.cd_mem_open = true;
        }

        let mut err = mz_stream_seek(&mut zip.cd_mem_stream, 0, Origin::Set);
        if err == MZ_OK {
            // mz_stream_copy_stream(cd_mem_stream, NULL, handle, mz_zip_reader_entry_read,
            //                       (int32_t)cd_info->uncompressed_size)
            let mut len = cd_info.uncompressed_size as i32;
            let mut buf = [0u8; 16384];
            while len > 0 {
                let bytes_to_copy = len.min(buf.len() as i32) as usize;
                let read = zip.entry_read(&mut buf[..bytes_to_copy]);
                if read <= 0 {
                    err = MZ_STREAM_ERROR;
                    break;
                }
                let written = zip.cd_mem_stream.write(&buf[..read as usize]);
                if written != read {
                    err = MZ_STREAM_ERROR;
                    break;
                }
                len -= read;
            }
        }

        if err == MZ_OK {
            self.zip.set_cd_stream(0, CdStream::Memory);
            self.zip.number_entry = number_entry;

            err = self.goto_first_entry();
        }

        err
    }

    /// Port of `mz_zip_reader_goto_first_entry` (mz_zip_rw.c:316-336 @ 4.0.10), without a
    /// pattern.
    pub(crate) fn goto_first_entry(&mut self) -> i32 {
        if self.zip.entry_is_open() {
            self.entry_close();
        }

        let mut err = self.zip.goto_first_entry();

        self.has_file_info = false;
        if err == MZ_OK {
            err = self.zip_entry_get_info();
        }
        err
    }

    /// Port of `mz_zip_reader_goto_next_entry` (mz_zip_rw.c:338-358 @ 4.0.10), without a
    /// pattern.
    pub(crate) fn goto_next_entry(&mut self) -> i32 {
        if self.zip.entry_is_open() {
            self.entry_close();
        }

        let mut err = self.zip.goto_next_entry();

        self.has_file_info = false;
        if err == MZ_OK {
            err = self.zip_entry_get_info();
        }
        err
    }

    /// Port of `mz_zip_entry_get_info` (mz_zip.c:2315-2328 @ 4.0.10) for reading.
    fn zip_entry_get_info(&mut self) -> i32 {
        if !self.zip.entry_scanned() {
            return MZ_PARAM_ERROR;
        }
        self.has_file_info = true;
        MZ_OK
    }

    /// The current entry's header.
    ///
    /// Port of `mz_zip_reader_entry_get_info` (mz_zip_rw.c:543-552 @ 4.0.10).
    pub(crate) fn entry_get_info(&self) -> Result<&FileInfo, i32> {
        if !self.has_file_info {
            return Err(MZ_EXIST_ERROR);
        }
        Ok(&self.zip.file_info)
    }

    /// Port of `mz_zip_reader_entry_open` (mz_zip_rw.c:378-428 @ 4.0.10), without a password
    /// and without hashes (`MZ_ZIP_NO_CRYPTO`, as OpenColorIO builds minizip-ng).
    fn entry_open(&mut self) -> i32 {
        if !self.has_file_info {
            return MZ_PARAM_ERROR;
        }

        // If the entry isn't open for reading, open it
        if self.zip.entry_is_open() {
            return MZ_OK;
        }

        self.zip.entry_read_open()
    }

    /// Port of `mz_zip_reader_entry_close` (mz_zip_rw.c:430-458 @ 4.0.10), without hashes.
    fn entry_close(&mut self) -> i32 {
        self.zip.entry_close()
    }

    /// Saves the current entry's data to `buf`, as far as it is read, through a memory
    /// stream that takes at most `buf.len()` bytes.
    ///
    /// Port of `mz_zip_reader_entry_save` and `mz_zip_reader_entry_save_process`
    /// (mz_zip_rw.c:601-645, 561-599 @ 4.0.10), without progress callbacks, and
    /// `mz_stream_mem_write` on a buffer of fixed size (mz_strm_mem.c:113-144).
    fn entry_save(&mut self, buf: &mut [u8]) -> i32 {
        if !self.has_file_info {
            return MZ_PARAM_ERROR;
        }

        let mut position = 0usize;
        let mut buffer = vec![0u8; READER_BUFFER_SIZE];
        loop {
            // mz_zip_reader_entry_save_process
            if !self.zip.entry_is_open() {
                let err = self.entry_open();
                if err != MZ_OK {
                    return err;
                }
            }

            // Unzip entry in zip file
            let read = self.zip.entry_read(&mut buffer);

            if read == 0 {
                // If we are done close the entry
                let err = self.entry_close();
                if err != MZ_OK {
                    return err;
                }
                // MZ_END_OF_STREAM
                return MZ_OK;
            }

            if read < 0 {
                return read;
            }

            // Write the data to the specified stream
            let written = (read as usize).min(buf.len() - position);
            buf[position..position + written].copy_from_slice(&buffer[..written]);
            position += written;
            if written != read as usize {
                return MZ_WRITE_ERROR;
            }
        }
    }

    /// Saves the current entry's data to `buf`, which must be as long as the entry.
    ///
    /// Port of `mz_zip_reader_entry_save_buffer` (mz_zip_rw.c:777-802 @ 4.0.10).
    pub(crate) fn entry_save_buffer(&mut self, buf: &mut [u8]) -> i32 {
        let Ok(info) = self.entry_get_info() else {
            return MZ_PARAM_ERROR;
        };
        if info.uncompressed_size > i64::from(i32::MAX) {
            return MZ_PARAM_ERROR;
        }
        if buf.len() as i64 != info.uncompressed_size {
            return MZ_BUF_ERROR;
        }
        self.entry_save(buf)
    }

    /// The size of the buffer the current entry needs, or `MZ_PARAM_ERROR` for an entry past
    /// `INT32_MAX` bytes.
    ///
    /// Port of `mz_zip_reader_entry_save_buffer_length` (mz_zip_rw.c:804-814 @ 4.0.10).
    pub(crate) fn entry_save_buffer_length(&self) -> i32 {
        match self.entry_get_info() {
            Ok(info) if info.uncompressed_size <= i64::from(i32::MAX) => {
                info.uncompressed_size as i32
            }
            _ => MZ_PARAM_ERROR,
        }
    }
}
