// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma op's data: a family of parametric power functions, per channel (alpha included).
//! The basic styles are a power law; the moncurve styles add a linear segment near black.
//!
//! Port of `GammaOpData` (src/OpenColorIO/ops/gamma/GammaOpData.h and GammaOpData.cpp
//! @ v2.5.2).
//!
//! Upstream's comment (GammaOpData.h:22-50): by convention, the gamma values should be >= 1
//! whenever possible. They are used as is for the forward direction, and the reverse
//! direction gives exponents of less than 1. For the moncurve styles, validation enforces
//! this so that the gamma and the offset work together. The moncurve parameters of a few
//! common functions: sRGB, gamma 2.4 and offset 0.055; Rec.709, 1/0.45 and 0.099; L*, 3.0 and
//! 0.16.
//!
//! # Parameters shorter than the style uses
//!
//! The setters take any number of parameters per channel, and only [`GammaOpData::validate`]
//! checks it (the basic styles take 1, the moncurve styles 2). Before that, upstream's queries
//! read the values they need without a check: past the end of a shorter vector, which is
//! undefined behaviour. The port returns an error there instead, where upstream would read
//! past the end and only there ([`SHORT_PARAMS`]; `docs/improvements.md` U-24):
//! [`is_identity`](GammaOpData::is_identity), [`is_no_op`](GammaOpData::is_no_op),
//! [`get_cache_id`](GammaOpData::get_cache_id) and [`compose`](GammaOpData::compose). The
//! processors validate their ops first, so only code that queries an op directly gets there.

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{NegativeStyle, TransformDirection};
use crate::ops::matrix::MatrixOpData;
use crate::ops::range::RangeOpData;
use crate::platform::strcasecmp;

/// The error of a query that would read past a channel's parameters upstream
/// (`docs/improvements.md` U-24).
pub const SHORT_PARAMS: &str =
    "GammaOp: a channel has fewer parameters than its style uses: upstream reads past them.";

/// The parameters of one channel: `[gamma]` for the basic styles, `[gamma, offset]` for the
/// moncurve styles.
///
/// Port of `GammaOpData::Params` (src/OpenColorIO/ops/gamma/GammaOpData.h:78 @ v2.5.2).
pub type Params = Vec<f64>;

/// The Gamma styles.
///
/// Port of `GammaOpData::Style` (src/OpenColorIO/ops/gamma/GammaOpData.h:57-69 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GammaStyle {
    /// `BASIC_FWD`: `pow(max(x, 0), gamma)`.
    BasicFwd = 0,
    /// `BASIC_REV`: `pow(max(x, 0), 1 / gamma)`.
    BasicRev,
    /// `BASIC_MIRROR_FWD`: the power law, mirrored for negatives.
    BasicMirrorFwd,
    /// `BASIC_MIRROR_REV`.
    BasicMirrorRev,
    /// `BASIC_PASS_THRU_FWD`: the power law, negatives unchanged.
    BasicPassThruFwd,
    /// `BASIC_PASS_THRU_REV`.
    BasicPassThruRev,
    /// `MONCURVE_FWD`: the power law with a linear segment (sRGB-like).
    MoncurveFwd,
    /// `MONCURVE_REV`.
    MoncurveRev,
    /// `MONCURVE_MIRROR_FWD`: the moncurve, mirrored for negatives.
    MoncurveMirrorFwd,
    /// `MONCURVE_MIRROR_REV`.
    MoncurveMirrorRev,
}

impl GammaStyle {
    /// Whether the style is one of the basic ones, which take one parameter per channel (the
    /// `case BASIC_*:` groups of GammaOpData.cpp's switches).
    pub fn is_basic(self) -> bool {
        match self {
            GammaStyle::BasicFwd
            | GammaStyle::BasicRev
            | GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::BasicPassThruRev => true,
            GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => false,
        }
    }
}

/// The cache ID's precision (GammaOpData.cpp:22 @ v2.5.2).
const FLOAT_DECIMALS: i64 = 7;

// Declare the values for an identity operation (GammaOpData.cpp:24-26).
const IDENTITY_SCALE: f64 = 1.0;
const IDENTITY_OFFSET: f64 = 0.0;

/// Port of `IsBasicIdentity` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:28-32 @ v2.5.2). `p`
/// holds at least one value.
fn is_basic_identity(p: &Params) -> bool {
    p[0] == IDENTITY_SCALE
}

/// Port of `IsMonCurveIdentity` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:34-38 @ v2.5.2). `p`
/// holds at least one value, and a second one when the first is 1.
fn is_mon_curve_identity(p: &Params) -> bool {
    p[0] == IDENTITY_SCALE && p[1] == IDENTITY_OFFSET
}

