// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociocheck` against the wheel's: stdout, stderr, exit code and the config it writes.

mod common;

use std::path::Path;

use common::{Case, sweep, test_file};
use ocio_testkit::paths;

const TOOL: &str = "ociocheck";
const PROGRAM: &str = env!("CARGO_BIN_EXE_ociocheck");

/// A config that ociocheck passes without warnings, the base of the broken ones.
const GOOD: &str = r#"ocio_profile_version: 2.5

environment:
  SHOT: shot1
  SEQ: seq1
search_path:
  - @DATA@
  - luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]
description: Good.

roles:
  default: raw
  scene_linear: linear
  color_timing: log
  compositing_log: log
  aces_interchange: linear
  cie_xyz_d65_interchange: xyz
  my_own_role: log

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: Shared, view_transform: vt, display_colorspace: <USE_DISPLAY_NAME>}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Look, colorspace: lut, looks: warm}
    - !<Views> [Shared]
  xyz:
    - !<View> {name: Raw, colorspace: raw}
    - !<Views> [Shared]

active_displays: [sRGB]
active_views: [Raw]
inactive_colorspaces: [hidden]

looks:
  - !<Look>
    name: warm
    process_space: linear
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}
  - !<Look>
    name: cool
    process_space: linear
    inverse_transform: !<CDLTransform> {slope: [0.9, 1, 1.1]}

view_transforms:
  - !<ViewTransform>
    name: vt
    from_scene_reference: !<MatrixTransform> {matrix: [0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1]}

named_transforms:
  - !<NamedTransform>
    name: nt
    categories: [a]
    transform: !<LogTransform> {base: 2}
  - !<NamedTransform>
    name: nt_inverse
    categories: [a]
    inverse_transform: !<ExponentTransform> {value: 2.2}

colorspaces:
  - !<ColorSpace>
    name: raw
    categories: [a]
    isdata: true

  - !<ColorSpace>
    name: linear
    categories: [a]
    aliases: [lin_ap0_scene]
    interop_id: lin_ap0_scene

  - !<ColorSpace>
    name: log
    categories: [a]
    aliases: ["mycompany:log"]
    interop_id: mycompany:log
    from_scene_reference: !<LogAffineTransform> {base: 2, logSideSlope: 0.1, logSideOffset: 0.5}

  - !<ColorSpace>
    name: lut
    categories: [a]
    from_scene_reference: !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}

  - !<ColorSpace>
    name: hidden
    to_scene_reference: !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}

display_colorspaces:
  - !<ColorSpace>
    name: xyz
    categories: [a]
    aliases: [srgb_rec709_display]
    interop_id: srgb_rec709_display
    from_display_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}

  - !<ColorSpace>
    name: sRGB
    categories: [a]
    from_display_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1]}
"#;

/// `GOOD` with its search path, which holds upstream's LUT files. They are read where they are,
/// not copied into the case's directory: on Linux the second half of `CacheID` covers the
/// device and inode of the files a config refers to (`Platform::CreateFileContentHash`,
/// Platform.cpp:333-357 @ v2.5.2), and a copy would have another inode in each run.
fn good() -> String {
    let lut = test_file("lut1d_1.spi1d");
    GOOD.replace("@DATA@", lut.strip_suffix("/lut1d_1.spi1d").unwrap())
}

/// A directory the case's file names can run into.
fn with_files(case: Case) -> Case {
    case.file("luts/note.txt", "a file")
}

/// `good()` with text replaced, which must be there.
fn good_with(from: &str, to: &str) -> String {
    let good = good();
    assert!(good.contains(from), "{from}");
    good.replacen(from, to, 1)
}

fn config_case(label: &str, config: impl Into<Vec<u8>>) -> Case {
    with_files(Case::new(label, &["--iconfig", "a.ocio"]).file("a.ocio", config))
}

