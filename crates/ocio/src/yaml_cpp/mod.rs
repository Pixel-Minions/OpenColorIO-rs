// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from yaml-cpp 0.8.0 (https://github.com/jbeder/yaml-cpp), which is:
//
//   Copyright (c) 2008-2015 Jesse Beder.
//
//   Permission is hereby granted, free of charge, to any person obtaining a copy
//   of this software and associated documentation files (the "Software"), to deal
//   in the Software without restriction, including without limitation the rights
//   to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//   copies of the Software, and to permit persons to whom the Software is
//   furnished to do so, subject to the following conditions:
//
//   The above copyright notice and this permission notice shall be included in
//   all copies or substantial portions of the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//   IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//   FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//   AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//   LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//   OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
//   THE SOFTWARE.

//! A port of yaml-cpp 0.8.0, the YAML library OpenColorIO 2.5.2 is built with
//! (`share/cmake/modules/FindExtPackages.cmake`); its source is pinned at `upstream/yaml-cpp`.
//!
//! - **The emitter.** OCIO's `Config::serialize()` is a sequence of calls to it
//!   (`OCIOYaml.cpp` `save` functions), so its layout, quoting and number formatting are part
//!   of OCIO's byte-exact output. The files mirror yaml-cpp's: `emitter.cpp`,
//!   `emitterstate.cpp`, `emitterutils.cpp`, `emittermanip.h`, `ostream_wrapper.cpp`,
//!   `regex_yaml.cpp`/`regeximpl.h` and `exp.h`. The emitter writes to its own buffer (the
//!   `Emitter()` constructor OCIO uses); yaml-cpp's `std::ostream` mode is not ported.
//! - **The parser.** OCIO reads a config with `YAML::Load` and the node API. Its accepted
//!   syntax, its lenient decoding and its error texts and marks ("yaml-cpp: error at line 3,
//!   column 5: ...") are part of what OCIO accepts and reports. The files mirror yaml-cpp's:
//!   `stream.cpp`, `exceptions.h`, `mark.h`, and the scanner, parser and node files.
//!
//! Each item cites the yaml-cpp source it translates.
//!
//! yaml-cpp is MIT-licensed; its notice is above and applies to this module.

pub mod emitter;
pub mod emitter_manip;
pub mod emitter_state;
pub mod emitter_utils;
pub mod exceptions;
pub mod exp;
pub mod mark;
pub mod ostream_wrapper;
pub mod regex_yaml;
pub mod stream;

pub use emitter::{Emittable, Emitter};
pub use emitter_manip::{
    Alias, Anchor, Binary, Comment, EmitterManip, Indent, Null, Precision, Tag, TagType,
    double_precision, float_precision, local_tag, local_tag_with_prefix, precision, secondary_tag,
    verbatim_tag,
};
