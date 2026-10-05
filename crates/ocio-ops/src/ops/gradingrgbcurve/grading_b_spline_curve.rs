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
use crate::math_utils::{sse_add, sse_mul, std_max};
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

/// The error of the hue curves' spline fitting, which comes with GradingHueCurve (Phase 5).
pub const HUE_CURVES_NOT_PORTED: &str =
    "GradingBSplineCurve: fitting the hue curves' splines is not ported yet (Phase 5).";

/// The error where upstream's `AdjustRGBSlopes` reads past the control points (a control
/// point's x is NaN): `docs/improvements.md` U-35.
pub const READS_PAST_THE_CONTROL_POINTS: &str =
    "RGB curve: fitting the curve would read past its control points.";

/// The packed knots and coefficients of all the curves of a grading op, which its renderers
/// evaluate: for curve `c`, `knots_offsets[2c]` and `knots_offsets[2c + 1]` are the offset and
/// count of its knots in `knots`, `coefs_offsets[2c]` and `coefs_offsets[2c + 1]` those of
/// its coefficients in `coefs` (the quadratic coefficients of every segment, then the linear
/// ones, then the constant ones). An identity curve has offset -1 and count 0.
///
/// Port of `GradingBSplineCurveImpl::KnotsCoefs` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingBSplineCurve.h:43-129 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct KnotsCoefs {
    /// `m_localBypass`: don't apply the op, all the curves are identities.
    pub local_bypass: bool,
    /// `m_knotsOffsetsArray`: offset and count per curve.
    pub knots_offsets: Vec<i32>,
    /// `m_coefsOffsetsArray`: offset and count per curve.
    pub coefs_offsets: Vec<i32>,
    /// `m_coefsArray`: [`KnotsCoefs::MAX_NUM_COEFS`] entries.
    pub coefs: Vec<f32>,
    /// `m_knotsArray`: [`KnotsCoefs::MAX_NUM_KNOTS`] entries.
    pub knots: Vec<f32>,
    /// `m_numCoefs`: the entries of `coefs` in use.
    pub num_coefs: i32,
    /// `m_numKnots`: the entries of `knots` in use.
    pub num_knots: i32,
}

impl KnotsCoefs {
    /// The most knots of all the curves together (GradingBSplineCurve.h:118).
    pub const MAX_NUM_KNOTS: i32 = 120;
    /// The most coefficients of all the curves together (GradingBSplineCurve.h:120).
    pub const MAX_NUM_COEFS: i32 = 360;

    /// Room for `num_curves` curves. Upstream leaves `m_localBypass` uninitialized; its users
    /// set it before they read it (`precompute`).
    ///
    /// Port of `KnotsCoefs::KnotsCoefs(size_t)` (GradingBSplineCurve.cpp:1247-1254 @ v2.5.2).
    pub fn new(num_curves: usize) -> Self {
        KnotsCoefs {
            local_bypass: false,
            knots_offsets: vec![0; 2 * num_curves],
            coefs_offsets: vec![0; 2 * num_curves],
            coefs: vec![0.0; Self::MAX_NUM_COEFS as usize],
            knots: vec![0.0; Self::MAX_NUM_KNOTS as usize],
            num_coefs: 0,
            num_knots: 0,
        }
    }

