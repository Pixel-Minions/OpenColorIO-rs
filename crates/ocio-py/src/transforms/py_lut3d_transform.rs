// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLut3DTransform` (src/bindings/python/transforms/PyLut3DTransform.cpp @ v2.5.2), with
//! its LUT data as NumPy arrays and Python buffers (chunk 6.5b).

use numpy::PyArray1;
use ocio::Transform;
use ocio::transforms::lut3d_transform::Lut3DTransform;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use pyo3::prelude::*;

use crate::dispatch::{CULong, Function, Overload, Param};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    BIT_DEPTH, DIRECTION_DEFAULT, FLOAT, INT, TransformRc, bit_depth, class, construct, def,
    default, enum_out, none, ok, outside_enumerators, set_direction, validate, with,
};
use crate::py_utils::{
    BUFFER, BufferArg, bit_depth_of, buffer_lut3d_grid_size, check_buffer_type, dtype,
};
use crate::transforms::py_file_transform::interpolation;
use crate::{arg, enum_param, method_def, q, ref_rc, this_rc};

const LUT3D_TRANSFORM: &str = q!("Lut3DTransform");
const INTERPOLATION: &str = q!("Interpolation");
const SELF: Param = Param::new("self", LUT3D_TRANSFORM);

// .def(py::init([]() { return Lut3DTransform::Create(); })), .def(py::init([](unsigned long
// gridSize) { return Lut3DTransform::Create(gridSize); }), "gridSize"_a),
// .def(py::init([](gridSize, fileOutputBitDepth, interpolation, dir) {...}), "gridSize"_a =
// DEFAULT->getGridSize(), ..., "direction"_a = DEFAULT->getDirection())
// (PyLut3DTransform.cpp:19-46 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::Lut3D(Lut3DTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("gridSize", INT)],
            ret: "None",
            call: |py, o, a, convert| {
                let grid_size = arg!(a, 1, convert, CULong);
                let t = py_result(py, Lut3DTransform::with_grid_size(grid_size))?;
                construct(&a[0], o.params[0].ty, TransformRc::new(Transform::Lut3D(t)))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "gridSize",
                    ty: INT,
                    default: default("2", |py| Ok(pyo3::types::PyInt::new(py, 2).into_any())),
                },
                Param {
                    name: "fileOutputBitDepth",
                    ty: BIT_DEPTH,
                    default: default("<BitDepth.BIT_DEPTH_UNKNOWN: 0>", |py| {
                        crate::enums::to_py(&crate::enums::enum_class(py, BIT_DEPTH)?, 0)
                    }),
                },
                Param {
                    name: "interpolation",
                    ty: INTERPOLATION,
                    default: default("<Interpolation.INTERP_DEFAULT: 254>", |py| {
                        crate::enums::to_py(&crate::enums::enum_class(py, INTERPOLATION)?, 254)
                    }),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let grid_size = arg!(a, 1, convert, CULong);
                let file_bit_depth = enum_param!(o, a, 2);
                let interp = enum_param!(o, a, 3);
                let dir = enum_param!(o, a, 4);
                let mut t = py_result(py, Lut3DTransform::with_grid_size(grid_size))?;
                // A bit depth outside the enumerators, which `validate` passes: the port refuses
                // it (U-112) once it passed. An interpolation outside them fails `validate` as
                // INTERP_UNKNOWN does ("... algorithm: unknown.", `InterpolationToString`).
                let library_bit_depth = bit_depth_of(file_bit_depth);
                if let Some(b) = library_bit_depth {
                    t.set_file_output_bit_depth(b);
                }
                t.set_interpolation(interpolation(interp).unwrap_or(Interpolation::Unknown));
                let rc = TransformRc::new(Transform::Lut3D(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                if library_bit_depth.is_none() {
                    return Err(outside_enumerators(BIT_DEPTH, file_bit_depth));
                }
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getFileOutputBitDepth", ...), .def("setFileOutputBitDepth", ..., "bitDepth"_a)
// (PyLut3DTransform.cpp:48-51 @ v2.5.2).
static GET_FILE_OUTPUT_BIT_DEPTH: Function =
    method_def!("getFileOutputBitDepth", [SELF], BIT_DEPTH, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let bd = with(&rc, |t: &mut Lut3DTransform| t.file_output_bit_depth())?;
        enum_out(py, BIT_DEPTH, bd as i64)
    });

static SET_FILE_OUTPUT_BIT_DEPTH: Function = method_def!(
    "setFileOutputBitDepth",
    [SELF, Param::new("bitDepth", BIT_DEPTH)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let bd = bit_depth(enum_param!(o, a, 1))?;
        with(&rc, |t: &mut Lut3DTransform| {
            t.set_file_output_bit_depth(bd)
        })?;
        none(py)
    }
);

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyLut3DTransform.cpp:52-55 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &Lut3DTransform::equals, "other"_a) (PyLut3DTransform.cpp:56-57 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", LUT3D_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal = crate::py_transform::equals::<Lut3DTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getGridSize", ...), .def("setGridSize", ..., "gridSize"_a) (PyLut3DTransform.cpp:58-61
// @ v2.5.2).
static GET_GRID_SIZE: Function = method_def!("getGridSize", [SELF], "int", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut Lut3DTransform| t.grid_size())?)
});

