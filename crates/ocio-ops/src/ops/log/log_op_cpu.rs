// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log op's CPU renderers.
//!
//! Port of `src/OpenColorIO/ops/log/LogOpCPU.cpp` @ v2.5.2. Each C++ renderer class is a
//! struct here, and each `...SSE` class, which the wheel uses when `OPTIMIZATION_FAST_LOG_EXP_POW`
//! is set, is its own struct that reproduces one lane of the SSE2 kernel (`crate::sse`).
//!
//! The constructors do what the C++ `updateData` methods do, with the same `float`/`double`
//! boundaries: parameters are `double`, the renderers' coefficients `float`.
//!
//! The SSE kernels load `(in[0], in[1], in[2], 0.0f)`, compute four lanes and store them before
//! restoring the alpha, so the fourth lane is discarded; the port computes the three color
//! lanes. Alpha is passed through unchanged, bit for bit.
//!
//! # Two NaN operands
//!
//! A NaN pixel can meet a NaN coefficient, or two NaN coefficients can meet: coefficients
//! derived from a NaN break on the log side (a negative `linSideSlope * linSideBreak +
//! linSideOffset`) or from finite parameters that overflow (`inf / inf`, `inf * 0`), or NaN
//! parameters, which OCIO 2.5.2 accepts. x86 then returns the first operand's NaN, so the
//! operand order decides the result. The renderers add and multiply their coefficients with
//! [`sse_add`]/[`sse_mul`], in the order the wheels' machine code uses: upstream's source order,
//! except where the comments say otherwise. Rust's `+` and `*` would leave it to LLVM. (The
//! Log2/Log10 renderers only multiply by constants.)

use std::sync::Arc;

use super::log_op_data::{
    LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE, LogOpData,
    Params,
};
use super::log_utils::{get_linear_offset, get_linear_slope, get_log_side_break};
use crate::math_utils::{sse_add, sse_max, sse_mul, std_max};
use crate::op::CpuOp;
use crate::open_color_types::TransformDirection;
use crate::sse::{sse_exp2, sse_log2};

/// `LOG2_10` (src/OpenColorIO/ops/log/LogOpCPU.cpp:221 @ v2.5.2).
const LOG2_10: f32 = 3.3219280948873623478703194294894_f64 as f32;
/// `LOG10_2` (src/OpenColorIO/ops/log/LogOpCPU.cpp:222 @ v2.5.2).
const LOG10_2: f32 = 0.3010299956639811952137388947245_f64 as f32;

/// `std::numeric_limits<float>::min()`, the smallest positive normal float: the renderers
/// clamp the argument of the logarithm to it.
const MIN_VALUE: f32 = f32::MIN_POSITIVE;

