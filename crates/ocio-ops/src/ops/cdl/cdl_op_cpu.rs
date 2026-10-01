// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL op's CPU renderers: a port of `src/OpenColorIO/ops/cdl/CDLOpCPU.h` and
//! `CDLOpCPU.cpp` @ v2.5.2.
//!
//! [`get_cdl_cpu_renderer`] picks one by the style and `OPTIMIZATION_FAST_LOG_EXP_POW`: the
//! forward renderers apply slope, offset, power, saturation; the reverse ones the inverse steps
//! in the reverse order with the reciprocal parameters ([`RenderParams::update`]). The clamping
//! styles clamp to [0, 1] before the power and at the end; the others pass negative values
//! through the power. The fast-math renderers (`...SSE`) are upstream's SSE2 kernels, which
//! every x86-64 CPU runs (no CPU dispatch): each processes a pixel as one four-lane vector,
//! alpha included, and the port computes the four lanes, since alpha's lane enters the luma of
//! the saturation (its weight is 0, so an infinite alpha makes the luma NaN). No renderer
//! writes alpha: upstream writes back the input's alpha (`out[3] = inAlpha`).
//!
//! # Operand orders
//!
//! Read from both wheels' machine code (`tools/wheel-inspect`, `CDLRenderer*::apply`): MSVC
//! (Windows) and GCC (Linux) chose different operand orders in places, but only where at most
//! one operand can be a NaN of its own or where the NaNs are all x86's default NaN, so no order
//! shows in the output:
//! - every renderer turns a NaN into 0 before the saturation (the clamp, `ApplyPower<false>`'s
//!   `IsNan` test, `ssePower`'s mask) or after it (the reverse renderers' power step), so the
//!   pixels' own NaNs never reach the output, and a NaN offset (the one parameter that can be
//!   NaN) meets non-NaN values only after it;
//! - the slope, the reciprocals, the saturation and the luma weights are never NaN, so their
//!   products return the other operand's NaN, if any.
//!
//! The port keeps the source's order there, except for the fast-math luma, which sums the four
//! lanes with two shuffles: GCC adds `luma + shuffled` as the source does, MSVC
//! `shuffled + luma` (`CDLRendererFwdSSE<false>::apply` at 0x1801763ce and 0x1801763dc;
//! Linux 0x33081d and 0x330827). The port follows each wheel there ([`luma_sse`]).

use std::sync::Arc;

use super::cdl_op_data::{CdlOpData, CdlOpStyle};
use crate::math_utils::{clamp, is_nan, sse_add, sse_max, sse_min, sse_mul, std_max};
use crate::op::CpuOp;
use crate::sse::{EONE, EZERO, sse_power, sse_select};

/// `RcpMinValue` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:18 @ v2.5.2).
const RCP_MIN_VALUE: f32 = 1e-2;

/// `1 / max(x, 0.01)`.
///
/// Port of `Reciprocal` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:20-23 @ v2.5.2).
#[inline]
fn reciprocal(x: f32) -> f32 {
    1.0f32 / std_max(x, RCP_MIN_VALUE)
}

/// The renderers' parameters, as `float`, with the alpha lane of the fast-math kernels: slope
/// 1, offset 0, power 1.
///
/// Port of `RenderParams` (src/OpenColorIO/ops/cdl/CDLOpCPU.h:19-54, CDLOpCPU.cpp:25-99 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderParams {
    /// `m_slope`.
    slope: [f32; 4],
    /// `m_offset`.
    offset: [f32; 4],
    /// `m_power`.
    power: [f32; 4],
    /// `m_saturation`.
    saturation: f32,
    /// `m_isReverse`.
    is_reverse: bool,
    /// `m_isNoClamp`.
    is_no_clamp: bool,
}

impl Default for RenderParams {
    /// Port of `RenderParams::RenderParams` (CDLOpCPU.cpp:25-31 @ v2.5.2).
    fn default() -> Self {
        let mut p = RenderParams {
            slope: [0.0; 4],
            offset: [0.0; 4],
            power: [0.0; 4],
            saturation: 0.0,
            is_reverse: false,
            is_no_clamp: false,
        };
        p.set_slope(1.0, 1.0, 1.0);
        p.set_offset(0.0, 0.0, 0.0);
        p.set_power(1.0, 1.0, 1.0);
        p.set_saturation(1.0);
        p
    }
}

