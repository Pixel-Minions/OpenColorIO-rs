// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The test of the color matrix helpers from
//! `tests/cpu/transforms/BuiltinTransform_tests.cpp` @ v2.5.2.

use super::*;
use ocio_testkit::upstream::equal_with_safe_rel_error;

/// `ValidateValues(idx, act, aim, errorThreshold, lineNo)`: a relative error that turns
/// absolute below 1.
///
/// Port of `ValidateValues` (tests/cpu/transforms/BuiltinTransform_tests.cpp:121-152 @ v2.5.2).
#[track_caller]
fn validate_values(idx: usize, act: f64, aim: f64, error_threshold: f64) {
    assert!(
        equal_with_safe_rel_error(act, aim, error_threshold, 1.0),
        "Index = {idx} with threshold = {error_threshold} - Values: {act} expected: {aim}"
    );
}

/// `expected[k]` against value `k` of each row of the 3x3 part, and the rest of the 4x4
/// identity exactly.
#[track_caller]
fn check_matrix(matrix: &MatrixArray, expected: [(usize, f64, f64); 9]) {
    let v = matrix.get_values();
    for (idx, aim, threshold) in expected {
        validate_values(idx, v[idx], aim, threshold);
    }
    for idx in [3, 7, 11, 12, 13, 14] {
        assert_eq!(v[idx], 0.);
    }
    assert_eq!(v[15], 1.);
}

/// Port of `OCIO_ADD_TEST(Builtins, color_matrix_helpers)` @ v2.5.2.
#[test]
fn color_matrix_helpers() {
    // Test all the color matrix helper methods.

    {
        let matrix = rgb2xyz_from_xy(&aces_ap1::PRIMARIES).unwrap();
        check_matrix(
            &matrix,
            [
                (0, 0.66245418, 1e-7),
                (1, 0.13400421, 1e-7),
                (2, 0.15618769, 1e-7),
                (4, 0.27222872, 1e-7),
                (5, 0.67408177, 1e-7),
                (6, 0.05368952, 1e-7),
                (8, -0.00557465, 1e-7),
                (9, 0.00406073, 1e-7),
                (10, 1.0103391, 1e-6),
            ],
        );
    }

    {
        // D65 to D60.
        let src_xyz = Offsets::new(0.9504559270516716, 1., 1.0890577507598784, 0.);
        let dst_xyz = Offsets::new(0.9526460745698463, 1., 1.0088251843515859, 0.);

        let matrix = build_vonkries_adapt(&src_xyz, &dst_xyz, AdaptationMethod::Bradford).unwrap();
        check_matrix(
            &matrix,
            [
                (0, 1.01303491, 1e-7),
                (1, 0.00610526, 1e-7),
                (2, -0.01497094, 1e-7),
                (4, 0.00769823, 1e-7),
                (5, 0.99816335, 1e-7),
                (6, -0.00503204, 1e-7),
                (8, -0.00284132, 1e-7),
                (9, 0.00468516, 1e-7),
                (10, 0.92450614, 1e-7),
            ],
        );
    }

    {
        // Note: Source and dest white points are equal.
        let matrix = build_conversion_matrix(
            &p3_d65::PRIMARIES,
            &rec709::PRIMARIES,
            AdaptationMethod::Bradford,
        )
        .unwrap();
        check_matrix(
            &matrix,
            [
                (0, 1.22494018, 1e-7),
                (1, -0.22494018, 1e-7),
                (2, 0., 1e-7),
                (4, -0.04205695, 1e-7),
                (5, 1.04205695, 1e-7),
                (6, 0., 1e-7),
                (8, -0.01963755, 1e-7),
                (9, -0.07863605, 1e-7),
                (10, 1.09827360, 1e-7),
            ],
        );
    }

    {
        // Note: Source and dest white points differ.
        let matrix = build_conversion_matrix(
            &aces_ap1::PRIMARIES,
            &rec709::PRIMARIES,
            AdaptationMethod::Bradford,
        )
        .unwrap();
        check_matrix(
            &matrix,
            [
                (0, 1.70505099, 1e-7),
                (1, -0.62179212, 1e-7),
                (2, -0.08325887, 1e-7),
                (4, -0.13025642, 1e-7),
                (5, 1.14080474, 1e-7),
                (6, -0.01054832, 1e-7),
                (8, -0.02400336, 1e-7),
                (9, -0.12896898, 1e-7),
                (10, 1.15297233, 1e-7),
            ],
        );
    }

    {
        // Note: Source and dest white points differ, manual override specified.
        let null = Offsets::new(0., 0., 0., 0.);
        let d65_wht_xyz = Offsets::new(0.95045592705167, 1., 1.08905775075988, 0.);
        let matrix = build_conversion_matrix_with_whites(
            &aces_ap0::PRIMARIES,
            &cie_xyz_illum_e::PRIMARIES,
            &null,
            &d65_wht_xyz,
            AdaptationMethod::Bradford,
        )
        .unwrap();
        check_matrix(
            &matrix,
            [
                (0, 0.93827985, 1e-7),
                (1, -0.00445145, 1e-7),
                (2, 0.01662752, 1e-7),
                (4, 0.33736889, 1e-7),
                (5, 0.72952157, 1e-7),
                (6, -0.06689046, 1e-7),
                (8, 0.00117395, 1e-7),
                (9, -0.00371071, 1e-7),
                (10, 1.09159451, 1e-7),
            ],
        );
    }
}
