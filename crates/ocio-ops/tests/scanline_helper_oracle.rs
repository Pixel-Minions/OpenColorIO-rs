// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The scanline helper (`ScanlineHelper.cpp`) against the wheel: images processed row by row,
//! RGBA-packed ones a whole row at a time and the others channel by channel, from one image to
//! another and in place, for pairs of every bit depth.
//!
//! The wheel applies `log_processor` (a LogTransform) with `CPUProcessor.apply` through the
//! oracle's `image_apply`. The port runs the same engine (`log_engine`) through
//! `GenericScanlineHelper`, as `CPUProcessor::Impl::apply` does
//! (src/OpenColorIO/CPUProcessor.cpp:379-432 @ v2.5.2): `prepRGBAScanline`, the ops, then
//! `finishRGBAScanline`, until no rows are left. Every buffer must then equal the wheel's, byte
//! for byte. Also the helper's errors, from `init`.
//!
//! Upstream's `apply(src, dst)` also takes one image as both source and destination; the port's
//! borrows don't, and the helper's `init_same` serves that case, which
//! `cpu_processor_apply_oracle.rs` checks through the CPU processor's `apply_same`.

mod common;

use common::image::{
    DEPTHS, Engine, GENERIC_SHAPES, PACKED_SHAPES, PAIRS, Shape, add_image, buffer_indices,
    log_engine, log_processor, port_depth, port_image, two_log_engine, two_log_processor,
    with_channel_types,
};
use ocio_ops::Result;
use ocio_ops::image_packing::Generic;
use ocio_ops::open_color_types::BitDepth;
use ocio_ops::scanline_helper::{GenericScanlineHelper, ScanlineHelper};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Image, Request};

/// The images of an apply.
#[derive(Debug, Clone)]
enum Apply {
    /// `apply(src, dst)`.
    SrcDst(Image, Image),
    /// `apply(img)`, in place.
    InPlace(Image),
}

/// Runs the helper's loop: every row, through `ops`.
fn process<'a>(helper: &mut dyn ScanlineHelper<'a>, engine: &Engine) -> Result<()> {
    while let Some(rgba) = helper.prep_rgba_scanline()? {
        for op in &engine.1 {
            op.apply(rgba);
        }
        helper.finish_rgba_scanline()?;
    }
    Ok(())
}

/// The port applies `engine` to the images of `apply` in `buffers`, through
/// `GenericScanlineHelper<I, O>`.
fn apply_scanlines<I: Generic, O: Generic>(
    engine: &Engine,
    (input, output): (BitDepth, BitDepth),
    apply: &Apply,
    buffers: &mut [Vec<u8>],
) -> Result<()> {
    let (first, _, last) = engine;
    let mut helper = GenericScanlineHelper::<I, O>::new(input, first.clone(), output, last.clone());
    match apply {
        Apply::SrcDst(src, dst) => {
            // The destination's buffers come after the source's.
            let first_dst = buffer_indices(dst).into_iter().min().expect("a buffer");
            let (src_buffers, dst_buffers) = buffers.split_at_mut(first_dst);
            let src = port_image(src, |b| ocio_ops::image_desc::Bytes(&src_buffers[b][..]))?;
            let mut slots: Vec<Option<&mut [u8]>> =
                dst_buffers.iter_mut().map(|b| Some(&mut b[..])).collect();
            let mut dst = port_image(dst, |b| {
                ocio_ops::image_desc::Bytes(slots[b - first_dst].take().expect("once"))
            })?;
            helper.init_src_dst(src.desc(), dst.desc_mut())?;
            process(&mut helper, engine)
        }
        Apply::InPlace(img) => {
            let mut slots: Vec<Option<&mut [u8]>> =
                buffers.iter_mut().map(|b| Some(&mut b[..])).collect();
            let mut img = port_image(img, |b| {
                ocio_ops::image_desc::Bytes(slots[b].take().expect("once"))
            })?;
            helper.init(img.desc_mut())?;
            process(&mut helper, engine)
        }
    }
}

