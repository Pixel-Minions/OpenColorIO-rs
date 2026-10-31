// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! lcms2's own tests of pipelines and of the interpolators through them:
//! `testbed/testcms2.c` @ lcms2.17.

use ocio_testkit::upstream::equal_with_abs_error;

use super::*;

/// Port of `Fn8D1` (testbed/testcms2.c @ lcms2.17).
fn fn_8d1(a: [u32; 8], m: u32) -> u16 {
    (a.iter().sum::<u32>() / m) as u16
}

/// Port of `Fn8D2` (testbed/testcms2.c @ lcms2.17).
fn fn_8d2(a: [u32; 8], m: u32) -> u16 {
    ((a[0] + 3 * a[1] + 3 * a[2] + a[3] + a[4] + a[5] + a[6] + a[7]) / (m + 4)) as u16
}

/// Port of `Fn8D3` (testbed/testcms2.c @ lcms2.17).
fn fn_8d3(a: [u32; 8], m: u32) -> u16 {
    ((3 * a[0] + 2 * a[1] + 3 * a[2] + a[3] + a[4] + a[5] + a[6] + a[7]) / (m + 5)) as u16
}

/// Port of `Sampler3D` to `Sampler8D` (testbed/testcms2.c @ lcms2.17): the same three functions of
/// `n` inputs.
fn sampler(n: usize, in_: &[u16], out: &mut [u16]) {
    let mut a = [0u32; 8];
    for (slot, value) in a.iter_mut().zip(&in_[..n]) {
        *slot = *value as u32;
    }
    out[0] = fn_8d1(a, n as u32);
    out[1] = fn_8d2(a, n as u32);
    out[2] = fn_8d3(a, n as u32);
}

/// `CheckOne3D` to `CheckOne8D` (testbed/testcms2.c @ lcms2.17).
fn check_one(lut: &Pipeline, n: usize, in_: &[u16]) {
    let mut out1 = [0u16; 3];
    let mut out2 = [0u16; 3];

    // This is the interpolated value
    cms_pipeline_eval16(in_, &mut out1, lut);

    // This is the real value
    sampler(n, in_, &mut out2);

    // Let's see the difference
    for channel in 0..3 {
        assert!(
            equal_with_abs_error(out1[channel] as f64, out2[channel] as f64, 2.0),
            "{in_:x?}: {out1:x?} and {out2:x?}"
        );
    }
}

/// A pipeline of one CLUT of `sampler`.
fn sampled_lut(n: usize, stage: Option<Stage>) -> Pipeline {
    let mut lut = cms_pipeline_alloc(n as u32, 3).unwrap();
    let mut mpe = stage;
    assert!(cms_stage_sample_clut16bit(
        mpe.as_mut(),
        |in_, out| {
            sampler(n, in_, out);
            true
        },
        0
    ));
    assert!(cms_pipeline_insert_stage(&mut lut, StageLoc::AtBegin, mpe));
    lut
}

/// The points of `Check3Dinterp` to `Check8DinterpGranular`, cut to `n` inputs.
fn check_points(lut: &Pipeline, n: usize) {
    const POINTS: [[u16; 8]; 8] = [
        [0, 0, 0, 0, 0, 0, 0, 0],
        [
            0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff, 0xffff,
        ],
        [
            0x8080, 0x8080, 0x8080, 0x8080, 0x1234, 0x1122, 0x0056, 0x0011,
        ],
        [
            0x0000, 0xFE00, 0x80FF, 0x8888, 0x8078, 0x2233, 0x0088, 0x2020,
        ],
        [
            0x1111, 0x2222, 0x3333, 0x4444, 0x1455, 0x3344, 0x1987, 0x4532,
        ],
        [
            0x0000, 0x0012, 0x0013, 0x0014, 0x2333, 0x4455, 0x9988, 0x1200,
        ],
        [
            0x3141, 0x1415, 0x1592, 0x9261, 0x4567, 0x5566, 0xfe56, 0x6666,
        ],
        [
            0xFF00, 0xFF01, 0xFF12, 0xFF13, 0xF344, 0x6677, 0xbabe, 0xface,
        ],
    ];
    for point in POINTS {
        check_one(lut, n, &point[..n]);
    }
}

/// Port of `Check3Dinterp` (testbed/testcms2.c:1972-1997 @ lcms2.17).
#[test]
fn check_3d_interp() {
    let lut = sampled_lut(3, cms_stage_alloc_clut16bit(9, 3, 3, None));
    check_points(&lut, 3);
}

