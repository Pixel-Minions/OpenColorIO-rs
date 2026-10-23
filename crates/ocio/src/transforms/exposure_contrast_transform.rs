// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The exposure and contrast transform: a port of
//! `src/OpenColorIO/transforms/ExposureContrastTransform.h` and `ExposureContrastTransform.cpp`
//! @ v2.5.2, with its op glue from `src/OpenColorIO/ops/exposurecontrast/ExposureContrastOp.cpp`:
//! `CreateExposureContrastTransform` and `BuildExposureContrastOp`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    ExposureContrastStyle, TransformDirection, exposure_contrast_style_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::exposurecontrast::exposure_contrast_op::create_exposure_contrast_op;
use ocio_ops::ops::exposurecontrast::exposure_contrast_op_data::ExposureContrastOpData;

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;

/// Exposure, contrast and gamma adjustments for viewports, in a linear, video or logarithmic
/// space ([`ExposureContrastStyle`]). Exposure, contrast and gamma can each be made dynamic:
/// the processor built from the transform then has a dynamic property of that type, which an
/// application sets between applies.
///
/// The transform is its op data, as upstream's `ExposureContrastTransformImpl` holds it; the
/// op's style carries the direction. The transform holds values and dynamic states: a clone
/// (upstream's `createEditableCopy`) and each processor built from it have their own
/// properties, so changing the transform changes no processor (the owner's decision P5-4).
///
/// Port of `ExposureContrastTransform` and `ExposureContrastTransformImpl`
/// (include/OpenColorIO/OpenColorTransforms.h:1010-1103,
/// src/OpenColorIO/transforms/ExposureContrastTransform.h, ExposureContrastTransform.cpp @
/// v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct ExposureContrastTransform {
    /// `m_data`.
    data: ExposureContrastOpData,
}

