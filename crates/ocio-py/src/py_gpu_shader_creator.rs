// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGpuShaderCreator` (src/bindings/python/PyGpuShaderCreator.cpp,
//! PyGpuShaderCreator.h @ v2.5.2): the class `GpuShaderCreator`, its two enums (chunk 6.2b) and
//! its iterator class `DynamicPropertyIterator` (chunk 6.3h).
//!
//! The library's [`GpuShaderDesc`] is upstream's `GpuShaderCreator` and `GpuShaderDesc` in one
//! type (the owner's decision, 2026-09-30), and the only `GpuShaderCreator` upstream makes is a
//! `GenericGpuShaderDesc`: pybind11 holds one by its `GpuShaderDescRcPtr` (a `shared_ptr` the
//! iterators and the textures share), and makes every returned `GpuShaderCreatorRcPtr` an
//! instance of its most derived class, `GpuShaderDesc` (`polymorphic_type_hook`). So an
//! instance of either class holds a [`DescRc`], and the creator's methods read it.

use ocio_gpu::GpuShaderDesc;
use ocio_gpu::open_color_types::GpuLanguage;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, Param};
use crate::enums::{EnumDef, Scalar, define_enum, export_values};
use crate::py_cpu_processor::dynamic_property_type;
use crate::py_dynamic_property::{DYNAMIC_PROPERTY, dynamic_property_out};
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_transform::{
    INT, class, def, enum_out, nested_class, none, ok, outside_enumerators, string_out,
};
use crate::py_utils::new_iterator;
use crate::shared::Shared;
use crate::{arg, enum_param, method_def, q, this_iter};

pub(crate) const GPU_SHADER_CREATOR: &str = q!("GpuShaderCreator");
const DYNAMIC_PROPERTY_ITERATOR: &str = q!("GpuShaderCreator.DynamicPropertyIterator");
pub(crate) const GPU_LANGUAGE: &str = q!("GpuLanguage");
pub(crate) const DYNAMIC_PROPERTY_TYPE: &str = q!("DynamicPropertyType");
const SELF: Param = Param::new("self", GPU_SHADER_CREATOR);
const IT_SELF: Param = Param::new("self", DYNAMIC_PROPERTY_ITERATOR);

/// A description as `GpuShaderDescRcPtr` holds it: cloning the handle shares it.
pub(crate) type DescRc = Shared<GpuShaderDesc>;

/// `f` of the description `rc` holds, called without holding its lock, for the library's calls
/// that log while they write into it (`extractGpuShaderInfo`, `finalize`).
///
/// Upstream takes no lock: a logging function that reads the description sees it as the call
/// has filled it so far, and what it changes is what the call goes on with. Here the
/// description is moved out of its lock for the call (the lock holds a default description
/// meanwhile, which only another thread could read, where upstream's read is a data race), and
/// its log hook ([`ocio_gpu::gpu_shader_desc::LogHook`]) moves it back in around each log
/// message, then out again with whatever the logging function changed. Holding the lock across
/// the call deadlocked the logging function's first read of the description (verifier 6.3h,
/// H1).
pub(crate) fn with_desc_unlocked<R>(rc: &DescRc, f: impl FnOnce(&mut GpuShaderDesc) -> R) -> R {
    let mut work = std::mem::take(&mut *rc.lock());
    let shared = rc.clone();
    let hook: ocio_gpu::gpu_shader_desc::LogHook =
        std::sync::Arc::new(move |desc: &mut GpuShaderDesc, log: &mut dyn FnMut()| {
            std::mem::swap(&mut *shared.lock(), desc);
            log();
            std::mem::swap(&mut *shared.lock(), desc);
        });
    // A call nested in a logging function finds the outer call's hook: it is put back after.
    let outer = work.replace_log_hook(Some(hook));
    let result = f(&mut work);
    work.replace_log_hook(outer);
    *rc.lock() = work;
    result
}

/// `GpuShaderCreator::TextureType` (include/OpenColorIO/OpenColorIO.h:3485-3489 @ v2.5.2),
/// with the values PyGpuShaderCreator.cpp:128-131 gives it.
const TEXTURE_TYPE: EnumDef = EnumDef {
    name: "TextureType",
    scalar: Scalar::Unfixed,
    arithmetic: false,
    value_docs: false,
    entries: &[("TEXTURE_RED_CHANNEL", 0), ("TEXTURE_RGB_CHANNEL", 1)],
};

