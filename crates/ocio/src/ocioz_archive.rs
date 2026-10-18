// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `.ocioz` config archives: a config and its LUT files in a zip archive, read and written
//! through minizip-ng's reader and writer ([`crate::minizip`]), as OpenColorIO reads and
//! writes them.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::platform::create_input_file_stream;
use ocio_ops::utils::pystring::{self, os_path};
use ocio_ops::utils::string_utils::c_str;

use crate::config::{Config, OCIO_CONFIG_DEFAULT_FILE_EXT, OCIO_CONFIG_DEFAULT_NAME};
use crate::config_io_proxy::ConfigIoProxy;
use crate::minizip::mz_os::{mz_os_is_dir, mz_os_list_dir, mz_path_compare_wc};
use crate::minizip::mz_strm::{MZ_END_OF_LIST, MZ_OK};
use crate::minizip::mz_zip::FileInfo;
use crate::minizip::mz_zip_rw::{
    MZ_COMPRESS_LEVEL_BEST, MZ_COMPRESS_METHOD_DEFLATE, MZ_VERSION_MADEBY, MZ_ZIP_FLAG_UTF8,
    Reader, Writer,
};
use crate::transforms::file_format::FormatRegistry;

/// "256 MB is well above any realistic individual OCIO config or LUT file size."
///
/// Port of `readEntryBuffer`'s `MAX_ENTRY_SIZE` (src/OpenColorIO/OCIOZArchive.cpp:394 @
/// v2.5.2).
const MAX_ENTRY_SIZE: usize = 256 * 1024 * 1024;

/// The contents of the reader's current entry, which must have between 1 byte and 256 MB:
/// as far as minizip-ng reads it, the rest zeros (its result is ignored).
///
/// Port of `readEntryBuffer` (OCIOZArchive.cpp:390-403 @ v2.5.2).
fn read_entry_buffer(reader: &mut Reader) -> Result<Vec<u8>> {
    let buf_size = reader.entry_save_buffer_length();
    // Reject negative values (minizip error codes) and implausibly large entries.
    // 256 MB is well above any realistic individual OCIO config or LUT file size.
    if buf_size <= 0 || buf_size as usize > MAX_ENTRY_SIZE {
        return Err(Exception::new(
            "OCIOZ archive entry size is invalid or exceeds maximum allowed size.",
        ));
    }
    let mut buffer = vec![0u8; buf_size as usize];
    let _ = reader.entry_save_buffer(&mut buffer);
    Ok(buffer)
}

/// The contents of the reader's current entry, `name`, when `name`, as a wildcard, matches
/// `filepath`, ignoring case and the kind of slashes; empty otherwise.
///
/// Port of `getFileBufferByPath` (OCIOZArchive.cpp:417-427 @ v2.5.2).
fn get_file_buffer_by_path(reader: &mut Reader, name: &[u8], filepath: &[u8]) -> Result<Vec<u8>> {
    // Verify that the file information and the current file matches while ignoring the slashes
    // differences in platforms.
    if mz_path_compare_wc(filepath, name, true) {
        return read_entry_buffer(reader);
    }
    Ok(Vec::new())
}

/// The contents of the first entry, in the archive's order, whose name matches `filepath`
/// (see [`get_file_buffer_by_path`]) in the archive at `archive_path`; empty when none does.
///
/// Port of `getFileStringFromArchiveFile` with `getFileBufferByPath`, which
/// `getFileBufferFromArchive` calls (OCIOZArchive.cpp:465-513, 521-524 @ v2.5.2). Its other
/// callback, `getFileBufferByExtension`, serves `getFileBufferFromArchiveByExtension`, which
/// nothing in OpenColorIO 2.5.2 calls.
fn get_file_buffer_from_archive(filepath: &[u8], archive_path: &[u8]) -> Result<Vec<u8>> {
    // Open the OCIOZ archive from the filesystem.
    let Ok(mut reader) = Reader::open_file(archive_path) else {
        let mut os = b"Could not open ".to_vec();
        os.extend_from_slice(c_str(archive_path));
        os.extend_from_slice(b" in order to get the file: ");
        os.extend_from_slice(filepath);
        return Err(Exception::new(os));
    };

    // Seek to the first entry in the OCIOZ archive.
    if reader.goto_first_entry() == MZ_OK {
        loop {
            // Get the current entry information.
            if let Ok(info) = reader.entry_get_info() {
                let name = info.filename.clone();
                let buffer = get_file_buffer_by_path(&mut reader, &name, filepath)?;
                if !buffer.is_empty() {
                    return Ok(buffer);
                }
            }
            if reader.goto_next_entry() != MZ_OK {
                break;
            }
        }
    }
    Ok(Vec::new())
}

