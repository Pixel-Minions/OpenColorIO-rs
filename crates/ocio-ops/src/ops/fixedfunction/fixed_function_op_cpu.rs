// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction op's CPU renderers: a port of
//! `src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp` @ v2.5.2.
//!
//! So far the ACES 1.x styles: the red modifiers 0.3 and 1.0, the glows 0.3 and 1.0, the dark
//! to dim surround 1.0 and the gamut compression 1.3, forward and inverse (chunk 2.3b); the
//! Rec.2100 surround, RGB to and from HSV, and XYZ to and from xyY, u'v'Y and CIELUV (2.3c1);
//! RGB to and from the three HSYs (2.3c2); the gamma-log and double-log curves, both ways
//! (2.3d1); ACES 2.0's RGB to and from JMh, and its tone scale and chroma compression (2.4e1).
//! The other styles' renderers come with chunks 2.3d2 (PQ) and 2.4e2 (ACES 2.0's output
//! transform and gamut compression); until then [`get_fixed_function_cpu_renderer`] refuses
//! them ([`not_ported`]).
//!
//! The renderers work in place and never write alpha, which upstream copies (`out[3] =
//! in[3]`), nor a channel upstream leaves as it was.
//!
//! # Operand order
//!
//! Where two NaNs of different payloads can meet, the result is the first operand's
//! (`CLAUDE.md`, "NaN operand order"), so the port pins the order both wheels' machine code
//! uses ([`sse_add`], [`sse_sub`], [`sse_mul`]). Among the ACES 1.x renderers only the glow
//! mixes input channels into a value that reaches the output: its luma-chroma value and the
//! final products (see [`rgb_to_yc`] and [`RendererAcesGlow03Fwd`]). The red modifier's hue
//! weight is 0 whenever an input is NaN, so a NaN pixel goes through it unchanged; the dark to
//! dim's luminance is never NaN (`std::max(minLum, NaN)` is `minLum`), and the gamut
//! compression's achromatic value is red's NaN or no NaN, which its arithmetic propagates
//! whatever the order. Every other NaN inside is the default NaN, whose bits don't depend on
//! the order.
//!
//! The Rec.2100 surround, the HSYs and the CIE conversions mix the channels throughout; each
//! documents the orders MSVC and GCC chose, which often differ (`cfg(target_os)`). LLVM
//! rewrites `a - c * x` as `a + (-c) * x` and may then swap the addition's operands, so those
//! subtractions are [`sse_sub`]. RGB to HSV's extremes carry only red's NaN
//! (`std::min(NaN, x)` is `NaN` only for a NaN first operand), and HSV to RGB's `Clamp` turns
//! a NaN hue or saturation into 0, so neither meets two NaNs in an operation whose operands
//! the compilers can swap.
//!
//! The gamma-log and double-log curves work on one channel at a time, so a NaN pixel meets
//! only NaN parameters (W0002); they follow both wheels' order anyway, and the gamma-log
//! restores the sign per platform (I-83, [`times_copysign_one`]).

use std::sync::Arc;

use super::aces2::common::{
    ChromaCompressParams, JMhParams, SharedCompressionParameters, ToneScaleParams, from_degrees,
    to_degrees, to_radians,
};
use super::aces2::transform::{
    chroma_compress_fwd, chroma_compress_inv, chroma_compress_norm, init_chroma_compress_params,
    init_jmh_params, init_shared_compression_params, init_tone_scale_params, jmh_to_rgb,
    resolve_compression_params, rgb_to_jmh, tonescale_fwd, tonescale_inv,
};
use super::fixed_function_op_data::{FixedFunctionOpData, FixedFunctionOpStyle, SHORT_PARAMS};
use crate::bit_depth_utils::clamp_macro;
use crate::exception::{Exception, Result};
use crate::math_utils::{clamp, sse_add, sse_cvttps_epi32, sse_mul, sse_sub, std_max, std_min};
use crate::op::CpuOp;
use crate::transforms::builtins::color_matrix_helpers::{
    Chromaticities, Primaries, aces_ap0, aces_ap1,
};

/// The error for a style whose renderer isn't ported yet.
pub fn not_ported(style: FixedFunctionOpStyle) -> Exception {
    Exception::new(format!(
        "FixedFunctionOp: the CPU renderer of the style '{}' is not ported yet (Phase 2, WP \
         2.3 and 2.4).",
        style.to_str(true)
    ))
}

/// The saturation measure, computed in a safe manner: the numerator is clamped to prevent
/// problems from negative values, the denominator is clamped higher to prevent dark noise from
/// being classified as having high saturation.
///
/// Port of `CalcSatWeight` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:483-495 @
/// v2.5.2).
#[inline]
fn calc_sat_weight(red: f32, grn: f32, blu: f32, noise_limit: f32) -> f32 {
    let min_val = std_min(red, std_min(grn, blu));
    let max_val = std_max(red, std_max(grn, blu));

    (std_max(1e-10f32, max_val) - std_max(1e-10f32, min_val)) / std_max(noise_limit, max_val)
}

/// The coefficients of a quadratic B-spline basis function (all coefs taken from the ACES ctl
/// code on github), FixedFunctionOpCPU.cpp:539-543 @ v2.5.2.
const M: [[f32; 4]; 4] = [
    [0.25, 0.00, 0.00, 0.00],
    [-0.75, 0.75, 0.75, 0.25],
    [0.75, -1.50, 0.00, 1.00],
    [-0.25, 0.75, -0.75, 0.25],
];

/// The hue weight of the red modifiers: a quadratic B-spline of the hue, centred on red, of
/// width `4 / inv_width` radians; 0 outside it. A NaN or infinite hue coordinate gives index
/// `INT_MIN` (`(int)` is `cvttss2si`), so 0.
///
/// Port of `CalcHueWeight` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:510-556 @
/// v2.5.2).
#[inline]
fn calc_hue_weight(red: f32, grn: f32, blu: f32, inv_width: f32) -> f32 {
    // Convert RGB to Yab (luma/chroma).
    let a = 2.0f32 * red - (grn + blu);
    const SQRT3: f32 = 1.7320508075688772;
    let b = SQRT3 * (grn - blu);

    let hue = b.atan2(a);

    // NB: The code in RedMod03 apply() assumes that in the range of the modification window
    // that red will be the largest channel. The center and width must be chosen to maintain
    // this. For this version, center = 0, so centering the hue is a no-op.

    // Determine normalized input coords to B-spline.
    let knot_coord = hue * inv_width + 2.0f32;
    let j = sse_cvttps_epi32(knot_coord); // index

    // Hue is in range of the window, calculate weight.
    let mut f_h = 0.0f32;
    if (0..4).contains(&j) {
        let t = knot_coord - j as f32; // fractional component

        // Calculate quadratic B-spline weighting function.
        let coefs = &M[j as usize];
        f_h = coefs[3] + t * (coefs[2] + t * (coefs[1] + t * coefs[0]));
    }

    f_h
}

/// The red modifier 0.3 (ACES 0.3/0.7): red is moved towards a pivot by the hue and
/// saturation weights, and the middle channel moves with it to keep the hue.
///
/// Port of `Renderer_ACES_RedMod03_Fwd` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:21-35, 497-508, 558-607 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesRedMod03Fwd {
    /// `m_1minusScale`: `1 - scale`, from the original ctl code.
    one_minus_scale: f32,
    /// `m_pivot`: the offset is applied to unnormalized input values.
    pivot: f32,
    /// `m_inv_width`: `4 / (width * pi / 180)` for a hue region of 120 degrees.
    inv_width: f32,
}

/// `m_noiseLimit` of the red modifiers and the glows (FixedFunctionOpCPU.cpp:34, 58, 80 @
/// v2.5.2).
const NOISE_LIMIT: f32 = 1e-2;

impl RendererAcesRedMod03Fwd {
    /// Port of `Renderer_ACES_RedMod03_Fwd::Renderer_ACES_RedMod03_Fwd`
    /// (FixedFunctionOpCPU.cpp:497-508 @ v2.5.2).
    pub fn new() -> Self {
        RendererAcesRedMod03Fwd {
            one_minus_scale: 1.0f32 - 0.85f32,
            pivot: 0.03,
            inv_width: 1.9098593171027443,
        }
    }
}

impl Default for RendererAcesRedMod03Fwd {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuOp for RendererAcesRedMod03Fwd {
    /// Port of `Renderer_ACES_RedMod03_Fwd::apply` (FixedFunctionOpCPU.cpp:558-607 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            let f_h = calc_hue_weight(red, grn, blu, self.inv_width);

            // Hue is in range of the window, apply mod.
            if f_h > 0.0 {
                let f_s = calc_sat_weight(red, grn, blu, NOISE_LIMIT);

                // Apply red modifier. NB: Red is still at inScale.
                let new_red = red + f_h * f_s * (self.pivot - red) * self.one_minus_scale;

                // Restore hue.
                if grn >= blu {
                    // red >= grn >= blu
                    let hue_fac = (grn - blu) / std_max(1e-10f32, red - blu);
                    pixel[1] = hue_fac * (new_red - blu) + blu;
                } else {
                    // red >= blu >= grn
                    let hue_fac = (blu - grn) / std_max(1e-10f32, red - grn);
                    pixel[2] = hue_fac * (new_red - grn) + grn;
                }

                pixel[0] = new_red;
            }
        }
    }
}

/// The inverse of the red modifier 0.3: red solves the forward's quadratic.
///
/// Port of `Renderer_ACES_RedMod03_Inv` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:37-43, 609-659 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererAcesRedMod03Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesRedMod03Fwd,
}

impl CpuOp for RendererAcesRedMod03Inv {
    /// Port of `Renderer_ACES_RedMod03_Inv::apply` (FixedFunctionOpCPU.cpp:614-659 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let RendererAcesRedMod03Fwd {
            one_minus_scale,
            pivot,
            inv_width,
        } = self.fwd;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            let f_h = calc_hue_weight(red, grn, blu, inv_width);
            if f_h > 0.0 {
                let min_chan = if grn < blu { grn } else { blu };

                let a = f_h * one_minus_scale - 1.0f32;
                let b = red - f_h * (pivot + min_chan) * one_minus_scale;
                let c = f_h * pivot * min_chan * one_minus_scale;

                let new_red = (-b - (b * b - 4.0f32 * a * c).sqrt()) / (2.0f32 * a);

                // Restore hue.
                if grn >= blu {
                    // red >= grn >= blu
                    let hue_fac = (grn - blu) / std_max(1e-10f32, red - blu);
                    pixel[1] = hue_fac * (new_red - blu) + blu;
                } else {
                    // red >= blu >= grn
                    let hue_fac = (blu - grn) / std_max(1e-10f32, red - grn);
                    pixel[2] = hue_fac * (new_red - grn) + grn;
                }

                pixel[0] = new_red;
            }
        }
    }
}

