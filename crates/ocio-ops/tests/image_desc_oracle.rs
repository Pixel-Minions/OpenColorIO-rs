// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Packed image descriptions against the wheel, through the oracle's `image_apply` with no
//! apply: the wheel constructs each description, and returns its getters or what it raised.
//!
//! - Packed images of every channel count and order, bit depth and constructor, with default,
//!   tight, padded, negative and misaligned strides: the port's getters equal the wheel's.
//! - Every error the library raises that Python reaches, with its message; where the Python
//!   binding raises first, the binding's message for the port's typed slices, with the
//!   expected type named cleanly (improvement candidate I-23).
//! - "Invalid x stride." (docs/improvements.md, I-2): the Windows wheel raises it; the Linux
//!   wheel builds the image, which the port refuses as reaching outside its buffer (D-2).
//!
//! The wheel's image objects alias their first entry (`oracle/ocio_oracle/image.py`,
//! `_vehicle`), so they reach no byte past it, whatever the strides. The port's descriptions
//! get a buffer that holds every pixel of the layout, with the first pixel in its middle, so
//! that its bounds check (D-2) never refuses a layout the wheel builds.

mod common;

use std::collections::HashMap;

use common::image::{
    DEPTHS, channel_count, extent, long_product, packed_getters, packed_reach, port_packed,
    span_union,
};
use ocio_ops::Exception;
use ocio_ops::image_desc::{At, Bytes, PackedImageDesc, PixelData};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{
    Buffer, ChannelOrder, Channels, Data, Packed, Raised, Reply, Request, RgbInput, RgbRequest,
    Stride, channel_bytes,
};
use ocio_testkit::probe::Rng;
use serde_json::json;

/// A request that constructs `spec` over a buffer holding every pixel, the first one in its
/// middle; the spec as the request has it; and the first pixel's offset. The buffer is at most
/// 1 MiB, enough for every layout the tests build.
fn construct(spec: &Packed) -> (Request, Packed, usize) {
    let reach = packed_reach(spec).min(1 << 19) as usize;
    let mut request = Request::new(json!({}));
    let buffer = request.buffer(Buffer::fill(2 * reach + 64, &[0]));
    let mut spec = spec.clone();
    spec.data = Data::at(buffer, reach + 32);
    request.image(spec.clone());
    (request, spec, reach + 32)
}

/// Every layout the getters test uses, of `width` by `height` pixels.
fn layouts(width: i64, height: i64) -> Vec<Packed> {
    let mut channel_specs = vec![Channels::Count(3), Channels::Count(4)];
    channel_specs.extend(ChannelOrder::ALL.map(Channels::Order));

    let mut out = Vec::new();
    for channels in channel_specs {
        let packed = |layout| Packed {
            layout,
            ..Packed::new(Data::at(0, 0), width, height, channels)
        };
        // The constructors without a bit depth: F32 and every stride derived.
        out.push(packed(None));
        let n = channel_count(&packed(None));
        for depth in DEPTHS {
            let item = channel_bytes(depth) as i64;
            let b = Stride::Bytes;
            let a = Stride::Auto;
            let strides = [
                [a, a, a],
                // Tight, given.
                [b(item), b(item * n), b(item * n * width)],
                // Padding after each channel, pixel and row.
                [
                    b(item + 2),
                    b((item + 2) * n + 4),
                    b(((item + 2) * n + 4) * width + 8),
                ],
                // Rows padded to 256 bytes, as a GPU readback has them.
                [a, a, b(256.max(item * n * width))],
                // Bottom-up rows, right-to-left pixels, channels backwards.
                [a, a, b(-(item * n * width))],
                [a, b(-(item * n)), a],
                [b(-item), b(item * n), a],
                // An x stride that isn't a whole number of channels.
                [b(item), b(item * n + 1), a],
                // Four channels' worth of x stride, whatever the image has.
                [b(item), b(item * 4), a],
            ];
            for strides in strides {
                out.push(packed(Some((depth.into(), strides))));
            }
            // An x stride 2^32 bytes more than 4 channels passes `isRGBAPacked`, which casts it
            // to `int` (docs/improvements.md, I-3). One pixel, so that no buffer needs 4 GiB.
            if (width, height) == (1, 1) {
                let x = (1i64 << 32) + 4 * item;
                out.push(packed(Some((depth.into(), [b(item), b(x), b(x)]))));
            }
        }
    }
    out
}

