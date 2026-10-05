// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The fixed function transform: a port of `src/OpenColorIO/transforms/FixedFunctionTransform.h`
//! and `FixedFunctionTransform.cpp` @ v2.5.2, with its op glue from
//! `src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.cpp`: `BuildFixedFunctionOp` and
//! `CreateFixedFunctionTransform`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    FixedFunctionStyle, TransformDirection, fixed_function_style_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::fixedfunction::FixedFunctionOpStyle;
use ocio_ops::ops::fixedfunction::fixed_function_op::create_fixed_function_op_from_data;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpData;

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// A fixed function: one of a set of fixed algorithms, chosen by its style, some with
/// parameters ([`FixedFunctionStyle`]).
///
/// The transform is its op data, as upstream's `FixedFunctionTransformImpl` holds it; the
/// op's style carries the direction.
///
/// `Clone` copies the data as it is. Upstream's `createEditableCopy` makes the copy with the
/// validating `Create`, so it refuses invalid parameters: that is
/// [`create_editable_copy`](Self::create_editable_copy).
///
/// Port of `FixedFunctionTransform` and `FixedFunctionTransformImpl`
/// (include/OpenColorIO/OpenColorTransforms.h:1187-1221,
/// src/OpenColorIO/transforms/FixedFunctionTransform.h, FixedFunctionTransform.cpp @ v2.5.2).
#[derive(Debug, Clone)]
pub struct FixedFunctionTransform {
    /// `m_data`.
    data: FixedFunctionOpData,
}

impl FixedFunctionTransform {
    /// The forward `style` with `params`, validated: a style that takes another number of
    /// parameters, a parameter out of its bounds, and the two styles upstream doesn't
    /// implement are refused, with the op data's messages (no prefix).
    ///
    /// Port of `FixedFunctionTransform::Create(style)` and `Create(style, params, num)`, and
    /// the `FixedFunctionTransformImpl` constructors (FixedFunctionTransform.cpp:14-47 @
    /// v2.5.2). (`Create`'s error for a null pointer with a count can't happen with a
    /// slice.)
    #[doc(alias = "Create")]
    pub fn new(style: FixedFunctionStyle, params: &[f64]) -> Result<FixedFunctionTransform> {
        let op_style =
            FixedFunctionOpStyle::from_transform_style(style, TransformDirection::Forward)?;
        Ok(FixedFunctionTransform {
            data: FixedFunctionOpData::with_params(op_style, params.to_vec())?,
        })
    }

    /// A copy made as upstream's `createEditableCopy` makes it: a new transform of the style
    /// and parameters ([`new`](Self::new), which validates them), then given a copy of the
    /// data, metadata included.
    ///
    /// Port of `FixedFunctionTransformImpl::createEditableCopy` (FixedFunctionTransform.cpp:
    /// 54-73 @ v2.5.2).
    #[doc(alias = "createEditableCopy")]
    pub fn create_editable_copy(&self) -> Result<FixedFunctionTransform> {
        let mut transform = FixedFunctionTransform::new(self.style(), self.data.params())?;
        // Also copy the Format Metadata if any.
        transform.data = self.data.clone();
        Ok(transform)
    }

    /// The op data the transform holds.
    ///
    /// Port of `FixedFunctionTransformImpl::data() const` (FixedFunctionTransform.h:46 @
    /// v2.5.2).
    pub(crate) fn data(&self) -> &FixedFunctionOpData {
        &self.data
    }

