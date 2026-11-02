// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Test-only port of upstream's EGL/OpenGL harness on Mesa llvmpipe.
//!
//! Owner decision P7-3: execution is Linux-only. Windows verifies shader text,
//! uniform/texture parity and HLSL compilation; this crate is empty there, except under the
//! `shader-corpus` feature of `cargo xtask compile-shaders` (P7-5). With that feature, on
//! either platform, nothing renders: [`gpu_unit_test::GpuUnitTest::run`] compares the port's
//! shaders with the wheel's in every compiled language and records them for the compilers.
#![cfg(any(target_os = "linux", feature = "shader-corpus"))]
#![deny(unsafe_code)]

#[cfg(not(feature = "shader-corpus"))]
mod exact_pin;
#[cfg(not(feature = "shader-corpus"))]
mod glsl;
pub mod gpu_unit_test;
#[cfg(not(feature = "shader-corpus"))]
mod native_reference;
#[cfg(not(feature = "shader-corpus"))]
mod nonfinite_class;
#[cfg(not(feature = "shader-corpus"))]
pub mod oglapp;
mod shader_settings;
#[cfg(not(feature = "shader-corpus"))]
mod wheel_shader;

#[cfg(not(feature = "shader-corpus"))]
pub use glsl::UniformUpload;
#[cfg(not(feature = "shader-corpus"))]
pub use oglapp::{Components, GlInfo, HeadlessApp};
