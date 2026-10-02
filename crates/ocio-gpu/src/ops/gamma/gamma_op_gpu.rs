// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/gamma/GammaOpGPU.h` and `GammaOpGPU.cpp` @ v2.5.2: the Gamma
//! op's GPU writer, one block per style.
//!
//! The basic styles declare their exponents as `double`s (the reverse ones as `1. / gamma`,
//! computed in `double`); the moncurve styles declare the `float` coefficients of the CPU
//! renderers ([`compute_params_fwd`], [`compute_params_rev`]).
//!
//! Upstream reads each channel's first parameter (and a moncurve style's second) without a
//! check, past the end of a shorter vector, which only `GammaOpData::validate` refuses; the
//! writer returns [`SHORT_PARAMS`] there instead (`docs/improvements.md` U-24).

use ocio_ops::ops::gamma::gamma_op_data::SHORT_PARAMS;
use ocio_ops::ops::gamma::gamma_op_utils::{
    RendererParams, compute_params_fwd, compute_params_rev,
};
use ocio_ops::ops::gamma::{GammaOpData, GammaStyle};
use ocio_ops::{Exception, Result};

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

/// Each channel's first parameter (red, green, blue, alpha), or, for a reverse style, its
/// inverse `1. / p[0]` in `double`. [`SHORT_PARAMS`] for a channel without one.
///
/// Port of the first lines of the basic styles' writers, `redGamma` to `alphaGamma`
/// (GammaOpGPU.cpp:21-24, 43-46, 66-69, 86-89, 107-110, 135-138 @ v2.5.2).
fn basic_gammas(gamma: &GammaOpData, reverse: bool) -> Result<[f64; 4]> {
    let mut out = [0.0; 4];
    for (o, p) in out.iter_mut().zip(gamma.all_params()) {
        let first = *p.first().ok_or_else(|| Exception::new(SHORT_PARAMS))?;
        *o = if reverse { 1. / first } else { first };
    }
    Ok(out)
}

/// The moncurve coefficients of the four channels (red, green, blue, alpha).
///
/// Port of the first lines of the moncurve styles' writers, `ComputeParamsFwd` or
/// `ComputeParamsRev` for each channel (GammaOpGPU.cpp:164-169, 202-207, 239-244, 281-286
/// @ v2.5.2).
fn moncurve_params(gamma: &GammaOpData, reverse: bool) -> Result<[RendererParams; 4]> {
    let compute = if reverse {
        compute_params_rev
    } else {
        compute_params_fwd
    };
    Ok([
        compute(gamma.red_params())?,
        compute(gamma.green_params())?,
        compute(gamma.blue_params())?,
        compute(gamma.alpha_params())?,
    ])
}

/// The moncurve styles' declarations: `breakPnt`, `slope`, `scale`, `offset` and `gamma`, as
/// `float`s. (Upstream: "Even if all components are the same, on OS X, a vec4 needs to be
/// declared. This code will work in both cases.")
///
/// Port of the moncurve styles' `declareFloat4` calls (GammaOpGPU.cpp:176-180, 214-218,
/// 250-254, 292-296 @ v2.5.2).
fn declare_moncurve_params(ss: &GpuShaderText, p: &[RendererParams; 4]) -> Result<()> {
    let [r, g, b, a] = p;
    ss.declare_float4_f32(
        "breakPnt",
        r.break_pnt,
        g.break_pnt,
        b.break_pnt,
        a.break_pnt,
    )?;
    ss.declare_float4_f32("slope", r.slope, g.slope, b.slope, a.slope)?;
    ss.declare_float4_f32("scale", r.scale, g.scale, b.scale, a.scale)?;
    ss.declare_float4_f32("offset", r.offset, g.offset, b.offset, a.offset)?;
    ss.declare_float4_f32("gamma", r.gamma, g.gamma, b.gamma, a.gamma)
}

/// Every style's last two lines: the pixel takes `res`.
///
/// Port of each writer's last two lines (GammaOpGPU.cpp:35-36, 57-58, 78-79, 98-99,
/// 127-128, 155-156, 193-194, 230-231, 272-273, 312-313 @ v2.5.2).
fn write_back(ss: &GpuShaderText, pxl: &[u8]) {
    ss.new_line()
        .put(pxl)
        .put(".rgb = ")
        .put(ss.float3_const("res.x", "res.y", "res.z"))
        .put(";");
    ss.new_line().put(pxl).put(".a = res.w;");
}

