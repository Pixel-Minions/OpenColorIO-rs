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

use ocio::TransformDirection;
use ocio_ops::open_color_types::GradingStyle;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_testkit::battery::Direction;
use ocio_testkit::battery::params::{Case, RGB, W0001Function};
use ocio_testkit::transform_text::f64_spec;

use super::api::{Arg, Calls, num};

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

/// A `LogTransform` of this base.
pub(crate) fn log_calls(base: f64) -> Calls {
    Calls::new("LogTransform").scalar("setBase", base, RGB)
}

/// `LogTransform`: the cases of `crates/ocio-ops/tests/log_oracle.rs`.
pub(crate) fn log() -> Cases {
    // 2 and 10 are the Log2/Log10 renderers; others use LinToLog and LogToLin.
    let mut cases: Vec<Case<Calls>> = [2.0, 10.0, std::f64::consts::E, 3.7, 0.5]
        .map(|base| Case::new(format!("base {base}"), log_calls(base)))
        .to_vec();
    let bases = vec![cases[3].clone()];
    cases.push(Case::new("base NaN", log_calls(f64::NAN)).w0002_nowhere());
    for base in [1.0, 0.0, -2.5, f64::NEG_INFINITY] {
        cases.push(Case::new(format!("refused base {base}"), log_calls(base)));
    }
    Cases { cases, bases }
}

/// The parameters of a `LogAffineTransform` or a `LogCameraTransform`: the base, then
/// `[logSideSlope, logSideOffset, linSideSlope, linSideOffset]`.
type Affine = (f64, [[f64; 3]; 4]);

/// A labelled `LogCameraTransform`: the break, the base and affine parameters, the linear slope.
type CameraCase = (&'static str, [f64; 3], Affine, Option<[f64; 3]>);

/// The setters of the affine parameters, in [`Affine`]'s order.
const AFFINE_SETTERS: [&str; 4] = [
    "setLogSideSlopeValue",
    "setLogSideOffsetValue",
    "setLinSideSlopeValue",
    "setLinSideOffsetValue",
];

/// The base and the affine parameters, set on `calls`.
fn affine_setters(mut calls: Calls, (base, params): Affine) -> Calls {
    calls = calls.scalar("setBase", base, RGB);
    for (setter, values) in AFFINE_SETTERS.iter().zip(params) {
        calls = calls.rgb(setter, values);
    }
    calls
}

/// A `LogAffineTransform`.
pub(crate) fn log_affine_calls(p: Affine) -> Calls {
    affine_setters(Calls::new("LogAffineTransform"), p)
}

/// `LogAffineTransform`: the cases of `crates/ocio-ops/tests/log_oracle.rs`, its extreme
/// finite and NaN cases included.
pub(crate) fn log_affine() -> Cases {
    let typical: [Affine; 6] = [
        // Different parameters per channel.
        (
            10.0,
            [
                [0.18, 0.5, 1.7],
                [0.4, -0.1, 0.0],
                [1.5, 0.9, 2.2],
                [0.01, 0.2, -0.05],
            ],
        ),
        // Cineon-like, base 10.
        (
            10.0,
            [[0.293255132; 3], [0.669599218; 3], [0.9892; 3], [0.0108; 3]],
        ),
        // Base e and base 2 with non-default parameters.
        (
            std::f64::consts::E,
            [[0.25, 0.3, 0.35], [0.5; 3], [4.0, 5.0, 6.0], [0.1; 3]],
        ),
        (2.0, [[0.05; 3], [0.6; 3], [1.0; 3], [0.0078125; 3]]),
        // Default parameters: plain Log2 and Log10 ops.
        (2.0, [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]]),
        (10.0, [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]]),
    ];
    let mut cases: Vec<Case<Calls>> = typical
        .iter()
        .enumerate()
        .map(|(i, &p)| Case::new(format!("case {i}"), log_affine_calls(p)))
        .collect();
    let bases = vec![cases[0].clone()];
    let base = typical[0];
    let refused = |label: &str, change: &dyn Fn(&mut Affine)| {
        let mut p = base;
        change(&mut p);
        Case::new(format!("refused: {label}"), log_affine_calls(p))
    };
    cases.extend([
        refused("green lin side slope 0", &|p| p.1[2][1] = 0.0),
        refused("red log side slope -0", &|p| p.1[0][0] = -0.0),
        refused("blue slopes 0, green log side slope 0", &|p| {
            p.1[2][2] = 0.0;
            p.1[0] = [1.0, 0.0, 0.0];
        }),
        refused("base 1, tiny lin side slope", &|p| {
            p.0 = 1.0;
            p.1[2][0] = 1e-300;
        }),
        refused("base 0", &|p| p.0 = 0.0),
        refused("lin side slope 0, infinite offset", &|p| {
            p.1[2][0] = 0.0;
            p.1[1][2] = f64::INFINITY;
        }),
        refused("base -inf", &|p| p.0 = f64::NEG_INFINITY),
    ]);
    // Finite parameters that overflow `float`.
    let extreme: [Affine; 3] = [
        (
            1e39,
            [
                [1e39, 0.5, -1e39],
                [0.1, 0.2, 0.3],
                [1.0; 3],
                [0.01, 0.02, 0.03],
            ],
        ),
        (
            1e-46,
            [
                [1e39, 0.5, -1e39],
                [0.1, 0.2, 0.3],
                [1.0; 3],
                [0.01, 0.02, 0.03],
            ],
        ),
        (
            10.0,
            [
                [0.3; 3],
                [1e39, -1e39, 0.3],
                [1e39, -1e39, 1.0],
                [-1e39, 1e39, 0.03],
            ],
        ),
    ];
    for (i, p) in extreme.into_iter().enumerate() {
        cases.push(Case::new(format!("extreme {i}"), log_affine_calls(p)));
    }
    // NaN parameters: under W0002 only where MSVC swapped the operands of `Log2LinRenderer`
    // (inverse, fast math off; `crates/ocio-ops/tests/log_oracle.rs`), bit for bit elsewhere.
    let nan = f64::NAN;
    cases.push(
        Case::new(
            "NaN parameters",
            log_affine_calls((
                10.0,
                [
                    [nan, 0.5, 1.0],
                    [0.1, nan, 0.2],
                    [1.0, 1.0, nan],
                    [nan, 0.01, 0.1],
                ],
            )),
        )
        .w0002_only_where(|c| c.direction == Direction::Inverse && !c.fast_math),
    );
    cases.push(
        Case::new(
            "NaN base",
            log_affine_calls((nan, [[0.3, 0.5, 1.0], [0.1, 0.2, 0.3], [1.0; 3], [0.0; 3]])),
        )
        .w0002_nowhere(),
    );
    Cases { cases, bases }
}

