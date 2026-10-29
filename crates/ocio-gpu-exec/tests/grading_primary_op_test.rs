// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/GradingPrimaryOp_test.cpp` @ v2.5.2.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::{Arc, OnceLock};

use ocio::{
    DynamicPropertyType, GradingPrimary, GradingPrimaryTransform, GradingRgbm, GradingStyle,
    GroupTransform, Transform, TransformDirection,
};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite4_reference.rs"]
mod reference;

const PROPERTY: &str = "DYNAMIC_PROPERTY_GRADING_PRIMARY";
// Suite1 gamma INPUT_DIGEST_NEITHER; five upstream recipes independently checked in Suite2.
const INPUT_DIGEST: &str = "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94";

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    style: GradingStyle,
    inverse: bool,
    dynamic: bool,
    retests: bool,
    two: bool,
    threshold: f32,
}

const CASES: &[Case] = &[
    Case {
        name: "style_log_fwd",
        style: GradingStyle::Log,
        inverse: false,
        dynamic: false,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_log_fwd_dynamic",
        style: GradingStyle::Log,
        inverse: false,
        dynamic: true,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_log_rev",
        style: GradingStyle::Log,
        inverse: true,
        dynamic: false,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_log_rev_dynamic",
        style: GradingStyle::Log,
        inverse: true,
        dynamic: true,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_lin_fwd",
        style: GradingStyle::Lin,
        inverse: false,
        dynamic: false,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_lin_fwd_dynamic",
        style: GradingStyle::Lin,
        inverse: false,
        dynamic: true,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_lin_rev",
        style: GradingStyle::Lin,
        inverse: true,
        dynamic: false,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_lin_rev_dynamic",
        style: GradingStyle::Lin,
        inverse: true,
        dynamic: true,
        retests: false,
        two: false,
        threshold: 2e-5,
    },
    Case {
        name: "style_video_fwd",
        style: GradingStyle::Video,
        inverse: false,
        dynamic: false,
        retests: false,
        two: false,
        threshold: 3e-5,
    },
    Case {
        name: "style_video_fwd_dynamic",
        style: GradingStyle::Video,
        inverse: false,
        dynamic: true,
        retests: false,
        two: false,
        threshold: 3e-5,
    },
    Case {
        name: "style_video_rev",
        style: GradingStyle::Video,
        inverse: true,
        dynamic: false,
        retests: false,
        two: false,
        threshold: 3e-5,
    },
    Case {
        name: "style_video_rev_dynamic",
        style: GradingStyle::Video,
        inverse: true,
        dynamic: true,
        retests: false,
        two: false,
        threshold: 3e-5,
    },
    Case {
        name: "style_log_dynamic_retests",
        style: GradingStyle::Log,
        inverse: false,
        dynamic: true,
        retests: true,
        two: false,
        threshold: 5e-5,
    },
    Case {
        name: "two_transforms_retests",
        style: GradingStyle::Log,
        inverse: false,
        dynamic: true,
        retests: true,
        two: true,
        threshold: 1e-4,
    },
];

#[derive(Clone)]
enum Parameter {
    Scalar(f64),
    Rgbm([f64; 4]),
}

/// Explicit upstream inputs only. Missing fields are left to the independent native
/// value constructor; no defaults are read from a port object to build an oracle spec.
#[derive(Clone)]
struct Input {
    style: GradingStyle,
    attrs: Vec<(&'static str, Parameter)>,
}

fn style_name(style: GradingStyle) -> &'static str {
    match style {
        GradingStyle::Log => "GRADING_LOG",
        GradingStyle::Lin => "GRADING_LIN",
        GradingStyle::Video => "GRADING_VIDEO",
    }
}
fn scalar(value: f64) -> Value {
    json!({"f64":value.to_bits()})
}

