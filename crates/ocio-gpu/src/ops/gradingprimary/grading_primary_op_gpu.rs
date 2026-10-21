// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/gradingprimary/GradingPrimaryOpGPU.h` and
//! `GradingPrimaryOpGPU.cpp` @ v2.5.2, the GradingPrimary op's GPU writer.
//!
//! The values' pre-render form (and the pivots, clamps and saturation) go into the shader as
//! local constants, or, for a dynamic op (in every language but OSL), as uniforms bound to a
//! copy of the op's dynamic property that the shader description holds, so the application
//! can change the values after the extraction; a boolean uniform then skips the op's block
//! when the values change nothing. OSL, which has no dynamic properties, gets the constants
//! and a warning. The shader of each style and direction computes in the shading language
//! what the CPU renderers compute.

use std::sync::Arc;

use ocio_ops::Result;
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::logging::log_warning;
use ocio_ops::open_color_types::{
    GradingStyle, TransformDirection, grading_style_to_string, transform_direction_to_string,
};
use ocio_ops::ops::gradingprimary::grading_primary_op_data::GradingPrimaryOpData;

use crate::gpu_shader::Getter;
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::{GpuShaderText, build_resource_name};
use crate::open_color_types::GpuLanguage;

/// The names of the values in the shader code: undecorated, for local variables, until a
/// dynamic op makes them uniforms' names.
///
/// Port of `GPProperties` (GradingPrimaryOpGPU.cpp:16-33 @ v2.5.2).
struct GpProperties {
    brightness: Vec<u8>,
    contrast: Vec<u8>,
    gamma: Vec<u8>,
    exposure: Vec<u8>,
    offset: Vec<u8>,
    slope: Vec<u8>,

    pivot: Vec<u8>,
    pivot_black: Vec<u8>,
    pivot_white: Vec<u8>,
    clamp_black: Vec<u8>,
    clamp_white: Vec<u8>,
    saturation: Vec<u8>,

    local_bypass: Vec<u8>,
}

impl Default for GpProperties {
    fn default() -> Self {
        GpProperties {
            brightness: b"brightness".to_vec(),
            contrast: b"contrast".to_vec(),
            gamma: b"gamma".to_vec(),
            exposure: b"exposure".to_vec(),
            offset: b"offset".to_vec(),
            slope: b"slope".to_vec(),
            pivot: b"pivot".to_vec(),
            pivot_black: b"pivotBlack".to_vec(),
            pivot_white: b"pivotWhite".to_vec(),
            clamp_black: b"clampBlack".to_vec(),
            clamp_white: b"clampWhite".to_vec(),
            saturation: b"saturation".to_vec(),
            local_bypass: b"localBypass".to_vec(),
        }
    }
}

/// Adds the float uniform `name` and declares it, unless the description has it already.
///
/// Port of `AddUniform` for a double (GradingPrimaryOpGPU.cpp:35-47 @ v2.5.2).
fn add_uniform_double(
    shader_creator: &mut GpuShaderDesc,
    getter: Getter<f64>,
    name: &[u8],
) -> Result<()> {
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
/// Port of `AddBoolUniform` (GradingPrimaryOpGPU.cpp:49-61 @ v2.5.2).
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

/// Adds the float3 uniform `name` and declares it, unless the description has it already.
///
/// Port of `AddUniform` for a Float3 (GradingPrimaryOpGPU.cpp:63-75 @ v2.5.2).
fn add_uniform_float3(
    shader_creator: &mut GpuShaderDesc,
    getter: Getter<[f32; 3]>,
    name: &[u8],
) -> Result<()> {
    // Add the uniform if it does not already exist.
    if shader_creator.add_uniform_float3(name, getter)? {
        // Declare uniform.
        let st_decl = GpuShaderText::new(shader_creator.language());
        st_decl.declare_uniform_float3(name);
        shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    }
    Ok(())
}

/// `opPrefix` (GradingPrimaryOpGPU.cpp:77 @ v2.5.2).
const OP_PREFIX: &str = "grading_primary";

/// `BuildResourceName(shaderCreator, opPrefix, name)`.
fn resource_name(shader_creator: &GpuShaderDesc, name: &[u8]) -> Vec<u8> {
    build_resource_name(shader_creator.resource_prefix(), OP_PREFIX, name)
}

/// Which values a style's shader uses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Values {
    Log,
    Lin,
    Video,
}