    /// Curve `c` at `x`: `identity_x` for an identity curve, the line of the first segment's
    /// slope below the first knot, the line of the last segment's end slope past the last
    /// knot, and the segment's quadratic in between.
    ///
    /// Port of `KnotsCoefs::evalCurve` (GradingBSplineCurve.cpp:1258-1307 @ v2.5.2), in the
    /// operand orders of each wheel's machine code, since a NaN input meets NaN coefficients
    /// (degenerate curves give them; `docs/wheel-inspect.md`):
    /// - Windows (`sub_1801e87d0`, which `GradingRGBCurveFwdOpCPU::apply` calls): `(x -
    ///   knStart) * B + C` below; `offs + slope * (x - knEnd)` above, with `offs = ((t * A) + B)
    ///   * t + C` and `slope = (A + A) * t + B`; `((t * A) + B) * t + C` in between;
    /// - Linux (`0x3bc930`): `B * (x - knStart) + C` below; `offs + (x - knEnd) * slope` above,
    ///   with `offs = ((A * t) + B) * t + C` and the same slope; `((A * t) + B) * t + C` in
    ///   between.
    ///
    /// Both compute upstream's `2.f * A` as `A + A`, which gives the same bits.
    pub fn eval_curve(&self, c: usize, x: f32, identity_x: f32) -> f32 {
        let coefs_sets = self.coefs_offsets[2 * c + 1] / 3;
        if coefs_sets == 0 {
            return identity_x;
        }
        let coefs_offs = self.coefs_offsets[2 * c];
        let knots_cnt = self.knots_offsets[2 * c + 1];
        let knots_offs = self.knots_offsets[2 * c];
        let knot = |i: i32| self.knots[(knots_offs + i) as usize];
        let coef = |i: i32| self.coefs[(coefs_offs + i) as usize];
        // `(A * t + B) * t + C`, with the product `A * t` in each wheel's order.
        let quadratic = |a: f32, b: f32, c: f32, t: f32| {
            let at = if WINDOWS {
                sse_mul(t, a)
            } else {
                sse_mul(a, t)
            };
            sse_add(sse_mul(sse_add(at, b), t), c)
        };

        let kn_start = knot(0);
        let kn_end = knot(knots_cnt - 1);

        if x <= kn_start {
            let b = coef(coefs_sets);
            let c = coef(coefs_sets * 2);
            let t = x - kn_start;
            let bt = if WINDOWS {
                sse_mul(t, b)
            } else {
                sse_mul(b, t)
            };
            sse_add(bt, c)
        } else if x >= kn_end {
            let a = coef(coefs_sets - 1);
            let b = coef(coefs_sets * 2 - 1);
            let c = coef(coefs_sets * 3 - 1);
            let kn = knot(knots_cnt - 2);
            let t = kn_end - kn;
            let slope = sse_add(sse_mul(sse_add(a, a), t), b);
            let offs = quadratic(a, b, c, t);
            let xe = x - kn_end;
            let rise = if WINDOWS {
                sse_mul(slope, xe)
            } else {
                sse_mul(xe, slope)
            };
            sse_add(offs, rise)
        } else {
            let mut i = 0;
            while i < knots_cnt - 2 {
                if x < knot(i + 1) {
                    break;
                }
                i += 1;
            }
            let a = coef(i);
            let b = coef(coefs_sets + i);
            let c = coef(coefs_sets * 2 + i);
            let kn = knot(i);
            quadratic(a, b, c, x - kn)
        }
    }