#[test]
fn bad_arguments() {
    let cases: Vec<Case> = [
        ("help", vec!["--help"]),
        ("help_one_dash", vec!["-help"]),
        ("h_is_invalid", vec!["-h"]),
        ("invalid", vec!["--bogus"]),
        ("positional", vec!["a.ocio"]),
        ("iconfig_missing_parameter", vec!["--iconfig"]),
        ("oconfig_missing_parameter", vec!["--oconfig"]),
        ("iconfig_one_dash_missing", vec!["-iconfig"]),
        ("i_missing_parameter", vec!["-i"]),
        ("i_missing_file", vec!["-i", "not_there.ocio"]),
        (
            "i_then_iconfig",
            vec!["-i", "first.ocio", "--iconfig", "second.ocio"],
        ),
        (
            "iconfig_then_i",
            vec!["--iconfig", "first.ocio", "-i", "second.ocio"],
        ),
        ("i_joined", vec!["-inot_there.ocio"]),
        ("v_with_parameter", vec!["-v", "not_there.ocio"]),
        (
            "v_twice_missing_file",
            vec!["-v", "-v", "-i", "not_there.ocio"],
        ),
        ("verbose_long", vec!["--verbose"]),
        ("help_after_error", vec!["--bogus", "--help"]),
        ("error_after_help", vec!["--help", "--bogus"]),
        ("missing_file", vec!["--iconfig", "not_there.ocio"]),
        ("empty_iconfig", vec!["--iconfig", ""]),
        ("directory", vec!["--iconfig", "."]),
    ]
    .into_iter()
    .map(|(label, argv)| Case::new(label, &argv))
    .collect();
    let summary = sweep("ociocheck_bad_arguments", TOOL, PROGRAM, &cases);
    assert!(summary.with_exit_code(1) >= 19);
}

#[test]
fn environment() {
    let with_good = |case: Case| with_files(case.file("a.ocio", good()));
    let cases = vec![
        Case::new("no_arguments", &[]),
        Case::new("empty_ocio", &[]).env("OCIO", ""),
        Case::new("missing_ocio", &[]).env("OCIO", "not_there.ocio"),
        with_good(Case::new("ocio", &[])).env("OCIO", "a.ocio"),
        with_good(Case::new("iconfig_wins", &["--iconfig", "a.ocio"]))
            .env("OCIO", "not_there.ocio"),
        with_good(Case::new("logging_none", &["--iconfig", "a.ocio"]))
            .env("OCIO_LOGGING_LEVEL", "none"),
        with_good(Case::new("logging_warning", &["--iconfig", "a.ocio"]))
            .env("OCIO_LOGGING_LEVEL", "warning"),
        with_good(Case::new("logging_info", &["--iconfig", "a.ocio"]))
            .env("OCIO_LOGGING_LEVEL", "info"),
        with_good(Case::new("context_variable", &["--iconfig", "a.ocio"])).env("SHOT", "shot2"),
        with_good(Case::new("logging_debug", &["--iconfig", "a.ocio"]))
            .env("OCIO_LOGGING_LEVEL", "debug"),
        with_good(Case::new("logging_bogus", &["--iconfig", "a.ocio"]))
            .env("OCIO_LOGGING_LEVEL", "bogus"),
    ];
    let summary = sweep("ociocheck_environment", TOOL, PROGRAM, &cases);
    assert!(summary.with_exit_code(0) >= 6);
}

