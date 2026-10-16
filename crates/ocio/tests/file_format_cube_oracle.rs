// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `.cube` readers against the wheel (`processor_ops`): a `.cube` file is read as an Iridas
//! cube first and, where that fails, as a Resolve cube. The processors of file transforms of
//! generated files and of upstream's, both directions of the processor and of the transform,
//! the LUTs and the domains or ranges by their bits; the errors of the files both refuse (each
//! reader's tags, sizes, domains and ranges, the entries, comments, the line numbers the
//! messages give); and what they log for the interpolations a LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.cube` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.cube", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.cube"), interp, dir)
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
fn entries3d(size: usize) -> Vec<String> {
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

/// The entries of a 1D LUT of `size` entries.
fn entries1d(size: usize) -> Vec<String> {
    let s = (size - 1) as f64;
    (0..size)
        .map(|i| {
            let x = i as f64 / s;
            format!("{} {} {}", x * x, x.sqrt(), 0.5 * x + 0.1)
        })
        .collect()
}

/// A file of `header` lines, then `data`, each line ended by `eol`.
fn cube(header: &[&str], data: &[String], eol: &str) -> String {
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

/// Files the readers take and refuse, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: &str, text: String| cases.push(case(label, text.as_bytes()));
    let d3 = entries3d(2);
    let d1 = entries1d(5);

    // Iridas cubes.
    add("3D", cube(&["LUT_3D_SIZE 2"], &d3, "\n"));
    add("3D, CRLF", cube(&["LUT_3D_SIZE 2"], &d3, "\r\n"));
    add("3D, CR", cube(&["LUT_3D_SIZE 2"], &d3, "\r"));
    add("3D of 5", cube(&["LUT_3D_SIZE 5"], &entries3d(5), "\n"));
    add("1D", cube(&["LUT_1D_SIZE 5"], &d1, "\n"));
    add(
        "3D with a domain",
        cube(
            &[
                "TITLE \"x\"",
                "LUT_3D_SIZE 2",
                "DOMAIN_MIN -0.5 0 0.25",
                "DOMAIN_MAX 2 1 1.5",
            ],
            &d3,
            "\n",
        ),
    );
    add(
        "1D with a domain",
        cube(
            &["LUT_1D_SIZE 5", "DOMAIN_MIN 0 0 0", "DOMAIN_MAX 4 4 4"],
            &d1,
            "\n",
        ),
    );
    add(
        "lower-case tags",
        cube(&["lut_3d_size 2", "domain_min 0.1 0.1 0.1"], &d3, "\n"),
    );
    add(
        "comments and blank lines",
        cube(
            &[
                "# a",
                "",
                "   ",
                "LUT_3D_SIZE 2",
                "# b",
                "\t",
                "DOMAIN_MAX 2 2 2",
                "# c",
            ],
            &d3,
            "\n",
        ),
    );
    let mut with_comments = d3.clone();
    with_comments.insert(3, "# in the data".into());
    with_comments.insert(5, "   # indented".into());
    with_comments.insert(6, "".into());
    add(
        "comments in the data",
        cube(&["LUT_3D_SIZE 2"], &with_comments, "\n"),
    );
    add(
        "an indented comment in the header",
        cube(&["LUT_3D_SIZE 2", "  # indented"], &d3, "\n"),
    );
    add(
        "1D and 3D sizes",
        cube(&["LUT_1D_SIZE 5", "LUT_3D_SIZE 2"], &d1, "\n"),
    );
    add(
        "two 3D sizes",
        cube(&["LUT_3D_SIZE 3", "LUT_3D_SIZE 2"], &d3, "\n"),
    );
    let mut no_eol = cube(&["LUT_3D_SIZE 2"], &d3, "\n");
    no_eol.pop();
    add("no line feed at the end", no_eol);
    let tabs: Vec<String> = d3.iter().map(|l| l.replace(' ', "\t")).collect();
    add("tabs", cube(&["LUT_3D_SIZE\t2"], &tabs, "\n"));

    // Resolve cubes (the Iridas reader refuses them).
    add(
        "1D shaper and 3D",
        cube(
            &[
                "LUT_1D_SIZE 5",
                "LUT_1D_INPUT_RANGE -0.25 1.5",
                "LUT_3D_SIZE 2",
                "LUT_3D_INPUT_RANGE 0.1 0.9",
            ],
            &[d1.clone(), d3.clone()].concat(),
            "\n",
        ),
    );
    add(
        "1D and 3D, default ranges",
        cube(
            &["LUT_1D_SIZE 5", "LUT_3D_SIZE 2"],
            &[d1.clone(), d3.clone()].concat(),
            "\n",
        ),
    );
    add(
        "1D with an input range",
        cube(&["LUT_1D_SIZE 5", "LUT_1D_INPUT_RANGE 0 4"], &d1, "\n"),
    );
    add(
        "3D with an input range",
        cube(&["LUT_3D_SIZE 2", "LUT_3D_INPUT_RANGE -1 2"], &d3, "\n"),
    );
    add(
        "the 3D size after the 1D data",
        cube(
            &["LUT_1D_SIZE 5"],
            &[d1.clone(), vec!["LUT_3D_SIZE 2".into()], d3.clone()].concat(),
            "\n",
        ),
    );
    let mut comment_after = [d1.clone(), d3.clone()].concat();
    comment_after.insert(2, "# not allowed".into());
    add(
        "a comment after the header",
        cube(&["LUT_1D_SIZE 5", "LUT_3D_SIZE 2"], &comment_after, "\n"),
    );
    add(
        "a title",
        cube(
            &["TITLE \"t\"", "LUT_1D_SIZE 5", "LUT_3D_SIZE 2"],
            &[d1.clone(), d3.clone()].concat(),
            "\n",
        ),
    );
    for (label, tag) in [
        ("a 1D range of 1 value", "LUT_1D_INPUT_RANGE 0"),
        ("a 1D range of 3 values", "LUT_1D_INPUT_RANGE 0 1 2"),
        ("a 1D range that doesn't parse", "LUT_1D_INPUT_RANGE 0 x"),
        ("a 3D range that doesn't parse", "LUT_3D_INPUT_RANGE y 1"),
        ("a 1D size that doesn't parse", "LUT_1D_SIZE five"),
        ("a 1D size with a suffix", "LUT_1D_SIZE 5x"),
        ("a 1D size of 1", "LUT_1D_SIZE 1"),
        ("a 1D size past 300000", "LUT_1D_SIZE 300001"),
        ("a 1D size of two values", "LUT_1D_SIZE 5 5"),
    ] {
        add(
            label,
            cube(
                &[tag, "LUT_3D_SIZE 2"],
                &[d1.clone(), d3.clone()].concat(),
                "\n",
            ),
        );
    }

    // Both refuse.
    for (label, header) in [
        ("no size", vec![]),
        ("a 2D size", vec!["LUT_2D_SIZE 4"]),
        ("a 3D size of 1", vec!["LUT_3D_SIZE 1"]),
        ("a 3D size of 130", vec!["LUT_3D_SIZE 130"]),
        ("a 3D size of 129", vec!["LUT_3D_SIZE 129"]),
        ("a negative 3D size", vec!["LUT_3D_SIZE -2"]),
        ("a 3D size past int", vec!["LUT_3D_SIZE 4294967298"]),
        ("a 3D size of two values", vec!["LUT_3D_SIZE 2 2"]),
        ("a 3D size with a letter", vec!["LUT_3D_SIZE 2x"]),
        ("a 3D size alone", vec!["LUT_3D_SIZE"]),
        (
            "a domain of 2 values",
            vec!["LUT_3D_SIZE 2", "DOMAIN_MIN 0 0"],
        ),
        (
            "a domain of 4 values",
            vec!["LUT_3D_SIZE 2", "DOMAIN_MAX 1 1 1 1"],
        ),
        (
            "a domain that doesn't parse",
            vec!["LUT_3D_SIZE 2", "DOMAIN_MAX 1 x 1"],
        ),
        (
            "a blue domain that doesn't parse",
            vec!["LUT_3D_SIZE 2", "DOMAIN_MIN 0 0 z"],
        ),
        ("an unknown tag", vec!["LUT_3D_SIZE 2", "WRONG_TAG"]),
    ] {
        add(label, cube(&header, &d3, "\n"));
    }
    let with = |i: usize, line: &str| {
        let mut d = d3.clone();
        d[i] = line.to_owned();
        d
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
        ("a 64-character value", &format!("0.{} 0 0", "1".repeat(62))),
        ("a value split by %63s", &format!("0.{} 0", "5".repeat(70))),
        ("an entry with a NUL", "0 0\0 0"),
    ] {
        add(label, cube(&["LUT_3D_SIZE 2"], &with(1, line), "\n"));
    }
    let mut extra = d3.clone();
    extra.push("1 1 1".into());
    add("an entry too many", cube(&["LUT_3D_SIZE 2"], &extra, "\n"));
    add("an entry missing", cube(&["LUT_3D_SIZE 2"], &d3[..7], "\n"));
    add(
        "1D entries missing",
        cube(&["LUT_1D_SIZE 5"], &d1[..3], "\n"),
    );
    add("a size and no entries", "LUT_3D_SIZE 2\n".into());
    add("empty", String::new());
    add("white space only", " \n\t\n".into());
    let mut numbered = d3.clone();
    numbered.insert(4, "".into());
    numbered.insert(5, "# c".into());
    numbered[6] = "0 x 0".into();
    add(
        "the line number after blank lines and comments",
        cube(&["# a", "", "LUT_3D_SIZE 2", "   "], &numbered, "\n"),
    );
    // Iridas domains of NaNs, infinities, and equal bounds (the min-max op refuses them).
    for (label, min, max) in [
        ("NaN and -NaN", "nan 0 -nan", "1 1 1"),
        ("NaNs at both ends", "nan nan -nan", "-nan 1 nan"),
        ("infinities", "-inf 0 0", "inf 1 1"),
        ("a signaling NaN", "nan(snan) 0 0", "1 1 1"),
        ("equal bounds", "1 1 1", "1 1 1"),
        ("a NaN maximum", "0 0 0", "nan -nan 1"),
        ("a minimum above the maximum", "1 0 0", "0 1 1"),
        ("0x", "0x 0 0", "1 1 1"),
        ("a hex float", "-0x1p-2 0 0", "1 1 1"),
    ] {
        let domain_min = format!("DOMAIN_MIN {min}");
        let domain_max = format!("DOMAIN_MAX {max}");
        add(
            &format!("an Iridas domain of {label}"),
            cube(&["LUT_3D_SIZE 2", &domain_min, &domain_max], &d3, "\n"),
        );
    }
    // Resolve ranges of the same kinds.
    for (label, range_1d, range_3d) in [
        ("NaN", "nan 1", "0 1"),
        ("NaNs", "-nan nan", "nan -nan"),
        ("equal 1D bounds", "1 1", "0 1"),
        ("equal 3D bounds", "0 1", "2 2"),
        ("infinities", "-inf inf", "0 inf"),
        ("0x", "0x 1", "0 1"),
    ] {
        let r1 = format!("LUT_1D_INPUT_RANGE {range_1d}");
        let r3 = format!("LUT_3D_INPUT_RANGE {range_3d}");
        let header = ["LUT_1D_SIZE 2", &r1, "LUT_3D_SIZE 2", &r3];
        let data = [vec!["0 0 0".to_owned(), "1 1 1".to_owned()], d3.clone()].concat();
        add(
            &format!("a Resolve range of {label}"),
            cube(&header, &data, "\n"),
        );
    }
    cases
}

#[test]
fn cube_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();

    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "cube") {
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
    let d3 = entries3d(2);
    let d1 = entries1d(5);
    let files = [
        ("iridas 3D", cube(&["LUT_3D_SIZE 2"], &d3, "\n")),
        ("iridas 1D", cube(&["LUT_1D_SIZE 5"], &d1, "\n")),
        (
            "resolve 1D and 3D",
            cube(
                &["LUT_1D_SIZE 5", "LUT_3D_SIZE 2"],
                &[d1.clone(), d3.clone()].concat(),
                "\n",
            ),
        ),
        (
            "resolve 1D",
            cube(&["LUT_1D_SIZE 5", "LUT_1D_INPUT_RANGE 0 2"], &d1, "\n"),
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
