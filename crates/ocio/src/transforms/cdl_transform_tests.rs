// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the CDL transform: the tests of `tests/cpu/transforms/CDLTransform_tests.cpp` @
//! v2.5.2 that need no file (`equality`, `buildops`, `description`, `style`,
//! `apply_optimize_simplify`), and `CDLOp create_transform` (tests/cpu/ops/cdl/CDLOp_tests.cpp
//! @ v2.5.2), which tests `CreateCDLTransform` and needed the transform. The file tests come
//! with the CDL readers (Phase 4). The text, the validation, the equality and the ops built
//! are compared with the wheel's in `tests/cdl_transform_oracle.rs`.

use std::sync::Arc;

use ocio_ops::format_metadata::{METADATA_DESCRIPTION, METADATA_ID};
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::OptimizationFlags;
use ocio_ops::ops::cdl::CdlOpStyle;

use super::*;
use crate::transform::Transform;

/// Port of `OCIO_ADD_TEST(CDLTransform, equality)` @ v2.5.2.
#[test]
fn equality() {
    let cdl1 = CdlTransform::new();
    let mut cdl2 = CdlTransform::new();

    assert!(cdl1.equals(&cdl1));
    assert!(cdl1.equals(&cdl2));
    assert!(cdl2.equals(&cdl1));

    let mut cdl3 = CdlTransform::new();
    cdl3.set_sat(cdl3.sat() + f64::from(0.002f32));

    assert!(!cdl1.equals(&cdl3));
    assert!(!cdl2.equals(&cdl3));
    assert!(cdl3.equals(&cdl3));

    cdl2.set_style(CdlStyle::Asc);
    assert!(!cdl1.equals(&cdl2));
}

/// The version 1 config, as upstream's tests make it with `setMajorVersion(1)`; upstream starts
/// from `Config::Create()`, the port from the raw config, only the major version matters to
/// `BuildCDLOp`.
fn config_of_version(version: u32) -> Arc<Config> {
    let mut config = Config::create_raw().unwrap();
    Arc::get_mut(&mut config)
        .unwrap()
        .set_major_version(version)
        .unwrap();
    config
}

/// Whether `op` holds data of `op_type`.
fn is_type(op: &Op, op_type: OpDataType) -> bool {
    op.data().get_type() == op_type
}

/// Port of `OCIO_ADD_TEST(CDLTransform, buildops)` @ v2.5.2.
#[test]
fn buildops() {
    let mut cdl = CdlTransform::new();

    // For a v1 config, a CDL uses an exponent and two matrix ops rather than the CDL op that
    // was introduced in v2.
    let mut config = config_of_version(1);

    let mut ops = OpVec::new();
    build_cdl_op(&mut ops, &config, &cdl, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 3);
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 0);

    ops.clear();
    let power = [1.1, 1.0, 1.0];
    cdl.set_power(&power);
    build_cdl_op(&mut ops, &config, &cdl, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 3);
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);
    assert!(is_type(&ops[0], OpDataType::Exponent));

    ops.clear();
    cdl.set_sat(1.5);
    build_cdl_op(&mut ops, &config, &cdl, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 3);
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(is_type(&ops[0], OpDataType::Exponent));
    assert!(is_type(&ops[1], OpDataType::Matrix));

    ops.clear();
    let offset = [0.0, 0.1, 0.0];
    cdl.set_offset(&offset);
    build_cdl_op(&mut ops, &config, &cdl, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 3);
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 3);
    assert!(is_type(&ops[0], OpDataType::Matrix));
    assert!(is_type(&ops[1], OpDataType::Exponent));
    assert!(is_type(&ops[2], OpDataType::Matrix));

    // Testing v2 onward behavior.
    Arc::get_mut(&mut config)
        .unwrap()
        .set_major_version(2)
        .unwrap();
    ops.clear();
    build_cdl_op(&mut ops, &config, &cdl, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    assert!(is_type(&ops[0], OpDataType::Cdl));
}

