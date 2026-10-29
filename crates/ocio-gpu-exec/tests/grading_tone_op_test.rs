// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/GradingToneOp_test.cpp` @ v2.5.2.
//! Original input values; actual uncached wheel dynamic CPU stream and shaders.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::{Arc, OnceLock};

use ocio::{
    DynamicPropertyType, GradingRgbmsw, GradingStyle, GradingTone, GradingToneTransform,
    GroupTransform, Transform, TransformDirection,
};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite4_reference.rs"]
mod reference;

#[derive(Clone, Copy, Debug)]
enum Preset {
    LogMidtones,
    LogHighlights,
    VideoShadows,
    VideoWhites,
    LogBlacks,
    LogScontrast(f64),
    LinMidtones,
    Retests { pair: bool },
}

#[derive(Clone, Copy, Debug)]
struct Case {
    name: &'static str,
    preset: Preset,
    inverse: bool,
    dynamic: bool,
    threshold: f32,
}

const INPUT_DIGEST: &str = "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94";
const TONE_BINDING: &str = "DYNAMIC_PROPERTY_GRADING_TONE";

// Literal inputs from GTTest1..GTTest7, GradingToneOp_test.cpp:10-377.
const MID_LOG: [f64; 6] = [0.3, 1.0, 1.8, 1.2, 0.47, 0.6];
const HIGH_LOG: [f64; 6] = [0.3, 1.0, 1.8, 1.4, -0.1, 0.9];
const SHADOW_VIDEO: [f64; 6] = [0.3, 1.0, 1.79, 0.6, 0.8, -0.1];
const WHITE_VIDEO: [f64; 6] = [0.3, 1.0, 1.9, 0.6, -0.2, 1.4];
const BLACK_LOG: [f64; 6] = [0.3, 1.0, 1.9, 0.6, 0.8, 0.9];
const MID_LIN: [f64; 6] = [0.3, 1.4, 1.8, 1.0, 1.0, 8.0];

fn style(case: Case) -> GradingStyle {
    match case.preset {
        Preset::VideoShadows | Preset::VideoWhites => GradingStyle::Video,
        Preset::LinMidtones => GradingStyle::Lin,
        _ => GradingStyle::Log,
    }
}

fn style_name(style: GradingStyle) -> &'static str {
    match style {
        GradingStyle::Log => "GRADING_LOG",
        GradingStyle::Lin => "GRADING_LIN",
        GradingStyle::Video => "GRADING_VIDEO",
    }
}

/// Source values for one initial preset or one cumulative retest input frame.
/// This changes inputs only; expected pixels and uniforms come from the wheel.
fn tone_value(case: Case, frame: usize) -> GradingTone {
    let mut tone = GradingTone::new(style(case));
    match case.preset {
        Preset::LogMidtones => tone.midtones = GradingRgbmsw::from_array(&MID_LOG),
        Preset::LogHighlights => tone.highlights = GradingRgbmsw::from_array(&HIGH_LOG),
        Preset::VideoShadows => tone.shadows = GradingRgbmsw::from_array(&SHADOW_VIDEO),
        Preset::VideoWhites => tone.whites = GradingRgbmsw::from_array(&WHITE_VIDEO),
        Preset::LogBlacks => tone.blacks = GradingRgbmsw::from_array(&BLACK_LOG),
        Preset::LogScontrast(value) => tone.scontrast = value,
        Preset::LinMidtones => tone.midtones = GradingRgbmsw::from_array(&MID_LIN),
        Preset::Retests { .. } => {
            if (1..=3).contains(&frame) {
                tone.midtones = GradingRgbmsw::from_array(&MID_LOG);
            }
            if (2..=3).contains(&frame) {
                tone.highlights = GradingRgbmsw::from_array(&HIGH_LOG);
                tone.whites = GradingRgbmsw::from_array(&WHITE_VIDEO);
            }
            if frame == 3 {
                tone.blacks = GradingRgbmsw::from_array(&BLACK_LOG);
            }
        }
    }
    tone
}

fn rgbmsw_spec(values: &[f64; 6]) -> Value {
    json!({"object":{"class":"GradingRGBMSW","args":values}})
}

