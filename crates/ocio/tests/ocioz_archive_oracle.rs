// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Config::archive` against the wheel, live on this platform (WP 4.9b): the oracle's `ocioz`
//! archives a config, and the port archives the same config, read from the same file, with the
//! same files around it. The archives are compared entry by entry, in order (owner decision
//! P4-3): each entry's name, method, flags, made-by system and version, version needed,
//! attributes, extra field, CRC, size and contents. The time stamps (the DOS dates and the NTFS
//! times of the extra fields) and the compressed bytes and sizes aren't compared: the entries
//! carry the time they're written (`time(NULL)`) and the times the files have when they're
//! read, and the deflate streams are flate2's (deviation D-6). The local headers and data
//! descriptors are compared too, but for those.

use std::path::{Path, PathBuf};

use ocio::Config;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::paths::upstream_dir;
use serde_json::{Value, json};

/// An entry as Python's `zipfile` reports it from the central directory, with its contents.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    name: Vec<u8>,
    method: u64,
    flag_bits: u64,
    create_system: u64,
    create_version: u64,
    extract_version: u64,
    internal_attr: u64,
    external_attr: u64,
    /// The extra field, its NTFS times zeroed.
    extra: Vec<u8>,
    crc: u64,
    size: u64,
    contents: Vec<u8>,
}

/// `extra` with the times of its NTFS field (0x000a, attribute 1 of 24 bytes) zeroed.
fn without_times(extra: &[u8]) -> Vec<u8> {
    let mut out = extra.to_vec();
    let mut p = 0;
    while p + 4 <= out.len() {
        let field_type = u16::from_le_bytes([out[p], out[p + 1]]);
        let length = usize::from(u16::from_le_bytes([out[p + 2], out[p + 3]]));
        if field_type == 0x000a && length == 32 && p + 4 + 32 <= out.len() {
            for b in &mut out[p + 12..p + 36] {
                *b = 0;
            }
        }
        p += 4 + length;
    }
    out
}

/// The wheel's entries.
fn wheel_entries(result: &Value, blobs: &[Vec<u8>]) -> Vec<Entry> {
    result["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| Entry {
            name: bytes(&e["name"]),
            method: e["method"].as_u64().unwrap(),
            flag_bits: e["flag_bits"].as_u64().unwrap(),
            create_system: e["create_system"].as_u64().unwrap(),
            create_version: e["create_version"].as_u64().unwrap(),
            extract_version: e["extract_version"].as_u64().unwrap(),
            internal_attr: e["internal_attr"].as_u64().unwrap(),
            external_attr: e["external_attr"].as_u64().unwrap(),
            extra: without_times(&hex_decode(e["extra"].as_str().unwrap())),
            crc: e["crc"].as_u64().unwrap(),
            size: e["size"].as_u64().unwrap(),
            contents: blobs[e["contents"].as_u64().unwrap() as usize].clone(),
        })
        .collect()
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// Raw deflate `data`, inflated.
fn inflate(data: &[u8]) -> Vec<u8> {
    let mut d = flate2::Decompress::new(false);
    let mut out = Vec::with_capacity(data.len() * 8 + 1024);
    loop {
        let before = d.total_in() as usize;
        let status = d
            .decompress_vec(&data[before..], &mut out, flate2::FlushDecompress::Finish)
            .unwrap();
        if status == flate2::Status::StreamEnd {
            return out;
        }
        out.reserve(out.capacity());
    }
}

/// What an archive's local headers and data descriptors say of its entries, in order:
/// each local header's version needed, flags, method, CRC, sizes, name and extra field (its NTFS
/// times zeroed; the DOS date isn't compared), and the data descriptor's CRC, uncompressed size
/// and width (8-byte sizes after a zip64 field); not the compressed sizes (deviation D-6).
fn local_records(archive: &[u8]) -> Vec<Vec<u64>> {
    let u16_at = |i: usize| u64::from(u16::from_le_bytes([archive[i], archive[i + 1]]));
    let u32_at = |i: usize| u64::from(u32::from_le_bytes(archive[i..i + 4].try_into().unwrap()));
    let eocd = archive.len() - 22;
    let count = u16_at(eocd + 10) as usize;
    let mut p = u32_at(eocd + 16) as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        let name_len = u16_at(p + 28) as usize;
        let extra_len = u16_at(p + 30) as usize;
        let comment_len = u16_at(p + 32) as usize;
        let cd_extra = &archive[p + 46 + name_len..p + 46 + name_len + extra_len];
        let mut csize = u32_at(p + 20);
        if csize == u64::from(u32::MAX) && cd_extra.len() >= 20 {
            csize = u64::from_le_bytes(cd_extra[12..20].try_into().unwrap());
        }
        let offset = u32_at(p + 42) as usize;
        let local_name = u16_at(offset + 26) as usize;
        let local_extra = u16_at(offset + 28) as usize;
        let mut record = vec![
            u16_at(offset + 4),
            u16_at(offset + 6),
            u16_at(offset + 8),
            u32_at(offset + 14),
            u32_at(offset + 18),
            u32_at(offset + 22),
        ];
        record.extend(
            archive[offset + 30..offset + 30 + local_name]
                .iter()
                .map(|&b| u64::from(b)),
        );
        let extra = without_times(
            &archive[offset + 30 + local_name..offset + 30 + local_name + local_extra],
        );
        record.push(u64::MAX);
        record.extend(extra.iter().map(|&b| u64::from(b)));
        let flag = u16_at(offset + 6);
        if flag & 8 != 0 {
            let d = offset + 30 + local_name + local_extra + csize as usize;
            let zip64 = extra.len() >= 2 && u16::from_le_bytes([extra[0], extra[1]]) == 1;
            record.push(u64::MAX);
            record.push(u32_at(d));
            record.push(u32_at(d + 4));
            if zip64 {
                record.push(u64::from_le_bytes(
                    archive[d + 16..d + 24].try_into().unwrap(),
                ));
            } else {
                record.push(u32_at(d + 12));
            }
            record.push(u64::from(zip64));
        }
        out.push(record);
        p += 46 + name_len + extra_len + comment_len;
    }
    out
}

