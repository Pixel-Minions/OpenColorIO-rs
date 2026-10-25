// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpGPU.h` and
//! `ExposureContrastOpGPU.cpp` @ v2.5.2, the ExposureContrast op's GPU writer.
//!
//! Exposure, contrast and gamma go into the shader as local constants, or, for a dynamic
//! property (in every language but OSL), as a float uniform bound to a copy of the property
//! that the shader description holds, so the application can change the value after the
//! extraction. OSL, which has no dynamic properties, gets the constant and a warning. The
//! shader of each style then computes in the shading language what the CPU renderers compute
//! (with `pow` where they use `ssePower`).

use std::sync::Arc;

use ocio_ops::Result;
use ocio_ops::dynamic_property::{DynamicPropertyDoubleImplRcPtr, DynamicPropertyRcPtr};
use ocio_ops::math_utils::{std_max, std_pow};
use ocio_ops::ops::exposurecontrast::exposure_contrast_op_data::{
    ExposureContrastOpData, ExposureContrastOpStyle, ec,
};

use crate::gpu_shader::Getter;
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::{GpuShaderText, build_resource_name};
use crate::open_color_types::GpuLanguage;

/// The uniforms' and local variables' base names (ExposureContrastOpGPU.cpp:19-21 @ v2.5.2).
const EC_EXPOSURE: &str = "exposureVal";
const EC_CONTRAST: &str = "contrastVal";
const EC_GAMMA: &str = "gammaVal";

/// Adds the float uniform `name`, read from `prop` when the application asks for its value,
/// and declares it.
///
/// Port of `AddUniform` (ExposureContrastOpGPU.cpp:23-34 @ v2.5.2). Upstream ignores whether
/// the description already had the uniform.
fn add_uniform(
    shader_creator: &mut GpuShaderDesc,
    prop: DynamicPropertyDoubleImplRcPtr,
    name: &[u8],
) -> Result<()> {
    let get_double: Getter<f64> = Arc::new(move || prop.get_value());
    shader_creator.add_uniform_double(name, get_double)?;
    // Declare uniform.
    let st_decl = GpuShaderText::new(shader_creator.language());
    st_decl.declare_uniform_float(name);
    shader_creator.add_to_parameter_declare_shader_code(st_decl.string());
    Ok(())
}

/// The name the shader code uses for the property: a uniform of a dynamic property (but in
/// OSL), else a local variable declared with the value, and in OSL a warning for a dynamic
/// property.
///
/// Port of `AddProperty` (ExposureContrastOpGPU.cpp:36-79 @ v2.5.2).
fn add_property(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    prop: DynamicPropertyDoubleImplRcPtr,
    name: &str,
) -> Result<Vec<u8>> {
    let final_name;

    if prop.is_dynamic() && shader_creator.language() != GpuLanguage::Osl1 {
        // Build the name for the uniform. The same type of property should give the same name,
        // so that uniform is declared only once, but multiple instances of the shader code can
        // reference that name.
        // Note: No need to add an index to the name to avoid collisions as the dynamic
        // properties are unique.
        final_name =
            build_resource_name(shader_creator.resource_prefix(), "exposure_contrast", name);

        // Property is decoupled and added to shader creator.
        let shader_prop = prop.create_editable_copy();
        let new_prop = DynamicPropertyRcPtr::Double(Arc::clone(&shader_prop));
        shader_creator.add_dynamic_property(new_prop)?;

        // Uniform is added, connected to the shader creator instance of the dynamic property.
        add_uniform(shader_creator, shader_prop, &final_name)?;
    } else {
        // Declare a local variable to be used by the shader code.
        final_name = name.as_bytes().to_vec();
        st.declare_var_f32(&final_name, prop.get_value() as f32)?;

        if shader_creator.language() == GpuLanguage::Osl1 && prop.is_dynamic() {
            let msg = format!(
                "The dynamic properties are not yet supported by the 'Open Shading language \
                 (OSL)' translation: The '{name}' dynamic property is replaced by a local \
                 variable."
            );

            shader_creator.log_warning(msg);
        }
    }

    Ok(final_name)
}

/// The names of exposure, contrast and gamma in the shader code.
struct EcNames {
    exposure: Vec<u8>,
    contrast: Vec<u8>,
    gamma: Vec<u8>,
}

/// Port of `AddProperties` (ExposureContrastOpGPU.cpp:81-91 @ v2.5.2).
fn add_properties(
    shader_creator: &mut GpuShaderDesc,
    st: &GpuShaderText,
    ec: &ExposureContrastOpData,
) -> Result<EcNames> {
    let exposure = add_property(shader_creator, st, ec.get_exposure_property(), EC_EXPOSURE)?;
    let contrast = add_property(shader_creator, st, ec.get_contrast_property(), EC_CONTRAST)?;
    let gamma = add_property(shader_creator, st, ec.get_gamma_property(), EC_GAMMA)?;
    Ok(EcNames {
        exposure,
        contrast,
        gamma,
    })
}

