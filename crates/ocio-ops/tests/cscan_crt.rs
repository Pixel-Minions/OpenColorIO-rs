// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio_ops::utils::cscan` against the platform's C runtime (`ocio_testkit::crt::sscanf`:
//! `sscanf_s` on Windows, `sscanf` on Linux): every format OCIO's LUT readers use, and more,
//! over generated inputs, the count and every argument after the call.

use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_testkit::crt::{self, Scanned};
use ocio_testkit::probe::Rng;

/// The formats of the readers (FileFormatSpi1D.cpp, FileFormatSpi3D.cpp:108 and 167,
/// FileFormatIridasCube.cpp, FileFormatDiscreet1DL.cpp, ctf/CTFReaderHelper.cpp:759,
/// ctf/CTFTransform.cpp:85 @ v2.5.2) and a few more of the same directives.
const FORMATS: &[&str] = &[
    "%d %d %d",
    "%d %d %d %63s %63s %63s",
    "%d.%d.%d",
    "Version %d",
    "From %63s %63s",
    "Length %d",
    "Components %d",
    "%63s %63s %63s %63s",
    "%63s %63s %63s %c",
    "domain_min %63s %63s %63s %c",
    "lut_1d_size %d %c",
    "lut_3d_size %d %c",
    "%*s %d %d %15s",
    "%d%c",
    "%d",
    "%c",
    "%3s%3s",
    "%1s %d",
    "a%db",
];

/// Pieces the inputs are made of: the formats' words, numbers at the limits, signs, white
/// space of every kind, and bytes past ASCII (0xA0 among them: white space in code page 1252,
/// not in the "C" locale both follow).
const PIECES: &[&[u8]] = &[
    b"Version",
    b"From",
    b"Length",
    b"Components",
    b"domain_min",
    b"lut_1d_size",
    b"lut_3d_size",
    b"LUT:",
    b"0",
    b"7",
    b"-3",
    b"+4",
    b"-",
    b"+",
    b"2147483647",
    b"2147483648",
    b"-2147483649",
    b"4294967296",
    b"9223372036854775807",
    b"9223372036854775808",
    b"-9223372036854775809",
    b"99999999999999999999",
    b"0x1A",
    b"1.5",
    b"16f",
    b"x",
    b"abcdefgh",
    b" ",
    b"  ",
    b"\t",
    b"\n",
    b"\x0b",
    b"\x0c",
    b"\r",
    b"a",
    b"b",
    b"\xff",
    b"\x80",
    b"\xa0",
    b".",
    b"%",
];

fn port(input: &[u8], format: &str, kinds: &[Scanned]) -> crt::Scan {
    let mut ints = vec![0i32; kinds.len()];
    let mut strs: Vec<Vec<u8>> = vec![vec![0u8; 256]; kinds.len()];
    let mut chars = vec![0u8; kinds.len()];
    let count = {
        let mut ints_iter = ints.iter_mut();
        let mut strs_iter = strs.iter_mut();
        let mut chars_iter = chars.iter_mut();
        let mut args: Vec<ScanArg<'_>> = kinds
            .iter()
            .map(|k| match k {
                Scanned::Int(_) => ScanArg::Int(ints_iter.next().unwrap()),
                Scanned::Str(_) => ScanArg::Str(strs_iter.next().unwrap()),
                Scanned::Char(_) => ScanArg::Char(chars_iter.next().unwrap()),
            })
            .collect();
        sscanf(ScanCrt::NATIVE, input, format.as_bytes(), &mut args)
    };
    let (mut i, mut s, mut c) = (0, 0, 0);
    let values = kinds
        .iter()
        .map(|k| match k {
            Scanned::Int(_) => {
                i += 1;
                Scanned::Int(ints[i - 1])
            }
            Scanned::Str(_) => {
                s += 1;
                let b = &strs[s - 1];
                Scanned::Str(b[..b.iter().position(|&x| x == 0).unwrap()].to_vec())
            }
            Scanned::Char(_) => {
                c += 1;
                Scanned::Char(chars[c - 1])
            }
        })
        .collect();
    crt::Scan { count, values }
}

/// Every format over 4,000 generated inputs: the port's scan is the C runtime's.
#[test]
fn scans_match_the_c_runtime() {
    let mut rng = Rng::new(0x5ca9);
    let mut failures = Vec::new();
    for format in FORMATS {
        for n in 0..4000 {
            let mut input = Vec::new();
            // Inputs that start like the format, and inputs of anything.
            if n % 2 == 0 {
                let word = format.split(' ').next().unwrap();
                if !word.starts_with('%') {
                    input.extend_from_slice(word.as_bytes());
                }
            }
            for _ in 0..rng.next_u64() % 7 {
                input.extend_from_slice(PIECES[(rng.next_u64() % PIECES.len() as u64) as usize]);
            }
            let reference = crt::sscanf(&input, format);
            let ours = port(&input, format, &reference.values);
            if ours != reference {
                failures.push(format!(
                    "{format:?} on {:?}: C {reference:?}, port {ours:?}",
                    input.escape_ascii().to_string()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}

/// Deviation D-1, pinned: under Python the Windows wheel scans in code page 1252's locale,
/// where 0xA0 is white space, so its spi1d reader takes `Length\xa03` for a length of 3; the
/// Linux wheel, in a C or UTF-8 locale, doesn't. The port scans in the "C" locale on both, as
/// the C runtime's reference does (`ocio_testkit::crt::sscanf`).
#[test]
fn a_no_break_space_is_white_space_only_in_the_windows_wheel() {
    use ocio_testkit::Oracle;
    use serde_json::json;

    let line = b"Length\xa03";
    let reference = crt::sscanf(line, "Length %d");
    assert_eq!(reference.count, 0);
    let mut length = 0;
    let count = sscanf(
        ScanCrt::NATIVE,
        line,
        b"Length %d",
        &mut [ScanArg::Int(&mut length)],
    );
    assert_eq!((count, length), (reference.count, 0));

    let spi1d = [
        b"Version 1\nFrom 0.0 1.0\n".as_slice(),
        line,
        b"\nComponents 1\n{\n0\n0.5\n1\n}\n",
    ]
    .concat();
    let hex: String = spi1d.iter().map(|b| format!("{b:02x}")).collect();
    let result = Oracle::get()
        .call(
            "with_files",
            json!({"files": {"lut.spi1d": {"bytes": hex}}, "command": "processor_ops",
                   "args": {"transform": {"class": "FileTransform",
                                          "calls": [["setSrc", "$FILES/lut.spi1d"]]}}}),
            &[],
        )
        .result;
    if cfg!(windows) {
        assert!(result.get("exception").is_none(), "{result}");
    } else {
        // The reader quotes the line, which isn't UTF-8.
        assert_eq!(
            result["exception"]["type"], "UnicodeDecodeError",
            "{result}"
        );
        let message = ocio_testkit::oracle_values::bytes(
            &json!({"bytes": result["exception"]["undecodable"]}),
        );
        let needle = b"Invalid 'Length' Tag";
        assert!(
            message.windows(needle.len()).any(|w| w == needle),
            "{result}"
        );
    }
}
