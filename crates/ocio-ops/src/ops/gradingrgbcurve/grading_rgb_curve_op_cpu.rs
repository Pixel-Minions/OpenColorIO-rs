// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingRGBCurve op's CPU renderers: a port of
//! `src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpCPU.h` and `GradingRGBCurveOpCPU.cpp`
//! @ v2.5.2.
//!
//! [`get_grading_rgb_curve_cpu_renderer`] picks one by direction, and for the linear style
//! (unless it bypasses it) one that converts to a log encoding around the curves. The curves
//! are evaluated from the dynamic property's knots and coefficients
//! ([`KnotsCoefs::eval_curve`], [`KnotsCoefs::eval_curve_rev`]): red, green and blue, then
//! master, forward; master, then red, green and blue, inverse.
//!
//! # The linear style's conversion and alpha
//!
//! Every x86-64 wheel takes the `OCIO_USE_SSE2` branch of `LinLog` and `LogLin`
//! (GradingRGBCurveOpCPU.cpp:179-229): four lanes with `sseLog2` and `ssePower` whatever the
//! optimization flags, and compare-and-select masks. The four lanes include alpha, which
//! upstream then restores with `out[3] = in[3]`. That restores it only when the renderer reads
//! one buffer and writes another, as the CPU processor's first op does from a packed F32
//! image (`ScanlineHelper.cpp:130-137`). Applied in place, as every other op of a chain is
//! (`CPUProcessor.cpp:400`), `in[3]` is the converted alpha, so alpha comes out as
//! `LogLin(LinLog(alpha))` (`docs/improvements.md` I-90). The port does the same:
//! [`CpuOp::apply`] converts alpha in place, and [`CpuOp::apply_bit_depth`] restores it from
//! its input. The other renderers never write alpha.

use std::sync::Arc;

use super::grading_b_spline_curve::KnotsCoefs;
use super::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use crate::dynamic_property::{DynamicPropertyGradingRgbCurveImplRcPtr, DynamicPropertyRcPtr};
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Pixels, PixelsMut};
use crate::open_color_types::{
    DynamicPropertyType, GradingStyle, RgbCurveType, TransformDirection,
};
use crate::sse::{sse_log2, sse_power};

const RED: usize = RgbCurveType::Red as usize;
const GREEN: usize = RgbCurveType::Green as usize;
const BLUE: usize = RgbCurveType::Blue as usize;
const MASTER: usize = RgbCurveType::Master as usize;

/// What every renderer holds: the dynamic property, the op data's own or, for a dynamic op, a
/// copy that the CPU processor exposes.
///
/// Port of `GradingRGBCurveOpCPU` (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpCPU.cpp:
/// 19-95 @ v2.5.2).
#[derive(Debug)]
struct GradingRgbCurveOpCpu {
    /// `m_grgbcurve`.
    grgbcurve: DynamicPropertyGradingRgbCurveImplRcPtr,
}

impl GradingRgbCurveOpCpu {
    /// Port of `GradingRGBCurveOpCPU::GradingRGBCurveOpCPU` (GradingRGBCurveOpCPU.cpp:56-63 @
    /// v2.5.2).
    fn new(grgbc: &GradingRgbCurveOpData) -> Result<Self> {
        let mut grgbcurve = grgbc.get_dynamic_property_internal();
        if grgbcurve.is_dynamic() {
            grgbcurve = grgbcurve.create_editable_copy()?;
        }
        Ok(GradingRgbCurveOpCpu { grgbcurve })
    }

    /// Port of `GradingRGBCurveOpCPU::eval` (GradingRGBCurveOpCPU.cpp:33-43 @ v2.5.2).
    fn eval(knots_coefs: &KnotsCoefs, px: &mut [f32; 4]) {
        px[0] = knots_coefs.eval_curve(RED, px[0], px[0]);
        px[1] = knots_coefs.eval_curve(GREEN, px[1], px[1]);
        px[2] = knots_coefs.eval_curve(BLUE, px[2], px[2]);
        px[0] = knots_coefs.eval_curve(MASTER, px[0], px[0]);
        px[1] = knots_coefs.eval_curve(MASTER, px[1], px[1]);
        px[2] = knots_coefs.eval_curve(MASTER, px[2], px[2]);
    }

    /// Port of `GradingRGBCurveOpCPU::evalRev` (GradingRGBCurveOpCPU.cpp:44-53 @ v2.5.2).
    fn eval_rev(knots_coefs: &KnotsCoefs, px: &mut [f32; 4]) {
        px[0] = knots_coefs.eval_curve_rev(MASTER, px[0]);
        px[1] = knots_coefs.eval_curve_rev(MASTER, px[1]);
        px[2] = knots_coefs.eval_curve_rev(MASTER, px[2]);
        px[0] = knots_coefs.eval_curve_rev(RED, px[0]);
        px[1] = knots_coefs.eval_curve_rev(GREEN, px[1]);
        px[2] = knots_coefs.eval_curve_rev(BLUE, px[2]);
    }