/// `pixel.rgb = pow( max( float3(0.), pixel.rgb / float3(pivot) ), float3(contrast) ) *
/// float3(pivot);` in a block run when the contrast isn't 1, the linear and video styles'
/// shared code.
fn add_contrast_block(shader_creator: &GpuShaderDesc, st: &GpuShaderText, pivot: f64) {
    let pixel = shader_creator.pixel_name();
    st.new_line().put("if (contrast != 1.0)");
    st.new_line().put("{");
    {
        st.indent();
        // outColor = pow(max(0, outColor/pivot), contrast) * pivot;
        st.new_line()
            .put(pixel)
            .put(".rgb = ")
            .put("pow( ")
            .put("max( ")
            .put(st.float3_splat_f32(0.0))
            .put(", ")
            .put(pixel)
            .put(".rgb / ")
            .put(st.float3_splat_f64(pivot))
            .put(" ), ")
            .put(st.float3_splat("contrast"))
            .put(" ) * ")
            .put(st.float3_splat_f64(pivot))
            .put(";");
        st.dedent();
    }
    st.new_line().put("}");
}

/// `contrast = max( MIN_CONTRAST, ( contrast * gamma ) );`, after `prefix` (`1. / ` for the
/// reverse styles).
fn add_contrast_decl(st: &GpuShaderText, names: &EcNames, prefix: &str) -> Result<()> {
    st.new_line()
        .put(st.float_decl("contrast")?)
        .put(" = ")
        .put(prefix)
        .put("max( ")
        .put(ec::MIN_CONTRAST)
        .put(", ")
        .put("( ")
        .put(names.contrast.as_slice())
        .put(" * ")
        .put(names.gamma.as_slice())
        .put(" ) );");
    Ok(())
}

/// Port of `AddECLinearShader` (ExposureContrastOpGPU.cpp:93-125 @ v2.5.2).
fn add_ec_linear_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    ec: &ExposureContrastOpData,
    names: &EcNames,
) -> Result<()> {
    let pivot = std_max(ec::MIN_PIVOT, ec.get_pivot());

    st.new_line()
        .put(st.float_decl("exposure")?)
        .put(" = pow( 2., ")
        .put(names.exposure.as_slice())
        .put(" );");
    add_contrast_decl(st, names, "")?;
    let pixel = shader_creator.pixel_name();
    st.new_line()
        .put(pixel)
        .put(".rgb = ")
        .put(pixel)
        .put(".rgb * exposure;");

    add_contrast_block(shader_creator, st, pivot);
    Ok(())
}

/// Port of `AddECLinearRevShader` (ExposureContrastOpGPU.cpp:127-160 @ v2.5.2).
fn add_ec_linear_rev_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    ec: &ExposureContrastOpData,
    names: &EcNames,
) -> Result<()> {
    let pivot = std_max(ec::MIN_PIVOT, ec.get_pivot());

    st.new_line()
        .put(st.float_decl("exposure")?)
        .put(" = pow( 2., ")
        .put(names.exposure.as_slice())
        .put(" );");
    add_contrast_decl(st, names, "1. / ")?;

    add_contrast_block(shader_creator, st, pivot);

    let pixel = shader_creator.pixel_name();
    st.new_line()
        .put(pixel)
        .put(".rgb = ")
        .put(pixel)
        .put(".rgb / exposure;");
    Ok(())
}

/// `exposure = pow( pow( 2., exposure ), VIDEO_OETF_POWER);`, the video styles' exposure.
fn add_video_exposure_decl(st: &GpuShaderText, names: &EcNames) -> Result<()> {
    st.new_line()
        .put(st.float_decl("exposure")?)
        .put(" = pow( pow( 2., ")
        .put(names.exposure.as_slice())
        .put(" ), ")
        .put(ec::VIDEO_OETF_POWER)
        .put(");");
    Ok(())
}

/// The video styles' pivot: `std::pow(std::max(EC::MIN_PIVOT, pivot), EC::VIDEO_OETF_POWER)`,
/// in double.
fn video_pivot(ec: &ExposureContrastOpData) -> f64 {
    std_pow(std_max(ec::MIN_PIVOT, ec.get_pivot()), ec::VIDEO_OETF_POWER)
}

/// Port of `AddECVideoShader` (ExposureContrastOpGPU.cpp:162-194 @ v2.5.2).
fn add_ec_video_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    ec: &ExposureContrastOpData,
    names: &EcNames,
) -> Result<()> {
    let pivot = video_pivot(ec);

    add_video_exposure_decl(st, names)?;
    add_contrast_decl(st, names, "")?;
    let pixel = shader_creator.pixel_name();
    st.new_line()
        .put(pixel)
        .put(".rgb = ")
        .put(pixel)
        .put(".rgb * exposure;");
    add_contrast_block(shader_creator, st, pivot);
    Ok(())
}

