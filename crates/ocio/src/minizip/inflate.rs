// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Raw deflate decompression, which minizip-ng asks of zlib (`inflate`, through
//! `mz_stream_zlib_read`, mz_strm_zlib.c:116-193 @ 4.0.10), done by `flate2` on its `zlib-rs`
//! backend, a port of zlib (owner decision 2026-10-17).
//!
//! What a reader of the stream sees depends only on how many bytes decode before the stream
//! ends or fails: zlib writes each decoded byte before it decodes the next symbol, so a read
//! that asks for no more than those bytes gets them, and the read that asks past them gets the
//! end of the stream or its error. [`inflate`] reports that count, which zlib's `total_out`
//! gives even where `inflate` fails.

use flate2::{Decompress, FlushDecompress, Status};

/// How a deflate stream's decoding stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// The stream ended (`Z_STREAM_END`).
    End,
    /// The stream is corrupt, or the input ran out before its end.
    Error,
    /// The output reached the bound the caller asked for.
    Bound,
}

/// Decodes the raw deflate stream `input` (`inflateInit2` with `-MAX_WBITS`, as
/// `mz_stream_zlib_open` asks, mz_strm_zlib.c:65-107, its `window_bits` from
/// `mz_stream_zlib_create`, :362 @ 4.0.10): the bytes that decode before it
/// ends or fails, at most `bound` of them, and why it stopped.
pub(crate) fn inflate(input: &[u8], bound: usize) -> (Vec<u8>, Stop) {
    let mut zstream = Decompress::new(false);
    let mut out = Vec::new();
    loop {
        let total_in = zstream.total_in() as usize;
        let total_out = zstream.total_out() as usize;
        if total_out == bound {
            return (out, Stop::Bound);
        }
        // The output grows a MiB at a time, up to the bound.
        out.resize(total_out + (bound - total_out).min(1 << 20), 0);
        let result = zstream.decompress(
            &input[total_in..],
            &mut out[total_out..],
            FlushDecompress::Sync,
        );
        let written = zstream.total_out() as usize;
        let read = zstream.total_in() as usize;
        out.truncate(written);
        match result {
            Ok(Status::StreamEnd) => return (out, Stop::End),
            Ok(Status::Ok | Status::BufError) if written == total_out && read == total_in => {
                // No progress: the input ran out (zlib's Z_BUF_ERROR).
                return (out, Stop::Error);
            }
            Ok(_) => {}
            Err(_) => return (out, Stop::Error),
        }
    }
}
