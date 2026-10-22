// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary op: part of a port of `src/OpenColorIO/ops/gradingprimary/
//! GradingPrimaryOp.h` and `GradingPrimaryOp.cpp` @ v2.5.2: the op's behaviors and
//! [`create_grading_primary_op`]. `BuildGradingPrimaryOp` and `CreateGradingPrimaryTransform`
//! work on the `GradingPrimaryTransform`, in `ocio`
//! (crates/ocio/src/transforms/grading_primary_transform.rs); `extractGpuShaderInfo` is the GPU
//! writer's (`ocio-gpu`, 5.2d).
//!
//! As for every family, the op is its data, [`OpData::GradingPrimary`]: [`Op`]'s methods match
//! on it and call the methods here, `GradingPrimaryOp`'s overrides.

use std::sync::Arc;

use super::grading_primary_op_cpu::get_grading_primary_cpu_renderer;
use super::grading_primary_op_data::GradingPrimaryOpData;
use crate::dynamic_property::DynamicPropertyRcPtr;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::{DynamicPropertyType, TransformDirection};

/// "Dynamic property type not supported by grading primary op." (GradingPrimaryOp.cpp:152-155,
/// 166-169, 175-178 @ v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by grading primary op.";

/// "Grading primary property is not dynamic." (GradingPrimaryOp.cpp:156-159, 170-173 @
/// v2.5.2).
const NOT_DYNAMIC: &str = "Grading primary property is not dynamic.";

impl GradingPrimaryOpData {
    /// A new op with a copy of the data (and its own dynamic property).
    ///
    /// Port of `GradingPrimaryOp::clone` (src/OpenColorIO/ops/gradingprimary/
    /// GradingPrimaryOp.cpp:75-79 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::GradingPrimary(self.clone()))
    }

    /// Port of `GradingPrimaryOp::getInfo` (GradingPrimaryOp.cpp:85-88 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<GradingPrimaryOp>"
    }

    /// Whether `op` is a GradingPrimary op.
    ///
    /// Port of `GradingPrimaryOp::isSameType` (GradingPrimaryOp.cpp:95-99 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::GradingPrimary(_))
    }

    /// Whether `op` is a GradingPrimary op whose data undoes this one's
    /// ([`GradingPrimaryOpData::is_inverse`]).
    ///
    /// Port of `GradingPrimaryOp::isInverse` (GradingPrimaryOp.cpp:101-108 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        if let OpData::GradingPrimary(other) = &**op.data() {
            self.is_inverse(other)
        } else {
            false
        }
    }

    /// Never.
    ///
    /// Port of `GradingPrimaryOp::canCombineWith` (GradingPrimaryOp.cpp:110-114 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, _op: &Op) -> bool {
        // TODO: In some cases this could be combined with itself or other ops.
        false
    }

    /// Always the error of an op that doesn't combine.
    ///
    /// Port of `GradingPrimaryOp::combineWith` (GradingPrimaryOp.cpp:116-123 @ v2.5.2).
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "GradingPrimaryOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        Ok(())
    }

    /// The op's cache ID: `<GradingPrimaryOp `, the data's cache ID, `>`.
    ///
    /// Port of `GradingPrimaryOp::getCacheID` (GradingPrimaryOp.cpp:125-134 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<GradingPrimaryOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// Port of `GradingPrimaryOp::hasDynamicProperty` (GradingPrimaryOp.cpp:141-148 @
    /// v2.5.2).
    pub(crate) fn has_dynamic_property_op(&self, type_: DynamicPropertyType) -> bool {
        if type_ != DynamicPropertyType::GradingPrimary {
            return false;
        }
        self.is_dynamic()
    }

    /// Port of `GradingPrimaryOp::getDynamicProperty` (GradingPrimaryOp.cpp:150-161 @
    /// v2.5.2).
    pub(crate) fn get_dynamic_property_op(
        &self,
        type_: DynamicPropertyType,
    ) -> Result<DynamicPropertyRcPtr> {
        if type_ != DynamicPropertyType::GradingPrimary {
            return Err(Exception::new(TYPE_NOT_SUPPORTED));
        }
        if !self.is_dynamic() {
            return Err(Exception::new(NOT_DYNAMIC));
        }
        Ok(self.get_dynamic_property())
    }

    /// Makes the op share `prop`. Only the overload for a GradingPrimary property is the op's
    /// own; the others are the `Op` defaults ([`Op::replace_dynamic_property`]).
    ///
    /// Port of `GradingPrimaryOp::replaceDynamicProperty` (GradingPrimaryOp.cpp:163-180 @
    /// v2.5.2).
    pub(crate) fn replace_dynamic_property_op(
        &mut self,
        type_: DynamicPropertyType,
        prop: &DynamicPropertyRcPtr,
    ) -> Result<()> {
        let DynamicPropertyRcPtr::GradingPrimary(prop) = prop else {
            return Err(crate::op::cannot_replace(prop));
        };
        if type_ != DynamicPropertyType::GradingPrimary {
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
    /// Port of `GradingPrimaryOp::getCPUOp` (GradingPrimaryOp.cpp:187-191 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Result<Arc<dyn CpuOp>> {
        get_grading_primary_cpu_renderer(self)
    }
}

/// Appends a GradingPrimary op holding `prim_data`, inverted when `direction` is inverse.
///
/// Upstream's op shares the caller's `GradingPrimaryOpDataRcPtr` when `direction` is forward;
/// here the op owns the data the caller passes (the caller passes a copy to keep its own).
///
/// Port of `CreateGradingPrimaryOp` (src/OpenColorIO/ops/gradingprimary/GradingPrimaryOp.cpp:
/// 208-219 @ v2.5.2).
pub fn create_grading_primary_op(
    ops: &mut OpVec,
    prim_data: GradingPrimaryOpData,
    direction: TransformDirection,
) {
    let prim = if direction == TransformDirection::Inverse {
        prim_data.inverse()
    } else {
        prim_data
    };
    ops.push_back(Op::new(OpData::GradingPrimary(prim)));
}

#[cfg(test)]
#[path = "grading_primary_op_tests.rs"]
mod tests;
