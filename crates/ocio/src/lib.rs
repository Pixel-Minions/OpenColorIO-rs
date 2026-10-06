// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OpenColorIO, ported to Rust. This release line matches OpenColorIO 2.5.2 exactly
//! (PLAN.md §2): results, text output, errors and accepted configs.
#![forbid(unsafe_code)]

pub mod caching;
pub mod color_space;
pub mod color_space_set;
pub mod config;
pub mod config_io_proxy;
pub mod context;
pub mod context_variable_utils;
pub(crate) mod display;
pub mod look;
pub(crate) mod look_parse;
pub mod named_transform;
pub mod path_utils;
pub mod processor;
pub(crate) mod tokens_manager;
pub mod transform;
pub mod transforms;
pub mod view_transform;
pub mod yaml_cpp;

#[cfg(test)]
mod test_env;

pub use color_space::ColorSpace;
pub use color_space_set::ColorSpaceSet;
pub use config::{Config, CurrentContext};
pub use context::Context;
pub use display::OCIO_VIEW_USE_DISPLAY_NAME;
pub use look::Look;
pub use named_transform::NamedTransform;
pub use ocio_ops::exception::{Exception, ExceptionKind, Result};
pub use ocio_ops::format_metadata::FormatMetadataImpl as FormatMetadata;
pub use ocio_ops::open_color_types::{
    Allocation, BitDepth, CdlStyle, ColorSpaceDirection, ColorSpaceVisibility, EnvironmentMode,
    FixedFunctionStyle, Lut1DHueAdjust, NamedTransformVisibility, NegativeStyle, OptimizationFlags,
    ReferenceSpaceType, SearchReferenceSpaceType, TransformDirection, ViewTransformDirection,
    ViewType,
};
pub use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
pub use ocio_ops::platform::{
    get_env_variable, is_env_variable_present, set_env_variable, unset_env_variable,
};
pub use processor::{Processor, ProcessorCacheFlags, ProcessorMetadata};
pub use transform::{Transform, TransformType};
pub use transforms::allocation_transform::AllocationTransform;
pub use transforms::builtin_transform::BuiltinTransform;
pub use transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;
pub use transforms::cdl_transform::CdlTransform;
pub use transforms::color_space_transform::ColorSpaceTransform;
pub use transforms::display_view_transform::DisplayViewTransform;
pub use transforms::exponent_transform::ExponentTransform;
pub use transforms::exponent_with_linear_transform::ExponentWithLinearTransform;
pub use transforms::file_transform::FileTransform;
pub use transforms::fixed_function_transform::FixedFunctionTransform;
pub use transforms::group_transform::GroupTransform;
pub use transforms::log_affine_transform::LogAffineTransform;
pub use transforms::log_camera_transform::LogCameraTransform;
pub use transforms::log_transform::LogTransform;
pub use transforms::look_transform::LookTransform;
pub use transforms::lut1d_transform::Lut1DTransform;
pub use transforms::matrix_transform::MatrixTransform;
pub use transforms::range_transform::{RangeStyle, RangeTransform};
pub use view_transform::ViewTransform;

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
