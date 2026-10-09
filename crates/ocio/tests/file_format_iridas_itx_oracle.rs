// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Iridas `.itx` reader against the wheel (`processor_ops`): the processors of file
//! transforms of generated files and of upstream's, both directions of the processor and of
//! the transform, the LUT by its bits; the errors of the files it refuses (the size tag, the
//! entries, the line numbers the messages give); and what it logs for the interpolations a 3D
//! LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.itx` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.itx", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.itx"), interp, dir)
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
fn entries(size: usize) -> Vec<String> {
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

/// A file of `header` lines, then `data`, each line ended by `eol`.
fn itx(header: &[&str], data: &[String], eol: &str) -> String {
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
    let d = entries(2);

    add("3D", itx(&["LUT_3D_SIZE 2"], &d, "\n"));
    add("CRLF", itx(&["LUT_3D_SIZE 2"], &d, "\r\n"));
    add("CR", itx(&["LUT_3D_SIZE 2"], &d, "\r"));
    add("5 per side", itx(&["LUT_3D_SIZE 5"], &entries(5), "\n"));
    add("lower case", itx(&["lut_3d_size 2"], &d, "\n"));
    add(
        "lines before the size",
        itx(
            &["anything", "LUT_1D_SIZE 4", "1 2 3", "LUT_3D_SIZE 2"],
            &d,
            "\n",
        ),
    );
    let mut comments = d.clone();
    comments.insert(2, "# a comment".into());
    comments.insert(4, "".into());
    comments.insert(5, "   ".into());
    add(
        "comments and blank lines",
        itx(&["# x", "LUT_3D_SIZE 2"], &comments, "\n"),
    );
    let mut indented = d.clone();
    indented.insert(2, "   # indented".into());
    add(
        "an indented comment",
        itx(&["LUT_3D_SIZE 2"], &indented, "\n"),
    );
    let tabs: Vec<String> = d
        .iter()
        .map(|l| format!("\t{}\t", l.replace(' ', "\t")))
        .collect();
    add("tabs", itx(&["  LUT_3D_SIZE\t2  "], &tabs, "\n"));
    let mut no_eol = itx(&["LUT_3D_SIZE 2"], &d, "\n");
    no_eol.pop();
    add("no line feed at the end", no_eol);
    let mut resized = d.clone();
    resized.insert(3, "LUT_3D_SIZE 2".into());
    add(
        "a second size among the entries",
        itx(&["LUT_3D_SIZE 2"], &resized, "\n"),
    );
    add(
        "a size of 3 then of 2",
        itx(&["LUT_3D_SIZE 3", "LUT_3D_SIZE 2"], &d, "\n"),
    );

    for (label, size) in [
        ("no size", ""),
        ("a size of two values", "LUT_3D_SIZE 2 2"),
        ("a size alone", "LUT_3D_SIZE"),
        ("a size that doesn't parse", "LUT_3D_SIZE two"),
        ("a size with a suffix", "LUT_3D_SIZE 2x"),
        ("a signed size", "LUT_3D_SIZE +2"),
        ("a size of 1", "LUT_3D_SIZE 1"),
        ("a size of 129", "LUT_3D_SIZE 129"),
        ("a size of 130", "LUT_3D_SIZE 130"),
        ("a negative size", "LUT_3D_SIZE -2"),
        ("a size past int", "LUT_3D_SIZE 4294967298"),
    ] {
        add(label, itx(&[size], &d, "\n"));
    }
    // A comment among the entries, which the Resolve cube reader refuses: these files are
    // read by the `.itx` reader alone, never by the fallback to every format.
    let mut commented = d.clone();
    commented.insert(1, "# only .itx files take this".into());
    add(
        "a size with a suffix, read by the .itx reader alone",
        itx(&["LUT_3D_SIZE 2x"], &commented, "\n"),
    );
    let mut upper = commented.clone();
    upper[0] = "0 1E-1 INF".into();
    upper[2] = "NAN 0X1P-2 -INFINITY".into();
    add("upper-case values", itx(&["LUT_3D_SIZE 2"], &upper, "\n"));
    let with = |i: usize, line: &str| {
        let mut e = d.clone();
        e[i] = line.to_owned();
        e
    };
    for (label, line) in [
        ("an entry of 2 values", "0 0"),
        ("an entry of 4 values", "0 0 0 0"),
        ("an entry that doesn't parse", "0 x 0"),
        ("a blue value that doesn't parse", "0 0 x"),
        ("a NaN entry", "nan 0 0"),
        ("an infinite entry", "inf -inf 0"),
        ("an entry past float", "1e39 0 0"),
        ("a hex entry", "0x1p-2 0 0"),
        ("an entry with a NUL", "0 0\0 0"),
        ("a tag among the entries", "WRONG_TAG"),
    ] {
        add(label, itx(&["LUT_3D_SIZE 2"], &with(1, line), "\n"));
    }
    let mut extra = d.clone();
    extra.push("1 1 1".into());
    add("an entry too many", itx(&["LUT_3D_SIZE 2"], &extra, "\n"));
    add("an entry missing", itx(&["LUT_3D_SIZE 2"], &d[..7], "\n"));
    add("a size and no entries", "LUT_3D_SIZE 2\n".into());
    add("empty", String::new());
    let mut numbered = d.clone();
    numbered.insert(3, "".into());
    numbered.insert(4, "# c".into());
    numbered[5] = "0 x 0".into();
    add(
        "the line number after blank lines and comments",
        itx(&["# a", "", "LUT_3D_SIZE 2", "   "], &numbered, "\n"),
    );
    cases
}

#[test]
fn itx_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "itx") {
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
    let text = itx(&["LUT_3D_SIZE 2"], &entries(2), "\n");
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