    /// The inverse of curve `c` (a monotonic one) at `y`: `y` for an identity curve, the
    /// inverses of the end lines outside the curve's range (the end knot where their slope is
    /// nearly 0), and the segment's quadratic solved in between.
    ///
    /// Port of `KnotsCoefs::evalCurveRev` (GradingBSplineCurve.cpp:1311-1381 @ v2.5.2), in the
    /// operand orders and operations of each wheel's machine code:
    /// - Windows (`sub_1801e8970`, which `GradingRGBCurveRevOpCPU::apply` calls): the end
    ///   lines' inverses add the knot second, as the source does; the quadratic's root is `kn -
    ///   C0 / B` and `kn - (C0 + C0) / denom`, for upstream's `kn + (-C0 / B)` and `kn + (-2.f *
    ///   C0) / denom`: the same values, but a NaN `C0` keeps its sign;
    /// - Linux (`0x3bcab0`): the end lines' inverses add the knot first (`knStart + q`); the
    ///   root is `kn + (-C0) / B` and `kn + (C0 * -2) / denom`, as the source.
    ///
    /// `knEndY` and the slope are computed as in [`eval_curve`](Self::eval_curve) on both,
    /// `4.f * A` as `A * 4` (Windows) or `4 * A` (Linux), which give the same bits.
    /// Upstream's unqualified `sqrt` of a `float` is `sqrtss` in both wheels (and `sqrtf` for
    /// a negative argument, for `errno`): the same `float`.
    pub fn eval_curve_rev(&self, c: usize, y: f32) -> f32 {
        let coefs_sets = self.coefs_offsets[2 * c + 1] / 3;
        if coefs_sets == 0 {
            return y;
        }
        let coefs_offs = self.coefs_offsets[2 * c];
        let knots_cnt = self.knots_offsets[2 * c + 1];
        let knots_offs = self.knots_offsets[2 * c];
        let knot = |i: i32| self.knots[(knots_offs + i) as usize];
        let coef = |i: i32| self.coefs[(coefs_offs + i) as usize];
        // `q + kn`, in each wheel's order.
        let add_knot = |q: f32, kn: f32| {
            if WINDOWS {
                sse_add(q, kn)
            } else {
                sse_add(kn, q)
            }
        };

        let kn_start = knot(0);
        let kn_end = knot(knots_cnt - 1);
        let kn_start_y = coef(coefs_sets * 2);
        let kn_end_y = {
            let a = coef(coefs_sets - 1);
            let b = coef(coefs_sets * 2 - 1);
            let c = coef(coefs_sets * 3 - 1);
            let kn = knot(knots_cnt - 2);
            let t = kn_end - kn;
            sse_add(sse_mul(sse_add(sse_mul(a, t), b), t), c)
        };

        if y <= kn_start_y {
            // Extrapolate low side.
            let b = coef(coefs_sets);
            let c = coef(coefs_sets * 2);
            if b.abs() < 1e-5 {
                kn_start
            } else {
                add_knot((y - c) / b, kn_start)
            }
        } else if y >= kn_end_y {
            // Extrapolate high side.
            let a = coef(coefs_sets - 1);
            let b = coef(coefs_sets * 2 - 1);
            let kn = knot(knots_cnt - 2);
            let t = kn_end - kn;
            let slope = sse_add(sse_mul(sse_add(a, a), t), b);
            // `offs` is `knEndY`: the same expression, which both wheels compute once.
            let offs = kn_end_y;
            if slope.abs() < 1e-5 {
                kn_end
            } else {
                add_knot((y - offs) / slope, kn_end)
            }
        } else {
            let mut i = 0;
            while i < knots_cnt - 2 {
                if y < coef(coefs_sets * 2 + i + 1) {
                    break;
                }
                i += 1;
            }
            let a = coef(i);
            let b = coef(coefs_sets + i);
            let c = coef(coefs_sets * 2 + i);
            let kn = knot(i);
            let c0 = c - y;
            let four_a = if WINDOWS {
                sse_mul(a, 4.0)
            } else {
                sse_mul(4.0, a)
            };
            let discrim = (sse_mul(b, b) - sse_mul(four_a, c0)).sqrt();
            let denom = sse_add(discrim, b);
            if denom.abs() < 1e-5 {
                // A~=0, B<0: linear segment with negative slope; use linear inverse.
                return if b.abs() < 1e-5 {
                    kn
                } else if WINDOWS {
                    kn - c0 / b
                } else {
                    sse_add(kn, -c0 / b)
                };
            }
            if WINDOWS {
                kn - sse_add(c0, c0) / denom
            } else {
                sse_add(kn, sse_mul(c0, -2.0) / denom)
            }
        }
    }
}

/// Whether this is the Windows build, whose wheel's machine code orders some operands
/// differently from the Linux wheel's (D12).
const WINDOWS: bool = cfg!(target_os = "windows");

