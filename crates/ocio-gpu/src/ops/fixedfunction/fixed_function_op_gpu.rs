// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/fixedfunction/FixedFunctionOpGPU.h` and
//! `FixedFunctionOpGPU.cpp` @ v2.5.2: the FixedFunction op's GPU writer.
//!
//! So far the ACES 1.x styles: the red modifiers 0.3 and 1.0, the glows 0.3 and 1.0, the dark
//! to dim surround 1.0 and the gamut compression 1.3, forward and inverse (chunk 2.3f). The
//! other styles' shaders come with chunks 2.3g1, 2.3g2 and card `p2-aces2-gpu`; until then
//! [`get_fixed_function_gpu_processing_text`] refuses them ([`not_ported`]).
//!
//! Upstream writes `float` values into the text with `operator<<`, which formats them with
//! `getFloatString` (`GpuShaderText`'s stream operators); the constants it derives from them
//! are computed in `float`, as here.

use ocio_ops::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle, SHORT_PARAMS,
};
use ocio_ops::{Exception, Result};

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

/// The port's error for a style whose shader isn't ported yet.
pub fn not_ported(style: FixedFunctionOpStyle) -> Exception {
    Exception::new(format!(
        "The GPU writer of FixedFunction style {} is not ported yet.",
        style.to_str(true)
    ))
}

/// `(float) params[i]`, or [`SHORT_PARAMS`] where upstream would read past the parameters
/// (`docs/improvements.md` U-31).
fn param_f32(func: &FixedFunctionOpData, i: usize) -> Result<f32> {
    func.params()
        .get(i)
        .map(|&v| v as f32)
        .ok_or_else(|| Exception::new(SHORT_PARAMS))
}

/// The hue weight of the red modifiers: the hue from `atan2`, then a cubic B-spline of the
/// hue over `width` degrees, into `f_H`.
///
/// Port of `Add_hue_weight_shader` (FixedFunctionOpGPU.cpp:16-54 @ v2.5.2).
fn add_hue_weight_shader(pxl: &[u8], st: &GpuShaderText, width: f32) -> Result<()> {
    // Convert from degrees to radians.
    #[allow(clippy::approx_constant)] // Upstream's literal.
    const PI: f32 = 3.14159265358979;
    let width_r = width * PI / 180.0;
    // Actually want to multiply by (4/width).
    let inv_width = 4.0f32 / width_r;

    st.new_line()
        .put(st.float_decl("a")?)
        .put(" = 2.0 * ")
        .put(pxl)
        .put(".rgb.r - (")
        .put(pxl)
        .put(".rgb.g + ")
        .put(pxl)
        .put(".rgb.b);");
    st.new_line()
        .put(st.float_decl("b")?)
        .put(" = 1.7320508075688772 * (")
        .put(pxl)
        .put(".rgb.g - ")
        .put(pxl)
        .put(".rgb.b);");
    st.new_line()
        .put(st.float_decl("hue")?)
        .put(" = ")
        .put(st.atan2("b", "a"))
        .put(";");

    st.new_line()
        .put(st.float_decl("knot_coord")?)
        .put(" = clamp(2. + hue * float(")
        .put(inv_width)
        .put("), 0., 4.);");
    st.new_line().put("int j = int(min(knot_coord, 3.));");
    st.new_line()
        .put(st.float_decl("t")?)
        .put(" = knot_coord - float(j);");
    st.new_line()
        .put(st.float4_decl("monomials")?)
        .put(" = ")
        .put(st.float4_const("t*t*t", "t*t", "t", "1."))
        .put(";");
    for (name, m) in [
        ("m0", [0.25, 0.00, 0.00, 0.00]),
        ("m1", [-0.75, 0.75, 0.75, 0.25]),
        ("m2", [0.75, -1.50, 0.00, 1.00]),
        ("m3", [-0.25, 0.75, -0.75, 0.25]),
    ] {
        st.new_line()
            .put(st.float4_decl(name)?)
            .put(" = ")
            .put(st.float4_const_f64(m[0], m[1], m[2], m[3]))
            .put(";");
    }
    st.new_line()
        .put(st.float4_decl("coefs")?)
        .put(" = ")
        .put(st.lerp("m0", "m1", "float(j == 1)"))
        .put(";");
    st.new_line()
        .put("coefs = ")
        .put(st.lerp("coefs", "m2", "float(j == 2)"))
        .put(";");
    st.new_line()
        .put("coefs = ")
        .put(st.lerp("coefs", "m3", "float(j == 3)"))
        .put(";");
    st.new_line()
        .put(st.float_decl("f_H")?)
        .put(" = dot(coefs, monomials);");
    Ok(())
}