/// Port of `AddECVideoRevShader` (ExposureContrastOpGPU.cpp:196-230 @ v2.5.2).
fn add_ec_video_rev_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    ec: &ExposureContrastOpData,
    names: &EcNames,
) -> Result<()> {
    let pivot = video_pivot(ec);

    add_video_exposure_decl(st, names)?;
    add_contrast_decl(st, names, "1. / ")?;

    add_contrast_block(shader_creator, st, pivot);

    let pixel = shader_creator.pixel_name();
    st.new_line()
        .put(pixel)
        .put(".rgb = ")
        .put(pixel)
        .put(".rgb / exposure;");
    Ok(())
}

/// The log styles' pivot: `(float)std::max(0., std::log2(pivot / 0.18) * logExposureStep +
/// logMidGray)`, with `pivot` at least `EC::MIN_PIVOT`, in double.
fn log_pivot(ec: &ExposureContrastOpData) -> f32 {
    let pivot = std_max(ec::MIN_PIVOT, ec.get_pivot());
    std_max(
        0.,
        (pivot / 0.18).log2() * ec.get_log_exposure_step() + ec.get_log_mid_gray(),
    ) as f32
}

/// `exposure = exposure * logExposureStep;` and the contrast, the log styles' start.
fn add_log_decls(st: &GpuShaderText, ec: &ExposureContrastOpData, names: &EcNames) -> Result<()> {
    st.new_line()
        .put(st.float_decl("exposure")?)
        .put(" = ")
        .put(names.exposure.as_slice())
        .put(" * ")
        .put(ec.get_log_exposure_step())
        .put(";");
    add_contrast_decl(st, names, "")
}

/// Port of `AddECLogarithmicShader` (ExposureContrastOpGPU.cpp:232-253 @ v2.5.2).
fn add_ec_logarithmic_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    ec: &ExposureContrastOpData,
    names: &EcNames,
) -> Result<()> {
    let log_pivot = log_pivot(ec);

    add_log_decls(st, ec, names)?;
    st.new_line()
        .put(st.float_decl("offset")?)
        .put(" = ( exposure - ")
        .put(log_pivot)
        .put(" ) * contrast + ")
        .put(log_pivot)
        .put(";");

    let pixel = shader_creator.pixel_name();
    st.new_line()
        .put(pixel)
        .put(".rgb = ")
        .put(pixel)
        .put(".rgb * contrast + offset;");
    Ok(())
}

/// Port of `AddECLogarithmicRevShader` (ExposureContrastOpGPU.cpp:255-276 @ v2.5.2). Unlike
/// the CPU renderer (I-217), it uses the op's log exposure step.
fn add_ec_logarithmic_rev_shader(
    shader_creator: &GpuShaderDesc,
    st: &GpuShaderText,
    ec: &ExposureContrastOpData,
    names: &EcNames,
) -> Result<()> {
    let log_pivot = log_pivot(ec);

    add_log_decls(st, ec, names)?;
    st.new_line()
        .put(st.float_decl("offset")?)
        .put(" = ")
        .put(log_pivot)
        .put(" - ")
        .put(log_pivot)
        .put(" / contrast - exposure;");

    let pixel = shader_creator.pixel_name();
    st.new_line()
        .put(pixel)
        .put(".rgb = ")
        .put(pixel)
        .put(".rgb / contrast + offset;");
    Ok(())
}

/// Adds the op's block, its dynamic properties and their uniforms to `shader_creator`.
///
/// Port of `GetExposureContrastGPUShaderProgram` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpGPU.cpp:281-331 @ v2.5.2).
pub fn get_exposure_contrast_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    ec: &ExposureContrastOpData,
) -> Result<()> {
    let st = GpuShaderText::new(shader_creator.language());
    st.indent();

    st.new_line().put("");
    st.new_line()
        .put("// Add ExposureContrast '")
        .put(ExposureContrastOpData::convert_style_to_string(
            ec.get_style(),
        ))
        .put("' processing");
    st.new_line().put("");
    st.new_line().put("{");
    st.indent();

    let names = add_properties(shader_creator, &st, ec)?;

    match ec.get_style() {
        ExposureContrastOpStyle::Linear => add_ec_linear_shader(shader_creator, &st, ec, &names)?,
        ExposureContrastOpStyle::LinearRev => {
            add_ec_linear_rev_shader(shader_creator, &st, ec, &names)?
        }
        ExposureContrastOpStyle::Video => add_ec_video_shader(shader_creator, &st, ec, &names)?,
        ExposureContrastOpStyle::VideoRev => {
            add_ec_video_rev_shader(shader_creator, &st, ec, &names)?
        }
        ExposureContrastOpStyle::Logarithmic => {
            add_ec_logarithmic_shader(shader_creator, &st, ec, &names)?
        }
        ExposureContrastOpStyle::LogarithmicRev => {
            add_ec_logarithmic_rev_shader(shader_creator, &st, ec, &names)?
        }
    }

    st.dedent();
    st.new_line().put("}");

    st.dedent();
    shader_creator.add_to_function_shader_code(st.string());
    Ok(())
}
