// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL op's data: the ASC CDL's slope, offset and power per RGB channel, and a saturation,
//! with a style for the direction and for clamping. A port of
//! `src/OpenColorIO/ops/cdl/CDLOpData.h` and `CDLOpData.cpp` @ v2.5.2.
//!
//! The parameters compare with a tolerance of 1e-9 ([`ChannelParams`]'s `==`), the saturation
//! exactly. The ASC v1.2 specification bounds them: slope >= 0, power > 0, saturation >= 0;
//! the offset is unbounded.

use std::cmp::Ordering;

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::math_utils::equal_with_abs_error;
use crate::op_data::{OpData, OpDataRcPtr, OpDataType, OpDataVec};
use crate::open_color_types::{CdlStyle, TransformDirection};
use crate::ops::matrix::MatrixOpData;
use crate::ops::matrix::matrix_op::matrix_transform_sat;
use crate::ops::range::RangeOpData;
use crate::platform::strcasecmp;

/// `DefaultValues::FLOAT_DECIMALS` (CDLOpData.cpp:18-21 @ v2.5.2): the precision of the
/// parameter strings and the cache ID.
const FLOAT_DECIMALS: i64 = 7;

/// The CDL styles.
///
/// Port of `CDLOpData::Style` (src/OpenColorIO/ops/cdl/CDLOpData.h:25-31 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CdlOpStyle {
    /// `CDL_V1_2_FWD`: forward (version 1.2) style, which clamps.
    V1_2Fwd = 0,
    /// `CDL_V1_2_REV`: reverse (version 1.2) style, which clamps.
    V1_2Rev,
    /// `CDL_NO_CLAMP_FWD`: forward no clamping style.
    NoClampFwd,
    /// `CDL_NO_CLAMP_REV`: reverse no clamping style.
    NoClampRev,
}

/// The values of a SOP parameter (slope, offset or power) for the three channels.
///
/// `==` compares each value with an absolute tolerance of 1e-9 (`EqualWithAbsError`), so it
/// is false for a NaN.
///
/// Port of `CDLOpData::ChannelParams` (src/OpenColorIO/ops/cdl/CDLOpData.h:43-104 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelParams {
    /// `m_data`.
    data: [f64; 3],
}

impl ChannelParams {
    /// Port of `ChannelParams(double r, double g, double b)` (CDLOpData.h:45-48 @ v2.5.2).
    pub fn new(r: f64, g: f64, b: f64) -> Self {
        ChannelParams { data: [r, g, b] }
    }

    /// The same value for the three channels.
    ///
    /// Port of `ChannelParams(double x)` (CDLOpData.h:50-53 @ v2.5.2).
    pub fn splat(x: f64) -> Self {
        ChannelParams { data: [x, x, x] }
    }

    /// Port of `ChannelParams::setRGB` (CDLOpData.h:60-65 @ v2.5.2).
    pub fn set_rgb(&mut self, r: f64, g: f64, b: f64) {
        self.data = [r, g, b];
    }

    /// Port of `ChannelParams::getRGB` (CDLOpData.h:67-72 @ v2.5.2).
    pub fn get_rgb(&self) -> [f64; 3] {
        self.data
    }

    /// The value of a channel; "Index is out of range" from 3 on.
    ///
    /// Port of `ChannelParams::operator[]` (CDLOpData.h:74-90 @ v2.5.2).
    pub fn get(&self, index: usize) -> Result<f64> {
        if index >= 3 {
            return Err(Exception::new("Index is out of range"));
        }
        Ok(self.data[index])
    }
}

impl PartialEq for ChannelParams {
    /// Port of `ChannelParams::operator==` (CDLOpData.h:92-98 @ v2.5.2).
    fn eq(&self, other: &Self) -> bool {
        equal_with_abs_error(self.data[0], other.data[0], 1e-9)
            && equal_with_abs_error(self.data[1], other.data[1], 1e-9)
            && equal_with_abs_error(self.data[2], other.data[2], 1e-9)
    }
}

/// `kOneParams` (CDLOpData.cpp:24 @ v2.5.2).
const K_ONE_PARAMS: ChannelParams = ChannelParams {
    data: [1.0, 1.0, 1.0],
};
/// `kZeroParams` (CDLOpData.cpp:25 @ v2.5.2).
const K_ZERO_PARAMS: ChannelParams = ChannelParams {
    data: [0.0, 0.0, 0.0],
};

