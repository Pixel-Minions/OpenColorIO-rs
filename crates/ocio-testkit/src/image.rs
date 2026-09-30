// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Requests for the oracle's image commands (`oracle/ocio_oracle/image.py`, chunk O1.2): images
//! described every way PyOpenColorIO allows, a CPU processor applied to them, and every buffer
//! returned byte for byte, padding included. They check the port's image descriptions, packing
//! and CPU engine (WP 1.1, 1.2d).
//!
//! - [`Request`] is an `image_apply` call: its [`Buffer`]s, the images over them ([`Packed`],
//!   [`Planar`]), and the images `CPUProcessor.apply` takes. Its [`Reply`] holds the wheel's
//!   getters, what it raised and where ([`Raised`]), and every buffer after the call.
//! - [`RgbRequest`] is an `image_apply_rgb` call: Python's `applyRGB` and `applyRGBA`, on an
//!   array or a list ([`RgbInput`]).
//! - [`Footprint`] says where the channels of an image's pixels are, to write input pixels and
//!   read output pixels, as upstream's tests fill their `std::vector`s.
//!
//! A port test builds the same buffers ([`Buffer::bytes`]), describes them with the port's image
//! descriptions, applies the port's CPU processor and compares every buffer with the wheel's
//! ([`crate::assert_bytes_eq`]). Getters compare as JSON and exceptions as text. The Python
//! binding checks buffers itself before the library sees them; `tests/image_oracle.rs` says
//! which checks are the binding's and which the library's, and which library paths Python
//! can't reach. The port's core must give the library's messages, `ocio-py` the binding's.
//!
//! ```no_run
//! use ocio_testkit::battery::BitDepth;
//! use ocio_testkit::image::{Buffer, ChannelOrder, Channels, Data, Packed, Request, Stride};
//! use serde_json::json;
//!
//! // Three BGRA F32 pixels into a BGRA image with 4 bytes after each pixel, over a 0xa5 prefill.
//! let mut request = Request::new(json!({"transform": {"class": "LogTransform"}}));
//! let input = request.buffer(Buffer::Bytes(vec![0; 3 * 16]));
//! let output = request.buffer(Buffer::fill(3 * 20, &[0xa5]));
//! let bgra = Channels::Order(ChannelOrder::Bgra);
//! let src = request.image(Packed::new(Data::at(input, 0), 3, 1, bgra));
//! let dst = request.image(Packed::new(Data::at(output, 0), 3, 1, bgra).layout(
//!     BitDepth::F32,
//!     [Stride::Auto, Stride::Bytes(20), Stride::Auto],
//! ));
//! request.apply = vec![src, dst];
//! let reply = request.run();
//! assert!(reply.raised().is_none(), "{}", reply.result);
//! let written: &[u8] = &reply.buffers[output];
//! ```

use serde_json::{Value, json};

use crate::battery::BitDepth;
use crate::oracle::{BatchCall, Oracle, Response};

/// A stride argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stride {
    /// `OCIO.AutoStride`: the library derives the stride.
    Auto,
    /// A stride in bytes, negative ones included.
    Bytes(i64),
}

impl Stride {
    fn json(self) -> Value {
        match self {
            Stride::Auto => json!("AutoStride"),
            Stride::Bytes(bytes) => json!(bytes),
        }
    }
}

/// A `bitDepth` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Depth {
    /// A bit depth the CPU processor takes.
    Supported(BitDepth),
    /// Another member of OCIO's `BitDepth`, by name: `BIT_DEPTH_UINT14`, `BIT_DEPTH_UINT32` or
    /// `BIT_DEPTH_UNKNOWN`.
    Named(&'static str),
    /// `OCIO.BitDepth(value)`, for values outside the enum.
    Value(i64),
}

impl From<BitDepth> for Depth {
    fn from(depth: BitDepth) -> Depth {
        Depth::Supported(depth)
    }
}

