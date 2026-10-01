// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CPU processor against the wheel: its cache ID, for every pair of the bit depths it
//! takes, several optimization levels, and lists of Matrix transforms. The cache ID names the
//! bit depths, the flags and the ops it renders, after the optimizer
//! (`CPUProcessor::Impl::finalize`, src/OpenColorIO/CPUProcessor.cpp:341-377 @ v2.5.2).
//!
//! The port builds the processor's ops as the wheel does: a `GroupTransform` builds each child
//! forward, `BuildMatrixOp` clones the transform's data (src/OpenColorIO/ops/matrix/
//! MatrixOp.cpp:395-404), and the processor finalizes them (src/OpenColorIO/Processor.cpp:
//! 618-641).

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

/// Held by each test: the debug-log test sets the process's logging level and function, and
/// the other tests' processors would log into it.
static LOGGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A matrix and its offsets.
type Matrix = ([f64; 16], [f64; 4]);

/// The bit depths the CPU processor takes, by name.
const BIT_DEPTHS: [(&str, BitDepth); 6] = [
    ("BIT_DEPTH_UINT8", BitDepth::Uint8),
    ("BIT_DEPTH_UINT10", BitDepth::Uint10),
    ("BIT_DEPTH_UINT12", BitDepth::Uint12),
    ("BIT_DEPTH_UINT16", BitDepth::Uint16),
    ("BIT_DEPTH_F16", BitDepth::F16),
    ("BIT_DEPTH_F32", BitDepth::F32),
];

/// Optimization levels.
const FLAGS: [(&str, OptimizationFlags); 5] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.0; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

/// Lists of transforms: none of which leave any op, one, two that combine, and a pair that
/// cancels around another (tests/cpu/ops/matrix/MatrixOp_tests.cpp:367-377 @ v2.5.2 for the
/// matrices).
fn chains() -> Vec<Vec<(Matrix, TransformDirection)>> {
    use TransformDirection::{Forward, Inverse};
    let upstream: Matrix = (
        [
            1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
        ],
        [-0.5, -0.25, 0.25, 0.0],
    );
    let scale: Matrix = (diagonal([2.0, 0.5, 4.0, 1.0]), [0.0; 4]);
    let offset: Matrix = (diagonal([1.0; 4]), [0.1, -0.2, 0.3, 0.0]);
    let identity: Matrix = (diagonal([1.0; 4]), [0.0; 4]);
    vec![
        vec![(identity, Forward)],
        vec![(scale, Forward)],
        vec![(upstream, Inverse)],
        vec![(scale, Forward), (offset, Forward)],
        vec![(upstream, Forward), (scale, Forward), (upstream, Inverse)],
        vec![(offset, Forward), (offset, Inverse)],
    ]
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

/// The bytes of a pixel of `bit_depth`, zeros.
fn pixel_bytes(bit_depth: BitDepth) -> Vec<u8> {
    let size = match bit_depth {
        BitDepth::Uint8 => 1,
        BitDepth::F32 => 4,
        _ => 2,
    };
    vec![0; 4 * size]
}

#[test]
fn the_cache_id_matches_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let chains = chains();
    let mut cases = Vec::new();
    for chain in &chains {
        for flags in &FLAGS {
            for input in &BIT_DEPTHS {
                for output in &BIT_DEPTHS {
                    cases.push((chain, flags, input, output));
                }
            }
        }
    }
    let pixels: Vec<Vec<u8>> = cases
        .iter()
        .map(|(_, _, (_, input), _)| pixel_bytes(*input))
        .collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .zip(&pixels)
        .map(
            |((chain, (flags, _), (in_name, _), (out_name, _)), pixel)| {
                let children: Vec<Value> =
                    chain.iter().map(|(m, dir)| transform(m, *dir)).collect();
                BatchCall {
                    cmd: "cpu_apply",
                    args: json!({
                        "transform": {"class": "GroupTransform", "children": children},
                        "optimization": flags,
                        "in_bitdepth": in_name,
                        "out_bitdepth": out_name,
                    }),
                    blobs: vec![pixel],
                }
            },
        )
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((chain, (name, flags), (_, input), (_, output)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = match result.get("exception") {
            Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
            None => Ok(result["cpu_cache_id"].as_str().unwrap().to_string()),
        };
        let port = (|| -> ocio_ops::exception::Result<String> {
            let mut raw = OpVec::new();
            for ((m, o), dir) in chain.iter() {
                let mut data = MatrixOpData::new();
                data.set_rgba(m);
                data.set_rgba_offsets(o);
                data.set_direction(*dir);
                data.validate()?;
                create_matrix_op(&mut raw, data, TransformDirection::Forward);
            }
            raw.finalize()?;
            let cpu = CpuProcessor::new(&raw, *input, *output, *flags)?;
            Ok(String::from_utf8(cpu.get_cache_id().to_vec()).unwrap())
        })()
        .map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!(
                "{chain:?} {name} {input:?}->{output:?}\n  wheel {wheel:?}\n  port  {port:?}"
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

/// The debug log of building the CPU processor, the optimizer's lists of ops before and after
/// with its pass counts (`OpRcPtrVec::optimize`, src/OpenColorIO/OpOptimizers.cpp:611-756 @
/// v2.5.2), against the wheel's, through the oracle's `processor_debug_log`, message for
/// message, for every list and level and a few bit depths. The test changes the process's
/// logging level and function, so it holds `LOGGING`, as the other tests do.
#[test]
fn the_debug_log_matches_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    use ocio_ops::logging::{
        LoggingFunction, get_logging_level, reset_to_default_logging_function,
        set_logging_function, set_logging_level,
    };
    use ocio_ops::open_color_types::LoggingLevel;
    use std::sync::{Arc, Mutex};

    let chains = chains();
    let bit_depths = [&BIT_DEPTHS[0], &BIT_DEPTHS[4], &BIT_DEPTHS[5]];
    let mut cases = Vec::new();
    for chain in &chains {
        for flags in &FLAGS {
            for input in bit_depths {
                cases.push((chain, flags, input));
            }
        }
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(chain, (flags, _), (in_name, _))| {
            let children: Vec<Value> = chain.iter().map(|(m, dir)| transform(m, *dir)).collect();
            BatchCall {
                cmd: "processor_debug_log",
                args: json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "optimization": flags,
                    "in_bitdepth": in_name,
                }),
                blobs: vec![],
            }
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = messages.clone();
    let function: LoggingFunction = Arc::new(move |m: &[u8]| {
        sink.lock()
            .unwrap()
            .push(String::from_utf8(m.to_vec()).unwrap());
    });
    let level = get_logging_level();
    set_logging_function(Some(function)).unwrap();
    set_logging_level(LoggingLevel::Debug);

    let mut failures = Vec::new();
    for ((chain, (name, flags), (_, input)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel: Vec<String> = result["cpu_processor"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().to_string())
            .collect();
        let mut raw = OpVec::new();
        for ((m, o), dir) in chain.iter() {
            let mut data = MatrixOpData::new();
            data.set_rgba(m);
            data.set_rgba_offsets(o);
            data.set_direction(*dir);
            create_matrix_op(&mut raw, data, TransformDirection::Forward);
        }
        raw.finalize().unwrap();
        messages.lock().unwrap().clear();
        CpuProcessor::new(&raw, *input, BitDepth::F32, *flags).unwrap();
        let port = messages.lock().unwrap().clone();
        if port != wheel {
            failures.push(format!(
                "{chain:?} {name} {input:?}\n  wheel {wheel:#?}\n  port  {port:#?}"
            ));
        }
    }

    set_logging_level(level);
    reset_to_default_logging_function();
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
