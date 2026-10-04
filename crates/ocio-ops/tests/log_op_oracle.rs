// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log op in the CPU engine against the wheel: lists of Log transforms (`LogTransform`,
//! `LogAffineTransform`, `LogCameraTransform`), with a `RangeTransform` or a `MatrixTransform`
//! next to them, at several optimization levels, through the CPU processor's cache ID, which
//! names the ops the processor renders after the optimizer (`CPUProcessor::Impl::finalize`,
//! src/OpenColorIO/CPUProcessor.cpp:341-377 @ v2.5.2), and through the processed pixels
//! (`image_apply`).
//!
//! Each Log op's cache ID is `<LogOp ` + its data's + `>` (`LogOp::getCacheID`,
//! src/OpenColorIO/ops/log/LogOp.cpp:94-103; `LogOpData::getCacheID`,
//! src/OpenColorIO/ops/log/LogOpData.cpp:299-325). A Log op never combines with another, but
//! the optimizer replaces a pair of inverse logs (`LogOpData::isInverse`: equal channels, the
//! same parameters and base) with the first one's identity replacement
//! (`RemoveInverseOps`, src/OpenColorIO/OpOptimizers.cpp:249-276): a Range op from 0, or from
//! `-linOffset / linSlope`, for a forward plain or affine log; an identity Matrix op, which the
//! optimizer then removes, otherwise (`LogOpData::getIdentityReplacement`,
//! LogOpData.cpp:237-292). The renderers take the fast `log2` and `exp2` approximations with
//! the default flags, and the math library without optimization.
//!
//! The port builds the processor's ops as the wheel does: a `GroupTransform` builds each child
//! forward; `BuildLogOp` validates the data and creates a Log op with a copy of it
//! (LogOp.cpp:185-216), `BuildRangeOp` and `BuildMatrixOp` likewise; the processor finalizes
//! them (src/OpenColorIO/Processor.cpp:623-641). An error compares as `<stage>: <message>`,
//! with the stages of the oracle's `cpu_apply` and the transforms' validation prefixes
//! ([`port_processor`]).
//!
//! Integer and half inputs are compared without optimization only: with the default and the
//! full flags, the optimizer bakes a separable prefix holding a Log op into a Lut1D
//! (`OptimizeSeparablePrefix`, OpOptimizers.cpp:553-596), which waits for the Lut1D op (the
//! p1-optimizer card). Until then the port refuses those processors, and
//! `integer_inputs_wait_for_the_lut1d_bake` pins the refusal where the wheel bakes.

mod common;

use common::image::{add_image, depth_name, port_depth, port_image};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::Bytes;
use std::sync::Arc;

use ocio_ops::format_metadata::METADATA_ID;
use ocio_ops::op::{OpVec, create_op_vec_from_op_data};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::log::log_op::create_log_op;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::noop::no_ops::create_file_no_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Request};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// The four affine parameters: `[logSideSlope, logSideOffset, linSideSlope, linSideOffset]`.
type Affine = [[f64; 3]; 4];

/// A transform of the lists.
#[derive(Debug, Clone)]
enum T {
    /// A `LogTransform` of this base.
    Log(f64, TransformDirection),
    /// A `LogAffineTransform`: the base and the four parameters.
    Affine(f64, Affine, TransformDirection),
    /// A `LogCameraTransform`: the base, the four parameters, the break and maybe the linear
    /// slope.
    Camera(f64, Affine, [f64; 3], Option<[f64; 3]>, TransformDirection),
    /// A clamping `RangeTransform` from 0.25 to 2 on both sides.
    Range,
    /// A `MatrixTransform` scaling RGB by 2 with an offset of 0.1.
    Matrix,
}

const SETTERS: [(&str, LogAffineParameter); 4] = [
    ("setLogSideSlopeValue", LogAffineParameter::LogSideSlope),
    ("setLogSideOffsetValue", LogAffineParameter::LogSideOffset),
    ("setLinSideSlopeValue", LogAffineParameter::LinSideSlope),
    ("setLinSideOffsetValue", LogAffineParameter::LinSideOffset),
];