/// The Log renderer for the op's style and direction: the SSE kernel when `fast_exp`
/// (`OPTIMIZATION_FAST_LOG_EXP_POW`) is set, the math-library one otherwise.
///
/// Port of `GetLogRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:224-315 @ v2.5.2). Its final
/// `throw Exception("Illegal Log direction.")` cannot be reached with a
/// [`TransformDirection`].
pub fn get_log_renderer(log: &LogOpData, fast_exp: bool) -> Arc<dyn CpuOp> {
    use TransformDirection::{Forward, Inverse};

    let dir = log.direction();
    if log.is_log2() {
        match (dir, fast_exp) {
            (Forward, true) => Arc::new(LogRendererSse::new(log, 1.0)),
            (Forward, false) => Arc::new(LogRenderer::new(log, 1.0)),
            (Inverse, true) => Arc::new(AntiLogRendererSse::new(log, 1.0)),
            (Inverse, false) => Arc::new(AntiLogRenderer::new(log, 1.0)),
        }
    } else if log.is_log10() {
        match (dir, fast_exp) {
            (Forward, true) => Arc::new(LogRendererSse::new(log, LOG10_2)),
            (Forward, false) => Arc::new(LogRenderer::new(log, LOG10_2)),
            (Inverse, true) => Arc::new(AntiLogRendererSse::new(log, LOG2_10)),
            (Inverse, false) => Arc::new(AntiLogRenderer::new(log, LOG2_10)),
        }
    } else if log.is_camera() {
        match (dir, fast_exp) {
            (Forward, true) => Arc::new(CameraLin2LogRendererSse::new(log)),
            (Forward, false) => Arc::new(CameraLin2LogRenderer::new(log)),
            (Inverse, true) => Arc::new(CameraLog2LinRendererSse::new(log)),
            (Inverse, false) => Arc::new(CameraLog2LinRenderer::new(log)),
        }
    } else {
        match (dir, fast_exp) {
            (Forward, true) => Arc::new(Lin2LogRendererSse::new(log)),
            (Forward, false) => Arc::new(Lin2LogRenderer::new(log)),
            (Inverse, true) => Arc::new(Log2LinRendererSse::new(log)),
            (Inverse, false) => Arc::new(Log2LinRenderer::new(log)),
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

/// The three channels' parameters (`L2LBaseRenderer::m_paramsR/G/B`).
fn channel_params(log: &LogOpData) -> [&Params; 3] {
    [log.red_params(), log.green_params(), log.blue_params()]
}

// -------------------------------------------------------------------------------------------
// Log10 and Log2.

/// `out = log2(max(in, FLT_MIN)) * logScale` on RGB, with `log2f`.
///
/// Port of `LogRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:177-187, 342-414 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LogRenderer {
    log_scale: f32,
}

impl LogRenderer {
    /// Port of `LogRenderer::LogRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:342-347
    /// @ v2.5.2).
    pub fn new(_log: &LogOpData, log_scale: f32) -> Self {
        LogRenderer { log_scale }
    }
}

impl CpuOp for LogRenderer {
    /// Port of `LogRenderer::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:391-414 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for v in &mut px[..3] {
                // ApplyMax, ApplyLog2, ApplyScale.
                *v = std_max(MIN_VALUE, *v);
                *v = v.log2();
                *v *= self.log_scale;
            }
        }
    }
}

/// [`LogRenderer`] with `sseLog2` and `_mm_max_ps`.
///
/// Port of `LogRendererSSE` (src/OpenColorIO/ops/log/LogOpCPU.cpp:189-197, 416-452 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LogRendererSse(LogRenderer);

impl LogRendererSse {
    /// Port of `LogRendererSSE::LogRendererSSE` (src/OpenColorIO/ops/log/LogOpCPU.cpp:417-420
    /// @ v2.5.2).
    pub fn new(log: &LogOpData, log_scale: f32) -> Self {
        LogRendererSse(LogRenderer::new(log, log_scale))
    }
}

impl CpuOp for LogRendererSse {
    /// Port of `LogRendererSSE::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:421-451 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for v in &mut px[..3] {
                let mut pixel = sse_max(*v, MIN_VALUE);
                pixel = sse_log2(pixel);
                *v = pixel * self.0.log_scale;
            }
        }
    }
}

// -------------------------------------------------------------------------------------------
// AntiLog10 and AntiLog2.

/// `out = exp2(in * log2(base))` on RGB, with `exp2f`.
///
/// Port of `AntiLogRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:199-209, 454-482
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct AntiLogRenderer {
    log2_base: f32,
}

impl AntiLogRenderer {
    /// Port of `AntiLogRenderer::AntiLogRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:455-460
    /// @ v2.5.2).
    pub fn new(_log: &LogOpData, log2_base: f32) -> Self {
        AntiLogRenderer { log2_base }
    }
}

impl CpuOp for AntiLogRenderer {
    /// Port of `AntiLogRenderer::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:462-482
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for v in &mut px[..3] {
                // ApplyScale, ApplyExp2.
                *v *= self.log2_base;
                *v = v.exp2();
            }
        }
    }
}

/// [`AntiLogRenderer`] with `sseExp2`.
///
/// Port of `AntiLogRendererSSE` (src/OpenColorIO/ops/log/LogOpCPU.cpp:211-219, 484-521
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct AntiLogRendererSse(AntiLogRenderer);

