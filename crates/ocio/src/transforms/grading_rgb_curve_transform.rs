// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The RGB curves grading transform: a port of
//! `src/OpenColorIO/transforms/GradingRGBCurveTransform.h` and `GradingRGBCurveTransform.cpp` @
//! v2.5.2 (the `operator<<` of `GradingControlPoint`, `GradingBSplineCurve` and
//! `GradingRGBCurve` is with the values, in `ocio-ops`), with its op glue from
//! `src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOp.cpp`: `CreateGradingRGBCurveTransform`
//! and `BuildGradingRGBCurveOp`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    GradingStyle, RgbCurveType, TransformDirection, grading_style_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op::create_grading_rgb_curve_op;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// B-spline curves applied to the red, green, blue and master channels, in a log, linear or
/// video space ([`GradingStyle`]; the linear style works in a log space unless its lin-to-log
/// conversion is bypassed). The curves can be made dynamic: the processor built from the
/// transform then has a dynamic property of type `GradingRGBCurve`, which an application sets
/// between applies.
///
/// The transform is its op data, as upstream's `GradingRGBCurveTransformImpl` holds it. It holds
/// values and a dynamic state: a clone (upstream's `createEditableCopy`) and each processor
/// built from it have their own property, so changing the transform changes no processor (the
/// owner's decision P5-4).
///
/// Port of `GradingRGBCurveTransform` and `GradingRGBCurveTransformImpl`
/// (include/OpenColorIO/OpenColorTransforms.h:1403-1461,
/// src/OpenColorIO/transforms/GradingRGBCurveTransform.h, GradingRGBCurveTransform.cpp @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct GradingRgbCurveTransform {
    /// `m_data`.
    data: GradingRgbCurveOpData,
}

