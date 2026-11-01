// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmspcs.c` @ lcms2.17: the PCS conversions that are reached, and the colorspace
//! tables at the end of the file.

use super::cmswtpnt::cms_d50_xyz;
use super::types::*;

/// Port of `cmsXYZ2xyY` (src/cmspcs.c:91-100 @ lcms2.17).
pub fn cms_xyz_2_xy_y(source: &CieXyz) -> CieXyY {
    let i_sum = 1. / (source.x + source.y + source.z);
    CieXyY {
        x: source.x * i_sum,
        y: source.y * i_sum,
        big_y: source.y,
    }
}

/// Port of `cmsxyY2XYZ` (src/cmspcs.c:102-107 @ lcms2.17).
pub fn cms_xy_y_2_xyz(source: &CieXyY) -> CieXyz {
    CieXyz {
        x: (source.x / source.y) * source.big_y,
        y: source.big_y,
        z: ((1.0 - source.x - source.y) / source.y) * source.big_y,
    }
}

/// Port of `f` (src/cmspcs.c:118-127 @ lcms2.17).
fn f(t: f64) -> f64 {
    let limit = (24.0 / 116.0) * (24.0 / 116.0) * (24.0 / 116.0);
    if t <= limit {
        (841.0 / 108.0) * t + (16.0 / 116.0)
    } else {
        t.powf(1.0 / 3.0)
    }
}

/// Port of `f_1` (src/cmspcs.c:129-139 @ lcms2.17).
fn f_1(t: f64) -> f64 {
    let limit = 24.0 / 116.0;
    if t <= limit {
        return (108.0 / 841.0) * (t - (16.0 / 116.0));
    }
    t * t * t
}

/// Port of `cmsXYZ2Lab` (src/cmspcs.c:143-157 @ lcms2.17), with the null white point: the only one
/// that is reached.
pub fn cms_xyz_2_lab(xyz: &CieXyz) -> CieLab {
    let white_point = cms_d50_xyz();
    let fx = f(xyz.x / white_point.x);
    let fy = f(xyz.y / white_point.y);
    let fz = f(xyz.z / white_point.z);
    CieLab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// Port of `cmsLab2XYZ` (src/cmspcs.c:161-176 @ lcms2.17), with the null white point.
pub fn cms_lab_2_xyz(lab: &CieLab) -> CieXyz {
    let white_point = cms_d50_xyz();
    let y = (lab.l + 16.0) / 116.0;
    let x = y + 0.002 * lab.a;
    let z = y - 0.005 * lab.b;
    CieXyz {
        x: f_1(x) * white_point.x,
        y: f_1(y) * white_point.y,
        z: f_1(z) * white_point.z,
    }
}

/// Port of `_cmsEndPointsBySpace` (src/cmspcs.c:707-756 @ lcms2.17), for its one reached question: the
/// black of an RGB profile, the only space `BlackPointAsDarkerColorant` is reached with.
pub fn cms_end_points_by_space_rgb_black() -> [u16; 3] {
    [0, 0, 0]
}

/// Port of `_cmsLCMScolorSpace` (src/cmspcs.c:810-874 @ lcms2.17).
pub fn cms_lcms_color_space(profile_space: u32) -> u32 {
    match profile_space {
        SIG_GRAY_DATA => PT_GRAY,
        SIG_RGB_DATA => PT_RGB,
        SIG_CMY_DATA => PT_CMY,
        SIG_CMYK_DATA => PT_CMYK,
        SIG_Y_CB_CR_DATA => PT_YCBCR,
        SIG_LUV_DATA => PT_YUV,
        SIG_XYZ_DATA => PT_XYZ,
        SIG_LAB_DATA => PT_LAB,
        SIG_LUV_K_DATA => PT_YUVK,
        SIG_HSV_DATA => PT_HSV,
        SIG_HLS_DATA => PT_HLS,
        SIG_YXY_DATA => PT_YXY,
        SIG_1COLOR_DATA | SIG_MCH1_DATA => PT_MCH1,
        SIG_2COLOR_DATA | SIG_MCH2_DATA => PT_MCH1 + 1,
        SIG_3COLOR_DATA | SIG_MCH3_DATA => PT_MCH1 + 2,
        SIG_4COLOR_DATA | SIG_MCH4_DATA => PT_MCH1 + 3,
        SIG_5COLOR_DATA | SIG_MCH5_DATA => PT_MCH1 + 4,
        SIG_6COLOR_DATA | SIG_MCH6_DATA => PT_MCH1 + 5,
        SIG_MCH7_DATA | SIG_7COLOR_DATA => PT_MCH1 + 6,
        SIG_MCH8_DATA | SIG_8COLOR_DATA => PT_MCH1 + 7,
        SIG_MCH9_DATA | SIG_9COLOR_DATA => PT_MCH1 + 8,
        SIG_MCHA_DATA | SIG_10COLOR_DATA => PT_MCH1 + 9,
        SIG_MCHB_DATA | SIG_11COLOR_DATA => PT_MCH1 + 10,
        SIG_MCHC_DATA | SIG_12COLOR_DATA => PT_MCH1 + 11,
        SIG_MCHD_DATA | SIG_13COLOR_DATA => PT_MCH1 + 12,
        SIG_MCHE_DATA | SIG_14COLOR_DATA => PT_MCH1 + 13,
        SIG_MCHF_DATA | SIG_15COLOR_DATA => PT_MCH1 + 14,
        _ => 0,
    }
}

/// Port of `cmsChannelsOfColorSpace` (src/cmspcs.c:877-940 @ lcms2.17).
pub fn cms_channels_of_color_space(color_space: u32) -> i32 {
    match color_space {
        SIG_MCH1_DATA | SIG_1COLOR_DATA | SIG_GRAY_DATA => 1,
        SIG_MCH2_DATA | SIG_2COLOR_DATA => 2,
        SIG_XYZ_DATA | SIG_LAB_DATA | SIG_LUV_DATA | SIG_Y_CB_CR_DATA | SIG_YXY_DATA
        | SIG_RGB_DATA | SIG_HSV_DATA | SIG_HLS_DATA | SIG_CMY_DATA | SIG_MCH3_DATA
        | SIG_3COLOR_DATA => 3,
        SIG_LUV_K_DATA | SIG_CMYK_DATA | SIG_MCH4_DATA | SIG_4COLOR_DATA => 4,
        SIG_MCH5_DATA | SIG_5COLOR_DATA => 5,
        SIG_MCH6_DATA | SIG_6COLOR_DATA => 6,
        SIG_MCH7_DATA | SIG_7COLOR_DATA => 7,
        SIG_MCH8_DATA | SIG_8COLOR_DATA => 8,
        SIG_MCH9_DATA | SIG_9COLOR_DATA => 9,
        SIG_MCHA_DATA | SIG_10COLOR_DATA => 10,
        SIG_MCHB_DATA | SIG_11COLOR_DATA => 11,
        SIG_MCHC_DATA | SIG_12COLOR_DATA => 12,
        SIG_MCHD_DATA | SIG_13COLOR_DATA => 13,
        SIG_MCHE_DATA | SIG_14COLOR_DATA => 14,
        SIG_MCHF_DATA | SIG_15COLOR_DATA => 15,
        _ => -1,
    }
}

/// Port of `cmsChannelsOf` (src/cmspcs.c:945-950 @ lcms2.17).
pub fn cms_channels_of(color_space: u32) -> u32 {
    let n = cms_channels_of_color_space(color_space);
    if n < 0 {
        return 3;
    }
    n as u32
}

#[cfg(test)]
#[path = "cmspcs_tests.rs"]
mod tests;