impl RenderParams {
    /// The parameters of `cdl`'s renderer.
    pub fn new(cdl: &CdlOpData) -> Self {
        let mut p = RenderParams::default();
        p.update(cdl);
        p
    }

    /// Port of `RenderParams::getSlope` (CDLOpCPU.h:24 @ v2.5.2).
    pub fn get_slope(&self) -> &[f32; 4] {
        &self.slope
    }

    /// Port of `RenderParams::getOffset` (CDLOpCPU.h:26 @ v2.5.2).
    pub fn get_offset(&self) -> &[f32; 4] {
        &self.offset
    }

    /// Port of `RenderParams::getPower` (CDLOpCPU.h:28 @ v2.5.2).
    pub fn get_power(&self) -> &[f32; 4] {
        &self.power
    }

    /// Port of `RenderParams::getSaturation` (CDLOpCPU.h:30 @ v2.5.2).
    pub fn get_saturation(&self) -> f32 {
        self.saturation
    }

    /// Port of `RenderParams::isReverse` (CDLOpCPU.h:32 @ v2.5.2).
    pub fn is_reverse(&self) -> bool {
        self.is_reverse
    }

    /// Port of `RenderParams::isNoClamp` (CDLOpCPU.h:34 @ v2.5.2).
    pub fn is_no_clamp(&self) -> bool {
        self.is_no_clamp
    }

    /// Port of `RenderParams::setSlope` (CDLOpCPU.cpp:33-39 @ v2.5.2).
    fn set_slope(&mut self, r: f32, g: f32, b: f32) {
        self.slope = [r, g, b, 1.0];
    }

    /// Port of `RenderParams::setOffset` (CDLOpCPU.cpp:41-47 @ v2.5.2).
    fn set_offset(&mut self, r: f32, g: f32, b: f32) {
        self.offset = [r, g, b, 0.0];
    }

    /// Port of `RenderParams::setPower` (CDLOpCPU.cpp:49-55 @ v2.5.2).
    fn set_power(&mut self, r: f32, g: f32, b: f32) {
        self.power = [r, g, b, 1.0];
    }

    /// Port of `RenderParams::setSaturation` (CDLOpCPU.cpp:57-60 @ v2.5.2).
    fn set_saturation(&mut self, sat: f32) {
        self.saturation = sat;
    }

    /// The parameters as `float`; for a reverse style, the reciprocals of the slope, the power
    /// and the saturation (each at least 0.01 first) and the negated offset.
    ///
    /// Port of `RenderParams::update` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:62-99 @ v2.5.2).
    fn update(&mut self, cdl: &CdlOpData) {
        let slope = cdl.get_slope_params().get_rgb();
        let offset = cdl.get_offset_params().get_rgb();
        let power = cdl.get_power_params().get_rgb();

        let saturation = cdl.get_saturation() as f32;
        let style = cdl.get_style();

        self.is_reverse = style == CdlOpStyle::V1_2Rev || style == CdlOpStyle::NoClampRev;

        self.is_no_clamp = style == CdlOpStyle::NoClampFwd || style == CdlOpStyle::NoClampRev;

        if self.is_reverse() {
            // Reverse render parameters
            self.set_slope(
                reciprocal(slope[0] as f32),
                reciprocal(slope[1] as f32),
                reciprocal(slope[2] as f32),
            );

            self.set_offset(
                (-offset[0]) as f32,
                (-offset[1]) as f32,
                (-offset[2]) as f32,
            );

            self.set_power(
                reciprocal(power[0] as f32),
                reciprocal(power[1] as f32),
                reciprocal(power[2] as f32),
            );

            self.set_saturation(reciprocal(saturation));
        } else {
            // Forward render parameters
            self.set_slope(slope[0] as f32, slope[1] as f32, slope[2] as f32);
            self.set_offset(offset[0] as f32, offset[1] as f32, offset[2] as f32);
            self.set_power(power[0] as f32, power[1] as f32, power[2] as f32);
            self.set_saturation(saturation);
        }
    }
}