/// The red modifier 1.0 (ACES 1.0): red alone is moved towards a pivot.
///
/// Port of `Renderer_ACES_RedMod10_Fwd` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:45-59, 661-709 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesRedMod10Fwd {
    /// `m_1minusScale`: `1 - scale`, from the original ctl code.
    one_minus_scale: f32,
    /// `m_pivot`: the offset is applied to unnormalized input values.
    pivot: f32,
    /// `m_inv_width`: `4 / (width * pi / 180)` for a hue region of 135 degrees.
    inv_width: f32,
}

impl RendererAcesRedMod10Fwd {
    /// Port of `Renderer_ACES_RedMod10_Fwd::Renderer_ACES_RedMod10_Fwd`
    /// (FixedFunctionOpCPU.cpp:661-672 @ v2.5.2).
    pub fn new() -> Self {
        RendererAcesRedMod10Fwd {
            one_minus_scale: 1.0f32 - 0.82f32,
            pivot: 0.03,
            inv_width: 1.6976527263135504,
        }
    }
}

impl Default for RendererAcesRedMod10Fwd {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuOp for RendererAcesRedMod10Fwd {
    /// Port of `Renderer_ACES_RedMod10_Fwd::apply` (FixedFunctionOpCPU.cpp:674-709 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            let f_h = calc_hue_weight(red, grn, blu, self.inv_width);

            // Hue is in range of the window, apply mod.
            if f_h > 0.0 {
                let f_s = calc_sat_weight(red, grn, blu, NOISE_LIMIT);

                // Apply red modifier. NB: Red is still at inScale.
                pixel[0] = red + f_h * f_s * (self.pivot - red) * self.one_minus_scale;
            }
        }
    }
}

/// The inverse of the red modifier 1.0.
///
/// Port of `Renderer_ACES_RedMod10_Inv` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:61-67, 711-748 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererAcesRedMod10Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesRedMod10Fwd,
}

impl CpuOp for RendererAcesRedMod10Inv {
    /// Port of `Renderer_ACES_RedMod10_Inv::apply` (FixedFunctionOpCPU.cpp:716-748 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let RendererAcesRedMod10Fwd {
            one_minus_scale,
            pivot,
            inv_width,
        } = self.fwd;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            let f_h = calc_hue_weight(red, grn, blu, inv_width);
            if f_h > 0.0 {
                let min_chan = if grn < blu { grn } else { blu };

                let a = f_h * one_minus_scale - 1.0f32;
                let b = red - f_h * (pivot + min_chan) * one_minus_scale;
                let c = f_h * pivot * min_chan * one_minus_scale;

                pixel[0] = (-b - (b * b - 4.0f32 * a * c).sqrt()) / (2.0f32 * a);
            }
        }
    }
}

/// The YC (luma + chroma factor) of RGB:
/// `(blu + grn + red + 1.75 * sqrt(blu*(blu-grn) + grn*(grn-red) + red*(red-blu))) / 3`.
///
/// Where NaNs of different payloads meet, the order is the wheels' (FixedFunctionOpCPU.cpp:
/// 759-766 inlined in `Renderer_ACES_Glow03_Fwd::apply` and `_Inv::apply`, Windows
/// `0x18018b37e`, `0x18018b3f0`; Linux `0x355195`, `0x3551c9`): MSVC adds the green term to
/// the blue one, GCC the blue term to the green one, then both add the red term; and both
/// add the sum of the channels to the chroma term (`1.75 * chroma + ((blu + grn) + red)`).
/// The products' operands can't hold two different NaNs (`blu - grn` is a NaN only where
/// `grn` or `blu` is, and then it is `blu`'s NaN when `blu` is one).
///
/// Port of `rgbToYC` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:759-766 @
/// v2.5.2).
#[inline]
fn rgb_to_yc(red: f32, grn: f32, blu: f32) -> f32 {
    // Convert RGB to YC (luma + chroma factor).
    const YC_RADIUS_WEIGHT: f32 = 1.75;
    let blu_term = blu * (blu - grn);
    let grn_term = grn * (grn - red);
    let red_term = red * (red - blu);
    #[cfg(target_os = "windows")]
    let sum = sse_add(sse_add(grn_term, blu_term), red_term);
    #[cfg(target_os = "linux")]
    let sum = sse_add(sse_add(blu_term, grn_term), red_term);
    let chroma = sum.sqrt();
    sse_add(YC_RADIUS_WEIGHT * chroma, sse_add(sse_add(blu, grn), red)) / 3.0f32
}

/// The sigmoid shaper of the glow's saturation.
///
/// Port of `SigmoidShaper` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:768-775
/// @ v2.5.2).
#[inline]
fn sigmoid_shaper(sat: f32) -> f32 {
    let x = (sat - 0.4f32) * 5.0f32;
    let sign = 1.0f32.copysign(x);
    let t = std_max(0.0f32, 1.0f32 - 0.5f32 * sign * x);
    (1.0f32 + sign * (1.0f32 - t * t)) * 0.5f32
}

/// The glow's factor times each channel. Where the factor and the channel are NaNs of
/// different payloads, the order is the wheels': the factor first for red and green on both
/// platforms and for blue on Windows, blue first on Linux (`Renderer_ACES_Glow03_Fwd::apply`,
/// Windows `0x18018b4c5`-`0x18018b4d4`, Linux `0x35513f` and `0x355130`; `_Inv::apply`, Windows
/// `0x18018b79a`-`0x18018b7b5`, Linux `0x3541f0` and `0x3541e1`).
#[inline]
fn scale_rgb(pixel: &mut [f32; 4], factor: f32) {
    pixel[0] = sse_mul(factor, pixel[0]);
    pixel[1] = sse_mul(factor, pixel[1]);
    #[cfg(target_os = "windows")]
    {
        pixel[2] = sse_mul(factor, pixel[2]);
    }
    #[cfg(target_os = "linux")]
    {
        pixel[2] = sse_mul(pixel[2], factor);
    }
}

/// The glow (ACES 0.3/0.7 and, with other constants, 1.0): dark saturated colors are
/// brightened.
///
/// Port of `Renderer_ACES_Glow03_Fwd` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:69-81, 750-757, 777-824 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesGlow03Fwd {
    /// `m_glowGain`.
    glow_gain: f32,
    /// `m_glowMid`.
    glow_mid: f32,
}

impl RendererAcesGlow03Fwd {
    /// Port of `Renderer_ACES_Glow03_Fwd::Renderer_ACES_Glow03_Fwd`
    /// (FixedFunctionOpCPU.cpp:750-757 @ v2.5.2).
    pub fn new(glow_gain: f32, glow_mid: f32) -> Self {
        RendererAcesGlow03Fwd {
            glow_gain,
            glow_mid,
        }
    }
}

impl CpuOp for RendererAcesGlow03Fwd {
    /// Port of `Renderer_ACES_Glow03_Fwd::apply` (FixedFunctionOpCPU.cpp:777-824 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            // NB: YC is at inScale.
            let yc = rgb_to_yc(red, grn, blu);

            let sat = calc_sat_weight(red, grn, blu, NOISE_LIMIT);

            let s = sigmoid_shaper(sat);

            let glow_gain = self.glow_gain * s;
            let glow_mid = self.glow_mid;

            // Apply FwdGlow.
            let glow_gain_out = if yc >= glow_mid * 2.0f32 {
                0.0f32
            } else if yc <= glow_mid * 2.0f32 / 3.0f32 {
                glow_gain
            } else {
                glow_gain * (glow_mid / yc - 0.5f32)
            };

            // Calculate glow factor.
            let added_glow = 1.0f32 + glow_gain_out;

            scale_rgb(pixel, added_glow);
        }
    }
}

/// The inverse of the glow.
///
/// Port of `Renderer_ACES_Glow03_Inv` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:83-89, 826-880 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesGlow03Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesGlow03Fwd,
}

impl RendererAcesGlow03Inv {
    /// Port of `Renderer_ACES_Glow03_Inv::Renderer_ACES_Glow03_Inv`
    /// (FixedFunctionOpCPU.cpp:826-831 @ v2.5.2).
    pub fn new(glow_gain: f32, glow_mid: f32) -> Self {
        RendererAcesGlow03Inv {
            fwd: RendererAcesGlow03Fwd::new(glow_gain, glow_mid),
        }
    }
}

impl CpuOp for RendererAcesGlow03Inv {
    /// Port of `Renderer_ACES_Glow03_Inv::apply` (FixedFunctionOpCPU.cpp:833-880 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            // NB: YC is at inScale.
            let yc = rgb_to_yc(red, grn, blu);

            let sat = calc_sat_weight(red, grn, blu, NOISE_LIMIT);

            let s = sigmoid_shaper(sat);

            let glow_gain = self.fwd.glow_gain * s;
            let glow_mid = self.fwd.glow_mid;

            // Apply InvGlow.
            let glow_gain_out = if yc >= glow_mid * 2.0f32 {
                0.0f32
            } else if yc <= (1.0f32 + glow_gain) * glow_mid * 2.0f32 / 3.0f32 {
                -glow_gain / (1.0f32 + glow_gain)
            } else {
                glow_gain * (glow_mid / yc - 0.5f32) / (glow_gain * 0.5f32 - 1.0f32)
            };

            // Calculate glow factor.
            let reduced_glow = 1.0f32 + glow_gain_out;

            scale_rgb(pixel, reduced_glow);
        }
    }
}

/// The dark to dim surround correction (ACES 1.0), and with the reciprocal gamma, its
/// inverse: each channel times `Y^(gamma - 1)`, of the AP1 luminance.
///
/// Port of `Renderer_ACES_DarkToDim10_Fwd` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:91-101, 882-922 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesDarkToDim10Fwd {
    /// `m_gamma`: `gamma - 1`, to compute `Y^gamma / Y`.
    gamma: f32,
}

