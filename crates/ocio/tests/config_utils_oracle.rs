// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Config::IdentifyBuiltinColorSpace`, `IdentifyInterchangeSpace`,
//! `GetProcessorToBuiltinColorSpace` and `GetProcessorFromBuiltinColorSpace` (ConfigUtils'
//! heuristics) against the wheel, through the oracle's `object_calls` (WP 9.1a).
//!
//! Source configs: the 8 built-in configs as they are (their interchange roles) and without
//! their interchange roles (the heuristics); upstream's two test configs of
//! `ConfigUtils/processor_to_known_colorspace*` with inactive lists that leave each heuristic
//! its candidates; upstream's configs of `tests/data/files/configs`; and generated ones
//! (interchange roles pointing to missing spaces, no reference space, no data space, blocked
//! transforms, spi1d and spimtx files, an sRGB alias in upper case and a from-reference
//! transform whose direction is inverse, docs/improvements.md I-377 and I-378). Each is
//! checked against every color space of the default built-in config (inactive and
//! display-referred ones included), aliases, a missing name and `""`: the name or names, or the
//! exception's message, and the log; and the processors to and from the built-in config (the
//! processor and the pixels of its CPU processors, as `config_from_configs_oracle.rs` compares
//! them).
//!
//! The names the heuristics pick follow the pixels of the CPU processors, compared within
//! tolerances: this target is in the `cpu-tests` alias.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::app_helpers::{
    EXPOSURE_CONTRAST_GAMMA, Reached, Rng, apply_calls, capture, compare_processor_call, pixels,
    report, text,
};
use ocio::{
    BuiltinConfigRegistry, ColorSpaceVisibility, Config, DynamicPropertyType,
    ExposureContrastTransform, Processor, SearchReferenceSpaceType, Transform,
};
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::image_desc::PackedImageDesc;
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, log};
use serde_json::{Value, json};

/// The source config of upstream's `ConfigUtils/processor_to_known_colorspace`
/// (ColorSpace_tests.cpp:1499-1624 @ v2.5.2).
const KNOWN_CS_CONFIG: &str = include_str!("data/config_utils_known_colorspace.ocio");

/// The source config of upstream's `ConfigUtils/processor_to_known_colorspace_alt_config`
/// (ColorSpace_tests.cpp:1932-2029 @ v2.5.2).
const ALT_CONFIG: &str = include_str!("data/config_utils_alt_config.ocio");

/// The AP0 to linear Rec.709 matrix of upstream's test configs.
const AP0_TO_709: &str = "[2.52168618674388, -1.13413098823972, -0.387555198504164, 0, \
                          -0.276479914229922, 1.37271908766826, -0.096239173438334, 0, \
                          -0.0153780649660342, -0.152975335867399, 1.16835340083343, 0, 0, 0, \
                          0, 1]";

/// A generated config: a reference space, a data space, and `spaces` (YAML list entries).
fn generated(roles: &str, spaces: &str) -> String {
    format!(
        "ocio_profile_version: 2\n\nroles:\n  default: raw\n{roles}\ncolorspaces:\n  - \
         !<ColorSpace>\n    name: raw\n    isdata: true\n\n  - !<ColorSpace>\n    name: \
         ref_cs\n{spaces}"
    )
}

/// An sRGB texture space named `name` (with `extra` keys) whose from-reference transform is
/// the AP0 to Rec.709 matrix then the sRGB curve.
fn srgb_space(name: &str, extra: &str) -> String {
    format!(
        "\n  - !<ColorSpace>\n    name: {name}\n{extra}    from_scene_reference: \
         !<GroupTransform>\n      children:\n        - !<MatrixTransform> {{matrix: \
         {AP0_TO_709}}}\n        - !<ExponentWithLinearTransform> {{gamma: 2.4, offset: \
         0.055, direction: inverse}}\n"
    )
}

