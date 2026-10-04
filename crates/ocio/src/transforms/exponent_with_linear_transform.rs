// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The exponent with linear segment transform: a port of
//! `src/OpenColorIO/transforms/ExponentWithLinearTransform.h` and
//! `ExponentWithLinearTransform.cpp` @ v2.5.2, with its op glue from
//! `src/OpenColorIO/ops/gamma/GammaOp.cpp`: `BuildExponentWithLinearOp`, and
//! `CreateGammaTransform`, which makes this transform of a moncurve Gamma op and an
//! [`ExponentTransform`] of a basic one.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    NegativeStyle, TransformDirection, negative_style_to_string, transform_direction_to_string,
};
use ocio_ops::ops::gamma::gamma_op::create_gamma_op;
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};

use crate::transform::validate_direction;
use crate::transforms::exponent_transform::ExponentTransform;
use crate::transforms::group_transform::GroupTransform;

/// A power function with a linear segment near black, `out = ((in + offset) / (1 + offset)) ^
/// gamma` above the break and linear below it, per channel, with a style for negative values:
/// linear (the default) or mirror.
///
/// The transform is its op data, a `GammaOpData` of a moncurve style, as upstream's
/// `ExponentWithLinearTransformImpl` holds it; the style carries the direction. A copy is
/// upstream's `createEditableCopy`.
///
/// Port of `ExponentWithLinearTransform` and `ExponentWithLinearTransformImpl`
/// (include/OpenColorIO/OpenColorTransforms.h:952-1003,
/// src/OpenColorIO/transforms/ExponentWithLinearTransform.h, ExponentWithLinearTransform.cpp
/// @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ExponentWithLinearTransform {
    /// `m_data`.
    data: GammaOpData,
}

impl Default for ExponentWithLinearTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl ExponentWithLinearTransform {
    /// Gammas of 1 and offsets of 0, `MONCURVE_FWD`.
    ///
    /// Port of `ExponentWithLinearTransform::Create` and
    /// `ExponentWithLinearTransformImpl::ExponentWithLinearTransformImpl`
    /// (ExponentWithLinearTransform.cpp:14-18, 25-33 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ExponentWithLinearTransform {
        let mut data = GammaOpData::default();
        data.set_red_params(vec![1., 0.]);
        data.set_green_params(vec![1., 0.]);
        data.set_blue_params(vec![1., 0.]);
        data.set_alpha_params(vec![1., 0.]);

