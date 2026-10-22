// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyColorSpaceSet` (src/bindings/python/PyColorSpaceSet.cpp @ v2.5.2): the class
//! `ColorSpaceSet` and its two iterator classes (chunk 6.3e).
//!
//! Upstream's set holds `ColorSpaceRcPtr`s: it adds copies, and returns its own color spaces,
//! through which a change shows in the set. The binding holds a set as the list of its color
//! spaces' nodes ([`ColorSpaceRc`]), and so ports `ColorSpaceSet::Impl` over them
//! ([`index`], [`add`], [`remove`]; the library's `ColorSpaceSet` holds color spaces by value).
//!
//! `None` for a set or a color space is a null pointer, which upstream dereferences but in the
//! difference of an empty set (U-124).

use ocio_ops::utils::string_utils::{c_str, compare, lower};
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, Overload, Param};
use crate::py_color_space::{COLOR_SPACE, ColorSpaceNode, ColorSpaceRc};
use crate::py_open_color_io::to_py_err;
use crate::py_transform::{INT, Out, class, def, nested_class, none, ok};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, holder, holder_arg, out};
use crate::{method_def, q, this_iter, this_shared};

const COLOR_SPACE_SET: &str = q!("ColorSpaceSet");
const NAME_ITERATOR: &str = q!("ColorSpaceSet.ColorSpaceNameIterator");
const COLOR_SPACE_ITERATOR: &str = q!("ColorSpaceSet.ColorSpaceIterator");
const SELF: Param = Param::new("self", COLOR_SPACE_SET);
const ARG0: Param = Param::new("arg0", COLOR_SPACE_SET);

/// The color spaces of a set (`m_colorSpaces`).
pub(crate) type SetRc = Shared<Vec<ColorSpaceRc>>;

/// The name and aliases of a color space.
fn names(cs: &ColorSpaceRc) -> (Vec<u8>, Vec<Vec<u8>>) {
    let node = cs.lock();
    let aliases = (0..node.cs.num_aliases())
        .map(|i| node.cs.alias(i).to_vec())
        .collect();
    (c_str(node.cs.name()).to_vec(), aliases)
}

/// Port of `ColorSpaceSet::Impl::getIndex` (src/OpenColorIO/ColorSpaceSet.cpp:87-111 @ v2.5.2):
/// the color space whose name or an alias is `name`, ignoring case; -1 for none (an empty or
/// null name).
pub(crate) fn index(set: &[ColorSpaceRc], name: Option<&[u8]>) -> i32 {
    let Some(name) = name.map(c_str).filter(|n| !n.is_empty()) else {
        return -1;
    };
    let s = lower(name);
    for (idx, cs) in set.iter().enumerate() {
        let (cs_name, aliases) = names(cs);
        if compare(&cs_name, &s) || aliases.iter().any(|a| compare(a, &s)) {
            return idx as i32;
        }
    }
    -1
}

/// Port of `ColorSpaceSet::Impl::add(const ConstColorSpaceRcPtr &)` (ColorSpaceSet.cpp:118-171
/// @ v2.5.2): a copy of `cs`, replacing the color space of its name, or after the others; the
/// errors of a name that is another's alias, or an alias another uses.
fn add(py: Python<'_>, set: &SetRc, cs: &ColorSpaceRc) -> PyResult<()> {
    let (cs_name, aliases) = names(cs);
    if cs_name.is_empty() {
        return Err(exception(
            py,
            b"Cannot add a color space with an empty name.".to_vec(),
        ));
    }
    let current = set.lock().clone();
    let entry = index(&current, Some(&cs_name));
    let mut replace = None;
    if entry != -1 {
        // If getIndex succeeds but the csName is not the name of the matching color space, it
        // means that csName must be an alias name.
        let existing = names(&current[entry as usize]).0;
        if !compare(&existing, &cs_name) {
            return Err(exception(
                py,
                [
                    b"Cannot add '".as_slice(),
                    &cs_name,
                    b"' color space, existing color space, '",
                    &existing,
                    b"' is using this name as an alias.",
                ]
                .concat(),
            ));
        }
        replace = Some(entry as usize);
    }
    for alias in &aliases {
        let e = index(&current, Some(alias));
        // Is an alias of the color space already used by a color space? Skip existing
        // colorspace that might be replaced.
        if e != -1 && replace != Some(e as usize) {
            return Err(exception(
                py,
                [
                    b"Cannot add '".as_slice(),
                    &cs_name,
                    b"' color space, it has '",
                    alias,
                    b"' alias and existing color space, '",
                    &names(&current[e as usize]).0,
                    b"' is using the same alias.",
                ]
                .concat(),
            ));
        }
    }
    let copy = Shared::new(cs.lock().copy(py)?);
    let mut list = set.lock();
    match replace {
        Some(i) => list[i] = copy,
        None => list.push(copy),
    }
    Ok(())
}

