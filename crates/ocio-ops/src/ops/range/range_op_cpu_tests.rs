// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/range/RangeOpCPU_tests.cpp` @ v2.5.2.

use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::math_utils::is_nan;

/// `g_error` (RangeOpCPU_tests.cpp:15-18 @ v2.5.2).
const G_ERROR: f32 = 1e-7;

/// Whether the renderer's type name contains `name`, as upstream finds it in
/// `typeid(c).name()`: the type is the name its `Debug` output starts with.
fn has_type(op: &Arc<dyn CpuOp>, name: &str) -> bool {
    let debug = format!("{op:?}");
    debug[..debug.find('(').unwrap_or(debug.len())] == *name
}

/// `OCIO_CHECK_CLOSE(image[i], expected[i], g_error)` for every value.
#[track_caller]
fn check_all_close(image: &[f32], expected: &[f32]) {
    assert_eq!(image.len(), expected.len());
    for (&value, &expected) in image.iter().zip(expected) {
        check_close(value, expected, G_ERROR);
    }
}

/// The image of most of upstream's tests: 3 pixels.
const IMAGE3: [f32; 12] = [
    -0.50, -0.25, 0.50, 0.0, //
    0.75, 1.00, 1.25, 1.0, //
    1.25, 1.50, 1.75, 0.0,
];

/// Port of `OCIO_ADD_TEST(RangeOpCPU, identity)` @ v2.5.2.
#[test]
fn identity() {
    let mut range = RangeOpData::new();
    range.set_min_in_value(0.);
    range.set_min_out_value(0.);
    range.validate().unwrap();
    assert!(range.is_identity());
    assert!(!range.is_no_op());

    let op = get_range_renderer(&range).unwrap();

    assert!(has_type(&op, "RangeMinRenderer"));
}

/// Port of `OCIO_ADD_TEST(RangeOpCPU, scale_with_low_and_high_clippings)` @ v2.5.2.
#[test]
fn scale_with_low_and_high_clippings() {
    let range = RangeOpData::with_values(0., 1., 0.5, 1.5).unwrap();

    range.validate().unwrap();

    let op = get_range_renderer(&range).unwrap();

    assert!(has_type(&op, "RangeScaleMinMaxRenderer"));

    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    let mut image: [f32; 36] = [
        -0.50, -0.25, 0.50, 0.0, //
        0.75, 1.00, 1.25, 1.0, //
        1.25, 1.50, 1.75, 0.0, //
        qnan, qnan, qnan, 0.0, //
        0.0, 0.0, 0.0, qnan, //
        inf, inf, inf, 0.0, //
        0.0, 0.0, 0.0, inf, //
        -inf, -inf, -inf, 0.0, //
        0.0, 0.0, 0.0, -inf,
    ];

    op.apply(&mut image);

    check_all_close(
        &image[..12],
        &[
            0.50, 0.50, 1.00, 0.00, //
            1.25, 1.50, 1.50, 1.00, //
            1.50, 1.50, 1.50, 0.00,
        ],
    );

    assert_eq!(image[12], 0.50);
    assert_eq!(image[13], 0.50);
    assert_eq!(image[14], 0.50);
    assert_eq!(image[15], 0.00);

    assert_eq!(image[16], 0.50);
    assert_eq!(image[17], 0.50);
    assert_eq!(image[18], 0.50);
    assert!(is_nan(image[19]));

    assert_eq!(image[20], 1.50);
    assert_eq!(image[21], 1.50);
    assert_eq!(image[22], 1.50);
    assert_eq!(image[23], 0.0);

    assert_eq!(image[24], 0.50);
    assert_eq!(image[25], 0.50);
    assert_eq!(image[26], 0.50);
    assert_eq!(image[27], inf);

    assert_eq!(image[28], 0.50);
    assert_eq!(image[29], 0.50);
    assert_eq!(image[30], 0.50);
    assert_eq!(image[31], 0.0);

    assert_eq!(image[32], 0.50);
    assert_eq!(image[33], 0.50);
    assert_eq!(image[34], 0.50);
    assert_eq!(image[35], -inf);
}

/// Port of `OCIO_ADD_TEST(RangeOpCPU, scale_with_low_and_high_clippings_2)` @ v2.5.2.
#[test]
fn scale_with_low_and_high_clippings_2() {
    let range = RangeOpData::with_values(0., 1., 0., 1.5).unwrap();

    range.validate().unwrap();

    let op = get_range_renderer(&range).unwrap();

    assert!(has_type(&op, "RangeScaleMinMaxRenderer"));

    let mut image = IMAGE3;

    op.apply(&mut image);

    check_all_close(
        &image,
        &[
            0.000, 0.000, 0.750, 0.000, //
            1.125, 1.500, 1.500, 1.000, //
            1.500, 1.500, 1.500, 0.000,
        ],
    );
}