/// An ACEScg space named `name` whose RGB are first scaled by `scale`.
fn scaled_cg(name: &str, scale: &str) -> String {
    format!(
        "\n  - !<ColorSpace>\n    name: {name}\n    to_scene_reference: !<GroupTransform>\n      \
         children:\n        - !<MatrixTransform> {{matrix: [{scale}, 0, 0, 0, 0, {scale}, 0, 0, \
         0, 0, {scale}, 0, 0, 0, 0, 1]}}\n        - !<BuiltinTransform> {{style: \
         ACEScg_to_ACES2065-1}}\n"
    )
}
/// The generated configs, by label.
fn generated_configs() -> Vec<(&'static str, String)> {
    let lin709 = format!(
        "\n  - !<ColorSpace>\n    name: lin709\n    from_scene_reference: !<MatrixTransform> \
         {{matrix: {AP0_TO_709}}}\n"
    );
    let quirk = format!(
        "\n  - !<ColorSpace>\n    name: my sRGB\n    from_scene_reference: \
         !<GroupTransform>\n      direction: inverse\n      children:\n        - \
         !<ExponentWithLinearTransform> {{gamma: 2.4, offset: 0.055}}\n        - \
         !<MatrixTransform> {{matrix: {AP0_TO_709}, direction: inverse}}\n"
    );
    let blocked = format!(
        "\n  - !<ColorSpace>\n    name: srgb clf\n    from_scene_reference: !<FileTransform> \
         {{src: clf/lut3d_as_matrix.clf}}\n\n  - !<ColorSpace>\n    name: srgb cst\n    \
         to_scene_reference: !<ColorSpaceTransform> {{src: lin709, dst: ref_cs}}\n\n  - \
         !<ColorSpace>\n    name: srgb look\n    to_scene_reference: !<LookTransform> {{src: \
         lin709, dst: ref_cs, looks: \"\"}}\n\n  - !<ColorSpace>\n    name: srgb dvt\n    \
         to_scene_reference: !<DisplayViewTransform> {{src: lin709, display: d, view: v}}\n\n  \
         - !<ColorSpace>\n    name: srgb \
         spi1d\n    to_scene_reference: !<GroupTransform>\n      children:\n        - \
         !<FileTransform> {{src: sRGB_to_linear.spi1d, interpolation: linear}}\n        - \
         !<MatrixTransform> {{matrix: {AP0_TO_709}, direction: inverse}}\n\n  - \
         !<ColorSpace>\n    name: spimtx space\n    to_scene_reference: !<FileTransform> \
         {{src: camera_to_aces.spimtx}}\n{lin709}"
    );
    vec![
        ("no roles, sRGB", generated("", &srgb_space("tex sRGB", ""))),
        ("no roles, linear", generated("", &lin709)),
        // ACEScg scaled by 1.003 (within IdentifyBuiltinColorSpace's 5e-3 on its test values,
        // past 1e-3) and by 1.01 (past 5e-3).
        (
            "near ACEScg",
            generated(
                "",
                &format!(
                    "{}{}{lin709}",
                    scaled_cg("cg near", "1.003"),
                    scaled_cg("cg far", "1.01")
                ),
            ),
        ),
        // A space whose encoding says it isn't linear, so the linear heuristic skips it before
        // making its processor (its file is missing), then a linear space.
        (
            "encoded, missing file",
            generated(
                "",
                &format!(
                    "\n  - !<ColorSpace>\n    name: video file\n    encoding: sdr-video\n    \
                     to_scene_reference: !<FileTransform> {{src: no_such_lut.spi1d}}\n{lin709}"
                ),
            ),
        ),
        // A linear space whose transform is an identity: the linear heuristic never pairs a
        // built-in space with itself, so it identifies nothing.
        (
            "identity linear",
            generated(
                "",
                "\n  - !<ColorSpace>\n    name: lin identity\n    to_scene_reference: \
                 !<MatrixTransform> {}\n",
            ),
        ),
        (
            "ghost roles",
            generated(
                "  aces_interchange: ghost\n  cie_xyz_d65_interchange: ghost_display\n",
                &lin709,
            ),
        ),
        (
            "alias SRGB",
            generated("", &srgb_space("my texture", "    aliases: [Tex_SRGB]\n")),
        ),
        (
            "alias srgb",
            generated("", &srgb_space("my texture", "    aliases: [tex_srgb]\n")),
        ),
        ("inverse from-reference", generated("", &quirk)),
        ("blocked", generated("", &blocked)),
        (
            "no reference",
            "ocio_profile_version: 2\n\nroles:\n  default: lin\n\ncolorspaces:\n  - \
             !<ColorSpace>\n    name: lin\n    to_scene_reference: !<MatrixTransform> \
             {offset: [0.1, 0, 0, 0]}\n"
                .to_string(),
        ),
        (
            "inactive reference, no data",
            format!(
                "ocio_profile_version: 2\n\nroles:\n  default: ref_cs\n\ninactive_colorspaces: \
                 [ref_cs]\n\ncolorspaces:\n  - !<ColorSpace>\n    name: ref_cs\n{}",
                srgb_space("tex sRGB", "")
            ),
        ),
    ]
}