/// The style's values: for a dynamic op, uniforms (with their decorated names) bound to the
/// shader description's copy of the property, added if the description doesn't have them;
/// else local constants of the values.
///
/// Port of `AddGPLogProperties`, `AddGPLinProperties` and `AddGPVideoProperties`
/// (GradingPrimaryOpGPU.cpp:79-154, 230-296, 363-436 @ v2.5.2), which differ only in their
/// values.
fn add_gp_properties(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    gp_data: &GradingPrimaryOpData,
    prop_names: &mut GpProperties,
    dyn_: bool,
    values: Values,
) -> Result<()> {
    let prop = gp_data.get_dynamic_property_internal();
    if dyn_ {
        // Build names. No need to add an index to the name to avoid collisions as the dynamic
        // properties are unique.
        match values {
            Values::Log => {
                prop_names.brightness = resource_name(shader_creator, &prop_names.brightness);
                prop_names.contrast = resource_name(shader_creator, &prop_names.contrast);
                prop_names.gamma = resource_name(shader_creator, &prop_names.gamma);

                prop_names.pivot = resource_name(shader_creator, &prop_names.pivot);
                prop_names.pivot_black = resource_name(shader_creator, &prop_names.pivot_black);
                prop_names.pivot_white = resource_name(shader_creator, &prop_names.pivot_white);
            }
            Values::Lin => {
                prop_names.offset = resource_name(shader_creator, &prop_names.offset);
                prop_names.exposure = resource_name(shader_creator, &prop_names.exposure);
                prop_names.contrast = resource_name(shader_creator, &prop_names.contrast);

                prop_names.pivot = resource_name(shader_creator, &prop_names.pivot);
            }
            Values::Video => {
                prop_names.gamma = resource_name(shader_creator, &prop_names.gamma);
                prop_names.offset = resource_name(shader_creator, &prop_names.offset);
                prop_names.slope = resource_name(shader_creator, &prop_names.slope);

                prop_names.pivot_black = resource_name(shader_creator, &prop_names.pivot_black);
                prop_names.pivot_white = resource_name(shader_creator, &prop_names.pivot_white);
            }
        }
        prop_names.clamp_black = resource_name(shader_creator, &prop_names.clamp_black);
        prop_names.clamp_white = resource_name(shader_creator, &prop_names.clamp_white);
        prop_names.saturation = resource_name(shader_creator, &prop_names.saturation);

        prop_names.local_bypass = resource_name(shader_creator, &prop_names.local_bypass);

        // Property is decoupled and added to shader creator.
        let shader_prop = prop.create_editable_copy();
        let new_prop = DynamicPropertyRcPtr::GradingPrimary(Arc::clone(&shader_prop));
        shader_creator.add_dynamic_property(new_prop)?;

        // Use the shader dynamic property to bind the uniforms (each getter reads the
        // property's values when the application asks for them).
        let float3 = |get: fn(
            &ocio_ops::dynamic_property::DynamicPropertyGradingPrimaryImpl,
        ) -> [f32; 3]|
         -> Getter<[f32; 3]> {
            let p = Arc::clone(&shader_prop);
            Arc::new(move || get(&p))
        };
        let double = |get: fn(
            &ocio_ops::dynamic_property::DynamicPropertyGradingPrimaryImpl,
        ) -> f64|
         -> Getter<f64> {
            let p = Arc::clone(&shader_prop);
            Arc::new(move || get(&p))
        };

        // Add uniforms if they are not already there.
        match values {
            Values::Log => {
                add_uniform_float3(
                    shader_creator,
                    float3(|p| p.get_brightness()),
                    &prop_names.brightness,
                )?;
                add_uniform_float3(
                    shader_creator,
                    float3(|p| p.get_contrast()),
                    &prop_names.contrast,
                )?;
                add_uniform_float3(shader_creator, float3(|p| p.get_gamma()), &prop_names.gamma)?;

                add_uniform_double(shader_creator, double(|p| p.get_pivot()), &prop_names.pivot)?;
                add_uniform_double(
                    shader_creator,
                    double(|p| p.state().value.pivot_black),
                    &prop_names.pivot_black,
                )?;
                add_uniform_double(
                    shader_creator,
                    double(|p| p.state().value.pivot_white),
                    &prop_names.pivot_white,
                )?;
            }
            Values::Lin => {
                add_uniform_float3(
                    shader_creator,
                    float3(|p| p.get_offset()),
                    &prop_names.offset,
                )?;
                add_uniform_float3(
                    shader_creator,
                    float3(|p| p.get_exposure()),
                    &prop_names.exposure,
                )?;
                add_uniform_float3(
                    shader_creator,
                    float3(|p| p.get_contrast()),
                    &prop_names.contrast,
                )?;

                add_uniform_double(shader_creator, double(|p| p.get_pivot()), &prop_names.pivot)?;
            }
            Values::Video => {
                add_uniform_float3(shader_creator, float3(|p| p.get_gamma()), &prop_names.gamma)?;
                add_uniform_float3(
                    shader_creator,
                    float3(|p| p.get_offset()),
                    &prop_names.offset,
                )?;
                add_uniform_float3(shader_creator, float3(|p| p.get_slope()), &prop_names.slope)?;

                add_uniform_double(
                    shader_creator,
                    double(|p| p.state().value.pivot_black),
                    &prop_names.pivot_black,
                )?;
                add_uniform_double(
                    shader_creator,
                    double(|p| p.state().value.pivot_white),
                    &prop_names.pivot_white,
                )?;
            }
        }
        add_uniform_double(
            shader_creator,
            double(|p| p.state().value.clamp_black),
            &prop_names.clamp_black,
        )?;
        add_uniform_double(
            shader_creator,
            double(|p| p.state().value.clamp_white),
            &prop_names.clamp_white,
        )?;
        add_uniform_double(
            shader_creator,
            double(|p| p.state().value.saturation),
            &prop_names.saturation,
        )?;
        let p = Arc::clone(&shader_prop);
        let get_lbp: Getter<bool> = Arc::new(move || p.get_local_bypass());
        add_bool_uniform(shader_creator, get_lbp, &prop_names.local_bypass)?;
    } else {
        let state = *prop.state();
        let value = &state.value;
        let comp = &state.pre_render;

        match values {
            Values::Log => {
                st.declare_float3_vec(&prop_names.brightness, comp.get_brightness())?;
                st.declare_float3_vec(&prop_names.contrast, comp.get_contrast())?;
                st.declare_float3_vec(&prop_names.gamma, comp.get_gamma())?;

                st.declare_var_const_f32(&prop_names.pivot, comp.get_pivot() as f32)?;
                st.declare_var_const_f32(&prop_names.pivot_black, value.pivot_black as f32)?;
                st.declare_var_const_f32(&prop_names.pivot_white, value.pivot_white as f32)?;
            }
            Values::Lin => {
                st.declare_float3_vec(&prop_names.offset, comp.get_offset())?;
                st.declare_float3_vec(&prop_names.exposure, comp.get_exposure())?;
                st.declare_float3_vec(&prop_names.contrast, comp.get_contrast())?;

                st.declare_var_const_f32(&prop_names.pivot, comp.get_pivot() as f32)?;
            }
            Values::Video => {
                st.declare_float3_vec(&prop_names.gamma, comp.get_gamma())?;
                st.declare_float3_vec(&prop_names.offset, comp.get_offset())?;
                st.declare_float3_vec(&prop_names.slope, comp.get_slope())?;

                st.declare_var_const_f32(&prop_names.pivot_black, value.pivot_black as f32)?;
                st.declare_var_const_f32(&prop_names.pivot_white, value.pivot_white as f32)?;
            }
        }
        st.declare_var_const_f32(&prop_names.clamp_black, value.clamp_black as f32)?;
        st.declare_var_const_f32(&prop_names.clamp_white, value.clamp_white as f32)?;
        st.declare_var_const_f32(&prop_names.saturation, value.saturation as f32)?;
    }
    Ok(())
}

