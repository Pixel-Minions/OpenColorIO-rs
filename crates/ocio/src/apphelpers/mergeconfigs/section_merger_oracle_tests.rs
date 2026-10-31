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
//!
//! From 9.2e on, whole merges are compared too, every section run by the port's mergers in
//! upstream's order (the displays and views from 9.2f, the view transforms from 9.2g, the color
//! spaces from 9.2h, the named transforms from 9.2i): the merged config's `serialize()` byte for
//! byte, the whole log, or the exception.

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

/// Base and input configs for the merges compared whole (`serialize()`): file rules of both,
/// of one, the same but for case, with and without custom keys (in another order, or fewer), the
/// path search rule on one side only, the strict parsing flag; looks of both and of one, with
/// names differing in case; v1 configs (their default rules); displays and views (9.2f); view
/// transforms of both and of one, with names differing in case, of either reference space type,
/// that differ in a transform, a direction or the reference space type, the default view
/// transforms, and input configs whose reference spaces the converters change or can't find
/// (9.2g); color spaces of both and of one, named as a role, an alias or a color space of the
/// other config, with aliases that are one, of either reference space type, families with
/// other separators, environment variables, search paths that differ in case or contain one
/// another, v1 configs (9.2h); named transforms of both and of one, named as a role, a color
/// space, an alias or another named transform, with aliases that are one, and inactive lists
/// with aliases, named transforms, spaces and names that don't exist (9.2i).
const WHOLE_PAIRS: &[Pair] = &[
    Pair {
        label: "rules and looks",
        base: r#"ocio_profile_version: 2

name: base
strictparsing: true

roles:
  default: raw

file_rules:
  - !<Rule> {name: LogC, colorspace: lin, pattern: "*LogC*", extension: "*"}
  - !<Rule> {name: TIFF, colorspace: lin, regex: ".*\\.TIF?F$"}
  - !<Rule> {name: OpenEXR, colorspace: log, extension: "exr", pattern: "*"}
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: raw}

looks:
  - !<Look>
    name: look_both
    process_space: lin
    transform: !<CDLTransform> {slope: [1, 2, 1]}
  - !<Look>
    name: look_base
    process_space: log

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: log
"#,
        input: r#"ocio_profile_version: 2.1

name: input
strictparsing: false

roles:
  default: raw

file_rules:
  - !<Rule> {name: LogC, colorspace: lin, pattern: "*LogC*", extension: "*"}
  - !<Rule> {name: TIFF, colorspace: log, regex: ".*\\.TIF?F$", custom: {key1: value1, key2: value2}}
  - !<Rule> {name: JPEG, colorspace: lin, regex: ".*\\.jpeg$"}
  - !<Rule> {name: Png, colorspace: log, pattern: "*", extension: "png", custom: {a: b}}
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: lin}

looks:
  - !<Look>
    name: look_both
    process_space: log
  - !<Look>
    name: LOOK_INPUT
    process_space: lin
    transform: !<CDLTransform> {offset: [0.1, 0, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: log
"#,
    },
    Pair {
        label: "case and custom keys",
        base: r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Keys, colorspace: lin, pattern: "*abc*", extension: "*", custom: {k1: V1, k2: v2}}
  - !<Rule> {name: Only Base, colorspace: lin, pattern: "*b*", extension: "*"}
  - !<Rule> {name: Extra Keys, colorspace: lin, pattern: "*e*", extension: "*", custom: {k1: v1, k2: v2}}
  - !<Rule> {name: ColorSpaceNamePathSearch, custom: {ps: key}}
  - !<Rule> {name: Default, colorspace: raw}

looks:
  - !<Look>
    name: Look A
    process_space: lin

colorspaces:
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: lin
"#,
        input: r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: KEYS, colorspace: LIN, pattern: "*ABC*", extension: "*", custom: {k2: V2, k1: v1}}
  - !<Rule> {name: Other, colorspace: lin, pattern: "*o*", extension: "*"}
  - !<Rule> {name: Extra Keys, colorspace: lin, pattern: "*e*", extension: "*", custom: {k1: v1}}
  - !<Rule> {name: Default, colorspace: RAW}

looks:
  - !<Look>
    name: look a
    process_space: raw

colorspaces:
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: lin
"#,
    },
    Pair {
        label: "v1 configs",
        base: r#"ocio_profile_version: 1

strictparsing: false

roles:
  default: a

looks:
  - !<Look>
    name: l1
    process_space: a

colorspaces:
  - !<ColorSpace>
    name: a
"#,
        input: r#"ocio_profile_version: 1

roles:
  default: a

looks:
  - !<Look>
    name: l2
    process_space: a
  - !<Look>
    name: l1
    process_space: a
    transform: !<CDLTransform> {power: [2, 2, 2]}

colorspaces:
  - !<ColorSpace>
    name: a
"#,
    },
    Pair {
        label: "displays and views",
        base: r#"ocio_profile_version: 2

roles:
  default: raw

shared_views:
  - !<View> {name: SHARED_1, colorspace: lin}
  - !<View> {name: SHARED_2, view_transform: vt, display_colorspace: <USE_DISPLAY_NAME>}

viewing_rules:
  - !<Rule> {name: RULE_1, colorspaces: [lin]}
  - !<Rule> {name: RULE_2, encodings: [scene-linear], custom: {k: v}}
  - !<Rule> {name: RULE_4, encodings: [sdr-video, log]}

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: lin, rule: RULE_1}
    - !<Views> [SHARED_1, SHARED_2]
  DISP_2:
    - !<View> {name: VIEW_1, colorspace: lin, rule: RULE_2}
    - !<View> {name: VIEW_2, colorspace: log, looks: lk}

