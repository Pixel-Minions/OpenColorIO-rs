// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGPUProcessor` (src/bindings/python/PyGPUProcessor.cpp @ v2.5.2): the class
//! `GPUProcessor` (chunk 6.3h).
//!
//! pybind11 holds a GPU processor by its `GPUProcessorRcPtr`, the pointer the library shares (a
//! processor's cache gives the same one again): the Python object is the one made for that
//! processor while it lives ([`gpu_processor_out`]).

use std::sync::Arc;

use ocio_gpu::gpu_processor::GpuProcessor;
use pyo3::prelude::*;

use crate::dispatch::{Function, Param};
use crate::py_gpu_shader_desc::GPU_SHADER_DESC;
use crate::py_open_color_io::py_result;
use crate::py_transform::{class, def, none, ok, string_out};
use crate::shared::{Keyed, holder_with};
use crate::{method_def, q};

pub(crate) const GPU_PROCESSOR: &str = q!("GPUProcessor");
const SELF: Param = Param::new("self", GPU_PROCESSOR);

/// A GPU processor as a `GPUProcessorRcPtr` holds it.
#[derive(Debug, Clone)]
pub(crate) struct GpuProcessorRc(pub(crate) Arc<GpuProcessor>);

impl Keyed for GpuProcessorRc {
    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as *const u8 as usize
    }
}

/// A returned GPU processor: the Python object made for it while it lives, else a new one.
pub(crate) fn gpu_processor_out(
    py: Python<'_>,
    p: Arc<GpuProcessor>,
) -> PyResult<Option<Py<PyAny>>> {
    crate::shared::out(py, GPU_PROCESSOR, &GpuProcessorRc(p))
}

/// The GPU processor `obj` holds (`Ok(None)`: not one).
pub(crate) fn gpu_processor_of(obj: &Bound<'_, PyAny>) -> PyResult<Option<Arc<GpuProcessor>>> {
    Ok(holder_with::<GpuProcessorRc>(obj, GPU_PROCESSOR)?.map(|rc| rc.0))
}

/// The GPU processor of `self`, or the next overload.
macro_rules! this_gpu {
    ($a:expr) => {
        match gpu_processor_of(&$a[0])? {
            Some(p) => p,
            None => return Ok(None),
        }
    };
}

// .def("isNoOp", &GPUProcessor::isNoOp) (PyGPUProcessor.cpp:14-15 @ v2.5.2).
static IS_NO_OP: Function = method_def!("isNoOp", [SELF], "bool", |py, _, a, _| {
    let p = this_gpu!(a);
    ok(py, p.is_no_op())
});

// .def("hasChannelCrosstalk", &GPUProcessor::hasChannelCrosstalk) (PyGPUProcessor.cpp:16-17 @
// v2.5.2).
static HAS_CHANNEL_CROSSTALK: Function =
    method_def!("hasChannelCrosstalk", [SELF], "bool", |py, _, a, _| {
        let p = this_gpu!(a);
        ok(py, p.has_channel_crosstalk())
    });

// .def("getCacheID", &GPUProcessor::getCacheID) (PyGPUProcessor.cpp:18-19 @ v2.5.2).
static GET_CACHE_ID: Function = method_def!("getCacheID", [SELF], "str", |py, _, a, _| {
    let p = this_gpu!(a);
    string_out(py, p.get_cache_id())
});

/// The port's error for `None` as the description: pybind11's holder caster passes an empty
/// `GpuShaderDescRcPtr`, through which `extractGpuShaderInfo` calls (docs/improvements.md,
/// U-195).
fn null_desc() -> PyErr {
    pyo3::exceptions::PyValueError::new_err(
        "GPUProcessor.extractGpuShaderInfo(shaderDesc): None, a null GpuShaderDesc, through \
         which the wheel calls (docs/improvements.md, U-195)",
    )
}

// .def("extractGpuShaderInfo", (void (GPUProcessor::*)(GpuShaderDescRcPtr &) const)
// &GPUProcessor::extractGpuShaderInfo, "shaderDesc"_a) (PyGPUProcessor.cpp:20-24 @ v2.5.2): a
// holder argument, which takes `None` (the dispatcher's conversions) as a null pointer.
static EXTRACT_GPU_SHADER_INFO: Function = method_def!(
    "extractGpuShaderInfo",
    [SELF, Param::new("shaderDesc", GPU_SHADER_DESC)],
    "None",
    |py, _, a, convert| {
        let p = this_gpu!(a);
        let Some(desc) =
            crate::shared::holder_arg::<ocio_gpu::GpuShaderDesc>(&a[1], GPU_SHADER_DESC, convert)?
        else {
            return Ok(None);
        };
        let desc = desc.ok_or_else(null_desc)?;
        let done = crate::py_gpu_shader_creator::with_desc_unlocked(&desc, |desc| {
            p.extract_gpu_shader_info(desc)
        });
        py_result(py, done)?;
        none(py)
    }
);

/// Port of `bindPyGPUProcessor` (src/bindings/python/PyGPUProcessor.cpp:9-25 @ v2.5.2), in its
/// order.
pub(crate) fn bind_py_gpu_processor(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), GPU_PROCESSOR)?;
    for f in [
        &IS_NO_OP,
        &HAS_CHANNEL_CROSSTALK,
        &GET_CACHE_ID,
        &EXTRACT_GPU_SHADER_INFO,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
