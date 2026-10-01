// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Exponent op's GPU writer: part of a port of `src/OpenColorIO/ops/exponent/ExponentOp.cpp`
//! @ v2.5.2, whose op writes its code itself (`ExponentOp::extractGpuShaderInfo`).

use ocio_ops::Result;
use ocio_ops::ops::exponent::ExponentOpData;

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

/// Adds the code of an Exponent op to `shader_creator`'s function body, as a block that
/// computes `res = pow(max(res, 0), exponents)` from the pixel and writes it back, the
/// exponents written as `double`s (the CPU renderer rounds them to `float`).
///
/// Port of `ExponentOp::extractGpuShaderInfo` (ExponentOp.cpp:258-297 @ v2.5.2).
pub fn get_exponent_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    exp: &ExponentOpData,
) -> Result<()> {
    let ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    ss.new_line().put("");
    ss.new_line().put("// Add an Exponent processing");
    ss.new_line().put("");

    ss.new_line().put("{");
    ss.indent();

    // outColor = pow(max(outColor, 0.), exp);

    let pxl = shader_creator.pixel_name().to_vec();
    let channel = |suffix: &str| [pxl.as_slice(), suffix.as_bytes()].concat();

    let res_decl = ss.float4_decl("res")?;
    ss.new_line()
        .put(res_decl)
        .put(" = ")
        .put(ss.float4_const(
            channel(".rgb.r"),
            channel(".rgb.g"),
            channel(".rgb.b"),
            channel(".a"),
        ))
        .put(";");

    let e = exp.exp4;
    ss.new_line()
        .put("res = pow( ")
        .put("max( res, ")
        .put(ss.float4_splat_f32(0.0))
        .put(" )")
        .put(", ")
        .put(ss.float4_const_f64(e[0], e[1], e[2], e[3]))
        .put(" );");

    ss.new_line()
        .put(&pxl)
        .put(".rgb = ")
        .put(ss.float3_const("res.x", "res.y", "res.z"))
        .put(";");
    ss.new_line().put(&pxl).put(".a = res.w;");

    ss.dedent();
    ss.new_line().put("}");

    shader_creator.add_to_function_shader_code(ss.string());
    Ok(())
}
