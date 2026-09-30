// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma op's CPU renderers.
//!
//! Port of `src/OpenColorIO/ops/gamma/GammaOpCPU.cpp` @ v2.5.2. Each C++ renderer class is a
//! struct here, and each `...SSE` class, which the wheel uses when `OPTIMIZATION_FAST_LOG_EXP_POW`
//! is set, is its own struct that reproduces one lane of the SSE2 kernel (`crate::sse`). The
//! renderers process all four channels, alpha included, each with its own parameters.
//!
//! # The sign of the scalar mirror styles
//!
//! The scalar mirror renderers compute `std::copysign(1.0f, in) * value`. The two wheels were
//! compiled differently, which changes the sign of a NaN result (a NaN input with its sign bit
//! set), and nothing else:
//! - MSVC (Windows) computes `copysign(1.0f, in)` with bit operations and multiplies. A NaN
//!   `value` keeps its sign.
//! - GCC (Linux) turns `copysign(1.0f, x) * y` into `y XOR signbit(x)` (its `xorsign`
//!   pattern), except for the alpha channel of the moncurve mirror styles, where it builds the
//!   `±1.0f` and multiplies. A NaN `value` gets the input's sign.
//!
//! Both were read from the wheels' machine code (`docs/spikes/s2-s5.md`). The port does what
//! each platform's wheel does (PLAN.md D12), channel by channel.
//!
//! # Two NaN operands
//!
//! With NaN parameters, which OCIO 2.5.2 accepts, a NaN pixel can meet a NaN coefficient of a
//! moncurve renderer, and x86 returns the first operand's NaN. The moncurve renderers multiply
//! and add with [`sse_mul`]/[`sse_add`], in the operand order of upstream's source; Rust's `*`
//! and `+` would leave it to LLVM. (The sign factor of the scalar mirror styles is never NaN.)

use std::sync::Arc;

use super::gamma_op_data::{GammaOpData, GammaStyle};
use super::gamma_op_utils::{RendererParams, compute_params_fwd, compute_params_rev};
use crate::math_utils::{sse_add, sse_mul, std_max};
use crate::op::CpuOp;
use crate::sse::{EABS_MASK, ESIGN_MASK, sse_power};

/// The Gamma renderer for the op's style: the SSE kernel when `fast_power`
/// (`OPTIMIZATION_FAST_LOG_EXP_POW`) is set, the math-library one otherwise.
///
/// Port of `GetGammaRenderer` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:211-292 @ v2.5.2). Its
/// final `throw Exception("Unsupported Gamma style")` cannot be reached with a [`GammaStyle`].
pub fn get_gamma_renderer(gamma: &GammaOpData, fast_power: bool) -> Arc<dyn CpuOp> {
    match (gamma.style(), fast_power) {
        (GammaStyle::MoncurveFwd, true) => Arc::new(GammaMoncurveOpCpuFwdSse::new(gamma)),
        (GammaStyle::MoncurveFwd, false) => Arc::new(GammaMoncurveOpCpuFwd::new(gamma)),
        (GammaStyle::MoncurveRev, true) => Arc::new(GammaMoncurveOpCpuRevSse::new(gamma)),
        (GammaStyle::MoncurveRev, false) => Arc::new(GammaMoncurveOpCpuRev::new(gamma)),
        (GammaStyle::MoncurveMirrorFwd, true) => {
            Arc::new(GammaMoncurveMirrorOpCpuFwdSse::new(gamma))
        }
        (GammaStyle::MoncurveMirrorFwd, false) => Arc::new(GammaMoncurveMirrorOpCpuFwd::new(gamma)),
        (GammaStyle::MoncurveMirrorRev, true) => {
            Arc::new(GammaMoncurveMirrorOpCpuRevSse::new(gamma))
        }
        (GammaStyle::MoncurveMirrorRev, false) => Arc::new(GammaMoncurveMirrorOpCpuRev::new(gamma)),
        (GammaStyle::BasicFwd | GammaStyle::BasicRev, true) => {
            Arc::new(GammaBasicOpCpuSse::new(gamma))
        }
        (GammaStyle::BasicFwd | GammaStyle::BasicRev, false) => {
            Arc::new(GammaBasicOpCpu::new(gamma))
        }
        (GammaStyle::BasicMirrorFwd | GammaStyle::BasicMirrorRev, true) => {
            Arc::new(GammaBasicMirrorOpCpuSse::new(gamma))
        }
        (GammaStyle::BasicMirrorFwd | GammaStyle::BasicMirrorRev, false) => {
            Arc::new(GammaBasicMirrorOpCpu::new(gamma))
        }
        (GammaStyle::BasicPassThruFwd | GammaStyle::BasicPassThruRev, true) => {
            Arc::new(GammaBasicPassThruOpCpuSse::new(gamma))
        }
        (GammaStyle::BasicPassThruFwd | GammaStyle::BasicPassThruRev, false) => {
            Arc::new(GammaBasicPassThruOpCpu::new(gamma))
        }
    }
}

