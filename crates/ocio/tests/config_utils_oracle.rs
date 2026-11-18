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
use ocio::transforms::group_transform::GroupTransform;
use ocio::transforms::lut3d_transform::Lut3DTransform;
use ocio::transforms::matrix_transform::MatrixTransform;
use ocio::{
    BuiltinConfigRegistry, ColorSpace, ColorSpaceDirection, ColorSpaceVisibility, Config,
    DynamicPropertyType, ExposureContrastTransform, Processor, SearchReferenceSpaceType, Transform,
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

/// The same matrix, for the transforms built through the API.
const AP0_TO_709_VALUES: [f64; 16] = [
    2.52168618674388,
    -1.13413098823972,
    -0.387555198504164,
    0.0,
    -0.276479914229922,
    1.37271908766826,
    -0.096239173438334,
    0.0,
    -0.0153780649660342,
    -0.152975335867399,
    1.16835340083343,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
];

/// The AP1 to linear Rec.709 matrix: the wrong one for an sRGB texture space of ACES2065-1.
const AP1_TO_709: &str = "[1.70505, -0.62179, -0.08326, 0, -0.13026, 1.14080, -0.01055, 0, \
                          -0.02400, -0.12897, 1.15297, 0, 0, 0, 0, 1]";

/// The AP0 to AP1 matrix.
const AP0_TO_AP1: &str = "[1.4514393161, -0.2365107469, -0.2149285693, 0, -0.0765537734, \
                          1.1762296998, -0.0996759264, 0, 0.0083161484, -0.0060324498, \
                          0.9977163014, 0, 0, 0, 0, 1]";

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
    srgb_space_of(name, extra, AP0_TO_709)
}

/// `srgb_space` with the matrix `matrix`.
fn srgb_space_of(name: &str, extra: &str, matrix: &str) -> String {
    format!(
        "\n  - !<ColorSpace>\n    name: {name}\n{extra}    from_scene_reference: \
         !<GroupTransform>\n      children:\n        - !<MatrixTransform> {{matrix: \
         {matrix}}}\n        - !<ExponentWithLinearTransform> {{gamma: 2.4, offset: \
         0.055, direction: inverse}}\n"
    )
}