/// The parameters with 7 significant digits, separated by `", "`. `params` holds at least one
/// value (upstream reads `params[0]` regardless).
///
/// Port of `GetParametersString` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:40-50 @ v2.5.2).
fn get_parameters_string(params: &Params) -> String {
    let mut oss = OStringStream::new(Crt::NATIVE);
    oss.precision = FLOAT_DECIMALS;
    oss.put_f64(params[0]);
    for &param in &params[1..] {
        oss.put_str(", ");
        oss.put_f64(param);
    }
    oss.into_string()
}

// The style names (GammaOpData.cpp:52-62). Note that CTF before version 2 was using
// "moncurveFwd". Parsing is case insensitive.
const GAMMA_STYLE_BASIC_FWD: &str = "basicFwd";
const GAMMA_STYLE_BASIC_REV: &str = "basicRev";
const GAMMA_STYLE_BASIC_MIRROR_FWD: &str = "basicMirrorFwd";
const GAMMA_STYLE_BASIC_MIRROR_REV: &str = "basicMirrorRev";
const GAMMA_STYLE_BASIC_PASS_THRU_FWD: &str = "basicPassThruFwd";
const GAMMA_STYLE_BASIC_PASS_THRU_REV: &str = "basicPassThruRev";
const GAMMA_STYLE_MONCURVE_FWD: &str = "monCurveFwd";
const GAMMA_STYLE_MONCURVE_REV: &str = "monCurveRev";
const GAMMA_STYLE_MONCURVE_MIRROR_FWD: &str = "monCurveMirrorFwd";
const GAMMA_STYLE_MONCURVE_MIRROR_REV: &str = "monCurveMirrorRev";

/// Checks one channel's parameters: their number, then each against its bounds.
///
/// Port of `validateParams` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:366-390 @ v2.5.2). The
/// messages print the values with a default `std::stringstream` (6 significant digits).
fn validate_params(
    p: &Params,
    reqd_size: usize,
    low_bounds: &[f64],
    high_bounds: &[f64],
) -> Result<()> {
    if p.len() != reqd_size {
        return Err(Exception::new("GammaOp: Wrong number of parameters"));
    }
    for i in 0..reqd_size {
        if p[i] < low_bounds[i] {
            let mut ss = OStringStream::new(Crt::NATIVE);
            ss.put_str("Parameter ");
            ss.put_f64(p[i]);
            ss.put_str(" is less than lower bound ");
            ss.put_f64(low_bounds[i]);
            return Err(Exception::new(ss.into_string()));
        }
        if p[i] > high_bounds[i] {
            let mut ss = OStringStream::new(Crt::NATIVE);
            ss.put_str("Parameter ");
            ss.put_f64(p[i]);
            ss.put_str(" is greater than upper bound ");
            ss.put_f64(high_bounds[i]);
            return Err(Exception::new(ss.into_string()));
        }
    }
    Ok(())
}

/// The Gamma op's data.
///
/// `==` is upstream's `operator==` ([`equals`](Self::equals)): the style and the parameters,
/// not the metadata.
///
/// Port of `GammaOpData` (src/OpenColorIO/ops/gamma/GammaOpData.h:53-165 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GammaOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: GammaStyle,
    /// `m_redParams`.
    red_params: Params,
    /// `m_greenParams`.
    green_params: Params,
    /// `m_blueParams`.
    blue_params: Params,
    /// `m_alphaParams`.
    alpha_params: Params,
}

impl Default for GammaOpData {
    /// `BASIC_FWD` with identity parameters.
    ///
    /// Port of `GammaOpData::GammaOpData()` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:244-252
    /// @ v2.5.2).
    fn default() -> Self {
        let style = GammaStyle::BasicFwd;
        GammaOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            red_params: Self::identity_parameters(style),
            green_params: Self::identity_parameters(style),
            blue_params: Self::identity_parameters(style),
            alpha_params: Self::identity_parameters(style),
        }
    }
}

