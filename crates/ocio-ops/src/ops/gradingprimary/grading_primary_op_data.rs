// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary op's data. A port of
//! `src/OpenColorIO/ops/gradingprimary/GradingPrimaryOpData.h` and `GradingPrimaryOpData.cpp`
//! @ v2.5.2.
//!
//! The data is a style and a dynamic property, which holds the values, the direction and their
//! pre-render form (`GradingPrimaryPreRender`): the op shares the property with its renderer
//! and the processors, so a set of the property's value reaches every one of them.

use std::sync::Arc;

use super::grading_primary::GradingPrimary;
use crate::cfmt::{Crt, OStringStream};
use crate::dynamic_property::{
    DynamicPropertyGradingPrimaryImpl, DynamicPropertyGradingPrimaryImplRcPtr, DynamicPropertyRcPtr,
};
use crate::exception::Result;
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{
    GradingStyle, TransformDirection, combine_transform_directions,
    get_inverse_transform_direction, grading_style_to_string, transform_direction_to_string,
};
use crate::ops::matrix::MatrixOpData;
use crate::ops::range::RangeOpData;

/// `DefaultValues::FLOAT_DECIMALS` (GradingPrimaryOpData.cpp:15-18 @ v2.5.2): the cache ID's
/// precision.
const FLOAT_DECIMALS: i64 = 7;

/// The GradingPrimary op's data: a style, and the values with their direction, held by a
/// dynamic property that the op shares with its renderer and the processors.
///
/// Port of `GradingPrimaryOpData` (src/OpenColorIO/ops/gradingprimary/GradingPrimaryOpData.h:
/// 22-73 @ v2.5.2).
#[derive(Debug)]
pub struct GradingPrimaryOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: GradingStyle,
    /// `m_value`: the values, the direction and the dynamic flag.
    value: DynamicPropertyGradingPrimaryImplRcPtr,
}