/// `GpuShaderCreator::TextureDimensions : uint8_t` (include/OpenColorIO/OpenColorIO.h:3494-3497
/// @ v2.5.2), with the values PyGpuShaderCreator.cpp:133-136 gives it.
const TEXTURE_DIMENSIONS: EnumDef = EnumDef {
    name: "TextureDimensions",
    scalar: Scalar::UChar,
    arithmetic: false,
    value_docs: false,
    entries: &[("TEXTURE_1D", 1), ("TEXTURE_2D", 2)],
};

/// The description of `self` (a `GpuShaderCreator` or a `GpuShaderDesc`), or the next overload.
#[macro_export]
macro_rules! this_desc {
    ($o:expr, $a:expr) => {
        $crate::this_shared!($o, $a, ocio_gpu::GpuShaderDesc)
    };
}

/// A `const char *` argument as the creator's setters take it: a null pointer is the empty
/// string (`name ? name : ""`, GpuShaderDesc.cpp:106-171 @ v2.5.2; the code sections'
/// `(shaderCode && *shaderCode) ? shaderCode : ""`, lines 284-359).
fn text(c: CharPtr) -> Vec<u8> {
    c.0.unwrap_or_default()
}

/// The library's `GpuLanguage` of the C++ value `v`: the port's error outside its enumerators,
/// which the wheel stores, and then reads through its `switch`es (docs/improvements.md, U-112).
fn gpu_language(v: i64) -> PyResult<GpuLanguage> {
    GpuLanguage::ALL
        .into_iter()
        .find(|l| *l as i64 == v)
        .ok_or_else(|| outside_enumerators(GPU_LANGUAGE, v))
}

// .def("clone", &GpuShaderCreator::clone) (PyGpuShaderCreator.cpp:59-60 @ v2.5.2): a
// `GpuShaderCreatorRcPtr`, made a `GpuShaderDesc` (its most derived class).
static CLONE: Function = method_def!("clone", [SELF], GPU_SHADER_CREATOR, |py, o, a, _| {
    let rc = this_desc!(o, a);
    let copy = rc.lock().clone_desc();
    crate::shared::out(
        py,
        crate::py_gpu_shader_desc::GPU_SHADER_DESC,
        &Shared::new(copy),
    )
});

// .def("getUniqueID", &GpuShaderCreator::getUniqueID) (PyGpuShaderCreator.cpp:61-62 @ v2.5.2).
static GET_UNIQUE_ID: Function = method_def!("getUniqueID", [SELF], "str", |py, o, a, _| {
    let rc = this_desc!(o, a);
    let uid = rc.lock().unique_id().to_vec();
    string_out(py, &uid)
});

// .def("setUniqueID", &GpuShaderCreator::setUniqueID, "uid"_a) (PyGpuShaderCreator.cpp:63-64 @
// v2.5.2).
static SET_UNIQUE_ID: Function = method_def!(
    "setUniqueID",
    [SELF, Param::new("uid", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let uid = text(arg!(a, 1, convert, CharPtr));
        rc.lock().set_unique_id(uid);
        none(py)
    }
);

// .def("getLanguage", &GpuShaderCreator::getLanguage) (PyGpuShaderCreator.cpp:65-66 @ v2.5.2).
static GET_LANGUAGE: Function = method_def!("getLanguage", [SELF], GPU_LANGUAGE, |py, o, a, _| {
    let rc = this_desc!(o, a);
    let lang = rc.lock().language();
    enum_out(py, GPU_LANGUAGE, lang as i64)
});

// .def("setLanguage", &GpuShaderCreator::setLanguage, "language"_a) (PyGpuShaderCreator.cpp:
// 67-68 @ v2.5.2).
static SET_LANGUAGE: Function = method_def!(
    "setLanguage",
    [SELF, Param::new("language", GPU_LANGUAGE)],
    "None",
    |py, o, a, _| {
        let rc = this_desc!(o, a);
        let lang = gpu_language(enum_param!(o, a, 1))?;
        rc.lock().set_language(lang);
        none(py)
    }
);

/// A getter and a setter of one of the creator's names, as statics `$get` and `$set`.
macro_rules! name_accessors {
    ($get:ident, $get_name:literal, $set:ident, $set_name:literal, $param:literal,
     $getter:ident, $setter:ident) => {
        static $get: Function = method_def!($get_name, [SELF], "str", |py, o, a, _| {
            let rc = this_desc!(o, a);
            let name = rc.lock().$getter().to_vec();
            string_out(py, &name)
        });

        static $set: Function = method_def!(
            $set_name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_desc!(o, a);
                let name = text(arg!(a, 1, convert, CharPtr));
                rc.lock().$setter(name);
                none(py)
            }
        );
    };
}

