// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op, `MatrixOffsetOp`: a port of `src/OpenColorIO/ops/matrix/MatrixOp.h` and
//! `MatrixOp.cpp` @ v2.5.2: the op's behaviors, the functions that create ops from data
//! ([`create_matrix_op`], [`create_identity_matrix_op`]) and from values ([`create_scale_op`],
//! [`create_fit_op`], [`create_saturation_op`], [`create_min_max_op`], ...). Not here:
//! `CreateMatrixTransform` and `BuildMatrixOp`, which work on the `MatrixTransform` (in `ocio`,
//! crates/ocio/src/transforms/matrix_transform.rs), `extractGpuShaderInfo` (1.3m4), and the
//! `float` overloads of `CreateMatrixOffsetOp` and `CreateFitOp`, which MatrixOp.h declares and
//! nothing defines.
//!
//! `CreateFitOp` and `CreateSaturationOp` compute their matrices with `MatrixTransform::Fit` and
//! `MatrixTransform::Sat`, static functions of the transform (src/OpenColorIO/transforms/
//! MatrixTransform.cpp:162-245); the ops can't call the transform's crate, so they are ported
//! here, [`matrix_transform_fit`] and [`matrix_transform_sat`], which `ocio::MatrixTransform`'s
//! `fit` and `sat` return.
//!
//! As for every family, the op is its data, [`OpData::Matrix`]: [`Op`]'s methods match on it
//! and call the methods here, `MatrixOffsetOp`'s overrides. Its `finalize` replaces the op's
//! data, so it is [`Op::finalize`]'s own arm.

use std::sync::Arc;

use super::matrix_op_cpu::get_matrix_renderer;
use super::matrix_op_data::{MatrixArray, MatrixOpData};
use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::math_utils::{is_scalar_equal_to_zero, sse_add, sse_mul};
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

/// The matrix and offsets that map `[old_min4, old_max4]` to `[new_min4, new_max4]`, channel by
/// channel: "Cannot create Fit operator. Max value equals min value '<max>' in channel index
/// <i>." where a channel's old range is 0.
///
/// Port of `MatrixTransform::Fit` (src/OpenColorIO/transforms/MatrixTransform.cpp:162-188 @
/// v2.5.2), with both outputs asked for. Internal to the ops: public for `ocio::MatrixTransform`.
#[doc(hidden)]
pub fn matrix_transform_fit(
    old_min4: &[f64; 4],
    old_max4: &[f64; 4],
    new_min4: &[f64; 4],
    new_max4: &[f64; 4],
) -> Result<([f64; 16], [f64; 4])> {
    let mut m44 = [0.0; 16];
    let mut offset4 = [0.0; 4];

    for i in 0..4 {
        let denom = old_max4[i] - old_min4[i];
        if is_scalar_equal_to_zero(denom) {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Cannot create Fit operator. ");
            os.put_str("Max value equals min value '");
            os.put_f64(old_max4[i]);
            os.put_str("' in channel index ");
            os.put_i32(i as i32);
            os.put_str(".");
            return Err(Exception::new(os.into_bytes()));
        }

        m44[5 * i] = (new_max4[i] - new_min4[i]) / denom;
        // `newmin4[i]*oldmax4[i] - newmax4[i]*oldmin4[i]`. Where two NaNs meet in a product, the
        // product keeps its first operand's: the Windows wheel multiplies in the source's order,
        // the Linux wheel too but for `oldmax4[i]` first in the first product
        // (docs/improvements.md, I-74; seen through each wheel's `MatrixTransform.Fit`,
        // crates/ocio/tests/matrix_transform_oracle.rs).
        let new_min_old_max = if cfg!(target_os = "windows") {
            sse_mul(new_min4[i], old_max4[i])
        } else {
            sse_mul(old_max4[i], new_min4[i])
        };
        let new_max_old_min = sse_mul(new_max4[i], old_min4[i]);
        offset4[i] = (new_min_old_max - new_max_old_min) / denom;
    }
    Ok((m44, offset4))
}

