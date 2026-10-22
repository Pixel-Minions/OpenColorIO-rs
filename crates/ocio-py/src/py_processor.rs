// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyProcessor` (src/bindings/python/PyProcessor.cpp @ v2.5.2): so far the holder of the
//! class `Processor`, which `Config.getProcessor` returns (chunk 6.3g); its methods come with
//! chunk 6.3h.
//!
//! pybind11 holds a processor by its `ConstProcessorRcPtr`, the pointer the library shares
//! (a config's processor cache gives the same one again): the Python object is the one made
//! for that processor while it lives.

use std::sync::Arc;

use ocio::Processor;
use pyo3::prelude::*;

use crate::q;
use crate::shared::Keyed;

pub(crate) const PROCESSOR: &str = q!("Processor");

/// A processor as a `ConstProcessorRcPtr` holds it.
#[derive(Debug, Clone)]
pub(crate) struct ProcessorRc(pub(crate) Arc<Processor>);

impl Keyed for ProcessorRc {
    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as *const u8 as usize
    }
}

/// A returned processor: the Python object made for it while it lives, else a new one.
pub(crate) fn processor_out(py: Python<'_>, p: Arc<Processor>) -> PyResult<Option<Py<PyAny>>> {
    crate::shared::out(py, PROCESSOR, &ProcessorRc(p))
}
