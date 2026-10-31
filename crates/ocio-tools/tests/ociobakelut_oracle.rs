// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociobakelut` against the wheel's: stdout, stderr, exit code and the LUT it writes.
//!
//! ICC output isn't ported (docs/cards/phase10-tools.md): a command line that gets as far as
//! writing a profile is in [`icc_output_is_reported_as_not_ported`], and the ones the wheel
//! refuses before that are compared like any other.

mod common;

use common::{Case, Outcome, run_both, sweep, test_file};
use ocio::Baker;

const TOOL: &str = "ociobakelut";
const PROGRAM: &str = env!("CARGO_BIN_EXE_ociobakelut");

const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: .
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]
description: A config for the sweep.

roles:
  default: raw
  scene_linear: lin
  compositing_log: log

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Film, colorspace: gamma, looks: warm}

looks:
  - !<Look>
    name: warm
    process_space: lin
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: lin

  - !<ColorSpace>
    name: log
    from_scene_reference: !<LogTransform> {base: 2}

  - !<ColorSpace>
    name: gamma
    from_scene_reference: !<ExponentTransform> {value: 2.2}

  - !<ColorSpace>
    name: missing_lut
    from_scene_reference: !<FileTransform> {src: not_there.spi1d}
"#;

fn with_config(case: Case) -> Case {
    case.file("a.ocio", CONFIG)
}

/// The baker's formats, to go through.
fn formats() -> Vec<String> {
    let formats: Vec<String> = (0..Baker::num_formats())
        .map(|i| String::from_utf8(Baker::format_name_by_index(i).to_vec()).unwrap())
        .collect();
    assert!(formats.len() >= 10, "{formats:?}");
    formats
}

#[test]
fn bad_arguments() {
    let lut = test_file("lut1d_1.spi1d");
    let lut = lut.as_str();
    let cases: Vec<Case> = [
        ("no_arguments", vec![]),
        ("help", vec!["--help"]),
        ("help_one_dash", vec!["-help"]),
        ("h_is_invalid", vec!["-h"]),
        ("invalid", vec!["--bogus"]),
        ("invalid_then_help", vec!["--bogus", "--help"]),
        ("only_an_output", vec!["out.spi1d"]),
        ("missing_inputspace_value", vec!["--inputspace"]),
        ("missing_displayview_view", vec!["--displayview", "a"]),
        ("missing_lut_value", vec!["--lut"]),
        ("missing_cccid_value", vec!["--cccid"]),
        ("missing_slope_values", vec!["--slope", "1", "2"]),
        ("missing_offset10_values", vec!["--offset10", "1"]),
        ("missing_sat_value", vec!["--sat"]),
        ("missing_format_value", vec!["--format"]),
        ("missing_cubesize_value", vec!["--cubesize"]),
        ("no_inputspace", vec!["--format", "spi1d", "out.spi1d"]),
        ("no_outputspace", vec!["--inputspace", "a", "out.spi1d"]),
        (
            "display_only",
            vec!["--inputspace", "a", "--displayview", "d", "", "out.spi1d"],
        ),
        (
            "view_only",
            vec!["--inputspace", "a", "--displayview", "", "v", "out.spi1d"],
        ),
        (
            "no_format",
            vec!["--inputspace", "a", "--outputspace", "b", "out.spi1d"],
        ),
        (
            "no_config",
            vec![
                "--inputspace",
                "a",
                "--outputspace",
                "b",
                "--format",
                "spi1d",
                "out.spi1d",
            ],
        ),
        (
            "lut_with_inputspace",
            vec!["--lut", lut, "--inputspace", "a", "out.spi1d"],
        ),
        (
            "lut_with_outputspace",
            vec!["--lut", lut, "--outputspace", "a", "out.spi1d"],
        ),
        (
            "lut_with_looks",
            vec!["--lut", lut, "--looks", "a", "out.spi1d"],
        ),
        (
            "lut_with_shaperspace",
            vec!["--lut", lut, "--shaperspace", "a", "out.spi1d"],
        ),
        (
            "lut_with_displayview",
            vec!["--lut", lut, "--displayview", "a", "b", "out.spi1d"],
        ),
        (
            "lut_with_display_only",
            vec!["--lut", lut, "--displayview", "a", "", "out.spi1d"],
        ),
        ("lut_no_output", vec!["--lut", lut, "--format", "spi1d"]),
        ("lut_no_format", vec!["--lut", lut, "out.spi1d"]),
        (
            "lut_unknown_format",
            vec!["--lut", lut, "--format", "nope", "out.spi1d"],
        ),
        (
            "lut_missing_file",
            vec!["--lut", "not_there.spi1d", "--format", "spi1d", "out.spi1d"],
        ),
        (
            "option_as_a_value",
            vec![
                "--inputspace",
                "--lut",
                lut,
                "--format",
                "spi1d",
                "out.spi1d",
            ],
        ),
        ("parse_luts_lut_at_the_end", vec!["--inputspace", "--lut"]),
        (
            "parse_luts_cccid_at_the_end",
            vec!["--inputspace", "--cccid"],
        ),
        (
            "parse_luts_invlut_at_the_end",
            vec!["--inputspace", "--invlut"],
        ),
        (
            "parse_luts_slope_at_the_end",
            vec!["--inputspace", "--slope"],
        ),
        (
            "parse_luts_slope_exact",
            vec!["--looks", "--slope", "1", "2", "3"],
        ),
        (
            "parse_luts_slope_one_short",
            vec!["--looks", "--slope", "1", "2"],
        ),
        (
            "parse_luts_slope_short",
            vec!["--looks", "--slope", "--format", "1"],
        ),
        (
            "parse_luts_offset_at_the_end",
            vec!["--inputspace", "--offset"],
        ),
        (
            "parse_luts_offset10_at_the_end",
            vec!["--inputspace", "--offset10"],
        ),
        (
            "parse_luts_power_at_the_end",
            vec!["--inputspace", "--power"],
        ),
        ("parse_luts_sat_at_the_end", vec!["--inputspace", "--sat"]),
    ]
    .into_iter()
    .map(|(label, argv)| Case::new(label, &argv))
    .collect();
    let summary = sweep("ociobakelut_bad_arguments", TOOL, PROGRAM, &cases);
    assert_eq!(summary.with_exit_code(1), cases.len());
}

