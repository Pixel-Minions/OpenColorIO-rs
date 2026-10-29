// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/GradingHueCurveOp_test.cpp` @ v2.5.2.
//! Original curve/input data; actual uncached wheel CPU and shader references.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{
    BSplineType, GradingBSplineCurve, GradingControlPoint, GradingHueCurve,
    GradingHueCurveTransform, GradingStyle, HsyTransformStyle, HueCurveType, Transform,
    TransformDirection,
};
use ocio_gpu_exec::gpu_unit_test::{CustomValues, GpuUnitTest, ReferenceRun};
use ocio_testkit::upstream::gpu::{comparison_dimensions, input_digest};
use serde_json::{Value, json};

#[path = "support/suite4_reference.rs"]
mod reference;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Log,
    Lin,
    Draw,
    Bypass,
}
#[derive(Clone, Copy, Debug)]
struct Case {
    name: &'static str,
    kind: Kind,
    inverse: bool,
    dynamic: bool,
}

// Original f32 control-point inputs, GradingHueCurveOp_test.cpp:35-67,137-171.
const HH: &[[f32; 2]] = &[
    [0.05, 0.15],
    [0.2, 0.3],
    [0.35, 0.4],
    [0.45, 0.45],
    [0.6, 0.7],
    [0.8, 0.85],
];
const HS: &[[f32; 2]] = &[
    [-0.1, 1.2],
    [0.2, 0.7],
    [0.4, 1.5],
    [0.5, 0.5],
    [0.6, 1.4],
    [0.8, 0.7],
];
const HL: &[[f32; 2]] = &[[0.1, 1.5], [0.2, 0.7], [0.4, 1.4], [0.5, 0.8], [0.8, 0.5]];
const LS_LOG: &[[f32; 2]] = &[[0.05, 1.5], [0.5, 0.9], [1.1, 1.4]];
const SS: &[[f32; 2]] = &[[0.0, 0.1], [0.5, 0.45], [1.0, 1.1]];
const LL_LOG: &[[f32; 2]] = &[[-0.02, -0.04], [0.2, 0.1], [0.8, 0.95], [1.1, 1.2]];
const SL_LOG: &[[f32; 2]] = &[[0.0, 1.2], [0.6, 0.8], [0.9, 1.1]];
const HFX: &[[f32; 2]] = &[
    [0.2, 0.05],
    [0.4, -0.09],
    [0.6, -0.2],
    [0.8, 0.05],
    [0.99, -0.02],
];
const LL_LIN: &[[f32; 2]] = &[[-8.0, -7.0], [-2.0, -3.0], [2.0, 3.5], [8.0, 7.0]];
const LS_LIN: &[[f32; 2]] = &[
    [-6.0, 0.9],
    [-3.0, 0.95],
    [0.0, 1.1],
    [2.0, 1.0],
    [4.0, 0.6],
    [6.0, 0.55],
];
const SL_LIN: &[[f32; 2]] = &[[0.0, 1.2], [0.6, 0.8], [0.9, 1.05], [1.0, 1.1]];
const CURVES: [(HueCurveType, &str); 8] = [
    (HueCurveType::HueHue, "HUE_HUE"),
    (HueCurveType::HueSat, "HUE_SAT"),
    (HueCurveType::HueLum, "HUE_LUM"),
    (HueCurveType::LumSat, "LUM_SAT"),
    (HueCurveType::SatSat, "SAT_SAT"),
    (HueCurveType::LumLum, "LUM_LUM"),
    (HueCurveType::SatLum, "SAT_LUM"),
    (HueCurveType::HueFx, "HUE_FX"),
];

fn point_data(kind: Kind) -> [&'static [[f32; 2]]; 8] {
    if matches!(kind, Kind::Lin) {
        [HH, HS, HL, LS_LIN, SS, LL_LIN, SL_LIN, HFX]
    } else {
        [HH, HS, HL, LS_LOG, SS, LL_LOG, SL_LOG, HFX]
    }
}

fn style(case: Case) -> GradingStyle {
    if matches!(case.kind, Kind::Log) {
        GradingStyle::Log
    } else {
        GradingStyle::Lin
    }
}
fn style_name(case: Case) -> &'static str {
    if matches!(case.kind, Kind::Log) {
        "GRADING_LOG"
    } else {
        "GRADING_LIN"
    }
}

fn curve(points: &[[f32; 2]], kind: HueCurveType) -> GradingBSplineCurve {
    let points: Vec<_> = points
        .iter()
        .map(|p| GradingControlPoint::new(p[0], p[1]))
        .collect();
    GradingBSplineCurve::with_points_and_hue_curve_type(&points, kind)
}

