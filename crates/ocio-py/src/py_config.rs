// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyConfig` (src/bindings/python/PyConfig.cpp @ v2.5.2): the class `Config`, its iterator
//! classes, and the module functions that end the binding (chunks 6.3f and 6.3g). So far: the
//! module functions `ExtractOCIOZArchive` and `ResolveConfigPath`.

use ocio::builtinconfigs::builtin_config_registry::resolve_config_path;
use pyo3::prelude::*;

use crate::arg;
use crate::dispatch::{CharPtr, Function, Overload, Param, std_string_to_py};
use crate::py_open_color_io::{function, py_result};

/// The port's error for a null path given to `ExtractOCIOZArchive`: the wheel makes a
/// `std::string` of a null destination and opens a null archive path (its process dies;
/// docs/improvements.md, U-131).
fn null_path() -> PyErr {
    pyo3::exceptions::PyValueError::new_err(
        "ExtractOCIOZArchive: None, a null pointer the wheel reads as a path (its process dies; \
         docs/improvements.md, U-131)",
    )
}

// m.def("ExtractOCIOZArchive", &ExtractOCIOZArchive) (PyConfig.cpp:1486-1487 @ v2.5.2): two
// `const char *` parameters without names.
static EXTRACT_OCIOZ_ARCHIVE: Function = function(
    "ExtractOCIOZArchive",
    &[Overload {
        params: &[Param::new("arg0", "str"), Param::new("arg1", "str")],
        ret: "None",
        call: |py, _, a, convert| {
            let CharPtr(archive_path) = arg!(a, 0, convert, CharPtr);
            let CharPtr(destination) = arg!(a, 1, convert, CharPtr);
            let (Some(archive_path), Some(destination)) = (archive_path, destination) else {
                return Err(null_path());
            };
            // The GIL stays held, as pybind11's binding holds it.
            py_result(py, ocio::extract_ocioz_archive(archive_path, destination))?;
            Ok(Some(py.None()))
        },
        pos_only: 0,
    }],
);

// m.def("ResolveConfigPath", &ResolveConfigPath) (PyConfig.cpp:1489-1490 @ v2.5.2): a
// `const char *` parameter without a name; a null pointer is the empty string.
static RESOLVE_CONFIG_PATH: Function = function(
    "ResolveConfigPath",
    &[Overload {
        params: &[Param::new("arg0", "str")],
        ret: "str",
        call: |py, _, a, convert| {
            let CharPtr(path) = arg!(a, 0, convert, CharPtr);
            let path = path.unwrap_or_default();
            let resolved = resolve_config_path(&path);
            Ok(Some(std_string_to_py(py, resolved)?.into_any().unbind()))
        },
        pos_only: 0,
    }],
);

/// Port of `bindPyConfig` (src/bindings/python/PyConfig.cpp:83-1491 @ v2.5.2), so far its
/// module functions `ExtractOCIOZArchive` and `ResolveConfigPath` (1486-1490).
pub(crate) fn bind_py_config(m: &Bound<'_, PyModule>) -> PyResult<()> {
    crate::pybind11::def_function(m, &EXTRACT_OCIOZ_ARCHIVE)?;
    crate::pybind11::def_function(m, &RESOLVE_CONFIG_PATH)?;
    Ok(())
}
