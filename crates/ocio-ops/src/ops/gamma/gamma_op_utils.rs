// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The moncurve renderers' coefficients: the break point and slope of the linear segment are
//! implied by the gamma and offset, where the linear segment meets the power function with
//! the same value and slope. Computed in `double` (with the platform's `pow`) and rounded to
//! `float` once.
//!
//! Port of `src/OpenColorIO/ops/gamma/GammaOpUtils.h` and `GammaOpUtils.cpp` @ v2.5.2.

use super::gamma_op_data::Params;
use crate::math_utils::std_max;

/// The coefficients of one channel of a moncurve renderer.
///
/// Port of `RendererParams` (src/OpenColorIO/ops/gamma/GammaOpUtils.h:17-33 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RendererParams {
    /// The exponent.
    pub gamma: f32,
    /// The offset added after scaling (forward) or subtracted (reverse).
    pub offset: f32,
    /// The end of the linear segment.
    pub break_pnt: f32,
    /// The slope of the linear segment.
    pub slope: f32,
    /// The scale applied before (forward) or after (reverse) the power.
    pub scale: f32,
}

impl Default for RendererParams {
    /// Port of `RendererParams::RendererParams` (src/OpenColorIO/ops/gamma/GammaOpUtils.h:19-25
    /// @ v2.5.2).
    fn default() -> Self {
        RendererParams {
            gamma: 1.0,
            offset: 0.0,
            break_pnt: 0.0,
            slope: 1.0,
            scale: 1.0,
        }
    }
}

// The moncurve model would get a div by 0 error with gain=1, offset=0, so the values need to
// get fudged slightly. We do that here rather than during construction or validation so that
// the object may contain the neat looking values since these are what would get written to a
// ctf file (GammaOpUtils.cpp:24-30).
const EPS: f64 = 1e-6;

/// Port of `monCurveGammaFwd` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:32-35 @ v2.5.2).
fn mon_curve_gamma_fwd(p: &Params) -> f64 {
    std_max(p[0], 1.0 + EPS)
}

/// Port of `monCurveOffsetFwd` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:37-41 @ v2.5.2).
fn mon_curve_offset_fwd(p: &Params) -> f64 {
    let offset = std_max(p[1], EPS);
    offset / (1.0 + offset)
}

/// Break point between the linear and power functions.
///
/// Port of `monCurveBreakFwd` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:43-49 @ v2.5.2).
fn mon_curve_break_fwd(p: &Params) -> f64 {
    let gamma = std_max(p[0], 1.0 + EPS);
    let offset = std_max(p[1], EPS);
    offset / (gamma - 1.0)
}

/// Slope of the linear segment.
///
/// Port of `monCurveSlopeFwd` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:51-59 @ v2.5.2).
fn mon_curve_slope_fwd(p: &Params) -> f64 {
    let gamma = std_max(p[0], 1.0 + EPS);
    let offset = std_max(p[1], EPS);
    let a = (gamma - 1.0) / offset;
    let b = offset * gamma / ((gamma - 1.0) * (1.0 + offset));
    a * b.powf(gamma)
}

/// This just rearranges the equation a little so we can get by with a single multiply rather
/// than two.
///
/// Port of `monCurveScaleFwd` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:61-67 @ v2.5.2).
fn mon_curve_scale_fwd(p: &Params) -> f64 {
    let offset = std_max(p[1], EPS);
    1.0 / (1.0 + offset)
}

/// Port of `monCurveGammaRev` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:71-74 @ v2.5.2).
fn mon_curve_gamma_rev(p: &Params) -> f64 {
    1.0 / std_max(p[0], 1.0 + EPS)
}

/// Port of `monCurveOffsetRev` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:76-79 @ v2.5.2).
fn mon_curve_offset_rev(p: &Params) -> f64 {
    std_max(p[1], EPS)
}

/// Port of `monCurveBreakRev` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:81-88 @ v2.5.2).
fn mon_curve_break_rev(p: &Params) -> f64 {
    let gamma = std_max(p[0], 1.0 + EPS);
    let offset = std_max(p[1], EPS);
    let a = offset * gamma;
    let b = (gamma - 1.0) * (1.0 + offset);
    (a / b).powf(gamma)
}

/// Port of `monCurveSlopeRev` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:90-97 @ v2.5.2).
fn mon_curve_slope_rev(p: &Params) -> f64 {
    let gamma = std_max(p[0], 1.0 + EPS);
    let offset = std_max(p[1], EPS);
    let a = (gamma - 1.0) / offset;
    let b = (1.0 + offset) / gamma;
    a.powf(gamma - 1.0) * b.powf(gamma)
}

/// Port of `monCurveScaleRev` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:99-103 @ v2.5.2).
fn mon_curve_scale_rev(p: &Params) -> f64 {
    let offset = std_max(p[1], EPS);
    1.0 + offset
}

/// The coefficients of the forward monitor curve.
///
/// Port of `ComputeParamsFwd` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:109-117 @ v2.5.2).
pub fn compute_params_fwd(g_params: &Params) -> RendererParams {
    RendererParams {
        gamma: mon_curve_gamma_fwd(g_params) as f32,
        offset: mon_curve_offset_fwd(g_params) as f32,
        break_pnt: mon_curve_break_fwd(g_params) as f32,
        slope: mon_curve_slope_fwd(g_params) as f32,
        scale: mon_curve_scale_fwd(g_params) as f32,
    }
}

/// The coefficients of the reverse monitor curve.
///
/// Port of `ComputeParamsRev` (src/OpenColorIO/ops/gamma/GammaOpUtils.cpp:119-127 @ v2.5.2).
pub fn compute_params_rev(g_params: &Params) -> RendererParams {
    RendererParams {
        gamma: mon_curve_gamma_rev(g_params) as f32,
        offset: mon_curve_offset_rev(g_params) as f32,
        break_pnt: mon_curve_break_rev(g_params) as f32,
        slope: mon_curve_slope_rev(g_params) as f32,
        scale: mon_curve_scale_rev(g_params) as f32,
    }
}

#[cfg(test)]
#[path = "gamma_op_utils_tests.rs"]
mod tests;