impl AntiLogRendererSse {
    /// Port of `AntiLogRendererSSE::AntiLogRendererSSE`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:485-488 @ v2.5.2).
    pub fn new(log: &LogOpData, log2_base: f32) -> Self {
        AntiLogRendererSse(AntiLogRenderer::new(log, log2_base))
    }
}

impl CpuOp for AntiLogRendererSse {
    /// Port of `AntiLogRendererSSE::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:490-520
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for v in &mut px[..3] {
                *v = sse_exp2(*v * self.0.log2_base);
            }
        }
    }
}

// -------------------------------------------------------------------------------------------
// LogToLin.

/// `out = (exp2((in - logOffset) * log2(base) / logSlope) - linOffset) / linSlope` on RGB,
/// with `exp2f`.
///
/// Port of `Log2LinRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:52-67, 523-572 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Log2LinRenderer {
    kinv: [f32; 3],
    minuskb: [f32; 3],
    minusb: [f32; 3],
    minv: [f32; 3],
}

impl Log2LinRenderer {
    /// Port of `Log2LinRenderer::Log2LinRenderer` and `Log2LinRenderer::updateData`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:524-546 @ v2.5.2), with
    /// `L2LBaseRenderer::updateData` (:332-340).
    pub fn new(log: &LogOpData) -> Self {
        let base = log.base() as f32;
        let params = channel_params(log);
        Log2LinRenderer {
            kinv: params.map(|p| base.log2() / p[LOG_SIDE_SLOPE] as f32),
            minuskb: params.map(|p| -(p[LOG_SIDE_OFFSET] as f32)),
            minusb: params.map(|p| -(p[LIN_SIDE_OFFSET] as f32)),
            minv: params.map(|p| 1.0f32 / p[LIN_SIDE_SLOPE] as f32),
        }
    }
}

impl CpuOp for Log2LinRenderer {
    /// Port of `Log2LinRenderer::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:549-572
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (c, v) in px[..3].iter_mut().enumerate() {
                // ApplyAdd, ApplyScale, ApplyExp2, ApplyAdd, ApplyScale: `pix[i] = pix[i] + add[i]`
                // and `pix[i] = pix[i] * scale[i]`.
                *v = sse_add(*v, self.minuskb[c]);
                *v = sse_mul(*v, self.kinv[c]);
                *v = v.exp2();
                *v = sse_add(*v, self.minusb[c]);
                *v = sse_mul(*v, self.minv[c]);
            }
        }
    }
}

/// [`Log2LinRenderer`] with `sseExp2`.
///
/// Port of `Log2LinRendererSSE` (src/OpenColorIO/ops/log/LogOpCPU.cpp:69-77, 574-621
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Log2LinRendererSse(Log2LinRenderer);

impl Log2LinRendererSse {
    /// Port of `Log2LinRendererSSE::Log2LinRendererSSE`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:575-579 @ v2.5.2).
    pub fn new(log: &LogOpData) -> Self {
        Log2LinRendererSse(Log2LinRenderer::new(log))
    }
}

impl CpuOp for Log2LinRendererSse {
    /// Port of `Log2LinRendererSSE::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:581-620
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let r = &self.0;
        for px in pixels(rgba) {
            for (c, v) in px[..3].iter_mut().enumerate() {
                let mut pixel = sse_add(*v, r.minuskb[c]);
                pixel = sse_mul(pixel, r.kinv[c]);
                pixel = sse_exp2(pixel);
                pixel = sse_add(pixel, r.minusb[c]);
                *v = sse_mul(pixel, r.minv[c]);
            }
        }
    }
}

// -------------------------------------------------------------------------------------------
// LinToLog.

/// `out = log2(max(in * linSlope + linOffset, FLT_MIN)) * logSlope / log2(base) + logOffset`
/// on RGB, with `log2f`.
///
/// Port of `Lin2LogRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:79-94, 623-674 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Lin2LogRenderer {
    m: [f32; 3],
    b: [f32; 3],
    klog: [f32; 3],
    kb: [f32; 3],
}

