// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `ocioz` command (`oracle/ocio_oracle/ocioz_api.py`), against the wheel, the file
//! system and upstream's test data: archiving `tests/data/files/configs/context_test1` gives the
//! config first and then every file of its directory the archiver takes, each entry's contents
//! the file's; the entries are the archive's, in its order and with its flags; extracting that
//! archive (through `with_files`) writes the same files; refusals
//! report their stage; the requests it can't read are refused.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::paths::upstream_dir;
use serde_json::{Value, json};

fn context_test1() -> PathBuf {
    upstream_dir().join("tests/data/files/configs/context_test1")
}

/// Every file under `dir`, by relative path with `/`.
fn files_under(dir: &PathBuf) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// `archive_config_and_compare_to_original`'s config (tests/cpu/OCIOZArchive_tests.cpp @
/// v2.5.2) archived: the first entry is `config.ocio`, the other entries are files of the
/// config's directory (upstream's LUTs, not its `.ocioz` archives), each with the file's
/// contents; the archive extracts to the same files.
#[test]
fn archive_and_extract_context_test1() {
    let config = context_test1().join("config.ocio");
    let response = Oracle::get().call(
        "ocioz",
        json!({"archive": {"config": {"file": config.to_string_lossy()}}}),
        &[],
    );
    let result = &response.result;
    let entries = result["entries"].as_array().unwrap();
    assert_eq!(bytes(&entries[0]["name"]), b"config.ocio", "{result}");
    let on_disk = files_under(&context_test1());
    let mut archived = BTreeMap::new();
    for entry in &entries[1..] {
        let name = String::from_utf8(bytes(&entry["name"])).unwrap();
        let contents = &response.blobs[entry["contents"].as_u64().unwrap() as usize];
        assert_eq!(Some(contents), on_disk.get(&name), "{name}");
        assert_eq!(entry["size"].as_u64().unwrap() as usize, contents.len());
        archived.insert(name, contents.clone());
    }
    assert!(
        archived.contains_key("shot3/subdir/lut3.clf"),
        "{archived:?}"
    );
    assert!(
        !archived.keys().any(|n| n.ends_with(".ocioz")),
        "{archived:?}"
    );
    let archive = &response.blobs[result["archive"].as_u64().unwrap() as usize];
    assert_eq!(&archive[..2], b"PK");

    let extracted = Oracle::get().call(
        "with_files",
        json!({"files": {"a.ocioz": {"blob": 0}}, "file_blobs": 1, "command": "ocioz",
               "args": {"extract": {"archive": "$FILES/a.ocioz"}}}),
        &[archive],
    );
    let files = extracted.result["files"].as_array().unwrap();
    let mut written = BTreeMap::new();
    for f in files {
        let contents = &extracted.blobs[f["contents"].as_u64().unwrap() as usize];
        written.insert(f["path"].as_str().unwrap().to_string(), contents.clone());
    }
    let config_text = &response.blobs[entries[0]["contents"].as_u64().unwrap() as usize];
    assert_eq!(written.remove("config.ocio").as_ref(), Some(config_text));
    assert_eq!(written, archived);
}

/// The central directory of a zip archive: each entry's name and general purpose flags, in
/// the archive's order (PKWARE APPNOTE 4.3.12, 4.3.16).
fn central_directory(archive: &[u8]) -> Vec<(Vec<u8>, u16)> {
    let u16_at = |i: usize| u16::from_le_bytes([archive[i], archive[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(archive[i..i + 4].try_into().unwrap());
    let end = (0..=archive.len() - 22)
        .rev()
        .find(|&i| u32_at(i) == 0x0605_4b50)
        .expect("an end of central directory record");
    let count = usize::from(u16_at(end + 10));
    let mut at = u32_at(end + 16) as usize;
    let mut entries = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(at), 0x0201_4b50, "a central directory header");
        let flags = u16_at(at + 8);
        let name_len = usize::from(u16_at(at + 28));
        let extra_len = usize::from(u16_at(at + 30));
        let comment_len = usize::from(u16_at(at + 32));
        entries.push((archive[at + 46..at + 46 + name_len].to_vec(), flags));
        at += 46 + name_len + extra_len + comment_len;
    }
    entries
}

/// The entries are the archive's own, in its order and with its flags (read from the archive's
/// central directory), and that order isn't the names' sorted order here: the config comes
/// first, before `a.spi1d`.
#[test]
fn entries_are_the_archives_own() {
    let config = "ocio_profile_version: 2\nsearch_path: .\nroles: {default: raw}\n\
                  colorspaces:\n  - !<ColorSpace> {name: raw}\n";
    let response = Oracle::get().call(
        "with_files",
        json!({"files": {"config.ocio": config, "a.spi1d": "Version 1\n"},
               "command": "ocioz",
               "args": {"archive": {"config": {"file": "$FILES/config.ocio"}}}}),
        &[],
    );
    let result = &response.result;
    let entries: Vec<(Vec<u8>, u16)> = result["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"))
        .iter()
        .map(|e| (bytes(&e["name"]), e["flag_bits"].as_u64().unwrap() as u16))
        .collect();
    let archive = &response.blobs[result["archive"].as_u64().unwrap() as usize];
    assert_eq!(entries, central_directory(archive), "{result}");
    let names: Vec<&Vec<u8>> = entries.iter().map(|(n, _)| n).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_ne!(names, sorted, "{result}");
}

/// An archive that isn't one, and a config that can't be archived, are refused at their stage.
#[test]
fn refusals_report_their_stage() {
    let not_zip = Oracle::get()
        .call(
            "with_files",
            json!({"files": {"a.ocioz": "not a zip"}, "command": "ocioz",
                   "args": {"extract": {"archive": "$FILES/a.ocioz"}}}),
            &[],
        )
        .result;
    assert_eq!(not_zip["stage"], "extract", "{not_zip}");
    let raw = Oracle::get()
        .call("ocioz", json!({"archive": {"config": Value::Null}}), &[])
        .result;
    assert_eq!(raw["stage"], "archive", "{raw}");
}

/// Requests it can't read are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({}),
        json!({"archive": {}, "extract": {"archive": "a"}}),
        json!({"archive": {"config": null, "extra": 1}}),
        json!({"extract": {"archive": 1}}),
        json!({"other": 1}),
    ] {
        assert!(
            Oracle::get().try_call("ocioz", args.clone(), &[]).is_err(),
            "{args}"
        );
    }
}
