// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsopt.c` @ lcms2.17, as far as `cmsFLAGS_NOOPTIMIZE` lets a pipeline get: the
//! removal of no-ops in `PreOptimize`. Every transform the tool reaches, and those the black
//! point detection makes, carry that flag; the optimizers after it aren't reached.

use super::cmslut::{Pipeline, StageData, cms_stage_alloc_matrix};
use super::cmsmtrx::{Mat3, cms_mat3_identity, cms_mat3_per};
use super::types::*;

/// Port of `_Remove1Op` (src/cmsopt.c:118-135 @ lcms2.17).
fn remove_1_op(lut: &mut Pipeline, unary_op: u32) -> bool {
    let before = lut.elements.len();
    lut.elements.retain(|mpe| mpe.implements != unary_op);
    lut.elements.len() != before
}

/// Port of `_Remove2Op` (src/cmsopt.c:138-163 @ lcms2.17).
fn remove_2_op(lut: &mut Pipeline, op1: u32, op2: u32) -> bool {
    let mut any_opt = false;
    let mut pt1 = 0;

    while pt1 < lut.elements.len() {
        let pt2 = pt1 + 1;
        if pt2 >= lut.elements.len() {
            return any_opt;
        }

        if lut.elements[pt1].implements == op1 && lut.elements[pt2].implements == op2 {
            lut.elements.remove(pt2);
            lut.elements.remove(pt1);
            any_opt = true;
        } else {
            pt1 += 1;
        }
    }

    any_opt
}

/// Port of `CloseEnoughFloat` (src/cmsopt.c:166-170 @ lcms2.17).
fn close_enough_float(a: f64, b: f64) -> bool {
    (b - a).abs() < 0.00001f32 as f64
}

/// Port of `isFloatMatrixIdentity` (src/cmsopt.c:172-185 @ lcms2.17).
fn is_float_matrix_identity(a: &Mat3) -> bool {
    let identity = cms_mat3_identity();
    for i in 0..3 {
        for j in 0..3 {
            if !close_enough_float(a[i][j], identity[i][j]) {
                return false;
            }
        }
    }
    true
}

/// The first nine values of a matrix stage, as C casts them to a `cmsMAT3*`.
fn as_mat3(double: &[f64]) -> Mat3 {
    [
        [double[0], double[1], double[2]],
        [double[3], double[4], double[5]],
        [double[6], double[7], double[8]],
    ]
}

/// Port of `_MultiplyMatrix` (src/cmsopt.c:188-246 @ lcms2.17).
fn multiply_matrix(lut: &mut Pipeline) -> bool {
    let mut any_opt = false;
    let mut pt1 = 0;

    while pt1 < lut.elements.len() {
        let pt2 = pt1 + 1;
        if pt2 >= lut.elements.len() {
            return any_opt;
        }

        let (first, second) = (&lut.elements[pt1], &lut.elements[pt2]);
        if first.implements == SIG_MATRIX_ELEM_TYPE && second.implements == SIG_MATRIX_ELEM_TYPE {
            // Get both matrices
            let StageData::Matrix {
                double: m1,
                offset: offset1,
            } = &first.data
            else {
                return false;
            };
            let StageData::Matrix {
                double: m2,
                offset: offset2,
            } = &second.data
            else {
                return false;
            };

            // Input offset and output offset should be zero to use this optimization
            if offset1.is_some()
                || offset2.is_some()
                || first.input_channels != 3
                || first.output_channels != 3
                || second.input_channels != 3
                || second.output_channels != 3
            {
                return false;
            }

            // Multiply both matrices to get the result
            let res = cms_mat3_per(&as_mat3(m2), &as_mat3(m1));

            // Remove both matrices
            lut.elements.remove(pt2);
            lut.elements.remove(pt1);

            // Now what if the result is a plain identity?
            if !is_float_matrix_identity(&res) {
                // We can not get rid of full matrix
                let flat = [
                    res[0][0], res[0][1], res[0][2], res[1][0], res[1][1], res[1][2], res[2][0],
                    res[2][1], res[2][2],
                ];
                let Some(multmat) = cms_stage_alloc_matrix(3, 3, &flat, None) else {
                    return false; // Should never happen
                };

                // Recover the chain
                lut.elements.insert(pt1, multmat);
            }

            any_opt = true;
        } else {
            pt1 += 1;
        }
    }

    any_opt
}

/// Port of `PreOptimize` (src/cmsopt.c:251-289 @ lcms2.17).
fn pre_optimize(lut: &mut Pipeline) -> bool {
    let mut any_opt = false;

    loop {
        let mut opt = false;

        // Remove all identities
        opt |= remove_1_op(lut, SIG_IDENTITY_ELEM_TYPE);

        // Remove XYZ2Lab followed by Lab2XYZ
        opt |= remove_2_op(lut, SIG_XYZ2_LAB_ELEM_TYPE, SIG_LAB2_XYZ_ELEM_TYPE);

        // Remove Lab2XYZ followed by XYZ2Lab
        opt |= remove_2_op(lut, SIG_LAB2_XYZ_ELEM_TYPE, SIG_XYZ2_LAB_ELEM_TYPE);

        // Remove V4 to V2 followed by V2 to V4
        opt |= remove_2_op(lut, SIG_LAB_V4TO_V2, SIG_LAB_V2TO_V4);

        // Remove V2 to V4 followed by V4 to V2
        opt |= remove_2_op(lut, SIG_LAB_V2TO_V4, SIG_LAB_V4TO_V2);

        // Remove float pcs Lab conversions
        opt |= remove_2_op(lut, SIG_LAB2_FLOAT_PCS, SIG_FLOAT_PCS2_LAB);

        // Remove float pcs Lab conversions
        opt |= remove_2_op(lut, SIG_XYZ2_FLOAT_PCS, SIG_FLOAT_PCS2_XYZ);

        // Simplify matrix.
        opt |= multiply_matrix(lut);

        if opt {
            any_opt = true;
        } else {
            return any_opt;
        }
    }
}

/// Port of `_cmsOptimizePipeline` (src/cmsopt.c:1905-1975 @ lcms2.17) under `cmsFLAGS_NOOPTIMIZE`.
pub fn cms_optimize_pipeline(lut: &mut Pipeline) {
    // Anything to optimize?
    if lut.elements.is_empty() {
        lut.fast_identity16 = true;
        return;
    }

    // Try to get rid of identities and trivial conversions.
    pre_optimize(lut);

    // After removal do we end with an identity?
    if lut.elements.is_empty() {
        lut.fast_identity16 = true;
    }
}

#[cfg(test)]
#[path = "cmsopt_tests.rs"]
mod tests;