    /// Applies `curves` (`eval` or `eval_rev`) to each pixel, around the linear style's
    /// conversion when `lin_to_log`; nothing when the property says to bypass.
    fn apply(&self, rgba: &mut [f32], lin_to_log: bool, curves: fn(&KnotsCoefs, &mut [f32; 4])) {
        let state = self.grgbcurve.state();
        let knots_coefs = &state.knots_coefs;
        if knots_coefs.local_bypass {
            return;
        }
        for px in rgba.as_chunks_mut::<4>().0 {
            if lin_to_log {
                lin_log(px);
            }
            curves(knots_coefs, px);
            if lin_to_log {
                log_lin(px);
            }
        }
    }

    /// The renderer between two buffers: in place on a copy of the input, then for the
    /// linear style, alpha restored from the input (`out[3] = in[3]`), as upstream's separate
    /// buffers give it. The restore is a loop of its own, on the bits, so that it can't
    /// become arithmetic on alpha.
    fn apply_bit_depth(
        &self,
        input: Pixels<'_>,
        output: PixelsMut<'_>,
        lin_to_log: bool,
        curves: fn(&KnotsCoefs, &mut [f32; 4]),
        name: &str,
    ) {
        match (input, output) {
            (Pixels::F32(input), PixelsMut::F32(output)) => {
                output.copy_from_slice(input);
                self.apply(output, lin_to_log, curves);
                if lin_to_log {
                    let inputs = input.as_chunks::<4>().0;
                    for (out, inp) in output.as_chunks_mut::<4>().0.iter_mut().zip(inputs) {
                        out[3] = f32::from_bits(std::hint::black_box(inp[3].to_bits()));
                    }
                }
            }
            (input, output) => panic!(
                "{name} processes float pixels only, not {} to {}",
                input.type_name(),
                output.type_name()
            ),
        }
    }

    /// Port of `GradingRGBCurveOpCPU::isDynamic` (GradingRGBCurveOpCPU.cpp:65-68 @ v2.5.2).
    fn is_dynamic(&self) -> bool {
        self.grgbcurve.is_dynamic()
    }

    /// Port of `GradingRGBCurveOpCPU::hasDynamicProperty` (GradingRGBCurveOpCPU.cpp:70-78 @
    /// v2.5.2).
    fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        type_ == DynamicPropertyType::GradingRgbCurve && self.grgbcurve.is_dynamic()
    }

    /// Port of `GradingRGBCurveOpCPU::getDynamicProperty` (GradingRGBCurveOpCPU.cpp:80-95 @
    /// v2.5.2).
    fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        if type_ == DynamicPropertyType::GradingRgbCurve {
            if self.grgbcurve.is_dynamic() {
                return Ok(DynamicPropertyRcPtr::GradingRgbCurve(Arc::clone(
                    &self.grgbcurve,
                )));
            }
        } else {
            return Err(Exception::new(
                "Dynamic property type not supported by GradingRGBCurve.",
            ));
        }
        Err(Exception::new("GradingRGBCurve property is not dynamic."))
    }
}

/// The constants of the linear style's conversion (GradingRGBCurveOpCPU.cpp:157-177 @
/// v2.5.2), `float` arithmetic in `constexpr` and in the `_mm_set1_ps` initializers.
mod log_lin {
    pub(super) const XBRK: f32 = 0.0041318374739483946;
    pub(super) const SHIFT: f32 = -0.000157849851665374;
    pub(super) const M: f32 = 1.0 / (0.18 + SHIFT);
    pub(super) const GAIN: f32 = 363.034608563;
    pub(super) const OFFS: f32 = -7.0;
    pub(super) const YBRK: f32 = -5.5;
    /// `mgainInv`: `1.f / gain`.
    pub(super) const GAIN_INV: f32 = 1.0 / GAIN;
    /// `mshift018`: `shift + 0.18f`.
    pub(super) const SHIFT_018: f32 = SHIFT + 0.18;
    /// `mpower`.
    pub(super) const POWER: f32 = 2.0;
}

/// The `OCIO_USE_SSE2` branch of `LinLog`, on all four lanes: `sseLog2((x + shift) * m)`
/// above the break, `x * gain + offs` at or below it and for NaN.
///
/// Port of `LinLog` (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpCPU.cpp:179-207 @
/// v2.5.2).
fn lin_log(px: &mut [f32; 4]) {
    for v in px.iter_mut() {
        let pix = *v;
        // _mm_cmpgt_ps(pix, mxbrk)
        let flag = pix > log_lin::XBRK;
        let pix_lin = pix * log_lin::GAIN + log_lin::OFFS;
        let log = sse_log2((pix + log_lin::SHIFT) * log_lin::M);
        // _mm_or_ps(_mm_and_ps(flag, pix), _mm_andnot_ps(flag, pixLin))
        *v = if flag { log } else { pix_lin };
    }
}

