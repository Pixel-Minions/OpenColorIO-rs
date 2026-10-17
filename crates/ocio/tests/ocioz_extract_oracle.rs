// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ExtractOCIOZArchive` against the wheel, live on this platform (WP 4.9b): the oracle's
//! `ocioz` extracts an archive into a new directory and reports the files it writes, and the
//! port extracts the same archive file into its own: the same files, with the same contents,
//! or the same error. The archives are upstream's test archives, the wheel's and the port's
//! archives of a config, and archives this test builds: names with `.`, `..`, backslashes,
//! leading slashes and repeats, directory entries, and entries minizip-ng can't extract.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ocio::{Config, extract_ocioz_archive};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::paths::upstream_dir;
use serde_json::json;

/// CRC-32 (ISO-HDLC, as zip uses), of the inputs this test writes.
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

/// `data` deflated as a raw stream at the best level.
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut compress = flate2::Compress::new(flate2::Compression::best(), false);
    let mut out = Vec::with_capacity(data.len() + 1024);
    compress
        .compress_vec(data, &mut out, flate2::FlushCompress::Finish)
        .unwrap();
    out
}

/// An entry: its name, its data as written, its method, CRC and uncompressed size.
struct Entry {
    name: Vec<u8>,
    written: Vec<u8>,
    method: u16,
    crc: u32,
    size: u32,
}

fn stored(name: &[u8], data: &[u8]) -> Entry {
    Entry {
        name: name.to_vec(),
        written: data.to_vec(),
        method: 0,
        crc: crc32(data),
        size: data.len() as u32,
    }
}

