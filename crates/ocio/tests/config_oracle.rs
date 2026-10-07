// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio::Config`'s own state against the wheel's, through the oracle's `config_calls`: the
//! same calls on a new config (`Config()`, `Config::Create`) on both sides, in the same
//! environment, each call's result or exception compared byte for byte, then the config's
//! getters and its context's `repr()` (upstream's `operator<<`).
//!
//! The oracle's process holds exactly the request's variables, set one by one; the port reads
//! a `MapEnv` the same variables are set in, through OCIO's `Setenv`, on the test's own thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use ocio::{
    ColorSpace, ColorSpaceDirection, ColorSpaceVisibility, Config, CurrentContext, FileRules,
    LogTransform, Look, MatrixTransform, NamedTransform, NamedTransformVisibility,
    ReferenceSpaceType, SearchReferenceSpaceType, Transform, TransformDirection, ViewTransform,
    ViewTransformDirection, ViewType, ViewingRules,
};
use ocio_ops::logging::{LoggingFunction, reset_to_default_logging_function, set_logging_function};
use ocio_ops::open_color_types::EnvironmentMode;
use ocio_ops::platform::{MapEnv, set_thread_env_provider, setenv};
use ocio_ops::utils::string_utils::compare;
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes_arg, hex, log};
use serde_json::{Value, json};

/// An environment: each variable's name and value.
type Env<'a> = &'a [(&'a str, &'a [u8])];

/// The port's side of a call: its outcome, as the oracle writes a call's (`{"result": value}`,
/// `{"exception": {"type", "message"}}` or `{"undecodable": hex}`).
type PortCall = Box<dyn Fn(&mut Config) -> Value>;

/// A call on the config: the wheel's (a `config_calls` call) and the port's.
struct Step {
    call: Value,
    port: PortCall,
}

fn step(call: Value, port: impl Fn(&mut Config) -> Value + 'static) -> Step {
    Step {
        call,
        port: Box::new(port),
    }
}

/// What a case does between two runs of the getters: a call, calls, or a copy of the config
/// (`copy.deepcopy`: `createEditableCopy`), which the calls after it act on, on both sides.
enum Item {
    One(Step),
    Group(Vec<Step>),
    Copy,
}

impl From<Step> for Item {
    fn from(s: Step) -> Item {
        Item::One(s)
    }
}

impl From<Vec<Step>> for Item {
    fn from(s: Vec<Step>) -> Item {
        Item::Group(s)
    }
}

/// An object the library returned, as `{"class", "repr"}`.
fn object_out(class: &str, repr: &[u8]) -> Value {
    let repr = match std::str::from_utf8(repr) {
        Ok(_) => bytes_arg(repr),
        Err(_) => json!({"undecodable": hex(repr)}),
    };
    json!({"class": class, "repr": repr})
}

/// A string the library returned, as the binding gives it: `{"bytes": hex}` when it is UTF-8,
/// else `{"undecodable": hex}` for the whole call.
fn text_out(s: &[u8]) -> Value {
    match std::str::from_utf8(s) {
        Ok(_) => json!({"result": bytes_arg(s)}),
        Err(_) => json!({"undecodable": hex(s)}),
    }
}

/// A list of strings, element by element (an element the binding can't decode is
/// `{"undecodable": hex}`).
fn texts_out(list: &[Vec<u8>]) -> Value {
    let elements: Vec<Value> = list
        .iter()
        .map(|s| match std::str::from_utf8(s) {
            Ok(_) => bytes_arg(s),
            Err(_) => json!({"undecodable": hex(s)}),
        })
        .collect();
    json!({ "result": elements })
}

/// What a call that returns nothing gives, or its exception.
fn unit_out(r: ocio::Result<()>) -> Value {
    match r {
        Ok(()) => json!({"result": null}),
        Err(e) => match std::str::from_utf8(e.what()) {
            Ok(_) => json!({"exception": {"type": "Exception", "message": bytes_arg(e.what())}}),
            Err(_) => json!({"undecodable": hex(e.what())}),
        },
    }
}

fn mode_name(mode: EnvironmentMode) -> &'static str {
    match mode {
        EnvironmentMode::Unknown => "ENV_ENVIRONMENT_UNKNOWN",
        EnvironmentMode::LoadPredefined => "ENV_ENVIRONMENT_LOAD_PREDEFINED",
        EnvironmentMode::LoadAll => "ENV_ENVIRONMENT_LOAD_ALL",
    }
}

fn mode(m: EnvironmentMode) -> Value {
    json!({"enum": mode_name(m)})
}

fn arg(s: &[u8]) -> Value {
    bytes_arg(s)
}

// ---------------------------------------------------------------------------------------------
// Calls

fn set_major_version(v: u32) -> Step {
    step(json!({"call": "setMajorVersion", "args": [v]}), move |c| {
        unit_out(c.set_major_version(v))
    })
}

fn set_minor_version(v: u32) -> Step {
    step(json!({"call": "setMinorVersion", "args": [v]}), move |c| {
        unit_out(c.set_minor_version(v))
    })
}

fn set_version(major: u32, minor: u32) -> Step {
    step(
        json!({"call": "setVersion", "args": [major, minor]}),
        move |c| unit_out(c.set_version(major, minor)),
    )
}

fn set_name(name: &[u8]) -> Step {
    let name = name.to_vec();
    step(json!({"call": "setName", "args": [arg(&name)]}), move |c| {
        c.set_name(&name);
        json!({"result": null})
    })
}

fn set_description(description: &[u8]) -> Step {
    let description = description.to_vec();
    step(
        json!({"call": "setDescription", "args": [arg(&description)]}),
        move |c| {
            c.set_description(&description);
            json!({"result": null})
        },
    )
}

fn set_family_separator(separator: u8) -> Step {
    step(
        json!({"call": "setFamilySeparator", "args": [arg(&[separator])]}),
        move |c| unit_out(c.set_family_separator(separator)),
    )
}

fn add_environment_var(name: &[u8], value: Option<&[u8]>) -> Step {
    let name = name.to_vec();
    let value = value.map(<[u8]>::to_vec);
    let value_arg = value.as_deref().map_or(Value::Null, arg);
    step(
        json!({"call": "addEnvironmentVar", "args": [arg(&name), value_arg]}),
        move |c| {
            c.add_environment_var(&name, value.as_deref());
            json!({"result": null})
        },
    )
}

fn get_environment_var_default(name: &[u8]) -> Step {
    let name = name.to_vec();
    step(
        json!({"call": "getEnvironmentVarDefault", "args": [arg(&name)]}),
        move |c| text_out(c.environment_var_default(&name)),
    )
}

fn clear_environment_vars() -> Step {
    step(json!({"call": "clearEnvironmentVars"}), |c| {
        c.clear_environment_vars();
        json!({"result": null})
    })
}

fn set_environment_mode(m: EnvironmentMode) -> Step {
    step(
        json!({"call": "setEnvironmentMode", "args": [mode(m)]}),
        move |c| {
            c.set_environment_mode(m);
            json!({"result": null})
        },
    )
}

fn load_environment() -> Step {
    step(json!({"call": "loadEnvironment"}), |c| {
        c.load_environment();
        json!({"result": null})
    })
}

fn set_search_path(path: &[u8]) -> Step {
    let path = path.to_vec();
    step(
        json!({"call": "setSearchPath", "args": [arg(&path)]}),
        move |c| {
            c.set_search_path(&path);
            json!({"result": null})
        },
    )
}

fn add_search_path(path: &[u8]) -> Step {
    let path = path.to_vec();
    step(
        json!({"call": "addSearchPath", "args": [arg(&path)]}),
        move |c| {
            c.add_search_path(&path);
            json!({"result": null})
        },
    )
}

fn clear_search_paths() -> Step {
    step(json!({"call": "clearSearchPaths"}), |c| {
        c.clear_search_paths();
        json!({"result": null})
    })
}

fn set_working_dir(dir: &[u8]) -> Step {
    let dir = dir.to_vec();
    step(
        json!({"call": "setWorkingDir", "args": [arg(&dir)]}),
        move |c| {
            c.set_working_dir(&dir);
            json!({"result": null})
        },
    )
}

/// A color space for a case: its reference space, name, aliases and categories.
#[derive(Clone)]
struct Cs {
    reference: ReferenceSpaceType,
    name: Vec<u8>,
    aliases: Vec<Vec<u8>>,
    categories: Vec<Vec<u8>>,
    encoding: Option<Vec<u8>>,
    is_data: bool,
    /// The transforms: their spec, the port's transform, and the direction.
    transforms: Vec<(Value, Transform, ColorSpaceDirection)>,
}

fn cs(name: &[u8]) -> Cs {
    Cs {
        reference: ReferenceSpaceType::Scene,
        name: name.to_vec(),
        aliases: Vec::new(),
        categories: Vec::new(),
        encoding: None,
        is_data: false,
        transforms: Vec::new(),
    }
}

impl Cs {
    fn display(mut self) -> Cs {
        self.reference = ReferenceSpaceType::Display;
        self
    }
    fn alias(mut self, alias: &[u8]) -> Cs {
        self.aliases.push(alias.to_vec());
        self
    }
    fn category(mut self, category: &[u8]) -> Cs {
        self.categories.push(category.to_vec());
        self
    }
    fn encoding(mut self, encoding: &[u8]) -> Cs {
        self.encoding = Some(encoding.to_vec());
        self
    }
    fn data(mut self) -> Cs {
        self.is_data = true;
        self
    }
    fn transform(mut self, spec: Value, port: Transform, dir: ColorSpaceDirection) -> Cs {
        self.transforms.push((spec, port, dir));
        self
    }
    fn build(&self) -> ColorSpace {
        let mut cs = ColorSpace::with_reference_space(self.reference);
        cs.set_name(&self.name);
        for a in &self.aliases {
            cs.add_alias(a);
        }
        for c in &self.categories {
            cs.add_category(c);
        }
        if let Some(e) = &self.encoding {
            cs.set_encoding(e);
        }
        if self.is_data {
            cs.set_is_data(true);
        }
        for (_, t, dir) in &self.transforms {
            cs.set_transform(Some(t), *dir);
        }
        cs
    }
}

fn reference_name(r: ReferenceSpaceType) -> &'static str {
    match r {
        ReferenceSpaceType::Scene => "REFERENCE_SPACE_SCENE",
        ReferenceSpaceType::Display => "REFERENCE_SPACE_DISPLAY",
    }
}

/// Makes the color space `spec` (stored as `cs`) and adds it to the config.
fn add_color_space(spec: Cs) -> Vec<Step> {
    let mut out = Vec::new();
    let made = spec.clone();
    out.push(step(
        json!({"new": "ColorSpace", "args": [{"enum": reference_name(spec.reference)}],
               "as": "cs"}),
        move |_| {
            let cs = ColorSpace::with_reference_space(made.reference);
            json!({"result": object_out("ColorSpace", &cs.to_bytes())})
        },
    ));
    out.push(step(
        json!({"call": "setName", "on": "cs", "args": [arg(&spec.name)]}),
        |_| json!({"result": null}),
    ));
    for a in &spec.aliases {
        out.push(step(
            json!({"call": "addAlias", "on": "cs", "args": [arg(a)]}),
            |_| json!({"result": null}),
        ));
    }
    for c in &spec.categories {
        out.push(step(
            json!({"call": "addCategory", "on": "cs", "args": [arg(c)]}),
            |_| json!({"result": null}),
        ));
    }
    if let Some(e) = &spec.encoding {
        out.push(step(
            json!({"call": "setEncoding", "on": "cs", "args": [arg(e)]}),
            |_| json!({"result": null}),
        ));
    }
    if spec.is_data {
        out.push(step(
            json!({"call": "setIsData", "on": "cs", "args": [true]}),
            |_| json!({"result": null}),
        ));
    }
    for (t, _, dir) in &spec.transforms {
        let dir = match dir {
            ColorSpaceDirection::ToReference => "COLORSPACE_DIR_TO_REFERENCE",
            ColorSpaceDirection::FromReference => "COLORSPACE_DIR_FROM_REFERENCE",
        };
        out.push(step(
            json!({"call": "setTransform", "on": "cs", "args": [{"transform": t}, {"enum": dir}]}),
            |_| json!({"result": null}),
        ));
    }
    out.push(step(
        json!({"call": "addColorSpace", "args": [{"ref": "cs"}]}),
        move |c| unit_out(c.add_color_space(&spec.build())),
    ));
    out
}

fn remove_color_space(name: &[u8]) -> Step {
    let name = name.to_vec();
    step(
        json!({"call": "removeColorSpace", "args": [arg(&name)]}),
        move |c| {
            c.remove_color_space(&name);
            json!({"result": null})
        },
    )
}

fn clear_color_spaces() -> Step {
    step(json!({"call": "clearColorSpaces"}), |c| {
        c.clear_color_spaces();
        json!({"result": null})
    })
}

fn set_role(role: &[u8], cs: Option<&[u8]>) -> Step {
    let role = role.to_vec();
    let cs = cs.map(<[u8]>::to_vec);
    let cs_arg = cs.as_deref().map_or(Value::Null, arg);
    step(
        json!({"call": "setRole", "args": [arg(&role), cs_arg]}),
        move |c| unit_out(c.set_role(&role, cs.as_deref())),
    )
}

const SEARCH_TYPES: [(SearchReferenceSpaceType, &str); 3] = [
    (
        SearchReferenceSpaceType::Scene,
        "SEARCH_REFERENCE_SPACE_SCENE",
    ),
    (
        SearchReferenceSpaceType::Display,
        "SEARCH_REFERENCE_SPACE_DISPLAY",
    ),
    (SearchReferenceSpaceType::All, "SEARCH_REFERENCE_SPACE_ALL"),
];

const VISIBILITIES: [(ColorSpaceVisibility, &str); 3] = [
    (ColorSpaceVisibility::Active, "COLORSPACE_ACTIVE"),
    (ColorSpaceVisibility::Inactive, "COLORSPACE_INACTIVE"),
    (ColorSpaceVisibility::All, "COLORSPACE_ALL"),
];

/// The names and categories the getters look up, besides the config's own.
#[derive(Clone, Default)]
struct Probes {
    names: Vec<Vec<u8>>,
    categories: Vec<Vec<u8>>,
    /// Displays, and the views the getters ask them for.
    displays: Vec<Vec<u8>>,
    views: Vec<Vec<u8>>,
    /// Paths the file rule getters match.
    paths: Vec<Vec<u8>>,
}

/// Probes of names only.
fn probes_of(names: &[&[u8]]) -> Probes {
    probes(names, &[])
}

fn probes(names: &[&[u8]], categories: &[&[u8]]) -> Probes {
    Probes {
        names: names.iter().map(|n| n.to_vec()).collect(),
        categories: categories.iter().map(|n| n.to_vec()).collect(),
        ..Probes::default()
    }
}

impl Probes {
    fn paths(mut self, paths: &[&[u8]]) -> Probes {
        self.paths = paths.iter().map(|n| n.to_vec()).collect();
        self
    }

    fn displays(mut self, displays: &[&[u8]], views: &[&[u8]]) -> Probes {
        self.displays = displays.iter().map(|n| n.to_vec()).collect();
        self.views = views.iter().map(|n| n.to_vec()).collect();
        self
    }
}

