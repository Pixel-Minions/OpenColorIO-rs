// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Shared helpers of the image tests: the test kit's image specs (`ocio_testkit::image`), which
//! the oracle's `image_apply` takes, as the port's constructors take them, and the port's
//! getters as the oracle reports the wheel's.

use std::hint::black_box;
use std::sync::Arc;

use ocio_ops::Result;
use ocio_ops::cpu_processor::create_generic_bit_depth_helper;
use ocio_ops::image_desc::{
    AUTO_STRIDE, At, Bytes, ImageDesc, ImageDescMut, PackedImageDesc, PixelData, PlanarImageDesc,
};
use ocio_ops::op::CpuOp;
use ocio_ops::open_color_types::{BitDepth, ChannelOrdering, TransformDirection};
use ocio_ops::ops::log::log_op_cpu::get_log_renderer;
use ocio_ops::ops::log::log_op_data::LogOpData;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{
    Buffer, ChannelOrder, Channels, Data, Image, Packed, Planar, Request, Stride, channel_bytes,
};
use serde_json::{Value, json};

/// The bit depths the CPU processor takes.
pub(crate) const DEPTHS: [Depth; 6] = [
    Depth::Uint8,
    Depth::Uint10,
    Depth::Uint12,
    Depth::Uint16,
    Depth::F16,
    Depth::F32,
];

/// The port's bit depth for the test kit's.
pub(crate) fn port_depth(depth: Depth) -> BitDepth {
    match depth {
        Depth::Uint8 => BitDepth::Uint8,
        Depth::Uint10 => BitDepth::Uint10,
        Depth::Uint12 => BitDepth::Uint12,
        Depth::Uint16 => BitDepth::Uint16,
        Depth::F16 => BitDepth::F16,
        Depth::F32 => BitDepth::F32,
    }
}

/// A bit depth's name in the Python API.
pub(crate) fn depth_name(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Unknown => "BIT_DEPTH_UNKNOWN",
        BitDepth::Uint8 => "BIT_DEPTH_UINT8",
        BitDepth::Uint10 => "BIT_DEPTH_UINT10",
        BitDepth::Uint12 => "BIT_DEPTH_UINT12",
        BitDepth::Uint14 => "BIT_DEPTH_UINT14",
        BitDepth::Uint16 => "BIT_DEPTH_UINT16",
        BitDepth::Uint32 => "BIT_DEPTH_UINT32",
        BitDepth::F16 => "BIT_DEPTH_F16",
        BitDepth::F32 => "BIT_DEPTH_F32",
    }
}

/// The port's channel ordering for the test kit's.
pub(crate) fn port_order(order: ChannelOrder) -> ChannelOrdering {
    match order {
        ChannelOrder::Rgba => ChannelOrdering::Rgba,
        ChannelOrder::Bgra => ChannelOrdering::Bgra,
        ChannelOrder::Abgr => ChannelOrdering::Abgr,
        ChannelOrder::Rgb => ChannelOrdering::Rgb,
        ChannelOrder::Bgr => ChannelOrdering::Bgr,
    }
}

/// A channel ordering's name in the Python API.
pub(crate) fn order_name(order: ChannelOrdering) -> &'static str {
    match order {
        ChannelOrdering::Rgba => "CHANNEL_ORDERING_RGBA",
        ChannelOrdering::Bgra => "CHANNEL_ORDERING_BGRA",
        ChannelOrdering::Abgr => "CHANNEL_ORDERING_ABGR",
        ChannelOrdering::Rgb => "CHANNEL_ORDERING_RGB",
        ChannelOrdering::Bgr => "CHANNEL_ORDERING_BGR",
    }
}

/// A stride argument for the port.
pub(crate) fn port_stride(stride: Stride) -> isize {
    match stride {
        Stride::Auto => AUTO_STRIDE,
        Stride::Bytes(bytes) => bytes as isize,
    }
}

/// The supported bit depth of a spec.
pub(crate) fn supported(depth: ocio_testkit::image::Depth) -> Depth {
    match depth {
        ocio_testkit::image::Depth::Supported(depth) => depth,
        other => panic!("the port's constructors take a BitDepth, not {other:?}"),
    }
}

