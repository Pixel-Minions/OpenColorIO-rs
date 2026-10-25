// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyFileRules` (src/bindings/python/PyFileRules.cpp @ v2.5.2): the class `FileRules` and
//! the module's `DEFAULT_RULE_NAME` and `FILE_PATH_SEARCH_RULE_NAME` (chunk 6.3d). Its objects
//! hold their rules as pybind11 holds a `FileRulesRcPtr` ([`Shared`]).
//!
//! Every `const char *` parameter takes `None` as a null pointer, which upstream's file rules
//! read as the empty string (`!s || !*s`, FileRules.cpp:305-470 @ v2.5.2), but for the rule
//! name `getIndexForRule` compares: `Platform::Strcasecmp`'s error, the rules never being
//! empty.

use ocio::{ConfigRules, FileRules};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, Overload, Param};
use crate::py_context::null_comparison;
use crate::py_open_color_io::py_result;
use crate::py_transform::{INT, class, def, none, ok, string_out};
use crate::shared::{Held, Shared, construct, out};
use crate::{method_def, q, this_held};

/// File rules shared by the Python objects that hold them: their own, or a config's
/// (`Config.getFileRules`).
pub(crate) type FileRulesRc = Held<FileRules, ConfigRules<FileRules>>;

pub(crate) const FILE_RULES: &str = q!("FileRules");
const SELF: Param = Param::new("self", FILE_RULES);
const RULE_INDEX: Param = Param::new("ruleIndex", INT);

/// A `const char *` argument's bytes, a null pointer as the empty string.
fn or_empty(s: CharPtr) -> Vec<u8> {
    s.0.unwrap_or_default()
}

// .def(py::init(&FileRules::Create)) (PyFileRules.cpp:16-17 @ v2.5.2).
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
                FILE_RULES,
                FileRulesRc::Own(Shared::new(FileRules::new())),
            )?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("__deepcopy__", [](const ConstFileRulesRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyFileRules.cpp:19-23 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    FILE_RULES,
    |py, o, a, _| {
        let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let copy = rc.lock().clone();
        out(py, FILE_RULES, &FileRulesRc::Own(Shared::new(copy)))
    }
);

// .def("getNumEntries", &FileRules::getNumEntries) (PyFileRules.cpp:25-26 @ v2.5.2).
static GET_NUM_ENTRIES: Function = method_def!("getNumEntries", [SELF], "int", |py, o, a, _| {
    let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
    let n = rc.lock().num_entries();
    ok(py, n)
});

// .def("getIndexForRule", &FileRules::getIndexForRule, "ruleName"_a) (PyFileRules.cpp:27-28 @
// v2.5.2).
static GET_INDEX_FOR_RULE: Function = method_def!(
    "getIndexForRule",
    [SELF, Param::new("ruleName", "str")],
    "int",
    |py, o, a, convert| {
        let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
        let Some(name) = crate::arg!(a, 1, convert, CharPtr).0 else {
            return Err(null_comparison(py));
        };
        let index = py_result(py, rc.lock().index_for_rule(&name))?;
        ok(py, index)
    }
);

/// A getter `(ruleIndex) -> str` of the rules.
macro_rules! rule_getter {
    ($name:literal, $get:expr) => {
        method_def!($name, [SELF, RULE_INDEX], "str", |py, o, a, convert| {
            let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
            let i = crate::arg!(a, 1, convert, u64) as usize;
            let text: Vec<u8> = py_result(py, $get(&*rc.lock(), i))?;
            string_out(py, c_str(&text))
        })
    };
}

/// A setter `(ruleIndex, <param>: str)` of the rules.
macro_rules! rule_setter {
    ($name:literal, $param:literal, $set:expr) => {
        method_def!(
            $name,
            [SELF, RULE_INDEX, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                let s = or_empty(crate::arg!(a, 2, convert, CharPtr));
                py_result(py, $set(&mut *rc.lock(), i, &s))?;
                none(py)
            }
        )
    };
}

// .def("getName", &FileRules::getName, "ruleIndex"_a) (PyFileRules.cpp:29-30 @ v2.5.2).
static GET_NAME: Function =
    rule_getter!("getName", |r: &FileRules, i| r.name(i).map(<[u8]>::to_vec));
// .def("getPattern", ...), .def("setPattern", ..., "ruleIndex"_a, "pattern"_a)
// (PyFileRules.cpp:31-34 @ v2.5.2).
static GET_PATTERN: Function = rule_getter!("getPattern", |r: &FileRules, i| r
    .pattern(i)
    .map(<[u8]>::to_vec));
static SET_PATTERN: Function =
    rule_setter!("setPattern", "pattern", |r: &mut FileRules, i, s: &[u8]| r
        .set_pattern(i, s));
// .def("getExtension", ...), .def("setExtension", ..., "ruleIndex"_a, "extension"_a)
// (PyFileRules.cpp:35-38 @ v2.5.2).
static GET_EXTENSION: Function = rule_getter!("getExtension", |r: &FileRules, i| r
    .extension(i)
    .map(<[u8]>::to_vec));
static SET_EXTENSION: Function = rule_setter!(
    "setExtension",
    "extension",
    |r: &mut FileRules, i, s: &[u8]| r.set_extension(i, s)
);
// .def("getRegex", ...), .def("setRegex", ..., "ruleIndex"_a, "regex"_a) (PyFileRules.cpp:
// 39-42 @ v2.5.2).
static GET_REGEX: Function = rule_getter!("getRegex", |r: &FileRules, i| r
    .regex(i)
    .map(<[u8]>::to_vec));
static SET_REGEX: Function = rule_setter!("setRegex", "regex", |r: &mut FileRules, i, s: &[u8]| r
    .set_regex(i, s));
// .def("getColorSpace", ...), .def("setColorSpace", ..., "ruleIndex"_a, "colorSpace"_a)
// (PyFileRules.cpp:43-46 @ v2.5.2).
static GET_COLOR_SPACE: Function =
    rule_getter!("getColorSpace", |r: &FileRules, i| r.color_space(i));
static SET_COLOR_SPACE: Function = rule_setter!(
    "setColorSpace",
    "colorSpace",
    |r: &mut FileRules, i, s: &[u8]| r.set_color_space(i, s)
);

// .def("getNumCustomKeys", &FileRules::getNumCustomKeys, "ruleIndex"_a) (PyFileRules.cpp:47-48
// @ v2.5.2).
static GET_NUM_CUSTOM_KEYS: Function = method_def!(
    "getNumCustomKeys",
    [SELF, RULE_INDEX],
    "int",
    |py, o, a, convert| {
        let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
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
                let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                let k = crate::arg!(a, 2, convert, u64) as usize;
                let text: Vec<u8> = py_result(py, $get(&*rc.lock(), i, k))?;
                string_out(py, c_str(&text))
            }
        )
    };
}

