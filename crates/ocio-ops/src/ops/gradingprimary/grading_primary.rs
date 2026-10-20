// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The values of the GradingPrimary op and their pre-render form. A port of
//! `src/OpenColorIO/ops/gradingprimary/GradingPrimary.h` and `GradingPrimary.cpp` @ v2.5.2, of
//! the public `GradingRGBM` and `GradingPrimary` (include/OpenColorIO/OpenColorTransforms.h:
//! 433-496), and of their `operator<<` (src/OpenColorIO/transforms/GradingPrimaryTransform.cpp:
//! 127-160).
//!
//! The values are plain Rust values with public fields, as upstream's structs (owner decision
//! P5-2): `==` is upstream's `operator==`, exact on the doubles.

use std::fmt;

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::math_utils::{sse_add, sse_mul, sse_sub, std_pow, std_powf};
use crate::open_color_types::{GradingStyle, TransformDirection};

/// Three floats, the red, green and blue of a pre-render value.
///
/// Port of `Float3` (`std::array<float, 3>`, include/OpenColorIO/OpenColorTypes.h @ v2.5.2).
pub type Float3 = [f32; 3];

/// The red, green, blue and master components of one grading parameter: the master affects
/// all three channels.
///
/// Port of `GradingRGBM` (include/OpenColorIO/OpenColorTransforms.h:433-456 @ v2.5.2) and its
/// `operator==` (src/OpenColorIO/ops/gradingprimary/GradingPrimary.cpp:15-24).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradingRgbm {
    /// `m_red`.
    #[doc(alias = "m_red")]
    pub red: f64,
    /// `m_green`.
    #[doc(alias = "m_green")]
    pub green: f64,
    /// `m_blue`.
    #[doc(alias = "m_blue")]
    pub blue: f64,
    /// `m_master`.
    #[doc(alias = "m_master")]
    pub master: f64,
}

impl GradingRgbm {
    /// Port of `GradingRGBM(double red, double green, double blue, double master)`
    /// (OpenColorTransforms.h:438-444 @ v2.5.2).
    pub fn new(red: f64, green: f64, blue: f64, master: f64) -> Self {
        GradingRgbm {
            red,
            green,
            blue,
            master,
        }
    }

    /// Port of `GradingRGBM(const double(&rgbm)[4])` (OpenColorTransforms.h:445-451 @ v2.5.2).
    pub fn from_array(rgbm: &[f64; 4]) -> Self {
        GradingRgbm::new(rgbm[0], rgbm[1], rgbm[2], rgbm[3])
    }

    /// `os << rgbm`: `<r=..., g=..., b=..., m=...>`, each double in the stream's format.
    ///
    /// Port of `operator<<(std::ostream &, const GradingRGBM &)`
    /// (src/OpenColorIO/transforms/GradingPrimaryTransform.cpp:127-132 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        os.put_str("<r=");
        os.put_f64(self.red);
        os.put_str(", g=");
        os.put_f64(self.green);
        os.put_str(", b=");
        os.put_f64(self.blue);
        os.put_str(", m=");
        os.put_f64(self.master);
        os.put_str(">");
    }
}

/// `operator<<` with a new stream's format (6 significant digits).
impl fmt::Display for GradingRgbm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// The values of a GradingPrimary op. Which ones it uses depends on its style: brightness,
/// contrast and gamma for log; exposure, contrast and offset for linear; lift, gain, gamma and
/// offset for video; saturation, the pivots and the clamps for all.
///
/// Port of `GradingPrimary` (include/OpenColorIO/OpenColorTransforms.h:461-496 @ v2.5.2) and
/// its `operator==` (src/OpenColorIO/ops/gradingprimary/GradingPrimary.cpp:31-46).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingPrimary {
    /// `m_brightness`.
    pub brightness: GradingRgbm,
    /// `m_contrast`.
    pub contrast: GradingRgbm,
    /// `m_gamma`.
    pub gamma: GradingRgbm,
    /// `m_offset`.
    pub offset: GradingRgbm,
    /// `m_exposure`.
    pub exposure: GradingRgbm,
    /// `m_lift`.
    pub lift: GradingRgbm,
    /// `m_gain`.
    pub gain: GradingRgbm,
    /// `m_saturation`.
    pub saturation: f64,
    /// `m_pivot`: -0.2 by default for log, 0.18 for linear and video.
    pub pivot: f64,
    /// `m_pivotBlack`.
    pub pivot_black: f64,
    /// `m_pivotWhite`.
    pub pivot_white: f64,
    /// `m_clampBlack`.
    pub clamp_black: f64,
    /// `m_clampWhite`.
    pub clamp_white: f64,
}

