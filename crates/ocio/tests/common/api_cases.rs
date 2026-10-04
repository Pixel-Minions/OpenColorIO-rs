// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The parameter cases of the tests through the API, per transform class, as [`Calls`] specs:
//! the explicit cases of the op families' batteries (`crates/ocio-ops/tests/*_oracle.rs`),
//! whose values come from upstream's tests or exercise a code path, written as the class's
//! setters. Each family also names its mutation bases, the cases the battery generates extreme,
//! NaN and infinite parameters from.
//!
//! The specs set the parameters with setters on the default transform, as a C++ program
//! does, so only `Config::getProcessor` validates them (`Processor::Impl::setTransform`,
//! src/OpenColorIO/Processor.cpp:623-641 @ v2.5.2), and NaN and infinite parameters take the
//! same route as finite ones.

use ocio_testkit::battery::params::{Case, RGB};

use super::api::Calls;

/// A family's cases: the explicit ones, and the mutation bases among them.
pub(crate) struct Cases {
    /// The explicit cases.
    pub(crate) cases: Vec<Case<Calls>>,
    /// The cases the battery mutates.
    pub(crate) bases: Vec<Case<Calls>>,
}

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// A `MatrixTransform` with this matrix and offsets.
pub(crate) fn matrix_calls(matrix: [f64; 16], offset: [f64; 4]) -> Calls {
    Calls::new("MatrixTransform")
        .matrix("setMatrix", matrix)
        .rgba("setOffset", offset)
}

/// `MatrixTransform`: the cases of `crates/ocio-ops/tests/matrix_oracle.rs`.
pub(crate) fn matrix() -> Cases {
    // tests/cpu/ops/matrix/MatrixOpCPU_tests.cpp and MatrixOpData_tests.cpp:576-581 @ v2.5.2.
    let upstream_matrix = [
        0.9f32, 0.8, -0.7, 0.6, -0.4, 0.5, 0.3, 0.2, 0.1, -0.2, 0.4, 0.3, -0.5, 0.6, 0.7, 0.8,
    ]
    .map(f64::from);
    let upstream_offset = [-0.1f32, 0.2, -0.3, 0.4].map(f64::from);
    let bases = vec![
        Case::new(
            "scale",
            matrix_calls(diagonal([2.0, 0.5, 4.0, 1.5]), [0.0; 4]),
        ),
        Case::new(
            "scale with offsets",
            matrix_calls(diagonal([2.0; 4]), [1.0, 2.0, 3.0, 4.0]),
        ),
        Case::new("matrix", matrix_calls(upstream_matrix, [0.0; 4])),
        Case::new(
            "matrix with offsets",
            matrix_calls(upstream_matrix, upstream_offset),
        ),
    ];
    let mut cases = bases.clone();
    let mut crosstalk = diagonal([2.0; 4]);
    crosstalk[3] = 0.5;
    cases.push(Case::new(
        "upstream's matrix with offsets",
        matrix_calls(crosstalk, [1.0, 2.0, 3.0, 4.0]),
    ));
    cases.push(Case::new(
        "sRGB to XYZ",
        matrix_calls(
            [
                0.4124564, 0.3575761, 0.1804375, 0.0, 0.2126729, 0.7151522, 0.0721750, 0.0,
                0.0193339, 0.1191920, 0.9503041, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.0; 4],
        ),
    ));
    cases.push(Case::new(
        "identity",
        matrix_calls(diagonal([1.0; 4]), [0.0; 4]),
    ));
    cases.push(Case::new(
        "near identity",
        matrix_calls(diagonal([1.0 + 5e-7, 1.0, 1.0 - 5e-7, 1.0]), [0.0; 4]),
    ));
    let mut singular = diagonal([1.0, 0.0, 1.0, 1.0]);
    singular[3] = 0.2;
    cases.push(Case::new("singular", matrix_calls(singular, [0.0; 4])));
    let nan = f64::NAN;
    cases.push(
        Case::new(
            "NaN in a matrix with offsets",
            matrix_calls(
                [
                    0.9, 0.8, -0.7, nan, -0.4, 0.5, 0.3, 0.2, 0.1, -0.2, nan, 0.3, -0.5, 0.6, 0.7,
                    0.8,
                ],
                [-0.1, nan, -0.3, 0.4],
            ),
        )
        .w0002_nowhere(),
    );
    cases.push(
        Case::new(
            "NaN in a scale",
            matrix_calls(diagonal([2.0, nan, 0.5, 1.0]), [0.0; 4]),
        )
        .w0002_nowhere(),
    );
    Cases { cases, bases }
}

