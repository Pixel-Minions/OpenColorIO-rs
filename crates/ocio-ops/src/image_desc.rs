// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Image descriptions: a port of `src/OpenColorIO/ImageDesc.cpp` @ v2.5.2, with `AutoStride`
//! and the `ImageDesc`, `PackedImageDesc` and `PlanarImageDesc` classes of
//! `include/OpenColorIO/OpenColorIO.h`, and `GenericImageDesc` (declared in `ImagePacking.h`).
//!
//! Upstream describes an image with `void *` pointers and byte strides, and never checks them
//! against a buffer. The port describes the same layouts over borrowed memory
//! (`docs/architecture.md`, "Image descriptions"):
//! - The memory is a slice of channel values (`&[T]` or `&mut [T]`, `T` one of `u8`, `u16`,
//!   `half::f16`, `f32`) or raw bytes ([`Bytes`]), viewed as bytes without copying
//!   (`zerocopy`). [`At`] puts the first pixel inside the memory, as a C++ pointer into an
//!   array does, so that negative strides reach the bytes before it.
//! - A channel's position is a byte offset into its buffer ([`ChannelPos`]) instead of a
//!   pointer. The offsets and strides are computed with upstream's integer arithmetic, which
//!   wraps where C++ overflows.
//! - The constructors make upstream's checks in upstream's order, with its messages. After
//!   them, a layout that would make the CPU engine touch a byte outside its memory is refused
//!   (deviation D-2, `docs/deviations.md`); upstream reads or writes outside it.
//! - A typed slice must hold the bit depth's channel type. That check and its message are the
//!   Python binding's (`checkBufferType`), which runs before the library's checks.

use core::ffi::c_long;
use std::fmt;
use std::sync::Arc;

use zerocopy::IntoBytes;

use crate::bit_depth_utils::{
    BitDepthInfo, F16, F32, Uint8, Uint10, Uint12, Uint16, get_channel_size_in_bytes,
};
use crate::exception::{Exception, Result};
use crate::op::CpuOp;
use crate::open_color_types::{BitDepth, ChannelOrdering, bit_depth_to_string};

/// A stride argument that asks the constructor to derive the stride.
///
/// Port of `AutoStride` (include/OpenColorIO/OpenColorIO.h:3092 @ v2.5.2):
/// `std::numeric_limits<ptrdiff_t>::min()`.
pub const AUTO_STRIDE: isize = isize::MIN;

/// The channel type of a typed slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Element {
    U8,
    U16,
    F16,
    F32,
}

impl Element {
    /// The element type the Python binding requires for a bit depth's buffers, or `None` for
    /// the bit depths it refuses (`bitDepthToDtype`, src/bindings/python/PyUtils.cpp:56-86
    /// @ v2.5.2).
    fn for_bit_depth(bit_depth: BitDepth) -> Option<Element> {
        match bit_depth {
            BitDepth::Uint8 => Some(Element::U8),
            BitDepth::Uint10 | BitDepth::Uint12 | BitDepth::Uint16 => Some(Element::U16),
            BitDepth::F16 => Some(Element::F16),
            BitDepth::F32 => Some(Element::F32),
            BitDepth::Unknown | BitDepth::Uint14 | BitDepth::Uint32 => None,
        }
    }

    /// The type's NumPy name, as the Python binding names a buffer it receives:
    /// `formatCodeToDtypeName(info.format, info.itemsize * 8)` of its format code, `B`, `H`,
    /// `e` or `f` (src/bindings/python/PyUtils.cpp:26-54 @ v2.5.2).
    fn name(self) -> &'static str {
        match self {
            Element::U8 => "uint8",
            Element::U16 => "uint16",
            Element::F16 => "float16",
            Element::F32 => "float32",
        }
    }
}

/// The Python binding's `checkBufferType` (src/bindings/python/PyUtils.cpp:165-180 @ v2.5.2),
/// for a typed slice: its channel type must be the bit depth's. Raw bytes pass; so do the bit
/// depths the binding refuses, for which the library's own check raises later. The message names
/// both types as NumPy does; the binding prints an expected unsigned type as `'u' (16-bit)`
/// (improvement candidate I-23).
fn check_buffer_type(element: Option<Element>, bit_depth: BitDepth) -> Result<()> {
    match (element, Element::for_bit_depth(bit_depth)) {
        (Some(received), Some(expected)) if received != expected => Err(Exception::new(format!(
            "Incompatible buffer format: expected {}, but received {}",
            expected.name(),
            received.name()
        ))),
        _ => Ok(()),
    }
}

/// Memory an image description can describe:
/// - `&[T]`, `&mut [T]`, `&Vec<T>` or `&mut Vec<T>`, with `T` the bit depth's channel type:
///   `u8` for `Uint8`; `u16` for `Uint10`, `Uint12` and `Uint16`; `half::f16` for `F16`; `f32`
///   for `F32`;
/// - [`Bytes`]: raw bytes of any bit depth;
/// - [`At`]: any of these with the first pixel inside the memory.
///
/// Shared memory (`&`) makes a description to read from; exclusive memory (`&mut`) one that the
/// CPU processor can also write ([`ImageDescMut`]).
pub trait PixelData {
    /// The memory as bytes: `&[u8]` or `&mut [u8]`.
    type Bytes: AsRef<[u8]>;

    /// The bytes, where the first pixel starts in them, and the channel type of a typed slice.
    #[doc(hidden)]
    fn into_parts(self) -> PixelParts<Self::Bytes>;
}

/// What a [`PixelData`] holds. Only the port reads it.
#[doc(hidden)]
#[derive(Debug)]
pub struct PixelParts<B> {
    bytes: B,
    origin: usize,
    element: Option<Element>,
}

