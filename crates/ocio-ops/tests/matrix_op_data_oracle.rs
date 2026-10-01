// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `MatrixOpData`'s own double math against the wheel: the inverse (`getAsForward`) and the
//! composition (`compose`, with `cleanUp`), bit for bit, through the data's cache ID, which
//! hashes the bits of the 16 matrix values and the 4 offsets (MatrixOpData.cpp:846-869 @
//! v2.5.2).
//!
//! The wheel's CPU processor names its ops' cache IDs in its own
//! (`CPUProcessor::Impl::finalize`, src/OpenColorIO/CPUProcessor.cpp:370-376), and a Matrix op's
//! is `<MatrixOffsetOp ` + its data's + ` >` (`MatrixOffsetOp::getCacheID`,
//! src/OpenColorIO/ops/matrix/MatrixOp.cpp:173-182 @ v2.5.2). The oracle's `cpu_apply` returns
//! it:
//! - a `MatrixTransform` applied inverse, without optimization, is one op, whose data
//!   `MatrixOffsetOp::finalize` replaces with `getAsForward()` (MatrixOp.cpp:164-171);
//! - a `GroupTransform` of two forward `MatrixTransform`s, with the default optimization, is one
//!   op too: the optimizer combines the pair, `MatrixOffsetOp::combineWith`, which is
//!   `first.compose(second)` (MatrixOp.cpp:147-162).
//!
//! The port builds the same data as `BuildMatrixOp` and `CreateMatrixOp` do
//! (MatrixOp.cpp:341-352, 379-388): the transform's matrix and offsets, in the direction asked.
//!
//! JSON can't hold NaN or infinite values, so the matrices with them reach the wheel in a
//! config's YAML, as the battery's `Spec::Yaml` does: `.nan` is yaml-cpp's
//! `std::numeric_limits<double>::quiet_NaN()`, positive, while the NaNs the arithmetic generates
//! (`inf - inf`, `0 * inf`) are x86's default NaN, negative. Where the two meet in a product or
//! a sum, the result shows which operand came first.

use ocio_ops::exception::Result;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_testkit::Oracle;
use ocio_testkit::battery::{Combo, Direction, Format, Spec, yaml_number};
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// A matrix and its offsets.
type Matrix = ([f64; 16], [f64; 4]);

/// The port's data of a `MatrixTransform` with `matrix` in the direction `dir`.
fn port_data((m, o): &Matrix, dir: TransformDirection) -> MatrixOpData {
    let mut data = MatrixOpData::new();
    data.set_rgba(m);
    data.set_rgba_offsets(o);
    data.set_direction(dir);
    data
}

/// The transform spec of a `MatrixTransform`.
fn transform((m, o): &Matrix) -> Value {
    json!({"class": "MatrixTransform", "args": {"matrix": m.to_vec(), "offset": o.to_vec()}})
}

/// A double in about [-2^scale, 2^scale], of every sign.
fn value(rng: &mut Rng, scale: i32) -> f64 {
    let unit = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    (unit * 2.0 - 1.0) * 2f64.powi(scale)
}

