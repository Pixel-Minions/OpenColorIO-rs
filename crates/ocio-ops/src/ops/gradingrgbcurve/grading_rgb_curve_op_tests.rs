// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingrgbcurve/GradingRGBCurveOp_tests.cpp @ v2.5.2. Its
//! `create_transform` and `build_ops` need the `GradingRGBCurveTransform` (Phase 3).

use super::*;
use crate::open_color_types::GradingStyle;

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOp, create)` @ v2.5.2. Upstream's ops share the data
/// it changes after making the first op; here each op gets the data passed (a copy), and the
/// second one the data made dynamic.
#[test]
fn create() {
    let direction = TransformDirection::Forward;
    let data = GradingRgbCurveOpData::new(GradingStyle::Log);
    let mut ops = OpVec::new();

    create_grading_rgb_curve_op(&mut ops, data.clone(), direction);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<GradingRGBCurveOp>");
    assert!(ops[0].is_identity().unwrap());
    assert!(ops[0].is_no_op().unwrap());

    data.get_dynamic_property_internal().make_dynamic();
    create_grading_rgb_curve_op(&mut ops, data, direction);
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[1].get_info(), "<GradingRGBCurveOp>");
    assert!(!ops[1].is_identity().unwrap());
    assert!(!ops[1].is_no_op().unwrap());
}
