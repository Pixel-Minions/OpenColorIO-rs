// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingHueCurve op's CPU renderers: a port of
//! `src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOpCPU.h` and
//! `GradingHueCurveOpCPU.cpp` @ v2.5.2.
//!
//! [`get_grading_hue_curve_cpu_renderer`] picks the draw renderer for an op that only draws its
//! curves (whatever its direction), else the forward or the reverse renderer. Those convert
//! each pixel from RGB to hue, saturation and luma (HSY) with the FixedFunction renderer of the
//! style, or an identity matrix when the op doesn't convert, evaluate the curves from the
//! dynamic property's knots and coefficients (`KnotsCoefs::eval_curve`, `eval_curve_rev`,
//! `eval_curve_rev_hue`), and convert back. The
//! linear style grades luma in a log encoding (`LinLog`, `LogLin`).
//!
//! # The wheels' machine code
//!
//! Both wheels call the conversions through their renderers (virtual calls), the curves'
//! evaluations out of line (Windows `sub_1801e87d0`, `sub_1801e8970`, `sub_1801e8bc0`; Linux
//! `KnotsCoefs::evalCurve` `0x3bc930`, `evalCurveRev` `0x3bcab0`, `evalCurveRevHue`
//! `0x3bcd20`: the same functions the RGB curves call), and `LinLog` and `LogLin` through
//! function pointers (Windows `0x1801cc700`, `0x1801cc770`; Linux `0x399000`, `0x398fa0`),
//! which call `logf` and `powf`. The gains go through `std::max(0.f, gain)`, so they are never
//! NaN, but they can be infinite: a product of an infinite gain and 0 is x86's default NaN,
//! which can then meet a NaN pixel. Where two NaNs can meet, the operand orders are the
//! machine code's (Windows `GradingHueCurveFwdOpCPU::apply` `0x1801cc890`, Linux `0x399060`):
//! - the linear style's luma gain: Windows `(lum * hueLumGain) * satLumGain`, as the source;
//!   Linux `satLumGain * (hueLumGain * lum)`;
//! - the log and video styles' luma shift: both `((satLumGain + hueLumGain) - 2) * 0.1 +
//!   lum`, for the source's `lum + (hueLumGain + satLumGain - 2) * 0.1`;
//! - the hue effect: both `HueFX(hue) + hue`, for the source's `hue + HueFX(hue)`.
//!
//! The reverse renderer (Windows `0x1801ccb20`, Linux `0x399350`) has no operation where two
//! NaNs meet: its saturation is clamped to 0 first, its gains are never NaN, and its luma
//! shift and divisions keep their operands in order. Its two products of gains are written in
//! each wheel's order (the gains' product can only be the default NaN, whatever the order).
//!
//! The renderers read the property once per [`CpuOp::apply`] (U-81), and never write alpha:
//! the HSY conversions leave it, and the identity matrix of an op that doesn't convert scales
//! it by 1, as upstream's does.

use std::sync::Arc;

use super::grading_hue_curve_op_data::GradingHueCurveOpData;
use crate::dynamic_property::{DynamicPropertyGradingHueCurveImplRcPtr, DynamicPropertyRcPtr};
use crate::exception::{Exception, Result};
use crate::math_utils::{sse_add, sse_mul, sse_sub, std_max, std_min, std_powf};
use crate::op::CpuOp;
use crate::open_color_types::{
    DynamicPropertyType, GradingStyle, HsyTransformStyle, HueCurveType, TransformDirection,
};
use crate::ops::fixedfunction::fixed_function_op_cpu::get_fixed_function_cpu_renderer;
use crate::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle,
};
use crate::ops::matrix::matrix_op_cpu::get_matrix_renderer;
use crate::ops::matrix::matrix_op_data::MatrixOpData;

const HUE_HUE: usize = HueCurveType::HueHue as usize;
const HUE_SAT: usize = HueCurveType::HueSat as usize;
const HUE_LUM: usize = HueCurveType::HueLum as usize;
const LUM_SAT: usize = HueCurveType::LumSat as usize;
const SAT_SAT: usize = HueCurveType::SatSat as usize;
const LUM_LUM: usize = HueCurveType::LumLum as usize;
const SAT_LUM: usize = HueCurveType::SatLum as usize;
const HUE_FX: usize = HueCurveType::HueFx as usize;

