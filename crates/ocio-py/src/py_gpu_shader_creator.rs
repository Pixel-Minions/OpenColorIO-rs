// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGpuShaderCreator` (src/bindings/python/PyGpuShaderCreator.cpp @ v2.5.2). Bound so
//! far (chunk 6.2b): the class's two enums, exported into the class. Its methods and
//! `DynamicPropertyIterator` come with the card that binds the GPU classes; they go between the
//! enums and the export of their values, as upstream defines them.

use pyo3::prelude::*;

use crate::enums::{EnumDef, Scalar, define_enum, export_values};

/// `GpuShaderCreator::TextureType` (include/OpenColorIO/OpenColorIO.h:3485-3489 @ v2.5.2),
/// with the values PyGpuShaderCreator.cpp:128-131 gives it.
const TEXTURE_TYPE: EnumDef = EnumDef {
    name: "TextureType",
    scalar: Scalar::Int,
    arithmetic: false,
    value_docs: false,
    entries: &[("TEXTURE_RED_CHANNEL", 0), ("TEXTURE_RGB_CHANNEL", 1)],
};

/// `GpuShaderCreator::TextureDimensions : uint8_t` (include/OpenColorIO/OpenColorIO.h:3494-3497
/// @ v2.5.2), with the values PyGpuShaderCreator.cpp:133-136 gives it.
const TEXTURE_DIMENSIONS: EnumDef = EnumDef {
    name: "TextureDimensions",
    scalar: Scalar::UChar,
    arithmetic: false,
    value_docs: false,
    entries: &[("TEXTURE_1D", 1), ("TEXTURE_2D", 2)],
};

/// Port of `bindPyGpuShaderCreator` (src/bindings/python/PyGpuShaderCreator.cpp:22-136 @
/// v2.5.2), its enums only: declared (29-36), then their values exported into the class
/// (127-136).
pub(crate) fn bind_py_gpu_shader_creator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = m.getattr("GpuShaderCreator")?;
    let texture_type = define_enum(&cls, Some("GpuShaderCreator"), &TEXTURE_TYPE)?;
    let texture_dimensions = define_enum(&cls, Some("GpuShaderCreator"), &TEXTURE_DIMENSIONS)?;
    export_values(&cls, &texture_type)?;
    export_values(&cls, &texture_dimensions)?;
    Ok(())
}
