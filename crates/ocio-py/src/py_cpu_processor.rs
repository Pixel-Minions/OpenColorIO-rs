// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyCPUProcessor` (src/bindings/python/PyCPUProcessor.cpp @ v2.5.2): the class
//! `CPUProcessor`, whose `apply`, `applyRGB` and `applyRGBA` process image descriptions, Python
//! buffers and lists in place, with the GIL released (chunk 6.5b).
//!
//! pybind11 holds a CPU processor by its `CPUProcessorRcPtr`, the pointer the library shares
//! (a processor's cache gives the same one again): the Python object is the one made for that
//! processor while it lives ([`cpu_processor_out`]).

use std::ffi::c_long;
use std::sync::{Arc, LazyLock, Mutex};

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::image_desc::{At, Bytes, ImageLayout, PackedImageDesc};
use ocio_ops::open_color_types::DynamicPropertyType;
use ocio_py_ffi::Buffer;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyCFunction;

use crate::dispatch::{Function, Overload, Param};
use crate::py_image_desc::{
    DescribeError, Described, IMAGE_DESC, Image, ImageArgs, describe, describe_in_one, null_image,
    read_memory, size,
};
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_transform::{
    BIT_DEPTH, Out, Registry, class, def, enum_out, lock, none, ok, register_in, string_out,
};
use crate::py_utils::{
    BUFFER, BufferArg, buffer_bit_depth, buffer_size, check_buffer_divisible,
    check_c_contiguous_array, check_vector_divisible,
};
use crate::{arg, enum_param, image_arg, method_def, pybind11, q};

pub(crate) const CPU_PROCESSOR: &str = q!("CPUProcessor");
const DYNAMIC_PROPERTY_TYPE: &str = q!("DynamicPropertyType");
const SELF: Param = Param::new("self", CPU_PROCESSOR);
const FLOATS: &str = "collections.abc.Sequence[typing.SupportsFloat]";

/// The value of a `CPUProcessor` instance: pybind11's holder, the shared processor.
#[pyclass(frozen, module = "pybind11_builtins", name = "_cpu_processor")]
#[derive(Debug)]
pub(crate) struct CpuProcessorHolder(pub(crate) Arc<CpuProcessor>);

/// pybind11's registered instances of `CPUProcessor`, by the processor's address.
static INSTANCES: LazyLock<Mutex<Registry<usize>>> = LazyLock::new(|| Mutex::new(Registry::new()));
static INSTANCES_CALLBACK: PyOnceLock<Py<PyCFunction>> = PyOnceLock::new();

/// A returned `CPUProcessorRcPtr`: the Python object registered for the processor while it
/// lives, else a new instance holding it, registered.
pub(crate) fn cpu_processor_out<'py>(
    py: Python<'py>,
    cpu: &Arc<CpuProcessor>,
) -> PyResult<Bound<'py, PyAny>> {
    let key = Arc::as_ptr(cpu).addr();
    if let Some(obj) = lock(&INSTANCES).find(py, &key) {
        return Ok(obj);
    }
    let cls = class(py, CPU_PROCESSOR)?;
    let obj = cls.call_method1("__new__", (&cls,))?;
    pybind11::set_value(
        &obj,
        CPU_PROCESSOR,
        Bound::new(py, CpuProcessorHolder(cpu.clone()))?.into_any(),
    )?;
    register_in(&INSTANCES, &INSTANCES_CALLBACK, &obj, key)?;
    Ok(obj)
}

/// The processor `obj` holds as a `CPUProcessor` (`Ok(None)`: not one).
pub(crate) fn cpu_processor_of(obj: &Bound<'_, PyAny>) -> PyResult<Option<Arc<CpuProcessor>>> {
    let Some(this) = pybind11::instance_of(obj, CPU_PROCESSOR) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, CPU_PROCESSOR)?;
    Ok(Some(value.cast::<CpuProcessorHolder>()?.get().0.clone()))
}

/// The processor of `self`, or the next overload.
macro_rules! this_cpu {
    ($a:expr) => {
        match cpu_processor_of(&$a[0])? {
            Some(cpu) => cpu,
            None => return Ok(None),
        }
    };
}

/// The library's `DynamicPropertyType` of the C++ value `v`, if it is an enumerator. No
/// renderer has a property of another value (each compares the types).
fn dynamic_property_type(v: i64) -> Option<DynamicPropertyType> {
    use DynamicPropertyType::*;
    [
        Exposure,
        Contrast,
        Gamma,
        GradingPrimary,
        GradingRgbCurve,
        GradingTone,
        GradingHueCurve,
    ]
    .into_iter()
    .find(|t| *t as i64 == v)
}

// .def("isNoOp", &CPUProcessor::isNoOp) (PyCPUProcessor.cpp:21-22 @ v2.5.2).
static IS_NO_OP: Function = method_def!("isNoOp", [SELF], "bool", |py, _, a, _| {
    let cpu = this_cpu!(a);
    ok(py, cpu.is_no_op())
});

// .def("isIdentity", &CPUProcessor::isIdentity) (PyCPUProcessor.cpp:23-24 @ v2.5.2).
static IS_IDENTITY: Function = method_def!("isIdentity", [SELF], "bool", |py, _, a, _| {
    let cpu = this_cpu!(a);
    ok(py, cpu.is_identity())
});

