// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyProcessor` (src/bindings/python/PyProcessor.cpp @ v2.5.2): the class `Processor`,
//! which `Config.getProcessor` returns (chunk 6.3g), its methods and its iterator class
//! `TransformFormatMetadataIterator` (chunk 6.3h).
//!
//! pybind11 holds a processor by its `ConstProcessorRcPtr`, the pointer the library shares
//! (a config's processor cache gives the same one again): the Python object is the one made
//! for that processor while it lives.
//!
//! **Metadata.** `getProcessorMetadata` returns the processor's `ConstProcessorMetadataRcPtr`,
//! which the processors made from it share (`Processor::Impl::operator=` copies the pointer),
//! and which Python can change (pybind11 drops the `const`): the binding keeps one object per
//! metadata the library shares, for as long as the library's metadata lives
//! ([`processor_metadata`]), so a change shows through every processor that shares it, as in
//! the wheel; the library itself never reads it back. (A Rust host reading the library's
//! metadata doesn't see Python's changes: the binding's object is a copy.)
//! `getFormatMetadata` and the iterator's items are references into the processor's metadata
//! and its ops' ([`crate::py_format_metadata`]): read as the wheel's, never changed (D-13).

use std::sync::{Arc, Mutex, Weak};

use ocio::{Processor, ProcessorMetadata, Transform};
use ocio_ops::open_color_types::OptimizationFlags;
use pyo3::prelude::*;

use crate::dispatch::{CULong, Function, Overload, Param};
use crate::py_cpu_processor::{CPU_PROCESSOR, cpu_processor_out, dynamic_property_type};
use crate::py_dynamic_property::{DYNAMIC_PROPERTY, dynamic_property_out};
use crate::py_gpu_processor::{GPU_PROCESSOR, gpu_processor_out};
use crate::py_gpu_shader_creator::DYNAMIC_PROPERTY_TYPE;
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_processor_metadata::PROCESSOR_METADATA;
use crate::py_transform::{
    BIT_DEPTH, FORMAT_METADATA, INT, TransformRc, bit_depth, class, def, nested_class, ok,
    string_out, transform_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Keyed, Shared, holder_with};
use crate::{arg, enum_param, method_def, q, this_iter};

pub(crate) const PROCESSOR: &str = q!("Processor");
const TRANSFORM_FORMAT_METADATA_ITERATOR: &str = q!("Processor.TransformFormatMetadataIterator");
const OPTIMIZATION_FLAGS: &str = q!("OptimizationFlags");
const GROUP_TRANSFORM: &str = q!("GroupTransform");
const SELF: Param = Param::new("self", PROCESSOR);
const IT_SELF: Param = Param::new("self", TRANSFORM_FORMAT_METADATA_ITERATOR);

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

/// The processor of `self`, or the next overload.
macro_rules! this_proc {
    ($a:expr) => {
        match holder_with::<ProcessorRc>(&$a[0], PROCESSOR)? {
            Some(rc) => rc.0,
            None => return Ok(None),
        }
    };
}

/// The library's bit depth of the C++ value `v` given to an optimized processor: a value
/// inside the enum's values (no fixed type, enumerators 0 to 8: 0 to 15) that is no
/// enumerator is D-15's error, one outside them U-112's.
fn processor_bit_depth(v: i64) -> PyResult<ocio_ops::open_color_types::BitDepth> {
    match v {
        9..=15 => Err(crate::py_transform::not_an_enumerator(BIT_DEPTH, v)),
        _ => bit_depth(v),
    }
}

/// The flags of the C++ value `v` (`OptimizationFlags : unsigned long`).
fn flags(v: i64) -> OptimizationFlags {
    OptimizationFlags(v as CULong)
}

// ---------------------------------------------------------------------------------------------
// ProcessorMetadata, shared

/// A metadata the library shares (weakly: while the entry is listed, its address is never
/// another metadata's) and its Python object's value.
struct SharedMetadata {
    metadata: Weak<ProcessorMetadata>,
    rc: Shared<ProcessorMetadata>,
}

