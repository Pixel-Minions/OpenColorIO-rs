// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Configs read from YAML (`Config::CreateFromStream`) against the wheel, through the
//! oracle's `config_calls`: the error, byte for byte, or the config's state through its
//! getters (the version, name, description, family separator, search path, environment and its
//! mode, strict parsing, luma, roles, color spaces, displays and their views with each view's
//! getters, shared and virtual views, active and inactive lists, looks, view transforms,
//! named transforms, the file and viewing rules by their text, the current context's
//! variables) and its text (`Config::serialize`, byte for byte, with what serializing logged);
//! and the warnings logged loading it.
//!
//! The configs: hand-written ones for each key of the config's loader and its errors, the
//! wheel's built-in configs (their YAML, `builtin_config_source`), and upstream's test configs
//! (`tests/data/files/configs`).
//!
//! The reader checks the config against its version after loading (`checkVersionConsistency`):
//! [`configs_their_versions_cant_have_are_refused`] checks each of its arms.

use std::sync::{Arc, Mutex, PoisonError};

use ocio::{
    ColorSpaceVisibility, Config, EnvironmentMode, NamedTransformVisibility,
    SearchReferenceSpaceType, ViewType,
};
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, hex, log};
use serde_json::{Value, json};

/// Serializes the tests that replace the logging function.
static LOGGING: Mutex<()> = Mutex::new(());

/// A config, or the message of the error that refused it.
type Loaded = Result<Arc<Config>, Vec<u8>>;

/// `f`'s result and what OCIO logged meanwhile, while OCIO reads the environment `env` and
/// nothing else.
fn captured<T>(env: &[(&str, &str)], f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(PoisonError::into_inner);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        sink.lock().unwrap().push(m.to_vec());
    })))
    .unwrap();
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries(env))));
    let result = f();
    set_thread_env_provider(None);
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (result, log)
}

/// The port's config of `text`, or its error's message, and what OCIO logged meanwhile. OCIO
/// reads the environment `env` and nothing else meanwhile.
fn port_load(text: &[u8], env: &[(&str, &str)]) -> (Loaded, Vec<Vec<u8>>) {
    captured(env, || {
        Config::create_from_stream(text).map_err(|e| e.what().to_vec())
    })
}

fn s(v: &[u8]) -> Value {
    json!({"bytes": hex(v)})
}

fn list(items: impl Iterator<Item = Vec<u8>>) -> Value {
    Value::Array(items.map(|v| s(&v)).collect())
}

fn view_type(t: ViewType) -> &'static str {
    match t {
        ViewType::Shared => "VIEW_SHARED",
        ViewType::DisplayDefined => "VIEW_DISPLAY_DEFINED",
    }
}

/// A getter call on the config: its name, its arguments, and the port's value as the oracle
/// writes it (an object by its text alone).
type Getter = (String, Vec<Value>, Value);

fn g(name: &str, args: Vec<Value>, port: Value) -> Getter {
    (name.to_string(), args, port)
}

