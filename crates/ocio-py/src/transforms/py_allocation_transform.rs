// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyAllocationTransform` (src/bindings/python/transforms/PyAllocationTransform.cpp @
//! v2.5.2).

use ocio::{AllocationTransform, Transform};
use ocio_ops::open_color_types::Allocation;
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_transform::{
    DIRECTION_DEFAULT, TransformRc, class, construct, def, default, enum_out, none, ok,
    outside_enumerators, set_direction, validate, with,
};
use crate::{enum_param, method_def, q, this_rc};

const ALLOCATION_TRANSFORM: &str = q!("AllocationTransform");
const ALLOCATION: &str = q!("Allocation");
const SELF: Param = Param::new("self", ALLOCATION_TRANSFORM);
const VARS: &str = "collections.abc.Sequence[typing.SupportsFloat]";

/// The library's `Allocation` of the C++ value `v`.
fn allocation(v: i64) -> PyResult<Allocation> {
    [Allocation::Unknown, Allocation::Uniform, Allocation::Lg2]
        .into_iter()
        .find(|a| *a as i64 == v)
        .ok_or_else(|| outside_enumerators(ALLOCATION, v))
}

/// Port of the binding's `setVars` (PyAllocationTransform.cpp:22-29 @ v2.5.2): 2 or 3 values.
fn set_vars(py: Python<'_>, rc: &TransformRc, vars: &[f32]) -> PyResult<()> {
    if vars.len() < 2 || vars.len() > 3 {
        return Err(crate::py_open_color_io::to_py_err(
            py,
            &ocio::Exception::new("vars must be a float array, size 2 or 3"),
        ));
    }
    with(rc, |t: &mut AllocationTransform| t.set_vars(vars))
}

// .def(py::init(&AllocationTransform::Create)), .def(py::init([](allocation, vars, dir)
// {...}), "allocation"_a = DEFAULT->getAllocation(), "vars"_a = getVarsStdVec(DEFAULT),
// "direction"_a = DEFAULT->getDirection()) (PyAllocationTransform.cpp:39-56 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::Allocation(AllocationTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "allocation",
                    ty: ALLOCATION,
                    default: default("<Allocation.ALLOCATION_UNIFORM: 1>", |py| {
                        crate::enums::to_py(&crate::enums::enum_class(py, ALLOCATION)?, 1)
                    }),
                },
                Param {
                    name: "vars",
                    ty: VARS,
                    default: default("[]", |py| Ok(pyo3::types::PyList::empty(py).into_any())),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let alloc = enum_param!(o, a, 1);
                let vars = crate::arg!(a, 2, convert, Vec<f32>);
                let dir = enum_param!(o, a, 3);
                let rc = TransformRc::new(Transform::Allocation(AllocationTransform::new()));
                // The wheel stores an allocation outside the enumerators, which `validate` then
                // refuses as it refuses ALLOCATION_UNKNOWN (AllocationTransform.cpp:98-115 @
                // v2.5.2): the port validates `Unknown` in its place, so the wheel's errors come
                // in the wheel's order, and the value is never stored.
                let library_alloc = allocation(alloc);
                let stored = *library_alloc.as_ref().unwrap_or(&Allocation::Unknown);
                with(&rc, |t: &mut AllocationTransform| t.set_allocation(stored))?;
                if !vars.is_empty() {
                    set_vars(py, &rc, &vars)?;
                }
                set_direction(&rc, dir);
                validate(py, &rc)?;
                library_alloc?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getAllocation", &AllocationTransform::getAllocation) (PyAllocationTransform.cpp:58-59
// @ v2.5.2).
static GET_ALLOCATION: Function =
    method_def!("getAllocation", [SELF], ALLOCATION, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let alloc = with(&rc, |t: &mut AllocationTransform| t.allocation())?;
        enum_out(py, ALLOCATION, alloc as i64)
    });

// .def("setAllocation", &AllocationTransform::setAllocation, "allocation"_a)
// (PyAllocationTransform.cpp:60-62 @ v2.5.2).
static SET_ALLOCATION: Function = method_def!(
    "setAllocation",
    [SELF, Param::new("allocation", ALLOCATION)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let alloc = allocation(enum_param!(o, a, 1))?;
        with(&rc, |t: &mut AllocationTransform| t.set_allocation(alloc))?;
        none(py)
    }
);

// .def("getVars", [](self) { return getVarsStdVec(self); }) (PyAllocationTransform.cpp:63-67
// @ v2.5.2).
static GET_VARS: Function = method_def!("getVars", [SELF], "list[float]", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let vars: Vec<f64> = with(&rc, |t: &mut AllocationTransform| {
        t.vars().iter().map(|&v| f64::from(v)).collect()
    })?;
    ok(py, vars)
});

// .def("setVars", [](self, vars) { setVars(self, vars); }, "vars"_a)
// (PyAllocationTransform.cpp:68-73 @ v2.5.2).
static SET_VARS: Function = method_def!(
    "setVars",
    [SELF, Param::new("vars", VARS)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let vars = crate::arg!(a, 1, convert, Vec<f32>);
        set_vars(py, &rc, &vars)?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyAllocationTransform` (src/bindings/python/transforms/
/// PyAllocationTransform.cpp:33-77 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), ALLOCATION_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_ALLOCATION,
        &SET_ALLOCATION,
        &GET_VARS,
        &SET_VARS,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
