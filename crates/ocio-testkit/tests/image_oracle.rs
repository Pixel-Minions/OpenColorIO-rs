// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's image commands (`oracle/ocio_oracle/image.py`, chunk O1.2) against the wheel
//! itself: the same pixels in every layout PyOpenColorIO can describe, in place or not; no
//! write outside the pixels; `applyRGB` and `applyRGBA` as `apply` on one row; the data
//! getters; the command's refusals; and every error path Python reaches, listed below.
//!
//! # Which checks are whose
//!
//! PyOpenColorIO checks buffers itself before the library sees them. The port's core gives the
//! library's messages, and `ocio-py` the binding's. Where the binding blocks a library path, the
//! port's tests of that path rely on upstream's C++ tests. Paths are relative to
//! `upstream/OpenColorIO/src` @ v2.5.2.
//!
//! **The binding** (`bindings/python/`), in the order it checks:
//!
//! | Check | Message | Raised by |
//! |---|---|---|
//! | pybind11 overloads | `__init__(): incompatible constructor arguments ...` (`TypeError`) | a width or height beyond a C `long` (32 bits on Windows), a stride beyond `ptrdiff_t` |
//! | overload order, PyPackedImageDesc.cpp:15-100 | none | a `ChannelOrdering` passed positionally is taken as `numChannels`: `PackedImageDesc(buf, w, h, CHANNEL_ORDERING_BGR)` is a 4-channel RGBA image. Only keyword calls reach the `chanOrder` constructors |
//! | `bitDepthToDtype`, PyUtils.cpp:56-86 | `Error: Unsupported bit-depth: 14ui` (OCIO `Exception`) | the constructors with a bit depth, for UINT14, UINT32, UNKNOWN or a value outside the enum, before any buffer check |
//! | `checkBufferType`, PyUtils.cpp:165-175 | `Incompatible buffer format: expected float32, but received float64` (`RuntimeError`); an expected integer type prints as `'u' (16-bit)` | every constructor; planes are checked last to first (B, then G, then R; A first when given), each for its type then its size, on both platforms |
//! | `chanOrderToNumChannels`, PyUtils.cpp:113-127 | `Error: Unsupported channel ordering` (OCIO `Exception`) | the `chanOrder` constructors, for a value outside the enum |
//! | `checkBufferSize`, PyUtils.cpp:214-223 | `Incompatible buffer dimensions: expected 16 entries, but received 15 entries` (`RuntimeError`) | every constructor: exactly width * height * channels entries (packed) or width * height (a plane), whatever the strides. The product is a C `long`: on Windows it wraps at 32 bits, so 65536 x 65536 x 4 expects 0 entries there |
//! | `checkBufferDivisible`, PyUtils.cpp:203-212 | `Incompatible buffer dimensions: expected size to be divisible by 3, but received 5 entries` (`RuntimeError`) | `applyRGB`, `applyRGBA` on an array |
//! | `checkCContiguousArray`, PyUtils.cpp:182-201 | `function only supports C-contiguous (row-major) arrays` (`RuntimeError`) | the same |
//! | `getBufferBitDepth`, PyUtils.cpp:141-163 | `Unsupported data type: float64` (`RuntimeError`) | the same; uint16 arrays are always `BIT_DEPTH_UINT16` |
//! | `checkVectorDivisible`, PyUtils.cpp:252-261 | `Incompatible vector dimensions: expected (N*3, 1), but received (2, 1)` (`RuntimeError`) | `applyRGB`, `applyRGBA` on a list |
//!
//! The binding requests buffers read-only but lets the library write them: `applyRGB` on a
//! read-only numpy array changes it. Its `repr()` prints the address of its `shared_ptr`
//! (PyImageDesc.cpp:20-25), so the oracle doesn't report it.
//!
//! **The library**, per error path:
//!
//! | Where | Message | Through Python |
//! |---|---|---|
//! | ImageDesc.cpp:95 `GenericImageDesc::init` | `Bit-depth mismatch between the image buffer and the finalization setting.` | yes: `apply` with an image whose bit depth isn't the processor's |
//! | ImageDesc.cpp:184 `initValues` | `PackedImageDesc Error: Unknown channel ordering.` | no: `chanOrderToNumChannels` raises first |
//! | ImageDesc.cpp:249-252 `isRGBAPacked` | `PackedImageDesc Error: Unsupported bit-depth: ...` | no, nor from C++: `GetChannelSizeInBytes` raises first |
//! | ImageDesc.cpp:286 `validate` | `PackedImageDesc Error: Invalid image buffer.` | only through `applyRGB([])` or `applyRGBA([])`: an empty list gives a null pointer. Python buffers are never null |
//! | ImageDesc.cpp:291 `validate` | `PackedImageDesc Error: Invalid image dimensions.` | yes: a width or height of 0 (an empty buffer), or both negative |
//! | ImageDesc.cpp:297 `validate` | `PackedImageDesc Error: Invalid channel stride.` | yes: a channel stride below the channel size. The `AutoStride` half of the test is dead: the constructors resolve it |
//! | ImageDesc.cpp:302 `validate` | `PackedImageDesc Error: Invalid channel number.` | no, nor from C++: the constructors raise `Invalid number of channels.` first |
//! | ImageDesc.cpp:307 `validate` | `PackedImageDesc Error: The channel and x strides are inconsistent.` | yes |
//! | ImageDesc.cpp:312 `validate` | `PackedImageDesc Error: Invalid x stride.` | Windows only: a channel stride of ±2^61 with 4 channels makes the derived x stride overflow to `AutoStride` (`INT64_MIN`), whatever the width. On Linux GCC dropped the check, since `std::abs` of the same value is evaluated before it (undefined for `INT64_MIN`). There an odd width fails the y stride check (:317) instead, as the derived y stride, `INT64_MIN` times the width, is `AutoStride` too, and an even width builds, with an x stride of `INT64_MIN` and a y stride of 0 |
//! | ImageDesc.cpp:317 `validate` | `PackedImageDesc Error: Invalid y stride.` | yes: an x stride of ±2^62 with a width of 2 makes the derived y stride overflow to `AutoStride` |
//! | ImageDesc.cpp:322 `validate` | `PackedImageDesc Error: The x and y strides are inconsistent.` | yes |
//! | ImageDesc.cpp:327 `validate` | `PackedImageDesc Error: Unknown bit-depth of the image buffer.` | no, nor from C++: `GetChannelSizeInBytes` raises first |
//! | ImageDesc.cpp:354, 489 constructors | `PackedImageDesc Error: Invalid number of channels.` | yes: `numChannels` other than 3 or 4 |
//! | ImageDesc.cpp:396, 442 constructors | `PackedImageDesc Error: Unknown channel ordering.` | no: `chanOrderToNumChannels` raises first |
//! | BitDepthUtils.cpp:90-117 `IsFloatBitDepth` | `Bit depth is not supported: 14ui.` | yes: `getOptimizedCPUProcessor` with UINT14, UINT32 or UNKNOWN in or out, and ops left after optimizing: `optimizeForBitdepth` asks it about both depths (OpOptimizers.cpp:764, 768) |
//! | BitDepthUtils.cpp:18-44 `GetBitDepthMaxValue`, :121-148 `GetChannelSizeInBytes` | the same | no: `bitDepthToDtype` raises first for a description, and `IsFloatBitDepth` or CPUProcessor.cpp:93, 113 for a processor |
//! | CPUProcessor.cpp:93, 113 `CreateGenericBitDepthHelper` | `Unsupported bit-depth` | yes: `getOptimizedCPUProcessor` with UINT14, UINT32 or UNKNOWN in (:113) or out (:93), and no op left (an empty group, or ops the optimizer removes). `optimizeForBitdepth` skips an empty list (OpOptimizers.cpp:762), and the CPU engine casts from or to the identity matrix it adds |
//! | CPUProcessor.cpp:119, 215, 233, 239 | `Unsupported bit-depths`, `Unsupported bit-depth` | no, nor from C++: no CPU processor has those bit depths, and :119 and :239 follow switches whose cases all return or raise |
//! | ImageDesc.cpp:618 `validate` | `PlanarImageDesc Error: Invalid x stride.` | no, nor from C++: `AutoStride` becomes the channel size |
//! | ImageDesc.cpp:623 `validate` | `PlanarImageDesc Error: Invalid y stride.` | yes: as for packed images |
//! | ImageDesc.cpp:628 `validate` | `PlanarImageDesc Error: The x and y strides are inconsistent.` | yes |
//! | ImageDesc.cpp:633 `validate` | `PlanarImageDesc Error: Unknown bit-depth of the image buffer.` | no, nor from C++: `GetChannelSizeInBytes` raises first |
//! | ImageDesc.cpp:645, 681 constructors | `PlanarImageDesc Error: Invalid image buffer.` | no: Python buffers are never null |
//! | ImageDesc.cpp:650, 686 constructors | `PlanarImageDesc Error: Invalid image dimensions.` | yes |
//! | ImagePacking.cpp:30, 100, 170, 240 | `Invalid output image buffer` (with a period at :100 only), `Invalid input image buffer` | no: `ScanlineHelper` passes its own row buffers. On Windows a width of 2^30 leaves them empty, as 4 * width overflows a 32-bit C long (ScanlineHelper.cpp:72-108), and `&buffer[0]` of an empty vector is null: undefined behavior both, which reaches :100 (probed outside the oracle). The oracle refuses widths from 2^29 |
//! | ImagePacking.cpp:39, 109 | `Invalid output image position.` | Windows only: an image that isn't RGBA-packed, whose pixel count overflows the 32-bit C long it is computed in (:35, :105), as does each row's first pixel index, y * width (ScanlineHelper.cpp:146). The apply raises at the first row whose index falls outside the wrapped count, after writing the rows before it: 65536 x 65536 (a count of 0) at row 0, 65536 x 65537 at row 1. With a 64-bit long it takes sizes the oracle refuses |
//! | ScanlineHelper.cpp:60 `init` | `Dimension inconsistency between source and destination image buffers.` | yes |
//!
//! Python's `applyRGB` and `applyRGBA` never reach the C++ `CPUProcessor::applyRGB(float *)` and
//! `applyRGBA(float *)`: they wrap the values in a `PackedImageDesc` and call `apply`
//! (PyCPUProcessor.cpp:94-249). Those two rely on upstream's C++ tests.

use std::collections::HashMap;

use ocio_testkit::battery::BitDepth;
use ocio_testkit::image::{
    Buffer, ChannelOrder, Channels, Data, Depth, Footprint, Image, Packed, Planar, Raised, Reply,
    Request, RgbInput, RgbReply, RgbRequest, Stride, channel_bytes, dtype,
};
use ocio_testkit::oracle::{BatchCall, Response, f32_to_bytes};
use ocio_testkit::probe::{self, Rng};
use ocio_testkit::{Oracle, assert_bytes_eq};
use serde_json::{Value, json};

/// Two ops, so that the CPU engine has a first and a last op to put the bit-depth conversions
/// at (CPUProcessor.cpp:122-184), without crosstalk between colour and alpha: the colours of an
/// RGB image don't depend on the alpha of 0 the library gives it (ImagePacking.cpp:73, 143).
fn processor(input: BitDepth, output: BitDepth) -> Value {
    let mut args = json!({"transform": {"class": "GroupTransform", "children": [
        {"class": "MatrixTransform", "args": {"offset": [0.125, -0.25, 0.0625, 0.5]}},
        {"class": "LogTransform", "args": {"base": 2.0}},
    ]}});
    if (input, output) != (BitDepth::F32, BitDepth::F32) {
        args["in_bitdepth"] = json!(input.oracle_name());
        args["out_bitdepth"] = json!(output.oracle_name());
    }
    args
}

/// `count` RGBA pixels of `depth`, in its storage type's little-endian bytes: specials and
/// seeded values for F32, seeded bit patterns for F16, seeded codes for the integers. The 10-
/// and 12-bit codes stay within the bit depth: the forward 1D LUT that starts these processors
/// at those depths would look larger color codes up outside its tables, and the oracle refuses
/// them.
fn pixels(depth: BitDepth, count: usize, seed: u64) -> Vec<u8> {
    let specials = probe::specials();
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for k in 0..count * 4 {
        let bits = rng.next_u64();
        match depth {
            BitDepth::F32 => {
                let value = if k % 2 == 0 {
                    specials[(k / 2 + seed as usize) % specials.len()]
                } else {
                    rng.uniform(-0.5, 2.0)
                };
                out.extend_from_slice(&value.to_le_bytes());
            }
            BitDepth::F16 | BitDepth::Uint16 => out.extend_from_slice(&(bits as u16).to_le_bytes()),
            BitDepth::Uint10 => out.extend_from_slice(&(bits as u16 & 0x3ff).to_le_bytes()),
            BitDepth::Uint12 => out.extend_from_slice(&(bits as u16 & 0xfff).to_le_bytes()),
            BitDepth::Uint8 => out.push(bits as u8),
        }
    }
    out
}