/// The RGBA pixels of a buffer.
fn pixels(rgba: &mut [f32]) -> &mut [[f32; 4]] {
    debug_assert!(
        rgba.len().is_multiple_of(4),
        "RGBA buffers hold whole pixels"
    );
    rgba.as_chunks_mut::<4>().0
}

/// How the wheel's compiler emitted `std::copysign(1.0f, in) * value` for one channel of a
/// scalar mirror renderer (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignOp {
    /// `copysign(1.0f, in) * value`.
    Multiply,
    /// `value` with its sign bit XOR-ed with `in`'s. (Only the Linux tables use it.)
    #[cfg_attr(windows, allow(dead_code))]
    XorSign,
}

/// `GammaBasicMirrorOpCPU::apply`, per channel (R, G, B, A), on this platform's wheel.
#[cfg(windows)]
const BASIC_MIRROR_SIGN: [SignOp; 4] = [SignOp::Multiply; 4];
#[cfg(not(windows))]
const BASIC_MIRROR_SIGN: [SignOp; 4] = [SignOp::XorSign; 4];

/// `GammaMoncurveMirrorOpCPUFwd::apply` and `GammaMoncurveMirrorOpCPURev::apply`, per channel
/// (R, G, B, A), on this platform's wheel.
#[cfg(windows)]
const MONCURVE_MIRROR_SIGN: [SignOp; 4] = [SignOp::Multiply; 4];
#[cfg(not(windows))]
const MONCURVE_MIRROR_SIGN: [SignOp; 4] = [
    SignOp::XorSign,
    SignOp::XorSign,
    SignOp::XorSign,
    SignOp::Multiply,
];

/// `std::copysign(1.0f, x) * value`, as `op` computes it.
#[inline]
fn apply_sign(op: SignOp, x: f32, value: f32) -> f32 {
    match op {
        SignOp::Multiply => value * 1.0f32.copysign(x),
        SignOp::XorSign => f32::from_bits(value.to_bits() ^ (x.to_bits() & ESIGN_MASK)),
    }
}

// -------------------------------------------------------------------------------------------
// Basic styles.

/// The four exponents of the basic styles.
fn basic_gammas(gamma: &GammaOpData) -> [f32; 4] {
    // The gamma calculations are done in normalized space.
    let style = gamma.style();
    let forward = style == GammaStyle::BasicFwd
        || style == GammaStyle::BasicMirrorFwd
        || style == GammaStyle::BasicPassThruFwd;

    // Calculate the actual power used in the function.
    [
        gamma.red_params(),
        gamma.green_params(),
        gamma.blue_params(),
        gamma.alpha_params(),
    ]
    .map(|p| (if forward { p[0] } else { 1.0 / p[0] }) as f32)
}

/// Basic style, clamping negatives: `pow(max(0, in), gamma)` with `powf`.
///
/// Port of `GammaBasicOpCPU` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:24-41, 295-362
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaBasicOpCpu {
    gamma: [f32; 4],
}

impl GammaBasicOpCpu {
    /// Port of `GammaBasicOpCPU::GammaBasicOpCPU` and `GammaBasicOpCPU::update`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:295-318 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaBasicOpCpu {
            gamma: basic_gammas(gamma),
        }
    }
}