// .def("hasChannelCrosstalk", &CPUProcessor::hasChannelCrosstalk) (PyCPUProcessor.cpp:25-26 @
// v2.5.2).
static HAS_CHANNEL_CROSSTALK: Function =
    method_def!("hasChannelCrosstalk", [SELF], "bool", |py, _, a, _| {
        let cpu = this_cpu!(a);
        ok(py, cpu.has_channel_crosstalk())
    });

// .def("getCacheID", &CPUProcessor::getCacheID) (PyCPUProcessor.cpp:27-28 @ v2.5.2).
static GET_CACHE_ID: Function = method_def!("getCacheID", [SELF], "str", |py, _, a, _| {
    let cpu = this_cpu!(a);
    string_out(py, cpu.get_cache_id())
});

// .def("getInputBitDepth", &CPUProcessor::getInputBitDepth) (PyCPUProcessor.cpp:29-30 @
// v2.5.2).
static GET_INPUT_BIT_DEPTH: Function =
    method_def!("getInputBitDepth", [SELF], BIT_DEPTH, |py, _, a, _| {
        let cpu = this_cpu!(a);
        enum_out(py, BIT_DEPTH, cpu.get_input_bit_depth() as i64)
    });

// .def("getOutputBitDepth", &CPUProcessor::getOutputBitDepth) (PyCPUProcessor.cpp:31-32 @
// v2.5.2).
static GET_OUTPUT_BIT_DEPTH: Function =
    method_def!("getOutputBitDepth", [SELF], BIT_DEPTH, |py, _, a, _| {
        let cpu = this_cpu!(a);
        enum_out(py, BIT_DEPTH, cpu.get_output_bit_depth() as i64)
    });

// .def("getDynamicProperty", [](CPUProcessorRcPtr & self, DynamicPropertyType type) { return
// PyDynamicProperty(self->getDynamicProperty(type)); }, "type"_a) (PyCPUProcessor.cpp:33-38 @
// v2.5.2).
static GET_DYNAMIC_PROPERTY: Function = method_def!(
    "getDynamicProperty",
    [SELF, Param::new("type", DYNAMIC_PROPERTY_TYPE)],
    crate::py_dynamic_property::DYNAMIC_PROPERTY,
    |py, o, a, _| {
        let cpu = this_cpu!(a);
        let v = enum_param!(o, a, 1);
        let prop = match dynamic_property_type(v) {
            Some(t) => py_result(py, cpu.get_dynamic_property(t))?,
            None => {
                return Err(to_py_err(
                    py,
                    &ocio::Exception::new(
                        "Cannot find dynamic property; not used by CPU processor.",
                    ),
                ));
            }
        };
        crate::py_dynamic_property::dynamic_property_out(py, prop)
    }
);

// .def("hasDynamicProperty", (bool (CPUProcessor::*)(DynamicPropertyType) const noexcept)
// &CPUProcessor::hasDynamicProperty, "type"_a) (PyCPUProcessor.cpp:39-43 @ v2.5.2).
static HAS_DYNAMIC_PROPERTY: Function = method_def!(
    "hasDynamicProperty",
    [SELF, Param::new("type", DYNAMIC_PROPERTY_TYPE)],
    "bool",
    |py, o, a, _| {
        let cpu = this_cpu!(a);
        let v = enum_param!(o, a, 1);
        ok(
            py,
            dynamic_property_type(v).is_some_and(|t| cpu.has_dynamic_property(t)),
        )
    }
);

// .def("isDynamic", &CPUProcessor::isDynamic) (PyCPUProcessor.cpp:44-45 @ v2.5.2).
static IS_DYNAMIC: Function = method_def!("isDynamic", [SELF], "bool", |py, _, a, _| {
    let cpu = this_cpu!(a);
    ok(py, cpu.is_dynamic())
});

/// Why an apply stopped with the GIL released.
enum ApplyError {
    /// The library's error.
    Describe(DescribeError),
    /// A byte the destination image writes is one the source image reads, without the images
    /// being the same image (U-150).
    Overlap,
}

impl From<DescribeError> for ApplyError {
    fn from(e: DescribeError) -> Self {
        ApplyError::Describe(e)
    }
}

impl From<ocio::Exception> for ApplyError {
    fn from(e: ocio::Exception) -> Self {
        ApplyError::Describe(DescribeError::Library(e))
    }
}

impl ApplyError {
    fn into_py(self, py: Python<'_>) -> PyErr {
        match self {
            ApplyError::Describe(e) => e.into_py(py),
            ApplyError::Overlap => pyo3::exceptions::PyBufferError::new_err(
                "CPUProcessor.apply: the destination image writes bytes that the source image \
                 reads, without being the same image, and the wheel's result then depends on \
                 the order in which it processes them (docs/improvements.md, U-150)",
            ),
        }
    }
}