/// The getters of the color spaces and roles: the names of each search, the roles, and for
/// each probe name and category, what the config finds.
fn color_space_getters(probes: &Probes) -> Vec<Step> {
    let mut out = Vec::new();
    for (t, t_name) in SEARCH_TYPES {
        for (v, v_name) in VISIBILITIES {
            out.push(step(
                json!({"call": "getColorSpaceNames",
                       "args": [{"enum": t_name}, {"enum": v_name}]}),
                move |c| {
                    let names: Vec<Vec<u8>> = (0..c.num_color_spaces_with(t, v))
                        .map(|i| c.color_space_name_by_index_with(t, v, i).to_vec())
                        .collect();
                    texts_out(&names)
                },
            ));
        }
    }
    out.push(step(json!({"call": "getColorSpaceNames"}), |c| {
        let names: Vec<Vec<u8>> = (0..c.num_color_spaces())
            .map(|i| c.color_space_name_by_index(i).to_vec())
            .collect();
        texts_out(&names)
    }));
    out.push(step(json!({"call": "getRoleNames"}), |c| {
        let names: Vec<Vec<u8>> = (0..c.num_roles())
            .map(|i| c.role_name(i).to_vec())
            .collect();
        texts_out(&names)
    }));
    out.push(step(json!({"call": "getRoles"}), |c| {
        let roles: Vec<Value> = (0..c.num_roles())
            .map(|i| {
                // The binding decodes the pair: an element it can't decode is the bytes it
                // refused.
                let (name, cs) = (c.role_name(i), c.role_color_space_by_index(i));
                match (std::str::from_utf8(name), std::str::from_utf8(cs)) {
                    (Err(_), _) => json!({"undecodable": hex(name)}),
                    (_, Err(_)) => json!({"undecodable": hex(cs)}),
                    _ => json!([bytes_arg(name), bytes_arg(cs)]),
                }
            })
            .collect();
        json!({ "result": roles })
    }));
    for name in &probes.names {
        let n = name.clone();
        out.push(step(
            json!({"call": "getColorSpace", "args": [arg(name)]}),
            move |c| match c.color_space(&n) {
                Some(cs) => json!({"result": object_out("ColorSpace", &cs.to_bytes())}),
                None => json!({"result": null}),
            },
        ));
        let n = name.clone();
        out.push(step(
            json!({"call": "getCanonicalName", "args": [arg(name)]}),
            move |c| text_out(c.canonical_name(&n)),
        ));
        let n = name.clone();
        out.push(step(
            json!({"call": "hasRole", "args": [arg(name)]}),
            move |c| json!({"result": c.has_role(&n)}),
        ));
        let n = name.clone();
        out.push(step(
            json!({"call": "getRoleColorSpace", "args": [arg(name)]}),
            move |c| text_out(c.role_color_space(&n)),
        ));
    }
    for category in &probes.categories {
        out.push(step(
            json!({"call": "getColorSpaces", "args": [arg(category)], "as": "set"}),
            |_| json!({"result": {"class": "ColorSpaceSet"}}),
        ));
        let cat = category.clone();
        out.push(step(
            json!({"call": "getColorSpaceNames", "on": "set"}),
            move |c| {
                let set = c.color_spaces(&cat);
                let names: Vec<Vec<u8>> = (0..set.num_color_spaces())
                    .map(|i| set.color_space_name_by_index(i).unwrap_or(&[]).to_vec())
                    .collect();
                texts_out(&names)
            },
        ));
    }
    out
}

fn set_inactive_color_spaces(list: &[u8]) -> Step {
    let list = list.to_vec();
    step(
        json!({"call": "setInactiveColorSpaces", "args": [arg(&list)]}),
        move |c| {
            c.set_inactive_color_spaces(&list);
            json!({"result": null})
        },
    )
}

fn set_strict_parsing_enabled(enabled: bool) -> Step {
    step(
        json!({"call": "setStrictParsingEnabled", "args": [enabled]}),
        move |c| {
            c.set_strict_parsing_enabled(enabled);
            json!({"result": null})
        },
    )
}

fn set_default_luma_coefs(c3: [f64; 3]) -> Step {
    let bits: Vec<Value> = c3.iter().map(|v| json!({"f64": v.to_bits()})).collect();
    step(
        json!({"call": "setDefaultLumaCoefs", "args": [bits]}),
        move |c| {
            c.set_default_luma_coefs(&c3);
            json!({"result": null})
        },
    )
}

/// The getters of the inactive list, strict parsing and the luma, and for each probe name
/// `isInactiveColorSpace` and `isColorSpaceLinear` for both reference spaces.
fn model_getters(probes: &Probes) -> Vec<Step> {
    let mut out = vec![
        step(json!({"call": "getInactiveColorSpaces"}), |c| {
            text_out(c.inactive_color_spaces())
        }),
        step(
            json!({"call": "isStrictParsingEnabled"}),
            |c| json!({"result": c.is_strict_parsing_enabled()}),
        ),
        step(json!({"call": "getDefaultLumaCoefs"}), |c| {
            let bits: Vec<Value> = c
                .default_luma_coefs()
                .iter()
                .map(|v| json!({"f64": v.to_bits()}))
                .collect();
            json!({ "result": bits })
        }),
    ];
    for name in &probes.names {
        let n = name.clone();
        out.push(step(
            json!({"call": "isInactiveColorSpace", "args": [arg(name)]}),
            move |c| json!({"result": c.is_inactive_color_space(&n)}),
        ));
        for (t, t_name) in [
            (ReferenceSpaceType::Scene, "REFERENCE_SPACE_SCENE"),
            (ReferenceSpaceType::Display, "REFERENCE_SPACE_DISPLAY"),
        ] {
            let n = name.clone();
            out.push(step(
                json!({"call": "isColorSpaceLinear", "args": [arg(name), {"enum": t_name}]}),
                move |c| match c.is_color_space_linear(&n, t) {
                    Ok(v) => json!({ "result": v }),
                    Err(e) => unit_out(Err(e)),
                },
            ));
        }
    }
    out
}

fn add_display_view(display: &[u8], view: &[u8], cs: &[u8], looks: &[u8]) -> Step {
    let (d, v, c, l) = (display.to_vec(), view.to_vec(), cs.to_vec(), looks.to_vec());
    step(
        json!({"call": "addDisplayView", "args": [arg(&d), arg(&v), arg(&c), arg(&l)]}),
        move |config| unit_out(config.add_display_view(&d, &v, &c, &l)),
    )
}

/// `addDisplayView` with a view transform, a rule and a description.
fn add_display_view_full(display: &[u8], view: &[u8], vt: &[u8], cs: &[u8], rule: &[u8]) -> Step {
    let (d, v, t, c, r) = (
        display.to_vec(),
        view.to_vec(),
        vt.to_vec(),
        cs.to_vec(),
        rule.to_vec(),
    );
    step(
        json!({"call": "addDisplayView",
               "args": [arg(&d), arg(&v), arg(&t), arg(&c), arg(b"look"), arg(&r),
                        arg(b"a description")]}),
        move |config| {
            unit_out(config.add_display_view_with_view_transform(
                &d,
                &v,
                &t,
                &c,
                b"look",
                &r,
                b"a description",
            ))
        },
    )
}

fn add_shared_view(view: &[u8], vt: &[u8], cs: &[u8]) -> Step {
    let (v, t, c) = (view.to_vec(), vt.to_vec(), cs.to_vec());
    step(
        json!({"call": "addSharedView",
               "args": [arg(&v), arg(&t), arg(&c), arg(b"l1, l2"), arg(b"rule"), arg(b"d")]}),
        move |config| unit_out(config.add_shared_view(&v, &t, &c, b"l1, l2", b"rule", b"d")),
    )
}

fn add_display_shared_view(display: &[u8], view: &[u8]) -> Step {
    let (d, v) = (display.to_vec(), view.to_vec());
    step(
        json!({"call": "addDisplaySharedView", "args": [arg(&d), arg(&v)]}),
        move |config| unit_out(config.add_display_shared_view(&d, &v)),
    )
}

fn remove_shared_view(view: &[u8]) -> Step {
    let v = view.to_vec();
    step(
        json!({"call": "removeSharedView", "args": [arg(&v)]}),
        move |config| unit_out(config.remove_shared_view(&v)),
    )
}

fn remove_display_view(display: &[u8], view: &[u8]) -> Step {
    let (d, v) = (display.to_vec(), view.to_vec());
    step(
        json!({"call": "removeDisplayView", "args": [arg(&d), arg(&v)]}),
        move |config| unit_out(config.remove_display_view(&d, &v)),
    )
}

fn clear_shared_views() -> Step {
    step(json!({"call": "clearSharedViews"}), |c| {
        c.clear_shared_views();
        json!({"result": null})
    })
}

fn clear_displays() -> Step {
    step(json!({"call": "clearDisplays"}), |c| {
        c.clear_displays();
        json!({"result": null})
    })
}

const VIEW_TYPES: [(ViewType, &str); 2] = [
    (ViewType::Shared, "VIEW_SHARED"),
    (ViewType::DisplayDefined, "VIEW_DISPLAY_DEFINED"),
];

/// A list of strings the binding's iterator gives, or the exception reading it raised (the
/// iterator itself is made, so the oracle writes the exception as the call's result).
fn texts_or(r: ocio::Result<Vec<Vec<u8>>>) -> Value {
    match r {
        Ok(list) => texts_out(&list),
        Err(e) => json!({"result": unit_out(Err(e))}),
    }
}

/// The getters of the displays and views: the active displays, the shared views, and for each
/// probe display its views (active, by type, and for each probe color space), and for each
/// probe view what the display (and the config, for an empty display) says of it.
fn display_getters(probes: &Probes) -> Vec<Step> {
    let mut out = vec![
        step(json!({"call": "getDisplays"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_displays())
                .map(|i| c.display(i).to_vec())
                .collect();
            texts_out(&names)
        }),
        step(json!({"call": "getDefaultDisplay"}), |c| {
            text_out(c.default_display())
        }),
        step(
            json!({"call": "getViewingRules"}),
            |c| json!({"result": object_out("ViewingRules", &c.viewing_rules().get().to_bytes())}),
        ),
        step(json!({"call": "getSharedViews"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_views_of_type(ViewType::Shared, b""))
                .map(|i| c.view_of_type(ViewType::Shared, b"", i).to_vec())
                .collect();
            texts_out(&names)
        }),
    ];
    for display in &probes.displays {
        let d = display.clone();
        out.push(step(
            json!({"call": "getViews", "args": [arg(display)]}),
            move |c| {
                let names: Vec<Vec<u8>> = (0..c.num_views(&d))
                    .map(|i| c.view(&d, i).to_vec())
                    .collect();
                texts_out(&names)
            },
        ));
        let d = display.clone();
        out.push(step(
            json!({"call": "getDefaultView", "args": [arg(display)]}),
            move |c| text_out(c.default_view(&d)),
        ));
        for (t, t_name) in VIEW_TYPES {
            let d = display.clone();
            out.push(step(
                json!({"call": "getViews", "args": [{"enum": t_name}, arg(display)]}),
                move |c| {
                    let names: Vec<Vec<u8>> = (0..c.num_views_of_type(t, &d))
                        .map(|i| c.view_of_type(t, &d, i).to_vec())
                        .collect();
                    texts_out(&names)
                },
            ));
        }
        for cs in &probes.names {
            let (d, n) = (display.clone(), cs.clone());
            out.push(step(
                json!({"call": "getViews", "args": [arg(display), arg(cs)]}),
                move |c| {
                    texts_or((|| {
                        let num = c.num_views_for_color_space(&d, &n)?;
                        (0..num)
                            .map(|i| c.view_for_color_space(&d, &n, i).map(<[u8]>::to_vec))
                            .collect()
                    })())
                },
            ));
            let (d, n) = (display.clone(), cs.clone());
            out.push(step(
                json!({"call": "getDefaultView", "args": [arg(display), arg(cs)]}),
                move |c| match c.default_view_for_color_space(&d, &n) {
                    Ok(v) => text_out(v),
                    Err(e) => unit_out(Err(e)),
                },
            ));
        }
        for view in &probes.views {
            let (d, v) = (display.clone(), view.clone());
            out.push(step(
                json!({"call": "hasView", "args": [arg(display), arg(view)]}),
                move |c| json!({"result": c.has_view(&d, &v)}),
            ));
            let (d, v) = (display.clone(), view.clone());
            out.push(step(
                json!({"call": "isViewShared", "args": [arg(display), arg(view)]}),
                move |c| json!({"result": c.is_view_shared(&d, &v)}),
            ));
            type Getter = fn(&Config, &[u8], &[u8]) -> Vec<u8>;
            let getters: [(&str, Getter); 5] = [
                ("getDisplayViewTransformName", |c, d, v| {
                    c.display_view_transform_name(d, v).to_vec()
                }),
                ("getDisplayViewColorSpaceName", |c, d, v| {
                    c.display_view_color_space_name(d, v).to_vec()
                }),
                ("getDisplayViewLooks", |c, d, v| {
                    c.display_view_looks(d, v).to_vec()
                }),
                ("getDisplayViewRule", |c, d, v| {
                    c.display_view_rule(d, v).to_vec()
                }),
                ("getDisplayViewDescription", |c, d, v| {
                    c.display_view_description(d, v).to_vec()
                }),
            ];
            for (name, getter) in getters {
                let (d, v) = (display.clone(), view.clone());
                out.push(step(
                    json!({"call": name, "args": [arg(display), arg(view)]}),
                    move |c| text_out(&getter(c, &d, &v)),
                ));
            }
        }
    }
    out
}

/// A step of a list setter of the config (`setActiveDisplays`, `addActiveView`, ...): its name,
/// its argument, and the port's call.
fn list_call(name: &str, value: &[u8], port: fn(&mut Config, &[u8]) -> ocio::Result<()>) -> Step {
    let v = value.to_vec();
    step(json!({"call": name, "args": [arg(&v)]}), move |c| {
        unit_out(port(c, &v))
    })
}

fn set_active_displays(v: &[u8]) -> Step {
    list_call("setActiveDisplays", v, |c, v| c.set_active_displays(v))
}

fn add_active_display(v: &[u8]) -> Step {
    list_call("addActiveDisplay", v, |c, v| c.add_active_display(v))
}

fn remove_active_display(v: &[u8]) -> Step {
    list_call("removeActiveDisplay", v, |c, v| c.remove_active_display(v))
}

fn set_active_views(v: &[u8]) -> Step {
    list_call("setActiveViews", v, |c, v| c.set_active_views(v))
}

fn add_active_view(v: &[u8]) -> Step {
    list_call("addActiveView", v, |c, v| c.add_active_view(v))
}

fn remove_active_view(v: &[u8]) -> Step {
    list_call("removeActiveView", v, |c, v| c.remove_active_view(v))
}

fn add_virtual_display_shared_view(v: &[u8]) -> Step {
    list_call("addVirtualDisplaySharedView", v, |c, v| {
        c.add_virtual_display_shared_view(v)
    })
}

fn remove_virtual_display_view(v: &[u8]) -> Step {
    list_call("removeVirtualDisplayView", v, |c, v| {
        c.remove_virtual_display_view(v);
        Ok(())
    })
}

fn instantiate_display_from_icc_profile(v: &[u8]) -> Step {
    let v = v.to_vec();
    step(
        json!({"call": "instantiateDisplayFromICCProfile", "args": [arg(&v)]}),
        move |c| match c.instantiate_display_from_icc_profile(&v) {
            Ok(i) => json!({ "result": i }),
            Err(e) => unit_out(Err(e)),
        },
    )
}

fn instantiate_display_from_monitor_name(v: &[u8]) -> Step {
    let v = v.to_vec();
    step(
        json!({"call": "instantiateDisplayFromMonitorName", "args": [arg(&v)]}),
        move |c| match c.instantiate_display_from_monitor_name(&v) {
            Ok(i) => json!({ "result": i }),
            Err(e) => unit_out(Err(e)),
        },
    )
}

/// The simple steps of the config that take no argument.
fn clear_step(name: &str, port: fn(&mut Config)) -> Step {
    step(json!({ "call": name }), move |c| {
        port(c);
        json!({"result": null})
    })
}

fn add_virtual_display_view(view: &[u8], vt: &[u8], cs: &[u8]) -> Step {
    let (v, t, c) = (view.to_vec(), vt.to_vec(), cs.to_vec());
    step(
        json!({"call": "addVirtualDisplayView",
               "args": [arg(&v), arg(&t), arg(&c), arg(b"l"), arg(b"r"), arg(b"d")]}),
        move |config| unit_out(config.add_virtual_display_view(&v, &t, &c, b"l", b"r", b"d")),
    )
}

/// The binding's `setDisplayTemporary(display, isTemporary)`: every display of that name
/// (ignoring case).
fn set_display_temporary(display: &[u8], temporary: bool) -> Step {
    let d = display.to_vec();
    step(
        json!({"call": "setDisplayTemporary", "args": [arg(&d), temporary]}),
        move |c| {
            for i in 0..c.num_displays_all() {
                let other = c.display_all(i).to_vec();
                if compare(&d, &other) {
                    c.set_display_temporary(i, temporary);
                }
            }
            json!({"result": null})
        },
    )
}

