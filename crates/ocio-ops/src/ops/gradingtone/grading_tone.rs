// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The values of the GradingTone op and their pre-render form. A port of
//! `src/OpenColorIO/ops/gradingtone/GradingTone.h` and `GradingTone.cpp` @ v2.5.2, of the public
//! `GradingRGBMSW` and `GradingTone` (include/OpenColorIO/OpenColorTransforms.h:642-708), and of
//! their `operator<<` (src/OpenColorIO/transforms/GradingToneTransform.cpp:127-146).
//!
//! The values are plain Rust values with public fields, as upstream's structs (owner decision
//! P5-2): `==` is upstream's `operator==`, exact on the doubles.

use std::fmt;

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::math_utils::{clamp, std_max, std_min};
use crate::open_color_types::GradingStyle;

/// The red, green, blue and master components of one tone parameter, with the start (or the
/// center, for the midtones) and the width (or the pivot, for the shadows and highlights) of
/// the range it acts on.
///
/// Port of `GradingRGBMSW` (include/OpenColorIO/OpenColorTransforms.h:642-674 @ v2.5.2) and its
/// `operator==` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:15-20).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingRgbmsw {
    /// `m_red`.
    pub red: f64,
    /// `m_green`.
    pub green: f64,
    /// `m_blue`.
    pub blue: f64,
    /// `m_master`.
    pub master: f64,
    /// `m_start`: or the center, for the midtones.
    pub start: f64,
    /// `m_width`: or the pivot, for the shadows and highlights.
    pub width: f64,
}

impl Default for GradingRgbmsw {
    /// Port of `GradingRGBMSW()` (OpenColorTransforms.h:644, 668-673 @ v2.5.2): 1, 1, 1, 1, a
    /// start of 0 and a width of 1.
    fn default() -> Self {
        GradingRgbmsw::new(1., 1., 1., 1., 0., 1.)
    }
}

impl GradingRgbmsw {
    /// Port of `GradingRGBMSW(double red, double green, double blue, double master, double
    /// start, double width)` (OpenColorTransforms.h:645-652 @ v2.5.2).
    pub fn new(red: f64, green: f64, blue: f64, master: f64, start: f64, width: f64) -> Self {
        GradingRgbmsw {
            red,
            green,
            blue,
            master,
            start,
            width,
        }
    }

    /// Port of `GradingRGBMSW(const double(&rgbmsw)[6])` (OpenColorTransforms.h:653-661 @
    /// v2.5.2).
    pub fn from_array(v: &[f64; 6]) -> Self {
        GradingRgbmsw::new(v[0], v[1], v[2], v[3], v[4], v[5])
    }

    /// The components 1, 1, 1, 1 with this start and width.
    ///
    /// Port of `GradingRGBMSW(double start, double width)` (OpenColorTransforms.h:662-666 @
    /// v2.5.2).
    pub fn with_start_width(start: f64, width: f64) -> Self {
        GradingRgbmsw::new(1., 1., 1., 1., start, width)
    }

    /// `os << rgbmsw`: `<red=... green=... blue=... master=... start=... width=...>`.
    ///
    /// Port of `operator<<(std::ostream &, const GradingRGBMSW &)`
    /// (src/OpenColorIO/transforms/GradingToneTransform.cpp:127-133 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        os.put_str("<red=");
        os.put_f64(self.red);
        os.put_str(" green=");
        os.put_f64(self.green);
        os.put_str(" blue=");
        os.put_f64(self.blue);
        os.put_str(" master=");
        os.put_f64(self.master);
        os.put_str(" start=");
        os.put_f64(self.start);
        os.put_str(" width=");
        os.put_f64(self.width);
        os.put_str(">");
    }
}

/// `operator<<` with a new stream's format (6 significant digits).
impl fmt::Display for GradingRgbmsw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// The values of a GradingTone op.
///
/// Port of `GradingTone` (include/OpenColorIO/OpenColorTransforms.h:681-708 @ v2.5.2) and its
/// `operator==` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:27-35).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingTone {
    /// `m_blacks`.
    pub blacks: GradingRgbmsw,
    /// `m_shadows`.
    pub shadows: GradingRgbmsw,
    /// `m_midtones`.
    pub midtones: GradingRgbmsw,
    /// `m_highlights`.
    pub highlights: GradingRgbmsw,
    /// `m_whites`.
    pub whites: GradingRgbmsw,
    /// `m_scontrast`.
    pub scontrast: f64,
}

