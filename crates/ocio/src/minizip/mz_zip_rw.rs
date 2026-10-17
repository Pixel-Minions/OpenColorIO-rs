// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's archive reader and writer (`mz_zip_rw.c`), as OpenColorIO uses them: an
//! archive file opened with recovery on (the reader's default), its entries one after the
//! other, and an entry's contents saved to a buffer; an archive written into memory from a
//! buffer and from files.

use ocio_ops::utils::string_utils::c_str;

use super::mz_os::{
    mz_dir_make, mz_os_file_exists, mz_os_get_file_attribs, mz_os_get_file_date,
    mz_os_get_file_size, mz_os_is_dir, mz_os_is_dir_separator, mz_os_is_symlink,
    mz_os_make_symlink, mz_os_set_file_attribs, mz_os_set_file_date, mz_path_combine,
    mz_path_remove_filename, mz_path_resolve, system_path,
};
use super::mz_strm::{
    MZ_BUF_ERROR, MZ_END_OF_LIST, MZ_EXIST_ERROR, MZ_OK, MZ_OPEN_ERROR, MZ_PARAM_ERROR,
    MZ_READ_ERROR, MZ_STREAM_ERROR, MZ_WRITE_ERROR, MemStream, Origin, SplitStream,
    mz_stream_read_uint64, mz_stream_seek,
};
use super::mz_zip::{
    CdStream, FileInfo, MZ_HOST_SYSTEM_MSDOS, MZ_HOST_SYSTEM_UNIX, MZ_HOST_SYSTEM_WINDOWS_NTFS,
    MZ_ZIP_EXTENSION_CDCD, Zip, ZipWrite, mz_host_system, mz_zip_attrib_convert,
    mz_zip_attrib_is_dir, mz_zip_attrib_is_symlink, mz_zip_extrafield_find,
};

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

/// `MZ_VERSION_MADEBY` (mz_os.h:20-40 @ 4.0.10): the host system (Windows NTFS on Windows,
/// UNIX elsewhere) and zip version 4.5 (no LZMA, AES or bzip2 in OpenColorIO's minizip-ng).
pub(crate) const MZ_VERSION_MADEBY: u16 = if cfg!(windows) {
    ((MZ_HOST_SYSTEM_WINDOWS_NTFS as u16) << 8) | 45
} else {
    ((MZ_HOST_SYSTEM_UNIX as u16) << 8) | 45
};

/// `MZ_ZIP_FLAG_UTF8` (mz.h:84 @ 4.0.10).
pub(crate) const MZ_ZIP_FLAG_UTF8: u16 = 1 << 11;

/// `MZ_COMPRESS_METHOD_DEFLATE` and `MZ_COMPRESS_LEVEL_BEST` (mz.h:64, 74 @ 4.0.10).
pub(crate) const MZ_COMPRESS_METHOD_DEFLATE: u16 = 8;
pub(crate) const MZ_COMPRESS_LEVEL_BEST: i16 = 9;

/// An archive writer, into memory.
///
/// Port of `mz_zip_writer` (mz_zip_rw.c:1052-1072 @ 4.0.10), with the fields OpenColorIO's use
/// reaches: no password, AES, links, zipped central directory or callbacks.
#[derive(Debug)]
pub(crate) struct Writer {
    zip: Option<ZipWrite>,
    file_info: FileInfo,
    compress_method: u16,
    compress_level: i16,
}

impl Writer {
    /// Port of `mz_zip_writer_create` (mz_zip_rw.c:1888-1907 @ 4.0.10).
    pub(crate) fn create() -> Writer {
        Writer {
            zip: None,
            file_info: FileInfo::default(),
            compress_method: MZ_COMPRESS_METHOD_DEFLATE,
            compress_level: MZ_COMPRESS_LEVEL_BEST,
        }
    }

    /// Port of `mz_zip_writer_set_compress_method` (mz_zip_rw.c:1822-1825 @ 4.0.10).
    pub(crate) fn set_compress_method(&mut self, compress_method: u16) {
        self.compress_method = compress_method;
    }

    /// Port of `mz_zip_writer_set_compress_level` (mz_zip_rw.c:1827-1830 @ 4.0.10).
    pub(crate) fn set_compress_level(&mut self, compress_level: i16) {
        self.compress_level = compress_level;
    }

    /// Opens the writer on a new memory stream (`append` 0).
    ///
    /// Port of `mz_zip_writer_open` and `mz_zip_writer_open_int` (mz_zip_rw.c:1158-1168,
    /// 1140-1156 @ 4.0.10).
    pub(crate) fn open(&mut self) -> i32 {
        self.zip = Some(ZipWrite::open());
        MZ_OK
    }