/// The `OCIO_USE_SSE2` branch of `LogLin`, on all four lanes: `ssePower(2, x) * (shift +
/// 0.18) - shift` above the break, `(x - offs) / gain` (as a product with `1 / gain`) at or
/// below it and for NaN.
///
/// Port of `LogLin` (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpCPU.cpp:209-235 @
/// v2.5.2).
fn log_lin(px: &mut [f32; 4]) {
    for v in px.iter_mut() {
        let pix = *v;
        // _mm_cmpgt_ps(pix, mybrk)
        let flag = pix > log_lin::YBRK;
        let pix_lin = (pix - log_lin::OFFS) * log_lin::GAIN_INV;
        let lin = sse_power(log_lin::POWER, pix) * log_lin::SHIFT_018 - log_lin::SHIFT;
        *v = if flag { lin } else { pix_lin };
    }
}

macro_rules! renderer {
    ($(#[$doc:meta])* $name:ident, $lin_to_log:expr, $curves:path) => {
        $(#[$doc])*
        #[derive(Debug)]
        pub struct $name(GradingRgbCurveOpCpu);

        impl $name {
            /// The renderer of the op data.
            pub fn new(grgbc: &GradingRgbCurveOpData) -> Result<Self> {
                Ok($name(GradingRgbCurveOpCpu::new(grgbc)?))
            }
        }

        impl CpuOp for $name {
            fn apply(&self, rgba: &mut [f32]) {
                self.0.apply(rgba, $lin_to_log, $curves);
            }

            fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
                self.0
                    .apply_bit_depth(input, output, $lin_to_log, $curves, stringify!($name));
            }

            fn is_dynamic(&self) -> bool {
                self.0.is_dynamic()
            }

            fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
                self.0.has_dynamic_property(type_)
            }

            fn get_dynamic_property(
                &self,
                type_: DynamicPropertyType,
            ) -> Result<DynamicPropertyRcPtr> {
                self.0.get_dynamic_property(type_)
            }
        }
    };
}

renderer!(
    /// Forward: the curves.
    ///
    /// Port of `GradingRGBCurveFwdOpCPU` (src/OpenColorIO/ops/gradingrgbcurve/
    /// GradingRGBCurveOpCPU.cpp:97-142 @ v2.5.2).
    GradingRgbCurveFwdOpCpu,
    false,
    GradingRgbCurveOpCpu::eval
);

renderer!(
    /// Forward, linear style: to the log encoding, the curves, back to linear.
    ///
    /// Port of `GradingRGBCurveLinearFwdOpCPU` (src/OpenColorIO/ops/gradingrgbcurve/
    /// GradingRGBCurveOpCPU.cpp:144-155, 237-263 @ v2.5.2).
    GradingRgbCurveLinearFwdOpCpu,
    true,
    GradingRgbCurveOpCpu::eval
);

renderer!(
    /// Inverse: the curves' inverses.
    ///
    /// Port of `GradingRGBCurveRevOpCPU` (src/OpenColorIO/ops/gradingrgbcurve/
    /// GradingRGBCurveOpCPU.cpp:265-304 @ v2.5.2).
    GradingRgbCurveRevOpCpu,
    false,
    GradingRgbCurveOpCpu::eval_rev
);

renderer!(
    /// Inverse, linear style: to the log encoding, the curves' inverses, back to linear.
    ///
    /// Port of `GradingRGBCurveLinearRevOpCPU` (src/OpenColorIO/ops/gradingrgbcurve/
    /// GradingRGBCurveOpCPU.cpp:306-347 @ v2.5.2).
    GradingRgbCurveLinearRevOpCpu,
    true,
    GradingRgbCurveOpCpu::eval_rev
);

/// The renderer of `prim`: by direction, with the linear style's conversion unless the style
/// isn't linear or the op bypasses it. Its "Illegal GradingRGBCurve direction." can't happen.
///
/// Port of `GetGradingRGBCurveCPURenderer` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingRGBCurveOpCPU.cpp:353-385 @ v2.5.2).
pub fn get_grading_rgb_curve_cpu_renderer(prim: &GradingRgbCurveOpData) -> Result<Arc<dyn CpuOp>> {
    let lin_to_log = prim.get_style() == GradingStyle::Lin && !prim.get_bypass_lin_to_log();
    Ok(match (prim.get_direction(), lin_to_log) {
        (TransformDirection::Forward, true) => Arc::new(GradingRgbCurveLinearFwdOpCpu::new(prim)?),
        (TransformDirection::Forward, false) => Arc::new(GradingRgbCurveFwdOpCpu::new(prim)?),
        (TransformDirection::Inverse, true) => Arc::new(GradingRgbCurveLinearRevOpCpu::new(prim)?),
        (TransformDirection::Inverse, false) => Arc::new(GradingRgbCurveRevOpCpu::new(prim)?),
    })
}

#[cfg(test)]
#[path = "grading_rgb_curve_op_cpu_tests.rs"]
mod tests;
