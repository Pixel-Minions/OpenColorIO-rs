// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What `src/bindings/python/PyUtils.h` @ v2.5.2 gives every binding: the iterators'
//! `PyIterator` (PyUtils.h:40-61), and the buffers' checks and types (PyUtils.cpp, chunk 6.5a).

use std::any::Any;
use std::ffi::{c_long, c_ulong};
use std::sync::Mutex;

use numpy::{PyArray1, PyArrayDescr, PyArrayDescrMethods};
use ocio_ops::open_color_types::{BitDepth, bit_depth_to_string};
use ocio_py_ffi::BufferInfo;
use pyo3::prelude::*;

use crate::dispatch::{Cast, std_string_to_py};
use crate::py_open_color_io::to_py_err;
use crate::pybind11;

/// The C++ value of an iterator class's instance: `PyIterator<T, UNIQUE, ARGS...>`, its object
/// (`m_obj`, a holder or null) and its next index (`m_i`).
#[pyclass(frozen, module = "pybind11_builtins", name = "_py_iterator")]
pub(crate) struct PyIterator {
    /// `m_obj`: what the iterator walks, as its class's binding holds it.
    obj: Option<Box<dyn Any + Send + Sync>>,
    /// `m_i`.
    index: Mutex<i32>,
    /// The class and method that made it of `None` as `self` (U-134): `m_obj` is an empty
    /// `shared_ptr` the wheel's methods read through.
    null_self: Option<(&'static str, &'static str)>,
}

impl PyIterator {
    /// `m_obj`, as the type `T` its class holds; for an iterator made of `None` as `self`,
    /// the port's error where the wheel reads through the null pointer (U-134).
    pub(crate) fn obj<T: 'static>(&self) -> PyResult<&T> {
        if let (None, Some((class, name))) = (&self.obj, self.null_self) {
            return Err(crate::self_kind::null_iterator(class, name));
        }
        self.obj
            .as_ref()
            .and_then(|o| o.downcast_ref::<T>())
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("an iterator of another class")
            })
    }

    /// Port of `PyIterator::nextIndex` (PyUtils.h:44-48 @ v2.5.2): the next index, or
    /// `StopIteration` (`py::stop_iteration()`, whose message is empty) from `num` on.
    pub(crate) fn next_index(&self, num: i32) -> PyResult<i32> {
        let mut i = self
            .index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *i >= num {
            return Err(pyo3::exceptions::PyStopIteration::new_err(""));
        }
        let at = *i;
        *i += 1;
        Ok(at)
    }
}

/// Port of `PyIterator::checkIndex` (PyUtils.h:50-53 @ v2.5.2): `IndexError` "Iterator index
/// out of range" for an index from `num` on (a negative index passes).
pub(crate) fn check_index(i: i32, num: i32) -> PyResult<()> {
    if i >= num {
        return Err(pyo3::exceptions::PyIndexError::new_err(
            "Iterator index out of range",
        ));
    }
    Ok(())
}

/// A new instance of the iterator class that signatures write `qualified`, over `obj` (`None`: a
/// null object), from its first index: a returned `PyIterator` (pybind11 moves it into a new
/// instance).
pub(crate) fn new_iterator<'py>(
    py: Python<'py>,
    qualified: &str,
    obj: Option<Box<dyn Any + Send + Sync>>,
) -> PyResult<Bound<'py, PyAny>> {
    let cls = pybind11::class(py, qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified.to_string()))?;
    let instance = cls.call_method1("__new__", (&cls,))?;
    let value = PyIterator {
        obj,
        index: Mutex::new(0),
        null_self: None,
    };
    pybind11::set_value(&instance, qualified, Bound::new(py, value)?.into_any())?;
    Ok(instance)
}

/// The iterator `class.name` returns for `None` as `self` (an unnamed overload taking the
/// class's `shared_ptr`, U-134): an instance of the iterator class `qualified` whose `m_obj` is
/// null, which the wheel's `__iter__` returns and whose other methods read through it.
pub(crate) fn new_null_iterator<'py>(
    py: Python<'py>,
    qualified: &str,
    class: &'static str,
    name: &'static str,
) -> PyResult<Bound<'py, PyAny>> {
    let cls = pybind11::class(py, qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified.to_string()))?;
    let instance = cls.call_method1("__new__", (&cls,))?;
    let value = PyIterator {
        obj: None,
        index: Mutex::new(0),
        null_self: Some((class, name)),
    };
    pybind11::set_value(&instance, qualified, Bound::new(py, value)?.into_any())?;
    Ok(instance)
}

