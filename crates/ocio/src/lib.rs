// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OpenColorIO, ported to Rust. This release line matches OpenColorIO 2.5.2 exactly
//! (PLAN.md §2): results, text output, errors and accepted configs.
//!
//! The package is `opencolorio-rs`; in code the library is `ocio`. Its calls are OCIO's C++ API
//! in Rust's spelling: `Config::CreateFromBuiltinConfig` is [`Config::create_from_builtin_config`],
//! `getProcessor` is [`Config::processor_between`] and its siblings, and
//! `getDefaultCPUProcessor` is [`Processor::default_cpu_processor`]. Names and strings are bytes,
//! as they are in OCIO, so they take `impl AsRef<[u8]>`.
//!
//! # Example
//!
//! Load a built-in config, convert a pixel on the CPU, and generate the matching GPU shader:
//!
//! ```
//! # fn main() -> ocio::Result<()> {
//! let config = ocio::Config::create_from_builtin_config("cg-config-v4.0.0_aces-v2.0_ocio-v2.5")?;
//! config.validate()?;
//!
//! // From a color space to a display and view: OCIO's getProcessor(src, display, view, dir).
//! let mut view = ocio::DisplayViewTransform::new();
//! view.set_src("ACEScg");
//! view.set_display("sRGB - Display");
//! view.set_view("ACES 2.0 - SDR 100 nits (Rec.709)");
//! let processor = config.processor(&view.into())?;
//!
//! // On the CPU, the same bits as the official library on the same machine.
//! let cpu = processor.default_cpu_processor()?;
//! let mut pixel = [0.18_f32, 0.18, 0.18, 1.0];
//! cpu.apply_rgba(&mut pixel)?;
//!
//! // On the GPU, the same shader text. The shader classes come from `opencolorio-rs-gpu`,
//! // whose library is `ocio_gpu`.
//! let mut desc = ocio_gpu::GpuShaderDesc::new(ocio_gpu::GpuLanguage::GlslVk4_6);
//! desc.set_function_name("ocio_display");
//! desc.set_resource_prefix("ocio_");
//! desc.set_descriptor_set_index(1, 1)?;
//! processor.default_gpu_processor()?.extract_gpu_shader_info(&mut desc)?;
//! let text: &[u8] = desc.shader_text();
//! # let _ = text;
//! # Ok(())
//! # }
//! ```
//!
//! Whole images go through [`CpuProcessor::apply`](ocio_ops::cpu_processor::CpuProcessor::apply)
//! with an image description, as `apply(ImageDesc &)` does in C++, or
//! [`apply_threaded`](ocio_ops::cpu_processor::CpuProcessor::apply_threaded), which renders rows
//! on several threads with the same results.
//!
//! # Versions
//!
//! [`version`] is the OpenColorIO version this release matches, `"2.5.2"`, as
//! `OCIO::GetVersion()` reports it. [`PORT_VERSION`] is this crate's own version. A later
//! OpenColorIO minor version is a new major version of this crate.
//!
//! # Exceptions
//!
//! Where the port behaves differently from OpenColorIO, the repository lists it:
//! `docs/deviations.md` for intended differences, `waivers.toml` for the approved exceptions to
//! byte-exact output, and `docs/improvements.md` for upstream bugs the port reproduces.
#![deny(unsafe_code)]

pub mod apphelpers;
pub mod baker;
pub mod baking_utils;
pub mod builtinconfigs;
pub mod caching;
pub mod color_space;
pub mod color_space_set;
pub mod config;
pub mod config_io_proxy;
pub(crate) mod config_utils;
pub mod context;
pub mod context_variable_utils;
pub(crate) mod custom_keys;
pub(crate) mod display;
pub mod file_rules;
pub mod fileformats;
pub mod look;
pub(crate) mod look_parse;
pub mod lut_limits;
pub(crate) mod minizip;
pub mod named_transform;
pub(crate) mod ocio_yaml;
pub mod ocioz_archive;
pub mod parse_utils;
pub mod path_utils;
pub mod processor;
pub mod system_monitor;
#[cfg(windows)]
mod system_monitor_windows;
#[cfg(windows)]
mod system_monitor_windows_ffi;
pub(crate) mod tokens_manager;
pub mod transform;
pub mod transforms;
pub mod view_transform;
pub mod viewing_rules;
pub mod yaml_cpp;

