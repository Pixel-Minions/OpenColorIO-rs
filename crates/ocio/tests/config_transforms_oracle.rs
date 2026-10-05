// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transforms that name a config's objects or a built-in transform, against the wheel
//! (`common::transforms`): `ColorSpaceTransform`, `DisplayViewTransform`, `LookTransform` and
//! `BuiltinTransform`. Their text (`repr()`, `str()`, upstream's `operator<<`) and their
//! validation, every message, for names empty, ending at a NUL, non-ASCII or holding the
//! text's separators, every flag and both directions, alone and in groups; every built-in
//! style the port registers, in any case, and the styles the wheel refuses, with its message.
//! The binding gives them no `equals()`.
//!
//! Their processors need their op builders (WP 3.2).

mod common;

use common::transforms::{Case, check_text, direction_spec, group};
use ocio::{
    BuiltinTransform, BuiltinTransformRegistry, ColorSpaceTransform, DisplayViewTransform,
    LookTransform, TransformDirection,
};
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

/// Names that print in their own way: empty, ending at a NUL (as a C string), non-ASCII
/// (UTF-8), holding the text's own separators, and an ordinary one.
const NAMES: [&str; 8] = [
    "",
    "source",
    "a\0b",
    "\0hidden",
    "caf\u{e9} \u{2014} \u{1f600}",
    "x, y=z>",
    "$SHOT_${SEQ}",
    "lin_srgb",
];

/// A bool, as the spec passes it.
fn bool_spec(b: bool) -> Value {
    json!(b)
}

fn color_space_case(src: &str, dst: &str, bypass: bool, dir: TransformDirection) -> Case {
    let mut port = ColorSpaceTransform::new();
    port.set_src(src);
    port.set_dst(dst);
    port.set_data_bypass(bypass);
    port.set_direction(dir);
    Case::new(
        format!("ColorSpaceTransform {src:?} {dst:?} {bypass} {dir:?}"),
        json!({"class": "ColorSpaceTransform", "calls": [
            ["setSrc", src], ["setDst", dst], ["setDataBypass", bool_spec(bypass)],
            ["setDirection", direction_spec(dir)]]}),
        port,
    )
}

fn display_view_case(
    names: [&str; 3],
    looks_bypass: bool,
    data_bypass: bool,
    dir: TransformDirection,
) -> Case {
    let [src, display, view] = names;
    let mut port = DisplayViewTransform::new();
    port.set_src(src);
    port.set_display(display);
    port.set_view(view);
    port.set_looks_bypass(looks_bypass);
    port.set_data_bypass(data_bypass);
    port.set_direction(dir);
    Case::new(
        format!("DisplayViewTransform {names:?} {looks_bypass} {data_bypass} {dir:?}"),
        json!({"class": "DisplayViewTransform", "calls": [
            ["setSrc", src], ["setDisplay", display], ["setView", view],
            ["setLooksBypass", bool_spec(looks_bypass)],
            ["setDataBypass", bool_spec(data_bypass)],
            ["setDirection", direction_spec(dir)]]}),
        port,
    )
}

fn look_case(names: [&str; 3], skip: bool, dir: TransformDirection) -> Case {
    let [src, dst, looks] = names;
    let mut port = LookTransform::new();
    port.set_src(src);
    port.set_dst(dst);
    port.set_looks(looks);
    port.set_skip_color_space_conversion(skip);
    port.set_direction(dir);
    Case::new(
        format!("LookTransform {names:?} {skip} {dir:?}"),
        json!({"class": "LookTransform", "calls": [
            ["setSrc", src], ["setDst", dst], ["setLooks", looks],
            ["setSkipColorSpaceConversion", bool_spec(skip)],
            ["setDirection", direction_spec(dir)]]}),
        port,
    )
}

