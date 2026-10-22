// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyContext` (src/bindings/python/PyContext.cpp @ v2.5.2): the class `Context` and its
//! three iterator classes (chunk 6.3d).
//!
//! A `Context` object holds its context as pybind11 holds a `ContextRcPtr` ([`Shared`]): its
//! iterators share it, and `resolveStringVar` and `resolveFileLocation` write the variables they
//! use into the one given.

use ocio::Context;
use ocio_ops::open_color_types::EnvironmentMode;
use ocio_ops::utils::string_utils::{c_str, compare};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::dispatch::{CharPtr, Default, Function, Overload, Param, StdString, StringMap};
use crate::py_format_metadata::key_error;
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    INT, Out, class, def, enum_out, nested_class, none, ok, outside_enumerators, string_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, holder_arg, out};
use crate::{enum_param, method_def, q, this_iter, this_shared};

pub(crate) const CONTEXT: &str = q!("Context");
const STRING_VAR_NAME_ITERATOR: &str = q!("Context.StringVarNameIterator");
const STRING_VAR_ITERATOR: &str = q!("Context.StringVarIterator");
const SEARCH_PATH_ITERATOR: &str = q!("Context.SearchPathIterator");
const ENVIRONMENT_MODE: &str = q!("EnvironmentMode");
const SELF: Param = Param::new("self", CONTEXT);

/// A context shared by the Python objects that hold it.
pub(crate) type ContextRc = Shared<Context>;

/// The library's `EnvironmentMode` of the C++ value `v`: the wheel stores a value outside the
/// enumerators, which the library can't hold (U-112).
fn environment_mode(v: i64) -> PyResult<EnvironmentMode> {
    [
        EnvironmentMode::Unknown,
        EnvironmentMode::LoadPredefined,
        EnvironmentMode::LoadAll,
    ]
    .into_iter()
    .find(|m| *m as i64 == v)
    .ok_or_else(|| outside_enumerators(ENVIRONMENT_MODE, v))
}

/// A `const char *` argument's bytes, a null pointer as the empty string (what each setter of
/// upstream's `Context` makes of it, Context.cpp:195-258 @ v2.5.2).
fn or_empty(s: CharPtr) -> Vec<u8> {
    s.0.unwrap_or_default()
}

// .def(py::init(&Context::Create)), .def(py::init([](workingDir, searchPaths, stringVars,
// environmentMode) {...}), "workingDir"_a = DEFAULT->getWorkingDir(), "searchPaths"_a =
// getSearchPathsStdVec(DEFAULT), "stringVars"_a = getStringVarsStdMap(DEFAULT),
// "environmentMode"_a = DEFAULT->getEnvironmentMode()) (PyContext.cpp:68-101 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                construct(&a[0], CONTEXT, Shared::new(Context::new()))?;
                let _ = o;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "workingDir",
                    ty: "str",
                    default: Some(Default {
                        repr: "''",
                        value: |py| Ok(pyo3::types::PyString::new(py, "").into_any()),
                    }),
                },
                Param {
                    name: "searchPaths",
                    ty: "collections.abc.Sequence[str]",
                    default: Some(Default {
                        repr: "[]",
                        value: |py| Ok(pyo3::types::PyList::empty(py).into_any()),
                    }),
                },
                Param {
                    name: "stringVars",
                    ty: "collections.abc.Mapping[str, str]",
                    default: Some(Default {
                        repr: "{}",
                        value: |py| Ok(pyo3::types::PyDict::new(py).into_any()),
                    }),
                },
                Param {
                    name: "environmentMode",
                    ty: ENVIRONMENT_MODE,
                    default: Some(Default {
                        repr: "<EnvironmentMode.ENV_ENVIRONMENT_LOAD_PREDEFINED: 1>",
                        value: |py| {
                            crate::enums::to_py(
                                &crate::enums::enum_class(py, ENVIRONMENT_MODE)?,
                                EnvironmentMode::LoadPredefined as i64,
                            )
                        },
                    }),
                },
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(working_dir) = crate::arg!(a, 1, convert, StdString);
                let search_paths = crate::arg!(a, 2, convert, Vec<StdString>);
                let StringMap(string_vars) = crate::arg!(a, 3, convert, StringMap);
                let mode = environment_mode(enum_param!(o, a, 4))?;
                let mut p = Context::new();
                if !working_dir.is_empty() {
                    p.set_working_dir(c_str(&working_dir));
                }
                for StdString(path) in &search_paths {
                    p.add_search_path(c_str(path));
                }
                for (name, value) in &string_vars {
                    p.set_string_var(c_str(name), Some(c_str(value)));
                }
                p.set_environment_mode(mode);
                construct(&a[0], CONTEXT, Shared::new(p))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__deepcopy__", [](const ConstContextRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyContext.cpp:103-107 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    CONTEXT,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let copy = rc.lock().clone();
        out(py, CONTEXT, &Shared::new(copy))
    }
);

