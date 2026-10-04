// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The exponent transform: a port of `src/OpenColorIO/transforms/ExponentTransform.h` and
//! `ExponentTransform.cpp` @ v2.5.2, with its op glue: `BuildExponentOp`
//! (src/OpenColorIO/ops/gamma/GammaOp.cpp), which builds an Exponent op in a version 1 config
//! and a Gamma op otherwise, and `CreateExponentTransform`
//! (src/OpenColorIO/ops/exponent/ExponentOp.cpp). `CreateGammaTransform` makes either an
//! exponent or an exponent with linear segment: it is in `exponent_with_linear_transform.rs`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    NegativeStyle, TransformDirection, combine_transform_directions, negative_style_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::exponent::exponent_op::create_exponent_op;
use ocio_ops::ops::exponent::exponent_op_data::ExponentOpData;
use ocio_ops::ops::gamma::gamma_op::create_gamma_op;
use ocio_ops::ops::gamma::gamma_op_data::GammaOpData;

use crate::config::Config;
use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// A power function, `out = in ^ value` per channel, with a style for negative values: clamp
/// (the default), mirror or pass through.
///
/// The transform is its op data, a `GammaOpData` of a basic style, as upstream's
/// `ExponentTransformImpl` holds it; the style carries the direction. A copy is upstream's
/// `createEditableCopy`.
///
/// Port of `ExponentTransform` and `ExponentTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:902-938, src/OpenColorIO/transforms/ExponentTransform.h,
/// ExponentTransform.cpp @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ExponentTransform {
    /// `m_data`.
    data: GammaOpData,
}

