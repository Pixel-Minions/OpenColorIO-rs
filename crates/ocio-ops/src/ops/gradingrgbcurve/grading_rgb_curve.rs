// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The red, green, blue and master curves of a GradingRGBCurve op. A port of
//! `src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurve.h` and `GradingRGBCurve.cpp` @ v2.5.2,
//! and of the public `GradingRGBCurve` (include/OpenColorIO/OpenColorTransforms.h:550-583).
//!
//! Upstream shares the curves through pointers; here they are values, and `clone` is
//! `createEditableCopy` (and `GradingRGBCurve::Create(const ConstGradingRGBCurveRcPtr &)`).

use std::fmt;

use super::grading_b_spline_curve::{GradingBSplineCurve, GradingControlPoint};
use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::open_color_types::{BSplineType, GradingStyle, RgbCurveType};

/// The default curve of the log and video styles: (0, 0), (0.5, 0.5), (1, 1).
///
/// Port of `GradingRGBCurveImpl::Default` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingRGBCurve.cpp:15, 18 @ v2.5.2).
pub fn default_curve() -> GradingBSplineCurve {
    GradingBSplineCurve::with_points(&[
        GradingControlPoint::new(0.0, 0.0),
        GradingControlPoint::new(0.5, 0.5),
        GradingControlPoint::new(1.0, 1.0),
    ])
}

/// The default curve of the linear style: (-7, -7), (0, 0), (7, 7).
///
/// Port of `GradingRGBCurveImpl::DefaultLin` (GradingRGBCurve.cpp:16, 19 @ v2.5.2).
pub fn default_lin_curve() -> GradingBSplineCurve {
    GradingBSplineCurve::with_points(&[
        GradingControlPoint::new(-7.0, -7.0),
        GradingControlPoint::new(0.0, 0.0),
        GradingControlPoint::new(7.0, 7.0),
    ])
}

/// The default curve of `style`: [`default_lin_curve`] for linear, [`default_curve`] else.
///
/// Port of `DefaultValues::Curve` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingRGBCurveOpData.cpp:18-22 @ v2.5.2), which `GradingRGBCurveImpl(GradingStyle)`
/// repeats (GradingRGBCurve.cpp:21-28).
pub fn default_curve_for(style: GradingStyle) -> GradingBSplineCurve {
    if style == GradingStyle::Lin {
        default_lin_curve()
    } else {
        default_curve()
    }
}

/// A set of red, green, blue and master curves.
///
/// Port of `GradingRGBCurveImpl` (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurve.h:15-40 @
/// v2.5.2) and the public `GradingRGBCurve` (include/OpenColorIO/OpenColorTransforms.h:550-583).
#[doc(alias = "GradingRGBCurve")]
#[derive(Debug, Clone, PartialEq)]
pub struct GradingRgbCurve {
    curves: [GradingBSplineCurve; 4],
}

impl GradingRgbCurve {
    /// The default curves of `style` (the style isn't kept: it only picks the defaults).
    ///
    /// Port of `GradingRGBCurve::Create(GradingStyle)` (GradingRGBCurve.cpp:146-151 @ v2.5.2)
    /// and `GradingRGBCurveImpl(GradingStyle)` (21-28).
    #[doc(alias = "Create")]
    pub fn new(style: GradingStyle) -> Self {
        let curve = default_curve_for(style);
        GradingRgbCurve {
            curves: [curve.clone(), curve.clone(), curve.clone(), curve],
        }
    }

    /// Copies of the four curves.
    ///
    /// Port of `GradingRGBCurve::Create(red, green, blue, master)` (GradingRGBCurve.cpp:
    /// 160-168 @ v2.5.2) and `GradingRGBCurveImpl(red, green, blue, master)` (30-44). Its "All
    /// curves have to be defined" for a null pointer can't happen.
    #[doc(alias = "Create")]
    pub fn with_curves(
        red: &GradingBSplineCurve,
        green: &GradingBSplineCurve,
        blue: &GradingBSplineCurve,
        master: &GradingBSplineCurve,
    ) -> Self {
        GradingRgbCurve {
            curves: [red.clone(), green.clone(), blue.clone(), master.clone()],
        }
    }