/// The slopes at the control points of an RGB curve, when the user gives none: weighted means
/// of the secants on either side (by their lengths, over runs of equal secants), and at the
/// ends a slope of at least 0.01.
///
/// Port of `EstimateRGBSlopes` (src/OpenColorIO/ops/gradingrgbcurve/GradingBSplineCurve.cpp:
/// 335-384 @ v2.5.2).
fn estimate_rgb_slopes(ctrl_pnts: &[GradingControlPoint], slopes: &mut Vec<f32>) {
    let mut secant_slope = Vec::new();
    let mut secant_len = Vec::new();
    let num_ctrl_pnts = ctrl_pnts.len();
    for i in 0..num_ctrl_pnts - 1 {
        let del_x = ctrl_pnts[i + 1].x - ctrl_pnts[i].x;
        let del_y = ctrl_pnts[i + 1].y - ctrl_pnts[i].y;
        secant_slope.push(del_y / del_x);
        secant_len.push((del_x * del_x + del_y * del_y).sqrt());
    }
    if num_ctrl_pnts == 2 {
        slopes.push(secant_slope[0]);
        slopes.push(secant_slope[0]);
        return;
    }
    let mut i = 0;
    loop {
        let mut j = i;
        let mut dl = secant_len[i];
        while j < num_ctrl_pnts - 2 && (secant_slope[j + 1] - secant_slope[j]).abs() < 1e-6 {
            dl += secant_len[j + 1];
            j += 1;
        }
        for len in &mut secant_len[i..=j] {
            *len = dl;
        }
        if j >= num_ctrl_pnts - 3 {
            break;
        }
        i = j + 1;
    }
    slopes.push(0.0);
    for k in 1..num_ctrl_pnts - 1 {
        let s = (secant_len[k] * secant_slope[k] + secant_len[k - 1] * secant_slope[k - 1])
            / (secant_len[k] + secant_len[k - 1]);
        slopes.push(s);
    }
    slopes.push(std_max(
        0.01,
        0.5 * (3.0 * secant_slope[num_ctrl_pnts - 2] - slopes[num_ctrl_pnts - 2]),
    ));
    slopes[0] = std_max(0.01, 0.5 * (3.0 * secant_slope[0] - slopes[1]));
}

/// The knots and coefficients of the quadratic B-spline through the control points with the
/// given slopes: one segment between two points whose slopes average to the secant, two
/// segments with a knot `ksi` between them otherwise.
///
/// Port of `FitRGBSpline` (GradingBSplineCurve.cpp:388-446 @ v2.5.2).
fn fit_rgb_spline(
    ctrl_pnts: &[GradingControlPoint],
    slopes: &[f32],
    knots: &mut Vec<f32>,
    coefs_a: &mut Vec<f32>,
    coefs_b: &mut Vec<f32>,
    coefs_c: &mut Vec<f32>,
) {
    let num_ctrl_pnts = ctrl_pnts.len();

    knots.push(ctrl_pnts[0].x);
    for i in 0..num_ctrl_pnts - 1 {
        let xi = ctrl_pnts[i].x;
        let xi_pl1 = ctrl_pnts[i + 1].x;
        let yi = ctrl_pnts[i].y;
        let yi_pl1 = ctrl_pnts[i + 1].y;
        let del_x = xi_pl1 - xi;
        let del_y = yi_pl1 - yi;
        let secant_slope = del_y / del_x;
        if ((slopes[i] + slopes[i + 1]) - 2.0 * secant_slope).abs() < 1e-6 {
            coefs_c.push(yi);
            coefs_b.push(slopes[i]);
            coefs_a.push(0.5 * (slopes[i + 1] - slopes[i]) / del_x);
        } else {
            let aa = slopes[i] - secant_slope;
            let bb = slopes[i + 1] - secant_slope;
            let ksi = if aa * bb >= 0.0 {
                (xi + xi_pl1) * 0.5
            } else if aa.abs() > bb.abs() {
                xi_pl1 + aa * del_x / (slopes[i + 1] - slopes[i])
            } else {
                xi + bb * del_x / (slopes[i + 1] - slopes[i])
            };
            let s_bar = (2.0 * secant_slope - slopes[i + 1])
                + (slopes[i + 1] - slopes[i]) * (ksi - xi) / del_x;
            let eta = (s_bar - slopes[i]) / (ksi - xi);
            coefs_c.push(yi);
            coefs_b.push(slopes[i]);
            coefs_a.push(0.5 * eta);
            coefs_c.push(yi + slopes[i] * (ksi - xi) + 0.5 * eta * (ksi - xi) * (ksi - xi));
            coefs_b.push(s_bar);
            coefs_a.push(0.5 * (slopes[i + 1] - s_bar) / (xi_pl1 - ksi));
            knots.push(ksi);
        }
        knots.push(xi_pl1);
    }
}