/// The matrix that sets the saturation to `sat` around the luma of `luma_coef3`, without
/// offsets.
///
/// Port of `MatrixTransform::Sat` (src/OpenColorIO/transforms/MatrixTransform.cpp:210-245 @
/// v2.5.2), with both outputs asked for. Internal to the ops: public for `ocio::MatrixTransform`.
#[doc(hidden)]
pub fn matrix_transform_sat(sat: f64, luma_coef3: &[f64; 3]) -> ([f64; 16], [f64; 4]) {
    let mut m44 = [0.0; 16];

    // `(1. - sat) * lumaCoef3[i]`, and `+ sat` on the diagonal. Where two NaNs meet, a product
    // or a sum keeps its first operand's: the Windows wheel multiplies in the source's order,
    // the Linux wheel with the luma first, except on the last diagonal value, which it computes
    // as if in the source's order (or adds `sat` first: the two give the same bits). The sums
    // keep the source's order (docs/improvements.md, I-74; seen through each wheel's
    // `MatrixTransform.Sat`, crates/ocio/tests/matrix_transform_oracle.rs).
    let one_minus_sat = 1. - sat;
    let scaled = |luma: f64| {
        if cfg!(target_os = "windows") {
            sse_mul(one_minus_sat, luma)
        } else {
            sse_mul(luma, one_minus_sat)
        }
    };

    m44[0] = sse_add(scaled(luma_coef3[0]), sat);
    m44[1] = scaled(luma_coef3[1]);
    m44[2] = scaled(luma_coef3[2]);
    m44[3] = 0.0;

    m44[4] = scaled(luma_coef3[0]);
    m44[5] = sse_add(scaled(luma_coef3[1]), sat);
    m44[6] = scaled(luma_coef3[2]);
    m44[7] = 0.0;

    m44[8] = scaled(luma_coef3[0]);
    m44[9] = scaled(luma_coef3[1]);
    m44[10] = sse_add(sse_mul(one_minus_sat, luma_coef3[2]), sat);
    m44[11] = 0.0;

    m44[12] = 0.0;
    m44[13] = 0.0;
    m44[14] = 0.0;
    m44[15] = 1.0;

    (m44, [0.0; 4])
}

/// Appends a scale op: a diagonal matrix of `scale4`, without offsets.
///
/// Port of `CreateScaleOp` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:204-210 @ v2.5.2).
pub fn create_scale_op(ops: &mut OpVec, scale4: &[f64; 4], direction: TransformDirection) {
    const OFFSET4: [f64; 4] = [0., 0., 0., 0.];
    create_scale_offset_op(ops, scale4, &OFFSET4, direction);
}

/// Appends the matrix op of `m44`, without offsets.
///
/// Port of `CreateMatrixOp(OpRcPtrVec &, const double *, TransformDirection)`
/// (src/OpenColorIO/ops/matrix/MatrixOp.cpp:212-218 @ v2.5.2).
pub fn create_matrix_op_from_m44(ops: &mut OpVec, m44: &[f64; 16], direction: TransformDirection) {
    const OFFSET4: [f64; 4] = [0.0, 0.0, 0.0, 0.0];
    create_matrix_offset_op(ops, m44, &OFFSET4, direction);
}

/// Appends an offset op: the identity matrix with `offset4`.
///
/// Port of `CreateOffsetOp` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:220-226 @ v2.5.2).
pub fn create_offset_op(ops: &mut OpVec, offset4: &[f64; 4], direction: TransformDirection) {
    const SCALE4: [f64; 4] = [1.0, 1.0, 1.0, 1.0];
    create_scale_offset_op(ops, &SCALE4, offset4, direction);
}

/// Appends the op of a diagonal matrix of `scale4`, with `offset4`.
///
/// Port of `CreateScaleOffsetOp` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:228-241 @ v2.5.2).
pub fn create_scale_offset_op(
    ops: &mut OpVec,
    scale4: &[f64; 4],
    offset4: &[f64; 4],
    direction: TransformDirection,
) {
    let mut m44 = [0.0; 16];

    m44[0] = scale4[0];
    m44[5] = scale4[1];
    m44[10] = scale4[2];
    m44[15] = scale4[3];

    create_matrix_offset_op(ops, &m44, offset4, direction);
}

/// Appends a saturation op ([`matrix_transform_sat`]).
///
/// Port of `CreateSaturationOp` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:243-253 @ v2.5.2).
pub fn create_saturation_op(
    ops: &mut OpVec,
    sat: f64,
    luma_coef3: &[f64; 3],
    direction: TransformDirection,
) {
    let (matrix, offset) = matrix_transform_sat(sat, luma_coef3);

    create_matrix_offset_op(ops, &matrix, &offset, direction);
}

/// Appends the op of `m44` and `offset4`, in the direction `direction`: the data holds the
/// direction, and the op is created forward.
///
/// Port of `CreateMatrixOffsetOp(OpRcPtrVec &, const double *, const double *,
/// TransformDirection)` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:255-265 @ v2.5.2).
pub fn create_matrix_offset_op(
    ops: &mut OpVec,
    m44: &[f64; 16],
    offset4: &[f64; 4],
    direction: TransformDirection,
) {
    let mut mat = MatrixOpData::new();
    mat.set_rgba(m44);
    mat.set_rgba_offsets(offset4);
    mat.set_direction(direction);

    create_matrix_op(ops, mat, TransformDirection::Forward);
}

