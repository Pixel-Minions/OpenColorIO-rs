// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociochecklut` against the wheel's: stdout, stderr and exit code.
//!
//! Its pixels are printed with 7 significant digits, and come from the same CPU and math
//! library on both sides, as CLAUDE.md requires for pixels: the wheel runs live.

mod common;

use std::path::{Path, PathBuf};

use common::{Case, sweep, test_file};
use ocio_testkit::paths;

const TOOL: &str = "ociochecklut";
const PROGRAM: &str = env!("CARGO_BIN_EXE_ociochecklut");

/// A LUT with one 1D step: `lut1d_1.spi1d`; and one with several steps.
fn lut() -> String {
    test_file("lut1d_1.spi1d")
}

fn multiple_ops() -> String {
    test_file("clf/multiple_ops.clf")
}

/// Every file of upstream's test data, however it ends: what isn't a LUT is an error to compare.
fn every_test_file() -> Vec<String> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&paths::upstream_dir().join("tests/data/files"), &mut found);
    assert!(found.len() >= 250, "{}", found.len());
    found
        .iter()
        .map(|path| path.to_str().unwrap().replace('\\', "/"))
        .collect()
}

#[test]
fn bad_arguments() {
    let lut = lut();
    let lut = lut.as_str();
    let cases: Vec<Case> = [
        ("no_arguments", vec![]),
        ("help", vec!["--help"]),
        ("help_one_dash", vec!["-help"]),
        ("help_with_file", vec![lut, "--help"]),
        ("help_with_error", vec!["--help", "--bogus"]),
        ("error_with_help", vec!["--bogus", "--help"]),
        ("invalid", vec!["--bogus"]),
        ("invalid_short", vec!["-x"]),
        ("two_dashes", vec!["--"]),
        ("test_two_dashes", vec![lut, "--t"]),
        ("verbose_two_dashes", vec![lut, "--v"]),
        ("steps_two_dashes", vec![lut, "--s", "0.5", "0.5", "0.5"]),
        ("inv_one_dash", vec![lut, "-inv", "0.5", "0.5", "0.5"]),
        ("gpu", vec![lut, "--gpu"]),
        ("gpu_pixel", vec![lut, "--gpu", "0.5", "0.5", "0.5"]),
        ("gpulegacy", vec![lut, "--gpulegacy"]),
        ("gpuinfo", vec![lut, "--gpuinfo"]),
        ("gpu_verbose", vec![lut, "-v", "--gpu"]),
        ("gpu_without_file", vec!["--gpu"]),
        ("test_without_file", vec!["-t"]),
        ("empty_file", vec![""]),
        ("missing_file", vec!["not_there.spi1d"]),
        ("missing_file_verbose", vec!["-v", "not_there.spi1d"]),
        ("missing_file_pixel", vec!["not_there.spi1d", "1", "2", "3"]),
        ("not_a_lut", vec!["."]),
        ("dash_is_a_file", vec!["-"]),
        ("one_value", vec![lut, "0.5"]),
        ("two_values", vec![lut, "0.5", "0.5"]),
        ("five_values", vec![lut, "1", "2", "3", "4", "5"]),
        ("six_values", vec![lut, "1", "2", "3", "4", "5", "6"]),
        ("test_with_pixel", vec![lut, "-t", "0.5", "0.5", "0.5"]),
        ("pixel_with_test", vec![lut, "0.5", "0.5", "0.5", "-t"]),
        ("option_like_value", vec![lut, "-inf", "0", "0"]),
        ("option_after_values", vec![lut, "0.5", "0.5", "0.5", "-v"]),
        (
            "file_after_options",
            vec!["-v", "-s", lut, "0.5", "0.5", "0.5"],
        ),
    ]
    .into_iter()
    .map(|(label, argv)| Case::new(label, &argv))
    .collect();
    let summary = sweep("ociochecklut_bad_arguments", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(1) >= 20,
        "{}",
        summary.with_exit_code(1)
    );
    assert!(
        summary.with_exit_code(0) >= 6,
        "{}",
        summary.with_exit_code(0)
    );
}

