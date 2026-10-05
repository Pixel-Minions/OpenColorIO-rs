// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The B-spline curves of the grading ops: control points, optional slopes, and the spline's
//! type. A port of `src/OpenColorIO/ops/gradingrgbcurve/GradingBSplineCurve.h` and
//! `GradingBSplineCurve.cpp` @ v2.5.2, and of the public `GradingControlPoint` and
//! `GradingBSplineCurve` (include/OpenColorIO/OpenColorTransforms.h:497-552).
//!
//! Upstream hands curves around as shared pointers (`GradingBSplineCurveRcPtr`) and copies
//! them with `createEditableCopy`; here a curve is a value, and `clone` is that copy.
//!
//! The hue curves' parts (`PrepHueCurveData`, `FitHueSpline`, `EstimateHueSlopes`, the hue
//! knots, `evalCurveRevHue`, `AddShaderEvalRevHue`) come with GradingHueCurve, Phase 5.

use std::fmt;

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::open_color_types::BSplineType;

/// A 2D control point of a [`GradingBSplineCurve`]. `==` compares both coordinates as floats.
///
/// Port of `GradingControlPoint` (include/OpenColorIO/OpenColorTransforms.h:497-504 @ v2.5.2)
/// and its `operator==` (src/OpenColorIO/ops/gradingrgbcurve/GradingBSplineCurve.cpp:
/// 1461-1469).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradingControlPoint {
    /// `m_x`.
    #[doc(alias = "m_x")]
    pub x: f32,
    /// `m_y`.
    #[doc(alias = "m_y")]
    pub y: f32,
}

impl GradingControlPoint {
    /// Port of `GradingControlPoint(float x, float y)` (OpenColorTransforms.h:501).
    pub fn new(x: f32, y: f32) -> Self {
        GradingControlPoint { x, y }
    }

    /// `os << cp`: `<x=0.5, y=0.4>`, each coordinate a `float` in the stream's format.
    ///
    /// Port of `operator<<(std::ostream &, const GradingControlPoint &)`
    /// (src/OpenColorIO/transforms/GradingRGBCurveTransform.cpp:155-159 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        os.put_str("<x=");
        os.put_f32(self.x);
        os.put_str(", y=");
        os.put_f32(self.y);
        os.put_str(">");
    }
}

/// `operator<<` with a new stream's format (6 significant digits), as `str()` of the Python
/// binding prints it.
impl fmt::Display for GradingControlPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(os.str())
    }
}

/// A B-spline curve defined by control points, with optional slopes (all zero: the slopes are
/// estimated from the points) and the kind of spline.
///
/// Port of `GradingBSplineCurveImpl` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingBSplineCurve.h:16-150 @ v2.5.2) and the public `GradingBSplineCurve`
/// (include/OpenColorIO/OpenColorTransforms.h:510-552).
#[derive(Debug, Clone)]
pub struct GradingBSplineCurve {
    control_points: Vec<GradingControlPoint>,
    /// Optional slope values for the control points.
    slopes: Vec<f32>,
    spline_type: BSplineType,
}

impl GradingBSplineCurve {
    /// A `B_SPLINE` curve of `size` control points at (0, 0), with default slopes.
    ///
    /// Port of `GradingBSplineCurve::Create(size_t)` (GradingBSplineCurve.cpp:494-499 @
    /// v2.5.2) and `GradingBSplineCurveImpl(size_t)` (556-559).
    #[doc(alias = "Create")]
    pub fn new(size: usize) -> Self {
        GradingBSplineCurve::with_spline_type(size, BSplineType::BSpline)
    }

    /// A curve of `size` control points at (0, 0), of `spline_type`.
    ///
    /// Port of `GradingBSplineCurve::Create(size_t, BSplineType)` (GradingBSplineCurve.cpp:
    /// 501-506 @ v2.5.2) and `GradingBSplineCurveImpl(size_t, BSplineType)` (561-564).
    #[doc(alias = "Create")]
    pub fn with_spline_type(size: usize, spline_type: BSplineType) -> Self {
        GradingBSplineCurve {
            control_points: vec![GradingControlPoint::default(); size],
            slopes: vec![0.0; size],
            spline_type,
        }
    }

