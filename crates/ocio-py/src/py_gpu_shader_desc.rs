// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGpuShaderDesc` (src/bindings/python/PyGpuShaderDesc.cpp @ v2.5.2): the class
//! `GpuShaderDesc`, a `GpuShaderCreator`, and its nested classes `UniformData`, `Texture` and
//! `Texture3D` with their iterator classes (chunk 6.3h). The textures' values come in through
//! Python buffers and go out as NumPy arrays (chunk 6.5a's buffers).
//!
//! **Values.** `UniformData`, `Texture` and `Texture3D` are C++ values that pybind11 moves into
//! each new instance: indexing an iterator twice gives two objects. A `UniformData` holds the
//! uniform's getters, which read the processor's dynamic properties when called (a change of the
//! property shows in what they return); its `type` and `bufferOffset` are its own, which Python
//! may set (the getters don't follow). A `Texture` holds the description and its index, and
//! reads the values from the description when asked (`getValues`).

use numpy::PyArray1;
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType, UniformData};
use ocio_gpu::open_color_types::UniformDataType;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::dispatch::{Function, Overload, Param, StdString};
use crate::py_gpu_shader_creator::{DescRc, GPU_LANGUAGE, GPU_SHADER_CREATOR};
use crate::py_image_desc::outside;
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_transform::{
    INT, Out, class, def, def_readonly, def_readwrite, def_static, default, enum_out, nested_class,
    none, ok, outside_enumerators, string_out,
};
use crate::py_utils::{
    BUFFER, BufferArg, check_buffer_size, check_buffer_type, dtype, new_iterator,
};
use crate::shared::{Shared, construct, holder, out};
use crate::{arg, enum_param, method_def, q, static_def, this_desc, this_iter};

pub(crate) const GPU_SHADER_DESC: &str = q!("GpuShaderDesc");
const UNIFORM_DATA: &str = q!("GpuShaderDesc.UniformData");
const UNIFORM_ITERATOR: &str = q!("GpuShaderDesc.UniformIterator");
const TEXTURE: &str = q!("GpuShaderDesc.Texture");
const TEXTURE_ITERATOR: &str = q!("GpuShaderDesc.TextureIterator");
const TEXTURE_3D: &str = q!("GpuShaderDesc.Texture3D");
const TEXTURE_3D_ITERATOR: &str = q!("GpuShaderDesc.Texture3DIterator");
const TEXTURE_TYPE: &str = q!("GpuShaderCreator.TextureType");
const TEXTURE_DIMENSIONS: &str = q!("GpuShaderCreator.TextureDimensions");
const UNIFORM_DATA_TYPE: &str = q!("UniformDataType");
const INTERPOLATION: &str = q!("Interpolation");

const SELF: Param = Param::new("self", GPU_SHADER_DESC);
const UD_SELF: Param = Param::new("self", UNIFORM_DATA);
const UI_SELF: Param = Param::new("self", UNIFORM_ITERATOR);
const TX_SELF: Param = Param::new("self", TEXTURE);
const TI_SELF: Param = Param::new("self", TEXTURE_ITERATOR);
const T3_SELF: Param = Param::new("self", TEXTURE_3D);
const T3I_SELF: Param = Param::new("self", TEXTURE_3D_ITERATOR);

// ---------------------------------------------------------------------------------------------
// The values

/// `GpuShaderDesc::UniformData` (include/OpenColorIO/OpenColorIO.h:3748-3765 @ v2.5.2): its type
/// as the C++ enum holds it (Python may set any value of the enum class, I-223), its buffer
/// offset (a `std::size_t`), and its getters, those of one type or none (a default-constructed
/// one's, whose calls throw `std::bad_function_call`).
#[derive(Debug, Clone)]
pub(crate) struct UniformValue {
    ty: i64,
    buffer_offset: u64,
    data: Option<UniformData>,
}

impl Default for UniformValue {
    /// `UniformData{}`: `UNIFORM_UNKNOWN`, offset 0, no getter.
    fn default() -> Self {
        UniformValue {
            ty: UniformDataType::Unknown as i64,
            buffer_offset: 0,
            data: None,
        }
    }
}

/// The `Texture` struct of PyGpuShaderDesc.cpp:23-35 @ v2.5.2: what `getTexture` gave, and
/// the description and index `getValues` reads from.
#[derive(Debug, Clone)]
struct TextureValue {
    texture_name: Vec<u8>,
    sampler_name: Vec<u8>,
    width: u32,
    height: u32,
    channel: i64,
    dimensions: i64,
    interpolation: i64,
    binding_index: u32,
    desc: DescRc,
    index: i32,
}

/// The `Texture3D` struct of PyGpuShaderDesc.cpp:37-46 @ v2.5.2.
#[derive(Debug, Clone)]
struct Texture3DValue {
    texture_name: Vec<u8>,
    sampler_name: Vec<u8>,
    edge_len: u32,
    interpolation: i64,
    binding_index: u32,
    desc: DescRc,
    index: i32,
}

/// The value of `self`, an instance of the class its parameter's type names, or the next
/// overload.
macro_rules! this_value {
    ($o:expr, $a:expr, $t:ty) => {
        $crate::this_shared!($o, $a, $t)
    };
}

