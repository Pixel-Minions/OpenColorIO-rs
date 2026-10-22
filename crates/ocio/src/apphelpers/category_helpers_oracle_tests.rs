// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `FindColorSpaceInfos` and `ExtractItems` against the wheel, through the oracle's
//! `object_calls`: the wheel has no binding of its own for them, but its
//! `ColorSpaceMenuHelper`, without a role, roles or added color spaces, is the result of
//! `FindColorSpaceInfos` on the parameters' lists, which `ExtractItems` splits
//! (`ColorSpaceMenuHelperImpl::refresh`, ColorSpaceHelpers.cpp:481-498 @ v2.5.2), and its log
//! is the one `FindColorSpaceInfos` writes. Each case compares the items' names (and, for one
//! case in four, their UI names, families, descriptions and hierarchy levels) and the log.
//!
//! The wheel keeps the menus it makes, by the text of their parameters, and logs only when it
//! makes one: every case of a request has its own parameters, and each request runs in its own
//! process.

use super::*;
use crate::apphelpers::configs_data::CATEGORY_TEST_CONFIG;
use crate::builtinconfigs::builtin_config_registry::BuiltinConfigRegistry;
use ocio_testkit::Oracle;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

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

    fn flip(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

/// A comma-separated list of 0 to 3 of `tokens`, each perhaps in another case or with
/// surrounding spaces, with the separators upstream's `ExtractItems` must cope with.
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
        let mut token = tokens[rng.below(tokens.len())].clone();
        match rng.below(6) {
            0 => token = string_utils::upper(&token),
            1 => token = [b"  ".as_slice(), &token, b"\t"].concat(),
            _ => {}
        }
        out.extend_from_slice(&token);
    }
    if rng.below(8) == 0 {
        out.extend_from_slice(b", ");
    }
    out
}

/// The parameters of a case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Case {
    app: Vec<u8>,
    encodings: Vec<u8>,
    user: Vec<u8>,
    include_color_spaces: bool,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    search: u8,
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

/// The categories and encodings of every color space and named transform of `config` (the
/// inactive ones too), with tokens no item has: the probe inputs.
fn tokens(config: &Config) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut cats = BTreeSet::new();
    let mut encs = BTreeSet::new();
    for i in
        0..config.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All)
    {
        let name = config.color_space_name_by_index_with(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::All,
            i,
        );
        let cs = config.color_space(name).unwrap();
        for c in 0..cs.num_categories() {
            cats.insert(cs.category(c).unwrap().to_vec());
        }
        encs.insert(cs.encoding().to_vec());
    }
    for i in 0..config.num_named_transforms() {
        let nt = config
            .named_transform(config.named_transform_name_by_index(i))
            .unwrap();
        for c in 0..nt.num_categories() {
            cats.insert(nt.category(c).unwrap().to_vec());
        }
        encs.insert(nt.encoding().to_vec());
    }
    cats.insert(b"not-a-category".to_vec());
    encs.insert(b"not-an-encoding".to_vec());
    encs.remove(&Vec::new());
    (cats.into_iter().collect(), encs.into_iter().collect())
}

/// `n` distinct cases for `config`.
fn cases(config: &Config, seed: u64, n: usize) -> Vec<Case> {
    let (cats, encs) = tokens(config);
    let mut rng = Rng(seed);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    while out.len() < n {
        let case = Case {
            app: list(&mut rng, &cats),
            encodings: list(&mut rng, &encs),
            user: list(&mut rng, &cats),
            include_color_spaces: rng.below(5) != 0,
            include_named_transforms: rng.flip(),
            treat_no_category_as_any: rng.flip(),
            search: rng.below(3) as u8,
        };
        if seen.insert(case.clone()) {
            out.push(case);
        }
    }
    out
}

/// A JSON string of bytes for the oracle: `{"bytes": hex}`.
fn arg(bytes: &[u8]) -> Value {
    ocio_testkit::oracle_values::bytes_arg(bytes)
}

/// The text of a string the wheel returned.
fn text(v: &Value) -> Vec<u8> {
    v["result"]
        .as_str()
        .unwrap_or_else(|| panic!("not a string result: {v}"))
        .as_bytes()
        .to_vec()
}

/// The port's infos and log for a case, as `ColorSpaceMenuHelperImpl::refresh` asks
/// `FindColorSpaceInfos` for them (ColorSpaceHelpers.cpp:483-498 @ v2.5.2).
fn port(config: &Config, case: &Case) -> (Infos, Vec<Vec<u8>>) {
    let search = SEARCH[case.search as usize].0;
    crate::test_env::capture_log(|| {
        let app = extract_items(Some(&case.app));
        let user = extract_items(Some(&case.user));
        let encodings = extract_items(Some(&case.encodings));
        let num_nt = config.num_named_transforms();
        let num_cs = config.num_color_spaces_with(search, ColorSpaceVisibility::Active);
        if (case.include_color_spaces && num_cs != 0)
            || (case.include_named_transforms && num_nt != 0)
        {
            find_color_space_infos(
                config,
                &app,
                &user,
                case.include_color_spaces,
                case.include_named_transforms,
                case.treat_no_category_as_any,
                &encodings,
                search,
            )
        } else {
            Infos::new()
        }
    })
}

