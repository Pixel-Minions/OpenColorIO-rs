// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `FixedFunctionStyle`'s names and `FixedFunctionOpData`'s style conversions against the
//! wheel, through `FixedFunctionTransform` and the oracle's `transform_text`:
//! - `FixedFunctionStyleToString`: the transform's `repr()` prints the style with it
//!   (`operator<<`, src/OpenColorIO/transforms/FixedFunctionTransform.cpp:157-177 @ v2.5.2);
//! - the two styles upstream doesn't implement: `FixedFunctionTransform::Create` converts the
//!   style with `FixedFunctionOpData::ConvertStyle`, which refuses them
//!   (FixedFunctionTransform.cpp:14-18, 35-38).

mod common;

use common::fixed_function::{ALL_STYLES, style_enum, unvalidated};
use ocio_ops::open_color_types::{
    FixedFunctionStyle, TransformDirection, fixed_function_style_to_string,
};
use ocio_ops::ops::fixedfunction::FixedFunctionOpStyle;
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use serde_json::json;

/// The style name in a FixedFunctionTransform's `repr()`: `<FixedFunction direction=forward,
/// style=ACES_Glow03>`.
fn repr_style(repr: &str) -> &str {
    let start = repr.find("style=").expect("a style") + "style=".len();
    let rest = &repr[start..];
    let end = rest.find([',', '>']).expect("the style's end");
    &rest[..end]
}

/// Every implemented style's name, as `repr()` prints it; the two unimplemented ones are
/// refused with upstream's message.
#[test]
fn style_names_match_the_wheel() {
    let transforms = ALL_STYLES
        .iter()
        .map(|&style| match style {
            FixedFunctionStyle::AcesGamutMap02 | FixedFunctionStyle::AcesGamutMap07 => json!({
                "class": "FixedFunctionTransform",
                "args": {"style": style_enum(style)},
            }),
            _ => unvalidated(style, json!([]), TransformDirection::Forward),
        })
        .collect();
    let reply = TransformTextRequest {
        transforms,
        pairs: Vec::new(),
    }
    .run();

    for (style, built) in ALL_STYLES.iter().zip(&reply.transforms) {
        match (fixed_function_style_to_string(*style), built) {
            (Ok(name), Built::Text(text)) => {
                assert_eq!(name, repr_style(&text.repr), "{style:?}");
            }
            (Err(port), Built::Raised(wheel)) => {
                assert_eq!(port.message(), wheel.message, "{style:?}");
                // `Create` refuses them through ConvertStyle, with the same text.
                let op =
                    FixedFunctionOpStyle::from_transform_style(*style, TransformDirection::Forward)
                        .unwrap_err();
                assert_eq!(op.message(), wheel.message, "{style:?}");
            }
            (port, wheel) => panic!("{style:?}: the port gives {port:?}, the wheel {wheel:?}"),
        }
    }
}
