// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! lcms2's own tests of the PCS conversions: `testbed/testcms2.c` @ lcms2.17.

use super::*;

/// `cmsDeltaE` (src/cmspcs.c:575-584 @ lcms2.17), which only these tests use.
fn cms_delta_e(lab1: &CieLab, lab2: &CieLab) -> f64 {
    let sqr = |v: f64| v * v;
    let dl = (lab1.l - lab2.l).abs();
    let da = (lab1.a - lab2.a).abs();
    let db = (lab1.b - lab2.b).abs();
    (sqr(dl) + sqr(da) + sqr(db)).powf(0.5)
}

/// The grid of Lab values of `CheckLab2XYZ` and `CheckLab2xyY`.
fn lab_grid() -> impl Iterator<Item = CieLab> {
    (0..=10).flat_map(|l| {
        (0..=32).flat_map(move |a| {
            (0..=32).map(move |b| CieLab {
                l: (l * 10) as f64,
                a: (-128 + a * 8) as f64,
                b: (-128 + b * 8) as f64,
            })
        })
    })
}

/// Port of `CheckLab2XYZ` (testbed/testcms2.c:2241-2267 @ lcms2.17).
#[test]
fn check_lab2xyz() {
    let mut max = 0f64;
    for lab in lab_grid() {
        let xyz = cms_lab_2_xyz(&lab);
        let lab2 = cms_xyz_2_lab(&xyz);
        max = max.max(cms_delta_e(&lab, &lab2));
    }
    assert!(max < 1E-12, "{max}");
}

/// Port of `CheckLab2xyY` (testbed/testcms2.c:2273-2303 @ lcms2.17).
#[test]
fn check_lab2xy_y() {
    let mut max = 0f64;
    for lab in lab_grid() {
        let xyz = cms_lab_2_xyz(&lab);
        let xy_y = cms_xyz_2_xy_y(&xyz);
        let xyz = cms_xy_y_2_xyz(&xy_y);
        let lab2 = cms_xyz_2_lab(&xyz);
        max = max.max(cms_delta_e(&lab, &lab2));
    }
    assert!(max < 1E-12, "{max}");
}
