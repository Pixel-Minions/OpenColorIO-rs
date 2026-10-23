// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `PyDynamicProperty` (src/bindings/python/PyDynamicProperty.h @ v2.5.2): so far the value an
//! instance of `DynamicProperty` holds, which `CPUProcessor.getDynamicProperty` returns (chunk
//! 6.5b); the class's methods (`bindPyDynamicProperty`) come with chunk 6.7a.

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use pyo3::prelude::*;

use crate::py_transform::{Out, class};
use crate::{pybind11, q};

pub(crate) const DYNAMIC_PROPERTY: &str = q!("DynamicProperty");

/// The value of a `DynamicProperty` instance: `PyDynamicProperty`'s `m_prop`, the property it
/// shares with its processor.
#[pyclass(frozen, module = "pybind11_builtins", name = "_dynamic_property")]
#[derive(Debug)]
// Its methods (chunk 6.7a) read the property; nothing does before.
#[allow(dead_code)]
pub(crate) struct DynamicPropertyHolder(pub(crate) DynamicPropertyRcPtr);

/// A returned `PyDynamicProperty` (a value, which pybind11 moves into a new instance).
pub(crate) fn dynamic_property_out(py: Python<'_>, prop: DynamicPropertyRcPtr) -> Out {
    let cls = class(py, DYNAMIC_PROPERTY)?;
    let obj = cls.call_method1("__new__", (&cls,))?;
    pybind11::set_value(
        &obj,
        DYNAMIC_PROPERTY,
        Bound::new(py, DynamicPropertyHolder(prop))?.into_any(),
    )?;
    Ok(Some(obj.unbind()))
}
