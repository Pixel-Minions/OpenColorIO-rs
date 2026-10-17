// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Houdini LUT reader against the wheel (`processor_ops`): the processors of file
//! transforms of generated `.lut` files and of upstream's (a `.lut` file is read as a
//! Discreet 1D LUT first, then as a Houdini LUT), both directions of the processor and of the
//! transform, the LUTs and ranges by their bits; the errors of the files it refuses (each
//! header line, the types, the sizes, the LUT blocks and their words); and what it logs for
//! the interpolations a LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.lut` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.lut", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.lut"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// The header lines of a LUT of `kind` with the sizes `length`.
fn header(kind: &str, length: &str) -> Vec<String> {
    vec![
        "Version\t\t3".into(),
        "Format\t\tany".into(),
        format!("Type\t\t{kind}"),
        "From\t\t-0.25 1.5".into(),
        "To\t\t0 1".into(),
        "Black\t\t0".into(),
        "White\t\t1".into(),
        format!("Length\t\t{length}"),
        "LUT:".into(),
    ]
}

/// The values of a 3D LUT of `size` entries per side, red fastest, a line each.
fn cube_values(size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let s = (size - 1) as f64;
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let (rf, gf, bf) = (r as f64 / s, g as f64 / s, b as f64 / s);
                out.push(format!(
                    "\t{} {} {}",
                    0.9 * rf + 0.05 * gf,
                    0.8 * gf + 0.1 * bf * bf,
                    0.7 * bf + 0.2 * rf * gf
                ));
            }
        }
    }
    out
}

/// The values of a 1D LUT of `size` entries, a line each.
fn values_1d(size: usize) -> Vec<String> {
    (0..size)
        .map(|i| format!("\t{}", (i as f64 / (size - 1) as f64).powf(0.45)))
        .collect()
}

/// A block `name {` (a lone `{` for no name), `values`, `}`.
fn block(name: &str, values: &[String]) -> Vec<String> {
    let open = if name.is_empty() {
        "{".to_owned()
    } else {
        format!("{name} {{")
    };
    [vec![open], values.to_vec(), vec!["}".into()]].concat()
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

fn lut_1d() -> Vec<String> {
    [header("C", "9"), block("RGB", &values_1d(9))].concat()
}

fn lut_3d() -> Vec<String> {
    [header("3D", "2"), block("", &cube_values(2))].concat()
}

fn lut_3d_1d() -> Vec<String> {
    [
        header("3D+1D", "2 10"),
        block("Pre", &values_1d(10)),
        block("3D", &cube_values(2)),
    ]
    .concat()
}

/// `lines` with the line starting with `prefix` replaced by `with` (or removed when `with`
/// is `None`).
fn replace(lines: &[String], prefix: &str, with: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        if line.starts_with(prefix) {
            if let Some(with) = with {
                out.push(with.to_owned());
            }
        } else {
            out.push(line.clone());
        }
    }
    out
}