/// Distinct logs or item lists.
type Seen = BTreeSet<Vec<Vec<u8>>>;

/// Compares `cases` on `config` (from `source`, as `object_calls` takes it) with the wheel;
/// returns the distinct logs and item lists seen, for the callers' checks that the cases
/// reach the paths they are meant to.
fn check(
    label: &str,
    source: Value,
    config: &Arc<Config>,
    cases: &[Case],
    failures: &mut Vec<String>,
) -> (Seen, Seen) {
    let max = (config
        .num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::Active)
        + config.num_named_transforms()) as usize;
    // The menu helper validates the config, which logs what it finds the first time only: once
    // here, so that the menus' logs are their own.
    let mut calls = vec![json!({"call": "validate"})];
    let mut layout = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let detail = k % 4 == 0;
        let first = calls.len();
        calls.push(
            json!({"new": "ColorSpaceMenuParameters", "args": [{"ref": "config"}],
            "kwargs": {
                "appCategories": arg(&case.app),
                "encodings": arg(&case.encodings),
                "userCategories": arg(&case.user),
                "includeColorSpaces": case.include_color_spaces,
                "includeNamedTransforms": case.include_named_transforms,
                "treatNoCategoryAsAny": case.treat_no_category_as_any,
                "searchReferenceSpaceType": {"enum": SEARCH[case.search as usize].1},
            }, "as": "p"}),
        );
        calls.push(json!({"new": "ColorSpaceMenuHelper", "args": [{"ref": "p"}], "as": "m"}));
        calls.push(json!({"call": "getNumColorSpaces", "on": "m"}));
        // One past the largest menu: an index outside the items gives "".
        for i in 0..=max {
            calls.push(json!({"call": "getName", "on": "m", "args": [i]}));
            if detail {
                calls.push(json!({"call": "getUIName", "on": "m", "args": [i]}));
                calls.push(json!({"call": "getFamily", "on": "m", "args": [i]}));
                calls.push(json!({"call": "getDescription", "on": "m", "args": [i]}));
                calls.push(json!({"call": "getHierarchyLevels", "on": "m", "args": [i]}));
            }
        }
        layout.push((first, detail));
    }
    let response = Oracle::get()
        .call(
            "object_calls",
            json!({"config": source, "calls": calls}),
            &[],
        )
        .result;
    assert_eq!(response["config"], Value::Null, "{label}: {response}");
    let results = response["calls"].as_array().unwrap();
    assert_eq!(results.len(), calls.len(), "{label}");
    assert!(
        results[0].get("result").is_some(),
        "{label}: {}",
        results[0]
    );

    let mut logs = BTreeSet::new();
    let mut lists = BTreeSet::new();
    for (case, &(first, detail)) in cases.iter().zip(&layout) {
        let (infos, log) = port(config, case);
        let wheel_log = ocio_testkit::oracle_values::log(&results[first + 1]["log"]);
        let num = results[first + 2]["result"]
            .as_u64()
            .unwrap_or_else(|| panic!("{label}: {}", results[first + 2]));
        let mut at = first + 3;
        let mut wheel_names = Vec::new();
        let mut mismatch = Vec::new();
        for i in 0..=max {
            let name = text(&results[at]);
            at += 1;
            let info = infos.get(i);
            if name != info.map_or(&[][..], ColorSpaceInfo::name) {
                mismatch.push(format!("name {i}"));
            }
            if i < num as usize {
                wheel_names.push(name);
            }
            if detail {
                let fields = [
                    (
                        text(&results[at]),
                        info.map_or(&[][..], ColorSpaceInfo::ui_name),
                        "UI name",
                    ),
                    (
                        text(&results[at + 1]),
                        info.map_or(&[][..], ColorSpaceInfo::family),
                        "family",
                    ),
                    (
                        text(&results[at + 2]),
                        info.map_or(&[][..], ColorSpaceInfo::description),
                        "description",
                    ),
                ];
                for (wheel, port, what) in fields {
                    if wheel != port {
                        mismatch.push(format!("{what} {i}"));
                    }
                }
                let levels: Vec<Vec<u8>> = results[at + 3]["result"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{label}: {}", results[at + 3]))
                    .iter()
                    .map(|v| v.as_str().unwrap().as_bytes().to_vec())
                    .collect();
                let port_levels: Vec<Vec<u8>> = info.map_or_else(Vec::new, |info| {
                    (0..info.num_hierarchy_levels())
                        .map(|l| info.hierarchy_level(l).to_vec())
                        .collect()
                });
                if levels != port_levels {
                    mismatch.push(format!("levels {i}"));
                }
                at += 4;
            }
        }
        if num as usize != infos.len() {
            mismatch.push(format!("count {num} != {}", infos.len()));
        }
        if wheel_log != log {
            mismatch.push(format!(
                "log {:?} != {:?}",
                wheel_log
                    .iter()
                    .map(|m| String::from_utf8_lossy(m))
                    .collect::<Vec<_>>(),
                log.iter()
                    .map(|m| String::from_utf8_lossy(m))
                    .collect::<Vec<_>>()
            ));
        }
        if !mismatch.is_empty() {
            failures.push(format!("{label} {case:?}: {}", mismatch.join("; ")));
        }
        logs.insert(wheel_log);
        lists.insert(wheel_names);
    }
    (logs, lists)
}

