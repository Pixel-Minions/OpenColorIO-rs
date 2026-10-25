// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/gradingtone/GradingToneOpGPU.h` and `GradingToneOpGPU.cpp` @
//! v2.5.2, the GradingTone op's GPU writer.
//!
//! The values (each tone's components, and the ranges of the highlights, shadows, whites and
//! blacks as their pre-render form has them) go into the shader as local constants, or, for a
//! dynamic op (in every language but OSL), as uniforms bound to a copy of the op's dynamic
//! property that the shader description holds, so the application can change the values after
//! the extraction; a boolean uniform then skips the op's block when the values change nothing.
//! OSL, which has no dynamic properties, gets the constants and a warning. Unlike the CPU
//! renderers, the shader computes each curve's control points itself, per pixel.

use std::sync::Arc;

use ocio_ops::Result;
use ocio_ops::cfmt::{Crt, to_string_f32};
use ocio_ops::dynamic_property::{DynamicPropertyGradingToneImpl, DynamicPropertyRcPtr};
use ocio_ops::open_color_types::{
    GradingStyle, TransformDirection, grading_style_to_string, transform_direction_to_string,
};
use ocio_ops::ops::gradingtone::grading_tone::GradingTonePreRender;
use ocio_ops::ops::gradingtone::grading_tone_op_data::GradingToneOpData;

use crate::gpu_shader::Getter;
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::{
    GpuShaderText, add_lin_to_log_shader, add_log_to_lin_shader, build_resource_name,
};
use crate::open_color_types::GpuLanguage;

/// Writes one line of shader code made of the pieces.
macro_rules! line {
    ($st:expr $(, $piece:expr)* $(,)?) => {{
        let mut l = $st.new_line();
        $( l.put($piece); )*
    }};
}

/// The names of the values in the shader code: undecorated, for local variables, until a
/// dynamic op makes them uniforms' names.
///
/// Port of `GTProperties` (GradingToneOpGPU.cpp:17-57 @ v2.5.2).
struct GtProperties {
    blacks_r: Vec<u8>,
    blacks_g: Vec<u8>,
    blacks_b: Vec<u8>,
    blacks_m: Vec<u8>,
    blacks_s: Vec<u8>,
    blacks_w: Vec<u8>,

    shadows_r: Vec<u8>,
    shadows_g: Vec<u8>,
    shadows_b: Vec<u8>,
    shadows_m: Vec<u8>,
    shadows_s: Vec<u8>,
    shadows_w: Vec<u8>,

    midtones_r: Vec<u8>,
    midtones_g: Vec<u8>,
    midtones_b: Vec<u8>,
    midtones_m: Vec<u8>,
    midtones_s: Vec<u8>,
    midtones_w: Vec<u8>,

    highlights_r: Vec<u8>,
    highlights_g: Vec<u8>,
    highlights_b: Vec<u8>,
    highlights_m: Vec<u8>,
    highlights_s: Vec<u8>,
    highlights_w: Vec<u8>,

    whites_r: Vec<u8>,
    whites_g: Vec<u8>,
    whites_b: Vec<u8>,
    whites_m: Vec<u8>,
    whites_s: Vec<u8>,
    whites_w: Vec<u8>,

    s_contrast: Vec<u8>,

    local_bypass: Vec<u8>,
}

impl Default for GtProperties {
    fn default() -> Self {
        let n = |s: &str| s.as_bytes().to_vec();
        GtProperties {
            blacks_r: n("blacksR"),
            blacks_g: n("blacksG"),
            blacks_b: n("blacksB"),
            blacks_m: n("blacksM"),
            blacks_s: n("blacksStart"),
            blacks_w: n("blacksWidth"),
            shadows_r: n("shadowsR"),
            shadows_g: n("shadowsG"),
            shadows_b: n("shadowsB"),
            shadows_m: n("shadowsM"),
            shadows_s: n("shadowsStart"),
            shadows_w: n("shadowsWidth"),
            midtones_r: n("midtonesR"),
            midtones_g: n("midtonesG"),
            midtones_b: n("midtonesB"),
            midtones_m: n("midtonesM"),
            midtones_s: n("midtonesStart"),
            midtones_w: n("midtonesWidth"),
            highlights_r: n("highlightsR"),
            highlights_g: n("highlightsG"),
            highlights_b: n("highlightsB"),
            highlights_m: n("highlightsM"),
            highlights_s: n("highlightsStart"),
            highlights_w: n("highlightsWidth"),
            whites_r: n("whitesR"),
            whites_g: n("whitesG"),
            whites_b: n("whitesB"),
            whites_m: n("whitesM"),
            whites_s: n("whitesStart"),
            whites_w: n("whitesWidth"),
            s_contrast: n("sContrast"),
            local_bypass: n("localBypass"),
        }
    }
}

