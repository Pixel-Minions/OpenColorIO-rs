// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociowrite` against the wheel's: stdout, stderr, exit code and the files it writes.

mod common;

use common::{Case, sweep, test_file, test_file_bytes};

const TOOL: &str = "ociowrite";
const PROGRAM: &str = env!("CARGO_BIN_EXE_ociowrite");

/// A config with every way of selecting a transform: a LUT file, a display with two views, a
/// look, a data color space, and a color space with a LUT that isn't there.
const CONFIG: &str = r#"ocio_profile_version: 2.1

environment:
  {}
search_path: luts
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]
description: A config for the sweep.

roles:
  default: raw
  scene_linear: linear
  compositing_log: log

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Film, colorspace: lut, looks: warm}
  P3:
    - !<View> {name: Log, colorspace: log}

looks:
  - !<Look>
    name: warm
    process_space: linear
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: linear

  - !<ColorSpace>
    name: log
    from_scene_reference: !<LogAffineTransform> {base: 2, logSideSlope: 0.1, logSideOffset: 0.5}

  - !<ColorSpace>
    name: lut
    from_scene_reference: !<FileTransform> {src: lut1d_1.spi1d, interpolation: linear}

  - !<ColorSpace>
    name: missing_lut
    from_scene_reference: !<FileTransform> {src: not_there.spi1d}

  - !<ColorSpace>
    name: matrix
    to_scene_reference: !<MatrixTransform> {matrix: [0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 1], offset: [0.1, 0.2, 0.3, 0]}

  - !<ColorSpace>
    name: fixed
    from_scene_reference: !<FixedFunctionTransform> {style: ACES_Glow10}
"#;

fn with_config(case: Case) -> Case {
    case.env("OCIO", "a.ocio")
        .file("a.ocio", CONFIG)
        .file("luts/lut1d_1.spi1d", test_file_bytes("lut1d_1.spi1d"))
}

#[test]
fn bad_arguments() {
    let cases: Vec<Case> = [
        ("no_arguments", vec![]),
        ("help", vec!["--help"]),
        ("help_short", vec!["--h"]),
        ("help_one_dash", vec!["-help"]),
        ("h_one_dash", vec!["-h"]),
        ("help_then_invalid", vec!["--help", "--bogus"]),
        ("invalid_then_help", vec!["--bogus", "--help"]),
        ("invalid", vec!["--bogus"]),
        ("invalid_one_dash", vec!["-x"]),
        ("two_dashes", vec!["--"]),
        ("three_dashes", vec!["---help"]),
        ("positional", vec!["out.ctf"]),
        ("positional_dash", vec!["-"]),
        ("positional_negative", vec!["-1"]),
        ("positional_empty", vec![""]),
        ("positional_after_option", vec!["--v", "out.ctf"]),
        ("file_missing_parameter", vec!["--file"]),
        ("colorspaces_missing_first", vec!["--colorspaces"]),
        ("colorspaces_missing_second", vec!["--colorspaces", "a"]),
        ("view_missing_second", vec!["--view", "a"]),
        ("view_missing_third", vec!["--view", "a", "b"]),
        ("displayview_missing", vec!["--displayview", "a"]),
        ("invertview_missing", vec!["--invertview", "a", "b"]),
        ("no_file", vec!["--colorspaces", "a", "b"]),
        ("option_as_parameter", vec!["--file", "--v.ctf"]),
        ("last_dot_decides", vec!["--file", "a.b.ctf"]),
        ("verbose_only", vec!["--v"]),
        ("verbose_one_dash", vec!["-v"]),
        ("empty_file", vec!["--file", ""]),
        ("no_extension", vec!["--file", "out"]),
        (
            "no_extension_directory_with_dot",
            vec!["--file", "a.ctf/out"],
        ),
        ("unknown_extension", vec!["--file", "out.txt"]),
        ("trailing_dot", vec!["--file", "out."]),
        (
            "verbose_unknown_extension",
            vec!["--v", "--file", "out.txt"],
        ),
        ("no_source", vec!["--file", "out.ctf"]),
        ("verbose_no_source", vec!["--v", "--file", "out.ctf"]),
        (
            "no_source_empty_names",
            vec!["--colorspaces", "", "", "--file", "out.ctf"],
        ),
        (
            "display_without_input",
            vec!["--view", "", "sRGB", "", "--file", "out.ctf"],
        ),
    ]
    .into_iter()
    .map(|(label, argv)| Case::new(label, &argv))
    .collect();
    let summary = sweep("ociowrite_bad_arguments", TOOL, PROGRAM, &cases);
    assert_eq!(summary.with_exit_code(1), cases.len());
}

