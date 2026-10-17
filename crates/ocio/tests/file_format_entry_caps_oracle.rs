// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The line readers' caps on the entries they collect, against the wheel (`processor_ops`):
//! each 3D LUT reader's "Too many ... entries" (Truelight, Pandora, .vf, Resolve .cube, .itx,
//! .3dl and Houdini), the Truelight 1D cap, and an Iridas `LUT_1D_SIZE` of exactly 300000.
//! Each cap is checked one entry below and at the first entry it refuses: the readers test
//! the count before they add an entry, so 129^3 + 1 entries pass the cap (and fail on the
//! LUT's size) and 129^3 + 2 hit it. Resolve tests it before each value, so 129^3 + 1
//! entries hit it already, at the same line as 129^3 + 2.
//!
//! The files are 10 to 20 MB each; the quick tier reads the .itx and Truelight 3D pairs, the
//! full and exhaustive tiers every pair.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};
use ocio_testkit::battery::Tier;

/// 129^3: the largest 3D LUT's entries (`Max3DLUTLength`, Lut3DOpData.h @ v2.5.2).
const CUBE_129: usize = 129 * 129 * 129;

/// The case of a file `lut.<ext>` holding `bytes`, in its own directory.
fn case(ext: &str, label: &str, bytes: &[u8]) -> Case {
    let name = format!("lut.{ext}");
    let dir_path = write_files(&[file(&name, bytes), file("label.txt", label)]);
    file_case(
        label,
        &format!("{dir_path}/{name}"),
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// `header`, then `count` copies of `line`, then `footer`.
fn repeated(header: &str, line: &str, count: usize, footer: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(header.len() + line.len() * count + footer.len());
    out.extend_from_slice(header.as_bytes());
    for _ in 0..count {
        out.extend_from_slice(line.as_bytes());
    }
    out.extend_from_slice(footer.as_bytes());
    out
}

/// A reader's pair of files around its 3D cap: `CUBE_129 + 1` and `CUBE_129 + 2` entries.
struct Pair {
    reader: &'static str,
    ext: &'static str,
    header: &'static str,
    line: &'static str,
    footer: &'static str,
    /// In the quick tier too.
    quick: bool,
}

const PAIRS: [Pair; 6] = [
    Pair {
        reader: "itx",
        ext: "itx",
        header: "LUT_3D_SIZE 129\n",
        line: "0 0 0\n",
        footer: "",
        quick: true,
    },
    Pair {
        reader: "truelight",
        ext: "cub",
        header: "# Truelight Cube v2.0\n# width 129 129 129\n# Cube\n",
        line: "0 0 0\n",
        footer: "# end\n",
        quick: true,
    },
    Pair {
        reader: "pandora",
        ext: "mga",
        header: "channel 3d\nin 2146689\nout 256\nformat lut\nvalues red green blue\n",
        line: "0 0 0 0\n",
        footer: "",
        quick: false,
    },
    Pair {
        reader: "vf",
        ext: "vf",
        header: "#Inventor V2.1 ascii\ngrid_size 129 129 129\ndata\n",
        line: "0 0 0\n",
        footer: "",
        quick: false,
    },
    Pair {
        reader: "resolve",
        ext: "cube",
        header: "LUT_3D_SIZE 129\nLUT_3D_INPUT_RANGE 0 1\n",
        line: "0 0 0\n",
        footer: "",
        quick: false,
    },
    Pair {
        reader: "3dl",
        ext: "3dl",
        header: "",
        line: "0 0 0\n",
        footer: "",
        quick: false,
    },
];

/// The cases of `tier`.
fn cases(tier: Tier) -> Vec<Case> {
    let quick = tier == Tier::Quick;
    let mut cases = Vec::new();
    for pair in PAIRS.iter().filter(|p| p.quick || !quick) {
        for count in [CUBE_129 + 1, CUBE_129 + 2] {
            let bytes = repeated(pair.header, pair.line, count, pair.footer);
            let label = format!("{} cap, {count} entries", pair.reader);
            cases.push(case(pair.ext, &label, &bytes));
        }
    }
    if !quick {
        // Houdini counts values, not entries: 3 * 129^3 + 1 values pass, one more doesn't.
        let header = "Version 1\nFormat any\nType 3D\nFrom 0 1\nTo 0 1\nBlack 0\nWhite 1\n\
                      Length 129\nLUT:\n3D {\n";
        for values in [3 * CUBE_129 + 1, 3 * CUBE_129 + 2] {
            let mut bytes = repeated(header, "0 0 0\n", values / 3, "");
            for _ in 0..values % 3 {
                bytes.extend_from_slice(b"0\n");
            }
            bytes.extend_from_slice(b"}\n");
            let label = format!("hdl cap, {values} values");
            cases.push(case("lut", &label, &bytes));
        }
    }
    // The Truelight 1D cap: 300001 entries pass it, 300002 don't.
    let header = "# Truelight Cube v2.0\n# lutLength 300000\n# InputLUT\n";
    for count in [300_001, 300_002] {
        let bytes = repeated(header, "0 0 0\n", count, "# end\n");
        cases.push(case(
            "cub",
            &format!("truelight 1D cap, {count} entries"),
            &bytes,
        ));
    }
    // An Iridas 1D LUT of the largest size.
    let mut text = String::from("LUT_1D_SIZE 300000\n");
    for i in 0..300_000 {
        let x = i as f32 / 299_999.0;
        text.push_str(&format!("{x} {x} {x}\n"));
    }
    cases.push(case("cube", "iridas 1D of 300000", text.as_bytes()));
    cases
}

#[test]
fn entry_caps_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    check_file_processors(&cases(Tier::current()));
}
