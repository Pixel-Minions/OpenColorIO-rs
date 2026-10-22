// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio-py-ffi`, the Python binding's only `unsafe` code, is sound whatever its safe
//! functions are given: what its C calls rely on is checked first, and refused with a
//! TypeError (the crate is one pinned file, so its tests live here). No wheel is involved:
//! these are the port's own safety checks.
//! - `from_metaclass`: the metatype must derive from `type` (`int` doesn't); a base must derive
//!   from `T`, by its MRO as CPython keeps it (a metatype's `__mro__` attribute can lie); no
//!   base may be larger than the instance size.
//! - `new_function`: only an instance method of `T` (a class method's C function takes its
//!   receiver as a type, unchecked).
#![cfg(feature = "python")]

mod common;

use ocio_py::pybind11::Pybind11Object;
use pyo3::prelude::*;
use pyo3::types::{PyTuple, PyType};

/// A class with an instance method and a class method, for `new_function`.
#[pyclass(frozen)]
struct Probe;

#[pymethods]
impl Probe {
    fn instance(&self) -> u32 {
        1
    }

    #[classmethod]
    fn class(_cls: &Bound<'_, PyType>) -> u32 {
        2
    }
}

/// The text of `result`'s TypeError.
fn refusal<T: std::fmt::Debug>(py: Python<'_>, result: PyResult<T>) -> String {
    let error = result.expect_err("refused");
    assert!(
        error.is_instance_of::<pyo3::exceptions::PyTypeError>(py),
        "{error}"
    );
    error.value(py).to_string()
}

/// `from_metaclass` of a class `probe.X` deriving from `bases`, `size` bytes.
fn make<'py>(
    metatype: &Bound<'py, PyType>,
    bases: Vec<Bound<'py, PyType>>,
    size: usize,
) -> PyResult<Bound<'py, PyType>> {
    let bases = PyTuple::new(metatype.py(), bases)?;
    ocio_py_ffi::from_metaclass::<Pybind11Object>(metatype, "probe.X", &bases, size)
}

#[test]
fn from_metaclass_refuses_what_its_call_cant_take() {
    common::start_python();
    Python::attach(|py| {
        let object = py.get_type::<Pybind11Object>();
        let builtins = py.import("builtins").unwrap();
        let int = builtins
            .getattr("int")
            .unwrap()
            .cast_into::<PyType>()
            .unwrap();
        let type_ = py.get_type::<PyType>();
        // Made as the module makes its classes: its own `__init__`, pybind11_object's slot.
        let made = make(&type_, vec![object.clone()], 56).unwrap();
        assert_eq!(made.getattr("__name__").unwrap().to_string(), "X");
        let init = made
            .getattr("__dict__")
            .unwrap()
            .get_item("__init__")
            .unwrap();
        assert_eq!(
            init.repr().unwrap().to_string(),
            "<slot wrapper '__init__' of 'probe.X' objects>"
        );
        assert_eq!(
            refusal(py, make(&int, vec![object.clone()], 56)),
            "probe.X: the metatype isn't a subclass of type"
        );
        assert_eq!(
            refusal(py, make(&type_, vec![], 56)),
            "probe.X: no base derives from pybind11_object"
        );
        let other = builtins
            .getattr("object")
            .unwrap()
            .cast_into::<PyType>()
            .unwrap();
        assert_eq!(
            refusal(py, make(&type_, vec![other], 56)),
            "probe.X: no base derives from pybind11_object"
        );
        assert_eq!(
            refusal(py, make(&type_, vec![object.clone()], 48)),
            "probe.X: a base's instances (56 bytes) are larger than 48 bytes"
        );
        // A metatype whose `__mro__` claims pybind11_object: CPython's MRO of the class
        // doesn't hold it.
        let globals = pyo3::types::PyDict::new(py);
        globals.set_item("O", &object).unwrap();
        py.run(
            c"class M(type):\n    __mro__ = property(lambda c: (c, O, object))\n\
              class L(metaclass=M):\n    pass\n\
              lies = L.__mro__\n",
            Some(&globals),
            None,
        )
        .unwrap();
        let lies: Bound<'_, PyTuple> = globals
            .get_item("lies")
            .unwrap()
            .unwrap()
            .cast_into()
            .unwrap();
        assert!(lies.get_item(1).unwrap().is(&object));
        let liar = globals
            .get_item("L")
            .unwrap()
            .unwrap()
            .cast_into::<PyType>()
            .unwrap();
        assert_eq!(
            refusal(py, make(&type_, vec![liar], 56)),
            "probe.X: no base derives from pybind11_object"
        );
    });
}

#[test]
fn new_function_takes_an_instance_method_only() {
    common::start_python();
    Python::attach(|py| {
        let probe = Bound::new(py, Probe).unwrap();
        let none = py.None().into_bound(py);
        let f = ocio_py_ffi::new_function(&probe, "instance", "named", Some("doc"), &none).unwrap();
        assert_eq!(f.getattr("__name__").unwrap().to_string(), "named");
        assert_eq!(f.getattr("__doc__").unwrap().to_string(), "doc");
        assert!(f.getattr("__self__").unwrap().is(&probe));
        assert_eq!(f.call0().unwrap().extract::<u32>().unwrap(), 1);
        let undocumented =
            ocio_py_ffi::new_function(&probe, "instance", "other", None, &none).unwrap();
        assert!(undocumented.getattr("__doc__").unwrap().is_none());
        for method in ["class", "nope", ""] {
            assert_eq!(
                refusal(
                    py,
                    ocio_py_ffi::new_function(&probe, method, "x", None, &none)
                ),
                format!("Probe has no instance method {method}")
            );
        }
        // A name with a NUL is no C string.
        let error = ocio_py_ffi::new_function(&probe, "instance", "a\0b", None, &none)
            .expect_err("refused");
        assert!(error.is_instance_of::<pyo3::exceptions::PyValueError>(py));
    });
}