/// The port's entries, read from its archive's central directory as `zipfile` reads them.
fn port_entries(archive: &[u8]) -> Vec<Entry> {
    let u16_at = |i: usize| u16::from_le_bytes([archive[i], archive[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(archive[i..i + 4].try_into().unwrap());
    let eocd = archive.len() - 22;
    assert_eq!(u32_at(eocd), 0x0605_4b50, "an end record without a comment");
    let count = usize::from(u16_at(eocd + 10));
    let mut p = u32_at(eocd + 16) as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(p), 0x0201_4b50);
        let name_len = usize::from(u16_at(p + 28));
        let extra_len = usize::from(u16_at(p + 30));
        let comment_len = usize::from(u16_at(p + 32));
        let name = archive[p + 46..p + 46 + name_len].to_vec();
        let extra = archive[p + 46 + name_len..p + 46 + name_len + extra_len].to_vec();
        let method = u16_at(p + 10);
        let mut csize = u64::from(u32_at(p + 20));
        let mut size = u64::from(u32_at(p + 24));
        // zip64 sizes, as zipfile's _decodeExtra reads them
        if extra.len() >= 4 && u16::from_le_bytes([extra[0], extra[1]]) == 1 {
            let mut q = 4;
            let read64 = |q: usize| u64::from_le_bytes(extra[q..q + 8].try_into().unwrap());
            if size == u64::from(u32::MAX) {
                size = read64(q);
                q += 8;
            }
            if csize == u64::from(u32::MAX) {
                csize = read64(q);
            }
        }
        let offset = u32_at(p + 42) as usize;
        let local_name = usize::from(u16_at(offset + 26));
        let local_extra = usize::from(u16_at(offset + 28));
        let start = offset + 30 + local_name + local_extra;
        let data = &archive[start..start + csize as usize];
        let contents = if method == 8 {
            inflate(data)
        } else {
            data.to_vec()
        };
        out.push(Entry {
            name,
            method: u64::from(method),
            flag_bits: u64::from(u16_at(p + 8)),
            create_system: u64::from(archive[p + 5]),
            create_version: u64::from(archive[p + 4]),
            extract_version: u64::from(archive[p + 6]),
            internal_attr: u64::from(u16_at(p + 36)),
            external_attr: u64::from(u32_at(p + 38)),
            extra: without_times(&extra),
            crc: u64::from(u32_at(p + 16)),
            size,
            contents,
        });
        p += 46 + name_len + extra_len + comment_len;
    }
    out
}

/// The directory of the files the tests write.
fn files_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("ocioz_archive_oracle")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `content` to `dir/relative` unless it holds it already (so the files keep their
/// times from run to run), read-only with `read_only`.
fn put(dir: &Path, relative: &str, content: &[u8], read_only: bool) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    if std::fs::read(&path).ok().as_deref() != Some(content) {
        if path.exists() {
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    if permissions.readonly() != read_only {
        permissions.set_readonly(read_only);
        std::fs::set_permissions(&path, permissions).unwrap();
    }
}

/// A 1D LUT of `n` values.
fn lut(n: usize) -> Vec<u8> {
    let mut text = format!("Version 1\nFrom 0.0 1.0\nLength {n}\nComponents 1\n{{\n");
    for i in 0..n {
        text.push_str(&format!("{:.6}\n", i as f64 / n.max(2) as f64));
    }
    text.push_str("}\n");
    text.into_bytes()
}

/// A directory of LUT files and others around a config.
fn generated_config() -> PathBuf {
    let dir = files_dir("generated");
    let config = b"ocio_profile_version: 2\nsearch_path: luts\nroles: {default: raw}\n\
colorspaces:\n  - !<ColorSpace> {name: raw}\n  - !<ColorSpace>\n    name: a\n    \
from_scene_reference: !<FileTransform> {src: a.spi1d}\n";
    put(&dir, "config.ocio", config, false);
    put(&dir, "luts/a.spi1d", &lut(2), false);
    put(&dir, "luts/UPPER.SPI1D", &lut(3), false);
    put(&dir, "luts/read_only.spi1d", &lut(4), true);
    put(&dir, "luts/long.spi1d", &lut(20000), false);
    put(&dir, "luts/empty.cube", b"", false);
    put(&dir, "luts/.hidden.spi1d", &lut(5), false);
    put(&dir, "luts/nested/deep/b.spi1d", &lut(6), false);
    put(&dir, "luts/readme.md", b"not a LUT\n", false);
    put(&dir, "luts/noext", b"not a LUT either\n", false);
    put(&dir, "luts/a.spi1d.bak", b"a backup\n", false);
    put(&dir, "other/c.clf", b"<not really a clf/>\n", false);
    // Links: minizip-ng skips a link to a file; OpenColorIO follows a link to a directory.
    #[cfg(unix)]
    {
        let link = dir.join("luts/link.spi1d");
        if std::fs::symlink_metadata(&link).is_err() {
            std::os::unix::fs::symlink("a.spi1d", &link).unwrap();
        }
        let linked_dir = dir.join("luts/linked_dir");
        if std::fs::symlink_metadata(&linked_dir).is_err() {
            std::os::unix::fs::symlink("nested", &linked_dir).unwrap();
        }
    }
    dir.join("config.ocio")
}

/// A config that isn't archivable: its LUT has an absolute path.
fn not_archivable_config() -> PathBuf {
    let dir = files_dir("not_archivable");
    let lut_path = dir.join("abs.spi1d");
    put(&dir, "abs.spi1d", &lut(2), false);
    let config = format!(
        "ocio_profile_version: 2\nroles: {{default: raw}}\ncolorspaces:\n  - !<ColorSpace> \
         {{name: raw}}\n  - !<ColorSpace>\n    name: a\n    from_scene_reference: \
         !<FileTransform> {{src: \"{}\"}}\n",
        lut_path.to_str().unwrap().replace('\\', "/")
    );
    put(&dir, "config.ocio", config.as_bytes(), false);
    dir.join("config.ocio")
}

/// Each config archived by the wheel and by the port.
fn check(configs: &[(&str, PathBuf)]) {
    let calls: Vec<BatchCall<'_>> = configs
        .iter()
        .map(|(_, path)| BatchCall {
            cmd: "ocioz",
            args: json!({"archive": {"config": {"file": path.to_str().unwrap()}}}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for ((label, path), w) in configs.iter().zip(wheel) {
        let w = w.expect("an oracle request");
        let port = Config::create_from_file(path.to_str().unwrap().as_bytes()).and_then(|c| {
            let mut out = Vec::new();
            c.archive(&mut out).map(|()| out)
        });
        match (w.result.get("exception"), port) {
            (None, Ok(archive)) => {
                let wheel_archive = &w.blobs[w.result["archive"].as_u64().unwrap() as usize];
                let wheel = wheel_entries(&w.result, &w.blobs);
                let port_cd = port_entries(&archive);
                if wheel != port_cd {
                    failures.push(format!("{label}:\n  wheel {wheel:?}\n  port  {port_cd:?}"));
                }
                let (wheel_local, port_local) =
                    (local_records(wheel_archive), local_records(&archive));
                if wheel_local != port_local {
                    failures.push(format!(
                        "{label}: local headers\n  wheel {wheel_local:?}\n  port  {port_local:?}"
                    ));
                }
            }
            (Some(exception), Err(e)) => {
                if exception["message"].as_str().map(str::as_bytes) != Some(e.what()) {
                    failures.push(format!("{label}: wheel {exception}, port {e}"));
                }
            }
            (wheel, port) => failures.push(format!(
                "{label}: wheel {:?}, port {:?}",
                wheel,
                port.map(|a| a.len())
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// upstream's `context_test1` (the config of `archive_config_and_compare_to_original`), a
/// directory of LUT files and others the test writes (upper-case and hidden names, a read-only
/// file, an empty one, one longer than minizip-ng's 64 KiB reads, nested directories, and on
/// Linux links to a file and to a directory), and a config that isn't archivable.
#[test]
fn archives_as_the_wheel() {
    check(&[
        (
            "context_test1",
            upstream_dir().join("tests/data/files/configs/context_test1/config.ocio"),
        ),
        ("generated", generated_config()),
        ("not archivable", not_archivable_config()),
    ]);
}