impl Lin2LogRenderer {
    /// Port of `Lin2LogRenderer::Lin2LogRenderer` and `Lin2LogRenderer::updateData`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:624-646 @ v2.5.2), with
    /// `L2LBaseRenderer::updateData` (:332-340). `log2(m_base)` takes a `float`, and
    /// `LogOpCPU.cpp` sees the `float` overload on both platforms: it is `log2f`, promoted to
    /// `double` for the division.
    pub fn new(log: &LogOpData) -> Self {
        let base = log.base() as f32;
        let params = channel_params(log);
        Lin2LogRenderer {
            m: params.map(|p| p[LIN_SIDE_SLOPE] as f32),
            b: params.map(|p| p[LIN_SIDE_OFFSET] as f32),
            klog: params.map(|p| (p[LOG_SIDE_SLOPE] / f64::from(base.log2())) as f32),
            kb: params.map(|p| p[LOG_SIDE_OFFSET] as f32),
        }
    }
}

impl CpuOp for Lin2LogRenderer {
    /// Port of `Lin2LogRenderer::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:648-674
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (c, v) in px[..3].iter_mut().enumerate() {
                // ApplyScale, ApplyAdd, ApplyMax, ApplyLog2, ApplyScale, ApplyAdd.
                *v = sse_mul(*v, self.m[c]);
                *v = sse_add(*v, self.b[c]);
                *v = std_max(MIN_VALUE, *v);
                *v = v.log2();
                *v = sse_mul(*v, self.klog[c]);
                *v = sse_add(*v, self.kb[c]);
            }
        }
    }
}

/// [`Lin2LogRenderer`] with `sseLog2` and `_mm_max_ps`.
///
/// Port of `Lin2LogRendererSSE` (src/OpenColorIO/ops/log/LogOpCPU.cpp:96-104, 676-721
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Lin2LogRendererSse(Lin2LogRenderer);

impl Lin2LogRendererSse {
    /// Port of `Lin2LogRendererSSE::Lin2LogRendererSSE`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:677-680 @ v2.5.2).
    pub fn new(log: &LogOpData) -> Self {
        Lin2LogRendererSse(Lin2LogRenderer::new(log))
    }
}

impl CpuOp for Lin2LogRendererSse {
    /// Port of `Lin2LogRendererSSE::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:682-720
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let r = &self.0;
        for px in pixels(rgba) {
            for (c, v) in px[..3].iter_mut().enumerate() {
                let mut pixel = sse_mul(*v, r.m[c]);
                pixel = sse_add(pixel, r.b[c]);
                pixel = sse_max(pixel, MIN_VALUE);
                pixel = sse_log2(pixel);
                pixel = sse_mul(pixel, r.klog[c]);
                *v = sse_add(pixel, r.kb[c]);
            }
        }
    }
}

// -------------------------------------------------------------------------------------------
// Camera styles.

/// What the camera renderers share: the linear segment and the break on the log side.
///
/// Port of `CameraL2LBaseRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:106-118, 723-742
/// @ v2.5.2).
#[derive(Debug, Clone)]
struct CameraL2LBase {
    log_side_break: [f32; 3],
    linear_slope: [f32; 3],
    linear_offset: [f32; 3],
    log2_base: f32,
}

impl CameraL2LBase {
    /// Port of `CameraL2LBaseRenderer::updateData`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:728-742 @ v2.5.2). The helpers take the `float`
    /// base promoted to `double`; `log2((float)m_base)` is `log2f` in `LogOpCPU.cpp`.
    fn new(log: &LogOpData) -> Self {
        let base = log.base() as f32;
        let params = channel_params(log);
        let linear_slope = params.map(|p| get_linear_slope(p, f64::from(base)));
        let log_side_break = params.map(|p| get_log_side_break(p, f64::from(base)));
        let linear_offset =
            [0, 1, 2].map(|c| get_linear_offset(params[c], linear_slope[c], log_side_break[c]));
        CameraL2LBase {
            log_side_break,
            linear_slope,
            linear_offset,
            log2_base: base.log2(),
        }
    }
}