virtual_display:
  - !<View> {name: ACES, view_transform: vt, display_colorspace: <USE_DISPLAY_NAME>}
  - !<View> {name: Log, colorspace: log}
  - !<Views> [SHARED_1]

active_displays: [DISP_1, DISP_2]
active_views: [SHARED_1, SHARED_2, VIEW_1, VIEW_2]

colorspaces:
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: log
"#,
        input: r#"ocio_profile_version: 2

roles:
  default: raw

shared_views:
  - !<View> {name: SHARED_1, colorspace: log}
  - !<View> {name: SHARED_3, colorspace: lin, description: from input}
  - !<View> {name: VIEW_2, colorspace: log, looks: lk}

viewing_rules:
  - !<Rule> {name: RULE_1, colorspaces: [log, lin]}
  - !<Rule> {name: RULE_3, colorspaces: [lin]}
  - !<Rule> {name: rule_2, encodings: [SCENE-LINEAR], custom: {k: V}}
  - !<Rule> {name: RULE_4, encodings: [log]}

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: log, rule: RULE_3}
    - !<Views> [SHARED_3, SHARED_1]
  DISP_3:
    - !<View> {name: VIEW_1, colorspace: lin}
    - !<View> {name: VIEW_3, colorspace: log, looks: lk}
  DISP_2:
    - !<Views> [VIEW_2]

virtual_display:
  - !<View> {name: ACES, view_transform: vt, display_colorspace: <USE_DISPLAY_NAME>}
  - !<View> {name: Lin, colorspace: lin}
  - !<View> {name: SHARED_1, colorspace: raw}
  - !<Views> [SHARED_3]

active_displays: [DISP_1, DISP_3]
active_views: [SHARED_1, SHARED_3, VIEW_1, VIEW_3]

colorspaces:
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: log
"#,
    },
    Pair {
        label: "active lists",
        base: r#"ocio_profile_version: 2

roles:
  default: raw

displays:
  A:
    - !<View> {name: v, colorspace: raw}
    - !<View> {name: w, colorspace: raw}
  B:
    - !<View> {name: v, colorspace: raw}
  "x, y":
    - !<View> {name: v, colorspace: raw}

active_displays: [A, B, "x, y"]
active_views: [v, w]

colorspaces:
  - !<ColorSpace>
    name: raw
"#,
        input: r#"ocio_profile_version: 2

roles:
  default: raw

displays:
  b:
    - !<View> {name: V, colorspace: raw}
  C:
    - !<View> {name: v, colorspace: raw}

active_displays: [b, C]
active_views: [w, V]

colorspaces:
  - !<ColorSpace>
    name: raw
"#,
    },
    Pair {
        label: "v1 displays",
        base: r#"ocio_profile_version: 1

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Film, colorspace: lin, looks: +lk}

colorspaces:
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: lin
"#,
        input: r#"ocio_profile_version: 1

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Film, colorspace: lin}
  Rec709:
    - !<View> {name: Raw, colorspace: raw}

active_views: [Film]

colorspaces:
  - !<ColorSpace>
    name: raw
  - !<ColorSpace>
    name: lin