/// Port of `ColorSpaceSet::Impl::add(const Impl &)` (ColorSpaceSet.cpp:173-179 @ v2.5.2): each
/// color space of `rhs` (as it was when the call began: `rhs` may be the set itself, whose
/// color spaces upstream replaces each with a copy).
fn add_all(py: Python<'_>, set: &SetRc, rhs: &SetRc) -> PyResult<()> {
    let items = rhs.lock().clone();
    for cs in &items {
        add(py, set, cs)?;
    }
    Ok(())
}

/// Port of `ColorSpaceSet::Impl::remove(const char *)` (ColorSpaceSet.cpp:181-196 @ v2.5.2):
/// the first color space named `name` (ignoring case, not its aliases).
fn remove(set: &SetRc, name: Option<&[u8]>) {
    let Some(name) = name.map(c_str).filter(|n| !n.is_empty()) else {
        return;
    };
    let name = lower(name);
    let current = set.lock().clone();
    if let Some(i) = current.iter().position(|cs| lower(&names(cs).0) == name) {
        set.lock().remove(i);
    }
}

/// An OCIO exception of the message `what`.
fn exception(py: Python<'_>, what: Vec<u8>) -> PyErr {
    to_py_err(py, &ocio::Exception::new(what))
}

/// The port's error where upstream dereferences a null set or color space (U-124).
fn null_object(what: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{what}: None, a null pointer the wheel dereferences (its process dies; \
         docs/improvements.md, U-124)"
    ))
}

/// Port of `ColorSpaceSet::createEditableCopy` and `Impl::operator=` (ColorSpaceSet.cpp:23-35,
/// 244-249 @ v2.5.2): a copy of each color space.
fn copy(py: Python<'_>, set: &SetRc) -> PyResult<SetRc> {
    let items = set.lock().clone();
    let mut copies = Vec::with_capacity(items.len());
    for cs in &items {
        copies.push(Shared::new(cs.lock().copy(py)?));
    }
    Ok(Shared::new(copies))
}

/// `hasColorSpace(name)`.
fn has(set: &SetRc, name: &[u8]) -> bool {
    let current = set.lock().clone();
    index(&current, Some(name)) != -1
}

