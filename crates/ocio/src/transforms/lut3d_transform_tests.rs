// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the 3D LUT transform: `tests/cpu/transforms/Lut3DTransform_tests.cpp` @ v2.5.2, and
//! `Lut3D create_transform` and `Lut3DTransform build_op` (tests/cpu/ops/lut3d/
//! Lut3DOp_tests.cpp @ v2.5.2), which test `CreateLut3DTransform` and `BuildLut3DOp` and needed
//! the transform. The text, the validation, the equality and the ops built are compared with
//! the wheel's in `tests/lut3d_transform_oracle.rs`.

use ocio_ops::format_metadata::METADATA_NAME;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{BitDepth, TransformDirection};
use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::config::Config;
use crate::transform::{Transform, build_ops};

/// Port of `OCIO_ADD_TEST(Lut3DTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut lut = Lut3DTransform::new();

    assert_eq!(lut.grid_size(), 2);
    assert_eq!(lut.direction(), TransformDirection::Forward);
    let [r, g, b] = lut.value(0, 0, 0).unwrap();
    assert_eq!(r, 0.0f32);
    assert_eq!(g, 0.0f32);
    assert_eq!(b, 0.0f32);
    let [r, g, b] = lut.value(0, 1, 1).unwrap();
    assert_eq!(r, 0.0f32);
    assert_eq!(g, 1.0f32);
    assert_eq!(b, 1.0f32);
    let [r, g, b] = lut.value(1, 0, 0).unwrap();
    assert_eq!(r, 1.0f32);
    assert_eq!(g, 0.0f32);
    assert_eq!(b, 0.0f32);

    lut.set_direction(TransformDirection::Inverse);
    assert_eq!(lut.direction(), TransformDirection::Inverse);

    lut.set_grid_size(3).unwrap();
    assert_eq!(lut.grid_size(), 3);
    let [r, g, b] = lut.value(0, 0, 0).unwrap();
    assert_eq!(r, 0.0f32);
    assert_eq!(g, 0.0f32);
    assert_eq!(b, 0.0f32);
    let [r, g, b] = lut.value(0, 1, 1).unwrap();
    assert_eq!(r, 0.0f32);
    assert_eq!(g, 0.5f32);
    assert_eq!(b, 0.5f32);
    let [r, g, b] = lut.value(2, 0, 2).unwrap();
    assert_eq!(r, 1.0f32);
    assert_eq!(g, 0.0f32);
    assert_eq!(b, 1.0f32);

    let [r, g, b] = lut.value(0, 1, 2).unwrap();
    assert_eq!(r, 0.0f32);
    assert_eq!(g, 0.5f32);
    assert_eq!(b, 1.0f32);

    let r = 0.1f32;
    let g = 0.52f32;
    let b = 0.93f32;
    lut.set_value(0, 1, 2, r, g, b).unwrap();

    let [r, g, b] = lut.value(0, 1, 2).unwrap();

    assert_eq!(r, 0.1f32);
    assert_eq!(g, 0.52f32);
    assert_eq!(b, 0.93f32);

    assert_eq!(lut.file_output_bit_depth(), BitDepth::Unknown);

    lut.set_file_output_bit_depth(BitDepth::Uint8);
    assert_eq!(lut.file_output_bit_depth(), BitDepth::Uint8);

    // File out bit-depth does not affect values.
    let [r, g, b] = lut.value(0, 1, 2).unwrap();

    assert_eq!(r, 0.1f32);
    assert_eq!(g, 0.52f32);
    assert_eq!(b, 0.93f32);

    check_throw_what(
        lut.set_value(3, 1, 1, 0.0, 0.0, 0.0),
        "should be less than the grid size",
    );
    check_throw_what(lut.value(0, 0, 4), "should be less than the grid size");

    check_throw_what(lut.set_grid_size(200), "must not be greater than '129'");

    lut.validate().unwrap();

    lut.set_value(0, 0, 0, -0.2, -0.1, -0.3).unwrap();
    lut.set_value(2, 2, 2, 1.2, 1.3, 1.8).unwrap();

    assert_eq!(
        lut.to_string(),
        "<Lut3DTransform direction=inverse, fileoutdepth=8ui, interpolation=default, \
         gridSize=3, minrgb=[-0.2, -0.1, -0.3], maxrgb=[1.2, 1.3, 1.8]>"
    );
}

/// Port of `OCIO_ADD_TEST(Lut3DTransform, create_with_parameters)` @ v2.5.2.
#[test]
fn create_with_parameters() {
    let lut = Lut3DTransform::with_grid_size(8).unwrap();

    assert_eq!(lut.grid_size(), 8);
    assert_eq!(lut.direction(), TransformDirection::Forward);
    assert_eq!(lut.interpolation(), Interpolation::Default);

    let [r, g, b] = lut.value(7, 7, 7).unwrap();
    assert_eq!(r, 1.0f32);
    assert_eq!(g, 1.0f32);
    assert_eq!(b, 1.0f32);
}

/// Port of `OCIO_ADD_TEST(Lut3D, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;

    let mut lut = Lut3DOpData::new(3).unwrap();

    lut.set_file_output_bit_depth(BitDepth::Uint10);

    lut.get_array_mut()[39] = 0.61f32;
    lut.get_array_mut()[40] = 0.52f32;
    lut.get_array_mut()[41] = 0.74f32;

    let metadata_source = lut.get_format_metadata_mut();
    metadata_source
        .add_attribute(Some(METADATA_NAME), Some(b"test"))
        .unwrap();

    let mut ops = OpVec::new();
    create_lut3d_op(&mut ops, lut, direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_lut3d_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::Lut3D(l_transform) = transform else {
        panic!("not a Lut3DTransform: {transform:?}");
    };

    let metadata = l_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), METADATA_NAME);
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(l_transform.direction(), direction);
    assert_eq!(l_transform.grid_size(), 3);

    assert_eq!(l_transform.file_output_bit_depth(), BitDepth::Uint10);

    let [r, g, b] = l_transform.value(1, 1, 1).unwrap();

    assert_eq!(r, 0.61f32);
    assert_eq!(g, 0.52f32);
    assert_eq!(b, 0.74f32);
}

/// Port of `OCIO_ADD_TEST(Lut3DTransform, build_op)` @ v2.5.2.
///
/// Upstream builds with `Config::Create()`; `BuildLut3DOp` doesn't read the config, so the raw
/// config stands in for it, as in the 1D LUT's test.
#[test]
fn build_op() {
    let mut lut = Lut3DTransform::new();
    let gs: c_ulong = 4;
    lut.set_grid_size(gs).unwrap();

    let r = 0.51f32;
    let g = 0.52f32;
    let b = 0.53f32;

    let ri: c_ulong = 1;
    let gi: c_ulong = 2;
    let bi: c_ulong = 3;
    lut.set_value(ri, gi, bi, r, g, b).unwrap();

    let config = Config::create_raw().unwrap();

    let mut ops = OpVec::new();
    build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &lut.into(),
        TransformDirection::Forward,
    )
    .unwrap();

    assert_eq!(ops.len(), 1);

    let OpData::Lut3D(lutdata) = &**ops[0].data() else {
        panic!("not a Lut3D op: {:?}", ops[0].data());
    };

    // Blue fast.
    let i = (3 * ((ri * gs + gi) * gs + bi)) as usize;
    assert_eq!(lutdata.get_array().get_length(), gs);
    assert_eq!(lutdata.get_array()[i], r);
    assert_eq!(lutdata.get_array()[i + 1], g);
    assert_eq!(lutdata.get_array()[i + 2], b);
}
