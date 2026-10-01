// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Exponent op's CPU renderer, `ExponentOpCPU`: part of a port of
//! `src/OpenColorIO/ops/exponent/ExponentOp.cpp` @ v2.5.2. One scalar renderer: upstream has
//! no SIMD version and no fast-math version (`ExponentOp::getCPUOp` ignores the flag).

use std::sync::Arc;

use super::exponent_op_data::ExponentOpData;
use crate::math_utils::std_max;
use crate::op::CpuOp;

/// `out = powf(std::max(0.0f, in), exponent)` on every channel, alpha included, with the
/// exponents rounded to `float`. A NaN input becomes 0 before the power
/// (`std::max(0.0f, NaN)` is 0).
///
/// Port of `ExponentOpCPU` (ExponentOp.cpp:98-130 @ v2.5.2).
#[derive(Debug)]
pub struct ExponentOpCpu {
    /// The exponents, as `float`s.
    exp: [f32; 4],
}

impl ExponentOpCpu {
    /// The renderer of `data`.
    ///
    /// Port of `ExponentOpCPU::ExponentOpCPU` and the exponents' cast in
    /// `ExponentOpCPU::apply` (ExponentOp.cpp:101, 115-118 @ v2.5.2).
    pub fn new(data: &ExponentOpData) -> ExponentOpCpu {
        ExponentOpCpu {
            exp: data.exp4.map(|e| e as f32),
        }
    }
}

impl CpuOp for ExponentOpCpu {
    /// Port of `ExponentOpCPU::apply` (ExponentOp.cpp:110-130 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let exp = self.exp;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[0] = std_max(0.0f32, pixel[0]).powf(exp[0]);
            pixel[1] = std_max(0.0f32, pixel[1]).powf(exp[1]);
            pixel[2] = std_max(0.0f32, pixel[2]).powf(exp[2]);
            pixel[3] = std_max(0.0f32, pixel[3]).powf(exp[3]);
        }
    }
}

/// The renderer of `data`.
///
/// Port of `ExponentOp::getCPUOp` (ExponentOp.cpp:253-256 @ v2.5.2).
pub fn get_exponent_renderer(data: &ExponentOpData) -> Arc<dyn CpuOp> {
    Arc::new(ExponentOpCpu::new(data))
}
