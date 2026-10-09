// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Truelight `.cub` reader against the wheel (`processor_ops`): the processors of file
//! transforms of generated files and of upstream's, both directions of the processor and of
//! the transform, the LUTs by their bits; the errors of the files it refuses (the first line,
//! the tags, the sizes, the entries); and what it logs for the interpolations a LUT doesn't
//! take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.cub` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.cub", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.cub"), interp, dir)
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
                    " {} {} {}",
                    0.9 * rf + 0.05 * gf,
                    0.8 * gf + 0.1 * bf * bf,
                    0.7 * bf + 0.2 * rf * gf
                ));
            }
        }
    }
    out
}

/// The entries of a shaper of `size` entries up to `max`.
fn shaper(size: usize, max: f64) -> Vec<String> {
    (0..size)
        .map(|i| {
            let x = i as f64 / (size - 1) as f64;
            format!(" {} {} {}", max * x.powf(0.5), max * x, max * x * x)
        })
        .collect()
}

/// A file of the first line, `tags` (`# ` added), the shaper block and the cube block (each
/// left out when empty), and `# end`.
fn cub(tags: &[&str], shaper_lines: &[String], cube_lines: &[String]) -> Vec<String> {
    let mut out = vec!["# Truelight Cube v2.0".to_owned()];
    out.extend(tags.iter().map(|t| format!("# {t}")));
    if !shaper_lines.is_empty() {
        out.push("# InputLUT".into());
        out.extend(shaper_lines.iter().cloned());
        out.push("".into());
    }
    if !cube_lines.is_empty() {
        out.push("# Cube".into());
        out.extend(cube_lines.iter().cloned());
        out.push("".into());
    }
    out.push("# end".into());
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
    let mut add =
        |label: &str, lines: Vec<String>| cases.push(case(label, text(&lines, "\n").as_bytes()));
    let both = || cub(&["width 3 3 3", "lutLength 5"], &shaper(5, 2.0), &cube(3));

    add("shaper and cube", both());
    add("cube", cub(&["width 3 3 3"], &[], &cube(3)));
    add("cube of 2", cub(&["width 2 2 2"], &[], &cube(2)));
    add("cube of 5", cub(&["width 5 5 5"], &[], &cube(5)));
    add("shaper", cub(&["lutLength 7"], &shaper(7, 1.0), &[]));
    add("shaper past 1", cub(&["lutLength 7"], &shaper(7, 4.0), &[]));
    add("no tags", cub(&[], &[], &[]));
    add("tags only", cub(&["iDims 3", "oDims 3"], &[], &[]));
    cases.push(case("CRLF", text(&both(), "\r\n").as_bytes()));
    cases.push(case("CR", text(&both(), "\r").as_bytes()));
    let mut add =
        |label: &str, lines: Vec<String>| cases.push(case(label, text(&lines, "\n").as_bytes()));
    add("the cube before the shaper", {
        let mut l = vec![
            "# Truelight Cube v2.0".to_owned(),
            "# width 3 3 3".into(),
            "# lutLength 5".into(),
            "# Cube".into(),
        ];
        l.extend(cube(3));
        l.push("# InputLUT".into());
        l.extend(shaper(5, 2.0));
        l
    });
    add(
        "upper-case tags and no space after #",
        both()
            .into_iter()
            .map(|l| match l.as_str() {
                "# InputLUT" => "#INPUTLUT".to_owned(),
                "# Cube" => "#   CUBE".to_owned(),
                _ => l,
            })
            .collect(),
    );
    add(
        "anything after # end",
        [
            both(),
            vec![
                " 1 1 1".into(),
                "garbage".into(),
                "# Cube".into(),
                " 1 1 1".into(),
                "# width 9 9 9".into(),
            ],
        ]
        .concat(),
    );
    add(
        "no # end",
        both().into_iter().filter(|l| l != "# end").collect(),
    );
    add(
        "lines of other lengths in the blocks",
        both()
            .into_iter()
            .flat_map(|l| {
                if l == "# Cube" {
                    vec![
                        l,
                        " 1 2".into(),
                        " 1 2 3 4".into(),
                        " x y z".into(),
                        "text".into(),
                    ]
                } else {
                    vec![l]
                }
            })
            .collect(),
    );
    add(
        "values before any block",
        [
            vec!["# Truelight Cube v2.0".to_owned(), " 1 2 3".into()],
            both()[1..].to_vec(),
        ]
        .concat(),
    );
    add(
        "a tag line with values",
        both()
            .into_iter()
            .map(|l| {
                if l == "# Cube" {
                    "# Cube 1 2 3".into()
                } else {
                    l
                }
            })
            .collect(),
    );
    add(
        "a lone #",
        [both()[..3].to_vec(), vec!["#".into()], both()[3..].to_vec()].concat(),
    );

    // The first line.
    for (label, first) in [
        ("lower case", "# truelight cube"),
        ("leading space", " # Truelight Cube"),
        ("no space after #", "#Truelight Cube"),
        ("another format", "# Truelight Profile"),
    ] {
        let mut lines = both();
        lines[0] = first.to_owned();
        add(label, lines);
    }
    add(
        "blank lines first",
        [vec!["".to_owned(), "   ".into()], both()].concat(),
    );

    // The tags.
    for (label, tag) in [
        ("a width of 2 values", "width 3 3"),
        ("a width of 4 values", "width 3 3 3 3"),
        ("a width that doesn't parse", "width 3 x 3"),
        ("a width with a suffix", "width 3 3 3x"),
        ("unequal widths", "width 3 3 4"),
        ("a width of 1", "width 1 1 1"),
        ("a width of 130", "width 130 130 130"),
        ("a width of 129", "width 129 129 129"),
        ("a negative width", "width -3 -3 -3"),
        ("a lutLength of 2 values", "lutLength 5 5"),
        ("a lutLength that doesn't parse", "lutLength five"),
        ("a lutLength of 1", "lutLength 1"),
        ("a lutLength past 300000", "lutLength 300001"),
        ("a lutLength of 4", "lutLength 4"),
        ("a lutLength of 6", "lutLength 6"),
    ] {
        let mut tags = vec!["width 3 3 3", "lutLength 5"];
        if tag.starts_with("width") {
            tags[0] = tag;
        } else {
            tags[1] = tag;
        }
        add(label, cub(&tags, &shaper(5, 2.0), &cube(3)));
    }
    add(
        "a missing cube entry",
        cub(&["width 3 3 3"], &[], &cube(3)[..26]),
    );
    add(
        "an extra cube entry",
        cub(
            &["width 3 3 3"],
            &[],
            &[cube(3), vec![" 1 1 1".into()]].concat(),
        ),
    );
    add("a cube without a width", cub(&[], &[], &cube(2)));
    add("a shaper without a length", cub(&[], &shaper(4, 1.0), &[]));
    for (label, value) in [
        ("a NaN entry", "nan 0 0"),
        ("an infinite entry", "inf -inf 0"),
        ("an entry past float", "1e39 0 0"),
        ("a hex entry", "0x1p-2 0 0"),
        ("a comma", "0,5 0 0"),
    ] {
        let mut c = cube(3);
        c[4] = format!(" {value}");
        add(label, cub(&["width 3 3 3"], &[], &c));
    }
    add("the first line alone", vec!["# Truelight Cube v2.0".into()]);
    cases.push(case("empty", b""));
    cases
}

#[test]
fn truelight_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "cub") {
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
        (
            "shaper and cube",
            cub(&["width 3 3 3", "lutLength 5"], &shaper(5, 2.0), &cube(3)),
        ),
        ("cube", cub(&["width 3 3 3"], &[], &cube(3))),
        ("shaper", cub(&["lutLength 7"], &shaper(7, 1.0), &[])),
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
