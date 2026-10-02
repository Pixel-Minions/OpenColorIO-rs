// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log op's data: `logSideSlope * log(linSideSlope * color + linSideOffset, base) +
//! logSideOffset`, per channel, with an optional linear segment below a break point (the
//! "camera" style).
//!
//! Port of `LogOpData` (src/OpenColorIO/ops/log/LogOpData.h and LogOpData.cpp @ v2.5.2).

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID};
use crate::math_utils::is_scalar_equal_to_zero;
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{
    TransformDirection, get_inverse_transform_direction, transform_direction_to_string,
};
use crate::ops::matrix::MatrixOpData;
use crate::ops::range::RangeOpData;

/// The parameters of one channel, indexed by [`LogAffineParameter`]. There are 4 (`LOG_SIDE_*`
/// and `LIN_SIDE_*`), 5 (with `LIN_SIDE_BREAK`, the camera style) or 6 (with `LINEAR_SLOPE`).
///
/// Port of `LogOpData::Params` (src/OpenColorIO/ops/log/LogOpData.h:45 @ v2.5.2).
pub type Params = Vec<f64>;

/// The index of each Log parameter in [`Params`].
///
/// Port of `LogAffineParameter` (src/OpenColorIO/ops/log/LogOpData.h:15-24 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogAffineParameter {
    /// `LOG_SIDE_SLOPE`.
    LogSideSlope = 0,
    /// `LOG_SIDE_OFFSET`.
    LogSideOffset,
    /// `LIN_SIDE_SLOPE`.
    LinSideSlope,
    /// `LIN_SIDE_OFFSET`.
    LinSideOffset,
    /// `LIN_SIDE_BREAK`.
    LinSideBreak,
    /// `LINEAR_SLOPE`.
    LinearSlope,
}

/// `LOG_SIDE_SLOPE`, as an index into [`Params`].
pub const LOG_SIDE_SLOPE: usize = LogAffineParameter::LogSideSlope as usize;
/// `LOG_SIDE_OFFSET`, as an index into [`Params`].
pub const LOG_SIDE_OFFSET: usize = LogAffineParameter::LogSideOffset as usize;
/// `LIN_SIDE_SLOPE`, as an index into [`Params`].
pub const LIN_SIDE_SLOPE: usize = LogAffineParameter::LinSideSlope as usize;
/// `LIN_SIDE_OFFSET`, as an index into [`Params`].
pub const LIN_SIDE_OFFSET: usize = LogAffineParameter::LinSideOffset as usize;
/// `LIN_SIDE_BREAK`, as an index into [`Params`].
pub const LIN_SIDE_BREAK: usize = LogAffineParameter::LinSideBreak as usize;
/// `LINEAR_SLOPE`, as an index into [`Params`].
pub const LINEAR_SLOPE: usize = LogAffineParameter::LinearSlope as usize;

/// The default parameters (src/OpenColorIO/ops/log/LogOpData.cpp:20-27 @ v2.5.2).
mod default_values {
    pub(super) const LOG_SLOPE: [f64; 3] = [1.0, 1.0, 1.0];
    pub(super) const LIN_SLOPE: [f64; 3] = [1.0, 1.0, 1.0];
    pub(super) const LIN_OFFSET: [f64; 3] = [0.0, 0.0, 0.0];
    pub(super) const LOG_OFFSET: [f64; 3] = [0.0, 0.0, 0.0];
    /// `FLOAT_DECIMALS`: the cache ID's precision.
    pub(super) const FLOAT_DECIMALS: i64 = 7;
}

/// The error where upstream reads or writes past a channel's parameters (U-20): only a log
/// whose channels have fewer than 4 parameters, or different numbers of them, gets there;
/// validation refuses both.
const SHORT_PARAMS: &str =
    "Log: the channels have fewer parameters than this needs: upstream accesses past them.";

