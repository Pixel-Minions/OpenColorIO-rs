// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/GradingRGBCurveOp_test.cpp` @ v2.5.2.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::{Arc, OnceLock};

use ocio::{
    DynamicPropertyType, GradingBSplineCurve, GradingControlPoint, GradingRgbCurve,
    GradingRgbCurveTransform, GradingStyle, GroupTransform, RgbCurveType, Transform,
    TransformDirection,
};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite4_reference.rs"]
mod reference;

const PROPERTY: &str = "DYNAMIC_PROPERTY_GRADING_RGBCURVE";
// Independent upstream harness recipe hashes checked with Suite1/Suite2.
const INPUT_BOTH: &str = "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af";
const INPUT_NAN: &str = "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642";

#[derive(Clone, Copy)]
enum Shape {
    Log,
    Lin,
    SCurve,
}
#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    shape: Shape,
    inverse: bool,
    dynamic: bool,
    retests: bool,
    two: bool,
    infinity: bool,
    threshold: f32,
}
const CASES: &[Case] = &[
    Case {
        name: "style_log_fwd",
        shape: Shape::Log,
        inverse: false,
        dynamic: false,
        retests: false,
        two: false,
        infinity: true,
        threshold: 2e-5,
    },
    Case {
        name: "style_log_fwd_dynamic",
        shape: Shape::Log,
        inverse: false,
        dynamic: true,
        retests: false,
        two: false,
        infinity: true,
        threshold: 2e-5,
    },
    Case {
        name: "style_log_rev",
        shape: Shape::Log,
        inverse: true,
        dynamic: false,
        retests: false,
        two: false,
        infinity: true,
        threshold: 2e-5,
    },
    Case {
        name: "style_log_rev_dynamic",
        shape: Shape::Log,
        inverse: true,
        dynamic: true,
        retests: false,
        two: false,
        infinity: true,
        threshold: 2e-5,
    },
    Case {
        name: "style_lin_fwd",
        shape: Shape::Lin,
        inverse: false,
        dynamic: false,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "style_lin_fwd_dynamic",
        shape: Shape::Lin,
        inverse: false,
        dynamic: true,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "style_lin_rev",
        shape: Shape::Lin,
        inverse: true,
        dynamic: false,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "style_lin_rev_dynamic",
        shape: Shape::Lin,
        inverse: true,
        dynamic: true,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "scurve_fwd",
        shape: Shape::SCurve,
        inverse: false,
        dynamic: false,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "scurve_fwd_dynamic",
        shape: Shape::SCurve,
        inverse: false,
        dynamic: true,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "scurve_rev",
        shape: Shape::SCurve,
        inverse: true,
        dynamic: false,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "scurve_rev_dynamic",
        shape: Shape::SCurve,
        inverse: true,
        dynamic: true,
        retests: false,
        two: false,
        infinity: false,
        threshold: 1.5e-4,
    },
    Case {
        name: "style_log_dynamic_retests",
        shape: Shape::Log,
        inverse: false,
        dynamic: true,
        retests: true,
        two: false,
        infinity: true,
        threshold: 5e-5,
    },
    Case {
        name: "two_transforms_retests",
        shape: Shape::Log,
        inverse: false,
        dynamic: true,
        retests: true,
        two: true,
        infinity: false,
        threshold: 5e-5,
    },
];

#[derive(Clone)]
struct CurveInput {
    points: Vec<(f32, f32)>,
    slopes: Option<Vec<f32>>,
}
#[derive(Clone)]
enum ValueInput {
    Curves([CurveInput; 4]),
    Identity,
}

fn curve(points: &[(f32, f32)]) -> CurveInput {
    CurveInput {
        points: points.to_vec(),
        slopes: None,
    }
}
fn f32_spec(value: f32) -> Value {
    json!({"f64":f64::from(value).to_bits()})
}

