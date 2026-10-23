// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Processors for step scripts: `Config.CreateRaw().getProcessor(t).getDefaultCPUProcessor()`.
//! Until the port binds those (cards p6-config and p6-processors), the port's interpreter gets
//! stand-ins for them, as `cpu_processor_oracle.rs`'s: the same Rust API (the library's raw
//! config, processor and default CPU processor of the transform), handed to Python through the
//! embedding API. The steps that make the config and the processor aren't compared, only the
//! CPU processor they give.
#![allow(dead_code)]

use std::sync::Once;

use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyDict, PyTuple};

use super::script::{Script, n, r, ra};

/// The Rust side of the stand-in: `_make(transform)`, the default CPU processor
/// `Config::create_raw()` makes of `transform`, as a Python `CPUProcessor`.
pub(crate) fn make_cpu(args: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
    let py = args.py();
    let error = |e: ocio::Exception| pyo3::exceptions::PyRuntimeError::new_err(format!("{e:?}"));
    let transform = ocio_py::embed::transform_from_python(&args.get_item(0)?)?;
    let config = ocio::Config::create_raw().map_err(error)?;
    let processor = config.processor(&transform).map_err(error)?;
    let cpu = processor.default_cpu_processor().map_err(error)?;
    ocio_py::embed::cpu_processor_to_python(py, &cpu)
}

/// The stand-ins' Python side, installed while the module lacks
/// `Processor.getDefaultCPUProcessor`.
const STAND_IN: &str = r#"
import PyOpenColorIO as OCIO

class _Processor:
    def __init__(self, transform):
        self.transform = transform

    def getDefaultCPUProcessor(self):
        return _make(self.transform)

class Config:
    @staticmethod
    def CreateRaw():
        return Config()

    def getProcessor(self, transform):
        return _Processor(transform)

if not hasattr(getattr(OCIO, "Processor", None), "getDefaultCPUProcessor"):
    OCIO.Config = Config
"#;

static STAND_IN_ONCE: Once = Once::new();

/// Starts the interpreter with the stand-ins, if the module needs them.
pub(crate) fn start() {
    super::start_python();
    STAND_IN_ONCE.call_once(|| {
        Python::attach(|py| {
            let make = PyCFunction::new_closure(py, Some(c"_make"), None, |args, _| make_cpu(args))
                .unwrap();
            let globals = PyDict::new(py);
            globals.set_item("_make", make).unwrap();
            let code = std::ffi::CString::new(STAND_IN).unwrap();
            py.run(&code, Some(&globals), None)
                .unwrap_or_else(|e| panic!("{e}"));
        });
    });
}

/// The default CPU processor of the transform of step `t`.
pub(crate) fn cpu(sc: &mut Script, t: usize) -> usize {
    let config = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.uncompare(config);
    let processor = sc.call(ra(config, "getProcessor"), vec![r(t)]);
    sc.uncompare(processor);
    sc.call(ra(processor, "getDefaultCPUProcessor"), vec![])
}
