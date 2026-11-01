// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast op's CPU renderers: a port of
//! `src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpCPU.h` and
//! `ExposureContrastOpCPU.cpp` @ v2.5.2.
//!
//! [`get_exposure_contrast_cpu_renderer`] picks one renderer per style. Each reads exposure,
//! contrast and gamma from its dynamic properties at every `apply` (the op data's own, or for
//! a dynamic one a copy the CPU processor exposes), and keeps the pivot (and the log style's
//! exposure step) from when it was made.
//!
//! # The `OCIO_USE_SSE2` branches
//!
//! Every x86-64 wheel compiles the renderers' `OCIO_USE_SSE2` branches, whatever the CPU and
//! the optimization flags: where the contrast isn't 1, the linear and video styles compute
//! four lanes with `ssePower` (OCIO's approximation, [`sse_power`], not libm's `powf`), and the
//! forward log style with `_mm_mul_ps` and `_mm_add_ps`. The port computes the three color
//! lanes with the same operations ([`sse_mul`], [`sse_add`]: a NaN pixel can meet a NaN
//! coefficient, which finite parameters make too: an exposure that overflows a `float` times a
//! log exposure step of 0, a pivot that overflows one). The fourth lane is alpha, which
//! upstream saves before the store and writes back after it (`out[3] = outAlpha`): the port
//! never writes alpha.
//!
//! The operand orders are those of the wheels' machine code, as the battery and the dynamic
//! sets show them (`tests/exposure_contrast_oracle.rs`, with no NaN left under W0002): the
//! source's, but for the forward log style's `_mm_add_ps(offset, _mm_mul_ps(data, contrast))`,
//! which both wheels compute with the product first, and the scalar `in[i] * exposure` of a
//! contrast of 1, which the Windows wheel computes as `exposure * in[i]` and the Linux wheel as
//! `in[i] * exposure` (see `scale_channel`). The reverse log style, which has no SSE2 branch,
//! keeps the source's order.
//!
//! # The reverse log style's exposure step
//!
//! `ECLogarithmicRevRenderer::updateData` doesn't set the exposure step, so the reverse log
//! style uses the default 0.088 whatever the op's (docs/improvements.md, I-217). Ported as it
//! is.

//!
//! # SSE2 fast paths
//!
//! With finite coefficients, blocks of finite pixels take the real SSE2 kernels of
//! `exposure_contrast_op_cpu_sse2`, which says why no operand order shows there. Any other
//! block takes the lanes, in each wheel's operand order. `in * contrast + offset` is the
//! instructions themselves, in the wheels' order, and needs no such check.

use std::sync::Arc;

use super::exposure_contrast_op_data::{ExposureContrastOpData, ExposureContrastOpStyle, ec};
use crate::dynamic_property::{DynamicPropertyDoubleImplRcPtr, DynamicPropertyRcPtr};
use crate::exception::{Exception, Result};
use crate::math_utils::{sse_add, sse_mul, sse_sub, std_max, std_powf};
use crate::op::{CpuOp, Pixels, PixelsMut};
use crate::open_color_types::DynamicPropertyType;
use crate::sse::sse_power;

/// "Dynamic property type not supported by ExposureContrast." (ExposureContrastOpCPU.cpp:125
/// @ v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by ExposureContrast.";

/// "ExposureContrast property is not dynamic." (ExposureContrastOpCPU.cpp:129 @ v2.5.2).
const NOT_DYNAMIC: &str = "ExposureContrast property is not dynamic.";

/// What every renderer holds: the three properties, the pivot as the style uses it, and the
/// log exposure step.
///
/// Port of `ECRendererBase` (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpCPU.cpp:
/// 20-130 @ v2.5.2).
#[derive(Debug)]
struct EcRendererBase {
    /// `m_exposure`.
    exposure: DynamicPropertyDoubleImplRcPtr,
    /// `m_contrast`.
    contrast: DynamicPropertyDoubleImplRcPtr,
    /// `m_gamma`.
    gamma: DynamicPropertyDoubleImplRcPtr,
    /// `m_pivot`, 0 until the renderer's `updateData` sets it.
    pivot: f32,
    /// `m_logExposureStep`, 0.088 until the forward log renderer's `updateData` sets it.
    log_exposure_step: f32,
}