impl Depth {
    fn json(self) -> Value {
        match self {
            Depth::Supported(depth) => json!(depth.oracle_name()),
            Depth::Named(name) => json!(name),
            Depth::Value(value) => json!(value),
        }
    }
}

/// OCIO's `ChannelOrdering`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelOrder {
    /// `CHANNEL_ORDERING_RGBA`.
    Rgba,
    /// `CHANNEL_ORDERING_BGRA`.
    Bgra,
    /// `CHANNEL_ORDERING_ABGR`.
    Abgr,
    /// `CHANNEL_ORDERING_RGB`.
    Rgb,
    /// `CHANNEL_ORDERING_BGR`.
    Bgr,
}

impl ChannelOrder {
    /// Every ordering, in the enum's order.
    pub const ALL: [ChannelOrder; 5] = [
        ChannelOrder::Rgba,
        ChannelOrder::Bgra,
        ChannelOrder::Abgr,
        ChannelOrder::Rgb,
        ChannelOrder::Bgr,
    ];

    /// The name the oracle takes.
    pub fn oracle_name(self) -> &'static str {
        match self {
            ChannelOrder::Rgba => "CHANNEL_ORDERING_RGBA",
            ChannelOrder::Bgra => "CHANNEL_ORDERING_BGRA",
            ChannelOrder::Abgr => "CHANNEL_ORDERING_ABGR",
            ChannelOrder::Rgb => "CHANNEL_ORDERING_RGB",
            ChannelOrder::Bgr => "CHANNEL_ORDERING_BGR",
        }
    }

    /// Where R, G, B and A are in a pixel, in channels from its first; `None` for the alpha of
    /// RGB and BGR.
    pub fn positions(self) -> [Option<usize>; 4] {
        match self {
            ChannelOrder::Rgba => [Some(0), Some(1), Some(2), Some(3)],
            ChannelOrder::Bgra => [Some(2), Some(1), Some(0), Some(3)],
            ChannelOrder::Abgr => [Some(3), Some(2), Some(1), Some(0)],
            ChannelOrder::Rgb => [Some(0), Some(1), Some(2), None],
            ChannelOrder::Bgr => [Some(2), Some(1), Some(0), None],
        }
    }

    /// The channels in a pixel: 4 or 3.
    pub fn channels(self) -> usize {
        self.positions().iter().flatten().count()
    }
}

/// How a packed image gives its channels: the constructors with `numChannels` or with
/// `chanOrder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channels {
    /// `numChannels`: 4 is RGBA and 3 is RGB; the library refuses other counts.
    Count(i64),
    /// `chanOrder`.
    Order(ChannelOrder),
    /// `chanOrder` as `OCIO.ChannelOrdering(value)`, for values outside the enum.
    OrderValue(i64),
}

/// The bytes of one channel of `depth` in its storage type: `uint8_t`, `uint16_t` for 10, 12 and
/// 16 bits, `half`, `float` (`BitDepthInfo`, src/OpenColorIO/BitDepthUtils.h:31-94 @ v2.5.2).
pub fn channel_bytes(depth: BitDepth) -> usize {
    match depth {
        BitDepth::Uint8 => 1,
        BitDepth::Uint10 | BitDepth::Uint12 | BitDepth::Uint16 | BitDepth::F16 => 2,
        BitDepth::F32 => 4,
    }
}

/// The numpy dtype of `depth`'s storage type, as the binding maps them (`bitDepthToDtype`,
/// src/bindings/python/PyUtils.cpp:56-86 @ v2.5.2).
pub fn dtype(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Uint8 => "uint8",
        BitDepth::Uint10 | BitDepth::Uint12 | BitDepth::Uint16 => "uint16",
        BitDepth::F16 => "float16",
        BitDepth::F32 => "float32",
    }
}

