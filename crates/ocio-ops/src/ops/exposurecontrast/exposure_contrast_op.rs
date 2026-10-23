// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast op: part of a port of `src/OpenColorIO/ops/exposurecontrast/
//! ExposureContrastOp.h` and `ExposureContrastOp.cpp` @ v2.5.2: the op's behaviors and
//! [`create_exposure_contrast_op`]. `BuildExposureContrastOp` and
//! `CreateExposureContrastTransform` work on the `ExposureContrastTransform`, in `ocio`
//! (crates/ocio/src/transforms/exposure_contrast_transform.rs);
//! `extractGpuShaderInfo` is the GPU writer's (`ocio-gpu`, 5.1c).
//!
//! As for every family, the op is its data, [`OpData::ExposureContrast`]: [`Op`]'s methods
//! match on it and call the methods here, `ExposureContrastOp`'s overrides.

use std::sync::Arc;

use super::exposure_contrast_op_cpu::get_exposure_contrast_cpu_renderer;
use super::exposure_contrast_op_data::ExposureContrastOpData;
use crate::dynamic_property::DynamicPropertyRcPtr;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::{DynamicPropertyType, TransformDirection};

impl ExposureContrastOpData {
    /// A new op with a copy of the data (and its own dynamic properties).
    ///
    /// Port of `ExposureContrastOp::clone` (src/OpenColorIO/ops/exposurecontrast/
    /// ExposureContrastOp.cpp:72-76 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::ExposureContrast(self.clone()))
    }

    /// Port of `ExposureContrastOp::getInfo` (ExposureContrastOp.cpp:82-85 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<ExposureContrastOp>"
    }

    /// Whether `op` is an ExposureContrast op.
    ///
    /// Port of `ExposureContrastOp::isSameType` (ExposureContrastOp.cpp:92-96 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::ExposureContrast(_))
    }

    /// Whether `op` is an ExposureContrast op whose data undoes this one's
    /// ([`ExposureContrastOpData::is_inverse`]).
    ///
    /// Port of `ExposureContrastOp::isInverse` (ExposureContrastOp.cpp:98-105 @ v2.5.2).
    pub(crate) fn is_inverse_op(&self, op: &Op) -> bool {
        if let OpData::ExposureContrast(other) = &**op.data() {
            self.is_inverse(other)
        } else {
            false
        }
    }

    /// Never.
    ///
    /// Port of `ExposureContrastOp::canCombineWith` (ExposureContrastOp.cpp:107-110 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, _op: &Op) -> bool {
        false
    }

    /// Always the error of an op that doesn't combine.
    ///
    /// Port of `ExposureContrastOp::combineWith` (ExposureContrastOp.cpp:112-119 @ v2.5.2).
    pub(crate) fn combine_with(&self, _ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "ExposureContrastOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        Ok(())
    }

    /// The op's cache ID: `<ExposureContrastOp `, the data's cache ID, `>`.
    ///
    /// Port of `ExposureContrastOp::getCacheID` (ExposureContrastOp.cpp:121-130 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<ExposureContrastOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// The renderer of the data, whatever the fast-math setting.
    ///
    /// Port of `ExposureContrastOp::getCPUOp` (ExposureContrastOp.cpp:132-136 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Arc<dyn CpuOp> {
        get_exposure_contrast_cpu_renderer(self)
    }

    /// Makes the op share `prop`. Only the overload for a double property is the op's own;
    /// the others are the `Op` defaults ([`Op::replace_dynamic_property`]).
    ///
    /// Port of `ExposureContrastOp::replaceDynamicProperty` (ExposureContrastOp.cpp:159-163 @
    /// v2.5.2).
    pub(crate) fn replace_dynamic_property_op(
        &mut self,
        type_: DynamicPropertyType,
        prop: &DynamicPropertyRcPtr,
    ) -> Result<()> {
        let DynamicPropertyRcPtr::Double(prop) = prop else {
            return Err(crate::op::cannot_replace(prop));
        };
        self.replace_dynamic_property(type_, Arc::clone(prop))
    }
}

/// Appends an ExposureContrast op holding `data`, inverted when `direction` is inverse.
///
/// Upstream's op shares the caller's `ExposureContrastOpDataRcPtr` when `direction` is
/// forward; here the op owns the data the caller passes (the caller passes a copy to keep its
/// own).
///
/// Port of `CreateExposureContrastOp` (src/OpenColorIO/ops/exposurecontrast/
/// ExposureContrastOp.cpp:175-193 @ v2.5.2).
pub fn create_exposure_contrast_op(
    ops: &mut OpVec,
    data: ExposureContrastOpData,
    direction: TransformDirection,
) {
    let data = match direction {
        TransformDirection::Forward => data,
        TransformDirection::Inverse => data.inverse(),
    };
    ops.push_back(Op::new(OpData::ExposureContrast(data)));
}

#[cfg(test)]
#[path = "exposure_contrast_op_tests.rs"]
mod tests;
