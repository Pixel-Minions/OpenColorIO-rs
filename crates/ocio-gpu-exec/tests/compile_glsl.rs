// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Chunk 7.4a (owner decision P7-5): the wheel's shaders in the six GLSL languages, which the
//! capture asserted equal to the port's, compiled by `glslangValidator` (`-V` for Vulkan GLSL).
//!
//! glslang is in the Rocky Linux 9 image (P7-4), so this target is Linux only, and it exists
//! only under `ocio-gpu-exec`'s `shader-corpus` feature: `cargo xtask compile-shaders` builds it
//! and runs it after capturing the corpus. Each check is strict: `ocio_testkit::shader_compile`
//! holds no list of expected failures. A wheel shader that fails to compile goes to the owner
//! (`docs/cards/phase7.md` WP 7.4).
#![cfg(all(target_os = "linux", feature = "shader-corpus"))]

use ocio_testkit::gpu::GpuLanguage::{self, Glsl12, Glsl13, Glsl40, GlslEs10, GlslEs30, GlslVk46};
use ocio_testkit::shader_compile::{Corpus, GLSL_LANGUAGES, Tool, check, check_controls};

/// A complete small shader as OCIO writes it (GpuShaderText's own shape).
const VALID: &str =
    "vec4 OCIOMain(vec4 inPixel)\n{\n  vec4 outColor = inPixel;\n  return outColor;\n}\n";

#[test]
fn control_glslang() {
    for language in GLSL_LANGUAGES {
        let name = format!("control_{language:?}");
        check_controls(&name, Tool::Glslang, language, VALID);
    }
}

fn suite(name: &str, language: GpuLanguage) {
    check(name, Corpus::Suite, language, Tool::Glslang);
}

fn family(name: &str, language: GpuLanguage) {
    check(name, Corpus::Family, language, Tool::Glslang);
}

#[test]
fn suite_glsl_1_2() {
    suite("suite_glsl_1_2", Glsl12);
}

#[test]
fn suite_glsl_1_3() {
    suite("suite_glsl_1_3", Glsl13);
}

#[test]
fn suite_glsl_4_0() {
    suite("suite_glsl_4_0", Glsl40);
}

#[test]
fn suite_glsl_es_1_0() {
    suite("suite_glsl_es_1_0", GlslEs10);
}

#[test]
fn suite_glsl_es_3_0() {
    suite("suite_glsl_es_3_0", GlslEs30);
}

#[test]
fn suite_glsl_vk_4_6() {
    suite("suite_glsl_vk_4_6", GlslVk46);
}

#[test]
fn family_glsl_1_2() {
    family("family_glsl_1_2", Glsl12);
}

#[test]
fn family_glsl_1_3() {
    family("family_glsl_1_3", Glsl13);
}

#[test]
fn family_glsl_4_0() {
    family("family_glsl_4_0", Glsl40);
}

#[test]
fn family_glsl_es_1_0() {
    family("family_glsl_es_1_0", GlslEs10);
}

#[test]
fn family_glsl_es_3_0() {
    family("family_glsl_es_3_0", GlslEs30);
}

#[test]
fn family_glsl_vk_4_6() {
    family("family_glsl_vk_4_6", GlslVk46);
}
