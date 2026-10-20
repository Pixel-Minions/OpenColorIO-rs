// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyTypes` (src/bindings/python/PyTypes.cpp @ v2.5.2): the module's enums, string
//! constants and string-conversion functions. Bound so far (chunk 6.2a, its test vehicle):
//! `TransformDirection`; the others come with chunk 6.2b.

use pyo3::prelude::*;

use crate::enums::add_enum;

// py::enum_<TransformDirection>(m, "TransformDirection", ...) with its values and
// export_values() (PyTypes.cpp:384-392 @ v2.5.2).
pub(crate) fn bind_py_types(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_enum(
        m.as_any(),
        None,
        "TransformDirection",
        &[("TRANSFORM_DIR_FORWARD", 0), ("TRANSFORM_DIR_INVERSE", 1)],
    )?;
    Ok(())
}
