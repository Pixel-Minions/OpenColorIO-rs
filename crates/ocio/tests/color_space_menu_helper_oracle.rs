// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The app helpers' menus (`ColorSpaceMenuParameters`, `ColorSpaceMenuHelper`) and
//! `ColorSpaceHelpers::AddColorSpace` against the wheel, through the oracle's `object_calls`.
//!
//! Each menu case compares the parameters' text and added color spaces, whether the menu is
//! made (or the exception), its log, its text, and every item's name, UI name, family,
//! description and hierarchy levels, an index past them, and the lookups by name and UI name;
//! some cases ask for the same menu again, which both keep and log about only once.
//!
//! Both keep their menus for the life of the process, by the text of the parameters (which
//! holds the config's cache ID). The wheel runs each request in a process of its own; the
//! port runs every test of this file in one. So every config here has a description of its
//! own (`tagged`), and no two requests share a config.

use std::collections::BTreeSet;
use std::sync::Arc;

use ocio::color_space_helpers::add_color_space;
use ocio::{
    BuiltinConfigRegistry, ColorSpaceMenuHelper, ColorSpaceMenuParameters, ColorSpaceVisibility,
    Config, SearchReferenceSpaceType,
};
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use ocio_ops::utils::string_utils;
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, bytes_arg, log};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------------------------
// Configs

/// The config of upstream's app helpers tests (tests/cpu/apphelpers/configs.data:5-173 @
/// v2.5.2).
const CATEGORY_TEST_CONFIG: &str = include_str!("data/apphelpers_category_test_config.ocio");

/// A config of upstream's `ColorSpaceMenuHelper/usability_issues` test
/// (tests/cpu/apphelpers/ColorSpaceHelpers_tests.cpp:699-724 @ v2.5.2), with a role named
/// like nothing and an alias, and without displays: the menus log the config's validation
/// warning.
const NO_DISPLAY_CONFIG: &str = "ocio_profile_version: 2\nroles:\n  default: raw\n  \
    Look_Space: test_2\n\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n    categories: [ \
    file-io ]\n\n  - !<ColorSpace>\n    name: test_1\n    aliases: [ first ]\n    categories: \
    [ file-io, working-space ]\n    family: A /B/ /C\n\n  - !<ColorSpace>\n    name: test_2\n    \
    categories: [ file-io ]\n\n  - !<ColorSpace>\n    name: test_3\n";

/// A version 1 config (upstream's `ColorSpaceMenuHelper/no_category` test,
/// ColorSpaceHelpers_tests.cpp:821-854 @ v2.5.2).
const V1_CONFIG: &str = "ocio_profile_version: 1\n\nenvironment:\n  {}\n\nsearch_path: luts\n\
    strictparsing: true\n\nroles:\n  rendering: test_1\n  default: raw\n\ndisplays:\n  DISP_1:\n    \
    - !<View> {name: VIEW_1, colorspace: test_1}\n    - !<View> {name: VIEW_2, colorspace: \
    test_2}\n\nactive_displays: []\nactive_views: []\n\ncolorspaces:\n  - !<ColorSpace>\n    \
    name: raw\n    family: Raw\n    description: A raw color space. Conversions to and from this \
    space are no-ops.\n    isdata: true\n    allocation: uniform\n\n  - !<ColorSpace>\n    name: \
    test_1\n\n  - !<ColorSpace>\n    name: test_2\n ";

/// Where a config of a case comes from: a YAML text, or a built-in config.
#[derive(Clone)]
enum Source {
    Yaml(String),
    Builtin(String),
}

/// A config of `source` with the description `tag`, on both sides: the port's, and the calls
/// that make the wheel's the same (`object_calls`' config, then `setDescription`).
fn tagged(source: &Source, tag: &str) -> (Arc<Config>, Value, Vec<Value>) {
    let (config, wheel) = match source {
        Source::Yaml(text) => (
            Config::create_from_stream(text.as_bytes()).unwrap(),
            json!({"yaml": text}),
        ),
        Source::Builtin(name) => (
            Config::create_from_builtin_config(name.as_bytes()).unwrap(),
            json!({"builtin": name}),
        ),
    };
    let mut config = (*config).clone();
    config.set_description(tag);
    let calls = vec![json!({"call": "setDescription", "args": [tag]})];
    let config = Arc::new(config);
    // The wheel's requests validate the config first (`compare`), which logs what it finds the
    // first time only: so does the port's.
    let _ = capture(|| config.validate());
    (config, wheel, calls)
}

// ---------------------------------------------------------------------------------------------
// Cases

/// A small deterministic generator (xorshift64*), so the cases never change unnoticed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, list: &'a [Vec<u8>]) -> &'a [u8] {
        &list[self.below(list.len())]
    }
}

/// A menu's parameters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Case {
    role: Vec<u8>,
    app: Vec<u8>,
    encodings: Vec<u8>,
    user: Vec<u8>,
    include_color_spaces: bool,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    include_roles: bool,
    search: usize,
    added: Vec<Vec<u8>>,
    /// Asks for the menu again, after the first.
    again: bool,
}

