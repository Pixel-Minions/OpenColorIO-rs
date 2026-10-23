// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLut1DTransform` (src/bindings/python/transforms/PyLut1DTransform.cpp @ v2.5.2), with
//! its LUT data as NumPy arrays and Python buffers (chunk 6.5b).

use numpy::PyArray1;
use ocio::transforms::lut1d_transform::Lut1DTransform;
use ocio::{Lut1DHueAdjust, Transform};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use pyo3::prelude::*;

use crate::dispatch::{CULong, Function, Overload, Param};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    BIT_DEPTH, DIRECTION_DEFAULT, FLOAT, INT, TransformRc, bit_depth, class, construct, def,
    default, enum_out, none, ok, outside_enumerators, set_direction, validate, with,
};
use crate::py_utils::{
    BUFFER, BufferArg, bit_depth_of, buffer_size, check_buffer_divisible, check_buffer_type, dtype,
};
use crate::transforms::py_file_transform::interpolation;
use crate::{arg, enum_param, method_def, q, ref_rc, this_rc};

const LUT1D_TRANSFORM: &str = q!("Lut1DTransform");
const LUT1D_HUE_ADJUST: &str = q!("Lut1DHueAdjust");
const INTERPOLATION: &str = q!("Interpolation");
const SELF: Param = Param::new("self", LUT1D_TRANSFORM);

/// The library's `Lut1DHueAdjust` of the C++ value `v`, if it is an enumerator.
fn hue_adjust(v: i64) -> Option<Lut1DHueAdjust> {
    use Lut1DHueAdjust::*;
    [None, Dw3, Wypn].into_iter().find(|h| *h as i64 == v)
}

/// An enum default of the class `qualified`: its repr and its value.
macro_rules! enum_default {
    ($name:literal, $qualified:expr, $repr:literal, $value:literal) => {
        Param {
            name: $name,
            ty: $qualified,
            default: default($repr, |py| {
                crate::enums::to_py(&crate::enums::enum_class(py, $qualified)?, $value)
            }),
        }
    };
}

const fn bool_default(name: &'static str) -> Param {
    Param {
        name,
        ty: "bool",
        default: default("False", |py| {
            Ok(pyo3::types::PyBool::new(py, false).to_owned().into_any())
        }),
    }
}

// .def(py::init([]() { return Lut1DTransform::Create(); })), .def(py::init([](unsigned long
// length, bool isHalfDomain) { return Lut1DTransform::Create(length, isHalfDomain); }),
// "length"_a, "inputHalfDomain"_a), .def(py::init([](length, isHalfDomain, isRawHalfs,
// fileOutputBitDepth, hueAdjust, interpolation, dir) {...}), "length"_a = DEFAULT->getLength(),
// ..., "direction"_a = DEFAULT->getDirection()) (PyLut1DTransform.cpp:17-52 @ v2.5.2).
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
                let rc = TransformRc::new(Transform::Lut1D(Lut1DTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("length", INT),
                Param::new("inputHalfDomain", "bool"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let length = arg!(a, 1, convert, CULong);
                let half = arg!(a, 2, convert, bool);
                let t = py_result(py, Lut1DTransform::with_length(length, half))?;
                construct(&a[0], o.params[0].ty, TransformRc::new(Transform::Lut1D(t)))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "length",
                    ty: INT,
                    default: default("2", |py| Ok(pyo3::types::PyInt::new(py, 2).into_any())),
                },
                bool_default("inputHalfDomain"),
                bool_default("outputRawHalfs"),
                enum_default!(
                    "fileOutputBitDepth",
                    BIT_DEPTH,
                    "<BitDepth.BIT_DEPTH_UNKNOWN: 0>",
                    0
                ),
                enum_default!(
                    "hueAdjust",
                    LUT1D_HUE_ADJUST,
                    "<Lut1DHueAdjust.HUE_NONE: 0>",
                    0
                ),
                enum_default!(
                    "interpolation",
                    INTERPOLATION,
                    "<Interpolation.INTERP_DEFAULT: 254>",
                    254
                ),
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let length = arg!(a, 1, convert, CULong);
                let half = arg!(a, 2, convert, bool);
                let raw_halfs = arg!(a, 3, convert, bool);
                let file_bit_depth = enum_param!(o, a, 4);
                let hue = enum_param!(o, a, 5);
                let interp = enum_param!(o, a, 6);
                let dir = enum_param!(o, a, 7);
                let mut t = py_result(py, Lut1DTransform::with_length(length, half))?;
                t.set_output_raw_halfs(raw_halfs);
                // The wheel stores a bit depth or a hue adjustment outside the enumerators,
                // which `validate` passes: the port refuses them (U-112) once it passed. An
                // interpolation outside them fails `validate` as INTERP_UNKNOWN does, "1D LUT
                // does not support interpolation algorithm: unknown." (`InterpolationToString`).
                let library_bit_depth = bit_depth_of(file_bit_depth);
                if let Some(b) = library_bit_depth {
                    t.set_file_output_bit_depth(b);
                }
                let library_hue = hue_adjust(hue);
                if let Some(h) = library_hue {
                    py_result(py, t.set_hue_adjust(h))?;
                }
                t.set_interpolation(interpolation(interp).unwrap_or(Interpolation::Unknown));
                let rc = TransformRc::new(Transform::Lut1D(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                if library_bit_depth.is_none() {
                    return Err(outside_enumerators(BIT_DEPTH, file_bit_depth));
                }
                if library_hue.is_none() {
                    return Err(outside_enumerators(LUT1D_HUE_ADJUST, hue));
                }
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getFileOutputBitDepth", ...), .def("setFileOutputBitDepth", ..., "bitDepth"_a)
// (PyLut1DTransform.cpp:54-57 @ v2.5.2).
static GET_FILE_OUTPUT_BIT_DEPTH: Function =
    method_def!("getFileOutputBitDepth", [SELF], BIT_DEPTH, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let bd = with(&rc, |t: &mut Lut1DTransform| t.file_output_bit_depth())?;
        enum_out(py, BIT_DEPTH, bd as i64)
    });

static SET_FILE_OUTPUT_BIT_DEPTH: Function = method_def!(
    "setFileOutputBitDepth",
    [SELF, Param::new("bitDepth", BIT_DEPTH)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let bd = bit_depth(enum_param!(o, a, 1))?;
        with(&rc, |t: &mut Lut1DTransform| {
            t.set_file_output_bit_depth(bd)
        })?;
        none(py)
    }
);

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyLut1DTransform.cpp:58-61 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &Lut1DTransform::equals, "other"_a) (PyLut1DTransform.cpp:62-63 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", LUT1D_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal = crate::py_transform::equals::<Lut1DTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getLength", ...), .def("setLength", ..., "length"_a) (PyLut1DTransform.cpp:64-67 @
// v2.5.2).
static GET_LENGTH: Function = method_def!("getLength", [SELF], "int", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut Lut1DTransform| t.length())?)
});

