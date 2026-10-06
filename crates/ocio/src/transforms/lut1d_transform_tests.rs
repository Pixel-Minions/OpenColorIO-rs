// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the 1D LUT transform: `tests/cpu/transforms/Lut1DTransform_tests.cpp` @ v2.5.2, and
//! `Lut1D create_transform` and `Lut1DTransform build_op` (tests/cpu/ops/lut1d/
//! Lut1DOp_tests.cpp @ v2.5.2), which test `CreateLut1DTransform` and `BuildLut1DOp` and needed
//! the transform. The text, the validation, the equality and the ops built are compared with
//! the wheel's in `tests/lut1d_transform_oracle.rs`.
//!
//! Not here: `Lut1DTransform non_monotonic`, whose processor of the inverse LUT needs the
//! inverse 1D LUT (Phase 2, WP 2.1).

use ocio_ops::format_metadata::METADATA_NAME;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{BitDepth, TransformDirection};
use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::config::Config;
use crate::transform::{Transform, build_ops};

/// Port of `OCIO_ADD_TEST(Lut1DTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut lut = Lut1DTransform::new();

    assert_eq!(lut.length(), 2);
    assert_eq!(lut.direction(), TransformDirection::Forward);
    assert_eq!(lut.hue_adjust(), Lut1DHueAdjust::None);
    assert!(!lut.input_half_domain());
    assert!(!lut.output_raw_halfs());
    let [r, g, b] = lut.value(0).unwrap();
    assert_eq!(r, 0.0f32);
    assert_eq!(g, 0.0f32);
    assert_eq!(b, 0.0f32);
    let [r, g, b] = lut.value(1).unwrap();
    assert_eq!(r, 1.0f32);
    assert_eq!(g, 1.0f32);
    assert_eq!(b, 1.0f32);

    lut.set_direction(TransformDirection::Inverse);
    assert_eq!(lut.direction(), TransformDirection::Inverse);

    lut.set_length(3).unwrap();
    assert_eq!(lut.length(), 3);
    let [r, g, b] = lut.value(0).unwrap();
    assert_eq!(r, 0.0f32);
    assert_eq!(g, 0.0f32);
    assert_eq!(b, 0.0f32);
    let [r, g, b] = lut.value(1).unwrap();
    assert_eq!(r, 0.5f32);
    assert_eq!(g, 0.5f32);
    assert_eq!(b, 0.5f32);
    let [r, g, b] = lut.value(2).unwrap();
    assert_eq!(r, 1.0f32);
    assert_eq!(g, 1.0f32);
    assert_eq!(b, 1.0f32);

    let (r, g, b) = (0.51f32, 0.52f32, 0.53f32);
    lut.set_value(1, r, g, b).unwrap();

    let [r, g, b] = lut.value(1).unwrap();

    assert_eq!(r, 0.51f32);
    assert_eq!(g, 0.52f32);
    assert_eq!(b, 0.53f32);

    assert_eq!(lut.file_output_bit_depth(), BitDepth::Unknown);

    lut.set_file_output_bit_depth(BitDepth::Uint8);
    assert_eq!(lut.file_output_bit_depth(), BitDepth::Uint8);

    // File out bit-depth does not affect values.
    let [r, g, b] = lut.value(1).unwrap();

    assert_eq!(r, 0.51f32);
    assert_eq!(g, 0.52f32);
    assert_eq!(b, 0.53f32);

    lut.validate().unwrap();

    check_throw_what(
        lut.set_value(3, 0.0, 0.0, 0.0),
        "should be less than the length",
    );
    check_throw_what(lut.value(3), "should be less than the length");

    lut.set_input_half_domain(true);
    check_throw_what(lut.validate(), "65536 required for halfDomain 1D LUT");

    check_throw_what(lut.set_length(1024 * 1024 + 1), "must not be greater than");

    lut.set_input_half_domain(false);
    lut.set_value(0, -0.2, 0.1, -0.3).unwrap();
    lut.set_value(2, 1.2, 1.3, 0.8).unwrap();

    let oss = lut.to_string();
    assert_eq!(
        oss,
        "<Lut1DTransform direction=inverse, fileoutdepth=8ui, interpolation=default, \
         inputhalf=0, outputrawhalf=0, hueadjust=0, length=3, minrgb=[-0.2, 0.1, -0.3], \
         maxrgb=[1.2, 1.3, 0.8]>"
    );

    let lut2 = lut.clone();
    let oss2 = lut2.to_string();
    assert_eq!(oss2, oss);

    assert!(lut.equals(&lut2));
}

