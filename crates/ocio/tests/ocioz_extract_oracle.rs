// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ExtractOCIOZArchive` against the wheel, live on this platform (WP 4.9b): the oracle's
//! `ocioz` extracts an archive into a new directory and reports what it writes, and the port
//! extracts the same archive file into its own: the same files with the same contents, times
//! and attributes, the same directories and links, or the same error and what was written
//! before it. The archives are upstream's test archives, the port's archive of a config, and
//! archives this test builds: names with `.`, `..`, backslashes, drive letters, leading slashes
//! and repeats, names that aren't UTF-8; directory and link entries; NTFS, UNIX1 and DOS times;
//! Windows and Unix attributes; and entries minizip-ng can't extract.
//!
//! Times: a file's time comes from its entry's NTFS field where it has one, and otherwise from
//! its DOS date, which the wheel reads in the local time zone and the port in UTC (deviation
//! D-6). The DOS times are compared where the local time zone is UTC (as on the reference
//! platforms' CI), by the C runtime's `mktime` and `_mkgmtime64`/`timegm`.
//!
//! Attributes: on Linux a file's and a directory's mode; on Windows the read-only attribute,
//! the only one the port sets (deviation D-6).
//!
//! Links: on Windows both call `CreateSymbolicLinkW` without the flag that lets an unelevated
//! process make a link (the port through `os_windows_ffi`, owner decision 2026-10-17): both
//! make links where this process holds the privilege (elevated), and both fail where it
//! doesn't.
//!
//! What an extraction writes outside its destination, through a link, is compared too: the
//! destination is `extracted` in a directory of its own, as the oracle extracts.
//! Alternate data streams (Windows: a name `a:b.txt` writes the stream `b.txt` of `a`), which
//! the oracle reports, aren't compared: safe Rust can't list them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ocio::{Config, extract_ocioz_archive};
use ocio_testkit::Oracle;
use ocio_testkit::crt::{mkgmtime_c, mktime_c};
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::paths::upstream_dir;
use serde_json::{Value, json};

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

/// The made-by versions of the hosts the entries say they come from: MS-DOS (0), Unix (3) and
/// Windows NTFS (10), zip 2.0 (PKWARE APPNOTE 4.4.2).
const DOS: u16 = 20;
const UNIX: u16 = 3 << 8 | 20;
const NTFS: u16 = 10 << 8 | 20;

/// A Unix mode as external attributes (the high 16 bits).
fn mode(mode: u32) -> u32 {
    mode << 16
}

/// A Unix symbolic link's mode (`S_IFLNK | 0777`).
const LINK: u32 = 0o120_777;

/// An entry: its name, its data as written, its method, CRC and uncompressed size, the host
/// and attributes, the extra field (in both headers) and the DOS time and date.
#[derive(Clone)]
struct Entry {
    name: Vec<u8>,
    written: Vec<u8>,
    method: u16,
    crc: u32,
    size: u32,
    madeby: u16,
    attributes: u32,
    extra: Vec<u8>,
    time: u16,
    date: u16,
}

impl Entry {
    fn host(mut self, madeby: u16, attributes: u32) -> Entry {
        self.madeby = madeby;
        self.attributes = attributes;
        self
    }

    fn extra(mut self, extra: Vec<u8>) -> Entry {
        self.extra = extra;
        self
    }

    fn dos(mut self, date: u16, time: u16) -> Entry {
        self.date = date;
        self.time = time;
        self
    }
}

fn stored(name: &[u8], data: &[u8]) -> Entry {
    Entry {
        name: name.to_vec(),
        written: data.to_vec(),
        method: 0,
        crc: crc32(data),
        size: data.len() as u32,
        madeby: DOS,
        attributes: 0,
        extra: Vec::new(),
        time: 0x6000,
        date: 0x5a21,
    }
}

fn deflated(name: &[u8], data: &[u8]) -> Entry {
    Entry {
        written: deflate(data),
        method: 8,
        ..stored(name, data)
    }
}

