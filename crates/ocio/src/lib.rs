// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OpenColorIO, ported to Rust. This release line matches OpenColorIO 2.5.2 exactly
//! (PLAN.md §2): results, text output, errors and accepted configs.
#![forbid(unsafe_code)]

pub mod config;
pub mod context;
pub mod transform;
pub mod transforms;
pub mod yaml_cpp;

pub use config::Config;
pub use context::Context;
pub use ocio_ops::exception::{Exception, ExceptionKind, Result};
pub use ocio_ops::format_metadata::FormatMetadataImpl as FormatMetadata;
pub use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
pub use transform::{Transform, TransformType};
pub use transforms::group_transform::GroupTransform;
pub use transforms::matrix_transform::MatrixTransform;

/// The OpenColorIO version this port matches, as `OCIO::GetVersion()` reports it.
pub const fn version() -> &'static str {
    "2.5.2"
}

/// The OpenColorIO version as a hex number, as `OCIO::GetVersionHex()` reports it.
pub const fn version_hex() -> i32 {
    0x0205_0200
}

/// This crate's own version, without the `+ocio.X.Y.Z` build metadata.
pub const PORT_VERSION: &str = port_version();

const fn port_version() -> &'static str {
    let full = env!("CARGO_PKG_VERSION").as_bytes();
    let mut len = 0;
    while len < full.len() && full[len] != b'+' {
        len += 1;
    }
    match std::str::from_utf8(full.split_at(len).0) {
        Ok(s) => s,
        Err(_) => panic!("CARGO_PKG_VERSION is ASCII"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions() {
        assert_eq!(super::version(), "2.5.2");
        assert!(!super::PORT_VERSION.contains('+'));
        assert!(env!("CARGO_PKG_VERSION").ends_with("+ocio.2.5.2"));
    }
}