#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod foreign_exception_tests;
#[cfg(test)]
mod test_env;

pub use apphelpers::color_space_helpers::{self, ColorSpaceMenuHelper, ColorSpaceMenuParameters};
pub use apphelpers::display_view_helpers;
pub use apphelpers::legacy_viewing_pipeline::LegacyViewingPipeline;
pub use apphelpers::mergeconfigs::merge_configs_helpers::{
    ConfigMerger, ConfigMergingParameters, MergeStrategies, config_merging_helpers,
};
pub use apphelpers::mixing_helpers::{MixingColorSpaceManager, MixingSlider};
pub use baker::Baker;
pub use builtinconfigs::builtin_config_registry::{
    BuiltinConfigRegistry, OCIO_BUILTIN_URI_PREFIX, resolve_config_path,
};
pub use caching::clear_all_caches;
pub use color_space::ColorSpace;
pub use color_space_set::ColorSpaceSet;
pub use config::{
    Config, ConfigRules, CurrentContext, OCIO_CONFIG_ENVVAR, get_current_config, set_current_config,
};
pub use context::Context;
pub use display::OCIO_VIEW_USE_DISPLAY_NAME;
pub use file_rules::FileRules;
pub use look::Look;
pub use named_transform::NamedTransform;
pub use ocio_ops::exception::{Exception, ExceptionKind, Result};
pub use ocio_ops::format_metadata::FormatMetadataImpl as FormatMetadata;
pub use ocio_ops::open_color_types::{
    Allocation, BSplineType, BitDepth, CdlStyle, ColorSpaceDirection, ColorSpaceVisibility,
    DynamicPropertyType, EnvironmentMode, ExposureContrastStyle, FixedFunctionStyle, GradingStyle,
    Lut1DHueAdjust, NamedTransformVisibility, NegativeStyle, OptimizationFlags, ReferenceSpaceType,
    RgbCurveType, SearchReferenceSpaceType, TransformDirection, ViewTransformDirection, ViewType,
};
pub use ocio_ops::open_color_types::{HsyTransformStyle, HueCurveType};
pub use ocio_ops::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
pub use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
pub use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
pub use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
pub use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone};
pub use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
pub use ocio_ops::platform::{
    get_env_variable, is_env_variable_present, set_env_variable, unset_env_variable,
};
pub use ocioz_archive::extract_ocioz_archive;
pub use processor::{Processor, ProcessorCacheFlags, ProcessorMetadata};
pub use system_monitor::SystemMonitors;
pub use transform::{Transform, TransformType};
pub use transforms::allocation_transform::AllocationTransform;
pub use transforms::builtin_transform::BuiltinTransform;
pub use transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;
pub use transforms::cdl_transform::CdlTransform;
pub use transforms::color_space_transform::ColorSpaceTransform;
pub use transforms::display_view_transform::DisplayViewTransform;
pub use transforms::exponent_transform::ExponentTransform;
pub use transforms::exponent_with_linear_transform::ExponentWithLinearTransform;
pub use transforms::exposure_contrast_transform::ExposureContrastTransform;
pub use transforms::file_transform::FileTransform;
pub use transforms::fixed_function_transform::FixedFunctionTransform;
pub use transforms::grading_hue_curve_transform::GradingHueCurveTransform;
pub use transforms::grading_primary_transform::GradingPrimaryTransform;
pub use transforms::grading_rgb_curve_transform::GradingRgbCurveTransform;
pub use transforms::grading_tone_transform::GradingToneTransform;
pub use transforms::group_transform::GroupTransform;
pub use transforms::log_affine_transform::LogAffineTransform;
pub use transforms::log_camera_transform::LogCameraTransform;
pub use transforms::log_transform::LogTransform;
pub use transforms::look_transform::LookTransform;
pub use transforms::lut1d_transform::Lut1DTransform;
pub use transforms::lut3d_transform::Lut3DTransform;
pub use transforms::matrix_transform::MatrixTransform;
pub use transforms::range_transform::{RangeStyle, RangeTransform};
pub use view_transform::ViewTransform;
pub use viewing_rules::ViewingRules;

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