    /// Refuses a curve that doesn't validate, or that isn't a `B_SPLINE`, naming the curve.
    ///
    /// Port of `GradingRGBCurveImpl::validate` (GradingRGBCurve.cpp:90-114 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        for (c, curve) in RgbCurveType::CURVES.into_iter().zip(&self.curves) {
            if let Err(e) = curve.validate() {
                return Err(Exception::new(format!(
                    "GradingRGBCurve validation failed for '{}' curve with: {}",
                    curve_type_name(c),
                    e.message()
                )));
            }
            if curve.spline_type() != BSplineType::BSpline {
                return Err(Exception::new(format!(
                    "GradingRGBCurve validation failed: '{}' curve is of the wrong BSplineType.",
                    curve_type_name(c)
                )));
            }
        }
        Ok(())
    }

    /// Whether every curve is an identity.
    ///
    /// Port of `GradingRGBCurveImpl::isIdentity` (GradingRGBCurve.cpp:116-126 @ v2.5.2),
    /// through `IsGradingCurveIdentity` (GradingBSplineCurve.cpp:781-789).
    pub fn is_identity(&self) -> bool {
        self.curves.iter().all(GradingBSplineCurve::is_identity)
    }

    /// The curve `c`; `RgbCurveType::NumCurves` is "Invalid curve.".
    ///
    /// Port of `GradingRGBCurveImpl::getCurve` (GradingRGBCurve.cpp:128-135 @ v2.5.2).
    #[doc(alias = "getCurve")]
    pub fn curve(&self, c: RgbCurveType) -> Result<&GradingBSplineCurve> {
        match c {
            RgbCurveType::NumCurves => Err(Exception::new("Invalid curve.")),
            _ => Ok(&self.curves[c as usize]),
        }
    }

    /// The curve `c`, to change it; `RgbCurveType::NumCurves` is "Invalid curve.".
    ///
    /// Port of the non-const `GradingRGBCurveImpl::getCurve` (GradingRGBCurve.cpp:137-144 @
    /// v2.5.2).
    #[doc(alias = "getCurve")]
    pub fn curve_mut(&mut self, c: RgbCurveType) -> Result<&mut GradingBSplineCurve> {
        match c {
            RgbCurveType::NumCurves => Err(Exception::new("Invalid curve.")),
            _ => Ok(&mut self.curves[c as usize]),
        }
    }

    /// The four curves, red, green, blue and master.
    pub fn curves(&self) -> &[GradingBSplineCurve; 4] {
        &self.curves
    }

    /// `os << curves`: `<red=..., green=..., blue=..., master=...>`.
    ///
    /// Port of `operator<<(std::ostream &, const GradingRGBCurve &)`
    /// (src/OpenColorIO/transforms/GradingRGBCurveTransform.cpp:181-190 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        for (i, name) in ["<red=", ", green=", ", blue=", ", master="]
            .into_iter()
            .enumerate()
        {
            os.put_str(name);
            self.curves[i].write_to(os);
        }
        os.put_str(">");
    }
}

/// A curve's name in the messages: `red`, `green`, `blue`, `master`; `invalid` for the count.
///
/// Port of `CurveType` (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurve.cpp:69-88 @
/// v2.5.2).
fn curve_type_name(c: RgbCurveType) -> &'static str {
    match c {
        RgbCurveType::Red => "red",
        RgbCurveType::Green => "green",
        RgbCurveType::Blue => "blue",
        RgbCurveType::Master => "master",
        RgbCurveType::NumCurves => "invalid",
    }
}

/// `operator<<` with a new stream's format (6 significant digits).
impl fmt::Display for GradingRgbCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(os.str())
    }
}

#[cfg(test)]
#[path = "grading_rgb_curve_tests.rs"]
mod tests;
