// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The eight curves of a GradingHueCurve op. A port of
//! `src/OpenColorIO/ops/gradinghuecurve/GradingHueCurve.h` and `GradingHueCurve.cpp` @ v2.5.2, of
//! the public `GradingHueCurve` (include/OpenColorIO/OpenColorTransforms.h:587-641), and of its
//! `operator<<` (src/OpenColorIO/transforms/GradingHueCurveTransform.cpp:157-170).
//!
//! Upstream shares the curves through pointers; here they are values, as `p2-rgbcurve` made the
//! RGB curves, and `clone` is `createEditableCopy` (and `GradingHueCurve::Create(const
//! ConstGradingHueCurveRcPtr &)`).

use std::fmt;

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::open_color_types::{GradingStyle, HueCurveType};
use crate::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};

/// The default curve of `c` for `style`: six points a sixth apart for the hue curves (on the
/// diagonal for hue-hue, at 1 for hue-sat and hue-lum, at 0 for hue-fx), three points on [0, 1]
/// (or [-7, 7] for linear) for the others.
///
/// Port of `GradingHueCurveImpl::DefaultCurves` and `DefaultCurvesLin`
/// (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurve.cpp:15-82 @ v2.5.2).
pub fn default_curve(c: HueCurveType, style: GradingStyle) -> GradingBSplineCurve {
    let p = GradingControlPoint::new;
    let sixths = |y: &dyn Fn(f32) -> f32| -> Vec<GradingControlPoint> {
        [0.0f32, 1. / 6., 2. / 6., 0.5, 4. / 6., 5. / 6.]
            .iter()
            .map(|&x| p(x, y(x)))
            .collect()
    };
    let lin = style == GradingStyle::Lin;
    let points: Vec<GradingControlPoint> = match c {
        HueCurveType::HueHue => sixths(&|x| x),
        // HUE_LUM uses the same as HUE_SAT.
        HueCurveType::HueSat | HueCurveType::HueLum => sixths(&|_| 1.0),
        HueCurveType::HueFx => sixths(&|_| 0.0),
        HueCurveType::LumSat if lin => vec![p(-7.0, 1.0), p(0.0, 1.0), p(7.0, 1.0)],
        HueCurveType::LumSat | HueCurveType::SatLum => {
            vec![p(0.0, 1.0), p(0.5, 1.0), p(1.0, 1.0)]
        }
        HueCurveType::LumLum if lin => vec![p(-7.0, -7.0), p(0.0, 0.0), p(7.0, 7.0)],
        HueCurveType::SatSat | HueCurveType::LumLum => {
            vec![p(0.0, 0.0), p(0.5, 0.5), p(1.0, 1.0)]
        }
        HueCurveType::NumCurves => unreachable!("a hue curve type"),
    };
    GradingBSplineCurve::with_points_and_spline_type(&points, c.b_spline_type())
}

/// The curve's name in the messages: `hue_hue`, `hue_sat`, ...
///
/// Port of `CurveTypeName` (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurve.cpp:154-180 @
/// v2.5.2). Its "illegal" for the count can't happen.
fn curve_type_name(c: HueCurveType) -> &'static str {
    match c {
        HueCurveType::HueHue => "hue_hue",
        HueCurveType::HueSat => "hue_sat",
        HueCurveType::HueLum => "hue_lum",
        HueCurveType::LumSat => "lum_sat",
        HueCurveType::SatSat => "sat_sat",
        HueCurveType::LumLum => "lum_lum",
        HueCurveType::SatLum => "sat_lum",
        HueCurveType::HueFx => "hue_fx",
        HueCurveType::NumCurves => "illegal",
    }
}

/// The eight curves of a GradingHueCurve op, and whether the op only draws its hue-sat curve
/// (for a user interface). `==` compares the curves and that flag.
///
/// Port of `GradingHueCurveImpl` (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurve.h:18-61 @
/// v2.5.2), of the public `GradingHueCurve`, and of `operator==` (GradingHueCurve.cpp:319-336).
#[derive(Debug, Clone, PartialEq)]
pub struct GradingHueCurve {
    curves: [GradingBSplineCurve; 8],
    draw_curve_only: bool,
}

impl GradingHueCurve {
    /// The default curves of `style` (the style isn't kept: it only picks the defaults).
    ///
    /// Port of `GradingHueCurve::Create(GradingStyle)` (GradingHueCurve.cpp:281-286 @ v2.5.2)
    /// and `GradingHueCurveImpl(GradingStyle)` (88-97).
    #[doc(alias = "Create")]
    pub fn new(style: GradingStyle) -> Self {
        GradingHueCurve {
            curves: HueCurveType::CURVES.map(|c| default_curve(c, style)),
            draw_curve_only: false,
        }
    }