/// A `LogCameraTransform`: the break, the base and the affine parameters, and the linear
/// slope when set.
pub(crate) fn log_camera_calls(brk: [f64; 3], p: Affine, linear_slope: Option<[f64; 3]>) -> Calls {
    let calls = affine_setters(
        Calls::new("LogCameraTransform").arg_rgb("linSideBreak", brk),
        p,
    );
    match linear_slope {
        Some(slope) => calls.rgb("setLinearSlopeValue", slope),
        None => calls,
    }
}

/// `LogCameraTransform`: the cases of `crates/ocio-ops/tests/log_oracle.rs`, its extreme
/// finite and NaN cases included.
pub(crate) fn log_camera() -> Cases {
    let logc3: Affine = (
        10.0,
        [[0.247190; 3], [0.385537; 3], [5.555556; 3], [0.052272; 3]],
    );
    let rgb: Affine = (
        2.0,
        [
            [0.2, 0.25, 0.18],
            [0.6, 0.55, 0.62],
            [1.1, 1.3, 0.9],
            [0.05, 0.02, 0.1],
        ],
    );
    let rgb10: Affine = (
        10.0,
        [
            [0.24719; 3],
            [0.385537, 0.6, 0.0],
            [5.555556, 1.0, 1.0],
            [0.05, 0.05, 0.0],
        ],
    );
    let mut cases = vec![
        Case::new(
            "LogC3 EI800, computed linear slope",
            log_camera_calls([0.010591; 3], logc3, None),
        ),
        Case::new(
            "LogC3 EI800",
            log_camera_calls([0.010591; 3], logc3, Some([5.367655; 3])),
        ),
        Case::new(
            "per channel, computed linear slope",
            log_camera_calls([0.1, 0.05, 0.2], rgb, None),
        ),
        Case::new(
            "per channel",
            log_camera_calls([0.1, 0.05, 0.2], rgb, Some([1.2, 1.4, 0.95])),
        ),
        Case::new(
            "per channel, base 10, computed linear slope",
            log_camera_calls([0.010591; 3], rgb10, None),
        ),
        Case::new(
            "per channel, base 10",
            log_camera_calls([0.010591; 3], rgb10, Some([5.367655, 1.1, 0.9])),
        ),
        Case::new(
            "base e",
            log_camera_calls(
                [0.18, 0.02, 0.3],
                (
                    std::f64::consts::E,
                    [
                        [0.3, 0.3, 0.3],
                        [0.5, 0.45, 0.55],
                        [3.0, 2.5, 4.0],
                        [0.01, 0.03, 0.005],
                    ],
                ),
                None,
            ),
        ),
        Case::new(
            "negative break",
            log_camera_calls(
                [-0.05, -0.1, -0.2],
                (2.0, [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]]),
                None,
            ),
        ),
    ];
    let bases = vec![cases[3].clone()];
    let refused = |label: &str, brk: [f64; 3], p: Affine| {
        Case::new(
            format!("refused: {label}"),
            log_camera_calls(brk, p, Some([1.2, 1.4, 0.95])),
        )
    };
    let with = |change: &dyn Fn(&mut Affine)| {
        let mut p = rgb;
        change(&mut p);
        p
    };
    cases.extend([
        refused(
            "blue lin side slope 0",
            [0.1, 0.05, 0.2],
            with(&|p| p.1[2][2] = 0.0),
        ),
        refused(
            "green log side slope 0",
            [0.1, 0.05, 0.2],
            with(&|p| p.1[0][1] = 0.0),
        ),
        refused("base -2", [0.1, 0.05, 0.2], with(&|p| p.0 = -2.0)),
        refused(
            "log side slope 0, infinite break",
            [0.1, f64::INFINITY, 0.2],
            with(&|p| p.1[0][0] = 0.0),
        ),
    ]);
    // Finite parameters that overflow `float` or `double`.
    let extreme: [CameraCase; 7] = [
        (
            "base 1e39, log slope 1e39, negative break",
            [-0.05, -0.1, -0.2],
            (1e39, [[1e39; 3], [0.0; 3], [1.0; 3], [0.0; 3]]),
            None,
        ),
        (
            "base 1e39, log slope 1e39, positive break",
            [0.1, 0.2, 0.3],
            (1e39, [[1e39; 3], [0.5; 3], [1.0; 3], [0.01; 3]]),
            None,
        ),
        (
            "base 1e-46, log slope -1e39, negative break",
            [-0.05, -0.1, -0.2],
            (1e-46, [[-1e39; 3], [0.0; 3], [1.0; 3], [0.0; 3]]),
            None,
        ),
        (
            "overflowing computed slope",
            [1e200, 1e200, 0.1],
            (
                10.0,
                [[1e200, 1e200, 0.3], [0.5; 3], [1e200, 1e200, 1.0], [0.0; 3]],
            ),
            None,
        ),
        (
            "linear slope 1e39, zero break",
            [0.0; 3],
            (2.0, [[0.25; 3], [0.5; 3], [1.0; 3], [0.0; 3]]),
            Some([1e39, -1e39, 1e39]),
        ),
        (
            "negative lin side to -inf",
            [-1e300, -0.1, 0.1],
            (
                2.0,
                [[1e39, 1.0, 1.0], [0.0; 3], [1e10, 1.0, 1.0], [0.0; 3]],
            ),
            None,
        ),
        (
            "overflowing lin side, finite break",
            [-2.0; 3],
            (2.0, [[2.0; 3], [0.0; 3], [1.7e308; 3], [0.0; 3]]),
            None,
        ),
    ];
    for (label, brk, p, slope) in extreme {
        cases.push(Case::new(label, log_camera_calls(brk, p, slope)));
    }
    let nan = f64::NAN;
    cases.push(
        Case::new(
            "NaN parameters",
            log_camera_calls(
                [0.1, nan, 0.2],
                (
                    2.0,
                    [
                        [0.25, 0.3, nan],
                        [0.5, nan, 0.6],
                        [1.0; 3],
                        [nan, 0.02, 0.01],
                    ],
                ),
                Some([1.2, 1.0, nan]),
            ),
        )
        .w0002_nowhere(),
    );
    Cases { cases, bases }
}

