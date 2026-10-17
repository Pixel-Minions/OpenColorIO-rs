// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of upstream's tests/cpu/ops/lut3d/Lut3DOpData_tests.cpp @ v2.5.2 that read files
//! (`BuildOpsTest`). The others are `crates/ocio-ops/src/ops/lut3d/lut3d_op_data_tests.rs`.

use ocio_ops::format_metadata::METADATA_DESCRIPTION;
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{BitDepth, TransformDirection};
use ocio_ops::ops::lut3d::lut3d_op_data::{Lut3DOpData, make_fast_lut3d_from_inverse};
use ocio_testkit::upstream::check_close;

use crate::context::Context;
use crate::fileformats::test_utils::build_ops_test;

/// The Lut3D of the ops of the test file `file_name`, forward: its second op, after the
/// file's no-op.
fn file_lut(file_name: &str, context: &Context) -> Lut3DOpData {
    let mut ops = OpVec::new();
    build_ops_test(&mut ops, file_name, context, TransformDirection::Forward).unwrap();

    // First op is a FileNoOp.
    assert_eq!(2, ops.len());

    let OpData::Lut3D(lut) = &**ops[1].data() else {
        panic!("not a Lut3D");
    };
    lut.clone()
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, compose)` @ v2.5.2.
#[test]
fn compose() {
    let context = Context::new();
    let lut_data0 = file_lut("spi_ocio_srgb_test.spi3d", &context);
    let lut_data1 = file_lut("comp2.spi3d", &context);

    let mut lut_data0_cloned = lut_data0.clone();
    lut_data0_cloned
        .get_format_metadata_mut()
        .set_name(Some(b"lut1"));
    lut_data0_cloned
        .get_format_metadata_mut()
        .add_child_element(Some(METADATA_DESCRIPTION), Some(b"description of lut1"))
        .unwrap();
    let mut lut_data1_cloned = lut_data1.clone();
    lut_data1_cloned
        .get_format_metadata_mut()
        .set_name(Some(b"lut2"));
    lut_data1_cloned
        .get_format_metadata_mut()
        .add_child_element(Some(METADATA_DESCRIPTION), Some(b"description of lut2"))
        .unwrap();
    let composed = Lut3DOpData::compose(&lut_data0_cloned, &lut_data1_cloned).unwrap();

    // FormatMetadata composition.
    let metadata = composed.get_format_metadata();
    assert_eq!(metadata.get_name(), b"lut1 + lut2");
    assert_eq!(metadata.get_num_children_elements(), 2);
    let desc1 = metadata.get_child_element(0).unwrap();
    assert_eq!(desc1.get_element_name(), METADATA_DESCRIPTION);
    assert_eq!(desc1.get_element_value(), b"description of lut1");
    let desc2 = metadata.get_child_element(1).unwrap();
    assert_eq!(desc2.get_element_name(), METADATA_DESCRIPTION);
    assert_eq!(desc2.get_element_value(), b"description of lut2");

    let array = composed.get_array();
    assert_eq!(array.get_length(), 32);
    assert_eq!(array.get_num_color_components(), 3);
    assert_eq!(array.get_num_values(), 32 * 32 * 32 * 3);

    let values = array.get_values();
    check_close(values[0], 0.0288210791f32, 1e-7f32);
    check_close(values[1], 0.0280428901f32, 1e-7f32);
    check_close(values[2], 0.0262413863f32, 1e-7f32);

    check_close(values[666], 0.0f32, 1e-7f32);
    check_close(values[667], 0.274289876f32, 1e-7f32);
    check_close(values[668], 0.854598403f32, 1e-7f32);

    check_close(values[1800], 0.0f32, 1e-7f32);
    check_close(values[1801], 0.411249638f32, 1e-7f32);
    check_close(values[1802], 0.881694913f32, 1e-7f32);

    check_close(values[96903], 1.0f32, 1e-7f32);
    check_close(values[96904], 0.588273168f32, 1e-7f32);
    check_close(values[96905], 0.0f32, 1e-7f32);
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, compose_2)` @ v2.5.2.
#[test]
fn compose_2() {
    let context = Context::new();
    let lut_data0 = file_lut("clf/lut3d_bizarre.clf", &context);
    let lut_data1 = file_lut("clf/lut3d_17x17x17_10i_12i.clf", &context);

    let composed = Lut3DOpData::compose(&lut_data0, &lut_data1).unwrap();

    assert_eq!(composed.get_array().get_length(), 17);
    let a = composed.get_array().get_values();
    check_close(a[6], 2.5942142f32 / 4095.0f32, 1e-7f32);
    check_close(a[7], 29.60961342f32 / 4095.0f32, 1e-7f32);
    check_close(a[8], 154.82646179f32 / 4095.0f32, 1e-7f32);
    check_close(a[8289], 0.4237791896f32, 5e-7f32);
    check_close(a[8290], 0.4076989293f32, 5e-7f32);
    check_close(a[8291], 0.4823103547f32, 5e-7f32);
    check_close(a[14736], 4094.07617188f32 / 4095.0f32, 1e-7f32);
    check_close(a[14737], 4067.37231445f32 / 4095.0f32, 1e-6f32);
    check_close(a[14738], 4088.30493164f32 / 4095.0f32, 1e-6f32);
}

/// Port of `OCIO_ADD_TEST(Lut3DOpData, inv_lut3d_lut_size)` @ v2.5.2.
#[test]
fn inv_lut3d_lut_size() {
    let context = Context::new();
    let fwd_lut_data = file_lut("clf/lut3d_17x17x17_10i_12i.clf", &context);
    let inv_lut_data = fwd_lut_data.inverse();
    let inv_fast_lut_data = make_fast_lut3d_from_inverse(&inv_lut_data).unwrap();

    assert_eq!(
        inv_fast_lut_data.get_file_output_bit_depth(),
        BitDepth::Uint12
    );

    assert_eq!(inv_fast_lut_data.get_array().get_length(), 48);
}