/// An empty bound of a `RangeTransform`.
const E: f64 = f64::NAN;

/// A `RangeTransform` with these bounds (`[minIn, maxIn, minOut, maxOut]`, NaN for an empty
/// one), clamping or not.
pub(crate) fn range_calls(bounds: [f64; 4], clamp: bool) -> Calls {
    let mut calls = Calls::new("RangeTransform");
    if !clamp {
        calls = calls.enumerated("setStyle", "RANGE_NO_CLAMP");
    }
    calls
        .scalar("setMinInValue", bounds[0], RGB)
        .scalar("setMaxInValue", bounds[1], RGB)
        .scalar("setMinOutValue", bounds[2], RGB)
        .scalar("setMaxOutValue", bounds[3], RGB)
}

/// `RangeTransform`: the cases of `crates/ocio-ops/tests/range_oracle.rs`. A NaN bound is an
/// empty one, which no renderer computes with, so the explicit cases compare exactly.
pub(crate) fn range() -> Cases {
    let case =
        |label: &str, bounds, clamp| Case::new(label, range_calls(bounds, clamp)).w0002_nowhere();
    let bases = vec![
        case("scale clamp", [0.1, 0.9, 0.2, 0.7], true),
        case("min max clamp", [1.0, 2.0, 1.0, 2.0], true),
        case("min clamp", [-0.1, E, -0.1, E], true),
        case("max clamp", [E, 1.1, E, 1.1], true),
        case("no clamp", [0.0, 0.5, 0.5, 1.5], false),
    ];
    let mut cases = vec![
        // tests/cpu/ops/range/RangeOpCPU_tests.cpp, RangeOp_tests.cpp @ v2.5.2.
        case("upstream 0 1 0.5 1.5", [0.0, 1.0, 0.5, 1.5], true),
        case("upstream 0 1 0 1.5", [0.0, 1.0, 0.0, 1.5], true),
        case("upstream 0 1 1 2", [0.0, 1.0, 1.0, 2.0], true),
        case("upstream 0 1.5 0 1", [0.0, 1.5, 0.0, 1.0], true),
        case("upstream arbitrary", [-0.101, 0.95, 0.194, 1.001], true),
        case("identity", [0.0, 1.0, 0.0, 1.0], true),
        case("clamp negatives", [0.0, E, 0.0, E], true),
        case("constant", [0.0, 1.0, 0.5, 0.5], true),
        case("zero bounds", [-0.0, 0.0 + 1e-3, 0.0, -0.0], true),
        case("scale is zero", [0.0, 1.0, 0.25, 0.25], true),
        case("offset below 1e-6", [0.0, 1.0, 0.5e-6, 1.0 + 0.5e-6], true),
        case("offset above 1e-6", [0.0, 1.0, 1.5e-6, 1.0 + 1.5e-6], true),
        case("float overflow", [-1e39, 1e39, -1e39, 1e39], true),
        case("NaN scale", [-1e308, 1e308, -1e308, 1e308], true),
        case("NaN offset", [-1e308, 1e308, 0.0, 1.0], true),
        case("no clamp identity", [0.0, 1.0, 0.0, 1.0], false),
        case("no clamp one-sided", [0.0, E, 0.0, E], false),
        case("refused", [0.5, 0.5 + 1e-7, 0.0, 1.0], true),
        case("refused one-sided", [0.25, E, 0.5, E], true),
        case(
            "infinite input bounds",
            [f64::NEG_INFINITY, f64::INFINITY, 0.0, 1.0],
            true,
        ),
    ];
    cases.extend(bases.clone());
    Cases { cases, bases }
}