impl ExposureContrastTransform {
    /// The linear style, forward: exposure 0, contrast 1 and gamma 1, none dynamic, pivot
    /// 0.18, and the default log exposure step and mid-gray.
    ///
    /// Port of `ExposureContrastTransform::Create` (ExposureContrastTransform.cpp:11-15 @
    /// v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ExposureContrastTransform {
        ExposureContrastTransform::default()
    }

    /// The op data the transform holds.
    ///
    /// Port of `ExposureContrastTransformImpl::data() const` (ExposureContrastTransform.h:67 @
    /// v2.5.2).
    pub(crate) fn data(&self) -> &ExposureContrastOpData {
        &self.data
    }

    /// The direction the op style encodes.
    ///
    /// Port of `ExposureContrastTransformImpl::getDirection` (ExposureContrastTransform.cpp:
    /// 29-32 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.get_direction()
    }

    /// The direction is set by changing the op style.
    ///
    /// Port of `ExposureContrastTransformImpl::setDirection` (ExposureContrastTransform.cpp:
    /// 34-37 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data, which have nothing to refuse; the message would be
    /// "ExposureContrastTransform validation failed: " and the problem.
    ///
    /// Port of `ExposureContrastTransformImpl::validate` (ExposureContrastTransform.cpp:39-52 @
    /// v2.5.2).
    pub fn validate(&self) -> Result<()> {
        validate_direction(self.direction()).map_err(|ex| {
            Exception::new(format!(
                "ExposureContrastTransform validation failed: {}",
                ex.message()
            ))
        })?;
        self.data.validate();
        Ok(())
    }

    /// Port of `ExposureContrastTransformImpl::getFormatMetadata() const`
    /// (ExposureContrastTransform.cpp:59-62 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `ExposureContrastTransformImpl::getFormatMetadata()`
    /// (ExposureContrastTransform.cpp:54-57 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the style (with the direction), the pivot, the log
    /// exposure step and mid-gray, and equal exposure, contrast and gamma, where a dynamic one
    /// equals no other (and a NaN value none); the metadata is ignored. A transform equals
    /// itself.
    ///
    /// Port of `ExposureContrastTransformImpl::equals` (ExposureContrastTransform.cpp:64-68 @
    /// v2.5.2).
    pub fn equals(&self, other: &ExposureContrastTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// The style, without the direction.
    ///
    /// Port of `ExposureContrastTransformImpl::getStyle` (ExposureContrastTransform.cpp:70-73
    /// @ v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> ExposureContrastStyle {
        ExposureContrastOpData::convert_style(self.data.get_style())
    }

    /// Selects the style, in the current direction.
    ///
    /// Port of `ExposureContrastTransformImpl::setStyle` (ExposureContrastTransform.cpp:75-79 @
    /// v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: ExposureContrastStyle) {
        let cur_dir = self.direction();
        self.data
            .set_style(ExposureContrastOpData::convert_transform_style(
                style, cur_dir,
            ));
    }

    /// Port of `ExposureContrastTransformImpl::getExposure` (ExposureContrastTransform.cpp:
    /// 81-84 @ v2.5.2).
    #[doc(alias = "getExposure")]
    pub fn exposure(&self) -> f64 {
        self.data.get_exposure()
    }

    /// Port of `ExposureContrastTransformImpl::setExposure` (ExposureContrastTransform.cpp:
    /// 86-89 @ v2.5.2).
    #[doc(alias = "setExposure")]
    pub fn set_exposure(&mut self, exposure: f64) {
        self.data.set_exposure(exposure);
    }

    /// Port of `ExposureContrastTransformImpl::makeExposureDynamic`
    /// (ExposureContrastTransform.cpp:91-94 @ v2.5.2).
    #[doc(alias = "makeExposureDynamic")]
    pub fn make_exposure_dynamic(&mut self) {
        self.data.get_exposure_property().make_dynamic();
    }

    /// Port of `ExposureContrastTransformImpl::makeExposureNonDynamic`
    /// (ExposureContrastTransform.cpp:96-99 @ v2.5.2).
    #[doc(alias = "makeExposureNonDynamic")]
    pub fn make_exposure_non_dynamic(&mut self) {
        self.data.get_exposure_property().make_non_dynamic();
    }

    /// Port of `ExposureContrastTransformImpl::isExposureDynamic`
    /// (ExposureContrastTransform.cpp:101-104 @ v2.5.2).
    #[doc(alias = "isExposureDynamic")]
    pub fn is_exposure_dynamic(&self) -> bool {
        self.data.get_exposure_property().is_dynamic()
    }

    /// Port of `ExposureContrastTransformImpl::getContrast` (ExposureContrastTransform.cpp:
    /// 106-109 @ v2.5.2).
    #[doc(alias = "getContrast")]
    pub fn contrast(&self) -> f64 {
        self.data.get_contrast()
    }

    /// Port of `ExposureContrastTransformImpl::setContrast` (ExposureContrastTransform.cpp:
    /// 111-114 @ v2.5.2).
    #[doc(alias = "setContrast")]
    pub fn set_contrast(&mut self, contrast: f64) {
        self.data.set_contrast(contrast);
    }

    /// Port of `ExposureContrastTransformImpl::makeContrastDynamic`
    /// (ExposureContrastTransform.cpp:116-119 @ v2.5.2).
    #[doc(alias = "makeContrastDynamic")]
    pub fn make_contrast_dynamic(&mut self) {
        self.data.get_contrast_property().make_dynamic();
    }

    /// Port of `ExposureContrastTransformImpl::makeContrastNonDynamic`
    /// (ExposureContrastTransform.cpp:121-124 @ v2.5.2).
    #[doc(alias = "makeContrastNonDynamic")]
    pub fn make_contrast_non_dynamic(&mut self) {
        self.data.get_contrast_property().make_non_dynamic();
    }

    /// Port of `ExposureContrastTransformImpl::isContrastDynamic`
    /// (ExposureContrastTransform.cpp:126-129 @ v2.5.2).
    #[doc(alias = "isContrastDynamic")]
    pub fn is_contrast_dynamic(&self) -> bool {
        self.data.get_contrast_property().is_dynamic()
    }

    /// Port of `ExposureContrastTransformImpl::getGamma` (ExposureContrastTransform.cpp:
    /// 131-134 @ v2.5.2).
    #[doc(alias = "getGamma")]
    pub fn gamma(&self) -> f64 {
        self.data.get_gamma()
    }

    /// Port of `ExposureContrastTransformImpl::setGamma` (ExposureContrastTransform.cpp:
    /// 136-139 @ v2.5.2).
    #[doc(alias = "setGamma")]
    pub fn set_gamma(&mut self, gamma: f64) {
        self.data.set_gamma(gamma);
    }

    /// Port of `ExposureContrastTransformImpl::makeGammaDynamic`
    /// (ExposureContrastTransform.cpp:141-144 @ v2.5.2).
    #[doc(alias = "makeGammaDynamic")]
    pub fn make_gamma_dynamic(&mut self) {
        self.data.get_gamma_property().make_dynamic();
    }

    /// Port of `ExposureContrastTransformImpl::makeGammaNonDynamic`
    /// (ExposureContrastTransform.cpp:146-149 @ v2.5.2).
    #[doc(alias = "makeGammaNonDynamic")]
    pub fn make_gamma_non_dynamic(&mut self) {
        self.data.get_gamma_property().make_non_dynamic();
    }

    /// Port of `ExposureContrastTransformImpl::isGammaDynamic` (ExposureContrastTransform.cpp:
    /// 151-154 @ v2.5.2).
    #[doc(alias = "isGammaDynamic")]
    pub fn is_gamma_dynamic(&self) -> bool {
        self.data.get_gamma_property().is_dynamic()
    }

    /// Port of `ExposureContrastTransformImpl::getPivot` (ExposureContrastTransform.cpp:
    /// 156-159 @ v2.5.2).
    #[doc(alias = "getPivot")]
    pub fn pivot(&self) -> f64 {
        self.data.get_pivot()
    }

    /// Port of `ExposureContrastTransformImpl::setPivot` (ExposureContrastTransform.cpp:
    /// 161-164 @ v2.5.2).
    #[doc(alias = "setPivot")]
    pub fn set_pivot(&mut self, pivot: f64) {
        self.data.set_pivot(pivot);
    }

    /// Port of `ExposureContrastTransformImpl::getLogExposureStep`
    /// (ExposureContrastTransform.cpp:166-169 @ v2.5.2).
    #[doc(alias = "getLogExposureStep")]
    pub fn log_exposure_step(&self) -> f64 {
        self.data.get_log_exposure_step()
    }

    /// Port of `ExposureContrastTransformImpl::setLogExposureStep`
    /// (ExposureContrastTransform.cpp:171-174 @ v2.5.2).
    #[doc(alias = "setLogExposureStep")]
    pub fn set_log_exposure_step(&mut self, log_exposure_step: f64) {
        self.data.set_log_exposure_step(log_exposure_step);
    }

    /// Port of `ExposureContrastTransformImpl::getLogMidGray` (ExposureContrastTransform.cpp:
    /// 176-179 @ v2.5.2).
    #[doc(alias = "getLogMidGray")]
    pub fn log_mid_gray(&self) -> f64 {
        self.data.get_log_mid_gray()
    }

    /// Port of `ExposureContrastTransformImpl::setLogMidGray` (ExposureContrastTransform.cpp:
    /// 181-184 @ v2.5.2).
    #[doc(alias = "setLogMidGray")]
    pub fn set_log_mid_gray(&mut self, log_mid_gray: f64) {
        self.data.set_log_mid_gray(log_mid_gray);
    }

    /// Writes the transform's text to `os`, its numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const ExposureContrastTransform &)`
    /// (ExposureContrastTransform.cpp:187-214 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<ExposureContrast ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", style=");
        os.put_str(exposure_contrast_style_to_string(self.style()));

        os.put_str(", exposure=");
        os.put_f64(self.exposure());
        os.put_str(", contrast=");
        os.put_f64(self.contrast());
        os.put_str(", gamma=");
        os.put_f64(self.gamma());
        os.put_str(", pivot=");
        os.put_f64(self.pivot());
        os.put_str(", logExposureStep=");
        os.put_f64(self.log_exposure_step());
        os.put_str(", logMidGray=");
        os.put_f64(self.log_mid_gray());
        if self.is_exposure_dynamic() {
            os.put_str(", exposureDynamic");
        }
        if self.is_contrast_dynamic() {
            os.put_str(", contrastDynamic");
        }
        if self.is_gamma_dynamic() {
            os.put_str(", gammaDynamic");
        }

        os.put_str(">");
    }
}

