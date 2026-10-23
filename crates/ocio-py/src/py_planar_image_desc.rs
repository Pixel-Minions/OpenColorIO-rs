// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyPlanarImageDesc` (src/bindings/python/PyPlanarImageDesc.cpp @ v2.5.2): the class
//! `PlanarImageDesc`, an image over three or four Python buffers, one per channel, which it keeps
//! alive (chunk 6.5a).

use std::ffi::c_long;

use numpy::{PyArrayDescr, PyArrayDescrMethods};
use ocio_py_ffi::Buffer;
use pyo3::prelude::*;

use crate::dispatch::{CLong, Function, Overload, Param};
use crate::py_image_desc::{Image, ImageArgs, construct, read_items};
use crate::py_transform::{BIT_DEPTH, INT, class, def, none};
use crate::py_utils::{
    BUFFER, BufferArg, array_of, bit_depth_of, bit_depth_to_bytes, bit_depth_to_dtype,
    check_buffer_size, check_buffer_type, dtype,
};
use crate::{arg, enum_param, method_def, q};

const PLANAR_IMAGE_DESC: &str = q!("PlanarImageDesc");
const SELF: Param = Param::new("self", PLANAR_IMAGE_DESC);
const R_DATA: Param = Param::new("rData", BUFFER);
const G_DATA: Param = Param::new("gData", BUFFER);
const B_DATA: Param = Param::new("bData", BUFFER);
const A_DATA: Param = Param::new("aData", BUFFER);
const WIDTH: Param = Param::new("width", INT);
const HEIGHT: Param = Param::new("height", INT);
const BIT_DEPTH_PARAM: Param = Param::new("bitDepth", BIT_DEPTH);
const X_STRIDE: Param = Param::new("xStrideBytes", INT);
const Y_STRIDE: Param = Param::new("yStrideBytes", INT);

/// The order in which the wheel's compiler evaluates the arguments of `make_shared<
/// PlanarImageDesc>(getBufferData(r), getBufferData(g), getBufferData(b), getBufferData(a),
/// ...)`, which C++ leaves unspecified: the first plane whose check fails names the error. Both
/// wheels evaluate them from the last to the first (I-327).
const LAST_TO_FIRST: bool = true;