/// Whether the library can describe an image whose buffers lie in the regions `places` names
/// over those regions: its buffers all in one region, or each in its own.
fn describable(places: &[(usize, usize)]) -> bool {
    let all_one = places.iter().all(|p| p.0 == places[0].0);
    let distinct = places
        .iter()
        .enumerate()
        .all(|(i, p)| places[..i].iter().all(|q| q.0 != p.0));
    all_one || distinct
}

/// The library's description, to write, of an image made with `args` over memory `with_bytes`
/// lent to write, whose buffers lie in the regions as [`describable`] takes them: each buffer's
/// region (`regions[place.0]`, taken) at its place; one byte from `ones` for a buffer with an
/// address and no items (`read_memory`'s [`NOT_NULL`] reason). Planes in one region are
/// described in one buffer, as C++ passes pointers into one allocation.
///
/// [`NOT_NULL`]: crate::py_image_desc
fn describe_mut<'b>(
    args: &ImageArgs,
    regions: &mut [Option<&'b mut [u8]>],
    places: &[(usize, usize)],
    addressed: &[bool],
    ones: &'b mut [[u8; 1]; 4],
) -> Result<Described<&'b mut [u8]>, DescribeError> {
    let first = places[0].0;
    if places.len() > 1 && places.iter().all(|p| p.0 == first) {
        let memory = regions[first].take().unwrap_or(&mut []);
        let offsets: Vec<usize> = places.iter().map(|p| p.1).collect();
        return describe_in_one(args, memory, &offsets);
    }
    let mut memory = Vec::new();
    for ((place, &addressed), one) in places.iter().zip(addressed).zip(ones.iter_mut()) {
        let bytes = regions[place.0].take().unwrap_or(&mut []);
        memory.push(if bytes.is_empty() && addressed {
            (&mut one[..], 0)
        } else {
            (bytes, place.1)
        });
    }
    describe(args, memory)
}

/// The port's error where the wheel writes into a read-only buffer, once the library accepted
/// the image (docs/improvements.md, U-156): the processing ran on a copy of its memory.
fn read_only(call: &str) -> PyErr {
    pyo3::exceptions::PyBufferError::new_err(format!(
        "{call}: the buffer is read-only, and the wheel writes into it (docs/improvements.md, \
         U-156)"
    ))
}

/// Whether one of `lends`' buffers to write is read-only and has items. Then every buffer is
/// lent to read, and the processing runs on copies of the memory it would write: the library's
/// error where it has one, as the wheel's, and the port's error otherwise ([`read_only`]),
/// where the wheel writes.
fn dry_run(lends: &[(&Buffer, bool)]) -> bool {
    lends.iter().any(|(b, write)| {
        let (start, end) = b.span();
        *write && b.info().readonly && start < end
    })
}

/// `regions`, with each region of `to_write` that was lent to read (a [`dry_run`]) replaced by
/// a copy of it in `copies`, to write: for `applyRGB`/`applyRGBA`, whose buffer is C-contiguous,
/// so that its region holds its items and nothing else.
fn writable<'c>(
    regions: Vec<ocio_py_ffi::Bytes<'c>>,
    to_write: &[bool],
    copies: &'c mut Vec<Vec<u8>>,
) -> Vec<ocio_py_ffi::Bytes<'c>> {
    *copies = regions
        .iter()
        .zip(to_write)
        .map(|(r, &w)| match r {
            ocio_py_ffi::Bytes::Read(b) if w => b.to_vec(),
            _ => Vec::new(),
        })
        .collect();
    regions
        .into_iter()
        .zip(to_write)
        .zip(copies.iter_mut())
        .map(|((r, &w), copy)| match r {
            ocio_py_ffi::Bytes::Read(_) if w => ocio_py_ffi::Bytes::Write(&mut copy[..]),
            r => r,
        })
        .collect()
}

/// The bytes of a region `with_bytes` lent, to read.
fn bytes_of<'r>(region: &'r ocio_py_ffi::Bytes<'_>) -> &'r [u8] {
    match region {
        ocio_py_ffi::Bytes::Read(b) => b,
        ocio_py_ffi::Bytes::Write(b) => b,
    }
}