#[test]
fn checks_of_a_config() {
    let lut = "!<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}";
    let missing = "!<FileTransform> {src: not_there.spi1d}";
    let cases = vec![
        config_case("good", good()),
        config_case("not_a_config", "not a config"),
        config_case("empty", ""),
        config_case(
            "version_1",
            "ocio_profile_version: 1\nroles:\n  default: raw\ndisplays:\n  d:\n    - !<View> {name: v, colorspace: raw}\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n",
        ),
        config_case(
            "no_displays",
            "ocio_profile_version: 2\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n",
        ),
        config_case(
            "no_colorspaces",
            "ocio_profile_version: 2\nroles:\n  default: raw\ndisplays:\n  d:\n    - !<View> {name: v, colorspace: raw}\n",
        ),
        config_case(
            "no_environment",
            good_with("environment:\n  SHOT: shot1\n  SEQ: seq1\n", ""),
        ),
        config_case(
            "empty_environment",
            good_with(
                "environment:\n  SHOT: shot1\n  SEQ: seq1\n",
                "environment: {}\n",
            ),
        ),
        config_case(
            "role_without_space",
            good_with("my_own_role: log", "my_own_role: nope"),
        ),
        config_case(
            "missing_scene_linear",
            good_with("  scene_linear: linear\n", ""),
        ),
        config_case(
            "missing_color_timing",
            good_with("  color_timing: log\n", ""),
        ),
        config_case(
            "missing_compositing_log",
            good_with("  compositing_log: log\n", ""),
        ),
        config_case(
            "missing_interchange",
            good_with("  aces_interchange: linear\n", ""),
        ),
        config_case(
            "missing_display_interchange",
            good_with("  cie_xyz_d65_interchange: xyz\n", ""),
        ),
        config_case(
            "old_version_without_roles",
            good_with(
                "  aces_interchange: linear\n  cie_xyz_d65_interchange: xyz\n",
                "",
            )
            .replacen("ocio_profile_version: 2.5", "ocio_profile_version: 2.1", 1)
            .replace("    interop_id: lin_ap0_scene\n", "")
            .replace("    interop_id: mycompany:log\n", "")
            .replace("    interop_id: srgb_rec709_display\n", ""),
        ),
        config_case(
            "view_missing_lut",
            good_with(
                &format!("name: lut\n    categories: [a]\n    from_scene_reference: {lut}"),
                &format!("name: lut\n    categories: [a]\n    from_scene_reference: {missing}"),
            ),
        ),
        config_case(
            "view_missing_colorspace",
            good_with(
                "{name: Raw, colorspace: raw}",
                "{name: Raw, colorspace: nope}",
            ),
        ),
        config_case(
            "view_missing_look",
            good_with("looks: warm}", "looks: nope}"),
        ),
        config_case(
            "shared_view_missing_transform",
            good_with("view_transform: vt,", "view_transform: nope,"),
        ),
        config_case(
            "hidden_missing_lut",
            good_with(
                "to_scene_reference: !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}",
                &format!("to_scene_reference: {missing}"),
            ),
        ),
        config_case(
            "both_directions_missing",
            good_with(
                "to_scene_reference: !<FileTransform> {src: lut1d_2.spi1d, interpolation: linear}",
                &format!("to_scene_reference: {missing}\n    from_scene_reference: {missing}"),
            ),
        ),
        config_case(
            "some_categories",
            good_with("name: log\n    categories: [a]\n", "name: log\n"),
        ),
        config_case("no_categories", good().replace("    categories: [a]\n", "")),
        config_case("hidden_has_no_category", good()),
        config_case(
            "named_transform_missing_lut",
            good_with(
                "transform: !<LogTransform> {base: 2}",
                &format!("transform: {missing}"),
            ),
        ),
        config_case(
            "named_transform_both_missing",
            good_with(
                "transform: !<LogTransform> {base: 2}",
                &format!("transform: {missing}\n    inverse_transform: {missing}"),
            ),
        ),
        config_case(
            "named_transform_some_categories",
            good_with(
                "name: nt_inverse\n    categories: [a]\n",
                "name: nt_inverse\n",
            ),
        ),
        config_case(
            "named_transform_inactive",
            good_with(
                "inactive_colorspaces: [hidden]",
                "inactive_colorspaces: [hidden, nt]",
            ),
        ),
        config_case(
            "no_named_transforms",
            good()
                .split_once("named_transforms:\n")
                .map(|(a, b)| {
                    format!(
                        "{a}colorspaces:\n{}",
                        b.split_once("colorspaces:\n").unwrap().1
                    )
                })
                .unwrap(),
        ),
        config_case(
            "look_missing_lut",
            good_with(
                "transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}",
                &format!("transform: {missing}"),
            ),
        ),
        config_case(
            "look_inverse_missing_lut",
            good_with(
                "inverse_transform: !<CDLTransform> {slope: [0.9, 1, 1.1]}",
                &format!("inverse_transform: {missing}"),
            ),
        ),
        config_case(
            "look_both_missing",
            good_with(
                "transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}",
                &format!("transform: {missing}\n    inverse_transform: {missing}"),
            ),
        ),
        config_case(
            "no_looks",
            good()
                .split_once("looks:\n  - !<Look>")
                .map(|(a, b)| {
                    format!(
                        "{a}{}",
                        b.split_once("view_transforms:")
                            .map(|(_, c)| format!("view_transforms:{c}"))
                            .unwrap()
                    )
                })
                .unwrap(),
        ),
        config_case(
            "interop_invalid",
            good_with("interop_id: lin_ap0_scene", "interop_id: not_a_standard_id"),
        ),
        config_case(
            "interop_standard_in_namespace",
            good_with(
                "interop_id: lin_ap0_scene",
                "interop_id: mine:lin_ap0_scene",
            ),
        ),
        config_case(
            "interop_display_in_namespace",
            good_with(
                "interop_id: lin_ap0_scene",
                "interop_id: mine:pq_p3d65_display",
            ),
        ),
        config_case(
            "interop_empty_namespace",
            good_with("interop_id: lin_ap0_scene", "interop_id: \":x\""),
        ),
        config_case(
            "interop_empty_id",
            good_with("interop_id: lin_ap0_scene", "interop_id: \"x:\""),
        ),
        config_case(
            "interop_two_colons",
            good_with("interop_id: lin_ap0_scene", "interop_id: a:b:data"),
        ),
        config_case(
            "interop_two_warnings",
            good_with("interop_id: lin_ap0_scene", "interop_id: a:data").replacen(
                "interop_id: mycompany:log",
                "interop_id: b:unknown",
                1,
            ),
        ),
        config_case(
            "validation_fails",
            good_with("default: raw", "default: nope"),
        ),
        config_case(
            "validation_warns",
            good_with("active_views: [Raw]", "active_views: [Raw, Nope]"),
        ),
        config_case(
            "validation_duplicate_alias",
            good_with("name: raw\n", "name: raw\n    aliases: [linear]\n"),
        ),
        config_case(
            "unknown_key_warns_on_load",
            good_with("description: Good.", "description: Good.\nnot_a_key: 1"),
        ),
        config_case(
            "view_needing_a_context_variable",
            good_with("src: lut1d_1.spi1d", "src: $SHOT/lut1d_1.spi1d"),
        ),
        config_case(
            "description_with_return",
            good_with("description: Good.", "description: \"line\\r\\nnext\""),
        ),
        config_case(
            "carriage_return_in_a_name",
            good_with("name: hidden\n", "name: \"hid\\rden\"\n").replacen(
                "inactive_colorspaces: [hidden]",
                "inactive_colorspaces: []",
                1,
            ),
        ),
    ];
    let summary = sweep("ociocheck_config_checks", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 8,
        "{}",
        summary.with_exit_code(0)
    );
    assert!(
        summary.with_exit_code(1) >= 15,
        "{}",
        summary.with_exit_code(1)
    );
}

