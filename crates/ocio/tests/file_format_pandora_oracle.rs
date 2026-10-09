// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Pandora reader against the wheel (`processor_ops`): the processors of file transforms
//! of generated `.mga` and `.m3d` files and of upstream's, both directions of the processor
//! and of the transform, the LUT by its bits and its bit depth; the errors of the files it
//! refuses (each tag, the sizes, the entries, the line numbers the messages give); and what it
//! logs for the interpolations a 3D LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.<ext>` holding `bytes`, read with `interp`. The set holds the
/// case's label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(
    label: &str,
    ext: &str,
    bytes: &[u8],
    interp: Interpolation,
    dir: TransformDirection,
) -> Case {
    let name = format!("lut.{ext}");
    let dir_path = write_files(&[file(&name, bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/{name}"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        "m3d",
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// The entry lines of a LUT of `size` entries per side, blue fastest, values up to `max`.
fn entries(size: usize, max: i64) -> Vec<String> {
    let mut out = Vec::new();
    let s = (size - 1) as i64;
    let mut i = 0;
    for r in 0..size as i64 {
        for g in 0..size as i64 {
            for b in 0..size as i64 {
                out.push(format!(
                    "{i} {} {} {}",
                    max * (9 * r + g) / (10 * s),
                    max * (8 * g + b) / (9 * s),
                    max * b / s
                ));
                i += 1;
            }
        }
    }
    out
}

/// The header lines of a LUT of `size^3` entries with an `out` of `out`.
fn header(size: usize, out: i64) -> Vec<String> {
    vec![
        "channel 3d".into(),
        format!("in {}", size * size * size),
        format!("out {out}"),
        "format lut".into(),
        "values red green blue".into(),
    ]
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

/// `lines` with the line starting with `prefix` replaced by `with` (removed for `None`).
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

fn basic() -> Vec<String> {
    [header(2, 256), entries(2, 255)].concat()
}

/// Files the reader takes and refuses, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: &str, lines: Vec<String>| {
        cases.push(case(label, text(&lines, "\n").as_bytes()));
    };

    add("2 per side", basic());
    add("5 per side", [header(5, 4096), entries(5, 4095)].concat());
    for out in [
        1i64,
        2,
        127,
        128,
        638,
        639,
        2558,
        2559,
        34814,
        34815,
        65536,
        2_000_000_000,
    ] {
        add(
            &format!("out {out}"),
            [header(2, out), entries(2, 255)].concat(),
        );
    }
    add("tags in other cases", {
        let mut l = basic();
        l[0] = "CHANNEL 3D".into();
        l[3] = "Format LUT".into();
        l[4] = "Values Red Green Blue".into();
        l
    });
    add("comments and blank lines", {
        let mut l = basic();
        l.insert(0, "# a comment".into());
        l.insert(3, "".into());
        l.insert(6, "  # indented".into());
        l
    });
    add("no channel tag", replace(&basic(), "channel", None));
    add("no format tag", replace(&basic(), "format", None));
    add("tags after the values tag", {
        let mut l = basic();
        l.insert(7, "channel 3d".into());
        l.insert(8, "out 1024".into());
        l
    });
    add("the out tag twice", {
        let mut l = basic();
        l.insert(3, "out 4096".into());
        l
    });
    add("indices out of order", {
        let mut l = basic();
        l.swap(5, 9);
        l
    });
    add("negative values", {
        let mut l = basic();
        l[6] = "1 -5 0 300".into();
        l
    });

    for (label, prefix, line) in [
        ("channel 2d", "channel", "channel 2d"),
        ("channel without a value", "channel", "channel"),
        ("channel of 2 values", "channel", "channel 3d 3d"),
        ("an in tag that doesn't parse", "in", "in eight"),
        ("an in tag with a suffix", "in", "in 8x"),
        ("an in of 7", "in", "in 7"),
        ("an in of 9", "in", "in 9"),
        ("an in past 129^3", "in", "in 2146690"),
        ("an in of 129^3", "in", "in 2146689"),
        ("an in of 27", "in", "in 27"),
        ("an out tag that doesn't parse", "out", "out x"),
        ("an out of 0", "out", "out 0"),
        ("a negative out", "out", "out -256"),
        ("no out tag", "out", "# out 256"),
        ("format 3d", "format", "format 3d"),
        ("values in another order", "values", "values blue green red"),
        ("values of 2 channels", "values", "values red green"),
        ("values red green alpha", "values", "values red green alpha"),
        ("values red alpha blue", "values", "values red alpha blue"),
        (
            "values alpha green blue",
            "values",
            "values alpha green blue",
        ),
    ] {
        add(label, replace(&basic(), prefix, Some(line)));
    }
    add("no in tag", replace(&basic(), "in ", None));
    add("no values tag", replace(&basic(), "values", None));
    let with = |i: usize, line: &str| {
        let mut l = basic();
        l[5 + i] = line.to_owned();
        l
    };
    for (label, line) in [
        ("an entry of 3 integers", "1 0 0"),
        ("an entry of 5 integers", "1 0 0 0 0"),
        ("an entry with a word", "1 0 WRONG 0"),
        ("an entry with a float", "1 0 0.5 0"),
        ("an integer past int", "1 0 4294967296 0"),
    ] {
        add(label, with(1, line));
    }
    add(
        "an entry too many",
        [basic(), vec!["8 255 255 255".into()]].concat(),
    );
    add("an entry missing", basic()[..12].to_vec());
    add("the tags alone", header(2, 256));
    add("empty", Vec::new());
    add("the line number after blank lines and comments", {
        let mut l = basic();
        l.insert(0, "".into());
        l.insert(1, "# c".into());
        l[9] = "1 0 x 0".into();
        l
    });
    cases.push(case_with(
        ".mga",
        "mga",
        text(&basic(), "\n").as_bytes(),
        Interpolation::Default,
        TransformDirection::Forward,
    ));
    cases.push(case("CRLF", text(&basic(), "\r\n").as_bytes()));
    cases.push(case("CR", text(&basic(), "\r").as_bytes()));
    cases
}

#[test]
fn pandora_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        if ext == "m3d" || ext == "mga" {
            let bytes = std::fs::read(&path).unwrap();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            cases.push(case_with(
                &name,
                &ext,
                &bytes,
                Interpolation::Default,
                TransformDirection::Forward,
            ));
            cases.push(case_with(
                &format!("{name} inverse transform"),
                &ext,
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
    let text = text(&basic(), "\n");
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
            "m3d",
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