/// An NTFS extra field (PKWARE APPNOTE 4.5.5): modification, access and creation times.
fn ntfs(modified: u64, accessed: u64, created: u64) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [0x000au16, 32, 0, 0, 1, 24] {
        put16(&mut out, v);
    }
    for t in [modified, accessed, created] {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out
}

/// A UNIX1 extra field (Info-ZIP's "UNIX" field, PKWARE APPNOTE 4.5.7): access and
/// modification times, user and group ids, and a link's target.
fn unix1(accessed: u32, modified: u32, link: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put16(&mut out, 0x000d);
    put16(&mut out, 12 + link.len() as u16);
    put32(&mut out, accessed);
    put32(&mut out, modified);
    put32(&mut out, 0);
    out.extend_from_slice(link);
    out
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
        for v in [20u16, 0, e.method, e.time, e.date] {
            put16(&mut out, v);
        }
        for v in [e.crc, e.written.len() as u32, e.size] {
            put32(&mut out, v);
        }
        put16(&mut out, e.name.len() as u16);
        put16(&mut out, e.extra.len() as u16);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.extra);
        out.extend_from_slice(&e.written);

        put32(&mut central, 0x0201_4b50);
        for v in [e.madeby, 20, 0, e.method, e.time, e.date] {
            put16(&mut central, v);
        }
        for v in [e.crc, e.written.len() as u32, e.size] {
            put32(&mut central, v);
        }
        for v in [e.name.len() as u16, e.extra.len() as u16, 0, 0, 0] {
            put16(&mut central, v);
        }
        put32(&mut central, e.attributes);
        put32(&mut central, offset);
        central.extend_from_slice(&e.name);
        central.extend_from_slice(&e.extra);
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

/// Removes `dir` and what it holds, read-only files and unreadable directories included,
/// without following links.
fn remove_tree(dir: &Path) {
    fn writable(path: &Path) {
        let Ok(m) = std::fs::symlink_metadata(path) else {
            return;
        };
        if m.file_type().is_symlink() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
        }
        #[cfg(windows)]
        {
            let mut p = m.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            p.set_readonly(false);
            let _ = std::fs::set_permissions(path, p);
        }
        if m.is_dir()
            && let Ok(entries) = std::fs::read_dir(path)
        {
            for entry in entries.flatten() {
                writable(&entry.path());
            }
        }
    }
    writable(dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// What an extraction left at a path.
#[derive(Debug, PartialEq)]
enum Node {
    /// A file: its contents (`None` where it can't be read), its modification, access and
    /// creation (Windows) times in nanoseconds since 1970 (where they are compared), and its
    /// mode (Linux) or read-only attribute (Windows).
    File {
        contents: Option<Vec<u8>>,
        mtime_ns: Option<i128>,
        atime_ns: Option<i128>,
        birthtime_ns: Option<i128>,
        mode: u64,
    },
    /// A directory and its mode (Linux) or read-only attribute (Windows).
    Dir { mode: u64 },
    /// A symbolic link and its target.
    Link { target: Vec<u8> },
}

/// Paths, as bytes with `/` between the components, and what is there.
type Tree = BTreeMap<Vec<u8>, Node>;

/// The times a case compares: modification, access, creation.
#[derive(Clone, Copy)]
struct Compared {
    mtime: bool,
    atime: bool,
    birthtime: bool,
}

/// Nanoseconds since 1970 of a system time.
fn nanos(t: std::time::SystemTime) -> i128 {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

/// The bytes of a name: UTF-8 on Windows, as the oracle's `os.fsencode` gives them, the bytes
/// on Linux.
fn name_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        name.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        name.to_string_lossy().into_owned().into_bytes()
    }
}

/// The mode (Linux) or the read-only attribute (Windows) of `m`.
fn mode_of(m: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        u64::from(m.permissions().mode() & 0o7777)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        u64::from(m.file_attributes() & 0x01)
    }
}

