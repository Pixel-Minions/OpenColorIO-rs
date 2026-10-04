// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The log transform: a port of `src/OpenColorIO/transforms/LogTransform.h` and
//! `LogTransform.cpp` @ v2.5.2, with its op glue from `src/OpenColorIO/ops/log/LogOp.cpp`:
//! `BuildLogOp(LogTransform)`, and `CreateLogTransform`, which makes a log, a log affine or a
//! log camera transform of a Log op, so it is here with the simplest of the three. The three
//! classes share the helpers of the Log op data's parameters below.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{TransformDirection, transform_direction_to_string};
use ocio_ops::ops::log::log_op::create_log_op;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData, SHORT_PARAMS};

use crate::transform::validate_direction;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::log_affine_transform::LogAffineTransform;
use crate::transforms::log_camera_transform::LogCameraTransform;

/// The data's parameter `param` of the three channels: one the data always has (the four
/// affine ones; the break of a camera log).
///
/// Port of `LogOpData::getValue` as the transforms call it (`LogAffineTransformImpl` and
/// `LogCameraTransformImpl`'s getters), on parameters every channel has.
pub(crate) fn param3(data: &LogOpData, param: LogAffineParameter) -> [f64; 3] {
    data.value(param)
        .expect("the transforms' channels have the same parameters")
        .expect("the transforms' data has the parameter")
}

/// Sets the data's parameter `param` of the three channels: one the data always has, or the
/// break, which grows the parameters.
///
/// Port of `LogOpData::setValue` as the transforms call it (`LogAffineTransformImpl` and
/// `LogCameraTransformImpl`'s setters).
pub(crate) fn set_param3(data: &mut LogOpData, param: LogAffineParameter, values: &[f64; 3]) {
    data.set_value(param, values)
        .expect("the transforms' channels have the parameter, or it is the break");
}

/// Writes `, <name>=[<r>, <g>, <b>]`, the values with the stream's precision.
pub(crate) fn put_values3(os: &mut OStringStream, name: &str, values: &[f64; 3]) {
    os.put_str(", ");
    os.put_str(name);
    os.put_str("=[");
    os.put_f64(values[0]);
    os.put_str(", ");
    os.put_f64(values[1]);
    os.put_str(", ");
    os.put_f64(values[2]);
    os.put_str("]");
}

/// A logarithm, `out = log(in) / log(base)` per channel.
///
/// The transform is its op data, a `LogOpData` with the default affine parameters, as
/// upstream's `LogTransformImpl` holds it. A copy is upstream's `createEditableCopy`.
///
/// Port of `LogTransform` and `LogTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:1712-1737, src/OpenColorIO/transforms/LogTransform.h,
/// LogTransform.cpp:14-95 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LogTransform {
    /// `m_data`.
    data: LogOpData,
}

impl Default for LogTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl LogTransform {
    /// Base 2, forward.
    ///
    /// Port of `LogTransform::Create` and `LogTransformImpl::LogTransformImpl`
    /// (LogTransform.cpp:14-17, 24-27 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> LogTransform {
        LogTransform {
            data: LogOpData::new(f64::from(2.0f32), TransformDirection::Forward),
        }
    }

    /// The op data the transform holds.
    ///
    /// Port of `LogTransformImpl::data() const` (LogTransform.h:40 @ v2.5.2).
    pub(crate) fn data(&self) -> &LogOpData {
        &self.data
    }

    /// Port of `LogTransformImpl::getDirection` (LogTransform.cpp:36-39 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.data.direction()
    }

    /// Port of `LogTransformImpl::setDirection` (LogTransform.cpp:41-44 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.data.set_direction(dir);
    }

    /// Checks the direction and the data: "LogTransform validation failed: " and the first
    /// problem.
    ///
    /// Port of `LogTransformImpl::validate` (LogTransform.cpp:46-59 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        let checked = validate_direction(self.direction()).and_then(|()| self.data.validate());
        checked.map_err(|ex| {
            Exception::new(format!("LogTransform validation failed: {}", ex.message()))
        })
    }

    /// Port of `LogTransformImpl::getFormatMetadata() const` (LogTransform.cpp:66-69 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        self.data.get_format_metadata()
    }

    /// Port of `LogTransformImpl::getFormatMetadata()` (LogTransform.cpp:61-64 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        self.data.get_format_metadata_mut()
    }

    /// Whether `other` has the same data: the direction, the base and the parameters, compared
    /// with `==`; the metadata is ignored. A transform equals itself.
    ///
    /// Port of `LogTransformImpl::equals` (LogTransform.cpp:71-75 @ v2.5.2).
    pub fn equals(&self, other: &LogTransform) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.data == other.data
    }

    /// Port of `LogTransformImpl::getBase` (LogTransform.cpp:77-80 @ v2.5.2).
    #[doc(alias = "getBase")]
    pub fn base(&self) -> f64 {
        self.data.base()
    }

    /// Port of `LogTransformImpl::setBase` (LogTransform.cpp:82-85 @ v2.5.2).
    #[doc(alias = "setBase")]
    pub fn set_base(&mut self, val: f64) {
        self.data.set_base(val);
    }

    /// Writes the transform's text to `os`, the base with the stream's precision: 6 on a new
    /// stream, 16 after a MatrixTransform in the same group (docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const LogTransform &)` (LogTransform.cpp:87-95 @
    /// v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<LogTransform");
        os.put_str(" direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", base=");
        os.put_f64(self.base());
        os.put_str(">");
    }
}