/// `<name> = max( <pxl>.rgb.r, max( <pxl>.rgb.g, <pxl>.rgb.b));`, or the `min`.
fn write_rgb_extreme(st: &GpuShaderText, pxl: &[u8], name: &str, function: &str) -> Result<()> {
    st.new_line()
        .put(st.float_decl(name)?)
        .put(" = ")
        .put(function)
        .put("( ")
        .put(pxl)
        .put(".rgb.r, ")
        .put(function)
        .put("( ")
        .put(pxl)
        .put(".rgb.g, ")
        .put(pxl)
        .put(".rgb.b));");
    Ok(())
}

/// The red modifier's saturation: `f_S = ( max(1e-10, maxval) - max(1e-10, minval) ) /
/// max(1e-2, maxval)`, or the glow's `sat`.
fn write_sat(st: &GpuShaderText, name: &str) -> Result<()> {
    st.new_line()
        .put(st.float_decl(name)?)
        .put(" = ( max(1e-10, maxval) - max(1e-10, minval) ) / max(1e-2, maxval);");
    Ok(())
}

/// The red modifiers' rescaling of the chroma: `newChroma` from the new maximum, then
/// `rgb = minval + delta * newChroma / oldChroma`.
fn write_rescale_chroma(st: &GpuShaderText, pxl: &[u8]) -> Result<()> {
    write_rgb_extreme(st, pxl, "maxval2", "max")?;
    st.new_line()
        .put(st.float_decl("newChroma")?)
        .put(" = maxval2 - minval;");
    st.new_line()
        .put(pxl)
        .put(".rgb = minval + delta * newChroma / oldChroma;");
    Ok(())
}

