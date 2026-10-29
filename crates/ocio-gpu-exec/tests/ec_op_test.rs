// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/ECOp_test.cpp` @ v2.5.2.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::{Arc, OnceLock};

use ocio::{
    DynamicPropertyType, ExposureContrastStyle, ExposureContrastTransform, GroupTransform,
    Transform, TransformDirection,
};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite4_reference.rs"]
mod reference;

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    style: ExposureContrastStyle,
    inverse: bool,
    infinity: bool,
    threshold: f32,
    mode: Mode,
}

#[derive(Clone, Copy)]
enum Mode {
    Static,
    ThreeDynamic,
    Two { first_dynamic: bool },
}

const CASES: &[Case] = &[
    Case {
        name: "style_linear_fwd",
        style: ExposureContrastStyle::Linear,
        inverse: false,
        infinity: false,
        threshold: 2e-5,
        mode: Mode::Static,
    },
    Case {
        name: "style_linear_rev",
        style: ExposureContrastStyle::Linear,
        inverse: true,
        infinity: true,
        threshold: 2e-5,
        mode: Mode::Static,
    },
    Case {
        name: "style_video_fwd",
        style: ExposureContrastStyle::Video,
        inverse: false,
        infinity: false,
        threshold: 2e-5,
        mode: Mode::Static,
    },
    Case {
        name: "style_video_rev",
        style: ExposureContrastStyle::Video,
        inverse: true,
        infinity: true,
        threshold: 2e-5,
        mode: Mode::Static,
    },
    Case {
        name: "style_log_fwd",
        style: ExposureContrastStyle::Logarithmic,
        inverse: false,
        infinity: true,
        threshold: 1e-6,
        mode: Mode::Static,
    },
    Case {
        name: "style_log_rev",
        style: ExposureContrastStyle::Logarithmic,
        inverse: true,
        infinity: true,
        threshold: 1e-6,
        mode: Mode::Static,
    },
    Case {
        name: "style_linear_dynamic_parameter",
        style: ExposureContrastStyle::Linear,
        inverse: false,
        infinity: false,
        threshold: 5e-5,
        mode: Mode::ThreeDynamic,
    },
    Case {
        name: "dp_several_one_dynamic",
        style: ExposureContrastStyle::Logarithmic,
        inverse: false,
        infinity: false,
        threshold: 5e-5,
        mode: Mode::Two {
            first_dynamic: false,
        },
    },
    Case {
        name: "dp_several_both_dynamic",
        style: ExposureContrastStyle::Logarithmic,
        inverse: false,
        infinity: false,
        threshold: 5e-5,
        mode: Mode::Two {
            first_dynamic: true,
        },
    },
];

#[derive(Clone, Copy)]
enum Update {
    Increment(DynamicPropertyType, f64),
    Set(DynamicPropertyType, f64),
}

impl Update {
    fn kind(self) -> DynamicPropertyType {
        match self {
            Self::Increment(kind, _) | Self::Set(kind, _) => kind,
        }
    }
    fn value(self, previous: f64) -> f64 {
        match self {
            Self::Increment(_, delta) => previous + delta,
            Self::Set(_, value) => value,
        }
    }
}

/// Port of the setup calls and `Prepare2ECDynamic` (ECOp_test.cpp:12-135,
/// 199-217,259-283 @ v2.5.2). Group children keep their default pivot/direction.
fn transform(case: Case) -> (Transform, Value) {
    let style = match case.style {
        ExposureContrastStyle::Linear => "EXPOSURE_CONTRAST_LINEAR",
        ExposureContrastStyle::Video => "EXPOSURE_CONTRAST_VIDEO",
        ExposureContrastStyle::Logarithmic => "EXPOSURE_CONTRAST_LOGARITHMIC",
    };
    let single = |group_child: bool, contrast_dynamic: bool, exposure_dynamic: bool| {
        let mut ec = ExposureContrastTransform::new();
        ec.set_style(case.style);
        let mut calls = vec![json!(["setStyle", {"enum": style}])];
        if group_child {
            ec.set_exposure(0.8);
            ec.set_contrast(0.5);
            ec.set_gamma(1.5);
            calls.extend([
                json!(["setExposure", 0.8]),
                json!(["setContrast", 0.5]),
                json!(["setGamma", 1.5]),
            ]);
        } else {
            let direction = if case.inverse {
                TransformDirection::Inverse
            } else {
                TransformDirection::Forward
            };
            ec.set_direction(direction);
            let contrast = if case.inverse { 0.7 } else { 0.8 };
            ec.set_exposure(1.1);
            ec.set_contrast(contrast);
            ec.set_gamma(0.9);
            ec.set_pivot(0.22);
            calls.extend([
                json!(["setDirection", {"enum": if case.inverse {
                "TRANSFORM_DIR_INVERSE" } else { "TRANSFORM_DIR_FORWARD" }}]),
                json!(["setExposure", 1.1]),
                json!(["setContrast", contrast]),
                json!(["setGamma", 0.9]),
                json!(["setPivot", 0.22]),
            ]);
        }
        if exposure_dynamic {
            ec.make_exposure_dynamic();
            calls.push(json!(["makeExposureDynamic"]));
        }
        if contrast_dynamic {
            ec.make_contrast_dynamic();
            calls.push(json!(["makeContrastDynamic"]));
        }
        if matches!(case.mode, Mode::ThreeDynamic) {
            ec.make_gamma_dynamic();
            calls.push(json!(["makeGammaDynamic"]));
        }
        (
            Transform::from(ec),
            json!({"class":"ExposureContrastTransform", "calls":calls}),
        )
    };
    match case.mode {
        Mode::Two { first_dynamic } => {
            let (first, first_spec) = single(true, first_dynamic, false);
            let (second, second_spec) = single(true, false, true);
            let mut group = GroupTransform::new();
            group.append_transform(first);
            group.append_transform(second);
            (
                Transform::from(group),
                json!({"class":"GroupTransform", "calls":[
                ["appendTransform", {"transform":first_spec}],
                ["appendTransform", {"transform":second_spec}]]}),
            )
        }
        Mode::ThreeDynamic => single(false, true, true),
        Mode::Static => single(false, false, false),
    }
}