/// The library's `TextureDimensions` of the C++ value `v`: the port's error outside its
/// enumerators, which the wheel stores (docs/improvements.md, U-112).
fn texture_dimensions(v: i64) -> PyResult<TextureDimensions> {
    match v {
        1 => Ok(TextureDimensions::D1),
        2 => Ok(TextureDimensions::D2),
        _ => Err(outside_enumerators(TEXTURE_DIMENSIONS, v)),
    }
}

/// The library's `Interpolation` of the C++ value `v`: the port's error outside its
/// enumerators, which the wheel stores (docs/improvements.md, U-112).
fn interpolation(v: i64) -> PyResult<Interpolation> {
    crate::transforms::py_file_transform::interpolation(v)
        .ok_or_else(|| outside_enumerators(INTERPOLATION, v))
}

/// The number of floats of a texel of the texture type `channel`, or upstream's error
/// (PyGpuShaderDesc.cpp:128-138, 267-277 @ v2.5.2).
fn num_channels(py: Python<'_>, channel: i64) -> PyResult<(isize, TextureType)> {
    match channel {
        0 => Ok((1, TextureType::RedChannel)),
        1 => Ok((3, TextureType::RgbChannel)),
        _ => Err(to_py_err(
            py,
            &ocio::Exception::new("Error: Unsupported texture type"),
        )),
    }
}

/// Whether `e` is the library's error for values shorter than the texture (U-11), which the
/// binding gets when it hands the library no values: the wheel reads them past the buffer.
fn short_values(e: &ocio::Exception) -> bool {
    let m = e.to_string();
    m.starts_with("The texture '") && m.contains("' needs ") && m.ends_with(" were given.")
}

/// The texture's `count` floats read one after another from the buffer's first item (its
/// strides aside, I-328), in the regions `with_bytes` lent: `None` when they reach past the
/// buffer's items (U-148).
fn floats(
    regions: &[ocio_py_ffi::Bytes<'_>],
    places: &[(usize, usize)],
    count: usize,
) -> Option<Vec<f32>> {
    let bytes: &[u8] = match &regions[0] {
        ocio_py_ffi::Bytes::Read(b) => b,
        ocio_py_ffi::Bytes::Write(b) => b,
    };
    let first = places[0].1;
    let end = first.checked_add(count.checked_mul(4)?)?;
    let span = bytes.get(first..end)?;
    Some(
        span.as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_ne_bytes(*c))
            .collect(),
    )
}

/// The library's call adding a texture of `count` floats from the buffer `buffer` (lent to
/// read, the GIL released): the values read as the wheel reads them; where they'd reach past
/// the buffer, the library's own checks first (it is handed no values, so that its error for
/// the values, U-11, is the port's U-148 error, `call`).
fn add_with_values(
    py: Python<'_>,
    buffer: &ocio_py_ffi::Buffer,
    count: usize,
    call: &'static str,
    add: impl FnOnce(&[f32]) -> ocio::Result<u32> + Send,
) -> PyResult<u32> {
    let done = ocio_py_ffi::with_bytes(py, &[(buffer, false)], move |regions, places| {
        match floats(&regions, &places, count) {
            Some(values) => add(&values).map_err(Some),
            None => match add(&[]) {
                Err(e) if !short_values(&e) => Err(Some(e)),
                // Unreachable: no values for a texture of some.
                _ => Err(None),
            },
        }
    })?;
    match done {
        Ok(index) => Ok(index),
        Err(Some(e)) => Err(to_py_err(py, &e)),
        Err(None) => Err(outside(call)),
    }
}

// ---------------------------------------------------------------------------------------------
// GpuShaderDesc

fn default_language(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    crate::enums::to_py(
        &crate::enums::enum_class(py, GPU_LANGUAGE)?,
        GpuLanguage::Glsl1_2 as i64,
    )
}

fn string_default<'py>(py: Python<'py>, s: &str) -> PyResult<Bound<'py, PyAny>> {
    Ok(pyo3::types::PyString::new(py, s).into_any())
}

