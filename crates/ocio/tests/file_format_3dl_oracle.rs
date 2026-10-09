// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `.3dl` reader against the wheel (`processor_ops`): the processors of file transforms
//! of generated files and of upstream's, both directions of the processor and of the
//! transform, the LUTs by their bits; the bit depths it infers (each bound, with the 2x
//! overshoot), shapers it takes for an identity and those it keeps, the lines it skips, and
//! the errors of the files it refuses; and what it logs for the interpolations a LUT doesn't
//! take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.3dl` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.3dl", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.3dl"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// The entries of a 3D LUT of `size` entries per side, blue fastest, whose largest value is
/// `max`.
fn entries(size: usize, max: i64) -> Vec<String> {
    let mut out = Vec::new();
    let s = (size - 1) as i64;
    for r in 0..size as i64 {
        for g in 0..size as i64 {
            for b in 0..size as i64 {
                out.push(format!(
                    "{} {} {}",
                    max * (9 * r + g) / (10 * s),
                    max * (8 * g + b) / (9 * s),
                    max * b / s
                ));
            }
        }
    }
    out
}

/// A shaper of `n` values up to `max`, each moved by `offset(i)`.
fn shaper(n: usize, max: i64, offset: impl Fn(usize) -> i64) -> String {
    (0..n)
        .map(|i| (max * i as i64 / (n as i64 - 1) + offset(i)).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A file of `header` lines, then `data`, each line ended by `eol`.
fn lut3dl(header: &[&str], data: &[String], eol: &str) -> String {
    let mut text = String::new();
    for line in header
        .iter()
        .copied()
        .chain(data.iter().map(String::as_str))
    {
        text.push_str(line);
        text.push_str(eol);
    }
    text
}

/// Files the reader takes and refuses, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: &str, text: String| cases.push(case(label, text.as_bytes()));
    let d = entries(2, 1023);

    add("3D of 2, 10 bits", lut3dl(&[], &d, "\n"));
    add("CRLF", lut3dl(&[], &d, "\r\n"));
    add("CR", lut3dl(&[], &d, "\r"));
    add("3D of 5, 12 bits", lut3dl(&[], &entries(5, 4095), "\n"));
    add("3D of 17", lut3dl(&[], &entries(17, 4095), "\n"));
    for max in [
        128i64,
        255,
        511,
        512,
        1023,
        2047,
        2048,
        4095,
        8191,
        8192,
        16383,
        65535,
        131071,
        131072,
        2_000_000_000,
    ] {
        add(
            &format!("3D of max {max}"),
            lut3dl(&[], &entries(2, max), "\n"),
        );
    }
    add("3D of max 127", lut3dl(&[], &entries(2, 127), "\n"));
    add(
        "the Lustre layout",
        lut3dl(
            &[
                "#Tokens required by applications - do not edit",
                "",
                "3DMESH",
                "Mesh 1 12",
                &shaper(17, 1023, |_| 0),
            ],
            &[
                entries(2, 4095),
                vec!["".into(), "LUT8".into(), "gamma 1.0".into()],
            ]
            .concat(),
            "\n",
        ),
    );
    for (label, s) in [
        ("an identity shaper", shaper(17, 1023, |_| 0)),
        (
            "a shaper 1 off",
            shaper(17, 1023, |i| if i == 5 { 1 } else { 0 }),
        ),
        (
            "a shaper 2 off",
            shaper(17, 1023, |i| if i == 5 { 2 } else { 0 }),
        ),
        (
            "a shaper -2 off",
            shaper(17, 1023, |i| if i == 5 { -2 } else { 0 }),
        ),
        ("a gamma shaper", {
            (0..33)
                .map(|i| ((i as f64 / 32.0).powf(0.6) * 4095.0).round().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        }),
        ("a 16-bit shaper", shaper(9, 65535, |_| 0)),
        ("an 8-bit shaper", shaper(4, 255, |i| i as i64)),
        ("a shaper of 4 values", shaper(4, 1023, |_| 0)),
    ] {
        add(label, lut3dl(&[&s], &d, "\n"));
        add(&format!("{label}, alone"), lut3dl(&[&s], &[], "\n"));
    }
    // Shapers whose steps are whole code values: 341 apart, one value exactly 2 off.
    add(
        "a shaper exactly 2 off",
        lut3dl(&["0 343 682 1023"], &d, "\n"),
    );
    add(
        "a shaper exactly 1 off",
        lut3dl(&["0 342 682 1023"], &d, "\n"),
    );
    add("a shaper of max 128", lut3dl(&["0 32 64 128"], &d, "\n"));
    // The largest value in blue only, then in green only.
    let blue: Vec<String> = (0..8)
        .map(|i| format!("{} {} {}", i * 10, i * 12, i * 146))
        .collect();
    add("the largest value in blue", lut3dl(&[], &blue, "\n"));
    let green: Vec<String> = (0..8)
        .map(|i| format!("{} {} {}", i * 10, i * 146, i * 12))
        .collect();
    add("the largest value in green", lut3dl(&[], &green, "\n"));
    add(
        "a shaper after the entries",
        lut3dl(
            &[],
            &[d.clone(), vec![shaper(17, 1023, |i| i as i64)]].concat(),
            "\n",
        ),
    );
    add(
        "a shaper of max 127",
        lut3dl(&[&shaper(17, 127, |_| 0)], &d, "\n"),
    );
    add(
        "two shapers",
        lut3dl(
            &[&shaper(17, 1023, |_| 0), &shaper(17, 1023, |_| 0)],
            &d,
            "\n",
        ),
    );
    add(
        "a line starting with <",
        lut3dl(&["<?xml version=\"1.0\"?>"], &d, "\n"),
    );
    add("an indented <", lut3dl(&["  <a>"], &d, "\n"));
    add("a # after text", lut3dl(&["x # y"], &d, "\n"));
    add("an indented comment", lut3dl(&["   # c"], &d, "\n"));
    let with = |i: usize, line: &str| {
        let mut e = d.clone();
        e[i] = line.to_owned();
        e
    };
    for (label, line) in [
        ("a line of 1 integer", "12"),
        ("a line of 2 integers", "12 13"),
        ("floats", "0.5 0.5 0.5"),
        ("a word", "0 x 0"),
        ("an integer past int", "4294967296 0 0"),
        ("negative values", "-5 -5 -5"),
        ("signs", "+5 0 -0"),
        ("an integer with a suffix", "5x 0 0"),
        ("tabs", "\t0\t0\t0\t"),
        ("a NUL", "0 0\0 0"),
    ] {
        add(label, lut3dl(&[], &with(1, line), "\n"));
    }
    let mut missing = d.clone();
    missing.remove(3);
    add("an entry missing", lut3dl(&[], &missing, "\n"));
    let mut extra = d.clone();
    extra.push("1 1 1".into());
    add("an entry too many", lut3dl(&[], &extra, "\n"));
    let negative: Vec<String> = d
        .iter()
        .map(|l| format!("-{}", l.replace(' ', " -")))
        .collect();
    add("all negative", lut3dl(&[], &negative, "\n"));
    let mut long = d.clone();
    long.insert(3, format!("0 0 0 {}", "1 ".repeat(3000)));
    add("a line past the buffer", lut3dl(&[], &long, "\n"));
    add(
        "keywords only",
        lut3dl(&["3DMESH", "Mesh 4 12", "LUT8", "gamma 1.0"], &[], "\n"),
    );
    add("empty", String::new());
    cases
}

#[test]
fn files_3dl_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "3dl") {
            let bytes = std::fs::read(&path).unwrap();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            cases.push(case(&name, &bytes));
            cases.push(case_with(
                &format!("{name} inverse transform"),
                &bytes,
                Interpolation::Tetrahedral,
                TransformDirection::Inverse,
            ));
        }
    }
    check_file_processors(&cases);
}

