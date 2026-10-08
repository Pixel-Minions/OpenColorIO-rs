// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The stream a file format reads: what upstream's `std::ifstream` gives a reader of a LUT
//! file, in text or binary mode, as each wheel's C++ runtime gives it.
//!
//! A text-mode `std::ifstream` reads through the C runtime's `FILE` on Windows (MSVC's
//! `basic_filebuf` opens it with `_Fiopen`, without `b`), whose low-level reads translate the
//! text: `CR LF` reads as `LF`, a lone `CR` is kept, and `0x1A` ends the file. On Linux text
//! and binary mode read the same bytes (docs/improvements.md, I-162). The file is read whole
//! when it opens; the readers' extractions (`getline`, `>>`) come with the readers (4.0c).

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