/// The iterator value `obj` holds as an instance of the iterator class `qualified` (`Ok(None)`:
/// not one, the next overload).
pub(crate) fn iterator<'py>(
    obj: &Bound<'py, PyAny>,
    qualified: &str,
) -> PyResult<Option<Bound<'py, PyIterator>>> {
    let Some(this) = pybind11::instance_of(obj, qualified) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, qualified)?;
    Ok(Some(value.cast_into::<PyIterator>()?))
}

/// The iterator of `self` (an overload's first parameter, whose type names its class), or the
/// next overload.
#[macro_export]
macro_rules! this_iter {
    ($o:expr, $a:expr) => {
        match $crate::py_utils::iterator(&$a[0], $o.params[0].ty)? {
            Some(it) => it,
            None => return Ok(None),
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Buffers (PyUtils.h:63-93, PyUtils.cpp:9-207 @ v2.5.2, chunk 6.5a)

/// The type pybind11's signatures write for a `py::buffer` parameter.
pub(crate) const BUFFER: &str = "collections.abc.Buffer";

/// A `py::buffer` parameter (pybind11's `pyobject_caster<buffer>`): an object whose type exports
/// buffers (`PyObject_CheckBuffer`), as it is, with or without conversions; nothing else.
#[derive(Debug, Clone)]
pub(crate) struct BufferArg<'py>(pub(crate) Bound<'py, PyAny>);

impl<'py> Cast<'py> for BufferArg<'py> {
    /// `py::buffer`: a pointer.
    const CPP_SIZE: usize = 8;

    fn load(obj: &Bound<'py, PyAny>, _convert: bool) -> Option<Self> {
        ocio_py_ffi::check_buffer(obj).then(|| BufferArg(obj.clone()))
    }
}

/// `buffer_info::size`: the product of the shape (1 without dimensions), wrapping as the wheels'
/// `ssize_t` does.
pub(crate) fn buffer_size(info: &BufferInfo) -> isize {
    info.shape
        .iter()
        .fold(1isize, |size, &n| size.wrapping_mul(n))
}

/// A `std::runtime_error` of `message` as pybind11 raises it: RuntimeError, its text decoded as
/// strict UTF-8 (`PyErr_SetString`; UnicodeDecodeError instead when it isn't UTF-8).
pub(crate) fn runtime_error(py: Python<'_>, message: &[u8]) -> PyErr {
    match std_string_to_py(py, message) {
        Ok(text) => pyo3::exceptions::PyRuntimeError::new_err((text.unbind(),)),
        Err(e) => e,
    }
}

/// An OCIO `Exception` of `message`, as the module raises it.
pub(crate) fn ocio_error(py: Python<'_>, message: impl Into<Vec<u8>>) -> PyErr {
    to_py_err(py, &ocio::Exception::new(message))
}

/// Port of the format lists (PyUtils.cpp:20-22 @ v2.5.2).
const UINT_FORMATS: [&[u8]; 6] = [b"B", b"H", b"I", b"L", b"Q", b"N"];
const INT_FORMATS: [&[u8]; 6] = [b"b", b"h", b"i", b"l", b"q", b"n"];
const FLOAT_FORMATS: [&[u8]; 8] = [b"e", b"f", b"d", b"g", b"Ze", b"Zf", b"Zd", b"Zg"];

/// Port of `formatCodeToDtypeName` (PyUtils.cpp:26-54 @ v2.5.2): the NumPy name of a buffer
/// format of `num_bits` bits per item, or the format itself with the bits for one it doesn't
/// know (`'<f' (32-bit)`).
pub(crate) fn format_code_to_dtype_name(format: &[u8], num_bits: isize) -> Vec<u8> {
    if FLOAT_FORMATS.contains(&format) {
        format!("float{num_bits}").into_bytes()
    } else if UINT_FORMATS.contains(&format) {
        format!("uint{num_bits}").into_bytes()
    } else if INT_FORMATS.contains(&format) {
        format!("int{num_bits}").into_bytes()
    } else {
        let mut os = b"'".to_vec();
        os.extend_from_slice(format);
        os.extend_from_slice(format!("' ({num_bits}-bit)").as_bytes());
        os
    }
}

/// The library's `BitDepth` of the C++ enum value `v`, if it is one of its enumerators.
pub(crate) fn bit_depth_of(v: i64) -> Option<BitDepth> {
    use BitDepth::*;
    [
        Unknown, Uint8, Uint10, Uint12, Uint14, Uint16, Uint32, F16, F32,
    ]
    .into_iter()
    .find(|b| *b as i64 == v)
}

/// `BitDepthToString` (src/OpenColorIO/ParseUtils.cpp:171-182 @ v2.5.2) of the C++ enum value
/// `v`: "unknown" for a value outside the enumerators too.
fn bit_depth_name(v: i64) -> &'static str {
    bit_depth_of(v).map_or("unknown", bit_depth_to_string)
}