/// A change made to a source config after loading it, on both sides.
#[derive(Clone, Debug)]
enum Edit {
    Role(&'static str, &'static str),
    Inactive(&'static str),
    AddSearchPath(String),
}

/// Where a config comes from, and the changes made to it.
#[derive(Clone, Debug)]
struct Source {
    label: String,
    load: Load,
    edits: Vec<Edit>,
}

#[derive(Clone, Debug)]
enum Load {
    Yaml(String),
    Builtin(String),
    File(String),
}

impl Source {
    /// The port's config, and what it logged while loading and changing it.
    fn port(&self) -> (Arc<Config>, Vec<Vec<u8>>) {
        capture(|| {
            let loaded = match &self.load {
                Load::Yaml(text) => Config::create_from_stream(text.as_bytes()).unwrap(),
                Load::Builtin(name) => Config::create_from_file(name).unwrap(),
                Load::File(path) => Config::create_from_file(path).unwrap(),
            };
            let mut config = (*loaded).clone();
            for edit in &self.edits {
                match edit {
                    Edit::Role(role, cs) => config.set_role(role, Some(cs.as_bytes())).unwrap(),
                    Edit::Inactive(list) => config.set_inactive_color_spaces(list),
                    Edit::AddSearchPath(path) => config.add_search_path(path),
                }
            }
            Arc::new(config)
        })
    }

