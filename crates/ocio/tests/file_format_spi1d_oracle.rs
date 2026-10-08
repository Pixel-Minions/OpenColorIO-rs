// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The spi1d reader against the wheel (`processor_ops`): the processors of file transforms of
//! generated spi1d files and of upstream's, both directions of the processor and of the
//! transform, every value of the range and the LUT by its bits; the errors of the files it
//! refuses (each header tag, the counts, the components, numbers each platform parses its way,
//! lines around its 4096-byte buffer, the `%d` and `%63s` scans); and what it logs for the
//! interpolations a 1D LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.spi1d` holding `bytes`, read with `interp`. The set holds the
/// case's label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.spi1d", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.spi1d"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// A file of `header` lines, then `{`, the `data` lines and `}`, each line ended by `eol`.
fn spi1d(header: &[&str], data: &[&str], eol: &str) -> String {
    let mut text = String::new();
    for line in header.iter().chain(&["{"]).chain(data).chain(&["}"]) {
        text.push_str(line);
        text.push_str(eol);
    }
    text
}

const HEADER: [&str; 4] = ["Version 1", "From 0.0 1.0", "Length 4", "Components 1"];
const DATA1: [&str; 4] = ["0.0", "0.25", "0.5", "1.0"];

fn header_with(tag: &str, line: &str) -> Vec<String> {
    HEADER
        .iter()
        .map(|h| {
            if h.starts_with(tag) {
                line.to_owned()
            } else {
                (*h).to_owned()
            }
        })
        .collect()
}

fn refs(lines: &[String]) -> Vec<&str> {
    lines.iter().map(String::as_str).collect()
}

