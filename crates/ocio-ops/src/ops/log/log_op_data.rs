// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log op's data: `logSideSlope * log(linSideSlope * color + linSideOffset, base) +
//! logSideOffset`, per channel, with an optional linear segment below a break point (the
//! "camera" style).
//!
//! Port of `LogOpData` (src/OpenColorIO/ops/log/LogOpData.h and LogOpData.cpp @ v2.5.2): the
//! parameters, their accessors and the style predicates the CPU renderers need. Not yet
//! ported (WP 1.3l1): `validate` and the other methods whose results or error messages print
//! doubles with C++ stream formatting (`getCacheID`, the `get*String` accessors, `inverse`'s
//! validation), and `getIdentityReplacement`, which builds Range and Matrix op data.

use crate::exception::{Exception, Result};
use crate::open_color_types::{TransformDirection, get_inverse_transform_direction};

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
}

/// The Log op's data.
///
/// Port of `LogOpData` (src/OpenColorIO/ops/log/LogOpData.h:30-155 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct LogOpData {
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

    /// The camera style: a linear segment below `LIN_SIDE_BREAK`.
    ///
    /// Port of `LogOpData::isCamera` (src/OpenColorIO/ops/log/LogOpData.cpp:488-491 @ v2.5.2).
    pub fn is_camera(&self) -> bool {
        self.red_params.len() > 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TransformDirection::{Forward, Inverse};

    #[test]
    fn styles_follow_the_parameters() {
        let log2 = LogOpData::new(2.0, Forward);
        assert!(log2.is_simple_log() && log2.is_log2() && !log2.is_log10());
        assert!(!log2.is_camera());
        assert!(LogOpData::new(10.0, Inverse).is_log10());

        // Any non-default parameter makes it an affine log.
        let mut affine = LogOpData::new(2.0, Forward);
        let mut slope = affine.value(LogAffineParameter::LogSideSlope).unwrap();
        slope[1] = 0.5;
        affine
            .set_value(LogAffineParameter::LogSideSlope, &slope)
            .unwrap();
        assert!(!affine.is_simple_log() && !affine.is_log2());
        assert!(!affine.all_components_equal());

        // The break adds a fifth parameter, the linear slope a sixth.
        let mut camera = LogOpData::new(2.0, Forward);
        let brk = camera.value(LogAffineParameter::LinSideSlope).unwrap();
        assert!(
            camera
                .set_value(LogAffineParameter::LinearSlope, &brk)
                .is_err()
        );
        camera
            .set_value(LogAffineParameter::LinSideBreak, &brk)
            .unwrap();
        assert!(camera.is_camera() && !camera.is_log2());
        assert_eq!(camera.red_params().len(), 5);
        camera
            .set_value(LogAffineParameter::LinearSlope, &brk)
            .unwrap();
        assert_eq!(camera.red_params().len(), 6);
        camera.unset_linear_slope();
        assert_eq!(camera.red_params().len(), 5);
        assert_eq!(camera.value(LogAffineParameter::LinearSlope), None);
    }

    #[test]
    fn channels_need_the_same_style() {
        let four = LogOpData::new(2.0, Forward).red_params().clone();
        let three = four[..3].to_vec();
        let err = LogOpData::from_channel_params(2.0, four.clone(), three, four.clone(), Forward)
            .unwrap_err();
        assert_eq!(
            err.message(),
            "Cannot create Log op, all channels need to have the same style."
        );
        let a = LogOpData::from_channel_params(2.0, four.clone(), four.clone(), four, Forward);
        let b = LogOpData::new(2.0, Inverse);
        assert!(a.unwrap().is_inverse(&b));
    }
}
