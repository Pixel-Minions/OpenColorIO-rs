// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The log camera transform: a port of `src/OpenColorIO/transforms/LogCameraTransform.h` and
//! `LogCameraTransform.cpp` @ v2.5.2. Its `BuildLogOp` overload and `CreateLogTransform` are in
//! `log_transform.rs`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::open_color_types::{TransformDirection, transform_direction_to_string};
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};

use crate::transform::validate_direction;
use crate::transforms::log_transform::{param3, put_values3, set_param3};

/// A logarithm with affine parameters and a linear segment below a break on the linear side,
/// per channel: the camera log of CLF.
///
/// The transform is its op data, a `LogOpData` of the camera style, as upstream's
/// `LogCameraTransformImpl` holds it. A copy is upstream's `createEditableCopy`.
///
/// Port of `LogCameraTransform` and `LogCameraTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:1652-1702, src/OpenColorIO/transforms/LogCameraTransform.h,
/// LogCameraTransform.cpp:15-175 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LogCameraTransform {
    /// `m_data`.
    data: LogOpData,
}

impl LogCameraTransform {
    /// Base 2, the identity parameters, the break `lin_side_break_values`, no linear slope,
    /// forward.
    ///
    /// Port of `LogCameraTransform::Create` and `LogCameraTransformImpl::LogCameraTransformImpl`
    /// (LogCameraTransform.cpp:15-19, 26-30 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(lin_side_break_values: &[f64; 3]) -> LogCameraTransform {
        let mut data = LogOpData::new(f64::from(2.0f32), TransformDirection::Forward);
        set_param3(
            &mut data,
            LogAffineParameter::LinSideBreak,
            lin_side_break_values,
        );
        LogCameraTransform { data }
    }

    /// The op data the transform holds.
    ///
    /// Port of `LogCameraTransformImpl::data() const` (LogCameraTransform.h:56 @ v2.5.2).
    pub(crate) fn data(&self) -> &LogOpData {
        &self.data
    }

    /// Port of `LogCameraTransformImpl::data()` (LogCameraTransform.h:55 @ v2.5.2).
    pub(crate) fn data_mut(&mut self) -> &mut LogOpData {
        &mut self.data
    }

    /// Port of `LogCameraTransformImpl::getDirection` (LogCameraTransform.cpp:40-43 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.direction()
    }

    /// Port of `LogCameraTransformImpl::setDirection` (LogCameraTransform.cpp:45-48 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction, the data, and that the break is defined: "LogCameraTransform
    /// validation failed: " and the first problem. (The break is always defined: the
    /// constructor sets it.)
    ///
    /// Port of `LogCameraTransformImpl::validate` (LogCameraTransform.cpp:50-67 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = (|| {
            validate_direction(self.direction())?;
            self.data.validate()?;
            if self.data.red_params().len() < 5 {
                return Err(Exception::new("LinSideBreak has to be defined."));
            }
            Ok(())
        })();
        checked.map_err(|ex| {
            Exception::new(
                [
                    b"LogCameraTransform validation failed: ".as_slice(),
                    ex.what(),
                ]
                .concat(),
            )
        })
    }

    /// Port of `LogCameraTransformImpl::getFormatMetadata() const` (LogCameraTransform.cpp:
    /// 74-77 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `LogCameraTransformImpl::getFormatMetadata()` (LogCameraTransform.cpp:69-72 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the direction, the base and the parameters, compared
    /// with `==`; the metadata is ignored. A transform equals itself.
    ///
    /// Port of `LogCameraTransformImpl::equals` (LogCameraTransform.cpp:79-83 @ v2.5.2).
    pub fn equals(&self, other: &LogCameraTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// Port of `LogCameraTransformImpl::setBase` (LogCameraTransform.cpp:85-88 @ v2.5.2).
    #[doc(alias = "setBase")]
    pub fn set_base(&mut self, base: f64) {
        self.data.set_base(base);
    }

    /// Port of `LogCameraTransformImpl::getBase` (LogCameraTransform.cpp:90-93 @ v2.5.2).
    #[doc(alias = "getBase")]
    pub fn base(&self) -> f64 {
        self.data.base()
    }

    /// Port of `LogCameraTransformImpl::setLogSideSlopeValue` (LogCameraTransform.cpp:95-98 @
    /// v2.5.2).
    #[doc(alias = "setLogSideSlopeValue")]
    pub fn set_log_side_slope_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LogSideSlope, values);
    }

    /// Port of `LogCameraTransformImpl::setLogSideOffsetValue` (LogCameraTransform.cpp:99-102
    /// @ v2.5.2).
    #[doc(alias = "setLogSideOffsetValue")]
    pub fn set_log_side_offset_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LogSideOffset, values);
    }

    /// Port of `LogCameraTransformImpl::setLinSideSlopeValue` (LogCameraTransform.cpp:103-106
    /// @ v2.5.2).
    #[doc(alias = "setLinSideSlopeValue")]
    pub fn set_lin_side_slope_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LinSideSlope, values);
    }

    /// Port of `LogCameraTransformImpl::setLinSideOffsetValue` (LogCameraTransform.cpp:107-110
    /// @ v2.5.2).
    #[doc(alias = "setLinSideOffsetValue")]
    pub fn set_lin_side_offset_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LinSideOffset, values);
    }

    /// Port of `LogCameraTransformImpl::getLogSideSlopeValue` (LogCameraTransform.cpp:112-115
    /// @ v2.5.2).
    #[doc(alias = "getLogSideSlopeValue")]
    pub fn log_side_slope_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LogSideSlope)
    }

    /// Port of `LogCameraTransformImpl::getLogSideOffsetValue` (LogCameraTransform.cpp:116-119
    /// @ v2.5.2).
    #[doc(alias = "getLogSideOffsetValue")]
    pub fn log_side_offset_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LogSideOffset)
    }

    /// Port of `LogCameraTransformImpl::getLinSideSlopeValue` (LogCameraTransform.cpp:120-123
    /// @ v2.5.2).
    #[doc(alias = "getLinSideSlopeValue")]
    pub fn lin_side_slope_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LinSideSlope)
    }

    /// Port of `LogCameraTransformImpl::getLinSideOffsetValue` (LogCameraTransform.cpp:124-127
    /// @ v2.5.2).
    #[doc(alias = "getLinSideOffsetValue")]
    pub fn lin_side_offset_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LinSideOffset)
    }

    /// Port of `LogCameraTransformImpl::setLinSideBreakValue` (LogCameraTransform.cpp:129-132
    /// @ v2.5.2).
    #[doc(alias = "setLinSideBreakValue")]
    pub fn set_lin_side_break_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LinSideBreak, values);
    }

    /// Sets the slope of the linear segment, which is otherwise computed from the other
    /// parameters. Upstream throws where the break isn't defined ("Log: LinSideBreak has to be
    /// defined before linearSlope"); a camera log always has it.
    ///
    /// Port of `LogCameraTransformImpl::setLinearSlopeValue` (LogCameraTransform.cpp:134-137 @
    /// v2.5.2).
    #[doc(alias = "setLinearSlopeValue")]
    pub fn set_linear_slope_value(&mut self, values: &[f64; 3]) -> Result<()> {
        self.data.set_value(LogAffineParameter::LinearSlope, values)
    }

    /// Removes the linear slope, if set: it is computed again.
    ///
    /// Port of `LogCameraTransformImpl::unsetLinearSlopeValue` (LogCameraTransform.cpp:139-142
    /// @ v2.5.2).
    #[doc(alias = "unsetLinearSlopeValue")]
    pub fn unset_linear_slope_value(&mut self) {
        self.data.unset_linear_slope();
    }

    /// Port of `LogCameraTransformImpl::getLinSideBreakValue` (LogCameraTransform.cpp:144-147
    /// @ v2.5.2).
    #[doc(alias = "getLinSideBreakValue")]
    pub fn lin_side_break_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LinSideBreak)
    }

    /// The linear slope, if it is set.
    ///
    /// Port of `LogCameraTransformImpl::getLinearSlopeValue` (LogCameraTransform.cpp:149-152 @
    /// v2.5.2), which returns whether it is set and writes it to the caller's array when it is.
    #[doc(alias = "getLinearSlopeValue")]
    pub fn linear_slope_value(&self) -> Option<[f64; 3]> {
        self.data
            .value(LogAffineParameter::LinearSlope)
            .expect("the transforms' channels have the same parameters")
    }

    /// Writes the transform's text to `os`, the numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const LogCameraTransform &)`
    /// (LogCameraTransform.cpp:154-175 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<LogCameraTransform");
        os.put_str(" direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", base=");
        os.put_f64(self.base());
        put_values3(os, "logSideSlope", &self.log_side_slope_value());
        put_values3(os, "logSideOffset", &self.log_side_offset_value());
        put_values3(os, "linSideSlope", &self.lin_side_slope_value());
        put_values3(os, "linSideOffset", &self.lin_side_offset_value());
        put_values3(os, "linSideBreak", &self.lin_side_break_value());
        if let Some(values) = self.linear_slope_value() {
            put_values3(os, "linearSlope", &values);
        }
        os.put_str(">");
    }
}

impl PartialEq for LogCameraTransform {
    /// [`LogCameraTransform::equals`].
    fn eq(&self, other: &LogCameraTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for LogCameraTransform {
    /// `<LogCameraTransform direction=<dir>, base=<base>, logSideSlope=[<3 values>], ...,
    /// linSideBreak=[...]`, then `, linearSlope=[...]` when it is set, then `>`.
    ///
    /// Port of `operator<<(std::ostream &, const LogCameraTransform &)`
    /// (LogCameraTransform.cpp:154-175 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

#[cfg(test)]
#[path = "log_camera_transform_tests.rs"]
mod tests;
