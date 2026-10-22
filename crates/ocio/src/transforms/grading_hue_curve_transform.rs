// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The hue curves grading transform: a port of
//! `src/OpenColorIO/transforms/GradingHueCurveTransform.h` and `GradingHueCurveTransform.cpp` @
//! v2.5.2 (the `operator<<` of `GradingHueCurve` is with the values, in `ocio-ops`), with its op glue from
//! `src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOp.cpp`: `CreateGradingHueCurveTransform`
//! and `BuildGradingHueCurveOp`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    GradingStyle, HsyTransformStyle, HueCurveType, TransformDirection, grading_style_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op::create_grading_hue_curve_op;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op_data::GradingHueCurveOpData;

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// Eight B-spline curves of hue, saturation and luma (hue-hue, hue-sat, hue-lum, lum-sat,
/// sat-sat, lum-lum, sat-lum and hue-fx), applied in HSY in a log, linear or video space
/// ([`GradingStyle`]), the RGB to HSY conversion around them unless it is
/// [`HsyTransformStyle::None`]. The curves can be made dynamic: the processor built from the
/// transform then has a dynamic property of type `GradingHueCurve`, which an application sets
/// between applies.
///
/// The transform is its op data, as upstream's `GradingHueCurveTransformImpl` holds it. It holds
/// values and a dynamic state: a clone (upstream's `createEditableCopy`) and each processor
/// built from it have their own property, so changing the transform changes no processor (the
/// owner's decision P5-4).
///
/// Port of `GradingHueCurveTransform` and `GradingHueCurveTransformImpl`
/// (include/OpenColorIO/OpenColorTransforms.h:1325-1386,
/// src/OpenColorIO/transforms/GradingHueCurveTransform.h, GradingHueCurveTransform.cpp @
/// v2.5.2).
#[derive(Debug, Clone)]
pub struct GradingHueCurveTransform {
    /// `m_data`.
    data: GradingHueCurveOpData,
}