/// Checks one channel's parameters: 4 to 6 of them, and slopes that aren't 0.
///
/// Port of `ValidateParams` (src/OpenColorIO/ops/log/LogOpData.cpp:31-61 @ v2.5.2).
fn validate_params(params: &Params, _direction: TransformDirection) -> Result<()> {
    const MIN_SIZE: usize = 4;
    if params.len() < MIN_SIZE {
        return Err(Exception::new("Log: expecting at least 4 parameters."));
    }
    const MAX_SIZE: usize = 6;
    if params.len() > MAX_SIZE {
        return Err(Exception::new("Log: expecting at most 6 parameters."));
    }

    let message = |before: &str, value: f64, after: &str| {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str(before);
        oss.put_f64(value);
        oss.put_str(after);
        Exception::new(oss.into_string())
    };
    if is_scalar_equal_to_zero(params[LIN_SIDE_SLOPE]) {
        return Err(message(
            "Log: Invalid linear side slope value '",
            params[LIN_SIDE_SLOPE],
            "', linear side slope cannot be 0.",
        ));
    }
    if is_scalar_equal_to_zero(params[LOG_SIDE_SLOPE]) {
        return Err(message(
            "Log: Invalid log side slope value '",
            params[LOG_SIDE_SLOPE],
            "', log side slope cannot be 0.",
        ));
    }
    Ok(())
}

/// The Log op's data.
///
/// Port of `LogOpData` (src/OpenColorIO/ops/log/LogOpData.h:30-155 @ v2.5.2). `Clone` is
/// upstream's `clone()`: the parameters, the direction and the metadata.
#[derive(Debug, Clone)]
pub struct LogOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    red_params: Params,
    green_params: Params,
    blue_params: Params,
    base: f64,
    direction: TransformDirection,
}

impl LogOpData {
    /// A pure logarithm in `base`: default parameters on every channel.
    ///
    /// Port of `LogOpData(double base, TransformDirection)`
    /// (src/OpenColorIO/ops/log/LogOpData.cpp:64-71 @ v2.5.2).
    pub fn new(base: f64, direction: TransformDirection) -> Self {
        let mut data = LogOpData {
            metadata: FormatMetadataImpl::default(),
            red_params: Params::new(),
            green_params: Params::new(),
            blue_params: Params::new(),
            base,
            direction,
        };
        data.set_parameters(
            &default_values::LOG_SLOPE,
            &default_values::LOG_OFFSET,
            &default_values::LIN_SLOPE,
            &default_values::LIN_OFFSET,
        );
        data
    }

    /// The four affine parameters, one value per channel.
    ///
    /// Port of `LogOpData(double base, logSlope, logOffset, linSlope, linOffset, direction)`
    /// (src/OpenColorIO/ops/log/LogOpData.cpp:73-84 @ v2.5.2).
    pub fn with_parameters(
        base: f64,
        log_slope: &[f64; 3],
        log_offset: &[f64; 3],
        lin_slope: &[f64; 3],
        lin_offset: &[f64; 3],
        direction: TransformDirection,
    ) -> Self {
        let mut data = LogOpData::new(base, direction);
        data.set_parameters(log_slope, log_offset, lin_slope, lin_offset);
        data
    }

    /// Per-channel parameter vectors. If any channel has 4 or more parameters, all must.
    ///
    /// Port of `LogOpData(double base, redParams, greenParams, blueParams, dir)`
    /// (src/OpenColorIO/ops/log/LogOpData.cpp:86-108 @ v2.5.2).
    pub fn from_channel_params(
        base: f64,
        red_params: Params,
        green_params: Params,
        blue_params: Params,
        dir: TransformDirection,
    ) -> Result<Self> {
        let sr = red_params.len();
        let sg = green_params.len();
        let sb = blue_params.len();
        if (sr >= 4 || sg >= 4 || sb >= 4) && (sr < 4 || sg < 4 || sb < 4) {
            return Err(Exception::new(
                "Cannot create Log op, all channels need to have the same style.",
            ));
        }
        Ok(LogOpData {
            metadata: FormatMetadataImpl::default(),
            red_params,
            green_params,
            blue_params,
            base,
            direction: dir,
        })
    }