/// Files the reader takes and refuses, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: &str, text: String| cases.push(case(label, text.as_bytes()));

    add("1D", text(&lut_1d(), "\n"));
    add("3D", text(&lut_3d(), "\n"));
    add("3D+1D", text(&lut_3d_1d(), "\n"));
    add("3D+1D, CRLF", text(&lut_3d_1d(), "\r\n"));
    add("3D+1D, CR", text(&lut_3d_1d(), "\r"));
    add(
        "3D of 5",
        text(
            &[header("3D", "5"), block("", &cube_values(5))].concat(),
            "\n",
        ),
    );
    add(
        "3D, a named block",
        text(
            &[header("3D", "2"), block("3d", &cube_values(2))].concat(),
            "\n",
        ),
    );
    add(
        "3D, the values on one line",
        text(
            &[
                header("3D", "2"),
                vec![format!("{{ {} }}", cube_values(2).join(" "))],
            ]
            .concat(),
            "\n",
        ),
    );
    add(
        "1D, the block closed on its last value's line",
        text(&replace(&lut_1d(), "}", Some(&format!("{} }}", "1"))), "\n"),
    );
    add(
        "header keys in other cases, blank lines",
        text(
            &[
                vec![
                    "".into(),
                    "VERSION 3".into(),
                    "  format ANY  ".into(),
                    "".into(),
                ],
                header("c", "9")[2..].to_vec(),
                block("rgb", &values_1d(9)),
            ]
            .concat(),
            "\n",
        ),
    );
    add(
        "extra header lines and a key twice",
        text(
            &[
                vec!["Sampling linear".into(), "Version 1 2".into()],
                lut_1d(),
            ]
            .concat(),
            "\n",
        ),
    );
    add(
        "extra blocks",
        text(
            &[
                lut_3d_1d(),
                block("Extra", &values_1d(3)),
                block("RGB", &values_1d(2)),
            ]
            .concat(),
            "\n",
        ),
    );
    add(
        "white space of every kind",
        text(&lut_3d(), "\n").replace(' ', "\x0b \x0c"),
    );

    // The header.
    for key in [
        "Version", "Format", "Type", "From", "To", "Black", "White", "Length",
    ] {
        add(
            &format!("no {key}"),
            text(&replace(&lut_1d(), key, None), "\n"),
        );
    }
    for (label, prefix, line) in [
        ("two versions", "Version", "Version 1 2"),
        ("an empty Version", "Version", "Version"),
        ("one From value", "From", "From 0"),
        ("three To values", "To", "To 0 1 2"),
        ("a From that doesn't parse", "From", "From x 1"),
        ("a second From value that doesn't parse", "From", "From 0 y"),
        ("a To that doesn't parse", "To", "To 0 nan1"),
        ("a Black that doesn't parse", "Black", "Black zero"),
        ("a White that doesn't parse", "White", "White 1x"),
        ("a NaN White", "White", "White nan"),
        ("an infinite From", "From", "From -inf inf"),
        ("an unknown type", "Type", "Type 2D"),
        ("type RGB", "Type", "Type RGB"),
        ("a Length that doesn't parse", "Length", "Length nine"),
        ("a Length with a suffix", "Length", "Length 9x"),
        ("three Length values", "Length", "Length 9 9 9"),
        ("a 1D Length of 1", "Length", "Length 1"),
        ("a 1D Length past 300000", "Length", "Length 300001"),
        ("a second Length that doesn't parse", "Length", "Length 9 x"),
    ] {
        add(label, text(&replace(&lut_1d(), prefix, Some(line)), "\n"));
    }
    for (label, length) in [
        ("a cube size of 1", "1"),
        ("a cube size of 130", "130"),
        ("a cube size of 129", "129"),
        ("a negative cube size", "-2"),
    ] {
        add(
            label,
            text(
                &replace(&lut_3d(), "Length", Some(&format!("Length {length}"))),
                "\n",
            ),
        );
    }
    for (label, length) in [
        ("a prelut size of 1", "2 1"),
        ("a prelut size past 300000", "2 300001"),
        ("a prelut size of 11", "2 11"),
        ("a prelut size of 9", "2 9"),
    ] {
        add(
            label,
            text(
                &replace(&lut_3d_1d(), "Length", Some(&format!("Length {length}"))),
                "\n",
            ),
        );
    }
    add(
        "no LUT: line",
        text(&replace(&lut_1d(), "LUT:", None), "\n"),
    );
    add("the header alone", text(&header("C", "9"), "\n"));

    // The blocks.
    add(
        "no RGB block",
        text(
            &[header("C", "9"), block("Pre", &values_1d(9))].concat(),
            "\n",
        ),
    );
    add(
        "no Pre block",
        text(
            &[header("3D+1D", "2 10"), block("", &cube_values(2))].concat(),
            "\n",
        ),
    );
    add(
        "no 3D block",
        text(
            &[header("3D+1D", "2 10"), block("Pre", &values_1d(10))].concat(),
            "\n",
        ),
    );
    add(
        "a 3D block too short",
        text(
            &[header("3D", "2"), block("", &cube_values(2)[..7])].concat(),
            "\n",
        ),
    );
    add(
        "a 3D block too long",
        text(
            &[
                header("3D", "2"),
                block("", &[cube_values(2), vec!["0".into()]].concat()),
            ]
            .concat(),
            "\n",
        ),
    );
    add(
        "an RGB block too short",
        text(
            &[header("C", "9"), block("RGB", &values_1d(8))].concat(),
            "\n",
        ),
    );
    add(
        "a name not followed by {",
        text(
            &[header("C", "9"), vec!["RGB [".into()], values_1d(9)].concat(),
            "\n",
        ),
    );
    add(
        "a name at the end",
        text(&[header("C", "9"), vec!["RGB".into()]].concat(), "\n"),
    );
    add(
        "a name glued to {",
        text(
            &[header("C", "9"), vec!["RGB{".into()], values_1d(9)].concat(),
            "\n",
        ),
    );
    add(
        "a } outside a block",
        text(
            &[
                header("C", "9"),
                vec!["}".into()],
                block("RGB", &values_1d(9)),
            ]
            .concat(),
            "\n",
        ),
    );
    add(
        "a block never closed",
        text(
            &[header("C", "9"), vec!["RGB {".into()], values_1d(9)].concat(),
            "\n",
        ),
    );
    for (label, value) in [
        ("a value that doesn't parse", "abc"),
        ("a value with a suffix", "0.5x"),
        ("a NaN value", "nan"),
        ("an infinite value", "-inf"),
        ("a value past float", "1e39"),
        ("a hex value", "0x1p-2"),
        ("a comma", "0,5"),
    ] {
        let mut values = values_1d(9);
        values[3] = format!("\t{value}");
        add(
            label,
            text(&[header("C", "9"), block("RGB", &values)].concat(), "\n"),
        );
    }
    add(
        "bytes past ASCII between words",
        text(&lut_1d(), "\n").replace("\t0.", "\u{a0}0."),
    );
    // Values separated by a vertical tab or a form feed only.
    let packed = values_1d(9)
        .iter()
        .map(|v| v.trim().to_owned())
        .collect::<Vec<_>>()
        .join("\x0b")
        .replacen('\x0b', "\x0c", 3);
    add(
        "values separated by vertical tabs and form feeds",
        text(&[header("C", "9"), block("RGB", &[packed])].concat(), "\n"),
    );
    // From ranges the min-max op refuses (equal bounds) and others it takes, on 1D and 3D+1D
    // LUTs.
    for (label, from) in [
        ("equal bounds", "1 1"),
        ("a NaN", "nan 1"),
        ("NaNs", "-nan nan"),
        ("infinities", "-inf inf"),
        ("a reversed range", "2 1"),
    ] {
        let line = format!("From\t\t{from}");
        add(
            &format!("a 1D LUT from {label}"),
            text(&replace(&lut_1d(), "From", Some(&line)), "\n"),
        );
        add(
            &format!("a 3D+1D LUT from {label}"),
            text(&replace(&lut_3d_1d(), "From", Some(&line)), "\n"),
        );
    }
    // A block's opening brace on the `LUT:` line, which the header reader consumes.
    let mut lut_line = replace(&header("3D", "2"), "LUT:", Some("LUT: {"));
    lut_line.extend(cube_values(2));
    lut_line.push("}".into());
    add(
        "the 3D block's brace on the LUT: line",
        text(&lut_line, "\n"),
    );
    let mut lut_line = replace(&header("C", "9"), "LUT:", Some("LUT: RGB {"));
    lut_line.extend(values_1d(9));
    lut_line.push("}".into());
    add(
        "the RGB block's name on the LUT: line",
        text(&lut_line, "\n"),
    );
    add("empty", String::new());
    cases
}

#[test]
fn hdl_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "lut") {
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
    let files = [
        ("1D", text(&lut_1d(), "\n")),
        ("3D", text(&lut_3d(), "\n")),
        ("3D+1D", text(&lut_3d_1d(), "\n")),
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