/// What the port wrote under `root`.
fn port_tree(root: &Path, compared: Compared, skip: Option<&str>) -> Tree {
    let mut out = Tree::new();
    let mut stack = vec![(root.to_path_buf(), Vec::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let entry = entry.unwrap();
            if prefix.is_empty() && skip.is_some_and(|s| entry.file_name() == s) {
                continue;
            }
            let mut relative: Vec<u8> = prefix.clone();
            if !relative.is_empty() {
                relative.push(b'/');
            }
            relative.extend_from_slice(&name_bytes(&entry.file_name()));
            let path = entry.path();
            let m = std::fs::symlink_metadata(&path).unwrap();
            let node = if m.file_type().is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                Node::Link {
                    target: name_bytes(target.as_os_str()),
                }
            } else if m.is_dir() {
                stack.push((path, relative.clone()));
                Node::Dir { mode: mode_of(&m) }
            } else {
                // The times before the read, which can change the access time.
                let mtime_ns = nanos(m.modified().unwrap());
                let atime_ns = nanos(m.accessed().unwrap());
                let birthtime_ns = if cfg!(windows) {
                    Some(nanos(m.created().unwrap()))
                } else {
                    None
                };
                Node::File {
                    contents: std::fs::read(&path).ok(),
                    mtime_ns: compared.mtime.then_some(mtime_ns),
                    atime_ns: compared.atime.then_some(atime_ns),
                    birthtime_ns: birthtime_ns.filter(|_| compared.birthtime),
                    mode: mode_of(&m),
                }
            };
            out.insert(relative, node);
        }
    }
    out
}

/// What the wheel wrote, as the oracle reports it.
fn wheel_tree(result: &Value, blobs: &[Vec<u8>], compared: Compared) -> Tree {
    let mut out = Tree::new();
    let mode = |e: &Value| -> u64 {
        if cfg!(windows) {
            e["attributes"].as_u64().unwrap() & 0x01
        } else {
            e["mode"].as_u64().unwrap()
        }
    };
    for f in result["files"].as_array().unwrap() {
        let contents = f["contents"].as_u64().map(|i| blobs[i as usize].clone());
        // A time past 64 bits comes as {"decimal": digits}.
        let time = |key: &str| {
            let v = &f[key];
            v.as_i64()
                .map(i128::from)
                .or_else(|| v["decimal"].as_str().and_then(|d| d.parse().ok()))
        };
        out.insert(
            bytes(&f["path"]),
            Node::File {
                contents,
                mtime_ns: time("mtime_ns").filter(|_| compared.mtime),
                atime_ns: time("atime_ns").filter(|_| compared.atime),
                birthtime_ns: time("birthtime_ns").filter(|_| compared.birthtime),
                mode: mode(f),
            },
        );
    }
    for d in result["dirs"].as_array().unwrap() {
        out.insert(bytes(&d["path"]), Node::Dir { mode: mode(d) });
    }
    for l in result["links"].as_array().unwrap() {
        out.insert(
            bytes(&l["path"]),
            Node::Link {
                target: bytes(&l["target"]),
            },
        );
    }
    out
}

/// Where a case's files get their times.
#[derive(Clone, Copy, PartialEq)]
enum Times {
    /// From NTFS fields, in every entry: compared.
    Ntfs,
    /// From DOS dates (in some entries at least): compared where the local time zone is UTC.
    Dos,
    /// Not set (the system refuses them, and the files keep the time they were written at):
    /// not compared.
    NotSet,
}

/// Whether the local time zone is UTC, in winter and in summer, as the C runtime's `mktime`
/// says.
fn local_time_is_utc() -> bool {
    [(80, 0, 1, 0, 0, 0), (125, 6, 1, 12, 0, 0)]
        .iter()
        .all(|&(y, mo, d, h, mi, s)| mktime_c(y, mo, d, h, mi, s) == mkgmtime_c(y, mo, d, h, mi, s))
}

