// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyViewingRules` (src/bindings/python/PyViewingRules.cpp @ v2.5.2): the class
//! `ViewingRules` and its two iterator classes (chunk 6.3d). Its objects hold their rules as
//! pybind11 holds a `ViewingRulesRcPtr` ([`Shared`]), which the iterators share, each with the
//! rule index it was made for (`PyIterator<ViewingRulesRcPtr, ..., size_t>`).
//!
//! Every `const char *` parameter takes `None` as a null pointer, which upstream's viewing rules
//! read as the empty string, but for the rule name `getIndexForRule` compares:
//! `Platform::Strcasecmp`'s error, or, without rules, U-123 (upstream writes the null pointer
//! into its message).

use ocio::ViewingRules;
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, Overload, Param, char_ptr_to_py};
use crate::py_context::{null_comparison, null_string};
use crate::py_open_color_io::py_result;
use crate::py_transform::{INT, Out, class, def, nested_class, none, ok, string_out};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Held, Shared, construct, out};
use crate::{method_def, q, this_held, this_iter};

pub(crate) const VIEWING_RULES: &str = q!("ViewingRules");
const COLOR_SPACE_ITERATOR: &str = q!("ViewingRules.ViewingRuleColorSpaceIterator");
const ENCODING_ITERATOR: &str = q!("ViewingRules.ViewingRuleEncodingIterator");
const SELF: Param = Param::new("self", VIEWING_RULES);
const RULE_INDEX: Param = Param::new("ruleIndex", INT);

pub(crate) type RulesRc = Held<ViewingRules, ocio::ConfigRules<ViewingRules>>;

/// A `const char *` argument's bytes, a null pointer as the empty string.
fn or_empty(s: CharPtr) -> Vec<u8> {
    s.0.unwrap_or_default()
}