#[test]
fn every_format_from_a_config() {
    let selections: Vec<(&str, Vec<&str>)> = vec![
        ("space", vec!["--inputspace", "lin", "--outputspace", "log"]),
        (
            "role",
            vec![
                "--inputspace",
                "scene_linear",
                "--outputspace",
                "compositing_log",
            ],
        ),
        (
            "gamma",
            vec!["--inputspace", "gamma", "--outputspace", "lin"],
        ),
        (
            "shaper",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--shaperspace",
                "log",
            ],
        ),
        (
            "shapersize",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--shaperspace",
                "log",
                "--shapersize",
                "64",
            ],
        ),
        (
            "displayview",
            vec!["--inputspace", "lin", "--displayview", "sRGB", "Film"],
        ),
        (
            "displayview_shaper",
            vec![
                "--inputspace",
                "lin",
                "--displayview",
                "sRGB",
                "Film",
                "--shaperspace",
                "log",
            ],
        ),
        (
            "looks",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--looks",
                "warm",
            ],
        ),
        (
            "looks_shaper",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--looks",
                "warm",
                "--shaperspace",
                "log",
            ],
        ),
        (
            "cubesize",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--cubesize",
                "5",
            ],
        ),
        (
            "cubesize_one",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--cubesize",
                "1",
            ],
        ),
        (
            "cubesize_zero",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--cubesize",
                "abc",
            ],
        ),
        (
            "cubesize_negative",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--cubesize",
                "-5",
            ],
        ),
        (
            "shapersize_small",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--shapersize",
                "2",
            ],
        ),
        (
            "unknown_input",
            vec!["--inputspace", "nope", "--outputspace", "log"],
        ),
        (
            "unknown_output",
            vec!["--inputspace", "lin", "--outputspace", "nope"],
        ),
        (
            "unknown_shaper",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--shaperspace",
                "nope",
            ],
        ),
        (
            "unknown_look",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--looks",
                "nope",
            ],
        ),
        (
            "unknown_view",
            vec!["--inputspace", "lin", "--displayview", "sRGB", "nope"],
        ),
        (
            "both_targets",
            vec![
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--displayview",
                "sRGB",
                "Raw",
            ],
        ),
        (
            "missing_lut",
            vec!["--inputspace", "lin", "--outputspace", "missing_lut"],
        ),
    ];
    let mut cases = Vec::new();
    for format in formats() {
        for (label, selection) in &selections {
            for (output, verbose) in [("file", false), ("stdout", false), ("verbose", true)] {
                let mut argv = vec!["--iconfig", "a.ocio", "--format", &format];
                argv.extend(selection);
                if verbose {
                    argv.push("--v");
                }
                if output == "stdout" {
                    argv.push("--stdout");
                } else {
                    argv.push("out.lut");
                }
                cases.push(with_config(Case::new(
                    format!("{format} {label} {output}"),
                    &argv,
                )));
            }
        }
    }
    let summary = sweep("ociobakelut_formats", TOOL, PROGRAM, &cases);
    assert!(
        summary.writing_files() >= 100,
        "{}",
        summary.writing_files()
    );
    assert!(
        summary.with_exit_code(0) >= 200,
        "{}",
        summary.with_exit_code(0)
    );
}