/// A linear space named `name` whose from-reference transform is the matrix `matrix`.
fn linear_space_of(name: &str, matrix: &str) -> String {
    format!(
        "\n  - !<ColorSpace>\n    name: {name}\n    from_scene_reference: !<MatrixTransform> \
         {{matrix: {matrix}}}\n"
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
        // ACEScg scaled by 1.0048 and by 1.0052, on each side of IdentifyBuiltinColorSpace's
        // 5e-3 on its test values.
        (
            "edge ACEScg",
            generated(
                "",
                &format!(
                    "{}{}{lin709}",
                    scaled_cg("cg edge in", "1.0048"),
                    scaled_cg("cg edge out", "1.0052")
                ),
            ),
        ),
        // Two sRGB texture spaces that both could match (the first one has AP1's primaries,
        // the second AP0's), then two linear spaces in the same order: each loop of the
        // heuristics stops at its first match, and the reference space it finds follows.
        (
            "two sRGB",
            generated(
                "",
                &format!(
                    "{}{}{}",
                    srgb_space_of("srgb wrong", "", AP1_TO_709),
                    srgb_space("srgb right", ""),
                    linear_space_of("acescg", AP0_TO_AP1)
                ),
            ),
        ),
        (
            "two linear",
            generated(
                "",
                &format!(
                    "{}{}",
                    linear_space_of("lin a", AP1_TO_709),
                    linear_space_of("lin b", AP0_TO_709)
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

/// The port's color space of `Edit::Lut3DSpace`.
fn lut3d_space() -> ColorSpace {
    let mut matrix = MatrixTransform::new();
    matrix.set_matrix(&AP0_TO_709_VALUES);
    let mut group = GroupTransform::new();
    group.append_transform(Transform::from(matrix));
    group.append_transform(Transform::from(Lut3DTransform::new()));
    let mut cs = ColorSpace::new();
    cs.set_name("lut lin709");
    cs.set_transform(
        Some(&Transform::Group(group)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cs
}

/// The wheel's calls of `Edit::Lut3DSpace` on the config stored as `config`.
fn lut3d_space_calls(config: &str) -> Vec<Value> {
    let group = json!({"class": "GroupTransform", "children": [
        {"class": "MatrixTransform", "calls": [["setMatrix", AP0_TO_709_VALUES]]},
        {"class": "Lut3DTransform"},
    ]});
    vec![
        json!({"new": "ColorSpace", "args": [{"enum": "REFERENCE_SPACE_SCENE"}], "as": "lut_cs"}),
        json!({"call": "setName", "on": "lut_cs", "args": ["lut lin709"]}),
        json!({"call": "setTransform", "on": "lut_cs",
               "args": [{"transform": group}, {"enum": "COLORSPACE_DIR_FROM_REFERENCE"}]}),
        json!({"call": "addColorSpace", "on": config, "args": [{"ref": "lut_cs"}]}),
    ]
}

/// A change made to a source config after loading it, on both sides.
#[derive(Clone, Debug)]
enum Edit {
    Role(&'static str, &'static str),
    Inactive(&'static str),
    AddSearchPath(String),
    /// A scene color space "lut lin709" whose from-reference transform is a group of the AP0 to
    /// Rec.709 matrix and a default `Lut3DTransform`, built through the API.
    Lut3DSpace,
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
                    Edit::Lut3DSpace => config.add_color_space(&lut3d_space()).unwrap(),
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
            match edit {
                Edit::Role(role, cs) => {
                    calls.push(json!({"call": "setRole", "on": name, "args": [role, cs]}));
                }
                Edit::Inactive(list) => calls
                    .push(json!({"call": "setInactiveColorSpaces", "on": name, "args": [list]})),
                Edit::AddSearchPath(path) => {
                    calls.push(json!({"call": "addSearchPath", "on": name, "args": [path]}));
                }
                Edit::Lut3DSpace => calls.extend(lut3d_space_calls(name)),
            }
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
    // The unnamed source space with ACEScg: the random names may never ask for this pair, which
    // the configs with two candidates (two sRGB, two linear) tell apart.
    out.push(Case::IdentifyInterchange(
        String::new(),
        "ACEScg".to_string(),
    ));
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

/// A space whose from-reference transform holds a `Lut3DTransform` (built through the API; no
/// config file has a default one): the heuristics don't pick it as an equivalent (the wheel's
/// `IdentifyBuiltinColorSpace` of "Linear Rec.709 (sRGB)" raises that it found none).
#[test]
fn an_api_built_lut3d_space_matches_the_wheel() {
    let s = source(
        "lut3d space",
        Load::Yaml(
            "ocio_profile_version: 2\n\nroles:\n  default: ref\n  aces_interchange: ref\n\n\
             colorspaces:\n  - !<ColorSpace>\n    name: ref\n"
                .to_string(),
        ),
        vec![Edit::Lut3DSpace],
    );
    let mut coverage = Coverage::default();
    let mut failures = Vec::new();
    check(&s, 0x91a0_3000, 6, &mut coverage, &mut failures);
    report("config_utils_lut3d_space", &failures);
    assert!(
        coverage.messages.iter().any(|m| m.contains(
            "Heuristics were not able to find an equivalent to the requested color space: \
             Linear Rec.709 (sRGB)."
        )),
        "{:#?}",
        coverage.messages
    );
}

// LocateBuiltinColorSpace (2.6) ////////////////////////////////////////////////////////////////

/// The configs `LocateBuiltinColorSpace` searches in the sweep, as the wheel and the port load
/// them: built-in configs both have, and the raw config, whose interchange spaces can't be
/// identified. (The 2.6 default and latest built-in configs, cg-config-v5.0.0, are card
/// ocio26-ops'.)
const LOCATE_TARGETS: [&str; 4] = [
    "ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
    "ocio://studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
    "ocio://cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
    "raw",
];

/// A target of [`LOCATE_TARGETS`], on the port's side.
fn locate_target(uri: &str) -> Config {
    if uri == "raw" {
        (*Config::create_raw().unwrap()).clone()
    } else {
        (*Config::create_from_file(uri).unwrap()).clone()
    }
}

/// The wheel's call that loads a target of [`LOCATE_TARGETS`], stored as `name`.
fn locate_target_call(uri: &str, name: &str) -> Value {
    if uri == "raw" {
        json!({"call": "CreateRaw", "on": "Config", "as": name})
    } else {
        json!({"call": "CreateFromFile", "on": "Config", "args": [uri], "as": name})
    }
}

/// What the cases of `LocateBuiltinColorSpace` reached.
#[derive(Default)]
struct LocateCoverage {
    found: usize,
    empty: usize,
    errors: usize,
    names: BTreeSet<String>,
    messages: BTreeSet<String>,
}

/// Compares one `LocateBuiltinColorSpace` call: the wheel's entry `wheel` with the port's
/// outcome and log.
fn compare_locate(
    wheel: &Value,
    port: &ocio::Result<String>,
    port_log: &[Vec<u8>],
    coverage: &mut LocateCoverage,
    problems: &mut Vec<String>,
) {
    match (wheel.get("exception"), port) {
        (Some(e), Err(p)) => {
            assert!(wheel.get("result").is_none(), "{wheel}");
            coverage.errors += 1;
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
            if p.is_empty() {
                coverage.empty += 1;
            } else {
                coverage.found += 1;
                coverage.names.insert(p.clone());
            }
            if wheel["result"] != json!(p) {
                problems.push(format!("wheel {wheel}, port {p:?}"));
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

/// The port's `LocateBuiltinColorSpace`, as the binding returns it, and its log.
fn port_locate(src: &Config, name: &str, builtin: &Config) -> (ocio::Result<String>, Vec<Vec<u8>>) {
    capture(|| Config::locate_builtin_color_space(src, name, builtin).map(text))
}

/// `LocateBuiltinColorSpace` of every name of `source` (inactive and display-referred ones,
/// aliases, roles, names cut by a NUL, a missing name and `""`) in each of
/// [`LOCATE_TARGETS`], against the wheel: the name or the exception's message, and the log.
fn check_locate(source: &Source, coverage: &mut LocateCoverage, failures: &mut Vec<String>) {
    let (src, src_log) = source.port();
    let mut names = all_names(&src);
    if let Some(first) = names.first().cloned() {
        names.push(format!("{first}\0tail"));
    }
    names.extend(
        [
            "scene_linear",
            "aces_interchange",
            "default",
            "no_such_space",
            "no_such_space\0tail",
            "\0tail",
            "",
        ]
        .map(String::from),
    );
    let mut calls = source.wheel("src");
    let loaded = calls.len();
    let mut plan = Vec::new();
    for (j, uri) in LOCATE_TARGETS.iter().enumerate() {
        let b = format!("b{j}");
        let (builtin, builtin_log) = capture(|| locate_target(uri));
        calls.push(locate_target_call(uri, &b));
        plan.push((calls.len() - 1, None, Ok(String::new()), builtin_log));
        for name in &names {
            calls.push(json!({"call": "LocateBuiltinColorSpace", "on": "Config",
                              "args": [{"ref": "src"}, name, {"ref": b}]}));
            let (made, port_log) = port_locate(&src, name, &builtin);
            plan.push((calls.len() - 1, Some((*uri, name.clone())), made, port_log));
        }
    }
    let response = Oracle::get().call("object_calls", json!({"calls": calls}), &[]);
    let results = response.result["calls"].as_array().expect("calls");
    assert_eq!(results.len(), calls.len(), "{}", source.label);
    let mut wheel_load_log = Vec::new();
    for call in &results[..loaded] {
        assert!(call.get("result").is_some(), "{}: {call}", source.label);
        wheel_load_log.extend(log(&call["log"]));
    }
    assert_eq!(
        wheel_load_log, src_log,
        "{}: the source config's log",
        source.label
    );
    for (at, case, made, port_log) in plan {
        let wheel = &results[at];
        let Some((uri, name)) = case else {
            assert!(wheel.get("result").is_some(), "{}: {wheel}", source.label);
            assert_eq!(
                log(&wheel["log"]),
                port_log,
                "{}: the target's log",
                source.label
            );
            continue;
        };
        let mut problems = Vec::new();
        compare_locate(wheel, &made, &port_log, coverage, &mut problems);
        if !problems.is_empty() {
            failures.push(format!(
                "{} {uri} {name:?}: {}",
                source.label,
                problems.join("\n    ")
            ));
        }
    }
}

/// `LocateBuiltinColorSpace` against the wheel, from the built-in configs (with and without
/// their interchange roles), upstream's test configs of `processor_to_known_colorspace*` (with
/// the interchange roles, without them, and with inactive lists that leave the heuristics
/// their candidates or none) and the generated configs.
#[test]
fn locate_builtin_color_space_matches_the_wheel() {
    let files = test_files_dir();
    let mut sources = Vec::new();
    let registry = BuiltinConfigRegistry::get();
    for i in 0..registry.num_builtin_configs() {
        let name = format!("ocio://{}", text(registry.builtin_config_name(i).unwrap()));
        sources.push(source(name.clone(), Load::Builtin(name.clone()), vec![]));
        sources.push(source(
            format!("{name} without roles"),
            Load::Builtin(name),
            vec![
                Edit::Role("aces_interchange", ""),
                Edit::Role("cie_xyz_d65_interchange", ""),
            ],
        ));
    }
    for (k, edits) in [
        vec![
            Edit::Role("aces_interchange", "ref_cs"),
            Edit::Role("cie_xyz_d65_interchange", "CIE-XYZ-D65"),
            Edit::Inactive(""),
        ],
        vec![
            Edit::Role("aces_interchange", "ref_cs"),
            Edit::Role("cie_xyz_d65_interchange", "CIE-XYZ-D65"),
            Edit::Inactive("ref_cs"),
        ],
        vec![Edit::Role("aces_interchange", "Texture -- sRGB")],
        vec![Edit::Inactive("")],
        vec![Edit::Inactive(
            "ACES cg, Linear ITU-R BT.709, Texture -- sRGB, OCIO v1 -- sRGB",
        )],
        vec![Edit::Inactive(
            "ACES cg, Linear ITU-R BT.709, Texture -- sRGB, OCIO v1 -- sRGB, ref_cs",
        )],
    ]
    .into_iter()
    .enumerate()
    {
        let mut all = vec![Edit::AddSearchPath(files.clone())];
        all.extend(edits);
        sources.push(source(
            format!("known {k}"),
            Load::Yaml(KNOWN_CS_CONFIG.to_string()),
            all,
        ));
    }
    for (k, inactive) in [
        "",
        "ACES2065-1",
        "texture sRGB, scene-linear Rec.709-sRGB, ACES2065-1",
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
    for (label, yaml) in generated_configs() {
        sources.push(source(
            label,
            Load::Yaml(yaml),
            vec![Edit::AddSearchPath(files.clone())],
        ));
    }
    let mut coverage = LocateCoverage::default();
    let mut failures = Vec::new();
    for s in &sources {
        check_locate(s, &mut coverage, &mut failures);
    }
    report("locate_builtin_color_space", &failures);
    assert!(
        coverage.found >= 500 && coverage.empty >= 100 && coverage.errors >= 200,
        "{} found, {} empty, {} errors",
        coverage.found,
        coverage.empty,
        coverage.errors
    );
    for name in [
        "ACEScg",
        "Raw",
        "raw",
        "sRGB - Display",
        "sRGB Encoded Rec.709 (sRGB)",
    ] {
        assert!(
            coverage.names.contains(name),
            "{name}: {:#?}",
            coverage.names
        );
    }
    for message in [
        "LocateBuiltinColorSpace: arguments must not be null.",
        "LocateBuiltinColorSpace: Source config does not contain the requested color space: \
         no_such_space.",
        "Heuristics were not able to find an interchange space in the source config, so it is \
         not possible to search for the color space: ",
        "Heuristics were not able to find an interchange space in the built-in config.",
    ] {
        assert!(
            coverage.messages.iter().any(|m| m.contains(message)),
            "{message:?}: {:#?}",
            coverage.messages
        );
    }
}

/// A step of a `LocateBuiltinColorSpace` script, on both sides; configs by name.
#[derive(Clone, Debug)]
enum Step {
    /// `setRole` on the config.
    Role(&'static str, &'static str, &'static str),
    /// `setInactiveColorSpaces` on the config.
    Inactive(&'static str, &'static str),
    /// Renames a color space of the config: an editable copy of it renamed, the color space
    /// removed and the copy added.
    Rename(&'static str, &'static str, &'static str),
    /// `createEditableCopy` of a config, stored as the second name.
    Copy(&'static str, &'static str),
    /// `LocateBuiltinColorSpace(src, name, builtin)`.
    Locate(&'static str, &'static str, &'static str),
}

/// The cached test values and fingerprints follow the configs' changes, against the wheel:
/// upstream's script of `processor_to_known_colorspace` (ColorSpace_tests.cpp:1927-2088 @
/// v2.6.0), each step's result compared, with the built-in copy's interchange role moved too.
#[test]
fn locate_builtin_color_space_follows_edits() {
    use Step::*;
    const TARGET: &str = "ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5";
    let steps = [
        Role("src", "aces_interchange", "ref_cs"),
        Role("src", "cie_xyz_d65_interchange", "CIE-XYZ-D65"),
        Inactive("src", ""),
        Locate("src", "ACES cg", "b"),
        Locate("src", "sRGB - Display CS", "b"),
        Role("src", "aces_interchange", "Texture -- sRGB"),
        Locate("src", "ACES cg", "b"),
        Role("src", "aces_interchange", "ref_cs"),
        Locate("src", "ACES cg", "b"),
        Copy("b", "b2"),
        Locate("src", "ACES cg", "b2"),
        Rename("b2", "ACEScg", "ACES cg 2"),
        Locate("src", "ACES cg", "b2"),
        Role("b2", "aces_interchange", "ACEScct"),
        Locate("src", "ACES cg", "b2"),
        Role("src", "aces_interchange", ""),
        Role("src", "cie_xyz_d65_interchange", ""),
        Locate("src", "ACES cg", "b"),
        Locate("src", "sRGB - Display CS", "b"),
        Rename("src", "CIE-XYZ-D65", "Not the interchange space"),
        Locate("src", "sRGB - Display CS", "b"),
        Locate("src", "ACES cg", "b"),
        Rename("src", "Not the interchange space", "CIE-XYZ-D65"),
        Locate("src", "sRGB - Display CS", "b"),
        Inactive(
            "src",
            "ACES cg, Linear ITU-R BT.709, Texture -- sRGB, OCIO v1 -- sRGB, ref_cs",
        ),
        Locate("src", "ACES cg", "b"),
        Inactive("src", ""),
        Locate("src", "ACES cg", "b"),
    ];
    let src_source = source(
        "known",
        Load::Yaml(KNOWN_CS_CONFIG.to_string()),
        vec![Edit::AddSearchPath(test_files_dir())],
    );
    let (src, _) = src_source.port();
    let mut configs: std::collections::BTreeMap<&str, Config> =
        [("src", (*src).clone()), ("b", locate_target(TARGET))]
            .into_iter()
            .collect();
    let mut calls = src_source.wheel("src");
    calls.push(locate_target_call(TARGET, "b"));
    let mut plan = Vec::new();
    for (k, step) in steps.iter().enumerate() {
        match *step {
            Role(c, role, cs) => {
                calls.push(json!({"call": "setRole", "on": c, "args": [role, cs]}));
                let config = configs.get_mut(c).unwrap();
                config.set_role(role, Some(cs.as_bytes())).unwrap();
            }
            Inactive(c, list) => {
                calls.push(json!({"call": "setInactiveColorSpaces", "on": c, "args": [list]}));
                configs.get_mut(c).unwrap().set_inactive_color_spaces(list);
            }
            Rename(c, old, new) => {
                let (got, copy) = (format!("cs{k}"), format!("copy{k}"));
                calls.extend([
                    json!({"call": "getColorSpace", "on": c, "args": [old], "as": got}),
                    json!({"copy": got, "as": copy}),
                    json!({"call": "setName", "on": copy, "args": [new]}),
                    json!({"call": "removeColorSpace", "on": c, "args": [old]}),
                    json!({"call": "addColorSpace", "on": c, "args": [{"ref": copy}]}),
                ]);
                let config = configs.get_mut(c).unwrap();
                let mut renamed = config.color_space(old).unwrap().clone();
                renamed.set_name(new);
                config.remove_color_space(old);
                config.add_color_space(&renamed).unwrap();
            }
            Copy(from, to) => {
                calls.push(json!({"copy": from, "as": to}));
                let copy = configs[from].create_editable_copy().unwrap();
                configs.insert(to, copy);
            }
            Locate(s, name, b) => {
                calls.push(json!({"call": "LocateBuiltinColorSpace", "on": "Config",
                                  "args": [{"ref": s}, name, {"ref": b}]}));
                let (made, port_log) = port_locate(&configs[s], name, &configs[b]);
                plan.push((k, calls.len() - 1, made, port_log));
            }
        }
    }
    let response = Oracle::get().call("object_calls", json!({"calls": calls}), &[]);
    let results = response.result["calls"].as_array().expect("calls");
    assert_eq!(results.len(), calls.len());
    let located: BTreeSet<usize> = plan.iter().map(|(_, at, _, _)| *at).collect();
    for (at, call) in results.iter().enumerate() {
        if !located.contains(&at) {
            assert!(call.get("exception").is_none(), "call {at}: {call}");
            assert!(call.get("result").is_some(), "call {at}: {call}");
            assert!(log(&call["log"]).is_empty(), "call {at}: {call}");
        }
    }
    let mut coverage = LocateCoverage::default();
    let mut failures = Vec::new();
    let mut outcomes = Vec::new();
    for (k, at, made, port_log) in &plan {
        let mut problems = Vec::new();
        compare_locate(&results[*at], made, port_log, &mut coverage, &mut problems);
        if !problems.is_empty() {
            failures.push(format!(
                "step {k} {:?}: {}",
                steps[*k],
                problems.join("\n    ")
            ));
        }
        outcomes.push(match results[*at].get("exception") {
            Some(_) => "error".to_string(),
            None => results[*at]["result"].as_str().unwrap_or("?").to_string(),
        });
    }
    report("locate_builtin_color_space_edits", &failures);
    assert_eq!(plan.len(), 14);
    // The steps reached each kind of outcome in the wheel: the moved role changed the answer,
    // the renamed built-in space was found by its new name, and the source config lost its
    // interchange spaces.
    assert_ne!(outcomes[2], outcomes[3], "{outcomes:?}");
    assert!(coverage.names.contains("ACES cg 2"), "{outcomes:?}");
    assert!(coverage.errors >= 2, "{outcomes:?}");
}
