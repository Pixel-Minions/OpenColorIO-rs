// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingRGBCurve op's data. A port of
//! `src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpData.h` and `GradingRGBCurveOpData.cpp`
//! @ v2.5.2.

use std::sync::Arc;

use super::grading_b_spline_curve::GradingBSplineCurve;
use super::grading_rgb_curve::{GradingRgbCurve, default_curve_for};
use crate::cfmt::{Crt, OStringStream};
use crate::dynamic_property::{
    DynamicPropertyGradingRgbCurveImpl, DynamicPropertyGradingRgbCurveImplRcPtr,
    DynamicPropertyRcPtr,
};
use crate::exception::Result;
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::op_data::OpDataType;
use crate::open_color_types::{
    GradingStyle, RgbCurveType, TransformDirection, combine_transform_directions,
    get_inverse_transform_direction, grading_style_to_string, transform_direction_to_string,
};

/// `DefaultValues::FLOAT_DECIMALS` (GradingRGBCurveOpData.cpp:15-17 @ v2.5.2): the cache ID's
/// precision.
const FLOAT_DECIMALS: i64 = 7;

/// The GradingRGBCurve op's data: a style, the curves (held by a dynamic property, which the
/// op shares with its renderer and the processors), whether the linear style bypasses its
/// lin-to-log conversion, and a direction.
///
/// Port of `GradingRGBCurveOpData` (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpData.h:
/// 22-86 @ v2.5.2).
#[derive(Debug)]
pub struct GradingRgbCurveOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: GradingStyle,
    /// `m_value`.
    value: DynamicPropertyGradingRgbCurveImplRcPtr,
    /// `m_bypassLinToLog`.
    bypass_lin_to_log: bool,
    /// `m_direction`.
    direction: TransformDirection,
}

impl GradingRgbCurveOpData {
    /// The default curves of `style`, forward.
    ///
    /// Port of `GradingRGBCurveOpData::GradingRGBCurveOpData(GradingStyle)`
    /// (GradingRGBCurveOpData.cpp:25-29 @ v2.5.2).
    pub fn new(style: GradingStyle) -> Self {
        let curve = default_curve_for(style);
        Self::with_curves(style, &curve, &curve, &curve, &curve)
            .expect("the default curves fit within the knots and coefficients")
    }