/// Adds the float uniform `name` and declares it, unless the description has it already.
///
/// Port of `AddUniform` (GradingToneOpGPU.cpp:59-71 @ v2.5.2).
fn add_uniform(shader_creator: &mut GpuShaderDesc, getter: Getter<f64>, name: &[u8]) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_double(name, getter)? {
        // Declare uniform.
        let st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_float(name);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// Adds the bool uniform `name` and declares it, unless the description has it already.
///
/// Port of `AddBoolUniform` (GradingToneOpGPU.cpp:73-85 @ v2.5.2).
fn add_bool_uniform(
    shader_creator: &mut GpuShaderDesc,
    get_bool: Getter<bool>,
    name: &[u8],
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_bool(name, get_bool)? {
        // Declare uniform.
        let st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_bool(name);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// A value of the property, read when the application asks for its uniform.
type ValueOf = fn(&DynamicPropertyGradingToneImpl) -> f64;

/// `opPrefix` (GradingToneOpGPU.cpp:87 @ v2.5.2).
const OP_PREFIX: &str = "grading_tone";

/// The values: for a dynamic op, uniforms (with their decorated names) bound to the shader
/// description's copy of the property, added if the description doesn't have them; else local
/// constants of the values.
///
/// Port of `AddGTProperties` (GradingToneOpGPU.cpp:89-264 @ v2.5.2).
fn add_gt_properties(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    gt_data: &GradingToneOpData,
    p: &mut GtProperties,
    dyn_: bool,
) -> Result<()> {
    let prop = gt_data.get_dynamic_property_internal();
    if dyn_ {
        // Build names. No need to add an index to the name to avoid collisions as the dynamic
        // properties are unique.
        let prefix = shader_creator.resource_prefix().to_vec();
        for name in [
            &mut p.blacks_r,
            &mut p.blacks_g,
            &mut p.blacks_b,
            &mut p.blacks_m,
            &mut p.blacks_s,
            &mut p.blacks_w,
            &mut p.shadows_r,
            &mut p.shadows_g,
            &mut p.shadows_b,
            &mut p.shadows_m,
            &mut p.shadows_s,
            &mut p.shadows_w,
            &mut p.midtones_r,
            &mut p.midtones_g,
            &mut p.midtones_b,
            &mut p.midtones_m,
            &mut p.midtones_s,
            &mut p.midtones_w,
            &mut p.highlights_r,
            &mut p.highlights_g,
            &mut p.highlights_b,
            &mut p.highlights_m,
            &mut p.highlights_s,
            &mut p.highlights_w,
            &mut p.whites_r,
            &mut p.whites_g,
            &mut p.whites_b,
            &mut p.whites_m,
            &mut p.whites_s,
            &mut p.whites_w,
            &mut p.s_contrast,
            &mut p.local_bypass,
        ] {
            *name = build_resource_name(&prefix, OP_PREFIX, name.as_slice());
        }

        // Property is decoupled and added to shader creator.
        let shader_prop = prop.create_editable_copy();
        let new_prop = DynamicPropertyRcPtr::GradingTone(Arc::clone(&shader_prop));
        shader_creator.add_dynamic_property(new_prop)?;

        // Use the shader dynamic property to bind the uniforms (each getter reads the
        // property's values when the application asks for them).
        let get = |f: ValueOf| -> Getter<f64> {
            let sp = Arc::clone(&shader_prop);
            Arc::new(move || f(&sp))
        };

        // Add uniforms if they are not already there.
        let uniforms: [(ValueOf, &[u8]); 31] = [
            (|p| p.state().value.blacks.red, &p.blacks_r),
            (|p| p.state().value.blacks.green, &p.blacks_g),
            (|p| p.state().value.blacks.blue, &p.blacks_b),
            (|p| p.state().value.blacks.master, &p.blacks_m),
            (|p| p.state().pre_render.blacks_start, &p.blacks_s),
            (|p| p.state().pre_render.blacks_width, &p.blacks_w),
            (|p| p.state().value.shadows.red, &p.shadows_r),
            (|p| p.state().value.shadows.green, &p.shadows_g),
            (|p| p.state().value.shadows.blue, &p.shadows_b),
            (|p| p.state().value.shadows.master, &p.shadows_m),
            (|p| p.state().pre_render.shadows_start, &p.shadows_s),
            (|p| p.state().pre_render.shadows_width, &p.shadows_w),
            (|p| p.state().value.midtones.red, &p.midtones_r),
            (|p| p.state().value.midtones.green, &p.midtones_g),
            (|p| p.state().value.midtones.blue, &p.midtones_b),
            (|p| p.state().value.midtones.master, &p.midtones_m),
            (|p| p.state().value.midtones.start, &p.midtones_s),
            (|p| p.state().value.midtones.width, &p.midtones_w),
            (|p| p.state().value.highlights.red, &p.highlights_r),
            (|p| p.state().value.highlights.green, &p.highlights_g),
            (|p| p.state().value.highlights.blue, &p.highlights_b),
            (|p| p.state().value.highlights.master, &p.highlights_m),
            (|p| p.state().pre_render.highlights_start, &p.highlights_s),
            (|p| p.state().pre_render.highlights_width, &p.highlights_w),
            (|p| p.state().value.whites.red, &p.whites_r),
            (|p| p.state().value.whites.green, &p.whites_g),
            (|p| p.state().value.whites.blue, &p.whites_b),
            (|p| p.state().value.whites.master, &p.whites_m),
            (|p| p.state().pre_render.whites_start, &p.whites_s),
            (|p| p.state().pre_render.whites_width, &p.whites_w),
            (|p| p.state().value.scontrast, &p.s_contrast),
        ];
        for (f, name) in uniforms {
            add_uniform(shader_creator, get(f), name)?;
        }

        let sp = Arc::clone(&shader_prop);
        add_bool_uniform(
            shader_creator,
            Arc::new(move || sp.get_local_bypass()),
            &p.local_bypass,
        )?;
    } else {
        let state = *prop.state();
        let (value, comp_val) = (state.value, state.pre_render);

        let constants: [(&[u8], f64); 31] = [
            (&p.blacks_r, value.blacks.red),
            (&p.blacks_g, value.blacks.green),
            (&p.blacks_b, value.blacks.blue),
            (&p.blacks_m, value.blacks.master),
            (&p.blacks_s, comp_val.blacks_start),
            (&p.blacks_w, comp_val.blacks_width),
            (&p.shadows_r, value.shadows.red),
            (&p.shadows_g, value.shadows.green),
            (&p.shadows_b, value.shadows.blue),
            (&p.shadows_m, value.shadows.master),
            (&p.shadows_s, comp_val.shadows_start),
            (&p.shadows_w, comp_val.shadows_width),
            (&p.midtones_r, value.midtones.red),
            (&p.midtones_g, value.midtones.green),
            (&p.midtones_b, value.midtones.blue),
            (&p.midtones_m, value.midtones.master),
            (&p.midtones_s, value.midtones.start),
            (&p.midtones_w, value.midtones.width),
            (&p.highlights_r, value.highlights.red),
            (&p.highlights_g, value.highlights.green),
            (&p.highlights_b, value.highlights.blue),
            (&p.highlights_m, value.highlights.master),
            (&p.highlights_s, comp_val.highlights_start),
            (&p.highlights_w, comp_val.highlights_width),
            (&p.whites_r, value.whites.red),
            (&p.whites_g, value.whites.green),
            (&p.whites_b, value.whites.blue),
            (&p.whites_m, value.whites.master),
            (&p.whites_s, comp_val.whites_start),
            (&p.whites_w, comp_val.whites_width),
            (&p.s_contrast, value.scontrast),
        ];
        for (name, v) in constants {
            st.declare_var_const_f32(name, v as f32)?;
        }
    }
    Ok(())
}

/// A component of the RGBM values: `R` (0), `G`, `B`, or `M` (3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    R,
    G,
    B,
    M,
}

use Channel::{B, G, M, R};

/// The channel's swizzle of the pixel and its component's value name.
fn channel_suffix(channel: Channel) -> &'static str {
    match channel {
        R => "rgb.r",
        G => "rgb.g",
        B => "rgb.b",
        M => "rgb",
    }
}

/// `std::to_string(float)`: `%f`.
fn cpp_to_string(value: f32) -> String {
    to_string_f32(Crt::NATIVE, value)
}

/// The style's top, s-contrast top, bottom and pivot.
fn from_style(style: GradingStyle) -> (f32, f32, f32, f32) {
    GradingTonePreRender::from_style(style)
}

/// Opens the midtones' block and computes the curve's control points.
///
/// Port of `Add_MidsPre_Shader` (GradingToneOpGPU.cpp:271-369 @ v2.5.2).
fn add_mids_pre_shader(
    channel: Channel,
    st: &GpuShaderText,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<&'static str> {
    // TODO: Everything in here should move to C++ (doesn't vary per pixel).

    let channel_value = match channel {
        R => &props.midtones_r,
        G => &props.midtones_g,
        B => &props.midtones_b,
        M => &props.midtones_m,
    };

    line!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();

    let (top, _top_sc, bottom, _pivot) = from_style(style);
    let top_point = cpp_to_string(top);
    let bottom_point = cpp_to_string(bottom);

    let fkc = st.float_keyword_const();
    line!(st, fkc.as_str(), " halo = 0.4;");
    line!(
        st,
        st.float_decl("mid_adj")?,
        " = clamp(",
        channel_value.as_slice(),
        ", 0.01, 1.99);"
    );

    line!(st, "if (mid_adj != 1.)");
    line!(st, "{");
    st.indent();

    line!(st, fkc.as_str(), " x0 = ", bottom_point.as_str(), ";");
    line!(st, fkc.as_str(), " x5 = ", top_point.as_str(), ";");

    line!(st, fkc.as_str(), " max_width = (x5 - x0) * 0.95;");
    line!(
        st,
        st.float_decl("width")?,
        " = clamp(",
        props.midtones_w.as_slice(),
        ", 0.01, max_width);"
    );
    line!(st, st.float_decl("min_cent")?, " = x0 + width * 0.51;");
    line!(st, st.float_decl("max_cent")?, " = x5 - width * 0.51;");
    line!(
        st,
        st.float_decl("center")?,
        " = clamp(",
        props.midtones_s.as_slice(),
        ", min_cent, max_cent);"
    );

    line!(st, st.float_decl("x1")?, " = center - width * 0.5;");
    line!(st, st.float_decl("x4")?, " = x1 + width;");

    line!(st, st.float_decl("x2")?, " = x1 + (x4 - x1) * 0.25;");
    line!(st, st.float_decl("x3")?, " = x1 + (x4 - x1) * 0.75;");
    line!(st, st.float_decl("y0")?, " = x0;");
    line!(st, fkc.as_str(), " m0 = 1.;");
    line!(st, fkc.as_str(), " m5 = 1.;");

    line!(st, fkc.as_str(), " min_slope = 0.1;");

    line!(st, "mid_adj = mid_adj - 1.;");
    line!(st, "mid_adj = mid_adj * (1. - min_slope);");

    line!(st, st.float_decl("m2")?, " = 1. + mid_adj;");
    line!(st, st.float_decl("m3")?, " = 1. - mid_adj;");
    line!(st, st.float_decl("m1")?, " = 1. + mid_adj * halo;");
    line!(st, st.float_decl("m4")?, " = 1. - mid_adj * halo;");

    line!(st, "if (center <= (x5 + x0) * 0.5)");
    line!(st, "{");
    st.indent();

    line!(
        st,
        st.float_decl("area")?,
        " = (x1 - x0) * (m1 - m0) * 0.5 + "
    );
    line!(
        st,
        "    (x2 - x1) * ((m1 - m0) + (m2 - m1)*0.5) + (center - x2) * (m2 - m0) * 0.5;"
    );
    line!(
        st,
        "m4 = ( -0.5*(x5 - x4)*m5 + (x4 - x3) * (0.5*m3 - m5) + "
    );
    line!(
        st,
        "    (x3 - center) * (m3 - m5) * 0.5 + area ) / ( -0.5*(x5 - x3) );"
    );

    st.dedent();
    line!(st, "}");
    line!(st, "else");
    line!(st, "{");
    st.indent();

    line!(
        st,
        st.float_decl("area")?,
        " = (x5 - x4) * (m4 - m5) * 0.5 + "
    );
    line!(
        st,
        "    (x4 - x3) * ((m4 - m5) + (m3 - m4) * 0.5) + (x3 - center) * (m3 - m5) * 0.5;"
    );
    line!(
        st,
        "m1 = ( -0.5*(x1 - x0)*m0 + (x2 - x1) * (0.5*m2 - m0) + "
    );
    line!(
        st,
        "    (center - x2) * (m2 - m0) * 0.5 + area ) / ( -0.5*(x2 - x0) );"
    );

    st.dedent();
    line!(st, "}");

    line!(
        st,
        st.float_decl("y1")?,
        " = y0 + (m0 + m1) * (x1 - x0) * 0.5;"
    );
    line!(
        st,
        st.float_decl("y2")?,
        " = y1 + (m1 + m2) * (x2 - x1) * 0.5;"
    );
    line!(
        st,
        st.float_decl("y3")?,
        " = y2 + (m2 + m3) * (x3 - x2) * 0.5;"
    );
    line!(
        st,
        st.float_decl("y4")?,
        " = y3 + (m3 + m4) * (x4 - x3) * 0.5;"
    );
    line!(
        st,
        st.float_decl("y5")?,
        " = y4 + (m4 + m5) * (x5 - x4) * 0.5;"
    );

    Ok(channel_suffix(channel))
}

/// The forward midtones of a component.
///
/// Port of `Add_MidsFwd_Shader` (GradingToneOpGPU.cpp:371-447 @ v2.5.2).
fn add_mids_fwd_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    let suffix = add_mids_pre_shader(channel, st, props, style)?;

    if channel != M {
        line!(st, st.float_decl("t")?, " = ", pix, ".", suffix, ";");
        line!(st, st.float_decl("tL")?, " = (t - x0) / (x1 - x0);");
        line!(st, st.float_decl("tM")?, " = (t - x1) / (x2 - x1);");
        line!(st, st.float_decl("tR")?, " = (t - x2) / (x3 - x2);");
        line!(st, st.float_decl("tR2")?, " = (t - x3) / (x4 - x3);");
        line!(st, st.float_decl("tR3")?, " = (t - x4) / (x5 - x4);");

        line!(
            st,
            st.float_decl("fL")?,
            " = tL * (x1 - x0) * ( tL * 0.5 * (m1 - m0) + m0 ) + y0;"
        );
        line!(
            st,
            st.float_decl("fM")?,
            " = tM * (x2 - x1) * ( tM * 0.5 * (m2 - m1) + m1 ) + y1;"
        );
        line!(
            st,
            st.float_decl("fR")?,
            " = tR * (x3 - x2) * ( tR * 0.5 * (m3 - m2) + m2 ) + y2;"
        );
        line!(
            st,
            st.float_decl("fR2")?,
            " = tR2 * (x4 - x3) * ( tR2 * 0.5 * (m4 - m3) + m3 ) + y3;"
        );
        line!(
            st,
            st.float_decl("fR3")?,
            " = tR3 * (x5 - x4) * ( tR3 * 0.5 * (m5 - m4) + m4 ) + y4;"
        );

        line!(st, st.float_decl("res")?, " = (t < x1) ? fL : fM;");
        line!(st, "if (t > x2) res = fR;");
        line!(st, "if (t > x3) res = fR2;");
        line!(st, "if (t > x4) res = fR3;");
        line!(st, "if (t < x0) res = y0 + (t - x0) * m0;");
        line!(st, "if (t > x5) res = y5 + (t - x5) * m5;");
        line!(st, pix, ".", suffix, " = res;");
    } else {
        line!(st, st.color_decl("t")?, " = ", pix, ".rgb;");
        line!(st, st.color_decl("res")?, ";");
        line!(st, st.color_decl("tL")?, " = (t - x0) / (x1 - x0);");
        line!(st, st.color_decl("tM")?, " = (t - x1) / (x2 - x1);");
        line!(st, st.color_decl("tR")?, " = (t - x2) / (x3 - x2);");
        line!(st, st.color_decl("tR2")?, " = (t - x3) / (x4 - x3);");
        line!(st, st.color_decl("tR3")?, " = (t - x4) / (x5 - x4);");

        line!(
            st,
            st.color_decl("fL")?,
            " = tL * (x1 - x0) * ( tL * 0.5 * (m1 - m0) + m0 ) + y0;"
        );
        line!(
            st,
            st.color_decl("fM")?,
            " = tM * (x2 - x1) * ( tM * 0.5 * (m2 - m1) + m1 ) + y1;"
        );
        line!(
            st,
            st.color_decl("fR")?,
            " = tR * (x3 - x2) * ( tR * 0.5 * (m3 - m2) + m2 ) + y2;"
        );
        line!(
            st,
            st.color_decl("fR2")?,
            " = tR2 * (x4 - x3) * ( tR2 * 0.5 * (m4 - m3) + m3 ) + y3;"
        );
        line!(
            st,
            st.color_decl("fR3")?,
            " = tR3 * (x5 - x4) * ( tR3 * 0.5 * (m5 - m4) + m4 ) + y4;"
        );

        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x1) ? fL.",
                c,
                " : fM.",
                c,
                ";"
            );
        }
        for (x, f) in [("x2", "fR"), ("x3", "fR2"), ("x4", "fR3")] {
            for c in ["r", "g", "b"] {
                line!(
                    st, "res.", c, " = (t.", c, " > ", x, ") ? ", f, ".", c, " : res.", c, ";"
                );
            }
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x0) ? y0 + (t.",
                c,
                " - x0) * m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > x5) ? y5 + (t.",
                c,
                " - x5) * m5 : res.",
                c,
                ";"
            );
        }
        line!(st, pix, ".rgb = res;");
    }

    st.dedent();
    line!(st, "}"); // if (mid_adj != 1.)

    st.dedent();
    line!(st, "}"); // local scope
    Ok(())
}