/// The port's description of `spec` over `data`: the constructor the spec names.
pub(crate) fn port_packed<S: PixelData>(
    spec: &Packed,
    data: S,
) -> Result<PackedImageDesc<S::Bytes>> {
    let (width, height) = (spec.width as usize, spec.height as usize);
    match (spec.channels, spec.layout) {
        (Channels::Count(n), None) => PackedImageDesc::new(data, width, height, n as usize),
        (Channels::Order(order), None) => {
            PackedImageDesc::with_channel_order(data, width, height, port_order(order))
        }
        (Channels::Count(n), Some((depth, [c, x, y]))) => PackedImageDesc::with_strides(
            data,
            width,
            height,
            n as usize,
            port_depth(supported(depth)),
            port_stride(c),
            port_stride(x),
            port_stride(y),
        ),
        (Channels::Order(order), Some((depth, [c, x, y]))) => {
            PackedImageDesc::with_channel_order_and_strides(
                data,
                width,
                height,
                port_order(order),
                port_depth(supported(depth)),
                port_stride(c),
                port_stride(x),
                port_stride(y),
            )
        }
        (Channels::OrderValue(value), _) => panic!("the port has no ordering {value}"),
    }
}

/// The getters every description has, as the oracle reports the wheel's.
pub(crate) fn getters(desc: &dyn ImageDesc) -> Value {
    json!({
        "getBitDepth": depth_name(desc.bit_depth()),
        "getWidth": desc.width(),
        "getHeight": desc.height(),
        "getXStrideBytes": desc.x_stride_bytes(),
        "getYStrideBytes": desc.y_stride_bytes(),
        "isRGBAPacked": desc.is_rgba_packed(),
        "isFloat": desc.is_float(),
    })
}

/// A packed description's getters, as the oracle reports the wheel's.
pub(crate) fn packed_getters<B: AsRef<[u8]>>(desc: &PackedImageDesc<B>) -> Value {
    let mut out = getters(desc);
    out["getChannelOrder"] = json!(order_name(desc.channel_order()));
    out["getNumChannels"] = json!(desc.num_channels());
    out["getChanStrideBytes"] = json!(desc.chan_stride_bytes());
    out
}

/// The bytes of a channel of a packed spec's bit depth (F32 without a layout).
pub(crate) fn packed_item(spec: &Packed) -> i64 {
    spec.layout
        .map_or(4, |(depth, _)| channel_bytes(supported(depth)) as i64)
}

/// The channels of a packed spec's pixels.
pub(crate) fn channel_count(spec: &Packed) -> i64 {
    match spec.channels {
        Channels::Count(n) => n,
        Channels::Order(order) => order.channels() as i64,
        Channels::OrderValue(_) => 4,
    }
}

/// The sum of terms, saturating.
pub(crate) fn saturating_sum(terms: &[i64]) -> i64 {
    terms.iter().copied().fold(0, i64::saturating_add)
}

/// How far a packed spec's channels can reach from its first pixel, in bytes, either way: its
/// strides (derived as upstream derives `AutoStride`) times the pixels and channels. It
/// saturates: the layouts with strides of 2^61 bytes and more are refused before any buffer
/// matters.
pub(crate) fn packed_reach(spec: &Packed) -> i64 {
    let item = packed_item(spec);
    let [c, x, y] = spec
        .layout
        .map_or([Stride::Auto; 3], |(_, strides)| strides);
    let c = match c {
        Stride::Auto => item,
        Stride::Bytes(c) => c,
    };
    let x = match x {
        Stride::Auto => c.saturating_mul(channel_count(spec)),
        Stride::Bytes(x) => x,
    };
    let y = match y {
        Stride::Auto => x.saturating_mul(spec.width),
        Stride::Bytes(y) => y,
    };
    let (width, height) = (spec.width.max(1), spec.height.max(1));
    saturating_sum(&[
        c.saturating_abs().saturating_mul(3),
        (width - 1).saturating_mul(x.saturating_abs()),
        (height - 1).saturating_mul(y.saturating_abs()),
        item,
    ])
}

/// The bytes of a channel of a planar spec's bit depth (F32 without a layout).
pub(crate) fn planar_item(spec: &Planar) -> i64 {
    spec.layout
        .map_or(4, |(depth, _)| channel_bytes(supported(depth)) as i64)
}