/// Scales the slopes of the segments whose middle slope would be negative, so that the curve
/// stays monotonic; whether it changed any.
///
/// Port of `AdjustRGBSlopes` (GradingBSplineCurve.cpp:450-488 @ v2.5.2). Upstream pairs each knot
/// that isn't a control point's x with the next control point. A NaN x matches no knot, so
/// upstream walks past the last control point and reads memory it doesn't own; the port
/// refuses the curve there instead ([`READS_PAST_THE_CONTROL_POINTS`], `docs/improvements.md`
/// U-35).
fn adjust_rgb_slopes(
    ctrl_pnts: &[GradingControlPoint],
    slopes: &mut [f32],
    knots: &[f32],
) -> Result<bool> {
    let mut adjustment_done = false;
    let (mut i, mut j) = (0, 0);
    let n = knots.len();
    while j < n {
        if ctrl_pnts[i].x != knots[j] {
            if i + 1 >= ctrl_pnts.len() {
                return Err(Exception::new(READS_PAST_THE_CONTROL_POINTS));
            }
            let ksi = knots[j];
            let xi = ctrl_pnts[i].x;
            let xi_pl1 = ctrl_pnts[i + 1].x;
            let yi = ctrl_pnts[i].y;
            let yi_pl1 = ctrl_pnts[i + 1].y;
            let s_bar =
                (2.0 * (yi_pl1 - yi) - (ksi - xi) * slopes[i] - (xi_pl1 - ksi) * slopes[i + 1])
                    / (xi_pl1 - xi);
            if s_bar < 0.0 {
                adjustment_done = true;
                let secant = (yi_pl1 - yi) / (xi_pl1 - xi);
                let blend_slope =
                    ((ksi - xi) * slopes[i] + (xi_pl1 - ksi) * slopes[i + 1]) / (xi_pl1 - xi);
                let mut aim_slope = 0.01 * 0.5 * (slopes[i] + slopes[i + 1]);
                if aim_slope > secant {
                    aim_slope = secant;
                }
                let adjust = (2.0 * secant - aim_slope) / blend_slope;
                slopes[i] *= adjust;
                slopes[i + 1] *= adjust;
            }
            i += 1;
        }
        j += 1;
    }
    Ok(adjustment_done)
}

impl GradingBSplineCurve {
    /// Fits this curve (curve `curve_idx` of the op) and adds its knots and coefficients to
    /// `knots_coefs`. Only `B_SPLINE` curves, the RGB curves' type, are fitted here; the hue
    /// curves' types come with GradingHueCurve ([`HUE_CURVES_NOT_PORTED`]).
    ///
    /// Port of `GradingBSplineCurveImpl::computeKnotsAndCoefs` (GradingBSplineCurve.cpp:
    /// 1009-1019 @ v2.5.2).
    pub fn compute_knots_and_coefs(
        &self,
        knots_coefs: &mut KnotsCoefs,
        curve_idx: usize,
        draw_curve_only: bool,
    ) -> Result<()> {
        let _ = draw_curve_only;
        if self.spline_type == BSplineType::BSpline {
            self.compute_knots_and_coefs_for_rgb_curve(knots_coefs, curve_idx)
        } else {
            Err(Exception::new(HUE_CURVES_NOT_PORTED))
        }
    }

