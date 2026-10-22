// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingHueCurve op's data. A port of
//! `src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOpData.h` and
//! `GradingHueCurveOpData.cpp` @ v2.5.2.
//!
//! The data is a style, a direction, whether the op converts RGB to HSY around its curves, and
//! a dynamic property, which holds the eight curves (with their draw-curve-only flag) and their
//! knots and coefficients: the op shares the property with its renderer and the processors, so
//! a set of the property's value reaches every one of them.

use std::sync::Arc;

use super::grading_hue_curve::GradingHueCurve;
use crate::cfmt::{Crt, OStringStream};
use crate::dynamic_property::{
    DynamicPropertyGradingHueCurveImpl, DynamicPropertyGradingHueCurveImplRcPtr,
    DynamicPropertyRcPtr,
};
use crate::exception::Result;
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::op_data::OpDataType;
use crate::open_color_types::{
    GradingStyle, HsyTransformStyle, HueCurveType, TransformDirection,
    combine_transform_directions, get_inverse_transform_direction, grading_style_to_string,
    transform_direction_to_string,
};
use crate::ops::gradingrgbcurve::grading_b_spline_curve::GradingBSplineCurve;

/// `DefaultValues::FLOAT_DECIMALS` (GradingHueCurveOpData.cpp:15-18 @ v2.5.2): the cache ID's
/// precision.
const FLOAT_DECIMALS: i64 = 7;

/// The GradingHueCurve op's data: a style, the curves (held by a dynamic property, which the op
/// shares with its renderer and the processors), whether it converts RGB to HSY, and a
/// direction.
///
/// Port of `GradingHueCurveOpData` (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOpData.h:
/// 22-89 @ v2.5.2).
#[derive(Debug)]
pub struct GradingHueCurveOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: GradingStyle,
    /// `m_value`.
    value: DynamicPropertyGradingHueCurveImplRcPtr,
    /// `m_RGBToHSY`.
    rgb_to_hsy: HsyTransformStyle,
    /// `m_direction`.
    direction: TransformDirection,
}