        data.set_style(GammaStyle::MoncurveFwd);
        ExponentWithLinearTransform { data }
    }

    /// The op data the transform holds.
    ///
    /// Port of `ExponentWithLinearTransformImpl::data() const`
    /// (ExponentWithLinearTransform.h:46 @ v2.5.2).
    pub(crate) fn data(&self) -> &GammaOpData {
        &self.data
    }

    /// Forward for `MONCURVE_FWD` and `MONCURVE_MIRROR_FWD`, inverse for every other style.
    ///
    /// Port of `ExponentWithLinearTransformImpl::getDirection`
    /// (ExponentWithLinearTransform.cpp:42-53 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        let style = self.data.style();
        if style == GammaStyle::MoncurveFwd || style == GammaStyle::MoncurveMirrorFwd {
            TransformDirection::Forward
        } else {
            TransformDirection::Inverse
        }
    }

    /// Port of `ExponentWithLinearTransformImpl::setDirection`
    /// (ExponentWithLinearTransform.cpp:55-58 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data: "ExponentWithLinearTransform validation failed: " and
    /// the first problem.
    ///
    /// Port of `ExponentWithLinearTransformImpl::validate` (ExponentWithLinearTransform.cpp:
    /// 60-73 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(format!(
                "ExponentWithLinearTransform validation failed: {}",
                ex.message()
            ))
        })
    }

    /// Port of `ExponentWithLinearTransformImpl::getFormatMetadata() const`
    /// (ExponentWithLinearTransform.cpp:80-83 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `ExponentWithLinearTransformImpl::getFormatMetadata()`
    /// (ExponentWithLinearTransform.cpp:75-78 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the style and the parameters; the metadata is
    /// ignored. A transform equals itself.
    ///
    /// Port of `ExponentWithLinearTransformImpl::equals` (ExponentWithLinearTransform.cpp:85-89
    /// @ v2.5.2).
    pub fn equals(&self, other: &ExponentWithLinearTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data.equals(&other.data)
    }

    /// Sets the R, G, B and A gammas: the first parameter of each channel.
    ///
    /// Port of `ExponentWithLinearTransformImpl::setGamma` (ExponentWithLinearTransform.cpp:
    /// 91-97 @ v2.5.2).
    #[doc(alias = "setGamma")]
    pub fn set_gamma(&mut self, values: &[f64; 4]) {
        self.data.red_params_mut()[0] = values[0];
        self.data.green_params_mut()[0] = values[1];
        self.data.blue_params_mut()[0] = values[2];
        self.data.alpha_params_mut()[0] = values[3];
    }

    /// The R, G, B and A gammas.
    ///
    /// Port of `ExponentWithLinearTransformImpl::getGamma` (ExponentWithLinearTransform.cpp:
    /// 99-105 @ v2.5.2).
    #[doc(alias = "getGamma")]
    pub fn gamma(&self) -> [f64; 4] {
        [
            self.data.red_params()[0],
            self.data.green_params()[0],
            self.data.blue_params()[0],
            self.data.alpha_params()[0],
        ]
    }

    /// Sets the R, G, B and A offsets: each channel's parameters become its gamma and the
    /// offset.
    ///
    /// Port of `ExponentWithLinearTransformImpl::setOffset` (ExponentWithLinearTransform.cpp:
    /// 107-118 @ v2.5.2).
    #[doc(alias = "setOffset")]
    pub fn set_offset(&mut self, values: &[f64; 4]) {
        let red = vec![self.data.red_params()[0], values[0]];
        let grn = vec![self.data.green_params()[0], values[1]];
        let blu = vec![self.data.blue_params()[0], values[2]];
        let alp = vec![self.data.alpha_params()[0], values[3]];
        self.data.set_red_params(red);
        self.data.set_green_params(grn);
        self.data.set_blue_params(blu);
        self.data.set_alpha_params(alp);
    }

    /// The R, G, B and A offsets: 0 for a channel without one.
    ///
    /// Port of `ExponentWithLinearTransformImpl::getOffset` (ExponentWithLinearTransform.cpp:
    /// 120-126 @ v2.5.2).
    #[doc(alias = "getOffset")]
    pub fn offset(&self) -> [f64; 4] {
        let second = |p: &Vec<f64>| if p.len() == 2 { p[1] } else { 0. };
        [
            second(self.data.red_params()),
            second(self.data.green_params()),
            second(self.data.blue_params()),
            second(self.data.alpha_params()),
        ]
    }

    /// How negative values are handled: the style's.
    ///
    /// Port of `ExponentWithLinearTransformImpl::getNegativeStyle`
    /// (ExponentWithLinearTransform.cpp:128-131 @ v2.5.2).
    #[doc(alias = "getNegativeStyle")]
    pub fn negative_style(&self) -> NegativeStyle {
        GammaOpData::convert_style(self.data.style())
    }

    /// Sets the moncurve style of `style` in the current direction: "Pass thru negative
    /// extrapolation is not valid for MonCurve exponent style." or "Clamp negative extrapolation
    /// is not valid for MonCurve exponent style.", which leave the transform as it was.
    ///
    /// Port of `ExponentWithLinearTransformImpl::setNegativeStyle`
    /// (ExponentWithLinearTransform.cpp:133-138 @ v2.5.2).
    #[doc(alias = "setNegativeStyle")]
    pub fn set_negative_style(&mut self, style: NegativeStyle) -> Result<()> {
        let dir = self.direction();
        let style_op = GammaOpData::convert_style_mon_curve(style, dir)?;
        self.data.set_style(style_op);
        Ok(())
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const ExponentWithLinearTransform &)`
    /// (ExponentWithLinearTransform.cpp:140-166 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<ExponentWithLinearTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        let gamma = self.gamma();
        os.put_str("gamma=[");
        os.put_f64(gamma[0]);
        for v in &gamma[1..] {
            os.put_str(", ");
            os.put_f64(*v);
        }
        let offset = self.offset();
        os.put_str("], offset=[");
        os.put_f64(offset[0]);
        for v in &offset[1..] {
            os.put_str(", ");
            os.put_f64(*v);
        }
        os.put_str("], style=");
        os.put_str(negative_style_to_string(self.negative_style()));
        os.put_str(">");
    }
}

impl PartialEq for ExponentWithLinearTransform {
    /// [`ExponentWithLinearTransform::equals`].
    fn eq(&self, other: &ExponentWithLinearTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for ExponentWithLinearTransform {
    /// `<ExponentWithLinearTransform direction=<dir>, gamma=[<4 values>], offset=[<4 values>],
    /// style=<style>>`.
    ///
    /// Port of `operator<<(std::ostream &, const ExponentWithLinearTransform &)`
    /// (ExponentWithLinearTransform.cpp:140-166 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends to `group` the transform of the Gamma op `op`: an exponent with linear segment for
/// a moncurve style, an exponent for a basic one, holding a copy of the op's data.
///
/// Port of `CreateGammaTransform` (src/OpenColorIO/ops/gamma/GammaOp.cpp:149-177 @ v2.5.2).
pub(crate) fn create_gamma_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Gamma(gamma_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateGammaTransform: op has to be a GammaOp",
        ));
    };

    let style = gamma_data.style();

    if style == GammaStyle::MoncurveFwd
        || style == GammaStyle::MoncurveMirrorFwd
        || style == GammaStyle::MoncurveRev
        || style == GammaStyle::MoncurveMirrorRev
    {
        let mut exp_transform = ExponentWithLinearTransform::new();
        exp_transform.data = gamma_data.clone();
        group.append_transform(exp_transform.into());
    } else {
        let exp_transform = ExponentTransform::from_data(gamma_data.clone());
        group.append_transform(exp_transform.into());
    }
    Ok(())
}

/// Validates the transform's data, then appends a Gamma op of a copy of it in the direction
/// `dir` combined with the data's.
///
/// Port of `BuildExponentWithLinearOp` (src/OpenColorIO/ops/gamma/GammaOp.cpp:179-188 @
/// v2.5.2).
pub(crate) fn build_exponent_with_linear_op(
    ops: &mut OpVec,
    transform: &ExponentWithLinearTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate()?;

    let gamma = data.clone();
    create_gamma_op(ops, gamma, dir);
    Ok(())
}

#[cfg(test)]
#[path = "exponent_with_linear_transform_tests.rs"]
mod tests;
