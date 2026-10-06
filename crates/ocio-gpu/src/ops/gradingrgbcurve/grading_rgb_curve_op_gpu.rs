// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpGPU.h` and
//! `GradingRGBCurveOpGPU.cpp` @ v2.5.2, the GradingRGBCurve op's GPU writer, with the curve
//! evaluation's shader text of `GradingBSplineCurveImpl::AddShaderEvalFwd` and
//! `AddShaderEvalRev` (src/OpenColorIO/ops/gradingrgbcurve/GradingBSplineCurve.cpp:1021-1164).
//!
//! The curves' knots and coefficients (`KnotsCoefs`) go into the shader as constant arrays in
//! an op-specific helper function, or, for a dynamic op, as uniforms bound to a copy of the
//! op's dynamic property that the shader description holds, so the application can change the
//! curves after the extraction. The helper function evaluates one curve
//! (`evalBSplineCurve(curveIdx, x, identity_x)`); the op's block calls it for red, green,
//! blue, then master, or the inverse in the opposite order, around the linear style's
//! conversion to and from the grading log.

use std::sync::Arc;

use ocio_ops::Result;
use ocio_ops::dynamic_property::{
    DynamicPropertyGradingRgbCurveImpl, DynamicPropertyGradingRgbCurveImplRcPtr,
    DynamicPropertyRcPtr,
};
use ocio_ops::logging::log_warning;
use ocio_ops::open_color_types::{
    GradingStyle, TransformDirection, grading_style_to_string, transform_direction_to_string,
};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::KnotsCoefs;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_ops::utils::string_utils::replace_in_place;

use crate::gpu_shader::Getter;
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::{
    GpuShaderText, add_lin_to_log_shader, add_log_to_lin_shader, build_resource_name,
};
use crate::open_color_types::GpuLanguage;

/// The names of the op's resources: the four arrays, the bypass flag and the helper
/// function.
///
/// Port of `GCProperties` (GradingRGBCurveOpGPU.cpp:71-79 @ v2.5.2).
struct GcProperties {
    knots_offsets: Vec<u8>,
    knots: Vec<u8>,
    coefs_offsets: Vec<u8>,
    coefs: Vec<u8>,
    local_bypass: Vec<u8>,
    eval: Vec<u8>,
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
        }
    }
}

/// `opPrefix` (GradingRGBCurveOpGPU.cpp:150 @ v2.5.2).
const OP_PREFIX: &str = "grading_rgbcurve";

/// The offsets and counts of the four curves (`RGB_NUM_CURVES * 2`).
const OFFSETS_LEN: u32 = 8;

/// Port of `BuildResourceNameIndexed` (GradingRGBCurveOpGPU.cpp:137-148 @ v2.5.2).
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
/// with the next resource index for each non-dynamic op, which gets its own helper.
///
/// Port of `SetGCProperties` (GradingRGBCurveOpGPU.cpp:152-187 @ v2.5.2).
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
    } else {
        // Non-dynamic ops need an helper function for each op.
        let res_index = shader_creator.next_resource_index();
        let name =
            |base: &[u8]| build_resource_name_indexed(shader_creator, OP_PREFIX, base, res_index);
        props.knots_offsets = name(&props.knots_offsets);
        props.knots = name(&props.knots);
        props.coefs_offsets = name(&props.coefs_offsets);
        props.coefs = name(&props.coefs);
        props.eval = name(&props.eval);
    }
}