impl GradingTone {
    /// The values that change nothing, with the ranges of `style`.
    ///
    /// Port of `GradingTone(GradingStyle)` (include/OpenColorIO/OpenColorTransforms.h:684-700 @
    /// v2.5.2).
    pub fn new(style: GradingStyle) -> Self {
        let sw = GradingRgbmsw::with_start_width;
        let (blacks, shadows, midtones, highlights, whites) = match style {
            GradingStyle::Lin => (sw(0., 4.), sw(2., -7.), sw(0., 8.), sw(-2., 9.), sw(0., 8.)),
            GradingStyle::Log => (
                sw(0.4, 0.4),
                sw(0.5, 0.),
                sw(0.4, 0.6),
                sw(0.3, 1.),
                sw(0.4, 0.5),
            ),
            GradingStyle::Video => (
                sw(0.4, 0.4),
                sw(0.6, 0.),
                sw(0.4, 0.7),
                sw(0.2, 1.),
                sw(0.5, 0.5),
            ),
        };
        GradingTone {
            blacks,
            shadows,
            midtones,
            highlights,
            whites,
            scontrast: 1.0,
        }
    }

    /// Checks that each value is in the range a client is expected to keep it in: blacks,
    /// midtones and whites in [0.1, 1.9], shadows and highlights in [0.2, 1.8], widths at least
    /// 0.01, s-contrast in [0.01, 1.99], each widened by 1e-6, and the shadows' and highlights'
    /// starts clear of their pivots. A NaN passes these checks, as upstream's comparisons let
    /// it.
    ///
    /// Port of `GradingTone::validate` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:42-236
    /// @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // Client app is expected to limit these to these stated bounds.
        //
        // Blacks, mids, whites: [0.1, 1.9]
        // Shadows, highlights: [0.2, 1.8]
        // Min width: 0.01.
        // SContrast: [0.01, 1.99].
        const MIN_BMW: f64 = 0.1;
        const MAX_BMW: f64 = 1.9;
        const MIN_SH: f64 = 0.2;
        const MAX_SH: f64 = 1.8;
        const MIN_WSC: f64 = 0.01;
        const MAX_SC: f64 = 1.99;
        // The bounds are widened here slightly to avoid failures due to precision issues.
        const ERROR: f64 = 0.000001;
        const MIN_BMW_TOL: f64 = MIN_BMW - ERROR;
        const MAX_BMW_TOL: f64 = MAX_BMW + ERROR;
        const MIN_SH_TOL: f64 = MIN_SH - ERROR;
        const MAX_SH_TOL: f64 = MAX_SH + ERROR;
        const MIN_WSC_TOL: f64 = MIN_WSC - ERROR;
        const MAX_SC_TOL: f64 = MAX_SC + ERROR;

        let below =
            |v: &GradingRgbmsw, min| v.red < min || v.green < min || v.blue < min || v.master < min;
        let above =
            |v: &GradingRgbmsw, max| v.red > max || v.green > max || v.blue > max || v.master > max;
        // "GradingTone <name> '<value>' are <below lower|above upper> bound (<bound>)."
        let values_error = |name: &str, v: &GradingRgbmsw, which: &str, bound: f64| {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("GradingTone ");
            oss.put_str(name);
            oss.put_str(" '");
            v.write_to(&mut oss);
            oss.put_str("' are ");
            oss.put_str(which);
            oss.put_str(" bound (");
            oss.put_f64(bound);
            oss.put_str(").");
            Err(Exception::new(oss.into_bytes()))
        };
        // "GradingTone <name> '<value>' is <below lower|above upper> bound (<bound>)."
        let value_error = |name: &str, v: f64, which: &str, bound: f64| {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("GradingTone ");
            oss.put_str(name);
            oss.put_str(" '");
            oss.put_f64(v);
            oss.put_str("' is ");
            oss.put_str(which);
            oss.put_str(" bound (");
            oss.put_f64(bound);
            oss.put_str(").");
            Err(Exception::new(oss.into_bytes()))
        };