#[test]
fn every_format_from_luts() {
    let lut1d = test_file("lut1d_1.spi1d");
    let lut3d = test_file("lut3d_1.spi3d");
    let cdl = test_file("cdl_test1.ccc");
    let chains: Vec<(&str, Vec<&str>)> = vec![
        ("lut1d", vec!["--lut", &lut1d]),
        ("lut3d", vec!["--lut", &lut3d]),
        ("two", vec!["--lut", &lut1d, "--lut", &lut3d]),
        ("one_dash", vec!["-lut", &lut1d, "-invlut", &lut1d]),
        ("invlut", vec!["--invlut", &lut1d]),
        ("ccc_first", vec!["--cccid", "cc0001", "--lut", &cdl]),
        ("ccc_index", vec!["--cccid", "1", "--lut", &cdl]),
        ("ccc_after", vec!["--lut", &cdl, "--cccid", "cc0001"]),
        (
            "ccc_with_invlut",
            vec!["--cccid", "cc0001", "--invlut", &cdl],
        ),
        ("ccc_unknown", vec!["--cccid", "nope", "--lut", &cdl]),
        ("slope", vec!["--slope", "1.1", "1", "0.9"]),
        ("offset", vec!["--offset", "0.01", "-0.02", "0.03"]),
        ("offset10", vec!["--offset10", "10", "-20", "30"]),
        ("power", vec!["--power", "1.1", "1", "0.9"]),
        ("sat", vec!["--sat", "0.5"]),
        (
            "all_cdl",
            vec![
                "--slope", "1.1", "1", "0.9", "--offset", "0.01", "0", "0", "--power", "1", "1.2",
                "1", "--sat", "0.9",
            ],
        ),
        ("garbage_numbers", vec!["--slope", "abc", "1x", "--lut"]),
        (
            "flag_as_a_number",
            vec!["--slope", "1", "2", "--sat", "out.lut"],
        ),
        ("nan_numbers", vec!["--slope", "nan", "inf", "-inf"]),
    ];
    let mut cases = Vec::new();
    for format in formats() {
        for (label, chain) in &chains {
            for (output, verbose) in [("file", false), ("stdout", false), ("verbose", true)] {
                let mut argv = chain.clone();
                argv.extend(["--format", &format]);
                if verbose {
                    argv.push("--v");
                }
                if output == "stdout" {
                    argv.push("--stdout");
                } else {
                    argv.push("out.lut");
                }
                cases.push(Case::new(format!("{format} {label} {output}"), &argv));
            }
        }
    }
    let summary = sweep("ociobakelut_luts", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 80, "{}", summary.writing_files());
    assert!(
        summary.with_exit_code(0) >= 200,
        "{}",
        summary.with_exit_code(0)
    );
}