static SET_LENGTH: Function = method_def!(
    "setLength",
    [SELF, Param::new("length", INT)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let length = arg!(a, 1, convert, CULong);
        py_result(
            py,
            with(&rc, |t: &mut Lut1DTransform| t.set_length(length))?,
        )?;
        none(py)
    }
);

// .def("getValue", [](Lut1DTransformRcPtr & self, unsigned long index) { ...
// py::make_tuple(r, g, b); }, "index"_a) (PyLut1DTransform.cpp:68-75 @ v2.5.2).
static GET_VALUE: Function = method_def!(
    "getValue",
    [SELF, Param::new("index", INT)],
    "tuple",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let index = arg!(a, 1, convert, CULong);
        let [r, g, b] = py_result(py, with(&rc, |t: &mut Lut1DTransform| t.value(index))?)?;
        ok(py, (f64::from(r), f64::from(g), f64::from(b)))
    }
);

// .def("setValue", &Lut1DTransform::setValue, "index"_a, "r"_a, "g"_a, "b"_a)
// (PyLut1DTransform.cpp:76-77 @ v2.5.2).
static SET_VALUE: Function = method_def!(
    "setValue",
    [
        SELF,
        Param::new("index", INT),
        Param::new("r", FLOAT),
        Param::new("g", FLOAT),
        Param::new("b", FLOAT)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let index = arg!(a, 1, convert, CULong);
        let r = arg!(a, 2, convert, f32);
        let g = arg!(a, 3, convert, f32);
        let b = arg!(a, 4, convert, f32);
        py_result(
            py,
            with(&rc, |t: &mut Lut1DTransform| t.set_value(index, r, g, b))?,
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

// .def("setData", [](Lut1DTransformRcPtr & self, py::buffer & data) {...}, "data"_a)
// (PyLut1DTransform.cpp:78-96 @ v2.5.2): the buffer's type and size checked, then, with the GIL
// released, its length set to a third of its entries, and each entry set from three floats read
// one after another from its first item (its strides aside). A value past the buffer's memory is
// the port's error, where the wheel reads past it (U-148).
static SET_DATA: Function = method_def!(
    "setData",
    [SELF, Param::new("data", BUFFER)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let data = arg!(a, 1, convert, BufferArg).0;
        let buffer = ocio_py_ffi::Buffer::request(&data)?;
        check_buffer_type(py, buffer.info(), &dtype(py, "float32")?)?;
        check_buffer_divisible(py, buffer.info(), 3)?;
        let size_ssize = buffer_size(buffer.info());
        let size: CULong = if size_ssize >= 0 {
            size_ssize as CULong
        } else {
            0
        };
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
            with(&rc, |t: &mut Lut1DTransform| -> Result<(), SetDataError> {
                t.set_length(size / 3).map_err(SetDataError::Library)?;
                let mut i: CULong = 0;
                while i < size {
                    let (Some(r), Some(g), Some(b)) = (value(i), value(i + 1), value(i + 2)) else {
                        return Err(SetDataError::Outside);
                    };
                    t.set_value(i / 3, r, g, b).map_err(SetDataError::Library)?;
                    i += 3;
                }
                Ok(())
            })
        })?;
        match done? {
            Ok(()) => none(py),
            Err(SetDataError::Library(e)) => Err(crate::py_open_color_io::to_py_err(py, &e)),
            Err(SetDataError::Outside) => {
                Err(crate::py_image_desc::outside("Lut1DTransform.setData"))
            }
        }
    }
);

// .def("getData", [](Lut1DTransformRcPtr & self) {...}) (PyLut1DTransform.cpp:97-119 @
// v2.5.2): with the GIL released, each entry's three floats in order; then a new float32 array
// of them.
static GET_DATA: Function = method_def!("getData", [SELF], "numpy.ndarray", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let values = py.detach(|| {
        with(&rc, |t: &mut Lut1DTransform| -> ocio::Result<Vec<f32>> {
            let mut values = Vec::new();
            let mut i: CULong = 0;
            while i < t.length() {
                values.extend_from_slice(&t.value(i)?);
                i += 1;
            }
            Ok(values)
        })
    })?;
    let values = py_result(py, values)?;
    Ok(Some(
        PyArray1::<f32>::from_slice(py, &values).into_any().unbind(),
    ))
});