// .def("getFunctionName", ...), .def("setFunctionName", ..., "name"_a), .def("getPixelName",
// ...), .def("setPixelName", ..., "name"_a), .def("getResourcePrefix", ...),
// .def("setResourcePrefix", ..., "prefix"_a) (PyGpuShaderCreator.cpp:69-80 @ v2.5.2).
name_accessors!(
    GET_FUNCTION_NAME,
    "getFunctionName",
    SET_FUNCTION_NAME,
    "setFunctionName",
    "name",
    function_name,
    set_function_name
);
name_accessors!(
    GET_PIXEL_NAME,
    "getPixelName",
    SET_PIXEL_NAME,
    "setPixelName",
    "name",
    pixel_name,
    set_pixel_name
);
name_accessors!(
    GET_RESOURCE_PREFIX,
    "getResourcePrefix",
    SET_RESOURCE_PREFIX,
    "setResourcePrefix",
    "prefix",
    resource_prefix,
    set_resource_prefix
);

// .def("setDescriptorSetIndex", &GpuShaderCreator::setDescriptorSetIndex, "index"_a,
// "textureBindingStart"_a) (PyGpuShaderCreator.cpp:81-83 @ v2.5.2): two `unsigned`.
static SET_DESCRIPTOR_SET_INDEX: Function = method_def!(
    "setDescriptorSetIndex",
    [
        SELF,
        Param::new("index", INT),
        Param::new("textureBindingStart", INT)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let index = arg!(a, 1, convert, u32);
        let start = arg!(a, 2, convert, u32);
        py_result(py, rc.lock().set_descriptor_set_index(index, start))?;
        none(py)
    }
);

// .def("getDescriptorSetIndex", ...), .def("getTextureBindingStart", ...)
// (PyGpuShaderCreator.cpp:84-87 @ v2.5.2).
static GET_DESCRIPTOR_SET_INDEX: Function =
    method_def!("getDescriptorSetIndex", [SELF], "int", |py, o, a, _| {
        let rc = this_desc!(o, a);
        let index = rc.lock().descriptor_set_index();
        ok(py, index)
    });

static GET_TEXTURE_BINDING_START: Function =
    method_def!("getTextureBindingStart", [SELF], "int", |py, o, a, _| {
        let rc = this_desc!(o, a);
        let start = rc.lock().texture_binding_start();
        ok(py, start)
    });

// .def("getCacheID", &GpuShaderCreator::getCacheID) (PyGpuShaderCreator.cpp:88-89 @ v2.5.2).
static GET_CACHE_ID: Function = method_def!("getCacheID", [SELF], "str", |py, o, a, _| {
    let rc = this_desc!(o, a);
    let id = rc.lock().cache_id();
    string_out(py, &id)
});

// .def("begin", &GpuShaderCreator::begin, "uid"_a) (PyGpuShaderCreator.cpp:90-91 @ v2.5.2):
// `begin(const char *)` reads nothing of its argument.
static BEGIN: Function = method_def!(
    "begin",
    [SELF, Param::new("uid", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let uid = text(arg!(a, 1, convert, CharPtr));
        rc.lock().begin(uid);
        none(py)
    }
);

// .def("end", &GpuShaderCreator::end) (PyGpuShaderCreator.cpp:92-93 @ v2.5.2).
static END: Function = method_def!("end", [SELF], "None", |py, o, a, _| {
    let rc = this_desc!(o, a);
    rc.lock().end();
    none(py)
});

// .def("getTextureMaxWidth", ...), .def("setTextureMaxWidth", ..., "maxWidth"_a)
// (PyGpuShaderCreator.cpp:94-97 @ v2.5.2).
static GET_TEXTURE_MAX_WIDTH: Function =
    method_def!("getTextureMaxWidth", [SELF], "int", |py, o, a, _| {
        let rc = this_desc!(o, a);
        let width = rc.lock().texture_max_width();
        ok(py, width)
    });

static SET_TEXTURE_MAX_WIDTH: Function = method_def!(
    "setTextureMaxWidth",
    [SELF, Param::new("maxWidth", INT)],
    "None",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let width = arg!(a, 1, convert, u32);
        rc.lock().set_texture_max_width(width);
        none(py)
    }
);