    /// The direction the op style encodes.
    ///
    /// Port of `FixedFunctionTransformImpl::getDirection` (FixedFunctionTransform.cpp:75-78 @
    /// v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.direction()
    }

    /// The direction is set by changing the op style.
    ///
    /// Port of `FixedFunctionTransformImpl::setDirection` (FixedFunctionTransform.cpp:80-84 @
    /// v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        // NB: The direction is set by modifying the OpData style.
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data: "FixedFunctionTransform validation failed: " and the
    /// first problem.
    ///
    /// Port of `FixedFunctionTransformImpl::validate` (FixedFunctionTransform.cpp:86-99 @
    /// v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(format!(
                "FixedFunctionTransform validation failed: {}",
                ex.message()
            ))
        })
    }

    /// Port of `FixedFunctionTransformImpl::getFormatMetadata() const`
    /// (FixedFunctionTransform.cpp:106-109 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `FixedFunctionTransformImpl::getFormatMetadata()`
    /// (FixedFunctionTransform.cpp:101-104 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the style and the parameters (a NaN parameter is
    /// never equal); the metadata is ignored. A transform equals itself.
    ///
    /// Port of `FixedFunctionTransformImpl::equals` (FixedFunctionTransform.cpp:111-115 @
    /// v2.5.2).
    pub fn equals(&self, other: &FixedFunctionTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data.equals(&other.data)
    }

    /// The transform style of the op style, in either direction.
    ///
    /// Port of `FixedFunctionTransformImpl::getStyle` (FixedFunctionTransform.cpp:117-120 @
    /// v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> FixedFunctionStyle {
        self.data.style().to_transform_style()
    }

    /// Selects the algorithm, in the current direction; the parameters stay and nothing is
    /// validated. The two styles upstream doesn't implement are refused, leaving the transform
    /// as it was. The styles of RGB to HSV and of XYZ to xyY, u'v'Y and LUV are always
    /// forward, so they make an inverse transform forward
    /// ([`FixedFunctionOpStyle::from_transform_style`], docs/improvements.md, I-80).
    ///
    /// Port of `FixedFunctionTransformImpl::setStyle` (FixedFunctionTransform.cpp:122-126 @
    /// v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: FixedFunctionStyle) -> Result<()> {
        let cur_dir = self.direction();
        self.data
            .set_style(FixedFunctionOpStyle::from_transform_style(style, cur_dir)?);
        Ok(())
    }

    /// The parameters.
    ///
    /// Port of `FixedFunctionTransformImpl::getNumParams` and `getParams`
    /// (FixedFunctionTransform.cpp:128-131, 147-151 @ v2.5.2), with the binding's
    /// `getParams` (src/bindings/python/transforms/PyFixedFunctionTransform.cpp:10-15).
    #[doc(alias = "getParams")]
    #[doc(alias = "getNumParams")]
    pub fn params(&self) -> Vec<f64> {
        self.data.params().clone()
    }

    /// Sets the parameters (for the styles that take some); nothing is validated.
    ///
    /// Port of `FixedFunctionTransformImpl::setParams` (FixedFunctionTransform.cpp:133-145 @
    /// v2.5.2). (Its error for a null pointer with a count can't happen with a slice.)
    #[doc(alias = "setParams")]
    pub fn set_params(&mut self, params: &[f64]) {
        self.data.set_params(params.to_vec());
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const FixedFunctionTransform &)`
    /// (FixedFunctionTransform.cpp:153-176 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<FixedFunction ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", style=");
        // `getStyle` never gives the two styles that `FixedFunctionStyleToString` refuses.
        let style = fixed_function_style_to_string(self.style())
            .unwrap_or_else(|e| unreachable!("{}", e.message()));
        os.put_str(style);
        let params = self.data.params();
        if let Some((first, rest)) = params.split_first() {
            os.put_str(", params=[");
            os.put_f64(*first);
            for param in rest {
                os.put_str(", ");
                os.put_f64(*param);
            }
            os.put_str("]");
        }
        os.put_str(">");
    }
}

impl PartialEq for FixedFunctionTransform {
    /// [`FixedFunctionTransform::equals`].
    fn eq(&self, other: &FixedFunctionTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for FixedFunctionTransform {
    /// `<FixedFunction direction=<dir>, style=<style>>`, with `, params=[<values>]` before the
    /// `>` for a transform with parameters.
    ///
    /// Port of `operator<<(std::ostream &, const FixedFunctionTransform &)`
    /// (FixedFunctionTransform.cpp:153-176 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends to `group` the transform of the FixedFunction op `op`: a copy of its data, metadata
/// included.
///
/// Port of `CreateFixedFunctionTransform` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOp.cpp:165-178 @ v2.5.2).
pub(crate) fn create_fixed_function_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::FixedFunction(ff_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateFixedFunctionTransform: op has to be a FixedFunctionOp",
        ));
    };
    let mut ff_transform = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[])?;
    ff_transform.data = ff_data.clone();
    group.append_transform(ff_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends a FixedFunction op of a copy of it (which
/// validates it again), in the direction `dir` combined with the data's.
///
/// Port of `BuildFixedFunctionOp` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.cpp:
/// 180-189 @ v2.5.2).
pub(crate) fn build_fixed_function_op(
    ops: &mut OpVec,
    transform: &FixedFunctionTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate()?;
    let func_data = data.try_clone()?;
    create_fixed_function_op_from_data(ops, func_data, dir)
}

#[cfg(test)]
#[path = "fixed_function_transform_tests.rs"]
mod tests;
