// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/gamma/GammaOp_tests.cpp` @ v2.5.2: the tests of the op. Its test of
//! `CreateGammaTransform` (`create_transform`) needs the `ExponentTransform` and
//! `ExponentWithLinearTransform`: it is in crates/ocio/src/transforms/
//! exponent_with_linear_transform_tests.rs.

use super::*;
use crate::format_metadata::{METADATA_ID, METADATA_NAME};
use crate::op_data::OpDataType;
use crate::ops::gamma::gamma_op_data::{GammaStyle, Params, SHORT_PARAMS};

/// Port of `OCIO_ADD_TEST(GammaOp, combining)` @ v2.5.2.
#[test]
fn combining() {
    let mut ops = OpVec::new();

    let param1_r: Params = vec![1.201];
    let param1_g: Params = vec![1.201];
    let param1_b: Params = vec![1.201];
    let param1_a: Params = vec![1.];

    let mut gamma_data1 = GammaOpData::new(
        GammaStyle::BasicFwd,
        param1_r.clone(),
        param1_g.clone(),
        param1_b.clone(),
        param1_a.clone(),
    );

    let info1 = gamma_data1.get_format_metadata_mut();
    info1
        .add_attribute(Some(METADATA_NAME), Some(b"gamma1"))
        .unwrap();
    info1
        .add_attribute(Some(METADATA_ID), Some(b"ID1"))
        .unwrap();
    info1.add_attribute(Some(b"Attrib"), Some(b"1")).unwrap();
    info1.add_attribute(Some(b"Attrib1"), Some(b"10")).unwrap();
    info1
        .add_child_element(Some(b"Gamma1Child"), Some(b"Some content"))
        .unwrap();
    let child1 = info1.get_child_element(0).unwrap().clone();

    create_gamma_op(&mut ops, gamma_data1, TransformDirection::Forward);

    let param2_r: Params = vec![2.345];
    let param2_g: Params = vec![2.345];
    let param2_b: Params = vec![2.345];
    let param2_a: Params = vec![1.];

    let mut gamma_data2 = GammaOpData::new(
        GammaStyle::BasicFwd,
        param2_r.clone(),
        param2_g.clone(),
        param2_b.clone(),
        param2_a.clone(),
    );

    let info2 = gamma_data2.get_format_metadata_mut();
    info2
        .add_attribute(Some(METADATA_NAME), Some(b"gamma2"))
        .unwrap();
    info2
        .add_attribute(Some(METADATA_ID), Some(b"ID2"))
        .unwrap();
    info2.add_attribute(Some(b"Attrib"), Some(b"2")).unwrap();
    info2.add_attribute(Some(b"Attrib2"), Some(b"20")).unwrap();
    info2
        .add_child_element(Some(b"Gamma2Child"), Some(b"Other content"))
        .unwrap();
    let child2 = info2.get_child_element(0).unwrap().clone();

    create_gamma_op(&mut ops, gamma_data2, TransformDirection::Forward);

    assert_eq!(ops.len(), 2);
    let op0 = ops[0].clone();
    let op1 = ops[1].clone();

    assert!(op0.can_combine_with(&op1).unwrap());
    op0.combine_with(&mut ops, &op1).unwrap();

    assert_eq!(ops.len(), 3);
    let op2 = ops[2].clone();

    let combined_data = op2.data();

    // Check metadata of combined op.
    assert_eq!(combined_data.get_name(), b"gamma1 + gamma2");
    assert_eq!(combined_data.get_id(), b"ID1 + ID2");
    // 5 attributes: name, id, Attrib, Attrib1 and Attrib2.
    assert_eq!(combined_data.get_format_metadata().get_num_attributes(), 5);
    let attribs = combined_data.get_format_metadata().get_attributes();
    assert_eq!(attribs[2].0, b"Attrib");
    assert_eq!(attribs[2].1, b"1 + 2");
    assert_eq!(attribs[3].0, b"Attrib1");
    assert_eq!(attribs[3].1, b"10");
    assert_eq!(attribs[4].0, b"Attrib2");
    assert_eq!(attribs[4].1, b"20");
    let children = combined_data.get_format_metadata().get_children_elements();
    assert_eq!(children.len(), 2);
    assert!(children[0] == child1);
    assert!(children[1] == child2);

    assert_eq!(op2.data().get_type(), OpDataType::Gamma);

    let OpData::Gamma(g) = &**op2.data() else {
        panic!("a Gamma op")
    };

    assert_eq!(g.red_params()[0], param1_r[0] * param2_r[0]);
    assert_eq!(g.green_params()[0], param1_g[0] * param2_g[0]);
    assert_eq!(g.blue_params()[0], param1_b[0] * param2_b[0]);
    assert_eq!(g.alpha_params()[0], param1_a[0] * param2_a[0]);
}

/// Port of `OCIO_ADD_TEST(GammaOp, basic)` @ v2.5.2.
#[test]
fn basic() {
    let red_params: Params = vec![1.001];
    let green_params: Params = vec![1.];
    let blue_params: Params = vec![2.];
    let alpha_params: Params = vec![1.];

    let gamma1 = GammaOpData::new(
        GammaStyle::BasicFwd,
        red_params.clone(),
        green_params.clone(),
        blue_params.clone(),
        alpha_params.clone(),
    );
    let op0 = gamma_op(gamma1);

    assert_eq!(op0.data().get_type(), OpDataType::Gamma);
    let OpData::Gamma(gamma_data) = &**op0.data() else {
        panic!("a Gamma op")
    };
    assert_eq!(gamma_data.style(), GammaStyle::BasicFwd);
    assert!(red_params == *gamma_data.red_params());
    assert!(green_params == *gamma_data.green_params());
    assert!(blue_params == *gamma_data.blue_params());
    assert!(alpha_params == *gamma_data.alpha_params());

    // Test isInverse, see also OCIO_ADD_TEST(GammaOpData, is_inverse).
    let mut ops = OpVec::new();
    let gamma2 = GammaOpData::new(
        GammaStyle::BasicRev,
        red_params,
        green_params,
        blue_params,
        alpha_params,
    );
    create_gamma_op(&mut ops, gamma2, TransformDirection::Forward);

    assert_eq!(ops.len(), 1);
    let op1 = &ops[0];
    assert!(op0.is_inverse(op1));
}