        // Blacks, midtones and whites: the values, the width, then the upper bound.
        for (name, width_name, upper_name, v) in [
            ("blacks", "blacks width", "blacks", &self.blacks),
            ("midtones", "midtones width", "midtones", &self.midtones),
            // Upstream's message says "white" for the upper bound.
            ("whites", "whites width", "white", &self.whites),
        ] {
            if below(v, MIN_BMW_TOL) {
                return values_error(name, v, "below lower", MIN_BMW);
            }
            if v.width < MIN_WSC_TOL {
                return value_error(width_name, v.width, "below lower", MIN_WSC);
            }
            if above(v, MAX_BMW_TOL) {
                return values_error(upper_name, v, "above upper", MAX_BMW);
            }
        }
        {
            let shadows = &self.shadows;
            if below(shadows, MIN_SH_TOL) {
                return values_error("shadows", shadows, "below lower", MIN_SH);
            }
            // Check that pivot is not overlapping start.
            if shadows.start < shadows.width + MIN_WSC_TOL {
                let mut oss = OStringStream::new(Crt::NATIVE);
                oss.put_str("GradingTone shadows start '");
                oss.put_f64(shadows.start);
                oss.put_str("' is less than pivot ('");
                oss.put_f64(shadows.width);
                oss.put_str("' + ");
                oss.put_f64(MIN_WSC);
                oss.put_str(").");
                return Err(Exception::new(oss.into_bytes()));
            }
            if above(shadows, MAX_SH_TOL) {
                return values_error("shadows", shadows, "above upper", MAX_SH);
            }
        }
        {
            let hl = &self.highlights;
            if below(hl, MIN_SH_TOL) {
                return values_error("highlights", hl, "below lower", MIN_SH);
            }
            // Check that pivot is not overlapping start.
            if hl.start > hl.width - MIN_WSC_TOL {
                let mut oss = OStringStream::new(Crt::NATIVE);
                oss.put_str("GradingTone highlights start '");
                oss.put_f64(hl.start);
                oss.put_str("' is greater than pivot ('");
                oss.put_f64(hl.width);
                oss.put_str("' - ");
                oss.put_f64(MIN_WSC);
                oss.put_str(").");
                return Err(Exception::new(oss.into_bytes()));
            }
            if above(hl, MAX_SH_TOL) {
                return values_error("highlights", hl, "above upper", MAX_SH);
            }
        }
        if self.scontrast < MIN_WSC_TOL {
            return value_error("s-contrast", self.scontrast, "below lower", MIN_WSC);
        }
        if self.scontrast > MAX_SC_TOL {
            return value_error("s-contrast", self.scontrast, "above upper", MAX_SC);
        }
        Ok(())
    }

    /// `os << tone`: `<blacks=... shadows=... midtones=... highlights=... whites=...
    /// s_contrast=...>`.
    ///
    /// Port of `operator<<(std::ostream &, const GradingTone &)`
    /// (src/OpenColorIO/transforms/GradingToneTransform.cpp:135-146 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        os.put_str("<blacks=");
        self.blacks.write_to(os);
        os.put_str(" shadows=");
        self.shadows.write_to(os);
        os.put_str(" midtones=");
        self.midtones.write_to(os);
        os.put_str(" highlights=");
        self.highlights.write_to(os);
        os.put_str(" whites=");
        self.whites.write_to(os);
        os.put_str(" s_contrast=");
        os.put_f64(self.scontrast);
        os.put_str(">");
    }
}

/// `operator<<` with a new stream's format (6 significant digits).
impl fmt::Display for GradingTone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// A component of a [`GradingRgbmsw`].
///
/// Port of `RGBMChannel` (src/OpenColorIO/ops/gradingtone/GradingTone.h:13-19 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RgbmChannel {
    /// `R`.
    R = 0,
    /// `G`.
    G,
    /// `B`.
    B,
    /// `M`.
    M,
}

impl RgbmChannel {
    /// The four, in order.
    pub const ALL: [RgbmChannel; 4] = [
        RgbmChannel::R,
        RgbmChannel::G,
        RgbmChannel::B,
        RgbmChannel::M,
    ];
}

/// The component `channel` of `value`, as a float.
///
/// Port of `GetChannelValue` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:238-257 @
/// v2.5.2).
pub fn get_channel_value(value: &GradingRgbmsw, channel: RgbmChannel) -> f32 {
    match channel {
        RgbmChannel::R => value.red as f32,
        RgbmChannel::G => value.green as f32,
        RgbmChannel::B => value.blue as f32,
        RgbmChannel::M => value.master as f32,
    }
}

/// Port of the anonymous `IsIdentity(const GradingRGBMSW &)` (GradingTone.cpp:261-264 @
/// v2.5.2).
fn rgbmsw_is_identity(val: &GradingRgbmsw) -> bool {
    val.red == 1. && val.green == 1. && val.blue == 1. && val.master == 1.
}

/// Whether the values change nothing: every component 1 and an s-contrast of 1.
///
/// Port of `IsIdentity(const GradingTone &)` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:
/// 266-271 @ v2.5.2).
pub fn is_identity(value: &GradingTone) -> bool {
    rgbmsw_is_identity(&value.blacks)
        && rgbmsw_is_identity(&value.shadows)
        && rgbmsw_is_identity(&value.midtones)
        && rgbmsw_is_identity(&value.highlights)
        && rgbmsw_is_identity(&value.whites)
        && value.scontrast == 1.
}

