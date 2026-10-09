// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the color space transform: `tests/cpu/transforms/ColorSpaceTransform_tests.cpp` @
//! v2.5.2. The text and the validation are compared with the wheel's in
//! `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::{FixedFunctionStyle, OptimizationFlags};
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpStyle;
use ocio_ops::ops::log::log_op_data::LogOpData;
use ocio_ops::ops::matrix::matrix_op_data::MatrixOpData;
use ocio_testkit::upstream::check_throw_what;

use crate::processor::ProcessorCacheFlags;
use crate::test_env::EnvGuard;
use crate::transform::Transform;
use crate::transforms::exponent_transform::ExponentTransform;
use crate::transforms::file_transform::FileTransform;
use crate::transforms::fixed_function_transform::FixedFunctionTransform;
use crate::transforms::log_transform::LogTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::view_transform::ViewTransform;

/// Port of `OCIO_ADD_TEST(ColorSpaceTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut cst = ColorSpaceTransform::new();
    assert_eq!(cst.direction(), TransformDirection::Forward);
    cst.set_direction(TransformDirection::Inverse);
    assert_eq!(cst.direction(), TransformDirection::Inverse);

    let empty: &[u8] = b"";

    assert_eq!(empty, cst.src());
    let src: &[u8] = b"source";
    cst.set_src(src);
    assert_eq!(src, cst.src());

    assert_eq!(empty, cst.dst());
    let dst: &[u8] = b"destination";
    cst.set_dst(dst);
    assert_eq!(dst, cst.dst());

    assert!(cst.data_bypass());
    cst.set_data_bypass(false);
    assert!(!cst.data_bypass());
    cst.set_data_bypass(true);
    assert!(cst.data_bypass());

    assert!(cst.validate().is_ok());

    cst.set_src("");
    check_throw_what(
        cst.validate(),
        "ColorSpaceTransform: empty source color space name",
    );
    cst.set_src(src);

    cst.set_dst("");
    check_throw_what(
        cst.validate(),
        "ColorSpaceTransform: empty destination color space name",
    );
    cst.set_dst(dst);
}

/// The setters keep a name up to its first NUL, as from a C string, and a copy keeps every
/// field (upstream's `createEditableCopy`, `Impl::operator=`, ColorSpaceTransform.cpp:41-50,
/// 61-66).
#[test]
fn names_stop_at_nul_and_copies_keep_every_field() {
    let mut cst = ColorSpaceTransform::new();
    cst.set_src(b"ab\0cd");
    cst.set_dst(b"\0x");
    assert_eq!(cst.src(), b"ab");
    assert_eq!(cst.dst(), b"");
    check_throw_what(
        cst.validate(),
        "ColorSpaceTransform: empty destination color space name",
    );

    cst.set_dst("d");
    cst.set_direction(TransformDirection::Inverse);
    cst.set_data_bypass(false);
    let copy = cst.clone();
    assert_eq!(copy.src(), b"ab");
    assert_eq!(copy.dst(), b"d");
    assert_eq!(copy.direction(), TransformDirection::Inverse);
    assert!(!copy.data_bypass());
}

// The tests that build ops (WP 3.2a).

/// The type of the data of `ops[index]`.
fn op_type(ops: &OpVec, index: usize) -> OpDataType {
    ops[index].data().get_type()
}

/// The matrix data of `ops[index]`.
fn matrix_at(ops: &OpVec, index: usize) -> &MatrixOpData {
    match &**ops[index].data() {
        OpData::Matrix(data) => data,
        other => panic!("not a Matrix op: {other:?}"),
    }
}

/// The fixed function style of `ops[index]`.
fn ff_style_at(ops: &OpVec, index: usize) -> FixedFunctionOpStyle {
    match &**ops[index].data() {
        OpData::FixedFunction(data) => data.style(),
        other => panic!("not a FixedFunction op: {other:?}"),
    }
}

/// The log data of `ops[index]`.
fn log_at(ops: &OpVec, index: usize) -> &LogOpData {
    match &**ops[index].data() {
        OpData::Log(data) => data,
        other => panic!("not a Log op: {other:?}"),
    }
}