// .def("__iter__", [](ContextRcPtr & self) { return StringVarNameIterator(self); })
// (PyContext.cpp:109-112 @ v2.5.2).
static ITER: Function = method_def!(
    "__iter__",
    [SELF],
    STRING_VAR_NAME_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        Ok(Some(
            new_iterator(py, STRING_VAR_NAME_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

// .def("__len__", &Context::getNumStringVars) (PyContext.cpp:113-114 @ v2.5.2).
static LEN: Function = method_def!("__len__", [SELF], "int", |py, o, a, _| {
    let rc = this_shared!(o, a, Context);
    let n = rc.lock().num_string_vars();
    ok(py, n)
});

/// The index of the variable whose name is `name` ignoring case (`StringUtils::Compare` of the
/// whole `std::string`s).
fn find_var(ctx: &Context, name: &[u8]) -> Option<i32> {
    (0..ctx.num_string_vars()).find(|&i| compare(ctx.string_var_name_by_index(i), name))
}

// .def("__getitem__", [](ContextRcPtr & self, const std::string & name) -> const char * {...},
// "name"_a) (PyContext.cpp:115-131 @ v2.5.2): the variable named `name` (its C string)
// once a name matches ignoring case, else KeyError "'<name>'".
static GETITEM: Function = method_def!(
    "__getitem__",
    [SELF, Param::new("name", "str")],
    "str",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Context);
        let StdString(name) = crate::arg!(a, 1, convert, StdString);
        let value = {
            let ctx = rc.lock();
            find_var(&ctx, &name).map(|_| ctx.string_var(c_str(&name)).to_vec())
        };
        match value {
            Some(value) => string_out(py, &value),
            None => {
                let mut message = b"'".to_vec();
                message.extend_from_slice(&name);
                message.push(b'\'');
                Err(key_error(py, &message))
            }
        }
    }
);

/// `setStringVar(name, value)`: a null name is ignored, a null value removes the variable.
fn set_string_var(rc: &ContextRc, name: CharPtr, value: CharPtr) {
    if let Some(name) = name.0 {
        rc.lock().set_string_var(name, value.0.as_deref());
    }
}

// .def("__setitem__", &Context::setStringVar, "name"_a, "value"_a) (PyContext.cpp:132-133 @
// v2.5.2).
static SETITEM: Function = method_def!(
    "__setitem__",
    [SELF, Param::new("name", "str"), Param::new("value", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Context);
        let name = crate::arg!(a, 1, convert, CharPtr);
        let value = crate::arg!(a, 2, convert, CharPtr);
        set_string_var(&rc, name, value);
        none(py)
    }
);

// .def("__contains__", [](ContextRcPtr & self, const std::string & name) -> bool {...},
// "name"_a) (PyContext.cpp:134-146 @ v2.5.2).
static CONTAINS: Function = method_def!(
    "__contains__",
    [SELF, Param::new("name", "str")],
    "bool",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Context);
        let StdString(name) = crate::arg!(a, 1, convert, StdString);
        let found = find_var(&rc.lock(), &name).is_some();
        ok(py, found)
    }
);