// .def("getInputHalfDomain", ...), .def("setInputHalfDomain", ..., "isHalfDomain"_a)
// (PyLut1DTransform.cpp:120-123 @ v2.5.2).
static GET_INPUT_HALF_DOMAIN: Function =
    method_def!("getInputHalfDomain", [SELF], "bool", |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(
            py,
            with(&rc, |t: &mut Lut1DTransform| t.input_half_domain())?,
        )
    });

static SET_INPUT_HALF_DOMAIN: Function = method_def!(
    "setInputHalfDomain",
    [SELF, Param::new("isHalfDomain", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let half = arg!(a, 1, convert, bool);
        with(&rc, |t: &mut Lut1DTransform| t.set_input_half_domain(half))?;
        none(py)
    }
);

// .def("getOutputRawHalfs", ...), .def("setOutputRawHalfs", ..., "isRawHalfs"_a)
// (PyLut1DTransform.cpp:124-127 @ v2.5.2).
static GET_OUTPUT_RAW_HALFS: Function =
    method_def!("getOutputRawHalfs", [SELF], "bool", |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(
            py,
            with(&rc, |t: &mut Lut1DTransform| t.output_raw_halfs())?,
        )
    });

static SET_OUTPUT_RAW_HALFS: Function = method_def!(
    "setOutputRawHalfs",
    [SELF, Param::new("isRawHalfs", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let raw = arg!(a, 1, convert, bool);
        with(&rc, |t: &mut Lut1DTransform| t.set_output_raw_halfs(raw))?;
        none(py)
    }
);

// .def("getHueAdjust", ...), .def("setHueAdjust", ..., "hueAdjust"_a)
// (PyLut1DTransform.cpp:128-131 @ v2.5.2).
static GET_HUE_ADJUST: Function =
    method_def!("getHueAdjust", [SELF], LUT1D_HUE_ADJUST, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let hue = with(&rc, |t: &mut Lut1DTransform| t.hue_adjust())?;
        enum_out(py, LUT1D_HUE_ADJUST, hue as i64)
    });

static SET_HUE_ADJUST: Function = method_def!(
    "setHueAdjust",
    [SELF, Param::new("hueAdjust", LUT1D_HUE_ADJUST)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let v = enum_param!(o, a, 1);
        let hue = hue_adjust(v).ok_or_else(|| outside_enumerators(LUT1D_HUE_ADJUST, v))?;
        py_result(
            py,
            with(&rc, |t: &mut Lut1DTransform| t.set_hue_adjust(hue))?,
        )?;
        none(py)
    }
);

// .def("getInterpolation", ...), .def("setInterpolation", ..., "interpolation"_a)
// (PyLut1DTransform.cpp:132-135 @ v2.5.2).
static GET_INTERPOLATION: Function =
    method_def!("getInterpolation", [SELF], INTERPOLATION, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let interp = with(&rc, |t: &mut Lut1DTransform| t.interpolation())?;
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
        with(&rc, |t: &mut Lut1DTransform| t.set_interpolation(interp))?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyLut1DTransform` (src/bindings/python/transforms/PyLut1DTransform.cpp:9-138
/// @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), LUT1D_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FILE_OUTPUT_BIT_DEPTH,
        &SET_FILE_OUTPUT_BIT_DEPTH,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_LENGTH,
        &SET_LENGTH,
        &GET_VALUE,
        &SET_VALUE,
        &SET_DATA,
        &GET_DATA,
        &GET_INPUT_HALF_DOMAIN,
        &SET_INPUT_HALF_DOMAIN,
        &GET_OUTPUT_RAW_HALFS,
        &SET_OUTPUT_RAW_HALFS,
        &GET_HUE_ADJUST,
        &SET_HUE_ADJUST,
        &GET_INTERPOLATION,
        &SET_INTERPOLATION,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
