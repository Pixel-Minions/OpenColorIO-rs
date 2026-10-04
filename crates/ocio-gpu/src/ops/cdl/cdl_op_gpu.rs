// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/cdl/CDLOpGPU.h` and `CDLOpGPU.cpp` @ v2.5.2: the CDL op's GPU
//! writer.

use ocio_ops::Result;
use ocio_ops::ops::cdl::CdlOpData;
use ocio_ops::ops::cdl::cdl_op_cpu::RenderParams;

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

/// Adds the code of a CDL op to `shader_creator`'s function body: a comment naming the style,
/// and a block on the pixel's RGB (alpha passes through) with the `float` parameters of the
/// CPU renderers ([`RenderParams`]): slope, offset, power (clamped to [0, 1] first, or
/// mirrored through `step` and `lerp` for the styles that don't clamp) and saturation,
/// in that order forward and in the reverse order for the reverse styles.
///
/// Port of `GetCDLGPUShaderProgram` (CDLOpGPU.cpp:14-122 @ v2.5.2).
pub fn get_cdl_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    cdl: &CdlOpData,
) -> Result<()> {
    let params = RenderParams::new(cdl);

    let slope = params.get_slope();
    let offset = params.get_offset();
    let power = params.get_power();
    let saturation = params.get_saturation();

    let ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    ss.new_line().put("");
    ss.new_line()
        .put("// Add CDL '")
        .put(CdlOpData::get_style_name(cdl.get_style()))
        .put("' processing");
    ss.new_line().put("");

    ss.new_line().put("{");
    ss.indent();

    let pix = shader_creator.pixel_name();
    let pixrgb = [pix, b".rgb"].concat();

    // Since alpha is not affected, only need to use the RGB components
    ss.declare_float3_f32("lumaWeights", 0.2126, 0.7152, 0.0722)?;
    ss.declare_float3_f32("slope", slope[0], slope[1], slope[2])?;
    ss.declare_float3_f32("offset", offset[0], offset[1], offset[2])?;
    ss.declare_float3_f32("power", power[0], power[1], power[2])?;

    ss.declare_var_f32("saturation", saturation)?;

    if !params.is_reverse() {
        // Forward style

        // Slope
        ss.new_line()
            .put(&pixrgb)
            .put(" = ")
            .put(&pixrgb)
            .put(" * slope;");

        // Offset
        ss.new_line()
            .put(&pixrgb)
            .put(" = ")
            .put(&pixrgb)
            .put(" + offset;");

        // Power
        write_power(&ss, &pixrgb, params.is_no_clamp())?;

        // Saturation
        write_saturation(&ss, &pixrgb);

        // Post-saturation clamp
        if !params.is_no_clamp() {
            ss.new_line()
                .put(&pixrgb)
                .put(" = clamp(")
                .put(&pixrgb)
                .put(", 0.0, 1.0);");
        }
    } else {
        // Reverse style

        // Pre-saturation clamp
        if !params.is_no_clamp() {
            ss.new_line()
                .put(&pixrgb)
                .put("  = clamp(")
                .put(&pixrgb)
                .put(", 0.0, 1.0);");
        }

        // Saturation
        write_saturation(&ss, &pixrgb);

        // Power
        write_power(&ss, &pixrgb, params.is_no_clamp())?;

        // Offset
        ss.new_line()
            .put(&pixrgb)
            .put(" = ")
            .put(&pixrgb)
            .put(" + offset;");

        // Slope
        ss.new_line()
            .put(&pixrgb)
            .put(" = ")
            .put(&pixrgb)
            .put(" * slope;");

        // Post-slope clamp
        if !params.is_no_clamp() {
            ss.new_line()
                .put(&pixrgb)
                .put(" = clamp(")
                .put(&pixrgb)
                .put(", 0.0, 1.0);");
        }
    }

    ss.dedent();
    ss.new_line().put("}");

    shader_creator.add_to_function_shader_code(ss.string());
    Ok(())
}

/// The power step: `pow(clamp(rgb, 0, 1), power)`, or, for the styles that don't clamp,
/// `pow(abs(rgb), power)` where `rgb` is at least 0 and `rgb` elsewhere.
///
/// Port of the power steps of `GetCDLGPUShaderProgram` (CDLOpGPU.cpp:55-66, 92-103 @ v2.5.2),
/// the same in both directions.
fn write_power(ss: &GpuShaderText, pixrgb: &[u8], no_clamp: bool) -> Result<()> {
    if !no_clamp {
        ss.new_line()
            .put(pixrgb)
            .put(" = clamp(")
            .put(pixrgb)
            .put(", 0.0, 1.0);");
        ss.new_line()
            .put(pixrgb)
            .put(" = pow(")
            .put(pixrgb)
            .put(", power);");
    } else {
        ss.new_line()
            .put(ss.float3_decl("posPix")?)
            .put(" = step(0.0, ")
            .put(pixrgb)
            .put(");");
        ss.new_line()
            .put(ss.float3_decl("pixPower")?)
            .put(" = pow(abs(")
            .put(pixrgb)
            .put("), power);");
        ss.new_line()
            .put(pixrgb)
            .put(" = ")
            .put(ss.lerp(pixrgb, "pixPower", "posPix"))
            .put(";");
    }
    Ok(())
}

/// The saturation step, the same in both directions.
///
/// Port of the saturation steps of `GetCDLGPUShaderProgram` (CDLOpGPU.cpp:68-70, 88-90
/// @ v2.5.2).
fn write_saturation(ss: &GpuShaderText, pixrgb: &[u8]) {
    ss.new_line()
        .put("float luma = dot(")
        .put(pixrgb)
        .put(", lumaWeights);");
    ss.new_line()
        .put(pixrgb)
        .put(" = luma + saturation * (")
        .put(pixrgb)
        .put(" - luma);");
}
