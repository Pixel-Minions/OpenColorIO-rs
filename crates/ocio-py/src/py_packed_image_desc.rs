// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyPackedImageDesc` (src/bindings/python/PyPackedImageDesc.cpp @ v2.5.2): the class
//! `PackedImageDesc`, an image over one Python buffer, which it keeps alive (chunk 6.5a).

use ocio_ops::open_color_types::ChannelOrdering;
use ocio_py_ffi::Buffer;
use pyo3::prelude::*;

use crate::dispatch::{CLong, Function, Overload, Param};
use crate::py_image_desc::{Channels, Image, ImageArgs, construct, read_items};
use crate::py_transform::{BIT_DEPTH, INT, class, def, enum_out, none, ok};
use crate::py_utils::{
    BUFFER, BufferArg, array_of, bit_depth_of, bit_depth_to_dtype, chan_order_to_num_channels,
    check_buffer_size, check_buffer_type, check_buffer_type_of, dtype,
};
use crate::{arg, enum_param, method_def, q};

const PACKED_IMAGE_DESC: &str = q!("PackedImageDesc");
const CHANNEL_ORDERING: &str = q!("ChannelOrdering");
const SELF: Param = Param::new("self", PACKED_IMAGE_DESC);
const DATA: Param = Param::new("data", BUFFER);
const WIDTH: Param = Param::new("width", INT);
const HEIGHT: Param = Param::new("height", INT);
const NUM_CHANNELS: Param = Param::new("numChannels", INT);
const CHAN_ORDER: Param = Param::new("chanOrder", CHANNEL_ORDERING);
const BIT_DEPTH_PARAM: Param = Param::new("bitDepth", BIT_DEPTH);
const CHAN_STRIDE: Param = Param::new("chanStrideBytes", INT);
const X_STRIDE: Param = Param::new("xStrideBytes", INT);
const Y_STRIDE: Param = Param::new("yStrideBytes", INT);

/// The library's channel ordering of the C++ enum value `v`, which `chanOrderToNumChannels`
/// accepted (0 to 4).
fn channel_ordering(v: i64) -> ChannelOrdering {
    use ChannelOrdering::*;
    [Rgba, Bgra, Abgr, Rgb, Bgr]
        .into_iter()
        .find(|c| *c as i64 == v)
        .unwrap_or(Rgba)
}

/// The end of each constructor: the image over `data`'s buffer, as `std::make_shared<
/// PackedImageDesc>(info.ptr, ...)` makes it once the binding's checks passed.
fn finish(
    py: Python<'_>,
    this: &Bound<'_, PyAny>,
    qualified: &str,
    data: Bound<'_, PyAny>,
    buffer: Buffer,
    args: ImageArgs,
) -> PyResult<()> {
    let image = Image::make(py, vec![(data, buffer)], args)?;
    construct(this, qualified, Some(image))
}