macro_rules! typed_pixel_data {
    ($t:ty, $element:expr) => {
        impl<'a> PixelData for &'a [$t] {
            type Bytes = &'a [u8];
            fn into_parts(self) -> PixelParts<&'a [u8]> {
                PixelParts {
                    bytes: IntoBytes::as_bytes(self),
                    origin: 0,
                    element: Some($element),
                }
            }
        }

        impl<'a> PixelData for &'a mut [$t] {
            type Bytes = &'a mut [u8];
            fn into_parts(self) -> PixelParts<&'a mut [u8]> {
                PixelParts {
                    bytes: IntoBytes::as_mut_bytes(self),
                    origin: 0,
                    element: Some($element),
                }
            }
        }

        impl<'a> PixelData for &'a Vec<$t> {
            type Bytes = &'a [u8];
            fn into_parts(self) -> PixelParts<&'a [u8]> {
                self.as_slice().into_parts()
            }
        }

        impl<'a> PixelData for &'a mut Vec<$t> {
            type Bytes = &'a mut [u8];
            fn into_parts(self) -> PixelParts<&'a mut [u8]> {
                self.as_mut_slice().into_parts()
            }
        }
    };
}

typed_pixel_data!(u8, Element::U8);
typed_pixel_data!(u16, Element::U16);
typed_pixel_data!(half::f16, Element::F16);
typed_pixel_data!(f32, Element::F32);

/// Raw memory holding channels of any bit depth in the machine's byte order: what a C++ caller
/// passes as `void *`. For example a GPU readback, whose rows are padded.
#[derive(Debug, Clone, Copy)]
pub struct Bytes<B>(pub B);

impl<'a> PixelData for Bytes<&'a [u8]> {
    type Bytes = &'a [u8];
    fn into_parts(self) -> PixelParts<&'a [u8]> {
        PixelParts {
            bytes: self.0,
            origin: 0,
            element: None,
        }
    }
}

impl<'a> PixelData for Bytes<&'a mut [u8]> {
    type Bytes = &'a mut [u8];
    fn into_parts(self) -> PixelParts<&'a mut [u8]> {
        PixelParts {
            bytes: self.0,
            origin: 0,
            element: None,
        }
    }
}

/// Memory whose first pixel is `.1` bytes from its start: what a C++ caller passes as a pointer
/// into an array (`&buffer[i]`). Negative strides then reach the bytes before that pixel, as in
/// upstream's tests of bottom-up and right-to-left images.
#[derive(Debug, Clone, Copy)]
pub struct At<S>(pub S, pub usize);

impl<S: PixelData> PixelData for At<S> {
    type Bytes = S::Bytes;
    fn into_parts(self) -> PixelParts<S::Bytes> {
        let mut parts = self.0.into_parts();
        // An origin beyond `usize` is outside any buffer, and the bounds check refuses it.
        parts.origin = parts.origin.saturating_add(self.1);
        parts
    }
}

/// Where a channel of an image's first pixel is: which of the description's buffers and the
/// byte offset in it. A packed image has one buffer, 0; a planar image has one per plane (R, G,
/// B, A: 0 to 3), or one for all of them. It stands for the pointer that upstream's
/// `getRData()`, `getGData()`, `getBData()` and `getAData()` return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelPos {
    /// The buffer.
    pub buffer: usize,
    /// The byte offset in the buffer.
    pub offset: usize,
}

/// An image description without its memory: the state behind upstream's `ImageDesc` getters,
/// with the channels' positions instead of pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageLayout {
    bit_depth: BitDepth,
    width: c_long,
    height: c_long,
    x_stride_bytes: isize,
    y_stride_bytes: isize,
    r: ChannelPos,
    g: ChannelPos,
    b: ChannelPos,
    a: Option<ChannelPos>,
    is_rgba_packed: bool,
    is_float: bool,
}

impl ImageLayout {
    /// `getBitDepth()`.
    pub fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }

    /// `getWidth()`: the width to process.
    pub fn width(&self) -> usize {
        // The constructors refuse widths below 1.
        self.width as usize
    }

    /// `getHeight()`: the height to process.
    pub fn height(&self) -> usize {
        self.height as usize
    }

    /// The width as C++'s `long`, which the CPU engine computes with.
    pub fn width_long(&self) -> c_long {
        self.width
    }

    /// The height as C++'s `long`.
    pub fn height_long(&self) -> c_long {
        self.height
    }

    /// `getXStrideBytes()`: the bytes from a channel of a pixel to the same channel of the next
    /// pixel, resolved (never [`AUTO_STRIDE`]).
    pub fn x_stride_bytes(&self) -> isize {
        self.x_stride_bytes
    }

    /// `getYStrideBytes()`: the bytes from a pixel to the pixel at the same position in the
    /// next row, resolved.
    pub fn y_stride_bytes(&self) -> isize {
        self.y_stride_bytes
    }

    /// `getRData()`: where the red channel of the first pixel is.
    pub fn r_data(&self) -> ChannelPos {
        self.r
    }

    /// `getGData()`.
    pub fn g_data(&self) -> ChannelPos {
        self.g
    }

    /// `getBData()`.
    pub fn b_data(&self) -> ChannelPos {
        self.b
    }

    /// `getAData()`: `None` where upstream returns null (no alpha channel).
    pub fn a_data(&self) -> Option<ChannelPos> {
        self.a
    }

    /// `isRGBAPacked()`: whether the image is packed RGBA with nothing between its channels
    /// and pixels, so that the CPU engine processes whole rows in place.
    pub fn is_rgba_packed(&self) -> bool {
        self.is_rgba_packed
    }

    /// `isFloat()`: whether the image is F32 with 4-byte channels.
    pub fn is_float(&self) -> bool {
        self.is_float
    }
}

mod sealed {
    /// Only this crate's image descriptions implement [`super::ImageDesc`].
    pub trait Sealed {}
}

/// What the CPU processor reads from any image description. Implemented by
/// [`PackedImageDesc`] and [`PlanarImageDesc`] only.
///
/// Port of `ImageDesc` (include/OpenColorIO/OpenColorIO.h:3094-3146 @ v2.5.2). Upstream lets
/// C++ code derive its own descriptions; the port doesn't, for now.
pub trait ImageDesc: sealed::Sealed + fmt::Debug {
    /// The description without its memory.
    fn layout(&self) -> &ImageLayout;

