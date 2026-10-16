// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Number forms where the wheels' `NumberUtils::from_chars` differ (`std::from_chars` on
//! Windows, `strtof_l` on Linux: `0x` prefixes, hex floats, partial exponents, `inf` and
//! `nan` spellings, subnormals), in a value of each line reader, against the wheel of this
//! platform (`processor_ops`).

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

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

/// The entries of a 3D LUT of `size` per side, red fastest, a line each.
fn red_fastest(size: usize) -> String {
    let mut s = String::new();
    let d = (size - 1) as f32;
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                s.push_str(&format!(
                    "{} {} {}\n",
                    r as f32 / d * 0.9 + 0.01,
                    g as f32 / d * 0.8 + 0.02,
                    b as f32 / d * 0.7 + 0.03
                ));
            }
        }
    }
    s
}

/// Each number form in the first value of each reader's data.
fn cases() -> Vec<Case> {
    let mut c = Vec::new();
    for tok in [
        "0x", "0x1", "0X1P-1", "-0x", "0x.8", "1e", "-.e1", "infinit", "nanx", "nan(", "1e-40",
        "1e-46", "1e39", "-0x1p-2",
    ] {
        let d = red_fastest(2).replacen("0.01", tok, 1);
        c.push(case(
            "cube",
            &format!("iridas data {tok}"),
            format!("LUT_3D_SIZE 2\n{d}").as_bytes(),
        ));
        c.push(case(
            "cube",
            &format!("resolve data {tok}"),
            format!("LUT_3D_SIZE 2\nLUT_3D_INPUT_RANGE 0 1\n{d}").as_bytes(),
        ));
        c.push(case(
            "itx",
            &format!("itx data {tok}"),
            format!("LUT_3D_SIZE 2\n{d}").as_bytes(),
        ));
        let spi = format!(
            "SPILUT 1.0\n3 3\n2 2 2\n0 0 0 {tok} 0 0\n0 0 1 0 0 1\n0 1 0 0 1 0\n0 1 1 0 1 1\n\
             1 0 0 1 0 0\n1 0 1 1 0 1\n1 1 0 1 1 0\n1 1 1 1 1 1\n"
        );
        c.push(case("spi3d", &format!("spi3d data {tok}"), spi.as_bytes()));
        let vf = format!("#Inventor V2.1 ascii\ngrid_size 2 2 2\ndata\n{d}");
        c.push(case("vf", &format!("vf data {tok}"), vf.as_bytes()));
        let tl = format!("# Truelight Cube v2.0\n# width 2 2 2\n# Cube\n{d}# end\n");
        c.push(case("cub", &format!("truelight data {tok}"), tl.as_bytes()));
        let hdl = format!(
            "Version 1\nFormat any\nType C\nFrom 0 1\nTo 0 1\nBlack 0\nWhite 1\nLength 3\nLUT:\n\
             RGB {{\n0\n{tok}\n1\n}}\n"
        );
        c.push(case("lut", &format!("hdl data {tok}"), hdl.as_bytes()));
        let csp = format!(
            "CSPLUTV100\n1D\n\n3\n0 0.5 1\n0 {tok} 1\n3\n0 0.5 1\n0 0.25 1\n3\n0 0.5 1\n\
             0 0.5 1\n\n2\n0 0 0\n1 1 1\n"
        );
        c.push(case("csp", &format!("csp prelut {tok}"), csp.as_bytes()));
    }
    c
}

#[test]
fn number_forms_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    check_file_processors(&cases());
}
