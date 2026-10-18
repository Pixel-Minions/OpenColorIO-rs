// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of upstream's tests/cpu/ops/lut1d/Lut1DOp_tests.cpp @ v2.5.2 that read files
//! (`BuildOpsTest`). The others are `crates/ocio-ops/src/ops/lut1d/lut1d_op_tests.rs` and
//! `crates/ocio/src/transforms/lut1d_transform_tests.rs`.

use ocio_ops::op::OpVec;
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::lut1d::lut1d_op_data::{ComposeMethod, Lut1DOpData};
use ocio_testkit::upstream::check_close;

use crate::context::Context;
use crate::fileformats::test_utils::build_ops_test;

/// Port of `OCIO_ADD_TEST(Lut1D, lut_1d_compose_with_bit_depth)` @ v2.5.2.
#[test]
fn lut_1d_compose_with_bit_depth() {
    let ctf_file = "clf/lut1d_comp.clf";

    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, ctf_file, &context, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 3);
    let lut = |i: usize| -> Lut1DOpData {
        let op_data = ops[i].data();
        assert_eq!(op_data.get_type(), OpDataType::Lut1D);
        let OpData::Lut1D(lut) = &**op_data else {
            panic!("not a Lut1D");
        };
        lut.clone()
    };
    let lut1 = lut(1);
    let lut2 = lut(2);

    {
        let lut_composed = Lut1DOpData::compose(&lut1, &lut2, ComposeMethod::ResampleNo).unwrap();

        let error = 1e-5f32;
        let values = lut_composed.get_array().get_values();
        assert_eq!(lut_composed.get_array().get_length(), 23);
        check_close(values[0], 0.463939786f32, error);
        check_close(values[1], 0.440741241f32, error);
        check_close(values[2], 0.417542696f32, error);
        check_close(values[3], 0.4876194298f32, error);
        check_close(values[4], 0.4632416070f32, error);
        check_close(values[5], 0.4388564825f32, error);
        check_close(values[66], 0.863989234f32, error);
        check_close(values[67], 0.820785284f32, error);
        check_close(values[68], 0.777587533f32, error);
    }
    {
        let lut_composed = Lut1DOpData::compose(&lut1, &lut2, ComposeMethod::ResampleBig).unwrap();

        let error = 1e-5f32;
        let values = lut_composed.get_array().get_values();
        assert_eq!(lut_composed.get_array().get_length(), 65536);
        check_close(values[0], 0.463939786f32, error);
        check_close(values[1], 0.440741241f32, error);
        check_close(values[2], 0.417542696f32, error);
        check_close(values[98688], 0.6885353923f32, error);
        check_close(values[98689], 0.6541032195f32, error);
        check_close(values[98690], 0.619679749f32, error);
        check_close(values[196605], 0.863989234f32, error);
        check_close(values[196606], 0.820785284f32, error);
        check_close(values[196607], 0.777587533f32, error);
    }
}