static METADATA: Mutex<Vec<SharedMetadata>> = Mutex::new(Vec::new());

/// The value of the `ProcessorMetadata` the processor `p` shares: the one made for its metadata
/// while the library's metadata lives (some processor holds it, whichever called this), else a
/// new one of the metadata. A Python object of it that outlives the library's keeps its value.
fn processor_metadata(p: &Arc<Processor>) -> Shared<ProcessorMetadata> {
    let md = p.processor_metadata_rc();
    let weak = Arc::downgrade(md);
    let mut list = METADATA
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    list.retain(|e| e.metadata.strong_count() > 0);
    if let Some(e) = list.iter().find(|e| e.metadata.ptr_eq(&weak)) {
        return e.rc.clone();
    }
    let rc = Shared::new(ProcessorMetadata::clone(md));
    list.push(SharedMetadata {
        metadata: weak,
        rc: rc.clone(),
    });
    rc
}

// ---------------------------------------------------------------------------------------------
// The class

// .def("isNoOp", &Processor::isNoOp) (PyProcessor.cpp:34-35 @ v2.5.2).
static IS_NO_OP: Function = method_def!("isNoOp", [SELF], "bool", |py, _, a, _| {
    let p = this_proc!(a);
    ok(py, py_result(py, p.is_no_op())?)
});

// .def("hasChannelCrosstalk", &Processor::hasChannelCrosstalk) (PyProcessor.cpp:36-37 @
// v2.5.2).
static HAS_CHANNEL_CROSSTALK: Function =
    method_def!("hasChannelCrosstalk", [SELF], "bool", |py, _, a, _| {
        let p = this_proc!(a);
        ok(py, p.has_channel_crosstalk())
    });

// .def("getCacheID", &Processor::getCacheID) (PyProcessor.cpp:38-39 @ v2.5.2).
static GET_CACHE_ID: Function = method_def!("getCacheID", [SELF], "str", |py, _, a, _| {
    let p = this_proc!(a);
    let id = py_result(py, p.cache_id())?;
    string_out(py, id.as_bytes())
});

// .def("getProcessorMetadata", &Processor::getProcessorMetadata) (PyProcessor.cpp:40-41 @
// v2.5.2): the shared `ConstProcessorMetadataRcPtr`.
static GET_PROCESSOR_METADATA: Function = method_def!(
    "getProcessorMetadata",
    [SELF],
    PROCESSOR_METADATA,
    |py, _, a, _| {
        let p = this_proc!(a);
        crate::shared::out(py, PROCESSOR_METADATA, &processor_metadata(&p))
    }
);

// .def("getFormatMetadata", &Processor::getFormatMetadata, py::return_value_policy::
// reference_internal) (PyProcessor.cpp:42-44 @ v2.5.2).
static GET_FORMAT_METADATA: Function = method_def!(
    "getFormatMetadata",
    [SELF],
    FORMAT_METADATA,
    |py, _, a, _| {
        let p = this_proc!(a);
        crate::py_format_metadata::processor_metadata_out(py, &a[0], &p, None)
    }
);

// .def("getTransformFormatMetadata", [](ProcessorRcPtr & self) { return
// TransformFormatMetadataIterator(self); }) (PyProcessor.cpp:45-48 @ v2.5.2).
static GET_TRANSFORM_FORMAT_METADATA: Function = method_def!(
    "getTransformFormatMetadata",
    [SELF],
    TRANSFORM_FORMAT_METADATA_ITERATOR,
    |py, _, a, _| {
        let p = this_proc!(a);
        Ok(Some(
            new_iterator(
                py,
                TRANSFORM_FORMAT_METADATA_ITERATOR,
                Some(Box::new(ProcessorRc(p))),
            )?
            .unbind(),
        ))
    }
);

