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
use crate::math_utils::{std_pow, std_powf};
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

impl GradingPrimaryPreRender {
    /// Computes the values for `style` and `dir` from `v`.
    ///
    /// Port of `GradingPrimaryPreRender::update` (src/OpenColorIO/ops/gradingprimary/
    /// GradingPrimary.cpp:108-269 @ v2.5.2).
    pub fn update(&mut self, style: GradingStyle, dir: TransformDirection, v: &GradingPrimary) {
        self.local_bypass = v.saturation == 1.
            && v.clamp_black == GradingPrimary::NO_CLAMP_BLACK
            && v.clamp_white == GradingPrimary::NO_CLAMP_WHITE;

        match style {
            GradingStyle::Log => {
                let b = &v.brightness;
                let c = &v.contrast;
                let g = &v.gamma;
                match dir {
                    TransformDirection::Forward => {
                        self.brightness[0] = ((b.master + b.red) * 6.25 / 1023.) as f32;
                        self.brightness[1] = ((b.master + b.green) * 6.25 / 1023.) as f32;
                        self.brightness[2] = ((b.master + b.blue) * 6.25 / 1023.) as f32;
                        self.contrast[0] = (c.master * c.red) as f32;
                        self.contrast[1] = (c.master * c.green) as f32;
                        self.contrast[2] = (c.master * c.blue) as f32;
                        self.gamma[0] = (1. / (g.master * g.red)) as f32;
                        self.gamma[1] = (1. / (g.master * g.green)) as f32;
                        self.gamma[2] = (1. / (g.master * g.blue)) as f32;
                    }
                    TransformDirection::Inverse => {
                        self.brightness[0] = -(((b.master + b.red) * 6.25 / 1023.) as f32);
                        self.brightness[1] = -(((b.master + b.green) * 6.25 / 1023.) as f32);
                        self.brightness[2] = -(((b.master + b.blue) * 6.25 / 1023.) as f32);
                        let c0 = c.master * c.red;
                        let c1 = c.master * c.green;
                        let c2 = c.master * c.blue;
                        self.contrast[0] = (1. / (if c0 == 0. { 1. } else { c0 })) as f32;
                        self.contrast[1] = (1. / (if c1 == 0. { 1. } else { c1 })) as f32;
                        self.contrast[2] = (1. / (if c2 == 0. { 1. } else { c2 })) as f32;
                        self.gamma[0] = (g.master * g.red) as f32;
                        self.gamma[1] = (g.master * g.green) as f32;
                        self.gamma[2] = (g.master * g.blue) as f32;
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
                let o = &v.offset;
                let e = &v.exposure;
                let c = &v.contrast;
                match dir {
                    TransformDirection::Forward => {
                        self.offset[0] = (o.master + o.red) as f32;
                        self.offset[1] = (o.master + o.green) as f32;
                        self.offset[2] = (o.master + o.blue) as f32;
                        // std::pow(float, float): powf.
                        self.exposure[0] = std_powf(2.0, (e.master + e.red) as f32);
                        self.exposure[1] = std_powf(2.0, (e.master + e.green) as f32);
                        self.exposure[2] = std_powf(2.0, (e.master + e.blue) as f32);
                        self.contrast[0] = (c.master * c.red) as f32;
                        self.contrast[1] = (c.master * c.green) as f32;
                        self.contrast[2] = (c.master * c.blue) as f32;
                    }
                    TransformDirection::Inverse => {
                        self.offset[0] = -((o.master + o.red) as f32);
                        self.offset[1] = -((o.master + o.green) as f32);
                        self.offset[2] = -((o.master + o.blue) as f32);
                        self.exposure[0] = 1. / std_powf(2.0, (e.master + e.red) as f32);
                        self.exposure[1] = 1. / std_powf(2.0, (e.master + e.green) as f32);
                        self.exposure[2] = 1. / std_powf(2.0, (e.master + e.blue) as f32);
                        // Validate ensures contrast is above a threshold.
                        self.contrast[0] = (1. / (c.master * c.red)) as f32;
                        self.contrast[1] = (1. / (c.master * c.green)) as f32;
                        self.contrast[2] = (1. / (c.master * c.blue)) as f32;
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
                let o = &v.offset;
                let l = &v.lift;
                let gain = |g: f64| if g == 0. { 1. } else { g };
                let gain0 = gain(v.gain.master * v.gain.red);
                let gain1 = gain(v.gain.master * v.gain.green);
                let gain2 = gain(v.gain.master * v.gain.blue);
                let g = &v.gamma;
                match dir {
                    TransformDirection::Forward => {
                        self.offset[0] = (o.master + o.red + l.master + l.red) as f32;
                        self.offset[1] = (o.master + o.green + l.master + l.green) as f32;
                        self.offset[2] = (o.master + o.blue + l.master + l.blue) as f32;
                        let slope_den0 = v.pivot_white / gain0 + l.master + l.red - v.pivot_black;
                        let slope_den1 = v.pivot_white / gain1 + l.master + l.green - v.pivot_black;
                        let slope_den2 = v.pivot_white / gain2 + l.master + l.blue - v.pivot_black;
                        let den = |d: f64| if d == 0. { 1. } else { d };
                        self.slope[0] = ((v.pivot_white - v.pivot_black) / den(slope_den0)) as f32;
                        self.slope[1] = ((v.pivot_white - v.pivot_black) / den(slope_den1)) as f32;
                        self.slope[2] = ((v.pivot_white - v.pivot_black) / den(slope_den2)) as f32;
                        self.gamma[0] = (1. / (g.master * g.red)) as f32;
                        self.gamma[1] = (1. / (g.master * g.green)) as f32;
                        self.gamma[2] = (1. / (g.master * g.blue)) as f32;
                    }
                    TransformDirection::Inverse => {
                        self.offset[0] = -((o.master + o.red + l.master + l.red) as f32);
                        self.offset[1] = -((o.master + o.green + l.master + l.green) as f32);
                        self.offset[2] = -((o.master + o.blue + l.master + l.blue) as f32);
                        self.slope[0] = ((v.pivot_white / gain0
                            + (l.master + l.red - v.pivot_black))
                            / (v.pivot_white - v.pivot_black))
                            as f32;
                        self.slope[1] = ((v.pivot_white / gain1
                            + (l.master + l.green - v.pivot_black))
                            / (v.pivot_white - v.pivot_black))
                            as f32;
                        self.slope[2] = ((v.pivot_white / gain2
                            + (l.master + l.blue - v.pivot_black))
                            / (v.pivot_white - v.pivot_black))
                            as f32;
                        self.gamma[0] = (g.master * g.red) as f32;
                        self.gamma[1] = (g.master * g.green) as f32;
                        self.gamma[2] = (g.master * g.blue) as f32;
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