// .def("setAllowTexture1D", ..., "allowed"_a), .def("getAllowTexture1D", ...)
// (PyGpuShaderCreator.cpp:98-101 @ v2.5.2).
static SET_ALLOW_TEXTURE_1D: Function = method_def!(
    "setAllowTexture1D",
    [SELF, Param::new("allowed", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let allowed = arg!(a, 1, convert, bool);
        rc.lock().set_allow_texture_1d(allowed);
        none(py)
    }
);

static GET_ALLOW_TEXTURE_1D: Function =
    method_def!("getAllowTexture1D", [SELF], "bool", |py, o, a, _| {
        let rc = this_desc!(o, a);
        let allowed = rc.lock().allow_texture_1d();
        ok(py, allowed)
    });

// .def("getNextResourceIndex", &GpuShaderCreator::getNextResourceIndex)
// (PyGpuShaderCreator.cpp:102-103 @ v2.5.2).
static GET_NEXT_RESOURCE_INDEX: Function =
    method_def!("getNextResourceIndex", [SELF], "int", |py, o, a, _| {
        let rc = this_desc!(o, a);
        let index = rc.lock().next_resource_index();
        ok(py, index)
    });

// .def("hasDynamicProperty", &GpuShaderCreator::hasDynamicProperty, "type"_a)
// (PyGpuShaderCreator.cpp:106-107 @ v2.5.2): a type outside the enumerators is no property's.
static HAS_DYNAMIC_PROPERTY: Function = method_def!(
    "hasDynamicProperty",
    [SELF, Param::new("type", DYNAMIC_PROPERTY_TYPE)],
    "bool",
    |py, o, a, _| {
        let rc = this_desc!(o, a);
        let v = enum_param!(o, a, 1);
        let has = dynamic_property_type(v).is_some_and(|t| rc.lock().has_dynamic_property(t));
        ok(py, has)
    }
);

// .def("getDynamicProperty", [](GpuShaderCreatorRcPtr & self, DynamicPropertyType type) {
// return PyDynamicProperty(self->getDynamicProperty(type)); }, "type"_a)
// (PyGpuShaderCreator.cpp:108-113 @ v2.5.2): a type outside the enumerators is no property's,
// upstream's error (`GpuShaderCreator::getDynamicProperty`, GpuShaderDesc.cpp:243-253).
static GET_DYNAMIC_PROPERTY: Function = method_def!(
    "getDynamicProperty",
    [SELF, Param::new("type", DYNAMIC_PROPERTY_TYPE)],
    DYNAMIC_PROPERTY,
    |py, o, a, _| {
        let rc = this_desc!(o, a);
        let v = enum_param!(o, a, 1);
        let prop = match dynamic_property_type(v) {
            Some(t) => py_result(py, rc.lock().dynamic_property_by_type(t))?,
            None => {
                return Err(to_py_err(
                    py,
                    &ocio::Exception::new("Dynamic property not found."),
                ));
            }
        };
        dynamic_property_out(py, prop)
    }
);

// .def("getDynamicProperties", [](GpuShaderCreatorRcPtr & self) { return
// DynamicPropertyIterator(self); }) (PyGpuShaderCreator.cpp:114-117 @ v2.5.2).
static GET_DYNAMIC_PROPERTIES: Function = method_def!(
    "getDynamicProperties",
    [SELF],
    DYNAMIC_PROPERTY_ITERATOR,
    |py, o, a, _| {
        let rc = this_desc!(o, a);
        Ok(Some(
            new_iterator(py, DYNAMIC_PROPERTY_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

/// A method adding a code section, as the static `$f`.
macro_rules! add_code {
    ($f:ident, $name:literal, $method:ident) => {
        static $f: Function = method_def!(
            $name,
            [SELF, Param::new("shaderCode", "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_desc!(o, a);
                let code = text(arg!(a, 1, convert, CharPtr));
                rc.lock().$method(code);
                none(py)
            }
        );
    };
}

// .def("addToParameterDeclareShaderCode", ..., "shaderCode"_a) and the five others
// (PyGpuShaderCreator.cpp:120-135 @ v2.5.2).
add_code!(
    ADD_TO_PARAMETER_DECLARE_SHADER_CODE,
    "addToParameterDeclareShaderCode",
    add_to_parameter_declare_shader_code
);
add_code!(
    ADD_TO_TEXTURE_DECLARE_SHADER_CODE,
    "addToTextureDeclareShaderCode",
    add_to_texture_declare_shader_code
);
add_code!(
    ADD_TO_HELPER_SHADER_CODE,
    "addToHelperShaderCode",
    add_to_helper_shader_code
);
add_code!(
    ADD_TO_FUNCTION_HEADER_SHADER_CODE,
    "addToFunctionHeaderShaderCode",
    add_to_function_header_shader_code
);
add_code!(
    ADD_TO_FUNCTION_SHADER_CODE,
    "addToFunctionShaderCode",
    add_to_function_shader_code
);
add_code!(
    ADD_TO_FUNCTION_FOOTER_SHADER_CODE,
    "addToFunctionFooterShaderCode",
    add_to_function_footer_shader_code
);

// .def("createShaderText", &GpuShaderCreator::createShaderText, "shaderParameterDeclarations"_a,
// "shaderTextureDeclarations"_a, "shaderHelperMethods"_a, "shaderFunctionHeader"_a,
// "shaderFunctionBody"_a, "shaderFunctionFooter"_a) (PyGpuShaderCreator.cpp:136-140 @ v2.5.2).
static CREATE_SHADER_TEXT: Function = method_def!(
    "createShaderText",
    [
        SELF,
        Param::new("shaderParameterDeclarations", "str"),
        Param::new("shaderTextureDeclarations", "str"),
        Param::new("shaderHelperMethods", "str"),
        Param::new("shaderFunctionHeader", "str"),
        Param::new("shaderFunctionBody", "str"),
        Param::new("shaderFunctionFooter", "str")
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_desc!(o, a);
        let p = text(arg!(a, 1, convert, CharPtr));
        let t = text(arg!(a, 2, convert, CharPtr));
        let h = text(arg!(a, 3, convert, CharPtr));
        let fh = text(arg!(a, 4, convert, CharPtr));
        let fb = text(arg!(a, 5, convert, CharPtr));
        let ff = text(arg!(a, 6, convert, CharPtr));
        rc.lock().create_shader_text(p, t, h, fh, fb, ff);
        none(py)
    }
);

// .def("finalize", &GpuShaderCreator::finalize) (PyGpuShaderCreator.cpp:141-142 @ v2.5.2).
static FINALIZE: Function = method_def!("finalize", [SELF], "None", |py, o, a, _| {
    let rc = this_desc!(o, a);
    py_result(py, with_desc_unlocked(&rc, GpuShaderDesc::finalize))?;
    none(py)
});

/// The description an iterator of `self` walks, or the next overload.
macro_rules! iter_desc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<DescRc>()?.clone();
        (it, rc)
    }};
}

fn num_dynamic_properties(rc: &DescRc) -> u32 {
    rc.lock().num_dynamic_properties()
}

// .def("__len__", [](DynamicPropertyIterator & it) { return
// it.m_obj->getNumDynamicProperties(); }) (PyGpuShaderCreator.cpp:155-158 @ v2.5.2).
static IT_LEN: Function = method_def!("__len__", [IT_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_desc!(o, a);
    ok(py, num_dynamic_properties(&rc))
});

// .def("__getitem__", [](DynamicPropertyIterator & it, int i) { return
// PyDynamicProperty(it.m_obj->getDynamicProperty(i)); }) (PyGpuShaderCreator.cpp:159-162 @
// v2.5.2): no `checkIndex`; the library's check of the index as an `unsigned`.
static IT_GETITEM: Function = method_def!(
    "__getitem__",
    [IT_SELF, Param::new("arg0", INT)],
    DYNAMIC_PROPERTY,
    |py, o, a, convert| {
        let (_, rc) = iter_desc!(o, a);
        let i = arg!(a, 1, convert, i32);
        let prop = py_result(py, rc.lock().dynamic_property(i as u32))?;
        dynamic_property_out(py, prop)
    }
);

// .def("__iter__", [](DynamicPropertyIterator & it) -> DynamicPropertyIterator & { return it;
// }) (PyGpuShaderCreator.cpp:163-166 @ v2.5.2).
static IT_ITER: Function = method_def!(
    "__iter__",
    [IT_SELF],
    DYNAMIC_PROPERTY_ITERATOR,
    |_, o, a, _| {
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    }
);

// .def("__next__", [](DynamicPropertyIterator & it) { int i =
// it.nextIndex(it.m_obj->getNumDynamicProperties()); return
// PyDynamicProperty(it.m_obj->getDynamicProperty(i)); }) (PyGpuShaderCreator.cpp:167-171 @
// v2.5.2).
static IT_NEXT: Function = method_def!("__next__", [IT_SELF], DYNAMIC_PROPERTY, |py, o, a, _| {
    let (it, rc) = iter_desc!(o, a);
    let i = it.get().next_index(num_dynamic_properties(&rc) as i32)?;
    let prop = py_result(py, rc.lock().dynamic_property(i as u32))?;
    dynamic_property_out(py, prop)
});

/// Port of `bindPyGpuShaderCreator` (src/bindings/python/PyGpuShaderCreator.cpp:38-175 @
/// v2.5.2), in its order: the enums and the iterator class declared (44-56), the methods
/// (58-142), the enums' values exported into the class (144-152), the iterator's methods
/// (154-171), then `bindPyGpuShaderDesc` (174).
pub(crate) fn bind_py_gpu_shader_creator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, GPU_SHADER_CREATOR)?;
    let texture_type = define_enum(&cls, Some("GpuShaderCreator"), &TEXTURE_TYPE)?;
    let texture_dimensions = define_enum(&cls, Some("GpuShaderCreator"), &TEXTURE_DIMENSIONS)?;
    let iterator = nested_class(&cls, "GpuShaderCreator", "DynamicPropertyIterator")?;
    for f in [
        &CLONE,
        &GET_UNIQUE_ID,
        &SET_UNIQUE_ID,
        &GET_LANGUAGE,
        &SET_LANGUAGE,
        &GET_FUNCTION_NAME,
        &SET_FUNCTION_NAME,
        &GET_PIXEL_NAME,
        &SET_PIXEL_NAME,
        &GET_RESOURCE_PREFIX,
        &SET_RESOURCE_PREFIX,
        &SET_DESCRIPTOR_SET_INDEX,
        &GET_DESCRIPTOR_SET_INDEX,
        &GET_TEXTURE_BINDING_START,
        &GET_CACHE_ID,
        &BEGIN,
        &END,
        &GET_TEXTURE_MAX_WIDTH,
        &SET_TEXTURE_MAX_WIDTH,
        &SET_ALLOW_TEXTURE_1D,
        &GET_ALLOW_TEXTURE_1D,
        &GET_NEXT_RESOURCE_INDEX,
        &HAS_DYNAMIC_PROPERTY,
        &GET_DYNAMIC_PROPERTY,
        &GET_DYNAMIC_PROPERTIES,
        &ADD_TO_PARAMETER_DECLARE_SHADER_CODE,
        &ADD_TO_TEXTURE_DECLARE_SHADER_CODE,
        &ADD_TO_HELPER_SHADER_CODE,
        &ADD_TO_FUNCTION_HEADER_SHADER_CODE,
        &ADD_TO_FUNCTION_SHADER_CODE,
        &ADD_TO_FUNCTION_FOOTER_SHADER_CODE,
        &CREATE_SHADER_TEXT,
        &FINALIZE,
    ] {
        def(&cls, f)?;
    }
    export_values(&cls, &texture_type)?;
    export_values(&cls, &texture_dimensions)?;
    for f in [&IT_LEN, &IT_GETITEM, &IT_ITER, &IT_NEXT] {
        def(&iterator, f)?;
    }
    crate::py_gpu_shader_desc::bind_py_gpu_shader_desc(m)
}