    /// Port of `getBitDepth()`.
    fn bit_depth(&self) -> BitDepth {
        self.layout().bit_depth()
    }

    /// Port of `getWidth()`.
    fn width(&self) -> usize {
        self.layout().width()
    }

    /// Port of `getHeight()`.
    fn height(&self) -> usize {
        self.layout().height()
    }

    /// Port of `getXStrideBytes()`.
    fn x_stride_bytes(&self) -> isize {
        self.layout().x_stride_bytes()
    }

    /// Port of `getYStrideBytes()`.
    fn y_stride_bytes(&self) -> isize {
        self.layout().y_stride_bytes()
    }

    /// Port of `getRData()`.
    fn r_data(&self) -> ChannelPos {
        self.layout().r_data()
    }

    /// Port of `getGData()`.
    fn g_data(&self) -> ChannelPos {
        self.layout().g_data()
    }

    /// Port of `getBData()`.
    fn b_data(&self) -> ChannelPos {
        self.layout().b_data()
    }

    /// Port of `getAData()`.
    fn a_data(&self) -> Option<ChannelPos> {
        self.layout().a_data()
    }

    /// Port of `isRGBAPacked()`.
    fn is_rgba_packed(&self) -> bool {
        self.layout().is_rgba_packed()
    }

    /// Port of `isFloat()`.
    fn is_float(&self) -> bool {
        self.layout().is_float()
    }
}

/// An image description whose memory the CPU processor can write: the image of an in-place
/// `apply`, or the destination of one. Descriptions of exclusive memory (`&mut`) implement it.
pub trait ImageDescMut: ImageDesc {}

/// Upstream's constructor argument for the channels: `numChannels` or `chanOrder`.
#[derive(Debug, Clone, Copy)]
enum Channels {
    Count(c_long),
    Order(ChannelOrdering),
}

/// A size as C++'s `long`. A size beyond it (possible on Windows, where `long` has 32 bits)
/// can't reach upstream; it becomes -1, an invalid size that upstream's own checks refuse at
/// their place in the order.
fn long(size: usize) -> c_long {
    c_long::try_from(size).unwrap_or(-1)
}

/// `size_of::<BitDepthInfo<BD>::Type>()` as a byte count.
fn item_size<BD: BitDepthInfo>() -> isize {
    size_of::<BD::Type>() as isize
}

/// Port of `PackedImageDesc` (include/OpenColorIO/OpenColorIO.h:3154-3237,
/// src/OpenColorIO/ImageDesc.cpp:118-588 @ v2.5.2): an image whose pixels hold their 3 or 4
/// channels side by side, in one buffer.
///
/// `B` is `&[u8]` for a description of shared memory and `&mut [u8]` for one of exclusive
/// memory; it follows from the [`PixelData`] the constructor gets.
pub struct PackedImageDesc<B> {
    data: B,
    data_offset: usize,
    chan_order: ChannelOrdering,
    num_channels: c_long,
    chan_stride_bytes: isize,
    layout: ImageLayout,
}

impl<B: AsRef<[u8]>> fmt::Debug for PackedImageDesc<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PackedImageDesc")
            .field("data_bytes", &self.data.as_ref().len())
            .field("data_offset", &self.data_offset)
            .field("chan_order", &self.chan_order)
            .field("num_channels", &self.num_channels)
            .field("chan_stride_bytes", &self.chan_stride_bytes)
            .field("layout", &self.layout)
            .finish()
    }
}

impl<B: AsRef<[u8]>> PackedImageDesc<B> {
    /// `PackedImageDesc(data, width, height, numChannels)`: F32 pixels of `num_channels`
    /// channels, RGBA (4) or RGB (3), with nothing between channels, pixels and rows.
    ///
    /// Port of `PackedImageDesc::PackedImageDesc(void *, long, long, long)`
    /// (src/OpenColorIO/ImageDesc.cpp:332-369 @ v2.5.2).
    pub fn new<S: PixelData<Bytes = B>>(
        data: S,
        width: usize,
        height: usize,
        num_channels: usize,
    ) -> Result<Self> {
        Self::construct(
            data.into_parts(),
            long(width),
            long(height),
            Channels::Count(long(num_channels)),
            None,
        )
    }

    /// `PackedImageDesc(data, width, height, chanOrder)`: F32 pixels in `chan_order`, with
    /// nothing between channels, pixels and rows.
    ///
    /// Port of `PackedImageDesc::PackedImageDesc(void *, long, long, ChannelOrdering)`
    /// (src/OpenColorIO/ImageDesc.cpp:371-411 @ v2.5.2).
    pub fn with_channel_order<S: PixelData<Bytes = B>>(
        data: S,
        width: usize,
        height: usize,
        chan_order: ChannelOrdering,
    ) -> Result<Self> {
        Self::construct(
            data.into_parts(),
            long(width),
            long(height),
            Channels::Order(chan_order),
            None,
        )
    }

    /// `PackedImageDesc(data, width, height, numChannels, bitDepth, chanStrideBytes,
    /// xStrideBytes, yStrideBytes)`: pixels of `num_channels` channels, RGBA (4) or RGB (3), of
    /// `bit_depth`, with the bytes between channels, pixels and rows given, each
    /// [`AUTO_STRIDE`] or a byte count (negative ones included).
    ///
    /// Port of `PackedImageDesc::PackedImageDesc(void *, long, long, long, BitDepth, ptrdiff_t,
    /// ptrdiff_t, ptrdiff_t)` (src/OpenColorIO/ImageDesc.cpp:462-507 @ v2.5.2).
    pub fn with_strides<S: PixelData<Bytes = B>>(
        data: S,
        width: usize,
        height: usize,
        num_channels: usize,
        bit_depth: BitDepth,
        chan_stride_bytes: isize,
        x_stride_bytes: isize,
        y_stride_bytes: isize,
    ) -> Result<Self> {
        Self::construct(
            data.into_parts(),
            long(width),
            long(height),
            Channels::Count(long(num_channels)),
            Some((
                bit_depth,
                [chan_stride_bytes, x_stride_bytes, y_stride_bytes],
            )),
        )
    }