/// Basic style: `pow(max(0, pixel), gamma)`, or `1. / gamma` reversed.
///
/// Port of `AddBasicFwdShader` and `AddBasicRevShader` (GammaOpGPU.cpp:16-59 @ v2.5.2).
fn add_basic_shader(
    shader_creator: &GpuShaderDesc,
    gamma: &GammaOpData,
    ss: &GpuShaderText,
    reverse: bool,
) -> Result<()> {
    let [red_gamma, grn_gamma, blu_gamma, alpha_gamma] = basic_gammas(gamma, reverse)?;

    let pxl = shader_creator.pixel_name();

    ss.declare_float4_f64("gamma", red_gamma, grn_gamma, blu_gamma, alpha_gamma)?;

    ss.new_line()
        .put(ss.float4_decl("res")?)
        .put(" = pow( max( ")
        .put(ss.float4_splat_f32(0.0))
        .put(", ")
        .put(pxl)
        .put(" ), gamma );");

    write_back(ss, pxl);
    Ok(())
}

/// Basic mirror style: `sign(pixel) * pow(abs(pixel), gamma)`, or `1. / gamma` reversed.
/// `sign` writes its own `;`, so the line ends with two.
///
/// Port of `AddBasicMirrorFwdShader` and `AddBasicMirrorRevShader` (GammaOpGPU.cpp:61-100
/// @ v2.5.2).
fn add_basic_mirror_shader(
    shader_creator: &GpuShaderDesc,
    gamma: &GammaOpData,
    ss: &GpuShaderText,
    reverse: bool,
) -> Result<()> {
    let [red_gamma, grn_gamma, blu_gamma, alpha_gamma] = basic_gammas(gamma, reverse)?;

    let pxl = shader_creator.pixel_name();

    ss.declare_float4_f64("gamma", red_gamma, grn_gamma, blu_gamma, alpha_gamma)?;

    ss.new_line()
        .put(ss.float4_decl("signcol")?)
        .put(" = ")
        .put(ss.sign(pxl))
        .put(";");
    ss.new_line()
        .put(ss.float4_decl("res")?)
        .put(" = signcol * pow( abs( ")
        .put(pxl)
        .put(" ), gamma );");

    write_back(ss, pxl);
    Ok(())
}

/// Basic pass-through style: the power above 0, the pixel elsewhere; `1. / gamma` reversed.
///
/// Port of `AddBasicPassThruFwdShader` and `AddBasicPassThruRevShader`
/// (GammaOpGPU.cpp:102-157 @ v2.5.2).
fn add_basic_pass_thru_shader(
    shader_creator: &GpuShaderDesc,
    gamma: &GammaOpData,
    ss: &GpuShaderText,
    reverse: bool,
) -> Result<()> {
    let [red_gamma, grn_gamma, blu_gamma, alpha_gamma] = basic_gammas(gamma, reverse)?;

    let pxl = shader_creator.pixel_name();

    ss.declare_float4_f64("gamma", red_gamma, grn_gamma, blu_gamma, alpha_gamma)?;
    ss.declare_float4_f32("breakPnt", 0.0, 0.0, 0.0, 0.0)?;

    ss.new_line()
        .put(ss.float4_decl("isAboveBreak")?)
        .put(" = ")
        .put(ss.float4_greater_than(pxl, "breakPnt"))
        .put(";");

    ss.new_line()
        .put(ss.float4_decl("powSeg")?)
        .put(" = pow(max( ")
        .put(ss.float4_splat_f32(0.0))
        .put(", ")
        .put(pxl)
        .put(" ), gamma);");

    ss.new_line()
        .put(ss.float4_decl("res")?)
        .put(" = isAboveBreak * powSeg + ( ")
        .put(ss.float4_splat_f32(1.0))
        .put(" - isAboveBreak ) * ")
        .put(pxl)
        .put(";");

    write_back(ss, pxl);
    Ok(())
}

/// Moncurve forward style: a linear segment up to the break point, then
/// `pow(max(0, scale * pixel + offset), gamma)`.
///
/// Port of `AddMoncurveFwdShader` (GammaOpGPU.cpp:159-195 @ v2.5.2).
fn add_moncurve_fwd_shader(
    shader_creator: &GpuShaderDesc,
    gamma: &GammaOpData,
    ss: &GpuShaderText,
) -> Result<()> {
    let params = moncurve_params(gamma, false)?;

    let pxl = shader_creator.pixel_name();

    declare_moncurve_params(ss, &params)?;

    ss.new_line()
        .put(ss.float4_decl("isAboveBreak")?)
        .put(" = ")
        .put(ss.float4_greater_than(pxl, "breakPnt"))
        .put(";");

    ss.new_line()
        .put(ss.float4_decl("linSeg")?)
        .put(" = ")
        .put(pxl)
        .put(" * slope;");

    ss.new_line()
        .put(ss.float4_decl("powSeg")?)
        .put(" = pow( max( ")
        .put(ss.float4_splat_f32(0.0))
        .put(", scale * ")
        .put(pxl)
        .put(" + offset), gamma);");

    ss.new_line()
        .put(ss.float4_decl("res")?)
        .put(" = isAboveBreak * powSeg + ( ")
        .put(ss.float4_splat_f32(1.0))
        .put(" - isAboveBreak ) * linSeg;");

    write_back(ss, pxl);
    Ok(())
}