impl EcRendererBase {
    /// The op data's properties, each dynamic one decoupled from the op data by a copy.
    ///
    /// Port of `ECRendererBase::ECRendererBase` (ExposureContrastOpCPU.cpp:43-62 @ v2.5.2) and
    /// of the members' initializers (39-40).
    fn new(ec: &ExposureContrastOpData) -> Self {
        // Initialized with the instances from the processor and decouple them.
        let mut exposure = ec.get_exposure_property();
        let mut contrast = ec.get_contrast_property();
        let mut gamma = ec.get_gamma_property();
        if exposure.is_dynamic() {
            exposure = exposure.create_editable_copy();
        }
        if contrast.is_dynamic() {
            contrast = contrast.create_editable_copy();
        }
        if gamma.is_dynamic() {
            gamma = gamma.create_editable_copy();
        }
        EcRendererBase {
            exposure,
            contrast,
            gamma,
            pivot: 0.0,
            log_exposure_step: 0.088,
        }
    }

    /// `(float)std::max(EC::MIN_CONTRAST, contrast * gamma)`, in double then narrowed: a NaN
    /// product gives the minimum.
    fn contrast_val(&self) -> f32 {
        std_max(
            ec::MIN_CONTRAST,
            self.contrast.get_value() * self.gamma.get_value(),
        ) as f32
    }

    /// Port of `ECRendererBase::isDynamic` (ExposureContrastOpCPU.cpp:68-71 @ v2.5.2).
    fn is_dynamic(&self) -> bool {
        self.exposure.is_dynamic() || self.contrast.is_dynamic() || self.gamma.is_dynamic()
    }

    /// Port of `ECRendererBase::hasDynamicProperty` (ExposureContrastOpCPU.cpp:73-96 @ v2.5.2).
    fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        match type_ {
            DynamicPropertyType::Exposure => self.exposure.is_dynamic(),
            DynamicPropertyType::Contrast => self.contrast.is_dynamic(),
            DynamicPropertyType::Gamma => self.gamma.is_dynamic(),
            DynamicPropertyType::GradingPrimary
            | DynamicPropertyType::GradingRgbCurve
            | DynamicPropertyType::GradingHueCurve
            | DynamicPropertyType::GradingTone => false,
        }
    }

    /// Port of `ECRendererBase::getDynamicProperty` (ExposureContrastOpCPU.cpp:98-130 @
    /// v2.5.2).
    fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        let prop = match type_ {
            DynamicPropertyType::Exposure => &self.exposure,
            DynamicPropertyType::Contrast => &self.contrast,
            DynamicPropertyType::Gamma => &self.gamma,
            DynamicPropertyType::GradingPrimary
            | DynamicPropertyType::GradingRgbCurve
            | DynamicPropertyType::GradingHueCurve
            | DynamicPropertyType::GradingTone => {
                return Err(Exception::new(TYPE_NOT_SUPPORTED));
            }
        };
        if prop.is_dynamic() {
            return Ok(DynamicPropertyRcPtr::Double(Arc::clone(prop)));
        }
        Err(Exception::new(NOT_DYNAMIC))
    }
}

/// One color channel times `factor`, in the operand order of each wheel's machine code.
///
/// The Windows wheel computes `factor * px` (`mulss xmm0, [rsi]`, `ECLinearRenderer::apply`
/// near 0x18017f586), so a NaN `factor` wins over a NaN pixel. The Linux wheel computes
/// `px * factor` in all four renderers, the vector loop and the scalar tail: `ECLinearRenderer`
/// at 0x33e6d6 and 0x33e793, `ECLinearRevRenderer` at 0x33ece3, `ECVideoRenderer` at 0x33f253
/// and `ECVideoRevRenderer` at 0x33e143 (read with `tools/wheel-inspect`).
#[inline(always)]
fn scale_channel(px: f32, factor: f32) -> f32 {
    #[cfg(windows)]
    {
        sse_mul(factor, px)
    }
    #[cfg(not(windows))]
    {
        sse_mul(px, factor)
    }
}