    /// A `B_SPLINE` curve through `points`, with default slopes.
    ///
    /// Port of `GradingBSplineCurve::Create(std::initializer_list<GradingControlPoint>)`
    /// (GradingBSplineCurve.cpp:516-527 @ v2.5.2) and `GradingBSplineCurveImpl(const
    /// std::vector<GradingControlPoint> &)` (566-569).
    #[doc(alias = "Create")]
    pub fn with_points(points: &[GradingControlPoint]) -> Self {
        GradingBSplineCurve::with_points_and_spline_type(points, BSplineType::BSpline)
    }

    /// A curve through `points`, of `spline_type`, with default slopes.
    ///
    /// Port of `GradingBSplineCurve::Create(std::initializer_list<GradingControlPoint>,
    /// BSplineType)` (GradingBSplineCurve.cpp:529-540 @ v2.5.2) and `GradingBSplineCurveImpl(
    /// const std::vector<GradingControlPoint> &, BSplineType)` (571-574).
    #[doc(alias = "Create")]
    pub fn with_points_and_spline_type(
        points: &[GradingControlPoint],
        spline_type: BSplineType,
    ) -> Self {
        GradingBSplineCurve {
            control_points: points.to_vec(),
            slopes: vec![0.0; points.len()],
            spline_type,
        }
    }

    /// Port of `GradingBSplineCurveImpl::getSplineType` (GradingBSplineCurve.cpp:587-590 @
    /// v2.5.2).
    #[doc(alias = "getSplineType")]
    pub fn spline_type(&self) -> BSplineType {
        self.spline_type
    }

    /// Port of `GradingBSplineCurveImpl::setSplineType` (GradingBSplineCurve.cpp:592-595 @
    /// v2.5.2).
    pub fn set_spline_type(&mut self, spline_type: BSplineType) {
        self.spline_type = spline_type;
    }

    /// The number of control points (and of slopes).
    ///
    /// Port of `GradingBSplineCurveImpl::getNumControlPoints` (GradingBSplineCurve.cpp:
    /// 597-600 @ v2.5.2).
    #[doc(alias = "getNumControlPoints")]
    pub fn num_control_points(&self) -> usize {
        self.control_points.len()
    }

    /// Resizes the control points, new ones at (0, 0), and the slopes, new ones 0.
    ///
    /// Port of `GradingBSplineCurveImpl::setNumControlPoints` (GradingBSplineCurve.cpp:
    /// 602-606 @ v2.5.2).
    pub fn set_num_control_points(&mut self, size: usize) {
        self.control_points
            .resize(size, GradingControlPoint::default());
        self.slopes.resize(size, 0.0);
    }

    /// Port of `GradingBSplineCurveImpl::validateIndex` (GradingBSplineCurve.cpp:608-617 @
    /// v2.5.2).
    fn validate_index(&self, index: usize) -> Result<()> {
        let num_points = self.control_points.len();
        if index >= num_points {
            return Err(Exception::new(format!(
                "There are '{num_points}' control points. '{index}' is out of bounds."
            )));
        }
        Ok(())
    }

    /// Control point `index`.
    ///
    /// Port of `GradingBSplineCurveImpl::getControlPoint` (GradingBSplineCurve.cpp:619-623 @
    /// v2.5.2).
    #[doc(alias = "getControlPoint")]
    pub fn control_point(&self, index: usize) -> Result<&GradingControlPoint> {
        self.validate_index(index)?;
        Ok(&self.control_points[index])
    }

    /// Control point `index`, to change it.
    ///
    /// Port of the non-const `GradingBSplineCurveImpl::getControlPoint`
    /// (GradingBSplineCurve.cpp:625-629 @ v2.5.2).
    #[doc(alias = "getControlPoint")]
    pub fn control_point_mut(&mut self, index: usize) -> Result<&mut GradingControlPoint> {
        self.validate_index(index)?;
        Ok(&mut self.control_points[index])
    }