impl Input {
    fn set(&mut self, name: &'static str, value: Parameter) {
        if let Some((_, old)) = self.attrs.iter_mut().find(|(key, _)| *key == name) {
            *old = value;
        } else {
            self.attrs.push((name, value));
        }
    }
    fn add(&mut self, name: &str, delta: f64) {
        let (_, Parameter::Scalar(value)) =
            self.attrs.iter_mut().find(|(key, _)| *key == name).unwrap()
        else {
            panic!("scalar source field");
        };
        *value += delta;
    }
    fn spec(&self) -> Value {
        let attrs: Vec<Value> = self
            .attrs
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    Parameter::Scalar(value) => scalar(*value),
                    Parameter::Rgbm(values) => {
                        json!({"object":{"class":"GradingRGBM","args":values.map(scalar)}})
                    }
                };
                json!([name, value])
            })
            .collect();
        json!({"object":{"class":"GradingPrimary","args":[{"enum":style_name(self.style)}],"attrs":attrs}})
    }
    fn port(&self) -> GradingPrimary {
        let mut value = GradingPrimary::new(self.style);
        for (name, param) in &self.attrs {
            match param {
                Parameter::Rgbm(rgbm) => {
                    let rgbm = GradingRgbm::from_array(rgbm);
                    match *name {
                        "brightness" => value.brightness = rgbm,
                        "contrast" => value.contrast = rgbm,
                        "gamma" => value.gamma = rgbm,
                        "offset" => value.offset = rgbm,
                        "exposure" => value.exposure = rgbm,
                        "lift" => value.lift = rgbm,
                        "gain" => value.gain = rgbm,
                        _ => panic!("unknown source RGBM field"),
                    }
                }
                Parameter::Scalar(number) => match *name {
                    "saturation" => value.saturation = *number,
                    "pivot" => value.pivot = *number,
                    "pivotBlack" => value.pivot_black = *number,
                    "pivotWhite" => value.pivot_white = *number,
                    "clampBlack" => value.clamp_black = *number,
                    "clampWhite" => value.clamp_white = *number,
                    _ => panic!("unknown source scalar field"),
                },
            }
        }
        value
    }
}

/// Port of GPTest1/2/3 and setup assignments (GradingPrimaryOp_test.cpp:10-198).
/// Retest log setup deliberately omits clamps and black/white pivots until callbacks.
fn initial(style: GradingStyle, complete: bool) -> Input {
    use Parameter::{Rgbm as R, Scalar as S};
    let attrs = match style {
        GradingStyle::Log => vec![
            ("brightness", R([-10., 45., -5., 50.])),
            ("contrast", R([0.9, 1.4, 0.7, 0.75])),
            ("gamma", R([1.1, 0.7, 1.05, 1.15])),
            ("saturation", S(1.21)),
            ("pivot", S(-0.3)),
        ],
        GradingStyle::Lin => vec![
            ("exposure", R([0.5, -0.2, 0.4, -0.25])),
            ("contrast", R([0.9, 1.4, 0.7, 0.75])),
            ("offset", R([-0.03, 0.02, 0.1, -0.1])),
            ("pivot", S(0.5)),
            ("saturation", S(1.33)),
            ("clampBlack", S(-0.40)),
            ("clampWhite", S(1.05)),
        ],
        GradingStyle::Video => vec![
            ("lift", R([0.05, -0.04, 0.02, 0.05])),
            ("gamma", R([0.9, 1.4, 0.7, 0.75])),
            ("gain", R([1.2, 1.1, 1.25, 0.8])),
            ("offset", R([-0.03, 0.02, 0.1, -0.1])),
            ("saturation", S(1.2)),
            ("clampBlack", S(-0.15)),
            ("clampWhite", S(1.50)),
            ("pivotBlack", S(0.05)),
            ("pivotWhite", S(0.9)),
        ],
    };
    let mut input = Input { style, attrs };
    if style == GradingStyle::Log && complete {
        input.set("pivotBlack", S(0.05));
        input.set("pivotWhite", S(0.9));
        input.set("clampBlack", S(-0.05));
        input.set("clampWhite", S(1.50));
    }
    input
}

#[derive(Clone, Copy)]
enum Update {
    Clamp,
    Pivot,
    Gamma,
    Saturation,
    White,
    Identity,
}
const UPDATES: [Update; 6] = [
    Update::Clamp,
    Update::Pivot,
    Update::Gamma,
    Update::Saturation,
    Update::White,
    Update::Identity,
];