/// The color channels of each pixel times `factor`, alpha left alone: the scalar loops of a
/// contrast of 1, `out[i] = in[i] * factor`, in each wheel's operand order
/// ([`scale_channel`]).
fn scale_rgb(rgba: &mut [f32], factor: f32) {
    let exact = |block: &mut [[f32; 4]]| {
        for px in block {
            px[0] = scale_channel(px[0], factor);
            px[1] = scale_channel(px[1], factor);
            px[2] = scale_channel(px[2], factor);
        }
    };
    render(
        rgba,
        factor.is_finite(),
        |block| sse2_scale(block, factor),
        exact,
    );
}

/// The color lanes of the SSE2 power branch, alpha left alone:
/// `_mm_mul_ps(ssePower(_mm_mul_ps(data, scale), power), factor)`.
fn power_rgb(rgba: &mut [f32], scale: f32, power: f32, factor: f32) {
    let exact = |block: &mut [[f32; 4]]| {
        for px in block {
            for v in &mut px[..3] {
                *v = sse_mul(sse_power(sse_mul(*v, scale), power), factor);
            }
        }
    };
    let finite = scale.is_finite() && power.is_finite() && factor.is_finite();
    render(
        rgba,
        finite,
        |block| sse2_power(block, scale, power, factor),
        exact,
    );
}

/// `in * contrast + offset` on the color channels, in the wheels' order: the product first.
fn linear_rgb(rgba: &mut [f32], contrast: f32, offset: f32) {
    #[cfg(target_arch = "x86_64")]
    sse2_linear(rgba.as_chunks_mut::<4>().0, contrast, offset);
    #[cfg(not(target_arch = "x86_64"))]
    linear_exact(rgba.as_chunks_mut::<4>().0, contrast, offset);
}

/// The lanes of [`linear_rgb`], where its instructions don't compile.
#[cfg(not(target_arch = "x86_64"))]
fn linear_exact(block: &mut [[f32; 4]], contrast: f32, offset: f32) {
    for px in block {
        for v in &mut px[..3] {
            *v = sse_add(sse_mul(*v, contrast), offset);
        }
    }
}

/// [`linear_rgb`] between two buffers, as the C++ renders the first op of a chain: without a
/// copy of the input.
fn linear_between(input: Pixels<'_>, output: PixelsMut<'_>, contrast: f32, offset: f32) {
    let (Pixels::F32(input), PixelsMut::F32(output)) = (input, output) else {
        panic!("the renderers process float pixels only");
    };
    #[cfg(target_arch = "x86_64")]
    sse2_linear_to(
        input.as_chunks::<4>().0,
        output.as_chunks_mut::<4>().0,
        contrast,
        offset,
    );
    #[cfg(not(target_arch = "x86_64"))]
    {
        output.copy_from_slice(input);
        linear_exact(output.as_chunks_mut::<4>().0, contrast, offset);
    }
}

/// Renders `rgba` with `exact`, or block by block with `fast` where `finite` allows it.
#[cfg(target_arch = "x86_64")]
#[inline]
fn render(
    rgba: &mut [f32],
    finite: bool,
    fast: impl Fn(&mut [[f32; 4]]),
    exact: impl Fn(&mut [[f32; 4]]),
) {
    let pixels = rgba.as_chunks_mut::<4>().0;
    if finite {
        crate::sse2::blocks::for_each_block(pixels, fast, |block, _| exact(block));
    } else {
        exact(pixels);
    }
}

/// [`render`] where the real kernels don't compile.
#[cfg(not(target_arch = "x86_64"))]
#[inline]
fn render(
    rgba: &mut [f32],
    _finite: bool,
    _fast: impl Fn(&mut [[f32; 4]]),
    exact: impl Fn(&mut [[f32; 4]]),
) {
    exact(rgba.as_chunks_mut::<4>().0);
}

#[cfg(target_arch = "x86_64")]
use super::exposure_contrast_op_cpu_sse2::{
    linear as sse2_linear, linear_to as sse2_linear_to, power as sse2_power, scale as sse2_scale,
};

#[cfg(not(target_arch = "x86_64"))]
fn sse2_scale(_: &mut [[f32; 4]], _: f32) {}
#[cfg(not(target_arch = "x86_64"))]
fn sse2_power(_: &mut [[f32; 4]], _: f32, _: f32, _: f32) {}