/// The gamma between the pivots, in a block run when the gamma isn't 1. `indent` is the
/// video forward shader's extra two spaces before its last line.
///
/// Port of the gamma blocks of `AddGPLogForwardShader`, `AddGPLogInverseShader`,
/// `AddGPVideoForwardShader` and `AddGPVideoInverseShader` (GradingPrimaryOpGPU.cpp:167-181,
/// 208-222, 448-460, 488-500 @ v2.5.2).
fn add_gamma_block(
    pxl: &[u8],
    st: &GpuShaderText,
    props: &GpProperties,
    indent: &str,
) -> Result<()> {
    // Not sure if the if helps performance, but it does allow out == in at the default values.
    st.new_line()
        .put("if ( ")
        .put(st.vector_compare_expression(&props.gamma, "!=", st.float3_splat_f32(1.0)))
        .put(" )");
    st.new_line().put("{");
    st.indent();
    st.new_line()
        .put(st.float3_decl("normalizedOut")?)
        .put(" = abs(")
        .put(pxl)
        .put(".rgb - ")
        .put(props.pivot_black.as_slice())
        .put(") / ")
        .put("(")
        .put(props.pivot_white.as_slice())
        .put(" - ")
        .put(props.pivot_black.as_slice())
        .put(");");
    // NB: The sign(outColor.rgb) is a vec3, preserving the sign of each channel.
    st.new_line()
        .put(st.float3_decl("scale")?)
        .put(" = sign(")
        .put(pxl)
        .put(".rgb - ")
        .put(props.pivot_black.as_slice())
        .put(") * ")
        .put("(")
        .put(props.pivot_white.as_slice())
        .put(" - ")
        .put(props.pivot_black.as_slice())
        .put(");");
    st.new_line()
        .put(indent)
        .put(pxl)
        .put(".rgb = pow( normalizedOut, ")
        .put(props.gamma.as_slice())
        .put(" ) * scale + ")
        .put(props.pivot_black.as_slice())
        .put(";");
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// `ApplyContrast` in the shading language: `pxl.rgb = ( pxl.rgb - pivot ) * contrast +
/// pivot;`.
fn add_contrast_line(pxl: &[u8], st: &GpuShaderText, pivot: &[u8], contrast: &[u8]) {
    st.new_line()
        .put(pxl)
        .put(".rgb = ( ")
        .put(pxl)
        .put(".rgb - ")
        .put(pivot)
        .put(" ) * ")
        .put(contrast)
        .put(" + ")
        .put(pivot)
        .put(";");
}

/// The forward styles' saturation and clamp.
///
/// Port of the last lines of `AddGPLogForwardShader`, `AddGPLinForwardShader` and
/// `AddGPVideoForwardShader` (GradingPrimaryOpGPU.cpp:183-188, 320-325, 462-467 @ v2.5.2).
fn add_forward_saturation_and_clamp(
    pxl: &[u8],
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    st.declare_float3_f32("lumaWgts", 0.2126, 0.7152, 0.0722)?;
    st.new_line()
        .put(st.float_decl("luma")?)
        .put(" = dot( ")
        .put(pxl)
        .put(".rgb, lumaWgts );");
    st.new_line()
        .put(pxl)
        .put(".rgb = luma + ")
        .put(props.saturation.as_slice())
        .put(" * (")
        .put(pxl)
        .put(".rgb - luma);");

    add_clamp_line(pxl, st, props);
    Ok(())
}

/// `pxl.rgb = clamp( pxl.rgb, clampBlack, clampWhite );`.
fn add_clamp_line(pxl: &[u8], st: &GpuShaderText, props: &GpProperties) {
    st.new_line()
        .put(pxl)
        .put(".rgb = clamp( ")
        .put(pxl)
        .put(".rgb, ")
        .put(props.clamp_black.as_slice())
        .put(", ")
        .put(props.clamp_white.as_slice())
        .put(" );");
}

/// The inverse styles' clamp and saturation, which they skip for a saturation of 0 or 1.
///
/// Port of the first lines of `AddGPLogInverseShader`, `AddGPLinInverseShader` and
/// `AddGPVideoInverseShader` (GradingPrimaryOpGPU.cpp:197-206, 334-344, 476-486 @ v2.5.2).
fn add_inverse_clamp_and_saturation(
    pxl: &[u8],
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    add_clamp_line(pxl, st, props);
    st.new_line()
        .put("if (")
        .put(props.saturation.as_slice())
        .put(" != 0. && ")
        .put(props.saturation.as_slice())
        .put(" != 1.)");
    st.new_line().put("{");
    st.indent();
    st.declare_float3_f32("lumaWgts", 0.2126, 0.7152, 0.0722)?;
    st.new_line()
        .put(st.float_decl("luma")?)
        .put(" = dot( ")
        .put(pxl)
        .put(".rgb, lumaWgts );");
    st.new_line()
        .put(pxl)
        .put(".rgb = luma + (")
        .put(pxl)
        .put(".rgb - luma) / ")
        .put(props.saturation.as_slice())
        .put(";");
    st.dedent();
    st.new_line().put("}");
    Ok(())
}

/// `pxl.rgb += v;`.
fn add_plus_line(pxl: &[u8], st: &GpuShaderText, v: &[u8]) {
    st.new_line().put(pxl).put(".rgb += ").put(v).put(";");
}

/// Port of `AddGPLogForwardShader` (GradingPrimaryOpGPU.cpp:156-189 @ v2.5.2).
fn add_gp_log_forward_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    let pxl = shader_creator.pixel_name();

    add_plus_line(pxl, st, &props.brightness);

    add_contrast_line(pxl, st, &props.pivot, &props.contrast);

    add_gamma_block(pxl, st, props, "")?;

    add_forward_saturation_and_clamp(pxl, st, props)
}