impl GradingPrimaryOpData {
    /// The default values of `style`, forward, not dynamic.
    ///
    /// Port of `GradingPrimaryOpData::GradingPrimaryOpData(GradingStyle)`
    /// (GradingPrimaryOpData.cpp:20-28 @ v2.5.2).
    pub fn new(style: GradingStyle) -> Self {
        GradingPrimaryOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            value: Arc::new(DynamicPropertyGradingPrimaryImpl::new(
                style,
                TransformDirection::Forward,
                &GradingPrimary::new(style),
                false,
            )),
        }
    }

    /// Checks the values for the style ([`GradingPrimary::validate`]). They should already
    /// be valid: the property validates each value it is given.
    ///
    /// Port of `GradingPrimaryOpData::validate` (GradingPrimaryOpData.cpp:69-73 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // This should already be valid.
        self.value.get_value().validate(self.style)
    }

    /// Port of `GradingPrimaryOpData::getType` (GradingPrimaryOpData.h:36 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::GradingPrimary
    }

    /// Port of `GradingPrimaryOpData::isNoOp` (GradingPrimaryOpData.cpp:75-78 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// Whether the op changes nothing: not dynamic, and the saturation, the clamps and the
    /// style's values are the style's defaults (the pivots are ignored but the log style's
    /// black and white ones). Exact comparisons: a NaN value is never a default.
    ///
    /// Port of `GradingPrimaryOpData::isIdentity` (GradingPrimaryOpData.cpp:80-126 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }

        let default_values = GradingPrimary::new(self.style);
        let values = self.value.get_value();

        if default_values.saturation == values.saturation
            && default_values.clamp_black == values.clamp_black
            && default_values.clamp_white == values.clamp_white
        {
            match self.style {
                GradingStyle::Log => {
                    if default_values.pivot_black == values.pivot_black
                        && default_values.pivot_white == values.pivot_white
                        && default_values.brightness == values.brightness
                        && default_values.contrast == values.contrast
                        && default_values.gamma == values.gamma
                    {
                        // Pivot value can be ignored if other values are identity.
                        return true;
                    }
                }
                GradingStyle::Lin => {
                    if default_values.contrast == values.contrast
                        && default_values.offset == values.offset
                        && default_values.exposure == values.exposure
                    {
                        // Pivot value can be ignored if other values are identity.
                        return true;
                    }
                }
                GradingStyle::Video => {
                    if default_values.gamma == values.gamma
                        && default_values.offset == values.offset
                        && default_values.lift == values.lift
                        && default_values.gain == values.gain
                    {
                        // PivotBlack/White value can be ignored if other values are identity.
                        return true;
                    }
                }
            }
        }
        false
    }

    /// The data of the op that replaces an identity: a Range op clamping to the clamps that
    /// aren't the no-clamp values (an empty bound for the others), or the identity matrix when
    /// neither clamps. The Range's validation can refuse the bounds.
    ///
    /// Port of `GradingPrimaryOpData::getIdentityReplacement` (GradingPrimaryOpData.cpp:
    /// 128-154 @ v2.5.2).
    pub fn get_identity_replacement(&self) -> Result<OpData> {
        let values = self.value.get_value();
        let mut clamp_low = values.clamp_black;
        let mut low_empty = false;
        let mut high_empty = false;
        if clamp_low == GradingPrimary::NO_CLAMP_BLACK {
            clamp_low = RangeOpData::empty_value();
            low_empty = true;
        }
        let mut clamp_high = values.clamp_white;
        if clamp_high == GradingPrimary::NO_CLAMP_WHITE {
            clamp_high = RangeOpData::empty_value();
            high_empty = true;
        }

        if low_empty && high_empty {
            return Ok(OpData::Matrix(MatrixOpData::new()));
        }
        Ok(OpData::Range(RangeOpData::with_values(
            clamp_low, clamp_high, clamp_low, clamp_high,
        )?))
    }

    /// Whether the saturation, which mixes the channels, is not 1.
    ///
    /// Port of `GradingPrimaryOpData::hasChannelCrosstalk` (GradingPrimaryOpData.cpp:156-160
    /// @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        self.value.get_value().saturation != 1.
    }

    /// Whether `r` undoes this op: neither is dynamic, the same style and values, and
    /// opposite directions.
    ///
    /// Port of `GradingPrimaryOpData::isInverse` (GradingPrimaryOpData.cpp:162-177 @ v2.5.2).
    pub fn is_inverse(&self, r: &GradingPrimaryOpData) -> bool {
        if self.is_dynamic() || r.is_dynamic() {
            return false;
        }

        self.style == r.style
            && self.value.equals(&r.value)
            && combine_transform_directions(self.get_direction(), r.get_direction())
                == TransformDirection::Inverse
    }

    /// A copy in the other direction.
    ///
    /// Port of `GradingPrimaryOpData::inverse` (GradingPrimaryOpData.cpp:179-185 @ v2.5.2).
    pub fn inverse(&self) -> GradingPrimaryOpData {
        let res = self.clone();
        let new_dir = get_inverse_transform_direction(self.get_direction());
        res.value.set_direction(new_dir);
        res
    }

    /// The ID (and a space) when there is one, the style, the direction, and the values with
    /// 7 significant digits unless the op is dynamic.
    ///
    /// Port of `GradingPrimaryOpData::getCacheID` (GradingPrimaryOpData.cpp:187-207 @ v2.5.2).
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
        if !self.is_dynamic() {
            self.value.get_value().write_to(&mut os);
        }
        cache_id.extend_from_slice(os.str());
        cache_id
    }

    /// Port of `GradingPrimaryOpData::getStyle` (GradingPrimaryOpData.h:49 @ v2.5.2).
    pub fn get_style(&self) -> GradingStyle {
        self.style
    }

    /// Changing the style resets the values to the new style's defaults.
    ///
    /// Port of `GradingPrimaryOpData::setStyle` (GradingPrimaryOpData.cpp:209-217 @ v2.5.2).
    pub fn set_style(&mut self, style: GradingStyle) {
        if style != self.style {
            self.style = style;
            // Reset value to default when style is changing.
            self.value.set_style(style);
        }
    }

    /// A copy of the values.
    ///
    /// Port of `GradingPrimaryOpData::getValue` (GradingPrimaryOpData.h:52 @ v2.5.2).
    pub fn get_value(&self) -> GradingPrimary {
        self.value.get_value()
    }

    /// Validates the values for the style and sets them (through the dynamic property, so a
    /// processor that shares it sees them).
    ///
    /// Port of `GradingPrimaryOpData::setValue` (GradingPrimaryOpData.h:53 @ v2.5.2).
    pub fn set_value(&self, values: &GradingPrimary) -> Result<()> {
        self.value.set_value(values)
    }

    /// Port of `GradingPrimaryOpData::getDirection` (GradingPrimaryOpData.cpp:219-222 @
    /// v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.value.get_direction()
    }

    /// Port of `GradingPrimaryOpData::setDirection` (GradingPrimaryOpData.cpp:224-227 @
    /// v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.value.set_direction(dir);
    }

    /// Port of `GradingPrimaryOpData::isDynamic` (GradingPrimaryOpData.cpp:229-232 @ v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    /// The dynamic property, shared.
    ///
    /// Port of `GradingPrimaryOpData::getDynamicProperty` (GradingPrimaryOpData.cpp:234-237 @
    /// v2.5.2).
    pub fn get_dynamic_property(&self) -> DynamicPropertyRcPtr {
        DynamicPropertyRcPtr::GradingPrimary(Arc::clone(&self.value))
    }

    /// Port of `GradingPrimaryOpData::replaceDynamicProperty` (GradingPrimaryOpData.cpp:
    /// 239-242 @ v2.5.2).
    pub fn replace_dynamic_property(&mut self, prop: DynamicPropertyGradingPrimaryImplRcPtr) {
        self.value = prop;
    }

    /// Makes the property non-dynamic.
    ///
    /// Port of `GradingPrimaryOpData::removeDynamicProperty` (GradingPrimaryOpData.cpp:
    /// 244-247 @ v2.5.2).
    pub fn remove_dynamic_property(&self) {
        self.value.make_non_dynamic();
    }

    /// The dynamic property, as its class.
    ///
    /// Port of `GradingPrimaryOpData::getDynamicPropertyInternal` (GradingPrimaryOpData.h:
    /// 63-66 @ v2.5.2).
    pub fn get_dynamic_property_internal(&self) -> DynamicPropertyGradingPrimaryImplRcPtr {
        Arc::clone(&self.value)
    }

    /// The same style, direction and property ([`DynamicPropertyGradingPrimaryImpl::equals`]:
    /// the values of two non-dynamic properties; a dynamic property equals no other). The
    /// metadata is ignored. The `OpData` base's shortcut for the same object stays. Its type
    /// comparison is [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `GradingPrimaryOpData::equals` (GradingPrimaryOpData.cpp:249-263 @ v2.5.2) and
    /// `OpData::equals` (src/OpenColorIO/Op.cpp:73-79).
    pub fn equals(&self, other: &GradingPrimaryOpData) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.style == other.style
            && self.get_direction() == other.get_direction()
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