// .def("createGroupTransform", &Processor::createGroupTransform) (PyProcessor.cpp:49-50 @
// v2.5.2): a new group.
static CREATE_GROUP_TRANSFORM: Function = method_def!(
    "createGroupTransform",
    [SELF],
    GROUP_TRANSFORM,
    |py, _, a, _| {
        let p = this_proc!(a);
        let group = py_result(py, p.create_group_transform())?;
        transform_out(py, Some(&TransformRc::new(Transform::Group(group))))
    }
);

// .def("getDynamicProperty", [](ProcessorRcPtr & self, DynamicPropertyType type) { return
// PyDynamicProperty(self->getDynamicProperty(type)); }, "type"_a) (PyProcessor.cpp:51-56 @
// v2.5.2): a type outside the enumerators is no op's, upstream's error
// (`OpRcPtrVec::getDynamicProperty`, Op.cpp:315-326).
static GET_DYNAMIC_PROPERTY: Function = method_def!(
    "getDynamicProperty",
    [SELF, Param::new("type", DYNAMIC_PROPERTY_TYPE)],
    DYNAMIC_PROPERTY,
    |py, o, a, _| {
        let p = this_proc!(a);
        let v = enum_param!(o, a, 1);
        let prop = match dynamic_property_type(v) {
            Some(t) => py_result(py, p.dynamic_property(t))?,
            None => {
                return Err(to_py_err(
                    py,
                    &ocio::Exception::new("Cannot find dynamic property."),
                ));
            }
        };
        dynamic_property_out(py, prop)
    }
);

// .def("hasDynamicProperty", (bool (Processor::*)(DynamicPropertyType) const noexcept)
// &Processor::hasDynamicProperty, "type"_a) (PyProcessor.cpp:57-61 @ v2.5.2).
static HAS_DYNAMIC_PROPERTY: Function = method_def!(
    "hasDynamicProperty",
    [SELF, Param::new("type", DYNAMIC_PROPERTY_TYPE)],
    "bool",
    |py, o, a, _| {
        let p = this_proc!(a);
        let v = enum_param!(o, a, 1);
        ok(
            py,
            dynamic_property_type(v).is_some_and(|t| p.has_dynamic_property(t)),
        )
    }
);

// .def("isDynamic", &Processor::isDynamic) (PyProcessor.cpp:62-63 @ v2.5.2).
static IS_DYNAMIC: Function = method_def!("isDynamic", [SELF], "bool", |py, _, a, _| {
    let p = this_proc!(a);
    ok(py, p.is_dynamic())
});

const FLAGS: Param = Param::new("oFlags", OPTIMIZATION_FLAGS);
const IN_BIT_DEPTH: Param = Param::new("inBitDepth", BIT_DEPTH);
const OUT_BIT_DEPTH: Param = Param::new("outBitDepth", BIT_DEPTH);

// .def("getOptimizedProcessor", (ConstProcessorRcPtr(Processor::*)(OptimizationFlags) const)
// &Processor::getOptimizedProcessor, "oFlags"_a), .def("getOptimizedProcessor",
// (ConstProcessorRcPtr(Processor::*)(BitDepth, BitDepth, OptimizationFlags) const)
// &Processor::getOptimizedProcessor, "inBitDepth"_a, "outBitDepth"_a, "oFlags"_a)
// (PyProcessor.cpp:64-72 @ v2.5.2). A bit depth outside the enumerators, which the wheel
// hands the library (its cache key writes the number), is the port's error (U-112).
static GET_OPTIMIZED_PROCESSOR: Function = Function {
    name: "getOptimizedProcessor",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, FLAGS],
            ret: PROCESSOR,
            call: |py, o, a, _| {
                let p = this_proc!(a);
                let f = flags(enum_param!(o, a, 1));
                processor_out(py, py_result(py, p.optimized_processor(f))?)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, IN_BIT_DEPTH, OUT_BIT_DEPTH, FLAGS],
            ret: PROCESSOR,
            call: |py, o, a, _| {
                let p = this_proc!(a);
                let in_bd = enum_param!(o, a, 1);
                let out_bd = enum_param!(o, a, 2);
                let f = flags(enum_param!(o, a, 3));
                let (in_bd, out_bd) = (processor_bit_depth(in_bd)?, processor_bit_depth(out_bd)?);
                let opt = p.optimized_processor_with_bit_depths(in_bd, out_bd, f);
                processor_out(py, py_result(py, opt)?)
            },
            pos_only: 0,
        },
    ],
};