/// The matrix of [`T::Matrix`].
const SCALE: [f64; 16] = [
    2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 1.,
];
/// The offsets of [`T::Matrix`].
const OFFSET: [f64; 4] = [0.1, 0.1, 0.1, 0.];
/// The bounds of [`T::Range`].
const RANGE: [f64; 4] = [0.25, 2.0, 0.25, 2.0];

fn dir_enum(dir: TransformDirection) -> Value {
    match dir {
        F => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
        I => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
    }
}

/// The spec of a transform: built empty, then set, so that the binding's constructors don't
/// validate.
fn transform(t: &T) -> Value {
    let log = |class: &str, args: Value, base: f64, params: Option<&Affine>, dir| {
        let mut calls = vec![json!(["setBase", base])];
        if let Some(params) = params {
            for ((setter, _), values) in SETTERS.iter().zip(params) {
                calls.push(json!([setter, values]));
            }
        }
        (class.to_string(), args, calls, dir)
    };
    let (class, args, mut calls, dir) = match t {
        T::Log(base, dir) => log("LogTransform", json!({}), *base, None, *dir),
        T::Affine(base, p, dir) => log("LogAffineTransform", json!({}), *base, Some(p), *dir),
        T::Camera(base, p, brk, _, dir) => log(
            "LogCameraTransform",
            json!({"linSideBreak": brk}),
            *base,
            Some(p),
            *dir,
        ),
        T::Range => {
            return json!({"class": "RangeTransform", "args": {
                "minInValue": RANGE[0], "maxInValue": RANGE[1],
                "minOutValue": RANGE[2], "maxOutValue": RANGE[3],
            }});
        }
        T::Matrix => {
            return json!({"class": "MatrixTransform",
                "args": {"matrix": SCALE.to_vec(), "offset": OFFSET.to_vec()}});
        }
    };
    if let T::Camera(_, _, _, Some(slope), _) = t {
        calls.push(json!(["setLinearSlopeValue", slope]));
    }
    calls.push(json!(["setDirection", dir_enum(dir)]));
    json!({"class": class, "args": args, "calls": calls})
}

/// The port's data of a Log transform, as the transform builds it: `m_data(2.0f,
/// TRANSFORM_DIR_FORWARD)`, the break for a camera log, then the setters.
fn port_log_data(t: &T) -> LogOpData {
    let mut data = LogOpData::new(f64::from(2.0f32), F);
    let (base, params, dir) = match t {
        T::Log(base, dir) => (*base, None, *dir),
        T::Affine(base, p, dir) => (*base, Some(p), *dir),
        T::Camera(base, p, brk, _, dir) => {
            data.set_value(LogAffineParameter::LinSideBreak, brk)
                .unwrap();
            (*base, Some(p), *dir)
        }
        T::Range | T::Matrix => unreachable!("a log"),
    };
    data.set_base(base);
    if let Some(params) = params {
        for ((_, param), values) in SETTERS.iter().zip(params) {
            data.set_value(*param, values).unwrap();
        }
    }
    if let T::Camera(_, _, _, Some(slope), _) = t {
        data.set_value(LogAffineParameter::LinearSlope, slope)
            .unwrap();
    }
    data.set_direction(dir);
    data
}

/// The processor of `chain`, as `cpu_apply` and `image_apply` take it.
fn processor(chain: &[T], flags: &str, input: Depth, output: Depth) -> Value {
    let children: Vec<Value> = chain.iter().map(transform).collect();
    json!({
        "transform": {"class": "GroupTransform", "children": children},
        "optimization": flags,
        "in_bitdepth": depth_name(port_depth(input)),
        "out_bitdepth": depth_name(port_depth(output)),
    })
}

/// The processor's ops of `chain`, finalized: a `GroupTransform` builds each child forward.
fn port_ops(chain: &[T]) -> Result<OpVec> {
    let mut raw = OpVec::new();
    for t in chain {
        match t {
            T::Range => {
                let data = RangeOpData::with_values(RANGE[0], RANGE[1], RANGE[2], RANGE[3])?;
                create_range_op(&mut raw, data, F)?;
            }
            T::Matrix => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&SCALE);
                data.set_rgba_offsets(&OFFSET);
                data.validate()?;
                create_matrix_op(&mut raw, data, F);
            }
            log => {
                // `BuildLogOp`: the transform's data, validated, then copied
                // (LogOp.cpp:185-216).
                let data = port_log_data(log);
                data.validate()?;
                create_log_op(&mut raw, data.try_clone()?, F)?;
            }
        }
    }
    raw.finalize()?;
    Ok(raw)
}