#[test]
fn output_config() {
    let cases = vec![
        with_files(Case::new("written", &["--iconfig", "a.ocio", "--oconfig", "b.ocio"]))
            .file("a.ocio", good()),
        with_files(Case::new("over_itself", &["--iconfig", "a.ocio", "--oconfig", "a.ocio"]))
            .file("a.ocio", good()),
        with_files(Case::new("no_directory", &["--iconfig", "a.ocio", "--oconfig", "no/b.ocio"]))
            .file("a.ocio", good()),
        with_files(Case::new("into_a_directory", &["--iconfig", "a.ocio", "--oconfig", "luts"]))
            .file("a.ocio", good()),
        with_files(Case::new("from_env", &["--oconfig", "b.ocio"]))
            .file("a.ocio", good())
            .env("OCIO", "a.ocio"),
        with_files(Case::new("failed_checks", &["--iconfig", "a.ocio", "--oconfig", "b.ocio"]))
            .file("a.ocio", good_with("default: raw", "default: nope")),
        Case::new("builtin", &["--iconfig", "ocio://default", "--oconfig", "b.ocio"]),
        Case::new("load_fails", &["--iconfig", "not_there.ocio", "--oconfig", "b.ocio"]),
        with_files(Case::new("version_1_written", &["--iconfig", "a.ocio", "--oconfig", "b.ocio"]))
            .file("a.ocio", "ocio_profile_version: 1\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n    from_reference: !<RangeTransform> {minInValue: 0, minOutValue: 0}\n"),
    ];
    let summary = sweep("ociocheck_output", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 5, "{}", summary.writing_files());
}