/// Port of `getBufferData` (PyPlanarImageDesc.cpp:12-18 @ v2.5.2) of each plane, in the order the
/// wheel evaluates them ([`LAST_TO_FIRST`]): each requested, its type and its number of entries
/// (`size`) checked. The requests, in R, G, B, A order.
fn buffer_data(
    py: Python<'_>,
    planes: &[Bound<'_, PyAny>],
    dt: &Bound<'_, PyArrayDescr>,
    size: c_long,
) -> PyResult<Vec<Buffer>> {
    let mut order: Vec<usize> = (0..planes.len()).collect();
    if LAST_TO_FIRST {
        order.reverse();
    }
    let mut buffers: Vec<Option<Buffer>> = (0..planes.len()).map(|_| None).collect();
    for i in order {
        let buffer = Buffer::request(&planes[i])?;
        check_buffer_type(py, buffer.info(), dt)?;
        check_buffer_size(py, buffer.info(), size as isize)?;
        buffers[i] = Some(buffer);
    }
    Ok(buffers
        .into_iter()
        .map(|b| b.expect("each plane"))
        .collect())
}

/// The end of each constructor: the image over the planes, as `std::make_shared<
/// PlanarImageDesc>(...)` makes it once the binding's checks passed.
fn finish(
    py: Python<'_>,
    this: &Bound<'_, PyAny>,
    qualified: &str,
    planes: Vec<Bound<'_, PyAny>>,
    buffers: Vec<Buffer>,
    args: ImageArgs,
) -> PyResult<()> {
    let image = Image::make(py, planes.into_iter().zip(buffers).collect(), args)?;
    construct(this, qualified, Some(image))
}

// The four `py::init` factories (PyPlanarImageDesc.cpp:28-152 @ v2.5.2): each keeps the planes
// (`p->m_data[i]`), computes `width * height` (a C++ `long` that wraps), the planes' type
// (float32, or `bitDepthToDtype(bitDepth)`), then checks each plane (`getBufferData`) as it makes
// the library's description over their pointers.
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, R_DATA, G_DATA, B_DATA, WIDTH, HEIGHT],
            ret: "None",
            call: |py, o, a, convert| {
                let planes = vec![
                    arg!(a, 1, convert, BufferArg).0,
                    arg!(a, 2, convert, BufferArg).0,
                    arg!(a, 3, convert, BufferArg).0,
                ];
                let width = arg!(a, 4, convert, CLong);
                let height = arg!(a, 5, convert, CLong);
                let size = width.wrapping_mul(height);
                let dt = dtype(py, "float32")?;
                let buffers = buffer_data(py, &planes, &dt, size)?;
                let args = ImageArgs::Planar {
                    width,
                    height,
                    strides: None,
                };
                finish(py, &a[0], o.params[0].ty, planes, buffers, args)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, R_DATA, G_DATA, B_DATA, A_DATA, WIDTH, HEIGHT],
            ret: "None",
            call: |py, o, a, convert| {
                let planes = vec![
                    arg!(a, 1, convert, BufferArg).0,
                    arg!(a, 2, convert, BufferArg).0,
                    arg!(a, 3, convert, BufferArg).0,
                    arg!(a, 4, convert, BufferArg).0,
                ];
                let width = arg!(a, 5, convert, CLong);
                let height = arg!(a, 6, convert, CLong);
                let size = width.wrapping_mul(height);
                let dt = dtype(py, "float32")?;
                let buffers = buffer_data(py, &planes, &dt, size)?;
                let args = ImageArgs::Planar {
                    width,
                    height,
                    strides: None,
                };
                finish(py, &a[0], o.params[0].ty, planes, buffers, args)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                R_DATA,
                G_DATA,
                B_DATA,
                WIDTH,
                HEIGHT,
                BIT_DEPTH_PARAM,
                X_STRIDE,
                Y_STRIDE,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let planes = vec![
                    arg!(a, 1, convert, BufferArg).0,
                    arg!(a, 2, convert, BufferArg).0,
                    arg!(a, 3, convert, BufferArg).0,
                ];
                let width = arg!(a, 4, convert, CLong);
                let height = arg!(a, 5, convert, CLong);
                let bit_depth = enum_param!(o, a, 6);
                let x = arg!(a, 7, convert, i64) as isize;
                let y = arg!(a, 8, convert, i64) as isize;
                let size = width.wrapping_mul(height);
                let dt = bit_depth_to_dtype(py, bit_depth)?;
                let buffers = buffer_data(py, &planes, &dt, size)?;
                let bd = bit_depth_of(bit_depth).expect("a bit depth bitDepthToDtype takes");
                let args = ImageArgs::Planar {
                    width,
                    height,
                    strides: Some((bd, [x, y])),
                };
                finish(py, &a[0], o.params[0].ty, planes, buffers, args)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                R_DATA,
                G_DATA,
                B_DATA,
                A_DATA,
                WIDTH,
                HEIGHT,
                BIT_DEPTH_PARAM,
                X_STRIDE,
                Y_STRIDE,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let planes = vec![
                    arg!(a, 1, convert, BufferArg).0,
                    arg!(a, 2, convert, BufferArg).0,
                    arg!(a, 3, convert, BufferArg).0,
                    arg!(a, 4, convert, BufferArg).0,
                ];
                let width = arg!(a, 5, convert, CLong);
                let height = arg!(a, 6, convert, CLong);
                let bit_depth = enum_param!(o, a, 7);
                let x = arg!(a, 8, convert, i64) as isize;
                let y = arg!(a, 9, convert, i64) as isize;
                let size = width.wrapping_mul(height);
                let dt = bit_depth_to_dtype(py, bit_depth)?;
                let buffers = buffer_data(py, &planes, &dt, size)?;
                let bd = bit_depth_of(bit_depth).expect("a bit depth bitDepthToDtype takes");
                let args = ImageArgs::Planar {
                    width,
                    height,
                    strides: Some((bd, [x, y])),
                };
                finish(py, &a[0], o.params[0].ty, planes, buffers, args)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

/// The port's error where the wheel returns an array over a null alpha plane: pybind11 then
/// makes a new array without copying, whose memory NumPy leaves uninitialized
/// (docs/improvements.md, U-149).
fn no_alpha() -> PyErr {
    pyo3::exceptions::PyValueError::new_err(
        "PlanarImageDesc.getAData: the image has no alpha plane, and the wheel returns a new \
         array of uninitialized memory (docs/improvements.md, U-149)",
    )
}

/// `py::array(bitDepthToDtype(bd), { height * width }, { bitDepthToBytes(bd) }, plane)` of the
/// plane `index` (PyPlanarImageDesc.cpp:154-189 @ v2.5.2): a copy of its first `height * width`
/// items, contiguous from its pointer.
fn plane_data(
    py: Python<'_>,
    o: &Overload,
    a: &[Bound<'_, PyAny>],
    index: usize,
    call: &str,
) -> crate::py_transform::Out {
    let image = match crate::image_arg!(o, a, 0) {
        Some(image) => image,
        None => {
            return Err(crate::py_image_desc::null_image(
                "PlanarImageDesc (a value no constructor made)",
            ));
        }
    };
    let bd = image.layout.bit_depth();
    let dt = bit_depth_to_dtype(py, bd as i64)?;
    let count = image
        .layout
        .height_long()
        .wrapping_mul(image.layout.width_long());
    let stride = bit_depth_to_bytes(py, bd as i64)?;
    if index >= image.data.len() {
        return Err(no_alpha());
    }
    let items = read_items(
        py,
        &image,
        index,
        count as isize,
        stride,
        dt.itemsize(),
        call,
    )?;
    Ok(Some(array_of(py, bd, &items)?.unbind()))
}

// .def("getRData", ...) (PyPlanarImageDesc.cpp:154-162 @ v2.5.2).
static GET_R_DATA: Function = method_def!("getRData", [SELF], "numpy.ndarray", |py, o, a, _| {
    plane_data(py, o, a, 0, "PlanarImageDesc.getRData")
});

// .def("getGData", ...) (PyPlanarImageDesc.cpp:163-171 @ v2.5.2).
static GET_G_DATA: Function = method_def!("getGData", [SELF], "numpy.ndarray", |py, o, a, _| {
    plane_data(py, o, a, 1, "PlanarImageDesc.getGData")
});

// .def("getBData", ...) (PyPlanarImageDesc.cpp:172-180 @ v2.5.2).
static GET_B_DATA: Function = method_def!("getBData", [SELF], "numpy.ndarray", |py, o, a, _| {
    plane_data(py, o, a, 2, "PlanarImageDesc.getBData")
});

// .def("getAData", ...) (PyPlanarImageDesc.cpp:181-189 @ v2.5.2).
static GET_A_DATA: Function = method_def!("getAData", [SELF], "numpy.ndarray", |py, o, a, _| {
    plane_data(py, o, a, 3, "PlanarImageDesc.getAData")
});

/// Port of `bindPyPlanarImageDesc` (src/bindings/python/PyPlanarImageDesc.cpp:22-190 @ v2.5.2).
pub(crate) fn bind_py_planar_image_desc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), PLANAR_IMAGE_DESC)?;
    for f in [&INIT, &GET_R_DATA, &GET_G_DATA, &GET_B_DATA, &GET_A_DATA] {
        def(&cls, f)?;
    }
    Ok(())
}