/// Whether this is the Windows build, whose wheel's machine code orders some operands
/// differently from the Linux wheel's (D12).
const WINDOWS: bool = cfg!(target_os = "windows");

/// `LogLinConstants` (GradingHueCurveOpCPU.cpp:24-33 @ v2.5.2), `float` arithmetic in
/// `constexpr`.
mod log_lin {
    pub(super) const XBRK: f32 = 0.0041318374739483946;
    pub(super) const SHIFT: f32 = -0.000157849851665374;
    pub(super) const M: f32 = 1.0 / (0.18 + SHIFT);
    pub(super) const GAIN: f32 = 363.034608563;
    pub(super) const OFFS: f32 = -7.0;
    pub(super) const YBRK: f32 = -5.5;
    /// `1/log(2)`.
    pub(super) const BASE2: f32 = 1.4426950408889634;
}

/// Luma (blue) to the grading log: `x * gain + offs` below the break,
/// `base2 * log((x + shift) * m)` at or above it and for NaN. Both wheels compute
/// `log(...) * base2` and `x * gain - 7`: the same values (one operand at most is NaN).
///
/// Port of `LinLog` (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOpCPU.cpp:35-40 @
/// v2.5.2).
fn lin_log(out: &mut [f32; 4]) {
    out[2] = if out[2] < log_lin::XBRK {
        out[2] * log_lin::GAIN + log_lin::OFFS
    } else {
        ((out[2] + log_lin::SHIFT) * log_lin::M).ln() * log_lin::BASE2
    };
}

/// Luma (blue) from the grading log: `(x - offs) / gain` below the break, `2^x * (0.18 +
/// shift) - shift` at or above it and for NaN (`powf`; Linux adds `-shift` first: the same
/// value).
///
/// Port of `LogLin` (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOpCPU.cpp:42-47 @
/// v2.5.2).
fn log_lin(out: &mut [f32; 4]) {
    out[2] = if out[2] < log_lin::YBRK {
        (out[2] - log_lin::OFFS) / log_lin::GAIN
    } else {
        std_powf(2.0, out[2]) * (0.18 + log_lin::SHIFT) - log_lin::SHIFT
    };
}

/// What every renderer holds: the dynamic property (the op data's own or, for a dynamic op, a
/// copy that the CPU processor exposes), the style's conversions to and from HSY, and whether
/// the style is linear, which grades luma in the log encoding.
///
/// Port of `GradingHueCurveOpCPU` (src/OpenColorIO/ops/gradinghuecurve/
/// GradingHueCurveOpCPU.cpp:55-160 @ v2.5.2). Its function pointers `m_applyLinLog` and
/// `m_applyLogLin` are `NoOp` unless the style is linear: `is_linear` says which.
#[derive(Debug)]
struct GradingHueCurveOpCpu {
    /// `m_ghuecurve`.
    ghuecurve: DynamicPropertyGradingHueCurveImplRcPtr,
    /// `m_isLinear`, and whether `m_applyLinLog` and `m_applyLogLin` are `LinLog` and `LogLin`.
    is_linear: bool,
    /// `m_rgbToHsyOp`.
    rgb_to_hsy_op: Arc<dyn CpuOp>,
    /// `m_hsyToRgbOp`.
    hsy_to_rgb_op: Arc<dyn CpuOp>,
}