impl CpuOp for GammaBasicOpCpu {
    /// Port of `GammaBasicOpCPU::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:342-362
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            let pixel = px.map(|v| std_max(0.0f32, v));
            for c in 0..4 {
                px[c] = pixel[c].powf(self.gamma[c]);
            }
        }
    }
}

/// [`GammaBasicOpCpu`] with `ssePower`, which maps every value that is not greater than 0
/// (NaN included) to +0.
///
/// Port of `GammaBasicOpCPUSSE` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:43-54, 320-340
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaBasicOpCpuSse(GammaBasicOpCpu);

impl GammaBasicOpCpuSse {
    /// Port of `GammaBasicOpCPUSSE::GammaBasicOpCPUSSE`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:47-50 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaBasicOpCpuSse(GammaBasicOpCpu::new(gamma))
    }
}

impl CpuOp for GammaBasicOpCpuSse {
    /// Port of `GammaBasicOpCPUSSE::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:321-339
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let gamma = self.0.gamma;
        for px in pixels(rgba) {
            for c in 0..4 {
                px[c] = sse_power(px[c], gamma[c]);
            }
        }
    }
}

/// Basic style, mirrored for negatives: `copysign(1, in) * pow(|in|, gamma)` with `powf`.
///
/// Port of `GammaBasicMirrorOpCPU` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:56-64, 364-414
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaBasicMirrorOpCpu(GammaBasicOpCpu);

impl GammaBasicMirrorOpCpu {
    /// Port of `GammaBasicMirrorOpCPU::GammaBasicMirrorOpCPU`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:364-367 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaBasicMirrorOpCpu(GammaBasicOpCpu::new(gamma))
    }
}

impl CpuOp for GammaBasicMirrorOpCpu {
    /// Port of `GammaBasicMirrorOpCPU::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:394-414
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let gamma = self.0.gamma;
        for px in pixels(rgba) {
            let input = *px;
            let pixel = input.map(f32::abs);
            for c in 0..4 {
                px[c] = apply_sign(BASIC_MIRROR_SIGN[c], input[c], pixel[c].powf(gamma[c]));
            }
        }
    }
}

/// [`GammaBasicMirrorOpCpu`] with `ssePower`: the sign bit is OR-ed back onto the result.
///
/// Port of `GammaBasicMirrorOpCPUSSE` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:66-77, 369-392
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaBasicMirrorOpCpuSse(GammaBasicOpCpu);

impl GammaBasicMirrorOpCpuSse {
    /// Port of `GammaBasicMirrorOpCPUSSE::GammaBasicMirrorOpCPUSSE`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:70-73 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaBasicMirrorOpCpuSse(GammaBasicOpCpu::new(gamma))
    }
}

impl CpuOp for GammaBasicMirrorOpCpuSse {
    /// Port of `GammaBasicMirrorOpCPUSSE::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:370-391
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let gamma = self.0.gamma;
        for px in pixels(rgba) {
            for c in 0..4 {
                let sign_pix = px[c].to_bits() & ESIGN_MASK;
                let abs_pix = f32::from_bits(px[c].to_bits() & EABS_MASK);
                let pixel = sse_power(abs_pix, gamma[c]);
                px[c] = f32::from_bits(sign_pix | pixel.to_bits());
            }
        }
    }
}

/// Basic style, negatives unchanged: `in > 0 ? pow(in, gamma) : in` with `powf`.
///
/// Port of `GammaBasicPassThruOpCPU` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:79-87, 416-467
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaBasicPassThruOpCpu(GammaBasicOpCpu);

impl GammaBasicPassThruOpCpu {
    /// Port of `GammaBasicPassThruOpCPU::GammaBasicPassThruOpCPU`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:416-419 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaBasicPassThruOpCpu(GammaBasicOpCpu::new(gamma))
    }
}

impl CpuOp for GammaBasicPassThruOpCpu {
    /// Port of `GammaBasicPassThruOpCPU::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:450-467
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let gamma = self.0.gamma;
        for px in pixels(rgba) {
            let pixel = *px;
            for c in 0..4 {
                px[c] = if pixel[c] > 0.0f32 {
                    pixel[c].powf(gamma[c])
                } else {
                    pixel[c]
                };
            }
        }
    }
}

