// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `.ocioz` archives read against the wheel, live on this platform (WP 4.9a): archives this
//! test builds, read with `Config::CreateFromFile`. For each, the config's outcome (its
//! `serialize()` and cache ID, which hashes the names and CRCs of the LUT files it finds, or
//! the error) and the processors that read its two LUT files (their cache IDs, which hash the
//! LUTs' values, or the error).
//!
//! The archives are well-formed ones and the unusual and broken ones minizip-ng reads its own
//! way: names that are wildcards, differ in case or slashes, repeat or hold a NUL; entries
//! whose method isn't supported, whose sizes or CRC are wrong, whose data is flagged as
//! encrypted or is corrupt; central directories at a wrong offset or with a wrong count;
//! extra fields that misalign the next one; entries on another disk.
//!
//! Many of the cases (from `inflate_at_read_boundaries` to `hashes_of_repeated_names`) are the
//! probes of the verifier of WP 4.9a, adopted: deflate data around minizip-ng's 65,535-byte
//! reads, end records, every cut of an archive, recoveries whose seeks fail, zipped central
//! directories and split archives; and the cases after them pin what its mutants left open
//! (comments, zip64 descriptors, the end record's search window, the header stream's stale
//! bytes, the 256 MiB limit).
//!
//! Both sides read the same file, by the same absolute path. OCIO caches file hashes by path,
//! so each request starts with its caches cleared, on both sides (`with_files` on the wheel's).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ocio::{Config, clear_all_caches};
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, log};
use serde_json::{Value, json};

/// The tests of this file share OCIO's caches and logging function.
static CACHES: Mutex<()> = Mutex::new(());

/// `f()`, and what OCIO logged meanwhile.
fn captured<T>(f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        sink.lock().unwrap().push(m.to_vec());
    })))
    .unwrap();
    let result = f();
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (result, log)
}

const CONFIG: &[u8] = b"ocio_profile_version: 2\nsearch_path: luts\nroles: {default: raw}\n\
colorspaces:\n  - !<ColorSpace> {name: raw}\n  - !<ColorSpace>\n    name: a\n    \
from_scene_reference: !<FileTransform> {src: a.spi1d}\n  - !<ColorSpace>\n    name: b\n    \
from_scene_reference: !<FileTransform> {src: sub/b.spi1d}\n";

/// A 1D LUT whose second value is `top`.
fn lut(top: &str) -> Vec<u8> {
    format!("Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{{\n0.0\n{top}\n}}\n").into_bytes()
}

/// A 1D LUT of `n` values, longer than minizip-ng's 64 KiB reads when `n` is large.
fn long_lut(n: usize) -> Vec<u8> {
    let mut text = format!("Version 1\nFrom 0.0 1.0\nLength {n}\nComponents 1\n{{\n");
    for i in 0..n {
        text.push_str(&format!("{:.6}\n", i as f64 / n as f64));
    }
    text.push_str("}\n");
    text.into_bytes()
}

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
    let status = compress
        .compress_vec(data, &mut out, flate2::FlushCompress::Finish)
        .unwrap();
    assert_eq!(status, flate2::Status::StreamEnd);
    out
}

/// An entry of an archive: its name and contents, and what the archive says of them.
#[derive(Clone, Debug)]
struct Entry {
    name: Vec<u8>,
    data: Vec<u8>,
    /// 0 (stored), 8 (deflated), or another, whose data is written as it is.
    method: u16,
    /// The data as written, where it isn't `data` stored or deflated.
    raw: Option<Vec<u8>>,
    flag: u16,
    crc: Option<u32>,
    compressed_size: Option<u32>,
    uncompressed_size: Option<u32>,
    cd_extra: Vec<u8>,
    disk: u16,
    /// The local header's extra field.
    local_extra: Vec<u8>,
    /// The CRC and sizes in a data descriptor after the data, the local header's zeros.
    descriptor: bool,
    /// The descriptor's sizes in 8 bytes each (zip64's).
    descriptor64: bool,
    /// The local header's name, where it isn't the central directory's.
    local_name: Option<Vec<u8>>,
    /// The central directory's comment.
    comment: Vec<u8>,
}

impl Entry {
    fn new(name: &[u8], data: &[u8], method: u16) -> Entry {
        Entry {
            name: name.to_vec(),
            data: data.to_vec(),
            method,
            raw: None,
            flag: 0,
            crc: None,
            compressed_size: None,
            uncompressed_size: None,
            cd_extra: Vec::new(),
            disk: 0,
            local_extra: Vec::new(),
            descriptor: false,
            descriptor64: false,
            local_name: None,
            comment: Vec::new(),
        }
    }

    /// The entry's local header and data.
    fn local_record(&self) -> Vec<u8> {
        let written = self.written();
        let mut out = Vec::new();
        let crc = self.crc.unwrap_or_else(|| crc32(&self.data));
        let csize = self.compressed_size.unwrap_or(written.len() as u32);
        let usize_ = self.uncompressed_size.unwrap_or(self.data.len() as u32);
        put32(&mut out, 0x0403_4b50);
        let flag = self.flag | if self.descriptor { 8 } else { 0 };
        for v in [20u16, flag, self.method, 0, 0x0021] {
            put16(&mut out, v);
        }
        for v in if self.descriptor {
            [0, 0, 0]
        } else {
            [crc, csize, usize_]
        } {
            put32(&mut out, v);
        }
        let name = self.local_name.as_ref().unwrap_or(&self.name);
        put16(&mut out, name.len() as u16);
        put16(&mut out, self.local_extra.len() as u16);
        out.extend_from_slice(name);
        out.extend_from_slice(&self.local_extra);
        out.extend_from_slice(&written);
        if self.descriptor {
            for v in [0x0807_4b50, crc] {
                put32(&mut out, v);
            }
            for v in [csize, usize_] {
                if self.descriptor64 {
                    out.extend_from_slice(&u64::from(v).to_le_bytes());
                } else {
                    put32(&mut out, v);
                }
            }
        }
        out
    }

    /// The entry's central header, its local header at `offset`.
    fn central_record(&self, offset: u32) -> Vec<u8> {
        let written = self.written();
        let mut out = Vec::new();
        put32(&mut out, 0x0201_4b50);
        for v in [20u16, 20, self.flag, self.method, 0, 0x0021] {
            put16(&mut out, v);
        }
        put32(&mut out, self.crc.unwrap_or_else(|| crc32(&self.data)));
        put32(
            &mut out,
            self.compressed_size.unwrap_or(written.len() as u32),
        );
        put32(
            &mut out,
            self.uncompressed_size.unwrap_or(self.data.len() as u32),
        );
        for v in [
            self.name.len() as u16,
            self.cd_extra.len() as u16,
            self.comment.len() as u16,
            self.disk,
            0,
        ] {
            put16(&mut out, v);
        }
        put32(&mut out, 0);
        put32(&mut out, offset);
        out.extend_from_slice(&self.name);
        out.extend_from_slice(&self.cd_extra);
        out.extend_from_slice(&self.comment);
        out
    }

    fn written(&self) -> Vec<u8> {
        match (&self.raw, self.method) {
            (Some(raw), _) => raw.clone(),
            (None, 8) => deflate(&self.data),
            (None, _) => self.data.clone(),
        }
    }
}

