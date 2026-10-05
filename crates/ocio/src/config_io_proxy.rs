// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The I/O proxy: what a config reads its LUT files and its own text through, instead of the
//! file system, when the application gives one.

use ocio_ops::exception::Result;

/// The source of a config's files: an application's archive, database or memory. A context
/// holds one (`Context::set_config_io_proxy`); file hashes and LUT reads then go through it.
///
/// Port of `ConfigIOProxy` (include/OpenColorIO/OpenColorIO.h:4095-4138 @ v2.5.2). Upstream's
/// virtual functions may throw; here they return [`Result`].
pub trait ConfigIoProxy: Send + Sync {
    /// The contents of the LUT file at `filepath`, the fully resolved path the file system
    /// would have been given.
    #[doc(alias = "getLutData")]
    fn lut_data(&self, filepath: &[u8]) -> Result<Vec<u8>>;

    /// The config's YAML text.
    #[doc(alias = "getConfigData")]
    fn config_data(&self) -> Result<Vec<u8>>;

    /// A fast unique ID of the LUT file at `filepath` (the key of OCIO's file cache), or empty
    /// when the proxy can't supply the file.
    #[doc(alias = "getFastLutFileHash")]
    fn fast_lut_file_hash(&self, filepath: &[u8]) -> Result<Vec<u8>>;
}
