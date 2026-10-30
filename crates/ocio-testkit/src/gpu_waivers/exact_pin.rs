// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Five individually approved finite GPU failure pins (owner, 2026-10-30).

use super::Renderer;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactPin {
    approved: bool,
    test_id: String,
    platform: String,
    ocio_version: String,
    language: String,
    run: usize,
    width: usize,
    height: usize,
    components: usize,
    image_filter: String,
    image_wrap: String,
    comparison_entries: usize,
    threshold_bits: String,
    minimum_bits: String,
    relative: bool,
    max_diff_bits: String,
    component: usize,
    input_sha256: String,
    cpu_sha256: String,
    gpu_sha256: String,
    gpu_rgb_bits: Option<[String; 3]>,
    cpu_rgb_bits: Option<[String; 3]>,
    whole_image_gpu_identity_required: bool,
    native_readback_required: bool,
    reject_passing: bool,
    renderer: Renderer,
}

fn bits(text: &str) -> Result<u32, String> {
    if text.len() != 8 {
        return Err("Exact GPU pin requires eight hex digits per f32".into());
    }
    u32::from_str_radix(text, 16).map_err(|e| e.to_string())
}

pub(super) fn rule(
    entry: &toml::Value,
    raw: &toml::Value,
    test_id: &str,
) -> Result<Option<serde_json::Value>, String> {
    let rule: ExactPin = raw
        .clone()
        .try_into()
        .map_err(|e| format!("Exact GPU pin schema: {e}"))?;
    let id = entry
        .get("id")
        .and_then(toml::Value::as_str)
        .ok_or("Exact GPU pin lacks id")?;
    let expected = match id {
        "W0014" => "FixedFunction/style_aces2_nan_bug",
        "W0015" => "FixedFunction/style_aces2_4000nit_p3_rndtrip",
        "W0016" => "FixedFunction/style_aces2_rec709_rndtrip",
        "W0017" => "FixedFunction/style_aces2_rgb_to_jmh_fwd",
        "W0018" => "FixedFunction/style_aces_gamutcomp13_inv",
        _ => return Err("Unapproved exact GPU pin id".into()),
    };
    let renderer = &rule.renderer;
    if rule.test_id != expected
        || !rule.approved
        || rule.platform != "linux"
        || rule.ocio_version != "2.5.2"
        || rule.language != "GPU_LANGUAGE_GLSL_1_2"
        || rule.run != 0
        || (rule.width, rule.height, rule.components) != (256, 256, 4)
        || rule.image_filter != "GL_LINEAR"
        || rule.image_wrap != "GL_CLAMP"
        || rule.comparison_entries == 0
        || rule.comparison_entries > 256 * 256 * 4
        || !rule.comparison_entries.is_multiple_of(4)
        || rule.component >= rule.comparison_entries
        || !rule.whole_image_gpu_identity_required
        || !rule.native_readback_required
        || !rule.reject_passing
        || renderer.vendor != "Mesa"
        || renderer.renderer != "llvmpipe (LLVM 21.1.8, 256 bits)"
        || renderer.version != "4.5 (Compatibility Profile) Mesa 25.2.7"
        || renderer.shading_language_version != "4.50"
        || renderer.egl_vendor != "Mesa Project"
        || renderer.egl_version != "1.5"
        || renderer.max_texture_size != 16384
    {
        return Err("Exact GPU pin has unsupported scope or missing safeguards".into());
    }
    for field in ["approved_by", "date"] {
        if entry
            .get(field)
            .and_then(toml::Value::as_str)
            .is_none_or(|v| v.trim().is_empty())
        {
            return Err("Exact GPU pin lacks owner provenance".into());
        }
    }
    for digest in [&rule.input_sha256, &rule.cpu_sha256, &rule.gpu_sha256] {
        if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Exact GPU pin has an invalid observed image digest".into());
        }
    }
    let error = f32::from_bits(bits(&rule.max_diff_bits)?);
    let threshold = f32::from_bits(bits(&rule.threshold_bits)?);
    let minimum = f32::from_bits(bits(&rule.minimum_bits)?);
    if !error.is_finite()
        || !threshold.is_finite()
        || threshold <= 0.0
        || error <= threshold
        || !minimum.is_finite()
        || minimum <= 0.0
        || rule.relative != (id == "W0014")
    {
        return Err("Exact GPU pin is not the original finite threshold failure".into());
    }
    match (&rule.gpu_rgb_bits, &rule.cpu_rgb_bits) {
        (Some(gpu), Some(cpu)) if id == "W0014" => {
            for value in gpu.iter().chain(cpu) {
                bits(value)?;
            }
        }
        (None, None) if id != "W0014" => {}
        _ => return Err("Only nan_bug requires its observed GPU/CPU RGB bits".into()),
    }
    if rule.test_id != test_id {
        return Ok(None);
    }
    let mut value = serde_json::to_value(raw).map_err(|e| e.to_string())?;
    value["id"] = id.into();
    value["kind"] = "gpu_exact_pin".into();
    Ok(Some(value))
}

#[cfg(test)]
mod tests;
