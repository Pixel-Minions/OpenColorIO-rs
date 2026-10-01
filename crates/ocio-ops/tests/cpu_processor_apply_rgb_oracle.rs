// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CPU processor's `applyRGB` and `applyRGBA` against the wheel. Python can't call them:
//! the binding's `applyRGB` and `applyRGBA` build an image and call `apply`
//! (src/bindings/python/PyCPUProcessor.cpp:94-249 @ v2.5.2). So the expected bytes come from
//! the wheel's `CPUProcessor.apply` (the oracle's `image_apply`), through identities with
//! upstream's code.
//!
//! `applyRGBA(pixel)` (src/OpenColorIO/CPUProcessor.cpp:451-465) calls
//! `m_inBitDepthOp->apply(pixel, pixel, 1)`, each op's `apply(pixel, pixel, 1)`, then
//! `m_outBitDepthOp->apply(pixel, pixel, 1)`: the conversions read and write the pixel's own 16
//! bytes (docs/improvements.md, I-41). `applyRGB(pixel)` (CPUProcessor.cpp:433-449) does the
//! same on `{pixel[0], pixel[1], pixel[2], 0.0f}` and keeps the first three floats.
//!
//! - **An F32 end.** `apply(src, dst)` from a 1x1 packed RGBA image of the input bit depth to
//!   a 1x1 packed RGBA image of the output bit depth over the same bytes runs the same calls.
//!   With an F32 output, the destination row is the RGBA row (`m_useDstBuffer`), so the helper
//!   calls `m_inBitDepthOp->apply(srcRow, dstRow, 1)` with both at the pixel, the ops on it,
//!   then `m_outBitDepthOp->apply(dstRow, dstRow, 1)` (src/OpenColorIO/ScanlineHelper.cpp:
//!   117-177 @ v2.5.2): every byte must match. With an F32 input and another output, the helper
//!   converts from its own F32 row into the pixel's first bytes; each wheel's conversion from
//!   F32 reads every float before storing over it (the addresses are in
//!   `BitDepthCast::apply_pixel_in_place`), so those bytes must match. The bytes after them
//!   hold, in `applyRGBA`, the ops' floats: the composition below checks them.
//! - **No F32 output.** No single `apply` converts the input in place then converts to
//!   another bit depth: the helper's RGBA row is its own buffer. The test composes two calls:
//!   the processor of the same transforms and flags from the input bit depth to F32, over the
//!   same bytes as above, whose ops must be the (in, out) processor's (the cache IDs say so);
//!   then an identity processor from F32 to the output bit depth, from the 16 bytes that left
//!   to the output bit depth over the same bytes. The identity's one op is an identity matrix,
//!   which `FinalizeOpsForCPU` adds (CPUProcessor.cpp:311-338) and which renders as the Scale
//!   renderer (`GetMatrixRenderer` and `ScaleRenderer::apply`,
//!   src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:90-105, 400-428), a multiply by 1, exact on
//!   the first call's floats: they come from the ops' arithmetic, so none is a signaling NaN.
//!   Its conversion reads the floats before storing over them, as above, and leaves the bytes
//!   after the output pixel alone: those are the ops' floats, as in `applyRGBA`.

mod common;

use common::image::{port_depth, source_bytes};
use common::matrix::{Chain, FLAGS, chains, identity, port_processor, processor};
use ocio_ops::open_color_types::BitDepth;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Channels, Data, Packed, Reply, Request, Stride};
use ocio_testkit::oracle::BatchCall;

/// The bit depths of the CPU processor.
const DEPTHS: [Depth; 6] = [
    Depth::Uint8,
    Depth::Uint10,
    Depth::Uint12,
    Depth::Uint16,
    Depth::F16,
    Depth::F32,
];

/// The bytes of a channel of `depth`.
fn size(depth: Depth) -> usize {
    match depth {
        Depth::Uint8 => 1,
        Depth::F32 => 4,
        _ => 2,
    }
}

/// A 1x1 packed RGBA image of `depth` at the start of buffer `buffer`.
fn pixel_image(buffer: usize, depth: Depth) -> Packed {
    Packed::new(Data::at(buffer, 0), 1, 1, Channels::Count(4)).layout(depth, [Stride::Auto; 3])
}

/// The wheel's `apply(src, dst)` from a 1x1 image of `input` to a 1x1 image of `output`, both
/// over `bytes` (16 bytes), with `processor`.
fn overlapping(processor: serde_json::Value, bytes: &[u8], input: Depth, output: Depth) -> Request {
    let mut request = Request::new(processor);
    let buffer = request.buffer(Buffer::Bytes(bytes.to_vec()));
    request.image(pixel_image(buffer, input));
    request.image(pixel_image(buffer, output));
    request.apply = vec![0, 1];
    request
}

/// The pixel `bytes` hold.
fn floats(bytes: &[u8]) -> [f32; 4] {
    std::array::from_fn(|k| f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().unwrap()))
}

/// The bytes of `values`.
fn bytes_of(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_ne_bytes()).collect()
}