impl GradingPrimary {
    /// The values that change nothing, for `style`.
    ///
    /// Port of `GradingPrimary(GradingStyle)` (include/OpenColorIO/OpenColorTransforms.h:
    /// 465-471 @ v2.5.2).
    pub fn new(style: GradingStyle) -> Self {
        GradingPrimary {
            brightness: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            contrast: GradingRgbm::new(1.0, 1.0, 1.0, 1.0),
            gamma: GradingRgbm::new(1.0, 1.0, 1.0, 1.0),
            offset: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            exposure: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            lift: GradingRgbm::new(0.0, 0.0, 0.0, 0.0),
            gain: GradingRgbm::new(1.0, 1.0, 1.0, 1.0),
            saturation: 1.0,
            pivot: if style == GradingStyle::Log {
                -0.2
            } else {
                0.18
            },
            pivot_black: 0.0,
            pivot_white: 1.0,
            clamp_black: Self::NO_CLAMP_BLACK,
            clamp_white: Self::NO_CLAMP_WHITE,
        }
    }

    /// The black clamp that clamps nothing: the renderers rely on this value.
    ///
    /// Port of `GradingPrimary::NoClampBlack` (src/OpenColorIO/ops/gradingprimary/
    /// GradingPrimary.cpp:53-57 @ v2.5.2).
    pub const NO_CLAMP_BLACK: f64 = -f64::MAX;

    /// The white clamp that clamps nothing: the renderers rely on this value.
    ///
    /// Port of `GradingPrimary::NoClampWhite` (src/OpenColorIO/ops/gradingprimary/
    /// GradingPrimary.cpp:59-63 @ v2.5.2).
    pub const NO_CLAMP_WHITE: f64 = f64::MAX;

    /// Checks the values for `style`: gamma (log and video) or contrast (linear) at least
    /// 0.01, less 1e-6; the white pivot above the black one by as much; the black clamp not
    /// above the white one. A NaN passes these checks, as upstream's comparisons let it.
    ///
    /// Port of `GradingPrimary::validate` (src/OpenColorIO/ops/gradingprimary/
    /// GradingPrimary.cpp:65-106 @ v2.5.2).
    pub fn validate(&self, style: GradingStyle) -> Result<()> {
        // Validate all value based on the style.
        const GRADING_PRIMARY_LOWER_BOUND: f64 = 0.01;
        const GRADING_PRIMARY_BOUND_ERROR: f64 = 0.000001;
        const GRADING_PRIMARY_MIN: f64 = GRADING_PRIMARY_LOWER_BOUND - GRADING_PRIMARY_BOUND_ERROR;

        let below = |v: &GradingRgbm| {
            v.red < GRADING_PRIMARY_MIN
                || v.green < GRADING_PRIMARY_MIN
                || v.blue < GRADING_PRIMARY_MIN
                || v.master < GRADING_PRIMARY_MIN
        };
        let bound_error = |name: &str, v: &GradingRgbm| {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("GradingPrimary ");
            oss.put_str(name);
            oss.put_str(" '");
            v.write_to(&mut oss);
            oss.put_str("' are below lower bound (");
            oss.put_f64(GRADING_PRIMARY_LOWER_BOUND);
            oss.put_str(").");
            Err(Exception::new(oss.into_bytes()))
        };

        if style != GradingStyle::Lin && below(&self.gamma) {
            return bound_error("gamma", &self.gamma);
        }

        if style == GradingStyle::Lin && below(&self.contrast) {
            return bound_error("contrast", &self.contrast);
        }

        if (self.pivot_white - self.pivot_black) < GRADING_PRIMARY_MIN {
            return Err(Exception::new(
                "GradingPrimary black pivot should be smaller than white pivot.",
            ));
        }

        if self.clamp_black > self.clamp_white {
            return Err(Exception::new(
                "GradingPrimary black clamp should be smaller than white clamp.",
            ));
        }
        Ok(())
    }