    /// `PackedImageDesc(data, width, height, chanOrder, bitDepth, chanStrideBytes,
    /// xStrideBytes, yStrideBytes)`: pixels in `chan_order` of `bit_depth`, with the bytes
    /// between channels, pixels and rows given, each [`AUTO_STRIDE`] or a byte count.
    ///
    /// Port of `PackedImageDesc::PackedImageDesc(void *, long, long, ChannelOrdering, BitDepth,
    /// ptrdiff_t, ptrdiff_t, ptrdiff_t)` (src/OpenColorIO/ImageDesc.cpp:413-460 @ v2.5.2).
    pub fn with_channel_order_and_strides<S: PixelData<Bytes = B>>(
        data: S,
        width: usize,
        height: usize,
        chan_order: ChannelOrdering,
        bit_depth: BitDepth,
        chan_stride_bytes: isize,
        x_stride_bytes: isize,
        y_stride_bytes: isize,
    ) -> Result<Self> {
        Self::construct(
            data.into_parts(),
            long(width),
            long(height),
            Channels::Order(chan_order),
            Some((
                bit_depth,
                [chan_stride_bytes, x_stride_bytes, y_stride_bytes],
            )),
        )
    }

    /// The four constructors (src/OpenColorIO/ImageDesc.cpp:332-507 @ v2.5.2). `strides` is
    /// `None` for the two without a bit depth: F32, and the strides `AutoStride` gives.
    fn construct(
        parts: PixelParts<B>,
        width: c_long,
        height: c_long,
        channels: Channels,
        strides: Option<(BitDepth, [isize; 3])>,
    ) -> Result<Self> {
        let PixelParts {
            bytes,
            origin,
            element,
        } = parts;
        let bit_depth = strides.map_or(BitDepth::F32, |(bit_depth, _)| bit_depth);

        // The Python binding checks the buffer's type before the library sees it.
        check_buffer_type(element, bit_depth)?;

        let (chan_order, num_channels) = match channels {
            Channels::Count(4) => (ChannelOrdering::Rgba, 4),
            Channels::Count(3) => (ChannelOrdering::Rgb, 3),
            Channels::Count(_) => {
                return Err(Exception::new(
                    "PackedImageDesc Error: Invalid number of channels.",
                ));
            }
            // A value outside the enum ("Unknown channel ordering.") can't reach Rust.
            Channels::Order(
                order @ (ChannelOrdering::Rgba | ChannelOrdering::Bgra | ChannelOrdering::Abgr),
            ) => (order, 4),
            Channels::Order(order @ (ChannelOrdering::Rgb | ChannelOrdering::Bgr)) => (order, 3),
        };

        // The constructors without a bit depth use `sizeof(BitDepthInfo<BIT_DEPTH_F32>::Type)`
        // and derive every stride; the others ask `GetChannelSizeInBytes`, which refuses the bit
        // depths the CPU processor doesn't take, and derive the `AutoStride` ones.
        let (one_channel_in_bytes, [chan, x, y]) = match strides {
            None => (item_size::<F32>(), [AUTO_STRIDE; 3]),
            Some((bit_depth, strides)) => (get_channel_size_in_bytes(bit_depth)? as isize, strides),
        };
        let chan_stride = if chan == AUTO_STRIDE {
            one_channel_in_bytes
        } else {
            chan
        };
        let x_stride = if x == AUTO_STRIDE {
            chan_stride.wrapping_mul(num_channels as isize)
        } else {
            x
        };
        let y_stride = if y == AUTO_STRIDE {
            x_stride.wrapping_mul(width as isize)
        } else {
            y
        };

        let channels = packed_channel_offsets(chan_order, num_channels, chan_stride);
        let is_rgba_packed = packed_is_rgba_packed(bit_depth, &channels, chan_stride, x_stride)?;
        let is_float = chan_stride == item_size::<F32>() && bit_depth == BitDepth::F32;

        packed_validate(
            bytes.as_ref().is_empty(),
            width,
            height,
            bit_depth,
            num_channels,
            chan_stride,
            x_stride,
            y_stride,
        )?;

        // Deviation D-2: the bytes the CPU engine will touch must be in the buffer. An RGBA-packed
        // image is read and written in whole rows from its red channel.
        let item = get_channel_size_in_bytes(bit_depth)? as usize;
        let len = bytes.as_ref().len();
        let starts = channels.map(|offset| offset.map(|o| origin as i128 + o as i128));
        let outside = if is_rgba_packed {
            let red = starts[0].expect("a packed image has red");
            rows_reach_outside(len, red, item, width, height, y_stride)
        } else {
            starts.iter().flatten().any(|&start| {
                channel_reaches_outside(len, start, item, width, height, x_stride, y_stride)
            })
        };
        if outside {
            return Err(Exception::new(
                "PackedImageDesc Error: The strides and dimensions reach outside the image buffer.",
            ));
        }

        // Inside the buffer, so every start is an offset in it.
        let pos = |start: i128| ChannelPos {
            buffer: 0,
            offset: start as usize,
        };
        let [r, g, b, a] = starts;
        let layout = ImageLayout {
            bit_depth,
            width,
            height,
            x_stride_bytes: x_stride,
            y_stride_bytes: y_stride,
            r: pos(r.expect("a packed image has red")),
            g: pos(g.expect("a packed image has green")),
            b: pos(b.expect("a packed image has blue")),
            a: a.map(pos),
            is_rgba_packed,
            is_float,
        };
        Ok(PackedImageDesc {
            data: bytes,
            data_offset: origin,
            chan_order,
            num_channels,
            chan_stride_bytes: chan_stride,
            layout,
        })
    }