/// The layout of an image made with `args` whose buffers lie in `regions` at `places`: the
/// library's description over the regions, read (planes in one region each described over it,
/// as separate buffers), whose channel positions are then the buffer's index among `places` and
/// the offset in its region.
fn layout_over(
    args: &ImageArgs,
    regions: &[ocio_py_ffi::Bytes<'_>],
    places: &[(usize, usize)],
    addressed: &[bool],
) -> Result<ImageLayout, DescribeError> {
    let shared: Vec<ocio_py_ffi::Bytes<'_>> = regions
        .iter()
        .map(|r| ocio_py_ffi::Bytes::Read(bytes_of(r)))
        .collect();
    let d = describe(args, read_memory(&shared, places, addressed))?;
    Ok(*d.desc().layout())
}

/// Calls `f(buffer, offset, len)` for each channel value the CPU engine reads or writes of an
/// image of `layout` (the channels of every pixel, R, G, B and A, row by row), `buffer` the index
/// of its buffer and `offset` its first byte's in that buffer, `len` its bytes. The constructors
/// checked that each is in its buffer (D-2). An RGBA-packed image, which the engine reads and
/// writes in whole rows from its red channel, has no bytes between its channels and pixels, so
/// its rows are these values.
fn for_each_value(layout: &ImageLayout, mut f: impl FnMut(usize, usize, usize)) {
    let len = ocio_ops::bit_depth_utils::get_channel_size_in_bytes(layout.bit_depth())
        .expect("a bit depth the constructors accepted") as usize;
    let channels: Vec<ocio_ops::image_desc::ChannelPos> =
        [layout.r_data(), layout.g_data(), layout.b_data()]
            .into_iter()
            .chain(layout.a_data())
            .collect();
    let (xs, ys) = (
        layout.x_stride_bytes() as i128,
        layout.y_stride_bytes() as i128,
    );
    for y in 0..layout.height() as i128 {
        for x in 0..layout.width() as i128 {
            for c in &channels {
                let at = c.offset as i128 + x * xs + y * ys;
                f(c.buffer, at as usize, len);
            }
        }
    }
}

/// A copy of the bytes an image reads or writes, in one buffer: the regions its buffers lie in
/// one after another, the bytes it doesn't touch zero (the bytes between items of a strided view
/// are neither read nor copied, as the wheel doesn't read them), and where each of its buffers'
/// first item is in it, in the order of `places`.
struct Scratch {
    bytes: Vec<u8>,
    /// Where each region starts in `bytes`, for the regions the image lies in.
    starts: Vec<Option<usize>>,
    offsets: Vec<usize>,
}

/// The [`Scratch`] of an image of `layout` whose buffers lie in `regions` at `places`.
fn scratch(
    regions: &[ocio_py_ffi::Bytes<'_>],
    places: &[(usize, usize)],
    layout: &ImageLayout,
) -> Scratch {
    let mut starts = vec![None; regions.len()];
    let mut len = 0;
    for p in places {
        if starts[p.0].is_none() {
            starts[p.0] = Some(len);
            len += bytes_of(&regions[p.0]).len();
        }
    }
    let mut bytes = vec![0u8; len];
    let start = |r: usize| starts[r].expect("a region the image lies in");
    for_each_value(layout, |buffer, at, n| {
        let r = places[buffer].0;
        let to = start(r) + at;
        bytes[to..to + n].copy_from_slice(&bytes_of(&regions[r])[at..at + n]);
    });
    let offsets = places.iter().map(|p| start(p.0) + p.1).collect();
    Scratch {
        bytes,
        starts,
        offsets,
    }
}

impl Scratch {
    /// Copies the bytes an image of `layout` writes back from the scratch into `regions` (lent
    /// to write), whose buffers lie at `places`.
    fn write_back(
        &self,
        regions: &mut [Option<&mut [u8]>],
        places: &[(usize, usize)],
        layout: &ImageLayout,
    ) {
        for_each_value(layout, |buffer, at, n| {
            let r = places[buffer].0;
            let from = self.starts[r].expect("a region the image lies in") + at;
            if let Some(region) = regions[r].as_deref_mut() {
                region[at..at + n].copy_from_slice(&self.bytes[from..from + n]);
            }
        });
    }
}

/// Whether a byte that an image of `dst` (layout, places) writes is one that an image of `src`
/// reads, in the regions they share: an exact check, each byte of every channel value marked.
fn share_a_byte(
    regions: &[ocio_py_ffi::Bytes<'_>],
    src: (&ImageLayout, &[(usize, usize)]),
    dst: (&ImageLayout, &[(usize, usize)]),
) -> bool {
    let shared: Vec<bool> = (0..regions.len())
        .map(|r| src.1.iter().any(|p| p.0 == r) && dst.1.iter().any(|p| p.0 == r))
        .collect();
    let mut read: Vec<Vec<u64>> = regions
        .iter()
        .zip(&shared)
        .map(|(region, &s)| {
            let words = if s {
                bytes_of(region).len() / 64 + 1
            } else {
                0
            };
            vec![0u64; words]
        })
        .collect();
    for_each_value(src.0, |buffer, at, n| {
        let r = src.1[buffer].0;
        if shared[r] {
            for byte in at..at + n {
                read[r][byte / 64] |= 1 << (byte % 64);
            }
        }
    });
    let mut found = false;
    for_each_value(dst.0, |buffer, at, n| {
        let r = dst.1[buffer].0;
        if shared[r] && (at..at + n).any(|byte| read[r][byte / 64] & (1 << (byte % 64)) != 0) {
            found = true;
        }
    });
    found
}

/// What [`process`] needs besides the memory: the processor, the images' arguments (the
/// destination's for `apply(src, dst)`), where the source's buffers end among the buffers, which
/// buffers have an address, and whether the processing runs on copies only ([`dry_run`]).
struct Job {
    cpu: Arc<CpuProcessor>,
    src_args: ImageArgs,
    dst_args: Option<ImageArgs>,
    n: usize,
    addressed: Vec<bool>,
    dry: bool,
}

/// The processing of [`apply_images`] over `regions` (`with_bytes`'), each buffer at its place:
/// - in place, over the regions, when the library describes the image over them
///   ([`describable`]); otherwise (planes some of which share memory, as `PlanarImageDesc(b, g,
///   b, ...)`) over a [`Scratch`] copy of the bytes it reads, described in one buffer as C++
///   passes pointers into one allocation, the bytes it writes then copied back: the wheel's
///   result, which processes each row's channels read and then written through the pointers;
/// - from the source to the destination, which share no region; or, for images that share
///   memory and are the same image, as one image (`CpuProcessor::apply_same`); or, for images
///   that share memory without a byte the destination writes being one the source reads (the
///   left and right halves of an image's rows), from a copy of the bytes the source reads, as
///   the wheel's result is then. Images whose bytes do overlap are the port's error (U-150), once
///   the processing ran on copies, so that the library's error comes first, as the wheel's.
///
/// A [`dry_run`] processes copies only. The library's error after part of the image is written
/// (U-1) leaves that part written, as the wheel's, copies too.
fn process(
    job: &Job,
    regions: Vec<ocio_py_ffi::Bytes<'_>>,
    places: &[(usize, usize)],
) -> Result<(), ApplyError> {
    let n = job.n;
    let mut ones = [[0u8; 1]; 4];
    let (src_places, dst_places) = places.split_at(n);
    let src_layout = layout_over(&job.src_args, &regions, src_places, &job.addressed[..n])?;
    let Some(dst_args) = job.dst_args else {
        if !job.dry && describable(src_places) {
            let mut writes = writes_of(regions);
            let mut d = describe_mut(
                &job.src_args,
                &mut writes,
                src_places,
                &job.addressed,
                &mut ones,
            )?;
            job.cpu.apply(d.desc_mut())?;
            return Ok(());
        }
        let mut copy = scratch(&regions, src_places, &src_layout);
        let done = {
            let mut d = describe_in_one(&job.src_args, &mut copy.bytes[..], &copy.offsets)?;
            job.cpu.apply(d.desc_mut())
        };
        if !job.dry {
            copy.write_back(&mut writes_of(regions), src_places, &src_layout);
        }
        return done.map_err(ApplyError::from);
    };
    let dst_layout = layout_over(&dst_args, &regions, dst_places, &job.addressed[n..])?;
    let shared = src_places
        .iter()
        .any(|p| dst_places.iter().any(|q| q.0 == p.0));
    if shared && job.src_args == dst_args && src_places == dst_places {
        if !job.dry && describable(dst_places) {
            let mut writes = writes_of(regions);
            let mut d = describe_mut(
                &dst_args,
                &mut writes,
                dst_places,
                &job.addressed[n..],
                &mut ones,
            )?;
            job.cpu.apply_same(d.desc_mut())?;
            return Ok(());
        }
        let mut copy = scratch(&regions, dst_places, &dst_layout);
        let done = {
            let mut d = describe_in_one(&dst_args, &mut copy.bytes[..], &copy.offsets)?;
            job.cpu.apply_same(d.desc_mut())
        };
        if !job.dry {
            copy.write_back(&mut writes_of(regions), dst_places, &dst_layout);
        }
        return done.map_err(ApplyError::from);
    }
    let overlap = shared
        && share_a_byte(
            &regions,
            (&src_layout, src_places),
            (&dst_layout, dst_places),
        );
    let src_copy = shared.then(|| scratch(&regions, src_places, &src_layout));
    let direct = !job.dry && !overlap && describable(dst_places);
    let mut dst_copy = (!direct).then(|| scratch(&regions, dst_places, &dst_layout));
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    for region in regions {
        match region {
            ocio_py_ffi::Bytes::Read(b) => {
                reads.push(ocio_py_ffi::Bytes::Read(b));
                writes.push(None);
            }
            ocio_py_ffi::Bytes::Write(b) => {
                reads.push(ocio_py_ffi::Bytes::Read(&[]));
                writes.push(Some(b));
            }
        }
    }
    let s = match &src_copy {
        Some(copy) => describe_in_one(&job.src_args, &copy.bytes[..], &copy.offsets)?,
        None => describe(
            &job.src_args,
            read_memory(&reads, src_places, &job.addressed[..n]),
        )?,
    };
    let done = match &mut dst_copy {
        None => {
            let mut d = describe_mut(
                &dst_args,
                &mut writes,
                dst_places,
                &job.addressed[n..],
                &mut ones,
            )?;
            job.cpu.apply_src_dst(s.desc(), d.desc_mut())
        }
        Some(copy) => {
            let done = {
                let mut d = describe_in_one(&dst_args, &mut copy.bytes[..], &copy.offsets)?;
                job.cpu.apply_src_dst(s.desc(), d.desc_mut())
            };
            if !job.dry && !overlap {
                copy.write_back(&mut writes, dst_places, &dst_layout);
            }
            done
        }
    };
    done?;
    if overlap {
        return Err(ApplyError::Overlap);
    }
    Ok(())
}

/// The regions lent to write, each once.
fn writes_of(regions: Vec<ocio_py_ffi::Bytes<'_>>) -> Vec<Option<&mut [u8]>> {
    regions
        .into_iter()
        .map(|r| match r {
            ocio_py_ffi::Bytes::Write(b) => Some(b),
            ocio_py_ffi::Bytes::Read(_) => None,
        })
        .collect()
}

/// `self->apply(*imgDesc.m_img)`, or `apply(*src.m_img, *dst.m_img)` (`dst` given), with the GIL
/// released: each image's buffers requested again ([`Image::request`]) and lent, the source's to
/// read and the destination's to write, and the library's descriptions made over them, as
/// [`process`] does.
///
/// Where the wheel writes memory it may not, the library's error comes first, as the wheel's
/// (the processing runs on copies of the memory, which are dropped), then the port's: a
/// read-only destination (U-156, [`dry_run`]); a source and a destination whose bytes overlap
/// (U-150).
fn apply_images(
    py: Python<'_>,
    cpu: &Arc<CpuProcessor>,
    src: &Image,
    dst: Option<&Image>,
) -> PyResult<()> {
    let src_buffers = src.request(py)?;
    let dst_buffers = match dst {
        Some(dst) => dst.request(py)?,
        None => Vec::new(),
    };
    let in_place = dst.is_none();
    let wanted: Vec<(&Buffer, bool)> = src_buffers
        .iter()
        .map(|b| (b, in_place))
        .chain(dst_buffers.iter().map(|b| (b, true)))
        .collect();
    let dry = dry_run(&wanted);
    let lends: Vec<(&Buffer, bool)> = wanted.iter().map(|&(b, w)| (b, w && !dry)).collect();
    let job = Job {
        cpu: cpu.clone(),
        src_args: src.args,
        dst_args: dst.map(|d| d.args),
        n: src_buffers.len(),
        addressed: lends.iter().map(|(b, _)| b.info().ptr != 0).collect(),
        dry,
    };
    let done = ocio_py_ffi::with_bytes(py, &lends, move |regions, places| {
        process(&job, regions, &places)
    })?;
    match done {
        Ok(()) if dry => Err(read_only("CPUProcessor.apply")),
        Ok(()) => Ok(()),
        Err(e) => Err(e.into_py(py)),
    }
}

// .def("apply", [](CPUProcessorRcPtr & self, PyImageDesc & imgDesc) { self->apply(
// (*imgDesc.m_img)); }, "imgDesc"_a, py::call_guard<py::gil_scoped_release>(), ...),
// .def("apply", [](CPUProcessorRcPtr & self, PyImageDesc & srcImgDesc, PyImageDesc &
// dstImgDesc) {...}, "srcImgDesc"_a, "dstImgDesc"_a, ...) (PyCPUProcessor.cpp:47-93 @ v2.5.2).
// An `ImageDesc()` is the port's error, where the wheel dereferences its null image (U-146).
static APPLY: Function = Function {
    name: "apply",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("imgDesc", IMAGE_DESC)],
            ret: "None",
            call: |py, o, a, _| {
                let cpu = this_cpu!(a);
                let image = image_arg!(o, a, 1).ok_or_else(|| null_image("CPUProcessor.apply"))?;
                apply_images(py, &cpu, &image, None)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("srcImgDesc", IMAGE_DESC),
                Param::new("dstImgDesc", IMAGE_DESC),
            ],
            ret: "None",
            call: |py, o, a, _| {
                let cpu = this_cpu!(a);
                let src = image_arg!(o, a, 1);
                let dst = image_arg!(o, a, 2);
                let (Some(src), Some(dst)) = (src, dst) else {
                    return Err(null_image("CPUProcessor.apply"));
                };
                apply_images(py, &cpu, &src, Some(&dst))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

/// `applyRGB`/`applyRGBA` of a buffer (PyCPUProcessor.cpp:94-123, 173-200 @ v2.5.2): the
/// buffer's size checked against `num_channels`, its C order, its bit depth from its format;
/// then, with the GIL released, a packed image of one row of `size / num_channels` pixels over
/// it (`(long)info.size / numChannels`), its strides from the item size, processed in place. A
/// read-only buffer: the library's error, else the port's (U-156, [`dry_run`]).
fn apply_buffer(
    py: Python<'_>,
    cpu: &Arc<CpuProcessor>,
    data: &Bound<'_, PyAny>,
    num_channels: c_long,
    call: &'static str,
) -> PyResult<()> {
    let buffer = Buffer::request(data)?;
    let info = buffer.info();
    check_buffer_divisible(py, info, num_channels as isize)?;
    check_c_contiguous_array(py, info)?;
    let bit_depth = buffer_bit_depth(py, info)?;

    let width = (buffer_size(info) as c_long).wrapping_div(num_channels);
    let chan_stride_bytes = info.itemsize;
    let x_stride_bytes = chan_stride_bytes.wrapping_mul(num_channels as isize);
    let y_stride_bytes = x_stride_bytes.wrapping_mul(width as isize);
    let addressed = info.ptr != 0;
    let dry = dry_run(&[(&buffer, true)]);
    let cpu = cpu.clone();
    let done = ocio_py_ffi::with_bytes(py, &[(&buffer, !dry)], move |regions, places| {
        let mut copies = Vec::new();
        let regions = writable(regions, &[true], &mut copies);
        let mut one = [0u8; 1];
        let (bytes, offset) = match regions.into_iter().next() {
            Some(ocio_py_ffi::Bytes::Write(b)) if !(b.is_empty() && addressed) => (b, places[0].1),
            _ => (&mut one[..], 0),
        };
        let mut img = PackedImageDesc::with_strides(
            At(Bytes(bytes), offset),
            size(width),
            1,
            size(num_channels),
            bit_depth,
            chan_stride_bytes,
            x_stride_bytes,
            y_stride_bytes,
        )?;
        cpu.apply(&mut img).map(|()| dry)
    })?;
    match py_result(py, done)? {
        false => Ok(()),
        true => Err(read_only(call)),
    }
}

/// `applyRGB`/`applyRGBA` of a list (PyCPUProcessor.cpp:140-152, 217-229 @ v2.5.2), with the GIL
/// released: its length checked against `num_channels`, then a packed F32 image of one row of
/// `size / num_channels` pixels over the values (a null pointer for none), processed in place;
/// the values returned as a new list.
fn apply_list(
    py: Python<'_>,
    cpu: &Arc<CpuProcessor>,
    mut data: Vec<f32>,
    num_channels: c_long,
) -> Out {
    check_vector_divisible(py, data.len(), num_channels as usize)?;
    let width = (data.len() as c_long).wrapping_div(num_channels);
    let cpu = cpu.clone();
    let done = py.detach(move || -> ocio::Result<Vec<f32>> {
        {
            let mut img = PackedImageDesc::new(&mut data, size(width), 1, size(num_channels))?;
            cpu.apply(&mut img)?;
        }
        Ok(data)
    });
    let data = py_result(py, done)?;
    ok(py, data.into_iter().map(f64::from).collect::<Vec<f64>>())
}

// .def("applyRGB", [](CPUProcessorRcPtr & self, py::buffer & data) {...}, "data"_a, ...),
// .def("applyRGB", [](CPUProcessorRcPtr & self, std::vector<float> & data) {...}, "data"_a,
// py::call_guard<py::gil_scoped_release>(), ...) (PyCPUProcessor.cpp:94-172 @ v2.5.2).
static APPLY_RGB: Function = Function {
    name: "applyRGB",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("data", BUFFER)],
            ret: "None",
            call: |py, _, a, convert| {
                let cpu = this_cpu!(a);
                let data = arg!(a, 1, convert, BufferArg).0;
                apply_buffer(py, &cpu, &data, 3, "CPUProcessor.applyRGB")?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("data", FLOATS)],
            ret: "list[float]",
            call: |py, _, a, convert| {
                let cpu = this_cpu!(a);
                let data = arg!(a, 1, convert, Vec<f32>);
                apply_list(py, &cpu, data, 3)
            },
            pos_only: 0,
        },
    ],
};

// .def("applyRGBA", [](CPUProcessorRcPtr & self, py::buffer & data) {...}, "data"_a, ...),
// .def("applyRGBA", [](CPUProcessorRcPtr & self, std::vector<float> & data) {...}, "data"_a,
// py::call_guard<py::gil_scoped_release>(), ...) (PyCPUProcessor.cpp:173-249 @ v2.5.2).
static APPLY_RGBA: Function = Function {
    name: "applyRGBA",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("data", BUFFER)],
            ret: "None",
            call: |py, _, a, convert| {
                let cpu = this_cpu!(a);
                let data = arg!(a, 1, convert, BufferArg).0;
                apply_buffer(py, &cpu, &data, 4, "CPUProcessor.applyRGBA")?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("data", FLOATS)],
            ret: "list[float]",
            call: |py, _, a, convert| {
                let cpu = this_cpu!(a);
                let data = arg!(a, 1, convert, Vec<f32>);
                apply_list(py, &cpu, data, 4)
            },
            pos_only: 0,
        },
    ],
};

