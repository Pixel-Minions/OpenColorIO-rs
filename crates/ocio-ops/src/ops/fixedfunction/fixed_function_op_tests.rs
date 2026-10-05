// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/fixedfunction/FixedFunctionOp_tests.cpp` @ v2.5.2: the tests of the
//! op. Its test of `CreateFixedFunctionTransform` (`create_transform`) needs the transform: it
//! is in crates/ocio/src/transforms/fixed_function_transform_tests.rs. The `FixedFunctionOps`
//! tests of the HSY, PQ, gamma-log and double-log styles come with their renderers (2.3c2,
//! 2.3d).

use super::*;

/// Whether the renderer's type name contains `name`, as upstream finds it in
/// `typeid(c).name()`: the type is the name its `Debug` output starts with.
fn has_type(op: &Arc<dyn CpuOp>, name: &str) -> bool {
    let debug = format!("{op:?}");
    let ty = &debug[..debug.find([' ', '(', '{']).unwrap_or(debug.len())];
    ty.contains(name)
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut ops = OpVec::new();
    let params = Params::new();

    create_fixed_function_op(&mut ops, FixedFunctionOpStyle::AcesRedMod10Fwd, &params).unwrap();

    assert_eq!(ops.len(), 1);
    let func = &ops[0];

    assert!(!func.is_no_op().unwrap());
    assert!(!func.is_identity().unwrap());

    let OpData::FixedFunction(func_data) = &**func.data() else {
        panic!("a FixedFunction op");
    };
    assert_eq!(func_data.style(), FixedFunctionOpStyle::AcesRedMod10Fwd);
    assert!(*func_data.params() == params);
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, glow03_cpu_engine)` @ v2.5.2.
#[test]
fn glow03_cpu_engine() {
    // Validate that the right CPU OP is created.

    let style = FixedFunctionOpStyle::AcesGlow03Fwd;
    let data = Params::new();

    let func_data = FixedFunctionOpData::with_params(style, data).unwrap();

    let mut func = Op::new(OpData::FixedFunction(func_data));
    func.validate().unwrap();

    let cpu_op = func.get_cpu_op(false).unwrap().unwrap();
    assert!(has_type(&cpu_op, "RendererAcesGlow03Fwd"));
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, darktodim10_cpu_engine)` @ v2.5.2.
#[test]
fn darktodim10_cpu_engine() {
    // Validate that the right CPU OP is created.

    let style = FixedFunctionOpStyle::AcesDarkToDim10Fwd;
    let data = Params::new();

    let func_data = FixedFunctionOpData::with_params(style, data).unwrap();

    let mut func = Op::new(OpData::FixedFunction(func_data));
    func.validate().unwrap();

    let cpu_op = func.get_cpu_op(false).unwrap().unwrap();
    assert!(has_type(&cpu_op, "RendererAcesDarkToDim10Fwd"));
}