/// An `ExponentTransform` with this value and negative style.
pub(crate) fn exponent_calls(value: [f64; 4], style: &str) -> Calls {
    Calls::new("ExponentTransform")
        .rgba("setValue", value)
        .enumerated("setNegativeStyle", style)
}

/// `ExponentTransform` in the raw config, a version 2 config, where it builds a Gamma op: the
/// cases of `crates/ocio-ops/tests/gamma_oracle.rs`.
pub(crate) fn exponent() -> Cases {
    let values: [[f64; 4]; 3] = [
        [1.0, 2.2, 0.45, 2.6],
        [2.4, 1.0, 1.8, 0.5],
        [0.01, 100.0, 1.0, 3.3],
    ];
    let mut cases = Vec::new();
    let mut bases = Vec::new();
    for style in ["NEGATIVE_CLAMP", "NEGATIVE_MIRROR", "NEGATIVE_PASS_THRU"] {
        for value in values {
            cases.push(Case::new(
                format!("{value:?} {style}"),
                exponent_calls(value, style),
            ));
        }
        bases.push(cases[cases.len() - 3].clone());
    }
    for value in [
        [2.2, 0.006, 1.0, 1.0],
        [2.2, 2.2, 1.0, 110.0],
        [f64::INFINITY, 2.2, 1.0, 1.0],
    ] {
        cases.push(Case::new(
            format!("refused {value:?}"),
            exponent_calls(value, "NEGATIVE_MIRROR"),
        ));
    }
    for style in ["NEGATIVE_CLAMP", "NEGATIVE_MIRROR", "NEGATIVE_PASS_THRU"] {
        cases.push(
            Case::new(
                format!("NaN value {style}"),
                exponent_calls([2.2, f64::NAN, 1.8, 1.0], style),
            )
            .w0002_nowhere(),
        );
    }
    Cases { cases, bases }
}

/// An `ExponentWithLinearTransform` with this gamma, offset and negative style.
pub(crate) fn exponent_with_linear_calls(gamma: [f64; 4], offset: [f64; 4], style: &str) -> Calls {
    Calls::new("ExponentWithLinearTransform")
        .rgba("setGamma", gamma)
        .rgba("setOffset", offset)
        .enumerated("setNegativeStyle", style)
}

/// `ExponentWithLinearTransform`: the cases of `crates/ocio-ops/tests/gamma_oracle.rs`.
pub(crate) fn exponent_with_linear() -> Cases {
    let params: [([f64; 4], [f64; 4]); 3] = [
        ([2.4, 2.2, 1.0, 1.8], [0.055, 0.2, 0.0, 0.6]),
        ([2.4, 1.0 / 0.45, 3.0, 10.0], [0.055, 0.099, 0.16, 0.9]),
        ([1.0, 1.5, 7.5, 2.0], [0.5, 0.0, 0.001, 0.4]),
    ];
    let mut cases = Vec::new();
    let mut bases = Vec::new();
    for style in ["NEGATIVE_LINEAR", "NEGATIVE_MIRROR"] {
        for (gamma, offset) in params {
            cases.push(Case::new(
                format!("{gamma:?} {offset:?} {style}"),
                exponent_with_linear_calls(gamma, offset, style),
            ));
        }
        bases.push(cases[cases.len() - 3].clone());
    }
    for (gamma, offset) in [
        ([2.4, 0.5, 2.2, 1.8], [0.055, 0.1, 0.1, 0.1]),
        ([2.4, 2.2, 2.2, 1.8], [0.055, 0.1, 0.1, 1.0]),
        ([2.4, 2.2, 2.2, 1.8], [0.055, f64::NEG_INFINITY, 0.1, 0.1]),
        ([2.4, f64::INFINITY, 2.2, 1.8], [0.055, 0.1, 0.1, 0.1]),
    ] {
        cases.push(Case::new(
            format!("refused {gamma:?} {offset:?}"),
            exponent_with_linear_calls(gamma, offset, "NEGATIVE_LINEAR"),
        ));
    }
    // NaN gamma and offset (`crates/ocio-ops/tests/gamma_oracle.rs`): the linear style under
    // W0002 only forward with fast math off, the mirror style bit for bit.
    let nan = f64::NAN;
    let (gamma, offset) = ([2.4, nan, 2.2, 1.8], [0.055, 0.1, nan, 0.2]);
    cases.push(
        Case::new(
            "NaN gamma and offset Linear",
            exponent_with_linear_calls(gamma, offset, "NEGATIVE_LINEAR"),
        )
        .w0002_only_where(|c| c.direction == Direction::Forward && !c.fast_math),
    );
    cases.push(
        Case::new(
            "NaN gamma and offset Mirror",
            exponent_with_linear_calls(gamma, offset, "NEGATIVE_MIRROR"),
        )
        .w0002_nowhere(),
    );
    Cases { cases, bases }
}

/// An `AllocationTransform` of `allocation` (`ALLOCATION_*`) with these variables, if any.
pub(crate) fn allocation_calls(allocation: &str, vars: &[f64]) -> Calls {
    let calls = Calls::new("AllocationTransform")
        .f32()
        .enumerated("setAllocation", allocation);
    if vars.is_empty() {
        calls
    } else {
        calls.list("setVars", vars, RGB)
    }
}

/// `AllocationTransform`: the default, uniform and log2 allocations with the default
/// variables, upstream's (tests/cpu/ops/allocation/AllocationOp_tests.cpp @ v2.5.2: 0 to 10,
/// 0 to 1), those of `crates/ocio/tests/allocation_transform_oracle.rs`, and an unknown
/// allocation, which is refused.
pub(crate) fn allocation() -> Cases {
    let mut cases = vec![Case::new(
        "the default",
        Calls::new("AllocationTransform").f32(),
    )];
    let mut bases = Vec::new();
    for allocation in ["ALLOCATION_UNIFORM", "ALLOCATION_LG2"] {
        let list: [&[f64]; 6] = [
            &[],
            &[0.0, 10.0],
            &[0.0, 1.0],
            &[-8.0, 8.0],
            &[-10.0, 6.0, 0.0001],
            &[0.0, 0.0],
        ];
        for vars in list {
            cases.push(Case::new(
                format!("{allocation} {vars:?}"),
                allocation_calls(allocation, vars),
            ));
        }
        bases.push(cases[cases.len() - 2].clone());
    }
    cases.push(Case::new(
        "refused unknown",
        allocation_calls("ALLOCATION_UNKNOWN", &[0.0, 1.0]),
    ));
    Cases { cases, bases }
}