/// How far a planar spec's pixels can reach from a plane's first pixel, in bytes, either way,
/// as [`packed_reach`] says it for a packed spec.
pub(crate) fn planar_reach(spec: &Planar) -> i64 {
    let item = planar_item(spec);
    let [x, y] = spec
        .layout
        .map_or([Stride::Auto; 2], |(_, strides)| strides);
    let x = match x {
        Stride::Auto => item,
        Stride::Bytes(x) => x,
    };
    let y = match y {
        Stride::Auto => x.saturating_mul(spec.width),
        Stride::Bytes(y) => y,
    };
    let (width, height) = (spec.width.max(1), spec.height.max(1));
    saturating_sum(&[
        (width - 1).saturating_mul(x.saturating_abs()),
        (height - 1).saturating_mul(y.saturating_abs()),
        item,
    ])
}

/// The port's description of `spec`, whose planes are in `buffers` (the request's, by index):
/// separate planes, or planes in one buffer when they all name the same one.
pub(crate) fn port_planar<'a>(
    spec: &Planar,
    buffers: &'a [Vec<u8>],
) -> Result<PlanarImageDesc<&'a [u8]>> {
    let (width, height) = (spec.width as usize, spec.height as usize);
    let (bit_depth, [x, y]) = match spec.layout {
        Some((depth, [x, y])) => (
            port_depth(supported(depth)),
            [port_stride(x), port_stride(y)],
        ),
        None => (BitDepth::F32, [AUTO_STRIDE; 2]),
    };
    let first = spec.planes[0].buffer;
    if spec.planes.iter().all(|plane| plane.buffer == first) {
        let offset = |k: usize| spec.planes[k].offset;
        return PlanarImageDesc::in_one_buffer(
            Bytes(&buffers[first][..]),
            offset(0),
            offset(1),
            offset(2),
            (spec.planes.len() == 4).then(|| offset(3)),
            width,
            height,
            bit_depth,
            x,
            y,
        );
    }
    let plane = |k: usize| {
        At(
            Bytes(&buffers[spec.planes[k].buffer][..]),
            spec.planes[k].offset,
        )
    };
    let alpha = (spec.planes.len() == 4).then(|| plane(3));
    match spec.layout {
        None => PlanarImageDesc::new(plane(0), plane(1), plane(2), alpha, width, height),
        Some(_) => PlanarImageDesc::with_strides(
            plane(0),
            plane(1),
            plane(2),
            alpha,
            width,
            height,
            bit_depth,
            x,
            y,
        ),
    }
}

/// The processor the pixel tests apply: a LogTransform of base 2 (one LogOp), with the fast
/// log (`OPTIMIZATION_FAST_LOG_EXP_POW`) and no other optimization. So the wheel doesn't bake a
/// Lut1D for integer inputs (the separable-prefix bake, WP 2.5), and its log is plain
/// arithmetic, the same on every CPU.
pub(crate) fn log_processor(input: BitDepth, output: BitDepth) -> Value {
    json!({
        "transform": {"class": "LogTransform", "args": {"base": 2.0}},
        "optimization": "OPTIMIZATION_FAST_LOG_EXP_POW",
        "in_bitdepth": depth_name(input),
        "out_bitdepth": depth_name(output),
    })
}

/// A CPU engine: the op that converts from the input bit depth, the ops between, and the op that
/// converts to the output bit depth.
pub(crate) type Engine = (Arc<dyn CpuOp>, Vec<Arc<dyn CpuOp>>, Arc<dyn CpuOp>);

/// The CPU engine of [`log_processor`]: the op that converts from the input bit depth, the ops
/// between, and the op that converts to the output bit depth.
///
/// `CreateCPUEngine` (src/OpenColorIO/CPUProcessor.cpp:122-184 @ v2.5.2) gives the first op the
/// input conversion when the input is F32, and otherwise puts `BitDepthCast<in, F32>` before
/// it; with one op, the output conversion is `BitDepthCast<F32, out>`. The LogOp's renderer is
/// `GetLogRenderer(data, fastLogExpPow)` with the data `LogTransform(base=2.0)` holds
/// (`LogOpData(2.0, TRANSFORM_DIR_FORWARD)`, src/OpenColorIO/transforms/LogTransform.cpp:25-28;
/// `BuildLogOp` clones it, src/OpenColorIO/ops/log/LogOp.cpp:212-221 @ v2.5.2).
pub(crate) fn log_engine(input: BitDepth, output: BitDepth) -> Engine {
    let data = LogOpData::new(2.0, TransformDirection::Forward);
    let log = get_log_renderer(&black_box(data), true);
    let (first, ops) = if input == BitDepth::F32 {
        (log, Vec::new())
    } else {
        let cast =
            create_generic_bit_depth_helper(input, BitDepth::F32).expect("a supported depth");
        (cast, vec![log])
    };
    let last = create_generic_bit_depth_helper(BitDepth::F32, output).expect("a supported depth");
    (first, ops, last)
}