const SEARCH: [(SearchReferenceSpaceType, &str); 3] = [
    (SearchReferenceSpaceType::All, "SEARCH_REFERENCE_SPACE_ALL"),
    (
        SearchReferenceSpaceType::Scene,
        "SEARCH_REFERENCE_SPACE_SCENE",
    ),
    (
        SearchReferenceSpaceType::Display,
        "SEARCH_REFERENCE_SPACE_DISPLAY",
    ),
];

/// The probe inputs of a config: its categories, encodings, roles and the names of its color
/// spaces (inactive ones too) and named transforms, with names it doesn't have.
struct Tokens {
    categories: Vec<Vec<u8>>,
    encodings: Vec<Vec<u8>>,
    roles: Vec<Vec<u8>>,
    names: Vec<Vec<u8>>,
}

fn tokens(config: &Config) -> Tokens {
    let mut categories = BTreeSet::new();
    let mut encodings = BTreeSet::new();
    let mut names = Vec::new();
    let all = SearchReferenceSpaceType::All;
    for i in 0..config.num_color_spaces_with(all, ColorSpaceVisibility::All) {
        let name = config.color_space_name_by_index_with(all, ColorSpaceVisibility::All, i);
        let cs = config.color_space(name).unwrap();
        for c in 0..cs.num_categories() {
            categories.insert(cs.category(c).unwrap().to_vec());
        }
        encodings.insert(cs.encoding().to_vec());
        names.push(name.to_vec());
        for a in 0..cs.num_aliases() {
            names.push(cs.alias(a).to_vec());
        }
    }
    for i in 0..config.num_named_transforms() {
        let nt = config
            .named_transform(config.named_transform_name_by_index(i))
            .unwrap();
        for c in 0..nt.num_categories() {
            categories.insert(nt.category(c).unwrap().to_vec());
        }
        encodings.insert(nt.encoding().to_vec());
        names.push(nt.name().to_vec());
    }
    categories.insert(b"not-a-category".to_vec());
    encodings.insert(b"not-an-encoding".to_vec());
    encodings.remove(&Vec::new());
    let roles: Vec<Vec<u8>> = (0..config.num_roles())
        .map(|i| config.role_name(i).to_vec())
        .collect();
    Tokens {
        categories: categories.into_iter().collect(),
        encodings: encodings.into_iter().collect(),
        roles,
        names,
    }
}

/// A token, perhaps in another case, or with surrounding spaces.
fn vary(rng: &mut Rng, token: &[u8]) -> Vec<u8> {
    match rng.below(6) {
        0 => string_utils::upper(token),
        1 => [b"  ".as_slice(), token, b"\t"].concat(),
        _ => token.to_vec(),
    }
}

/// A comma-separated list of 0 to 3 of `tokens`.
fn list(rng: &mut Rng, tokens: &[Vec<u8>]) -> Vec<u8> {
    let count = rng.below(4);
    let mut out = Vec::new();
    if rng.below(8) == 0 {
        out.extend_from_slice(b" ,");
    }
    for k in 0..count {
        if k > 0 {
            out.extend_from_slice([&b","[..], b", ", b" , ", b",,"][rng.below(4)]);
        }
        let picked = rng_pick(rng, tokens);
        let token = vary(rng, &picked);
        out.extend_from_slice(&token);
    }
    out
}

fn rng_pick(rng: &mut Rng, list: &[Vec<u8>]) -> Vec<u8> {
    rng.pick(list).to_vec()
}

/// `n` distinct cases of the config of `tokens`.
fn cases(tokens: &Tokens, seed: u64, n: usize) -> Vec<Case> {
    let mut rng = Rng(seed);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    while out.len() < n {
        let role = match rng.below(10) {
            0 if !tokens.roles.is_empty() => rng_pick(&mut rng, &tokens.roles),
            1 if !tokens.roles.is_empty() => string_utils::upper(rng.pick(&tokens.roles)),
            2 => b"unknown_role".to_vec(),
            3 => rng_pick(&mut rng, &tokens.names),
            _ => Vec::new(),
        };
        let mut added = Vec::new();
        for _ in 0..[0, 0, 0, 1, 1, 2, 3][rng.below(7)] {
            let name = match rng.below(12) {
                0 if !tokens.roles.is_empty() => rng_pick(&mut rng, &tokens.roles),
                1 => string_utils::lower(rng.pick(&tokens.names)),
                2 => string_utils::upper(rng.pick(&tokens.names)),
                3 if rng.below(4) == 0 => b"doesNotExist".to_vec(),
                _ => rng_pick(&mut rng, &tokens.names),
            };
            added.push(name);
        }
        let case = Case {
            role,
            app: list(&mut rng, &tokens.categories),
            encodings: list(&mut rng, &tokens.encodings),
            user: list(&mut rng, &tokens.categories),
            include_color_spaces: rng.below(6) != 0,
            include_named_transforms: rng.below(2) == 0,
            treat_no_category_as_any: rng.below(2) == 0,
            include_roles: rng.below(3) == 0,
            search: rng.below(3),
            added,
            again: rng.below(6) == 0,
        };
        let mut key = case.clone();
        key.again = false;
        if seen.insert(key) {
            out.push(case);
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The port's side

/// The port's log while a case runs: OCIO's logging function is the process's, so the cases
/// of this file log one at a time.
static LOGGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let me = std::thread::current().id();
    let messages = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        if std::thread::current().id() == me {
            sink.lock().unwrap().push(m.to_vec());
        } else {
            eprint!("{}", String::from_utf8_lossy(m));
        }
    })))
    .unwrap();
    let out = f();
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (out, log)
}