/// Port of `AddGPLogInverseShader` (GradingPrimaryOpGPU.cpp:191-228 @ v2.5.2).
fn add_gp_log_inverse_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    let pxl = shader_creator.pixel_name();

    add_inverse_clamp_and_saturation(pxl, st, props)?;

    add_gamma_block(pxl, st, props, "")?;

    add_contrast_line(pxl, st, &props.pivot, &props.contrast);

    add_plus_line(pxl, st, &props.brightness);
    Ok(())
}

/// The linear style's contrast around the pivot, with its sign, in a block run when the
/// contrast isn't 1.
///
/// Port of the contrast blocks of `AddGPLinForwardShader` and `AddGPLinInverseShader`
/// (GradingPrimaryOpGPU.cpp:308-318, 346-356 @ v2.5.2).
fn add_lin_contrast_block(pxl: &[u8], st: &GpuShaderText, props: &GpProperties) {
    // Not sure if the if helps performance, but it does allow out == in at the default values.
    // Although note that the log-to-lin in Tone Op also prevents out == in.
    st.new_line()
        .put("if ( ")
        .put(st.vector_compare_expression(&props.contrast, "!=", st.float3_splat_f32(1.0)))
        .put(" )");
    st.new_line().put("{");
    st.indent();

    // NB: The sign(outColor.rgb) is a vec3, preserving the sign of each channel.
    st.new_line()
        .put(pxl)
        .put(".rgb = pow( abs(")
        .put(pxl)
        .put(".rgb / ")
        .put(props.pivot.as_slice())
        .put("), ")
        .put(props.contrast.as_slice())
        .put(" ) * ")
        .put("sign(")
        .put(pxl)
        .put(".rgb) * ")
        .put(props.pivot.as_slice())
        .put(";");
    st.dedent();
    st.new_line().put("}");
}