    /// `os << prim`: the values, and each clamp that isn't its no-clamp value.
    ///
    /// Port of `operator<<(std::ostream &, const GradingPrimary &)`
    /// (src/OpenColorIO/transforms/GradingPrimaryTransform.cpp:134-158 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        os.put_str("<brightness=");
        self.brightness.write_to(os);
        os.put_str(", contrast=");
        self.contrast.write_to(os);
        os.put_str(", gamma=");
        self.gamma.write_to(os);
        os.put_str(", offset=");
        self.offset.write_to(os);
        os.put_str(", exposure=");
        self.exposure.write_to(os);
        os.put_str(", lift=");
        self.lift.write_to(os);
        os.put_str(", gain=");
        self.gain.write_to(os);
        os.put_str(", saturation=");
        os.put_f64(self.saturation);
        os.put_str(", pivot=<contrast=");
        os.put_f64(self.pivot);
        os.put_str(", black=");
        os.put_f64(self.pivot_black);
        os.put_str(", white=");
        os.put_f64(self.pivot_white);
        os.put_str(">");
        if self.clamp_black != Self::NO_CLAMP_BLACK {
            os.put_str(", clampBlack=");
            os.put_f64(self.clamp_black);
        }
        if self.clamp_white != Self::NO_CLAMP_WHITE {
            os.put_str(", clampWhite=");
            os.put_f64(self.clamp_white);
        }
        os.put_str(">");
    }
}

/// `operator<<` with a new stream's format (6 significant digits).
impl fmt::Display for GradingPrimary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// The values the renderers use, computed once from a [`GradingPrimary`] for a style and a
/// direction: already inverted for the inverse direction.
///
/// Port of `GradingPrimaryPreRender` (src/OpenColorIO/ops/gradingprimary/GradingPrimary.h:
/// 16-63 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradingPrimaryPreRender {
    brightness: Float3,
    contrast: Float3,
    gamma: Float3,
    exposure: Float3,
    offset: Float3,
    slope: Float3,
    pivot: f64,
    is_power_identity: bool,
    local_bypass: bool,
}

/// Which operand of `master op channel` the wheel's machine code puts first, where both can be
/// NaN: the result is then the first one's NaN (`docs/improvements.md` I-91). MSVC and GCC
/// chose differently, and differently per channel; `tools/wheel-inspect` read both
/// (`docs/wheel-inspect.md`; Windows `sub_1801d34a0`, Linux 0x3a67b0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum First {
    /// The master value first.
    M,
    /// The channel's value first.
    C,
}

/// The operand orders of `GradingPrimaryPreRender::update`, per channel (red, green, blue).
struct Orders {
    /// `b.master + b.channel`, forward log.
    log_brightness_fwd: [First; 3],
    /// `b.master + b.channel`, inverse log.
    log_brightness_inv: [First; 3],
    /// `c.master * c.channel`, forward log.
    log_contrast_fwd: [First; 3],
    /// `c.master * c.channel`, inverse log.
    log_contrast_inv: [First; 3],
    /// `g.master * g.channel`, log (both directions).
    log_gamma: [First; 3],
    /// `o.master + o.channel`, linear (both directions).
    lin_offset: [First; 3],
    /// `e.master + e.channel`, linear (both directions).
    lin_exposure: [First; 3],
    /// `c.master * c.channel`, linear (both directions).
    lin_contrast: [First; 3],
    /// `gain.master * gain.channel`, video.
    video_gain: [First; 3],
    /// `o.master + o.channel`, video (both directions); the sum then adds `l.master`, then
    /// `l.channel`, on both wheels.
    video_offset: [First; 3],
    /// `g.master * g.channel`, video (both directions).
    video_gamma: [First; 3],
    /// `l.master + l.channel` of the inverse video slope.
    video_slope_inv: [First; 3],
    /// Whether the inverse log brightness `-(float)(x * 6.25 / 1023.)` keeps a NaN's sign:
    /// MSVC folded the negation into the constant (`x * -6.25 / 1023.`), GCC flips the sign.
    log_brightness_inv_folds: bool,
}

use First::{C, M};

/// The Windows wheel's (MSVC) orders.
#[cfg(windows)]
const ORDERS: Orders = Orders {
    log_brightness_fwd: [M, C, C],
    log_brightness_inv: [M, C, C],
    log_contrast_fwd: [M, C, C],
    log_contrast_inv: [M, M, M],
    log_gamma: [C, C, C],
    lin_offset: [M, C, C],
    lin_exposure: [M, C, C],
    lin_contrast: [M, C, C],
    video_gain: [M, M, M],
    video_offset: [M, C, C],
    video_gamma: [M, C, C],
    video_slope_inv: [M, M, C],
    log_brightness_inv_folds: true,
};

/// The Linux wheel's (GCC) orders.
#[cfg(not(windows))]
const ORDERS: Orders = Orders {
    log_brightness_fwd: [C, C, M],
    log_brightness_inv: [M, M, M],
    log_contrast_fwd: [M, C, C],
    log_contrast_inv: [C, C, M],
    log_gamma: [C, C, M],
    lin_offset: [M, M, M],
    lin_exposure: [M, M, M],
    lin_contrast: [C, C, M],
    video_gain: [C, C, M],
    video_offset: [C, C, M],
    video_gamma: [C, C, M],
    video_slope_inv: [C, C, M],
    log_brightness_inv_folds: false,
};