impl GradingHueCurveOpCpu {
    /// Port of `GradingHueCurveOpCPU::GradingHueCurveOpCPU` (GradingHueCurveOpCPU.cpp:78-128 @
    /// v2.5.2).
    fn new(gc_data: &GradingHueCurveOpData) -> Result<Self> {
        let mut ghuecurve = gc_data.get_dynamic_property_internal();
        if ghuecurve.is_dynamic() {
            ghuecurve = ghuecurve.create_editable_copy()?;
        }

        let style = gc_data.get_style();
        let (fwd_style, inv_style) = match style {
            GradingStyle::Lin => (
                FixedFunctionOpStyle::RgbToHsyLin,
                FixedFunctionOpStyle::HsyLinToRgb,
            ),
            GradingStyle::Log => (
                FixedFunctionOpStyle::RgbToHsyLog,
                FixedFunctionOpStyle::HsyLogToRgb,
            ),
            GradingStyle::Video => (
                FixedFunctionOpStyle::RgbToHsyVid,
                FixedFunctionOpStyle::HsyVidToRgb,
            ),
        };

        let (rgb_to_hsy_op, hsy_to_rgb_op) = if gc_data.get_rgb_to_hsy() == HsyTransformStyle::None
        {
            // TODO: Could template the apply function to avoid the need for a matrix op.
            let fwd_op_data = MatrixOpData::with_direction(TransformDirection::Forward);
            (
                get_matrix_renderer(&fwd_op_data)?,
                get_matrix_renderer(&fwd_op_data)?,
            )
        } else {
            let fwd_op_data = FixedFunctionOpData::new(fwd_style)?;
            let inv_op_data = FixedFunctionOpData::new(inv_style)?;
            (
                get_fixed_function_cpu_renderer(&fwd_op_data, false)?,
                get_fixed_function_cpu_renderer(&inv_op_data, false)?,
            )
        };

        Ok(GradingHueCurveOpCpu {
            ghuecurve,
            is_linear: style == GradingStyle::Lin,
            rgb_to_hsy_op,
            hsy_to_rgb_op,
        })
    }

    /// Port of `GradingHueCurveOpCPU::isDynamic` (GradingHueCurveOpCPU.cpp:130-133 @ v2.5.2).
    fn is_dynamic(&self) -> bool {
        self.ghuecurve.is_dynamic()
    }