/// The getters of the config, and the port's values.
fn getters(c: &Config) -> Vec<Getter> {
    let mut out = vec![
        g("getMajorVersion", vec![], json!(c.major_version())),
        g("getMinorVersion", vec![], json!(c.minor_version())),
        g("getName", vec![], s(c.name())),
        g("getDescription", vec![], s(c.description())),
        g("getFamilySeparator", vec![], s(&[c.family_separator()])),
        g("getSearchPath", vec![], s(&c.search_path())),
        g("getWorkingDir", vec![], s(&c.working_dir())),
        g(
            "getEnvironmentMode",
            vec![],
            json!({"enum": match c.environment_mode() {
                EnvironmentMode::Unknown => "ENV_ENVIRONMENT_UNKNOWN",
                EnvironmentMode::LoadPredefined => "ENV_ENVIRONMENT_LOAD_PREDEFINED",
                EnvironmentMode::LoadAll => "ENV_ENVIRONMENT_LOAD_ALL",
            }}),
        ),
        g(
            "getEnvironmentVarNames",
            vec![],
            list(
                (0..c.num_environment_vars()).map(|i| c.environment_var_name_by_index(i).to_vec()),
            ),
        ),
        g(
            "isStrictParsingEnabled",
            vec![],
            json!(c.is_strict_parsing_enabled()),
        ),
        g(
            "getUseDisplayAliases",
            vec![],
            json!(c.use_display_aliases()),
        ),
        g(
            "getDefaultLumaCoefs",
            vec![],
            Value::Array(
                c.default_luma_coefs()
                    .iter()
                    .map(|v| json!({"f64": v.to_bits()}))
                    .collect(),
            ),
        ),
        g(
            "getRoleNames",
            vec![],
            list((0..c.num_roles()).map(|i| c.role_name(i).to_vec())),
        ),
        g(
            "getColorSpaceNames",
            vec![
                json!({"enum": "SEARCH_REFERENCE_SPACE_ALL"}),
                json!({"enum": "COLORSPACE_ALL"}),
            ],
            list(
                (0..c.num_color_spaces_with(
                    SearchReferenceSpaceType::All,
                    ColorSpaceVisibility::All,
                ))
                    .map(|i| {
                        c.color_space_name_by_index_with(
                            SearchReferenceSpaceType::All,
                            ColorSpaceVisibility::All,
                            i,
                        )
                        .to_vec()
                    }),
            ),
        ),
        g(
            "getActiveDisplays",
            vec![],
            list(
                (0..c.num_active_displays())
                    .map(|i| c.active_display(i).unwrap_or_default().to_vec()),
            ),
        ),
        g(
            "getActiveViews",
            vec![],
            list((0..c.num_active_views()).map(|i| c.active_view(i).unwrap_or_default().to_vec())),
        ),
        g(
            "getInactiveColorSpaces",
            vec![],
            s(c.inactive_color_spaces()),
        ),
        g(
            "getLookNames",
            vec![],
            list((0..c.num_looks()).map(|i| c.look_name_by_index(i).to_vec())),
        ),
        g(
            "getViewTransformNames",
            vec![],
            list((0..c.num_view_transforms()).map(|i| c.view_transform_name_by_index(i).to_vec())),
        ),
        g(
            "getDefaultViewTransformName",
            vec![],
            s(c.default_view_transform_name()),
        ),
        g(
            "getNamedTransformNames",
            vec![json!({"enum": "NAMEDTRANSFORM_ALL"})],
            list(
                (0..c.num_named_transforms_with(NamedTransformVisibility::All)).map(|i| {
                    c.named_transform_name_by_index_with(NamedTransformVisibility::All, i)
                        .to_vec()
                }),
            ),
        ),
        g("getFileRules", vec![], s(&c.file_rules().get().to_bytes())),
        g(
            "getViewingRules",
            vec![],
            s(&c.viewing_rules().get().to_bytes()),
        ),
        g(
            "getDisplaysAll",
            vec![],
            list((0..c.num_displays_all()).map(|i| c.display_all(i).to_vec())),
        ),
    ];
    // The config's text (WP 3.7), or the message of the error that stopped it, and what
    // serializing logged.
    let (text, serialize_log) = captured(&[], || c.serialize());
    out.push(g(
        "serialize",
        vec![],
        json!({
            "result": match text {
                Ok(text) => s(&text),
                Err(e) => json!({"exception": s(e.what())}),
            },
            "log": serialize_log.iter().map(|m| s(m)).collect::<Vec<_>>(),
        }),
    ));
    // Its cache ID with its current context (which serializes it again: what that logs is
    // kept from the other tests' logs).
    let (cache_id, _) = captured(&[], || c.cache_id());
    out.push(g(
        "getCacheID",
        vec![],
        match cache_id {
            Ok(id) => s(id.as_bytes()),
            Err(e) => json!({"exception": s(e.what())}),
        },
    ));
    // Its validation (WP 3.8), and what it logged; then again: the second call gives the result
    // the config kept, and logs nothing (I-147).
    for _ in 0..2 {
        let (valid, validate_log) = captured(&[], || c.validate());
        out.push(g(
            "validate",
            vec![],
            json!({
                "result": match valid {
                    Ok(()) => Value::Null,
                    Err(e) => json!({"exception": s(e.what())}),
                },
                "log": validate_log.iter().map(|m| s(m)).collect::<Vec<_>>(),
            }),
        ));
    }
    for i in 0..c.num_roles() {
        let role = c.role_name(i);
        out.push(g(
            "getRoleColorSpace",
            vec![s(role)],
            s(c.role_color_space(role)),
        ));
    }
    out.push(g(
        "getColorSpaceNames",
        vec![
            json!({"enum": "SEARCH_REFERENCE_SPACE_ALL"}),
            json!({"enum": "COLORSPACE_ACTIVE"}),
        ],
        list(
            (0..c.num_color_spaces_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::Active,
            ))
                .map(|i| {
                    c.color_space_name_by_index_with(
                        SearchReferenceSpaceType::All,
                        ColorSpaceVisibility::Active,
                        i,
                    )
                    .to_vec()
                }),
        ),
    ));
    for i in 0..c.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All) {
        let name = c
            .color_space_name_by_index_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All,
                i,
            )
            .to_vec();
        out.push(g(
            "isInactiveColorSpace",
            vec![s(&name)],
            json!(c.is_inactive_color_space(&name)),
        ));
        // 2.6: whether it is used, by its name and by each alias (either side resolves to the
        // canonical name), and the display each names (with use_display_aliases, through the
        // display color spaces) and its description.
        let mut names = vec![name.clone()];
        if let Some(cs) = c.color_space(&name) {
            names.extend((0..cs.num_aliases()).map(|a| cs.alias(a).to_vec()));
        }
        for n in names {
            // 2.6 Color Interop IDs: the local ID of the name or alias, and the color space of
            // that ID, of the name and of the name under a namespace and under `local:`.
            let mut ids = vec![n.clone(), [b"ns:".as_slice(), &n].concat()];
            ids.push([b"local:".as_slice(), &n].concat());
            out.push(g(
                "generateLocalIDForColorSpace",
                vec![s(&n)],
                match c.generate_local_id_for_color_space(&n) {
                    Ok(id) => {
                        ids.push(id.clone());
                        s(&id)
                    }
                    Err(e) => json!({"exception": s(e.what())}),
                },
            ));
            for id in ids {
                out.push(g(
                    "findColorSpaceForID",
                    vec![s(&id)],
                    match c.find_color_space_for_id(&id) {
                        Some(cs) => s(&cs.to_bytes()),
                        None => Value::Null,
                    },
                ));
            }
            out.push(g(
                "isColorSpaceUsed",
                vec![s(&n)],
                json!(c.is_color_space_used(&n)),
            ));
            out.push(g(
                "getCanonicalDisplayName",
                vec![s(&n)],
                s(c.canonical_display_name(&n)),
            ));
            out.push(g(
                "getDisplayDescription",
                vec![s(&n)],
                s(c.display_description(&n)),
            ));
        }
    }
    for i in 0..c.num_roles() {
        let role = c.role_name(i).to_vec();
        out.push(g(
            "isColorSpaceUsed",
            vec![s(&role)],
            json!(c.is_color_space_used(&role)),
        ));
        out.push(g(
            "getCanonicalDisplayName",
            vec![s(&role)],
            s(c.canonical_display_name(&role)),
        ));
    }
    // The current context, then strings it resolves (the oracle stores it as `ctx`).
    out.push(g("getCurrentContext", vec![], Value::Null));
    let ctx = c.current_context().get();
    for v in ["$A", "$B", "$Z", "${A}/${Z}", "%A%"] {
        out.push(g(
            "ctx.resolveStringVar",
            vec![json!(v)],
            s(&ctx.resolve_string_var(v)),
        ));
    }
    let view_getters = |out: &mut Vec<Getter>, display: &[u8], view: &[u8]| {
        let args = vec![s(display), s(view)];
        out.push(g("hasView", args.clone(), json!(c.has_view(display, view))));
        out.push(g(
            "isViewShared",
            args.clone(),
            json!(c.is_view_shared(display, view)),
        ));
        out.push(g(
            "getDisplayViewTransformName",
            args.clone(),
            s(c.display_view_transform_name(display, view)),
        ));
        out.push(g(
            "getDisplayViewColorSpaceName",
            args.clone(),
            s(c.display_view_color_space_name(display, view)),
        ));
        out.push(g(
            "getDisplayViewLooks",
            args.clone(),
            s(c.display_view_looks(display, view)),
        ));
        out.push(g(
            "getDisplayViewRule",
            args.clone(),
            s(c.display_view_rule(display, view)),
        ));
        out.push(g(
            "getDisplayViewDescription",
            args.clone(),
            s(c.display_view_description(display, view)),
        ));
        // The view's aliases (2.6).
        let aliases: Vec<Vec<u8>> = (0..c.num_display_view_aliases(display, view))
            .map(|i| c.display_view_alias(display, view, i).to_vec())
            .collect();
        out.push(g(
            "getDisplayViewAliases",
            args.clone(),
            list(aliases.iter().cloned()),
        ));
        // The names the display and view resolve to (2.6). The probes by other names of the
        // view and of its display come from the wheel's dump of the config (`dump_probes`).
        out.push(g(
            "getCanonicalViewName",
            args.clone(),
            s(c.canonical_view_name(display, view)),
        ));
        out.push(g(
            "getResolvedDisplayViewColorSpaceName",
            args.clone(),
            s(c.resolved_display_view_color_space_name(display, view)),
        ));
    };
    for i in 0..c.num_displays_all() {
        let display = c.display_all(i).to_vec();
        out.push(g(
            "getCanonicalDisplayName",
            vec![s(&display)],
            s(c.canonical_display_name(&display)),
        ));
        out.push(g(
            "getDisplayDescription",
            vec![s(&display)],
            s(c.display_description(&display)),
        ));
        out.push(g(
            "getViews",
            vec![s(&display)],
            list((0..c.num_views(&display)).map(|j| c.view(&display, j).to_vec())),
        ));
        for t in [ViewType::Shared, ViewType::DisplayDefined] {
            let views: Vec<Vec<u8>> = (0..c.num_views_of_type(t, &display))
                .map(|j| c.view_of_type(t, &display, j).to_vec())
                .collect();
            out.push(g(
                "getViews",
                vec![json!({"enum": view_type(t)}), s(&display)],
                list(views.iter().cloned()),
            ));
            for view in views {
                view_getters(&mut out, &display, &view);
            }
        }
    }
    for j in 0..c.num_views_of_type(ViewType::Shared, b"") {
        let view = c.view_of_type(ViewType::Shared, b"", j).to_vec();
        view_getters(&mut out, b"", &view);
    }
    for t in [ViewType::Shared, ViewType::DisplayDefined] {
        out.push(g(
            "getVirtualDisplayViews",
            vec![json!({"enum": view_type(t)})],
            list(
                (0..c.virtual_display_num_views(t)).map(|j| c.virtual_display_view(t, j).to_vec()),
            ),
        ));
    }
    out
}

/// A result as the port writes it: a string the binding couldn't decode as its bytes, an
/// object by its text.
fn normalized(call: &Value) -> Value {
    match (call.get("result"), call.get("undecodable")) {
        (Some(result), _) => match result.get("repr") {
            Some(repr) => match repr.get("undecodable") {
                Some(h) => json!({"bytes": h}),
                None => repr.clone(),
            },
            None => result.clone(),
        },
        (None, Some(h)) => json!({"bytes": h}),
        _ => match call.get("exception") {
            Some(e) => json!({"exception": e["message"]}),
            None => call.clone(),
        },
    }
}

/// A value of the wheel's dump as the port writes it: a string the binding couldn't decode as
/// its bytes.
fn dumped(value: &Value) -> Value {
    match value.get("undecodable") {
        Some(h) => json!({"bytes": h}),
        None => value.clone(),
    }
}

