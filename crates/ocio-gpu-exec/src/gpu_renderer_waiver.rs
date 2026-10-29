// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Owner-approved P7-6 scope enforcement; runtime validation is pending.

use ocio_ops::{Exception, Result};
use ocio_testkit::upstream::gpu::{COMPONENTS, HEIGHT, ImageComparison, WIDTH, input_digest};

use crate::GlInfo;
use crate::gpu_unit_test::GpuUnitTest;

fn bits(value: f32) -> String {
    format!("{:08x}", value.to_bits())
}

/// The original complete image comparison has already run and failed. Only its
/// exact signed NaN-classification discrepancy may be accepted; the numeric/Inf
/// results of that same complete scan and whole GPU bit identity stay mandatory.
pub(super) fn matching(
    test: &GpuUnitTest,
    run: usize,
    info: &GlInfo,
    buffers: [&[f32]; 3], // live CPU, port GPU, wheel GPU, in that order
    entries: usize,
    comparison: &ImageComparison,
) -> Result<Option<String>> {
    let [cpu, port, wheel] = buffers;
    let test_id = format!("{}/{}", test.group, test.name);
    let rules =
        ocio_testkit::gpu_waivers::gpu_renderer_waivers(&test_id).map_err(Exception::new)?;
    if rules.is_empty() {
        return Ok(None);
    }
    let Some(processor) = test.processor() else {
        return Ok(None);
    };
    let processor_cache_id = processor.cache_id()?;
    if !cfg!(target_os = "linux")
        || !test.enabled
        || test.verbose
        || entries != 262_144
        || cpu.len() != entries
        || port.len() != entries
        || wheel.len() != entries
        || test.values.input_values.len() != entries
        || !port
            .iter()
            .zip(wheel)
            .all(|(a, b)| a.to_bits() == b.to_bits())
        || !comparison.max_diff.is_finite()
        || comparison.max_diff > test.comparison.error_threshold
        || comparison.index_inf.is_some()
        || test.comparison.print_min_max
    {
        return Ok(None);
    }
    // These are ALL inconsistencies across the unchanged comparison rectangle.
    // No index is dropped because it lies outside a guessed special-value block.
    let actual: Vec<usize> = (0..entries)
        .filter(|&i| cpu[i].is_nan() != port[i].is_nan() || cpu[i].is_nan() != wheel[i].is_nan())
        .collect();
    if actual.is_empty() || comparison.index_nan != actual.last().copied() {
        return Ok(None);
    }
    let digest = input_digest(&test.values.input_values);
    let mut applied = None;
    for rule in rules {
        let r = &rule["renderer"];
        let s = &test.comparison;
        if rule["test_id"].as_str() != Some(test_id.as_str())
            || rule["processor_cache_id"].as_str() != Some(processor_cache_id.as_str())
            || rule["platform"].as_str() != Some("linux")
            || rule["ocio_version"].as_str() != Some("2.5.2")
            || rule["input_sha256"].as_str() != Some(digest.as_str())
            || rule["width"].as_u64() != Some(WIDTH as u64)
            || rule["height"].as_u64() != Some(HEIGHT as u64)
            || rule["components"].as_u64() != Some(COMPONENTS as u64)
            || rule["entries"].as_u64() != Some(entries as u64)
            || rule["original_entries"].as_u64()
                != Some(test.values.original_input_value_size as u64)
            || rule["run"].as_u64() != Some(run as u64)
            || rule["retests"].as_u64() != Some(test.num_retests() as u64)
            || rule["threshold_bits"].as_str() != Some(bits(s.error_threshold).as_str())
            || rule["minimal_value_bits"].as_str() != Some(bits(s.expected_minimal_value).as_str())
            || rule["relative"].as_bool() != Some(s.relative)
            || rule["print_min_max"].as_bool() != Some(s.print_min_max)
            || rule["nan"].as_bool() != Some(test.test_nan)
            || rule["infinity"].as_bool() != Some(test.test_infinity)
            || rule["wide"].as_bool() != Some(test.test_wide_range)
            || rule["range_min_bits"].as_str() != Some(bits(test.range_min).as_str())
            || rule["range_max_bits"].as_str() != Some(bits(test.range_max).as_str())
            || rule["legacy"].as_bool() != Some(test.legacy_shader)
            || rule["legacy_edge"].as_u64() != Some(u64::from(test.legacy_shader_lut_edge))
            || rule["shader_settings_json"].as_str()
                != Some(test.shader_settings.json().to_string().as_str())
            || r["vendor"].as_str() != Some(info.vendor.as_str())
            || r["renderer"].as_str() != Some(info.renderer.as_str())
            || r["version"].as_str() != Some(info.version.as_str())
            || r["shading_language_version"].as_str()
                != Some(info.shading_language_version.as_str())
            || r["egl_vendor"].as_str() != Some(info.egl_vendor.as_str())
            || r["egl_version"].as_str() != Some(info.egl_version.as_str())
            || r["max_texture_size"].as_i64() != Some(i64::from(info.max_texture_size))
        {
            continue;
        }
        let rows = rule["nan_mismatches"]
            .as_array()
            .ok_or_else(|| Exception::new("P7-6 has no component tuple array"))?;
        let expected: Option<Vec<usize>> = rows
            .iter()
            .map(|row| row["index"].as_u64().and_then(|i| usize::try_from(i).ok()))
            .collect();
        if expected.as_deref() != Some(actual.as_slice()) {
            continue;
        }
        // Exact observed tuples constrain class direction, signed zeros, NaN
        // sign/payload and Inf signs, including finite CPU values near specials.
        if !rows.iter().zip(&actual).all(|(row, &i)| {
            row["input"].as_str() == Some(bits(test.values.input_values[i]).as_str())
                && row["cpu"].as_str() == Some(bits(cpu[i]).as_str())
                && row["gpu"].as_str() == Some(bits(wheel[i]).as_str())
                && port[i].to_bits() == wheel[i].to_bits()
        }) {
            continue;
        }
        if applied.is_some() {
            return Err(Exception::new("P7-6 has ambiguous matching owner rules"));
        }
        applied = Some(
            rule["id"]
                .as_str()
                .ok_or_else(|| Exception::new("P7-6 has no owner waiver ID"))?
                .to_owned(),
        );
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocio_testkit::upstream::gpu::compare_image;

    fn set_actual_matrix_case(test: &mut GpuUnitTest, name: &str) {
        let case = ocio_testkit::gpu_cases::matrix_cases()
            .into_iter()
            .find(|case| case.name == name)
            .expect("actual upstream Matrix case");
        let mut matrix = ocio::MatrixTransform::new();
        matrix.set_direction(if case.inverse {
            ocio::TransformDirection::Inverse
        } else {
            ocio::TransformDirection::Forward
        });
        if let Some(values) = case.matrix {
            matrix.set_matrix(&values);
        }
        if let Some(values) = case.offset {
            matrix.set_offset(&values);
        }
        test.set_processor_transform(&ocio::Transform::from(matrix))
            .unwrap();
    }

    // Pure policy-input fixtures, not expected OCIO/GL output images. Special
    // tuples come from the live wheel evidence in W0003; other slots are neutral
    // zero-initialized control inputs as in GPUUnitTest's initial GL buffers.
    // The manifest records the explicit project-owner approval on 2026-10-27.
    fn observed_fixture() -> (GpuUnitTest, GlInfo, Vec<f32>, Vec<f32>, Vec<f32>) {
        let rules = ocio_testkit::gpu_waivers::gpu_renderer_waivers("MatrixOps/matrix").unwrap();
        let rule = rules
            .iter()
            .find(|r| r["id"].as_str() == Some("W0003"))
            .expect("signed W0003");
        let mut test = GpuUnitTest::new("MatrixOps", "matrix");
        test.comparison.error_threshold = 5e-7;
        test.legacy_shader = true;
        set_actual_matrix_case(&mut test, "matrix");
        test.prepare_values().unwrap();
        let mut cpu = vec![0.0; 262_144];
        let mut wheel = vec![0.0; 262_144];
        for row in rule["nan_mismatches"].as_array().unwrap() {
            let i = row["index"].as_u64().unwrap() as usize;
            cpu[i] = f32::from_bits(u32::from_str_radix(row["cpu"].as_str().unwrap(), 16).unwrap());
            wheel[i] =
                f32::from_bits(u32::from_str_radix(row["gpu"].as_str().unwrap(), 16).unwrap());
        }
        let r = &rule["renderer"];
        let info = GlInfo {
            vendor: r["vendor"].as_str().unwrap().into(),
            renderer: r["renderer"].as_str().unwrap().into(),
            version: r["version"].as_str().unwrap().into(),
            shading_language_version: r["shading_language_version"].as_str().unwrap().into(),
            egl_vendor: r["egl_vendor"].as_str().unwrap().into(),
            egl_version: r["egl_version"].as_str().unwrap().into(),
            max_texture_size: r["max_texture_size"].as_i64().unwrap() as i32,
        };
        (test, info, cpu, wheel.clone(), wheel)
    }

    fn decision(
        test: &GpuUnitTest,
        info: &GlInfo,
        cpu: &[f32],
        port: &[f32],
        wheel: &[f32],
    ) -> Option<String> {
        let c = compare_image(cpu, port, cpu.len(), test.comparison);
        matching(test, 0, info, [cpu, port, wheel], cpu.len(), &c).unwrap()
    }

    #[test]
    fn exact_observed_scope_is_the_only_accepted_scope() {
        let (mut test, mut info, mut cpu, mut port, mut wheel) = observed_fixture();
        assert_eq!(
            decision(&test, &info, &cpu, &port, &wheel).as_deref(),
            Some("W0003")
        );
        info.version.push_str(" changed");
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
        info.version.truncate(info.version.len() - " changed".len());
        test.values.input_values[100] = f32::from_bits(test.values.input_values[100].to_bits() ^ 1);
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
        test.values.input_values[100] = f32::from_bits(test.values.input_values[100].to_bits() ^ 1);
        cpu[16] = f32::from_bits(cpu[16].to_bits() ^ 0x8000_0000);
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
        cpu[16] = f32::from_bits(cpu[16].to_bits() ^ 0x8000_0000);
        // Negative zero differs only in bits: this isolates full GPU identity.
        port[100] = f32::from_bits(0x8000_0000);
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
        port[100] = 0.0;
        // An additional NaN and a missing NaN must each fail the complete set check.
        wheel[100] = f32::from_bits(0x7fc0_0000);
        port[100] = wheel[100];
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
        wheel[100] = 0.0;
        port[100] = 0.0;
        wheel[16] = cpu[16];
        port[16] = wheel[16];
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
    }

    #[test]
    fn numeric_infinity_and_settings_checks_remain_mandatory() {
        let (mut test, info, mut cpu, port, wheel) = observed_fixture();
        cpu[100] = 1.0;
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
        cpu[100] = f32::INFINITY;
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
        cpu[100] = 0.0;
        test.comparison.error_threshold = 1.0;
        assert!(decision(&test, &info, &cpu, &port, &wheel).is_none());
    }

    #[test]
    fn missing_or_changed_processor_cannot_use_the_same_test_name() {
        let (test, info, cpu, port, wheel) = observed_fixture();
        let mut unset = GpuUnitTest::new("MatrixOps", "matrix");
        unset.comparison = test.comparison;
        unset.legacy_shader = true;
        unset.prepare_values().unwrap();
        assert!(decision(&unset, &info, &cpu, &port, &wheel).is_none());
        // Preserve the same name/input/settings/observed tuple controls, but use
        // the distinct existing upstream scale processor to isolate the cache pin.
        set_actual_matrix_case(&mut unset, "scale");
        assert!(decision(&unset, &info, &cpu, &port, &wheel).is_none());
    }
}
