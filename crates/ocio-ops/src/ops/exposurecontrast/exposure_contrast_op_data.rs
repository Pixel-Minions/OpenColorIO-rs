// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast op's data. A port of
//! `src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpData.h` and
//! `ExposureContrastOpData.cpp` @ v2.5.2.
//!
//! The op enables interactive exposure and contrast/gamma adjustments in viewports. Exposure,
//! contrast and gamma are dynamic properties (doubles), so an application can change them
//! after the processor is built. The linear, video and log styles adjust the math so that the
//! controls behave the same in each kind of space (ExposureContrastOpData.h:26-120):
//!
//! - linear: `OUT = pivot * (IN * 2^exposure / pivot)^contrast`;
//! - video: `OUT = pivot^G * (IN * (2^exposure / pivot)^G)^contrast`, with `G` =
//!   [`ec::VIDEO_OETF_POWER`];
//! - log: `logPivot = log2(pivot / 0.18) * logExposureStep + logMidGray`, and
//!   `OUT = (IN + exposure * logExposureStep - logPivot) * contrast + logPivot`.

use std::sync::Arc;

use crate::cfmt::{Crt, OStringStream};
use crate::dynamic_property::{
    DynamicPropertyDoubleImpl, DynamicPropertyDoubleImplRcPtr, DynamicPropertyRcPtr,
};
use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::op_data::OpDataType;
use crate::open_color_types::{DynamicPropertyType, ExposureContrastStyle, TransformDirection};
use crate::platform::strcasecmp;

/// The op's limits and the video style's power.
///
/// Port of namespace `EC` (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpData.h:15-20
/// @ v2.5.2).
pub mod ec {
    /// `EC::MIN_PIVOT`.
    pub const MIN_PIVOT: f64 = 0.001;
    /// `EC::MIN_CONTRAST`.
    pub const MIN_CONTRAST: f64 = 0.001;
    /// `EC::VIDEO_OETF_POWER`: 1 / 1.83, an approximation of the ITU-R BT.709 OETF. Upstream's
    /// literal, kept as written.
    #[allow(clippy::excessive_precision)]
    pub const VIDEO_OETF_POWER: f64 = 0.54644808743169393;
}

/// The CTF style attribute strings (ExposureContrastOpData.cpp:16-23 @ v2.5.2).
const EC_STYLE_LINEAR: &str = "linear";
const EC_STYLE_LINEAR_REV: &str = "linearRev";
const EC_STYLE_VIDEO: &str = "video";
const EC_STYLE_VIDEO_REV: &str = "videoRev";
const EC_STYLE_LOGARITHMIC: &str = "log";
const EC_STYLE_LOGARITHMIC_REV: &str = "logRev";

/// `DefaultValues::FLOAT_DECIMALS` (ExposureContrastOpData.cpp:25-28 @ v2.5.2): the cache ID's
/// precision.
const FLOAT_DECIMALS: i64 = 7;

/// "Dynamic property type not supported by ExposureContrast." (ExposureContrastOpData.cpp:351,
/// 391, 395 @ v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by ExposureContrast.";

/// "ExposureContrast property is not dynamic." (ExposureContrastOpData.cpp:353, 393 @ v2.5.2).
const NOT_DYNAMIC: &str = "ExposureContrast property is not dynamic.";

/// The algorithm and its direction: the transform's style and direction in one.
///
/// Port of `ExposureContrastOpData::Style` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpData.h:124-132 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExposureContrastOpStyle {
    /// `STYLE_LINEAR`: E/C to be applied to a linear space image.
    Linear,
    /// `STYLE_LINEAR_REV`: inverse of `STYLE_LINEAR`.
    LinearRev,
    /// `STYLE_VIDEO`: E/C to be applied to a video space image.
    Video,
    /// `STYLE_VIDEO_REV`: inverse of `STYLE_VIDEO`.
    VideoRev,
    /// `STYLE_LOGARITHMIC`: E/C to be applied to a log space image.
    Logarithmic,
    /// `STYLE_LOGARITHMIC_REV`: inverse of `STYLE_LOGARITHMIC`.
    LogarithmicRev,
}