/// [`GammaBasicPassThruOpCpu`] with `ssePower`, selected by the mask `in > 0`.
///
/// Port of `GammaBasicPassThruOpCPUSSE` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:89-100,
/// 421-448 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaBasicPassThruOpCpuSse(GammaBasicOpCpu);

impl GammaBasicPassThruOpCpuSse {
    /// Port of `GammaBasicPassThruOpCPUSSE::GammaBasicPassThruOpCPUSSE`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:93-96 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaBasicPassThruOpCpuSse(GammaBasicOpCpu::new(gamma))
    }
}

impl CpuOp for GammaBasicPassThruOpCpuSse {
    /// Port of `GammaBasicPassThruOpCPUSSE::apply`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:422-447 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let gamma = self.0.gamma;
        for px in pixels(rgba) {
            for c in 0..4 {
                let pixel = px[c];
                let data = sse_power(pixel, gamma[c]);
                // breakPnt is 0: flag = _mm_cmpgt_ps(pixel, breakPnt).
                px[c] = if pixel > 0.0f32 { data } else { pixel };
            }
        }
    }
}

// -------------------------------------------------------------------------------------------
// Moncurve styles.

/// The four channels' moncurve coefficients (`m_red`, `m_green`, `m_blue`, `m_alpha`).
fn moncurve_params(
    gamma: &GammaOpData,
    compute: fn(&super::gamma_op_data::Params) -> RendererParams,
) -> [RendererParams; 4] {
    [
        compute(gamma.red_params()),
        compute(gamma.green_params()),
        compute(gamma.blue_params()),
        compute(gamma.alpha_params()),
    ]
}

/// Moncurve forward: `in <= breakPnt ? in * slope : pow(in * scale + offset, gamma)` with
/// `powf`.
///
/// Port of `GammaMoncurveOpCPUFwd` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:102-123, 469-556
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveOpCpuFwd {
    params: [RendererParams; 4],
}

impl GammaMoncurveOpCpuFwd {
    /// Port of `GammaMoncurveOpCPUFwd::GammaMoncurveOpCPUFwd` and `update`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:469-481 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveOpCpuFwd {
            params: moncurve_params(gamma, compute_params_fwd),
        }
    }
}

impl CpuOp for GammaMoncurveOpCpuFwd {
    /// Port of `GammaMoncurveOpCPUFwd::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:525-556
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            let pixel = *px;
            let data: [f32; 4] = [0, 1, 2, 3].map(|c| {
                let p = &self.params[c];
                sse_add(sse_mul(pixel[c], p.scale), p.offset).powf(p.gamma)
            });
            for c in 0..4 {
                let p = &self.params[c];
                px[c] = if pixel[c] <= p.break_pnt {
                    sse_mul(pixel[c], p.slope)
                } else {
                    data[c]
                };
            }
        }
    }
}

/// [`GammaMoncurveOpCpuFwd`] with `ssePower`, selected by the mask `in > breakPnt`.
///
/// Port of `GammaMoncurveOpCPUFwdSSE` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:125-136,
/// 483-523 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveOpCpuFwdSse(GammaMoncurveOpCpuFwd);

impl GammaMoncurveOpCpuFwdSse {
    /// Port of `GammaMoncurveOpCPUFwdSSE::GammaMoncurveOpCPUFwdSSE`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:129-132 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveOpCpuFwdSse(GammaMoncurveOpCpuFwd::new(gamma))
    }
}

impl CpuOp for GammaMoncurveOpCpuFwdSse {
    /// Port of `GammaMoncurveOpCPUFwdSSE::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:484-522
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (v, p) in px.iter_mut().zip(&self.0.params) {
                let pixel = *v;
                let mut data = sse_add(sse_mul(pixel, p.scale), p.offset);
                data = sse_power(data, p.gamma);
                let flag = pixel > p.break_pnt;
                *v = if flag { data } else { sse_mul(pixel, p.slope) };
            }
        }
    }
}

