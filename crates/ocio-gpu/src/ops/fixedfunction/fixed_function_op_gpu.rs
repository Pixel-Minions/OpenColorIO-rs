// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/fixedfunction/FixedFunctionOpGPU.h` and
//! `FixedFunctionOpGPU.cpp` @ v2.5.2: the FixedFunction op's GPU writer.
//!
//! Every style: the ACES 1.x ones (chunk 2.3f); the Rec.2100 surround, RGB to and from HSV and
//! the three HSYs, and XYZ to and from xyY, u'v'Y and CIELUV (2.3g1); PQ, the gamma-log and the
//! double-log curves (2.3g2), with `double` parameters as upstream reads them; ACES 2.0's RGB
//! to and from JMh (2.4f1), its tone scale and chroma compression, with the reach table as a
//! texture (2.4f2), its gamut compression, with the cusp table as a texture and the hues as a
//! constant array (2.4g), and its output transform (2.4h).
//!
//! Upstream writes `float` values into the text with `operator<<`, which formats them with
//! `getFloatString` (`GpuShaderText`'s stream operators); the constants it derives from them
//! are computed in `float`, as here.

use ocio_ops::cfmt::{Crt, to_string_f32};
use ocio_ops::ops::fixedfunction::aces2::common::{
    CAM_NL_OFFSET, COMPRESSION_THRESHOLD, CUSP_MID_BLEND, ChromaCompressParams, FOCUS_GAIN_BLEND,
    GamutCompressParams, J_SCALE, JMhParams, REFERENCE_LUMINANCE, SMOOTH_CUSPS,
    SharedCompressionParameters, Table1D, ToneScaleParams, table_base,
};
use ocio_ops::ops::fixedfunction::aces2::transform::{
    init_chroma_compress_params, init_gamut_compress_params, init_jmh_params,
    init_shared_compression_params, init_tone_scale_params,
};
use ocio_ops::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle, SHORT_PARAMS,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::transforms::builtins::color_matrix_helpers::{
    Chromaticities, Primaries, aces_ap0, aces_ap1,
};
use ocio_ops::utils::string_utils::replace_in_place;
use ocio_ops::{Exception, Result};

use crate::GpuLanguage;
use crate::gpu_shader::{TextureDimensions, TextureType};
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

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

/// The Rec.2100 surround correction: the pixel times `Y^(gamma - 1)` of its luminance's
/// magnitude, limited below; the inverse with `1 / gamma` and the limit raised to `gamma`.
///
/// Port of `Add_Rec2100_Surround_Shader` (FixedFunctionOpGPU.cpp:1651-1673 @ v2.5.2).
fn add_rec2100_surround_shader(
    pxl: &[u8],
    st: &GpuShaderText,
    gamma: f32,
    is_forward: bool,
) -> Result<()> {
    let mut gamma = gamma;
    let mut min_lum = 1e-4f32;
    if !is_forward {
        min_lum = min_lum.powf(gamma);
        gamma = 1.0 / gamma;
    }

    st.new_line()
        .put(st.float_decl("Y")?)
        .put(" = 0.2627 * ")
        .put(pxl)
        .put(".rgb.r + ")
        .put("0.6780 * ")
        .put(pxl)
        .put(".rgb.g + ")
        .put("0.0593 * ")
        .put(pxl)
        .put(".rgb.b;");

    st.new_line()
        .put("Y = max( ")
        .put(min_lum)
        .put(", abs(Y) );");

    st.new_line()
        .put(st.float_decl("Ypow_over_Y")?)
        .put(" = pow( Y, ")
        .put(gamma - 1.0)
        .put(");");

    st.new_line()
        .put("")
        .put(pxl)
        .put(".rgb = ")
        .put(pxl)
        .put(".rgb * Ypow_over_Y;");
    Ok(())
}

/// Port of `Add_RGB_TO_HSV` (FixedFunctionOpGPU.cpp:1675-1702 @ v2.5.2).
fn add_rgb_to_hsv(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("minRGB")?)
        .put(" = min( ")
        .put(pxl)
        .put(".rgb.r, min( ")
        .put(pxl)
        .put(".rgb.g, ")
        .put(pxl)
        .put(".rgb.b ) );");
    st.new_line()
        .put(st.float_decl("maxRGB")?)
        .put(" = max( ")
        .put(pxl)
        .put(".rgb.r, max( ")
        .put(pxl)
        .put(".rgb.g, ")
        .put(pxl)
        .put(".rgb.b ) );");
    st.new_line().put(st.float_decl("val")?).put(" = maxRGB;");

    st.new_line()
        .put(st.float_decl("sat")?)
        .put(" = 0.0, hue = 0.0;");
    st.new_line().put("if (minRGB != maxRGB)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put("if (val != 0.0) sat = (maxRGB - minRGB) / val;");
    st.new_line()
        .put(st.float_decl("OneOverMaxMinusMin")?)
        .put(" = 1.0 / (maxRGB - minRGB);");
    st.new_line()
        .put("if ( maxRGB == ")
        .put(pxl)
        .put(".rgb.r ) hue = (")
        .put(pxl)
        .put(".rgb.g - ")
        .put(pxl)
        .put(".rgb.b) * OneOverMaxMinusMin;");
    st.new_line()
        .put("else if ( maxRGB == ")
        .put(pxl)
        .put(".rgb.g ) hue = 2.0 + (")
        .put(pxl)
        .put(".rgb.b - ")
        .put(pxl)
        .put(".rgb.r) * OneOverMaxMinusMin;");
    st.new_line()
        .put("else hue = 4.0 + (")
        .put(pxl)
        .put(".rgb.r - ")
        .put(pxl)
        .put(".rgb.g) * OneOverMaxMinusMin;");
    st.new_line().put("if ( hue < 0.0 ) hue += 6.0;");

    st.dedent();
    st.new_line().put("}");

    st.new_line().put("if ( minRGB < 0.0 ) val += minRGB;");
    st.new_line()
        .put("if ( -minRGB > maxRGB ) sat = (maxRGB - minRGB) / -minRGB;");

    st.new_line()
        .put(pxl)
        .put(".rgb = ")
        .put(st.float3_const("hue * 1./6.", "sat", "val"))
        .put(";");
    Ok(())
}

/// The variant of RGB to HSY and back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hsy {
    Lin,
    Log,
    Vid,
}

