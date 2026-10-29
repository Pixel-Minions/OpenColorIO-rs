// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Owner-approved 2026-10-29 CPU/GPU class: exact GPU identity, finite sampling
//! footprints, complete finite companion, and independent native platform proof.
//! This is authored acceptance policy; upstream's comparison arithmetic is reused.

use ocio_ops::{Exception, Result};
use ocio_testkit::gpu::{GpuLanguage, Shader, UniformValue};
use ocio_testkit::upstream::gpu::{
    COMPONENTS, ComparisonSettings, ENTRIES, HEIGHT, LimitsDiff, WIDTH, compare_image,
    compute_diff, validate_inf, validate_nan,
};
use serde_json::Value;

use crate::GlInfo;

pub(super) struct Approval {
    pub id: String,
    pub retained_legacy: bool,
    scope: Value,
}

pub(super) fn approval(test_id: &str) -> Result<Option<Approval>> {
    let rules = ocio_testkit::gpu_waivers::gpu_renderer_waivers(test_id).map_err(Exception::new)?;
    let retained_legacy = rules
        .iter()
        .any(|r| r["kind"].as_str() != Some("gpu_nonfinite"));
    let mut classes = rules
        .into_iter()
        .filter(|r| r["kind"].as_str() == Some("gpu_nonfinite"));
    let Some(scope) = classes.next() else {
        return Ok(None);
    };
    if classes.next().is_some() {
        return Err(Exception::new("Ambiguous GPU non-finite class"));
    }
    Ok(Some(Approval {
        id: scope["id"].as_str().expect("validated class id").into(),
        retained_legacy,
        scope,
    }))
}

pub(super) struct Environment<'a> {
    pub info: &'a GlInfo,
    pub native: Option<&'a GlInfo>,
    pub language: GpuLanguage,
}

pub(super) struct Companion<'a> {
    pub cpu: &'a [f32],
    pub port: &'a [f32],
    pub wheel: &'a [f32],
}

pub(super) struct Frame<'a> {
    pub inputs: &'a [f32],
    pub cpu: &'a [f32],
    pub port: &'a [f32],
    pub wheel: &'a [f32],
    pub entries: usize,
    pub settings: ComparisonSettings,
    pub companion: Option<Companion<'a>>,
}

fn reject(message: &str) -> Exception {
    Exception::new(format!("GPU non-finite class: {message}"))
}

fn same_bits(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits())
}

fn same_renderer(a: &GlInfo, b: &GlInfo) -> bool {
    a.vendor == b.vendor
        && a.renderer == b.renderer
        && a.version == b.version
        && a.shading_language_version == b.shading_language_version
        && a.egl_vendor == b.egl_vendor
        && a.egl_version == b.egl_version
        && a.max_texture_size == b.max_texture_size
}

fn scope_matches(scope: &Value, info: &GlInfo) -> bool {
    let renderer = &scope["renderer"];
    [
        ("vendor", &info.vendor),
        ("renderer", &info.renderer),
        ("version", &info.version),
        ("shading_language_version", &info.shading_language_version),
        ("egl_vendor", &info.egl_vendor),
        ("egl_version", &info.egl_version),
    ]
    .iter()
    .all(|(key, value)| renderer[*key].as_str() == Some(value.as_str()))
        && renderer["max_texture_size"].as_i64() == Some(i64::from(info.max_texture_size))
}

/// Every potential texel participates, including zero-weight neighbors. The
/// existing fixed quad samples centers one-to-one; outside GL_CLAMP border is
/// the unchanged finite zero border. No transform or channel mask is inferred.
fn finite_footprint(inputs: &[f32], pixel: usize) -> bool {
    let (x, y) = (pixel % WIDTH, pixel / WIDTH);
    (y..=(y + 1).min(HEIGHT - 1)).all(|yy| {
        (x..=(x + 1).min(WIDTH - 1)).all(|xx| {
            inputs[(yy * WIDTH + xx) * COMPONENTS..(yy * WIDTH + xx + 1) * COMPONENTS]
                .iter()
                .all(|v| v.is_finite())
        })
    })
}