/// A copy with its own dynamic property, holding the same direction, values (and pre-render
/// values) and dynamic state: sharing happens when needed, with the CPU op for instance.
///
/// Port of the copy constructor and `operator=` of `GradingPrimaryOpData`
/// (GradingPrimaryOpData.cpp:30-58 @ v2.5.2), and `clone` (64-67). Upstream copies the values
/// with the property's `setValue`, which validates them again and computes their pre-render
/// form; the port copies both as they are. Every value the property holds passed that
/// validation for the style (the constructor's defaults, `setValue`, `setStyle`'s defaults), and
/// the pre-render form depends only on the style, the direction and the values, so the copy is
/// the same.
impl Clone for GradingPrimaryOpData {
    fn clone(&self) -> Self {
        let state = *self.value.state();
        let value = DynamicPropertyGradingPrimaryImpl::from_state(state, self.is_dynamic());
        GradingPrimaryOpData {
            metadata: self.metadata.clone(),
            style: self.style,
            value: Arc::new(value),
        }
    }
}

/// Port of `operator==(const GradingPrimaryOpData &, const GradingPrimaryOpData &)`
/// (GradingPrimaryOpData.cpp:265-268 @ v2.5.2).
impl PartialEq for GradingPrimaryOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "grading_primary_op_data_tests.rs"]
mod tests;
