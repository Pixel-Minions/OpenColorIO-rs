// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::Mark` (include/yaml-cpp/mark.h, yaml-cpp 0.8.0): a position in the input.

/// Port of `YAML::Mark` (mark.h:13-26): the character offset, line and column, each counted
/// from 0 as C++ `int`s.
///
/// The stream counts them with `++` on `int`, which overflows past 2^31 characters or lines;
/// both wheels compile it as a wrapping add, and the port wraps too (`docs/improvements.md`
/// I-100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mark {
    pub pos: i32,
    pub line: i32,
    pub column: i32,
}

impl Mark {
    /// `Mark::null_mark()` (mark.h:16): every field -1.
    pub const fn null_mark() -> Mark {
        Mark {
            pos: -1,
            line: -1,
            column: -1,
        }
    }

    /// `Mark::is_null()` (mark.h:18).
    pub fn is_null(&self) -> bool {
        self.pos == -1 && self.line == -1 && self.column == -1
    }
}
