// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `ocioz` command (`oracle/ocio_oracle/ocioz_api.py`), against the wheel, the file
//! system and upstream's test data: archiving `tests/data/files/configs/context_test1` gives the
//! config first and then every file of its directory the archiver takes, each entry's contents
//! the file's; the entries are the archive's, in its order and with its flags; extracting that
//! archive (through `with_files`) writes the same files, in the directories they need;
//! extracting reports directories, links, times and attributes, and what was written before a
//! failure; refusals report their stage; the requests it can't read are refused.

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
    let result = &extracted.result;
    let mut written = BTreeMap::new();
    for f in result["files"].as_array().unwrap() {
        let contents = &extracted.blobs[f["contents"].as_u64().unwrap() as usize];
        let path = String::from_utf8(bytes(&f["path"])).unwrap();
        written.insert(path, contents.clone());
    }
    let config_text = &response.blobs[entries[0]["contents"].as_u64().unwrap() as usize];
    assert_eq!(written.remove("config.ocio").as_ref(), Some(config_text));
    assert_eq!(written, archived);
    // The directories are those the files are in (the archive has no empty one), sorted.
    let mut parents = std::collections::BTreeSet::new();
    for name in archived.keys() {
        let mut path = name.as_str();
        while let Some(i) = path.rfind('/') {
            path = &path[..i];
            parents.insert(path.as_bytes().to_vec());
        }
    }
    assert_eq!(
        paths(result, "dirs"),
        parents.into_iter().collect::<Vec<_>>(),
        "{result}"
    );
    assert_eq!(result["links"], json!([]), "{result}");
}

/// CRC-32 (ISO-HDLC, as zip uses) of the data this test archives.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A stored entry: its name, data, made-by version, external attributes, extra field and
/// CRC.
struct Entry<'a> {
    name: &'a [u8],
    data: &'a [u8],
    madeby: u16,
    attributes: u32,
    extra: Vec<u8>,
    crc: u32,
}

fn entry<'a>(name: &'a [u8], data: &'a [u8]) -> Entry<'a> {
    Entry {
        name,
        data,
        madeby: 20,
        attributes: 0,
        extra: Vec::new(),
        crc: crc32(data),
    }
}

/// An NTFS extra field (PKWARE APPNOTE 4.5.5) with the same three times.
fn ntfs_times(time: u64) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [0x000au16, 32, 0, 0, 1, 24] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for _ in 0..3 {
        out.extend_from_slice(&time.to_le_bytes());
    }
    out
}

