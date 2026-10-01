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
//! With integer and half inputs, the default and the full flags bake a separable prefix
//! holding a Log op into a Lut1D (`OptimizeSeparablePrefix`, OpOptimizers.cpp:553-596), which
//! `lut1d_bake_oracle.rs` checks entry for entry.

mod common;

use common::image::{add_image, port_image};
use common::log_chain::{Affine, T, port_processor, processor};
use ocio_ops::exception::Result;
use ocio_ops::image_desc::Bytes;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Request};
use ocio_testkit::oracle::BatchCall;

use TransformDirection::{Forward as F, Inverse as I};

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

/// The bit depths, in and out.
const DEPTHS: [(Depth, Depth); 6] = [
    (Depth::F32, Depth::F32),
    (Depth::F32, Depth::Uint16),
    (Depth::F32, Depth::F16),
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
    }
    cases
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