#[test]
fn configs_and_output_paths() {
    let context = test_file("configs/context_test1/config.ocio");
    let bake = [
        "--inputspace",
        "lin",
        "--outputspace",
        "log",
        "--format",
        "spi1d",
    ];
    let with = |extra: &[&'static str]| {
        let mut argv = bake.to_vec();
        argv.extend_from_slice(extra);
        argv
    };
    let cases = vec![
        with_config(Case::new("from_env", &with(&["out.lut"]))).env("OCIO", "a.ocio"),
        with_config(Case::new("from_env_verbose", &with(&["--v", "out.lut"])))
            .env("OCIO", "a.ocio"),
        with_config(Case::new(
            "iconfig_wins",
            &with(&["--iconfig", "a.ocio", "out.lut"]),
        ))
        .env("OCIO", "not_there.ocio"),
        with_config(Case::new(
            "last_output_wins",
            &with(&["--iconfig", "a.ocio", "one.lut", "two.lut"]),
        )),
        with_config(Case::new(
            "output_first",
            &[
                "out.lut",
                "--iconfig",
                "a.ocio",
                "--inputspace",
                "lin",
                "--outputspace",
                "log",
                "--format",
                "spi1d",
            ],
        )),
        with_config(Case::new(
            "no_directory",
            &with(&["--iconfig", "a.ocio", "no/out.lut"]),
        )),
        with_config(Case::new(
            "into_a_directory",
            &with(&["--iconfig", "a.ocio", "d"]),
        ))
        .file("d/x.txt", "x"),
        with_config(Case::new("no_output", &with(&["--iconfig", "a.ocio"]))),
        with_config(Case::new(
            "empty_output",
            &with(&["--iconfig", "a.ocio", ""]),
        )),
        with_config(Case::new(
            "stdout_and_output",
            &with(&["--iconfig", "a.ocio", "--stdout", "out.lut"]),
        )),
        with_config(Case::new(
            "logging_debug",
            &with(&["--iconfig", "a.ocio", "out.lut"]),
        ))
        .env("OCIO_LOGGING_LEVEL", "debug"),
        with_config(Case::new(
            "logging_bogus",
            &with(&["--iconfig", "a.ocio", "--stdout"]),
        ))
        .env("OCIO_LOGGING_LEVEL", "bogus"),
        Case::new("no_config", &with(&["out.lut"])),
        Case::new(
            "builtin",
            &[
                "--iconfig",
                "ocio://cg-config-latest",
                "--inputspace",
                "ACEScg",
                "--outputspace",
                "sRGB - Texture",
                "--format",
                "spi3d",
                "--cubesize",
                "5",
                "out.lut",
            ],
        ),
        Case::new(
            "builtin_view",
            &[
                "--iconfig",
                "ocio://default",
                "--inputspace",
                "ACEScg",
                "--displayview",
                "sRGB - Display",
                "ACES 2.0 - SDR 100 nits (Rec.709)",
                "--format",
                "cinespace",
                "--cubesize",
                "5",
                "--stdout",
            ],
        ),
        Case::new(
            "context",
            &[
                "--iconfig",
                &context,
                "--inputspace",
                "plain_lut1_cs",
                "--outputspace",
                "shot1_lut1_cs",
                "--format",
                "spi1d",
                "out.lut",
            ],
        ),
    ];
    let summary = sweep("ociobakelut_paths", TOOL, PROGRAM, &cases);
    assert!(summary.writing_files() >= 7, "{}", summary.writing_files());
}

/// `--format icc` where the wheel refuses the command line before it writes a profile.
#[test]
fn icc_refusals() {
    let select = [
        "--iconfig",
        "a.ocio",
        "--inputspace",
        "lin",
        "--format",
        "icc",
    ];
    let with = |extra: &[&'static str]| {
        let mut argv = select.to_vec();
        argv.extend_from_slice(extra);
        argv
    };
    let cases = vec![
        with_config(Case::new(
            "stdout",
            &with(&["--outputspace", "log", "--stdout"]),
        )),
        with_config(Case::new(
            "stdout_with_output",
            &with(&["--outputspace", "log", "--stdout", "out.icc"]),
        )),
        with_config(Case::new("no_output", &with(&["--outputspace", "log"]))),
        with_config(Case::new(
            "unknown_output",
            &with(&["--outputspace", "nope", "out.icc"]),
        )),
        with_config(Case::new(
            "unknown_view",
            &with(&["--displayview", "sRGB", "nope", "out.icc"]),
        )),
        with_config(Case::new(
            "unknown_look",
            &with(&["--outputspace", "log", "--looks", "nope", "out.icc"]),
        )),
        with_config(Case::new(
            "unknown_look_view",
            &with(&["--displayview", "sRGB", "Raw", "--looks", "nope", "out.icc"]),
        )),
        with_config(Case::new(
            "missing_lut",
            &with(&["--outputspace", "missing_lut", "out.icc"]),
        )),
        with_config(Case::new(
            "verbose_stdout",
            &with(&["--outputspace", "log", "--v", "--stdout"]),
        )),
        with_config(Case::new(
            "verbose_refused",
            &with(&["--outputspace", "nope", "--v", "out.icc"]),
        )),
        with_config(Case::new(
            "described_refused",
            &with(&[
                "--outputspace",
                "nope",
                "--v",
                "--description",
                "d",
                "out.icc",
            ]),
        )),
        with_config(Case::new(
            "shaper_refused",
            &with(&["--outputspace", "nope", "--shaperspace", "log", "out.icc"]),
        )),
    ];
    let summary = sweep("ociobakelut_icc_refusals", TOOL, PROGRAM, &cases);
    assert_eq!(summary.with_exit_code(1), cases.len());
}