/// The getters of the active lists, all the displays and the virtual display.
fn virtual_display_getters(probes: &Probes) -> Vec<Step> {
    let mut out = vec![
        step(json!({"call": "getActiveDisplays"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_active_displays())
                .map(|i| c.active_display(i).unwrap_or(&[]).to_vec())
                .collect();
            texts_out(&names)
        }),
        step(
            json!({"call": "getNumActiveDisplays"}),
            |c| json!({"result": c.num_active_displays()}),
        ),
        step(json!({"call": "getActiveViews"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_active_views())
                .map(|i| c.active_view(i).unwrap_or(&[]).to_vec())
                .collect();
            texts_out(&names)
        }),
        step(
            json!({"call": "getNumActiveViews"}),
            |c| json!({"result": c.num_active_views()}),
        ),
        step(json!({"call": "getDisplaysAll"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_displays_all())
                .map(|i| c.display_all(i).to_vec())
                .collect();
            texts_out(&names)
        }),
    ];
    for (t, t_name) in VIEW_TYPES {
        out.push(step(
            json!({"call": "getVirtualDisplayViews", "args": [{"enum": t_name}]}),
            move |c| {
                let names: Vec<Vec<u8>> = (0..c.virtual_display_num_views(t))
                    .map(|i| c.virtual_display_view(t, i).to_vec())
                    .collect();
                texts_out(&names)
            },
        ));
    }
    for display in &probes.displays {
        let d = display.clone();
        out.push(step(
            json!({"call": "isDisplayTemporary", "args": [arg(display)]}),
            move |c| {
                let found = (0..c.num_displays_all()).find(|&i| compare(&d, c.display_all(i)));
                json!({"result": found.is_some_and(|i| c.is_display_temporary(i))})
            },
        ));
    }
    for view in &probes.views {
        let v = view.clone();
        out.push(step(
            json!({"call": "hasVirtualView", "args": [arg(view)]}),
            move |c| json!({"result": c.has_virtual_view(&v)}),
        ));
        let v = view.clone();
        out.push(step(
            json!({"call": "isVirtualViewShared", "args": [arg(view)]}),
            move |c| json!({"result": c.is_virtual_view_shared(&v)}),
        ));
        type Getter = fn(&Config, &[u8]) -> Vec<u8>;
        let getters: [(&str, Getter); 5] = [
            ("getVirtualDisplayViewTransformName", |c, v| {
                c.virtual_display_view_transform_name(v).to_vec()
            }),
            ("getVirtualDisplayViewColorSpaceName", |c, v| {
                c.virtual_display_view_color_space_name(v).to_vec()
            }),
            ("getVirtualDisplayViewLooks", |c, v| {
                c.virtual_display_view_looks(v).to_vec()
            }),
            ("getVirtualDisplayViewRule", |c, v| {
                c.virtual_display_view_rule(v).to_vec()
            }),
            ("getVirtualDisplayViewDescription", |c, v| {
                c.virtual_display_view_description(v).to_vec()
            }),
        ];
        for (name, getter) in getters {
            let v = view.clone();
            out.push(step(json!({"call": name, "args": [arg(view)]}), move |c| {
                text_out(&getter(c, &v))
            }));
        }
    }
    out
}

/// A transform for the looks, view transforms and named transforms: its spec and the port's.
fn offset_transform(v: f64) -> (Value, Transform) {
    let mut m = MatrixTransform::new();
    m.set_offset(&[v, 0.0, 0.0, 0.0]);
    (
        json!({"class": "MatrixTransform", "calls": [["setOffset", [v, 0.0, 0.0, 0.0]]]}),
        Transform::from(m),
    )
}

/// Makes a look (stored as `lk`) and adds it.
fn add_look(name: &[u8], process_space: &[u8], with_transform: bool) -> Vec<Step> {
    let (n, ps) = (name.to_vec(), process_space.to_vec());
    let (spec, t) = offset_transform(0.25);
    let mut out = vec![
        step(
            json!({"new": "Look", "as": "lk"}),
            |_| json!({"result": object_out("Look", &Look::new().to_bytes())}),
        ),
        step(
            json!({"call": "setName", "on": "lk", "args": [arg(&n)]}),
            |_| json!({"result": null}),
        ),
        step(
            json!({"call": "setProcessSpace", "on": "lk", "args": [arg(&ps)]}),
            |_| json!({"result": null}),
        ),
    ];
    if with_transform {
        out.push(step(
            json!({"call": "setTransform", "on": "lk", "args": [{"transform": spec}]}),
            |_| json!({"result": null}),
        ));
    }
    out.push(step(
        json!({"call": "addLook", "args": [{"ref": "lk"}]}),
        move |c| {
            let mut look = Look::new();
            look.set_name(&n);
            look.set_process_space(&ps);
            if with_transform {
                look.set_transform(&t);
            }
            unit_out(c.add_look(&look))
        },
    ));
    out
}

/// Makes a view transform (stored as `vt`) and adds it.
fn add_view_transform(
    name: &[u8],
    reference: ReferenceSpaceType,
    dir: Option<ViewTransformDirection>,
) -> Vec<Step> {
    let n = name.to_vec();
    let (spec, t) = offset_transform(0.5);
    let mut out = vec![
        step(
            json!({"new": "ViewTransform", "args": [{"enum": reference_name(reference)}],
                   "as": "vt"}),
            move |_| {
                let vt = ViewTransform::new(reference);
                json!({"result": object_out("ViewTransform", &vt.to_bytes())})
            },
        ),
        step(
            json!({"call": "setName", "on": "vt", "args": [arg(&n)]}),
            |_| json!({"result": null}),
        ),
    ];
    if let Some(d) = dir {
        let d_name = match d {
            ViewTransformDirection::ToReference => "VIEWTRANSFORM_DIR_TO_REFERENCE",
            ViewTransformDirection::FromReference => "VIEWTRANSFORM_DIR_FROM_REFERENCE",
        };
        out.push(step(
            json!({"call": "setTransform", "on": "vt",
                   "args": [{"transform": spec}, {"enum": d_name}]}),
            |_| json!({"result": null}),
        ));
    }
    out.push(step(
        json!({"call": "addViewTransform", "args": [{"ref": "vt"}]}),
        move |c| {
            let mut vt = ViewTransform::new(reference);
            vt.set_name(&n);
            if let Some(d) = dir {
                vt.set_transform(Some(&t), d);
            }
            unit_out(c.add_view_transform(&vt))
        },
    ));
    out
}

/// Makes a named transform (stored as `nt`) with `aliases`, and adds it.
fn add_named_transform(
    name: &[u8],
    aliases: &[&[u8]],
    dir: Option<TransformDirection>,
) -> Vec<Step> {
    let n = name.to_vec();
    let aliases: Vec<Vec<u8>> = aliases.iter().map(|a| a.to_vec()).collect();
    let (spec, t) = offset_transform(0.75);
    let mut out = vec![
        step(
            json!({"new": "NamedTransform", "as": "nt"}),
            |_| json!({"result": object_out("NamedTransform", &NamedTransform::new().to_bytes())}),
        ),
        step(
            json!({"call": "setName", "on": "nt", "args": [arg(&n)]}),
            |_| json!({"result": null}),
        ),
    ];
    for a in &aliases {
        out.push(step(
            json!({"call": "addAlias", "on": "nt", "args": [arg(a)]}),
            |_| json!({"result": null}),
        ));
    }
    if let Some(d) = dir {
        let d_name = match d {
            TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
            TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
        };
        out.push(step(
            json!({"call": "setTransform", "on": "nt",
                   "args": [{"transform": spec}, {"enum": d_name}]}),
            |_| json!({"result": null}),
        ));
    }
    out.push(step(
        json!({"call": "addNamedTransform", "args": [{"ref": "nt"}]}),
        move |c| {
            let mut nt = NamedTransform::new();
            nt.set_name(&n);
            for a in &aliases {
                nt.add_alias(a);
            }
            if let Some(d) = dir {
                nt.set_transform(Some(&t), d);
            }
            unit_out(c.add_named_transform(&nt))
        },
    ));
    out
}

fn set_default_view_transform_name(name: &[u8]) -> Step {
    list_call("setDefaultViewTransformName", name, |c, v| {
        c.set_default_view_transform_name(v);
        Ok(())
    })
}

const NT_VISIBILITIES: [(NamedTransformVisibility, &str); 3] = [
    (NamedTransformVisibility::Active, "NAMEDTRANSFORM_ACTIVE"),
    (
        NamedTransformVisibility::Inactive,
        "NAMEDTRANSFORM_INACTIVE",
    ),
    (NamedTransformVisibility::All, "NAMEDTRANSFORM_ALL"),
];

/// An object the port may not find, as the oracle writes it.
fn maybe_object(class: &str, repr: Option<Vec<u8>>) -> Value {
    match repr {
        Some(r) => json!({"result": object_out(class, &r)}),
        None => json!({"result": null}),
    }
}

/// The getters of the looks, the view transforms and the named transforms, and for each probe
/// name the look, view transform and named transform of that name.
fn transform_getters(probes: &Probes) -> Vec<Step> {
    let mut out = vec![
        step(json!({"call": "getLookNames"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_looks())
                .map(|i| c.look_name_by_index(i).to_vec())
                .collect();
            texts_out(&names)
        }),
        step(json!({"call": "getViewTransformNames"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_view_transforms())
                .map(|i| c.view_transform_name_by_index(i).to_vec())
                .collect();
            texts_out(&names)
        }),
        step(json!({"call": "getDefaultViewTransformName"}), |c| {
            text_out(c.default_view_transform_name())
        }),
        step(
            json!({"call": "getDefaultSceneToDisplayViewTransform"}),
            |c| {
                maybe_object(
                    "ViewTransform",
                    c.default_scene_to_display_view_transform()
                        .map(ViewTransform::to_bytes),
                )
            },
        ),
        step(json!({"call": "getNamedTransformNames"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_named_transforms())
                .map(|i| c.named_transform_name_by_index(i).to_vec())
                .collect();
            texts_out(&names)
        }),
    ];
    for (v, v_name) in NT_VISIBILITIES {
        out.push(step(
            json!({"call": "getNamedTransformNames", "args": [{"enum": v_name}]}),
            move |c| {
                let names: Vec<Vec<u8>> = (0..c.num_named_transforms_with(v))
                    .map(|i| c.named_transform_name_by_index_with(v, i).to_vec())
                    .collect();
                texts_out(&names)
            },
        ));
    }
    for name in &probes.names {
        let n = name.clone();
        out.push(step(
            json!({"call": "getLook", "args": [arg(name)]}),
            move |c| maybe_object("Look", c.look(&n).map(Look::to_bytes)),
        ));
        let n = name.clone();
        out.push(step(
            json!({"call": "getViewTransform", "args": [arg(name)]}),
            move |c| {
                maybe_object(
                    "ViewTransform",
                    c.view_transform(&n).map(ViewTransform::to_bytes),
                )
            },
        ));
        let n = name.clone();
        out.push(step(
            json!({"call": "getNamedTransform", "args": [arg(name)]}),
            move |c| {
                maybe_object(
                    "NamedTransform",
                    c.named_transform(&n).map(NamedTransform::to_bytes),
                )
            },
        ));
    }
    out
}

/// A file rule for [`set_file_rules`]: a glob (name, color space, pattern, extension), a
/// regular expression (name, color space, expression), or the path search rule.
#[derive(Clone)]
enum Rule {
    Glob(&'static [u8], &'static [u8], &'static [u8], &'static [u8]),
    Regex(&'static [u8], &'static [u8], &'static [u8]),
    PathSearch,
}

/// Makes file rules (stored as `fr`) with `rules` before the default rule, whose color space
/// is `default` (when given), and sets them on the config. The rules must insert.
fn set_file_rules(rules: &[Rule], default: Option<&'static [u8]>) -> Vec<Step> {
    let mut out = vec![step(
        json!({"new": "FileRules", "as": "fr"}),
        |_| json!({"result": object_out("FileRules", &FileRules::new().to_bytes())}),
    )];
    for (i, rule) in rules.iter().enumerate() {
        let call = match rule {
            Rule::Glob(n, c, p, e) => json!({"call": "insertRule", "on": "fr",
                "args": [i, arg(n), arg(c), arg(p), arg(e)]}),
            Rule::Regex(n, c, x) => json!({"call": "insertRule", "on": "fr",
                "args": [i, arg(n), arg(c), arg(x)]}),
            Rule::PathSearch => json!({"call": "insertPathSearchRule", "on": "fr", "args": [i]}),
        };
        out.push(step(call, |_| json!({"result": null})));
    }
    if let Some(d) = default {
        out.push(step(
            json!({"call": "setDefaultRuleColorSpace", "on": "fr", "args": [arg(d)]}),
            |_| json!({"result": null}),
        ));
    }
    let rules = rules.to_vec();
    out.push(step(
        json!({"call": "setFileRules", "args": [{"ref": "fr"}]}),
        move |c| {
            let mut fr = FileRules::new();
            for (i, rule) in rules.iter().enumerate() {
                let inserted = match rule {
                    Rule::Glob(n, cs, p, e) => fr.insert_rule(i, n, cs, p, e),
                    Rule::Regex(n, cs, x) => fr.insert_regex_rule(i, n, cs, x),
                    Rule::PathSearch => fr.insert_path_search_rule(i),
                };
                inserted.expect("a rule the case inserts");
            }
            if let Some(d) = default {
                fr.set_default_rule_color_space(d)
                    .expect("a default color space");
            }
            c.set_file_rules(&fr);
            json!({"result": null})
        },
    ));
    out
}

/// A viewing rule for [`set_viewing_rules`]: its name, color spaces and encodings.
type VRule = (
    &'static [u8],
    &'static [&'static [u8]],
    &'static [&'static [u8]],
);

/// Makes viewing rules (stored as `vr`) and sets them on the config. The rules must insert.
fn set_viewing_rules(rules: &'static [VRule]) -> Vec<Step> {
    let mut out = vec![step(
        json!({"new": "ViewingRules", "as": "vr"}),
        |_| json!({"result": object_out("ViewingRules", &ViewingRules::new().to_bytes())}),
    )];
    for (i, (name, color_spaces, encodings)) in rules.iter().enumerate() {
        out.push(step(
            json!({"call": "insertRule", "on": "vr", "args": [i, arg(name)]}),
            |_| json!({"result": null}),
        ));
        for c in *color_spaces {
            out.push(step(
                json!({"call": "addColorSpace", "on": "vr", "args": [i, arg(c)]}),
                |_| json!({"result": null}),
            ));
        }
        for e in *encodings {
            out.push(step(
                json!({"call": "addEncoding", "on": "vr", "args": [i, arg(e)]}),
                |_| json!({"result": null}),
            ));
        }
    }
    out.push(step(
        json!({"call": "setViewingRules", "args": [{"ref": "vr"}]}),
        move |c| {
            let mut vr = ViewingRules::new();
            for (i, (name, color_spaces, encodings)) in rules.iter().enumerate() {
                vr.insert_rule(i, name).expect("a rule the case inserts");
                for cs in *color_spaces {
                    vr.add_color_space(i, cs)
                        .expect("a color space the case adds");
                }
                for e in *encodings {
                    vr.add_encoding(i, e).expect("an encoding the case adds");
                }
            }
            c.set_viewing_rules(&vr);
            json!({"result": null})
        },
    ));
    out
}

fn upgrade_to_latest_version() -> Step {
    step(json!({"call": "upgradeToLatestVersion"}), |c| {
        unit_out(c.upgrade_to_latest_version())
    })
}

/// A `std::regex_error`, as the binding raises it.
fn regex_error_out(e: &ocio::Exception) -> Value {
    match std::str::from_utf8(e.what()) {
        Ok(_) => json!({"exception": {"type": "RuntimeError", "message": bytes_arg(e.what())}}),
        Err(_) => json!({"undecodable": hex(e.what())}),
    }
}

/// An error as the binding raises it.
fn error_out(e: ocio::Exception) -> Value {
    if e.kind() == ocio::ExceptionKind::RegexError {
        regex_error_out(&e)
    } else {
        unit_out(Err(e))
    }
}

/// The getters of the file rules: the config's rules, and for each probe path the color space
/// and rule that match it, whether only the default rule does, and the color space parsed from
/// it; for each probe name, `isColorSpaceUsed`.
fn file_rule_getters(probes: &Probes) -> Vec<Step> {
    let mut out = vec![step(
        json!({"call": "getFileRules"}),
        |c| json!({"result": object_out("FileRules", &c.file_rules().get().to_bytes())}),
    )];
    for path in &probes.paths {
        let p = path.clone();
        out.push(step(
            json!({"call": "getColorSpaceFromFilepath", "args": [arg(path)]}),
            move |c| match c.color_space_from_filepath_with_index(&p) {
                Ok((cs, i)) => match std::str::from_utf8(&cs) {
                    Ok(_) => json!({"result": [bytes_arg(&cs), i]}),
                    Err(_) => json!({"undecodable": hex(&cs)}),
                },
                Err(e) => error_out(e),
            },
        ));
        let p = path.clone();
        out.push(step(
            json!({"call": "filepathOnlyMatchesDefaultRule", "args": [arg(path)]}),
            move |c| match c.filepath_only_matches_default_rule(&p) {
                Ok(b) => json!({ "result": b }),
                Err(e) => error_out(e),
            },
        ));
        let p = path.clone();
        out.push(step(
            json!({"call": "parseColorSpaceFromString", "args": [arg(path)]}),
            move |c| text_out(c.parse_color_space_from_string(&p)),
        ));
    }
    for name in &probes.names {
        let n = name.clone();
        out.push(step(
            json!({"call": "isColorSpaceUsed", "args": [arg(name)]}),
            move |c| json!({"result": c.is_color_space_used(&n)}),
        ));
    }
    out
}