/// `good()` with each (from, to) replaced once; each `from` must be there.
fn good_with_all(replacements: &[(&str, &str)]) -> String {
    let mut config = good();
    for (from, to) in replacements {
        assert!(config.contains(from), "{from}");
        config = config.replacen(from, to, 1);
    }
    config
}

/// What makes `GOOD` meet `CONFIG_HDR_DISPLAY_SUPPORT_26`: an encoding on each display color
/// space, an interop ID on `sRGB`, and an active view with a view transform.
const HDR_26: [(&str, &str); 3] = [
    ("active_views: [Raw]", "active_views: [Raw, Shared]"),
    (
        "    interop_id: srgb_rec709_display\n",
        "    interop_id: srgb_rec709_display\n    encoding: sdr-video\n",
    ),
    (
        "    name: sRGB\n    categories: [a]\n",
        "    name: sRGB\n    categories: [a]\n    interop_id: g22_rec709_display\n    encoding: sdr-video\n",
    ),
];

/// `HDR_26` and then `more`.
fn hdr_26_with(more: &[(&str, &str)]) -> String {
    let all: Vec<(&str, &str)> = HDR_26.iter().chain(more).copied().collect();
    good_with_all(&all)
}

/// The Compatibility section of 2.6.0: each reason `CheckHDRDisplaySupport26` gives, with and
/// without `-v`, which prints the reasons.
#[test]
fn compatibility() {
    let case = |label: &str, verbose: bool, config: String| {
        let argv: &[&str] = if verbose {
            &["-v", "--iconfig", "a.ocio"]
        } else {
            &["--iconfig", "a.ocio"]
        };
        with_files(Case::new(label, argv).file("a.ocio", config))
    };
    let both = |label: &str, config: String| {
        [
            case(label, false, config.clone()),
            case(&format!("{label}_verbose"), true, config),
        ]
    };
    let no_display_colorspaces = "ocio_profile_version: 2\nroles:\n  default: raw\n  cie_xyz_d65_interchange: raw\ndisplays:\n  d:\n    - !<View> {name: v, colorspace: raw}\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n    isdata: true\n";
    let mut cases: Vec<Case> = [
        both("good", good()),
        both("hdr_26", hdr_26_with(&[])),
        both(
            "no_interchange_role",
            hdr_26_with(&[("  cie_xyz_d65_interchange: xyz\n", "")]),
        ),
        both(
            "no_interop_id",
            hdr_26_with(&[("    interop_id: g22_rec709_display\n", "")]),
        ),
        both(
            "empty_encoding",
            hdr_26_with(&[(
                "    interop_id: g22_rec709_display\n    encoding: sdr-video\n",
                "    interop_id: g22_rec709_display\n    encoding: \"\"\n",
            )]),
        ),
        both(
            "display_without_colorspace",
            hdr_26_with(&[
                ("active_displays: [sRGB]", "active_displays: [sRGB, other]"),
                (
                    "  xyz:\n",
                    "  other:\n    - !<View> {name: Raw, colorspace: raw}\n  xyz:\n",
                ),
            ]),
        ),
        both(
            "display_named_after_a_scene_colorspace",
            hdr_26_with(&[
                ("active_displays: [sRGB]", "active_displays: [sRGB, log]"),
                (
                    "  xyz:\n",
                    "  log:\n    - !<View> {name: Raw, colorspace: raw}\n  xyz:\n",
                ),
            ]),
        ),
        both(
            "view_without_view_transform",
            hdr_26_with(&[("active_views: [Raw, Shared]", "active_views: [Raw, Look, Shared]")]),
        ),
        both(
            "named_transform_view",
            hdr_26_with(&[
                ("active_views: [Raw, Shared]", "active_views: [Raw, NT, Shared]"),
                (
                    "    - !<View> {name: Look, colorspace: lut, looks: warm}\n",
                    "    - !<View> {name: Look, colorspace: lut, looks: warm}\n    - !<View> {name: NT, colorspace: nt}\n",
                ),
            ]),
        ),
        both(
            "no_view_transform",
            hdr_26_with(&[("active_views: [Raw, Shared]", "active_views: [Raw]")]),
        ),
        both(
            "no_active_view",
            hdr_26_with(&[("active_views: [Raw, Shared]", "active_views: [Nope]")]),
        ),
        both("no_display_colorspaces", no_display_colorspaces.to_string()),
        both(
            "failed_validation",
            hdr_26_with(&[("default: raw", "default: nope")]),
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    cases.extend([
        with_files(Case::new("i_verbose", &["-i", "a.ocio", "-v"])).file("a.ocio", good()),
        with_files(Case::new("verbose_from_env", &["-v"]))
            .file("a.ocio", good())
            .env("OCIO", "a.ocio"),
        with_files(Case::new("verbose_logging_debug", &["-v", "-i", "a.ocio"]))
            .file("a.ocio", good())
            .env("OCIO_LOGGING_LEVEL", "debug"),
        with_files(Case::new("verbose_logging_none", &["-v", "-i", "a.ocio"]))
            .file("a.ocio", good())
            .env("OCIO_LOGGING_LEVEL", "none"),
        with_files(Case::new(
            "verbose_output_config",
            &["-v", "-i", "a.ocio", "--oconfig", "b.ocio"],
        ))
        .file("a.ocio", good()),
        with_files(Case::new(
            "verbose_output_config_fails",
            &["-v", "-i", "a.ocio", "--oconfig", "no/b.ocio"],
        ))
        .file("a.ocio", good()),
        with_files(Case::new(
            "verbose_merged_streams",
            &["-v", "-i", "a.ocio", "--oconfig", "b.ocio"],
        ))
        .file("a.ocio", good())
        .env("OCIO_LOGGING_LEVEL", "debug")
        .merged(),
    ]);
    let summary = sweep("ociocheck_compatibility", TOOL, PROGRAM, &cases);

    // The cases do what they are for: both answers, and reasons that only -v prints.
    let answer = |answer: &str| {
        let line = format!("HDR Display Support (OCIO 2.6): {answer}");
        summary
            .outcomes
            .iter()
            .filter(|o| o.stdout.windows(line.len()).any(|w| w == line.as_bytes()))
            .count()
    };
    assert!(answer("yes") >= 6, "{}", answer("yes"));
    assert!(answer("no") >= 20, "{}", answer("no"));
    let reason = b"HDR Display Support (2.6): ";
    let with_reasons: Vec<&str> = cases
        .iter()
        .zip(&summary.outcomes)
        .filter(|(_, o)| o.stdout.windows(reason.len()).any(|w| w == reason))
        .map(|(c, _)| c.label.as_str())
        .collect();
    assert!(with_reasons.len() >= 12, "{with_reasons:?}");
    assert!(
        with_reasons.iter().all(|label| label.contains("verbose")),
        "{with_reasons:?}"
    );
}

/// Every config of upstream's test data.
#[test]
fn configs_of_upstream() {
    fn walk(dir: &Path, found: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, found);
            } else if path
                .extension()
                .is_some_and(|e| ["ocio", "ocioz", "yaml", "ociom"].contains(&e.to_str().unwrap()))
            {
                found.push(path.to_str().unwrap().replace('\\', "/"));
            }
        }
    }
    let root = paths::upstream_dir().join("tests/data/files");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() >= 20, "{}", files.len());
    let cases: Vec<Case> = files
        .iter()
        .flat_map(|file| {
            let name = file.rsplit("files/").next().unwrap();
            [
                Case::new(name, &["--iconfig", file, "--oconfig", "out.ocio"]),
                Case::new(format!("{name} with $OCIO"), &[]).env("OCIO", file.clone()),
            ]
        })
        .collect();
    let summary = sweep("ociocheck_upstream", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 10, "{}", summary.writing_files());
}