    /// Port of `getChannelOrder()`: the channel ordering of all the pixels.
    pub fn channel_order(&self) -> ChannelOrdering {
        self.chan_order
    }

    /// Port of `getNumChannels()`: 3 or 4.
    pub fn num_channels(&self) -> usize {
        self.num_channels as usize
    }

    /// Port of `getChanStrideBytes()`: the bytes from a channel to the next in a pixel.
    pub fn chan_stride_bytes(&self) -> isize {
        self.chan_stride_bytes
    }

    /// Port of `getData()`, with [`data_offset`](Self::data_offset): the memory, whose first
    /// pixel starts at that offset.
    pub fn data(&self) -> &[u8] {
        self.data.as_ref()
    }

    /// Where `getData()` points in [`data`](Self::data): the byte offset of the first channel of
    /// the first pixel.
    pub fn data_offset(&self) -> usize {
        self.data_offset
    }
}

impl<B: AsRef<[u8]>> sealed::Sealed for PackedImageDesc<B> {}

impl<B: AsRef<[u8]>> ImageDesc for PackedImageDesc<B> {
    fn layout(&self) -> &ImageLayout {
        &self.layout
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> ImageDescMut for PackedImageDesc<B> {}

/// Where R, G, B and A of the first pixel are, in bytes from the data pointer; A is `None` for
/// 3 channels. Upstream computes pointers (`(char*)m_rData + 2 * m_chanStrideBytes`); the
/// offsets wrap where that arithmetic overflows.
///
/// Port of `PackedImageDesc::Impl::initValues` (src/OpenColorIO/ImageDesc.cpp:143-186
/// @ v2.5.2). Its "Unknown channel ordering." can't be reached: the constructors refuse such
/// orderings first, and a Rust enum has no others.
fn packed_channel_offsets(
    chan_order: ChannelOrdering,
    num_channels: c_long,
    chan_stride: isize,
) -> [Option<isize>; 4] {
    let channel = |k: isize| k.wrapping_mul(chan_stride);
    let alpha = (num_channels == 4).then(|| channel(3));
    match chan_order {
        ChannelOrdering::Rgba | ChannelOrdering::Rgb => {
            [Some(0), Some(channel(1)), Some(channel(2)), alpha]
        }
        ChannelOrdering::Bgra | ChannelOrdering::Bgr => {
            [Some(channel(2)), Some(channel(1)), Some(0), alpha]
        }
        ChannelOrdering::Abgr => [
            Some(channel(3)),
            Some(channel(2)),
            Some(channel(1)),
            Some(0),
        ],
    }
}

/// Whether the pixels are RGBA with each channel right after the previous one, and each pixel
/// right after the previous one. Rows may be padded: the CPU engine processes one row at a
/// time.
///
/// Port of `PackedImageDesc::Impl::isRGBAPacked` (src/OpenColorIO/ImageDesc.cpp:188-275
/// @ v2.5.2), including its casts of the x stride and the channel stride to `int`: an x stride
/// that differs from 4 channels by a multiple of 2^32 bytes passes. Its "Unsupported bit-depth"
/// error can't be reached: the constructors refuse those bit depths first.
fn packed_is_rgba_packed(
    bit_depth: BitDepth,
    channels: &[Option<isize>; 4],
    chan_stride: isize,
    x_stride: isize,
) -> Result<bool> {
    let [Some(r), Some(g), Some(b), Some(a)] = *channels else {
        return Ok(false);
    };

    let size = match bit_depth {
        BitDepth::Uint8 => item_size::<Uint8>(),
        // A 10-bit or 12-bit integer is stored in a uint16_t, and a half in a half.
        BitDepth::Uint10 => item_size::<Uint10>(),
        BitDepth::Uint12 => item_size::<Uint12>(),
        BitDepth::Uint16 => item_size::<Uint16>(),
        BitDepth::F16 => item_size::<F16>(),
        BitDepth::F32 => item_size::<F32>(),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            return Err(Exception::new(format!(
                "PackedImageDesc Error: Unsupported bit-depth: {}.",
                bit_depth_to_string(bit_depth)
            )));
        }
    };
    if chan_stride != size {
        return Ok(false);
    }

    if g.wrapping_sub(r) != chan_stride {
        return Ok(false);
    }
    if b.wrapping_sub(g) != chan_stride {
        return Ok(false);
    }
    if a.wrapping_sub(b) != chan_stride {
        return Ok(false);
    }

    // Confirm xStrideBytes is a pure packing (i.e. it divides evenly).
    if chan_stride == 0 {
        return Ok(false);
    }
    // div((int)m_xStrideBytes, (int)m_chanStrideBytes): the channel stride is 1, 2 or 4 here.
    let x = x_stride as i32;
    let c = chan_stride as i32;
    if x % c != 0 {
        return Ok(false);
    }
    let implicit_channels = x / c;
    Ok(implicit_channels == 4)
}

/// Upstream's checks of a packed image, in its order and with its messages. The arithmetic
/// wraps as the wheels' machine code does where C++ overflows (strides of 2^61 bytes and
/// more): `std::abs` of the most negative value is that value.
///
/// Port of `PackedImageDesc::Impl::validate` (src/OpenColorIO/ImageDesc.cpp:282-329 @ v2.5.2).
/// "Invalid channel number." and "Unknown bit-depth of the image buffer." can't be reached:
/// the constructors raise "Invalid number of channels." and "Bit depth is not supported"
/// first.
fn packed_validate(
    null_data: bool,
    width: c_long,
    height: c_long,
    bit_depth: BitDepth,
    num_channels: c_long,
    chan_stride: isize,
    x_stride: isize,
    y_stride: isize,
) -> Result<()> {
    let fail = |message: &str| Err(Exception::new(format!("PackedImageDesc Error: {message}")));

    // An empty buffer stands for a null pointer.
    if null_data {
        return fail("Invalid image buffer.");
    }

    if width <= 0 || height <= 0 {
        return fail("Invalid image dimensions.");
    }

    if chan_stride.wrapping_abs() < get_channel_size_in_bytes(bit_depth)? as isize
        || chan_stride == AUTO_STRIDE
    {
        return fail("Invalid channel stride.");
    }

    if !(3..=4).contains(&num_channels) {
        return fail("Invalid channel number.");
    }

    if chan_stride
        .wrapping_mul(num_channels as isize)
        .wrapping_abs()
        > x_stride.wrapping_abs()
    {
        return fail("The channel and x strides are inconsistent.");
    }

    // The check above takes `std::abs` of the x stride, which is undefined for INT64_MIN, the
    // value of `AutoStride`. GCC concluded that the x stride isn't `AutoStride` and removed this
    // check from the Linux wheel, which builds such an image with an x stride of INT64_MIN; the
    // port then refuses it as reaching outside its buffer (D-2): a channel stride of 2^61 bytes
    // or more puts the channels outside any buffer. The Windows wheel (MSVC) makes the check.
    // (docs/improvements.md, I-2.)
    if cfg!(target_os = "windows") && x_stride == AUTO_STRIDE {
        return fail("Invalid x stride.");
    }

    if y_stride == AUTO_STRIDE {
        return fail("Invalid y stride.");
    }

    if x_stride.wrapping_abs().wrapping_mul(width as isize) > y_stride.wrapping_abs() {
        return fail("The x and y strides are inconsistent.");
    }

    if bit_depth == BitDepth::Unknown {
        return fail("Unknown bit-depth of the image buffer.");
    }

    Ok(())
}

/// The lowest and highest of `k * stride` for `k` in `0..count`, for a count of at least 1.
fn span(stride: isize, count: c_long) -> (i128, i128) {
    let last = (i128::from(count) - 1) * stride as i128;
    (last.min(0), last.max(0))
}

/// Deviation D-2 (`docs/deviations.md`) for one channel of an image the CPU engine reads and
/// writes channel by channel (src/OpenColorIO/ImagePacking.cpp:21-299 @ v2.5.2): whether the
/// `item` bytes at `start + x * x_stride + y * y_stride`, for every `x < width` and
/// `y < height`, leave a buffer of `len` bytes. `start` is the channel of the first pixel, in
/// bytes from the buffer's start. The arithmetic is in `i128`, where it can't overflow.
fn channel_reaches_outside(
    len: usize,
    start: i128,
    item: usize,
    width: c_long,
    height: c_long,
    x_stride: isize,
    y_stride: isize,
) -> bool {
    let (x_low, x_high) = span(x_stride, width);
    let (y_low, y_high) = span(y_stride, height);
    start + x_low + y_low < 0 || start + x_high + y_high + item as i128 > len as i128
}

/// Deviation D-2 for an RGBA-packed image, which the CPU engine reads and writes in whole rows
/// of `4 * width` channels from its red channel (src/OpenColorIO/ScanlineHelper.cpp:129-136,
/// 158-164 @ v2.5.2): whether those rows, `y_stride` bytes apart from `start`, leave a buffer of
/// `len` bytes.
fn rows_reach_outside(
    len: usize,
    start: i128,
    item: usize,
    width: c_long,
    height: c_long,
    y_stride: isize,
) -> bool {
    let (y_low, y_high) = span(y_stride, height);
    let row = 4 * item as i128 * i128::from(width);
    start + y_low < 0 || start + y_high + row > len as i128
}

/// Port of `PlanarImageDesc` (include/OpenColorIO/OpenColorIO.h:3243-3300,
/// src/OpenColorIO/ImageDesc.cpp:592-771 @ v2.5.2): an image whose red, green, blue and
/// (optional) alpha channels are in separate planes, with one bit depth and one pair of strides
/// for all of them.
///
/// The planes are separate buffers ([`new`](Self::new), [`with_strides`](Self::with_strides)) or
/// places in one buffer ([`in_one_buffer`](Self::in_one_buffer)), as C++ can pass four pointers
/// into one allocation. `B` is `&[u8]` or `&mut [u8]`, as for [`PackedImageDesc`].
pub struct PlanarImageDesc<B> {
    /// R, G, B and A (3 or 4 buffers), or the one buffer of `in_one_buffer`.
    buffers: Vec<B>,
    layout: ImageLayout,
}

impl<B: AsRef<[u8]>> fmt::Debug for PlanarImageDesc<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lengths: Vec<usize> = self.buffers.iter().map(|b| b.as_ref().len()).collect();
        f.debug_struct("PlanarImageDesc")
            .field("buffer_bytes", &lengths)
            .field("layout", &self.layout)
            .finish()
    }
}