/// The ExposureContrast op's data: a style, the exposure, contrast and gamma (each a double
/// dynamic property, which the op shares with its renderer and the processors), the pivot, and
/// the log style's exposure step and mid-gray.
///
/// Port of `ExposureContrastOpData` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpData.h:121-224 @ v2.5.2).
#[derive(Debug)]
pub struct ExposureContrastOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: ExposureContrastOpStyle,
    /// `m_exposure`.
    exposure: DynamicPropertyDoubleImplRcPtr,
    /// `m_contrast`.
    contrast: DynamicPropertyDoubleImplRcPtr,
    /// `m_gamma`.
    gamma: DynamicPropertyDoubleImplRcPtr,
    /// `m_pivot`.
    pivot: f64,
    /// `m_logExposureStep`.
    log_exposure_step: f64,
    /// `m_logMidGray`.
    log_mid_gray: f64,
}

impl Default for ExposureContrastOpData {
    /// The linear style: exposure 0, contrast 1, gamma 1, none dynamic.
    ///
    /// Port of `ExposureContrastOpData::ExposureContrastOpData()` (ExposureContrastOpData.cpp:
    /// 157-163 @ v2.5.2).
    fn default() -> Self {
        ExposureContrastOpData::new(ExposureContrastOpStyle::Linear)
    }
}

impl ExposureContrastOpData {
    /// `LOGEXPOSURESTEP_DEFAULT` (ExposureContrastOpData.h:211 @ v2.5.2).
    pub const LOGEXPOSURESTEP_DEFAULT: f64 = 0.088;
    /// `LOGMIDGRAY_DEFAULT` (ExposureContrastOpData.h:212 @ v2.5.2).
    pub const LOGMIDGRAY_DEFAULT: f64 = 0.435;