/// Moncurve reverse style: a linear segment up to the break point, then
/// `pow(max(0, pixel), gamma) * scale - offset`.
///
/// Port of `AddMoncurveRevShader` (GammaOpGPU.cpp:197-232 @ v2.5.2).
fn add_moncurve_rev_shader(
    shader_creator: &GpuShaderDesc,
    gamma: &GammaOpData,
    ss: &GpuShaderText,
) -> Result<()> {
    let params = moncurve_params(gamma, true)?;

    let pxl = shader_creator.pixel_name();

    declare_moncurve_params(ss, &params)?;

    ss.new_line()
        .put(ss.float4_decl("isAboveBreak")?)
        .put(" = ")
        .put(ss.float4_greater_than(pxl, "breakPnt"))
        .put(";");

    ss.new_line()
        .put(ss.float4_decl("linSeg")?)
        .put(" = ")
        .put(pxl)
        .put(" * slope;");
    ss.new_line()
        .put(ss.float4_decl("powSeg")?)
        .put(" = pow( max( ")
        .put(ss.float4_splat_f32(0.0))
        .put(", ")
        .put(pxl)
        .put(" ), gamma ) * scale - offset;");

    ss.new_line()
        .put(ss.float4_decl("res")?)
        .put(" = isAboveBreak * powSeg + ( ")
        .put(ss.float4_splat_f32(1.0))
        .put(" - isAboveBreak ) * linSeg;");

    write_back(ss, pxl);
    Ok(())
}

/// Moncurve mirror forward style: the forward moncurve of `abs(pixel)`, with the pixel's
/// sign. The pixel is overwritten with its absolute value first.
///
/// Port of `AddMoncurveMirrorFwdShader` (GammaOpGPU.cpp:234-274 @ v2.5.2).
fn add_moncurve_mirror_fwd_shader(
    shader_creator: &GpuShaderDesc,
    gamma: &GammaOpData,
    ss: &GpuShaderText,
) -> Result<()> {
    let params = moncurve_params(gamma, false)?;

    let pxl = shader_creator.pixel_name();

    declare_moncurve_params(ss, &params)?;

    ss.new_line()
        .put(ss.float4_decl("signcol")?)
        .put(" = ")
        .put(ss.sign(pxl))
        .put(";");
    ss.new_line().put(pxl).put(" = abs( ").put(pxl).put(" );");

    ss.new_line()
        .put(ss.float4_decl("isAboveBreak")?)
        .put(" = ")
        .put(ss.float4_greater_than(pxl, "breakPnt"))
        .put(";");

    ss.new_line()
        .put(ss.float4_decl("linSeg")?)
        .put(" = ")
        .put(pxl)
        .put(" * slope;");

    // Max() not needed since offset cannot be negative.
    ss.new_line()
        .put(ss.float4_decl("powSeg")?)
        .put(" = pow( scale * ")
        .put(pxl)
        .put(" + offset, gamma);");

    ss.new_line()
        .put(ss.float4_decl("res")?)
        .put(" = isAboveBreak * powSeg + ( ")
        .put(ss.float4_splat_f32(1.0))
        .put(" - isAboveBreak ) * linSeg;");

    ss.new_line().put("res = signcol * res;");

    write_back(ss, pxl);
    Ok(())
}

/// Moncurve mirror reverse style: the reverse moncurve of `abs(pixel)`, with the pixel's
/// sign. The pixel is overwritten with its absolute value first.
///
/// Port of `AddMoncurveMirrorRevShader` (GammaOpGPU.cpp:276-314 @ v2.5.2).
fn add_moncurve_mirror_rev_shader(
    shader_creator: &GpuShaderDesc,
    gamma: &GammaOpData,
    ss: &GpuShaderText,
) -> Result<()> {
    let params = moncurve_params(gamma, true)?;

    let pxl = shader_creator.pixel_name();

    declare_moncurve_params(ss, &params)?;

    ss.new_line()
        .put(ss.float4_decl("signcol")?)
        .put(" = ")
        .put(ss.sign(pxl))
        .put(";");
    ss.new_line().put(pxl).put(" = abs( ").put(pxl).put(" );");

    ss.new_line()
        .put(ss.float4_decl("isAboveBreak")?)
        .put(" = ")
        .put(ss.float4_greater_than(pxl, "breakPnt"))
        .put(";");

    ss.new_line()
        .put(ss.float4_decl("linSeg")?)
        .put(" = ")
        .put(pxl)
        .put(" * slope;");
    ss.new_line()
        .put(ss.float4_decl("powSeg")?)
        .put(" = pow( ")
        .put(pxl)
        .put(", gamma ) * scale - offset;");

    ss.new_line()
        .put(ss.float4_decl("res")?)
        .put(" = isAboveBreak * powSeg + ( ")
        .put(ss.float4_splat_f32(1.0))
        .put(" - isAboveBreak ) * linSeg;");

    ss.new_line().put("res = signcol * res;");

    write_back(ss, pxl);
    Ok(())
}