/// The port's CPU processor of `chain`, or the wheel's error as `<stage>: <message>`, with
/// the stages of `cpu_apply`.
///
/// `Config::getProcessor(group)` validates the group, which validates each child with its
/// prefix (`Processor::Impl::setTransform`, src/OpenColorIO/Processor.cpp:623-633 @ v2.5.2;
/// `GroupTransformImpl::validate`, src/OpenColorIO/transforms/GroupTransform.cpp:56-73), before
/// it builds any op: "LogTransform validation failed: ", "LogAffineTransform validation
/// failed: " or "LogCameraTransform validation failed: ", whose `validate` also needs the break
/// (src/OpenColorIO/transforms/LogCameraTransform.cpp:50-67). The range and the matrix of the
/// lists are valid.
fn port_processor(
    chain: &[T],
    flags: OptimizationFlags,
    input: Depth,
    output: Depth,
) -> std::result::Result<CpuProcessor, String> {
    for t in chain {
        let prefix = match t {
            T::Log(..) => "LogTransform validation failed: ",
            T::Affine(..) => "LogAffineTransform validation failed: ",
            T::Camera(..) => "LogCameraTransform validation failed: ",
            T::Range | T::Matrix => continue,
        };
        let data = port_log_data(t);
        let mut valid = data.validate();
        if valid.is_ok() && matches!(t, T::Camera(..)) && data.red_params().len() < 5 {
            valid = Err(Exception::new("LinSideBreak has to be defined."));
        }
        valid.map_err(|e| format!("processor: {prefix}{}", e.message()))?;
    }
    let raw = port_ops(chain).map_err(|e| format!("processor: {}", e.message()))?;
    CpuProcessor::new(&raw, port_depth(input), port_depth(output), flags)
        .map_err(|e| format!("cpu_processor: {}", e.message()))
}

/// The wheel's error, as [`port_processor`] gives the port's.
fn wheel_error(stage: &str, message: &str) -> String {
    format!("{stage}: {message}")
}