/// The inverse midtones of a component.
///
/// Port of `Add_MidsRev_Shader` (GradingToneOpGPU.cpp:449-625 @ v2.5.2).
fn add_mids_rev_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    let suffix = add_mids_pre_shader(channel, st, props, style)?;
    let fk = st.float_keyword();

    // The segments from the top: (lower knot, upper knot).
    let segments = [("4", "5"), ("3", "4"), ("2", "3"), ("1", "2"), ("0", "1")];

    if channel != M {
        line!(st, fk, " t = ", pix, ".", suffix, ";");
        line!(st, fk, " res;");

        line!(st, "if (t >= y5)");
        line!(st, "{");
        st.indent();
        line!(st, "res = x5 + (t - y5) / m5;");
        st.dedent();
        line!(st, "}");
        for (lo, hi) in segments {
            line!(st, "else if (t >= y", lo, ")");
            line!(st, "{");
            st.indent();
            line!(st, fk, " c = y", lo, " - t;");
            line!(st, fk, " b = m", lo, " * (x", hi, " - x", lo, ");");
            line!(
                st,
                fk,
                " a = 0.5 * (m",
                hi,
                " - m",
                lo,
                ") * (x",
                hi,
                " - x",
                lo,
                ");"
            );
            line!(st, fk, " discrim = sqrt(b * b - 4. * a * c);");
            line!(st, fk, " tmp = (-2. * c) / (discrim + b);");
            line!(st, "res =  tmp * (x", hi, " - x", lo, ") + x", lo, ";");
            st.dedent();
            line!(st, "}");
        }
        line!(st, "else");
        line!(st, "{");
        st.indent();
        line!(st, "res = x0 + (t - y0) / m0;");
        st.dedent();
        line!(st, "}");

        line!(st, pix, ".", suffix, " = res;");
    } else {
        line!(st, st.color_decl("t")?, " = ", pix, ".rgb;");
        for out in ["outL", "outM", "outR", "outR2", "outR3"] {
            line!(st, st.color_decl(out)?, ";");
        }

        // TODO: Would probably be better to call the preceding if-block 3 times
        // rather than trying to do a float3 computation here.  Extra computation is
        // done and it still doesn't avoid the if/else.
        for ((lo, hi), out) in segments
            .into_iter()
            .zip(["outR3", "outR2", "outR", "outM", "outL"])
        {
            line!(st, "{");
            st.indent();
            line!(st, st.float3_decl("c")?, " = y", lo, " - t;");
            line!(
                st,
                st.float_decl("b")?,
                " = m",
                lo,
                " * (x",
                hi,
                " - x",
                lo,
                ");"
            );
            line!(
                st,
                st.float_decl("a")?,
                " = 0.5 * (m",
                hi,
                " - m",
                lo,
                ") * (x",
                hi,
                " - x",
                lo,
                ");"
            );
            line!(
                st,
                st.float3_decl("discrim")?,
                " = sqrt(b * b - 4. * a * c);"
            );
            line!(st, st.float3_decl("tmp")?, " = (-2. * c) / (discrim + b);");
            line!(st, out, " =  tmp * (x", hi, " - x", lo, ") + x", lo, ";");
            st.dedent();
            line!(st, "}");
        }

        line!(st, st.color_decl("res")?, ";");
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y1) ? outL.",
                c,
                " : outM.",
                c,
                ";"
            );
        }
        for (y, out) in [("y2", "outR"), ("y3", "outR2"), ("y4", "outR3")] {
            for c in ["r", "g", "b"] {
                line!(
                    st, "res.", c, " = (t.", c, " > ", y, ") ? ", out, ".", c, " : res.", c, ";"
                );
            }
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y0) ? x0 + (t.",
                c,
                " - y0) * m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > y5) ? x5 + (t.",
                c,
                " - y5) * m5 : res.",
                c,
                ";"
            );
        }
        line!(st, pix, ".rgb = res;");
    }

    st.dedent();
    line!(st, "}"); // if (mid_adj != 1.)

    st.dedent();
    line!(st, "}"); // local scope
    Ok(())
}

