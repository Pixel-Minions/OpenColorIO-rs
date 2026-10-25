// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Config.archive` and `isArchivable` (chunk 6.3g) against the wheel: a config read from a file
//! with a LUT on its search path, archived by each side into its own directory and read back
//! by the same side (`CreateFromFile` of the `.ocioz`), its text, its processors and the LUT
//! inside the archive; and the configs `isArchivable` refuses and `archive` raises for.
//!
//! Both sides run in a directory holding the files: the wheel's through the oracle's
//! with_files, the port's in a directory this binary's only test makes its working directory.
//! The archives' own bytes differ by D-6 (time stamps, deflate streams); what they hold
//! compares through the configs read back.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, compare_with, n, r, ra, s};

const CONFIG: &str = r#"ocio_profile_version: 2.1

search_path: luts

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: lut
    from_scene_reference: !<FileTransform> {src: lut.spi1d}
"#;

/// The files: the config and its LUT.
fn files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("config.ocio", CONFIG.as_bytes().to_vec()),
        (
            "luts/lut.spi1d",
            b"Version 1\nFrom 0 1\nLength 2\nComponents 1\n{\n0\n0.5\n}\n".to_vec(),
        ),
    ]
}

fn archive_script() -> Script {
    let mut sc = Script::new();
    let c = sc.call(n("OCIO.Config.CreateFromFile"), vec![s("config.ocio")]);
    sc.call(ra(c, "isArchivable"), vec![]);
    sc.call(ra(c, "archive"), vec![s("out.ocioz")]);
    let back = sc.call(n("OCIO.Config.CreateFromFile"), vec![s("out.ocioz")]);
    sc.call(ra(back, "serialize"), vec![]);
    sc.call(ra(back, "getSearchPath"), vec![]);
    sc.call(ra(back, "isArchivable"), vec![]);
    let p = sc.call(ra(back, "getProcessor"), vec![s("raw"), s("lut")]);
    sc.call(n("type"), vec![r(p)]);
    // Search paths `isArchivable` refuses (absolute, leaving the working directory, a context
    // variable) and takes.
    for path in ["/abs/luts", "../up", "sub/dir", "C:/abs", "$VAR/luts"] {
        sc.call(ra(c, "setSearchPath"), vec![s(path)]);
        sc.call(ra(c, "isArchivable"), vec![]);
    }
    sc.call(ra(c, "setSearchPath"), vec![s("/abs/luts")]);
    sc.raises(ra(c, "archive"), vec![s("refused.ocioz")]);
    let raw = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    sc.call(ra(raw, "isArchivable"), vec![]);
    sc.raises(ra(raw, "archive"), vec![s("raw.ocioz")]);
    sc.call(ra(raw, "setWorkingDir"), vec![s(".")]);
    sc.call(ra(raw, "isArchivable"), vec![]);
    sc.raises(ra(raw, "archive"), vec![s("raw.ocioz")]);
    // A file the stream can't open.
    let c2 = sc.call(n("OCIO.Config.CreateFromFile"), vec![s("config.ocio")]);
    sc.step(
        serde_json::json!({ "call": ra(c2, "archive"), "args": [s("no/such/dir/x.ocioz")] }),
        Expect::Value,
    );
    sc.raises(ra(c2, "archive"), vec![b(true)]);
    sc.raises(ra(c2, "archive"), vec![]);
    sc
}

/// Archiving behaves as the wheel's.
#[test]
fn archiving_as_the_wheels() {
    let files = files();
    let scripts = [archive_script()];
    let json: Vec<serde_json::Value> = scripts.iter().map(Script::json).collect();
    let theirs = common::wheel_with_files(&json, &files);

    let dir = std::env::temp_dir().join(format!("ocio-py-archive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, bytes) in &files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::env::set_current_dir(&dir).unwrap();
    compare_with(&scripts, &theirs);
    std::env::set_current_dir(std::env::temp_dir()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