    fn zip(&mut self) -> &mut ZipWrite {
        self.zip.as_mut().expect("the writer is open")
    }

    /// Port of `mz_zip_writer_entry_open` (mz_zip_rw.c:1319-1363 @ 4.0.10), without a password
    /// and without hashes (`MZ_ZIP_NO_CRYPTO`).
    pub(crate) fn entry_open(&mut self, file_info: &FileInfo) -> i32 {
        // Copy file info to access data upon close
        self.file_info = file_info.clone();
        let level = self.compress_level;
        let file_info = self.file_info.clone();
        self.zip().entry_write_open(&file_info, level)
    }

    /// Port of `mz_zip_writer_entry_write` (mz_zip_rw.c:1437-1446 @ 4.0.10).
    pub(crate) fn entry_write(&mut self, buf: &[u8]) -> i32 {
        self.zip().entry_write(buf)
    }

    /// Port of `mz_zip_writer_entry_close` (mz_zip_rw.c:1365-1435 @ 4.0.10), without hashes.
    pub(crate) fn entry_close(&mut self) -> i32 {
        self.zip().entry_close()
    }

    /// Adds the data `file` gives, read 64 KiB at a time, to the open entry.
    ///
    /// Port of `mz_zip_writer_add` and `mz_zip_writer_add_process` (mz_zip_rw.c:1478-1517,
    /// 1449-1476 @ 4.0.10), without progress callbacks, reading with `mz_stream_os_read`
    /// (`fread`'s full reads).
    fn add(&mut self, file: &mut std::fs::File) -> i32 {
        use std::io::Read;
        let mut buffer = vec![0u8; u16::MAX as usize];
        loop {
            let mut read = 0;
            while read < buffer.len() {
                match file.read(&mut buffer[read..]) {
                    Ok(0) => break,
                    Ok(n) => read += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return MZ_READ_ERROR,
                }
            }
            if read == 0 {
                return MZ_OK;
            }
            let written = self.entry_write(&buffer[..read]);
            if written != read as i32 {
                return MZ_WRITE_ERROR;
            }
        }
    }

    /// Adds an entry `file_info` with the data `file` gives (none for a directory).
    ///
    /// Port of `mz_zip_writer_add_info` (mz_zip_rw.c:1519-1544 @ 4.0.10).
    fn add_info(&mut self, file: Option<&mut std::fs::File>, file_info: &FileInfo) -> i32 {
        // Add to zip
        let err = self.entry_open(file_info);
        if err != MZ_OK {
            return err;
        }

        if let Some(file) = file
            && !mz_zip_attrib_is_dir(self.file_info.external_fa, self.file_info.version_madeby)
        {
            let err = self.add(file);
            if err != MZ_OK {
                return err;
            }
        }

        self.entry_close()
    }

    /// Adds the file `path` as `filename_in_zip`, with its size, dates and attributes.
    ///
    /// Port of `mz_zip_writer_add_file` (mz_zip_rw.c:1570-1647 @ 4.0.10), without links
    /// (OpenColorIO doesn't store them) or a zipped central directory.
    pub(crate) fn add_file(&mut self, path: &[u8], filename_in_zip: &[u8]) -> i32 {
        let path = c_str(path);
        let mut filename = c_str(filename_in_zip);

        // The path name saved, should not include a leading slash.
        // If it did, windows/xp and dynazip couldn't read the zip file.
        while let [b'\\' | b'/', rest @ ..] = filename {
            filename = rest;
        }

        // Get information about the file on disk so we can store it in zip
        let mut file_info = FileInfo {
            version_madeby: MZ_VERSION_MADEBY,
            compression_method: self.compress_method,
            filename: filename.to_vec(),
            uncompressed_size: mz_os_get_file_size(path),
            flag: MZ_ZIP_FLAG_UTF8,
            ..FileInfo::default()
        };

        if let Some((modified, accessed, creation)) = mz_os_get_file_date(path) {
            file_info.modified_date = modified;
            file_info.accessed_date = accessed;
            file_info.creation_date = creation;
        }
        let src_attrib = mz_os_get_file_attribs(path);

        let src_sys = mz_host_system(file_info.version_madeby);

        if src_sys != MZ_HOST_SYSTEM_MSDOS && src_sys != MZ_HOST_SYSTEM_WINDOWS_NTFS {
            // High bytes are OS specific attributes, low byte is always DOS attributes
            if let Some(target_attrib) =
                mz_zip_attrib_convert(src_sys, src_attrib, MZ_HOST_SYSTEM_MSDOS)
            {
                file_info.external_fa = target_attrib;
            }
            file_info.external_fa |= src_attrib << 16;
        } else {
            file_info.external_fa = src_attrib;
        }

        let mut file = None;
        if !mz_os_is_dir(path) {
            match std::fs::File::open(system_path(path)) {
                Ok(f) => file = Some(f),
                Err(_) => return MZ_OPEN_ERROR,
            }
        }

        self.add_info(file.as_mut(), &file_info)
    }