impl GammaOpData {
    /// Port of `GammaOpData(style, redParams, greenParams, blueParams, alphaParams)`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:254-266 @ v2.5.2). As upstream's, it doesn't
    /// validate.
    pub fn new(
        style: GammaStyle,
        red_params: Params,
        green_params: Params,
        blue_params: Params,
        alpha_params: Params,
    ) -> Self {
        GammaOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            red_params,
            green_params,
            blue_params,
            alpha_params,
        }
    }

    /// The style a name gives, ignoring ASCII case (`Platform::Strcasecmp`, deviation D-4).
    /// `None` is a null pointer. The name ends at its first NUL, as upstream's C string does.
    ///
    /// Port of `GammaOpData::ConvertStringToStyle`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:66-118 @ v2.5.2).
    pub fn convert_string_to_style(str: Option<&str>) -> Result<GammaStyle> {
        let str = str.map(|s| s.split('\0').next().unwrap_or(""));
        if let Some(str) = str.filter(|s| !s.is_empty()) {
            let styles = [
                (GAMMA_STYLE_BASIC_FWD, GammaStyle::BasicFwd),
                (GAMMA_STYLE_BASIC_REV, GammaStyle::BasicRev),
                (GAMMA_STYLE_BASIC_MIRROR_FWD, GammaStyle::BasicMirrorFwd),
                (GAMMA_STYLE_BASIC_MIRROR_REV, GammaStyle::BasicMirrorRev),
                (
                    GAMMA_STYLE_BASIC_PASS_THRU_FWD,
                    GammaStyle::BasicPassThruFwd,
                ),
                (
                    GAMMA_STYLE_BASIC_PASS_THRU_REV,
                    GammaStyle::BasicPassThruRev,
                ),
                (GAMMA_STYLE_MONCURVE_FWD, GammaStyle::MoncurveFwd),
                (GAMMA_STYLE_MONCURVE_REV, GammaStyle::MoncurveRev),
                (
                    GAMMA_STYLE_MONCURVE_MIRROR_FWD,
                    GammaStyle::MoncurveMirrorFwd,
                ),
                (
                    GAMMA_STYLE_MONCURVE_MIRROR_REV,
                    GammaStyle::MoncurveMirrorRev,
                ),
            ];
            for (name, style) in styles {
                if strcasecmp(str, name).is_eq() {
                    return Ok(style);
                }
            }

            return Err(Exception::new(format!("Unknown gamma style: '{str}'.")));
        }

        Err(Exception::new("Missing gamma style."))
    }

    /// The style's name. (Upstream's error for a value outside the enum can't happen.)
    ///
    /// Port of `GammaOpData::ConvertStyleToString`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:120-150 @ v2.5.2).
    pub fn convert_style_to_string(style: GammaStyle) -> &'static str {
        match style {
            GammaStyle::BasicFwd => GAMMA_STYLE_BASIC_FWD,
            GammaStyle::BasicRev => GAMMA_STYLE_BASIC_REV,
            GammaStyle::BasicMirrorFwd => GAMMA_STYLE_BASIC_MIRROR_FWD,
            GammaStyle::BasicMirrorRev => GAMMA_STYLE_BASIC_MIRROR_REV,
            GammaStyle::BasicPassThruFwd => GAMMA_STYLE_BASIC_PASS_THRU_FWD,
            GammaStyle::BasicPassThruRev => GAMMA_STYLE_BASIC_PASS_THRU_REV,
            GammaStyle::MoncurveFwd => GAMMA_STYLE_MONCURVE_FWD,
            GammaStyle::MoncurveRev => GAMMA_STYLE_MONCURVE_REV,
            GammaStyle::MoncurveMirrorFwd => GAMMA_STYLE_MONCURVE_MIRROR_FWD,
            GammaStyle::MoncurveMirrorRev => GAMMA_STYLE_MONCURVE_MIRROR_REV,
        }
    }

    /// The negative-value handling of a style.
    ///
    /// Port of `GammaOpData::ConvertStyle` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:152-176
    /// @ v2.5.2).
    pub fn convert_style(style: GammaStyle) -> NegativeStyle {
        match style {
            GammaStyle::BasicFwd | GammaStyle::BasicRev => NegativeStyle::Clamp,
            GammaStyle::MoncurveFwd | GammaStyle::MoncurveRev => NegativeStyle::Linear,
            GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => NegativeStyle::Mirror,
            GammaStyle::BasicPassThruFwd | GammaStyle::BasicPassThruRev => NegativeStyle::PassThru,
        }
    }

    /// The basic style for a negative style and direction.
    ///
    /// Port of `GammaOpData::ConvertStyleBasic` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:178-209
    /// @ v2.5.2).
    pub fn convert_style_basic(
        neg_style: NegativeStyle,
        dir: TransformDirection,
    ) -> Result<GammaStyle> {
        let is_forward = dir == TransformDirection::Forward;
        let pick = |fwd, rev| if is_forward { fwd } else { rev };
        match neg_style {
            NegativeStyle::Clamp => Ok(pick(GammaStyle::BasicFwd, GammaStyle::BasicRev)),
            NegativeStyle::Mirror => {
                Ok(pick(GammaStyle::BasicMirrorFwd, GammaStyle::BasicMirrorRev))
            }
            NegativeStyle::PassThru => Ok(pick(
                GammaStyle::BasicPassThruFwd,
                GammaStyle::BasicPassThruRev,
            )),
            NegativeStyle::Linear => Err(Exception::new(
                "Linear negative extrapolation is not valid for basic exponent style.",
            )),
        }
    }

    /// The moncurve style for a negative style and direction.
    ///
    /// Port of `GammaOpData::ConvertStyleMonCurve`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:211-242 @ v2.5.2).
    pub fn convert_style_mon_curve(
        neg_style: NegativeStyle,
        dir: TransformDirection,
    ) -> Result<GammaStyle> {
        let is_forward = dir == TransformDirection::Forward;
        let pick = |fwd, rev| if is_forward { fwd } else { rev };
        match neg_style {
            NegativeStyle::Linear => Ok(pick(GammaStyle::MoncurveFwd, GammaStyle::MoncurveRev)),
            NegativeStyle::Mirror => Ok(pick(
                GammaStyle::MoncurveMirrorFwd,
                GammaStyle::MoncurveMirrorRev,
            )),
            NegativeStyle::PassThru => Err(Exception::new(
                "Pass thru negative extrapolation is not valid for MonCurve exponent style.",
            )),
            NegativeStyle::Clamp => Err(Exception::new(
                "Clamp negative extrapolation is not valid for MonCurve exponent style.",
            )),
        }
    }

    /// Port of `GammaOpData::getType` (src/OpenColorIO/ops/gamma/GammaOpData.h:95 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Gamma
    }

    /// The style.
    ///
    /// Port of `GammaOpData::getStyle` (src/OpenColorIO/ops/gamma/GammaOpData.h:93 @ v2.5.2).
    pub fn style(&self) -> GammaStyle {
        self.style
    }

    /// Sets the style. Upstream notes that `validate` must be called afterwards.
    ///
    /// Port of `GammaOpData::setStyle` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:327-331
    /// @ v2.5.2).
    pub fn set_style(&mut self, style: GammaStyle) {
        self.style = style;
    }

    /// The red channel's parameters.
    pub fn red_params(&self) -> &Params {
        &self.red_params
    }

    /// The red channel's parameters, mutable.
    pub fn red_params_mut(&mut self) -> &mut Params {
        &mut self.red_params
    }

    /// The green channel's parameters.
    pub fn green_params(&self) -> &Params {
        &self.green_params
    }

    /// The green channel's parameters, mutable.
    pub fn green_params_mut(&mut self) -> &mut Params {
        &mut self.green_params
    }

    /// The blue channel's parameters.
    pub fn blue_params(&self) -> &Params {
        &self.blue_params
    }

    /// The blue channel's parameters, mutable.
    pub fn blue_params_mut(&mut self) -> &mut Params {
        &mut self.blue_params
    }

    /// The alpha channel's parameters.
    pub fn alpha_params(&self) -> &Params {
        &self.alpha_params
    }

    /// The alpha channel's parameters, mutable.
    pub fn alpha_params_mut(&mut self) -> &mut Params {
        &mut self.alpha_params
    }

    /// The four channels' parameters, red first.
    pub fn all_params(&self) -> [&Params; 4] {
        [
            &self.red_params,
            &self.green_params,
            &self.blue_params,
            &self.alpha_params,
        ]
    }

    /// Port of `GammaOpData::setRedParams` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:333-337
    /// @ v2.5.2).
    pub fn set_red_params(&mut self, p: Params) {
        self.red_params = p;
    }

    /// Port of `GammaOpData::setGreenParams`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:339-343 @ v2.5.2).
    pub fn set_green_params(&mut self, p: Params) {
        self.green_params = p;
    }

    /// Port of `GammaOpData::setBlueParams` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:345-349
    /// @ v2.5.2).
    pub fn set_blue_params(&mut self, p: Params) {
        self.blue_params = p;
    }

    /// Port of `GammaOpData::setAlphaParams`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:351-355 @ v2.5.2).
    pub fn set_alpha_params(&mut self, p: Params) {
        self.alpha_params = p;
    }

    /// Sets the red, green and blue parameters to `p`, and alpha to the style's identity.
    ///
    /// Port of `GammaOpData::setParams` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:357-364
    /// @ v2.5.2).
    pub fn set_params(&mut self, p: &Params) {
        self.red_params = p.clone();
        self.green_params = p.clone();
        self.blue_params = p.clone();
        self.alpha_params = Self::identity_parameters(self.style);
    }

    /// Checks the parameters: their number for the style, then their bounds, channel by
    /// channel (red, green, blue, alpha): `[0.01, 100]` for the basic styles' gamma, `[1, 10]`
    /// for the moncurve styles' gamma and `[0, 0.9]` for their offset. A NaN passes.
    ///
    /// Port of `GammaOpData::validate` and `GammaOpData::validateParameters`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:392-436 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // Note: When loading from a CTF we want to enforce the canonical bounds on the
        // parameters.
        let (reqd_size, low_bounds, high_bounds): (usize, &[f64], &[f64]) = if self.style.is_basic()
        {
            (1, &[0.01], &[100.])
        } else {
            (2, &[1., 0.], &[10., 0.9])
        };
        for p in self.all_params() {
            validate_params(p, reqd_size, low_bounds, high_bounds)?;
        }
        Ok(())
    }

    /// The identity parameters of a style.
    ///
    /// Port of `GammaOpData::getIdentityParameters`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:438-465 @ v2.5.2).
    pub fn identity_parameters(style: GammaStyle) -> Params {
        if style.is_basic() {
            vec![IDENTITY_SCALE]
        } else {
            vec![IDENTITY_SCALE, IDENTITY_OFFSET]
        }
    }

    /// True when `parameters` are the style's identity.
    ///
    /// Port of `GammaOpData::isIdentityParameters`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:467-490 @ v2.5.2).
    pub fn is_identity_parameters(parameters: &Params, style: GammaStyle) -> bool {
        if style.is_basic() {
            parameters.len() == 1 && is_basic_identity(parameters)
        } else {
            parameters.len() == 2 && is_mon_curve_identity(parameters)
        }
    }

    /// Port of `GammaOpData::isAlphaComponentIdentity`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:492-495 @ v2.5.2).
    pub fn is_alpha_component_identity(&self) -> bool {
        Self::is_identity_parameters(&self.alpha_params, self.style)
    }

    /// Port of `GammaOpData::areAllComponentsEqual`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:497-504 @ v2.5.2).
    pub fn are_all_components_equal(&self) -> bool {
        // Comparing floats is generally not a good idea, but in this case it is ok to be
        // strict. Since the same operations are applied to all components, if they started
        // equal, they should remain equal.
        self.red_params == self.green_params
            && self.red_params == self.blue_params
            && self.red_params == self.alpha_params
    }

    /// Port of `GammaOpData::isNonChannelDependent`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:506-510 @ v2.5.2).
    pub fn is_non_channel_dependent(&self) -> bool {
        self.red_params == self.green_params
            && self.red_params == self.blue_params
            && self.is_alpha_component_identity()
    }

    /// An identity that doesn't clamp. [`SHORT_PARAMS`] where [`is_identity`](Self::is_identity)
    /// gives it.
    ///
    /// Port of `GammaOpData::isNoOp` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:512-515
    /// @ v2.5.2).
    pub fn is_no_op(&self) -> Result<bool> {
        Ok(self.is_identity()? && !self.is_clamping())
    }

    /// Every channel, alpha included, has the same parameters, and red's are the style's
    /// identity values. [`SHORT_PARAMS`] where upstream reads past red's parameters: when the
    /// channels are equal and red has none, or, for a moncurve style, only a gamma of 1.
    ///
    /// Port of `GammaOpData::isIdentity` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:517-549
    /// @ v2.5.2).
    pub fn is_identity(&self) -> Result<bool> {
        if !self.are_all_components_equal() {
            return Ok(false);
        }
        let p = &self.red_params;
        // `IsBasicIdentity` reads p[0]; `IsMonCurveIdentity` reads p[0], then p[1] when p[0]
        // is 1.
        let read = if self.style.is_basic() || p.first() != Some(&IDENTITY_SCALE) {
            1
        } else {
            2
        };
        if p.len() < read {
            return Err(Exception::new(SHORT_PARAMS));
        }
        Ok(if self.style.is_basic() {
            is_basic_identity(p)
        } else {
            is_mon_curve_identity(p)
        })
    }

    /// Port of `GammaOpData::isChannelIndependent`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.h:121 @ v2.5.2).
    pub fn is_channel_independent(&self) -> bool {
        true
    }

    /// Port of `GammaOpData::hasChannelCrosstalk`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.h:138 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// The basic styles clamp negative values.
    ///
    /// Port of `GammaOpData::isClamping` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:551-554
    /// @ v2.5.2).
    pub fn is_clamping(&self) -> bool {
        self.style == GammaStyle::BasicFwd || self.style == GammaStyle::BasicRev
    }

    /// Whether [`compose`](Self::compose) takes `b` after this one: two basic styles whose
    /// negative handling combines (clamp with any; mirror with mirror; pass-thru with
    /// pass-thru). Moncurve styles never compose. Only the styles count.
    ///
    /// Port of `GammaOpData::mayCompose` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:556-619
    /// @ v2.5.2).
    pub fn may_compose(&self, b: &GammaOpData) -> bool {
        // NB: This also does not check bypass or dynamic.
        use GammaStyle::*;
        let style_a = self.style;
        let style_b = b.style;

        match style_a {
            BasicFwd | BasicRev => match style_b {
                BasicFwd | BasicRev | BasicMirrorFwd | BasicMirrorRev | BasicPassThruFwd
                | BasicPassThruRev => true,
                MoncurveFwd | MoncurveRev | MoncurveMirrorFwd | MoncurveMirrorRev => false,
            },
            BasicMirrorFwd | BasicMirrorRev => match style_b {
                BasicFwd | BasicRev | BasicMirrorFwd | BasicMirrorRev => true,
                BasicPassThruFwd | BasicPassThruRev | MoncurveFwd | MoncurveRev
                | MoncurveMirrorFwd | MoncurveMirrorRev => false,
            },
            BasicPassThruFwd | BasicPassThruRev => match style_b {
                BasicFwd | BasicRev | BasicPassThruFwd | BasicPassThruRev => true,
                BasicMirrorFwd | BasicMirrorRev | MoncurveFwd | MoncurveRev | MoncurveMirrorFwd
                | MoncurveMirrorRev => false,
            },
            MoncurveFwd | MoncurveRev | MoncurveMirrorFwd | MoncurveMirrorRev => false,
        }
    }

    /// The data of the op that replaces this one where the optimizer finds it to be an
    /// identity: a Range clamping below 0 for the basic styles that clamp, otherwise the
    /// identity matrix.
    ///
    /// Port of `GammaOpData::getIdentityReplacement`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:621-655 @ v2.5.2).
    pub fn get_identity_replacement(&self) -> OpData {
        match self.style {
            // These clamp values below 0 -- replace with range.
            // TODO: Gamma processes alpha whereas Range does not. So the replacement
            // potentially gives somewhat different results since negative alpha values would
            // not be clamped.
            GammaStyle::BasicFwd | GammaStyle::BasicRev => OpData::Range(
                RangeOpData::with_values(
                    0.,
                    RangeOpData::empty_value(), // Don't clamp high end.
                    0.,
                    RangeOpData::empty_value(),
                )
                .expect("a range clamping at 0 is valid"),
            ),

            // These pass through the full range of values -- replace with matrix.
            GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::BasicPassThruRev
            | GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => OpData::Matrix(MatrixOpData::new()),
        }
    }

    /// The data that applies this one then `b`, for two styles that
    /// [`may_compose`](Self::may_compose) accepts, from the first parameter of each channel:
    /// the exponents multiply (a reverse style's exponent is `1 / gamma`), a product within
    /// 1e-6 of 1 becomes 1, and the style is the forward one that keeps the stricter negative
    /// handling; when red, green and blue come out below 1, every exponent is inverted and
    /// the style becomes reverse. The metadata is this one's combined with `b`'s.
    ///
    /// "GammaOp can only be combined with some GammaOps" where `may_compose` refuses;
    /// [`SHORT_PARAMS`] where a channel has no parameter (upstream reads `[0]` of each).
    ///
    /// Port of `GammaOpData::compose` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:707-783
    /// @ v2.5.2).
    pub fn compose(&self, b: &GammaOpData) -> Result<GammaOpData> {
        if !self.may_compose(b) {
            return Err(Exception::new(
                "GammaOp can only be combined with some GammaOps",
            ));
        }
        if self
            .all_params()
            .iter()
            .chain(&b.all_params())
            .any(|p| p.is_empty())
        {
            return Err(Exception::new(SHORT_PARAMS));
        }

        let style_a = self.style;
        let style_b = b.style;

        let mut r1 = self.red_params[0];
        let mut g1 = self.green_params[0];
        let mut b1 = self.blue_params[0];
        let mut a1 = self.alpha_params[0];

        if is_reverse_basic(style_a) {
            r1 = 1. / r1;
            g1 = 1. / g1;
            b1 = 1. / b1;
            a1 = 1. / a1;
        }

        let mut r2 = b.red_params[0];
        let mut g2 = b.green_params[0];
        let mut b2 = b.blue_params[0];
        let mut a2 = b.alpha_params[0];
        if is_reverse_basic(style_b) {
            r2 = 1. / r2;
            g2 = 1. / g2;
            b2 = 1. / b2;
            a2 = 1. / a2;
        }

        let mut r_out = r1 * r2;
        let mut g_out = g1 * g2;
        let mut b_out = b1 * b2;
        let mut a_out = a1 * a2;

        // Prevent small rounding errors from not making an identity.
        // E.g., 1/0.45 * 0.45 should have a value exactly 1.
        round_around_1(&mut r_out);
        round_around_1(&mut g_out);
        round_around_1(&mut b_out);
        round_around_1(&mut a_out);

        // NB: This always returns a forward style.
        let mut style = combine_basic_styles(style_a, style_b);

        // By convention, we try to keep the gamma parameter > 1.
        if r_out < 1.0 && g_out < 1.0 && b_out < 1.0 {
            r_out = 1. / r_out;
            g_out = 1. / g_out;
            b_out = 1. / b_out;
            a_out = 1. / a_out;
            style = inverse_basic_style(style);
        }

        let mut out_op =
            GammaOpData::new(style, vec![r_out], vec![g_out], vec![b_out], vec![a_out]);

        // TODO: May want to revisit how the metadata is set.
        out_op.metadata = self.metadata.clone();
        out_op.metadata.combine(&b.metadata)?;

        Ok(out_op)
    }

    /// Whether `other` has the same style and parameters. The metadata is ignored.
    ///
    /// Port of `GammaOpData::equals` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:785-796
    /// @ v2.5.2), after `OpData::equals`, which compares the types.
    pub fn equals(&self, other: &GammaOpData) -> bool {
        self.style == other.style
            && self.red_params == other.red_params
            && self.green_params == other.green_params
            && self.blue_params == other.blue_params
            && self.alpha_params == other.alpha_params
    }

    /// The ID (followed by a space) if there is one, the style's name, then each channel's
    /// parameters with 7 significant digits: `basicFwd r:2.2 g:2.2 b:2.2 a:1 `.
    /// [`SHORT_PARAMS`] where a channel has no parameter (upstream prints `params[0]`
    /// regardless).
    ///
    /// Port of `GammaOpData::getCacheID` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:798-816
    /// @ v2.5.2).
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        if self.all_params().iter().any(|p| p.is_empty()) {
            return Err(Exception::new(SHORT_PARAMS));
        }

        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let mut text = String::new();
        text.push_str(Self::convert_style_to_string(self.style));
        text.push(' ');

        for (name, params) in ["r:", "g:", "b:", "a:"].into_iter().zip(self.all_params()) {
            text.push_str(name);
            text.push_str(&get_parameters_string(params));
            text.push(' ');
        }

        cache_id.extend_from_slice(text.as_bytes());
        Ok(cache_id)
    }

    /// The direction the style encodes.
    ///
    /// Port of `GammaOpData::getDirection` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:818-838
    /// @ v2.5.2).
    pub fn direction(&self) -> TransformDirection {
        match self.style {
            GammaStyle::BasicFwd
            | GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveMirrorFwd => TransformDirection::Forward,
            GammaStyle::BasicRev
            | GammaStyle::BasicMirrorRev
            | GammaStyle::BasicPassThruRev
            | GammaStyle::MoncurveRev
            | GammaStyle::MoncurveMirrorRev => TransformDirection::Inverse,
        }
    }

    /// Inverts the style when the direction differs.
    ///
    /// Port of `GammaOpData::setDirection` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:840-846
    /// @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        if self.direction() != dir {
            self.invert();
        }
    }

    /// Swaps the style's direction.
    ///
    /// Port of `GammaOpData::invert` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:848-865
    /// @ v2.5.2).
    fn invert(&mut self) {
        let inv_style = match self.style {
            GammaStyle::BasicFwd => GammaStyle::BasicRev,
            GammaStyle::BasicRev => GammaStyle::BasicFwd,
            GammaStyle::BasicMirrorFwd => GammaStyle::BasicMirrorRev,
            GammaStyle::BasicMirrorRev => GammaStyle::BasicMirrorFwd,
            GammaStyle::BasicPassThruFwd => GammaStyle::BasicPassThruRev,
            GammaStyle::BasicPassThruRev => GammaStyle::BasicPassThruFwd,
            GammaStyle::MoncurveFwd => GammaStyle::MoncurveRev,
            GammaStyle::MoncurveRev => GammaStyle::MoncurveFwd,
            GammaStyle::MoncurveMirrorFwd => GammaStyle::MoncurveMirrorRev,
            GammaStyle::MoncurveMirrorRev => GammaStyle::MoncurveMirrorFwd,
        };
        self.set_style(inv_style);
    }

    /// A copy with the direction swapped. The metadata is copied as it is.
    ///
    /// Port of `GammaOpData::inverse` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:277-287
    /// @ v2.5.2).
    pub fn inverse(&self) -> GammaOpData {
        let mut gamma = self.clone();
        gamma.invert();

        // Note that any existing metadata could become stale at this point but trying to
        // update it is also challenging since inverse() is sometimes called even during the
        // creation of new ops.
        gamma
    }

    /// True for a forward/reverse pair of the same family with equal parameters.
    ///
    /// Port of `GammaOpData::isInverse` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:289-325
    /// @ v2.5.2).
    pub fn is_inverse(&self, b: &GammaOpData) -> bool {
        use GammaStyle::*;

        // Note: It's possible that someone could create something where they don't respect
        // our convention of keeping gamma > 1, in which case, there could be two BASIC_FWD
        // that would be an identity. This code does not to try and handle that case yet,
        // however the pair of ops would get combined and removed as an identity in the
        // optimizer.

        // It's possible that some combinations such as BASIC_PASS_THRU_FWD and BASIC_REV will
        // become an identity after being combined and removed later. We need to do it that
        // way (rather than here) since if isInverse is true, the optimizer calls
        // getIdentityReplacement on only the first of the pair of ops.
        let pair = matches!(
            (self.style, b.style),
            (BasicFwd, BasicRev)
                | (BasicRev, BasicFwd)
                | (MoncurveFwd, MoncurveRev)
                | (MoncurveRev, MoncurveFwd)
                | (MoncurveMirrorFwd, MoncurveMirrorRev)
                | (MoncurveMirrorRev, MoncurveMirrorFwd)
                | (BasicMirrorFwd, BasicMirrorRev)
                | (BasicMirrorRev, BasicMirrorFwd)
                | (BasicPassThruFwd, BasicPassThruRev)
                | (BasicPassThruRev, BasicPassThruFwd)
        );
        pair && self.red_params == b.red_params
            && self.green_params == b.green_params
            && self.blue_params == b.blue_params
            && self.alpha_params == b.alpha_params
    }

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// Port of `OpData::getID` (src/OpenColorIO/Op.cpp:81-84 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        self.metadata.get_attribute_value_string(Some(METADATA_ID))
    }

    /// Port of `OpData::setID` (src/OpenColorIO/Op.cpp:86-89 @ v2.5.2).
    pub fn set_id(&mut self, id: &[u8]) {
        self.metadata.set_id(Some(id));
    }

    /// Port of `OpData::getName` (src/OpenColorIO/Op.cpp:91-94 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        self.metadata
            .get_attribute_value_string(Some(METADATA_NAME))
    }
}