/// The Python buffer object the binding receives for an image or a plane. Its first entry is
/// `offset` bytes into request buffer `buffer`: that is the image's (or plane's) data pointer.
/// It has the entry count and type the binding requires, unless `entries` or `dtype` (a numpy
/// dtype name) say otherwise, to probe the binding's checks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Data {
    /// The request buffer.
    pub buffer: usize,
    /// The byte offset of the first entry in the buffer.
    pub offset: usize,
    /// The number of entries, if not the one the binding requires.
    pub entries: Option<i64>,
    /// The numpy dtype, if not the bit depth's storage type.
    pub dtype: Option<String>,
}

impl Data {
    /// The data at `offset` bytes into request buffer `buffer`.
    pub fn at(buffer: usize, offset: usize) -> Data {
        Data {
            buffer,
            offset,
            ..Data::default()
        }
    }

    /// The same data with `entries` entries.
    pub fn entries(mut self, entries: i64) -> Data {
        self.entries = Some(entries);
        self
    }

    /// The same data as numpy dtype `dtype`.
    pub fn dtype(mut self, dtype: &str) -> Data {
        self.dtype = Some(dtype.to_string());
        self
    }

    fn json(&self) -> Value {
        let mut data = json!({"buffer": self.buffer, "offset": self.offset});
        if let Some(entries) = self.entries {
            data["entries"] = json!(entries);
        }
        if let Some(dtype) = &self.dtype {
            data["dtype"] = json!(dtype);
        }
        data
    }
}

/// A `PackedImageDesc`.
#[derive(Debug, Clone, PartialEq)]
pub struct Packed {
    /// The pixels.
    pub data: Data,
    /// The width in pixels.
    pub width: i64,
    /// The height in pixels.
    pub height: i64,
    /// `numChannels` or `chanOrder`.
    pub channels: Channels,
    /// `bitDepth` and `chanStrideBytes`, `xStrideBytes`, `yStrideBytes`: the constructors with
    /// strides. `None`: the constructors without (F32, default strides).
    pub layout: Option<(Depth, [Stride; 3])>,
    /// Positional arguments, as a script passes them, instead of keywords. The binding then
    /// takes a `chanOrder` for `numChannels`: it tries that overload first.
    pub positional: bool,
}

impl Packed {
    /// `PackedImageDesc(data, width, height, numChannels or chanOrder)`.
    pub fn new(data: Data, width: i64, height: i64, channels: Channels) -> Packed {
        Packed {
            data,
            width,
            height,
            channels,
            layout: None,
            positional: false,
        }
    }

    /// The constructor with a bit depth and the chan, x and y strides.
    pub fn layout(mut self, depth: impl Into<Depth>, strides: [Stride; 3]) -> Packed {
        self.layout = Some((depth.into(), strides));
        self
    }

    /// Positional arguments.
    pub fn positional(mut self) -> Packed {
        self.positional = true;
        self
    }

    fn json(&self) -> Value {
        let mut image = json!({
            "kind": "packed",
            "data": self.data.json(),
            "width": self.width,
            "height": self.height,
            "positional": self.positional,
        });
        match self.channels {
            Channels::Count(count) => image["num_channels"] = json!(count),
            Channels::Order(order) => image["chan_order"] = json!(order.oracle_name()),
            Channels::OrderValue(value) => image["chan_order"] = json!(value),
        }
        if let Some((depth, [chan, x, y])) = self.layout {
            image["bitdepth"] = depth.json();
            image["chan_stride"] = chan.json();
            image["x_stride"] = x.json();
            image["y_stride"] = y.json();
        }
        image
    }
}

/// A `PlanarImageDesc`.
#[derive(Debug, Clone, PartialEq)]
pub struct Planar {
    /// The R, G and B planes, and the A plane if the image has one.
    pub planes: Vec<Data>,
    /// The width in pixels.
    pub width: i64,
    /// The height in pixels.
    pub height: i64,
    /// `bitDepth` and `xStrideBytes`, `yStrideBytes`: the constructors with strides. `None`: the
    /// constructors without (F32, default strides).
    pub layout: Option<(Depth, [Stride; 2])>,
    /// Positional arguments instead of keywords.
    pub positional: bool,
}