// .def(py::init(&ViewingRules::Create)) (PyViewingRules.cpp:35-36 @ v2.5.2).
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
                VIEWING_RULES,
                RulesRc::Own(Shared::new(ViewingRules::new())),
            )?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("__deepcopy__", [](const ConstViewingRulesRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyViewingRules.cpp:38-42 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    VIEWING_RULES,
    |py, o, a, _| {
        let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let copy = rc.lock().clone();
        out(py, VIEWING_RULES, &RulesRc::Own(Shared::new(copy)))
    }
);

// .def("getNumEntries", &ViewingRules::getNumEntries) (PyViewingRules.cpp:44-45 @ v2.5.2).
static GET_NUM_ENTRIES: Function = method_def!("getNumEntries", [SELF], "int", |py, o, a, _| {
    let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
    let n = rc.lock().num_entries();
    ok(py, n)
});

// .def("getIndexForRule", &ViewingRules::getIndexForRule, "ruleName"_a) (PyViewingRules.cpp:
// 46-47 @ v2.5.2).
static GET_INDEX_FOR_RULE: Function = method_def!(
    "getIndexForRule",
    [SELF, Param::new("ruleName", "str")],
    "int",
    |py, o, a, convert| {
        let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
        let Some(name) = crate::arg!(a, 1, convert, CharPtr).0 else {
            return Err(if rc.lock().num_entries() > 0 {
                null_comparison(py)
            } else {
                null_string("getIndexForRule(ruleName)")
            });
        };
        let index = py_result(py, rc.lock().index_for_rule(&name))?;
        ok(py, index)
    }
);

// .def("getName", &ViewingRules::getName, "ruleIndex"_a) (PyViewingRules.cpp:48-49 @ v2.5.2).
static GET_NAME: Function =
    method_def!("getName", [SELF, RULE_INDEX], "str", |py, o, a, convert| {
        let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
        let i = crate::arg!(a, 1, convert, u64) as usize;
        let name = py_result(py, rc.lock().name(i).map(<[u8]>::to_vec))?;
        string_out(py, c_str(&name))
    });

/// A method returning the iterator class `$qualified` over the rule `ruleIndex`.
macro_rules! iterator_getter {
    ($name:literal, $qualified:expr) => {
        method_def!(
            $name,
            [SELF, RULE_INDEX],
            $qualified,
            |py, o, a, convert| {
                let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                Ok(Some(
                    new_iterator(py, $qualified, Some(Box::new((rc, i))))?.unbind(),
                ))
            }
        )
    };
}

/// A method `(ruleIndex, <param>: str) -> None`.
macro_rules! rule_text_action {
    ($name:literal, $param:literal, $act:expr) => {
        method_def!(
            $name,
            [SELF, RULE_INDEX, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                let s = or_empty(crate::arg!(a, 2, convert, CharPtr));
                py_result(py, $act(&mut *rc.lock(), i, &s))?;
                none(py)
            }
        )
    };
}

/// A method `(ruleIndex, <param>: int) -> None`.
macro_rules! rule_index_action {
    ($name:literal, $param:literal, $act:expr) => {
        method_def!(
            $name,
            [SELF, RULE_INDEX, Param::new($param, INT)],
            "None",
            |py, o, a, convert| {
                let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                let k = crate::arg!(a, 2, convert, u64) as usize;
                py_result(py, $act(&mut *rc.lock(), i, k))?;
                none(py)
            }
        )
    };
}

// .def("getColorSpaces", [](ViewingRulesRcPtr & self, size_t ruleIndex) {...}, "ruleIndex"_a),
// .def("addColorSpace", ..., "ruleIndex"_a, "colorSpaceName"_a), .def("removeColorSpace", ...,
// "ruleIndex"_a, "colorSpaceIndex"_a) (PyViewingRules.cpp:50-60 @ v2.5.2).
static GET_COLOR_SPACES: Function = iterator_getter!("getColorSpaces", COLOR_SPACE_ITERATOR);
static ADD_COLOR_SPACE: Function = rule_text_action!(
    "addColorSpace",
    "colorSpaceName",
    |r: &mut ViewingRules, i, s: &[u8]| r.add_color_space(i, s)
);
static REMOVE_COLOR_SPACE: Function = rule_index_action!(
    "removeColorSpace",
    "colorSpaceIndex",
    |r: &mut ViewingRules, i, k| r.remove_color_space(i, k)
);

// .def("getEncodings", ...), .def("addEncoding", ..., "ruleIndex"_a, "encodingName"_a),
// .def("removeEncoding", ..., "ruleIndex"_a, "encodingIndex"_a) (PyViewingRules.cpp:61-70 @
// v2.5.2).
static GET_ENCODINGS: Function = iterator_getter!("getEncodings", ENCODING_ITERATOR);
static ADD_ENCODING: Function = rule_text_action!(
    "addEncoding",
    "encodingName",
    |r: &mut ViewingRules, i, s: &[u8]| r.add_encoding(i, s)
);
static REMOVE_ENCODING: Function = rule_index_action!(
    "removeEncoding",
    "encodingIndex",
    |r: &mut ViewingRules, i, k| r.remove_encoding(i, k)
);

// .def("getNumCustomKeys", &ViewingRules::getNumCustomKeys, "ruleIndex"_a)
// (PyViewingRules.cpp:71-72 @ v2.5.2).
static GET_NUM_CUSTOM_KEYS: Function = method_def!(
    "getNumCustomKeys",
    [SELF, RULE_INDEX],
    "int",
    |py, o, a, convert| {
        let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
        let i = crate::arg!(a, 1, convert, u64) as usize;
        let n = py_result(py, rc.lock().num_custom_keys(i))?;
        ok(py, n)
    }
);

/// A getter `(ruleIndex, key) -> str` of the custom keys.
macro_rules! key_getter {
    ($name:literal, $get:expr) => {
        method_def!(
            $name,
            [SELF, RULE_INDEX, Param::new("key", INT)],
            "str",
            |py, o, a, convert| {
                let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                let k = crate::arg!(a, 2, convert, u64) as usize;
                let text: Vec<u8> = py_result(py, $get(&*rc.lock(), i, k))?;
                string_out(py, c_str(&text))
            }
        )
    };
}

// .def("getCustomKeyName", ...), .def("getCustomKeyValue", ..., "ruleIndex"_a, "key"_a)
// (PyViewingRules.cpp:73-76 @ v2.5.2).
static GET_CUSTOM_KEY_NAME: Function = key_getter!("getCustomKeyName", |r: &ViewingRules, i, k| r
    .custom_key_name(i, k)
    .map(<[u8]>::to_vec));
static GET_CUSTOM_KEY_VALUE: Function =
    key_getter!("getCustomKeyValue", |r: &ViewingRules, i, k| r
        .custom_key_value(i, k)
        .map(<[u8]>::to_vec));

// .def("setCustomKey", &ViewingRules::setCustomKey, "ruleIndex"_a, "key"_a, "value"_a)
// (PyViewingRules.cpp:77-78 @ v2.5.2).
static SET_CUSTOM_KEY: Function = method_def!(
    "setCustomKey",
    [
        SELF,
        RULE_INDEX,
        Param::new("key", "str"),
        Param::new("value", "str")
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
        let i = crate::arg!(a, 1, convert, u64) as usize;
        let key = or_empty(crate::arg!(a, 2, convert, CharPtr));
        let value = or_empty(crate::arg!(a, 3, convert, CharPtr));
        py_result(py, rc.lock().set_custom_key(i, &key, &value))?;
        none(py)
    }
);

// .def("insertRule", (void (ViewingRules::*)(size_t, const char *)), "ruleIndex"_a, "name"_a)
// (PyViewingRules.cpp:79-82 @ v2.5.2).
static INSERT_RULE: Function =
    rule_text_action!("insertRule", "name", |r: &mut ViewingRules, i, s: &[u8]| r
        .insert_rule(i, s));

// .def("removeRule", &ViewingRules::removeRule, "ruleIndex"_a) (PyViewingRules.cpp:83-84 @
// v2.5.2).
static REMOVE_RULE: Function = method_def!(
    "removeRule",
    [SELF, RULE_INDEX],
    "None",
    |py, o, a, convert| {
        let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
        let i = crate::arg!(a, 1, convert, u64) as usize;
        py_result(py, rc.lock().remove_rule(i))?;
        none(py)
    }
);

// defRepr(clsViewingRules) (PyViewingRules.cpp:86 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_held!(o, a, ViewingRules, ocio::ConfigRules<ViewingRules>);
    let text = rc.lock().to_bytes();
    string_out(py, &text)
});