/// `bytes` bytes of channel values of `depth`, seeded: floats of every kind for F32 (specials,
/// and uniform values), any bits for F16 and 16-bit integers, and codes up to the maximum for
/// 8, 10 and 12 bits (the oracle refuses larger 10- and 12-bit codes, which the wheel would look
/// up outside a table).
pub(crate) fn source_bytes(depth: BitDepth, bytes: usize, seed: u64) -> Vec<u8> {
    let specials = ocio_testkit::probe::specials();
    let mut rng = ocio_testkit::probe::Rng::new(seed);
    let mut out = Vec::with_capacity(bytes + 4);
    let mut k = 0usize;
    while out.len() < bytes {
        let bits = rng.next_u64();
        match depth {
            BitDepth::F32 => {
                let value = if k.is_multiple_of(2) {
                    specials[(k / 2) % specials.len()]
                } else {
                    rng.uniform(-0.5, 2.0)
                };
                out.extend_from_slice(&value.to_ne_bytes());
            }
            BitDepth::F16 | BitDepth::Uint16 => out.extend_from_slice(&(bits as u16).to_ne_bytes()),
            BitDepth::Uint10 => out.extend_from_slice(&(bits as u16 & 0x3ff).to_ne_bytes()),
            BitDepth::Uint12 => out.extend_from_slice(&(bits as u16 & 0xfff).to_ne_bytes()),
            BitDepth::Uint8 => out.push(bits as u8),
            other => panic!("no channel values of {other:?}"),
        }
        k += 1;
    }
    out.truncate(bytes);
    out
}

/// The buffers at `indices` of `all`, to write, in the order of `indices`, which must be
/// distinct.
pub(crate) fn buffers_mut<'a>(all: &'a mut [Vec<u8>], indices: &[usize]) -> Vec<&'a mut [u8]> {
    let mut picked: Vec<(usize, &'a mut [u8])> = all
        .iter_mut()
        .enumerate()
        .filter(|(index, _)| indices.contains(index))
        .map(|(index, buffer)| (index, buffer.as_mut_slice()))
        .collect();
    indices
        .iter()
        .map(|index| {
            let at = picked
                .iter()
                .position(|(i, _)| i == index)
                .expect("distinct buffers");
            picked.swap_remove(at).1
        })
        .collect()
}

/// Calls `$f::<I, O>($args...)` with the channel types `I` and `O` of bit depths `$input` and
/// `$output`, as `CreateScanlineHelper` instantiates `GenericScanlineHelper`
/// (src/OpenColorIO/CPUProcessor.cpp:187-238 @ v2.5.2).
#[allow(unused_macros)] // Each test crate uses a subset.
macro_rules! with_channel_types {
    ($input:expr, $output:expr, $f:ident($($args:expr),*)) => {{
        use ocio_ops::open_color_types::BitDepth as B;
        macro_rules! out {
            ($i:ty) => {
                match $output {
                    B::Uint8 => $f::<$i, u8>($($args),*),
                    B::Uint10 | B::Uint12 | B::Uint16 => $f::<$i, u16>($($args),*),
                    B::F16 => $f::<$i, half::f16>($($args),*),
                    B::F32 => $f::<$i, f32>($($args),*),
                    other => panic!("no channel type for {other:?}"),
                }
            };
        }
        match $input {
            B::Uint8 => out!(u8),
            B::Uint10 | B::Uint12 | B::Uint16 => out!(u16),
            B::F16 => out!(half::f16),
            B::F32 => out!(f32),
            other => panic!("no channel type for {other:?}"),
        }
    }};
}
#[allow(unused_imports)]
pub(crate) use with_channel_types;

/// A port description of either kind.
#[derive(Debug)]
pub(crate) enum PortImage<B: AsRef<[u8]>> {
    /// A packed image.
    Packed(PackedImageDesc<B>),
    /// A planar image.
    Planar(PlanarImageDesc<B>),
}

