// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tools with a pipe nobody reads, against the wheel's. By default SIGPIPE ends a C++ tool,
//! and one that inherits it ignored goes on; the port's tools do what they inherit too
//! (`crates/ocio-tools/src/sigpipe.rs`). Linux only: Windows has no SIGPIPE.

#![cfg(target_os = "linux")]

mod common;

use common::{Case, sweep, test_file};

/// A config with one LUT-free transform, for the bakes.
const CONFIG: &str = "ocio_profile_version: 2\n\nroles:\n  default: raw\n\ndisplays:\n  sRGB:\n    \
     - !<View> {name: Raw, colorspace: raw}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n  \
     - !<ColorSpace>\n    name: lin\n    to_scene_reference: !<ExponentTransform> {value: 2}\n";

/// A bake with more output than a pipe holds.
const BIG_BAKE: [&str; 11] = [
    "--iconfig",
    "a.ocio",
    "--inputspace",
    "raw",
    "--outputspace",
    "lin",
    "--format",
    "spi3d",
    "--cubesize",
    "33",
    "--stdout",
];

/// The cases of one tool with its stream closed: on the default disposition the tool dies,
/// and with SIGPIPE ignored it finishes.
fn closed_cases(cases: &[(&str, &str, Vec<&str>)]) -> Vec<Case> {
    cases
        .iter()
        .flat_map(|(label, closed, argv)| {
            let case = |suffix: &str| {
                Case::new(format!("{label}_{suffix}"), argv)
                    .file("a.ocio", CONFIG)
                    .closed_pipe(if *closed == "stdout" {
                        "stdout"
                    } else {
                        "stderr"
                    })
            };
            [case("default").dies(), case("ignored").sigpipe_ignored()]
        })
        .collect()
}

#[test]
fn ociocheck() {
    let cases = closed_cases(&[
        (
            "check",
            "stdout",
            vec!["--iconfig", "a.ocio", "--oconfig", "o.ocio"],
        ),
        ("help", "stdout", vec!["--help"]),
    ]);
    let summary = sweep(
        "sigpipe_ociocheck",
        "ociocheck",
        env!("CARGO_BIN_EXE_ociocheck"),
        &cases,
    );
    // The default case died before it wrote; ignored, it went on and wrote the config.
    assert_eq!(summary.writing_files(), 1);
}

#[test]
fn ociowrite() {
    let cases = closed_cases(&[
        ("stderr", "stderr", vec!["--file", "o.txt"]),
        (
            "verbose",
            "stdout",
            vec!["--v", "--colorspaces", "raw", "lin", "--file", "o.ctf"],
        ),
    ]);
    sweep(
        "sigpipe_ociowrite",
        "ociowrite",
        env!("CARGO_BIN_EXE_ociowrite"),
        &cases,
    );
}

#[test]
fn ociochecklut() {
    let lut = test_file("lut1d_1.spi1d");
    let cases = closed_cases(&[("test", "stdout", vec![lut.as_str(), "-t"])]);
    sweep(
        "sigpipe_ociochecklut",
        "ociochecklut",
        env!("CARGO_BIN_EXE_ociochecklut"),
        &cases,
    );
}

#[test]
fn ociobakelut() {
    let cases = closed_cases(&[
        ("bake", "stdout", BIG_BAKE.to_vec()),
        ("help", "stdout", vec!["--help"]),
    ]);
    sweep(
        "sigpipe_ociobakelut",
        "ociobakelut",
        env!("CARGO_BIN_EXE_ociobakelut"),
        &cases,
    );
}

/// The shell line `ociobakelut ... --stdout | head -c 3`: the tool's output doesn't fit the pipe,
/// so it is still writing when `head` goes away, and `PIPESTATUS` is 141.
#[test]
fn head_of_a_long_output() {
    let cases = vec![
        Case::new("three_bytes", &BIG_BAKE)
            .file("a.ocio", CONFIG)
            .head_bytes(3),
        Case::new("one_line", &BIG_BAKE)
            .file("a.ocio", CONFIG)
            .head_bytes(40),
    ];
    let summary = sweep(
        "sigpipe_head",
        "ociobakelut",
        env!("CARGO_BIN_EXE_ociobakelut"),
        &cases,
    );
    assert_eq!(summary.with_exit_code(141), cases.len());
}
