// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmswtpnt.c` @ lcms2.17: white points and chromatic adaptation.

use super::cmserr::cms_signal_error;
use super::cmsmtrx::{Mat3, Vec3, cms_mat3_eval, cms_mat3_inverse, cms_mat3_per};
use super::cmspcs::cms_xy_y_2_xyz;
use super::types::{
    CMS_D50X, CMS_D50Y, CMS_D50Z, CMS_ERROR_RANGE, CieXyY, CieXyYTriple, CieXyz,
    MATRIX_DET_TOLERANCE,
};

/// Port of `cmsD50_XYZ` (src/cmswtpnt.c:31-36 @ lcms2.17).
pub fn cms_d50_xyz() -> CieXyz {
    CieXyz {
        x: CMS_D50X,
        y: CMS_D50Y,
        z: CMS_D50Z,
    }
}

/// Port of `cmsWhitePointFromTemp` (src/cmswtpnt.c:48-91 @ lcms2.17). `None` is its `FALSE`, where
/// the caller's point is left as it was.
pub fn cms_white_point_from_temp(temp_k: f64) -> Option<CieXyY> {
    let t = temp_k;
    let t2 = t * t; // Square
    let t3 = t2 * t; // Cube

    // For correlated color temperature (T) between 4000K and 7000K:
    let x = if (4000. ..=7000.).contains(&t) {
        -4.6070 * (1E9 / t3) + 2.9678 * (1E6 / t2) + 0.09911 * (1E3 / t) + 0.244063
    } else if t > 7000.0 && t <= 25000.0 {
        // or for correlated color temperature (T) between 7000K and 25000K:
        -2.0064 * (1E9 / t3) + 1.9018 * (1E6 / t2) + 0.24748 * (1E3 / t) + 0.237040
    } else {
        cms_signal_error(CMS_ERROR_RANGE, b"cmsWhitePointFromTemp: invalid temp");
        return None;
    };

    // Obtain y(x)
    let y = -3.000 * (x * x) + 2.870 * x - 0.275;

    Some(CieXyY { x, y, big_y: 1.0 })
}

/// Port of `ComputeChromaticAdaptation` (src/cmswtpnt.c:190-232 @ lcms2.17).
fn compute_chromatic_adaptation(
    source_white_point: &CieXyz,
    dest_white_point: &CieXyz,
    chad: &Mat3,
) -> Option<Mat3> {
    let chad_inv = cms_mat3_inverse(chad)?;

    let cone_source_xyz: Vec3 = [
        source_white_point.x,
        source_white_point.y,
        source_white_point.z,
    ];
    let cone_dest_xyz: Vec3 = [dest_white_point.x, dest_white_point.y, dest_white_point.z];

    let cone_source_rgb = cms_mat3_eval(chad, &cone_source_xyz);
    let cone_dest_rgb = cms_mat3_eval(chad, &cone_dest_xyz);

    if cone_source_rgb[0].abs() < MATRIX_DET_TOLERANCE
        || cone_source_rgb[1].abs() < MATRIX_DET_TOLERANCE
        || cone_source_rgb[2].abs() < MATRIX_DET_TOLERANCE
    {
        return None;
    }

    // Build matrix
    let cone: Mat3 = [
        [cone_dest_rgb[0] / cone_source_rgb[0], 0.0, 0.0],
        [0.0, cone_dest_rgb[1] / cone_source_rgb[1], 0.0],
        [0.0, 0.0, cone_dest_rgb[2] / cone_source_rgb[2]],
    ];

    // Normalize
    let tmp = cms_mat3_per(&cone, chad);
    Some(cms_mat3_per(&chad_inv, &tmp))
}

/// Port of `_cmsAdaptationMatrix` (src/cmswtpnt.c:236-248 @ lcms2.17), with the Bradford cone matrix:
/// no caller that is reached gives another.
pub fn cms_adaptation_matrix(from_ill: &CieXyz, to_ill: &CieXyz) -> Option<Mat3> {
    const LAM_RIGG: Mat3 = [
        // Bradford matrix
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    compute_chromatic_adaptation(from_ill, to_ill, &LAM_RIGG)
}

/// Port of `_cmsAdaptMatrixToD50` (src/cmswtpnt.c:251-266 @ lcms2.17).
fn cms_adapt_matrix_to_d50(r: &mut Mat3, source_white_pt: &CieXyY) -> bool {
    let dn = cms_xy_y_2_xyz(source_white_pt);
    let Some(bradford) = cms_adaptation_matrix(&dn, &cms_d50_xyz()) else {
        return false;
    };
    let tmp = *r;
    *r = cms_mat3_per(&bradford, &tmp);
    true
}

/// Port of `_cmsBuildRGB2XYZtransferMatrix` (src/cmswtpnt.c:281-323 @ lcms2.17).
pub fn cms_build_rgb2xyz_transfer_matrix(white_pt: &CieXyY, primrs: &CieXyYTriple) -> Option<Mat3> {
    let xn = white_pt.x;
    let yn = white_pt.y;
    let xr = primrs.red.x;
    let yr = primrs.red.y;
    let xg = primrs.green.x;
    let yg = primrs.green.y;
    let xb = primrs.blue.x;
    let yb = primrs.blue.y;

    // Build Primaries matrix
    let primaries: Mat3 = [
        [xr, xg, xb],
        [yr, yg, yb],
        [1.0 - xr - yr, 1.0 - xg - yg, 1.0 - xb - yb],
    ];

    // Result = Primaries ^ (-1) inverse matrix
    let result = cms_mat3_inverse(&primaries)?;

    let white_point: Vec3 = [xn / yn, 1.0, (1.0 - xn - yn) / yn];

    // Across inverse primaries ...
    let coef = cms_mat3_eval(&result, &white_point);

    // Give us the Coefs, then I build transformation matrix
    let mut r: Mat3 = [
        [coef[0] * xr, coef[1] * xg, coef[2] * xb],
        [coef[0] * yr, coef[1] * yg, coef[2] * yb],
        [
            coef[0] * (1.0 - xr - yr),
            coef[1] * (1.0 - xg - yg),
            coef[2] * (1.0 - xb - yb),
        ],
    ];

    cms_adapt_matrix_to_d50(&mut r, white_pt).then_some(r)
}