    /// The style's defaults: exposure 0, contrast 1, gamma 1, none dynamic, pivot 0.18, and the
    /// default log exposure step and mid-gray.
    ///
    /// Port of `ExposureContrastOpData::ExposureContrastOpData(Style)`
    /// (ExposureContrastOpData.cpp:165-172 @ v2.5.2), and of the members' initializers
    /// (ExposureContrastOpData.h:217-223).
    pub fn new(style: ExposureContrastOpStyle) -> Self {
        ExposureContrastOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            exposure: Arc::new(DynamicPropertyDoubleImpl::new(
                DynamicPropertyType::Exposure,
                0.,
                false,
            )),
            contrast: Arc::new(DynamicPropertyDoubleImpl::new(
                DynamicPropertyType::Contrast,
                1.,
                false,
            )),
            gamma: Arc::new(DynamicPropertyDoubleImpl::new(
                DynamicPropertyType::Gamma,
                1.,
                false,
            )),
            pivot: 0.18,
            log_exposure_step: Self::LOGEXPOSURESTEP_DEFAULT,
            log_mid_gray: Self::LOGMIDGRAY_DEFAULT,
        }
    }

    /// The style a CTF style attribute names, ignoring ASCII case (`Platform::Strcasecmp`,
    /// deviation D-4). `None` is a null pointer. The name ends at its first NUL, as upstream's
    /// C string does.
    ///
    /// Port of `ExposureContrastOpData::ConvertStringToStyle` (src/OpenColorIO/ops/
    /// exposurecontrast/ExposureContrastOpData.cpp:30-66 @ v2.5.2).
    pub fn convert_string_to_style(str: Option<&str>) -> Result<ExposureContrastOpStyle> {
        let str = str.map(|s| s.split('\0').next().unwrap_or(""));
        if let Some(str) = str.filter(|s| !s.is_empty()) {
            let styles = [
                (EC_STYLE_LINEAR, ExposureContrastOpStyle::Linear),
                (EC_STYLE_LINEAR_REV, ExposureContrastOpStyle::LinearRev),
                (EC_STYLE_VIDEO, ExposureContrastOpStyle::Video),
                (EC_STYLE_VIDEO_REV, ExposureContrastOpStyle::VideoRev),
                (EC_STYLE_LOGARITHMIC, ExposureContrastOpStyle::Logarithmic),
                (
                    EC_STYLE_LOGARITHMIC_REV,
                    ExposureContrastOpStyle::LogarithmicRev,
                ),
            ];
            for (name, style) in styles {
                if strcasecmp(str, name).is_eq() {
                    return Ok(style);
                }
            }

            return Err(Exception::new(format!(
                "Unknown exposure contrast style: '{str}'."
            )));
        }
        Err(Exception::new("Missing exposure contrast style."))
    }

    /// The style's CTF attribute string. (Upstream's error for a value outside the enum can't
    /// happen.)
    ///
    /// Port of `ExposureContrastOpData::ConvertStyleToString` (ExposureContrastOpData.cpp:
    /// 68-100 @ v2.5.2).
    pub fn convert_style_to_string(style: ExposureContrastOpStyle) -> &'static str {
        match style {
            ExposureContrastOpStyle::Linear => EC_STYLE_LINEAR,
            ExposureContrastOpStyle::LinearRev => EC_STYLE_LINEAR_REV,
            ExposureContrastOpStyle::Video => EC_STYLE_VIDEO,
            ExposureContrastOpStyle::VideoRev => EC_STYLE_VIDEO_REV,
            ExposureContrastOpStyle::Logarithmic => EC_STYLE_LOGARITHMIC,
            ExposureContrastOpStyle::LogarithmicRev => EC_STYLE_LOGARITHMIC_REV,
        }
    }

    /// The op style of a transform's style and direction. (Upstream's error for a style
    /// outside the enum can't happen.)
    ///
    /// Port of `ExposureContrastOpData::ConvertStyle(ExposureContrastStyle, TransformDirection)`
    /// (ExposureContrastOpData.cpp:102-131 @ v2.5.2).
    pub fn convert_transform_style(
        style: ExposureContrastStyle,
        dir: TransformDirection,
    ) -> ExposureContrastOpStyle {
        let is_forward = dir == TransformDirection::Forward;

        match style {
            ExposureContrastStyle::Video => {
                if is_forward {
                    ExposureContrastOpStyle::Video
                } else {
                    ExposureContrastOpStyle::VideoRev
                }
            }
            ExposureContrastStyle::Logarithmic => {
                if is_forward {
                    ExposureContrastOpStyle::Logarithmic
                } else {
                    ExposureContrastOpStyle::LogarithmicRev
                }
            }
            ExposureContrastStyle::Linear => {
                if is_forward {
                    ExposureContrastOpStyle::Linear
                } else {
                    ExposureContrastOpStyle::LinearRev
                }
            }
        }
    }

    /// The transform's style of an op style. (Upstream's error for a style outside the enum
    /// can't happen.)
    ///
    /// Port of `ExposureContrastOpData::ConvertStyle(Style)` (ExposureContrastOpData.cpp:
    /// 133-155 @ v2.5.2).
    pub fn convert_style(style: ExposureContrastOpStyle) -> ExposureContrastStyle {
        match style {
            ExposureContrastOpStyle::Video | ExposureContrastOpStyle::VideoRev => {
                ExposureContrastStyle::Video
            }
            ExposureContrastOpStyle::Logarithmic | ExposureContrastOpStyle::LogarithmicRev => {
                ExposureContrastStyle::Logarithmic
            }
            ExposureContrastOpStyle::Linear | ExposureContrastOpStyle::LinearRev => {
                ExposureContrastStyle::Linear
            }
        }
    }

    /// Port of `ExposureContrastOpData::getType` (ExposureContrastOpData.h:150 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::ExposureContrast
    }

    /// Nothing to check.
    ///
    /// Port of `ExposureContrastOpData::validate` (ExposureContrastOpData.cpp:186-188 @ v2.5.2).
    pub fn validate(&self) {}

    /// Port of `ExposureContrastOpData::isNoOp` (ExposureContrastOpData.cpp:190-193 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// Not dynamic, exposure 0, contrast 1 and gamma 1.
    ///
    /// Port of `ExposureContrastOpData::isIdentity` (ExposureContrastOpData.cpp:195-201 @
    /// v2.5.2).
    pub fn is_identity(&self) -> bool {
        !self.is_dynamic()
            && (self.exposure.get_value() == 0.)
            && (self.contrast.get_value() == 1.)
            && (self.gamma.get_value() == 1.)
    }

    /// Port of `ExposureContrastOpData::hasChannelCrosstalk` (ExposureContrastOpData.h:156 @
    /// v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// Whether any of exposure, contrast and gamma is dynamic.
    ///
    /// Port of `ExposureContrastOpData::isDynamic` (ExposureContrastOpData.cpp:203-208 @
    /// v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.exposure.is_dynamic() || self.contrast.is_dynamic() || self.gamma.is_dynamic()
    }

    /// Whether `r` undoes this op: neither is dynamic, and `r` equals this op's
    /// [`inverse`](Self::inverse).
    ///
    /// Port of `ExposureContrastOpData::isInverse` (ExposureContrastOpData.cpp:210-218 @
    /// v2.5.2).
    pub fn is_inverse(&self, r: &ExposureContrastOpData) -> bool {
        if self.is_dynamic() || r.is_dynamic() {
            return false;
        }

        *r == self.inverse()
    }

    /// Switches the style to its other direction.
    ///
    /// Port of `ExposureContrastOpData::invert` (ExposureContrastOpData.cpp:220-233 @ v2.5.2).
    fn invert(&mut self) {
        let inv_style = match self.get_style() {
            ExposureContrastOpStyle::Linear => ExposureContrastOpStyle::LinearRev,
            ExposureContrastOpStyle::LinearRev => ExposureContrastOpStyle::Linear,
            ExposureContrastOpStyle::Video => ExposureContrastOpStyle::VideoRev,
            ExposureContrastOpStyle::VideoRev => ExposureContrastOpStyle::Video,
            ExposureContrastOpStyle::Logarithmic => ExposureContrastOpStyle::LogarithmicRev,
            ExposureContrastOpStyle::LogarithmicRev => ExposureContrastOpStyle::Logarithmic,
        };
        self.set_style(inv_style);
    }

    /// A copy (with its own dynamic properties) in the other direction.
    ///
    /// Port of `ExposureContrastOpData::inverse` (ExposureContrastOpData.cpp:235-246 @ v2.5.2).
    pub fn inverse(&self) -> ExposureContrastOpData {
        let mut ec = self.clone();

        ec.invert();

        // Note that any existing metadata could become stale at this point but trying to
        // update it is also challenging since inverse() is sometimes called even during the
        // creation of new ops.

        ec
    }

    /// The ID (and a space) if any, the style, then `E: `, `C: ` and `G: ` with their values
    /// for those not dynamic, and `P: `, `LES: ` and `LMG: `, the doubles with 7 significant
    /// digits.
    ///
    /// Port of `ExposureContrastOpData::getCacheID` (ExposureContrastOpData.cpp:248-279 @
    /// v2.5.2).
    pub fn get_cache_id(&self) -> Vec<u8> {
        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let mut os = OStringStream::new(Crt::NATIVE);
        os.precision = FLOAT_DECIMALS;

        os.put_str(Self::convert_style_to_string(self.style));
        os.put_str(" ");

        if !self.exposure.is_dynamic() {
            os.put_str("E: ");
            os.put_f64(self.exposure.get_value());
            os.put_str(" ");
        }
        if !self.contrast.is_dynamic() {
            os.put_str("C: ");
            os.put_f64(self.contrast.get_value());
            os.put_str(" ");
        }
        if !self.gamma.is_dynamic() {
            os.put_str("G: ");
            os.put_f64(self.gamma.get_value());
            os.put_str(" ");
        }
        os.put_str("P: ");
        os.put_f64(self.pivot);
        os.put_str(" ");
        os.put_str("LES: ");
        os.put_f64(self.log_exposure_step);
        os.put_str(" ");
        os.put_str("LMG: ");
        os.put_f64(self.log_mid_gray);

        cache_id.extend_from_slice(os.str());
        cache_id
    }

    /// The same style, pivot, log exposure step and mid-gray, and equal properties
    /// ([`DynamicPropertyDoubleImpl::equals`]: a dynamic property equals no other). The
    /// metadata is ignored. The `OpData` base's shortcut for the same object stays: an op data
    /// with a NaN pivot equals itself. Its type comparison is
    /// [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `ExposureContrastOpData::equals` (ExposureContrastOpData.cpp:281-296 @ v2.5.2)
    /// and `OpData::equals` (src/OpenColorIO/Op.cpp:73-79).
    pub fn equals(&self, other: &ExposureContrastOpData) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }

        // NB: Please see note in DynamicProperty.h describing how dynamic properties are
        // compared for equality.
        self.get_style() == other.get_style()
            && self.get_pivot() == other.get_pivot()
            && self.get_log_exposure_step() == other.get_log_exposure_step()
            && self.get_log_mid_gray() == other.get_log_mid_gray()
            && self.exposure.equals(&other.exposure)
            && self.contrast.equals(&other.contrast)
            && self.gamma.equals(&other.gamma)
    }

    /// Whether the property of the type is dynamic; false for the grading types.
    ///
    /// Port of `ExposureContrastOpData::hasDynamicProperty` (ExposureContrastOpData.cpp:
    /// 298-321 @ v2.5.2).
    pub fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        match type_ {
            DynamicPropertyType::Exposure => self.exposure.is_dynamic(),
            DynamicPropertyType::Contrast => self.contrast.is_dynamic(),
            DynamicPropertyType::Gamma => self.gamma.is_dynamic(),
            DynamicPropertyType::GradingPrimary
            | DynamicPropertyType::GradingRgbCurve
            | DynamicPropertyType::GradingHueCurve
            | DynamicPropertyType::GradingTone => false,
        }
    }

    /// The property of the type, shared, if it is dynamic.
    ///
    /// Port of `ExposureContrastOpData::getDynamicProperty` (ExposureContrastOpData.cpp:
    /// 323-354 @ v2.5.2).
    pub fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        let prop = match type_ {
            DynamicPropertyType::Exposure => &self.exposure,
            DynamicPropertyType::Contrast => &self.contrast,
            DynamicPropertyType::Gamma => &self.gamma,
            DynamicPropertyType::GradingPrimary
            | DynamicPropertyType::GradingRgbCurve
            | DynamicPropertyType::GradingHueCurve
            | DynamicPropertyType::GradingTone => {
                return Err(Exception::new(TYPE_NOT_SUPPORTED));
            }
        };
        if prop.is_dynamic() {
            return Ok(DynamicPropertyRcPtr::Double(Arc::clone(prop)));
        }
        Err(Exception::new(NOT_DYNAMIC))
    }

    /// Makes the data share `prop` as its property of the type, when that property is
    /// dynamic. As upstream, `prop`'s own type isn't checked.
    ///
    /// Upstream also refuses a null `prop` ("Dynamic property type not supported by
    /// ExposureContrast."): an [`Arc`] is never null.
    ///
    /// Port of `ExposureContrastOpData::replaceDynamicProperty` (ExposureContrastOpData.cpp:
    /// 356-396 @ v2.5.2).
    pub fn replace_dynamic_property(
        &mut self,
        type_: DynamicPropertyType,
        prop: DynamicPropertyDoubleImplRcPtr,
    ) -> Result<()> {
        let slot = match type_ {
            DynamicPropertyType::Exposure => &mut self.exposure,
            DynamicPropertyType::Contrast => &mut self.contrast,
            DynamicPropertyType::Gamma => &mut self.gamma,
            DynamicPropertyType::GradingPrimary
            | DynamicPropertyType::GradingRgbCurve
            | DynamicPropertyType::GradingHueCurve
            | DynamicPropertyType::GradingTone => {
                return Err(Exception::new(TYPE_NOT_SUPPORTED));
            }
        };
        if slot.is_dynamic() {
            *slot = prop;
            return Ok(());
        }
        Err(Exception::new(NOT_DYNAMIC))
    }

    /// Makes the three properties non-dynamic, in place: for every op data that shares them.
    ///
    /// Port of `ExposureContrastOpData::removeDynamicProperties` (ExposureContrastOpData.cpp:
    /// 398-403 @ v2.5.2).
    pub fn remove_dynamic_properties(&self) {
        self.exposure.make_non_dynamic();
        self.contrast.make_non_dynamic();
        self.gamma.make_non_dynamic();
    }

    /// Takes `rhs`'s metadata, style, values, pivot, log exposure step and mid-gray. The
    /// values go into this data's own properties (and so to whatever shares them), and a
    /// property that is dynamic in `rhs` becomes dynamic; one that isn't keeps its state.
    ///
    /// Upstream returns at once when `rhs` is the data itself; a Rust borrow can't be both.
    ///
    /// Port of `ExposureContrastOpData::operator=` (ExposureContrastOpData.cpp:405-433 @
    /// v2.5.2) and `OpData::operator=` (src/OpenColorIO/Op.cpp:54-62).
    pub fn assign(&mut self, rhs: &ExposureContrastOpData) {
        self.metadata = rhs.metadata.clone();

        self.style = rhs.style;
        // Copy dynamic properties. Sharing happens when needed, with CPUop for instance.
        self.exposure.set_value(rhs.exposure.get_value());
        self.contrast.set_value(rhs.contrast.get_value());
        self.gamma.set_value(rhs.gamma.get_value());
        if rhs.exposure.is_dynamic() {
            self.exposure.make_dynamic();
        }
        if rhs.contrast.is_dynamic() {
            self.contrast.make_dynamic();
        }
        if rhs.gamma.is_dynamic() {
            self.gamma.make_dynamic();
        }
        self.pivot = rhs.pivot;
        self.log_exposure_step = rhs.log_exposure_step;
        self.log_mid_gray = rhs.log_mid_gray;
    }

    /// Port of `ExposureContrastOpData::getStyle` (ExposureContrastOpData.h:174 @ v2.5.2).
    pub fn get_style(&self) -> ExposureContrastOpStyle {
        self.style
    }

    /// Port of `ExposureContrastOpData::setStyle` (ExposureContrastOpData.h:175 @ v2.5.2).
    pub fn set_style(&mut self, style: ExposureContrastOpStyle) {
        self.style = style;
    }

    /// Forward for the three forward styles, inverse for the `*_REV` ones.
    ///
    /// Port of `ExposureContrastOpData::getDirection` (ExposureContrastOpData.cpp:435-451 @
    /// v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        match self.style {
            ExposureContrastOpStyle::Linear
            | ExposureContrastOpStyle::Video
            | ExposureContrastOpStyle::Logarithmic => TransformDirection::Forward,
            ExposureContrastOpStyle::LinearRev
            | ExposureContrastOpStyle::VideoRev
            | ExposureContrastOpStyle::LogarithmicRev => TransformDirection::Inverse,
        }
    }

    /// Switches the style to its other direction when `dir` isn't its direction.
    ///
    /// Port of `ExposureContrastOpData::setDirection` (ExposureContrastOpData.cpp:453-459 @
    /// v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        if self.get_direction() != dir {
            self.invert();
        }
    }

    /// Port of `ExposureContrastOpData::getExposure` (ExposureContrastOpData.h:180 @ v2.5.2).
    pub fn get_exposure(&self) -> f64 {
        self.exposure.get_value()
    }

    /// Sets the exposure through its property, so whatever shares it sees the value.
    ///
    /// Port of `ExposureContrastOpData::setExposure` (ExposureContrastOpData.h:181 @ v2.5.2).
    pub fn set_exposure(&self, exposure: f64) {
        self.exposure.set_value(exposure);
    }

    /// Port of `ExposureContrastOpData::getContrast` (ExposureContrastOpData.h:183 @ v2.5.2).
    pub fn get_contrast(&self) -> f64 {
        self.contrast.get_value()
    }

    /// Sets the contrast through its property.
    ///
    /// Port of `ExposureContrastOpData::setContrast` (ExposureContrastOpData.h:184 @ v2.5.2).
    pub fn set_contrast(&self, contrast: f64) {
        self.contrast.set_value(contrast);
    }

    /// Port of `ExposureContrastOpData::getGamma` (ExposureContrastOpData.h:186 @ v2.5.2).
    pub fn get_gamma(&self) -> f64 {
        self.gamma.get_value()
    }

    /// Sets the gamma through its property.
    ///
    /// Port of `ExposureContrastOpData::setGamma` (ExposureContrastOpData.h:187 @ v2.5.2).
    pub fn set_gamma(&self, gamma: f64) {
        self.gamma.set_value(gamma);
    }

    /// Port of `ExposureContrastOpData::getPivot` (ExposureContrastOpData.h:189 @ v2.5.2).
    pub fn get_pivot(&self) -> f64 {
        self.pivot
    }

    /// Port of `ExposureContrastOpData::setPivot` (ExposureContrastOpData.h:190 @ v2.5.2).
    pub fn set_pivot(&mut self, pivot: f64) {
        self.pivot = pivot;
    }

    /// Port of `ExposureContrastOpData::getLogExposureStep` (ExposureContrastOpData.h:192 @
    /// v2.5.2).
    pub fn get_log_exposure_step(&self) -> f64 {
        self.log_exposure_step
    }

    /// Port of `ExposureContrastOpData::setLogExposureStep` (ExposureContrastOpData.h:193 @
    /// v2.5.2).
    pub fn set_log_exposure_step(&mut self, step: f64) {
        self.log_exposure_step = step;
    }

    /// Port of `ExposureContrastOpData::getLogMidGray` (ExposureContrastOpData.h:195 @ v2.5.2).
    pub fn get_log_mid_gray(&self) -> f64 {
        self.log_mid_gray
    }

    /// Port of `ExposureContrastOpData::setLogMidGray` (ExposureContrastOpData.h:196 @ v2.5.2).
    pub fn set_log_mid_gray(&mut self, mid_gray: f64) {
        self.log_mid_gray = mid_gray;
    }

    /// The exposure property, shared.
    ///
    /// Port of `ExposureContrastOpData::getExposureProperty` (ExposureContrastOpData.h:198-201
    /// @ v2.5.2).
    pub fn get_exposure_property(&self) -> DynamicPropertyDoubleImplRcPtr {
        Arc::clone(&self.exposure)
    }

    /// The contrast property, shared.
    ///
    /// Port of `ExposureContrastOpData::getContrastProperty` (ExposureContrastOpData.h:202-205
    /// @ v2.5.2).
    pub fn get_contrast_property(&self) -> DynamicPropertyDoubleImplRcPtr {
        Arc::clone(&self.contrast)
    }

    /// The gamma property, shared.
    ///
    /// Port of `ExposureContrastOpData::getGammaProperty` (ExposureContrastOpData.h:206-209 @
    /// v2.5.2).
    pub fn get_gamma_property(&self) -> DynamicPropertyDoubleImplRcPtr {
        Arc::clone(&self.gamma)
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

/// A copy with its own dynamic properties, holding the same values and dynamic states: sharing
/// happens when needed, with the CPU op for instance.
///
/// Port of `ExposureContrastOpData::clone` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOpData.cpp:178-184 @ v2.5.2): new data of the style, then `operator=`
/// ([`ExposureContrastOpData::assign`]).
impl Clone for ExposureContrastOpData {
    fn clone(&self) -> Self {
        let mut res = ExposureContrastOpData::new(self.get_style());
        res.assign(self);
        res
    }
}

/// Port of `operator==(const ExposureContrastOpData &, const ExposureContrastOpData &)`
/// (ExposureContrastOpData.cpp:461-464 @ v2.5.2).
impl PartialEq for ExposureContrastOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "exposure_contrast_op_data_tests.rs"]
mod tests;