// .def("getDefaultGPUProcessor", &Processor::getDefaultGPUProcessor) (PyProcessor.cpp:75-76 @
// v2.5.2).
static GET_DEFAULT_GPU_PROCESSOR: Function = method_def!(
    "getDefaultGPUProcessor",
    [SELF],
    GPU_PROCESSOR,
    |py, _, a, _| {
        let p = this_proc!(a);
        gpu_processor_out(py, py_result(py, p.default_gpu_processor())?)
    }
);

// .def("getOptimizedGPUProcessor", &Processor::getOptimizedGPUProcessor, "oFlags"_a)
// (PyProcessor.cpp:77-78 @ v2.5.2).
static GET_OPTIMIZED_GPU_PROCESSOR: Function = method_def!(
    "getOptimizedGPUProcessor",
    [SELF, FLAGS],
    GPU_PROCESSOR,
    |py, o, a, _| {
        let p = this_proc!(a);
        let f = flags(enum_param!(o, a, 1));
        gpu_processor_out(py, py_result(py, p.optimized_gpu_processor(f))?)
    }
);

// .def("getDefaultCPUProcessor", &Processor::getDefaultCPUProcessor) (PyProcessor.cpp:81-82 @
// v2.5.2).
static GET_DEFAULT_CPU_PROCESSOR: Function = method_def!(
    "getDefaultCPUProcessor",
    [SELF],
    CPU_PROCESSOR,
    |py, _, a, _| {
        let p = this_proc!(a);
        let cpu = py_result(py, p.default_cpu_processor())?;
        Ok(Some(cpu_processor_out(py, &cpu)?.unbind()))
    }
);

// .def("getOptimizedCPUProcessor", (ConstCPUProcessorRcPtr (Processor::*)(OptimizationFlags)
// const) &Processor::getOptimizedCPUProcessor, "oFlags"_a), .def("getOptimizedCPUProcessor",
// (ConstCPUProcessorRcPtr (Processor::*)(BitDepth, BitDepth, OptimizationFlags) const)
// &Processor::getOptimizedCPUProcessor, "inBitDepth"_a, "outBitDepth"_a, "oFlags"_a)
// (PyProcessor.cpp:83-92 @ v2.5.2). A bit depth outside the enumerators is the port's error
// (U-112).
static GET_OPTIMIZED_CPU_PROCESSOR: Function = Function {
    name: "getOptimizedCPUProcessor",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, FLAGS],
            ret: CPU_PROCESSOR,
            call: |py, o, a, _| {
                let p = this_proc!(a);
                let f = flags(enum_param!(o, a, 1));
                let cpu = py_result(py, p.optimized_cpu_processor(f))?;
                Ok(Some(cpu_processor_out(py, &cpu)?.unbind()))
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, IN_BIT_DEPTH, OUT_BIT_DEPTH, FLAGS],
            ret: CPU_PROCESSOR,
            call: |py, o, a, _| {
                let p = this_proc!(a);
                let in_bd = enum_param!(o, a, 1);
                let out_bd = enum_param!(o, a, 2);
                let f = flags(enum_param!(o, a, 3));
                let (in_bd, out_bd) = (processor_bit_depth(in_bd)?, processor_bit_depth(out_bd)?);
                let cpu = p.optimized_cpu_processor_with_bit_depths(in_bd, out_bd, f);
                let cpu = py_result(py, cpu)?;
                Ok(Some(cpu_processor_out(py, &cpu)?.unbind()))
            },
            pos_only: 0,
        },
    ],
};

