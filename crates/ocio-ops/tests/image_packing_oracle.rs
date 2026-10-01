// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Image packing (`ImagePacking.cpp`) against the wheel: every layout the CPU engine packs
//! channel by channel, from one image to another, for pairs of every bit depth.
//!
//! The wheel applies `log_processor` (a LogTransform) with `CPUProcessor.apply(src, dst)`
//! through the oracle's `image_apply`. The port runs the same engine (`log_engine`) the way
//! `GenericScanlineHelper` runs a layout that isn't RGBA-packed: each row is packed into RGBA F32
//! with `Generic<InType>::PackRGBAFromImageDesc`, processed, and unpacked with
//! `Generic<OutType>::UnpackRGBAToImageDesc` (src/OpenColorIO/ScanlineHelper.cpp:117-177
//! @ v2.5.2). Every buffer must then equal the wheel's, byte for byte: the destination's pixels
//! and padding, and the unchanged source. The scanline helper itself, with the RGBA-packed
//! paths, is `scanline_helper_oracle.rs`.

mod common;

use core::ffi::{c_int, c_long};

use common::image::{
    DEPTHS, Engine, GENERIC_SHAPES, PAIRS, add_image, buffer_indices, buffers_mut, log_engine,
    log_processor, port_depth, port_image, two_log_engine, two_log_processor, with_channel_types,
};
use ocio_ops::Result;
use ocio_ops::image_desc::{Bytes, GenericImageDesc, ImageLayout};
use ocio_ops::image_packing::Generic;
use ocio_ops::open_color_types::BitDepth;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Image, Request};
use serde_json::Value;

/// The port's description of `image` over `buffers`, as its layout.
fn port_layout(image: &Image, buffers: &[Vec<u8>]) -> ImageLayout {
    let desc = port_image(image, |b| Bytes(&buffers[b][..])).expect("a valid layout");
    *desc.desc().layout()
}

/// The port applies `engine` from `src` to `dst` in `buffers`, row by row through
/// `Generic<I>` and `Generic<O>`, as `GenericScanlineHelper` does for layouts that aren't
/// RGBA-packed (src/OpenColorIO/ScanlineHelper.cpp:117-177 @ v2.5.2): the row's first pixel
/// is `m_yIndex * m_dstImg.m_width`, an `int` times a `long`.
fn apply_rows<I: Generic, O: Generic>(
    engine: &Engine,
    (input, output): (BitDepth, BitDepth),
    (src, dst): (&Image, &Image),
    buffers: &mut [Vec<u8>],
) -> Result<()> {
    let (first, ops, last) = engine;
    let src_img = GenericImageDesc::init(&port_layout(src, buffers), input, first.clone())?;
    let dst_img = GenericImageDesc::init(&port_layout(dst, buffers), output, last.clone())?;
    assert!(!src_img.is_rgba_packed() && !dst_img.is_rgba_packed());

    let width = dst_img.width;
    let values = 4 * width as usize;
    let mut rgba = vec![0f32; values];
    let mut in_buffer = vec![I::default(); values];
    let mut out_buffer = vec![O::default(); values];
    let (src_indices, dst_indices) = (buffer_indices(src), buffer_indices(dst));
    let height = c_int::try_from(dst_img.height).expect("rows that an int counts");
    for y in 0..height {
        let start = c_long::from(y).wrapping_mul(width);
        let src_buffers: Vec<&[u8]> = src_indices.iter().map(|&i| &buffers[i][..]).collect();
        I::pack_rgba_from_image_desc(
            &src_img,
            &src_buffers,
            &mut in_buffer,
            &mut rgba,
            width as c_int,
            start,
        )?;
        for op in ops {
            op.apply(&mut rgba);
        }
        let mut dst_buffers = buffers_mut(buffers, &dst_indices);
        O::unpack_rgba_to_image_desc(
            &dst_img,
            &mut dst_buffers,
            &mut rgba,
            &mut out_buffer,
            width as c_int,
            start,
        )?;
    }
    Ok(())
}

/// A processor spec and its CPU engine, for bit depths in and out.
type Processor = (
    fn(BitDepth, BitDepth) -> Value,
    fn(BitDepth, BitDepth) -> Engine,
);