impl GradingHueCurveOpData {
    /// The default curves of `style`, forward, converting RGB to HSY.
    ///
    /// Port of `GradingHueCurveOpData::GradingHueCurveOpData(GradingStyle)`
    /// (GradingHueCurveOpData.cpp:20-26 @ v2.5.2).
    pub fn new(style: GradingStyle) -> Self {
        let hue_curve = GradingHueCurve::new(style);
        GradingHueCurveOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            value: Arc::new(
                DynamicPropertyGradingHueCurveImpl::new(&hue_curve, false)
                    .expect("the default curves fit within the knots and coefficients"),
            ),
            rgb_to_hsy: HsyTransformStyle::Transform1,
            direction: TransformDirection::Forward,
        }
    }

    /// Copies of the eight curves, validated ([`GradingHueCurve::with_curves`]), forward; the
    /// errors of validating or fitting them.
    ///
    /// Port of `GradingHueCurveOpData::GradingHueCurveOpData(GradingStyle, hueHue, hueSat,
    /// hueLum, lumSat, satSat, lumLum, satLum, hueFx)` (GradingHueCurveOpData.cpp:38-60 @
    /// v2.5.2).
    #[allow(clippy::too_many_arguments)]
    pub fn with_curves(
        style: GradingStyle,
        hue_hue: &GradingBSplineCurve,
        hue_sat: &GradingBSplineCurve,
        hue_lum: &GradingBSplineCurve,
        lum_sat: &GradingBSplineCurve,
        sat_sat: &GradingBSplineCurve,
        lum_lum: &GradingBSplineCurve,
        sat_lum: &GradingBSplineCurve,
        hue_fx: &GradingBSplineCurve,
    ) -> Result<Self> {
        let hue_curve = GradingHueCurve::with_curves(
            hue_hue, hue_sat, hue_lum, lum_sat, sat_sat, lum_lum, sat_lum, hue_fx,
        )?;
        Ok(GradingHueCurveOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            value: Arc::new(DynamicPropertyGradingHueCurveImpl::new(&hue_curve, false)?),
            rgb_to_hsy: HsyTransformStyle::Transform1,
            direction: TransformDirection::Forward,
        })
    }

    /// Checks the curves ([`GradingHueCurve::validate`]). They should already be valid: the
    /// property validates each value it is given.
    ///
    /// Port of `GradingHueCurveOpData::validate` (GradingHueCurveOpData.cpp:91-96 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // This should already be valid.
        self.value.get_value().validate()
    }

    /// Port of `GradingHueCurveOpData::getType` (GradingHueCurveOpData.h:44 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::GradingHueCurve
    }

    /// Port of `GradingHueCurveOpData::isNoOp` (GradingHueCurveOpData.cpp:98-101 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// A dynamic op is never an identity: its curves can change. Otherwise, whether every
    /// curve is an identity of its spline type (whatever the draw-curve-only flag).
    ///
    /// Port of `GradingHueCurveOpData::isIdentity` (GradingHueCurveOpData.cpp:103-108 @
    /// v2.5.2).
    pub fn is_identity(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }
        self.value.state().value.is_identity()
    }

    /// Always: the curves of one channel depend on the others (hue, saturation and luma).
    ///
    /// Port of `GradingHueCurveOpData::hasChannelCrosstalk` (GradingHueCurveOpData.h:49 @
    /// v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        true
    }

    /// Whether `r` undoes this op: neither is dynamic, the same style (and, for the linear
    /// style only, the same RGB-to-HSY setting), the same curves, and opposite directions.
    ///
    /// Port of `GradingHueCurveOpData::isInverse` (GradingHueCurveOpData.cpp:110-127 @ v2.5.2).
    pub fn is_inverse(&self, r: &GradingHueCurveOpData) -> bool {
        if self.is_dynamic() || r.is_dynamic() {
            return false;
        }

        self.style == r.style
            && (self.style != GradingStyle::Lin || self.rgb_to_hsy == r.rgb_to_hsy)
            && self.value.equals(&r.value)
            && combine_transform_directions(self.get_direction(), r.get_direction())
                == TransformDirection::Inverse
    }

    /// A copy in the other direction.
    ///
    /// Port of `GradingHueCurveOpData::inverse` (GradingHueCurveOpData.cpp:129-134 @ v2.5.2).
    pub fn inverse(&self) -> GradingHueCurveOpData {
        let mut res = self.clone();
        res.direction = get_inverse_transform_direction(self.direction);
        res
    }

    /// The ID (and a space) when there is one, the style, the direction, `  bypassRGBToHSY `
    /// (after a second space) when the op doesn't convert, and the curves with 7 significant
    /// digits unless the op is dynamic. The draw-curve-only flag isn't in it.
    ///
    /// Port of `GradingHueCurveOpData::getCacheID` (GradingHueCurveOpData.cpp:136-159 @
    /// v2.5.2).
    pub fn get_cache_id(&self) -> Vec<u8> {
        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let mut os = OStringStream::new(Crt::NATIVE);
        os.precision = FLOAT_DECIMALS;

        os.put_str(grading_style_to_string(self.get_style()));
        os.put_str(" ");
        os.put_str(transform_direction_to_string(self.get_direction()));
        os.put_str(" ");
        if self.rgb_to_hsy != HsyTransformStyle::Transform1 {
            os.put_str(" bypassRGBToHSY ");
        }
        if !self.is_dynamic() {
            self.value.state().value.write_to(&mut os);
        }
        cache_id.extend_from_slice(os.str());
        cache_id
    }

    /// Port of `GradingHueCurveOpData::getStyle` (GradingHueCurveOpData.h:56 @ v2.5.2).
    pub fn get_style(&self) -> GradingStyle {
        self.style
    }

    /// Changing the style resets the curves to the new style's defaults (and the
    /// draw-curve-only flag to off).
    ///
    /// Port of `GradingHueCurveOpData::setStyle` (GradingHueCurveOpData.cpp:161-170 @ v2.5.2).
    pub fn set_style(&mut self, style: GradingStyle) {
        if style != self.style {
            self.style = style;
            // Reset value to default when style is changing.
            let reset = GradingHueCurve::new(style);
            self.value
                .set_value(&reset)
                .expect("the default curves are valid and fit");
        }
    }

    /// A copy of the curves.
    ///
    /// Port of `GradingHueCurveOpData::getValue` (GradingHueCurveOpData.h:59 @ v2.5.2).
    pub fn get_value(&self) -> GradingHueCurve {
        self.value.get_value()
    }

    /// Validates the curves and sets them (through the dynamic property, so a processor that
    /// shares it sees them).
    ///
    /// Port of `GradingHueCurveOpData::setValue` (GradingHueCurveOpData.h:60 @ v2.5.2).
    pub fn set_value(&self, values: &GradingHueCurve) -> Result<()> {
        self.value.set_value(values)
    }

    /// The slope of curve `c` at control point `index`.
    ///
    /// Port of `GradingHueCurveOpData::getSlope` (GradingHueCurveOpData.cpp:172-176 @ v2.5.2).
    pub fn get_slope(&self, c: HueCurveType, index: usize) -> Result<f32> {
        self.value.state().value.curve(c)?.slope(index)
    }

    /// Sets the slope of curve `c` at control point `index`, on a copy of the curves that the
    /// property then validates and takes.
    ///
    /// Port of `GradingHueCurveOpData::setSlope` (GradingHueCurveOpData.cpp:178-184 @ v2.5.2).
    pub fn set_slope(&self, c: HueCurveType, index: usize, slope: f32) -> Result<()> {
        let mut hue_curve = self.value.get_value();
        hue_curve.curve_mut(c)?.set_slope(index, slope)?;
        self.value.set_value(&hue_curve)
    }

    /// Port of `GradingHueCurveOpData::slopesAreDefault` (GradingHueCurveOpData.cpp:186-190 @
    /// v2.5.2).
    pub fn slopes_are_default(&self, c: HueCurveType) -> Result<bool> {
        Ok(self.value.state().value.curve(c)?.slopes_are_default())
    }

    /// Port of `GradingHueCurveOpData::getRGBToHSY` (GradingHueCurveOpData.h:66 @ v2.5.2).
    #[doc(alias = "getRGBToHSY")]
    pub fn get_rgb_to_hsy(&self) -> HsyTransformStyle {
        self.rgb_to_hsy
    }

    /// Port of `GradingHueCurveOpData::setRGBToHSY` (GradingHueCurveOpData.h:67 @ v2.5.2).
    #[doc(alias = "setRGBToHSY")]
    pub fn set_rgb_to_hsy(&mut self, style: HsyTransformStyle) {
        self.rgb_to_hsy = style;
    }

    /// Port of `GradingHueCurveOpData::getDirection` (GradingHueCurveOpData.cpp:192-195 @
    /// v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `GradingHueCurveOpData::setDirection` (GradingHueCurveOpData.cpp:197-200 @
    /// v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// Port of `GradingHueCurveOpData::isDynamic` (GradingHueCurveOpData.cpp:202-205 @ v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    /// The dynamic property, shared.
    ///
    /// Port of `GradingHueCurveOpData::getDynamicProperty` (GradingHueCurveOpData.cpp:207-210
    /// @ v2.5.2).
    pub fn get_dynamic_property(&self) -> DynamicPropertyRcPtr {
        DynamicPropertyRcPtr::GradingHueCurve(Arc::clone(&self.value))
    }

    /// Port of `GradingHueCurveOpData::replaceDynamicProperty` (GradingHueCurveOpData.cpp:
    /// 212-215 @ v2.5.2).
    pub fn replace_dynamic_property(&mut self, prop: DynamicPropertyGradingHueCurveImplRcPtr) {
        self.value = prop;
    }

    /// Makes the property non-dynamic.
    ///
    /// Port of `GradingHueCurveOpData::removeDynamicProperty` (GradingHueCurveOpData.cpp:
    /// 217-220 @ v2.5.2).
    pub fn remove_dynamic_property(&self) {
        self.value.make_non_dynamic();
    }

    /// The dynamic property, as its class.
    ///
    /// Port of `GradingHueCurveOpData::getDynamicPropertyInternal` (GradingHueCurveOpData.h:
    /// 77-80 @ v2.5.2).
    pub fn get_dynamic_property_internal(&self) -> DynamicPropertyGradingHueCurveImplRcPtr {
        Arc::clone(&self.value)
    }

    /// The same direction, style, RGB-to-HSY setting and property
    /// ([`DynamicPropertyGradingHueCurveImpl::equals`]: the curves and draw-curve-only flag of
    /// two non-dynamic properties; a dynamic property equals no other). The metadata is
    /// ignored. The `OpData` base's type comparison is [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `GradingHueCurveOpData::equals` (GradingHueCurveOpData.cpp:222-237 @ v2.5.2) and
    /// `OpData::equals` (src/OpenColorIO/Op.cpp:73-79).
    pub fn equals(&self, other: &GradingHueCurveOpData) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.direction == other.direction
            && self.style == other.style
            && self.rgb_to_hsy == other.rgb_to_hsy
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

