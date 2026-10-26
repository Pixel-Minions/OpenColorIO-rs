// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyConfigMergingHelpers` (src/bindings/python/apphelpers/PyMergeConfigs.cpp @
//! v2.5.2): shared merge parameters, config mergers and the in-memory merge helpers.

use std::sync::Arc;

use ocio::{Config, ConfigMerger, ConfigMergingParameters, MergeStrategies};
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, Param};
use crate::enums::{EnumDef, Scalar, define_enum, export_values};
use crate::py_color_space::{COLOR_SPACE, ColorSpaceNode};
use crate::py_config::{
    CONFIG, ConfigNode, ConfigRc, config_arc, config_arg, config_out, with_config, with_two,
};
use crate::py_open_color_io::py_result;
use crate::py_transform::{INT, Out, class, def, def_static, enum_out, none, ok, string_out};
use crate::shared::{Shared, holder_arg, out};
use crate::{arg, enum_param, method_def, q, static_def, this_shared};

const PARAMS: &str = q!("ConfigMergingParameters");
const MERGER: &str = q!("ConfigMerger");
const STRATEGY: &str = q!("ConfigMergingParameters.MergeStrategies");
const SELF: Param = Param::new("self", PARAMS);
const MERGER_SELF: Param = Param::new("self", MERGER);
type ParamsRc = Shared<ConfigMergingParameters>;

/// A merger and the Python nodes of its shared parameters and output configs. Keeping the
/// nodes also preserves Python identity after the caller drops its earlier wrapper.
struct MergerNode {
    merger: ConfigMerger,
    params: Vec<ParamsRc>,
    configs: Vec<(Arc<Config>, ConfigRc)>,
}

impl MergerNode {
    fn new(merger: ConfigMerger) -> Self {
        Self {
            merger,
            params: Vec::new(),
            configs: Vec::new(),
        }
    }

    /// The library's shared parameters returned as their existing Python holder.
    fn params(&mut self, index: i32) -> Option<ParamsRc> {
        let p = self.merger.params(index)?;
        if let Some(rc) = self
            .params
            .iter()
            .find(|rc| ConfigMergingParameters::ptr_eq(&rc.lock(), &p))
        {
            return Some(rc.clone());
        }
        let rc = Shared::new(p);
        self.params.push(rc.clone());
        Some(rc)
    }

    /// The very config pointer returned by either getMergedConfig overload.
    fn config(&mut self, config: Option<Arc<Config>>) -> Option<ConfigRc> {
        let config = config?;
        if let Some((_, rc)) = self.configs.iter().find(|(c, _)| Arc::ptr_eq(c, &config)) {
            return Some(rc.clone());
        }
        let rc = Shared::new(ConfigNode::from_arc(config.clone()));
        self.configs.push((config, rc.clone()));
        Some(rc)
    }

    /// Python may edit output configs in place before the merger is copied again.
    fn snapshot(&self) -> PyResult<ConfigMerger> {
        let mut snapshot = self.merger.binding_copy();
        for (original, rc) in &self.configs {
            let current = config_arc(rc)?;
            snapshot.replace_merged_config_as_is(original, current);
        }
        Ok(snapshot)
    }
}

fn params_out(py: Python<'_>, params: ConfigMergingParameters) -> Out {
    out(py, PARAMS, &Shared::new(params))
}

fn merger_out(py: Python<'_>, merger: ConfigMerger) -> Out {
    out(py, MERGER, &Shared::new(MergerNode::new(merger)))
}

fn params_arg(obj: &Bound<'_, PyAny>, convert: bool) -> PyResult<Option<Option<ParamsRc>>> {
    holder_arg(obj, PARAMS, convert)
}

/// Upstream assigns a null const char pointer to std::string (U-221).
fn null_string(method: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{method}: None, a null string pointer the wheel dereferences (its process dies; docs/improvements.md, U-221)"
    ))
}

fn strategy(value: i64) -> PyResult<MergeStrategies> {
    use MergeStrategies::*;
    [
        PreferInput,
        PreferBase,
        InputOnly,
        BaseOnly,
        Remove,
        Unspecified,
    ]
    .into_iter()
    .find(|s| *s as i64 == value)
    .ok_or_else(|| {
        if (6..=7).contains(&value) {
            crate::py_transform::not_an_enumerator(STRATEGY, value)
        } else {
            crate::py_transform::outside_enumerators(STRATEGY, value)
        }
    })
}

