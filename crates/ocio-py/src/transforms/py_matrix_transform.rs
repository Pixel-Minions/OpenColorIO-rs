// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyMatrixTransform` (src/bindings/python/transforms/PyMatrixTransform.cpp @ v2.5.2).

use ocio::Transform;
use ocio::transforms::matrix_transform::{MatrixAndOffset, MatrixTransform};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, TransformRc, class, construct, def, def_static, default, float_list,
    none, ok, set_direction, transform_out, validate, with,
};
use crate::{enum_param, float_array, float_array_out, method_def, q, ref_rc, static_def, this_rc};

const MATRIX_TRANSFORM: &str = q!("MatrixTransform");
const SELF: Param = Param::new("self", MATRIX_TRANSFORM);
const MATRIX_16: &str = float_array!("16");
const ARRAY_4: &str = float_array!("4");
const ARRAY_3: &str = float_array!("3");

const IDENTITY_16: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// `[0.0, 0.0, 0.0, 0.0]`.
fn zeros4(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    float_list(py, &[0.0; 4])
}

/// `[1.0, 1.0, 1.0, 1.0]`.
fn ones4(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    float_list(py, &[1.0; 4])
}

/// A new MatrixTransform of `m44` and `offset4`, validated, as each static function makes it
/// (`MatrixTransform::Create`, `setMatrix`, `setOffset`, `validate`).
fn create(py: Python<'_>, (m44, offset4): MatrixAndOffset) -> PyResult<TransformRc> {
    let mut t = MatrixTransform::new();
    t.set_matrix(&m44);
    t.set_offset(&offset4);
    let rc = TransformRc::new(Transform::Matrix(t));
    validate(py, &rc)?;
    Ok(rc)
}

