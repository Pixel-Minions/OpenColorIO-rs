// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingRGBCurve op: part of a port of `src/OpenColorIO/ops/gradingrgbcurve/
//! GradingRGBCurveOp.h` and `GradingRGBCurveOp.cpp` @ v2.5.2: the op's behaviors and
//! [`create_grading_rgb_curve_op`]. `BuildGradingRGBCurveOp` and
//! `CreateGradingRGBCurveTransform` work on the `GradingRGBCurveTransform` (Phase 3);
//! `extractGpuShaderInfo` is the GPU writer's (`ocio-gpu`, WP 2.6e).
//!
//! As for every family, the op is its data, [`OpData::GradingRgbCurve`]: [`Op`]'s methods
//! match on it and call the methods here, `GradingRGBCurveOp`'s overrides.

use std::sync::Arc;

use super::grading_rgb_curve_op_cpu::get_grading_rgb_curve_cpu_renderer;
use super::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use crate::dynamic_property::DynamicPropertyRcPtr;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::{DynamicPropertyType, TransformDirection};

/// "Dynamic property type not supported by grading rgb curve op." (GradingRGBCurveOp.cpp:
/// 148-151, 160-163, 172-175 @ v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by grading rgb curve op.";

/// "Grading rgb curve property is not dynamic." (GradingRGBCurveOp.cpp:152-155, 164-167 @
/// v2.5.2).
const NOT_DYNAMIC: &str = "Grading rgb curve property is not dynamic.";

impl GradingRgbCurveOpData {
    /// A new op with a copy of the data (and its own dynamic property).
    ///
    /// Port of `GradingRGBCurveOp::clone` (src/OpenColorIO/ops/gradingrgbcurve/
    /// GradingRGBCurveOp.cpp:72-76 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::GradingRgbCurve(self.clone()))
    }

    /// Port of `GradingRGBCurveOp::getInfo` (GradingRGBCurveOp.cpp:82-85 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<GradingRGBCurveOp>"
    }

    /// Whether `op` is a GradingRGBCurve op.
    ///
    /// Port of `GradingRGBCurveOp::isSameType` (GradingRGBCurveOp.cpp:92-96 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::GradingRgbCurve(_))
    }

    /// Whether `op` is a GradingRGBCurve op whose data undoes this one's
    /// ([`GradingRgbCurveOpData::is_inverse`]).
    ///
    /// Port of `GradingRGBCurveOp::isInverse` (GradingRGBCurveOp.cpp:98-105 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        if let OpData::GradingRgbCurve(other) = &**op.data() {
            self.is_inverse(other)
        } else {
            false
        }
    }

    /// Never.
    ///
    /// Port of `GradingRGBCurveOp::canCombineWith` (GradingRGBCurveOp.cpp:107-110 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, _op: &Op) -> bool {
        false
    }

    /// Always the error of an op that doesn't combine.
    ///
    /// Port of `GradingRGBCurveOp::combineWith` (GradingRGBCurveOp.cpp:112-119 @ v2.5.2).
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "GradingRGBCurveOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        Ok(())
    }

    /// The op's cache ID: `<GradingRGBCurveOp `, the data's cache ID, `>`.
    ///
    /// Port of `GradingRGBCurveOp::getCacheID` (GradingRGBCurveOp.cpp:121-130 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<GradingRGBCurveOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// Port of `GradingRGBCurveOp::hasDynamicProperty` (GradingRGBCurveOp.cpp:137-144 @
    /// v2.5.2).
    pub(crate) fn has_dynamic_property_op(&self, type_: DynamicPropertyType) -> bool {
        type_ == DynamicPropertyType::GradingRgbCurve && self.is_dynamic()
    }

    /// Port of `GradingRGBCurveOp::getDynamicProperty` (GradingRGBCurveOp.cpp:146-157 @
    /// v2.5.2).
    pub(crate) fn get_dynamic_property_op(
        &self,
        type_: DynamicPropertyType,
    ) -> Result<DynamicPropertyRcPtr> {
        if type_ != DynamicPropertyType::GradingRgbCurve {
            return Err(Exception::new(TYPE_NOT_SUPPORTED));
        }
        if !self.is_dynamic() {
            return Err(Exception::new(NOT_DYNAMIC));
        }
        Ok(self.get_dynamic_property())
    }

    /// Makes the op share `prop`. Only the overload for a GradingRGBCurve property is the
    /// op's own; the others are the `Op` defaults ([`Op::replace_dynamic_property`]).
    ///
    /// Port of `GradingRGBCurveOp::replaceDynamicProperty` (GradingRGBCurveOp.cpp:159-178 @
    /// v2.5.2).
    pub(crate) fn replace_dynamic_property_op(
        &mut self,
        type_: DynamicPropertyType,
        prop: &DynamicPropertyRcPtr,
    ) -> Result<()> {
        let DynamicPropertyRcPtr::GradingRgbCurve(prop) = prop else {
            return Err(crate::op::cannot_replace(prop));
        };
        if type_ != DynamicPropertyType::GradingRgbCurve {
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
    /// Port of `GradingRGBCurveOp::getCPUOp` (GradingRGBCurveOp.cpp:185-189 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Result<Arc<dyn CpuOp>> {
        get_grading_rgb_curve_cpu_renderer(self)
    }
}

/// Appends a GradingRGBCurve op holding `curve_data`, inverted when `direction` is inverse.
///
/// Upstream's op shares the caller's `GradingRGBCurveOpDataRcPtr` when `direction` is
/// forward; here the op owns the data the caller passes (the caller passes a copy to keep
/// its own).
///
/// Port of `CreateGradingRGBCurveOp` (src/OpenColorIO/ops/gradingrgbcurve/
/// GradingRGBCurveOp.cpp:204-215 @ v2.5.2).
pub fn create_grading_rgb_curve_op(
    ops: &mut OpVec,
    curve_data: GradingRgbCurveOpData,
    direction: TransformDirection,
) {
    let curve = if direction == TransformDirection::Inverse {
        curve_data.inverse()
    } else {
        curve_data
    };
    ops.push_back(Op::new(OpData::GradingRgbCurve(curve)));
}

#[cfg(test)]
#[path = "grading_rgb_curve_op_tests.rs"]
mod tests;