impl GradingHueCurveTransform {
    /// The style's default (identity) curves, forward, not dynamic, with the RGB to HSY
    /// conversion.
    ///
    /// Port of `GradingHueCurveTransform::Create` and the `GradingHueCurveTransformImpl`
    /// constructor (GradingHueCurveTransform.cpp:13-22 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(style: GradingStyle) -> GradingHueCurveTransform {
        GradingHueCurveTransform {
            data: GradingHueCurveOpData::new(style),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `GradingHueCurveTransformImpl::data() const` (GradingHueCurveTransform.h:57 @
    /// v2.5.2).
    pub(crate) fn data(&self) -> &GradingHueCurveOpData {
        &self.data
    }

    /// Port of `GradingHueCurveTransformImpl::getDirection` (GradingHueCurveTransform.cpp:36-39
    /// @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `GradingHueCurveTransformImpl::setDirection` (GradingHueCurveTransform.cpp:41-44
    /// @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the curves: "GradingHueCurveTransform validation failed: " and
    /// the first problem.
    ///
    /// Port of `GradingHueCurveTransformImpl::validate` (GradingHueCurveTransform.cpp:46-61 @
    /// v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(
                [
                    &b"GradingHueCurveTransform validation failed: "[..],
                    ex.what(),
                ]
                .concat(),
            )
        })
    }

    /// Port of `GradingHueCurveTransformImpl::getFormatMetadata() const`
    /// (GradingHueCurveTransform.cpp:68-71 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `GradingHueCurveTransformImpl::getFormatMetadata()`
    /// (GradingHueCurveTransform.cpp:63-66 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the direction, the style, the HSY conversion and
    /// equal curves, where a dynamic transform equals no other; the metadata is ignored. A transform
    /// equals itself.
    ///
    /// Port of `GradingHueCurveTransformImpl::equals` (GradingHueCurveTransform.cpp:73-77 @
    /// v2.5.2).
    pub fn equals(&self, other: &GradingHueCurveTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// Port of `GradingHueCurveTransformImpl::getStyle` (GradingHueCurveTransform.cpp:79-82 @
    /// v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> GradingStyle {
        self.data.get_style()
    }

    /// Selects the style; a new style resets the curves to its defaults.
    ///
    /// Port of `GradingHueCurveTransformImpl::setStyle` (GradingHueCurveTransform.cpp:84-87 @
    /// v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: GradingStyle) {
        self.data.set_style(style);
    }

    /// A copy of the curves.
    ///
    /// Port of `GradingHueCurveTransformImpl::getValue` (GradingHueCurveTransform.cpp:89-92 @
    /// v2.5.2).
    #[doc(alias = "getValue")]
    pub fn value(&self) -> GradingHueCurve {
        self.data.get_value()
    }

    /// Sets the curves, refused when they aren't valid or can't be fitted.
    ///
    /// Port of `GradingHueCurveTransformImpl::setValue` (GradingHueCurveTransform.cpp:94-97 @
    /// v2.5.2).
    #[doc(alias = "setValue")]
    pub fn set_value(&mut self, values: &GradingHueCurve) -> Result<()> {
        self.data.set_value(values)
    }

    /// The slope of control point `index` of the curve `c`.
    ///
    /// Port of `GradingHueCurveTransformImpl::getSlope` (GradingHueCurveTransform.cpp:99-102 @
    /// v2.5.2).
    #[doc(alias = "getSlope")]
    pub fn slope(&self, c: HueCurveType, index: usize) -> Result<f32> {
        self.data.get_slope(c, index)
    }

    /// Sets the slope of control point `index` of the curve `c`.
    ///
    /// Port of `GradingHueCurveTransformImpl::setSlope` (GradingHueCurveTransform.cpp:104-107
    /// @ v2.5.2).
    #[doc(alias = "setSlope")]
    pub fn set_slope(&mut self, c: HueCurveType, index: usize, slope: f32) -> Result<()> {
        self.data.set_slope(c, index, slope)
    }

    /// Whether the slopes of the curve `c` are its defaults.
    ///
    /// Port of `GradingHueCurveTransformImpl::slopesAreDefault` (GradingHueCurveTransform.cpp:
    /// 109-112 @ v2.5.2).
    #[doc(alias = "slopesAreDefault")]
    pub fn slopes_are_default(&self, c: HueCurveType) -> Result<bool> {
        self.data.slopes_are_default(c)
    }

    /// Whether the op converts RGB to HSY around its curves.
    ///
    /// Port of `GradingHueCurveTransformImpl::getRGBToHSY` (GradingHueCurveTransform.cpp:
    /// 114-117 @ v2.5.2).
    #[doc(alias = "getRGBToHSY")]
    pub fn rgb_to_hsy(&self) -> HsyTransformStyle {
        self.data.get_rgb_to_hsy()
    }

    /// Port of `GradingHueCurveTransformImpl::setRGBToHSY` (GradingHueCurveTransform.cpp:
    /// 119-122 @ v2.5.2).
    #[doc(alias = "setRGBToHSY")]
    pub fn set_rgb_to_hsy(&mut self, style: HsyTransformStyle) {
        self.data.set_rgb_to_hsy(style);
    }

    /// Port of `GradingHueCurveTransformImpl::isDynamic` (GradingHueCurveTransform.cpp:124-127
    /// @ v2.5.2).
    #[doc(alias = "isDynamic")]
    pub fn is_dynamic(&self) -> bool {
        self.data.is_dynamic()
    }

    /// Port of `GradingHueCurveTransformImpl::makeDynamic` (GradingHueCurveTransform.cpp:
    /// 129-132 @ v2.5.2).
    #[doc(alias = "makeDynamic")]
    pub fn make_dynamic(&mut self) {
        self.data.get_dynamic_property_internal().make_dynamic();
    }

    /// Port of `GradingHueCurveTransformImpl::makeNonDynamic` (GradingHueCurveTransform.cpp:
    /// 134-137 @ v2.5.2).
    #[doc(alias = "makeNonDynamic")]
    pub fn make_non_dynamic(&mut self) {
        self.data.get_dynamic_property_internal().make_non_dynamic();
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const GradingHueCurveTransform &)`
    /// (GradingHueCurveTransform.cpp:139-155 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<GradingHueCurveTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", style=");
        os.put_str(grading_style_to_string(self.style()));
        os.put_str(", values=");
        self.value().write_to(os);
        if self.rgb_to_hsy() == HsyTransformStyle::None {
            os.put_str(", hsy_transform=none");
        }
        if self.is_dynamic() {
            os.put_str(", dynamic");
        }
        os.put_str(">");
    }
}

impl PartialEq for GradingHueCurveTransform {
    /// [`GradingHueCurveTransform::equals`].
    fn eq(&self, other: &GradingHueCurveTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for GradingHueCurveTransform {
    /// `<GradingHueCurveTransform direction=<dir>, style=<style>, values=<curves>>`, with
    /// `, hsy_transform=none` and `, dynamic` before the `>` when they apply.
    ///
    /// Port of `operator<<(std::ostream &, const GradingHueCurveTransform &)`
    /// (GradingHueCurveTransform.cpp:139-155 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends to `group` the transform of the GradingHueCurve op `op`: a transform of its style
/// given a copy of its data (`GradingHueCurveOpData::operator=`): the metadata, direction,
/// HSY conversion, curves and dynamic state, in the transform's own property.
///
/// Port of `CreateGradingHueCurveTransform` (src/OpenColorIO/ops/gradinghuecurve/
/// GradingHueCurveOp.cpp:219-232 @ v2.5.2).
pub(crate) fn create_grading_hue_curve_transform(
    group: &mut GroupTransform,
    op: &Op,
) -> Result<()> {
    let OpData::GradingHueCurve(gc_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateGradingHueCurveTransform: op has to be a GradingHueCurveOp.",
        ));
    };
    let mut gc_transform = GradingHueCurveTransform::new(gc_data.get_style());
    gc_transform.data = gc_data.clone();

    group.append_transform(gc_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends a GradingHueCurve op of a copy of it (with its
/// own dynamic property), in the direction `dir` combined with the data's.
///
/// Port of `BuildGradingHueCurveOp` (src/OpenColorIO/ops/gradinghuecurve/
/// GradingHueCurveOp.cpp:234-245 @ v2.5.2). (The config and context it takes are unused.)
pub(crate) fn build_grading_hue_curve_op(
    ops: &mut OpVec,
    transform: &GradingHueCurveTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate()?;

    let curve_data = data.clone();
    create_grading_hue_curve_op(ops, curve_data, dir);
    Ok(())
}

#[cfg(test)]
#[path = "grading_hue_curve_transform_tests.rs"]
mod tests;