/// `GroupTransform`: lists of the other classes that the optimizer combines, replaces or
/// removes, and nested groups in either direction. The bases are a list of three classes and a
/// pair of matrices, which the optimizer combines.
pub(crate) fn group() -> Cases {
    let inverse = Direction::Inverse;
    let group = || Calls::new("GroupTransform");
    let log_matrix_range = group()
        .child(log_calls(2.0))
        .child(matrix_calls(
            diagonal([2.0, 0.5, 4.0, 1.0]),
            [0.1, 0.0, -0.1, 0.0],
        ))
        .child(range_calls([0.0, 2.0, 0.0, 1.0], true));
    let two_matrices = group()
        .child(matrix_calls(
            [
                0.9, 0.8, -0.7, 0.0, -0.4, 0.5, 0.3, 0.0, 0.1, -0.2, 0.4, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.1, 0.2, 0.3, 0.0],
        ))
        .child(matrix_calls(
            diagonal([1.5, 2.0, 0.25, 1.0]),
            [0.0, -0.5, 0.5, 0.0],
        ));
    let affine: Affine = (
        10.0,
        [
            [0.18, 0.5, 1.7],
            [0.4, -0.1, 0.0],
            [1.5, 0.9, 2.2],
            [0.01, 0.2, -0.05],
        ],
    );
    let cdl = cdl_calls(
        [1.35, 1.1, 0.071],
        [0.05, -0.23, 0.11],
        [0.93, 0.81, 1.27],
        1.23,
        true,
    );
    let exponent = exponent_calls([2.2, 2.4, 1.8, 1.0], "NEGATIVE_CLAMP");
    let bases = vec![
        Case::new("log, matrix, range", log_matrix_range.clone()),
        Case::new("two matrices", two_matrices.clone()),
    ];
    let mut cases = bases.clone();
    cases.extend([
        Case::new("empty", group()),
        Case::new(
            "a log and its inverse",
            group()
                .child(log_calls(10.0))
                .child_in(log_calls(10.0), inverse),
        ),
        Case::new(
            "an affine log and its inverse",
            group()
                .child_in(log_affine_calls(affine), inverse)
                .child(log_affine_calls(affine)),
        ),
        Case::new(
            "an exponent and its inverse",
            group()
                .child(exponent.clone())
                .child_in(exponent.clone(), inverse),
        ),
        Case::new(
            "two exponents",
            group()
                .child(exponent)
                .child(exponent_calls([0.5, 1.25, 2.0, 1.0], "NEGATIVE_CLAMP")),
        ),
        Case::new(
            "a CDL and its inverse",
            group().child(cdl.clone()).child_in(cdl.clone(), inverse),
        ),
        Case::new(
            "two ranges",
            group()
                .child(range_calls([0.0, 1.0, 0.25, 1.5], true))
                .child(range_calls([0.5, 1.0, 0.0, 2.0], true)),
        ),
        Case::new(
            "inverse nested",
            group().child_in(log_matrix_range, inverse).child(
                group()
                    .child(allocation_calls("ALLOCATION_LG2", &[-8.0, 5.0, 0.002]))
                    .child_in(
                        exponent_with_linear_calls(
                            [2.4, 2.2, 1.0, 1.8],
                            [0.055, 0.2, 0.0, 0.6],
                            "NEGATIVE_LINEAR",
                        ),
                        inverse,
                    ),
            ),
        ),
        Case::new(
            "camera log, CDL, matrices",
            group()
                .child(log_camera_calls(
                    [0.010591; 3],
                    (
                        10.0,
                        [[0.247190; 3], [0.385537; 3], [5.555556; 3], [0.052272; 3]],
                    ),
                    None,
                ))
                .child(cdl)
                .child(two_matrices.clone())
                .child_in(two_matrices, inverse),
        ),
        Case::new(
            "uniform allocations",
            group()
                .child(allocation_calls("ALLOCATION_UNIFORM", &[-0.125, 1.125]))
                .child_in(
                    allocation_calls("ALLOCATION_UNIFORM", &[-0.125, 1.125]),
                    inverse,
                ),
        ),
        Case::new(
            "a NaN in a matrix pair",
            group()
                .child(matrix_calls(diagonal([2.0, f64::NAN, 0.5, 1.0]), [0.0; 4]))
                .child(matrix_calls(diagonal([0.5, 2.0, 2.0, 1.0]), [0.1; 4])),
        )
        // The NaN is compared bit for bit: the combined matrix's renderers follow each wheel's
        // operand orders, as the Matrix family's NaN cases do.
        .w0002_nowhere(),
    ]);
    Cases { cases, bases }
}

/// `ExponentTransform` in a version 1 config, where it builds an Exponent op
/// (`BuildExponentOp`, src/OpenColorIO/ops/gamma/GammaOp.cpp:190-215 @ v2.5.2): the cases of
/// `crates/ocio-ops/tests/exponent_oracle.rs`. Through `getProcessor(transform)` the transform
/// is validated first, so the values outside the Gamma op's bounds are refused there.
pub(crate) fn exponent_v1() -> Cases {
    let case = |label: &str, value: [f64; 4]| {
        Case::new(
            label,
            Calls::new("ExponentTransform").rgba("setValue", value),
        )
    };
    let mut cases = vec![
        case("value", [1.2, 1.3, 1.4, 1.5]),
        case("value_limits", [0.0, 2.0, -2.0, 1.5]),
        case("combining 1", [2.0, 2.0, 2.0, 1.0]),
        case("combining 2", [1.2, 1.2, 1.2, 1.0]),
        case("combining 3", [1.037289, 1.019015, 0.966082, 1.0]),
        case("cache_id 1", [2.0, 2.1, 3.0, 3.1]),
        case("identity", [1.0, 1.0, 1.0, 1.0]),
        case("near identity", [1.0000002, 1.0, 1.0, 1.0]),
    ];
    let bases = vec![cases[0].clone()];
    cases.push(case("NaN", [f64::NAN, 2.0, 2.0, 1.0]).w0002_nowhere());
    Cases { cases, bases }
}

/// A `Lut1DTransform` of `length` entries, `f(i)` for entry `i`, in R, G and B. The entries
/// are not battery slots: no test mutates them, and the battery doesn't run this class.
pub(crate) fn lut1d_calls(length: u64, f: impl Fn(u64) -> [f32; 3]) -> Calls {
    let mut calls = Calls::new("Lut1DTransform").fixed("setLength", serde_json::json!(length));
    for i in 0..length {
        let [r, g, b] = f(i).map(f64::from);
        calls = calls.call(
            "setValue",
            vec![
                Arg::Fixed(serde_json::json!(i)),
                Arg::Fixed(num(r)),
                Arg::Fixed(num(g)),
                Arg::Fixed(num(b)),
            ],
        );
    }
    calls
}

