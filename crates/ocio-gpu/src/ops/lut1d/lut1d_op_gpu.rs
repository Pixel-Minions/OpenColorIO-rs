// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/lut1d/Lut1DOpGPU.h` and `Lut1DOpGPU.cpp` @ v2.5.2: the 1D LUT
//! op's GPU writer. The LUT goes in a texture (1D, or 2D when it is longer than the texture
//! width limit, has a half domain, or the language or the description has no 1D textures),
//! which the shader samples per channel, with the hue adjustment around it.

use std::os::raw::c_ulong;

use ocio_ops::math_utils::sanitize_float;
use ocio_ops::open_color_types::{Lut1DHueAdjust, TransformDirection};
use ocio_ops::ops::lut1d::lut1d_op_data::{Lut1DOpData, make_fast_lut1d_from_inverse};
use ocio_ops::{Exception, Result};

use crate::gpu_shader::{TextureDimensions, TextureType};
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::{GpuShaderText, build_resource_name};
use crate::open_color_types::GpuLanguage;

/// `HALF_NRM_MIN` as a `float`: the smallest normal half.
const HALF_NRM_MIN: f32 = 6.103_515_625e-5;

/// `HALF_MAX` as a `float`: the largest finite half.
const HALF_MAX: f32 = 65504.0;

/// The texels of a 1D LUT laid out in a texture `width` texels wide and `height` high, with
/// the 3 channels of each entry, NaNs and infinities sanitized. In more than one row, the last
/// texel of a row repeats the first of the next, so that a lookup based on `width - 1` keeps
/// the interpolation continuous across rows; the texture is padded with the last entry.
///
/// The caller checks that the layout fits ([`padding_fits`]); upstream divides by zero, loops
/// forever or exhausts memory where it doesn't (docs/improvements.md, U-5).
///
/// Port of `CreatePaddedLutChannels` (src/OpenColorIO/ops/lut1d/Lut1DOpGPU.cpp:19-82 @
/// v2.5.2).
fn create_padded_lut_channels(
    width: c_ulong,
    height: c_ulong,
    channel: &[f32],
    padded_channel: &mut Vec<f32>,
) {
    // The 1D LUT always contains 3 channels.
    let curr_width = (channel.len() / 3) as c_ulong;

    if height > 1 {
        // Fill the texture values.
        //
        // Make the last texel of a given row the same as the first texel
        // of its next row.  This will preserve the continuity along row breaks
        // as long as the lookup position used by the sampler is based on (width-1)
        // to account for the 1 texel padding at the end of each row.
        let mut leftover = curr_width;

        let step = width - 1;
        let mut i: c_ulong = 0;
        while i < (curr_width - step) {
            let (start, end) = (3 * i as usize, 3 * (i + step) as usize);
            padded_channel.extend(channel[start..end].iter().map(|&v| sanitize_float(v)));

            padded_channel.push(sanitize_float(channel[end]));
            padded_channel.push(sanitize_float(channel[end + 1]));
            padded_channel.push(sanitize_float(channel[end + 2]));
            leftover -= step;
            i += step;
        }

        // If there are still texels to fill, add them to the texture data.
        if leftover > 0 {
            let start = 3 * (curr_width - leftover) as usize;
            let last = 3 * (curr_width - 1) as usize;
            padded_channel.extend(channel[start..last].iter().map(|&v| sanitize_float(v)));

            padded_channel.push(sanitize_float(channel[last]));
            padded_channel.push(sanitize_float(channel[last + 1]));
            padded_channel.push(sanitize_float(channel[last + 2]));
        }
    } else {
        padded_channel.extend(channel.iter().map(|&v| sanitize_float(v)));
    }

    // Pad the remaining of the texture with the last LUT entry.
    // Note: GPU Textures are expected a size of width*height.

    let missing_entries = width * height - (padded_channel.len() / 3) as c_ulong;
    let last = 3 * (curr_width - 1) as usize;
    for _ in 0..missing_entries {
        padded_channel.push(sanitize_float(channel[last]));
        padded_channel.push(sanitize_float(channel[last + 1]));
        padded_channel.push(sanitize_float(channel[last + 2]));
    }
}