impl Default for ExponentTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl ExponentTransform {
    /// Exponents of 1, clamping negative values, forward.
    ///
    /// Port of `ExponentTransform::Create` (ExponentTransform.cpp:13-16 @ v2.5.2) and the
    /// default `GammaOpData` (ExponentTransform.h:19).
    #[doc(alias = "Create")]
    pub fn new() -> ExponentTransform {
        ExponentTransform {
            data: GammaOpData::default(),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `ExponentTransformImpl::data() const` (ExponentTransform.h:43 @ v2.5.2).
    pub(crate) fn data(&self) -> &GammaOpData {
        &self.data
    }

    /// The transform holding `data`: what `CreateGammaTransform` builds from an op.
    pub(crate) fn from_data(data: GammaOpData) -> ExponentTransform {
        ExponentTransform { data }
    }

    /// The direction, which the data's style carries.
    ///
    /// Port of `ExponentTransformImpl::getDirection` (ExponentTransform.cpp:30-33 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.direction()
    }

    /// Port of `ExponentTransformImpl::setDirection` (ExponentTransform.cpp:35-38 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data: "ExponentTransform validation failed: " and the
    /// first problem.
    ///
    /// Port of `ExponentTransformImpl::validate` (ExponentTransform.cpp:40-53 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(format!(
                "ExponentTransform validation failed: {}",
                ex.message()
            ))
        })
    }

    /// Port of `ExponentTransformImpl::getFormatMetadata() const` (ExponentTransform.cpp:60-63
    /// @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `ExponentTransformImpl::getFormatMetadata()` (ExponentTransform.cpp:55-58 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the style and the parameters; the metadata is
    /// ignored. A transform equals itself.
    ///
    /// Port of `ExponentTransformImpl::equals` (ExponentTransform.cpp:65-69 @ v2.5.2).
    pub fn equals(&self, other: &ExponentTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data.equals(&other.data)
    }

    /// Sets the R, G, B and A exponents: the first parameter of each channel.
    ///
    /// Port of `ExponentTransformImpl::setValue` (ExponentTransform.cpp:71-77 @ v2.5.2).
    #[doc(alias = "setValue")]
    pub fn set_value(&mut self, vec4: &[f64; 4]) {
        self.data.red_params_mut()[0] = vec4[0];
        self.data.green_params_mut()[0] = vec4[1];
        self.data.blue_params_mut()[0] = vec4[2];
        self.data.alpha_params_mut()[0] = vec4[3];
    }

    /// The R, G, B and A exponents.
    ///
    /// Port of `ExponentTransformImpl::getValue` (ExponentTransform.cpp:79-85 @ v2.5.2).
    #[doc(alias = "getValue")]
    pub fn value(&self) -> [f64; 4] {
        [
            self.data.red_params()[0],
            self.data.green_params()[0],
            self.data.blue_params()[0],
            self.data.alpha_params()[0],
        ]
    }

    /// How negative values are handled: the style's.
    ///
    /// Port of `ExponentTransformImpl::getNegativeStyle` (ExponentTransform.cpp:87-90 @
    /// v2.5.2).
    #[doc(alias = "getNegativeStyle")]
    pub fn negative_style(&self) -> NegativeStyle {
        GammaOpData::convert_style(self.data.style())
    }

    /// Sets the basic style of `style` in the current direction: "Linear negative
    /// extrapolation is not valid for basic exponent style." for `Linear`, which leaves the
    /// transform as it was.
    ///
    /// Port of `ExponentTransformImpl::setNegativeStyle` (ExponentTransform.cpp:92-96 @ v2.5.2).
    #[doc(alias = "setNegativeStyle")]
    pub fn set_negative_style(&mut self, style: NegativeStyle) -> Result<()> {
        let cur_dir = self.direction();
        self.data
            .set_style(GammaOpData::convert_style_basic(style, cur_dir)?);
        Ok(())
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const ExponentTransform &)`
    /// (ExponentTransform.cpp:98-115 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        let value = self.value();

        os.put_str("<ExponentTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("value=[");
        os.put_f64(value[0]);
        for v in &value[1..] {
            os.put_str(", ");
            os.put_f64(*v);
        }
        os.put_str("], style=");
        os.put_str(negative_style_to_string(self.negative_style()));
        os.put_str(">");
    }
}

impl PartialEq for ExponentTransform {
    /// [`ExponentTransform::equals`].
    fn eq(&self, other: &ExponentTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for ExponentTransform {
    /// `<ExponentTransform direction=<dir>, value=[<r>, <g>, <b>, <a>], style=<style>>`.
    ///
    /// Port of `operator<<(std::ostream &, const ExponentTransform &)`
    /// (ExponentTransform.cpp:98-115 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends the ops of `transform` in the direction `dir`. In a version 1 config, the style is
/// ignored: an Exponent op of the values, with the transform's metadata, in the direction
/// combined with the transform's. Otherwise, after validating the data, a Gamma op of a copy
/// of it in the direction `dir` combined with the data's.
///
/// Port of `BuildExponentOp` (src/OpenColorIO/ops/gamma/GammaOp.cpp:190-216 @ v2.5.2).
pub(crate) fn build_exponent_op(
    ops: &mut OpVec,
    config: &Config,
    transform: &ExponentTransform,
    dir: TransformDirection,
) -> Result<()> {
    if config.major_version() == 1 {
        // Ignore style, use a simple exponent.
        let combined_dir = combine_transform_directions(dir, transform.direction());

        let vec4 = transform.value();
        let mut exp_data = ExponentOpData::from_values(&vec4);
        *exp_data.get_format_metadata_mut() = transform.format_metadata().clone();
        create_exponent_op(ops, exp_data, combined_dir)
    } else {
        let data = transform.data();
        data.validate()?;

        let gamma = data.clone();
        create_gamma_op(ops, gamma, dir);
        Ok(())
    }
}

/// Appends to `group` the transform of the Exponent op `op`: its exponents and metadata,
/// forward, clamping.
///
/// Port of `CreateExponentTransform` (src/OpenColorIO/ops/exponent/ExponentOp.cpp:339-356 @
/// v2.5.2).
pub(crate) fn create_exponent_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Exponent(exp_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateExponentTransform: op has to be a ExponentOp",
        ));
    };
    let mut exp_transform = ExponentTransform::new();

    *exp_transform.format_metadata_mut() = exp_data.get_format_metadata().clone();

    exp_transform.set_value(&exp_data.exp4);

    group.append_transform(exp_transform.into());
    Ok(())
}

#[cfg(test)]
#[path = "exponent_transform_tests.rs"]
mod tests;