// .def(py::init(&ColorSpaceSet::Create)) (PyColorSpaceSet.cpp:45-46 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[SELF],
        ret: "None",
        call: |py, _, a, _| {
            construct(
                &a[0],
                COLOR_SPACE_SET,
                Shared::new(Vec::<ColorSpaceRc>::new()),
            )?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("__deepcopy__", [](const ConstColorSpaceSetRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyColorSpaceSet.cpp:48-52 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    COLOR_SPACE_SET,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        out(py, COLOR_SPACE_SET, &copy(py, &rc)?)
    }
);

/// An operator of one overload (`py::is_operator()`): `(self, arg0: ColorSpaceSet) -> ret`.
macro_rules! operator_def {
    ($name:literal, $ret:expr, $call:expr) => {
        Function {
            name: $name,
            constructor: false,
            method: true,
            operator: true,
            overloads: &[Overload {
                params: &[SELF, ARG0],
                ret: $ret,
                call: $call,
                pos_only: 0,
            }],
        }
    };
}

/// Port of `ColorSpaceSet::Impl::operator==` (ColorSpaceSet.cpp:38-58 @ v2.5.2): the same set,
/// or as many color spaces, each named in the other (names only).
fn equals(lhs: &SetRc, rhs: &SetRc) -> bool {
    if lhs.same(rhs) {
        return true;
    }
    let l = lhs.lock().clone();
    let r = rhs.lock().clone();
    if l.len() != r.len() {
        return false;
    }
    l.iter().all(|cs| index(&r, Some(&names(cs).0)) != -1)
}

// .def("__eq__", &ColorSpaceSet::operator==, py::is_operator()), .def("__ne__",
// &ColorSpaceSet::operator!=, py::is_operator()) (PyColorSpaceSet.cpp:54-57 @ v2.5.2): the
// other set by reference (`None` refused).
static EQ: Function = operator_def!("__eq__", "bool", |py, o, a, _| {
    let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
    let Some(other) = holder::<Vec<ColorSpaceRc>>(&a[1], COLOR_SPACE_SET)? else {
        return Ok(None);
    };
    ok(py, equals(&rc, &other))
});
static NE: Function = operator_def!("__ne__", "bool", |py, o, a, _| {
    let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
    let Some(other) = holder::<Vec<ColorSpaceRc>>(&a[1], COLOR_SPACE_SET)? else {
        return Ok(None);
    };
    ok(py, !equals(&rc, &other))
});

/// The other set of an operator (`ConstColorSpaceSetRcPtr &`: `None`, a null pointer, with
/// conversions), or the next overload.
macro_rules! other_set {
    ($a:expr, $convert:expr) => {
        match holder_arg::<Vec<ColorSpaceRc>>(&$a[1], COLOR_SPACE_SET, $convert)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

// .def("__sub__", [](self, rcss) { return self - rcss; }, py::is_operator())
// (PyColorSpaceSet.cpp:59-64 @ v2.5.2); port of `operator-` (ColorSpaceSet.cpp:340-355 @
// v2.5.2): a copy of each color space of `self` that `rcss` doesn't name.
static SUB: Function = operator_def!("__sub__", COLOR_SPACE_SET, |py, o, a, convert| {
    let lcss = this_shared!(o, a, Vec<ColorSpaceRc>);
    let rcss = other_set!(a, convert);
    let css: SetRc = Shared::new(Vec::new());
    let items = lcss.lock().clone();
    for cs in &items {
        let Some(rcss) = &rcss else {
            return Err(null_object("ColorSpaceSet.__sub__"));
        };
        if !has(rcss, &names(cs).0) {
            add(py, &css, cs)?;
        }
    }
    out(py, COLOR_SPACE_SET, &css)
});

// .def("__or__", [](self, rcss) { return self || rcss; }, py::is_operator())
// (PyColorSpaceSet.cpp:65-70 @ v2.5.2); port of `operator||` (ColorSpaceSet.cpp:314-320 @
// v2.5.2): a copy of `self`, then the color spaces of `rcss`.
static OR: Function = operator_def!("__or__", COLOR_SPACE_SET, |py, o, a, convert| {
    let lcss = this_shared!(o, a, Vec<ColorSpaceRc>);
    let rcss = other_set!(a, convert);
    let css = copy(py, &lcss)?;
    let Some(rcss) = rcss else {
        return Err(null_object("ColorSpaceSet.__or__"));
    };
    add_all(py, &css, &rcss)?;
    out(py, COLOR_SPACE_SET, &css)
});

// .def("__and__", [](self, rcss) { return self && rcss; }, py::is_operator())
// (PyColorSpaceSet.cpp:71-76 @ v2.5.2); port of `operator&&` (ColorSpaceSet.cpp:322-338 @
// v2.5.2): a copy of each color space of `rcss` that `self` names.
static AND: Function = operator_def!("__and__", COLOR_SPACE_SET, |py, o, a, convert| {
    let lcss = this_shared!(o, a, Vec<ColorSpaceRc>);
    let Some(rcss) = other_set!(a, convert) else {
        return Err(null_object("ColorSpaceSet.__and__"));
    };
    let css: SetRc = Shared::new(Vec::new());
    let items = rcss.lock().clone();
    for cs in &items {
        if has(&lcss, &names(cs).0) {
            add(py, &css, cs)?;
        }
    }
    out(py, COLOR_SPACE_SET, &css)
});

// .def("getColorSpaceNames", ...), .def("getColorSpaces", ...) (PyColorSpaceSet.cpp:78-85 @
// v2.5.2).
static GET_COLOR_SPACE_NAMES: Function = method_def!(
    "getColorSpaceNames",
    [SELF],
    NAME_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        Ok(Some(
            new_iterator(py, NAME_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);
static GET_COLOR_SPACES: Function = method_def!(
    "getColorSpaces",
    [SELF],
    COLOR_SPACE_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        Ok(Some(
            new_iterator(py, COLOR_SPACE_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

/// A returned `ConstColorSpaceRcPtr`: `None` for a null pointer, else the set's own color
/// space's object.
fn color_space_out(py: Python<'_>, cs: Option<&ColorSpaceRc>) -> Out {
    match cs {
        Some(cs) => out(py, COLOR_SPACE, cs),
        None => none(py),
    }
}

// .def("getColorSpace", &ColorSpaceSet::getColorSpace, "name"_a), .def("hasColorSpace", ...,
// "name"_a) (PyColorSpaceSet.cpp:86-89 @ v2.5.2).
static GET_COLOR_SPACE: Function = method_def!(
    "getColorSpace",
    [SELF, Param::new("name", "str")],
    COLOR_SPACE,
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        let CharPtr(name) = crate::arg!(a, 1, convert, CharPtr);
        let current = rc.lock().clone();
        let i = index(&current, name.as_deref());
        color_space_out(py, usize::try_from(i).ok().and_then(|i| current.get(i)))
    }
);
static HAS_COLOR_SPACE: Function = method_def!(
    "hasColorSpace",
    [SELF, Param::new("name", "str")],
    "bool",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        let CharPtr(name) = crate::arg!(a, 1, convert, CharPtr);
        let current = rc.lock().clone();
        ok(py, index(&current, name.as_deref()) != -1)
    }
);

// .def("addColorSpace", ..., "colorSpace"_a), .def("addColorSpaces", ..., "colorSpaces"_a)
// (PyColorSpaceSet.cpp:90-93 @ v2.5.2).
static ADD_COLOR_SPACE: Function = method_def!(
    "addColorSpace",
    [SELF, Param::new("colorSpace", COLOR_SPACE)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        let Some(cs) = holder_arg::<ColorSpaceNode>(&a[1], COLOR_SPACE, convert)? else {
            return Ok(None);
        };
        let Some(cs) = cs else {
            return Err(null_object("ColorSpaceSet.addColorSpace"));
        };
        add(py, &rc, &cs)?;
        none(py)
    }
);
static ADD_COLOR_SPACES: Function = method_def!(
    "addColorSpaces",
    [SELF, Param::new("colorSpaces", COLOR_SPACE_SET)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        let Some(other) = other_set!(a, convert) else {
            return Err(null_object("ColorSpaceSet.addColorSpaces"));
        };
        add_all(py, &rc, &other)?;
        none(py)
    }
);

// .def("removeColorSpace", ..., "colorSpace"_a), .def("removeColorSpaces", ...,
// "colorSpaces"_a), .def("clearColorSpaces", ...) (PyColorSpaceSet.cpp:94-99 @ v2.5.2).
static REMOVE_COLOR_SPACE: Function = method_def!(
    "removeColorSpace",
    [SELF, Param::new("colorSpace", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        let CharPtr(name) = crate::arg!(a, 1, convert, CharPtr);
        remove(&rc, name.as_deref());
        none(py)
    }
);
static REMOVE_COLOR_SPACES: Function = method_def!(
    "removeColorSpaces",
    [SELF, Param::new("colorSpaces", COLOR_SPACE_SET)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        let Some(other) = other_set!(a, convert) else {
            return Err(null_object("ColorSpaceSet.removeColorSpaces"));
        };
        // Port of `ColorSpaceSet::Impl::remove(const Impl &)` (ColorSpaceSet.cpp:198-204 @
        // v2.5.2), whose loop over the other set's vector erases from it when it is the set
        // itself (U-124).
        if other.same(&rc) && !rc.lock().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "ColorSpaceSet.removeColorSpaces: the set itself, whose vector upstream erases \
                 from while it loops over it (docs/improvements.md, U-124)",
            ));
        }
        let items = other.lock().clone();
        for cs in &items {
            let name = names(cs).0;
            remove(&rc, Some(&name));
        }
        none(py)
    }
);
static CLEAR_COLOR_SPACES: Function =
    method_def!("clearColorSpaces", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, Vec<ColorSpaceRc>);
        rc.lock().clear();
        none(py)
    });

// ---------------------------------------------------------------------------------------------
// The iterators (PyColorSpaceSet.cpp:101-127 @ v2.5.2)

/// The set an iterator of `self` walks, or the next overload.
macro_rules! iter_rc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<SetRc>()?.clone();
        (it, rc)
    }};
}

fn num(rc: &SetRc) -> i32 {
    rc.lock().len() as i32
}

/// `getColorSpaceNameByIndex(i)`: `None` outside the set (a null pointer).
fn name_at(py: Python<'_>, rc: &SetRc, i: i32) -> Out {
    let cs = usize::try_from(i)
        .ok()
        .and_then(|i| rc.lock().get(i).cloned());
    let name = cs.map(|cs| names(&cs).0);
    Ok(Some(
        crate::dispatch::char_ptr_to_py(py, name.as_deref())?.unbind(),
    ))
}

/// `getColorSpaceByIndex(i)`: `None` outside the set.
fn color_space_at(py: Python<'_>, rc: &SetRc, i: i32) -> Out {
    let cs = usize::try_from(i)
        .ok()
        .and_then(|i| rc.lock().get(i).cloned());
    color_space_out(py, cs.as_ref())
}

/// The four methods of an iterator class.
macro_rules! iterator {
    ($self:expr, $qualified:expr, $ret:expr, $item:expr,
     $len:ident, $getitem:ident, $iter:ident, $next:ident) => {
        static $len: Function = method_def!("__len__", [$self], "int", |py, o, a, _| {
            let (_, rc) = iter_rc!(o, a);
            ok(py, num(&rc))
        });

        static $getitem: Function = method_def!(
            "__getitem__",
            [$self, Param::new("arg0", INT)],
            $ret,
            |py, o, a, convert| {
                let (_, rc) = iter_rc!(o, a);
                let i = crate::arg!(a, 1, convert, i32);
                check_index(i, num(&rc))?;
                $item(py, &rc, i)
            }
        );

        static $iter: Function = method_def!("__iter__", [$self], $qualified, |_, o, a, _| {
            let _ = iter_rc!(o, a);
            Ok(Some(a[0].clone().unbind()))
        });

        static $next: Function = method_def!("__next__", [$self], $ret, |py, o, a, _| {
            let (it, rc) = iter_rc!(o, a);
            let i = it.get().next_index(num(&rc))?;
            $item(py, &rc, i)
        });
    };
}

const NAME_SELF: Param = Param::new("self", NAME_ITERATOR);
const CS_SELF: Param = Param::new("self", COLOR_SPACE_ITERATOR);

iterator!(
    NAME_SELF,
    NAME_ITERATOR,
    "str",
    name_at,
    NAME_LEN,
    NAME_GETITEM,
    NAME_ITER,
    NAME_NEXT
);
iterator!(
    CS_SELF,
    COLOR_SPACE_ITERATOR,
    COLOR_SPACE,
    color_space_at,
    CS_LEN,
    CS_GETITEM,
    CS_ITER,
    CS_NEXT
);

/// Port of `bindPyColorSpaceSet` (src/bindings/python/PyColorSpaceSet.cpp:28-128 @ v2.5.2), in
/// its order: the iterator classes nested in the class (34-43), the class's methods (45-99,
/// `__eq__` followed by the `__hash__` None pybind11 sets for a class that defines `__eq__`),
/// the iterators' (101-127).
pub(crate) fn bind_py_color_space_set(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, COLOR_SPACE_SET)?;
    let names = nested_class(&cls, "ColorSpaceSet", "ColorSpaceNameIterator")?;
    let spaces = nested_class(&cls, "ColorSpaceSet", "ColorSpaceIterator")?;
    def(&cls, &INIT)?;
    def(&cls, &DEEPCOPY)?;
    def(&cls, &EQ)?;
    cls.setattr("__hash__", py.None())?;
    for f in [
        &NE,
        &SUB,
        &OR,
        &AND,
        &GET_COLOR_SPACE_NAMES,
        &GET_COLOR_SPACES,
        &GET_COLOR_SPACE,
        &HAS_COLOR_SPACE,
        &ADD_COLOR_SPACE,
        &ADD_COLOR_SPACES,
        &REMOVE_COLOR_SPACE,
        &REMOVE_COLOR_SPACES,
        &CLEAR_COLOR_SPACES,
    ] {
        def(&cls, f)?;
    }
    for f in [&NAME_LEN, &NAME_GETITEM, &NAME_ITER, &NAME_NEXT] {
        def(&names, f)?;
    }
    for f in [&CS_LEN, &CS_GETITEM, &CS_ITER, &CS_NEXT] {
        def(&spaces, f)?;
    }
    Ok(())
}