/// The NumPy type name of the bit depths `bitDepthToDtype` takes, and its error for the others.
fn dtype_name(py: Python<'_>, bit_depth: i64) -> PyResult<&'static str> {
    match bit_depth_of(bit_depth) {
        Some(BitDepth::Uint8) => Ok("uint8"),
        Some(BitDepth::Uint10 | BitDepth::Uint12 | BitDepth::Uint16) => Ok("uint16"),
        Some(BitDepth::F16) => Ok("float16"),
        Some(BitDepth::F32) => Ok("float32"),
        _ => Err(ocio_error(
            py,
            format!(
                "Error: Unsupported bit-depth: {}",
                bit_depth_name(bit_depth)
            ),
        )),
    }
}

/// pybind11's `py::dtype(name)`: NumPy's type `name` (`PyArray_DescrConverter`).
pub(crate) fn dtype<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyArrayDescr>> {
    PyArrayDescr::new(py, name)
}

/// Port of `bitDepthToDtype` (PyUtils.cpp:56-86 @ v2.5.2): the NumPy type of a bit depth's
/// channels (of the C++ enum value `bit_depth`), or OCIO's "Error: Unsupported bit-depth: ...".
pub(crate) fn bit_depth_to_dtype<'py>(
    py: Python<'py>,
    bit_depth: i64,
) -> PyResult<Bound<'py, PyArrayDescr>> {
    let name = dtype_name(py, bit_depth)?;
    dtype(py, name)
}

/// Port of `bitDepthToBytes` (PyUtils.cpp:88-111 @ v2.5.2).
pub(crate) fn bit_depth_to_bytes(py: Python<'_>, bit_depth: i64) -> PyResult<isize> {
    match bit_depth_of(bit_depth) {
        Some(BitDepth::Uint8) => Ok(1),
        Some(BitDepth::Uint10 | BitDepth::Uint12 | BitDepth::Uint16 | BitDepth::F16) => Ok(2),
        Some(BitDepth::F32) => Ok(4),
        _ => Err(ocio_error(
            py,
            format!(
                "Error: Unsupported bit-depth: {}",
                bit_depth_name(bit_depth)
            ),
        )),
    }
}

/// Port of `chanOrderToNumChannels` (PyUtils.cpp:113-127 @ v2.5.2), of the C++ enum value
/// `chan_order`.
pub(crate) fn chan_order_to_num_channels(py: Python<'_>, chan_order: i64) -> PyResult<c_long> {
    match chan_order {
        // CHANNEL_ORDERING_RGBA, _BGRA, _ABGR.
        0..=2 => Ok(4),
        // CHANNEL_ORDERING_RGB, _BGR.
        3 | 4 => Ok(3),
        _ => Err(ocio_error(py, "Error: Unsupported channel ordering")),
    }
}

/// Port of `getBufferShapeStr` (PyUtils.cpp:129-139 @ v2.5.2): `(2, 3)`.
pub(crate) fn buffer_shape_str(info: &BufferInfo) -> String {
    let shape: Vec<String> = info.shape.iter().map(isize::to_string).collect();
    format!("({})", shape.join(", "))
}

/// Port of `getBufferBitDepth` (PyUtils.cpp:141-163 @ v2.5.2): the bit depth of a buffer's
/// format, or RuntimeError "Unsupported data type: ...".
pub(crate) fn buffer_bit_depth(py: Python<'_>, info: &BufferInfo) -> PyResult<BitDepth> {
    let dt_name = format_code_to_dtype_name(&info.format, info.itemsize.wrapping_mul(8));
    match dt_name.as_slice() {
        b"float32" => Ok(BitDepth::F32),
        b"float16" => Ok(BitDepth::F16),
        b"uint16" | b"uint12" | b"uint10" => Ok(BitDepth::Uint16),
        b"uint8" => Ok(BitDepth::Uint8),
        _ => {
            let mut os = b"Unsupported data type: ".to_vec();
            os.extend_from_slice(&dt_name);
            Err(runtime_error(py, &os))
        }
    }
}