/// The text of `outcome`'s stderr.
fn stderr(outcome: &Outcome) -> String {
    String::from_utf8(outcome.stderr.clone()).unwrap()
}

/// Where the wheel writes a profile, the port reports that it can't. Everything the tool prints
/// before that point still matches, and nothing is left behind.
#[test]
fn icc_output_is_reported_as_not_ported() {
    let select = [
        "--iconfig",
        "a.ocio",
        "--inputspace",
        "lin",
        "--format",
        "icc",
    ];
    for (label, extra) in [
        ("space", vec!["--outputspace", "log", "out.icc"]),
        ("view", vec!["--displayview", "sRGB", "Film", "out.icc"]),
        ("verbose", vec!["--outputspace", "log", "--v", "out.icc"]),
        ("debug", vec!["--outputspace", "log", "out.icc"]),
        (
            "shaper",
            vec!["--outputspace", "log", "--shaperspace", "gamma", "out.icc"],
        ),
    ] {
        let mut argv = select.to_vec();
        argv.extend(extra);
        let mut case = with_config(Case::new(label, &argv));
        // The optimizer's lines are how the CPU processor upstream makes here shows.
        if label == "debug" {
            case = case.env("OCIO_LOGGING_LEVEL", "debug");
        }
        let (wheel, port) = run_both("ociobakelut_icc", TOOL, PROGRAM, &case);
        assert_eq!(wheel.exit_code, 0, "{label}: {}", stderr(&wheel));
        assert!(wheel.files.contains_key("out.icc"), "{label}");
        assert_eq!(port.exit_code, 1, "{label}");
        assert!(port.files.is_empty(), "{label}");
        assert!(wheel.stdout.starts_with(&port.stdout), "{label}");
        let reported = stderr(&port);
        assert!(
            reported.contains("ICC output is not available"),
            "{reported}"
        );
        assert!(reported.starts_with(&stderr(&wheel)), "{reported}");
    }
}