/// The luma weights (Rec.709): `LumaWeights` (CDLOpCPU.cpp:104, 203 @ v2.5.2), with the
/// fast-math kernels' alpha lane, 0.
const LUMA_WEIGHTS: [f32; 4] = [0.2126, 0.7152, 0.0722, 0.0];

/// The RGBA pixels of a buffer.
fn pixels(rgba: &mut [f32]) -> &mut [[f32; 4]] {
    debug_assert!(
        rgba.len().is_multiple_of(4),
        "RGBA buffers hold whole pixels"
    );
    rgba.as_chunks_mut::<4>().0
}

// -------------------------------------------------------------------------------------------
// Scalar steps (CDLOpCPU.cpp:175-258).

/// Port of `ApplySlope(float *, const float *)` (CDLOpCPU.cpp:182-188 @ v2.5.2).
#[inline]
fn apply_slope(pix: &mut [f32; 3], slope: &[f32]) {
    pix[0] = sse_mul(pix[0], slope[0]);
    pix[1] = sse_mul(pix[1], slope[1]);
    pix[2] = sse_mul(pix[2], slope[2]);
}

/// Port of `ApplyOffset(float *, const float *)` (CDLOpCPU.cpp:190-196 @ v2.5.2).
#[inline]
fn apply_offset(pix: &mut [f32; 3], offset: &[f32]) {
    pix[0] = sse_add(pix[0], offset[0]);
    pix[1] = sse_add(pix[1], offset[1]);
    pix[2] = sse_add(pix[2], offset[2]);
}

/// `luma + saturation * (pix - luma)` per channel, with `luma` the dot product of the pixel and
/// the luma weights, `(r * wr + g * wg) + b * wb`.
///
/// Port of `ApplySaturation(float *, const float)` (CDLOpCPU.cpp:198-215 @ v2.5.2).
#[inline]
fn apply_saturation(pix: &mut [f32; 3], saturation: f32) {
    let srcpix = *pix;

    // Compute luma: dot product of pixel values and the luma weights
    apply_slope(pix, &LUMA_WEIGHTS);

    // luma = x+y+z+w
    let luma = sse_add(sse_add(pix[0], pix[1]), pix[2]);

    // Apply saturation
    for c in 0..3 {
        // Both wheels: `(srcpix - luma) * saturation + luma` (module docs).
        pix[c] = sse_add(sse_mul(srcpix[c] - luma, saturation), luma);
    }
}

/// With `CLAMP`, clamps to [0, 1] (`Clamp`: a NaN becomes 0); otherwise nothing.
///
/// Port of `ApplyClamp<bool>(float *)` (CDLOpCPU.cpp:217-233 @ v2.5.2).
#[inline]
fn apply_clamp<const CLAMP: bool>(pix: &mut [f32; 3]) {
    if CLAMP {
        // NaNs become 0.
        for v in pix.iter_mut() {
            *v = clamp(*v, 0.0f32, 1.0f32);
        }
    }
}

/// With `CLAMP`, clamps to [0, 1] then `powf`; otherwise a NaN becomes 0, a negative value
/// passes through, and the others get `powf`.
///
/// Port of `ApplyPower<bool>(float *, const float *)` (CDLOpCPU.cpp:235-258 @ v2.5.2).
#[inline]
fn apply_power<const CLAMP: bool>(pix: &mut [f32; 3], power: &[f32]) {
    if CLAMP {
        apply_clamp::<true>(pix);
        for c in 0..3 {
            pix[c] = pix[c].powf(power[c]);
        }
    } else {
        // Note: Set NaNs to 0 to match the SSE path.
        for c in 0..3 {
            pix[c] = if is_nan(pix[c]) {
                0.0f32
            } else if pix[c] < 0.0f32 {
                pix[c]
            } else {
                pix[c].powf(power[c])
            };
        }
    }
}

// -------------------------------------------------------------------------------------------
// Fast-math steps, one lane per channel (CDLOpCPU.cpp:102-171).

/// Four lanes.
type Lanes = [f32; 4];

