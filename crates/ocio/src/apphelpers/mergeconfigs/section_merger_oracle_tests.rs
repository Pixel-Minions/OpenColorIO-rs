// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The general and roles sections (9.2d) against the wheel. The wheel merges only whole configs
//! (`ConfigMergingHelpers.MergeConfigs`), so it runs every section, while the port runs the two
//! of 9.2d; the whole merge is 9.2j's. What only these two sections change is compared: the
//! merged config's name, description, version and luma coefficients, its roles (no other
//! section sets a role, SectionMerger.cpp @ v2.5.2), the exception when the merge raised, and
//! the messages these two sections log (the roles' conflicts, the strategies a section doesn't
//! support), in order. The other sections take the base's (`STRATEGY_BASE_ONLY`), without the
//! duplicate search or the reference-space conversion, so that they neither raise nor change
//! these.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, log};
use serde_json::{Value, json};

use super::*;

/// A base and an input config, as YAML.
struct Pair {
    label: &'static str,
    base: &'static str,
    input: &'static str,
}

/// Base and input configs: roles that the input changes, adds, or names as a color space, an
/// alias, a named transform or its alias of the base; versions 1 and 2 on either side; a v1
/// input role with `$`, which a v2 merged config refuses.
const PAIRS: &[Pair] = &[
    Pair {
        label: "conflicts",
        base: r#"ocio_profile_version: 2

name: base0
description: My description 1
luma: [0.2126, 0.7152, 0.0722]

roles:
  aces_interchange: ACES2065-1
  default: Raw
  texture_paint: ACEScct
  data: Raw

named_transforms:
  - !<NamedTransform>
    name: nt_base
    aliases: [nt_alias]
    transform: !<MatrixTransform> {}

colorspaces:
  - !<ColorSpace>
    name: ACES2065-1
  - !<ColorSpace>
    name: ACEScct
  - !<ColorSpace>
    name: Raw
    isdata: true
  - !<ColorSpace>
    name: Gamma 2.2 AP1 - Texture
    aliases: [g22_ap1_tx]
"#,
        input: r#"ocio_profile_version: 2.1

name: input0
description: My description 2
luma: [0.2627, 0.677998, 0.059301]

roles:
  aces_interchange: aces2065-1
  texture_paint: ACEScct - Other
  matte_paint: sRGB - Texture
  g22_ap1_tx: sRGB - Texture
  gamma 2.2 ap1 - texture: sRGB - Texture
  nt_base: sRGB - Texture
  NT_ALIAS: sRGB - Texture
  Data: RAW
  default: sRGB - Texture

colorspaces:
  - !<ColorSpace>
    name: ACEScct - Other
  - !<ColorSpace>
    name: sRGB - Texture
"#,
    },
    Pair {
        label: "v1 and v1",
        base: r#"ocio_profile_version: 1

name: base1
luma: [0.25, 0.5, 0.25]

roles:
  default: a
  reference: a

colorspaces:
  - !<ColorSpace>
    name: a
"#,
        input: r#"ocio_profile_version: 1

description: input1

roles:
  default: b
  rendering: b

colorspaces:
  - !<ColorSpace>
    name: b
"#,
    },
    Pair {
        label: "v2.3 base, v1 input with $",
        base: r#"ocio_profile_version: 2.3

name: base2

roles:
  default: a

colorspaces:
  - !<ColorSpace>
    name: a
"#,
        input: r#"ocio_profile_version: 1

name: input2

roles:
  default: a
  x$y: b

colorspaces:
  - !<ColorSpace>
    name: b
"#,
    },
    Pair {
        label: "v2 base, v1 input",
        base: r#"ocio_profile_version: 2

roles:
  default: a
  data: a

colorspaces:
  - !<ColorSpace>
    name: a
"#,
        input: r#"ocio_profile_version: 1

name: input3
description: from input 3
luma: [0.3, 0.3, 0.4]

roles:
  DEFAULT: b
  Data: A

colorspaces:
  - !<ColorSpace>
    name: b
"#,
    },
];

/// The strategies, by the wheel's names, with `None` for none set (unspecified).
const STRATEGIES: &[(Option<&str>, MergeStrategies)] = &[
    (None, MergeStrategies::Unspecified),
    (Some("STRATEGY_PREFER_INPUT"), MergeStrategies::PreferInput),
    (Some("STRATEGY_PREFER_BASE"), MergeStrategies::PreferBase),
    (Some("STRATEGY_INPUT_ONLY"), MergeStrategies::InputOnly),
    (Some("STRATEGY_BASE_ONLY"), MergeStrategies::BaseOnly),
    (Some("STRATEGY_REMOVE"), MergeStrategies::Remove),
    (Some("STRATEGY_UNSPECIFIED"), MergeStrategies::Unspecified),
];

