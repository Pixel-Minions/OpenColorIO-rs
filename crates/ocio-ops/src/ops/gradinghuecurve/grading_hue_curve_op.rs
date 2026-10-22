// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingHueCurve op: part of a port of
//! `src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOp.h` and `GradingHueCurveOp.cpp` @
//! v2.5.2: the op's behaviors and [`create_grading_hue_curve_op`]. `BuildGradingHueCurveOp`
//! and `CreateGradingHueCurveTransform` work on the `GradingHueCurveTransform` (5.5e);
//! `extractGpuShaderInfo` is the GPU writer's (`ocio-gpu`, 5.4c).
//!
//! As for every family, the op is its data, [`OpData::GradingHueCurve`]: [`Op`]'s methods match
//! on it and call the methods here, `GradingHueCurveOp`'s overrides.

use std::sync::Arc;

use super::grading_hue_curve_op_cpu::get_grading_hue_curve_cpu_renderer;
use super::grading_hue_curve_op_data::GradingHueCurveOpData;
use crate::dynamic_property::DynamicPropertyRcPtr;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::{DynamicPropertyType, TransformDirection};

/// "Dynamic property type not supported by hue curve op." (GradingHueCurveOp.cpp:151-154,
/// 166-169, 175-178 @ v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by hue curve op.";

/// "Hue curve property is not dynamic." (GradingHueCurveOp.cpp:155-158, 170-173 @ v2.5.2).
const NOT_DYNAMIC: &str = "Hue curve property is not dynamic.";

impl GradingHueCurveOpData {
    /// A new op with a copy of the data (and its own dynamic property).
    ///
    /// Port of `GradingHueCurveOp::clone` (src/OpenColorIO/ops/gradinghuecurve/
    /// GradingHueCurveOp.cpp:74-78 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::GradingHueCurve(self.clone()))
    }

    /// Port of `GradingHueCurveOp::getInfo` (GradingHueCurveOp.cpp:84-87 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<GradingHueCurveOp>"
    }

    /// Whether `op` is a GradingHueCurve op.
    ///
    /// Port of `GradingHueCurveOp::isSameType` (GradingHueCurveOp.cpp:94-98 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::GradingHueCurve(_))
    }

    /// Whether `op` is a GradingHueCurve op whose data undoes this one's
    /// ([`GradingHueCurveOpData::is_inverse`]).
    ///
    /// Port of `GradingHueCurveOp::isInverse` (GradingHueCurveOp.cpp:100-107 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        if let OpData::GradingHueCurve(other) = &**op.data() {
            self.is_inverse(other)
        } else {
            false
        }
    }

    /// Never.
    ///
    /// Port of `GradingHueCurveOp::canCombineWith` (GradingHueCurveOp.cpp:109-112 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, _op: &Op) -> bool {
        false
    }

    /// Always the error of an op that doesn't combine.
    ///
    /// Port of `GradingHueCurveOp::combineWith` (GradingHueCurveOp.cpp:114-121 @ v2.5.2).
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "GradingHueCurveOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        Ok(())
    }

    /// The op's cache ID: `<GradingHueCurveOp `, the data's cache ID, `>`.
    ///
    /// Port of `GradingHueCurveOp::getCacheID` (GradingHueCurveOp.cpp:123-132 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<GradingHueCurveOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// Port of `GradingHueCurveOp::hasDynamicProperty` (GradingHueCurveOp.cpp:139-147 @
    /// v2.5.2).
    pub(crate) fn has_dynamic_property_op(&self, type_: DynamicPropertyType) -> bool {
        if type_ != DynamicPropertyType::GradingHueCurve {
            return false;
        }
        self.is_dynamic()
    }

    /// Port of `GradingHueCurveOp::getDynamicProperty` (GradingHueCurveOp.cpp:149-161 @
    /// v2.5.2).
    pub(crate) fn get_dynamic_property_op(
        &self,
        type_: DynamicPropertyType,
    ) -> Result<DynamicPropertyRcPtr> {
        if type_ != DynamicPropertyType::GradingHueCurve {
            return Err(Exception::new(TYPE_NOT_SUPPORTED));
        }
        if !self.is_dynamic() {
            return Err(Exception::new(NOT_DYNAMIC));
        }
        Ok(self.get_dynamic_property())
    }

    /// Makes the op share `prop`. Only the overload for a GradingHueCurve property is the op's
    /// own; the others are the `Op` defaults ([`Op::replace_dynamic_property`]). Upstream's
    /// last check, a cast of the property to its class, can't fail here: the variant is the
    /// class.
    ///
    /// Port of `GradingHueCurveOp::replaceDynamicProperty` (GradingHueCurveOp.cpp:163-181 @
    /// v2.5.2).
    pub(crate) fn replace_dynamic_property_op(
        &mut self,
        type_: DynamicPropertyType,
        prop: &DynamicPropertyRcPtr,
    ) -> Result<()> {
        let DynamicPropertyRcPtr::GradingHueCurve(prop) = prop else {
            return Err(crate::op::cannot_replace(prop));
        };
        if type_ != DynamicPropertyType::GradingHueCurve {
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
    /// Port of `GradingHueCurveOp::getCPUOp` (GradingHueCurveOp.cpp:188-192 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Result<Arc<dyn CpuOp>> {
        get_grading_hue_curve_cpu_renderer(self)
    }
}

/// Appends a GradingHueCurve op holding `curve_data`, inverted when `direction` is inverse.
///
/// Upstream's op shares the caller's `GradingHueCurveOpDataRcPtr` when `direction` is forward;
/// here the op owns the data the caller passes (the caller passes a copy to keep its own).
///
/// Port of `CreateGradingHueCurveOp` (src/OpenColorIO/ops/gradinghuecurve/
/// GradingHueCurveOp.cpp:204-215 @ v2.5.2).
pub fn create_grading_hue_curve_op(
    ops: &mut OpVec,
    curve_data: GradingHueCurveOpData,
    direction: TransformDirection,
) {
    let curve = if direction == TransformDirection::Inverse {
        curve_data.inverse()
    } else {
        curve_data
    };
    ops.push_back(Op::new(OpData::GradingHueCurve(curve)));
}

#[cfg(test)]
#[path = "grading_hue_curve_op_tests.rs"]
mod tests;