/// The checks of the `_inv` tests: two non-identity ops of the same type, each the other's
/// inverse.
#[track_caller]
fn check_pair_inverse(first: FixedFunctionOpStyle, second: FixedFunctionOpStyle, params: &Params) {
    let mut ops = OpVec::new();

    create_fixed_function_op(&mut ops, first, params).unwrap();
    create_fixed_function_op(&mut ops, second, params).unwrap();

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 2);

    let op0 = &ops[0];
    let op1 = &ops[1];

    assert!(!op0.is_identity().unwrap());
    assert!(!op1.is_identity().unwrap());

    assert!(op0.is_same_type(op1));
    assert!(op0.is_inverse(op1));
    assert!(op1.is_inverse(op0));
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, aces_red_mod_inv)` @ v2.5.2.
#[test]
fn aces_red_mod_inv() {
    check_pair_inverse(
        FixedFunctionOpStyle::AcesRedMod03Inv,
        FixedFunctionOpStyle::AcesRedMod03Fwd,
        &Params::new(),
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, aces_glow_inv)` @ v2.5.2.
#[test]
fn aces_glow_inv() {
    check_pair_inverse(
        FixedFunctionOpStyle::AcesGlow03Inv,
        FixedFunctionOpStyle::AcesGlow03Fwd,
        &Params::new(),
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, aces_darktodim10_inv)` @ v2.5.2.
#[test]
fn aces_darktodim10_inv() {
    check_pair_inverse(
        FixedFunctionOpStyle::AcesDarkToDim10Inv,
        FixedFunctionOpStyle::AcesDarkToDim10Fwd,
        &Params::new(),
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, aces_gamutmap13_inv)` @ v2.5.2.
#[test]
fn aces_gamutmap13_inv() {
    let params: Params = vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];

    check_pair_inverse(
        FixedFunctionOpStyle::AcesGamutComp13Inv,
        FixedFunctionOpStyle::AcesGamutComp13Fwd,
        &params,
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOp, rec2100_surround_inv)` @ v2.5.2.
#[test]
fn rec2100_surround_inv() {
    let mut ops = OpVec::new();

    create_fixed_function_op(
        &mut ops,
        FixedFunctionOpStyle::Rec2100SurroundFwd,
        &vec![2.],
    )
    .unwrap();

    create_fixed_function_op(
        &mut ops,
        FixedFunctionOpStyle::Rec2100SurroundFwd,
        &vec![1. / 2.],
    )
    .unwrap();

    create_fixed_function_op(
        &mut ops,
        FixedFunctionOpStyle::Rec2100SurroundInv,
        &vec![2.],
    )
    .unwrap();

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 3);
    {
        let op0 = &ops[0];
        let op1 = &ops[1];
        let op2 = &ops[2];

        assert!(!op0.is_identity().unwrap());
        assert!(!op1.is_identity().unwrap());
        assert!(!op2.is_identity().unwrap());

        assert!(op0.is_same_type(op1));
        assert!(op0.is_inverse(op1));
        assert!(op1.is_inverse(op0));
        assert!(op0.is_inverse(op2));
        assert!(op2.is_inverse(op0));
    }
    create_fixed_function_op(
        &mut ops,
        FixedFunctionOpStyle::Rec2100SurroundFwd,
        &vec![2.01],
    )
    .unwrap();

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 4);
    {
        let op0 = &ops[0];
        let op1 = &ops[1];
        let op3 = &ops[3];

        assert!(!op0.is_inverse(op3));
        assert!(!op1.is_inverse(op3));
    }
}

/// The op's info, a copy (`clone_op`) that validates, and `combineWith`'s refusal. (The cache
/// ID is compared with the wheel's through the processors, `tests/fixed_function_op_oracle.rs`.)
#[test]
fn info_clone_and_combine() {
    let mut ops = OpVec::new();
    create_fixed_function_op(
        &mut ops,
        FixedFunctionOpStyle::AcesGlow10Fwd,
        &Params::new(),
    )
    .unwrap();
    let op = ops[0].clone();
    assert_eq!(op.get_info(), "<FixedFunctionOp>");
    let OpData::FixedFunction(data) = &**op.data() else {
        panic!("a FixedFunction op");
    };
    assert!(!op.can_combine_with(&op).unwrap());
    let mut out = OpVec::new();
    let err = op.combine_with(&mut out, &op).unwrap_err();
    assert_eq!(
        err.message(),
        "FixedFunctionOp: canCombineWith must be checked before calling combineWith."
    );

    // `clone` copies through the validating constructor.
    let mut invalid = data.clone();
    invalid.set_params(vec![1.0]);
    let invalid_op = Op::new(OpData::FixedFunction(invalid));
    assert_eq!(
        invalid_op.clone_op().unwrap_err().message(),
        "The style 'ACES_Glow10 (Forward)' must have zero parameters but 1 found."
    );

    // An inverse op is made from a validated copy.
    let mut ops = OpVec::new();
    let mut bad = FixedFunctionOpData::new(FixedFunctionOpStyle::AcesGlow10Fwd).unwrap();
    bad.set_params(vec![1.0]);
    assert_eq!(
        create_fixed_function_op_from_data(&mut ops, bad.clone(), TransformDirection::Inverse)
            .unwrap_err()
            .message(),
        "The style 'ACES_Glow10 (Forward)' must have zero parameters but 1 found."
    );
    // A forward op keeps the data as it is.
    create_fixed_function_op_from_data(&mut ops, bad, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
}

/// The `FixedFunctionOps` tests: a style and its inverse are inverse ops of the same type,
/// neither an identity, and the first renders with `renderer`.
#[track_caller]
fn check_ops(fwd: FixedFunctionOpStyle, inv: FixedFunctionOpStyle, renderer: &str) {
    check_ops_with(fwd, inv, &Params::new(), renderer);
}

/// [`check_ops`] with parameters.
#[track_caller]
fn check_ops_with(
    fwd: FixedFunctionOpStyle,
    inv: FixedFunctionOpStyle,
    params: &Params,
    renderer: &str,
) {
    let mut ops = OpVec::new();

    create_fixed_function_op(&mut ops, fwd, params).unwrap();
    create_fixed_function_op(&mut ops, inv, params).unwrap();

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 2);

    let op0 = &ops[0];
    let op1 = &ops[1];

    assert!(!op0.is_identity().unwrap());
    assert!(!op1.is_identity().unwrap());

    assert!(op0.is_same_type(op1));
    assert!(op0.is_inverse(op1));
    assert!(op1.is_inverse(op0));

    let cpu_op = op0.get_cpu_op(false).unwrap().unwrap();
    assert!(has_type(&cpu_op, renderer), "{cpu_op:?}");
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOps, RGB_TO_HSV)` @ v2.5.2.
#[test]
fn ops_rgb_to_hsv() {
    use FixedFunctionOpStyle::*;
    check_ops(RgbToHsv, HsvToRgb, "RendererRgbToHsv");
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOps, XYZ_TO_xyY)` @ v2.5.2.
#[test]
fn ops_xyz_to_xyy() {
    use FixedFunctionOpStyle::*;
    check_ops(XyzToXyy, XyyToXyz, "RendererXyzToXyy");
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOps, XYZ_TO_uvY)` @ v2.5.2.
#[test]
fn ops_xyz_to_uvy() {
    use FixedFunctionOpStyle::*;
    check_ops(XyzToUvy, UvyToXyz, "RendererXyzToUvy");
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOps, XYZ_TO_LUV)` @ v2.5.2.
#[test]
fn ops_xyz_to_luv() {
    use FixedFunctionOpStyle::*;
    check_ops(XyzToLuv, LuvToXyz, "RendererXyzToLuv");
}
