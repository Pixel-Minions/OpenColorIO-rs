// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/gamma/GammaOpUtils_tests.cpp` @ v2.5.2.

use super::*;
use ocio_testkit::upstream::equal_with_abs_error;

/// Port of `OCIO_ADD_TEST(GammaOpUtils, compute_params_forward)` @ v2.5.2.
#[test]
fn compute_params_forward() {
    // `{ 2.0f, 0.1f }`: float literals in a vector of doubles.
    let g_params: Params = vec![f64::from(2.0f32), f64::from(0.1f32)];

    let r_params = compute_params_fwd(&std::hint::black_box(g_params));

    assert_eq!(r_params.gamma, 2.0f32);
    assert_eq!(r_params.offset, (0.1f64 / (1.0 + 0.1)) as f32);
    assert_eq!(r_params.break_pnt, (0.1f64 / (2.0 - 1.0)) as f32);
    assert_eq!(r_params.scale, (1.0f64 / (1.0 + 0.1)) as f32);

    assert!(equal_with_abs_error(r_params.slope, 0.33057851f32, 1e-7f32));
}

/// Port of `OCIO_ADD_TEST(GammaOpUtils, compute_params_reverse)` @ v2.5.2.
#[test]
fn compute_params_reverse() {
    let g_params: Params = vec![f64::from(2.0f32), f64::from(0.1f32)];

    let r_params = compute_params_rev(&std::hint::black_box(g_params));

    assert_eq!(r_params.gamma, 0.5f32);
    assert_eq!(r_params.offset, 0.1f32);
    assert_eq!(r_params.scale, 1.0f32 + 0.1f32);

    assert!(equal_with_abs_error(
        r_params.break_pnt,
        0.03305785f32,
        1e-7f32
    ));
    assert!(equal_with_abs_error(r_params.slope, 3.02499986f32, 1e-7f32));
}
