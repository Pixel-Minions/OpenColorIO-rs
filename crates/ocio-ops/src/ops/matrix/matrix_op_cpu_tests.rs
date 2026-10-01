// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/matrix/MatrixOpCPU_tests.cpp` @ v2.5.2.

use super::*;

/// The renderer's type, which upstream checks with `dynamic_cast`: the name its `Debug` output
/// starts with.
fn renderer_type(op: &Arc<dyn CpuOp>) -> String {
    let debug = format!("{op:?}");
    debug[..debug.find(' ').unwrap_or(debug.len())].to_string()
}

/// Port of `OCIO_ADD_TEST(MatrixOpCPU, scale_renderer)` @ v2.5.2.
#[test]
fn scale_renderer() {
    let mat = MatrixOpData::create_diagonal_matrix(2.0);

    let op = get_matrix_renderer(&mat).unwrap();
    assert_eq!(renderer_type(&op), "ScaleRenderer");

    let mut rgba = [4.0f32, 3.0, 2.0, 1.0];

    op.apply(&mut rgba);

    assert_eq!(rgba[0], 8.0);
    assert_eq!(rgba[1], 6.0);
    assert_eq!(rgba[2], 4.0);
    assert_eq!(rgba[3], 2.0);
}

/// Port of `OCIO_ADD_TEST(MatrixOpCPU, scale_with_offset_renderer)` @ v2.5.2.
#[test]
fn scale_with_offset_renderer() {
    let mut mat = MatrixOpData::create_diagonal_matrix(2.0);

    mat.set_offset_value(0, 1.0f32.into()).unwrap();
    mat.set_offset_value(1, 2.0f32.into()).unwrap();
    mat.set_offset_value(2, 3.0f32.into()).unwrap();
    mat.set_offset_value(3, 4.0f32.into()).unwrap();

    let op = get_matrix_renderer(&mat).unwrap();
    assert_eq!(renderer_type(&op), "ScaleWithOffsetRenderer");

    let mut rgba = [4.0f32, 3.0, 2.0, 1.0];

    op.apply(&mut rgba);

    assert_eq!(rgba[0], 9.0);
    assert_eq!(rgba[1], 8.0);
    assert_eq!(rgba[2], 7.0);
    assert_eq!(rgba[3], 6.0);
}

/// Port of `OCIO_ADD_TEST(MatrixOpCPU, matrix_with_offset_renderer)` @ v2.5.2.
#[test]
fn matrix_with_offset_renderer() {
    let mut mat = MatrixOpData::create_diagonal_matrix(2.0);

    // set offset
    mat.set_offset_value(0, 1.0f32.into()).unwrap();
    mat.set_offset_value(1, 2.0f32.into()).unwrap();
    mat.set_offset_value(2, 3.0f32.into()).unwrap();
    mat.set_offset_value(3, 4.0f32.into()).unwrap();

    // make not diag
    mat.set_array_value(3, 0.5f32.into());

    let op = get_matrix_renderer(&mat).unwrap();
    assert_eq!(renderer_type(&op), "MatrixWithOffsetRenderer");

    let mut rgba = [4.0f32, 3.0, 2.0, 1.0];

    op.apply(&mut rgba);

    assert_eq!(rgba[0], 9.5);
    assert_eq!(rgba[1], 8.0);
    assert_eq!(rgba[2], 7.0);
    assert_eq!(rgba[3], 6.0);
}

/// Port of `OCIO_ADD_TEST(MatrixOpCPU, matrix_renderer)` @ v2.5.2.
#[test]
fn matrix_renderer() {
    let mut mat = MatrixOpData::create_diagonal_matrix(2.0);

    // Make not diagonal.
    mat.set_array_value(3, 0.5f32.into());

    let op = get_matrix_renderer(&mat).unwrap();
    assert_eq!(renderer_type(&op), "MatrixRenderer");

    let mut rgba = [4.0f32, 3.0, 2.0, 1.0];

    op.apply(&mut rgba);

    assert_eq!(rgba[0], 8.5);
    assert_eq!(rgba[1], 6.0);
    assert_eq!(rgba[2], 4.0);
    assert_eq!(rgba[3], 2.0);
}