/// Independent wheel value construction: only explicit source fields are set;
/// the wheel itself supplies its style defaults, never port getters/defaults.
fn tone_spec(case: Case, frame: usize) -> Value {
    let mut attrs = Vec::new();
    let field = |name: &str, value: &[f64; 6]| json!([name, rgbmsw_spec(value)]);
    match case.preset {
        Preset::LogMidtones => attrs.push(field("midtones", &MID_LOG)),
        Preset::LogHighlights => attrs.push(field("highlights", &HIGH_LOG)),
        Preset::VideoShadows => attrs.push(field("shadows", &SHADOW_VIDEO)),
        Preset::VideoWhites => attrs.push(field("whites", &WHITE_VIDEO)),
        Preset::LogBlacks => attrs.push(field("blacks", &BLACK_LOG)),
        Preset::LogScontrast(value) => attrs.push(json!(["scontrast", value])),
        Preset::LinMidtones => attrs.push(field("midtones", &MID_LIN)),
        Preset::Retests { .. } => {
            if (1..=3).contains(&frame) {
                attrs.push(field("midtones", &MID_LOG));
            }
            if (2..=3).contains(&frame) {
                attrs.push(field("highlights", &HIGH_LOG));
                attrs.push(field("whites", &WHITE_VIDEO));
            }
            if frame == 3 {
                attrs.push(field("blacks", &BLACK_LOG));
            }
        }
    }
    json!({"object":{"class":"GradingTone","args":[{"enum":style_name(style(case))}],"attrs":attrs}})
}

fn single_spec(case: Case, frame: usize) -> Value {
    let mut calls = vec![
        json!(["setDirection",{"enum":if case.inverse {"TRANSFORM_DIR_INVERSE"}else{"TRANSFORM_DIR_FORWARD"}}]),
    ];
    if case.dynamic {
        calls.push(json!(["makeDynamic"]));
    }
    calls.push(json!(["setValue", tone_spec(case, frame)]));
    json!({"class":"GradingToneTransform","args":{"style":{"enum":style_name(style(case))}},"calls":calls})
}

fn transform_spec(case: Case, frame: usize) -> Value {
    let first = single_spec(case, frame);
    if !matches!(case.preset, Preset::Retests { pair: true }) {
        return first;
    }
    // Exact source mismatch: second TRANSFORM style is LOG (GTTest6::style),
    // but its value starts with LIN defaults, GradingToneOp_test.cpp:506-512.
    let second = json!({"class":"GradingToneTransform","args":{"style":{"enum":"GRADING_LOG"}},
        "calls":[["setValue",{"object":{"class":"GradingTone","args":[{"enum":"GRADING_LIN"}],
            "attrs":[["scontrast",1.8],["midtones",rgbmsw_spec(&MID_LIN)]]}}]]});
    json!({"class":"GroupTransform","children":[first,second]})
}

/// Port of each helper's Create/setDirection/makeDynamic/setValue, and the
/// two-transforms setup (GradingToneOp_test.cpp:498-517 @ v2.5.2).
fn transform(case: Case) -> Transform {
    let mut first = GradingToneTransform::new(style(case));
    first.set_direction(if case.inverse {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    });
    if case.dynamic {
        first.make_dynamic();
    }
    first.set_value(&tone_value(case, 0)).unwrap();
    if !matches!(case.preset, Preset::Retests { pair: true }) {
        return first.into();
    }
    let mut second = GradingToneTransform::new(GradingStyle::Log);
    let mut second_value = GradingTone::new(GradingStyle::Lin);
    second_value.scontrast = 1.8;
    second_value.midtones = GradingRgbmsw::from_array(&MID_LIN);
    second.set_value(&second_value).unwrap();
    let mut group = GroupTransform::new();
    group.append_transform(first.into());
    group.append_transform(second.into());
    group.into()
}

/// Port of GTRetest and MyGTRetest in both source tests: obtain the extracted
/// GPU property only on retest1, retain it, and update both decoupled properties.
fn add_retests(test: &mut GpuUnitTest, case: Case) {
    if !matches!(case.preset, Preset::Retests { .. }) {
        assert_eq!(test.num_retests(), 0);
        return;
    }
    let cpu = test
        .processor()
        .unwrap()
        .dynamic_property(DynamicPropertyType::GradingTone)
        .unwrap()
        .as_grading_tone()
        .unwrap();
    let gpu = Arc::new(OnceLock::new());
    for frame in 1..=4 {
        let cpu = cpu.clone();
        let gpu = gpu.clone();
        test.add_retest(move |desc, _| {
            let gpu = gpu.get_or_init(|| {
                let value = desc
                    .dynamic_property_by_type(DynamicPropertyType::GradingTone)
                    .unwrap()
                    .as_grading_tone()
                    .unwrap();
                assert!(
                    !Arc::ptr_eq(&cpu, &value),
                    "extraction decouples tone properties"
                );
                value
            });
            let mut values = cpu.get_value();
            match frame {
                1 => values.midtones = GradingRgbmsw::from_array(&MID_LOG),
                2 => {
                    values.highlights = GradingRgbmsw::from_array(&HIGH_LOG);
                    values.whites = GradingRgbmsw::from_array(&WHITE_VIDEO);
                }
                3 => values.blacks = GradingRgbmsw::from_array(&BLACK_LOG),
                4 => values = GradingTone::new(GradingStyle::Log),
                _ => unreachable!("exactly four source callbacks"),
            }
            cpu.set_value(&values).unwrap();
            gpu.set_value(&values).unwrap();
        });
    }
    assert_eq!(test.num_retests(), 4);
}