/// [`create_padded_lut_channels`] for a LUT whose channels are all the same: the red channel
/// of `channel` (which holds RGB) only.
///
/// Port of `CreatePaddedRedChannel` (src/OpenColorIO/ops/lut1d/Lut1DOpGPU.cpp:84-141 @
/// v2.5.2).
fn create_padded_red_channel(
    width: c_ulong,
    height: c_ulong,
    channel: &[f32],
    padded_channel: &mut Vec<f32>,
) {
    // The 1D LUT always contains 3 channels.
    let curr_width = (channel.len() / 3) as c_ulong;

    if height > 1 {
        // Fill the texture values.
        //
        // Make the last texel of a given row the same as the first texel
        // of its next row.  This will preserve the continuity along row breaks
        // as long as the lookup position used by the sampler is based on (width-1)
        // to account for the 1 texel padding at the end of each row.
        let mut leftover = curr_width;

        let step = width - 1;
        let mut i: c_ulong = 0;
        while i < (curr_width - step) {
            for idx in i..(i + step) {
                padded_channel.push(sanitize_float(channel[3 * idx as usize]));
            }

            padded_channel.push(sanitize_float(channel[3 * (i + step) as usize]));
            leftover -= step;
            i += step;
        }

        // If there are still texels to fill, add them to the texture data.
        if leftover > 0 {
            for idx in (curr_width - leftover)..(curr_width - 1) {
                padded_channel.push(sanitize_float(channel[3 * idx as usize]));
            }

            padded_channel.push(sanitize_float(channel[3 * (curr_width - 1) as usize]));
        }
    } else {
        for idx in 0..curr_width {
            padded_channel.push(sanitize_float(channel[3 * idx as usize]));
        }
    }

    // Pad the remaining of the texture with the last LUT entry.
    // Note: GPU Textures are expected a size of width * height.

    let missing_entries = width * height - padded_channel.len() as c_ulong;
    for _ in 0..missing_entries {
        padded_channel.push(sanitize_float(channel[3 * (curr_width - 1) as usize]));
    }
}

/// Whether a 1D LUT of `length` entries lays out in a texture at most `max_width` texels
/// wide: [`create_padded_lut_channels`]' rows, each repeating the last entry of the one
/// before, must not outnumber the texture's texels. Where they would, or where the limit is
/// 0 (a division by zero) or the texture 1 texel wide in more than one row (a loop that never
/// advances), the port refuses the LUT (docs/improvements.md, U-5).
fn padding_fits(length: u64, max_width: u64) -> bool {
    if max_width == 0 {
        return false;
    }
    let width = length.min(max_width);
    let height = length / max_width + 1;
    if height == 1 {
        return true;
    }
    let step = width - 1;
    if step == 0 {
        return false;
    }
    let rows = (length - step).div_ceil(step);
    let padded = rows * width + (length - rows * step);
    padded <= width * height
}

/// An `unsigned long` (32 bits on Windows, 64 on Linux) as a `u64`.
#[allow(clippy::useless_conversion)]
fn wide(v: c_ulong) -> u64 {
    u64::from(v)
}

/// `(unsigned)v`: an `unsigned long` passed as an `unsigned`, the low 32 bits where `unsigned
/// long` is wider (Linux).
#[allow(clippy::unnecessary_cast)]
fn unsigned(v: c_ulong) -> u32 {
    v as u32
}