/// Checks the four offsets of a matrix against `offset`.
fn check_offsets(mat: &MatrixOpData, offset: &[f64; 4]) {
    for (i, o) in offset.iter().enumerate() {
        assert_eq!(*o, mat.get_offset_value(i as _).unwrap());
    }
}

/// `BuildColorSpaceOps` with the config's current context.
fn build(config: &Config, cst: &ColorSpaceTransform, dir: TransformDirection) -> Result<OpVec> {
    let mut ops = OpVec::new();
    build_color_space_ops(&mut ops, config, &config.current_context().get(), cst, dir)?;
    Ok(ops)
}

/// Port of `OCIO_ADD_TEST(ColorSpaceTransform, build_colorspace_ops)` @ v2.5.2.
#[test]
fn build_colorspace_ops() {
    let _env = EnvGuard::new();
    //
    // Prepare.
    //

    let mut cst = ColorSpaceTransform::new();
    let src = "source";
    cst.set_src(src);
    let dst = "destination";
    cst.set_dst(dst);

    let mut config = (*Config::create_raw().unwrap()).clone();
    let mut cs_scene_to_ref = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    cs_scene_to_ref.set_name(src);
    let mut mat = MatrixTransform::new();
    let offset: [f64; 4] = [0., 0.1, 0.2, 0.];
    mat.set_offset(&offset);
    let mat = Transform::from(mat);
    cs_scene_to_ref
        .set_transform(Some(&mat), ColorSpaceDirection::ToReference)
        .unwrap();
    config.add_color_space(&cs_scene_to_ref).unwrap();

    let mut cs_scene_from_ref = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    cs_scene_from_ref.set_name(dst);
    let mut ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow03, &[]).unwrap();
    cs_scene_from_ref
        .set_transform(
            Some(&Transform::from(ff.clone())),
            ColorSpaceDirection::FromReference,
        )
        .unwrap();
    config.add_color_space(&cs_scene_from_ref).unwrap();

    config.add_display_view("display", "view", dst, "").unwrap();

    config.validate().unwrap();

    {
        // Test from source to destination.
        // Source has the to_scene_reference transform defined.
        // Destination has the from_scene_reference transform defined.
        // Expecting source to_scene_reference + destination from_scene_reference.
        // (The no-ops are the Allocation transforms.)

        let mut ops = build(&config, &cst, TransformDirection::Forward).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 4);

        // Allocation no-op.
        assert_eq!(op_type(&ops, 0), OpDataType::NoOp);

        // Src CS to reference.
        assert_eq!(op_type(&ops, 1), OpDataType::Matrix);
        check_offsets(matrix_at(&ops, 1), &offset);

        // Reference to dst CS.
        assert_eq!(op_type(&ops, 2), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 2), FixedFunctionOpStyle::AcesGlow03Fwd);

        // Allocation no-op.
        assert_eq!(op_type(&ops, 3), OpDataType::NoOp);
    }

    {
        // Add alias names to color spaces.

        cs_scene_to_ref.add_alias("aliasToRef");
        config.add_color_space(&cs_scene_to_ref).unwrap();

        cs_scene_from_ref.add_alias("aliasFromRef");
        config.add_color_space(&cs_scene_from_ref).unwrap();

        // Use aliases in transform.

        let mut cst_alias = ColorSpaceTransform::new();
        cst_alias.set_src("aliasToRef");
        cst_alias.set_dst("aliasFromRef");

        // Same result as previous block.

        let mut ops = build(&config, &cst_alias, TransformDirection::Forward).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 4);

        // Allocation no-op.
        assert_eq!(op_type(&ops, 0), OpDataType::NoOp);

        // Src CS to reference.
        assert_eq!(op_type(&ops, 1), OpDataType::Matrix);

        // Reference to dst CS.
        assert_eq!(op_type(&ops, 2), OpDataType::FixedFunction);

        // Allocation no-op.
        assert_eq!(op_type(&ops, 3), OpDataType::NoOp);
    }

    {
        // From a color space to the same one, identified by its name and an alias.

        let mut cst_alias = ColorSpaceTransform::new();
        cst_alias.set_src(src);
        cst_alias.set_dst("aliasToRef");

        let ops = build(&config, &cst_alias, TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 0);
    }

    {
        // Test that data color space will not create a color space conversion unless the
        // color space transform forces data to be processed.

        cs_scene_to_ref.set_is_data(true);
        config.add_color_space(&cs_scene_to_ref).unwrap();

        let mut ops = build(&config, &cst, TransformDirection::Forward).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 0);

        config.set_processor_cache_flags(ProcessorCacheFlags::OFF); // Cache disabled
        let proc = config.processor(&Transform::from(cst.clone())).unwrap();
        assert_eq!(proc.num_transforms(), 0);

        cst.set_data_bypass(false);
        let mut ops = build(&config, &cst, TransformDirection::Forward).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 4);

        // Some of the ops are no-ops, processor has in fact 2 transforms.
        let proc = config.processor(&Transform::from(cst.clone())).unwrap();
        // Remove the no-ops, since they are useless here.
        let proc = proc.optimized_processor(OptimizationFlags::NONE).unwrap();
        assert_eq!(proc.num_transforms(), 2);

        // Similar test with color space, data by-pass can't be controlled.
        let proc = config.processor_between(src, dst).unwrap();
        assert_eq!(proc.num_transforms(), 0);

        // Restore default data flags.
        cs_scene_to_ref.set_is_data(false);
        config.add_color_space(&cs_scene_to_ref).unwrap();
        cst.set_data_bypass(true);

        // Same with destination color space.
        cs_scene_from_ref.set_is_data(true);
        config.add_color_space(&cs_scene_from_ref).unwrap();

        let mut ops = build(&config, &cst, TransformDirection::Forward).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 0);

        let proc = config.processor(&Transform::from(cst.clone())).unwrap();
        assert_eq!(proc.num_transforms(), 0);

        cst.set_data_bypass(false);
        let mut ops = build(&config, &cst, TransformDirection::Forward).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 4);

        let proc = config.processor(&Transform::from(cst.clone())).unwrap();
        // Remove the no-ops, since they are useless here.
        let proc = proc.optimized_processor(OptimizationFlags::NONE).unwrap();
        assert_eq!(proc.num_transforms(), 2);

        // Similar test with color space, data bypass can't be controlled.
        let proc = config.processor_between(src, dst).unwrap();
        assert_eq!(proc.num_transforms(), 0);

        // Restore default data flags.
        cs_scene_from_ref.set_is_data(false);
        config.add_color_space(&cs_scene_from_ref).unwrap();
        cst.set_data_bypass(true);
    }

    {
        // Test in other direction.
        // Expecting destination-from reference inverted + source-to reference inverted.

        let mut ops = build(&config, &cst, TransformDirection::Inverse).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 4);

        assert_eq!(op_type(&ops, 0), OpDataType::NoOp);

        // Dst CS to reference.
        assert_eq!(op_type(&ops, 1), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 1), FixedFunctionOpStyle::AcesGlow03Inv);

        // Reference to src CS.
        assert_eq!(op_type(&ops, 2), OpDataType::Matrix);
        let mat_data = matrix_at(&ops, 2);
        // Not finalized, opData is still inverse.
        assert_eq!(mat_data.get_direction(), TransformDirection::Inverse);
        check_offsets(mat_data, &offset);

        assert_eq!(op_type(&ops, 3), OpDataType::NoOp);

        // Finalize converts invert matrix into forward matrix.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::NONE).unwrap();
        // No-ops are gone.
        assert_eq!(ops.len(), 2);
        assert_eq!(op_type(&ops, 1), OpDataType::Matrix);
        let mat_data = matrix_at(&ops, 1);
        // Matrix is now forward.
        assert_eq!(mat_data.get_direction(), TransformDirection::Forward);
        // Offset is inverted.
        check_offsets(mat_data, &offset.map(|o| -o));
    }

    {
        // Color space to reference ops: color space only defines from_reference, expecting the
        // inverse of that transform. Don't bypass data color spaces.

        let mut ops = OpVec::new();
        build_color_space_to_reference_ops(
            &mut ops,
            &config,
            &config.current_context().get(),
            &cs_scene_from_ref,
            false,
        )
        .unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(op_type(&ops, 1), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 1), FixedFunctionOpStyle::AcesGlow03Inv);
    }

    {
        // Color space from reference ops: color space defines from_reference, expecting that
        // transform.

        let mut ops = OpVec::new();
        build_color_space_from_reference_ops(
            &mut ops,
            &config,
            &config.current_context().get(),
            &cs_scene_from_ref,
            true,
        )
        .unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(op_type(&ops, 0), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 0), FixedFunctionOpStyle::AcesGlow03Fwd);
    }

    {
        // Color space with both to_scene_reference and from_scene_reference transform defined. No
        // inversion is made.

        let mut cs_scene_both = cs_scene_from_ref.clone();
        ff.set_style(FixedFunctionStyle::AcesGlow10).unwrap();
        cs_scene_both
            .set_transform(
                Some(&Transform::from(ff.clone())),
                ColorSpaceDirection::ToReference,
            )
            .unwrap();

        // Make it a data colorspace.
        cs_scene_both.set_is_data(true);

        // If data is bypassed, nothing is created.
        let context = config.current_context().get();
        let mut ops = OpVec::new();
        build_color_space_from_reference_ops(&mut ops, &config, &context, &cs_scene_both, true)
            .unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 0);

        // If data is not bypassed ops are created.
        build_color_space_from_reference_ops(&mut ops, &config, &context, &cs_scene_both, false)
            .unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(op_type(&ops, 0), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 0), FixedFunctionOpStyle::AcesGlow03Fwd);

        // Remove the data property and try in the other direction.
        cs_scene_both.set_is_data(false);
        ops.clear();
        build_color_space_to_reference_ops(&mut ops, &config, &context, &cs_scene_both, true)
            .unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(op_type(&ops, 1), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 1), FixedFunctionOpStyle::AcesGlow10Fwd);
    }

    // Replace the 2 color spaces by display-referred color spaces.
    let mut cs_display_to_ref = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    cs_display_to_ref.set_name(src);
    cs_display_to_ref
        .set_transform(Some(&mat), ColorSpaceDirection::ToReference)
        .unwrap();
    config.add_color_space(&cs_display_to_ref).unwrap();

    let mut cs_display_from_ref = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    cs_display_from_ref.set_name(dst);
    cs_display_from_ref
        .set_transform(
            Some(&Transform::from(ff.clone())),
            ColorSpaceDirection::FromReference,
        )
        .unwrap();
    config.add_color_space(&cs_display_from_ref).unwrap();

    // Because there are display-referred color spaces, there need to be a view transform.
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    vt.set_transform(Some(&mat), ViewTransformDirection::FromReference)
        .unwrap();
    config.add_view_transform(&vt).unwrap();

    // 3 color spaces including "raw".
    assert_eq!(config.num_color_spaces(), 3);
    config.validate().unwrap();

    // cst is now from csDisplayToRef to csDisplayFromRef.
    {
        // Still getting 4 ops. View transform is not used.

        let mut ops = build(&config, &cst, TransformDirection::Forward).unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 4);

        assert_eq!(op_type(&ops, 0), OpDataType::NoOp);
        assert_eq!(op_type(&ops, 1), OpDataType::Matrix);
        assert_eq!(op_type(&ops, 2), OpDataType::FixedFunction);
        assert_eq!(op_type(&ops, 3), OpDataType::NoOp);
    }

    {
        // Errors.

        let mut cst = ColorSpaceTransform::new();
        cst.set_src("source_missing");
        cst.set_dst(dst);

        check_throw_what(
            build(&config, &cst, TransformDirection::Forward),
            "Color space 'source_missing' could not be found",
        );

        let mut cst = ColorSpaceTransform::new();
        cst.set_src(src);
        cst.set_dst("destination_missing");

        check_throw_what(
            build(&config, &cst, TransformDirection::Forward),
            "Color space 'destination_missing' could not be found",
        );
    }
}

