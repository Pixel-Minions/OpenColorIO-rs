// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The spimtx reader against the wheel (`processor_ops`): the processors of file transforms
//! of generated spimtx files and of upstream's, both directions of the processor and of the
//! transform, every value of the matrix by its bits; and the errors of the files the reader
//! refuses (counts, words, sizes around its 1024-byte cap), with the line ends, `0x1A` and
//! NUL bytes that text mode reads differently on Windows and Linux.

mod common;

use common::lut_files::{Entry, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a set of one file, `lut.spimtx`, holding `bytes`.
fn case(label: &str, bytes: &[u8]) -> Case {
    let dir = write_files(&[file("lut.spimtx", bytes)]);
    file_case(
        label,
        &format!("{dir}/lut.spimtx"),
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// The 12 numbers of a valid file, joined by `sep`.
fn numbers(sep: &str) -> String {
    [
        "0.754338638",
        "0.133697046",
        "0.111968437",
        "6553.5",
        "0.021198141",
        "1.005410934",
        "-0.026610548",
        "32767.5",
        "-0.009756991",
        "0.004508563",
        "1.005253201",
        "65535.0",
    ]
    .join(sep)
}

#[test]
fn spimtx_files_read_as_in_the_wheel() {
    let mut cases = vec![
        case("spaces", numbers(" ").as_bytes()),
        case("line feeds", format!("{}\n", numbers("\n")).as_bytes()),
        case("CR LF", format!("{}\r\n", numbers("\r\n")).as_bytes()),
        case("lone CRs", numbers("\r").as_bytes()),
        case("tabs and form feeds", numbers("\t\x0b\x0c").as_bytes()),
        case(
            "leading and trailing white space",
            format!(" \n\t{}\n\n ", numbers(" ")).as_bytes(),
        ),
        case(
            "number forms",
            b"1e0 +0 -0 .5 1. 0x10 1e-3 -1E2 3.40282347e38 1e-45 inf -nan".as_slice(),
        ),
        case("overflow", b"1e39 0 0 0 0 1 0 0 0 0 1 0".as_slice()),
        case(
            "11 numbers",
            numbers(" ").rsplit_once(' ').unwrap().0.as_bytes(),
        ),
        case("13 numbers", format!("{} 1", numbers(" ")).as_bytes()),
        case("a word", numbers(" ").replace("6553.5", "error").as_bytes()),
        case(
            "a number and a word",
            numbers(" ").replace("6553.5", "6553.5x").as_bytes(),
        ),
        case("empty", b"".as_slice()),
        case("white space only", b" \n\t\r\n".as_slice()),
        case(
            "0x1A after the numbers",
            format!("{}\x1a 1 2 3", numbers(" ")).as_bytes(),
        ),
        case(
            "0x1A in the numbers",
            format!("1 2 3\x1a{}", numbers(" ")).as_bytes(),
        ),
        case(
            "NUL after the numbers",
            format!("{}\0", numbers(" ")).as_bytes(),
        ),
        case(
            "NUL in a number",
            numbers(" ").replace("6553.5", "65\x003.5").as_bytes(),
        ),
    ];
    // The 1024-byte cap: the file must end before it.
    for size in [1023, 1024, 1025, 2048] {
        let mut bytes = numbers(" ").into_bytes();
        bytes.resize(size, b' ');
        cases.push(case(&format!("{size} bytes"), &bytes));
        // The CR LFs Windows reads as LFs bring the file under the cap there.
        let mut bytes = numbers(" ").into_bytes();
        while bytes.len() + 2 <= size {
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.resize(size, b' ');
        cases.push(case(&format!("{size} bytes of CR LFs"), &bytes));
    }

    // Upstream's file, both directions of the transform, and a directory of the extension
    // (on Linux it opens and fails to read; on Windows it doesn't open).
    let upstream =
        ocio_testkit::paths::upstream_dir().join("tests/data/files/camera_to_aces.spimtx");
    let upstream = upstream.to_str().unwrap().replace('\\', "/");
    let upstream = upstream.trim_start_matches("//?/");
    for (interp, dir) in [
        (Interpolation::Default, TransformDirection::Forward),
        (Interpolation::Nearest, TransformDirection::Inverse),
    ] {
        cases.push(file_case(
            format!("{upstream} {interp:?} {dir:?}"),
            upstream,
            interp,
            dir,
        ));
    }
    let dir = write_files(&[Entry::Dir("lut.spimtx".to_owned())]);
    cases.push(file_case(
        "a directory",
        &format!("{dir}/lut.spimtx"),
        Interpolation::Default,
        TransformDirection::Forward,
    ));

    check_file_processors(&cases);
}