impl PartialEq for ExposureContrastTransform {
    /// [`ExposureContrastTransform::equals`].
    fn eq(&self, other: &ExposureContrastTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for ExposureContrastTransform {
    /// `<ExposureContrast direction=<dir>, style=<style>, exposure=<e>, contrast=<c>,
    /// gamma=<g>, pivot=<p>, logExposureStep=<s>, logMidGray=<m>>`, with `, exposureDynamic`,
    /// `, contrastDynamic` and `, gammaDynamic` before the `>` for those that are dynamic.
    ///
    /// Port of `operator<<(std::ostream &, const ExposureContrastTransform &)`
    /// (ExposureContrastTransform.cpp:187-214 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends to `group` the transform of the ExposureContrast op `op`: its data assigned to a new
/// transform's (`ExposureContrastOpData::operator=`): the metadata, style and values, and the
/// dynamic states, in the transform's own properties.
///
/// Port of `CreateExposureContrastTransform` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOp.cpp:197-210 @ v2.5.2).
pub(crate) fn create_exposure_contrast_transform(
    group: &mut GroupTransform,
    op: &Op,
) -> Result<()> {
    let OpData::ExposureContrast(ec_data) = &**op.data() else {
        return Err(Exception::new(
            "CreateExposureContrastTransform: op has to be a ExposureContrastOp",
        ));
    };
    let mut ec_transform = ExposureContrastTransform::new();
    ec_transform.data.assign(ec_data);

    group.append_transform(ec_transform.into());
    Ok(())
}

/// Validates the transform's data, then appends an ExposureContrast op of a copy of it (with
/// its own dynamic properties), in the direction `dir` combined with the data's.
///
/// Port of `BuildExposureContrastOp` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOp.cpp:212-221 @ v2.5.2).
pub(crate) fn build_exposure_contrast_op(
    ops: &mut OpVec,
    transform: &ExposureContrastTransform,
    dir: TransformDirection,
) -> Result<()> {
    let data = transform.data();
    data.validate();

    let ec_data = data.clone();
    create_exposure_contrast_op(ops, ec_data, dir);
    Ok(())
}

#[cfg(test)]
#[path = "exposure_contrast_transform_tests.rs"]
mod tests;