/// What a call gave, as the oracle writes it: `{"result": value}` or `{"exception":
/// {"message": bytes}}`, and its log.
fn outcome(result: Result<Value, ocio::Exception>) -> Value {
    match result {
        Ok(v) => json!({"result": v}),
        Err(e) => json!({"exception": {"message": bytes_arg(e.what())}}),
    }
}

/// A string the port returned, as the oracle writes a `str`.
fn s(v: &[u8]) -> Value {
    Value::String(String::from_utf8(v.to_vec()).expect("UTF-8 probe text"))
}

/// The wheel's calls of a case, and the port's outcomes of the same calls, in order (one
/// entry per call: what it gives, and its log).
struct Run {
    calls: Vec<Value>,
    port: Vec<(Value, Vec<Vec<u8>>)>,
    labels: Vec<String>,
}

impl Run {
    fn new() -> Run {
        Run {
            calls: Vec::new(),
            port: Vec::new(),
            labels: Vec::new(),
        }
    }

    fn push(&mut self, label: String, call: Value, port: Value, log: Vec<Vec<u8>>) {
        self.calls.push(call);
        self.port.push((port, log));
        self.labels.push(label);
    }
}

/// The parameters of `case` on `config`, on both sides, stored as `p`.
fn parameters(
    run: &mut Run,
    config: &Arc<Config>,
    case: &Case,
    p: &str,
) -> ColorSpaceMenuParameters {
    let mut params = ColorSpaceMenuParameters::new(Arc::clone(config));
    // The binding's constructor with keywords (PyColorSpaceHelpers.cpp:54-97 @ v2.5.2):
    // the strings only when not empty.
    if !case.role.is_empty() {
        params.set_role(&case.role);
    }
    if !case.app.is_empty() {
        params.set_app_categories(&case.app);
    }
    if !case.user.is_empty() {
        params.set_user_categories(&case.user);
    }
    if !case.encodings.is_empty() {
        params.set_encodings(&case.encodings);
    }
    params.set_search_reference_space_type(SEARCH[case.search].0);
    params.set_include_color_spaces(case.include_color_spaces);
    params.set_include_roles(case.include_roles);
    params.set_include_named_transforms(case.include_named_transforms);
    params.set_treat_no_category_as_any(case.treat_no_category_as_any);
    run.push(
        format!("{p} = ColorSpaceMenuParameters"),
        json!({"new": "ColorSpaceMenuParameters", "args": [{"ref": "config"}], "kwargs": {
            "role": bytes_arg(&case.role),
            "appCategories": bytes_arg(&case.app),
            "encodings": bytes_arg(&case.encodings),
            "userCategories": bytes_arg(&case.user),
            "includeColorSpaces": case.include_color_spaces,
            "includeNamedTransforms": case.include_named_transforms,
            "treatNoCategoryAsAny": case.treat_no_category_as_any,
            "includeRoles": case.include_roles,
            "searchReferenceSpaceType": {"enum": SEARCH[case.search].1},
        }, "as": p}),
        Value::Null,
        Vec::new(),
    );
    for name in &case.added {
        params.add_color_space(name);
        run.push(
            format!("{p}.addColorSpace"),
            json!({"call": "addColorSpace", "on": p, "args": [bytes_arg(name)]}),
            json!({"result": null}),
            Vec::new(),
        );
    }
    let added: Vec<Value> = (0..params.num_added_color_spaces())
        .map(|i| s(params.added_color_space(i)))
        .collect();
    run.push(
        format!("{p}.getAddedColorSpaces"),
        json!({"call": "getAddedColorSpaces", "on": p}),
        json!({"result": added}),
        Vec::new(),
    );
    for (getter, value) in [
        ("getRole", s(params.role())),
        ("getAppCategories", s(params.app_categories())),
        ("getUserCategories", s(params.user_categories())),
        ("getEncodings", s(params.encodings())),
        (
            "getIncludeColorSpaces",
            json!(params.include_color_spaces()),
        ),
        (
            "getIncludeNamedTransforms",
            json!(params.include_named_transforms()),
        ),
        ("getIncludeRoles", json!(params.include_roles())),
        (
            "getTreatNoCategoryAsAny",
            json!(params.treat_no_category_as_any()),
        ),
        (
            "getSearchReferenceSpaceType",
            json!({"enum": SEARCH.iter().find(|(t, _)| *t == params.search_reference_space_type()).unwrap().1}),
        ),
    ] {
        run.push(
            format!("{p}.{getter}"),
            json!({"call": getter, "on": p}),
            json!({"result": value}),
            Vec::new(),
        );
    }
    let text = params.to_bytes().map(|t| s(&t));
    run.push(
        format!("repr({p})"),
        json!({"call": "__repr__", "on": p}),
        outcome(text),
        Vec::new(),
    );
    params
}