impl CurveInput {
    fn port(&self) -> GradingBSplineCurve {
        let points: Vec<_> = self
            .points
            .iter()
            .map(|&(x, y)| GradingControlPoint::new(x, y))
            .collect();
        let mut value = GradingBSplineCurve::with_points(&points);
        if let Some(slopes) = &self.slopes {
            for (index, &slope) in slopes.iter().enumerate() {
                value.set_slope(index, slope).unwrap();
            }
        }
        value
    }
    fn spec(&self) -> Value {
        let points: Vec<Value> = self
            .points
            .iter()
            .flat_map(|&(x, y)| [f32_spec(x), f32_spec(y)])
            .collect();
        let mut calls = Vec::new();
        if let Some(slopes) = &self.slopes {
            calls.push(json!([
                "setSlopes",
                slopes.iter().copied().map(f32_spec).collect::<Vec<_>>()
            ]));
        }
        json!({"object":{"class":"GradingBSplineCurve","args":[points],"calls":calls}})
    }
}
impl ValueInput {
    fn port(&self) -> GradingRgbCurve {
        match self {
            Self::Identity => GradingRgbCurve::new(GradingStyle::Log),
            Self::Curves(curves) => {
                let [r, g, b, m] = curves.each_ref().map(CurveInput::port);
                GradingRgbCurve::with_curves(&r, &g, &b, &m)
            }
        }
    }
    fn spec(&self) -> Value {
        match self {
            Self::Identity => {
                json!({"object":{"class":"GradingRGBCurve","args":[{"enum":"GRADING_LOG"}]}})
            }
            Self::Curves([r, g, b, m]) => json!({"object":{"class":"GradingRGBCurve","args":{
                "red":r.spec(),"green":g.spec(),"blue":b.spec(),"master":m.spec()}}}),
        }
    }
}

/// Port of exact authored float control points/slopes in GradingRGBCurveOp_test.cpp:10-162.
/// These source inputs build both sides; the oracle never reads port curve/default outputs.
#[allow(clippy::excessive_precision)] // Preserve the upstream decimal f32 source literals.
fn initial(shape: Shape) -> ValueInput {
    let r = curve(&[
        (0., 0.),
        (0.785, 0.231),
        (0.809, 0.631),
        (0.948, 0.704),
        (1., 1.),
    ]);
    match shape {
        Shape::Log => ValueInput::Curves([
            r,
            curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)]),
            curve(&[(-6., -8.), (-2., -5.), (2., 4.), (5., 6.)]),
            curve(&[(-0.1, 0.1), (1.1, 1.3)]),
        ]),
        Shape::Lin => ValueInput::Curves([
            r,
            curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 0.8)]),
            curve(&[(-6., -4.), (-2., -1.), (2., 2.), (5., 4.)]),
            curve(&[(-0.1, 0.1), (1.1, 0.9)]),
        ]),
        Shape::SCurve => {
            let s = CurveInput {
                points: vec![
                    (-5.26017743, -4.),
                    (-3.75502745, -3.57868829),
                    (-2.24987747, -1.82131329),
                    (-0.74472749, 0.68124124),
                    (1.06145248, 2.87457742),
                    (2.86763245, 3.83406206),
                    (4.67381243, 4.),
                ],
                slopes: Some(vec![
                    0., 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.,
                ]),
            };
            ValueInput::Curves([s.clone(), s.clone(), s, curve(&[(-5., 0.), (5., 1.)])])
        }
    }
}

/// The fixed linear second transform in the source's two_transforms_retests.
fn fixed_input() -> ValueInput {
    ValueInput::Curves([
        curve(&[(0., 0.), (0.2, 0.2), (0.5, 0.7), (1., 1.)]),
        curve(&[(0., 0.5), (0.3, 0.7), (0.5, 1.1), (1., 1.5)]),
        curve(&[
            (0., -0.5),
            (0.2, -0.4),
            (0.3, 0.1),
            (0.5, 0.4),
            (0.7, 0.9),
            (1., 1.1),
        ]),
        curve(&[(-1., 0.), (0.2, 0.2), (0.8, 0.8), (2., 1.)]),
    ])
}

