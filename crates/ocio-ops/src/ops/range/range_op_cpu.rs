// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Range op's CPU renderers: a port of `src/OpenColorIO/ops/range/RangeOpCPU.h` and
//! `RangeOpCPU.cpp` @ v2.5.2.
//!
//! [`get_range_renderer`] picks one by the bounds: [`RangeMaxRenderer`] without a lower bound,
//! [`RangeMinRenderer`] without an upper one, and with both [`RangeMinMaxRenderer`] or, when the
//! range scales or offsets, [`RangeScaleMinMaxRenderer`]. They compute in `float`: the
//! constructor rounds the data's scale, offset and output bounds to `float`. They never write
//! alpha, which upstream copies (`out[3] = in[3]`).
//!
//! The clamps are `std::min` and `std::max`, which pick an operand, so a NaN input becomes the
//! bound. Both wheels compile them to that (MSVC `comiss` and a conditional move, GCC `minss`,
//! `maxss`, `minps`, `maxps` or a compare and a blend with the bound second), and keep the bits
//! of the operand they pick.
//!
//! # Operand orders
//!
//! [`RangeScaleMinMaxRenderer`] computes `in * scale + offset`, then clamps. Neither wheel
//! keeps the source's operand order everywhere: MSVC (Windows) computes `scale * in`
//! (`RangeScaleMinMaxRenderer::apply` at 0x1802bad98, 0x1802bb0c6), and GCC (Linux) `in *
//! scale` in its vector loops (0x4f1657, 0x4f17e3) but `scale * in` for blue in its one-pixel
//! code (0x4f1904, 0x4f199e). The order only decides which NaN a product returns when `in` and
//! the scale are both NaN, and the clamp then turns any NaN into the lower bound, which is a
//! number for this renderer (both bounds are set). So no order shows in the output, and the
//! port keeps the source's.

use std::sync::Arc;

use super::range_op_data::RangeOpData;
use crate::exception::{Exception, Result};
use crate::math_utils::{clamp, sse_add, sse_mul, std_max, std_min};
use crate::op::CpuOp;
use crate::open_color_types::TransformDirection;

/// The renderers' parameters: the scale and offset, and the output bounds, as `float`.
///
/// Port of `RangeOpCPU` (src/OpenColorIO/ops/range/RangeOpCPU.cpp:16-30, 65-76 @ v2.5.2).
#[derive(Debug, Clone, Copy)]
struct RangeOpCpu {
    /// `m_scale`.
    scale: f32,
    /// `m_offset`.
    offset: f32,
    /// `m_lowerBound`: the minimum output value.
    lower_bound: f32,
    /// `m_upperBound`: the maximum output value.
    upper_bound: f32,
}

impl RangeOpCpu {
    /// Port of `RangeOpCPU::RangeOpCPU` (RangeOpCPU.cpp:65-76 @ v2.5.2).
    fn new(range: &RangeOpData) -> Self {
        RangeOpCpu {
            scale: range.get_scale() as f32,
            offset: range.get_offset() as f32,
            lower_bound: range.get_min_out_value() as f32,
            upper_bound: range.get_max_out_value() as f32,
        }
    }
}

/// Both bounds, with a scale or an offset: `clamp(in * scale + offset)`.
///
/// Port of `RangeScaleMinMaxRenderer` (src/OpenColorIO/ops/range/RangeOpCPU.cpp:32-38, 78-103
/// @ v2.5.2).
#[derive(Debug)]
pub struct RangeScaleMinMaxRenderer(RangeOpCpu);

impl RangeScaleMinMaxRenderer {
    /// Port of `RangeScaleMinMaxRenderer::RangeScaleMinMaxRenderer` (RangeOpCPU.cpp:78-81 @
    /// v2.5.2).
    pub fn new(range: &RangeOpData) -> Self {
        RangeScaleMinMaxRenderer(RangeOpCpu::new(range))
    }
}

impl CpuOp for RangeScaleMinMaxRenderer {
    /// Port of `RangeScaleMinMaxRenderer::apply` (RangeOpCPU.cpp:83-103 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let p = &self.0;
        for px in rgba.as_chunks_mut::<4>().0 {
            let t = [
                sse_add(sse_mul(px[0], p.scale), p.offset),
                sse_add(sse_mul(px[1], p.scale), p.offset),
                sse_add(sse_mul(px[2], p.scale), p.offset),
            ];

            // NaNs become m_lowerBound.
            px[0] = clamp(t[0], p.lower_bound, p.upper_bound);
            px[1] = clamp(t[1], p.lower_bound, p.upper_bound);
            px[2] = clamp(t[2], p.lower_bound, p.upper_bound);
        }
    }
}