/// Implements [`CpuOp`]'s dynamic property methods with the renderer's base.
macro_rules! base_dynamic_properties {
    () => {
        fn is_dynamic(&self) -> bool {
            self.base.is_dynamic()
        }

        fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
            self.base.has_dynamic_property(type_)
        }

        fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
            self.base.get_dynamic_property(type_)
        }
    };
}

/// The linear style: `out = pivot * (in * 2^exposure / pivot)^contrast`.
///
/// Port of `ECLinearRenderer` (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpCPU.cpp:
/// 133-235 @ v2.5.2).
#[derive(Debug)]
struct EcLinearRenderer {
    base: EcRendererBase,
}

impl EcLinearRenderer {
    /// Port of `ECLinearRenderer::ECLinearRenderer` and `updateData`
    /// (ExposureContrastOpCPU.cpp:144-153 @ v2.5.2).
    fn new(ec: &ExposureContrastOpData) -> Self {
        let mut base = EcRendererBase::new(ec);
        base.pivot = std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32;
        EcLinearRenderer { base }
    }
}

impl CpuOp for EcLinearRenderer {
    /// Port of `ECLinearRenderer::apply` (ExposureContrastOpCPU.cpp:155-235 @ v2.5.2), its
    /// `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let contrast_val = self.base.contrast_val();
        let exposure_val = std_powf(2.0, self.base.exposure.get_value() as f32);

        if contrast_val == 1.0 {
            // out = in * exposure;
            scale_rgb(rgba, exposure_val);
        } else {
            // out = powf( i * exposure / pivot, contrast ) * pivot
            let exposure_over_pivot = exposure_val / self.base.pivot;
            power_rgb(rgba, exposure_over_pivot, contrast_val, self.base.pivot);
        }
    }

    base_dynamic_properties!();
}

/// The reverse linear style.
///
/// Port of `ECLinearRevRenderer` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpCPU.cpp:237-341 @ v2.5.2).
#[derive(Debug)]
struct EcLinearRevRenderer {
    base: EcRendererBase,
}

impl EcLinearRevRenderer {
    /// Port of `ECLinearRevRenderer::ECLinearRevRenderer` and `updateData`
    /// (ExposureContrastOpCPU.cpp:248-257 @ v2.5.2).
    fn new(ec: &ExposureContrastOpData) -> Self {
        let mut base = EcRendererBase::new(ec);
        base.pivot = std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32;
        EcLinearRevRenderer { base }
    }
}

impl CpuOp for EcLinearRevRenderer {
    /// Port of `ECLinearRevRenderer::apply` (ExposureContrastOpCPU.cpp:259-341 @ v2.5.2), its
    /// `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let contrast_val = self.base.contrast_val();
        let inv_contrast_val = 1.0 / contrast_val;
        let inv_exposure_val = 1.0 / std_powf(2.0, self.base.exposure.get_value() as f32);

        if contrast_val == 1.0 {
            scale_rgb(rgba, inv_exposure_val);
        } else {
            let pivot_over_exposure_val = self.base.pivot * inv_exposure_val;
            let inv_pivot_val = 1.0 / self.base.pivot;
            // out = powf( i / pivot, 1 / contrast ) * pivot / exposure
            power_rgb(
                rgba,
                inv_pivot_val,
                inv_contrast_val,
                pivot_over_exposure_val,
            );
        }
    }

    base_dynamic_properties!();
}

/// `powf(x, (float)EC::VIDEO_OETF_POWER)`.
fn video_oetf(x: f32) -> f32 {
    x.powf(ec::VIDEO_OETF_POWER as f32)
}

/// The video style: `out = pivot^G * (in * (2^exposure / pivot)^G)^contrast`.
///
/// Port of `ECVideoRenderer` (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpCPU.cpp:
/// 343-443 @ v2.5.2).
#[derive(Debug)]
struct EcVideoRenderer {
    base: EcRendererBase,
}

impl EcVideoRenderer {
    /// Port of `ECVideoRenderer::ECVideoRenderer` and `updateData`
    /// (ExposureContrastOpCPU.cpp:354-364 @ v2.5.2).
    fn new(ec: &ExposureContrastOpData) -> Self {
        let mut base = EcRendererBase::new(ec);
        base.pivot = video_oetf(std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32);
        EcVideoRenderer { base }
    }
}