/// One merge: a pair, the default and roles strategies (indices into [`STRATEGIES`]), the
/// error-on-conflict option, and the name and description overrides.
#[derive(Debug, Clone, Copy)]
struct Case {
    pair: usize,
    default: usize,
    roles: usize,
    error_on_conflict: bool,
    overrides: bool,
}

/// Every pair with every default and roles strategy, each with and without error on conflict,
/// the overrides on every other one.
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pair in 0..PAIRS.len() {
        for default in 0..STRATEGIES.len() {
            for roles in 0..STRATEGIES.len() {
                for error_on_conflict in [false, true] {
                    let overrides = (default + roles + usize::from(error_on_conflict)) % 2 == 0;
                    cases.push(Case {
                        pair,
                        default,
                        roles,
                        error_on_conflict,
                        overrides,
                    });
                }
            }
        }
    }
    cases
}

/// The setters of the sections' strategies other than the general and roles sections'.
const OTHER_SECTIONS: &[&str] = &[
    "setFileRules",
    "setDisplayViews",
    "setViewTransforms",
    "setLooks",
    "setColorspaces",
    "setNamedTransforms",
];

const NAME_OVERRIDE: &str = "merged name";
const DESCRIPTION_OVERRIDE: &str = "merged description";

/// The wheel's calls of a case: the configs, the parameters, `MergeConfigs`, then the merged
/// config's values.
fn calls(case: &Case) -> Vec<Value> {
    let pair = &PAIRS[case.pair];
    let mut calls = vec![
        // A config stored as "merged" first, so that the calls after a merge that raises act
        // on it (and are then not compared).
        json!({"call": "CreateFromStream", "on": "Config", "args": [pair.base], "as": "merged"}),
        json!({"call": "CreateFromStream", "on": "Config", "args": [pair.base], "as": "base"}),
        json!({"call": "CreateFromStream", "on": "Config", "args": [pair.input], "as": "input"}),
        json!({"call": "Create", "on": "ConfigMergingParameters", "as": "p"}),
        // Neither the duplicate search nor the reference-space heuristics, which these small
        // configs don't need.
        json!({"call": "setAvoidDuplicates", "on": "p", "args": [false]}),
        json!({"call": "setAdjustInputReferenceSpace", "on": "p", "args": [false]}),
        json!({"call": "setErrorOnConflict", "on": "p", "args": [case.error_on_conflict]}),
    ];
    // The other sections keep the base's (their strategies' BaseOnly handlers change nothing
    // these configs have, SectionMerger.cpp @ v2.5.2), so that they neither raise nor log.
    for setter in OTHER_SECTIONS {
        calls.push(json!({"call": setter, "on": "p",
                          "args": [{"enum": "ConfigMergingParameters.STRATEGY_BASE_ONLY"}]}));
    }
    if let Some(name) = STRATEGIES[case.default].0 {
        let value = format!("ConfigMergingParameters.{name}");
        calls.push(json!({"call": "setDefaultStrategy", "on": "p", "args": [{"enum": value}]}));
    }
    if let Some(name) = STRATEGIES[case.roles].0 {
        let value = format!("ConfigMergingParameters.{name}");
        calls.push(json!({"call": "setRoles", "on": "p", "args": [{"enum": value}]}));
    }
    if case.overrides {
        calls.push(json!({"call": "setName", "on": "p", "args": [NAME_OVERRIDE]}));
        calls.push(json!({"call": "setDescription", "on": "p", "args": [DESCRIPTION_OVERRIDE]}));
    }
    calls.extend([
        json!({"call": "MergeConfigs", "on": "ConfigMergingHelpers",
               "args": [{"ref": "p"}, {"ref": "base"}, {"ref": "input"}], "as": "merged"}),
        json!({"call": "getName", "on": "merged"}),
        json!({"call": "getDescription", "on": "merged"}),
        json!({"call": "getMajorVersion", "on": "merged"}),
        json!({"call": "getMinorVersion", "on": "merged"}),
        json!({"call": "getDefaultLumaCoefs", "on": "merged"}),
        json!({"call": "getRoles", "on": "merged"}),
    ]);
    calls
}

/// Whether a message of the wheel's merge is one of the general or roles sections'.
fn is_section_message(message: &[u8]) -> bool {
    const PREFIXES: &[&[u8]] = &[
        b"[OpenColorIO Warning]: The Input config contains a role",
        b"[OpenColorIO Warning]: General section does not support strategy",
        b"[OpenColorIO Warning]: Roles section does not support strategy",
    ];
    PREFIXES.iter().any(|p| message.starts_with(p))
}