pub(super) fn finite_shader_parameters(shader: &Shader) -> bool {
    let finite_uniforms = shader.uniforms.iter().all(|u| match &u.value {
        UniformValue::Double(v) => v.is_finite(),
        UniformValue::Float3(v) => v.iter().all(|v| v.is_finite()),
        UniformValue::VectorFloat(v) => v.iter().all(|v| v.is_finite()),
        UniformValue::Bool(_) | UniformValue::VectorInt(_) => true,
        UniformValue::Unknown => false,
    });
    let finite_text = shader
        .text
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .all(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "nan" | "inf" | "infinity"
            )
        });
    finite_uniforms
        && finite_text
        && shader
            .textures
            .iter()
            .all(|t| t.values.iter().all(|v| v.is_finite()))
        && shader
            .textures_3d
            .iter()
            .all(|t| t.values.iter().all(|v| v.is_finite()))
}

/// The full original comparison has already run. Returns only actual excepted
/// component indices; original finite-footprint values retain upstream rules.
pub(super) fn assess(
    rule: &Approval,
    environment: Environment<'_>,
    frame: Frame<'_>,
) -> Result<Vec<usize>> {
    if environment.language != GpuLanguage::Glsl12
        || !scope_matches(&rule.scope, environment.info)
        || environment
            .native
            .is_none_or(|native| !same_renderer(native, environment.info))
    {
        return Err(reject("unverified renderer, language, or native controls"));
    }
    if frame.inputs.len() != ENTRIES
        || frame.port.len() != ENTRIES
        || frame.wheel.len() != ENTRIES
        || frame.entries == 0
        || frame.entries > ENTRIES
        || !frame.entries.is_multiple_of(COMPONENTS)
        || frame.cpu.len() != frame.entries
        || frame.settings.print_min_max
        || !frame.settings.error_threshold.is_finite()
        || frame.settings.error_threshold < 0.0
        || !frame.settings.expected_minimal_value.is_finite()
        || frame.settings.expected_minimal_value <= 0.0
    {
        return Err(reject("invalid frame dimensions or diagnostic settings"));
    }
    if !same_bits(frame.port, frame.wheel) {
        return Err(reject("port/official GPU bits differ"));
    }
    if frame.inputs.iter().all(|v| v.is_finite()) {
        return Err(reject("finite-only image cannot use this class"));
    }
    let mut cpu = Vec::new();
    let mut gpu = Vec::new();
    for pixel in 0..frame.entries / COMPONENTS {
        if finite_footprint(frame.inputs, pixel) {
            cpu.extend_from_slice(&frame.cpu[pixel * COMPONENTS..(pixel + 1) * COMPONENTS]);
            gpu.extend_from_slice(&frame.port[pixel * COMPONENTS..(pixel + 1) * COMPONENTS]);
        }
    }
    if !cpu.is_empty()
        && !compare_image(&cpu, &gpu, cpu.len(), frame.settings).passes(frame.settings)
    {
        return Err(reject("original finite-footprint comparison failed"));
    }
    let companion = frame
        .companion
        .ok_or_else(|| reject("missing complete finite companion"))?;
    if companion.cpu.len() != ENTRIES
        || companion.port.len() != ENTRIES
        || companion.wheel.len() != ENTRIES
    {
        return Err(reject("incomplete finite companion"));
    }
    if !same_bits(companion.port, companion.wheel) {
        return Err(reject("finite companion port/official GPU bits differ"));
    }
    if !compare_image(companion.cpu, companion.port, ENTRIES, frame.settings).passes(frame.settings)
    {
        return Err(reject("complete finite companion comparison failed"));
    }
    let mut excepted = Vec::new();
    for i in 0..frame.entries {
        let (a, b) = (frame.cpu[i], frame.port[i]);
        let failed = match validate_inf(a, b) {
            LimitsDiff::Inconsistent => true,
            LimitsDiff::Consistent => false,
            LimitsDiff::NotApplicable => match validate_nan(a, b) {
                LimitsDiff::Inconsistent => true,
                LimitsDiff::Consistent => false,
                LimitsDiff::NotApplicable => {
                    let mut diff = 0.0;
                    compute_diff(
                        a,
                        b,
                        frame.settings.relative,
                        frame.settings.expected_minimal_value,
                        &mut diff,
                    );
                    diff > frame.settings.error_threshold
                }
            },
        };
        if failed {
            excepted.push(i);
        }
    }
    Ok(excepted)
}

#[cfg(test)]
mod tests;