/// The archive's entries by name, each with its name followed by its CRC-32 in decimal; the
/// first entry of a name, where several share it.
///
/// Port of `getEntriesMappingFromArchiveFile` (OCIOZArchive.cpp:531-577 @ v2.5.2).
fn get_entries_mapping_from_archive_file(
    archive_path: &[u8],
    map: &mut BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<()> {
    // Open the zip from file.
    let Ok(mut reader) = Reader::open_file(archive_path) else {
        let mut os = b"Could not open ".to_vec();
        os.extend_from_slice(c_str(archive_path));
        os.extend_from_slice(b" in order to get the entries.");
        return Err(Exception::new(os));
    };

    // Seek to the first entry in the OCIOZ archive.
    if reader.goto_first_entry() == MZ_OK {
        loop {
            // Get the current entry information.
            if let Ok(info) = reader.entry_get_info() {
                // file_info->filename is the complete path of the file from the root of the
                // archive.
                let mut hash = info.filename.clone();
                hash.extend_from_slice(info.crc.to_string().as_bytes());
                map.entry(info.filename.clone()).or_insert(hash);
            }
            if reader.goto_next_entry() != MZ_OK {
                break;
            }
        }
    }
    Ok(())
}

/// The I/O proxy of a config read from an `.ocioz` archive: the config, its LUT files and
/// their hashes come from the archive, which each request opens again.
///
/// Port of `CIOPOciozArchive` (src/OpenColorIO/OCIOZArchive.h:77-107 @ v2.5.2).
#[derive(Debug, Default)]
pub(crate) struct CiopOciozArchive {
    archive_abs_path: Vec<u8>,
    entries: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl CiopOciozArchive {
    /// Sets the archive's path (as given: a relative path is the working directory's).
    ///
    /// Port of `CIOPOciozArchive::setArchiveAbsPath` (OCIOZArchive.cpp:641-644 @ v2.5.2).
    pub(crate) fn set_archive_abs_path(&mut self, abs_path: &[u8]) {
        self.archive_abs_path = c_str(abs_path).to_vec();
    }

    /// Lists the archive's entries.
    ///
    /// Port of `CIOPOciozArchive::buildEntries` (OCIOZArchive.cpp:646-661 @ v2.5.2).
    pub(crate) fn build_entries(&mut self) -> Result<()> {
        if create_input_file_stream(&self.archive_abs_path).is_err() {
            let mut os = b"Error could not read OCIOZ archive: ".to_vec();
            os.extend_from_slice(&self.archive_abs_path);
            return Err(Exception::new(os));
        }

        get_entries_mapping_from_archive_file(&self.archive_abs_path, &mut self.entries)
    }
}

impl ConfigIoProxy for CiopOciozArchive {
    /// Port of `CIOPOciozArchive::getLutData` (OCIOZArchive.cpp:586-601 @ v2.5.2).
    fn lut_data(&self, filepath: &[u8]) -> Result<Vec<u8>> {
        get_file_buffer_from_archive(&os_path::normpath(c_str(filepath)), &self.archive_abs_path)
    }

    /// Port of `CIOPOciozArchive::getConfigData` (OCIOZArchive.cpp:603-620 @ v2.5.2).
    fn config_data(&self) -> Result<Vec<u8>> {
        let config_filename = [OCIO_CONFIG_DEFAULT_NAME, OCIO_CONFIG_DEFAULT_FILE_EXT].concat();
        get_file_buffer_from_archive(config_filename.as_bytes(), &self.archive_abs_path)
    }

    /// The hash of the last entry, in the order of their names, whose name matches `filepath`
    /// as a wildcard; empty when none does.
    ///
    /// Port of `CIOPOciozArchive::getFastLutFileHash` (OCIOZArchive.cpp:622-639 @ v2.5.2).
    fn fast_lut_file_hash(&self, filepath: &[u8]) -> Result<Vec<u8>> {
        let mut hash = Vec::new();
        // Check into the map to check if the file exists in the archive.
        // The key is the full path of the file inside the archive and the value is the hash.
        // Normalize filepath and find it in the std::map.
        let fpath = os_path::normpath(c_str(filepath));
        for (name, value) in &self.entries {
            // Verify that the key and the specfied filepath matches while ignoring the slashes
            // differences in platforms.
            if mz_path_compare_wc(name, &fpath, true) {
                hash = value.clone();
            }
        }
        Ok(hash)
    }
}

/// The config of the archive `filename`, read through its I/O proxy.
///
/// Port of `Config::CreateFromFile`'s archive branch (src/OpenColorIO/Config.cpp:1187-1200 @
/// v2.5.2).
pub(crate) fn create_from_archive(filename: &[u8]) -> Result<Arc<Config>> {
    let mut ciop = CiopOciozArchive::default();
    // Store archive absolute path.
    ciop.set_archive_abs_path(filename);
    // Build the entries map.
    ciop.build_entries()?;
    Config::create_from_config_io_proxy(Arc::new(ciop))
}

/// Adds the files under `path` whose extension a LUT format reads, directory by directory in
/// the order the system lists them, each named by its path from `config_working_directory`.
///
/// Port of `addSupportedFiles` (src/OpenColorIO/OCIOZArchive.cpp:149-202 @ v2.5.2).
fn add_supported_files(
    archiver: &mut Writer,
    path: &[u8],
    config_working_directory: &[u8],
) -> Result<()> {
    let Some(names) = mz_os_list_dir(path) else {
        return Ok(());
    };
    for name in names {
        let name = match name {
            Ok(name) => name,
            Err(name) => {
                // A name minizip-ng reads into memory nothing wrote (docs/improvements.md, U-79).
                let mut os = b"Could not archive ".to_vec();
                os.extend_from_slice(&os_path::join(path, &name));
                os.extend_from_slice(b": its name is longer than minizip-ng reads.");
                return Err(Exception::new(os));
            }
        };
        // Join the current path and the directory/file name to get the absolute path.
        let abs_path = os_path::join(path, &name);
        // Check if the absolute path is a directory.
        if mz_os_is_dir(&abs_path) {
            // Since mz_os_read_dir is listing the whole directory, "." and ".." must be
            // ignored (the listing leaves them out).
            add_supported_files(archiver, &abs_path, config_working_directory)?;
        } else {
            // Absolute path is a file.
            let (_root, ext) = os_path::splitext(&name);
            // Strip leading dot character in order to get the extension name only.
            let ext = pystring::lstrip(&ext, b".");

            // Check if the extension is supported. Using logic from LoadFileUncached().
            let possible_formats = FormatRegistry::instance().file_formats_for_extension(&ext);
            if !possible_formats.is_empty() {
                // The extension is supported. Add the current file to the OCIOZ archive.
                if archiver.add_path_of_file(&abs_path, config_working_directory) != MZ_OK {
                    let mut os = b"Could not write LUT file ".to_vec();
                    os.extend_from_slice(&abs_path);
                    os.extend_from_slice(b" to in-memory archive.");
                    return Err(Exception::new(os));
                }
            }
        }
    }
    Ok(())
}

/// Writes the archive of `config`: its serialized text as `config.ocio`, then the LUT files
/// under `config_working_directory`.
///
/// Its entries carry the time they are written (`time(NULL)` for the config) and the files'
/// times, and the files come in the order the system lists them, so archives aren't
/// reproducible byte for byte; the deflate streams are flate2's (deviation D-6).
///
/// Port of `archiveConfig` (src/OpenColorIO/OCIOZArchive.cpp:205-333 @ v2.5.2).
pub(crate) fn archive_config(
    ostream: &mut dyn Write,
    config: &Config,
    config_working_directory: &[u8],
) -> Result<()> {
    if !config.is_archivable() {
        return Err(Exception::new("Config is not archivable."));
    }

    // Retrieve and store the config as string.
    let config_str = config.serialize()?;

    // Create the writer handle, deflating at the best level.
    let mut archiver = Writer::create();
    archiver.set_compress_method(MZ_COMPRESS_METHOD_DEFLATE);
    archiver.set_compress_level(MZ_COMPRESS_LEVEL_BEST);

    // Open the in-memory zip.
    if archiver.open() == MZ_OK {
        // Use a hardcoded name for the config's filename inside the archive.
        let config_fullname = [OCIO_CONFIG_DEFAULT_NAME, OCIO_CONFIG_DEFAULT_FILE_EXT].concat();

        // Get config string size.
        let config_size = config_str.len() as i32;

        // Initialize the config file information structure.
        let file_info = FileInfo {
            filename: config_fullname.into_bytes(),
            modified_date: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
            version_madeby: MZ_VERSION_MADEBY,
            compression_method: MZ_COMPRESS_METHOD_DEFLATE,
            flag: MZ_ZIP_FLAG_UTF8,
            uncompressed_size: i64::from(config_size),
            ..FileInfo::default()
        };

        // Opens an entry in the in-memory zip file for writing.
        if archiver.entry_open(&file_info) == MZ_OK {
            // Add config to the in-memory zip using buffer.
            let written = archiver.entry_write(&config_str);
            if written < MZ_OK {
                return Err(Exception::new(
                    "Could not write config to in-memory archive.",
                ));
            }
            // Close the entry.
            archiver.entry_close();
        } else {
            return Err(Exception::new("Could not prepare an entry for writing."));
        }

        // Add all supported files to in-memory zip from any directories under working
        // directory. (recursive)
        add_supported_files(
            &mut archiver,
            config_working_directory,
            config_working_directory,
        )?;
    }

    // Close in-memory zip, and write it to the stream.
    let buffer = archiver.close();
    // A write that fails sets the stream's badbit upstream, which nothing checks.
    let _ = ostream.write_all(&buffer);
    Ok(())
}

/// Extracts the `.ocioz` archive `archive_path` into the directory `destination`: each entry
/// at its name (without `.` and `..`, which don't go above `destination`), its directories
/// made, with the dates and attributes the archive gives (deviation D-6 for those of a file
/// on Windows).
///
/// The errors: "Could not open <archive_path> for reading.", "No files in archive." and
/// "Could not extract: <archive_path>".
///
/// Port of `ExtractOCIOZArchive` (src/OpenColorIO/OCIOZArchive.cpp:335-388 @ v2.5.2).
#[doc(alias = "ExtractOCIOZArchive")]
pub fn extract_ocioz_archive(
    archive_path: impl AsRef<[u8]>,
    destination: impl AsRef<[u8]>,
) -> Result<()> {
    let archive_path = c_str(archive_path.as_ref());
    // Normalize the path for the platform.
    let output_destination = os_path::normpath(c_str(destination.as_ref()));

    // Open the OCIOZ archive.
    let Ok(mut extracter) = Reader::open_file(archive_path) else {
        let mut os = b"Could not open ".to_vec();
        os.extend_from_slice(archive_path);
        os.extend_from_slice(b" for reading.");
        return Err(Exception::new(os));
    };

    // Extract all entries to outputDestination directory.
    let err = extracter.save_all(&output_destination);
    if err == MZ_END_OF_LIST {
        // The archive has no files.
        return Err(Exception::new("No files in archive."));
    } else if err != MZ_OK {
        let mut os = b"Could not extract: ".to_vec();
        os.extend_from_slice(archive_path);
        return Err(Exception::new(os));
    }

    // Closing a reader that saved every entry doesn't fail ("Could not close <archive_path>
    // after reading."): its entries are closed.
    Ok(())
}

#[cfg(test)]
#[path = "ocioz_archive_tests.rs"]
mod tests;
