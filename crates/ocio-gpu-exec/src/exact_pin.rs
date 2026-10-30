// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Owner-approved individual finite failure fingerprints; no comparison limit changes.

use ocio_ops::{Exception, Result};
use ocio_testkit::gpu::GpuLanguage;
use ocio_testkit::upstream::gpu::{ENTRIES, ImageComparison, input_digest};
use serde_json::Value;

use crate::GlInfo;
use crate::nonfinite_class::{Environment, Frame};

pub(super) struct Approval {
    pub id: String,
    scope: Value,
}

pub(super) fn approval(test_id: &str) -> Result<Option<Approval>> {
    let rules = ocio_testkit::gpu_waivers::gpu_renderer_waivers(test_id).map_err(Exception::new)?;
    let mut pins = rules.into_iter().filter(|r| r["kind"] == "gpu_exact_pin");
    let Some(scope) = pins.next() else {
        return Ok(None);
    };
    if pins.next().is_some() {
        return Err(reject("ambiguous test pin"));
    }
    Ok(Some(Approval {
        id: scope["id"].as_str().expect("validated id").into(),
        scope,
    }))
}

fn reject(message: &str) -> Exception {
    Exception::new(format!("Exact GPU failure pin: {message}"))
}

fn renderer_matches(scope: &Value, info: &GlInfo) -> bool {
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

fn bits(scope: &Value, key: &str) -> u32 {
    u32::from_str_radix(scope[key].as_str().expect("validated bits"), 16).expect("validated f32")
}

/// This runs for an active pin even when upstream starts passing. Every live output is still
/// obtained normally, with complete port/wheel identity checked before accepting a failure.
pub(super) fn assess(
    pin: &Approval,
    environment: Environment<'_>,
    frame: Frame<'_>,
    run: usize,
    result: &ImageComparison,
) -> Result<Value> {
    let scope = &pin.scope;
    if environment.language != GpuLanguage::Glsl12
        || !renderer_matches(scope, environment.info)
        || environment
            .native
            .is_none_or(|info| !renderer_matches(scope, info))
    {
        return Err(reject(
            "renderer, language or native readback proof differs",
        ));
    }
    if run != 0
        || scope["run"].as_u64() != Some(run as u64)
        || scope["comparison_entries"].as_u64() != Some(frame.entries as u64)
        || frame.cpu.len() != frame.entries
        || frame.inputs.len() != ENTRIES
        || frame.port.len() != ENTRIES
        || frame.wheel.len() != ENTRIES
    {
        return Err(reject("original run or image shape differs"));
    }
    if frame.settings.print_min_max
        || frame.settings.error_threshold.to_bits() != bits(scope, "threshold_bits")
        || frame.settings.expected_minimal_value.to_bits() != bits(scope, "minimum_bits")
        || scope["relative"].as_bool() != Some(frame.settings.relative)
    {
        return Err(reject("original upstream comparison settings differ"));
    }
    if result.passes(frame.settings) {
        return Err(reject(
            "original upstream test now passes; retire/review the pin",
        ));
    }
    if result.max_diff.to_bits() != bits(scope, "max_diff_bits")
        || result.index_diff.map(|i| i as u64) != scope["component"].as_u64()
        || result.index_inf.is_some()
        || result.index_nan.is_some()
    {
        return Err(reject(
            "observed failure bits, component or category changed",
        ));
    }
    if !frame
        .port
        .iter()
        .zip(frame.wheel)
        .all(|(a, b)| a.to_bits() == b.to_bits())
    {
        return Err(reject("whole-image port/wheel GPU bits differ"));
    }
    let input = input_digest(frame.inputs);
    let cpu = input_digest(frame.cpu);
    let gpu = input_digest(frame.wheel);
    if scope["input_sha256"].as_str() != Some(input.as_str())
        || scope["cpu_sha256"].as_str() != Some(cpu.as_str())
        || scope["gpu_sha256"].as_str() != Some(gpu.as_str())
    {
        return Err(reject("recorded complete input/CPU/GPU bits changed"));
    }
    for (key, values) in [("gpu_rgb_bits", frame.wheel), ("cpu_rgb_bits", frame.cpu)] {
        if let Some(expected) = scope[key].as_array() {
            for (value, expected) in values[..3].iter().zip(expected) {
                let expected =
                    u32::from_str_radix(expected.as_str().expect("validated RGB bits"), 16)
                        .expect("validated RGB bits");
                if value.to_bits() != expected {
                    return Err(reject("nan_bug's reported GPU/CPU RGB bits changed"));
                }
            }
        }
    }
    Ok(
        serde_json::json!({"id":pin.id,"run":run,"max_diff_bits":format!("{:08x}",result.max_diff.to_bits()),
        "component":result.index_diff,"input_sha256":input,"cpu_sha256":cpu,"gpu_sha256":gpu,
        "whole_image_gpu_identity":true,"original_failure_unchanged":true,"reject_passing":true,
        "renderer":environment.info.renderer,"version":environment.info.version}),
    )
}

#[cfg(test)]
mod tests;