    /// Copies of the eight curves, validated.
    ///
    /// Port of `GradingHueCurve::Create` of eight curves (GradingHueCurve.cpp:295-317 @ v2.5.2)
    /// and the constructor it calls (99-117).
    #[doc(alias = "Create")]
    #[allow(clippy::too_many_arguments)]
    pub fn with_curves(
        hue_hue: &GradingBSplineCurve,
        hue_sat: &GradingBSplineCurve,
        hue_lum: &GradingBSplineCurve,
        lum_sat: &GradingBSplineCurve,
        sat_sat: &GradingBSplineCurve,
        lum_lum: &GradingBSplineCurve,
        sat_lum: &GradingBSplineCurve,
        hue_fx: &GradingBSplineCurve,
    ) -> Result<Self> {
        let res = GradingHueCurve {
            curves: [
                hue_hue.clone(),
                hue_sat.clone(),
                hue_lum.clone(),
                lum_sat.clone(),
                sat_sat.clone(),
                lum_lum.clone(),
                sat_lum.clone(),
                hue_fx.clone(),
            ],
            draw_curve_only: false,
        };
        res.validate()?;
        Ok(res)
    }

    /// Validates each curve, and, unless the op only draws its curves, that each curve has its
    /// type's spline type.
    ///
    /// Port of `GradingHueCurveImpl::validate` (GradingHueCurve.cpp:182-204 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        for (c, curve) in HueCurveType::CURVES.into_iter().zip(&self.curves) {
            if let Err(e) = curve.validate() {
                return Err(Exception::new(format!(
                    "GradingHueCurve validation failed for '{}' curve with: {}",
                    curve_type_name(c),
                    e.message()
                )));
            }

            // Unless drawCurveOnly is enabled, check that the spline type is correct for the
            // given hue curve type.
            if !self.draw_curve_only && curve.spline_type() != c.b_spline_type() {
                return Err(Exception::new(format!(
                    "GradingHueCurve validation failed: '{}' curve is of the wrong BSplineType.",
                    curve_type_name(c)
                )));
            }
        }
        Ok(())
    }

    /// Whether every curve is an identity of its spline type.
    ///
    /// Port of `GradingHueCurveImpl::isIdentity` (GradingHueCurve.cpp:206-216 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        self.curves.iter().all(GradingBSplineCurve::is_identity)
    }

    /// Port of `GradingHueCurveImpl::getDrawCurveOnly` (GradingHueCurve.cpp:218-221 @ v2.5.2).
    pub fn get_draw_curve_only(&self) -> bool {
        self.draw_curve_only
    }

    /// Port of `GradingHueCurveImpl::setDrawCurveOnly` (GradingHueCurve.cpp:223-226 @ v2.5.2).
    pub fn set_draw_curve_only(&mut self, draw_curve_only: bool) {
        self.draw_curve_only = draw_curve_only;
    }

    /// Curve `c`; "The HueCurveType provided is illegal" for the count.
    ///
    /// Port of `GradingHueCurveImpl::getCurve(HueCurveType) const` (GradingHueCurve.cpp:
    /// 234-242 @ v2.5.2).
    #[doc(alias = "getCurve")]
    pub fn curve(&self, c: HueCurveType) -> Result<&GradingBSplineCurve> {
        match c {
            HueCurveType::NumCurves => Err(Exception::new("The HueCurveType provided is illegal")),
            _ => Ok(&self.curves[c as usize]),
        }
    }

    /// Curve `c`, to change it; "The HueCurveType provided is illegal" for the count.
    ///
    /// Port of `GradingHueCurveImpl::getCurve(HueCurveType)` (GradingHueCurve.cpp:244-252 @
    /// v2.5.2).
    #[doc(alias = "getCurve")]
    pub fn curve_mut(&mut self, c: HueCurveType) -> Result<&mut GradingBSplineCurve> {
        match c {
            HueCurveType::NumCurves => Err(Exception::new("The HueCurveType provided is illegal")),
            _ => Ok(&mut self.curves[c as usize]),
        }
    }

    /// The eight curves, in [`HueCurveType::CURVES`]'s order.
    pub fn curves(&self) -> &[GradingBSplineCurve; 8] {
        &self.curves
    }

    /// `os << curves`: `<hue_hue=..., hue_sat=..., ..., hue_fx=...>`.
    ///
    /// Port of `operator<<(std::ostream &, const GradingHueCurve &)`
    /// (src/OpenColorIO/transforms/GradingHueCurveTransform.cpp:157-170 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        for (i, c) in HueCurveType::CURVES.into_iter().enumerate() {
            os.put_str(if i == 0 { "<" } else { ", " });
            os.put_str(curve_type_name(c));
            os.put_str("=");
            self.curves[i].write_to(os);
        }
        os.put_str(">");
    }
}

/// `operator<<` with a new stream's format (6 significant digits).
impl fmt::Display for GradingHueCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

#[cfg(test)]
#[path = "grading_hue_curve_tests.rs"]
mod tests;