/// The docstrings the definitions give after pybind11's signatures, overload by overload
/// (PyCPUProcessor.cpp:53-249 @ v2.5.2), line by line as written (some lines end in a space).
const APPLY_DOCS: [&str; 2] = [APPLY_IMAGE_DOC, APPLY_SRC_DST_DOC];
const APPLY_RGB_DOCS: [&str; 2] = [APPLY_RGB_BUFFER_DOC, APPLY_RGB_LIST_DOC];
const APPLY_RGBA_DOCS: [&str; 2] = [APPLY_RGBA_BUFFER_DOC, APPLY_RGBA_LIST_DOC];

const APPLY_IMAGE_DOC: &str = concat!(
    "\n",
    "Apply to an image with any kind of channel ordering while respecting \n",
    "the input and output bit-depths. Image values are modified in place.\n",
    "\n",
    ".. note::\n",
    "    The GIL is released during processing, freeing up Python to execute \n",
    "    other threads concurrently.\n",
    "\n",
    ".. note::\n",
    "    For large images, ``applyRGB`` or ``applyRGBA`` are preferred for \n",
    "    processing a NumPy array. The Python ``ImageDesc`` implementation \n",
    "    requires copying all values (once) in order to own the underlying \n",
    "    pointer. The dedicated packed ``apply*`` methods utilize \n",
    "    ``ImageDesc`` on the C++ side so avoid the copy.\n",
    "\n",
);