/// Port of `OCIO_ADD_TEST(CDLTransform, description)` @ v2.5.2. Upstream's null description is
/// the empty one here (a C string ends at its first NUL; a slice can't be null).
#[test]
fn description() {
    let mut cdl = CdlTransform::new();

    let id = b"TestCDL";
    cdl.set_id(id);

    let initial_desc = cdl.first_sop_description().to_vec();
    assert!(initial_desc.is_empty());

    let metadata = cdl.format_metadata_mut();
    metadata
        .add_child_element(Some(METADATA_DESCRIPTION), Some(b"Desc"))
        .unwrap();
    metadata
        .add_child_element(Some(METADATA_INPUT_DESCRIPTION), Some(b"Input Desc"))
        .unwrap();
    let sop_desc = b"SOP Desc";
    metadata
        .add_child_element(Some(METADATA_SOP_DESCRIPTION), Some(sop_desc))
        .unwrap();
    metadata
        .add_child_element(Some(METADATA_SAT_DESCRIPTION), Some(b"Sat Desc"))
        .unwrap();
    let sop_other = b"Additional SOP";
    metadata
        .add_child_element(Some(METADATA_SOP_DESCRIPTION), Some(sop_other))
        .unwrap();

    assert_eq!(cdl.format_metadata().get_num_children_elements(), 5);
    assert_eq!(cdl.first_sop_description(), sop_desc);

    let new_sop_desc = b"SOP Desc New";
    cdl.set_first_sop_description(new_sop_desc);

    assert_eq!(cdl.first_sop_description(), new_sop_desc);
    // The first SOP_DESCRIPTION has been replaced.
    assert_eq!(cdl.format_metadata().get_num_children_elements(), 5);

    // Null description is removing the first SOP_DESCRIPTION.
    cdl.set_first_sop_description(b"");
    assert_eq!(cdl.format_metadata().get_num_children_elements(), 4);
    // There is still a SOP description because there were 2.
    assert_eq!(cdl.first_sop_description(), sop_other);
    // Removing the second one.
    cdl.set_first_sop_description(b"");
    assert_eq!(cdl.format_metadata().get_num_children_elements(), 3);
    // SOP description is now gone.
    assert_eq!(cdl.first_sop_description(), b"");
}

