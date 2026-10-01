// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Exponent op, `ExponentOp`: part of a port of `src/OpenColorIO/ops/exponent/ExponentOp.h`
//! and `ExponentOp.cpp` @ v2.5.2, the op's behaviors and the functions that create it,
//! [`create_exponent_op`] and [`create_exponent_op_from_values`]. Its GPU writer is
//! `ocio-gpu`'s; `CreateExponentTransform` comes with the transforms (1.8).
//!
//! As for every family, the op is its data, [`OpData::Exponent`]: [`Op`]'s methods match on
//! it and call the methods here, `ExponentOp`'s overrides.

use std::sync::Arc;

use super::exponent_op_cpu::get_exponent_renderer;
use super::exponent_op_data::ExponentOpData;
use crate::exception::{Exception, Result};
use crate::math_utils::{is_scalar_equal_to_zero, is_vec_equal_to_one};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::TransformDirection;

impl ExponentOpData {
    /// A new Exponent op with the same exponents, and no metadata: upstream's `clone` builds
    /// the op from the values alone.
    ///
    /// Port of `ExponentOp::clone` (ExponentOp.cpp:180-183 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::Exponent(ExponentOpData::from_values(&self.exp4)))
    }

    /// Port of `ExponentOp::getInfo` (ExponentOp.cpp:189-192 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<ExponentOp>"
    }

    /// Whether `op` is an Exponent op.
    ///
    /// Port of `ExponentOp::isSameType` (ExponentOp.cpp:194-199 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::Exponent(_))
    }

    /// Never: "It is simpler to handle a pair of inverses by combining them and then removing
    /// the identity."
    ///
    /// Port of `ExponentOp::isInverse` (ExponentOp.cpp:201-207 @ v2.5.2).
    pub(crate) fn is_inverse(&self, _op: &Op) -> bool {
        false
    }

    /// Whether `op` is an Exponent op, which this one combines with.
    ///
    /// Port of `ExponentOp::canCombineWith` (ExponentOp.cpp:209-212 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, op: &Op) -> bool {
        self.is_same_type(op)
    }

    /// Appends to `ops` the Exponent op whose exponents are the products of this op's and
    /// `second_op`'s (in `double`), with their metadata combined, unless every product is 1.
    ///
    /// Port of `ExponentOp::combineWith` (ExponentOp.cpp:214-241 @ v2.5.2).
    pub(crate) fn combine_with(&self, ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op) {
            return Err(Exception::new(
                "ExponentOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        let OpData::Exponent(second) = &**second_op.data() else {
            unreachable!("can_combine_with found an Exponent op");
        };

        let combined = [
            self.exp4[0] * second.exp4[0],
            self.exp4[1] * second.exp4[1],
            self.exp4[2] * second.exp4[2],
            self.exp4[3] * second.exp4[3],
        ];

        if !is_vec_equal_to_one(&combined) {
            let mut combined_data = ExponentOpData::from_values(&combined);

            // Combine metadata.
            // TODO: May want to revisit how the metadata is set.
            let mut new_desc = self.get_format_metadata().clone();
            new_desc.combine(second.get_format_metadata())?;
            *combined_data.get_format_metadata_mut() = new_desc;

            ops.push_back(Op::new(OpData::Exponent(combined_data)));
        }
        Ok(())
    }

    /// The op's cache ID: `<ExponentOp `, the data's cache ID, `>`.
    ///
    /// Port of `ExponentOp::getCacheID` (ExponentOp.cpp:243-251 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<ExponentOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.push(b'>');
        cache_id
    }

    /// The renderer, the same with fast math or not.
    ///
    /// Port of `ExponentOp::getCPUOp` (ExponentOp.cpp:253-256 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Arc<dyn CpuOp> {
        get_exponent_renderer(self)
    }
}

/// Appends an Exponent op of the exponents `vec4`, R, G, B and A, in the direction
/// `direction` (see [`create_exponent_op`]).
///
/// Port of `CreateExponentOp(OpRcPtrVec &, const double(&)[4], TransformDirection)`
/// (ExponentOp.cpp:299-305 @ v2.5.2).
pub fn create_exponent_op_from_values(
    ops: &mut OpVec,
    vec4: &[f64; 4],
    direction: TransformDirection,
) -> Result<()> {
    create_exponent_op(ops, ExponentOpData::from_values(vec4), direction)
}

/// Appends an Exponent op: forward, one holding `exp_data`; inverse, one holding the inverse
/// exponents, `1 / e`, without `exp_data`'s metadata. An exponent that is 0 within 2 float
/// ULPs (`IsScalarEqualToZero`, which tests the `double` as a `float`) has no inverse: "Cannot
/// apply ExponentOp op, Cannot apply 0.0 exponent in the inverse.", and nothing is appended.
///
/// Upstream's forward op shares the caller's `ExponentOpDataRcPtr`; here the op owns the
/// data, which nothing changes afterwards (`Op`'s docs).
///
/// Port of `CreateExponentOp(OpRcPtrVec &, ExponentOpDataRcPtr &, TransformDirection)`
/// (ExponentOp.cpp:307-337 @ v2.5.2).
pub fn create_exponent_op(
    ops: &mut OpVec,
    exp_data: ExponentOpData,
    direction: TransformDirection,
) -> Result<()> {
    match direction {
        TransformDirection::Forward => {
            ops.push_back(Op::new(OpData::Exponent(exp_data)));
        }
        TransformDirection::Inverse => {
            let mut values = [0.0; 4];
            for (value, &exp) in values.iter_mut().zip(&exp_data.exp4) {
                if !is_scalar_equal_to_zero(exp) {
                    *value = 1.0 / exp;
                } else {
                    return Err(Exception::new(
                        "Cannot apply ExponentOp op, Cannot apply 0.0 exponent in the inverse.",
                    ));
                }
            }
            ops.push_back(Op::new(OpData::Exponent(ExponentOpData::from_values(
                &values,
            ))));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "exponent_op_tests.rs"]
mod tests;
