// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's input stream (src/stream.h, src/stream.cpp) and the regex source
//! over it (src/streamcharsource.h).
//!
//! The stream guesses the encoding from the first bytes (a byte order mark, or the zeros of
//! UTF-16 and UTF-32 text), and turns UTF-16 and UTF-32 into UTF-8. UTF-8 bytes pass through
//! unchecked. The scanner reads it one character (byte) at a time, with lookahead, and the
//! end of the input is the character `0x04` (`Stream::eof()`).
//!
//! yaml-cpp reads a `std::istream`. OCIO gives it a `std::istringstream` (a config from a
//! string) or a `std::ifstream` (a config file); the port reads the bytes such a stream
//! holds. [`IStream`] keeps the parts of `std::istream` the stream uses: `get`, `putback`,
//! `clear`, `good` and the end-of-file state.

use std::cell::RefCell;
use std::collections::VecDeque;

use super::mark::Mark;
use super::regex_yaml::Source;

/// `Stream::eof()` (stream.h:40): the character that stands for the end of the input.
pub const STREAM_EOF: u8 = 0x04;

/// `CP_REPLACEMENT_CHARACTER` (stream.cpp:12).
const CP_REPLACEMENT_CHARACTER: u32 = 0xFFFD;

/// The part of `std::istream` yaml-cpp's `Stream` uses, over the bytes the stream holds: the
/// stream buffer ([`StreamBuf`]) and the `eofbit` and `failbit` states (`badbit`, which only a
/// failed `putback` sets, counts as `failbit`: no caller tells them apart).
#[derive(Debug)]
pub struct IStream<'a> {
    source: StreamBuf<'a>,
    eof: bool,
    fail: bool,
}