"#,
    },
    Pair {
        label: "view transforms",
        base: r#"ocio_profile_version: 2

roles:
  default: raw
  aces_interchange: ACES2065-1
  cie_xyz_d65_interchange: CIE-XYZ-D65

default_view_transform: SDR Video

view_transforms:
  - !<ViewTransform>
    name: SDR Video
    from_scene_reference: !<BuiltinTransform> {style: ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0}
  - !<ViewTransform>
    name: Un-tone-mapped
    from_scene_reference: !<BuiltinTransform> {style: UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD}
  - !<ViewTransform>
    name: Equal
    description: from base
    from_scene_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}
  - !<ViewTransform>
    name: Display VT
    from_display_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}
  - !<ViewTransform>
    name: Both
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}
    from_scene_reference: !<MatrixTransform> {offset: [-0.1, 0, 0, 0]}
  - !<ViewTransform>
    name: Ref Type
    from_scene_reference: !<CDLTransform> {slope: [2, 2, 2]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: ACES2065-1

display_colorspaces:
  - !<ColorSpace>
    name: CIE-XYZ-D65
"#,
        input: r#"ocio_profile_version: 2.1

roles:
  default: raw
  aces_interchange: ACES2065-1
  cie_xyz_d65_interchange: CIE-XYZ-D65

default_view_transform: Un-tone-mapped-2

view_transforms:
  - !<ViewTransform>
    name: sdr video
    from_scene_reference: !<BuiltinTransform> {style: ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1}
  - !<ViewTransform>
    name: Un-tone-mapped-2
    from_scene_reference: !<BuiltinTransform> {style: UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD}
  - !<ViewTransform>
    name: Equal
    family: other
    description: from input
    categories: [input]
    from_scene_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}
  - !<ViewTransform>
    name: DISPLAY VT
    from_display_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1]}
  - !<ViewTransform>
    name: Both
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}
  - !<ViewTransform>
    name: Ref Type
    from_display_reference: !<CDLTransform> {slope: [2, 2, 2]}
  - !<ViewTransform>
    name: To Only
    to_display_reference: !<ExponentTransform> {value: [2, 2, 2, 1]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: ACES2065-1

display_colorspaces:
  - !<ColorSpace>
    name: CIE-XYZ-D65
"#,
    },
    Pair {
        label: "view transforms in other reference spaces",
        base: r#"ocio_profile_version: 2

roles:
  default: raw
  aces_interchange: ACES2065-1
  cie_xyz_d65_interchange: CIE-XYZ-D65

view_transforms:
  - !<ViewTransform>
    name: VT1
    from_scene_reference: !<CDLTransform> {slope: [2, 2, 2]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: ACES2065-1

display_colorspaces:
  - !<ColorSpace>
    name: CIE-XYZ-D65
"#,
        input: r#"ocio_profile_version: 2

roles:
  default: raw
  aces_interchange: ACES2065-1
  cie_xyz_d65_interchange: CIE-XYZ-D65

default_view_transform: VT2

view_transforms:
  - !<ViewTransform>
    name: VT1
    from_scene_reference: !<CDLTransform> {slope: [1, 2, 1]}
  - !<ViewTransform>
    name: VT2
    from_scene_reference: !<BuiltinTransform> {style: ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0}
  - !<ViewTransform>
    name: VT3
    to_display_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1]}
    from_display_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1], direction: inverse}
  - !<ViewTransform>
    name: VT4
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: lin_rec709
  - !<ColorSpace>
    name: ACES2065-1
    to_scene_reference: !<MatrixTransform> {matrix: [2.52168618674388, -1.13413098823972, -0.387555198504164, 0, -0.276479914229922, 1.37271908766826, -0.096239173438334, 0, -0.0153780649660342, -0.152975335867399, 1.16835340083343, 0, 0, 0, 0, 1]}

display_colorspaces:
  - !<ColorSpace>
    name: XYZ half
  - !<ColorSpace>
    name: CIE-XYZ-D65
    to_display_reference: !<MatrixTransform> {matrix: [0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1]}
"#,
    },
    Pair {
        label: "view transforms of an unknown reference space",
        base: r#"ocio_profile_version: 2

roles:
  default: raw
  aces_interchange: ACES2065-1

default_view_transform: VT1

view_transforms:
  - !<ViewTransform>
    name: VT1
    from_scene_reference: !<CDLTransform> {slope: [2, 2, 2]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: ACES2065-1
"#,
        input: r#"ocio_profile_version: 2

roles:
  default: raw

view_transforms:
  - !<ViewTransform>
    name: vt1
    from_scene_reference: !<CDLTransform> {slope: [2, 2, 2]}
  - !<ViewTransform>
    name: VT2
    from_scene_reference: !<CDLTransform> {slope: [1, 2, 1]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: weird
    to_scene_reference: !<ExponentTransform> {value: [3, 3, 3, 1]}
"#,
    },
    Pair {
        label: "color spaces",
        base: r##"ocio_profile_version: 2

environment:
  test: value
  test1: value1
  test4: ""

search_path:
  - .
  - abc
  - luts

family_separator: "#"

roles:
  default: ACEScg
  rendering: ACEScg
  texture_paint: sRGB - Texture

displays:
  MyDisplay:
    - !<View> {name: MyDisplayView, colorspace: sRGB - Display}

display_colorspaces:
  - !<ColorSpace>
    name: sRGB - Display
    aliases: [srgb_display]
    family: Display#Basic
    description: from base
    from_display_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1], direction: inverse}

colorspaces:
  - !<ColorSpace>
    name: ACEScg
    aliases: [aces]
    family: ACES#Linear
    description: from base
    to_scene_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}

  - !<ColorSpace>
    name: sRGB - Texture
    family: Texture
    aliases: [srgb, srgb_tx]
    categories: [file-io]
    description: from base
    from_scene_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1], direction: inverse}

  - !<ColorSpace>
    name: look
    description: from base
    from_scene_reference: !<ColorSpaceTransform> {src: ACEScg, dst: sRGB - Texture}

  - !<ColorSpace>
    name: Linear
    aliases: [lin, Shared Alias]
    family: ""
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}
"##,
        input: r##"ocio_profile_version: 2.1

environment:
  test: differentValue
  test3: value3
  test4: from input

search_path:
  - .
  - def
  - ABC
  - luts2

family_separator: "~"

roles:
  default: ACES2065-1

display_colorspaces:
  - !<ColorSpace>
    name: sRGB - Display
    aliases: [srgb_display]
    family: Display~Standard
    description: from input
    from_display_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

  - !<ColorSpace>
    name: look
    aliases: [look1]
    family: Display~Standard
    description: from input
    from_display_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}

colorspaces:
  - !<ColorSpace>
    name: ACES2065-1
    aliases: [aces]
    family: ACES~Linear
    description: from input
    to_scene_reference: !<MatrixTransform> {matrix: [0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1]}

  - !<ColorSpace>
    name: sRGB
    aliases: [my_srgb]
    family: Texture~
    description: from input
    to_scene_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}

  - !<ColorSpace>
    name: Rendering
    description: named as a base role
    to_scene_reference: !<MatrixTransform> {offset: [0.2, 0, 0, 0]}

  - !<ColorSpace>
    name: Other Linear
    aliases: [LIN, texture_paint, other]
    family: Linear~Other
    to_scene_reference: !<MatrixTransform> {offset: [0.3, 0, 0, 0]}

  - !<ColorSpace>
    name: shared alias
    family: ""
    to_scene_reference: !<MatrixTransform> {offset: [0.4, 0, 0, 0]}

  - !<ColorSpace>
    name: Takes a name
    aliases: [linear]
    to_scene_reference: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}