    /// The red channel's parameters.
    pub fn red_params(&self) -> &Params {
        &self.red_params
    }

    /// Replaces the red channel's parameters.
    pub fn set_red_params(&mut self, p: Params) {
        self.red_params = p;
    }

    /// The green channel's parameters.
    pub fn green_params(&self) -> &Params {
        &self.green_params
    }

    /// Replaces the green channel's parameters.
    pub fn set_green_params(&mut self, p: Params) {
        self.green_params = p;
    }

    /// The blue channel's parameters.
    pub fn blue_params(&self) -> &Params {
        &self.blue_params
    }

    /// Replaces the blue channel's parameters.
    pub fn set_blue_params(&mut self, p: Params) {
        self.blue_params = p;
    }

    /// The direction: forward is lin to log, inverse is log to lin.
    pub fn direction(&self) -> TransformDirection {
        self.direction
    }

    /// Sets the direction.
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// Port of `LogOpData::setBase` (src/OpenColorIO/ops/log/LogOpData.cpp:110-113 @ v2.5.2).
    pub fn set_base(&mut self, base: f64) {
        self.base = base;
    }

    /// Port of `LogOpData::getBase` (src/OpenColorIO/ops/log/LogOpData.cpp:115-118 @ v2.5.2).
    pub fn base(&self) -> f64 {
        self.base
    }

    /// Sets one parameter on the three channels. Setting `LIN_SIDE_BREAK` grows the
    /// parameters to 5; setting `LINEAR_SLOPE` needs the break and grows them to 6.
    ///
    /// A channel that is still too short for the parameter is an error (U-20), where upstream
    /// writes past its parameters.
    ///
    /// Port of `LogOpData::setValue` (src/OpenColorIO/ops/log/LogOpData.cpp:120-148 @ v2.5.2).
    pub fn set_value(&mut self, val: LogAffineParameter, values: &[f64; 3]) -> Result<()> {
        if val == LogAffineParameter::LinSideBreak {
            if self.red_params.len() < 5 {
                self.red_params.resize(5, 0.0);
                self.green_params.resize(5, 0.0);
                self.blue_params.resize(5, 0.0);
            }
        } else if val == LogAffineParameter::LinearSlope {
            let cur_size = self.red_params.len();
            if cur_size == 4 {
                return Err(Exception::new(
                    "Log: LinSideBreak has to be defined before linearSlope",
                ));
            } else if cur_size == 5 {
                self.red_params.resize(6, 0.0);
                self.green_params.resize(6, 0.0);
                self.blue_params.resize(6, 0.0);
            }
        }
        let i = val as usize;
        if i >= self.red_params.len() || i >= self.green_params.len() || i >= self.blue_params.len()
        {
            return Err(Exception::new(SHORT_PARAMS));
        }
        self.red_params[i] = values[0];
        self.green_params[i] = values[1];
        self.blue_params[i] = values[2];
        Ok(())
    }

    /// Removes `LINEAR_SLOPE`, if set.
    ///
    /// Port of `LogOpData::unsetLinearSlope`
    /// (src/OpenColorIO/ops/log/LogOpData.cpp:150-158 @ v2.5.2).
    pub fn unset_linear_slope(&mut self) {
        if self.red_params.len() == 6 {
            self.red_params.resize(5, 0.0);
            self.green_params.resize(5, 0.0);
            self.blue_params.resize(5, 0.0);
        }
    }

    /// One parameter of the three channels, or `None` if it is not defined.
    ///
    /// Port of `LogOpData::getValue` (src/OpenColorIO/ops/log/LogOpData.cpp:160-170 @ v2.5.2).
    pub fn value(&self, val: LogAffineParameter) -> Option<[f64; 3]> {
        let i = val as usize;
        if i >= self.red_params.len() {
            return None;
        }
        Some([
            self.red_params[i],
            self.green_params[i],
            self.blue_params[i],
        ])
    }

