// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The module functions of `bindPyConfig` bound so far (chunk 6.3f) against the wheel (their
//! surface entries: py_types_oracle.rs, with the module's other functions):
//! - `ResolveConfigPath` through the same step scripts on both sides: the special names in
//!   any case, a URI inside a path, a NUL, `None` (a null pointer: the empty string), bytes,
//!   and what the caster refuses, including keywords (an unnamed parameter takes none);
//! - `ExtractOCIOZArchive`: upstream's test archives extracted through the wheel (the oracle's
//!   `ocioz extract`) and through the port's module into a directory of its own, the same
//!   files with the same contents and the same directories; its errors through the scripts.
//!
//! And, without the wheel, whose process dies there, `None` for either path is the port's
//! error (U-131).
#![cfg(feature = "python")]

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::script::{Expect, Script, by, compare, i, n, none, s};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::paths::upstream_dir;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::json;

fn functions_script() -> Script {
    let mut sc = Script::new();
    for path in [
        "ocio://default",
        "ocio://DEFAULT",
        "ocio://cg-config-latest",
        "ocio://Studio-Config-Latest",
        "OCIO://default",
        "x/ocio://default",
        "ocio://studio-config-latest\u{0}tail",
        "ocio://cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
        "ocio://nope",
        "/some/config.ocio",
        "caf\u{e9}",
        "",
    ] {
        sc.call(n("OCIO.ResolveConfigPath"), vec![s(path)]);
    }
    sc.call(n("OCIO.ResolveConfigPath"), vec![none()]);
    sc.call(n("OCIO.ResolveConfigPath"), vec![by(b"ocio://default")]);
    sc.raises(n("OCIO.ResolveConfigPath"), vec![i(1)]);
    sc.raises(n("OCIO.ResolveConfigPath"), vec![]);
    sc.raises(n("OCIO.ResolveConfigPath"), vec![s("a"), s("b")]);
    sc.call_kw(
        n("OCIO.ResolveConfigPath"),
        vec![],
        vec![("arg0", s("ocio://default"))],
        Expect::Raised,
    );
    // ExtractOCIOZArchive's errors (nothing is written).
    for path in ["no/such/archive.ocioz", ""] {
        sc.raises(n("OCIO.ExtractOCIOZArchive"), vec![s(path), s("unused")]);
    }
    sc.raises(n("OCIO.ExtractOCIOZArchive"), vec![s("a")]);
    sc.raises(n("OCIO.ExtractOCIOZArchive"), vec![i(1), s("a")]);
    sc.call_kw(
        n("OCIO.ExtractOCIOZArchive"),
        vec![],
        vec![("arg0", s("a")), ("arg1", s("b"))],
        Expect::Raised,
    );
    sc
}

/// The functions behave as the wheel's.
#[test]
fn the_functions_behave_as_the_wheels() {
    compare(&[functions_script()]);
}

/// A tree of what an extraction wrote: each relative path ("/" separated) and its contents
/// (`None` for a directory).
type Tree = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

fn read_tree(root: &Path) -> Tree {
    let mut out = Tree::new();
    let mut stack = vec![(root.to_path_buf(), Vec::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let mut relative: Vec<u8> = prefix.clone();
            if !relative.is_empty() {
                relative.push(b'/');
            }
            relative.extend_from_slice(entry.file_name().to_str().unwrap().as_bytes());
            if entry.file_type().unwrap().is_dir() {
                stack.push((entry.path(), relative.clone()));
                out.insert(relative, None);
            } else {
                out.insert(relative, Some(std::fs::read(entry.path()).unwrap()));
            }
        }
    }
    out
}

/// A new, empty directory of this test under the target directory.
fn scratch(name: &str) -> PathBuf {
    let dir = ocio_testkit::paths::target_dir()
        .join("ocio-py-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Upstream's test archives extract as the wheel extracts them.
#[test]
fn extraction_writes_what_the_wheels_writes() {
    let archives = upstream_dir().join("tests/data/files/configs/context_test1");
    for name in ["context_test1_windows.ocioz", "context_test1_linux.ocioz"] {
        let archive = archives.join(name);
        let archive_text = archive.to_str().unwrap().to_string();
        let response = Oracle::get().call(
            "ocioz",
            json!({ "extract": { "archive": archive_text } }),
            &[],
        );
        let result = &response.result;
        assert!(result.get("exception").is_none(), "{name}: {result}");
        let mut theirs = Tree::new();
        for f in result["files"].as_array().unwrap() {
            let contents = f["contents"]
                .as_u64()
                .map(|k| response.blobs[k as usize].clone());
            assert!(contents.is_some(), "{name}: {f}");
            theirs.insert(bytes(&f["path"]), contents);
        }
        for d in result["dirs"].as_array().unwrap() {
            theirs.insert(bytes(&d["path"]), None);
        }
        assert!(result["links"].as_array().unwrap().is_empty(), "{name}");
        assert!(theirs.len() > 5, "{name}: {theirs:?}");

        let root = scratch(name);
        let destination = root.join("extracted");
        common::start_python();
        Python::attach(|py| {
            let globals = PyDict::new(py);
            globals.set_item("archive", &archive_text).unwrap();
            globals
                .set_item("destination", destination.to_str().unwrap())
                .unwrap();
            py.run(
                c"import PyOpenColorIO as OCIO\nOCIO.ExtractOCIOZArchive(archive, destination)",
                Some(&globals),
                None,
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        });
        let ours = read_tree(&destination);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(ours, theirs, "{name}");
    }
}

/// Where the wheel's process dies, the port's error: `None` for either path (U-131).
#[test]
fn where_the_wheel_dies_the_port_raises() {
    common::start_python();
    Python::attach(|py| {
        for code in [
            c"OCIO.ExtractOCIOZArchive(None, 'unused')",
            c"OCIO.ExtractOCIOZArchive('no/such/archive.ocioz', None)",
            c"OCIO.ExtractOCIOZArchive(None, None)",
        ] {
            let globals = PyDict::new(py);
            py.run(c"import PyOpenColorIO as OCIO", Some(&globals), None)
                .unwrap();
            let err = py.run(code, Some(&globals), None).expect_err("U-131");
            let text = err.to_string();
            assert!(
                text.starts_with("ValueError") && text.contains("docs/improvements.md, U-131"),
                "{code:?}: {text}"
            );
        }
        // The control: the same call with paths reaches the library's error.
        let globals = PyDict::new(py);
        py.run(c"import PyOpenColorIO as OCIO", Some(&globals), None)
            .unwrap();
        let err = py
            .run(
                c"OCIO.ExtractOCIOZArchive('no/such/archive.ocioz', 'unused')",
                Some(&globals),
                None,
            )
            .expect_err("control");
        assert!(
            err.to_string()
                .contains("Could not open no/such/archive.ocioz for reading."),
            "{err}"
        );
    });
}