fn prepared_test(case: Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("GradingTone", case.name);
    test.comparison.error_threshold = case.threshold;
    test.comparison.expected_minimal_value = 1.0;
    test.comparison.relative = true;
    test.test_wide_range = true;
    test.test_infinity = false;
    test.test_nan = false;
    test.set_processor_transform(&transform(case)).unwrap();
    add_retests(&mut test, case);
    test.prepare_values().unwrap();
    assert_eq!(input_digest(&test.values.input_values), INPUT_DIGEST);
    test
}

fn references() -> &'static [Vec<ReferenceRun>] {
    static REFERENCES: OnceLock<Vec<Vec<ReferenceRun>>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        assert_eq!(CASES.len(), 34);
        assert_eq!(CASES.iter().filter(|case| case.dynamic).count(), 18);
        assert_eq!(
            CASES
                .iter()
                .filter(|case| matches!(case.preset, Preset::Retests { .. }))
                .count(),
            2
        );
        let plans: Vec<_> = CASES
            .iter()
            .map(|&case| {
                let test = prepared_test(case);
                let count = test.num_retests() + 1;
                let transforms: Vec<_> = (0..count)
                    .map(|frame| transform_spec(case, frame))
                    .collect();
                let updates: Vec<_> = (0..count)
                    .map(|frame| {
                        if frame == 0 {
                            vec![]
                        } else {
                            vec![json!({"set":TONE_BINDING,"value":tone_spec(case,frame)})]
                        }
                    })
                    .collect();
                let bindings = if case.dynamic {
                    vec![TONE_BINDING]
                } else {
                    vec![]
                };
                reference::Plan {
                    test,
                    transforms,
                    bindings,
                    updates,
                }
            })
            .collect();
        let result = reference::batch(&plans);
        assert_eq!(result.len(), 34);
        assert_eq!(result.iter().map(Vec::len).sum::<usize>(), 42);
        result
    })
}

fn run_case(index: usize, name: &str) {
    let case = CASES[index];
    assert_eq!(case.name, name);
    let mut test = prepared_test(case);
    let references = &references()[index];
    assert_eq!(references.len(), test.num_retests() + 1);
    assert!(test.run(references).unwrap());
}