/// The port's answers to the 2.6 display and view probes of the wheel's dump of the config
/// (`dump_config`, oracle/ocio_oracle/config_api.py): each display's `display_probes`
/// (`getCanonicalDisplayName`), and each view's `hasDisplayViewAlias`, `by_probe` and
/// `by_display` (`getCanonicalViewName` and `getResolvedDisplayViewColorSpaceName` of each
/// probe, by the display and by each of its display probes), the shared views' with display
/// "". Returns the differences.
fn dump_probes(c: &Config, dump: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut compare = |what: String, wheel: &Value, port: Value| {
        if dumped(wheel) != port {
            out.push(format!("{what}\n  wheel {wheel}\n  port  {port}"));
        }
    };
    let by_probe = |display: &[u8],
                    view: &[u8],
                    probes: &Value,
                    compare: &mut dyn FnMut(String, &Value, Value)| {
        for p in probes.as_array().expect("probes") {
            let probe = bytes(&p[0]);
            let at = |name: &str| {
                format!(
                    "{name}({:?}, {:?}) (view {:?})",
                    String::from_utf8_lossy(display),
                    String::from_utf8_lossy(&probe),
                    String::from_utf8_lossy(view)
                )
            };
            compare(
                at("getCanonicalViewName"),
                &p[1],
                s(c.canonical_view_name(display, &probe)),
            );
            compare(
                at("getResolvedDisplayViewColorSpaceName"),
                &p[2],
                s(c.resolved_display_view_color_space_name(display, &probe)),
            );
        }
    };
    let view_probes =
        |display: &[u8], entry: &Value, compare: &mut dyn FnMut(String, &Value, Value)| {
            let view = bytes(&entry["view"]);
            for p in entry["hasDisplayViewAlias"]
                .as_array()
                .expect("hasDisplayViewAlias")
            {
                let probe = bytes(&p[0]);
                compare(
                    format!(
                        "hasDisplayViewAlias({:?}, {:?}, {:?})",
                        String::from_utf8_lossy(display),
                        String::from_utf8_lossy(&view),
                        String::from_utf8_lossy(&probe)
                    ),
                    &p[1],
                    json!(c.has_display_view_alias(display, &view, &probe)),
                );
            }
            by_probe(display, &view, &entry["by_probe"], compare);
            if let Some(by_display) = entry.get("by_display") {
                for d in by_display.as_array().expect("by_display") {
                    by_probe(&bytes(&d[0]), &view, &d[1], compare);
                }
            }
        };
    let mut probed = 0;
    for display in dump["displays"].as_array().expect("displays") {
        let name = bytes(&display["display"]);
        for p in display["display_probes"]
            .as_array()
            .expect("display_probes")
        {
            let probe = bytes(&p[0]);
            compare(
                format!(
                    "getCanonicalDisplayName({:?})",
                    String::from_utf8_lossy(&probe)
                ),
                &p[1],
                s(c.canonical_display_name(&probe)),
            );
        }
        for view in display["views"].as_array().expect("views") {
            view_probes(&name, view, &mut compare);
            probed += 1;
        }
    }
    for view in dump["shared_views"].as_array().expect("shared_views") {
        view_probes(b"", view, &mut compare);
        probed += 1;
    }
    // Every view of the port's config was probed: the dump lists each display's views of both
    // types and its active ones (each once), and the shared views.
    let port_views: usize = (0..c.num_displays_all())
        .map(|i| {
            let display = c.display_all(i).to_vec();
            let mut views: Vec<Vec<u8>> = Vec::new();
            for t in [ViewType::Shared, ViewType::DisplayDefined] {
                for j in 0..c.num_views_of_type(t, &display) {
                    let v = c.view_of_type(t, &display, j).to_vec();
                    if !views.contains(&v) {
                        views.push(v);
                    }
                }
            }
            for j in 0..c.num_views(&display) {
                let v = c.view(&display, j).to_vec();
                if !views.contains(&v) {
                    views.push(v);
                }
            }
            views.len()
        })
        .sum::<usize>()
        + c.num_views_of_type(ViewType::Shared, b"") as usize;
    compare(
        "the views the dump probes".to_string(),
        &json!(probed),
        json!(port_views),
    );
    out
}

/// Checks each config against the wheel, both reading an empty environment; panics listing
/// every difference.
fn check(cases: &[(&str, Vec<u8>)]) {
    check_env(&[], cases);
}