impl RendererAcesDarkToDim10Fwd {
    /// Port of `Renderer_ACES_DarkToDim10_Fwd::Renderer_ACES_DarkToDim10_Fwd`
    /// (FixedFunctionOpCPU.cpp:882-887 @ v2.5.2).
    pub fn new(gamma: f32) -> Self {
        RendererAcesDarkToDim10Fwd {
            gamma: gamma - 1.0f32,
        }
    }
}

impl CpuOp for RendererAcesDarkToDim10Fwd {
    /// Port of `Renderer_ACES_DarkToDim10_Fwd::apply` (FixedFunctionOpCPU.cpp:889-922 @
    /// v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        // With the modest 2% ACES surround, this minLum allows the min/max gain applied to
        // dark colors to be about 0.6 to 1.6.
        const MIN_LUM: f32 = 1e-10;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            // Calculate luminance assuming input is AP1 RGB.
            let y = std_max(
                MIN_LUM,
                0.27222871678091454f32 * red
                    + 0.67408176581114831f32 * grn
                    + 0.053689517407937051f32 * blu,
            );

            let ypow_over_y = y.powf(self.gamma);

            pixel[0] = red * ypow_over_y;
            pixel[1] = grn * ypow_over_y;
            pixel[2] = blu * ypow_over_y;
        }
    }
}

/// The compression of a distance beyond the threshold.
///
/// Port of `compress` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:924-932 @
/// v2.5.2).
fn compress(dist: f32, thr: f32, scale: f32, power: f32) -> f32 {
    // Normalize distance outside threshold by scale factor.
    let nd = (dist - thr) / scale;
    let p = nd.powf(power);

    thr + scale * nd / (1.0f32 + p).powf(1.0f32 / power)
}

/// The inverse of [`compress`]; a distance at or beyond `thr + scale` (the singularity) is
/// kept.
///
/// Port of `uncompress` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:934-950 @
/// v2.5.2).
fn uncompress(dist: f32, thr: f32, scale: f32, power: f32) -> f32 {
    // Avoid singularity
    if dist >= (thr + scale) {
        dist
    } else {
        // Normalize distance outside threshold by scale factor.
        let nd = (dist - thr) / scale;
        let p = nd.powf(power);

        thr + scale * (-(p / (p - 1.0f32))).powf(1.0f32 / power)
    }
}

/// One channel of the gamut compression: its distance from the achromatic axis, compressed
/// or uncompressed by `f` beyond the threshold.
///
/// Upstream's note: strict equality is fine here. For example, consider the RGB `{ 1e-7, 0,
/// -1e-5 }`. This will become a `dist = (1e-7 - -1e-5) / 1e-7 = 101.0`. So, there will
/// definitely be very large dist values. But the compression function is able to handle those
/// since they approach the asymptote. So 101 will become something like 1.12. Then at the
/// other end the B values is reconstructed as `1e-7 - 1.12 * 1e-7 = -1.2e-8`. So it went from
/// -1e-5 to -1.2e-8, but it caused no numerical instability.
///
/// Port of `gamut_comp` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:952-982 @
/// v2.5.2).
#[inline]
fn gamut_comp(
    val: f32,
    ach: f32,
    thr: f32,
    scale: f32,
    power: f32,
    f: fn(f32, f32, f32, f32) -> f32,
) -> f32 {
    if ach == 0.0f32 {
        return 0.0f32;
    }

    // Distance from the achromatic axis, aka inverse RGB ratios
    let dist = (ach - val) / ach.abs();

    // No compression below threshold
    if dist < thr {
        return val;
    }

    // Compress / Uncompress distance with parameterized shaper function.
    let compr_dist = f(dist, thr, scale, power);

    // Recalculate RGB from compressed distance and achromatic.
    ach - compr_dist * ach.abs()
}

/// The parametric gamut compression (ACES 1.3): each channel's distance from the achromatic
/// axis is compressed beyond a threshold, to a limit.
///
/// Port of `Renderer_ACES_GamutComp13_Fwd` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:103-123, 984-1026 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesGamutComp13Fwd {
    /// `m_thrCyan`.
    thr_cyan: f32,
    /// `m_thrMagenta`.
    thr_magenta: f32,
    /// `m_thrYellow`.
    thr_yellow: f32,
    /// `m_power`.
    power: f32,
    /// `m_scaleCyan`.
    scale_cyan: f32,
    /// `m_scaleMagenta`.
    scale_magenta: f32,
    /// `m_scaleYellow`.
    scale_yellow: f32,
}

impl RendererAcesGamutComp13Fwd {
    /// The renderer of `data`'s seven parameters, as `float`s, with the scale factors that
    /// make each compression reach 1 at its limit. [`SHORT_PARAMS`] for fewer parameters,
    /// which upstream reads past (`docs/improvements.md` U-31).
    ///
    /// Port of `Renderer_ACES_GamutComp13_Fwd::Renderer_ACES_GamutComp13_Fwd`
    /// (FixedFunctionOpCPU.cpp:984-1002 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let params = data.params();
        if params.len() < 7 {
            return Err(Exception::new(SHORT_PARAMS));
        }
        let lim_cyan = params[0] as f32;
        let lim_magenta = params[1] as f32;
        let lim_yellow = params[2] as f32;
        let thr_cyan = params[3] as f32;
        let thr_magenta = params[4] as f32;
        let thr_yellow = params[5] as f32;
        let power = params[6] as f32;

        // Precompute scale factor for y = 1 intersect
        let f_scale = |lim: f32, thr: f32| {
            (lim - thr)
                / (((1.0f32 - thr) / (lim - thr)).powf(-power) - 1.0f32).powf(1.0f32 / power)
        };
        Ok(RendererAcesGamutComp13Fwd {
            thr_cyan,
            thr_magenta,
            thr_yellow,
            power,
            scale_cyan: f_scale(lim_cyan, thr_cyan),
            scale_magenta: f_scale(lim_magenta, thr_magenta),
            scale_yellow: f_scale(lim_yellow, thr_yellow),
        })
    }

    /// The forward or inverse renderer's loop, with `f` the shaper.
    fn apply_with(&self, rgba: &mut [f32], f: fn(f32, f32, f32, f32) -> f32) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            // Achromatic axis
            let ach = std_max(red, std_max(grn, blu));

            pixel[0] = gamut_comp(red, ach, self.thr_cyan, self.scale_cyan, self.power, f);
            pixel[1] = gamut_comp(
                grn,
                ach,
                self.thr_magenta,
                self.scale_magenta,
                self.power,
                f,
            );
            pixel[2] = gamut_comp(blu, ach, self.thr_yellow, self.scale_yellow, self.power, f);
        }
    }
}

impl CpuOp for RendererAcesGamutComp13Fwd {
    /// Port of `Renderer_ACES_GamutComp13_Fwd::apply` (FixedFunctionOpCPU.cpp:1004-1026 @
    /// v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        self.apply_with(rgba, compress);
    }
}

/// The inverse of the gamut compression.
///
/// Port of `Renderer_ACES_GamutComp13_Inv` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:125-131, 1028-1055 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesGamutComp13Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesGamutComp13Fwd,
}

impl RendererAcesGamutComp13Inv {
    /// Port of `Renderer_ACES_GamutComp13_Inv::Renderer_ACES_GamutComp13_Inv`
    /// (FixedFunctionOpCPU.cpp:1028-1031 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        Ok(RendererAcesGamutComp13Inv {
            fwd: RendererAcesGamutComp13Fwd::new(data)?,
        })
    }
}

impl CpuOp for RendererAcesGamutComp13Inv {
    /// Port of `Renderer_ACES_GamutComp13_Inv::apply` (FixedFunctionOpCPU.cpp:1033-1055 @
    /// v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        self.fwd.apply_with(rgba, uncompress);
    }
}

/// `(float) params[i]`, or [`SHORT_PARAMS`] where upstream would read past the parameters
/// (U-31).
fn param_f32(data: &FixedFunctionOpData, i: usize) -> Result<f32> {
    data.params()
        .get(i)
        .map(|&v| v as f32)
        .ok_or_else(|| Exception::new(SHORT_PARAMS))
}

/// The primaries of the parameters from `first` on (red, green, blue and white x and y), each
/// narrowed to `float` as the renderers read them, then held in `double` (`Primaries`'
/// `Chromaticities`).
fn primaries_from(data: &FixedFunctionOpData, first: usize) -> Result<Primaries> {
    let c = |i: usize| -> Result<Chromaticities> {
        Ok(Chromaticities::new(
            f64::from(param_f32(data, first + 2 * i)?),
            f64::from(param_f32(data, first + 2 * i + 1)?),
        ))
    };
    Ok(Primaries::new(c(0)?, c(1)?, c(2)?, c(3)?))
}

/// RGB of the parameters' primaries to ACES 2.0's JMh (hue in degrees), or back.
///
/// Port of `Renderer_ACES_RGB_TO_JMh_20` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:153-167, 1168-1241 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesRgbToJmh20 {
    /// `m_fwd`.
    fwd: bool,
    /// `m_p`.
    p: JMhParams,
}

impl RendererAcesRgbToJmh20 {
    /// The model of the parameters' primaries. [`SHORT_PARAMS`] for fewer than eight
    /// parameters (U-31); refused where a matrix is singular.
    ///
    /// Port of `Renderer_ACES_RGB_TO_JMh_20::Renderer_ACES_RGB_TO_JMh_20`
    /// (FixedFunctionOpCPU.cpp:1168-1190 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = FixedFunctionOpStyle::AcesRgbToJmh20 == data.style();
        let primaries = primaries_from(data, 0)?;
        Ok(RendererAcesRgbToJmh20 {
            fwd,
            p: init_jmh_params(&primaries)?,
        })
    }
}

impl CpuOp for RendererAcesRgbToJmh20 {
    /// Port of `Renderer_ACES_RGB_TO_JMh_20::apply`, `fwd` and `inv`
    /// (FixedFunctionOpCPU.cpp:1192-1241 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            if self.fwd {
                let jmh = rgb_to_jmh(&[pixel[0], pixel[1], pixel[2]], &self.p);

                pixel[0] = jmh[0];
                pixel[1] = jmh[1];
                pixel[2] = to_degrees(jmh[2]);
            } else {
                let normalised_hue = from_degrees(pixel[2]);
                let rgb = jmh_to_rgb(&[pixel[0], pixel[1], normalised_hue], &self.p);

                pixel[0] = rgb[0];
                pixel[1] = rgb[1];
                pixel[2] = rgb[2];
            }
        }
    }
}

