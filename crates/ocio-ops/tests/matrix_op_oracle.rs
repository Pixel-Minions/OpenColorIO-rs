// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op (`MatrixOffsetOp`) against the wheel: its cache ID, after `finalize`, and the
//! ops its `combineWith` leaves, through the wheel's CPU processor cache ID, whose end is its
//! ops' cache IDs: `" ops: "` then `OpRcPtrVec::getCacheID()`, each op's after a space
//! (src/OpenColorIO/CPUProcessor.cpp:367-372, Op.cpp:448-465 @ v2.5.2).
//!
//! The port builds the ops as the wheel does (each case cites it):
//! - `BuildMatrixOp` clones the transform's data and calls `CreateMatrixOp` with the
//!   processor's direction (src/OpenColorIO/ops/matrix/MatrixOp.cpp:395-404); a
//!   `GroupTransform` builds each child forward;
//! - the CPU processor finalizes the ops, then optimizes them (CPUProcessor.cpp:311-338):
//!   without optimization the op stays as it is; with the default optimization, `RemoveNoOps`
//!   removes the no-ops and `CombineOps` combines a pair of Matrix ops with `combineWith`
//!   (src/OpenColorIO/OpOptimizers.cpp:113-130, 678-700), and nothing else applies to Matrix
//!   ops at F32;
//! - when no op is left, the CPU processor adds `CreateIdentityMatrixOp`'s
//!   (CPUProcessor.cpp:327-332).

use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::{create_identity_matrix_op, create_matrix_op};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// A matrix and its offsets.
type Matrix = ([f64; 16], [f64; 4]);

/// The data of a `MatrixTransform` with `matrix` in the direction `dir`.
fn data((m, o): &Matrix, dir: TransformDirection) -> MatrixOpData {
    let mut data = MatrixOpData::new();
    data.set_rgba(m);
    data.set_rgba_offsets(o);
    data.set_direction(dir);
    data
}

/// The transform spec of a `MatrixTransform` in the direction `dir`.
fn transform((m, o): &Matrix, dir: TransformDirection) -> Value {
    let dir = match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    };
    json!({
        "class": "MatrixTransform",
        "args": {"matrix": m.to_vec(), "offset": o.to_vec(), "direction": {"enum": dir}},
    })
}