/// `[f(0), f(1), f(2), f(3)]`.
#[inline]
fn lanes(f: impl Fn(usize) -> f32) -> Lanes {
    [f(0), f(1), f(2), f(3)]
}

/// `_mm_add_ps(luma, shuffled)` of the luma's horizontal sums, in this platform's wheel's
/// operand order: the source's on Linux (GCC), the reverse on Windows (MSVC; module docs).
#[inline]
fn add_shuffled(luma: f32, shuffled: f32) -> f32 {
    if cfg!(target_os = "windows") {
        sse_add(shuffled, luma)
    } else {
        sse_add(luma, shuffled)
    }
}

/// The luma in every lane: `pix * LumaWeights`, then `[x+y, y+x, z+w, w+z]`, then
/// `[x+y+z+w, y+x+w+z, z+w+x+y, w+z+y+x]` (two shuffles and adds).
///
/// Port of the luma of `ApplySaturation(__m128 &, const __m128)` (CDLOpCPU.cpp:158-167 @
/// v2.5.2).
#[inline]
fn luma_sse(pix: &Lanes) -> Lanes {
    // Compute luma: dot product of pixel values and the luma weights
    let luma = lanes(|i| sse_mul(pix[i], LUMA_WEIGHTS[i]));

    // luma = [ x+y , y+x , z+w , w+z ]: _MM_SHUFFLE(2,3,0,1).
    let shuffled = [luma[1], luma[0], luma[3], luma[2]];
    let luma = lanes(|i| add_shuffled(luma[i], shuffled[i]));

    // luma = [ x+y+z+w , y+x+w+z , z+w+x+y , w+z+y+x ]: _MM_SHUFFLE(1,0,3,2).
    let shuffled = [luma[2], luma[3], luma[0], luma[1]];
    lanes(|i| add_shuffled(luma[i], shuffled[i]))
}

/// `luma + saturation * (pix - luma)` per lane, as both wheels compute it,
/// `(pix - luma) * saturation + luma` (GCC's `CDLRendererRevSSE<true>` adds `luma + ...`, where
/// the values are clamped numbers; module docs).
///
/// Port of `ApplySaturation(__m128 &, const __m128)` (CDLOpCPU.cpp:157-171 @ v2.5.2).
#[inline]
fn apply_saturation_sse(pix: &mut Lanes, saturation: f32) {
    let luma = luma_sse(pix);

    // Apply saturation
    *pix = lanes(|i| sse_add(sse_mul(pix[i] - luma[i], saturation), luma[i]));
}

/// With `CLAMP`, `_mm_min_ps(_mm_max_ps(pix, EZERO), EONE)` (a NaN becomes 0); otherwise
/// nothing.
///
/// Port of `ApplyClamp<bool>(__m128 &)` (CDLOpCPU.cpp:120-133 @ v2.5.2).
#[inline]
fn apply_clamp_sse<const CLAMP: bool>(pix: &mut Lanes) {
    if CLAMP {
        *pix = lanes(|i| sse_min(sse_max(pix[i], EZERO), EONE));
    }
}

/// With `CLAMP`, clamps then `ssePower`; otherwise `ssePower` except for negative lanes, which
/// pass through (`ssePower` gives 0 for a NaN).
///
/// Port of `ApplyPower<bool>(__m128 &, const __m128 &)` (CDLOpCPU.cpp:135-155 @ v2.5.2).
#[inline]
fn apply_power_sse<const CLAMP: bool>(pix: &mut Lanes, power: &Lanes) {
    if CLAMP {
        apply_clamp_sse::<true>(pix);
        *pix = lanes(|i| sse_power(pix[i], power[i]));
    } else {
        *pix = lanes(|i| {
            // negMask = _mm_cmplt_ps(pix, EZERO)
            let neg_mask = if pix[i] < EZERO { u32::MAX } else { 0 };
            let pix_power = sse_power(pix[i], power[i]);
            sse_select(neg_mask, pix[i], pix_power)
        });
    }
}

// -------------------------------------------------------------------------------------------
// Renderers.

/// Forward: slope, offset, power (clamped first with `CLAMP`), saturation, then a clamp with
/// `CLAMP`, with `powf`.
///
/// Port of `CDLRendererFwd<bool>` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:274-284, 378-407 @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct CdlRendererFwd<const CLAMP: bool> {
    /// `m_renderParams`.
    params: RenderParams,
}

