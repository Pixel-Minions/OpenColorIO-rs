// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Discreet 1D LUT reader against the wheel (`processor_ops`, from empty caches): the
//! processors of file transforms of generated `.lut` files and of upstream's (both processor
//! directions, an inverse transform; the LUT by its bits), and the reader's errors: old and new
//! formats, table counts and lengths, depths from the header and from the file's name, half
//! tables, numbers `std::stoi` takes and refuses, blank lines, comments, tabs, line ends, and
//! what follows the tables.
//!
//! The `.lut` extension is also the Houdini format's, whose reader isn't ported yet: where the
//! Discreet reader fails, the wheel's error holds the Houdini reader's error too, so the errors
//! compare up to it (the Discreet reader's error in full).

mod common;

use common::lut_files::{LOGGING, check_file_processors, check_logs, file, file_case, write_files};
use common::transforms::{Case, port_processors};
use ocio::{BitDepth, Config, Interpolation, OptimizationFlags, TransformDirection};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::processor_ops::{ProcessorOpsReply, ProcessorOpsRequest};
use serde_json::json;

/// The case of a file `name` holding `bytes`, read with `interp`, the transform in the
/// direction `dir`. The set holds the label too, so that no two cases share a path (the file
/// cache keeps a path's first read, with its interpolation).
fn case_with(
    label: &str,
    name: &str,
    bytes: &[u8],
    interp: Interpolation,
    dir: TransformDirection,
) -> Case {
    let dir_path = write_files(&[file(name, bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/{name}"), interp, dir)
}

/// The case of a file `name` holding `bytes`.
fn case(label: &str, name: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        name,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// `count` entries of a table, one per line, each `f(i)`, ended by `eol`.
fn table(count: usize, eol: &str, f: impl Fn(usize) -> String) -> String {
    (0..count).map(|i| format!("{}{eol}", f(i))).collect()
}

/// A new-format file: `header`, then `tables` tables of `length` entries.
fn new_format(header: &str, tables: usize, length: usize, max: usize) -> String {
    let mut text = format!("{header}\n");
    for t in 0..tables {
        text.push_str(&table(length, "\n", |i| {
            ((i * (max + 1) / length + t * 7) % (max + 1)).to_string()
        }));
    }
    text
}

/// Files the reader takes.
fn readable_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let ramp = |i: usize| ((i * 255) / 255).to_string();
    let old = table(256, "\n", ramp);
    cases.push(case("old format", "old.lut", old.as_bytes()));
    cases.push(case(
        "old format, CR LF",
        "old.lut",
        table(256, "\r\n", ramp).as_bytes(),
    ));
    cases.push(case(
        "old format, comments, blank lines, tabs and spaces",
        "old.lut",
        format!(
            "# a comment\n\n  \t\n{}# after\n\n",
            table(256, "\n", |i| format!(" \t{}\t ", (i * 3) % 256))
        )
        .as_bytes(),
    ));
    cases.push(case(
        "old format, values past 65535 and with words after them",
        "old.lut",
        table(256, "\n", |i| match i % 4 {
            0 => (65536 + i).to_string(),
            1 => format!("{i}abc"),
            2 => format!("0000{i} 99"),
            _ => (2_147_483_647 - i).to_string(),
        })
        .as_bytes(),
    ));
    cases.push(case(
        "old format, a 199-byte line",
        "old.lut",
        table(
            256,
            "
",
            |i| {
                if i == 5 {
                    format!("5{}", " ".repeat(198))
                } else {
                    i.to_string()
                }
            },
        )
        .as_bytes(),
    ));
    for (label, header, tables, length, max) in [
        ("1 table of 256", "LUT: 1 256", 1, 256, 255),
        ("3 tables of 1024", "LUT: 3 1024", 3, 1024, 1023),
        ("4 tables of 4096", "LUT: 4 4096", 4, 4096, 4095),
        ("lower case, 12 to 10", "lut: 3 4096 1024", 3, 4096, 1023),
        ("8 to 16", "LUT: 3 256 65536", 3, 256, 65535),
        ("8 to 16f", "LUT: 3 256 65536f", 3, 256, 31743),
        ("8 to 16F, tabs", "LUT:\t3\t256\t65536F", 3, 256, 31743),
        ("16f to 16f", "LUT: 3 65536 65536f", 3, 65536, 31743),
        ("16f to 12", "LUT: 1 65536 4096", 1, 65536, 4095),
        (
            "8 to 8, extra words",
            "LUT: 3 256 256 more words",
            3,
            256,
            255,
        ),
        ("2 entries of 4096", "LUT: 3 2 4096", 3, 2, 4095),
    ] {
        cases.push(case(
            label,
            "lut.lut",
            new_format(header, tables, length, max).as_bytes(),
        ));
    }
    // The depth from the file's name, without one in the header.
    for name in [
        "a12to10.lut",
        "x_to8.lut",
        "TO12log.lut",
        "lin_to16.lut",
        "lin_to16f.lut",
        "lin_to16Fp.lut",
        "to32F.lut",
        "to32.lut",
        "tox.lut",
        "to1.lut",
        "toto10.lut",
    ] {
        cases.push(case(
            &format!("named {name}"),
            name,
            new_format("LUT: 3 256", 3, 256, 255).as_bytes(),
        ));
    }
    cases
}

/// Files the reader refuses.
fn refused_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let ramp = |i: usize| i.to_string();
    let lut = new_format("LUT: 3 256", 3, 256, 255);
    for (label, text) in [
        ("empty", String::new()),
        ("comments only", "# a\n\n#b\n".to_owned()),
        ("old format, short", table(100, "\n", ramp)),
        (
            "old format, no line end at the end",
            table(256, "\n", ramp).trim_end().to_owned(),
        ),
        (
            "old format, a word",
            table(
                256,
                "\n",
                |i| if i == 9 { "x".into() } else { i.to_string() },
            ),
        ),
        (
            "old format, a sign",
            table(
                256,
                "\n",
                |i| if i == 9 { "+9".into() } else { i.to_string() },
            ),
        ),
        (
            "old format, past int",
            table(256, "\n", |i| {
                if i == 9 {
                    "2147483648".into()
                } else {
                    i.to_string()
                }
            }),
        ),
        (
            "old format, a first line past int",
            table(256, "\n", |i| {
                if i == 0 {
                    "99999999999".into()
                } else {
                    i.to_string()
                }
            }),
        ),
        (
            "old format, more after",
            format!("{}17\n", table(256, "\n", ramp)),
        ),
        (
            "old format, a 200-byte line",
            table(256, "\n", |i| {
                if i == 5 {
                    format!("5{}", " ".repeat(199))
                } else {
                    i.to_string()
                }
            }),
        ),
        ("2 tables", new_format("LUT: 2 256", 2, 256, 255)),
        ("no length", "LUT: 3\n1\n".to_owned()),
        ("length 0", new_format("LUT: 3 0", 3, 0, 255)),
        ("length 65537", "LUT: 3 65537\n".to_owned()),
        ("length 1", new_format("LUT: 3 1", 3, 1, 255)),
        (
            "no space after the colon",
            lut.replacen("LUT: 3", "LUT:3", 1),
        ),
        (
            "a word after the colon",
            lut.replacen("LUT: 3", "LUT:x 3", 1),
        ),
        ("a longer word", lut.replacen("LUT: 3", "LUTS: 3", 1)),
        ("not LUT", lut.replacen("LUT:", "XYZ:", 1)),
        (
            "an unknown depth",
            lut.replacen("LUT: 3 256", "LUT: 3 256 1000", 1),
        ),
        (
            "a depth of 16",
            lut.replacen("LUT: 3 256", "LUT: 3 256 16f", 1),
        ),
        ("a length of 100", new_format("LUT: 3 100", 3, 100, 255)),
        ("short tables", new_format("LUT: 3 256", 2, 256, 255)),
        (
            "a table entry with a word",
            lut.replacen("\n7\n", "\nseven\n", 1),
        ),
        ("more after the tables", format!("{lut}0\n")),
    ] {
        cases.push(case(label, "lut.lut", text.as_bytes()));
    }
    cases
}

/// The message up to the Houdini reader's part, which follows the Discreet reader's.
fn up_to_houdini(message: &str) -> &str {
    message
        .split("    'houdini' failed with:")
        .next()
        .unwrap_or(message)
}

#[test]
fn discreet_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = readable_cases();
    // Upstream's files.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for name in [
        "logtolin_8to8.lut",
        "Test_12to16fp.lut",
        "photo_default_16fpto16fp.lut",
        "Test_16fpto12.lut",
    ] {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        cases.push(case(name, name, &bytes));
    }
    cases.push(case_with(
        "an inverse transform",
        "lut.lut",
        new_format("LUT: 3 1024 4096", 3, 1024, 4095).as_bytes(),
        Interpolation::Linear,
        TransformDirection::Inverse,
    ));
    check_file_processors(&cases);
}

