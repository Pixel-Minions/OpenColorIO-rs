// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Planar image descriptions against the wheel, through the oracle's `image_apply` with no
//! apply: the wheel constructs each description, and returns its getters or what it raised.
//!
//! - Planar images with and without alpha, in separate buffers and in one, of every bit depth
//!   and constructor, with default, tight, padded, negative and zero strides: the port's
//!   getters equal the wheel's.
//! - Every error the library raises that Python reaches, with its message; where the Python
//!   binding raises first, the binding's message for the port's typed slices, with the
//!   expected type named cleanly (improvement candidate I-23).
//!
//! As in `image_desc_oracle.rs`, the wheel's planes alias their first entry, and the port's
//! get buffers that hold every pixel, the first one in the middle.

mod common;

use std::collections::HashMap;

use common::image::{DEPTHS, getters, planar_reach, port_planar};
use ocio_ops::Exception;
use ocio_ops::image_desc::{AUTO_STRIDE, PixelData, PlanarImageDesc};
use ocio_ops::open_color_types::BitDepth;
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{Buffer, Data, Planar, Raised, Reply, Request, Stride, channel_bytes};
use serde_json::json;

/// A request that constructs `spec`, with `planes` planes (3 or 4), each holding every pixel with
/// the first one in its middle: in separate buffers, or in one. The spec as the request has it.
fn construct(spec: &Planar, planes: usize, one_buffer: bool) -> (Request, Planar) {
    let reach = planar_reach(spec).min(1 << 19) as usize;
    let span = 2 * reach + 64;
    let mut request = Request::new(json!({}));
    let mut spec = spec.clone();
    spec.planes = if one_buffer {
        let buffer = request.buffer(Buffer::fill(planes * span, &[0]));
        (0..planes)
            .map(|k| Data::at(buffer, k * span + reach + 32))
            .collect()
    } else {
        (0..planes)
            .map(|_| Data::at(request.buffer(Buffer::fill(span, &[0])), reach + 32))
            .collect()
    };
    request.image(spec.clone());
    (request, spec)
}

/// A planar spec of `width` by `height` pixels, with a bit depth and strides.
fn spec(width: i64, height: i64, layout: Option<(Depth, [Stride; 2])>) -> Planar {
    Planar {
        layout: layout.map(|(depth, strides)| (depth.into(), strides)),
        ..Planar::new(Vec::new(), width, height)
    }
}

/// Every layout the getters test uses, of `width` by `height` pixels.
fn layouts(width: i64, height: i64) -> Vec<Planar> {
    let b = Stride::Bytes;
    let a = Stride::Auto;
    let mut out = vec![spec(width, height, None)];
    for depth in DEPTHS {
        let item = channel_bytes(depth) as i64;
        let strides = [
            [a, a],
            // Tight, given.
            [b(item), b(item * width)],
            // Padding after each value and row.
            [b(item + 2), b((item + 2) * width + 8)],
            // Rows padded to 256 bytes.
            [a, b(256.max(item * width))],
            // Bottom-up rows; right-to-left pixels, with the y stride derived from them.
            [a, b(-(item * width))],
            [b(-item), a],
            // Planar strides may be 0: a whole row, or the whole image, reads one value.
            [b(0), a],
            [b(0), b(0)],
            // An x stride smaller than a value: the values overlap.
            [b(1), a],
        ];
        for strides in strides {
            out.push(spec(width, height, Some((depth, strides))));
        }
    }
    out
}