/// Port of `Check3DinterpGranular` (testbed/testcms2.c:2000-2026 @ lcms2.17).
#[test]
fn check_3d_interp_granular() {
    let stage = cms_stage_alloc_clut16bit_granular(&[7, 8, 9], 3, 3, None);
    check_points(&sampled_lut(3, stage), 3);
}

/// Port of `Check4Dinterp` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_4d_interp() {
    let lut = sampled_lut(4, cms_stage_alloc_clut16bit(9, 4, 3, None));
    check_points(&lut, 4);
}

/// Port of `Check4DinterpGranular` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_4d_interp_granular() {
    let stage = cms_stage_alloc_clut16bit_granular(&[9, 8, 7, 6], 4, 3, None);
    check_points(&sampled_lut(4, stage), 4);
}

/// Port of `Check5DinterpGranular` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_5d_interp_granular() {
    let stage = cms_stage_alloc_clut16bit_granular(&[3, 2, 2, 2, 2], 5, 3, None);
    check_points(&sampled_lut(5, stage), 5);
}

/// Port of `Check6DinterpGranular` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_6d_interp_granular() {
    let stage = cms_stage_alloc_clut16bit_granular(&[4, 3, 3, 2, 2, 2], 6, 3, None);
    check_points(&sampled_lut(6, stage), 6);
}

/// Port of `Check7DinterpGranular` (testbed/testcms2.c:2130-2152 @ lcms2.17).
#[test]
fn check_7d_interp_granular() {
    let stage = cms_stage_alloc_clut16bit_granular(&[4, 3, 3, 2, 2, 2, 2], 7, 3, None);
    check_points(&sampled_lut(7, stage), 7);
}

/// Port of `Check8DinterpGranular` (testbed/testcms2.c:2155-2178 @ lcms2.17).
#[test]
fn check_8d_interp_granular() {
    let stage = cms_stage_alloc_clut16bit_granular(&[4, 3, 3, 2, 2, 2, 2, 2], 8, 3, None);
    check_points(&sampled_lut(8, stage), 8);
}

/// Port of `CheckLUTcreation` (testbed/testcms2.c:3119-3132 @ lcms2.17).
#[test]
fn check_lut_creation() {
    let lut = cms_pipeline_alloc(1, 1).unwrap();
    assert_eq!(lut.elements.len(), 0);
    assert_eq!(lut.clone().elements.len(), 0);
}

/// Port of `AddIdentityMatrix` (testbed/testcms2.c:3137-3144 @ lcms2.17).
fn add_identity_matrix(lut: &mut Pipeline) {
    const IDENTITY: [f64; 12] = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
    let stage = cms_stage_alloc_matrix(3, 3, &IDENTITY, None);
    assert!(cms_pipeline_insert_stage(lut, StageLoc::AtEnd, stage));
}

/// Port of `AddIdentityCLUTfloat` (testbed/testcms2.c:3148-3161 @ lcms2.17).
fn add_identity_clut_float(lut: &mut Pipeline) {
    const TABLE: [f32; 24] = [
        0., 0., 0., 0., 0., 1., 0., 1., 0., 0., 1., 1., 1., 0., 0., 1., 0., 1., 1., 1., 0., 1., 1.,
        1.,
    ];
    let stage = cms_stage_alloc_clut_float_granular(&[2; 15], 3, 3, Some(&TABLE));
    assert!(cms_pipeline_insert_stage(lut, StageLoc::AtEnd, stage));
}

/// Port of `AddIdentityCLUT16` (testbed/testcms2.c:3165-3178 @ lcms2.17).
fn add_identity_clut16(lut: &mut Pipeline) {
    const TABLE: [u16; 24] = [
        0, 0, 0, 0, 0, 0xffff, 0, 0xffff, 0, 0, 0xffff, 0xffff, 0xffff, 0, 0, 0xffff, 0, 0xffff,
        0xffff, 0xffff, 0, 0xffff, 0xffff, 0xffff,
    ];
    let stage = cms_stage_alloc_clut16bit(2, 3, 3, Some(&TABLE));
    assert!(cms_pipeline_insert_stage(lut, StageLoc::AtEnd, stage));
}

/// Port of `Add3GammaCurves` (testbed/testcms2.c:3182-3194 @ lcms2.17).
fn add_3_gamma_curves(lut: &mut Pipeline, curve: f64) {
    let id = cms_build_gamma(curve).unwrap();
    let id3 = [id.clone(), id.clone(), id];
    let stage = cms_stage_alloc_tone_curves(3, Some(&id3));
    assert!(cms_pipeline_insert_stage(lut, StageLoc::AtEnd, stage));
}