/// A `CDLTransform`: slope, offset, power, saturation and style (`CDL_ASC` or not).
pub(crate) fn cdl_calls(
    slope: [f64; 3],
    offset: [f64; 3],
    power: [f64; 3],
    sat: f64,
    asc: bool,
) -> Calls {
    Calls::new("CDLTransform")
        .rgb("setSlope", slope)
        .rgb("setOffset", offset)
        .rgb("setPower", power)
        .scalar("setSat", sat, RGB)
        .enumerated("setStyle", if asc { "CDL_ASC" } else { "CDL_NO_CLAMP" })
}

/// `CDLTransform`: the cases of `crates/ocio-ops/tests/cdl_oracle.rs`.
pub(crate) fn cdl() -> Cases {
    let mut cases = Vec::new();
    let mut bases = Vec::new();
    for asc in [true, false] {
        let list = [
            // tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66, 428-434, 469-475 @ v2.5.2.
            (
                [1.35, 1.1, 0.071],
                [0.05, -0.23, 0.11],
                [0.93, 0.81, 1.27],
                1.23,
            ),
            (
                [1.15, 1.10, 0.9],
                [0.05, 0.02, 0.07],
                [1.2, 0.95, 1.13],
                0.87,
            ),
            ([3.405, 1.0, 1.0], [-0.178; 3], [1.095; 3], 0.99),
            // A saturation of 1 (no crosstalk), and of 0; a slope of 0; reciprocals at their
            // floor of 0.01; a power of 1 in two channels.
            ([0.8, 1.2, 1.0], [0.1, 0.0, -0.1], [2.2, 1.0, 1.0], 1.0),
            ([0.0, 1.0, 0.005], [0.5, 0.0, 0.25], [0.004, 1.5, 0.5], 0.0),
            (
                [100.0, 1e-3, 2.0],
                [-50.0, 3.0, 0.0],
                [50.0, 1e-3, 0.7],
                0.005,
            ),
        ];
        for (k, (slope, offset, power, sat)) in list.into_iter().enumerate() {
            let case = Case::new(
                format!("asc {asc} {k}"),
                cdl_calls(slope, offset, power, sat, asc),
            );
            if k == 0 {
                bases.push(case.clone());
            }
            cases.push(case);
        }
    }
    // Refusals: a negative slope, a zero power, a negative saturation.
    for (label, c) in [
        (
            "negative slope",
            cdl_calls([-0.9, 1.0, 1.0], [0.0; 3], [1.2; 3], 1.0, true),
        ),
        (
            "zero power",
            cdl_calls([1.0; 3], [0.0; 3], [1.2, 0.0, 1.2], 1.0, false),
        ),
        (
            "negative sat",
            cdl_calls([1.0; 3], [0.0; 3], [1.2; 3], -1.17, true),
        ),
    ] {
        cases.push(Case::new(format!("refused {label}"), c));
    }
    // Non-finite parameters, compared bit for bit (`crates/ocio-ops/tests/cdl_oracle.rs`): a
    // NaN offset (accepted), an infinite slope and power (accepted), and a NaN power (refused).
    let (nan, inf) = (f64::NAN, f64::INFINITY);
    for (label, c) in [
        (
            "NaN offset asc",
            cdl_calls([1.2, 1.0, 0.9], [0.1, nan, 0.0], [1.1; 3], 0.9, true),
        ),
        (
            "NaN offset",
            cdl_calls([1.2, 1.0, 0.9], [nan, 0.0, -0.1], [0.8; 3], 1.3, false),
        ),
        (
            "infinite slope and power",
            cdl_calls(
                [inf, 1.0, 0.9],
                [0.1, 0.0, 0.0],
                [1.1, inf, 0.9],
                0.9,
                false,
            ),
        ),
        (
            "NaN power",
            cdl_calls([1.2, 1.0, 0.9], [0.1, 0.0, 0.0], [1.1, nan, 0.9], 0.9, true),
        ),
    ] {
        cases.push(Case::new(label, c).w0002_nowhere());
    }
    Cases { cases, bases }
}
