// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The range transform: a port of `src/OpenColorIO/transforms/RangeTransform.h` and
//! `RangeTransform.cpp` @ v2.5.2, with its op glue from `src/OpenColorIO/ops/range/RangeOp.cpp`
//! (`CreateRangeTransform`, `BuildRangeOp`), and its style, `RangeStyle`
//! (include/OpenColorIO/OpenColorTypes.h) with `RangeStyleToString`
//! (src/OpenColorIO/ParseUtils.cpp).

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    BitDepth, TransformDirection, bit_depth_to_string, transform_direction_to_string,
};
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// Whether a range transform clamps to its bounds.
///
/// Port of `enum RangeStyle` (include/OpenColorIO/OpenColorTypes.h:491-496 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RangeStyle {
    /// `RANGE_NO_CLAMP`: scale and offset only, as a matrix.
    NoClamp = 0,
    /// `RANGE_CLAMP`: scale and offset, clamped to the bounds.
    Clamp,
}

/// The style's name: `noClamp` or `Clamp`.
///
/// Port of `RangeStyleToString` (src/OpenColorIO/ParseUtils.cpp:334-339 @ v2.5.2). Its
/// fallback for a value outside the enum can't happen.
pub fn range_style_to_string(style: RangeStyle) -> &'static str {
    match style {
        RangeStyle::NoClamp => "noClamp",
        RangeStyle::Clamp => "Clamp",
    }
}

/// A range: `out = (in - minIn) * scale + minOut` on RGB, the scale mapping the input bounds to
/// the output bounds, clamped to the output bounds unless the style is
/// [`RangeStyle::NoClamp`]. A bound can be unset (a NaN), which leaves that side unclamped.
///
/// The transform is its op data and its style, as upstream's `RangeTransformImpl` holds them. A
/// copy is upstream's `createEditableCopy`.
///
/// Port of `RangeTransform` and `RangeTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:2080-2191, src/OpenColorIO/transforms/RangeTransform.h,
/// RangeTransform.cpp @ v2.5.2).
#[derive(Debug, Clone)]
pub struct RangeTransform {
    /// `m_style`.
    style: RangeStyle,
    /// `m_data`.
    data: RangeOpData,
}

impl Default for RangeTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeTransform {
    /// A forward range that clamps, without bounds (not valid as it is), with unknown file bit
    /// depths.
    ///
    /// Port of `RangeTransform::Create` (RangeTransform.cpp:14-17 @ v2.5.2) and the members'
    /// initializers (RangeTransform.h:70-71).
    #[doc(alias = "Create")]
    pub fn new() -> RangeTransform {
        RangeTransform {
            style: RangeStyle::Clamp,
            data: RangeOpData::new(),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `RangeTransformImpl::data() const` (RangeTransform.h:65 @ v2.5.2).
    pub(crate) fn data(&self) -> &RangeOpData {
        &self.data
    }

    /// Port of `RangeTransformImpl::getDirection` (RangeTransform.cpp:32-35 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// Port of `RangeTransformImpl::setDirection` (RangeTransform.cpp:37-40 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Port of `RangeTransformImpl::getStyle` (RangeTransform.cpp:42-45 @ v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> RangeStyle {
        self.style
    }

    /// Port of `RangeTransformImpl::setStyle` (RangeTransform.cpp:47-50 @ v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: RangeStyle) {
        self.style = style;
    }

