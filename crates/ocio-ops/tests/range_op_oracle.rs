// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Range op in the CPU engine against the wheel: lists of `RangeTransform`s (and a
//! `MatrixTransform` between them), at several optimization levels and bit depths, through the
//! CPU processor's cache ID, which names the ops the processor renders after the optimizer
//! (`CPUProcessor::Impl::finalize`, src/OpenColorIO/CPUProcessor.cpp:341-377 @ v2.5.2): each
//! Range op's is `<RangeOp ` + its data's + ` >` (`RangeOp::getCacheID`,
//! src/OpenColorIO/ops/range/RangeOp.cpp:185-194). The optimizer combines neighbouring Range ops
//! (`RangeOp::combineWith`, which composes them, RangeOp.cpp:150-174), and removes the leading
//! and trailing identity ranges next to an integer bit depth (`RemoveLeadingClampIdentity`,
//! `RemoveTrailingClampIdentity`, src/OpenColorIO/OpOptimizers.cpp:526-556). Then the
//! processors' pixels, through `image_apply`.
//!
//! A range with only a maximum followed by one with only a minimum, or the other way round,
//! can't be combined: the composition has one bound set on one side only, which `validate`
//! refuses, so the wheel can't build the processor when the optimizer combines Range ops
//! (docs/improvements.md, I-50). The port raises the same.
//!
//! The port builds the processor's ops as the wheel does: a `GroupTransform` builds each child
//! forward, `BuildRangeOp` validates the data and creates a Range op with a copy of it
//! (RangeOp.cpp:263-281), and the processor finalizes them (src/OpenColorIO/Processor.cpp:
//! 623-641), after validating the transform (the group's `validate` doesn't validate its
//! children).

mod common;

use common::image::{add_image, depth_name, port_depth, port_image};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::exception::Result;
use ocio_ops::image_desc::Bytes;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Request};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Map, Value, json};

/// An empty bound.
const E: f64 = f64::NAN;

/// A transform of the lists.
#[derive(Debug, Clone, Copy)]
enum T {
    /// A clamping `RangeTransform`: `[minIn, maxIn, minOut, maxOut]`, NaN for an empty bound.
    Range([f64; 4], TransformDirection),
    /// A `MatrixTransform` scaling RGB by 2 with an offset of 0.1.
    Matrix,
}

use TransformDirection::{Forward as F, Inverse as I};

/// Lists of transforms: upstream's compositions (tests/cpu/ops/range/RangeOpData_tests.cpp,
/// `compose` @ v2.5.2), one-sided ranges each way, constants, inverses, identities at either
/// end, and a matrix between ranges.
fn chains() -> Vec<Vec<T>> {
    let r1 = [0., 1., 0., 1.];
    let r2 = [0.1, 0.9, 0.1, 0.9];
    let r3 = [0.1, 1.9, 0.1, 1.9];
    let r4 = [0.1, 1.9, 0.2, 1.8];
    let r6 = [-1.0, 1.0, 0., 1.2];
    let r7 = [E, 0.5, E, 0.5];
    let r8 = [0.5, E, 0.5, E];
    let r9 = [1.1, 1.9, 1.2, 1.5];
    let r10 = [-1.1, -0.1, 1.1, 1.9];
    let neg = [0., E, 0., E];
    let wide = [-0.1, 1.1, -0.1, 1.1];
    vec![
        vec![T::Range(r1, F)],
        vec![T::Range(r4, F)],
        vec![T::Range(r4, I)],
        vec![T::Range(r7, F)],
        vec![T::Range(r8, I)],
        vec![T::Range(r1, F), T::Range(r2, F)],
        vec![T::Range(r1, F), T::Range(r3, F)],
        vec![T::Range(r1, F), T::Range(r4, F)],
        vec![T::Range(r1, F), T::Range(r6, F)],
        vec![T::Range(r7, F), T::Range(r4, F)],
        vec![T::Range(r4, F), T::Range(r7, F)],
        vec![T::Range(r8, F), T::Range(r3, F)],
        vec![T::Range(r4, F), T::Range(r8, F)],
        vec![T::Range(r1, F), T::Range(r9, F)],
        vec![T::Range(r1, F), T::Range(r10, F)],
        vec![T::Range(r7, F), T::Range(r8, F)],
        vec![T::Range(r8, F), T::Range(r7, F)],
        vec![T::Range(r4, F), T::Range(r4, I)],
        vec![T::Range(r6, I), T::Range(r2, F), T::Range(r9, I)],
        vec![T::Range(neg, F), T::Matrix, T::Range(wide, F)],
        vec![T::Range(r1, F), T::Matrix, T::Range(r1, F)],
        vec![T::Matrix, T::Range(neg, F)],
    ]
}

