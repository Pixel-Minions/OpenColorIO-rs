// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::*;
use ocio_testkit::upstream::gpu::{ComparisonSettings, compare_image};

struct Fixture {
    pin: Approval,
    info: GlInfo,
    inputs: Vec<f32>,
    cpu: Vec<f32>,
    gpu: Vec<f32>,
    settings: ComparisonSettings,
    comparison: ImageComparison,
}

impl Fixture {
    fn new() -> Self {
        let mut pin = approval("FixedFunction/style_aces2_nan_bug")
            .unwrap()
            .unwrap();
        // Policy-test inputs are the debugger's reported real RGB bits, repeated to the
        // harness dimensions. No expected value comes from a port renderer. The actual
        // original capture hashes remain mandatory in all live GPU integration tests.
        let input = f32::from_bits(0x3f66_40e6);
        let inputs = [input, input, input, 1.0]
            .into_iter()
            .cycle()
            .take(ENTRIES)
            .collect::<Vec<_>>();
        let cpu = [0x4473_9272, 0x440e_0021, 0x45ba_139c, 0x3f80_0000]
            .map(f32::from_bits)
            .to_vec();
        let gpu = [0x4389_b37a, 0x444b_9830, 0x4470_eaee, 0x3f80_0000]
            .map(f32::from_bits)
            .into_iter()
            .cycle()
            .take(ENTRIES)
            .collect::<Vec<_>>();
        pin.scope["input_sha256"] = input_digest(&inputs).into();
        pin.scope["cpu_sha256"] = input_digest(&cpu).into();
        pin.scope["gpu_sha256"] = input_digest(&gpu).into();
        let settings = ComparisonSettings {
            error_threshold: f32::from_bits(bits(&pin.scope, "threshold_bits")),
            expected_minimal_value: 1.0,
            relative: true,
            print_min_max: false,
        };
        let comparison = compare_image(&cpu, &gpu, 4, settings);
        assert_eq!(comparison.max_diff.to_bits(), 0x3f56_91a0);
        let info = GlInfo {
            vendor: "Mesa".into(),
            renderer: "llvmpipe (LLVM 21.1.8, 256 bits)".into(),
            version: "4.5 (Compatibility Profile) Mesa 25.2.7".into(),
            shading_language_version: "4.50".into(),
            egl_vendor: "Mesa Project".into(),
            egl_version: "1.5".into(),
            max_texture_size: 16384,
        };
        Self {
            pin,
            info,
            inputs,
            cpu,
            gpu,
            settings,
            comparison,
        }
    }

    fn check(&self) -> Result<Value> {
        assess(
            &self.pin,
            Environment {
                info: &self.info,
                native: Some(&self.info),
                language: GpuLanguage::Glsl12,
            },
            Frame {
                inputs: &self.inputs,
                cpu: &self.cpu,
                port: &self.gpu,
                wheel: &self.gpu,
                entries: 4,
                settings: self.settings,
                companion: None,
            },
            0,
            &self.comparison,
        )
    }
}

#[test]
fn original_failure_is_required_and_a_new_pass_is_rejected() {
    let mut f = Fixture::new();
    assert!(f.check().is_ok());
    f.comparison.max_diff = 0.0;
    f.comparison.index_diff = None;
    assert!(f.check().unwrap_err().to_string().contains("now passes"));
}

#[test]
fn changed_failure_component_settings_renderer_or_rgb_pins_are_rejected() {
    let mut f = Fixture::new();
    f.comparison.max_diff = f32::from_bits(f.comparison.max_diff.to_bits() + 1);
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.comparison.index_diff = Some(1);
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.comparison.index_nan = Some(2);
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.settings.error_threshold = f32::from_bits(f.settings.error_threshold.to_bits() + 1);
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.settings.relative = false;
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.info.version.push_str(" changed");
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.info.renderer.push_str(" changed");
    assert!(f.check().is_err());
    for key in ["gpu_rgb_bits", "cpu_rgb_bits"] {
        let mut f = Fixture::new();
        f.pin.scope[key][0] = "00000000".into();
        assert!(f.check().unwrap_err().to_string().contains("RGB bits"));
    }
}

#[test]
fn any_historical_input_cpu_or_gpu_bit_change_is_rejected() {
    let mut f = Fixture::new();
    let last = f.inputs.len() - 1;
    f.inputs[last] = f32::from_bits(f.inputs[last].to_bits() ^ 1);
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.cpu[0] = f32::from_bits(f.cpu[0].to_bits() ^ 1);
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    let last = f.gpu.len() - 1;
    f.gpu[last] = f32::from_bits(f.gpu[last].to_bits() ^ 1);
    assert!(f.check().is_err());
}

#[test]
fn missing_native_proof_language_run_and_whole_gpu_identity_fail_closed() {
    let f = Fixture::new();
    let frame = || Frame {
        inputs: &f.inputs,
        cpu: &f.cpu,
        port: &f.gpu,
        wheel: &f.gpu,
        entries: 4,
        settings: f.settings,
        companion: None,
    };
    assert!(
        assess(
            &f.pin,
            Environment {
                info: &f.info,
                native: None,
                language: GpuLanguage::Glsl12
            },
            frame(),
            0,
            &f.comparison
        )
        .is_err()
    );
    assert!(
        assess(
            &f.pin,
            Environment {
                info: &f.info,
                native: Some(&f.info),
                language: GpuLanguage::Glsl40
            },
            frame(),
            0,
            &f.comparison
        )
        .is_err()
    );
    assert!(
        assess(
            &f.pin,
            Environment {
                info: &f.info,
                native: Some(&f.info),
                language: GpuLanguage::Glsl12
            },
            frame(),
            1,
            &f.comparison
        )
        .is_err()
    );
    let mut changed = f.gpu.clone();
    changed[ENTRIES - 1] = f32::from_bits(changed[ENTRIES - 1].to_bits() ^ 1);
    let mut changed_frame = frame();
    changed_frame.port = &changed;
    assert!(
        assess(
            &f.pin,
            Environment {
                info: &f.info,
                native: Some(&f.info),
                language: GpuLanguage::Glsl12
            },
            changed_frame,
            0,
            &f.comparison
        )
        .unwrap_err()
        .to_string()
        .contains("whole-image")
    );
}