"##,
    },
    Pair {
        label: "color spaces of v1 configs",
        base: r#"ocio_profile_version: 1

search_path: luts:shared

roles:
  default: raw
  reference: lin

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    isdata: true
  - !<ColorSpace>
    name: lin
    family: linear/scene
  - !<ColorSpace>
    name: Gamma
    family: display/gamma
    from_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}
"#,
        input: r#"ocio_profile_version: 1

search_path: SHARED:other

roles:
  default: raw
  reference: lin

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    isdata: true
  - !<ColorSpace>
    name: lin
    family: linear/scene
  - !<ColorSpace>
    name: gamma
    family: display/gamma
    from_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1]}
  - !<ColorSpace>
    name: log
    family: log
    to_reference: !<LogTransform> {base: 2, direction: inverse}
"#,
    },
    Pair {
        label: "named transforms",
        base: r#"ocio_profile_version: 2

roles:
  default: raw
  rendering: lin
  texture_paint: log

inactive_colorspaces: [log, nt_base, lin_alias, missing, nt_shared]

named_transforms:
  - !<NamedTransform>
    name: nt_base
    aliases: [ntb, shared_nt_alias]
    family: NT/Base
    transform: !<MatrixTransform> {offset: [0.1, 0, 0, 0]}
  - !<NamedTransform>
    name: nt_shared
    aliases: [nts]
    transform: !<MatrixTransform> {offset: [0.2, 0, 0, 0]}
  - !<NamedTransform>
    name: nt_alias_target
    aliases: [nt_in_name]
    inverse_transform: !<MatrixTransform> {offset: [0.3, 0, 0, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: lin
    aliases: [lin_alias]
  - !<ColorSpace>
    name: log
    to_scene_reference: !<LogTransform> {base: 2, direction: inverse}
"#,
        input: r#"ocio_profile_version: 2.1

roles:
  default: raw

inactive_colorspaces: [nt_input, lin, nt_shared, ghost]

named_transforms:
  - !<NamedTransform>
    name: NT_SHARED
    aliases: [nts, ntb]
    family: NT/Input
    transform: !<MatrixTransform> {offset: [0.4, 0, 0, 0]}
  - !<NamedTransform>
    name: nt_input
    aliases: [lin, nt_base, texture_paint, input_only_alias]
    transform: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}
  - !<NamedTransform>
    name: nt_in_name
    transform: !<MatrixTransform> {offset: [0.6, 0, 0, 0]}
  - !<NamedTransform>
    name: log
    transform: !<MatrixTransform> {offset: [0.7, 0, 0, 0]}
  - !<NamedTransform>
    name: lin_alias
    transform: !<MatrixTransform> {offset: [0.8, 0, 0, 0]}
  - !<NamedTransform>
    name: Rendering
    transform: !<MatrixTransform> {offset: [0.9, 0, 0, 0]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: Input Space
    aliases: [shared_nt_alias]
"#,
    },
    Pair {
        label: "inactive lists",
        base: r#"ocio_profile_version: 2

roles:
  default: a

inactive_colorspaces: [b, " c ", A_ALIAS, nt1]

named_transforms:
  - !<NamedTransform>
    name: nt1
    transform: !<MatrixTransform> {}

colorspaces:
  - !<ColorSpace>
    name: a
    aliases: [a_alias]
  - !<ColorSpace>
    name: b
  - !<ColorSpace>
    name: c
"#,
        input: r#"ocio_profile_version: 2

roles:
  default: a

inactive_colorspaces: [B, d, nt2, a]

named_transforms:
  - !<NamedTransform>
    name: nt2
    transform: !<MatrixTransform> {}

colorspaces:
  - !<ColorSpace>
    name: a
  - !<ColorSpace>
    name: d
"#,
    },
];

/// The index in [`WHOLE_PAIRS`] of the first pair with displays: the displays and views section
/// takes every strategy over these, the file rules and looks sections over the others.
const FIRST_DISPLAY_PAIR: usize = 3;

/// The index in [`WHOLE_PAIRS`] of the first pair with view transforms: the view transforms
/// section takes every strategy over these, with and without the adjustment of the input's
/// reference space.
const FIRST_VIEW_TRANSFORMS_PAIR: usize = 6;

/// The index in [`WHOLE_PAIRS`] of the first pair with color spaces the merge changes: the
/// color spaces section takes every strategy over these, with and without the overrides of the
/// search path and the environment, and with and without family prefixes.
const FIRST_COLORSPACES_PAIR: usize = 9;

/// The index in [`WHOLE_PAIRS`] of the first pair with named transforms: the named transforms
/// section takes every strategy over these, with and without the override of the inactive color
/// spaces, and with and without family prefixes.
const FIRST_NAMED_TRANSFORMS_PAIR: usize = 11;

/// The inactive color spaces the override sets, when a case sets it.
const INACTIVE_OVERRIDE: &str = "d, nt1, ghost, a_alias, nt_base, LOG";

/// The active displays and views the overrides set, when a case sets them.
const ACTIVE_DISPLAYS_OVERRIDE: &str = "DISP_2, A";
const ACTIVE_VIEWS_OVERRIDE: &str = "VIEW_2, v";

/// The search path and environment the overrides set, when a case sets them.
const SEARCH_PATH_OVERRIDE: &str = "ovr1:OVR2";
const ENVIRONMENT_OVERRIDE: &[(&str, &str)] = &[("test", "ovr value"), ("OVR2", "VALUE2")];

/// The family prefixes a case sets, when it sets them.
const INPUT_FAMILY_PREFIX: &str = "Input/";
const BASE_FAMILY_PREFIX: &str = "Base/Prefix/";

/// Fragments of the messages of the color spaces section (SectionMerger.cpp:2350-2618 @ v2.5.2)
/// that the merges of the color space pairs must log.
const COLORSPACES_MESSAGES: &[&str] = &[
    "' was not merged as it's identical to a role name.",
    "' will replace a color space in the base config.",
    "' was not merged as it's already present in the base config.",
    "' has a conflict with an alias in color space '",
    "' was not merged as it conflicts with an alias in color space '",
    "' has a different reference space type than the color space it's replacing.",
    "' that conflicts with color space '",
    "' in color space '",
    "' that conflicts with a role.",
];

/// Fragments of the messages of the named transforms section (SectionMerger.cpp:3123-3468 @
/// v2.5.2) that the merges of the named transform pairs must log.
const NAMED_TRANSFORMS_MESSAGES: &[&str] = &[
    "' was not merged as it's identical to a role name.",
    "' was not merged as there's a color space with that name.",
    "' was not merged as there's a color space alias with that name.",
    "' will replace a named transform in the base config.",
    "' was not merged as it's already present in the base config.",
    "' has a conflict with an alias in named transform '",
    "' was not merged as it conflicts with an alias in named transform '",
    "' that conflicts with color space '",
    "' that conflicts with a role.",
    "' that conflicts with named transform '",
    "' in named transform '",
];

/// One whole merge: a pair of [`WHOLE_PAIRS`], the default strategy (which the general and
/// roles sections take), the file rules', the displays and views', the view transforms', the
/// looks' and the color spaces' (indices into [`STRATEGIES`]), the input-first and
/// error-on-conflict options, whether the active displays and views are overridden, whether
/// the input's reference space is adjusted to the base's, whether the search path and the
/// environment are overridden, and whether the families get prefixes.
#[derive(Debug, Clone, Copy, Default)]
struct WholeCase {
    pair: usize,
    default: usize,
    file_rules: usize,
    display_views: usize,
    view_transforms: usize,
    looks: usize,
    colorspaces: usize,
    input_first: bool,
    error_on_conflict: bool,
    active_overrides: bool,
    adjust_reference: bool,
    colorspaces_overrides: bool,
    named_transforms: usize,
    inactive_override: bool,
    family_prefixes: bool,
}

/// Every pair with every default strategy but `STRATEGY_UNSPECIFIED` (none set is the default
/// default, `PreferInput`) and both options: the pairs without displays with every file rules
/// and looks strategy, those with displays with every displays and views strategy, with and
/// without the overrides of the active lists, those with view transforms with every view
/// transforms strategy, with and without the adjustment of the reference space, and those with
/// color spaces with every color spaces strategy, with and without the overrides of the search
/// path and the environment and the family prefixes. Unset strategies follow the default.
fn whole_cases() -> Vec<WholeCase> {
    let mut cases = Vec::new();
    for pair in 0..WHOLE_PAIRS.len() {
        for default in 0..STRATEGIES.len() - 1 {
            let mut sections = Vec::new();
            if pair < FIRST_DISPLAY_PAIR {
                for file_rules in 0..STRATEGIES.len() {
                    for looks in 0..STRATEGIES.len() {
                        sections.push(WholeCase {
                            file_rules,
                            looks,
                            ..WholeCase::default()
                        });
                    }
                }
            } else if pair < FIRST_VIEW_TRANSFORMS_PAIR {
                for display_views in 0..STRATEGIES.len() {
                    for active_overrides in [false, true] {
                        sections.push(WholeCase {
                            display_views,
                            active_overrides,
                            ..WholeCase::default()
                        });
                    }
                }
            } else if pair < FIRST_COLORSPACES_PAIR {
                for view_transforms in 0..STRATEGIES.len() {
                    for adjust_reference in [false, true] {
                        sections.push(WholeCase {
                            view_transforms,
                            adjust_reference,
                            ..WholeCase::default()
                        });
                    }
                }
            } else if pair < FIRST_NAMED_TRANSFORMS_PAIR {
                for colorspaces in 0..STRATEGIES.len() {
                    for (colorspaces_overrides, family_prefixes) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        sections.push(WholeCase {
                            colorspaces,
                            colorspaces_overrides,
                            family_prefixes,
                            ..WholeCase::default()
                        });
                    }
                }
            } else {
                for named_transforms in 0..STRATEGIES.len() {
                    for (inactive_override, family_prefixes) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        sections.push(WholeCase {
                            named_transforms,
                            inactive_override,
                            family_prefixes,
                            ..WholeCase::default()
                        });
                    }
                }
            }
            for section in sections {
                for input_first in [true, false] {
                    for error_on_conflict in [false, true] {
                        cases.push(WholeCase {
                            pair,
                            default,
                            input_first,
                            error_on_conflict,
                            ..section
                        });
                    }
                }
            }
        }
    }
    cases
}