// .def_static("Create", ...), .def("__deepcopy__", ..., "memo"_a)
// (PyMergeConfigs.cpp:24-31 @ v2.5.2).
static CREATE_PARAMS: Function = static_def!("Create", [], PARAMS, |py, _, _, _| {
    params_out(py, py_result(py, ConfigMergingParameters::new())?)
});
static COPY_PARAMS: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    PARAMS,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ConfigMergingParameters);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let p = py_result(py, rc.lock().create_editable_copy())?;
        params_out(py, p)
    }
);

// .def_static("Create", ...), .def_static("CreateFromFile", ..., "filepath"_a),
// .def("__deepcopy__", ..., "memo"_a) (PyMergeConfigs.cpp:172-184 @ v2.5.2).
static CREATE_MERGER: Function = static_def!("Create", [], MERGER, |py, _, _, _| {
    merger_out(py, ConfigMerger::new())
});
static FROM_FILE: Function = static_def!(
    "CreateFromFile",
    [Param::new("filepath", "str")],
    MERGER,
    |py, _, a, convert| {
        let path = arg!(a, 0, convert, CharPtr);
        let merger = py_result(
            py,
            ConfigMerger::from_file(path.0.as_deref().unwrap_or_default()),
        )?;
        merger_out(
            py,
            Arc::try_unwrap(merger).unwrap_or_else(|m| m.binding_copy()),
        )
    }
);
static COPY_MERGER: Function = method_def!(
    "__deepcopy__",
    [MERGER_SELF, Param::new("memo", "dict")],
    MERGER,
    |py, o, a, _| {
        let rc = this_shared!(o, a, MergerNode);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let source = rc.lock().snapshot()?;
        let merger = py_result(py, source.create_editable_copy())?;
        merger_out(py, merger)
    }
);

// .def("getParams", ...), .def("addParams", ...) (PyMergeConfigs.cpp:198-203 @ v2.5.2).
static GET_PARAMS: Function = method_def!(
    "getParams",
    [MERGER_SELF, Param::new("index", INT)],
    PARAMS,
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MergerNode);
        let index = arg!(a, 1, convert, i32);
        let p = rc.lock().params(index);
        match p {
            Some(p) => out(py, PARAMS, &p),
            None => none(py),
        }
    }
);
static ADD_PARAMS: Function = method_def!(
    "addParams",
    [MERGER_SELF, Param::new("params", PARAMS)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MergerNode);
        let Some(p) = params_arg(&a[1], convert)? else {
            return Ok(None);
        };
        let param = p.as_ref().map(|p| p.lock().clone());
        let mut n = rc.lock();
        n.merger.add_params(param);
        if let Some(p) = p {
            n.params.push(p);
        }
        none(py)
    }
);

// .def("mergeConfigs", &ConfigMerger::mergeConfigs) (PyMergeConfigs.cpp:204-205 @ v2.5.2).
// No binding lock spans callbacks. The source merger is read again before each iteration,
// as upstream's loop reads getNumConfigMergingParameters and m_mergeParams anew.
static EXECUTE: Function = method_def!("mergeConfigs", [MERGER_SELF], MERGER, |py, o, a, _| {
    let rc = this_shared!(o, a, MergerNode);
    let source = rc.lock().snapshot()?;
    let merger = py_result(
        py,
        source.merge_configs_with_params(|i| {
            let n = rc.lock();
            (i < n.merger.num_config_merging_parameters() as usize)
                .then(|| n.merger.params(i as i32))
        }),
    )?;
    merger_out(
        py,
        Arc::try_unwrap(merger).unwrap_or_else(|m| m.binding_copy()),
    )
});

// .def("getMergedConfig", overload_cast<>), then overload_cast<int>
// (PyMergeConfigs.cpp:208-215 @ v2.5.2).
static GET_CONFIG: Function = Function {
    name: "getMergedConfig",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        crate::dispatch::Overload {
            params: &[MERGER_SELF],
            ret: CONFIG,
            pos_only: 0,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, MergerNode);
                let mut n = rc.lock();
                let c = n.merger.last_merged_config();
                match n.config(c) {
                    Some(c) => out(py, CONFIG, &c),
                    None => none(py),
                }
            },
        },
        crate::dispatch::Overload {
            params: &[MERGER_SELF, Param::new("index", INT)],
            ret: CONFIG,
            pos_only: 0,
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, MergerNode);
                let index = arg!(a, 1, convert, i32);
                let mut n = rc.lock();
                let c = n.merger.merged_config(index);
                match n.config(c) {
                    Some(c) => out(py, CONFIG, &c),
                    None => none(py),
                }
            },
        },
    ],
};