/// The config's getters, then its context's `repr()`.
fn getters() -> Vec<Step> {
    vec![
        step(
            json!({"call": "getMajorVersion"}),
            |c| json!({"result": c.major_version()}),
        ),
        step(
            json!({"call": "getMinorVersion"}),
            |c| json!({"result": c.minor_version()}),
        ),
        step(json!({"call": "getName"}), |c| text_out(c.name())),
        step(json!({"call": "getDescription"}), |c| {
            text_out(c.description())
        }),
        step(json!({"call": "getFamilySeparator"}), |c| {
            text_out(&[c.family_separator()])
        }),
        step(json!({"call": "getEnvironmentVarNames"}), |c| {
            let names: Vec<Vec<u8>> = (0..c.num_environment_vars())
                .map(|i| c.environment_var_name_by_index(i).to_vec())
                .collect();
            texts_out(&names)
        }),
        step(
            json!({"call": "getEnvironmentMode"}),
            |c| json!({"result": mode(c.environment_mode())}),
        ),
        step(json!({"call": "getSearchPath"}), |c| {
            text_out(&c.search_path())
        }),
        step(json!({"call": "getSearchPaths"}), |c| {
            let paths: Vec<Vec<u8>> = (0..c.num_search_paths())
                .map(|i| c.search_path_with_index(i).to_vec())
                .collect();
            texts_out(&paths)
        }),
        step(json!({"call": "getWorkingDir"}), |c| {
            text_out(&c.working_dir())
        }),
        step(json!({"call": "getCurrentContext", "as": "context"}), |c| {
            let repr = c.current_context().get().to_bytes();
            let repr = match std::str::from_utf8(&repr) {
                Ok(_) => bytes_arg(&repr),
                Err(_) => json!({"undecodable": hex(&repr)}),
            };
            json!({"result": {"class": "Context", "repr": repr}})
        }),
    ]
}

/// The getters of a case: the config's own, then those of its color spaces and roles.
fn all_getters(probes: &Probes) -> Vec<Step> {
    let mut out = getters();
    out.extend(color_space_getters(probes));
    out.extend(model_getters(probes));
    out.extend(display_getters(probes));
    out.extend(virtual_display_getters(probes));
    out.extend(transform_getters(probes));
    out.extend(file_rule_getters(probes));
    out
}

/// The name the copy of the config is stored as.
const COPY: &str = "copy";

// ---------------------------------------------------------------------------------------------
// The check

/// The port's log while a case runs: OCIO's logging function is the process's, so the cases
/// of this binary log one at a time.
struct LogCapture {
    messages: Arc<Mutex<Vec<Vec<u8>>>>,
    _lock: MutexGuard<'static, ()>,
}

static LOGGING: Mutex<()> = Mutex::new(());

impl LogCapture {
    fn start() -> LogCapture {
        let lock = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sink = messages.clone();
        let function: LoggingFunction = Arc::new(move |m: &[u8]| {
            sink.lock().unwrap().push(m.to_vec());
        });
        set_logging_function(Some(function)).expect("a logging function");
        LogCapture {
            messages,
            _lock: lock,
        }
    }

    /// The messages logged since the last call.
    fn take(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.messages.lock().unwrap())
    }
}

impl Drop for LogCapture {
    fn drop(&mut self) {
        reset_to_default_logging_function();
    }
}

/// Runs `steps` (each followed by the getters) on a new config in the environment `env`, on
/// both sides, and compares every outcome. `steps` may hold `None`: a copy of the config.
fn check(label: &str, env: &[(&str, &[u8])], steps: Vec<Option<Step>>) {
    let items = steps
        .into_iter()
        .map(|s| s.map_or(Item::Copy, Item::One))
        .collect();
    check_items(label, "new", env, items, &Probes::default());
}

/// As [`check`], on the config of `source`: `"new"` (`Config()`) or `"raw"`
/// (`Config.CreateRaw()`).
fn check_source(label: &str, source: &str, env: &[(&str, &[u8])], steps: Vec<Option<Step>>) {
    let items = steps
        .into_iter()
        .map(|s| s.map_or(Item::Copy, Item::One))
        .collect();
    check_items(label, source, env, items, &Probes::default());
}

/// Runs `items` on the config of `source`, each followed by the getters of `probes`.
fn check_items(label: &str, source: &str, env: Env, items: Vec<Item>, probes: &Probes) {
    let mut sequence: Vec<Option<Step>> = Vec::new();
    sequence.extend(all_getters(probes).into_iter().map(Some));
    for item in items {
        match item {
            Item::One(s) => sequence.push(Some(s)),
            Item::Group(g) => sequence.extend(g.into_iter().map(Some)),
            Item::Copy => sequence.push(None),
        }
        sequence.extend(all_getters(probes).into_iter().map(Some));
    }

    let mut calls = Vec::new();
    let mut on = "config".to_string();
    for s in &sequence {
        match s {
            Some(s) => {
                let mut call = s.call.clone();
                if call.get("call").is_some() && call.get("on").is_none() {
                    call["on"] = json!(on);
                }
                calls.push(call);
            }
            None => {
                calls.push(json!({"copy": on, "as": COPY}));
                on = COPY.to_string();
            }
        }
    }
    let env_json: serde_json::Map<String, Value> = env
        .iter()
        .map(|(k, v)| {
            let value = match std::str::from_utf8(v) {
                Ok(text) => json!(text),
                Err(_) => arg(v),
            };
            (k.to_string(), value)
        })
        .collect();
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": source, "env": env_json, "calls": calls}),
        &[],
    );
    let wheel = &response.result;

    set_thread_env_provider(Some(Arc::new(MapEnv::default())));
    for (name, value) in env {
        setenv(name, value).expect("a request's variable");
    }
    let capture = LogCapture::start();
    let made = match source {
        "new" => Config::new(),
        "raw" => Config::create_raw().map(|c| Arc::try_unwrap(c).expect("a config of its own")),
        _ => panic!("unknown source {source}"),
    };
    let mut failures = Vec::new();
    let mut config = match made {
        Ok(config) => {
            assert_eq!(
                wheel["config"],
                Value::Null,
                "{label}: the wheel refused the config"
            );
            config
        }
        Err(e) => {
            set_thread_env_provider(None);
            assert_eq!(
                wheel["config"],
                unit_out(Err(e)),
                "{label}: the port refused the config"
            );
            return;
        }
    };
    let results = wheel["calls"].as_array().expect("the calls' results");
    assert_eq!(results.len(), sequence.len(), "{label}");
    assert_eq!(
        capture.take(),
        log(&wheel["config_log"]),
        "{label}: the logs of making the config"
    );
    for (i, (s, w)) in sequence.iter().zip(results).enumerate() {
        let wheel_log = log(&w["log"]);
        let mut w = w.clone();
        w.as_object_mut().expect("a call's outcome").remove("log");
        match s {
            Some(s) => {
                let port = (s.port)(&mut config);
                let port_log = capture.take();
                if port_log != wheel_log {
                    failures.push(format!(
                        "{label}, call {i} {}: wheel log {:?}, port log {:?}",
                        s.call,
                        wheel_log
                            .iter()
                            .map(|m| String::from_utf8_lossy(m))
                            .collect::<Vec<_>>(),
                        port_log
                            .iter()
                            .map(|m| String::from_utf8_lossy(m))
                            .collect::<Vec<_>>()
                    ));
                }
                if port != w {
                    failures.push(format!(
                        "{label}, call {i} {}: wheel {w}, port {port}",
                        s.call
                    ));
                }
            }
            None => {
                config = config.clone();
                assert!(w.get("result").is_some(), "{label}, call {i}: {w}");
            }
        }
    }
    set_thread_env_provider(None);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// Cases

#[test]
fn a_new_config_matches_the_wheel() {
    check("defaults", &[], vec![]);
}

#[test]
fn versions_match_the_wheel() {
    check(
        "versions",
        &[],
        vec![
            Some(set_minor_version(6)),
            Some(set_minor_version(0)),
            Some(set_minor_version(5)),
            Some(set_major_version(1)),
            Some(set_minor_version(1)),
            Some(set_minor_version(0)),
            Some(set_major_version(0)),
            Some(set_major_version(3)),
            Some(set_major_version(u32::MAX)),
            Some(set_version(2, 9)),
            Some(set_version(1, 0)),
            Some(set_version(3, 4)),
            Some(set_version(2, 3)),
            Some(set_version(1, 1)),
            Some(set_version(2, 0)),
            None,
            Some(set_minor_version(4)),
        ],
    );
}

#[test]
fn name_description_and_family_separator_match_the_wheel() {
    let mut steps = vec![
        Some(set_name(b"my config")),
        Some(set_description(b"line 1\nline 2")),
        Some(set_name(b"a\0b")),
        Some(set_description(b"\xff\xfe")),
        Some(set_description(b"")),
        Some(set_name(b"\xc3\xa9t\xc3\xa9")),
        None,
        Some(set_name(b"")),
    ];
    for separator in [
        b' ', 0, b'/', 0x7f, 0x1f, b'~', b'!', 31, 32, 126, 127, 0x80, 0xe9, 0xff, b'\n',
    ] {
        steps.push(Some(set_family_separator(separator)));
    }
    steps.push(None);
    check("name, description and family separator", &[], steps);
}

#[test]
fn environment_variables_match_the_wheel() {
    let env: &[(&str, &[u8])] = &[("OCIO_TEST_A", b"from the environment"), ("HOME2", b"/h")];
    check(
        "environment variables",
        env,
        vec![
            Some(add_environment_var(b"B", Some(b"1"))),
            Some(add_environment_var(b"A", Some(b"2"))),
            Some(add_environment_var(b"a", Some(b"lower"))),
            Some(add_environment_var(b"C", Some(b""))),
            Some(add_environment_var(b"", Some(b"ignored"))),
            Some(add_environment_var(b"D\0E", Some(b"v\0w"))),
            Some(add_environment_var(b"\xc3\xa9", Some(b"\xff"))),
            Some(get_environment_var_default(b"A")),
            Some(get_environment_var_default(b"a")),
            Some(get_environment_var_default(b"C")),
            Some(get_environment_var_default(b"D")),
            Some(get_environment_var_default(b"unknown")),
            Some(get_environment_var_default(b"")),
            Some(get_environment_var_default(b"\xc3\xa9")),
            Some(add_environment_var(b"A", None)),
            Some(add_environment_var(b"unknown", None)),
            Some(add_environment_var(b"B", Some(b"1"))),
            Some(add_environment_var(b"OCIO_TEST_A", Some(b"default"))),
            Some(load_environment()),
            None,
            Some(add_environment_var(b"Z", Some(b"z"))),
            Some(set_environment_mode(EnvironmentMode::LoadAll)),
            Some(load_environment()),
            None,
            Some(load_environment()),
            Some(set_environment_mode(EnvironmentMode::LoadAll)),
            Some(load_environment()),
            Some(clear_environment_vars()),
            Some(set_environment_mode(EnvironmentMode::Unknown)),
            Some(load_environment()),
        ],
    );
}

#[test]
fn search_paths_and_working_dir_match_the_wheel() {
    check(
        "search paths",
        &[],
        vec![
            Some(set_search_path(b"a:b:c")),
            Some(add_search_path(b"")),
            Some(add_search_path(b"d")),
            Some(add_search_path(b"\0e")),
            Some(add_search_path(b"f:g")),
            Some(set_working_dir(b"/work/dir")),
            None,
            Some(set_search_path(b"")),
            Some(add_search_path(b"$VAR/luts")),
            Some(set_search_path(b"C:\\luts;D:\\other:x")),
            Some(clear_search_paths()),
            Some(set_working_dir(b"")),
            Some(set_working_dir(b"\xff")),
        ],
    );
}

/// The active displays and views of the environment, read when the config is made: the
/// config is refused when a list opens a quote it doesn't close before a separator.
#[test]
fn the_environment_lists_match_the_wheel() {
    let cases: &[(&str, Env)] = &[
        (
            "an unclosed quote in the active views",
            &[("OCIO_ACTIVE_VIEWS", b"\"a,b")],
        ),
        (
            "an unclosed quote in the active displays",
            &[("OCIO_ACTIVE_DISPLAYS", b"x:\"y")],
        ),
        (
            "both refused",
            &[
                ("OCIO_ACTIVE_DISPLAYS", b"\"d1,d2"),
                ("OCIO_ACTIVE_VIEWS", b"\"v1,v2"),
            ],
        ),
        ("an unclosed quote alone", &[("OCIO_ACTIVE_VIEWS", b"\"a")]),
        (
            "quoted lists",
            &[
                ("OCIO_ACTIVE_DISPLAYS", b" \"a,b\", c "),
                ("OCIO_ACTIVE_VIEWS", b"v1:v2"),
                ("OCIO_INACTIVE_COLORSPACES", b"  cs1, cs2  "),
            ],
        ),
        (
            "empty lists",
            &[
                ("OCIO_ACTIVE_DISPLAYS", b"   "),
                ("OCIO_ACTIVE_VIEWS", b""),
                ("OCIO_INACTIVE_COLORSPACES", b""),
            ],
        ),
        ("not UTF-8", &[("OCIO_ACTIVE_VIEWS", b"\"\xff,b")]),
    ];
    for (label, env) in cases {
        if cfg!(windows) && env.iter().any(|(_, v)| std::str::from_utf8(v).is_err()) {
            // The oracle sets Windows variables from text only.
            continue;
        }
        check(label, env, vec![]);
    }
}

/// The raw config: its version, and the whole environment in its context.
#[test]
fn the_raw_config_matches_the_wheel() {
    let env: &[(&str, &[u8])] = &[("OCIO_TEST_A", b"a"), ("PATH_LIKE", b"x:y")];
    check_source(
        "raw",
        "raw",
        env,
        vec![
            None,
            Some(add_environment_var(b"OCIO_TEST_A", Some(b"default"))),
            Some(load_environment()),
        ],
    );
}

/// Color spaces: added, refused (empty names, conflicts with roles and other color spaces,
/// context variable tokens in either version), replaced, looked up by name, alias and role,
/// searched by reference space and category, removed and cleared; and roles: set, refused,
/// unset, and set to an empty name.
#[test]
fn color_spaces_and_roles_match_the_wheel() {
    let items: Vec<Item> = vec![
        add_color_space(cs(b"a").alias(b"a1").alias(b"A2").category(b"cat1")).into(),
        add_color_space(cs(b"b").display().category(b"cat1").category(b"Cat2")).into(),
        add_color_space(cs(b"c")).into(),
        add_color_space(cs(b"")).into(),
        add_color_space(cs(b"A1")).into(),
        add_color_space(cs(b"d").alias(b"C")).into(),
        add_color_space(cs(b"A").alias(b"a3").display()).into(),
        add_color_space(cs(b"e$")).into(),
        add_color_space(cs(b"e").alias(b"%e")).into(),
        set_role(b"role1", Some(b"b")).into(),
        set_role(b"Role2", Some(b"A2")).into(),
        add_color_space(cs(b"ROLE1")).into(),
        add_color_space(cs(b"f").alias(b"role2")).into(),
        set_role(b"a", Some(b"c")).into(),
        set_role(b"A3", Some(b"c")).into(),
        set_role(b"$r", Some(b"c")).into(),
        set_role(b"", Some(b"c")).into(),
        set_role(b"\0r", Some(b"c")).into(),
        set_role(b"r3", Some(b"")).into(),
        set_role(b"role1", Some(b"c")).into(),
        set_role(b"unknown_cs", Some(b"nowhere")).into(),
        set_role(b"ROLE2", None).into(),
        set_role(b"never_set", None).into(),
        set_major_version(1).into(),
        add_color_space(cs(b"g$").alias(b"g%")).into(),
        add_color_space(cs(b"g$")).into(),
        set_role(b"$r", Some(b"c")).into(),
        set_major_version(2).into(),
        Item::Copy,
        remove_color_space(b"a3").into(),
        remove_color_space(b"B").into(),
        remove_color_space(b"unknown").into(),
        add_color_space(cs(b"h").category(b"cat1")).into(),
        clear_color_spaces().into(),
        add_color_space(cs(b"c")).into(),
    ];
    let probes = probes(
        &[
            b"a",
            b"A",
            b"a1",
            b"A2",
            b"a3",
            b"b",
            b"B",
            b"c",
            b"C",
            b"d",
            b"g$",
            b"role1",
            b"ROLE1",
            b"role2",
            b"r3",
            b"$r",
            b"unknown_cs",
            b"unknown",
            b"",
        ],
        &[b"cat1", b"CAT1", b"cat2", b"", b"missing"],
    );
    check_items("color spaces and roles", "new", &[], items, &probes);
}

