// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transforms that name a config's objects, against the wheel (`common::transforms`):
//! `ColorSpaceTransform`, `DisplayViewTransform` and `LookTransform`. Their text (`repr()`,
//! `str()`, upstream's `operator<<`) and their validation, every message, for names empty,
//! ending at a NUL, non-ASCII or holding the text's separators, every flag and both
//! directions, alone and in groups. The binding gives them no `equals()`.
//!
//! Their processors need their op builders (WP 3.2).

mod common;

use common::transforms::{Case, check_text, direction_spec, group};
use ocio::{ColorSpaceTransform, DisplayViewTransform, LookTransform, TransformDirection};
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