/// A case: the bit depths, the images, and the request.
type Case = (Depth, Depth, Apply, Request);

/// Runs `cases` on the wheel and the port, whose engine for the cases' bit depths is `engine`:
/// each gives the same buffers, or the same error.
fn check(cases: &[Case], engine: fn(BitDepth, BitDepth) -> Engine) {
    let calls: Vec<_> = cases.iter().map(|case| case.3.call()).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((input, output, apply, request), response) in cases.iter().zip(responses) {
        let what = format!("{input:?} to {output:?}: {apply:?}");
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{what}: {e}")));

        let (input, output) = (port_depth(*input), port_depth(*output));
        let mut buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
        let engine = engine(input, output);
        let port = with_channel_types!(
            input,
            output,
            apply_scanlines(&engine, (input, output), apply, &mut buffers)
        );
        match (reply.raised(), port) {
            (None, Ok(())) => {
                for (k, (port, wheel)) in buffers.iter().zip(&reply.buffers).enumerate() {
                    if port != wheel {
                        let first = port.iter().zip(wheel).position(|(a, b)| a != b);
                        failures.push(format!("{what}\n  buffer {k} differs at byte {first:?}"));
                    }
                }
            }
            (Some(raised), Err(e)) if raised.message == e.message() => {}
            (raised, port) => failures.push(format!(
                "{what}\n  wheel {:?}\n  port  {:?}",
                raised.map(|r| r.message),
                port.map_err(|e| e.message().to_string())
            )),
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

/// The layouts of the scanline tests: the RGBA-packed ones, and some the helper packs.
fn shapes() -> Vec<Shape> {
    let mut shapes = PACKED_SHAPES.to_vec();
    shapes.extend([GENERIC_SHAPES[1], GENERIC_SHAPES[9], GENERIC_SHAPES[0]]);
    shapes
}

/// From one image to another: each layout to itself and to the next one (RGBA-packed to
/// channel by channel and back), at every pair of bit depths and several sizes.
#[test]
fn src_to_dst_matches_the_wheel() {
    let shapes = shapes();
    let mut cases = Vec::new();
    for (pair, (input, output)) in PAIRS.into_iter().enumerate() {
        for size in [(1, 1), (3, 2), (17, 3), (33, 2)] {
            for (k, &shape) in shapes.iter().enumerate() {
                for dst_shape in [shape, shapes[(k + 1) % shapes.len()]] {
                    let processor = log_processor(port_depth(input), port_depth(output));
                    let mut request = Request::new(processor);
                    let seed = Some((pair * 1000 + k) as u64);
                    let src = add_image(&mut request, shape, input, size, seed);
                    let dst = add_image(&mut request, dst_shape, output, size, None);
                    request.apply = vec![0, 1];
                    cases.push((input, output, Apply::SrcDst(src, dst), request));
                }
            }
        }
    }
    check(&cases, log_engine);
}

/// In place: every layout, at every bit depth, as input and output.
#[test]
fn in_place_matches_the_wheel() {
    let mut cases = Vec::new();
    for (pair, (input, output)) in PAIRS.into_iter().enumerate() {
        if input != output {
            continue;
        }
        for size in [(1, 1), (3, 2), (17, 3), (33, 2)] {
            for (k, &shape) in shapes().iter().enumerate() {
                let processor = log_processor(port_depth(input), port_depth(output));
                let mut request = Request::new(processor);
                let seed = Some((pair * 1000 + k) as u64);
                let img = add_image(&mut request, shape, input, size, seed);
                request.apply = vec![0];
                cases.push((input, output, Apply::InPlace(img), request));
            }
        }
    }
    check(&cases, log_engine);
}

/// The helper's errors, with the wheel's messages: an image whose bit depth isn't the CPU
/// processor's (`GenericImageDesc::init`), and images of different sizes (`init`).
#[test]
fn init_errors_match_the_wheel() {
    let rgba = PACKED_SHAPES[0];
    let mut cases = Vec::new();
    // The processor takes 8 bits in and F32 out; each image has the wrong depth in turn.
    for (src_depth, dst_depth) in [(Depth::F32, Depth::F32), (Depth::Uint8, Depth::Uint8)] {
        let processor = log_processor(BitDepth::Uint8, BitDepth::F32);
        let mut request = Request::new(processor);
        let src = add_image(&mut request, rgba, src_depth, (2, 2), Some(7));
        let dst = add_image(&mut request, rgba, dst_depth, (2, 2), None);
        request.apply = vec![0, 1];
        cases.push((Depth::Uint8, Depth::F32, Apply::SrcDst(src, dst), request));
    }
    // In place, the image has the input's depth but not the output's.
    let mut request = Request::new(log_processor(BitDepth::Uint8, BitDepth::F32));
    let img = add_image(&mut request, rgba, Depth::Uint8, (2, 2), Some(8));
    request.apply = vec![0];
    cases.push((Depth::Uint8, Depth::F32, Apply::InPlace(img), request));
    // Different sizes.
    for (src_size, dst_size) in [((3, 2), (2, 3)), ((3, 2), (3, 1))] {
        let mut request = Request::new(log_processor(BitDepth::F32, BitDepth::F32));
        let src = add_image(&mut request, rgba, Depth::F32, src_size, Some(9));
        let dst = add_image(&mut request, rgba, Depth::F32, dst_size, None);
        request.apply = vec![0, 1];
        cases.push((Depth::F32, Depth::F32, Apply::SrcDst(src, dst), request));
    }
    check(&cases, log_engine);
}

/// Two LogOps, from each bit depth into F32, from one image to another and in place: the second
/// op is then the destination's, which the helper applies to an RGBA-packed F32 row in place
/// and `Generic<float>::UnpackRGBAToImageDesc` to any other.
#[test]
fn two_ops_match_the_wheel() {
    let shapes = shapes();
    let mut cases = Vec::new();
    for (d, input) in DEPTHS.into_iter().enumerate() {
        let output = Depth::F32;
        for size in [(1, 1), (17, 3)] {
            for (k, &shape) in shapes.iter().enumerate() {
                let seed = Some((d * 1000 + k) as u64);
                for dst_shape in [shape, shapes[(k + 1) % shapes.len()]] {
                    let processor = two_log_processor(port_depth(input), port_depth(output));
                    let mut request = Request::new(processor);
                    let src = add_image(&mut request, shape, input, size, seed);
                    let dst = add_image(&mut request, dst_shape, output, size, None);
                    request.apply = vec![0, 1];
                    cases.push((input, output, Apply::SrcDst(src, dst), request));
                }
                if input == output {
                    let processor = two_log_processor(port_depth(input), port_depth(output));
                    let mut request = Request::new(processor);
                    let img = add_image(&mut request, shape, input, size, seed);
                    request.apply = vec![0];
                    cases.push((input, output, Apply::InPlace(img), request));
                }
            }
        }
    }
    check(&cases, two_log_engine);
}

/// Huge images on Windows (docs/improvements.md, I-1), through the helper: the case of
/// `image_packing_oracle.rs`, a planar F32 image of 65,536 by 65,537 pixels, whose wrapped
/// pixel count is 65,536. The helper processes the first row, and the second fails with
/// "Invalid output image position.", in the wheel and in the port, which leave the same bytes.
#[cfg(target_os = "windows")]
#[test]
fn huge_images_fail_on_windows() {
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
    let apply = Apply::SrcDst(Image::Planar(src), Image::Planar(dst));
    let port = apply_scanlines::<f32, f32>(
        &engine,
        (BitDepth::F32, BitDepth::F32),
        &apply,
        &mut buffers,
    );
    assert_eq!(
        port.map_err(|e| e.message().to_string()),
        Err(raised.message)
    );
    // The first row was written, as in the wheel.
    assert_eq!(buffers, reply.buffers);
}