/// A getter of one `const char *` of the context.
macro_rules! text_getter {
    ($name:literal, $get:expr) => {
        method_def!($name, [SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, Context);
            let text: Vec<u8> = $get(&*rc.lock());
            string_out(py, &text)
        })
    };
}

/// A setter of one `const char *` of the context, a null pointer as `""`.
macro_rules! text_setter {
    ($name:literal, $param:literal, $set:expr) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, Context);
                let s = or_empty(crate::arg!(a, 1, convert, CharPtr));
                $set(&mut *rc.lock(), &s);
                none(py)
            }
        )
    };
}

// .def("getCacheID", &Context::getCacheID) (PyContext.cpp:148-149 @ v2.5.2).
static GET_CACHE_ID: Function = text_getter!("getCacheID", |c: &Context| c.cache_id().into_bytes());

// .def("getSearchPath", (const char * (Context::*)() const) &Context::getSearchPath)
// (PyContext.cpp:150-151 @ v2.5.2).
static GET_SEARCH_PATH: Function =
    text_getter!("getSearchPath", |c: &Context| c_str(c.search_path())
        .to_vec());

// .def("setSearchPath", &Context::setSearchPath, "path"_a) (PyContext.cpp:152-153 @ v2.5.2).
static SET_SEARCH_PATH: Function =
    text_setter!("setSearchPath", "path", |c: &mut Context, s: &[u8]| c
        .set_search_path(s));

