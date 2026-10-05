// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Path helpers: a port of `src/OpenColorIO/PathUtils.cpp` @ v2.5.2, so far the file hashes
//! and their cache, `FileExists`, the working directory (`GetCwd`) and `AbsPath`.
//! `ParseColorSpaceFromString` comes with the file rules (WP 3.9).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

use ocio_ops::exception::Result;
use ocio_ops::platform::create_file_content_hash;
use ocio_ops::utils::pystring::os_path;

use crate::context::Context;

/// A function computing a file's hash from its path: empty when the file doesn't exist.
///
/// Port of `ComputeHashFunction` (include/OpenColorIO/OpenColorIO.h @ v2.5.2).
pub type ComputeHashFunction = fn(&[u8]) -> Vec<u8>;

/// The hash function in use: `CreateFileContentHash` unless set otherwise.
///
/// Port of `g_hashFunction` (src/OpenColorIO/PathUtils.cpp:33 @ v2.5.2).
static HASH_FUNCTION: RwLock<ComputeHashFunction> = RwLock::new(create_file_content_hash);

/// One file's hash, computed once.
#[derive(Default)]
struct FileHashResult {
    hash: Vec<u8>,
    ready: bool,
}

/// Per path, its hash. A path's own lock keeps a slow hash from blocking other paths.
///
/// Port of `g_fastFileHashCache` (PathUtils.cpp:50-51 @ v2.5.2).
static FAST_FILE_HASH_CACHE: Mutex<BTreeMap<Vec<u8>, Arc<Mutex<FileHashResult>>>> =
    Mutex::new(BTreeMap::new());

/// Replaces the function that computes file hashes.
///
/// Port of `SetComputeHashFunction` (PathUtils.cpp:54-57 @ v2.5.2).
#[doc(alias = "SetComputeHashFunction")]
pub fn set_compute_hash_function(hash_function: ComputeHashFunction) {
    *HASH_FUNCTION.write().unwrap_or_else(|e| e.into_inner()) = hash_function;
}

/// Restores `CreateFileContentHash` as the function that computes file hashes.
///
/// Port of `ResetComputeHashFunction` (PathUtils.cpp:59-62 @ v2.5.2).
#[doc(alias = "ResetComputeHashFunction")]
pub fn reset_compute_hash_function() {
    set_compute_hash_function(create_file_content_hash);
}

fn hash_function() -> ComputeHashFunction {
    *HASH_FUNCTION.read().unwrap_or_else(|e| e.into_inner())
}

/// A file's hash, computed once per path (OCIO never notices a file that changed): from the
/// context's I/O proxy when it has one (and, for an absolute path the proxy gives no hash
/// for, from the hash function), else from the hash function. Empty when the file doesn't
/// exist.
///
/// Port of `GetFastFileHash` (PathUtils.cpp:64-115 @ v2.5.2).
pub fn get_fast_file_hash(filename: &[u8], context: &Context) -> Result<Vec<u8>> {
    let result = {
        let mut cache = FAST_FILE_HASH_CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache.entry(filename.to_vec()).or_default().clone()
    };
    let mut result = result.lock().unwrap_or_else(|e| e.into_inner());
    if !result.ready {
        result.ready = true;
        let h = if let Some(proxy) = context.config_io_proxy() {
            // Case for when ConfigIOProxy is used (callbacks mechanism).
            let h = proxy.fast_lut_file_hash(filename)?;
            // For absolute paths, if the proxy does not provide a hash, try the file system.
            if h.is_empty() && os_path::isabs(filename) {
                hash_function()(filename)
            } else {
                h
            }
        } else {
            // Default case
            hash_function()(filename)
        };
        result.hash = h;
    }
    Ok(result.hash.clone())
}

/// Whether the file has a hash: whether it exists, as far as OCIO knows.
///
/// Port of `FileExists` (PathUtils.cpp:117-121 @ v2.5.2).
pub fn file_exists(filename: &[u8], context: &Context) -> Result<bool> {
    Ok(!get_fast_file_hash(filename, context)?.is_empty())
}

/// Forgets every file hash.
///
/// Port of `ClearPathCaches` (PathUtils.cpp:123-127 @ v2.5.2).
pub fn clear_path_caches() {
    FAST_FILE_HASH_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

/// The length of the buffer the Windows wheel reads the working directory into:
/// `MAXPATHLEN`, which PathUtils.cpp defines as 4096 on Windows (PathUtils.cpp:22).
#[cfg(windows)]
const MAXPATHLEN: usize = 4096;

/// The process's working directory.
///
/// Port of `GetCwd` (PathUtils.cpp:131-150 @ v2.5.2). Linux: `getcwd`, the buffer grown until
/// the path fits; a failure of another kind leaves the buffer zeroed, so "". Windows:
/// `_getcwd` into a `MAXPATHLEN` buffer, in the ANSI code page, where the port gives the path
/// as UTF-8 (docs/improvements.md, I-116); when `_getcwd` fails (a path that doesn't fit, or a
/// working directory the system can't give), upstream returns the uninitialized buffer, and the
/// port an error (U-45).
fn get_cwd() -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        let cwd = std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned().into_bytes())
            .filter(|path| path.len() < MAXPATHLEN);
        cwd.ok_or_else(|| {
            ocio_ops::exception::Exception::new(
                "The current working directory could not be read (_getcwd failed).",
            )
        })
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::env::current_dir()
            .map(|path| path.into_os_string().into_vec())
            .unwrap_or_default())
    }
}

/// `path` made absolute against the working directory when it isn't, then normalized.
///
/// Port of `AbsPath` (PathUtils.cpp:153-158 @ v2.5.2).
pub fn abs_path(path: &[u8]) -> Result<Vec<u8>> {
    let mut p = path.to_vec();
    if !os_path::isabs(&p) {
        p = os_path::join(&get_cwd()?, &p);
    }
    Ok(os_path::normpath(&p))
}

/// Held by the unit tests that change the hash function and by those that look for files, which
/// run in parallel in one process: the hash function is global.
#[cfg(test)]
pub(crate) fn hash_function_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[path = "path_utils_tests.rs"]
mod tests;
