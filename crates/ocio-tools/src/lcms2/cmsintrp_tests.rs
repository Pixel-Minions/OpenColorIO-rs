// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! lcms2's own tests of the interpolators: `testbed/testcms2.c` @ lcms2.17. The higher
//! dimensions go through a stage, in `cmslut_tests.rs`.

use ocio_testkit::upstream::equal_with_abs_error;

use super::*;

/// `FIXED_PRECISION_15_16` (testbed/testcms2.c:832 @ lcms2.17).
const FIXED_PRECISION_15_16: f64 = 1.0 / 65535.0;

/// Port of `BuildTable` (testbed/testcms2.c:1002-1012 @ lcms2.17).
fn build_table(n: usize, descending: bool) -> Vec<u16> {
    let mut tab = vec![0u16; n];
    for i in 0..n {
        let v = (65535.0 * i as f64) / (n - 1) as f64;
        tab[if descending { n - i - 1 } else { i }] = (v + 0.5).floor() as u16;
    }
    tab
}

/// Port of `Check1D` (testbed/testcms2.c:1020-1060 @ lcms2.17).
fn check_1d(n_nodes_to_check: usize, down: bool, max_err: i32) {
    let p =
        cms_compute_interp_params(n_nodes_to_check as u32, 1, 1, CMS_LERP_FLAGS_16BITS).unwrap();
    let tab = build_table(n_nodes_to_check, down);

    for i in 0..=0xffffu32 {
        let in_ = i as u16;
        let mut out = [0u16];
        p.lerp16(&tab, &[in_], &mut out);

        let out = if down { 0xffff - out[0] } else { out[0] };

        assert!(
            equal_with_abs_error(out as f64, in_ as f64, max_err as f64),
            "({n_nodes_to_check}p): must be {in_:x}, but is {out:x}"
        );
    }
}

/// Port of `Check1DLERP2` (testbed/testcms2.c:1063-1066 @ lcms2.17).
#[test]
fn check_1d_lerp2() {
    check_1d(2, false, 0);
}

/// Port of `Check1DLERP3` (testbed/testcms2.c:1070-1073 @ lcms2.17).
#[test]
fn check_1d_lerp3() {
    check_1d(3, false, 1);
}

/// Port of `Check1DLERP4` (testbed/testcms2.c:1077-1080 @ lcms2.17).
#[test]
fn check_1d_lerp4() {
    check_1d(4, false, 0);
}

/// Port of `Check1DLERP6` (testbed/testcms2.c:1083-1086 @ lcms2.17).
#[test]
fn check_1d_lerp6() {
    check_1d(6, false, 0);
}

/// Port of `Check1DLERP18` (testbed/testcms2.c:1089-1092 @ lcms2.17).
#[test]
fn check_1d_lerp18() {
    check_1d(18, false, 0);
}

/// Port of `Check1DLERP2Down` (testbed/testcms2.c:1096-1099 @ lcms2.17).
#[test]
fn check_1d_lerp2_down() {
    check_1d(2, true, 0);
}

/// Port of `Check1DLERP3Down` (testbed/testcms2.c:1103-1106 @ lcms2.17).
#[test]
fn check_1d_lerp3_down() {
    check_1d(3, true, 1);
}

/// Port of `Check1DLERP6Down` (testbed/testcms2.c:1109-1112 @ lcms2.17).
#[test]
fn check_1d_lerp6_down() {
    check_1d(6, true, 0);
}

/// Port of `Check1DLERP18Down` (testbed/testcms2.c:1115-1118 @ lcms2.17).
#[test]
fn check_1d_lerp18_down() {
    check_1d(18, true, 0);
}