/// The pixels of a case: the RGBA pixel's bytes, values of the input bit depth, and the bytes
/// of the RGB pixel's `{r, g, b, 0.0f}`.
fn pixels(input: Depth, seed: u64) -> [Vec<u8>; 2] {
    let rgba = source_bytes(port_depth(input), 16, seed);
    let mut rgb = rgba[..12].to_vec();
    rgb.extend_from_slice(&0.0f32.to_ne_bytes());
    [rgba, rgb]
}

/// The port's `applyRGBA` and `applyRGB` on the pixels of [`pixels`]: the RGBA pixel's 16
/// bytes, and the RGB pixel's 12.
fn port_apply(
    chain: &Chain,
    flags: usize,
    (input, output): (Depth, Depth),
    [rgba, rgb]: &[Vec<u8>; 2],
) -> [Vec<u8>; 2] {
    let cpu = port_processor(chain, FLAGS[flags].1, port_depth(input), port_depth(output))
        .unwrap_or_else(|e| panic!("{}", e.message()));
    let mut rgba = floats(rgba);
    cpu.apply_rgba(&mut rgba);
    let rgb4 = floats(rgb);
    let mut rgb = [rgb4[0], rgb4[1], rgb4[2]];
    cpu.apply_rgb(&mut rgb);
    [bytes_of(&rgba), bytes_of(&rgb)]
}

/// The replies of `requests`, which must not raise.
fn run(requests: &[Request]) -> Vec<Reply> {
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .zip(requests)
        .map(|(response, request)| {
            let reply = request.reply(response.unwrap_or_else(|e| panic!("{e}")));
            assert!(reply.raised().is_none(), "{:?}", reply.raised());
            reply
        })
        .collect()
}

/// The ops part of a CPU processor's cache ID, after `ops: `.
fn ops(cache_id: &str) -> &str {
    let (_, ops) = cache_id
        .split_once(" ops: ")
        .unwrap_or_else(|| panic!("no ops in {cache_id:?}"));
    ops
}