/// The port's getters of every layout equal the wheel's, or both raise the same message.
#[test]
fn packed_getters_match_the_wheel() {
    let specs: Vec<Packed> = [(1, 1), (3, 2), (17, 3)]
        .into_iter()
        .flat_map(|(w, h)| layouts(w, h))
        .collect();
    let requests: Vec<(Request, Packed, usize)> = specs.iter().map(construct).collect();
    let calls: Vec<_> = requests
        .iter()
        .map(|(request, ..)| request.call())
        .collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((request, spec, origin), response) in requests.iter().zip(responses) {
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{spec:?}: {e}")));
        let mut bytes = request.buffers[0].bytes();
        let port = port_packed(spec, At(Bytes(&mut bytes[..]), *origin));
        let wheel = match reply.raised() {
            Some(raised) => Err(raised.message),
            None => Ok(reply.getters(0).clone()),
        };
        let port = port
            .map(|desc| packed_getters(&desc))
            .map_err(|e| e.message().to_string());
        if port != wheel {
            failures.push(format!("{spec:?}\n  wheel {wheel:?}\n  port  {port:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} layouts differ:\n{}",
        failures.len(),
        specs.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}

/// The wheel's exception for a construct-only request, which must have raised while
/// constructing its image.
fn wheel_raised(spec: &Packed, reply: &Reply) -> Raised {
    let raised = reply
        .raised()
        .unwrap_or_else(|| panic!("{spec:?}: the wheel built it: {}", reply.getters(0)));
    assert_eq!(raised.stage, "image", "{spec:?}: {raised:?}");
    raised
}

/// A packed spec of `width` by `height` pixels with `channels`, and a bit depth and strides.
fn spec(
    width: i64,
    height: i64,
    channels: Channels,
    layout: Option<(Depth, [Stride; 3])>,
) -> Packed {
    Packed {
        layout: layout.map(|(depth, strides)| (depth.into(), strides)),
        ..Packed::new(Data::at(0, 0), width, height, channels)
    }
}

/// Every library error of `PackedImageDesc` that Python reaches, with the wheel's message: the
/// port raises the same one. Some layouts have two faults, so that the order of the checks
/// shows.
#[test]
fn packed_errors_match_the_wheel() {
    let b = Stride::Bytes;
    let a = Stride::Auto;
    let rgba = Channels::Count(4);
    let f32_ = Depth::F32;
    let cases = [
        // Invalid number of channels, before the bit depth and the strides.
        spec(2, 2, Channels::Count(2), None),
        spec(2, 2, Channels::Count(5), Some((f32_, [b(1), a, a]))),
        spec(2, 2, Channels::Count(0), None),
        // Invalid image dimensions.
        spec(0, 2, rgba, None),
        spec(2, 0, rgba, Some((Depth::Uint8, [a, a, a]))),
        // Invalid channel stride: below the channel size, zero, negative and small.
        spec(2, 2, rgba, Some((f32_, [b(3), a, a]))),
        spec(2, 2, rgba, Some((f32_, [b(0), a, a]))),
        spec(2, 2, rgba, Some((f32_, [b(-3), a, a]))),
        spec(
            2,
            2,
            Channels::Count(3),
            Some((Depth::Uint16, [b(1), a, a])),
        ),
        // The channel and x strides are inconsistent.
        spec(2, 2, rgba, Some((f32_, [b(4), b(12), a]))),
        spec(2, 2, rgba, Some((f32_, [b(-4), b(-15), a]))),
        // The x and y strides are inconsistent.
        spec(3, 2, rgba, Some((f32_, [a, b(16), b(47)]))),
        spec(3, 2, rgba, Some((f32_, [a, b(-16), b(-47)]))),
        // Invalid y stride: the derived y stride is AutoStride, after an overflow, and exactly.
        spec(2, 2, rgba, Some((f32_, [a, b(1 << 62), a]))),
        spec(2, 2, rgba, Some((f32_, [a, b(-(1 << 62)), a]))),
    ];
    let requests: Vec<(Request, Packed, usize)> = cases.iter().map(construct).collect();
    let calls: Vec<_> = requests
        .iter()
        .map(|(request, ..)| request.call())
        .collect();
    let responses = Oracle::get().batch(&calls, true);

    let mut failures = Vec::new();
    for ((request, spec, origin), response) in requests.iter().zip(responses) {
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{spec:?}: {e}")));
        let raised = wheel_raised(spec, &reply);
        assert_eq!(raised.kind, "Exception", "{spec:?}: {raised:?}");
        let mut bytes = request.buffers[0].bytes();
        match port_packed(spec, At(Bytes(&mut bytes[..]), *origin)) {
            Err(e) if e.message() == raised.message => {}
            other => failures.push(format!(
                "{spec:?}\n  wheel {:?}\n  port  {:?}",
                raised.message,
                other.map(|_| ()).map_err(|e| e.message().to_string())
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// "Invalid x stride." (docs/improvements.md, I-2): a channel stride of ±2^61 with 4 channels
/// derives an x stride of INT64_MIN, `AutoStride`. The Windows wheel refuses it; the Linux wheel
/// builds the image, and the port refuses it there as reaching outside its buffer (D-2), with
/// any other error the Linux wheel raises first.
#[test]
fn invalid_x_stride_is_windows_only() {
    let b = Stride::Bytes;
    let a = Stride::Auto;
    let rgba = Channels::Count(4);
    let cases = [
        spec(1, 1, rgba, Some((Depth::F32, [b(1 << 61), a, a]))),
        spec(1, 1, rgba, Some((Depth::F32, [b(-(1 << 61)), a, a]))),
        spec(1, 1, rgba, Some((Depth::F32, [b(1 << 61), a, b(64)]))),
        spec(2, 1, rgba, Some((Depth::F32, [b(1 << 61), a, a]))),
        spec(2, 2, rgba, Some((Depth::Uint8, [b(-(1 << 61)), a, b(0)]))),
    ];
    // The port's buffer can't reach 2^61 bytes: a small one will do, as every one of these is
    // refused before or by the bounds check.
    let outside =
        "PackedImageDesc Error: The strides and dimensions reach outside the image buffer.";
    let mut built = 0;
    let mut failures = Vec::new();
    for spec in &cases {
        let mut request = Request::new(json!({}));
        let buffer = request.buffer(Buffer::fill(64, &[0]));
        let mut spec = spec.clone();
        spec.data = Data::at(buffer, 0);
        request.image(spec.clone());
        let reply = request.run();

        let mut bytes = request.buffers[0].bytes();
        let port = port_packed(&spec, Bytes(&mut bytes[..]))
            .map(|_| ())
            .map_err(|e| e.message().to_string());
        let expected = match reply.raised() {
            Some(raised) => raised.message,
            None => {
                // The Linux wheel built it, with the overflowed x stride.
                built += 1;
                let x_stride = &reply.getters(0)["getXStrideBytes"];
                assert_eq!(x_stride, &json!(i64::MIN), "{spec:?}");
                outside.to_string()
            }
        };
        if port != Err(expected.clone()) {
            failures.push(format!(
                "{spec:?}\n  expected {expected:?}\n  port     {port:?}"
            ));
        }
        if cfg!(target_os = "windows") {
            assert_eq!(
                expected, "PackedImageDesc Error: Invalid x stride.",
                "{spec:?}"
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Windows refuses every one; Linux builds some of them, and the others fail later checks.
    if cfg!(target_os = "windows") {
        assert_eq!(built, 0);
    } else {
        assert!(built > 0, "the Linux wheel built none of them");
    }
}

/// A typed slice of the wrong channel type: the Python binding refuses a buffer of the wrong
/// numpy type before the library sees it, with "Incompatible buffer format: expected X, but
/// received Y". The port refuses the slice with the same message, except that it names an
/// expected unsigned type as the binding names a received one: `uint16`, where the binding
/// writes `'u' (16-bit)` (improvement candidate I-23). Both names come from the wheel's
/// messages.
#[test]
fn typed_slices_of_the_wrong_type_match_the_bindings_message() {
    // The numpy dtype of each port slice type.
    let types = ["uint8", "uint16", "float16", "float32"];
    let mut cases = Vec::new();
    // With 2 channels, the library would also refuse the channel count: the type comes first.
    for channels in [Channels::Count(4), Channels::Count(2)] {
        for depth in DEPTHS {
            for dtype in types {
                if dtype == ocio_testkit::image::dtype(depth) {
                    continue;
                }
                let mut request = Request::new(json!({}));
                let buffer = request.buffer(Buffer::fill(64, &[0]));
                let spec = Packed::new(Data::at(buffer, 0).dtype(dtype), 2, 1, channels)
                    .layout(depth, [Stride::Auto; 3]);
                request.image(spec.clone());
                let reply = request.run();
                let raised = wheel_raised(&spec, &reply);
                assert_eq!(raised.kind, "RuntimeError", "{spec:?}: {raised:?}");
                cases.push((depth, dtype, spec, raised.message));
            }
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
        .map(|(_, dtype, _, message)| (*dtype, received(message)))
        .collect();

    let mut failures = Vec::new();
    for (depth, dtype, spec, message) in &cases {
        let expected = format!(
            "Incompatible buffer format: expected {}, but received {}",
            names[ocio_testkit::image::dtype(*depth)],
            received(message)
        );
        match typed_packed(dtype, spec, channel_bytes(*depth)) {
            Err(e) if e.message() == expected => {}
            other => failures.push(format!(
                "{depth:?} as {dtype}\n  wheel {message:?}\n  port  {:?}",
                other.map_err(|e| e.message().to_string())
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The port's description of `spec` over a typed slice of numpy type `dtype`, long enough for
/// its pixels.
fn typed_packed(dtype: &str, spec: &Packed, item: usize) -> Result<(), Exception> {
    let values = 2 * 4 * item;
    fn build<T>(values: &[T], spec: &Packed) -> Result<(), Exception>
    where
        for<'a> &'a [T]: PixelData,
    {
        port_packed(spec, values).map(|_| ())
    }
    match dtype {
        "uint8" => build(&vec![0u8; values], spec),
        "uint16" => build(&vec![0u16; values], spec),
        "float16" => build(&vec![half::f16::ZERO; values], spec),
        "float32" => build(&vec![0f32; values], spec),
        other => panic!("no slice type for {other}"),
    }
}

/// An empty buffer is C++'s null pointer: Python reaches it through `applyRGB([])`, which
/// describes an empty list as a packed F32 image 0 pixels wide.
#[test]
fn an_empty_buffer_is_invalid() {
    let reply = RgbRequest {
        processor: json!({"transform": {"class": "LogTransform"}}),
        rgba: false,
        input: RgbInput::list(&[]),
    }
    .run();
    let raised = reply.raised().expect("applyRGB([]) raises");
    assert_eq!(
        (raised.kind.as_str(), raised.stage.as_str()),
        ("Exception", "apply")
    );

    let mut empty: Vec<f32> = Vec::new();
    let port = PackedImageDesc::new(&mut empty, 0, 1, 3).map(|_| ());
    assert_eq!(
        port.map_err(|e| e.message().to_string()),
        Err(raised.message)
    );
}

/// Sizes past a C `long`, which has 32 bits on Windows, can't reach upstream: the port makes
/// them -1, an invalid size that upstream's own check refuses (docs/architecture.md, "Image
/// descriptions"), with the message the wheel gives for a width of 0. A size that truncated to
/// 32 bits would be valid, 2^32 + 1 as 1, is refused too.
#[cfg(target_os = "windows")]
#[test]
fn sizes_past_a_long_are_invalid() {
    let (request, spec, _) = construct(&spec(0, 2, Channels::Count(4), None));
    let reply = request.run();
    let raised = wheel_raised(&spec, &reply);

    let mut bytes = [0u8; 16];
    let past = (1usize << 32) + 1;
    for (width, height) in [(past, 1), (1, past), (1 << 32, 1)] {
        let port = PackedImageDesc::new(Bytes(&mut bytes[..]), width, height, 4);
        assert_eq!(
            port.map(|_| ()).map_err(|e| e.message().to_string()),
            Err(raised.message.clone()),
            "{width} by {height}"
        );
    }
}

/// The D-2 error of a packed image.
const OUTSIDE: &str =
    "PackedImageDesc Error: The strides and dimensions reach outside the image buffer.";

/// One of `items`, at random.
fn pick<T: Copy>(rng: &mut Rng, items: &[T]) -> T {
    items[(rng.next_u64() % items.len() as u64) as usize]
}

/// The review's differential fuzz of `PackedImageDesc` (verifier C, p1-bitdepth): 6,000
/// layouts, with channels of every count and order, sizes from 0 to 4 by 4, and strides small,
/// automatic and past 2^32, 2^61 and 2^62, both signs, so that several checks fail at once and
/// their order shows. Where the wheel raises, the port raises the same message. Where it builds
/// the image, the port builds it over a buffer that holds exactly its bytes, with the same
/// getters, and refuses the buffer one byte short at either end, or any buffer when the image
/// spans more than a megabyte (D-2).
#[test]
fn packed_constructor_fuzz() {
    let mut rng = Rng::new(0xC0FFEE);
    let channel_specs = [
        Channels::Count(3),
        Channels::Count(4),
        Channels::Count(2),
        Channels::Count(5),
        Channels::Order(ChannelOrder::Rgba),
        Channels::Order(ChannelOrder::Bgra),
        Channels::Order(ChannelOrder::Abgr),
        Channels::Order(ChannelOrder::Rgb),
        Channels::Order(ChannelOrder::Bgr),
    ];
    let sizes = [
        (1i64, 1i64),
        (2, 1),
        (1, 2),
        (3, 2),
        (5, 3),
        (0, 1),
        (1, 0),
        (4, 4),
    ];
    let mut cases = Vec::new();
    for _ in 0..6000 {
        let channels = pick(&mut rng, &channel_specs);
        let (w, h) = pick(&mut rng, &sizes);
        let depth = if rng.next_u64().is_multiple_of(8) {
            None
        } else {
            Some(pick(&mut rng, &DEPTHS))
        };
        let n = match channels {
            Channels::Count(n) => n,
            Channels::Order(o) => o.channels() as i64,
            _ => 4,
        };
        let item = depth.map_or(4, |d| channel_bytes(d) as i64);
        let big = [
            1i64 << 61,
            -(1i64 << 61),
            1i64 << 62,
            -(1i64 << 62),
            (1i64 << 61) + item,
            i64::MAX,
            i64::MIN + 1,
            (1i64 << 32) + 4 * item,
            (1i64 << 32) + n * item,
            -(1i64 << 32) + 4 * item,
            (1i64 << 63) / w.max(1),
            -((1i64 << 62) / w.max(1)) * 2,
        ];
        let stride = |rng: &mut Rng, small: &[i64]| -> Stride {
            match rng.next_u64() % 10 {
                0..=1 => Stride::Auto,
                2..=6 => Stride::Bytes(pick(rng, small)),
                _ => Stride::Bytes(pick(rng, &big)),
            }
        };
        let cs = stride(
            &mut rng,
            &[item, item + 1, 0, -item, 2 * item, 1, item - 1, 3],
        );
        let csv = match cs {
            Stride::Bytes(c) => c,
            Stride::Auto => item,
        };
        let xs = stride(
            &mut rng,
            &[
                n.wrapping_mul(csv),
                n.wrapping_mul(csv).wrapping_add(1),
                n.wrapping_mul(csv).wrapping_neg(),
                csv.wrapping_mul(4),
                0,
                csv.wrapping_mul(3),
                n.wrapping_mul(csv).wrapping_sub(1),
                1,
            ],
        );
        let xsv = match xs {
            Stride::Bytes(x) => x,
            Stride::Auto => csv.wrapping_mul(n),
        };
        let ys = stride(
            &mut rng,
            &[
                xsv.wrapping_mul(w),
                xsv.wrapping_mul(w).wrapping_neg(),
                xsv.wrapping_mul(w).wrapping_sub(1),
                0,
                256,
                xsv.wrapping_mul(w).wrapping_abs().wrapping_add(8),
                xsv.wrapping_mul(w).wrapping_abs().wrapping_neg(),
                1,
            ],
        );
        let mut request = Request::new(json!({}));
        let buffer = request.buffer(Buffer::fill(64, &[0]));
        let entries = long_product(w, h, n);
        let mut spec = Packed::new(Data::at(buffer, 0).entries(entries), w, h, channels);
        if let Some(d) = depth {
            spec = spec.layout(d, [cs, xs, ys]);
        }
        request.image(spec.clone());
        cases.push((spec, request));
    }
    let calls: Vec<_> = cases.iter().map(|(_, r)| r.call()).collect();
    let responses = Oracle::get().batch(&calls, true);

    let port = |spec: &Packed, bytes: &mut [u8], origin: usize| {
        port_packed(spec, At(Bytes(bytes), origin))
            .map(|_| ())
            .map_err(|e| e.message().to_string())
    };
    let mut failures = Vec::new();
    let mut built = 0;
    for ((spec, request), response) in cases.iter().zip(responses) {
        let reply = request.reply(response.unwrap_or_else(|e| panic!("{spec:?}: {e}")));
        let item = match spec.layout {
            Some((ocio_testkit::image::Depth::Supported(d), _)) => channel_bytes(d) as i64,
            _ => 4,
        };
        if let Some(raised) = reply.raised() {
            // Only the library's checks: the binding's own are Python's.
            if raised.kind != "Exception" {
                failures.push(format!("{spec:?}: the binding raised {raised:?}"));
                continue;
            }
            let mut bytes = vec![0u8; 64];
            let port = port(spec, &mut bytes, 0);
            if port != Err(raised.message.clone()) {
                failures.push(format!(
                    "{spec:?}\n  wheel raised {:?}\n  port {port:?}",
                    raised.message
                ));
            }
            continue;
        }
        let g = reply.getters(0).clone();
        let (w, h) = (spec.width, spec.height);
        let cs = g["getChanStrideBytes"].as_i64().unwrap();
        let xs = g["getXStrideBytes"].as_i64().unwrap();
        let ys = g["getYStrideBytes"].as_i64().unwrap();
        let nc = g["getNumChannels"].as_i64().unwrap();
        let (lo, hi) = if g["isRGBAPacked"].as_bool().unwrap() {
            extent(0, (1, h), (0, ys), 4 * item * w)
        } else {
            (0..nc)
                .fold(None, |span, k| {
                    let start = k.wrapping_mul(cs) as i128;
                    span_union(span, extent(start, (w, h), (xs, ys), item))
                })
                .unwrap()
        };
        let size = hi - lo;
        if size > (1 << 20) || lo < -(1 << 20) {
            let mut bytes = vec![0u8; 64];
            let port = port(spec, &mut bytes, 32);
            if port != Err(OUTSIDE.to_string()) {
                failures.push(format!(
                    "{spec:?}\n  wheel built {g}, spanning [{lo}, {hi})\n  port {port:?}"
                ));
            }
            continue;
        }
        built += 1;
        let (size, origin) = (size as usize, (-lo) as usize);
        let mut bytes = vec![0u8; size];
        match port_packed(spec, At(Bytes(&mut bytes[..]), origin)) {
            Ok(desc) => {
                let pg = packed_getters(&desc);
                if pg != g {
                    failures.push(format!("{spec:?}\n  wheel {g}\n  port  {pg}"));
                }
            }
            Err(e) => failures.push(format!(
                "{spec:?}\n  wheel built {g} ({size} bytes from {origin})\n  port refused {:?}",
                e.message()
            )),
        }
        if port(spec, &mut bytes[..size - 1], origin) != Err(OUTSIDE.to_string()) {
            failures.push(format!("{spec:?}: one byte short at the end is accepted"));
        }
        if origin > 0 && port(spec, &mut bytes[1..], origin - 1) != Err(OUTSIDE.to_string()) {
            failures.push(format!("{spec:?}: one byte short at the start is accepted"));
        }
    }
    assert!(built > 500, "only {built} layouts built");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures[..failures.len().min(25)].join("\n")
    );
}