/// The port's getters of every layout, with 3 and 4 planes, separate and in one buffer, equal
/// the wheel's, or both raise the same message.
#[test]
fn planar_getters_match_the_wheel() {
    let mut requests = Vec::new();
    for (w, h) in [(1, 1), (3, 2), (17, 3)] {
        for layout in layouts(w, h) {
            for planes in [3, 4] {
                for one_buffer in [false, true] {
                    requests.push(construct(&layout, planes, one_buffer));
                }
            }
        }
    }
    let calls: Vec<_> = requests.iter().map(|(request, _)| request.call()).collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((request, spec), response) in requests.iter().zip(responses) {
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{spec:?}: {e}")));
        let buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
        let port = port_planar(spec, &buffers)
            .map(|desc| getters(&desc))
            .map_err(|e| e.message().to_string());
        let wheel = match reply.raised() {
            Some(raised) => Err(raised.message),
            None => Ok(reply.getters(0).clone()),
        };
        if port != wheel {
            failures.push(format!("{spec:?}\n  wheel {wheel:?}\n  port  {port:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} layouts differ:\n{}",
        failures.len(),
        requests.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}

/// The wheel's exception for a construct-only request, which must have raised while
/// constructing its image.
fn wheel_raised(spec: &Planar, reply: &Reply) -> Raised {
    let raised = reply
        .raised()
        .unwrap_or_else(|| panic!("{spec:?}: the wheel built it: {}", reply.getters(0)));
    assert_eq!(raised.stage, "image", "{spec:?}: {raised:?}");
    raised
}

/// Every library error of `PlanarImageDesc` that Python reaches, with the wheel's message: the
/// port raises the same one.
#[test]
fn planar_errors_match_the_wheel() {
    let b = Stride::Bytes;
    let a = Stride::Auto;
    let cases = [
        // Invalid image dimensions.
        spec(0, 2, None),
        spec(2, 0, Some((Depth::Uint16, [a, a]))),
        // Invalid y stride: the derived y stride is AutoStride, after an overflow, and exactly.
        spec(2, 2, Some((Depth::F32, [b(1 << 62), a]))),
        spec(2, 2, Some((Depth::F32, [b(-(1 << 62)), a]))),
        // The x and y strides are inconsistent.
        spec(3, 2, Some((Depth::F32, [b(4), b(11)]))),
        spec(3, 2, Some((Depth::Uint8, [b(-4), b(-11)]))),
    ];
    let mut failures = Vec::new();
    for case in &cases {
        for planes in [3, 4] {
            let (request, spec) = construct(case, planes, false);
            let reply = request.run();
            let raised = wheel_raised(&spec, &reply);
            assert_eq!(raised.kind, "Exception", "{spec:?}: {raised:?}");
            let buffers: Vec<Vec<u8>> = request.buffers.iter().map(Buffer::bytes).collect();
            match port_planar(&spec, &buffers) {
                Err(e) if e.message() == raised.message => {}
                other => failures.push(format!(
                    "{spec:?}\n  wheel {:?}\n  port  {:?}",
                    raised.message,
                    other.map(|_| ()).map_err(|e| e.message().to_string())
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Planes of the wrong channel type: the Python binding refuses a plane of the wrong numpy
/// type before the library sees it, and the port refuses typed slices of that type with the
/// binding's message, the expected type named as the wheel names a received one (improvement
/// candidate I-23), as in `image_desc_oracle.rs`.
#[test]
fn typed_planes_of_the_wrong_type_match_the_bindings_message() {
    let types = ["uint8", "uint16", "float16", "float32"];
    let mut cases = Vec::new();
    for depth in DEPTHS {
        for dtype in types {
            if dtype == ocio_testkit::image::dtype(depth) {
                continue;
            }
            let mut request = Request::new(json!({}));
            let planes = (0..3)
                .map(|_| Data::at(request.buffer(Buffer::fill(64, &[0])), 0).dtype(dtype))
                .collect();
            let spec = Planar::new(planes, 2, 1).layout(depth, [Stride::Auto; 2]);
            request.image(spec.clone());
            let reply = request.run();
            let raised = wheel_raised(&spec, &reply);
            assert_eq!(raised.kind, "RuntimeError", "{spec:?}: {raised:?}");
            cases.push((depth, dtype, raised.message));
        }
    }
    // How the wheel names each numpy type it receives.
    let received = |message: &str| {
        message
            .rsplit_once(", but received ")
            .unwrap_or_else(|| panic!("no received type in {message:?}"))
            .1
            .to_string()
    };
    let names: HashMap<&str, String> = cases
        .iter()
        .map(|(_, dtype, message)| (*dtype, received(message)))
        .collect();

    let mut failures = Vec::new();
    for (depth, dtype, message) in &cases {
        let expected = format!(
            "Incompatible buffer format: expected {}, but received {}",
            names[ocio_testkit::image::dtype(*depth)],
            received(message)
        );
        match typed_planar(dtype, *depth) {
            Err(e) if e.message() == expected => {}
            other => failures.push(format!(
                "{depth:?} as {dtype}\n  wheel {message:?}\n  port  {:?}",
                other.map_err(|e| e.message().to_string())
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The port's description of 2 by 1 pixels of `depth` over three typed slices of numpy type
/// `dtype`.
fn typed_planar(dtype: &str, depth: Depth) -> Result<(), Exception> {
    let bit_depth = common::image::port_depth(depth);
    fn build<T: Copy>(value: T, bit_depth: BitDepth) -> Result<(), Exception>
    where
        for<'a> &'a [T]: PixelData,
    {
        let plane = [value; 2];
        let desc = PlanarImageDesc::with_strides(
            &plane[..],
            &plane[..],
            &plane[..],
            None,
            2,
            1,
            bit_depth,
            AUTO_STRIDE,
            AUTO_STRIDE,
        );
        desc.map(|_| ())
    }
    match dtype {
        "uint8" => build(0u8, bit_depth),
        "uint16" => build(0u16, bit_depth),
        "float16" => build(half::f16::ZERO, bit_depth),
        "float32" => build(0f32, bit_depth),
        other => panic!("no slice type for {other}"),
    }
}
