// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOpGPU.h` and
//! `GradingHueCurveOpGPU.cpp` @ v2.5.2, the GradingHueCurve op's GPU writer, with the inverse
//! hue curve evaluation's shader text of `GradingBSplineCurveImpl::AddShaderEvalRevHue`
//! (src/OpenColorIO/ops/gradingrgbcurve/GradingBSplineCurve.cpp:1168-1246).
//!
//! The eight curves' knots and coefficients (`KnotsCoefs`) go into the shader as constant
//! arrays in op-specific helper functions, or, for a dynamic op, as uniforms bound to a copy of
//! the op's dynamic property that the shader description holds, so the application can change
//! the curves after the extraction. The helpers evaluate one curve forward
//! (`evalBSplineCurve`, the RGB curves' text), and for the inverse direction backward
//! (`evalBSplineCurveRev`) and backward as a hue curve (`evalBSplineCurveRevHue`). The op's
//! block converts RGB to HSY with the FixedFunction writer of the style (unless the op
//! doesn't convert), applies the curves (luma in the grading log for the linear style), and
//! converts back; a draw-curve-only op evaluates the hue-sat curve on each channel instead.

use std::sync::Arc;

use ocio_ops::Result;
use ocio_ops::dynamic_property::{
    DynamicPropertyGradingHueCurveImpl, DynamicPropertyGradingHueCurveImplRcPtr,
    DynamicPropertyRcPtr,
};
use ocio_ops::open_color_types::{
    GradingStyle, HsyTransformStyle, TransformDirection, transform_direction_to_string,
};
use ocio_ops::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle,
};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op_data::GradingHueCurveOpData;
use ocio_ops::utils::string_utils::replace_in_place;

use crate::gpu_shader::Getter;
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::{
    GpuShaderText, add_lin_to_log_shader_channel_blue, add_log_to_lin_shader_channel_blue,
    build_resource_name,
};
use crate::open_color_types::GpuLanguage;
use crate::ops::fixedfunction::fixed_function_op_gpu::get_fixed_function_gpu_processing_text;
use crate::ops::gradingrgbcurve::grading_rgb_curve_op_gpu::{
    add_shader_eval_fwd, add_shader_eval_rev,
};

/// The names of the op's resources: the four arrays, the bypass flag and the three helper
/// functions.
///
/// Port of `GCProperties` (GradingHueCurveOpGPU.cpp:74-84 @ v2.5.2).
struct GcProperties {
    knots_offsets: Vec<u8>,
    knots: Vec<u8>,
    coefs_offsets: Vec<u8>,
    coefs: Vec<u8>,
    local_bypass: Vec<u8>,
    eval: Vec<u8>,
    eval_rev: Vec<u8>,
    eval_rev_hue: Vec<u8>,
}

impl Default for GcProperties {
    fn default() -> Self {
        GcProperties {
            knots_offsets: b"knotsOffsets".to_vec(),
            knots: b"knots".to_vec(),
            coefs_offsets: b"coefsOffsets".to_vec(),
            coefs: b"coefs".to_vec(),
            local_bypass: b"localBypass".to_vec(),
            eval: b"evalBSplineCurve".to_vec(),
            eval_rev: b"evalBSplineCurveRev".to_vec(),
            eval_rev_hue: b"evalBSplineCurveRevHue".to_vec(),
        }
    }
}

/// `opPrefix` (GradingHueCurveOpGPU.cpp:146 @ v2.5.2).
const OP_PREFIX: &str = "grading_huecurve";

/// The offsets and counts of the eight curves (`HUE_NUM_CURVES * 2`,
/// GradingHueCurveOpGPU.cpp:107-108 @ v2.5.2).
const OFFSETS_LEN: u32 = 16;

/// Port of `BuildResourceNameIndexed` (GradingHueCurveOpGPU.cpp:135-144 @ v2.5.2).
fn build_resource_name_indexed(
    shader_creator: &GpuShaderDesc,
    prefix: &str,
    base: &[u8],
    index: u32,
) -> Vec<u8> {
    let mut name = build_resource_name(shader_creator.resource_prefix(), prefix, base);
    name.push(b'_');
    name.extend_from_slice(index.to_string().as_bytes());
    // Note: Remove potentially problematic double underscores from GLSL resource names.
    replace_in_place(&mut name, b"__", b"_");
    name
}