// Original CTF styles (CDLOpData.cpp:27-31):
const V1_2_FWD_NAME: &str = "v1.2_Fwd";
const V1_2_REV_NAME: &str = "v1.2_Rev";
const NO_CLAMP_FWD_NAME: &str = "noClampFwd";
const NO_CLAMP_REV_NAME: &str = "noClampRev";

// CLF styles (also allowed now in CTF) (CDLOpData.cpp:33-37):
const V1_2_FWD_CLF_NAME: &str = "Fwd";
const V1_2_REV_CLF_NAME: &str = "Rev";
const NO_CLAMP_FWD_CLF_NAME: &str = "FwdNoClamp";
const NO_CLAMP_REV_CLF_NAME: &str = "RevNoClamp";

/// Validates that a parameter is greater than or equal to `threshold` (a NaN isn't).
///
/// Port of `validateGreaterEqual` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:221-236 @ v2.5.2).
fn validate_greater_equal(name: &str, value: f64, threshold: f64) -> Result<()> {
    // `!(value >= threshold)`: true for a NaN.
    if !matches!(
        value.partial_cmp(&threshold),
        Some(Ordering::Greater | Ordering::Equal)
    ) {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str("CDL: Invalid '");
        oss.put_str(name);
        oss.put_str("' ");
        oss.put_f64(value);
        oss.put_str(" should be greater than ");
        oss.put_f64(threshold);
        oss.put_str(".");
        return Err(Exception::new(oss.into_string()));
    }
    Ok(())
}

/// Validates that a parameter is greater than `threshold` (a NaN isn't).
///
/// Port of `validateGreaterThan` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:238-253 @ v2.5.2).
fn validate_greater_than(name: &str, value: f64, threshold: f64) -> Result<()> {
    // `!(value > threshold)`: true for a NaN.
    if value.partial_cmp(&threshold) != Some(Ordering::Greater) {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str("CDLOpData: Invalid '");
        oss.put_str(name);
        oss.put_str("' ");
        oss.put_f64(value);
        oss.put_str(" should be greater than ");
        oss.put_f64(threshold);
        oss.put_str(".");
        return Err(Exception::new(oss.into_string()));
    }
    Ok(())
}

/// Validates the three channels of `params` with `fn_val`, red first.
///
/// Port of `validateChannelParams` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:255-268 @ v2.5.2).
fn validate_channel_params(
    fn_val: fn(&str, f64, f64) -> Result<()>,
    name: &str,
    params: &ChannelParams,
    threshold: f64,
) -> Result<()> {
    for value in params.data {
        fn_val(name, value, threshold)?;
    }
    Ok(())
}

/// The ASC v1.2 spec 2009-05-04 places the following restrictions: slope >= 0, power > 0,
/// sat >= 0, (offset unbounded).
///
/// Port of `validateParams` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:270-285 @ v2.5.2).
fn validate_params(
    slope_params: &ChannelParams,
    power_params: &ChannelParams,
    saturation: f64,
) -> Result<()> {
    // slope >= 0
    validate_channel_params(validate_greater_equal, "slope", slope_params, 0.0)?;

    // power > 0
    validate_channel_params(validate_greater_than, "power", power_params, 0.0)?;

    // saturation >= 0
    validate_greater_equal("saturation", saturation, 0.0)
}

/// `params` as `r, g, b` with 7 significant digits.
///
/// Port of `CDLOpData::GetChannelParametersString` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:
/// 455-461 @ v2.5.2).
fn get_channel_parameters_string(params: &ChannelParams) -> String {
    let mut oss = OStringStream::new(Crt::NATIVE);
    oss.precision = FLOAT_DECIMALS;
    oss.put_f64(params.data[0]);
    oss.put_str(", ");
    oss.put_f64(params.data[1]);
    oss.put_str(", ");
    oss.put_f64(params.data[2]);
    oss.into_string()
}

/// The CDL op's data.
///
/// `==` is upstream's `operator==` ([`equals`](Self::equals)).
///
/// Port of `CDLOpData` (src/OpenColorIO/ops/cdl/CDLOpData.h:21-187 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CdlOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: CdlOpStyle,
    /// `m_slopeParams`.
    slope_params: ChannelParams,
    /// `m_offsetParams`.
    offset_params: ChannelParams,
    /// `m_powerParams`.
    power_params: ChannelParams,
    /// `m_saturation`.
    saturation: f64,
}

impl Default for CdlOpData {
    fn default() -> Self {
        Self::new_default()
    }
}

