// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The optimizer's separable-prefix bake against the wheel, entry for entry: with integer and
//! half input, the default and the full flags (`OPTIMIZATION_COMP_SEPARABLE_PREFIX`) replace
//! the leading ops that don't mix channels, if one of them isn't a Matrix or a Range, with a
//! 1D LUT that looks the input's codes up (`OptimizeSeparablePrefix` and
//! `FindSeparablePrefix`, src/OpenColorIO/OpOptimizers.cpp:464-596 @ v2.5.2). The LUT is the
//! lookup domain (`Lut1DOpData::MakeLookupDomain`, src/OpenColorIO/ops/lut1d/
//! Lut1DOpData.cpp:508-526) rendered through copies of the prefix's ops at F32, without fast
//! math (`ComposeVec` and `EvalTransform`, Lut1DOpData.cpp:683-705, src/OpenColorIO/ops/
//! OpTools.cpp:11-46), so its entries follow each platform's math library.
//!
//! - The optimized processor (`getOptimizedProcessor`: finalize, optimize, optimizeForBitdepth,
//!   src/OpenColorIO/Processor.cpp:382-399), through the oracle's `processor_ops`: its cache
//!   ID, and its first transform's `getData()`, the baked LUT's values, bit for bit;
//! - the CPU processor's pixels, for every code of each input bit depth on each channel.
//!
//! The prefixes hold Log, LogAffine and LogCamera transforms, with Matrix and Range
//! transforms before and after them; a matrix that mixes channels ends the prefix; a prefix
//! of Matrix and Range transforms alone isn't baked.

mod common;

use common::image::{depth_name, port_depth, port_image};
use common::log_chain::{Affine, T, port_processor, port_raw_ops, processor};
use ocio_ops::exception::Result;
use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::image_desc::Bytes;
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Channels, Data, Packed, Request, Stride};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// The input bit depths the bake serves.
const INPUTS: [Depth; 5] = [
    Depth::Uint8,
    Depth::Uint10,
    Depth::Uint12,
    Depth::Uint16,
    Depth::F16,
];