/// Matrices to invert and compose: upstream's tests', permutations (pivoting), diagonal ones,
/// near-singular ones, and random ones at several magnitudes, some with integer entries (which
/// `cleanUp` snaps to).
fn matrices() -> Vec<Matrix> {
    let mut out: Vec<Matrix> = vec![
        // tests/cpu/ops/matrix/MatrixOpData_tests.cpp:576-581 @ v2.5.2.
        (
            [
                0.9f32, 0.8, -0.7, 0.6, -0.4, 0.5, 0.3, 0.2, 0.1, -0.2, 0.4, 0.3, -0.5, 0.6, 0.7,
                0.8,
            ]
            .map(f64::from),
            [-0.1f32, 0.2, -0.3, 0.4].map(f64::from),
        ),
        (
            [
                2., 0., 0., 0., 0., 4., 0., 0., 0., 0., 0.5, 0., 0., 0., 0., 1.,
            ],
            [1., 2., 0., 0.5],
        ),
        // A permutation: every pivot needs a swap.
        (
            [
                0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 1., 0., 0., 0.,
            ],
            [0.; 4],
        ),
        // Nearly singular.
        (
            [
                1., 2., 3., 4., 2., 4., 6., 8.000001, 1., 0., 1., 0., 0., 1., 0., 1.,
            ],
            [0.25, -0.5, 0., 1e-9],
        ),
        // Ties for every pivot of the first column, and for the second's: the first of the
        // largest rows is the pivot.
        (
            [
                0.75, 0.3, -0.2, 0.1, -0.75, 0.6, 0.15, -0.4, 0.75, -0.6, 0.5, 0.25, 0.75, 0.05,
                -0.45, 0.9,
            ],
            [0.5, -0.25, 0.125, 0.],
        ),
    ];
    let mut rng = Rng::new(0x3a7_1a1b);
    for scale in [-8, -1, 0, 1, 4, 30] {
        for _ in 0..12 {
            let mut m = [0.; 16];
            for v in &mut m {
                *v = value(&mut rng, scale);
            }
            let mut o = [0.; 4];
            for v in &mut o {
                *v = value(&mut rng, scale);
            }
            out.push((m, o));
        }
    }
    // Integers, and values within a hair of them.
    for _ in 0..12 {
        let mut m = [0.; 16];
        for v in &mut m {
            *v = (value(&mut rng, 3)).round();
        }
        m[5] += 1e-9;
        out.push((m, [1., -2., 3., 0.]));
    }
    out
}

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// Pairs whose composition `cleanUp` snaps to integers, or just doesn't: values a fraction of the
/// tolerance (`1e-7` times the largest) from an integer, or a few times it; matrices and
/// offsets under its floor (`1e-4`); and offsets whose tolerance only the second matrix's
/// offsets set. The second matrix scales blue, so that neither is the identity.
fn snap_pairs() -> Vec<(Matrix, Matrix)> {
    let second = diagonal([1., 1., 2., 1.]);
    let tiny = {
        let mut m = diagonal([1e-5, 2e-5, 3e-5, 4e-5]);
        m[1] = 5e-12;
        m
    };
    vec![
        // 1.5e-7 from 2, for a tolerance of 3e-7: snapped.
        (
            (diagonal([2. + 1.5e-7, 3., 1., 1.]), [0.; 4]),
            (second, [0.; 4]),
        ),
        // 4e-7 from 2: kept.
        (
            (diagonal([2. + 4e-7, 3., 1., 1.]), [0.; 4]),
            (second, [0.; 4]),
        ),
        // An offset 5e-8 from 1, for a tolerance of about 1e-7: snapped; and one 4e-7 from 3:
        // kept.
        (
            (diagonal([2., 1., 1., 1.]), [1. + 5e-8, 3. + 4e-7, 0., 0.]),
            (second, [0.; 4]),
        ),
        // The second matrix's offset of about 1000 makes the tolerance 1e-4: snapped.
        (
            (diagonal([2., 1., 1., 1.]), [0.; 4]),
            (second, [1000. + 5e-6, 0., 0., 0.]),
        ),
        // A matrix under the floor: 5e-12 is within 1e-11 of 0, and snapped.
        ((tiny, [0.; 4]), (diagonal([1.; 4]), [0.5, 0., 0., 0.])),
        // Offsets under the floor: 5e-12 snapped, 1e-5 kept.
        (
            (diagonal([2., 1., 1., 1.]), [5e-12, 0., 0., 0.]),
            (second, [0., 1e-5, 0., 0.]),
        ),
        // A value exactly at the tolerance, 3 * 1e-7 for the largest value 3: kept, as the
        // comparison is strict.
        (
            (
                {
                    let mut m = diagonal([2., 3., 1., 1.]);
                    m[1] = 3. * 1e-7;
                    m
                },
                [0.; 4],
            ),
            (second, [0.; 4]),
        ),
        // An offset exactly at the offsets' tolerance, 2 * 1e-7 for the largest offset 2: kept.
        (
            (diagonal([2., 1., 1., 1.]), [0.; 4]),
            (second, [2., 2. * 1e-7, 0., 0.]),
        ),
    ]
}