#[derive(Clone, Copy)]
enum Update {
    Red,
    Green,
    Master,
    Identity,
}
const UPDATES: [Update; 4] = [Update::Red, Update::Green, Update::Master, Update::Identity];

impl Update {
    /// Independently authored source input state; adds/subtracts are f32 like C++'s 0.1f.
    fn input(self, value: &mut ValueInput) {
        if matches!(self, Self::Identity) {
            *value = ValueInput::Identity;
            return;
        }
        let ValueInput::Curves(curves) = value else {
            panic!("source curve state")
        };
        match self {
            Self::Red => curves[0].points[1].1 += 0.1_f32,
            Self::Green => curves[1].points[1].1 -= 0.1_f32,
            Self::Master => {
                curves[3].points.resize(3, (0., 0.));
                curves[3].points[1] = (0.2, 0.5);
                curves[3].points[2] = (1.1, 1.3);
            }
            Self::Identity => unreachable!(),
        }
    }
    /// Port of the actual createEditableCopy/read/modify/set callbacks (lines248-281,354-387).
    fn port(self, mut value: GradingRgbCurve) -> GradingRgbCurve {
        match self {
            Self::Red => {
                value
                    .curve_mut(RgbCurveType::Red)
                    .unwrap()
                    .control_point_mut(1)
                    .unwrap()
                    .y += 0.1_f32
            }
            Self::Green => {
                value
                    .curve_mut(RgbCurveType::Green)
                    .unwrap()
                    .control_point_mut(1)
                    .unwrap()
                    .y -= 0.1_f32
            }
            Self::Master => {
                let master = value.curve_mut(RgbCurveType::Master).unwrap();
                master.set_num_control_points(3);
                let point = master.control_point_mut(1).unwrap();
                point.x = 0.2;
                point.y = 0.5;
                let point = master.control_point_mut(2).unwrap();
                point.x = 1.1;
                point.y = 1.3;
            }
            Self::Identity => value = GradingRgbCurve::new(GradingStyle::Log),
        }
        value
    }
}

fn style(case: Case) -> GradingStyle {
    if matches!(case.shape, Shape::Lin) {
        GradingStyle::Lin
    } else {
        GradingStyle::Log
    }
}
fn style_name(case: Case) -> &'static str {
    if matches!(case.shape, Shape::Lin) {
        "GRADING_LIN"
    } else {
        "GRADING_LOG"
    }
}

fn transform_spec(case: Case, value: &ValueInput) -> Value {
    let mut calls = Vec::new();
    if case.retests {
        calls.push(json!(["makeDynamic"]));
    }
    calls.push(json!(["setValue", value.spec()]));
    if !case.retests {
        calls.push(json!(["setDirection",{"enum":if case.inverse{"TRANSFORM_DIR_INVERSE"}else{"TRANSFORM_DIR_FORWARD"}}]));
        if case.dynamic {
            calls.push(json!(["makeDynamic"]));
        }
    }
    let first = json!({"class":"GradingRGBCurveTransform","args":{"style":{"enum":style_name(case)}},"calls":calls});
    if case.two {
        json!({"class":"GroupTransform","children":[first,
        {"class":"GradingRGBCurveTransform","args":{"style":{"enum":"GRADING_LIN"}},"calls":[["setValue",fixed_input().spec()]]}]})
    } else {
        first
    }
}

fn transform(case: Case, value: &ValueInput) -> Transform {
    let mut port = GradingRgbCurveTransform::new(style(case));
    if case.retests {
        port.make_dynamic();
    }
    port.set_value(&value.port()).unwrap();
    if !case.retests {
        port.set_direction(if case.inverse {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        });
        if case.dynamic {
            port.make_dynamic();
        }
    }
    if case.two {
        let mut fixed = GradingRgbCurveTransform::new(GradingStyle::Lin);
        fixed.set_value(&fixed_input().port()).unwrap();
        let mut group = GroupTransform::new();
        group.append_transform(port.into());
        group.append_transform(fixed.into());
        group.into()
    } else {
        port.into()
    }
}

