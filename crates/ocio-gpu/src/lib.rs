// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! GPU shader generation: a port of `GpuShader*.cpp`, `GPUProcessor.cpp` and the ops'
//! GPU writers from OpenColorIO 2.5.2.
//!
//! - [`gpu_shader_utils`]: `GpuShaderText`, which writes shader code line by line in each
//!   language, and the helpers the writers share.
//! - [`open_color_types`]: the public enums only the GPU side uses ([`GpuLanguage`]).
#![forbid(unsafe_code)]

pub mod gpu_shader_utils;
pub mod open_color_types;

pub use open_color_types::GpuLanguage;
