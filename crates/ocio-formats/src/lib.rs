// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! File formats: a port of `src/OpenColorIO/fileformats/` from OpenColorIO 2.5.2.
//!
//! This crate is a building block of [`opencolorio-rs`](https://crates.io/crates/opencolorio-rs):
//! use that crate. Its API follows OpenColorIO's internals and may change in minor releases.
#![forbid(unsafe_code)]

pub mod expat;
pub mod fileformats;