/// The constants the CPU (and in part the GPU) renderers use, computed from a [`GradingTone`]
/// for a style: the ranges of each tone curve and the control points and slopes of its
/// segments.
///
/// Port of `GradingTonePreRender` (src/OpenColorIO/ops/gradingtone/GradingTone.h:25-86 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradingTonePreRender {
    // These values are used by CPU & GPU.
    /// `m_shadowsStart`.
    pub shadows_start: f64,
    /// `m_shadowsWidth`.
    pub shadows_width: f64,
    /// `m_highlightsStart`.
    pub highlights_start: f64,
    /// `m_highlightsWidth`.
    pub highlights_width: f64,
    /// `m_blacksStart`.
    pub blacks_start: f64,
    /// `m_blacksWidth`.
    pub blacks_width: f64,
    /// `m_whitesStart`.
    pub whites_start: f64,
    /// `m_whitesWidth`.
    pub whites_width: f64,

    // Arrays are currently only used by CPU.
    /// `m_midX`.
    pub mid_x: [[f32; 6]; 4],
    /// `m_midY`.
    pub mid_y: [[f32; 6]; 4],
    /// `m_midM`.
    pub mid_m: [[f32; 6]; 4],

    /// `m_hsX`.
    pub hs_x: [[[f32; 3]; 4]; 2],
    /// `m_hsY`.
    pub hs_y: [[[f32; 3]; 4]; 2],
    /// `m_hsM`: m1 not used, `[..][..][1]` is m2.
    pub hs_m: [[[f32; 2]; 4]; 2],

    /// `m_wbX`.
    pub wb_x: [[[f32; 2]; 4]; 2],
    /// `m_wbY`.
    pub wb_y: [[[f32; 2]; 4]; 2],
    /// `m_wbM`.
    pub wb_m: [[[f32; 2]; 4]; 2],
    /// `m_wbGain`.
    pub wb_gain: [[f32; 4]; 2],

    /// `m_scX`: top/bottom, 4 values.
    pub sc_x: [[f32; 4]; 2],
    /// `m_scY`.
    pub sc_y: [[f32; 4]; 2],
    /// `m_scM`: m0 & m3.
    pub sc_m: [[f32; 2]; 2],

    // These values are changing with the style.
    /// `m_top`.
    pub top: f32,
    /// `m_topSC`.
    pub top_sc: f32,
    /// `m_bottom`.
    pub bottom: f32,
    /// `m_pivot`.
    pub pivot: f32,

    /// `m_localBypass`: do not apply the op if all params are identity.
    pub local_bypass: bool,

    style: GradingStyle,
}

impl GradingTonePreRender {
    /// The constants of the defaults, for `style`: only the style's top, bottom and pivot are
    /// set; the rest waits for [`update`](Self::update).
    ///
    /// Port of `GradingTonePreRender::GradingTonePreRender(GradingStyle)`
    /// (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:273-276 @ v2.5.2) and of the
    /// members' initializers (GradingTone.h:35-84).
    pub fn new(style: GradingStyle) -> Self {
        let mut res = GradingTonePreRender {
            shadows_start: 0.,
            shadows_width: 0.,
            highlights_start: 0.,
            highlights_width: 0.,
            blacks_start: 0.,
            blacks_width: 0.,
            whites_start: 0.,
            whites_width: 0.,
            mid_x: [[0.; 6]; 4],
            mid_y: [[0.; 6]; 4],
            mid_m: [[0.; 6]; 4],
            hs_x: [[[0.; 3]; 4]; 2],
            hs_y: [[[0.; 3]; 4]; 2],
            hs_m: [[[0.; 2]; 4]; 2],
            wb_x: [[[0.; 2]; 4]; 2],
            wb_y: [[[0.; 2]; 4]; 2],
            wb_m: [[[0.; 2]; 4]; 2],
            wb_gain: [[0.; 4]; 2],
            sc_x: [[0.; 4]; 2],
            sc_y: [[0.; 4]; 2],
            sc_m: [[0.; 2]; 2],
            top: 1.,
            top_sc: 1.,
            bottom: 0.,
            pivot: 0.4,
            local_bypass: false,
            style: GradingStyle::Log,
        };
        res.set_style(style);
        res
    }

    /// The top, s-contrast top, bottom and pivot of `style`.
    ///
    /// Port of `GradingTonePreRender::FromStyle` (src/OpenColorIO/ops/gradingtone/
    /// GradingTone.cpp:278-305 @ v2.5.2).
    pub fn from_style(style: GradingStyle) -> (f32, f32, f32, f32) {
        match style {
            // Might like to move these for ACES, but cannot for ARRI K1S1.
            GradingStyle::Log => (1., 1., 0., 0.4),
            // Bottom at the breakpoint of lin-to-log.
            GradingStyle::Lin => (7.5, 6.5, -5.5, 0.),
            // Pivot: aces 0.18 --> 0.39.
            GradingStyle::Video => (1., 1., 0., 0.4),
        }
    }