/// `{"enum": "ConfigMergingParameters.<name>"}`.
fn strategy_arg(name: &str) -> Value {
    json!({"enum": format!("ConfigMergingParameters.{name}")})
}

/// The wheel's calls of a whole merge: the configs, the parameters, `MergeConfigs`, then the
/// merged config's `serialize()`.
fn whole_calls(case: &WholeCase) -> Vec<Value> {
    let pair = &WHOLE_PAIRS[case.pair];
    let mut calls = vec![
        json!({"call": "CreateFromStream", "on": "Config", "args": [pair.base], "as": "merged"}),
        json!({"call": "CreateFromStream", "on": "Config", "args": [pair.base], "as": "base"}),
        json!({"call": "CreateFromStream", "on": "Config", "args": [pair.input], "as": "input"}),
        json!({"call": "Create", "on": "ConfigMergingParameters", "as": "p"}),
        json!({"call": "setAvoidDuplicates", "on": "p", "args": [false]}),
        json!({"call": "setAdjustInputReferenceSpace", "on": "p",
               "args": [case.adjust_reference]}),
        json!({"call": "setInputFirst", "on": "p", "args": [case.input_first]}),
        json!({"call": "setErrorOnConflict", "on": "p", "args": [case.error_on_conflict]}),
    ];
    if case.active_overrides {
        calls.push(
            json!({"call": "setActiveDisplays", "on": "p", "args": [ACTIVE_DISPLAYS_OVERRIDE]}),
        );
        calls.push(json!({"call": "setActiveViews", "on": "p", "args": [ACTIVE_VIEWS_OVERRIDE]}));
    }
    if case.colorspaces_overrides {
        calls.push(json!({"call": "setSearchPath", "on": "p", "args": [SEARCH_PATH_OVERRIDE]}));
        for (name, value) in ENVIRONMENT_OVERRIDE {
            calls.push(json!({"call": "addEnvironmentVar", "on": "p", "args": [name, value]}));
        }
    }
    if case.family_prefixes {
        calls.push(
            json!({"call": "setInputFamilyPrefix", "on": "p", "args": [INPUT_FAMILY_PREFIX]}),
        );
        calls.push(json!({"call": "setBaseFamilyPrefix", "on": "p", "args": [BASE_FAMILY_PREFIX]}));
    }
    if case.inactive_override {
        calls.push(
            json!({"call": "setInactiveColorSpaces", "on": "p", "args": [INACTIVE_OVERRIDE]}),
        );
    }
    for (setter, index) in [
        ("setDefaultStrategy", case.default),
        ("setFileRules", case.file_rules),
        ("setDisplayViews", case.display_views),
        ("setViewTransforms", case.view_transforms),
        ("setLooks", case.looks),
        ("setColorspaces", case.colorspaces),
        ("setNamedTransforms", case.named_transforms),
    ] {
        if let Some(name) = STRATEGIES[index].0 {
            calls.push(json!({"call": setter, "on": "p", "args": [strategy_arg(name)]}));
        }
    }
    calls.extend([
        json!({"call": "MergeConfigs", "on": "ConfigMergingHelpers",
               "args": [{"ref": "p"}, {"ref": "base"}, {"ref": "input"}], "as": "merged"}),
        json!({"call": "serialize", "on": "merged"}),
    ]);
    calls
}