impl<B: AsRef<[u8]>> PortImage<B> {
    /// The description.
    pub(crate) fn desc(&self) -> &dyn ImageDesc {
        match self {
            PortImage::Packed(desc) => desc,
            PortImage::Planar(desc) => desc,
        }
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> PortImage<B> {
    /// The description, for the CPU engine to write.
    pub(crate) fn desc_mut(&mut self) -> &mut dyn ImageDescMut {
        match self {
            PortImage::Packed(desc) => desc,
            PortImage::Planar(desc) => desc,
        }
    }
}

/// The port's description of `image`, whose request buffers `take` gives by index (each at
/// most once): packed, planes in separate buffers, or planes in one buffer.
pub(crate) fn port_image<B: AsRef<[u8]>>(
    image: &Image,
    mut take: impl FnMut(usize) -> Bytes<B>,
) -> Result<PortImage<B>>
where
    Bytes<B>: PixelData<Bytes = B>,
{
    let spec = match image {
        Image::Packed(spec) => {
            let data = At(take(spec.data.buffer), spec.data.offset);
            return port_packed(spec, data).map(PortImage::Packed);
        }
        Image::Planar(spec) => spec,
    };
    let (width, height) = (spec.width as usize, spec.height as usize);
    let (bit_depth, [x, y]) = match spec.layout {
        Some((depth, [x, y])) => (
            port_depth(supported(depth)),
            [port_stride(x), port_stride(y)],
        ),
        None => (BitDepth::F32, [AUTO_STRIDE; 2]),
    };
    let first = spec.planes[0].buffer;
    let offset = |k: usize| spec.planes[k].offset;
    let desc = if spec.planes.iter().all(|plane| plane.buffer == first) {
        PlanarImageDesc::in_one_buffer(
            take(first),
            offset(0),
            offset(1),
            offset(2),
            (spec.planes.len() == 4).then(|| offset(3)),
            width,
            height,
            bit_depth,
            x,
            y,
        )
    } else {
        let mut plane = |k: usize| At(take(spec.planes[k].buffer), offset(k));
        let (r, g, b) = (plane(0), plane(1), plane(2));
        let alpha = (spec.planes.len() == 4).then(|| plane(3));
        match spec.layout {
            None => PlanarImageDesc::new(r, g, b, alpha, width, height),
            Some(_) => {
                PlanarImageDesc::with_strides(r, g, b, alpha, width, height, bit_depth, x, y)
            }
        }
    };
    desc.map(PortImage::Planar)
}

/// What a destination buffer holds before an apply.
pub(crate) const PREFILL: [u8; 5] = [0xa5, 0x5a, 0xc3, 0x3c, 0x96];

/// The pairs of input and output bit depths of the pixel tests.
pub(crate) const PAIRS: [(Depth, Depth); 12] = [
    (Depth::F32, Depth::F32),
    (Depth::Uint8, Depth::Uint8),
    (Depth::Uint10, Depth::Uint10),
    (Depth::Uint12, Depth::Uint12),
    (Depth::Uint16, Depth::Uint16),
    (Depth::F16, Depth::F16),
    (Depth::F32, Depth::Uint8),
    (Depth::Uint8, Depth::F32),
    (Depth::F32, Depth::F16),
    (Depth::F16, Depth::F32),
    (Depth::Uint16, Depth::Uint10),
    (Depth::Uint12, Depth::F16),
];

/// A layout, for any bit depth and size.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Shape {
    /// A packed image in `channels`, with strides from the channel size plus padding: bytes
    /// after each channel, pixel and row. A negative pixel padding flips the pixels (right to
    /// left); a negative row padding flips the rows (bottom-up).
    Packed(Channels, [i64; 3]),
    /// Planes R, G, B (and A), separate or in one buffer, rows flipped or not.
    Planar {
        alpha: bool,
        one_buffer: bool,
        flipped: bool,
    },
}

/// The layouts the CPU engine packs channel by channel: none is RGBA-packed.
pub(crate) const GENERIC_SHAPES: [Shape; 12] = [
    Shape::Packed(Channels::Count(3), [0, 0, 0]),
    Shape::Packed(Channels::Order(ChannelOrder::Bgra), [0, 0, 0]),
    Shape::Packed(Channels::Order(ChannelOrder::Abgr), [0, 0, 0]),
    Shape::Packed(Channels::Order(ChannelOrder::Bgr), [0, 0, 0]),
    // RGBA with bytes after each channel, or after each pixel, and padded rows.
    Shape::Packed(Channels::Count(4), [2, 0, 8]),
    Shape::Packed(Channels::Count(4), [0, 4, 0]),
    // BGRA bottom-up; RGB right to left.
    Shape::Packed(Channels::Order(ChannelOrder::Bgra), [0, 0, -1]),
    Shape::Packed(Channels::Count(3), [0, -1, 0]),
    Shape::Planar {
        alpha: false,
        one_buffer: false,
        flipped: false,
    },
    Shape::Planar {
        alpha: true,
        one_buffer: false,
        flipped: false,
    },
    Shape::Planar {
        alpha: true,
        one_buffer: true,
        flipped: false,
    },
    Shape::Planar {
        alpha: false,
        one_buffer: false,
        flipped: true,
    },
];

/// RGBA-packed layouts, which the CPU engine processes a whole row at a time: tight, with
/// padded rows, and bottom-up.
pub(crate) const PACKED_SHAPES: [Shape; 3] = [
    Shape::Packed(Channels::Count(4), [0, 0, 0]),
    Shape::Packed(Channels::Order(ChannelOrder::Rgba), [0, 0, 24]),
    Shape::Packed(Channels::Count(4), [0, 0, -1]),
];

/// Adds a buffer of `size` bytes: source values of `depth` (seeded by `seed`), or the prefill.
fn add_buffer(request: &mut Request, depth: Depth, size: usize, seed: Option<u64>) -> usize {
    request.buffer(match seed {
        Some(seed) => Buffer::Bytes(source_bytes(port_depth(depth), size, seed)),
        None => Buffer::fill(size, &PREFILL),
    })
}

/// Adds an image of `shape` and `depth` to `request`, over new buffers that hold its pixels in
/// their middle: source values when `seed` is given, the prefill otherwise.
pub(crate) fn add_image(
    request: &mut Request,
    shape: Shape,
    depth: Depth,
    (width, height): (i64, i64),
    seed: Option<u64>,
) -> Image {
    let item = channel_bytes(depth) as i64;
    match shape {
        Shape::Packed(channels, [chan_pad, pixel_pad, row_pad]) => {
            let n = match channels {
                Channels::Count(n) => n,
                Channels::Order(order) => order.channels() as i64,
                Channels::OrderValue(_) => 4,
            };
            let chan = item + chan_pad;
            let x = if pixel_pad < 0 {
                -(chan * n)
            } else {
                chan * n + pixel_pad
            };
            let y = if row_pad < 0 {
                -(x.abs() * width)
            } else {
                x.abs() * width + row_pad
            };
            let mut spec = Packed::new(Data::at(0, 0), width, height, channels).layout(
                depth,
                [Stride::Bytes(chan), Stride::Bytes(x), Stride::Bytes(y)],
            );
            let reach = packed_reach(&spec) as usize;
            let buffer = add_buffer(request, depth, 2 * reach + 64, seed);
            spec.data = Data::at(buffer, reach + 32);
            request.image(spec.clone());
            Image::Packed(spec)
        }
        Shape::Planar {
            alpha,
            one_buffer,
            flipped,
        } => {
            let y = if flipped {
                -(item * width)
            } else {
                item * width
            };
            let mut spec = Planar::new(Vec::new(), width, height)
                .layout(depth, [Stride::Auto, Stride::Bytes(y)]);
            let reach = planar_reach(&spec) as usize;
            let span = 2 * reach + 64;
            let planes = if alpha { 4 } else { 3 };
            spec.planes = if one_buffer {
                let buffer = add_buffer(request, depth, planes * span, seed);
                (0..planes)
                    .map(|k| Data::at(buffer, k * span + reach + 32))
                    .collect()
            } else {
                (0..planes)
                    .map(|k| {
                        let seed = seed.map(|s| s + k as u64);
                        Data::at(add_buffer(request, depth, span, seed), reach + 32)
                    })
                    .collect()
            };
            request.image(spec.clone());
            Image::Planar(spec)
        }
    }
}

/// The request buffers an image's channel positions index, in their order.
pub(crate) fn buffer_indices(image: &Image) -> Vec<usize> {
    match image {
        Image::Packed(spec) => vec![spec.data.buffer],
        Image::Planar(spec) => {
            let first = spec.planes[0].buffer;
            if spec.planes.iter().all(|p| p.buffer == first) {
                vec![first]
            } else {
                spec.planes.iter().map(|p| p.buffer).collect()
            }
        }
    }
}
