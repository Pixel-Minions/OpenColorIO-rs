// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Rendering values through ops: a port of `src/OpenColorIO/ops/OpTools.h` and `OpTools.cpp`
//! @ v2.5.2.

use crate::exception::Result;
use crate::op::OpVec;
use crate::open_color_types::OptimizationFlags;

/// Renders `num_pixels` RGB values of `input` through `ops` into `output` (which may be the
/// same values): as RGBA with an alpha of 1, after finalizing the ops and removing the no-op
/// types (`optimize(OPTIMIZATION_NONE)`), each op's renderer without fast math in place
/// (`Op::apply`).
///
/// Port of `EvalTransform` (src/OpenColorIO/ops/OpTools.cpp:11-46 @ v2.5.2).
pub fn eval_transform(
    input: &[f32],
    output: &mut [f32],
    num_pixels: usize,
    ops: &mut OpVec,
) -> Result<()> {
    let mut tmp = vec![0.0f32; num_pixels * 4];

    // Render the LUT entries (domain) through the ops.
    for idx in 0..num_pixels {
        tmp[4 * idx] = input[3 * idx];
        tmp[4 * idx + 1] = input[3 * idx + 1];
        tmp[4 * idx + 2] = input[3 * idx + 2];
        tmp[4 * idx + 3] = 1.0f32;
    }

    ops.finalize()?;
    ops.optimize(OptimizationFlags::NONE)?;

    for op in ops.iter() {
        op.apply(&mut tmp)?;
    }

    for idx in 0..num_pixels {
        output[3 * idx] = tmp[4 * idx];
        output[3 * idx + 1] = tmp[4 * idx + 1];
        output[3 * idx + 2] = tmp[4 * idx + 2];
    }
    Ok(())
}
