// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Range op: part of a port of `src/OpenColorIO/ops/range/RangeOp.h` and `RangeOp.cpp` @
//! v2.5.2, what the CPU engine needs: the op's behaviors, and the functions that create ops,
//! [`create_range_op`] and [`create_range_op_from_values`]. `BuildRangeOp` and
//! `CreateRangeTransform` work on the `RangeTransform`, and come with the transforms (WP 1.8);
//! `extractGpuShaderInfo` comes with the GPU writer (1.3r3).
//!
//! As for every family, the op is its data, [`OpData::Range`]: [`Op`]'s methods match on it
//! and call the methods here, `RangeOp`'s overrides. Its `finalize` replaces the op's data, so
//! it is [`Op::finalize`]'s own arm.

use std::sync::Arc;

use super::range_op_cpu::get_range_renderer;
use super::range_op_data::RangeOpData;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::{TransformDirection, combine_transform_directions};

/// The error of a Range op that is still inverse where it must be forward
/// (src/OpenColorIO/ops/range/RangeOp.cpp:109-112, 140-143 @ v2.5.2).
const FINALIZE_FIRST: &str = "Op::finalize has to be called.";

impl RangeOpData {
    /// A new Range op with a copy of the data.
    ///
    /// Port of `RangeOp::clone` (src/OpenColorIO/ops/range/RangeOp.cpp:70-74 @ v2.5.2).
    pub(crate) fn clone_op(&self) -> Result<Op> {
        range_op(self.clone())
    }

    /// Port of `RangeOp::getInfo` (RangeOp.cpp:80-83 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<RangeOp>"
    }

    /// Whether `op` is a Range op: what upstream's `DynamicPtrCast` to `RangeOp` finds.
    ///
    /// Port of `RangeOp::isSameType` (RangeOp.cpp:85-89 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::Range(_))
    }

    /// Never: "It is simpler to handle a pair of inverses by combining them and then removing
    /// the identity." A clamp can't be undone.
    ///
    /// Port of `RangeOp::isInverse` (RangeOp.cpp:91-98 @ v2.5.2).
    pub(crate) fn is_inverse(&self, _op: &Op) -> bool {
        false
    }

    /// Whether this op combines with `op2`: with another Range op, and, when this one is an
    /// identity, with a forward Lut1D that isn't a half-domain one or a forward Lut3D, which
    /// replaces it. "Op::finalize has to be called." if either Range op is still inverse. It
    /// validates this op's data first, which updates its scale and offset.
    ///
    /// Port of `RangeOp::canCombineWith` (RangeOp.cpp:100-148 @ v2.5.2). The Lut3D arm comes
    /// with its family; no op of that type exists before.
    pub(crate) fn can_combine_with(&self, op2: &Op) -> Result<bool> {
        let op_data2 = op2.data();
        let range1 = self;

        // Need to validate prior to calling isIdentity to make sure scale and offset are
        // updated.
        range1.validate()?;
        if range1.get_direction() == TransformDirection::Inverse {
            return Err(Exception::new(FINALIZE_FIRST));
        }

        match &**op_data2 {
            OpData::Range(range2) => {
                if range2.get_direction() == TransformDirection::Inverse {
                    return Err(Exception::new(FINALIZE_FIRST));
                }

                Ok(true)
            }
            // If op is LUT range op can be removed. Keep range for half domain LUT.
            OpData::Lut1D(lut) => Ok(range1.is_identity()
                && !lut.is_input_half_domain()
                && lut.get_direction() == TransformDirection::Forward),
            // `if (range1->isIdentity())`: the Lut3D type, whose op can replace an identity
            // range, comes with its family.
            OpData::Log(_) | OpData::Matrix(_) | OpData::Reference(_) | OpData::NoOp(_) => {
                Ok(false)
            }
        }
    }

    /// Appends to `ops` the Range op that does what this one then `second_op` does, the
    /// composition (`compose`); with a LUT, the LUT op itself.
    ///
    /// Port of `RangeOp::combineWith` (RangeOp.cpp:150-174 @ v2.5.2).
    pub(crate) fn combine_with(&self, ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op)? {
            return Err(Exception::new(
                "RangeOp: canCombineWith must be checked before calling combineWith.",
            ));
        }

        match &**second_op.data() {
            // Range + Range.
            OpData::Range(range2) => {
                let range1 = self;
                let res_range = range1.compose(range2)?;
                create_range_op(ops, res_range, TransformDirection::Forward)
            }
            // Avoid clone (we actually want to use the second op): here the op shares its
            // data. (The Lut3D type comes with its family.)
            OpData::Lut1D(_) => {
                ops.push_back(second_op.clone());
                Ok(())
            }
            OpData::Log(_) | OpData::Matrix(_) | OpData::Reference(_) | OpData::NoOp(_) => {
                unreachable!("can_combine_with accepts Range ops only")
            }
        }
    }

    /// The op's cache ID: `<RangeOp `, the data's cache ID, ` >`.
    ///
    /// Port of `RangeOp::getCacheID` (RangeOp.cpp:185-194 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Vec<u8> {
        let mut cache_id = b"<RangeOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id());
        cache_id.extend_from_slice(b" >");
        cache_id
    }

    /// The renderer for the range: "Op::finalize has to be called." while it is inverse.
    ///
    /// Port of `RangeOp::getCPUOp` (RangeOp.cpp:196-200 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Result<Arc<dyn CpuOp>> {
        get_range_renderer(self)
    }
}

/// A Range op holding `range`, which it validates.
///
/// Port of `RangeOp::RangeOp` (RangeOp.cpp:63-68 @ v2.5.2).
fn range_op(range: RangeOpData) -> Result<Op> {
    range.validate()?;
    Ok(Op::new(OpData::Range(range)))
}

/// Appends a Range op with these bounds, validated, in the direction `direction`.
///
/// Port of `CreateRangeOp(OpRcPtrVec &, double, double, double, double, TransformDirection)`
/// (src/OpenColorIO/ops/range/RangeOp.cpp:221-229 @ v2.5.2).
pub fn create_range_op_from_values(
    ops: &mut OpVec,
    min_in_value: f64,
    max_in_value: f64,
    min_out_value: f64,
    max_out_value: f64,
    direction: TransformDirection,
) -> Result<()> {
    let data = RangeOpData::with_values(min_in_value, max_in_value, min_out_value, max_out_value)?;

    create_range_op(ops, data, direction)
}

/// Appends a Range op holding `range_data`, in the direction of `range_data` combined with
/// `direction`: an inverse `direction` inverts it. The op validates the data.
///
/// Upstream's op shares the caller's `RangeOpDataRcPtr` when `direction` is forward; here the
/// op owns the data, which nothing changes afterwards (`Op`'s docs).
///
/// Port of `CreateRangeOp(OpRcPtrVec &, RangeOpDataRcPtr &, TransformDirection)`
/// (src/OpenColorIO/ops/range/RangeOp.cpp:231-242 @ v2.5.2).
pub fn create_range_op(
    ops: &mut OpVec,
    range_data: RangeOpData,
    direction: TransformDirection,
) -> Result<()> {
    let mut range = range_data;
    if direction == TransformDirection::Inverse {
        let new_dir = combine_transform_directions(range.get_direction(), direction);
        range.set_direction(new_dir);
    }

    ops.push_back(range_op(range)?);
    Ok(())
}

#[cfg(test)]
#[path = "range_op_tests.rs"]
mod tests;