    /// Resets every channel to the four affine parameters.
    ///
    /// Port of `LogOpData::setParameters`
    /// (src/OpenColorIO/ops/log/LogOpData.cpp:172-185 @ v2.5.2).
    pub fn set_parameters(
        &mut self,
        log_slope: &[f64; 3],
        log_offset: &[f64; 3],
        lin_slope: &[f64; 3],
        lin_offset: &[f64; 3],
    ) {
        self.red_params.resize(4, 0.0);
        self.green_params.resize(4, 0.0);
        self.blue_params.resize(4, 0.0);

        // None of these parameters can fail: they are not LIN_SIDE_BREAK or LINEAR_SLOPE.
        for (param, values) in [
            (LogAffineParameter::LogSideSlope, log_slope),
            (LogAffineParameter::LogSideOffset, log_offset),
            (LogAffineParameter::LinSideSlope, lin_slope),
            (LogAffineParameter::LinSideOffset, lin_offset),
        ] {
            let i = param as usize;
            self.red_params[i] = values[0];
            self.green_params[i] = values[1];
            self.blue_params[i] = values[2];
        }
    }

    /// True when the three channels have the same parameters.
    ///
    /// Port of `LogOpData::allComponentsEqual`
    /// (src/OpenColorIO/ops/log/LogOpData.cpp:381-387 @ v2.5.2).
    pub fn all_components_equal(&self) -> bool {
        // Comparing doubles is generally not a good idea, but in this case it is ok to be
        // strict. Since the same operations are applied to all components, if they started
        // equal, they should remain equal.
        self.red_params == self.green_params && self.red_params == self.blue_params
    }

    /// True for an inverse pair with equal channels.
    ///
    /// Port of `LogOpData::isInverse` (src/OpenColorIO/ops/log/LogOpData.cpp:364-379 @ v2.5.2).
    pub fn is_inverse(&self, log: &LogOpData) -> bool {
        get_inverse_transform_direction(self.direction) == log.direction
            && self.all_components_equal()
            && log.all_components_equal()
            && self.red_params == log.red_params
            && self.base == log.base
    }

    /// True for a plain logarithm: equal channels, 4 parameters, all at their defaults.
    ///
    /// Port of `LogOpData::isSimpleLog` (src/OpenColorIO/ops/log/LogOpData.cpp:454-467
    /// @ v2.5.2).
    pub fn is_simple_log(&self) -> bool {
        if self.all_components_equal() && self.red_params.len() == 4 {
            let p = &self.red_params;
            if p[LOG_SIDE_SLOPE] == 1.0
                && p[LIN_SIDE_SLOPE] == 1.0
                && p[LIN_SIDE_OFFSET] == 0.0
                && p[LOG_SIDE_OFFSET] == 0.0
            {
                return true;
            }
        }
        false
    }

    /// Port of `LogOpData::isLogBase` (src/OpenColorIO/ops/log/LogOpData.cpp:469-476 @ v2.5.2).
    fn is_log_base(&self, base: f64) -> bool {
        self.is_simple_log() && self.base == base
    }

    /// A plain base-2 logarithm.
    ///
    /// Port of `LogOpData::isLog2` (src/OpenColorIO/ops/log/LogOpData.cpp:478-481 @ v2.5.2).
    pub fn is_log2(&self) -> bool {
        self.is_log_base(2.0)
    }

    /// A plain base-10 logarithm.
    ///
    /// Port of `LogOpData::isLog10` (src/OpenColorIO/ops/log/LogOpData.cpp:483-486 @ v2.5.2).
    pub fn is_log10(&self) -> bool {
        self.is_log_base(10.0)
    }