// ---------------------------------------------------------------------------------------------
// The iterators (PyViewingRules.cpp:88-130 @ v2.5.2)

/// The rules an iterator of `self` walks and its rule index, or the next overload.
macro_rules! iter_rc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let (rc, rule) = it.get().obj::<(RulesRc, usize)>()?.clone();
        (it, rc, rule)
    }};
}

/// `getNumColorSpaces(ruleIndex)` (its error for a rule that isn't there).
fn num_color_spaces(py: Python<'_>, rc: &RulesRc, rule: usize) -> PyResult<usize> {
    py_result(py, rc.lock().num_color_spaces(rule))
}

fn num_encodings(py: Python<'_>, rc: &RulesRc, rule: usize) -> PyResult<usize> {
    py_result(py, rc.lock().num_encodings(rule))
}

/// `getColorSpace(ruleIndex, i)`: a C string, or `None` for a null pointer.
fn color_space(py: Python<'_>, rc: &RulesRc, rule: usize, i: usize) -> Out {
    let cs = py_result(
        py,
        rc.lock()
            .color_space(rule, i)
            .map(|c| c.map(<[u8]>::to_vec)),
    )?;
    Ok(Some(char_ptr_to_py(py, cs.as_deref().map(c_str))?.unbind()))
}

fn encoding(py: Python<'_>, rc: &RulesRc, rule: usize, i: usize) -> Out {
    let e = py_result(
        py,
        rc.lock().encoding(rule, i).map(|c| c.map(<[u8]>::to_vec)),
    )?;
    Ok(Some(char_ptr_to_py(py, e.as_deref().map(c_str))?.unbind()))
}