/// The text of values: what `strtof` makes of it, and how each value prints and aligns.
#[test]
fn pixel_values() {
    let lut = test_file("matrix_example4x4.ctf");
    let values: Vec<[&str; 3]> = vec![
        ["0", "0", "0"],
        ["1", "2", "3"],
        ["-1", "-2", "-3"],
        ["-0", "-0.0", "+0"],
        ["0.18", "0.5", "100"],
        ["1e-3", "1e3", "1E10"],
        ["1e-40", "1e-45", "1e-46"],
        ["1e38", "1e39", "-1e39"],
        ["1234567", "12345678", "0.1234567891"],
        ["0.000012345", "123456.7", "9999999.5"],
        ["abc", "1abc", "1 2"],
        ["", " 1", "\t2"],
        ["nan", "NaN", "nan(ind)"],
        ["nan(snan)", "nan(1)", "nan(abc)"],
        ["inf", "infinity", "+inf"],
        ["0x10", "0x1p3", "0x"],
        ["1,5", "1.5.5", ".5"],
    ];
    let mut cases = Vec::new();
    for (index, [r, g, b]) in values.iter().enumerate() {
        for (label, argv) in [
            ("rgb", vec![lut.as_str(), r, g, b]),
            ("rgba", vec![lut.as_str(), r, g, b, "0.25"]),
            ("verbose", vec![lut.as_str(), "-v", r, g, b]),
            ("verbose_rgba", vec![lut.as_str(), "-v", r, g, b, "1e-5"]),
            ("steps", vec![lut.as_str(), "-s", r, g, b]),
            ("inverse", vec![lut.as_str(), "--inv", r, g, b]),
        ] {
            cases.push(Case::new(format!("{index}_{label}"), &argv));
        }
    }
    let summary = sweep("ociochecklut_pixels", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 90,
        "{}",
        summary.with_exit_code(0)
    );
}

#[test]
fn test_values_and_steps() {
    let lut = lut();
    let multiple_ops = multiple_ops();
    let mut cases = Vec::new();
    for (name, file) in [("one", lut.as_str()), ("several", multiple_ops.as_str())] {
        for (label, options) in [
            ("test", vec!["-t"]),
            ("test_verbose", vec!["-t", "-v"]),
            ("test_steps", vec!["-t", "-s"]),
            ("test_verbose_steps", vec!["-t", "-v", "-s"]),
            ("test_inverse", vec!["-t", "--inv"]),
            ("ops", vec![]),
            ("ops_verbose", vec!["-v"]),
            ("ops_inverse", vec!["--inv"]),
            ("ops_steps", vec!["-s"]),
            ("steps", vec!["-s", "0.5", "0.25", "0.125"]),
            ("steps_alpha", vec!["-s", "0.5", "0.25", "0.125", "0.75"]),
            ("steps_verbose", vec!["-s", "-v", "0.5", "0.25", "0.125"]),
            ("steps_inverse", vec!["-s", "--inv", "0.5", "0.25", "0.125"]),
        ] {
            let mut argv = vec![file];
            argv.extend(options);
            cases.push(Case::new(format!("{name}_{label}"), &argv));
        }
    }
    let levels = ["none", "warning", "info", "debug", "bogus"];
    for level in levels {
        cases.push(
            Case::new(format!("log_{level}"), &[multiple_ops.as_str()])
                .env("OCIO_LOGGING_LEVEL", level),
        );
        cases.push(
            Case::new(
                format!("log_{level}_pixel"),
                &[multiple_ops.as_str(), "0.5", "0.5", "0.5"],
            )
            .env("OCIO_LOGGING_LEVEL", level),
        );
        cases.push(
            Case::new(format!("log_{level}_stopped_early"), &["-v"])
                .env("OCIO_LOGGING_LEVEL", level),
        );
    }
    let summary = sweep("ociochecklut_options", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 30,
        "{}",
        summary.with_exit_code(0)
    );
}

