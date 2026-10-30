// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Direct setup helpers for FixedFunctionOp_test.cpp @ v2.5.2.
//! No expected output is computed: transform/input setters are source inputs.

use ocio::{
    BuiltinTransform, FixedFunctionStyle, FixedFunctionTransform, GroupTransform, Transform,
    TransformDirection,
};
use ocio_gpu_exec::gpu_unit_test::{CustomValues, GpuUnitTest};
use ocio_testkit::upstream::gpu::{comparison_dimensions, input_digest};
use serde_json::{Value, json};

#[derive(Debug)]
pub(super) enum Build {
    Fixed {
        style: FixedFunctionStyle,
        oracle_style: &'static str,
        params: fn() -> Vec<f64>,
        inverse: bool,
    },
    DisplayView {
        display: &'static str,
        view: &'static str,
        roundtrip: bool,
    },
    OutputPair {
        inverse: fn() -> Vec<f64>,
        forward: fn() -> Vec<f64>,
    },
}

#[derive(Debug)]
pub(super) enum Input {
    Ramp { low: f32, high: f32 },
    Literal(&'static [f32]),
    Grid { edge: usize, scale: f32 },
}

#[derive(Debug)]
pub(super) struct Case {
    pub(super) name: &'static str,
    pub(super) build: Build,
    pub(super) input: Input,
    pub(super) threshold: f32,
    pub(super) relative: bool,
    pub(super) minimum: f32,
    pub(super) nan: bool,
    pub(super) infinity: bool,
    pub allow_1d: bool,
    pub(super) original_entries: usize,
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) comparison_entries: usize,
    pub(super) raw_input_digest: &'static str,
    pub(super) padded_input_digest: &'static str,
}

/// Port of GenerateIdentityLut3D (FixedFunctionOp_test.cpp:508-523 @ v2.5.2).
/// RED varies fastest; alpha remains zero. Keep both f32 multiplications separate.
fn identity_grid(edge: usize, scale: f32) -> Vec<f32> {
    let mut values = vec![0.0; edge * edge * edge * 4];
    let c = 1.0 / (edge as f32 - 1.0);
    for i in 0..edge * edge * edge {
        values[4 * i] = scale * (i % edge) as f32 * c;
        values[4 * i + 1] = scale * ((i / edge) % edge) as f32 * c;
        values[4 * i + 2] = scale * ((i / edge / edge) % edge) as f32 * c;
    }
    values
}

pub(super) fn input_test(case: &Case) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("FixedFunction", case.name);
    test.comparison.error_threshold = case.threshold;
    test.comparison.relative = case.relative;
    test.comparison.expected_minimal_value = case.minimum;
    test.test_nan = case.nan;
    test.test_infinity = case.infinity;
    if !case.allow_1d {
        test.shader_settings.allow_texture_1d = Some(false);
    }
    let custom = match &case.input {
        Input::Ramp { low, high } => {
            test.range_min = *low;
            test.range_max = *high;
            None
        }
        Input::Literal(values) => Some(values.to_vec()),
        Input::Grid { edge, scale } => Some(identity_grid(*edge, *scale)),
    };
    if let Some(values) = custom {
        assert_eq!(values.len(), case.original_entries);
        assert_eq!(
            input_digest(&values),
            case.raw_input_digest,
            "{}",
            case.name
        );
        test.set_custom_values(CustomValues {
            input_values: values,
            ..CustomValues::default()
        });
    }
    test.prepare_values().unwrap();
    assert_eq!(test.values.original_input_value_size, case.original_entries);
    assert_eq!(test.values.input_values.len(), 256 * 256 * 4);
    assert_eq!(
        comparison_dimensions(case.original_entries),
        Ok((case.width, case.height))
    );
    assert_eq!(test.comparison_entries().unwrap(), case.comparison_entries);
    assert_eq!(
        input_digest(&test.values.input_values),
        case.padded_input_digest,
        "{}",
        case.name
    );
    assert!(
        test.values.input_values[case.original_entries..]
            .iter()
            .all(|v| v.to_bits() == 0)
    );
    assert!(!test.legacy_shader);
    assert_eq!(test.num_retests(), 0);
    test
}