/// The resources' names: shared by every dynamic op (one dynamic property per type), indexed
/// with the next resource index for each non-dynamic op, which gets its own helpers.
///
/// Port of `SetGCProperties` (GradingHueCurveOpGPU.cpp:148-185 @ v2.5.2).
fn set_gc_properties(shader_creator: &mut GpuShaderDesc, dynamic: bool, props: &mut GcProperties) {
    if dynamic {
        // If there are several dynamic ops, they will use the same names for uniforms.
        let name =
            |base: &[u8]| build_resource_name(shader_creator.resource_prefix(), OP_PREFIX, base);
        props.knots_offsets = name(&props.knots_offsets);
        props.knots = name(&props.knots);
        props.coefs_offsets = name(&props.coefs_offsets);
        props.coefs = name(&props.coefs);
        props.local_bypass = name(&props.local_bypass);
        props.eval = name(&props.eval);
        props.eval_rev = name(&props.eval_rev);
        props.eval_rev_hue = name(&props.eval_rev_hue);
    } else {
        // Non-dynamic ops need a helper function for each op.
        let res_index = shader_creator.next_resource_index();
        let name =
            |base: &[u8]| build_resource_name_indexed(shader_creator, OP_PREFIX, base, res_index);
        props.knots_offsets = name(&props.knots_offsets);
        props.knots = name(&props.knots);
        props.coefs_offsets = name(&props.coefs_offsets);
        props.coefs = name(&props.coefs);
        props.eval = name(&props.eval);
        props.eval_rev = name(&props.eval_rev);
        props.eval_rev_hue = name(&props.eval_rev_hue);
    }
}