/// The file transform's interpolation: kept when the LUTs take it, else a warning, and the
/// default.
#[test]
fn interpolations_read_and_log_as_in_the_wheel() {
    let d = entries(2, 1023);
    let files = [
        ("3D", lut3dl(&[], &d, "\n")),
        (
            "shaper and 3D",
            lut3dl(
                &[&shaper(17, 1023, |i| if i == 5 { 9 } else { 0 })],
                &d,
                "\n",
            ),
        ),
        (
            "shaper",
            lut3dl(
                &[&shaper(17, 1023, |i| if i == 5 { 9 } else { 0 })],
                &[],
                "\n",
            ),
        ),
    ];
    let mut cases = Vec::new();
    for (name, text) in &files {
        for interp in [
            Interpolation::Unknown,
            Interpolation::Nearest,
            Interpolation::Linear,
            Interpolation::Tetrahedral,
            Interpolation::Cubic,
            Interpolation::Default,
            Interpolation::Best,
        ] {
            cases.push(case_with(
                &format!("{name}, {interp:?}"),
                text.as_bytes(),
                interp,
                TransformDirection::Forward,
            ));
        }
    }
    {
        let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
        check_file_processors(&cases);
    }
    common::lut_files::check_logs(&cases, TransformDirection::Forward);
    common::lut_files::check_logs(&cases, TransformDirection::Inverse);
}