    /// Adds the file `path`, named in the archive by its path from `root_path` (which `path`
    /// starts with), unless it is a symbolic link (which isn't added).
    ///
    /// Port of `mz_zip_writer_add_path` (mz_zip_rw.c:1649-1729 @ 4.0.10) with `include_path` 0,
    /// for a `path` that isn't a directory, as OpenColorIO calls it (`addSupportedFiles` lists
    /// the directories itself): its branch for a directory isn't ported.
    ///
    /// A `path` with a `*` that doesn't exist (on Linux, a dangling link whose name has one)
    /// takes upstream's wildcard branch, which lists the file's directory and adds each entry
    /// that matches the name: the link itself among them, which takes the branch again, without
    /// end, until the stack overflows (docs/improvements.md, U-80). The port returns
    /// `MZ_EXIST_ERROR` instead.
    pub(crate) fn add_path_of_file(&mut self, path: &[u8], root_path: &[u8]) -> i32 {
        let path = c_str(path);
        if path.contains(&b'*') && mz_os_file_exists(path) != MZ_OK {
            return MZ_EXIST_ERROR;
        }
        // Should the file be stored with any path info at all?
        let filenameinzip = path.get(c_str(root_path).len()..).unwrap_or(&[]);

        // Neither links are stored nor followed.
        if mz_os_is_symlink(path) {
            return MZ_OK;
        }

        if !filenameinzip.is_empty() {
            return self.add_file(path, filenameinzip);
        }
        MZ_OK
    }

    /// Closes the writer and its archive, and gives what its memory stream holds (OpenColorIO
    /// ignores the result of closing).
    ///
    /// Port of `mz_zip_writer_close` (mz_zip_rw.c:1283-1315 @ 4.0.10), without a comment or a
    /// zipped central directory.
    pub(crate) fn close(&mut self) -> Vec<u8> {
        let Some(mut zip) = self.zip.take() else {
            return Vec::new();
        };
        zip.version_madeby = MZ_VERSION_MADEBY;
        let _ = zip.close();
        zip.stream.bytes_at(0).to_vec()
    }
}

impl Reader {
    /// Saves the current entry's data to the file `path`, making its directory; a directory
    /// entry makes a directory, a link entry a link. The file gets the entry's dates and
    /// attributes.
    ///
    /// Port of `mz_zip_reader_entry_save_file` (mz_zip_rw.c:647-775 @ 4.0.10), without an entry
    /// or overwrite callback.
    pub(crate) fn entry_save_file(&mut self, path: &[u8]) -> i32 {
        let Ok(file_info) = self.entry_get_info().cloned() else {
            return MZ_PARAM_ERROR;
        };
        let mut pathwfs = c_str(path).to_vec();
        let mut directory = pathwfs.clone();
        mz_path_remove_filename(&mut directory);

        let is_symlink = mz_zip_attrib_is_symlink(file_info.external_fa, file_info.version_madeby);

        // If it is a directory entry then create a directory instead of writing file
        if self.zip_entry_is_dir() && !is_symlink {
            return mz_dir_make(&directory);
        }

        // If symbolic link then properly construct destination path and link path
        if is_symlink && pathwfs.last().is_some_and(|&c| mz_os_is_dir_separator(c)) {
            // mz_path_remove_slash
            while pathwfs.last().is_some_and(|&c| mz_os_is_dir_separator(c)) {
                pathwfs.pop();
            }
            mz_path_remove_filename(&mut directory);
        }

        // Create the output directory if it doesn't already exist
        if !mz_os_is_dir(&directory) {
            let err = mz_dir_make(&directory);
            if err != MZ_OK {
                return err;
            }
        }

        // If it is a symbolic link then create symbolic link instead of writing file
        if is_symlink {
            let mut err = MZ_OK;
            if !file_info.linkname.is_empty() {
                // Create symbolic link from UNIX1 extrafield
                err = mz_os_make_symlink(&pathwfs, &file_info.linkname);
            } else if file_info.uncompressed_size < i64::from(u16::MAX) {
                // Create symbolic link from zip entry contents
                let mut linkname = Vec::new();
                err = self.entry_save_to(&mut linkname);
                if err == MZ_OK {
                    err = mz_os_make_symlink(&pathwfs, c_str(&linkname));
                }
            }
            // Don't check return value because we aren't validating symbolic link target
            // (upstream's comment: the error of making the link is returned all the same)
            return err;
        }

        // Create the file on disk so we can save to it
        let file = match std::fs::File::create(system_path(&pathwfs)) {
            Ok(file) => file,
            Err(_) => return MZ_OPEN_ERROR,
        };
        let mut file = std::io::BufWriter::new(file);
        let mut err = self.entry_save_to(&mut file);
        if std::io::Write::flush(&mut file).is_err() && err == MZ_OK {
            err = MZ_WRITE_ERROR;
        }
        drop(file);

        if err == MZ_OK {
            // Set the time of the file that has been created
            mz_os_set_file_date(
                &pathwfs,
                file_info.modified_date,
                file_info.accessed_date,
                file_info.creation_date,
            );
        }

        if err == MZ_OK {
            // Set file attributes for the correct system
            if let Some(target_attrib) = mz_zip_attrib_convert(
                mz_host_system(file_info.version_madeby),
                file_info.external_fa,
                mz_host_system(MZ_VERSION_MADEBY),
            ) {
                mz_os_set_file_attribs(&pathwfs, target_attrib);
            }
        }

        err
    }