/// Port of `MyECRetest::retest1/2/3` (ECOp_test.cpp:219-238,291-321 @ v2.5.2).
/// These values describe input operations, never expected output pixels.
fn updates(case: Case) -> Vec<Vec<Update>> {
    use DynamicPropertyType::{Contrast, Exposure, Gamma};
    match case.mode {
        Mode::Static => vec![],
        Mode::ThreeDynamic => vec![
            vec![Update::Increment(Exposure, 0.1)],
            vec![Update::Increment(Contrast, 0.1)],
            vec![Update::Increment(Gamma, 0.1)],
        ],
        Mode::Two { first_dynamic } => [(0.5, 1.1), (0.7, 2.1)]
            .into_iter()
            .map(|(contrast, exposure)| {
                let mut out = Vec::new();
                if first_dynamic {
                    out.push(Update::Set(Contrast, contrast));
                }
                out.push(Update::Set(Exposure, exposure));
                out
            })
            .collect(),
    }
}

fn initial_dynamic_values(case: Case) -> Vec<(DynamicPropertyType, f64)> {
    use DynamicPropertyType::{Contrast, Exposure, Gamma};
    match case.mode {
        Mode::Static => vec![],
        Mode::ThreeDynamic => vec![(Exposure, 1.1), (Contrast, 0.8), (Gamma, 0.9)],
        Mode::Two {
            first_dynamic: false,
        } => vec![(Exposure, 0.8)],
        Mode::Two {
            first_dynamic: true,
        } => vec![(Contrast, 0.5), (Exposure, 0.8)],
    }
}

fn property_name(kind: DynamicPropertyType) -> &'static str {
    match kind {
        DynamicPropertyType::Exposure => "DYNAMIC_PROPERTY_EXPOSURE",
        DynamicPropertyType::Contrast => "DYNAMIC_PROPERTY_CONTRAST",
        DynamicPropertyType::Gamma => "DYNAMIC_PROPERTY_GAMMA",
        other => panic!("unexpected EC property: {other:?}"),
    }
}

fn input_test(case: Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("ExposureContrast", case.name);
    test.comparison.error_threshold = case.threshold;
    test.comparison.expected_minimal_value = 1.0;
    test.comparison.relative = true;
    test.test_wide_range = true;
    test.test_infinity = case.infinity;
    test.prepare_values().unwrap();
    assert_eq!(
        input_digest(&test.values.input_values),
        if case.infinity {
            "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af"
        } else {
            "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642"
        }
    );
    test
}

/// Port of `ECRetest` and its retained CPU/GPU properties (ECOp_test.cpp:141-195
/// @ v2.5.2). GPU properties are obtained only after the real description exists.
fn add_retests(test: &mut GpuUnitTest, case: Case) {
    let kinds = initial_dynamic_values(case);
    let processor = test.processor().unwrap();
    let cpu: Vec<_> = kinds
        .iter()
        .map(|&(kind, initial)| {
            let value = processor
                .dynamic_property(kind)
                .unwrap()
                .as_double()
                .unwrap();
            assert_eq!(value.get_value().to_bits(), initial.to_bits());
            (kind, value)
        })
        .collect();
    let cpu = Arc::new(cpu);
    let gpu = Arc::new(OnceLock::new());
    for run in updates(case) {
        let cpu = cpu.clone();
        let gpu = gpu.clone();
        test.add_retest(move |desc, _| {
            let gpu: &Vec<_> = gpu.get_or_init(|| {
                cpu.iter()
                    .map(|(kind, cpu)| {
                        let value = desc
                            .dynamic_property_by_type(*kind)
                            .unwrap()
                            .as_double()
                            .unwrap();
                        assert!(
                            !Arc::ptr_eq(cpu, &value),
                            "upstream extraction decouples properties"
                        );
                        (*kind, value)
                    })
                    .collect()
            });
            for update in &run {
                let kind = update.kind();
                let cpu = &cpu.iter().find(|(found, _)| *found == kind).unwrap().1;
                let gpu = &gpu.iter().find(|(found, _)| *found == kind).unwrap().1;
                cpu.set_value(update.value(cpu.get_value()));
                gpu.set_value(cpu.get_value());
            }
        });
    }
    assert_eq!(test.num_retests(), updates(case).len());
}