/// What the padding of a source buffer holds.
const PAD: [u8; 3] = [0x77, 0x11, 0xee];
/// What a destination buffer holds before the call.
const PREFILL: [u8; 5] = [0xa5, 0x5a, 0xc3, 0x3c, 0x96];

/// Which constructors describe a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    /// The constructors without strides for F32; for the other bit depths, those with strides,
    /// all `AutoStride`.
    Plain,
    /// The constructors with strides, all `AutoStride`.
    Auto,
    /// The constructors with strides, all given.
    Explicit,
}

/// How a test lays out an image of any size and bit depth.
#[derive(Debug, Clone, Copy)]
struct Layout {
    name: &'static str,
    /// R, G, B (and A) planes rather than packed pixels.
    planar: bool,
    /// A packed image's channel order; for a planar image, RGBA or RGB.
    order: ChannelOrder,
    /// A packed image described with `numChannels` rather than `chanOrder`.
    count: bool,
    style: Style,
    /// Padding, in channels: between a pixel's channels (packed), after each pixel, after each
    /// row, before the image, and after it (and after each plane).
    pad: [usize; 5],
    /// Pixels from right to left: a negative x stride.
    flip_x: bool,
    /// Rows from bottom to top: a negative y stride.
    flip_y: bool,
    /// A packed pixel's channels from last to first: a negative channel stride.
    flip_chan: bool,
    /// A planar image's planes in one buffer.
    one_buffer: bool,
}

const PACKED: Layout = Layout {
    name: "",
    planar: false,
    order: ChannelOrder::Rgba,
    count: true,
    style: Style::Plain,
    pad: [0; 5],
    flip_x: false,
    flip_y: false,
    flip_chan: false,
    one_buffer: false,
};

const PLANAR: Layout = Layout {
    planar: true,
    count: false,
    ..PACKED
};

/// Every layout the test uses; the first is the reference.
fn layouts() -> Vec<Layout> {
    use ChannelOrder::{Abgr, Bgr, Bgra, Rgb};
    use Style::{Auto, Explicit};
    let mut layouts = vec![
        Layout {
            name: "numChannels 4",
            ..PACKED
        },
        Layout {
            name: "numChannels 3",
            order: Rgb,
            ..PACKED
        },
    ];
    for order in ChannelOrder::ALL {
        layouts.push(Layout {
            name: order.oracle_name(),
            order,
            count: false,
            ..PACKED
        });
    }
    layouts.extend([
        Layout {
            name: "numChannels 4, AutoStride",
            style: Auto,
            ..PACKED
        },
        Layout {
            name: "BGR, AutoStride",
            order: Bgr,
            count: false,
            style: Auto,
            ..PACKED
        },
        Layout {
            name: "numChannels 4, explicit strides",
            style: Explicit,
            ..PACKED
        },
        Layout {
            name: "BGRA, padded",
            order: Bgra,
            count: false,
            style: Explicit,
            pad: [1, 1, 3, 2, 3],
            ..PACKED
        },
        Layout {
            name: "numChannels 3, padded",
            order: Rgb,
            style: Explicit,
            pad: [0, 2, 1, 1, 1],
            ..PACKED
        },
        Layout {
            name: "ABGR, rows flipped",
            order: Abgr,
            count: false,
            style: Explicit,
            pad: [0, 0, 2, 1, 1],
            flip_y: true,
            ..PACKED
        },
        Layout {
            name: "numChannels 4, pixels flipped",
            style: Explicit,
            flip_x: true,
            ..PACKED
        },
        Layout {
            name: "BGR, padded, both flipped",
            order: Bgr,
            count: false,
            style: Explicit,
            pad: [1, 1, 1, 1, 1],
            flip_x: true,
            flip_y: true,
            ..PACKED
        },
        Layout {
            name: "planar RGBA",
            ..PLANAR
        },
        Layout {
            name: "planar RGB",
            order: Rgb,
            ..PLANAR
        },
        Layout {
            name: "planar RGBA, AutoStride",
            style: Auto,
            ..PLANAR
        },
        Layout {
            name: "planar RGBA, one buffer, padded",
            style: Explicit,
            pad: [0, 1, 2, 1, 2],
            one_buffer: true,
            ..PLANAR
        },
        Layout {
            name: "planar RGB, padded, rows flipped",
            order: Rgb,
            style: Explicit,
            pad: [0, 2, 1, 0, 1],
            flip_y: true,
            ..PLANAR
        },
        Layout {
            name: "planar RGBA, pixels flipped",
            style: Explicit,
            flip_x: true,
            ..PLANAR
        },
        Layout {
            name: "numChannels 4, padded, channels flipped",
            style: Explicit,
            pad: [1, 0, 1, 1, 1],
            flip_chan: true,
            ..PACKED
        },
    ]);
    layouts
}

/// Source and destination layouts that differ, by index into [`layouts`].
const CROSS: [(usize, usize); 6] = [(0, 15), (15, 10), (1, 0), (12, 16), (18, 4), (20, 11)];

/// What an image's buffers hold before the call.
#[derive(Debug, Clone, Copy)]
enum Content<'a> {
    /// These RGBA pixels, and [`PAD`] around them.
    Pixels(&'a [u8]),
    /// [`PREFILL`].
    Prefill,
}

/// Adds the buffers and the image of `layout` to `request`, holding `content`, and returns the
/// image's index and footprint.
fn add(
    request: &mut Request,
    layout: &Layout,
    depth: BitDepth,
    (width, height): (usize, usize),
    content: Content<'_>,
) -> (usize, Footprint) {
    assert!(
        layout.style == Style::Explicit
            || (layout.pad == [0; 5]
                && !layout.flip_x
                && !layout.flip_y
                && !layout.flip_chan
                && !layout.one_buffer),
        "{}: only explicit strides can pad or flip",
        layout.name
    );
    let item = channel_bytes(depth);
    let [pad_chan, pad_pixel, pad_row, head, tail] = layout.pad.map(|p| p * item);
    let channels = layout.order.channels();
    let chan = item + pad_chan;
    let x = if layout.planar {
        item + pad_pixel
    } else {
        channels * chan + pad_pixel
    };
    let y = width * x + pad_row;
    // One packed image, or one plane, from its first byte to its last.
    let extent = (height - 1) * y
        + (width - 1) * x
        + if layout.planar {
            item
        } else {
            (channels - 1) * chan + item
        };
    let start = head
        + if layout.flip_x { (width - 1) * x } else { 0 }
        + if layout.flip_y { (height - 1) * y } else { 0 }
        + if layout.flip_chan {
            (channels - 1) * chan
        } else {
            0
        };
    let x_stride = if layout.flip_x { -(x as i64) } else { x as i64 };
    let y_stride = if layout.flip_y { -(y as i64) } else { y as i64 };
    let chan_stride = if layout.flip_chan {
        -(chan as i64)
    } else {
        chan as i64
    };

    let region = head + extent + tail;
    let planes = if layout.planar { channels } else { 1 };
    let sizes = if layout.one_buffer {
        vec![planes * region]
    } else {
        vec![region; planes]
    };
    let first = request.buffers.len();
    let data = |k: usize| {
        if layout.one_buffer {
            Data::at(first, k * region + start)
        } else {
            Data::at(first + k, start)
        }
    };
    let footprint = if layout.planar {
        let planes: Vec<(usize, i64)> = (0..planes)
            .map(|k| {
                let d = data(k);
                (d.buffer, d.offset as i64)
            })
            .collect();
        Footprint::planar(&planes, width, height, item, x_stride, y_stride)
    } else {
        let order = layout.order;
        Footprint::packed(
            first,
            start as i64,
            width,
            height,
            order,
            item,
            chan_stride,
            x_stride,
            y_stride,
        )
    };

    match content {
        Content::Pixels(pixels) => {
            // Write through a footprint of these buffers alone, then add them.
            let mut local = footprint;
            local.channels = local.channels.map(|c| c.map(|(b, at)| (b - first, at)));
            let mut buffers: Vec<Vec<u8>> = sizes
                .iter()
                .map(|&n| PAD.iter().copied().cycle().take(n).collect())
                .collect();
            local.write(&mut buffers, pixels);
            for bytes in buffers {
                request.buffer(Buffer::Bytes(bytes));
            }
        }
        Content::Prefill => {
            for n in sizes {
                request.buffer(Buffer::fill(n, &PREFILL));
            }
        }
    }

    let plain = layout.style == Style::Plain && depth == BitDepth::F32;
    let image: Image = if layout.planar {
        let planar = Planar::new((0..planes).map(data).collect(), width as i64, height as i64);
        match layout.style {
            _ if plain => planar,
            Style::Plain | Style::Auto => planar.layout(depth, [Stride::Auto; 2]),
            Style::Explicit => {
                planar.layout(depth, [Stride::Bytes(x_stride), Stride::Bytes(y_stride)])
            }
        }
        .into()
    } else {
        let channels = if layout.count {
            Channels::Count(channels as i64)
        } else {
            Channels::Order(layout.order)
        };
        let packed = Packed::new(data(0), width as i64, height as i64, channels);
        match layout.style {
            _ if plain => packed,
            Style::Plain | Style::Auto => packed.layout(depth, [Stride::Auto; 3]),
            Style::Explicit => packed.layout(
                depth,
                [
                    Stride::Bytes(chan_stride),
                    Stride::Bytes(x_stride),
                    Stride::Bytes(y_stride),
                ],
            ),
        }
        .into()
    };
    (request.image(image), footprint)
}

/// Every pair of processor bit depths the layout test runs: each depth to itself (the only
/// pairs an image can be processed in place in), and to or from others.
const PAIRS: [(BitDepth, BitDepth); 12] = {
    use BitDepth::{F16, F32, Uint8, Uint10, Uint12, Uint16};
    [
        (F32, F32),
        (F16, F16),
        (Uint8, Uint8),
        (Uint10, Uint10),
        (Uint12, Uint12),
        (Uint16, Uint16),
        (Uint8, F32),
        (F32, Uint8),
        (Uint16, F16),
        (F16, Uint12),
        (Uint10, Uint16),
        (F32, F16),
    ]
};

/// Image sizes: a single pixel, and widths below, at and past SIMD block sizes, over several
/// rows.
const SIZES: [(usize, usize); 4] = [(1, 1), (3, 2), (17, 3), (33, 2)];

/// One call of the layout test.
#[derive(Debug)]
struct Case {
    label: String,
    /// The index of the case whose output this one's must equal.
    reference: usize,
    request: Request,
    /// Where the output pixels are.
    output: Footprint,
    /// Whether the output alpha is compared: the source and the destination both have alpha.
    alpha: bool,
}

/// Where a case's output goes.
#[derive(Debug, Clone, Copy)]
enum Target {
    /// Another image, of this layout.
    Image(Layout),
    /// `apply(image)`.
    InPlace,
    /// `apply(image, image)`.
    Itself,
}

/// Every case: per bit-depth pair and size, the reference (packed RGBA to packed RGBA), then
/// every layout to itself, the cross pairs, and for equal bit depths every layout in place and
/// one image applied to itself.
fn layout_cases() -> Vec<Case> {
    let layouts = layouts();
    let mut cases = Vec::new();
    for (p, &(input, output)) in PAIRS.iter().enumerate() {
        for (s, &size) in SIZES.iter().enumerate() {
            let group = format!(
                "{} to {}, {}x{}",
                input.oracle_name(),
                output.oracle_name(),
                size.0,
                size.1
            );
            let pixels = pixels(input, size.0 * size.1, (p * SIZES.len() + s) as u64);
            let mut targets: Vec<(Layout, Target)> =
                layouts.iter().map(|&l| (l, Target::Image(l))).collect();
            targets.extend(CROSS.map(|(a, b)| (layouts[a], Target::Image(layouts[b]))));
            if input == output {
                targets.extend(layouts.iter().map(|&l| (l, Target::InPlace)));
                targets.push((layouts[10], Target::Itself));
            }
            let reference = cases.len();
            for (src, target) in targets {
                let mut request = Request::new(processor(input, output));
                let (image, footprint) =
                    add(&mut request, &src, input, size, Content::Pixels(&pixels));
                let (label, output, alpha) = match target {
                    Target::Image(dst) => {
                        let (dst_image, dst_footprint) =
                            add(&mut request, &dst, output, size, Content::Prefill);
                        request.apply = vec![image, dst_image];
                        let alpha = src.order.channels() == 4 && dst.order.channels() == 4;
                        (
                            format!("{} to {}", src.name, dst.name),
                            dst_footprint,
                            alpha,
                        )
                    }
                    Target::InPlace => {
                        request.apply = vec![image];
                        let alpha = src.order.channels() == 4;
                        (format!("{} in place", src.name), footprint, alpha)
                    }
                    Target::Itself => {
                        request.apply = vec![image, image];
                        let alpha = src.order.channels() == 4;
                        (format!("{} to itself", src.name), footprint, alpha)
                    }
                };
                cases.push(Case {
                    label: format!("{group}: {label}"),
                    reference,
                    request,
                    output,
                    alpha,
                });
            }
        }
    }
    cases
}

