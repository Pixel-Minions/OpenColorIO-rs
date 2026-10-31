// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `PreOptimize` on pipelines no profile makes. A matrix of a profile that isn't a `lut16Type`'s
//! has offsets, so that `_MultiplyMatrix` stops there, and two of a `lut16Type` never meet: the
//! sweeps against the wheel don't get to the multiplication. `CheckMatrixSimplify` of
//! `testbed/testcms2.c` needs formatters that aren't reached, so these say what it says with
//! pipelines: the result of a simplification is the same, or nothing is left.

use super::*;
use crate::lcms2::cmslut::{
    StageLoc, cms_pipeline_alloc, cms_pipeline_eval_float, cms_pipeline_insert_stage,
    cms_stage_alloc_identity_curves, cms_stage_alloc_lab_v2_to_v4, cms_stage_alloc_lab_v4_to_v2,
};

fn with_stages(stages: Vec<Option<crate::lcms2::cmslut::Stage>>) -> Pipeline {
    let mut lut = cms_pipeline_alloc(3, 3).unwrap();
    for stage in stages {
        assert!(cms_pipeline_insert_stage(&mut lut, StageLoc::AtEnd, stage));
    }
    lut
}

/// Two matrices that don't commute, of whole numbers: the product is exact either way.
#[test]
fn matrices_multiply_in_the_order_of_the_stages() {
    let first = [1., 2., 0., 0., 1., 0., 0., 0., 1.];
    let second = [1., 0., 0., 3., 1., 0., 0., 0., 1.];
    let original = with_stages(vec![
        cms_stage_alloc_matrix(3, 3, &first, None),
        cms_stage_alloc_matrix(3, 3, &second, None),
    ]);
    let mut simplified = original.clone();
    cms_optimize_pipeline(&mut simplified);
    assert_eq!(simplified.elements.len(), 1);

    let input = [0.125f32, 0.25, 0.5];
    let mut expected = [0f32; 3];
    let mut got = [0f32; 3];
    cms_pipeline_eval_float(&input, &mut expected, &original);
    cms_pipeline_eval_float(&input, &mut got, &simplified);
    assert_eq!(expected.map(f32::to_bits), got.map(f32::to_bits));
}

#[test]
fn a_matrix_and_its_inverse_leave_nothing() {
    let first = [1., 2., 0., 0., 1., 0., 0., 0., 1.];
    let second = [1., -2., 0., 0., 1., 0., 0., 0., 1.];
    let mut lut = with_stages(vec![
        cms_stage_alloc_matrix(3, 3, &first, None),
        cms_stage_alloc_matrix(3, 3, &second, None),
    ]);
    cms_optimize_pipeline(&mut lut);
    assert!(lut.elements.is_empty());
    assert!(lut.fast_identity16);
}

#[test]
fn identities_and_lab_pairs_go() {
    let mut lut = with_stages(vec![
        cms_stage_alloc_lab_v4_to_v2(),
        cms_stage_alloc_identity_curves(3),
        cms_stage_alloc_lab_v2_to_v4(),
    ]);
    cms_optimize_pipeline(&mut lut);
    assert!(lut.elements.is_empty());

    let mut lut = with_stages(vec![
        cms_stage_alloc_lab_v2_to_v4(),
        cms_stage_alloc_lab_v4_to_v2(),
    ]);
    cms_optimize_pipeline(&mut lut);
    assert!(lut.elements.is_empty());
}