// .def("getSearchPaths", [](ContextRcPtr & self) { return SearchPathIterator(self); })
// (PyContext.cpp:154-157 @ v2.5.2).
static GET_SEARCH_PATHS: Function = method_def!(
    "getSearchPaths",
    [SELF],
    SEARCH_PATH_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        Ok(Some(
            new_iterator(py, SEARCH_PATH_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

// .def("clearSearchPaths", &Context::clearSearchPaths) (PyContext.cpp:158-159 @ v2.5.2).
static CLEAR_SEARCH_PATHS: Function =
    method_def!("clearSearchPaths", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        rc.lock().clear_search_paths();
        none(py)
    });

// .def("addSearchPath", &Context::addSearchPath, "path"_a) (PyContext.cpp:160-161 @ v2.5.2).
static ADD_SEARCH_PATH: Function =
    text_setter!("addSearchPath", "path", |c: &mut Context, s: &[u8]| c
        .add_search_path(s));

// .def("getWorkingDir", &Context::getWorkingDir) (PyContext.cpp:162-163 @ v2.5.2).
static GET_WORKING_DIR: Function =
    text_getter!("getWorkingDir", |c: &Context| c_str(c.working_dir())
        .to_vec());

// .def("setWorkingDir", &Context::setWorkingDir, "dirName"_a) (PyContext.cpp:164-165 @
// v2.5.2).
static SET_WORKING_DIR: Function =
    text_setter!("setWorkingDir", "dirName", |c: &mut Context, s: &[u8]| c
        .set_working_dir(s));

// .def("getStringVars", [](ContextRcPtr & self) { return StringVarIterator(self); })
// (PyContext.cpp:166-169 @ v2.5.2).
static GET_STRING_VARS: Function = method_def!(
    "getStringVars",
    [SELF],
    STRING_VAR_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        Ok(Some(
            new_iterator(py, STRING_VAR_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

// .def("setStringVar", &Context::setStringVar, "name"_a, "value"_a) (PyContext.cpp:170-171 @
// v2.5.2).
static SET_STRING_VAR: Function = method_def!(
    "setStringVar",
    [SELF, Param::new("name", "str"), Param::new("value", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Context);
        let name = crate::arg!(a, 1, convert, CharPtr);
        let value = crate::arg!(a, 2, convert, CharPtr);
        set_string_var(&rc, name, value);
        none(py)
    }
);

// .def("clearStringVars", &Context::clearStringVars) (PyContext.cpp:172-173 @ v2.5.2).
static CLEAR_STRING_VARS: Function =
    method_def!("clearStringVars", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        rc.lock().clear_string_vars();
        none(py)
    });

// .def("getEnvironmentMode", &Context::getEnvironmentMode) (PyContext.cpp:174-175 @ v2.5.2).
static GET_ENVIRONMENT_MODE: Function = method_def!(
    "getEnvironmentMode",
    [SELF],
    ENVIRONMENT_MODE,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        let mode = rc.lock().environment_mode();
        enum_out(py, ENVIRONMENT_MODE, mode as i64)
    }
);

// .def("setEnvironmentMode", &Context::setEnvironmentMode, "mode"_a) (PyContext.cpp:176-177 @
// v2.5.2).
static SET_ENVIRONMENT_MODE: Function = method_def!(
    "setEnvironmentMode",
    [SELF, Param::new("mode", ENVIRONMENT_MODE)],
    "None",
    |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        let mode = environment_mode(enum_param!(o, a, 1))?;
        rc.lock().set_environment_mode(mode);
        none(py)
    }
);

// .def("loadEnvironment", &Context::loadEnvironment) (PyContext.cpp:178-179 @ v2.5.2).
static LOAD_ENVIRONMENT: Function =
    method_def!("loadEnvironment", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, Context);
        rc.lock().load_environment();
        none(py)
    });

/// A `ContextRcPtr &` argument `a[i]` (`None`, a null pointer, with conversions), or the next
/// overload.
macro_rules! context_param {
    ($a:expr, $i:expr, $convert:expr) => {
        match holder_arg::<Context>(&$a[$i], CONTEXT, $convert)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

/// The port's error where the wheel's context would record the variables it used into itself:
/// `setStringVar` locks the context's `std::mutex`, which the resolution holds already: the
/// wheel's process dies (Windows) or deadlocks (glibc, unverified) (docs/improvements.md,
/// U-122).
fn records_into_itself() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "the context records the variables it used into itself, which locks its mutex twice: \
         the wheel's process dies (Windows) or deadlocks (glibc, unverified) \
         (docs/improvements.md, U-122)",
    )
}

/// `resolve(ctx, used)` of the context `rc`, recording the variables used into `used`: none
/// (a null pointer), another context, or the context itself (U-122 when a variable is used).
fn resolve<R>(
    rc: &ContextRc,
    used: Option<&ContextRc>,
    resolve: impl Fn(&Context, &mut Context) -> R,
) -> PyResult<R> {
    let mut scratch = Context::new();
    match used {
        None => Ok(resolve(&rc.lock(), &mut scratch)),
        Some(used) if used.same(rc) => {
            let result = resolve(&rc.lock(), &mut scratch);
            if scratch.num_string_vars() > 0 {
                return Err(records_into_itself());
            }
            Ok(result)
        }
        Some(used) => Ok(resolve(&rc.lock(), &mut used.lock())),
    }
}

/// The error of `Platform::Strcasecmp` given a null pointer, which the rules'
/// `getIndexForRule` compares with each rule's name.
///
/// Port of the null check of `Platform::Strcasecmp` (src/OpenColorIO/Platform.cpp:155-160 @
/// v2.5.2).
pub(crate) fn null_comparison(py: Python<'_>) -> PyErr {
    crate::py_open_color_io::to_py_err(
        py,
        &ocio::Exception::new("String pointer for comparison must not be null."),
    )
}

/// The port's error for a null file name, from which upstream makes a `std::string` (MSVC's
/// reads address 0; docs/improvements.md, U-123).
pub(crate) fn null_string(what: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{what}: None, a null pointer from which the wheel constructs a std::string (its process \
         dies; docs/improvements.md, U-123)"
    ))
}