impl CpuOp for EcVideoRenderer {
    /// Port of `ECVideoRenderer::apply` (ExposureContrastOpCPU.cpp:366-443 @ v2.5.2), its
    /// `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let contrast_val = self.base.contrast_val();
        let exposure_val = video_oetf(std_powf(2.0, self.base.exposure.get_value() as f32));

        if contrast_val == 1.0 {
            // out = in * exposure;
            scale_rgb(rgba, exposure_val);
        } else {
            // out = powf( i * exposure / pivot, contrast ) * pivot
            let exposure_over_pivot = exposure_val / self.base.pivot;
            power_rgb(rgba, exposure_over_pivot, contrast_val, self.base.pivot);
        }
    }

    base_dynamic_properties!();
}

/// The reverse video style.
///
/// Port of `ECVideoRevRenderer` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpCPU.cpp:445-548 @ v2.5.2).
#[derive(Debug)]
struct EcVideoRevRenderer {
    base: EcRendererBase,
}

impl EcVideoRevRenderer {
    /// Port of `ECVideoRevRenderer::ECVideoRevRenderer` and `updateData`
    /// (ExposureContrastOpCPU.cpp:456-466 @ v2.5.2).
    fn new(ec: &ExposureContrastOpData) -> Self {
        let mut base = EcRendererBase::new(ec);
        base.pivot = video_oetf(std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32);
        EcVideoRevRenderer { base }
    }
}

impl CpuOp for EcVideoRevRenderer {
    /// Port of `ECVideoRevRenderer::apply` (ExposureContrastOpCPU.cpp:468-548 @ v2.5.2), its
    /// `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let contrast_val = self.base.contrast_val();
        let inv_contrast_val = 1.0 / contrast_val;
        let inv_exposure_val =
            1.0 / video_oetf(std_powf(2.0, self.base.exposure.get_value() as f32));
        let pivot_over_exposure_val = self.base.pivot * inv_exposure_val;
        let inv_pivot_val = 1.0 / self.base.pivot;

        if contrast_val == 1.0 {
            // out = in / exposure
            scale_rgb(rgba, inv_exposure_val);
        } else {
            // out = powf( i / pivot, 1 / contrast ) * pivot / exposure
            power_rgb(
                rgba,
                inv_pivot_val,
                inv_contrast_val,
                pivot_over_exposure_val,
            );
        }
    }

    base_dynamic_properties!();
}

/// The log style's pivot: `(float)std::max(0., log2(pivot / 0.18) * logExposureStep +
/// logMidGray)`, with `pivot` the op's, at least `EC::MIN_PIVOT`, as a `float`; the rest in
/// double. A NaN gives 0.
fn log_pivot(ec: &ExposureContrastOpData) -> f32 {
    let pivot = std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32;
    std_max(
        0.,
        (f64::from(pivot) / 0.18).log2() * ec.get_log_exposure_step() + ec.get_log_mid_gray(),
    ) as f32
}

/// The log style: `out = (in + exposure * logExposureStep - logPivot) * contrast + logPivot`.
///
/// Port of `ECLogarithmicRenderer` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpCPU.cpp:550-632 @ v2.5.2).
#[derive(Debug)]
struct EcLogarithmicRenderer {
    base: EcRendererBase,
}

impl EcLogarithmicRenderer {
    /// Port of `ECLogarithmicRenderer::ECLogarithmicRenderer` and `updateData`
    /// (ExposureContrastOpCPU.cpp:561-575 @ v2.5.2).
    fn new(ec: &ExposureContrastOpData) -> Self {
        let mut base = EcRendererBase::new(ec);
        base.pivot = log_pivot(ec);
        base.log_exposure_step = ec.get_log_exposure_step() as f32;
        EcLogarithmicRenderer { base }
    }
}

impl EcLogarithmicRenderer {
    /// `contrast` and `offset` of `out = in * contrast + offset`.
    ///
    /// Port of the first lines of `ECLogarithmicRenderer::apply`
    /// (ExposureContrastOpCPU.cpp:577-602 @ v2.5.2).
    fn line(&self) -> (f32, f32) {
        let exposure_val = sse_mul(
            self.base.exposure.get_value() as f32,
            self.base.log_exposure_step,
        );
        let contrast_val = self.base.contrast_val();
        let offset_val = sse_add(
            sse_mul(exposure_val - self.base.pivot, contrast_val),
            self.base.pivot,
        );

        // Equation is:
        // out = ( (in + expos) - pivot ) * contrast + pivot
        // Rearrange as:
        // out = [in * contrast] + [(expos - pivot) * contrast + pivot]
        (contrast_val, offset_val)
    }
}