/// Lists of transforms: each kind of log alone in both directions, inverse pairs of each kind
/// (whose replacement is a Range op or an identity Matrix op), pairs that aren't inverses
/// (another base, unequal channels, the same direction), and logs next to a range or a matrix.
fn chains() -> Vec<Vec<T>> {
    let up: Affine = [[0.18; 3], [1.0; 3], [2.0; 3], [0.1; 3]];
    let unequal: Affine = [[0.5, 1.0, 1.5], [1.0; 3], [2.0; 3], [0.1; 3]];
    let neg_offset: Affine = [[0.25; 3], [0.5; 3], [4.0; 3], [-0.5; 3]];
    let red_same: Affine = [[0.18, 0.3, 0.18], [1.0; 3], [2.0; 3], [0.1; 3]];
    let huge_min: Affine = [[0.18; 3], [1.0; 3], [1e-30; 3], [1e300; 3]];
    let tiny_min: Affine = [[0.18; 3], [1.0; 3], [1e300; 3], [1e-300; 3]];
    let zero_offset: Affine = [[0.18; 3], [1.0; 3], [2.0; 3], [0.0; 3]];
    let negative_slope: Affine = [[0.18; 3], [1.0; 3], [-2.0; 3], [0.1; 3]];
    // -linOffset / linSlope in double, then in float, isn't the float quotient of the
    // float parameters (-1 - 2^-23, not -1).
    let double_min: Affine = [
        [0.18; 3],
        [1.0; 3],
        [1.0 - 0.5f64.powi(25); 3],
        [1.0 + 0.5f64.powi(25); 3],
    ];
    // The default parameters, but parameter `k` (`LogAffineParameter` order) set to `value`.
    let only = |k: usize, value: f64| {
        let mut p: Affine = [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]];
        p[k] = [value; 3];
        p
    };
    let brk = [0.1, 0.1, 0.1];
    let cam = |p, slope, dir| T::Camera(2.0, p, brk, slope, dir);
    vec![
        vec![T::Log(2.0, F)],
        vec![T::Log(10.0, I)],
        vec![T::Log(std::f64::consts::E, F)],
        vec![T::Affine(10.0, up, F)],
        vec![T::Affine(10.0, up, I)],
        vec![T::Affine(10.0, unequal, F)],
        vec![cam(up, None, F)],
        vec![cam(up, Some([1.5, 1.5, 1.5]), I)],
        // Inverse pairs.
        vec![T::Log(2.0, F), T::Log(2.0, I)],
        vec![T::Log(10.0, I), T::Log(10.0, F)],
        vec![T::Affine(10.0, up, F), T::Affine(10.0, up, I)],
        vec![
            T::Affine(10.0, neg_offset, F),
            T::Affine(10.0, neg_offset, I),
        ],
        vec![T::Affine(10.0, up, I), T::Affine(10.0, up, F)],
        vec![cam(up, None, F), cam(up, None, I)],
        vec![cam(up, Some([1.5; 3]), I), cam(up, Some([1.5; 3]), F)],
        // A plain log and an affine one with the default parameters: the same data.
        vec![
            T::Log(2.0, F),
            T::Affine(2.0, [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]], I),
        ],
        // Not inverses.
        vec![T::Log(2.0, F), T::Log(10.0, I)],
        vec![T::Log(2.0, F), T::Log(2.0, F)],
        vec![T::Affine(10.0, unequal, F), T::Affine(10.0, unequal, I)],
        vec![cam(up, None, F), cam(up, Some([1.5; 3]), I)],
        // With other ops.
        vec![T::Range, T::Log(2.0, F)],
        vec![T::Log(2.0, I), T::Range],
        vec![
            T::Matrix,
            T::Affine(10.0, up, F),
            T::Affine(10.0, up, I),
            T::Matrix,
        ],
        vec![T::Log(2.0, F), T::Log(2.0, I), T::Range],
        vec![
            T::Log(10.0, F),
            T::Affine(10.0, up, F),
            T::Affine(10.0, up, I),
            T::Log(10.0, I),
        ],
        // A forward plain log, then its inverse: a Range from 0.
        vec![T::Log(10.0, F), T::Log(10.0, I)],
        // Equal red channels, unequal others: not inverses, either way.
        vec![T::Affine(10.0, up, F), T::Affine(10.0, red_same, I)],
        vec![T::Affine(10.0, red_same, I), T::Affine(10.0, up, F)],
        // Inverse pairs whose Range minimum, -linOffset / linSlope, is extreme: -Inf, -0 by
        // underflow, -0 from a zero offset, and positive from a negative slope.
        vec![T::Affine(10.0, huge_min, F), T::Affine(10.0, huge_min, I)],
        vec![T::Affine(10.0, tiny_min, F), T::Affine(10.0, tiny_min, I)],
        vec![
            T::Affine(10.0, double_min, F),
            T::Affine(10.0, double_min, I),
        ],
        vec![
            T::Affine(10.0, zero_offset, F),
            T::Affine(10.0, zero_offset, I),
        ],
        vec![
            T::Affine(10.0, negative_slope, F),
            T::Affine(10.0, negative_slope, I),
        ],
        // A base-2 or base-10 affine log with one parameter off its default is no plain log.
        vec![T::Affine(2.0, only(1, 0.5), F)],
        vec![T::Affine(10.0, only(1, -0.25), I)],
        vec![
            T::Affine(10.0, only(1, 0.5), F),
            T::Affine(10.0, only(1, 0.5), I),
        ],
        vec![
            T::Affine(2.0, only(3, 0.5), F),
            T::Affine(2.0, only(3, 0.5), I),
        ],
        vec![T::Affine(10.0, only(0, 0.5), F)],
        vec![T::Affine(2.0, only(2, 3.0), I)],
        // Refused by the group's validation, with each transform's prefix.
        vec![T::Range, T::Affine(10.0, only(2, 0.0), F)],
        vec![T::Log(1.0, I), T::Matrix],
        vec![cam(only(0, 0.0), None, I)],
    ]
}

/// The optimization levels.
const FLAGS: [(&str, OptimizationFlags); 3] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