impl Planar {
    /// `PlanarImageDesc(rData, gData, bData[, aData], width, height)`.
    pub fn new(planes: Vec<Data>, width: i64, height: i64) -> Planar {
        Planar {
            planes,
            width,
            height,
            layout: None,
            positional: false,
        }
    }

    /// The constructor with a bit depth and the x and y strides.
    pub fn layout(mut self, depth: impl Into<Depth>, strides: [Stride; 2]) -> Planar {
        self.layout = Some((depth.into(), strides));
        self
    }

    fn json(&self) -> Value {
        let mut image = json!({
            "kind": "planar",
            "planes": self.planes.iter().map(Data::json).collect::<Vec<_>>(),
            "width": self.width,
            "height": self.height,
            "positional": self.positional,
        });
        if let Some((depth, [x, y])) = self.layout {
            image["bitdepth"] = depth.json();
            image["x_stride"] = x.json();
            image["y_stride"] = y.json();
        }
        image
    }
}

/// An image description.
#[derive(Debug, Clone, PartialEq)]
pub enum Image {
    /// A `PackedImageDesc`.
    Packed(Packed),
    /// A `PlanarImageDesc`.
    Planar(Planar),
}

impl From<Packed> for Image {
    fn from(image: Packed) -> Image {
        Image::Packed(image)
    }
}

impl From<Planar> for Image {
    fn from(image: Planar) -> Image {
        Image::Planar(image)
    }
}

/// One of a request's buffers: in the oracle, a fresh writable allocation that starts on a
/// 64-byte boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Buffer {
    /// These bytes.
    Bytes(Vec<u8>),
    /// `size` bytes of `pattern`, repeated.
    Fill {
        /// The size in bytes.
        size: usize,
        /// The repeated bytes.
        pattern: Vec<u8>,
    },
}

impl Buffer {
    /// `size` bytes of `pattern`, repeated.
    pub fn fill(size: usize, pattern: &[u8]) -> Buffer {
        Buffer::Fill {
            size,
            pattern: pattern.to_vec(),
        }
    }

    /// The buffer's bytes before the call.
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            Buffer::Bytes(bytes) => bytes.clone(),
            Buffer::Fill { size, pattern } => pattern.iter().copied().cycle().take(*size).collect(),
        }
    }
}

/// An `image_apply` call.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The processor, as the oracle's `cpu_apply` takes it: `transform` (or `config`, `src` and
    /// `dst`), `direction`, `optimization`, `in_bitdepth`, `out_bitdepth`.
    pub processor: Value,
    /// The memory the images describe.
    pub buffers: Vec<Buffer>,
    /// The image descriptions, constructed in order.
    pub images: Vec<Image>,
    /// The images `CPUProcessor.apply` takes: `[]` constructs the images only (and builds no
    /// processor), `[i]` applies in place, `[i, j]` from image `i` to image `j`.
    pub apply: Vec<usize>,
    /// Whether to call the binding's data getters after the call (`getData`; `getRData`,
    /// `getGData`, `getBData`, `getAData`), where they copy only bytes of their buffer.
    pub data_getters: bool,
}

impl Request {
    /// A request with this processor and nothing else yet.
    pub fn new(processor: Value) -> Request {
        Request {
            processor,
            buffers: Vec::new(),
            images: Vec::new(),
            apply: Vec::new(),
            data_getters: false,
        }
    }

    /// Adds a buffer and returns its index.
    pub fn buffer(&mut self, buffer: Buffer) -> usize {
        self.buffers.push(buffer);
        self.buffers.len() - 1
    }

    /// Adds an image and returns its index.
    pub fn image(&mut self, image: impl Into<Image>) -> usize {
        self.images.push(image.into());
        self.images.len() - 1
    }