/// Appends the op that maps `[old_min4, old_max4]` to `[new_min4, new_max4]`
/// ([`matrix_transform_fit`]), or its error.
///
/// Port of `CreateFitOp(OpRcPtrVec &, const double *, ...)` (src/OpenColorIO/ops/matrix/
/// MatrixOp.cpp:267-280 @ v2.5.2).
pub fn create_fit_op(
    ops: &mut OpVec,
    old_min4: &[f64; 4],
    old_max4: &[f64; 4],
    new_min4: &[f64; 4],
    new_max4: &[f64; 4],
    direction: TransformDirection,
) -> Result<()> {
    let (matrix, offset) = matrix_transform_fit(old_min4, old_max4, new_min4, new_max4)?;

    create_matrix_offset_op(ops, &matrix, &offset, direction);
    Ok(())
}

/// Appends an identity matrix op in the direction `direction`.
///
/// Port of `CreateIdentityMatrixOp(OpRcPtrVec &, TransformDirection)` (src/OpenColorIO/ops/
/// matrix/MatrixOp.cpp:282-294 @ v2.5.2), and the `MatrixOffsetOp(m44, offset4, direction)`
/// constructor it uses (MatrixOp.cpp:79-88).
pub fn create_identity_matrix_op_with_direction(ops: &mut OpVec, direction: TransformDirection) {
    let mut matrix = [0.0; 16];
    matrix[0] = 1.0;
    matrix[5] = 1.0;
    matrix[10] = 1.0;
    matrix[15] = 1.0;
    let offset = [0.0, 0.0, 0.0, 0.0];

    // MatrixOffsetOp(m44, offset4, direction).
    let mut mat = MatrixOpData::with_direction(direction);
    mat.set_rgba(&matrix);
    mat.set_rgba_offsets(&offset);
    ops.push_back(Op::new(OpData::Matrix(mat)));
}

/// Appends the op that maps `[from_min3, from_max3]` to [0, 1] on RGB, unless it is the
/// identity: "CreateMinMaxOp: from_min and from_max must not be equal." where a channel's range
/// is 0.
///
/// Port of `CreateMinMaxOp(OpRcPtrVec &, const double *, const double *, TransformDirection)`
/// (src/OpenColorIO/ops/matrix/MatrixOp.cpp:296-321 @ v2.5.2).
pub fn create_min_max_op(
    ops: &mut OpVec,
    from_min3: &[f64; 3],
    from_max3: &[f64; 3],
    direction: TransformDirection,
) -> Result<()> {
    let mut scale4 = [1.0, 1.0, 1.0, 1.0];
    let mut offset4 = [0.0, 0.0, 0.0, 0.0];

    let mut something_to_do = false;
    for i in 0..3 {
        let range = from_max3[i] - from_min3[i];
        if range == 0.0 {
            return Err(Exception::new(
                "CreateMinMaxOp: from_min and from_max must not be equal.",
            ));
        }
        scale4[i] = 1.0 / range;
        offset4[i] = -from_min3[i] * scale4[i];
        something_to_do |= scale4[i] != 1.0 || offset4[i] != 0.0;
    }

    if something_to_do {
        create_scale_offset_op(ops, &scale4, &offset4, direction);
    }
    Ok(())
}

/// [`create_min_max_op`] with the same bounds on R, G and B, as `float`s.
///
/// Port of `CreateMinMaxOp(OpRcPtrVec &, float, float, TransformDirection)`
/// (src/OpenColorIO/ops/matrix/MatrixOp.cpp:323-331 @ v2.5.2).
pub fn create_min_max_op_f32(
    ops: &mut OpVec,
    from_min: f32,
    from_max: f32,
    direction: TransformDirection,
) -> Result<()> {
    let min = [f64::from(from_min); 3];
    let max = [f64::from(from_max); 3];
    create_min_max_op(ops, &min, &max, direction)
}

/// Appends the matrix op of `matrix`, without offsets, in the direction `direction`.
///
/// Port of `CreateMatrixOp(OpRcPtrVec &, MatrixOpData::MatrixArrayPtr &, TransformDirection)`
/// (src/OpenColorIO/ops/matrix/MatrixOp.cpp:333-339 @ v2.5.2).
pub fn create_matrix_op_from_array(
    ops: &mut OpVec,
    matrix: &MatrixArray,
    direction: TransformDirection,
) {
    let mat = MatrixOpData::from_array(matrix.clone());
    create_matrix_op(ops, mat, direction);
}

#[cfg(test)]
#[path = "matrix_op_tests.rs"]
mod tests;