/// The stream buffer of an [`IStream`].
#[derive(Debug)]
enum StreamBuf<'a> {
    /// A `std::stringbuf` (OCIO's `std::istringstream`), or libstdc++'s `std::filebuf` (an
    /// `std::ifstream` on Linux), which puts back what was read wherever it is: a position in
    /// the bytes.
    Memory { data: &'a [u8], pos: usize },
    /// MSVC's `std::basic_filebuf<char>` (an `std::ifstream` on Windows).
    MsvcFile(MsvcFileBuf<'a>),
}

impl<'a> From<&'a [u8]> for IStream<'a> {
    fn from(data: &'a [u8]) -> IStream<'a> {
        IStream::new(data)
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for IStream<'a> {
    fn from(data: &'a [u8; N]) -> IStream<'a> {
        IStream::new(data)
    }
}

impl<'a> From<&'a Vec<u8>> for IStream<'a> {
    fn from(data: &'a Vec<u8>) -> IStream<'a> {
        IStream::new(data)
    }
}

impl<'a> IStream<'a> {
    /// A string stream over `data` (`std::istringstream`), in the good state.
    pub fn new(data: &'a [u8]) -> IStream<'a> {
        IStream {
            source: StreamBuf::Memory { data, pos: 0 },
            eof: false,
            fail: false,
        }
    }

    /// The file stream OCIO's `Config::CreateFromFile` hands yaml-cpp, over the file's bytes
    /// `data`, in the good state: the stream after `read(magicNumber, 2)`, `clear()` and
    /// `seekg(0)` (Config.cpp:1176-1206 @ v2.5.2). On Windows it is MSVC's `std::filebuf`
    /// over the C runtime's `FILE`, whose putback differs from a string stream's at the end of
    /// the file ([`MsvcFileBuf`]); libstdc++'s seeks back in the file, as a string stream.
    pub fn file(data: &'a [u8]) -> IStream<'a> {
        let source = if cfg!(windows) {
            StreamBuf::MsvcFile(MsvcFileBuf::new(data))
        } else {
            StreamBuf::Memory { data, pos: 0 }
        };
        IStream {
            source,
            eof: false,
            fail: false,
        }
    }

    /// `good()`: no state bit is set.
    fn good(&self) -> bool {
        !self.eof && !self.fail
    }

    /// `get()`: the next byte, or `None` (`traits_type::eof()`) at the end, which sets
    /// `eofbit` and `failbit`. A stream that isn't good gives `None` (the sentry fails).
    fn get(&mut self) -> Option<u8> {
        if !self.good() {
            self.fail = true;
            return None;
        }
        let next = match &mut self.source {
            StreamBuf::Memory { data, pos } => data.get(*pos).map(|&b| {
                *pos += 1;
                b
            }),
            StreamBuf::MsvcFile(buf) => buf.sbumpc(),
        };
        if next.is_none() {
            self.eof = true;
            self.fail = true;
        }
        next
    }

    /// `putback(c)` of `c`, the byte just read. A stream buffer that can't put it back sets
    /// `badbit`.
    fn putback(&mut self, c: u8) {
        // C++11 putback clears eofbit first; the sentry then needs a good stream.
        self.eof = false;
        if !self.good() {
            self.fail = true;
            return;
        }
        let put_back = match &mut self.source {
            StreamBuf::Memory { pos, .. } => {
                // The bytes are in memory (or the file is seeked back): stepping back over the
                // byte just read always succeeds.
                if *pos > 0 {
                    *pos -= 1;
                }
                true
            }
            StreamBuf::MsvcFile(buf) => buf.sputbackc(c),
        };
        if !put_back {
            self.fail = true;
        }
    }

    /// `clear()`.
    fn clear(&mut self) {
        self.eof = false;
        self.fail = false;
    }

    /// `GetNextByte` (stream.cpp:388-404): the next byte through the stream buffer, bypassing
    /// the stream's state. At the end it sets `eofbit` and gives 0. (yaml-cpp reads ahead
    /// `YAML_PREFETCH_SIZE` bytes at a time with `sgetn`; reading them one by one gives the
    /// same bytes and sets `eofbit` at the same read.)
    fn next_byte(&mut self) -> u8 {
        let next = match &mut self.source {
            StreamBuf::Memory { data, pos } => data.get(*pos).map(|&b| {
                *pos += 1;
                b
            }),
            StreamBuf::MsvcFile(buf) => buf.xsgetn_byte(),
        };
        match next {
            Some(b) => b,
            None => {
                self.eof = true;
                0
            }
        }
    }
}

/// MSVC's `std::basic_filebuf<char>` (`<__msvc_filebuf.hpp>`) over a `FILE` of the Universal C
/// Runtime opened for reading, as `Config::CreateFromFile` leaves it (after `seekg(0)`): the
/// `FILE`'s buffer, read position and count, which the filebuf's get area shares
/// (`_get_stream_buffer_pointers`), and the filebuf's one-byte putback area `_Mychar`.
///
/// It puts back a byte by stepping back in the get area when the byte before is the one put
/// back; otherwise with the C runtime's `ungetc`, which fails when the `FILE` already holds a
/// pushed-back byte at the start of its buffer; otherwise in `_Mychar`, unless the get area is
/// already `_Mychar`. At the end of the file the `FILE`'s read position is the start of its
/// buffer, so after a read past the end two bytes can be put back, and a third fails. A string
/// stream (and libstdc++'s filebuf) puts back any number: yaml-cpp's detection of the
/// encoding puts back three bytes after the end of a file of fewer than four, such as
/// `00 00 FE`, which then reads as an empty document on Windows only.
#[derive(Debug)]
struct MsvcFileBuf<'a> {
    /// The file's bytes.
    data: &'a [u8],
    /// The position of the next byte `_read` gives.
    file_pos: usize,
    /// The `FILE`'s buffer (`_base`), as large as `_INTERNAL_BUFSIZ`.
    buf: Vec<u8>,
    /// `_bufsiz`: the size of the next refill. After a seek it is `_SMALL_BUFSIZ` (512) for
    /// one refill, then `_INTERNAL_BUFSIZ` (4096).
    bufsiz: usize,
    /// `_ptr`, as an index into `buf`.
    ptr: usize,
    /// `_cnt`.
    cnt: usize,
    /// `_Mychar`.
    mychar: u8,
    /// Whether the get area is `_Mychar`'s (`eback() == &_Mychar`): with whether its byte was
    /// read (`gptr()` past it), and the end of the `FILE`'s get area saved (`_Set_egptr`).
    back: Option<MsvcBack>,
}

/// The get area of [`MsvcFileBuf`] while it is `_Mychar`'s.
#[derive(Debug, Clone, Copy)]
struct MsvcBack {
    consumed: bool,
    saved_egptr: usize,
}

impl<'a> MsvcFileBuf<'a> {
    /// `_INTERNAL_BUFSIZ`.
    const INTERNAL_BUFSIZ: usize = 4096;
    /// `_SMALL_BUFSIZ`.
    const SMALL_BUFSIZ: usize = 512;

    fn new(data: &'a [u8]) -> MsvcFileBuf<'a> {
        MsvcFileBuf {
            data,
            file_pos: 0,
            buf: vec![0; Self::INTERNAL_BUFSIZ],
            bufsiz: Self::SMALL_BUFSIZ,
            ptr: 0,
            cnt: 0,
            mychar: 0,
            back: None,
        }
    }

    /// `_filbuf` (`common_refill_and_read_nolock`): reads the next `_bufsiz` bytes into the
    /// buffer from its start; the first of them, or `None` at the end, which leaves the read
    /// position at the start of the buffer and nothing in it.
    fn refill(&mut self) -> Option<u8> {
        self.ptr = 0;
        let n = self.bufsiz.min(self.data.len() - self.file_pos);
        self.buf[..n].copy_from_slice(&self.data[self.file_pos..self.file_pos + n]);
        self.file_pos += n;
        self.bufsiz = Self::INTERNAL_BUFSIZ;
        if n == 0 {
            self.cnt = 0;
            return None;
        }
        self.cnt = n - 1;
        self.ptr = 1;
        Some(self.buf[0])
    }

    /// `fgetc` (and `fread`, a byte at a time).
    fn fgetc(&mut self) -> Option<u8> {
        if self.cnt > 0 {
            self.cnt -= 1;
            self.ptr += 1;
            Some(self.buf[self.ptr - 1])
        } else {
            self.refill()
        }
    }

    /// `_Reset_back`: the get area is the `FILE`'s again, from the start of its buffer
    /// (`setg(_Set_eback, _Set_eback, _Set_egptr)`).
    fn reset_back(&mut self) {
        if let Some(back) = self.back.take() {
            self.ptr = 0;
            self.cnt = back.saved_egptr;
        }
    }

    /// `sbumpc()`, through `uflow` when the get area is empty.
    fn sbumpc(&mut self) -> Option<u8> {
        if let Some(back) = &mut self.back
            && !back.consumed
        {
            back.consumed = true;
            return Some(self.mychar);
        }
        self.reset_back();
        self.fgetc()
    }

    /// One byte of `xsgetn`: the get area's, then (after `_Reset_back`) the `FILE`'s, through
    /// `fread`.
    fn xsgetn_byte(&mut self) -> Option<u8> {
        self.sbumpc()
    }

    /// `sputbackc(c)`: steps back when the byte before the get position is `c`, otherwise
    /// `pbackfail(c)`.
    fn sputbackc(&mut self, c: u8) -> bool {
        match &mut self.back {
            Some(back) => {
                if back.consumed && self.mychar == c {
                    back.consumed = false;
                    return true;
                }
            }
            None => {
                if self.ptr > 0 && self.buf[self.ptr - 1] == c {
                    self.ptr -= 1;
                    self.cnt += 1;
                    return true;
                }
            }
        }
        self.pbackfail(c)
    }

    /// `pbackfail(c)` past its first branch (the step back `sputbackc` tried): `ungetc`, else
    /// `_Mychar` unless the get position is already there, else failure.
    fn pbackfail(&mut self, c: u8) -> bool {
        if self.ungetc(c) {
            return true;
        }
        let at_mychar = matches!(
            self.back,
            Some(MsvcBack {
                consumed: false,
                ..
            })
        );
        if at_mychar {
            return false;
        }
        self.mychar = c;
        // _Set_back: the FILE's get area is saved unless the area is already _Mychar's.
        let saved_egptr = match self.back {
            Some(back) => back.saved_egptr,
            None => self.ptr + self.cnt,
        };
        self.back = Some(MsvcBack {
            consumed: false,
            saved_egptr,
        });
        true
    }

    /// The Universal C Runtime's `_ungetc_nolock` on a file opened for reading, with its
    /// buffer: at the start of the buffer it fails if the buffer holds a byte, and otherwise
    /// writes the byte there.
    fn ungetc(&mut self, c: u8) -> bool {
        if self.ptr == 0 {
            if self.cnt != 0 {
                return false;
            }
            self.ptr = 1;
        }
        self.ptr -= 1;
        self.buf[self.ptr] = c;
        self.cnt += 1;
        true
    }
}

/// `UtfIntroState` (stream.cpp:15-35), without `uis_error`: no transition leads to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum UtfIntroState {
    Start,
    UtfbeB1,
    Utf32beB2,
    Utf32beBom3,
    Utf32be,
    Utf16be,
    Utf16beBom1,
    UtfleBom1,
    Utf16leBom2,
    Utf32leBom3,
    Utf16le,
    Utf32le,
    Utf8Imp,
    Utf16leImp,
    Utf32leImp3,
    Utf8Bom1,
    Utf8Bom2,
    Utf8,
}

/// `UtfIntroCharType` (stream.cpp:37-47), as a column index of the tables below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UtfIntroCharType {
    Zero,
    Bb,
    Bf,
    Ef,
    Fe,
    Ff,
    Ascii,
    Other,
}

/// `s_introFinalState` (stream.cpp:49-69).
fn intro_final_state(state: UtfIntroState) -> bool {
    use UtfIntroState::*;
    matches!(state, Utf32be | Utf16be | Utf16le | Utf32le | Utf8)
}

/// `s_introTransitions` (stream.cpp:71-112): the next state, by state and character type.
fn intro_transition(state: UtfIntroState, ch: UtfIntroCharType) -> UtfIntroState {
    use UtfIntroState::*;
    const T: [[UtfIntroState; 8]; 18] = [
        // uict00, uictBB, uictBF, uictEF, uictFE, uictFF, uictAscii, uictOther
        [
            UtfbeB1,
            Utf8,
            Utf8,
            Utf8Bom1,
            Utf16beBom1,
            UtfleBom1,
            Utf8Imp,
            Utf8,
        ],
        [Utf32beB2, Utf8, Utf8, Utf8, Utf8, Utf8, Utf16be, Utf8],
        [Utf32be, Utf8, Utf8, Utf8, Utf32beBom3, Utf8, Utf8, Utf8],
        [Utf8, Utf8, Utf8, Utf8, Utf8, Utf32be, Utf8, Utf8],
        [
            Utf32be, Utf32be, Utf32be, Utf32be, Utf32be, Utf32be, Utf32be, Utf32be,
        ],
        [
            Utf16be, Utf16be, Utf16be, Utf16be, Utf16be, Utf16be, Utf16be, Utf16be,
        ],
        [Utf8, Utf8, Utf8, Utf8, Utf8, Utf16be, Utf8, Utf8],
        [Utf8, Utf8, Utf8, Utf8, Utf16leBom2, Utf8, Utf8, Utf8],
        [
            Utf32leBom3,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
        ],
        [
            Utf32le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le,
        ],
        [
            Utf16le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le,
        ],
        [
            Utf32le, Utf32le, Utf32le, Utf32le, Utf32le, Utf32le, Utf32le, Utf32le,
        ],
        [Utf16leImp, Utf8, Utf8, Utf8, Utf8, Utf8, Utf8, Utf8],
        [
            Utf32leImp3,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
            Utf16le,
        ],
        [
            Utf32le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le, Utf16le,
        ],
        [Utf8, Utf8Bom2, Utf8, Utf8, Utf8, Utf8, Utf8, Utf8],
        [Utf8, Utf8, Utf8, Utf8, Utf8, Utf8, Utf8, Utf8],
        [Utf8, Utf8, Utf8, Utf8, Utf8, Utf8, Utf8, Utf8],
    ];
    T[state as usize][ch as usize]
}

/// `s_introUngetCount` (stream.cpp:114-124): how many read bytes go back to the stream.
fn intro_unget_count(state: UtfIntroState, ch: UtfIntroCharType) -> usize {
    const U: [[u8; 8]; 18] = [
        [0, 1, 1, 0, 0, 0, 0, 1],
        [0, 2, 2, 2, 2, 2, 2, 2],
        [3, 3, 3, 3, 0, 3, 3, 3],
        [4, 4, 4, 4, 4, 0, 4, 4],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [2, 2, 2, 2, 2, 0, 2, 2],
        [2, 2, 2, 2, 0, 2, 2, 2],
        [0, 1, 1, 1, 1, 1, 1, 1],
        [0, 2, 2, 2, 2, 2, 2, 2],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [0, 2, 2, 2, 2, 2, 2, 2],
        [0, 3, 3, 3, 3, 3, 3, 3],
        [4, 4, 4, 4, 4, 4, 4, 4],
        [2, 0, 2, 2, 2, 2, 2, 2],
        [3, 3, 0, 3, 3, 3, 3, 3],
        [1, 1, 1, 1, 1, 1, 1, 1],
    ];
    usize::from(U[state as usize][ch as usize])
}

/// `IntroCharTypeOf` (stream.cpp:126-153); `None` is `traits_type::eof()`.
fn intro_char_type_of(ch: Option<u8>) -> UtfIntroCharType {
    let Some(ch) = ch else {
        return UtfIntroCharType::Other;
    };
    match ch {
        0 => UtfIntroCharType::Zero,
        0xBB => UtfIntroCharType::Bb,
        0xBF => UtfIntroCharType::Bf,
        0xEF => UtfIntroCharType::Ef,
        0xFE => UtfIntroCharType::Fe,
        0xFF => UtfIntroCharType::Ff,
        // `(ch > 0) && (ch < 0xFF)`: every other byte.
        _ => UtfIntroCharType::Ascii,
    }
}

/// `Utf8Adjust` (stream.cpp:155-162): one byte of a code point's UTF-8 encoding.
fn utf8_adjust(ch: u32, lead_bits: u32, rshift: u32) -> u8 {
    let header = (((1u32 << lead_bits) - 1) << (8 - lead_bits)) as u8;
    let mask = (0xFFu32 >> (lead_bits + 1)) as u8;
    header | ((ch >> rshift) as u8 & mask)
}

/// `QueueUnicodeCodepoint` (stream.cpp:164-187): a code point as UTF-8, with no check of its
/// range (bits above the 21st are dropped). `Stream::eof()` itself becomes U+FFFD.
fn queue_unicode_codepoint(q: &mut VecDeque<u8>, mut ch: u32) {
    if ch == u32::from(STREAM_EOF) {
        ch = CP_REPLACEMENT_CHARACTER;
    }
    if ch < 0x80 {
        q.push_back(utf8_adjust(ch, 0, 0));
    } else if ch < 0x800 {
        q.push_back(utf8_adjust(ch, 2, 6));
        q.push_back(utf8_adjust(ch, 1, 0));
    } else if ch < 0x10000 {
        q.push_back(utf8_adjust(ch, 3, 12));
        q.push_back(utf8_adjust(ch, 1, 6));
        q.push_back(utf8_adjust(ch, 1, 0));
    } else {
        q.push_back(utf8_adjust(ch, 4, 18));
        q.push_back(utf8_adjust(ch, 1, 12));
        q.push_back(utf8_adjust(ch, 1, 6));
        q.push_back(utf8_adjust(ch, 1, 0));
    }
}

/// `Stream::CharacterSet` (stream.h:52).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharacterSet {
    Utf8,
    Utf16le,
    Utf16be,
    Utf32le,
    Utf32be,
}

/// The members `Stream` changes from `const` functions (`mutable` in C++).
#[derive(Debug)]
struct Inner<'a> {
    input: IStream<'a>,
    readahead: VecDeque<u8>,
}

/// Port of `YAML::Stream` (stream.h:20-73, stream.cpp).
#[derive(Debug)]
pub struct Stream<'a> {
    inner: RefCell<Inner<'a>>,
    mark: Mark,
    char_set: CharacterSet,
}