/// `master + channel` in the order `first` says.
fn add(first: First, master: f64, channel: f64) -> f64 {
    match first {
        M => sse_add(master, channel),
        C => sse_add(channel, master),
    }
}

/// `master * channel` in the order `first` says.
fn mul(first: First, master: f64, channel: f64) -> f64 {
    match first {
        M => sse_mul(master, channel),
        C => sse_mul(channel, master),
    }
}

/// The red, green and blue values of an RGBM.
fn channels(v: &GradingRgbm) -> [f64; 3] {
    [v.red, v.green, v.blue]
}

impl GradingPrimaryPreRender {
    /// Computes the values for `style` and `dir` from `v`, in the operand orders of the
    /// wheel's machine code where two NaN values meet ([`ORDERS`]).
    ///
    /// Port of `GradingPrimaryPreRender::update` (src/OpenColorIO/ops/gradingprimary/
    /// GradingPrimary.cpp:108-269 @ v2.5.2).
    pub fn update(&mut self, style: GradingStyle, dir: TransformDirection, v: &GradingPrimary) {
        self.local_bypass = v.saturation == 1.
            && v.clamp_black == GradingPrimary::NO_CLAMP_BLACK
            && v.clamp_white == GradingPrimary::NO_CLAMP_WHITE;
        let o = &ORDERS;

        match style {
            GradingStyle::Log => {
                let b = &v.brightness;
                let c = &v.contrast;
                let g = &v.gamma;
                for i in 0..3 {
                    match dir {
                        TransformDirection::Forward => {
                            let sum = add(o.log_brightness_fwd[i], b.master, channels(b)[i]);
                            self.brightness[i] = (sum * 6.25 / 1023.) as f32;
                            self.contrast[i] =
                                mul(o.log_contrast_fwd[i], c.master, channels(c)[i]) as f32;
                            self.gamma[i] =
                                (1. / mul(o.log_gamma[i], g.master, channels(g)[i])) as f32;
                        }
                        TransformDirection::Inverse => {
                            let sum = add(o.log_brightness_inv[i], b.master, channels(b)[i]);
                            self.brightness[i] = if o.log_brightness_inv_folds {
                                (sum * -6.25 / 1023.) as f32
                            } else {
                                -((sum * 6.25 / 1023.) as f32)
                            };
                            let ci = mul(o.log_contrast_inv[i], c.master, channels(c)[i]);
                            self.contrast[i] = (1. / (if ci == 0. { 1. } else { ci })) as f32;
                            self.gamma[i] = mul(o.log_gamma[i], g.master, channels(g)[i]) as f32;
                        }
                    }
                }
                self.is_power_identity =
                    self.gamma[0] == 1.0 && self.gamma[1] == 1.0 && self.gamma[2] == 1.0;
                self.pivot = 0.5 + v.pivot * 0.5;
                self.local_bypass = self.local_bypass
                    && self.is_power_identity
                    && self.brightness[0] == 0.
                    && self.brightness[1] == 0.
                    && self.brightness[2] == 0.
                    && self.contrast[0] == 1.
                    && self.contrast[1] == 1.
                    && self.contrast[2] == 1.;
            }
            GradingStyle::Lin => {
                let off = &v.offset;
                let e = &v.exposure;
                let c = &v.contrast;
                for i in 0..3 {
                    let offset = add(o.lin_offset[i], off.master, channels(off)[i]) as f32;
                    // std::pow(float, float): powf.
                    let exposure =
                        std_powf(2.0, add(o.lin_exposure[i], e.master, channels(e)[i]) as f32);
                    let contrast = mul(o.lin_contrast[i], c.master, channels(c)[i]);
                    match dir {
                        TransformDirection::Forward => {
                            self.offset[i] = offset;
                            self.exposure[i] = exposure;
                            self.contrast[i] = contrast as f32;
                        }
                        TransformDirection::Inverse => {
                            self.offset[i] = -offset;
                            self.exposure[i] = 1. / exposure;
                            // Validate ensures contrast is above a threshold.
                            self.contrast[i] = (1. / contrast) as f32;
                        }
                    }
                }
                self.is_power_identity =
                    self.contrast[0] == 1.0 && self.contrast[1] == 1.0 && self.contrast[2] == 1.0;
                // std::pow(double, double).
                self.pivot = 0.18 * std_pow(2.0, v.pivot);
                self.local_bypass = self.local_bypass
                    && self.is_power_identity
                    && self.exposure[0] == 1.
                    && self.exposure[1] == 1.
                    && self.exposure[2] == 1.
                    && self.offset[0] == 0.
                    && self.offset[1] == 0.
                    && self.offset[2] == 0.;
            }
            GradingStyle::Video => {
                let off = &v.offset;
                let l = &v.lift;
                let g = &v.gamma;
                for i in 0..3 {
                    let gain = mul(o.video_gain[i], v.gain.master, channels(&v.gain)[i]);
                    let gain = if gain == 0. { 1. } else { gain };
                    let offset = sse_add(
                        sse_add(
                            add(o.video_offset[i], off.master, channels(off)[i]),
                            l.master,
                        ),
                        channels(l)[i],
                    ) as f32;
                    let gamma = mul(o.video_gamma[i], g.master, channels(g)[i]);
                    match dir {
                        TransformDirection::Forward => {
                            self.offset[i] = offset;
                            let slope_den = sse_sub(
                                sse_add(sse_add(v.pivot_white / gain, l.master), channels(l)[i]),
                                v.pivot_black,
                            );
                            let den = if slope_den == 0. { 1. } else { slope_den };
                            self.slope[i] = ((v.pivot_white - v.pivot_black) / den) as f32;
                            self.gamma[i] = (1. / gamma) as f32;
                        }
                        TransformDirection::Inverse => {
                            self.offset[i] = -offset;
                            let lift = sse_sub(
                                add(o.video_slope_inv[i], l.master, channels(l)[i]),
                                v.pivot_black,
                            );
                            // Both wheels add the lift's part first.
                            self.slope[i] = (sse_add(lift, v.pivot_white / gain)
                                / (v.pivot_white - v.pivot_black))
                                as f32;
                            self.gamma[i] = gamma as f32;
                        }
                    }
                }
                self.is_power_identity =
                    self.gamma[0] == 1.0 && self.gamma[1] == 1.0 && self.gamma[2] == 1.0;
                self.local_bypass = self.local_bypass
                    && self.is_power_identity
                    && self.slope[0] == 1.
                    && self.slope[1] == 1.
                    && self.slope[2] == 1.
                    && self.offset[0] == 0.
                    && self.offset[1] == 0.
                    && self.offset[2] == 0.;
            }
        }
    }