/// Files written for the sweep: what upstream's data doesn't have.
#[test]
fn files_of_the_sweep() {
    let spi1d = "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n";
    let cases = vec![
        Case::new("relative", &["a.spi1d", "0.5", "0.5", "0.5"]).file("a.spi1d", spi1d),
        Case::new("in_a_directory", &["d/a.spi1d", "-t"]).file("d/a.spi1d", spi1d),
        Case::new("relative_ops", &["a.spi1d"]).file("a.spi1d", spi1d),
        Case::new("unknown_extension", &["a.nope"]).file("a.nope", spi1d),
        Case::new("empty", &["a.spi1d"]).file("a.spi1d", ""),
        Case::new("truncated", &["a.spi1d"]).file("a.spi1d", "Version 1\nFrom 0.0 1.0\n"),
        // Alpha reaches the color channels, and a pixel of 3 values has an alpha of 0.
        Case::new("alpha_into_red", &["a.ctf", "0.25", "0.5", "0.75"]).file(
            "a.ctf",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <ProcessList version=\"1.3\" id=\"a\">\n\
             <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"><Array dim=\"4 4 4\">\
             1 0 0 0.5 0 1 0 0 0 0 1 0 0 0 0 1</Array></Matrix>\n\
             </ProcessList>\n",
        ),
        Case::new("alpha_into_red_test", &["a.ctf", "-t"]).file(
            "a.ctf",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <ProcessList version=\"1.3\" id=\"a\">\n\
             <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"><Array dim=\"4 4 4\">\
             1 0 0 0.5 0 1 0 0 0 0 1 0 0 0 0 1</Array></Matrix>\n\
             </ProcessList>\n",
        ),
        // Nothing left after the processor drops an identity.
        Case::new("no_transform", &["a.spimtx"]).file("a.spimtx", "1 0 0 0\n0 1 0 0\n0 0 1 0\n"),
        Case::new("no_transform_steps", &["a.spimtx", "-s", "1", "2", "3"])
            .file("a.spimtx", "1 0 0 0\n0 1 0 0\n0 0 1 0\n"),
        // A key the CTF reader warns about, printed on stdout through the tool's function.
        Case::new("warning_while_reading", &["a.ctf"]).file(
            "a.ctf",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <ProcessList version=\"1.3\" id=\"a\">\n\
             <Unknown>x</Unknown>\n\
             <Matrix inBitDepth=\"32f\" outBitDepth=\"32f\"><Array dim=\"3 3 3\">\
             2 0 0 0 2 0 0 0 2</Array></Matrix>\n\
             </ProcessList>\n",
        ),
    ];
    let summary = sweep("ociochecklut_files", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 4,
        "{}",
        summary.with_exit_code(0)
    );
}

/// Every file of upstream's test data, each way of using it.
#[test]
fn every_file_of_upstream() {
    let files = every_test_file();
    let mut cases = Vec::new();
    for file in &files {
        let name = file.rsplit("files/").next().unwrap();
        for (label, options) in [
            ("ops", vec![]),
            ("ops_inverse", vec!["--inv"]),
            ("test", vec!["-t"]),
            ("pixel", vec!["0.25", "0.5", "0.75", "0.5"]),
            ("inverse", vec!["--inv", "0.25", "0.5", "0.75"]),
            ("steps", vec!["-s", "-v", "0.25", "0.5", "0.75"]),
        ] {
            let mut argv = vec![file.as_str()];
            argv.extend(options);
            cases.push(Case::new(format!("{name} {label}"), &argv));
        }
    }
    let summary = sweep("ociochecklut_upstream", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 500,
        "{}",
        summary.with_exit_code(0)
    );
    assert!(
        summary.with_exit_code(1) >= 100,
        "{}",
        summary.with_exit_code(1)
    );
}