/// The library's built-in configs.
#[test]
fn builtin_configs() {
    let names = [
        "ocio://default",
        "ocio://cg-config-latest",
        "ocio://studio-config-latest",
        "ocio://cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
        "ocio://cg-config-v2.1.0_aces-v1.3_ocio-v2.3",
        "ocio://cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
        "ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
        "ocio://cg-config-v5.0.0_aces-v2.1_ocio-v2.6",
        "ocio://studio-config-v1.0.0_aces-v1.3_ocio-v2.1",
        "ocio://studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
        "ocio://studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
        "ocio://studio-config-v4.0.0_aces-v2.0_ocio-v2.5",
        "ocio://studio-config-v5.0.0_aces-v2.1_ocio-v2.6",
        "ocio://not-a-config",
    ];
    // With -v too, which prints why a config fails the compatibility check (new in 2.6.0).
    let cases: Vec<Case> = names
        .iter()
        .flat_map(|name| {
            [
                Case::new(*name, &["--iconfig", name, "--oconfig", "out.ocio"]),
                Case::new(format!("{name} -v"), &["-v", "--iconfig", name]),
            ]
        })
        .collect();
    let summary = sweep("ociocheck_builtin", TOOL, PROGRAM, &cases);
    assert_eq!(summary.with_exit_code(0), 2 * (names.len() - 1));
    assert_eq!(summary.writing_files(), names.len() - 1);
}