    /// The four affine parameters, one value per channel. A parameter the channels don't have
    /// leaves its array as it is.
    ///
    /// Port of `LogOpData::getParameters` (src/OpenColorIO/ops/log/LogOpData.cpp:187-196 @
    /// v2.5.2).
    pub fn get_parameters(
        &self,
        log_slope: &mut [f64; 3],
        log_offset: &mut [f64; 3],
        lin_slope: &mut [f64; 3],
        lin_offset: &mut [f64; 3],
    ) {
        for (param, values) in [
            (LogAffineParameter::LogSideSlope, log_slope),
            (LogAffineParameter::LogSideOffset, log_offset),
            (LogAffineParameter::LinSideSlope, lin_slope),
            (LogAffineParameter::LinSideOffset, lin_offset),
        ] {
            if let Some(v) = self.value(param) {
                *values = v;
            }
        }
    }

    /// Checks the parameters of each channel, their sizes, and the base: "Log: Invalid base
    /// value '<base>', base cannot be 1." or "... must be greater than 0.".
    ///
    /// Port of `LogOpData::validate` (src/OpenColorIO/ops/log/LogOpData.cpp:202-230 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        validate_params(&self.red_params, self.direction)?;
        validate_params(&self.green_params, self.direction)?;
        validate_params(&self.blue_params, self.direction)?;

        if self.red_params.len() != self.green_params.len()
            || self.red_params.len() != self.blue_params.len()
        {
            return Err(Exception::new(
                "Log: Red, green & blue parameters must have the same size.",
            ));
        }

