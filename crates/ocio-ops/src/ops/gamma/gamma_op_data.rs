// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma op's data: a family of parametric power functions, per channel (alpha included).
//! The basic styles are a power law; the moncurve styles add a linear segment near black.
//!
//! Port of `GammaOpData` (src/OpenColorIO/ops/gamma/GammaOpData.h and GammaOpData.cpp
//! @ v2.5.2): the styles, parameters, accessors, style conversions, identity and direction
//! handling the CPU renderers and the transforms need. Not yet ported (WP 1.3g1): `validate`
//! (its messages print doubles with C++ stream formatting), `getCacheID`, the style strings,
//! `mayCompose`/`compose` and `getIdentityReplacement`, which builds Range and Matrix op data.

use crate::exception::{Exception, Result};
use crate::open_color_types::{NegativeStyle, TransformDirection};

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

// Declare the values for an identity operation (GammaOpData.cpp:24-26).
const IDENTITY_SCALE: f64 = 1.0;
const IDENTITY_OFFSET: f64 = 0.0;

/// Port of `IsBasicIdentity` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:28-32 @ v2.5.2).
fn is_basic_identity(p: &Params) -> bool {
    p[0] == IDENTITY_SCALE
}

/// Port of `IsMonCurveIdentity` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:34-38 @ v2.5.2).
fn is_mon_curve_identity(p: &Params) -> bool {
    p[0] == IDENTITY_SCALE && p[1] == IDENTITY_OFFSET
}

/// The Gamma op's data.
///
/// Port of `GammaOpData` (src/OpenColorIO/ops/gamma/GammaOpData.h:53-165 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct GammaOpData {
    style: GammaStyle,
    red_params: Params,
    green_params: Params,
    blue_params: Params,
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
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:254-266 @ v2.5.2).
    pub fn new(
        style: GammaStyle,
        red_params: Params,
        green_params: Params,
        blue_params: Params,
        alpha_params: Params,
    ) -> Self {
        GammaOpData {
            style,
            red_params,
            green_params,
            blue_params,
            alpha_params,
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

    /// The style.
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

    /// The identity parameters of a style.
    ///
    /// Port of `GammaOpData::getIdentityParameters`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:438-465 @ v2.5.2).
    pub fn identity_parameters(style: GammaStyle) -> Params {
        match style {
            GammaStyle::BasicFwd
            | GammaStyle::BasicRev
            | GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::BasicPassThruRev => vec![IDENTITY_SCALE],
            GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => vec![IDENTITY_SCALE, IDENTITY_OFFSET],
        }
    }

    /// True when `parameters` are the style's identity.
    ///
    /// Port of `GammaOpData::isIdentityParameters`
    /// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:467-490 @ v2.5.2).
    pub fn is_identity_parameters(parameters: &Params, style: GammaStyle) -> bool {
        match style {
            GammaStyle::BasicFwd
            | GammaStyle::BasicRev
            | GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::BasicPassThruRev => {
                parameters.len() == 1 && is_basic_identity(parameters)
            }
            GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => {
                parameters.len() == 2 && is_mon_curve_identity(parameters)
            }
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

    /// An identity that doesn't clamp.
    ///
    /// Port of `GammaOpData::isNoOp` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:512-515
    /// @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity() && !self.is_clamping()
    }

    /// Every channel, alpha included, has the style's identity parameters.
    ///
    /// Port of `GammaOpData::isIdentity` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:517-549
    /// @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        match self.style {
            GammaStyle::BasicFwd
            | GammaStyle::BasicRev
            | GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::BasicPassThruRev => {
                self.are_all_components_equal() && is_basic_identity(&self.red_params)
            }
            GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => {
                self.are_all_components_equal() && is_mon_curve_identity(&self.red_params)
            }
        }
    }

    /// The basic styles clamp negative values.
    ///
    /// Port of `GammaOpData::isClamping` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:551-554
    /// @ v2.5.2).
    pub fn is_clamping(&self) -> bool {
        self.style == GammaStyle::BasicFwd || self.style == GammaStyle::BasicRev
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

    /// A copy with the direction swapped.
    ///
    /// Port of `GammaOpData::inverse` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:277-287
    /// @ v2.5.2).
    pub fn inverse(&self) -> GammaOpData {
        let mut gamma = self.clone();
        gamma.invert();
        gamma
    }

    /// True for a forward/reverse pair of the same family with equal parameters.
    ///
    /// Port of `GammaOpData::isInverse` (src/OpenColorIO/ops/gamma/GammaOpData.cpp:289-325
    /// @ v2.5.2).
    pub fn is_inverse(&self, b: &GammaOpData) -> bool {
        use GammaStyle::*;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use TransformDirection::{Forward, Inverse};

    #[test]
    fn styles_and_directions() {
        for neg in [
            NegativeStyle::Clamp,
            NegativeStyle::Mirror,
            NegativeStyle::PassThru,
        ] {
            let fwd = GammaOpData::convert_style_basic(neg, Forward).unwrap();
            let rev = GammaOpData::convert_style_basic(neg, Inverse).unwrap();
            assert_eq!(GammaOpData::convert_style(fwd), neg);
            let mut g = GammaOpData::new(fwd, vec![2.2], vec![2.2], vec![2.2], vec![2.2]);
            assert_eq!(g.direction(), Forward);
            g.set_direction(Inverse);
            assert_eq!(g.style(), rev);
            assert!(g.is_inverse(&g.inverse()));
        }
        let err = GammaOpData::convert_style_basic(NegativeStyle::Linear, Forward).unwrap_err();
        assert_eq!(
            err.message(),
            "Linear negative extrapolation is not valid for basic exponent style."
        );
        let err = GammaOpData::convert_style_mon_curve(NegativeStyle::Clamp, Inverse).unwrap_err();
        assert_eq!(
            err.message(),
            "Clamp negative extrapolation is not valid for MonCurve exponent style."
        );
    }

    #[test]
    fn identities() {
        let g = GammaOpData::default();
        assert!(g.is_identity() && g.is_clamping() && !g.is_no_op());
        let mut g = GammaOpData::default();
        g.set_style(GammaStyle::MoncurveFwd);
        g.set_params(&GammaOpData::identity_parameters(GammaStyle::MoncurveFwd));
        assert!(g.is_identity() && g.is_no_op() && g.is_non_channel_dependent());
    }
}