/// `oldChroma` and `delta`, the red modifier 0.3's chroma before the change.
fn write_old_chroma(st: &GpuShaderText, pxl: &[u8]) -> Result<()> {
    st.new_line()
        .put(st.float_decl("oldChroma")?)
        .put(" = max(1e-10, maxval - minval);");
    st.new_line()
        .put(st.float3_decl("delta")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb - minval;");
    Ok(())
}

/// The forward red modifier: red moved towards the pivot by the hue weight times the
/// saturation.
fn write_red_mod_fwd(st: &GpuShaderText, pxl: &[u8], one_minus_scale: f32, pivot: f32) {
    st.new_line()
        .put(pxl)
        .put(".rgb.r = ")
        .put(pxl)
        .put(".rgb.r + f_H * f_S * (")
        .put(pivot)
        .put(" - ")
        .put(pxl)
        .put(".rgb.r) * ")
        .put(one_minus_scale)
        .put(";");
}

/// The inverse red modifier's quadratic for red, given `minval`.
fn write_red_mod_inv(
    st: &GpuShaderText,
    pxl: &[u8],
    one_minus_scale: f32,
    pivot: f32,
) -> Result<()> {
    // Note: If f_H == 0, the following generally doesn't change the red value,
    //       but it does for R < 0, hence the need for the if-statement above.
    st.new_line()
        .put(st.float_decl("ka")?)
        .put(" = f_H * ")
        .put(one_minus_scale)
        .put(" - 1.;");
    st.new_line()
        .put(st.float_decl("kb")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.r - f_H * (")
        .put(pivot)
        .put(" + minval) * ")
        .put(one_minus_scale)
        .put(";");
    st.new_line()
        .put(st.float_decl("kc")?)
        .put(" = f_H * ")
        .put(pivot)
        .put(" * minval * ")
        .put(one_minus_scale)
        .put(";");
    st.new_line()
        .put(pxl)
        .put(".rgb.r = ( -kb - sqrt( kb * kb - 4. * ka * kc)) / ( 2. * ka);");
    Ok(())
}

/// Port of `Add_RedMod_03_Fwd_Shader` (FixedFunctionOpGPU.cpp:56-79 @ v2.5.2).
fn add_red_mod_03_fwd_shader(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    let one_minus_scale = 1.0f32 - 0.85; // (1. - scale) from the original ctl code
    let pivot = 0.03f32;

    add_hue_weight_shader(pxl, st, 120.0)?;

    write_rgb_extreme(st, pxl, "maxval", "max")?;
    write_rgb_extreme(st, pxl, "minval", "min")?;
    write_old_chroma(st, pxl)?;
    write_sat(st, "f_S")?;
    write_red_mod_fwd(st, pxl, one_minus_scale, pivot);
    write_rescale_chroma(st, pxl)
}

/// Port of `Add_RedMod_03_Inv_Shader` (FixedFunctionOpGPU.cpp:81-114 @ v2.5.2).
fn add_red_mod_03_inv_shader(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    let one_minus_scale = 1.0f32 - 0.85; // (1. - scale) from the original ctl code
    let pivot = 0.03f32;

    add_hue_weight_shader(pxl, st, 120.0)?;

    st.new_line().put("if (f_H > 0.)");
    st.new_line().put("{");
    st.indent();

    write_rgb_extreme(st, pxl, "maxval", "max")?;
    write_rgb_extreme(st, pxl, "minval", "min")?;
    write_old_chroma(st, pxl)?;
    write_red_mod_inv(st, pxl, one_minus_scale, pivot)?;
    write_rescale_chroma(st, pxl)?;

    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// Port of `Add_RedMod_10_Fwd_Shader` (FixedFunctionOpGPU.cpp:116-132 @ v2.5.2).
fn add_red_mod_10_fwd_shader(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    let one_minus_scale = 1.0f32 - 0.82; // (1. - scale) from the original ctl code
    let pivot = 0.03f32;

    add_hue_weight_shader(pxl, st, 135.0)?;

    write_rgb_extreme(st, pxl, "maxval", "max")?;
    write_rgb_extreme(st, pxl, "minval", "min")?;
    write_sat(st, "f_S")?;
    write_red_mod_fwd(st, pxl, one_minus_scale, pivot);
    Ok(())
}

/// Port of `Add_RedMod_10_Inv_Shader` (FixedFunctionOpGPU.cpp:134-159 @ v2.5.2).
fn add_red_mod_10_inv_shader(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    let one_minus_scale = 1.0f32 - 0.82; // (1. - scale) from the original ctl code
    let pivot = 0.03f32;

    add_hue_weight_shader(pxl, st, 135.0)?;

    st.new_line().put("if (f_H > 0.)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("minval")?)
        .put(" = min( ")
        .put(pxl)
        .put(".rgb.g, ")
        .put(pxl)
        .put(".rgb.b);");
    write_red_mod_inv(st, pxl, one_minus_scale, pivot)?;

    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// The glows' chroma, `YC`, saturation and its sigmoid `s`, and `GlowGain` and `GlowMid`.
fn write_glow_common(pxl: &[u8], st: &GpuShaderText, glow_gain: f32, glow_mid: f32) -> Result<()> {
    st.new_line()
        .put(st.float_decl("chroma")?)
        .put(" = sqrt( ")
        .put(pxl)
        .put(".rgb.b * (")
        .put(pxl)
        .put(".rgb.b - ")
        .put(pxl)
        .put(".rgb.g)")
        .put(" + ")
        .put(pxl)
        .put(".rgb.g * (")
        .put(pxl)
        .put(".rgb.g - ")
        .put(pxl)
        .put(".rgb.r)")
        .put(" + ")
        .put(pxl)
        .put(".rgb.r * (")
        .put(pxl)
        .put(".rgb.r - ")
        .put(pxl)
        .put(".rgb.b) );");
    st.new_line()
        .put(st.float_decl("YC")?)
        .put(" = (")
        .put(pxl)
        .put(".rgb.b + ")
        .put(pxl)
        .put(".rgb.g + ")
        .put(pxl)
        .put(".rgb.r + 1.75 * chroma) / 3.;");

    write_rgb_extreme(st, pxl, "maxval", "max")?;
    write_rgb_extreme(st, pxl, "minval", "min")?;
    write_sat(st, "sat")?;

    st.new_line()
        .put(st.float_decl("x")?)
        .put(" = (sat - 0.4) * 5.;");
    st.new_line()
        .put(st.float_decl("t")?)
        .put(" = max( 0., 1. - 0.5 * abs(x));");
    st.new_line()
        .put(st.float_decl("s")?)
        .put(" = 0.5 * (1. + sign(x) * (1. - t * t));");

    st.new_line()
        .put(st.float_decl("GlowGain")?)
        .put(" = ")
        .put(glow_gain)
        .put(" * s;");
    st.new_line()
        .put(st.float_decl("GlowMid")?)
        .put(" = ")
        .put(glow_mid)
        .put(";");
    Ok(())
}

/// `glowGainOut` limited to 0 above `GlowMid * 2`, then applied to the pixel.
fn write_glow_apply(pxl: &[u8], st: &GpuShaderText) {
    st.new_line()
        .put("glowGainOut = ")
        .put(st.lerp("glowGainOut", "0.", "float( YC > GlowMid * 2. )"))
        .put(";");

    st.new_line()
        .put(pxl)
        .put(".rgb = ")
        .put(pxl)
        .put(".rgb * glowGainOut + ")
        .put(pxl)
        .put(".rgb;");
}

/// Port of `Add_Glow_03_Fwd_Shader` (FixedFunctionOpGPU.cpp:161-186 @ v2.5.2).
fn add_glow_03_fwd_shader(
    pxl: &[u8],
    st: &GpuShaderText,
    glow_gain: f32,
    glow_mid: f32,
) -> Result<()> {
    write_glow_common(pxl, st, glow_gain, glow_mid)?;
    st.new_line()
        .put(st.float_decl("glowGainOut")?)
        .put(" = ")
        .put(st.lerp(
            "GlowGain",
            "GlowGain * (GlowMid / YC - 0.5)",
            "float( YC > GlowMid * 2. / 3. )",
        ))
        .put(";");
    write_glow_apply(pxl, st);
    Ok(())
}

/// Port of `Add_Glow_03_Inv_Shader` (FixedFunctionOpGPU.cpp:188-215 @ v2.5.2).
fn add_glow_03_inv_shader(
    pxl: &[u8],
    st: &GpuShaderText,
    glow_gain: f32,
    glow_mid: f32,
) -> Result<()> {
    write_glow_common(pxl, st, glow_gain, glow_mid)?;
    st.new_line()
        .put(st.float_decl("glowGainOut")?)
        .put(" = ")
        .put(st.lerp(
            "-GlowGain / (1. + GlowGain)",
            "GlowGain * (GlowMid / YC - 0.5) / (GlowGain * 0.5 - 1.)",
            "float( YC > (1. + GlowGain) * GlowMid * 2. / 3. )",
        ))
        .put(";");
    write_glow_apply(pxl, st);
    Ok(())
}

/// The compression of one distance above its threshold.
///
/// Port of `Add_GamutComp_13_Shader_Compress` (FixedFunctionOpGPU.cpp:217-236 @ v2.5.2).
fn add_gamut_comp_13_shader_compress(
    st: &GpuShaderText,
    dist: &str,
    cdist: &str,
    scl: f32,
    thr: f32,
    power: f32,
) -> Result<()> {
    // Only compress if greater or equal than threshold.
    st.new_line()
        .put("if (")
        .put(dist)
        .put(" >= ")
        .put(thr)
        .put(")");
    st.new_line().put("{");
    st.indent();

    // Normalize distance outside threshold by scale factor.
    st.new_line()
        .put(st.float_decl("nd")?)
        .put(" = (")
        .put(dist)
        .put(" - ")
        .put(thr)
        .put(") / ")
        .put(scl)
        .put(";");
    st.new_line()
        .put(st.float_decl("p")?)
        .put(" = pow(nd, ")
        .put(power)
        .put(");");
    st.new_line()
        .put(cdist)
        .put(" = ")
        .put(thr)
        .put(" + ")
        .put(scl)
        .put(" * nd / (pow(1.0 + p, ")
        .put(1.0f32 / power)
        .put("));");

    st.dedent();
    st.new_line().put("}"); // if (dist >= thr)
    Ok(())
}

/// The inverse compression of one distance between its threshold and the limit.
///
/// Port of `Add_GamutComp_13_Shader_UnCompress` (FixedFunctionOpGPU.cpp:238-257 @ v2.5.2).
fn add_gamut_comp_13_shader_uncompress(
    st: &GpuShaderText,
    dist: &str,
    cdist: &str,
    scl: f32,
    thr: f32,
    power: f32,
) -> Result<()> {
    // Only compress if greater or equal than threshold, avoid singularity.
    st.new_line()
        .put("if (")
        .put(dist)
        .put(" >= ")
        .put(thr)
        .put(" && ")
        .put(dist)
        .put(" < ")
        .put(thr + scl)
        .put(" )");
    st.new_line().put("{");
    st.indent();

    // Normalize distance outside threshold by scale factor.
    st.new_line()
        .put(st.float_decl("nd")?)
        .put(" = (")
        .put(dist)
        .put(" - ")
        .put(thr)
        .put(") / ")
        .put(scl)
        .put(";");
    st.new_line()
        .put(st.float_decl("p")?)
        .put(" = pow(nd, ")
        .put(power)
        .put(");");
    st.new_line()
        .put(cdist)
        .put(" = ")
        .put(thr)
        .put(" + ")
        .put(scl)
        .put(" * pow(-(p / (p - 1.0)), ")
        .put(1.0f32 / power)
        .put(");");

    st.dedent();
    st.new_line().put("}"); // if (dist >= thr && dist < thr + scl)
    Ok(())
}

/// The writer of one distance's compression or its inverse.
type DistanceShader = fn(&GpuShaderText, &str, &str, f32, f32, f32) -> Result<()>;

/// The ACES 1.3 gamut compression's frame: each channel's distance from the achromatic
/// axis, compressed by `f`, then the pixel rebuilt from them.
///
/// Port of `Add_GamutComp_13_Shader` (FixedFunctionOpGPU.cpp:259-302 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn add_gamut_comp_13_shader(
    st: &GpuShaderText,
    pix: &[u8],
    lim_cyan: f32,
    lim_magenta: f32,
    lim_yellow: f32,
    thr_cyan: f32,
    thr_magenta: f32,
    thr_yellow: f32,
    power: f32,
    f: DistanceShader,
) -> Result<()> {
    // Precompute scale factor for y = 1 intersect
    let f_scale = |lim: f32, thr: f32| {
        (lim - thr) / (((1.0f32 - thr) / (lim - thr)).powf(-power) - 1.0).powf(1.0 / power)
    };
    let scale_cyan = f_scale(lim_cyan, thr_cyan);
    let scale_magenta = f_scale(lim_magenta, thr_magenta);
    let scale_yellow = f_scale(lim_yellow, thr_yellow);

    // Achromatic axis
    st.new_line()
        .put(st.float_decl("ach")?)
        .put(" = max( ")
        .put(pix)
        .put(".rgb.r, max( ")
        .put(pix)
        .put(".rgb.g, ")
        .put(pix)
        .put(".rgb.b ) );");

    st.new_line().put("if ( ach != 0. )");
    st.new_line().put("{");
    st.indent();

    // Distance from the achromatic axis for each color component aka inverse rgb ratios.
    st.new_line()
        .put(st.float3_decl("dist")?)
        .put(" = (ach - ")
        .put(pix)
        .put(".rgb) / abs(ach);");
    st.new_line().put(st.float3_decl("cdist")?).put(" = dist;");

    f(st, "dist.x", "cdist.x", scale_cyan, thr_cyan, power)?;
    f(st, "dist.y", "cdist.y", scale_magenta, thr_magenta, power)?;
    f(st, "dist.z", "cdist.z", scale_yellow, thr_yellow, power)?;

    // Recalculate rgb from compressed distance and achromatic.
    // Effectively this scales each color component relative to achromatic axis by the
    // compressed distance.
    st.new_line().put(pix).put(".rgb = ach - cdist * abs(ach);");

    st.dedent();
    st.new_line().put("}"); // if ( ach != 0.0f )
    Ok(())
}

/// Port of `Add_GamutComp_13_Fwd_Shader` and `Add_GamutComp_13_Inv_Shader`
/// (FixedFunctionOpGPU.cpp:304-350 @ v2.5.2), the parameters read as upstream's dispatch reads
/// them (2338-2365).
fn add_gamut_comp_13(
    st: &GpuShaderText,
    pix: &[u8],
    func: &FixedFunctionOpData,
    f: DistanceShader,
) -> Result<()> {
    let p = |i| param_f32(func, i);
    add_gamut_comp_13_shader(st, pix, p(0)?, p(1)?, p(2)?, p(3)?, p(4)?, p(5)?, p(6)?, f)
}

/// The ACES dark to dim surround 1.0: the pixel times `Y^(gamma - 1)` of its luminance.
///
/// Port of `Add_Surround_10_Fwd_Shader` (FixedFunctionOpGPU.cpp:1636-1649 @ v2.5.2).
fn add_surround_10_fwd_shader(pxl: &[u8], st: &GpuShaderText, gamma: f32) -> Result<()> {
    // TODO: -- add vector inner product to GPUShaderUtils
    st.new_line()
        .put(st.float_decl("Y")?)
        .put(" = max( 1e-10, 0.27222871678091454 * ")
        .put(pxl)
        .put(".rgb.r + ")
        .put("0.67408176581114831 * ")
        .put(pxl)
        .put(".rgb.g + ")
        .put("0.053689517407937051 * ")
        .put(pxl)
        .put(".rgb.b );");

    st.new_line()
        .put(st.float_decl("Ypow_over_Y")?)
        .put(" = pow( Y, ")
        .put(gamma - 1.0)
        .put(");");

    st.new_line()
        .put(pxl)
        .put(".rgb = ")
        .put(pxl)
        .put(".rgb * Ypow_over_Y;");
    Ok(())
}

/// Adds the code of a FixedFunction op to `shader_creator`'s function body.
///
/// Port of `GetFixedFunctionGPUShaderProgram` (FixedFunctionOpGPU.cpp:2225-2231 @ v2.5.2).
pub fn get_fixed_function_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    func: &FixedFunctionOpData,
) -> Result<()> {
    let st = GpuShaderText::new(shader_creator.language());
    get_fixed_function_gpu_processing_text(shader_creator, &st, func)?;
    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}

/// Writes the code of a FixedFunction op into `st`: a comment naming the style, then the
/// style's shader in a block. The styles whose shaders aren't ported yet are the port's error
/// ([`not_ported`]).
///
/// Port of `GetFixedFunctionGPUProcessingText` (FixedFunctionOpGPU.cpp:2233-2491 @ v2.5.2).
pub fn get_fixed_function_gpu_processing_text(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    func: &FixedFunctionOpData,
) -> Result<()> {
    use FixedFunctionOpStyle::*;

    st.indent();

    st.new_line().put("");
    st.new_line()
        .put("// Add FixedFunction '")
        .put(func.style().to_str(true))
        .put("' processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let pxl = shader_creator.pixel_name().to_vec();
    match func.style() {
        AcesRedMod03Fwd => add_red_mod_03_fwd_shader(&pxl, st)?,
        AcesRedMod03Inv => add_red_mod_03_inv_shader(&pxl, st)?,
        AcesRedMod10Fwd => add_red_mod_10_fwd_shader(&pxl, st)?,
        AcesRedMod10Inv => add_red_mod_10_inv_shader(&pxl, st)?,
        AcesGlow03Fwd => add_glow_03_fwd_shader(&pxl, st, 0.075, 0.1)?,
        AcesGlow03Inv => add_glow_03_inv_shader(&pxl, st, 0.075, 0.1)?,
        // Use 03 renderer with different params.
        AcesGlow10Fwd => add_glow_03_fwd_shader(&pxl, st, 0.05, 0.08)?,
        // Use 03 renderer with different params.
        AcesGlow10Inv => add_glow_03_inv_shader(&pxl, st, 0.05, 0.08)?,
        AcesDarkToDim10Fwd => add_surround_10_fwd_shader(&pxl, st, 0.9811)?,
        // Call forward renderer with the inverse gamma.
        AcesDarkToDim10Inv => add_surround_10_fwd_shader(&pxl, st, 1.0192640913260627)?,
        AcesGamutComp13Fwd => {
            add_gamut_comp_13(st, &pxl, func, add_gamut_comp_13_shader_compress)?;
        }
        AcesGamutComp13Inv => {
            add_gamut_comp_13(st, &pxl, func, add_gamut_comp_13_shader_uncompress)?;
        }
        style => return Err(not_ported(style)),
    }

    st.dedent();
    st.new_line().put("}");

    st.dedent();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GpuLanguage;

    /// U-31: where upstream's shader reads past the parameters (data that doesn't validate,
    /// which no processor extracts), the port refuses.
    #[test]
    fn short_params_are_refused() {
        for style in [
            FixedFunctionOpStyle::AcesGamutComp13Fwd,
            FixedFunctionOpStyle::AcesGamutComp13Inv,
        ] {
            let mut data =
                FixedFunctionOpData::with_params(style, vec![1.2, 1.2, 1.2, 0.5, 0.5, 0.5, 1.2])
                    .unwrap();
            data.set_params(vec![1.2; 6]);
            let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl1_2);
            let error = get_fixed_function_gpu_shader_program(&mut desc, &data).unwrap_err();
            assert_eq!(error.message(), SHORT_PARAMS);
            assert!(desc.shader_text().is_empty());
        }
    }
}