/// A matrix entry or offset among NaN, the infinities, 0, and multiples of 1/8 in [-2, 2], which
/// every parser reads exactly.
fn special_value(rng: &mut Rng) -> f64 {
    match rng.next_u64() % 10 {
        0 | 1 => f64::NAN,
        2 => f64::INFINITY,
        3 => f64::NEG_INFINITY,
        4 => 0.,
        _ => ((rng.next_u64() % 33) as f64 - 16.) / 8.,
    }
}

/// Matrices with NaN and infinite values, whose inverses and products make NaNs of both signs
/// meet: one where a generated NaN meets a `.nan` in the first step of the elimination
/// (`inf / inf`, then `3 - f * .nan`), and random ones.
fn special_matrices() -> Vec<Matrix> {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    let mut out: Vec<Matrix> = vec![(
        [
            inf, nan, 1., 2., inf, 3., 4., 5., 1., 0., 2., 0., 0., 1., 0., 3.,
        ],
        [nan, 1., -inf, 0.],
    )];
    let mut rng = Rng::new(0x5bec_1a15);
    for _ in 0..160 {
        let mut m = [0.; 16];
        for v in &mut m {
            *v = special_value(&mut rng);
        }
        let mut o = [0.; 4];
        for v in &mut o {
            *v = special_value(&mut rng);
        }
        out.push((m, o));
    }
    out
}

