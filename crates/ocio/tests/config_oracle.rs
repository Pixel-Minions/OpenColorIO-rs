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
use std::rc::Rc;
use std::sync::Arc;

use ocio::{
    ColorSpace, ColorSpaceDirection, ColorSpaceVisibility, Config, CurrentContext, LogTransform,
    MatrixTransform, ReferenceSpaceType, SearchReferenceSpaceType, Transform, ViewType,
};
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
}

fn probes(names: &[&[u8]], categories: &[&[u8]]) -> Probes {
    Probes {
        names: names.iter().map(|n| n.to_vec()).collect(),
        categories: categories.iter().map(|n| n.to_vec()).collect(),
        ..Probes::default()
    }
}

impl Probes {
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
                json!([
                    bytes_arg(c.role_name(i)),
                    bytes_arg(c.role_color_space_by_index(i))
                ])
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
            text_out(c.search_path())
        }),
        step(json!({"call": "getSearchPaths"}), |c| {
            let paths: Vec<Vec<u8>> = (0..c.num_search_paths())
                .map(|i| c.search_path_with_index(i).to_vec())
                .collect();
            texts_out(&paths)
        }),
        step(json!({"call": "getWorkingDir"}), |c| {
            text_out(c.working_dir())
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
    out
}

/// The name the copy of the config is stored as.
const COPY: &str = "copy";

// ---------------------------------------------------------------------------------------------
// The check

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
    assert!(log(&wheel["config_log"]).is_empty(), "{label}: {wheel}");

    set_thread_env_provider(Some(Arc::new(MapEnv::default())));
    for (name, value) in env {
        setenv(name, value).expect("a request's variable");
    }
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
    for (i, (s, w)) in sequence.iter().zip(results).enumerate() {
        assert!(log(&w["log"]).is_empty(), "{label}, call {i}: {w}");
        let mut w = w.clone();
        w.as_object_mut().expect("a call's outcome").remove("log");
        match s {
            Some(s) => {
                let port = (s.port)(&mut config);
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