/// Adds the code of a Gamma op to `shader_creator`'s function body: a comment naming the
/// style, and a block that computes `res` from the pixel with the style's writer and writes
/// it back.
///
/// Port of `GetGammaGPUShaderProgram` (GammaOpGPU.cpp:318-392 @ v2.5.2).
pub fn get_gamma_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    gamma_data: &GammaOpData,
) -> Result<()> {
    let ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    ss.new_line().put("");
    ss.new_line()
        .put("// Add Gamma '")
        .put(GammaOpData::convert_style_to_string(gamma_data.style()))
        .put("' processing");
    ss.new_line().put("");

    ss.new_line().put("{");
    ss.indent();

    let creator = &*shader_creator;
    match gamma_data.style() {
        GammaStyle::MoncurveFwd => add_moncurve_fwd_shader(creator, gamma_data, &ss)?,
        GammaStyle::MoncurveRev => add_moncurve_rev_shader(creator, gamma_data, &ss)?,
        GammaStyle::MoncurveMirrorFwd => add_moncurve_mirror_fwd_shader(creator, gamma_data, &ss)?,
        GammaStyle::MoncurveMirrorRev => add_moncurve_mirror_rev_shader(creator, gamma_data, &ss)?,
        GammaStyle::BasicFwd => add_basic_shader(creator, gamma_data, &ss, false)?,
        GammaStyle::BasicRev => add_basic_shader(creator, gamma_data, &ss, true)?,
        GammaStyle::BasicMirrorFwd => add_basic_mirror_shader(creator, gamma_data, &ss, false)?,
        GammaStyle::BasicMirrorRev => add_basic_mirror_shader(creator, gamma_data, &ss, true)?,
        GammaStyle::BasicPassThruFwd => {
            add_basic_pass_thru_shader(creator, gamma_data, &ss, false)?
        }
        GammaStyle::BasicPassThruRev => add_basic_pass_thru_shader(creator, gamma_data, &ss, true)?,
    }

    ss.dedent();
    ss.new_line().put("}");
    ss.dedent();

    shader_creator.add_to_function_shader_code(ss.string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_color_types::GpuLanguage;

    /// Every style refuses a channel without the parameters it reads (upstream reads past
    /// them: `docs/improvements.md` U-24), whichever channel it is; a basic style reads only
    /// the first one, and a moncurve style the first two.
    #[test]
    fn short_parameters_are_refused() {
        let styles = [
            GammaStyle::BasicFwd,
            GammaStyle::BasicRev,
            GammaStyle::BasicMirrorFwd,
            GammaStyle::BasicMirrorRev,
            GammaStyle::BasicPassThruFwd,
            GammaStyle::BasicPassThruRev,
            GammaStyle::MoncurveFwd,
            GammaStyle::MoncurveRev,
            GammaStyle::MoncurveMirrorFwd,
            GammaStyle::MoncurveMirrorRev,
        ];
        for style in styles {
            let used = if style.is_basic() { 1 } else { 2 };
            let full = vec![2.0, 0.1];
            for channel in 0..4 {
                let mut params = [full.clone(), full.clone(), full.clone(), full.clone()];
                params[channel].truncate(used - 1);
                let [r, g, b, a] = params;
                let data = GammaOpData::new(style, r, g, b, a);
                let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl4_0);
                let err = get_gamma_gpu_shader_program(&mut desc, &data).unwrap_err();
                assert_eq!(err.message(), SHORT_PARAMS, "{style:?}, channel {channel}");
            }
            let used_only = vec![full[..used].to_vec(); 4];
            let [r, g, b, a] = <[Vec<f64>; 4]>::try_from(used_only).unwrap();
            let data = GammaOpData::new(style, r, g, b, a);
            let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl4_0);
            assert!(
                get_gamma_gpu_shader_program(&mut desc, &data).is_ok(),
                "{style:?}"
            );
        }
    }
}