    /// The curves given, not validated, forward; the errors of fitting them (too many control
    /// points).
    ///
    /// Port of `GradingRGBCurveOpData::GradingRGBCurveOpData(GradingStyle, red, green, blue,
    /// master)` (GradingRGBCurveOpData.cpp:31-41 @ v2.5.2).
    pub fn with_curves(
        style: GradingStyle,
        red: &GradingBSplineCurve,
        green: &GradingBSplineCurve,
        blue: &GradingBSplineCurve,
        master: &GradingBSplineCurve,
    ) -> Result<Self> {
        let rgb_curve = GradingRgbCurve::with_curves(red, green, blue, master);
        Ok(GradingRgbCurveOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            value: Arc::new(DynamicPropertyGradingRgbCurveImpl::new(&rgb_curve, false)?),
            bypass_lin_to_log: false,
            direction: TransformDirection::Forward,
        })
    }

    /// Port of `GradingRGBCurveOpData::validate` (GradingRGBCurveOpData.cpp:84-88 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // This should already be valid.
        self.value.get_value().validate()
    }

    /// Port of `GradingRGBCurveOpData::getType` (GradingRGBCurveOpData.h:42 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::GradingRgbCurve
    }

    /// Port of `GradingRGBCurveOpData::isNoOp` (GradingRGBCurveOpData.cpp:90-93 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// A dynamic op is never an identity: its curves can change.
    ///
    /// Port of `GradingRGBCurveOpData::isIdentity` (GradingRGBCurveOpData.cpp:95-100 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }
        self.value.get_value().is_identity()
    }

    /// Port of `GradingRGBCurveOpData::hasChannelCrosstalk` (GradingRGBCurveOpData.h:47 @
    /// v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// Whether `r` undoes this op: neither is dynamic, the same style (and bypass for the
    /// linear style), the same curves, and opposite directions.
    ///
    /// Port of `GradingRGBCurveOpData::isInverse` (GradingRGBCurveOpData.cpp:102-119 @
    /// v2.5.2).
    pub fn is_inverse(&self, r: &GradingRgbCurveOpData) -> bool {
        if self.is_dynamic() || r.is_dynamic() {
            return false;
        }
        self.style == r.style
            && (self.style != GradingStyle::Lin || self.bypass_lin_to_log == r.bypass_lin_to_log)
            && self.value.equals(&r.value)
            && combine_transform_directions(self.get_direction(), r.get_direction())
                == TransformDirection::Inverse
    }

    /// A copy in the other direction.
    ///
    /// Port of `GradingRGBCurveOpData::inverse` (GradingRGBCurveOpData.cpp:121-126 @ v2.5.2).
    pub fn inverse(&self) -> GradingRgbCurveOpData {
        let mut res = self.clone();
        res.direction = get_inverse_transform_direction(self.direction);
        res
    }

    /// The ID, the style, the direction, ` bypassLinToLog` when set (after a second space),
    /// and the curves with 7 significant digits unless the op is dynamic.
    ///
    /// Port of `GradingRGBCurveOpData::getCacheID` (GradingRGBCurveOpData.cpp:128-150 @
    /// v2.5.2).
    pub fn get_cache_id(&self) -> Vec<u8> {
        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let mut os = OStringStream::new(Crt::NATIVE);
        os.precision = FLOAT_DECIMALS;

        os.put_str(grading_style_to_string(self.style));
        os.put_str(" ");
        os.put_str(transform_direction_to_string(self.direction));
        os.put_str(" ");
        if self.bypass_lin_to_log {
            os.put_str(" bypassLinToLog");
        }
        if !self.is_dynamic() {
            self.value.state().value.write_to(&mut os);
        }
        cache_id.extend_from_slice(os.str());
        cache_id
    }

    /// Port of `GradingRGBCurveOpData::getStyle` (GradingRGBCurveOpData.h:57 @ v2.5.2).
    pub fn get_style(&self) -> GradingStyle {
        self.style
    }

    /// Changing the style resets the curves to the new style's defaults.
    ///
    /// Port of `GradingRGBCurveOpData::setStyle` (GradingRGBCurveOpData.cpp:152-161 @ v2.5.2).
    pub fn set_style(&mut self, style: GradingStyle) {
        if style != self.style {
            self.style = style;
            // Reset value to default when style is changing.
            let reset = GradingRgbCurve::new(style);
            self.value
                .set_value(&reset)
                .expect("the default curves are valid and fit");
        }
    }

    /// A copy of the curves.
    ///
    /// Port of `GradingRGBCurveOpData::getValue` (GradingRGBCurveOpData.h:60 @ v2.5.2).
    pub fn get_value(&self) -> GradingRgbCurve {
        self.value.get_value()
    }

    /// Validates the curves and sets them (through the dynamic property, so a processor that
    /// shares it sees them).
    ///
    /// Port of `GradingRGBCurveOpData::setValue` (GradingRGBCurveOpData.h:61 @ v2.5.2).
    pub fn set_value(&self, values: &GradingRgbCurve) -> Result<()> {
        self.value.set_value(values)
    }

    /// Port of `GradingRGBCurveOpData::getSlope` (GradingRGBCurveOpData.cpp:163-167 @ v2.5.2).
    pub fn get_slope(&self, c: RgbCurveType, index: usize) -> Result<f32> {
        self.value.state().value.curve(c)?.slope(index)
    }

    /// Port of `GradingRGBCurveOpData::setSlope` (GradingRGBCurveOpData.cpp:169-175 @ v2.5.2).
    pub fn set_slope(&self, c: RgbCurveType, index: usize, slope: f32) -> Result<()> {
        let mut rgbcurve = self.value.get_value();
        rgbcurve.curve_mut(c)?.set_slope(index, slope)?;
        self.value.set_value(&rgbcurve)
    }

    /// Port of `GradingRGBCurveOpData::slopesAreDefault` (GradingRGBCurveOpData.cpp:177-181 @
    /// v2.5.2).
    pub fn slopes_are_default(&self, c: RgbCurveType) -> Result<bool> {
        Ok(self.value.state().value.curve(c)?.slopes_are_default())
    }

    /// Port of `GradingRGBCurveOpData::getBypassLinToLog` (GradingRGBCurveOpData.h:67 @
    /// v2.5.2).
    pub fn get_bypass_lin_to_log(&self) -> bool {
        self.bypass_lin_to_log
    }

    /// Port of `GradingRGBCurveOpData::setBypassLinToLog` (GradingRGBCurveOpData.h:68 @
    /// v2.5.2).
    pub fn set_bypass_lin_to_log(&mut self, bypass: bool) {
        self.bypass_lin_to_log = bypass;
    }

    /// Port of `GradingRGBCurveOpData::getDirection` (GradingRGBCurveOpData.cpp:183-186 @
    /// v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `GradingRGBCurveOpData::setDirection` (GradingRGBCurveOpData.cpp:188-191 @
    /// v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// Port of `GradingRGBCurveOpData::isDynamic` (GradingRGBCurveOpData.cpp:193-196 @ v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    /// The dynamic property, shared.
    ///
    /// Port of `GradingRGBCurveOpData::getDynamicProperty` (GradingRGBCurveOpData.cpp:198-201 @
    /// v2.5.2).
    pub fn get_dynamic_property(&self) -> DynamicPropertyRcPtr {
        DynamicPropertyRcPtr::GradingRgbCurve(Arc::clone(&self.value))
    }

    /// Port of `GradingRGBCurveOpData::replaceDynamicProperty` (GradingRGBCurveOpData.cpp:
    /// 203-206 @ v2.5.2).
    pub fn replace_dynamic_property(&mut self, prop: DynamicPropertyGradingRgbCurveImplRcPtr) {
        self.value = prop;
    }

    /// Makes the property non-dynamic.
    ///
    /// Port of `GradingRGBCurveOpData::removeDynamicProperty` (GradingRGBCurveOpData.cpp:
    /// 208-211 @ v2.5.2).
    pub fn remove_dynamic_property(&self) {
        self.value.make_non_dynamic();
    }

    /// The dynamic property, as its class.
    ///
    /// Port of `GradingRGBCurveOpData::getDynamicPropertyInternal` (GradingRGBCurveOpData.h:
    /// 77-80 @ v2.5.2).
    pub fn get_dynamic_property_internal(&self) -> DynamicPropertyGradingRgbCurveImplRcPtr {
        Arc::clone(&self.value)
    }

    /// The same direction, style, bypass and property ([`DynamicPropertyGradingRgbCurveImpl::
    /// equals`]). The metadata is ignored. The `OpData` base's type comparison is
    /// [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `GradingRGBCurveOpData::equals` (GradingRGBCurveOpData.cpp:213-229 @ v2.5.2).
    pub fn equals(&self, other: &GradingRgbCurveOpData) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.direction == other.direction
            && self.style == other.style
            && self.bypass_lin_to_log == other.bypass_lin_to_log
            && self.value.equals(&other.value)
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

