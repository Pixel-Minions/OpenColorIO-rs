// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of upstream's tests/cpu/ops/lut1d/Lut1DOpData_tests.cpp @ v2.5.2 that read files
//! (`BuildOpsTest`). The others are `crates/ocio-ops/src/ops/lut1d/lut1d_op_data_tests.rs`.

use ocio_ops::op::OpVec;
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::{BitDepth, TransformDirection};
use ocio_ops::ops::lut1d::lut1d_op_data::{Lut1DOpData, make_fast_lut1d_from_inverse};

use crate::context::Context;
use crate::fileformats::test_utils::build_ops_test;

/// The Lut1D of op `index` of the ops of the test file `ctf_file`, forward, which has
/// `num_ops` ops.
fn file_lut(ctf_file: &str, num_ops: usize, index: usize) -> Lut1DOpData {
    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, ctf_file, &context, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), num_ops);

    let op_data = ops[index].data();
    assert_eq!(op_data.get_type(), OpDataType::Lut1D);
    let OpData::Lut1D(lut) = &**op_data else {
        panic!("not a Lut1D");
    };
    lut.clone()
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, make_fast_from_inverse_gpu_extented_domain)` @ v2.5.2.
#[test]
fn make_fast_from_inverse_gpu_extented_domain() {
    let mut lut_edit = file_lut("lut1d_inverse_gpu.ctf", 2, 1);
    // Ordinarily the entries would be determined by the inDepth. This is just to make sure the
    // 32f depth of the above is not what gets the half domain.
    lut_edit.set_file_output_bit_depth(BitDepth::Uint10);
    let lut = lut_edit;

    let new_lut = make_fast_lut1d_from_inverse(&lut).unwrap();

    // This LUT has values outside [0,1], so the fastLut needs to have a half domain even on
    // GPU.
    assert_eq!(new_lut.get_array().get_length(), 65536);
    assert!(new_lut.is_input_half_domain());
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, make_fast_from_inverse_f32_opt)` @ v2.5.2.
#[test]
fn make_fast_from_inverse_f32_opt() {
    let lut = file_lut("lut1d_inv.ctf", 3, 2);

    let new_lut = make_fast_lut1d_from_inverse(&lut).unwrap();

    // TODO: This LUT has all values in [0,1], so the fastLut should be compact for more
    // efficient evaluation and less texture usage on GPU.
    assert_eq!(new_lut.get_array().get_length(), 65536);
    assert!(new_lut.is_input_half_domain());
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, make_fast_from_inverse_half_domain)` @ v2.5.2.
#[test]
fn make_fast_from_inverse_half_domain() {
    let lut = file_lut("lut1d_halfdom.ctf", 2, 1);
    let lut_inv = lut.inverse();

    let new_lut = make_fast_lut1d_from_inverse(&lut_inv).unwrap();

    // Source LUT has an extended domain, so fastLut should have a half domain.
    assert_eq!(new_lut.get_array().get_length(), 65536);
    assert!(new_lut.is_input_half_domain());
}