// .def("serialize", lambda), .def("setVersion", ..., "major"_a, "minor"_a)
// (PyMergeConfigs.cpp:216-223 @ v2.5.2).
static SERIALIZE: Function = method_def!("serialize", [MERGER_SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, MergerNode);
    string_out(py, &py_result(py, rc.lock().merger.serialize())?)
});
static SET_VERSION: Function = method_def!(
    "setVersion",
    [
        MERGER_SELF,
        Param::new("major", INT),
        Param::new("minor", INT)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MergerNode);
        let major = arg!(a, 1, convert, u32);
        let minor = arg!(a, 2, convert, u32);
        rc.lock().merger.set_version(major, minor);
        none(py)
    }
);

// ConfigMergingHelpers namespace functions (PyMergeConfigs.cpp:231-247 @ v2.5.2).
static MERGE_CONFIGS: Function = static_def!(
    "MergeConfigs",
    [
        Param::new("params", PARAMS),
        Param::new("baseConfig", CONFIG),
        Param::new("inputConfig", CONFIG)
    ],
    CONFIG,
    |py, _, a, convert| {
        let Some(p) = params_arg(&a[0], convert)? else {
            return Ok(None);
        };
        let Some(base) = config_arg(&a[1], convert)? else {
            return Ok(None);
        };
        let Some(input) = config_arg(&a[2], convert)? else {
            return Ok(None);
        };
        let p = p.map(|p| p.lock().clone());
        let config = match (&base, &input) {
            (Some(base), Some(input)) => with_two(base, input, |base, input| {
                py_result(
                    py,
                    ocio::config_merging_helpers::merge_configs(
                        p.as_ref(),
                        Some(base),
                        Some(input),
                    ),
                )
            })?,
            _ => py_result(
                py,
                ocio::config_merging_helpers::merge_configs(p.as_ref(), None, None),
            )?,
        };
        config_out(py, config)
    }
);
static MERGE_COLOR_SPACE: Function = static_def!(
    "MergeColorSpace",
    [
        Param::new("params", PARAMS),
        Param::new("baseConfig", CONFIG),
        Param::new("colorspace", COLOR_SPACE)
    ],
    CONFIG,
    |py, _, a, convert| {
        let Some(p) = params_arg(&a[0], convert)? else {
            return Ok(None);
        };
        let Some(base) = config_arg(&a[1], convert)? else {
            return Ok(None);
        };
        let Some(cs) = holder_arg::<ColorSpaceNode>(&a[2], COLOR_SPACE, convert)? else {
            return Ok(None);
        };
        // Upstream checks these pointers before it dereferences params (U-176).
        if base.is_none() || cs.is_none() {
            return Err(crate::py_open_color_io::to_py_err(
                py,
                &ocio::Exception::new("The base config or color space object was not set."),
            ));
        }
        let cs = cs.expect("checked above").lock().materialize()?;
        let p = p.map(|p| p.lock().clone());
        let config = with_config(base.as_ref().expect("checked above"), |base| {
            let Some(p) = p else {
                // Config::Create/addColorSpace and the params' Create precede the null read.
                let mut input = py_result(py, Config::new())?;
                py_result(py, input.add_color_space(&cs))?;
                let _ = py_result(py, ConfigMergingParameters::new())?;
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "ConfigMergingHelpers.MergeColorSpace: None parameters, a null pointer the wheel dereferences (docs/improvements.md, U-176)",
                ));
            };
            py_result(
                py,
                ocio::config_merging_helpers::merge_color_space(&p, Some(&base.config), Some(&cs)),
            )
        })?;
        config_out(py, config)
    }
);