/// Matrices: upstream's (tests/cpu/ops/matrix/MatrixOp_tests.cpp:367-377 @ v2.5.2), a scale,
/// a permutation, the identity, one within 1e-6 of it, and random ones.
fn matrices() -> Vec<Matrix> {
    let mut out: Vec<Matrix> = vec![
        (
            [
                1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
            ],
            [-0.5, -0.25, 0.25, 0.0],
        ),
        (
            [
                1.1, -0.1, -0.1, 0.0, 0.1, 0.9, -0.2, 0.0, 0.05, 0.0, 1.1, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [-0.2, -0.1, -0.1, -0.2],
        ),
        (
            [
                2.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.0; 4],
        ),
        (
            [
                0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.0; 4],
        ),
        (
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            [0.0; 4],
        ),
        (
            [
                1.0 + 5e-7,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0 - 5e-7,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
            [0.0; 4],
        ),
    ];
    let mut rng = Rng::new(0x3a7_0f5e);
    for _ in 0..10 {
        let mut value = || ((rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64) * 4.0 - 2.0;
        let mut m = [0.0; 16];
        for v in &mut m {
            *v = value();
        }
        let mut o = [0.0; 4];
        for v in &mut o {
            *v = value();
        }
        out.push((m, o));
    }
    out
}

/// The wheel's CPU processor cache ID of each call's processor, from `" ops: "` on.
fn ops_cache_ids(calls: Vec<Value>) -> Vec<Result<String, String>> {
    let pixel: Vec<u8> = [0.5f32, 0.25, 0.125, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let calls: Vec<BatchCall<'_>> = calls
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
        .map(|r| {
            let result = r.unwrap_or_else(|e| panic!("{e}")).result;
            if let Some(exception) = result.get("exception") {
                return Err(exception["message"].as_str().unwrap().to_string());
            }
            let cpu = result["cpu_cache_id"].as_str().unwrap();
            let (_, ops) = cpu
                .split_once(" ops: ")
                .unwrap_or_else(|| panic!("no ops in {cpu:?}"));
            Ok(ops.to_string())
        })
        .collect()
}

#[test]
fn the_op_cache_id_matches_the_wheel() {
    let matrices = matrices();
    let mut cases = Vec::new();
    for matrix in &matrices {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            cases.push((matrix, dir));
        }
    }
    let calls = cases
        .iter()
        .map(|(matrix, dir)| {
            let dir = match dir {
                TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
                TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
            };
            json!({
                "transform": transform(matrix, TransformDirection::Forward),
                "direction": dir,
                "optimization": "OPTIMIZATION_NONE",
            })
        })
        .collect();
    let wheel = ops_cache_ids(calls);

    let mut failures = Vec::new();
    for ((matrix, dir), wheel) in cases.iter().zip(&wheel) {
        // BuildMatrixOp: the transform's data, in the processor's direction; then finalize.
        let mut ops = OpVec::new();
        create_matrix_op(&mut ops, data(matrix, TransformDirection::Forward), *dir);
        let port = ops
            .finalize()
            .map(|()| String::from_utf8(ops.get_cache_id().unwrap()).unwrap())
            .map_err(|e| e.message().to_string());
        if port != *wheel {
            failures.push(format!(
                "{matrix:?} {dir:?}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn combining_matches_the_wheel() {
    use TransformDirection::{Forward, Inverse};
    let matrices = matrices();
    let mut cases = Vec::new();
    for (i, a) in matrices.iter().enumerate() {
        for b in [&matrices[(i + 1) % matrices.len()], a] {
            for dirs in [
                (Forward, Forward),
                (Forward, Inverse),
                (Inverse, Forward),
                (Inverse, Inverse),
            ] {
                cases.push((a, b, dirs));
            }
        }
    }
    let calls = cases
        .iter()
        .map(|(a, b, (a_dir, b_dir))| {
            json!({
                "transform": {
                    "class": "GroupTransform",
                    "children": [transform(a, *a_dir), transform(b, *b_dir)],
                },
                "optimization": "OPTIMIZATION_DEFAULT",
            })
        })
        .collect();
    let wheel = ops_cache_ids(calls);

    let mut failures = Vec::new();
    for ((a, b, (a_dir, b_dir)), wheel) in cases.iter().zip(&wheel) {
        let port = (|| -> ocio_ops::exception::Result<String> {
            // A group builds each child forward; the ops hold the transforms' directions.
            let mut ops = OpVec::new();
            create_matrix_op(&mut ops, data(a, *a_dir), Forward);
            create_matrix_op(&mut ops, data(b, *b_dir), Forward);
            ops.validate()?;
            ops.finalize()?;
            // RemoveNoOps, then CombineOps on the pair.
            let mut kept = OpVec::new();
            for op in ops.iter() {
                if !op.is_no_op().unwrap() {
                    kept.push_back(op.clone());
                }
            }
            let mut result = OpVec::new();
            if kept.len() == 2 && kept[0].can_combine_with(&kept[1])? {
                kept[0].combine_with(&mut result, &kept[1])?;
            } else {
                result = kept;
            }
            // The CPU processor's op when none is left.
            if result.is_empty() {
                create_identity_matrix_op(&mut result);
            }
            Ok(String::from_utf8(result.get_cache_id().unwrap()).unwrap())
        })()
        .map_err(|e| e.message().to_string());
        if port != *wheel {
            failures.push(format!(
                "{a:?} {a_dir:?}\n  then {b:?} {b_dir:?}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