    /// Changes the style and its top, bottom and pivot, when it differs.
    ///
    /// Port of `GradingTonePreRender::setStyle` (src/OpenColorIO/ops/gradingtone/
    /// GradingTone.cpp:307-314 @ v2.5.2).
    pub fn set_style(&mut self, style: GradingStyle) {
        if self.style != style {
            self.style = style;
            (self.top, self.top_sc, self.bottom, self.pivot) = Self::from_style(style);
        }
    }

    /// Computes the constants from `v`. Values that change nothing leave the constants as they
    /// were and only set the bypass, as upstream's do.
    ///
    /// Port of `GradingTonePreRender::update` (src/OpenColorIO/ops/gradingtone/
    /// GradingTone.cpp:411-458 @ v2.5.2).
    pub fn update(&mut self, v: &GradingTone) {
        self.local_bypass = is_identity(v);
        if self.local_bypass {
            return;
        }

        {
            let master = v.highlights.master;
            let start = v.highlights.start;
            let pivot = v.highlights.width;
            let startw = v.whites.start;
            let widthw = v.whites.width;

            self.highlights_start = if start > pivot - 0.01 {
                pivot - 0.01
            } else {
                start
            };
            self.highlights_width = pivot;

            let new_start =
                highlight_fwd_eval(startw, self.highlights_start, self.highlights_width, master);
            let new_end = highlight_fwd_eval(
                startw + widthw,
                self.highlights_start,
                self.highlights_width,
                master,
            );
            self.whites_start = new_start;
            self.whites_width = new_end - new_start;
        }
        {
            let master = v.shadows.master;
            let start = v.shadows.start;
            let pivot = v.shadows.width;
            let startb = v.blacks.start;
            let widthb = v.blacks.width;

            self.shadows_start = if start < pivot + 0.01 {
                pivot + 0.01
            } else {
                start
            };
            self.shadows_width = pivot;

            let new_start = shadow_fwd_eval(startb, self.shadows_width, self.shadows_start, master);
            let new_end = shadow_fwd_eval(
                startb - widthb,
                self.shadows_width,
                self.shadows_start,
                master,
            );
            self.blacks_start = new_start;
            self.blacks_width = new_start - new_end;
        }

        let (top, bottom, top_sc, pivot) = (self.top, self.bottom, self.top_sc, self.pivot);
        self.mids_precompute(v, top, bottom);
        self.highlight_shadow_precompute(v);
        self.white_black_precompute(v);
        self.scontrast_precompute(v, top_sc, bottom, pivot);
    }

    /// Port of `GradingTonePreRender::mids_precompute` (src/OpenColorIO/ops/gradingtone/
    /// GradingTone.cpp:460-536 @ v2.5.2).
    fn mids_precompute(&mut self, v: &GradingTone, top: f32, bottom: f32) {
        const HALO: f32 = 0.4;

        for channel in RgbmChannel::ALL {
            let c = channel as usize;
            let mut mid_adj = clamp(get_channel_value(&v.midtones, channel), 0.01f32, 1.99f32);

            if mid_adj != 1. {
                let [x0, x1, x2, x3, x4, x5] = &mut self.mid_x[c];
                let [y0, y1, y2, y3, y4, y5] = &mut self.mid_y[c];
                let [m0, m1, m2, m3, m4, m5] = &mut self.mid_m[c];

                *x0 = bottom;
                *x5 = top;

                let max_width = (*x5 - *x0) * 0.95f32;
                let width = clamp(v.midtones.width as f32, 0.01f32, max_width);
                let min_cent = *x0 + width * 0.51f32;
                let max_cent = *x5 - width * 0.51f32;
                let center = clamp(v.midtones.start as f32, min_cent, max_cent);

                *x1 = center - width * 0.5f32;
                *x4 = *x1 + width;

                *x2 = *x1 + (*x4 - *x1) * 0.25f32;
                *x3 = *x1 + (*x4 - *x1) * 0.75f32;
                *y0 = *x0;
                *m0 = 1.;
                *m5 = 1.;

                let min_slope = 0.1f32;

                mid_adj -= 1.;
                mid_adj *= 1. - min_slope;

                *m2 = 1. + mid_adj;
                *m3 = 1. - mid_adj;
                *m1 = 1. + mid_adj * HALO;
                *m4 = 1. - mid_adj * HALO;

                if center <= (*x5 + *x0) * 0.5f32 {
                    let area = (*x1 - *x0) * (*m1 - *m0) * 0.5f32
                        + (*x2 - *x1) * ((*m1 - *m0) + (*m2 - *m1) * 0.5f32)
                        + (center - *x2) * (*m2 - *m0) * 0.5f32;
                    *m4 = (-0.5f32 * (*x5 - *x4) * *m5
                        + (*x4 - *x3) * (0.5f32 * *m3 - *m5)
                        + (*x3 - center) * (*m3 - *m5) * 0.5f32
                        + area)
                        / (-0.5f32 * (*x5 - *x3));
                } else {
                    let area = (*x5 - *x4) * (*m4 - *m5) * 0.5f32
                        + (*x4 - *x3) * ((*m4 - *m5) + (*m3 - *m4) * 0.5f32)
                        + (*x3 - center) * (*m3 - *m5) * 0.5f32;
                    *m1 = (-0.5f32 * (*x1 - *x0) * *m0
                        + (*x2 - *x1) * (0.5f32 * *m2 - *m0)
                        + (center - *x2) * (*m2 - *m0) * 0.5f32
                        + area)
                        / (-0.5f32 * (*x2 - *x0));
                }

                *y1 = *y0 + (*m0 + *m1) * (*x1 - *x0) * 0.5f32;
                *y2 = *y1 + (*m1 + *m2) * (*x2 - *x1) * 0.5f32;
                *y3 = *y2 + (*m2 + *m3) * (*x3 - *x2) * 0.5f32;
                *y4 = *y3 + (*m3 + *m4) * (*x4 - *x3) * 0.5f32;
                *y5 = *y4 + (*m4 + *m5) * (*x5 - *x4) * 0.5f32;
            }
        }
    }