/// ACES 2.0's tone scale and chroma compression of a JMh (hue in degrees), or back.
///
/// Port of `Renderer_ACES_TONESCALE_COMPRESS_20` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:169-187, 1243-1319 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesTonescaleCompress20 {
    /// `m_fwd`.
    fwd: bool,
    /// `m_p`.
    p: JMhParams,
    /// `m_t`.
    t: ToneScaleParams,
    /// `m_s`.
    s: SharedCompressionParameters,
    /// `m_c`.
    c: ChromaCompressParams,
}

impl RendererAcesTonescaleCompress20 {
    /// The models of AP0 and the reach (AP1), and the parameters for the peak luminance.
    /// [`SHORT_PARAMS`] without the parameter (U-31).
    ///
    /// Port of `Renderer_ACES_TONESCALE_COMPRESS_20::Renderer_ACES_TONESCALE_COMPRESS_20`
    /// (FixedFunctionOpCPU.cpp:1243-1255 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = FixedFunctionOpStyle::AcesTonescaleCompress20Fwd == data.style();

        let peak_luminance = param_f32(data, 0)?;

        let p = init_jmh_params(&aces_ap0::PRIMARIES)?;
        let t = init_tone_scale_params(peak_luminance);
        let reach_gamut = init_jmh_params(&aces_ap1::PRIMARIES)?;
        let s = init_shared_compression_params(peak_luminance, &p, &reach_gamut);
        let c = init_chroma_compress_params(peak_luminance, &t);
        Ok(RendererAcesTonescaleCompress20 { fwd, p, t, s, c })
    }
}

impl CpuOp for RendererAcesTonescaleCompress20 {
    /// Port of `Renderer_ACES_TONESCALE_COMPRESS_20::apply`, `fwd` and `inv`
    /// (FixedFunctionOpCPU.cpp:1257-1319 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let normalised_hue = from_degrees(pixel[2]);
            let h_rad = to_radians(normalised_hue);
            let cos_hr1 = h_rad.cos();
            let sin_hr1 = h_rad.sin();
            let mnorm = chroma_compress_norm(cos_hr1, sin_hr1, self.c.chroma_compress_scale);
            let rp = resolve_compression_params(normalised_hue, &self.s);
            let jmh_in = [pixel[0], pixel[1], normalised_hue];
            let jmh = if self.fwd {
                let j_ts = tonescale_fwd(pixel[0], &self.p, &self.t);
                chroma_compress_fwd(&jmh_in, j_ts, mnorm, &rp, &self.c)
            } else {
                let j = tonescale_inv(pixel[0], &self.p, &self.t);
                chroma_compress_inv(&jmh_in, j, mnorm, &rp, &self.c)
            };

            pixel[0] = jmh[0];
            pixel[1] = jmh[1];
            pixel[2] = to_degrees(jmh[2]);
        }
    }
}

/// The Rec.2100 surround correction: each channel times `Y^(gamma - 1)` of the Rec.2100
/// luminance, mirrored around 0 and limited below; the inverse uses `1 / gamma`.
///
/// Port of `Renderer_REC2100_Surround` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:210-221, 1406-1454 @ v2.5.2).
#[derive(Debug)]
pub struct RendererRec2100Surround {
    /// `m_gamma`: `gamma - 1`, to compute `Y^gamma / Y`.
    gamma: f32,
    /// `m_minLum`.
    min_lum: f32,
}

impl RendererRec2100Surround {
    /// [`SHORT_PARAMS`] without a parameter, which upstream reads past (U-31).
    ///
    /// Port of `Renderer_REC2100_Surround::Renderer_REC2100_Surround`
    /// (FixedFunctionOpCPU.cpp:1406-1417 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = FixedFunctionOpStyle::Rec2100SurroundFwd == data.style();
        let Some(&param) = data.params().first() else {
            return Err(Exception::new(SHORT_PARAMS));
        };
        let mut gamma = param as f32;

        let min_lum = if fwd { 1e-4f32 } else { 1e-4f32.powf(gamma) };

        gamma = if fwd { gamma } else { 1.0f32 / gamma };

        Ok(RendererRec2100Surround {
            gamma: gamma - 1.0f32,
            min_lum,
        })
    }
}

impl CpuOp for RendererRec2100Surround {
    /// The products' order where the factor and a channel are NaNs (only with a NaN parameter,
    /// W0002): the channel first on Windows; on Linux, the factor first for red and green and
    /// blue first (`Renderer_REC2100_Surround::apply`, Windows `0x18018da66`-`0x18018da73`,
    /// Linux `0x3548b8`, `0x3548c4`).
    ///
    /// Port of `Renderer_REC2100_Surround::apply` (FixedFunctionOpCPU.cpp:1419-1454 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            // Calculate luminance assuming input is Rec.2100 RGB.
            let mut y = sse_add(sse_add(0.2627f32 * red, 0.6780f32 * grn), 0.0593f32 * blu);

            // Mirror the function around the origin.
            y = y.abs();

            // Since the slope may approach infinity as Y approaches 0, limit the min value
            // to avoid gaining up the RGB values (which may not be as close to 0).
            y = std_max(self.min_lum, y);

            let ypow_over_y = y.powf(self.gamma);

            #[cfg(target_os = "windows")]
            {
                pixel[0] = sse_mul(red, ypow_over_y);
                pixel[1] = sse_mul(grn, ypow_over_y);
                pixel[2] = sse_mul(blu, ypow_over_y);
            }
            #[cfg(target_os = "linux")]
            {
                pixel[0] = sse_mul(ypow_over_y, red);
                pixel[1] = sse_mul(ypow_over_y, grn);
                pixel[2] = sse_mul(blu, ypow_over_y);
            }
        }
    }
}

/// RGB to HSV, for extended range values: if RGB are non-negative or all negative, S is on
/// [0,1]; if RGB are a mix of positive and negative, S is on [1,2]. H is [0,1] for all inputs,
/// with 1 meaning 360 degrees. For RGB on [0,1], the classic HSV formula.
///
/// Port of `Renderer_RGB_TO_HSV` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:
/// 223-230, 1456-1534 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererRgbToHsv;

impl CpuOp for RendererRgbToHsv {
    /// Port of `Renderer_RGB_TO_HSV::apply` (FixedFunctionOpCPU.cpp:1466-1534 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let red = pixel[0];
            let grn = pixel[1];
            let blu = pixel[2];

            let rgb_min = std_min(std_min(red, grn), blu);
            let rgb_max = std_max(std_max(red, grn), blu);

            let mut val = rgb_max;
            let mut sat = 0.0f32;
            let mut hue = 0.0f32;

            if rgb_min != rgb_max {
                // Sat
                let delta = rgb_max - rgb_min;
                if rgb_max != 0.0 {
                    sat = delta / rgb_max;
                }

                // Hue
                if red == rgb_max {
                    hue = (grn - blu) / delta;
                } else if grn == rgb_max {
                    hue = 2.0f32 + (blu - red) / delta;
                } else {
                    hue = 4.0f32 + (red - grn) / delta;
                }
                if hue < 0.0 {
                    hue += 6.0f32;
                }
                hue *= 0.16666666666666666f32;
            }

            // Handle extended range inputs.
            if rgb_min < 0.0 {
                val += rgb_min;
            }
            if -rgb_min > rgb_max {
                // GCC computes `-(rgb_max - rgb_min) / rgb_min`, which flips the sign of the NaN
                // that infinities of one sign give (`Renderer_RGB_TO_HSV::apply`, Linux
                // `0x35398c`); MSVC divides by `-rgb_min`.
                #[cfg(target_os = "windows")]
                {
                    sat = (rgb_max - rgb_min) / -rgb_min;
                }
                #[cfg(target_os = "linux")]
                {
                    sat = -(rgb_max - rgb_min) / rgb_min;
                }
            }

            pixel[0] = hue;
            pixel[1] = sat;
            pixel[2] = val;
        }
    }
}

/// HSV to RGB, for extended range values: H is nominally on [0,1], but values outside are
/// wrapped back into range; S is nominally on [0,1] for non-negative RGB but may extend up to
/// 2, and values outside [0, 1.999] are clamped.
///
/// Port of `Renderer_HSV_TO_RGB` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:
/// 232-239, 1536-1589 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererHsvToRgb;

impl CpuOp for RendererHsvToRgb {
    /// Port of `Renderer_HSV_TO_RGB::apply` (FixedFunctionOpCPU.cpp:1548-1589 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        const MAX_SAT: f32 = 1.999;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let hue = (pixel[0] - pixel[0].floor()) * 6.0f32;
            let sat = clamp(pixel[1], 0.0f32, MAX_SAT);
            let val = pixel[2];

            let red = clamp((hue - 3.0f32).abs() - 1.0f32, 0.0f32, 1.0f32);
            let grn = clamp(2.0f32 - (hue - 2.0f32).abs(), 0.0f32, 1.0f32);
            let blu = clamp(2.0f32 - (hue - 4.0f32).abs(), 0.0f32, 1.0f32);

            let mut rgb_max = val;
            let mut rgb_min = val * (1.0f32 - sat);

            // Handle extended range inputs.
            if sat > 1.0 {
                rgb_min = val * (1.0f32 - sat) / (2.0f32 - sat);
                rgb_max = val - rgb_min;
            }
            if val < 0.0 {
                rgb_min = val / (2.0f32 - sat);
                rgb_max = val - rgb_min;
            }

            let delta = rgb_max - rgb_min;
            pixel[0] = red * delta + rgb_min;
            pixel[1] = grn * delta + rgb_min;
            pixel[2] = blu * delta + rgb_min;
        }
    }
}

/// XYZ to xyY.
///
/// Where NaNs meet: MSVC sums `(Y + X) + Z` and multiplies `d * X`, `d * Y`; GCC `(X + Y) + Z` and
/// `X * d`, `Y * d` (`Renderer_XYZ_TO_xyY::apply`, Windows `0x18018e1f9`, Linux `0x353a7e`).
///
/// Port of `Renderer_XYZ_TO_xyY` (FixedFunctionOpCPU.cpp:295-302, 1825-1854 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererXyzToXyy;

impl CpuOp for RendererXyzToXyy {
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let x_ = pixel[0];
            let y_ = pixel[1];
            let z_ = pixel[2];

