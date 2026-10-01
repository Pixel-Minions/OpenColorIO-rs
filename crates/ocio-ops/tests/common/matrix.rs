// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Lists of Matrix transforms, as the wheel's processors take them and as the port builds
//! their ops: a `GroupTransform` builds each child forward, `BuildMatrixOp` clones the
//! transform's data (src/OpenColorIO/ops/matrix/MatrixOp.cpp:395-404), and the processor
//! finalizes them (src/OpenColorIO/Processor.cpp:618-641 @ v2.5.2). The optimizer never bakes
//! Matrix ops into a Lut1D (`FindSeparablePrefix`, src/OpenColorIO/OpOptimizers.cpp:417-469),
//! so every processor renders with the ported ops.

use ocio_ops::Result;
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use serde_json::{Value, json};

use super::image::depth_name;

/// A matrix and its offsets.
pub(crate) type Matrix = ([f64; 16], [f64; 4]);

/// A list of Matrix transforms.
pub(crate) type Chain = Vec<(Matrix, TransformDirection)>;

/// A diagonal matrix.
pub(crate) fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.0; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// Lists of transforms: one that leaves no op (so the processor renders an identity matrix),
/// one with channel crosstalk, its inverse, and two that the optimizer combines unless it is
/// off (tests/cpu/ops/matrix/MatrixOp_tests.cpp:367-377 @ v2.5.2 for the matrix).
pub(crate) fn chains() -> Vec<Chain> {
    use TransformDirection::{Forward, Inverse};
    let upstream: Matrix = (
        [
            1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
        ],
        [-0.5, -0.25, 0.25, 0.0],
    );
    let scale: Matrix = (diagonal([2.0, 0.5, 4.0, 1.0]), [0.0; 4]);
    let offset: Matrix = (diagonal([1.0; 4]), [0.1, -0.2, 0.3, 0.0]);
    vec![
        vec![(offset, Forward), (offset, Inverse)],
        vec![(upstream, Forward)],
        vec![(upstream, Inverse)],
        vec![(scale, Forward), (offset, Forward), (upstream, Forward)],
    ]
}

/// Optimization levels: none, and the default.
pub(crate) const FLAGS: [(&str, OptimizationFlags); 2] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
];

/// The spec of a `MatrixTransform`.
pub(crate) fn transform((m, o): &Matrix, dir: TransformDirection) -> Value {
    let dir = match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    };
    json!({
        "class": "MatrixTransform",
        "args": {"matrix": m.to_vec(), "offset": o.to_vec(), "direction": {"enum": dir}},
    })
}

/// The processor of `chain`, as `image_apply` takes it.
pub(crate) fn processor(chain: &Chain, flags: &str, input: BitDepth, output: BitDepth) -> Value {
    let children: Vec<Value> = chain.iter().map(|(m, dir)| transform(m, *dir)).collect();
    json!({
        "transform": {"class": "GroupTransform", "children": children},
        "optimization": flags,
        "in_bitdepth": depth_name(input),
        "out_bitdepth": depth_name(output),
    })
}

/// The port's CPU processor of `chain`.
pub(crate) fn port_processor(
    chain: &Chain,
    flags: OptimizationFlags,
    input: BitDepth,
    output: BitDepth,
) -> Result<CpuProcessor> {
    let mut raw = OpVec::new();
    for ((m, o), dir) in chain {
        let mut data = MatrixOpData::new();
        data.set_rgba(m);
        data.set_rgba_offsets(o);
        data.set_direction(*dir);
        data.validate()?;
        create_matrix_op(&mut raw, data, TransformDirection::Forward);
    }
    raw.finalize()?;
    CpuProcessor::new(&raw, input, output, flags)
}

/// A list of one identity Matrix transform.
pub(crate) fn identity() -> Chain {
    vec![((diagonal([1.0; 4]), [0.0; 4]), TransformDirection::Forward)]
}