impl Update {
    /// Explicit input-state counterpart of the upstream callback, not a port value dump.
    fn input(self, state: &mut Input) {
        use Parameter::Scalar as S;
        match self {
            Self::Clamp => {
                state.set("clampBlack", S(-0.05));
                state.set("clampWhite", S(1.50));
            }
            Self::Pivot => {
                state.set("clampBlack", S(-100.));
                state.set("clampWhite", S(100.));
                state.set("pivotBlack", S(0.05));
                state.set("pivotWhite", S(0.9));
            }
            Self::Gamma => {
                let (_, Parameter::Rgbm(gamma)) = state
                    .attrs
                    .iter_mut()
                    .find(|(key, _)| *key == "gamma")
                    .unwrap()
                else {
                    panic!("gamma source input")
                };
                gamma[0] += 0.1;
                gamma[3] += 0.1;
            }
            Self::Saturation => state.add("saturation", 0.1),
            Self::White => state.add("clampWhite", 1.),
            Self::Identity => {
                *state = Input {
                    style: GradingStyle::Log,
                    attrs: Vec::new(),
                }
            }
        }
    }
    /// Port of MyGPRetest's actual read/modify/set callback (lines268-316,382-430).
    fn port(self, mut value: GradingPrimary) -> GradingPrimary {
        match self {
            Self::Clamp => {
                value.clamp_black = -0.05;
                value.clamp_white = 1.50;
            }
            Self::Pivot => {
                value.clamp_black = -100.;
                value.clamp_white = 100.;
                value.pivot_black = 0.05;
                value.pivot_white = 0.9;
            }
            Self::Gamma => {
                value.gamma.red += 0.1;
                value.gamma.master += 0.1;
            }
            Self::Saturation => value.saturation += 0.1,
            Self::White => value.clamp_white += 1.,
            Self::Identity => value = GradingPrimary::new(GradingStyle::Log),
        }
        value
    }
}

fn transform_spec(case: Case, input: &Input) -> Value {
    let mut calls = Vec::new();
    if !case.retests {
        calls.push(json!(["setDirection",{"enum":if case.inverse{"TRANSFORM_DIR_INVERSE"}else{"TRANSFORM_DIR_FORWARD"}}]));
    }
    if case.dynamic {
        calls.push(json!(["makeDynamic"]));
    }
    calls.push(json!(["setValue", input.spec()]));
    let spec = json!({"class":"GradingPrimaryTransform","args":[{"enum":style_name(case.style)}],"calls":calls});
    if case.two {
        json!({"class":"GroupTransform","children":[spec,
            {"class":"GradingPrimaryTransform","args":[{"enum":"GRADING_LIN"}],"calls":[["setValue",initial(GradingStyle::Lin,true).spec()]]}]})
    } else {
        spec
    }
}

fn transform(case: Case, input: &Input) -> Transform {
    let direction = if case.inverse {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    };
    let mut port = GradingPrimaryTransform::new(case.style);
    if !case.retests {
        port.set_direction(direction);
    }
    if case.dynamic {
        port.make_dynamic();
    }
    port.set_value(&input.port()).unwrap();
    if case.two {
        let fixed = initial(GradingStyle::Lin, true);
        let mut fixed_port = GradingPrimaryTransform::new(GradingStyle::Lin);
        fixed_port.set_value(&fixed.port()).unwrap();
        let mut group = GroupTransform::new();
        group.append_transform(port.into());
        group.append_transform(fixed_port.into());
        group.into()
    } else {
        port.into()
    }
}

