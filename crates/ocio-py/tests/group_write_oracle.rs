// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `GroupTransform.write` to a file against the wheel (`std::ofstream`, PyGroupTransform.cpp:
//! 81-101 @ v2.5.2): the file's bytes, read back with `numpy.fromfile` (the step interpreter
//! has no `open`), as the wheel writes them (a text-mode stream: CR LF line ends on Windows);
//! a format that fails leaves the file created and empty; a file that can't be opened makes no
//! error; a file name as `bytes`, cut at a NUL, given by keyword.
//!
//! Each side writes in a directory of its own: the oracle runs each script in a new one, and
//! this binary's only test makes one and works in it (the process's working directory, which
//! no other test of the binary shares).
//!
//! And, without the wheel (U-113): a file name that isn't ASCII is the port's error on
//! Windows.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, by, compare, fl, i, l, n, none, r, ra, s};

/// `numpy.fromfile(name, dtype="u1")`: the file's bytes.
fn read(sc: &mut Script, name: &str, expect: Expect) -> usize {
    sc.call_kw(
        n("numpy.fromfile"),
        vec![s(name)],
        vec![("dtype", s("u1"))],
        expect,
    )
}

fn script() -> Script {
    let mut sc = Script::new();
    sc.call(n("OCIO.UnsetEnvVariable"), vec![s("OCIO")]);
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(m, "setOffset"), vec![fl(&[0.1, 0.2, 0.3, 0.0])]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m)])]);
    let first = sc.call(
        ra(g, "write"),
        vec![s("Color Transform Format"), s("out.ctf")],
    );
    sc.unlog(first);
    read(&mut sc, "out.ctf", Expect::Value);
    sc.call_kw(
        ra(g, "write"),
        vec![],
        vec![
            ("fileName", s("out.clf")),
            ("formatName", s("Academy/ASC Common LUT Format")),
        ],
        Expect::Value,
    );
    read(&mut sc, "out.clf", Expect::Value);
    // A failing format: the file is created, empty; then over a file written before.
    sc.raises(ra(g, "write"), vec![s("nope"), s("bad.ctf")]);
    read(&mut sc, "bad.ctf", Expect::Value);
    sc.raises(ra(g, "write"), vec![s("ColorDecisionList"), s("out.ctf")]);
    read(&mut sc, "out.ctf", Expect::Value);
    // A file that can't be opened.
    sc.call(
        ra(g, "write"),
        vec![s("Color Transform Format"), s("missing/out.ctf")],
    );
    read(&mut sc, "missing/out.ctf", Expect::Raised);
    // A name as bytes, a name cut at a NUL.
    sc.call(
        ra(g, "write"),
        vec![s("Color Transform Format"), by(b"bytes.ctf")],
    );
    read(&mut sc, "bytes.ctf", Expect::Value);
    sc.call(
        ra(g, "write"),
        vec![s("Color Transform Format"), s("cut.ctf\u{0}tail")],
    );
    read(&mut sc, "cut.ctf", Expect::Value);
    // Refused arguments.
    sc.raises(
        ra(g, "write"),
        vec![s("Color Transform Format"), s("x.ctf"), s("y")],
    );
    sc.raises(ra(g, "write"), vec![s("Color Transform Format"), i(1)]);
    // A null child and an inverse group.
    sc.call(ra(g, "appendTransform"), vec![none()]);
    sc.call(ra(g, "setDirection"), vec![n("OCIO.TRANSFORM_DIR_INVERSE")]);
    sc.call(
        ra(g, "write"),
        vec![s("Color Transform Format"), s("holes.ctf")],
    );
    read(&mut sc, "holes.ctf", Expect::Value);
    // A name that isn't ASCII: the bytes on Linux (U-113 on Windows).
    if cfg!(not(windows)) {
        sc.call(
            ra(g, "write"),
            vec![s("Color Transform Format"), s("caf\u{e9}.ctf")],
        );
        // Read back by its bytes: the embedded interpreter of the port's side may encode a str
        // path as ASCII (its file system encoding in a C locale), the wheel's runs in UTF-8 mode.
        sc.call_kw(
            n("numpy.fromfile"),
            vec![by("caf\u{e9}.ctf".as_bytes())],
            vec![("dtype", s("u1"))],
            Expect::Value,
        );
    }
    sc
}

/// Writing to files behaves as the wheel's, and on Windows a name that isn't ASCII is the
/// port's error (U-113).
#[test]
fn write_to_a_file_as_the_wheel() {
    let dir = std::env::temp_dir().join(format!("ocio-py-group-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();
    compare(&[script()]);
    if cfg!(windows) {
        let mut sc = Script::new();
        let g = sc.call(n("OCIO.GroupTransform"), vec![]);
        sc.raises(
            ra(g, "write"),
            vec![s("Color Transform Format"), s("caf\u{e9}.ctf")],
        );
        let ours = common::port(&sc.json());
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &ours["steps"][1]["raised"]["message"],
        ))
        .unwrap();
        assert!(message.contains("U-113"), "{ours}");
        assert!(!dir.join("caf\u{e9}.ctf").exists());
    }
    std::env::set_current_dir(std::env::temp_dir()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