#[test]
fn environment() {
    let ok = ["--colorspaces", "linear", "log", "--file", "out.ctf"];
    let cases = vec![
        Case::new("no_ocio", &ok),
        Case::new("empty_ocio", &ok).env("OCIO", ""),
        Case::new("missing_config", &ok).env("OCIO", "not_there.ocio"),
        Case::new("verbose_missing_config", &["--v", "--file", "out.ctf"])
            .env("OCIO", "not_there.ocio"),
        Case::new(
            "verbose_missing_config_full",
            &["--v", "--colorspaces", "a", "b", "--file", "out.ctf"],
        )
        .env("OCIO", "not_there.ocio"),
        Case::new("broken_config", &ok)
            .env("OCIO", "a.ocio")
            .file("a.ocio", "not a config"),
        Case::new("verbose_broken_config", &["--v", "--file", "out.ctf"])
            .env("OCIO", "a.ocio")
            .file("a.ocio", "not a config"),
        with_config(Case::new(
            "verbose_config_no_source",
            &["--v", "--file", "out.ctf"],
        )),
        with_config(Case::new("logging_none", &ok)).env("OCIO_LOGGING_LEVEL", "none"),
        with_config(Case::new("logging_warning", &ok)).env("OCIO_LOGGING_LEVEL", "warning"),
        with_config(Case::new("logging_info", &ok)).env("OCIO_LOGGING_LEVEL", "info"),
        with_config(Case::new("logging_debug", &ok)).env("OCIO_LOGGING_LEVEL", "debug"),
        with_config(Case::new("logging_bogus", &ok)).env("OCIO_LOGGING_LEVEL", "bogus"),
        Case::new("logging_debug_stopped_early", &["--file", "out"])
            .env("OCIO_LOGGING_LEVEL", "debug"),
        with_config(Case::new("no_optimization_flag", &ok)).env("OCIO_OPTIMIZATION_FLAGS", "0"),
    ];
    let summary = sweep("ociowrite_environment", TOOL, PROGRAM, &cases);
    assert!(summary.with_exit_code(0) >= 4);
    assert!(summary.with_exit_code(1) >= 5);
}