    /// The command's arguments and blobs.
    pub fn args(&self) -> (Value, Vec<&[u8]>) {
        let mut args = self.processor.clone();
        assert!(
            args.is_object(),
            "the processor must be a JSON object: {args}"
        );
        let mut blobs = Vec::new();
        let buffers: Vec<Value> = self
            .buffers
            .iter()
            .map(|buffer| match buffer {
                Buffer::Bytes(bytes) => {
                    blobs.push(bytes.as_slice());
                    json!({"blob": blobs.len() - 1})
                }
                Buffer::Fill { size, pattern } => json!({"size": size, "fill": pattern}),
            })
            .collect();
        args["buffers"] = json!(buffers);
        args["images"] = self
            .images
            .iter()
            .map(|image| match image {
                Image::Packed(image) => image.json(),
                Image::Planar(image) => image.json(),
            })
            .collect();
        args["apply"] = json!(self.apply);
        args["data_getters"] = json!(self.data_getters);
        (args, blobs)
    }

    /// The call, for [`Oracle::batch`].
    pub fn call(&self) -> BatchCall<'_> {
        let (args, blobs) = self.args();
        BatchCall {
            cmd: "image_apply",
            args,
            blobs,
        }
    }

    /// Runs the request on its own.
    #[track_caller]
    pub fn run(&self) -> Reply {
        let (args, blobs) = self.args();
        self.reply(Oracle::get().call("image_apply", args, &blobs))
    }

    /// The oracle's response to this request, as a [`Reply`].
    pub fn reply(&self, mut response: Response) -> Reply {
        assert!(
            response.blobs.len() >= self.buffers.len(),
            "image_apply returned {} blobs for {} buffers",
            response.blobs.len(),
            self.buffers.len()
        );
        let copies = response.blobs.split_off(self.buffers.len());
        Reply {
            result: response.result,
            buffers: response.blobs,
            copies,
        }
    }
}

/// What OCIO or the Python binding raised, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raised {
    /// The Python type: `Exception` (OCIO's), `RuntimeError` (the binding's buffer checks),
    /// `TypeError` (no constructor of the binding takes the arguments).
    pub kind: String,
    /// The message.
    pub message: String,
    /// `config`, `transform`, `processor`, `cpu_processor`, `image` or `apply`.
    pub stage: String,
    /// With stage `image`: the image being constructed.
    pub image: Option<usize>,
}

impl Raised {
    fn from_result(result: &Value) -> Option<Raised> {
        let exception = result.get("exception")?;
        Some(Raised {
            kind: exception["type"].as_str().unwrap_or_default().to_string(),
            message: exception["message"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            stage: result["stage"].as_str().unwrap_or_default().to_string(),
            image: result
                .get("image")
                .and_then(Value::as_u64)
                .map(|i| i as usize),
        })
    }
}

/// The wheel's answer to a [`Request`].
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    /// The command's result: `images` (each constructed image's getters), the cache IDs and
    /// `cpu_processor` getters, `exception`, `stage` and `image` when something raised, `log`.
    pub result: Value,
    /// Every buffer after the call, in the request's order.
    pub buffers: Vec<Vec<u8>>,
    /// The data getters' copies (see [`Reply::data`]).
    pub copies: Vec<Vec<u8>>,
}

impl Reply {
    /// What OCIO or the binding raised, if anything.
    pub fn raised(&self) -> Option<Raised> {
        Raised::from_result(&self.result)
    }

    /// The getters of constructed image `image`: `getBitDepth`, `getWidth`, `getHeight`,
    /// `getXStrideBytes`, `getYStrideBytes`, `isRGBAPacked`, `isFloat`, and for a packed image
    /// `getChannelOrder`, `getNumChannels`, `getChanStrideBytes`.
    pub fn getters(&self, image: usize) -> &Value {
        &self.result["images"][image]
    }

    /// What data getter `getter` (`getData`, `getRData`, ...) of image `image` returned, or
    /// `None` where the oracle didn't call it.
    pub fn data(&self, image: usize, getter: &str) -> Option<&[u8]> {
        let blob = self.getters(image).get(getter)?.as_u64()? as usize;
        Some(&self.copies[blob - self.buffers.len()])
    }

