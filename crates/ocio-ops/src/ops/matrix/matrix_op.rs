// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op, `MatrixOffsetOp`: part of a port of `src/OpenColorIO/ops/matrix/MatrixOp.h`
//! and `MatrixOp.cpp` @ v2.5.2, what the CPU engine needs: the op's behaviors, and the two
//! functions that create ops from data, [`create_matrix_op`] and
//! [`create_identity_matrix_op`]. The factories from values (`CreateScaleOp`, `CreateFitOp`,
//! ...) and `CreateMatrixTransform` are chunk 1.3m3's.
//!
//! As for every family, the op is its data, [`OpData::Matrix`]: [`Op`]'s methods match on it
//! and call the methods here, `MatrixOffsetOp`'s overrides. Its `finalize` replaces the op's
//! data, so it is [`Op::finalize`]'s own arm.

use std::sync::Arc;

use super::matrix_op_cpu::get_matrix_renderer;
use super::matrix_op_data::MatrixOpData;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Op, OpVec};
use crate::op_data::OpData;
use crate::open_color_types::{TransformDirection, combine_transform_directions};

/// The error of a Matrix op that is still inverse where it must be forward
/// (src/OpenColorIO/ops/matrix/MatrixOp.cpp:127-142 @ v2.5.2).
const FINALIZE_FIRST: &str = "Op::finalize has to be called.";

impl MatrixOpData {
    /// A new Matrix op with a copy of the data.
    ///
    /// Port of `MatrixOffsetOp::clone` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:94-98 @
    /// v2.5.2).
    pub(crate) fn clone_op(&self) -> Op {
        Op::new(OpData::Matrix(self.clone()))
    }

    /// Port of `MatrixOffsetOp::getInfo` (MatrixOp.cpp:103-106 @ v2.5.2).
    pub(crate) fn get_info(&self) -> &'static str {
        "<MatrixOffsetOp>"
    }

    /// Whether `op` is a Matrix op: what upstream's `DynamicPtrCast` to `MatrixOffsetOp`
    /// finds.
    ///
    /// Port of `MatrixOffsetOp::isSameType` (MatrixOp.cpp:108-113 @ v2.5.2).
    pub(crate) fn is_same_type(&self, op: &Op) -> bool {
        matches!(**op.data(), OpData::Matrix(_))
    }

    /// Never: "It is simpler to handle a pair of inverses by combining them and then removing
    /// the identity."
    ///
    /// Port of `MatrixOffsetOp::isInverse` (MatrixOp.cpp:115-120 @ v2.5.2).
    pub(crate) fn is_inverse(&self, _op: &Op) -> bool {
        false
    }

    /// Whether `op` is a Matrix op, which this one combines with; "Op::finalize has to be
    /// called." if either is still inverse.
    ///
    /// Port of `MatrixOffsetOp::canCombineWith` (MatrixOp.cpp:122-145 @ v2.5.2).
    pub(crate) fn can_combine_with(&self, op: &Op) -> Result<bool> {
        // TODO: Could combine with certain ASC_CDL ops.
        if self.is_same_type(op) {
            if self.get_direction() == TransformDirection::Inverse {
                return Err(Exception::new(FINALIZE_FIRST));
            }
            if let OpData::Matrix(other_mat) = &**op.data()
                && other_mat.get_direction() == TransformDirection::Inverse
            {
                return Err(Exception::new(FINALIZE_FIRST));
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Appends to `ops` the Matrix op that does what this one then `second_op` does, the
    /// composition, unless it is a no-op.
    ///
    /// Port of `MatrixOffsetOp::combineWith` (MatrixOp.cpp:147-162 @ v2.5.2).
    pub(crate) fn combine_with(&self, ops: &mut OpVec, second_op: &Op) -> Result<()> {
        if !self.can_combine_with(second_op)? {
            return Err(Exception::new(
                "MatrixOffsetOp: canCombineWith must be checked before calling combineWith.",
            ));
        }
        let OpData::Matrix(this_data) = &**second_op.data() else {
            unreachable!("can_combine_with found a Matrix op");
        };
        let composed_mat = self.compose(this_data)?;
        if !composed_mat.is_no_op()? {
            create_matrix_op(ops, composed_mat, TransformDirection::Forward);
        }
        Ok(())
    }

    /// The op's cache ID: `<MatrixOffsetOp `, the data's cache ID, ` >`.
    ///
    /// Port of `MatrixOffsetOp::getCacheID` (MatrixOp.cpp:173-182 @ v2.5.2).
    pub(crate) fn get_op_cache_id(&self) -> Result<Vec<u8>> {
        let mut cache_id = b"<MatrixOffsetOp ".to_vec();
        cache_id.extend_from_slice(&self.get_cache_id()?);
        cache_id.extend_from_slice(b" >");
        Ok(cache_id)
    }

    /// The renderer for the matrix: "Op::finalize has to be called." while it is inverse.
    ///
    /// Port of `MatrixOffsetOp::getCPUOp` (MatrixOp.cpp:184-188 @ v2.5.2).
    pub(crate) fn get_cpu_op(&self) -> Result<Arc<dyn CpuOp>> {
        get_matrix_renderer(self)
    }
}

/// Appends a Matrix op holding `matrix`, in the direction of `matrix` combined with
/// `direction`: an inverse `direction` inverts it.
///
/// Upstream's op shares the caller's `MatrixOpDataRcPtr` when `direction` is forward; here the
/// op owns the data, which nothing changes afterwards (`Op`'s docs).
///
/// Port of `CreateMatrixOp(OpRcPtrVec &, MatrixOpDataRcPtr &, TransformDirection)`
/// (src/OpenColorIO/ops/matrix/MatrixOp.cpp:341-352 @ v2.5.2).
pub fn create_matrix_op(ops: &mut OpVec, matrix: MatrixOpData, direction: TransformDirection) {
    let mut mat = matrix;
    if direction == TransformDirection::Inverse {
        let new_dir = combine_transform_directions(mat.get_direction(), direction);
        mat.set_direction(new_dir);
    }

    ops.push_back(Op::new(OpData::Matrix(mat)));
}

/// Appends an identity Matrix op (the CPU processor's op when the optimizer leaves none).
///
/// Port of `CreateIdentityMatrixOp(OpRcPtrVec &)` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:
/// 354-359 @ v2.5.2).
pub fn create_identity_matrix_op(ops: &mut OpVec) {
    let mat = MatrixOpData::create_diagonal_matrix(1.0);

    ops.push_back(Op::new(OpData::Matrix(mat)));
}

#[cfg(test)]
#[path = "matrix_op_tests.rs"]
mod tests;