    /// The control points, in order.
    pub fn control_points(&self) -> &[GradingControlPoint] {
        &self.control_points
    }

    /// The slope at control point `index`.
    ///
    /// Port of `GradingBSplineCurveImpl::getSlope` (GradingBSplineCurve.cpp:631-635 @
    /// v2.5.2).
    #[doc(alias = "getSlope")]
    pub fn slope(&self, index: usize) -> Result<f32> {
        self.validate_index(index)?;
        Ok(self.slopes[index])
    }

    /// Port of `GradingBSplineCurveImpl::setSlope` (GradingBSplineCurve.cpp:637-641 @ v2.5.2).
    pub fn set_slope(&mut self, index: usize, slope: f32) -> Result<()> {
        self.validate_index(index)?;
        self.slopes[index] = slope;
        Ok(())
    }

    /// The slopes, one per control point.
    pub fn slopes(&self) -> &[f32] {
        &self.slopes
    }

    /// Whether every slope is 0 (as a float: -0.0 too), so the slopes are estimated.
    ///
    /// Port of `GradingBSplineCurveImpl::slopesAreDefault` (GradingBSplineCurve.cpp:643-653 @
    /// v2.5.2).
    pub fn slopes_are_default(&self) -> bool {
        self.slopes.iter().all(|&s| s == 0.0)
    }

    /// Refuses a curve of fewer than 2 points, x coordinates that decrease, a hue-hue spline
    /// outside x in [0, 1], y coordinates that decrease in a diagonal spline (a hue-hue one
    /// wrapping around), and a periodic spline of 2 points a period apart.
    ///
    /// Port of `GradingBSplineCurveImpl::validate` (GradingBSplineCurve.cpp:657-743 @ v2.5.2).
    /// "The slopes array must be the same length as the control points." can't happen here:
    /// the setters keep the lengths equal, as upstream's do.
    pub fn validate(&self) -> Result<()> {
        let num_points = self.control_points.len();
        if num_points < 2 {
            return Err(Exception::new("There must be at least 2 control points."));
        }
        if num_points != self.slopes.len() {
            return Err(Exception::new(
                "The slopes array must be the same length as the control points.",
            ));
        }

        // Make sure the x-coordinates are non-decreasing.
        let mut last_x = -f32::MAX;
        for (i, p) in self.control_points.iter().enumerate() {
            let x = p.x;
            if x < last_x {
                let mut oss = OStringStream::new(Crt::NATIVE);
                oss.put_str(&format!("Control point at index {i} has a x coordinate '"));
                oss.put_f32(x);
                oss.put_str("' that is less than previous control point x coordinate '");
                oss.put_f32(last_x);
                oss.put_str("'.");
                return Err(Exception::new(oss.into_string()));
            }
            last_x = x;
        }

        // The x-coordinates for a hue-hue spline must be in [0,1].
        if self.spline_type == BSplineType::HueHueBSpline {
            if self.control_points[0].x < 0.0 {
                return Err(Exception::new(
                    "The HUE-HUE spline may not have negative x coordinates.",
                ));
            } else if self.control_points[num_points - 1].x > 1.0 {
                return Err(Exception::new(
                    "The HUE-HUE spline may not have x coordinates greater than one.",
                ));
            }
        }

        // Make sure the y-coordinates are non-decreasing, for diagonal spline types.
        if matches!(
            self.spline_type,
            BSplineType::BSpline | BSplineType::DiagonalBSpline | BSplineType::HueHueBSpline
        ) {
            let mut last_y = -f32::MAX;
            if self.spline_type == BSplineType::HueHueBSpline {
                // The curve is diagonal but continues in a periodic way, so wrap the last point
                // around and ensure the first point would preserve monotonicity.
                last_y = self.control_points[num_points - 1].y - 1.0;
            }
            for (i, p) in self.control_points.iter().enumerate() {
                let y = p.y;
                if y < last_y {
                    let mut oss = OStringStream::new(Crt::NATIVE);
                    oss.put_str(&format!("Control point at index {i} has a y coordinate '"));
                    oss.put_f32(y);
                    oss.put_str("' that is less than previous control point y coordinate '");
                    oss.put_f32(last_y);
                    oss.put_str("'.");
                    return Err(Exception::new(oss.into_string()));
                }
                last_y = y;
            }
        }

        // Don't allow only x values of 0 and 1 for periodic curves (since they are essentially
        // only one point).
        if num_points == 2
            && matches!(
                self.spline_type,
                BSplineType::Periodic1BSpline
                    | BSplineType::Periodic0BSpline
                    | BSplineType::HueHueBSpline
            )
        {
            let del_x = self.control_points[1].x - self.control_points[0].x;
            // `std::abs(float)` compared with the double 1e-3.
            if f64::from((1.0f32 - del_x).abs()) < 1e-3 {
                return Err(Exception::new(
                    "The periodic spline x coordinates may not wrap to the same value.",
                ));
            }
        }
        Ok(())
    }