/// The cases: the defaults, every name in each position (so each empty one in turn), every
/// flag, both directions.
fn cases() -> Vec<Case> {
    let mut out = vec![
        Case::new(
            "ColorSpaceTransform default",
            json!({"class": "ColorSpaceTransform"}),
            ColorSpaceTransform::new(),
        ),
        Case::new(
            "DisplayViewTransform default",
            json!({"class": "DisplayViewTransform"}),
            DisplayViewTransform::new(),
        ),
        Case::new(
            "LookTransform default",
            json!({"class": "LookTransform"}),
            LookTransform::new(),
        ),
    ];
    let n = NAMES.len();
    for (k, &a) in NAMES.iter().enumerate() {
        let b = NAMES[(k + 1) % n];
        let c = NAMES[(k + 3) % n];
        for (j, dir) in [Forward, Inverse].into_iter().enumerate() {
            let flag = (k + j) % 2 == 0;
            out.push(color_space_case(a, b, flag, dir));
            out.push(color_space_case(b, a, !flag, dir));
            out.push(look_case([a, b, c], flag, dir));
            out.push(look_case([c, a, b], !flag, dir));
            for looks_bypass in [false, true] {
                for data_bypass in [false, true] {
                    out.push(display_view_case([a, b, c], looks_bypass, data_bypass, dir));
                }
            }
            out.push(display_view_case([b, c, a], flag, !flag, dir));
            out.push(display_view_case([c, a, b], !flag, flag, dir));
        }
    }
    out
}

#[test]
fn text_and_validation_match_the_wheel() {
    check_text(&cases(), &[]);
}

/// Groups print their children on the group's stream, in order.
#[test]
fn groups_of_them_match_the_wheel() {
    let cases = cases();
    let groups: Vec<Case> = cases
        .chunks(5)
        .enumerate()
        .map(|(k, chunk)| {
            let dir = if k % 2 == 0 { Forward } else { Inverse };
            group(&format!("group {k}"), dir, chunk)
        })
        .collect();
    check_text(&groups, &[]);
}

/// The built-in transform of `style`, set as the wheel's `setStyle` sets it.
fn builtin_case(style: &[u8], dir: TransformDirection) -> Case {
    let text = std::str::from_utf8(style).expect("an ASCII style");
    let mut port = BuiltinTransform::new();
    port.set_style(style).expect("a registered style");
    port.set_direction(dir);
    Case::new(
        format!("BuiltinTransform {text:?} {dir:?}"),
        json!({"class": "BuiltinTransform", "calls": [
            ["setStyle", text], ["setDirection", direction_spec(dir)]]}),
        port,
    )
}

/// Every style the port registers names the same style in the wheel, which prints it with the
/// same spelling, also when it is set in lower or upper case.
#[test]
fn every_builtin_style_matches_the_wheel() {
    let registry = BuiltinTransformRegistry::get();
    let mut cases = vec![Case::new(
        "BuiltinTransform default",
        json!({"class": "BuiltinTransform"}),
        BuiltinTransform::new(),
    )];
    for index in 0..registry.num_builtins() {
        let style = registry.builtin_style(index).unwrap();
        let dir = if index % 2 == 0 { Forward } else { Inverse };
        cases.push(builtin_case(style, dir));
        cases.push(builtin_case(&style.to_ascii_lowercase(), Inverse));
        cases.push(builtin_case(&style.to_ascii_uppercase(), Forward));
    }
    check_text(&cases, &[]);
    let groups: Vec<Case> = cases
        .chunks(7)
        .enumerate()
        .map(|(k, chunk)| {
            let dir = if k % 2 == 0 { Inverse } else { Forward };
            group(&format!("group {k}"), dir, chunk)
        })
        .collect();
    check_text(&groups, &[]);
}

/// A style the registry doesn't hold raises the wheel's message, and the port's `set_style`
/// returns the same.
#[test]
fn unknown_builtin_styles_raise_the_wheels_message() {
    let styles = [
        "",
        "\0IDENTITY",
        "IDENTITY ",
        " IDENTITY",
        "IDENTITY\u{a0}",
        "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD_UNKNOWN",
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.",
        "caf\u{e9}",
        "it's",
    ];
    let reply = TransformTextRequest {
        transforms: styles
            .iter()
            .map(|style| json!({"class": "BuiltinTransform", "calls": [["setStyle", style]]}))
            .collect(),
        pairs: Vec::new(),
    }
    .run();
    for (&style, built) in styles.iter().zip(&reply.transforms) {
        let Built::Raised(raised) = built else {
            panic!("{style:?}: the wheel accepted it: {built:?}");
        };
        let port = BuiltinTransform::new().set_style(style).unwrap_err();
        assert_eq!(port.message(), raised.message, "{style:?}");
    }
}