fn setup(case: Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("GradingRGBCurve", case.name);
    test.comparison.error_threshold = case.threshold;
    test.comparison.expected_minimal_value = 1.;
    test.comparison.relative = true;
    test.test_wide_range = true;
    test.test_nan = true;
    test.test_infinity = case.infinity;
    test.set_processor_transform(&transform(case, &initial(case.shape)))
        .unwrap();
    if case.retests {
        let cpu = test
            .processor()
            .unwrap()
            .dynamic_property(DynamicPropertyType::GradingRgbCurve)
            .unwrap()
            .as_grading_rgb_curve()
            .unwrap();
        let gpu = Arc::new(OnceLock::new());
        for update in UPDATES {
            let cpu = cpu.clone();
            let gpu = gpu.clone();
            test.add_retest(move |desc, _| {
                let gpu = gpu.get_or_init(|| {
                    let property = desc
                        .dynamic_property_by_type(DynamicPropertyType::GradingRgbCurve)
                        .unwrap()
                        .as_grading_rgb_curve()
                        .unwrap();
                    assert!(
                        !Arc::ptr_eq(&cpu, &property),
                        "upstream decouples actual extracted GPU property"
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
    assert_eq!(
        input_digest(&test.values.input_values),
        if case.infinity { INPUT_BOTH } else { INPUT_NAN }
    );
    test
}

fn plan(case: Case) -> reference::Plan {
    let test = setup(case);
    let mut state = initial(case.shape);
    let mut transforms = vec![transform_spec(case, &state)];
    let mut updates = vec![Vec::new()];
    if case.retests {
        for update in UPDATES {
            update.input(&mut state);
            transforms.push(transform_spec(case, &state));
            updates.push(vec![json!({"set":PROPERTY,"value":state.spec()})]);
        }
    }
    reference::Plan {
        test,
        transforms,
        bindings: if case.dynamic {
            vec![PROPERTY]
        } else {
            Vec::new()
        },
        updates,
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
            22
        );
        let result = reference::batch(&plans);
        assert_eq!(result.len(), CASES.len());
        result
    })
}

fn run_case(index: usize, name: &str) {
    let case = CASES[index];
    assert_eq!(case.name, name);
    let mut test = setup(case);
    let references = &references()[index];
    assert_eq!(references.len(), if case.retests { 5 } else { 1 });
    assert!(test.run(references).unwrap());
}

/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_log_fwd)` @ v2.5.2.
#[test]
fn style_log_fwd() {
    run_case(0, "style_log_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_log_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_fwd_dynamic() {
    run_case(1, "style_log_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_log_rev)` @ v2.5.2.
#[test]
fn style_log_rev() {
    run_case(2, "style_log_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_log_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_rev_dynamic() {
    run_case(3, "style_log_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_lin_fwd)` @ v2.5.2.
#[test]
fn style_lin_fwd() {
    run_case(4, "style_lin_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_lin_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_lin_fwd_dynamic() {
    run_case(5, "style_lin_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_lin_rev)` @ v2.5.2.
#[test]
fn style_lin_rev() {
    run_case(6, "style_lin_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_lin_rev_dynamic)` @ v2.5.2.
#[test]
fn style_lin_rev_dynamic() {
    run_case(7, "style_lin_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, scurve_fwd)` @ v2.5.2.
#[test]
fn scurve_fwd() {
    run_case(8, "scurve_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, scurve_fwd_dynamic)` @ v2.5.2.
#[test]
fn scurve_fwd_dynamic() {
    run_case(9, "scurve_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, scurve_rev)` @ v2.5.2.
#[test]
fn scurve_rev() {
    run_case(10, "scurve_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, scurve_rev_dynamic)` @ v2.5.2.
#[test]
fn scurve_rev_dynamic() {
    run_case(11, "scurve_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, style_log_dynamic_retests)` @ v2.5.2.
#[test]
fn style_log_dynamic_retests() {
    run_case(12, "style_log_dynamic_retests");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingRGBCurve, two_transforms_retests)` @ v2.5.2.
#[test]
fn two_transforms_retests() {
    run_case(13, "two_transforms_retests");
}