/// A setter of a strategy of [`ConfigMergingParameters`].
type StrategySetter = fn(&ConfigMergingParameters, MergeStrategies);

/// The port's merge of a case by the ported sections, in upstream's order (`MergeConfigs`,
/// MergeConfigsHelpers.cpp:748-758 @ v2.5.2): the merged config's `serialize()` or the
/// exception, the log, and the log of reading the two configs.
fn port_whole_merge(case: &WholeCase) -> (Result<Vec<u8>>, Vec<u8>, Vec<u8>) {
    let pair = &WHOLE_PAIRS[case.pair];
    let ((base, input), load_log) = crate::test_env::capture_log(|| {
        (
            Config::create_from_stream(pair.base.as_bytes()).expect("the base config"),
            Config::create_from_stream(pair.input.as_bytes()).expect("the input config"),
        )
    });
    let params = ConfigMergingParameters::new().expect("Create");
    params.set_avoid_duplicates(false);
    params.set_adjust_input_reference_space(case.adjust_reference);
    params.set_input_first(case.input_first);
    params.set_error_on_conflict(case.error_on_conflict);
    if case.active_overrides {
        params
            .set_active_displays(ACTIVE_DISPLAYS_OVERRIDE)
            .expect("setActiveDisplays");
        params
            .set_active_views(ACTIVE_VIEWS_OVERRIDE)
            .expect("setActiveViews");
    }
    if case.colorspaces_overrides {
        params.set_search_path(SEARCH_PATH_OVERRIDE);
        for (name, value) in ENVIRONMENT_OVERRIDE {
            params.add_environment_var(name, Some(value.as_bytes()));
        }
    }
    if case.family_prefixes {
        params.set_input_family_prefix(INPUT_FAMILY_PREFIX);
        params.set_base_family_prefix(BASE_FAMILY_PREFIX);
    }
    if case.inactive_override {
        params.set_inactive_color_spaces(INACTIVE_OVERRIDE);
    }
    let ported: [(StrategySetter, usize); 7] = [
        (ConfigMergingParameters::set_default_strategy, case.default),
        (ConfigMergingParameters::set_file_rules, case.file_rules),
        (
            ConfigMergingParameters::set_display_views,
            case.display_views,
        ),
        (
            ConfigMergingParameters::set_view_transforms,
            case.view_transforms,
        ),
        (ConfigMergingParameters::set_looks, case.looks),
        (ConfigMergingParameters::set_colorspaces, case.colorspaces),
        (
            ConfigMergingParameters::set_named_transforms,
            case.named_transforms,
        ),
    ];
    for (setter, index) in ported {
        if STRATEGIES[index].0.is_some() {
            setter(&params, STRATEGIES[index].1);
        }
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
        RolesMerger::new(&mut options).and_then(|mut m| m.merge())?;
        FileRulesMerger::new(&mut options).and_then(|mut m| m.merge())?;
        DisplayViewMerger::new(&mut options).and_then(|mut m| m.merge())?;
        ViewTransformsMerger::new(&mut options).and_then(|mut m| m.merge())?;
        LooksMerger::new(&mut options).and_then(|mut m| m.merge())?;
        ColorspacesMerger::new(&mut options).and_then(|mut m| m.merge())?;
        NamedTransformsMerger::new(&mut options).and_then(|mut m| m.merge())
    });
    (
        result.and_then(|()| merged.serialize()),
        log.concat(),
        load_log.concat(),
    )
}

