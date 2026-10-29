// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

// Pure policy inputs, not expected OCIO/GL images. The neutral zero values and
// non-finite markers exercise the approved decision, while native_reference
// tests independently compare actual full readbacks on the platform.
use super::*;

fn info() -> GlInfo {
    GlInfo {
        vendor: "Mesa".into(),
        renderer: "llvmpipe (LLVM 21.1.8, 256 bits)".into(),
        version: "4.5 (Compatibility Profile) Mesa 25.2.7".into(),
        shading_language_version: "4.50".into(),
        egl_vendor: "Mesa Project".into(),
        egl_version: "1.5".into(),
        max_texture_size: 16384,
    }
}

struct Inputs {
    input: Vec<f32>,
    cpu: Vec<f32>,
    port: Vec<f32>,
    wheel: Vec<f32>,
    finite_cpu: Vec<f32>,
    finite_port: Vec<f32>,
    finite_wheel: Vec<f32>,
}

impl Inputs {
    fn new() -> Self {
        let mut value = Self {
            input: vec![0.0; ENTRIES],
            cpu: vec![0.0; ENTRIES],
            port: vec![0.0; ENTRIES],
            wheel: vec![0.0; ENTRIES],
            finite_cpu: vec![0.0; ENTRIES],
            finite_port: vec![0.0; ENTRIES],
            finite_wheel: vec![0.0; ENTRIES],
        };
        value.input[4] = f32::INFINITY;
        value.port[0] = f32::NAN;
        value.wheel[0] = f32::NAN;
        value
    }
    fn check(
        &self,
        i: &GlInfo,
        native: bool,
        companion: bool,
        language: GpuLanguage,
    ) -> Result<Vec<usize>> {
        let rule = approval("Infrastructure/nonfinite policy")
            .unwrap()
            .unwrap();
        assess(
            &rule,
            Environment {
                info: i,
                native: if native { Some(i) } else { None },
                language,
            },
            Frame {
                inputs: &self.input,
                cpu: &self.cpu,
                port: &self.port,
                wheel: &self.wheel,
                entries: ENTRIES,
                settings: ComparisonSettings::default(),
                companion: companion.then_some(Companion {
                    cpu: &self.finite_cpu,
                    port: &self.finite_port,
                    wheel: &self.finite_wheel,
                }),
            },
        )
    }
}

#[test]
fn positive_control_includes_zero_weight_nonfinite_neighbor() {
    assert_eq!(
        Inputs::new()
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .unwrap(),
        [0]
    );
}

#[test]
fn finite_footprint_errors_are_not_excepted() {
    let mut value = Inputs::new();
    value.cpu[ENTRIES - 1] = 1.0;
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .unwrap_err()
            .to_string()
            .contains("finite-footprint")
    );
}

#[test]
fn missing_failed_and_incomplete_finite_companions_are_rejected() {
    let mut value = Inputs::new();
    assert!(
        value
            .check(&info(), true, false, GpuLanguage::Glsl12)
            .is_err()
    );
    value.finite_cpu[ENTRIES - 1] = 1.0;
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .is_err()
    );
    value.finite_cpu.pop();
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .is_err()
    );
}

#[test]
fn changed_original_and_companion_gpu_bits_are_rejected() {
    let mut value = Inputs::new();
    value.port[0] = f32::from_bits(value.wheel[0].to_bits() ^ 1);
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .is_err()
    );
    value.port[0] = value.wheel[0];
    value.finite_port[ENTRIES - 1] = 1.0;
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .is_err()
    );
}

#[test]
fn wrong_renderer_language_and_missing_native_proof_are_rejected() {
    let value = Inputs::new();
    let mut i = info();
    i.version.push_str(" changed");
    assert!(value.check(&i, true, true, GpuLanguage::Glsl12).is_err());
    assert!(
        value
            .check(&info(), false, true, GpuLanguage::Glsl12)
            .is_err()
    );
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl13)
            .is_err()
    );
}

#[test]
fn finite_only_inputs_and_nan_failures_in_finite_regions_are_rejected() {
    let mut value = Inputs::new();
    value.input.fill(0.0);
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .is_err()
    );
    value.input[4] = f32::INFINITY;
    value.cpu[ENTRIES - 1] = f32::NAN;
    assert!(
        value
            .check(&info(), true, true, GpuLanguage::Glsl12)
            .is_err()
    );
}