/// Checks each config against the wheel, both reading the environment `env` and nothing else;
/// panics listing every difference.
fn check_env(env: &[(&str, &str)], cases: &[(&str, Vec<u8>)]) {
    let ported: Vec<_> = cases.iter().map(|(_, text)| port_load(text, env)).collect();
    // The getters once per config: what a call keeps (validate's result) and logs is the
    // first call's.
    let port_getters: Vec<Vec<Getter>> = ported
        .iter()
        .map(|(loaded, _)| match loaded {
            Ok(config) => getters(config),
            Err(_) => Vec::new(),
        })
        .collect();
    let env_json: serde_json::Map<String, Value> =
        env.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .zip(&port_getters)
        .map(|((_, text), getters)| {
            let mut calls: Vec<Value> = getters
                .iter()
                .map(|(name, args, _)| {
                    if name == "getCurrentContext" {
                        json!({"call": name, "args": args, "as": "ctx"})
                    } else if let Some(m) = name.strip_prefix("ctx.") {
                        json!({"call": m, "on": "ctx", "args": args})
                    } else {
                        json!({"call": name, "args": args})
                    }
                })
                .collect();
            // Last, so that the getters' calls keep and log what they did before it: the
            // wheel's dump of the config, whose view probes the port answers too.
            if !getters.is_empty() {
                calls.push(json!({"dump": "config"}));
            }
            BatchCall {
                cmd: "config_calls",
                args: json!({"config": {"yaml": {"bytes": hex(text)}}, "calls": calls,
                             "env": env_json}),
                blobs: Vec::new(),
            }
        })
        .collect();
    let wheel: Vec<Value> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| r.expect("config_calls").result)
        .collect();

    let mut failures = Vec::new();
    for ((((label, _), (loaded, port_log)), w), port_getters_of) in
        cases.iter().zip(&ported).zip(&wheel).zip(&port_getters)
    {
        let wheel_log = log(&w["config_log"]);
        if &wheel_log != port_log {
            failures.push(format!(
                "{label}: log\n  wheel {:?}\n  port  {:?}",
                wheel_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>(),
                port_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .collect::<Vec<_>>()
            ));
        }
        let config = &w["config"];
        match loaded {
            Err(message) => {
                let wheel_message = match config.get("undecodable") {
                    Some(h) => bytes(&json!({"bytes": h})),
                    None if config.is_null() => b"(the wheel loads it)".to_vec(),
                    None => bytes(&config["exception"]["message"]),
                };
                if &wheel_message != message {
                    failures.push(format!(
                        "{label}: error\n  wheel {}\n  port  {}",
                        String::from_utf8_lossy(&wheel_message),
                        String::from_utf8_lossy(message)
                    ));
                }
            }
            Ok(port_config) => {
                if !config.is_null() {
                    failures.push(format!("{label}: the wheel fails: {config}"));
                    continue;
                }
                let results = w["calls"].as_array().unwrap();
                for ((name, args, port), call) in port_getters_of.iter().zip(results) {
                    if name == "getCurrentContext" {
                        continue;
                    }
                    let wheel = if name == "serialize" || name == "validate" {
                        json!({"result": normalized(call), "log": call["log"]})
                    } else {
                        normalized(call)
                    };
                    if &wheel != port {
                        failures.push(format!(
                            "{label}: {name}{args:?}\n  wheel {wheel}\n  port  {port}"
                        ));
                    }
                }
                assert_eq!(results.len(), port_getters_of.len() + 1, "{label}");
                let dump = &results[port_getters_of.len()]["result"];
                failures.extend(
                    dump_probes(port_config, dump)
                        .into_iter()
                        .map(|d| format!("{label}: {d}")),
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A display of one view of `raw`, so that validate reaches its checks after the displays'.
const DISPLAY: &str = "displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n";

/// A version 2 config of a color space `raw`, the default role's, with `extra` after it.
fn v2(extra: &str) -> Vec<u8> {
    format!(
        "ocio_profile_version: 2\nenvironment: {{}}\nroles: {{default: raw}}\ncolorspaces:\n  \
         - !<ColorSpace> {{name: raw}}\n{extra}"
    )
    .into_bytes()
}

/// Each key of the config's loader, its values and its errors.
#[test]
fn configs_load_as_in_the_wheel() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("minimal", v2("")),
        (
            "version 1",
            b"ocio_profile_version: 1\nroles: {default: raw}\ncolorspaces:\n  - !<ColorSpace> \
              {name: raw, to_reference: !<LogTransform> {}}\n"
                .to_vec(),
        ),
        (
            "version 2.5",
            b"ocio_profile_version: 2.5\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "version 2.6",
            b"ocio_profile_version: 2.6\nroles: {default: raw}\n".to_vec(),
        ),
        ("version 2.7", b"ocio_profile_version: 2.7\n".to_vec()),
        ("no version", b"roles: {default: raw}\n".to_vec()),
        ("version x", b"ocio_profile_version: x\n".to_vec()),
        ("version 2.x", b"ocio_profile_version: 2.x\n".to_vec()),
        ("version 1.2.3", b"ocio_profile_version: 1.2.3\n".to_vec()),
        ("version 3", b"ocio_profile_version: 3\n".to_vec()),
        ("version 2.9", b"ocio_profile_version: 2.9\n".to_vec()),
        ("version 0", b"ocio_profile_version: 0\n".to_vec()),
        ("version -1", b"ocio_profile_version: -1\n".to_vec()),
        (
            "version ' 2'",
            b"ocio_profile_version: \" 2\"\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "version 2abc",
            b"ocio_profile_version: 2abc\nroles: {default: raw}\n".to_vec(),
        ),
        ("version []", b"ocio_profile_version: [2]\n".to_vec()),
        (
            "version 99999999999",
            b"ocio_profile_version: 99999999999\n".to_vec(),
        ),
        ("empty", b"".to_vec()),
        ("a scalar", b"abc".to_vec()),
        ("a sequence", b"[1, 2]".to_vec()),
        ("parse error", b"ocio_profile_version: [2\n".to_vec()),
        ("no default role", b"ocio_profile_version: 2\n".to_vec()),
        (
            "environment",
            b"ocio_profile_version: 2\nenvironment: {A: a, B: \"\", C: ~}\nroles: {default: \
              raw}\n"
                .to_vec(),
        ),
        (
            "no environment",
            b"ocio_profile_version: 2\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "environment twice",
            b"ocio_profile_version: 2\nenvironment: {}\nenvironment: {}\n".to_vec(),
        ),
        (
            "environment list",
            b"ocio_profile_version: 2\nenvironment: [a]\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "search paths",
            b"ocio_profile_version: 2\nsearch_path: a:b:c\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "search path list",
            b"ocio_profile_version: 2\nsearch_path: [a, \"b:c\", \"\"]\nroles: {default: raw}\n"
                .to_vec(),
        ),
        (
            "resource path",
            b"ocio_profile_version: 2\nresource_path: r\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "strictparsing",
            b"ocio_profile_version: 2\nstrictparsing: false\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "strictparsing bad",
            b"ocio_profile_version: 2\nstrictparsing: maybe\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "name and description",
            b"ocio_profile_version: 2\nname: \"n\\n\\n\"\ndescription: |\n  a\n  b\n\nroles: \
              {default: raw}\n"
                .to_vec(),
        ),
        (
            "family separator",
            b"ocio_profile_version: 2\nfamily_separator: \"-\"\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "family separator long",
            b"ocio_profile_version: 2\nfamily_separator: ab\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "family separator bad",
            b"ocio_profile_version: 2\nfamily_separator: \"\\t\"\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "family separator v1",
            b"ocio_profile_version: 1\nfamily_separator: \"-\"\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "luma",
            b"ocio_profile_version: 2\nluma: [0.1, 0.2, 0.7]\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "luma short",
            b"ocio_profile_version: 2\nluma: [0.1, 0.2]\nroles: {default: raw}\n".to_vec(),
        ),
        (
            "roles list",
            b"ocio_profile_version: 2\nroles: [default]\n".to_vec(),
        ),
        (
            "roles many",
            b"ocio_profile_version: 2\nroles: {default: raw, scene_linear: lin, Data: d, b: }\n"
                .to_vec(),
        ),
        (
            "displays",
            v2(
                "displays:\n  sRGB:\n    - !<View> {name: Raw, colorspace: raw}\n    - \
                !<View> {name: Film, colorspace: raw, looks: \"+a\", rule: r, description: \
                \"d\\n\"}\n  P3:\n    - !<View> {name: Raw, colorspace: raw}\n",
            ),
        ),
        ("displays not a map", v2("displays: [sRGB]\n")),
        ("display views not a list", v2("displays:\n  sRGB: x\n")),
        (
            "view without name",
            v2("displays:\n  sRGB:\n    - !<View> {colorspace: raw}\n"),
        ),
        (
            "view without color space",
            v2("displays:\n  sRGB:\n    - !<View> {name: v}\n"),
        ),
        (
            "view with both",
            v2(
                "displays:\n  sRGB:\n    - !<View> {name: v, colorspace: raw, view_transform: \
                vt, display_colorspace: raw}\n",
            ),
        ),
        (
            "view transform view",
            v2(
                "displays:\n  sRGB:\n    - !<View> {name: v, view_transform: vt, \
                display_colorspace: raw}\n",
            ),
        ),
        (
            "view unknown key",
            v2("displays:\n  sRGB:\n    - !<View> {name: v, colorspace: raw, foo: 1}\n"),
        ),
        (
            "view other tag",
            v2(
                "displays:\n  sRGB:\n    - !<Foo> {name: v, colorspace: raw}\n    - !<View> \
                {name: w, colorspace: raw}\n",
            ),
        ),
        (
            "shared views",
            v2(
                "shared_views:\n  - !<View> {name: s, colorspace: raw}\ndisplays:\n  sRGB:\n    \
                - !<View> {name: Raw, colorspace: raw}\n    - !<Views> [s]\n",
            ),
        ),
        ("shared views not a list", v2("shared_views: {a: b}\n")),
        (
            "virtual display",
            v2(
                "shared_views:\n  - !<View> {name: s, colorspace: raw}\nvirtual_display:\n  - \
                !<View> {name: v, colorspace: raw}\n  - !<Views> [s]\n  - !<Foo> {}\n",
            ),
        ),
        (
            "active lists",
            v2(
                "displays:\n  sRGB:\n    - !<View> {name: Raw, colorspace: raw}\n  P3:\n    - \
                !<View> {name: Raw, colorspace: raw}\nactive_displays: [P3, \"a, b\"]\n\
                active_views: [Raw]\ninactive_colorspaces: [raw, x]\n",
            ),
        ),
        ("active displays not a list", v2("active_displays: P3\n")),
        (
            "colorspaces",
            v2(
                "  - !<ColorSpace> {name: lin, family: f}\n  - !<Foo> {name: x}\ndisplay_colorspaces:\n  \
                - !<ColorSpace> {name: d, to_display_reference: !<LogTransform> {}}\n",
            ),
        ),
        (
            "colorspace defined twice",
            v2("  - !<ColorSpace> {name: raw}\n"),
        ),
        (
            "colorspaces not a list",
            b"ocio_profile_version: 2\nroles: {default: raw}\ncolorspaces: {a: b}\n".to_vec(),
        ),
        (
            "looks",
            v2("looks:\n  - !<Look> {name: l, process_space: raw}\n  - !<Foo> {}\n"),
        ),
        (
            "view transforms",
            v2(
                "view_transforms:\n  - !<ViewTransform> {name: vt, from_scene_reference: \
                !<LogTransform> {}}\ndefault_view_transform: vt\n",
            ),
        ),
        (
            "named transforms",
            v2(
                "named_transforms:\n  - !<NamedTransform> {name: nt, transform: \
                !<LogTransform> {}}\n  - !<NamedTransform> {name: nt, transform: \
                !<LogTransform> {}}\n",
            ),
        ),
        (
            "file rules",
            v2(
                "file_rules:\n  - !<Rule> {name: exr, colorspace: raw, pattern: \"*\", \
                extension: exr}\n  - !<Rule> {name: re, colorspace: raw, regex: \".*\\\\.dpx\"}\n  \
                - !<Rule> {name: ColorSpaceNamePathSearch}\n  - !<Rule> {name: Default, \
                colorspace: raw, custom: {a: b}}\n",
            ),
        ),
        (
            "file rules without default",
            v2(
                "file_rules:\n  - !<Rule> {name: exr, colorspace: raw, pattern: \"*\", \
                extension: exr}\n",
            ),
        ),
        (
            "file rules default not last",
            v2(
                "file_rules:\n  - !<Rule> {name: Default, colorspace: raw}\n  - !<Rule> {name: \
                exr, colorspace: raw, pattern: \"*\", extension: exr}\n",
            ),
        ),
        (
            "file rules errors",
            v2(
                "file_rules:\n  - !<Rule> {name: r, colorspace: raw, regex: x, pattern: y}\n  - \
                !<Rule> {name: Default, colorspace: raw}\n",
            ),
        ),
        (
            "file rules default mismatch",
            v2("file_rules:\n  - !<Rule> {name: Default, colorspace: other}\n"),
        ),
        (
            "file rules v1",
            b"ocio_profile_version: 1\nfile_rules:\n  - !<Rule> {name: Default, colorspace: \
              raw}\n"
                .to_vec(),
        ),
        (
            "viewing rules",
            v2(
                "viewing_rules:\n  - !<Rule> {name: r1, colorspaces: raw}\n  - !<Rule> {name: \
                r2, encodings: [log, sdr-video], custom: {k: v}}\n  - !<Foo> {}\n",
            ),
        ),
        (
            "viewing rules errors",
            v2("viewing_rules:\n  - !<Rule> {name: r1}\n"),
        ),
        // The checks of validate past its display check: configs with a display.
        (
            "environment default naming itself with %",
            b"ocio_profile_version: 2\nenvironment: {A: \"%A%\"}\nroles: {default: raw}\n\
              colorspaces:\n  - !<ColorSpace> {name: raw}\ndisplays:\n  d:\n    - !<View> \
              {name: v, colorspace: raw}\n"
                .to_vec(),
        ),
        (
            "environment default naming itself with braces",
            b"ocio_profile_version: 2\nenvironment: {A: \"${A}\"}\nroles: {default: raw}\n\
              colorspaces:\n  - !<ColorSpace> {name: raw}\ndisplays:\n  d:\n    - !<View> \
              {name: v, colorspace: raw}\n"
                .to_vec(),
        ),
        (
            "default view transform in another case",
            v2(&format!(
                "{DISPLAY}view_transforms:\n  - !<ViewTransform> {{name: vt, \
                 from_scene_reference: !<MatrixTransform> {{}}}}\ndefault_view_transform: VT\n"
            )),
        ),
        (
            "file transform source resolving to empty",
            b"ocio_profile_version: 2\nenvironment: {F: \"\"}\nroles: {default: raw}\n\
              colorspaces:\n  - !<ColorSpace> {name: raw}\n  - !<ColorSpace> {name: f, \
              from_scene_reference: !<FileTransform> {src: $F}}\ndisplays:\n  d:\n    - \
              !<View> {name: v, colorspace: raw}\n"
                .to_vec(),
        ),
        (
            "search paths with variables and an empty one",
            v2(&format!(
                "{DISPLAY}search_path: [\"$Q\", \"x/$R/y\", \"\"]\n"
            )),
        ),
        (
            "an empty search path",
            v2(&format!("{DISPLAY}search_path: \"\"\n")),
        ),
        ("no search path", v2(DISPLAY)),
        (
            "encodings differing in case, one unused",
            b"ocio_profile_version: 2\nenvironment: {}\nroles: {default: raw}\ncolorspaces:\n  \
              - !<ColorSpace> {name: raw, encoding: sdr-video}\ndisplays:\n  d:\n    - !<View> \
              {name: v, colorspace: raw}\nviewing_rules:\n  - !<Rule> {name: r1, encodings: \
              [SDR-Video, hdr-video]}\n"
                .to_vec(),
        ),
        (
            "an undefined shared view in the virtual display",
            v2(&format!(
                "{DISPLAY}virtual_display:\n  - !<Views> [nosuch]\n"
            )),
        ),
        (
            "a look as its own process space",
            v2(&format!(
                "{DISPLAY}looks:\n  - !<Look> {{name: lk, process_space: lk}}\n"
            )),
        ),
        (
            "a look named as a named transform",
            v2(&format!(
                "{DISPLAY}named_transforms:\n  - !<NamedTransform> {{name: lk, transform: \
                 !<MatrixTransform> {{}}}}\nlooks:\n  - !<Look> {{name: lk, process_space: \
                 raw}}\n"
            )),
        ),
        (
            "an interop ID naming a role",
            ver(
                "2.5",
                &format!("  - !<ColorSpace> {{name: b, interop_id: default}}\n{DISPLAY}"),
            ),
        ),
        (
            "an interop ID naming an alias",
            ver(
                "2.5",
                &format!(
                    "  - !<ColorSpace> {{name: b, aliases: [al]}}\n  - !<ColorSpace> {{name: \
                     c, interop_id: al}}\n{DISPLAY}"
                ),
            ),
        ),
        (
            "an interop ID naming a named transform",
            ver(
                "2.5",
                &format!(
                    "  - !<ColorSpace> {{name: b, interop_id: nt}}\n{DISPLAY}named_transforms:\n  \
                     - !<NamedTransform> {{name: nt, transform: !<MatrixTransform> {{}}}}\n"
                ),
            ),
        ),
        ("unknown keys", v2("foo: 1\n\"b\\0r\": 2\n")),
        (
            "keys twice",
            b"ocio_profile_version: 2\nroles: {default: raw}\nname: a\nname: b\n".to_vec(),
        ),
    ];
    check(&cases);
}

/// The wheel's built-in configs, from their YAML.
#[test]
fn builtin_configs_load_as_in_the_wheel() {
    let names = Oracle::get()
        .call("builtin_config_names", json!({}), &[])
        .result;
    let mut cases = Vec::new();
    let names: Vec<String> = names
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect();
    for name in &names {
        let source = Oracle::get().call("builtin_config_source", json!({"name": name}), &[]);
        cases.push((name.as_str(), source.blobs[0].clone()));
    }
    check(&cases);
}

/// Upstream's test configs (every `.ocio` and config `.yaml` of `tests/data/files/configs`),
/// read from a stream.
#[test]
fn upstream_test_configs_load_as_in_the_wheel() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/OpenColorIO/tests/data/files/configs");
    let mut files = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("ocio" | "yaml")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    let labels: Vec<String> = files
        .iter()
        .map(|f| {
            f.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    // The configs of upstream's tests as of v2.5.2: if this changes, so did the submodule.
    assert_eq!(labels.len(), 12, "{labels:?}");
    let cases: Vec<(&str, Vec<u8>)> = labels
        .iter()
        .zip(&files)
        .map(|(label, f)| (label.as_str(), std::fs::read(f).unwrap()))
        .collect();
    check(&cases);
}

/// A config of the profile version `version`, of a color space `raw` (the default role's), with
/// `extra` after it.
fn ver(version: &str, extra: &str) -> Vec<u8> {
    format!(
        "ocio_profile_version: {version}\nenvironment: {{}}\nroles: {{default: raw}}\n\
         colorspaces:\n  - !<ColorSpace> {{name: raw}}\n{extra}"
    )
    .into_bytes()
}

/// A config of the profile version `version` whose second color space has the transform `t`
/// (`to_reference`, read in every version).
fn ver_transform(version: &str, t: &str) -> Vec<u8> {
    ver(
        version,
        &format!("  - !<ColorSpace> {{name: b, to_reference: {t}}}\n"),
    )
}

/// What a config's version can't have, refused after loading (`checkVersionConsistency`), and
/// the same at the versions that allow it.
#[test]
fn configs_their_versions_cant_have_are_refused() {
    let builtin = "!<BuiltinTransform> {style: ACEScct_to_ACES2065-1}";
    let mut cases: Vec<(String, Vec<u8>)> = vec![
        // Version 1 transforms, as a color space's, in a group, and as a look's.
        ("v1 builtin".into(), ver_transform("1", builtin)),
        (
            "v1 group builtin".into(),
            ver_transform(
                "1",
                &format!("!<GroupTransform> {{children: [!<LogTransform> {{}}, {builtin}]}}"),
            ),
        ),
        (
            "v1 look builtin".into(),
            ver(
                "1",
                &format!(
                    "looks:\n  - !<Look> {{name: l, process_space: raw, transform: {builtin}}}\n"
                ),
            ),
        ),
        (
            "v1 look inverse group".into(),
            ver(
                "1",
                "looks:\n  - !<Look> {name: l, process_space: raw, inverse_transform: \
                 !<GroupTransform> {children: [!<RangeTransform> {}]}}\n",
            ),
        ),
        (
            "v1 display view transform".into(),
            ver_transform(
                "1",
                "!<DisplayViewTransform> {src: raw, display: d, view: v}",
            ),
        ),
        (
            "v1 exponent with linear".into(),
            ver_transform(
                "1",
                "!<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}",
            ),
        ),
        (
            "v1 fixed function".into(),
            ver_transform("1", "!<FixedFunctionTransform> {style: ACES_RedMod03}"),
        ),
        (
            "v1 log affine".into(),
            ver_transform("1", "!<LogAffineTransform> {base: 10}"),
        ),
        (
            "v1 log camera".into(),
            ver_transform("1", "!<LogCameraTransform> {lin_side_break: 0.1}"),
        ),
        (
            "v1 range".into(),
            ver_transform("1", "!<RangeTransform> {min_in_value: 0, max_in_value: 1}"),
        ),
        (
            "v1 cdl style".into(),
            ver_transform("1", "!<CDLTransform> {slope: [2, 2, 2], style: asc}"),
        ),
        (
            "v1 cdl default style".into(),
            ver_transform("1", "!<CDLTransform> {slope: [2, 2, 2], style: noclamp}"),
        ),
        (
            "v1 exponent style".into(),
            ver_transform("1", "!<ExponentTransform> {value: 2, style: mirror}"),
        ),
        (
            "v1 exponent clamp".into(),
            ver_transform(
                "1",
                "!<ExponentTransform> {value: [2, 2, 2, 1], style: clamp}",
            ),
        ),
        (
            "v1 file cubic".into(),
            ver_transform("1", "!<FileTransform> {src: a.lut, interpolation: cubic}"),
        ),
        (
            "v1 file cdl style".into(),
            ver_transform("1", "!<FileTransform> {src: a.cc, cdl_style: asc}"),
        ),
        (
            "v1 v1 transforms".into(),
            ver_transform(
                "1",
                "!<GroupTransform> {children: [!<LogTransform> {}, !<MatrixTransform> {}, \
                 !<AllocationTransform> {}, !<ColorSpaceTransform> {src: raw, dst: raw}, \
                 !<LookTransform> {src: raw, dst: raw, looks: l}, !<FileTransform> {src: a.lut, \
                 interpolation: best}]}",
            ),
        ),
        // Version 1 config sections.
        (
            "v1 inactive".into(),
            ver("1", "inactive_colorspaces: [raw]\n"),
        ),
        (
            "v1 viewing rules".into(),
            ver(
                "1",
                "viewing_rules:\n  - !<Rule> {name: r, colorspaces: raw}\n",
            ),
        ),
        (
            "v1 shared views".into(),
            ver(
                "1",
                "shared_views:\n  - !<View> {name: s, colorspace: raw}\n",
            ),
        ),
        (
            "v1 display views".into(),
            ver(
                "1",
                "displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n    - !<Views> [s]\n",
            ),
        ),
        (
            "v1 virtual display".into(),
            ver(
                "1",
                "virtual_display:\n  - !<View> {name: s, colorspace: raw}\n",
            ),
        ),
        (
            "v1 display colorspaces".into(),
            ver("1", "display_colorspaces:\n  - !<ColorSpace> {name: d}\n"),
        ),
        (
            "v1 interop".into(),
            ver("1", "  - !<ColorSpace> {name: b, interop_id: abc}\n"),
        ),
        (
            "v1 view transforms".into(),
            ver(
                "1",
                "view_transforms:\n  - !<ViewTransform> {name: vt, from_reference: \
                 !<LogTransform> {}}\n",
            ),
        ),
        (
            "v1 default vt".into(),
            ver("1", "default_view_transform: vt\n"),
        ),
        (
            "v1 named transforms".into(),
            ver(
                "1",
                "named_transforms:\n  - !<NamedTransform> {name: nt, transform: !<LogTransform> \
                 {}}\n",
            ),
        ),
        // Interchange attributes before 2.5.
        (
            "v2.4 cs interchange".into(),
            ver(
                "2.4",
                "  - !<ColorSpace> {name: b, interchange: {amf_transform_ids: x}}\n",
            ),
        ),
        (
            "v2.4 look interchange".into(),
            ver(
                "2.4",
                "looks:\n  - !<Look> {name: l, process_space: raw, interchange: \
                 {amf_transform_ids: x}}\n",
            ),
        ),
        (
            "v2.4 vt interchange".into(),
            ver(
                "2.4",
                "view_transforms:\n  - !<ViewTransform> {name: vt, from_scene_reference: \
                 !<LogTransform> {}, interchange: {amf_transform_ids: x}}\n",
            ),
        ),
        (
            "v2.5 interchange".into(),
            ver(
                "2.5",
                "  - !<ColorSpace> {name: b, interchange: {amf_transform_ids: x}}\nlooks:\n  - \
                 !<Look> {name: l, process_space: raw, interchange: {amf_transform_ids: y}}\n",
            ),
        ),
    ];
    // Built-in styles of later minor versions, a version before and at the first that allows
    // them; the names compare ignoring case.
    for (style, first) in [
        ("ACES-LMT - ACES 1.3 Reference Gamut Compression", 1),
        ("aces-lmt - aces 1.3 reference gamut compression", 1),
        ("ARRI_LOGC4_to_ACES2065-1", 2),
        ("CURVE - CANON_CLOG2_to_LINEAR", 2),
        ("CURVE - CANON_CLOG3_to_LINEAR", 2),
        ("DISPLAY - CIE-XYZ-D65_to_DisplayP3", 3),
        ("APPLE_LOG_to_ACES2065-1", 4),
        ("CURVE - HLG-OETF-INVERSE", 4),
        (
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0",
            4,
        ),
        ("DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR", 4),
        ("display - cie-xyz-d65_to_displayp3-hdr", 4),
        // The MIRROR NEGS styles of 2.5, whose check 2.6.0 added.
        ("DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS", 5),
        (
            "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
            5,
        ),
        ("DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS", 5),
        ("display - cie-xyz-d65_to_srgb - mirror negs", 5),
        ("DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS", 5),
        // The style of 2.6.
        ("APPLE_LOG-APPLEWG_to_ACES2065-1", 6),
        ("apple_log-applewg_to_aces2065-1", 6),
    ] {
        for minor in [first - 1, first] {
            cases.push((
                format!("{style} in 2.{minor}"),
                ver_transform(
                    &format!("2.{minor}"),
                    &format!("!<BuiltinTransform> {{style: {style}}}"),
                ),
            ));
        }
    }
    // Fixed function styles of later minor versions.
    for (style, first) in [
        ("ACES_GamutComp13", 1),
        ("Lin_TO_PQ", 4),
        ("ACES2_OutputTransform", 4),
        ("RGB_TO_HSY_LIN", 5),
        ("RGB_TO_HSY_LOG", 5),
        ("RGB_TO_HSY_VID", 5),
        ("ACES2_RGB_TO_HMJ", 6),
        ("aces2_rgb_to_hmj", 6),
    ] {
        let params = match style {
            "ACES_GamutComp13" => ", params: [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2]",
            "ACES2_RGB_TO_HMJ" | "aces2_rgb_to_hmj" => {
                ", params: [0.7347, 0.2653, 0, 1, 0.0001, -0.077, 0.32168, 0.33767]"
            }
            "ACES2_OutputTransform" => {
                ", params: [100, 0.7347, 0.2653, 0, 1, 0.0001, -0.077, 0.3127, 0.329]"
            }
            _ => "",
        };
        for minor in [first - 1, first] {
            cases.push((
                format!("{style} in 2.{minor}"),
                ver_transform(
                    &format!("2.{minor}"),
                    &format!(
                        "!<GroupTransform> {{children: [!<FixedFunctionTransform> {{style: \
                         {style}{params}}}]}}"
                    ),
                ),
            ));
        }
    }
    let refs: Vec<(&str, Vec<u8>)> = cases.iter().map(|(l, t)| (l.as_str(), t.clone())).collect();
    check(&refs);
}

/// Views, search paths and descriptions at the edges of their loaders.
#[test]
fn view_search_path_and_description_edges_load_as_in_the_wheel() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        // A shared view without its tag is no view: its name stays empty.
        (
            "shared view untagged",
            v2("shared_views:\n  - {name: s, colorspace: raw}\n"),
        ),
        // `look` is read as `looks`, the later key wins.
        (
            "view look and looks",
            v2("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, look: a, looks: b}\n"),
        ),
        (
            "view looks and look",
            v2("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, looks: b, look: a}\n"),
        ),
        (
            "view transform only",
            v2("displays:\n  d:\n    - !<View> {name: v, view_transform: vt}\n"),
        ),
        (
            "view empty color space",
            v2("displays:\n  d:\n    - !<View> {name: v, colorspace: \"\"}\n"),
        ),
        (
            "view empty display color space",
            v2(
                "displays:\n  d:\n    - !<View> {name: v, view_transform: vt, \
                 display_colorspace: \"\"}\n",
            ),
        ),
        // A display's untagged list isn't its shared views; an empty verbatim tag is no tag.
        (
            "display untagged list",
            v2(
                "shared_views:\n  - !<View> {name: s, colorspace: raw}\ndisplays:\n  d:\n    - \
                 !<View> {name: v, colorspace: raw}\n    - [s]\n",
            ),
        ),
        (
            "display empty tag",
            v2(
                "shared_views:\n  - !<View> {name: s, colorspace: raw}\ndisplays:\n  d:\n    - \
                 !<View> {name: v, colorspace: raw}\n    - !<> [s]\n",
            ),
        ),
        ("search path [a]", v2("search_path: [a]\n")),
        ("search path [\"\"]", v2("search_path: [\"\"]\n")),
        ("search path {a: b}", v2("search_path: {a: b}\n")),
        // Descriptions ending in a carriage return keep it; the virtual display's views keep
        // their trailing newlines (I-144).
        (
            "descriptions with carriage returns",
            v2(
                "  - !<ColorSpace> {name: b, description: \"c\\r\\n\\n\"}\ndescription: \
                 \"d\\r\"\ndisplays:\n  d:\n    - !<View> {name: v, colorspace: raw, \
                 description: \"x\\r\\n\"}\n",
            ),
        ),
        (
            "virtual display view description",
            v2(
                "virtual_display:\n  - !<View> {name: v, colorspace: raw, description: \
                 \"d\\n\\n\"}\n",
            ),
        ),
    ];
    check(&cases);
}

/// Configs read in an environment: the active and inactive lists the environment overrides
/// (`OCIO_ACTIVE_DISPLAYS`, `OCIO_ACTIVE_VIEWS`, `OCIO_INACTIVE_COLORSPACES`), and the current
/// context's variables, with and without an `environment` section.
#[test]
fn configs_load_in_an_environment_as_in_the_wheel() {
    const COLOR_SPACES: &str = "colorspaces:\n  - !<ColorSpace> {name: raw}\n  - \
                                !<ColorSpace> {name: b}\n  - !<ColorSpace> {name: c}\n";
    const DISPLAYS: &str = "displays:\n  D1:\n    - !<View> {name: v1, colorspace: raw}\n    - \
                            !<View> {name: v2, colorspace: b}\n  D2:\n    - !<View> {name: v1, \
                            colorspace: raw}\n";
    let mut owned = Vec::new();
    for environment in [
        "environment: {}\n",
        "environment: {A: adef, B: bdef}\n",
        "",
        "environment: {A: \"$Z\"}\n",
    ] {
        for extra in [
            "",
            "inactive_colorspaces: [b]\n",
            "inactive_colorspaces: [c, b]\nactive_displays: [D1]\nactive_views: [v2]\n",
            "search_path: \"$A:x\"\n",
        ] {
            owned.push((
                format!("{}|{}", environment.trim(), extra.trim()),
                format!(
                    "ocio_profile_version: 2\n{environment}roles: {{default: raw}}\n\
                     {COLOR_SPACES}{DISPLAYS}{extra}"
                )
                .into_bytes(),
            ));
        }
    }
    owned.push((
        "version 1 environment".to_string(),
        format!(
            "ocio_profile_version: 1\nenvironment: {{A: adef}}\nroles: {{default: raw}}\n\
             {COLOR_SPACES}{DISPLAYS}"
        )
        .into_bytes(),
    ));
    let cases: Vec<(&str, Vec<u8>)> = owned.iter().map(|(l, t)| (l.as_str(), t.clone())).collect();
    for env in [
        &[("A", "aenv"), ("Z", "zenv")][..],
        &[("OCIO_INACTIVE_COLORSPACES", "b"), ("A", "aenv")],
        &[("OCIO_INACTIVE_COLORSPACES", "c, raw"), ("B", "benv")],
        &[("OCIO_ACTIVE_DISPLAYS", "D2"), ("OCIO_ACTIVE_VIEWS", "v1")],
        &[("OCIO_INACTIVE_COLORSPACES", "")],
    ] {
        check_env(env, &cases);
    }
}

/// A config validated twice: the second call gives the first result again, the message
/// upstream kept (`m_validationtext`), which is empty where the first failure didn't set it
/// (docs/improvements.md, I-147).
#[test]
fn validating_twice_as_in_the_wheel() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("passes", v2("displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n")),
        ("no displays", v2("")),
        (
            "unresolved environment variable",
            b"ocio_profile_version: 2\nenvironment: {A: $B}\nroles: {default: raw}\ncolorspaces:\n  \
              - !<ColorSpace> {name: raw}\ndisplays:\n  d:\n    - !<View> {name: v, colorspace: \
              raw}\n"
                .to_vec(),
        ),
        (
            "bad looks",
            v2("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, looks: \"a, |\"}\n"),
        ),
    ];
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(_, text)| BatchCall {
            cmd: "config_calls",
            args: json!({"config": {"yaml": {"bytes": hex(text)}},
                         "calls": [{"call": "validate"}, {"call": "validate"}]}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for ((label, text), w) in cases.iter().zip(wheel) {
        let w = w.expect("config_calls").result;
        let (loaded, _) = port_load(text, &[]);
        let config = loaded.unwrap_or_else(|e| panic!("{label}: {}", String::from_utf8_lossy(&e)));
        for i in 0..2 {
            let (port, _) = captured(&[], || config.validate());
            let port = match port {
                Ok(()) => Value::Null,
                Err(e) => json!({"exception": s(e.what())}),
            };
            let wheel = normalized(&w["calls"][i]);
            if wheel != port {
                failures.push(format!(
                    "{label}: validate #{i}\n  wheel {wheel}\n  port  {port}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `OCIO_ACTIVE_DISPLAYS` naming no display of the config, or a separator only, with a config
/// whose `active_displays` names none either: validate's checks of both lists.
#[test]
fn active_displays_from_the_environment_as_in_the_wheel() {
    let text = v2(&format!("{DISPLAY}active_displays: [zz]\n"));
    for value in ["qq:d", "qq", " , "] {
        check_env(&[("OCIO_ACTIVE_DISPLAYS", value)], &[(value, text.clone())]);
    }
}

/// A version 2.6 config of the color spaces `raw` (the default role's), `cs` and the display
/// color space `dcs` (alias `dcs_alias`), with `extra` after them.
fn v26(extra: &str) -> Vec<u8> {
    format!(
        "ocio_profile_version: 2.6\nenvironment: {{}}\nroles: {{default: raw}}\ncolorspaces:\n  \
         - !<ColorSpace> {{name: raw}}\n  - !<ColorSpace> {{name: cs}}\ndisplay_colorspaces:\n  \
         - !<ColorSpace> {{name: dcs, aliases: [dcs_alias]}}\nview_transforms:\n  - \
         !<ViewTransform> {{name: vt, from_scene_reference: !<MatrixTransform> {{}}}}\n{extra}"
    )
    .into_bytes()
}

/// View aliases and `use_display_aliases` (2.6): read, kept, written and validated as in the
/// wheel, and the loader's refusals of colliding names and aliases, of aliases on the virtual
/// display's views and of what a version before 2.6 can't have.
#[test]
fn view_aliases_load_as_in_the_wheel() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "aliases of views and shared views",
            v26(
                "shared_views:\n  - !<View> {name: sv, view_transform: vt, display_colorspace: \
                 <USE_DISPLAY_NAME>, aliases: [sv_a, sv_b]}\ndisplays:\n  dcs:\n    - !<View> \
                 {name: v1, colorspace: raw, aliases: [v1_b, v1_a]}\n    - !<View> {name: v2, \
                 colorspace: cs, looks: l, rule: r, description: d, aliases: [v2_a]}\n    - \
                 !<Views> [sv]\n  other:\n    - !<View> {name: v3, colorspace: raw, aliases: \
                 [v1_a]}\n",
            ),
        ),
        (
            "use_display_aliases true",
            v26(
                "use_display_aliases: true\ndisplays:\n  dcs:\n    - !<View> {name: v, colorspace: raw}\n",
            ),
        ),
        (
            "use_display_aliases false",
            v26(
                "use_display_aliases: false\ndisplays:\n  dcs:\n    - !<View> {name: v, colorspace: raw}\n",
            ),
        ),
        (
            "use_display_aliases not a bool",
            v26("use_display_aliases: maybe\n"),
        ),
        (
            "use_display_aliases in 2.5",
            ver("2.5", "use_display_aliases: true\n"),
        ),
        (
            "use_display_aliases false in 2.5",
            ver("2.5", "use_display_aliases: false\n"),
        ),
        (
            "aliases that need quotes",
            v26(
                "displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: [\"a,b\", \
                 \"c: d\", \"'q'\", \"\\\"dq\\\"\", \"x\\xe9\", \" sp \", \"\"]}\n",
            ),
        ),
        (
            "an alias with a NUL",
            v26(
                "displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: [\"a\0b\", c]}\n",
            ),
        ),
        (
            "aliases empty",
            v26("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: []}\n"),
        ),
        (
            "aliases null",
            v26("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: ~}\n"),
        ),
        (
            "aliases a scalar",
            v26("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: a}\n"),
        ),
        (
            "aliases a map",
            v26("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: {a: b}}\n"),
        ),
        (
            "an alias that is the view's own name",
            v26("displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: [V, v]}\n"),
        ),
        (
            "an alias naming a sibling view",
            v26(
                "displays:\n  d:\n    - !<View> {name: v1, colorspace: raw}\n    - !<View> \
                 {name: v2, colorspace: raw, aliases: [V1]}\n",
            ),
        ),
        (
            "a name that is a sibling's alias",
            v26(
                "displays:\n  d:\n    - !<View> {name: v1, colorspace: raw, aliases: [a]}\n    \
                 - !<View> {name: A, colorspace: raw}\n",
            ),
        ),
        (
            "aliases shared by siblings",
            v26(
                "displays:\n  d:\n    - !<View> {name: v1, colorspace: raw, aliases: [x, a]}\n    \
                 - !<View> {name: v2, colorspace: raw, aliases: [y, A]}\n",
            ),
        ),
        (
            "a view replaced with other aliases",
            v26(
                "displays:\n  d:\n    - !<View> {name: v1, colorspace: raw, aliases: [a]}\n    \
                 - !<View> {name: v2, colorspace: raw, aliases: [b]}\n    - !<View> {name: V1, \
                 colorspace: cs, aliases: [c]}\n",
            ),
        ),
        (
            "a view alias naming a shared view of the display",
            v26(
                "shared_views:\n  - !<View> {name: sv, colorspace: raw, aliases: [s]}\n\
                 displays:\n  d:\n    - !<Views> [sv]\n    - !<View> {name: v, colorspace: raw, \
                 aliases: [S]}\n",
            ),
        ),
        (
            "a shared view whose alias names a view of the display",
            v26(
                "shared_views:\n  - !<View> {name: sv, colorspace: raw, aliases: [v]}\n\
                 displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n    - !<Views> \
                 [sv]\n",
            ),
        ),
        (
            "shared views sharing an alias",
            v26(
                "shared_views:\n  - !<View> {name: sv1, colorspace: raw, aliases: [a]}\n  - \
                 !<View> {name: sv2, colorspace: raw, aliases: [b, A]}\n",
            ),
        ),
        (
            "a shared view named as another's alias",
            v26(
                "shared_views:\n  - !<View> {name: sv1, colorspace: raw, aliases: [a]}\n  - \
                 !<View> {name: a, colorspace: raw}\n",
            ),
        ),
        (
            "a shared view replaced",
            v26(
                "shared_views:\n  - !<View> {name: sv1, colorspace: raw, aliases: [a]}\n  - \
                 !<View> {name: sv1, colorspace: cs, aliases: [b]}\n",
            ),
        ),
        (
            "an unused shared view's alias names a view",
            v26(
                "shared_views:\n  - !<View> {name: sv, colorspace: raw, aliases: [v]}\n\
                 displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n",
            ),
        ),
        (
            "view aliases in 2.5",
            ver(
                "2.5",
                "displays:\n  d:\n    - !<View> {name: v, colorspace: raw, aliases: [a]}\n",
            ),
        ),
        (
            "shared view aliases in 2.5",
            ver(
                "2.5",
                "shared_views:\n  - !<View> {name: sv, colorspace: raw, aliases: [a]}\n\
                 displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n",
            ),
        ),
        (
            "virtual display view aliases",
            v26(
                "displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\nvirtual_display:\n  \
                 - !<View> {name: vv, colorspace: raw, aliases: [a]}\n",
            ),
        ),
        (
            "virtual display shared view with aliases",
            v26(
                "shared_views:\n  - !<View> {name: sv, colorspace: raw, aliases: [a]}\n\
                 displays:\n  d:\n    - !<View> {name: v, colorspace: raw}\nvirtual_display:\n  \
                 - !<Views> [sv]\n",
            ),
        ),
    ];
    check(&cases);
}

/// Display and view names resolved as in the wheel (2.6): canonical display names through the
/// display color spaces and their aliases (with and without `use_display_aliases`), canonical
/// view names through view aliases, the views' resolved color spaces (`<USE_DISPLAY_NAME>`,
/// aliases, roles, named transforms, names that resolve to nothing), display descriptions, and
/// whether each color space, alias and role is used.
#[test]
fn display_aliases_resolve_as_in_the_wheel() {
    const DISPLAY_SPACES: &str = "  - !<ColorSpace> {name: d1, aliases: [d1_old, D1 alias], \
                                  description: the first}\n  - !<ColorSpace> {name: d2_cs, \
                                  aliases: [d2], description: the second}\n  - !<ColorSpace> \
                                  {name: unused_dcs, aliases: [d3], description: unused}\n";
    let config = |use_display_aliases: &str, extra: &str| {
        format!(
            "ocio_profile_version: 2.6\n{use_display_aliases}environment: {{}}\nroles: \
             {{default: raw, cs_role: cs, d_role: d1, dr: d2_cs}}\ncolorspaces:\n  - !<ColorSpace> {{name: \
             raw}}\n  - !<ColorSpace> {{name: cs, aliases: [cs_old]}}\n  - !<ColorSpace> \
             {{name: d4, description: scene}}\n  - !<ColorSpace> {{name: s1, aliases: [sd]}}\n  \
             - !<ColorSpace> {{name: x, aliases: [x_old]}}\n\
             display_colorspaces:\n{DISPLAY_SPACES}\
             view_transforms:\n  - !<ViewTransform> {{name: vt, from_scene_reference: \
             !<MatrixTransform> {{}}}}\nnamed_transforms:\n  - !<NamedTransform> {{name: nt, \
             aliases: [nt_old], transform: !<MatrixTransform> {{}}}}\n{extra}"
        )
        .into_bytes()
    };
    let displays = "shared_views:\n  - !<View> {name: sv, view_transform: vt, display_colorspace: \
                    <USE_DISPLAY_NAME>, aliases: [sv_old]}\ndisplays:\n  d1:\n    - !<View> \
                    {name: v1, view_transform: vt, display_colorspace: d1_old, aliases: [v1_old]}\n\
                    \x20   - !<View> {name: v2, colorspace: cs_old}\n    - !<View> {name: v3, \
                    colorspace: cs_role}\n    - !<View> {name: v4, colorspace: nt_old}\n    - \
                    !<View> {name: v5, colorspace: x_old}\n    - \
                    !<Views> [sv]\n  d2:\n    - !<Views> [sv]\n  d3:\n    - !<View> {name: v, \
                    colorspace: raw}\n  d4:\n    - !<View> {name: v, colorspace: d4}\n  sd:\n    - \
                    !<View> {name: v, colorspace: s1}\n  d_role:\n    \
                    - !<View> {name: v, view_transform: vt, display_colorspace: nothing}\n";
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "display aliases on",
            config("use_display_aliases: true\n", displays),
        ),
        ("display aliases off", config("", displays)),
        (
            "a display named by an alias",
            config(
                "use_display_aliases: true\n",
                "displays:\n  D1 alias:\n    - !<View> {name: v, view_transform: vt, \
                 display_colorspace: d1}\n  d1_old:\n    - !<View> {name: w, colorspace: raw}\n",
            ),
        ),
        (
            "views of an inactive display",
            config(
                "use_display_aliases: true\n",
                "displays:\n  d1:\n    - !<View> {name: v, colorspace: d1, aliases: [old]}\n  \
                 other:\n    - !<View> {name: o, colorspace: raw}\nactive_displays: [other]\n\
                 active_views: [o]\ninactive_colorspaces: [d1]\n",
            ),
        ),
    ];
    // Every config loads, so that the getters, not the reader's error, are compared.
    for (label, text) in &cases {
        assert!(Config::create_from_stream(text).is_ok(), "{label}");
    }
    check(&cases);
}

/// Color Interop IDs as in the wheel (2.6): the local ID of every color space name and alias
/// and the color space each ID finds, in configs named for the sanitizing (upper case, spaces,
/// punctuation, bytes past ASCII), the reserved ASWF config names (and names that only look
/// like them), and a config named `local`.
#[test]
fn color_interop_ids_as_in_the_wheel() {
    let spaces = "colorspaces:\n  - !<ColorSpace> {name: raw}\n  - !<ColorSpace> {name: ACES cg, \
                  aliases: [lin_ap1, ACES CG alias]}\n  - !<ColorSpace> {name: \"Odd: name, \
                  {x}\", aliases: [\"Also odd!\", clean_alias]}\n  - !<ColorSpace> {name: \"caf\
                  \u{e9} \u{4e2d}\", aliases: [\"\t tab\"]}\n  - !<ColorSpace> {name: \"ns:raw\"}\
                  \n  - !<ColorSpace> {name: \"My Color Space?\"}\n  - !<ColorSpace> {name: \
                  \"My Color Space!\"}\n";
    let named = |name: &str| {
        format!(
            "ocio_profile_version: 2.6\nname: {name}\nenvironment: {{}}\nroles: {{default: raw, \
             r: ACES cg}}\n{spaces}"
        )
        .into_bytes()
    };
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("no name", named("\"\"")),
        ("My Studio!", named("My Studio!")),
        ("mystudio*", named("mystudio*")),
        ("ns", named("ns")),
        ("local", named("local")),
        ("a name with a colon", named("\"a:b\"")),
        ("a name past ASCII", named("\"st\u{fc}dio\"")),
        ("reserved cg", named("cg-config-v4.0.0_aces-v2.0_ocio-v2.5")),
        (
            "reserved upper case",
            named("CG-config-v4.0.0_aces-v2.0_ocio-v2.5"),
        ),
        (
            "reserved future",
            named("studio-config-v5.1.2_aces-v2.2.1_ocio-v2.7"),
        ),
        (
            "reserved v10",
            named("cg-config-v10.0.0_aces-v2.0_ocio-v2.5"),
        ),
        (
            "not reserved v3",
            named("studio-config-v3.0.0_aces-v2.0_ocio-v2.4"),
        ),
        (
            "not reserved suffix",
            named("cg-config-v4.0.0_aces-v2.0_ocio-v2.5_custom"),
        ),
        (
            "not reserved huge",
            named("cg-config-v99999999999.0.0_aces-v2.0_ocio-v2.5"),
        ),
        (
            "not reserved two digits",
            named("cg-config-v4.0_aces-v2.0_ocio-v2.5"),
        ),
        (
            "not reserved empty aces",
            named("cg-config-v4.0.0_aces-v_ocio-v2.5"),
        ),
        ("reserved dots", named("cg-config-v4.0.0_aces-v..._ocio-v.")),
        (
            "reserved leading zeros",
            named("cg-config-v0004.0.0_aces-v2_ocio-v2"),
        ),
    ];
    // Every config loads, so that the getters, not the reader's error, are compared.
    for (label, text) in &cases {
        assert!(Config::create_from_stream(text).is_ok(), "{label}");
    }
    check(&cases);
}