/// Update only fields explicitly dynamic in the original transform spec.
/// Both the CPU stream and the independent shader extraction receive these same
/// input values; output pixels, shader bytes and uniform bytes come from the wheel.
fn frame_transform(mut spec: Value, state: &[(DynamicPropertyType, f64)]) -> Value {
    if spec["class"] == "GroupTransform" {
        for call in spec["calls"].as_array_mut().unwrap() {
            let child = call[1]["transform"].take();
            call[1]["transform"] = frame_transform(child, state);
        }
    } else {
        let calls = spec["calls"].as_array_mut().unwrap();
        for &(kind, value) in state {
            let (dynamic, setter) = match kind {
                DynamicPropertyType::Exposure => ("makeExposureDynamic", "setExposure"),
                DynamicPropertyType::Contrast => ("makeContrastDynamic", "setContrast"),
                DynamicPropertyType::Gamma => ("makeGammaDynamic", "setGamma"),
                _ => unreachable!("only EC properties from upstream"),
            };
            if calls.iter().any(|call| call[0] == dynamic) {
                calls.push(json!([setter, {"f64":value.to_bits()}]));
            }
        }
    }
    spec
}

fn reference_plan(case: Case) -> reference::Plan {
    let mut test = input_test(case);
    let (port, transform) = transform(case);
    test.set_processor_transform(&port).unwrap();
    add_retests(&mut test, case);
    let mut state = initial_dynamic_values(case);
    let bindings = state.iter().map(|&(kind, _)| property_name(kind)).collect();
    let mut transforms = Vec::new();
    let mut updates_by_frame = Vec::new();
    let mut frames = vec![Vec::new()];
    frames.extend(updates(case));
    for run in frames {
        let mut steps = Vec::new();
        for update in run {
            let kind = update.kind();
            let value = &mut state
                .iter_mut()
                .find(|(found, _)| *found == kind)
                .unwrap()
                .1;
            *value = update.value(*value);
            steps.push(json!({"set":property_name(kind), "value":{"f64":value.to_bits()}}));
        }
        updates_by_frame.push(steps);
        transforms.push(frame_transform(transform.clone(), &state));
    }
    reference::Plan {
        test,
        transforms,
        bindings,
        updates: updates_by_frame,
    }
}

fn references() -> &'static [Vec<ReferenceRun>] {
    static REFERENCES: OnceLock<Vec<Vec<ReferenceRun>>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let plans: Vec<_> = CASES.iter().copied().map(reference_plan).collect();
        assert_eq!(plans.len(), 9);
        assert_eq!(
            plans
                .iter()
                .map(|plan| plan.transforms.len())
                .sum::<usize>(),
            16
        );
        reference::batch(&plans)
    })
}
fn run_case(index: usize, name: &str) {
    let case = CASES[index];
    assert_eq!(case.name, name);
    let (transform, _) = transform(case);
    let mut test = input_test(case);
    test.set_processor_transform(&transform).unwrap();
    add_retests(&mut test, case);
    let references = &references()[index];
    assert_eq!(references.len(), test.num_retests() + 1);
    assert!(test.run(references).unwrap());
}

/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, style_linear_fwd)` @ v2.5.2.
#[test]
fn style_linear_fwd() {
    run_case(0, "style_linear_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, style_linear_rev)` @ v2.5.2.
#[test]
fn style_linear_rev() {
    run_case(1, "style_linear_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, style_video_fwd)` @ v2.5.2.
#[test]
fn style_video_fwd() {
    run_case(2, "style_video_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, style_video_rev)` @ v2.5.2.
#[test]
fn style_video_rev() {
    run_case(3, "style_video_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, style_log_fwd)` @ v2.5.2.
#[test]
fn style_log_fwd() {
    run_case(4, "style_log_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, style_log_rev)` @ v2.5.2.
#[test]
fn style_log_rev() {
    run_case(5, "style_log_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, style_linear_dynamic_parameter)` @ v2.5.2.
#[test]
fn style_linear_dynamic_parameter() {
    run_case(6, "style_linear_dynamic_parameter");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, dp_several_one_dynamic)` @ v2.5.2.
#[test]
fn dp_several_one_dynamic() {
    run_case(7, "dp_several_one_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(ExposureContrast, dp_several_both_dynamic)` @ v2.5.2.
#[test]
fn dp_several_both_dynamic() {
    run_case(8, "dp_several_both_dynamic");
}