// .def_static("CreateShaderDesc", [](GpuLanguage lang, const std::string & functionName, const
// std::string & pixelName, const std::string & resourcePrefix, const std::string & uid) {...},
// "language"_a = DEFAULT->getLanguage(), "functionName"_a = DEFAULT->getFunctionName(),
// "pixelName"_a = DEFAULT->getPixelName(), "resourcePrefix"_a = DEFAULT->getResourcePrefix(),
// "uid"_a = DEFAULT->getUniqueID()) (PyGpuShaderDesc.cpp:83-102 @ v2.5.2): a new description,
// its language set, and each name that isn't empty.
static CREATE_SHADER_DESC: Function = static_def!(
    "CreateShaderDesc",
    [
        Param {
            name: "language",
            ty: GPU_LANGUAGE,
            default: default("<GpuLanguage.GPU_LANGUAGE_GLSL_1_2: 1>", default_language),
        },
        Param {
            name: "functionName",
            ty: "str",
            default: default("'OCIOMain'", |py| string_default(py, "OCIOMain")),
        },
        Param {
            name: "pixelName",
            ty: "str",
            default: default("'outColor'", |py| string_default(py, "outColor")),
        },
        Param {
            name: "resourcePrefix",
            ty: "str",
            default: default("'ocio'", |py| string_default(py, "ocio")),
        },
        Param {
            name: "uid",
            ty: "str",
            default: default("''", |py| string_default(py, "")),
        }
    ],
    GPU_SHADER_DESC,
    |py, o, a, convert| {
        let lang = enum_param!(o, a, 0);
        let StdString(function_name) = arg!(a, 1, convert, StdString);
        let StdString(pixel_name) = arg!(a, 2, convert, StdString);
        let StdString(resource_prefix) = arg!(a, 3, convert, StdString);
        let StdString(uid) = arg!(a, 4, convert, StdString);
        let lang = GpuLanguage::ALL
            .into_iter()
            .find(|l| *l as i64 == lang)
            .ok_or_else(|| outside_enumerators(GPU_LANGUAGE, lang))?;
        let mut p = GpuShaderDesc::default();
        p.set_language(lang);
        if !function_name.is_empty() {
            p.set_function_name(function_name);
        }
        if !pixel_name.is_empty() {
            p.set_pixel_name(pixel_name);
        }
        if !resource_prefix.is_empty() {
            p.set_resource_prefix(resource_prefix);
        }
        if !uid.is_empty() {
            p.set_unique_id(uid);
        }
        out(py, GPU_SHADER_DESC, &Shared::new(p))
    }
);

// .def("clone", &GpuShaderDesc::clone) (PyGpuShaderDesc.cpp:104-105 @ v2.5.2): a
// `GpuShaderCreatorRcPtr`, made a `GpuShaderDesc`.
static CLONE: Function = method_def!("clone", [SELF], GPU_SHADER_CREATOR, |py, o, a, _| {
    let rc = this_desc!(o, a);
    let copy = rc.lock().clone_desc();
    out(py, GPU_SHADER_DESC, &Shared::new(copy))
});

// .def("getShaderText", &GpuShaderDesc::getShaderText) (PyGpuShaderDesc.cpp:106-107 @ v2.5.2).
static GET_SHADER_TEXT: Function = method_def!("getShaderText", [SELF], "str", |py, o, a, _| {
    let rc = this_desc!(o, a);
    let text = rc.lock().shader_text().to_vec();
    string_out(py, &text)
});