impl<const CLAMP: bool> CdlRendererFwd<CLAMP> {
    /// Port of `CDLRendererFwd::CDLRendererFwd` and `CDLOpCPU::CDLOpCPU`
    /// (CDLOpCPU.cpp:278-281, 326-330 @ v2.5.2).
    pub fn new(cdl: &CdlOpData) -> Self {
        CdlRendererFwd {
            params: RenderParams::new(cdl),
        }
    }
}

impl<const CLAMP: bool> CpuOp for CdlRendererFwd<CLAMP> {
    /// Port of `CDLRendererFwd<CLAMP>::apply` (CDLOpCPU.cpp:378-407 @ v2.5.2). Alpha is not
    /// written: upstream writes back the input's.
    fn apply(&self, rgba: &mut [f32]) {
        let p = &self.params;
        for px in pixels(rgba) {
            let mut out = [px[0], px[1], px[2]];

            apply_slope(&mut out, p.get_slope());
            apply_offset(&mut out, p.get_offset());

            apply_power::<CLAMP>(&mut out, p.get_power());

            apply_saturation(&mut out, p.get_saturation());
            apply_clamp::<CLAMP>(&mut out);

            px[..3].copy_from_slice(&out);
        }
    }
}

/// [`CdlRendererFwd`] with `ssePower`, one four-lane vector per pixel.
///
/// Port of `CDLRendererFwdSSE<bool>` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:286-298, 346-376 @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct CdlRendererFwdSse<const CLAMP: bool> {
    /// `m_renderParams`.
    params: RenderParams,
}

impl<const CLAMP: bool> CdlRendererFwdSse<CLAMP> {
    /// Port of `CDLRendererFwdSSE::CDLRendererFwdSSE` (CDLOpCPU.cpp:291-294 @ v2.5.2).
    pub fn new(cdl: &CdlOpData) -> Self {
        CdlRendererFwdSse {
            params: RenderParams::new(cdl),
        }
    }
}

impl<const CLAMP: bool> CpuOp for CdlRendererFwdSse<CLAMP> {
    /// Port of `CDLRendererFwdSSE<CLAMP>::apply` (CDLOpCPU.cpp:346-376 @ v2.5.2): the four
    /// lanes are computed, and alpha's is not stored (`StorePixel` writes back the input's).
    fn apply(&self, rgba: &mut [f32]) {
        let p = &self.params;
        let (slope, offset, power) = (p.get_slope(), p.get_offset(), p.get_power());
        let saturation = p.get_saturation();
        for px in pixels(rgba) {
            let mut pix: Lanes = *px;

            pix = lanes(|i| sse_mul(pix[i], slope[i]));
            pix = lanes(|i| sse_add(pix[i], offset[i]));

            apply_power_sse::<CLAMP>(&mut pix, power);

            apply_saturation_sse(&mut pix, saturation);
            apply_clamp_sse::<CLAMP>(&mut pix);

            px[..3].copy_from_slice(&pix[..3]);
        }
    }
}

/// Reverse: a clamp with `CLAMP`, saturation, power (clamped first with `CLAMP`), offset,
/// slope, then a clamp with `CLAMP`, with the reverse parameters and `powf`.
///
/// Port of `CDLRendererRev<bool>` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:300-310, 442-469 @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct CdlRendererRev<const CLAMP: bool> {
    /// `m_renderParams`.
    params: RenderParams,
}

impl<const CLAMP: bool> CdlRendererRev<CLAMP> {
    /// Port of `CDLRendererRev::CDLRendererRev` and `CDLOpCPU::CDLOpCPU`
    /// (CDLOpCPU.cpp:304-307, 326-330 @ v2.5.2).
    pub fn new(cdl: &CdlOpData) -> Self {
        CdlRendererRev {
            params: RenderParams::new(cdl),
        }
    }
}

