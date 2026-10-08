// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/lut3d/Lut3DOpGPU.h` and `Lut3DOpGPU.cpp` @ v2.5.2, the Lut3D
//! op's GPU writer, and of `Lut3DOp::extractGpuShaderInfo`
//! (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:203-219), which writes an inverse LUT as its fast
//! forward LUT.
//!
//! The LUT goes into the shader as a 3D texture of its values (blue fastest). Tetrahedral
//! interpolation is written out in the shader from four texels fetched with nearest
//! sampling; the other interpolations use the GPU's own trilinear sampling.

use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::lut3d::lut3d_op_data::{
    Interpolation, Lut3DOpData, make_fast_lut3d_from_inverse,
};
use ocio_ops::utils::string_utils::replace_in_place;
use ocio_ops::{Exception, Result};

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;
use crate::open_color_types::GpuLanguage;

/// One of the six tetrahedra of a cube, as the shader picks it: the offsets of its second and
/// third corners (in the texture's BGR order) and the expressions of its four weights.
struct Tetrahedron {
    v2: [f32; 3],
    v3: [f32; 3],
    f1: &'static str,
    f4: &'static str,
    f2: &'static str,
    f3: &'static str,
}

/// Writes the block of one tetrahedron: its two middle corners, its four weights, and the sum
/// of the middle corners' terms into the pixel.
fn write_tetrahedron(
    ss: &GpuShaderText,
    name: &[u8],
    pixel_name: &[u8],
    t: &Tetrahedron,
) -> Result<()> {
    ss.new_line()
        .put("nextInd = baseInd + ")
        .put(ss.float3_const_f32(t.v2[0], t.v2[1], t.v2[2]))
        .put(";");
    ss.new_line()
        .put(ss.float3_decl("v2")?)
        .put(" = ")
        .put(ss.sample_tex3d(name, "nextInd")?)
        .put(".rgb;");

    ss.new_line()
        .put("nextInd = baseInd + ")
        .put(ss.float3_const_f32(t.v3[0], t.v3[1], t.v3[2]))
        .put(";");
    ss.new_line()
        .put(ss.float3_decl("v3")?)
        .put(" = ")
        .put(ss.sample_tex3d(name, "nextInd")?)
        .put(".rgb;");

    ss.new_line()
        .put("f1 = ")
        .put(ss.float3_splat(t.f1))
        .put(";");
    ss.new_line()
        .put("f4 = ")
        .put(ss.float3_splat(t.f4))
        .put(";");
    ss.new_line()
        .put(ss.float3_decl("f2")?)
        .put(" = ")
        .put(ss.float3_splat(t.f2))
        .put(";");
    ss.new_line()
        .put(ss.float3_decl("f3")?)
        .put(" = ")
        .put(ss.float3_splat(t.f3))
        .put(";");

    ss.new_line()
        .put(pixel_name)
        .put(".rgb = (f2 * v2) + (f3 * v3);");
    Ok(())
}