    /// Port of `GradingHueCurveOpCPU::hasDynamicProperty` (GradingHueCurveOpCPU.cpp:135-143 @
    /// v2.5.2).
    fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        type_ == DynamicPropertyType::GradingHueCurve && self.ghuecurve.is_dynamic()
    }

    /// Port of `GradingHueCurveOpCPU::getDynamicProperty` (GradingHueCurveOpCPU.cpp:145-160 @
    /// v2.5.2).
    fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        if type_ == DynamicPropertyType::GradingHueCurve {
            if self.ghuecurve.is_dynamic() {
                return Ok(DynamicPropertyRcPtr::GradingHueCurve(Arc::clone(
                    &self.ghuecurve,
                )));
            }
        } else {
            return Err(Exception::new(
                "Dynamic property type not supported by GradingHueCurve.",
            ));
        }
        Err(Exception::new("GradingHueCurve property is not dynamic."))
    }

    /// `m_applyLinLog`.
    fn apply_lin_log(&self, out: &mut [f32; 4]) {
        if self.is_linear {
            lin_log(out);
        }
    }

    /// `m_applyLogLin`.
    fn apply_log_lin(&self, out: &mut [f32; 4]) {
        if self.is_linear {
            log_lin(out);
        }
    }

    /// The draw renderer's pixels: the hue-sat curve of each of R, G and B; alpha untouched.
    ///
    /// Port of `GradingHueCurveDrawOpCPU::apply` (GradingHueCurveOpCPU.cpp:177-198 @ v2.5.2).
    fn apply_draw(&self, rgba: &mut [f32]) {
        // NB: LocalBypass does not matter, need to evaluate even if it's an identity.
        let state = self.ghuecurve.state();
        let knots_coefs = &state.knots_coefs;
        for out in rgba.as_chunks_mut::<4>().0 {
            // In drawCurveOnly mode, only evaluate the HueSat curve, with no RGB-to-HSY or
            // LogLin. (In practice it may be any of the curves, but only eval the one stored in
            // that slot.)
            out[0] = knots_coefs.eval_curve(HUE_SAT, out[0], 1.0);
            out[1] = knots_coefs.eval_curve(HUE_SAT, out[1], 1.0);
            out[2] = knots_coefs.eval_curve(HUE_SAT, out[2], 1.0);
        }
    }

    /// The forward renderer's pixels.
    ///
    /// Port of `GradingHueCurveFwdOpCPU::apply` (GradingHueCurveOpCPU.cpp:217-278 @ v2.5.2),
    /// in the wheels' operand orders (module docs).
    fn apply_fwd(&self, rgba: &mut [f32]) {
        let state = self.ghuecurve.state();
        let knots_coefs = &state.knots_coefs;
        if knots_coefs.local_bypass {
            return;
        }
        for out in rgba.as_chunks_mut::<4>().0 {
            self.rgb_to_hsy_op.apply(out);

            self.apply_lin_log(out);

            // HUE-SAT
            let hue_sat_gain = std_max(0.0, knots_coefs.eval_curve(HUE_SAT, out[0], 1.0));
            // HUE-LUM
            let mut hue_lum_gain = std_max(0.0, knots_coefs.eval_curve(HUE_LUM, out[0], 1.0));
            // HUE-HUE
            out[0] = knots_coefs.eval_curve(HUE_HUE, out[0], out[0]);
            // SAT-SAT
            out[1] = std_max(0.0, knots_coefs.eval_curve(SAT_SAT, out[1], out[1]));
            // LUM-SAT
            let lum_sat_gain = std_max(0.0, knots_coefs.eval_curve(LUM_SAT, out[2], 1.0));

            // Apply sat gain.
            let sat_gain = sse_mul(lum_sat_gain, hue_sat_gain);
            out[1] = sse_mul(sat_gain, out[1]);

            // SAT-LUM
            let sat_lum_gain = std_max(0.0, knots_coefs.eval_curve(SAT_LUM, out[1], 1.0));
            // LUM-LUM
            out[2] = knots_coefs.eval_curve(LUM_LUM, out[2], out[2]);

            self.apply_log_lin(out);

            // Limit hue-lum gain at low sat, since the hue is more noisy, and when sat is 0
            // the hue becomes unknown (and is not invertible).
            hue_lum_gain = sse_sub(1.0, sse_mul(1.0 - hue_lum_gain, std_min(out[1], 1.0)));

            // Apply lum gain.
            out[2] = if self.is_linear {
                if WINDOWS {
                    sse_mul(sse_mul(out[2], hue_lum_gain), sat_lum_gain)
                } else {
                    sse_mul(sat_lum_gain, sse_mul(hue_lum_gain, out[2]))
                }
            } else {
                let shift = sse_mul(sse_sub(sse_add(sat_lum_gain, hue_lum_gain), 2.0), 0.1);
                sse_add(shift, out[2])
            };

            // HUE-FX
            out[0] -= out[0].floor(); // wrap to [0,1)
            out[0] = sse_add(knots_coefs.eval_curve(HUE_FX, out[0], 0.0), out[0]);

            self.hsy_to_rgb_op.apply(out);
        }
    }

    /// The reverse renderer's pixels.
    ///
    /// Port of `GradingHueCurveRevOpCPU::apply` (GradingHueCurveOpCPU.cpp:295-359 @ v2.5.2),
    /// in the wheels' operand orders (module docs).
    fn apply_rev(&self, rgba: &mut [f32]) {
        let state = self.ghuecurve.state();
        let knots_coefs = &state.knots_coefs;
        if knots_coefs.local_bypass {
            return;
        }
        for out in rgba.as_chunks_mut::<4>().0 {
            self.rgb_to_hsy_op.apply(out);

            // Invert HUE-FX.
            out[0] = knots_coefs.eval_curve_rev_hue(HUE_FX, out[0]);

            // Invert HUE-HUE.
            out[0] = knots_coefs.eval_curve_rev_hue(HUE_HUE, out[0]);

            // Use the inverted hue to calculate the HUE-SAT & HUE-LUM gains.
            out[0] -= out[0].floor(); // wrap to [0,1)
            let hue_sat_gain = std_max(0.0, knots_coefs.eval_curve(HUE_SAT, out[0], 1.0));
            let mut hue_lum_gain = std_max(0.0, knots_coefs.eval_curve(HUE_LUM, out[0], 1.0));

            // Use the output sat to calculate the SAT-LUM gain.
            out[1] = std_max(0.0, out[1]); // guard against negative saturation
            let sat_lum_gain = std_max(0.0, knots_coefs.eval_curve(SAT_LUM, out[1], 1.0));

            hue_lum_gain = sse_sub(1.0, sse_mul(1.0 - hue_lum_gain, std_min(out[1], 1.0)));

            // Invert the lum gain.
            let lum_gain = if WINDOWS {
                sse_mul(hue_lum_gain, sat_lum_gain)
            } else {
                sse_mul(sat_lum_gain, hue_lum_gain)
            };
            out[2] = if self.is_linear {
                out[2] / std_max(0.01, lum_gain)
            } else {
                let shift = sse_mul(sse_sub(sse_add(sat_lum_gain, hue_lum_gain), 2.0), 0.1);
                sse_sub(out[2], shift)
            };

            self.apply_lin_log(out);

            // Invert LUM-LUM.
            out[2] = knots_coefs.eval_curve_rev(LUM_LUM, out[2]);

            // Use it to calc the LUM-SAT gain.
            let lum_sat_gain = std_max(0.0, knots_coefs.eval_curve(LUM_SAT, out[2], 1.0));

            self.apply_log_lin(out);

            // Invert the sat gain.
            let sat_gain = if WINDOWS {
                sse_mul(lum_sat_gain, hue_sat_gain)
            } else {
                sse_mul(hue_sat_gain, lum_sat_gain)
            };
            out[1] /= std_max(0.01, sat_gain);

            // Invert SAT-SAT.
            out[1] = std_max(0.0, knots_coefs.eval_curve_rev(SAT_SAT, out[1]));

            self.hsy_to_rgb_op.apply(out);
        }
    }
}