/// Port of `OCIO_ADD_TEST(ColorSpaceTransform, build_reference_conversion_ops)` @ v2.5.2.
#[test]
fn build_reference_conversion_ops_test() {
    let _env = EnvGuard::new();
    let scn = "scene";

    let mut config = (*Config::create_raw().unwrap()).clone();
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    cs.set_name(scn);
    let ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow03, &[]).unwrap();
    cs.set_transform(
        Some(&Transform::from(ff)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    config.add_color_space(&cs).unwrap();

    config.add_display_view("display", "view", scn, "").unwrap();

    config.validate().unwrap();

    let conversion = |config: &Config, src, dst| {
        let mut ops = OpVec::new();
        build_reference_conversion_ops(&mut ops, config, &config.current_context().get(), src, dst)
            .map(|()| ops)
    };

    //
    // No view transform.
    //
    {
        let ops = conversion(
            &config,
            ReferenceSpaceType::Scene,
            ReferenceSpaceType::Scene,
        )
        .unwrap();
        assert_eq!(ops.len(), 0);

        let ops = conversion(
            &config,
            ReferenceSpaceType::Display,
            ReferenceSpaceType::Display,
        )
        .unwrap();
        assert_eq!(ops.len(), 0);

        check_throw_what(
            conversion(
                &config,
                ReferenceSpaceType::Scene,
                ReferenceSpaceType::Display,
            ),
            "no view transform between the main scene-referred space and the display-referred \
             space",
        );
        check_throw_what(
            conversion(
                &config,
                ReferenceSpaceType::Display,
                ReferenceSpaceType::Scene,
            ),
            "no view transform between the main scene-referred space and the display-referred \
             space",
        );
    }

    //
    // Add scene-referred view transform.
    //

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    let mut mat = MatrixTransform::new();
    let offset: [f64; 4] = [0., 0.1, 0.2, 0.];
    mat.set_offset(&offset);
    vt.set_transform(
        Some(&Transform::from(mat)),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config.add_view_transform(&vt).unwrap();

    config.validate().unwrap();

    {
        let mut ops = conversion(
            &config,
            ReferenceSpaceType::Scene,
            ReferenceSpaceType::Display,
        )
        .unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 1);

        // Scene reference to display reference.
        assert_eq!(op_type(&ops, 0), OpDataType::Matrix);
        check_offsets(matrix_at(&ops, 0), &offset);
    }

    {
        let mut ops = conversion(
            &config,
            ReferenceSpaceType::Display,
            ReferenceSpaceType::Scene,
        )
        .unwrap();
        ops.validate().unwrap();
        assert_eq!(ops.len(), 1);

        // Dispaly reference to scene reference.
        assert_eq!(op_type(&ops, 0), OpDataType::Matrix);
        let mat_data = matrix_at(&ops, 0);
        assert_eq!(mat_data.get_direction(), TransformDirection::Inverse);
        check_offsets(mat_data, &offset);
    }
}

/// Port of `OCIO_ADD_TEST(ColorSpaceTransform, build_colorspace_ops_with_reference_conversion)`
/// @ v2.5.2.
#[test]
fn build_colorspace_ops_with_reference_conversion() {
    let _env = EnvGuard::new();
    let scn = "scene";

    let mut config = (*Config::create_raw().unwrap()).clone();
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    cs.set_name(scn);
    let ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow03, &[]).unwrap();
    cs.set_transform(
        Some(&Transform::from(ff)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    config.add_color_space(&cs).unwrap();

    config.add_display_view("display", "view", scn, "").unwrap();

    // Add scene-referred view transform.
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    let mut mat = MatrixTransform::new();
    let offset: [f64; 4] = [0., 0.1, 0.2, 0.];
    mat.set_offset(&offset);
    vt.set_transform(
        Some(&Transform::from(mat)),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config.add_view_transform(&vt).unwrap();

    config.validate().unwrap();

    //
    // Add display-referred color space.
    //

    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    let dsp = "display";
    cs.set_name(dsp);
    let log = LogTransform::new();
    cs.set_transform(
        Some(&Transform::from(log)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    let mut cst = ColorSpaceTransform::new();
    cst.set_src(scn);
    cst.set_dst(dsp);

    //
    // Color space to color space with reference space conversion.
    //

    {
        let ops = build(&config, &cst, TransformDirection::Forward).unwrap();

        // Expecting 5 transforms (including 2 no-ops).
        assert_eq!(ops.len(), 5);

        assert_eq!(op_type(&ops, 0), OpDataType::NoOp);

        // CS to scene reference.
        assert_eq!(op_type(&ops, 1), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 1), FixedFunctionOpStyle::AcesGlow03Inv);

        // Scene reference to display reference.
        assert_eq!(op_type(&ops, 2), OpDataType::Matrix);
        check_offsets(matrix_at(&ops, 2), &offset);

        // Display reference to CS.
        assert_eq!(op_type(&ops, 3), OpDataType::Log);
        let log_data = log_at(&ops, 3);
        assert_eq!(log_data.base(), 2.0);
        assert_eq!(log_data.direction(), TransformDirection::Forward);

        assert_eq!(op_type(&ops, 4), OpDataType::NoOp);
    }

    //
    // Same test in inverse direction.
    //

    {
        let mut ops = build(&config, &cst, TransformDirection::Inverse).unwrap();
        ops.validate().unwrap();

        // Expecting 5 transforms (including 2 no-ops).
        assert_eq!(ops.len(), 5);

        assert_eq!(op_type(&ops, 0), OpDataType::NoOp);

        assert_eq!(op_type(&ops, 1), OpDataType::Log);
        let log_data = log_at(&ops, 1);
        assert_eq!(log_data.base(), 2.0);
        assert_eq!(log_data.direction(), TransformDirection::Inverse);

        // Display reference to scene reference. The view transform only defines the other
        // direction. Expecting the inverse of that transform.
        assert_eq!(op_type(&ops, 2), OpDataType::Matrix);
        let mat_data = matrix_at(&ops, 2);
        assert_eq!(mat_data.get_direction(), TransformDirection::Inverse);
        check_offsets(mat_data, &offset);

        assert_eq!(op_type(&ops, 3), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 3), FixedFunctionOpStyle::AcesGlow03Fwd);

        assert_eq!(op_type(&ops, 4), OpDataType::NoOp);
    }

    //
    // Add a to_scene_reference transform to the view transform.
    //

    let exp = ExponentTransform::new();
    vt.set_transform(
        Some(&Transform::from(exp)),
        ViewTransformDirection::ToReference,
    )
    .unwrap();
    config.add_view_transform(&vt).unwrap();
    {
        let mut ops = build(&config, &cst, TransformDirection::Inverse).unwrap();
        ops.validate().unwrap();

        // Expecting 5 transforms (including 2 no-ops).
        assert_eq!(ops.len(), 5);

        assert_eq!(op_type(&ops, 0), OpDataType::NoOp);

        assert_eq!(op_type(&ops, 1), OpDataType::Log);
        let log_data = log_at(&ops, 1);
        assert_eq!(log_data.base(), 2.0);
        assert_eq!(log_data.direction(), TransformDirection::Inverse);

        // Display reference to scene reference. (ExponentTransform is implemented using a
        // GammaOpData from v2.)
        assert_eq!(op_type(&ops, 2), OpDataType::Gamma);
        match &**ops[2].data() {
            OpData::Gamma(data) => assert!(data.is_identity().unwrap()),
            other => panic!("not a Gamma op: {other:?}"),
        }

        assert_eq!(op_type(&ops, 3), OpDataType::FixedFunction);
        assert_eq!(ff_style_at(&ops, 3), FixedFunctionOpStyle::AcesGlow03Fwd);

        assert_eq!(op_type(&ops, 4), OpDataType::NoOp);
    }
}

/// Port of `OCIO_ADD_TEST(ColorSpaceTransform, context_variables)` @ v2.5.2.
#[test]
fn context_variables() {
    let _env = EnvGuard::new();
    let mut used_context_vars = Context::new();

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    let test_files_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/OpenColorIO/tests/data/files"
    );
    cfg.set_search_path(test_files_dir);
    let mut ctx = (*cfg.current_context().get()).clone();

    let mut matrix = MatrixTransform::new();
    let offset4: [f64; 4] = [0.1, 0.2, 0.3, 0.];
    matrix.set_offset(&offset4);
    let matrix = Transform::from(matrix);

    let mut cs1 = ColorSpace::new();
    cs1.set_name("cs1");
    cs1.set_transform(Some(&matrix), ColorSpaceDirection::ToReference)
        .unwrap();
    cfg.add_color_space(&cs1).unwrap();

    let mut cs2 = ColorSpace::new();
    cs2.set_name("cs2");
    cs2.set_transform(Some(&matrix), ColorSpaceDirection::ToReference)
        .unwrap();
    cfg.add_color_space(&cs2).unwrap();

    let mut cst = ColorSpaceTransform::new();
    cst.set_src("cs1");
    cst.set_dst("cs2");

    let mut cs3 = ColorSpace::new();
    cs3.set_name("cs3");
    cs3.set_transform(
        Some(&Transform::from(cst.clone())),
        ColorSpaceDirection::ToReference,
    )
    .unwrap();
    cfg.add_color_space(&cs3).unwrap();

    cfg.validate().unwrap();

    // Case 1 - No context variables.

    assert!(
        !collect_color_space_transform_context_variables(&cfg, &ctx, &cst, &mut used_context_vars)
            .unwrap()
    );
    assert_eq!(0, used_context_vars.num_string_vars());

    // Case 2 - The source color space name is now a context variable.

    ctx.set_string_var("ENV1", Some(b"cs1"));
    cst.set_src("$ENV1");
    let mut used_context_vars = Context::new(); // New & empty instance.
    assert!(
        collect_color_space_transform_context_variables(&cfg, &ctx, &cst, &mut used_context_vars)
            .unwrap()
    );
    assert_eq!(1, used_context_vars.num_string_vars());
    assert_eq!(b"ENV1", used_context_vars.string_var_name_by_index(0));
    assert_eq!(b"cs1", used_context_vars.string_var_by_index(0));

    // Case 3 - A context variable exists but is not used.

    cst.set_src("cs1");
    let mut used_context_vars = Context::new(); // New & empty instance.
    assert!(
        !collect_color_space_transform_context_variables(&cfg, &ctx, &cst, &mut used_context_vars)
            .unwrap()
    );
    assert_eq!(0, used_context_vars.num_string_vars());

    // Case 4 - Context variable indirectly used.

    ctx.set_string_var("ENV1", Some(b"exposure_contrast_linear.ctf"));
    let mut file = FileTransform::new();
    file.set_src("$ENV1");
    let file = Transform::from(file);
    let mut cs4 = ColorSpace::new();
    cs4.set_name("cs4");
    cs4.set_transform(Some(&file), ColorSpaceDirection::ToReference)
        .unwrap();
    cfg.add_color_space(&cs4).unwrap();

    // 'cst' now uses 'cs4' which is a FileTransform where the file name is a context variable.
    cst.set_src("cs4");
    let mut used_context_vars = Context::new(); // New & empty instance.
    assert!(
        collect_color_space_transform_context_variables(&cfg, &ctx, &cst, &mut used_context_vars)
            .unwrap()
    );
    assert_eq!(1, used_context_vars.num_string_vars());
    assert_eq!(b"ENV1", used_context_vars.string_var_name_by_index(0));
    assert_eq!(
        b"exposure_contrast_linear.ctf",
        used_context_vars.string_var_by_index(0)
    );

    // Case 5 - Context variable indirectly used via a NamedTransform.

    let mut named_transform = NamedTransform::new();
    named_transform.set_name("nt");
    let mut file2 = FileTransform::new();
    file2.set_src("$ENV1");
    named_transform
        .set_transform(Some(&Transform::from(file2)), TransformDirection::Forward)
        .unwrap();
    cfg.add_named_transform(&named_transform).unwrap();

    // 'cst' now uses 'nt' which is a NamedTransform whose transform uses a context variable.
    cst.set_src("nt");
    let mut used_context_vars = Context::new(); // New & empty instance.
    assert!(
        collect_color_space_transform_context_variables(&cfg, &ctx, &cst, &mut used_context_vars)
            .unwrap()
    );
    assert_eq!(1, used_context_vars.num_string_vars());
    assert_eq!(b"ENV1", used_context_vars.string_var_name_by_index(0));
    assert_eq!(
        b"exposure_contrast_linear.ctf",
        used_context_vars.string_var_by_index(0)
    );
}