// .def("getCustomKeyName", ...), .def("getCustomKeyValue", ..., "ruleIndex"_a, "key"_a)
// (PyFileRules.cpp:49-52 @ v2.5.2).
static GET_CUSTOM_KEY_NAME: Function = key_getter!("getCustomKeyName", |r: &FileRules, i, k| r
    .custom_key_name(i, k)
    .map(<[u8]>::to_vec));
static GET_CUSTOM_KEY_VALUE: Function = key_getter!("getCustomKeyValue", |r: &FileRules, i, k| r
    .custom_key_value(i, k)
    .map(<[u8]>::to_vec));

// .def("setCustomKey", &FileRules::setCustomKey, "ruleIndex"_a, "key"_a, "value"_a)
// (PyFileRules.cpp:53-54 @ v2.5.2).
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
        let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
        let i = crate::arg!(a, 1, convert, u64) as usize;
        let key = or_empty(crate::arg!(a, 2, convert, CharPtr));
        let value = or_empty(crate::arg!(a, 3, convert, CharPtr));
        py_result(py, rc.lock().set_custom_key(i, &key, &value))?;
        none(py)
    }
);

// .def("insertRule", (void (FileRules::*)(size_t, const char *, const char *, const char *,
// const char *)), "ruleIndex"_a, "name"_a, "colorSpace"_a, "pattern"_a, "extension"_a),
// .def("insertRule", (void (FileRules::*)(size_t, const char *, const char *, const char *)),
// "ruleIndex"_a, "name"_a, "colorSpace"_a, "regex"_a) (PyFileRules.cpp:55-64 @ v2.5.2).
static INSERT_RULE: Function = Function {
    name: "insertRule",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[
                SELF,
                RULE_INDEX,
                Param::new("name", "str"),
                Param::new("colorSpace", "str"),
                Param::new("pattern", "str"),
                Param::new("extension", "str"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                let name = or_empty(crate::arg!(a, 2, convert, CharPtr));
                let cs = or_empty(crate::arg!(a, 3, convert, CharPtr));
                let pattern = or_empty(crate::arg!(a, 4, convert, CharPtr));
                let extension = or_empty(crate::arg!(a, 5, convert, CharPtr));
                py_result(
                    py,
                    rc.lock().insert_rule(i, &name, &cs, &pattern, &extension),
                )?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                RULE_INDEX,
                Param::new("name", "str"),
                Param::new("colorSpace", "str"),
                Param::new("regex", "str"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
                let i = crate::arg!(a, 1, convert, u64) as usize;
                let name = or_empty(crate::arg!(a, 2, convert, CharPtr));
                let cs = or_empty(crate::arg!(a, 3, convert, CharPtr));
                let regex = or_empty(crate::arg!(a, 4, convert, CharPtr));
                py_result(py, rc.lock().insert_regex_rule(i, &name, &cs, &regex))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

/// A method `(ruleIndex) -> None` of the rules.
macro_rules! rule_action {
    ($name:literal, $act:expr) => {
        method_def!($name, [SELF, RULE_INDEX], "None", |py, o, a, convert| {
            let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
            let i = crate::arg!(a, 1, convert, u64) as usize;
            py_result(py, $act(&mut *rc.lock(), i))?;
            none(py)
        })
    };
}

// .def("insertPathSearchRule", &FileRules::insertPathSearchRule, "ruleIndex"_a)
// (PyFileRules.cpp:65-66 @ v2.5.2).
static INSERT_PATH_SEARCH_RULE: Function =
    rule_action!("insertPathSearchRule", |r: &mut FileRules, i| r
        .insert_path_search_rule(i));

// .def("setDefaultRuleColorSpace", &FileRules::setDefaultRuleColorSpace, "colorSpace"_a)
// (PyFileRules.cpp:67-68 @ v2.5.2).
static SET_DEFAULT_RULE_COLOR_SPACE: Function = method_def!(
    "setDefaultRuleColorSpace",
    [SELF, Param::new("colorSpace", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
        let cs = or_empty(crate::arg!(a, 1, convert, CharPtr));
        py_result(py, rc.lock().set_default_rule_color_space(&cs))?;
        none(py)
    }
);

// .def("removeRule", ...), .def("increaseRulePriority", ...), .def("decreaseRulePriority", ...),
// each "ruleIndex"_a (PyFileRules.cpp:69-74 @ v2.5.2).
static REMOVE_RULE: Function = rule_action!("removeRule", |r: &mut FileRules, i| r.remove_rule(i));
static INCREASE_RULE_PRIORITY: Function =
    rule_action!("increaseRulePriority", |r: &mut FileRules, i| r
        .increase_rule_priority(i));
static DECREASE_RULE_PRIORITY: Function =
    rule_action!("decreaseRulePriority", |r: &mut FileRules, i| r
        .decrease_rule_priority(i));

// .def("isDefault", &FileRules::isDefault) (PyFileRules.cpp:75 @ v2.5.2).
static IS_DEFAULT: Function = method_def!("isDefault", [SELF], "bool", |py, o, a, _| {
    let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
    let d = rc.lock().is_default();
    ok(py, d)
});

// defRepr(clsFileRules) (PyFileRules.cpp:77 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_held!(o, a, FileRules, ConfigRules<FileRules>);
    let text = rc.lock().to_bytes();
    string_out(py, &text)
});

/// Port of `bindPyFileRules` (src/bindings/python/PyFileRules.cpp:10-81 @ v2.5.2), in its order.
pub(crate) fn bind_py_file_rules(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), FILE_RULES)?;
    for f in [
        &INIT,
        &DEEPCOPY,
        &GET_NUM_ENTRIES,
        &GET_INDEX_FOR_RULE,
        &GET_NAME,
        &GET_PATTERN,
        &SET_PATTERN,
        &GET_EXTENSION,
        &SET_EXTENSION,
        &GET_REGEX,
        &SET_REGEX,
        &GET_COLOR_SPACE,
        &SET_COLOR_SPACE,
        &GET_NUM_CUSTOM_KEYS,
        &GET_CUSTOM_KEY_NAME,
        &GET_CUSTOM_KEY_VALUE,
        &SET_CUSTOM_KEY,
        &INSERT_RULE,
        &INSERT_PATH_SEARCH_RULE,
        &SET_DEFAULT_RULE_COLOR_SPACE,
        &REMOVE_RULE,
        &INCREASE_RULE_PRIORITY,
        &DECREASE_RULE_PRIORITY,
        &IS_DEFAULT,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    m.setattr("DEFAULT_RULE_NAME", FileRules::DEFAULT_RULE_NAME)?;
    m.setattr(
        "FILE_PATH_SEARCH_RULE_NAME",
        FileRules::FILE_PATH_SEARCH_RULE_NAME,
    )?;
    Ok(())
}