/// Port of `AddGPLinForwardShader` (GradingPrimaryOpGPU.cpp:298-326 @ v2.5.2).
fn add_gp_lin_forward_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    let pxl = shader_creator.pixel_name();

    add_plus_line(pxl, st, &props.offset);

    st.new_line()
        .put(pxl)
        .put(".rgb *= ")
        .put(props.exposure.as_slice())
        .put(";");

    add_lin_contrast_block(pxl, st, props);

    add_forward_saturation_and_clamp(pxl, st, props)
}

/// Port of `AddGPLinInverseShader` (GradingPrimaryOpGPU.cpp:328-361 @ v2.5.2).
fn add_gp_lin_inverse_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    let pxl = shader_creator.pixel_name();

    add_inverse_clamp_and_saturation(pxl, st, props)?;

    add_lin_contrast_block(pxl, st, props);

    st.new_line()
        .put(pxl)
        .put(".rgb *= ")
        .put(props.exposure.as_slice())
        .put(";");

    add_plus_line(pxl, st, &props.offset);
    Ok(())
}

/// Port of `AddGPVideoForwardShader` (GradingPrimaryOpGPU.cpp:438-468 @ v2.5.2).
fn add_gp_video_forward_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    let pxl = shader_creator.pixel_name();

    add_plus_line(pxl, st, &props.offset);
    add_contrast_line(pxl, st, &props.pivot_black, &props.slope);

    add_gamma_block(pxl, st, props, "  ")?;

    add_forward_saturation_and_clamp(pxl, st, props)
}