const APPLY_SRC_DST_DOC: &str = concat!(
    "\n",
    "Apply to an image with any kind of channel ordering while respecting \n",
    "the input and output bit-depths. Modified srcImgDesc image values are\n",
    "written to the dstImgDesc image, leaving srcImgDesc unchanged.\n",
    "\n",
    ".. note::\n",
    "    The GIL is released during processing, freeing up Python to execute \n",
    "    other threads concurrently.\n",
    "\n",
    ".. note::\n",
    "    For large images, ``applyRGB`` or ``applyRGBA`` are preferred for \n",
    "    processing a NumPy array. The Python ``ImageDesc`` implementation \n",
    "    requires copying all values (once) in order to own the underlying \n",
    "    pointer. The dedicated packed ``apply*`` methods utilize \n",
    "    ``ImageDesc`` on the C++ side so avoid the copy.\n",
    "\n",
);

const APPLY_RGB_BUFFER_DOC: &str = concat!(
    "\n",
    "Apply to a packed RGB array adhering to the Python buffer protocol. \n",
    "This will typically be a NumPy array. Input and output bit-depths are\n",
    "respected but must match. Any array size or shape is supported as long\n",
    "as the flattened array size is divisible by 3. Array values are \n",
    "modified in place.\n",
    "\n",
    ".. note::\n",
    "    This differs from the C++ implementation which only applies to a \n",
    "    single pixel. This method uses a ``PackedImageDesc`` under the \n",
    "    hood to apply to an entire image at once. The GIL is released \n",
    "    during processing, freeing up Python to execute other threads \n",
    "    concurrently.\n",
    "\n",
);

