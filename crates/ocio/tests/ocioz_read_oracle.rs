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
        }
    }

    /// The entry's local header and data.
    fn local_record(&self) -> Vec<u8> {
        let written = self.written();
        let mut out = Vec::new();
        put32(&mut out, 0x0403_4b50);
        for v in [20u16, self.flag, self.method, 0, 0x0021] {
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
        put16(&mut out, self.name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(&self.name);
        out.extend_from_slice(&written);
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
            0,
            self.disk,
            0,
        ] {
            put16(&mut out, v);
        }
        put32(&mut out, 0);
        put32(&mut out, offset);
        out.extend_from_slice(&self.name);
        out.extend_from_slice(&self.cd_extra);
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