/// Whether the style is a reverse basic one, whose exponent is `1 / gamma`
/// (GammaOpData.cpp:722, 734 @ v2.5.2).
fn is_reverse_basic(style: GammaStyle) -> bool {
    style == GammaStyle::BasicRev
        || style == GammaStyle::BasicMirrorRev
        || style == GammaStyle::BasicPassThruRev
}

/// The forward style of the composition of `a` and `b`. This function assumes that
/// `mayCompose` was called on the inputs and returned true. The logic here is only valid for
/// that situation. There is no intent to preserve the direction, a forward style is always
/// returned.
///
/// Port of `CombineBasicStyles` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:659-682 @ v2.5.2).
fn combine_basic_styles(a: GammaStyle, b: GammaStyle) -> GammaStyle {
    if a == GammaStyle::BasicFwd
        || a == GammaStyle::BasicRev
        || b == GammaStyle::BasicFwd
        || b == GammaStyle::BasicRev
    {
        // If either a or b is a BASIC style, that is the combined style since the BASIC style
        // clamps negatives, so the combination must also clamp.
        GammaStyle::BasicFwd
    } else if a == GammaStyle::BasicMirrorFwd || a == GammaStyle::BasicMirrorRev {
        // Neither a or b is a BASIC style, so it may only be MIRROR or PASS_THRU, but
        // mayCompose will not allow b to be PASS_THRU in this case, both are MIRROR.
        GammaStyle::BasicMirrorFwd
    } else {
        // Both a and b are BASIC_PASS_THRU as a consequence of mayCompose being true.
        GammaStyle::BasicPassThruFwd
    }
}

/// The reverse of a forward basic style.
///
/// Port of `InverseBasicStyle` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:684-695 @ v2.5.2).
fn inverse_basic_style(style: GammaStyle) -> GammaStyle {
    if style == GammaStyle::BasicPassThruFwd {
        GammaStyle::BasicPassThruRev
    } else if style == GammaStyle::BasicMirrorFwd {
        GammaStyle::BasicMirrorRev
    } else {
        GammaStyle::BasicRev
    }
}

/// Sets a value within 1e-6 of 1 to exactly 1.
///
/// Port of `RoundAround1` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:697-704 @ v2.5.2).
fn round_around_1(val: &mut f64) {
    let diff = (*val - 1.).abs();
    if diff < 1e-6 {
        *val = 1.;
    }
}

/// Port of `operator==(const GammaOpData &, const GammaOpData &)`
/// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:867-870 @ v2.5.2).
impl PartialEq for GammaOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "gamma_op_data_tests.rs"]
mod tests;