/// A `MatrixTransform` in the config's YAML syntax, in the direction `dir`.
fn yaml_transform((m, o): &Matrix, dir: TransformDirection) -> String {
    let list = |values: &[f64]| {
        values
            .iter()
            .map(|v| yaml_number(*v))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let dir = match dir {
        TransformDirection::Forward => "forward",
        TransformDirection::Inverse => "inverse",
    };
    format!(
        "!<MatrixTransform> {{matrix: [{}], offset: [{}], direction: {dir}}}",
        list(m),
        list(o)
    )
}

/// The `cpu_apply` arguments of the processor of `transform`, given in YAML, with the
/// optimization `flags`.
fn yaml_args(transform: String, flags: &str) -> Value {
    let combo = Combo {
        direction: Direction::Forward,
        fast_math: true,
        format: Format::F32_RGBA,
    };
    let mut args = Spec::Yaml(transform).cpu_apply_args(&combo);
    args["optimization"] = json!(flags);
    args
}

/// The oracle's `cpu_apply` of each spec, on one pixel.
fn run(specs: Vec<Value>) -> Vec<Value> {
    let pixel: Vec<u8> = [0.5f32, 0.25, 0.125, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let calls: Vec<BatchCall<'_>> = specs
        .into_iter()
        .map(|args| BatchCall {
            cmd: "cpu_apply",
            args,
            blobs: vec![&pixel],
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")).result)
        .collect()
}

/// The data cache ID of the one Matrix op in the wheel's CPU processor cache ID.
fn matrix_cache_id(result: &Value) -> String {
    let cpu = result["cpu_cache_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no CPU processor: {result}"));
    let start = cpu
        .find("<MatrixOffsetOp ")
        .unwrap_or_else(|| panic!("no Matrix op in {cpu:?}"));
    let rest = &cpu[start + "<MatrixOffsetOp ".len()..];
    let end = rest.find(" >").expect("the op's end");
    assert!(
        !rest[end..].contains("<MatrixOffsetOp"),
        "more than one Matrix op in {cpu:?}"
    );
    rest[..end].to_string()
}

/// Compares the port's data, or its error, with the wheel's one Matrix op, or its exception,
/// and adds a failure described by `what` when they differ.
fn check(
    what: impl Fn() -> String,
    port: Result<MatrixOpData>,
    result: &Value,
    failures: &mut Vec<String>,
) {
    match (result.get("exception"), port) {
        (None, Ok(data)) => {
            let port = String::from_utf8(data.get_cache_id().unwrap()).unwrap();
            let wheel = matrix_cache_id(result);
            if port != wheel {
                failures.push(format!("{}\n  wheel {wheel}\n  port  {port}", what()));
            }
        }
        (Some(exception), Err(e)) if exception["message"] == e.message() => {}
        (exception, port) => failures.push(format!(
            "{}\n  wheel {exception:?}\n  port  {:?}",
            what(),
            port.map(|_| ()).map_err(|e| e.message().to_string())
        )),
    }
}

#[test]
fn the_inverse_matches_the_wheel() {
    let matrices = matrices();
    let specs = matrices
        .iter()
        .map(|matrix| {
            json!({
                "transform": transform(matrix),
                "direction": "TRANSFORM_DIR_INVERSE",
                "optimization": "OPTIMIZATION_NONE",
            })
        })
        .collect();
    let results = run(specs);

    let mut failures = Vec::new();
    for (matrix, result) in matrices.iter().zip(&results) {
        let port = port_data(matrix, TransformDirection::Inverse).get_as_forward();
        check(|| format!("{matrix:?}"), port, result, &mut failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_singular_matrix_has_no_inverse() {
    let singular: Matrix = (
        [
            1.0, 0., 0., 0.2, 0.0, 0., 0., 0.0, 0.0, 0., 0., 0.0, 0.2, 0., 0., 1.0,
        ],
        [0.; 4],
    );
    let results = run(vec![json!({
        "transform": transform(&singular),
        "direction": "TRANSFORM_DIR_INVERSE",
        "optimization": "OPTIMIZATION_NONE",
    })]);
    let wheel = results[0]["exception"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("the wheel inverted it: {}", results[0]));
    let port = port_data(&singular, TransformDirection::Inverse)
        .get_as_forward()
        .unwrap_err();
    assert_eq!(port.message(), wheel);
}

#[test]
fn the_composition_matches_the_wheel() {
    let matrices = matrices();
    let snap_pairs = snap_pairs();
    let pairs: Vec<(&Matrix, &Matrix)> = matrices
        .iter()
        .zip(matrices.iter().cycle().skip(1))
        .chain(matrices.iter().zip(matrices.iter().rev()))
        .chain(snap_pairs.iter().map(|(a, b)| (a, b)))
        .collect();
    let specs = pairs
        .iter()
        .map(|(a, b)| {
            json!({
                "transform": {
                    "class": "GroupTransform",
                    "children": [transform(a), transform(b)],
                },
                "optimization": "OPTIMIZATION_DEFAULT",
            })
        })
        .collect();
    let results = run(specs);

    let mut failures = Vec::new();
    for ((a, b), result) in pairs.iter().zip(&results) {
        let a_data = port_data(a, TransformDirection::Forward);
        let b_data = port_data(b, TransformDirection::Forward);
        let composed = a_data.compose(&b_data).unwrap();
        // `combineWith` makes no op of an identity composition.
        assert!(
            !composed.is_no_op().unwrap(),
            "{a:?} then {b:?} is the identity"
        );
        check(
            || format!("{a:?}\n  then {b:?}"),
            Ok(composed),
            result,
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn nan_and_infinities_invert_as_in_the_wheel() {
    let matrices = special_matrices();
    let specs = matrices
        .iter()
        .map(|matrix| {
            yaml_args(
                yaml_transform(matrix, TransformDirection::Inverse),
                "OPTIMIZATION_NONE",
            )
        })
        .collect();
    let results = run(specs);

    let mut failures = Vec::new();
    for (matrix, result) in matrices.iter().zip(&results) {
        let port = port_data(matrix, TransformDirection::Inverse).get_as_forward();
        check(|| format!("{matrix:?}"), port, result, &mut failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn nan_and_infinities_compose_as_in_the_wheel() {
    use TransformDirection::{Forward, Inverse};
    let matrices = special_matrices();
    // The offsets' largest magnitude, which sets their `cleanUp` tolerance, is the ternary's
    // (MatrixOpData.cpp:713-719): after a NaN, the second matrix's offset or a product with its
    // NaN, it starts again from the next magnitude, here 0, so 1e6 + 0.05 stays (`f64::max`
    // would skip the NaN, and snap it to 1e6 for a tolerance of 0.1).
    let nan_in_the_middle: [(Matrix, Matrix); 2] = [
        (
            (diagonal([2., 1., 1., 1.]), [1e6 + 0.05, 0., 0., 0.]),
            (diagonal([1., 1., 2., 1.]), [0., f64::NAN, 0., 0.]),
        ),
        (
            (diagonal([2., 1., 1., 1.]), [1e6 + 0.05, 0., 0., 0.]),
            (diagonal([1., f64::NAN, 2., 1.]), [0.; 4]),
        ),
    ];
    // Each matrix then the next, in every pair of directions: an inverse's generated NaNs then
    // meet the other's `.nan`s in the products.
    let cases: Vec<(&Matrix, TransformDirection, &Matrix, TransformDirection)> = matrices
        .iter()
        .zip(matrices.iter().cycle().skip(1))
        .flat_map(|(a, b)| {
            [
                (a, Forward, b, Forward),
                (a, Inverse, b, Forward),
                (a, Forward, b, Inverse),
                (a, Inverse, b, Inverse),
            ]
        })
        .chain(
            nan_in_the_middle
                .iter()
                .map(|(a, b)| (a, Forward, b, Forward)),
        )
        .collect();
    let specs = cases
        .iter()
        .map(|&(a, a_dir, b, b_dir)| {
            yaml_args(
                format!(
                    "!<GroupTransform> {{children: [{}, {}]}}",
                    yaml_transform(a, a_dir),
                    yaml_transform(b, b_dir)
                ),
                "OPTIMIZATION_DEFAULT",
            )
        })
        .collect();
    let results = run(specs);

    let mut failures = Vec::new();
    for (&(a, a_dir, b, b_dir), result) in cases.iter().zip(&results) {
        // The ops are finalized, as forward ones, before the optimizer combines them.
        let port = port_data(a, a_dir).get_as_forward().and_then(|a_data| {
            let b_data = port_data(b, b_dir).get_as_forward()?;
            let composed = a_data.compose(&b_data)?;
            // `combineWith` makes no op of an identity composition.
            assert!(
                !composed.is_no_op().unwrap(),
                "{a:?} then {b:?} is the identity"
            );
            Ok(composed)
        });
        check(
            || format!("{a:?} {a_dir:?}\n  then {b:?} {b_dir:?}"),
            port,
            result,
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `MatrixOpData::equals` against the wheel's `MatrixTransform::equals`, which compares the
/// transforms' data with it (src/OpenColorIO/transforms/MatrixTransform.cpp, `equals`; the
/// oracle's `transform_text` pairs). The offsets compare their bits, as upstream's `memcmp`
/// (MatrixOpData.cpp:39-42 @ v2.5.2): 0 and -0 differ there, but not in the matrix, whose
/// values compare with `==`. (JSON can't carry NaN, the other case where the two differ.)
#[test]
fn equality_compares_the_offsets_bits_and_the_matrix_values() {
    let zero = [0.0; 4];
    let negative_zero_offset = [-0.0, 0.0, 0.0, 0.0];
    let mut negative_zero_matrix = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let identity = negative_zero_matrix;
    negative_zero_matrix[1] = -0.0;
    let matrices: Vec<Matrix> = vec![
        (identity, zero),
        (identity, negative_zero_offset),
        (negative_zero_matrix, zero),
        (identity, [0.5, -0.0, 0.25, 0.0]),
        (identity, [0.5, 0.0, 0.25, 0.0]),
    ];
    let pairs: Vec<[usize; 2]> = vec![[0, 1], [1, 0], [0, 2], [2, 0], [1, 1], [3, 4], [4, 3]];
    let specs: Vec<Value> = matrices.iter().map(transform).collect();
    let response = Oracle::get().call(
        "transform_text",
        json!({"transforms": specs, "pairs": pairs}),
        &[],
    );
    for (pair, result) in pairs
        .iter()
        .zip(response.result["pairs"].as_array().unwrap())
    {
        let wheel = result["equals"]
            .as_bool()
            .unwrap_or_else(|| panic!("no equals for {pair:?}: {result}"));
        let a = port_data(&matrices[pair[0]], TransformDirection::Forward);
        let b = port_data(&matrices[pair[1]], TransformDirection::Forward);
        assert_eq!(a.equals(&b), wheel, "{pair:?}");
    }
}