/// CameraLogToLin: below the log-side break, `(in - linearOffset) / linearSlope`; above it,
/// [`Log2LinRenderer`]'s curve. With `exp2f`.
///
/// Port of `CameraLog2LinRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:120-137, 744-802
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CameraLog2LinRenderer {
    base: CameraL2LBase,
    kinv: [f32; 3],
    minuskb: [f32; 3],
    minusb: [f32; 3],
    minv: [f32; 3],
    linsinv: [f32; 3],
    minuslino: [f32; 3],
}

impl CameraLog2LinRenderer {
    /// Port of `CameraLog2LinRenderer::CameraLog2LinRenderer` and `updateData`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:744-772 @ v2.5.2).
    pub fn new(log: &LogOpData) -> Self {
        let base = CameraL2LBase::new(log);
        let params = channel_params(log);
        CameraLog2LinRenderer {
            kinv: params.map(|p| base.log2_base / p[LOG_SIDE_SLOPE] as f32),
            minuskb: params.map(|p| -(p[LOG_SIDE_OFFSET] as f32)),
            minusb: params.map(|p| -(p[LIN_SIDE_OFFSET] as f32)),
            minv: params.map(|p| 1.0f32 / p[LIN_SIDE_SLOPE] as f32),
            linsinv: base.linear_slope.map(|s| 1.0f32 / s),
            minuslino: base.linear_offset.map(|o| -o),
            base,
        }
    }
}

impl CpuOp for CameraLog2LinRenderer {
    /// Port of `CameraLog2LinRenderer::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:774-802
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (i, v) in px[..3].iter_mut().enumerate() {
                let x = *v;
                *v = if x < self.base.log_side_break[i] {
                    // `m_linsinv[i] * (in[i] + m_minuslino[i])`, which both wheels compile as
                    // `(in[i] + m_minuslino[i]) * m_linsinv[i]` (Windows at 0x180210d18,
                    // Linux at 0x3f26c0). Finite parameters that overflow can make both
                    // coefficients NaN, with different signs.
                    sse_mul(sse_add(x, self.minuslino[i]), self.linsinv[i])
                } else {
                    let mut out = sse_mul(sse_add(x, self.minuskb[i]), self.kinv[i]);
                    out = out.exp2();
                    sse_mul(sse_add(out, self.minusb[i]), self.minv[i])
                };
            }
        }
    }
}

/// [`CameraLog2LinRenderer`] with `sseExp2`. The branch is a mask of `in > logSideBreak`, so
/// the break itself and NaN take the linear segment.
///
/// Port of `CameraLog2LinRendererSSE` (src/OpenColorIO/ops/log/LogOpCPU.cpp:139-147, 804-860
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CameraLog2LinRendererSse(CameraLog2LinRenderer);

impl CameraLog2LinRendererSse {
    /// Port of `CameraLog2LinRendererSSE::CameraLog2LinRendererSSE`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:805-808 @ v2.5.2).
    pub fn new(log: &LogOpData) -> Self {
        CameraLog2LinRendererSse(CameraLog2LinRenderer::new(log))
    }
}

impl CpuOp for CameraLog2LinRendererSse {
    /// Port of `CameraLog2LinRendererSSE::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:810-859
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let r = &self.0;
        for px in pixels(rgba) {
            for (c, v) in px[..3].iter_mut().enumerate() {
                let pixel = *v;
                let flag = pixel > r.base.log_side_break[c];

                let mut pixel_lin = sse_add(pixel, r.minuslino[c]);
                pixel_lin = sse_mul(pixel_lin, r.linsinv[c]);

                let mut pixel_log = sse_add(pixel, r.minuskb[c]);
                pixel_log = sse_mul(pixel_log, r.kinv[c]);
                pixel_log = sse_exp2(pixel_log);
                pixel_log = sse_add(pixel_log, r.minusb[c]);
                pixel_log = sse_mul(pixel_log, r.minv[c]);

                *v = if flag { pixel_log } else { pixel_lin };
            }
        }
    }
}