#[test]
fn transforms() {
    let mut cases = Vec::new();
    let selections: Vec<(&str, Vec<&str>)> = vec![
        ("identity", vec!["--colorspaces", "linear", "linear"]),
        ("log", vec!["--colorspaces", "linear", "log"]),
        ("log_inverse", vec!["--colorspaces", "log", "linear"]),
        ("lut", vec!["--colorspaces", "linear", "lut"]),
        ("matrix", vec!["--colorspaces", "matrix", "linear"]),
        ("fixed", vec!["--colorspaces", "linear", "fixed"]),
        (
            "role",
            vec!["--colorspaces", "scene_linear", "compositing_log"],
        ),
        ("data", vec!["--colorspaces", "raw", "log"]),
        ("unknown_input", vec!["--colorspaces", "nope", "log"]),
        ("unknown_output", vec!["--colorspaces", "log", "nope"]),
        (
            "missing_lut",
            vec!["--colorspaces", "linear", "missing_lut"],
        ),
        ("view", vec!["--view", "linear", "sRGB", "Film"]),
        ("view_log", vec!["--view", "linear", "P3", "Log"]),
        (
            "displayview",
            vec!["--displayview", "linear", "sRGB", "Film"],
        ),
        (
            "view_unknown_display",
            vec!["--view", "linear", "nope", "Film"],
        ),
        (
            "view_unknown_view",
            vec!["--view", "linear", "sRGB", "nope"],
        ),
        ("invertview", vec!["--invertview", "sRGB", "Film", "linear"]),
        ("invertview_log", vec!["--invertview", "P3", "Log", "lut"]),
        (
            "invertview_unknown",
            vec!["--invertview", "sRGB", "nope", "linear"],
        ),
        (
            "view_and_colorspaces",
            vec![
                "--view",
                "linear",
                "sRGB",
                "Film",
                "--colorspaces",
                "linear",
                "log",
            ],
        ),
        (
            "view_and_invertview",
            vec![
                "--view",
                "linear",
                "sRGB",
                "Film",
                "--invertview",
                "sRGB",
                "Film",
                "log",
            ],
        ),
        (
            "two_colorspaces",
            vec![
                "--colorspaces",
                "linear",
                "log",
                "--colorspaces",
                "log",
                "lut",
            ],
        ),
        ("display_only", vec!["--invertview", "sRGB", "", "linear"]),
        ("output_only", vec!["--colorspaces", "", "linear"]),
    ];
    for (label, selection) in &selections {
        for (extension, verbose) in [("ctf", false), ("clf", false), ("CTF", true), ("cLf", true)] {
            let file = format!("out.{extension}");
            let mut argv = selection.clone();
            argv.extend(["--file", &file]);
            if verbose {
                argv.insert(0, "--v");
            }
            cases.push(with_config(Case::new(
                format!("{label}_{extension}_{verbose}"),
                &argv,
            )));
        }
    }
    cases.push(with_config(Case::new(
        "directory_missing",
        &[
            "--colorspaces",
            "linear",
            "log",
            "--file",
            "no/such/out.ctf",
        ],
    )));
    cases.push(with_config(Case::new(
        "file_in_directory",
        &["--colorspaces", "linear", "log", "--file", "luts/out.ctf"],
    )));
    cases.push(with_config(
        Case::new(
            "file_is_a_directory",
            &["--colorspaces", "linear", "log", "--file", "luts.ctf"],
        )
        .file("luts.ctf/inner.txt", "x"),
    ));
    let summary = sweep("ociowrite_transforms", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 30, "{}", summary.writing_files());
    assert!(summary.with_exit_code(1) >= 20);
}

#[test]
fn configs_of_upstream_and_the_library() {
    let context = test_file("configs/context_test1/config.ocio");
    let cases = vec![
        Case::new(
            "builtin_default",
            &[
                "--colorspaces",
                "ACES2065-1",
                "sRGB - Texture",
                "--file",
                "out.ctf",
            ],
        )
        .env("OCIO", "ocio://default"),
        Case::new(
            "builtin_cg",
            &[
                "--v",
                "--view",
                "ACEScg",
                "sRGB - Display",
                "ACES 2.0 - SDR 100 nits (Rec.709)",
                "--file",
                "out.clf",
            ],
        )
        .env("OCIO", "ocio://cg-config-latest"),
        Case::new(
            "builtin_studio",
            &[
                "--invertview",
                "sRGB - Display",
                "ACES 2.0 - SDR 100 nits (Rec.709)",
                "ACEScct",
                "--file",
                "out.ctf",
            ],
        )
        .env("OCIO", "ocio://studio-config-latest"),
        Case::new(
            "builtin_unknown",
            &["--colorspaces", "a", "b", "--file", "out.ctf"],
        )
        .env("OCIO", "ocio://nope"),
        Case::new(
            "context",
            &[
                "--colorspaces",
                "plain_lut1_cs",
                "shot1_lut1_cs",
                "--file",
                "out.ctf",
            ],
        )
        .env("OCIO", context.clone()),
        Case::new(
            "context_with_env",
            &[
                "--v",
                "--colorspaces",
                "shot1_lut1_cs",
                "shot2_lut1_cs",
                "--file",
                "out.ctf",
            ],
        )
        .env("OCIO", context.clone())
        .env("SHOT", "shot1"),
        Case::new(
            "archive",
            &[
                "--colorspaces",
                "plain_lut1_cs",
                "shot1_lut1_cs",
                "--file",
                "out.ctf",
            ],
        )
        .env(
            "OCIO",
            test_file("configs/context_test1/context_test1_windows.ocioz"),
        ),
    ];
    let summary = sweep("ociowrite_configs", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 3, "{}", summary.writing_files());
}