/// Both bounds, no scale or offset: `clamp(in)`.
///
/// Port of `RangeMinMaxRenderer` (src/OpenColorIO/ops/range/RangeOpCPU.cpp:40-46, 105-126 @
/// v2.5.2).
#[derive(Debug)]
pub struct RangeMinMaxRenderer(RangeOpCpu);

impl RangeMinMaxRenderer {
    /// Port of `RangeMinMaxRenderer::RangeMinMaxRenderer` (RangeOpCPU.cpp:105-108 @ v2.5.2).
    pub fn new(range: &RangeOpData) -> Self {
        RangeMinMaxRenderer(RangeOpCpu::new(range))
    }
}

impl CpuOp for RangeMinMaxRenderer {
    /// Port of `RangeMinMaxRenderer::apply` (RangeOpCPU.cpp:110-126 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let p = &self.0;
        for px in rgba.as_chunks_mut::<4>().0 {
            // NaNs become m_lowerBound.
            px[0] = clamp(px[0], p.lower_bound, p.upper_bound);
            px[1] = clamp(px[1], p.lower_bound, p.upper_bound);
            px[2] = clamp(px[2], p.lower_bound, p.upper_bound);
        }
    }
}

/// A lower bound only: `std::max(lowerBound, in)`.
///
/// Port of `RangeMinRenderer` (src/OpenColorIO/ops/range/RangeOpCPU.cpp:48-54, 128-149 @
/// v2.5.2).
#[derive(Debug)]
pub struct RangeMinRenderer(RangeOpCpu);

impl RangeMinRenderer {
    /// Port of `RangeMinRenderer::RangeMinRenderer` (RangeOpCPU.cpp:128-131 @ v2.5.2).
    pub fn new(range: &RangeOpData) -> Self {
        RangeMinRenderer(RangeOpCpu::new(range))
    }
}

impl CpuOp for RangeMinRenderer {
    /// Port of `RangeMinRenderer::apply` (RangeOpCPU.cpp:133-149 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let lower = self.0.lower_bound;
        for px in rgba.as_chunks_mut::<4>().0 {
            // NaNs become m_lowerBound.
            px[0] = std_max(lower, px[0]);
            px[1] = std_max(lower, px[1]);
            px[2] = std_max(lower, px[2]);
        }
    }
}

/// An upper bound only: `std::min(upperBound, in)`.
///
/// Port of `RangeMaxRenderer` (src/OpenColorIO/ops/range/RangeOpCPU.cpp:56-62, 151-172 @
/// v2.5.2).
#[derive(Debug)]
pub struct RangeMaxRenderer(RangeOpCpu);

impl RangeMaxRenderer {
    /// Port of `RangeMaxRenderer::RangeMaxRenderer` (RangeOpCPU.cpp:151-154 @ v2.5.2).
    pub fn new(range: &RangeOpData) -> Self {
        RangeMaxRenderer(RangeOpCpu::new(range))
    }
}

impl CpuOp for RangeMaxRenderer {
    /// Port of `RangeMaxRenderer::apply` (RangeOpCPU.cpp:156-172 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let upper = self.0.upper_bound;
        for px in rgba.as_chunks_mut::<4>().0 {
            // NaNs become m_upperBound.
            px[0] = std_min(upper, px[0]);
            px[1] = std_min(upper, px[1]);
            px[2] = std_min(upper, px[2]);
        }
    }
}

/// The renderer of a forward range: "Op::finalize has to be called." for an inverse one.
///
/// Port of `GetRangeRenderer` (src/OpenColorIO/ops/range/RangeOpCPU.cpp:175-198 @ v2.5.2).
pub fn get_range_renderer(range: &RangeOpData) -> Result<Arc<dyn CpuOp>> {
    let range_fwd = range;
    if range.get_direction() == TransformDirection::Inverse {
        return Err(Exception::new("Op::finalize has to be called."));
    }
    // Both min & max can not be empty at the same time.
    if range_fwd.min_is_empty() {
        return Ok(Arc::new(RangeMaxRenderer::new(range_fwd)));
    } else if range_fwd.max_is_empty() {
        return Ok(Arc::new(RangeMinRenderer::new(range_fwd)));
    }

    // Both min and max have values.
    if !range_fwd.scales() {
        return Ok(Arc::new(RangeMinMaxRenderer::new(range_fwd)));
    }
    Ok(Arc::new(RangeScaleMinMaxRenderer::new(range_fwd)))
}

#[cfg(test)]
#[path = "range_op_cpu_tests.rs"]
mod tests;
