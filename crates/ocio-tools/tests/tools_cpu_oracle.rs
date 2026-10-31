// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tools' output that depends on the CPU: pixels through `ociochecklut`, and the LUT values
//! `ociobakelut` computes. On each CPU the wheel and the port pick the same kernels, so this
//! target is in the `cpu-tests` alias (`.cargo/config.toml`), which CI runs on emulated CPUs. The
//! full sweeps stay out of it: this one holds a few cases per kind of op, so that each CPU takes
//! seconds natively and minutes under SDE.

mod common;

use common::{Case, sweep, test_file};

/// LUTs with a kernel of their own: 1D, 1D with half domain, 3D, inverse-capable, matrix, log,
/// CDL, range, gamma, fixed function, exposure/contrast.
const FILES: [&str; 11] = [
    "lut1d_1.spi1d",
    "clf/lut1d_half_domain_raw_half_set.clf",
    "lut3d_1.spi3d",
    "clf/lut3d_17x17x17_10i_12i.clf",
    "matrix_example4x4.ctf",
    "log_lintolog_3chan.ctf",
    "clf/cdl_all_styles.clf",
    "clf/range.clf",
    "gamma_test1.ctf",
    "fixed_function.ctf",
    "exposure_contrast_log.ctf",
];

#[test]
fn pixels_through_ociochecklut() {
    const TOOL: &str = "ociochecklut";
    const PROGRAM: &str = env!("CARGO_BIN_EXE_ociochecklut");
    let mut cases = Vec::new();
    for name in FILES {
        let file = test_file(name);
        let file = file.as_str();
        for (label, options) in [
            ("pixel", vec!["0.25", "0.5", "0.75", "0.5"]),
            ("nan", vec!["nan", "inf", "-0.1"]),
            ("test", vec!["-t"]),
            ("steps", vec!["-s", "0.25", "0.5", "0.75"]),
            ("inverse", vec!["--inv", "0.25", "0.5", "0.75"]),
        ] {
            let argv = [&[file][..], &options].concat();
            cases.push(Case::new(format!("{name} {label}"), &argv));
        }
    }
    let summary = sweep("tools_cpu_checklut", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 30,
        "{}",
        summary.with_exit_code(0)
    );
}

#[test]
fn values_through_ociobakelut() {
    const TOOL: &str = "ociobakelut";
    const PROGRAM: &str = env!("CARGO_BIN_EXE_ociobakelut");
    let lut1d = test_file("lut1d_1.spi1d");
    let lut3d = test_file("lut3d_1.spi3d");
    let chains: [(&str, Vec<&str>); 4] = [
        ("lut1d", vec!["--lut", &lut1d]),
        ("lut3d", vec!["--lut", &lut3d]),
        (
            "inverse",
            vec!["--invlut", &lut1d, "--slope", "1.1", "1", "0.9"],
        ),
        (
            "cdl",
            vec![
                "--slope", "1.1", "1", "0.9", "--power", "1.2", "1", "0.8", "--sat", "0.9",
            ],
        ),
    ];
    let mut cases = Vec::new();
    for (label, chain) in &chains {
        for (format, cube) in [("spi3d", "5"), ("cinespace", "5"), ("spi1d", "33")] {
            let argv = [
                &chain[..],
                &["--format", format, "--cubesize", cube, "--stdout"],
            ]
            .concat();
            cases.push(Case::new(format!("{label} {format}"), &argv));
        }
    }
    let summary = sweep("tools_cpu_bakelut", TOOL, PROGRAM, &cases);
    assert!(
        summary.with_exit_code(0) >= 6,
        "{}",
        summary.with_exit_code(0)
    );
}