/// Direct source setup, including a copy of the actual default value for Draw /
/// Bypass (GradingHueCurveOp_test.cpp:32-83,106-162,185-211,234-253 @ v2.5.2).
fn transform(case: Case) -> Transform {
    let mut hc = GradingHueCurveTransform::new(style(case));
    let value = match case.kind {
        Kind::Log | Kind::Lin => {
            let data = point_data(case.kind);
            let curves: Vec<_> = data
                .iter()
                .zip(CURVES)
                .map(|(points, (kind, _))| curve(points, kind))
                .collect();
            GradingHueCurve::with_curves(
                &curves[0], &curves[1], &curves[2], &curves[3], &curves[4], &curves[5], &curves[6],
                &curves[7],
            )
            .unwrap()
        }
        Kind::Draw => {
            let mut value = hc.value(); // Actual editable copy of the transform value.
            value.set_draw_curve_only(true);
            let curve = value.curve_mut(HueCurveType::HueSat).unwrap();
            curve.set_spline_type(BSplineType::DiagonalBSpline);
            curve.set_num_control_points(3);
            *curve.control_point_mut(0).unwrap() = GradingControlPoint::new(0.0, 0.0);
            *curve.control_point_mut(1).unwrap() = GradingControlPoint::new(0.5, 0.7);
            *curve.control_point_mut(2).unwrap() = GradingControlPoint::new(1.0, 1.0);
            value
        }
        Kind::Bypass => {
            let mut value = hc.value();
            *value
                .curve_mut(HueCurveType::SatSat)
                .unwrap()
                .control_point_mut(1)
                .unwrap() = GradingControlPoint::new(0.4, 0.6);
            hc.set_rgb_to_hsy(HsyTransformStyle::None);
            value
        }
    };
    hc.set_value(&value).unwrap();
    hc.set_direction(if case.inverse {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    });
    if case.dynamic {
        hc.make_dynamic();
    }
    hc.into()
}

/// Independent wheel construction from literal input points only.
fn curve_spec(points: &[[f32; 2]], kind: &str) -> Value {
    let values: Vec<_> = points
        .iter()
        .flat_map(|point| point.iter().copied())
        .collect();
    json!({"object":{"class":"GradingBSplineCurve","args":[values,{"enum":kind}]}})
}

fn value_spec(case: Case) -> Value {
    match case.kind {
        Kind::Log | Kind::Lin => {
            let data = point_data(case.kind);
            let args: Vec<_> = data
                .iter()
                .zip(CURVES)
                .map(|(points, (_, name))| curve_spec(points, name))
                .collect();
            json!({"object":{"class":"GradingHueCurve","args":args}})
        }
        Kind::Draw => {
            // The wheel supplies all unchanged default curves. PyGradingData.cpp's
            // hue_sat property copies point/count/spline/slopes into that curve.
            let changed = json!({"object":{"class":"GradingBSplineCurve",
                "args":[[0.0,0.0,0.5,0.7,1.0,1.0]],
                "calls":[["setSplineType",{"enum":"DIAGONAL_B_SPLINE"}]]}});
            json!({"object":{"class":"GradingHueCurve","args":[{"enum":"GRADING_LIN"}],
                "attrs":[["hue_sat",changed]],"calls":[["setDrawCurveOnly",true]]}})
        }
        Kind::Bypass => {
            // Default endpoint inputs are GradingHueCurve.cpp:40 @ v2.5.2:
            // DefaultSatSatCtrl {0,0},{0.5,0.5},{1,1}; only middle point changes.
            let changed = curve_spec(&[[0.0, 0.0], [0.4, 0.6], [1.0, 1.0]], "SAT_SAT");
            json!({"object":{"class":"GradingHueCurve","args":[{"enum":"GRADING_LIN"}],
                "attrs":[["sat_sat",changed]]}})
        }
    }
}

fn transform_spec(case: Case) -> Value {
    let mut calls = Vec::new();
    if matches!(case.kind, Kind::Bypass) {
        calls.push(json!(["setRGBToHSY",{"enum":"HSY_TRANSFORM_NONE"}]));
    }
    calls.push(json!(["setValue", value_spec(case)]));
    calls.push(json!(["setDirection",{"enum":if case.inverse {"TRANSFORM_DIR_INVERSE"}else{"TRANSFORM_DIR_FORWARD"}}]));
    if case.dynamic {
        calls.push(json!(["makeDynamic"]));
    }
    json!({"class":"GradingHueCurveTransform","args":{"style":{"enum":style_name(case)}},"calls":calls})
}