static SET_GRID_SIZE: Function = method_def!(
    "setGridSize",
    [SELF, Param::new("gridSize", INT)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let grid_size = arg!(a, 1, convert, CULong);
        py_result(
            py,
            with(&rc, |t: &mut Lut3DTransform| t.set_grid_size(grid_size))?,
        )?;
        none(py)
    }
);

// .def("getValue", [](Lut3DTransformRcPtr & self, unsigned long indexR, unsigned long indexG,
// unsigned long indexB) { ... py::make_tuple(r, g, b); }, "indexR"_a, "indexG"_a, "indexB"_a)
// (PyLut3DTransform.cpp:62-72 @ v2.5.2).
static GET_VALUE: Function = method_def!(
    "getValue",
    [
        SELF,
        Param::new("indexR", INT),
        Param::new("indexG", INT),
        Param::new("indexB", INT)
    ],
    "tuple",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let r = arg!(a, 1, convert, CULong);
        let g = arg!(a, 2, convert, CULong);
        let b = arg!(a, 3, convert, CULong);
        let [vr, vg, vb] = py_result(py, with(&rc, |t: &mut Lut3DTransform| t.value(r, g, b))?)?;
        ok(py, (f64::from(vr), f64::from(vg), f64::from(vb)))
    }
);

// .def("setValue", &Lut3DTransform::setValue, "indexR"_a, "indexG"_a, "indexB"_a, "r"_a,
// "g"_a, "b"_a) (PyLut3DTransform.cpp:73-75 @ v2.5.2).
static SET_VALUE: Function = method_def!(
    "setValue",
    [
        SELF,
        Param::new("indexR", INT),
        Param::new("indexG", INT),
        Param::new("indexB", INT),
        Param::new("r", FLOAT),
        Param::new("g", FLOAT),
        Param::new("b", FLOAT)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let ir = arg!(a, 1, convert, CULong);
        let ig = arg!(a, 2, convert, CULong);
        let ib = arg!(a, 3, convert, CULong);
        let r = arg!(a, 4, convert, f32);
        let g = arg!(a, 5, convert, f32);
        let b = arg!(a, 6, convert, f32);
        py_result(
            py,
            with(&rc, |t: &mut Lut3DTransform| {
                t.set_value(ir, ig, ib, r, g, b)
            })?,
        )?;
        none(py)
    }
);

/// Why `setData` stopped with the GIL released: the library's error, or a value past the
/// buffer's memory.
enum SetDataError {
    Library(ocio::Exception),
    Outside,
}

