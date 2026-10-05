// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! FixedFunctionTransform specs for the oracle (`oracle/ocio_oracle/spec.py`).

use ocio_ops::open_color_types::{FixedFunctionStyle, TransformDirection};
use serde_json::{Value, json};

/// Every transform style, in upstream's order.
pub(crate) const ALL_STYLES: [FixedFunctionStyle; 23] = {
    use FixedFunctionStyle::*;
    [
        AcesRedMod03,
        AcesRedMod10,
        AcesGlow03,
        AcesGlow10,
        AcesDarkToDim10,
        Rec2100Surround,
        RgbToHsv,
        XyzToXyy,
        XyzToUvy,
        XyzToLuv,
        AcesGamutMap02,
        AcesGamutMap07,
        AcesGamutComp13,
        LinToPq,
        LinToGammaLog,
        LinToDoubleLog,
        AcesOutputTransform20,
        AcesRgbToJmh20,
        AcesTonescaleCompress20,
        AcesGamutCompress20,
        RgbToHsyLin,
        RgbToHsyLog,
        RgbToHsyVid,
    ]
};

/// The style's name in PyOpenColorIO (`OCIO.FIXED_FUNCTION_...`).
pub(crate) fn style_name(style: FixedFunctionStyle) -> &'static str {
    use FixedFunctionStyle::*;
    match style {
        AcesRedMod03 => "FIXED_FUNCTION_ACES_RED_MOD_03",
        AcesRedMod10 => "FIXED_FUNCTION_ACES_RED_MOD_10",
        AcesGlow03 => "FIXED_FUNCTION_ACES_GLOW_03",
        AcesGlow10 => "FIXED_FUNCTION_ACES_GLOW_10",
        AcesDarkToDim10 => "FIXED_FUNCTION_ACES_DARK_TO_DIM_10",
        Rec2100Surround => "FIXED_FUNCTION_REC2100_SURROUND",
        RgbToHsv => "FIXED_FUNCTION_RGB_TO_HSV",
        XyzToXyy => "FIXED_FUNCTION_XYZ_TO_xyY",
        XyzToUvy => "FIXED_FUNCTION_XYZ_TO_uvY",
        XyzToLuv => "FIXED_FUNCTION_XYZ_TO_LUV",
        AcesGamutMap02 => "FIXED_FUNCTION_ACES_GAMUTMAP_02",
        AcesGamutMap07 => "FIXED_FUNCTION_ACES_GAMUTMAP_07",
        AcesGamutComp13 => "FIXED_FUNCTION_ACES_GAMUT_COMP_13",
        LinToPq => "FIXED_FUNCTION_LIN_TO_PQ",
        LinToGammaLog => "FIXED_FUNCTION_LIN_TO_GAMMA_LOG",
        LinToDoubleLog => "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
        AcesOutputTransform20 => "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20",
        AcesRgbToJmh20 => "FIXED_FUNCTION_ACES_RGB_TO_JMH_20",
        AcesTonescaleCompress20 => "FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20",
        AcesGamutCompress20 => "FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20",
        RgbToHsyLin => "FIXED_FUNCTION_RGB_TO_HSY_LIN",
        RgbToHsyLog => "FIXED_FUNCTION_RGB_TO_HSY_LOG",
        RgbToHsyVid => "FIXED_FUNCTION_RGB_TO_HSY_VID",
    }
}

/// `{"enum": "FIXED_FUNCTION_..."}`.
pub(crate) fn style_enum(style: FixedFunctionStyle) -> Value {
    json!({"enum": style_name(style)})
}

/// `{"enum": "TRANSFORM_DIR_..."}`.
pub(crate) fn direction_enum(dir: TransformDirection) -> Value {
    match dir {
        TransformDirection::Forward => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
        TransformDirection::Inverse => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
    }
}

/// A transform built with the binding's constructor, `FixedFunctionTransform(style, params,
/// direction)`, which validates it
/// (src/bindings/python/transforms/PyFixedFunctionTransform.cpp:25-40 @ v2.5.2).
pub(crate) fn constructed(
    style: FixedFunctionStyle,
    params: Value,
    dir: TransformDirection,
) -> Value {
    json!({
        "class": "FixedFunctionTransform",
        "args": {"style": style_enum(style), "params": params, "direction": direction_enum(dir)},
    })
}

/// A transform built without validation: the default `FixedFunctionTransform(ACES_GLOW_03)`,
/// then `setDirection(dir)`, `setStyle(style)` and `setParams(params)`, none of which
/// validates. (`setStyle` keeps the direction: `FixedFunctionTransformImpl::setStyle`,
/// src/OpenColorIO/transforms/FixedFunctionTransform.cpp:136-140 @ v2.5.2.)
pub(crate) fn unvalidated(
    style: FixedFunctionStyle,
    params: Value,
    dir: TransformDirection,
) -> Value {
    json!({
        "class": "FixedFunctionTransform",
        "args": {"style": style_enum(FixedFunctionStyle::AcesGlow03)},
        "calls": [
            ["setDirection", direction_enum(dir)],
            ["setStyle", style_enum(style)],
            ["setParams", params],
        ],
    })
}
