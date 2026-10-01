// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/GPUProcessor.h` and `GPUProcessor.cpp` @ v2.5.2, so far the
//! function header and footer that every extraction writes around the ops' code
//! (`WriteShaderHeader`, `WriteShaderFooter`).
//!
//! Still to come, with the op model (WP 1.2c) and the optimizer: `GPUProcessor::Impl`, whose
//! `finalize` optimizes the ops and makes the cache ID, and whose extraction asks each op for
//! its code before writing the header and footer and finalizing the description.
//!
//! The resource key of the `GpuShaderCreator` overload of `extractGpuShaderInfo`
//! (GPUProcessor.cpp:157-206) reaches only the creator's `begin`, which does nothing in the
//! one description there is ([`GpuShaderDesc::begin`]), so it changes no output; Python's
//! overload (lines 151-155) doesn't compute it. It isn't ported (docs/improvements.md, U-6).

use ocio_ops::Result;

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;
use crate::open_color_types::GpuLanguage;

/// Adds the OCIO function's header to the description: its signature, its opening brace, and
/// the pixel variable set to the input. An empty pixel name is an error ("GPU variable name
/// is empty."), but in OSL, which declares the pixel as a `color4` without that check.
///
/// Port of `WriteShaderHeader` (GPUProcessor.cpp:27-54 @ v2.5.2).
pub fn write_shader_header(shader_creator: &mut GpuShaderDesc) -> Result<()> {
    let fcn_name = shader_creator.function_name().to_vec();

    let ss = GpuShaderText::new(shader_creator.language());

    ss.new_line();
    ss.new_line()
        .put("// Declaration of the OCIO shader function");
    ss.new_line();

    if shader_creator.language() == GpuLanguage::Osl1 {
        ss.new_line()
            .put("color4 ")
            .put(&fcn_name)
            .put("(color4 inPixel)");
        ss.new_line().put("{");
        ss.indent();
        ss.new_line()
            .put("color4 ")
            .put(shader_creator.pixel_name())
            .put(" = inPixel;");
    } else {
        ss.new_line()
            .put(ss.float4_keyword())
            .put(" ")
            .put(&fcn_name)
            .put("(")
            .put(ss.float4_keyword())
            .put(" inPixel)");
        ss.new_line().put("{");
        ss.indent();
        let decl = ss.float4_decl(shader_creator.pixel_name())?;
        ss.new_line().put(decl).put(" = inPixel;");
    }

    shader_creator.add_to_function_header_shader_code(ss.string());
    Ok(())
}

/// Adds the OCIO function's footer to the description: it returns the pixel, and closes.
///
/// Port of `WriteShaderFooter` (GPUProcessor.cpp:57-68 @ v2.5.2).
pub fn write_shader_footer(shader_creator: &mut GpuShaderDesc) {
    let ss = GpuShaderText::new(shader_creator.language());

    ss.new_line();
    ss.indent();
    ss.new_line()
        .put("return ")
        .put(shader_creator.pixel_name())
        .put(";");
    ss.dedent();
    ss.new_line().put("}");

    shader_creator.add_to_function_footer_shader_code(ss.string());
}