impl CdlOpData {
    /// The style of `CDL_TRANSFORM_DEFAULT`, forward: `CDL_NO_CLAMP_FWD`.
    ///
    /// Port of `CDLOpData::GetDefaultStyle` (src/OpenColorIO/ops/cdl/CDLOpData.h:33-34 @
    /// v2.5.2).
    pub fn get_default_style() -> CdlOpStyle {
        Self::convert_style(CdlStyle::TRANSFORM_DEFAULT, TransformDirection::Forward)
    }

    /// The style a name gives, ignoring ASCII case (`Platform::Strcasecmp`, deviation D-4):
    /// the CTF names (`v1.2_Fwd`, ...) and the CLF ones (`Fwd`, ...). `None` is a null pointer.
    /// The name ends at its first NUL, as upstream's C string does.
    ///
    /// Port of `CDLOpData::GetStyle` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:39-65 @ v2.5.2).
    pub fn get_style_from_name(name: Option<&str>) -> Result<CdlOpStyle> {
        if let Some(name) = name.map(|s| s.split('\0').next().unwrap_or(""))
            && !name.is_empty()
        {
            for (style_name, style) in [
                (V1_2_FWD_NAME, CdlOpStyle::V1_2Fwd),
                (V1_2_FWD_CLF_NAME, CdlOpStyle::V1_2Fwd),
                (V1_2_REV_NAME, CdlOpStyle::V1_2Rev),
                (V1_2_REV_CLF_NAME, CdlOpStyle::V1_2Rev),
                (NO_CLAMP_FWD_NAME, CdlOpStyle::NoClampFwd),
                (NO_CLAMP_FWD_CLF_NAME, CdlOpStyle::NoClampFwd),
                (NO_CLAMP_REV_NAME, CdlOpStyle::NoClampRev),
                (NO_CLAMP_REV_CLF_NAME, CdlOpStyle::NoClampRev),
            ] {
                if strcasecmp(name, style_name).is_eq() {
                    return Ok(style);
                }
            }
        }

        Err(Exception::new("Unknown style for CDL."))
    }