/// A copy with its own dynamic property, holding the same curves (and knots) and dynamic state,
/// and the same style, RGB-to-HSY setting and direction: sharing happens when needed, with the
/// CPU op for instance.
///
/// Port of the copy constructor and `operator=` of `GradingHueCurveOpData`
/// (GradingHueCurveOpData.cpp:28-36, 62-80 @ v2.5.2), and `clone` (86-89). Upstream copies the
/// curves with the property's `setValue`, which validates and fits them again: copying data
/// whose curves don't validate, or don't fit, throws there. The port copies the curves and
/// their knots as they are. Such data comes only from a `setValue` that failed to fit its
/// curves (the constructors and setters validate and fit first), which raises before the data
/// is copied (building it from a transform validates it).
impl Clone for GradingHueCurveOpData {
    fn clone(&self) -> Self {
        let state = self.value.state().clone();
        let value = DynamicPropertyGradingHueCurveImpl::from_state(state, self.is_dynamic());
        GradingHueCurveOpData {
            metadata: self.metadata.clone(),
            style: self.style,
            value: Arc::new(value),
            rgb_to_hsy: self.rgb_to_hsy,
            direction: self.direction,
        }
    }
}

/// Port of `operator==(const GradingHueCurveOpData &, const GradingHueCurveOpData &)`
/// (GradingHueCurveOpData.cpp:239-242 @ v2.5.2).
impl PartialEq for GradingHueCurveOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "grading_hue_curve_op_data_tests.rs"]
mod tests;