/// Every role name of `OpenColorTypes.h`, which `ociocheck` tells from a user's.
#[test]
fn standard_roles() {
    let roles = [
        "reference",
        "data",
        "color_picking",
        "texture_paint",
        "matte_paint",
        "rendering",
        "scene_linear",
        "color_timing",
        "compositing_log",
        "default",
        "aces_interchange",
        "cie_xyz_d65_interchange",
        "Rendering",
        "rendering2",
    ];
    let cases: Vec<Case> = roles
        .iter()
        .map(|role| {
            config_case(
                role,
                good_with("my_own_role: log", &format!("{role}x: log\n  {role}: log")).replace(
                    "  scene_linear: linear\n  scene_linear: log",
                    "  scene_linear: log",
                ),
            )
        })
        .collect();
    let summary = sweep("ociocheck_roles", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 8,
        "{}",
        summary.with_exit_code(0)
    );
}

/// Every ID of the Color Interop Forum's lists, alone and with a namespace, and near misses.
#[test]
fn interop_ids() {
    let ids = [
        "lin_ap1_scene",
        "lin_ap0_scene",
        "lin_rec709_scene",
        "lin_p3d65_scene",
        "lin_rec2020_scene",
        "lin_adobergb_scene",
        "lin_ciexyzd65_scene",
        "srgb_rec709_scene",
        "g22_rec709_scene",
        "g18_rec709_scene",
        "srgb_ap1_scene",
        "g22_ap1_scene",
        "srgb_p3d65_scene",
        "g22_adobergb_scene",
        "data",
        "unknown",
        "srgb_rec709_display",
        "g24_rec709_display",
        "srgb_p3d65_display",
        "srgbe_p3d65_display",
        "pq_p3d65_display",
        "pq_rec2020_display",
        "hlg_rec2020_display",
        "g22_rec709_display",
        "g22_adobergb_display",
        "g26_p3d65_display",
        "g26_xyzd65_display",
        "pq_xyzd65_display",
        "Data",
        "lin_ap1_display",
    ];
    let cases: Vec<Case> = ids
        .iter()
        .flat_map(|id| {
            let with = |value: String, alias: String| {
                good_with(
                    "aliases: [lin_ap0_scene]\n    interop_id: lin_ap0_scene",
                    &format!("aliases: [\"{alias}\"]\n    interop_id: \"{value}\""),
                )
            };
            [
                config_case(id, with(id.to_string(), id.to_string())),
                config_case(
                    &format!("ns:{id}"),
                    with(format!("ns:{id}"), format!("ns:{id}")),
                ),
            ]
        })
        .collect();
    let summary = sweep("ociocheck_interop_ids", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 28,
        "{}",
        summary.with_exit_code(0)
    );
}