/// `FloatTable` of `Check3DinterpolationFloatTetrahedral`
/// (testbed/testcms2.c:1166-1176 @ lcms2.17).
const FLOAT_TABLE: [f32; 24] = [
    0., 0., 0., // B=0,G=0,R=0
    0., 0., 0.25, // B=1,G=0,R=0
    0., 0.5, 0., // B=0,G=1,R=0
    0., 0.5, 0.25, // B=1,G=1,R=0
    1., 0., 0., // B=0,G=0,R=1
    1., 0., 0.25, // B=1,G=0,R=1
    1., 0.5, 0., // B=0,G=1,R=1
    1., 0.5, 0.25, // B=1,G=1,R=1
];

/// `Table` of `Check3DinterpolationTetrahedral16` (testbed/testcms2.c:1258-1268 @ lcms2.17).
const TABLE16: [u16; 24] = [
    0, 0, 0, //
    0, 0, 0xffff, //
    0, 0xffff, 0, //
    0, 0xffff, 0xffff, //
    0xffff, 0, 0, //
    0xffff, 0, 0xffff, //
    0xffff, 0xffff, 0, //
    0xffff, 0xffff, 0xffff,
];

/// What `Check3DinterpolationFloatTetrahedral` and `Check3DinterpolationFloatTrilinear` share.
fn check_3d_float(flags: u32) {
    let p = cms_compute_interp_params(2, 3, 3, flags).unwrap();

    for i in 0..0xffff {
        let v = i as f32 / 65535.0f32;
        let in_ = [v, v, v];
        let mut out = [0f32; 3];
        p.lerp_float(&FLOAT_TABLE, &in_, &mut out);

        assert!(equal_with_abs_error(
            out[0] as f64,
            in_[0] as f64,
            FIXED_PRECISION_15_16
        ));
        assert!(equal_with_abs_error(
            out[1] as f64,
            (in_[1] / 2.0) as f64,
            FIXED_PRECISION_15_16
        ));
        assert!(equal_with_abs_error(
            out[2] as f64,
            (in_[2] / 4.0) as f64,
            FIXED_PRECISION_15_16
        ));
    }
}

/// Port of `Check3DinterpolationFloatTetrahedral` (testbed/testcms2.c:1160-1203 @ lcms2.17).
#[test]
fn check_3d_interpolation_float_tetrahedral() {
    let p = cms_compute_interp_params(2, 3, 3, CMS_LERP_FLAGS_FLOAT).unwrap();
    assert_eq!(p.interpolator, Interpolator::Tetrahedral);
    check_3d_float(CMS_LERP_FLAGS_FLOAT);
}

/// Port of `Check3DinterpolationFloatTrilinear` (testbed/testcms2.c:1206-1249 @ lcms2.17).
#[test]
fn check_3d_interpolation_float_trilinear() {
    let flags = CMS_LERP_FLAGS_FLOAT | CMS_LERP_FLAGS_TRILINEAR;
    let p = cms_compute_interp_params(2, 3, 3, flags).unwrap();
    assert_eq!(p.interpolator, Interpolator::Trilinear);
    check_3d_float(flags);
}

/// What `Check3DinterpolationTetrahedral16` and `Check3DinterpolationTrilinear16` share.
fn check_3d_16(flags: u32) {
    let p = cms_compute_interp_params(2, 3, 3, flags).unwrap();

    for i in 0..0xffff {
        let v = i as u16;
        let mut out = [0u16; 3];
        p.lerp16(&TABLE16, &[v, v, v], &mut out);

        assert_eq!(out, [v, v, v]);
    }
}

/// Port of `Check3DinterpolationTetrahedral16` (testbed/testcms2.c:1252-1293 @ lcms2.17).
#[test]
fn check_3d_interpolation_tetrahedral16() {
    check_3d_16(CMS_LERP_FLAGS_16BITS);
}

/// Port of `Check3DinterpolationTrilinear16` (testbed/testcms2.c:1296-1337 @ lcms2.17).
#[test]
fn check_3d_interpolation_trilinear16() {
    check_3d_16(CMS_LERP_FLAGS_TRILINEAR);
}