    /// Port of `GradingTonePreRender::highlightShadow_precompute` (src/OpenColorIO/ops/
    /// gradingtone/GradingTone.cpp:538-590 @ v2.5.2).
    fn highlight_shadow_precompute(&mut self, v: &GradingTone) {
        for is_shadow in [false, true] {
            let s = usize::from(is_shadow);
            for channel in RgbmChannel::ALL {
                let c = channel as usize;
                let mut val = if is_shadow {
                    get_channel_value(&v.shadows, channel)
                } else {
                    get_channel_value(&v.highlights, channel)
                };
                if !is_shadow {
                    val = 2. - val;
                }
                if val != 1. {
                    let start = (if is_shadow {
                        self.shadows_start
                    } else {
                        self.highlights_start
                    }) as f32;
                    let pivot = (if is_shadow {
                        self.shadows_width
                    } else {
                        self.highlights_width
                    }) as f32;

                    let [x0, x1, x2] = &mut self.hs_x[s][c];
                    let [y0, y1, y2] = &mut self.hs_y[s][c];
                    let [m0, m2] = &mut self.hs_m[s][c];

                    *x0 = if is_shadow { pivot } else { start };
                    *x2 = if is_shadow { start } else { pivot };
                    *y0 = *x0;
                    *y2 = *x2;
                    *x1 = *x0 + (*x2 - *x0) * 0.5f32;

                    if val < 1. {
                        *m0 = if is_shadow { std_max(0.01f32, val) } else { 1. };
                        *m2 = if is_shadow { 1. } else { std_max(0.01f32, val) };
                        *y1 = (0.5f32 / (*x2 - *x0))
                            * ((2.0f32 * *y0 + *m0 * (*x1 - *x0)) * (*x2 - *x1)
                                + (2.0f32 * *y2 - *m2 * (*x2 - *x1)) * (*x1 - *x0));
                    } else if val > 1. {
                        *m0 = if is_shadow {
                            std_max(0.01f32, 2. - val)
                        } else {
                            1.
                        };
                        *m2 = if is_shadow {
                            1.
                        } else {
                            std_max(0.01f32, 2. - val)
                        };
                        *y1 = (0.5f32 / ((*x2 - *x1) + (*x1 - *x0)))
                            * ((2.0f32 * *y0 + *m0 * (*x1 - *x0)) * (*x2 - *x1)
                                + (2.0f32 * *y2 - *m2 * (*x2 - *x1)) * (*x1 - *x0));
                    }
                }
            }
        }
    }