/// An archive: its entries, what comes before it, and its end of central directory.
#[derive(Clone, Debug, Default)]
struct Archive {
    entries: Vec<Entry>,
    /// Bytes before the archive, which its offsets don't count.
    prefix: Vec<u8>,
    /// The end record's entry counts, where they aren't the entries'.
    count: Option<(u16, u16)>,
    disk_with_cd: u16,
    comment: Vec<u8>,
    /// Added to the central directory's offset.
    cd_offset_delta: i64,
    /// Where the file ends, before the archive's end, when it is cut.
    cut: Option<usize>,
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

impl Archive {
    fn of(entries: Vec<Entry>) -> Archive {
        Archive {
            entries,
            ..Archive::default()
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let mut out = self.prefix.clone();
        let base = self.prefix.len();
        let mut central = Vec::new();
        for e in &self.entries {
            let offset = (out.len() - base) as u32;
            out.extend_from_slice(&e.local_record());
            central.extend_from_slice(&e.central_record(offset));
        }
        let cd_offset = ((out.len() - base) as i64 + self.cd_offset_delta) as u32;
        out.extend_from_slice(&central);
        let n = self.entries.len() as u16;
        let (on_disk, total) = self.count.unwrap_or((n, n));
        put32(&mut out, 0x0605_4b50);
        for v in [0u16, self.disk_with_cd, on_disk, total] {
            put16(&mut out, v);
        }
        put32(&mut out, central.len() as u32);
        put32(&mut out, cd_offset);
        put16(&mut out, self.comment.len() as u16);
        out.extend_from_slice(&self.comment);
        if let Some(cut) = self.cut {
            out.truncate(cut);
        }
        out
    }
}

/// An archive whose central directory is zipped, as minizip-ng writes one (`zip_cd`): the
/// entries' central headers are the data of an entry `__cdcd__`, which its central directory
/// lists first (the entries' own headers after it with `also_listed`), with the number of
/// entries in its `0xcdcd` extra field when `count` is given.
fn zipped_cd(entries: &[Entry], method: u16, count: Option<u64>, also_listed: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut inner = Vec::new();
    for e in entries {
        let offset = out.len() as u32;
        out.extend_from_slice(&e.local_record());
        inner.extend_from_slice(&e.central_record(offset));
    }
    let mut cd_entry = Entry::new(b"__cdcd__", &inner, method);
    if let Some(count) = count {
        cd_entry.cd_extra = vec![0xcd, 0xcd, 8, 0];
        cd_entry.cd_extra.extend_from_slice(&count.to_le_bytes());
    }
    let cd_entry_offset = out.len() as u32;
    out.extend_from_slice(&cd_entry.local_record());
    let mut central = cd_entry.central_record(cd_entry_offset);
    let mut n = 1u16;
    if also_listed {
        central.extend_from_slice(&inner);
        n += entries.len() as u16;
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    for v in [0u16, 0, n, n] {
        put16(&mut out, v);
    }
    put32(&mut out, central.len() as u32);
    put32(&mut out, cd_offset);
    put16(&mut out, 0);
    out
}

/// The directory of the archives the tests write.
fn files_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ocioz_read_oracle");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes `content` to a file named by its length and FNV-1a hash; its absolute path, with
/// `/` separators.
fn write_file(content: &[u8]) -> String {
    let name = format!(
        "{}_{:016x}.ocioz",
        content.len(),
        ocio::caching::msvc_fnv1a(content)
    );
    let path = files_dir().join(name);
    std::fs::write(&path, content).unwrap();
    path.to_str().unwrap().replace('\\', "/")
}

/// An outcome: the result's bytes, or the error's message.
type Outcome = Result<Vec<Vec<u8>>, Vec<u8>>;

/// The wheel's outcome of `with_files` running `config_calls` on the archive `path`: its
/// `serialize()` and cache ID, or the error; and the config's log.
fn wheel_config(result: &Value) -> (Outcome, Vec<Vec<u8>>) {
    let config = &result["config"];
    let outcome = if config.is_null() {
        Ok(result["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| bytes(&c["result"]))
            .collect())
    } else {
        Err(bytes(&config["exception"]["message"]))
    };
    (outcome, log(&result["config_log"]))
}

/// The wheel's outcome of `config_processor`: the processor's cache ID, or the error.
fn wheel_processor(result: &Value) -> Outcome {
    if !result["config"].is_null() {
        return Err(bytes(&result["config"]["exception"]["message"]));
    }
    let processor = &result["processor"];
    match processor.get("cache_id") {
        Some(id) => Ok(vec![bytes(id)]),
        None => Err(bytes(&processor["exception"]["message"])),
    }
}

fn show(outcome: &Outcome) -> String {
    match outcome {
        Ok(parts) => format!(
            "ok {:?}",
            parts
                .iter()
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .collect::<Vec<_>>()
        ),
        Err(e) => format!("error {:?}", String::from_utf8_lossy(e)),
    }
}

/// The color spaces whose processors from `raw` read the LUT files.
const LUT_SPACES: [&str; 2] = ["a", "b"];

/// Each archive against the wheel.
fn check(cases: &[(&str, Archive)]) {
    let files: Vec<(&str, Vec<u8>)> = cases.iter().map(|(l, a)| (*l, a.bytes())).collect();
    check_files(&files);
}

/// Each archive file against the wheel.
fn check_files(cases: &[(&str, Vec<u8>)]) {
    let _lock = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    let paths: Vec<String> = cases.iter().map(|(_, a)| write_file(a)).collect();

    let mut calls = Vec::new();
    for path in &paths {
        calls.push(BatchCall {
            cmd: "with_files",
            args: json!({"files": {}, "command": "config_calls", "args": {
                "config": {"file": path},
                "calls": [{"call": "serialize"}, {"call": "getCacheID"}]}}),
            blobs: Vec::new(),
        });
        for space in LUT_SPACES {
            calls.push(BatchCall {
                cmd: "with_files",
                args: json!({"files": {}, "command": "config_processor", "args": {
                    "config": {"file": path},
                    "overload": {"names": ["raw", space]}}}),
                blobs: Vec::new(),
            });
        }
    }
    let wheel = Oracle::get().batch(&calls, true);
    let mut wheel = wheel
        .into_iter()
        .map(|w| w.expect("an oracle request").result);

    let mut failures = Vec::new();
    for ((label, _), path) in cases.iter().zip(&paths) {
        let (wheel_outcome, wheel_log) = wheel_config(&wheel.next().unwrap());

        clear_all_caches();
        let (port_outcome, port_log) = captured(|| {
            Config::create_from_file(path.as_bytes())
                .and_then(|config| Ok(vec![config.serialize()?, config.cache_id()?.into_bytes()]))
                .map_err(|e| e.what().to_vec())
        });
        if wheel_outcome != port_outcome {
            failures.push(format!(
                "{label}: config\n  wheel {}\n  port  {}",
                show(&wheel_outcome),
                show(&port_outcome)
            ));
        }
        if wheel_log != port_log {
            failures.push(format!(
                "{label}: config log\n  wheel {wheel_log:?}\n  port  {port_log:?}"
            ));
        }

        for space in LUT_SPACES {
            let wheel_outcome = wheel_processor(&wheel.next().unwrap());
            clear_all_caches();
            let port_outcome = Config::create_from_file(path.as_bytes())
                .and_then(|config| config.processor_between("raw", space))
                .and_then(|processor| Ok(vec![processor.cache_id()?.into_bytes()]))
                .map_err(|e| e.what().to_vec());
            if wheel_outcome != port_outcome {
                failures.push(format!(
                    "{label}: raw to {space}\n  wheel {}\n  port  {}",
                    show(&wheel_outcome),
                    show(&port_outcome)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The config and its two LUTs, with the given method.
fn standard(method: u16) -> Vec<Entry> {
    vec![
        Entry::new(b"config.ocio", CONFIG, method),
        Entry::new(b"luts/a.spi1d", &lut("2.0"), method),
        Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), method),
    ]
}

/// Well-formed archives, and their names: the config and LUTs are found by names compared
/// ignoring case and the kind of slashes, as wildcards (the archive's name is the wildcard
/// for the data, the path asked for is the wildcard for the hash), and up to a NUL.
#[test]
fn archives_and_their_names() {
    let mut cases = vec![
        ("stored", Archive::of(standard(0))),
        ("deflated", Archive::of(standard(8))),
        ("empty archive", Archive::of(Vec::new())),
        ("no config", Archive::of(standard(8)[1..].to_vec())),
        (
            "names in other cases",
            Archive::of(vec![
                Entry::new(b"CONFIG.OCIO", CONFIG, 8),
                Entry::new(b"LUTS/A.SPI1D", &lut("2.0"), 8),
                Entry::new(b"Luts/Sub/B.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "backslashes",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts\\a.spi1d", &lut("2.0"), 8),
                Entry::new(b"luts\\sub\\b.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "wildcard names",
            Archive::of(vec![
                Entry::new(b"*.ocio", CONFIG, 8),
                Entry::new(b"luts/*", &lut("2.0"), 8),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "a wildcard before the name",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts/*.spi1d", &lut("5.0"), 8),
                Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "a lone star",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"*", &lut("7.0"), 8),
            ]),
        ),
        (
            "repeated names",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
                Entry::new(b"luts/a.spi1d", &lut("4.0"), 8),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 0),
                Entry::new(b"luts/sub/b.spi1d", &lut("6.0"), 0),
            ]),
        ),
        (
            "a NUL in a name",
            Archive::of(vec![
                Entry::new(b"config.ocio\0x", CONFIG, 8),
                Entry::new(b"luts/a.spi1d\0", &lut("2.0"), 8),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "directories",
            Archive::of(vec![
                Entry::new(b"luts/", b"", 0),
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts/sub/", b"", 0),
                Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "an empty LUT",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts/a.spi1d", b"", 0),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "an empty config",
            Archive::of(vec![Entry::new(b"config.ocio", b"", 0)]),
        ),
        (
            "long LUTs",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts/a.spi1d", &long_lut(20000), 8),
                Entry::new(b"luts/sub/b.spi1d", &long_lut(30000), 0),
            ]),
        ),
    ];
    let mut comment = Archive::of(standard(8));
    comment.comment = b"an archive's comment".to_vec();
    cases.push(("a comment", comment));
    check(&cases);
}

/// Entries minizip-ng reads its own way: an unsupported method or an AES entry leaves the
/// buffer zeros; data flagged as encrypted is read as stored; sizes and CRCs are the central
/// directory's, right or wrong; corrupt deflate data loses the 64 KiB read it fails in.
#[test]
fn entries_read_their_own_way() {
    let with = |index: usize, change: &dyn Fn(&mut Entry)| {
        let mut entries = standard(8);
        change(&mut entries[index]);
        Archive::of(entries)
    };
    let corrupt = |data: &[u8], at: usize| {
        let mut raw = deflate(data);
        let at = at.min(raw.len() - 1);
        raw[at] ^= 0xff;
        raw
    };
    let long = long_lut(20000);
    let cases = vec![
        ("LUT of an unknown method", with(1, &|e| e.method = 12)),
        ("config of an unknown method", with(0, &|e| e.method = 14)),
        (
            "LUT flagged encrypted, stored",
            with(1, &|e| {
                e.method = 0;
                e.flag = 1;
            }),
        ),
        ("LUT flagged encrypted, deflated", with(1, &|e| e.flag = 1)),
        ("config flagged encrypted", with(0, &|e| e.flag = 1)),
        (
            "AES entry",
            with(1, &|e| {
                e.method = 99;
                e.flag = 1;
                e.cd_extra = vec![0x01, 0x99, 7, 0, 2, 0, b'A', b'E', 3, 8, 0];
            }),
        ),
        (
            "LUT longer than its size",
            with(1, &|e| e.uncompressed_size = Some(20)),
        ),
        (
            "LUT shorter than its size",
            with(1, &|e| e.uncompressed_size = Some(200)),
        ),
        (
            "stored LUT longer than its size",
            with(1, &|e| {
                e.method = 0;
                e.uncompressed_size = Some(20);
            }),
        ),
        (
            "stored LUT shorter than its compressed size",
            with(1, &|e| {
                e.method = 0;
                e.compressed_size = Some(30);
            }),
        ),
        ("wrong CRC", with(1, &|e| e.crc = Some(12345))),
        (
            "compressed size 0",
            with(1, &|e| e.compressed_size = Some(0)),
        ),
        (
            "uncompressed size 0",
            with(1, &|e| e.uncompressed_size = Some(0)),
        ),
        (
            "corrupt LUT",
            with(1, &|e| e.raw = Some(corrupt(&e.data, 2))),
        ),
        (
            "corrupt config",
            with(0, &|e| e.raw = Some(corrupt(&e.data, 40))),
        ),
        (
            "corrupt long LUT",
            with(1, &|e| {
                e.data = long.clone();
                e.raw = Some(corrupt(&long, 2000));
            }),
        ),
        (
            "truncated deflate data",
            with(1, &|e| {
                let raw = deflate(&e.data);
                e.raw = Some(raw[..raw.len() / 2].to_vec());
            }),
        ),
        ("LUT on another disk", with(1, &|e| e.disk = 1)),
        ("data descriptor flag", with(1, &|e| e.flag = 8)),
        (
            "a large uncompressed size",
            with(1, &|e| e.uncompressed_size = Some(0x8000_0000)),
        ),
        (
            "a size past 256 MB",
            with(1, &|e| e.uncompressed_size = Some(256 * 1024 * 1024 + 1)),
        ),
        (
            "a zip64 field",
            with(1, &|e| {
                let size = lut("2.0").len() as u64;
                e.uncompressed_size = Some(u32::MAX);
                let mut extra = vec![0x01, 0x00, 8, 0];
                extra.extend_from_slice(&size.to_le_bytes());
                e.cd_extra = extra;
            }),
        ),
        (
            "a zip64 field that misaligns the next",
            with(1, &|e| {
                let size = lut("2.0").len() as u64;
                e.uncompressed_size = Some(u32::MAX);
                let mut extra = vec![0x01, 0x00, 16, 0];
                extra.extend_from_slice(&size.to_le_bytes());
                extra.extend_from_slice(&[0x0a, 0, 8, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
                e.cd_extra = extra;
            }),
        ),
        (
            "an NTFS field",
            with(1, &|e| {
                let mut extra = vec![0x0a, 0x00, 32, 0, 0, 0, 0, 0, 1, 0, 24, 0];
                extra.extend_from_slice(&[0x11; 24]);
                e.cd_extra = extra;
            }),
        ),
        (
            "a UNIX1 field with a link",
            with(1, &|e| {
                let mut extra = vec![0x0d, 0x00, 16, 0];
                extra.extend_from_slice(&[0; 12]);
                extra.extend_from_slice(b"link");
                e.cd_extra = extra;
            }),
        ),
        (
            "a field longer than the extra field",
            with(1, &|e| {
                e.cd_extra = vec![0x34, 0x12, 200, 0, 1, 2];
            }),
        ),
    ];
    check(&cases);
}

/// Archives whose central directory isn't where its end record says, whose end record counts
/// the entries wrong, or whose data has bytes before it.
#[test]
fn central_directories() {
    let mut cases = Vec::new();
    let mut fewer = Archive::of(standard(8));
    fewer.count = Some((1, 1));
    cases.push(("count of 1", fewer));
    let mut mismatch = Archive::of(standard(8));
    mismatch.count = Some((3, 2));
    cases.push(("counts that differ", mismatch));
    let mut prefixed = Archive::of(standard(8));
    prefixed.prefix = b"PK, a stub before the archive".to_vec();
    cases.push(("bytes before the archive", prefixed));
    let mut shifted = Archive::of(standard(8));
    shifted.cd_offset_delta = 7;
    cases.push(("a central directory offset 7 too far", shifted));
    let mut early = Archive::of(standard(8));
    early.cd_offset_delta = -5;
    cases.push(("a central directory offset 5 too near", early));
    let mut disk = Archive::of(standard(8));
    disk.disk_with_cd = 1;
    cases.push(("the central directory on disk 1", disk));
    let mut both = Archive::of(standard(8));
    both.disk_with_cd = 1;
    for e in &mut both.entries {
        e.disk = 1;
    }
    cases.push(("everything on disk 1", both));
    let mut not_zip = Archive::default();
    not_zip.prefix = b"PK, and no archive".to_vec();
    not_zip.cut = Some(not_zip.prefix.len());
    cases.push(("PK and no archive", not_zip));
    check(&cases);
}

/// Two archives read one after the other in the same process, whose LUT files have the same
/// paths in the archives and different contents: OCIO caches file hashes by path, whatever
/// the archive, and its file cache by hash.
#[test]
fn two_archives_in_one_process() {
    let _lock = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    let first = write_file(&Archive::of(standard(8)).bytes());
    let mut entries = standard(8);
    entries[1] = Entry::new(b"luts/a.spi1d", &lut("9.0"), 8);
    let second = write_file(&Archive::of(entries).bytes());

    let calls = [BatchCall {
        cmd: "with_files",
        args: json!({"files": {}, "command": "config_calls", "args": {
        "config": {"file": first},
        "calls": [
            {"call": "getProcessor", "args": ["raw", "a"], "as": "p1"},
            {"call": "getCacheID", "on": "p1"},
            {"call": "CreateFromFile", "on": "Config", "args": [second], "as": "c2"},
            {"call": "getCacheID", "on": "c2"},
            {"call": "getProcessor", "on": "c2", "args": ["raw", "a"], "as": "p2"},
            {"call": "getCacheID", "on": "p2"},
        ]}}),
        blobs: Vec::new(),
    }];
    let wheel = Oracle::get().batch(&calls, true).remove(0).unwrap().result;
    let wheel: Vec<Vec<u8>> = [1, 3, 5]
        .iter()
        .map(|&i| bytes(&wheel["calls"][i]["result"]))
        .collect();

    clear_all_caches();
    let c1 = Config::create_from_file(first.as_bytes()).unwrap();
    let p1 = c1.processor_between("raw", "a").unwrap();
    let c2 = Config::create_from_file(second.as_bytes()).unwrap();
    let p2 = c2.processor_between("raw", "a").unwrap();
    let port = [
        p1.cache_id().unwrap().into_bytes(),
        c2.cache_id().unwrap().into_bytes(),
        p2.cache_id().unwrap().into_bytes(),
    ];
    assert_eq!(
        wheel
            .iter()
            .map(|w| String::from_utf8_lossy(w).into_owned())
            .collect::<Vec<_>>(),
        port.iter()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect::<Vec<_>>()
    );
    // What the wheel shows: the second archive's processor is the first's.
    assert_eq!(wheel[0], wheel[2]);
}

/// Archives whose central directory minizip-ng can't read, which it rebuilds from the local
/// headers (`mz_zip_recover_cd`, the reader's recovery): cut short, without their end record,
/// with data descriptors, and with the local extra fields it rewrites.
#[test]
fn recovered_archives() {
    let full = Archive::of(standard(8)).bytes();
    let cd_start = {
        let mut n = 0;
        for e in standard(8) {
            n += e.local_record().len();
        }
        n
    };
    let cut = |at: usize| {
        let mut archive = Archive::of(standard(8));
        archive.cut = Some(at);
        archive
    };
    let mut cases = vec![
        ("cut before the central directory", cut(cd_start)),
        ("cut in the central directory", cut(cd_start + 30)),
        ("cut before the end record", cut(full.len() - 22)),
        ("cut in the last LUT", cut(cd_start - 20)),
        ("cut in the first LUT", cut(400)),
        ("cut after the config's header", cut(45)),
    ];
    let with = |change: &dyn Fn(&mut Vec<Entry>)| {
        let mut entries = standard(8);
        change(&mut entries);
        let mut archive = Archive::of(entries);
        archive.cut = Some(archive.bytes().len() - 22);
        archive
    };
    cases.push((
        "data descriptors",
        with(&|es| {
            for e in es.iter_mut() {
                e.descriptor = true;
            }
        }),
    ));
    cases.push((
        "a stored entry with a descriptor",
        with(&|es| {
            es[1].method = 0;
            es[1].descriptor = true;
        }),
    ));
    cases.push((
        "backslashes",
        with(&|es| {
            es[1].name = b"luts\\a.spi1d".to_vec();
            es[2].name = b"luts\\sub\\b.spi1d".to_vec();
        }),
    ));
    cases.push((
        "local NTFS times",
        with(&|es| {
            let mut extra = vec![0x0a, 0x00, 32, 0, 0, 0, 0, 0, 1, 0, 24, 0];
            extra.extend_from_slice(&[0x11; 24]);
            es[1].local_extra = extra;
        }),
    ));
    cases.push((
        "a local UNIX1 link",
        with(&|es| {
            let mut extra = vec![0x0d, 0x00, 16, 0];
            extra.extend_from_slice(&[0; 12]);
            extra.extend_from_slice(b"link");
            es[1].local_extra = extra;
        }),
    ));
    cases.push((
        "a local zip64 field and another",
        with(&|es| {
            let mut extra = vec![0x01, 0x00, 16, 0];
            extra.extend_from_slice(&[0; 16]);
            extra.extend_from_slice(&[0x34, 0x12, 2, 0, 7, 7]);
            es[1].local_extra = extra;
        }),
    ));
    cases.push((
        "repeated names",
        with(&|es| {
            es.push(Entry::new(b"luts/a.spi1d", &lut("4.0"), 8));
        }),
    ));
    let mut only_local = Archive::default();
    only_local.prefix = b"PK\x03\x04 and nothing else".to_vec();
    only_local.cut = Some(only_local.prefix.len());
    cases.push(("a local signature and nothing else", only_local));
    check(&cases);
}

/// Archives whose central directory is zipped, which minizip-ng reads instead of the archive's
/// own when the zipped one decompresses whole.
#[test]
fn zipped_central_directories() {
    let entries = standard(8);
    let mut short = entries.clone();
    short[1].uncompressed_size = Some(9);
    check_files(&[
        ("deflated", zipped_cd(&entries, 8, Some(3), false)),
        ("stored", zipped_cd(&entries, 0, Some(3), false)),
        ("also listed", zipped_cd(&entries, 8, Some(3), true)),
        ("without its count", zipped_cd(&entries, 8, None, false)),
        (
            "without its count, also listed",
            zipped_cd(&entries, 8, None, true),
        ),
        ("a count of 1", zipped_cd(&entries, 8, Some(1), false)),
        (
            "an entry's size wrong",
            zipped_cd(&short, 8, Some(3), false),
        ),
    ]);
}

/// `CONFIG` padded with a YAML comment to exactly `n` bytes.
fn padded_config(n: usize) -> Vec<u8> {
    let mut c = CONFIG.to_vec();
    c.extend_from_slice(b"#");
    while c.len() < n - 1 {
        c.push(b'x');
    }
    c.push(b'\n');
    assert_eq!(c.len(), n);
    c
}

/// Raw deflate of stored blocks (each <= 65535 bytes), then `tail` bytes.
fn stored_blocks(data: &[u8], last_final: bool, tail: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let chunks: Vec<&[u8]> = data.chunks(65535).collect();
    for (i, c) in chunks.iter().enumerate() {
        let fin = last_final && i + 1 == chunks.len();
        out.push(if fin { 1 } else { 0 });
        out.extend_from_slice(&(c.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        out.extend_from_slice(c);
    }
    out.extend_from_slice(tail);
    out
}

fn with_entry(index: usize, change: &dyn Fn(&mut Entry)) -> Archive {
    let mut entries = standard(8);
    change(&mut entries[index]);
    Archive::of(entries)
}

/// A config of 65,534 to 131,070 bytes in stored deflate blocks, followed by an invalid block type,
/// by nothing (the last entry), or ended by a final block: minizip-ng reads it 65,535 bytes at a
/// time, and zlib decodes the next block's header in the read that fills the buffer.
#[test]
fn inflate_at_read_boundaries() {
    let mut cases = Vec::new();
    for n in [65534usize, 65535, 65536, 131070] {
        let data = padded_config(n);
        let raw = stored_blocks(&data, false, &[0x06]);
        let mut e = Entry::new(b"config.ocio", &data, 8);
        e.raw = Some(raw);
        let mut entries = standard(8);
        entries[0] = e;
        cases.push((
            format!("stored blocks then a bad block type, {n}"),
            Archive::of(entries),
        ));
        // Control: a valid final block.
        let raw = stored_blocks(&data, true, &[]);
        let mut e = Entry::new(b"config.ocio", &data, 8);
        e.raw = Some(raw);
        let mut entries = standard(8);
        entries[0] = e;
        cases.push((format!("stored blocks, final, {n}"), Archive::of(entries)));
        // Truncated exactly at the end of the stored data (input runs out).
        let raw = stored_blocks(&data, false, &[]);
        let mut e = Entry::new(b"config.ocio", &data, 8);
        e.raw = Some(raw);
        let mut entries = standard(8);
        entries[0] = e;
        // put it last so that nothing follows it but the CD
        entries.rotate_left(1);
        cases.push((
            format!("stored blocks, no final block, last entry, {n}"),
            Archive::of(entries),
        ));
    }
    let cases: Vec<(&str, Archive)> = cases.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check(&cases);
}

/// End of central directory records: after trailing bytes (up to the 1 MiB minizip-ng searches),
/// a comment longer than the file, a fake record in the comment, wrong counts, offsets and
/// sizes, and zip64 end records with wrong fields.
#[test]
fn end_records() {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let base = Archive::of(standard(8)).bytes();
    // Trailing fake EOCD after the real one.
    let mut f = base.clone();
    f.extend_from_slice(&[0x50, 0x4b, 5, 6]);
    f.extend_from_slice(&[0u8; 18]);
    files.push(("trailing fake EOCD".into(), f));
    // Trailing garbage with no signature.
    let mut f = base.clone();
    f.extend_from_slice(b"trailing garbage after the end record");
    files.push(("trailing garbage".into(), f));
    // Trailing garbage near the 1 MiB search window.
    for t in [1_048_554usize, 1_048_555] {
        let mut f = base.clone();
        f.extend(std::iter::repeat_n(0u8, t));
        files.push((format!("{t} trailing zeros"), f));
    }
    // A comment longer than declared bytes.
    let mut a = Archive::of(standard(8));
    a.comment = b"0123456789".to_vec();
    let mut f = a.bytes();
    let n = f.len();
    f[n - 12] = 100; // comment length 100, only 10 bytes there
    files.push(("comment length past the end".into(), f));
    // A comment holding a fake end record pointing at the CD's second entry.
    let entries = standard(8);
    let mut locals = Vec::new();
    let mut central = Vec::new();
    let mut second_cd = 0usize;
    for (i, e) in entries.iter().enumerate() {
        let off = locals.len() as u32;
        locals.extend_from_slice(&e.local_record());
        if i == 1 {
            second_cd = central.len();
        }
        central.extend_from_slice(&e.central_record(off));
    }
    let cd_off = locals.len();
    let mut f = locals.clone();
    f.extend_from_slice(&central);
    let mut fake = Vec::new();
    put32(&mut fake, 0x0605_4b50);
    for v in [0u16, 0, 2, 2] {
        put16(&mut fake, v);
    }
    put32(&mut fake, (central.len() - second_cd) as u32);
    put32(&mut fake, (cd_off + second_cd) as u32);
    put16(&mut fake, 0);
    put32(&mut f, 0x0605_4b50);
    for v in [0u16, 0, 3, 3] {
        put16(&mut f, v);
    }
    put32(&mut f, central.len() as u32);
    put32(&mut f, cd_off as u32);
    put16(&mut f, fake.len() as u16);
    f.extend_from_slice(&fake);
    files.push(("fake end record in the comment".into(), f));
    // Counts.
    for (on, total) in [(0u16, 0u16), (0xffff, 0xffff), (5, 5), (0xffff, 3)] {
        let mut a = Archive::of(standard(8));
        a.count = Some((on, total));
        files.push((format!("count {on} {total}"), a.bytes()));
    }
    // CD offset 0xFFFFFFFF without zip64.
    let mut f = base.clone();
    let n = f.len();
    f[n - 6..n - 2].copy_from_slice(&u32::MAX.to_le_bytes());
    files.push(("cd offset u32::MAX".into(), f));
    // CD offset pointing at the second central header.
    let mut f = locals.clone();
    f.extend_from_slice(&central);
    put32(&mut f, 0x0605_4b50);
    for v in [0u16, 0, 3, 3] {
        put16(&mut f, v);
    }
    put32(&mut f, central.len() as u32);
    put32(&mut f, (cd_off + second_cd) as u32);
    put16(&mut f, 0);
    files.push(("cd offset at the second header".into(), f));
    // CD size wrong (too big) and offset wrong.
    for (dsize, doff) in [(10i64, 3i64), (-10, 3), (1000, 0), (0, 1), (-46, 7)] {
        let mut f = locals.clone();
        f.extend_from_slice(&central);
        put32(&mut f, 0x0605_4b50);
        for v in [0u16, 0, 3, 3] {
            put16(&mut f, v);
        }
        put32(&mut f, (central.len() as i64 + dsize) as u32);
        put32(&mut f, (cd_off as i64 + doff) as u32);
        put16(&mut f, 0);
        files.push((format!("cd size {dsize:+} offset {doff:+}"), f));
    }
    // Zip64 end records.
    let zip64 = |num: u64, num_cd: u64, size: i64, off: i64, loc_delta: i64, eocd16: (u16, u32)| {
        let mut f = locals.clone();
        f.extend_from_slice(&central);
        let z64_pos = f.len();
        put32(&mut f, 0x0606_4b50);
        f.extend_from_slice(&44u64.to_le_bytes());
        put16(&mut f, 45);
        put16(&mut f, 45);
        put32(&mut f, 0);
        put32(&mut f, 0);
        f.extend_from_slice(&num.to_le_bytes());
        f.extend_from_slice(&num_cd.to_le_bytes());
        f.extend_from_slice(&size.to_le_bytes());
        f.extend_from_slice(&off.to_le_bytes());
        put32(&mut f, 0x0706_4b50);
        put32(&mut f, 0);
        f.extend_from_slice(&((z64_pos as i64 + loc_delta) as u64).to_le_bytes());
        put32(&mut f, 1);
        put32(&mut f, 0x0605_4b50);
        for v in [0u16, 0, eocd16.0, eocd16.0] {
            put16(&mut f, v);
        }
        put32(&mut f, central.len() as u32);
        put32(&mut f, eocd16.1);
        put16(&mut f, 0);
        f
    };
    let cl = central.len() as i64;
    let co = cd_off as i64;
    files.push((
        "zip64 end".into(),
        zip64(3, 3, cl, co, 0, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, count only".into(),
        zip64(3, 3, cl, co, 0, (0xffff, co as u32)),
    ));
    files.push((
        "zip64 end, offset only".into(),
        zip64(3, 3, cl, co, 0, (3, u32::MAX)),
    ));
    files.push((
        "zip64 end, counts differ".into(),
        zip64(3, 2, cl, co, 0, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, negative offset".into(),
        zip64(3, 3, cl, -5, 0, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, negative size".into(),
        zip64(3, 3, -1, co, 0, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, locator off by 4".into(),
        zip64(3, 3, cl, co, 4, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, locator past the end".into(),
        zip64(3, 3, cl, co, 1 << 40, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, big size".into(),
        zip64(3, 3, 1 << 40, co, 0, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, wrong offset".into(),
        zip64(3, 3, cl, co + 9, 0, (0xffff, u32::MAX)),
    ));
    files.push((
        "zip64 end, count 0".into(),
        zip64(0, 0, cl, co, 0, (0xffff, u32::MAX)),
    ));
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

fn cuts(label: &str, full: &[u8]) -> Vec<(String, Vec<u8>)> {
    (0..full.len())
        .map(|i| (format!("{label} cut at {i}"), full[..i].to_vec()))
        .collect()
}

/// A stored archive cut at each of its bytes.
#[test]
fn every_cut_of_a_stored_archive() {
    let full = Archive::of(standard(0)).bytes();
    let files = cuts("stored", &full);
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// A deflated archive cut at each of its bytes.
#[test]
fn every_cut_of_a_deflated_archive() {
    let full = Archive::of(standard(8)).bytes();
    let files = cuts("deflated", &full);
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// An archive with data descriptors cut at each of its bytes.
#[test]
fn every_cut_of_an_archive_with_descriptors() {
    let mut entries = standard(8);
    for e in &mut entries {
        e.descriptor = true;
    }
    entries[1].method = 0;
    let full = Archive::of(entries).bytes();
    let files = cuts("descriptors", &full);
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// Names that resolve the way minizip-ng's wildcard comparison does (`./`, leading and double
/// slashes, case, backslashes, trailing spaces, `*` and `?`, high bytes, empty names), configs
/// found first or second, and LUT references with other bytes.
#[test]
fn more_names() {
    let n = |names: [&[u8]; 3]| {
        Archive::of(vec![
            Entry::new(names[0], CONFIG, 8),
            Entry::new(names[1], &lut("2.0"), 8),
            Entry::new(names[2], &lut("3.0"), 8),
        ])
    };
    let mut cases = vec![
        (
            "dot slash",
            n([b"./config.ocio", b"./luts/a.spi1d", b"luts/sub/b.spi1d"]),
        ),
        (
            "leading slash",
            n([b"/config.ocio", b"/luts/a.spi1d", b"luts/sub/b.spi1d"]),
        ),
        (
            "double slash",
            n([b"config.ocio", b"luts//a.spi1d", b"luts/sub//b.spi1d"]),
        ),
        (
            "dot in middle",
            n([
                b"config.ocio",
                b"luts/./a.spi1d",
                b"luts/sub/../sub/b.spi1d",
            ]),
        ),
        (
            "upper backslashes",
            n([b"CONFIG.OCIO", b"LUTS\\A.SPI1D", b"LUTS\\SUB\\B.SPI1D"]),
        ),
        (
            "trailing space",
            n([b"config.ocio ", b"luts/a.spi1d ", b"luts/sub/b.spi1d"]),
        ),
        (
            "star suffix",
            n([b"config.ocio*", b"luts/a.spi1d**", b"luts/sub/b*"]),
        ),
        (
            "star prefix",
            n([b"*config.ocio", b"*a.spi1d", b"*/b.spi1d"]),
        ),
        (
            "question marks",
            n([b"config.oci?", b"luts/?.spi1d", b"luts/sub/b.spi1d"]),
        ),
        (
            "high bytes",
            n([b"config.ocio", b"luts/\xff\xfe.spi1d", b"luts/sub/b.spi1d"]),
        ),
        ("empty name", n([b"config.ocio", b"", b"luts/sub/b.spi1d"])),
    ];
    // A star entry first: the config comes from it.
    cases.push((
        "star first",
        Archive::of(vec![
            Entry::new(b"*", CONFIG, 8),
            Entry::new(b"config.ocio", b"garbage: [", 8),
            Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
        ]),
    ));
    // Two configs, the first one empty (readEntryBuffer throws), the first one unreadable.
    cases.push((
        "empty config first",
        Archive::of(vec![
            Entry::new(b"config.ocio", b"", 0),
            Entry::new(b"config.ocio", CONFIG, 8),
            Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
            Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
        ]),
    ));
    cases.push((
        "unknown-method config first",
        Archive::of(vec![
            Entry::new(b"config.ocio", CONFIG, 12),
            Entry::new(b"config.ocio", CONFIG, 8),
            Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
        ]),
    ));
    cases.push((
        "config in upper case second",
        Archive::of(vec![
            Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
            Entry::new(b"CONFIG.OCIO", CONFIG, 8),
            Entry::new(b"config.ocio", b"x: [", 8),
        ]),
    ));
    check(&cases);

    // Configs whose src names hold other bytes.
    let cfg = |src: &[u8]| {
        let mut c = b"ocio_profile_version: 2\nroles: {default: raw}\ncolorspaces:\n  - !<ColorSpace> {name: raw}\n  - !<ColorSpace>\n    name: a\n    from_scene_reference: !<FileTransform> {src: \"".to_vec();
        c.extend_from_slice(src);
        c.extend_from_slice(b"\"}\n  - !<ColorSpace> {name: b}\n");
        c
    };
    let mut files = Vec::new();
    for (label, src, name) in [
        // Not `\xc4\x9a` against `\xc4\x8a`, which the Windows wheel folds by code page 1252
        // under Python and the port doesn't (deviation D-4).
        ("utf8 same", &b"\xc3\xa9.spi1d"[..], &b"\xc3\xa9.spi1d"[..]),
        (
            "latin upper lower",
            &b"\xc3\xa0.spi1d"[..],
            &b"\xc3\x80.spi1d"[..],
        ),
        ("dir dotdot", &b"x/../a.spi1d"[..], &b"a.spi1d"[..]),
        ("abs path", &b"/a.spi1d"[..], &b"a.spi1d"[..]),
        ("backslash src", &b"sub\\\\a.spi1d"[..], &b"sub/a.spi1d"[..]),
    ] {
        let a = Archive::of(vec![
            Entry::new(b"config.ocio", &cfg(src), 8),
            Entry::new(name, &lut("2.0"), 8),
        ]);
        files.push((label, a.bytes()));
    }
    check_files(&files);
}

/// Entries whose local extra field, sizes, CRC, zip64, NTFS or UNIX1 fields, disk or local magic
/// are unusual, and an archive after a prefix whose local offsets count it.
#[test]
fn more_entries() {
    let mut cases = Vec::new();
    // Local header names differ from the central ones.
    let mut a = Archive::of(standard(8));
    a.entries[1].local_extra = vec![0x34, 0x12, 3, 0, 1, 2, 3];
    cases.push(("local extra only", a));
    // Stored with a huge compressed size.
    cases.push((
        "stored, compressed size huge",
        with_entry(1, &|e| {
            e.method = 0;
            e.compressed_size = Some(0xffff_fff0);
        }),
    ));
    cases.push((
        "deflated, compressed size huge",
        with_entry(1, &|e| {
            e.compressed_size = Some(0xffff_fff0);
        }),
    ));
    cases.push((
        "encrypted deflated, compressed size huge",
        with_entry(1, &|e| {
            e.flag = 1;
            e.compressed_size = Some(0xffff_fff0);
        }),
    ));
    cases.push((
        "encrypted deflated, compressed size short",
        with_entry(1, &|e| {
            e.flag = 1;
            e.compressed_size = Some(5);
        }),
    ));
    cases.push((
        "uncompressed size i32::MAX",
        with_entry(1, &|e| e.uncompressed_size = Some(0x7fff_ffff)),
    ));
    cases.push((
        "uncompressed size u32::MAX no zip64",
        with_entry(1, &|e| e.uncompressed_size = Some(u32::MAX)),
    ));
    cases.push((
        "config uncompressed 1",
        with_entry(0, &|e| e.uncompressed_size = Some(1)),
    ));
    // zip64 extra with negative values.
    cases.push((
        "zip64 negative size",
        with_entry(1, &|e| {
            e.uncompressed_size = Some(u32::MAX);
            let mut extra = vec![0x01, 0x00, 8, 0];
            extra.extend_from_slice(&(-1i64).to_le_bytes());
            e.cd_extra = extra;
        }),
    ));
    cases.push((
        "zip64 offset",
        with_entry(1, &|e| {
            let mut extra = vec![0x01, 0x00, 8, 0];
            extra.extend_from_slice(&5u64.to_le_bytes());
            e.cd_extra = extra;
        }),
    ));
    cases.push((
        "zip64 field of 4 bytes",
        with_entry(1, &|e| {
            e.uncompressed_size = Some(u32::MAX);
            e.cd_extra = vec![0x01, 0x00, 4, 0, 9, 0, 0, 0];
        }),
    ));
    cases.push((
        "zip64 field reading past extra",
        with_entry(1, &|e| {
            e.uncompressed_size = Some(u32::MAX);
            e.compressed_size = Some(u32::MAX);
            let size = lut("2.0").len() as u64;
            let mut extra = vec![0x01, 0x00, 8, 0];
            extra.extend_from_slice(&size.to_le_bytes());
            e.cd_extra = extra;
        }),
    ));
    cases.push((
        "ntfs short attribute",
        with_entry(1, &|e| {
            let mut extra = vec![0x0a, 0x00, 8, 0, 0, 0, 0, 0, 1, 0, 24, 0];
            extra.extend_from_slice(&[0x11; 4]);
            e.cd_extra = extra;
        }),
    ));
    cases.push((
        "unix1 short",
        with_entry(1, &|e| {
            e.cd_extra = vec![0x0d, 0x00, 12, 0, 1, 2, 3, 4];
        }),
    ));
    cases.push((
        "disk 0xffff with zip64 disk",
        with_entry(1, &|e| {
            e.disk = 0xffff;
            let mut extra = vec![0x01, 0x00, 4, 0];
            extra.extend_from_slice(&0u32.to_le_bytes());
            e.cd_extra = extra;
        }),
    ));
    // Local header magic wrong.
    let mut a = Archive::of(standard(8));
    a.entries[1].local_extra = Vec::new();
    let mut bytes = a.bytes();
    let off = a.entries[0].local_record().len();
    bytes[off] = b'X';
    let mut files: Vec<(String, Vec<u8>)> = cases
        .iter()
        .map(|(l, a)| (l.to_string(), a.bytes()))
        .collect();
    files.push(("local magic wrong".into(), bytes));
    // Prefix with absolute local offsets but relative CD offset: the shift then fallback.
    let mut a = Archive::of(standard(8));
    let prefix = b"PK, a stub of 21 bytes".to_vec();
    a.prefix = prefix.clone();
    let mut f = Vec::new();
    f.extend_from_slice(&prefix);
    let mut central = Vec::new();
    for e in &a.entries {
        let off = f.len() as u32; // absolute
        f.extend_from_slice(&e.local_record());
        central.extend_from_slice(&e.central_record(off));
    }
    let cd_off = (f.len() - prefix.len()) as u32; // relative
    f.extend_from_slice(&central);
    put32(&mut f, 0x0605_4b50);
    for v in [0u16, 0, 3, 3] {
        put16(&mut f, v);
    }
    put32(&mut f, central.len() as u32);
    put32(&mut f, cd_off);
    put16(&mut f, 0);
    files.push(("prefix, absolute local offsets".into(), f));
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// Archives without an end record, recovered from their local headers: descriptors, magic
/// numbers in the data, wrong local sizes, garbage between entries, local names with a NUL or
/// empty, links, zip64 local fields, and a truncated archive.
#[test]
fn more_recoveries() {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let strip_end = |a: &Archive| {
        let b = a.bytes();
        b[..b.len() - 22].to_vec()
    };
    // Descriptors whose data holds local header magic.
    let mut entries = standard(8);
    let mut data = lut("2.0");
    data.extend_from_slice(b"# PK\x03\x04 inside\n");
    entries[1] = Entry::new(b"luts/a.spi1d", &data, 0);
    for e in &mut entries {
        e.descriptor = true;
    }
    files.push((
        "descriptors, magic in stored data".into(),
        strip_end(&Archive::of(entries.clone())),
    ));
    // Same with the descriptor magic in data too.
    let mut entries2 = entries.clone();
    let mut data = lut("2.0");
    data.extend_from_slice(b"# PK\x07\x08 inside PK\x03\x04\n");
    entries2[1] = Entry::new(b"luts/a.spi1d", &data, 0);
    entries2[1].descriptor = true;
    files.push((
        "descriptors, both magics in data".into(),
        strip_end(&Archive::of(entries2)),
    ));
    // No descriptor, compressed size 0 in local header (deflate).
    let mut entries3 = standard(8);
    entries3[1].compressed_size = Some(0);
    files.push((
        "local compressed size 0".into(),
        strip_end(&Archive::of(entries3)),
    ));
    // Huge local compressed size.
    let mut entries4 = standard(8);
    entries4[1].compressed_size = Some(0x7fff_0000);
    files.push((
        "local compressed size huge".into(),
        strip_end(&Archive::of(entries4)),
    ));
    // Garbage between entries.
    let a = Archive::of(standard(8));
    let mut f = Vec::new();
    for e in &a.entries {
        f.extend_from_slice(&e.local_record());
        f.extend_from_slice(b"garbage between the entries");
    }
    files.push(("garbage between entries".into(), f));
    // A local extra field longer than its data (write_header's copy fails).
    let mut entries5 = standard(8);
    entries5[1].local_extra = vec![0x34, 0x12, 200, 0, 1, 2];
    files.push((
        "local field past extra".into(),
        strip_end(&Archive::of(entries5)),
    ));
    // Local extra with a CDCD field and a name of __cdcd__? No: name with NUL in local header.
    let mut entries6 = standard(8);
    entries6[1].name = b"luts/a.spi1d\0junk".to_vec();
    files.push((
        "local name with NUL".into(),
        strip_end(&Archive::of(entries6)),
    ));
    // Empty local name.
    let mut entries7 = standard(8);
    entries7.insert(1, Entry::new(b"", b"x", 0));
    files.push(("empty local name".into(), strip_end(&Archive::of(entries7))));
    // UNIX1 local link with NUL in the middle.
    let mut entries8 = standard(8);
    let mut extra = vec![0x0d, 0x00, 18, 0];
    extra.extend_from_slice(&[0; 12]);
    extra.extend_from_slice(b"li\0nk!");
    entries8[1].local_extra = extra;
    files.push((
        "local link with NUL".into(),
        strip_end(&Archive::of(entries8)),
    ));
    // zip64 local field then descriptors (64-bit descriptors for later entries).
    let mut entries9 = standard(8);
    let mut extra = vec![0x01, 0x00, 16, 0];
    extra.extend_from_slice(&[0; 16]);
    entries9[0].local_extra = extra;
    entries9[0].descriptor = true;
    entries9[1].descriptor = true;
    files.push((
        "zip64 local then descriptors".into(),
        strip_end(&Archive::of(entries9)),
    ));
    // A central header magic within data, no EOCD.
    let mut entries10 = standard(8);
    let mut data = lut("2.0");
    data.extend_from_slice(b"# PK\x01\x02\n");
    entries10[2] = Entry::new(b"luts/sub/b.spi1d", &data, 0);
    entries10[2].descriptor = true;
    files.push((
        "central magic in last data".into(),
        strip_end(&Archive::of(entries10)),
    ));
    // Only a local header of the config, deflate truncated.
    let full = Archive::of(standard(8)).bytes();
    files.push(("first 100 bytes".into(), full[..100].to_vec()));
    // No EOCD; the zipped CD as first local entry.
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// Zipped central directories: a short `0xcdcd` field (which wraps minizip-ng's search limit,
/// docs/improvements.md, I-213), one after another field, an unknown method, sizes that are
/// negative as an `int` or wrong, names that aren't `__cdcd__` exactly, garbage after the inner
/// directory, and a recovered archive whose first entry is `__cdcd__`.
#[test]
fn more_zipped_central_directories() {
    let entries = standard(8);
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    // CDCD field shorter than 8 bytes: only 2 bytes of data in the extra field.
    files.push((
        "cdcd field of 2".into(),
        zipped_cd_extra(&entries, 8, vec![0xcd, 0xcd, 2, 0, 3, 0], true),
    ));
    files.push((
        "cdcd field after another".into(),
        zipped_cd_extra(
            &entries,
            8,
            {
                let mut x = vec![0x34, 0x12, 2, 0, 9, 9, 0xcd, 0xcd, 8, 0];
                x.extend_from_slice(&3u64.to_le_bytes());
                x
            },
            false,
        ),
    ));
    // Unknown method.
    let mut f = zipped_cd(&entries, 8, Some(3), true);
    let first_c = outer_cd(&f);
    f[first_c + 10] = 12;
    files.push(("cdcd unknown method".into(), f));
    // Big uncompressed size (negative as int32).
    let mut f = zipped_cd(&entries, 8, Some(3), true);
    let first_c = outer_cd(&f);
    f[first_c + 24..first_c + 28].copy_from_slice(&0x8000_0000u32.to_le_bytes());
    files.push(("cdcd size 2^31".into(), f));
    // Uncompressed size larger than data.
    let mut f = zipped_cd(&entries, 0, Some(3), true);
    let first_c = outer_cd(&f);
    let us = u32::from_le_bytes(f[first_c + 24..first_c + 28].try_into().unwrap());
    f[first_c + 24..first_c + 28].copy_from_slice(&(us + 50).to_le_bytes());
    files.push(("cdcd size +50 stored".into(), f));
    let mut f = zipped_cd(&entries, 8, Some(3), true);
    let first_c = outer_cd(&f);
    let us = u32::from_le_bytes(f[first_c + 24..first_c + 28].try_into().unwrap());
    f[first_c + 24..first_c + 28].copy_from_slice(&(us + 50).to_le_bytes());
    files.push(("cdcd size +50 deflated".into(), f));
    let mut f = zipped_cd(&entries, 8, Some(3), true);
    let first_c = outer_cd(&f);
    let us = u32::from_le_bytes(f[first_c + 24..first_c + 28].try_into().unwrap());
    f[first_c + 24..first_c + 28].copy_from_slice(&(us - 50).to_le_bytes());
    files.push(("cdcd size -50 deflated".into(), f));
    // Name with a NUL after __cdcd__ and upper case.
    let mut inner = Vec::new();
    let mut out = Vec::new();
    for e in &entries {
        let off = out.len() as u32;
        out.extend_from_slice(&e.local_record());
        inner.extend_from_slice(&e.central_record(off));
    }
    for (label, name) in [
        ("cdcd NUL name", &b"__cdcd__\0x"[..]),
        ("cdcd upper", &b"__CDCD__"[..]),
    ] {
        let mut f = out.clone();
        let mut cd = Entry::new(name, &inner, 8);
        cd.cd_extra = vec![0xcd, 0xcd, 8, 0];
        cd.cd_extra.extend_from_slice(&3u64.to_le_bytes());
        let off = f.len() as u32;
        f.extend_from_slice(&cd.local_record());
        let central = cd.central_record(off);
        let cdo = f.len() as u32;
        f.extend_from_slice(&central);
        put32(&mut f, 0x0605_4b50);
        for v in [0u16, 0, 1, 1] {
            put16(&mut f, v);
        }
        put32(&mut f, central.len() as u32);
        put32(&mut f, cdo);
        put16(&mut f, 0);
        files.push((label.into(), f));
    }
    // Inner CD with garbage after, and inner CD listing __cdcd__ first again.
    {
        let mut f = out.clone();
        let mut inner2 = inner.clone();
        inner2.extend_from_slice(b"PK\x01\x02 garbage");
        let mut cd = Entry::new(b"__cdcd__", &inner2, 8);
        cd.cd_extra = vec![0xcd, 0xcd, 8, 0];
        cd.cd_extra.extend_from_slice(&3u64.to_le_bytes());
        let off = f.len() as u32;
        f.extend_from_slice(&cd.local_record());
        let central = cd.central_record(off);
        let cdo = f.len() as u32;
        f.extend_from_slice(&central);
        put32(&mut f, 0x0605_4b50);
        for v in [0u16, 0, 1, 1] {
            put16(&mut f, v);
        }
        put32(&mut f, central.len() as u32);
        put32(&mut f, cdo);
        put16(&mut f, 0);
        files.push(("cdcd inner garbage".into(), f));
    }
    // A recovered archive whose first local entry is __cdcd__ with the CDCD field in its
    // local extra.
    {
        let mut f = Vec::new();
        let mut inner3 = Vec::new();
        let mut cd = Entry::new(b"__cdcd__", b"", 0);
        cd.local_extra = vec![0xcd, 0xcd, 8, 0];
        cd.local_extra.extend_from_slice(&3u64.to_le_bytes());
        // The inner CD needs the offsets of the entries after it, so build twice.
        let mut cd_len = 0;
        for _ in 0..2 {
            inner3.clear();
            let mut off = cd_len as u32;
            let mut locals = Vec::new();
            for e in &entries {
                inner3.extend_from_slice(&e.central_record(off));
                let l = e.local_record();
                off += l.len() as u32;
                locals.extend_from_slice(&l);
            }
            cd.data = inner3.clone();
            cd.raw = None;
            let lr = cd.local_record();
            cd_len = lr.len();
            f = lr;
            f.extend_from_slice(&locals);
        }
        files.push(("recovered with cdcd first".into(), f));
    }
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

fn outer_cd(f: &[u8]) -> usize {
    let n = f.len();
    u32::from_le_bytes(f[n - 6..n - 2].try_into().unwrap()) as usize
}

fn zipped_cd_extra(entries: &[Entry], method: u16, extra: Vec<u8>, also_listed: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut inner = Vec::new();
    for e in entries {
        let offset = out.len() as u32;
        out.extend_from_slice(&e.local_record());
        inner.extend_from_slice(&e.central_record(offset));
    }
    let mut cd_entry = Entry::new(b"__cdcd__", &inner, method);
    cd_entry.cd_extra = extra;
    let cd_entry_offset = out.len() as u32;
    out.extend_from_slice(&cd_entry.local_record());
    let mut central = cd_entry.central_record(cd_entry_offset);
    let mut n = 1u16;
    if also_listed {
        central.extend_from_slice(&inner);
        n += entries.len() as u16;
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    for v in [0u16, 0, n, n] {
        put16(&mut out, v);
    }
    put32(&mut out, central.len() as u32);
    put32(&mut out, cd_offset);
    put16(&mut out, 0);
    out
}

/// Recovered archives whose zip64 local sizes make minizip-ng's seek past the entry fail, the
/// buffered stream's position wrapped (docs/improvements.md, I-214), first in the archive or
/// after 40,000 bytes.
#[test]
fn recovery_after_failed_seeks() {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let strip_end = |a: &Archive| {
        let b = a.bytes();
        b[..b.len() - 22].to_vec()
    };
    for (label, index, csize) in [
        (
            "config, zip64 csize overflowing",
            0usize,
            0x7fff_ffff_ffff_fff0i64,
        ),
        ("lut a, zip64 csize overflowing", 1, 0x7fff_ffff_ffff_fff0),
        ("lut b, zip64 csize overflowing", 2, 0x7fff_ffff_ffff_fff0),
        ("config, zip64 csize 2^40", 0, 1 << 40),
        ("config, zip64 csize i64::MAX - 40", 0, i64::MAX - 40),
        ("config, zip64 csize i64::MAX - 20000", 0, i64::MAX - 20000),
    ] {
        let mut entries = standard(8);
        entries[index].compressed_size = Some(u32::MAX);
        let mut extra = vec![0x01, 0x00, 8, 0];
        extra.extend_from_slice(&csize.to_le_bytes());
        entries[index].local_extra = extra;
        let a = Archive::of(entries);
        files.push((label.to_string(), strip_end(&a)));
        // Also with a big first entry so the buffer doesn't reach the end of the file.
        let mut entries = standard(8);
        entries.insert(0, Entry::new(b"pad.bin", &vec![b'p'; 40000], 0));
        let index = index + 1;
        entries[index].compressed_size = Some(u32::MAX);
        let mut extra = vec![0x01, 0x00, 8, 0];
        extra.extend_from_slice(&csize.to_le_bytes());
        entries[index].local_extra = extra;
        let a = Archive::of(entries);
        files.push((format!("{label}, after 40000 bytes"), strip_end(&a)));
    }
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// Writes `content` as `<stem>.ocioz` and each companion as `<stem><suffix>`; the archive's path.
fn write_with(content: &[u8], companions: &[(&str, Vec<u8>)]) -> String {
    let mut all = content.to_vec();
    for (s, c) in companions {
        all.extend_from_slice(s.as_bytes());
        all.extend_from_slice(c);
    }
    let stem = format!("c{}_{:016x}", all.len(), ocio::caching::msvc_fnv1a(&all));
    let dir = files_dir();
    for (s, c) in companions {
        std::fs::write(dir.join(format!("{stem}{s}")), c).unwrap();
    }
    let path = dir.join(format!("{stem}.ocioz"));
    std::fs::write(&path, content).unwrap();
    path.to_str().unwrap().replace('\\', "/")
}

/// A split archive's case: its label, the archive, and its parts (a suffix and the bytes).
type SplitCase = (String, Vec<u8>, Vec<(&'static str, Vec<u8>)>);

/// Split archives: the parts `.z01` and `.z02` next to the archive, with and without their
/// disk header, an entry or the central directory on another disk.
#[test]
fn split_disks() {
    let _lock = CACHES.lock().unwrap_or_else(PoisonError::into_inner);
    let mut cases: Vec<SplitCase> = Vec::new();
    // C1: truncated archive + .z01 holding another archive's local entries.
    let mine = Archive::of(standard(8)).bytes();
    let mut other = vec![0x50, 0x4b, 7, 8];
    let mut ents = standard(8);
    ents[1] = Entry::new(b"luts/a.spi1d", &lut("9.0"), 8);
    for e in &ents {
        other.extend_from_slice(&e.local_record());
    }
    cases.push((
        "truncated + z01 with magic".into(),
        mine[..mine.len() - 22].to_vec(),
        vec![(".z01", other.clone())],
    ));
    cases.push((
        "truncated + z01 without magic".into(),
        mine[..mine.len() - 22].to_vec(),
        vec![(".z01", other[4..].to_vec())],
    ));
    cases.push((
        "complete + z01 with magic".into(),
        mine.clone(),
        vec![(".z01", other.clone())],
    ));
    // C2: a LUT entry on disk 1, its local header in .z02 at offset 0.
    let mut a = Archive::of(standard(8));
    a.entries[1].disk = 1;
    let lut_local = a.entries[1].local_record();
    // The archive's own CD gives the LUT's offset as its offset in the main file; put the
    // local record in z02 at that same offset.
    let off = a.entries[0].local_record().len();
    let mut z02 = vec![b'z'; off];
    z02.extend_from_slice(&lut_local);
    cases.push((
        "lut on disk 1 with z02".into(),
        a.bytes(),
        vec![(".z02", z02.clone())],
    ));
    // C3: CD on disk 1, entries on disk 0 in z01 (with magic) at the same offsets + 4.
    let mut b = Archive::of(standard(8));
    b.disk_with_cd = 1;
    let mut z01 = vec![0x50, 0x4b, 7, 8];
    z01.extend_from_slice(&b.bytes());
    cases.push((
        "cd on disk 1, entries in z01".into(),
        b.bytes(),
        vec![(".z01", z01.clone())],
    ));
    // C4: z01 and z02 both, truncated main.
    cases.push((
        "truncated + z01 + z02".into(),
        mine[..mine.len() - 22].to_vec(),
        vec![(".z01", other.clone()), (".z02", ents[2].local_record())],
    ));

    let paths: Vec<String> = cases.iter().map(|(_, a, c)| write_with(a, c)).collect();
    let mut calls = Vec::new();
    for path in &paths {
        calls.push(BatchCall {
            cmd: "with_files",
            args: json!({"files": {}, "command": "config_calls", "args": {
                "config": {"file": path},
                "calls": [{"call": "serialize"}, {"call": "getCacheID"}]}}),
            blobs: Vec::new(),
        });
        for space in LUT_SPACES {
            calls.push(BatchCall {
                cmd: "with_files",
                args: json!({"files": {}, "command": "config_processor", "args": {
                    "config": {"file": path},
                    "overload": {"names": ["raw", space]}}}),
                blobs: Vec::new(),
            });
        }
    }
    let wheel = Oracle::get().batch(&calls, false);
    let mut wheel = wheel
        .into_iter()
        .map(|w| w.expect("an oracle request").result);
    let mut failures = Vec::new();
    for ((label, _, _), path) in cases.iter().zip(&paths) {
        let (wheel_outcome, _) = wheel_config(&wheel.next().unwrap());
        clear_all_caches();
        let port_outcome: Outcome = Config::create_from_file(path.as_bytes())
            .and_then(|config| Ok(vec![config.serialize()?, config.cache_id()?.into_bytes()]))
            .map_err(|e| e.what().to_vec());
        if wheel_outcome != port_outcome {
            failures.push(format!(
                "{label}: config\n  wheel {}\n  port  {}",
                show(&wheel_outcome),
                show(&port_outcome)
            ));
        }
        for space in LUT_SPACES {
            let wheel_outcome = wheel_processor(&wheel.next().unwrap());
            clear_all_caches();
            let port_outcome = Config::create_from_file(path.as_bytes())
                .and_then(|config| config.processor_between("raw", space))
                .and_then(|processor| Ok(vec![processor.cache_id()?.into_bytes()]))
                .map_err(|e| e.what().to_vec());
            if wheel_outcome != port_outcome {
                failures.push(format!(
                    "{label}: raw to {space}\n  wheel {}\n  port  {}",
                    show(&wheel_outcome),
                    show(&port_outcome)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// LUTs whose names repeat in other cases or with other slashes: which one's CRC the cache ID
/// hashes.
#[test]
fn hashes_of_repeated_names() {
    let cases = vec![
        (
            "two case variants of a LUT",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
                Entry::new(b"LUTS/A.SPI1D", &lut("5.0"), 8),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
            ]),
        ),
        (
            "slash variants of a LUT",
            Archive::of(vec![
                Entry::new(b"config.ocio", CONFIG, 8),
                Entry::new(b"luts\\sub\\b.spi1d", &lut("6.0"), 8),
                Entry::new(b"luts/a.spi1d", &lut("2.0"), 8),
                Entry::new(b"luts/sub/b.spi1d", &lut("3.0"), 8),
            ]),
        ),
    ];
    check(&cases);
}

/// Entries with comments in the central directory: minizip-ng steps from header to header by
/// the name's, extra field's and comment's sizes.
#[test]
fn entry_comments() {
    let mut entries = standard(8);
    entries[0].comment = b"the config".to_vec();
    entries[1].comment = vec![b'c'; 300];
    check(&[("entry comments", Archive::of(entries))]);
}

/// Recovered archives with data descriptors after an entry whose local header has a zip64
/// field: minizip-ng reads every later descriptor as zip64's (its `zip64` isn't reset from
/// entry to entry, docs/improvements.md, I-210), so a 32-bit descriptor is misread.
#[test]
fn zip64_descriptors_after_a_zip64_entry() {
    let strip_end = |a: &Archive| {
        let b = a.bytes();
        b[..b.len() - 22].to_vec()
    };
    let mut entries = standard(8);
    let mut pad = Entry::new(b"pad.txt", b"padding", 0);
    pad.local_extra = vec![0x01, 0x00, 16, 0];
    pad.local_extra.extend_from_slice(&[0; 16]);
    pad.descriptor = true;
    pad.descriptor64 = true;
    entries.insert(1, pad);
    entries[2].descriptor = true;
    entries[3].descriptor = true;
    let mut sixty_four = entries.clone();
    sixty_four[2].descriptor64 = true;
    sixty_four[3].descriptor64 = true;
    check_files(&[
        (
            "32-bit descriptors after a zip64 one",
            strip_end(&Archive::of(entries)),
        ),
        (
            "zip64 descriptors throughout",
            strip_end(&Archive::of(sixty_four)),
        ),
    ]);
}

/// The end record after 100 KiB of trailing bytes, within the 1 MiB minizip-ng searches (past
/// the 64 KiB of a zip comment), in an archive whose local names aren't the central
/// directory's: read from the central directory, the LUTs are found; recovered from the local
/// headers, they wouldn't be.
#[test]
fn end_record_far_from_the_end() {
    let mut a = Archive::of(standard(8));
    a.entries[1].local_name = Some(b"luts/x.spi1d".to_vec());
    let mut f = a.bytes();
    f.extend(std::iter::repeat_n(b'.', 100 * 1024));
    check_files(&[("100 KiB after the end record", f)]);
}

/// A header's fields read past its extra field: minizip-ng reads each header into the same
/// memory stream, so a zip64 field of 8 bytes that both sizes are read from reads the second
/// past the field, into what an earlier header's longer name left there (a fresh stream would
/// end, and the read fail).
#[test]
fn stale_header_bytes() {
    let mut entries = standard(8);
    let pad_name = [b"pad/".as_slice(), &[b'7'; 60], b".txt"].concat();
    entries.insert(1, Entry::new(&pad_name, b"padding", 0));
    let size = entries[2].data.len() as u64;
    entries[2].uncompressed_size = Some(u32::MAX);
    entries[2].compressed_size = Some(u32::MAX);
    entries[2].cd_extra = vec![0x01, 0x00, 8, 0];
    entries[2].cd_extra.extend_from_slice(&size.to_le_bytes());
    check(&[("a zip64 field read past its end", Archive::of(entries))]);
}

/// LUT entries whose sizes are at OpenColorIO's limit of 256 MiB and past it
/// (`readEntryBuffer`, OCIOZArchive.cpp:390-403 @ v2.5.2): the first is read into a buffer of
/// that size (its data short of it, the rest zeros), the second refused.
#[test]
fn entry_size_limit() {
    let cases: Vec<(String, Archive)> = [256u32 << 20, (256 << 20) + 1]
        .iter()
        .map(|&size| {
            (
                format!("a LUT of {size} bytes"),
                with_entry(1, &|e| {
                    e.method = 0;
                    e.uncompressed_size = Some(size);
                }),
            )
        })
        .collect();
    let cases: Vec<(&str, Archive)> = cases.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check(&cases);
}

/// Recovered archives whose zip64 local sizes (2^43 to 2^60 bytes) make minizip-ng's seek past
/// the entry go past the largest file the file system holds: on Linux `fseeko`'s `lseek` fails
/// there (16 TiB on ext4), and the reader reads on from the end of its last read
/// (docs/improvements.md, I-214); on Windows `SetFilePointerEx` takes it. First in the archive
/// or after 40,000 bytes. (The verifier's probe p13 of the fix review.)
#[test]
fn recovery_after_seeks_past_the_file_system() {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let strip_end = |a: &Archive| {
        let b = a.bytes();
        b[..b.len() - 22].to_vec()
    };
    for csize in [1i64 << 43, 1i64 << 45, 1i64 << 50, 1i64 << 60] {
        for index in [0usize, 1] {
            let mut entries = standard(8);
            entries[index].compressed_size = Some(u32::MAX);
            let mut extra = vec![0x01, 0x00, 8, 0];
            extra.extend_from_slice(&csize.to_le_bytes());
            entries[index].local_extra = extra;
            files.push((
                format!("entry {index}, zip64 size {csize}"),
                strip_end(&Archive::of(entries)),
            ));
            let mut entries = standard(8);
            entries.insert(0, Entry::new(b"pad.bin", &[b'p'; 40000], 0));
            entries[index + 1].compressed_size = Some(u32::MAX);
            let mut extra = vec![0x01, 0x00, 8, 0];
            extra.extend_from_slice(&csize.to_le_bytes());
            entries[index + 1].local_extra = extra;
            files.push((
                format!("entry {index}, zip64 size {csize}, after 40000 bytes"),
                strip_end(&Archive::of(entries)),
            ));
        }
    }
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// As `recovery_after_seeks_past_the_file_system`, with more than a read buffer (32,767
/// bytes) of data after the entry whose size is huge, so that a seek that fails reads on
/// before the end of the file. (The verifier's probe p14.)
#[test]
fn recovery_after_seeks_past_the_file_system_long_tail() {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let strip_end = |a: &Archive| {
        let b = a.bytes();
        b[..b.len() - 22].to_vec()
    };
    for csize in [
        1i64 << 43,
        1i64 << 45,
        1i64 << 50,
        1i64 << 60,
        0x7fff_ffff_ffff_fff0,
    ] {
        let mut entries = standard(8);
        entries.insert(2, Entry::new(b"pad.bin", &[b'p'; 70000], 0));
        entries[1].compressed_size = Some(u32::MAX);
        let mut extra = vec![0x01, 0x00, 8, 0];
        extra.extend_from_slice(&csize.to_le_bytes());
        entries[1].local_extra = extra;
        files.push((
            format!("lut a, zip64 size {csize}, 70000 bytes after"),
            strip_end(&Archive::of(entries)),
        ));
    }
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}

/// A recovered archive (no end record) whose stored entries hold a local header's magic
/// number in their data, with their sizes in their local headers: minizip-ng seeks past each
/// entry's data from where its header ends (within the buffered stream's read buffer) before
/// it searches for the next header, so the magic in the data isn't taken for one.
#[test]
fn recovery_seeks_past_stored_data() {
    let strip_end = |a: &Archive| {
        let b = a.bytes();
        b[..b.len() - 22].to_vec()
    };
    let mut entries = standard(0);
    let mut data = b"padding PK\x03\x04 and more padding".to_vec();
    data.extend_from_slice(&[b'.'; 100]);
    entries.insert(1, Entry::new(b"pad.bin", &data, 0));
    let mut lut_a = lut("2.0");
    lut_a.extend_from_slice(b"# PK\x03\x04 in a comment\n");
    entries[2] = Entry::new(b"luts/a.spi1d", &lut_a, 0);
    check_files(&[(
        "stored data holding PK\x03\x04",
        strip_end(&Archive::of(entries)),
    )]);
}

/// Search paths that aren't normalized (`./sub/../luts`, `luts/./`): OpenColorIO normalizes
/// the path of a LUT it looks for in an archive (`getLutData`, OCIOZArchive.cpp:599 @ v2.5.2)
/// before comparing it with the entries' names.
#[test]
fn lut_paths_are_normalized() {
    let config = |search_path: &str| {
        String::from_utf8(CONFIG.to_vec())
            .unwrap()
            .replace("search_path: luts", &format!("search_path: {search_path}"))
            .into_bytes()
    };
    let cases: Vec<(String, Archive)> = ["./sub/../luts", "luts/./", "luts//"]
        .iter()
        .map(|search_path| {
            let mut entries = standard(8);
            entries[0] = Entry::new(b"config.ocio", &config(search_path), 8);
            (format!("search path {search_path}"), Archive::of(entries))
        })
        .collect();
    let cases: Vec<(&str, Archive)> = cases.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check(&cases);
}

/// Where reading resumes after a seek fails: from the end of the buffered stream's last read
/// of 32,767 bytes. The entry after the one whose zip64 size makes the seek fail holds local
/// headers of `luts/sub/b.spi1d`, each with its own LUT, every few hundred bytes, so the LUT
/// the recovered archive finds first tells where reading resumed.
#[test]
fn recovery_resumes_after_its_last_read() {
    let strip_end = |a: &Archive| {
        let b = a.bytes();
        b[..b.len() - 22].to_vec()
    };
    let mut decoys = Vec::new();
    for k in 0..400 {
        decoys.extend_from_slice(
            &Entry::new(b"luts/sub/b.spi1d", &lut(&format!("{k}.5")), 0).local_record(),
        );
    }
    let mut files = Vec::new();
    for csize in [0x7fff_ffff_ffff_fff0i64, 1 << 50] {
        let mut entries = standard(8);
        entries.insert(2, Entry::new(b"pad.bin", &decoys, 0));
        entries[1].compressed_size = Some(u32::MAX);
        let mut extra = vec![0x01, 0x00, 8, 0];
        extra.extend_from_slice(&csize.to_le_bytes());
        entries[1].local_extra = extra;
        files.push((
            format!("decoys after a zip64 size of {csize}"),
            strip_end(&Archive::of(entries)),
        ));
    }
    let cases: Vec<(&str, Vec<u8>)> = files.iter().map(|(l, a)| (l.as_str(), a.clone())).collect();
    check_files(&cases);
}
