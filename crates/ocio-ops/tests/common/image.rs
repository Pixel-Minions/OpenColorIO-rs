// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Shared helpers of the image tests: the test kit's image specs (`ocio_testkit::image`), which
//! the oracle's `image_apply` takes, as the port's constructors take them, and the port's
//! getters as the oracle reports the wheel's.

use ocio_ops::Result;
use ocio_ops::image_desc::{
    AUTO_STRIDE, At, Bytes, ImageDesc, PackedImageDesc, PixelData, PlanarImageDesc,
};
use ocio_ops::open_color_types::{BitDepth, ChannelOrdering};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::image::{ChannelOrder, Channels, Packed, Planar, Stride, channel_bytes};
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