/// Opens the highlights' or shadows' block: the curve's ends and middle, and the component's
/// value (the highlights' mirrored).
///
/// Port of `Add_HighlightShadowPre_Shader` (GradingToneOpGPU.cpp:627-683 @ v2.5.2).
fn add_highlight_shadow_pre_shader(
    st: &GpuShaderText,
    channel: Channel,
    props: &GtProperties,
    is_shadow: bool,
) -> Result<&'static str> {
    // TODO: Everything in here should move to C++ (doesn't vary per pixel).

    let start = if is_shadow {
        &props.shadows_s
    } else {
        &props.highlights_s
    };
    let pivot = if is_shadow {
        &props.shadows_w
    } else {
        &props.highlights_w
    };
    let channel_value = match (channel, is_shadow) {
        (R, true) => &props.shadows_r,
        (R, false) => &props.highlights_r,
        (G, true) => &props.shadows_g,
        (G, false) => &props.highlights_g,
        (B, true) => &props.shadows_b,
        (B, false) => &props.highlights_b,
        (M, true) => &props.shadows_m,
        (M, false) => &props.highlights_m,
    };

    line!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    if is_shadow {
        line!(st, st.float_decl("x0")?, " = ", pivot.as_slice(), ";");
        line!(st, st.float_decl("x2")?, " = ", start.as_slice(), ";");
        st.declare_var_f32("m2", 1.0)?;
    } else {
        line!(st, st.float_decl("x0")?, " = ", start.as_slice(), ";");
        line!(st, st.float_decl("x2")?, " = ", pivot.as_slice(), ";");
        st.declare_var_f32("m0", 1.0)?;
    }
    line!(st, st.float_decl("y0")?, " = x0;");
    line!(st, st.float_decl("y2")?, " = x2;");
    line!(st, st.float_decl("x1")?, " = x0 + (x2 - x0) * 0.5;");

    line!(
        st,
        st.float_decl("val")?,
        " = ",
        channel_value.as_slice(),
        ";"
    );
    if !is_shadow {
        line!(st, "val = 2. - val;");
    }
    Ok(channel_suffix(channel))
}

/// The curve's middle point `y1`, which both evaluations start with.
const Y1_LINE: &str = " y1 = ( 0.5 / (x2 - x0) ) * \
    ( (2.*y0 + m0 * (x1 - x0)) * (x2 - x1) + (2.*y2 - m2 * (x2 - x1)) * (x1 - x0) );";

/// The highlights' or shadows' curve.
///
/// Port of `Add_FauxCubicFwdEval_Shader` (GradingToneOpGPU.cpp:685-735 @ v2.5.2).
fn add_faux_cubic_fwd_eval_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    suffix: &str,
) -> Result<()> {
    let fk = st.float_keyword();
    line!(st, fk, Y1_LINE);

    if channel != M {
        line!(st, fk, " t = ", pix, ".", suffix, ";");
        line!(st, fk, " res, tL, tR, fL, fR;");
    } else {
        line!(st, st.color_decl("t")?, " = ", pix, ".", suffix, ";");
        for v in ["res", "tL", "tR", "fL", "fR"] {
            line!(st, st.color_decl(v)?, ";");
        }
    }

    line!(st, "tL = (t - x0) / (x1 - x0);");
    line!(st, "tR = (t - x1) / (x2 - x1);");
    line!(
        st,
        "fL = y0 * (1. - tL*tL) + y1 * tL*tL + m0 * (1. - tL) * tL * (x1 - x0);"
    );
    line!(
        st,
        "fR = y1 * (1. - tR)*(1. - tR) + y2 * (2. - tR)*tR + m2 * (tR - 1.)*tR * (x2 - x1);"
    );

    if channel != M {
        line!(st, "res = (t < x1) ? fL : fR;");
        line!(st, "res = (t < x0) ? y0 + (t - x0) * m0 : res;");
        line!(st, "res = (t > x2) ? y2 + (t - x2) * m2 : res;");
    } else {
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x1) ? fL.",
                c,
                " : fR.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x0) ? y0 + (t.",
                c,
                " - x0) * m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > x2) ? y2 + (t.",
                c,
                " - x2) * m2 : res.",
                c,
                ";"
            );
        }
    }
    line!(st, pix, ".", suffix, " = res;");
    Ok(())
}

