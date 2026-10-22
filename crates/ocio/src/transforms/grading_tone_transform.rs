// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tone grading transform: a port of
//! `src/OpenColorIO/transforms/GradingToneTransform.h` and `GradingToneTransform.cpp` @
//! v2.5.2 (the `operator<<` of `GradingRGBMSW` and `GradingTone` is with the values, in
//! `ocio-ops`), with its op glue from `src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp`:
//! `CreateGradingToneTransform` and `BuildGradingToneOp`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    GradingStyle, TransformDirection, grading_style_to_string, transform_direction_to_string,
};
use ocio_ops::ops::gradingtone::grading_tone::GradingTone;
use ocio_ops::ops::gradingtone::grading_tone_op::create_grading_tone_op;
use ocio_ops::ops::gradingtone::grading_tone_op_data::GradingToneOpData;

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// Tonal range controls (blacks, shadows, midtones, highlights, whites and s-contrast), whose
/// meaning depends on the style: log, linear or video ([`GradingStyle`]). The values can be
/// made dynamic: the processor built from the transform then has a dynamic property of type
/// `GradingTone`, which an application sets between applies.
///
/// The transform is its op data, as upstream's `GradingToneTransformImpl` holds it. It holds
/// values and a dynamic state: a clone (upstream's `createEditableCopy`) and each processor
/// built from it have their own property, so changing the transform changes no processor (the
/// owner's decision P5-4).
///
/// Port of `GradingToneTransform` and `GradingToneTransformImpl`
/// (include/OpenColorIO/OpenColorTransforms.h:1489-1525,
/// src/OpenColorIO/transforms/GradingToneTransform.h, GradingToneTransform.cpp @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GradingToneTransform {
    /// `m_data`.
    data: GradingToneOpData,
}

impl GradingToneTransform {
    /// The style's default values, forward, not dynamic.
    ///
    /// Port of `GradingToneTransform::Create` and the `GradingToneTransformImpl`
    /// constructor (GradingToneTransform.cpp:13-22 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(style: GradingStyle) -> GradingToneTransform {
        GradingToneTransform {
            data: GradingToneOpData::new(style),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `GradingToneTransformImpl::data() const` (GradingToneTransform.h:49 @
    /// v2.5.2).
    pub(crate) fn data(&self) -> &GradingToneOpData {
        &self.data
    }

    /// Port of `GradingToneTransformImpl::getDirection` (GradingToneTransform.cpp:36-39 @
    /// v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `GradingToneTransformImpl::setDirection` (GradingToneTransform.cpp:41-44 @
    /// v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the values for the style: "GradingToneTransform validation
    /// failed: " and the first problem.
    ///
    /// Port of `GradingToneTransformImpl::validate` (GradingToneTransform.cpp:46-59 @
    /// v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new([&b"GradingToneTransform validation failed: "[..], ex.what()].concat())
        })
    }

    /// Port of `GradingToneTransformImpl::getFormatMetadata() const`
    /// (GradingToneTransform.cpp:66-69 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `GradingToneTransformImpl::getFormatMetadata()` (GradingToneTransform.cpp:
    /// 61-64 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the style, the direction and equal values, where a
    /// dynamic transform equals no other (and a NaN value none); the metadata is ignored. A
    /// transform equals itself.
    ///
    /// Port of `GradingToneTransformImpl::equals` (GradingToneTransform.cpp:72-76 @
    /// v2.5.2).
    pub fn equals(&self, other: &GradingToneTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// Port of `GradingToneTransformImpl::getStyle` (GradingToneTransform.cpp:78-81 @
    /// v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> GradingStyle {
        self.data.get_style()
    }

    /// Selects the style; a new style resets the values to its defaults.
    ///
    /// Port of `GradingToneTransformImpl::setStyle` (GradingToneTransform.cpp:83-86 @
    /// v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: GradingStyle) {
        self.data.set_style(style);
    }

    /// A copy of the values.
    ///
    /// Port of `GradingToneTransformImpl::getValue` (GradingToneTransform.cpp:88-91 @
    /// v2.5.2).
    #[doc(alias = "getValue")]
    pub fn value(&self) -> GradingTone {
        self.data.get_value()
    }

    /// Sets the values, refused (with [`GradingTone::validate`]'s message) when they aren't
    /// valid for the style.
    ///
    /// Port of `GradingToneTransformImpl::setValue` (GradingToneTransform.cpp:93-96 @
    /// v2.5.2).
    #[doc(alias = "setValue")]
    pub fn set_value(&mut self, values: &GradingTone) -> Result<()> {
        self.data.set_value(values)
    }

    /// Port of `GradingToneTransformImpl::isDynamic` (GradingToneTransform.cpp:98-101 @
    /// v2.5.2).
    #[doc(alias = "isDynamic")]
    pub fn is_dynamic(&self) -> bool {
        self.data.is_dynamic()
    }

    /// Port of `GradingToneTransformImpl::makeDynamic` (GradingToneTransform.cpp:103-106
    /// @ v2.5.2).
    #[doc(alias = "makeDynamic")]
    pub fn make_dynamic(&mut self) {
        self.data.get_dynamic_property_internal().make_dynamic();
    }

    /// Port of `GradingToneTransformImpl::makeNonDynamic` (GradingToneTransform.cpp:
    /// 108-111 @ v2.5.2).
    #[doc(alias = "makeNonDynamic")]
    pub fn make_non_dynamic(&mut self) {
        self.data.get_dynamic_property_internal().make_non_dynamic();
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const GradingToneTransform &)`
    /// (GradingToneTransform.cpp:113-125 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<GradingToneTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", style=");
        os.put_str(grading_style_to_string(self.style()));
        os.put_str(", values=");
        self.value().write_to(os);
        if self.is_dynamic() {
            os.put_str(", dynamic");
        }
        os.put_str(">");
    }
}

impl PartialEq for GradingToneTransform {
    /// [`GradingToneTransform::equals`].
    fn eq(&self, other: &GradingToneTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for GradingToneTransform {
    /// `<GradingToneTransform direction=<dir>, style=<style>, values=<values>>`, with
    /// `, dynamic` before the `>` for a dynamic transform.
    ///
    /// Port of `operator<<(std::ostream &, const GradingToneTransform &)`
    /// (GradingToneTransform.cpp:113-125 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends to `group` the transform of the GradingTone op `op`: a transform of its style
/// given a copy of its data (`GradingToneOpData::operator=`): the metadata, direction,
/// values and dynamic state, in the transform's own property.
///
/// Port of `CreateGradingToneTransform` (src/OpenColorIO/ops/gradingtone/
/// GradingToneOp.cpp:217-230 @ v2.5.2).
pub(crate) fn create_grading_tone_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::GradingTone(tone_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateGradingToneTransform: op has to be a GradingToneOp.",
        ));
    };
    let mut tone_transform = GradingToneTransform::new(tone_data.get_style());
    tone_transform.data = tone_data.clone();

    group.append_transform(tone_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends a GradingTone op of a copy of it (with its
/// own dynamic property), in the direction `dir` combined with the data's.
///
/// Port of `BuildGradingToneOp` (src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp:
/// 232-243 @ v2.5.2). (The config and context it takes are unused.)
pub(crate) fn build_grading_tone_op(
    ops: &mut OpVec,
    transform: &GradingToneTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate()?;

    let tone_data = data.clone();
    create_grading_tone_op(ops, tone_data, dir);
    Ok(())
}

#[cfg(test)]
#[path = "grading_tone_transform_tests.rs"]
mod tests;