/// The inactive color spaces of `OCIO_INACTIVE_COLORSPACES`, by name, alias or role, the
/// unknown ones left out, as the color spaces come and go.
#[test]
fn inactive_color_spaces_of_the_environment_match_the_wheel() {
    let env: Env = &[("OCIO_INACTIVE_COLORSPACES", b" b , A2,unknown, role1,, c")];
    let items: Vec<Item> = vec![
        add_color_space(cs(b"a").alias(b"a2")).into(),
        add_color_space(cs(b"b").display()).into(),
        add_color_space(cs(b"c").display()).into(),
        add_color_space(cs(b"d")).into(),
        set_role(b"role1", Some(b"d")).into(),
        add_color_space(cs(b"e")).into(),
        Item::Copy,
        remove_color_space(b"b").into(),
        set_role(b"role1", None).into(),
        add_color_space(cs(b"f")).into(),
    ];
    let probes = probes(&[b"a", b"b", b"c", b"d", b"role1"], &[b""]);
    check_items("inactive color spaces", "new", env, items, &probes);
}

/// The raw config's color space and role.
#[test]
fn the_raw_config_color_space_matches_the_wheel() {
    let probes = probes(&[b"raw", b"RAW", b"default", b"Default", b"data"], &[b""]);
    check_items(
        "raw color space",
        "raw",
        &[],
        vec![
            add_color_space(cs(b"default")).into(),
            set_role(b"raw", Some(b"raw")).into(),
        ],
        &probes,
    );
}

/// The list of inactive color spaces of the API, which supersedes the environment's, and
/// `isInactiveColorSpace`, which reads the config's list only; strict parsing; the default luma.
#[test]
fn inactive_lists_strict_parsing_and_luma_match_the_wheel() {
    let env: Env = &[("OCIO_INACTIVE_COLORSPACES", b"b")];
    let items: Vec<Item> = vec![
        add_color_space(cs(b"a").alias(b"a2")).into(),
        add_color_space(cs(b"b").display()).into(),
        add_color_space(cs(b"c")).into(),
        set_role(b"role1", Some(b"c")).into(),
        set_inactive_color_spaces(b" a2, role1 ,unknown ").into(),
        set_inactive_color_spaces(b"a, b").into(),
        set_inactive_color_spaces(b"a,b").into(),
        set_inactive_color_spaces(b"A, , c").into(),
        Item::Copy,
        set_inactive_color_spaces(b"").into(),
        set_inactive_color_spaces(b"   ").into(),
        set_inactive_color_spaces(b"c\0b").into(),
        set_strict_parsing_enabled(false).into(),
        set_default_luma_coefs([0.25, -0.0, f64::MAX]).into(),
        Item::Copy,
        set_strict_parsing_enabled(true).into(),
        set_default_luma_coefs([1.0 / 3.0, f64::MIN_POSITIVE, 0.0]).into(),
    ];
    let probes = probes(
        &[
            b"a", b"A", b"a2", b"b", b"c", b"role1", b"unknown", b"", b"a, b",
        ],
        &[],
    );
    check_items("inactive lists", "new", env, items, &probes);
}

