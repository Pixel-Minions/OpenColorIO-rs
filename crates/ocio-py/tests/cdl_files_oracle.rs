// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `CDLTransform.CreateFromFile` and `CreateGroupFromFile` against the wheel, on upstream's CC,
//! CCC and CDL test files (tests/data/files @ v2.5.2) and on files that aren't there or aren't
//! CDLs: the transforms and groups they give (their values, ids, descriptions, reprs), each id
//! form, and their errors.
//!
//! Both sides read the files by relative paths from a directory holding them: the wheel's
//! through the oracle's with_files, the port's from a directory this binary's only test makes
//! its working directory.
//!
//! What still differs (I-174): the wheel returns the file cache's own transforms, so a second
//! read of a CDL is the same Python object as the first; the port's are new objects.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, compare_with, i, n, r, ra, s};
use serde_json::Value;

/// The files, with upstream's test files' contents.
fn files() -> Vec<(&'static str, Vec<u8>)> {
    let data = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    let mut files: Vec<(&'static str, Vec<u8>)> = [
        "cdl_test1.cc",
        "cdl_test1.ccc",
        "cdl_test1.cdl",
        "cdl_test2.cc",
        "cdl_test_cc_file_with_extension.ccc",
    ]
    .into_iter()
    .map(|f| (f, std::fs::read(data.join(f)).unwrap()))
    .collect();
    files.push(("not_a_cdl.cc", b"<ColorCorrection>".to_vec()));
    files.push((
        "lut.spi1d",
        b"Version 1\nFrom 0 1\nLength 2\nComponents 1\n{\n0\n1\n}\n".to_vec(),
    ));
    files
}

const ALL: [&str; 9] = [
    "getSlope",
    "getOffset",
    "getPower",
    "getSat",
    "getID",
    "getFirstSOPDescription",
    "getStyle",
    "getDirection",
    "__repr__",
];

/// Each (file, id) once: a second read would be the first's object in the wheel (I-174).
fn from_file_script() -> Script {
    let mut sc = Script::new();
    for (file, id, ok) in [
        ("cdl_test1.cc", "", true),
        ("cdl_test2.cc", "foo", false),
        ("cdl_test1.ccc", "cc0003", true),
        ("cdl_test1.ccc", "3", true),
        ("cdl_test1.ccc", "0", true),
        ("cdl_test1.ccc", "-1", false),
        ("cdl_test1.ccc", "99", false),
        ("cdl_test1.ccc", "nope", false),
        ("cdl_test1.cdl", "cc0001", true),
        ("cdl_test_cc_file_with_extension.ccc", "", true),
        ("missing.cc", "", false),
        ("not_a_cdl.cc", "", false),
        ("lut.spi1d", "", false),
        ("", "x", false),
    ] {
        let step = sc.step(
            serde_json::json!({ "call": n("OCIO.CDLTransform.CreateFromFile"), "args": [s(file), s(id)], "getters": ALL }),
            if ok { Expect::Value } else { Expect::Raised },
        );
        // The CDL file's warnings, logged on its first read in a process (the file cache keeps
        // it): the wheel's process is new, the port's may have read it already.
        if file == "cdl_test1.cdl" {
            sc.unlog(step);
        }
    }
    sc
}

fn group_from_file_script() -> Script {
    let mut sc = Script::new();
    for (file, ok) in [
        ("cdl_test1.ccc", true),
        ("cdl_test1.cdl", true),
        ("cdl_test2.cc", true),
        ("missing.ccc", false),
        ("lut.spi1d", false),
    ] {
        let g = sc.step(
            serde_json::json!({ "call": n("OCIO.CDLTransform.CreateGroupFromFile"), "args": [s(file)], "getters": ["__len__", "getDirection", "__repr__"] }),
            if ok { Expect::Value } else { Expect::Raised },
        );
        if ok {
            if file == "cdl_test1.cdl" {
                sc.unlog(g);
            }
            sc.call(n("type"), vec![r(g)]);
            sc.step(
                serde_json::json!({ "call": ra(g, "__getitem__"), "args": [i(0)], "getters": ALL }),
                Expect::Value,
            );
            sc.call(n("len"), vec![r(g)]);
        }
    }
    sc
}

/// Reading CDLs from files behaves as the wheel's, but for I-174's identity.
#[test]
fn reading_cdl_files_as_the_wheel() {
    let files = files();
    let scripts = [from_file_script(), group_from_file_script()];
    let json: Vec<Value> = scripts.iter().map(Script::json).collect();
    let theirs = common::wheel_with_files(&json, &files);

    let dir = std::env::temp_dir().join(format!("ocio-py-cdl-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in &files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    std::env::set_current_dir(&dir).unwrap();
    compare_with(&scripts, &theirs);

    // I-174: a second read of the same CDL is the first's object in the wheel, a new one in the
    // port; and an edit of the first doesn't reach the second in the port.
    let mut sc = Script::new();
    let first = sc.call(
        n("OCIO.CDLTransform.CreateFromFile"),
        vec![s("cdl_test1.ccc"), s("cc0003")],
    );
    sc.call(ra(first, "setSat"), vec![common::script::f(0.25)]);
    let second = sc.call(
        n("OCIO.CDLTransform.CreateFromFile"),
        vec![s("cdl_test1.ccc"), s("cc0003")],
    );
    let sat = sc.call(ra(second, "getSat"), vec![]);
    let theirs = common::wheel_with_files(&[sc.json()], &files).remove(0);
    let ours = common::port(&sc.json());
    std::env::set_current_dir(std::env::temp_dir()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        theirs["steps"][second]["same_as"],
        serde_json::json!([first])
    );
    assert_eq!(ours["steps"][second]["same_as"], serde_json::json!([]));
    assert_eq!(
        theirs["steps"][sat]["value"]["float"],
        common::script::f(0.25)["float"]
    );
    assert_ne!(
        ours["steps"][sat]["value"]["float"],
        theirs["steps"][sat]["value"]["float"]
    );
    assert_eq!(ours["steps"][first], theirs["steps"][first]);
}