// .def("setData", [](Lut3DTransformRcPtr & self, py::buffer & data) {...}, "data"_a)
// (PyLut3DTransform.cpp:76-104 @ v2.5.2): the buffer's type checked and its grid size computed,
// then, with the GIL released, the grid size set and each entry, blue fastest, set from three
// floats read one after another from its first item (its strides aside), at `3 * ((indexR*gs +
// indexG)*gs + indexB)` in C++'s `unsigned long`. A value past the buffer's memory is the port's
// error, where the wheel reads past it (U-148).
static SET_DATA: Function = method_def!(
    "setData",
    [SELF, Param::new("data", BUFFER)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let data = arg!(a, 1, convert, BufferArg).0;
        let buffer = ocio_py_ffi::Buffer::request(&data)?;
        check_buffer_type(py, buffer.info(), &dtype(py, "float32")?)?;
        let gs = buffer_lut3d_grid_size(py, buffer.info())?;
        let done = ocio_py_ffi::with_bytes(py, &[(&buffer, false)], move |regions, places| {
            let bytes: &[u8] = match &regions[0] {
                ocio_py_ffi::Bytes::Read(b) => b,
                ocio_py_ffi::Bytes::Write(b) => b,
            };
            let first = places[0].1;
            let value = |i: CULong| -> Option<f32> {
                let at = first.checked_add((i as usize).checked_mul(4)?)?;
                let b = bytes.get(at..at.checked_add(4)?)?;
                Some(f32::from_ne_bytes(b.try_into().ok()?))
            };
            with(&rc, |t: &mut Lut3DTransform| -> Result<(), SetDataError> {
                t.set_grid_size(gs).map_err(SetDataError::Library)?;
                for index_r in 0..gs {
                    for index_g in 0..gs {
                        for index_b in 0..gs {
                            let i = index_r
                                .wrapping_mul(gs)
                                .wrapping_add(index_g)
                                .wrapping_mul(gs)
                                .wrapping_add(index_b)
                                .wrapping_mul(3);
                            let (Some(r), Some(g), Some(b)) =
                                (value(i), value(i.wrapping_add(1)), value(i.wrapping_add(2)))
                            else {
                                return Err(SetDataError::Outside);
                            };
                            t.set_value(index_r, index_g, index_b, r, g, b)
                                .map_err(SetDataError::Library)?;
                        }
                    }
                }
                Ok(())
            })
        })?;
        match done? {
            Ok(()) => none(py),
            Err(SetDataError::Library(e)) => Err(crate::py_open_color_io::to_py_err(py, &e)),
            Err(SetDataError::Outside) => {
                Err(crate::py_image_desc::outside("Lut3DTransform.setData"))
            }
        }
    }
);

// .def("getData", [](Lut3DTransformRcPtr & self) {...}) (PyLut3DTransform.cpp:105-134 @
// v2.5.2): with the GIL released, each entry's three floats, blue fastest; then a new float32
// array of them.
static GET_DATA: Function = method_def!("getData", [SELF], "numpy.ndarray", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let values = py.detach(|| {
        with(&rc, |t: &mut Lut3DTransform| -> ocio::Result<Vec<f32>> {
            let gs = t.grid_size();
            let mut values = Vec::new();
            for index_r in 0..gs {
                for index_g in 0..gs {
                    for index_b in 0..gs {
                        values.extend_from_slice(&t.value(index_r, index_g, index_b)?);
                    }
                }
            }
            Ok(values)
        })
    })?;
    let values = py_result(py, values)?;
    Ok(Some(
        PyArray1::<f32>::from_slice(py, &values).into_any().unbind(),
    ))
});

// .def("getInterpolation", ...), .def("setInterpolation", ..., "interpolation"_a)
// (PyLut3DTransform.cpp:135-138 @ v2.5.2).
static GET_INTERPOLATION: Function =
    method_def!("getInterpolation", [SELF], INTERPOLATION, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let interp = with(&rc, |t: &mut Lut3DTransform| t.interpolation())?;
        enum_out(py, INTERPOLATION, interp as i64)
    });

static SET_INTERPOLATION: Function = method_def!(
    "setInterpolation",
    [SELF, Param::new("interpolation", INTERPOLATION)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let v = enum_param!(o, a, 1);
        let interp = interpolation(v).ok_or_else(|| outside_enumerators(INTERPOLATION, v))?;
        with(&rc, |t: &mut Lut3DTransform| t.set_interpolation(interp))?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyLut3DTransform` (src/bindings/python/transforms/PyLut3DTransform.cpp:11-141
/// @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), LUT3D_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FILE_OUTPUT_BIT_DEPTH,
        &SET_FILE_OUTPUT_BIT_DEPTH,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_GRID_SIZE,
        &SET_GRID_SIZE,
        &GET_VALUE,
        &SET_VALUE,
        &SET_DATA,
        &GET_DATA,
        &GET_INTERPOLATION,
        &SET_INTERPOLATION,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
