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
/// cancels around another, and identities within the tolerance (tests/cpu/ops/matrix/MatrixOp_tests.cpp:367-377 @ v2.5.2 for the
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
    // Alpha's diagonal value within 1e-6 of 1, and just past: `MatrixOpData::hasAlpha` and
    // `isIdentity` compare it with that tolerance (MatrixOpData.cpp:534-614 @ v2.5.2), so the
    // first is an identity, which the optimizer removes, and the second isn't.
    let near_alpha: Matrix = (diagonal([1.0, 1.0, 1.0, 1.0 + 5e-7]), [0.0; 4]);
    let past_alpha: Matrix = (diagonal([1.0, 1.0, 1.0, 1.0 + 2e-6]), [0.0; 4]);
    let near_red: Matrix = (diagonal([1.0 - 9e-7, 1.0, 1.0, 1.0]), [0.0; 4]);
    vec![
        vec![(identity, Forward)],
        vec![(near_alpha, Forward)],
        vec![(past_alpha, Forward)],
        vec![(near_red, Forward), (near_alpha, Forward)],
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

/// The ops of `chain`, as the wheel's processor of a `GroupTransform` of them has them.
fn raw_ops(chain: &[(Matrix, TransformDirection)]) -> OpVec {
    let mut raw = OpVec::new();
    for ((m, o), dir) in chain {
        let mut data = MatrixOpData::new();
        data.set_rgba(m);
        data.set_rgba_offsets(o);
        data.set_direction(*dir);
        create_matrix_op(&mut raw, data, TransformDirection::Forward);
    }
    raw.finalize().unwrap();
    raw
}

/// The testkit's bit depth of `depth`.
fn kit_depth(depth: BitDepth) -> ocio_testkit::battery::BitDepth {
    use ocio_testkit::battery::BitDepth as D;
    match depth {
        BitDepth::Uint8 => D::Uint8,
        BitDepth::Uint10 => D::Uint10,
        BitDepth::Uint12 => D::Uint12,
        BitDepth::Uint16 => D::Uint16,
        BitDepth::F16 => D::F16,
        BitDepth::F32 => D::F32,
        other => panic!("no image of {other:?}"),
    }
}

/// The CPU processor's queries, `isNoOp`, `isIdentity` and `hasChannelCrosstalk`, and its bit
/// depths, against the wheel's `CPUProcessor` getters (the oracle's `image_apply` reports them
/// once it has built the processor), for every list, level and pair of bit depths.
#[test]
fn the_queries_match_the_wheel() {
    use ocio_testkit::image::{Buffer, Channels, Data, Packed, Request, Stride};

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
    let requests: Vec<Request> = cases
        .iter()
        .map(
            |(chain, (flags, _), (in_name, input), (out_name, output))| {
                let children: Vec<Value> =
                    chain.iter().map(|(m, dir)| transform(m, *dir)).collect();
                let mut request = Request::new(json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "optimization": flags,
                    "in_bitdepth": in_name,
                    "out_bitdepth": out_name,
                }));
                for depth in [*input, *output] {
                    let buffer = request.buffer(Buffer::Bytes(vec![0; 16]));
                    request.image(
                        Packed::new(Data::at(buffer, 0), 1, 1, Channels::Count(4))
                            .layout(kit_depth(depth), [Stride::Auto; 3]),
                    );
                }
                request.apply = vec![0, 1];
                request
            },
        )
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for (((chain, (name, flags), (_, input), (_, output)), request), response) in
        cases.iter().zip(&requests).zip(responses)
    {
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{e}")));
        assert!(reply.raised().is_none(), "{:?}", reply.raised());
        let getters = &reply.result["cpu_processor"];
        let wheel = json!({
            "isNoOp": getters["isNoOp"],
            "isIdentity": getters["isIdentity"],
            "hasChannelCrosstalk": getters["hasChannelCrosstalk"],
            "getInputBitDepth": getters["getInputBitDepth"],
            "getOutputBitDepth": getters["getOutputBitDepth"],
        });
        let cpu = CpuProcessor::new(&raw_ops(chain), *input, *output, *flags).unwrap();
        let bit_depth_name = |depth: BitDepth| {
            BIT_DEPTHS
                .iter()
                .find(|(_, d)| *d == depth)
                .map(|(n, _)| *n)
                .unwrap()
        };
        let port = json!({
            "isNoOp": cpu.is_no_op(),
            "isIdentity": cpu.is_identity(),
            "hasChannelCrosstalk": cpu.has_channel_crosstalk(),
            "getInputBitDepth": bit_depth_name(cpu.get_input_bit_depth()),
            "getOutputBitDepth": bit_depth_name(cpu.get_output_bit_depth()),
        });
        if port != wheel {
            failures.push(format!(
                "{chain:?} {name} {input:?}->{output:?}\n  wheel {wheel}\n  port  {port}"
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

/// `n` scale matrices: each pass of the optimizer combines one pair of them.
fn scales(n: usize) -> Vec<(Matrix, TransformDirection)> {
    let scale: Matrix = (diagonal([2.0, 0.5, 4.0, 1.0]), [0.0; 4]);
    vec![(scale, TransformDirection::Forward); n]
}

/// The optimizer's cap of 80 passes (`OpRcPtrVec::optimize`, src/OpenColorIO/OpOptimizers.cpp:
/// 628-735 @ v2.5.2): around the cap, the cache ID shows how many ops a long list of matrices
/// keeps, and the debug log when it says the cap was reached, at every count. Default
/// optimization, F32.
#[test]
fn the_pass_cap_matches_the_wheel() {
    use ocio_ops::logging::{
        LoggingFunction, get_logging_level, reset_to_default_logging_function,
        set_logging_function, set_logging_level,
    };
    use ocio_ops::open_color_types::LoggingLevel;
    use std::sync::{Arc, Mutex};

    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let counts = [79, 80, 81, 82, 83, 84, 90];
    let calls: Vec<BatchCall<'_>> = counts
        .iter()
        .map(|&n| {
            let children: Vec<Value> = scales(n).iter().map(|(m, d)| transform(m, *d)).collect();
            BatchCall {
                cmd: "processor_debug_log",
                args: json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "optimization": "OPTIMIZATION_DEFAULT",
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
    for (n, result) in counts.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel_log: Vec<String> = result["cpu_processor"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().to_string())
            .collect();
        let wheel_id = result["cpu_cache_id"].as_str().unwrap().to_string();
        messages.lock().unwrap().clear();
        let cpu = CpuProcessor::new(
            &raw_ops(&scales(*n)),
            BitDepth::F32,
            BitDepth::F32,
            OptimizationFlags::DEFAULT,
        )
        .unwrap();
        let port_log = messages.lock().unwrap().clone();
        let port_id = String::from_utf8(cpu.get_cache_id().to_vec()).unwrap();
        if port_id != wheel_id {
            failures.push(format!(
                "{n} ops: cache IDs\n  wheel {wheel_id}\n  port  {port_id}"
            ));
        }
        if port_log != wheel_log {
            failures.push(format!(
                "{n} ops: debug logs\n  wheel {wheel_log:#?}\n  port  {port_log:#?}"
            ));
        }
    }

    set_logging_level(level);
    reset_to_default_logging_function();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A CLF file with one 3x3 Matrix, doubling RGB: the CLF reader keeps it 3x3, and validating the
/// op makes it 4x4 (`MatrixArray::validate`, src/OpenColorIO/ops/matrix/MatrixOpData.cpp:
/// 413-436 @ v2.5.2).
const CLF_3X3: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProcessList compCLFversion="3" id="m">
  <Matrix inBitDepth="32f" outBitDepth="32f">
    <Array dim="3 3">
2 0 0
0 2 0
0 0 2
    </Array>
  </Matrix>
</ProcessList>
"#;

/// A config with a look, `look0`, that doubles RGB.
const LOOK_CONFIG: &str = "ocio_profile_version: 2\n\
roles: {default: raw}\n\
looks:\n  \
- !<Look> {name: look0, process_space: raw, transform: !<MatrixTransform> \
{matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}}\n\
colorspaces:\n  \
- !<ColorSpace> {name: raw, isdata: true}\n";

/// The no-op types in the engine against the wheel: the ops a `FileTransform` and a
/// `LookTransform` build start with a `FileNoOp` and a `LookNoOp`, which the optimizer removes
/// first at every level, and whose lines in the debug log show their `getInfo` and cache IDs
/// (`SerializeOpVec`, src/OpenColorIO/Op.cpp:473-489 @ v2.5.2; the file no-op's cache ID is
/// empty, docs/improvements.md I-40). The CPU processor stage's messages and the cache ID must
/// be the wheel's.
///
/// The port builds the ops as the wheel's builders do (the builders themselves come with the
/// transforms, WP 1.8, and the file readers): a look `CreateLookNoOp(lookName)`, then its
/// transform, in the look's direction (`-look0` for the inverse; src/OpenColorIO/transforms/
/// LookTransform.cpp:243-273); between data color spaces nothing else. A file
/// `CreateFileNoOp(path)`, then the file's ops through `CreateOpVecFromOpData`; the CLF reader
/// gives the 3x3 Matrix as it is. The processor then finalizes them.
#[test]
fn the_no_op_types_match_the_wheel() {
    use ocio_ops::logging::{
        LoggingFunction, get_logging_level, reset_to_default_logging_function,
        set_logging_function, set_logging_level,
    };
    use ocio_ops::open_color_types::LoggingLevel;
    use ocio_ops::ops::noop::{create_file_no_op, create_look_no_op};
    use std::sync::{Arc, Mutex};

    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("ocio-rs-no-ops-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clf = dir.join("m.clf");
    std::fs::write(&clf, CLF_3X3).unwrap();
    let clf_path = clf.to_str().unwrap().to_string();

    let look = |looks: &str| {
        json!({"class": "LookTransform",
        "args": {"src": "raw", "dst": "raw", "looks": looks}})
    };
    let file = json!({"class": "FileTransform", "args": {"src": clf_path}});
    let doubling = || {
        let mut data = MatrixOpData::new();
        data.set_rgba(&diagonal([2.0, 2.0, 2.0, 1.0]));
        data
    };
    let port_look = |ops: &mut OpVec, inverse: bool| {
        if inverse {
            create_look_no_op(ops, b"-look0");
            create_matrix_op(ops, doubling(), TransformDirection::Inverse);
        } else {
            create_look_no_op(ops, b"look0");
            create_matrix_op(ops, doubling(), TransformDirection::Forward);
        }
    };
    let port_file = |ops: &mut OpVec| {
        create_file_no_op(ops, clf_path.as_bytes());
        let mut data = MatrixOpData::new();
        data.get_array_mut().resize(3, 3);
        data.get_array_mut()
            .get_values_mut()
            .copy_from_slice(&[2., 0., 0., 0., 2., 0., 0., 0., 2.]);
        data.set_file_input_bit_depth(BitDepth::F32);
        data.set_file_output_bit_depth(BitDepth::F32);
        create_matrix_op(ops, data, TransformDirection::Forward);
    };
    type Build<'a> = Box<dyn Fn(&mut OpVec) + 'a>;
    let lists: Vec<(Value, Build<'_>)> = vec![
        (
            look("look0"),
            Box::new(|ops: &mut OpVec| port_look(ops, false)),
        ),
        (
            look("-look0"),
            Box::new(|ops: &mut OpVec| port_look(ops, true)),
        ),
        (
            json!({"class": "GroupTransform", "children": [file, look("look0")]}),
            Box::new(|ops: &mut OpVec| {
                port_file(ops);
                port_look(ops, false);
            }),
        ),
    ];
    let mut cases = Vec::new();
    for (k, _) in lists.iter().enumerate() {
        for flags in &FLAGS {
            cases.push((k, flags));
        }
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(k, (flags, _))| BatchCall {
            cmd: "processor_debug_log",
            args: json!({
                "config": {"yaml": LOOK_CONFIG},
                "transform": lists[*k].0,
                "optimization": flags,
            }),
            blobs: vec![],
        })
        .collect();
    let results = Oracle::get().batch(&calls, false);

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
    for ((k, (name, flags)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel_log: Vec<String> = result["cpu_processor"]
            .as_array()
            .unwrap_or_else(|| panic!("{result}"))
            .iter()
            .map(|m| m.as_str().unwrap().to_string())
            .collect();
        let wheel_id = result["cpu_cache_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{result}"))
            .to_string();
        let mut raw = OpVec::new();
        (lists[*k].1)(&mut raw);
        raw.finalize().unwrap();
        messages.lock().unwrap().clear();
        let cpu = CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, *flags).unwrap();
        let port_log = messages.lock().unwrap().clone();
        let port_id = String::from_utf8(cpu.get_cache_id().to_vec()).unwrap();
        if port_id != wheel_id || port_log != wheel_log {
            failures.push(format!(
                "{} {name}\n  wheel {wheel_id}\n  port  {port_id}\n  wheel {wheel_log:#?}\n  \
                 port  {port_log:#?}",
                lists[*k].0
            ));
        }
    }

    set_logging_level(level);
    reset_to_default_logging_function();
    std::fs::remove_dir_all(&dir).ok();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The bit depths a CPU processor refuses, UINT14 and UINT32, in and out, with the wheel's
/// messages: the optimizer's `IsFloatBitDepth` refuses them first where the list keeps ops
/// (`OpRcPtrVec::optimizeForBitdepth`, src/OpenColorIO/OpOptimizers.cpp:758-778 @ v2.5.2), and
/// `CreateGenericBitDepthHelper` otherwise (CPUProcessor.cpp:68-120).
#[test]
fn unsupported_bit_depths_match_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let chains = chains();
    let pairs = [
        (
            "BIT_DEPTH_UINT14",
            BitDepth::Uint14,
            "BIT_DEPTH_F32",
            BitDepth::F32,
        ),
        (
            "BIT_DEPTH_UINT32",
            BitDepth::Uint32,
            "BIT_DEPTH_F32",
            BitDepth::F32,
        ),
        (
            "BIT_DEPTH_F32",
            BitDepth::F32,
            "BIT_DEPTH_UINT14",
            BitDepth::Uint14,
        ),
        (
            "BIT_DEPTH_UINT8",
            BitDepth::Uint8,
            "BIT_DEPTH_UINT32",
            BitDepth::Uint32,
        ),
    ];
    let mut cases = Vec::new();
    for chain in &chains {
        for flags in &FLAGS {
            for pair in &pairs {
                cases.push((chain, flags, pair));
            }
        }
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(chain, (flags, _), (in_name, _, out_name, _))| {
            let children: Vec<Value> = chain.iter().map(|(m, dir)| transform(m, *dir)).collect();
            BatchCall {
                cmd: "processor_debug_log",
                args: json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "optimization": flags,
                    "in_bitdepth": in_name,
                    "out_bitdepth": out_name,
                }),
                blobs: vec![],
            }
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((chain, (name, flags), (_, input, _, output)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = match result.get("exception") {
            Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
            None => Ok(result["cpu_cache_id"].as_str().unwrap().to_string()),
        };
        let port = CpuProcessor::new(&raw_ops(chain), *input, *output, *flags)
            .map(|cpu| String::from_utf8(cpu.get_cache_id().to_vec()).unwrap())
            .map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!(
                "{chain:?} {name} {input:?}->{output:?}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
