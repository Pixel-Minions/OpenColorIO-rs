// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `.ocioz` config archives: a config and its LUT files in a zip archive, read through
//! minizip-ng's reader ([`crate::minizip`]), as OpenColorIO reads them.

use std::collections::BTreeMap;
use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::platform::create_input_file_stream;
use ocio_ops::utils::pystring::os_path;
use ocio_ops::utils::string_utils::c_str;

use crate::config::{Config, OCIO_CONFIG_DEFAULT_FILE_EXT, OCIO_CONFIG_DEFAULT_NAME};
use crate::config_io_proxy::ConfigIoProxy;
use crate::minizip::mz_os::mz_path_compare_wc;
use crate::minizip::mz_strm::MZ_OK;
use crate::minizip::mz_zip_rw::Reader;

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

#[cfg(test)]
#[path = "ocioz_archive_tests.rs"]
mod tests;