/// Adds the float array uniform `name` unless the description has it, and declares it.
///
/// Port of `AddUniform` for a vector of floats (GradingRGBCurveOpGPU.cpp:81-95 @ v2.5.2).
fn add_uniform_floats(
    shader_creator: &mut GpuShaderDesc,
    size: Getter<i32>,
    values: Getter<Vec<f32>>,
    max_size: u32,
    name: &[u8],
) -> Result<()> {
    if shader_creator.add_uniform_vector_float(name, size, values, max_size)? {
        let st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_array_float(name, max_size);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// Adds the int array uniform `name` (two ints per curve) unless the description has it,
/// and declares it.
///
/// Port of `AddUniform` for a vector of ints (GradingRGBCurveOpGPU.cpp:97-115 @ v2.5.2).
fn add_uniform_ints(
    shader_creator: &mut GpuShaderDesc,
    size: Getter<i32>,
    values: Getter<Vec<i32>>,
    name: &[u8],
) -> Result<()> {
    if shader_creator.add_uniform_vector_int(name, size, values, OFFSETS_LEN)? {
        let st_decl = GpuShaderText::new(shader_creator.language());
        // Need 2 ints for each RGBM curve.
        st_decl.declare_uniform_array_int(name, OFFSETS_LEN);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// Adds the bool uniform `name` unless the description has it, and declares it.
///
/// Port of `AddUniform` for a bool (GradingRGBCurveOpGPU.cpp:117-130 @ v2.5.2).
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
/// Port of `AddGCPropertiesUniforms` (GradingRGBCurveOpGPU.cpp:189-221 @ v2.5.2).
fn add_gc_properties_uniforms(
    shader_creator: &mut GpuShaderDesc,
    shader_prop: &DynamicPropertyGradingRgbCurveImplRcPtr,
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
        Arc::new(|| DynamicPropertyGradingRgbCurveImpl::NUM_OFFSET_VALUES);

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
        DynamicPropertyGradingRgbCurveImpl::MAX_KNOTS,
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
        DynamicPropertyGradingRgbCurveImpl::MAX_COEFS,
        &prop_names.coefs,
    )?;
    add_uniform_bool(shader_creator, get_lb, &prop_names.local_bypass)
}

/// The helper function that evaluates one curve, with the curves' arrays as its constants
/// unless the op is dynamic (then the uniforms).
///
/// Port of `AddCurveEvalMethodTextToShaderProgram` (GradingRGBCurveOpGPU.cpp:223-267 @
/// v2.5.2).
fn add_curve_eval_method_text_to_shader_program(
    shader_creator: &mut GpuShaderDesc,
    gc_data: &GradingRgbCurveOpData,
    props: &GcProperties,
    dynamic: bool,
) -> Result<()> {
    let st = GpuShaderText::new(shader_creator.language());

    // Dynamic version uses uniforms declared globally. Non-dynamic version declares local
    // variables in the op specific helper function.
    if !dynamic {
        let prop_gc = gc_data.get_dynamic_property_internal();
        let state = prop_gc.state();
        let kc: &KnotsCoefs = &state.knots_coefs;
        let num_offsets = DynamicPropertyGradingRgbCurveImpl::NUM_OFFSET_VALUES as usize;

        // 2 ints for each curve.
        st.new_line().put("");
        st.declare_int_array_const(&props.knots_offsets, &kc.knots_offsets[..num_offsets])?;
        st.declare_float_array_const(&props.knots, &kc.knots[..kc.num_knots as usize])?;
        st.declare_int_array_const(&props.coefs_offsets, &kc.coefs_offsets[..num_offsets])?;
        st.declare_float_array_const(&props.coefs, &kc.coefs[..kc.num_coefs as usize])?;
    }

    st.new_line().put("");
    let lang = shader_creator.language();
    if lang == GpuLanguage::Osl1 || lang == GpuLanguage::Msl2_0 {
        st.new_line()
            .put(st.float_keyword())
            .put(" ")
            .put(&props.eval)
            .put("(int curveIdx, float x, float identity_x)");
    } else {
        st.new_line()
            .put(st.float_keyword())
            .put(" ")
            .put(&props.eval)
            .put("(in int curveIdx, in float x, in float identity_x)");
    }
    st.new_line().put("{");
    st.indent();
    if gc_data.get_direction() == TransformDirection::Inverse {
        add_shader_eval_rev(
            &st,
            &props.knots_offsets,
            &props.coefs_offsets,
            &props.knots,
            &props.coefs,
        );
    } else {
        add_shader_eval_fwd(
            &st,
            &props.knots_offsets,
            &props.coefs_offsets,
            &props.knots,
            &props.coefs,
        );
    }
    st.dedent();
    st.new_line().put("}");

    shader_creator.add_to_helper_shader_code(st.string());
    Ok(())
}

/// The body of the forward curve evaluation: `identity_x` for a curve without coefficients,
/// the end lines outside the knots, the segment's quadratic in between.
///
/// Port of `GradingBSplineCurveImpl::AddShaderEvalFwd` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingBSplineCurve.cpp:1021-1084 @ v2.5.2).
pub fn add_shader_eval_fwd(
    st: &GpuShaderText,
    knots_offsets: &[u8],
    coefs_offsets: &[u8],
    knots: &[u8],
    coefs: &[u8],
) {
    add_offsets(st, knots_offsets, coefs_offsets);
    // If the curve has the default/identity values, the coef data is empty, return the
    // identity.
    st.new_line().put("if (coefsSets == 0)");
    st.new_line().put("{");
    st.new_line().put("  return identity_x;");
    st.new_line().put("}");

    st.new_line()
        .put("float knStart = ")
        .put(knots)
        .put("[knotsOffs];");
    st.new_line()
        .put("float knEnd = ")
        .put(knots)
        .put("[knotsOffs + knotsCnt - 1];");

    st.new_line().put("if (x <= knStart)");
    st.new_line().put("{");
    st.new_line()
        .put("  float B = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets];");
    st.new_line()
        .put("  float C = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 2];");
    st.new_line().put("  return (x - knStart) * B + C;");
    st.new_line().put("}");

    st.new_line().put("else if (x >= knEnd)");
    st.new_line().put("{");
    add_last_segment(st, knots, coefs);
    st.new_line().put("  float slope = 2. * A * t + B;");
    st.new_line().put("  float offs = ( A * t + B ) * t + C;");
    st.new_line().put("  return (x - knEnd) * slope + offs;");
    st.new_line().put("}");

    // else
    st.new_line().put("int i = 0;");
    st.new_line().put("for (i = 0; i < knotsCnt - 2; ++i)");
    st.new_line().put("{");
    st.new_line()
        .put("  if (x < ")
        .put(knots)
        .put("[knotsOffs + i + 1])");
    st.new_line().put("  {");
    st.new_line().put("    break;");
    st.new_line().put("  }");
    st.new_line().put("}");

    add_segment(st, knots, coefs);
    st.new_line().put("float t = x - kn;");
    st.new_line().put("return ( A * t + B ) * t + C;");
}

/// The body of the inverse curve evaluation (of `B_SPLINE` and `DIAGONAL_B_SPLINE` curves):
/// `x` for a curve without coefficients, the end lines' inverses outside the curve's range,
/// the segment's quadratic solved in between.
///
/// Port of `GradingBSplineCurveImpl::AddShaderEvalRev` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingBSplineCurve.cpp:1086-1164 @ v2.5.2).
pub fn add_shader_eval_rev(
    st: &GpuShaderText,
    knots_offsets: &[u8],
    coefs_offsets: &[u8],
    knots: &[u8],
    coefs: &[u8],
) {
    add_offsets(st, knots_offsets, coefs_offsets);

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
    add_last_segment(st, knots, coefs);
    st.new_line().put("  knEndY = ( A * t + B ) * t + C;");
    st.new_line().put("}");

    st.new_line().put("if (x <= knStartY)");
    st.new_line().put("{");
    st.new_line()
        .put("  float B = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets];");
    st.new_line()
        .put("  float C = ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 2];");
    st.new_line()
        .put("  return abs(B) < 1e-5 ? knStart : (x - C) / B + knStart;");
    st.new_line().put("}");

    st.new_line().put("else if (x >= knEndY)");
    st.new_line().put("{");
    add_last_segment(st, knots, coefs);
    st.new_line().put("  float slope = 2. * A * t + B;");
    st.new_line().put("  float offs = ( A * t + B ) * t + C;");
    st.new_line()
        .put("  return abs(slope) < 1e-5 ? knEnd : (x - offs) / slope + knEnd;");
    st.new_line().put("}");

    // else
    st.new_line().put("int i = 0;");
    st.new_line().put("for (i = 0; i < knotsCnt - 2; ++i)");
    st.new_line().put("{");
    st.new_line()
        .put("  if (x < ")
        .put(coefs)
        .put("[coefsOffs + coefsSets * 2 + i + 1])");
    st.new_line().put("  {");
    st.new_line().put("    break;");
    st.new_line().put("  }");
    st.new_line().put("}");

    add_segment(st, knots, coefs);
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

/// The curve's offsets and counts, the first lines of both evaluations.
fn add_offsets(st: &GpuShaderText, knots_offsets: &[u8], coefs_offsets: &[u8]) {
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
}

/// The last segment's coefficients, knot and length, indented in a block.
fn add_last_segment(st: &GpuShaderText, knots: &[u8], coefs: &[u8]) {
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
}

/// Segment `i`'s coefficients and knot.
fn add_segment(st: &GpuShaderText, knots: &[u8], coefs: &[u8]) {
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
}

/// The calls of the evaluation for red, green and blue (`order` holds the curve indices).
fn add_curve_calls(st: &GpuShaderText, pix: &[u8], eval: &[u8], order: [(u8, &str); 6]) {
    for (curve, channel) in order {
        st.new_line()
            .put(pix)
            .put(".rgb.")
            .put(channel)
            .put(" = ")
            .put(eval)
            .put("(")
            .put(u32::from(curve))
            .put(", ")
            .put(pix)
            .put(".rgb.")
            .put(channel)
            .put(", ")
            .put(pix)
            .put(".rgb.")
            .put(channel)
            .put(");");
    }
}

/// The op's block: the bypass test of a dynamic op, the linear style's conversion, the calls.
///
/// Port of `AddGCForwardShader` and `AddGCInverseShader` (GradingRGBCurveOpGPU.cpp:270-376
/// @ v2.5.2), which differ in the order of the calls only.
fn add_gc_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    props: &GcProperties,
    dynamic: bool,
    do_lin_to_log: bool,
    inverse: bool,
) -> Result<()> {
    if dynamic {
        st.new_line()
            .put("if (!")
            .put(st.cast_to_bool(&props.local_bypass))
            .put(")");
        st.new_line().put("{");
        st.indent();
    }

    let pix = shader_creator.pixel_name();
    if do_lin_to_log {
        // NB:  Although the linToLog and logToLin are correct inverses, the limits of
        // floating-point arithmetic cause errors in the lowest bit of the round trip.
        st.new_line().put("// Convert from lin to log.");
        add_lin_to_log_shader(pix, st)?;
        st.new_line().put("");
    }

    // Call the curve evaluation method for each curve.
    const RGB: [(u8, &str); 3] = [(0, "r"), (1, "g"), (2, "b")];
    const MASTER: [(u8, &str); 3] = [(3, "r"), (3, "g"), (3, "b")];
    let order = if inverse {
        [MASTER[0], MASTER[1], MASTER[2], RGB[0], RGB[1], RGB[2]]
    } else {
        [RGB[0], RGB[1], RGB[2], MASTER[0], MASTER[1], MASTER[2]]
    };
    add_curve_calls(st, pix, &props.eval, order);

    if do_lin_to_log {
        st.new_line().put("");
        st.new_line().put("// Convert from log to lin.");
        add_log_to_lin_shader(pix, st)?;
    }

    if dynamic {
        st.dedent();
        st.new_line().put("}");
    }
    Ok(())
}

/// Adds the code of a GradingRGBCurve op to `shader_creator`: nothing for a non-dynamic op
/// whose curves are all identities; for a dynamic op (except in OSL, which has no dynamic
/// properties: a warning, and the curves as constants), the shader's own copy of the
/// dynamic property and its uniforms.
///
/// Port of `GetGradingRGBCurveGPUShaderProgram` (GradingRGBCurveOpGPU.cpp:380-438 @ v2.5.2).
pub fn get_grading_rgb_curve_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    gc_data: &GradingRgbCurveOpData,
) -> Result<()> {
    let dynamic = gc_data.is_dynamic() && shader_creator.language() != GpuLanguage::Osl1;
    if !dynamic && gc_data.get_dynamic_property_internal().get_local_bypass() {
        return Ok(());
    }

    if gc_data.is_dynamic() && shader_creator.language() == GpuLanguage::Osl1 {
        log_warning(format!(
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
        .put("// Add GradingRGBCurve '")
        .put(grading_style_to_string(style))
        .put("' ")
        .put(transform_direction_to_string(dir))
        .put(" processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let mut properties = GcProperties::default();
    set_gc_properties(shader_creator, dynamic, &mut properties);

    if dynamic {
        // Add the dynamic property to the shader creator.
        let prop = gc_data.get_dynamic_property_internal();

        // Property is decoupled.
        let shader_prop = prop.create_editable_copy()?;
        shader_creator.add_dynamic_property(DynamicPropertyRcPtr::GradingRgbCurve(Arc::clone(
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

    let do_lin_to_log = style == GradingStyle::Lin && !gc_data.get_bypass_lin_to_log();
    add_gc_shader(
        shader_creator,
        &st,
        &properties,
        dynamic,
        do_lin_to_log,
        dir == TransformDirection::Inverse,
    )?;

    st.dedent();
    st.new_line().put("}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}