    /// Port of `GradingTonePreRender::whiteBlack_precompute` (src/OpenColorIO/ops/gradingtone/
    /// GradingTone.cpp:592-666 @ v2.5.2).
    fn white_black_precompute(&mut self, v: &GradingTone) {
        for is_black in [false, true] {
            let b = usize::from(is_black);
            for channel in RgbmChannel::ALL {
                let c = channel as usize;
                let start = (if is_black {
                    self.blacks_start
                } else {
                    self.whites_start
                }) as f32;
                let width = (if is_black {
                    self.blacks_width
                } else {
                    self.whites_width
                }) as f32;

                let val = if is_black {
                    get_channel_value(&v.blacks, channel)
                } else {
                    get_channel_value(&v.whites, channel)
                };

                let [x0, x1] = &mut self.wb_x[b][c];
                let [y0, y1] = &mut self.wb_y[b][c];
                let [m0, m1] = &mut self.wb_m[b][c];
                let gain = &mut self.wb_gain[b][c];

                *x0 = if !is_black { start } else { start - width };
                *x1 = if !is_black { *x0 + width } else { start };

                let mtest = if !is_black { val } else { 2. - val };

                if mtest < 1. {
                    // Slope is decreasing case.
                    if !is_black {
                        *m0 = 1.;
                        *m1 = std_max(0.01f32, val);
                        *y0 = *x0; // for !isBlack
                        *y1 = *y0 + (*m0 + *m1) * (*x1 - *x0) * 0.5f32;
                    } else {
                        *m0 = 2. - val;
                        *m0 = std_max(0.01f32, *m0);
                        *m1 = 1.;
                        *y1 = *x1; // for isBlack
                        *y0 = *y1 - (*m0 + *m1) * (*x1 - *x0) * 0.5f32;
                    }
                } else if mtest > 1. {
                    // Slope is increasing case.
                    if !is_black {
                        *m0 = 1.;
                        *m1 = 2. - val;
                        *m1 = std_max(0.01f32, *m1);
                        *y0 = *x0; // for !isBlack
                    // y1 won't be used.
                    } else {
                        *m0 = val;
                        *m0 = std_max(0.01f32, *m0);
                        *m1 = 1.;
                        *y1 = *x1; // for isBlack
                        *y0 = *y1 - (*m0 + *m1) * (*x1 - *x0) * 0.5f32;
                    }
                    *gain = (*m0 + *m1) * 0.5f32;
                }
            }
        }
    }

    /// Port of `GradingTonePreRender::scontrast_precompute` (src/OpenColorIO/ops/gradingtone/
    /// GradingTone.cpp:668-749 @ v2.5.2).
    fn scontrast_precompute(&mut self, v: &GradingTone, top_sc: f32, bottom: f32, pivot: f32) {
        let mut contrast = v.scontrast as f32;
        if contrast != 1. {
            // Limit the range of values to prevent reversals.
            contrast = if contrast > 1. {
                1. / (1.8125f32 - 0.8125f32 * std_min(contrast, 1.99f32))
            } else {
                0.28125f32 + 0.71875f32 * std_max(contrast, 0.01f32)
            };

            // Top end
            {
                let [x0, x1, x2, x3] = &mut self.sc_x[0];
                let [y0, y1, y2, y3] = &mut self.sc_y[0];
                let [m0, m3] = &mut self.sc_m[0];

                *x3 = top_sc;
                *y3 = top_sc;
                *y0 = pivot + (*y3 - pivot) * 0.25f32;
                *m0 = contrast;
                *x0 = pivot + (*y0 - pivot) / *m0;
                let min_width = (*x3 - *x0) * 0.3f32;
                *m3 = 1. / *m0;
                // NB: Due to the if (contrast != 1.) clause above, m0 != m3.
                let center = (*y3 - *y0 - *m3 * *x3 + *m0 * *x0) / (*m0 - *m3);
                *x1 = *x0;
                *x2 = 2.0f32 * center - *x1;
                if *x2 > *x3 {
                    *x2 = *x3;
                    *x1 = 2.0f32 * center - *x2;
                } else if (*x2 - *x1) < min_width {
                    *x2 = *x1 + min_width;
                    let new_center = (*x2 + *x1) * 0.5f32;
                    *m3 = (*y3 - *y0 + *m0 * *x0 - new_center * *m0) / (*x3 - new_center);
                }
                *y1 = *y0;
                *y2 = *y1 + (*m0 + *m3) * (*x2 - *x1) * 0.5f32;
            }

            // Bottom end
            {
                let [x0, x1, x2, x3] = &mut self.sc_x[1];
                let [y0, y1, y2, y3] = &mut self.sc_y[1];
                let [m0, m3] = &mut self.sc_m[1];

                *x0 = bottom;
                *y0 = bottom;
                *y3 = pivot - (pivot - *y0) * 0.25f32;
                *m3 = contrast;
                *x3 = pivot - (pivot - *y3) / *m3;
                let min_width = (*x3 - *x0) * 0.3f32;
                *m0 = 1. / *m3;
                let center = (*y3 - *y0 - *m3 * *x3 + *m0 * *x0) / (*m0 - *m3);
                *x2 = *x3;
                *x1 = 2.0f32 * center - *x2;
                if *x1 < *x0 {
                    *x1 = *x0;
                    *x2 = 2.0f32 * center - *x1;
                } else if (*x2 - *x1) < min_width {
                    *x1 = *x2 - min_width;
                    let new_center = (*x2 + *x1) * 0.5f32;
                    *m0 = (*y3 - *y0 - *m3 * *x3 + new_center * *m3) / (new_center - *x0);
                }
                *y2 = *y3;
                *y1 = *y2 - (*m0 + *m3) * (*x2 - *x1) * 0.5f32;
            }
        } // end if contrast != 1.
    }
}