/// Where a planar image's channels are before the checks: the channel's buffer and the byte
/// offset of its first pixel in it.
type PlanarStarts = [Option<(usize, i128)>; 4];

impl<B: AsRef<[u8]>> PlanarImageDesc<B> {
    /// `PlanarImageDesc(rData, gData, bData, aData, width, height)`: F32 planes with nothing
    /// between pixels and rows; `a` is `None` for an image without alpha.
    ///
    /// Port of `PlanarImageDesc::PlanarImageDesc(void *, void *, void *, void *, long, long)`
    /// (src/OpenColorIO/ImageDesc.cpp:638-669 @ v2.5.2).
    pub fn new<S: PixelData<Bytes = B>>(
        r: S,
        g: S,
        b: S,
        a: Option<S>,
        width: usize,
        height: usize,
    ) -> Result<Self> {
        Self::separate(r, g, b, a, long(width), long(height), None)
    }

    /// `PlanarImageDesc(rData, gData, bData, aData, width, height, bitDepth, xStrideBytes,
    /// yStrideBytes)`: planes of `bit_depth`, with the bytes between pixels and rows given, each
    /// [`AUTO_STRIDE`] or a byte count (negative ones included); `a` is `None` for an image
    /// without alpha.
    ///
    /// Port of `PlanarImageDesc::PlanarImageDesc(void *, void *, void *, void *, long, long,
    /// BitDepth, ptrdiff_t, ptrdiff_t)` (src/OpenColorIO/ImageDesc.cpp:671-710 @ v2.5.2).
    pub fn with_strides<S: PixelData<Bytes = B>>(
        r: S,
        g: S,
        b: S,
        a: Option<S>,
        width: usize,
        height: usize,
        bit_depth: BitDepth,
        x_stride_bytes: isize,
        y_stride_bytes: isize,
    ) -> Result<Self> {
        Self::separate(
            r,
            g,
            b,
            a,
            long(width),
            long(height),
            Some((bit_depth, [x_stride_bytes, y_stride_bytes])),
        )
    }