/// `FindColorSpaceInfos` on generated categories, encodings and flags, against the wheel's
/// menus: upstream's test config, a version 1 config (no categories) and the built-in
/// configs.
#[test]
fn find_color_space_infos_matches_the_wheel() {
    let mut failures = Vec::new();
    let mut all_logs = BTreeSet::new();
    let mut total_lists = 0;

    let category = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    let category_cases = cases(&category, 0x5eed_0001, 240);
    let (logs, lists) = check(
        "category_test_config",
        json!({"yaml": CATEGORY_TEST_CONFIG}),
        &category,
        &category_cases,
        &mut failures,
    );
    // Every fall-back of the algorithm logs its own message; all are reached.
    assert!(logs.len() >= 8, "{} distinct logs: {logs:?}", logs.len());
    assert!(lists.len() >= 40, "{} distinct menus", lists.len());
    all_logs.extend(logs);
    total_lists += lists.len();

    // The same config with categories and encodings in mixed case: both compare ignoring it.
    let mixed_case = CATEGORY_TEST_CONFIG
        .replace(
            "encoding: sdr-video\n    from_reference",
            "encoding: SDR-Video\n    from_reference",
        )
        .replace(
            "encoding: log\n    to_reference",
            "encoding: Log\n    to_reference",
        )
        .replace("[ file-io, basic-2d ]", "[ File-IO, Basic-2D ]");
    assert_eq!(mixed_case.matches("SDR-Video").count(), 2);
    assert_eq!(mixed_case.matches("encoding: Log").count(), 1);
    assert_eq!(mixed_case.matches("File-IO").count(), 2);
    let mixed = Config::create_from_stream(mixed_case.as_bytes()).unwrap();
    let mixed_cases = cases(&category, 0x5eed_0003, 120);
    let (logs, lists) = check(
        "mixed case",
        json!({"yaml": mixed_case}),
        &mixed,
        &mixed_cases,
        &mut failures,
    );
    all_logs.extend(logs);
    total_lists += lists.len();

    let v1 = "ocio_profile_version: 1\n\nroles:\n  default: raw\n\ndisplays:\n  DISP_1:\n    \
              - !<View> {name: VIEW_1, colorspace: test_1}\n\ncolorspaces:\n  - !<ColorSpace>\n    \
              name: raw\n    family: Raw\n    isdata: true\n\n  - !<ColorSpace>\n    name: test_1\n    \
              family: a/ b /c\n";
    let v1_config = Config::create_from_stream(v1.as_bytes()).unwrap();
    let mut v1_cases = cases(&category, 0x5eed_0002, 30);
    v1_cases
        .iter_mut()
        .for_each(|c| c.include_color_spaces = true);
    v1_cases.sort();
    v1_cases.dedup();
    let (logs, _) = check(
        "v1",
        json!({"yaml": v1}),
        &v1_config,
        &v1_cases,
        &mut failures,
    );
    // A version 1 config has no categories: nothing is logged.
    assert_eq!(
        logs.into_iter().collect::<Vec<_>>(),
        [Vec::<Vec<u8>>::new()]
    );

    let registry = BuiltinConfigRegistry::get();
    assert!(registry.num_builtin_configs() >= 4);
    for i in 0..registry.num_builtin_configs() {
        let name = registry.builtin_config_name(i).unwrap().to_vec();
        let config = Config::create_from_builtin_config(&name).unwrap();
        let builtin_cases = cases(&config, 0x5eed_0100 + i as u64, 60);
        let label = String::from_utf8(name.clone()).unwrap();
        let (logs, lists) = check(
            &label,
            json!({"builtin": label}),
            &config,
            &builtin_cases,
            &mut failures,
        );
        assert!(lists.len() >= 10, "{label}: {} distinct menus", lists.len());
        all_logs.extend(logs);
        total_lists += lists.len();
    }
    assert!(all_logs.len() >= 8);
    assert!(total_lists >= 100);

    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