/// The optimization levels.
const FLAGS: [(&str, OptimizationFlags); 3] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

/// The bit depths, in and out.
const DEPTHS: [(Depth, Depth); 5] = [
    (Depth::F32, Depth::F32),
    (Depth::Uint8, Depth::F32),
    (Depth::F32, Depth::Uint16),
    (Depth::Uint10, Depth::Uint12),
    (Depth::F16, Depth::F16),
];

/// The direction enum of a transform spec.
fn dir_enum(dir: TransformDirection) -> Value {
    match dir {
        F => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
        I => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
    }
}

/// The matrix of [`T::Matrix`].
const SCALE: [f64; 16] = [
    2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 1.,
];
/// The offsets of [`T::Matrix`].
const OFFSET: [f64; 4] = [0.1, 0.1, 0.1, 0.];

/// The spec of a transform.
fn transform(t: &T) -> Value {
    match t {
        T::Range(bounds, dir) => {
            let mut args = Map::new();
            for (key, b) in ["minInValue", "maxInValue", "minOutValue", "maxOutValue"]
                .iter()
                .zip(bounds)
            {
                if !b.is_nan() {
                    args.insert(key.to_string(), json!(b));
                }
            }
            args.insert("direction".into(), dir_enum(*dir));
            json!({"class": "RangeTransform", "args": args})
        }
        T::Matrix => json!({"class": "MatrixTransform",
            "args": {"matrix": SCALE.to_vec(), "offset": OFFSET.to_vec()}}),
    }
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
            T::Range([min_in, max_in, min_out, max_out], dir) => {
                let mut data = RangeOpData::new();
                data.set_min_in_value(*min_in);
                data.set_max_in_value(*max_in);
                data.set_min_out_value(*min_out);
                data.set_max_out_value(*max_out);
                data.set_direction(*dir);
                data.validate()?;
                create_range_op(&mut raw, data, F)?;
            }
            T::Matrix => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&SCALE);
                data.set_rgba_offsets(&OFFSET);
                data.validate()?;
                create_matrix_op(&mut raw, data, F);
            }
        }
    }
    raw.finalize()?;
    CpuProcessor::new(&raw, port_depth(input), port_depth(output), flags)
}

/// Every case: a chain, flags and bit depths.
fn cases() -> Vec<(Vec<T>, usize, (Depth, Depth))> {
    let mut cases = Vec::new();
    for chain in chains() {
        for flags in 0..FLAGS.len() {
            for depths in DEPTHS {
                cases.push((chain.clone(), flags, depths));
            }
        }
    }
    cases
}

#[test]
fn the_cache_id_matches_the_wheel() {
    let cases = cases();
    let pixels: Vec<Vec<u8>> = cases
        .iter()
        .map(|(_, _, (input, _))| {
            vec![
                0;
                4 * match input {
                    Depth::Uint8 => 1,
                    Depth::F32 => 4,
                    _ => 2,
                }
            ]
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .zip(&pixels)
        .map(|((chain, flags, (input, output)), pixel)| BatchCall {
            cmd: "cpu_apply",
            args: processor(chain, FLAGS[*flags].0, *input, *output),
            blobs: vec![pixel],
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((chain, flags, (input, output)), result) in cases.iter().zip(results) {
        let result = result.unwrap_or_else(|e| panic!("{e}")).result;
        let wheel = match result.get("exception") {
            Some(exception) => Err(exception["message"].as_str().unwrap().to_string()),
            None => Ok(result["cpu_cache_id"].as_str().unwrap().to_string()),
        };
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
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
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
                "{what}
  wheel {:?}
  port  {:?}",
                raised.map(|r| r.message),
                port.map_err(|e| e.message().to_string())
            )),
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