/// What the port's general and roles mergers give for a case: the merge's outcome, the merged
/// config, and the log.
fn port_merge(case: &Case) -> (Result<()>, Config, Vec<u8>) {
    let pair = &PAIRS[case.pair];
    let base = Config::create_from_stream(pair.base.as_bytes()).expect("the base config");
    let input = Config::create_from_stream(pair.input.as_bytes()).expect("the input config");
    let params = ConfigMergingParameters::new().expect("Create");
    params.set_avoid_duplicates(false);
    params.set_adjust_input_reference_space(false);
    params.set_error_on_conflict(case.error_on_conflict);
    if STRATEGIES[case.default].0.is_some() {
        params.set_default_strategy(STRATEGIES[case.default].1);
    }
    if STRATEGIES[case.roles].0.is_some() {
        params.set_roles(STRATEGIES[case.roles].1);
    }
    if case.overrides {
        params.set_name(NAME_OVERRIDE);
        params.set_description(DESCRIPTION_OVERRIDE);
    }
    let mut merged = (*base).clone();
    let (result, log) = crate::test_env::capture_log(|| {
        let mut options = MergeHandlerOptions {
            base_config: &base,
            input_config: &input,
            params: Some(&params),
            merged_config: &mut merged,
        };
        GeneralMerger::new(&mut options).and_then(|mut m| m.merge())?;
        RolesMerger::new(&mut options).and_then(|mut m| m.merge())
    });
    (result, merged, log.concat())
}

/// The general and roles sections of `MergeConfigs` merge as the wheel's, over every strategy
/// of each, error on conflict or not, and the overrides of the name and description.
#[test]
fn general_and_roles_match_the_wheel() {
    let cases = cases();
    let requests: Vec<Value> = cases
        .iter()
        .map(|case| json!({"calls": calls(case)}))
        .collect();
    let batch: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|args| BatchCall {
            cmd: "object_calls",
            args: args.clone(),
            blobs: Vec::new(),
        })
        .collect();
    let responses = Oracle::get().batch(&batch, true);
    assert_eq!(responses.len(), cases.len());

    let mut raised = 0;
    let mut logged = 0;
    let mut v2_0_from_v1 = 0;
    let mut mismatches = Vec::new();
    for (case, response) in cases.iter().zip(responses) {
        let response = response.unwrap_or_else(|e| panic!("{case:?}: {e}")).result;
        let calls = response["calls"].as_array().expect("the calls");
        let label = PAIRS[case.pair].label;
        for call in &calls[..calls.len() - 7] {
            assert!(call.get("result").is_some(), "{label} {case:?}: {call}");
        }
        let merge = &calls[calls.len() - 7];
        let values = &calls[calls.len() - 6..];
        let wheel_log: Vec<u8> = log(&merge["log"])
            .into_iter()
            .filter(|m| is_section_message(m))
            .collect::<Vec<_>>()
            .concat();

        let (result, merged, port_log) = port_merge(case);
        let mut problems = Vec::new();
        if wheel_log != port_log {
            problems.push(format!(
                "log: wheel {:?}, port {:?}",
                String::from_utf8_lossy(&wheel_log),
                String::from_utf8_lossy(&port_log)
            ));
        }
        if !port_log.is_empty() {
            logged += 1;
        }
        match (merge.get("exception"), result) {
            (Some(e), Err(port)) => {
                raised += 1;
                assert_eq!(e["type"], json!("Exception"), "{label} {case:?}");
                if bytes(&e["message"]) != port.what() {
                    problems.push(format!("exception: wheel {e}, port {port}"));
                }
            }
            (None, Ok(())) => {
                let port_values = json!([
                    {"result": String::from_utf8(merged.name().to_vec()).unwrap()},
                    {"result": String::from_utf8(merged.description().to_vec()).unwrap()},
                    {"result": merged.major_version()},
                    {"result": merged.minor_version()},
                    {"result": merged.default_luma_coefs().map(|c| json!({"f64": c.to_bits()}))},
                    {"result": (0..merged.num_roles())
                        .map(|i| json!([
                            String::from_utf8(merged.role_name(i).to_vec()).unwrap(),
                            String::from_utf8(merged.role_color_space_by_index(i).to_vec())
                                .unwrap(),
                        ]))
                        .collect::<Vec<_>>()},
                ]);
                let wheel_values: Vec<Value> = values
                    .iter()
                    .map(|v| json!({"result": v["result"]}))
                    .collect();
                if Value::Array(wheel_values) != port_values {
                    problems.push(format!("values: wheel {values:?}, port {port_values}"));
                }
                if case.pair == 1 && merged.major_version() == 2 && merged.minor_version() == 0 {
                    v2_0_from_v1 += 1;
                }
            }
            (wheel, port) => problems.push(format!("outcome: wheel {wheel:?}, port {port:?}")),
        }
        if !problems.is_empty() {
            mismatches.push(format!("{label} {case:?}: {}", problems.join("; ")));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {} cases differ:\n{}",
        mismatches.len(),
        cases.len(),
        mismatches.join("\n")
    );
    // Every outcome is reached: exceptions (conflicts with error on conflict, the `$` role),
    // warnings, and a v2.0 config merged from two v1 configs.
    assert!(raised >= 20, "{raised} merges raised");
    assert!(logged >= 50, "{logged} merges logged");
    assert!(v2_0_from_v1 >= 20, "{v2_0_from_v1} v1 merges made v2.0");
}