/// Moncurve reverse: `in <= breakPnt ? in * slope : pow(in, gamma) * scale - offset` with
/// `powf`.
///
/// Port of `GammaMoncurveOpCPURev` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:138-148, 558-645
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveOpCpuRev {
    params: [RendererParams; 4],
}

impl GammaMoncurveOpCpuRev {
    /// Port of `GammaMoncurveOpCPURev::GammaMoncurveOpCPURev` and `update`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:558-570 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveOpCpuRev {
            params: moncurve_params(gamma, compute_params_rev),
        }
    }
}

impl CpuOp for GammaMoncurveOpCpuRev {
    /// Port of `GammaMoncurveOpCPURev::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:614-645
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            let pixel = *px;
            let data: [f32; 4] = [0, 1, 2, 3].map(|c| {
                let p = &self.params[c];
                sse_mul(pixel[c].powf(p.gamma), p.scale) - p.offset
            });
            for c in 0..4 {
                let p = &self.params[c];
                px[c] = if pixel[c] <= p.break_pnt {
                    sse_mul(pixel[c], p.slope)
                } else {
                    data[c]
                };
            }
        }
    }
}

/// [`GammaMoncurveOpCpuRev`] with `ssePower`, selected by the mask `in > breakPnt`.
///
/// Port of `GammaMoncurveOpCPURevSSE` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:150-161,
/// 572-612 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveOpCpuRevSse(GammaMoncurveOpCpuRev);

impl GammaMoncurveOpCpuRevSse {
    /// Port of `GammaMoncurveOpCPURevSSE::GammaMoncurveOpCPURevSSE`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:154-157 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveOpCpuRevSse(GammaMoncurveOpCpuRev::new(gamma))
    }
}

impl CpuOp for GammaMoncurveOpCpuRevSse {
    /// Port of `GammaMoncurveOpCPURevSSE::apply` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:573-611
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (v, p) in px.iter_mut().zip(&self.0.params) {
                let pixel = *v;
                let mut data = sse_power(pixel, p.gamma);
                data = sse_mul(data, p.scale) - p.offset;
                let flag = pixel > p.break_pnt;
                *v = if flag { data } else { sse_mul(pixel, p.slope) };
            }
        }
    }
}

/// Moncurve forward, mirrored for negatives: [`GammaMoncurveOpCpuFwd`] on `|in|`, times
/// `copysign(1, in)`.
///
/// Port of `GammaMoncurveMirrorOpCPUFwd` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:163-172,
/// 647-741 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveMirrorOpCpuFwd {
    params: [RendererParams; 4],
}

impl GammaMoncurveMirrorOpCpuFwd {
    /// Port of `GammaMoncurveMirrorOpCPUFwd::GammaMoncurveMirrorOpCPUFwd` and `update`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:647-659 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveMirrorOpCpuFwd {
            params: moncurve_params(gamma, compute_params_fwd),
        }
    }
}

impl CpuOp for GammaMoncurveMirrorOpCpuFwd {
    /// Port of `GammaMoncurveMirrorOpCPUFwd::apply`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:707-741 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            let input = *px;
            let pixel = input.map(f32::abs);
            let data: [f32; 4] = [0, 1, 2, 3].map(|c| {
                let p = &self.params[c];
                sse_add(sse_mul(pixel[c], p.scale), p.offset).powf(p.gamma)
            });
            for c in 0..4 {
                let p = &self.params[c];
                let value = if pixel[c] <= p.break_pnt {
                    sse_mul(pixel[c], p.slope)
                } else {
                    data[c]
                };
                px[c] = apply_sign(MONCURVE_MIRROR_SIGN[c], input[c], value);
            }
        }
    }
}

/// [`GammaMoncurveMirrorOpCpuFwd`] with `ssePower`: the sign bit is OR-ed back onto the result.
///
/// Port of `GammaMoncurveMirrorOpCPUFwdSSE` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:174-185,
/// 661-705 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveMirrorOpCpuFwdSse(GammaMoncurveMirrorOpCpuFwd);