/// A case: its label, the archive, and where its files get their times.
struct Case {
    label: String,
    path: PathBuf,
    times: Times,
}

fn case(label: impl Into<String>, entries: &[Entry], times: Times) -> Case {
    Case {
        label: label.into(),
        path: write_file(&archive(entries)),
        times,
    }
}

/// Each archive extracted by the wheel and by the port.
fn check(cases: &[Case]) {
    let utc = local_time_is_utc();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|c| BatchCall {
            cmd: "ocioz",
            args: json!({"extract": {"archive": c.path.to_str().unwrap()}}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, false);
    let mut failures = Vec::new();
    for (i, (c, w)) in cases.iter().zip(wheel).enumerate() {
        let w = w.expect("an oracle request");
        // On Windows minizip-ng sets the access and creation times only from NTFS times (a time
        // of 0 is left alone); on Linux it sets the access time with the modification time.
        let mtime = match c.times {
            Times::Ntfs => true,
            Times::Dos => utc,
            Times::NotSet => false,
        };
        let compared = Compared {
            mtime,
            atime: if cfg!(windows) {
                c.times == Times::Ntfs
            } else {
                mtime
            },
            birthtime: cfg!(windows) && c.times == Times::Ntfs,
        };
        // As the oracle: the destination `extracted` in a directory of its own, one per test
        // (they run in parallel) and case.
        let test = std::thread::current()
            .name()
            .unwrap_or("test")
            .replace("::", "_");
        let root = files_dir("extracted").join(format!("{test}_{i}"));
        remove_tree(&root);
        std::fs::create_dir_all(&root).unwrap();
        let destination = root.join("extracted");
        let port = extract_ocioz_archive(
            c.path.to_str().unwrap().as_bytes(),
            destination.to_str().unwrap().as_bytes(),
        );
        let wheel_written = wheel_tree(&w.result, &w.blobs, compared);
        let port_written = port_tree(&destination, compared, None);
        let wheel_outside = wheel_tree(&w.result["outside"], &w.blobs, compared);
        let port_outside = port_tree(&root, compared, Some("extracted"));
        remove_tree(&root);
        let label = &c.label;
        match (w.result.get("exception"), &port) {
            (None, Ok(())) => {}
            (Some(exception), Err(e)) => {
                if exception["message"].as_str().map(str::as_bytes) != Some(e.what()) {
                    failures.push(format!("{label}: wheel {exception}, port {e}"));
                }
            }
            (wheel, port) => failures.push(format!("{label}: wheel {wheel:?}, port {port:?}")),
        }
        if wheel_written != port_written {
            failures.push(format!(
                "{label}:\n  wheel {wheel_written:?}\n  port  {port_written:?}"
            ));
        }
        if wheel_outside != port_outside {
            failures.push(format!(
                "{label}, outside the destination:\n  wheel {wheel_outside:?}\n  port  {port_outside:?}"
            ));
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
    let upstream = |label: &str, dir: &str, name: &str| Case {
        label: label.to_owned(),
        path: upstream_archive(dir, name),
        times: Times::Dos,
    };
    check(&[
        upstream(
            "context_test1_windows",
            "context_test1",
            "context_test1_windows.ocioz",
        ),
        upstream(
            "context_test1_linux",
            "context_test1",
            "context_test1_linux.ocioz",
        ),
        upstream(
            "config_missing_luts",
            "ocioz_archive_configs",
            "config_missing_luts.ocioz",
        ),
        upstream(
            "missing_config",
            "ocioz_archive_configs",
            "missing_config.ocioz",
        ),
        upstream("empty", "ocioz_archive_configs", "empty.ocioz"),
        Case {
            label: "the port's context_test1".to_owned(),
            path: write_file(&port_archive),
            times: Times::Dos,
        },
    ]);
}

/// Each name in its own archive, first, before an entry of a plain name: names minizip-ng
/// resolves (`.`, `..`, repeated and leading separators), backslashes (separators on Windows
/// only), names Windows refuses, a drive letter, and names that aren't UTF-8 or hold a NUL.
/// The names that are only `..`s read a byte nothing wrote (docs/improvements.md, U-78).
#[test]
fn names() {
    let long_name = format!("{}.txt", "e\u{20ac}".repeat(90));
    let long_path = format!("long/{}.txt", "n".repeat(300));
    let names: Vec<&[u8]> = vec![
        b"a/../b.txt",
        b"./c.txt",
        b"dir\\back.txt",
        b"/abs.txt",
        b"../../escape.txt",
        b"x/./y/../z.txt",
        b"x//double//slash.txt",
        b"trailing/.",
        b"..",
        b"../",
        b"..\\",
        b"../..",
        b"/..",
        b"",
        b"C:/x.txt",
        b"C:x.txt",
        b"a:b.txt",
        b"\\abs.txt",
        b"\\\\srv\\share\\x.txt",
        b"..\\x.txt",
        b"..\\..\\y.txt",
        b"...",
        b".../x.txt",
        b"a/./../../b.txt",
        b"x ",
        b"y.",
        b"dir./f.txt",
        b" lead.txt",
        b"a\\..\\..\\c.txt",
        b"./",
        b".",
        b"a/..",
        b"a/../..",
        b"//x.txt",
        b"/",
        b"\\",
        b"q?.txt",
        b"s*.txt",
        b"p|.txt",
        b"lt<.txt",
        b"dq\".txt",
        b"nul.txt",
        b"x/nul",
        b"\x01ctl.txt",
        "e\u{e9}.txt".as_bytes(),
        long_name.as_bytes(),
        long_path.as_bytes(),
        b"caf\xe9.txt",
        b"c\xc3\x28.txt",
        b"t\xf0\x9f\x98.txt",
        b"s\xed\xa0\x80.txt",
        b"o\xc0\xaf.txt",
        b"z\xff\xfe.txt",
        b"a\x00b.txt",
        b"shot1\\lut1.clf",
    ];
    let cases: Vec<Case> = names
        .iter()
        .map(|name| {
            case(
                format!("the name {:?}", String::from_utf8_lossy(name)),
                &[stored(name, b"text"), deflated(b"after.txt", b"after")],
                Times::Dos,
            )
        })
        .collect();
    check(&cases);
}

/// Directory entries, by their names and by their attributes; repeated names, and names that
/// are a file's and a directory's; names that differ in case; `..` after a name whose first
/// byte is a separator.
#[test]
fn structures() {
    check(&[
        case(
            "directories",
            &[
                stored(b"dir/", b""),
                stored(b"dir/sub/", b""),
                stored(b"dir/sub/f.txt", b"f"),
            ],
            Times::Dos,
        ),
        case(
            "a directory by its Windows attributes",
            &[
                stored(b"d", b"").host(NTFS, 0x10),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
        case(
            "a directory by its Unix mode",
            &[
                stored(b"d2", b"").host(UNIX, mode(0o40755)),
                stored(b"d2/f.txt", b"f"),
            ],
            Times::Dos,
        ),
        case(
            "a directory of mode 0700",
            &[
                stored(b"d3/", b"").host(UNIX, mode(0o40700)),
                stored(b"d3/f.txt", b"f").host(UNIX, mode(0o100600)),
            ],
            Times::Dos,
        ),
        case(
            "repeated names",
            &[
                stored(b"dup.txt", b"first"),
                deflated(b"dup.txt", b"second"),
            ],
            Times::Dos,
        ),
        case(
            "a file, then a directory of its name",
            &[
                stored(b"a", b"A"),
                stored(b"a/b", b"B"),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
        case(
            "a directory, then a file of its name",
            &[
                stored(b"a/b", b"B"),
                stored(b"a", b"A"),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
        case(
            "names that differ in case",
            &[stored(b"Case.txt", b"upper"), stored(b"case.txt", b"lower")],
            Times::Dos,
        ),
        case(
            ".. after a leading slash",
            &[
                stored(b"/abs.txt", b"A"),
                stored(b"..", b"D"),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
        case(
            ".. after a leading backslash",
            &[
                stored(b"\\b.txt", b"A"),
                stored(b"..", b"D"),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
        case(
            ".. after a plain name",
            &[
                stored(b"p.txt", b"A"),
                stored(b"..", b"D"),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
    ]);
}

/// The times files get: an entry's NTFS times (0, past what a `FILETIME` holds, all ones,
/// ordinary), a UNIX1 field's (which minizip-ng takes only where the DOS date gave 0, which
/// none does), and DOS dates with fields past their ranges, which `mktime` normalizes.
#[test]
fn times() {
    let ntfs_case = |label: &str, t: u64, times: Times| {
        case(
            label,
            &[
                stored(b"t.txt", b"t").host(NTFS, 0x20).extra(ntfs(t, t, t)),
                stored(b"after.txt", b"a").extra(ntfs(t, t, t)),
            ],
            times,
        )
    };
    // The NTFS times that convert to a time_t that SetFileTime refuses on Windows, as the
    // FILETIME minizip-ng converts it back to.
    let refused = if cfg!(windows) {
        Times::NotSet
    } else {
        Times::Ntfs
    };
    check(&[
        ntfs_case("NTFS times of 0", 0, refused),
        ntfs_case("huge NTFS times", (1 << 63) + 5, refused),
        ntfs_case("NTFS times of all ones", u64::MAX, refused),
        ntfs_case("ordinary NTFS times", 133_000_000_000_000_000, Times::Ntfs),
        case(
            "UNIX1 times",
            &[
                stored(b"u.txt", b"u")
                    .host(UNIX, mode(0o100644))
                    .extra(unix1(1_000_000_000, 1_000_000_000, b"")),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
        case(
            "a DOS date of zeros",
            &[stored(b"z.txt", b"z").dos(0, 0)],
            Times::Dos,
        ),
        case(
            "a DOS date of ones",
            &[stored(b"o.txt", b"o").dos(0xffff, 0xffff)],
            Times::Dos,
        ),
        case(
            "a DOS date in February of a leap year",
            &[stored(b"f.txt", b"f").dos((20 << 9) | (2 << 5) | 30, 0xbf7d)],
            Times::Dos,
        ),
    ]);
}

/// The attributes files get: Windows ones (read-only, hidden, system) and Unix modes, set
/// where the entry's host gives them.
#[test]
fn attributes() {
    check(&[
        case(
            "Windows attributes",
            &[
                stored(b"ro.txt", b"r").host(NTFS, 0x21),
                stored(b"hid.txt", b"h").host(NTFS, 0x22),
                stored(b"sys.txt", b"s").host(NTFS, 0x24),
                stored(b"dos_ro.txt", b"d").host(DOS, 0x01),
            ],
            Times::Dos,
        ),
        case(
            "Unix modes",
            &[
                stored(b"m000.txt", b"0").host(UNIX, mode(0o100000)),
                stored(b"m4755.txt", b"s").host(UNIX, mode(0o104755)),
                stored(b"m222.txt", b"w").host(UNIX, mode(0o100222)),
                stored(b"m444.txt", b"r").host(UNIX, mode(0o100444)),
                stored(b"after.txt", b"a"),
            ],
            Times::Dos,
        ),
    ]);
}

/// Link entries: links from an entry's contents and from its UNIX1 field, to a file, a
/// directory and outside the destination; a link repeated (the second fails); a link, then a
/// file of its name, which writes through the link outside the destination
/// (docs/improvements.md, I-215); a link named with a trailing slash; a reparse point of a
/// Windows host. On Windows both make links only where this process may (see the module's
/// documentation), and fail where it may not. (Absolute targets, and targets that leave the
/// oracle's directory, the oracle refuses.)
#[test]
fn links() {
    let link = |name: &[u8], target: &[u8]| stored(name, target).host(UNIX, mode(LINK));
    let after = stored(b"after.txt", b"a");
    check(&[
        case(
            "a link from the entry's contents",
            &[
                stored(b"target.txt", b"T"),
                link(b"l.txt", b"target.txt"),
                after.clone(),
            ],
            Times::Dos,
        ),
        case(
            "a link from the UNIX1 field",
            &[
                stored(b"target.txt", b"T"),
                stored(b"l.txt", b"")
                    .host(UNIX, mode(LINK))
                    .extra(unix1(0, 0, b"target.txt")),
                after.clone(),
            ],
            Times::Dos,
        ),
        case(
            "a link to a directory",
            &[stored(b"d/", b""), link(b"l", b"d/"), after.clone()],
            Times::Dos,
        ),
        case(
            "a link outside",
            &[link(b"out", b"../outside.txt"), after.clone()],
            Times::Dos,
        ),
        case(
            "a link twice",
            &[
                link(b"l.txt", b"target.txt"),
                link(b"l.txt", b"target.txt"),
                after.clone(),
            ],
            Times::Dos,
        ),
        case(
            "a link, then a file of its name",
            &[
                link(b"l.txt", b"../outside.txt"),
                stored(b"l.txt", b"written through the link"),
                after.clone(),
            ],
            Times::Dos,
        ),
        case(
            "a link named with a trailing slash",
            &[link(b"ldir/", b"target"), after.clone()],
            Times::Dos,
        ),
        case(
            "a reparse point of a Windows host",
            &[stored(b"r.txt", b"target.txt").host(NTFS, 0x400), after],
            Times::Dos,
        ),
    ]);
}

/// Archives this test builds that minizip-ng can't extract, or extracts its own way: corrupt
/// data, a wrong CRC, an unknown method, a stored entry with no data, a deflated one with
/// bytes after its end; no entries; a file that isn't an archive, and one that doesn't exist.
/// An entry that fails is left with the time it was written at (minizip-ng sets the dates only
/// where the entry was saved whole), so these cases don't compare times.
#[test]
fn failures() {
    let mut corrupt = deflated(b"corrupt.txt", b"some text, long enough to be compressed");
    corrupt.written[1] ^= 0xff;
    let mut wrong_crc = stored(b"wrong_crc.txt", b"text");
    wrong_crc.crc ^= 1;
    let mut unknown = stored(b"unknown.txt", b"text");
    unknown.method = 12;
    let mut empty_data = stored(b"zc.txt", b"abc");
    empty_data.written.clear();
    let mut trailing = deflated(b"dt.txt", &[b'x'; 100]);
    trailing.written.extend_from_slice(b"junk");
    let after = stored(b"after.txt", b"a");
    let mut cases = vec![
        case(
            "a corrupt entry",
            &[stored(b"ok.txt", b"ok"), corrupt],
            Times::NotSet,
        ),
        case(
            "a wrong CRC",
            &[stored(b"ok.txt", b"ok"), wrong_crc, after.clone()],
            Times::NotSet,
        ),
        case(
            "an unknown method",
            &[unknown, after.clone()],
            Times::NotSet,
        ),
        case(
            "a stored entry without its data",
            &[empty_data, after.clone()],
            Times::NotSet,
        ),
        case(
            "a deflated entry with bytes after its end",
            &[trailing, after],
            Times::NotSet,
        ),
        case("no entries", &[], Times::NotSet),
    ];
    cases.push(Case {
        label: "not an archive".to_owned(),
        path: write_file(b"PK, and no archive"),
        times: Times::NotSet,
    });
    cases.push(Case {
        label: "missing".to_owned(),
        path: files_dir("archives").join("does_not_exist.ocioz"),
        times: Times::NotSet,
    });
    check(&cases);
}