// .def("resolveStringVar", (const char * (Context::*)(const char *) const noexcept), "string"_a),
// .def("resolveStringVar", (const char * (Context::*)(const char *, ContextRcPtr &) const
// noexcept), "string"_a, "usedContextVars"_a) (PyContext.cpp:180-189 @ v2.5.2).
static RESOLVE_STRING_VAR: Function = Function {
    name: "resolveStringVar",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("string", "str")],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, Context);
                let s = or_empty(crate::arg!(a, 1, convert, CharPtr));
                let resolved = rc.lock().resolve_string_var(&s);
                string_out(py, c_str(&resolved))
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("string", "str"),
                Param::new("usedContextVars", CONTEXT),
            ],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, Context);
                let s = or_empty(crate::arg!(a, 1, convert, CharPtr));
                let used = context_param!(a, 2, convert);
                let resolved = resolve(&rc, used.as_ref(), |c, u| {
                    c.resolve_string_var_with_used(&s, u)
                })?;
                string_out(py, c_str(&resolved))
            },
            pos_only: 0,
        },
    ],
};

// .def("resolveFileLocation", (const char * (Context::*)(const char *) const), "filename"_a),
// .def("resolveFileLocation", (const char * (Context::*)(const char *, ContextRcPtr &) const),
// "filename"_a, "usedContextVars"_a) (PyContext.cpp:190-199 @ v2.5.2).
static RESOLVE_FILE_LOCATION: Function = Function {
    name: "resolveFileLocation",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("filename", "str")],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, Context);
                let Some(name) = crate::arg!(a, 1, convert, CharPtr).0 else {
                    return Err(null_string("resolveFileLocation(filename)"));
                };
                let resolved = py_result(py, rc.lock().resolve_file_location(&name))?;
                string_out(py, c_str(&resolved))
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("filename", "str"),
                Param::new("usedContextVars", CONTEXT),
            ],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, Context);
                let name = crate::arg!(a, 1, convert, CharPtr);
                let used = context_param!(a, 2, convert);
                let Some(name) = name.0 else {
                    return Err(null_string("resolveFileLocation(filename)"));
                };
                let resolved = resolve(&rc, used.as_ref(), |c, u| {
                    c.resolve_file_location_with_used(&name, u)
                })?;
                string_out(py, c_str(&py_result(py, resolved)?))
            },
            pos_only: 0,
        },
    ],
};

// defRepr(clsContext) (PyContext.cpp:201 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, Context);
    let text = rc.lock().to_bytes();
    string_out(py, &text)
});

// ---------------------------------------------------------------------------------------------
// The iterators (PyContext.cpp:203-248 @ v2.5.2)

const NAME_SELF: Param = Param::new("self", STRING_VAR_NAME_ITERATOR);
const VAR_SELF: Param = Param::new("self", STRING_VAR_ITERATOR);
const PATH_SELF: Param = Param::new("self", SEARCH_PATH_ITERATOR);

/// The context an iterator of `self` walks, or the next overload.
macro_rules! iter_rc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<ContextRc>()?.clone();
        (it, rc)
    }};
}

/// `(name, getStringVar(name))` of the variable at `i`.
fn string_var(py: Python<'_>, rc: &ContextRc, i: i32) -> Out {
    let (name, value) = {
        let ctx = rc.lock();
        let name = c_str(ctx.string_var_name_by_index(i)).to_vec();
        let value = c_str(ctx.string_var(&name)).to_vec();
        (name, value)
    };
    let tuple = PyTuple::new(
        py,
        [
            crate::dispatch::std_string_to_py(py, &name)?,
            crate::dispatch::std_string_to_py(py, &value)?,
        ],
    )?;
    Ok(Some(tuple.into_any().unbind()))
}

fn var_name(py: Python<'_>, rc: &ContextRc, i: i32) -> Out {
    let name = c_str(rc.lock().string_var_name_by_index(i)).to_vec();
    string_out(py, &name)
}

fn search_path(py: Python<'_>, rc: &ContextRc, i: i32) -> Out {
    let path = c_str(rc.lock().search_path_with_index(i)).to_vec();
    string_out(py, &path)
}

fn num_vars(rc: &ContextRc) -> i32 {
    rc.lock().num_string_vars()
}

fn num_paths(rc: &ContextRc) -> i32 {
    rc.lock().num_search_paths()
}