/// Runs every case in one oracle process.
fn run(cases: &[Case]) -> Vec<Reply> {
    let calls: Vec<BatchCall<'_>> = cases.iter().map(|c| c.request.call()).collect();
    let replies: Vec<Reply> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .zip(cases)
        .map(|(response, case)| {
            let response = response.unwrap_or_else(|e| panic!("{}: {e}", case.label));
            case.request.reply(response)
        })
        .collect();
    for (case, reply) in cases.iter().zip(&replies) {
        assert!(reply.raised().is_none(), "{}: {}", case.label, reply.result);
    }
    replies
}

/// The same processor on the same pixels gives the same values in every layout, channel order
/// and constructor, packed or planar, with any padding and flipped rows or pixels, in place, to
/// another buffer or from an image to itself, at every bit depth.
#[test]
fn every_layout_gives_the_same_pixels() {
    let cases = layout_cases();
    let replies = run(&cases);
    for (case, reply) in cases.iter().zip(&replies) {
        let reference = &cases[case.reference];
        let expected = &replies[case.reference];
        for c in 0..if case.alpha { 4 } else { 3 } {
            assert_bytes_eq(
                &format!("{}, channel {}", case.label, "RGBA".as_bytes()[c] as char),
                &reference.output.read(&expected.buffers, c).unwrap(),
                &case.output.read(&reply.buffers, c).unwrap(),
            );
        }
    }
}

/// `apply` writes the channels of the destination pixels and nothing else: the padding keeps
/// its prefill, a destination without alpha keeps it where alpha would be, and the source
/// buffers are unchanged.
#[test]
fn apply_writes_the_pixels_only() {
    let cases = layout_cases();
    let replies = run(&cases);
    let mut written_in_padded_layouts = 0;
    for (case, reply) in cases.iter().zip(&replies) {
        let sizes: Vec<usize> = reply.buffers.iter().map(Vec::len).collect();
        let writable = case.output.covered(&sizes);
        for (b, (after, buffer)) in reply.buffers.iter().zip(&case.request.buffers).enumerate() {
            let before = buffer.bytes();
            let untouched: Vec<(usize, u8, u8)> = (0..before.len())
                .filter(|&i| !writable[b][i] && before[i] != after[i])
                .map(|i| (i, before[i], after[i]))
                .collect();
            assert!(
                untouched.is_empty(),
                "{}: buffer {b} changed outside the output pixels at (offset, before, after) \
                 {:?}",
                case.label,
                &untouched[..untouched.len().min(8)]
            );
            if case.label.contains("padded") {
                written_in_padded_layouts +=
                    (0..before.len()).filter(|&i| before[i] != after[i]).count();
            }
        }
    }
    assert!(written_in_padded_layouts > 0, "the check saw no write");
}

/// Every bit depth.
const DEPTHS: [BitDepth; 6] = [
    BitDepth::Uint8,
    BitDepth::Uint10,
    BitDepth::Uint12,
    BitDepth::Uint16,
    BitDepth::F16,
    BitDepth::F32,
];

/// Runs `calls` in one oracle process; each must succeed.
fn batch(calls: &[BatchCall<'_>]) -> Vec<Response> {
    Oracle::get()
        .batch(calls, true)
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.unwrap_or_else(|e| panic!("call {i}: {e}")))
        .collect()
}

/// The constructors without strides (F32), `AutoStride` everywhere, and the tight strides
/// written out describe the same image: the same getters, for every bit depth, channel count
/// and order, packed or planar. `numChannels` 4 and 3 are RGBA and RGB.
#[test]
fn auto_stride_is_the_tight_layout() {
    let (w, h) = (3usize, 2usize);
    let mut shapes: Vec<(String, Option<Channels>, usize)> = ChannelOrder::ALL
        .iter()
        .map(|&o| {
            (
                o.oracle_name().to_string(),
                Some(Channels::Order(o)),
                o.channels(),
            )
        })
        .collect();
    shapes.push(("numChannels 4".into(), Some(Channels::Count(4)), 4));
    shapes.push(("numChannels 3".into(), Some(Channels::Count(3)), 3));
    shapes.push(("planar RGB".into(), None, 3));
    shapes.push(("planar RGBA".into(), None, 4));
    let mut requests = Vec::new();
    for depth in DEPTHS {
        let item = channel_bytes(depth);
        for (name, channels, n) in &shapes {
            let mut request = Request::new(json!({}));
            let images: Vec<Image> = match channels {
                Some(channels) => {
                    let buffer = request.buffer(Buffer::fill(w * h * n * item, &[0]));
                    let packed = Packed::new(Data::at(buffer, 0), w as i64, h as i64, *channels);
                    let tight = [item, n * item, w * n * item].map(|s| Stride::Bytes(s as i64));
                    let mut images = vec![
                        packed.clone().layout(depth, [Stride::Auto; 3]).into(),
                        packed.clone().layout(depth, tight).into(),
                    ];
                    if depth == BitDepth::F32 {
                        images.push(packed.into());
                    }
                    images
                }
                None => {
                    let planes: Vec<Data> = (0..*n)
                        .map(|_| Data::at(request.buffer(Buffer::fill(w * h * item, &[0])), 0))
                        .collect();
                    let planar = Planar::new(planes, w as i64, h as i64);
                    let tight = [item, w * item].map(|s| Stride::Bytes(s as i64));
                    let mut images = vec![
                        planar.clone().layout(depth, [Stride::Auto; 2]).into(),
                        planar.clone().layout(depth, tight).into(),
                    ];
                    if depth == BitDepth::F32 {
                        images.push(planar.into());
                    }
                    images
                }
            };
            for image in images {
                request.image(image);
            }
            requests.push((format!("{}, {name}", depth.oracle_name()), request));
        }
    }
    let calls: Vec<BatchCall<'_>> = requests.iter().map(|(_, r)| r.call()).collect();
    let mut getters: HashMap<String, Value> = HashMap::new();
    for ((label, request), response) in requests.iter().zip(batch(&calls)) {
        let reply = request.reply(response);
        assert!(reply.raised().is_none(), "{label}: {}", reply.result);
        for i in 1..request.images.len() {
            assert_eq!(reply.getters(i), reply.getters(0), "{label}: image {i}");
        }
        getters.insert(label.clone(), reply.getters(0).clone());
    }
    for depth in DEPTHS.map(BitDepth::oracle_name) {
        for (count, order) in [
            ("numChannels 4", "CHANNEL_ORDERING_RGBA"),
            ("numChannels 3", "CHANNEL_ORDERING_RGB"),
        ] {
            assert_eq!(
                getters[&format!("{depth}, {count}")],
                getters[&format!("{depth}, {order}")],
                "{depth}: {count} and {order}"
            );
        }
    }
}

/// Python's `applyRGB` and `applyRGBA` on an array are `apply` in place on one row of packed
/// RGB or RGBA pixels of the array's bit depth; on a list of floats, the same on F32 pixels.
#[test]
fn apply_rgb_is_apply_on_one_row_of_pixels() {
    const COUNT: usize = 37;
    let mut inputs = Vec::new();
    for (d, depth) in [
        BitDepth::F32,
        BitDepth::F16,
        BitDepth::Uint8,
        BitDepth::Uint16,
    ]
    .into_iter()
    .enumerate()
    {
        let item = channel_bytes(depth);
        let rgba = pixels(depth, COUNT, 100 + d as u64);
        for channels in [3, 4] {
            let bytes: Vec<u8> = rgba
                .chunks_exact(4 * item)
                .flat_map(|pixel| pixel[..channels * item].to_vec())
                .collect();
            inputs.push((
                depth,
                channels,
                RgbInput::array(bytes.clone(), dtype(depth)),
                bytes,
            ));
        }
    }
    // An array whose first entry is 16 bytes into its memory, which keeps those bytes.
    let bytes = pixels(BitDepth::F32, COUNT, 104);
    let memory = [
        PAD.iter().copied().cycle().take(16).collect(),
        bytes.clone(),
    ]
    .concat();
    inputs.push((
        BitDepth::F32,
        4,
        RgbInput::Array {
            bytes: memory,
            dtype: "float32".into(),
            shape: None,
            strides: None,
            offset: 16,
        },
        bytes,
    ));
    // Finite values and infinities only: the binding converts each Python float to a C float
    // and back, which would quiet a signalling NaN.
    let mut values: Vec<f32> = probe::uniform(7, COUNT * 4 - 6, -0.5, 2.0);
    values.extend([
        0.0,
        -0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::from_bits(1),
    ]);
    for channels in [3, 4] {
        let list = &values[values.len() - COUNT * channels..];
        let floats: Vec<f64> = list.iter().map(|&v| f64::from(v)).collect();
        let bytes: Vec<u8> = list.iter().flat_map(|v| v.to_le_bytes()).collect();
        inputs.push((BitDepth::F32, channels, RgbInput::list(&floats), bytes));
    }
    let mut labels = Vec::new();
    let mut images = Vec::new();
    let mut rgbs = Vec::new();
    for (depth, channels, input, bytes) in inputs {
        labels.push(format!(
            "{} on {} of {}",
            if channels == 4 {
                "applyRGBA"
            } else {
                "applyRGB"
            },
            match input {
                RgbInput::List(_) => "a list",
                RgbInput::Array { offset: 0, .. } => "an array",
                RgbInput::Array { .. } => "an array at an offset",
            },
            depth.oracle_name()
        ));
        let mut request = Request::new(processor(depth, depth));
        let buffer = request.buffer(Buffer::Bytes(bytes));
        let packed = Packed::new(
            Data::at(buffer, 0),
            COUNT as i64,
            1,
            Channels::Count(channels as i64),
        );
        let image = request.image(if depth == BitDepth::F32 {
            packed
        } else {
            packed.layout(depth, [Stride::Auto; 3])
        });
        request.apply = vec![image];
        images.push(request);
        rgbs.push(RgbRequest {
            processor: processor(depth, depth),
            rgba: channels == 4,
            input,
        });
    }
    let mut calls: Vec<BatchCall<'_>> = images.iter().map(Request::call).collect();
    calls.extend(rgbs.iter().map(RgbRequest::call));
    let mut responses = batch(&calls);
    let rgb_responses = responses.split_off(images.len());
    for (i, (image_response, rgb_response)) in responses.into_iter().zip(rgb_responses).enumerate()
    {
        let label = &labels[i];
        let image = images[i].reply(image_response);
        let rgb = RgbReply::from_response(rgb_response);
        assert!(image.raised().is_none(), "{label}: {}", image.result);
        assert!(rgb.raised().is_none(), "{label}: {}", rgb.result);
        let expected: Vec<u8> = match &rgbs[i].input {
            // The list comes back as Python floats: C floats widened to double.
            RgbInput::List(_) => image.buffers[0]
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|&b| f64::from(f32::from_le_bytes(b)).to_le_bytes())
                .collect(),
            // The array's whole memory: what precedes its first entry, then the pixels.
            RgbInput::Array { bytes, offset, .. } => {
                [&bytes[..*offset], &image.buffers[0][..]].concat()
            }
        };
        assert_bytes_eq(label, &expected, rgb.output.as_deref().expect("an output"));
        assert_eq!(
            rgb.result["cpu_cache_id"], image.result["cpu_cache_id"],
            "{label}"
        );
    }
}

/// A probe of one error path: the call, and what the wheel raises.
#[derive(Debug)]
struct Refusal {
    label: &'static str,
    call: Probe,
    /// `None`: the call succeeds on this platform.
    raises: Option<(&'static str, &'static str, Option<usize>)>,
    /// A fragment of the message, from upstream's text at the cited line: it identifies the
    /// path.
    fragment: String,
}

#[derive(Debug)]
enum Probe {
    Image(Request),
    Rgb(RgbRequest),
}

/// A request constructing `image` over a zeroed buffer of `bytes` bytes, without a processor.
fn construct(image: impl Into<Image>, bytes: usize) -> Probe {
    let mut request = Request::new(json!({}));
    request.buffer(Buffer::fill(bytes, &[0]));
    request.image(image);
    Probe::Image(request)
}

/// A request applying `processor` to `images` over zeroed buffers of `bytes` bytes.
fn apply(images: Vec<Image>, bytes: &[usize], processor: Value) -> Probe {
    let mut request = Request::new(processor);
    for &n in bytes {
        request.buffer(Buffer::fill(n, &[0]));
    }
    for image in images {
        let image = request.image(image);
        request.apply.push(image);
    }
    Probe::Image(request)
}