/// The inverse of the highlights' or shadows' curve.
///
/// Port of `Add_FauxCubicRevEval_Shader` (GradingToneOpGPU.cpp:737-793 @ v2.5.2).
fn add_faux_cubic_rev_eval_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    suffix: &str,
) -> Result<()> {
    let fk = st.float_keyword();
    line!(st, fk, Y1_LINE);

    if channel != M {
        line!(st, fk, " t = ", pix, ".", suffix, ";");
        line!(st, fk, " res, cL, cR, discrimL, discrimR, outL, outR;");
    } else {
        line!(st, st.color_decl("t")?, " = ", pix, ".", suffix, ";");
        for v in ["res", "cL", "cR", "discrimL", "discrimR", "outL", "outR"] {
            line!(st, st.color_decl(v)?, ";");
        }
    }

    line!(st, "cL = y0 - t;");
    line!(st, fk, " bL = m0 * (x1 - x0);");
    line!(st, fk, " aL = y1 - y0 - m0 * (x1 - x0);");
    line!(st, "discrimL = sqrt( bL * bL - 4. * aL * cL );");
    line!(
        st,
        "outL = (-2. * cL) / ( discrimL + bL ) * (x1 - x0) + x0;"
    );
    line!(st, "cR = y1 - t;");
    line!(st, fk, " bR = 2.*y2 - 2.*y1 - m2 * (x2 - x1);");
    line!(st, fk, " aR = y1 - y2 + m2 * (x2 - x1);");
    line!(st, "discrimR = sqrt( bR * bR - 4. * aR * cR );");
    line!(
        st,
        "outR = (-2. * cR) / ( discrimR + bR ) * (x2 - x1) + x1;"
    );
    if channel != M {
        line!(st, "res = (t < y1) ? outL : outR;");
        line!(st, "res = (t < y0) ? x0 + (t - y0) / m0 : res;");
        line!(st, "res = (t > y2) ? x2 + (t - y2) / m2 : res;");
    } else {
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y1) ? outL.",
                c,
                " : outR.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y0) ? x0 + (t.",
                c,
                " - y0) / m0 : res.",
                c,
                ";"
            );
        }
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " > y2) ? x2 + (t.",
                c,
                " - y2) / m2 : res.",
                c,
                ";"
            );
        }
    }
    line!(st, pix, ".", suffix, " = res;");
    Ok(())
}

/// The highlights or shadows of a component, forward (`inverse` false) or inverse: the curve
/// below a value of 1 and its inverse above it, or the other way around.
///
/// Port of `Add_HighlightShadowFwd_Shader` and `Add_HighlightShadowRev_Shader`
/// (GradingToneOpGPU.cpp:795-886 @ v2.5.2).
fn add_highlight_shadow_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    is_shadow: bool,
    props: &GtProperties,
    inverse: bool,
) -> Result<()> {
    let suffix = add_highlight_shadow_pre_shader(st, channel, props, is_shadow)?;
    let fk = st.float_keyword();
    let m = if is_shadow { " m0" } else { " m2" };

    line!(st, "if (val < 1.)");
    line!(st, "{");
    st.indent();
    line!(st, fk, m, " = max( 0.01, val );");
    if inverse {
        add_faux_cubic_rev_eval_shader(pix, st, channel, suffix)?; // Rev
    } else {
        add_faux_cubic_fwd_eval_shader(pix, st, channel, suffix)?; // Fwd
    }
    st.dedent();
    line!(st, "}");

    line!(st, "else if (val > 1.)");
    line!(st, "{");
    st.indent();
    line!(st, fk, m, " = max( 0.01, 2. - val );");
    if inverse {
        add_faux_cubic_fwd_eval_shader(pix, st, channel, suffix)?; // Fwd
    } else {
        add_faux_cubic_rev_eval_shader(pix, st, channel, suffix)?; // Rev
    }
    st.dedent();
    line!(st, "}");

    st.dedent();
    line!(st, "}"); // establish scope
    Ok(())
}

/// Opens the whites' or blacks' block: the curve's ends, the component's value as the slope at
/// the far end, and the pixel.
///
/// Port of `Add_WhiteBlackPre_Shader` (GradingToneOpGPU.cpp:888-951 @ v2.5.2).
fn add_white_black_pre_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    is_black: bool,
    props: &GtProperties,
) -> Result<&'static str> {
    let start = if is_black {
        &props.blacks_s
    } else {
        &props.whites_s
    };
    let width = if is_black {
        &props.blacks_w
    } else {
        &props.whites_w
    };
    let channel_value = match (channel, is_black) {
        (R, true) => &props.blacks_r,
        (R, false) => &props.whites_r,
        (G, true) => &props.blacks_g,
        (G, false) => &props.whites_g,
        (B, true) => &props.blacks_b,
        (B, false) => &props.whites_b,
        (M, true) => &props.blacks_m,
        (M, false) => &props.whites_m,
    };
    let fk = st.float_keyword();
    let fkc = st.float_keyword_const();

    line!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    if !is_black {
        line!(st, fk, " x0 = ", start.as_slice(), ";");
        line!(st, fk, " x1 = x0 + ", width.as_slice(), ";");
        line!(st, fkc.as_str(), " m0 = 1.;");
        line!(st, fk, " y0 = x0;");
        line!(st, fk, " m1 = ", channel_value.as_slice(), ";");
        line!(st, fk, " mtest = m1;");
    } else {
        line!(st, fk, " x1 = ", start.as_slice(), ";");
        line!(st, fk, " x0 = x1 - ", width.as_slice(), ";");
        line!(st, fkc.as_str(), " m1 = 1.;");
        line!(st, fk, " y1 = x1;");
        line!(st, fk, " m0 = ", channel_value.as_slice(), ";");
        line!(st, "m0 = 2. - m0;"); // increasing blacks control should lighten
        line!(st, fk, " mtest = m0;");
    }

    if channel != M {
        line!(st, fk, " t = ", pix, ".", channel_suffix(channel), ";");
    } else {
        line!(st, st.color_decl("t")?, " = ", pix, ".rgb;");
    }
    Ok(channel_suffix(channel))
}

/// The whites' or blacks' curve, with the line above it when `linear_extrap`.
///
/// Port of `Add_WBFwd_Shader` (GradingToneOpGPU.cpp:953-982 @ v2.5.2).
fn add_wb_fwd_shader(channel: Channel, linear_extrap: bool, st: &GpuShaderText) -> Result<()> {
    let fk = st.float_keyword();
    if channel != M {
        line!(st, fk, " tlocal = (t - x0) / (x1 - x0);");
        line!(
            st,
            fk,
            " res = tlocal * (x1 - x0) * ( tlocal * 0.5 * (m1 - m0) + m0 ) + y0;"
        );
        line!(st, "res = (t < x0) ? y0 + (t - x0) * m0 : res;");
    } else {
        line!(st, st.float3_decl("tlocal")?, " = (t - x0) / (x1 - x0);");
        line!(
            st,
            st.color_decl("res")?,
            " = tlocal * (x1 - x0) * ( tlocal * 0.5 * (m1 - m0) + m0 ) + y0;"
        );
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < x0) ? y0 + (t.",
                c,
                " - x0) * m0 : res.",
                c,
                ";"
            );
        }
    }
    if linear_extrap {
        if channel != M {
            line!(st, "res = (t > x1) ? y1 + (t - x1) * m1 : res;");
        } else {
            for c in ["r", "g", "b"] {
                line!(
                    st,
                    "res.",
                    c,
                    " = (t.",
                    c,
                    " > x1) ? y1 + (t.",
                    c,
                    " - x1) * m1 : res.",
                    c,
                    ";"
                );
            }
        }
    }
    Ok(())
}

