// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyImageDesc` (src/bindings/python/PyImageDesc.cpp @ v2.5.2) and `PyImageDesc`
//! (PyImageDesc.h:33-66): the class `ImageDesc`, the base of `PackedImageDesc` and
//! `PlanarImageDesc` (chunk 6.5a).
//!
//! Upstream's `PyImageDesc` keeps the buffer objects it was made from (`m_data`), so that their
//! memory lives, and the library's description over the pointer each buffer had then (`m_img`).
//! The port's library describes memory it borrows, so an [`Image`] keeps the buffer objects,
//! their addresses then, its constructor's arguments, and the getters' values, which the
//! library's checks of that description gave over the length of each buffer's memory (the
//! constructor reads none of it, as the wheel's). Whenever it is used, it requests the
//! buffers again ([`Image::request`]), and processing makes the library's description again
//! over their memory, with the same checks ([`describe`]):
//! - a buffer whose memory has moved since (a `bytearray` or an array resized) is the port's
//!   error, where the wheel uses the old address (U-147);
//! - the memory is lent as slices while the GIL is released (`ocio_py_ffi::with_bytes`).

use std::ffi::c_long;
use std::sync::Arc;

use ocio_ops::image_desc::{
    At, Bytes, ImageDesc, ImageDescMut, ImageLayout, PackedChannels, PackedImageDesc,
    PlanarImageDesc, packed_layout, planar_layout,
};
use ocio_ops::open_color_types::{BitDepth, ChannelOrdering};
use ocio_py_ffi::Buffer;
use pyo3::prelude::*;

use crate::dispatch::{Function, Param};
use crate::py_transform::{BIT_DEPTH, class, def, enum_out, none, ok};
use crate::pybind11;
use crate::{method_def, q};

pub(crate) const IMAGE_DESC: &str = q!("ImageDesc");
const SELF: Param = Param::new("self", IMAGE_DESC);

/// How a packed image's constructor gave its channels: `numChannels` or `chanOrder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channels {
    Count(c_long),
    Order(ChannelOrdering),
}

/// The arguments an image description was made with, but its buffers: what makes the library's
/// description again over their memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageArgs {
    /// `PackedImageDesc(data, width, height, numChannels | chanOrder[, bitDepth,
    /// chanStrideBytes, xStrideBytes, yStrideBytes])`.
    Packed {
        width: c_long,
        height: c_long,
        channels: Channels,
        strides: Option<(BitDepth, [isize; 3])>,
    },
    /// `PlanarImageDesc(rData, gData, bData[, aData], width, height[, bitDepth, xStrideBytes,
    /// yStrideBytes])`.
    Planar {
        width: c_long,
        height: c_long,
        strides: Option<(BitDepth, [isize; 2])>,
    },
}

/// A packed image's own getters: `getChannelOrder`, `getNumChannels`, `getChanStrideBytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackedGetters {
    pub(crate) chan_order: ChannelOrdering,
    pub(crate) num_channels: usize,
    pub(crate) chan_stride_bytes: isize,
}

/// What a `PyImageDesc` holds once constructed: `m_data`, the buffer objects (the packed image's
/// one, or the planes R, G, B and, if given, A); the address each buffer's first item had then,
/// which the wheel's `m_img` keeps; the constructor's arguments; and the getters' values.
#[derive(Debug)]
pub(crate) struct Image {
    pub(crate) data: Vec<Py<PyAny>>,
    pub(crate) ptrs: Vec<usize>,
    pub(crate) args: ImageArgs,
    pub(crate) layout: ImageLayout,
    pub(crate) packed: Option<PackedGetters>,
}

/// The value of an instance of `ImageDesc` or a subclass: its image, `None` for one that
/// `ImageDesc()` made, whose `m_img` is null.
#[pyo3::pyclass(frozen, module = "pybind11_builtins", name = "_image_desc")]
#[derive(Debug)]
pub(crate) struct ImageHolder(pub(crate) Option<Arc<Image>>);

/// The port's error where the wheel dereferences the null `m_img` of an `ImageDesc()`
/// (docs/improvements.md, U-146).
pub(crate) fn null_image(call: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{call}: ImageDesc() describes no image, and the wheel dereferences its null image \
         (its process dies; docs/improvements.md, U-146)"
    ))
}