impl CpuOp for EcLogarithmicRenderer {
    /// Port of `ECLogarithmicRenderer::apply` (ExposureContrastOpCPU.cpp:577-632 @ v2.5.2),
    /// its `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let (contrast_val, offset_val) = self.line();
        // out = ( in * contrast ) + offset, the wheel's _mm_add_ps with the product first.
        linear_rgb(rgba, contrast_val, offset_val);
    }

    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (contrast_val, offset_val) = self.line();
        linear_between(input, output, contrast_val, offset_val);
    }

    base_dynamic_properties!();
}

/// The reverse log style.
///
/// Port of `ECLogarithmicRevRenderer` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpCPU.cpp:634-685 @ v2.5.2).
#[derive(Debug)]
struct EcLogarithmicRevRenderer {
    base: EcRendererBase,
}

impl EcLogarithmicRevRenderer {
    /// Port of `ECLogarithmicRevRenderer::ECLogarithmicRevRenderer` and `updateData`
    /// (ExposureContrastOpCPU.cpp:645-657 @ v2.5.2). Upstream's `updateData` leaves the
    /// exposure step at its default (I-217).
    fn new(ec: &ExposureContrastOpData) -> Self {
        let mut base = EcRendererBase::new(ec);
        base.pivot = log_pivot(ec);
        EcLogarithmicRevRenderer { base }
    }
}

impl EcLogarithmicRevRenderer {
    /// `inv_contrast` and `neg_offset` of `out = in * inv_contrast + neg_offset`.
    ///
    /// Port of the first lines of `ECLogarithmicRevRenderer::apply`
    /// (ExposureContrastOpCPU.cpp:659-676 @ v2.5.2).
    fn line(&self) -> (f32, f32) {
        let exposure_val = sse_mul(
            self.base.exposure.get_value() as f32,
            self.base.log_exposure_step,
        );
        let inv_contrast_val = std_max(
            ec::MIN_CONTRAST,
            1. / (self.base.contrast.get_value() * self.base.gamma.get_value()),
        ) as f32;
        let neg_offset_val = sse_sub(
            sse_sub(self.base.pivot, sse_mul(self.base.pivot, inv_contrast_val)),
            exposure_val,
        );

        (inv_contrast_val, neg_offset_val)
    }
}

impl CpuOp for EcLogarithmicRevRenderer {
    /// Port of `ECLogarithmicRevRenderer::apply` (ExposureContrastOpCPU.cpp:659-685 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let (inv_contrast_val, neg_offset_val) = self.line();
        // out = ( in * inv_contrast ) + neg_offset
        linear_rgb(rgba, inv_contrast_val, neg_offset_val);
    }

    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (inv_contrast_val, neg_offset_val) = self.line();
        linear_between(input, output, inv_contrast_val, neg_offset_val);
    }

    base_dynamic_properties!();
}

/// The renderer of the op data's style.
///
/// Port of `GetExposureContrastCPURenderer` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpCPU.cpp:689-709 @ v2.5.2). Its "Unknown exposure contrast style" can't
/// happen.
pub fn get_exposure_contrast_cpu_renderer(ec: &ExposureContrastOpData) -> Arc<dyn CpuOp> {
    match ec.get_style() {
        ExposureContrastOpStyle::Linear => Arc::new(EcLinearRenderer::new(ec)),
        ExposureContrastOpStyle::LinearRev => Arc::new(EcLinearRevRenderer::new(ec)),
        ExposureContrastOpStyle::Video => Arc::new(EcVideoRenderer::new(ec)),
        ExposureContrastOpStyle::VideoRev => Arc::new(EcVideoRevRenderer::new(ec)),
        ExposureContrastOpStyle::Logarithmic => Arc::new(EcLogarithmicRenderer::new(ec)),
        ExposureContrastOpStyle::LogarithmicRev => Arc::new(EcLogarithmicRevRenderer::new(ec)),
    }
}

#[cfg(test)]
#[path = "exposure_contrast_op_cpu_tests.rs"]
mod tests;