impl GradingRgbCurveTransform {
    /// The style's default (identity) curves, forward, not dynamic, the lin-to-log conversion
    /// not bypassed.
    ///
    /// Port of `GradingRGBCurveTransform::Create` and the `GradingRGBCurveTransformImpl`
    /// constructor (GradingRGBCurveTransform.cpp:12-21 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(style: GradingStyle) -> GradingRgbCurveTransform {
        GradingRgbCurveTransform {
            data: GradingRgbCurveOpData::new(style),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `GradingRGBCurveTransformImpl::data() const` (GradingRGBCurveTransform.h:56 @
    /// v2.5.2).
    pub(crate) fn data(&self) -> &GradingRgbCurveOpData {
        &self.data
    }

    /// Port of `GradingRGBCurveTransformImpl::getDirection` (GradingRGBCurveTransform.cpp:35-38
    /// @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `GradingRGBCurveTransformImpl::setDirection` (GradingRGBCurveTransform.cpp:40-43
    /// @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the curves: "GradingRGBCurveTransform validation failed: " and
    /// the first problem.
    ///
    /// Port of `GradingRGBCurveTransformImpl::validate` (GradingRGBCurveTransform.cpp:45-58 @
    /// v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(
                [
                    &b"GradingRGBCurveTransform validation failed: "[..],
                    ex.what(),
                ]
                .concat(),
            )
        })
    }

    /// Port of `GradingRGBCurveTransformImpl::getFormatMetadata() const`
    /// (GradingRGBCurveTransform.cpp:65-68 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `GradingRGBCurveTransformImpl::getFormatMetadata()`
    /// (GradingRGBCurveTransform.cpp:60-63 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the direction, the style, the bypass and equal
    /// curves, where a dynamic transform equals no other; the metadata is ignored. A transform
    /// equals itself.
    ///
    /// Port of `GradingRGBCurveTransformImpl::equals` (GradingRGBCurveTransform.cpp:71-75 @
    /// v2.5.2).
    pub fn equals(&self, other: &GradingRgbCurveTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// Port of `GradingRGBCurveTransformImpl::getStyle` (GradingRGBCurveTransform.cpp:77-80 @
    /// v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> GradingStyle {
        self.data.get_style()
    }

    /// Selects the style; a new style resets the curves to its defaults.
    ///
    /// Port of `GradingRGBCurveTransformImpl::setStyle` (GradingRGBCurveTransform.cpp:82-85 @
    /// v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: GradingStyle) {
        self.data.set_style(style);
    }

    /// A copy of the curves.
    ///
    /// Port of `GradingRGBCurveTransformImpl::getValue` (GradingRGBCurveTransform.cpp:87-90 @
    /// v2.5.2).
    #[doc(alias = "getValue")]
    pub fn value(&self) -> GradingRgbCurve {
        self.data.get_value()
    }

    /// Sets the curves, refused when they aren't valid or can't be fitted.
    ///
    /// Port of `GradingRGBCurveTransformImpl::setValue` (GradingRGBCurveTransform.cpp:92-95 @
    /// v2.5.2).
    #[doc(alias = "setValue")]
    pub fn set_value(&mut self, values: &GradingRgbCurve) -> Result<()> {
        self.data.set_value(values)
    }

    /// Holds the curves as they are, not validated (the property's
    /// `set_value_as_is`). For the Python binding only (`ocio-py`), whose `getValue` returns
    /// the transform's own curves, as upstream's `GradingRGBCurveTransformImpl::getValue` does, which
    /// Python code changes in place; behaviour-neutral for the Rust API, whose `value` is a
    /// copy (owner decision 2026-10-23, as `replace_*_as_is` of the config's objects).
    #[doc(hidden)]
    pub fn set_value_as_is(&mut self, values: &GradingRgbCurve) {
        self.data
            .get_dynamic_property_internal()
            .set_value_as_is(values);
    }

    /// The slope of control point `index` of the curve `c`.
    ///
    /// Port of `GradingRGBCurveTransformImpl::getSlope` (GradingRGBCurveTransform.cpp:97-100 @
    /// v2.5.2).
    #[doc(alias = "getSlope")]
    pub fn slope(&self, c: RgbCurveType, index: usize) -> Result<f32> {
        self.data.get_slope(c, index)
    }

    /// Sets the slope of control point `index` of the curve `c`.
    ///
    /// Port of `GradingRGBCurveTransformImpl::setSlope` (GradingRGBCurveTransform.cpp:102-105
    /// @ v2.5.2).
    #[doc(alias = "setSlope")]
    pub fn set_slope(&mut self, c: RgbCurveType, index: usize, slope: f32) -> Result<()> {
        self.data.set_slope(c, index, slope)
    }

    /// Whether the slopes of the curve `c` are its defaults.
    ///
    /// Port of `GradingRGBCurveTransformImpl::slopesAreDefault` (GradingRGBCurveTransform.cpp:
    /// 107-110 @ v2.5.2).
    #[doc(alias = "slopesAreDefault")]
    pub fn slopes_are_default(&self, c: RgbCurveType) -> Result<bool> {
        self.data.slopes_are_default(c)
    }

    /// Port of `GradingRGBCurveTransformImpl::getBypassLinToLog` (GradingRGBCurveTransform.cpp:
    /// 112-115 @ v2.5.2).
    #[doc(alias = "getBypassLinToLog")]
    pub fn bypass_lin_to_log(&self) -> bool {
        self.data.get_bypass_lin_to_log()
    }

    /// Port of `GradingRGBCurveTransformImpl::setBypassLinToLog` (GradingRGBCurveTransform.cpp:
    /// 117-120 @ v2.5.2).
    #[doc(alias = "setBypassLinToLog")]
    pub fn set_bypass_lin_to_log(&mut self, bypass: bool) {
        self.data.set_bypass_lin_to_log(bypass);
    }

    /// Port of `GradingRGBCurveTransformImpl::isDynamic` (GradingRGBCurveTransform.cpp:122-125
    /// @ v2.5.2).
    #[doc(alias = "isDynamic")]
    pub fn is_dynamic(&self) -> bool {
        self.data.is_dynamic()
    }

    /// Port of `GradingRGBCurveTransformImpl::makeDynamic` (GradingRGBCurveTransform.cpp:
    /// 127-130 @ v2.5.2).
    #[doc(alias = "makeDynamic")]
    pub fn make_dynamic(&mut self) {
        self.data.get_dynamic_property_internal().make_dynamic();
    }

    /// Port of `GradingRGBCurveTransformImpl::makeNonDynamic` (GradingRGBCurveTransform.cpp:
    /// 132-135 @ v2.5.2).
    #[doc(alias = "makeNonDynamic")]
    pub fn make_non_dynamic(&mut self) {
        self.data.get_dynamic_property_internal().make_non_dynamic();
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const GradingRGBCurveTransform &)`
    /// (GradingRGBCurveTransform.cpp:137-153 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<GradingRGBCurveTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", style=");
        os.put_str(grading_style_to_string(self.style()));
        os.put_str(", values=");
        self.value().write_to(os);
        if self.bypass_lin_to_log() {
            os.put_str(", bypass_lintolog");
        }
        if self.is_dynamic() {
            os.put_str(", dynamic");
        }
        os.put_str(">");
    }
}

impl PartialEq for GradingRgbCurveTransform {
    /// [`GradingRgbCurveTransform::equals`].
    fn eq(&self, other: &GradingRgbCurveTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for GradingRgbCurveTransform {
    /// `<GradingRGBCurveTransform direction=<dir>, style=<style>, values=<curves>>`, with
    /// `, bypass_lintolog` and `, dynamic` before the `>` when they apply.
    ///
    /// Port of `operator<<(std::ostream &, const GradingRGBCurveTransform &)`
    /// (GradingRGBCurveTransform.cpp:137-153 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends to `group` the transform of the GradingRGBCurve op `op`: a transform of its style
/// given a copy of its data (`GradingRGBCurveOpData::operator=`): the metadata, direction,
/// bypass, curves and dynamic state, in the transform's own property.
///
/// Port of `CreateGradingRGBCurveTransform` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingRGBCurveOp.cpp:223-236 @ v2.5.2).
pub(crate) fn create_grading_rgb_curve_transform(
    group: &mut GroupTransform,
    op: &Op,
) -> Result<()> {
    let OpData::GradingRgbCurve(gc_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateGradingRGBCurveTransform: op has to be a GradingRGBCurveOp.",
        ));
    };
    let mut gc_transform = GradingRgbCurveTransform::new(gc_data.get_style());
    gc_transform.data = gc_data.clone();

    group.append_transform(gc_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends a GradingRGBCurve op of a copy of it (with its
/// own dynamic property), in the direction `dir` combined with the data's.
///
/// Port of `BuildGradingRGBCurveOp` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingRGBCurveOp.cpp:238-249 @ v2.5.2). (The config and context it takes are unused.)
pub(crate) fn build_grading_rgb_curve_op(
    ops: &mut OpVec,
    transform: &GradingRgbCurveTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate()?;

    // Upstream's `data.clone()` (GradingRGBCurveOp.cpp:247 @ v2.5.2) is the copy constructor
    // (GradingRGBCurveOpData.cpp:44-52), whose `*this = rhs` sets the copy's value with the
    // property's `setValue` (operator=, 54-72, at 65): it
    // validates and fits the curves again (the port's clone copies the knots), so curves held
    // as they are (`set_value_as_is`) are fitted here, with upstream's errors.
    let curve_data = data.clone();
    curve_data.set_value(&curve_data.get_value())?;
    create_grading_rgb_curve_op(ops, curve_data, dir);
    Ok(())
}

#[cfg(test)]
#[path = "grading_rgb_curve_transform_tests.rs"]
mod tests;