    /// The wheel's calls that load the config, stored as `name`, and change it.
    fn wheel(&self, name: &str) -> Vec<Value> {
        let (call, arg) = match &self.load {
            Load::Yaml(text) => ("CreateFromStream", text),
            Load::Builtin(name) | Load::File(name) => ("CreateFromFile", name),
        };
        let mut calls = vec![json!({"call": call, "on": "Config", "args": [arg], "as": name})];
        for edit in &self.edits {
            calls.push(match edit {
                Edit::Role(role, cs) => json!({"call": "setRole", "on": name, "args": [role, cs]}),
                Edit::Inactive(list) => {
                    json!({"call": "setInactiveColorSpaces", "on": name, "args": [list]})
                }
                Edit::AddSearchPath(path) => {
                    json!({"call": "addSearchPath", "on": name, "args": [path]})
                }
            });
        }
        calls
    }
}

fn source(label: impl Into<String>, load: Load, edits: Vec<Edit>) -> Source {
    Source {
        label: label.into(),
        load,
        edits,
    }
}

/// Upstream's test files directory (`GetTestFilesDir`), with forward slashes.
fn test_files_dir() -> String {
    ocio_testkit::paths::upstream_dir()
        .join("tests/data/files")
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every name of a color space of `config`, inactive ones included, with its aliases.
fn all_names(config: &Config) -> Vec<String> {
    let mut out = Vec::new();
    let n = config.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    for i in 0..n {
        let name = config.color_space_name_by_index_with(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::All,
            i,
        );
        let cs = config
            .color_space(name)
            .expect("a color space of the config");
        out.push(text(name));
        out.extend((0..cs.num_aliases()).map(|a| text(cs.alias(a))));
    }
    out
}

/// The names of the built-in config the cases ask for: every color space and alias, a role, a
/// name cut by a NUL, a color space only the studio config has, a missing name and `""`.
fn builtin_names(builtin: &Config) -> Vec<String> {
    let mut names = all_names(builtin);
    names.extend([
        "aces_interchange".to_string(),
        "ACEScg\0tail".to_string(),
        // A color space of the studio config that the default config lacks.
        "ARRI LogC3 (EI800)".to_string(),
        "no_such_space".to_string(),
        String::new(),
    ]);
    names
}

/// One case: a call and its arguments, names only.
#[derive(Clone, Debug)]
enum Case {
    IdentifyBuiltin(String),
    IdentifyInterchange(String, String),
    ToBuiltin(String, String),
    FromBuiltin(String, String),
}

/// The cases of `src`: every built-in name for IdentifyBuiltinColorSpace, and `n` of each
/// other call with names drawn by `seed` (the source's names, a missing one, `""`).
fn cases(src: &Config, builtin: &Config, seed: u64, n: usize) -> Vec<Case> {
    let builtin_names = builtin_names(builtin);
    let mut src_names = all_names(src);
    src_names.extend(["no_such_space".to_string(), String::new()]);
    let mut rng = Rng(seed);
    let mut out: Vec<Case> = builtin_names
        .iter()
        .map(|b| Case::IdentifyBuiltin(b.clone()))
        .collect();
    for _ in 0..n {
        out.push(Case::IdentifyInterchange(
            rng.pick(&src_names),
            rng.pick(&builtin_names),
        ));
        out.push(Case::ToBuiltin(
            rng.pick(&src_names),
            rng.pick(&builtin_names),
        ));
        out.push(Case::FromBuiltin(
            rng.pick(&builtin_names),
            rng.pick(&src_names),
        ));
    }
    out
}

/// The dynamic properties of `proc`'s CPU processor, set away from their defaults.
fn dynamic_sets(proc: &Processor) -> Vec<common::app_helpers::DynamicSet> {
    EXPOSURE_CONTRAST_GAMMA
        .iter()
        .copied()
        .filter(|(t, _, _)| proc.has_dynamic_property(*t))
        .collect()
}

/// What the cases reached: the processors' comparisons, the names the identifications
/// returned, and the messages they raised.
#[derive(Default)]
struct Coverage {
    reached: Reached,
    names: usize,
    name_errors: usize,
    found: BTreeSet<String>,
    messages: BTreeSet<String>,
}

/// The port's names of an identification, as the binding returns them (a string, or a tuple
/// of two).
fn port_names(case: &Case, src: &Config, builtin: &Config) -> ocio::Result<Value> {
    match case {
        Case::IdentifyBuiltin(name) => {
            Config::identify_builtin_color_space(src, builtin, name).map(|n| json!(text(n)))
        }
        Case::IdentifyInterchange(src_name, name) => {
            Config::identify_interchange_space(src, src_name, builtin, name)
                .map(|(s, b)| json!([text(s), text(b)]))
        }
        _ => unreachable!("an identification"),
    }
}

/// The port's outcome of a case.
enum Outcome {
    /// An identification: its names, as the binding returns them.
    Names(ocio::Result<Value>),
    /// A processor to or from the built-in config.
    Processor(ocio::Result<Arc<Processor>>),
}

/// The default built-in config, the built-in config of `GetProcessorTo/FromBuiltinColorSpace`.
const DEFAULT_BUILTIN: &str = "ocio://default";

/// Compares the cases of `source` with the wheel, 48 a request: IdentifyBuiltinColorSpace for
/// every name of the default built-in config, and `n` of each other call.
fn check(
    source: &Source,
    seed: u64,
    n: usize,
    coverage: &mut Coverage,
    failures: &mut Vec<String>,
) {
    let (src, _) = source.port();
    let (builtin, _) = capture(|| Config::create_from_file(DEFAULT_BUILTIN).unwrap());
    let all = cases(&src, &builtin, seed, n);
    for (chunk_index, chunk) in all.chunks(48).enumerate() {
        let label = format!("{} #{chunk_index}", source.label);
        // Each request loads its configs again, as the port does here.
        let (src, src_log) = source.port();
        let (builtin, builtin_log) = capture(|| Config::create_from_file(DEFAULT_BUILTIN).unwrap());
        let mut calls = source.wheel("src");
        let first_case = calls.len() + 1;
        calls.push(
            json!({"call": "CreateFromFile", "on": "Config", "args": [DEFAULT_BUILTIN],
                          "as": "b"}),
        );
        let mut plan = Vec::new();
        for (k, case) in chunk.iter().enumerate() {
            let at = calls.len();
            match case {
                Case::IdentifyBuiltin(name) => {
                    calls.push(json!({"call": "IdentifyBuiltinColorSpace", "on": "Config",
                        "args": [{"ref": "src"}, {"ref": "b"}, name]}));
                    let (made, port_log) = capture(|| port_names(case, &src, &builtin));
                    plan.push((k, case, at, Outcome::Names(made), port_log, Vec::new()));
                }
                Case::IdentifyInterchange(src_name, name) => {
                    calls.push(json!({"call": "IdentifyInterchangeSpace", "on": "Config",
                        "args": [{"ref": "src"}, src_name, {"ref": "b"}, name]}));
                    let (made, port_log) = capture(|| port_names(case, &src, &builtin));
                    plan.push((k, case, at, Outcome::Names(made), port_log, Vec::new()));
                }
                Case::ToBuiltin(src_name, name) | Case::FromBuiltin(name, src_name) => {
                    let proc_name = format!("proc{k}");
                    let (call, args) = if matches!(case, Case::ToBuiltin(..)) {
                        (
                            "GetProcessorToBuiltinColorSpace",
                            json!([{"ref": "src"}, src_name, name]),
                        )
                    } else {
                        (
                            "GetProcessorFromBuiltinColorSpace",
                            json!([name, {"ref": "src"}, src_name]),
                        )
                    };
                    calls.push(json!({"call": call, "on": "Config", "args": args,
                                      "as": proc_name}));
                    let (made, port_log) = capture(|| {
                        if matches!(case, Case::ToBuiltin(..)) {
                            Config::processor_to_builtin_color_space(&src, src_name, name)
                        } else {
                            Config::processor_from_builtin_color_space(name, &src, src_name)
                        }
                    });
                    let applies = match &made {
                        Ok(proc) => apply_calls(&mut calls, &proc_name, proc, &dynamic_sets(proc)),
                        Err(_) => Vec::new(),
                    };
                    plan.push((k, case, at, Outcome::Processor(made), port_log, applies));
                }
            }
        }
        let response = Oracle::get().call("object_calls", json!({"calls": calls}), &[]);
        let result = &response.result;
        let results = result["calls"].as_array().expect("calls");
        assert_eq!(results.len(), calls.len(), "{label}");
        // The configs load and change on both sides, with the same logs.
        let mut wheel_load_log = Vec::new();
        for call in &results[..first_case - 1] {
            assert!(call.get("result").is_some(), "{label}: {call}");
            wheel_load_log.extend(log(&call["log"]));
        }
        assert_eq!(wheel_load_log, src_log, "{label}: the source config's log");
        assert!(results[first_case - 1].get("result").is_some(), "{label}");
        assert_eq!(
            log(&results[first_case - 1]["log"]),
            builtin_log,
            "{label}: the built-in config's log"
        );
        for (k, case, at, made, port_log, applies) in plan {
            let mut problems = Vec::new();
            match made {
                Outcome::Names(port) => {
                    let wheel = &results[at];
                    match (wheel.get("exception"), &port) {
                        (Some(e), Err(p)) => {
                            coverage.name_errors += 1;
                            let message = bytes(&e["message"]);
                            coverage
                                .messages
                                .insert(String::from_utf8_lossy(&message).into_owned());
                            if message != p.what() {
                                problems.push(format!(
                                    "messages {:?} != {:?}",
                                    String::from_utf8_lossy(&message),
                                    p.message()
                                ));
                            }
                        }
                        (None, Ok(p)) => {
                            coverage.names += 1;
                            coverage.found.insert(p.to_string());
                            if wheel["result"] != *p {
                                problems.push(format!("wheel {wheel}, port {p}"));
                            }
                        }
                        _ => problems.push(format!(
                            "wheel {wheel}, port {:?}",
                            port.as_ref().map_err(|e| e.message())
                        )),
                    }
                    if log(&wheel["log"]) != port_log {
                        problems.push(format!("logs {} != {port_log:?}", wheel["log"]));
                    }
                }
                Outcome::Processor(made) => compare_processor_call(
                    &response,
                    results,
                    at,
                    &made,
                    &port_log,
                    applies,
                    &mut coverage.reached,
                    &mut problems,
                ),
            }
            if !problems.is_empty() {
                failures.push(format!(
                    "{label} case {k} {case:?}: {}",
                    problems.join("\n    ")
                ));
            }
        }
    }
}

/// The built-in configs, with their interchange roles and without them.
#[test]
fn builtin_configs_match_the_wheel() {
    let registry = BuiltinConfigRegistry::get();
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    for i in 0..registry.num_builtin_configs() {
        let name = format!("ocio://{}", text(registry.builtin_config_name(i).unwrap()));
        for (k, edits) in [
            vec![],
            vec![
                Edit::Role("aces_interchange", ""),
                Edit::Role("cie_xyz_d65_interchange", ""),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let s = source(format!("{name} {k}"), Load::Builtin(name.clone()), edits);
            check(
                &s,
                0x91a0_0000 + (i * 2 + k) as u64,
                6,
                &mut coverage,
                &mut failures,
            );
        }
    }
    report("config_utils_builtin", &failures);
    assert_eq!(coverage.reached.pixels, 2 * coverage.reached.made);
    assert!(
        coverage.names >= 200 && coverage.name_errors >= 100 && coverage.reached.made >= 40,
        "{} names, {} errors, {} processors",
        coverage.names,
        coverage.name_errors,
        coverage.reached.made
    );
    for message in [
        "The heuristics currently only support scene-referred color spaces.",
        "Heuristics were not able to find an equivalent to the requested color space",
        "Built-in config does not contain the requested color space",
        "Could not find destination color space",
    ] {
        assert!(
            coverage.messages.iter().any(|m| m.contains(message)),
            "{message:?}: {:#?}",
            coverage.messages
        );
    }
}

/// Upstream's two test configs, with inactive lists that leave each heuristic (the sRGB
/// texture spaces, the linear spaces both ways, the v1 sRGB space with a spi1d file) its
/// candidates, and upstream's configs of tests/data/files/configs.
#[test]
fn upstream_test_configs_match_the_wheel() {
    let files = test_files_dir();
    let mut sources = Vec::new();
    for (k, inactive) in [
        "ACES cg, Linear ITU-R BT.709, Texture -- sRGB, OCIO v1 -- sRGB",
        "ACES cg, Linear ITU-R BT.709, OCIO v1 -- sRGB",
        "ACES cg, Texture -- sRGB, OCIO v1 -- sRGB",
        "Linear ITU-R BT.709, Texture -- sRGB, OCIO v1 -- sRGB",
        "ACES cg, Linear ITU-R BT.709, Texture -- sRGB",
        "Texture -- sRGB, ref_cs, OCIO v1 -- sRGB",
        "OCIO v1 -- sRGB",
    ]
    .into_iter()
    .enumerate()
    {
        sources.push(source(
            format!("known {k}"),
            Load::Yaml(KNOWN_CS_CONFIG.to_string()),
            vec![Edit::AddSearchPath(files.clone()), Edit::Inactive(inactive)],
        ));
    }
    sources.push(source(
        "known, display role",
        Load::Yaml(KNOWN_CS_CONFIG.to_string()),
        vec![
            Edit::AddSearchPath(files.clone()),
            Edit::Role("cie_xyz_d65_interchange", "CIE-XYZ-D65"),
            Edit::Inactive("CIE-XYZ-D65"),
        ],
    ));
    for (k, inactive) in [
        "scene-linear Rec.709-sRGB, ACES2065-1",
        "ACES2065-1",
        "ACES2065-1, texture sRGB",
        "texture sRGB, scene-linear Rec.709-sRGB, ACES2065-1",
        "scene-linear P3-D65, texture sRGB, scene-linear Rec.709-sRGB, ACES2065-1",
        "scene-linear P3-D65, texture sRGB, scene-linear Rec.709-sRGB",
        "",
    ]
    .into_iter()
    .enumerate()
    {
        sources.push(source(
            format!("alt {k}"),
            Load::Yaml(ALT_CONFIG.to_string()),
            vec![Edit::Inactive(inactive)],
        ));
    }
    let configs = ocio_testkit::paths::upstream_dir().join("tests/data/files/configs");
    for file in [
        "context_test1/config.ocio",
        "mergeconfigs/base_config.yaml",
        "mergeconfigs/input_config.yaml",
        "mergeconfigs/base_colorspaces_config.yaml",
        "mergeconfigs/input_colorspaces_config.yaml",
        "mergeconfigs/merged1/base1.ocio",
        "mergeconfigs/merged3/base.ocio",
        "mergeconfigs/merged3/input.ocio",
    ] {
        let path = configs.join(file).to_string_lossy().replace('\\', "/");
        sources.push(source(file, Load::File(path), vec![]));
    }
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    for (i, s) in sources.iter().enumerate() {
        check(s, 0x91a0_1000 + i as u64, 6, &mut coverage, &mut failures);
    }
    report("config_utils_upstream", &failures);
    assert_eq!(coverage.reached.pixels, 2 * coverage.reached.made);
    assert!(
        coverage.names >= 200 && coverage.name_errors >= 100 && coverage.reached.made >= 40,
        "{} names, {} errors, {} processors",
        coverage.names,
        coverage.name_errors,
        coverage.reached.made
    );
    // The heuristics found each kind of space upstream's tests name.
    for name in [
        "\"ACES cg\"",
        "\"Texture -- sRGB\"",
        "\"OCIO v1 -- sRGB\"",
        "\"Linear ITU-R BT.709\"",
        "\"scene-linear Rec.2020\"",
        "\"texture sRGB\"",
    ] {
        assert!(
            coverage.found.contains(name),
            "{name}: {:#?}",
            coverage.found
        );
    }
}

/// The generated configs, with the test files as their search path.
#[test]
fn generated_configs_match_the_wheel() {
    let files = test_files_dir();
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    for (i, (label, yaml)) in generated_configs().into_iter().enumerate() {
        let s = source(
            label,
            Load::Yaml(yaml),
            vec![Edit::AddSearchPath(files.clone())],
        );
        check(&s, 0x91a0_2000 + i as u64, 6, &mut coverage, &mut failures);
    }
    report("config_utils_generated", &failures);
    assert_eq!(coverage.reached.pixels, 2 * coverage.reached.made);
    for message in [
        "' refers to a color space that is missing in the source config.",
        "The supplied config does not have a color space for the reference.",
        "The requested space is a data space but the supplied config does not have a data \
         space.",
        "Heuristics were not able to find a known color space in the provided config. Please \
         set the interchange roles.",
    ] {
        assert!(
            coverage.messages.iter().any(|m| m.contains(message)),
            "{message:?}: {:#?}",
            coverage.messages
        );
    }
}

/// The two quirks of the sRGB heuristic, against the wheel: an alias holding "SRGB" in upper
/// case isn't seen (docs/improvements.md, I-377), nor is an sRGB texture space whose
/// from-reference transform has the inverse direction (I-378); the same spaces with the alias
/// in lower case, and written forward, are. Each config's only candidate is that space, so the
/// wheel's identification succeeds or fails on it alone.
#[test]
fn the_srgb_heuristic_quirks_match_the_wheel() {
    let configs: Vec<(String, String)> = generated_configs()
        .into_iter()
        .filter(|(label, _)| {
            [
                "no roles, sRGB",
                "alias SRGB",
                "alias srgb",
                "inverse from-reference",
            ]
            .contains(label)
        })
        .map(|(label, yaml)| (label.to_string(), yaml))
        .collect();
    assert_eq!(configs.len(), 4);
    let mut calls = vec![json!({"call": "CreateFromFile", "on": "Config",
                                "args": [DEFAULT_BUILTIN], "as": "b"})];
    let (builtin, _) = capture(|| Config::create_from_file(DEFAULT_BUILTIN).unwrap());
    let mut port = Vec::new();
    for (k, (_, yaml)) in configs.iter().enumerate() {
        let name = format!("src{k}");
        calls.push(
            json!({"call": "CreateFromStream", "on": "Config", "args": [yaml],
                          "as": name}),
        );
        calls.push(json!({"call": "IdentifyInterchangeSpace", "on": "Config",
                          "args": [{"ref": name}, "raw", {"ref": "b"}, "ACEScg"]}));
        let src = Config::create_from_stream(yaml.as_bytes()).unwrap();
        port.push(
            Config::identify_interchange_space(&src, "raw", &builtin, "ACEScg")
                .map(|(s, b)| json!([text(s), text(b)]))
                .map_err(|e| e.what().to_vec()),
        );
    }
    let response = Oracle::get().call("object_calls", json!({"calls": calls}), &[]);
    let results = response.result["calls"].as_array().expect("calls");
    let mut found = Vec::new();
    for (k, (label, _)) in configs.iter().enumerate() {
        let wheel = &results[2 + 2 * k];
        let wheel = match wheel.get("exception") {
            Some(e) => Err(bytes(&e["message"])),
            None => Ok(wheel["result"].clone()),
        };
        assert_eq!(wheel, port[k], "{label}");
        found.push((label.as_str(), wheel.is_ok()));
    }
    assert_eq!(
        found,
        [
            ("no roles, sRGB", true),
            ("alias SRGB", false),
            ("alias srgb", true),
            ("inverse from-reference", false),
        ],
        "the wheel's identifications"
    );
}

/// The heuristics put the configs' processor caches back as they were (SuspendCacheGuard): after
/// `IdentifyBuiltinColorSpace`, two processors of one transform with a dynamic exposure from the
/// source config, and from the built-in config, are the same processor again, so an exposure set
/// on the first one's CPU processor reaches the second one's pixels. A config whose cache is off
/// shows the difference: its second processor is a new one. The binding has no getter of the
/// cache flags, so the pixels show them.
#[test]
fn the_heuristics_put_the_caches_back() {
    let (_, yaml) = generated_configs()
        .into_iter()
        .find(|(label, _)| *label == "no roles, sRGB")
        .expect("the config");
    let exposure = 1.5f64;
    let mut ec = ExposureContrastTransform::new();
    ec.make_exposure_dynamic();
    let ec = Transform::from(ec);
    let ec_spec = json!({"transform": {"class": "ExposureContrastTransform",
                                       "calls": [["makeExposureDynamic"]]}});
    let pixel_values: Vec<Value> = pixels()
        .iter()
        .map(|&v| json!({"f64": f64::from(v).to_bits()}))
        .collect();

    let src = Config::create_from_stream(yaml.as_bytes()).unwrap();
    let builtin = Config::create_from_file(DEFAULT_BUILTIN).unwrap();
    let off = Config::create_from_stream(yaml.as_bytes()).unwrap();
    off.set_processor_cache_flags(ocio::ProcessorCacheFlags::OFF);
    let identified = Config::identify_builtin_color_space(&src, &builtin, "sRGB - Texture")
        .map(|n| json!(text(n)))
        .unwrap();
    let mut calls = vec![
        json!({"call": "CreateFromStream", "on": "Config", "args": [yaml], "as": "src"}),
        json!({"call": "CreateFromFile", "on": "Config", "args": [DEFAULT_BUILTIN], "as": "b"}),
        json!({"call": "CreateFromStream", "on": "Config", "args": [yaml], "as": "off"}),
        json!({"call": "setProcessorCacheFlags", "on": "off",
               "args": [{"enum": "PROCESSOR_CACHE_OFF"}]}),
        json!({"call": "IdentifyBuiltinColorSpace", "on": "Config",
               "args": [{"ref": "src"}, {"ref": "b"}, "sRGB - Texture"]}),
    ];
    let mut port = Vec::new();
    for (name, config) in [("src", &src), ("b", &builtin), ("off", &off)] {
        let (p1, p2) = (format!("{name}_p1"), format!("{name}_p2"));
        let (c1, c2) = (format!("{name}_c1"), format!("{name}_c2"));
        calls.extend([
            json!({"call": "getProcessor", "on": name, "args": [ec_spec], "as": p1}),
            json!({"call": "getProcessor", "on": name, "args": [ec_spec], "as": p2}),
            json!({"call": "getDefaultCPUProcessor", "on": p1, "as": c1}),
            json!({"call": "getDynamicProperty", "on": c1,
                   "args": [{"enum": "DYNAMIC_PROPERTY_EXPOSURE"}], "as": format!("{name}_dp")}),
            json!({"call": "setDouble", "on": format!("{name}_dp"),
                   "args": [{"f64": exposure.to_bits()}]}),
            json!({"call": "getDefaultCPUProcessor", "on": p2, "as": c2}),
            json!({"call": "applyRGBA", "on": c2, "args": [pixel_values]}),
        ]);
        let pixels_out = (|| -> ocio::Result<Vec<u64>> {
            let first = config.processor(&ec)?.default_cpu_processor()?;
            match first.get_dynamic_property(DynamicPropertyType::Exposure)? {
                DynamicPropertyRcPtr::Double(p) => p.set_value(exposure),
                other => panic!("exposure is no double: {other:?}"),
            }
            let second = config.processor(&ec)?.default_cpu_processor()?;
            let mut data = pixels();
            let width = data.len() / 4;
            {
                let mut img = PackedImageDesc::new(&mut data[..], width, 1, 4)?;
                second.apply(&mut img)?;
            }
            Ok(data.iter().map(|&v| f64::from(v).to_bits()).collect())
        })()
        .unwrap();
        port.push(pixels_out);
    }
    let response = Oracle::get().call("object_calls", json!({"calls": calls}), &[]);
    let results = response.result["calls"].as_array().expect("calls");
    assert_eq!(results.len(), calls.len());
    assert_eq!(results[4]["result"], identified, "the identification");
    let mut wheel = Vec::new();
    for k in 0..3 {
        let applied = &results[5 + 7 * k + 6];
        let bits: Vec<u64> = applied["result"]
            .as_array()
            .unwrap_or_else(|| panic!("{applied}"))
            .iter()
            .map(|v| v["f64"].as_u64().expect("an f64"))
            .collect();
        assert_eq!(bits, port[k], "config {k}");
        wheel.push(bits);
    }
    // The probe tells the caches apart: shared (src, b) and off (off) differ in the wheel.
    assert_eq!(wheel[0], wheel[1]);
    assert_ne!(wheel[0], wheel[2]);
}