// ---------------------------------------------------------------------------------------------
// TransformFormatMetadataIterator

/// The processor an iterator of `self` walks, or the next overload.
macro_rules! iter_proc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let p = it.get().obj::<ProcessorRc>()?.0.clone();
        (it, p)
    }};
}

// .def("__len__", [](TransformFormatMetadataIterator & it) { return
// it.m_obj->getNumTransforms(); }) (PyProcessor.cpp:95-98 @ v2.5.2).
static IT_LEN: Function = method_def!("__len__", [IT_SELF], "int", |py, o, a, _| {
    let (_, p) = iter_proc!(o, a);
    ok(py, p.num_transforms())
});

// .def("__getitem__", [](TransformFormatMetadataIterator & it, int i) -> const FormatMetadata &
// { it.checkIndex(i, it.m_obj->getNumTransforms()); return
// it.m_obj->getTransformFormatMetadata(i); }, py::return_value_policy::reference_internal)
// (PyProcessor.cpp:99-105 @ v2.5.2): a negative index passes the check, and the library refuses
// it.
static IT_GETITEM: Function = method_def!(
    "__getitem__",
    [IT_SELF, Param::new("arg0", INT)],
    FORMAT_METADATA,
    |py, o, a, convert| {
        let (_, p) = iter_proc!(o, a);
        let i = arg!(a, 1, convert, i32);
        check_index(i, p.num_transforms())?;
        crate::py_format_metadata::processor_metadata_out(py, &a[0], &p, Some(i))
    }
);

// .def("__iter__", [](TransformFormatMetadataIterator & it) -> TransformFormatMetadataIterator
// & { return it; }) (PyProcessor.cpp:106-110 @ v2.5.2).
static IT_ITER: Function = method_def!(
    "__iter__",
    [IT_SELF],
    TRANSFORM_FORMAT_METADATA_ITERATOR,
    |_, o, a, _| {
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    }
);

// .def("__next__", [](TransformFormatMetadataIterator & it) -> const FormatMetadata & { int i =
// it.nextIndex(it.m_obj->getNumTransforms()); return it.m_obj->getTransformFormatMetadata(i);
// }, py::return_value_policy::reference_internal) (PyProcessor.cpp:111-116 @ v2.5.2).
static IT_NEXT: Function = method_def!("__next__", [IT_SELF], FORMAT_METADATA, |py, o, a, _| {
    let (it, p) = iter_proc!(o, a);
    let i = it.get().next_index(p.num_transforms())?;
    crate::py_format_metadata::processor_metadata_out(py, &a[0], &p, Some(i))
});

/// Port of `bindPyProcessor` (src/bindings/python/PyProcessor.cpp:23-117 @ v2.5.2), in its
/// order: the iterator class declared (29-31), the class's methods (33-92), the iterator's
/// (94-116).
pub(crate) fn bind_py_processor(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, PROCESSOR)?;
    let iterator = nested_class(&cls, "Processor", "TransformFormatMetadataIterator")?;
    for f in [
        &IS_NO_OP,
        &HAS_CHANNEL_CROSSTALK,
        &GET_CACHE_ID,
        &GET_PROCESSOR_METADATA,
        &GET_FORMAT_METADATA,
        &GET_TRANSFORM_FORMAT_METADATA,
        &CREATE_GROUP_TRANSFORM,
        &GET_DYNAMIC_PROPERTY,
        &HAS_DYNAMIC_PROPERTY,
        &IS_DYNAMIC,
        &GET_OPTIMIZED_PROCESSOR,
        &GET_DEFAULT_GPU_PROCESSOR,
        &GET_OPTIMIZED_GPU_PROCESSOR,
        &GET_DEFAULT_CPU_PROCESSOR,
        &GET_OPTIMIZED_CPU_PROCESSOR,
    ] {
        def(&cls, f)?;
    }
    for f in [&IT_LEN, &IT_GETITEM, &IT_ITER, &IT_NEXT] {
        def(&iterator, f)?;
    }
    Ok(())
}