    /// Checks the direction and the bounds (which computes the data's scale and offset), and
    /// that a range that doesn't clamp has both bounds: "RangeTransform validation failed: " and
    /// the first problem. That last check's own message has the prefix already, so it comes
    /// out twice (docs/improvements.md, I-75).
    ///
    /// Port of `RangeTransformImpl::validate` (RangeTransform.cpp:52-73 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = (|| {
            validate_direction(self.direction())?;
            self.data.validate()?;
            if self.style == RangeStyle::NoClamp
                && (self.data.min_is_empty() || self.data.max_is_empty())
            {
                return Err(Exception::new(
                    "RangeTransform validation failed: non clamping range must have min and max \
                     values defined.",
                ));
            }
            Ok(())
        })();
        checked.map_err(|ex| {
            Exception::new([b"RangeTransform validation failed: ".as_slice(), ex.what()].concat())
        })
    }

    /// Port of `RangeTransformImpl::getFormatMetadata() const` (RangeTransform.cpp:80-83 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `RangeTransformImpl::getFormatMetadata()` (RangeTransform.cpp:75-78 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// The bit depth of the file the values came from, at the input.
    ///
    /// Port of `RangeTransformImpl::getFileInputBitDepth` (RangeTransform.cpp:85-88 @ v2.5.2).
    #[doc(alias = "getFileInputBitDepth")]
    pub fn file_input_bit_depth(&self) -> BitDepth {
        self.data.get_file_input_bit_depth()
    }

    /// Port of `RangeTransformImpl::getFileOutputBitDepth` (RangeTransform.cpp:89-92 @ v2.5.2).
    #[doc(alias = "getFileOutputBitDepth")]
    pub fn file_output_bit_depth(&self) -> BitDepth {
        self.data.get_file_output_bit_depth()
    }

    /// Port of `RangeTransformImpl::setFileInputBitDepth` (RangeTransform.cpp:93-96 @ v2.5.2).
    #[doc(alias = "setFileInputBitDepth")]
    pub fn set_file_input_bit_depth(&mut self, bit_depth: BitDepth) {
        self.data.set_file_input_bit_depth(bit_depth);
    }

    /// Port of `RangeTransformImpl::setFileOutputBitDepth` (RangeTransform.cpp:97-100 @ v2.5.2).
    #[doc(alias = "setFileOutputBitDepth")]
    pub fn set_file_output_bit_depth(&mut self, bit_depth: BitDepth) {
        self.data.set_file_output_bit_depth(bit_depth);
    }

    /// Whether `other` has the same data (the data's equality: the direction and the bounds,
    /// within its tolerance, ignoring the metadata and the file bit depths) and style.
    ///
    /// Port of `RangeTransformImpl::equals` (RangeTransform.cpp:102-107 @ v2.5.2).
    pub fn equals(&self, other: &RangeTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data && self.style == other.style()
    }

    /// The lower input bound: a NaN when unset.
    ///
    /// Port of `RangeTransformImpl::getMinInValue` (RangeTransform.cpp:114-117 @ v2.5.2).
    #[doc(alias = "getMinInValue")]
    pub fn min_in_value(&self) -> f64 {
        self.data.get_min_in_value()
    }

    /// Port of `RangeTransformImpl::setMinInValue` (RangeTransform.cpp:109-112 @ v2.5.2).
    #[doc(alias = "setMinInValue")]
    pub fn set_min_in_value(&mut self, val: f64) {
        self.data.set_min_in_value(val);
    }

    /// Whether the lower input bound is set: not a NaN once converted to float.
    ///
    /// Port of `RangeTransformImpl::hasMinInValue` (RangeTransform.cpp:119-122 @ v2.5.2).
    #[doc(alias = "hasMinInValue")]
    pub fn has_min_in_value(&self) -> bool {
        self.data.has_min_in_value()
    }

    /// Port of `RangeTransformImpl::unsetMinInValue` (RangeTransform.cpp:124-127 @ v2.5.2).
    #[doc(alias = "unsetMinInValue")]
    pub fn unset_min_in_value(&mut self) {
        self.data.unset_min_in_value();
    }

    /// Port of `RangeTransformImpl::setMaxInValue` (RangeTransform.cpp:130-133 @ v2.5.2).
    #[doc(alias = "setMaxInValue")]
    pub fn set_max_in_value(&mut self, val: f64) {
        self.data.set_max_in_value(val);
    }

    /// The upper input bound: a NaN when unset.
    ///
    /// Port of `RangeTransformImpl::getMaxInValue` (RangeTransform.cpp:135-138 @ v2.5.2).
    #[doc(alias = "getMaxInValue")]
    pub fn max_in_value(&self) -> f64 {
        self.data.get_max_in_value()
    }

    /// Port of `RangeTransformImpl::hasMaxInValue` (RangeTransform.cpp:140-143 @ v2.5.2).
    #[doc(alias = "hasMaxInValue")]
    pub fn has_max_in_value(&self) -> bool {
        self.data.has_max_in_value()
    }

    /// Port of `RangeTransformImpl::unsetMaxInValue` (RangeTransform.cpp:145-148 @ v2.5.2).
    #[doc(alias = "unsetMaxInValue")]
    pub fn unset_max_in_value(&mut self) {
        self.data.unset_max_in_value();
    }

    /// Port of `RangeTransformImpl::setMinOutValue` (RangeTransform.cpp:151-154 @ v2.5.2).
    #[doc(alias = "setMinOutValue")]
    pub fn set_min_out_value(&mut self, val: f64) {
        self.data.set_min_out_value(val);
    }

    /// The lower output bound: a NaN when unset.
    ///
    /// Port of `RangeTransformImpl::getMinOutValue` (RangeTransform.cpp:156-159 @ v2.5.2).
    #[doc(alias = "getMinOutValue")]
    pub fn min_out_value(&self) -> f64 {
        self.data.get_min_out_value()
    }

    /// Port of `RangeTransformImpl::hasMinOutValue` (RangeTransform.cpp:161-164 @ v2.5.2).
    #[doc(alias = "hasMinOutValue")]
    pub fn has_min_out_value(&self) -> bool {
        self.data.has_min_out_value()
    }

    /// Port of `RangeTransformImpl::unsetMinOutValue` (RangeTransform.cpp:166-169 @ v2.5.2).
    #[doc(alias = "unsetMinOutValue")]
    pub fn unset_min_out_value(&mut self) {
        self.data.unset_min_out_value();
    }

    /// Port of `RangeTransformImpl::setMaxOutValue` (RangeTransform.cpp:172-175 @ v2.5.2).
    #[doc(alias = "setMaxOutValue")]
    pub fn set_max_out_value(&mut self, val: f64) {
        self.data.set_max_out_value(val);
    }

    /// The upper output bound: a NaN when unset.
    ///
    /// Port of `RangeTransformImpl::getMaxOutValue` (RangeTransform.cpp:177-180 @ v2.5.2).
    #[doc(alias = "getMaxOutValue")]
    pub fn max_out_value(&self) -> f64 {
        self.data.get_max_out_value()
    }

    /// Port of `RangeTransformImpl::hasMaxOutValue` (RangeTransform.cpp:182-185 @ v2.5.2).
    #[doc(alias = "hasMaxOutValue")]
    pub fn has_max_out_value(&self) -> bool {
        self.data.has_max_out_value()
    }

    /// Port of `RangeTransformImpl::unsetMaxOutValue` (RangeTransform.cpp:187-190 @ v2.5.2).
    #[doc(alias = "unsetMaxOutValue")]
    pub fn unset_max_out_value(&mut self) {
        self.data.unset_max_out_value();
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const RangeTransform &)` (RangeTransform.cpp:
    /// 193-206 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<RangeTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", fileindepth=");
        os.put_str(bit_depth_to_string(self.file_input_bit_depth()));
        os.put_str(", fileoutdepth=");
        os.put_str(bit_depth_to_string(self.file_output_bit_depth()));
        if self.style() != RangeStyle::Clamp {
            os.put_str(", style=");
            os.put_str(range_style_to_string(self.style()));
        }
        if self.has_min_in_value() {
            os.put_str(", minInValue=");
            os.put_f64(self.min_in_value());
        }
        if self.has_max_in_value() {
            os.put_str(", maxInValue=");
            os.put_f64(self.max_in_value());
        }
        if self.has_min_out_value() {
            os.put_str(", minOutValue=");
            os.put_f64(self.min_out_value());
        }
        if self.has_max_out_value() {
            os.put_str(", maxOutValue=");
            os.put_f64(self.max_out_value());
        }
        os.put_str(">");
    }
}

impl PartialEq for RangeTransform {
    /// [`RangeTransform::equals`].
    fn eq(&self, other: &RangeTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for RangeTransform {
    /// `<RangeTransform direction=<dir>, fileindepth=<depth>, fileoutdepth=<depth>`, then
    /// `, style=noClamp` unless it clamps, and each bound that is set, then `>`.
    ///
    /// Port of `operator<<(std::ostream &, const RangeTransform &)` (RangeTransform.cpp:
    /// 193-206 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends to `group` the transform of the Range op `op`: a copy of its data, metadata and file
/// bit depths included, with the default style (it clamps).
///
/// Port of `CreateRangeTransform` (src/OpenColorIO/ops/range/RangeOp.cpp:246-261 @ v2.5.2).
pub(crate) fn create_range_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Range(range_data_src) = &**op.data() else {
        return Err(Exception::new(
            "CreateRangeTransform: op has to be a RangeOp",
        ));
    };
    let mut range_transform = RangeTransform::new();
    range_transform.data = range_data_src.clone();

    group.append_transform(range_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends, in the direction `dir` combined with the
/// data's, a Range op of a copy of it when the transform clamps, else the Matrix op of its
/// scale and offset.
///
/// Port of `BuildRangeOp` (src/OpenColorIO/ops/range/RangeOp.cpp:263-281 @ v2.5.2).
pub(crate) fn build_range_op(
    ops: &mut OpVec,
    transform: &RangeTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();

    data.validate()?;

    if transform.style() == RangeStyle::Clamp {
        let d = data.clone();
        create_range_op(ops, d, dir)
    } else {
        let m = data.convert_to_matrix()?;
        create_matrix_op(ops, m, dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "range_transform_tests.rs"]
mod tests;