/// Port of `AddGPVideoInverseShader` (GradingPrimaryOpGPU.cpp:470-505 @ v2.5.2).
fn add_gp_video_inverse_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    props: &GpProperties,
) -> Result<()> {
    let pxl = shader_creator.pixel_name();

    add_inverse_clamp_and_saturation(pxl, st, props)?;

    add_gamma_block(pxl, st, props, "")?;

    add_contrast_line(pxl, st, &props.pivot_black, &props.slope);
    add_plus_line(pxl, st, &props.offset);
    Ok(())
}

/// Adds the op's shader code to `shader_creator`: nothing for an op whose values change
/// nothing (unless it is dynamic), else a block that declares the values (or binds them to
/// uniforms) and applies the style's shader, inside a test of the bypass uniform for a dynamic
/// op.
///
/// Port of `GetGradingPrimaryGPUShaderProgram` (GradingPrimaryOpGPU.cpp:509-638 @ v2.5.2).
pub fn get_grading_primary_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    gp_data: &GradingPrimaryOpData,
) -> Result<()> {
    let dyn_ = gp_data.is_dynamic() && shader_creator.language() != GpuLanguage::Osl1;
    if !dyn_ {
        let prop_gp = gp_data.get_dynamic_property_internal();
        if prop_gp.get_local_bypass() {
            return Ok(());
        }
    }

    if gp_data.is_dynamic() && shader_creator.language() == GpuLanguage::Osl1 {
        log_warning(format!(
            "The dynamic properties are not yet supported by the 'Open Shading language (OSL)' \
             translation: The '{OP_PREFIX}' dynamic property is replaced by a local variable."
        ));
    }

    let style = gp_data.get_style();
    let dir = gp_data.get_direction();

    let st = GpuShaderText::new(shader_creator.language());
    st.indent();

    st.new_line().put("");
    st.new_line()
        .put("// Add GradingPrimary '")
        .put(grading_style_to_string(style))
        .put("' ")
        .put(transform_direction_to_string(dir))
        .put(" processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    // Properties holds shader variables names and is initialized with undecorated names
    // suitable for local variables.
    let mut properties = GpProperties::default();
    let values = match style {
        GradingStyle::Log => Values::Log,
        GradingStyle::Lin => Values::Lin,
        GradingStyle::Video => Values::Video,
    };
    add_gp_properties(shader_creator, &st, gp_data, &mut properties, dyn_, values)?;
    if dyn_ {
        st.new_line()
            .put("if (!")
            .put(st.cast_to_bool(&properties.local_bypass))
            .put(")");
        st.new_line().put("{");
        st.indent();
    }

    match (style, dir) {
        (GradingStyle::Log, TransformDirection::Forward) => {
            add_gp_log_forward_shader(shader_creator, &st, &properties)?
        }
        (GradingStyle::Log, TransformDirection::Inverse) => {
            add_gp_log_inverse_shader(shader_creator, &st, &properties)?
        }
        (GradingStyle::Lin, TransformDirection::Forward) => {
            add_gp_lin_forward_shader(shader_creator, &st, &properties)?
        }
        (GradingStyle::Lin, TransformDirection::Inverse) => {
            add_gp_lin_inverse_shader(shader_creator, &st, &properties)?
        }
        (GradingStyle::Video, TransformDirection::Forward) => {
            add_gp_video_forward_shader(shader_creator, &st, &properties)?
        }
        (GradingStyle::Video, TransformDirection::Inverse) => {
            add_gp_video_inverse_shader(shader_creator, &st, &properties)?
        }
    }

    if dyn_ {
        st.dedent();
        st.new_line().put("}");
    }
    st.dedent();
    st.new_line().put("}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}
