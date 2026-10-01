// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The optimizer against the wheel: the ops the wheel's CPU processor keeps for lists of
//! Matrix ops, through the end of its cache ID, `" ops: "` then `OpRcPtrVec::getCacheID()`
//! (src/OpenColorIO/CPUProcessor.cpp:367-372, Op.cpp:448-465 @ v2.5.2), for every
//! optimization level and several flags alone, at several bit depths.
//!
//! The port builds the ops as the wheel does: a `GroupTransform` builds each child forward,
//! `BuildMatrixOp` clones the transform's data (src/OpenColorIO/ops/matrix/MatrixOp.cpp:
//! 395-404), and the processor finalizes them (src/OpenColorIO/Processor.cpp:618-641). The CPU
//! processor then finalizes, optimizes and optimizes for the bit depths a copy, and adds an
//! identity matrix op when none is left (`FinalizeOpsForCPU`, CPUProcessor.cpp:311-338).

use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::{create_identity_matrix_op, create_matrix_op};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// A matrix and its offsets.
type Matrix = ([f64; 16], [f64; 4]);

/// The optimization levels and some flags alone, by name.
const FLAGS: [(&str, OptimizationFlags); 11] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_IDENTITY", OptimizationFlags::IDENTITY),
    ("OPTIMIZATION_COMP_MATRIX", OptimizationFlags::COMP_MATRIX),
    ("OPTIMIZATION_SIMPLIFY_OPS", OptimizationFlags::SIMPLIFY_OPS),
    (
        "OPTIMIZATION_NO_DYNAMIC_PROPERTIES",
        OptimizationFlags::NO_DYNAMIC_PROPERTIES,
    ),
    ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
    ("OPTIMIZATION_VERY_GOOD", OptimizationFlags::VERY_GOOD),
    ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
    ("OPTIMIZATION_DRAFT", OptimizationFlags::DRAFT),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

/// The bit depths of the processors, in and out.
const BIT_DEPTHS: [(&str, BitDepth, &str, BitDepth); 4] = [
    (
        "BIT_DEPTH_F32",
        BitDepth::F32,
        "BIT_DEPTH_F32",
        BitDepth::F32,
    ),
    (
        "BIT_DEPTH_UINT8",
        BitDepth::Uint8,
        "BIT_DEPTH_UINT16",
        BitDepth::Uint16,
    ),
    (
        "BIT_DEPTH_F16",
        BitDepth::F16,
        "BIT_DEPTH_UINT10",
        BitDepth::Uint10,
    ),
    (
        "BIT_DEPTH_UINT12",
        BitDepth::Uint12,
        "BIT_DEPTH_F32",
        BitDepth::F32,
    ),
];

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.0; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// Matrices: upstream's (tests/cpu/ops/matrix/MatrixOp_tests.cpp:367-377 @ v2.5.2), scales,
/// offsets alone, the identity, one within 1e-6 of it, and random ones.
fn matrices() -> Vec<Matrix> {
    let mut out: Vec<Matrix> = vec![
        (
            [
                1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
            ],
            [-0.5, -0.25, 0.25, 0.0],
        ),
        (diagonal([2.0, 0.5, 4.0, 1.0]), [0.0; 4]),
        (diagonal([1.0; 4]), [0.1, -0.2, 0.3, 0.0]),
        (diagonal([1.0; 4]), [0.0; 4]),
        (diagonal([1.0 + 5e-7, 1.0, 1.0 - 5e-7, 1.0]), [0.0; 4]),
    ];
    let mut rng = Rng::new(0x0971_ce55);
    for _ in 0..4 {
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

/// A list of transforms: matrices with directions.
type Chain = Vec<(usize, TransformDirection)>;

/// Lists of 1 to 5 transforms: nested inverse pairs, a matrix between its inverses, and
/// random ones.
fn chains(count: usize) -> Vec<Chain> {
    use TransformDirection::{Forward, Inverse};
    let mut out: Vec<Chain> = vec![
        vec![(0, Forward)],
        vec![(0, Forward), (1, Forward), (1, Inverse), (0, Inverse)],
        vec![(3, Forward), (4, Forward), (3, Inverse)],
        vec![(1, Inverse), (0, Forward), (1, Forward)],
        vec![(2, Forward), (2, Inverse), (5, Forward)],
    ];
    let mut rng = Rng::new(0x5eed_c4a1);
    while out.len() < 5 + 40 {
        let len = 1 + (rng.next_u64() % 5) as usize;
        let chain = (0..len)
            .map(|_| {
                let index = (rng.next_u64() % count as u64) as usize;
                let dir = if rng.next_u64().is_multiple_of(2) {
                    Forward
                } else {
                    Inverse
                };
                (index, dir)
            })
            .collect();
        out.push(chain);
    }
    out
}

/// The spec of a `MatrixTransform`.
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

/// The ops the port's CPU processor keeps for `chain`, as their cache IDs.
fn port_ops(
    matrices: &[Matrix],
    chain: &Chain,
    flags: OptimizationFlags,
    in_bd: BitDepth,
    out_bd: BitDepth,
) -> Result<String, String> {
    let run = || -> ocio_ops::exception::Result<String> {
        // The processor's ops: each child built forward, then finalized.
        let mut raw = OpVec::new();
        for &(index, dir) in chain {
            let (m, o) = &matrices[index];
            let mut data = MatrixOpData::new();
            data.set_rgba(m);
            data.set_rgba_offsets(o);
            data.set_direction(dir);
            data.validate()?;
            create_matrix_op(&mut raw, data, TransformDirection::Forward);
        }
        raw.finalize()?;

        // FinalizeOpsForCPU.
        let mut ops = raw.clone();
        if !ops.is_empty() {
            ops.finalize()?;
            ops.optimize(flags)?;
            ops.optimize_for_bitdepth(in_bd, out_bd, flags)?;
        }
        if ops.is_empty() {
            create_identity_matrix_op(&mut ops);
        }
        Ok(String::from_utf8(ops.get_cache_id().unwrap()).unwrap())
    };
    run().map_err(|e| e.message().to_string())
}

#[test]
fn the_optimized_ops_match_the_wheel() {
    let matrices = matrices();
    let chains = chains(matrices.len());
    let mut cases = Vec::new();
    for chain in &chains {
        for flags in &FLAGS {
            for bit_depths in &BIT_DEPTHS {
                cases.push((chain, flags, bit_depths));
            }
        }
    }

    let pixel: Vec<u8> = vec![0; 16];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(chain, (flags, _), (in_name, _, out_name, _))| {
            let children: Vec<Value> = chain
                .iter()
                .map(|&(index, dir)| transform(&matrices[index], dir))
                .collect();
            BatchCall {
                cmd: "cpu_apply",
                args: json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "optimization": flags,
                    "in_bitdepth": in_name,
                    "out_bitdepth": out_name,
                }),
                blobs: vec![&pixel],
            }
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((chain, (name, flags), (_, in_bd, _, out_bd)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = match result.get("exception") {
            Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
            None => {
                let cpu = result["cpu_cache_id"].as_str().unwrap();
                let (_, ops) = cpu.split_once(" ops: ").expect("the ops");
                Ok(ops.to_string())
            }
        };
        let port = port_ops(&matrices, chain, *flags, *in_bd, *out_bd);
        if port != wheel {
            failures.push(format!(
                "{chain:?} {name} {in_bd:?}->{out_bd:?}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