            #[cfg(target_os = "windows")]
            let mut d = sse_add(sse_add(y_, x_), z_);
            #[cfg(target_os = "linux")]
            let mut d = sse_add(sse_add(x_, y_), z_);
            d = if d == 0.0 { 0.0 } else { 1.0f32 / d };
            #[cfg(target_os = "windows")]
            let (x, y) = (sse_mul(d, x_), sse_mul(d, y_));
            #[cfg(target_os = "linux")]
            let (x, y) = (sse_mul(x_, d), sse_mul(y_, d));

            pixel[0] = x;
            pixel[1] = y;
            pixel[2] = y_;
        }
    }
}

/// xyY to XYZ.
///
/// Where NaNs meet: `X` is `(Y * x) * d` with MSVC and `(x * Y) * d` with GCC; `Z` is `((1 - x - y)
/// * Y) * d` with both (`Renderer_xyY_TO_XYZ::apply`, Windows `0x18018e703`, Linux `0x353b40`).
///
/// Port of `Renderer_xyY_TO_XYZ` (FixedFunctionOpCPU.cpp:304-311, 1856-1884 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererXyyToXyz;

impl CpuOp for RendererXyyToXyz {
    /// Port of `Renderer_xyY_TO_XYZ::apply` (FixedFunctionOpCPU.cpp:1861-1884 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let x = pixel[0];
            let y = pixel[1];
            let y_ = pixel[2];

            let d = if y == 0.0 { 0.0f32 } else { 1.0f32 / y };
            #[cfg(target_os = "windows")]
            let x_ = sse_mul(sse_mul(y_, x), d);
            #[cfg(target_os = "linux")]
            let x_ = sse_mul(sse_mul(x, y_), d);
            let z_ = sse_mul(sse_mul(1.0f32 - x - y, y_), d);

            pixel[0] = x_;
            pixel[1] = y_;
            pixel[2] = z_;
        }
    }
}

/// XYZ to u'v'Y.
///
/// Where NaNs meet, both wheels: `d` is `(15 * Y + X) + 3 * Z`, and `u`, `v` are `(4 * X) * d`,
/// `(9 * Y) * d` (`Renderer_XYZ_TO_uvY::apply`, Windows `0x18018df71`, Linux `0x353bd7`).
///
/// Port of `Renderer_XYZ_TO_uvY` (FixedFunctionOpCPU.cpp:313-320, 1886-1917 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererXyzToUvy;

impl CpuOp for RendererXyzToUvy {
    /// Port of `Renderer_XYZ_TO_uvY::apply` (FixedFunctionOpCPU.cpp:1891-1917 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let x_ = pixel[0];
            let y_ = pixel[1];
            let z_ = pixel[2];

            let mut d = sse_add(sse_add(15.0f32 * y_, x_), 3.0f32 * z_);
            d = if d == 0.0 { 0.0 } else { 1.0f32 / d };
            let u = sse_mul(4.0f32 * x_, d);
            let v = sse_mul(9.0f32 * y_, d);

            pixel[0] = u;
            pixel[1] = v;
            pixel[2] = y_;
        }
    }
}

/// u'v'Y to XYZ.
///
/// Where NaNs meet, both wheels: `X` is `((9/4 * Y) * u) * d` and `Z` is `((4 - u - 20/3 * v) *
/// (3/4 * Y)) * d` (`Renderer_uvY_TO_XYZ::apply`, Windows `0x18018e457`, Linux `0x353ca4`), the
/// subtraction kept as one ([`sse_sub`]).
///
/// Port of `Renderer_uvY_TO_XYZ` (FixedFunctionOpCPU.cpp:322-329, 1919-1949 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererUvyToXyz;

impl CpuOp for RendererUvyToXyz {
    /// Port of `Renderer_uvY_TO_XYZ::apply` (FixedFunctionOpCPU.cpp:1924-1949 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let u = pixel[0];
            let v = pixel[1];
            let y_ = pixel[2];

            let d = if v == 0.0 { 0.0f32 } else { 1.0f32 / v };
            let x_ = sse_mul(sse_mul((9.0f32 / 4.0f32) * y_, u), d);
            let z_ = sse_mul(
                sse_mul(
                    sse_sub(4.0f32 - u, 6.666666666666667f32 * v),
                    (3.0f32 / 4.0f32) * y_,
                ),
                d,
            );

            pixel[0] = x_;
            pixel[1] = y_;
            pixel[2] = z_;
        }
    }
}

/// XYZ to CIELUV (D65 white).
///
/// Where NaNs meet: `d`, `u` as [`RendererXyzToUvy`]; `v` is `(9 * Y) * d` with MSVC and
/// `d * (9 * Y)` with GCC; `u*` and `v*` are `(u - u'n) * (13 * L*)` with both
/// (`Renderer_XYZ_TO_LUV::apply`, Windows `0x18018ddbd`, Linux `0x35464f`).
///
/// Port of `Renderer_XYZ_TO_LUV` (FixedFunctionOpCPU.cpp:331-338, 1951-1987 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererXyzToLuv;

impl CpuOp for RendererXyzToLuv {
    /// Port of `Renderer_XYZ_TO_LUV::apply` (FixedFunctionOpCPU.cpp:1956-1987 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let x_ = pixel[0];
            let y_ = pixel[1];
            let z_ = pixel[2];

            let mut d = sse_add(sse_add(15.0f32 * y_, x_), 3.0f32 * z_);
            d = if d == 0.0 { 0.0 } else { 1.0f32 / d };
            let u = sse_mul(4.0f32 * x_, d);
            #[cfg(target_os = "windows")]
            let v = sse_mul(9.0f32 * y_, d);
            #[cfg(target_os = "linux")]
            let v = sse_mul(d, 9.0f32 * y_);

            let lstar = if y_ <= 0.008856451679f32 {
                9.0329629629629608f32 * y_
            } else {
                1.16f32 * y_.powf(0.333333333f32) - 0.16f32
            };
            let ustar = sse_mul(u - 0.19783001f32, 13.0f32 * lstar); // D65 white
            let vstar = sse_mul(v - 0.46831999f32, 13.0f32 * lstar); // D65 white

            pixel[0] = lstar;
            pixel[1] = ustar;
            pixel[2] = vstar;
        }
    }
}

/// CIELUV (D65 white) to XYZ.
///
/// Where NaNs meet: `u`, `v` are `d * u*` with MSVC and `u* * d` with GCC; `X` is `((9 * Y) * u) *
/// dd` with both; `Z` is `((12 - 3u - 20v) * Y) * dd` with MSVC and `dd * ((12 - 3u - 20v) * Y)`
/// with GCC (`Renderer_LUV_TO_XYZ::apply`, Windows `0x18018d5ac`, Linux `0x353e2f`).
///
/// Port of `Renderer_LUV_TO_XYZ` (FixedFunctionOpCPU.cpp:340-347, 1989-2026 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererLuvToXyz;

impl CpuOp for RendererLuvToXyz {
    /// Port of `Renderer_LUV_TO_XYZ::apply` (FixedFunctionOpCPU.cpp:1994-2026 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let lstar = pixel[0];
            let ustar = pixel[1];
            let vstar = pixel[2];

            let d = if lstar == 0.0 {
                0.0f32
            } else {
                0.076923076923076927f32 / lstar
            };
            #[cfg(target_os = "windows")]
            let (u, v) = (
                sse_mul(d, ustar) + 0.19783001f32, // D65 white
                sse_mul(d, vstar) + 0.46831999f32, // D65 white
            );
            #[cfg(target_os = "linux")]
            let (u, v) = (
                sse_mul(ustar, d) + 0.19783001f32, // D65 white
                sse_mul(vstar, d) + 0.46831999f32, // D65 white
            );

            let tmp = (lstar + 0.16f32) * 0.86206896551724144f32;
            let y_ = if lstar <= 0.08f32 {
                0.11070564598794539f32 * lstar
            } else {
                tmp * tmp * tmp
            };

            let dd = if v == 0.0 { 0.0f32 } else { 0.25f32 / v };
            let x_ = sse_mul(sse_mul(9.0f32 * y_, u), dd);
            let paren_y = sse_mul(sse_sub(12.0f32 - 3.0f32 * u, 20.0f32 * v), y_);
            #[cfg(target_os = "windows")]
            let z_ = sse_mul(paren_y, dd);
            #[cfg(target_os = "linux")]
            let z_ = sse_mul(dd, paren_y);

            pixel[0] = x_;
            pixel[1] = y_;
            pixel[2] = z_;
        }
    }
}

/// The HSY variants: luma and saturation for linear, log or video encodings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HsyStyle {
    Lin,
    Log,
    Vid,
}

impl HsyStyle {
    /// For [`apply_rgb_to_hsy`], where NaNs of different payloads meet: whether the luma adds
    /// the red term to the green one (else the green to the red), and whether the distance
    /// adds `|r - luma|` to `|g - luma|` (else the reverse). MSVC compiles one function for
    /// the three styles (Windows `0x18018ed10`: green first in the luma, red first in the
    /// distance); GCC a loop per style (`applyRGBToHSY`, Linux `0x358880`: the linear style
    /// red first in the luma and green first in the distance, the log style green first in
    /// both, the video style red first in both).
    fn rgb_to_hsy_orders(self) -> (bool, bool) {
        #[cfg(target_os = "windows")]
        {
            let _ = self;
            (true, false)
        }
        #[cfg(target_os = "linux")]
        {
            match self {
                HsyStyle::Lin => (false, true),
                HsyStyle::Log => (true, true),
                HsyStyle::Vid => (false, false),
            }
        }
    }
}