/// Port of the anonymous `FauxCubicFwdEval` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:
/// 319-336 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn faux_cubic_fwd_eval(
    t: f64,
    x0: f64,
    x2: f64,
    y0: f64,
    y2: f64,
    m0: f64,
    m2: f64,
    x1: f64,
) -> f64 {
    let y1 = (0.5 / ((x2 - x1) + (x1 - x0)))
        * ((2. * y0 + m0 * (x1 - x0)) * (x2 - x1) + (2. * y2 - m2 * (x2 - x1)) * (x1 - x0));

    let t_l = (t - x0) / (x1 - x0);
    let t_r = (t - x1) / (x2 - x1);
    let f_l = y0 * (1. - t_l * t_l) + y1 * t_l * t_l + m0 * (1. - t_l) * t_l * (x1 - x0);
    let f_r =
        y1 * (1. - t_r) * (1. - t_r) + y2 * (2. - t_r) * t_r + m2 * (t_r - 1.) * t_r * (x2 - x1);

    let mut res = if t < x1 { f_l } else { f_r };
    res = if t < x0 { y0 + (t - x0) * m0 } else { res };
    res = if t > x2 { y2 + (t - x2) * m2 } else { res };
    res
}

/// Port of the anonymous `FauxCubicRevEval` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:
/// 338-363 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn faux_cubic_rev_eval(
    t: f64,
    x0: f64,
    x2: f64,
    y0: f64,
    y2: f64,
    m0: f64,
    m2: f64,
    x1: f64,
) -> f64 {
    let y1 = (0.5 / ((x2 - x1) + (x1 - x0)))
        * ((2. * y0 + m0 * (x1 - x0)) * (x2 - x1) + (2. * y2 - m2 * (x2 - x1)) * (x1 - x0));

    let c_l = y0 - t;
    let b_l = m0 * (x1 - x0);
    let a_l = y1 - y0 - m0 * (x1 - x0);
    let discrim_l = (b_l * b_l - 4. * a_l * c_l).sqrt();
    let tmp_l = (2. * c_l) / (-discrim_l - b_l);
    let out_l = tmp_l * (x1 - x0) + x0;

    let c_r = y1 - t;
    let b_r = 2. * y2 - 2. * y1 - m2 * (x2 - x1);
    let a_r = y1 - y2 + m2 * (x2 - x1);
    let discrim_r = (b_r * b_r - 4. * a_r * c_r).sqrt();
    let tmp_r = (2. * c_r) / (-discrim_r - b_r);
    let out_r = tmp_r * (x2 - x1) + x1;

    let mut res = if t < y1 { out_l } else { out_r };
    res = if t < y0 { x0 + (t - y0) / m0 } else { res };
    res = if t > y2 { x2 + (t - y2) / m2 } else { res };
    res
}

/// Port of the anonymous `HighlightFwdEval` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:
/// 365-385 @ v2.5.2).
fn highlight_fwd_eval(t: f64, start: f64, pivot: f64, val: f64) -> f64 {
    let x0 = start;
    let x2 = pivot;
    let y0 = x0;
    let y2 = x2;
    let m0 = 1.;
    let x1 = x0 + (x2 - x0) * 0.5;
    let val = 2. - val;
    if val <= 1. {
        let m2 = if val < 0.01 { 0.01 } else { val };
        faux_cubic_fwd_eval(t, x0, x2, y0, y2, m0, m2, x1)
    } else {
        let m2 = if 2. - val < 0.01 { 0.01 } else { 2. - val };
        faux_cubic_rev_eval(t, x0, x2, y0, y2, m0, m2, x1)
    }
}

/// Port of the anonymous `ShadowFwdEval` (src/OpenColorIO/ops/gradingtone/GradingTone.cpp:
/// 387-406 @ v2.5.2).
fn shadow_fwd_eval(t: f64, start: f64, pivot: f64, val: f64) -> f64 {
    let x0 = start;
    let x2 = pivot;
    let y0 = x0;
    let y2 = x2;
    let m2 = 1.;
    let x1 = x0 + (x2 - x0) * 0.5;
    if val <= 1. {
        let m0 = if val < 0.01 { 0.01 } else { val };
        faux_cubic_fwd_eval(t, x0, x2, y0, y2, m0, m2, x1)
    } else {
        let m0 = if 2. - val < 0.01 { 0.01 } else { 2. - val };
        faux_cubic_rev_eval(t, x0, x2, y0, y2, m0, m2, x1)
    }
}

#[cfg(test)]
#[path = "grading_tone_tests.rs"]
mod tests;