        let message = |after: &str| {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("Log: Invalid base value '");
            oss.put_f64(self.base);
            oss.put_str(after);
            Exception::new(oss.into_string())
        };
        if self.base == 1.0 {
            return Err(message("', base cannot be 1."));
        } else if self.base <= 0.0 {
            return Err(message("', base must be greater than 0."));
        }
        Ok(())
    }

    /// Port of `LogOpData::getType` (src/OpenColorIO/ops/log/LogOpData.h:72 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Log
    }

    /// Never: a logarithm changes every value.
    ///
    /// Port of `LogOpData::isIdentity` (src/OpenColorIO/ops/log/LogOpData.cpp:232-235 @
    /// v2.5.2).
    pub fn is_identity(&self) -> bool {
        false
    }

    /// The data of an op that replaces a pair of inverse logs, emulating the clamping the pair
    /// does: a forward plain or affine log clamps below the smallest value it accepts (a Range
    /// from 0, or from `-linOffset / linSlope`), an inverse one or a camera log doesn't (an
    /// identity matrix). The range's validation can raise, and so can a forward affine log
    /// whose red channel has fewer than 4 parameters (U-20), where upstream reads past them.
    ///
    /// Port of `LogOpData::getIdentityReplacement` (src/OpenColorIO/ops/log/LogOpData.cpp:
    /// 237-292 @ v2.5.2).
    pub fn get_identity_replacement(&self) -> Result<OpData> {
        let res_op;
        if self.is_log2() || self.is_log10() {
            match self.direction {
                TransformDirection::Forward => {
                    // The first op logarithm is not defined for negative values.
                    res_op = OpData::Range(RangeOpData::with_values(
                        0.,
                        // Don't clamp high end.
                        RangeOpData::empty_value(),
                        0.,
                        RangeOpData::empty_value(),
                    )?);
                }
                TransformDirection::Inverse => {
                    // In principle, the power function is defined over the entire domain.
                    // However, in practice the input to the following logarithm is clamped to
                    // a very small positive number and this imposes a limit. E.g.,
                    // log10(FLOAT_MIN) = -37.93, but this is so small that it makes more sense
                    // to consider it an exact inverse.
                    res_op = OpData::Matrix(MatrixOpData::new());
                }
            }
        } else if !self.is_camera() {
            match self.direction {
                // LinToLog -> LogToLin
                TransformDirection::Forward => {
                    if self.red_params.len() <= LIN_SIDE_OFFSET {
                        return Err(Exception::new(SHORT_PARAMS));
                    }
                    // Minimum value allowed is -linOffset/linSlope so that
                    // linSlope*x+linOffset > 0.
                    let min_value =
                        -self.red_params[LIN_SIDE_OFFSET] / self.red_params[LIN_SIDE_SLOPE];
                    res_op = OpData::Range(RangeOpData::with_values(
                        min_value,
                        // Don't clamp high end.
                        RangeOpData::empty_value(),
                        min_value,
                        RangeOpData::empty_value(),
                    )?);
                }
                // LogToLin -> LinToLog
                TransformDirection::Inverse => {
                    res_op = OpData::Matrix(MatrixOpData::new());
                }
            }
        } else {
            res_op = OpData::Matrix(MatrixOpData::new());
        }
        Ok(res_op)
    }

    /// Never: a logarithm changes every value.
    ///
    /// Port of `LogOpData::isNoOp` (src/OpenColorIO/ops/log/LogOpData.cpp:294-297 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        false
    }

    /// Port of `LogOpData::hasChannelCrosstalk` (src/OpenColorIO/ops/log/LogOpData.h:80 @
    /// v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// The data's cache ID: its id and a space, if it has one, the direction, then each
    /// parameter's string with 7 significant digits: `Base`, `LogSideSlope`, `LogSideOffset`,
    /// `LinSideSlope`, `LinSideOffset`, and `LinSideBreak` and `LinearSlope` when set. An error
    /// for channels with fewer than 4 parameters.
    ///
    /// Port of `LogOpData::getCacheID` (src/OpenColorIO/ops/log/LogOpData.cpp:299-325 @
    /// v2.5.2).
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let precision = default_values::FLOAT_DECIMALS;
        let mut s = String::new();
        s += transform_direction_to_string(self.direction);
        s += " ";
        s += &format!("Base {} ", self.get_base_string(precision));
        s += &format!("LogSideSlope {} ", self.get_log_slope_string(precision)?);
        s += &format!("LogSideOffset {} ", self.get_log_offset_string(precision)?);
        s += &format!("LinSideSlope {} ", self.get_lin_slope_string(precision)?);
        s += &format!("LinSideOffset {}", self.get_lin_offset_string(precision)?);
        if self.red_params.len() > 4 {
            s += &format!(" LinSideBreak {}", self.get_lin_break_string(precision)?);
            if self.red_params.len() > 5 {
                s += &format!(" LinearSlope {}", self.get_linear_slope_string(precision)?);
            }
        }
        cache_id.extend_from_slice(s.as_bytes());
        Ok(cache_id)
    }

    /// Whether `log` has the same direction, base and parameters, compared with `==` (so 0
    /// and -0 are equal and a NaN never is). The metadata is ignored. The `OpData` base's type
    /// comparison is [`OpData::equals`]'s.
    ///
    /// Port of `LogOpData::equals` (src/OpenColorIO/ops/log/LogOpData.cpp:327-338 @ v2.5.2).
    pub fn equals(&self, log: &LogOpData) -> bool {
        // `OpData::equals`: the same object, or the same type.
        if std::ptr::eq(self, log) {
            return true;
        }
        self.direction == log.direction
            && self.base == log.base
            && self.red_params == log.red_params
            && self.green_params == log.green_params
            && self.blue_params == log.blue_params
    }

    /// The same log in the other direction, validated.
    ///
    /// Port of `LogOpData::inverse` (src/OpenColorIO/ops/log/LogOpData.cpp:351-362 @ v2.5.2).
    pub fn inverse(&self) -> Result<LogOpData> {
        let mut inv_op = self.clone();

        inv_op.set_direction(get_inverse_transform_direction(self.direction));
        inv_op.validate()?;

        // Note that any existing metadata could become stale at this point but trying to update
        // it is also challenging since inverse() is sometimes called even during the creation
        // of new ops.
        Ok(inv_op)
    }

    /// One parameter's text with `precision` significant digits: the red value when the
    /// channels are equal, else the three, comma-separated; "Log: accessing parameter that does
    /// not exist." past the red channel's parameters, and the U-20 error past the green or blue
    /// channel's, where upstream reads past them.
    ///
    /// Port of `getParameterString<index>` (src/OpenColorIO/ops/log/LogOpData.cpp:389-414 @
    /// v2.5.2).
    fn get_parameter_string(&self, index: usize, precision: i64) -> Result<String> {
        let mut o = OStringStream::new(Crt::NATIVE);
        o.precision = precision;

        if index < self.red_params.len() {
            if self.all_components_equal() {
                o.put_f64(self.red_params[index]);
            } else {
                if index >= self.green_params.len() || index >= self.blue_params.len() {
                    return Err(Exception::new(SHORT_PARAMS));
                }
                o.put_f64(self.red_params[index]);
                o.put_str(", ");
                o.put_f64(self.green_params[index]);
                o.put_str(", ");
                o.put_f64(self.blue_params[index]);
            }
        } else {
            return Err(Exception::new(
                "Log: accessing parameter that does not exist.",
            ));
        }
        Ok(o.into_string())
    }

    /// Port of `LogOpData::getBaseString` (src/OpenColorIO/ops/log/LogOpData.cpp:416-422 @
    /// v2.5.2).
    pub fn get_base_string(&self, precision: i64) -> String {
        let mut o = OStringStream::new(Crt::NATIVE);
        o.precision = precision;
        o.put_f64(self.base);
        o.into_string()
    }

    /// Port of `LogOpData::getLogSlopeString` (src/OpenColorIO/ops/log/LogOpData.cpp:424-427 @
    /// v2.5.2).
    pub fn get_log_slope_string(&self, precision: i64) -> Result<String> {
        self.get_parameter_string(LOG_SIDE_SLOPE, precision)
    }

    /// Port of `LogOpData::getLinSlopeString` (src/OpenColorIO/ops/log/LogOpData.cpp:429-432 @
    /// v2.5.2).
    pub fn get_lin_slope_string(&self, precision: i64) -> Result<String> {
        self.get_parameter_string(LIN_SIDE_SLOPE, precision)
    }

    /// Port of `LogOpData::getLinOffsetString` (src/OpenColorIO/ops/log/LogOpData.cpp:434-437
    /// @ v2.5.2).
    pub fn get_lin_offset_string(&self, precision: i64) -> Result<String> {
        self.get_parameter_string(LIN_SIDE_OFFSET, precision)
    }

    /// Port of `LogOpData::getLogOffsetString` (src/OpenColorIO/ops/log/LogOpData.cpp:439-442
    /// @ v2.5.2).
    pub fn get_log_offset_string(&self, precision: i64) -> Result<String> {
        self.get_parameter_string(LOG_SIDE_OFFSET, precision)
    }

    /// Port of `LogOpData::getLinBreakString` (src/OpenColorIO/ops/log/LogOpData.cpp:444-447 @
    /// v2.5.2).
    pub fn get_lin_break_string(&self, precision: i64) -> Result<String> {
        self.get_parameter_string(LIN_SIDE_BREAK, precision)
    }

    /// Port of `LogOpData::getLinearSlopeString` (src/OpenColorIO/ops/log/LogOpData.cpp:449-452
    /// @ v2.5.2).
    pub fn get_linear_slope_string(&self, precision: i64) -> Result<String> {
        self.get_parameter_string(LINEAR_SLOPE, precision)
    }

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// Port of `OpData::getID` (src/OpenColorIO/Op.cpp:81-84 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        self.metadata.get_attribute_value_string(Some(METADATA_ID))
    }

    /// The camera style: a linear segment below `LIN_SIDE_BREAK`.
    ///
    /// Port of `LogOpData::isCamera` (src/OpenColorIO/ops/log/LogOpData.cpp:488-491 @ v2.5.2).
    pub fn is_camera(&self) -> bool {
        self.red_params.len() > 4
    }
}

/// Port of `operator==(const LogOpData &, const LogOpData &)` (src/OpenColorIO/ops/log/
/// LogOpData.cpp:493-496 @ v2.5.2).
impl PartialEq for LogOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "log_op_data_tests.rs"]
mod tests;