fn deflated(name: &[u8], data: &[u8]) -> Entry {
    Entry {
        name: name.to_vec(),
        written: deflate(data),
        method: 8,
        crc: crc32(data),
        size: data.len() as u32,
    }
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// The archive of `entries`.
fn archive(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let offset = out.len() as u32;
        put32(&mut out, 0x0403_4b50);
        for v in [20u16, 0, e.method, 0, 0x0021] {
            put16(&mut out, v);
        }
        for v in [e.crc, e.written.len() as u32, e.size] {
            put32(&mut out, v);
        }
        put16(&mut out, e.name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.written);

        put32(&mut central, 0x0201_4b50);
        for v in [20u16, 20, 0, e.method, 0, 0x0021] {
            put16(&mut central, v);
        }
        for v in [e.crc, e.written.len() as u32, e.size] {
            put32(&mut central, v);
        }
        for v in [e.name.len() as u16, 0, 0, 0, 0] {
            put16(&mut central, v);
        }
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(&e.name);
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    let n = entries.len() as u16;
    for v in [0u16, 0, n, n] {
        put16(&mut out, v);
    }
    put32(&mut out, central.len() as u32);
    put32(&mut out, cd_offset);
    put16(&mut out, 0);
    out
}

/// The directory of the files the tests write.
fn files_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("ocioz_extract_oracle")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `content` to a file named by its length and FNV-1a hash; its absolute path.
fn write_file(content: &[u8]) -> PathBuf {
    let name = format!(
        "{}_{:016x}.ocioz",
        content.len(),
        ocio::caching::msvc_fnv1a(content)
    );
    let path = files_dir("archives").join(name);
    std::fs::write(&path, content).unwrap();
    path
}

/// Every file under `dir`, by relative path with `/` between its components.
fn files_under(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                // The components joined with "/", as the oracle joins them (os.sep replaced):
                // on Linux a backslash is part of a file's name.
                let relative = path
                    .strip_prefix(dir)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// Each archive extracted by the wheel and by the port.
fn check(archives: &[(&str, PathBuf)]) {
    let calls: Vec<BatchCall<'_>> = archives
        .iter()
        .map(|(_, path)| BatchCall {
            cmd: "ocioz",
            args: json!({"extract": {"archive": path.to_str().unwrap()}}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for (i, ((label, path), w)) in archives.iter().zip(wheel).enumerate() {
        let w = w.expect("an oracle request");
        let destination = files_dir("extracted").join(i.to_string());
        let _ = std::fs::remove_dir_all(&destination);
        std::fs::create_dir_all(&destination).unwrap();
        let port = extract_ocioz_archive(
            path.to_str().unwrap().as_bytes(),
            destination.to_str().unwrap().as_bytes(),
        );
        match (w.result.get("exception"), port) {
            (None, Ok(())) => {
                let wheel: BTreeMap<String, Vec<u8>> = w.result["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| {
                        (
                            String::from_utf8_lossy(&bytes(&f["path"])).into_owned(),
                            w.blobs[f["contents"].as_u64().unwrap() as usize].clone(),
                        )
                    })
                    .collect();
                let port = files_under(&destination);
                if wheel != port {
                    failures.push(format!(
                        "{label}:\n  wheel {:?}\n  port  {:?}",
                        wheel.keys().collect::<Vec<_>>(),
                        port.keys().collect::<Vec<_>>()
                    ));
                }
            }
            (Some(exception), Err(e)) => {
                if exception["message"].as_str().map(str::as_bytes) != Some(e.what()) {
                    failures.push(format!("{label}: wheel {exception}, port {e}"));
                }
            }
            (wheel, port) => failures.push(format!("{label}: wheel {wheel:?}, port {port:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn upstream_archive(dir: &str, name: &str) -> PathBuf {
    upstream_dir()
        .join("tests/data/files/configs")
        .join(dir)
        .join(name)
}

/// Upstream's test archives, and the port's archive of upstream's `context_test1`.
#[test]
fn upstream_and_port_archives() {
    let config = upstream_dir().join("tests/data/files/configs/context_test1/config.ocio");
    let mut port_archive = Vec::new();
    Config::create_from_file(config.to_str().unwrap().as_bytes())
        .unwrap()
        .archive(&mut port_archive)
        .unwrap();
    let port_path = write_file(&port_archive);
    check(&[
        (
            "context_test1_windows",
            upstream_archive("context_test1", "context_test1_windows.ocioz"),
        ),
        (
            "context_test1_linux",
            upstream_archive("context_test1", "context_test1_linux.ocioz"),
        ),
        (
            "config_missing_luts",
            upstream_archive("ocioz_archive_configs", "config_missing_luts.ocioz"),
        ),
        (
            "missing_config",
            upstream_archive("ocioz_archive_configs", "missing_config.ocioz"),
        ),
        (
            "empty",
            upstream_archive("ocioz_archive_configs", "empty.ocioz"),
        ),
        ("the port's context_test1", port_path),
    ]);
}

/// Archives this test builds: names minizip-ng resolves, directory entries, repeated names,
/// and entries it can't extract; a file that isn't an archive.
#[test]
fn built_archives() {
    let mut corrupt = deflated(b"corrupt.txt", b"some text, long enough to be compressed");
    corrupt.written[1] ^= 0xff;
    let mut wrong_crc = stored(b"wrong_crc.txt", b"text");
    wrong_crc.crc ^= 1;
    let mut unknown = stored(b"unknown.txt", b"text");
    unknown.method = 12;
    // Each name in its own archive, before an entry of a plain name.
    let names: [&[u8]; 10] = [
        b"a/../b.txt",
        b"./c.txt",
        b"dir\\back.txt",
        b"/abs.txt",
        b"../../escape.txt",
        b"x/./y/../z.txt",
        b"x//double//slash.txt",
        b"trailing/.",
        b"..",
        b"",
    ];
    let labels: Vec<String> = names
        .iter()
        .map(|name| format!("the name {:?}", String::from_utf8_lossy(name)))
        .collect();
    let cases: Vec<(&str, PathBuf)> = names
        .iter()
        .zip(&labels)
        .map(|(name, label)| {
            let entries = [stored(name, b"text"), deflated(b"after.txt", b"after")];
            (label.as_str(), write_file(&archive(&entries)))
        })
        .collect();
    check(&cases);
    check(&[
        (
            "directories",
            write_file(&archive(&[
                stored(b"dir/", b""),
                stored(b"dir/sub/", b""),
                stored(b"dir/sub/f.txt", b"f"),
            ])),
        ),
        (
            "repeated names",
            write_file(&archive(&[
                stored(b"dup.txt", b"first"),
                deflated(b"dup.txt", b"second"),
            ])),
        ),
        (
            "a corrupt entry",
            write_file(&archive(&[stored(b"ok.txt", b"ok"), corrupt])),
        ),
        ("a wrong CRC", write_file(&archive(&[wrong_crc]))),
        ("an unknown method", write_file(&archive(&[unknown]))),
        ("not an archive", write_file(b"PK, and no archive")),
        (
            "missing",
            files_dir("archives").join("does_not_exist.ocioz"),
        ),
    ]);
}
