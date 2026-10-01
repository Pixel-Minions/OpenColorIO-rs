// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Test support for the port (PLAN.md §7–§8).
//!
//! - [`oracle`]: runs the real OpenColorIO 2.5.2 (the `opencolorio==2.5.2` wheel, pinned in
//!   `oracle/uv.lock`) on this machine and returns its output. Pixel checks always run live,
//!   because OCIO's kernel choice and math library belong to the machine.
//! - [`gpu`]: typed requests and replies of the oracle's `gpu_shader` command, a GPU
//!   processor's shader with its uniforms and textures.
//! - [`gpu_desc`]: typed requests and replies of the oracle's `gpu_shader_desc` command, a
//!   shader description driven through the calls Python has.
//! - [`processor_ops`]: typed requests and replies of the oracle's `processor_ops` command,
//!   what the optimizer makes of a processor, each transform with all its getters.
//! - [`transform_text`]: typed requests and replies of the oracle's `transform_text` command,
//!   transforms' `repr()`, validation and equality.
//! - [`fixtures`]: committed, oracle-generated text fixtures, verified against
//!   `fixtures/MANIFEST.toml` on every read.
//! - [`compare`]: exact comparators. There is no tolerance anywhere in this crate except in
//!   [`upstream`]; any other check that needs one needs a waiver in `waivers.toml`, approved
//!   by the owner.
//! - [`image`]: requests for the oracle's image commands: images described every way
//!   PyOpenColorIO allows, a CPU processor applied to them, every buffer returned byte for byte.
//! - [`probe`]: deterministic probe inputs and the battery's probe sets (all half bit
//!   patterns, specials, seeded random values in named ranges, ±N ulp neighbourhoods, NaN
//!   buffers of every length, the sweep of every `f32`).
//! - [`crt`]: the platform C runtime (UCRT or glibc) through FFI, the reference for C and
//!   iostream number formatting and for `strtod`-style parsing.
//! - [`upstream`]: upstream's own tolerance checks (`OCIO_CHECK_CLOSE`,
//!   `EqualWithSafeRelError`, ...), for ported upstream tests only.
//!
//! Expected values come only from the oracle or from upstream's tests. Never from the port.
#![deny(unsafe_code)]

pub mod battery;
pub mod compare;
pub mod crt;
pub mod fixtures;
pub mod gpu;
pub mod gpu_desc;
pub mod image;
pub mod oracle;
pub mod paths;
pub mod probe;
pub mod processor_ops;
pub mod transform_text;
pub mod upstream;

pub use compare::{assert_bytes_eq, assert_f32_bits_eq, assert_text_eq};
pub use oracle::Oracle;