/// Port of `OCIO_ADD_TEST(GammaOp, computed_identifier)` @ v2.5.2.
#[test]
fn computed_identifier() {
    let mut ops = OpVec::new();

    let red_params: Params = vec![1.001];
    let mut green_params: Params = vec![1.];
    let blue_params: Params = vec![1.];
    let alpha_params: Params = vec![1.];

    let gamma1 = GammaOpData::new(
        GammaStyle::BasicFwd,
        red_params.clone(),
        green_params.clone(),
        blue_params.clone(),
        alpha_params.clone(),
    );
    create_gamma_op(&mut ops, gamma1, TransformDirection::Forward);

    assert_eq!(ops.len(), 1);

    green_params[0] = 1.001;
    let gamma2 = GammaOpData::new(
        GammaStyle::BasicFwd,
        red_params.clone(),
        green_params.clone(),
        blue_params.clone(),
        alpha_params.clone(),
    );
    create_gamma_op(&mut ops, gamma2.clone(), TransformDirection::Forward);
    assert_eq!(ops.len(), 2);

    ops.validate().unwrap();

    let id0 = ops[0].get_cache_id().unwrap();
    let id1 = ops[1].get_cache_id().unwrap();
    assert!(id0 != id1);

    create_gamma_op(&mut ops, gamma2, TransformDirection::Forward);

    assert_eq!(ops.len(), 3);

    ops.validate().unwrap();

    let id2 = ops[2].get_cache_id().unwrap();
    assert!(id0 != id2);
    assert!(id1 == id2);

    let gamma3 = GammaOpData::new(
        GammaStyle::BasicRev,
        red_params,
        green_params,
        blue_params,
        alpha_params,
    );
    create_gamma_op(&mut ops, gamma3, TransformDirection::Forward);

    assert_eq!(ops.len(), 4);

    ops.validate().unwrap();

    let id3 = ops[3].get_cache_id().unwrap();
    assert!(id0 != id3);
    assert!(id1 != id3);
    assert!(id2 != id3);
}

// ---------------------------------------------------------------------------------------------
// The port's own checks.

/// `combineWith` refuses an op `canCombineWith` refuses, with upstream's message
/// (GammaOp.cpp:96-101 @ v2.5.2); an op of another type never combines.
#[test]
fn combine_with_needs_can_combine_with() {
    let basic = GammaOpData::new(GammaStyle::BasicFwd, vec![2.], vec![2.], vec![2.], vec![1.]);
    let moncurve = GammaOpData::new(
        GammaStyle::MoncurveFwd,
        vec![2., 0.1],
        vec![2., 0.1],
        vec![2., 0.1],
        vec![1., 0.],
    );
    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, basic, TransformDirection::Forward);
    create_gamma_op(&mut ops, moncurve, TransformDirection::Forward);
    crate::ops::matrix::matrix_op::create_matrix_op(
        &mut ops,
        crate::ops::matrix::MatrixOpData::new(),
        TransformDirection::Forward,
    );
    let (op0, op1, op2) = (ops[0].clone(), ops[1].clone(), ops[2].clone());
    for other in [&op1, &op2] {
        assert!(!op0.can_combine_with(other).unwrap());
        assert_eq!(
            op0.combine_with(&mut ops, other).unwrap_err().message(),
            "GammaOp: canCombineWith must be checked before calling combineWith."
        );
        assert!(!op0.is_inverse(other));
    }
    assert!(op0.is_same_type(&op1) && !op0.is_same_type(&op2));
}

/// `CreateGammaOp` inverts the data for an inverse direction and doesn't validate it
/// (GammaOp.cpp:134-145 @ v2.5.2): only `validate` refuses the parameters, and the queries
/// that would read past them return U-24's error.
#[test]
fn create_gamma_op_inverts_and_does_not_validate() {
    let mut ops = OpVec::new();
    let data = GammaOpData::new(
        GammaStyle::MoncurveMirrorFwd,
        vec![2.4, 0.055],
        vec![2.4, 0.055],
        vec![2.4, 0.055],
        vec![1., 0.],
    );
    create_gamma_op(&mut ops, data.clone(), TransformDirection::Inverse);
    let OpData::Gamma(inverted) = &**ops[0].data() else {
        panic!("a Gamma op")
    };
    assert!(inverted.equals(&data.inverse()));

    let short = GammaOpData::new(GammaStyle::BasicRev, vec![], vec![], vec![], vec![]);
    create_gamma_op(&mut ops, short, TransformDirection::Forward);
    assert_eq!(ops.len(), 2);
    let mut op = ops[1].clone();
    assert_eq!(
        op.validate().unwrap_err().message(),
        "GammaOp: Wrong number of parameters"
    );
    for result in [
        op.is_no_op().map(|_| ()),
        op.is_identity().map(|_| ()),
        op.get_cache_id().map(|_| ()),
        op.get_cpu_op(true).map(|_| ()),
        op.get_cpu_op(false).map(|_| ()),
    ] {
        assert_eq!(result.unwrap_err().message(), SHORT_PARAMS);
    }
}