/// Port of GenerateIdentityLut3D (GradingHueCurveOp_test.cpp:13-29 @ v2.5.2).
/// RED-fast, alpha0, every f32 operation in the original order.
fn input_grid() -> Vec<f32> {
    let edge = 21;
    let min = -0.075_f32;
    let max = 1.425_f32;
    let scale = max - min;
    let c = 1.0 / (edge as f32 - 1.0);
    let mut values = vec![0.0; edge * edge * edge * 4];
    for i in 0..edge * edge * edge {
        values[4 * i] = scale * (i % edge) as f32 * c + min;
        values[4 * i + 1] = scale * ((i / edge) % edge) as f32 * c + min;
        values[4 * i + 2] = scale * ((i / edge / edge) % edge) as f32 * c + min;
    }
    values
}

fn prepared_test(case: Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("GradingHueCurve", case.name);
    match case.kind {
        Kind::Log | Kind::Lin => {
            let values = input_grid();
            assert_eq!(values.len(), 37044);
            assert_eq!(input_digest(&values), GRID_RAW_DIGEST);
            test.set_custom_values(CustomValues {
                input_values: values,
                ..CustomValues::default()
            });
            test.comparison.error_threshold = if matches!(case.kind, Kind::Log) {
                2e-5
            } else {
                4e-4
            };
            test.comparison.expected_minimal_value = 1.0;
            if matches!(case.kind, Kind::Lin) {
                test.comparison.relative = true;
            }
        }
        Kind::Draw => test.comparison.error_threshold = 1e-5,
        Kind::Bypass => {
            test.comparison.error_threshold = 1e-5;
            test.test_infinity = false;
        }
    }
    test.set_processor_transform(&transform(case)).unwrap();
    test.prepare_values().unwrap();
    match case.kind {
        Kind::Log | Kind::Lin => {
            assert_eq!(test.values.original_input_value_size, 37044);
            assert_eq!(test.values.input_values.len(), 256 * 256 * 4);
            assert_eq!(comparison_dimensions(37044), Ok((256, 37)));
            assert_eq!(test.comparison_entries().unwrap(), 37888);
            assert!(
                test.values.input_values[37044..]
                    .iter()
                    .all(|value| value.to_bits() == 0)
            );
            assert_eq!(input_digest(&test.values.input_values), GRID_PADDED_DIGEST);
        }
        Kind::Draw => assert_eq!(
            input_digest(&test.values.input_values),
            "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af"
        ),
        Kind::Bypass => assert_eq!(
            input_digest(&test.values.input_values),
            "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642"
        ),
    }
    assert_eq!(test.num_retests(), 0);
    test
}

fn references() -> &'static [Vec<ReferenceRun>] {
    static REFERENCES: OnceLock<Vec<Vec<ReferenceRun>>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        assert_eq!(CASES.len(), 16);
        assert_eq!(CASES.iter().filter(|case| case.dynamic).count(), 8);
        let plans: Vec<_> = CASES
            .iter()
            .map(|&case| {
                let test = prepared_test(case);
                let bindings = if case.dynamic {
                    vec!["DYNAMIC_PROPERTY_GRADING_HUECURVE"]
                } else {
                    vec![]
                };
                reference::Plan {
                    test,
                    transforms: vec![transform_spec(case)],
                    bindings,
                    updates: vec![vec![]],
                }
            })
            .collect();
        let result = reference::batch(&plans);
        assert_eq!(result.len(), 16);
        assert!(result.iter().all(|frames| frames.len() == 1));
        result
    })
}

fn run_case(index: usize, name: &str) {
    let case = CASES[index];
    assert_eq!(case.name, name);
    let mut test = prepared_test(case);
    assert_eq!(references()[index].len(), 1);
    assert!(test.run(&references()[index]).unwrap());
}

const GRID_RAW_DIGEST: &str = "5da75db0a532a42bd020060c77ab9fa1ffb7023dbcad6aabc199d67c9c18a3d2";
const GRID_PADDED_DIGEST: &str = "105f71cb6a66e4d7c71fc0a2a256c1e3feda5194660b3ea6be956d96eb27eff7";