const APPLY_RGB_LIST_DOC: &str = concat!(
    "\n",
    "Apply to a packed RGB list of float values. Any size is supported as \n",
    "long as the list length is divisible by 3. A new list with processed\n",
    "float values is returned, leaving the input list unchanged.\n",
    "\n",
    ".. note::\n",
    "    This differs from the C++ implementation which only applies to a \n",
    "    single pixel. This method uses a ``PackedImageDesc`` under the \n",
    "    hood to apply to an entire image at once. The GIL is released \n",
    "    during processing, freeing up Python to execute other threads \n",
    "    concurrently.\n",
    "\n",
    ".. note::\n",
    "    For large images, a NumPy array should be preferred over a list.\n",
    "    List values are copied on input and output, where an array is \n",
    "    modified in place.\n",
    "\n",
);

const APPLY_RGBA_BUFFER_DOC: &str = concat!(
    "\n",
    "Apply to a packed RGBA array adhering to the Python buffer protocol. \n",
    "This will typically be a NumPy array. Input and output bit-depths are\n",
    "respected but must match. Any array size or shape is supported as long\n",
    "as the flattened array size is divisible by 4. Array values are \n",
    "modified in place.\n",
    "\n",
    ".. note::\n",
    "    This differs from the C++ implementation which only applies to a \n",
    "    single pixel. This method uses a ``PackedImageDesc`` under the \n",
    "    hood to apply to an entire image at once. The GIL is released \n",
    "    during processing, freeing up Python to execute other threads \n",
    "    concurrently.\n",
    "\n",
);