/// Adds a 3D LUT's texture, its declaration, and its code to `shader_creator`: tetrahedral
/// interpolation written out from nearest samples, or the GPU's trilinear sampling. OSL is
/// refused ("The Lut3DOp is not yet supported by the 'Open Shading language (OSL)'
/// translation").
///
/// Port of `GetLut3DGPUShaderProgram` (src/OpenColorIO/ops/lut3d/Lut3DOpGPU.cpp:17-249 @
/// v2.5.2).
pub fn get_lut3d_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    lut_data: &Lut3DOpData,
) -> Result<()> {
    if shader_creator.language() == GpuLanguage::Osl1 {
        return Err(Exception::new(
            "The Lut3DOp is not yet supported by the 'Open Shading language (OSL)' translation",
        ));
    }

    let index = shader_creator.next_resource_index();
    let mut name = [
        shader_creator.resource_prefix(),
        b"_",
        b"lut3d_",
        index.to_string().as_bytes(),
    ]
    .concat();

    // Note: Remove potentially problematic double underscores from GLSL resource names.
    replace_in_place(&mut name, b"__", b"_");

    let mut sampler_interpolation = lut_data.get_concrete_interpolation();
    // Enforce GL_NEAREST with shader-generated tetrahedral interpolation.
    if sampler_interpolation == Interpolation::Tetrahedral {
        sampler_interpolation = Interpolation::Nearest;
    }

    // (A grid size is at most 129.)
    let grid_size = lut_data.get_grid_size() as usize as u32;

    // Copy the LUT into the shaderCreator as a Texture object.
    let texture_shader_binding_index = shader_creator.add_3d_texture(
        &name,
        GpuShaderText::get_sampler_name(&name),
        grid_size,
        sampler_interpolation,
        lut_data.get_array().get_values(),
    )?;

    // Create the texture declaration.
    {
        let ss = GpuShaderText::new(shader_creator.language());
        ss.declare_tex3d(
            &name,
            shader_creator.descriptor_set_index(),
            texture_shader_binding_index,
        )?;
        shader_creator.add_to_texture_declare_shader_code(ss.string());
    }

    let dim = grid_size as f32;

    // incr = 1/dim (amount needed to increment one index in the grid)
    let incr = 1.0f32 / dim;

    let pixel_name = shader_creator.pixel_name().to_vec();

    let ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    ss.new_line().put("");
    ss.new_line()
        .put("// Add LUT 3D processing for ")
        .put(&name);
    ss.new_line().put("");

    // Tetrahedral interpolation
    // The strategy is to use texture3d lookups with GL_NEAREST to fetch the 4 corners of the
    // cube (v1,v2,v3,v4), compute the 4 barycentric weights (f1,f2,f3,f4), and then perform
    // the interpolation manually. One side benefit of this is that we are not subject to the
    // 8-bit quantization of the fractional weights that happens using GL_LINEAR.
    if lut_data.get_concrete_interpolation() == Interpolation::Tetrahedral {
        ss.new_line().put("{");
        ss.indent();

        ss.new_line()
            .put(ss.float3_decl("coords")?)
            .put(" = ")
            .put(&pixel_name)
            .put(".rgb * ")
            .put(ss.float3_splat_f32(dim - 1.0f32))
            .put("; ");

        // baseInd is on [0,dim-1]
        ss.new_line()
            .put(ss.float3_decl("baseInd")?)
            .put(" = floor(coords);");

        // frac is on [0,1]
        ss.new_line()
            .put(ss.float3_decl("frac")?)
            .put(" = coords - baseInd;");

        // scale/offset baseInd onto [0,1] as usual for doing texture lookups
        // we use zyx to flip the order since blue varies most rapidly
        // in the grid array ordering
        ss.new_line().put(ss.float3_decl("f1, f4")?).put(";");

        ss.new_line()
            .put("baseInd = ( baseInd.zyx + ")
            .put(ss.float3_splat_f32(0.5f32))
            .put(" ) / ")
            .put(ss.float3_splat_f32(dim))
            .put(";");
        ss.new_line()
            .put(ss.float3_decl("v1")?)
            .put(" = ")
            .put(ss.sample_tex3d(&name, "baseInd")?)
            .put(".rgb;");

        ss.new_line()
            .put(ss.float3_decl("nextInd")?)
            .put(" = baseInd + ")
            .put(ss.float3_splat_f32(incr))
            .put(";");
        ss.new_line()
            .put(ss.float3_decl("v4")?)
            .put(" = ")
            .put(ss.sample_tex3d(&name, "nextInd")?)
            .put(".rgb;");

        // Note that compared to the CPU version of the algorithm, we increment in inverted
        // order since baseInd & nextInd are essentially BGR rather than RGB.
        let block = |condition: Option<&str>, t: &Tetrahedron| -> Result<()> {
            if let Some(condition) = condition {
                ss.new_line().put(condition);
            }
            ss.new_line().put("{");
            ss.indent();
            write_tetrahedron(&ss, &name, &pixel_name, t)?;
            ss.dedent();
            ss.new_line().put("}");
            Ok(())
        };

        ss.new_line().put("if (frac.r >= frac.g)");
        ss.new_line().put("{");
        ss.indent();
        // R > G > B
        block(
            Some("if (frac.g >= frac.b)"),
            &Tetrahedron {
                v2: [0.0, 0.0, incr],
                v3: [0.0, incr, incr],
                f1: "1. - frac.r",
                f4: "frac.b",
                f2: "frac.r - frac.g",
                f3: "frac.g - frac.b",
            },
        )?;
        // R > B > G
        block(
            Some("else if (frac.r >= frac.b)"),
            &Tetrahedron {
                v2: [0.0, 0.0, incr],
                v3: [incr, 0.0, incr],
                f1: "1. - frac.r",
                f4: "frac.g",
                f2: "frac.r - frac.b",
                f3: "frac.b - frac.g",
            },
        )?;
        // B > R > G
        block(
            Some("else"),
            &Tetrahedron {
                v2: [incr, 0.0, 0.0],
                v3: [incr, 0.0, incr],
                f1: "1. - frac.b",
                f4: "frac.g",
                f2: "frac.b - frac.r",
                f3: "frac.r - frac.g",
            },
        )?;
        ss.dedent();
        ss.new_line().put("}");
        ss.new_line().put("else");
        ss.new_line().put("{");
        ss.indent();
        // B > G > R
        block(
            Some("if (frac.g <= frac.b)"),
            &Tetrahedron {
                v2: [incr, 0.0, 0.0],
                v3: [incr, incr, 0.0],
                f1: "1. - frac.b",
                f4: "frac.r",
                f2: "frac.b - frac.g",
                f3: "frac.g - frac.r",
            },
        )?;
        // G > R > B
        block(
            Some("else if (frac.r >= frac.b)"),
            &Tetrahedron {
                v2: [0.0, incr, 0.0],
                v3: [0.0, incr, incr],
                f1: "1. - frac.g",
                f4: "frac.b",
                f2: "frac.g - frac.r",
                f3: "frac.r - frac.b",
            },
        )?;
        // G > B > R
        block(
            Some("else"),
            &Tetrahedron {
                v2: [0.0, incr, 0.0],
                v3: [incr, incr, 0.0],
                f1: "1. - frac.g",
                f4: "frac.r",
                f2: "frac.g - frac.b",
                f3: "frac.b - frac.r",
            },
        )?;
        ss.dedent();
        ss.new_line().put("}");

        ss.new_line()
            .put(&pixel_name)
            .put(".rgb = ")
            .put(&pixel_name)
            .put(".rgb + (f1 * v1) + (f4 * v4);");

        ss.dedent();
        ss.new_line().put("}");
    } else {
        // Trilinear interpolation
        // Use texture3d and GL_LINEAR and the GPU's built-in trilinear algorithm.
        // Note that the fractional components are quantized to 8-bits on some hardware,
        // which introduces significant error with small grid sizes.

        let coords = [name.as_slice(), b"_coords"].concat();
        ss.new_line()
            .put(ss.float3_decl(&coords)?)
            .put(" = (")
            .put(&pixel_name)
            .put(".zyx * ")
            .put(ss.float3_splat_f32(dim - 1.0f32))
            .put(" + ")
            .put(ss.float3_splat_f32(0.5f32))
            .put(") / ")
            .put(ss.float3_splat_f32(dim))
            .put(";");

        ss.new_line()
            .put(&pixel_name)
            .put(".rgb = ")
            .put(ss.sample_tex3d(&name, &coords)?)
            .put(".rgb;");
    }

    shader_creator.add_to_function_shader_code(ss.string());
    Ok(())
}

/// Writes a Lut3D op: an inverse LUT as its fast forward LUT ([`make_fast_lut3d_from_inverse`];
/// the exact inverse has no GPU writer), then [`get_lut3d_gpu_shader_program`].
///
/// Port of `Lut3DOp::extractGpuShaderInfo` (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:203-219 @
/// v2.5.2). Upstream's "Cannot apply Lut3DOp, inversion failed." is for a null result, which
/// `MakeFastLut3DFromInverse` never returns.
pub fn extract_lut3d_gpu_shader_info(
    shader_creator: &mut GpuShaderDesc,
    lut_data: &Lut3DOpData,
) -> Result<()> {
    if lut_data.get_direction() == TransformDirection::Inverse {
        // TODO: Add GPU renderer for EXACT mode.
        let tmp = make_fast_lut3d_from_inverse(lut_data)?;
        return get_lut3d_gpu_shader_program(shader_creator, &tmp);
    }
    get_lut3d_gpu_shader_program(shader_creator, lut_data)
}