/// Bytes that aren't a Windows code page's pass through on Linux, in names and paths alike.
#[cfg(unix)]
#[test]
fn non_ascii_paths() {
    let cases = vec![
        with_files(Case::new(
            "non_ascii",
            &["--iconfig", "caf\u{e9}.ocio", "--oconfig", "\u{fc}.ocio"],
        ))
        .file("caf\u{e9}.ocio", good()),
    ];
    let summary = sweep("ociocheck_non_ascii", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 1);
}

/// `--oconfig` over a file with Windows attributes.
#[cfg(windows)]
#[test]
fn output_file_attributes() {
    let argv = ["--iconfig", "a.ocio", "--oconfig", "o.ocio"];
    let cases = common::output_attribute_cases("o.ocio", &argv, |case| case.file("a.ocio", good()));
    let summary = sweep("ociocheck_attributes", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 0);
    assert_eq!(summary.with_exit_code(0), cases.len());
}

/// Interop IDs with several colons, or with nothing on one side: the first colon decides.
#[test]
fn interop_id_colons() {
    let ids = [
        "a:b:data", "a:data:b", ":data", "data:", ":", "a:", "a::data", "DATA", "x:DATA",
    ];
    let cases: Vec<Case> = ids
        .iter()
        .map(|id| {
            config_case(
                id,
                good_with(
                    "aliases: [lin_ap0_scene]\n    interop_id: lin_ap0_scene",
                    &format!("interop_id: \"{id}\""),
                ),
            )
        })
        .collect();
    sweep("ociocheck_interop_colons", TOOL, PROGRAM, &cases);
}

/// Names that aren't ASCII, and one that isn't UTF-8, go through the streams as bytes.
#[test]
fn names_as_bytes() {
    let config = [
        good().as_bytes(),
        b"  - !<ColorSpace>\n    name: \"caf\xc3\xa9\"\n",
        b"  - !<ColorSpace>\n    name: bad\xff\n",
        b"  - !<ColorSpace>\n    name: \"a\\r\\nb\"\n",
    ]
    .concat();
    let cases = vec![
        config_case("stdout", config.clone()),
        config_case("merged", config.clone()).merged(),
        with_files(Case::new(
            "written",
            &["--iconfig", "a.ocio", "--oconfig", "o.ocio"],
        ))
        .file("a.ocio", config),
    ];
    let summary = sweep("ociocheck_bytes", TOOL, PROGRAM, &cases);
    assert!(summary.outcomes.iter().any(|o| o.stdout.contains(&0xff)));
}

/// One pipe for stdout and stderr: the library's log, on stderr, between the tool's lines.
#[test]
fn order_of_stdout_and_stderr() {
    let cases: Vec<Case> = [
        Case::new("no_config", &[]),
        config_case("warnings_while_loading", good()),
        config_case("debug", good()).env("OCIO_LOGGING_LEVEL", "debug"),
        Case::new("help", &["--help"]),
    ]
    .into_iter()
    .map(Case::merged)
    .collect();
    sweep("ociocheck_order", TOOL, PROGRAM, &cases);
}