/// A request calling `applyRGBA` (or `applyRGB`) on `input` with `processor`.
fn rgb(rgba: bool, input: RgbInput, processor: Value) -> Probe {
    Probe::Rgb(RgbRequest {
        processor,
        rgba,
        input,
    })
}

/// Every error path of the table at the top that Python reaches, with its exception type,
/// stage and message; a CPU processor's bit depths have their own test
/// (`unsupported_bit_depths_raise_building_the_cpu_processor`). These calls raise before they
/// write, so their buffers are unchanged; an apply can also raise after writing rows
/// (`a_raising_apply_can_write`).
#[test]
fn every_error_path_python_reaches_raises() {
    use BitDepth::{F32, Uint8};
    use Channels::Count;
    let f32 = || processor(F32, F32);
    let rgba = |data: Data, w: i64, h: i64| Packed::new(data, w, h, Count(4));
    let at0 = || Data::at(0, 0);
    let planes = |data: [Data; 3]| data.to_vec();
    // Raised constructing the image (the only one), or applying the processor.
    let constructing = |kind: &'static str| Some((kind, "image", Some(0)));
    let applied = |kind: &'static str| Some((kind, "apply", None));
    let windows = cfg!(windows);
    let refusals = vec![
        // The binding.
        Refusal {
            label: "checkBufferType",
            call: construct(rgba(at0().dtype("float64"), 2, 2), 64),
            raises: constructing("RuntimeError"),
            fragment: "Incompatible buffer format: expected float32, but received float64".into(),
        },
        Refusal {
            label: "checkBufferType, an integer type",
            call: construct(
                rgba(at0().dtype("float32"), 2, 2).layout(BitDepth::Uint16, [Stride::Auto; 3]),
                64,
            ),
            raises: constructing("RuntimeError"),
            fragment: "expected 'u' (16-bit)".into(),
        },
        Refusal {
            label: "checkBufferSize",
            call: construct(rgba(at0().entries(15), 2, 2), 64),
            raises: constructing("RuntimeError"),
            fragment: "Incompatible buffer dimensions".into(),
        },
        Refusal {
            label: "bitDepthToDtype",
            call: construct(
                rgba(at0(), 2, 2).layout(Depth::Named("BIT_DEPTH_UINT14"), [Stride::Auto; 3]),
                64,
            ),
            raises: constructing("Exception"),
            fragment: "Error: Unsupported bit-depth: ".into(),
        },
        Refusal {
            label: "bitDepthToDtype, a value outside the enum",
            call: construct(
                rgba(at0(), 2, 2).layout(Depth::Value(99), [Stride::Auto; 3]),
                64,
            ),
            raises: constructing("Exception"),
            fragment: "Error: Unsupported bit-depth: ".into(),
        },
        Refusal {
            label: "chanOrderToNumChannels",
            call: construct(Packed::new(at0(), 2, 2, Channels::OrderValue(7)), 64),
            raises: constructing("Exception"),
            fragment: "Error: Unsupported channel ordering".into(),
        },
        Refusal {
            label: "checkBufferType, a plane",
            call: construct(
                Planar::new(planes([at0(), at0().dtype("float64"), at0()]), 2, 2),
                16,
            ),
            raises: constructing("RuntimeError"),
            fragment: "Incompatible buffer format".into(),
        },
        Refusal {
            label: "checkBufferSize, planes: B is checked first",
            call: construct(
                Planar::new(
                    planes([at0().entries(5), at0().entries(6), at0().entries(7)]),
                    2,
                    2,
                ),
                16,
            ),
            raises: constructing("RuntimeError"),
            fragment: "but received 7 entries".into(),
        },
        Refusal {
            label: "checkBufferSize, planes: A is checked first",
            call: construct(
                Planar::new(
                    vec![
                        at0().entries(5),
                        at0().entries(6),
                        at0().entries(7),
                        at0().entries(8),
                    ],
                    2,
                    2,
                ),
                16,
            ),
            raises: constructing("RuntimeError"),
            fragment: "but received 8 entries".into(),
        },
        Refusal {
            label: "bitDepthToDtype, planar",
            call: construct(
                Planar::new(planes([at0(), at0(), at0()]), 2, 2)
                    .layout(Depth::Named("BIT_DEPTH_UINT32"), [Stride::Auto; 2]),
                16,
            ),
            raises: constructing("Exception"),
            fragment: "Error: Unsupported bit-depth: ".into(),
        },
        Refusal {
            label: "a width beyond a 32-bit C long",
            call: construct(rgba(at0().entries(0), 1 << 31, 1), 16),
            raises: constructing(if windows { "TypeError" } else { "RuntimeError" }),
            fragment: if windows {
                "incompatible constructor arguments"
            } else {
                "Incompatible buffer dimensions"
            }
            .into(),
        },
        Refusal {
            label: "checkBufferSize's C long product wraps on Windows",
            call: construct(rgba(at0().entries(0), 65536, 65536), 16),
            raises: if windows {
                None
            } else {
                constructing("RuntimeError")
            },
            fragment: if windows {
                ""
            } else {
                "Incompatible buffer dimensions"
            }
            .into(),
        },
        Refusal {
            label: "checkBufferDivisible",
            call: rgb(false, RgbInput::array(vec![0; 20], "float32"), f32()),
            raises: applied("RuntimeError"),
            fragment: "expected size to be divisible by 3".into(),
        },
        Refusal {
            label: "checkCContiguousArray",
            call: rgb(
                false,
                RgbInput::Array {
                    bytes: vec![0; 24],
                    dtype: "float32".into(),
                    shape: Some(vec![3]),
                    strides: Some(vec![8]),
                    offset: 0,
                },
                f32(),
            ),
            raises: applied("RuntimeError"),
            fragment: "function only supports C-contiguous (row-major) arrays".into(),
        },
        Refusal {
            label: "getBufferBitDepth",
            call: rgb(false, RgbInput::array(vec![0; 24], "float64"), f32()),
            raises: applied("RuntimeError"),
            fragment: "Unsupported data type: float64".into(),
        },
        Refusal {
            label: "checkVectorDivisible",
            call: rgb(true, RgbInput::list(&[0.5, 0.25]), f32()),
            raises: applied("RuntimeError"),
            fragment: "Incompatible vector dimensions".into(),
        },
        // The library.
        Refusal {
            label: "ImageDesc.cpp:354",
            call: construct(Packed::new(at0(), 2, 2, Count(5)), 80),
            raises: constructing("Exception"),
            fragment: "PackedImageDesc Error: Invalid number of channels.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:291",
            call: construct(rgba(at0(), 0, 2), 16),
            raises: constructing("Exception"),
            fragment: "PackedImageDesc Error: Invalid image dimensions.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:297",
            call: construct(
                rgba(at0(), 2, 2).layout(F32, [Stride::Bytes(2), Stride::Auto, Stride::Auto]),
                64,
            ),
            raises: constructing("Exception"),
            fragment: "PackedImageDesc Error: Invalid channel stride.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:307",
            call: construct(
                rgba(at0(), 2, 2).layout(F32, [Stride::Auto, Stride::Bytes(12), Stride::Auto]),
                64,
            ),
            raises: constructing("Exception"),
            fragment: "PackedImageDesc Error: The channel and x strides are inconsistent.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:312",
            call: construct(
                rgba(at0(), 2, 1).layout(F32, [Stride::Bytes(1 << 61), Stride::Auto, Stride::Auto]),
                32,
            ),
            raises: if windows {
                constructing("Exception")
            } else {
                None
            },
            fragment: if windows {
                "PackedImageDesc Error: Invalid x stride."
            } else {
                ""
            }
            .into(),
        },
        Refusal {
            label: "ImageDesc.cpp:312, an odd width",
            call: construct(
                rgba(at0(), 1, 1).layout(F32, [Stride::Bytes(1 << 61), Stride::Auto, Stride::Auto]),
                16,
            ),
            raises: constructing("Exception"),
            fragment: if windows {
                "PackedImageDesc Error: Invalid x stride."
            } else {
                "PackedImageDesc Error: Invalid y stride."
            }
            .into(),
        },
        Refusal {
            label: "ImageDesc.cpp:317",
            call: construct(
                rgba(at0(), 2, 1)
                    .layout(F32, [Stride::Auto, Stride::Bytes(-(1 << 62)), Stride::Auto]),
                32,
            ),
            raises: constructing("Exception"),
            fragment: "PackedImageDesc Error: Invalid y stride.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:322",
            call: construct(
                rgba(at0(), 2, 2).layout(F32, [Stride::Auto, Stride::Auto, Stride::Bytes(16)]),
                64,
            ),
            raises: constructing("Exception"),
            fragment: "PackedImageDesc Error: The x and y strides are inconsistent.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:650",
            call: construct(Planar::new(planes([at0(), at0(), at0()]), 0, 2), 16),
            raises: constructing("Exception"),
            fragment: "PlanarImageDesc Error: Invalid image dimensions.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:623",
            call: construct(
                Planar::new(planes([at0(), at0(), at0()]), 2, 1)
                    .layout(F32, [Stride::Bytes(-(1 << 62)), Stride::Auto]),
                16,
            ),
            raises: constructing("Exception"),
            fragment: "PlanarImageDesc Error: Invalid y stride.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:628",
            call: construct(
                Planar::new(planes([at0(), at0(), at0()]), 2, 2)
                    .layout(F32, [Stride::Auto, Stride::Bytes(4)]),
                16,
            ),
            raises: constructing("Exception"),
            fragment: "PlanarImageDesc Error: The x and y strides are inconsistent.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:95",
            call: apply(
                vec![rgba(at0(), 2, 2).layout(Uint8, [Stride::Auto; 3]).into()],
                &[16],
                f32(),
            ),
            raises: applied("Exception"),
            fragment: "Bit-depth mismatch between the image buffer and the finalization setting."
                .into(),
        },
        Refusal {
            label: "ScanlineHelper.cpp:60",
            call: apply(
                vec![rgba(at0(), 2, 2).into(), rgba(Data::at(1, 0), 4, 1).into()],
                &[64, 64],
                f32(),
            ),
            raises: applied("Exception"),
            fragment: "Dimension inconsistency between source and destination image buffers."
                .into(),
        },
        Refusal {
            label: "ImagePacking.cpp:109, a pixel count that wraps to 0 on Windows",
            call: apply(
                vec![
                    Planar::new(vec![at0().entries(0); 3], 65536, 65536)
                        .layout(F32, [Stride::Bytes(0); 2])
                        .into(),
                ],
                &[4],
                f32(),
            ),
            raises: if windows {
                applied("Exception")
            } else {
                constructing("RuntimeError")
            },
            fragment: if windows {
                "Invalid output image position."
            } else {
                "Incompatible buffer dimensions"
            }
            .into(),
        },
        Refusal {
            label: "ImageDesc.cpp:286, applyRGB([])",
            call: rgb(false, RgbInput::list(&[]), f32()),
            raises: applied("Exception"),
            fragment: "PackedImageDesc Error: Invalid image buffer.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:291, applyRGBA on an empty array",
            call: rgb(true, RgbInput::array(Vec::new(), "float32"), f32()),
            raises: applied("Exception"),
            fragment: "PackedImageDesc Error: Invalid image dimensions.".into(),
        },
        Refusal {
            label: "ImageDesc.cpp:95, applyRGB on a uint8 array",
            call: rgb(false, RgbInput::array(vec![0; 3], "uint8"), f32()),
            raises: applied("Exception"),
            fragment: "Bit-depth mismatch".into(),
        },
    ];
    let calls: Vec<BatchCall<'_>> = refusals
        .iter()
        .map(|r| match &r.call {
            Probe::Image(request) => request.call(),
            Probe::Rgb(request) => request.call(),
        })
        .collect();
    for (refusal, response) in refusals.iter().zip(batch(&calls)) {
        let label = refusal.label;
        let (raised, unchanged) = match &refusal.call {
            Probe::Image(request) => {
                let reply = request.reply(response);
                let unchanged = reply
                    .buffers
                    .iter()
                    .zip(&request.buffers)
                    .all(|(a, b)| *a == b.bytes());
                (reply.raised(), unchanged)
            }
            Probe::Rgb(request) => {
                let reply = RgbReply::from_response(response);
                let unchanged = match (&request.input, &reply.output) {
                    (RgbInput::Array { bytes, .. }, Some(after)) => bytes == after,
                    (RgbInput::List(_), output) => output.is_none(),
                    (RgbInput::Array { .. }, None) => false,
                };
                (reply.raised(), unchanged)
            }
        };
        match (refusal.raises, raised) {
            (None, None) => {}
            (Some((kind, stage, image)), Some(raised)) => {
                assert_eq!(
                    (raised.kind.as_str(), raised.stage.as_str(), raised.image),
                    (kind, stage, image),
                    "{label}: {raised:?}"
                );
                assert!(
                    raised.message.contains(&refusal.fragment),
                    "{label}: {raised:?}"
                );
                assert!(unchanged, "{label}: a call that raised wrote a buffer");
            }
            (expected, raised) => panic!("{label}: expected {expected:?}, got {raised:?}"),
        }
    }
}