/// `isColorSpaceLinear`: data spaces, the other reference space, encodings, transforms in
/// either direction (linear or not), no transform, and unknown spaces.
#[test]
fn color_space_linearity_matches_the_wheel() {
    let matrix = || {
        let mut m = MatrixTransform::new();
        m.set_matrix(&[
            2.0, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        (
            json!({"class": "MatrixTransform", "calls": [["setMatrix",
                [2.0, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0]]]}),
            Transform::from(m),
        )
    };
    let offset = || {
        let mut m = MatrixTransform::new();
        m.set_offset(&[0.01, 0.0, 0.0, 0.0]);
        (
            json!({"class": "MatrixTransform", "calls": [["setOffset", [0.01, 0.0, 0.0, 0.0]]]}),
            Transform::from(m),
        )
    };
    let log = || {
        (
            json!({"class": "LogTransform"}),
            Transform::from(LogTransform::new()),
        )
    };
    let with = |c: Cs, (spec, t): (Value, Transform), dir| c.transform(spec, t, dir);
    let to = ColorSpaceDirection::ToReference;
    let from = ColorSpaceDirection::FromReference;
    let items: Vec<Item> = vec![
        add_color_space(cs(b"plain")).into(),
        add_color_space(cs(b"plain_display").display()).into(),
        add_color_space(cs(b"data").data()).into(),
        add_color_space(cs(b"enc_scene").encoding(b"Scene-Linear")).into(),
        add_color_space(cs(b"enc_display").display().encoding(b"display-linear")).into(),
        add_color_space(cs(b"enc_other").encoding(b"log")).into(),
        add_color_space(with(cs(b"matrix_to"), matrix(), to)).into(),
        add_color_space(with(cs(b"matrix_from").display(), matrix(), from)).into(),
        add_color_space(with(cs(b"log_to"), log(), to)).into(),
        add_color_space(with(cs(b"log_from"), log(), from)).into(),
        add_color_space(with(with(cs(b"both"), matrix(), to), log(), from)).into(),
        add_color_space(with(cs(b"offset"), offset(), to)).into(),
        add_color_space(with(cs(b"data_log").data(), log(), to)).into(),
        add_color_space(with(cs(b"enc_log").encoding(b"scene-linear"), log(), to)).into(),
        set_role(b"lin_role", Some(b"matrix_to")).into(),
    ];
    let probes = probes(
        &[
            b"plain",
            b"plain_display",
            b"data",
            b"enc_scene",
            b"enc_display",
            b"enc_other",
            b"matrix_to",
            b"matrix_from",
            b"log_to",
            b"log_from",
            b"both",
            b"offset",
            b"data_log",
            b"enc_log",
            b"lin_role",
            b"unknown",
            b"",
        ],
        &[],
    );
    check_items("linearity", "new", &[], items, &probes);
}

/// The raw config reads the environment's lists as any config does: its color space inactive
/// by `OCIO_INACTIVE_COLORSPACES`, and refused for an unclosed quote in a list.
#[test]
fn the_raw_config_reads_the_environment_lists() {
    let probes = probes(&[b"raw", b"default"], &[b""]);
    let inactive: Env = &[("OCIO_INACTIVE_COLORSPACES", b"default")];
    check_items("raw, inactive", "raw", inactive, vec![Item::Copy], &probes);
    let refused: Env = &[("OCIO_ACTIVE_DISPLAYS", b"\"sRGB,b")];
    check_items("raw, refused", "raw", refused, vec![], &probes);
}

/// A context taken from the config before the calls that follow, kept on both sides.
type Held = Rc<RefCell<Option<CurrentContext>>>;

/// `getCurrentContext`, kept as `held`.
fn hold_context(held: &Held) -> Step {
    let held = held.clone();
    step(
        json!({"call": "getCurrentContext", "as": "held"}),
        move |c| {
            let ctx = c.current_context();
            let repr = ctx.get().to_bytes();
            *held.borrow_mut() = Some(ctx);
            json!({"result": object_out("Context", &repr)})
        },
    )
}

/// What the held context says now: its `repr()`, search path, working directory, environment
/// mode and variables.
fn held_getters(held: &Held) -> Vec<Step> {
    let get = |held: &Held| held.borrow().as_ref().expect("a held context").get();
    let h = held.clone();
    let repr = step(json!({"call": "__repr__", "on": "held"}), move |_| {
        text_out(&get(&h).to_bytes())
    });
    let h = held.clone();
    let path = step(json!({"call": "getSearchPath", "on": "held"}), move |_| {
        text_out(get(&h).search_path())
    });
    let h = held.clone();
    let dir = step(json!({"call": "getWorkingDir", "on": "held"}), move |_| {
        text_out(get(&h).working_dir())
    });
    let h = held.clone();
    let env_mode = step(
        json!({"call": "getEnvironmentMode", "on": "held"}),
        move |_| json!({"result": mode(get(&h).environment_mode())}),
    );
    let h = held.clone();
    let vars = step(json!({"call": "getStringVars", "on": "held"}), move |_| {
        let ctx = get(&h);
        let pairs: Vec<Value> = (0..ctx.num_string_vars())
            .map(|i| {
                json!([
                    bytes_arg(ctx.string_var_name_by_index(i)),
                    bytes_arg(ctx.string_var_by_index(i))
                ])
            })
            .collect();
        json!({ "result": pairs })
    });
    vec![repr, path, dir, env_mode, vars]
}

/// A context taken from the config before a change sees the change, as upstream's config and
/// the callers of `getCurrentContext` share one context; a copy of the config has its own.
#[test]
fn a_held_context_sees_later_changes() {
    let held: Held = Rc::new(RefCell::new(None));
    let env: Env = &[("OCIO_TEST_A", b"a")];
    let items: Vec<Item> = vec![
        hold_context(&held).into(),
        held_getters(&held).into(),
        set_search_path(b"a:b").into(),
        held_getters(&held).into(),
        add_search_path(b"c").into(),
        set_working_dir(b"/w").into(),
        add_environment_var(b"V", Some(b"1")).into(),
        set_environment_mode(EnvironmentMode::LoadAll).into(),
        load_environment().into(),
        held_getters(&held).into(),
        clear_search_paths().into(),
        clear_environment_vars().into(),
        held_getters(&held).into(),
        Item::Copy,
        set_search_path(b"copy").into(),
        set_working_dir(b"/copy").into(),
        held_getters(&held).into(),
        hold_context(&held).into(),
        held_getters(&held).into(),
        add_search_path(b"after").into(),
        held_getters(&held).into(),
    ];
    check_items("held context", "new", env, items, &Probes::default());
}

/// Displays, their views and shared views: added, replaced, refused, removed (with the
/// display when it has no view left) and cleared; the active displays and views of the
/// environment; the views of each display, by type and for an image's color space (an unknown
/// color space is an error); the getters of each (display, view) pair, and of the shared views
/// with an empty display.
#[test]
fn displays_and_views_match_the_wheel() {
    let env: Env = &[
        ("OCIO_ACTIVE_DISPLAYS", b"sRGB, unknown, P3"),
        ("OCIO_ACTIVE_VIEWS", b"v2, shared1,Raw"),
    ];
    let items: Vec<Item> = vec![
        add_color_space(cs(b"raw")).into(),
        add_color_space(cs(b"disp").display()).into(),
        add_display_view(b"sRGB", b"Raw", b"raw", b"").into(),
        add_display_view(b"sRGB", b"v2", b"disp", b"look1").into(),
        add_display_view_full(b"P3", b"v1", b"vt", b"disp", b"").into(),
        add_display_view_full(b"P3", b"ruled", b"vt", b"raw", b"some_rule").into(),
        add_display_view(b"Other", b"v3", b"raw", b"").into(),
        add_display_view(b"SRGB", b"RAW", b"<use_display_name>", b"").into(),
        add_display_view(b"", b"v", b"raw", b"").into(),
        add_display_view(b"sRGB", b"", b"raw", b"").into(),
        add_display_view(b"sRGB", b"v", b"", b"").into(),
        add_shared_view(b"shared1", b"vt", b"<USE_DISPLAY_NAME>").into(),
        add_shared_view(b"shared2", b"", b"raw").into(),
        add_shared_view(b"", b"", b"raw").into(),
        add_shared_view(b"shared3", b"", b"").into(),
        add_display_shared_view(b"sRGB", b"shared1").into(),
        add_display_shared_view(b"sRGB", b"SHARED1").into(),
        add_display_shared_view(b"sRGB", b"raw").into(),
        add_display_shared_view(b"New", b"shared2").into(),
        add_display_shared_view(b"New", b"missing").into(),
        add_display_shared_view(b"", b"shared2").into(),
        add_display_shared_view(b"P3", b"").into(),
        add_display_view(b"sRGB", b"Shared1", b"raw", b"").into(),
        Item::Copy,
        remove_shared_view(b"SHARED2").into(),
        remove_shared_view(b"shared2").into(),
        remove_shared_view(b"").into(),
        remove_display_view(b"New", b"shared2").into(),
        remove_display_view(b"New", b"shared2").into(),
        remove_display_view(b"sRGB", b"nope").into(),
        remove_display_view(b"", b"v").into(),
        remove_display_view(b"P3", b"").into(),
        remove_display_view(b"other", b"V3").into(),
        add_shared_view(b"shared4", b"", b"raw").into(),
        clear_shared_views().into(),
        clear_displays().into(),
        add_display_view(b"P3", b"v1", b"raw", b"").into(),
    ];
    let probes = probes(&[b"raw", b"disp", b"RAW", b"unknown", b""], &[]).displays(
        &[b"sRGB", b"srgb", b"P3", b"Other", b"New", b"unknown", b""],
        &[
            b"Raw", b"v2", b"v1", b"ruled", b"shared1", b"shared2", b"missing", b"",
        ],
    );
    check_items("displays", "new", env, items, &probes);
}

/// The raw config's display and view.
#[test]
fn the_raw_config_display_matches_the_wheel() {
    let probes = probes(&[b"raw"], &[]).displays(&[b"sRGB"], &[b"Raw"]);
    check_items("raw display", "raw", &[], vec![], &probes);
}

/// The config's active displays and views over all of them (the environment's lists, when
/// set, win), their setters and refusals; all the displays and the temporary ones; the virtual
/// display's views, shared or its own, and their getters; the instantiation of displays,
/// refused without a name or a path.
#[test]
fn active_lists_and_the_virtual_display_match_the_wheel() {
    let items: Vec<Item> = vec![
        add_color_space(cs(b"raw")).into(),
        add_display_view(b"sRGB", b"Raw", b"raw", b"").into(),
        add_display_view(b"sRGB", b"v2", b"raw", b"").into(),
        add_display_view(b"P3", b"v1", b"raw", b"").into(),
        add_display_view(b"P3", b"Raw", b"raw", b"").into(),
        add_shared_view(b"shared1", b"vt", b"raw").into(),
        set_active_displays(b"P3, unknown").into(),
        set_active_views(b"v1:raw").into(),
        set_active_displays(b"\"a,b\", P3").into(),
        set_active_displays(b"\"a,b").into(),
        set_active_displays(b"").into(),
        set_active_displays(b"  ").into(),
        add_active_display(b"sRGB").into(),
        add_active_display(b"sRGB").into(),
        add_active_display(b"SRGB").into(),
        add_active_display(b"").into(),
        remove_active_display(b"srgb").into(),
        remove_active_display(b"SRGB").into(),
        remove_active_display(b"").into(),
        clear_step("clearActiveDisplays", |c| c.clear_active_displays()).into(),
        add_active_view(b"v2").into(),
        add_active_view(b"").into(),
        add_active_view(b"Raw").into(),
        remove_active_view(b"raw").into(),
        remove_active_view(b"Raw").into(),
        set_active_views(b"\"x:y").into(),
        set_active_views(b"x:y, z").into(),
        clear_step("clearActiveViews", |c| c.clear_active_views()).into(),
        set_display_temporary(b"p3", true).into(),
        set_display_temporary(b"unknown", true).into(),
        add_virtual_display_view(b"vv", b"vt", b"<USE_DISPLAY_NAME>").into(),
        add_virtual_display_view(b"VV", b"", b"raw").into(),
        add_virtual_display_view(b"", b"", b"raw").into(),
        add_virtual_display_view(b"w", b"", b"").into(),
        add_virtual_display_view(b"shared1", b"", b"raw").into(),
        add_virtual_display_shared_view(b"shared1").into(),
        add_virtual_display_shared_view(b"SHARED1").into(),
        add_virtual_display_shared_view(b"missing").into(),
        add_virtual_display_shared_view(b"").into(),
        instantiate_display_from_icc_profile(b"").into(),
        instantiate_display_from_monitor_name(b"").into(),
        Item::Copy,
        remove_virtual_display_view(b"VV").into(),
        remove_virtual_display_view(b"missing").into(),
        remove_virtual_display_view(b"shared1").into(),
        remove_virtual_display_view(b"shared1").into(),
        add_virtual_display_view(b"vv", b"", b"raw").into(),
        clear_step("clearVirtualDisplay", |c| c.clear_virtual_display()).into(),
        set_display_temporary(b"P3", false).into(),
    ];
    let probes = probes(&[b"raw"], &[]).displays(
        &[b"sRGB", b"P3", b"p3", b"unknown"],
        &[b"Raw", b"v1", b"v2", b"vv", b"shared1", b"missing", b""],
    );
    check_items(
        "active lists and virtual display",
        "new",
        &[],
        items,
        &probes,
    );
    let env: Env = &[
        ("OCIO_ACTIVE_DISPLAYS", b"sRGB"),
        ("OCIO_ACTIVE_VIEWS", b"v2"),
    ];
    let items: Vec<Item> = vec![
        add_color_space(cs(b"raw")).into(),
        add_display_view(b"sRGB", b"Raw", b"raw", b"").into(),
        add_display_view(b"sRGB", b"v2", b"raw", b"").into(),
        add_display_view(b"P3", b"v1", b"raw", b"").into(),
        set_active_displays(b"P3").into(),
        set_active_views(b"v1, Raw").into(),
    ];
    check_items(
        "active lists under the environment's",
        "new",
        env,
        items,
        &probes,
    );
}

/// Looks, view transforms and the default one, and named transforms: added, replaced,
/// refused (empty names, no transform, conflicts with roles, color spaces and each other's
/// names and aliases, context variable tokens), looked up ignoring case and by alias,
/// cleared; the named transforms made inactive by the API's list and the environment's; the
/// canonical names of named transforms.
#[test]
fn looks_view_transforms_and_named_transforms_match_the_wheel() {
    let fwd = Some(TransformDirection::Forward);
    let inv = Some(TransformDirection::Inverse);
    let to = Some(ViewTransformDirection::ToReference);
    let from = Some(ViewTransformDirection::FromReference);
    let items: Vec<Item> = vec![
        add_color_space(cs(b"raw").alias(b"raw_alias")).into(),
        set_role(b"role1", Some(b"raw")).into(),
        add_look(b"look1", b"raw", true).into(),
        add_look(b"Look2", b"other", false).into(),
        add_look(b"LOOK1", b"raw2", false).into(),
        add_look(b"", b"raw", true).into(),
        add_view_transform(b"vt1", ReferenceSpaceType::Display, to).into(),
        add_view_transform(b"vt2", ReferenceSpaceType::Scene, from).into(),
        add_view_transform(b"VT3", ReferenceSpaceType::Scene, to).into(),
        add_view_transform(b"vt4", ReferenceSpaceType::Scene, None).into(),
        add_view_transform(b"", ReferenceSpaceType::Scene, to).into(),
        set_default_view_transform_name(b"vt3").into(),
        set_default_view_transform_name(b"vt1").into(),
        set_default_view_transform_name(b"missing").into(),
        add_view_transform(b"VT2", ReferenceSpaceType::Display, to).into(),
        add_named_transform(b"nt1", &[b"nt_alias", b"NT1b"], fwd).into(),
        add_named_transform(b"nt2", &[], inv).into(),
        add_named_transform(b"nt3", &[], None).into(),
        add_named_transform(b"", &[], fwd).into(),
        add_named_transform(b"role1", &[], fwd).into(),
        add_named_transform(b"RAW_ALIAS", &[], fwd).into(),
        add_named_transform(b"nt$", &[], fwd).into(),
        add_named_transform(b"nt4", &[b"Role1"], fwd).into(),
        add_named_transform(b"nt4", &[b"raw"], fwd).into(),
        add_named_transform(b"nt4", &[b"a%"], fwd).into(),
        add_named_transform(b"nt4", &[b"NT_ALIAS"], fwd).into(),
        add_named_transform(b"nt_alias", &[], fwd).into(),
        add_named_transform(b"NT1", &[b"nt_alias"], inv).into(),
        add_named_transform(b"nt5", &[b"nt2"], fwd).into(),
        add_color_space(cs(b"nt2")).into(),
        add_color_space(cs(b"x").alias(b"nt1b")).into(),
        set_role(b"nt2", Some(b"raw")).into(),
        set_inactive_color_spaces(b"nt2, raw, nt_alias").into(),
        Item::Copy,
        clear_step("clearLooks", |c| c.clear_looks()).into(),
        clear_step("clearViewTransforms", |c| c.clear_view_transforms()).into(),
        set_inactive_color_spaces(b"").into(),
        clear_step("clearNamedTransforms", |c| c.clear_named_transforms()).into(),
        add_named_transform(b"nt6", &[], fwd).into(),
    ];
    let probes = probes(
        &[
            b"look1",
            b"LOOK2",
            b"vt1",
            b"vt2",
            b"Vt3",
            b"vt4",
            b"nt1",
            b"NT_ALIAS",
            b"nt1b",
            b"nt2",
            b"nt3",
            b"nt6",
            b"raw",
            b"raw_alias",
            b"role1",
            b"missing",
            b"",
        ],
        &[],
    );
    check_items("looks and transforms", "new", &[], items, &probes);

    let env: Env = &[("OCIO_INACTIVE_COLORSPACES", b"NT_ALIAS, raw")];
    let items: Vec<Item> = vec![
        add_color_space(cs(b"raw")).into(),
        add_named_transform(b"nt1", &[b"nt_alias"], fwd).into(),
        add_named_transform(b"nt2", &[], fwd).into(),
    ];
    let more = probes_of(&[b"nt1", b"nt2", b"raw"]);
    check_items("inactive named transforms", "new", env, items, &more);
}

// ---------------------------------------------------------------------------------------------
// The cache of processors: which calls empty it (`Config::Impl::resetCacheIDs`), seen through
// the identity of the processors getProcessor returns before and after each call. The
// verifier's harness (2026-10-06), adapted.

/// A processor taken from the config, kept on the port's side.
type HeldProcessor = Rc<RefCell<Option<Arc<ocio::Processor>>>>;

/// `getProcessor` of an offset, stored as `store` on both sides.
/// A port action on the config, with no outcome.
type ConfigAction = Box<dyn Fn(&mut Config)>;

fn get_processor(store: &'static str, held: &HeldProcessor) -> (Value, ConfigAction) {
    let (spec, t) = offset_transform(0.5);
    let h = held.clone();
    (
        json!({"call": "getProcessor", "args": [{"transform": spec}], "as": store}),
        Box::new(move |c: &mut Config| {
            *h.borrow_mut() = Some(c.processor(&t).expect("a processor"));
        }),
    )
}

/// Every setter, failing and no-op calls included, keeps or empties the config's cache of
/// processors as the wheel's does: the processor of the same transform after the call is the
/// one before it (`p1 == p2` in the binding, `Arc::ptr_eq` here), or a new one.
#[test]
fn processor_cache_resets_match_the_wheel() {
    let mut cases: Vec<(&str, Vec<Step>)> = vec![
        ("setMajorVersion(2)", vec![set_major_version(2)]),
        ("setMajorVersion(9) fails", vec![set_major_version(9)]),
        ("setMinorVersion(3)", vec![set_minor_version(3)]),
        ("setVersion(2,1)", vec![set_version(2, 1)]),
        ("setName", vec![set_name(b"n")]),
        ("setDescription", vec![set_description(b"d")]),
        ("setFamilySeparator", vec![set_family_separator(b'~')]),
        ("setFamilySeparator fails", vec![set_family_separator(0x80)]),
        (
            "addEnvironmentVar",
            vec![add_environment_var(b"E", Some(b"1"))],
        ),
        (
            "addEnvironmentVar empty",
            vec![add_environment_var(b"", Some(b"1"))],
        ),
        (
            "addEnvironmentVar None",
            vec![add_environment_var(b"Q", None)],
        ),
        ("clearEnvironmentVars", vec![clear_environment_vars()]),
        (
            "setEnvironmentMode",
            vec![set_environment_mode(EnvironmentMode::LoadAll)],
        ),
        ("loadEnvironment", vec![load_environment()]),
        ("setSearchPath", vec![set_search_path(b"a")]),
        ("addSearchPath empty", vec![add_search_path(b"")]),
        ("addSearchPath", vec![add_search_path(b"b")]),
        ("clearSearchPaths", vec![clear_search_paths()]),
        ("setWorkingDir", vec![set_working_dir(b"/w")]),
        ("addColorSpace", add_color_space(cs(b"a"))),
        ("addColorSpace fails", add_color_space(cs(b""))),
        (
            "addColorSpace alias fails",
            add_color_space(cs(b"q").alias(b"%")),
        ),
        ("removeColorSpace missing", vec![remove_color_space(b"zz")]),
        ("removeColorSpace", vec![remove_color_space(b"a")]),
        ("clearColorSpaces", vec![clear_color_spaces()]),
        ("setRole", vec![set_role(b"r", Some(b"x"))]),
        ("setRole None", vec![set_role(b"zz", None)]),
        ("setRole fails", vec![set_role(b"", Some(b"x"))]),
        (
            "setInactiveColorSpaces",
            vec![set_inactive_color_spaces(b"x")],
        ),
        (
            "setStrictParsingEnabled",
            vec![set_strict_parsing_enabled(false)],
        ),
        (
            "setDefaultLumaCoefs",
            vec![set_default_luma_coefs([0.1, 0.2, 0.7])],
        ),
        (
            "addDisplayView",
            vec![add_display_view(b"D", b"v", b"x", b"")],
        ),
        (
            "addDisplayView fails",
            vec![add_display_view(b"D", b"", b"x", b"")],
        ),
        (
            "addDisplayView replace",
            vec![add_display_view(b"D", b"V", b"y", b"")],
        ),
        ("clearSharedViews none", vec![clear_shared_views()]),
        ("addSharedView", vec![add_shared_view(b"s", b"", b"x")]),
        ("addSharedView fails", vec![add_shared_view(b"s", b"", b"")]),
        (
            "addDisplaySharedView",
            vec![add_display_shared_view(b"D", b"s")],
        ),
        (
            "addDisplaySharedView fails",
            vec![add_display_shared_view(b"D", b"S")],
        ),
        (
            "addDisplaySharedView new display",
            vec![add_display_shared_view(b"E", b"s")],
        ),
        ("removeSharedView fails", vec![remove_shared_view(b"zz")]),
        (
            "removeDisplayView fails",
            vec![remove_display_view(b"D", b"zz")],
        ),
        ("removeDisplayView", vec![remove_display_view(b"E", b"s")]),
        ("clearSharedViews", vec![clear_shared_views()]),
        ("addSharedView 2", vec![add_shared_view(b"s2", b"", b"x")]),
        ("removeSharedView", vec![remove_shared_view(b"S2")]),
        ("setActiveDisplays", vec![set_active_displays(b"D")]),
        (
            "setActiveDisplays fails",
            vec![set_active_displays(b"\"D, E")],
        ),
        ("addActiveDisplay", vec![add_active_display(b"D")]),
        ("addActiveDisplay dup", vec![add_active_display(b"D")]),
        ("addActiveDisplay fails", vec![add_active_display(b"")]),
        (
            "removeActiveDisplay fails",
            vec![remove_active_display(b"zz")],
        ),
        ("removeActiveDisplay", vec![remove_active_display(b"D")]),
        (
            "clearActiveDisplays",
            vec![clear_step("clearActiveDisplays", |c| {
                c.clear_active_displays()
            })],
        ),
        ("setActiveViews", vec![set_active_views(b"v")]),
        ("setActiveViews fails", vec![set_active_views(b"\"v, w")]),
        ("addActiveView", vec![add_active_view(b"v")]),
        ("addActiveView dup", vec![add_active_view(b"v")]),
        ("removeActiveView fails", vec![remove_active_view(b"zz")]),
        ("removeActiveView", vec![remove_active_view(b"v")]),
        (
            "clearActiveViews",
            vec![clear_step("clearActiveViews", |c| c.clear_active_views())],
        ),
        (
            "addVirtualDisplayView",
            vec![add_virtual_display_view(b"vv", b"", b"x")],
        ),
        (
            "addVirtualDisplayView dup",
            vec![add_virtual_display_view(b"VV", b"", b"x")],
        ),
        (
            "addVirtualDisplaySharedView",
            vec![add_virtual_display_shared_view(b"s")],
        ),
        (
            "addVirtualDisplaySharedView dup",
            vec![add_virtual_display_shared_view(b"S")],
        ),
        (
            "removeVirtualDisplayView missing",
            vec![remove_virtual_display_view(b"zz")],
        ),
        (
            "removeVirtualDisplayView own",
            vec![remove_virtual_display_view(b"VV")],
        ),
        (
            "removeVirtualDisplayView shared",
            vec![remove_virtual_display_view(b"s")],
        ),
        (
            "clearVirtualDisplay",
            vec![clear_step("clearVirtualDisplay", |c| {
                c.clear_virtual_display()
            })],
        ),
        (
            "setDisplayTemporary",
            vec![set_display_temporary(b"D", true)],
        ),
        (
            "setDisplayTemporary missing",
            vec![set_display_temporary(b"zz", true)],
        ),
        (
            "instantiateDisplayFromICCProfile fails",
            vec![instantiate_display_from_icc_profile(b"")],
        ),
        ("addLook", add_look(b"lk", b"x", true)),
        ("addLook fails", add_look(b"", b"x", true)),
        (
            "clearLooks",
            vec![clear_step("clearLooks", |c| c.clear_looks())],
        ),
        (
            "addViewTransform",
            add_view_transform(
                b"vt",
                ReferenceSpaceType::Scene,
                Some(ViewTransformDirection::ToReference),
            ),
        ),
        (
            "addViewTransform fails",
            add_view_transform(b"vt2", ReferenceSpaceType::Scene, None),
        ),
        (
            "setDefaultViewTransformName",
            vec![set_default_view_transform_name(b"vt")],
        ),
        (
            "clearViewTransforms",
            vec![clear_step("clearViewTransforms", |c| {
                c.clear_view_transforms()
            })],
        ),
        (
            "addNamedTransform",
            add_named_transform(b"nt", &[b"nta"], Some(TransformDirection::Forward)),
        ),
        (
            "addNamedTransform fails",
            add_named_transform(b"nt2", &[], None),
        ),
        (
            "clearNamedTransforms",
            vec![clear_step("clearNamedTransforms", |c| {
                c.clear_named_transforms()
            })],
        ),
        ("clearDisplays", vec![clear_displays()]),
        (
            "setFileRules",
            set_file_rules(&[Rule::Glob(b"g", b"x", b"*", b"exr")], None),
        ),
        (
            "setViewingRules",
            set_viewing_rules(&[(b"r", &[b"x"], &[])]),
        ),
        ("upgradeToLatestVersion", vec![upgrade_to_latest_version()]),
    ];
    let p1: HeldProcessor = Rc::new(RefCell::new(None));
    let p2: HeldProcessor = Rc::new(RefCell::new(None));
    let mut calls = Vec::new();
    // (label, kind): kind 0 = run port step (ignore outcome), 1 = get p1, 2 = get p2, 3 = compare
    enum K {
        S(Step),
        G(ConfigAction),
        Cmp(String),
    }
    let mut plan: Vec<K> = Vec::new();
    for (label, steps) in cases.drain(..) {
        let (c, f) = get_processor("p1", &p1);
        calls.push(c);
        plan.push(K::G(f));
        for s in steps {
            let mut call = s.call.clone();
            if call.get("call").is_some() && call.get("on").is_none() {
                call["on"] = json!("config");
            }
            calls.push(call);
            plan.push(K::S(s));
        }
        let (c, f) = get_processor("p2", &p2);
        calls.push(c);
        plan.push(K::G(f));
        calls.push(json!({"call": "__eq__", "on": "p1", "args": [{"ref": "p2"}]}));
        plan.push(K::Cmp(label.to_string()));
    }
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "new", "env": {}, "calls": calls}),
        &[],
    );
    let wheel = &response.result;
    set_thread_env_provider(Some(Arc::new(MapEnv::default())));
    let mut config = Config::new().unwrap();
    let results = wheel["calls"].as_array().unwrap();
    assert_eq!(results.len(), plan.len());
    let mut failures = Vec::new();
    for (i, (k, w)) in plan.iter().zip(results).enumerate() {
        match k {
            K::S(s) => {
                let port = (s.port)(&mut config);
                let mut w = w.clone();
                w.as_object_mut().unwrap().remove("log");
                if port != w {
                    failures.push(format!("call {i} {}: wheel {w}, port {port}", calls[i]));
                }
            }
            K::G(f) => f(&mut config),
            K::Cmp(label) => {
                let same =
                    Arc::ptr_eq(p1.borrow().as_ref().unwrap(), p2.borrow().as_ref().unwrap());
                let wheel_same = w["result"] == json!(true);
                if same != wheel_same {
                    failures.push(format!(
                        "{label}: wheel kept the processor: {wheel_same}, port: {same}"
                    ));
                }
            }
        }
    }
    set_thread_env_provider(None);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What a two-config case does: a call on one of them, a call on both (a static function of
/// the config class), or the copy of the first into the second.
enum PairItem {
    On(usize, Step),
    Pair(Value, PairCall),
    CopyTo1,
}

/// The port's side of a call on both configs.
type PairCall = Box<dyn Fn(&Config, &Config) -> Value>;

/// Runs `items` on `config` and `other` on both sides, in the environment `env`, with the
/// getters every third item and at the end, and compares every outcome.
fn run_pair(label: &str, env: &[(&str, Vec<u8>)], items: Vec<PairItem>, probes: &Probes) -> usize {
    let names = ["config", "other"];
    let mut calls = Vec::new();
    let mut plan: Vec<PairItem> = Vec::new();
    // `other` starts as a copy.
    plan.push(PairItem::CopyTo1);
    let cur = 0usize;
    let mut k = 0;
    for it in items {
        plan.push(it);
        k += 1;
        if k % 3 == 0 {
            for s in all_getters(probes) {
                plan.push(PairItem::On(cur, s));
            }
        }
    }
    for t in 0..2 {
        for s in all_getters(probes) {
            plan.push(PairItem::On(t, s));
        }
    }
    for p in &plan {
        match p {
            PairItem::On(i, s) => {
                let mut call = s.call.clone();
                if call.get("call").is_some() && call.get("on").is_none() {
                    call["on"] = json!(names[*i]);
                }
                calls.push(call);
            }
            PairItem::Pair(c, _) => calls.push(c.clone()),
            PairItem::CopyTo1 => calls.push(json!({"copy": "config", "as": "other"})),
        }
    }
    let env_json: serde_json::Map<String, Value> = env
        .iter()
        .map(|(k, v)| (k.to_string(), json!(std::str::from_utf8(v).unwrap())))
        .collect();
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "new", "env": env_json, "calls": calls}),
        &[],
    );
    let wheel = &response.result;
    set_thread_env_provider(Some(Arc::new(MapEnv::default())));
    for (name, value) in env {
        setenv(name, value).expect("a request's variable");
    }
    let made = Config::new();
    let mut cfgs = match made {
        Ok(c) => {
            assert_eq!(
                wheel["config"],
                Value::Null,
                "{label}: the wheel refused the config"
            );
            [c.clone(), c]
        }
        Err(e) => {
            set_thread_env_provider(None);
            assert_eq!(
                wheel["config"],
                unit_out(Err(e)),
                "{label}: refusal differs"
            );
            return 0;
        }
    };
    let results = wheel["calls"].as_array().expect("the calls' results");
    assert_eq!(results.len(), plan.len(), "{label}");
    let mut failures = Vec::new();
    let mut first_fail = None;
    for (i, (p, w)) in plan.iter().zip(results).enumerate() {
        let logged = log(&w["log"]);
        if !logged.is_empty() {
            failures.push(format!("{label}, call {i}: wheel logged {logged:?}"));
            first_fail.get_or_insert(i);
        }
        let mut w = w.clone();
        w.as_object_mut().expect("a call's outcome").remove("log");
        let port = match p {
            PairItem::On(t, s) => (s.port)(&mut cfgs[*t]),
            PairItem::Pair(_, f) => f(&cfgs[0], &cfgs[1]),
            PairItem::CopyTo1 => {
                cfgs[1] = cfgs[0].clone();
                continue;
            }
        };
        if port != w {
            failures.push(format!(
                "{label}, call {i} {}: wheel {w}, port {port}",
                calls[i]
            ));
            first_fail.get_or_insert(i);
        }
    }
    set_thread_env_provider(None);
    if let Some(first) = first_fail {
        let mut seq = Vec::new();
        for (i, c) in calls.iter().enumerate().take(first + 1) {
            let name = c.get("call").and_then(Value::as_str).unwrap_or("");
            if !name.starts_with("get") && !name.starts_with("is") && !name.starts_with("has") {
                seq.push(format!("  #{i} {c}"));
            }
        }
        panic!(
            "{} failures; env {:?}; first:\n{}\nsequence:\n{}",
            failures.len(),
            env.iter()
                .map(|(k, v)| format!("{k}={}", String::from_utf8_lossy(v)))
                .collect::<Vec<_>>(),
            failures[..failures.len().min(6)].join("\n"),
            seq.join("\n")
        );
    }
    plan.len()
}