/// The inverse of the whites' or blacks' curve, with the line above it when `linear_extrap`.
///
/// Port of `Add_WBRev_Shader` (GradingToneOpGPU.cpp:984-1020 @ v2.5.2).
fn add_wb_rev_shader(channel: Channel, linear_extrap: bool, st: &GpuShaderText) -> Result<()> {
    let fk = st.float_keyword();
    line!(st, fk, " a = 0.5 * (m1 - m0) * (x1 - x0);");
    line!(st, fk, " b = m0 * (x1 - x0);");
    if channel != M {
        line!(st, fk, " c = y0 - t;");
        line!(st, fk, " discrim = sqrt( b * b - 4. * a * c );");
        line!(st, fk, " tmp = ( -2. * c ) / ( discrim + b );");
        line!(st, fk, " res = tmp * (x1 - x0) + x0;");
        line!(st, "res = (t < y0) ? x0 + (t - y0) / m0 : res;");
    } else {
        line!(st, st.float3_decl("c")?, " = y0 - t;");
        line!(
            st,
            st.float3_decl("discrim")?,
            " = sqrt( b * b - 4. * a * c );"
        );
        line!(
            st,
            st.float3_decl("tmp")?,
            " = ( -2. * c ) / ( discrim + b );"
        );
        line!(st, st.color_decl("res")?, " = tmp * (x1 - x0) + x0;");
        for c in ["r", "g", "b"] {
            line!(
                st,
                "res.",
                c,
                " = (t.",
                c,
                " < y0) ? x0 + (t.",
                c,
                " - y0) / m0 : res.",
                c,
                ";"
            );
        }
    }
    if linear_extrap {
        if channel != M {
            line!(st, "res = (t > y1) ? x1 + (t - y1) / m1 : res;");
        } else {
            // TODO: When m1 = 1., y1=x1, this becomes t.
            for c in ["r", "g", "b"] {
                line!(
                    st,
                    "res.",
                    c,
                    " = (t.",
                    c,
                    " > y1) ? x1 + (t.",
                    c,
                    " - y1) / m1 : res.",
                    c,
                    ";"
                );
            }
        }
    }
    Ok(())
}

/// The whites' quadratic extrapolation's coefficients, and the pixel back from the gain.
///
/// Port of `Add_WBExtrapPre_Shader` (GradingToneOpGPU.cpp:1022-1034 @ v2.5.2).
fn add_wb_extrap_pre_shader(st: &GpuShaderText) {
    let fk = st.float_keyword();
    line!(st, "res = (res - x0) / gain + x0;");
    // Quadratic extrapolation for better HDR control.
    line!(st, fk, " new_y1 = (x1 - x0) / gain + x0;");
    line!(st, fk, " xd = x0 + (x1 - x0) * 0.99;");
    line!(st, fk, " md = m0 + (xd - x0) * (m1 - m0) / (x1 - x0);");
    line!(st, "md = 1. / md;");
    line!(st, fk, " aa = 0.5 * (1. / m1 - md) / (x1 - xd);");
    line!(st, fk, " bb = 1. / m1 - 2. * aa * x1;");
    line!(st, fk, " cc = new_y1 - bb * x1 - aa * x1 * x1;");
    line!(st, "t = (t - x0) / gain + x0;");
}

/// Writes the result back to the pixel.
fn store_res(pix: &[u8], st: &GpuShaderText, channel: Channel, suffix: &str) {
    if channel != M {
        line!(st, pix, ".", suffix, " = res;");
    } else {
        line!(st, pix, ".rgb = res;");
    }
}

/// The decreasing case's head: the slope at the far end, limited, and the near end's value.
fn add_wb_decreasing_head(st: &GpuShaderText, is_black: bool) {
    let fk = st.float_keyword();
    if !is_black {
        line!(st, "m1 = max( 0.01, m1 );");
        line!(st, fk, " y1 = y0 + (m0 + m1) * (x1 - x0) * 0.5;");
    } else {
        line!(st, "m0 = max( 0.01, m0 );");
        line!(st, fk, " y0 = y1 - (m0 + m1) * (x1 - x0) * 0.5;");
    }
}

/// The increasing case's head: the slope mirrored and limited, the gain, and the pixel scaled
/// by it.
fn add_wb_increasing_head(st: &GpuShaderText, is_black: bool) {
    let fk = st.float_keyword();
    if !is_black {
        line!(st, "m1 = 2. - m1;");
        line!(st, "m1 = max( 0.01, m1 );");
        line!(st, fk, " gain = (m0 + m1) * 0.5;");
        line!(st, "t = (t - x0) * gain + x0;");
    } else {
        line!(st, "m0 = 2. - m0;");
        line!(st, "m0 = max( 0.01, m0 );");
        line!(st, fk, " y0 = y1 - (m0 + m1) * (x1 - x0) * 0.5;");
        line!(st, fk, " gain = (m0 + m1) * 0.5;");
        line!(st, "t = (t - x1) * gain + x1;");
    }
}

/// The forward whites or blacks of a component.
///
/// Port of `Add_WhiteBlackFwd_Shader` (GradingToneOpGPU.cpp:1036-1132 @ v2.5.2).
fn add_white_black_fwd_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    is_black: bool,
    props: &GtProperties,
) -> Result<()> {
    let suffix = add_white_black_pre_shader(pix, st, channel, is_black, props)?;

    // Slope is decreasing case.

    line!(st, "if (mtest < 1.)");
    line!(st, "{");
    st.indent();
    add_wb_decreasing_head(st, is_black);

    add_wb_fwd_shader(channel, true, st)?;

    store_res(pix, st, channel, suffix);
    st.dedent();
    line!(st, "}");

    // Slope is increasing case.

    line!(st, "else if (mtest > 1.)");
    line!(st, "{");
    st.indent();
    add_wb_increasing_head(st, is_black);

    add_wb_rev_shader(channel, is_black, st)?;

    if !is_black {
        add_wb_extrap_pre_shader(st);

        if channel != M {
            line!(st, "if (t > x1) res = (aa * t  + bb) * t + cc;");
        } else {
            for c in ["r", "g", "b"] {
                line!(
                    st,
                    "if (t.",
                    c,
                    " > x1) res.",
                    c,
                    " = (aa * t.",
                    c,
                    " + bb) * t.",
                    c,
                    " + cc;"
                );
            }
        }
    } else {
        line!(st, "res = (res - x1) / gain + x1;");
    }

    store_res(pix, st, channel, suffix);
    st.dedent();
    line!(st, "}"); // else if (mtest > 1.)

    st.dedent();
    line!(st, "}"); // establish scope so local variable names won't conflict
    Ok(())
}