    /// Whether the curve leaves values unchanged: its points on the identity of its kind of
    /// spline (y = x for the diagonal ones, 0 or 1 for the horizontal ones), and default
    /// slopes.
    ///
    /// Port of `GradingBSplineCurveImpl::isIdentity` (GradingBSplineCurve.cpp:747-777 @
    /// v2.5.2). Its "Unknown curve type" for a value outside the enum can't happen.
    pub fn is_identity(&self) -> bool {
        let points = &self.control_points;
        let is_identity = match self.spline_type {
            BSplineType::DiagonalBSpline | BSplineType::BSpline | BSplineType::HueHueBSpline => {
                points.iter().all(|cp| cp.x == cp.y)
            }
            BSplineType::Periodic0BSpline => points.iter().all(|cp| cp.y == 0.0),
            BSplineType::Horizontal1BSpline | BSplineType::Periodic1BSpline => {
                points.iter().all(|cp| cp.y == 1.0)
            }
        };
        is_identity && self.slopes_are_default()
    }

    /// `os << curve`: `<control_points=[<x=0, y=0><x=1, y=1>]>`, each point with its slope
    /// (`<x=0, y=0, slp=1>`) when the slopes aren't all default.
    ///
    /// Port of `operator<<(std::ostream &, const GradingBSplineCurve &)`
    /// (src/OpenColorIO/transforms/GradingRGBCurveTransform.cpp:161-179 @ v2.5.2).
    pub fn write_to(&self, os: &mut OStringStream) {
        os.put_str("<control_points=[");
        let default_slopes = self.slopes_are_default();
        for (cp, &slope) in self.control_points.iter().zip(&self.slopes) {
            if default_slopes {
                cp.write_to(os);
            } else {
                os.put_str("<x=");
                os.put_f32(cp.x);
                os.put_str(", y=");
                os.put_f32(cp.y);
                os.put_str(", slp=");
                os.put_f32(slope);
                os.put_str(">");
            }
        }
        os.put_str("]>");
    }
}

/// Port of `operator==(const GradingBSplineCurve &, const GradingBSplineCurve &)`
/// (src/OpenColorIO/ops/gradingrgbcurve/GradingBSplineCurve.cpp:1471-1492 @ v2.5.2): the same
/// spline type, and the same number of points with equal points and slopes, as floats.
impl PartialEq for GradingBSplineCurve {
    fn eq(&self, other: &Self) -> bool {
        if self.spline_type != other.spline_type {
            return false;
        }
        self.control_points.len() == other.control_points.len()
            && self
                .control_points
                .iter()
                .zip(&other.control_points)
                .zip(self.slopes.iter().zip(&other.slopes))
                .all(|((a, b), (sa, sb))| a == b && sa == sb)
    }
}

/// `operator<<` with a new stream's format (6 significant digits).
impl fmt::Display for GradingBSplineCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_to(&mut os);
        f.write_str(os.str())
    }
}

#[cfg(test)]
#[path = "grading_b_spline_curve_tests.rs"]
mod tests;