    /// OCIO's log messages.
    pub fn log(&self) -> Vec<String> {
        self.result["log"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| m.as_str().unwrap_or_default().to_string())
            .collect()
    }
}

/// What Python's `applyRGB` and `applyRGBA` take.
#[derive(Debug, Clone, PartialEq)]
pub enum RgbInput {
    /// A numpy array over a copy of `bytes`, which they process in place.
    Array {
        /// The memory.
        bytes: Vec<u8>,
        /// The numpy dtype.
        dtype: String,
        /// The shape; `None` for one dimension over the rest of the memory.
        shape: Option<Vec<i64>>,
        /// The strides in bytes; `None` for C-contiguous.
        strides: Option<Vec<i64>>,
        /// The byte offset of the first entry.
        offset: usize,
    },
    /// A list of floats, for which they return a new list: its values as little-endian `f64`
    /// ([`RgbInput::list`]).
    List(Vec<u8>),
}

impl RgbInput {
    /// A one-dimensional array of numpy dtype `dtype` over `bytes`.
    pub fn array(bytes: Vec<u8>, dtype: &str) -> RgbInput {
        RgbInput::Array {
            bytes,
            dtype: dtype.to_string(),
            shape: None,
            strides: None,
            offset: 0,
        }
    }

    /// A list of these floats.
    pub fn list(values: &[f64]) -> RgbInput {
        RgbInput::List(values.iter().flat_map(|v| v.to_le_bytes()).collect())
    }
}

/// An `image_apply_rgb` call: Python's `CPUProcessor.applyRGB` or `applyRGBA`.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbRequest {
    /// The processor, as for a [`Request`].
    pub processor: Value,
    /// `applyRGBA` if true, `applyRGB` if false.
    pub rgba: bool,
    /// The array or list.
    pub input: RgbInput,
}

impl RgbRequest {
    /// The command's arguments and blob.
    pub fn args(&self) -> (Value, &[u8]) {
        let mut args = self.processor.clone();
        assert!(
            args.is_object(),
            "the processor must be a JSON object: {args}"
        );
        args["call"] = json!(if self.rgba { "applyRGBA" } else { "applyRGB" });
        let blob = match &self.input {
            RgbInput::Array {
                bytes,
                dtype,
                shape,
                strides,
                offset,
            } => {
                args["dtype"] = json!(dtype);
                args["offset"] = json!(offset);
                if let Some(shape) = shape {
                    args["shape"] = json!(shape);
                }
                if let Some(strides) = strides {
                    args["strides"] = json!(strides);
                }
                bytes
            }
            RgbInput::List(values) => {
                args["list"] = json!(true);
                values
            }
        };
        (args, blob)
    }

    /// The call, for [`Oracle::batch`].
    pub fn call(&self) -> BatchCall<'_> {
        let (args, blob) = self.args();
        BatchCall {
            cmd: "image_apply_rgb",
            args,
            blobs: vec![blob],
        }
    }

    /// Runs the request on its own.
    #[track_caller]
    pub fn run(&self) -> RgbReply {
        let (args, blob) = self.args();
        RgbReply::from_response(Oracle::get().call("image_apply_rgb", args, &[blob]))
    }
}

/// The wheel's answer to an [`RgbRequest`].
#[derive(Debug, Clone, PartialEq)]
pub struct RgbReply {
    /// The command's result: the cache IDs and `cpu_processor` getters, or `exception` and
    /// `stage`; `log`.
    pub result: Value,
    /// The array's memory after the call, or the returned list as little-endian `f64`; `None`
    /// for a list when something raised.
    pub output: Option<Vec<u8>>,
}

impl RgbReply {
    /// The reply in `response`.
    pub fn from_response(response: Response) -> RgbReply {
        RgbReply {
            result: response.result,
            output: response.blobs.into_iter().next(),
        }
    }