impl<const CLAMP: bool> CpuOp for CdlRendererRev<CLAMP> {
    /// Port of `CDLRendererRev<CLAMP>::apply` (CDLOpCPU.cpp:442-469 @ v2.5.2). Alpha is not
    /// written: upstream writes back the input's.
    fn apply(&self, rgba: &mut [f32]) {
        let p = &self.params;
        for px in pixels(rgba) {
            let mut out = [px[0], px[1], px[2]];

            apply_clamp::<CLAMP>(&mut out);
            apply_saturation(&mut out, p.get_saturation());

            apply_power::<CLAMP>(&mut out, p.get_power());

            apply_offset(&mut out, p.get_offset());
            apply_slope(&mut out, p.get_slope());
            apply_clamp::<CLAMP>(&mut out);

            px[..3].copy_from_slice(&out);
        }
    }
}

/// [`CdlRendererRev`] with `ssePower`, one four-lane vector per pixel.
///
/// Port of `CDLRendererRevSSE<bool>` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:312-324, 409-440 @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct CdlRendererRevSse<const CLAMP: bool> {
    /// `m_renderParams`.
    params: RenderParams,
}

impl<const CLAMP: bool> CdlRendererRevSse<CLAMP> {
    /// Port of `CDLRendererRevSSE::CDLRendererRevSSE` (CDLOpCPU.cpp:317-320 @ v2.5.2).
    pub fn new(cdl: &CdlOpData) -> Self {
        CdlRendererRevSse {
            params: RenderParams::new(cdl),
        }
    }
}

impl<const CLAMP: bool> CpuOp for CdlRendererRevSse<CLAMP> {
    /// Port of `CDLRendererRevSSE<CLAMP>::apply` (CDLOpCPU.cpp:409-440 @ v2.5.2): the four
    /// lanes are computed, and alpha's is not stored (`StorePixel` writes back the input's).
    fn apply(&self, rgba: &mut [f32]) {
        let p = &self.params;
        let (slope, offset, power) = (p.get_slope(), p.get_offset(), p.get_power());
        let saturation = p.get_saturation();
        for px in pixels(rgba) {
            let mut pix: Lanes = *px;

            apply_clamp_sse::<CLAMP>(&mut pix);
            apply_saturation_sse(&mut pix, saturation);

            apply_power_sse::<CLAMP>(&mut pix, power);

            pix = lanes(|i| sse_add(pix[i], offset[i]));
            pix = lanes(|i| sse_mul(pix[i], slope[i]));
            apply_clamp_sse::<CLAMP>(&mut pix);

            px[..3].copy_from_slice(&pix[..3]);
        }
    }
}

/// The renderer for the style: the fast-math one when `fast_power`
/// (`OPTIMIZATION_FAST_LOG_EXP_POW`). Upstream notes that with a power of 1 the optimizer
/// replaces the CDL op with matrices and clamps, so these mostly render a power other than 1.
/// (Its final `throw Exception("Unknown CDL style")` can't be reached with a [`CdlOpStyle`].)
///
/// Port of `GetCDLCPURenderer` (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:471-508 @ v2.5.2).
pub fn get_cdl_cpu_renderer(cdl: &CdlOpData, fast_power: bool) -> Arc<dyn CpuOp> {
    match (cdl.get_style(), fast_power) {
        (CdlOpStyle::V1_2Fwd, true) => Arc::new(CdlRendererFwdSse::<true>::new(cdl)),
        (CdlOpStyle::V1_2Fwd, false) => Arc::new(CdlRendererFwd::<true>::new(cdl)),
        (CdlOpStyle::NoClampFwd, true) => Arc::new(CdlRendererFwdSse::<false>::new(cdl)),
        (CdlOpStyle::NoClampFwd, false) => Arc::new(CdlRendererFwd::<false>::new(cdl)),
        (CdlOpStyle::V1_2Rev, true) => Arc::new(CdlRendererRevSse::<true>::new(cdl)),
        (CdlOpStyle::V1_2Rev, false) => Arc::new(CdlRendererRev::<true>::new(cdl)),
        (CdlOpStyle::NoClampRev, true) => Arc::new(CdlRendererRevSse::<false>::new(cdl)),
        (CdlOpStyle::NoClampRev, false) => Arc::new(CdlRendererRev::<false>::new(cdl)),
    }
}