/// The image `obj` holds as an instance of the class `qualified` (`Ok(None)`: not an instance,
/// the next overload; `Some(None)`: an `ImageDesc()`).
pub(crate) fn holder(
    obj: &Bound<'_, PyAny>,
    qualified: &str,
) -> PyResult<Option<Option<Arc<Image>>>> {
    let Some(this) = pybind11::instance_of(obj, qualified) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, qualified)?;
    Ok(Some(value.cast::<ImageHolder>()?.get().0.clone()))
}

/// The image of the overload's argument `$i` (its type names the class), or the next overload.
#[macro_export]
macro_rules! image_arg {
    ($o:expr, $a:expr, $i:expr) => {
        match $crate::py_image_desc::holder(&$a[$i], $o.params[$i].ty)? {
            Some(image) => image,
            None => return Ok(None),
        }
    };
}

/// A constructor's result: `image` as the value of `this`, an instance of the class `qualified`
/// (pybind11's `py::init` of a factory returning a new `PyImageDesc`).
pub(crate) fn construct(
    this: &Bound<'_, PyAny>,
    qualified: &str,
    image: Option<Image>,
) -> PyResult<()> {
    let holder = ImageHolder(image.map(Arc::new));
    pybind11::set_value(this, qualified, Bound::new(this.py(), holder)?.into_any())
}

/// The library's description of an image over memory `with_bytes` lent: a packed or a planar
/// one.
pub(crate) enum Described<B> {
    Packed(PackedImageDesc<B>),
    Planar(PlanarImageDesc<B>),
}