/// The bit depths, in and out: F32 in at every level (module docs).
const DEPTHS: [(Depth, Depth); 3] = [
    (Depth::F32, Depth::F32),
    (Depth::F32, Depth::Uint16),
    (Depth::F32, Depth::F16),
];

/// The bit depths with integer and half inputs, compared without optimization (module docs).
const DEPTHS_NONE: [(Depth, Depth); 3] = [
    (Depth::Uint8, Depth::F32),
    (Depth::Uint16, Depth::Uint16),
    (Depth::F16, Depth::F16),
];

/// A case: a list of transforms, an index into [`FLAGS`], and the bit depths.
type Case = (Vec<T>, usize, (Depth, Depth));

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for chain in chains() {
        for flags in 0..FLAGS.len() {
            for depths in DEPTHS {
                cases.push((chain.clone(), flags, depths));
            }
        }
        for depths in DEPTHS_NONE {
            cases.push((chain.clone(), 0, depths));
        }
    }
    cases
}

/// With the default flags, a processor from an integer or half input bakes its separable
/// prefix into a Lut1D in the wheel; the port refuses it until the Lut1D op is ported (the
/// p1-optimizer card). When it is, this test fails: compare these cases like the others.
#[test]
fn integer_inputs_wait_for_the_lut1d_bake() {
    let chain = vec![T::Log(2.0, F)];
    for input in [Depth::Uint8, Depth::Uint16, Depth::F16] {
        let pixel = vec![0u8; 8];
        let call = BatchCall {
            cmd: "cpu_apply",
            args: processor(&chain, "OPTIMIZATION_DEFAULT", input, Depth::F32),
            blobs: vec![&pixel],
        };
        let result = Oracle::get().batch(&[call], true).remove(0).unwrap().result;
        assert!(result.get("exception").is_none(), "{input:?}: {result}");
        let port = port_processor(&chain, OptimizationFlags::DEFAULT, input, Depth::F32);
        let message = port.err().unwrap_or_default();
        assert!(
            message.starts_with("cpu_processor: ") && message.contains("needs the Lut1D op"),
            "{input:?}: {message:?}"
        );
    }
}

#[track_caller]
fn assert_none(failures: Vec<String>, total: usize) {
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}

