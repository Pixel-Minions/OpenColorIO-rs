// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction op's CPU renderers: a port of
//! `src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp` @ v2.5.2.
//!
//! So far the ACES 1.x styles: the red modifiers 0.3 and 1.0, the glows 0.3 and 1.0, the dark
//! to dim surround 1.0 and the gamut compression 1.3, forward and inverse (chunk 2.3b). The
//! other styles' renderers come with chunks 2.3c, 2.3d and 2.4e; until then
//! [`get_fixed_function_cpu_renderer`] refuses them ([`not_ported`]).
//!
//! The renderers work in place and never write alpha, which upstream copies (`out[3] =
//! in[3]`), nor a channel upstream leaves as it was.
//!
//! # Operand order
//!
//! Where two NaNs of different payloads can meet, the result is the first operand's
//! (`CLAUDE.md`, "NaN operand order"), so the port pins the order both wheels' machine code
//! uses ([`math_utils::sse_add`], [`math_utils::sse_mul`]). In these renderers only the glow
//! mixes input channels into a value that reaches the output: its luma-chroma value and the
//! final products (see [`rgb_to_yc`] and [`RendererAcesGlow03Fwd`]). The red modifier's
//! hue weight is 0 whenever an input is NaN, so a NaN pixel goes through it unchanged; the
//! dark to dim's luminance is never NaN (`std::max(minLum, NaN)` is `minLum`), and the gamut
//! compression's achromatic value is red's NaN or no NaN, which its arithmetic propagates
//! whatever the order. Every other NaN inside is the default NaN, whose bits don't depend on
//! the order.

use std::sync::Arc;

use super::fixed_function_op_data::{FixedFunctionOpData, FixedFunctionOpStyle, SHORT_PARAMS};
use crate::exception::{Exception, Result};
use crate::math_utils::{sse_add, sse_cvttps_epi32, sse_mul, std_max, std_min};
use crate::op::CpuOp;

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
/// Port of `CalcSatWeight` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:481-494 @
/// v2.5.2).
#[inline]
fn calc_sat_weight(red: f32, grn: f32, blu: f32, noise_limit: f32) -> f32 {
    let min_val = std_min(red, std_min(grn, blu));
    let max_val = std_max(red, std_max(grn, blu));

    (std_max(1e-10f32, max_val) - std_max(1e-10f32, min_val)) / std_max(noise_limit, max_val)
}

/// The coefficients of a quadratic B-spline basis function (all coefs taken from the ACES ctl
/// code on github), FixedFunctionOpCPU.cpp:535-539 @ v2.5.2.
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
/// Port of `CalcHueWeight` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:508-556 @
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
/// FixedFunctionOpCPU.cpp:21-35, 496-506, 558-604 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesRedMod03Fwd {
    /// `m_1minusScale`: `1 - scale`, from the original ctl code.
    one_minus_scale: f32,
    /// `m_pivot`: the offset is applied to unnormalized input values.
    pivot: f32,
    /// `m_inv_width`: `4 / (width * pi / 180)` for a hue region of 120 degrees.
    inv_width: f32,
}

/// `m_noiseLimit` of the red modifiers and the glows (FixedFunctionOpCPU.cpp:34, 58, 79 @
/// v2.5.2).
const NOISE_LIMIT: f32 = 1e-2;

impl RendererAcesRedMod03Fwd {
    /// Port of `Renderer_ACES_RedMod03_Fwd::Renderer_ACES_RedMod03_Fwd`
    /// (FixedFunctionOpCPU.cpp:496-506 @ v2.5.2).
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
    /// Port of `Renderer_ACES_RedMod03_Fwd::apply` (FixedFunctionOpCPU.cpp:558-604 @ v2.5.2).
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
/// FixedFunctionOpCPU.cpp:37-43, 606-656 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererAcesRedMod03Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesRedMod03Fwd,
}

impl CpuOp for RendererAcesRedMod03Inv {
    /// Port of `Renderer_ACES_RedMod03_Inv::apply` (FixedFunctionOpCPU.cpp:611-656 @ v2.5.2).
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
/// FixedFunctionOpCPU.cpp:45-59, 658-704 @ v2.5.2).
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
    /// (FixedFunctionOpCPU.cpp:658-668 @ v2.5.2).
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
    /// Port of `Renderer_ACES_RedMod10_Fwd::apply` (FixedFunctionOpCPU.cpp:670-704 @ v2.5.2).
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
/// FixedFunctionOpCPU.cpp:61-67, 706-740 @ v2.5.2).
#[derive(Debug, Default)]
pub struct RendererAcesRedMod10Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesRedMod10Fwd,
}

impl CpuOp for RendererAcesRedMod10Inv {
    /// Port of `Renderer_ACES_RedMod10_Inv::apply` (FixedFunctionOpCPU.cpp:711-740 @ v2.5.2).
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
/// 751-758 inlined in `Renderer_ACES_Glow03_Fwd::apply` and `_Inv::apply`, Windows
/// `0x18018b37e`, `0x18018b3f0`; Linux `0x355195`, `0x3551c9`): MSVC adds the green term to
/// the blue one, GCC the blue term to the green one, then both add the red term; and both
/// add the sum of the channels to the chroma term (`1.75 * chroma + ((blu + grn) + red)`).
/// The products' operands can't hold two different NaNs (`blu - grn` is a NaN only where
/// `grn` or `blu` is, and then it is `blu`'s NaN when `blu` is one).
///
/// Port of `rgbToYC` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:751-758 @
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
/// Port of `SigmoidShaper` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:760-767
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
/// FixedFunctionOpCPU.cpp:69-81, 742-749, 769-822 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesGlow03Fwd {
    /// `m_glowGain`.
    glow_gain: f32,
    /// `m_glowMid`.
    glow_mid: f32,
}