// ---------------------------------------------------------------------------------------------
// Two configs side by side (`config` and `other`, a copy at the start): the verifier's inputs
// (2026-10-06) for what the cases above don't reach, each confirmed against the wheel.

fn pair_views(d: &[u8], v: &[u8]) -> PairItem {
    let (d2, v2) = (d.to_vec(), v.to_vec());
    PairItem::Pair(
        json!({"call": "AreViewsEqual", "on": "Config",
               "args": [{"ref": "config"}, {"ref": "other"}, arg(d), arg(v)]}),
        Box::new(move |a, b| json!({"result": Config::are_views_equal(a, b, &d2, &v2)})),
    )
}

fn pair_virtual(v: &[u8]) -> PairItem {
    let v2 = v.to_vec();
    PairItem::Pair(
        json!({"call": "AreVirtualViewsEqual", "on": "Config",
               "args": [{"ref": "config"}, {"ref": "other"}, arg(v)]}),
        Box::new(move |a, b| json!({"result": Config::are_virtual_views_equal(a, b, &v2)})),
    )
}

fn virtual_view_looks(view: &[u8], looks: &[u8], rule: &[u8]) -> Step {
    let (v, l, r) = (view.to_vec(), looks.to_vec(), rule.to_vec());
    step(
        json!({"call": "addVirtualDisplayView",
               "args": [arg(&v), arg(b""), arg(b"raw"), arg(&l), arg(&r), arg(b"d")]}),
        move |config| unit_out(config.add_virtual_display_view(&v, b"", b"raw", &l, &r, b"d")),
    )
}

/// The environment's active lists naming nothing that exists (the config's lists then don't
/// apply); a display made by `addDisplaySharedView` after the active displays were computed;
/// `AreViewsEqual` and `AreVirtualViewsEqual` with rules and looks that differ; NULs in a
/// view's looks and in the default view transform's name.
#[test]
fn two_configs_and_edges_match_the_wheel() {
    let probes =
        probes(&[b"raw"], &[]).displays(&[b"A", b"B", b"P3"], &[b"v1", b"v2", b"vv", b"s"]);
    let on = |s: Step| PairItem::On(0, s);
    let other = |s: Step| PairItem::On(1, s);

    // C22: OCIO_ACTIVE_VIEWS names no view of the display: all its views, not the config's list.
    run_pair(
        "C22",
        &[("OCIO_ACTIVE_VIEWS", b"nomatch".to_vec())],
        vec![
            on(add_display_view(b"A", b"v1", b"raw", b"")),
            on(add_display_view(b"A", b"v2", b"raw", b"")),
            on(set_active_views(b"v2")),
        ],
        &probes,
    );
    // D4: OCIO_ACTIVE_DISPLAYS names no display: all the displays, not the config's list.
    run_pair(
        "D4",
        &[("OCIO_ACTIVE_DISPLAYS", b"nomatch".to_vec())],
        vec![
            on(add_display_view(b"A", b"v1", b"raw", b"")),
            on(add_display_view(b"B", b"v1", b"raw", b"")),
            on(set_active_displays(b"B")),
        ],
        &probes,
    );
    // C28: a display made by addDisplaySharedView after the active displays were computed.
    run_pair(
        "C28",
        &[],
        vec![
            on(add_display_view(b"A", b"v1", b"raw", b"")),
            on(add_shared_view(b"s", b"", b"raw")),
            on(add_display_view(b"P3", b"v2", b"raw", b"")),
            on(add_display_shared_view(b"B", b"s")),
        ],
        &probes,
    );
    // C53: same view, rules differ. C54: same virtual view, looks differ. Rules differ too.
    run_pair(
        "C53/C54",
        &[],
        vec![
            on(add_display_view_full(b"A", b"v1", b"", b"raw", b"rule")),
            on(virtual_view_looks(b"vv", b"l1", b"r")),
            on(virtual_view_looks(b"vw", b"l", b"r1")),
            PairItem::CopyTo1,
            other(remove_display_view(b"A", b"v1")),
            other(add_display_view_full(b"A", b"v1", b"", b"raw", b"")),
            other(remove_virtual_display_view(b"vv")),
            other(virtual_view_looks(b"vv", b"l2", b"r")),
            other(remove_virtual_display_view(b"vw")),
            other(virtual_view_looks(b"vw", b"l", b"r2")),
            pair_views(b"A", b"v1"),
            pair_virtual(b"vv"),
            pair_virtual(b"vw"),
        ],
        &probes,
    );
    // D3 / C19: NULs in looks and in the default view transform's name.
    run_pair(
        "D3/C19",
        &[],
        vec![
            on(add_display_view(b"A", b"v1", b"raw", b"l\0x")),
            on(set_default_view_transform_name(b"vt\0x")),
        ],
        &probes,
    );
}

/// A change through the current context the config gave is the config's own: the Python
/// binding lets a caller change it (`getCurrentContext().setSearchPath(...)`), and the config
/// then reads the changed context, without emptying its cache of processors. The port does it
/// through `CurrentContext::update`, kept for the Python module.
#[test]
fn a_change_through_the_held_context_is_the_configs() {
    let held: HeldProcessor = Rc::new(RefCell::new(None));
    let again: HeldProcessor = Rc::new(RefCell::new(None));
    let (get1, port_get1) = get_processor("p1", &held);
    let (get2, port_get2) = get_processor("p2", &again);
    type Change = fn(&mut ocio::Context);
    let changes: [(Value, Change); 4] = [
        (
            json!({"call": "setSearchPath", "on": "ctx", "args": ["a:b"]}),
            |c| c.set_search_path("a:b"),
        ),
        (
            json!({"call": "setWorkingDir", "on": "ctx", "args": ["/w"]}),
            |c| c.set_working_dir("/w"),
        ),
        (
            json!({"call": "__setitem__", "on": "ctx", "args": ["V", "1"]}),
            |c| c.set_string_var("V", Some(b"1")),
        ),
        (
            json!({"call": "setEnvironmentMode", "on": "ctx",
                   "args": [{"enum": "ENV_ENVIRONMENT_LOAD_ALL"}]}),
            |c| c.set_environment_mode(EnvironmentMode::LoadAll),
        ),
    ];
    let mut calls = vec![
        json!({"call": "getCurrentContext", "on": "config", "as": "ctx"}),
        {
            let mut c = get1;
            c["on"] = json!("config");
            c
        },
    ];
    calls.extend(changes.iter().map(|(c, _)| c.clone()));
    calls.push({
        let mut c = get2;
        c["on"] = json!("config");
        c
    });
    calls.push(json!({"call": "__eq__", "on": "p1", "args": [{"ref": "p2"}]}));
    let getters = getters();
    for g in &getters {
        let mut c = g.call.clone();
        c["on"] = json!("config");
        calls.push(c);
    }
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "new", "env": {}, "calls": calls}),
        &[],
    );
    let wheel = &response.result;
    let results = wheel["calls"].as_array().expect("the calls' results");

    set_thread_env_provider(Some(Arc::new(MapEnv::default())));
    let mut config = Config::new().unwrap();
    let ctx = config.current_context();
    port_get1(&mut config);
    for (_, change) in &changes {
        ctx.update(change);
    }
    port_get2(&mut config);
    let kept = Arc::ptr_eq(
        held.borrow().as_ref().unwrap(),
        again.borrow().as_ref().unwrap(),
    );
    let at = 2 + changes.len() + 1;
    assert_eq!(
        results[at]["result"] == json!(true),
        kept,
        "the cache of processors: wheel {}",
        results[at]
    );
    let mut failures = Vec::new();
    for (g, w) in getters.iter().zip(&results[at + 1..]) {
        let mut w = w.clone();
        w.as_object_mut().expect("a call's outcome").remove("log");
        let port = (g.port)(&mut config);
        if port != w {
            failures.push(format!("{}: wheel {w}, port {port}", g.call));
        }
    }
    set_thread_env_provider(None);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// File rules on a config: the color space and rule of each path (globs, expressions, the path
/// search rule, which takes the color space it finds, the default rule), the paths only the
/// default rule matches, strict and lenient parsing of color spaces from paths; the color
/// spaces a config uses; a version 1 config upgraded (its file rules get the path search rule
/// and a default color space).
#[test]
fn file_rules_on_a_config_match_the_wheel() {
    use Rule::{Glob, PathSearch, Regex};
    let to = ColorSpaceDirection::ToReference;
    let cst = |s: &[u8], d: &[u8]| {
        let mut t = ocio::ColorSpaceTransform::new();
        t.set_src(s);
        t.set_dst(d);
        (
            json!({"class": "ColorSpaceTransform", "calls": [["setSrc", String::from_utf8_lossy(s)],
                ["setDst", String::from_utf8_lossy(d)]]}),
            Transform::from(t),
        )
    };
    let with = |c: Cs, (spec, t): (Value, Transform), dir| c.transform(spec, t, dir);
    let items: Vec<Item> = vec![
        add_color_space(cs(b"raw").data()).into(),
        add_color_space(cs(b"lin_srgb").alias(b"linear")).into(),
        add_color_space(cs(b"srgb")).into(),
        add_color_space(cs(b"acescg").category(b"c")).into(),
        add_color_space(with(cs(b"used_by_cst"), cst(b"$SRC", b"lin_srgb"), to)).into(),
        set_role(b"default", Some(b"raw")).into(),
        add_environment_var(b"SRC", Some(b"acescg")).into(),
        set_file_rules(
            &[
                Glob(b"exr", b"lin_srgb", b"*", b"exr"),
                Glob(b"jpg", b"srgb", b"*render*", b"jp[e]g"),
                Regex(b"tiff", b"acescg", b".*\\.tiff?$"),
                PathSearch,
                Glob(b"nl", b"srgb", b"\n.*", b"png"),
            ],
            None,
        )
        .into(),
        set_strict_parsing_enabled(false).into(),
        Item::Copy,
        set_file_rules(
            &[PathSearch, Glob(b"any", b"raw", b"*.*", b"*")],
            Some(b"srgb"),
        )
        .into(),
        set_strict_parsing_enabled(true).into(),
        set_role(b"default", None).into(),
    ];
    let probes = probes(
        &[
            b"raw",
            b"lin_srgb",
            b"srgb",
            b"acescg",
            b"default",
            b"unknown",
        ],
        &[],
    )
    .paths(&[
        b"/a/b/file.exr",
        b"/a/b/FILE.EXR",
        b"/a/render/x.jpeg",
        b"/a/render/x.jpg",
        b"img.tif",
        b"img.TIFF",
        b"/shots/acescg/linear_x.dpx",
        b"/shots/x_lin_srgb_srgb.dpx",
        b"nothing.dpx",
        b"\nfoo.png",
        b"
foo.png",
        b"
.foo.png",
        b"",
    ]);
    check_items("file rules", "new", &[], items, &probes);
}

