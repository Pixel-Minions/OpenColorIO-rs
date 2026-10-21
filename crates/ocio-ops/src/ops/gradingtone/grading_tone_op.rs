// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingTone op: part of a port of `src/OpenColorIO/ops/gradingtone/GradingToneOp.h` and
//! `GradingToneOp.cpp` @ v2.5.2: the op's behaviors and [`create_grading_tone_op`].
//! `BuildGradingToneOp` and `CreateGradingToneTransform` work on the `GradingToneTransform`
//! (5.5d); `extractGpuShaderInfo` is the GPU writer's (`ocio-gpu`, 5.3d, 5.3e).
//!
//! As for every family, the op is its data, [`OpData::GradingTone`]: [`Op`]'s methods match on
//! it and call the methods here, `GradingToneOp`'s overrides.

use std::sync::Arc;

use super::grading_tone_op_cpu::get_grading_tone_cpu_renderer;
use super::grading_tone_op_data::GradingToneOpData;
use crate::dynamic_property::DynamicPropertyRcPtr;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::{DynamicPropertyType, TransformDirection};

/// "Dynamic property type not supported by grading tone op." (GradingToneOp.cpp:151-154,
/// 165-168 @ v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by grading tone op.";

/// "Grading tone property is not dynamic." (GradingToneOp.cpp:155-158, 169-172 @ v2.5.2).
const NOT_DYNAMIC: &str = "Grading tone property is not dynamic.";

impl GradingToneOpData {
    /// A new op with a copy of the data (and its own dynamic property).
    ///
    /// Port of `GradingToneOp::clone` (src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp:75-79
    /// @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::GradingTone(self.clone()))
    }

    /// Port of `GradingToneOp::getInfo` (GradingToneOp.cpp:85-88 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<GradingToneOp>"
    }

    /// Whether `op` is a GradingTone op.
    ///
    /// Port of `GradingToneOp::isSameType` (GradingToneOp.cpp:95-99 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::GradingTone(_))
    }

    /// Whether `op` is a GradingTone op whose data undoes this one's
    /// ([`GradingToneOpData::is_inverse`]).
    ///
    /// Port of `GradingToneOp::isInverse` (GradingToneOp.cpp:101-108 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        if let OpData::GradingTone(other) = &**op.data() {
            self.is_inverse(other)
        } else {
            false
        }
    }

    /// Never.
    ///
    /// Port of `GradingToneOp::canCombineWith` (GradingToneOp.cpp:110-113 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, _op: &Op) -> bool {
        false
    }

    /// Always the error of an op that doesn't combine.
    ///
    /// Port of `GradingToneOp::combineWith` (GradingToneOp.cpp:115-122 @ v2.5.2).
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "GradingToneOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        Ok(())
    }

    /// The op's cache ID: `<GradingToneOp `, the data's cache ID, `>`.
    ///
    /// Port of `GradingToneOp::getCacheID` (GradingToneOp.cpp:124-133 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<GradingToneOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// Port of `GradingToneOp::hasDynamicProperty` (GradingToneOp.cpp:140-147 @ v2.5.2).
    pub(crate) fn has_dynamic_property_op(&self, type_: DynamicPropertyType) -> bool {
        if type_ != DynamicPropertyType::GradingTone {
            return false;
        }
        self.is_dynamic()
    }

    /// Port of `GradingToneOp::getDynamicProperty` (GradingToneOp.cpp:149-160 @ v2.5.2).
    pub(crate) fn get_dynamic_property_op(
        &self,
        type_: DynamicPropertyType,
    ) -> Result<DynamicPropertyRcPtr> {
        if type_ != DynamicPropertyType::GradingTone {
            return Err(Exception::new(TYPE_NOT_SUPPORTED));
        }
        if !self.is_dynamic() {
            return Err(Exception::new(NOT_DYNAMIC));
        }
        Ok(self.get_dynamic_property())
    }

    /// Makes the op share `prop`. Only the overload for a GradingTone property is the op's
    /// own; the others are the `Op` defaults ([`Op::replace_dynamic_property`]).
    ///
    /// Port of `GradingToneOp::replaceDynamicProperty` (GradingToneOp.cpp:162-174 @ v2.5.2).
    pub(crate) fn replace_dynamic_property_op(
        &mut self,
        type_: DynamicPropertyType,
        prop: &DynamicPropertyRcPtr,
    ) -> Result<()> {
        let DynamicPropertyRcPtr::GradingTone(prop) = prop else {
            return Err(crate::op::cannot_replace(prop));
        };
        if type_ != DynamicPropertyType::GradingTone {
            return Err(Exception::new(TYPE_NOT_SUPPORTED));
        }
        if !self.is_dynamic() {
            return Err(Exception::new(NOT_DYNAMIC));
        }
        self.replace_dynamic_property(Arc::clone(prop));
        Ok(())
    }

    /// The renderer of the data, whatever the fast-math setting.
    ///
    /// Port of `GradingToneOp::getCPUOp` (GradingToneOp.cpp:181-185 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Result<Arc<dyn CpuOp>> {
        get_grading_tone_cpu_renderer(self)
    }
}

/// Appends a GradingTone op holding `tone_data`, inverted when `direction` is inverse.
///
/// Upstream's op shares the caller's `GradingToneOpDataRcPtr` when `direction` is forward; here
/// the op owns the data the caller passes (the caller passes a copy to keep its own).
///
/// Port of `CreateGradingToneOp` (src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp:202-213 @
/// v2.5.2).
pub fn create_grading_tone_op(
    ops: &mut OpVec,
    tone_data: GradingToneOpData,
    direction: TransformDirection,
) {
    let tone = if direction == TransformDirection::Inverse {
        tone_data.inverse()
    } else {
        tone_data
    };
    ops.push_back(Op::new(OpData::GradingTone(tone)));
}

#[cfg(test)]
#[path = "grading_tone_op_tests.rs"]
mod tests;
