// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingTone op's data. A port of `src/OpenColorIO/ops/gradingtone/GradingToneOpData.h`
//! and `GradingToneOpData.cpp` @ v2.5.2.
//!
//! The data is a style, a direction and a dynamic property, which holds the values and their
//! pre-render form (`GradingTonePreRender`): the op shares the property with its renderer and
//! the processors, so a set of the property's value reaches every one of them. Unlike the
//! GradingPrimary property, the GradingTone property doesn't hold the direction: the data does.

use std::sync::Arc;

use super::grading_tone::{GradingTone, is_identity};
use crate::cfmt::{Crt, OStringStream};
use crate::dynamic_property::{
    DynamicPropertyGradingToneImpl, DynamicPropertyGradingToneImplRcPtr, DynamicPropertyRcPtr,
};
use crate::exception::Result;
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::op_data::OpDataType;
use crate::open_color_types::{
    GradingStyle, TransformDirection, combine_transform_directions,
    get_inverse_transform_direction, grading_style_to_string, transform_direction_to_string,
};

/// `DefaultValues::FLOAT_DECIMALS` (GradingToneOpData.cpp:16-19 @ v2.5.2): the cache ID's
/// precision.
const FLOAT_DECIMALS: i64 = 7;

/// The GradingTone op's data: a style, a direction, and the values, held by a dynamic property
/// that the op shares with its renderer and the processors.
///
/// Port of `GradingToneOpData` (src/OpenColorIO/ops/gradingtone/GradingToneOpData.h:22-78 @
/// v2.5.2).
#[derive(Debug)]
pub struct GradingToneOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: GradingStyle,
    /// `m_value`: the values and the dynamic flag.
    value: DynamicPropertyGradingToneImplRcPtr,
    /// `m_direction`.
    direction: TransformDirection,
}