/// Port of `bindPyConfigMergingHelpers` (PyMergeConfigs.cpp:12-248 @ v2.5.2), in source order.
pub(crate) fn bind_py_config_merging_helpers(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let sub = crate::pybind11::def_submodule(m, "ConfigMergingHelpers")?;
    let cls = class(py, PARAMS)?;
    let strategies = define_enum(
        &cls,
        Some("ConfigMergingParameters"),
        &EnumDef {
            name: "MergeStrategies",
            scalar: Scalar::Unfixed,
            arithmetic: false,
            value_docs: true,
            entries: &[
                ("STRATEGY_PREFER_INPUT", 0),
                ("STRATEGY_PREFER_BASE", 1),
                ("STRATEGY_INPUT_ONLY", 2),
                ("STRATEGY_BASE_ONLY", 3),
                ("STRATEGY_REMOVE", 4),
                ("STRATEGY_UNSPECIFIED", 5),
            ],
        },
    )?;
    def_static(&cls, &CREATE_PARAMS)?;
    def(&cls, &COPY_PARAMS)?;
    // Each macro expands into a separate local static, preserving method declaration order.
    macro_rules! text {
        ($set:literal, $get:literal, $param:literal, $setter:ident, $getter:ident) => {{
            static SET: Function = method_def!(
                $set,
                [SELF, Param::new($param, "str")],
                "None",
                |py, o, a, convert| {
                    let rc = this_shared!(o, a, ConfigMergingParameters);
                    let value = arg!(a, 1, convert, CharPtr);
                    if value.0.is_none() && $set != "setInactiveColorSpaces" {
                        return Err(null_string(concat!("ConfigMergingParameters.", $set)));
                    }
                    rc.lock().$setter(value.0.as_deref().unwrap_or_default());
                    none(py)
                }
            );
            static GET: Function = method_def!($get, [SELF], "str", |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigMergingParameters);
                string_out(py, &rc.lock().$getter())
            });
            def(&cls, &SET)?;
            def(&cls, &GET)?;
        }};
    }
    macro_rules! strategy_pair {
        ($set:literal, $get:literal, $setter:ident, $getter:ident) => {{
            static SET: Function = method_def!(
                $set,
                [SELF, Param::new("strategy", STRATEGY)],
                "None",
                |py, o, a, _| {
                    let rc = this_shared!(o, a, ConfigMergingParameters);
                    let v = strategy(enum_param!(o, a, 1))?;
                    rc.lock().$setter(v);
                    none(py)
                }
            );
            static GET: Function = method_def!($get, [SELF], STRATEGY, |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigMergingParameters);
                enum_out(py, STRATEGY, rc.lock().$getter() as i64)
            });
            def(&cls, &SET)?;
            def(&cls, &GET)?;
        }};
    }
    macro_rules! boolean {
        ($set:literal, $get:literal, $setter:ident, $getter:ident) => {{
            static SET: Function = method_def!(
                $set,
                [SELF, Param::new("enabled", "bool")],
                "None",
                |py, o, a, convert| {
                    let rc = this_shared!(o, a, ConfigMergingParameters);
                    let v = arg!(a, 1, convert, bool);
                    rc.lock().$setter(v);
                    none(py)
                }
            );
            static GET: Function = method_def!($get, [SELF], "bool", |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigMergingParameters);
                ok(py, rc.lock().$getter())
            });
            def(&cls, &SET)?;
            def(&cls, &GET)?;
        }};
    }
    // PyMergeConfigs.cpp:32-96: the names, default, family prefixes, options, overrides.
    text!(
        "setBaseConfigName",
        "getBaseConfigName",
        "baseConfig",
        set_base_config_name,
        base_config_name
    );
    text!(
        "setInputConfigName",
        "getInputConfigName",
        "inputConfig",
        set_input_config_name,
        input_config_name
    );
    text!(
        "setOutputName",
        "getOutputName",
        "outputName",
        set_output_name,
        output_name
    );
    strategy_pair!(
        "setDefaultStrategy",
        "getDefaultStrategy",
        set_default_strategy,
        default_strategy
    );
    text!(
        "setInputFamilyPrefix",
        "getInputFamilyPrefix",
        "prefix",
        set_input_family_prefix,
        input_family_prefix
    );
    text!(
        "setBaseFamilyPrefix",
        "getBaseFamilyPrefix",
        "prefix",
        set_base_family_prefix,
        base_family_prefix
    );
    boolean!(
        "setInputFirst",
        "isInputFirst",
        set_input_first,
        is_input_first
    );
    boolean!(
        "setErrorOnConflict",
        "isErrorOnConflict",
        set_error_on_conflict,
        is_error_on_conflict
    );
    boolean!(
        "setAvoidDuplicates",
        "isAvoidDuplicates",
        set_avoid_duplicates,
        is_avoid_duplicates
    );
    boolean!(
        "setAdjustInputReferenceSpace",
        "isAdjustInputReferenceSpace",
        set_adjust_input_reference_space,
        is_adjust_input_reference_space
    );
    text!("setName", "getName", "mergedConfigName", set_name, name);
    text!(
        "setDescription",
        "getDescription",
        "mergedConfigDesc",
        set_description,
        description
    );
    // PyMergeConfigs.cpp:97-105: the environment (invalid indices return empty strings).
    static ADD_ENV: Function = method_def!(
        "addEnvironmentVar",
        [
            SELF,
            Param::new("name", "str"),
            Param::new("defaultValue", "str")
        ],
        "None",
        |py, o, a, convert| {
            let rc = this_shared!(o, a, ConfigMergingParameters);
            let name = arg!(a, 1, convert, CharPtr);
            let value = arg!(a, 2, convert, CharPtr);
            rc.lock()
                .add_environment_var(name.0.as_deref().unwrap_or_default(), value.0.as_deref());
            none(py)
        }
    );
    static NUM_ENV: Function =
        method_def!("getNumEnvironmentVars", [SELF], "int", |py, o, a, _| {
            let rc = this_shared!(o, a, ConfigMergingParameters);
            ok(py, rc.lock().num_environment_vars())
        });
    def(&cls, &ADD_ENV)?;
    def(&cls, &NUM_ENV)?;
    macro_rules! env_getter {
        ($name:literal, $getter:ident) => {{
            static GET: Function = method_def!(
                $name,
                [SELF, Param::new("index", INT)],
                "str",
                |py, o, a, convert| {
                    let rc = this_shared!(o, a, ConfigMergingParameters);
                    let index = arg!(a, 1, convert, i32);
                    string_out(py, &rc.lock().$getter(index))
                }
            );
            def(&cls, &GET)?;
        }};
    }
    env_getter!("getEnvironmentVar", environment_var);
    env_getter!("getEnvironmentVarValue", environment_var_value);
    // PyMergeConfigs.cpp:107-129: search paths and active lists, then section strategies.
    static SET_PATH: Function = method_def!(
        "setSearchPath",
        [SELF, Param::new("path", "str")],
        "None",
        |py, o, a, convert| {
            let rc = this_shared!(o, a, ConfigMergingParameters);
            let path = arg!(a, 1, convert, CharPtr);
            rc.lock()
                .set_search_path(path.0.as_deref().unwrap_or_default());
            none(py)
        }
    );
    static ADD_PATH: Function = method_def!(
        "addSearchPath",
        [SELF, Param::new("path", "str")],
        "None",
        |py, o, a, convert| {
            let rc = this_shared!(o, a, ConfigMergingParameters);
            let path = arg!(a, 1, convert, CharPtr);
            rc.lock()
                .add_search_path(path.0.as_deref().unwrap_or_default());
            none(py)
        }
    );
    static GET_PATH: Function = method_def!("getSearchPath", [SELF], "str", |py, o, a, _| {
        let rc = this_shared!(o, a, ConfigMergingParameters);
        string_out(py, &rc.lock().search_path())
    });
    for f in [&SET_PATH, &ADD_PATH, &GET_PATH] {
        def(&cls, f)?;
    }
    macro_rules! active {
        ($set:literal, $get:literal, $param:literal, $setter:ident, $getter:ident) => {{
            static SET: Function = method_def!(
                $set,
                [SELF, Param::new($param, "str")],
                "None",
                |py, o, a, convert| {
                    let rc = this_shared!(o, a, ConfigMergingParameters);
                    let v = arg!(a, 1, convert, CharPtr);
                    if v.0.is_none() {
                        return Err(crate::py_context::null_string(concat!(
                            "ConfigMergingParameters.",
                            $set
                        )));
                    }
                    py_result(py, rc.lock().$setter(v.0.as_deref().unwrap_or_default()))?;
                    none(py)
                }
            );
            static GET: Function = method_def!($get, [SELF], "str", |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigMergingParameters);
                string_out(py, &rc.lock().$getter())
            });
            def(&cls, &SET)?;
            def(&cls, &GET)?;
        }};
    }
    active!(
        "setActiveDisplays",
        "getActiveDisplays",
        "displays",
        set_active_displays,
        active_displays
    );
    active!(
        "setActiveViews",
        "getActiveViews",
        "views",
        set_active_views,
        active_views
    );
    text!(
        "setInactiveColorSpaces",
        "getInactiveColorSpaces",
        "colorspaces",
        set_inactive_color_spaces,
        inactive_color_spaces
    );
    strategy_pair!("setRoles", "getRoles", set_roles, roles);
    strategy_pair!("setFileRules", "getFileRules", set_file_rules, file_rules);
    strategy_pair!(
        "setDisplayViews",
        "getDisplayViews",
        set_display_views,
        display_views
    );
    strategy_pair!(
        "setViewTransforms",
        "getViewTransforms",
        set_view_transforms,
        view_transforms
    );
    strategy_pair!("setLooks", "getLooks", set_looks, looks);
    strategy_pair!(
        "setColorspaces",
        "getColorspaces",
        set_colorspaces,
        colorspaces
    );
    strategy_pair!(
        "setNamedTransforms",
        "getNamedTransforms",
        set_named_transforms,
        named_transforms
    );
    export_values(&cls, &strategies)?;
    static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
        let rc = this_shared!(o, a, ConfigMergingParameters);
        string_out(py, &rc.lock().to_bytes())
    });
    def(&cls, &REPR)?;

    let cls = class(py, MERGER)?;
    def_static(&cls, &CREATE_MERGER)?;
    def_static(&cls, &FROM_FILE)?;
    def(&cls, &COPY_MERGER)?;
    macro_rules! merger_text_setter {
        ($name:literal, $param:literal, $setter:ident) => {{
            static SET: Function = method_def!(
                $name,
                [MERGER_SELF, Param::new($param, "str")],
                "None",
                |py, o, a, convert| {
                    let rc = this_shared!(o, a, MergerNode);
                    let v = arg!(a, 1, convert, CharPtr);
                    if v.0.is_none() && $name == "setWorkingDir" {
                        return Err(null_string("ConfigMerger.setWorkingDir"));
                    }
                    rc.lock().merger.$setter(v.0.as_deref().unwrap_or_default());
                    none(py)
                }
            );
            def(&cls, &SET)?;
        }};
    }
    macro_rules! merger_int {
        ($name:literal, $getter:ident) => {{
            static GET: Function = method_def!($name, [MERGER_SELF], "int", |py, o, a, _| {
                let rc = this_shared!(o, a, MergerNode);
                ok(py, rc.lock().merger.$getter())
            });
            def(&cls, &GET)?;
        }};
    }
    merger_text_setter!("setSearchPath", "path", set_search_path);
    merger_text_setter!("addSearchPath", "path", add_search_path);
    merger_int!("getNumSearchPaths", num_search_paths);
    static SEARCH_PATH: Function = method_def!(
        "getSearchPath",
        [MERGER_SELF, Param::new("index", INT)],
        "str",
        |py, o, a, convert| {
            let rc = this_shared!(o, a, MergerNode);
            let i = arg!(a, 1, convert, i32);
            string_out(py, &rc.lock().merger.search_path(i))
        }
    );
    def(&cls, &SEARCH_PATH)?;
    merger_text_setter!("setWorkingDir", "dirname", set_working_dir);
    static WORKING_DIR: Function =
        method_def!("getWorkingDir", [MERGER_SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, MergerNode);
            string_out(py, rc.lock().merger.working_dir())
        });
    def(&cls, &WORKING_DIR)?;
    def(&cls, &GET_PARAMS)?;
    merger_int!(
        "getNumConfigMergingParameters",
        num_config_merging_parameters
    );
    def(&cls, &ADD_PARAMS)?;
    def(&cls, &EXECUTE)?;
    merger_int!("getNumMergedConfigs", num_merged_configs);
    for f in [&GET_CONFIG, &SERIALIZE, &SET_VERSION] {
        def(&cls, f)?;
    }
    merger_int!("getMajorVersion", major_version);
    merger_int!("getMinorVersion", minor_version);
    static MERGER_REPR: Function = method_def!("__repr__", [MERGER_SELF], "str", |py, o, a, _| {
        let rc = this_shared!(o, a, MergerNode);
        string_out(py, &py_result(py, rc.lock().merger.serialize())?)
    });
    def(&cls, &MERGER_REPR)?;
    for f in [&MERGE_CONFIGS, &MERGE_COLOR_SPACE] {
        crate::pybind11::def_submodule_function(&sub, f)?;
    }
    Ok(())
}
