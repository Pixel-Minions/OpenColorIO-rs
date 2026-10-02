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
//! them (src/OpenColorIO/Processor.cpp:623-641).
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
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::log::log_op::create_log_op;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
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

/// The port's CPU processor of `chain`.
fn port_processor(
    chain: &[T],
    flags: OptimizationFlags,
    input: Depth,
    output: Depth,
) -> Result<CpuProcessor> {
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
                // (LogOp.cpp:185-216); `LogCameraTransform::validate` also needs the break.
                let data = port_log_data(log);
                data.validate()?;
                if let T::Camera(..) = log
                    && data.red_params().len() < 5
                {
                    return Err(Exception::new("LinSideBreak has to be defined."));
                }
                create_log_op(&mut raw, data, F)?;
            }
        }
    }
    raw.finalize()?;
    CpuProcessor::new(&raw, port_depth(input), port_depth(output), flags)
}

/// Lists of transforms: each kind of log alone in both directions, inverse pairs of each kind
/// (whose replacement is a Range op or an identity Matrix op), pairs that aren't inverses
/// (another base, unequal channels, the same direction), and logs next to a range or a matrix.
fn chains() -> Vec<Vec<T>> {
    let up: Affine = [[0.18; 3], [1.0; 3], [2.0; 3], [0.1; 3]];
    let unequal: Affine = [[0.5, 1.0, 1.5], [1.0; 3], [2.0; 3], [0.1; 3]];
    let neg_offset: Affine = [[0.25; 3], [0.5; 3], [4.0; 3], [-0.5; 3]];
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
        let message = port
            .err()
            .map(|e| e.message().to_string())
            .unwrap_or_default();
        assert!(
            message.contains("needs the Lut1D op"),
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
    for ((chain, flags, (input, output)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = match result.get("exception") {
            Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
            None => Ok(result["cpu_cache_id"].as_str().unwrap().to_string()),
        };
        // Some inverse pairs become a Range op.
        replaced += usize::from(
            chain.len() == 2
                && wheel
                    .as_ref()
                    .is_ok_and(|id| id.contains("<RangeOp ") && !id.contains("<LogOp ")),
        );
        let port = port_processor(chain, FLAGS[*flags].1, *input, *output)
            .map(|cpu| String::from_utf8(cpu.get_cache_id().to_vec()).unwrap())
            .map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!(
                "{chain:?} {} {input:?}->{output:?}\n  wheel {wheel:?}\n  port  {port:?}",
                FLAGS[*flags].0
            ));
        }
    }
    assert_none(failures, cases.len());
    assert!(replaced > 0);
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
        let port = (|| -> Result<()> {
            let cpu = port_processor(chain, FLAGS[*flags].1, *input, *output)?;
            let (src_buffers, dst_buffers) = buffers.split_at_mut(1);
            let src = port_image(&request.images[0], |_| Bytes(&src_buffers[0][..]))?;
            let mut slot = Some(&mut dst_buffers[0][..]);
            let mut dst = port_image(&request.images[1], |_| {
                Bytes(slot.take().expect("one buffer"))
            })?;
            cpu.apply_src_dst(src.desc(), dst.desc_mut())
        })();
        match (reply.raised(), port) {
            (None, Ok(())) if buffers == reply.buffers => {}
            (None, Ok(())) => failures.push(format!("{what}: the pixels differ")),
            (Some(raised), Err(e)) if raised.message == e.message() => {}
            (raised, port) => failures.push(format!(
                "{what}\n  wheel {:?}\n  port  {:?}",
                raised.map(|r| r.message),
                port.map_err(|e| e.message().to_string())
            )),
        }
    }
    assert_none(failures, cases.len());
}
