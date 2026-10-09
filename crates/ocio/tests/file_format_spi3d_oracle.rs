// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The spi3d reader against the wheel (`processor_ops`): the processors of file transforms of
//! generated spi3d files and of upstream's, both directions of the processor and of the
//! transform, the LUT by its bits; the errors of the files it refuses (the header, the sizes,
//! the entries: their indices, duplicates, numbers each platform parses its way, the `%d` and
//! `%63s` scans, lines around the 4096-byte buffer, missing entries); and what it logs for the
//! interpolations a 3D LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.spi3d` holding `bytes`, read with `interp`. The set holds the
/// case's label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.spi3d", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.spi3d"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// The entries of a LUT of `size` entries per side, blue fastest: `r g b` and values that
/// differ per entry and channel.
fn entries(size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let s = (size - 1) as f64;
    for r in 0..size {
        for g in 0..size {
            for b in 0..size {
                let (rf, gf, bf) = (r as f64 / s, g as f64 / s, b as f64 / s);
                out.push(format!(
                    "{r} {g} {b} {} {} {}",
                    0.9 * rf + 0.05 * gf,
                    0.8 * gf + 0.1 * bf * bf,
                    0.7 * bf + 0.2 * rf * gf
                ));
            }
        }
    }
    out
}

/// A file of the header lines `header`, then `data`, each line ended by `eol`.
fn spi3d(header: &[&str], data: &[String], eol: &str) -> String {
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

const HEADER: [&str; 3] = ["SPILUT 1.0", "3 3", "2 2 2"];

/// Files the reader takes and refuses, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: &str, text: String| cases.push(case(label, text.as_bytes()));
    let data = entries(2);

    add("in order", spi3d(&HEADER, &data, "\n"));
    add("CRLF", spi3d(&HEADER, &data, "\r\n"));
    add("CR", spi3d(&HEADER, &data, "\r"));
    let mut reversed = data.clone();
    reversed.reverse();
    add("reversed", spi3d(&HEADER, &reversed, "\n"));
    let mut shuffled = data.clone();
    shuffled.swap(0, 5);
    shuffled.swap(2, 7);
    add("shuffled", spi3d(&HEADER, &shuffled, "\n"));
    let mut noisy = Vec::new();
    for (i, line) in data.iter().enumerate() {
        noisy.push(line.clone());
        noisy.push(
            [
                "",
                "# a comment",
                "0 0 0",
                "1 1 1 0.5 0.5",
                "x 0 0 1 1 1",
                "  ",
                "0 0",
                "1 2",
            ][i % 8]
                .to_owned(),
        );
    }
    add(
        "lines that don't scan in between",
        spi3d(&HEADER, &noisy, "\n"),
    );
    let mut trailing = data.clone();
    trailing.push("0 0 0 9 9 9".into());
    trailing.push("anything".into());
    add(
        "anything after the last entry",
        spi3d(&HEADER, &trailing, "\n"),
    );
    let mut no_eol = spi3d(&HEADER, &data, "\n");
    no_eol.pop();
    add("no line feed at the end", no_eol);
    add(
        "3 entries per side",
        spi3d(&["SPILUT 1.0", "3 3", "3 3 3"], &entries(3), "\n"),
    );
    add(
        "17 entries per side",
        spi3d(&["SPILUT 1.0", "3 3", "17 17 17"], &entries(17), "\n"),
    );

    // The header.
    for (label, first) in [
        ("lower case", "spilut 1.0"),
        ("mixed case", "SpiLut"),
        ("SPILUTS", "SPILUTS and more"),
        ("SPI LUT", "SPI LUT 1.0"),
        ("leading space", " SPILUT 1.0"),
        ("empty first line", ""),
        ("a 0xE9 first line", "SPILUT \u{e9}"),
    ] {
        add(label, spi3d(&[first, "3 3", "2 2 2"], &data, "\n"));
    }
    let long = format!("SPILUT {}", "x".repeat(5000));
    add(
        "a first line past the buffer",
        spi3d(&[&long, "3 3", "2 2 2"], &data, "\n"),
    );
    let long = format!("SPILUT {}", "x".repeat(4088));
    add(
        "a first line of 4095 bytes",
        spi3d(&[&long, "3 3", "2 2 2"], &data, "\n"),
    );
    let long = format!("SPILUT {}", "x".repeat(4089));
    add(
        "a first line of 4096 bytes",
        spi3d(&[&long, "3 3", "2 2 2"], &data, "\n"),
    );
    add(
        "anything on the second line",
        spi3d(&["SPILUT", "1 2 3 4", "2 2 2"], &data, "\n"),
    );
    for (label, sizes) in [
        ("one size", "42"),
        ("two sizes", "2 2"),
        ("different sizes", "2 3 2"),
        ("a different blue size", "2 2 3"),
        ("a size of 129", "129 129 129"),
        ("a size of 1", "1 1 1"),
        ("a size of 0", "0 0 0"),
        ("negative sizes", "-2 -2 -2"),
        ("a size of 130", "130 130 130"),
        ("a size past int", "4294967298 4294967298 4294967298"),
        ("sizes and more", "2 2 2 2"),
        ("sizes in tabs", "\t2\t2\t2"),
        ("signed sizes", "+2 +2 +2"),
        ("sizes of 2.5", "2.5 2.5 2.5"),
        ("no sizes", ""),
        ("letters", "a b c"),
    ] {
        add(label, spi3d(&["SPILUT 1.0", "3 3", sizes], &data, "\n"));
    }
    add("no sizes line", "SPILUT 1.0\n3 3\n".into());
    add("the header alone", "SPILUT 1.0\n".into());
    add("empty", String::new());

    // The entries.
    let with = |i: usize, line: &str| {
        let mut d = data.clone();
        d[i] = line.to_owned();
        d
    };
    for (label, line) in [
        ("an index out of the cube", "0 2 0 0 0 0"),
        ("a negative index", "0 -1 0 0 0 0"),
        ("a red index out", "2 0 0 0 0 0"),
        ("a blue index out", "0 0 2 0 0 0"),
        ("an index past int", "4294967296 0 0 0 0 0"),
        ("a duplicate", "1 1 1 0.5 0.5 0.5"),
        ("a value that doesn't parse", "0 0 0 abc 0 0"),
        ("a green value that doesn't parse", "0 0 0 0 abc 0"),
        ("a blue value that doesn't parse", "0 0 0 0 0 abc"),
        ("a value with a suffix", "0 0 0 0.5x 0 0"),
        ("a NaN", "0 0 0 nan 0 0"),
        ("a NaN in capitals", "0 0 0 NAN 0 0"),
        ("an infinity", "0 0 0 inf -inf 0"),
        ("infinity", "0 0 0 infinity 0 0"),
        ("a value past float", "0 0 0 1e39 0 0"),
        ("a value below float", "0 0 0 1e-50 0 0"),
        ("a hex float", "0 0 0 0x1p-2 0 0"),
        ("a plus sign", "0 0 0 +0.5 0 0"),
        ("a comma", "0 0 0 0,5 0 0"),
        (
            "a 64-character value",
            &format!("0 0 0 0.{} 0 0", "1".repeat(62)),
        ),
        (
            "a 63-character value",
            &format!("0 0 0 0.{} 0 0", "1".repeat(61)),
        ),
        (
            "a value split by %63s",
            &format!("0 0 0 0.{} 0", "5".repeat(70)),
        ),
        ("extra values", "0 0 0 0.1 0.2 0.3 0.4"),
        ("tabs", "0\t0\t0\t0.1\t0.2\t0.3"),
        ("a NUL", "0 0 0 0.1 0.2\0 0.3"),
        ("an index of +0", "+0 0 0 0.1 0.2 0.3"),
    ] {
        add(label, spi3d(&HEADER, &with(0, line), "\n"));
    }
    let mut missing = data.clone();
    missing.remove(3);
    add("an entry missing", spi3d(&HEADER, &missing, "\n"));
    let mut long_line = data.clone();
    long_line.insert(2, format!("0 0 0 {}", "1".repeat(5000)));
    add(
        "a data line past the buffer",
        spi3d(&HEADER, &long_line, "\n"),
    );
    let mut long_line = data.clone();
    long_line[1] = format!("{}{}", data[1], " ".repeat(4095 - data[1].len()));
    add(
        "a data line of 4095 bytes",
        spi3d(&HEADER, &long_line, "\n"),
    );
    let mut long_line = data.clone();
    long_line[1] = format!("{}{}", data[1], " ".repeat(4096 - data[1].len()));
    add(
        "a data line of 4096 bytes",
        spi3d(&HEADER, &long_line, "\n"),
    );
    cases
}

#[test]
fn spi3d_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "spi3d") {
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

/// The file transform's interpolation: kept when a 3D LUT takes it, else a warning, and the
/// default.
#[test]
fn interpolations_read_and_log_as_in_the_wheel() {
    let text = spi3d(&HEADER, &entries(2), "\n");
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