    /// Planes inside one buffer, as C++ passes four pointers into one allocation: the first
    /// pixel of the R, G, B and (optional) A planes is `r`, `g`, `b` and `a` bytes after the
    /// data's first pixel. Otherwise as [`with_strides`](Self::with_strides).
    pub fn in_one_buffer<S: PixelData<Bytes = B>>(
        data: S,
        r: usize,
        g: usize,
        b: usize,
        a: Option<usize>,
        width: usize,
        height: usize,
        bit_depth: BitDepth,
        x_stride_bytes: isize,
        y_stride_bytes: isize,
    ) -> Result<Self> {
        let PixelParts {
            bytes,
            origin,
            element,
        } = data.into_parts();
        let start = |offset: usize| Some((0, origin as i128 + offset as i128));
        let starts = [start(r), start(g), start(b), a.and_then(start)];
        let null = bytes.as_ref().is_empty();
        Self::construct(
            vec![bytes],
            element,
            starts,
            null,
            long(width),
            long(height),
            Some((bit_depth, [x_stride_bytes, y_stride_bytes])),
        )
    }

    /// The constructors with separate planes.
    fn separate<S: PixelData<Bytes = B>>(
        r: S,
        g: S,
        b: S,
        a: Option<S>,
        width: c_long,
        height: c_long,
        strides: Option<(BitDepth, [isize; 2])>,
    ) -> Result<Self> {
        let mut buffers = Vec::with_capacity(4);
        let mut starts: PlanarStarts = [None; 4];
        let mut element = None;
        let mut null = false;
        for (channel, plane) in [Some(r), Some(g), Some(b), a].into_iter().enumerate() {
            let Some(plane) = plane else { continue };
            let parts = plane.into_parts();
            // Null for R, G or B; a null A means no alpha, which `None` says.
            null |= channel < 3 && parts.bytes.as_ref().is_empty();
            element = parts.element;
            starts[channel] = Some((buffers.len(), parts.origin as i128));
            buffers.push(parts.bytes);
        }
        Self::construct(buffers, element, starts, null, width, height, strides)
    }

    /// The two constructors (src/OpenColorIO/ImageDesc.cpp:638-710 @ v2.5.2) and
    /// `PlanarImageDesc::Impl::isFloat` (609-612). `strides` is `None` for the one without a bit
    /// depth: F32, and the strides `AutoStride` gives.
    fn construct(
        buffers: Vec<B>,
        element: Option<Element>,
        starts: PlanarStarts,
        null: bool,
        width: c_long,
        height: c_long,
        strides: Option<(BitDepth, [isize; 2])>,
    ) -> Result<Self> {
        let bit_depth = strides.map_or(BitDepth::F32, |(bit_depth, _)| bit_depth);

        // The Python binding checks each plane's type before the library sees them; the planes
        // of one call all have the same type here.
        check_buffer_type(element, bit_depth)?;

        if null {
            return Err(Exception::new(
                "PlanarImageDesc Error: Invalid image buffer.",
            ));
        }

        if width <= 0 || height <= 0 {
            return Err(Exception::new(
                "PlanarImageDesc Error: Invalid image dimensions.",
            ));
        }

        // The constructor without a bit depth uses `sizeof(BitDepthInfo<BIT_DEPTH_F32>::Type)`
        // and derives both strides; the other asks `GetChannelSizeInBytes`, which refuses the bit
        // depths the CPU processor doesn't take, and derives the `AutoStride` ones.
        let (one_channel_in_bytes, [x, y]) = match strides {
            None => (item_size::<F32>(), [AUTO_STRIDE; 2]),
            Some((bit_depth, strides)) => (get_channel_size_in_bytes(bit_depth)? as isize, strides),
        };
        let x_stride = if x == AUTO_STRIDE {
            one_channel_in_bytes
        } else {
            x
        };
        let y_stride = if y == AUTO_STRIDE {
            x_stride.wrapping_mul(width as isize)
        } else {
            y
        };

        let is_float = x_stride == item_size::<F32>() && bit_depth == BitDepth::F32;

        planar_validate(bit_depth, width, x_stride, y_stride)?;

        // Deviation D-2: the bytes the CPU engine will touch must be in the buffers. A planar
        // image is never RGBA-packed, so it is read and written channel by channel.
        let item = get_channel_size_in_bytes(bit_depth)? as usize;
        let outside = starts.iter().flatten().any(|&(buffer, start)| {
            let len = buffers[buffer].as_ref().len();
            channel_reaches_outside(len, start, item, width, height, x_stride, y_stride)
        });
        if outside {
            return Err(Exception::new(
                "PlanarImageDesc Error: The strides and dimensions reach outside the image buffer.",
            ));
        }

        // Inside their buffers, so every start is an offset in its buffer.
        let pos = |(buffer, start): (usize, i128)| ChannelPos {
            buffer,
            offset: start as usize,
        };
        let [r, g, b, a] = starts;
        let layout = ImageLayout {
            bit_depth,
            width,
            height,
            x_stride_bytes: x_stride,
            y_stride_bytes: y_stride,
            r: pos(r.expect("a planar image has red")),
            g: pos(g.expect("a planar image has green")),
            b: pos(b.expect("a planar image has blue")),
            a: a.map(pos),
            // Port of `PlanarImageDesc::isRGBAPacked` (src/OpenColorIO/ImageDesc.cpp:763-766).
            is_rgba_packed: false,
            is_float,
        };
        Ok(PlanarImageDesc { buffers, layout })
    }