/// `Lut1DTransform`: curves of 256 entries (the length an 8-bit input looks up without
/// resampling), 65 and 1024 entries, nearest and linear interpolation, an output beyond [0, 1]
/// and a decreasing one. Phase 1 has the lookups of integer and half input to float only: the
/// float renderers, composing LUTs, the inverse LUT and the hue adjustment are Phase 2's (WP
/// 2.1, 2.5), and the tests list the port's "not ported yet" refusals as deferrals.
pub(crate) fn lut1d() -> Cases {
    let curve = |n: u64| {
        move |i: u64| {
            let x = i as f32 / (n - 1) as f32;
            [x * x, x.sqrt(), 0.25 + 0.5 * x]
        }
    };
    let cases = vec![
        Case::new("256 entries", lut1d_calls(256, curve(256))),
        Case::new("65 entries", lut1d_calls(65, curve(65))),
        Case::new("1024 entries", lut1d_calls(1024, curve(1024))),
        Case::new(
            "256 entries, nearest",
            lut1d_calls(256, curve(256)).enumerated("setInterpolation", "INTERP_NEAREST"),
        ),
        Case::new(
            "256 entries, wide and decreasing",
            lut1d_calls(256, |i| {
                let x = i as f32 / 255.0;
                [2.0 - 3.0 * x, -0.5 + 2.0 * x, 1.0 - x]
            }),
        ),
        Case::new(
            "256 entries, hue adjust",
            lut1d_calls(256, curve(256)).enumerated("setHueAdjust", "HUE_DW3"),
        ),
    ];
    Cases {
        cases,
        bases: Vec::new(),
    }
}

/// `Lut1DTransform`s of the lengths a 12-bit and a 16-bit input look up without resampling
/// (4096 and 65536 entries), and a half-domain one (65536 entries, one per half code), which a
/// half input looks up: the lookups of the other inputs than [`lut1d`]'s through a
/// `Lut1DTransform`. Their specs are large (a setter per entry), so the format sweep runs them
/// on few combinations, and the GPU sweep not at all.
pub(crate) fn lut1d_lookups() -> Cases {
    let ramp = |n: u64| {
        move |i: u64| {
            let x = i as f32 / (n - 1) as f32;
            [x.sqrt(), 1.5 * x - 0.25, 1.0 - x * x]
        }
    };
    let cases = vec![
        Case::new("4096 entries", lut1d_calls(4096, ramp(4096))),
        Case::new("65536 entries", lut1d_calls(65536, ramp(65536))),
        Case::new(
            "half domain",
            lut1d_calls(65536, ramp(65536)).fixed("setInputHalfDomain", serde_json::json!(true)),
        ),
    ];
    Cases {
        cases,
        bases: Vec::new(),
    }
}

/// A `Lut3DTransform` of `grid_size` entries per side, `f(r, g, b)` for each entry (its grid
/// position scaled to [0, 1]), with `interpolation` (an `INTERP_*` name). The entries are not
/// battery slots.
pub(crate) fn lut3d_calls(
    grid_size: u64,
    interpolation: &str,
    f: impl Fn([f32; 3]) -> [f32; 3],
) -> Calls {
    let mut calls = Calls::new("Lut3DTransform").fixed("setGridSize", serde_json::json!(grid_size));
    let last = grid_size.saturating_sub(1).max(1) as f32;
    for i in 0..grid_size {
        for j in 0..grid_size {
            for k in 0..grid_size {
                let [r, g, b] = f([i, j, k].map(|x| x as f32 / last)).map(f64::from);
                calls = calls.call(
                    "setValue",
                    [i, j, k]
                        .map(|x| Arg::Fixed(serde_json::json!(x)))
                        .into_iter()
                        .chain([r, g, b].map(|v| Arg::Fixed(num(v))))
                        .collect(),
                );
            }
        }
    }
    calls.enumerated("setInterpolation", interpolation)
}

/// `Lut3DTransform`: smooth and folded cubes of 2 to 17 entries per side in each
/// interpolation, and groups of a LUT and its inverse in either order, which the optimizer
/// replaces with a [0, 1] range (`OPTIMIZATION_PAIR_IDENTITY_LUT3D`,
/// `Lut3DOpData::getIdentityReplacement`), and of two LUTs, which it composes where the level
/// has `OPTIMIZATION_COMP_LUT3D`. An inverse LUT renders with its fast forward LUT
/// (`OPTIMIZATION_LUT_INV_FAST`), or without the flag with its exact inverse. The cube of one
/// entry is [`lut3d_one_entry`]'s.
pub(crate) fn lut3d() -> Cases {
    let smooth = |[r, g, b]: [f32; 3]| [r * r * 0.9 + 0.05, g.sqrt(), 0.2 + 0.6 * b + 0.1 * r];
    let folded = |[r, g, b]: [f32; 3]| [1.5 * g - 0.25, (r - b).abs(), 1.0 - r * g];
    let mut cases = Vec::new();
    for interp in [
        "INTERP_TETRAHEDRAL",
        "INTERP_LINEAR",
        "INTERP_BEST",
        "INTERP_NEAREST",
    ] {
        cases.push(Case::new(
            format!("smooth 5^3, {interp}"),
            lut3d_calls(5, interp, smooth),
        ));
    }
    cases.extend([
        Case::new(
            "folded 3^3, INTERP_DEFAULT",
            lut3d_calls(3, "INTERP_DEFAULT", folded),
        ),
        Case::new(
            "folded 2^3, INTERP_TETRAHEDRAL",
            lut3d_calls(2, "INTERP_TETRAHEDRAL", folded),
        ),
        Case::new(
            "smooth 17^3, INTERP_TETRAHEDRAL",
            lut3d_calls(17, "INTERP_TETRAHEDRAL", smooth),
        ),
    ]);
    let group = || Calls::new("GroupTransform");
    let inverse = Direction::Inverse;
    for (fwd, inv) in [
        ("INTERP_TETRAHEDRAL", "INTERP_TETRAHEDRAL"),
        ("INTERP_LINEAR", "INTERP_LINEAR"),
        ("INTERP_LINEAR", "INTERP_TETRAHEDRAL"),
    ] {
        cases.push(Case::new(
            format!("a LUT ({fwd}) and its inverse ({inv})"),
            group()
                .child(lut3d_calls(3, fwd, folded))
                .child_in(lut3d_calls(3, inv, folded), inverse),
        ));
        cases.push(Case::new(
            format!("an inverse LUT ({inv}) and the LUT ({fwd})"),
            group()
                .child_in(lut3d_calls(3, inv, folded), inverse)
                .child(lut3d_calls(3, fwd, folded)),
        ));
    }
    cases.push(Case::new(
        "two LUTs",
        group()
            .child(lut3d_calls(3, "INTERP_LINEAR", folded))
            .child(lut3d_calls(5, "INTERP_TETRAHEDRAL", smooth)),
    ));
    Cases {
        cases,
        bases: Vec::new(),
    }
}