impl<B: AsRef<[u8]>> Described<B> {
    pub(crate) fn desc(&self) -> &dyn ImageDesc {
        match self {
            Described::Packed(d) => d,
            Described::Planar(d) => d,
        }
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> Described<B> {
    pub(crate) fn desc_mut(&mut self) -> &mut dyn ImageDescMut {
        match self {
            Described::Packed(d) => d,
            Described::Planar(d) => d,
        }
    }
}

/// Why the library's description can't be made over what `with_bytes` lent: the library's own
/// error, as upstream's constructor raises it.
#[derive(Debug)]
pub(crate) enum DescribeError {
    Library(ocio::Exception),
}

impl DescribeError {
    pub(crate) fn into_py(self, py: Python<'_>) -> PyErr {
        match self {
            DescribeError::Library(e) => crate::py_open_color_io::to_py_err(py, &e),
        }
    }
}

/// A channel count or size as the library's constructors take it: a C++ `long` that isn't
/// positive stands for the library's own invalid value, which its checks refuse at the same
/// place (`usize` past `long` is -1 there).
pub(crate) fn size(v: c_long) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// Memory for a buffer that has no items but an address: the library takes an empty slice for a
/// null pointer, which the wheel's buffer isn't. Its byte is never read: an image over a buffer
/// without items has no pixels (its dimensions are refused), or reaches outside it (D-2).
const NOT_NULL: usize = 1;

/// The library's description of an image made with `args` over `memory`: for each buffer, in
/// the order of `Image::data`, its memory and where its first item is in it.
pub(crate) fn describe<B: AsRef<[u8]>>(
    args: &ImageArgs,
    memory: Vec<(B, usize)>,
) -> Result<Described<B>, DescribeError>
where
    At<Bytes<B>>: ocio_ops::image_desc::PixelData<Bytes = B>,
{
    let lib = DescribeError::Library;
    match *args {
        ImageArgs::Packed {
            width,
            height,
            channels,
            strides,
        } => {
            let (bytes, offset) = memory.into_iter().next().expect("a packed image's buffer");
            let data = At(Bytes(bytes), offset);
            let (w, h) = (size(width), size(height));
            let d = match (channels, strides) {
                (Channels::Count(n), None) => PackedImageDesc::new(data, w, h, size(n)),
                (Channels::Order(order), None) => {
                    PackedImageDesc::with_channel_order(data, w, h, order)
                }
                (Channels::Count(n), Some((bd, [c, x, y]))) => {
                    PackedImageDesc::with_strides(data, w, h, size(n), bd, c, x, y)
                }
                (Channels::Order(order), Some((bd, [c, x, y]))) => {
                    PackedImageDesc::with_channel_order_and_strides(data, w, h, order, bd, c, x, y)
                }
            };
            d.map(Described::Packed).map_err(lib)
        }
        ImageArgs::Planar {
            width,
            height,
            strides,
        } => {
            let (w, h) = (size(width), size(height));
            let (bd, [x, y]) =
                strides.unwrap_or((BitDepth::F32, [ocio_ops::image_desc::AUTO_STRIDE; 2]));
            let mut planes = memory
                .into_iter()
                .map(|(bytes, offset)| At(Bytes(bytes), offset));
            let (r, g, b) = (
                planes.next().expect("R"),
                planes.next().expect("G"),
                planes.next().expect("B"),
            );
            let a = planes.next();
            let d = match strides {
                None => PlanarImageDesc::new(r, g, b, a, w, h),
                Some(_) => PlanarImageDesc::with_strides(r, g, b, a, w, h, bd, x, y),
            };
            d.map(Described::Planar).map_err(lib)
        }
    }
}

/// [`describe`] of an image whose buffers are all in one buffer (`memory`), each at its offset:
/// a packed image's one; the planes R, G, B and, if given, A, as C++ passes pointers into one
/// allocation.
pub(crate) fn describe_in_one<B: AsRef<[u8]>>(
    args: &ImageArgs,
    memory: B,
    offsets: &[usize],
) -> Result<Described<B>, DescribeError>
where
    At<Bytes<B>>: ocio_ops::image_desc::PixelData<Bytes = B>,
{
    let ImageArgs::Planar {
        width,
        height,
        strides,
    } = *args
    else {
        return describe(args, vec![(memory, offsets[0])]);
    };
    let (bd, [x, y]) = strides.unwrap_or((BitDepth::F32, [ocio_ops::image_desc::AUTO_STRIDE; 2]));
    PlanarImageDesc::in_one_buffer(
        At(Bytes(memory), 0),
        offsets[0],
        offsets[1],
        offsets[2],
        offsets.get(3).copied(),
        size(width),
        size(height),
        bd,
        x,
        y,
    )
    .map(Described::Planar)
    .map_err(DescribeError::Library)
}

/// What [`describe`] makes of `args` over memory of the lengths `memory` gives, each buffer's
/// first item at its offset (as [`read_memory`] gives them: one byte for a buffer with an address
/// and no items), without the memory: the layout, and the packed image's own getters.
fn layout_of(
    args: &ImageArgs,
    memory: &[(usize, usize)],
) -> ocio::Result<(ImageLayout, Option<PackedGetters>)> {
    match *args {
        ImageArgs::Packed {
            width,
            height,
            channels,
            strides,
        } => {
            let (len, origin) = memory[0];
            let channels = match channels {
                Channels::Count(n) => PackedChannels::Count(size(n)),
                Channels::Order(order) => PackedChannels::Order(order),
            };
            let made = packed_layout(len, origin, size(width), size(height), channels, strides)?;
            let getters = PackedGetters {
                chan_order: made.chan_order,
                num_channels: made.num_channels,
                chan_stride_bytes: made.chan_stride_bytes,
            };
            Ok((made.layout, Some(getters)))
        }
        ImageArgs::Planar {
            width,
            height,
            strides,
        } => {
            let layout = planar_layout(
                memory[0],
                memory[1],
                memory[2],
                memory.get(3).copied(),
                size(width),
                size(height),
                strides,
            )?;
            Ok((layout, None))
        }
    }
}

impl Image {
    /// Makes an image of `args` over `buffers` (each object with its request), as the
    /// constructor does once the binding's checks passed: the library's checks and errors, and
    /// the getters' values. As the wheel's constructor, which reads none of the pixels, it
    /// reads no memory: the library's description is checked over the length of each buffer's
    /// span ([`layout_of`]), so a constructor runs while another thread's call writes the
    /// buffer.
    pub(crate) fn make(
        py: Python<'_>,
        buffers: Vec<(Bound<'_, PyAny>, Buffer)>,
        args: ImageArgs,
    ) -> PyResult<Image> {
        let memory: Vec<(usize, usize)> = buffers
            .iter()
            .map(|(_, b)| {
                let (start, end) = b.span();
                if start == end && b.info().ptr != 0 {
                    (NOT_NULL, 0)
                } else {
                    (end - start, b.span_offset())
                }
            })
            .collect();
        let (layout, packed) =
            layout_of(&args, &memory).map_err(|e| crate::py_open_color_io::to_py_err(py, &e))?;
        Ok(Image {
            data: buffers.iter().map(|(o, _)| o.clone().unbind()).collect(),
            ptrs: buffers.iter().map(|(_, b)| b.info().ptr).collect(),
            args,
            layout,
            packed,
        })
    }

    /// Each buffer requested again, as the image describes its memory: the port's error for one
    /// whose first item has moved since the image was made (U-147).
    pub(crate) fn request(&self, py: Python<'_>) -> PyResult<Vec<Buffer>> {
        self.data
            .iter()
            .zip(&self.ptrs)
            .map(|(obj, &ptr)| {
                let buffer = Buffer::request(obj.bind(py))?;
                if buffer.info().ptr != ptr {
                    return Err(moved());
                }
                Ok(buffer)
            })
            .collect()
    }

    /// The address `operator<<` prints for `m_img`: the image's own.
    fn address(this: &Option<Arc<Image>>) -> usize {
        this.as_ref().map_or(0, |i| Arc::as_ptr(i).addr())
    }
}

/// The port's error for a buffer whose memory moved after the image was made
/// (docs/improvements.md, U-147).
pub(crate) fn moved() -> PyErr {
    pyo3::exceptions::PyBufferError::new_err(
        "the buffer's memory moved since the image description was made over it (it was \
         resized), and the wheel reads and writes the memory it had then (docs/improvements.md, \
         U-147)",
    )
}

/// The memory of buffers lent to read, for [`describe`]: each buffer's region and the offset of
/// its first item, or one byte for a buffer with an address but no items ([`NOT_NULL`]).
pub(crate) fn read_memory<'b>(
    regions: &[ocio_py_ffi::Bytes<'b>],
    places: &[(usize, usize)],
    addressed: &[bool],
) -> Vec<(&'b [u8], usize)> {
    static ONE: [u8; NOT_NULL] = [0];
    places
        .iter()
        .zip(addressed)
        .map(|(&(r, offset), &addressed)| {
            let bytes: &'b [u8] = match &regions[r] {
                ocio_py_ffi::Bytes::Read(b) => b,
                ocio_py_ffi::Bytes::Write(_) => &[],
            };
            if bytes.is_empty() && addressed {
                (&ONE[..], 0)
            } else {
                (bytes, offset)
            }
        })
        .collect()
}

// .def(py::init<>()) (PyImageDesc.cpp:16-17 @ v2.5.2): an image description without an image.
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[crate::dispatch::Overload {
        params: &[SELF],
        ret: "None",
        call: |py, o, a, _| {
            construct(&a[0], o.params[0].ty, None)?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("__repr__", [](const PyImageDesc & self) { std::ostringstream os; os << self.m_img; ... })
// (PyImageDesc.cpp:19-24 @ v2.5.2): `operator<<` of the `shared_ptr`, its pointer, as the
// compiler's C++ library prints a pointer: MSVC's 16 upper-case hexadecimal digits, libstdc++'s
// `0x` and lower-case digits (`0` for a null pointer). The address is the port's own (D-8).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let image = crate::image_arg!(o, a, 0);
    ok(py, pointer_text(Image::address(&image)))
});

/// A pointer as `std::ostream::operator<<(const void *)` prints it in the wheel.
pub(crate) fn pointer_text(address: usize) -> String {
    if cfg!(windows) {
        format!("{address:016X}")
    } else if address == 0 {
        "0".to_string()
    } else {
        format!("{address:#x}")
    }
}

/// The image of `self`, which a getter reads: the port's error for an `ImageDesc()` (U-146).
macro_rules! this_image {
    ($o:expr, $a:expr, $call:expr) => {
        match $crate::image_arg!($o, $a, 0) {
            Some(image) => image,
            None => return Err(null_image($call)),
        }
    };
}

// .def("getBitDepth", &PyImageDesc::getBitDepth) (PyImageDesc.cpp:26-27 @ v2.5.2).
static GET_BIT_DEPTH: Function = method_def!("getBitDepth", [SELF], BIT_DEPTH, |py, o, a, _| {
    let image = this_image!(o, a, "ImageDesc.getBitDepth");
    enum_out(py, BIT_DEPTH, image.layout.bit_depth() as i64)
});

// .def("getWidth", &PyImageDesc::getWidth) (PyImageDesc.cpp:28-29 @ v2.5.2).
static GET_WIDTH: Function = method_def!("getWidth", [SELF], "int", |py, o, a, _| {
    let image = this_image!(o, a, "ImageDesc.getWidth");
    ok(py, image.layout.width_long())
});

// .def("getHeight", &PyImageDesc::getHeight) (PyImageDesc.cpp:30-31 @ v2.5.2).
static GET_HEIGHT: Function = method_def!("getHeight", [SELF], "int", |py, o, a, _| {
    let image = this_image!(o, a, "ImageDesc.getHeight");
    ok(py, image.layout.height_long())
});

// .def("getXStrideBytes", &PyImageDesc::getXStrideBytes) (PyImageDesc.cpp:32-33 @ v2.5.2).
static GET_X_STRIDE_BYTES: Function =
    method_def!("getXStrideBytes", [SELF], "int", |py, o, a, _| {
        let image = this_image!(o, a, "ImageDesc.getXStrideBytes");
        ok(py, image.layout.x_stride_bytes())
    });

// .def("getYStrideBytes", &PyImageDesc::getYStrideBytes) (PyImageDesc.cpp:34-35 @ v2.5.2).
static GET_Y_STRIDE_BYTES: Function =
    method_def!("getYStrideBytes", [SELF], "int", |py, o, a, _| {
        let image = this_image!(o, a, "ImageDesc.getYStrideBytes");
        ok(py, image.layout.y_stride_bytes())
    });

// .def("isRGBAPacked", &PyImageDesc::isRGBAPacked) (PyImageDesc.cpp:36-37 @ v2.5.2).
static IS_RGBA_PACKED: Function = method_def!("isRGBAPacked", [SELF], "bool", |py, o, a, _| {
    let image = this_image!(o, a, "ImageDesc.isRGBAPacked");
    ok(py, image.layout.is_rgba_packed())
});

// .def("isFloat", &PyImageDesc::isFloat) (PyImageDesc.cpp:38-39 @ v2.5.2).
static IS_FLOAT: Function = method_def!("isFloat", [SELF], "bool", |py, o, a, _| {
    let image = this_image!(o, a, "ImageDesc.isFloat");
    ok(py, image.layout.is_float())
});

/// Port of `bindPyImageDesc` (src/bindings/python/PyImageDesc.cpp:8-44 @ v2.5.2): the module's
/// `AutoStride`, the base class's methods in their order, then the subclasses'.
pub(crate) fn bind_py_image_desc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.setattr("AutoStride", ocio_ops::image_desc::AUTO_STRIDE)?;
    let cls = class(py, IMAGE_DESC)?;
    for f in [
        &INIT,
        &REPR,
        &GET_BIT_DEPTH,
        &GET_WIDTH,
        &GET_HEIGHT,
        &GET_X_STRIDE_BYTES,
        &GET_Y_STRIDE_BYTES,
        &IS_RGBA_PACKED,
        &IS_FLOAT,
    ] {
        def(&cls, f)?;
    }
    crate::py_packed_image_desc::bind_py_packed_image_desc(m)?;
    crate::py_planar_image_desc::bind_py_planar_image_desc(m)?;
    Ok(())
}

/// The port's error where the wheel reads past a buffer's items (docs/improvements.md, U-148).
pub(crate) fn outside(call: &str) -> PyErr {
    pyo3::exceptions::PyBufferError::new_err(format!(
        "{call}: the items reach outside the buffer's memory, which the wheel reads past \
         (docs/improvements.md, U-148)"
    ))
}

/// `py::array(dt, {count}, {stride}, ptr)` of the buffer `index` of `image`, as pybind11 makes it
/// without a base object: a copy of the `count` items of `itemsize` bytes, `stride` bytes apart
/// from the buffer's first item (`ptr`), whose bytes this returns in order. NumPy's error for a
/// negative count; the port's error for items outside the buffer's memory (U-148), named
/// `call`.
pub(crate) fn read_items(
    py: Python<'_>,
    image: &Image,
    index: usize,
    count: isize,
    stride: isize,
    itemsize: usize,
    call: &str,
) -> PyResult<Vec<u8>> {
    if count < 0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "negative dimensions are not allowed",
        ));
    }
    let buffers = image.request(py)?;
    let buffer = &buffers[index];
    let items = ocio_py_ffi::with_bytes(py, &[(buffer, false)], move |regions, places| {
        let bytes: &[u8] = match &regions[0] {
            ocio_py_ffi::Bytes::Read(b) => b,
            ocio_py_ffi::Bytes::Write(b) => b,
        };
        if count == 0 {
            return Some(Vec::new());
        }
        let first = places[0].1 as i128;
        let last = first + (count as i128 - 1) * stride as i128;
        let (low, high) = (first.min(last), first.max(last) + itemsize as i128);
        if low < 0 || high > bytes.len() as i128 {
            return None;
        }
        let mut items = Vec::with_capacity(count as usize * itemsize);
        for k in 0..count as i128 {
            let at = (first + k * stride as i128) as usize;
            items.extend_from_slice(&bytes[at..at + itemsize]);
        }
        Some(items)
    })?;
    items.ok_or_else(|| outside(call))
}