#[test]
fn discreet_errors_read_as_in_the_wheel() {
    let cases = refused_cases();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|case| {
            let mut request = ProcessorOpsRequest::new(json!({"transform": case.spec}));
            request.optimization = Some(json!("OPTIMIZATION_NONE"));
            BatchCall {
                cmd: "with_files",
                args: json!({"files": {}, "command": "processor_ops", "args": request.args()}),
                blobs: Vec::new(),
            }
        })
        .collect();
    let config = Config::create_raw().unwrap();
    let mut failures = Vec::new();
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = ProcessorOpsReply::from_response(response.unwrap_or_else(|e| panic!("{e}")));
        let wheel = reply
            .raised()
            .unwrap_or_else(|| panic!("{}: the wheel read it {}", case.label, reply.result));
        let port = port_processors(
            &config,
            &case.port,
            TransformDirection::Forward,
            (BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE),
        );
        let port = match port {
            Err((stage, e)) => (stage, e.message().to_owned()),
            Ok(_) => {
                failures.push(format!("{}: the port read it", case.label));
                continue;
            }
        };
        if (wheel.stage.as_str(), up_to_houdini(&wheel.message)) != (port.0, up_to_houdini(&port.1))
        {
            failures.push(format!(
                "{}:\n  wheel {} {:?}\n  port  {} {:?}",
                case.label, wheel.stage, wheel.message, port.0, port.1
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The interpolations a 1D LUT doesn't take log a warning, as in the wheel.
#[test]
fn interpolations_log_as_in_the_wheel() {
    let text = new_format("LUT: 3 256", 3, 256, 255);
    let cases: Vec<Case> = [
        Interpolation::Linear,
        Interpolation::Tetrahedral,
        Interpolation::Cubic,
    ]
    .into_iter()
    .map(|interp| {
        case_with(
            &format!("{interp:?}"),
            "lut.lut",
            text.as_bytes(),
            interp,
            TransformDirection::Forward,
        )
    })
    .collect();
    check_logs(&cases, TransformDirection::Forward);
}