/// The four methods of an iterator class: `__len__` (`$num`), `__getitem__` (index checked,
/// then `$item`), `__iter__`, `__next__`.
macro_rules! iterator {
    ($self:expr, $qualified:expr, $ret:expr, $num:expr, $item:expr,
     $len:ident, $getitem:ident, $iter:ident, $next:ident) => {
        static $len: Function = method_def!("__len__", [$self], "int", |py, o, a, _| {
            let (_, rc) = iter_rc!(o, a);
            ok(py, $num(&rc))
        });

        static $getitem: Function = method_def!(
            "__getitem__",
            [$self, Param::new("arg0", INT)],
            $ret,
            |py, o, a, convert| {
                let (_, rc) = iter_rc!(o, a);
                let i = crate::arg!(a, 1, convert, i32);
                check_index(i, $num(&rc))?;
                $item(py, &rc, i)
            }
        );

        static $iter: Function = method_def!("__iter__", [$self], $qualified, |_, o, a, _| {
            let _ = iter_rc!(o, a);
            Ok(Some(a[0].clone().unbind()))
        });

        static $next: Function = method_def!("__next__", [$self], $ret, |py, o, a, _| {
            let (it, rc) = iter_rc!(o, a);
            let i = it.get().next_index($num(&rc))?;
            $item(py, &rc, i)
        });
    };
}

iterator!(
    NAME_SELF,
    STRING_VAR_NAME_ITERATOR,
    "str",
    num_vars,
    var_name,
    NAME_LEN,
    NAME_GETITEM,
    NAME_ITER,
    NAME_NEXT
);
iterator!(
    VAR_SELF,
    STRING_VAR_ITERATOR,
    "tuple",
    num_vars,
    string_var,
    VAR_LEN,
    VAR_GETITEM,
    VAR_ITER,
    VAR_NEXT
);
iterator!(
    PATH_SELF,
    SEARCH_PATH_ITERATOR,
    "str",
    num_paths,
    search_path,
    PATH_LEN,
    PATH_GETITEM,
    PATH_ITER,
    PATH_NEXT
);

/// Port of `bindPyContext` (src/bindings/python/PyContext.cpp:48-249 @ v2.5.2), in its order:
/// the iterator classes nested in the class (52-66), the class's methods (68-199), `__repr__`
/// (201), the iterators' (203-248).
pub(crate) fn bind_py_context(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, CONTEXT)?;
    let names = nested_class(&cls, "Context", "StringVarNameIterator")?;
    let vars = nested_class(&cls, "Context", "StringVarIterator")?;
    let paths = nested_class(&cls, "Context", "SearchPathIterator")?;
    for f in [
        &INIT,
        &DEEPCOPY,
        &ITER,
        &LEN,
        &GETITEM,
        &SETITEM,
        &CONTAINS,
        &GET_CACHE_ID,
        &GET_SEARCH_PATH,
        &SET_SEARCH_PATH,
        &GET_SEARCH_PATHS,
        &CLEAR_SEARCH_PATHS,
        &ADD_SEARCH_PATH,
        &GET_WORKING_DIR,
        &SET_WORKING_DIR,
        &GET_STRING_VARS,
        &SET_STRING_VAR,
        &CLEAR_STRING_VARS,
        &GET_ENVIRONMENT_MODE,
        &SET_ENVIRONMENT_MODE,
        &LOAD_ENVIRONMENT,
        &RESOLVE_STRING_VAR,
        &RESOLVE_FILE_LOCATION,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    for f in [&NAME_LEN, &NAME_GETITEM, &NAME_ITER, &NAME_NEXT] {
        def(&names, f)?;
    }
    for f in [&VAR_LEN, &VAR_GETITEM, &VAR_ITER, &VAR_NEXT] {
        def(&vars, f)?;
    }
    for f in [&PATH_LEN, &PATH_GETITEM, &PATH_ITER, &PATH_NEXT] {
        def(&paths, f)?;
    }
    Ok(())
}
