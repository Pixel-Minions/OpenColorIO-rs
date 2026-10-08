// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from expat 2.7.2 (https://github.com/libexpat/libexpat), which is:
//
//   Copyright (c) 1998-2000 Thai Open Source Software Center Ltd and Clark Cooper
//   Copyright (c) 2001-2025 Expat maintainers
//
//   Permission is hereby granted, free of charge, to any person obtaining
//   a copy of this software and associated documentation files (the
//   "Software"), to deal in the Software without restriction, including
//   without limitation the rights to use, copy, modify, merge, publish,
//   distribute, sublicense, and/or sell copies of the Software, and to
//   permit persons to whom the Software is furnished to do so, subject to
//   the following conditions:
//
//   The above copyright notice and this permission notice shall be included
//   in all copies or substantial portions of the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
//   EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//   MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
//   IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//   CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
//   TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
//   SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! A port of the parts of expat 2.7.2 that OpenColorIO 2.5.2 reaches (owner decision P4-1):
//! the XML parser its CTF/CLF, CDL/CC/CCC and Iridas look readers feed one line at a time.
//! Which line an error surfaces on, the error's text (`XML_ErrorString`), and how character
//! data is split between callbacks are expat's, and part of OCIO's output.
//!
//! **The version.** Both wheels bundle expat 2.7.2: the Linux wheel's `libOpenColorIO.so`
//! carries `expat_2.7.2` (`XML_ExpatVersion`'s string); the Windows wheel's DLL, which keeps no
//! version string, carries the string literals of `xmlparse.c` 2.7.2 (2.7.2 added
//! `EXPAT_MALLOC_DEBUG` and the allocation tracker's message, which 2.7.1 lacks; 2.7.3 adds no
//! literal), and OCIO 2.5.2 builds expat 2.7.2 where it installs it
//! (`RECOMMENDED_VERSION 2.7.2`, `share/cmake/modules/FindExtPackages.cmake:57-60`). Its source
//! is pinned as the submodule `upstream/expat` (tag `R_2_7_2`), so the citations can be
//! checked; they are relative to `expat/lib/`.
//!
//! **The build.** As the wheels build it (expat's CMake defaults): `XML_DTD` and `XML_GE`
//! (the DTD, general entities and the amplification limits), `XML_NS` (compiled in; OCIO
//! creates its parsers without namespace processing), `XML_CONTEXT_BYTES` 1024, no
//! `XML_UNICODE` (`XML_Char` is `char`, and handlers see UTF-8), no `XML_MIN_SIZE`, no
//! `XML_LARGE_SIZE`.
//!
//! - [`xmltok`]: the tokenizer's encodings, the XML declaration, the conversions to UTF-8
//!   (`xmltok.c`, `xmltok_ns.c`).
//! - [`xmltok_impl`]: the tokenizer's scanners (`xmltok_impl.c`).
//! - `tables`: the character tables (`nametab.h` and the byte-type tables).
//!
//! Each item cites the expat source it translates. expat is MIT-licensed; its notice is above
//! and applies to this module.

mod tables;
pub mod xmltok;
pub mod xmltok_impl;