/// The four methods of an iterator class: `__len__` (`$num`), `__getitem__` (its `size_t`
/// index and the count cast to `int` for the check), `__iter__`, `__next__` (the `int` index
/// given back as a `size_t`).
macro_rules! iterator {
    ($self:expr, $qualified:expr, $num:expr, $item:expr,
     $len:ident, $getitem:ident, $iter:ident, $next:ident) => {
        static $len: Function = method_def!("__len__", [$self], "int", |py, o, a, _| {
            let (_, rc, rule) = iter_rc!(o, a);
            ok(py, $num(py, &rc, rule)?)
        });

        static $getitem: Function = method_def!(
            "__getitem__",
            [$self, Param::new("arg0", INT)],
            "str",
            |py, o, a, convert| {
                let (_, rc, rule) = iter_rc!(o, a);
                let i = crate::arg!(a, 1, convert, u64);
                check_index(i as i32, $num(py, &rc, rule)? as i32)?;
                $item(py, &rc, rule, i as usize)
            }
        );

        static $iter: Function = method_def!("__iter__", [$self], $qualified, |_, o, a, _| {
            let _ = iter_rc!(o, a);
            Ok(Some(a[0].clone().unbind()))
        });

        static $next: Function = method_def!("__next__", [$self], "str", |py, o, a, _| {
            let (it, rc, rule) = iter_rc!(o, a);
            let i = it.get().next_index($num(py, &rc, rule)? as i32)?;
            $item(py, &rc, rule, i as isize as usize)
        });
    };
}

const CS_SELF: Param = Param::new("self", COLOR_SPACE_ITERATOR);
const ENC_SELF: Param = Param::new("self", ENCODING_ITERATOR);

iterator!(
    CS_SELF,
    COLOR_SPACE_ITERATOR,
    num_color_spaces,
    color_space,
    CS_LEN,
    CS_GETITEM,
    CS_ITER,
    CS_NEXT
);
iterator!(
    ENC_SELF,
    ENCODING_ITERATOR,
    num_encodings,
    encoding,
    ENC_LEN,
    ENC_GETITEM,
    ENC_ITER,
    ENC_NEXT
);

/// Port of `bindPyViewingRules` (src/bindings/python/PyViewingRules.cpp:22-131 @ v2.5.2), in
/// its order: the iterator classes nested in the class (24-33), the class's methods (35-84),
/// `__repr__` (86), the iterators' (88-130).
pub(crate) fn bind_py_viewing_rules(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, VIEWING_RULES)?;
    let color_spaces = nested_class(&cls, "ViewingRules", "ViewingRuleColorSpaceIterator")?;
    let encodings = nested_class(&cls, "ViewingRules", "ViewingRuleEncodingIterator")?;
    for f in [
        &INIT,
        &DEEPCOPY,
        &GET_NUM_ENTRIES,
        &GET_INDEX_FOR_RULE,
        &GET_NAME,
        &GET_COLOR_SPACES,
        &ADD_COLOR_SPACE,
        &REMOVE_COLOR_SPACE,
        &GET_ENCODINGS,
        &ADD_ENCODING,
        &REMOVE_ENCODING,
        &GET_NUM_CUSTOM_KEYS,
        &GET_CUSTOM_KEY_NAME,
        &GET_CUSTOM_KEY_VALUE,
        &SET_CUSTOM_KEY,
        &INSERT_RULE,
        &REMOVE_RULE,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    for f in [&CS_LEN, &CS_GETITEM, &CS_ITER, &CS_NEXT] {
        def(&color_spaces, f)?;
    }
    for f in [&ENC_LEN, &ENC_GETITEM, &ENC_ITER, &ENC_NEXT] {
        def(&encodings, f)?;
    }
    Ok(())
}