// BEGIN SOURCE CASE TABLE / WRAPPERS
const CASES: &[Case] = &[
    // GradingToneOp_test.cpp:39-42 @ v2.5.2.
    Case {
        name: "style_log_midtones_fwd",
        preset: Preset::LogMidtones,
        inverse: false,
        dynamic: false,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:44-47 @ v2.5.2.
    Case {
        name: "style_log_midtones_fwd_dynamic",
        preset: Preset::LogMidtones,
        inverse: false,
        dynamic: true,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:49-52 @ v2.5.2.
    Case {
        name: "style_log_midtones_rev",
        preset: Preset::LogMidtones,
        inverse: true,
        dynamic: false,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:54-57 @ v2.5.2.
    Case {
        name: "style_log_midtones_rev_dynamic",
        preset: Preset::LogMidtones,
        inverse: true,
        dynamic: true,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:88-91 @ v2.5.2.
    Case {
        name: "style_log_highlights_fwd",
        preset: Preset::LogHighlights,
        inverse: false,
        dynamic: false,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:93-96 @ v2.5.2.
    Case {
        name: "style_log_highlights_fwd_dynamic",
        preset: Preset::LogHighlights,
        inverse: false,
        dynamic: true,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:98-101 @ v2.5.2.
    Case {
        name: "style_log_highlights_rev",
        preset: Preset::LogHighlights,
        inverse: true,
        dynamic: false,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:103-106 @ v2.5.2.
    Case {
        name: "style_log_highlights_rev_dynamic",
        preset: Preset::LogHighlights,
        inverse: true,
        dynamic: true,
        threshold: 2e-5,
    },
    // GradingToneOp_test.cpp:137-140 @ v2.5.2.
    Case {
        name: "style_video_shadows_fwd",
        preset: Preset::VideoShadows,
        inverse: false,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:142-145 @ v2.5.2.
    Case {
        name: "style_video_shadows_fwd_dynamic",
        preset: Preset::VideoShadows,
        inverse: false,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:147-150 @ v2.5.2.
    Case {
        name: "style_video_shadows_rev",
        preset: Preset::VideoShadows,
        inverse: true,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:152-155 @ v2.5.2.
    Case {
        name: "style_video_shadows_rev_dynamic",
        preset: Preset::VideoShadows,
        inverse: true,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:186-189 @ v2.5.2.
    Case {
        name: "style_video_white_detail_fwd",
        preset: Preset::VideoWhites,
        inverse: false,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:191-194 @ v2.5.2.
    Case {
        name: "style_video_white_detail_fwd_dynamic",
        preset: Preset::VideoWhites,
        inverse: false,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:196-199 @ v2.5.2.
    Case {
        name: "style_video_white_detail_rev",
        preset: Preset::VideoWhites,
        inverse: true,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:201-204 @ v2.5.2.
    Case {
        name: "style_video_white_detail_rev_dynamic",
        preset: Preset::VideoWhites,
        inverse: true,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:235-238 @ v2.5.2.
    Case {
        name: "style_log_black_detail_fwd",
        preset: Preset::LogBlacks,
        inverse: false,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:240-243 @ v2.5.2.
    Case {
        name: "style_log_black_detail_fwd_dynamic",
        preset: Preset::LogBlacks,
        inverse: false,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:245-248 @ v2.5.2.
    Case {
        name: "style_log_black_detail_rev",
        preset: Preset::LogBlacks,
        inverse: true,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:250-253 @ v2.5.2.
    Case {
        name: "style_log_black_detail_rev_dynamic",
        preset: Preset::LogBlacks,
        inverse: true,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:285-289 @ v2.5.2.
    Case {
        name: "style_log_scontrast_fwd",
        preset: Preset::LogScontrast(1.8),
        inverse: false,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:291-295 @ v2.5.2.
    Case {
        name: "style_log_scontrast_fwd_dynamic",
        preset: Preset::LogScontrast(1.8),
        inverse: false,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:297-301 @ v2.5.2.
    Case {
        name: "style_log_scontrast2_fwd",
        preset: Preset::LogScontrast(0.3),
        inverse: false,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:303-307 @ v2.5.2.
    Case {
        name: "style_log_scontrast2_fwd_dynamic",
        preset: Preset::LogScontrast(0.3),
        inverse: false,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:309-313 @ v2.5.2.
    Case {
        name: "style_log_scontrast_rev",
        preset: Preset::LogScontrast(1.8),
        inverse: true,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:315-319 @ v2.5.2.
    Case {
        name: "style_log_scontrast_rev_dynamic",
        preset: Preset::LogScontrast(1.8),
        inverse: true,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:321-325 @ v2.5.2.
    Case {
        name: "style_log_scontrast2_rev",
        preset: Preset::LogScontrast(0.3),
        inverse: true,
        dynamic: false,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:327-331 @ v2.5.2.
    Case {
        name: "style_log_scontrast2_rev_dynamic",
        preset: Preset::LogScontrast(0.3),
        inverse: true,
        dynamic: true,
        threshold: 3e-5,
    },
    // GradingToneOp_test.cpp:362-365 @ v2.5.2.
    Case {
        name: "style_lin_midtones_fwd",
        preset: Preset::LinMidtones,
        inverse: false,
        dynamic: false,
        threshold: 5e-5,
    },
    // GradingToneOp_test.cpp:367-370 @ v2.5.2.
    Case {
        name: "style_lin_midtones_fwd_dynamic",
        preset: Preset::LinMidtones,
        inverse: false,
        dynamic: true,
        threshold: 5e-5,
    },
    // GradingToneOp_test.cpp:372-375 @ v2.5.2.
    Case {
        name: "style_lin_midtones_rev",
        preset: Preset::LinMidtones,
        inverse: true,
        dynamic: false,
        threshold: 5e-5,
    },
    // GradingToneOp_test.cpp:377-380 @ v2.5.2.
    Case {
        name: "style_lin_midtones_rev_dynamic",
        preset: Preset::LinMidtones,
        inverse: true,
        dynamic: true,
        threshold: 5e-5,
    },
    // GradingToneOp_test.cpp:428-496 @ v2.5.2.
    Case {
        name: "style_log_dynamic_retests",
        preset: Preset::Retests { pair: false },
        inverse: false,
        dynamic: true,
        threshold: 5e-5,
    },
    // GradingToneOp_test.cpp:498-578 @ v2.5.2.
    Case {
        name: "two_transforms_retests",
        preset: Preset::Retests { pair: true },
        inverse: false,
        dynamic: true,
        threshold: 5e-5,
    },
];

/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_midtones_fwd)` @ v2.5.2.
#[test]
fn style_log_midtones_fwd() {
    run_case(0, "style_log_midtones_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_midtones_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_midtones_fwd_dynamic() {
    run_case(1, "style_log_midtones_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_midtones_rev)` @ v2.5.2.
#[test]
fn style_log_midtones_rev() {
    run_case(2, "style_log_midtones_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_midtones_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_midtones_rev_dynamic() {
    run_case(3, "style_log_midtones_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_highlights_fwd)` @ v2.5.2.
#[test]
fn style_log_highlights_fwd() {
    run_case(4, "style_log_highlights_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_highlights_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_highlights_fwd_dynamic() {
    run_case(5, "style_log_highlights_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_highlights_rev)` @ v2.5.2.
#[test]
fn style_log_highlights_rev() {
    run_case(6, "style_log_highlights_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_highlights_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_highlights_rev_dynamic() {
    run_case(7, "style_log_highlights_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_shadows_fwd)` @ v2.5.2.
#[test]
fn style_video_shadows_fwd() {
    run_case(8, "style_video_shadows_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_shadows_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_video_shadows_fwd_dynamic() {
    run_case(9, "style_video_shadows_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_shadows_rev)` @ v2.5.2.
#[test]
fn style_video_shadows_rev() {
    run_case(10, "style_video_shadows_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_shadows_rev_dynamic)` @ v2.5.2.
#[test]
fn style_video_shadows_rev_dynamic() {
    run_case(11, "style_video_shadows_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_white_detail_fwd)` @ v2.5.2.
#[test]
fn style_video_white_detail_fwd() {
    run_case(12, "style_video_white_detail_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_white_detail_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_video_white_detail_fwd_dynamic() {
    run_case(13, "style_video_white_detail_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_white_detail_rev)` @ v2.5.2.
#[test]
fn style_video_white_detail_rev() {
    run_case(14, "style_video_white_detail_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_video_white_detail_rev_dynamic)` @ v2.5.2.
#[test]
fn style_video_white_detail_rev_dynamic() {
    run_case(15, "style_video_white_detail_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_black_detail_fwd)` @ v2.5.2.
#[test]
fn style_log_black_detail_fwd() {
    run_case(16, "style_log_black_detail_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_black_detail_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_black_detail_fwd_dynamic() {
    run_case(17, "style_log_black_detail_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_black_detail_rev)` @ v2.5.2.
#[test]
fn style_log_black_detail_rev() {
    run_case(18, "style_log_black_detail_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_black_detail_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_black_detail_rev_dynamic() {
    run_case(19, "style_log_black_detail_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast_fwd)` @ v2.5.2.
#[test]
fn style_log_scontrast_fwd() {
    run_case(20, "style_log_scontrast_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_scontrast_fwd_dynamic() {
    run_case(21, "style_log_scontrast_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast2_fwd)` @ v2.5.2.
#[test]
fn style_log_scontrast2_fwd() {
    run_case(22, "style_log_scontrast2_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast2_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_scontrast2_fwd_dynamic() {
    run_case(23, "style_log_scontrast2_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast_rev)` @ v2.5.2.
#[test]
fn style_log_scontrast_rev() {
    run_case(24, "style_log_scontrast_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_scontrast_rev_dynamic() {
    run_case(25, "style_log_scontrast_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast2_rev)` @ v2.5.2.
#[test]
fn style_log_scontrast2_rev() {
    run_case(26, "style_log_scontrast2_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_scontrast2_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_scontrast2_rev_dynamic() {
    run_case(27, "style_log_scontrast2_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_lin_midtones_fwd)` @ v2.5.2.
#[test]
fn style_lin_midtones_fwd() {
    run_case(28, "style_lin_midtones_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_lin_midtones_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_lin_midtones_fwd_dynamic() {
    run_case(29, "style_lin_midtones_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_lin_midtones_rev)` @ v2.5.2.
#[test]
fn style_lin_midtones_rev() {
    run_case(30, "style_lin_midtones_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_lin_midtones_rev_dynamic)` @ v2.5.2.
#[test]
fn style_lin_midtones_rev_dynamic() {
    run_case(31, "style_lin_midtones_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, style_log_dynamic_retests)` @ v2.5.2.
#[test]
fn style_log_dynamic_retests() {
    run_case(32, "style_log_dynamic_retests");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingTone, two_transforms_retests)` @ v2.5.2.
#[test]
fn two_transforms_retests() {
    run_case(33, "two_transforms_retests");
}