    /// What OCIO or the binding raised, if anything.
    pub fn raised(&self) -> Option<Raised> {
        Raised::from_result(&self.result)
    }
}

/// Where the channels of an image's pixels are: channel `c` of pixel `(x, y)` is `item` bytes at
/// `channels[c]`'s offset `+ x * x_stride + y * y_stride` in `channels[c]`'s buffer. A test lays
/// out its input pixels and reads the output pixels with it, as upstream's tests fill their
/// `std::vector`s; the images describe the same layout to the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Footprint {
    /// The width in pixels.
    pub width: usize,
    /// The height in pixels.
    pub height: usize,
    /// The bytes of a channel.
    pub item: usize,
    /// The bytes from a pixel to the next.
    pub x_stride: i64,
    /// The bytes from a row to the next.
    pub y_stride: i64,
    /// R, G, B and A: the buffer and the byte offset of the channel of pixel (0, 0), or `None`
    /// for a channel the image doesn't have.
    pub channels: [Option<(usize, i64)>; 4],
}

impl Footprint {
    /// A packed image: its first channel at `offset` in `buffer`, the channels in `order`,
    /// `chan_stride` bytes apart.
    pub fn packed(
        buffer: usize,
        offset: i64,
        width: usize,
        height: usize,
        order: ChannelOrder,
        item: usize,
        chan_stride: i64,
        x_stride: i64,
        y_stride: i64,
    ) -> Footprint {
        Footprint {
            width,
            height,
            item,
            x_stride,
            y_stride,
            channels: order
                .positions()
                .map(|p| p.map(|p| (buffer, offset + p as i64 * chan_stride))),
        }
    }

    /// A planar image: its R, G, B and optional A planes, each a buffer and the byte offset of
    /// the plane's pixel (0, 0).
    pub fn planar(
        planes: &[(usize, i64)],
        width: usize,
        height: usize,
        item: usize,
        x_stride: i64,
        y_stride: i64,
    ) -> Footprint {
        assert!(matches!(planes.len(), 3 | 4), "{} planes", planes.len());
        Footprint {
            width,
            height,
            item,
            x_stride,
            y_stride,
            channels: std::array::from_fn(|c| planes.get(c).copied()),
        }
    }

    /// The buffer and byte offset of channel `c` (R, G, B, A) of pixel `(x, y)`, if the image
    /// has the channel.
    pub fn at(&self, c: usize, x: usize, y: usize) -> Option<(usize, usize)> {
        let (buffer, offset) = self.channels[c]?;
        let at = offset + x as i64 * self.x_stride + y as i64 * self.y_stride;
        Some((
            buffer,
            usize::try_from(at).expect("the footprint starts before its buffer"),
        ))
    }

    /// Writes `pixels` into `buffers`: RGBA pixels, row by row, `item` bytes per channel. The
    /// channels the image lacks are skipped.
    pub fn write(&self, buffers: &mut [Vec<u8>], pixels: &[u8]) {
        assert_eq!(pixels.len(), self.width * self.height * 4 * self.item);
        for (i, pixel) in pixels.chunks_exact(4 * self.item).enumerate() {
            for (c, value) in pixel.chunks_exact(self.item).enumerate() {
                if let Some((buffer, at)) = self.at(c, i % self.width, i / self.width) {
                    buffers[buffer][at..at + self.item].copy_from_slice(value);
                }
            }
        }
    }

    /// Channel `c` of every pixel, row by row, `item` bytes each; `None` if the image lacks
    /// the channel.
    pub fn read(&self, buffers: &[Vec<u8>], c: usize) -> Option<Vec<u8>> {
        self.channels[c]?;
        let mut out = Vec::with_capacity(self.width * self.height * self.item);
        for y in 0..self.height {
            for x in 0..self.width {
                let (buffer, at) = self.at(c, x, y)?;
                out.extend_from_slice(&buffers[buffer][at..at + self.item]);
            }
        }
        Some(out)
    }