/// Port of `CheckFloatLUT` (testbed/testcms2.c:3197-3224 @ lcms2.17).
fn check_float_lut(lut: &Pipeline) {
    for j in 0..65535i32 {
        let v = j as f32 / 65535.0f32;
        let in_f = [v, v, v];
        let mut out_f = [0f32; 3];
        cms_pipeline_eval_float(&in_f, &mut out_f, lut);

        for out in out_f {
            assert_eq!((out as f64 * 65535.0 + 0.5).floor() as i32, j);
        }
    }
}

/// Port of `Check16LUT` (testbed/testcms2.c:3227-3254 @ lcms2.17).
fn check_16_lut(lut: &Pipeline) {
    for j in 0..65535i32 {
        let v = j as u16;
        let mut out_w = [0u16; 3];
        cms_pipeline_eval16(&[v, v, v], &mut out_w, lut);

        assert_eq!(out_w, [v, v, v]);
    }
}

/// Port of `CheckStagesLUT` (testbed/testcms2.c:3258-3267 @ lcms2.17).
fn check_stages_lut(lut: &Pipeline, expected_stages: usize) {
    assert_eq!(lut.input_channels, 3);
    assert_eq!(lut.output_channels, 3);
    assert_eq!(lut.elements.len(), expected_stages);
}

/// Port of `CheckFullLUT` (testbed/testcms2.c:3271-3279 @ lcms2.17).
fn check_full_lut(lut: &Pipeline, expected_stages: usize) {
    check_stages_lut(lut, expected_stages);
    check_16_lut(lut);
    check_float_lut(lut);
}

/// Port of `Check1StageLUT` (testbed/testcms2.c:3301-3307 @ lcms2.17).
#[test]
fn check_1_stage_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    check_full_lut(&lut, 1);
}

/// Port of `Check2StageLUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_2_stage_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut_float(&mut lut);
    check_full_lut(&lut, 2);
}

/// Port of `Check2Stage16LUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_2_stage_16_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut16(&mut lut);
    check_full_lut(&lut, 2);
}

/// Port of `Check3StageLUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_3_stage_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut_float(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    check_full_lut(&lut, 3);
}

/// Port of `Check3Stage16LUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_3_stage_16_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut16(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    check_full_lut(&lut, 3);
}

/// Port of `Check4StageLUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_4_stage_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut_float(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_matrix(&mut lut);
    check_full_lut(&lut, 4);
}

/// Port of `Check4Stage16LUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_4_stage_16_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut16(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_matrix(&mut lut);
    check_full_lut(&lut, 4);
}

/// Port of `Check5StageLUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_5_stage_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut_float(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_matrix(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    check_full_lut(&lut, 5);
}

/// Port of `Check5Stage16LUT` (testbed/testcms2.c @ lcms2.17).
#[test]
fn check_5_stage_16_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_identity_clut16(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_matrix(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    check_full_lut(&lut, 5);
}

/// Port of `Check6StageLUT` (testbed/testcms2.c:3417-3429 @ lcms2.17).
#[test]
fn check_6_stage_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_clut_float(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_matrix(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    check_full_lut(&lut, 6);
}

/// Port of `Check6Stage16LUT` (testbed/testcms2.c:3432-3444 @ lcms2.17).
#[test]
fn check_6_stage_16_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    add_identity_matrix(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_clut16(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    add_identity_matrix(&mut lut);
    add_3_gamma_curves(&mut lut, 1.0);
    check_full_lut(&lut, 6);
}

/// Port of `CheckLab2LabLUT` (testbed/testcms2.c:3448-3460 @ lcms2.17).
#[test]
fn check_lab2lab_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    assert!(cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        Some(cms_stage_alloc_lab2xyz())
    ));
    assert!(cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        Some(cms_stage_alloc_xyz2lab())
    ));
    check_float_lut(&lut);
    check_stages_lut(&lut, 2);
}

/// Port of `CheckXYZ2XYZLUT` (testbed/testcms2.c:3465-3477 @ lcms2.17).
#[test]
fn check_xyz2xyz_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    assert!(cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        Some(cms_stage_alloc_xyz2lab())
    ));
    assert!(cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        Some(cms_stage_alloc_lab2xyz())
    ));
    check_float_lut(&lut);
    check_stages_lut(&lut, 2);
}

/// Port of `CheckLab2LabMatLUT` (testbed/testcms2.c:3483-3495 @ lcms2.17).
#[test]
fn check_lab2lab_mat_lut() {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    assert!(cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        Some(cms_stage_alloc_lab2xyz())
    ));
    add_identity_matrix(&mut lut);
    assert!(cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        Some(cms_stage_alloc_xyz2lab())
    ));
    check_float_lut(&lut);
    check_stages_lut(&lut, 3);
}