/// The inverse whites or blacks of a component.
///
/// Port of `Add_WhiteBlackRev_Shader` (GradingToneOpGPU.cpp:1134-1238 @ v2.5.2).
fn add_white_black_rev_shader(
    pix: &[u8],
    st: &GpuShaderText,
    channel: Channel,
    is_black: bool,
    props: &GtProperties,
) -> Result<()> {
    let suffix = add_white_black_pre_shader(pix, st, channel, is_black, props)?;
    let fk = st.float_keyword();

    // Slope is decreasing case.

    line!(st, "if (mtest < 1.)");
    line!(st, "{");
    st.indent();
    add_wb_decreasing_head(st, is_black);

    add_wb_rev_shader(channel, true, st)?;

    store_res(pix, st, channel, suffix);
    st.dedent();
    line!(st, "}");

    // Slope is increasing case.

    line!(st, "else if (mtest > 1.)");
    line!(st, "{");
    st.indent();
    add_wb_increasing_head(st, is_black);

    add_wb_fwd_shader(channel, is_black, st)?;

    if !is_black {
        add_wb_extrap_pre_shader(st);

        if channel != M {
            line!(st, fk, " c = cc - t;");
            line!(st, fk, " discrim = sqrt( bb * bb - 4. * aa * c );");
            line!(st, fk, " res1 = ( -2. * c ) / ( discrim + bb );");
            line!(st, fk, " brk = (aa * x1 + bb) * x1 + cc;");
            line!(st, "res = (t < brk) ? res : res1;");
        } else {
            line!(st, st.float3_decl("c")?, " = cc - t;");
            line!(
                st,
                st.float3_decl("discrim")?,
                " = sqrt( bb * bb - 4. * aa * c );"
            );
            line!(
                st,
                st.color_decl("res1")?,
                " = ( -2. * c ) / ( discrim + bb );"
            );
            line!(st, fk, " brk = (aa * x1 + bb) * x1 + cc;");
            for c in ["r", "g", "b"] {
                line!(
                    st,
                    "res.",
                    c,
                    " = (t.",
                    c,
                    " < brk) ? res.",
                    c,
                    " : res1.",
                    c,
                    ";"
                );
            }
        }
    } else {
        line!(st, "res = (res - x1) / gain + x1;");
    }

    store_res(pix, st, channel, suffix);
    st.dedent();
    line!(st, "}"); // else if (mtest > 1.)

    st.dedent();
    line!(st, "}"); // establish scope so local variable names won't conflict
    Ok(())
}

/// Opens the s-contrast's block, computes the slope and opens the top end's block with its
/// control points.
///
/// Port of `Add_SContrastTopPre_Shader` (GradingToneOpGPU.cpp:1240-1294 @ v2.5.2).
fn add_s_contrast_top_pre_shader(
    pix: &[u8],
    st: &GpuShaderText,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    let (_top, top_sc, _bottom, pivot) = from_style(style);
    let top_point = cpp_to_string(top_sc);
    let fk = st.float_keyword();
    let fkc = st.float_keyword_const();

    line!(st, fk, " contrast = ", props.s_contrast.as_slice(), ";");
    line!(st, "if (contrast != 1.)");
    line!(st, "{");
    st.indent();

    // Limit the range of values to prevent reversals.
    line!(
        st,
        "contrast = (contrast > 1.) ? 1. / (1.8125 - 0.8125 * min( contrast, 1.99 )) : \
         0.28125 + 0.71875 * max( contrast, 0.01 );"
    );
    line!(
        st,
        fkc.as_str(),
        " pivot = ",
        cpp_to_string(pivot).as_str(),
        ";"
    );

    line!(st, st.color_decl("t")?, " = ", pix, ".rgb;");

    // Top end
    line!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    line!(st, fkc.as_str(), " x3 = ", top_point.as_str(), ";");
    line!(st, fkc.as_str(), " y3 = ", top_point.as_str(), ";");
    line!(st, fkc.as_str(), " y0 = pivot + (y3 - pivot) * 0.25;");
    line!(st, fk, " m0 = contrast;");
    line!(st, fk, " x0 = pivot + (y0 - pivot) / m0;");
    line!(st, fk, " min_width = (x3 - x0) * 0.3;");
    line!(st, fk, " m3 = 1. / m0;");
    // NB: Due to the if (contrast != 1.) clause above, m0 != m3.
    line!(st, fk, " center = (y3 - y0 - m3*x3 + m0*x0) / (m0 - m3);");
    line!(st, fk, " x1 = x0;");
    line!(st, fk, " x2 = 2. * center - x1;");
    line!(st, "if (x2 > x3)");
    line!(st, "{");
    line!(st, "  x2 = x3;");
    line!(st, "  x1 = 2. * center - x2;");
    line!(st, "}");
    line!(st, "else if ((x2 - x1) < min_width)");
    line!(st, "{");
    line!(st, "  x2 = x1 + min_width;");
    line!(st, "  float new_center = (x2 + x1) * 0.5;");
    line!(
        st,
        "  m3 = (y3 - y0 + m0*x0 - new_center * m0) / (x3 - new_center);"
    );
    line!(st, "}");
    line!(st, fk, " y1 = y0;");
    line!(st, fk, " y2 = y1 + (m0 + m3) * (x2 - x1) * 0.5;");

    // TODO: the above should not be in the GLSL (is not per-pixel)
    Ok(())
}

/// Opens the s-contrast bottom end's block with its control points.
///
/// Port of `Add_SContrastBottomPre_Shader` (GradingToneOpGPU.cpp:1296-1330 @ v2.5.2).
fn add_s_contrast_bottom_pre_shader(st: &GpuShaderText, style: GradingStyle) {
    let (_top, _top_sc, bottom, _pivot) = from_style(style);
    let bottom_point = cpp_to_string(bottom);
    let fk = st.float_keyword();
    let fkc = st.float_keyword_const();

    // Bottom end
    line!(st, "{"); // establish scope so local variable names won't conflict
    st.indent();
    line!(st, fkc.as_str(), " x0 = ", bottom_point.as_str(), ";");
    line!(st, fkc.as_str(), " y0 = ", bottom_point.as_str(), ";");
    line!(st, fkc.as_str(), " y3 = pivot - (pivot - y0) * 0.25;");
    line!(st, fk, " m3 = contrast;");
    line!(st, fk, " x3 = pivot - (pivot - y3) / m3;");
    line!(st, fk, " min_width = (x3 - x0) * 0.3;");
    line!(st, fk, " m0 = 1. / m3;");
    line!(st, fk, " center = (y3 - y0 - m3*x3 + m0*x0) / (m0 - m3);");
    line!(st, fk, " x2 = x3;");
    line!(st, fk, " x1 = 2. * center - x2;");
    line!(st, "if (x1 < x0)");
    line!(st, "{");
    line!(st, "  x1 = x0;");
    line!(st, "  x2 = 2. * center - x1;");
    line!(st, "}");
    line!(st, "else if ((x2 - x1) < min_width)");
    line!(st, "{");
    line!(st, "  x1 = x2 - min_width;");
    line!(st, "  float new_center = (x2 + x1) * 0.5;");
    line!(
        st,
        "  m0 = (y3 - y0 - m3*x3 + new_center * m3) / (new_center - x0);"
    );
    line!(st, "}");
    line!(st, fk, " y2 = y3;");
    line!(st, fk, " y1 = y2 - (m0 + m3) * (x2 - x1) * 0.5;");

    // TODO: the above should not be in the GLSL (is not per-pixel)
}

/// The forward s-contrast.
///
/// Port of `Add_SContrastFwd_Shader` (GradingToneOpGPU.cpp:1332-1371 @ v2.5.2).
fn add_s_contrast_fwd_shader(
    pix: &[u8],
    st: &GpuShaderText,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    add_s_contrast_top_pre_shader(pix, st, props, style)?;

    line!(st, pix, ".rgb = (t - pivot) * contrast + pivot;");

    line!(st, st.float3_decl("tR")?, " = (t - x1) / (x2 - x1);");
    line!(
        st,
        st.color_decl("res")?,
        " = tR * (x2 - x1) * ( tR * 0.5 * (m3 - m0) + m0 ) + y1;"
    );

    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > x1) ? res.",
            c,
            " : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > x2) ? y2 + (t.",
            c,
            " - x2) * m3 : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    st.dedent();
    line!(st, "}"); // end local scope

    add_s_contrast_bottom_pre_shader(st, style);

    line!(st, st.float3_decl("tR")?, " = (t - x1) / (x2 - x1);");
    line!(
        st,
        st.color_decl("res")?,
        " = tR * (x2 - x1) * ( tR * 0.5 * (m3 - m0) + m0 ) + y1;"
    );

    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " < x2) ? res.",
            c,
            " : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " < x1) ? y1 + (t.",
            c,
            " - x1) * m0 : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    st.dedent();
    line!(st, "}"); // end local scope

    st.dedent();
    line!(st, "}"); // end if contrast != 1.
    Ok(())
}