    /// Which bytes of each buffer hold a channel of a pixel, for buffers of these sizes.
    pub fn covered(&self, sizes: &[usize]) -> Vec<Vec<bool>> {
        let mut covered: Vec<Vec<bool>> = sizes.iter().map(|&n| vec![false; n]).collect();
        for c in 0..4 {
            for y in 0..self.height {
                for x in 0..self.width {
                    if let Some((buffer, at)) = self.at(c, x, y) {
                        covered[buffer][at..at + self.item].fill(true);
                    }
                }
            }
        }
        covered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request's arguments: processor keys at the top level, `Bytes` buffers as blobs in
    /// order, `Fill` buffers as patterns, images with their constructor's arguments.
    #[test]
    fn requests_become_the_commands_arguments() {
        let mut request = Request::new(json!({"transform": {"class": "LogTransform"}}));
        let a = request.buffer(Buffer::fill(6, &[1, 2, 3, 4]));
        let b = request.buffer(Buffer::Bytes(vec![9; 3]));
        request.image(Packed::new(
            Data::at(a, 2),
            1,
            1,
            Channels::Order(ChannelOrder::Bgr),
        ));
        request.image(
            Planar::new(vec![Data::at(b, 0).entries(5).dtype("float64"); 3], 1, 1)
                .layout(BitDepth::Uint8, [Stride::Bytes(-1), Stride::Auto]),
        );
        request.apply = vec![0, 1];
        let (args, blobs) = request.args();
        assert_eq!(blobs, vec![&[9u8, 9, 9][..]]);
        assert_eq!(args["transform"]["class"], "LogTransform");
        assert_eq!(
            args["buffers"],
            json!([{"size": 6, "fill": [1, 2, 3, 4]}, {"blob": 0}])
        );
        assert_eq!(args["images"][0]["chan_order"], "CHANNEL_ORDERING_BGR");
        assert_eq!(args["images"][0]["data"], json!({"buffer": 0, "offset": 2}));
        assert_eq!(args["images"][1]["bitdepth"], "BIT_DEPTH_UINT8");
        assert_eq!(args["images"][1]["x_stride"], -1);
        assert_eq!(args["images"][1]["y_stride"], "AutoStride");
        assert_eq!(
            args["images"][1]["planes"][2],
            json!({"buffer": 1, "offset": 0, "entries": 5, "dtype": "float64"})
        );
        assert_eq!(args["apply"], json!([0, 1]));
        assert_eq!(
            Buffer::fill(6, &[1, 2, 3, 4]).bytes(),
            vec![1, 2, 3, 4, 1, 2]
        );
    }

    /// A footprint addresses each channel from its plane or its position in the pixel, and
    /// reads back what it wrote.
    #[test]
    fn footprints_address_every_channel() {
        let bgr = Footprint::packed(0, 40, 2, 2, ChannelOrder::Bgr, 2, 4, 12, -30);
        assert_eq!(bgr.at(0, 0, 0), Some((0, 48)));
        assert_eq!(bgr.at(2, 1, 0), Some((0, 52)));
        assert_eq!(bgr.at(1, 1, 1), Some((0, 26)));
        assert_eq!(bgr.at(3, 0, 0), None);
        let planar = Footprint::planar(&[(0, 0), (1, 4), (1, 0)], 2, 1, 1, 2, 4);
        assert_eq!(planar.at(1, 1, 0), Some((1, 6)));
        let mut buffers = vec![vec![0u8; 8], vec![0u8; 8]];
        planar.write(&mut buffers, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(
            buffers,
            vec![vec![1, 0, 5, 0, 0, 0, 0, 0], vec![3, 0, 7, 0, 2, 0, 6, 0]]
        );
        assert_eq!(planar.read(&buffers, 1), Some(vec![2, 6]));
        assert_eq!(planar.read(&buffers, 3), None);
        let covered = planar.covered(&[8, 8]);
        assert_eq!(
            covered[1],
            vec![true, false, true, false, true, false, true, false]
        );
    }
}