/// HSY to RGB.
///
/// Where NaNs of different payloads meet, the order is the wheels' machine code's: MSVC
/// compiles one function for the three styles (Windows `0x18018e8e0`), GCC a loop per style
/// (`applyHSYToRGB`, Linux `0x357ce0`), with these differences from upstream's source order:
/// - the luminance of the hue's RGB: MSVC and GCC's log loop add the red term to the green
///   one; GCC's linear and video loops the green to the red;
/// - the scaling by `luma / currY`: MSVC puts the factor first for the three channels; GCC
///   puts blue first, and red and green after the factor (log and video), or first (linear;
///   its distance uses the factor first for red);
/// - the distance: MSVC and GCC's linear loop add red to green, then blue; GCC's log and video
///   loops add red and green, then that to blue;
/// - the linear style's quadratic: see [`hsy_lin_gain`];
/// - the result: MSVC computes `(red - luma) * gainS + luma`; GCC puts the gain first for red
///   and green, and the difference first for blue, except in the log loop.
///
/// Port of `applyHSYToRGB` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:
/// 1591-1681 @ v2.5.2).
fn apply_hsy_to_rgb(rgba: &mut [f32], func_style: HsyStyle) {
    #[cfg(target_os = "windows")]
    let msvc = true;
    #[cfg(target_os = "linux")]
    let msvc = false;
    for pixel in rgba.as_chunks_mut::<4>().0 {
        // Make magenta 0 hue, rather than red.
        let mut hue = pixel[0] - 1.0f32 / 6.0f32;
        let mut sat = pixel[1];
        let luma = pixel[2];

        // Rotate hue 180 deg. for negative luma values.
        hue = if luma < 0.0 { hue + 0.5f32 } else { hue };
        hue = (hue - hue.floor()) * 6.0f32;

        let red = clamp_macro((hue - 3.0f32).abs() - 1.0f32, 0.0f32, 1.0f32);
        let grn = clamp_macro(2.0f32 - (hue - 2.0f32).abs(), 0.0f32, 1.0f32);
        let blu = clamp_macro(2.0f32 - (hue - 4.0f32).abs(), 0.0f32, 1.0f32);

        let (r_term, g_term) = (0.2126f32 * red, 0.7152f32 * grn);
        let rg_term = if msvc || func_style == HsyStyle::Log {
            sse_add(g_term, r_term)
        } else {
            sse_add(r_term, g_term)
        };
        let curr_y = sse_add(rg_term, 0.0722f32 * blu);
        let t = luma / curr_y;

        // `red *= luma / currY` and the others, and the differences from luma.
        let (red, grn, blu, rm_dist) = if msvc {
            let (red, grn, blu) = (sse_mul(t, red), sse_mul(t, grn), sse_mul(t, blu));
            (red, grn, blu, red - luma)
        } else if func_style == HsyStyle::Lin {
            (
                sse_mul(red, t),
                sse_mul(grn, t),
                sse_mul(blu, t),
                sse_mul(t, red) - luma,
            )
        } else {
            let red_dist = if func_style == HsyStyle::Log {
                sse_mul(red, t)
            } else {
                sse_mul(t, red)
            };
            (
                sse_mul(t, red),
                sse_mul(t, grn),
                sse_mul(blu, t),
                red_dist - luma,
            )
        };
        let (rm, gm, bm) = (red - luma, grn - luma, blu - luma);

        let dist_rgb = if msvc || func_style == HsyStyle::Lin {
            sse_add(sse_add(gm.abs(), rm_dist.abs()), bm.abs())
        } else {
            sse_add(bm.abs(), sse_add(rm_dist.abs(), gm.abs()))
        };

        let gain_s = match func_style {
            HsyStyle::Lin => {
                sat /= 1.4f32;
                hsy_lin_gain(red, grn, blu, sat, luma, dist_rgb, msvc)
            }
            HsyStyle::Log => {
                let sat_gain = 4.0f32;
                let curr_sat = sse_mul(dist_rgb, sat_gain);
                sat / std_max(1e-10f32, curr_sat)
            }
            HsyStyle::Vid => {
                let sat_gain = 1.25f32;
                let curr_sat = sse_mul(dist_rgb, sat_gain);
                sat / std_max(1e-10f32, curr_sat)
            }
        };

        if msvc {
            pixel[0] = sse_add(sse_mul(rm, gain_s), luma); // red
            pixel[1] = sse_add(sse_mul(gm, gain_s), luma); // grn
            pixel[2] = sse_add(sse_mul(bm, gain_s), luma); // blu
        } else {
            pixel[0] = sse_add(sse_mul(gain_s, rm), luma); // red
            pixel[1] = sse_add(sse_mul(gain_s, gm), luma); // grn
            pixel[2] = if func_style == HsyStyle::Log {
                sse_add(sse_mul(gain_s, bm), luma)
            } else {
                sse_add(sse_mul(bm, gain_s), luma)
            }; // blu
        }
    }
}

/// The linear HSY's saturation gain, from its quadratic between the low and high
/// saturations. `sat` is already divided by 1.4. In the order of the wheels' machine code
/// where NaNs of different payloads meet (`msvc` for the Windows wheel), which reorders
/// upstream's `-sat * sumRgb + sat * 3 * luma + distRgb` as `sat * 3 * luma - sumRgb * sat +
/// distRgb` (GCC: `sat * sumRgb`), and `-sat * (k + 3 * luma)` as `-sat * (3 * luma + k)` (GCC:
/// `-((3 * luma + k) * sat)`).
///
/// Port of the linear branch of `applyHSYToRGB` (FixedFunctionOpCPU.cpp:1620-1658 @ v2.5.2).
fn hsy_lin_gain(
    red: f32,
    grn: f32,
    blu: f32,
    sat: f32,
    luma: f32,
    dist_rgb: f32,
    msvc: bool,
) -> f32 {
    let sum_rgb = sse_add(sse_add(grn, red), blu);

    let k = 0.15f32;
    let lo_gain = 5.0f32;

    let three_l = luma * 3.0f32;
    let k_3l = three_l + k;
    let sat_sum = if msvc {
        sse_mul(sum_rgb, sat)
    } else {
        sse_mul(sat, sum_rgb)
    };
    let mut tmp = sse_add(sse_sub(sse_mul(sat * 3.0f32, luma), sat_sum), dist_rgb);
    // Don't allow tmp to go negative, which would cause a negative gainS.
    tmp = std_max(1e-6f32, tmp);

    let mut s1 = sse_mul(k_3l, sat) / tmp;
    // Prevent gainS from becoming too extreme.
    s1 = std_min(s1, 50.0f32);

    let dist_lo = sse_mul(dist_rgb, lo_gain);
    let s0 = sat / std_max(1e-10f32, dist_lo);

    let max_lum = 0.01f32;
    let min_lum = max_lum * 0.1f32;
    let alpha = clamp_macro((luma - min_lum) / (max_lum - min_lum), 0.0f32, 1.0f32);

    if alpha == 1.0 {
        s1
    } else if alpha == 0.0 {
        s0
    } else {
        let one_minus_alpha = 1.0f32 - alpha;
        let sum_3l = sse_sub(sum_rgb, three_l);
        let alpha_dist = sse_mul(alpha, dist_rgb);
        let (t5, sat_sum_3l, c) = if msvc {
            (
                sse_mul(one_minus_alpha, dist_lo),
                sse_mul(sum_3l, sat),
                sse_mul(-sat, k_3l),
            )
        } else {
            (
                sse_mul(dist_lo, one_minus_alpha),
                sse_mul(sat, sum_3l),
                -sse_mul(k_3l, sat),
            )
        };
        let (b_head, a) = if msvc {
            (sse_mul(t5, k_3l), sse_mul(t5, sum_3l))
        } else {
            (sse_mul(k_3l, t5), sse_mul(t5, sum_3l))
        };
        let b = sse_sub(sse_add(b_head, alpha_dist), sat_sum_3l);
        let discrim = sse_sub(b * b, sse_mul(a * 4.0f32, c)).sqrt();
        let denom = sse_sub(-discrim, b);
        let two_c = sse_add(c, c);
        let gain_s = two_c / denom;

        if gain_s >= 0.0 {
            gain_s
        } else {
            two_c / sse_add(sse_add(discrim, discrim), denom)
        }
    }
}

/// RGB to HSY. Unlike typical HSV, HSY maps magenta rather than red to a hue of zero (which
/// allows for better placement of red when manipulating curves in a UI).
///
/// Port of `applyRGBToHSY` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:
/// 1683-1763 @ v2.5.2).
fn apply_rgb_to_hsy(rgba: &mut [f32], func_style: HsyStyle) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let red = pixel[0];
        let grn = pixel[1];
        let blu = pixel[2];

        let rgb_min = std_min(std_min(red, grn), blu);
        let rgb_max = std_max(std_max(red, grn), blu);

        // The wheels' orders where NaNs meet: see `HsyStyle::rgb_to_hsy_orders`.
        let (green_first_luma, green_first_dist) = func_style.rgb_to_hsy_orders();
        let (r_term, g_term) = (0.2126f32 * red, 0.7152f32 * grn);
        let rg = if green_first_luma {
            sse_add(g_term, r_term)
        } else {
            sse_add(r_term, g_term)
        };
        let luma = sse_add(rg, 0.0722f32 * blu);

        let rm = red - luma;
        let gm = grn - luma;
        let bm = blu - luma;

        let rgm = if green_first_dist {
            sse_add(gm.abs(), rm.abs())
        } else {
            sse_add(rm.abs(), gm.abs())
        };
        let dist_rgb = sse_add(rgm, bm.abs());

        let sat = match func_style {
            HsyStyle::Lin => {
                let sum_rgb = sse_add(sse_add(red, grn), blu);
                let k = 0.15f32;
                let sat_hi = dist_rgb / std_max(0.07f32 * dist_rgb + 1e-6f32, k + sum_rgb);
                let lo_gain = 5.0f32;
                let sat_lo = dist_rgb * lo_gain;
                let max_lum = 0.01f32;
                let min_lum = max_lum * 0.1f32;
                let alpha = clamp_macro((luma - min_lum) / (max_lum - min_lum), 0.0f32, 1.0f32);
                let sat = sse_add(sse_mul(sat_hi - sat_lo, alpha), sat_lo);
                sat * 1.4f32
            }
            HsyStyle::Log => {
                let sat_gain = 4.0f32;
                dist_rgb * sat_gain
            }
            HsyStyle::Vid => {
                let sat_gain = 1.25f32;
                dist_rgb * sat_gain
            }
        };

        // NB: Unlike typical HSV, HSY maps magenta rather than red to a hue of zero.
        let mut hue = 0.0f32;
        if rgb_min != rgb_max {
            let delta = rgb_max - rgb_min;
            if red == rgb_max {
                hue = 1.0f32 + (grn - blu) / delta;
            } else if grn == rgb_max {
                hue = 3.0f32 + (blu - red) / delta;
            } else {
                hue = 5.0f32 + (red - grn) / delta;
            }
            hue *= 0.16666666666666666f32;
        }

        pixel[0] = hue;
        pixel[1] = sat;
        pixel[2] = luma;
    }
}