impl PartialEq for LogTransform {
    /// [`LogTransform::equals`].
    fn eq(&self, other: &LogTransform) -> bool {
        self.equals(other)
    }
}

impl fmt::Display for LogTransform {
    /// `<LogTransform direction=<dir>, base=<base>>`.
    ///
    /// Port of `operator<<(std::ostream &, const LogTransform &)` (LogTransform.cpp:87-95 @
    /// v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends to `group` the transform of the Log op `op`, holding a copy of its data: a log
/// camera transform for the camera style (its constructor's break replaced by the data's), a
/// log transform for a simple log, a log affine transform otherwise.
///
/// The log affine and log camera transforms' getters and setters read the four affine
/// parameters of each channel, and the camera's its break and linear slope, which upstream
/// reads past the end of a channel too short for them (an op's data that wasn't validated;
/// docs/improvements.md, U-20). The port refuses to make such a transform instead: "Log: the
/// channels have fewer parameters than this needs: upstream accesses past them.", and adds
/// nothing.
///
/// Port of `CreateLogTransform` (src/OpenColorIO/ops/log/LogOp.cpp:158-188 @ v2.5.2).
pub(crate) fn create_log_transform(group: &mut GroupTransform, op: &Op) -> Result<()> {
    let OpData::Log(log_data) = &**op.data() else {
        return Err(Exception::new("CreateLogTransform: op has to be a LogOp."));
    };
    if log_data.is_camera() {
        check_params(log_data, &CAMERA_PARAMS)?;
        // Absent unless set, but a channel can't be shorter than the red one.
        log_data.value(LogAffineParameter::LinearSlope)?;
        let lin_sb = [0.1, 0.1, 0.1];
        let mut log_transform = LogCameraTransform::new(&lin_sb);
        *log_transform.data_mut() = log_data.clone();
        group.append_transform(log_transform.into());
    } else if log_data.is_simple_log() {
        let log_transform = LogTransform {
            data: log_data.clone(),
        };
        group.append_transform(log_transform.into());
    } else {
        check_params(log_data, &CAMERA_PARAMS[..4])?;
        let mut log_transform = LogAffineTransform::new();
        *log_transform.data_mut() = log_data.clone();
        group.append_transform(log_transform.into());
    }
    Ok(())
}

/// The parameters the camera transform holds for each channel: the four affine ones, then the
/// break.
const CAMERA_PARAMS: [LogAffineParameter; 5] = [
    LogAffineParameter::LogSideSlope,
    LogAffineParameter::LogSideOffset,
    LogAffineParameter::LinSideSlope,
    LogAffineParameter::LinSideOffset,
    LogAffineParameter::LinSideBreak,
];

/// Refuses data whose channels don't all hold `params` (U-20).
fn check_params(data: &LogOpData, params: &[LogAffineParameter]) -> Result<()> {
    for &param in params {
        if data.value(param)?.is_none() {
            return Err(Exception::new(SHORT_PARAMS));
        }
    }
    Ok(())
}

/// Validates the data, then appends a Log op of a copy of it (the data's `clone`, which
/// refuses channels of different styles) in the direction `dir`.
///
/// Port of `BuildLogOp(OpRcPtrVec &, const LogTransform &, TransformDirection)`
/// (src/OpenColorIO/ops/log/LogOp.cpp:212-221 @ v2.5.2), and of the other two overloads
/// (190-210), which do the same with the other classes' data.
pub(crate) fn build_log_op(
    ops: &mut OpVec,
    data: &LogOpData,
    dir: TransformDirection,
) -> Result<()> {
    data.validate()?;
    let log = data.try_clone()?;

    create_log_op(ops, log, dir)
}

#[cfg(test)]
#[path = "log_transform_tests.rs"]
mod tests;