/// Adds the float array uniform `name` unless the description has it, and declares it.
///
/// Port of `AddUniform` for a vector of floats (GradingHueCurveOpGPU.cpp:86-100 @ v2.5.2).
fn add_uniform_floats(
    shader_creator: &mut GpuShaderDesc,
    size: Getter<i32>,
    values: Getter<Vec<f32>>,
    max_size: u32,
    name: &[u8],
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_vector_float(name, size, values, max_size)? {
        // Declare uniform.
        let st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_array_float(name, max_size);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// Adds the int array uniform `name` (two ints per curve) unless the description has it,
/// and declares it.
///
/// Port of `AddUniform` for a vector of ints (GradingHueCurveOpGPU.cpp:102-119 @ v2.5.2).
fn add_uniform_ints(
    shader_creator: &mut GpuShaderDesc,
    size: Getter<i32>,
    values: Getter<Vec<i32>>,
    name: &[u8],
) -> Result<()> {
    // 8 curves x 2 values (count and offset).
    if shader_creator.add_uniform_vector_int(name, size, values, OFFSETS_LEN)? {
        let st_decl = GpuShaderText::new(shader_creator.language());
        // Need 2 ints for each curve.
        st_decl.declare_uniform_array_int(name, OFFSETS_LEN);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// Adds the bool uniform `name` unless the description has it, and declares it.
///
/// Port of `AddUniform` for a bool (GradingHueCurveOpGPU.cpp:121-133 @ v2.5.2).
fn add_uniform_bool(
    shader_creator: &mut GpuShaderDesc,
    get: Getter<bool>,
    name: &[u8],
) -> Result<()> {
    if shader_creator.add_uniform_bool(name, get)? {
        let st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_bool(name);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// Binds the uniforms to the shader's copy of the property: each reads it when the
/// application asks for the uniform's value.
///
/// Port of `AddGCPropertiesUniforms` (GradingHueCurveOpGPU.cpp:187-219 @ v2.5.2).
fn add_gc_properties_uniforms(
    shader_creator: &mut GpuShaderDesc,
    shader_prop: &DynamicPropertyGradingHueCurveImplRcPtr,
    prop_names: &GcProperties,
) -> Result<()> {
    let p = Arc::clone(shader_prop);
    let get_nk: Getter<i32> = Arc::new(move || p.get_num_knots());
    let p = Arc::clone(shader_prop);
    let get_ko: Getter<Vec<i32>> = Arc::new(move || p.state().knots_coefs.knots_offsets.clone());
    let p = Arc::clone(shader_prop);
    let get_k: Getter<Vec<f32>> = Arc::new(move || p.state().knots_coefs.knots.clone());
    let p = Arc::clone(shader_prop);
    let get_nc: Getter<i32> = Arc::new(move || p.get_num_coefs());
    let p = Arc::clone(shader_prop);
    let get_co: Getter<Vec<i32>> = Arc::new(move || p.state().knots_coefs.coefs_offsets.clone());
    let p = Arc::clone(shader_prop);
    let get_c: Getter<Vec<f32>> = Arc::new(move || p.state().knots_coefs.coefs.clone());
    let p = Arc::clone(shader_prop);
    let get_lb: Getter<bool> = Arc::new(move || p.get_local_bypass());
    let num_offsets: Getter<i32> =
        Arc::new(|| DynamicPropertyGradingHueCurveImpl::NUM_OFFSET_VALUES);

    // Uniforms are added if they are not already there (added by another op).
    add_uniform_ints(
        shader_creator,
        Arc::clone(&num_offsets),
        get_ko,
        &prop_names.knots_offsets,
    )?;
    add_uniform_floats(
        shader_creator,
        get_nk,
        get_k,
        DynamicPropertyGradingHueCurveImpl::MAX_KNOTS,
        &prop_names.knots,
    )?;
    add_uniform_ints(
        shader_creator,
        num_offsets,
        get_co,
        &prop_names.coefs_offsets,
    )?;
    add_uniform_floats(
        shader_creator,
        get_nc,
        get_c,
        DynamicPropertyGradingHueCurveImpl::MAX_COEFS,
        &prop_names.coefs,
    )?;
    add_uniform_bool(shader_creator, get_lb, &prop_names.local_bypass)
}

/// The signature of a helper function: `(int curveIdx, float x, float identity_x)` forward,
/// `(int curveIdx, float x)` backward, with `in` qualifiers except in OSL and MSL.
///
/// Port of `AddCurveFunctionName` (GradingHueCurveOpGPU.cpp:221-249 @ v2.5.2).
fn add_curve_function_name(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    func_name: &[u8],
    is_fwd: bool,
) {
    st.new_line().put("");
    let lang = shader_creator.language();
    let args = match (
        lang == GpuLanguage::Osl1 || lang == GpuLanguage::Msl2_0,
        is_fwd,
    ) {
        (true, true) => "(int curveIdx, float x, float identity_x)",
        (true, false) => "(int curveIdx, float x)",
        (false, true) => "(in int curveIdx, in float x, in float identity_x)",
        (false, false) => "(in int curveIdx, in float x)",
    };
    st.new_line()
        .put(st.float_keyword())
        .put(" ")
        .put(func_name)
        .put(args);
}

/// The helper functions: the forward evaluation, and for the inverse direction the backward
/// and the hue backward ones, with the curves' arrays as constants unless the op is dynamic
/// (then the uniforms).
///
/// Port of `AddCurveEvalMethodTextToShaderProgram` (GradingHueCurveOpGPU.cpp:251-308 @
/// v2.5.2).
fn add_curve_eval_method_text_to_shader_program(
    shader_creator: &mut GpuShaderDesc,
    gc_data: &GradingHueCurveOpData,
    props: &GcProperties,
    dynamic: bool,
) -> Result<()> {
    let st = GpuShaderText::new(shader_creator.language());

    // Dynamic version uses uniforms declared globally. Non-dynamic version declares local
    // variables in the op specific helper function.
    if !dynamic {
        let prop_gc = gc_data.get_dynamic_property_internal();
        let state = prop_gc.state();
        let kc = &state.knots_coefs;
        let num_offsets = DynamicPropertyGradingHueCurveImpl::NUM_OFFSET_VALUES as usize;

        // 2 ints for each curve.
        st.new_line().put("");
        st.declare_int_array_const(&props.knots_offsets, &kc.knots_offsets[..num_offsets])?;
        st.declare_float_array_const(&props.knots, &kc.knots[..kc.num_knots as usize])?;
        st.declare_int_array_const(&props.coefs_offsets, &kc.coefs_offsets[..num_offsets])?;
        st.declare_float_array_const(&props.coefs, &kc.coefs[..kc.num_coefs as usize])?;
    }

    // Both the forward and inverse hue curve eval need the forward spline eval, so always add
    // that.

    add_curve_function_name(shader_creator, &st, &props.eval, true);

    st.new_line().put("{");
    st.indent();
    add_shader_eval_fwd(
        &st,
        &props.knots_offsets,
        &props.coefs_offsets,
        &props.knots,
        &props.coefs,
    );
    st.dedent();
    st.new_line().put("}");

    if gc_data.get_direction() == TransformDirection::Inverse {
        // Add inverse curve eval.
        add_curve_function_name(shader_creator, &st, &props.eval_rev, false);

        st.new_line().put("{");
        st.indent();
        add_shader_eval_rev(
            &st,
            &props.knots_offsets,
            &props.coefs_offsets,
            &props.knots,
            &props.coefs,
        );
        st.dedent();
        st.new_line().put("}");

        // Add inverse hue curve eval.
        add_curve_function_name(shader_creator, &st, &props.eval_rev_hue, false);

        st.new_line().put("{");
        st.indent();
        add_shader_eval_rev_hue(
            &st,
            &props.knots_offsets,
            &props.coefs_offsets,
            &props.knots,
            &props.coefs,
        );
        st.dedent();
        st.new_line().put("}");
    }

    shader_creator.add_to_helper_shader_code(st.string());
    Ok(())
}

/// The body of the inverse hue curve evaluation (of the hue-hue and hue-fx curves): `x` for a
/// curve without coefficients; otherwise `x` wrapped by whole periods into the curve's range
/// of outputs (for the hue-fx curve, index 7, of the input plus its output), and the segment's
/// quadratic solved (the hue-fx curve's with the diagonal added).
///
/// Port of `GradingBSplineCurveImpl::AddShaderEvalRevHue` (src/OpenColorIO/ops/
/// gradingrgbcurve/GradingBSplineCurve.cpp:1168-1246 @ v2.5.2).
pub fn add_shader_eval_rev_hue(
    st: &GpuShaderText,
    knots_offsets: &[u8],
    coefs_offsets: &[u8],
    knots: &[u8],
    coefs: &[u8],
) {
    // See GradingHueCurveOpGPU.cpp:AddCurveEvalMethodTextToShaderProgram.
    // The input arguments are:
    //      curveIdx -- The index of the curve being evaluated.
    //             x -- The input value.

    st.new_line()
        .put("int knotsOffs = ")
        .put(knots_offsets)
        .put("[curveIdx * 2];");
    st.new_line()
        .put("int knotsCnt = ")
        .put(knots_offsets)
        .put("[curveIdx * 2 + 1];");
    st.new_line()
        .put("int coefsOffs = ")
        .put(coefs_offsets)
        .put("[curveIdx * 2];");
    st.new_line()
        .put("int coefsCnt = ")
        .put(coefs_offsets)
        .put("[curveIdx * 2 + 1];");
    st.new_line().put("int coefsSets = coefsCnt / 3;");

    st.new_line().put("if (coefsSets == 0)");
    st.new_line().put("{");
    st.new_line().put("  return x;");
    st.new_line().put("}");

    st.new_line()
        .put("float knStart = ")
        .put(knots)
        .put("[knotsOffs];");
    st.new_line()
        .put("float knEnd = ")
        .put(knots)
        .put("[knotsOffs + knotsCnt - 1];");
    st.new_line()
        .put("float knStartY = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 2];");
    st.new_line().put("float knEndY;");
    st.new_line().put("{");
    st.new_line()
        .put("  float A = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets - 1];");
    st.new_line()
        .put("  float B = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 2 - 1];");
    st.new_line()
        .put("  float C = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 3 - 1];");
    st.new_line()
        .put("  float kn = ")
        .put(knots)
        .put("[knotsOffs + knotsCnt - 2];");
    st.new_line().put("  float t = knEnd - kn;");
    st.new_line().put("  knEndY = ( A * t + B ) * t + C;");
    // The HUE-FX curve is index 7 and requires special handling.
    st.new_line()
        .put("  knEndY = (curveIdx == 7) ? knEndY + knEnd : knEndY;");
    st.new_line().put("}");

    st.new_line().put("if (x < knStartY)");
    st.new_line().put("{");
    st.new_line().put("  x = x + ceil(knStartY - x);");
    st.new_line().put("}");

    st.new_line().put("else if (x > knEndY)");
    st.new_line().put("{");
    st.new_line().put("  x = x - ceil(x - knEndY);");
    st.new_line().put("}");

    st.new_line().put("int i = 0;");
    st.new_line().put("for (i = 0; i < knotsCnt - 2; ++i)");
    st.new_line().put("{");
    st.new_line()
        .put("  float curve_x = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 2 + i + 1];");
    st.new_line()
        .put("  curve_x = (curveIdx == 7) ? curve_x + ")
        .put(knots)
        .put("[knotsOffs + i + 1] : curve_x;");
    st.new_line().put("  if (x < curve_x)");
    st.new_line().put("  {");
    st.new_line().put("    break;");
    st.new_line().put("  }");
    st.new_line().put("}");

    st.new_line()
        .put("float A = ")
        .put(coefs)
        .put("[coefsOffs + i];");
    st.new_line()
        .put("float B = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets + i];");
    st.new_line()
        .put("float C = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 2 + i];");
    st.new_line()
        .put("float kn = ")
        .put(knots)
        .put("[knotsOffs + i];");
    st.new_line().put("if (curveIdx == 7)");
    st.new_line().put("{");
    st.new_line().put("  C = C + kn;");
    st.new_line().put("  B = B + 1.;");
    st.new_line().put("}");
    st.new_line().put("float C0 = C - x;");
    st.new_line()
        .put("float discrim = sqrt(B * B - 4. * A * C0);");
    st.new_line().put("float denom = discrim + B;");
    st.new_line().put("if (abs(denom) < 1e-5)");
    st.new_line().put("{");
    st.new_line()
        .put("  return abs(B) < 1e-5 ? kn : kn + (-C0 / B);");
    st.new_line().put("}");
    st.new_line().put("return kn + (-2. * C0) / denom;");
}

/// The three lines of a draw-curve-only op: the hue-sat curve of each channel. They are not
/// within the local bypass test: the default horizontal curves are all 1 rather than an
/// identity, so the pixel must be processed even if every curve is a default.
///
/// The `drawCurveOnly` branch of `AddGCForwardShader` and `AddGCInverseShader`
/// (GradingHueCurveOpGPU.cpp:320-329, 463-472 @ v2.5.2).
fn add_draw_curve_only(st: &GpuShaderText, pix: &[u8], eval: &[u8]) {
    for channel in ["r", "g", "b"] {
        st.new_line()
            .put(pix)
            .put(".")
            .put(channel)
            .put(" = ")
            .put(eval)
            .put("(1, ")
            .put(pix)
            .put(".")
            .put(channel)
            .put(", 1.);"); // HUE-SAT
    }
}

/// The FixedFunction writer's conversion of `style` (`RGB_TO_HSY_*` or `HSY_*_TO_RGB`) in a
/// block of its own, so local variable names won't conflict.
///
/// The `doRGBToHSY` branches of `AddGCForwardShader` and `AddGCInverseShader`
/// (GradingHueCurveOpGPU.cpp:338-361, 421-443, 481-504, 568-590 @ v2.5.2).
fn add_hsy_conversion(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    style: FixedFunctionOpStyle,
) -> Result<()> {
    st.new_line().put("{"); // establish scope so local variable names won't conflict
    st.indent();
    let func_op_data = FixedFunctionOpData::new(style)?;
    get_fixed_function_gpu_processing_text(shader_creator, st, &func_op_data)?;
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// The styles of the conversions to and from HSY.
fn hsy_styles(style: GradingStyle) -> (FixedFunctionOpStyle, FixedFunctionOpStyle) {
    match style {
        GradingStyle::Lin => (
            FixedFunctionOpStyle::RgbToHsyLin,
            FixedFunctionOpStyle::HsyLinToRgb,
        ),
        GradingStyle::Log => (
            FixedFunctionOpStyle::RgbToHsyLog,
            FixedFunctionOpStyle::HsyLogToRgb,
        ),
        GradingStyle::Video => (
            FixedFunctionOpStyle::RgbToHsyVid,
            FixedFunctionOpStyle::HsyVidToRgb,
        ),
    }
}

/// The forward block.
///
/// Port of `AddGCForwardShader` (GradingHueCurveOpGPU.cpp:310-450 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn add_gc_forward_shader(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    props: &GcProperties,
    dynamic: bool,
    do_lin_to_log: bool,
    do_rgb_to_hsy: bool,
    draw_curve_only: bool,
    style: GradingStyle,
) -> Result<()> {
    let pix = shader_creator.pixel_name().to_vec();
    let eval = &props.eval;
    if draw_curve_only {
        add_draw_curve_only(st, &pix, eval);
        return Ok(());
    }

    if dynamic {
        st.new_line()
            .put("if (!")
            .put(st.cast_to_bool(&props.local_bypass))
            .put(")");
        st.new_line().put("{");
        st.indent();
    }

    // Add the conversion from RGB to HSY.
    if do_rgb_to_hsy {
        add_hsy_conversion(shader_creator, st, hsy_styles(style).0)?;
    }

    // Lin to Log (on Luma only).
    if do_lin_to_log {
        // NB:  Although the linToLog and logToLin are correct inverses, the limits of
        // floating-point arithmetic cause errors in the lowest bit of the round trip.
        st.new_line().put("// Convert from lin to log.");
        add_lin_to_log_shader_channel_blue(&pix, st)?;
        st.new_line().put("");
    }

    // Apply the hue curves.

    let p = |s: &str| [pix.as_slice(), s.as_bytes()].concat();
    st.new_line().put("");
    // HUE-SAT
    st.new_line()
        .put("float hueSatGain = max(0., ")
        .put(eval)
        .put("(1, ")
        .put(p(".r"))
        .put(", 1.));");
    // HUE-LUM
    st.new_line()
        .put("float hueLumGain = max(0., ")
        .put(eval)
        .put("(2, ")
        .put(p(".r"))
        .put(", 1.));");
    // HUE-HUE
    st.new_line()
        .put(p(".r"))
        .put(" = ")
        .put(eval)
        .put("(0, ")
        .put(p(".r"))
        .put(", ")
        .put(p(".r"))
        .put(");");
    // SAT-SAT
    st.new_line()
        .put(p(".g"))
        .put(" = max(0., ")
        .put(eval)
        .put("(4, ")
        .put(p(".g"))
        .put(", ")
        .put(p(".g"))
        .put("));");
    // LUM-SAT
    st.new_line()
        .put("float lumSatGain = max(0., ")
        .put(eval)
        .put("(3, ")
        .put(p(".b"))
        .put(", 1.));");
    // SAT-LUM
    st.new_line()
        .put("float satGain = lumSatGain * hueSatGain;");
    st.new_line()
        .put(p(".g"))
        .put(" = satGain * ")
        .put(p(".g"))
        .put(";");
    st.new_line()
        .put("float satLumGain = max(0., ")
        .put(eval)
        .put("(6, ")
        .put(p(".g"))
        .put(", 1.));");
    // LUM-LUM
    st.new_line()
        .put(p(".b"))
        .put(" = ")
        .put(eval)
        .put("(5, ")
        .put(p(".b"))
        .put(", ")
        .put(p(".b"))
        .put(");");
    st.new_line().put("");

    // Log to Lin.
    if do_lin_to_log {
        st.new_line().put("");
        st.new_line().put("// Convert from log to lin.");
        add_log_to_lin_shader_channel_blue(&pix, st)?;
    }

    st.new_line().put("");
    st.new_line()
        .put("hueLumGain = 1. - (1. - hueLumGain) * min( 1., ")
        .put(p(".g"))
        .put(" );");
    if style == GradingStyle::Log {
        // Use shift rather than scale for log mode.
        st.new_line()
            .put(p(".b"))
            .put(" = ")
            .put(p(".b"))
            .put(" + (hueLumGain + satLumGain - 2.) * 0.1;");
    } else {
        // Note this is applied in linear space, for linear style.
        st.new_line()
            .put(p(".b"))
            .put(" = ")
            .put(p(".b"))
            .put(" * hueLumGain * satLumGain;");
    }
    st.new_line().put("");

    // HUE-FX
    st.new_line()
        .put(p(".r"))
        .put(" = ")
        .put(p(".r"))
        .put(" - floor( ")
        .put(p(".r"))
        .put(" );");
    st.new_line()
        .put(p(".r"))
        .put(" = ")
        .put(p(".r"))
        .put(" + ")
        .put(eval)
        .put("(7, ")
        .put(p(".r"))
        .put(", 0.);");

    // Add the conversion from HSY to RGB.
    if do_rgb_to_hsy {
        add_hsy_conversion(shader_creator, st, hsy_styles(style).1)?;
    }

    if dynamic {
        st.dedent();
        st.new_line().put("}");
    }
    Ok(())
}

/// The inverse block.
///
/// Port of `AddGCInverseShader` (GradingHueCurveOpGPU.cpp:452-597 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn add_gc_inverse_shader(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    props: &GcProperties,
    dynamic: bool,
    do_lin_to_log: bool,
    do_rgb_to_hsy: bool,
    draw_curve_only: bool,
    style: GradingStyle,
) -> Result<()> {
    let pix = shader_creator.pixel_name().to_vec();
    let eval = &props.eval;

    if draw_curve_only {
        add_draw_curve_only(st, &pix, eval);
        return Ok(());
    }

    if dynamic {
        st.new_line()
            .put("if (!")
            .put(st.cast_to_bool(&props.local_bypass))
            .put(")");
        st.new_line().put("{");
        st.indent();
    }

    // Add the conversion from RGB to HSY.
    if do_rgb_to_hsy {
        add_hsy_conversion(shader_creator, st, hsy_styles(style).0)?;
    }

    // Apply the hue curves inverse.

    let p = |s: &str| [pix.as_slice(), s.as_bytes()].concat();
    // Invert HUE-FX.
    st.new_line()
        .put(p(".r"))
        .put(" = ")
        .put(&props.eval_rev_hue)
        .put("(7, ")
        .put(p(".r"))
        .put(");");

    // Invert HUE-HUE.
    st.new_line()
        .put(p(".r"))
        .put(" = ")
        .put(&props.eval_rev_hue)
        .put("(0, ")
        .put(p(".r"))
        .put(");");
    st.new_line().put("");

    // Use the inverted hue to calculate the HUE-SAT & HUE-LUM gains.
    st.new_line()
        .put(p(".r"))
        .put(" = ")
        .put(p(".r"))
        .put(" - floor( ")
        .put(p(".r"))
        .put(" );");
    st.new_line()
        .put("float hueSatGain = max(0., ")
        .put(eval)
        .put("(1, ")
        .put(p(".r"))
        .put(", 1.));");
    st.new_line()
        .put("float hueLumGain = max(0., ")
        .put(eval)
        .put("(2, ")
        .put(p(".r"))
        .put(", 1.));");

    // Use the output sat to calculate the SAT-LUM gain.
    st.new_line()
        .put(p(".g"))
        .put(" = max(0., ")
        .put(p(".g"))
        .put(");");
    st.new_line()
        .put("float satLumGain = max(0., ")
        .put(eval)
        .put("(6, ")
        .put(p(".g"))
        .put(", 1.));");

    st.new_line().put("");
    st.new_line()
        .put("hueLumGain = 1. - (1. - hueLumGain) * min( 1., ")
        .put(p(".g"))
        .put(" );");

    // Invert the lum gain.
    if style == GradingStyle::Log {
        // Use shift rather than scale for log mode.
        st.new_line()
            .put(p(".b"))
            .put(" = ")
            .put(p(".b"))
            .put(" - (hueLumGain + satLumGain - 2.) * 0.1;");
    } else {
        // Note this is applied in linear space, for linear style.
        st.new_line()
            .put(p(".b"))
            .put(" = ")
            .put(p(".b"))
            .put(" / max(0.01, hueLumGain * satLumGain);");
    }
    st.new_line().put("");

    if do_lin_to_log {
        st.new_line().put("// Convert from lin to log.");
        add_lin_to_log_shader_channel_blue(&pix, st)?;
        st.new_line().put("");
    }

    // Invert LUM-LUM.
    st.new_line()
        .put(p(".b"))
        .put(" = ")
        .put(&props.eval_rev)
        .put("(5, ")
        .put(p(".b"))
        .put(");");
    st.new_line().put("");

    // Use it to calc the LUM-SAT gain.
    st.new_line()
        .put("float lumSatGain = max(0., ")
        .put(eval)
        .put("(3, ")
        .put(p(".b"))
        .put(", 1.));");

    if do_lin_to_log {
        st.new_line().put("");
        st.new_line().put("// Convert from log to lin.");
        add_log_to_lin_shader_channel_blue(&pix, st)?;
    }

    // Invert the sat gain.
    st.new_line()
        .put("float satGain = max(0.01, lumSatGain * hueSatGain);");
    st.new_line()
        .put(p(".g"))
        .put(" = ")
        .put(p(".g"))
        .put(" / satGain;");

    // Invert SAT-SAT.
    st.new_line()
        .put(p(".g"))
        .put(" = max(0., ")
        .put(&props.eval_rev)
        .put("(4, ")
        .put(p(".g"))
        .put("));");

    // Add the conversion from HSY to RGB.
    if do_rgb_to_hsy {
        add_hsy_conversion(shader_creator, st, hsy_styles(style).1)?;
    }

    if dynamic {
        st.dedent();
        st.new_line().put("}");
    }
    Ok(())
}

/// Adds the code of a GradingHueCurve op to `shader_creator`: nothing for a non-dynamic op
/// whose property says to bypass (every curve an identity, and not drawn: I-272); for a
/// dynamic op (except in OSL, which has no dynamic properties: a warning, and the curves as
/// constants), the shader's own copy of the dynamic property and its uniforms.
///
/// Port of `GetGradingHueCurveGPUShaderProgram` (GradingHueCurveOpGPU.cpp:601-682 @ v2.5.2).
pub fn get_grading_hue_curve_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    gc_data: &GradingHueCurveOpData,
) -> Result<()> {
    let dynamic = gc_data.is_dynamic() && shader_creator.language() != GpuLanguage::Osl1;
    if !dynamic && gc_data.get_dynamic_property_internal().get_local_bypass() {
        return Ok(());
    }

    if gc_data.is_dynamic() && shader_creator.language() == GpuLanguage::Osl1 {
        shader_creator.log_warning(format!(
            "The dynamic properties are not yet supported by the 'Open Shading language (OSL)' \
             translation: The '{OP_PREFIX}' dynamic property is replaced by a local variable."
        ));
    }

    let style = gc_data.get_style();
    let dir = gc_data.get_direction();

    let st = GpuShaderText::new(shader_creator.language());
    st.indent();

    st.new_line().put("");
    st.new_line()
        .put("// Add GradingHueCurve ")
        .put(transform_direction_to_string(dir))
        .put(" processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let mut properties = GcProperties::default();
    set_gc_properties(shader_creator, dynamic, &mut properties);

    let dyn_prop = gc_data.get_dynamic_property_internal();

    if dynamic {
        // Add the dynamic property to the shader creator.

        // Property is decoupled.
        let shader_prop = dyn_prop.create_editable_copy()?;
        shader_creator.add_dynamic_property(DynamicPropertyRcPtr::GradingHueCurve(Arc::clone(
            &shader_prop,
        )))?;

        // Add uniforms only if needed.
        add_gc_properties_uniforms(shader_creator, &shader_prop, &properties)?;

        // Add helper function plus global variables if they are not dynamic.
        add_curve_eval_method_text_to_shader_program(
            shader_creator,
            gc_data,
            &properties,
            dynamic,
        )?;
    } else {
        // Declare the op specific helper function.
        add_curve_eval_method_text_to_shader_program(
            shader_creator,
            gc_data,
            &properties,
            dynamic,
        )?;
    }

    let do_lin_to_log = style == GradingStyle::Lin;
    let do_rgb_to_hsy = gc_data.get_rgb_to_hsy() == HsyTransformStyle::Transform1;
    let draw_curve_only = dyn_prop.get_value().get_draw_curve_only();
    match dir {
        TransformDirection::Forward => add_gc_forward_shader(
            shader_creator,
            &st,
            &properties,
            dynamic,
            do_lin_to_log,
            do_rgb_to_hsy,
            draw_curve_only,
            style,
        )?,
        TransformDirection::Inverse => add_gc_inverse_shader(
            shader_creator,
            &st,
            &properties,
            dynamic,
            do_lin_to_log,
            do_rgb_to_hsy,
            draw_curve_only,
            style,
        )?,
    }

    st.dedent();
    st.new_line().put("}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}