/// RGB to HSY for log spaces.
///
/// Port of `Renderer_RGB_TO_HSY_LOG` (FixedFunctionOpCPU.cpp:241-248, 1765-1773 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererRgbToHsyLog;

impl CpuOp for RendererRgbToHsyLog {
    fn apply(&self, rgba: &mut [f32]) {
        apply_rgb_to_hsy(rgba, HsyStyle::Log);
    }
}

/// HSY to RGB for log spaces.
///
/// Port of `Renderer_HSY_LOG_TO_RGB` (FixedFunctionOpCPU.cpp:250-257, 1775-1783 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererHsyLogToRgb;

impl CpuOp for RendererHsyLogToRgb {
    fn apply(&self, rgba: &mut [f32]) {
        apply_hsy_to_rgb(rgba, HsyStyle::Log);
    }
}

/// RGB to HSY for linear spaces.
///
/// Port of `Renderer_RGB_TO_HSY_LIN` (FixedFunctionOpCPU.cpp:277-284, 1785-1793 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererRgbToHsyLin;

impl CpuOp for RendererRgbToHsyLin {
    fn apply(&self, rgba: &mut [f32]) {
        apply_rgb_to_hsy(rgba, HsyStyle::Lin);
    }
}

/// HSY to RGB for linear spaces.
///
/// Port of `Renderer_HSY_LIN_TO_RGB` (FixedFunctionOpCPU.cpp:286-293, 1795-1803 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererHsyLinToRgb;

impl CpuOp for RendererHsyLinToRgb {
    fn apply(&self, rgba: &mut [f32]) {
        apply_hsy_to_rgb(rgba, HsyStyle::Lin);
    }
}

/// RGB to HSY for video spaces.
///
/// Port of `Renderer_RGB_TO_HSY_VID` (FixedFunctionOpCPU.cpp:259-266, 1805-1813 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererRgbToHsyVid;

impl CpuOp for RendererRgbToHsyVid {
    fn apply(&self, rgba: &mut [f32]) {
        apply_rgb_to_hsy(rgba, HsyStyle::Vid);
    }
}

/// HSY to RGB for video spaces.
///
/// Port of `Renderer_HSY_VID_TO_RGB` (FixedFunctionOpCPU.cpp:268-275, 1815-1823 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererHsyVidToRgb;

impl CpuOp for RendererHsyVidToRgb {
    fn apply(&self, rgba: &mut [f32]) {
        apply_hsy_to_rgb(rgba, HsyStyle::Vid);
    }
}

/// `params[i]`, or [`SHORT_PARAMS`] where upstream would read past the parameters (U-31).
fn param(params: &[f64], i: usize) -> Result<f64> {
    params
        .get(i)
        .copied()
        .ok_or_else(|| Exception::new(SHORT_PARAMS))
}

/// `value * std::copysign(1.0f, sign)` as each wheel computes it (I-83). MSVC builds `±1.0f`
/// and multiplies it by `value` (Windows `0x18018d39e`-`0x18018d3a7`), which keeps a NaN
/// `value`'s sign; GCC's `xorsign` pattern flips `value`'s sign bit where `sign`'s is set
/// (Linux `0x354db5`-`0x354dc2`), a NaN's too, and leaves a signalling NaN signalling.
#[inline]
fn times_copysign_one(value: f32, sign: f32) -> f32 {
    #[cfg(target_os = "windows")]
    {
        sse_mul(1.0f32.copysign(sign), value)
    }
    #[cfg(target_os = "linux")]
    {
        f32::from_bits(value.to_bits() ^ (sign.to_bits() & 0x8000_0000))
    }
}

/// The gamma segment of [`RendererLinToGammaLog`]: `Ygamma = slope * (Xlin + off)^power`.
#[derive(Debug, Clone, Copy)]
struct GammaSegment {
    /// `power`.
    power: f32,
    /// `slope`: the post-power scale.
    slope: f32,
    /// `off`: the pre-power offset.
    off: f32,
}

/// The log segment of [`RendererLinToGammaLog`]: `Ylog = logSlope * log(linSlope * Xlin +
/// linOff, base) + logOff`.
#[derive(Debug, Clone, Copy)]
struct GammaLogSegment {
    /// `logSlope`, with the base conversion baked in.
    log_slope: f32,
    /// `logOff`.
    log_off: f32,
    /// `linSlope`.
    lin_slope: f32,
    /// `linOff`.
    lin_off: f32,
}

/// A curve with a gamma segment below a break point and a log segment above it, mirrored
/// around a point.
///
/// Port of `Renderer_LIN_TO_GAMMA_LOG` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:389-419, 2230-2277 @ v2.5.2).
#[derive(Debug)]
pub struct RendererLinToGammaLog {
    /// `m_mirror`: the mirroring point in lin space.
    mirror: f32,
    /// `m_break`: the break point between gamma and log in lin space.
    break_: f32,
    /// `m_gammaSeg`.
    gamma_seg: GammaSegment,
    /// `m_logSeg`.
    log_seg: GammaLogSegment,
}

impl RendererLinToGammaLog {
    /// The parameters as floats, the log base conversion baked into `logSlope`: `params[6] /
    /// log(params[5])` in double, with the C library's `log` (both wheels; the base is
    /// positive or NaN after validation, so glibc's compatibility `log` that the Linux wheel
    /// links answers as the current one). [`SHORT_PARAMS`] for fewer than ten parameters,
    /// which upstream reads past (U-31).
    ///
    /// Port of `Renderer_LIN_TO_GAMMA_LOG::Renderer_LIN_TO_GAMMA_LOG`
    /// (FixedFunctionOpCPU.cpp:2230-2246 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let params = data.params();
        let p = |i| param(params, i);
        let log_slope = p(6)? / p(5)?.ln();
        Ok(RendererLinToGammaLog {
            mirror: p(0)? as f32,
            break_: p(1)? as f32,
            gamma_seg: GammaSegment {
                power: p(2)? as f32,
                slope: p(3)? as f32,
                off: p(4)? as f32,
            },
            log_seg: GammaLogSegment {
                log_slope: log_slope as f32,
                log_off: p(7)? as f32,
                lin_slope: p(8)? as f32,
                lin_off: p(9)? as f32,
            },
        })
    }
}

impl CpuOp for RendererLinToGammaLog {
    /// Where a NaN pixel meets a NaN parameter (W0002), the operand order is both wheels':
    /// `(E + off)^power * slope` and `log(E * linSlope + linOff) * logSlope + logOff`
    /// (Windows `0x18018d35e`-`0x18018d38e`, Linux `0x354d92`-`0x354daf`, `0x354e08`-`0x354e2b`).
    /// The sign comes back per platform ([`times_copysign_one`]).
    ///
    /// Port of `Renderer_LIN_TO_GAMMA_LOG::apply` (FixedFunctionOpCPU.cpp:2248-2277 @
    /// v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let g = &self.gamma_seg;
        let l = &self.log_seg;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            for value in &mut pixel[..3] {
                let ein = *value;

                let mirrorin = ein - self.mirror;
                let e = sse_add(mirrorin.abs(), self.mirror);
                let eprime = if e < self.break_ {
                    sse_mul(sse_add(e, g.off).powf(g.power), g.slope)
                } else {
                    let x = sse_add(sse_mul(e, l.lin_slope), l.lin_off);
                    sse_add(sse_mul(x.ln(), l.log_slope), l.log_off)
                };
                *value = times_copysign_one(eprime, mirrorin);
            }
        }
    }
}

/// The inverse of [`RendererLinToGammaLog`].
///
/// Port of `Renderer_GAMMA_LOG_TO_LIN` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:421-430, 2279-2320 @ v2.5.2).
#[derive(Debug)]
pub struct RendererGammaLogToLin {
    /// The forward renderer, whose parameters it shares.
    fwd: RendererLinToGammaLog,
    /// `m_primeBreak`: the break point in the non-linear axis.
    prime_break: f32,
    /// `m_primeMirror`: the mirror point in the non-linear axis.
    prime_mirror: f32,
}

impl RendererGammaLogToLin {
    /// Assuming that the function is continuous, the gamma segment gives the break and mirror
    /// points in the non-linear domain. Both wheels compute `(x + off)^power * slope`
    /// (Linux `0x35931e`, `0x359341`).
    ///
    /// Port of `Renderer_GAMMA_LOG_TO_LIN::Renderer_GAMMA_LOG_TO_LIN`
    /// (FixedFunctionOpCPU.cpp:2279-2288 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = RendererLinToGammaLog::new(data)?;
        let g = fwd.gamma_seg;
        let prime_break = sse_mul(sse_add(fwd.break_, g.off).powf(g.power), g.slope);
        let prime_mirror = sse_mul(sse_add(fwd.mirror, g.off).powf(g.power), g.slope);
        Ok(RendererGammaLogToLin {
            fwd,
            prime_break,
            prime_mirror,
        })
    }
}

impl CpuOp for RendererGammaLogToLin {
    /// The sign comes back per platform ([`times_copysign_one`]; Windows `0x18018cd17`-
    /// `0x18018cd2b`, Linux `0x355071`-`0x35507e`).
    ///
    /// Port of `Renderer_GAMMA_LOG_TO_LIN::apply` (FixedFunctionOpCPU.cpp:2290-2320 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let g = &self.fwd.gamma_seg;
        let l = &self.fwd.log_seg;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            for value in &mut pixel[..3] {
                let eprimein = *value;

                let mirrorin = eprimein - self.prime_mirror;
                let eprime = sse_add(mirrorin.abs(), self.prime_mirror);
                let e = if eprime < self.prime_break {
                    (eprime / g.slope).powf(1.0f32 / g.power) - g.off
                } else {
                    (((eprime - l.log_off) / l.log_slope).exp() - l.lin_off) / l.lin_slope
                };
                // Flip the sign below the mirror point.
                *value = times_copysign_one(e, mirrorin);
            }
        }
    }
}

/// A log segment of [`RendererLinToDoubleLog`]: `Ylog = logSlope * log(linSlope * Xlin +
/// linOff, base) + logOff`.
#[derive(Debug, Clone, Copy)]
struct DoubleLogSegment {
    /// `logSlope`, with the base conversion baked in.
    log_slope: f32,
    /// `logOff`.
    log_off: f32,
    /// `linSlope`.
    lin_slope: f32,
    /// `linOff`.
    lin_off: f32,
}