/// pybind11's `import_numpy_core_submodule(name)`: `numpy._core.<name>` (`numpy.core.<name>`
/// before NumPy 2), by the major version `numpy.lib.NumpyVersion` reads.
fn numpy_core_submodule<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    let numpy = py.import("numpy")?;
    let version = numpy.getattr("__version__")?;
    let numpy_lib = py.import("numpy.lib")?;
    let major: i32 = numpy_lib
        .getattr("NumpyVersion")?
        .call1((version,))?
        .getattr("major")?
        .extract()?;
    let core = if major >= 2 {
        "numpy._core"
    } else {
        "numpy.core"
    };
    py.import(format!("{core}.{name}"))
}

/// pybind11's `py::dtype(info)`: NumPy's `_internal._dtype_from_pep3118(str(info.format))`, then
/// `strip_padding`, which returns a type without fields as it is. A type with fields (a
/// structured format) becomes a new type made from its fields, never one of NumPy's own, which
/// [`check_buffer_type`] only compares by identity: it is left as it is here.
fn dtype_of_buffer<'py>(py: Python<'py>, info: &BufferInfo) -> PyResult<Bound<'py, PyAny>> {
    // pybind11::str(info.format): "Could not allocate string object!" when it isn't UTF-8.
    let format = std_string_to_py(py, &info.format)
        .map_err(|_| runtime_error(py, b"Could not allocate string object!"))?;
    numpy_core_submodule(py, "_internal")?
        .getattr("_dtype_from_pep3118")?
        .call1((format,))
}

/// Port of `checkBufferType(info, dt)` (PyUtils.cpp:165-175 @ v2.5.2): RuntimeError
/// "Incompatible buffer format: expected ..., but received ..." unless the buffer's NumPy type
/// is `dt` itself (pybind11's `is`). The expected type is named by its kind character, which
/// `formatCodeToDtypeName` names only for floats (`'u' (16-bit)`, I-23).
pub(crate) fn check_buffer_type(
    py: Python<'_>,
    info: &BufferInfo,
    dt: &Bound<'_, PyArrayDescr>,
) -> PyResult<()> {
    if !dtype_of_buffer(py, info)?.is(dt) {
        let mut os = b"Incompatible buffer format: expected ".to_vec();
        os.extend(format_code_to_dtype_name(
            &[dt.kind()],
            (dt.itemsize() as isize).wrapping_mul(8),
        ));
        os.extend_from_slice(b", but received ");
        os.extend(format_code_to_dtype_name(
            &info.format,
            info.itemsize.wrapping_mul(8),
        ));
        return Err(runtime_error(py, &os));
    }
    Ok(())
}

/// Port of `checkBufferType(info, bitDepth)` (PyUtils.cpp:177-180 @ v2.5.2).
pub(crate) fn check_buffer_type_of(
    py: Python<'_>,
    info: &BufferInfo,
    bit_depth: i64,
) -> PyResult<()> {
    let dt = bit_depth_to_dtype(py, bit_depth)?;
    check_buffer_type(py, info, &dt)
}

/// Port of `checkCContiguousArray` (PyUtils.cpp:182-201 @ v2.5.2).
pub(crate) fn check_c_contiguous_array(py: Python<'_>, info: &BufferInfo) -> PyResult<()> {
    let mut expected = info.itemsize;
    for (&n, &stride) in info.shape.iter().zip(&info.strides).rev() {
        if stride != expected {
            return Err(runtime_error(
                py,
                b"function only supports C-contiguous (row-major) arrays",
            ));
        }
        expected = expected.wrapping_mul(n);
    }
    Ok(())
}

/// Port of `checkBufferDivisible` (PyUtils.cpp:203-212 @ v2.5.2).
pub(crate) fn check_buffer_divisible(
    py: Python<'_>,
    info: &BufferInfo,
    num_channels: isize,
) -> PyResult<()> {
    let size = buffer_size(info);
    if size % num_channels != 0 {
        return Err(runtime_error(
            py,
            format!(
                "Incompatible buffer dimensions: expected size to be divisible by \
                 {num_channels}, but received {size} entries"
            )
            .as_bytes(),
        ));
    }
    Ok(())
}

