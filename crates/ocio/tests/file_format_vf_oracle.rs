// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Nuke `.vf` reader against the wheel (`processor_ops`): the processors of file
//! transforms of generated files and of upstream's, both directions of the processor and of
//! the transform, the LUT and the global matrix by their bits; the errors of the files it
//! refuses (the first line, the tags, the sizes, the entries, the line numbers the messages
//! give); and what it logs for the interpolations a 3D LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.vf` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.vf", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.vf"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// The entry lines of a LUT of `size` entries per side, blue fastest.
fn entries(size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let s = (size - 1) as f64;
    for r in 0..size {
        for g in 0..size {
            for b in 0..size {
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

const IDENTITY: &str = "global_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1";

/// A file of the first line, the grid size, `transform` (left out when empty), `data` and the
/// entries.
fn vf(size: usize, transform: &str) -> Vec<String> {
    let mut out = vec![
        "#Inventor V2.1 ascii".to_owned(),
        format!("grid_size {size} {size} {size}"),
    ];
    if !transform.is_empty() {
        out.push(transform.to_owned());
    }
    out.push("data".into());
    out.extend(entries(size));
    out
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

    add("2 per side", vf(2, IDENTITY));
    add("5 per side", vf(5, IDENTITY));
    add("no global transform", vf(3, ""));
    add(
        "a global transform",
        vf(
            3,
            "global_transform 0.5 0.1 0 0  0 0.25 0 0  0.05 0 0.125 0  0.1 0.2 0.3 1",
        ),
    );
    add(
        "a global transform of every kind of value",
        vf(
            2,
            "global_transform 1e-3 -2 0x1p-3 inf  -0 1 nan 0  0 0 1 0  0 0 0 1",
        ),
    );
    add("upper-case tags", {
        let mut l = vf(2, IDENTITY);
        l[1] = "GRID_SIZE 2 2 2".into();
        l[2] = l[2].to_uppercase();
        l[3] = "DATA".into();
        l
    });
    add("comments and blank lines", {
        let mut l = vf(2, IDENTITY);
        l.insert(1, "# a comment".into());
        l.insert(2, "".into());
        l.insert(6, "  # indented".into());
        l
    });
    add("lines of other lengths among the entries", {
        let mut l = vf(2, IDENTITY);
        l.insert(5, "1 2".into());
        l.insert(6, "1 2 3 4".into());
        l.insert(7, "x y z".into());
        l.insert(8, "grid_size 3 3 3".into());
        l
    });
    add("unknown tags before data", {
        let mut l = vf(2, IDENTITY);
        l.insert(2, "element_size 1 1 1".into());
        l.insert(3, "world_origin 0 0 0".into());
        l.insert(4, "1 2 3".into());
        l
    });
    add("the grid size after the transform", {
        let mut l = vf(2, IDENTITY);
        l.swap(1, 2);
        l
    });
    add("two grid sizes", {
        let mut l = vf(2, IDENTITY);
        l.insert(1, "grid_size 3 3 3".into());
        l
    });
    for (label, first) in [
        ("lower case", "#inventor V2.1 ascii"),
        ("leading space", " #Inventor V2.1 ascii"),
        ("another version", "#Inventor V1.0 binary"),
        ("another format", "#VRML V2.0 utf8"),
    ] {
        let mut l = vf(2, IDENTITY);
        l[0] = first.into();
        add(label, l);
    }
    add("blank lines first", {
        let mut l = vf(2, IDENTITY);
        l.insert(0, "".into());
        l.insert(1, "  ".into());
        l
    });
    for (label, size) in [
        ("a grid size of 2 values", "grid_size 2 2"),
        ("a grid size that doesn't parse", "grid_size 2 x 2"),
        ("a grid size with a suffix", "grid_size 2 2 2x"),
        ("unequal grid sizes", "grid_size 2 2 3"),
        ("a grid size of 1", "grid_size 1 1 1"),
        ("a grid size of 130", "grid_size 130 130 130"),
        ("a grid size of 129", "grid_size 129 129 129"),
        ("a negative grid size", "grid_size -2 -2 -2"),
    ] {
        let mut l = vf(2, IDENTITY);
        l[1] = size.into();
        add(label, l);
    }
    add("no grid size", {
        let mut l = vf(2, IDENTITY);
        l.remove(1);
        l
    });
    for (label, transform) in [
        (
            "a transform of 15 values",
            "global_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0",
        ),
        (
            "a transform of 17 values",
            "global_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1 0",
        ),
        (
            "a transform that doesn't parse",
            "global_transform 1 0 0 0  0 x 0 0  0 0 1 0  0 0 0 1",
        ),
    ] {
        add(label, vf(2, transform));
    }
    add("no data tag", {
        let mut l = vf(2, IDENTITY);
        l.retain(|x| x != "data");
        l
    });
    add("an entry missing", {
        let mut l = vf(2, IDENTITY);
        l.pop();
        l
    });
    add("an entry too many", {
        let mut l = vf(2, IDENTITY);
        l.push("1 1 1".into());
        l
    });
    add("the line number of an error after blank lines", {
        let mut l = vf(2, IDENTITY);
        l.insert(1, "".into());
        l.insert(2, "# c".into());
        l[3] = "grid_size 2 2 3".into();
        l
    });
    add("the first line alone", vec!["#Inventor V2.1 ascii".into()]);
    add("empty", Vec::new());
    cases.push(case("CRLF", text(&vf(2, IDENTITY), "\r\n").as_bytes()));
    cases.push(case("CR", text(&vf(2, IDENTITY), "\r").as_bytes()));
    cases
}

#[test]
fn vf_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "vf") {
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
    let text = text(&vf(2, IDENTITY), "\n");
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
