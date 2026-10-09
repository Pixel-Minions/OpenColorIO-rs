// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The part of `src/OpenColorIO/ParseUtils.cpp` @ v2.5.2 that reads an input stream:
//! `nextline`, which the line-based readers share. The rest of ParseUtils is
//! `ocio_ops::parse_utils`.

use ocio_ops::utils::string_utils::is_empty_or_white_space;

use crate::fileformats::input_stream::InputStream;

/// Reads lines into `line` (without their `\n`, and without one `\r` before it) until one
/// that isn't empty or white space only: `true` then; at the end of the stream, `line`
/// emptied and `false`.
///
/// Port of `nextline` (src/OpenColorIO/ParseUtils.cpp:674-691 @ v2.5.2).
pub fn nextline(istream: &mut InputStream, line: &mut Vec<u8>) -> bool {
    while istream.good() {
        istream.getline(line);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if !is_empty_or_white_space(line) {
            return true;
        }
    }

    line.clear();
    false
}
