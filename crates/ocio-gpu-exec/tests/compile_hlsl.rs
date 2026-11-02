// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Chunk 7.4b (owner decision P7-5): the wheel's HLSL shaders, which the capture asserted equal
//! to the port's, compiled by the Windows SDK's `fxc.exe /T ps_5_0` and `dxc.exe /T ps_6_0`.
//!
//! The compilers are Windows' own (P7-5), so this target is Windows only, and it exists only
//! under `ocio-gpu-exec`'s `shader-corpus` feature: `cargo xtask compile-shaders` builds it and
//! runs it after capturing the corpus. Each check is strict: `ocio_testkit::shader_compile`
//! holds no list of expected failures. A wheel shader that fails to compile goes to the owner
//! (`docs/cards/phase7.md` WP 7.4).
#![cfg(all(windows, feature = "shader-corpus"))]

use ocio_testkit::gpu::GpuLanguage::HlslSm50;
use ocio_testkit::shader_compile::{Corpus, Tool, check, check_controls};

/// A complete small shader as OCIO writes it (GpuShaderText's own shape).
const VALID: &str =
    "float4 OCIOMain(float4 inPixel)\n{\n  float4 outColor = inPixel;\n  return outColor;\n}\n";

#[test]
fn control_fxc() {
    check_controls("control_fxc", Tool::Fxc, HlslSm50, VALID);
}

#[test]
fn control_dxc() {
    check_controls("control_dxc", Tool::Dxc, HlslSm50, VALID);
}

#[test]
fn suite_hlsl_fxc() {
    check("suite_hlsl_fxc", Corpus::Suite, HlslSm50, Tool::Fxc);
}

#[test]
fn suite_hlsl_dxc() {
    check("suite_hlsl_dxc", Corpus::Suite, HlslSm50, Tool::Dxc);
}

#[test]
fn family_hlsl_fxc() {
    check("family_hlsl_fxc", Corpus::Family, HlslSm50, Tool::Fxc);
}

#[test]
fn family_hlsl_dxc() {
    check("family_hlsl_dxc", Corpus::Family, HlslSm50, Tool::Dxc);
}