// .def(py::init(&MatrixTransform::Create)), .def(py::init([](m44, offset4, dir) {...}),
// "matrix"_a = DEFAULT_MATRIX, "offset"_a = DEFAULT_OFFSET, "direction"_a =
// DEFAULT->getDirection()) (PyMatrixTransform.cpp:20-36 @ v2.5.2).
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
                let rc = TransformRc::new(Transform::Matrix(MatrixTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "matrix",
                    ty: MATRIX_16,
                    default: default(
                        "[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, \
                         0.0, 1.0]",
                        |py| float_list(py, &IDENTITY_16),
                    ),
                },
                Param {
                    name: "offset",
                    ty: ARRAY_4,
                    default: default("[0.0, 0.0, 0.0, 0.0]", zeros4),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let m44 = crate::arg!(a, 1, convert, [f64; 16]);
                let offset4 = crate::arg!(a, 2, convert, [f64; 4]);
                let dir = enum_param!(o, a, 3);
                let mut t = MatrixTransform::new();
                t.set_matrix(&m44);
                t.set_offset(&offset4);
                let rc = TransformRc::new(Transform::Matrix(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def_static("Fit", ..., "oldMin"_a = {0, 0, 0, 0}, "oldMax"_a = {1, 1, 1, 1}, "newMin"_a =
// {0, 0, 0, 0}, "newMax"_a = {1, 1, 1, 1}) (PyMatrixTransform.cpp:39-58 @ v2.5.2).
static FIT: Function = static_def!(
    "Fit",
    [
        Param {
            name: "oldMin",
            ty: ARRAY_4,
            default: default("[0.0, 0.0, 0.0, 0.0]", zeros4),
        },
        Param {
            name: "oldMax",
            ty: ARRAY_4,
            default: default("[1.0, 1.0, 1.0, 1.0]", ones4),
        },
        Param {
            name: "newMin",
            ty: ARRAY_4,
            default: default("[0.0, 0.0, 0.0, 0.0]", zeros4),
        },
        Param {
            name: "newMax",
            ty: ARRAY_4,
            default: default("[1.0, 1.0, 1.0, 1.0]", ones4),
        }
    ],
    MATRIX_TRANSFORM,
    |py, _, a, convert| {
        let old_min = crate::arg!(a, 0, convert, [f64; 4]);
        let old_max = crate::arg!(a, 1, convert, [f64; 4]);
        let new_min = crate::arg!(a, 2, convert, [f64; 4]);
        let new_max = crate::arg!(a, 3, convert, [f64; 4]);
        let fit = py_result(
            py,
            MatrixTransform::fit(&old_min, &old_max, &new_min, &new_max),
        )?;
        transform_out(py, Some(&create(py, fit)?))
    }
);

// .def_static("Identity", ...) (PyMatrixTransform.cpp:59-70 @ v2.5.2).
static IDENTITY: Function = static_def!("Identity", [], MATRIX_TRANSFORM, |py, _, _, _| {
    transform_out(py, Some(&create(py, MatrixTransform::identity())?))
});

// .def_static("Sat", ..., "sat"_a, "lumaCoef"_a) (PyMatrixTransform.cpp:71-83 @ v2.5.2).
static SAT: Function = static_def!(
    "Sat",
    [Param::new("sat", FLOAT), Param::new("lumaCoef", ARRAY_3)],
    MATRIX_TRANSFORM,
    |py, _, a, convert| {
        let sat = crate::arg!(a, 0, convert, f64);
        let luma = crate::arg!(a, 1, convert, [f64; 3]);
        transform_out(py, Some(&create(py, MatrixTransform::sat(sat, &luma))?))
    }
);

// .def_static("Scale", ..., "scale"_a) (PyMatrixTransform.cpp:84-96 @ v2.5.2).
static SCALE: Function = static_def!(
    "Scale",
    [Param::new("scale", ARRAY_4)],
    MATRIX_TRANSFORM,
    |py, _, a, convert| {
        let scale = crate::arg!(a, 0, convert, [f64; 4]);
        transform_out(py, Some(&create(py, MatrixTransform::scale(&scale))?))
    }
);

// .def_static("View", ..., "channelHot"_a, "lumaCoef"_a) (PyMatrixTransform.cpp:97-110 @
// v2.5.2).
static VIEW: Function = static_def!(
    "View",
    [
        Param::new(
            "channelHot",
            "typing.Annotated[collections.abc.Sequence[typing.SupportsInt], \"FixedSize(4)\"]"
        ),
        Param::new("lumaCoef", ARRAY_3)
    ],
    MATRIX_TRANSFORM,
    |py, _, a, convert| {
        let hot = crate::arg!(a, 0, convert, [i32; 4]);
        let luma = crate::arg!(a, 1, convert, [f64; 3]);
        transform_out(py, Some(&create(py, MatrixTransform::view(&hot, &luma))?))
    }
);

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyMatrixTransform.cpp:112-115 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &MatrixTransform::equals, "other"_a) (PyMatrixTransform.cpp:116-117 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", MATRIX_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal =
            crate::py_transform::equals::<MatrixTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getMatrix", ...) (PyMatrixTransform.cpp:118-124 @ v2.5.2).
static GET_MATRIX: Function = method_def!(
    "getMatrix",
    [SELF],
    float_array_out!("16"),
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(
            py,
            with(&rc, |t: &mut MatrixTransform| t.matrix())?.to_vec(),
        )
    }
);

// .def("setMatrix", ..., "matrix"_a) (PyMatrixTransform.cpp:125-130 @ v2.5.2).
static SET_MATRIX: Function = method_def!(
    "setMatrix",
    [SELF, Param::new("matrix", MATRIX_16)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let m44 = crate::arg!(a, 1, convert, [f64; 16]);
        with(&rc, |t: &mut MatrixTransform| t.set_matrix(&m44))?;
        none(py)
    }
);

// .def("getOffset", ...) (PyMatrixTransform.cpp:131-137 @ v2.5.2).
static GET_OFFSET: Function =
    method_def!("getOffset", [SELF], float_array_out!("4"), |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(
            py,
            with(&rc, |t: &mut MatrixTransform| t.offset())?.to_vec(),
        )
    });

// .def("setOffset", ..., "offset"_a) (PyMatrixTransform.cpp:138-143 @ v2.5.2).
static SET_OFFSET: Function = method_def!(
    "setOffset",
    [SELF, Param::new("offset", ARRAY_4)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let offset4 = crate::arg!(a, 1, convert, [f64; 4]);
        with(&rc, |t: &mut MatrixTransform| t.set_offset(&offset4))?;
        none(py)
    }
);

crate::file_bit_depths!(MatrixTransform, SELF);

/// Port of `bindPyMatrixTransform` (src/bindings/python/transforms/PyMatrixTransform.cpp:9-157
/// @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), MATRIX_TRANSFORM)?;
    def(&cls, &INIT)?;
    for f in [&FIT, &IDENTITY, &SAT, &SCALE, &VIEW] {
        def_static(&cls, f)?;
    }
    for f in [
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_MATRIX,
        &SET_MATRIX,
        &GET_OFFSET,
        &SET_OFFSET,
        &GET_FILE_INPUT_BIT_DEPTH,
        &SET_FILE_INPUT_BIT_DEPTH,
        &GET_FILE_OUTPUT_BIT_DEPTH,
        &SET_FILE_OUTPUT_BIT_DEPTH,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}

static REPR: Function = crate::repr_def!(SELF);