/// Files the reader takes and refuses, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: String, text: String| cases.push(case(&label, text.as_bytes()));

    add("one component".into(), spi1d(&HEADER, &DATA1, "\n"));
    add(
        "two components".into(),
        spi1d(
            &refs(&header_with("Components", "Components 2")),
            &["0 0.5", "0.25 0.5", "0.5 0.5", "1 0.5"],
            "\n",
        ),
    );
    add(
        "three components".into(),
        spi1d(
            &refs(&header_with("Components", "Components 3")),
            &["0 0.5 1", "0.25 0.5 1", "0.5 0.5 1", "1 0.5 1"],
            "\n",
        ),
    );
    for (tag, line) in [
        ("From", "From -0.125 4.5"),
        ("From", "From 1 1"),
        ("From", "From 2 1"),
        ("From", "From 1e-3 1e3"),
        ("From", "From 0x10 inf"),
        ("From", "From nan 1"),
        ("From", "From 1e40 2"),
        ("From", "From 0"),
        ("From", "From"),
        ("From", "From a b"),
        ("From", "From 0.0 1.0 2.0"),
        ("Version", "Version1"),
        ("Version", "Version\t1"),
        ("Version", "Version 1 2"),
        ("Version", "Version 2"),
        ("Version", "Version -1"),
        ("Version", "Version A"),
        ("Version", "Version"),
        ("Version", "Version 4294967297"),
        ("Version", "VERSION 1"),
        ("Length", "Length 1"),
        ("Length", "Length 2"),
        ("Length", "Length -4"),
        ("Length", "Length 300001"),
        ("Length", "Length 4294967300"),
        ("Length", "Length +4"),
        ("Length", "Length 4.5"),
        ("Length", "Length"),
        ("Length", "Length-"),
        ("Components", "Components 0"),
        ("Components", "Components 4"),
        ("Components", "Components -1"),
        ("Components", "Components x"),
        ("Components", "Components 4294967297"),
    ] {
        add(
            format!("header {line:?}"),
            spi1d(&refs(&header_with(tag, line)), &DATA1, "\n"),
        );
    }
    // Missing tags, tags in another order, other lines.
    for skipped in HEADER {
        let header: Vec<&str> = HEADER.iter().filter(|h| **h != skipped).copied().collect();
        add(format!("without {skipped}"), spi1d(&header, &DATA1, "\n"));
    }
    add(
        "tags reversed, comments".into(),
        spi1d(
            &[
                "# comment",
                "Components 1",
                "Length 4",
                "",
                "From 0 1",
                "Version 1",
            ],
            &DATA1,
            "\n",
        ),
    );
    // Line ends, white space, the closing brace.
    add("CR LF".into(), spi1d(&HEADER, &DATA1, "\r\n"));
    add("lone CR".into(), spi1d(&HEADER, &DATA1, "\r"));
    add(
        "blank and indented lines".into(),
        spi1d(
            &HEADER,
            &["  0.0", "", "\t0.25 ", "   ", "0.5\t", "1.0"],
            "\n",
        ),
    );
    add(
        "brace with spaces".into(),
        spi1d(&HEADER, &DATA1, "\n").replace("}\n", " } \n"),
    );
    add(
        "no closing brace".into(),
        spi1d(&HEADER, &DATA1, "\n").replace("}\n", ""),
    );
    add(
        "no line end at the end".into(),
        spi1d(&HEADER, &DATA1, "\n").trim_end().to_owned(),
    );
    add(
        "brace on the header line".into(),
        format!(
            "{}{{ 0\n0.25\n0.5\n1\n}}\n",
            HEADER.map(|h| format!("{h}\n")).concat()
        ),
    );
    add(
        "data after the brace".into(),
        format!("{}0.5\n", spi1d(&HEADER, &DATA1, "\n")),
    );
    // Counts and components.
    add("too few entries".into(), spi1d(&HEADER, &DATA1[..3], "\n"));
    add(
        "too many entries".into(),
        spi1d(&HEADER, &["0", "0.25", "0.5", "1", "1"], "\n"),
    );
    add(
        "two values for one component".into(),
        spi1d(&HEADER, &["0", "0.25 1", "0.5", "1"], "\n"),
    );
    add(
        "four values for three components".into(),
        spi1d(
            &refs(&header_with("Components", "Components 3")),
            &["0 0 0", "1 1 1 1", "0.5 0.5 0.5", "1 1 1"],
            "\n",
        ),
    );
    add(
        "no component".into(),
        spi1d(&refs(&header_with("Components", "Components 0")), &[], "\n"),
    );
    // Numbers: forms each platform parses its own way, and a word.
    for value in [
        "1e-3",
        "-0",
        "+.5",
        "1.",
        "inf",
        "-inf",
        "nan",
        "-nan",
        "infinity",
        "0x1.8p1",
        "0x10",
        "1e40",
        "1e-46",
        "1.5f",
        "1,5",
        "abc",
        "3.4028235e38",
        "1.17549435e-38",
        "0.000000000000000000000000000000000000000000001",
    ] {
        add(
            format!("value {value:?}"),
            spi1d(&HEADER, &["0", value, "0.5", "1"], "\n"),
        );
    }
    // A word of 64 bytes or more: %63s splits it.
    let long_number = format!("0.{}", "1".repeat(70));
    add(
        "a 72-byte number".into(),
        spi1d(&HEADER, &["0", &long_number, "0.5", "1"], "\n"),
    );
    add(
        "a 72-byte number, two components".into(),
        spi1d(
            &refs(&header_with("Components", "Components 2")),
            &["0 0", &long_number, "0.5 0.5", "1 1"],
            "\n",
        ),
    );
    // Lines around the 4096-byte buffer, in the header and in the data.
    for len in [4094, 4095, 4096, 4097] {
        let comment = format!("#{}", "x".repeat(len - 1));
        add(
            format!("a {len}-byte header line"),
            spi1d(
                &[
                    "Version 1",
                    &comment,
                    "From 0 1",
                    "Length 4",
                    "Components 1",
                ],
                &DATA1,
                "\n",
            ),
        );
        let data = format!("0.25{}", " ".repeat(len - 4));
        add(
            format!("a {len}-byte data line"),
            spi1d(&HEADER, &["0", &data, "0.5", "1"], "\n"),
        );
    }
    // Bytes text mode reads its own way, and NUL.
    add(
        "0x1A in the data".into(),
        spi1d(&HEADER, &["0", "0.25", "0.5\x1a", "1"], "\n"),
    );
    add(
        "0x1A after the brace".into(),
        format!("{}\x1agarbage\n", spi1d(&HEADER, &DATA1, "\n")),
    );
    add(
        "NUL in a header line".into(),
        spi1d(
            &refs(&header_with("Length", "Length 4\0 extra")),
            &DATA1,
            "\n",
        ),
    );
    add(
        "NUL in a data line".into(),
        spi1d(&HEADER, &["0", "0.25\0 9", "0.5", "1"], "\n"),
    );
    add("empty".into(), String::new());
    cases
}

#[test]
fn spi1d_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // A LUT of the largest length.
    let mut data = vec![String::new(); 300_000];
    for (i, line) in data.iter_mut().enumerate() {
        *line = format!("{}", i as f64 / 299_999.0);
    }
    let header = header_with("Length", "Length 300000");
    cases.push(case(
        "300000 entries",
        spi1d(&refs(&header), &refs(&data), "\n").as_bytes(),
    ));

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "spi1d") {
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

/// The file transform's interpolation: kept when a 1D LUT takes it, else a warning, and the
/// default.
#[test]
fn interpolations_read_and_log_as_in_the_wheel() {
    let text = spi1d(&HEADER, &DATA1, "\n");
    let cases: Vec<Case> = [
        Interpolation::Unknown,
        Interpolation::Nearest,
        Interpolation::Linear,
        Interpolation::Tetrahedral,
        Interpolation::Cubic,
        Interpolation::Default,
        Interpolation::Best,
    ]
    .into_iter()
    .map(|interp| {
        case_with(
            &format!("{interp:?}"),
            text.as_bytes(),
            interp,
            TransformDirection::Forward,
        )
    })
    .collect();
    {
        let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
        check_file_processors(&cases);
    }
    common::lut_files::check_logs(&cases, TransformDirection::Forward);
    common::lut_files::check_logs(&cases, TransformDirection::Inverse);
}