/// The `Lut3DTransform` of one entry per side, the default cube (a NaN identity, I-152), for
/// the forward direction only: its inverse never returns in the wheel (U-65).
pub(crate) fn lut3d_one_entry() -> Cases {
    Cases {
        cases: vec![Case::new(
            "the default cube of 1 entry",
            Calls::new("Lut3DTransform").fixed("setGridSize", serde_json::json!(1)),
        )],
        bases: Vec::new(),
    }
}

/// A `FixedFunctionTransform` of `style` (a `FIXED_FUNCTION_*` name) and `params`: the
/// binding's constructor of a style without parameters, `ACES_GLOW_03`, which validates, then
/// `setStyle` and `setParams`, which don't. The ACES 1.3 gamut compression's limits and
/// thresholds apply to one channel each (cyan to red, magenta to green, yellow to blue), its
/// power to the three; every other style's parameters to the three.
pub(crate) fn fixed_function_calls(style: &str, params: &[f64]) -> Calls {
    use ocio_testkit::battery::params::{B, G, R};
    let calls = Calls::new("FixedFunctionTransform")
        .arg_fixed(
            "style",
            serde_json::json!({"enum": "FIXED_FUNCTION_ACES_GLOW_03"}),
        )
        .enumerated("setStyle", style);
    if params.is_empty() {
        return calls;
    }
    let channels: Vec<_> = if style == "FIXED_FUNCTION_ACES_GAMUT_COMP_13" && params.len() == 7 {
        vec![R, G, B, R, G, B, RGB]
    } else {
        vec![RGB; params.len()]
    };
    let list = params.iter().copied().zip(channels).collect();
    calls.call("setParams", vec![Arg::List(list)])
}

/// The ACES 1.3 gamut compression's parameters, as upstream's tests use them
/// (tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:393 @ v2.5.2).
const GAMUT_COMP_13: [f64; 7] = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];

/// `FixedFunctionTransform`: the cases of `crates/ocio-ops/tests/fixed_function_oracle.rs` for
/// the styles whose renderers are ported, and a pair of inverse transforms, which the
/// optimizer removes.
pub(crate) fn fixed_function() -> Cases {
    let mut cases: Vec<Case<Calls>> = [
        "FIXED_FUNCTION_ACES_RED_MOD_03",
        "FIXED_FUNCTION_ACES_RED_MOD_10",
        "FIXED_FUNCTION_ACES_GLOW_03",
        "FIXED_FUNCTION_ACES_GLOW_10",
        "FIXED_FUNCTION_ACES_DARK_TO_DIM_10",
        "FIXED_FUNCTION_RGB_TO_HSV",
        "FIXED_FUNCTION_RGB_TO_HSY_LIN",
        "FIXED_FUNCTION_RGB_TO_HSY_LOG",
        "FIXED_FUNCTION_RGB_TO_HSY_VID",
        "FIXED_FUNCTION_XYZ_TO_xyY",
        "FIXED_FUNCTION_XYZ_TO_uvY",
        "FIXED_FUNCTION_XYZ_TO_LUV",
    ]
    .into_iter()
    .map(|style| Case::new(style, fixed_function_calls(style, &[])))
    .collect();
    let gamut = Case::new(
        "ACES_GAMUT_COMP_13",
        fixed_function_calls("FIXED_FUNCTION_ACES_GAMUT_COMP_13", &GAMUT_COMP_13),
    );
    let mut bases = vec![gamut.clone()];
    cases.push(gamut);
    for (label, params) in [
        ("lower bounds", [1.001, 1.001, 1.001, 0.0, 0.0, 0.0, 1.0]),
        (
            "upper bounds",
            [65504.0, 65504.0, 65504.0, 0.9995, 0.9995, 0.9995, 65504.0],
        ),
        ("mixed", [1.5, 2.0, 1.01, 0.5, 0.9, 0.2, 3.0]),
    ] {
        cases.push(Case::new(
            format!("ACES_GAMUT_COMP_13 {label}"),
            fixed_function_calls("FIXED_FUNCTION_ACES_GAMUT_COMP_13", &params),
        ));
    }
    // Random sets within the bounds, as the op battery runs them
    // (crates/ocio-ops/tests/common/fixed_function.rs, `random_gamut_comp_13_params`); the
    // digest pins them.
    let random = random_gamut_comp_13_params(6);
    let bytes: Vec<u8> = random
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(xxhash_rust::xxh3::xxh3_64(&bytes), 0xdec4_928a_cfc1_3ecc);
    for (i, params) in random.iter().enumerate() {
        cases.push(Case::new(
            format!("ACES_GAMUT_COMP_13 random {i}"),
            fixed_function_calls("FIXED_FUNCTION_ACES_GAMUT_COMP_13", params),
        ));
    }
    // tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:996, 1030 @ v2.5.2.
    let surround = Case::new(
        "REC2100_SURROUND 0.78",
        fixed_function_calls("FIXED_FUNCTION_REC2100_SURROUND", &[0.78]),
    );
    bases.push(surround.clone());
    cases.push(surround);
    cases.push(Case::new(
        "REC2100_SURROUND 1.2",
        fixed_function_calls("FIXED_FUNCTION_REC2100_SURROUND", &[1.2]),
    ));
    cases.push(Case::new(
        "REC2100_SURROUND NaN",
        fixed_function_calls("FIXED_FUNCTION_REC2100_SURROUND", &[f64::NAN]),
    ));
    // The Rec.2100 HLG curve, and a double log (tests/cpu/ops/fixedfunction/
    // FixedFunctionOpCPU_tests.cpp:1311-1325, 1374-1382 @ v2.5.2).
    let hlg = [
        0.0,
        0.25,
        0.5,
        1.0,
        0.0,
        std::f64::consts::E,
        0.17883277,
        0.807825590164,
        1.0,
        -0.07116723,
    ];
    let gamma_log = Case::new(
        "LIN_TO_GAMMA_LOG HLG",
        fixed_function_calls("FIXED_FUNCTION_LIN_TO_GAMMA_LOG", &hlg),
    );
    bases.push(gamma_log.clone());
    cases.push(gamma_log);
    let double_log = Case::new(
        "LIN_TO_DOUBLE_LOG",
        fixed_function_calls(
            "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
            &[
                10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
            ],
        ),
    );
    bases.push(double_log.clone());
    cases.push(double_log);
    // ACES 2.0, as upstream's tests run it (tests/cpu/ops/fixedfunction/
    // FixedFunctionOpCPU_tests.cpp @ v2.5.2): the output transform and the gamut compression to
    // P3-D65 at 1000 nits, the tone scale at 1000 nits, RGB to JMh of AP0 (773). Not bases: a
    // NaN primary makes the wheel's hue table code write past its arrays (U-32).
    let p3_d65_1000 = [
        1000.0, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.3127, 0.3290,
    ];
    let ap0 = [
        0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
    ];
    for (style, params) in [
        ("FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20", &p3_d65_1000[..]),
        ("FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20", &p3_d65_1000[..]),
        (
            "FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20",
            &p3_d65_1000[..1],
        ),
        ("FIXED_FUNCTION_ACES_RGB_TO_JMH_20", &ap0[..]),
    ] {
        cases.push(Case::new(
            style.trim_start_matches("FIXED_FUNCTION_"),
            fixed_function_calls(style, params),
        ));
    }
    // W0001: without fast math, the Windows wheel computes PQ with SVML's pow, the port with
    // powf; Linux and fast math compare bit for bit.
    cases.push(
        Case::new(
            "LIN_TO_PQ",
            fixed_function_calls("FIXED_FUNCTION_LIN_TO_PQ", &[]),
        )
        .w0001(W0001Function::LinToPq, W0001Function::PqToLin),
    );
    let mut nan = GAMUT_COMP_13;
    nan[6] = f64::NAN;
    cases.push(Case::new(
        "ACES_GAMUT_COMP_13 NaN power",
        fixed_function_calls("FIXED_FUNCTION_ACES_GAMUT_COMP_13", &nan),
    ));
    let mut low = GAMUT_COMP_13;
    low[0] = 1.0;
    cases.push(Case::new(
        "refused lim_cyan 1",
        fixed_function_calls("FIXED_FUNCTION_ACES_GAMUT_COMP_13", &low),
    ));
    cases.push(Case::new(
        "refused glow parameter",
        fixed_function_calls("FIXED_FUNCTION_ACES_GLOW_03", &[1.0]),
    ));
    cases.push(Case::new(
        "refused 6 parameters",
        fixed_function_calls("FIXED_FUNCTION_ACES_GAMUT_COMP_13", &GAMUT_COMP_13[..6]),
    ));
    cases.push(Case::new(
        "glow and its inverse",
        Calls::new("GroupTransform")
            .child(fixed_function_calls("FIXED_FUNCTION_ACES_GLOW_10", &[]))
            .child_in(
                fixed_function_calls("FIXED_FUNCTION_ACES_GLOW_10", &[]),
                Direction::Inverse,
            ),
    ));
    Cases { cases, bases }
}

