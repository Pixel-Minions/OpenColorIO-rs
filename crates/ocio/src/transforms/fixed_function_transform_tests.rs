// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the fixed function transform: `tests/cpu/transforms/FixedFunctionTransform_tests.cpp`
//! @ v2.5.2, and `FixedFunctionOp create_transform` (tests/cpu/ops/fixedfunction/
//! FixedFunctionOp_tests.cpp @ v2.5.2), which tests `CreateFixedFunctionTransform` and needed
//! the transform. The text, the validation, the equality and the ops built are compared with
//! the wheel's in `tests/fixed_function_transform_oracle.rs`.

use ocio_ops::format_metadata::METADATA_NAME;
use ocio_ops::open_color_types::TransformDirection;
use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::transform::Transform;

/// Port of `OCIO_ADD_TEST(FixedFunctionTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut func = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]).unwrap();
    assert_eq!(func.direction(), TransformDirection::Forward);
    assert_eq!(func.style(), FixedFunctionStyle::AcesRedMod03);
    assert_eq!(func.params().len(), 0);
    func.validate().unwrap();

    func.set_direction(TransformDirection::Inverse);
    assert_eq!(func.direction(), TransformDirection::Inverse);
    assert_eq!(func.style(), FixedFunctionStyle::AcesRedMod03);
    assert_eq!(func.params().len(), 0);
    func.validate().unwrap();

    func.set_style(FixedFunctionStyle::AcesRedMod10).unwrap();
    assert_eq!(func.style(), FixedFunctionStyle::AcesRedMod10);
    assert_eq!(func.direction(), TransformDirection::Inverse);
    assert_eq!(func.params().len(), 0);
    func.validate().unwrap();

    func.set_style(FixedFunctionStyle::AcesGamutComp13).unwrap();
    assert_eq!(func.style(), FixedFunctionStyle::AcesGamutComp13);
    assert_eq!(func.direction(), TransformDirection::Inverse);
    assert_eq!(func.params().len(), 0);
    check_throw_what(
        func.validate(),
        "The style 'ACES_GamutComp13 (Inverse)' must have seven parameters but 0 found.",
    );
    let values_7 = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    func.set_params(&values_7);
    assert_eq!(func.params().len(), 7);
    func.validate().unwrap();

    func.set_params(&[]);
    func.set_style(FixedFunctionStyle::Rec2100Surround).unwrap();
    check_throw_what(
        func.validate(),
        "The style 'REC2100_Surround (Inverse)' must have one parameter but 0 found.",
    );

    assert_eq!(func.params().len(), 0);
    let values_1 = [1.];
    func.set_params(&values_1);
    assert_eq!(func.params().len(), 1);
    let results = func.params();
    assert_eq!(results[0], values_1[0]);

    func.validate().unwrap();

    func.set_style(FixedFunctionStyle::AcesDarkToDim10).unwrap();
    check_throw_what(
        func.validate(),
        "The style 'ACES_DarkToDim10 (Inverse)' must have zero parameters but 1 found.",
    );

    func.set_style(FixedFunctionStyle::RgbToHsv).unwrap();
    check_throw_what(
        func.validate(),
        "The style 'RGB_TO_HSV' must have zero parameters but 1 found.",
    );

    check_throw_what(
        func.set_style(FixedFunctionStyle::AcesGamutMap02),
        "Unimplemented fixed function types: FIXED_FUNCTION_ACES_GAMUTMAP_02, \
         FIXED_FUNCTION_ACES_GAMUTMAP_07.",
    );

    check_throw_what(
        FixedFunctionTransform::new(FixedFunctionStyle::AcesGamutMap07, &[]),
        "Unimplemented fixed function types: FIXED_FUNCTION_ACES_GAMUTMAP_02, \
         FIXED_FUNCTION_ACES_GAMUTMAP_07.",
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionTransform, createEditableCopy)` @ v2.5.2.
#[test]
fn create_editable_copy() {
    // Create an editable copy for fixed transforms without params.

    let func = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]).unwrap();
    func.create_editable_copy().unwrap();

    // Create an editable copy for fixed transforms with params.

    let values = [1.];
    let func = FixedFunctionTransform::new(FixedFunctionStyle::Rec2100Surround, &values).unwrap();
    func.create_editable_copy().unwrap();
}

/// `createEditableCopy` makes the copy with the validating `Create`, in the forward style:
/// parameters set without validation are refused with its message, in either direction
/// (FixedFunctionTransform.cpp:54-73 @ v2.5.2); `Clone` copies them as they are.
#[test]
fn create_editable_copy_validates() {
    let mut func = FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow03, &[]).unwrap();
    func.set_direction(TransformDirection::Inverse);
    func.set_params(&[1.]);
    check_throw_what(
        func.create_editable_copy(),
        "The style 'ACES_Glow03 (Forward)' must have zero parameters but 1 found.",
    );
    assert_eq!(func.clone().params(), [1.]);

    func.set_params(&[]);
    func.format_metadata_mut().set_name(Some(b"glow"));
    let copy = func.create_editable_copy().unwrap();
    assert_eq!(copy.direction(), TransformDirection::Inverse);
    assert_eq!(copy.format_metadata().get_name(), b"glow");
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let data = vec![0.5];
    let style = FixedFunctionOpStyle::Rec2100SurroundInv;

    let mut func_data = FixedFunctionOpData::with_params(style, data).unwrap();

    assert_eq!(FixedFunctionOpStyle::Rec2100SurroundInv, func_data.style());
    // Direction is already inverse, this does nothing.
    func_data.set_direction(TransformDirection::Inverse);
    assert_eq!(FixedFunctionOpStyle::Rec2100SurroundInv, func_data.style());
    // Changing the direction is changing the style.
    func_data.set_direction(TransformDirection::Forward);
    assert_eq!(FixedFunctionOpStyle::Rec2100SurroundFwd, func_data.style());
    func_data.set_direction(TransformDirection::Inverse);
    assert_eq!(FixedFunctionOpStyle::Rec2100SurroundInv, func_data.style());

    func_data
        .get_format_metadata_mut()
        .add_attribute(Some(METADATA_NAME), Some(b"test"))
        .unwrap();

    let mut ops = OpVec::new();
    create_fixed_function_op_from_data(&mut ops, func_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_fixed_function_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::FixedFunction(ff_transform) = transform else {
        panic!("not a FixedFunctionTransform: {transform:?}");
    };

    let metadata = ff_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), METADATA_NAME);
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(ff_transform.direction(), TransformDirection::Inverse);
    assert_eq!(ff_transform.style(), FixedFunctionStyle::Rec2100Surround);
    assert_eq!(ff_transform.params().len(), 1);
    let param = ff_transform.params();
    assert_eq!(param[0], 0.5);
}

/// `CreateFixedFunctionTransform`'s refusal of another op type
/// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.cpp:167-171 @ v2.5.2).
#[test]
fn create_transform_of_another_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::matrix::matrix_op::create_scale_op(
        &mut ops,
        &[2.0, 2.0, 2.0, 1.0],
        TransformDirection::Forward,
    );
    let mut group = GroupTransform::new();
    check_throw_what(
        create_fixed_function_transform(&mut group, &ops[0]),
        "CreateFixedFunctionTransform: op has to be a FixedFunctionOp",
    );
    assert_eq!(group.num_transforms(), 0);
}
