// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transforms that hold a `GammaOpData`, `ExponentTransform` and
//! `ExponentWithLinearTransform`, as the wheel's binding builds them and as the port builds
//! their op data, until the transforms are ported (WP 1.8). In a version 2 config, both build
//! a Gamma op with a copy of the data (`BuildExponentOp`, `BuildExponentWithLinearOp`,
//! src/OpenColorIO/ops/gamma/GammaOp.cpp:179-216 @ v2.5.2).

use ocio_ops::open_color_types::{NegativeStyle, TransformDirection};
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use serde_json::{Value, json};

/// The negative style's enum in an oracle transform spec.
pub(crate) fn negative_style_enum(style: NegativeStyle) -> Value {
    let name = match style {
        NegativeStyle::Clamp => "NEGATIVE_CLAMP",
        NegativeStyle::Mirror => "NEGATIVE_MIRROR",
        NegativeStyle::PassThru => "NEGATIVE_PASS_THRU",
        NegativeStyle::Linear => "NEGATIVE_LINEAR",
    };
    json!({ "enum": name })
}

/// The direction's enum in an oracle transform spec.
pub(crate) fn direction_enum(dir: TransformDirection) -> Value {
    match dir {
        TransformDirection::Forward => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
        TransformDirection::Inverse => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
    }
}

/// The negative style in the config's YAML syntax.
pub(crate) fn yaml_style(style: NegativeStyle) -> &'static str {
    match style {
        NegativeStyle::Clamp => "clamp",
        NegativeStyle::Mirror => "mirror",
        NegativeStyle::PassThru => "pass_thru",
        NegativeStyle::Linear => "linear",
    }
}

/// The direction in the config's YAML syntax.
pub(crate) fn yaml_direction(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "forward",
        TransformDirection::Inverse => "inverse",
    }
}

/// The spec of `ExponentTransform(value, negativeStyle, direction)`, finite values.
pub(crate) fn exponent_spec(value: [f64; 4], neg: NegativeStyle, dir: TransformDirection) -> Value {
    json!({
        "class": "ExponentTransform",
        "args": {
            "value": value,
            "negativeStyle": negative_style_enum(neg),
            "direction": direction_enum(dir),
        },
    })
}

/// The spec of `ExponentWithLinearTransform(gamma, offset, negativeStyle, direction)`, finite
/// values.
pub(crate) fn exponent_with_linear_spec(
    gamma: [f64; 4],
    offset: [f64; 4],
    neg: NegativeStyle,
    dir: TransformDirection,
) -> Value {
    json!({
        "class": "ExponentWithLinearTransform",
        "args": {
            "gamma": gamma,
            "offset": offset,
            "negativeStyle": negative_style_enum(neg),
            "direction": direction_enum(dir),
        },
    })
}

/// The op data of `ExponentTransform(value, negativeStyle, direction)`, not validated.
///
/// `ExponentTransformImpl` holds a default `GammaOpData` (BASIC_FWD, identity parameters;
/// src/OpenColorIO/transforms/ExponentTransform.h:46 and ops/gamma/GammaOpData.cpp:244-252
/// @ v2.5.2). The Python constructor calls `setValue`, `setNegativeStyle` (which converts the
/// style for the current direction) and `setDirection` (which inverts the style if needed),
/// then `validate` (src/bindings/python/transforms/PyExponentTransform.cpp:16-30;
/// transforms/ExponentTransform.cpp:30-53, 72-99). A config's YAML calls the same setters in
/// the order of its keys, without validating (src/OpenColorIO/OCIOYaml.cpp:917-977); the
/// specs here and in the battery give `value`, `style`, then `direction`.
pub(crate) fn exponent_op(
    value: [f64; 4],
    neg: NegativeStyle,
    dir: TransformDirection,
) -> GammaOpData {
    let mut data = GammaOpData::default();
    data.red_params_mut()[0] = value[0];
    data.green_params_mut()[0] = value[1];
    data.blue_params_mut()[0] = value[2];
    data.alpha_params_mut()[0] = value[3];
    let cur_dir = data.direction();
    data.set_style(GammaOpData::convert_style_basic(neg, cur_dir).unwrap());
    data.set_direction(dir);
    data
}

/// The op data of `ExponentWithLinearTransform(gamma, offset, negativeStyle, direction)`, not
/// validated.
///
/// `ExponentWithLinearTransformImpl()` sets `{1, 0}` on the four channels and MONCURVE_FWD
/// (src/OpenColorIO/transforms/ExponentWithLinearTransform.cpp:25-33 @ v2.5.2). The Python
/// constructor calls `setGamma`, `setOffset`, `setNegativeStyle` (which converts the style for
/// the current direction) and `setDirection`, then `validate`
/// (src/bindings/python/transforms/PyExponentWithLinearTransform.cpp:25-40;
/// ExponentWithLinearTransform.cpp:60-73, 91-138).
pub(crate) fn exponent_with_linear_op(
    gamma: [f64; 4],
    offset: [f64; 4],
    neg: NegativeStyle,
    dir: TransformDirection,
) -> GammaOpData {
    let mut data = GammaOpData::default();
    data.set_red_params(vec![1.0, 0.0]);
    data.set_green_params(vec![1.0, 0.0]);
    data.set_blue_params(vec![1.0, 0.0]);
    data.set_alpha_params(vec![1.0, 0.0]);
    data.set_style(GammaStyle::MoncurveFwd);
    // setGamma.
    data.red_params_mut()[0] = gamma[0];
    data.green_params_mut()[0] = gamma[1];
    data.blue_params_mut()[0] = gamma[2];
    data.alpha_params_mut()[0] = gamma[3];
    // setOffset.
    let red = vec![data.red_params()[0], offset[0]];
    let grn = vec![data.green_params()[0], offset[1]];
    let blu = vec![data.blue_params()[0], offset[2]];
    let alp = vec![data.alpha_params()[0], offset[3]];
    data.set_red_params(red);
    data.set_green_params(grn);
    data.set_blue_params(blu);
    data.set_alpha_params(alp);
    let cur_dir = data.direction();
    data.set_style(GammaOpData::convert_style_mon_curve(neg, cur_dir).unwrap());
    data.set_direction(dir);
    data
}