impl GradingToneOpData {
    /// The default values of `style`, forward, not dynamic.
    ///
    /// Port of `GradingToneOpData::GradingToneOpData(GradingStyle)` (GradingToneOpData.cpp:
    /// 21-26 @ v2.5.2).
    pub fn new(style: GradingStyle) -> Self {
        GradingToneOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            value: Arc::new(DynamicPropertyGradingToneImpl::new(
                &GradingTone::new(style),
                style,
                false,
            )),
            direction: TransformDirection::Forward,
        }
    }

    /// Checks the values ([`GradingTone::validate`]). They should already be valid: the
    /// property validates each value it is given.
    ///
    /// Port of `GradingToneOpData::validate` (GradingToneOpData.cpp:64-68 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        // This should already be valid.
        self.value.get_value().validate()
    }

    /// Port of `GradingToneOpData::getType` (GradingToneOpData.h:41 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::GradingTone
    }

    /// Port of `GradingToneOpData::isNoOp` (GradingToneOpData.cpp:70-73 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_identity()
    }

    /// Whether the op changes nothing: not dynamic, and every component of the values 1 and
    /// an s-contrast of 1 ([`is_identity`]); the starts and widths are ignored. Exact
    /// comparisons: a NaN value is never an identity.
    ///
    /// Port of `GradingToneOpData::isIdentity` (GradingToneOpData.cpp:75-82 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }

        let value = self.value.get_value();

        is_identity(&value)
    }

    /// Never: each channel is graded on its own, the master component on the three alike.
    ///
    /// Port of `GradingToneOpData::hasChannelCrosstalk` (GradingToneOpData.h:46 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        false
    }

    /// Whether `r` undoes this op: neither is dynamic, the same style and values, and
    /// opposite directions.
    ///
    /// Port of `GradingToneOpData::isInverse` (GradingToneOpData.cpp:84-99 @ v2.5.2).
    pub fn is_inverse(&self, r: &GradingToneOpData) -> bool {
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
    /// Port of `GradingToneOpData::inverse` (GradingToneOpData.cpp:101-106 @ v2.5.2).
    pub fn inverse(&self) -> GradingToneOpData {
        let mut res = self.clone();
        res.direction = get_inverse_transform_direction(self.direction);
        res
    }

    /// The ID (and a space) when there is one, the style, the direction, and the values with
    /// 7 significant digits unless the op is dynamic.
    ///
    /// Port of `GradingToneOpData::getCacheID` (GradingToneOpData.cpp:108-128 @ v2.5.2).
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

    /// Port of `GradingToneOpData::getStyle` (GradingToneOpData.h:53 @ v2.5.2).
    pub fn get_style(&self) -> GradingStyle {
        self.style
    }

    /// Changing the style resets the values to the new style's defaults.
    ///
    /// Port of `GradingToneOpData::setStyle` (GradingToneOpData.cpp:130-138 @ v2.5.2).
    pub fn set_style(&mut self, style: GradingStyle) {
        if self.style != style {
            self.style = style;
            // Reset value to default when style is changing.
            self.value.set_style(style);
        }
    }

    /// A copy of the values.
    ///
    /// Port of `GradingToneOpData::getValue` (GradingToneOpData.h:56 @ v2.5.2).
    pub fn get_value(&self) -> GradingTone {
        self.value.get_value()
    }

    /// Validates the values and sets them (through the dynamic property, so a processor that
    /// shares it sees them).
    ///
    /// Port of `GradingToneOpData::setValue` (GradingToneOpData.h:57 @ v2.5.2).
    pub fn set_value(&self, values: &GradingTone) -> Result<()> {
        self.value.set_value(values)
    }

    /// Port of `GradingToneOpData::getDirection` (GradingToneOpData.cpp:140-143 @ v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `GradingToneOpData::setDirection` (GradingToneOpData.cpp:145-148 @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// Port of `GradingToneOpData::isDynamic` (GradingToneOpData.cpp:150-153 @ v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.value.is_dynamic()
    }

    /// The dynamic property, shared.
    ///
    /// Port of `GradingToneOpData::getDynamicProperty` (GradingToneOpData.cpp:155-158 @
    /// v2.5.2).
    pub fn get_dynamic_property(&self) -> DynamicPropertyRcPtr {
        DynamicPropertyRcPtr::GradingTone(Arc::clone(&self.value))
    }

    /// Port of `GradingToneOpData::replaceDynamicProperty` (GradingToneOpData.cpp:160-163 @
    /// v2.5.2).
    pub fn replace_dynamic_property(&mut self, prop: DynamicPropertyGradingToneImplRcPtr) {
        self.value = prop;
    }

    /// Makes the property non-dynamic.
    ///
    /// Port of `GradingToneOpData::removeDynamicProperty` (GradingToneOpData.cpp:165-168 @
    /// v2.5.2).
    pub fn remove_dynamic_property(&self) {
        self.value.make_non_dynamic();
    }

    /// The dynamic property, as its class.
    ///
    /// Port of `GradingToneOpData::getDynamicPropertyInternal` (GradingToneOpData.h:67-70 @
    /// v2.5.2).
    pub fn get_dynamic_property_internal(&self) -> DynamicPropertyGradingToneImplRcPtr {
        Arc::clone(&self.value)
    }

    /// The same direction, style and property ([`DynamicPropertyGradingToneImpl::equals`]:
    /// the values of two non-dynamic properties; a dynamic property equals no other). The
    /// metadata is ignored. The `OpData` base's shortcut for the same object stays. Its type
    /// comparison is [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `GradingToneOpData::equals` (GradingToneOpData.cpp:170-184 @ v2.5.2) and
    /// `OpData::equals` (src/OpenColorIO/Op.cpp:73-79).
    pub fn equals(&self, other: &GradingToneOpData) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        self.direction == other.direction
            && self.style == other.style
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

/// A copy with its own dynamic property, holding the same values (and pre-render values) and
/// dynamic state, and the same style and direction: sharing happens when needed, with the CPU
/// op for instance.
///
/// Port of the copy constructor and `operator=` of `GradingToneOpData`
/// (GradingToneOpData.cpp:28-53 @ v2.5.2), and `clone` (59-62). Upstream copies the values with
/// the property's `setValue`, which validates them again and computes their pre-render form;
/// the port copies both as they are. Every value the property holds passed that validation (the
/// constructor's defaults, `setValue`, `setStyle`'s defaults), and the pre-render form depends
/// only on the style and the values, so the copy is the same.
impl Clone for GradingToneOpData {
    fn clone(&self) -> Self {
        let state = *self.value.state();
        let value = DynamicPropertyGradingToneImpl::from_state(state, self.is_dynamic());
        GradingToneOpData {
            metadata: self.metadata.clone(),
            style: self.style,
            value: Arc::new(value),
            direction: self.direction,
        }
    }
}

/// Port of `operator==(const GradingToneOpData &, const GradingToneOpData &)`
/// (GradingToneOpData.cpp:186-189 @ v2.5.2).
impl PartialEq for GradingToneOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "grading_tone_op_data_tests.rs"]
mod tests;
