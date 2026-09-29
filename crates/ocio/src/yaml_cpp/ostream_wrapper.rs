// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `ostream_wrapper` (include/yaml-cpp/ostream_wrapper.h,
//! src/ostream_wrapper.cpp) in its buffer mode, and of `Indentation`/`IndentTo`
//! (src/indentation.h).

/// Port of `YAML::ostream_wrapper` constructed without a stream: text goes to a byte buffer,
/// and the row, the column (in bytes) and whether a comment is open are tracked.
#[derive(Debug, Clone, Default)]
pub struct OstreamWrapper {
    buffer: Vec<u8>,
    row: usize,
    col: usize,
    comment: bool,
}

impl OstreamWrapper {
    /// Port of `ostream_wrapper::ostream_wrapper()` (ostream_wrapper.cpp:8-14).
    pub fn new() -> OstreamWrapper {
        OstreamWrapper::default()
    }

    /// Port of `ostream_wrapper::write(const std::string &)` (ostream_wrapper.cpp:26-37).
    pub fn write_str(&mut self, s: &str) {
        self.write_bytes(s.as_bytes());
    }

    /// Port of `ostream_wrapper::write(const char *, std::size_t)` (ostream_wrapper.cpp:
    /// 39-50).
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
        for &ch in bytes {
            self.update_pos(ch);
        }
    }

    /// Port of `operator<<(ostream_wrapper &, char)` (ostream_wrapper.h:66-69).
    pub fn write_byte(&mut self, ch: u8) {
        self.write_bytes(&[ch]);
    }

    /// Port of `ostream_wrapper::set_comment`.
    pub fn set_comment(&mut self) {
        self.comment = true;
    }

    /// The bytes written: `str()` without the NUL terminator.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Port of `ostream_wrapper::str()` read as a C string, the way callers use it: the text
    /// up to its first NUL byte.
    pub fn c_str(&self) -> &[u8] {
        let end = self
            .buffer
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.buffer.len());
        &self.buffer[..end]
    }

    /// Port of `ostream_wrapper::row`.
    pub fn row(&self) -> usize {
        self.row
    }

    /// Port of `ostream_wrapper::col`: bytes since the last newline.
    pub fn col(&self) -> usize {
        self.col
    }

    /// Port of `ostream_wrapper::pos`: bytes written.
    pub fn pos(&self) -> usize {
        self.buffer.len()
    }

    /// Port of `ostream_wrapper::comment`.
    pub fn comment(&self) -> bool {
        self.comment
    }

    /// Port of `ostream_wrapper::update_pos` (ostream_wrapper.cpp:52-61).
    fn update_pos(&mut self, ch: u8) {
        self.col += 1;
        if ch == b'\n' {
            self.row += 1;
            self.col = 0;
            self.comment = false;
        }
    }

    /// Port of `operator<<(ostream_wrapper &, const Indentation &)` (indentation.h:14-19):
    /// `n` spaces.
    pub fn indentation(&mut self, n: usize) {
        for _ in 0..n {
            self.write_byte(b' ');
        }
    }

    /// Port of `operator<<(ostream_wrapper &, const IndentTo &)` (indentation.h:26-31): spaces
    /// until the column is at least `n`.
    pub fn indent_to(&mut self, n: usize) {
        while self.col < n {
            self.write_byte(b' ');
        }
    }
}

#[cfg(test)]
mod tests {
    //! Port of yaml-cpp 0.8.0 `test/ostream_wrapper_test.cpp`, buffer mode (the stream-mode
    //! tests exercise the unported `std::ostream` constructor).
    use super::*;

    /// Port of yaml-cpp 0.8.0 `TEST(OstreamWrapperTest, BufferNoWrite)`.
    #[test]
    fn buffer_no_write() {
        let wrapper = OstreamWrapper::new();
        assert_eq!(wrapper.c_str(), b"");
    }

    /// Port of yaml-cpp 0.8.0 `TEST(OstreamWrapperTest, BufferWriteStr)`.
    #[test]
    fn buffer_write_str() {
        let mut wrapper = OstreamWrapper::new();
        wrapper.write_str("Hello, world");
        assert_eq!(wrapper.c_str(), b"Hello, world");
    }

    /// Port of yaml-cpp 0.8.0 `TEST(OstreamWrapperTest, BufferWriteCStr)`.
    #[test]
    fn buffer_write_c_str() {
        let mut wrapper = OstreamWrapper::new();
        wrapper.write_bytes(b"Hello, world");
        assert_eq!(wrapper.c_str(), b"Hello, world");
    }

    /// Port of yaml-cpp 0.8.0 `TEST(OstreamWrapperTest, Position)`.
    #[test]
    fn position() {
        let mut wrapper = OstreamWrapper::new();
        wrapper.write_bytes(b"Hello, world\n");
        assert_eq!(wrapper.row(), 1);
        assert_eq!(wrapper.col(), 0);
        assert_eq!(wrapper.pos(), 13);
    }

    /// Port of yaml-cpp 0.8.0 `TEST(OstreamWrapperTest, Comment)`.
    #[test]
    fn comment() {
        let mut wrapper = OstreamWrapper::new();
        wrapper.write_bytes(b"Hello, world ");
        wrapper.set_comment();
        assert!(wrapper.comment());
        wrapper.write_bytes(b"foo");
        assert!(wrapper.comment());
        wrapper.write_bytes(b"\n");
        assert!(!wrapper.comment());
    }
}
