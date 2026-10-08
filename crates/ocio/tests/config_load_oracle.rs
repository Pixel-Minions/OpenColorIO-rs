// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Configs read from YAML (`Config::CreateFromStream`) against the wheel, through the
//! oracle's `config_calls`: the error, byte for byte, or the config's state through its
//! getters (the version, name, description, family separator, search path, environment and its
//! mode, strict parsing, luma, roles, color spaces, displays and their views with each view's
//! getters, shared and virtual views, active and inactive lists, looks, view transforms,
//! named transforms, and the file and viewing rules by their text); and the warnings logged.
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

/// The port's config of `text`, or its error's message, and what OCIO logged meanwhile. OCIO
/// reads the environment `env` and nothing else meanwhile.
fn port_load(text: &[u8], env: &[(&str, &str)]) -> (Loaded, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(PoisonError::into_inner);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        sink.lock().unwrap().push(m.to_vec());
    })))
    .unwrap();
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries(env))));
    let loaded = Config::create_from_stream(text).map_err(|e| e.what().to_vec());
    set_thread_env_provider(None);
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (loaded, log)
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
            args,
            s(c.display_view_description(display, view)),
        ));
    };
    for i in 0..c.num_displays_all() {
        let display = c.display_all(i).to_vec();
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
        _ => call.clone(),
    }
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
    let env_json: serde_json::Map<String, Value> =
        env.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .zip(&ported)
        .map(|((_, text), (loaded, _))| {
            let calls: Vec<Value> = match loaded {
                Ok(config) => getters(config)
                    .into_iter()
                    .map(|(name, args, _)| {
                        if name == "getCurrentContext" {
                            json!({"call": name, "args": args, "as": "ctx"})
                        } else if let Some(m) = name.strip_prefix("ctx.") {
                            json!({"call": m, "on": "ctx", "args": args})
                        } else {
                            json!({"call": name, "args": args})
                        }
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
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
    for (((label, _), (loaded, port_log)), w) in cases.iter().zip(&ported).zip(&wheel) {
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
            Ok(c) => {
                if !config.is_null() {
                    failures.push(format!("{label}: the wheel fails: {config}"));
                    continue;
                }
                let results = w["calls"].as_array().unwrap();
                for ((name, args, port), call) in getters(c).iter().zip(results) {
                    if name == "getCurrentContext" {
                        continue;
                    }
                    let wheel = normalized(call);
                    if &wheel != port {
                        failures.push(format!(
                            "{label}: {name}{args:?}\n  wheel {wheel}\n  port  {port}"
                        ));
                    }
                }
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
    ] {
        let params = match style {
            "ACES_GamutComp13" => ", params: [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2]",
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