/// `getOptimizedCPUProcessor` takes any bit depth: the binding doesn't check them. UINT14,
/// UINT32 and UNKNOWN, in or out, raise while it builds the CPU processor. With ops left after
/// optimizing, `optimizeForBitdepth` asks `IsFloatBitDepth` of the input and output depths
/// (OpOptimizers.cpp:764, 768; BitDepthUtils.cpp:90-117 @ v2.5.2). With none, it skips the
/// empty list (OpOptimizers.cpp:762), and the CPU engine's cast to or from the identity matrix
/// it adds raises (CPUProcessor.cpp:93, 113, 122-184 @ v2.5.2). An empty group has no op; an
/// identity matrix has none left once the optimizer removes it. The messages name the depth as
/// `BitDepthToString` does (ParseUtils.cpp:171-182 @ v2.5.2).
#[test]
fn unsupported_bit_depths_raise_building_the_cpu_processor() {
    let transforms = [
        (
            "ops",
            processor(BitDepth::F32, BitDepth::F32)["transform"].clone(),
            true,
        ),
        ("an empty group", json!({"class": "GroupTransform"}), false),
        (
            "an identity matrix",
            json!({"class": "MatrixTransform"}),
            false,
        ),
    ];
    let mut cases = Vec::new();
    for (depth, name) in [
        ("BIT_DEPTH_UINT14", "14ui"),
        ("BIT_DEPTH_UINT32", "32ui"),
        ("BIT_DEPTH_UNKNOWN", "unknown"),
    ] {
        for (input, output) in [(depth, "BIT_DEPTH_F32"), ("BIT_DEPTH_F32", depth)] {
            for (what, transform, ops) in &transforms {
                let mut request = Request::new(json!({
                    "transform": transform,
                    "in_bitdepth": input,
                    "out_bitdepth": output,
                }));
                let buffer = request.buffer(Buffer::fill(16, &[0]));
                let image =
                    request.image(Packed::new(Data::at(buffer, 0), 1, 1, Channels::Count(4)));
                request.apply = vec![image];
                let message = if *ops {
                    format!("Bit depth is not supported: {name}.")
                } else {
                    "Unsupported bit-depth".to_string()
                };
                cases.push((format!("{what}, {input} to {output}"), request, message));
            }
        }
    }
    let calls: Vec<BatchCall<'_>> = cases.iter().map(|(_, r, _)| r.call()).collect();
    for ((label, request, message), response) in cases.iter().zip(batch(&calls)) {
        let raised = request.reply(response).raised();
        assert_eq!(
            raised,
            Some(Raised {
                kind: "Exception".into(),
                message: message.clone(),
                stage: "cpu_processor".into(),
                image: None,
            }),
            "{label}"
        );
    }
}

/// Windows only: an apply can raise after writing rows. A planar F32 image of 65536 x 65537
/// pixels, each plane's pixels on one float (strides of 0), has 2^32 + 65536 pixels, a count
/// PackRGBAFromImageDesc computes in a C long (ImagePacking.cpp:103-110 @ v2.5.2), 32 bits on
/// Windows: it wraps to 65536 (a signed overflow). Row 0 is processed and written; row 1, whose
/// first pixel index is 65536, raises. The binding expects the wrapped count of entries too
/// (checkBufferSize). The reply has the buffers as the call left them: each plane's float holds
/// what the same processor makes of the pixel on its own.
#[cfg(windows)]
#[test]
fn a_raising_apply_can_write() {
    let pixel = [0.75f32, 0.5, 0.25];
    let planar = |width: i64, height: i64, entries: i64| {
        let mut request = Request::new(processor(BitDepth::F32, BitDepth::F32));
        let planes = pixel
            .iter()
            .map(|&v| {
                Data::at(request.buffer(Buffer::Bytes(f32_to_bytes(&[v]))), 0).entries(entries)
            })
            .collect();
        let image = request
            .image(Planar::new(planes, width, height).layout(BitDepth::F32, [Stride::Bytes(0); 2]));
        request.apply = vec![image];
        request
    };
    let wide = planar(65536, 65537, 65536);
    let one = planar(1, 1, 1);
    let responses = batch(&[wide.call(), one.call()]);
    let (wide_reply, one_reply) = (
        wide.reply(responses[0].clone()),
        one.reply(responses[1].clone()),
    );
    let raised = wide_reply.raised().expect("the apply raises");
    assert_eq!(
        (raised.stage.as_str(), raised.message.as_str()),
        ("apply", "Invalid output image position.")
    );
    assert!(one_reply.raised().is_none(), "{}", one_reply.result);
    for (plane, (before, after)) in wide.buffers.iter().zip(&wide_reply.buffers).enumerate() {
        assert_ne!(&before.bytes(), after, "plane {plane} is written");
        assert_bytes_eq(&format!("plane {plane}"), &one_reply.buffers[plane], after);
    }
}

/// A `ChannelOrdering` passed positionally is taken as `numChannels`: the binding tries that
/// overload first, and its enums convert to int. `PackedImageDesc(buf, w, h,
/// CHANNEL_ORDERING_BGR)` is a 4-channel RGBA image. Keywords reach the `chanOrder`
/// constructors, which the other tests use.
#[test]
fn positional_channel_orderings_are_channel_counts() {
    let (w, h) = (3usize, 2usize);
    let input = pixels(BitDepth::F32, w * h, 11);
    let mut requests = Vec::new();
    for (value, order) in ChannelOrder::ALL.into_iter().enumerate() {
        let entries = (w * h * value) as i64;
        for positional in [true, false] {
            let mut request = Request::new(processor(BitDepth::F32, BitDepth::F32));
            let buffer = request.buffer(Buffer::Bytes(input.clone()));
            let data = Data::at(buffer, 0).entries(entries);
            let image = if positional {
                Packed::new(data, w as i64, h as i64, Channels::Order(order)).positional()
            } else {
                Packed::new(data, w as i64, h as i64, Channels::Count(value as i64))
            };
            request.apply = vec![request.image(image)];
            requests.push(request);
        }
    }
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    let replies: Vec<Reply> = requests
        .iter()
        .zip(batch(&calls))
        .map(|(r, s)| r.reply(s))
        .collect();
    for (i, [positional, count]) in replies.as_chunks::<2>().0.iter().enumerate() {
        let label = ChannelOrder::ALL[i].oracle_name();
        assert_eq!(
            positional.result, count.result,
            "{label} positionally, and numChannels {i}"
        );
        assert_eq!(positional.buffers, count.buffers, "{label}");
    }
    // Both kinds of outcome are covered: RGB and BGR (3 and 4) apply, the others raise.
    assert!(replies[6].raised().is_none() && replies[8].raised().is_none());
    assert!(replies[0].raised().is_some() && replies[2].raised().is_some());
}

/// The binding's data getters copy width * height * channels entries from the data pointer at
/// the channel stride (packed), or width * height entries from each plane: for tight images,
/// the image's whole buffer as `apply` left it. The oracle doesn't call a getter that would
/// copy bytes outside the buffer, or `getAData` without an alpha plane.
#[test]
fn data_getters_copy_the_tight_buffers() {
    use BitDepth::{F32, Uint10};
    let (w, h) = (5usize, 3usize);
    let mut requests = Vec::new();
    // Packed RGBA to packed BGRA.
    let mut request = Request::new(processor(F32, F32));
    let src = request.buffer(Buffer::Bytes(pixels(F32, w * h, 21)));
    let dst = request.buffer(Buffer::fill(w * h * 16, &PREFILL));
    let src = request.image(Packed::new(
        Data::at(src, 0),
        w as i64,
        h as i64,
        Channels::Count(4),
    ));
    let dst = request.image(Packed::new(
        Data::at(dst, 0),
        w as i64,
        h as i64,
        Channels::Order(ChannelOrder::Bgra),
    ));
    request.apply = vec![src, dst];
    requests.push(request);
    // Planar RGB, and 10-bit planar RGBA, in place; packed with its rows flipped.
    for (depth, planes) in [(F32, 3), (Uint10, 4)] {
        let item = channel_bytes(depth);
        let mut request = Request::new(processor(depth, depth));
        let rgba = pixels(depth, w * h, 22);
        let data: Vec<Data> = (0..planes)
            .map(|c| {
                let plane: Vec<u8> = rgba
                    .chunks_exact(4 * item)
                    .flat_map(|p| p[c * item..(c + 1) * item].to_vec())
                    .collect();
                Data::at(request.buffer(Buffer::Bytes(plane)), 0)
            })
            .collect();
        let planar = Planar::new(data, w as i64, h as i64);
        let image = request.image(if depth == F32 {
            planar
        } else {
            planar.layout(depth, [Stride::Auto; 2])
        });
        request.apply = vec![image];
        requests.push(request);
    }
    let mut request = Request::new(processor(F32, F32));
    let buffer = request.buffer(Buffer::Bytes(pixels(F32, w * h, 23)));
    let rows = (w * 16) as i64;
    let flipped = Packed::new(
        Data::at(buffer, (h - 1) * w * 16),
        w as i64,
        h as i64,
        Channels::Count(4),
    )
    .layout(F32, [Stride::Auto, Stride::Auto, Stride::Bytes(-rows)]);
    request.apply = vec![request.image(flipped)];
    requests.push(request);

    for request in &mut requests {
        request.data_getters = true;
    }
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    let replies: Vec<Reply> = requests
        .iter()
        .zip(batch(&calls))
        .map(|(r, s)| r.reply(s))
        .collect();
    for reply in &replies {
        assert!(reply.raised().is_none(), "{}", reply.result);
    }
    assert_bytes_eq(
        "RGBA getData",
        &replies[0].buffers[0],
        replies[0].data(0, "getData").unwrap(),
    );
    assert_bytes_eq(
        "BGRA getData",
        &replies[0].buffers[1],
        replies[0].data(1, "getData").unwrap(),
    );
    for (r, planes) in [(1, 3), (2, 4)] {
        for (c, getter) in ["getRData", "getGData", "getBData", "getAData"]
            .into_iter()
            .enumerate()
        {
            let copy = replies[r].data(0, getter);
            if c < planes {
                assert_bytes_eq(getter, &replies[r].buffers[c], copy.unwrap());
            } else {
                assert!(copy.is_none(), "{getter} without an alpha plane");
            }
        }
    }
    assert!(
        replies[3].data(0, "getData").is_none(),
        "getData reading past the last row"
    );
}