/// Writes a forward 1D LUT's shader: its texture, the helper that computes a 2D texture's
/// coordinates, and the lookup of each channel, with the hue adjustment around it. Refused
/// for OSL, which upstream doesn't translate, and for a LUT that doesn't fit the texture
/// width limit (docs/improvements.md, U-5).
///
/// Port of `GetLut1DGPUShaderProgram` (src/OpenColorIO/ops/lut1d/Lut1DOpGPU.cpp:145-393 @
/// v2.5.2).
pub fn get_lut1d_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    lut_data: &Lut1DOpData,
) -> Result<()> {
    if shader_creator.language() == GpuLanguage::Osl1 {
        return Err(Exception::new(
            "The Lut1DOp is not yet supported by the 'Open Shading language (OSL)' translation",
        ));
    }

    let default_max_width = c_ulong::from(shader_creator.texture_max_width());

    let length = lut_data.get_array().get_length();
    if !padding_fits(wide(length), wide(default_max_width)) {
        return Err(Exception::new(format!(
            "The Lut1DOp of {length} entries doesn't fit in a texture at most \
             {default_max_width} texels wide."
        )));
    }
    let width = length.min(default_max_width);
    let height = (length / default_max_width) + 1;
    let num_channels = lut_data.get_array().get_num_color_components();

    // Note: The 1D LUT needs a GPU texture for the Look-up table implementation.
    // However, the texture type & content may vary based on the number of channels
    // i.e. when all channels are identical a F32 Red GPU texture is enough.

    let single_channel = num_channels == 1;

    // Adjust LUT texture to allow for correct 2d linear interpolation, if needed.

    let mut values: Vec<f32> = Vec::with_capacity((width * height * num_channels) as usize);

    if single_channel {
        // i.e. numChannels == 1.
        create_padded_red_channel(
            width,
            height,
            lut_data.get_array().get_values(),
            &mut values,
        );
    } else {
        create_padded_lut_channels(
            width,
            height,
            lut_data.get_array().get_values(),
            &mut values,
        );
    }

    // Register the RGB LUT.

    let index = shader_creator.next_resource_index();
    let name = build_resource_name(
        shader_creator.resource_prefix(),
        b"lut1d",
        index.to_string(),
    );

    let language = shader_creator.language();
    let mut dimensions = TextureDimensions::D1;
    if height > 1
        || lut_data.is_input_half_domain()
        || language == GpuLanguage::GlslEs1_0
        || language == GpuLanguage::GlslEs3_0
        || !shader_creator.allow_texture_1d()
    {
        dimensions = TextureDimensions::D2;
    }

    // Copy the LUT into the shaderCreator as a Texture object.
    let texture_shader_binding_index = shader_creator.add_texture(
        &name,
        GpuShaderText::get_sampler_name(&name),
        unsigned(width),
        unsigned(height),
        if single_channel {
            TextureType::RedChannel
        } else {
            TextureType::RgbChannel
        },
        dimensions,
        lut_data.get_concrete_interpolation(),
        &values,
    )?;

    // Add the LUT code to the OCIO shader program.

    if dimensions == TextureDimensions::D2 {
        // In case the 1D LUT length exceeds the 1D texture maximum length,
        // or the language doesn't support 1D textures,
        // a 2D texture is used.

        // Create a 2D texture declaration.
        {
            let ss = GpuShaderText::new(language);
            ss.declare_tex2d(
                &name,
                shader_creator.descriptor_set_index(),
                texture_shader_binding_index,
            )?;
            shader_creator.add_to_texture_declare_shader_code(ss.string());
        }
        // Create the helper function to deal with 2D array lookups.
        {
            let ss = GpuShaderText::new(language);

            ss.new_line()
                .put(ss.float2_keyword())
                .put(" ")
                .put(&name)
                .put("_computePos(float f)");
            ss.new_line().put("{");
            ss.indent();

            if lut_data.is_input_half_domain() {
                const NEG_MIN_EXP: f32 = 15.0;
                const EXP_SCALE: f32 = 1024.0;
                const INV_DENRM_STEP: f32 = 16777216.0; // 1 / 2^-24

                ss.new_line().put("float dep;");
                ss.new_line().put("float abs_f = abs(f);");
                ss.new_line().put("if (abs_f > ").put(HALF_NRM_MIN).put(")");
                ss.new_line().put("{");
                ss.indent();
                ss.declare_float3_f32("fComp", NEG_MIN_EXP, NEG_MIN_EXP, NEG_MIN_EXP)?;
                ss.new_line()
                    .put("float absarr = min( abs_f, ")
                    .put(HALF_MAX)
                    .put(");");
                // Compute the exponent, scaled [-14,15].
                ss.new_line().put("fComp.x = floor( log2( absarr ) );");
                // Lower is the greatest power of 2 <= f.
                ss.new_line().put("float lower = pow( 2.0, fComp.x );");
                // Compute the mantissa (scaled [0-1]).
                ss.new_line().put("fComp.y = ( absarr - lower ) / lower;");
                // The dot product recombines the parts into a raw half without the sign
                // component:
                //   dep = [ exponent + mantissa + NEG_MIN_EXP] * scale
                ss.declare_float3_f32("scale", EXP_SCALE, EXP_SCALE, EXP_SCALE)?;
                ss.new_line().put("dep = dot( fComp, scale );");
                ss.dedent();
                ss.new_line().put("}");
                ss.new_line().put("else");
                ss.new_line().put("{");
                ss.indent();
                // Extract bits from denormalized values.
                ss.new_line()
                    .put("dep = abs_f * ")
                    .put(INV_DENRM_STEP)
                    .put(";");
                ss.dedent();
                ss.new_line().put("}");

                // Adjust position for negative values.
                ss.new_line().put("dep += (f < 0.) ? 32768.0 : 0.0;");

                // At this point 'dep' contains the raw half.
                // Note: Raw halfs for NaN floats cannot be computed using
                //       floating-point operations.
            } else {
                // Need clamp() to protect against f outside [0,1] causing a bogus x value.
                // clamp( f, 0., 1.) * (dim - 1)
                ss.new_line()
                    .put("float dep = clamp(f, 0.0, 1.0) * ")
                    .put((length - 1) as f32)
                    .put(";");
            }

            ss.new_line().put(ss.float2_decl("retVal")?).put(";");

            if height > 1 {
                // floor( dep / (width-1) ))
                ss.new_line()
                    .put("retVal.y = floor(dep / ")
                    .put((width - 1) as f32)
                    .put(");");
                // dep - retVal.y * (width-1)
                ss.new_line()
                    .put("retVal.x = dep - retVal.y * ")
                    .put((width - 1) as f32)
                    .put(";");

                // (retVal.x + 0.5) / width;
                ss.new_line()
                    .put("retVal.x = (retVal.x + 0.5) / ")
                    .put(width as f32)
                    .put(";");
                // (retVal.x + 0.5) / height;
                ss.new_line()
                    .put("retVal.y = (retVal.y + 0.5) / ")
                    .put(height as f32)
                    .put(";");
            } else {
                // (dep + 0.5) / width;
                ss.new_line()
                    .put("retVal.x = (dep + 0.5) / ")
                    .put(width as f32)
                    .put(";");
                ss.new_line().put("retVal.y = 0.5;");
            }

            ss.new_line().put("return retVal;");
            ss.dedent();
            ss.new_line().put("}");

            shader_creator.add_to_helper_shader_code(ss.string());
        }
    } else {
        // Create a 1D texture declaration.
        let ss = GpuShaderText::new(language);
        ss.declare_tex1d(
            &name,
            shader_creator.descriptor_set_index(),
            texture_shader_binding_index,
        )?;
        shader_creator.add_to_texture_declare_shader_code(ss.string());
    }

    let ss = GpuShaderText::new(language);
    ss.indent();

    let pixel = shader_creator.pixel_name().to_vec();
    let hue_adjust = lut_data.get_hue_adjust() == Lut1DHueAdjust::Dw3;

    ss.new_line().put("");
    ss.new_line()
        .put("// Add LUT 1D processing for ")
        .put(&name);
    ss.new_line().put("");

    ss.new_line().put("{");
    ss.indent();

    if hue_adjust {
        ss.new_line().put("// Add the pre hue adjustment");
        ss.new_line()
            .put(ss.float3_decl("maxval")?)
            .put(" = max(")
            .put(&pixel)
            .put(".rgb, max(")
            .put(&pixel)
            .put(".gbr, ")
            .put(&pixel)
            .put(".brg));");
        ss.new_line()
            .put(ss.float3_decl("minval")?)
            .put(" = min(")
            .put(&pixel)
            .put(".rgb, min(")
            .put(&pixel)
            .put(".gbr, ")
            .put(&pixel)
            .put(".brg));");
        ss.new_line()
            .put("float oldChroma = max(1e-8, maxval.r - minval.r);");
        ss.new_line()
            .put(ss.float3_decl("delta")?)
            .put(" = ")
            .put(&pixel)
            .put(".rgb - minval;");
        ss.new_line().put("");
    }

    let (g_swizzle, b_swizzle) = if single_channel {
        (".r;", ".r;")
    } else {
        (".g;", ".b;")
    };

    if dimensions == TextureDimensions::D2 {
        let str = [name.as_slice(), b"_computePos(", &pixel].concat();

        ss.new_line()
            .put(&pixel)
            .put(".r = ")
            .put(ss.sample_tex2d(&name, [str.as_slice(), b".r)"].concat())?)
            .put(".r;");

        ss.new_line()
            .put(&pixel)
            .put(".g = ")
            .put(ss.sample_tex2d(&name, [str.as_slice(), b".g)"].concat())?)
            .put(g_swizzle);

        ss.new_line()
            .put(&pixel)
            .put(".b = ")
            .put(ss.sample_tex2d(&name, [str.as_slice(), b".b)"].concat())?)
            .put(b_swizzle);
    } else {
        let dim = lut_data.get_array().get_length() as f32;
        let coords = [name.as_slice(), b"_coords"].concat();

        ss.new_line()
            .put(ss.float3_decl(&coords)?)
            .put(" = (")
            .put(&pixel)
            .put(".rgb * ")
            .put(ss.float3_splat_f32(dim - 1.0))
            .put(" + ")
            .put(ss.float3_splat_f32(0.5))
            .put(" ) / ")
            .put(ss.float3_splat_f32(dim))
            .put(";");

        ss.new_line()
            .put(&pixel)
            .put(".r = ")
            .put(ss.sample_tex1d(&name, [coords.as_slice(), b".r"].concat())?)
            .put(".r;");

        ss.new_line()
            .put(&pixel)
            .put(".g = ")
            .put(ss.sample_tex1d(&name, [coords.as_slice(), b".g"].concat())?)
            .put(g_swizzle);

        ss.new_line()
            .put(&pixel)
            .put(".b = ")
            .put(ss.sample_tex1d(&name, [coords.as_slice(), b".b"].concat())?)
            .put(b_swizzle);
    }

    if hue_adjust {
        ss.new_line().put("");
        ss.new_line().put("// Add the post hue adjustment");
        ss.new_line()
            .put(ss.float3_decl("maxval2")?)
            .put(" = max(")
            .put(&pixel)
            .put(".rgb, max(")
            .put(&pixel)
            .put(".gbr, ")
            .put(&pixel)
            .put(".brg));");
        ss.new_line()
            .put(ss.float3_decl("minval2")?)
            .put(" = min(")
            .put(&pixel)
            .put(".rgb, min(")
            .put(&pixel)
            .put(".gbr, ")
            .put(&pixel)
            .put(".brg));");
        ss.new_line()
            .put("float newChroma = maxval2.r - minval2.r;");
        ss.new_line()
            .put(&pixel)
            .put(".rgb = minval2.r + delta * newChroma / oldChroma;");
    }

    ss.dedent();
    ss.new_line().put("}");

    shader_creator.add_to_function_shader_code(ss.string());
    Ok(())
}

/// Writes a 1D LUT op's shader. An inverse LUT is first made into its fast forward LUT, as
/// the GPU has no exact inverse (`MakeFastLut1DFromInverse`); upstream's "Cannot apply
/// Lut1DOp, inversion failed." is for a null result, which that function never returns.
///
/// Port of `Lut1DOp::extractGpuShaderInfo` (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:157-175 @
/// v2.5.2).
pub fn extract_lut1d_gpu_shader_info(
    shader_creator: &mut GpuShaderDesc,
    lut_data: &Lut1DOpData,
) -> Result<()> {
    if lut_data.get_direction() == TransformDirection::Inverse {
        // TODO: Even if the optim flags specify exact inversion, only fast inversion is
        // supported on the GPU. Add GPU renderer for EXACT mode.

        let tmp = make_fast_lut1d_from_inverse(lut_data)?;
        return get_lut1d_gpu_shader_program(shader_creator, &tmp);
    }

    get_lut1d_gpu_shader_program(shader_creator, lut_data)
}

#[cfg(test)]
#[path = "lut1d_op_gpu_tests.rs"]
mod tests;
