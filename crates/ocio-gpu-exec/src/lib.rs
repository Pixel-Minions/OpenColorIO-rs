// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Test-only port of upstream's EGL/OpenGL harness on Mesa llvmpipe.
//!
//! Owner decision P7-3: execution is Linux-only. Windows verifies shader text,
//! uniform/texture parity and HLSL compilation; this crate is empty there.
#![cfg(target_os = "linux")]
#![deny(unsafe_code)]

mod glsl;
pub mod oglapp;

pub use oglapp::{Components, GlInfo, HeadlessApp};