/// Port of `checkBufferSize` (PyUtils.cpp:214-223 @ v2.5.2).
pub(crate) fn check_buffer_size(
    py: Python<'_>,
    info: &BufferInfo,
    num_entries: isize,
) -> PyResult<()> {
    let size = buffer_size(info);
    if size != num_entries {
        return Err(runtime_error(
            py,
            format!(
                "Incompatible buffer dimensions: expected {num_entries} entries, but received \
                 {size} entries"
            )
            .as_bytes(),
        ));
    }
    Ok(())
}

/// Port of `getBufferLut3DGridSize` (PyUtils.cpp:225-250 @ v2.5.2): the grid size of a packed 3D
/// LUT's buffer, in C++'s `unsigned long` (32 bits in the Windows wheel, I-326), whose products
/// wrap.
pub(crate) fn buffer_lut3d_grid_size(py: Python<'_>, info: &BufferInfo) -> PyResult<c_ulong> {
    check_buffer_divisible(py, info, 3)?;

    let size_ssize = buffer_size(info);
    let mut gs: c_ulong = 2;
    let size: c_ulong = if size_ssize >= 0 {
        size_ssize as c_ulong
    } else {
        0
    };

    if info.ndim == 1 {
        // std::cbrt of an unsigned long: of the double, as the C runtime computes it.
        gs = ((size / 3) as f64).cbrt().round() as c_ulong;
    } else if info.ndim > 1 {
        gs = if size_ssize >= 0 {
            info.shape[0] as c_ulong
        } else {
            0
        };
    }

    if gs.wrapping_mul(gs).wrapping_mul(gs).wrapping_mul(3) != size {
        return Err(runtime_error(
            py,
            format!(
                "Incompatible buffer dimensions: failed to calculate grid size from shape {}",
                buffer_shape_str(info)
            )
            .as_bytes(),
        ));
    }

    Ok(gs)
}

/// Port of `checkVectorDivisible` (PyUtils.cpp:252-261 @ v2.5.2), of a vector of `len` values.
pub(crate) fn check_vector_divisible(
    py: Python<'_>,
    len: usize,
    num_channels: usize,
) -> PyResult<()> {
    if !len.is_multiple_of(num_channels) {
        return Err(runtime_error(
            py,
            format!(
                "Incompatible vector dimensions: expected (N*{num_channels}, 1), but received \
                 ({len}, 1)"
            )
            .as_bytes(),
        ));
    }
    Ok(())
}

/// `py::array(dt, {count}, {stride}, ptr)` of a bit depth's channels (`bit_depth`, one that
/// [`bit_depth_to_dtype`] takes): pybind11 copies the strided items into a new array of its own
/// (`PyArray_NewCopy`, as it has no base object), C-contiguous; `items` are their bytes, in order.
pub(crate) fn array_of<'py>(
    py: Python<'py>,
    bit_depth: BitDepth,
    items: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    fn chunks<const N: usize>(items: &[u8]) -> impl Iterator<Item = [u8; N]> + '_ {
        items.as_chunks::<N>().0.iter().copied()
    }
    Ok(match bit_depth {
        BitDepth::Uint8 => PyArray1::<u8>::from_slice(py, items).into_any(),
        BitDepth::Uint10 | BitDepth::Uint12 | BitDepth::Uint16 => {
            let v: Vec<u16> = chunks::<2>(items).map(u16::from_ne_bytes).collect();
            PyArray1::<u16>::from_slice(py, &v).into_any()
        }
        BitDepth::F16 => {
            let v: Vec<half::f16> = chunks::<2>(items)
                .map(|b| half::f16::from_bits(u16::from_ne_bytes(b)))
                .collect();
            PyArray1::<half::f16>::from_slice(py, &v).into_any()
        }
        BitDepth::F32 => {
            let v: Vec<f32> = chunks::<4>(items).map(f32::from_ne_bytes).collect();
            PyArray1::<f32>::from_slice(py, &v).into_any()
        }
        BitDepth::Unknown | BitDepth::Uint14 | BitDepth::Uint32 => {
            return Err(ocio_error(
                py,
                format!(
                    "Error: Unsupported bit-depth: {}",
                    bit_depth_to_string(bit_depth)
                ),
            ));
        }
    })
}