/// Port of `OCIO_ADD_TEST(Lut1DTransform, create_with_parameters)` @ v2.5.2.
#[test]
fn create_with_parameters() {
    let lut0 = Lut1DTransform::with_length(65536, true).unwrap();

    assert_eq!(lut0.length(), 65536);
    assert_eq!(lut0.direction(), TransformDirection::Forward);
    assert_eq!(lut0.hue_adjust(), Lut1DHueAdjust::None);
    assert!(lut0.input_half_domain());
    lut0.validate().unwrap();

    let lut1 = Lut1DTransform::with_length(10, true).unwrap();

    assert_eq!(lut1.length(), 10);
    assert!(lut1.input_half_domain());
    check_throw_what(lut1.validate(), "65536 required for halfDomain 1D LUT");

    let lut2 = Lut1DTransform::with_length(8, false).unwrap();

    assert_eq!(lut2.length(), 8);
    assert!(!lut2.input_half_domain());
    lut2.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(Lut1DTransform, hue_adjust)` @ v2.5.2.
#[test]
fn hue_adjust() {
    let mut lut = Lut1DTransform::new();
    assert_eq!(lut.hue_adjust(), Lut1DHueAdjust::None);
    lut.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    assert_eq!(lut.hue_adjust(), Lut1DHueAdjust::Dw3);
    check_throw_what(
        lut.set_hue_adjust(Lut1DHueAdjust::Wypn),
        "1D LUT HUE_WYPN hue adjust style is not implemented.",
    );
}

/// Port of `OCIO_ADD_TEST(Lut1DTransform, format_metadata)` @ v2.5.2.
#[test]
fn format_metadata() {
    let mut lut = Lut1DTransform::new();
    let fmd = lut.format_metadata_mut();
    fmd.set_name(Some(b"test LUT"));
    fmd.set_id(Some(b"LUTID"));
    let cfmd = lut.format_metadata();
    assert_eq!(cfmd.get_name(), b"test LUT");
    assert_eq!(cfmd.get_id(), b"LUTID");
}

/// Port of `OCIO_ADD_TEST(Lut1D, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;

    let mut lut = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 3, false).unwrap();
    lut.set_file_output_bit_depth(BitDepth::Uint10);
    lut.get_array_mut()[3] = 0.51;
    lut.get_array_mut()[4] = 0.52;
    lut.get_array_mut()[5] = 0.53;

    let metadata_source = lut.get_format_metadata_mut();
    metadata_source
        .add_attribute(Some(METADATA_NAME), Some(b"test"))
        .unwrap();

    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut, direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_lut1d_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::Lut1D(l_transform) = transform else {
        panic!("not a Lut1DTransform: {transform:?}");
    };

    let metadata = l_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), METADATA_NAME);
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(l_transform.direction(), direction);
    assert_eq!(l_transform.length(), 3);

    assert_eq!(l_transform.file_output_bit_depth(), BitDepth::Uint10);

    let [r, g, b] = l_transform.value(1).unwrap();

    assert_eq!(r, 0.51f32);
    assert_eq!(g, 0.52f32);
    assert_eq!(b, 0.53f32);
}

/// Port of `OCIO_ADD_TEST(Lut1DTransform, build_op)` @ v2.5.2.
///
/// Upstream builds with `Config::Create()`; `BuildLut1DOp` doesn't read the config, so the raw
/// config stands in for it until the config is ported (WP 1.8g).
#[test]
fn build_op() {
    let mut lut = Lut1DTransform::new();
    lut.set_length(3).unwrap();

    let r = 0.51f32;
    let g = 0.52f32;
    let b = 0.53f32;
    lut.set_value(1, r, g, b).unwrap();

    let config = Config::create_raw().unwrap();

    let mut ops = OpVec::new();
    build_ops(
        &mut ops,
        &config,
        config.current_context(),
        &lut.into(),
        TransformDirection::Forward,
    )
    .unwrap();

    assert_eq!(ops.len(), 1);

    let OpData::Lut1D(lutdata) = &**ops[0].data() else {
        panic!("not a Lut1D op: {:?}", ops[0].data());
    };

    assert_eq!(lutdata.get_array().get_length(), 3);
    assert_eq!(lutdata.get_array()[3], r);
    assert_eq!(lutdata.get_array()[4], g);
    assert_eq!(lutdata.get_array()[5], b);
}

/// `CreateLut1DTransform` refuses an op of another type (which `CreateTransform`'s dispatch
/// never passes it) and adds nothing.
#[test]
fn create_lut1d_transform_needs_a_lut1d_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::noop::create_file_no_op(&mut ops, b"f");
    let mut group = GroupTransform::new();
    check_throw_what(
        create_lut1d_transform(&mut group, &ops[0]),
        "CreateLut1DTransform: op has to be a Lut1DOp",
    );
    assert_eq!(group.num_transforms(), 0);
}

/// `BuildLut1DOp` validates the data itself: an invalid LUT adds no op, with the data's error,
/// the one `validate` reports after its prefix.
#[test]
fn build_lut1d_op_validates_the_data() {
    let mut lut = Lut1DTransform::new();
    lut.set_input_half_domain(true);
    let validated = lut.validate().unwrap_err();

    let mut ops = OpVec::new();
    let built = build_lut1d_op(&mut ops, &lut, TransformDirection::Forward).unwrap_err();
    assert_eq!(
        format!("Lut1DTransform validation failed: {}", built.message()),
        validated.message()
    );
    assert_eq!(ops.len(), 0);
}

/// `setLength` and `Create(length, isHalfDomain)` refuse a length under 2, and a refused
/// `setLength` leaves the values as they were.
#[test]
fn length_errors() {
    check_throw_what(
        Lut1DTransform::with_length(1, false),
        "LUT 1D length needs to be at least 2.",
    );
    let mut lut = Lut1DTransform::new();
    lut.set_value(1, 0.25, 0.5, 0.75).unwrap();
    check_throw_what(lut.set_length(1), "LUT 1D length needs to be at least 2.");
    assert_eq!(lut.length(), 2);
    assert_eq!(lut.value(1).unwrap(), [0.25, 0.5, 0.75]);
}