impl<'a> Stream<'a> {
    /// `Stream(std::istream&)` (stream.cpp:189-245): detects the encoding from the first bytes
    /// (yaml-cpp's state machine for the YAML specification's algorithm), steps back over the
    /// bytes that aren't a byte order mark, and reads the first character.
    pub fn new(input: impl Into<IStream<'a>>) -> Stream<'a> {
        let mut input = input.into();

        let mut intro: [Option<u8>; 4] = [None; 4];
        let mut n_intro_used = 0usize;
        let mut state = UtfIntroState::Start;
        while !intro_final_state(state) {
            let ch = input.get();
            intro[n_intro_used] = ch;
            n_intro_used += 1;
            let char_type = intro_char_type_of(ch);
            let new_state = intro_transition(state, char_type);
            let mut n_ungets = intro_unget_count(state, char_type);
            if n_ungets > 0 {
                input.clear();
                while n_ungets > 0 {
                    n_intro_used -= 1;
                    if let Some(c) = intro[n_intro_used] {
                        input.putback(c);
                    }
                    n_ungets -= 1;
                }
            }
            state = new_state;
        }

        let char_set = match state {
            UtfIntroState::Utf16le => CharacterSet::Utf16le,
            UtfIntroState::Utf16be => CharacterSet::Utf16be,
            UtfIntroState::Utf32le => CharacterSet::Utf32le,
            UtfIntroState::Utf32be => CharacterSet::Utf32be,
            _ => CharacterSet::Utf8,
        };

        let stream = Stream {
            inner: RefCell::new(Inner {
                input,
                readahead: VecDeque::new(),
            }),
            mark: Mark::default(),
            char_set,
        };
        stream.read_ahead_to(0);
        stream
    }

    /// `peek()` (stream.cpp:249-255): the next character, or `Stream::eof()`.
    pub fn peek(&self) -> u8 {
        self.inner
            .borrow()
            .readahead
            .front()
            .copied()
            .unwrap_or(STREAM_EOF)
    }

    /// `operator bool` (stream.cpp:257-260): the input is good, or a character other than
    /// `Stream::eof()` is waiting.
    pub fn is_valid(&self) -> bool {
        let inner = self.inner.borrow();
        inner.input.good() || inner.readahead.front().is_some_and(|&c| c != STREAM_EOF)
    }

    /// `get()` (stream.cpp:262-275): extracts a character and moves the mark past it.
    pub fn get(&mut self) -> u8 {
        let ch = self.peek();
        self.advance_current();
        self.mark.column = self.mark.column.wrapping_add(1);

        if ch == b'\n' {
            self.mark.column = 0;
            self.mark.line = self.mark.line.wrapping_add(1);
        }

        ch
    }

    /// `get(int n)` (stream.cpp:277-287): extracts `n` characters.
    pub fn get_n(&mut self, n: i32) -> Vec<u8> {
        let mut ret = Vec::new();
        if n > 0 {
            ret.reserve(n as usize);
            for _ in 0..n {
                ret.push(self.get());
            }
        }
        ret
    }

    /// `eat(int n)` (stream.cpp:289-294).
    pub fn eat(&mut self, n: i32) {
        for _ in 0..n {
            self.get();
        }
    }

    /// `mark()`
    pub fn mark(&self) -> Mark {
        self.mark
    }

    /// `pos()`
    pub fn pos(&self) -> i32 {
        self.mark.pos
    }

    /// `line()`
    pub fn line(&self) -> i32 {
        self.mark.line
    }

    /// `column()`
    pub fn column(&self) -> i32 {
        self.mark.column
    }

    /// `ResetColumn()`
    pub fn reset_column(&mut self) {
        self.mark.column = 0;
    }

    /// `AdvanceCurrent()` (stream.cpp:296-303).
    fn advance_current(&mut self) {
        let popped = self.inner.get_mut().readahead.pop_front().is_some();
        if popped {
            self.mark.pos = self.mark.pos.wrapping_add(1);
        }
        self.read_ahead_to(0);
    }

    /// `CharAt(i)` (stream.h:76): unchecked in C++. Every caller has checked
    /// `ReadAheadTo(i)` first, so the character exists.
    fn char_at(&self, i: usize) -> u8 {
        self.inner
            .borrow()
            .readahead
            .get(i)
            .copied()
            .unwrap_or(STREAM_EOF)
    }

    /// `ReadAheadTo(i)` (stream.h:78-82) and `_ReadAheadTo(i)` (stream.cpp:305-332): decodes
    /// characters until `i + 1` are waiting or the input ends. Once the input has ended, each
    /// call that needs more characters appends one more `Stream::eof()`.
    pub(crate) fn read_ahead_to(&self, i: usize) -> bool {
        let mut inner = self.inner.borrow_mut();
        if inner.readahead.len() > i {
            return true;
        }
        let Inner { input, readahead } = &mut *inner;
        while input.good() && readahead.len() <= i {
            match self.char_set {
                CharacterSet::Utf8 => stream_in_utf8(input, readahead),
                CharacterSet::Utf16le | CharacterSet::Utf16be => {
                    stream_in_utf16(input, readahead, self.char_set == CharacterSet::Utf16be)
                }
                CharacterSet::Utf32le | CharacterSet::Utf32be => {
                    stream_in_utf32(input, readahead, self.char_set == CharacterSet::Utf32be)
                }
            }
        }

        // signal end of stream
        if !input.good() {
            readahead.push_back(STREAM_EOF);
        }

        readahead.len() > i
    }
}

/// `StreamInUtf8` (stream.cpp:334-339): the byte as it is.
fn stream_in_utf8(input: &mut IStream<'_>, q: &mut VecDeque<u8>) {
    let b = input.next_byte();
    if input.good() {
        q.push_back(b);
    }
}

/// `StreamInUtf16` (stream.cpp:341-386): one UTF-16 unit, or a surrogate pair, as UTF-8. A
/// lone low surrogate, or a high one not followed by a low one, becomes U+FFFD.
fn stream_in_utf16(input: &mut IStream<'_>, q: &mut VecDeque<u8>, big_endian: bool) {
    let n_big_end = if big_endian { 0 } else { 1 };
    let mut bytes = [0u8; 2];

    bytes[0] = input.next_byte();
    bytes[1] = input.next_byte();
    if !input.good() {
        return;
    }
    let mut ch = (u32::from(bytes[n_big_end]) << 8) | u32::from(bytes[1 ^ n_big_end]);

    if (0xDC00..0xE000).contains(&ch) {
        // Trailing (low) surrogate...ugh, wrong order
        queue_unicode_codepoint(q, CP_REPLACEMENT_CHARACTER);
        return;
    }

    if (0xD800..0xDC00).contains(&ch) {
        // ch is a leading (high) surrogate; read the trailing (low) surrogate.
        loop {
            bytes[0] = input.next_byte();
            bytes[1] = input.next_byte();
            if !input.good() {
                queue_unicode_codepoint(q, CP_REPLACEMENT_CHARACTER);
                return;
            }
            let ch_low = (u32::from(bytes[n_big_end]) << 8) | u32::from(bytes[1 ^ n_big_end]);
            if !(0xDC00..0xE000).contains(&ch_low) {
                // Not a low surrogate: dump a replacement character into the stream.
                queue_unicode_codepoint(q, CP_REPLACEMENT_CHARACTER);

                // Deal with the next UTF-16 unit
                if !(0xD800..0xE000).contains(&ch_low) {
                    // Easiest case: queue the codepoint and return
                    queue_unicode_codepoint(q, ch);
                    return;
                }
                // Start the loop over with the new high surrogate
                ch = ch_low;
                continue;
            }

            // Select the payload bits from the high surrogate, then the low one's, and add
            // the surrogacy offset.
            ch &= 0x3FF;
            ch <<= 10;
            ch |= ch_low & 0x3FF;
            ch += 0x10000;
            break;
        }
    }

    queue_unicode_codepoint(q, ch);
}

/// `StreamInUtf32` (stream.cpp:406-429).
fn stream_in_utf32(input: &mut IStream<'_>, q: &mut VecDeque<u8>, big_endian: bool) {
    const INDEXES: [[usize; 4]; 2] = [[3, 2, 1, 0], [0, 1, 2, 3]];
    let indexes = if big_endian { INDEXES[1] } else { INDEXES[0] };

    let mut bytes = [0u8; 4];
    for b in &mut bytes {
        *b = input.next_byte();
    }
    if !input.good() {
        return;
    }

    let mut ch: u32 = 0;
    for &i in &indexes {
        ch <<= 8;
        ch |= u32::from(bytes[i]);
    }

    queue_unicode_codepoint(q, ch);
}

/// Port of `YAML::StreamCharSource` (streamcharsource.h): a position ahead of the stream's
/// current character.
#[derive(Debug, Clone, Copy)]
pub struct StreamCharSource<'s, 'a> {
    offset: usize,
    stream: &'s Stream<'a>,
}

impl<'s, 'a> StreamCharSource<'s, 'a> {
    /// `StreamCharSource(const Stream&)`
    pub fn new(stream: &'s Stream<'a>) -> StreamCharSource<'s, 'a> {
        StreamCharSource { offset: 0, stream }
    }
}

impl Source for StreamCharSource<'_, '_> {
    /// `operator bool` (streamcharsource.h:34-36): `ReadAheadTo(m_offset)`.
    fn is_valid(&self) -> bool {
        self.stream.read_ahead_to(self.offset)
    }

    /// `operator[]` (streamcharsource.h:27).
    fn at(&self, i: usize) -> u8 {
        self.stream.char_at(self.offset + i)
    }

    /// `operator+` (streamcharsource.h:38-45).
    fn plus(&self, i: i32) -> Self {
        let mut source = *self;
        if (self.offset as i32).wrapping_add(i) >= 0 {
            source.offset = self.offset.wrapping_add(i as usize);
        } else {
            source.offset = 0;
        }
        source
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