/// BuildDisplayViewTransform (FixedFunctionOp_test.cpp:478-505 @ v2.5.2).
fn display_view(display: &str, view: &str, roundtrip: bool) -> Transform {
    let mut display_builtin = BuiltinTransform::new();
    display_builtin.set_style(display).unwrap();
    display_builtin.validate().unwrap();
    let mut display_inverse = display_builtin.clone(); // Actual createEditableCopy port.
    display_inverse.set_direction(TransformDirection::Inverse);
    let mut view_builtin = BuiltinTransform::new();
    view_builtin.set_style(view).unwrap();
    view_builtin.validate().unwrap();
    let mut view_inverse = view_builtin.clone();
    view_inverse.set_direction(TransformDirection::Inverse);
    let mut group = GroupTransform::new();
    group.append_transform(display_inverse.into());
    group.append_transform(view_inverse.into());
    if roundtrip {
        group.append_transform(view_builtin.into());
        group.append_transform(display_builtin.into());
    }
    group.into()
}

pub(super) fn transform(case: &Case) -> Transform {
    match &case.build {
        Build::Fixed {
            style,
            params,
            inverse,
            ..
        } => {
            let mut fixed = FixedFunctionTransform::new(*style, &params()).unwrap();
            fixed.set_direction(if *inverse {
                TransformDirection::Inverse
            } else {
                TransformDirection::Forward
            });
            fixed.into()
        }
        Build::DisplayView {
            display,
            view,
            roundtrip,
        } => display_view(display, view, *roundtrip),
        Build::OutputPair { inverse, forward } => {
            // FixedFunctionOp_test.cpp:435-471: deliberately not a round-trip.
            let mut inv =
                FixedFunctionTransform::new(FixedFunctionStyle::AcesOutputTransform20, &inverse())
                    .unwrap();
            inv.set_direction(TransformDirection::Inverse);
            let fwd =
                FixedFunctionTransform::new(FixedFunctionStyle::AcesOutputTransform20, &forward())
                    .unwrap();
            let mut group = GroupTransform::new();
            group.append_transform(inv.into());
            group.append_transform(fwd.into());
            group.into()
        }
    }
}

fn fixed_spec(style: &str, params: Vec<f64>, inverse: Option<bool>) -> Value {
    let calls=inverse.map(|inverse|vec![json!(["setDirection",{"enum":if inverse {"TRANSFORM_DIR_INVERSE"}else{"TRANSFORM_DIR_FORWARD"}}])]).unwrap_or_default();
    json!({"class":"FixedFunctionTransform","args":{"style":{"enum":style},"params":params},"calls":calls})
}

fn builtin_spec(style: &str, inverse: bool) -> Value {
    let mut calls = vec![json!(["setStyle", style]), json!(["validate"])];
    if inverse {
        calls.push(json!(["setDirection",{"enum":"TRANSFORM_DIR_INVERSE"}]));
    }
    json!({"class":"BuiltinTransform","calls":calls})
}

/// Equivalent independent wheel construction from the same literal setter inputs.
/// Builtin copies have no dynamic state/metadata; each validated style is rebuilt
/// on the wheel, with the same group ordering as BuildDisplayViewTransform.
pub(super) fn processor_spec(case: &Case) -> Value {
    let transform = match &case.build {
        Build::Fixed {
            oracle_style,
            params,
            inverse,
            ..
        } => fixed_spec(oracle_style, params(), Some(*inverse)),
        Build::DisplayView {
            display,
            view,
            roundtrip,
        } => {
            let mut children = vec![builtin_spec(display, true), builtin_spec(view, true)];
            if *roundtrip {
                children.push(builtin_spec(view, false));
                children.push(builtin_spec(display, false));
            }
            json!({"class":"GroupTransform","children":children})
        }
        Build::OutputPair { inverse, forward } => json!({"class":"GroupTransform","children":[
            fixed_spec("FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20",inverse(),Some(true)),
            fixed_spec("FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20",forward(),None)]}),
    };
    json!({"config":{"create":{"processor_cache":"off"}},"transform":transform})
}