/// Random ACES 1.3 gamut compression parameters within `validate`'s bounds, as
/// `crates/ocio-ops/tests/common/fixed_function.rs` generates them for the op battery.
fn random_gamut_comp_13_params(n: usize) -> Vec<[f64; 7]> {
    let mut rng = ocio_testkit::probe::Rng::new(0x6a3c_0013);
    let mut unit = move || (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    (0..n)
        .map(|_| {
            let mut p = [0.0; 7];
            for v in &mut p[..3] {
                *v = 1.001 + 1.499 * unit();
            }
            for v in &mut p[3..6] {
                *v = 0.9995 * unit();
            }
            p[6] = 1.0 + 4.0 * unit();
            p
        })
        .collect()
}

/// `BuiltinTransform`: the built-in transforms named, each a case. They have no parameters, so
/// the battery generates no cases from them.
pub(crate) fn builtin(styles: &[&str]) -> Cases {
    let cases = styles
        .iter()
        .map(|style| {
            Case::new(
                *style,
                Calls::new("BuiltinTransform").fixed("setStyle", serde_json::json!(style)),
            )
        })
        .collect();
    Cases {
        cases,
        bases: Vec::new(),
    }
}

/// The built-in transforms whose ops are ported (WP 3.2e-g): the identity, the ARRI, Panasonic,
/// RED and Sony cameras, and the ACES and display entries built from Phase 1 ops.
pub(crate) const BUILTINS_WITH_OPS: &[&str] = &[
    "IDENTITY",
    "ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1",
    "ARRI_LOGC4_to_ACES2065-1",
    "PANASONIC_VLOG-VGAMUT_to_ACES2065-1",
    "RED_REDLOGFILM-RWG_to_ACES2065-1",
    "RED_LOG3G10-RWG_to_ACES2065-1",
    "SONY_SLOG3-SGAMUT3_to_ACES2065-1",
    "SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1",
    "SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1",
    "SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1",
    "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
    "UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD",
    "UTILITY - ACES-AP1_to_LINEAR-REC709_BFD",
    "CURVE - ACEScct-LOG_to_LINEAR",
    "ACEScct_to_ACES2065-1",
    "ACEScg_to_ACES2065-1",
    "ACESproxy10i_to_ACES2065-1",
    "ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX",
    "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
    "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS",
    "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
    "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
    "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709",
    "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS",
    "DISPLAY - CIE-XYZ-D65_to_sRGB",
    "DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS",
    "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD",
    "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65",
    "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS",
    "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D60-BFD",
    "DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
    "DISPLAY - CIE-XYZ-D65_to_DisplayP3",
    "DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
];

/// One `GradingRGBCurveTransform`: its style, its four curves (red, green, blue, master) with
/// their slopes, and whether it bypasses the lin-to-log conversion and is dynamic. The port
/// builds it as op data until Phase 3 ports the transform ([`RgbCurve::op_data`]); the wheel
/// builds the transform from its spec ([`RgbCurve::calls`]).
#[derive(Debug, Clone)]
pub(crate) struct RgbCurve {
    style: GradingStyle,
    curves: [Vec<(f32, f32)>; 4],
    slopes: [Vec<f32>; 4],
    bypass: bool,
    dynamic: bool,
}

impl RgbCurve {
    fn new(style: GradingStyle, curves: [&[(f32, f32)]; 4]) -> RgbCurve {
        RgbCurve {
            style,
            curves: curves.map(<[(f32, f32)]>::to_vec),
            slopes: curves.map(|c| vec![0.0; c.len()]),
            bypass: false,
            dynamic: false,
        }
    }

    fn bypass(mut self) -> RgbCurve {
        self.bypass = true;
        self
    }

    fn dynamic(mut self) -> RgbCurve {
        self.dynamic = true;
        self
    }

    fn slopes(mut self, c: usize, slopes: &[f32]) -> RgbCurve {
        self.slopes[c] = slopes.to_vec();
        self
    }

    fn curve(&self, c: usize) -> GradingBSplineCurve {
        let points: Vec<GradingControlPoint> = self.curves[c]
            .iter()
            .map(|&(x, y)| GradingControlPoint::new(x, y))
            .collect();
        let mut curve = GradingBSplineCurve::with_points(&points);
        for (i, &s) in self.slopes[c].iter().enumerate() {
            curve.set_slope(i, s).expect("a slope per point");
        }
        curve
    }

    /// The transform's spec: the binding's constructor of the curves (object specs), the
    /// style and whether it is dynamic, then `setBypassLinToLog` (the direction follows).
    pub(crate) fn calls(&self) -> Calls {
        let f = |v: f32| f64_spec(f64::from(v));
        let curve = |c: usize| {
            let values: Vec<serde_json::Value> = self.curves[c]
                .iter()
                .flat_map(|&(x, y)| [f(x), f(y)])
                .collect();
            let slopes: Vec<serde_json::Value> = self.slopes[c].iter().map(|&s| f(s)).collect();
            serde_json::json!({"object": {"class": "GradingBSplineCurve", "args": [values],
                "calls": [["setSlopes", slopes]]}})
        };
        let style = match self.style {
            GradingStyle::Log => "GRADING_LOG",
            GradingStyle::Lin => "GRADING_LIN",
            GradingStyle::Video => "GRADING_VIDEO",
        };
        Calls::new("GradingRGBCurveTransform")
            .arg_fixed(
                "values",
                serde_json::json!({"object": {"class": "GradingRGBCurve", "args": {
                    "red": curve(0), "green": curve(1), "blue": curve(2), "master": curve(3),
                }}}),
            )
            .arg_fixed("style", serde_json::json!({"enum": style}))
            .arg_fixed("dynamic", serde_json::json!(self.dynamic))
            .fixed("setBypassLinToLog", serde_json::json!(self.bypass))
    }

    /// The op data the transform holds in the direction `dir`, as the binding's constructor,
    /// `setBypassLinToLog` and `setDirection` make it, validated as
    /// `Processor::Impl::setTransform` validates the transform
    /// (src/bindings/python/transforms/PyGradingRGBCurveTransform.cpp:17-34,
    /// src/OpenColorIO/Processor.cpp:623-641 @ v2.5.2).
    pub(crate) fn op_data(&self, dir: Direction) -> ocio_ops::Result<GradingRgbCurveOpData> {
        let curves = GradingRgbCurve::with_curves(
            &self.curve(0),
            &self.curve(1),
            &self.curve(2),
            &self.curve(3),
        );
        let mut data = GradingRgbCurveOpData::new(self.style);
        data.set_value(&curves)?;
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_bypass_lin_to_log(self.bypass);
        data.set_direction(match dir {
            Direction::Forward => TransformDirection::Forward,
            Direction::Inverse => TransformDirection::Inverse,
        });
        data.validate()?;
        Ok(data)
    }
}

/// `GradingRGBCurveTransform`, through its op data until Phase 3 ports the transform: upstream's
/// CPU test curves (tests/cpu/ops/gradingrgbcurve/GradingRGBCurveOpCPU_tests.cpp @ v2.5.2) and
/// the ACES RRT shaper (src/OpenColorIO/transforms/builtins/ACES.cpp:178-199), in each style,
/// bypassed, dynamic and not, as the GPU writer's test runs them
/// (crates/ocio-gpu/tests/grading_rgb_curve_op_gpu_oracle.rs). Finite curves only: NaN ones
/// are W0002's, which the op battery covers (crates/ocio-ops/tests/grading_rgb_curve_oracle.rs).
pub(crate) fn rgb_curve() -> Vec<(&'static str, RgbCurve)> {
    use GradingStyle::{Lin, Log, Video};
    let identity: &[(f32, f32)] = &[(0.0, 0.0), (1.0, 1.0)];
    let lin_rgb: &[(f32, f32)] = &[(-6.0, -8.0), (-2.0, -5.0), (2.0, 4.0), (5.0, 6.0)];
    let lin_m: &[(f32, f32)] = &[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)];
    let log = RgbCurve::new(
        Log,
        [
            &[(0.1, 0.15), (0.55, 0.45), (0.9, 1.1)],
            &[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)],
            &[(0.1, 0.15), (0.55, 0.85), (0.9, 1.1)],
            &[(-0.1, 0.1), (1.1, 1.3)],
        ],
    );
    let lin = RgbCurve::new(Lin, [lin_rgb, lin_rgb, lin_rgb, lin_m]);
    let rrt = RgbCurve::new(
        Log,
        [
            identity,
            identity,
            identity,
            &[
                (-5.26017743, -4.0),
                (-3.75502745, -3.57868829),
                (-2.24987747, -1.82131329),
                (-0.74472749, 0.68124124),
                (1.06145248, 2.87457742),
                (2.86763245, 3.83406206),
                (4.67381243, 4.0),
            ],
        ],
    )
    .slopes(
        3,
        &[
            0.0, 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.0,
        ],
    );
    let video = RgbCurve::new(Video, [lin_m; 4]);
    vec![
        ("log", log.clone()),
        ("log dynamic", log.dynamic()),
        ("lin", lin.clone()),
        ("lin bypass", lin.clone().bypass()),
        ("lin dynamic", lin.dynamic()),
        ("ACES RRT shaper", rrt),
        ("video identity", video.clone()),
        ("video identity dynamic", video.dynamic()),
    ]
}

/// [`rgb_curve`]'s transforms as the sweeps' cases (no numbers to mutate: the curves are
/// object specs).
pub(crate) fn rgb_curve_cases() -> Cases {
    Cases {
        cases: rgb_curve()
            .into_iter()
            .map(|(label, c)| Case::new(label, c.calls()))
            .collect(),
        bases: Vec::new(),
    }
}