const APPLY_RGBA_LIST_DOC: &str = concat!(
    "\n",
    "Apply to a packed RGBA list of float values. Any size is supported as \n",
    "long as the list length is divisible by 4. A new list with processed\n",
    "float values is returned, leaving the input list unchanged.\n",
    "\n",
    ".. note::\n",
    "    This differs from the C++ implementation which only applies to a \n",
    "    single pixel. This method uses a ``PackedImageDesc`` under the \n",
    "    hood to apply to an entire image at once. The GIL is released \n",
    "    during processing, freeing up Python to execute other threads \n",
    "    concurrently.\n",
    "\n",
    ".. note::\n",
    "    For large images, a NumPy array should be preferred over a list.\n",
    "    List values are copied on input and output, where an array is \n",
    "    modified in place.\n",
    "\n",
);

/// Port of `bindPyCPUProcessor` (src/bindings/python/PyCPUProcessor.cpp:15-250 @ v2.5.2), in
/// its order.
pub(crate) fn bind_py_cpu_processor(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, CPU_PROCESSOR)?;
    for f in [
        &IS_NO_OP,
        &IS_IDENTITY,
        &HAS_CHANNEL_CROSSTALK,
        &GET_CACHE_ID,
        &GET_INPUT_BIT_DEPTH,
        &GET_OUTPUT_BIT_DEPTH,
        &GET_DYNAMIC_PROPERTY,
        &HAS_DYNAMIC_PROPERTY,
        &IS_DYNAMIC,
    ] {
        def(&cls, f)?;
    }
    cls.setattr(
        "apply",
        pybind11::method_with_docs(py, &APPLY, &APPLY_DOCS)?,
    )?;
    cls.setattr(
        "applyRGB",
        pybind11::method_with_docs(py, &APPLY_RGB, &APPLY_RGB_DOCS)?,
    )?;
    cls.setattr(
        "applyRGBA",
        pybind11::method_with_docs(py, &APPLY_RGBA, &APPLY_RGBA_DOCS)?,
    )?;
    Ok(())
}