/// The zip archive of `entries`, each stored, its DOS date 2025-01-01 12:00.
fn zip(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let offset = out.len() as u32;
        let sizes = [e.crc, e.data.len() as u32, e.data.len() as u32];
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for v in [20u16, 0, 0, 0x6000, 0x5a21] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in sizes {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.name.len() as u16, e.extra.len() as u16] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(e.name);
        out.extend_from_slice(&e.extra);
        out.extend_from_slice(e.data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for v in [e.madeby, 20, 0, 0, 0x6000, 0x5a21] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in sizes {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.name.len() as u16, e.extra.len() as u16, 0, 0, 0] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.attributes, offset] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        central.extend_from_slice(e.name);
        central.extend_from_slice(&e.extra);
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    let n = entries.len() as u16;
    for v in [0u16, 0, n, n] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [central.len() as u32, cd_offset] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// The wheel extracting `archive`: its result and blobs.
fn extract(archive: &[u8]) -> (Value, Vec<Vec<u8>>) {
    let response = Oracle::get().call(
        "with_files",
        json!({"files": {"a.ocioz": {"blob": 0}}, "file_blobs": 1, "command": "ocioz",
               "args": {"extract": {"archive": "$FILES/a.ocioz"}}}),
        &[archive],
    );
    (response.result, response.blobs)
}

/// The paths of a listing ("files", "dirs" or "links").
fn paths(result: &Value, key: &str) -> Vec<Vec<u8>> {
    result[key]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"))
        .iter()
        .map(|e| bytes(&e["path"]))
        .collect()
}

/// Extracting reports directories, empty ones included, sorted by their paths' bytes; and
/// each file's modification time and attributes as minizip-ng sets them: the entry's NTFS
/// times (mz_zip.c:344-372 @ 4.0.10; 1,000,000,000 seconds after 1970 here), and its Windows
/// attributes as they are on Windows (`SetFileAttributesW`, mz_os_win32.c:320-335) and as
/// `mz_zip_attrib_win32_to_posix` gives them on Linux (mz_zip.c, read-only 0444, otherwise
/// 0666).
#[test]
fn extract_reports_directories_times_and_attributes() {
    let unix = 1_000_000_000u64;
    let ntfs = unix * 10_000_000 + 116_444_736_000_000_000;
    let windows_host = 10 << 8 | 20;
    let archive = zip(&[
        entry(b"zdir/", b""),
        entry(b"adir/sub/", b""),
        Entry {
            madeby: windows_host,
            attributes: 0x21,
            extra: ntfs_times(ntfs),
            ..entry(b"adir/ro.txt", b"read only")
        },
        Entry {
            madeby: windows_host,
            attributes: 0x20,
            extra: ntfs_times(ntfs),
            ..entry(b"rw.txt", b"read and write")
        },
    ]);
    let (result, blobs) = extract(&archive);
    assert_eq!(
        paths(&result, "dirs"),
        [&b"adir"[..], b"adir/sub", b"zdir"],
        "{result}"
    );
    assert_eq!(
        paths(&result, "files"),
        [&b"adir/ro.txt"[..], b"rw.txt"],
        "{result}"
    );
    assert_eq!(result["links"], json!([]), "{result}");
    let files = result["files"].as_array().unwrap();
    for (f, contents) in files.iter().zip([&b"read only"[..], b"read and write"]) {
        assert_eq!(blobs[f["contents"].as_u64().unwrap() as usize], contents);
        assert_eq!(
            f["mtime_ns"].as_u64(),
            Some(unix * 1_000_000_000),
            "{result}"
        );
    }
    if cfg!(windows) {
        assert_eq!(files[0]["attributes"], 0x21, "{result}");
        assert_eq!(files[1]["attributes"], 0x20, "{result}");
        for d in result["dirs"].as_array().unwrap() {
            assert_eq!(d["attributes"].as_u64().unwrap() & 0x10, 0x10, "{result}");
        }
    } else {
        assert_eq!(files[0]["mode"], 0o444, "{result}");
        assert_eq!(files[1]["mode"], 0o666, "{result}");
        for d in result["dirs"].as_array().unwrap() {
            assert!(d["mode"].is_u64(), "{result}");
        }
    }
}

/// A symbolic link is reported with its target, and isn't read through: minizip-ng makes one
/// from an entry of a Unix host whose mode is a link's, the entry's contents its target
/// (mz_zip_rw.c:691-724 @ 4.0.10). On Windows a link needs a privilege the process may not
/// have, and the extraction then fails.
#[test]
fn extract_reports_links() {
    let unix_host = 3 << 8 | 20;
    let archive = zip(&[
        entry(b"t.txt", b"target"),
        Entry {
            madeby: unix_host,
            attributes: 0o120_777 << 16,
            ..entry(b"l.txt", b"t.txt")
        },
        Entry {
            madeby: unix_host,
            attributes: 0o120_777 << 16,
            ..entry(b"d", b"nowhere/")
        },
    ]);
    let (result, _) = extract(&archive);
    if cfg!(windows) && result.get("exception").is_some() {
        assert_eq!(result["stage"], "extract", "{result}");
        assert_eq!(paths(&result, "files"), [b"t.txt"], "{result}");
        return;
    }
    assert_eq!(paths(&result, "files"), [b"t.txt"], "{result}");
    let links: Vec<(Vec<u8>, Vec<u8>)> = result["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (bytes(&l["path"]), bytes(&l["target"])))
        .collect();
    assert_eq!(
        links,
        [
            (b"d".to_vec(), b"nowhere/".to_vec()),
            (b"l.txt".to_vec(), b"t.txt".to_vec())
        ],
        "{result}"
    );
}

/// Where extracting fails partway, what it wrote before is reported with the exception: here
/// the first entry, and the second, whose CRC is wrong (minizip-ng checks it as it closes the
/// entry, after writing its data).
#[test]
fn extract_failure_reports_what_was_written() {
    let archive = zip(&[
        entry(b"ok.txt", b"ok"),
        Entry {
            crc: 0,
            ..entry(b"bad.txt", b"bad")
        },
        entry(b"never.txt", b"never"),
    ]);
    let (result, _) = extract(&archive);
    assert_eq!(result["stage"], "extract", "{result}");
    assert!(result["exception"].is_object(), "{result}");
    assert_eq!(
        paths(&result, "files"),
        [&b"bad.txt"[..], b"ok.txt"],
        "{result}"
    );
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