// BEGIN SOURCE CASE TABLE / WRAPPERS
const CASES: &[Case] = &[
    // GradingHueCurveOp_test.cpp:86-89 @ v2.5.2.
    Case {
        name: "style_log_fwd",
        kind: Kind::Log,
        inverse: false,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:91-94 @ v2.5.2.
    Case {
        name: "style_log_fwd_dynamic",
        kind: Kind::Log,
        inverse: false,
        dynamic: true,
    },
    // GradingHueCurveOp_test.cpp:96-99 @ v2.5.2.
    Case {
        name: "style_log_rev",
        kind: Kind::Log,
        inverse: true,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:101-104 @ v2.5.2.
    Case {
        name: "style_log_rev_dynamic",
        kind: Kind::Log,
        inverse: true,
        dynamic: true,
    },
    // GradingHueCurveOp_test.cpp:165-168 @ v2.5.2.
    Case {
        name: "style_lin_fwd",
        kind: Kind::Lin,
        inverse: false,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:170-173 @ v2.5.2.
    Case {
        name: "style_lin_fwd_dynamic",
        kind: Kind::Lin,
        inverse: false,
        dynamic: true,
    },
    // GradingHueCurveOp_test.cpp:175-178 @ v2.5.2.
    Case {
        name: "style_lin_rev",
        kind: Kind::Lin,
        inverse: true,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:180-183 @ v2.5.2.
    Case {
        name: "style_lin_rev_dynamic",
        kind: Kind::Lin,
        inverse: true,
        dynamic: true,
    },
    // GradingHueCurveOp_test.cpp:214-217 @ v2.5.2.
    Case {
        name: "draw_lin_fwd",
        kind: Kind::Draw,
        inverse: false,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:219-222 @ v2.5.2.
    Case {
        name: "draw_lin_fwd_dynamic",
        kind: Kind::Draw,
        inverse: false,
        dynamic: true,
    },
    // GradingHueCurveOp_test.cpp:224-227 @ v2.5.2.
    Case {
        name: "draw_lin_rev",
        kind: Kind::Draw,
        inverse: true,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:229-232 @ v2.5.2.
    Case {
        name: "draw_lin_rev_dynamic",
        kind: Kind::Draw,
        inverse: true,
        dynamic: true,
    },
    // GradingHueCurveOp_test.cpp:256-259 @ v2.5.2.
    Case {
        name: "bypass_rgbtohsy_fwd",
        kind: Kind::Bypass,
        inverse: false,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:261-264 @ v2.5.2.
    Case {
        name: "bypass_rgbtohsy_fwd_dynamic",
        kind: Kind::Bypass,
        inverse: false,
        dynamic: true,
    },
    // GradingHueCurveOp_test.cpp:266-269 @ v2.5.2.
    Case {
        name: "bypass_rgbtohsy_rev",
        kind: Kind::Bypass,
        inverse: true,
        dynamic: false,
    },
    // GradingHueCurveOp_test.cpp:271-274 @ v2.5.2.
    Case {
        name: "bypass_rgbtohsy_rev_dynamic",
        kind: Kind::Bypass,
        inverse: true,
        dynamic: true,
    },
];

/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_log_fwd)` @ v2.5.2.
#[test]
fn style_log_fwd() {
    run_case(0, "style_log_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_log_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_log_fwd_dynamic() {
    run_case(1, "style_log_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_log_rev)` @ v2.5.2.
#[test]
fn style_log_rev() {
    run_case(2, "style_log_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_log_rev_dynamic)` @ v2.5.2.
#[test]
fn style_log_rev_dynamic() {
    run_case(3, "style_log_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_lin_fwd)` @ v2.5.2.
#[test]
fn style_lin_fwd() {
    run_case(4, "style_lin_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_lin_fwd_dynamic)` @ v2.5.2.
#[test]
fn style_lin_fwd_dynamic() {
    run_case(5, "style_lin_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_lin_rev)` @ v2.5.2.
#[test]
fn style_lin_rev() {
    run_case(6, "style_lin_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, style_lin_rev_dynamic)` @ v2.5.2.
#[test]
fn style_lin_rev_dynamic() {
    run_case(7, "style_lin_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, draw_lin_fwd)` @ v2.5.2.
#[test]
fn draw_lin_fwd() {
    run_case(8, "draw_lin_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, draw_lin_fwd_dynamic)` @ v2.5.2.
#[test]
fn draw_lin_fwd_dynamic() {
    run_case(9, "draw_lin_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, draw_lin_rev)` @ v2.5.2.
#[test]
fn draw_lin_rev() {
    run_case(10, "draw_lin_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, draw_lin_rev_dynamic)` @ v2.5.2.
#[test]
fn draw_lin_rev_dynamic() {
    run_case(11, "draw_lin_rev_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, bypass_rgbtohsy_fwd)` @ v2.5.2.
#[test]
fn bypass_rgbtohsy_fwd() {
    run_case(12, "bypass_rgbtohsy_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, bypass_rgbtohsy_fwd_dynamic)` @ v2.5.2.
#[test]
fn bypass_rgbtohsy_fwd_dynamic() {
    run_case(13, "bypass_rgbtohsy_fwd_dynamic");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, bypass_rgbtohsy_rev)` @ v2.5.2.
#[test]
fn bypass_rgbtohsy_rev() {
    run_case(14, "bypass_rgbtohsy_rev");
}
/// Port of `OCIO_ADD_GPU_TEST(GradingHueCurve, bypass_rgbtohsy_rev_dynamic)` @ v2.5.2.
#[test]
fn bypass_rgbtohsy_rev_dynamic() {
    run_case(15, "bypass_rgbtohsy_rev_dynamic");
}