/// A copy with its own dynamic property, holding the same curves (and knots) and dynamic state:
/// sharing happens when needed, with the CPU op for instance.
///
/// Port of the copy constructor and `operator=` of `GradingRGBCurveOpData`
/// (GradingRGBCurveOpData.cpp:43-72 @ v2.5.2), and `clone` (78-82). Upstream copies the curves
/// with the property's `setValue`, which validates and fits them again: copying data whose
/// curves don't validate, or don't fit, throws there. The port copies the curves and their
/// knots as they are. Such data comes only from [`GradingRgbCurveOpData::with_curves`] with
/// invalid curves, or from a `setValue` that failed to fit its curves; both raise before the
/// data is copied (validating the op, building it from a transform).
impl Clone for GradingRgbCurveOpData {
    fn clone(&self) -> Self {
        let state = self.value.state().clone();
        let value = DynamicPropertyGradingRgbCurveImpl::from_state(state, self.is_dynamic());
        GradingRgbCurveOpData {
            metadata: self.metadata.clone(),
            style: self.style,
            value: Arc::new(value),
            bypass_lin_to_log: self.bypass_lin_to_log,
            direction: self.direction,
        }
    }
}

/// Port of `operator==(const GradingRGBCurveOpData &, const GradingRGBCurveOpData &)`
/// (GradingRGBCurveOpData.cpp:231-234 @ v2.5.2).
impl PartialEq for GradingRgbCurveOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "grading_rgb_curve_op_data_tests.rs"]
mod tests;
