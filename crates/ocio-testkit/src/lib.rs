// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Test support for the port (PLAN.md §7–§8).
//!
//! - [`oracle`]: runs the real OpenColorIO 2.5.2 (the `opencolorio==2.5.2` wheel, pinned in
//!   `oracle/uv.lock`) on this machine and returns its output. Pixel checks always run live,
//!   because OCIO's kernel choice and math library belong to the machine.
//! - [`fixtures`]: committed, oracle-generated text fixtures, verified against
//!   `fixtures/MANIFEST.toml` on every read.
//! - [`compare`]: exact comparators. There is no tolerance anywhere in this crate except in
//!   [`upstream`]; any other check that needs one needs a waiver in `waivers.toml`, approved
//!   by the owner.
//! - [`probe`]: deterministic probe inputs (all half bit patterns, specials, ramps, random).
//! - [`crt`]: the platform C runtime (UCRT or glibc) through FFI, the reference for C and
//!   iostream number formatting and for `strtod`-style parsing.
//! - [`upstream`]: upstream's own tolerance checks (`OCIO_CHECK_CLOSE`,
//!   `EqualWithSafeRelError`, ...), for ported upstream tests only.
//!
//! Expected values come only from the oracle or from upstream's tests. Never from the port.
#![deny(unsafe_code)]

pub mod compare;
pub mod crt;
pub mod fixtures;
pub mod oracle;
pub mod paths;
pub mod probe;
pub mod upstream;

pub use compare::{assert_bytes_eq, assert_f32_bits_eq, assert_text_eq};
pub use oracle::Oracle;