macro_rules! renderer {
    ($(#[$doc:meta])* $name:ident, $apply:ident) => {
        $(#[$doc])*
        #[derive(Debug)]
        pub struct $name(GradingHueCurveOpCpu);

        impl $name {
            /// The renderer of the op data.
            pub fn new(ghuec: &GradingHueCurveOpData) -> Result<Self> {
                Ok($name(GradingHueCurveOpCpu::new(ghuec)?))
            }
        }

        impl CpuOp for $name {
            fn apply(&self, rgba: &mut [f32]) {
                self.0.$apply(rgba);
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
    /// Draw-curve-only: the hue-sat curve of each channel, for a user interface.
    ///
    /// Port of `GradingHueCurveDrawOpCPU` (src/OpenColorIO/ops/gradinghuecurve/
    /// GradingHueCurveOpCPU.cpp:162-198 @ v2.5.2).
    GradingHueCurveDrawOpCpu,
    apply_draw
);

renderer!(
    /// Forward: to HSY, the curves, back to RGB.
    ///
    /// Port of `GradingHueCurveFwdOpCPU` (src/OpenColorIO/ops/gradinghuecurve/
    /// GradingHueCurveOpCPU.cpp:200-278 @ v2.5.2).
    GradingHueCurveFwdOpCpu,
    apply_fwd
);

renderer!(
    /// Inverse: to HSY, the curves' inverses, back to RGB.
    ///
    /// Port of `GradingHueCurveRevOpCPU` (src/OpenColorIO/ops/gradinghuecurve/
    /// GradingHueCurveOpCPU.cpp:280-359 @ v2.5.2).
    GradingHueCurveRevOpCpu,
    apply_rev
);

/// The renderer of `prim`: the draw renderer when its curves only draw (whatever the
/// direction), else by direction. Its "Illegal GradingHueCurve direction." can't happen.
///
/// Port of `GetGradingHueCurveCPURenderer` (src/OpenColorIO/ops/gradinghuecurve/
/// GradingHueCurveOpCPU.cpp:365-390 @ v2.5.2).
pub fn get_grading_hue_curve_cpu_renderer(prim: &GradingHueCurveOpData) -> Result<Arc<dyn CpuOp>> {
    // DrawCurveOnly mode ignores the direction, it's always the forward transform.
    let dyn_prop = prim.get_dynamic_property_internal();
    if dyn_prop.get_value().get_draw_curve_only() {
        return Ok(Arc::new(GradingHueCurveDrawOpCpu::new(prim)?));
    }

    Ok(match prim.get_direction() {
        TransformDirection::Forward => Arc::new(GradingHueCurveFwdOpCpu::new(prim)?),
        TransformDirection::Inverse => Arc::new(GradingHueCurveRevOpCpu::new(prim)?),
    })
}

#[cfg(test)]
#[path = "grading_hue_curve_op_cpu_tests.rs"]
mod tests;