/// Port of `Add_RGB_TO_HSY` (FixedFunctionOpGPU.cpp:1704-1745 @ v2.5.2).
fn add_rgb_to_hsy(pxl: &[u8], st: &GpuShaderText, func_style: Hsy) -> Result<()> {
    st.new_line()
        .put(st.float3_decl("lumaWeights")?)
        .put(" = ")
        .put(st.float3_const_f32(0.2126, 0.7152, 0.0722))
        .put(";");
    st.new_line()
        .put(st.float3_decl("ones")?)
        .put(" = ")
        .put(st.float3_const_f32(1.0, 1.0, 1.0))
        .put(";");
    st.new_line()
        .put("float luma = dot(")
        .put(pxl)
        .put(".rgb, lumaWeights);");
    st.new_line()
        .put("float minRGB =  min( ")
        .put(pxl)
        .put(".x, min( ")
        .put(pxl)
        .put(".y, ")
        .put(pxl)
        .put(".z ) );");
    st.new_line()
        .put("float maxRGB =  max( ")
        .put(pxl)
        .put(".x, max( ")
        .put(pxl)
        .put(".y, ")
        .put(pxl)
        .put(".z ) );");
    st.new_line()
        .put(st.float3_decl("RGBm")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb - luma;");
    st.new_line()
        .put("float distRGB  = dot( abs(RGBm), ones );");
    match func_style {
        Hsy::Lin => {
            st.new_line()
                .put("float sumRGB  = dot( ")
                .put(pxl)
                .put(".rgb, ones );");
            st.new_line()
                .put("float sat_hi  = distRGB / max(0.07 * distRGB + 1e-6, 0.15 + sumRGB);");
            st.new_line().put("float sat_lo  = distRGB * 5.;");
            st.new_line()
                .put("float alpha  = clamp( (luma - 0.001) / (0.01 - 0.001), 0., 1.);");

            st.new_line()
                .put("float sat = sat_lo + alpha * (sat_hi - sat_lo);");
            st.new_line().put("sat *= 1.4;");
        }
        Hsy::Log => {
            st.new_line().put("float sat = distRGB * 4.;");
        }
        Hsy::Vid => {
            st.new_line().put("float sat = distRGB * 1.25;");
        }
    }
    // NB: Unlike typical HSV, HSY maps magenta rather than red to a hue of zero.
    // (This allows for better placement of red when manipulating curves in a UI.)
    st.new_line().put("float hue = 0.0;");
    st.new_line().put("if (minRGB != maxRGB) {");
    st.new_line()
        .put("   float OneOverMaxMinusMin = 1.0 / (maxRGB - minRGB);");
    st.new_line()
        .put("   if ( maxRGB == ")
        .put(pxl)
        .put(".r ) hue = 1.0 + (")
        .put(pxl)
        .put(".g - ")
        .put(pxl)
        .put(".b) * OneOverMaxMinusMin;");
    st.new_line()
        .put("   else if ( maxRGB == ")
        .put(pxl)
        .put(".g ) hue = 3.0 + (")
        .put(pxl)
        .put(".b - ")
        .put(pxl)
        .put(".r) * OneOverMaxMinusMin;");
    st.new_line()
        .put("   else hue = 5.0 + (")
        .put(pxl)
        .put(".r - ")
        .put(pxl)
        .put(".g) * OneOverMaxMinusMin;");
    st.new_line().put("}");
    st.new_line()
        .put("")
        .put(pxl)
        .put(".r = hue * 1./6.; ")
        .put(pxl)
        .put(".g = sat; ")
        .put(pxl)
        .put(".b = luma;");
    Ok(())
}

/// Port of `Add_HSY_TO_RGB` (FixedFunctionOpGPU.cpp:1747-1800 @ v2.5.2).
fn add_hsy_to_rgb(pxl: &[u8], st: &GpuShaderText, func_style: Hsy) -> Result<()> {
    st.new_line().put("float luma = ").put(pxl).put(".z;");
    st.new_line()
        .put("float Hue = ")
        .put(pxl)
        .put(".x - 1./6.;");
    st.new_line().put("Hue = (luma < 0.) ? Hue + 0.5 : Hue;");
    st.new_line().put("Hue = ( Hue - floor( Hue ) ) * 6.0;");
    st.new_line().put("float R = abs(Hue - 3.0) - 1.0;");
    st.new_line().put("float G = 2.0 - abs(Hue - 2.0);");
    st.new_line().put("float B = 2.0 - abs(Hue - 4.0);");
    st.new_line()
        .put(st.float3_decl("RGB0")?)
        .put(" = ")
        .put(st.float3_const("R", "G", "B"))
        .put(";");
    st.new_line().put("RGB0 = clamp( RGB0, 0., 1. );");

    st.new_line()
        .put(st.float3_decl("lumaWeights")?)
        .put(" = ")
        .put(st.float3_const_f32(0.2126, 0.7152, 0.0722))
        .put(";");
    st.new_line()
        .put(st.float3_decl("ones")?)
        .put(" = ")
        .put(st.float3_const_f32(1.0, 1.0, 1.0))
        .put(";");
    st.new_line().put("float currY = dot(RGB0, lumaWeights);");
    st.new_line().put("RGB0 *= luma / currY;");

    st.new_line().put("float sat = ").put(pxl).put(".y;");
    st.new_line()
        .put("float distRGB = dot( abs(RGB0 - luma), ones );");
    match func_style {
        Hsy::Lin => {
            for line in [
                "float sumRGB  = dot( RGB0, ones );",
                "float k = 0.15;",
                "float lo_gain = 5.;",
                "sat /= 1.4;",
                "float tmp = -sat * sumRGB + sat * 3. * luma + distRGB;",
                "tmp = max(1e-6, tmp);",
                "float s1 = sat * (k + 3. * luma) / tmp;",
                "s1 = min(s1, 50.);",
                "float s0 = sat / max(1e-10, distRGB * lo_gain);",
                "float alpha  = clamp( (luma - 0.001) / (0.01 - 0.001), 0., 1.);",
                "float a = distRGB * lo_gain * (1. - alpha) * (sumRGB - 3. * luma);",
                "float b = distRGB * lo_gain * (1. - alpha) * (k + 3. * luma) + distRGB * alpha \
                 - sat * (sumRGB - 3. * luma);",
                "float c = -sat * (k + 3. * luma);",
                "float discrim = sqrt( b * b - 4. * a * c );",
                "float denom = -discrim - b;",
                "float sm = (2. * c) / denom;",
                "sm = (sm >= 0.) ? sm : (2. * c) / (denom + discrim * 2.);",
                "float gainS = (alpha == 1.) ? s1 : (alpha == 0.) ? s0 : sm;",
            ] {
                st.new_line().put(line);
            }
        }
        Hsy::Log => {
            st.new_line()
                .put("float gainS = sat / max(1e-10, distRGB * 4.);");
        }
        Hsy::Vid => {
            st.new_line()
                .put("float gainS = sat / max(1e-10, distRGB * 1.25);");
        }
    }
    st.new_line()
        .put("")
        .put(pxl)
        .put(".rgb = luma + gainS * (RGB0 - luma);");
    Ok(())
}

/// Port of `Add_HSV_TO_RGB` (FixedFunctionOpGPU.cpp:1832-1867 @ v2.5.2).
fn add_hsv_to_rgb(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("Hue")?)
        .put(" = ( ")
        .put(pxl)
        .put(".rgb.r - floor( ")
        .put(pxl)
        .put(".rgb.r ) ) * 6.0;");
    st.new_line()
        .put(st.float_decl("Sat")?)
        .put(" = clamp( ")
        .put(pxl)
        .put(".rgb.g, 0., 1.999 );");
    st.new_line()
        .put(st.float_decl("Val")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.b;");

    st.new_line()
        .put(st.float_decl("R")?)
        .put(" = abs(Hue - 3.0) - 1.0;");
    st.new_line()
        .put(st.float_decl("G")?)
        .put(" = 2.0 - abs(Hue - 2.0);");
    st.new_line()
        .put(st.float_decl("B")?)
        .put(" = 2.0 - abs(Hue - 4.0);");
    st.new_line()
        .put(st.float3_decl("RGB")?)
        .put(" = ")
        .put(st.float3_const("R", "G", "B"))
        .put(";");
    st.new_line().put("RGB = clamp( RGB, 0., 1. );");

    st.new_line().put(st.float_keyword()).put(" rgbMax = Val;");
    st.new_line()
        .put(st.float_keyword())
        .put(" rgbMin = Val * (1.0 - Sat);");

    st.new_line().put("if ( Sat > 1.0 )");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("rgbMin = Val * (1.0 - Sat) / (2.0 - Sat);");
    st.new_line().put("rgbMax = Val - rgbMin;");
    st.dedent();
    st.new_line().put("}");
    st.new_line().put("if ( Val < 0.0 )");
    st.new_line().put("{");
    st.indent();
    st.new_line().put("rgbMin = Val / (2.0 - Sat);");
    st.new_line().put("rgbMax = Val - rgbMin;");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("RGB = RGB * (rgbMax - rgbMin) + rgbMin;");

    st.new_line().put("").put(pxl).put(".rgb = RGB;");
    Ok(())
}

/// Port of `Add_XYZ_TO_xyY` (FixedFunctionOpGPU.cpp:1869-1878 @ v2.5.2).
fn add_xyz_to_xyy(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("d")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.r + ")
        .put(pxl)
        .put(".rgb.g + ")
        .put(pxl)
        .put(".rgb.b;");
    st.new_line().put("d = (d == 0.) ? 0. : 1. / d;");
    st.new_line()
        .put(pxl)
        .put(".rgb.b = ")
        .put(pxl)
        .put(".rgb.g;");
    st.new_line().put(pxl).put(".rgb.r *= d;");
    st.new_line().put(pxl).put(".rgb.g *= d;");
    Ok(())
}

/// Port of `Add_xyY_TO_XYZ` (FixedFunctionOpGPU.cpp:1880-1889 @ v2.5.2).
fn add_xyy_to_xyz(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("d")?)
        .put(" = (")
        .put(pxl)
        .put(".rgb.g == 0.) ? 0. : 1. / ")
        .put(pxl)
        .put(".rgb.g;");
    st.new_line()
        .put(st.float_decl("Y")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.b;");
    st.new_line()
        .put(pxl)
        .put(".rgb.b = Y * (1. - ")
        .put(pxl)
        .put(".rgb.r - ")
        .put(pxl)
        .put(".rgb.g) * d;");
    st.new_line().put(pxl).put(".rgb.r *= Y * d;");
    st.new_line().put(pxl).put(".rgb.g = Y;");
    Ok(())
}

/// The CIE 1976 denominator `X + 15 Y + 3 Z`, inverted where it isn't 0.
fn write_uv_denominator(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("d")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.r + 15. * ")
        .put(pxl)
        .put(".rgb.g + 3. * ")
        .put(pxl)
        .put(".rgb.b;");
    st.new_line().put("d = (d == 0.) ? 0. : 1. / d;");
    Ok(())
}

/// Port of `Add_XYZ_TO_uvY` (FixedFunctionOpGPU.cpp:1891-1900 @ v2.5.2).
fn add_xyz_to_uvy(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    write_uv_denominator(pxl, st)?;
    st.new_line()
        .put(pxl)
        .put(".rgb.b = ")
        .put(pxl)
        .put(".rgb.g;");
    st.new_line().put(pxl).put(".rgb.r *= 4. * d;");
    st.new_line().put(pxl).put(".rgb.g *= 9. * d;");
    Ok(())
}

/// Port of `Add_uvY_TO_XYZ` (FixedFunctionOpGPU.cpp:1902-1911 @ v2.5.2).
fn add_uvy_to_xyz(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("d")?)
        .put(" = (")
        .put(pxl)
        .put(".rgb.g == 0.) ? 0. : 1. / ")
        .put(pxl)
        .put(".rgb.g;");
    st.new_line()
        .put(st.float_decl("Y")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.b;");
    st.new_line()
        .put(pxl)
        .put(".rgb.b = (3./4.) * Y * (4. - ")
        .put(pxl)
        .put(".rgb.r - 6.6666666666666667 * ")
        .put(pxl)
        .put(".rgb.g) * d;");
    st.new_line().put(pxl).put(".rgb.r *= (9./4.) * Y * d;");
    st.new_line().put(pxl).put(".rgb.g = Y;");
    Ok(())
}

/// Port of `Add_XYZ_TO_LUV` (FixedFunctionOpGPU.cpp:1913-1929 @ v2.5.2).
fn add_xyz_to_luv(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    write_uv_denominator(pxl, st)?;
    st.new_line()
        .put(st.float_decl("u")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.r * 4. * d;");
    st.new_line()
        .put(st.float_decl("v")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.g * 9. * d;");
    st.new_line()
        .put(st.float_decl("Y")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.g;");

    st.new_line()
        .put(st.float_decl("Lstar")?)
        .put(" = ")
        .put(st.lerp(
            "1.16 * pow( max(0., Y), 1./3. ) - 0.16",
            "9.0329629629629608 * Y",
            "float(Y <= 0.008856451679)",
        ))
        .put(";");
    st.new_line()
        .put(st.float_decl("ustar")?)
        .put(" = 13. * Lstar * (u - 0.19783001);");
    st.new_line()
        .put(st.float_decl("vstar")?)
        .put(" = 13. * Lstar * (v - 0.46831999);");

    st.new_line()
        .put(pxl)
        .put(".rgb = ")
        .put(st.float3_const("Lstar", "ustar", "vstar"))
        .put(";");
    Ok(())
}

/// Port of `Add_LUV_TO_XYZ` (FixedFunctionOpGPU.cpp:1931-1948 @ v2.5.2).
fn add_luv_to_xyz(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("Lstar")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.r;");
    st.new_line()
        .put(st.float_decl("d")?)
        .put(" = (Lstar == 0.) ? 0. : 0.076923076923076927 / Lstar;");
    st.new_line()
        .put(st.float_decl("u")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.g * d + 0.19783001;");
    st.new_line()
        .put(st.float_decl("v")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb.b * d + 0.46831999;");

    st.new_line()
        .put(st.float_decl("tmp")?)
        .put(" = (Lstar + 0.16) * 0.86206896551724144;");
    st.new_line()
        .put(st.float_decl("Y")?)
        .put(" = ")
        .put(st.lerp(
            "tmp * tmp * tmp",
            "0.11070564598794539 * Lstar",
            "float(Lstar <= 0.08)",
        ))
        .put(";");

    st.new_line()
        .put(st.float_decl("dd")?)
        .put(" = (v == 0.) ? 0. : 0.25 / v;");
    st.new_line().put(pxl).put(".rgb.r = 9. * Y * u * dd;");
    st.new_line()
        .put(pxl)
        .put(".rgb.b = Y * (12. - 3. * u - 20. * v) * dd;");
    st.new_line().put(pxl).put(".rgb.g = Y;");
    Ok(())
}

/// SMPTE ST 2084's constants, in `double`.
///
/// Port of `ST_2084` (FixedFunctionOpGPU.cpp:1950-1960 @ v2.5.2).
mod st_2084 {
    pub(super) const M1: f64 = 0.25 * 2610. / 4096.;
    pub(super) const M2: f64 = 128. * 2523. / 4096.;
    pub(super) const C2: f64 = 32. * 2413. / 4096.;
    pub(super) const C3: f64 = 32. * 2392. / 4096.;
    pub(super) const C1: f64 = C3 - C2 + 1.;
}

/// Port of `Add_LIN_TO_PQ` (FixedFunctionOpGPU.cpp:1962-1977 @ v2.5.2).
fn add_lin_to_pq(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    use st_2084::{C1, C2, C3, M1, M2};

    st.new_line()
        .put(st.float3_decl("sign3")?)
        .put(" = sign(")
        .put(pxl)
        .put(".rgb);");
    st.new_line()
        .put(st.float3_decl("L")?)
        .put(" = abs(0.01 * ")
        .put(pxl)
        .put(".rgb);");
    st.new_line()
        .put(st.float3_decl("y")?)
        .put(" = pow(L, ")
        .put(st.float3_splat_f64(M1))
        .put(");");
    st.new_line()
        .put(st.float3_decl("ratpoly")?)
        .put(" = (")
        .put(st.float3_splat_f64(C1))
        .put(" + ")
        .put(C2)
        .put(" * y) / (")
        .put(st.float3_splat_f64(1.0))
        .put(" + ")
        .put(C3)
        .put(" * y);");
    st.new_line()
        .put(pxl)
        .put(".rgb = sign3 * pow(ratpoly, ")
        .put(st.float3_splat_f64(M2))
        .put(");");

    // The sign transfer here is very slightly different than in the CPU path,
    // resulting in a PQ value of 0 at 0 rather than the true value of
    // 0.836^78.84 = 7.36e-07, however, this is well below visual threshold.
    Ok(())
}

/// Port of `Add_PQ_TO_LIN` (FixedFunctionOpGPU.cpp:1979-1988 @ v2.5.2).
fn add_pq_to_lin(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    use st_2084::{C1, C2, C3, M1, M2};

    st.new_line()
        .put(st.float3_decl("sign3")?)
        .put(" = sign(")
        .put(pxl)
        .put(".rgb);");
    st.new_line()
        .put(st.float3_decl("x")?)
        .put(" = pow(abs(")
        .put(pxl)
        .put(".rgb), ")
        .put(st.float3_splat_f64(1.0 / M2))
        .put(");");
    st.new_line()
        .put(pxl)
        .put(".rgb = 100. * sign3 * pow(max(")
        .put(st.float3_splat_f64(0.0))
        .put(", x - ")
        .put(st.float3_splat_f64(C1))
        .put(") / (")
        .put(st.float3_splat_f64(C2))
        .put(" - ")
        .put(C3)
        .put(" * x), ")
        .put(st.float3_splat_f64(1.0 / M1))
        .put(");");
    Ok(())
}

/// `params[i]` as the `double` it is, or [`SHORT_PARAMS`] where upstream would read past the
/// parameters (`docs/improvements.md` U-31).
fn param(func: &FixedFunctionOpData, i: usize) -> Result<f64> {
    func.params()
        .get(i)
        .copied()
        .ok_or_else(|| Exception::new(SHORT_PARAMS))
}

/// The gamma-log curve's parameters, the log base folded into the log slope.
struct GammaLog {
    mirror_pt: f64,
    break_pt: f64,
    gamma_seg_power: f64,
    gamma_seg_slope: f64,
    gamma_seg_off: f64,
    log_seg_log_slope: f64,
    log_seg_log_off: f64,
    log_seg_lin_slope: f64,
    log_seg_lin_off: f64,
}

impl GammaLog {
    /// Port of the parameters' reading in `Add_LIN_TO_GAMMA_LOG` and `Add_GAMMA_LOG_TO_LIN`
    /// (FixedFunctionOpGPU.cpp:1995-2005, 2043-2053 @ v2.5.2).
    fn new(func: &FixedFunctionOpData) -> Result<GammaLog> {
        let p = |i| param(func, i);
        // Get parameters, baking the log base conversion into 'logSlope'.
        let log_seg_base = p(5)?;
        Ok(GammaLog {
            mirror_pt: p(0)?,
            break_pt: p(1)?,
            gamma_seg_power: p(2)?,
            gamma_seg_slope: p(3)?,
            gamma_seg_off: p(4)?,
            log_seg_log_slope: p(6)? / log_seg_base.ln(),
            log_seg_log_off: p(7)?,
            log_seg_lin_slope: p(8)?,
            log_seg_lin_off: p(9)?,
        })
    }
}

/// The gamma segment subtracts its offset here where the CPU renderer adds it (upstream's,
/// `docs/improvements.md` I-85).
///
/// Port of `Add_LIN_TO_GAMMA_LOG` (FixedFunctionOpGPU.cpp:1990-2036 @ v2.5.2).
fn add_lin_to_gamma_log(pxl: &[u8], st: &GpuShaderText, func: &FixedFunctionOpData) -> Result<()> {
    let g = GammaLog::new(func)?;

    st.new_line()
        .put(st.float3_decl("mirrorin")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb - ")
        .put(st.float3_splat_f64(g.mirror_pt))
        .put(";");
    st.new_line()
        .put(st.float3_decl("sign3")?)
        .put(" = sign(mirrorin);");
    st.new_line()
        .put(st.float3_decl("E")?)
        .put(" = abs(mirrorin) + ")
        .put(st.float3_splat_f64(g.mirror_pt))
        .put(";");
    st.new_line()
        .put(st.float3_decl("isAboveBreak")?)
        .put(" = ")
        .put(st.float3_greater_than("E", st.float3_splat_f64(g.break_pt)))
        .put(";");
    st.new_line()
        .put(st.float3_decl("isAtOrBelowBreak")?)
        .put(" = ")
        .put(st.float3_splat_f32(1.0))
        .put(" - isAboveBreak;");

    st.new_line()
        .put(st.float3_decl("Ep_gamma")?)
        .put(" = ")
        .put(st.float3_splat_f64(g.gamma_seg_slope))
        .put(" * pow( E - ")
        .put(st.float3_splat_f64(g.gamma_seg_off))
        .put(", ")
        .put(st.float3_splat_f64(g.gamma_seg_power))
        .put(");");

    // Avoid NaNs by clamping log input below 1 if the branch will not be used.
    st.new_line()
        .put(st.float3_decl("Ep_clamped")?)
        .put(" = max( isAtOrBelowBreak, E * ")
        .put(st.float3_splat_f64(g.log_seg_lin_slope))
        .put(" + ")
        .put(st.float3_splat_f64(g.log_seg_lin_off))
        .put(" );");
    st.new_line()
        .put(st.float3_decl("Ep_log")?)
        .put(" = ")
        .put(st.float3_splat_f64(g.log_seg_log_slope))
        .put(" * log( Ep_clamped ) + ")
        .put(st.float3_splat_f64(g.log_seg_log_off))
        .put(";");

    // Combine log and gamma parts.
    st.new_line()
        .put(pxl)
        .put(".rgb = sign3 * (isAboveBreak * Ep_log + ( ")
        .put(st.float3_splat_f32(1.0))
        .put(" - isAboveBreak ) * Ep_gamma);");
    Ok(())
}

/// Port of `Add_GAMMA_LOG_TO_LIN` (FixedFunctionOpGPU.cpp:2038-2084 @ v2.5.2).
fn add_gamma_log_to_lin(pxl: &[u8], st: &GpuShaderText, func: &FixedFunctionOpData) -> Result<()> {
    let g = GammaLog::new(func)?;

    let prime_break = g.gamma_seg_slope * (g.break_pt + g.gamma_seg_off).powf(g.gamma_seg_power);
    let prime_mirror = g.gamma_seg_slope * (g.mirror_pt + g.gamma_seg_off).powf(g.gamma_seg_power);

    st.new_line()
        .put(st.float3_decl("mirrorin")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb - ")
        .put(st.float3_splat_f64(prime_mirror))
        .put(";");
    st.new_line()
        .put(st.float3_decl("sign3")?)
        .put(" = sign(mirrorin);");
    st.new_line()
        .put(st.float3_decl("Eprime")?)
        .put(" = abs(mirrorin) + ")
        .put(st.float3_splat_f64(prime_mirror))
        .put(";");
    st.new_line()
        .put(st.float3_decl("isAboveBreak")?)
        .put(" = ")
        .put(st.float3_greater_than("Eprime", st.float3_splat_f64(prime_break)))
        .put(";");

    // Gamma Segment.
    st.new_line()
        .put(st.float3_decl("E_gamma")?)
        .put(" = pow( Eprime * ")
        .put(st.float3_splat_f64(1.0 / g.gamma_seg_slope))
        .put(",")
        .put(st.float3_splat_f64(1.0 / g.gamma_seg_power))
        .put(") - ")
        .put(st.float3_splat_f64(g.gamma_seg_off))
        .put(";");

    // Log Segment.
    st.new_line()
        .put(st.float3_decl("E_log")?)
        .put(" = (exp((Eprime - ")
        .put(st.float3_splat_f64(g.log_seg_log_off))
        .put(") * ")
        .put(st.float3_splat_f64(1.0 / g.log_seg_log_slope))
        .put(") - ")
        .put(st.float3_splat_f64(g.log_seg_lin_off))
        .put(") * ")
        .put(st.float3_splat_f64(1.0 / g.log_seg_lin_slope))
        .put(";");

    // Combine log and gamma parts.
    st.new_line()
        .put(pxl)
        .put(".rgb = sign3 * (isAboveBreak * E_log + ( ")
        .put(st.float3_splat_f32(1.0))
        .put(" - isAboveBreak ) * E_gamma);");
    Ok(())
}

/// The double-log curve's parameters, the log base folded into the log slopes.
struct DoubleLog {
    break1: f64,
    break2: f64,
    log_seg1_log_slope: f64,
    log_seg1_log_off: f64,
    log_seg1_lin_slope: f64,
    log_seg1_lin_off: f64,
    log_seg2_log_slope: f64,
    log_seg2_log_off: f64,
    log_seg2_lin_slope: f64,
    log_seg2_lin_off: f64,
    lin_seg_slope: f64,
    lin_seg_off: f64,
}

impl DoubleLog {
    /// Port of the parameters' reading in `Add_LIN_TO_DOUBLE_LOG` and `Add_DOUBLE_LOG_TO_LIN`
    /// (FixedFunctionOpGPU.cpp:2091-2104, 2165-2178 @ v2.5.2).
    fn new(func: &FixedFunctionOpData) -> Result<DoubleLog> {
        let p = |i| param(func, i);
        // Get parameters, baking the log base conversion into 'logSlope'.
        let base = p(0)?;
        Ok(DoubleLog {
            break1: p(1)?,
            break2: p(2)?,
            log_seg1_log_slope: p(3)? / base.ln(),
            log_seg1_log_off: p(4)?,
            log_seg1_lin_slope: p(5)?,
            log_seg1_lin_off: p(6)?,
            log_seg2_log_slope: p(7)? / base.ln(),
            log_seg2_log_off: p(8)?,
            log_seg2_lin_slope: p(9)?,
            log_seg2_lin_off: p(10)?,
            lin_seg_slope: p(11)?,
            lin_seg_off: p(12)?,
        })
    }
}

/// The three segments' masks: `isSegment1` at or below `break1`, `isSegment3` at or above
/// `break2`, `isSegment2` between.
fn write_double_log_segments(
    st: &GpuShaderText,
    pix3: &[u8],
    break1: f64,
    break2: f64,
) -> Result<()> {
    st.new_line()
        .put(st.float3_decl("isSegment1")?)
        .put(" = ")
        .put(st.float3_greater_than_equal(st.float3_splat_f64(break1), pix3))
        .put(";");
    st.new_line()
        .put(st.float3_decl("isSegment3")?)
        .put(" = ")
        .put(st.float3_greater_than_equal(pix3, st.float3_splat_f64(break2)))
        .put(";");
    st.new_line()
        .put(st.float3_decl("isSegment2")?)
        .put(" = ")
        .put(st.float3_splat_f32(1.0))
        .put(" - isSegment1 - isSegment3;");
    Ok(())
}

/// One log segment of the forward double log, clamped below 1 where it isn't used.
fn write_lin_to_log_segment(
    st: &GpuShaderText,
    pix3: &[u8],
    seg: &str,
    mask: &str,
    log_slope: f64,
    log_off: f64,
    lin_slope: f64,
    lin_off: f64,
) -> Result<()> {
    st.new_line();
    st.new_line()
        .put(st.float3_decl(seg)?)
        .put(" = ")
        .put(pix3)
        .put(" * ")
        .put(st.float3_splat_f64(lin_slope))
        .put(" + ")
        .put(st.float3_splat_f64(lin_off))
        .put(";");

    // Clamp below 1 to avoid NaNs if the branch will not be used.
    st.new_line()
        .put(seg)
        .put(" = max( ")
        .put(st.float3_splat_f64(1.0))
        .put(" - ")
        .put(mask)
        .put(", ")
        .put(seg)
        .put(" );");

    st.new_line()
        .put(seg)
        .put(" = ")
        .put(st.float3_splat_f64(log_slope))
        .put(" * log( ")
        .put(seg)
        .put(" ) + ")
        .put(st.float3_splat_f64(log_off))
        .put(";");
    Ok(())
}

/// Port of `Add_LIN_TO_DOUBLE_LOG` (FixedFunctionOpGPU.cpp:2086-2158 @ v2.5.2).
fn add_lin_to_double_log(pix: &[u8], st: &GpuShaderText, func: &FixedFunctionOpData) -> Result<()> {
    let d = DoubleLog::new(func)?;
    let pix3 = [pix, b".rgb"].concat();

    write_double_log_segments(st, &pix3, d.break1, d.break2)?;

    // Log Segment 1.
    // TODO: This segment usually handles very dark (even negative) values, thus
    // is rarely hit. As an optimization we can use "any()" to skip this in a
    // branch (needs benchmarking to see if it's worth the effort).
    write_lin_to_log_segment(
        st,
        &pix3,
        "logSeg1",
        "isSegment1",
        d.log_seg1_log_slope,
        d.log_seg1_log_off,
        d.log_seg1_lin_slope,
        d.log_seg1_lin_off,
    )?;

    // Log Segment 2.
    write_lin_to_log_segment(
        st,
        &pix3,
        "logSeg2",
        "isSegment3",
        d.log_seg2_log_slope,
        d.log_seg2_log_off,
        d.log_seg2_lin_slope,
        d.log_seg2_lin_off,
    )?;

    // Linear Segment.
    st.new_line();
    st.new_line()
        .put(st.float3_decl("linSeg")?)
        .put("= ")
        .put(st.float3_splat_f64(d.lin_seg_slope))
        .put(" * ")
        .put(&pix3)
        .put(" + ")
        .put(st.float3_splat_f64(d.lin_seg_off))
        .put(";");

    // Combine segments.
    st.new_line();
    st.new_line()
        .put(&pix3)
        .put(" = isSegment1 * logSeg1 + isSegment2 * linSeg + isSegment3 * logSeg2;");
    Ok(())
}

/// One log segment of the inverse double log.
fn write_log_to_lin_segment(
    st: &GpuShaderText,
    pix3: &[u8],
    seg: &str,
    log_slope: f64,
    log_off: f64,
    lin_slope: f64,
    lin_off: f64,
) -> Result<()> {
    st.new_line();
    st.new_line()
        .put(st.float3_decl(seg)?)
        .put(" = (")
        .put(pix3)
        .put(" - ")
        .put(st.float3_splat_f64(log_off))
        .put(") * ")
        .put(st.float3_splat_f64(1.0 / log_slope))
        .put(";");
    st.new_line()
        .put(seg)
        .put(" = (")
        .put("exp(")
        .put(seg)
        .put(") - ")
        .put(st.float3_splat_f64(lin_off))
        .put(") * ")
        .put(st.float3_splat_f64(1.0 / lin_slope))
        .put(";");
    Ok(())
}

/// `std::log(double)` as each wheel links it. The Windows wheel calls the UCRT's `log`, as Rust
/// does. The Linux wheel calls `log@GLIBC_2.2.5` (`Add_DOUBLE_LOG_TO_LIN` at 0x3711f0; it was
/// built against a glibc older than 2.29), while Rust links the current `log@GLIBC_2.29`.
///
/// Both glibc symbols run the same `__ieee754_log`. They differ only for `x < 0`: the old one
/// is the SVID/XOPEN compatibility wrapper `__log_compat` (glibc `math/w_log_compat.c`), which
/// returns `__kernel_standard(x, x, 17)`, that is `NAN`, a *positive* quiet NaN; the new one
/// returns the x86 default NaN, which is negative. (`log(+/-0)` is `-Inf` in both.) The sign
/// reaches the inverse double-log shader's break points when `linSlope * break + linOff < 0`,
/// which validation allows. The same as `log2_glibc_2_2_5` in `ocio_ops::ops::log::log_utils`.
/// The log bases go through Rust's `ln` as they are: validation leaves them positive (or NaN,
/// which both symbols propagate).
fn log_as_linked(x: f64) -> f64 {
    if cfg!(target_os = "linux") && x < 0.0 {
        f64::from_bits(0x7ff8_0000_0000_0000)
    } else {
        x.ln()
    }
}

/// Port of `Add_DOUBLE_LOG_TO_LIN` (FixedFunctionOpGPU.cpp:2160-2223 @ v2.5.2).
fn add_double_log_to_lin(pix: &[u8], st: &GpuShaderText, func: &FixedFunctionOpData) -> Result<()> {
    let d = DoubleLog::new(func)?;

    let break1_log = d.log_seg1_log_slope
        * log_as_linked(d.log_seg1_lin_slope * d.break1 + d.log_seg1_lin_off)
        + d.log_seg1_log_off;
    let break2_log = d.log_seg2_log_slope
        * log_as_linked(d.log_seg2_lin_slope * d.break2 + d.log_seg2_lin_off)
        + d.log_seg2_log_off;

    let pix3 = [pix, b".rgb"].concat();

    // This assumes the forward function is monotonically increasing.
    write_double_log_segments(st, &pix3, break1_log, break2_log)?;

    // Log Segment 1.
    // TODO: This segment usually handles very dark (even negative) values, thus
    // is rarely hit. As an optimization we can use "any()" to skip this in a
    // branch (needs benchmarking to see if it's worth the effort).
    write_log_to_lin_segment(
        st,
        &pix3,
        "logSeg1",
        d.log_seg1_log_slope,
        d.log_seg1_log_off,
        d.log_seg1_lin_slope,
        d.log_seg1_lin_off,
    )?;

    // Log Segment 2.
    write_log_to_lin_segment(
        st,
        &pix3,
        "logSeg2",
        d.log_seg2_log_slope,
        d.log_seg2_log_off,
        d.log_seg2_lin_slope,
        d.log_seg2_lin_off,
    )?;

    // Linear Segment.
    st.new_line();
    st.new_line()
        .put(st.float3_decl("linSeg")?)
        .put(" = (")
        .put(&pix3)
        .put(" - ")
        .put(st.float3_splat_f64(d.lin_seg_off))
        .put(") * ")
        .put(st.float3_splat_f64(1.0 / d.lin_seg_slope))
        .put(";");

    // Combine segments.
    st.new_line();
    st.new_line()
        .put(&pix3)
        .put(" = isSegment1 * logSeg1 + isSegment2 * linSeg + isSegment3 * logSeg2;");
    Ok(())
}

//
// ACES 2.0
//

/// `<resource prefix>_<name>`, with every `__` replaced by `_`: how the ACES 2.0 shaders name
/// their textures and helper functions (`name` ends with the resource index).
///
/// Port of the names' reservation in `_Add_Reach_table`, `_Add_Toe_func` and the other
/// helpers (FixedFunctionOpGPU.cpp:511-518, 597-606 @ v2.5.2).
fn aces2_resource_name(shader_creator: &GpuShaderDesc, name: &str) -> Vec<u8> {
    let mut res = [shader_creator.resource_prefix(), b"_", name.as_bytes()].concat();
    // Note: Remove potentially problematic double underscores from GLSL resource names.
    replace_in_place(&mut res, b"__", b"_");
    res
}

/// The primaries of the parameters from `first` on (red, green, blue and white x and y), each
/// narrowed to `float` as upstream reads them.
fn aces2_primaries(func: &FixedFunctionOpData, first: usize) -> Result<Primaries> {
    let c = |i: usize| -> Result<Chromaticities> {
        Ok(Chromaticities::new(
            f64::from(param_f32(func, first + 2 * i)?),
            f64::from(param_f32(func, first + 2 * i + 1)?),
        ))
    };
    Ok(Primaries::new(c(0)?, c(1)?, c(2)?, c(3)?))
}

/// The hue, the pixel's blue, wrapped into [0, 360).
///
/// Port of `_Add_WrapHueChannel_Shader` (FixedFunctionOpGPU.cpp:352-361 @ v2.5.2).
fn add_wrap_hue_channel_shader(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    st.new_line()
        .put(st.float_decl("hwrap")?)
        .put(" = ")
        .put(pxl)
        .put(".b;");
    st.new_line()
        .put("hwrap = hwrap - floor(hwrap / 360.0) * 360.0;");
    st.new_line()
        .put("hwrap = (hwrap < 0.0) ? hwrap + 360.0 : hwrap;");
    st.new_line().put(pxl).put(".b = hwrap;");
    Ok(())
}

/// The hue in radians, and its cosine and sine.
///
/// Port of `_Add_SinCos_Shader` (FixedFunctionOpGPU.cpp:363-371 @ v2.5.2).
fn add_sin_cos_shader(pxl: &[u8], st: &GpuShaderText) -> Result<()> {
    #[allow(clippy::approx_constant)] // Upstream's literal.
    const PI: f32 = 3.14159265358979;
    st.new_line()
        .put(st.float_decl("h_rad")?)
        .put(" = ")
        .put(pxl)
        .put(".b * ")
        .put(PI / 180.0)
        .put(";");
    st.new_line()
        .put(st.float_decl("cos_hr")?)
        .put(" = cos(h_rad);");
    st.new_line()
        .put(st.float_decl("sin_hr")?)
        .put(" = sin(h_rad);");
    Ok(())
}

/// Port of `_Add_RGB_to_Aab_Shader` (FixedFunctionOpGPU.cpp:374-393 @ v2.5.2).
fn add_rgb_to_aab_shader(pxl: &[u8], st: &GpuShaderText, p: &JMhParams) -> Result<()> {
    st.new_line().put("{");
    st.indent();

    let pxl_rgb = [pxl, b".rgb"].concat();
    st.new_line()
        .put(st.float3_decl("lms")?)
        .put(" = ")
        .put(st.mat3f_mul_f32(&p.matrix_rgb_to_cam16_c, &pxl_rgb)?)
        .put(";");

    st.new_line()
        .put(st.float3_decl("F_L_v")?)
        .put(" = pow(abs(lms), ")
        .put(st.float3_splat_f32(0.42))
        .put(");");
    st.new_line()
        .put(st.float3_decl("rgb_a")?)
        .put(" = (sign(lms) * F_L_v) / ( ")
        .put(CAM_NL_OFFSET)
        .put(" + F_L_v);");

    st.new_line()
        .put("Aab = ")
        .put(st.mat3f_mul_f32(&p.matrix_cone_response_to_aab, "rgb_a.rgb")?)
        .put(";");

    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// Port of `_Add_Aab_to_JMh_Shader` (FixedFunctionOpGPU.cpp:395-429 @ v2.5.2).
fn add_aab_to_jmh_shader(st: &GpuShaderText, p: &JMhParams) -> Result<()> {
    st.new_line().put("{");
    st.indent();

    st.new_line().put("if (Aab.r <= 0.0)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("JMh.rgb = ")
        .put(st.float3_splat_f64(0.0))
        .put(";");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put("else");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put(st.float_decl("J")?)
        .put(" = ")
        .put(J_SCALE)
        .put(" * pow(Aab.r, ")
        .put(p.cz)
        .put(");");

    st.new_line()
        .put(st.float_decl("M")?)
        .put(" = (J == 0.0) ? 0.0 : sqrt(Aab.g * Aab.g + Aab.b * Aab.b);");

    #[allow(clippy::approx_constant)] // Upstream's literal.
    const PI: f64 = 3.14159265358979;
    st.new_line()
        .put(st.float_decl("h")?)
        .put(" = (Aab.g == 0.0) ? 0.0 : ")
        .put(st.atan2("Aab.b", "Aab.g"))
        .put(" * ")
        .put(180.0 / PI)
        .put(";");
    st.new_line().put("h = h - floor(h / 360.0) * 360.0;");
    st.new_line().put("h = (h < 0.0) ? h + 360.0 : h;");

    st.new_line()
        .put("JMh.rgb = ")
        .put(st.float3_const("J", "M", "h"))
        .put(";");
    st.dedent();
    st.new_line().put("}");

    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// Port of `_Add_RGB_to_JMh_Shader` (FixedFunctionOpGPU.cpp:431-452 @ v2.5.2).
fn add_rgb_to_jmh_shader_(pxl: &[u8], st: &GpuShaderText, p: &JMhParams) -> Result<()> {
    // TODO: leaky abstraction should really be explicit functions
    st.new_line().put(st.float3_decl("JMh")?).put(";");
    // TODO: leaky abstraction should really be explicit functions
    st.new_line().put(st.float3_decl("Aab")?).put(";");

    st.new_line().put("{");
    st.indent();

    add_rgb_to_aab_shader(pxl, st, p)?;
    add_aab_to_jmh_shader(st, p)?;

    st.new_line().put(pxl).put(".rgb = JMh;");

    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// Port of `_Add_JMh_to_Aab_Shader` (FixedFunctionOpGPU.cpp:454-471 @ v2.5.2).
fn add_jmh_to_aab_shader(st: &GpuShaderText, p: &JMhParams) {
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put("Aab.r = pow(JMh.r * ")
        .put(1.0f32 / J_SCALE)
        .put(", ")
        .put(p.inv_cz)
        .put(");");
    st.new_line().put("Aab.g = JMh.g * cos_hr;");
    st.new_line().put("Aab.b = JMh.g * sin_hr;");

    st.dedent();
    st.new_line().put("}");
}

/// Port of `_Add_Aab_to_RGB_Shader` (FixedFunctionOpGPU.cpp:473-488 @ v2.5.2).
fn add_aab_to_rgb_shader(st: &GpuShaderText, p: &JMhParams) -> Result<()> {
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float3_decl("rgb_a")?)
        .put(" = ")
        .put(st.mat3f_mul_f32(&p.matrix_aab_to_cone_response, "Aab.rgb")?)
        .put(";");
    st.new_line()
        .put(st.float3_decl("rgb_a_lim")?)
        .put(" = min( abs(rgb_a), ")
        .put(st.float3_splat_f32(0.99))
        .put(" );");
    st.new_line()
        .put(st.float3_decl("lms")?)
        .put(" = sign(rgb_a) * pow( ")
        .put(CAM_NL_OFFSET)
        .put(" * rgb_a_lim / (1.0f - rgb_a_lim), ")
        .put(st.float3_splat_f32(1.0f32 / 0.42))
        .put(");");
    st.new_line()
        .put("JMh.rgb = ")
        .put(st.mat3f_mul_f32(&p.matrix_cam16_c_to_rgb, "lms")?)
        .put(";");

    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// Port of `_Add_JMh_to_RGB_Shader` (FixedFunctionOpGPU.cpp:490-503 @ v2.5.2).
fn add_jmh_to_rgb_shader_(pxl: &[u8], st: &GpuShaderText, p: &JMhParams) -> Result<()> {
    st.new_line()
        .put(st.float3_decl("JMh")?)
        .put(" = ")
        .put(pxl)
        .put(".rgb;");
    st.new_line().put(st.float3_decl("Aab")?).put(";");
    add_jmh_to_aab_shader(st, p);
    add_aab_to_rgb_shader(st, p)?;

    st.new_line().put(pxl).put(".rgb = JMh;");
    Ok(())
}

/// The dimensions of the ACES 2.0 tables' textures: 2D in GLSL ES, or where 1D textures aren't
/// allowed.
fn aces2_texture_dimensions(shader_creator: &GpuShaderDesc) -> TextureDimensions {
    let lang = shader_creator.language();
    if lang == GpuLanguage::GlslEs1_0
        || lang == GpuLanguage::GlslEs3_0
        || !shader_creator.allow_texture_1d()
    {
        TextureDimensions::D2
    } else {
        TextureDimensions::D1
    }
}

/// Declares a table's texture in the texture declarations.
fn declare_aces2_texture(
    shader_creator: &mut GpuShaderDesc,
    name: &[u8],
    dimensions: TextureDimensions,
    binding_index: u32,
) -> Result<()> {
    let st = GpuShaderText::new(shader_creator.language());
    match dimensions {
        TextureDimensions::D1 => {
            st.declare_tex1d(name, shader_creator.descriptor_set_index(), binding_index)?;
        }
        TextureDimensions::D2 => {
            st.declare_tex2d(name, shader_creator.descriptor_set_index(), binding_index)?;
        }
    }
    shader_creator.add_to_texture_declare_shader_code(st.string());
    Ok(())
}

/// The reach table as a texture of one channel, and `<name>_sample(h)`, which interpolates it
/// linearly at a hue in degrees. Returns the name.
///
/// Port of `_Add_Reach_table` (FixedFunctionOpGPU.cpp:505-589 @ v2.5.2).
fn add_reach_table(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    table: &Table1D,
) -> Result<Vec<u8>> {
    // Reserve name.
    let name = aces2_resource_name(shader_creator, &format!("reach_m_table_{resource_index}"));

    // Determine texture dimensions.
    let dimensions = aces2_texture_dimensions(shader_creator);

    // Copy the LUT into the shaderCreator as a Texture object.
    let binding_index = shader_creator.add_texture(
        &name,
        GpuShaderText::get_sampler_name(&name),
        table_base::TOTAL_SIZE as u32,
        1,
        TextureType::RedChannel,
        dimensions,
        Interpolation::Nearest,
        &table[..],
    )?;

    // Create the texture declaration.
    declare_aces2_texture(shader_creator, &name, dimensions, binding_index)?;

    // Sampler function.
    let st = GpuShaderText::new(shader_creator.language());

    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(&name)
        .put("_sample(float h)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("i_base")?)
        .put(" = floor(h);");
    st.new_line()
        .put(st.float_decl("i_lo")?)
        .put(" = i_base + ")
        .put(st.float_keyword())
        .put("(")
        .put(table_base::BASE_INDEX as u32)
        .put(");");
    st.new_line()
        .put(st.float_decl("i_hi")?)
        .put(" = i_lo + 1.0;");

    let coord = |i: &str| {
        format!(
            "({i} + 0.5) / {} ({})",
            st.float_keyword(),
            table_base::TOTAL_SIZE
        )
    };
    match dimensions {
        TextureDimensions::D1 => {
            st.new_line()
                .put(st.float_decl("lo")?)
                .put(" = ")
                .put(st.sample_tex1d(&name, coord("i_lo"))?)
                .put(".r;");
            st.new_line()
                .put(st.float_decl("hi")?)
                .put(" = ")
                .put(st.sample_tex1d(&name, coord("i_hi"))?)
                .put(".r;");
        }
        TextureDimensions::D2 => {
            st.new_line()
                .put(st.float_decl("lo")?)
                .put(" = ")
                .put(st.sample_tex2d(&name, st.float2_const(coord("i_lo"), "0.0"))?)
                .put(".r;");
            st.new_line()
                .put(st.float_decl("hi")?)
                .put(" = ")
                .put(st.sample_tex2d(&name, st.float2_const(coord("i_hi"), "0.5"))?)
                .put(".r;");
        }
    }

    // Hardcoded single degree spacing
    st.new_line().put(st.float_decl("t")?).put(" = h - i_base;");
    st.new_line()
        .put("return ")
        .put(st.lerp("lo", "hi", "t"))
        .put(";");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The toe of the chroma compression, or its inverse, as a helper function. Returns its name.
///
/// Port of `_Add_Toe_func` (FixedFunctionOpGPU.cpp:591-633 @ v2.5.2).
fn add_toe_func(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    invert: bool,
) -> Result<Vec<u8>> {
    // Reserve name
    let direction = if invert { "_inv" } else { "_fwd" };
    let name = aces2_resource_name(shader_creator, &format!("toe{direction}{resource_index}"));

    let st = GpuShaderText::new(shader_creator.language());

    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(&name)
        .put("(float x, float limit, float k1_in, float k2_in)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("k2")?)
        .put(" = max(k2_in, 0.001);");
    st.new_line()
        .put(st.float_decl("k1")?)
        .put(" = sqrt(k1_in * k1_in + k2 * k2);");
    st.new_line()
        .put(st.float_decl("k3")?)
        .put(" = (limit + k1) / (limit + k2);");

    if invert {
        st.new_line()
            .put("return (x > limit) ? x : (x * x + k1 * x) / (k3 * (x + k2));");
    } else {
        st.new_line().put(
            "return (x > limit) ? x : 0.5 * (k3 * x - k1 + sqrt((k3 * x - k1) * (k3 * x - k1) \
             + 4.0 * k2 * k3 * x));",
        );
    }

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The tone scale of a J, or its inverse, through Y, as a helper function. Returns its name.
///
/// Port of `_Add_Tonescale_func` (FixedFunctionOpGPU.cpp:635-694 @ v2.5.2).
fn add_tonescale_func(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    invert: bool,
    p: &JMhParams,
    t: &ToneScaleParams,
) -> Result<Vec<u8>> {
    // Reserve name
    let direction = if invert { "_inv" } else { "_fwd" };
    let name = aces2_resource_name(
        shader_creator,
        &format!("tonescale{direction}{resource_index}"),
    );

    let st = GpuShaderText::new(shader_creator.language());

    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(&name)
        .put("(float J)");
    st.new_line().put("{");
    st.indent();

    // Tonescale applied in Y (convert to and from J)
    // TODO: Investigate if we can receive negative J here at all.
    // If not, abs(J) here and the sign(J) at the return may not be needed at all.
    st.new_line()
        .put(st.float_decl("A")?)
        .put(" = ")
        .put(p.a_w_j)
        .put(" * pow(abs(J) * ")
        .put(1.0f32 / J_SCALE)
        .put(", ")
        .put(p.inv_cz)
        .put(");");
    st.new_line()
        .put(st.float_decl("Y")?)
        .put(" = pow(( ")
        .put(CAM_NL_OFFSET)
        .put(" * A) / (1.0f - A), ")
        .put(1.0f64 / 0.42)
        .put(");");

    if invert {
        // Inverse Tonescale applied in Y (convert to and from J)
        st.new_line()
            .put(st.float_decl("Y_i")?)
            .put(" = Y / ")
            .put(f64::from(p.f_l_n) * f64::from(REFERENCE_LUMINANCE))
            .put(";");

        st.new_line()
            .put(st.float_decl("Z")?)
            .put(" = max(0.0, min(")
            .put(t.inverse_limit)
            .put(", Y_i));");
        st.new_line()
            .put(st.float_decl("ht")?)
            .put(" = 0.5 * (Z + sqrt(Z * (")
            .put(4.0 * f64::from(t.t_1))
            .put(" + Z)));");
        st.new_line()
            .put(st.float_decl("Yo")?)
            .put(" = ")
            .put(f64::from(p.f_l_n) * f64::from(t.s_2))
            .put(" / (pow((")
            .put(t.m_2)
            .put(" / ht), (")
            .put(1.0 / f64::from(t.g))
            .put(")) - 1.0);");

        st.new_line()
            .put(st.float_decl("F_L_Y")?)
            .put(" = pow(abs(Yo), 0.42);");
    } else {
        // Tonescale applied in Y (convert to and from J)
        st.new_line()
            .put(st.float_decl("f")?)
            .put(" = ")
            .put(t.m_2)
            .put(" * pow(Y / (Y + ")
            .put(f64::from(t.s_2) * f64::from(p.f_l_n))
            .put("), ")
            .put(t.g)
            .put(");");
        st.new_line()
            .put(st.float_decl("Y_ts")?)
            .put(" = max(0.0, f * f / (f + ")
            .put(t.t_1)
            .put("));");
        st.new_line()
            .put(st.float_decl("F_L_Y")?)
            .put(" = pow(")
            .put(f64::from(p.f_l_n) * f64::from(REFERENCE_LUMINANCE))
            .put(" * Y_ts, 0.42);");
    }

    st.new_line()
        .put(st.float_decl("J_ts")?)
        .put(" = ")
        .put(J_SCALE)
        .put(" * pow((F_L_Y / ( ")
        .put(CAM_NL_OFFSET)
        .put(" + F_L_Y)) * ")
        .put(p.inv_a_w_j)
        .put(", ")
        .put(p.cz)
        .put(");");
    st.new_line().put("return sign(J) * J_ts;");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The chroma compression's normalisation `Mnorm`, from the hue's first three harmonics.
///
/// Port of `_Add_ChromaCompressionNorm_Shader` (FixedFunctionOpGPU.cpp:696-722 @ v2.5.2).
fn add_chroma_compression_norm_shader(st: &GpuShaderText, c: &ChromaCompressParams) -> Result<()> {
    let scale = f64::from(c.chroma_compress_scale);

    // Mnorm
    st.new_line().put(st.float_decl("Mnorm")?).put(";");
    st.new_line().put("{");
    st.indent();

    // TODO: optimization: can bake weights into terms and convert dotprods to addition. /coz
    st.new_line()
        .put(st.float_decl("cos_hr2")?)
        .put(" = 2.0 * cos_hr * cos_hr - 1.0;");
    st.new_line()
        .put(st.float_decl("sin_hr2")?)
        .put(" = 2.0 * cos_hr * sin_hr;");
    st.new_line()
        .put(st.float_decl("cos_hr3")?)
        .put(" = 4.0 * cos_hr * cos_hr * cos_hr - 3.0 * cos_hr;");
    st.new_line()
        .put(st.float_decl("sin_hr3")?)
        .put(" = 3.0 * sin_hr - 4.0 * sin_hr * sin_hr * sin_hr;");
    st.new_line()
        .put(st.float3_decl("cosines")?)
        .put(" = ")
        .put(st.float3_const("cos_hr", "cos_hr2", "cos_hr3"))
        .put(";");
    st.new_line()
        .put(st.float3_decl("cosine_weights")?)
        .put(" = ")
        .put(st.float3_const_f64(11.34072 * scale, 16.46899 * scale, 7.88380 * scale))
        .put(";");
    st.new_line()
        .put(st.float3_decl("sines")?)
        .put(" = ")
        .put(st.float3_const("sin_hr", "sin_hr2", "sin_hr3"))
        .put(";");
    st.new_line()
        .put(st.float3_decl("sine_weights")?)
        .put(" = ")
        .put(st.float3_const_f64(14.66441 * scale, -6.37224 * scale, 9.19364 * scale))
        .put(";");
    st.new_line()
        .put("Mnorm = dot(cosines, cosine_weights) + dot(sines, sine_weights) + ")
        .put(77.12896 * scale)
        .put(";");

    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// The chroma compression of `J`, `M`, `h` with `J_ts` and `reachMaxM` already declared.
///
/// Port of `_Add_Tonescale_Compress_Fwd_Shader` (FixedFunctionOpGPU.cpp:724-764 @ v2.5.2).
fn add_tonescale_compress_fwd_shader_(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    resource_index: u32,
    s: &SharedCompressionParameters,
    c: &ChromaCompressParams,
) -> Result<()> {
    let toe_name = add_toe_func(shader_creator, resource_index, false)?;

    let pxl = shader_creator.pixel_name().to_vec();

    st.new_line()
        .put(st.float_decl("J")?)
        .put(" = ")
        .put(&pxl)
        .put(".r;");
    st.new_line()
        .put(st.float_decl("M")?)
        .put(" = ")
        .put(&pxl)
        .put(".g;");
    st.new_line()
        .put(st.float_decl("h")?)
        .put(" = ")
        .put(&pxl)
        .put(".b;");

    // ChromaCompress
    st.new_line().put(st.float_decl("M_cp")?).put(" = M;");

    st.new_line().put("if (M != 0.0)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("nJ")?)
        .put(" = J_ts / ")
        .put(s.limit_j_max)
        .put(";");
    st.new_line()
        .put(st.float_decl("snJ")?)
        .put(" = max(0.0, 1.0 - nJ);");

    add_chroma_compression_norm_shader(st, c)?;

    st.new_line()
        .put(st.float_decl("limit")?)
        .put(" = pow(nJ, ")
        .put(s.model_gamma_inv)
        .put(") * reachMaxM / Mnorm;");
    st.new_line()
        .put("M_cp = M * pow(J_ts / J, ")
        .put(s.model_gamma_inv)
        .put(");");
    st.new_line().put("M_cp = M_cp / Mnorm;");

    st.new_line()
        .put("M_cp = limit - ")
        .put(&toe_name)
        .put("(limit - M_cp, limit - 0.001, snJ * ")
        .put(c.sat)
        .put(", sqrt(nJ * nJ + ")
        .put(c.sat_thr)
        .put("));");
    st.new_line()
        .put("M_cp = ")
        .put(&toe_name)
        .put("(M_cp, limit, nJ * ")
        .put(c.compr)
        .put(", snJ);");
    st.new_line().put("M_cp = M_cp * Mnorm;");

    st.dedent();
    st.new_line().put("}");

    st.new_line()
        .put(&pxl)
        .put(".rgb = ")
        .put(st.float3_const("J_ts", "M_cp", "h"))
        .put(";");
    Ok(())
}

/// The inverse chroma compression of `J_ts`, `M_cp`, `h` with `J` and `reachMaxM` already
/// declared.
///
/// Port of `_Add_Tonescale_Compress_Inv_Shader` (FixedFunctionOpGPU.cpp:766-805 @ v2.5.2).
fn add_tonescale_compress_inv_shader_(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    resource_index: u32,
    s: &SharedCompressionParameters,
    c: &ChromaCompressParams,
) -> Result<()> {
    let toe_name = add_toe_func(shader_creator, resource_index, true)?;

    let pxl = shader_creator.pixel_name().to_vec();

    st.new_line()
        .put(st.float_decl("J_ts")?)
        .put(" = ")
        .put(&pxl)
        .put(".r;");
    st.new_line()
        .put(st.float_decl("M_cp")?)
        .put(" = ")
        .put(&pxl)
        .put(".g;");
    st.new_line()
        .put(st.float_decl("h")?)
        .put(" = ")
        .put(&pxl)
        .put(".b;");

    // ChromaCompress
    st.new_line().put(st.float_decl("M")?).put(" = M_cp;");

    st.new_line().put("if (M_cp != 0.0)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("nJ")?)
        .put(" = J_ts / ")
        .put(s.limit_j_max)
        .put(";");
    st.new_line()
        .put(st.float_decl("snJ")?)
        .put(" = max(0.0, 1.0 - nJ);");

    add_chroma_compression_norm_shader(st, c)?;

    st.new_line()
        .put(st.float_decl("limit")?)
        .put(" = pow(nJ, ")
        .put(s.model_gamma_inv)
        .put(") * reachMaxM / Mnorm;");

    st.new_line().put("M = M_cp / Mnorm;");
    st.new_line()
        .put("M = ")
        .put(&toe_name)
        .put("(M, limit, nJ * ")
        .put(c.compr)
        .put(", snJ);");
    st.new_line()
        .put("M = limit - ")
        .put(&toe_name)
        .put("(limit - M, limit - 0.001, snJ * ")
        .put(c.sat)
        .put(", sqrt(nJ * nJ + ")
        .put(c.sat_thr)
        .put("));");
    st.new_line().put("M = M * Mnorm;");
    st.new_line()
        .put("M = M * pow(J_ts / J, ")
        .put(-s.model_gamma_inv)
        .put(");");

    st.dedent();
    st.new_line().put("}");

    st.new_line()
        .put(&pxl)
        .put(".rgb = ")
        .put(st.float3_const("J", "M", "h"))
        .put(";");
    Ok(())
}

/// Port of `Add_RGB_to_JMh_Shader` (FixedFunctionOpGPU.cpp:1442-1465 @ v2.5.2).
fn add_rgb_to_jmh_shader(pxl: &[u8], st: &GpuShaderText, func: &FixedFunctionOpData) -> Result<()> {
    let p = init_jmh_params(&aces2_primaries(func, 0)?)?;
    add_rgb_to_jmh_shader_(pxl, st, &p)
}

/// Port of `Add_JMh_to_RGB_Shader` (FixedFunctionOpGPU.cpp:1467-1492 @ v2.5.2).
fn add_jmh_to_rgb_shader(pxl: &[u8], st: &GpuShaderText, func: &FixedFunctionOpData) -> Result<()> {
    let p = init_jmh_params(&aces2_primaries(func, 0)?)?;
    add_wrap_hue_channel_shader(pxl, st)?;
    add_sin_cos_shader(pxl, st)?;
    add_jmh_to_rgb_shader_(pxl, st, &p)
}

/// The models and parameters the tone scale's shaders use, for the peak luminance.
fn aces2_tonescale_params(
    func: &FixedFunctionOpData,
) -> Result<(
    JMhParams,
    ToneScaleParams,
    SharedCompressionParameters,
    ChromaCompressParams,
)> {
    let peak_luminance = param_f32(func, 0)?;

    let p = init_jmh_params(&aces_ap0::PRIMARIES)?;
    let t = init_tone_scale_params(peak_luminance);
    let reach_gamut = init_jmh_params(&aces_ap1::PRIMARIES)?;
    let s = init_shared_compression_params(peak_luminance, &p, &reach_gamut);
    let c = init_chroma_compress_params(peak_luminance, &t);
    Ok((p, t, s, c))
}

/// Port of `Add_Tonescale_Compress_Fwd_Shader` and `Add_Tonescale_Compress_Inv_Shader`
/// (FixedFunctionOpGPU.cpp:1494-1548 @ v2.5.2).
fn add_tonescale_compress_shader(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    func: &FixedFunctionOpData,
    invert: bool,
) -> Result<()> {
    let (p, t, s, c) = aces2_tonescale_params(func)?;

    let resource_index = shader_creator.next_resource_index();
    let pxl = shader_creator.pixel_name().to_vec();

    let reach_name = add_reach_table(shader_creator, resource_index, &s.reach_m_table)?;
    let tonescale_name = add_tonescale_func(shader_creator, resource_index, invert, &p, &t)?;

    add_wrap_hue_channel_shader(&pxl, st)?;
    add_sin_cos_shader(&pxl, st)?;

    st.new_line()
        .put(st.float_decl("reachMaxM")?)
        .put(" = ")
        .put(&reach_name)
        .put("_sample(")
        .put(&pxl)
        .put(".b);");
    st.new_line()
        .put(st.float_decl(if invert { "J" } else { "J_ts" })?)
        .put(" = ")
        .put(&tonescale_name)
        .put("(")
        .put(&pxl)
        .put(".r);");

    if invert {
        add_tonescale_compress_inv_shader_(shader_creator, st, resource_index, &s, &c)
    } else {
        add_tonescale_compress_fwd_shader_(shader_creator, st, resource_index, &s, &c)
    }
}

/// `std::to_string(float)`: `%f` on the platform's C runtime.
fn cpp_to_string(value: f32) -> String {
    to_string_f32(Crt::NATIVE, value)
}

/// The gamut's cusp table as a texture of three channels (J, M and the upper hull's gamma),
/// the hue table as a constant array, and `<name>_sample(h)`, which finds the hue's interval by
/// a binary search within the hue linearity range and interpolates the cusp linearly. Returns
/// the name.
///
/// Port of `_Add_Cusp_table` (FixedFunctionOpGPU.cpp:807-923 @ v2.5.2).
fn add_cusp_table(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    g: &GamutCompressParams,
) -> Result<Vec<u8>> {
    // Reserve name.
    let name = aces2_resource_name(
        shader_creator,
        &format!("gamut_cusp_table_{resource_index}"),
    );

    // Determine texture dimensions.
    let dimensions = aces2_texture_dimensions(shader_creator);

    // Copy the LUT into the shaderCreator as a Texture object.
    let values: Vec<f32> = g.gamut_cusp_table.iter().flatten().copied().collect();
    let binding_index = shader_creator.add_texture(
        &name,
        GpuShaderText::get_sampler_name(&name),
        table_base::TOTAL_SIZE as u32,
        1,
        TextureType::RgbChannel,
        dimensions,
        Interpolation::Nearest,
        &values,
    )?;

    // Create the texture declaration.
    declare_aces2_texture(shader_creator, &name, dimensions, binding_index)?;

    // Sampler function.
    let st = GpuShaderText::new(shader_creator.language());

    let hues_array_name = [&name[..], b"_hues_array"].concat();
    st.declare_float_array_const(&hues_array_name, &g.hue_table[..])?;

    st.new_line()
        .put(st.float3_keyword())
        .put(" ")
        .put(&name)
        .put("_sample(float h)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.int_decl("i")?)
        .put(" = ")
        .put(st.int_keyword())
        .put("(h) + ")
        .put(table_base::BASE_INDEX as u32)
        .put(";");

    st.new_line()
        .put(st.int_decl("i_lo")?)
        .put(" = ")
        .put(st.int_keyword())
        .put("(max(")
        .put(st.float_keyword())
        .put("(")
        .put(table_base::LOWER_WRAP_INDEX as u32)
        .put("), ")
        .put(st.float_keyword())
        .put("(i + ")
        .put(g.hue_linearity_search_range[0])
        .put(")));");
    st.new_line()
        .put(st.int_decl("i_hi")?)
        .put(" = ")
        .put(st.int_keyword())
        .put("(min(")
        .put(st.float_keyword())
        .put("(")
        .put(table_base::UPPER_WRAP_INDEX as u32)
        .put("), ")
        .put(st.float_keyword())
        .put("(i + ")
        .put(g.hue_linearity_search_range[1])
        .put(")));");

    st.new_line().put("while (i_lo + 1 < i_hi)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("hcur")?)
        .put(" = ")
        .put(&hues_array_name)
        .put("[i];");

    st.new_line().put("if (h > hcur)");
    st.new_line().put("{");
    st.indent();
    st.new_line().put("i_lo = i;");
    st.dedent();
    st.new_line().put("}");
    st.new_line().put("else");
    st.new_line().put("{");
    st.indent();
    st.new_line().put("i_hi = i;");
    st.dedent();
    st.new_line().put("}");
    st.new_line().put("i = (i_lo + i_hi) / 2;");

    st.dedent();
    st.new_line().put("}");

    let coord = |offset: &str| {
        format!(
            "({float}(i_hi){offset} + 0.5) / {float}({})",
            table_base::TOTAL_SIZE,
            float = st.float_keyword()
        )
    };
    match dimensions {
        TextureDimensions::D1 => {
            st.new_line()
                .put(st.float3_decl("lo")?)
                .put(" = ")
                .put(st.sample_tex1d(&name, coord(" - 1.0"))?)
                .put(".rgb;");
            st.new_line()
                .put(st.float3_decl("hi")?)
                .put(" = ")
                .put(st.sample_tex1d(&name, coord(""))?)
                .put(".rgb;");
        }
        TextureDimensions::D2 => {
            st.new_line()
                .put(st.float3_decl("lo")?)
                .put(" = ")
                .put(st.sample_tex2d(&name, st.float2_const(coord(" - 1.0"), "0.5"))?)
                .put(".rgb;");
            st.new_line()
                .put(st.float3_decl("hi")?)
                .put(" = ")
                .put(st.sample_tex2d(&name, st.float2_const(coord(""), "0.5"))?)
                .put(".rgb;");
        }
    }

    st.new_line()
        .put(st.float_decl("t")?)
        .put(" = (h - ")
        .put(&hues_array_name)
        .put("[i_hi - 1]) / (")
        .put(&hues_array_name)
        .put("[i_hi] - ")
        .put(&hues_array_name)
        .put("[i_hi - 1]);");
    st.new_line()
        .put("return ")
        .put(st.lerp("lo", "hi", "t"))
        .put(";");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The focus gain as a helper function. Returns its name.
///
/// Port of `_Add_Focus_Gain_func` (FixedFunctionOpGPU.cpp:925-971 @ v2.5.2).
fn add_focus_gain_func(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    s: &SharedCompressionParameters,
) -> Result<Vec<u8>> {
    // Reserve name
    let name = aces2_resource_name(shader_creator, &format!("get_focus_gain{resource_index}"));

    let st = GpuShaderText::new(shader_creator.language());

    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(&name)
        .put("(float J, float cuspJ)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("thr")?)
        .put(" = ")
        .put(st.lerp(
            "cuspJ",
            cpp_to_string(s.limit_j_max),
            cpp_to_string(FOCUS_GAIN_BLEND),
        ))
        .put(";");

    st.new_line().put("if (J > thr)"); // TODO threshold
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put(st.float_decl("gain")?)
        .put(" = ( ")
        .put(s.limit_j_max)
        .put(" - thr) / max(0.0001, ")
        .put(s.limit_j_max)
        .put(" - J);");
    // TODO log10(gain) but not all shading languages have log10() would log2(gain)/log2(10)
    // be better? perhaps delegate to GpuShaderText?
    st.new_line().put("gain = log(gain)/log(10.0);");
    st.new_line().put("return gain * gain + 1.0;");
    st.dedent();
    st.new_line().put("}");
    st.new_line().put("else");
    st.new_line().put("{");
    st.indent();
    st.new_line().put("return 1.0;");
    st.dedent();
    st.new_line().put("}");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The J where the compression line meets the J axis, as a helper function. Returns its name.
///
/// Port of `_Add_Solve_J_Intersect_func` (FixedFunctionOpGPU.cpp:973-1025 @ v2.5.2).
fn add_solve_j_intersect_func(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    s: &SharedCompressionParameters,
) -> Result<Vec<u8>> {
    // Reserve name
    let name = aces2_resource_name(
        shader_creator,
        &format!("solve_J_intersect{resource_index}"),
    );

    let st = GpuShaderText::new(shader_creator.language());

    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(&name)
        .put("(float J, float M, float focusJ, float slope_gain)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("M_scaled")?)
        .put(" = M / slope_gain;");
    st.new_line()
        .put(st.float_decl("a")?)
        .put(" = M_scaled / focusJ;");

    st.new_line().put("if (J < focusJ)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put(st.float_decl("b")?)
        .put(" = 1.0 - M_scaled;");
    st.new_line().put(st.float_decl("c")?).put(" = -J;");
    st.new_line()
        .put(st.float_decl("det")?)
        .put(" =  b * b - 4.f * a * c;");
    st.new_line()
        .put(st.float_decl("root")?)
        .put(" =  sqrt(det);");
    st.new_line().put("return -2.0 * c / (b + root);");
    st.dedent();
    st.new_line().put("}");
    st.new_line().put("else");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put(st.float_decl("b")?)
        .put(" = - (1.0 + M_scaled + ")
        .put(s.limit_j_max)
        .put(" * a);");
    st.new_line()
        .put(st.float_decl("c")?)
        .put(" = ")
        .put(s.limit_j_max)
        .put(" * M_scaled + J;");
    st.new_line()
        .put(st.float_decl("det")?)
        .put(" =  b * b - 4.f * a * c;");
    st.new_line()
        .put(st.float_decl("root")?)
        .put(" =  sqrt(det);");
    st.new_line().put("return -2.0 * c / (b - root);");
    st.dedent();
    st.new_line().put("}");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The gamut boundary's M along the compression line, as a helper function. Returns its name.
///
/// Port of `_Add_Find_Gamut_Boundary_Intersection_func` (FixedFunctionOpGPU.cpp:1027-1074 @
/// v2.5.2).
fn add_find_gamut_boundary_intersection_func(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    s: &SharedCompressionParameters,
) -> Result<Vec<u8>> {
    // Reserve name
    let name = aces2_resource_name(
        shader_creator,
        &format!("find_gamut_boundary_intersection{resource_index}"),
    );

    let st = GpuShaderText::new(shader_creator.language());

    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(&name)
        .put("(")
        .put(st.float2_keyword())
        .put(
            " JM_cusp, float gamma_top_inv, float gamma_bottom_inv, float J_intersect_source, \
             float J_intersect_cusp, float slope)",
        );
    st.new_line().put("{");
    st.indent();

    st.new_line().put(st.float_decl("M_boundary_lower")?).put(
        " = J_intersect_cusp * pow(J_intersect_source / J_intersect_cusp, gamma_bottom_inv) \
             / (JM_cusp.r / JM_cusp.g - slope);",
    );
    let l = s.limit_j_max;
    st.new_line()
        .put(st.float_decl("M_boundary_upper")?)
        .put(" = JM_cusp.g * (")
        .put(l)
        .put(" - J_intersect_cusp) * pow((")
        .put(l)
        .put(" - J_intersect_source) / (")
        .put(l)
        .put(" - J_intersect_cusp), gamma_top_inv) / (slope * JM_cusp.g + ")
        .put(l)
        .put(" - JM_cusp.r);");

    st.new_line().put(st.float_decl("smin")?).put(" = 0.0;");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put(st.float_decl("a")?)
        .put(" = M_boundary_lower;");
    st.new_line()
        .put(st.float_decl("b")?)
        .put(" = M_boundary_upper;");
    st.new_line()
        .put(st.float_decl("s")?)
        .put(" = ")
        .put(SMOOTH_CUSPS)
        .put(" * JM_cusp.g;");

    st.new_line()
        .put(st.float_decl("h")?)
        .put(" = max(s - abs(a - b), 0.0) / s;");
    st.new_line()
        .put("smin = min(a, b) - h * h * h * s * ")
        .put(1.0f64 / 6.0)
        .put(";");

    st.dedent();
    st.new_line().put("}");

    st.new_line().put("return smin;");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The remapping of M between the gamut and the reach boundaries, or its inverse, as a helper
/// function. Returns its name.
///
/// Port of `_Add_Compression_func` (FixedFunctionOpGPU.cpp:1076-1142 @ v2.5.2).
fn add_compression_func(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    invert: bool,
) -> Result<Vec<u8>> {
    // Reserve name
    let direction = if invert { "_inv" } else { "_fwd" };
    let name = aces2_resource_name(
        shader_creator,
        &format!("remap_M{direction}{resource_index}"),
    );

    let st = GpuShaderText::new(shader_creator.language());

    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(&name)
        .put("(float M, float gamut_boundary_M, float reach_boundary_M)");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float_decl("boundary_ratio")?)
        .put(" = gamut_boundary_M / reach_boundary_M;");
    st.new_line()
        .put(st.float_decl("proportion")?)
        .put(" = max(boundary_ratio, ")
        .put(COMPRESSION_THRESHOLD)
        .put(");");
    st.new_line()
        .put(st.float_decl("threshold")?)
        .put(" = proportion * gamut_boundary_M;");

    st.new_line()
        .put("if (proportion >= 1.0f || M <= threshold)");
    st.new_line().put("{");
    st.indent();
    st.new_line().put("return M;");
    st.dedent();
    st.new_line().put("}");
    st.new_line()
        .put(st.float_decl("m_offset")?)
        .put(" = M - threshold;");
    st.new_line()
        .put(st.float_decl("gamut_offset")?)
        .put(" = gamut_boundary_M - threshold;");
    st.new_line()
        .put(st.float_decl("reach_offset")?)
        .put(" = reach_boundary_M - threshold;");

    st.new_line()
        .put(st.float_decl("scale")?)
        .put(" = reach_offset / ((reach_offset / gamut_offset) - 1.0f);");
    st.new_line()
        .put(st.float_decl("nd")?)
        .put(" = m_offset / scale;");

    if invert {
        st.new_line().put("if (nd >= 1.0f)"); // TODO: could be done branchless?
        st.new_line().put("{");
        st.indent();
        st.new_line().put("return threshold + scale;");
        st.dedent();
        st.new_line().put("}");
        st.new_line().put("else");
        st.new_line().put("{");
        st.indent();
        st.new_line()
            .put("return threshold + scale * -(nd / (nd - 1.0f));");
        st.dedent();
        st.new_line().put("}");
    } else {
        st.new_line()
            .put("return threshold + scale * nd / (1.0f + nd);");
    }

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The names of the gamut compression's helper functions.
struct GamutCompressNames<'a> {
    get_focus_gain: &'a [u8],
    find_gamut_boundary_intersection: &'a [u8],
    compression: &'a [u8],
    solve_j_intersect: &'a [u8],
}

/// The gamut compression of a JMh, as a helper function. Returns its name. Unlike the CPU, it
/// compresses a J at or below 0 (`docs/improvements.md` I-86).
///
/// Port of `_Add_Compress_Gamut_func` (FixedFunctionOpGPU.cpp:1144-1225 @ v2.5.2).
fn add_compress_gamut_func(
    shader_creator: &mut GpuShaderDesc,
    resource_index: u32,
    s: &SharedCompressionParameters,
    g: &GamutCompressParams,
    names: &GamutCompressNames<'_>,
) -> Result<Vec<u8>> {
    // Reserve name
    let name = aces2_resource_name(shader_creator, &format!("gamut_compress{resource_index}"));

    let st = GpuShaderText::new(shader_creator.language());
    let l = s.limit_j_max;

    st.new_line()
        .put(st.float3_keyword())
        .put(" ")
        .put(&name)
        .put("(")
        .put(st.float3_keyword())
        .put(" JMh, float Jx, ")
        .put(st.float3_keyword())
        .put(" JMGcusp, float reachMaxM)");
    st.new_line().put("{");
    st.indent();

    st.new_line().put(st.float_decl("J")?).put(" = JMh.r;");
    st.new_line().put(st.float_decl("M")?).put(" = JMh.g;");
    st.new_line().put(st.float_decl("h")?).put(" = JMh.b;");

    st.new_line().put("if (M <= 0.0 || J > ").put(l).put(")");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return ")
        .put(st.float3_const("J", "0.0", "h"))
        .put(";");
    st.dedent();
    st.new_line().put("}");
    st.new_line().put("else");
    st.new_line().put("{");
    st.indent();

    st.new_line()
        .put(st.float2_decl("JMcusp")?)
        .put(" = JMGcusp.rg;");

    st.new_line()
        .put(st.float_decl("focusJ")?)
        .put(" = ")
        .put(st.lerp(
            "JMcusp.r",
            cpp_to_string(g.mid_j),
            format!(
                "min(1.0, {} - (JMcusp.r / {}",
                cpp_to_string(CUSP_MID_BLEND),
                cpp_to_string(l)
            ),
        ))
        .put("));");
    st.new_line()
        .put(st.float_decl("slope_gain")?)
        .put(" = ")
        .put(l * g.focus_dist)
        .put(" * ")
        .put(names.get_focus_gain)
        .put("(Jx, JMcusp.r);");
    st.new_line()
        .put(st.float_decl("J_intersect_source")?)
        .put(" = ")
        .put(names.solve_j_intersect)
        .put("(JMh.r, JMh.g, focusJ, slope_gain);");
    st.new_line()
        .put(st.float_decl("gamut_slope")?)
        .put(" = (J_intersect_source < focusJ) ? J_intersect_source : (")
        .put(l)
        .put(" - J_intersect_source);");
    st.new_line()
        .put("gamut_slope = gamut_slope * (J_intersect_source - focusJ) / (focusJ * slope_gain);");

    st.new_line()
        .put(st.float_decl("gamma_top_inv")?)
        .put(" = JMGcusp.b;");
    st.new_line()
        .put(st.float_decl("gamma_bottom_inv")?)
        .put(" = ")
        .put(g.lower_hull_gamma_inv)
        .put(";"); // TODO move to where it is used

    st.new_line()
        .put(st.float_decl("J_intersect_cusp")?)
        .put(" = ")
        .put(names.solve_j_intersect)
        .put("(JMcusp.r, JMcusp.g, focusJ, slope_gain);");
    st.new_line()
        .put(st.float_decl("gamutBoundaryM")?)
        .put(" = ")
        .put(names.find_gamut_boundary_intersection)
        .put(
            "(JMcusp, gamma_top_inv, gamma_bottom_inv, J_intersect_source, J_intersect_cusp, \
             gamut_slope);",
        );

    st.new_line().put("if (gamutBoundaryM <= 0.0)");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("return ")
        .put(st.float3_const("J", "0.0", "h"))
        .put(";");
    st.dedent();
    st.new_line().put("}");

    st.new_line()
        .put(st.float_decl("reachBoundaryM")?)
        .put(" = ")
        .put(l)
        .put(" * pow(J_intersect_source / ")
        .put(l)
        .put(",  ")
        .put(s.model_gamma_inv)
        .put(");");
    st.new_line()
        .put("reachBoundaryM = reachBoundaryM / ((")
        .put(l)
        .put(" / reachMaxM) - gamut_slope);");

    st.new_line()
        .put(st.float_decl("remapped_M")?)
        .put(" = ")
        .put(names.compression)
        .put("(M, gamutBoundaryM, reachBoundaryM);");
    st.new_line()
        .put(st.float_decl("remapped_J")?)
        .put(" = J_intersect_source + remapped_M * gamut_slope;");

    st.new_line()
        .put("return ")
        .put(st.float3_const("remapped_J", "remapped_M", "h"))
        .put(";");

    st.dedent();
    st.new_line().put("}");

    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());

    Ok(name)
}

/// The gamut compression's helper functions and its use on the pixel, with `reachMaxM`
/// already declared, or the inverse's.
///
/// Port of `_Add_Gamut_Compress_Fwd_Shader` and `_Add_Gamut_Compress_Inv_Shader`
/// (FixedFunctionOpGPU.cpp:1227-1285 @ v2.5.2).
fn add_gamut_compress_shader_(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    resource_index: u32,
    s: &SharedCompressionParameters,
    g: &GamutCompressParams,
    invert: bool,
) -> Result<()> {
    let cusp_name = add_cusp_table(shader_creator, resource_index, g)?;
    let get_focus_gain = add_focus_gain_func(shader_creator, resource_index, s)?;
    let solve_j_intersect = add_solve_j_intersect_func(shader_creator, resource_index, s)?;
    let find_gamut_boundary_intersection =
        add_find_gamut_boundary_intersection_func(shader_creator, resource_index, s)?;
    let compression = add_compression_func(shader_creator, resource_index, invert)?;
    let names = GamutCompressNames {
        get_focus_gain: &get_focus_gain,
        find_gamut_boundary_intersection: &find_gamut_boundary_intersection,
        compression: &compression,
        solve_j_intersect: &solve_j_intersect,
    };
    let gamut_compress_name =
        add_compress_gamut_func(shader_creator, resource_index, s, g, &names)?;

    let pxl = shader_creator.pixel_name().to_vec();

    st.new_line()
        .put(st.float3_decl("JMGcusp")?)
        .put(" = ")
        .put(&cusp_name)
        .put("_sample(")
        .put(&pxl)
        .put(".b);");

    let compress = |jx: &str| {
        [
            &gamut_compress_name[..],
            b"(",
            &pxl[..],
            b".rgb, ",
            jx.as_bytes(),
            b", JMGcusp, reachMaxM)",
        ]
        .concat()
    };
    if !invert {
        st.new_line()
            .put(&pxl)
            .put(".rgb = ")
            .put(compress(&format!("{}.r", String::from_utf8_lossy(&pxl))))
            .put(";");
        return Ok(());
    }

    st.new_line()
        .put(st.float_decl("Jx")?)
        .put(" = ")
        .put(&pxl)
        .put(".r;");
    st.new_line()
        .put(st.float3_decl("unCompressedJMh")?)
        .put(";");

    // Analytic inverse below threshold
    st.new_line()
        .put("if (Jx <= ")
        .put(st.lerp(
            "JMGcusp.r",
            cpp_to_string(s.limit_j_max),
            cpp_to_string(FOCUS_GAIN_BLEND),
        ))
        .put(")");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put("unCompressedJMh = ")
        .put(compress("Jx"))
        .put(";");
    st.dedent();
    st.new_line().put("}");
    // Approximation above threshold
    st.new_line().put("else");
    st.new_line().put("{");
    st.indent();
    st.new_line().put("Jx = ").put(compress("Jx")).put(".r;");
    st.new_line()
        .put("unCompressedJMh = ")
        .put(compress("Jx"))
        .put(";");
    st.dedent();
    st.new_line().put("}");

    st.new_line().put(&pxl).put(".rgb = unCompressedJMh;");
    Ok(())
}

/// The models and parameters the gamut compression's shaders use, for the peak luminance and
/// the limiting primaries of the parameters.
#[allow(clippy::type_complexity)]
fn aces2_gamut_params(
    func: &FixedFunctionOpData,
) -> Result<(
    JMhParams,
    JMhParams,
    ToneScaleParams,
    SharedCompressionParameters,
    GamutCompressParams,
)> {
    let peak_luminance = param_f32(func, 0)?;
    let lim_primaries = aces2_primaries(func, 1)?;

    let p_in = init_jmh_params(&aces_ap0::PRIMARIES)?;
    let p_lim = init_jmh_params(&lim_primaries)?;
    let t = init_tone_scale_params(peak_luminance);
    let reach_gamut = init_jmh_params(&aces_ap1::PRIMARIES)?;
    let s = init_shared_compression_params(peak_luminance, &p_in, &reach_gamut);
    let g = init_gamut_compress_params(peak_luminance, &p_in, &p_lim, &t, &s, &reach_gamut)?;
    Ok((p_in, p_lim, t, s, g))
}

/// Port of `Add_Gamut_Compress_Fwd_Shader` and `Add_Gamut_Compress_Inv_Shader`
/// (FixedFunctionOpGPU.cpp:1550-1634 @ v2.5.2).
fn add_gamut_compress_shader(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    func: &FixedFunctionOpData,
    invert: bool,
) -> Result<()> {
    let (_, _, _, s, g) = aces2_gamut_params(func)?;

    let resource_index = shader_creator.next_resource_index();
    let pxl = shader_creator.pixel_name().to_vec();

    let reach_name = add_reach_table(shader_creator, resource_index, &s.reach_m_table)?;

    add_wrap_hue_channel_shader(&pxl, st)?;
    add_sin_cos_shader(&pxl, st)?;

    st.new_line()
        .put(st.float_decl("reachMaxM")?)
        .put(" = ")
        .put(&reach_name)
        .put("_sample(")
        .put(&pxl)
        .put(".b);");

    add_gamut_compress_shader_(shader_creator, st, resource_index, &s, &g, invert)
}

/// A block of the output transform: an empty line, the comment, an empty line.
fn write_aces2_section(st: &GpuShaderText, comment: &str) {
    st.new_line().put("");
    st.new_line().put(comment);
    st.new_line().put("");
}

/// Port of `Add_ACES_OutputTransform_Fwd_Shader` (FixedFunctionOpGPU.cpp:1287-1366 @ v2.5.2).
fn add_aces_output_transform_fwd_shader(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    func: &FixedFunctionOpData,
) -> Result<()> {
    let (p_in, p_lim, t, s, g) = aces2_gamut_params(func)?;
    let c = init_chroma_compress_params(param_f32(func, 0)?, &t);

    let resource_index = shader_creator.next_resource_index();

    let reach_name = add_reach_table(shader_creator, resource_index, &s.reach_m_table)?;
    let tonescale_name_fwd = add_tonescale_func(shader_creator, resource_index, false, &p_in, &t)?;
    let pxl = shader_creator.pixel_name().to_vec();

    write_aces2_section(st, "// Add RGB to JMh");
    add_rgb_to_jmh_shader_(&pxl, st, &p_in)?;
    add_sin_cos_shader(&pxl, st)?;

    write_aces2_section(st, "// Add ToneScale and ChromaCompress (fwd)");

    st.new_line()
        .put(st.float_decl("J_ts")?)
        .put(" = ")
        .put(&tonescale_name_fwd)
        .put("(")
        .put(&pxl)
        .put(".r);");

    st.new_line().put("// Sample tables (fwd)");
    st.new_line()
        .put(st.float_decl("reachMaxM")?)
        .put(" = ")
        .put(&reach_name)
        .put("_sample(")
        .put(&pxl)
        .put(".b);");

    st.new_line().put("");

    st.new_line().put("{");
    st.indent();
    add_tonescale_compress_fwd_shader_(shader_creator, st, resource_index, &s, &c)?;
    st.dedent();
    st.new_line().put("}");

    write_aces2_section(st, "// Add GamutCompress (fwd)");
    st.new_line().put("{");
    st.indent();
    add_gamut_compress_shader_(shader_creator, st, resource_index, &s, &g, false)?;
    st.dedent();
    st.new_line().put("}");

    write_aces2_section(st, "// Add JMh to RGB");
    st.new_line().put("{");
    st.indent();
    add_jmh_to_rgb_shader_(&pxl, st, &p_lim)?;
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// Port of `Add_ACES_OutputTransform_Inv_Shader` (FixedFunctionOpGPU.cpp:1368-1440 @ v2.5.2).
fn add_aces_output_transform_inv_shader(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    func: &FixedFunctionOpData,
) -> Result<()> {
    let (p_in, p_lim, t, s, g) = aces2_gamut_params(func)?;
    let c = init_chroma_compress_params(param_f32(func, 0)?, &t);

    let resource_index = shader_creator.next_resource_index();
    let pxl = shader_creator.pixel_name().to_vec();

    let reach_name = add_reach_table(shader_creator, resource_index, &s.reach_m_table)?;
    let tonescale_name_inv = add_tonescale_func(shader_creator, resource_index, true, &p_in, &t)?;

    write_aces2_section(st, "// Add RGB to JMh");
    add_rgb_to_jmh_shader_(&pxl, st, &p_lim)?;
    add_sin_cos_shader(&pxl, st)?;

    st.new_line()
        .put(st.float_decl("reachMaxM")?)
        .put(" = ")
        .put(&reach_name)
        .put("_sample(")
        .put(&pxl)
        .put(".b);");
    write_aces2_section(st, "// Add GamutCompress (inv)");
    st.new_line().put("{");
    st.indent();
    add_gamut_compress_shader_(shader_creator, st, resource_index, &s, &g, true)?;
    st.dedent();
    st.new_line().put("}");

    write_aces2_section(st, "// Add ToneScale and ChromaCompress (inv)");
    st.new_line()
        .put(st.float_decl("J")?)
        .put(" = ")
        .put(&tonescale_name_inv)
        .put("(")
        .put(&pxl)
        .put(".r);");
    st.new_line().put("{");
    st.indent();
    add_tonescale_compress_inv_shader_(shader_creator, st, resource_index, &s, &c)?;
    st.dedent();
    st.new_line().put("}");

    write_aces2_section(st, "// Add JMh to RGB");
    st.new_line().put("{");
    st.indent();
    add_jmh_to_rgb_shader_(&pxl, st, &p_in)?;
    st.dedent();
    st.new_line().put("}");
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
/// style's shader in a block.
///
/// Port of `GetFixedFunctionGPUProcessingText` (FixedFunctionOpGPU.cpp:2233-2491 @ v2.5.2).
pub fn get_fixed_function_gpu_processing_text(
    shader_creator: &mut GpuShaderDesc,
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
        Rec2100SurroundFwd => add_rec2100_surround_shader(&pxl, st, param_f32(func, 0)?, true)?,
        Rec2100SurroundInv => add_rec2100_surround_shader(&pxl, st, param_f32(func, 0)?, false)?,
        RgbToHsv => add_rgb_to_hsv(&pxl, st)?,
        RgbToHsyLog => add_rgb_to_hsy(&pxl, st, Hsy::Log)?,
        RgbToHsyLin => add_rgb_to_hsy(&pxl, st, Hsy::Lin)?,
        RgbToHsyVid => add_rgb_to_hsy(&pxl, st, Hsy::Vid)?,
        HsyLogToRgb => add_hsy_to_rgb(&pxl, st, Hsy::Log)?,
        HsyLinToRgb => add_hsy_to_rgb(&pxl, st, Hsy::Lin)?,
        HsyVidToRgb => add_hsy_to_rgb(&pxl, st, Hsy::Vid)?,
        HsvToRgb => add_hsv_to_rgb(&pxl, st)?,
        XyzToXyy => add_xyz_to_xyy(&pxl, st)?,
        XyyToXyz => add_xyy_to_xyz(&pxl, st)?,
        XyzToUvy => add_xyz_to_uvy(&pxl, st)?,
        UvyToXyz => add_uvy_to_xyz(&pxl, st)?,
        XyzToLuv => add_xyz_to_luv(&pxl, st)?,
        LuvToXyz => add_luv_to_xyz(&pxl, st)?,
        LinToPq => add_lin_to_pq(&pxl, st)?,
        PqToLin => add_pq_to_lin(&pxl, st)?,
        LinToGammaLog => add_lin_to_gamma_log(&pxl, st, func)?,
        GammaLogToLin => add_gamma_log_to_lin(&pxl, st, func)?,
        LinToDoubleLog => add_lin_to_double_log(&pxl, st, func)?,
        DoubleLogToLin => add_double_log_to_lin(&pxl, st, func)?,
        AcesRgbToJmh20 => add_rgb_to_jmh_shader(&pxl, st, func)?,
        AcesJmhToRgb20 => add_jmh_to_rgb_shader(&pxl, st, func)?,
        AcesTonescaleCompress20Fwd => {
            add_tonescale_compress_shader(shader_creator, st, func, false)?
        }
        AcesTonescaleCompress20Inv => {
            add_tonescale_compress_shader(shader_creator, st, func, true)?
        }
        AcesGamutCompress20Fwd => add_gamut_compress_shader(shader_creator, st, func, false)?,
        AcesGamutCompress20Inv => add_gamut_compress_shader(shader_creator, st, func, true)?,
        AcesOutputTransform20Fwd => add_aces_output_transform_fwd_shader(shader_creator, st, func)?,
        AcesOutputTransform20Inv => add_aces_output_transform_inv_shader(shader_creator, st, func)?,
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

    /// The shader of `data`, or the writer's error.
    fn shader_of(data: &FixedFunctionOpData) -> (Result<()>, Vec<u8>) {
        let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl1_2);
        let result = get_fixed_function_gpu_shader_program(&mut desc, data);
        (result, desc.shader_text().to_vec())
    }

    /// U-31: where upstream's shader reads past the parameters (data that doesn't validate,
    /// which no processor extracts), the port refuses: each style that reads parameters, with
    /// one fewer than it reads.
    #[test]
    fn short_params_are_refused() {
        use FixedFunctionOpStyle::*;
        let gamut = vec![1.2, 1.2, 1.2, 0.5, 0.5, 0.5, 1.2];
        let gamma_log = vec![0.0, 0.25, 0.5, 1.0, 0.0, 2.5, 0.2, 0.8, 1.0, -0.07];
        let double_log = vec![
            10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
        ];
        // ACES 2.0: a peak luminance and limiting primaries (P3-D65 at 1000 nits), AP0's.
        let peak_primaries = vec![
            1000.0, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.3127, 0.3290,
        ];
        let ap0 = vec![
            0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
        ];
        for (style, params) in [
            (AcesGamutComp13Fwd, &gamut),
            (AcesGamutComp13Inv, &gamut),
            (Rec2100SurroundFwd, &vec![0.78]),
            (Rec2100SurroundInv, &vec![0.78]),
            (LinToGammaLog, &gamma_log),
            (GammaLogToLin, &gamma_log),
            (LinToDoubleLog, &double_log),
            (DoubleLogToLin, &double_log),
            (AcesOutputTransform20Fwd, &peak_primaries),
            (AcesOutputTransform20Inv, &peak_primaries),
            (AcesGamutCompress20Fwd, &peak_primaries),
            (AcesGamutCompress20Inv, &peak_primaries),
            (AcesTonescaleCompress20Fwd, &vec![1000.0]),
            (AcesTonescaleCompress20Inv, &vec![1000.0]),
            (AcesRgbToJmh20, &ap0),
            (AcesJmhToRgb20, &ap0),
        ] {
            let mut data = FixedFunctionOpData::with_params(style, params.clone()).unwrap();
            assert!(shader_of(&data).0.is_ok(), "{style:?}");
            data.set_params(params[..params.len() - 1].to_vec());
            let (result, text) = shader_of(&data);
            assert_eq!(result.unwrap_err().message(), SHORT_PARAMS, "{style:?}");
            assert!(text.is_empty(), "{style:?}");
        }
    }
}