// .def("getUniforms", [](GpuShaderDescRcPtr & self) { return UniformIterator(self); })
// (PyGpuShaderDesc.cpp:108-111 @ v2.5.2).
static GET_UNIFORMS: Function =
    method_def!("getUniforms", [SELF], UNIFORM_ITERATOR, |py, o, a, _| {
        let rc = this_desc!(o, a);
        Ok(Some(
            new_iterator(py, UNIFORM_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    });

// .def("getUniformBufferSize", &GpuShaderDesc::getUniformBufferSize) (PyGpuShaderDesc.cpp:
// 112-113 @ v2.5.2).
static GET_UNIFORM_BUFFER_SIZE: Function =
    method_def!("getUniformBufferSize", [SELF], "int", |py, o, a, _| {
        let rc = this_desc!(o, a);
        let size = rc.lock().uniform_buffer_size();
        ok(py, size)
    });

// .def("addTexture", [](GpuShaderDescRcPtr & self, const std::string & textureName, const
// std::string & samplerName, unsigned width, unsigned height, GpuShaderDesc::TextureType
// channel, GpuShaderDesc::TextureDimensions dimensions, Interpolation interpolation, const
// py::buffer & values) {...}, "textureName"_a, "samplerName"_a, "width"_a, "height"_a,
// "channel"_a, "dimensions"_a, "interpolation"_a, "values"_a) (PyGpuShaderDesc.cpp:116-155 @
// v2.5.2): the buffer requested, the channel's count of floats (or upstream's error), the
// buffer's type and size checked (`width*height` in `unsigned`, times the count in
// `py::ssize_t`); then, the GIL released, the library reads the texture's floats one after
// another from the buffer's first item (I-328; past the buffer's items, the port's error,
// U-148). A dimension or interpolation outside its enumerators, which the wheel stores, is the
// port's error (U-112), after the binding's checks.
static ADD_TEXTURE: Function = method_def!(
    "addTexture",
    [
        SELF,
        Param::new("textureName", "str"),
        Param::new("samplerName", "str"),
        Param::new("width", INT),
        Param::new("height", INT),
        Param::new("channel", TEXTURE_TYPE),
        Param::new("dimensions", TEXTURE_DIMENSIONS),
        Param::new("interpolation", INTERPOLATION),
        Param::new("values", BUFFER)
    ],
    "int",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let StdString(texture_name) = arg!(a, 1, convert, StdString);
        let StdString(sampler_name) = arg!(a, 2, convert, StdString);
        let width = arg!(a, 3, convert, u32);
        let height = arg!(a, 4, convert, u32);
        let channel = enum_param!(o, a, 5);
        let dimensions = enum_param!(o, a, 6);
        let interp = enum_param!(o, a, 7);
        let values = arg!(a, 8, convert, BufferArg).0;
        let buffer = ocio_py_ffi::Buffer::request(&values)?;
        let (num_channels, channel) = num_channels(py, channel)?;
        check_buffer_type(py, buffer.info(), &dtype(py, "float32")?)?;
        check_buffer_size(
            py,
            buffer.info(),
            (width.wrapping_mul(height) as isize).wrapping_mul(num_channels),
        )?;
        let dimensions = texture_dimensions(dimensions)?;
        let interp = interpolation(interp)?;
        // `width * height * 1 * channels`, in `unsigned` (GpuShader.cpp:67-108 @ v2.5.2).
        let count = width.wrapping_mul(height).wrapping_mul(num_channels as u32) as usize;
        let index = add_with_values(py, &buffer, count, "GpuShaderDesc.addTexture", |values| {
            rc.lock().add_texture(
                &texture_name,
                &sampler_name,
                width,
                height,
                channel,
                dimensions,
                interp,
                values,
            )
        })?;
        ok(py, index)
    }
);

// .def("getTextures", [](GpuShaderDescRcPtr & self) { return TextureIterator(self); })
// (PyGpuShaderDesc.cpp:157-160 @ v2.5.2).
static GET_TEXTURES: Function =
    method_def!("getTextures", [SELF], TEXTURE_ITERATOR, |py, o, a, _| {
        let rc = this_desc!(o, a);
        Ok(Some(
            new_iterator(py, TEXTURE_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    });

// .def("add3DTexture", [](GpuShaderDescRcPtr & self, const std::string & textureName, const
// std::string & samplerName, unsigned edgelen, Interpolation interpolation, const py::buffer &
// values) {...}, "textureName"_a, "samplerName"_a, "edgeLen"_a, "interpolation"_a, "values"_a)
// (PyGpuShaderDesc.cpp:163-183 @ v2.5.2): the buffer's type and size checked
// (`edgelen*edgelen*edgelen*3` in `unsigned`), then as `addTexture`.
static ADD_3D_TEXTURE: Function = method_def!(
    "add3DTexture",
    [
        SELF,
        Param::new("textureName", "str"),
        Param::new("samplerName", "str"),
        Param::new("edgeLen", INT),
        Param::new("interpolation", INTERPOLATION),
        Param::new("values", BUFFER)
    ],
    "int",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let StdString(texture_name) = arg!(a, 1, convert, StdString);
        let StdString(sampler_name) = arg!(a, 2, convert, StdString);
        let edgelen = arg!(a, 3, convert, u32);
        let interp = enum_param!(o, a, 4);
        let values = arg!(a, 5, convert, BufferArg).0;
        let buffer = ocio_py_ffi::Buffer::request(&values)?;
        check_buffer_type(py, buffer.info(), &dtype(py, "float32")?)?;
        let count = edgelen
            .wrapping_mul(edgelen)
            .wrapping_mul(edgelen)
            .wrapping_mul(3);
        check_buffer_size(py, buffer.info(), count as isize)?;
        let interp = interpolation(interp)?;
        let index = add_with_values(
            py,
            &buffer,
            count as usize,
            "GpuShaderDesc.add3DTexture",
            |values| {
                rc.lock()
                    .add_3d_texture(&texture_name, &sampler_name, edgelen, interp, values)
            },
        )?;
        ok(py, index)
    }
);

// .def("get3DTextures", [](GpuShaderDescRcPtr & self) { return Texture3DIterator(self); })
// (PyGpuShaderDesc.cpp:184-187 @ v2.5.2).
static GET_3D_TEXTURES: Function = method_def!(
    "get3DTextures",
    [SELF],
    TEXTURE_3D_ITERATOR,
    |py, o, a, _| {
        let rc = this_desc!(o, a);
        Ok(Some(
            new_iterator(py, TEXTURE_3D_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

// ---------------------------------------------------------------------------------------------
// UniformData

// .def(py::init<>()), .def(py::init<const GpuShaderDesc::UniformData &>(), "data"_a)
// (PyGpuShaderDesc.cpp:190-191 @ v2.5.2).
static UD_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[UD_SELF],
            ret: "None",
            call: |py, _, a, _| {
                construct(&a[0], UNIFORM_DATA, Shared::new(UniformValue::default()))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[UD_SELF, Param::new("data", UNIFORM_DATA)],
            ret: "None",
            call: |py, _, a, _| {
                let Some(data) = holder::<UniformValue>(&a[1], UNIFORM_DATA)? else {
                    return Ok(None);
                };
                let copy = data.lock().clone();
                construct(&a[0], UNIFORM_DATA, Shared::new(copy))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def_readwrite("type", &GpuShaderDesc::UniformData::m_type) (PyGpuShaderDesc.cpp:193 @
// v2.5.2).
static UD_GET_TYPE: Function = method_def!("", [UD_SELF], UNIFORM_DATA_TYPE, |py, o, a, _| {
    let rc = this_value!(o, a, UniformValue);
    let ty = rc.lock().ty;
    enum_out(py, UNIFORM_DATA_TYPE, ty)
});

static UD_SET_TYPE: Function = method_def!(
    "",
    [UD_SELF, Param::new("arg0", UNIFORM_DATA_TYPE)],
    "None",
    |py, o, a, _| {
        let rc = this_value!(o, a, UniformValue);
        let ty = enum_param!(o, a, 1);
        rc.lock().ty = ty;
        none(py)
    }
);

// .def_readwrite("bufferOffset", &GpuShaderDesc::UniformData::m_bufferOffset)
// (PyGpuShaderDesc.cpp:194 @ v2.5.2): a `std::size_t`.
static UD_GET_BUFFER_OFFSET: Function = method_def!("", [UD_SELF], "int", |py, o, a, _| {
    let rc = this_value!(o, a, UniformValue);
    let offset = rc.lock().buffer_offset;
    ok(py, offset)
});

static UD_SET_BUFFER_OFFSET: Function = method_def!(
    "",
    [UD_SELF, Param::new("arg0", INT)],
    "None",
    |py, o, a, convert| {
        let rc = this_value!(o, a, UniformValue);
        let offset = arg!(a, 1, convert, u64);
        rc.lock().buffer_offset = offset;
        none(py)
    }
);

/// The text of `std::bad_function_call::what()`, which pybind11 raises as RuntimeError: MSVC's
/// STL's and libstdc++'s.
const BAD_FUNCTION_CALL: &str = if cfg!(windows) {
    "bad function call"
} else {
    "bad_function_call"
};

/// The error of a call of a getter the value doesn't hold (an empty `std::function`).
fn bad_function_call() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(BAD_FUNCTION_CALL)
}

// .def("getDouble", [](GpuShaderDesc::UniformData & self) -> double { return
// self.m_getDouble(); }) (PyGpuShaderDesc.cpp:196-199 @ v2.5.2).
static UD_GET_DOUBLE: Function = method_def!("getDouble", [UD_SELF], "float", |py, o, a, _| {
    let rc = this_value!(o, a, UniformValue);
    let data = rc.lock().data.clone();
    match data {
        Some(UniformData::Double(get)) => ok(py, get()),
        _ => Err(bad_function_call()),
    }
});

// .def("getBool", [](GpuShaderDesc::UniformData & self) -> bool { return self.m_getBool(); })
// (PyGpuShaderDesc.cpp:200-203 @ v2.5.2).
static UD_GET_BOOL: Function = method_def!("getBool", [UD_SELF], "bool", |py, o, a, _| {
    let rc = this_value!(o, a, UniformValue);
    let data = rc.lock().data.clone();
    match data {
        Some(UniformData::Bool(get)) => ok(py, get()),
        _ => Err(bad_function_call()),
    }
});

// .def("getFloat3", [](GpuShaderDesc::UniformData & self) -> Float3 { return
// self.m_getFloat3(); }) (PyGpuShaderDesc.cpp:204-207 @ v2.5.2).
static UD_GET_FLOAT3: Function = method_def!(
    "getFloat3",
    [UD_SELF],
    crate::float_array_out!("3"),
    |py, o, a, _| {
        let rc = this_value!(o, a, UniformValue);
        let data = rc.lock().data.clone();
        match data {
            Some(UniformData::Float3(get)) => ok(py, get().map(f64::from).to_vec()),
            _ => Err(bad_function_call()),
        }
    }
);

/// The first `size` of `values`, a vector uniform's (`py::array(dt, {size}, {stride},
/// values)`, which copies them). The library's getters give at least `size` values.
fn vector_values<T: Copy>(size: i32, values: &[T]) -> PyResult<&[T]> {
    usize::try_from(size)
        .ok()
        .and_then(|n| values.get(..n))
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "a vector uniform's size is outside its values",
            )
        })
}

// .def("getVectorFloat", [](GpuShaderDesc::UniformData & self) -> py::array { return
// py::array(py::dtype("float32"), { self.m_vectorFloat.m_getSize() }, { sizeof(float) },
// self.m_vectorFloat.m_getVector()); }) (PyGpuShaderDesc.cpp:208-214 @ v2.5.2).
static UD_GET_VECTOR_FLOAT: Function = method_def!(
    "getVectorFloat",
    [UD_SELF],
    "numpy.ndarray",
    |py, o, a, _| {
        let rc = this_value!(o, a, UniformValue);
        let data = rc.lock().data.clone();
        match data {
            Some(UniformData::VectorFloat { size, values }) => {
                let values = values();
                let values = vector_values(size(), &values)?;
                Ok(Some(
                    PyArray1::<f32>::from_slice(py, values).into_any().unbind(),
                ))
            }
            _ => Err(bad_function_call()),
        }
    }
);

// .def("getVectorInt", [](GpuShaderDesc::UniformData & self) -> py::array { return
// py::array(py::dtype("intc"), { self.m_vectorInt.m_getSize() }, { sizeof(int) },
// self.m_vectorInt.m_getVector()); }) (PyGpuShaderDesc.cpp:215-221 @ v2.5.2).
static UD_GET_VECTOR_INT: Function =
    method_def!("getVectorInt", [UD_SELF], "numpy.ndarray", |py, o, a, _| {
        let rc = this_value!(o, a, UniformValue);
        let data = rc.lock().data.clone();
        match data {
            Some(UniformData::VectorInt { size, values }) => {
                let values = values();
                let values = vector_values(size(), &values)?;
                Ok(Some(
                    PyArray1::<i32>::from_slice(py, values).into_any().unbind(),
                ))
            }
            _ => Err(bad_function_call()),
        }
    });

// ---------------------------------------------------------------------------------------------
// The iterators

/// The description an iterator of `self` walks, or the next overload.
macro_rules! iter_desc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<DescRc>()?.clone();
        (it, rc)
    }};
}

/// `getUniform(i, data)`, then `py::make_tuple(name, data)` (PyGpuShaderDesc.cpp:228-248 @
/// v2.5.2): the library's check of the index as an `unsigned`.
fn uniform_item(py: Python<'_>, rc: &DescRc, i: i32) -> Out {
    let (name, value) = {
        let desc = rc.lock();
        let u = py_result(py, desc.uniform(i as u32))?;
        let value = UniformValue {
            ty: u.data().data_type() as i64,
            buffer_offset: u.buffer_offset() as u64,
            data: Some(u.data().clone()),
        };
        (u.name().to_vec(), value)
    };
    let name = crate::dispatch::char_ptr_to_py(py, Some(&name))?;
    let data = crate::shared::to_py(py, UNIFORM_DATA, &Shared::new(value))?;
    Ok(Some(PyTuple::new(py, [name, data])?.into_any().unbind()))
}

fn num_uniforms(rc: &DescRc) -> u32 {
    rc.lock().num_uniforms()
}

// .def("__len__", ...), .def("__getitem__", ...), .def("__iter__", ...), .def("__next__", ...)
// of UniformIterator (PyGpuShaderDesc.cpp:223-248 @ v2.5.2).
static UI_LEN: Function = method_def!("__len__", [UI_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_desc!(o, a);
    ok(py, num_uniforms(&rc))
});

static UI_GETITEM: Function = method_def!(
    "__getitem__",
    [UI_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let (_, rc) = iter_desc!(o, a);
        let i = arg!(a, 1, convert, i32);
        uniform_item(py, &rc, i)
    }
);

static UI_ITER: Function = method_def!("__iter__", [UI_SELF], UNIFORM_ITERATOR, |_, o, a, _| {
    let _ = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static UI_NEXT: Function = method_def!("__next__", [UI_SELF], "tuple", |py, o, a, _| {
    let (it, rc) = iter_desc!(o, a);
    let i = it.get().next_index(num_uniforms(&rc) as i32)?;
    uniform_item(py, &rc, i)
});

/// `getTexture(i, ...)` and `getTextureShaderBindingIndex(i)`, as a `Texture`
/// (PyGpuShaderDesc.cpp:292-328 @ v2.5.2): the library's checks of the index as an `unsigned`.
fn texture_item(py: Python<'_>, rc: &DescRc, i: i32) -> Out {
    let value = {
        let desc = rc.lock();
        let t = py_result(py, desc.texture(i as u32))?;
        let binding_index = py_result(py, desc.texture_shader_binding_index(i as u32))?;
        TextureValue {
            texture_name: t.texture_name().to_vec(),
            sampler_name: t.sampler_name().to_vec(),
            width: t.width(),
            height: t.height(),
            channel: t.channel() as i64,
            dimensions: t.dimensions() as i64,
            interpolation: t.interpolation() as i64,
            binding_index,
            desc: rc.clone(),
            index: i,
        }
    };
    out(py, TEXTURE, &Shared::new(value))
}

fn num_textures(rc: &DescRc) -> u32 {
    rc.lock().num_textures()
}

// TextureIterator (PyGpuShaderDesc.cpp:287-328 @ v2.5.2).
static TI_LEN: Function = method_def!("__len__", [TI_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_desc!(o, a);
    ok(py, num_textures(&rc))
});

static TI_GETITEM: Function = method_def!(
    "__getitem__",
    [TI_SELF, Param::new("arg0", INT)],
    TEXTURE,
    |py, o, a, convert| {
        let (_, rc) = iter_desc!(o, a);
        let i = arg!(a, 1, convert, i32);
        texture_item(py, &rc, i)
    }
);

static TI_ITER: Function = method_def!("__iter__", [TI_SELF], TEXTURE_ITERATOR, |_, o, a, _| {
    let _ = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static TI_NEXT: Function = method_def!("__next__", [TI_SELF], TEXTURE, |py, o, a, _| {
    let (it, rc) = iter_desc!(o, a);
    let i = it.get().next_index(num_textures(&rc) as i32)?;
    texture_item(py, &rc, i)
});

/// `get3DTexture(i, ...)` and `get3DTextureShaderBindingIndex(i)`, as a `Texture3D`
/// (PyGpuShaderDesc.cpp:356-386 @ v2.5.2).
fn texture_3d_item(py: Python<'_>, rc: &DescRc, i: i32) -> Out {
    let value = {
        let desc = rc.lock();
        let t = py_result(py, desc.texture_3d(i as u32))?;
        let binding_index = py_result(py, desc.texture_3d_shader_binding_index(i as u32))?;
        Texture3DValue {
            texture_name: t.texture_name().to_vec(),
            sampler_name: t.sampler_name().to_vec(),
            edge_len: t.edge_len(),
            interpolation: t.interpolation() as i64,
            binding_index,
            desc: rc.clone(),
            index: i,
        }
    };
    out(py, TEXTURE_3D, &Shared::new(value))
}

fn num_textures_3d(rc: &DescRc) -> u32 {
    rc.lock().num_textures_3d()
}

// Texture3DIterator (PyGpuShaderDesc.cpp:351-386 @ v2.5.2).
static T3I_LEN: Function = method_def!("__len__", [T3I_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_desc!(o, a);
    ok(py, num_textures_3d(&rc))
});

static T3I_GETITEM: Function = method_def!(
    "__getitem__",
    [T3I_SELF, Param::new("arg0", INT)],
    TEXTURE_3D,
    |py, o, a, convert| {
        let (_, rc) = iter_desc!(o, a);
        let i = arg!(a, 1, convert, i32);
        texture_3d_item(py, &rc, i)
    }
);

static T3I_ITER: Function =
    method_def!("__iter__", [T3I_SELF], TEXTURE_3D_ITERATOR, |_, o, a, _| {
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    });

static T3I_NEXT: Function = method_def!("__next__", [T3I_SELF], TEXTURE_3D, |py, o, a, _| {
    let (it, rc) = iter_desc!(o, a);
    let i = it.get().next_index(num_textures_3d(&rc) as i32)?;
    texture_3d_item(py, &rc, i)
});

// ---------------------------------------------------------------------------------------------
// Texture and Texture3D

/// A `def_readonly` getter of a member of `$t`, as the static `$f`.
macro_rules! readonly {
    ($f:ident, $self:expr, $t:ty, $ret:expr, |$py:ident, $v:ident| $body:expr) => {
        static $f: Function = method_def!("", [$self], $ret, |$py, o, a, _| {
            let rc = this_value!(o, a, $t);
            let $v = rc.lock().clone();
            $body
        });
    };
}

// .def_readonly(...) of Texture (PyGpuShaderDesc.cpp:251-258 @ v2.5.2).
readonly!(TX_TEXTURE_NAME, TX_SELF, TextureValue, "str", |py, v| {
    string_out(py, &v.texture_name)
});
readonly!(TX_SAMPLER_NAME, TX_SELF, TextureValue, "str", |py, v| {
    string_out(py, &v.sampler_name)
});
readonly!(TX_WIDTH, TX_SELF, TextureValue, "int", |py, v| ok(
    py, v.width
));
readonly!(TX_HEIGHT, TX_SELF, TextureValue, "int", |py, v| ok(
    py, v.height
));
readonly!(TX_CHANNEL, TX_SELF, TextureValue, TEXTURE_TYPE, |py, v| {
    enum_out(py, TEXTURE_TYPE, v.channel)
});
readonly!(
    TX_DIMENSIONS,
    TX_SELF,
    TextureValue,
    TEXTURE_DIMENSIONS,
    |py, v| enum_out(py, TEXTURE_DIMENSIONS, v.dimensions)
);
readonly!(
    TX_INTERPOLATION,
    TX_SELF,
    TextureValue,
    INTERPOLATION,
    |py, v| enum_out(py, INTERPOLATION, v.interpolation)
);
readonly!(TX_BINDING_INDEX, TX_SELF, TextureValue, "int", |py, v| ok(
    py,
    v.binding_index
));

/// The port's error where `Texture.getValues` copies more floats than the texture holds:
/// `height*width` wraps in `unsigned` before the channels multiply it in `py::ssize_t`, where
/// the texture's count wrapped after them (docs/improvements.md, U-194).
fn past_texture_values() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "GpuShaderDesc.Texture.getValues: height*width*channels exceeds 32 bits, and the wheel \
         copies more floats than the texture holds (docs/improvements.md, U-194)",
    )
}

// .def("getValues", [](Texture & self) {...}) (PyGpuShaderDesc.cpp:259-285 @ v2.5.2): the
// GIL released, the texture's values from the description, the channel's count of floats
// (or upstream's error); then a new float32 array of `m_height * m_width * numChannels`
// values.
static TX_GET_VALUES: Function =
    method_def!("getValues", [TX_SELF], "numpy.ndarray", |py, o, a, _| {
        let rc = this_value!(o, a, TextureValue);
        let v = rc.lock().clone();
        let values = py.detach(|| {
            let desc = v.desc.lock();
            desc.texture(v.index as u32).map(|t| t.values().to_vec())
        });
        let values = py_result(py, values)?;
        let (num_channels, _) = num_channels(py, v.channel)?;
        let count = (v.height.wrapping_mul(v.width) as u64) * num_channels as u64;
        let values = usize::try_from(count)
            .ok()
            .and_then(|n| values.get(..n))
            .ok_or_else(past_texture_values)?;
        Ok(Some(
            PyArray1::<f32>::from_slice(py, values).into_any().unbind(),
        ))
    });

// .def_readonly(...) of Texture3D (PyGpuShaderDesc.cpp:331-335 @ v2.5.2).
readonly!(T3_TEXTURE_NAME, T3_SELF, Texture3DValue, "str", |py, v| {
    string_out(py, &v.texture_name)
});
readonly!(T3_SAMPLER_NAME, T3_SELF, Texture3DValue, "str", |py, v| {
    string_out(py, &v.sampler_name)
});
readonly!(T3_EDGE_LEN, T3_SELF, Texture3DValue, "int", |py, v| ok(
    py, v.edge_len
));
readonly!(
    T3_INTERPOLATION,
    T3_SELF,
    Texture3DValue,
    INTERPOLATION,
    |py, v| enum_out(py, INTERPOLATION, v.interpolation)
);
readonly!(
    T3_BINDING_INDEX,
    T3_SELF,
    Texture3DValue,
    "int",
    |py, v| ok(py, v.binding_index)
);

// .def("getValues", [](Texture3D & self) {...}) (PyGpuShaderDesc.cpp:336-349 @ v2.5.2): the
// GIL released, the texture's values; a new float32 array of `edgelen^3 * 3` (in `unsigned`)
// values, the texture's count.
static T3_GET_VALUES: Function =
    method_def!("getValues", [T3_SELF], "numpy.ndarray", |py, o, a, _| {
        let rc = this_value!(o, a, Texture3DValue);
        let v = rc.lock().clone();
        let values = py.detach(|| {
            let desc = v.desc.lock();
            desc.texture_3d(v.index as u32).map(|t| t.values().to_vec())
        });
        let values = py_result(py, values)?;
        let count = v
            .edge_len
            .wrapping_mul(v.edge_len)
            .wrapping_mul(v.edge_len)
            .wrapping_mul(3) as usize;
        let values = values.get(..count).ok_or_else(past_texture_values)?;
        Ok(Some(
            PyArray1::<f32>::from_slice(py, values).into_any().unbind(),
        ))
    });

/// Port of `bindPyGpuShaderDesc` (src/bindings/python/PyGpuShaderDesc.cpp:50-387 @ v2.5.2), in
/// its order: the nested classes declared (58-80), the description's methods (82-187), then
/// each nested class's members.
pub(crate) fn bind_py_gpu_shader_desc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, GPU_SHADER_DESC)?;
    let uniform_data = nested_class(&cls, "GpuShaderDesc", "UniformData")?;
    let uniform_iterator = nested_class(&cls, "GpuShaderDesc", "UniformIterator")?;
    let texture = nested_class(&cls, "GpuShaderDesc", "Texture")?;
    let texture_iterator = nested_class(&cls, "GpuShaderDesc", "TextureIterator")?;
    let texture_3d = nested_class(&cls, "GpuShaderDesc", "Texture3D")?;
    let texture_3d_iterator = nested_class(&cls, "GpuShaderDesc", "Texture3DIterator")?;

    def_static(&cls, &CREATE_SHADER_DESC)?;
    for f in [
        &CLONE,
        &GET_SHADER_TEXT,
        &GET_UNIFORMS,
        &GET_UNIFORM_BUFFER_SIZE,
        &ADD_TEXTURE,
        &GET_TEXTURES,
        &ADD_3D_TEXTURE,
        &GET_3D_TEXTURES,
    ] {
        def(&cls, f)?;
    }

    def(&uniform_data, &UD_INIT)?;
    def_readwrite(&uniform_data, "type", &UD_GET_TYPE, &UD_SET_TYPE)?;
    def_readwrite(
        &uniform_data,
        "bufferOffset",
        &UD_GET_BUFFER_OFFSET,
        &UD_SET_BUFFER_OFFSET,
    )?;
    for f in [
        &UD_GET_DOUBLE,
        &UD_GET_BOOL,
        &UD_GET_FLOAT3,
        &UD_GET_VECTOR_FLOAT,
        &UD_GET_VECTOR_INT,
    ] {
        def(&uniform_data, f)?;
    }

    for f in [&UI_LEN, &UI_GETITEM, &UI_ITER, &UI_NEXT] {
        def(&uniform_iterator, f)?;
    }

    def_readonly(&texture, "textureName", &TX_TEXTURE_NAME)?;
    def_readonly(&texture, "samplerName", &TX_SAMPLER_NAME)?;
    def_readonly(&texture, "width", &TX_WIDTH)?;
    def_readonly(&texture, "height", &TX_HEIGHT)?;
    def_readonly(&texture, "channel", &TX_CHANNEL)?;
    def_readonly(&texture, "dimensions", &TX_DIMENSIONS)?;
    def_readonly(&texture, "interpolation", &TX_INTERPOLATION)?;
    def_readonly(&texture, "textureShaderBindingIndex", &TX_BINDING_INDEX)?;
    def(&texture, &TX_GET_VALUES)?;

    for f in [&TI_LEN, &TI_GETITEM, &TI_ITER, &TI_NEXT] {
        def(&texture_iterator, f)?;
    }

    def_readonly(&texture_3d, "textureName", &T3_TEXTURE_NAME)?;
    def_readonly(&texture_3d, "samplerName", &T3_SAMPLER_NAME)?;
    def_readonly(&texture_3d, "edgeLen", &T3_EDGE_LEN)?;
    def_readonly(&texture_3d, "interpolation", &T3_INTERPOLATION)?;
    def_readonly(&texture_3d, "textureShaderBindingIndex", &T3_BINDING_INDEX)?;
    def(&texture_3d, &T3_GET_VALUES)?;

    for f in [&T3I_LEN, &T3I_GETITEM, &T3I_ITER, &T3I_NEXT] {
        def(&texture_3d_iterator, f)?;
    }
    Ok(())
}
