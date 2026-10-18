// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's zlib stream for reading (`mz_strm_zlib.c`), with zlib's `inflate` done by
//! `flate2` on its `zlib-rs` backend, a port of zlib (owner decision 2026-10-17).

use flate2::{Decompress, FlushDecompress, Status};

use super::mz_strm::{MZ_BUF_ERROR, MZ_OK, Stream};

/// zlib's `Z_DATA_ERROR` (minizip-ng returns zlib's errors as its own).
const Z_DATA_ERROR: i32 = -3;

/// The size of the stream's input buffer (`uint8_t buffer[INT16_MAX]`, mz_strm_zlib.c:51 @
/// 4.0.10).
const BUFFER_SIZE: usize = i16::MAX as usize;

/// A raw deflate stream read from a base stream.
///
/// Port of `mz_stream_zlib` (mz_strm_zlib.c:43-63 @ 4.0.10) opened for reading
/// (`mz_stream_zlib_open`, 65-107: `inflateInit2` with `-MAX_WBITS`), over the raw stream of
/// the entry's data (`mz_stream_raw`, mz_strm.c:442-460), both limited to `max_total_in`
/// bytes when it isn't 0.
pub(crate) struct Inflater {
    zstream: Decompress,
    buffer: Vec<u8>,
    /// The unread input: `buffer[next_in..avail_end]`.
    next_in: usize,
    avail_end: usize,
    max_total_in: i64,
    /// What inflate consumed (`zlib->total_in`).
    pub(crate) total_in: i64,
    /// What the raw stream passed up (`raw->total_in`).
    raw_total_in: i64,
    /// zlib's error, which stays.
    error: i32,
}

impl std::fmt::Debug for Inflater {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inflater")
            .field("total_in", &self.total_in)
            .field("error", &self.error)
            .finish()
    }
}

impl Inflater {
    pub(crate) fn new(max_total_in: i64) -> Inflater {
        Inflater {
            zstream: Decompress::new(false),
            buffer: vec![0u8; BUFFER_SIZE],
            next_in: 0,
            avail_end: 0,
            max_total_in,
            total_in: 0,
            raw_total_in: 0,
            error: 0,
        }
    }

    /// Reads up to `buf.len()` bytes: as many as decode before the stream ends (fewer), or an
    /// error, where the bytes this read decoded are lost.
    ///
    /// Port of `mz_stream_zlib_read` (mz_strm_zlib.c:116-193 @ 4.0.10).
    pub(crate) fn read(&mut self, base: &mut dyn Stream, buf: &mut [u8]) -> i32 {
        let mut total_out = 0usize;

        loop {
            if self.next_in == self.avail_end {
                let mut bytes_to_read = BUFFER_SIZE as i64;
                if self.max_total_in > 0 {
                    bytes_to_read = bytes_to_read.min(self.max_total_in - self.total_in);
                    // mz_stream_raw_read's own limit, on what it passed up
                    bytes_to_read = bytes_to_read.min(self.max_total_in - self.raw_total_in);
                }

                let read = base.read(&mut self.buffer[..bytes_to_read.max(0) as usize]);
                if read < 0 {
                    return read;
                }
                self.raw_total_in += i64::from(read);

                self.next_in = 0;
                self.avail_end = read as usize;
            }

            let before_in = self.zstream.total_in();
            let before_out = self.zstream.total_out();

            let result = self.zstream.decompress(
                &self.buffer[self.next_in..self.avail_end],
                &mut buf[total_out..],
                FlushDecompress::Sync,
            );

            let in_bytes = (self.zstream.total_in() - before_in) as usize;
            let out_bytes = (self.zstream.total_out() - before_out) as usize;
            self.next_in += in_bytes;
            total_out += out_bytes;
            self.total_in += in_bytes as i64;

            match result {
                Ok(Status::StreamEnd) => break,
                Ok(Status::Ok) => {}
                Ok(Status::BufError) => {
                    self.error = MZ_BUF_ERROR;
                    break;
                }
                Err(_) => {
                    self.error = Z_DATA_ERROR;
                    break;
                }
            }
            if total_out == buf.len() {
                break;
            }
        }

        if self.error != MZ_OK {
            // Zlib errors are compatible with MZ
            return self.error;
        }

        total_out as i32
    }
}