    /// Whether the current entry is a directory: its attributes say so, or its name ends with
    /// a separator.
    ///
    /// Port of `mz_zip_entry_is_dir` (mz_zip.c:2287-2302 @ 4.0.10).
    fn zip_entry_is_dir(&self) -> bool {
        let Ok(info) = self.entry_get_info() else {
            return false;
        };
        mz_zip_attrib_is_dir(info.external_fa, info.version_madeby)
            || info
                .filename
                .last()
                .is_some_and(|&c| mz_os_is_dir_separator(c))
    }

    /// Saves the current entry's data to `out`, as far as it is read.
    ///
    /// Port of `mz_zip_reader_entry_save` and `mz_zip_reader_entry_save_process`
    /// (mz_zip_rw.c:601-645, 561-599 @ 4.0.10) with a stream that takes all it is given.
    fn entry_save_to(&mut self, out: &mut dyn std::io::Write) -> i32 {
        if !self.has_file_info {
            return MZ_PARAM_ERROR;
        }
        let mut buffer = vec![0u8; READER_BUFFER_SIZE];
        loop {
            if !self.zip.entry_is_open() {
                let err = self.entry_open();
                if err != MZ_OK {
                    return err;
                }
            }
            let read = self.zip.entry_read(&mut buffer);
            if read == 0 {
                return self.entry_close();
            }
            if read < 0 {
                return read;
            }
            if out.write_all(&buffer[..read as usize]).is_err() {
                return MZ_WRITE_ERROR;
            }
        }
    }

    /// Saves every entry under `destination_dir`, at its name resolved (no `.`, `..` or
    /// repeated separators): `MZ_END_OF_LIST` for an archive without entries.
    ///
    /// Port of `mz_zip_reader_save_all` (mz_zip_rw.c:818-907 @ 4.0.10), with the reader's
    /// encoding 0 (names as they are).
    pub(crate) fn save_all(&mut self, destination_dir: &[u8]) -> i32 {
        let mut err = self.goto_first_entry();

        if err == MZ_END_OF_LIST {
            return err;
        }

        let destination_dir_len = c_str(destination_dir).len() + 1;
        // The buffer mz_path_resolve writes to, which realloc keeps from entry to entry.
        let mut resolved_name = Vec::new();

        while err == MZ_OK {
            let Ok(info) = self.entry_get_info() else {
                break;
            };
            // Assume 4 bytes per character needed + 1 for terminating null
            let utf8_name_size = usize::from(info.filename_size) * 4 + 1;
            // +1 is for the "/" separator
            let resolved_name_size = utf8_name_size + destination_dir_len;

            let name = info.filename.clone();
            let utf8_name = &name[..name.len().min(utf8_name_size - 1)];

            err = mz_path_resolve(utf8_name, &mut resolved_name, resolved_name_size as i32);
            if err != MZ_OK {
                break;
            }
            let resolved = c_str(&resolved_name).to_vec();

            let mut path = Vec::new();
            mz_path_combine(&mut path, destination_dir, resolved_name_size);
            mz_path_combine(&mut path, &resolved, resolved_name_size);

            // Save file to disk
            err = self.entry_save_file(&path);

            if err == MZ_OK {
                err = self.goto_next_entry();
            }
        }

        if err == MZ_END_OF_LIST {
            err = MZ_OK;
        }
        err
    }
}