/// A version 1 config upgraded to the latest version: its file rules get the path search rule
/// and a default color space (the default role's, `raw` when data, the first data one, the
/// first active one, or the first one, with a warning); without a color space it is refused.
#[test]
fn upgrades_match_the_wheel() {
    let cases: Vec<(&str, Vec<Item>)> = vec![
        ("v2", vec![upgrade_to_latest_version().into()]),
        (
            "default role",
            vec![
                add_color_space(cs(b"a")).into(),
                set_role(b"default", Some(b"a")).into(),
                set_major_version(1).into(),
                upgrade_to_latest_version().into(),
                upgrade_to_latest_version().into(),
            ],
        ),
        (
            "raw",
            vec![
                add_color_space(cs(b"a")).into(),
                add_color_space(cs(b"RAW").data()).into(),
                set_major_version(1).into(),
                upgrade_to_latest_version().into(),
            ],
        ),
        (
            "first data",
            vec![
                add_color_space(cs(b"a")).into(),
                add_color_space(cs(b"raw")).into(),
                add_color_space(cs(b"d").display().data()).into(),
                add_color_space(cs(b"e").data()).into(),
                set_major_version(1).into(),
                upgrade_to_latest_version().into(),
            ],
        ),
        (
            "first active",
            vec![
                add_color_space(cs(b"d").display()).into(),
                add_color_space(cs(b"b")).into(),
                set_inactive_color_spaces(b"d").into(),
                set_major_version(1).into(),
                set_file_rules(&[Rule::PathSearch], None).into(),
                upgrade_to_latest_version().into(),
            ],
        ),
        (
            "first",
            vec![
                add_color_space(cs(b"d").display()).into(),
                add_color_space(cs(b"b")).into(),
                set_inactive_color_spaces(b"d, b").into(),
                set_major_version(1).into(),
                upgrade_to_latest_version().into(),
            ],
        ),
    ];
    for (label, items) in cases {
        let probes = probes(&[b"a", b"b", b"d"], &[]).paths(&[b"x_a.exr", b"b.exr"]);
        check_items(label, "new", &[], items, &probes);
    }
}

/// Views filtered by the viewing rules for an image's color space: rules that name color
/// spaces, roles (resolved to their color spaces), aliases, unknown names, or encodings (in
/// any case, against the color space's encoding as it is written); views with a rule that
/// doesn't exist; shared views with rules; inactive views. The image's color space is given by
/// its name, an alias, a role or in another case.
#[test]
fn views_by_viewing_rules_match_the_wheel() {
    let items: Vec<Item> = vec![
        add_color_space(cs(b"raw").data()).into(),
        add_color_space(cs(b"lin").alias(b"linear").encoding(b"scene-linear")).into(),
        add_color_space(cs(b"logc").encoding(b"log")).into(),
        add_color_space(cs(b"Log2").encoding(b"Log")).into(),
        add_color_space(cs(b"vid").encoding(b"sdr-video")).into(),
        set_role(b"scene_linear", Some(b"lin")).into(),
        set_role(b"compositing_log", Some(b"logc")).into(),
        add_display_view_full(b"D", b"plain", b"", b"raw", b"").into(),
        add_display_view_full(b"D", b"by_cs", b"", b"raw", b"rule_cs").into(),
        add_display_view_full(b"D", b"by_role", b"", b"raw", b"RULE_ROLE").into(),
        add_display_view_full(b"D", b"by_alias", b"", b"raw", b"rule_alias").into(),
        add_display_view_full(b"D", b"by_enc", b"", b"raw", b"rule_enc").into(),
        add_display_view_full(b"D", b"by_upper_enc", b"", b"raw", b"rule_upper_enc").into(),
        add_display_view_full(b"D", b"missing_rule", b"", b"raw", b"no_such_rule").into(),
        add_shared_view(b"shared", b"", b"raw").into(),
        add_display_shared_view(b"D", b"shared").into(),
        set_viewing_rules(&[
            (b"rule_cs", &[b"LIN", b"vid"], &[]),
            (b"rule_role", &[b"compositing_log", b"unknown"], &[]),
            (b"rule_alias", &[b"linear"], &[]),
            (b"rule_enc", &[], &[b"LOG", b"scene-linear"]),
            (b"rule_upper_enc", &[], &[b"Log"]),
            (b"Rule", &[], &[b"sdr-video"]),
        ])
        .into(),
        Item::Copy,
        set_active_views(b"by_role, by_enc, plain").into(),
        set_viewing_rules(&[]).into(),
    ];
    let probes = probes(
        &[
            b"raw",
            b"lin",
            b"LIN",
            b"linear",
            b"scene_linear",
            b"logc",
            b"compositing_log",
            b"Log2",
            b"vid",
            b"unknown",
        ],
        &[],
    )
    .displays(&[b"D"], &[b"by_cs", b"by_enc"]);
    check_items("viewing rules", "new", &[], items, &probes);
}

// ---------------------------------------------------------------------------------------------
// The rules the config shares: a change through the file rules or viewing rules a caller took
// from the config is the config's own (the binding casts the constness away), and keeps its
// cache of processors; `setFileRules` and `setViewingRules` give the config new rules, which
// the earlier rules no longer change; a copy of the config has its own; the upgrade of a
// version 1 config changes the shared file rules in place.

/// The port's side of a held-rules case: the configs and the rules taken from them.
#[derive(Default)]
struct HeldRules {
    configs: BTreeMap<&'static str, Config>,
    file_rules: BTreeMap<&'static str, ocio::ConfigRules<FileRules>>,
    viewing_rules: BTreeMap<&'static str, ocio::ConfigRules<ViewingRules>>,
}

type HeldCall = Box<dyn Fn(&mut HeldRules) -> Value>;

/// A step of [`check_held_rules`]: the wheel's call and the port's (`Value::Null` for a call
/// whose outcome isn't compared).
struct HeldStep {
    call: Value,
    port: HeldCall,
}

fn held(call: Value, port: impl Fn(&mut HeldRules) -> Value + 'static) -> HeldStep {
    HeldStep {
        call,
        port: Box::new(port),
    }
}

/// A step on the config named `on`.
fn on_config(on: &'static str, s: Step) -> HeldStep {
    let mut call = s.call;
    if call.get("call").is_some() && call.get("on").is_none() {
        call["on"] = json!(on);
    }
    let port = s.port;
    held(call, move |h| {
        port(h.configs.get_mut(on).expect("a config"))
    })
}

fn take_file_rules(config: &'static str, store: &'static str) -> HeldStep {
    held(
        json!({"call": "getFileRules", "on": config, "as": store}),
        move |h| {
            let fr = h.configs[config].file_rules();
            let out = object_out("FileRules", &fr.get().to_bytes());
            h.file_rules.insert(store, fr);
            json!({ "result": out })
        },
    )
}

fn take_viewing_rules(config: &'static str, store: &'static str) -> HeldStep {
    held(
        json!({"call": "getViewingRules", "on": config, "as": store}),
        move |h| {
            let vr = h.configs[config].viewing_rules();
            let out = object_out("ViewingRules", &vr.get().to_bytes());
            h.viewing_rules.insert(store, vr);
            json!({ "result": out })
        },
    )
}

/// `insertRule` (a glob of any name with the extension `ext`) on held file rules.
fn held_insert_file_rule(
    on: &'static str,
    i: usize,
    name: &[u8],
    cs: &[u8],
    ext: &[u8],
) -> HeldStep {
    let (n, c, e) = (name.to_vec(), cs.to_vec(), ext.to_vec());
    held(
        json!({"call": "insertRule", "on": on,
               "args": [i, arg(&n), arg(&c), arg(b"*"), arg(&e)]}),
        move |h| unit_out(h.file_rules[on].update(|r| r.insert_rule(i, &n, &c, b"*", &e))),
    )
}

fn held_insert_path_search_rule(on: &'static str, i: usize) -> HeldStep {
    held(
        json!({"call": "insertPathSearchRule", "on": on, "args": [i]}),
        move |h| unit_out(h.file_rules[on].update(|r| r.insert_path_search_rule(i))),
    )
}

fn held_file_rules_repr(on: &'static str) -> HeldStep {
    held(json!({"call": "__repr__", "on": on}), move |h| {
        text_out(&h.file_rules[on].get().to_bytes())
    })
}

fn held_file_rule_color_space(on: &'static str, i: usize) -> HeldStep {
    held(
        json!({"call": "getColorSpace", "on": on, "args": [i]}),
        move |h| match h.file_rules[on].get().color_space(i) {
            Ok(cs) => text_out(&cs),
            Err(e) => unit_out(Err(e)),
        },
    )
}

fn held_insert_viewing_rule(on: &'static str, i: usize, name: &[u8]) -> HeldStep {
    let n = name.to_vec();
    held(
        json!({"call": "insertRule", "on": on, "args": [i, arg(&n)]}),
        move |h| unit_out(h.viewing_rules[on].update(|r| r.insert_rule(i, &n))),
    )
}

fn held_add_viewing_color_space(on: &'static str, i: usize, cs: &[u8]) -> HeldStep {
    let c = cs.to_vec();
    held(
        json!({"call": "addColorSpace", "on": on, "args": [i, arg(&c)]}),
        move |h| unit_out(h.viewing_rules[on].update(|r| r.add_color_space(i, &c))),
    )
}

fn held_viewing_rules_repr(on: &'static str) -> HeldStep {
    held(json!({"call": "__repr__", "on": on}), move |h| {
        text_out(&h.viewing_rules[on].get().to_bytes())
    })
}

/// What a config says of the rules: its file rules and viewing rules, the color space of each
/// path, and the views of display `D` for each color space.
fn held_rules_getters(on: &'static str) -> Vec<HeldStep> {
    let mut out = vec![
        held(json!({"call": "getFileRules", "on": on}), move |h| {
            let fr = h.configs[on].file_rules().get().to_bytes();
            json!({ "result": object_out("FileRules", &fr) })
        }),
        held(json!({"call": "getViewingRules", "on": on}), move |h| {
            let vr = h.configs[on].viewing_rules().get().to_bytes();
            json!({ "result": object_out("ViewingRules", &vr) })
        }),
    ];
    for path in [&b"a.exr"[..], b"a.tif", b"a.png", b"x_lin.dpx", b"x.dpx"] {
        let p = path.to_vec();
        out.push(held(
            json!({"call": "getColorSpaceFromFilepath", "on": on, "args": [arg(path)]}),
            move |h| match h.configs[on].color_space_from_filepath_with_index(&p) {
                Ok((cs, i)) => json!({"result": [bytes_arg(&cs), i]}),
                Err(e) => error_out(e),
            },
        ));
    }
    for cs in [&b"raw"[..], b"lin"] {
        let n = cs.to_vec();
        out.push(held(
            json!({"call": "getViews", "on": on, "args": [arg(b"D"), arg(cs)]}),
            move |h| {
                let c = &h.configs[on];
                texts_or((|| {
                    let num = c.num_views_for_color_space(b"D", &n)?;
                    (0..num)
                        .map(|i| c.view_for_color_space(b"D", &n, i).map(<[u8]>::to_vec))
                        .collect()
                })())
            },
        ));
    }
    out
}

/// The processors `getProcessor` gave, by name.
type ProcessorSlots = Rc<RefCell<BTreeMap<&'static str, Arc<ocio::Processor>>>>;

/// `getProcessor` on `config`, stored as `store` on both sides, then whether it is the
/// processor stored as `before` (`__eq__` in the binding, `Arc::ptr_eq` here, as
/// `{"same": bool}`).
fn held_same_processor(
    config: &'static str,
    store: &'static str,
    before: &'static str,
    slots: &ProcessorSlots,
) -> Vec<HeldStep> {
    let (spec, t) = offset_transform(0.5);
    let s = slots.clone();
    let get = held(
        json!({"call": "getProcessor", "on": config, "args": [{"transform": spec}], "as": store}),
        move |h| {
            let config = h.configs.get_mut(config).expect("a config");
            let p = config.processor(&t).expect("a processor");
            s.borrow_mut().insert(store, p);
            Value::Null
        },
    );
    let s = slots.clone();
    let same = held(
        json!({"call": "__eq__", "on": before, "args": [{"ref": store}]}),
        move |_| {
            let s = s.borrow();
            json!({ "same": Arc::ptr_eq(&s[before], &s[store]) })
        },
    );
    vec![get, same]
}

/// Runs `steps` on both sides, on a new config named `config` (and the configs and rules the
/// steps make), and compares every outcome but those the port gives as `Value::Null`.
fn check_held_rules(steps: Vec<HeldStep>) {
    let calls: Vec<Value> = steps.iter().map(|s| s.call.clone()).collect();
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "new", "env": {}, "calls": calls}),
        &[],
    );
    let wheel = &response.result;
    assert_eq!(wheel["config"], Value::Null, "{wheel}");
    let results = wheel["calls"].as_array().expect("the calls' results");
    assert_eq!(results.len(), steps.len());

    set_thread_env_provider(Some(Arc::new(MapEnv::default())));
    let mut h = HeldRules::default();
    h.configs.insert("config", Config::new().unwrap());
    let mut failures = Vec::new();
    for (i, (s, w)) in steps.iter().zip(results).enumerate() {
        let port = (s.port)(&mut h);
        if port.is_null() {
            assert!(w.get("result").is_some(), "call {i} {}: {w}", s.call);
            continue;
        }
        if let Some(same) = port.get("same") {
            // The binding's processors compare by identity: `__eq__` of another processor is
            // `NotImplemented`.
            let wheel_same = w["result"] == json!(true);
            if *same != json!(wheel_same) {
                failures.push(format!("call {i} {}: wheel {w}, port {port}", s.call));
            }
            continue;
        }
        let mut w = w.clone();
        w.as_object_mut().expect("a call's outcome").remove("log");
        if port != w {
            failures.push(format!("call {i} {}: wheel {w}, port {port}", s.call));
        }
    }
    set_thread_env_provider(None);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A change through the file rules and viewing rules a caller holds is the config's (its
/// matching and its views use it, and the path search rule's color space it sets shows in
/// the held rules), and keeps its processors; after `setFileRules` and `setViewingRules` the
/// held rules no longer change the config; a copy's rules are its own; the upgrade of a
/// version 1 config changes the held file rules.
#[test]
fn a_change_through_the_held_rules_is_the_configs() {
    let slots: ProcessorSlots = Rc::new(RefCell::new(BTreeMap::new()));
    let mut steps: Vec<HeldStep> = Vec::new();
    let c = |s: Step| on_config("config", s);
    steps.extend(add_color_space(cs(b"raw").data()).into_iter().map(c));
    steps.extend(add_color_space(cs(b"lin")).into_iter().map(c));
    steps.push(c(set_role(b"default", Some(b"raw"))));
    steps.push(c(add_display_view_full(b"D", b"v0", b"", b"raw", b"")));
    steps.push(c(add_display_view_full(b"D", b"v1", b"", b"raw", b"vr1")));
    steps.push(take_file_rules("config", "held_fr"));
    steps.push(take_viewing_rules("config", "held_vr"));
    steps.extend(held_same_processor("config", "p1", "p1", &slots));
    steps.extend(held_rules_getters("config"));

    // Changes through the held rules.
    steps.push(held_insert_file_rule("held_fr", 0, b"exr", b"lin", b"exr"));
    steps.push(held_insert_path_search_rule("held_fr", 1));
    steps.push(held_insert_viewing_rule("held_vr", 0, b"vr1"));
    steps.push(held_add_viewing_color_space("held_vr", 0, b"lin"));
    steps.extend(held_same_processor("config", "p2", "p1", &slots));
    steps.extend(held_rules_getters("config"));
    steps.push(held_file_rule_color_space("held_fr", 1));
    steps.push(held_file_rules_repr("held_fr"));
    steps.push(held_viewing_rules_repr("held_vr"));

    // New rules: the held ones no longer change the config.
    let tif = [Rule::Glob(b"tif", b"raw", b"*", b"tif")];
    steps.extend(set_file_rules(&tif, None).into_iter().map(c));
    steps.extend(set_viewing_rules(&[]).into_iter().map(c));
    steps.push(held_insert_file_rule("held_fr", 0, b"png", b"lin", b"png"));
    steps.push(held_add_viewing_color_space("held_vr", 0, b"raw"));
    steps.extend(held_same_processor("config", "p3", "p2", &slots));
    steps.extend(held_rules_getters("config"));
    steps.push(held_file_rules_repr("held_fr"));

    // A copy has its own rules.
    steps.push(take_file_rules("config", "held_fr3"));
    steps.push(held(json!({"copy": "config", "as": "copy"}), |h| {
        let copy = h.configs["config"].clone();
        h.configs.insert("copy", copy);
        Value::Null
    }));
    steps.push(take_file_rules("copy", "held_fr4"));
    steps.push(held_insert_file_rule("held_fr4", 0, b"png", b"lin", b"png"));
    steps.extend(held_rules_getters("config"));
    steps.extend(held_rules_getters("copy"));

    // The upgrade of a version 1 config changes the held file rules.
    steps.push(c(set_major_version(1)));
    steps.push(c(upgrade_to_latest_version()));
    steps.push(held_file_rules_repr("held_fr3"));
    steps.extend(held_rules_getters("config"));

    check_held_rules(steps);
}
