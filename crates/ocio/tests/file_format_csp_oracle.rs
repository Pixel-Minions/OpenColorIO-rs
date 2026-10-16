// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The cineSpace reader against the wheel (`processor_ops`): the processors of file
//! transforms of generated `.csp` files and of upstream's, both directions of the processor
//! and of the transform, the 65536-entry prelut each prelut resamples to, the LUTs and the
//! ranges by their bits; preluts of 2 to 11 points, monotonic or not, with repeated points,
//! infinities and NaNs of either sign in every position (where the wheels' operand orders
//! decide which NaN a sum returns); the errors of the files it refuses; and what it logs for
//! the interpolations a LUT doesn't take. A metadata block never closed is left out: the wheel
//! never returns from it (U-72).

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.csp` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.csp", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.csp"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// The entries of a 3D LUT of `size` entries per side, red fastest.
fn cube(size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let s = (size - 1) as f64;
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let (rf, gf, bf) = (r as f64 / s, g as f64 / s, b as f64 / s);
                out.push(format!(
                    "{} {} {}",
                    0.9 * rf + 0.05 * gf,
                    0.8 * gf + 0.1 * bf * bf,
                    0.7 * bf + 0.2 * rf * gf
                ));
            }
        }
    }
    out
}

/// A 1D LUT's entries.
fn lut1d(size: usize) -> Vec<String> {
    (0..size)
        .map(|i| {
            let x = i as f64 / (size - 1) as f64;
            format!("{} {} {}", x * x, x.sqrt(), 0.5 * x + 0.1)
        })
        .collect()
}

/// A prelut channel: its count, inputs and outputs.
fn channel(inputs: &str, outputs: &str) -> Vec<String> {
    let n = inputs.split_whitespace().count();
    vec![n.to_string(), inputs.to_owned(), outputs.to_owned()]
}

/// The identity channel of 2 points.
fn identity() -> Vec<String> {
    channel("0.0 1.0", "0.0 1.0")
}

/// A file of `kind` (`1D` or `3D`), the metadata block `metadata` (left out when empty), the
/// 3 prelut channels, then `data`.
fn csp(kind: &str, metadata: &[&str], channels: &[Vec<String>; 3], data: &[String]) -> Vec<String> {
    let mut out = vec!["CSPLUTV100".to_owned(), kind.to_owned(), "".to_owned()];
    if !metadata.is_empty() {
        out.push("BEGIN METADATA".into());
        out.extend(metadata.iter().map(|m| (*m).to_owned()));
        out.push("END METADATA".into());
        out.push("".into());
    }
    for c in channels {
        out.extend(c.iter().cloned());
    }
    out.push("".into());
    out.extend(data.iter().cloned());
    out
}

fn csp_3d(channels: &[Vec<String>; 3]) -> Vec<String> {
    csp(
        "3D",
        &["meta"],
        channels,
        &[vec!["2 2 2".to_owned()], cube(2)].concat(),
    )
}

/// The text of `lines`, each ended by `eol`.
fn text(lines: &[String], eol: &str) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        out.push_str(eol);
    }
    out
}

/// Files the reader takes and refuses, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: &str, lines: Vec<String>| {
        cases.push(case(label, text(&lines, "\n").as_bytes()));
    };
    let gamma = || {
        channel(
            "0.0 0.1 0.2 0.35 0.5 0.75 1.0",
            "0.0 0.3 0.45 0.6 0.72 0.88 1.0",
        )
    };

    // Preluts of every size and shape, on a 3D LUT.
    add(
        "identity preluts",
        csp_3d(&[identity(), identity(), identity()]),
    );
    add("a gamma prelut", csp_3d(&[gamma(), identity(), identity()]));
    add("three preluts", csp_3d(&[gamma(), gamma(), gamma()]));
    for (label, inputs, outputs) in [
        ("2 points", "0.0 1.0", "0.0 2.0"),
        ("3 points", "0.0 0.1 1.0", "0.0 0.2 2.0"),
        ("4 points", "0.0 0.25 0.5 1.0", "0.0 0.1 0.6 1.0"),
        ("5 points", "-0.5 0.0 0.5 1.0 4.0", "0.0 0.1 0.5 0.9 1.0"),
        (
            "11 points",
            "0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1",
            "0 0.01 0.04 0.09 0.16 0.25 0.36 0.49 0.64 0.81 1",
        ),
        ("a decreasing range", "1.0 0.6 0.3 0.0", "0.0 0.4 0.7 1.0"),
        ("inputs out of order", "0.0 0.7 0.3 1.0", "0.0 0.2 0.5 1.0"),
        ("a repeated input", "0.0 0.5 0.5 1.0", "0.0 0.3 0.6 1.0"),
        (
            "the first input repeated",
            "0.0 0.0 0.5 1.0",
            "0.0 0.3 0.6 1.0",
        ),
        (
            "the last input repeated",
            "0.0 0.5 1.0 1.0",
            "0.0 0.3 0.6 1.0",
        ),
        ("one input", "0.5 0.5 0.5 0.5", "0.0 0.3 0.6 1.0"),
        ("an infinite output", "0.0 0.3 0.6 1.0", "0.0 inf 0.6 1.0"),
        ("an infinite input", "0.0 0.3 inf 1.0", "0.0 0.2 0.6 1.0"),
        (
            "a negative infinite input first",
            "-inf 0.3 0.6 1.0",
            "0.0 0.2 0.6 1.0",
        ),
        ("a 2-point infinite range", "0.0 inf", "0.0 1.0"),
        (
            "an input of 1e-30 steps",
            "0.0 1e-30 2e-30 1.0",
            "0.0 0.1 0.2 1.0",
        ),
    ] {
        add(
            &format!("a prelut of {label}"),
            csp_3d(&[channel(inputs, outputs), identity(), identity()]),
        );
    }
    // NaNs of both signs at each position of a 5-point prelut, alone and together.
    let nan_sets: [(&str, &str, &str); 20] = [
        ("output 0", "0.0 0.25 0.5 0.75 1.0", "nan 0.2 0.5 0.7 1.0"),
        ("output 1", "0.0 0.25 0.5 0.75 1.0", "0.0 nan 0.5 0.7 1.0"),
        ("output 2", "0.0 0.25 0.5 0.75 1.0", "0.0 0.2 nan 0.7 1.0"),
        ("output 3", "0.0 0.25 0.5 0.75 1.0", "0.0 0.2 0.5 nan 1.0"),
        ("output 4", "0.0 0.25 0.5 0.75 1.0", "0.0 0.2 0.5 0.7 nan"),
        (
            "outputs 0 and 1",
            "0.0 0.25 0.5 0.75 1.0",
            "nan -nan 0.5 0.7 1.0",
        ),
        (
            "outputs 1 and 0",
            "0.0 0.25 0.5 0.75 1.0",
            "-nan nan 0.5 0.7 1.0",
        ),
        (
            "outputs 0, 1 and 2",
            "0.0 0.25 0.5 0.75 1.0",
            "-nan nan -nan 0.7 1.0",
        ),
        (
            "outputs 1, 2 and 3",
            "0.0 0.25 0.5 0.75 1.0",
            "0.0 -nan nan -nan 1.0",
        ),
        (
            "outputs 2, 3 and 4",
            "0.0 0.25 0.5 0.75 1.0",
            "0.0 0.2 nan -nan nan",
        ),
        (
            "outputs 3 and 4",
            "0.0 0.25 0.5 0.75 1.0",
            "0.0 0.2 0.5 -nan nan",
        ),
        (
            "all outputs",
            "0.0 0.25 0.5 0.75 1.0",
            "nan -nan nan -nan nan",
        ),
        ("input 2", "0.0 0.25 nan 0.75 1.0", "0.0 0.2 0.5 0.7 1.0"),
        // A middle interval whose start is a NaN, of another sign than its value's: the
        // product and the sum the cubic starts with meet two NaNs.
        (
            "input 2 and output 2",
            "0.0 0.25 -nan 0.75 1.0",
            "0.0 0.2 nan 0.7 1.0",
        ),
        (
            "input 2 and output 2, swapped",
            "0.0 0.25 nan 0.75 1.0",
            "0.0 0.2 -nan 0.7 1.0",
        ),
        // The same, with the slope at the interval's start a NaN of the other sign (from the
        // value before it).
        (
            "input 2 and output 1",
            "0.0 0.25 -nan 0.75 1.0",
            "0.0 nan 0.5 0.7 1.0",
        ),
        (
            "input 2 and output 1, swapped",
            "0.0 0.25 nan 0.75 1.0",
            "0.0 -nan 0.5 0.7 1.0",
        ),
        // A range of two NaNs: the lerp meets them.
        (
            "the first and last inputs",
            "nan 0.25 0.5 0.75 -nan",
            "0.0 0.2 0.5 0.7 1.0",
        ),
        (
            "the first and last inputs, swapped",
            "-nan 0.25 0.5 0.75 nan",
            "0.0 0.2 0.5 0.7 1.0",
        ),
        (
            "inputs and outputs",
            "0.0 -nan 0.5 nan 1.0",
            "nan 0.2 -nan 0.7 nan",
        ),
    ];
    for (label, inputs, outputs) in nan_sets {
        add(
            &format!("NaNs: {label}"),
            csp_3d(&[channel(inputs, outputs), identity(), identity()]),
        );
    }
    add(
        "NaNs: 3 points",
        csp_3d(&[
            channel("0.0 0.5 1.0", "nan -nan nan"),
            identity(),
            identity(),
        ]),
    );
    add(
        "NaNs: 4 points",
        csp_3d(&[
            channel("0.0 0.3 0.6 1.0", "-nan nan -nan nan"),
            identity(),
            identity(),
        ]),
    );
    add(
        "NaNs: 2 points",
        csp_3d(&[channel("0.0 1.0", "-nan nan"), identity(), identity()]),
    );
    add(
        "NaNs: infinities and NaNs",
        csp_3d(&[
            channel("0.0 0.25 0.5 0.75 1.0", "inf nan -inf -nan inf"),
            channel("0.0 0.25 0.5 0.75 1.0", "-inf -nan 0.5 nan -inf"),
            identity(),
        ]),
    );
    add(
        "a 1D LUT with a prelut",
        csp(
            "1D",
            &["meta"],
            &[gamma(), identity(), gamma()],
            &[vec!["5".to_owned()], lut1d(5)].concat(),
        ),
    );
    add(
        "a 1D LUT without a prelut",
        csp(
            "1D",
            &[],
            &[identity(), identity(), identity()],
            &[vec!["5".to_owned()], lut1d(5)].concat(),
        ),
    );
    add(
        "a 1D LUT of 1 entry",
        csp(
            "1D",
            &[],
            &[identity(), identity(), identity()],
            &[vec!["1".to_owned()], lut1d(2)[..1].to_vec()].concat(),
        ),
    );
    add(
        "a 3D LUT of 1 per side",
        csp(
            "3D",
            &[],
            &[identity(), identity(), identity()],
            &["1 1 1".to_owned(), "0.5 0.5 0.5".to_owned()],
        ),
    );
    add(
        "a 3D LUT of 3 per side",
        csp(
            "3D",
            &[],
            &[gamma(), identity(), identity()],
            &[vec!["3 3 3".to_owned()], cube(3)].concat(),
        ),
    );
    add(
        "prelut counts of 0 and 1",
        csp_3d(&[vec!["0".into()], vec!["1".into()], gamma()]),
    );
    add(
        "a prelut equal within 1e-6",
        csp_3d(&[
            channel("0.0 0.5 1.0", "0.0 0.5000001 1.0"),
            identity(),
            identity(),
        ]),
    );

    // The header and the metadata.
    for (label, first, kind) in [
        ("a lower-case header", "csplutv100", "3D"),
        ("an indented header", "   CSPLUTV100 and more", "3D"),
        ("a header of another version", "CSPLUTV101", "3D"),
        ("a lower-case type", "CSPLUTV100", "3d"),
        ("a type with a trailing space", "CSPLUTV100", "3D "),
        ("a type with more", "CSPLUTV100", "3D LUT"),
        ("an unknown type", "CSPLUTV100", "2D"),
        ("an indented type", "CSPLUTV100", "  3D"),
    ] {
        let mut lines = csp_3d(&[gamma(), identity(), identity()]);
        lines[0] = first.to_owned();
        lines[1] = kind.to_owned();
        add(label, lines);
    }
    add(
        "metadata of several lines",
        csp(
            "3D",
            &["a b c", "", "   indented", "END METADATA? no"],
            &[gamma(), identity(), identity()],
            &[vec!["2 2 2".to_owned()], cube(2)].concat(),
        ),
    );
    add(
        "an empty metadata block",
        csp(
            "3D",
            &[],
            &[gamma(), identity(), identity()],
            &[vec!["2 2 2".to_owned()], cube(2)].concat(),
        )
        .into_iter()
        .flat_map(|l| {
            if l == "3D" {
                vec![l, "BEGIN METADATA".into(), "END METADATA".into()]
            } else {
                vec![l]
            }
        })
        .collect(),
    );
    add(
        "metadata in other cases",
        csp_3d(&[gamma(), identity(), identity()])
            .into_iter()
            .map(|l| match l.as_str() {
                "BEGIN METADATA" => "  begin metadata please".to_owned(),
                "END METADATA" => "end METADATA now".to_owned(),
                _ => l,
            })
            .collect(),
    );

    // The preluts' errors.
    for (label, count) in [
        ("a prelut count that doesn't parse", "two"),
        ("a negative prelut count", "-2"),
        ("a prelut count with a suffix", "2x"),
    ] {
        let mut ch = gamma();
        ch[0] = count.to_owned();
        add(label, csp_3d(&[ch, identity(), identity()]));
    }
    add(
        "a prelut count too high",
        csp_3d(&[
            vec![
                "8".into(),
                "0 0.1 0.2 0.35 0.5 0.75 1".into(),
                "0 0.3 0.45 0.6 0.72 0.88 1".into(),
            ],
            identity(),
            identity(),
        ]),
    );
    add(
        "prelut outputs of another count",
        csp_3d(&[channel("0.0 0.5 1.0", "0.0 1.0"), identity(), identity()]),
    );
    add(
        "a prelut input that doesn't parse",
        csp_3d(&[channel("0.0 x 1.0", "0.0 0.5 1.0"), identity(), identity()]),
    );
    add(
        "a prelut output that doesn't parse",
        csp_3d(&[
            channel("0.0 0.5 1.0", "0.0 0.5 one"),
            identity(),
            identity(),
        ]),
    );

    // The LUTs' errors.
    for (label, size) in [
        ("a 1D size that doesn't parse", "five"),
        ("a 1D size of 0", "0"),
        ("a negative 1D size", "-5"),
        ("a 1D size past 300000", "300001"),
    ] {
        add(
            label,
            csp(
                "1D",
                &[],
                &[identity(), identity(), identity()],
                &[vec![size.to_owned()], lut1d(5)].concat(),
            ),
        );
    }
    add(
        "a 1D entry of 2 values",
        csp(
            "1D",
            &[],
            &[identity(), identity(), identity()],
            &[
                "3".to_owned(),
                "0 0 0".to_owned(),
                "0.5 0.5".to_owned(),
                "1 1 1".to_owned(),
            ],
        ),
    );
    add(
        "a 1D LUT short of entries",
        csp(
            "1D",
            &[],
            &[identity(), identity(), identity()],
            &[vec!["6".to_owned()], lut1d(5)].concat(),
        ),
    );
    for (label, size) in [
        ("a cube size of 2 values", "2 2"),
        ("a cube size that doesn't parse", "2 x 2"),
        ("unequal cube sizes", "2 2 3"),
        ("a cube size of 0", "0 0 0"),
        ("a cube size of 130", "130 130 130"),
    ] {
        add(
            label,
            csp(
                "3D",
                &[],
                &[identity(), identity(), identity()],
                &[vec![size.to_owned()], cube(2)].concat(),
            ),
        );
    }
    add(
        "a cube short of entries",
        csp(
            "3D",
            &[],
            &[identity(), identity(), identity()],
            &[vec!["2 2 2".to_owned()], cube(2)[..7].to_vec()].concat(),
        ),
    );
    add(
        "a cube entry that doesn't parse",
        csp(
            "3D",
            &[],
            &[identity(), identity(), identity()],
            &[
                vec!["2 2 2".to_owned()],
                cube(2)[..3].to_vec(),
                vec!["0.5 x 0.5".to_owned()],
                cube(2)[4..].to_vec(),
            ]
            .concat(),
        ),
    );
    // The identity test's tolerance, 1e-6 relative to the input (`EqualWithRelError`): 1 + 8
    // ulp is within it, 1 + 9 ulp isn't, on either side of a positive or a negative input.
    for (label, inputs, outputs) in [
        ("1 + 8 ulp", "0 0.5 1", "0 0.5 1.0000009536743164"),
        ("1 + 9 ulp", "0 0.5 1", "0 0.5 1.000001072883606"),
        ("1 - 8 ulp", "0 0.5 1", "0 0.5 0.99999904632568359"),
        ("1 - 9 ulp", "0 0.5 1", "0 0.5 0.99999892711639404"),
        ("-1 - 8 ulp", "-1 0 1", "-1.0000009536743164 0 1"),
        ("-1 - 9 ulp", "-1 0 1", "-1.000001072883606 0 1"),
        ("0 against -0", "0 0.5 1", "-0 0.5 1"),
        ("-0 against 0", "-0 0.5 1", "0 0.5 1"),
    ] {
        add(
            &format!("identity tolerance, {label}"),
            csp_3d(&[channel(inputs, outputs), identity(), identity()]),
        );
    }
    // Signed zeros in a prelut that is used: its range (the min-max op) and its coefficients.
    for (label, inputs, outputs) in [
        ("-0 first input", "-0 0.5 1", "0 0.3 1"),
        ("-0 last input", "-1 -0.5 -0", "0 0.3 1"),
        ("-0 first output", "0 0.5 1", "-0 0.3 1"),
        ("-0 outputs", "0 0.5 1", "-0 -0 1"),
        ("-0 last output", "0 0.5 1", "1 0.3 -0"),
        ("-0 input and output", "-0 0.5 1", "-0 0.3 1"),
        ("-0 2 points", "-0 1", "-0 2"),
    ] {
        add(
            &format!("signed zeros, {label}"),
            csp_3d(&[channel(inputs, outputs), identity(), gamma()]),
        );
    }

    add("the header alone", vec!["CSPLUTV100".into()]);
    add("empty", Vec::new());
    add("white space only", vec!["   ".into(), "".into()]);
    cases.push(case(
        "CRLF",
        text(&csp_3d(&[gamma(), identity(), identity()]), "\r\n").as_bytes(),
    ));
    cases
}

#[test]
fn csp_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "csp") {
            let bytes = std::fs::read(&path).unwrap();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            cases.push(case(&name, &bytes));
            cases.push(case_with(
                &format!("{name} inverse transform"),
                &bytes,
                Interpolation::Linear,
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
    let gamma = channel("0.0 0.2 0.5 1.0", "0.0 0.4 0.7 1.0");
    let files = [
        ("3D", csp_3d(&[identity(), identity(), identity()])),
        (
            "prelut and 3D",
            csp_3d(&[gamma.clone(), identity(), identity()]),
        ),
        (
            "prelut and 1D",
            csp(
                "1D",
                &[],
                &[gamma.clone(), identity(), identity()],
                &[vec!["5".to_owned()], lut1d(5)].concat(),
            ),
        ),
    ];
    let mut cases = Vec::new();
    for (name, lines) in &files {
        let text = text(lines, "\n");
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
