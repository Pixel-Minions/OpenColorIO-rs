// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! GPU shader generation: a port of `GpuShader*.cpp`, `GPUProcessor.cpp` and the ops'
//! GPU writers from OpenColorIO 2.5.2.
//!
//! - [`gpu_shader_utils`]: `GpuShaderText`, which writes shader code line by line in each
//!   language, and the helpers the writers share.
//! - [`gpu_shader_class_wrapper`]: the class wrappers OSL and MSL shaders put around OCIO's
//!   function.
//! - [`gpu_shader_desc`]: [`GpuShaderDesc`], the description of a shader program that a GPU
//!   processor fills in; [`gpu_shader`]: its uniforms and textures.
//! - [`open_color_types`]: the public enums only the GPU side uses ([`GpuLanguage`],
//!   [`UniformDataType`]).
#![forbid(unsafe_code)]

pub mod gpu_shader;
pub mod gpu_shader_class_wrapper;
pub mod gpu_shader_desc;
pub mod gpu_shader_utils;
pub mod open_color_types;

pub use gpu_shader_desc::GpuShaderDesc;
pub use open_color_types::{GpuLanguage, UniformDataType};