    /// The style's CLF name. (Upstream's error for a value outside the enum can't happen.)
    ///
    /// Port of `CDLOpData::GetStyleName` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:67-80 @
    /// v2.5.2).
    pub fn get_style_name(style: CdlOpStyle) -> &'static str {
        match style {
            CdlOpStyle::V1_2Fwd => V1_2_FWD_CLF_NAME,
            CdlOpStyle::V1_2Rev => V1_2_REV_CLF_NAME,
            CdlOpStyle::NoClampFwd => NO_CLAMP_FWD_CLF_NAME,
            CdlOpStyle::NoClampRev => NO_CLAMP_REV_CLF_NAME,
        }
    }

    /// Combines the transform's style and direction into the op data's style.
    ///
    /// Port of `CDLOpData::ConvertStyle(CDLStyle, TransformDirection)`
    /// (src/OpenColorIO/ops/cdl/CDLOpData.cpp:83-106 @ v2.5.2).
    pub fn convert_style(style: CdlStyle, dir: TransformDirection) -> CdlOpStyle {
        let is_forward = dir == TransformDirection::Forward;
        match style {
            CdlStyle::Asc => {
                if is_forward {
                    CdlOpStyle::V1_2Fwd
                } else {
                    CdlOpStyle::V1_2Rev
                }
            }
            CdlStyle::NoClamp => {
                if is_forward {
                    CdlOpStyle::NoClampFwd
                } else {
                    CdlOpStyle::NoClampRev
                }
            }
        }
    }

    /// The transform's style of an op data's style.
    ///
    /// Port of `CDLOpData::ConvertStyle(CDLOpData::Style)` (src/OpenColorIO/ops/cdl/
    /// CDLOpData.cpp:108-125 @ v2.5.2).
    pub fn convert_style_to_transform(style: CdlOpStyle) -> CdlStyle {
        match style {
            CdlOpStyle::V1_2Fwd | CdlOpStyle::V1_2Rev => CdlStyle::Asc,
            CdlOpStyle::NoClampFwd | CdlOpStyle::NoClampRev => CdlStyle::NoClamp,
        }
    }

    /// The default style, slope 1, offset 0, power 1 and saturation 1: an identity.
    ///
    /// Port of `CDLOpData::CDLOpData()` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:127-135 @
    /// v2.5.2).
    pub fn new_default() -> Self {
        CdlOpData {
            metadata: FormatMetadataImpl::default(),
            style: Self::get_default_style(),
            slope_params: ChannelParams::splat(1.0),
            offset_params: ChannelParams::splat(0.0),
            power_params: ChannelParams::splat(1.0),
            saturation: 1.0,
        }
    }

    /// The data with these parameters, validated.
    ///
    /// Port of `CDLOpData::CDLOpData(style, slope, offset, power, saturation)`
    /// (src/OpenColorIO/ops/cdl/CDLOpData.cpp:137-150 @ v2.5.2).
    pub fn new(
        style: CdlOpStyle,
        slope_params: ChannelParams,
        offset_params: ChannelParams,
        power_params: ChannelParams,
        saturation: f64,
    ) -> Result<Self> {
        let cdl = CdlOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            slope_params,
            offset_params,
            power_params,
            saturation,
        };
        cdl.validate()?;
        Ok(cdl)
    }

    /// Whether `other` has the same style, the same slope, offset and power within 1e-9, and
    /// the same saturation. The metadata is ignored.
    ///
    /// Port of `CDLOpData::equals` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:161-172 @ v2.5.2),
    /// after `OpData::equals`, which compares the types.
    pub fn equals(&self, other: &CdlOpData) -> bool {
        self.style == other.style
            && self.slope_params == other.slope_params
            && self.offset_params == other.offset_params
            && self.power_params == other.power_params
            && self.saturation == other.saturation
    }

    /// Port of `CDLOpData::getType` (src/OpenColorIO/ops/cdl/CDLOpData.h:127 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::Cdl
    }

    /// Whether the style is a reverse one.
    ///
    /// Port of `CDLOpData::isReverse` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:429-440 @
    /// v2.5.2).
    pub fn is_reverse(&self) -> bool {
        match self.style {
            CdlOpStyle::V1_2Fwd | CdlOpStyle::NoClampFwd => false,
            CdlOpStyle::V1_2Rev | CdlOpStyle::NoClampRev => true,
        }
    }

    /// Port of `CDLOpData::getStyle` (src/OpenColorIO/ops/cdl/CDLOpData.h:132 @ v2.5.2).
    pub fn get_style(&self) -> CdlOpStyle {
        self.style
    }

    /// Port of `CDLOpData::setStyle` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:174-177 @ v2.5.2).
    pub fn set_style(&mut self, cdl_style: CdlOpStyle) {
        self.style = cdl_style;
    }

    /// The direction the style encodes.
    ///
    /// Port of `CDLOpData::getDirection` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:179-191 @
    /// v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        match self.style {
            CdlOpStyle::V1_2Fwd | CdlOpStyle::NoClampFwd => TransformDirection::Forward,
            CdlOpStyle::V1_2Rev | CdlOpStyle::NoClampRev => TransformDirection::Inverse,
        }
    }

    /// Inverts the style when the direction differs.
    ///
    /// Port of `CDLOpData::setDirection` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:193-199 @
    /// v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        if self.get_direction() != dir {
            self.invert();
        }
    }

    /// Port of `CDLOpData::getSlopeParams` (src/OpenColorIO/ops/cdl/CDLOpData.h:137 @ v2.5.2).
    pub fn get_slope_params(&self) -> &ChannelParams {
        &self.slope_params
    }

    /// Port of `CDLOpData::setSlopeParams` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:201-204 @
    /// v2.5.2).
    pub fn set_slope_params(&mut self, slope_params: ChannelParams) {
        self.slope_params = slope_params;
    }

    /// Port of `CDLOpData::getOffsetParams` (src/OpenColorIO/ops/cdl/CDLOpData.h:140 @ v2.5.2).
    pub fn get_offset_params(&self) -> &ChannelParams {
        &self.offset_params
    }

    /// Port of `CDLOpData::setOffsetParams` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:206-209 @
    /// v2.5.2).
    pub fn set_offset_params(&mut self, offset_params: ChannelParams) {
        self.offset_params = offset_params;
    }

    /// Port of `CDLOpData::getPowerParams` (src/OpenColorIO/ops/cdl/CDLOpData.h:143 @ v2.5.2).
    pub fn get_power_params(&self) -> &ChannelParams {
        &self.power_params
    }

    /// Port of `CDLOpData::setPowerParams` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:211-214 @
    /// v2.5.2).
    pub fn set_power_params(&mut self, power_params: ChannelParams) {
        self.power_params = power_params;
    }

    /// Port of `CDLOpData::getSaturation` (src/OpenColorIO/ops/cdl/CDLOpData.h:146 @ v2.5.2).
    pub fn get_saturation(&self) -> f64 {
        self.saturation
    }

    /// Port of `CDLOpData::setSaturation` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:216-219 @
    /// v2.5.2).
    pub fn set_saturation(&mut self, saturation: f64) {
        self.saturation = saturation;
    }

    /// An identity that doesn't clamp.
    ///
    /// Port of `CDLOpData::isNoOp` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:287-291 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity() && !self.is_clamping()
    }

    /// Slope 1, offset 0 and power 1 within 1e-9, and saturation exactly 1.
    ///
    /// Port of `CDLOpData::isIdentity` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:293-299 @
    /// v2.5.2).
    pub fn is_identity(&self) -> bool {
        self.slope_params == K_ONE_PARAMS
            && self.offset_params == K_ZERO_PARAMS
            && self.power_params == K_ONE_PARAMS
            && self.saturation == 1.0
    }

    /// The data of the op that replaces this one where the optimizer finds it to be an
    /// identity: a Range clamping to [0, 1] for the clamping styles, the identity matrix
    /// otherwise.
    ///
    /// Port of `CDLOpData::getIdentityReplacement` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:
    /// 301-323 @ v2.5.2).
    pub fn get_identity_replacement(&self) -> OpData {
        match self.style {
            // These clamp values -- replace with range.
            CdlOpStyle::V1_2Fwd | CdlOpStyle::V1_2Rev => OpData::Range(clamp_range()),

            // These pass through the full range of values -- replace with matrix.
            CdlOpStyle::NoClampFwd | CdlOpStyle::NoClampRev => OpData::Matrix(MatrixOpData::new()),
        }
    }

    /// Appends to `tmpops` the simpler ops that do what this one does when its power is 1 and
    /// it isn't an identity: a slope and offset matrix, then for a saturation other than 1 a
    /// clamp (the clamping styles) and the saturation matrix (Rec.709 luma), then a clamp (the
    /// clamping styles); reversed for an inverse direction. Nothing otherwise.
    ///
    /// Port of `CDLOpData::getSimplerReplacement` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:
    /// 325-394 @ v2.5.2).
    pub fn get_simpler_replacement(&self, tmpops: &mut OpDataVec) {
        // If identity, let the identityReplacement mechanism handle the situation.
        if self.power_params != K_ONE_PARAMS || self.is_identity() {
            return;
        }

        // Power is identity, we can replace CDL by simpler ops that the optimizer might be
        // able to combine with other ones.

        // Slope + offset.
        let scale4 = self.slope_params.get_rgb();

        let mut m44 = [0.0f64; 16];
        m44[0] = scale4[0];
        m44[5] = scale4[1];
        m44[10] = scale4[2];
        m44[15] = 1.0;

        let offset_rgb = self.offset_params.get_rgb();
        let offset4 = [offset_rgb[0], offset_rgb[1], offset_rgb[2], 0.];

        let mut mat_so = MatrixOpData::new();
        mat_so.set_rgba(&m44);
        mat_so.set_rgba_offsets(&offset4);

        mat_so.set_direction(self.get_direction());

        tmpops.push(OpDataRcPtr::new(OpData::Matrix(mat_so)));

        // Saturation.
        if self.saturation != 1. {
            if self.is_clamping() {
                // Same in both directions.
                tmpops.push(OpDataRcPtr::new(OpData::Range(clamp_range())));
            }

            const LUMA_COEF3: [f64; 3] = [0.2126, 0.7152, 0.0722];

            let (matrix, offset_sat) = matrix_transform_sat(self.saturation, &LUMA_COEF3);

            let mut mat = MatrixOpData::new();
            mat.set_rgba(&matrix);
            mat.set_rgba_offsets(&offset_sat);

            mat.set_direction(self.get_direction());

            tmpops.push(OpDataRcPtr::new(OpData::Matrix(mat)));
        }

        // Clamping
        if self.is_clamping() {
            tmpops.push(OpDataRcPtr::new(OpData::Range(clamp_range())));
        }

        if self.get_direction() == TransformDirection::Inverse {
            tmpops.reverse();
        }
    }

    /// Whether the saturation mixes the channels: when it isn't 1.
    ///
    /// Port of `CDLOpData::hasChannelCrosstalk` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:
    /// 396-399 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        self.saturation != 1.0
    }

    /// Checks the slope (each >= 0), the power (each > 0) and the saturation (>= 0), with the
    /// value printed by a default stream (6 significant digits). A NaN fails.
    ///
    /// Port of `CDLOpData::validate` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:401-404 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        validate_params(&self.slope_params, &self.power_params, self.saturation)
    }

    /// Port of `CDLOpData::getSlopeString` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:406-409 @
    /// v2.5.2).
    pub fn get_slope_string(&self) -> String {
        get_channel_parameters_string(&self.slope_params)
    }

    /// Port of `CDLOpData::getOffsetString` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:411-414 @
    /// v2.5.2).
    pub fn get_offset_string(&self) -> String {
        get_channel_parameters_string(&self.offset_params)
    }

    /// Port of `CDLOpData::getPowerString` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:416-419 @
    /// v2.5.2).
    pub fn get_power_string(&self) -> String {
        get_channel_parameters_string(&self.power_params)
    }

    /// Port of `CDLOpData::getSaturationString` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:421-427
    /// @ v2.5.2).
    pub fn get_saturation_string(&self) -> String {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.precision = FLOAT_DECIMALS;
        oss.put_f64(self.saturation);
        oss.into_string()
    }

    /// The `V1_2` styles clamp.
    ///
    /// Port of `CDLOpData::isClamping` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:442-453 @
    /// v2.5.2).
    pub fn is_clamping(&self) -> bool {
        match self.style {
            CdlOpStyle::V1_2Fwd | CdlOpStyle::V1_2Rev => true,
            CdlOpStyle::NoClampFwd | CdlOpStyle::NoClampRev => false,
        }
    }

    /// Whether `r` equals this one's inverse ([`equals`](Self::equals), with its tolerance).
    ///
    /// Port of `CDLOpData::isInverse` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:463-468 @
    /// v2.5.2).
    pub fn is_inverse(&self, r: &CdlOpData) -> bool {
        // TODO: We are not detecting the case where you have two transforms with the same
        // direction but the parameters are inverses, e.g. offset of 0.1 and then -0.1
        r.equals(&self.inverse())
    }

    /// Swaps the style's direction.
    ///
    /// Port of `CDLOpData::invert` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:470-479 @ v2.5.2).
    fn invert(&mut self) {
        let style = match self.style {
            CdlOpStyle::V1_2Fwd => CdlOpStyle::V1_2Rev,
            CdlOpStyle::V1_2Rev => CdlOpStyle::V1_2Fwd,
            CdlOpStyle::NoClampFwd => CdlOpStyle::NoClampRev,
            CdlOpStyle::NoClampRev => CdlOpStyle::NoClampFwd,
        };
        self.set_style(style);
    }

    /// A copy with the direction swapped; the metadata is copied as it is.
    ///
    /// Port of `CDLOpData::inverse` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:481-490 @ v2.5.2).
    pub fn inverse(&self) -> CdlOpData {
        let mut cdl = self.clone();
        cdl.invert();

        // Note that any existing metadata could become stale at this point but trying to
        // update it is also challenging since inverse() is sometimes called even during the
        // creation of new ops.
        cdl
    }

    /// The ID (followed by a space) if there is one, the style's CLF name, the slope, offset
    /// and power (`r, g, b` each) and the saturation, with 7 significant digits, each followed
    /// by a space.
    ///
    /// Port of `CDLOpData::getCacheID` (src/OpenColorIO/ops/cdl/CDLOpData.cpp:492-511 @
    /// v2.5.2).
    pub fn get_cache_id(&self) -> Vec<u8> {
        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let text = format!(
            "{} {} {} {} {} ",
            Self::get_style_name(self.style),
            self.get_slope_string(),
            self.get_offset_string(),
            self.get_power_string(),
            self.get_saturation_string()
        );
        cache_id.extend_from_slice(text.as_bytes());
        cache_id
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

    /// Port of `OpData::setID` (src/OpenColorIO/Op.cpp:86-89 @ v2.5.2).
    pub fn set_id(&mut self, id: &[u8]) {
        self.metadata.set_id(Some(id));
    }

    /// Port of `OpData::getName` (src/OpenColorIO/Op.cpp:91-94 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        self.metadata
            .get_attribute_value_string(Some(METADATA_NAME))
    }
}

/// `std::make_shared<RangeOpData>(0., 1., 0., 1.)`, which validates.
fn clamp_range() -> RangeOpData {
    RangeOpData::with_values(0., 1., 0., 1.).expect("a [0, 1] range is valid")
}

/// Port of `operator==(const CDLOpData &, const CDLOpData &)`
/// (src/OpenColorIO/ops/cdl/CDLOpData.cpp:513-516 @ v2.5.2).
impl PartialEq for CdlOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "cdl_op_data_tests.rs"]
mod tests;