/// Port of `OCIO_ADD_TEST(RangeOpCPU, offset_with_low_and_high_clippings)` @ v2.5.2.
#[test]
fn offset_with_low_and_high_clippings() {
    let range = RangeOpData::with_values(0., 1., 1., 2.).unwrap();

    range.validate().unwrap();

    let op = get_range_renderer(&range).unwrap();

    assert!(has_type(&op, "RangeScaleMinMaxRenderer"));

    let mut image = IMAGE3;

    op.apply(&mut image);

    check_all_close(
        &image,
        &[
            1.00, 1.00, 1.50, 0.00, //
            1.75, 2.00, 2.00, 1.00, //
            2.00, 2.00, 2.00, 0.00,
        ],
    );
}

/// Port of `OCIO_ADD_TEST(RangeOpCPU, low_and_high_clippings)` @ v2.5.2.
#[test]
fn low_and_high_clippings() {
    let range = RangeOpData::with_values(1., 2., 1., 2.).unwrap();

    range.validate().unwrap();

    let op = get_range_renderer(&range).unwrap();

    assert!(has_type(&op, "RangeMinMaxRenderer"));

    let mut image: [f32; 16] = [
        -0.50, -0.25, 0.50, 0.0, //
        0.75, 1.00, 1.25, 1.0, //
        1.25, 1.50, 1.75, 0.0, //
        2.00, 2.50, 2.75, 1.0,
    ];

    op.apply(&mut image);

    check_all_close(
        &image,
        &[
            1.00, 1.00, 1.00, 0.00, //
            1.00, 1.00, 1.25, 1.00, //
            1.25, 1.50, 1.75, 0.00, //
            2.00, 2.00, 2.00, 1.00,
        ],
    );
}

/// Port of `OCIO_ADD_TEST(RangeOpCPU, low_clipping)` @ v2.5.2.
#[test]
fn low_clipping() {
    let range = RangeOpData::with_values(
        -0.1,
        RangeOpData::empty_value(),
        -0.1,
        RangeOpData::empty_value(),
    )
    .unwrap();

    range.validate().unwrap();

    let op = get_range_renderer(&range).unwrap();

    assert!(has_type(&op, "RangeMinRenderer"));

    let mut image = IMAGE3;

    op.apply(&mut image);

    check_all_close(
        &image,
        &[
            -0.10, -0.10, 0.50, 0.00, //
            0.75, 1.00, 1.25, 1.00, //
            1.25, 1.50, 1.75, 0.00,
        ],
    );
}

/// Port of `OCIO_ADD_TEST(RangeOpCPU, high_clipping)` @ v2.5.2.
#[test]
fn high_clipping() {
    let range = RangeOpData::with_values(
        RangeOpData::empty_value(),
        1.1,
        RangeOpData::empty_value(),
        1.1,
    )
    .unwrap();

    range.validate().unwrap();

    let op = get_range_renderer(&range).unwrap();

    assert!(has_type(&op, "RangeMaxRenderer"));

    let mut image = IMAGE3;

    op.apply(&mut image);

    check_all_close(
        &image,
        &[
            -0.50, -0.25, 0.50, 0.00, //
            0.75, 1.00, 1.10, 1.00, //
            1.10, 1.10, 1.10, 0.00,
        ],
    );
}

/// Port of `OCIO_ADD_TEST(RangeOpCPU, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    // Based on scale_with_low_and_high_clippings_2. Setting the direction to inverse and swap
    // the in/out values should give the same numeric result.
    let mut range = RangeOpData::with_values(0., 1.5, 0., 1.).unwrap();
    range.set_direction(TransformDirection::Inverse);

    range.validate().unwrap();

    check_throw_what(get_range_renderer(&range), "Op::finalize has to be called");

    let r = range.get_as_forward().unwrap();
    let op = get_range_renderer(&r).unwrap();

    assert!(has_type(&op, "RangeScaleMinMaxRenderer"));

    let mut image = IMAGE3;

    op.apply(&mut image);

    check_all_close(
        &image,
        &[
            0.000, 0.000, 0.750, 0.000, //
            1.125, 1.500, 1.500, 1.000, //
            1.500, 1.500, 1.500, 0.000,
        ],
    );
}