// The four `py::init` factories (PyPackedImageDesc.cpp:15-100 @ v2.5.2): each keeps the buffer
// (`p->m_data[0] = data`), requests it (`m_data[0].request()`), checks its type and its number
// of entries (`width*height*numChannels`, a C++ `long` that wraps), then makes the library's
// description over its pointer.
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, DATA, WIDTH, HEIGHT, NUM_CHANNELS],
            ret: "None",
            call: |py, o, a, convert| {
                let data = arg!(a, 1, convert, BufferArg).0;
                let width = arg!(a, 2, convert, CLong);
                let height = arg!(a, 3, convert, CLong);
                let num_channels = arg!(a, 4, convert, CLong);
                let buffer = Buffer::request(&data)?;
                check_buffer_type(py, buffer.info(), &dtype(py, "float32")?)?;
                check_buffer_size(
                    py,
                    buffer.info(),
                    width.wrapping_mul(height).wrapping_mul(num_channels) as isize,
                )?;
                let args = ImageArgs::Packed {
                    width,
                    height,
                    channels: Channels::Count(num_channels),
                    strides: None,
                };
                finish(py, &a[0], o.params[0].ty, data, buffer, args)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                DATA,
                WIDTH,
                HEIGHT,
                NUM_CHANNELS,
                BIT_DEPTH_PARAM,
                CHAN_STRIDE,
                X_STRIDE,
                Y_STRIDE,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let data = arg!(a, 1, convert, BufferArg).0;
                let width = arg!(a, 2, convert, CLong);
                let height = arg!(a, 3, convert, CLong);
                let num_channels = arg!(a, 4, convert, CLong);
                let bit_depth = enum_param!(o, a, 5);
                let chan = arg!(a, 6, convert, i64) as isize;
                let x = arg!(a, 7, convert, i64) as isize;
                let y = arg!(a, 8, convert, i64) as isize;
                let buffer = Buffer::request(&data)?;
                check_buffer_type_of(py, buffer.info(), bit_depth)?;
                check_buffer_size(
                    py,
                    buffer.info(),
                    width.wrapping_mul(height).wrapping_mul(num_channels) as isize,
                )?;
                let bd = bit_depth_of(bit_depth).expect("a bit depth bitDepthToDtype takes");
                let args = ImageArgs::Packed {
                    width,
                    height,
                    channels: Channels::Count(num_channels),
                    strides: Some((bd, [chan, x, y])),
                };
                finish(py, &a[0], o.params[0].ty, data, buffer, args)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, DATA, WIDTH, HEIGHT, CHAN_ORDER],
            ret: "None",
            call: |py, o, a, convert| {
                let data = arg!(a, 1, convert, BufferArg).0;
                let width = arg!(a, 2, convert, CLong);
                let height = arg!(a, 3, convert, CLong);
                let chan_order = enum_param!(o, a, 4);
                let buffer = Buffer::request(&data)?;
                check_buffer_type(py, buffer.info(), &dtype(py, "float32")?)?;
                let num_channels = chan_order_to_num_channels(py, chan_order)?;
                check_buffer_size(
                    py,
                    buffer.info(),
                    width.wrapping_mul(height).wrapping_mul(num_channels) as isize,
                )?;
                let args = ImageArgs::Packed {
                    width,
                    height,
                    channels: Channels::Order(channel_ordering(chan_order)),
                    strides: None,
                };
                finish(py, &a[0], o.params[0].ty, data, buffer, args)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                DATA,
                WIDTH,
                HEIGHT,
                CHAN_ORDER,
                BIT_DEPTH_PARAM,
                CHAN_STRIDE,
                X_STRIDE,
                Y_STRIDE,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let data = arg!(a, 1, convert, BufferArg).0;
                let width = arg!(a, 2, convert, CLong);
                let height = arg!(a, 3, convert, CLong);
                let chan_order = enum_param!(o, a, 4);
                let bit_depth = enum_param!(o, a, 5);
                let chan = arg!(a, 6, convert, i64) as isize;
                let x = arg!(a, 7, convert, i64) as isize;
                let y = arg!(a, 8, convert, i64) as isize;
                let buffer = Buffer::request(&data)?;
                check_buffer_type_of(py, buffer.info(), bit_depth)?;
                let num_channels = chan_order_to_num_channels(py, chan_order)?;
                check_buffer_size(
                    py,
                    buffer.info(),
                    width.wrapping_mul(height).wrapping_mul(num_channels) as isize,
                )?;
                let bd = bit_depth_of(bit_depth).expect("a bit depth bitDepthToDtype takes");
                let args = ImageArgs::Packed {
                    width,
                    height,
                    channels: Channels::Order(channel_ordering(chan_order)),
                    strides: Some((bd, [chan, x, y])),
                };
                finish(py, &a[0], o.params[0].ty, data, buffer, args)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

/// The image of `self`, a constructed `PackedImageDesc`, or the next overload.
macro_rules! this_packed {
    ($o:expr, $a:expr) => {
        match crate::image_arg!($o, $a, 0) {
            Some(image) => image,
            None => {
                return Err(crate::py_image_desc::null_image(
                    "PackedImageDesc (a value no constructor made)",
                ));
            }
        }
    };
}

// .def("getData", [](const PyPackedImageDesc & self) { ... py::array(bitDepthToDtype(
// p->getBitDepth()), { height * width * numChannels }, { chanStrideBytes }, p->getData()); })
// (PyPackedImageDesc.cpp:102-110 @ v2.5.2): a copy of the items from the data pointer.
static GET_DATA: Function = method_def!("getData", [SELF], "numpy.ndarray", |py, o, a, _| {
    let image = this_packed!(o, a);
    let packed = image.packed.expect("a packed image");
    let bd = image.layout.bit_depth();
    let dt = bit_depth_to_dtype(py, bd as i64)?;
    let count = image
        .layout
        .height_long()
        .wrapping_mul(image.layout.width_long())
        .wrapping_mul(packed.num_channels as std::ffi::c_long);
    let items = read_items(
        py,
        &image,
        0,
        count as isize,
        packed.chan_stride_bytes,
        numpy::PyArrayDescrMethods::itemsize(&dt),
        "PackedImageDesc.getData",
    )?;
    Ok(Some(array_of(py, bd, &items)?.unbind()))
});

// .def("getChannelOrder", ...) (PyPackedImageDesc.cpp:111-115 @ v2.5.2).
static GET_CHANNEL_ORDER: Function = method_def!(
    "getChannelOrder",
    [SELF],
    CHANNEL_ORDERING,
    |py, o, a, _| {
        let image = this_packed!(o, a);
        let packed = image.packed.expect("a packed image");
        enum_out(py, CHANNEL_ORDERING, packed.chan_order as i64)
    }
);

// .def("getNumChannels", ...) (PyPackedImageDesc.cpp:116-120 @ v2.5.2).
static GET_NUM_CHANNELS: Function = method_def!("getNumChannels", [SELF], "int", |py, o, a, _| {
    let image = this_packed!(o, a);
    ok(py, image.packed.expect("a packed image").num_channels)
});

// .def("getChanStrideBytes", ...) (PyPackedImageDesc.cpp:121-125 @ v2.5.2).
static GET_CHAN_STRIDE_BYTES: Function =
    method_def!("getChanStrideBytes", [SELF], "int", |py, o, a, _| {
        let image = this_packed!(o, a);
        ok(py, image.packed.expect("a packed image").chan_stride_bytes)
    });

/// Port of `bindPyPackedImageDesc` (src/bindings/python/PyPackedImageDesc.cpp:9-126 @ v2.5.2).
pub(crate) fn bind_py_packed_image_desc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), PACKED_IMAGE_DESC)?;
    for f in [
        &INIT,
        &GET_DATA,
        &GET_CHANNEL_ORDER,
        &GET_NUM_CHANNELS,
        &GET_CHAN_STRIDE_BYTES,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