/// Every non-RGBA-packed layout, to itself and to the next one, at the pairs of bit depths
/// `pairs` and several sizes, through `processor`: the port's buffers equal the wheel's after
/// the apply.
fn check_packing((processor_spec, engine): Processor, pairs: &[(Depth, Depth)]) {
    let mut cases = Vec::new();
    for (pair, &(input, output)) in pairs.iter().enumerate() {
        for size in [(1, 1), (3, 2), (17, 3)] {
            for (k, &shape) in GENERIC_SHAPES.iter().enumerate() {
                let next = GENERIC_SHAPES[(k + 1) % GENERIC_SHAPES.len()];
                for dst_shape in [shape, next] {
                    let processor = processor_spec(port_depth(input), port_depth(output));
                    let mut request = Request::new(processor);
                    let seed = Some((pair * 1000 + k) as u64);
                    let src = add_image(&mut request, shape, input, size, seed);
                    let dst = add_image(&mut request, dst_shape, output, size, None);
                    request.apply = vec![0, 1];
                    cases.push((input, output, src, dst, request));
                }
            }
        }
    }
    let calls: Vec<_> = cases.iter().map(|case| case.4.call()).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((input, output, src, dst, request), response) in cases.iter().zip(responses) {
        let what = format!("{input:?} to {output:?}: {src:?}\n  to {dst:?}");
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));
        assert!(reply.raised().is_none(), "{what}: {}", reply.result);

        let (input, output) = (port_depth(*input), port_depth(*output));
        let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
        let engine = engine(input, output);
        with_channel_types!(
            input,
            output,
            apply_rows(&engine, (input, output), (src, dst), &mut buffers)
        )
        .unwrap_or_else(|e| panic!("{what}: the port raised {:?}", e.message()));
        for (k, (port, wheel)) in buffers.iter().zip(&reply.buffers).enumerate() {
            if port != wheel {
                let first = port.iter().zip(wheel).position(|(a, b)| a != b);
                failures.push(format!(
                    "{what}\n  buffer {k} differs first at byte {first:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} applies differ:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

/// Every non-RGBA-packed layout through a LogTransform, at every pair of bit depths.
#[test]
fn generic_packing_matches_the_wheel() {
    check_packing((log_processor, log_engine), &PAIRS);
}

/// Every non-RGBA-packed layout through two LogOps, from each bit depth into F32: the second op
/// is then the destination's, which `Generic<float>::UnpackRGBAToImageDesc` applies.
#[test]
fn two_ops_match_the_wheel() {
    let pairs: Vec<_> = DEPTHS.into_iter().map(|d| (d, Depth::F32)).collect();
    check_packing((two_log_processor, two_log_engine), &pairs);
}

/// Huge images on Windows (docs/improvements.md, I-1): `PackRGBAFromImageDesc` counts the
/// pixels as `long imgPixels = imgWidth * imgHeight` (src/OpenColorIO/ImagePacking.cpp:33-40
/// @ v2.5.2), which wraps at 32 bits on Windows. A planar image of 65,536 by 65,537 pixels has
/// 2^32 + 65,536 of them, which count as 65,536: the first row is processed, and the second
/// fails with "Invalid output image position.", in the wheel and in the port.
///
/// Planar strides of 0 put every pixel of a plane on one value, so a few bytes hold the image.
/// The Python binding counts the entries it requires with the same wrapping product
/// (`checkBufferSize`), so each plane's object has 65,536 entries. Windows only: Linux counts in
/// 64 bits, and would process all 4.3 billion pixels.
#[cfg(target_os = "windows")]
#[test]
fn huge_images_fail_on_windows() {
    use ocio_testkit::battery::BitDepth as Depth;
    use ocio_testkit::image::{Data, Planar, Stride};

    let (width, height) = (65_536, 65_537);
    let wrapped_pixels = 65_536;
    let mut request = Request::new(log_processor(BitDepth::F32, BitDepth::F32));
    let plane = |request: &mut Request, seed: Option<u64>| {
        let bytes = match seed {
            Some(seed) => common::image::source_bytes(BitDepth::F32, 16, seed),
            None => vec![0xa5; 16],
        };
        Data::at(request.buffer(Buffer::Bytes(bytes)), 4).entries(wrapped_pixels)
    };
    let zero = [Stride::Bytes(0), Stride::Bytes(0)];
    let src_planes = (0..3).map(|k| plane(&mut request, Some(k))).collect();
    let src = Planar::new(src_planes, width, height).layout(Depth::F32, zero);
    let dst_planes = (0..3).map(|_| plane(&mut request, None)).collect();
    let dst = Planar::new(dst_planes, width, height).layout(Depth::F32, zero);
    request.image(src.clone());
    request.image(dst.clone());
    request.apply = vec![0, 1];
    let reply = request.run();
    let raised = reply.raised().expect("the wheel refuses the second row");
    assert_eq!(raised.stage, "apply", "{raised:?}");

    let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
    let engine = log_engine(BitDepth::F32, BitDepth::F32);
    let (src, dst) = (Image::Planar(src), Image::Planar(dst));
    let port = apply_rows::<f32, f32>(
        &engine,
        (BitDepth::F32, BitDepth::F32),
        (&src, &dst),
        &mut buffers,
    );
    assert_eq!(
        port.map_err(|e| e.message().to_string()),
        Err(raised.message)
    );
    // The first row was written, as in the wheel.
    assert_eq!(buffers, reply.buffers);
}