/// The menu of `params`, on both sides, stored as `m`, and its items and lookups when the
/// port makes it (the wheel's request refuses calls on a menu it didn't make, which fails the
/// test as loudly as a mismatch).
fn menu_calls(run: &mut Run, params: &ColorSpaceMenuParameters, p: &str, m: &str) {
    let (made, log) = capture(|| ColorSpaceMenuHelper::new(params));
    let helper = match made {
        Ok(helper) => {
            run.push(
                format!("{m} = ColorSpaceMenuHelper"),
                json!({"new": "ColorSpaceMenuHelper", "args": [{"ref": p}], "as": m}),
                Value::Null,
                log,
            );
            helper
        }
        Err(e) => {
            run.push(
                format!("{m} = ColorSpaceMenuHelper"),
                json!({"new": "ColorSpaceMenuHelper", "args": [{"ref": p}], "as": m}),
                json!({"exception": {"message": bytes_arg(e.what())}}),
                log,
            );
            return;
        }
    };
    let n = helper.num_color_spaces();
    run.push(
        format!("{m}.getNumColorSpaces"),
        json!({"call": "getNumColorSpaces", "on": m}),
        json!({"result": n}),
        Vec::new(),
    );
    run.push(
        format!("repr({m})"),
        json!({"call": "__repr__", "on": m}),
        outcome(helper.to_bytes().map(|t| s(&t))),
        Vec::new(),
    );
    // The menus of these cases have no null item (docs/improvements.md, U-142): the getters
    // return.
    let mut probes = Vec::new();
    for i in 0..=n {
        let levels: Vec<Value> = (0..helper.num_hierarchy_levels(i).unwrap())
            .map(|l| s(helper.hierarchy_level(i, l).unwrap()))
            .collect();
        for (getter, value) in [
            ("getName", s(helper.name(i).unwrap())),
            ("getUIName", s(helper.ui_name(i).unwrap())),
            ("getFamily", s(helper.family(i).unwrap())),
            ("getDescription", s(helper.description(i).unwrap())),
            ("getHierarchyLevels", json!(levels)),
        ] {
            run.push(
                format!("{m}.{getter}({i})"),
                json!({"call": getter, "on": m, "args": [i]}),
                json!({"result": value}),
                Vec::new(),
            );
        }
        probes.push(string_utils::upper(helper.name(i).unwrap()));
        probes.push(string_utils::lower(helper.ui_name(i).unwrap()));
    }
    probes.push(b"unknown".to_vec());
    probes.sort();
    probes.dedup();
    for probe in &probes {
        for (getter, value) in [
            (
                "getIndexFromName",
                json!(helper.index_from_name(probe).unwrap()),
            ),
            (
                "getIndexFromUIName",
                json!(helper.index_from_ui_name(probe).unwrap()),
            ),
            (
                "getNameFromUIName",
                s(helper.name_from_ui_name(probe).unwrap()),
            ),
            (
                "getUINameFromName",
                s(helper.ui_name_from_name(probe).unwrap()),
            ),
        ] {
            run.push(
                format!("{m}.{getter}({})", String::from_utf8_lossy(probe)),
                json!({"call": getter, "on": m, "args": [bytes_arg(probe)]}),
                json!({"result": value}),
                Vec::new(),
            );
        }
    }
}