/// The getters the oracle reports describe the bytes the wheel wrote: a footprint built from a
/// reply's own getters covers every byte the apply changed, and reads the same pixels as the
/// layout the request described, for padded channel strides, negative x and y strides and
/// several channel orders.
#[test]
fn getters_describe_the_written_bytes() {
    use BitDepth::{F32, Uint16};
    // (depth, order, channel, x and y strides, buffer bytes, data offset): 3 x 2 images.
    let layouts = [
        (F32, ChannelOrder::Bgra, [8i64, 36, 120], 240usize, 0usize),
        (F32, ChannelOrder::Rgb, [4, -16, 56], 112, 32),
        (Uint16, ChannelOrder::Abgr, [2, 10, -34], 68, 34),
    ];
    let requests: Vec<Request> = layouts
        .iter()
        .enumerate()
        .map(|(i, &(depth, order, strides, size, offset))| {
            let mut request = Request::new(processor(depth, depth));
            // Pixel values in every byte, padding included.
            let bytes = pixels(depth, size, 40 + i as u64)[..size].to_vec();
            let buffer = request.buffer(Buffer::Bytes(bytes));
            let image = request.image(
                Packed::new(Data::at(buffer, offset), 3, 2, Channels::Order(order))
                    .layout(depth, strides.map(Stride::Bytes)),
            );
            request.apply = vec![image];
            request
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = requests.iter().map(Request::call).collect();
    for ((&(depth, order, [chan, x, y], _, offset), request), response) in
        layouts.iter().zip(&requests).zip(batch(&calls))
    {
        let reply = request.reply(response);
        assert!(reply.raised().is_none(), "{order:?}: {}", reply.result);
        let getters = reply.getters(0);
        let number = |name: &str| getters[name].as_i64().expect(name);
        let reported_order = ChannelOrder::ALL
            .into_iter()
            .find(|o| getters["getChannelOrder"] == o.oracle_name())
            .expect("a channel order");
        let reported = Footprint::packed(
            0,
            offset as i64,
            number("getWidth") as usize,
            number("getHeight") as usize,
            reported_order,
            channel_bytes(depth),
            number("getChanStrideBytes"),
            number("getXStrideBytes"),
            number("getYStrideBytes"),
        );
        let requested = Footprint::packed(
            0,
            offset as i64,
            3,
            2,
            order,
            channel_bytes(depth),
            chan,
            x,
            y,
        );
        let before = request.buffers[0].bytes();
        let after = &reply.buffers[0];
        let covered = &reported.covered(&[after.len()])[0];
        let changed: Vec<usize> = (0..after.len())
            .filter(|&i| before[i] != after[i])
            .collect();
        assert!(!changed.is_empty(), "{order:?}: the apply wrote nothing");
        assert!(
            changed.iter().all(|&i| covered[i]),
            "{order:?}: bytes changed outside the reported layout"
        );
        for c in 0..4 {
            assert_eq!(
                reported.read(&reply.buffers, c),
                requested.read(&reply.buffers, c),
                "{order:?}: channel {c}"
            );
        }
    }
}

/// Upstream's own tests of the image descriptions, which its Java binding runs, through the
/// Python binding, with their pixels, calls and expected values:
/// `PackedImageDescTest.test_interface` and `PlanarImageDescTest.test_interface`
/// (upstream/OpenColorIO/tests/java/org/OpenColorIO/PackedImageDescTest.java:16-39 and
/// PlanarImageDescTest.java:16-51 @ v2.5.2).
#[test]
fn upstreams_image_description_tests() {
    let (width, height) = (2, 2);
    let mut packed = Request::new(json!({}));
    let buffer = packed.buffer(Buffer::Bytes(f32_to_bytes(&[
        0.1, 0.1, 0.1, 1.0, 0.2, 0.2, 0.2, 1.0, 0.3, 0.3, 0.3, 1.0, 0.4, 0.4, 0.4, 1.0,
    ])));
    packed.image(Packed::new(
        Data::at(buffer, 0),
        width,
        height,
        Channels::Count(4),
    ));
    packed.data_getters = true;
    let mut planar = Request::new(json!({}));
    let planes = [
        [0.1f32, 0.2, 0.3, 0.4],
        [0.1, 0.2, 0.3, 0.4],
        [0.1, 0.2, 0.3, 0.4],
        [1.0, 1.0, 1.0, 1.0],
    ]
    .iter()
    .map(|plane| Data::at(planar.buffer(Buffer::Bytes(f32_to_bytes(plane))), 0))
    .collect();
    planar.image(Planar::new(planes, width, height));
    planar.data_getters = true;
    let responses = batch(&[packed.call(), planar.call()]);
    let (packed, planar) = (
        packed.reply(responses[0].clone()),
        planar.reply(responses[1].clone()),
    );
    let entry = |reply: &Reply, getter: &str, i: usize| {
        let bytes = reply.data(0, getter).expect(getter);
        f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap())
    };
    // PackedImageDescTest.java:31-37.
    assert_eq!(entry(&packed, "getData", 10).to_bits(), 0.3f32.to_bits());
    let getters = packed.getters(0);
    assert_eq!(getters["getWidth"], 2);
    assert_eq!(getters["getHeight"], 2);
    assert_eq!(getters["getNumChannels"], 4);
    assert_eq!(getters["getChanStrideBytes"], 4);
    assert_eq!(getters["getXStrideBytes"], 16);
    assert_eq!(getters["getYStrideBytes"], 32);
    // PlanarImageDescTest.java:43-49.
    assert_eq!(entry(&planar, "getRData", 0).to_bits(), 0.1f32.to_bits());
    assert_eq!(entry(&planar, "getGData", 1).to_bits(), 0.2f32.to_bits());
    assert_eq!(entry(&planar, "getBData", 2).to_bits(), 0.3f32.to_bits());
    assert_eq!(entry(&planar, "getAData", 3).to_bits(), 1.0f32.to_bits());
    let getters = planar.getters(0);
    assert_eq!(getters["getWidth"], 2);
    assert_eq!(getters["getHeight"], 2);
    assert_eq!(getters["getYStrideBytes"], 8);
}

/// The optimization flags reach the CPU processor: `image_apply` and `cpu_apply` give the same
/// cache ID and pixels for the same processor spec, with flags other than the default ones.
#[test]
fn optimization_reaches_the_processor() {
    let values = f32_to_bytes(&[
        0.5, 0.25, 1.5, 1.0, -0.125, 2.0, 0.75, 0.5, 3.0, 0.0625, 0.3, 0.9,
    ]);
    let mut spec = processor(BitDepth::F32, BitDepth::F32);
    spec["optimization"] = json!("OPTIMIZATION_NONE");
    let mut request = Request::new(spec.clone());
    let buffer = request.buffer(Buffer::Bytes(values.clone()));
    let image = request.image(Packed::new(Data::at(buffer, 0), 3, 1, Channels::Count(4)));
    request.apply = vec![image];
    let calls = [
        request.call(),
        BatchCall {
            cmd: "cpu_apply",
            args: spec,
            blobs: vec![&values],
        },
    ];
    let responses = batch(&calls);
    let reply = request.reply(responses[0].clone());
    assert!(reply.raised().is_none(), "{}", reply.result);
    let cache_id = reply.result["cpu_cache_id"].as_str().expect("a cache ID");
    assert_eq!(cache_id, responses[1].result["cpu_cache_id"]);
    assert_eq!(reply.buffers[0], responses[1].blobs[0]);
}

/// Data getter copies that aren't the whole buffer: rows with padding, where the copy of
/// `width * height * channels` entries stops before the buffer does, and a padded channel
/// stride with rows flipped, where the copy would run past the buffer, so the oracle doesn't
/// make it.
#[test]
fn data_getter_copies_that_are_not_the_buffer() {
    use BitDepth::F32;
    let floats = |scale: f32| f32_to_bytes(&(0..32).map(|i| i as f32 * scale).collect::<Vec<_>>());
    let mut padded = Request::new(processor(F32, F32));
    let buffer = padded.buffer(Buffer::Bytes(floats(0.125)));
    let image = padded.image(
        Packed::new(Data::at(buffer, 0), 3, 2, Channels::Count(4))
            .layout(F32, [Stride::Auto, Stride::Auto, Stride::Bytes(64)]),
    );
    padded.apply = vec![image];
    padded.data_getters = true;
    let mut flipped = Request::new(processor(F32, F32));
    let buffer = flipped.buffer(Buffer::Bytes(floats(0.25)));
    let image = flipped.image(
        Packed::new(
            Data::at(buffer, 64),
            2,
            2,
            Channels::Order(ChannelOrder::Bgra),
        )
        .layout(
            F32,
            [Stride::Bytes(8), Stride::Bytes(32), Stride::Bytes(-64)],
        ),
    );
    flipped.apply = vec![image];
    flipped.data_getters = true;
    let responses = batch(&[padded.call(), flipped.call()]);
    let padded_reply = padded.reply(responses[0].clone());
    assert!(padded_reply.raised().is_none(), "{}", padded_reply.result);
    // 3 * 2 * 4 floats from the data pointer at the channel stride, 4 bytes: the first 96 of
    // the buffer's 128 bytes, row 0's padding included and row 1's cut short.
    assert_bytes_eq(
        "padded rows",
        &padded_reply.buffers[0][..96],
        padded_reply.data(0, "getData").expect("the copy"),
    );
    let flipped_reply = flipped.reply(responses[1].clone());
    assert!(flipped_reply.raised().is_none(), "{}", flipped_reply.result);
    assert!(
        flipped_reply.data(0, "getData").is_none(),
        "a copy that would read past the buffer"
    );
}

/// Bit depths and channel orders given as integers are the enum values they name
/// (`BIT_DEPTH_F32` is 8 and `CHANNEL_ORDERING_BGRA` 1,
/// upstream/OpenColorIO/include/OpenColorIO/OpenColorTypes.h:428-457 @ v2.5.2): the same
/// getters as the names.
#[test]
fn integer_enum_values_are_the_named_ones() {
    let mut request = Request::new(json!({}));
    let buffer = request.buffer(Buffer::fill(96, &[0]));
    for (depth, order) in [
        (
            Depth::Named("BIT_DEPTH_F32"),
            Channels::Order(ChannelOrder::Bgra),
        ),
        (Depth::Value(8), Channels::OrderValue(1)),
    ] {
        request
            .image(Packed::new(Data::at(buffer, 0), 3, 2, order).layout(depth, [Stride::Auto; 3]));
    }
    let reply = request.reply(batch(&[request.call()]).remove(0));
    assert!(reply.raised().is_none(), "{}", reply.result);
    assert_eq!(reply.getters(1), reply.getters(0));
}

/// What the oracle does with a request of [`applies_outside_the_wheels_memory_are_refused`].
#[derive(Debug, Clone, Copy)]
enum Outcome {
    /// The command refuses the request, with this fragment in its message.
    Refused(&'static str),
    /// The wheel applies without raising.
    Applies,
    /// The wheel raises, with this fragment in its message, before its apply reads a pixel.
    Raises(&'static str),
}

/// The oracle refuses to apply where the wheel would read or write outside its memory: an
/// image reaching outside its buffer, whatever the direction of its strides; a data pointer
/// outside its buffer; an image too wide for the scanline helper's integers; 10- and 12-bit
/// color codes above the bit depth's largest, which the forward 1D LUT that starts these
/// processors would look up. Alpha codes are scaled, not
/// looked up, an inverse 1D LUT searches, a processor without a LUT first multiplies, and an
/// apply that raises before it reads the source reads no code: those aren't refused.
/// Constructing the same image reads no pixels and is allowed, and the other calls of the batch
/// still run. A spec with a key the command doesn't know is refused.
#[test]
fn applies_outside_the_wheels_memory_are_refused() {
    use BitDepth::{F32, Uint10, Uint12, Uint16};
    use Outcome::{Applies, Raises, Refused};
    let rgba = |offset: usize, strides: [Stride; 3]| {
        Packed::new(Data::at(0, offset), 2, 2, Channels::Count(4)).layout(F32, strides)
    };
    let codes_buffer = |request: &mut Request, codes: &[u16]| {
        request.buffer(Buffer::Bytes(
            codes.iter().flat_map(|c| c.to_le_bytes()).collect(),
        ))
    };
    let pixel = |depth: BitDepth, channels: Channels, codes: [u16; 4], processor: Value| {
        let mut request = Request::new(processor);
        let buffer = codes_buffer(&mut request, &codes);
        let image =
            Packed::new(Data::at(buffer, 0), 1, 1, channels).layout(depth, [Stride::Auto; 3]);
        request.apply = vec![request.image(image)];
        request
    };
    let one_pixel = |depth: BitDepth, codes: [u16; 4]| {
        pixel(depth, Channels::Count(4), codes, processor(depth, depth))
    };
    let rgba_count = Channels::Count(4);
    // A 10-bit pixel applied to an RGBA image of `depth` and `width` pixels.
    let onto = |depth: BitDepth, width: i64, codes: [u16; 4]| {
        let mut request = Request::new(processor(Uint10, Uint10));
        let src = codes_buffer(&mut request, &codes);
        let bytes = width as usize * 4 * channel_bytes(depth);
        let dst = request.buffer(Buffer::fill(bytes, &[0]));
        let src = request.image(
            Packed::new(Data::at(src, 0), 1, 1, rgba_count).layout(Uint10, [Stride::Auto; 3]),
        );
        let dst = request.image(
            Packed::new(Data::at(dst, 0), width, 1, rgba_count).layout(depth, [Stride::Auto; 3]),
        );
        request.apply = vec![src, dst];
        request
    };
    // A 3 x 2 10-bit RGBA image whose last pixel of its last row has a red code of 1024, with
    // its rows in order or flipped (row 1 first in the buffer).
    let last_pixel = |flipped: bool| {
        let mut codes = [7u16; 24];
        codes[if flipped { 8 } else { 20 }] = 1024;
        let mut request = Request::new(processor(Uint10, Uint10));
        let buffer = codes_buffer(&mut request, &codes);
        let (offset, y_stride) = if flipped {
            (24, Stride::Bytes(-24))
        } else {
            (0, Stride::Auto)
        };
        let image = Packed::new(Data::at(buffer, offset), 3, 2, rgba_count)
            .layout(Uint10, [Stride::Auto, Stride::Auto, y_stride]);
        request.apply = vec![request.image(image)];
        request
    };
    // A 10-bit pixel in four planes.
    let planes = |codes: [u16; 4]| {
        let mut request = Request::new(processor(Uint10, Uint10));
        let buffer = codes_buffer(&mut request, &codes);
        let planes = (0..4).map(|c| Data::at(buffer, 2 * c)).collect();
        let image = Planar::new(planes, 1, 1).layout(Uint10, [Stride::Auto; 2]);
        request.apply = vec![request.image(image)];
        request
    };
    // A 2 x 2 RGBA F32 image applied to a destination of `bytes` bytes, which needs 64.
    let into_bytes = |bytes: usize| {
        let mut request = Request::new(processor(F32, F32));
        let src = request.buffer(Buffer::Bytes(f32_to_bytes(&[0.5; 16])));
        let dst = request.buffer(Buffer::fill(bytes, &[0]));
        let src = request.image(Packed::new(Data::at(src, 0), 2, 2, rgba_count));
        let dst = request.image(Packed::new(Data::at(dst, 0), 2, 2, rgba_count));
        request.apply = vec![src, dst];
        request
    };
    // A 2 x 2 RGBA source of `src` depth holding the codes 100 to 810 from the start of a
    // 128-byte buffer, and a 2 x 2 destination of `dst` depth and `order`: `at` that offset in
    // the same buffer, or in a buffer of its own. The verifier's G1 probe is the destination
    // at 0.
    let overlapping =
        |src: BitDepth, dst: BitDepth, order: ChannelOrder, at: Option<usize>, processor: Value| {
            let mut request = Request::new(processor);
            let codes: Vec<u16> = (0..16).map(|k| 100 * (k % 8 + 1) + 10 * (k / 8)).collect();
            let mut bytes: Vec<u8> = codes.iter().flat_map(|c| c.to_le_bytes()).collect();
            bytes.resize(128, 0);
            let first = request.buffer(Buffer::Bytes(bytes));
            let (second, offset) = match at {
                Some(offset) => (first, offset),
                None => (request.buffer(Buffer::fill(128, &[0])), 0),
            };
            let src = request.image(
                Packed::new(Data::at(first, 0), 2, 2, rgba_count).layout(src, [Stride::Auto; 3]),
            );
            let dst = request.image(
                Packed::new(Data::at(second, offset), 2, 2, Channels::Order(order))
                    .layout(dst, [Stride::Auto; 3]),
            );
            request.apply = vec![src, dst];
            request
        };
    let unoptimized_to = |output: BitDepth| {
        let mut p = processor(Uint10, output);
        p["optimization"] = json!("OPTIMIZATION_NONE");
        p
    };
    // A planar F32 image `width` pixels wide, every pixel of each plane on one float.
    let aliased = |width: i64| {
        let mut request = Request::new(processor(F32, F32));
        let buffer = request.buffer(Buffer::fill(4, &[0]));
        let planes = vec![Data::at(buffer, 0).entries(width); 3];
        let image = Planar::new(planes, width, 1).layout(F32, [Stride::Bytes(0); 2]);
        request.apply = vec![request.image(image)];
        request
    };
    let abgr = Channels::Order(ChannelOrder::Abgr);
    let unoptimized = |transform: Value| {
        json!({
            "transform": transform,
            "optimization": "OPTIMIZATION_NONE",
            "in_bitdepth": Uint10.oracle_name(),
            "out_bitdepth": Uint10.oracle_name(),
        })
    };
    let lut =
        |calls: Value| json!({"class": "Lut1DTransform", "args": {"length": 3}, "calls": calls});
    let matrix =
        json!({"class": "MatrixTransform", "args": {"offset": [0.125, -0.25, 0.0625, 0.5]}});
    // Unoptimized, a processor keeps its ops: the default optimization would bake the matrix
    // and the LUT into one forward LUT, and replace an inverse LUT with a forward one.
    let without_lut = unoptimized(matrix.clone());
    let lut_second = unoptimized(json!({"class": "GroupTransform", "children": [
        matrix,
        lut(json!([["setValue", 1, 0.25, 0.25, 0.25]])),
    ]}));
    let inverse_lut = unoptimized(lut(json!([
        ["setValue", 1, 0.25, 0.25, 0.25],
        ["setDirection", {"enum": "TRANSFORM_DIR_INVERSE"}],
    ])));
    let in_64_bytes = |image: Image, apply: bool| {
        let mut request = Request::new(processor(F32, F32));
        request.buffer(Buffer::fill(64, &[0]));
        let image = request.image(image);
        if apply {
            request.apply = vec![image];
        }
        request
    };
    let auto = Stride::Auto;
    let cases: Vec<(&str, Request, Outcome)> = vec![
        (
            "rows too far apart",
            in_64_bytes(rgba(0, [auto, auto, Stride::Bytes(64)]).into(), true),
            Refused("refuses to apply"),
        ),
        (
            "rows flipped from the first",
            in_64_bytes(rgba(0, [auto, auto, Stride::Bytes(-32)]).into(), true),
            Refused("refuses to apply"),
        ),
        (
            "pixels flipped from the first",
            in_64_bytes(
                rgba(0, [auto, Stride::Bytes(-16), Stride::Bytes(32)]).into(),
                true,
            ),
            Refused("refuses to apply"),
        ),
        (
            "channels too far apart",
            in_64_bytes(rgba(0, [Stride::Bytes(8), auto, auto]).into(), true),
            Refused("refuses to apply"),
        ),
        (
            "a plane past the end",
            in_64_bytes(
                Planar::new(vec![Data::at(0, 0), Data::at(0, 16), Data::at(0, 52)], 2, 2).into(),
                true,
            ),
            Refused("refuses to apply"),
        ),
        (
            "a data pointer past the end",
            in_64_bytes(rgba(64, [auto; 3]).into(), false),
            Refused("isn't inside"),
        ),
        (
            "a destination one byte short",
            into_bytes(63),
            Refused("refuses to apply to images[1]"),
        ),
        ("a destination that fits exactly", into_bytes(64), Applies),
        (
            "a 10-bit source under an F32 destination, over the same bytes (G1)",
            overlapping(
                Uint10,
                F32,
                ChannelOrder::Rgba,
                Some(0),
                processor(Uint10, F32),
            ),
            Refused("their bytes overlap in buffers[0]"),
        ),
        (
            "a 12-bit source under a 10-bit destination of another order, over the same bytes",
            overlapping(
                Uint12,
                Uint10,
                ChannelOrder::Bgra,
                Some(0),
                processor(Uint12, Uint10),
            ),
            Refused("their bytes overlap in buffers[0]"),
        ),
        (
            "a 10-bit source whose last byte is an F32 destination's first",
            overlapping(
                Uint10,
                F32,
                ChannelOrder::Rgba,
                Some(31),
                processor(Uint10, F32),
            ),
            Refused("their bytes overlap in buffers[0]"),
        ),
        (
            "a 10-bit source right before an F32 destination, in one buffer",
            overlapping(
                Uint10,
                F32,
                ChannelOrder::Rgba,
                Some(32),
                processor(Uint10, F32),
            ),
            Applies,
        ),
        (
            "a 10-bit source and an F32 destination in two buffers",
            overlapping(
                Uint10,
                F32,
                ChannelOrder::Rgba,
                None,
                processor(Uint10, F32),
            ),
            Applies,
        ),
        (
            "a 10-bit source under an F32 destination, without a LUT first",
            overlapping(
                Uint10,
                F32,
                ChannelOrder::Rgba,
                Some(0),
                unoptimized_to(F32),
            ),
            Applies,
        ),
        (
            "a 16-bit source under an F32 destination: its LUT holds every code",
            overlapping(
                Uint16,
                F32,
                ChannelOrder::Rgba,
                Some(0),
                processor(Uint16, F32),
            ),
            Applies,
        ),
        (
            "two 10-bit descriptions of the same layout, over the same bytes",
            overlapping(
                Uint10,
                Uint10,
                ChannelOrder::Rgba,
                Some(0),
                processor(Uint10, Uint10),
            ),
            Applies,
        ),
        (
            "an image 2^30 + 1 pixels wide, past the scanline helper's integers",
            aliased((1 << 30) + 1),
            Refused("pixels wide"),
        ),
        (
            "a 10-bit green code of 1024",
            one_pixel(Uint10, [0, 1024, 0, 0]),
            Refused("the color code 1024"),
        ),
        (
            "a 10-bit code of 1024 in the last pixel of the last row",
            last_pixel(false),
            Refused("the color code 1024"),
        ),
        (
            "a 10-bit code of 1024 in the last pixel of the last row, rows flipped",
            last_pixel(true),
            Refused("the color code 1024"),
        ),
        (
            "a 10-bit blue plane code of 1024",
            planes([0, 0, 1024, 0]),
            Refused("the color code 1024"),
        ),
        (
            "a 12-bit blue code of 4096",
            one_pixel(Uint12, [0, 0, 4096, 0]),
            Refused("the color code 4096"),
        ),
        (
            "a 10-bit red code of 2000, last in an ABGR pixel",
            pixel(Uint10, abgr, [0, 0, 0, 2000], processor(Uint10, Uint10)),
            Refused("the color code 2000"),
        ),
        (
            "a 10-bit code of 2000, applied to another image",
            onto(Uint10, 1, [0, 2000, 0, 0]),
            Refused("the color code 2000"),
        ),
        (
            "a 10-bit code of 1023",
            one_pixel(Uint10, [1023, 0, 1023, 1023]),
            Applies,
        ),
        (
            "a 12-bit code of 4095",
            one_pixel(Uint12, [4095, 0, 0, 0]),
            Applies,
        ),
        (
            "a 12-bit alpha code of 4096, which is scaled, not looked up",
            one_pixel(Uint12, [0, 0, 0, 4096]),
            Applies,
        ),
        (
            "a 10-bit alpha code of 2000, first in an ABGR pixel",
            pixel(Uint10, abgr, [2000, 0, 0, 0], processor(Uint10, Uint10)),
            Applies,
        ),
        (
            "a 10-bit alpha plane code of 2000",
            planes([0, 0, 0, 2000]),
            Applies,
        ),
        (
            "a 10-bit red code of 2000, without a LUT to look it up",
            pixel(Uint10, rgba_count, [2000, 0, 0, 0], without_lut),
            Applies,
        ),
        (
            "a 10-bit red code of 2000, with a 1D LUT after the first op",
            pixel(Uint10, rgba_count, [2000, 0, 0, 0], lut_second),
            Applies,
        ),
        (
            "a 10-bit red code of 2000, into an inverse 1D LUT, which searches",
            pixel(Uint10, rgba_count, [2000, 0, 0, 0], inverse_lut),
            Applies,
        ),
        (
            "a 10-bit code of 2000, for a 12-bit processor",
            pixel(
                Uint10,
                Channels::Count(4),
                [0, 2000, 0, 0],
                processor(Uint12, Uint12),
            ),
            Raises("Bit-depth mismatch"),
        ),
        (
            "a 10-bit code of 2000, applied to a 12-bit image",
            onto(Uint12, 1, [0, 2000, 0, 0]),
            Raises("Bit-depth mismatch"),
        ),
        (
            "a 10-bit code of 2000, applied to a wider image",
            onto(Uint10, 2, [0, 2000, 0, 0]),
            Raises("Dimension inconsistency"),
        ),
        (
            "rows too far apart, constructed",
            in_64_bytes(rgba(0, [auto, auto, Stride::Bytes(64)]).into(), false),
            Applies,
        ),
    ];
    // Specs the typed builders can't write.
    let packed = json!({"kind": "packed", "data": {"buffer": 0}, "width": 1, "height": 1,
        "num_channels": 4});
    let with = |key: &str, value: Value| {
        let mut spec = packed.clone();
        spec[key] = value;
        spec
    };
    let floats = f32_to_bytes(&[0.5; 3]);
    let raw: Vec<(&str, &str, Value, &'static str)> = vec![
        (
            "a misspelled fill",
            "image_apply",
            json!({"buffers": [{"size": 16, "fil": [1]}]}),
            "unknown keys ['fil']",
        ),
        (
            "an unknown image key",
            "image_apply",
            json!({"buffers": [{"size": 16}], "images": [with("stride", json!(4))]}),
            "unknown keys ['stride']",
        ),
        (
            "an unknown data key",
            "image_apply",
            json!({"buffers": [{"size": 32}],
                "images": [with("data", json!({"buffer": 0, "ofset": 16}))]}),
            "unknown keys ['ofset']",
        ),
        (
            "an unknown top-level key",
            "image_apply",
            json!({"buffers": [{"size": 16}], "channels": 4}),
            "unknown keys ['channels']",
        ),
        (
            "both num_channels and chan_order",
            "image_apply",
            json!({"buffers": [{"size": 16}],
                "images": [with("chan_order", json!("CHANNEL_ORDERING_RGBA"))]}),
            "takes num_channels or chan_order",
        ),
        (
            "a misspelled array key",
            "image_apply_rgb",
            json!({"call": "applyRGB", "dtyp": "float32"}),
            "unknown keys ['dtyp']",
        ),
        (
            "an array key with a list",
            "image_apply_rgb",
            json!({"call": "applyRGB", "list": true, "dtype": "float32"}),
            "unknown keys ['dtype']",
        ),
    ];
    let mut calls: Vec<BatchCall<'_>> = cases.iter().map(|(_, r, _)| r.call()).collect();
    calls.extend(raw.iter().map(|(_, cmd, args, _)| BatchCall {
        cmd,
        args: args.clone(),
        blobs: if *cmd == "image_apply_rgb" {
            vec![&floats]
        } else {
            Vec::new()
        },
    }));
    let expected = cases
        .iter()
        .map(|(label, _, outcome)| (*label, *outcome))
        .chain(
            raw.iter()
                .map(|(label, _, _, fragment)| (*label, Refused(fragment))),
        );
    for ((label, outcome), result) in expected.zip(Oracle::get().batch(&calls, false)) {
        let raised = result.as_ref().ok().map(|r| r.result.get("exception"));
        match (outcome, &result, raised) {
            (Refused(fragment), Err(e), _) => assert!(e.contains(fragment), "{label}: {e}"),
            (Applies, Ok(_), Some(None)) => {}
            (Raises(fragment), Ok(response), Some(Some(exception))) => {
                assert!(
                    exception["message"]
                        .as_str()
                        .is_some_and(|m| m.contains(fragment)),
                    "{label}: {exception}"
                );
                assert_eq!(response.result["stage"], "apply", "{label}");
            }
            _ => panic!("{label}: expected {outcome:?}, got {result:?}"),
        }
    }
}

/// Runs `script` in the oracle's environment, as the oracle runs, with `args`; returns the lines
/// it prints. For checks of the oracle's own refusals that no request can reach.
fn oracle_python(script: &str, args: &[String]) -> Vec<String> {
    let output = std::process::Command::new(Oracle::get().python())
        .args(["-X", "utf8", "-c", script])
        .args(args)
        .current_dir(ocio_testkit::paths::oracle_dir())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env_remove("PYTHONPATH")
        .output()
        .expect("the oracle's Python runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout.lines().map(str::to_string).collect()
}

/// An image the library calls RGBA-packed is read and written a row at a time: 4 * width
/// contiguous channels from the data pointer plus y * yStride (ScanlineHelper.cpp:129-136,
/// 158-164 @ v2.5.2). isRGBAPacked tests the x stride truncated to an `int` (ImageDesc.cpp:264
/// @ v2.5.2), so the wheel calls an RGBA F32 image with an x stride of 16 - 2^32 packed: its
/// second pixel is 2^32 - 16 bytes before its first, and its rows reach 16 bytes past the first.
/// The oracle checks the rows too. A request can't show it, as its buffer would need 12 GiB:
/// this runs the oracle's check itself, in the oracle's environment, on a 2 x 2 description the
/// wheel builds, against stand-ins for a buffer that ends with the pixels, which is refused,
/// and one that ends with the last row.
#[test]
fn packed_rows_are_checked_where_the_library_reads_them() {
    const SCRIPT: &str = r#"
import sys
import numpy as np
import PyOpenColorIO as OCIO
from ocio_oracle import commands, image

x_stride, y_stride, offset, *sizes = map(int, sys.argv[1:])
desc = OCIO.PackedImageDesc(np.zeros(16, np.float32), width=2, height=2, numChannels=4,
                            bitDepth=OCIO.BIT_DEPTH_F32, chanStrideBytes=OCIO.AutoStride,
                            xStrideBytes=x_stride, yStrideBytes=y_stride)
print(desc.isRGBAPacked())

class Buffer:
    def __init__(self, nbytes):
        self.nbytes = nbytes

spec = {"kind": "packed", "data": {"buffer": 0, "offset": offset}}
for size in sizes:
    try:
        image._check_inside(0, desc, spec, [Buffer(size)])
        print("accepted")
    except ValueError as exc:
        print(exc)
"#;
    // Row 1 is after row 0, as close as the x and y strides allow (ImageDesc.cpp:320), and the
    // first pixel is as close to the buffer's start as the second allows.
    let x_stride: i64 = 16 - (1 << 32);
    let y_stride = 2 * -x_stride;
    let offset = -x_stride;
    // The pixels end with the alpha of pixel (0, 1); its row, of 2 pixels of 16 bytes, goes 16
    // bytes further.
    let pixels_end = offset + y_stride + 16;
    let rows_end = offset + y_stride + 2 * 16;
    let args = [x_stride, y_stride, offset, pixels_end, rows_end].map(|v| v.to_string());
    let lines = oracle_python(SCRIPT, &args);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0], "True", "the wheel calls the image packed");
    let span = format!("its packed rows span bytes [{offset}, {rows_end})");
    assert!(lines[1].contains(&span), "{}", lines[1]);
    assert_eq!(lines[2], "accepted");
}

/// The scanline helper sizes its row buffers with 4 * width in a C long, indexes rows with an
/// `int`, and computes a row's first pixel index y * width in a C long (ScanlineHelper.cpp:72,
/// 78, 104, 146, 173; ScanlineHelper.h:92 @ v2.5.2). The oracle refuses the applies that would
/// overflow them, and nothing short of that: images 2^29 or more pixels wide, images of 2^31
/// or more rows, and RGBA-packed sources of 2^31 or more pixels into images that aren't. The
/// refusal test sends a width past the limit (it crashed the wheel on Windows); the other
/// sizes need a 64-bit C long or 8 GiB of pixels, so this runs the oracle's check itself, in
/// the oracle's environment, on stand-ins for descriptions, at each limit and one short of it.
#[test]
fn sizes_past_the_scanline_helpers_integers_are_refused() {
    const SCRIPT: &str = r#"
import json
import sys
from ocio_oracle import commands, image

class Desc:
    def __init__(self, width, height, packed):
        self.width, self.height, self.packed = width, height, packed

    def getWidth(self):
        return self.width

    def getHeight(self):
        return self.height

    def isRGBAPacked(self):
        return self.packed

for descs, apply in json.loads(sys.argv[1]):
    try:
        image._check_sizes([Desc(*desc) for desc in descs], apply)
        print("accepted")
    except ValueError as exc:
        print(exc)
"#;
    let (wide, tall, pixels) = (1i64 << 29, 1i64 << 31, 1i64 << 31);
    // Two images, the first applied to the second.
    let onto = |src: (i64, i64, bool), dst: (i64, i64, bool)| (vec![src, dst], vec![0, 1]);
    let in_place = |image: (i64, i64, bool)| (vec![image], vec![0]);
    let cases = [
        (
            "one pixel short of the width limit",
            in_place((wide - 1, 1, false)),
            false,
        ),
        ("at the width limit", in_place((wide, 1, false)), true),
        (
            "a destination at the width limit",
            onto((1, 1, false), (wide, 1, false)),
            true,
        ),
        (
            "one row short of the row limit",
            in_place((1, tall - 1, false)),
            false,
        ),
        ("at the row limit", in_place((1, tall, false)), true),
        (
            "a packed source one pixel short of the limit, into a planar image",
            onto((1, pixels - 1, true), (1, pixels - 1, false)),
            false,
        ),
        (
            "a packed source at the limit, into a planar image",
            onto((2, pixels / 2, true), (2, pixels / 2, false)),
            true,
        ),
        (
            "a packed source at the limit, into a packed image",
            onto((2, pixels / 2, true), (2, pixels / 2, true)),
            false,
        ),
        (
            "a planar source at the limit, into a planar image",
            onto((2, pixels / 2, false), (2, pixels / 2, false)),
            false,
        ),
        (
            "a planar source at the limit, into a packed image",
            onto((2, pixels / 2, false), (2, pixels / 2, true)),
            false,
        ),
        (
            "a packed image at the limit, in place",
            in_place((2, pixels / 2, true)),
            false,
        ),
    ];
    let requests: Vec<_> = cases
        .iter()
        .map(|(_, request, _)| request.clone())
        .collect();
    let lines = oracle_python(SCRIPT, &[json!(requests).to_string()]);
    assert_eq!(lines.len(), cases.len(), "{lines:?}");
    for ((label, _, refused), line) in cases.iter().zip(&lines) {
        if *refused {
            assert!(line.contains("refuses to apply"), "{label}: {line}");
        } else {
            assert_eq!(line, "accepted", "{label}");
        }
    }
}

/// The processor keys reach both commands (the verifier's N15 and N16): a config with a source
/// and a destination color space gives the pixels of the transform between them, and a
/// direction inverts a transform, for `image_apply` and for `image_apply_rgb`.
#[test]
fn processor_keys_reach_both_commands() {
    let matrix =
        json!({"class": "MatrixTransform", "args": {"offset": [0.125, -0.25, 0.0625, 0.5]}});
    let log = json!({"class": "LogTransform", "args": {"base": 2.0}});
    let config = json!({"yaml": "ocio_profile_version: 2
roles:
  default: ref
colorspaces:
  - !<ColorSpace>
    name: ref
  - !<ColorSpace>
    name: graded
    from_scene_reference: !<MatrixTransform> {offset: [0.125, -0.25, 0.0625, 0.5]}
"});
    let processors = [
        json!({"config": config, "src": "ref", "dst": "graded"}),
        json!({"transform": matrix}),
        json!({"transform": log, "direction": "TRANSFORM_DIR_INVERSE"}),
        json!({"transform": log}),
    ];
    let values = [0.5f32, 0.25, 1.5, 1.0, -0.125, 2.0, 0.75, 0.5];
    let images: Vec<Request> = processors
        .iter()
        .map(|p| {
            let mut request = Request::new(p.clone());
            let buffer = request.buffer(Buffer::Bytes(f32_to_bytes(&values)));
            let image = request.image(Packed::new(Data::at(buffer, 0), 2, 1, Channels::Count(4)));
            request.apply = vec![image];
            request
        })
        .collect();
    let rgbs: Vec<RgbRequest> = processors
        .iter()
        .map(|p| RgbRequest {
            processor: p.clone(),
            rgba: true,
            input: RgbInput::array(f32_to_bytes(&values), "float32"),
        })
        .collect();
    let mut calls: Vec<BatchCall<'_>> = images.iter().map(Request::call).collect();
    calls.extend(rgbs.iter().map(RgbRequest::call));
    let mut responses = batch(&calls);
    let rgb_responses = responses.split_off(images.len());
    let image_outputs: Vec<Vec<u8>> = responses
        .into_iter()
        .zip(&images)
        .map(|(response, request)| {
            let reply = request.reply(response);
            assert!(reply.raised().is_none(), "{}", reply.result);
            reply.buffers[0].clone()
        })
        .collect();
    let rgb_outputs: Vec<Vec<u8>> = rgb_responses
        .into_iter()
        .map(|response| {
            let reply = RgbReply::from_response(response);
            assert!(reply.raised().is_none(), "{}", reply.result);
            reply.output.expect("an output")
        })
        .collect();
    for outputs in [&image_outputs, &rgb_outputs] {
        assert_bytes_eq(
            "the config's source and destination",
            &outputs[1],
            &outputs[0],
        );
        assert_ne!(outputs[2], outputs[3], "the inverse direction");
    }
    assert_eq!(image_outputs, rgb_outputs);
}

/// A request's reply is the same on every run, alone or in a batch, bytes included: nothing
/// the commands return depends on where the oracle's buffers are or on memory it didn't write,
/// so batching and the oracle's cache (keyed by the request) stay valid.
#[test]
fn replies_are_the_same_on_every_run() {
    use BitDepth::{F16, F32, Uint16};
    let layouts = layouts();
    let size = (17, 3);
    let input = pixels(F32, size.0 * size.1, 31);
    let mut padded = Request::new(processor(F32, F32));
    let (src, _) = add(
        &mut padded,
        &layouts[10],
        F32,
        size,
        Content::Pixels(&input),
    );
    let (dst, _) = add(&mut padded, &layouts[18], F32, size, Content::Prefill);
    padded.apply = vec![src, dst];
    padded.data_getters = true;
    let input = pixels(Uint16, size.0 * size.1, 32);
    let mut planar = Request::new(processor(Uint16, Uint16));
    let (image, _) = add(
        &mut planar,
        &layouts[20],
        Uint16,
        size,
        Content::Pixels(&input),
    );
    planar.apply = vec![image];
    planar.data_getters = true;
    let list = RgbRequest {
        processor: processor(F32, F32),
        rgba: true,
        input: RgbInput::list(&[0.5, -0.25, 1e30, 0.0, 2.0, 0.125, 7.0, 1.0]),
    };
    let array = RgbRequest {
        processor: processor(F16, F16),
        rgba: false,
        input: RgbInput::array(pixels(F16, 9, 33)[..9 * 3 * 2].to_vec(), "float16"),
    };
    let calls = vec![padded.call(), planar.call(), list.call(), array.call()];
    let runs = [
        Oracle::get().batch(&calls, false),
        Oracle::get().batch(&calls, false),
    ];
    for (i, call) in calls.iter().enumerate() {
        let alone = Oracle::get().call_uncached(call.cmd, call.args.clone(), &call.blobs);
        assert!(
            alone.result.get("exception").is_none(),
            "call {i}: {}",
            alone.result
        );
        for run in &runs {
            let batched = run[i].as_ref().unwrap_or_else(|e| panic!("call {i}: {e}"));
            assert_eq!(batched.result, alone.result, "call {i}");
            assert_eq!(batched.blobs, alone.blobs, "call {i}");
        }
    }
}