/// The CDL op data of the op `op`.
fn cdl_data(op: &Op) -> &CdlOpData {
    match &**op.data() {
        OpData::Cdl(data) => data,
        other => panic!("not a CDL op: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(CDLTransform, style)` @ v2.5.2.
#[test]
fn style() {
    let mut cdl = CdlTransform::new();
    assert_eq!(cdl.style(), CdlStyle::TRANSFORM_DEFAULT);
    assert_eq!(cdl.style(), CdlStyle::NoClamp);

    cdl.set_style(CdlStyle::Asc);
    assert_eq!(cdl.style(), CdlStyle::Asc);
    cdl.set_style(CdlStyle::NoClamp);
    assert_eq!(cdl.style(), CdlStyle::NoClamp);

    let config = Config::create_raw().unwrap();
    let built = |cdl: &CdlTransform, dir| {
        let mut ops = OpVec::new();
        build_cdl_op(&mut ops, &config, cdl, dir).unwrap();
        assert_eq!(ops.len(), 1);
        cdl_data(&ops[0]).get_style()
    };
    assert_eq!(
        built(&cdl, TransformDirection::Forward),
        CdlOpStyle::NoClampFwd
    );
    assert_eq!(
        built(&cdl, TransformDirection::Inverse),
        CdlOpStyle::NoClampRev
    );

    cdl.set_style(CdlStyle::Asc);
    assert_eq!(cdl.style(), CdlStyle::Asc);

    assert_eq!(
        built(&cdl, TransformDirection::Forward),
        CdlOpStyle::V1_2Fwd
    );
    assert_eq!(
        built(&cdl, TransformDirection::Inverse),
        CdlOpStyle::V1_2Rev
    );
}

/// `CDL_DATA_1` (tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66 @ v2.5.2).
mod cdl_data_1 {
    pub(super) const SLOPE: [f64; 3] = [1.35, 1.1, 0.071];
    pub(super) const OFFSET: [f64; 3] = [0.05, -0.23, 0.11];
    pub(super) const POWER: [f64; 3] = [0.93, 0.81, 1.27];
    pub(super) const SATURATION: f64 = 1.23;
}

/// The op of a `CDLOpData` of `style` and `CDL_DATA_1`, as upstream's test makes it with
/// `std::make_shared<CDLOp>(cdlData)`: a forward CDL op holding the data.
fn cdl_op_of(style: CdlOpStyle, id: Option<&[u8]>) -> OpVec {
    let mut cdl = CdlOpData::new(
        style,
        ChannelParams::new(
            cdl_data_1::SLOPE[0],
            cdl_data_1::SLOPE[1],
            cdl_data_1::SLOPE[2],
        ),
        ChannelParams::new(
            cdl_data_1::OFFSET[0],
            cdl_data_1::OFFSET[1],
            cdl_data_1::OFFSET[2],
        ),
        ChannelParams::new(
            cdl_data_1::POWER[0],
            cdl_data_1::POWER[1],
            cdl_data_1::POWER[2],
        ),
        cdl_data_1::SATURATION,
    )
    .unwrap();
    if let Some(id) = id {
        cdl.get_format_metadata_mut()
            .add_attribute(Some(METADATA_ID), Some(id))
            .unwrap();
    }
    let mut ops = OpVec::new();
    create_cdl_op(&mut ops, cdl, TransformDirection::Forward);
    ops
}

/// The CDL transform `CreateCDLTransform` makes of the only op of `ops`.
fn transform_of(ops: &OpVec) -> CdlTransform {
    let mut group = GroupTransform::new();
    create_cdl_transform(&mut group, &ops[0]).unwrap();
    assert_eq!(group.num_transforms(), 1);
    match group.transform(0).unwrap() {
        Transform::Cdl(t) => t.clone(),
        other => panic!("not a CDLTransform: {other:?}"),
    }
}

/// The values of `CDL_DATA_1` read back.
fn check_values(cdl_transform: &CdlTransform) {
    let slope = cdl_transform.slope();
    assert_eq!(slope[0], cdl_data_1::SLOPE[0]);
    assert_eq!(slope[1], cdl_data_1::SLOPE[1]);
    assert_eq!(slope[2], cdl_data_1::SLOPE[2]);

    let offset = cdl_transform.offset();
    assert_eq!(offset[0], cdl_data_1::OFFSET[0]);
    assert_eq!(offset[1], cdl_data_1::OFFSET[1]);
    assert_eq!(offset[2], cdl_data_1::OFFSET[2]);

    let power = cdl_transform.power();
    assert_eq!(power[0], cdl_data_1::POWER[0]);
    assert_eq!(power[1], cdl_data_1::POWER[1]);
    assert_eq!(power[2], cdl_data_1::POWER[2]);

    assert_eq!(cdl_transform.sat(), cdl_data_1::SATURATION);
}

/// The styles of the two ops `BuildCDLOp` makes of `cdl_transform`, forward then inverse.
fn back_to_ops(config: &Config, cdl_transform: &CdlTransform) -> (CdlOpStyle, CdlOpStyle) {
    let mut ops = OpVec::new();
    build_cdl_op(&mut ops, config, cdl_transform, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    build_cdl_op(&mut ops, config, cdl_transform, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].data().get_type(), OpDataType::Cdl);
    assert_eq!(ops[1].data().get_type(), OpDataType::Cdl);
    (cdl_data(&ops[0]).get_style(), cdl_data(&ops[1]).get_style())
}

/// Port of `OCIO_ADD_TEST(CDLOp, create_transform)` @ v2.5.2. Upstream starts from
/// `Config::Create()`; the port from the raw config (only the major version matters).
#[test]
fn create_transform() {
    let config = Config::create_raw().unwrap();
    {
        // Forward direction.
        let ops = cdl_op_of(CdlOpStyle::V1_2Fwd, Some(b"Test look: 01-A."));
        let cdl_transform = transform_of(&ops);

        let metadata = cdl_transform.format_metadata();
        assert_eq!(metadata.get_num_attributes(), 1);
        assert_eq!(metadata.get_attribute_name(0), METADATA_ID);
        assert_eq!(metadata.get_attribute_value(0), b"Test look: 01-A.");
        assert_eq!(cdl_transform.direction(), TransformDirection::Forward);

        check_values(&cdl_transform);
        assert_eq!(cdl_transform.style(), CdlStyle::Asc);

        // Back to op.
        let (style0, style1) = back_to_ops(&config, &cdl_transform);
        assert_eq!(style0, CdlOpStyle::V1_2Fwd);
        assert_eq!(style1, CdlOpStyle::V1_2Rev);
    }
    {
        // Inverse direction.
        let ops = cdl_op_of(CdlOpStyle::V1_2Rev, None);
        let cdl_transform = transform_of(&ops);

        assert_eq!(cdl_transform.direction(), TransformDirection::Inverse);
        assert_eq!(cdl_transform.style(), CdlStyle::Asc);

        check_values(&cdl_transform);

        // Back to op.
        let (style0, style1) = back_to_ops(&config, &cdl_transform);
        assert_eq!(style0, CdlOpStyle::V1_2Rev);
        assert_eq!(style1, CdlOpStyle::V1_2Fwd);
    }
    {
        let ops = cdl_op_of(CdlOpStyle::NoClampFwd, None);
        let mut cdl_transform = transform_of(&ops);
        assert_eq!(cdl_transform.style(), CdlStyle::NoClamp);
        assert_eq!(cdl_transform.direction(), TransformDirection::Forward);
        cdl_transform.set_direction(TransformDirection::Inverse);
        assert_eq!(cdl_transform.direction(), TransformDirection::Inverse);

        check_values(&cdl_transform);

        // Back to op.
        let (style0, style1) = back_to_ops(&config, &cdl_transform);
        assert_eq!(style0, CdlOpStyle::NoClampRev);
        assert_eq!(style1, CdlOpStyle::NoClampFwd);
    }
}

/// `CreateCDLTransform` refuses an op of another type (which `CreateTransform`'s dispatch never
/// passes it) and adds nothing.
#[test]
fn create_cdl_transform_needs_a_cdl_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::noop::create_file_no_op(&mut ops, b"f");
    let mut group = GroupTransform::new();
    assert!(create_cdl_transform(&mut group, &ops[0]).is_err());
    assert_eq!(group.num_transforms(), 0);
}

/// `BuildCDLOp` validates the data itself in a version 2 config: a negative slope adds no op,
/// with the data's error, the one `validate` reports after its prefix.
#[test]
fn build_cdl_op_validates_the_data() {
    let mut cdl = CdlTransform::new();
    cdl.set_slope(&[-1.0, 1.0, 1.0]);
    let validated = cdl.validate().unwrap_err();

    let config = Config::create_raw().unwrap();
    let mut ops = OpVec::new();
    let built = build_cdl_op(&mut ops, &config, &cdl, TransformDirection::Forward).unwrap_err();
    assert_eq!(
        format!("CDLTransform validation failed: {}", built.message()),
        validated.message()
    );
    assert_eq!(ops.len(), 0);
}

/// Port of `OCIO_ADD_TEST(CDLTransform, apply_optimize_simplify)` @ v2.5.2.
#[test]
fn apply_optimize_simplify() {
    use ocio_testkit::upstream::check_close;

    use crate::config::Config;

    let mut cdl = CdlTransform::new();
    const SLOPE: [f64; 3] = [0.8, 0.9, 1.1];
    cdl.set_slope(&SLOPE);
    const OFFSET: [f64; 3] = [0.1, 0.05, -0.2];
    cdl.set_offset(&OFFSET);
    cdl.set_sat(1.23);
    let config = Config::create_raw().unwrap();
    let proc = config.processor(&Transform::Cdl(cdl.clone())).unwrap();

    // Verify that non-simplified and simplified cpu processors are equivalent.

    let no_simplify =
        OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::SIMPLIFY_OPS.0);
    let cpu = proc.optimized_cpu_processor(no_simplify).unwrap();
    const SOURCE: [f32; 3] = [-0.1, 0.5, 1.5];
    let mut pix_no_simplify = [SOURCE[0], SOURCE[1], SOURCE[2]];
    cpu.apply_rgb(&mut pix_no_simplify).unwrap();

    let cpu = proc
        .optimized_cpu_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    let mut pix_simplify = [SOURCE[0], SOURCE[1], SOURCE[2]];
    cpu.apply_rgb(&mut pix_simplify).unwrap();

    const ERROR: f32 = 2.0e-5;
    check_close(pix_no_simplify[0], pix_simplify[0], ERROR);
    check_close(pix_no_simplify[1], pix_simplify[1], ERROR);
    check_close(pix_no_simplify[2], pix_simplify[2], ERROR);

    // Same in inverse direction.

    cdl.set_direction(TransformDirection::Inverse);

    let proc = config.processor(&Transform::Cdl(cdl)).unwrap();
    let cpu = proc.optimized_cpu_processor(no_simplify).unwrap();
    pix_no_simplify[0] = SOURCE[0];
    pix_no_simplify[1] = SOURCE[1];
    pix_no_simplify[2] = SOURCE[2];
    cpu.apply_rgb(&mut pix_no_simplify).unwrap();

    let cpu = proc
        .optimized_cpu_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    pix_simplify[0] = SOURCE[0];
    pix_simplify[1] = SOURCE[1];
    pix_simplify[2] = SOURCE[2];
    cpu.apply_rgb(&mut pix_simplify).unwrap();

    check_close(pix_no_simplify[0], pix_simplify[0], ERROR);
    check_close(pix_no_simplify[1], pix_simplify[1], ERROR);
    check_close(pix_no_simplify[2], pix_simplify[2], ERROR);
}