    /// The buffer `index` that [`ImageDesc::r_data`] and the other channel positions point into:
    /// R, G, B and A in that order for separate planes, or the one buffer of
    /// [`in_one_buffer`](Self::in_one_buffer).
    pub fn buffer(&self, index: usize) -> Option<&[u8]> {
        self.buffers.get(index).map(AsRef::as_ref)
    }
}

impl<B: AsRef<[u8]>> sealed::Sealed for PlanarImageDesc<B> {}

impl<B: AsRef<[u8]>> ImageDesc for PlanarImageDesc<B> {
    fn layout(&self) -> &ImageLayout {
        &self.layout
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> ImageDescMut for PlanarImageDesc<B> {}

/// Upstream's checks of a planar image, in its order and with its messages; the product wraps
/// as the wheels' machine code does where C++ overflows.
///
/// Port of `PlanarImageDesc::Impl::validate` (src/OpenColorIO/ImageDesc.cpp:614-635 @ v2.5.2).
/// "Invalid x stride." can't be reached (`AutoStride` becomes the channel size), nor can
/// "Unknown bit-depth of the image buffer." (the constructor raises "Bit depth is not
/// supported" first).
fn planar_validate(
    bit_depth: BitDepth,
    width: c_long,
    x_stride: isize,
    y_stride: isize,
) -> Result<()> {
    let fail = |message: &str| Err(Exception::new(format!("PlanarImageDesc Error: {message}")));

    if x_stride == AUTO_STRIDE {
        return fail("Invalid x stride.");
    }

    if y_stride == AUTO_STRIDE {
        return fail("Invalid y stride.");
    }

    if x_stride.wrapping_mul(width as isize).wrapping_abs() > y_stride.wrapping_abs() {
        return fail("The x and y strides are inconsistent.");
    }

    if bit_depth == BitDepth::Unknown {
        return fail("Unknown bit-depth of the image buffer.");
    }

    Ok(())
}

/// An image description as the CPU engine reads it: its size and strides, where its channels
/// are, and the conversion between its bit depth and the F32 the ops process.
///
/// Port of `GenericImageDesc` (declared in src/OpenColorIO/ImagePacking.h:16-47, defined in
/// src/OpenColorIO/ImageDesc.cpp:75-112 @ v2.5.2). Upstream keeps the channels' pointers; the
/// port keeps their positions, and the scanline code borrows the buffers from the description.
#[derive(Debug, Clone)]
pub struct GenericImageDesc {
    /// `m_width`.
    pub width: c_long,
    /// `m_height`.
    pub height: c_long,
    /// `m_xStrideBytes`.
    pub x_stride_bytes: isize,
    /// `m_yStrideBytes`.
    pub y_stride_bytes: isize,
    /// `m_rData`.
    pub r_data: ChannelPos,
    /// `m_gData`.
    pub g_data: ChannelPos,
    /// `m_bData`.
    pub b_data: ChannelPos,
    /// `m_aData`: `None` for null.
    pub a_data: Option<ChannelPos>,
    /// `m_bitDepthOp`: the conversion to or from 32-bit float, so that the ops process floats.
    pub bit_depth_op: Arc<dyn CpuOp>,
    /// `m_isRGBAPacked`: whether the image buffer is an RGBA packed buffer.
    pub is_rgba_packed: bool,
    /// `m_isFloat`: whether the image buffer is a 32-bit float image buffer.
    pub is_float: bool,
}

impl GenericImageDesc {
    /// The description of an image with `layout` ([`ImageDesc::layout`]), for a CPU processor
    /// whose bit depth on this side is `bit_depth`, with its conversion `bit_depth_op`.
    ///
    /// Port of `GenericImageDesc::init` (src/OpenColorIO/ImageDesc.cpp:75-97 @ v2.5.2), as a
    /// constructor: upstream fills a default-constructed struct from the description's getters.
    pub fn init(
        layout: &ImageLayout,
        bit_depth: BitDepth,
        bit_depth_op: Arc<dyn CpuOp>,
    ) -> Result<Self> {
        let desc = GenericImageDesc {
            width: layout.width_long(),
            height: layout.height_long(),
            x_stride_bytes: layout.x_stride_bytes(),
            y_stride_bytes: layout.y_stride_bytes(),
            r_data: layout.r_data(),
            g_data: layout.g_data(),
            b_data: layout.b_data(),
            a_data: layout.a_data(),
            bit_depth_op,
            is_rgba_packed: layout.is_rgba_packed(),
            is_float: layout.is_float(),
        };

        if layout.bit_depth() != bit_depth {
            return Err(Exception::new(
                "Bit-depth mismatch between the image buffer and the finalization setting.",
            ));
        }
        Ok(desc)
    }

    /// Whether the image buffer is a packed RGBA 32-bit float buffer.
    ///
    /// Port of `GenericImageDesc::isPackedFloatRGBA` (src/OpenColorIO/ImageDesc.cpp:99-102
    /// @ v2.5.2).
    pub fn is_packed_float_rgba(&self) -> bool {
        self.is_float && self.is_rgba_packed
    }

    /// Port of `GenericImageDesc::isRGBAPacked` (src/OpenColorIO/ImageDesc.cpp:104-107
    /// @ v2.5.2).
    pub fn is_rgba_packed(&self) -> bool {
        self.is_rgba_packed
    }

    /// Port of `GenericImageDesc::isFloat` (src/OpenColorIO/ImageDesc.cpp:109-112 @ v2.5.2).
    pub fn is_float(&self) -> bool {
        self.is_float
    }
}

#[cfg(test)]
#[path = "image_desc_tests.rs"]
mod tests;
