// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's `GpuShaderDesc` for the oracle's description settings.

use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::Result;
use ocio_testkit::gpu::{GpuLanguage as OracleLanguage, ShaderSettings};

/// The port's language for the oracle's name of it; both lists are upstream's enumerator order.
fn language(language: OracleLanguage) -> GpuLanguage {
    let index = OracleLanguage::ALL
        .iter()
        .position(|l| *l == language)
        .expect("every language is in the list");
    GpuLanguage::ALL[index]
}

/// Apply the original setter arguments exactly once. Canonical getter names
/// must not be passed through underscore normalization a second time.
pub(crate) fn description(settings: &ShaderSettings) -> Result<GpuShaderDesc> {
    let mut desc = GpuShaderDesc::new(language(
        settings.language.unwrap_or(OracleLanguage::Glsl12),
    ));
    if let Some(value) = &settings.function_name {
        desc.set_function_name(value);
    }
    if let Some(value) = &settings.pixel_name {
        desc.set_pixel_name(value);
    }
    if let Some(value) = &settings.resource_prefix {
        desc.set_resource_prefix(value);
    }
    if let Some(value) = &settings.uid {
        desc.set_unique_id(value);
    }
    if let Some((index, start)) = settings.descriptor_set {
        desc.set_descriptor_set_index(index, start)?;
    }
    if let Some(value) = settings.texture_max_width {
        desc.set_texture_max_width(value);
    }
    if let Some(value) = settings.allow_texture_1d {
        desc.set_allow_texture_1d(value);
    }
    Ok(desc)
}