/// CameraLinToLog: below `linSideBreak`, `linearSlope * in + linearOffset`; above it,
/// [`Lin2LogRenderer`]'s curve. With `log2f`.
///
/// Port of `CameraLin2LogRenderer` (src/OpenColorIO/ops/log/LogOpCPU.cpp:149-165, 862-920
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CameraLin2LogRenderer {
    base: CameraL2LBase,
    m: [f32; 3],
    b: [f32; 3],
    klog: [f32; 3],
    kb: [f32; 3],
    linb: [f32; 3],
}

impl CameraLin2LogRenderer {
    /// Port of `CameraLin2LogRenderer::CameraLin2LogRenderer` and `updateData`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:862-887 @ v2.5.2).
    pub fn new(log: &LogOpData) -> Self {
        let base = CameraL2LBase::new(log);
        let params = channel_params(log);
        CameraLin2LogRenderer {
            m: params.map(|p| p[LIN_SIDE_SLOPE] as f32),
            b: params.map(|p| p[LIN_SIDE_OFFSET] as f32),
            klog: params.map(|p| (p[LOG_SIDE_SLOPE] / f64::from(base.log2_base)) as f32),
            kb: params.map(|p| p[LOG_SIDE_OFFSET] as f32),
            linb: params.map(|p| p[LIN_SIDE_BREAK] as f32),
            base,
        }
    }
}

impl CpuOp for CameraLin2LogRenderer {
    /// Port of `CameraLin2LogRenderer::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:889-920
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for px in pixels(rgba) {
            for (i, v) in px[..3].iter_mut().enumerate() {
                let x = *v;
                *v = if x < self.linb[i] {
                    // m_linearSlope[i] * in[i] + m_linearOffset[i]
                    sse_add(
                        sse_mul(self.base.linear_slope[i], x),
                        self.base.linear_offset[i],
                    )
                } else {
                    let mut out = sse_add(sse_mul(x, self.m[i]), self.b[i]);
                    out = std_max(MIN_VALUE, out);
                    out = out.log2();
                    sse_add(sse_mul(out, self.klog[i]), self.kb[i])
                };
            }
        }
    }
}

/// [`CameraLin2LogRenderer`] with `sseLog2` and `_mm_max_ps`. The branch is a mask of
/// `in > linSideBreak`, so the break itself and NaN take the linear segment.
///
/// Port of `CameraLin2LogRendererSSE` (src/OpenColorIO/ops/log/LogOpCPU.cpp:167-175, 922-982
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CameraLin2LogRendererSse(CameraLin2LogRenderer);

impl CameraLin2LogRendererSse {
    /// Port of `CameraLin2LogRendererSSE::CameraLin2LogRendererSSE`
    /// (src/OpenColorIO/ops/log/LogOpCPU.cpp:923-926 @ v2.5.2).
    pub fn new(log: &LogOpData) -> Self {
        CameraLin2LogRendererSse(CameraLin2LogRenderer::new(log))
    }
}

impl CpuOp for CameraLin2LogRendererSse {
    /// Port of `CameraLin2LogRendererSSE::apply` (src/OpenColorIO/ops/log/LogOpCPU.cpp:928-981
    /// @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let r = &self.0;
        for px in pixels(rgba) {
            for (c, v) in px[..3].iter_mut().enumerate() {
                let pixel = *v;
                let flag = pixel > r.linb[c];

                let mut pixel_lin = sse_mul(pixel, r.base.linear_slope[c]);
                pixel_lin = sse_add(pixel_lin, r.base.linear_offset[c]);

                let mut pixel_log = sse_mul(pixel, r.m[c]);
                pixel_log = sse_add(pixel_log, r.b[c]);
                pixel_log = sse_max(pixel_log, MIN_VALUE);
                pixel_log = sse_log2(pixel_log);
                pixel_log = sse_mul(pixel_log, r.klog[c]);
                pixel_log = sse_add(pixel_log, r.kb[c]);

                *v = if flag { pixel_log } else { pixel_lin };
            }
        }
    }
}

#[cfg(test)]
#[path = "log_op_cpu_tests.rs"]
mod tests;