/// Pairs with an F32 end, each list of transforms and both optimization levels, several
/// pixels: the single `apply` over the pixel's bytes (see the module documentation).
#[test]
fn apply_rgb_and_rgba_with_an_f32_end_match_the_wheel() {
    let chains = chains();
    let mut cases = Vec::new();
    for input in DEPTHS {
        for output in DEPTHS {
            if input != Depth::F32 && output != Depth::F32 {
                continue;
            }
            for (c, chain) in chains.iter().enumerate() {
                for flags in 0..FLAGS.len() {
                    for seed in 0..3 {
                        let pixels = pixels(input, (c * 10 + flags * 100 + seed) as u64);
                        cases.push((chain, flags, (input, output), pixels));
                    }
                }
            }
        }
    }
    let requests: Vec<Request> = cases
        .iter()
        .flat_map(|(chain, flags, (input, output), pixels)| {
            let processor = processor(
                chain,
                FLAGS[*flags].0,
                port_depth(*input),
                port_depth(*output),
            );
            pixels
                .iter()
                .map(move |bytes| overlapping(processor.clone(), bytes, *input, *output))
        })
        .collect();
    let replies = run(&requests);

    let mut failures = Vec::new();
    for ((chain, flags, depths, pixels), wheel) in cases.iter().zip(replies.chunks(2)) {
        // With an F32 output every byte; else the output pixel's.
        let n = if depths.1 == Depth::F32 {
            16
        } else {
            4 * size(depths.1)
        };
        let port = port_apply(chain, *flags, *depths, pixels);
        for (k, (port, wheel)) in port.iter().zip(wheel).enumerate() {
            let n = n.min(port.len());
            if port[..n] != wheel.buffers[0][..n] {
                failures.push(format!(
                    "{} {chain:?} {} {depths:?} {:02x?}\n  wheel {:02x?}\n  port  {:02x?}",
                    ["applyRGBA", "applyRGB"][k],
                    FLAGS[*flags].0,
                    pixels[k],
                    &wheel.buffers[0][..n],
                    &port[..n]
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        2 * cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

/// Pairs without an F32 output, F32 input included, each list of transforms and both
/// optimization levels, several pixels: the two-call composition, every byte (see the module
/// documentation). The first processor's ops must be the (in, out) processor's.
#[test]
fn apply_rgb_and_rgba_without_an_f32_output_match_the_composition() {
    let chains = chains();
    let mut cases = Vec::new();
    for input in DEPTHS {
        for output in DEPTHS {
            if output == Depth::F32 {
                continue;
            }
            for (c, chain) in chains.iter().enumerate() {
                for flags in 0..FLAGS.len() {
                    for seed in 0..3 {
                        let pixels = pixels(input, (c * 10 + flags * 100 + seed + 7) as u64);
                        cases.push((chain, flags, (input, output), pixels));
                    }
                }
            }
        }
    }

    // The (in, out) processors' cache IDs, through `cpu_apply` on a pixel of zeros.
    let zeros: Vec<Vec<u8>> = cases
        .iter()
        .map(|(_, _, (input, _), _)| vec![0; 4 * size(*input)])
        .collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .zip(&zeros)
        .map(|((chain, flags, (input, output), _), pixel)| BatchCall {
            cmd: "cpu_apply",
            args: processor(
                chain,
                FLAGS[*flags].0,
                port_depth(*input),
                port_depth(*output),
            ),
            blobs: vec![pixel],
        })
        .collect();
    let cache_ids: Vec<String> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| {
            let result = r.unwrap_or_else(|e| panic!("{e}")).result;
            result["cpu_cache_id"]
                .as_str()
                .unwrap_or_else(|| panic!("{result}"))
                .to_string()
        })
        .collect();

    // First call: from the input bit depth to F32, over the pixel's bytes.
    let first: Vec<Request> = cases
        .iter()
        .flat_map(|(chain, flags, (input, _), pixels)| {
            let processor = processor(chain, FLAGS[*flags].0, port_depth(*input), BitDepth::F32);
            pixels
                .iter()
                .map(move |bytes| overlapping(processor.clone(), bytes, *input, Depth::F32))
        })
        .collect();
    let first = run(&first);
    let mut ops_differ = Vec::new();
    for ((case, cache_id), replies) in cases.iter().zip(&cache_ids).zip(first.chunks(2)) {
        let first_id = replies[0].result["cpu_cache_id"].as_str().unwrap();
        if ops(first_id) != ops(cache_id) {
            ops_differ.push(format!(
                "{:?} {} {:?}\n  (in, out) {cache_id:?}\n  (in, F32) {first_id:?}",
                case.0, FLAGS[case.1].0, case.2
            ));
        }
    }
    assert!(
        ops_differ.is_empty(),
        "the processors to F32 render other ops:\n{}",
        ops_differ.join("\n")
    );

    // Second call: an identity processor from F32 to the output bit depth, on those bytes.
    let second: Vec<Request> = cases
        .iter()
        .zip(first.chunks(2))
        .flat_map(|((_, _, (_, output), _), replies)| {
            let processor = processor(&identity(), FLAGS[1].0, BitDepth::F32, port_depth(*output));
            replies.iter().map(move |reply| {
                overlapping(processor.clone(), &reply.buffers[0], Depth::F32, *output)
            })
        })
        .collect();
    let second = run(&second);

    let mut failures = Vec::new();
    for ((chain, flags, depths, pixels), wheel) in cases.iter().zip(second.chunks(2)) {
        let port = port_apply(chain, *flags, *depths, pixels);
        for (k, (port, wheel)) in port.iter().zip(wheel).enumerate() {
            let n = port.len();
            if port[..] != wheel.buffers[0][..n] {
                failures.push(format!(
                    "{} {chain:?} {} {depths:?} {:02x?}\n  wheel {:02x?}\n  port  {:02x?}",
                    ["applyRGBA", "applyRGB"][k],
                    FLAGS[*flags].0,
                    pixels[k],
                    &wheel.buffers[0][..n],
                    port
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        2 * cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

/// The in-place conversion from UINT16 differs between the wheels (docs/improvements.md, I-41):
/// the codes 1000, 2000, 3000 and 4000 through an identity processor from UINT16 to F32, over
/// the pixel's bytes and between two buffers, on the wheel of the platform that runs the test.
/// The port's `applyRGBA` gives the wheel's bytes. In place, Windows reads green, blue and
/// alpha after storing a float over them; Linux reads alpha only so, and green and blue are the
/// codes' own conversions, as between two buffers.
#[test]
fn the_in_place_conversion_from_uint16_follows_the_wheel() {
    let codes: Vec<u8> = [1000u16, 2000, 3000, 4000]
        .iter()
        .flat_map(|c| c.to_ne_bytes())
        .chain([0u8; 8])
        .collect();
    let flags = FLAGS[0];
    let processor = || processor(&identity(), flags.0, BitDepth::Uint16, BitDepth::F32);

    let in_place = overlapping(processor(), &codes, Depth::Uint16, Depth::F32);
    let mut apart = Request::new(processor());
    let src = apart.buffer(Buffer::Bytes(codes.clone()));
    let dst = apart.buffer(Buffer::Bytes(vec![0; 16]));
    apart.image(pixel_image(src, Depth::Uint16));
    apart.image(pixel_image(dst, Depth::F32));
    apart.apply = vec![0, 1];
    let replies = run(&[in_place, apart]);
    let in_place = floats(&replies[0].buffers[0]);
    let apart = floats(&replies[1].buffers[1]);

    let cpu = port_processor(&identity(), flags.1, BitDepth::Uint16, BitDepth::F32)
        .unwrap_or_else(|e| panic!("{}", e.message()));
    let mut port = floats(&codes);
    cpu.apply_rgba(&mut port);
    assert_eq!(
        bytes_of(&port),
        replies[0].buffers[0],
        "port {port:?}, wheel {in_place:?}"
    );

    let same: Vec<bool> = (0..4)
        .map(|k| in_place[k].to_bits() == apart[k].to_bits())
        .collect();
    let expected = if cfg!(target_os = "windows") {
        [true, false, false, false]
    } else {
        [true, true, true, false]
    };
    assert_eq!(same, expected, "in place {in_place:?}, apart {apart:?}");
}
