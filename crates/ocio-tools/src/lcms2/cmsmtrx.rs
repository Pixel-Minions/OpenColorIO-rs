// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsmtrx.c` @ lcms2.17: the parts of the 3x3 matrix library that are reached.

use super::types::MATRIX_DET_TOLERANCE;

/// `cmsVEC3`.
pub type Vec3 = [f64; 3];
/// `cmsMAT3`.
pub type Mat3 = [Vec3; 3];

/// Port of `_cmsMAT3identity` (src/cmsmtrx.c:84-89 @ lcms2.17).
pub fn cms_mat3_identity() -> Mat3 {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

/// Port of `CloseEnough` (src/cmsmtrx.c:91-95 @ lcms2.17).
fn close_enough(a: f64, b: f64) -> bool {
    (b - a).abs() < (1.0 / 65535.0)
}

/// Port of `_cmsMAT3isIdentity` (src/cmsmtrx.c:98-110 @ lcms2.17).
pub fn cms_mat3_is_identity(a: &Mat3) -> bool {
    let identity = cms_mat3_identity();
    for i in 0..3 {
        for j in 0..3 {
            if !close_enough(a[i][j], identity[i][j]) {
                return false;
            }
        }
    }
    true
}

/// Port of `_cmsMAT3per` (src/cmsmtrx.c:114-124 @ lcms2.17).
pub fn cms_mat3_per(a: &Mat3, b: &Mat3) -> Mat3 {
    let rowcol = |i: usize, j: usize| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
    [
        [rowcol(0, 0), rowcol(0, 1), rowcol(0, 2)],
        [rowcol(1, 0), rowcol(1, 1), rowcol(1, 2)],
        [rowcol(2, 0), rowcol(2, 1), rowcol(2, 2)],
    ]
}

/// Port of `_cmsMAT3inverse` (src/cmsmtrx.c:129-152 @ lcms2.17).
pub fn cms_mat3_inverse(a: &Mat3) -> Option<Mat3> {
    let c0 = a[1][1] * a[2][2] - a[1][2] * a[2][1];
    let c1 = -a[1][0] * a[2][2] + a[1][2] * a[2][0];
    let c2 = a[1][0] * a[2][1] - a[1][1] * a[2][0];

    let det = a[0][0] * c0 + a[0][1] * c1 + a[0][2] * c2;

    if det.abs() < MATRIX_DET_TOLERANCE {
        return None; // singular matrix; can't invert
    }

    Some([
        [
            c0 / det,
            (a[0][2] * a[2][1] - a[0][1] * a[2][2]) / det,
            (a[0][1] * a[1][2] - a[0][2] * a[1][1]) / det,
        ],
        [
            c1 / det,
            (a[0][0] * a[2][2] - a[0][2] * a[2][0]) / det,
            (a[0][2] * a[1][0] - a[0][0] * a[1][2]) / det,
        ],
        [
            c2 / det,
            (a[0][1] * a[2][0] - a[0][0] * a[2][1]) / det,
            (a[0][0] * a[1][1] - a[0][1] * a[1][0]) / det,
        ],
    ])
}

/// Port of `_cmsMAT3eval` (src/cmsmtrx.c:169-174 @ lcms2.17).
pub fn cms_mat3_eval(a: &Mat3, v: &Vec3) -> Vec3 {
    [
        a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
        a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
        a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2],
    ]
}