/// Runs `run`'s calls in the wheel on `source` (after `setup`, in the environment `env`), and
/// compares each with the port's; the mismatches go to `failures`. Returns the wheel's calls.
fn compare(
    label: &str,
    source: &Value,
    setup: &[Value],
    env: Option<&Value>,
    run: &Run,
    failures: &mut Vec<String>,
) -> Vec<Value> {
    let mut calls = setup.to_vec();
    // The menus validate the config, which logs what it finds the first time only.
    calls.push(json!({"call": "validate"}));
    let first = calls.len();
    calls.extend(run.calls.iter().cloned());
    let mut args = json!({"config": source, "calls": calls});
    if let Some(env) = env {
        args["env"] = env.clone();
    }
    let response = Oracle::get().call("object_calls", args, &[]).result;
    assert_eq!(response["config"], Value::Null, "{label}: {response}");
    let results = response["calls"].as_array().unwrap().clone();
    assert_eq!(results.len(), calls.len(), "{label}");
    let mut checked = 0;
    for (k, ((port, port_log), name)) in run.port.iter().zip(&run.labels).enumerate() {
        let wheel = &results[first + k];
        let mut problems = Vec::new();
        if let Some(e) = wheel.get("exception") {
            if port.get("exception").is_none() {
                problems.push(format!("the wheel raised {e}"));
            } else {
                let w = bytes(&e["message"]);
                let p = bytes(&port["exception"]["message"]);
                if w != p {
                    problems.push(format!(
                        "messages {:?} != {:?}",
                        String::from_utf8_lossy(&w),
                        String::from_utf8_lossy(&p)
                    ));
                }
            }
        } else if port.get("exception").is_some() {
            problems.push(format!("the port raised {port}, the wheel gave {wheel}"));
        } else if !port.is_null() && wheel.get("result") != port.get("result") {
            problems.push(format!("{wheel} != {port}"));
        } else if wheel.get("result").is_none() {
            problems.push(format!("the wheel gave {wheel}"));
        }
        let wheel_log = log(&wheel["log"]);
        if &wheel_log != port_log {
            problems.push(format!(
                "log {:?} != {:?}",
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
        if !problems.is_empty() {
            failures.push(format!("{label}: {name}: {}", problems.join("; ")));
        }
        checked += 1;
    }
    assert_eq!(checked, run.calls.len());
    results[first..].to_vec()
}

/// What the cases of a config reached: the distinct menu logs, menus made and refused, and
/// item lists.
#[derive(Default)]
struct Reached {
    logs: BTreeSet<Vec<Vec<u8>>>,
    made: usize,
    refused: usize,
    again: usize,
    menus: BTreeSet<String>,
}

/// Compares the menus of `cases` on the config of `source`, `per_request` cases a request.
fn check_menus(
    test: &str,
    source: &Source,
    cases: &[Case],
    per_request: usize,
    env: Option<(&str, &[u8])>,
    failures: &mut Vec<String>,
) -> Reached {
    let mut reached = Reached::default();
    for (chunk_index, chunk) in cases.chunks(per_request).enumerate() {
        let label = format!("{test} #{chunk_index}");
        let (config, wheel_source, setup) = tagged(source, &label);
        let mut run = Run::new();
        let mut menus = Vec::new();
        {
            let _env = env.map(|(name, value)| {
                set_thread_env_provider(Some(Arc::new(MapEnv::from_entries(&[(name, value)]))));
                Unset
            });
            for (k, case) in chunk.iter().enumerate() {
                let p = format!("p{k}");
                let m = format!("m{k}");
                let params = parameters(&mut run, &config, case, &p);
                let start = run.calls.len();
                menu_calls(&mut run, &params, &p, &m);
                menus.push(start);
                if case.again {
                    menu_calls(&mut run, &params, &p, &format!("{m}again"));
                    reached.again += 1;
                }
            }
        }
        let wheel_env =
            env.map(|(name, value)| json!({ name: String::from_utf8(value.to_vec()).unwrap() }));
        let results = compare(
            &label,
            &wheel_source,
            &setup,
            wheel_env.as_ref(),
            &run,
            failures,
        );
        for &start in &menus {
            let made = &results[start];
            reached.logs.insert(log(&made["log"]));
            if made.get("exception").is_some() {
                reached.refused += 1;
            } else {
                reached.made += 1;
                if let Some(text) = results[start + 2]["result"].as_str() {
                    let items = text.rsplit_once(", color spaces = ").map_or("", |(_, i)| i);
                    reached.menus.insert(items.to_string());
                }
            }
        }
    }
    reached
}

/// Restores the process environment of this thread when dropped.
struct Unset;

impl Drop for Unset {
    fn drop(&mut self) {
        set_thread_env_provider(None);
    }
}

// ---------------------------------------------------------------------------------------------
// The tests

/// Generated menus of upstream's test config, its variants, a config the menus' validation
/// refuses, a version 1 config, and the built-in configs.
#[test]
fn menus_match_the_wheel() {
    let mut failures = Vec::new();
    let mut all = Reached::default();
    let gather = |r: Reached, all: &mut Reached| {
        all.logs.extend(r.logs);
        all.made += r.made;
        all.refused += r.refused;
        all.again += r.again;
        all.menus.extend(r.menus);
    };

    let category = Source::Yaml(CATEGORY_TEST_CONFIG.to_string());
    let category_tokens = tokens(&tagged(&category, "tokens").0);
    let r = check_menus(
        "category",
        &category,
        &cases(&category_tokens, 0xca7e_0001, 240),
        60,
        None,
        &mut failures,
    );
    // The fall-backs' logs, menus of a role, of roles and of added color spaces, refused
    // menus, and menus asked for again.
    assert!(r.logs.len() >= 8, "{} distinct logs", r.logs.len());
    assert!(
        r.made >= 150 && r.refused >= 10,
        "{} made, {} refused",
        r.made,
        r.refused
    );
    assert!(r.again >= 20, "{} asked again", r.again);
    assert!(r.menus.len() >= 60, "{} distinct menus", r.menus.len());
    gather(r, &mut all);

    let no_display = Source::Yaml(NO_DISPLAY_CONFIG.to_string());
    let no_display_tokens = tokens(&tagged(&no_display, "tokens").0);
    let r = check_menus(
        "no display",
        &no_display,
        &cases(&no_display_tokens, 0xca7e_0002, 60),
        60,
        None,
        &mut failures,
    );
    // Every menu logs the validation's warning, the menus asked for again too.
    assert!(r.logs.iter().all(|l| !l.is_empty()), "{:?}", r.logs);
    gather(r, &mut all);

    let v1 = Source::Yaml(V1_CONFIG.to_string());
    let r = check_menus(
        "v1",
        &v1,
        &cases(&tokens(&tagged(&v1, "tokens").0), 0xca7e_0003, 40),
        40,
        None,
        &mut failures,
    );
    gather(r, &mut all);

    let registry = BuiltinConfigRegistry::get();
    assert!(registry.num_builtin_configs() >= 4);
    for i in 0..registry.num_builtin_configs() {
        let name = String::from_utf8(registry.builtin_config_name(i).unwrap().to_vec()).unwrap();
        let builtin = Source::Builtin(name.clone());
        let builtin_tokens = tokens(&tagged(&builtin, "tokens").0);
        let r = check_menus(
            &name,
            &builtin,
            &cases(&builtin_tokens, 0xca7e_0100 + i as u64, 40),
            20,
            None,
            &mut failures,
        );
        assert!(
            r.menus.len() >= 10,
            "{name}: {} distinct menus",
            r.menus.len()
        );
        gather(r, &mut all);
    }
    assert!(all.made >= 300 && all.refused >= 20);

    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `OCIO_USER_CATEGORIES` replaces the user categories when it isn't blank once trimmed.
#[test]
fn user_categories_from_the_environment() {
    let mut failures = Vec::new();
    let category = Source::Yaml(CATEGORY_TEST_CONFIG.to_string());
    let category_tokens = tokens(&tagged(&category, "tokens").0);
    let mut distinct = BTreeSet::new();
    for (k, value) in [
        &b"basic-3d"[..],
        b"  advanced-2d , BASIC-3D \t",
        b"   ",
        b"not-a-category",
        b"file-io,working-space",
    ]
    .iter()
    .enumerate()
    {
        let r = check_menus(
            &format!("env {k}"),
            &category,
            &cases(&category_tokens, 0xe4f0_0000 + k as u64, 30),
            30,
            Some(("OCIO_USER_CATEGORIES", value)),
            &mut failures,
        );
        distinct.insert(r.menus.into_iter().collect::<Vec<_>>());
    }
    // The variable changes the menus.
    assert_eq!(distinct.len(), 5);
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Parameters asked for again give the menu made first, which logs nothing then; another
/// config's cache ID, or other parameters, make another.
#[test]
fn menus_are_kept_by_their_parameters() {
    let mut failures = Vec::new();
    let category = Source::Yaml(CATEGORY_TEST_CONFIG.to_string());
    let (config, wheel_source, setup) = tagged(&category, "kept");
    let mut run = Run::new();
    let case = Case {
        role: Vec::new(),
        app: b"unknown_category".to_vec(),
        encodings: Vec::new(),
        user: Vec::new(),
        include_color_spaces: true,
        include_named_transforms: false,
        treat_no_category_as_any: false,
        include_roles: false,
        search: 0,
        added: Vec::new(),
        again: false,
    };
    let mut other = case.clone();
    other.user = b"basic-3d".to_vec();
    let p = parameters(&mut run, &config, &case, "p");
    let q = parameters(&mut run, &config, &other, "q");
    let first = run.calls.len();
    menu_calls(&mut run, &p, "p", "m1");
    let second = run.calls.len();
    menu_calls(&mut run, &p, "p", "m2");
    let third = run.calls.len();
    menu_calls(&mut run, &q, "q", "m3");
    menu_calls(&mut run, &p, "p", "m4");
    let results = compare("kept", &wheel_source, &setup, None, &run, &mut failures);
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The wheel logs the first menu only.
    assert_eq!(log(&results[first]["log"]).len(), 1);
    assert!(log(&results[second]["log"]).is_empty());
    assert_eq!(log(&results[third]["log"]).len(), 1);
}

/// A role that names a missing color space: the wheel's process dies while it makes the menu
/// of that role, or with the roles included, where the port refuses the menu
/// (docs/improvements.md, U-141).
#[test]
fn roles_naming_a_missing_color_space_are_refused() {
    let base = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    let mut missing = (*base).clone();
    missing.set_description("U-141");
    missing
        .set_role("badrole", Some(&b"nonexistent"[..]))
        .unwrap();
    let missing = Arc::new(missing);
    assert!(missing.has_role("badrole"));
    let mut params = ColorSpaceMenuParameters::new(Arc::clone(&missing));
    params.set_role("badrole");
    let error = ColorSpaceMenuHelper::new(&params).unwrap_err();
    assert!(error.to_string().contains("U-141"), "{error}");
    params.set_role("");
    params.set_include_roles(true);
    let error = ColorSpaceMenuHelper::new(&params).unwrap_err();
    assert!(error.to_string().contains("U-141"), "{error}");
    params.set_include_roles(false);
    assert!(ColorSpaceMenuHelper::new(&params).is_ok());
}

/// A role set to `""`, with the roles included: the menu holds a null item, as the wheel's,
/// and what reads it is an error in the port, where the wheel's process dies
/// (docs/improvements.md, U-142).
///
/// The wheel can't be asked about that menu through the oracle: writing the menu out calls its
/// getters without arguments, and pybind11's TypeError message holds the menu's `repr`, which
/// reads the null item (the oracle's process died, 2026-10-22). What doesn't make that menu is
/// compared: the parameters, and an added color space the config doesn't have, which fails
/// before the items are read.
#[test]
fn a_role_set_to_the_empty_name_gives_a_null_item() {
    let mut failures = Vec::new();
    let tag = "U-142";
    let mut config =
        (*Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap()).clone();
    config.set_description(tag);
    config.set_role("emptyrole", Some(&b""[..])).unwrap();
    let config = Arc::new(config);
    assert!(!config.has_role("emptyrole"));
    let _ = capture(|| config.validate());
    let setup = vec![
        json!({"call": "setDescription", "args": [tag]}),
        json!({"call": "setRole", "args": ["emptyrole", ""]}),
    ];
    let source = json!({"yaml": CATEGORY_TEST_CONFIG});

    let case = Case {
        role: Vec::new(),
        app: b"file-io".to_vec(),
        encodings: Vec::new(),
        user: Vec::new(),
        include_color_spaces: true,
        include_named_transforms: false,
        treat_no_category_as_any: false,
        include_roles: true,
        search: 0,
        added: Vec::new(),
        again: false,
    };

    // The port's menu: one null item (the role's), whose getters fail; the other items, and
    // the lookups that stop before it, return.
    let mut scratch = Run::new();
    let params = parameters(&mut scratch, &config, &case, "p");
    let helper = ColorSpaceMenuHelper::new(&params).unwrap();
    let n = helper.num_color_spaces();
    let nulls: Vec<usize> = (0..n).filter(|&i| helper.name(i).is_err()).collect();
    assert_eq!(nulls.len(), 1, "{nulls:?}");
    let null = nulls[0];
    for check in [
        helper.ui_name(null).err(),
        helper.family(null).err(),
        helper.description(null).err(),
        helper.num_hierarchy_levels(null).err(),
        helper.hierarchy_level(null, 0).err(),
        helper.to_bytes().err(),
        helper.index_from_name("unknown").err(),
        helper.index_from_ui_name("unknown").err(),
        helper.name_from_ui_name("unknown").err(),
        helper.ui_name_from_name("unknown").err(),
    ] {
        let error = check.expect("an error");
        assert!(error.to_string().contains("U-142"), "{error}");
    }
    assert!(null >= 3);
    for i in (0..=n).filter(|&i| i != null) {
        assert!(helper.name(i).is_ok() && helper.hierarchy_level(i, 0).is_ok());
    }
    for i in 0..null {
        let name = string_utils::upper(helper.name(i).unwrap());
        let ui_name = string_utils::upper(helper.ui_name(i).unwrap());
        assert_eq!(helper.index_from_name(&name).unwrap(), i);
        assert_eq!(helper.index_from_ui_name(&ui_name).unwrap(), i);
    }

    // Added color spaces: one the config doesn't have fails before the items are read, as the
    // wheel's (compared); one the search finds before the null item makes the menu; one it
    // doesn't find before it is an error.
    let mut run = Run::new();
    let missing = Case {
        added: vec![b"doesNotExist".to_vec()],
        ..case.clone()
    };
    let q = parameters(&mut run, &config, &missing, "q");
    let (made, made_log) = capture(|| ColorSpaceMenuHelper::new(&q));
    let error = made.unwrap_err();
    run.push(
        "ColorSpaceMenuHelper(q)".to_string(),
        json!({"new": "ColorSpaceMenuHelper", "args": [{"ref": "q"}], "as": "m"}),
        json!({"exception": {"message": bytes_arg(error.what())}}),
        made_log,
    );
    compare(tag, &source, &setup, None, &run, &mut failures);
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );

    for (added, refused) in [(&b"IN_1"[..], false), (b"lin_1", true)] {
        let mut scratch = Run::new();
        let with_added = Case {
            added: vec![added.to_vec()],
            ..case.clone()
        };
        let q = parameters(&mut scratch, &config, &with_added, "q");
        match ColorSpaceMenuHelper::new(&q) {
            Ok(menu) => {
                assert!(!refused);
                assert_eq!(menu.num_color_spaces(), n);
            }
            Err(error) => {
                assert!(refused, "{error}");
                assert!(error.to_string().contains("U-142"), "{error}");
            }
        }
    }
}

/// The arguments of `AddColorSpace` after the config.
type AddCase<'a> = (&'a [u8], &'a [u8], &'a [u8], &'a [u8]);

/// `ColorSpaceHelpers.AddColorSpace`: the config it leaves (its YAML text) or its exception,
/// on upstream's test config and a built-in config.
#[test]
fn add_color_space_matches_the_wheel() {
    let mut failures = Vec::new();
    let category = Source::Yaml(CATEGORY_TEST_CONFIG.to_string());
    // (name, transform file, categories, connection color space)
    let cases: &[AddCase] = &[
        (b"added", b"lut.spi1d", b"file-io", b"lin_1"),
        (b"added", b"lut.spi1d", b"not-used-cat", b"lin_1"),
        (b"added", b"lut.spi1d", b"  File-IO , other ,", b"log_1"),
        (b"added", b"lut.spi1d", b"", b"view_1"),
        (b"added", b"lut.spi1d", b"", b"view_2"),
        (b"added", b"lut.spi1d", b"working-space", b"lin_2"),
        (b"added", b"lut.spi1d", b"", b"display_log_1"),
        (b"added", b"lut.spi1d", b"", b"rendering"),
        (b"added", b"lut.spi1d", b"", b"LIN_1"),
        (b"in_1", b"lut.spi1d", b"", b"lin_1"),
        (b"IN_1", b"lut.spi1d", b"", b"lin_1"),
        (b"rendering", b"lut.spi1d", b"", b"lin_1"),
        (b"nt1", b"lut.spi1d", b"", b"lin_1"),
        (b"", b"lut.spi1d", b"", b"lin_1"),
        (b"$var", b"lut.spi1d", b"", b"lin_1"),
        (b"x", b"", b"", b"lin_1"),
        (b"x", b"lut.spi1d", b"", b""),
        (b"x", b"lut.spi1d", b"", b"missing"),
        (b"x", b"lut.spi1d", b"", b"nt1"),
    ];
    let (config, wheel_source, setup) = tagged(&category, "add");
    let mut run = Run::new();
    let mut raised = 0;
    for (k, &(name, path, categories, connection)) in cases.iter().enumerate() {
        let c = format!("c{k}");
        run.push(
            format!("{c} = copy"),
            json!({"copy": "config", "as": c}),
            Value::Null,
            Vec::new(),
        );
        let mut port = (*config).clone();
        let added = add_color_space(&mut port, name, path, categories, connection);
        raised += usize::from(added.is_err());
        run.push(
            format!("AddColorSpace #{k}"),
            json!({"call": "AddColorSpace", "on": "ColorSpaceHelpers", "args": [
                {"ref": c}, bytes_arg(name), bytes_arg(path), bytes_arg(categories),
                bytes_arg(connection)]}),
            outcome(added.map(|()| Value::Null)),
            Vec::new(),
        );
        run.push(
            format!("{c}.serialize #{k}"),
            json!({"call": "serialize", "on": c}),
            outcome(port.serialize().map(|t| s(&t))),
            Vec::new(),
        );
    }
    assert!(raised >= 8 && raised < cases.len(), "{raised} raised");
    compare("add", &wheel_source, &setup, None, &run, &mut failures);

    // A built-in config: categories it uses, and a connection space it has.
    let studio = Source::Builtin("studio-config-v2.1.0_aces-v1.3_ocio-v2.3".to_string());
    let (config, wheel_source, setup) = tagged(&studio, "add studio");
    let mut run = Run::new();
    for (k, &(categories, connection)) in [
        (&b"file-io"[..], &b"ACEScg"[..]),
        (b"file-io, texture", b"Linear Rec.709 (sRGB)"),
        (b"", b"sRGB - Display"),
        (b"working-space", b"scene_linear"),
    ]
    .iter()
    .enumerate()
    {
        let c = format!("c{k}");
        run.push(
            format!("{c} = copy"),
            json!({"copy": "config", "as": c}),
            Value::Null,
            Vec::new(),
        );
        let mut port = (*config).clone();
        let added = add_color_space(&mut port, b"Added", b"lut.spi1d", categories, connection);
        run.push(
            format!("AddColorSpace #{k}"),
            json!({"call": "AddColorSpace", "on": "ColorSpaceHelpers", "args": [
                {"ref": c}, "Added", "lut.spi1d", bytes_arg(categories), bytes_arg(connection)]}),
            outcome(added.map(|()| Value::Null)),
            Vec::new(),
        );
        run.push(
            format!("{c}.serialize #{k}"),
            json!({"call": "serialize", "on": c}),
            outcome(port.serialize().map(|t| s(&t))),
            Vec::new(),
        );
    }
    compare(
        "add studio",
        &wheel_source,
        &setup,
        None,
        &run,
        &mut failures,
    );

    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