fn setup(case: Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("GradingPrimary", case.name);
    test.comparison.error_threshold = case.threshold;
    test.comparison.expected_minimal_value = 1.;
    test.comparison.relative = true;
    test.test_wide_range = true;
    test.test_infinity = false;
    test.test_nan = false;
    let transform = transform(case, &initial(case.style, !case.retests));
    test.set_processor_transform(&transform).unwrap();
    if case.retests {
        let cpu = test
            .processor()
            .unwrap()
            .dynamic_property(DynamicPropertyType::GradingPrimary)
            .unwrap()
            .as_grading_primary()
            .unwrap();
        let gpu = Arc::new(OnceLock::new());
        for update in UPDATES {
            let cpu = cpu.clone();
            let gpu = gpu.clone();
            test.add_retest(move |desc, _| {
                let gpu = gpu.get_or_init(|| {
                    let property = desc
                        .dynamic_property_by_type(DynamicPropertyType::GradingPrimary)
                        .unwrap()
                        .as_grading_primary()
                        .unwrap();
                    assert!(
                        !Arc::ptr_eq(&cpu, &property),
                        "upstream decouples extracted GPU properties"
                    );
                    property
                });
                let value = update.port(cpu.get_value());
                cpu.set_value(&value).unwrap();
                gpu.set_value(&value).unwrap();
            });
        }
    }
    test.prepare_values().unwrap();
    assert_eq!(input_digest(&test.values.input_values), INPUT_DIGEST);
    test
}

fn plan(case: Case) -> reference::Plan {
    let test = setup(case);
    let mut state = initial(case.style, !case.retests);
    let mut transforms = vec![transform_spec(case, &state)];
    let mut steps = vec![Vec::new()];
    if case.retests {
        for update in UPDATES {
            update.input(&mut state);
            transforms.push(transform_spec(case, &state));
            steps.push(vec![json!({"set":PROPERTY,"value":state.spec()})]);
        }
    }
    let bindings = if case.dynamic {
        vec![PROPERTY]
    } else {
        Vec::new()
    };
    reference::Plan {
        test,
        transforms,
        bindings,
        updates: steps,
    }
}

fn references() -> &'static [Vec<ReferenceRun>] {
    static REFERENCES: OnceLock<Vec<Vec<ReferenceRun>>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let plans: Vec<_> = CASES.iter().copied().map(plan).collect();
        assert_eq!(plans.len(), 14);
        assert_eq!(
            plans
                .iter()
                .map(|plan| plan.transforms.len())
                .sum::<usize>(),
            26
        );
        let results = reference::batch(&plans);
        assert_eq!(results.len(), CASES.len());
        results
    })
}

fn run_case(index: usize, name: &str) {
    let case = CASES[index];
    assert_eq!(case.name, name);
    let mut test = setup(case);
    let references = &references()[index];
    assert_eq!(references.len(), if case.retests { 7 } else { 1 });
    assert!(test.run(references).unwrap());
}

/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_log_fwd)` @ v2.5.2.
#[test]
fn style_log_fwd() {
    run_case(0, "style_log_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_log_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_fwd_dynamic() {
    run_case(1, "style_log_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_log_rev)` @ v2.5.2.
#[test]
fn style_log_rev() {
    run_case(2, "style_log_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_log_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_rev_dynamic() {
    run_case(3, "style_log_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_lin_fwd)` @ v2.5.2.
#[test]
fn style_lin_fwd() {
    run_case(4, "style_lin_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_lin_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_lin_fwd_dynamic() {
    run_case(5, "style_lin_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_lin_rev)` @ v2.5.2.
#[test]
fn style_lin_rev() {
    run_case(6, "style_lin_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_lin_rev_dynamic)` @ v2.5.2.
#[test]
fn style_lin_rev_dynamic() {
    run_case(7, "style_lin_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_video_fwd)` @ v2.5.2.
#[test]
fn style_video_fwd() {
    run_case(8, "style_video_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_video_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_video_fwd_dynamic() {
    run_case(9, "style_video_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_video_rev)` @ v2.5.2.
#[test]
fn style_video_rev() {
    run_case(10, "style_video_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_video_rev_dynamic)` @ v2.5.2.
#[test]
fn style_video_rev_dynamic() {
    run_case(11, "style_video_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, style_log_dynamic_retests)` @ v2.5.2.
#[test]
fn style_log_dynamic_retests() {
    run_case(12, "style_log_dynamic_retests");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingPrimary, two_transforms_retests)` @ v2.5.2.
#[test]
fn two_transforms_retests() {
    run_case(13, "two_transforms_retests");
}