    /// An identity curve, or one of fewer than 2 points, gets offset -1 and count 0; any other
    /// is fitted with the given slopes, or estimated ones, adjusted once to stay monotonic.
    ///
    /// Port of `GradingBSplineCurveImpl::computeKnotsAndCoefsForRGBCurve`
    /// (GradingBSplineCurve.cpp:793-860 @ v2.5.2).
    fn compute_knots_and_coefs_for_rgb_curve(
        &self,
        knots_coefs: &mut KnotsCoefs,
        curve_idx: usize,
    ) -> Result<()> {
        // Skip invalid data and identity.
        if self.control_points.len() < 2 || self.is_identity() {
            // Identity curve: offset is -1 and count is 0.
            knots_coefs.knots_offsets[curve_idx * 2] = -1;
            knots_coefs.knots_offsets[curve_idx * 2 + 1] = 0;
            knots_coefs.coefs_offsets[curve_idx * 2] = -1;
            knots_coefs.coefs_offsets[curve_idx * 2 + 1] = 0;
            return Ok(());
        }
        let mut knots = Vec::new();
        let mut coefs_a = Vec::new();
        let mut coefs_b = Vec::new();
        let mut coefs_c = Vec::new();
        let mut slopes = Vec::new();

        if !self.slopes_are_default() && self.slopes.len() == self.control_points.len() {
            // If the user-supplied slopes are non-zero, use those.
            slopes.clone_from(&self.slopes);
        } else {
            // Otherwise, estimate slopes based on the control points.
            estimate_rgb_slopes(&self.control_points, &mut slopes);
        }

        let points = &self.control_points;
        fit_rgb_spline(
            points,
            &slopes,
            &mut knots,
            &mut coefs_a,
            &mut coefs_b,
            &mut coefs_c,
        );

        if adjust_rgb_slopes(points, &mut slopes, &knots)? {
            knots.clear();
            coefs_a.clear();
            coefs_b.clear();
            coefs_c.clear();
            fit_rgb_spline(
                points,
                &slopes,
                &mut knots,
                &mut coefs_a,
                &mut coefs_b,
                &mut coefs_c,
            );
        }

        let num_knots = knots_coefs.num_knots;
        let new_knots = knots.len() as i32;
        let num_coefs = knots_coefs.num_coefs;
        let new_coefs = (coefs_a.len() * 3) as i32;

        if num_knots + new_knots > KnotsCoefs::MAX_NUM_KNOTS
            || num_coefs + new_coefs > KnotsCoefs::MAX_NUM_COEFS
        {
            return Err(Exception::new(
                "RGB curve: maximum number of control points reached.",
            ));
        }

        knots_coefs.knots_offsets[curve_idx * 2] = num_knots;
        knots_coefs.knots_offsets[curve_idx * 2 + 1] = new_knots;
        knots_coefs.coefs_offsets[curve_idx * 2] = num_coefs;
        knots_coefs.coefs_offsets[curve_idx * 2 + 1] = new_coefs;

        let coefs_size = coefs_a.len();
        let (nk, nc) = (num_knots as usize, num_coefs as usize);
        knots_coefs.knots[nk..nk + knots.len()].copy_from_slice(&knots);
        knots_coefs.coefs[nc..nc + coefs_size].copy_from_slice(&coefs_a);
        knots_coefs.coefs[nc + coefs_size..nc + 2 * coefs_size].copy_from_slice(&coefs_b);
        knots_coefs.coefs[nc + 2 * coefs_size..nc + 3 * coefs_size].copy_from_slice(&coefs_c);

        knots_coefs.num_knots += new_knots;
        knots_coefs.num_coefs += new_coefs;
        Ok(())
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