/// The optimization levels that bake.
const FLAGS: [(&str, OptimizationFlags); 2] = [
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

fn chains() -> Vec<Vec<T>> {
    let up: Affine = [[0.18; 3], [1.0; 3], [2.0; 3], [0.1; 3]];
    let unequal: Affine = [[0.5, 1.0, 1.5], [1.0; 3], [2.0; 3], [-0.1, 0.0, 0.1]];
    let brk = [0.1, 0.2, 0.3];
    vec![
        vec![T::Log(2.0, F)],
        vec![T::Log(10.0, I)],
        vec![T::Log(std::f64::consts::E, F)],
        vec![T::Affine(10.0, up, F)],
        vec![T::Affine(10.0, unequal, I)],
        vec![T::Camera(2.0, up, brk, None, F)],
        vec![T::Camera(2.0, up, brk, Some([1.5, 1.25, 1.0]), I)],
        vec![T::Matrix, T::Log(2.0, F)],
        vec![T::Range, T::Log(10.0, F), T::Matrix],
        vec![T::Log(2.0, F), T::CrossMatrix, T::Log(2.0, I)],
        vec![T::CrossMatrix, T::Log(2.0, F)],
        vec![T::Range, T::Matrix],
    ]
}

/// The port's optimized ops: `getOptimizedProcessor`'s steps on the processor's ops.
fn port_optimized(chain: &[T], flags: OptimizationFlags, input: Depth) -> Result<OpVec> {
    let mut ops = port_raw_ops(chain)?;
    ops.finalize()?;
    ops.optimize(flags)?;
    ops.optimize_for_bitdepth(port_depth(input), BitDepth::F32, flags)?;
    Ok(ops)
}

#[test]
fn the_baked_luts_match_the_wheel() {
    let mut cases = Vec::new();
    for chain in chains() {
        for input in INPUTS {
            for (name, flags) in FLAGS {
                cases.push((chain.clone(), input, name, flags));
            }
        }
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(chain, input, name, _)| {
            let children: Vec<Value> = chain.iter().map(common::log_chain::transform).collect();
            BatchCall {
                cmd: "processor_ops",
                args: json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "in_bitdepth": depth_name(port_depth(*input)),
                    "optimization": name,
                }),
                blobs: vec![],
            }
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    let mut baked = 0;
    for ((chain, input, name, flags), response) in cases.iter().zip(responses) {
        let what = format!("{chain:?} {input:?} {name}");
        let response = response.unwrap_or_else(|e| panic!("{what}: {e}"));
        let result = &response.result;
        assert!(result.get("exception").is_none(), "{what}: {result}");
        let optimized = &result["optimized"];
        let ops = match port_optimized(chain, *flags, *input) {
            Ok(ops) => ops,
            Err(e) => {
                failures.push(format!("{what}: the port raised {}", e.message()));
                continue;
            }
        };

        // The cache ID: the hash of the ops' cache IDs.
        let port_id = if ops.is_empty() {
            "<NOOP>".to_string()
        } else {
            cache_id_hash(&ops.get_cache_id().unwrap())
        };
        if port_id != optimized["cache_id"].as_str().unwrap() {
            failures.push(format!("{what}: the cache IDs differ"));
        }

        // The baked LUT, entry for entry.
        let first = &optimized["group"]["children"][0];
        let wheel_lut = first["class"] == "Lut1DTransform";
        let port_lut = ops.first().and_then(|op| match &**op.data() {
            OpData::Lut1D(lut) => Some(lut.clone()),
            _ => None,
        });
        match (wheel_lut, port_lut) {
            (false, None) => {}
            (true, Some(lut)) => {
                baked += 1;
                let getters = &first["getters"];
                let blob = &response.blobs[getters["getData"]["blob"].as_u64().unwrap() as usize];
                let wheel: Vec<u32> = blob
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_ne_bytes(*b))
                    .collect();
                let port: Vec<u32> = lut
                    .get_array()
                    .get_values()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect();
                if getters["getInputHalfDomain"] != lut.is_input_half_domain()
                    || getters["getLength"] != lut.get_array().get_length()
                {
                    failures.push(format!("{what}: the LUTs' domains differ"));
                } else if port != wheel {
                    let k = port
                        .iter()
                        .zip(&wheel)
                        .position(|(p, w)| p != w)
                        .unwrap_or(0);
                    failures.push(format!(
                        "{what}: {} of {} values differ, first at {k}: wheel {:#010x} port \
                         {:#010x}",
                        port.iter().zip(&wheel).filter(|(p, w)| p != w).count(),
                        wheel.len(),
                        wheel.get(k).copied().unwrap_or(0),
                        port.get(k).copied().unwrap_or(0)
                    ));
                }
            }
            (wheel, port) => failures.push(format!(
                "{what}: the wheel baked {wheel}, the port {}",
                port.is_some()
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
    // Most cases bake; some don't.
    assert!(baked > cases.len() / 2 && baked < cases.len(), "{baked}");
}

/// Every code of `depth` on each channel: red `i`, green the codes backwards, blue a
/// permutation, alpha `i`, in rows of 256 pixels.
fn ramp(depth: Depth) -> (Vec<u8>, i64) {
    let n = Lut1DOpData::get_lut_ideal_size(port_depth(depth)).unwrap() as usize;
    let mut bytes = Vec::new();
    for i in 0..n {
        for code in [i, n - 1 - i, (i * 97) % n, i] {
            if depth == Depth::Uint8 {
                bytes.push(code as u8);
            } else {
                bytes.extend_from_slice(&(code as u16).to_ne_bytes());
            }
        }
    }
    (bytes, (n / 256) as i64)
}

fn packed(buffer: usize, height: i64, depth: Depth) -> Packed {
    Packed::new(Data::at(buffer, 0), 256, height, Channels::Count(4))
        .layout(depth, [Stride::Auto; 3])
}

/// The CPU processors' pixels, for every code, with the default flags, to F32 and to 16-bit
/// output.
#[test]
fn every_code_matches_the_wheel() {
    let mut cases = Vec::new();
    for chain in chains() {
        for input in INPUTS {
            for output in [Depth::F32, Depth::Uint16] {
                cases.push((chain.clone(), input, output));
            }
        }
    }
    let requests: Vec<Request> = cases
        .iter()
        .map(|(chain, input, output)| {
            let (bytes, height) = ramp(*input);
            let out_size = if *output == Depth::F32 { 4 } else { 2 };
            let mut request = Request::new(processor(chain, FLAGS[0].0, *input, *output));
            let src = request.buffer(Buffer::Bytes(bytes));
            let dst = request.buffer(Buffer::Bytes(vec![0; 4 * 256 * height as usize * out_size]));
            request.image(packed(src, height, *input));
            request.image(packed(dst, height, *output));
            request.apply = vec![0, 1];
            request
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for (((chain, input, output), request), response) in cases.iter().zip(&requests).zip(responses)
    {
        let what = format!("{chain:?} {input:?}->{output:?}");
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));
        assert!(reply.raised().is_none(), "{what}: {:?}", reply.raised());
        let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
        let port = (|| -> Result<()> {
            let cpu = port_processor(chain, FLAGS[0].1, *input, *output)?;
            let (src_buffers, dst_buffers) = buffers.split_at_mut(1);
            let src = port_image(&request.images[0], |_| Bytes(&src_buffers[0][..]))?;
            let mut slot = Some(&mut dst_buffers[0][..]);
            let mut dst = port_image(&request.images[1], |_| {
                Bytes(slot.take().expect("one buffer"))
            })?;
            cpu.apply_src_dst(src.desc(), dst.desc_mut())
        })();
        match port {
            Err(e) => failures.push(format!("{what}: the port raised {}", e.message())),
            Ok(()) if buffers[1] != reply.buffers[1] => {
                failures.push(format!("{what}: the pixels differ"));
            }
            Ok(()) => {}
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