/// `Config::CreateFromFile` and `CreateFromEnv` run outside `main`'s `try` blocks, so a config
/// that doesn't load is an uncaught exception, and the C++ runtime ends the process
/// (docs/improvements.md, I-606). The port ends the same way. A line printed before it stays,
/// because `std::endl` flushed it.
#[test]
fn an_exception_upstream_does_not_catch() {
    let bake = [
        "--inputspace",
        "a",
        "--outputspace",
        "b",
        "--format",
        "spi1d",
    ];
    let with = |extra: &[&'static str]| [&bake[..], extra].concat();
    let cases = vec![
        Case::new(
            "iconfig",
            &with(&["--iconfig", "not_there.ocio", "out.lut"]),
        )
        .dies(),
        Case::new(
            "verbose",
            &with(&["--iconfig", "not_there.ocio", "--v", "out.lut"]),
        )
        .dies(),
        Case::new("env", &with(&["out.lut"]))
            .env("OCIO", "not_there.ocio")
            .dies(),
        Case::new("env_verbose", &with(&["--v", "out.lut"]))
            .env("OCIO", "not_there.ocio")
            .dies(),
        Case::new("broken", &with(&["--iconfig", "a.ocio", "out.lut"]))
            .file("a.ocio", "not a config")
            .dies(),
        Case::new(
            "merged",
            &with(&["--iconfig", "not_there.ocio", "--v", "out.lut"]),
        )
        .merged()
        .dies(),
    ];
    let summary = sweep("ociobakelut_uncaught", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 0);
}

/// The output file with Windows attributes.
#[cfg(windows)]
#[test]
fn output_file_attributes() {
    let argv = [
        "--iconfig",
        "a.ocio",
        "--inputspace",
        "lin",
        "--outputspace",
        "log",
        "--format",
        "spi1d",
        "--v",
        "o.lut",
    ];
    let cases = common::output_attribute_cases("o.lut", &argv, with_config);
    let summary = sweep("ociobakelut_attributes", TOOL, PROGRAM, &cases);
    assert_eq!(summary.writing_files(), 0);
    assert_eq!(summary.with_exit_code(1), cases.len());
}

/// The options of `parse_luts` with one dash, which `ArgParse` takes too.
#[test]
fn cdl_options_with_one_dash() {
    let ccc = test_file("cdl_test1.ccc");
    let chains: Vec<(&str, Vec<&str>)> = vec![
        ("slope", vec!["-slope", "1.1", "1", "0.9"]),
        ("offset", vec!["-offset", "0.1", "0", "0"]),
        ("offset10", vec!["-offset10", "10", "0", "0"]),
        ("power", vec!["-power", "1.1", "1", "1"]),
        ("sat", vec!["-sat", "0.5"]),
        ("cccid", vec!["-cccid", "cc0001", "-lut", &ccc]),
    ];
    let mut cases = Vec::new();
    for (label, chain) in &chains {
        let stdout = [&chain[..], &["--format", "spi1d", "--stdout"]].concat();
        cases.push(Case::new(format!("{label}_stdout"), &stdout));
        let verbose = [&chain[..], &["--format", "spi1d", "--v", "out.lut"]].concat();
        cases.push(Case::new(format!("{label}_verbose"), &verbose));
    }
    let summary = sweep("ociobakelut_one_dash", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 8,
        "{}",
        summary.with_exit_code(0)
    );
}

/// Sizes go through `atoi`, which stops at what isn't a digit.
#[test]
fn sizes_as_text() {
    let lut = test_file("lut1d_1.spi1d");
    let mut cases = Vec::new();
    for format in formats() {
        for size in ["0", "1e3", "0x10", " 7", "7.9", "+5", "-2"] {
            let argv = [
                "--lut",
                &lut,
                "--format",
                &format,
                "--cubesize",
                size,
                "--stdout",
            ];
            cases.push(Case::new(format!("{format} cube {size}"), &argv));
            let argv = [
                "--iconfig",
                "a.ocio",
                "--inputspace",
                "lin",
                "--outputspace",
                "gamma",
                "--shaperspace",
                "log",
                "--format",
                &format,
                "--shapersize",
                size,
                "--cubesize",
                "3",
                "--stdout",
            ];
            cases.push(with_config(Case::new(
                format!("{format} shaper {size}"),
                &argv,
            )));
        }
    }
    let summary = sweep("ociobakelut_sizes", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 20,
        "{}",
        summary.with_exit_code(0)
    );
}

/// `--offset10`'s value is a double divided in double, which a format of many digits shows.
#[test]
fn offset10_in_double() {
    let mut cases = Vec::new();
    for value in ["10.1", "7.3", "33.3", "101.7", "0.1", "511.9"] {
        let argv = [
            "--offset10",
            value,
            value,
            value,
            "--format",
            "Color Transform Format",
            "--stdout",
        ];
        cases.push(Case::new(value, &argv));
    }
    let summary = sweep("ociobakelut_offset10", TOOL, PROGRAM, &cases);
    assert_eq!(summary.with_exit_code(0), cases.len());
}

/// One pipe for stdout and stderr: `--v` leaves text in `std::cout` with no flush, and what
/// `std::cerr` writes next must come after it.
#[test]
fn order_of_stdout_and_stderr() {
    let lut = test_file("lut1d_1.spi1d");
    let cases = vec![
        Case::new(
            "error_after_unflushed_text",
            &["--lut", &lut, "--format", "nope", "--v", "o.lut"],
        ),
        with_config(Case::new(
            "error_after_lines",
            &[
                "--iconfig",
                "a.ocio",
                "--inputspace",
                "nope",
                "--outputspace",
                "log",
                "--format",
                "spi1d",
                "--v",
                "o.lut",
            ],
        )),
        Case::new("refused_after_unflushed_text", &["--lut", &lut, "--v"]),
    ];
    let cases: Vec<Case> = cases.into_iter().map(Case::merged).collect();
    sweep("ociobakelut_order", TOOL, PROGRAM, &cases);
}