    /// Port of `GradingPrimaryPreRender::getLocalBypass` (GradingPrimary.h:26 @ v2.5.2): the
    /// op changes nothing.
    pub fn get_local_bypass(&self) -> bool {
        self.local_bypass
    }

    /// Port of `GradingPrimaryPreRender::getBrightness` (GradingPrimary.h:30 @ v2.5.2).
    pub fn get_brightness(&self) -> &Float3 {
        &self.brightness
    }

    /// Port of `GradingPrimaryPreRender::getContrast` (GradingPrimary.h:31 @ v2.5.2).
    pub fn get_contrast(&self) -> &Float3 {
        &self.contrast
    }

    /// Port of `GradingPrimaryPreRender::getGamma` (GradingPrimary.h:32 @ v2.5.2).
    pub fn get_gamma(&self) -> &Float3 {
        &self.gamma
    }

    /// Port of `GradingPrimaryPreRender::getPivot` (GradingPrimary.h:33 @ v2.5.2).
    pub fn get_pivot(&self) -> f64 {
        self.pivot
    }

    /// Port of `GradingPrimaryPreRender::isGammaIdentity` (GradingPrimary.h:34 @ v2.5.2).
    pub fn is_gamma_identity(&self) -> bool {
        self.is_power_identity
    }

    /// Port of `GradingPrimaryPreRender::getExposure` (GradingPrimary.h:36 @ v2.5.2).
    pub fn get_exposure(&self) -> &Float3 {
        &self.exposure
    }

    /// Port of `GradingPrimaryPreRender::getOffset` (GradingPrimary.h:37 @ v2.5.2).
    pub fn get_offset(&self) -> &Float3 {
        &self.offset
    }

    /// Port of `GradingPrimaryPreRender::isContrastIdentity` (GradingPrimary.h:38 @ v2.5.2).
    pub fn is_contrast_identity(&self) -> bool {
        self.is_power_identity
    }

    /// Port of `GradingPrimaryPreRender::getSlope` (GradingPrimary.h:40 @ v2.5.2).
    pub fn get_slope(&self) -> &Float3 {
        &self.slope
    }
}

#[cfg(test)]
#[path = "grading_primary_tests.rs"]
mod tests;
