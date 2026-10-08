// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The stream a file format reads: what upstream's `std::ifstream` gives a reader of a LUT
//! file, in text or binary mode, as each wheel's C++ runtime gives it.
//!
//! A text-mode `std::ifstream` reads through the C runtime's `FILE` on Windows (MSVC's
//! `basic_filebuf` opens it with `_Fiopen`, without `b`), whose low-level reads translate the
//! text: `CR LF` reads as `LF`, a lone `CR` is kept, and `0x1A` ends the file. On Linux text
//! and binary mode read the same bytes (docs/improvements.md, I-162). The file is read whole
//! when it opens. A `std::istringstream` (the readers' tests, a `ConfigIOProxy`'s data) holds
//! its bytes as given.
//!
//! The extractions follow the C++ standard's unformatted and formatted input ([istream]),
//! which both wheels' libraries implement; each comes with the first reader that uses it.

use std::io::Read;

use ocio_ops::platform::create_input_file_stream;

/// How a stream opens a file: upstream's `std::ios_base::in` or `std::ios_base::binary`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// `std::ios_base::in`: text mode, translated on Windows.
    Text,
    /// `std::ios_base::binary`: the file's bytes.
    Binary,
}

/// A LUT file's contents and the state bits of the `std::istream` that reads them.
#[derive(Clone, Debug, Default)]
pub struct InputStream {
    data: Vec<u8>,
    /// The position of the next byte to extract.
    pos: usize,
    /// The number of bytes the last unformatted extraction extracted (`gcount`).
    gcount: usize,
    eof: bool,
    fail: bool,
    bad: bool,
    /// The file opened but reading it failed (a directory on Linux): libstdc++'s
    /// `basic_filebuf::underflow` then throws, and the extraction that asked sets `badbit`.
    read_error: bool,
}

impl InputStream {
    /// Opens the file `filepath` (up to its first NUL) in `mode`: a stream whose `failbit` is
    /// set when the file can't be opened.
    ///
    /// Port of the `std::ifstream(Platform::filenameToUTF(filepath), mode)` of `getLutData`
    /// (src/OpenColorIO/transforms/FileTransform.cpp:219-220 @ v2.5.2): through the UTF-16
    /// name on Windows, the bytes on Linux ([`create_input_file_stream`]). A directory opens
    /// on Linux (glibc's `fopen` accepts it) and fails to read; on Windows it doesn't open.
    pub fn open_file(filepath: &[u8], mode: OpenMode) -> InputStream {
        let Ok(mut file) = create_input_file_stream(filepath) else {
            return InputStream {
                fail: true,
                ..InputStream::default()
            };
        };
        let mut data = Vec::new();
        let read_error = file.read_to_end(&mut data).is_err();
        if cfg!(windows) && mode == OpenMode::Text {
            data = msvc_text_mode(&data);
        }
        InputStream {
            data,
            read_error,
            ..InputStream::default()
        }
    }

    /// A stream of `data`, as a `std::istringstream` of it holds it.
    pub fn from_bytes(data: impl Into<Vec<u8>>) -> InputStream {
        InputStream {
            data: data.into(),
            ..InputStream::default()
        }
    }

    /// Whether no state bit is set.
    ///
    /// Port of `std::ios::good`.
    pub fn good(&self) -> bool {
        !(self.eof || self.fail || self.bad)
    }

    /// The bytes the stream holds, what its reader sees from the start.
    pub fn contents(&self) -> &[u8] {
        &self.data
    }

    /// Whether reading the file failed after it opened: the next extraction sets `badbit`.
    pub fn read_error(&self) -> bool {
        self.read_error
    }

    /// Port of `std::ios::eof`.
    pub fn eof(&self) -> bool {
        self.eof
    }

    /// Whether `failbit` or `badbit` is set.
    ///
    /// Port of `std::ios::fail`.
    pub fn fail(&self) -> bool {
        self.fail || self.bad
    }

    /// The number of bytes the last unformatted extraction extracted.
    ///
    /// Port of `std::istream::gcount`.
    pub fn gcount(&self) -> usize {
        self.gcount
    }

    /// The next byte, or `None` at the end of the stream; at the end of a file that failed to
    /// read, `badbit` is set (the underflow's exception, which the extraction catches).
    fn peek_byte(&mut self) -> Option<u8> {
        match self.data.get(self.pos) {
            Some(&b) => Some(b),
            None => {
                if self.read_error {
                    self.bad = true;
                }
                None
            }
        }
    }

    /// Extracts a line into `line` (cleared first): bytes up to a line feed, which is
    /// extracted and not stored; the end of the stream sets `eofbit`; a line of `n - 1` bytes
    /// or more stops after `n - 1` and sets `failbit`; nothing extracted sets `failbit`. A
    /// stream that isn't `good()` extracts nothing (its sentry). `line` holds what upstream's
    /// buffer holds before the NUL the call writes after it.
    ///
    /// Port of `std::istream::getline(char *, std::streamsize)` (C++17 [istream.unformatted]
    /// 30.7.4.3/18-21; MSVC's and libstdc++'s implementations test the end of the stream, the
    /// delimiter, then the full buffer, in that order).
    pub fn getline(&mut self, line: &mut Vec<u8>, n: usize) {
        line.clear();
        self.gcount = 0;
        if self.good() && n > 0 {
            let mut count = n;
            loop {
                match self.peek_byte() {
                    None => {
                        if !self.bad {
                            self.eof = true;
                        }
                        break;
                    }
                    Some(b'\n') => {
                        self.gcount += 1;
                        self.pos += 1;
                        break;
                    }
                    Some(b) => {
                        count -= 1;
                        if count == 0 {
                            // buffer full, quit
                            self.fail = true;
                            break;
                        }
                        line.push(b);
                        self.gcount += 1;
                        self.pos += 1;
                    }
                }
            }
        } else if !self.good() {
            self.fail = true;
        }
        if self.gcount == 0 {
            self.fail = true;
        }
    }

    /// Extracts up to `n` bytes into the start of `buf`: all `n`, or those up to the end of the
    /// stream, which sets `eofbit` and `failbit`; a stream that isn't `good()` extracts
    /// nothing and sets `failbit` (its sentry).
    ///
    /// Port of `std::istream::read(char *, std::streamsize)` (C++17 [istream.unformatted]
    /// 30.7.4.3/28-30).
    pub fn read(&mut self, buf: &mut [u8], n: usize) {
        self.gcount = 0;
        if !self.good() {
            self.fail = true;
            return;
        }
        while self.gcount < n {
            match self.peek_byte() {
                Some(b) => {
                    buf[self.gcount] = b;
                    self.pos += 1;
                    self.gcount += 1;
                }
                None => {
                    if !self.bad {
                        self.eof = true;
                        self.fail = true;
                    }
                    return;
                }
            }
        }
    }
}

/// The bytes a text-mode read of `raw` gives in the UCRT (`_read` of a text-mode file):
/// everything from the first `0x1A` on is dropped, and each `CR LF` becomes `LF`; a `CR`
/// before anything else (another `CR`, `0x1A`, the end) is kept.
fn msvc_text_mode(raw: &[u8]) -> Vec<u8> {
    let end = raw.iter().position(|&b| b == 0x1a).unwrap_or(raw.len());
    let raw = &raw[..end];
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\r' && raw.get(i + 1) == Some(&b'\n') {
            out.push(b'\n');
            i += 2;
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    out
}