#[test]
fn the_cache_id_matches_the_wheel() {
    let cases = cases();
    let pixel: Vec<u8> = [0.5f32, 0.25, 0.125, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(chain, flags, (input, output))| BatchCall {
            cmd: "cpu_apply",
            args: processor(chain, FLAGS[*flags].0, *input, *output),
            blobs: vec![&pixel],
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    let mut replaced = 0;
    let mut refused = 0;
    for ((chain, flags, (input, output)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = match result.get("exception") {
            Some(exception) => Err(wheel_error(
                result["stage"].as_str().unwrap(),
                exception["message"].as_str().unwrap(),
            )),
            None => Ok(result["cpu_cache_id"].as_str().unwrap().to_string()),
        };
        refused += usize::from(wheel.as_ref().is_err_and(|e| {
            e.starts_with("processor: ") && e.contains("Transform validation failed: ")
        }));
        // Some inverse pairs become a Range op.
        replaced += usize::from(
            chain.len() == 2
                && wheel
                    .as_ref()
                    .is_ok_and(|id| id.contains("<RangeOp ") && !id.contains("<LogOp ")),
        );
        let port = port_processor(chain, FLAGS[*flags].1, *input, *output)
            .map(|cpu| String::from_utf8(cpu.get_cache_id().to_vec()).unwrap());
        if port != wheel {
            failures.push(format!(
                "{chain:?} {} {input:?}->{output:?}\n  wheel {wheel:?}\n  port  {port:?}",
                FLAGS[*flags].0
            ));
        }
    }
    assert_none(failures, cases.len());
    assert!(replaced > 0);
    // The refused lists compare their stage and prefix.
    assert!(refused > 0);
}

#[test]
fn the_pixels_match_the_wheel() {
    let cases = cases();
    let shape = common::image::PACKED_SHAPES[0];
    let requests: Vec<Request> = cases
        .iter()
        .enumerate()
        .map(|(k, (chain, flags, (input, output)))| {
            let mut request = Request::new(processor(chain, FLAGS[*flags].0, *input, *output));
            add_image(&mut request, shape, *input, (37, 2), Some(k as u64));
            add_image(&mut request, shape, *output, (37, 2), None);
            request.apply = vec![0, 1];
            request
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for (((chain, flags, (input, output)), request), response) in
        cases.iter().zip(&requests).zip(responses)
    {
        let what = format!("{chain:?} {} {input:?}->{output:?}", FLAGS[*flags].0);
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));
        let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
        let port = (|| -> std::result::Result<(), String> {
            let cpu = port_processor(chain, FLAGS[*flags].1, *input, *output)?;
            let at = |stage: &'static str| move |e: Exception| wheel_error(stage, e.message());
            let (src_buffers, dst_buffers) = buffers.split_at_mut(1);
            let src = port_image(&request.images[0], |_| Bytes(&src_buffers[0][..]))
                .map_err(at("image"))?;
            let mut slot = Some(&mut dst_buffers[0][..]);
            let mut dst = port_image(&request.images[1], |_| {
                Bytes(slot.take().expect("one buffer"))
            })
            .map_err(at("image"))?;
            cpu.apply_src_dst(src.desc(), dst.desc_mut())
                .map_err(at("apply"))
        })();
        let wheel = match reply.raised() {
            Some(raised) => Err(wheel_error(&raised.stage, &raised.message)),
            None => Ok(()),
        };
        match (wheel, port) {
            (Ok(()), Ok(())) if buffers == reply.buffers => {}
            (Ok(()), Ok(())) => failures.push(format!("{what}: the pixels differ")),
            (Err(wheel), Err(port)) if wheel == port => {}
            (wheel, port) => failures.push(format!("{what}\n  wheel {wheel:?}\n  port  {port:?}")),
        }
    }
    assert_none(failures, cases.len());
}

/// A Log op's cache ID starts with its id: the CTF reader sets a `Log` element's `id` on the
/// op data's metadata (`CTFReaderOpElt::start`), and `LogOpData::getCacheID` writes it first
/// (LogOpData.cpp:299-325). The file's ops are copies of the cached file's
/// (`CreateOpVecFromOpData`, src/OpenColorIO/Op.cpp:567-573), after a `FileNoOp` that the
/// optimizer removes.
#[test]
fn a_ctf_log_id_is_in_the_cache_id() {
    let dir = std::env::temp_dir().join(format!("ocio-rs-log-op-ctf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("log10.ctf");
    std::fs::write(
        &path,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <ProcessList version=\"1.3\" id=\"a\">\n  \
         <Log inBitDepth=\"32f\" outBitDepth=\"32f\" id=\"abc\" style=\"log10\" />\n\
         </ProcessList>\n",
    )
    .unwrap();
    let path_text = path.to_str().unwrap();
    let pixel: Vec<u8> = [0.5f32, 0.25, 0.125, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let calls: Vec<BatchCall<'_>> = FLAGS
        .iter()
        .map(|(name, _)| BatchCall {
            cmd: "cpu_apply",
            args: json!({
                "transform": {"class": "FileTransform", "args": {"src": path_text}},
                "optimization": name,
            }),
            blobs: vec![&pixel],
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);
    std::fs::remove_dir_all(&dir).ok();

    for ((name, flags), result) in FLAGS.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = result["cpu_cache_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: {result}"));
        // The reader's data: `LogOpData(2.0, TRANSFORM_DIR_FORWARD)` with the id, then the
        // converted base, direction and default parameters (CTFReaderHelper.cpp:3587-3619).
        let mut data = LogOpData::new(2.0, F);
        data.get_format_metadata_mut()
            .add_attribute(Some(METADATA_ID), Some(b"abc"))
            .unwrap();
        data.set_base(10.0);
        let port = (|| -> Result<Vec<u8>> {
            let mut raw = OpVec::new();
            create_file_no_op(&mut raw, path_text.as_bytes());
            create_op_vec_from_op_data(&mut raw, &Arc::new(OpData::Log(data)), F)?;
            raw.finalize()?;
            let cpu = CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, *flags)?;
            Ok(cpu.get_cache_id().to_vec())
        })()
        .unwrap();
        assert_eq!(String::from_utf8(port).unwrap(), wheel, "{name}");
        assert!(wheel.contains("<LogOp abc forward Base 10 "), "{wheel}");
    }
}