impl DoubleLogSegment {
    /// `logSlope * log(linSlope * x + linOff) + logOff`, in both wheels' operand order:
    /// `log(x * linSlope + linOff) * logSlope + logOff` (`Renderer_LIN_TO_DOUBLE_LOG::apply`,
    /// Windows `0x18018d19e`-`0x18018d1b8`, Linux `0x354c06`-`0x354c26`).
    ///
    /// Port of the log segments of `Renderer_LIN_TO_DOUBLE_LOG::apply`
    /// (FixedFunctionOpCPU.cpp:2359-2370 @ v2.5.2).
    #[inline]
    fn eval(&self, x: f32) -> f32 {
        let x = sse_add(sse_mul(x, self.lin_slope), self.lin_off);
        sse_add(sse_mul(x.ln(), self.log_slope), self.log_off)
    }

    /// The same at a break point, for the inverse's constructor, which both wheels compute
    /// as `log(linSlope * x + linOff) * logSlope + logOff` (Windows `0x180189bfc`-`0x180189c19`,
    /// inlined in `GetFixedFunctionCPURenderer`; Linux `0x35954d`-`0x359581`).
    ///
    /// Port of a break of `Renderer_DOUBLE_LOG_TO_LIN::Renderer_DOUBLE_LOG_TO_LIN`
    /// (FixedFunctionOpCPU.cpp:2387-2388 @ v2.5.2).
    #[inline]
    fn eval_break(&self, x: f32) -> f32 {
        let x = sse_add(sse_mul(self.lin_slope, x), self.lin_off);
        sse_add(sse_mul(x.ln(), self.log_slope), self.log_off)
    }
}

/// Two log segments with a linear one between them.
///
/// Port of `Renderer_LIN_TO_DOUBLE_LOG` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:432-462, 2322-2378 @ v2.5.2).
#[derive(Debug)]
pub struct RendererLinToDoubleLog {
    /// `m_break1`: between the first log segment and the linear segment.
    break1: f32,
    /// `m_break2`: between the linear segment and the second log segment.
    break2: f32,
    /// `m_logSeg1`.
    log_seg1: DoubleLogSegment,
    /// `m_logSeg2`.
    log_seg2: DoubleLogSegment,
    /// `m_linSeg`: `slope` and `off`.
    lin_seg: (f32, f32),
}

impl RendererLinToDoubleLog {
    /// The parameters as floats, the log base conversion baked into each `logSlope`:
    /// `(float)params[i] / logf(base)`. [`SHORT_PARAMS`] for fewer than 13 parameters, which
    /// upstream reads past (U-31).
    ///
    /// Port of `Renderer_LIN_TO_DOUBLE_LOG::Renderer_LIN_TO_DOUBLE_LOG`
    /// (FixedFunctionOpCPU.cpp:2322-2344 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let params = data.params();
        let p = |i| param(params, i).map(|v| v as f32);
        let base = p(0)?;
        Ok(RendererLinToDoubleLog {
            break1: p(1)?,
            break2: p(2)?,
            log_seg1: DoubleLogSegment {
                log_slope: p(3)? / base.ln(),
                log_off: p(4)?,
                lin_slope: p(5)?,
                lin_off: p(6)?,
            },
            log_seg2: DoubleLogSegment {
                log_slope: p(7)? / base.ln(),
                log_off: p(8)?,
                lin_slope: p(9)?,
                lin_off: p(10)?,
            },
            lin_seg: (p(11)?, p(12)?),
        })
    }
}

impl CpuOp for RendererLinToDoubleLog {
    /// The linear segment is `x * slope + off` in both wheels (Windows `0x18018d1c9`, Linux
    /// `0x354b5e`).
    ///
    /// Port of `Renderer_LIN_TO_DOUBLE_LOG::apply` (FixedFunctionOpCPU.cpp:2346-2378 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let (slope, off) = self.lin_seg;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            for value in &mut pixel[..3] {
                let mut x = *value;

                // Linear segment may not exist or be valid. Thus we include the break points in
                // the log segments.
                if x <= self.break1 {
                    x = self.log_seg1.eval(x);
                } else if x < self.break2 {
                    x = sse_add(sse_mul(x, slope), off);
                } else {
                    x = self.log_seg2.eval(x);
                }

                *value = x;
            }
        }
    }
}

/// The inverse of [`RendererLinToDoubleLog`].
///
/// Port of `Renderer_DOUBLE_LOG_TO_LIN` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:464-474, 2380-2423 @ v2.5.2).
#[derive(Debug)]
pub struct RendererDoubleLogToLin {
    /// The forward renderer, whose parameters it shares.
    fwd: RendererLinToDoubleLog,
    /// `m_break1Log`: the first break in log space.
    break1_log: f32,
    /// `m_break2Log`: the second break in log space.
    break2_log: f32,
}

impl RendererDoubleLogToLin {
    /// The break locations in log space (the break points belong to the log segments, not the
    /// linear segment, which may be missing).
    ///
    /// Port of `Renderer_DOUBLE_LOG_TO_LIN::Renderer_DOUBLE_LOG_TO_LIN`
    /// (FixedFunctionOpCPU.cpp:2380-2389 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        let fwd = RendererLinToDoubleLog::new(data)?;
        let break1_log = fwd.log_seg1.eval_break(fwd.break1);
        let break2_log = fwd.log_seg2.eval_break(fwd.break2);
        Ok(RendererDoubleLogToLin {
            fwd,
            break1_log,
            break2_log,
        })
    }
}

impl CpuOp for RendererDoubleLogToLin {
    /// Port of `Renderer_DOUBLE_LOG_TO_LIN::apply` (FixedFunctionOpCPU.cpp:2391-2423 @
    /// v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let (s1, s2) = (&self.fwd.log_seg1, &self.fwd.log_seg2);
        let (slope, off) = self.fwd.lin_seg;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            for value in &mut pixel[..3] {
                let mut y = *value;

                if y <= self.break1_log {
                    y = (((y - s1.log_off) / s1.log_slope).exp() - s1.lin_off) / s1.lin_slope;
                } else if y < self.break2_log {
                    y = (y - off) / slope;
                } else {
                    y = (((y - s2.log_off) / s2.log_slope).exp() - s2.lin_off) / s2.lin_slope;
                }

                *value = y;
            }
        }
    }
}

/// The renderer of `func`'s style. `fast_log_exp_pow` picks the fast-math variants of the
/// styles that have one (PQ, chunk 2.3d). The styles whose renderers aren't ported yet are
/// refused ([`not_ported`]).
///
/// Port of `GetFixedFunctionCPURenderer` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:2429-2643 @ v2.5.2). Its "Unsupported FixedFunction style" for a
/// value outside the enum can't happen.
pub fn get_fixed_function_cpu_renderer(
    func: &FixedFunctionOpData,
    fast_log_exp_pow: bool,
) -> Result<Arc<dyn CpuOp>> {
    use FixedFunctionOpStyle::*;
    let _ = fast_log_exp_pow;
    Ok(match func.style() {
        AcesRedMod03Fwd => Arc::new(RendererAcesRedMod03Fwd::new()),
        AcesRedMod03Inv => Arc::new(RendererAcesRedMod03Inv::default()),
        AcesRedMod10Fwd => Arc::new(RendererAcesRedMod10Fwd::new()),
        AcesRedMod10Inv => Arc::new(RendererAcesRedMod10Inv::default()),
        AcesGlow03Fwd => Arc::new(RendererAcesGlow03Fwd::new(0.075, 0.1)),
        AcesGlow03Inv => Arc::new(RendererAcesGlow03Inv::new(0.075, 0.1)),
        AcesGlow10Fwd => Arc::new(RendererAcesGlow03Fwd::new(0.05, 0.08)),
        AcesGlow10Inv => Arc::new(RendererAcesGlow03Inv::new(0.05, 0.08)),
        AcesDarkToDim10Fwd => Arc::new(RendererAcesDarkToDim10Fwd::new(0.9811)),
        AcesDarkToDim10Inv => Arc::new(RendererAcesDarkToDim10Fwd::new(1.0192640913260627)),
        AcesGamutComp13Fwd => Arc::new(RendererAcesGamutComp13Fwd::new(func)?),
        AcesGamutComp13Inv => Arc::new(RendererAcesGamutComp13Inv::new(func)?),

        AcesRgbToJmh20 | AcesJmhToRgb20 => Arc::new(RendererAcesRgbToJmh20::new(func)?),
        AcesTonescaleCompress20Fwd | AcesTonescaleCompress20Inv => {
            Arc::new(RendererAcesTonescaleCompress20::new(func)?)
        }

        Rec2100SurroundFwd | Rec2100SurroundInv => {
            // Sharing same renderer (param will be inverted to handle direction).
            Arc::new(RendererRec2100Surround::new(func)?)
        }

        RgbToHsv => Arc::new(RendererRgbToHsv),
        HsvToRgb => Arc::new(RendererHsvToRgb),

        XyzToXyy => Arc::new(RendererXyzToXyy),
        XyyToXyz => Arc::new(RendererXyyToXyz),

        XyzToUvy => Arc::new(RendererXyzToUvy),
        UvyToXyz => Arc::new(RendererUvyToXyz),

        XyzToLuv => Arc::new(RendererXyzToLuv),
        LuvToXyz => Arc::new(RendererLuvToXyz),

        RgbToHsyLog => Arc::new(RendererRgbToHsyLog),
        HsyLogToRgb => Arc::new(RendererHsyLogToRgb),

        RgbToHsyLin => Arc::new(RendererRgbToHsyLin),
        HsyLinToRgb => Arc::new(RendererHsyLinToRgb),

        RgbToHsyVid => Arc::new(RendererRgbToHsyVid),
        HsyVidToRgb => Arc::new(RendererHsyVidToRgb),

        style @ (AcesOutputTransform20Fwd
        | AcesOutputTransform20Inv
        | AcesGamutCompress20Fwd
        | AcesGamutCompress20Inv
        | LinToPq
        | PqToLin) => return Err(not_ported(style)),

        LinToGammaLog => Arc::new(RendererLinToGammaLog::new(func)?),
        GammaLogToLin => Arc::new(RendererGammaLogToLin::new(func)?),

        LinToDoubleLog => Arc::new(RendererLinToDoubleLog::new(func)?),
        DoubleLogToLin => Arc::new(RendererDoubleLogToLin::new(func)?),
    })
}

#[cfg(test)]
#[path = "fixed_function_op_cpu_tests.rs"]
mod tests;