impl RendererAcesGlow03Fwd {
    /// Port of `Renderer_ACES_Glow03_Fwd::Renderer_ACES_Glow03_Fwd`
    /// (FixedFunctionOpCPU.cpp:742-749 @ v2.5.2).
    pub fn new(glow_gain: f32, glow_mid: f32) -> Self {
        RendererAcesGlow03Fwd {
            glow_gain,
            glow_mid,
        }
    }
}

impl CpuOp for RendererAcesGlow03Fwd {
    /// Port of `Renderer_ACES_Glow03_Fwd::apply` (FixedFunctionOpCPU.cpp:769-822 @ v2.5.2).
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
/// FixedFunctionOpCPU.cpp:83-89, 824-882 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesGlow03Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesGlow03Fwd,
}

impl RendererAcesGlow03Inv {
    /// Port of `Renderer_ACES_Glow03_Inv::Renderer_ACES_Glow03_Inv`
    /// (FixedFunctionOpCPU.cpp:824-829 @ v2.5.2).
    pub fn new(glow_gain: f32, glow_mid: f32) -> Self {
        RendererAcesGlow03Inv {
            fwd: RendererAcesGlow03Fwd::new(glow_gain, glow_mid),
        }
    }
}

impl CpuOp for RendererAcesGlow03Inv {
    /// Port of `Renderer_ACES_Glow03_Inv::apply` (FixedFunctionOpCPU.cpp:831-882 @ v2.5.2).
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
/// FixedFunctionOpCPU.cpp:91-101, 884-923 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesDarkToDim10Fwd {
    /// `m_gamma`: `gamma - 1`, to compute `Y^gamma / Y`.
    gamma: f32,
}

impl RendererAcesDarkToDim10Fwd {
    /// Port of `Renderer_ACES_DarkToDim10_Fwd::Renderer_ACES_DarkToDim10_Fwd`
    /// (FixedFunctionOpCPU.cpp:884-889 @ v2.5.2).
    pub fn new(gamma: f32) -> Self {
        RendererAcesDarkToDim10Fwd {
            gamma: gamma - 1.0f32,
        }
    }
}

impl CpuOp for RendererAcesDarkToDim10Fwd {
    /// Port of `Renderer_ACES_DarkToDim10_Fwd::apply` (FixedFunctionOpCPU.cpp:891-923 @
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
/// Port of `compress` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:925-933 @
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
/// Port of `uncompress` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:935-950 @
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
/// Port of `gamut_comp` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:952-980 @
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
/// FixedFunctionOpCPU.cpp:103-123, 982-1023 @ v2.5.2).
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
    /// (FixedFunctionOpCPU.cpp:982-1001 @ v2.5.2).
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
    /// Port of `Renderer_ACES_GamutComp13_Fwd::apply` (FixedFunctionOpCPU.cpp:1003-1023 @
    /// v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        self.apply_with(rgba, compress);
    }
}

/// The inverse of the gamut compression.
///
/// Port of `Renderer_ACES_GamutComp13_Inv` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpCPU.cpp:125-131, 1025-1053 @ v2.5.2).
#[derive(Debug)]
pub struct RendererAcesGamutComp13Inv {
    /// The forward renderer, whose constants it shares.
    fwd: RendererAcesGamutComp13Fwd,
}

impl RendererAcesGamutComp13Inv {
    /// Port of `Renderer_ACES_GamutComp13_Inv::Renderer_ACES_GamutComp13_Inv`
    /// (FixedFunctionOpCPU.cpp:1025-1028 @ v2.5.2).
    pub fn new(data: &FixedFunctionOpData) -> Result<Self> {
        Ok(RendererAcesGamutComp13Inv {
            fwd: RendererAcesGamutComp13Fwd::new(data)?,
        })
    }
}

impl CpuOp for RendererAcesGamutComp13Inv {
    /// Port of `Renderer_ACES_GamutComp13_Inv::apply` (FixedFunctionOpCPU.cpp:1030-1053 @
    /// v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        self.fwd.apply_with(rgba, uncompress);
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

        style @ (AcesOutputTransform20Fwd
        | AcesOutputTransform20Inv
        | AcesRgbToJmh20
        | AcesJmhToRgb20
        | AcesTonescaleCompress20Fwd
        | AcesTonescaleCompress20Inv
        | AcesGamutCompress20Fwd
        | AcesGamutCompress20Inv
        | Rec2100SurroundFwd
        | Rec2100SurroundInv
        | RgbToHsv
        | HsvToRgb
        | XyzToXyy
        | XyyToXyz
        | XyzToUvy
        | UvyToXyz
        | XyzToLuv
        | LuvToXyz
        | LinToPq
        | PqToLin
        | LinToGammaLog
        | GammaLogToLin
        | LinToDoubleLog
        | DoubleLogToLin
        | RgbToHsyLog
        | HsyLogToRgb
        | RgbToHsyLin
        | HsyLinToRgb
        | RgbToHsyVid
        | HsyVidToRgb) => return Err(not_ported(style)),
    })
}

#[cfg(test)]
#[path = "fixed_function_op_cpu_tests.rs"]
mod tests;
