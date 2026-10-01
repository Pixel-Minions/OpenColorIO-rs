// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/matrix/MatrixOpGPU.h` and `MatrixOpGPU.cpp` @ v2.5.2: the
//! Matrix op's GPU writer.

use ocio_ops::Result;
use ocio_ops::ops::matrix::MatrixOpData;

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

/// Adds the code of a matrix with offsets to `shader_creator`'s function body, as a block
/// that computes `res` from the pixel and writes it back:
/// - a matrix whose diagonal is all ones and whose other values are zeros is left out;
/// - a diagonal one is 4 products, written as `float`s (rounded from the matrix's `double`s);
/// - any other is a product with the whole matrix, written as `double`s (`mat4fMul`);
/// - the offsets, when one isn't zero, are added, written as `float`s.
///
/// Port of `GetMatrixGPUShaderProgram` (MatrixOpGPU.cpp:13-69 @ v2.5.2).
pub fn get_matrix_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    matrix: &MatrixOpData,
) -> Result<()> {
    let ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    ss.new_line().put("");
    ss.new_line().put("// Add Matrix processing");
    ss.new_line().put("");

    ss.new_line().put("{");
    ss.indent();

    let values = matrix.get_array().get_values();
    let offs = matrix.get_offsets().get_values();

    let pxl = shader_creator.pixel_name().to_vec();
    let channel = |suffix: &str| [pxl.as_slice(), suffix.as_bytes()].concat();

    let res_decl = ss.float4_decl("res")?;
    ss.new_line()
        .put(res_decl)
        .put(" = ")
        .put(ss.float4_const(
            channel(".rgb.r"),
            channel(".rgb.g"),
            channel(".rgb.b"),
            channel(".a"),
        ))
        .put(";");

    if !matrix.is_unity_diagonal() {
        if matrix.is_diagonal() {
            ss.new_line()
                .put("res = ")
                .put(ss.float4_const_f32(
                    values[0] as f32,
                    values[5] as f32,
                    values[10] as f32,
                    values[15] as f32,
                ))
                .put(" * res;");
        } else {
            // NOTE: The in-place matrix computation is not supported by OSL so,
            // a temporary variable is needed.
            let tmp_decl = ss.float4_decl("tmp")?;
            ss.new_line().put(tmp_decl).put(" = res;");
            let m4x4: &[f64; 16] = values
                .as_slice()
                .try_into()
                .expect("a validated matrix has 16 values");
            let product = ss.mat4f_mul_f64(m4x4, "tmp")?;
            ss.new_line().put("res = ").put(product).put(";");
        }
    }

    if matrix.has_offsets() {
        ss.new_line()
            .put("res = ")
            .put(ss.float4_const_f32(
                offs[0] as f32,
                offs[1] as f32,
                offs[2] as f32,
                offs[3] as f32,
            ))
            .put(" + res;");
    }

    ss.new_line()
        .put(&pxl)
        .put(".rgb = ")
        .put(ss.float3_const("res.x", "res.y", "res.z"))
        .put(";");
    ss.new_line().put(&pxl).put(".a = res.w;");

    ss.dedent();
    ss.new_line().put("}");

    shader_creator.add_to_function_shader_code(ss.string());
    Ok(())
}
