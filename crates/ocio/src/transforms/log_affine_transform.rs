// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The log affine transform: a port of `src/OpenColorIO/transforms/LogAffineTransform.h` and
//! `LogAffineTransform.cpp` @ v2.5.2. Its `BuildLogOp` overload and `CreateLogTransform` are in
//! `log_transform.rs`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::open_color_types::{TransformDirection, transform_direction_to_string};
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};

use crate::transform::validate_direction;
use crate::transforms::log_transform::{param3, put_values3, set_param3};

/// A logarithm with affine parameters: `out = logSideSlope * log(linSideSlope * in +
/// linSideOffset) / log(base) + logSideOffset`, per channel.
///
/// The transform is its op data, a `LogOpData`, as upstream's `LogAffineTransformImpl` holds
/// it. A copy is upstream's `createEditableCopy`.
///
/// Port of `LogAffineTransform` and `LogAffineTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:1604-1640, src/OpenColorIO/transforms/LogAffineTransform.h,
/// LogAffineTransform.cpp:15-139 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LogAffineTransform {
    /// `m_data`.
    data: LogOpData,
}

impl Default for LogAffineTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl LogAffineTransform {
    /// Base 2, the identity parameters, forward.
    ///
    /// Port of `LogAffineTransform::Create` and `LogAffineTransformImpl::LogAffineTransformImpl`
    /// (LogAffineTransform.cpp:15-18, 25-28 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> LogAffineTransform {
        LogAffineTransform {
            data: LogOpData::new(f64::from(2.0f32), TransformDirection::Forward),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `LogAffineTransformImpl::data() const` (LogAffineTransform.h:48 @ v2.5.2).
    pub(crate) fn data(&self) -> &LogOpData {
        &self.data
    }

    /// Port of `LogAffineTransformImpl::data()` (LogAffineTransform.h:47 @ v2.5.2).
    pub(crate) fn data_mut(&mut self) -> &mut LogOpData {
        &mut self.data
    }

    /// Port of `LogAffineTransformImpl::getDirection` (LogAffineTransform.cpp:37-40 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.direction()
    }

    /// Port of `LogAffineTransformImpl::setDirection` (LogAffineTransform.cpp:42-45 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data: "LogAffineTransform validation failed: " and the
    /// first problem.
    ///
    /// Port of `LogAffineTransformImpl::validate` (LogAffineTransform.cpp:47-60 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(format!(
                "LogAffineTransform validation failed: {}",
                ex.message()
            ))
        })
    }

    /// Port of `LogAffineTransformImpl::getFormatMetadata() const` (LogAffineTransform.cpp:
    /// 67-70 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `LogAffineTransformImpl::getFormatMetadata()` (LogAffineTransform.cpp:62-65 @
    /// v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the direction, the base and the parameters, compared
    /// with `==`; the metadata is ignored. A transform equals itself.
    ///
    /// Port of `LogAffineTransformImpl::equals` (LogAffineTransform.cpp:72-76 @ v2.5.2).
    pub fn equals(&self, other: &LogAffineTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// Port of `LogAffineTransformImpl::setBase` (LogAffineTransform.cpp:78-81 @ v2.5.2).
    #[doc(alias = "setBase")]
    pub fn set_base(&mut self, base: f64) {
        self.data.set_base(base);
    }

    /// Port of `LogAffineTransformImpl::getBase` (LogAffineTransform.cpp:83-86 @ v2.5.2).
    #[doc(alias = "getBase")]
    pub fn base(&self) -> f64 {
        self.data.base()
    }

    /// Port of `LogAffineTransformImpl::setLogSideSlopeValue` (LogAffineTransform.cpp:88-91 @
    /// v2.5.2).
    #[doc(alias = "setLogSideSlopeValue")]
    pub fn set_log_side_slope_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LogSideSlope, values);
    }

    /// Port of `LogAffineTransformImpl::setLogSideOffsetValue` (LogAffineTransform.cpp:92-95 @
    /// v2.5.2).
    #[doc(alias = "setLogSideOffsetValue")]
    pub fn set_log_side_offset_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LogSideOffset, values);
    }

    /// Port of `LogAffineTransformImpl::setLinSideSlopeValue` (LogAffineTransform.cpp:96-99 @
    /// v2.5.2).
    #[doc(alias = "setLinSideSlopeValue")]
    pub fn set_lin_side_slope_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LinSideSlope, values);
    }

    /// Port of `LogAffineTransformImpl::setLinSideOffsetValue` (LogAffineTransform.cpp:100-103
    /// @ v2.5.2).
    #[doc(alias = "setLinSideOffsetValue")]
    pub fn set_lin_side_offset_value(&mut self, values: &[f64; 3]) {
        set_param3(&mut self.data, LogAffineParameter::LinSideOffset, values);
    }

    /// Port of `LogAffineTransformImpl::getLogSideSlopeValue` (LogAffineTransform.cpp:105-108
    /// @ v2.5.2).
    #[doc(alias = "getLogSideSlopeValue")]
    pub fn log_side_slope_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LogSideSlope)
    }

    /// Port of `LogAffineTransformImpl::getLogSideOffsetValue` (LogAffineTransform.cpp:109-112
    /// @ v2.5.2).
    #[doc(alias = "getLogSideOffsetValue")]
    pub fn log_side_offset_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LogSideOffset)
    }

    /// Port of `LogAffineTransformImpl::getLinSideSlopeValue` (LogAffineTransform.cpp:113-116
    /// @ v2.5.2).
    #[doc(alias = "getLinSideSlopeValue")]
    pub fn lin_side_slope_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LinSideSlope)
    }

    /// Port of `LogAffineTransformImpl::getLinSideOffsetValue` (LogAffineTransform.cpp:117-120
    /// @ v2.5.2).
    #[doc(alias = "getLinSideOffsetValue")]
    pub fn lin_side_offset_value(&self) -> [f64; 3] {
        param3(&self.data, LogAffineParameter::LinSideOffset)
    }

    /// Writes the transform's text to `os`, the numbers with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const LogAffineTransform &)`
    /// (LogAffineTransform.cpp:122-139 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<LogAffineTransform");
        os.put_str(" direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", base=");
        os.put_f64(self.base());
        put_values3(os, "logSideSlope", &self.log_side_slope_value());
        put_values3(os, "logSideOffset", &self.log_side_offset_value());
        put_values3(os, "linSideSlope", &self.lin_side_slope_value());
        put_values3(os, "linSideOffset", &self.lin_side_offset_value());
        os.put_str(">");
    }
}

impl PartialEq for LogAffineTransform {
    /// [`LogAffineTransform::equals`].
    fn eq(&self, other: &LogAffineTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for LogAffineTransform {
    /// `<LogAffineTransform direction=<dir>, base=<base>, logSideSlope=[<3 values>],
    /// logSideOffset=[...], linSideSlope=[...], linSideOffset=[...]>`.
    ///
    /// Port of `operator<<(std::ostream &, const LogAffineTransform &)`
    /// (LogAffineTransform.cpp:122-139 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

#[cfg(test)]
#[path = "log_affine_transform_tests.rs"]
mod tests;
