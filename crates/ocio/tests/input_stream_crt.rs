// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! A LUT file's stream against the platform's C runtime (`ocio_testkit::crt::read_text_mode`,
//! the CRT's `fopen(path, "r")` and `fread`): in text mode, the bytes a reader sees are the
//! runtime's, on Windows translated (`CR LF`, a lone `CR`, `0x1A`), on Linux the file's own;
//! in binary mode, the file's own; and a file opens where the runtime opens it.

use std::path::{Path, PathBuf};

use ocio::fileformats::input_stream::{InputStream, OpenMode};
use ocio_testkit::crt::read_text_mode;

/// A scratch directory of this test process.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ocio-rs-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path_bytes(path: &Path) -> &[u8] {
    path.to_str().expect("a UTF-8 path").as_bytes()
}

/// Writes `contents` to `path` and checks both modes of a stream of it against the runtime.
fn check(path: &Path, contents: &[u8]) {
    std::fs::write(path, contents).unwrap();
    let text = InputStream::open_file(path_bytes(path), OpenMode::Text);
    assert!(text.good());
    assert!(!text.read_error());
    assert_eq!(
        text.contents(),
        read_text_mode(path).unwrap().as_slice(),
        "text mode of {}",
        contents.escape_ascii()
    );
    let binary = InputStream::open_file(path_bytes(path), OpenMode::Binary);
    assert!(binary.good());
    assert_eq!(binary.contents(), contents, "binary mode");
}

/// Every file of up to 4 bytes of `a`, `CR`, `LF`, `0x1A` and NUL.
#[test]
fn short_files_read_as_the_runtime_reads_them() {
    let dir = scratch("stream-short");
    let path = dir.join("lut.txt");
    let alphabet = [b'a', b'\r', b'\n', 0x1a, 0];
    let mut cases: Vec<Vec<u8>> = vec![Vec::new()];
    let mut last: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..4 {
        last = last
            .iter()
            .flat_map(|s| {
                alphabet.iter().map(move |&b| {
                    let mut t = s.clone();
                    t.push(b);
                    t
                })
            })
            .collect();
        cases.extend(last.iter().cloned());
    }
    assert_eq!(cases.len(), 781);
    for case in &cases {
        check(&path, case);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Line ends and `0x1A` at the runtime's buffer boundaries (4096 bytes and multiples).
#[test]
fn line_ends_across_buffer_boundaries_read_as_the_runtime_reads_them() {
    let dir = scratch("stream-boundary");
    let path = dir.join("lut.txt");
    for boundary in [4096usize, 8192, 65536] {
        for before in boundary - 3..=boundary + 1 {
            for tail in [
                &b"\r\n"[..],
                b"\r",
                b"\r\r\n",
                b"\r\n\r\n",
                b"\x1a",
                b"\r\x1a\n",
                b"\n\r",
            ] {
                let mut contents = vec![b'a'; before];
                contents.extend_from_slice(tail);
                contents.extend_from_slice(b"b\r\nc");
                check(&path, &contents);
            }
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A missing file doesn't open; a directory opens where the runtime opens it (Linux), and then
/// fails to read.
#[test]
fn files_open_where_the_runtime_opens_them() {
    let dir = scratch("stream-open");
    let missing = dir.join("missing.cube");
    assert!(read_text_mode(&missing).is_err());
    for mode in [OpenMode::Text, OpenMode::Binary] {
        assert!(!InputStream::open_file(path_bytes(&missing), mode).good());
        let stream = InputStream::open_file(path_bytes(&dir), mode);
        assert_eq!(stream.good(), read_text_mode(&dir).is_ok());
        assert_eq!(stream.read_error(), stream.good());
        // The name ends at its first NUL.
        let mut name = path_bytes(&missing).to_vec();
        name.push(0);
        name.extend_from_slice(b"ignored");
        assert!(!InputStream::open_file(&name, mode).good());
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