impl GammaMoncurveMirrorOpCpuFwdSse {
    /// Port of `GammaMoncurveMirrorOpCPUFwdSSE::GammaMoncurveMirrorOpCPUFwdSSE`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:178-181 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveMirrorOpCpuFwdSse(GammaMoncurveMirrorOpCpuFwd::new(gamma))
    }
}

impl CpuOp for GammaMoncurveMirrorOpCpuFwdSse {
    /// Port of `GammaMoncurveMirrorOpCPUFwdSSE::apply`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:662-704 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (v, p) in px.iter_mut().zip(&self.0.params) {
                let sign_pix = v.to_bits() & ESIGN_MASK;
                let abs_pix = f32::from_bits(v.to_bits() & EABS_MASK);

                let mut data = sse_add(sse_mul(abs_pix, p.scale), p.offset);
                data = sse_power(data, p.gamma);

                let flagbrk = abs_pix > p.break_pnt;
                data = if flagbrk {
                    data
                } else {
                    sse_mul(abs_pix, p.slope)
                };

                *v = f32::from_bits(sign_pix | data.to_bits());
            }
        }
    }
}

/// Moncurve reverse, mirrored for negatives: [`GammaMoncurveOpCpuRev`] on `|in|`, times
/// `copysign(1, in)`.
///
/// Port of `GammaMoncurveMirrorOpCPURev` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:187-196,
/// 743-838 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveMirrorOpCpuRev {
    params: [RendererParams; 4],
}

impl GammaMoncurveMirrorOpCpuRev {
    /// Port of `GammaMoncurveMirrorOpCPURev::GammaMoncurveMirrorOpCPURev` and `update`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:743-755 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveMirrorOpCpuRev {
            params: moncurve_params(gamma, compute_params_rev),
        }
    }
}

impl CpuOp for GammaMoncurveMirrorOpCpuRev {
    /// Port of `GammaMoncurveMirrorOpCPURev::apply`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:803-838 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            let input = *px;
            let pixel = input.map(f32::abs);
            let data: [f32; 4] = [0, 1, 2, 3].map(|c| {
                let p = &self.params[c];
                sse_mul(pixel[c].powf(p.gamma), p.scale) - p.offset
            });
            for c in 0..4 {
                let p = &self.params[c];
                let value = if pixel[c] <= p.break_pnt {
                    sse_mul(pixel[c], p.slope)
                } else {
                    data[c]
                };
                px[c] = apply_sign(MONCURVE_MIRROR_SIGN[c], input[c], value);
            }
        }
    }
}

/// [`GammaMoncurveMirrorOpCpuRev`] with `ssePower`: the sign bit is OR-ed back onto the result.
///
/// Port of `GammaMoncurveMirrorOpCPURevSSE` (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:198-209,
/// 757-801 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaMoncurveMirrorOpCpuRevSse(GammaMoncurveMirrorOpCpuRev);

impl GammaMoncurveMirrorOpCpuRevSse {
    /// Port of `GammaMoncurveMirrorOpCPURevSSE::GammaMoncurveMirrorOpCPURevSSE`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:202-205 @ v2.5.2).
    pub fn new(gamma: &GammaOpData) -> Self {
        GammaMoncurveMirrorOpCpuRevSse(GammaMoncurveMirrorOpCpuRev::new(gamma))
    }
}

impl CpuOp for GammaMoncurveMirrorOpCpuRevSse {
    /// Port of `GammaMoncurveMirrorOpCPURevSSE::apply`
    /// (src/OpenColorIO/ops/gamma/GammaOpCPU.cpp:758-800 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (v, p) in px.iter_mut().zip(&self.0.params) {
                let sign_pix = v.to_bits() & ESIGN_MASK;
                let abs_pix = f32::from_bits(v.to_bits() & EABS_MASK);

                let mut data = sse_power(abs_pix, p.gamma);
                data = sse_mul(data, p.scale) - p.offset;

                let flagbrk = abs_pix > p.break_pnt;
                data = if flagbrk {
                    data
                } else {
                    sse_mul(abs_pix, p.slope)
                };

                *v = f32::from_bits(sign_pix | data.to_bits());
            }
        }
    }
}

#[cfg(test)]
#[path = "gamma_op_cpu_tests.rs"]
mod tests;