/// Whole merges, as the wheel's `MergeConfigs` makes them: the merged config's `serialize()`
/// byte for byte, the log, or the exception, over the strategies of every section, the
/// input-first and error-on-conflict options, the adjustment of the reference space, the
/// overrides (active lists, search path, environment, inactive color spaces) and the family
/// prefixes.
fn check_whole_merges(part: usize) {
    // Full reference transcripts are retained in each part so the original matrix-wide
    // coverage requirements below run unchanged on the selected emulated CPU. Serialize
    // these large transcripts in native libtest too, to avoid multiplying peak memory.
    static WHOLE_MERGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _whole_merge = WHOLE_MERGE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(part < WHOLE_MERGE_PARTS);
    let cases = whole_cases();
    let requests: Vec<Value> = cases
        .iter()
        .map(|case| json!({"calls": whole_calls(case)}))
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

    let (mut raised, mut logged) = (0, 0);
    let mut texts = std::collections::BTreeSet::new();
    let mut display_texts = std::collections::BTreeSet::new();
    let mut view_transform_texts = std::collections::BTreeSet::new();
    let mut colorspace_texts = std::collections::BTreeSet::new();
    let mut colorspaces_messages = [0; COLORSPACES_MESSAGES.len()];
    let mut named_transform_texts = std::collections::BTreeSet::new();
    let mut named_transforms_messages = [0; NAMED_TRANSFORMS_MESSAGES.len()];
    let mut adjusted_raised = 0;
    let mut adjusted_texts = std::collections::BTreeSet::new();
    let mut mismatches = Vec::new();
    let mut checked = 0;
    for (index, (case, response)) in cases.iter().zip(responses).enumerate() {
        let response = response.unwrap_or_else(|e| panic!("{case:?}: {e}")).result;
        let calls = response["calls"].as_array().expect("the calls");
        let label = WHOLE_PAIRS[case.pair].label;
        for call in &calls[..calls.len() - 2] {
            assert!(call.get("result").is_some(), "{label} {case:?}: {call}");
        }
        let merge = &calls[calls.len() - 2];
        let serialize = &calls[calls.len() - 1];
        let wheel_log = log(&merge["log"]).concat();
        if (FIRST_COLORSPACES_PAIR..FIRST_NAMED_TRANSFORMS_PAIR).contains(&case.pair) {
            for (i, fragment) in COLORSPACES_MESSAGES.iter().enumerate() {
                if wheel_log
                    .windows(fragment.len())
                    .any(|w| w == fragment.as_bytes())
                {
                    colorspaces_messages[i] += 1;
                }
            }
        }
        if case.pair >= FIRST_NAMED_TRANSFORMS_PAIR {
            for (i, fragment) in NAMED_TRANSFORMS_MESSAGES.iter().enumerate() {
                if wheel_log
                    .windows(fragment.len())
                    .any(|w| w == fragment.as_bytes())
                {
                    named_transforms_messages[i] += 1;
                }
            }
        }

        // All original coverage bounds apply to the complete reference matrix. The exact
        // comparisons in the parts, together with the exhaustive/disjoint plan test below,
        // imply the same port outcome counts and text sets. No comparison is sampled out:
        // each original case executes its unchanged Rust merge in exactly one part.
        if !wheel_log.is_empty() {
            logged += 1;
        }
        if merge.get("exception").is_some() {
            raised += 1;
            if case.adjust_reference {
                adjusted_raised += 1;
            }
        } else {
            let text = serialize["result"]
                .as_str()
                .unwrap_or_else(|| panic!("{label} {case:?}: {serialize}"))
                .as_bytes()
                .to_vec();
            if case.pair >= FIRST_NAMED_TRANSFORMS_PAIR {
                named_transform_texts.insert(text.clone());
            } else if case.pair >= FIRST_COLORSPACES_PAIR {
                colorspace_texts.insert(text.clone());
            } else if case.pair >= FIRST_VIEW_TRANSFORMS_PAIR {
                // (The base configs of these pairs have no group transforms: the input's
                // view transforms with the reference space converters do.)
                if case.adjust_reference && text.windows(17).any(|w| w == b"!<GroupTransform>") {
                    adjusted_texts.insert(text.clone());
                }
                view_transform_texts.insert(text.clone());
            } else if case.pair >= FIRST_DISPLAY_PAIR {
                display_texts.insert(text.clone());
            }
            texts.insert(text);
        }
        if !whole_merge_in_part(index, part) {
            continue;
        }
        checked += 1;
        let (result, port_log, port_load_log) = port_whole_merge(case);
        let wheel_load_log = [log(&calls[1]["log"]), log(&calls[2]["log"])]
            .concat()
            .concat();
        assert_eq!(
            String::from_utf8_lossy(&wheel_load_log),
            String::from_utf8_lossy(&port_load_log),
            "{label} {case:?}: reading the configs"
        );
        let mut problems = Vec::new();
        if wheel_log != port_log {
            problems.push(format!(
                "log: wheel {:?}, port {:?}",
                String::from_utf8_lossy(&wheel_log),
                String::from_utf8_lossy(&port_log)
            ));
        }
        match (merge.get("exception"), result) {
            (Some(e), Err(port)) => {
                if bytes(&e["message"]) != port.what() {
                    problems.push(format!("exception: wheel {e}, port {port}"));
                }
            }
            (None, Ok(text)) => {
                let wheel_text = serialize["result"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{label} {case:?}: {serialize}"));
                if wheel_text.as_bytes() != text {
                    problems.push(format!(
                        "serialize:\n--- wheel\n{wheel_text}\n--- port\n{}",
                        String::from_utf8_lossy(&text)
                    ));
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
        checked,
        mismatches.join("\n")
    );
    assert!(checked > 0);
    assert_eq!(
        checked,
        (0..cases.len())
            .filter(|&index| whole_merge_in_part(index, part))
            .count()
    );
    println!(
        "whole-merge part {part}/{WHOLE_MERGE_PARTS}: checked {checked} of {} original cases",
        cases.len()
    );
    // Every outcome is reached: conflicts as exceptions and as warnings, and many different
    // merged configs.
    assert!(raised >= 50, "{raised} merges raised");
    assert!(logged >= 200, "{logged} merges logged");
    assert!(
        texts.len() >= 40,
        "{} different merged configs",
        texts.len()
    );
    assert!(
        display_texts.len() >= 20,
        "{} different merged configs with displays",
        display_texts.len()
    );
    assert!(
        view_transform_texts.len() >= 30,
        "{} different merged configs with view transforms",
        view_transform_texts.len()
    );
    // The reference space converters change the input's view transforms, and fail where the
    // input's reference space is unknown.
    assert!(
        adjusted_texts.len() >= 3,
        "{} different merged configs with adjusted view transforms",
        adjusted_texts.len()
    );
    assert!(
        adjusted_raised >= 150,
        "{adjusted_raised} merges adjusting the reference space raised"
    );
    assert!(
        colorspace_texts.len() >= 80,
        "{} different merged configs with color spaces",
        colorspace_texts.len()
    );
    // Each kind of color space conflict is logged.
    for (fragment, count) in COLORSPACES_MESSAGES.iter().zip(colorspaces_messages) {
        assert!(count >= 10, "{count} merges logged \"{fragment}\"");
    }
    assert!(
        named_transform_texts.len() >= 60,
        "{} different merged configs with named transforms",
        named_transform_texts.len()
    );
    // Each kind of named transform conflict is logged.
    for (fragment, count) in NAMED_TRANSFORMS_MESSAGES
        .iter()
        .zip(named_transforms_messages)
    {
        assert!(count >= 10, "{count} merges logged \"{fragment}\"");
    }
}

fn whole_merge_in_part(index: usize, part: usize) -> bool {
    index % WHOLE_MERGE_PARTS == part
}

// These are execution partitions of one synthetic oracle sweep, not upstream test markers.
// Generating the test functions and partition inventory together prevents an unregistered part.
macro_rules! whole_merge_parts {
    ($($name:ident => $part:expr),+ $(,)?) => {
        const WHOLE_MERGE_PARTS: usize = [$($part),+].len();

        #[test]
        fn whole_merge_parts_cover_original_cases_once() {
            let parts = [$($part),+];
            assert!(parts.into_iter().eq(0..WHOLE_MERGE_PARTS));
            let cases = whole_cases();
            let mut visits = vec![0; cases.len()];
            for part in parts {
                for (index, visited) in visits.iter_mut().enumerate() {
                    if whole_merge_in_part(index, part) {
                        *visited += 1;
                    }
                }
            }
            assert!(visits.iter().all(|&visited| visited == 1));
        }

        $(
            #[test]
            fn $name() {
                check_whole_merges($part);
            }
        )+
    };
}

whole_merge_parts! {
    whole_merges_part_0 => 0,
    whole_merges_part_1 => 1,
    whole_merges_part_2 => 2,
    whole_merges_part_3 => 3,
}