/// The inverse s-contrast.
///
/// Port of `Add_SContrastRev_Shader` (GradingToneOpGPU.cpp:1373-1418 @ v2.5.2).
fn add_s_contrast_rev_shader(
    pix: &[u8],
    st: &GpuShaderText,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    add_s_contrast_top_pre_shader(pix, st, props, style)?;

    line!(st, pix, ".rgb = (t - pivot) / contrast + pivot;");

    let quadratic = |st: &GpuShaderText| -> Result<()> {
        line!(st, st.float3_decl("c")?, " = y1 - t;");
        line!(st, st.float_decl("b")?, " = m0 * (x2 - x1);");
        line!(st, st.float_decl("a")?, " = (m3 - m0) * 0.5 * (x2 - x1);");
        line!(
            st,
            st.float3_decl("discrim")?,
            " = sqrt( b * b - 4. * a * c );"
        );
        line!(
            st,
            st.color_decl("res")?,
            " = (x2 - x1) * (-2. * c) / ( discrim + b ) + x1;"
        );
        Ok(())
    };
    quadratic(st)?;

    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y1) ? res.",
            c,
            " : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y2) ? x2 + (t.",
            c,
            " - y2) / m3 : ",
            pix,
            ".rgb.",
            c,
            ";"
        );
    }
    st.dedent();
    line!(st, "}"); // end local scope

    add_s_contrast_bottom_pre_shader(st, style);

    quadratic(st)?;

    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y2) ? ",
            pix,
            ".rgb.",
            c,
            " : res.",
            c,
            ";"
        );
    }
    for c in ["r", "g", "b"] {
        line!(
            st,
            pix,
            ".rgb.",
            c,
            " = (t.",
            c,
            " > y1) ? ",
            pix,
            ".rgb.",
            c,
            " : x1 + (t.",
            c,
            " - y1) / m0;"
        );
    }
    st.dedent();
    line!(st, "}"); // end local scope

    st.dedent();
    line!(st, "}"); // end if contrast != 1.
    Ok(())
}

/// The forward program.
///
/// Port of `AddGTForwardShader` (GradingToneOpGPU.cpp:1420-1471 @ v2.5.2).
fn add_gt_forward_shader(
    pix: &[u8],
    st: &GpuShaderText,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    if style == GradingStyle::Lin {
        // NB:  Although the linToLog and logToLin are correct inverses, the limits of
        // floating-point arithmetic cause errors in the lowest bit of the round trip.
        add_lin_to_log_shader(pix, st)?;
    }

    for ch in [R, G, B, M] {
        add_mids_fwd_shader(pix, st, ch, props, style)?;
    }
    for ch in [R, G, B, M] {
        add_highlight_shadow_shader(pix, st, ch, false, props, false)?;
    }
    for ch in [R, G, B, M] {
        add_white_black_fwd_shader(pix, st, ch, false, props)?;
    }
    for ch in [R, G, B, M] {
        add_highlight_shadow_shader(pix, st, ch, true, props, false)?;
    }
    for ch in [R, G, B, M] {
        add_white_black_fwd_shader(pix, st, ch, true, props)?;
    }

    add_s_contrast_fwd_shader(pix, st, props, style)?;

    if style == GradingStyle::Lin {
        add_log_to_lin_shader(pix, st)?;
    }

    // TODO: The grading controls at high values are able to push values above the max
    // half-float at which point they overflow to infinity.  Currently the ACES view
    // transforms make black for Inf but even if it is probably not desirable to output Inf
    // under any circumstances.
    line!(st, pix, " = min( ", pix, ", 65504. );");
    Ok(())
}

/// The inverse program.
///
/// Port of `AddGTInverseShader` (GradingToneOpGPU.cpp:1473-1524 @ v2.5.2).
fn add_gt_inverse_shader(
    pix: &[u8],
    st: &GpuShaderText,
    props: &GtProperties,
    style: GradingStyle,
) -> Result<()> {
    if style == GradingStyle::Lin {
        // NB:  Although the linToLog and logToLin are correct inverses, the limits of
        // floating-point arithmetic cause errors in the lowest bit of the round trip.
        add_lin_to_log_shader(pix, st)?;
    }

    add_s_contrast_rev_shader(pix, st, props, style)?;

    for ch in [M, R, G, B] {
        add_white_black_rev_shader(pix, st, ch, true, props)?;
    }
    for ch in [M, R, G, B] {
        add_highlight_shadow_shader(pix, st, ch, true, props, true)?;
    }
    for ch in [M, R, G, B] {
        add_white_black_rev_shader(pix, st, ch, false, props)?;
    }
    for ch in [M, R, G, B] {
        add_highlight_shadow_shader(pix, st, ch, false, props, true)?;
    }
    for ch in [M, R, G, B] {
        add_mids_rev_shader(pix, st, ch, props, style)?;
    }

    if style == GradingStyle::Lin {
        add_log_to_lin_shader(pix, st)?;
    }

    // TODO: The grading controls at high values are able to push values above the max
    // half-float at which point they overflow to infinity.  Currently the ACES view
    // transforms make black for Inf but even if it is probably not desirable to output Inf
    // under any circumstances.
    line!(st, pix, " = min( ", pix, ", 65504. );");
    Ok(())
}

/// Adds the op's shader code (and, for a dynamic op, its uniforms and its dynamic property)
/// to `shader_creator`; nothing for values that change nothing, unless the op is dynamic.
///
/// Port of `GetGradingToneGPUShaderProgram` (GradingToneOpGPU.cpp:1528-1598 @ v2.5.2).
pub fn get_grading_tone_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    gt_data: &GradingToneOpData,
) -> Result<()> {
    let dyn_ = gt_data.is_dynamic() && shader_creator.language() != GpuLanguage::Osl1;
    if !dyn_ {
        let prop_gt = gt_data.get_dynamic_property_internal();
        if prop_gt.get_local_bypass() {
            return Ok(());
        }
    }

    if gt_data.is_dynamic() && shader_creator.language() == GpuLanguage::Osl1 {
        shader_creator.log_warning(format!(
            "The dynamic properties are not yet supported by the 'Open Shading language (OSL)' \
             translation: The '{OP_PREFIX}' dynamic property is replaced by a local variable."
        ));
    }

    let style = gt_data.get_style();
    let dir = gt_data.get_direction();

    let st = GpuShaderText::new(shader_creator.language());
    st.indent();

    line!(st, "");
    line!(
        st,
        "// Add GradingTone '",
        grading_style_to_string(style),
        "' ",
        transform_direction_to_string(dir),
        " processing"
    );
    line!(st, "");
    line!(st, "{");
    st.indent();

    // Properties hold shader variables names and are initialized with undecorated names
    // suitable for local variables.
    let mut properties = GtProperties::default();
    add_gt_properties(shader_creator, &st, gt_data, &mut properties, dyn_)?;

    if dyn_ {
        line!(st, "if (!", st.cast_to_bool(&properties.local_bypass), ")");
        line!(st, "{");
        st.indent();
    }

    let pix = shader_creator.pixel_name().to_vec();
    match dir {
        TransformDirection::Forward => add_gt_forward_shader(&pix, &st, &properties, style)?,
        TransformDirection::Inverse => add_gt_inverse_shader(&pix, &st, &properties, style)?,
    }

    if dyn_ {
        st.dedent();
        line!(st, "}");
    }

    st.dedent();
    line!(st, "}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}
