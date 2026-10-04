// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/ops/range/RangeOpGPU.h` and `RangeOpGPU.cpp` @ v2.5.2: the Range
//! op's GPU writer.

use ocio_ops::Result;
use ocio_ops::ops::range::RangeOpData;

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;

/// Adds the code of a Range op to `shader_creator`'s function body, as a block on the pixel's
/// RGB (alpha passes through), each step written with `double`s:
/// - `rgb * scale + offset`, when the range [`scales`](RangeOpData::scales);
/// - `max(minOut, rgb)`, unless the minimum is empty;
/// - `min(maxOut, rgb)`, unless the maximum is empty.
///
/// The scale and offset are the ones the data's last `validate` computed.
///
/// Port of `GetRangeGPUShaderProgram` (RangeOpGPU.cpp:15-82 @ v2.5.2).
pub fn get_range_gpu_shader_program(
    shader_creator: &mut GpuShaderDesc,
    range: &RangeOpData,
) -> Result<()> {
    let ss = GpuShaderText::new(shader_creator.language());
    ss.indent();

    ss.new_line().put("");
    ss.new_line().put("// Add Range processing");
    ss.new_line().put("");
    ss.new_line().put("{");
    ss.indent();

    let pix = shader_creator.pixel_name();
    let pixrgb = [pix, b".rgb"].concat();

    if range.scales() {
        let scale = [range.get_scale(), range.get_scale(), range.get_scale()];

        let offset = [range.get_offset(), range.get_offset(), range.get_offset()];

        ss.new_line()
            .put(&pixrgb)
            .put(" = ")
            .put(&pixrgb)
            .put(" * ")
            .put(ss.float3_const_f64(scale[0], scale[1], scale[2]))
            .put(" + ")
            .put(ss.float3_const_f64(offset[0], offset[1], offset[2]))
            .put(";");
    }

    if !range.min_is_empty() {
        let lower_bound = [
            range.get_min_out_value(),
            range.get_min_out_value(),
            range.get_min_out_value(),
        ];

        ss.new_line()
            .put(&pixrgb)
            .put(" = ")
            .put("max(")
            .put(ss.float3_const_f64(lower_bound[0], lower_bound[1], lower_bound[2]))
            .put(", ")
            .put(&pixrgb)
            .put(");");
    }

    if !range.max_is_empty() {
        let upper_bound = [
            range.get_max_out_value(),
            range.get_max_out_value(),
            range.get_max_out_value(),
        ];

        ss.new_line()
            .put(&pixrgb)
            .put(" = ")
            .put("min(")
            .put(ss.float3_const_f64(upper_bound[0], upper_bound[1], upper_bound[2]))
            .put(", ")
            .put(&pixrgb)
            .put(");");
    }

    ss.dedent();
    ss.new_line().put("}");

    ss.dedent();
    shader_creator.add_to_function_shader_code(ss.string());
    Ok(())
}
