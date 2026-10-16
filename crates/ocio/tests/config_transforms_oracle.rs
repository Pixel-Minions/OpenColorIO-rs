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
//! The processors of color space transforms (WP 3.2a), in a config of every kind of color space
//! and named transform; the display view and look transforms' come with their op builders (WP
//! 3.2b-c).

mod common;

use common::transforms::{Case, check_processors_in, check_text, direction_spec, group};
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

/// The file transform of `src` and `cccid`, with the CDL style and the interpolation.
fn file_case(
    [src, cccid]: [&str; 2],
    style: ocio::CdlStyle,
    interp: ocio::Interpolation,
    dir: TransformDirection,
) -> Case {
    let mut port = ocio::FileTransform::new();
    port.set_src(src);
    port.set_ccc_id(cccid);
    port.set_cdl_style(style);
    port.set_interpolation(interp);
    port.set_direction(dir);
    let style_name = match style {
        ocio::CdlStyle::Asc => "CDL_ASC",
        ocio::CdlStyle::NoClamp => "CDL_NO_CLAMP",
    };
    Case::new(
        format!("FileTransform {src:?} {cccid:?} {style:?} {interp:?} {dir:?}"),
        json!({"class": "FileTransform", "calls": [
            ["setSrc", src], ["setCCCId", cccid],
            ["setCDLStyle", {"enum": style_name}],
            ["setInterpolation", {"enum": common::transforms::interpolation_name(interp)}],
            ["setDirection", direction_spec(dir)]]}),
        port,
    )
}

/// File transforms: the default, every interpolation and CDL style, paths and CCC IDs empty,
/// ending at a NUL or non-ASCII, both directions, alone and in groups.
#[test]
fn file_transforms_match_the_wheel() {
    use ocio::{CdlStyle, Interpolation};
    let interps = [
        Interpolation::Unknown,
        Interpolation::Nearest,
        Interpolation::Linear,
        Interpolation::Tetrahedral,
        Interpolation::Cubic,
        Interpolation::Default,
        Interpolation::Best,
    ];
    let mut cases = vec![Case::new(
        "FileTransform default",
        json!({"class": "FileTransform"}),
        ocio::FileTransform::new(),
    )];
    let n = NAMES.len();
    for (k, &src) in NAMES.iter().enumerate() {
        let cccid = NAMES[(k + 2) % n];
        for (j, &interp) in interps.iter().enumerate() {
            let style = if (k + j) % 2 == 0 {
                CdlStyle::Asc
            } else {
                CdlStyle::NoClamp
            };
            let dir = if j % 3 == 0 { Inverse } else { Forward };
            cases.push(file_case([src, cccid], style, interp, dir));
            cases.push(file_case([cccid, src], CdlStyle::NoClamp, interp, dir));
        }
    }
    check_text(&cases, &[]);
    let groups: Vec<Case> = cases
        .chunks(6)
        .enumerate()
        .map(|(k, chunk)| {
            let dir = if k % 2 == 0 { Forward } else { Inverse };
            group(&format!("group {k}"), dir, chunk)
        })
        .collect();
    check_text(&groups, &[]);
}

/// A config of scene-referred and display-referred color spaces, data, an equality group, an
/// alias, roles, view transforms and named transforms (one by its forward transform, one by its
/// inverse), for the processors of color space transforms (WP 3.2a).
const PROCESSOR_CONFIG: &str = "ocio_profile_version: 2
environment: {}
roles:
  default: raw
  scene_linear: lin
displays:
  d:
    - !<View> {name: v, colorspace: raw}
view_transforms:
  - !<ViewTransform>
    name: vt
    from_scene_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 1]}
  - !<ViewTransform>
    name: vt_display
    from_display_reference: !<ExponentTransform> {value: 1.5}
display_colorspaces:
  - !<ColorSpace>
    name: disp
    allocation: lg2
    allocationvars: [-8, 5, 0.00390625]
    from_display_reference: !<ExponentTransform> {value: 2.4}
  - !<ColorSpace>
    name: disp_to
    to_display_reference: !<LogTransform> {base: 10}
colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: lin
    aliases: [linear_alias]
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<ColorSpace>
    name: log
    allocation: lg2
    from_scene_reference: !<LogTransform> {base: 2}
  - !<ColorSpace>
    name: both
    to_scene_reference: !<CDLTransform> {slope: [1.1, 1, 0.9]}
    from_scene_reference: !<CDLTransform> {offset: [0.1, 0, 0]}
  - !<ColorSpace>
    name: glow
    from_scene_reference: !<FixedFunctionTransform> {style: ACES_Glow03}
  - !<ColorSpace>
    name: eq_a
    equalitygroup: g
    to_scene_reference: !<RangeTransform> {min_in_value: 0, max_in_value: 1, min_out_value: 0, max_out_value: 2}
  - !<ColorSpace>
    name: eq_b
    equalitygroup: g
    to_scene_reference: !<RangeTransform> {min_in_value: 0, max_in_value: 1, min_out_value: 0, max_out_value: 3}
  - !<ColorSpace>
    name: none
named_transforms:
  - !<NamedTransform>
    name: nt
    aliases: [nt_alias]
    transform: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}
  - !<NamedTransform>
    name: nt_inv
    inverse_transform: !<LogTransform> {base: 10, direction: inverse}
";

/// The processors of color space transforms between every pair of the config's names (color
/// spaces, a role, an alias, named transforms and their alias, and missing names), with and
/// without data bypass, both directions.
#[test]
fn color_space_transform_processors_match_the_wheel() {
    let config = ocio::Config::create_from_stream(PROCESSOR_CONFIG.as_bytes()).unwrap();
    let names = [
        "raw",
        "lin",
        "linear_alias",
        "scene_linear",
        "LOG",
        "both",
        "glow",
        "eq_a",
        "eq_b",
        "none",
        "disp",
        "disp_to",
        "nt",
        "nt_alias",
        "nt_inv",
        "missing",
    ];
    let mut cases = Vec::new();
    for (i, &src) in names.iter().enumerate() {
        for (j, &dst) in names.iter().enumerate() {
            let bypass = (i + j) % 3 != 0;
            cases.push(color_space_case(src, dst, bypass, Forward));
            if (i * names.len() + j).is_multiple_of(4) {
                cases.push(color_space_case(src, dst, !bypass, Inverse));
            }
        }
    }
    check_processors_in(&cases, Some(&json!({"yaml": PROCESSOR_CONFIG})), &config);
}

/// A config of looks applied forward, inverse or both ways, in process spaces scene- and
/// display-referred, data, the source's own, and one that doesn't exist, for the processors of
/// look transforms (WP 3.2b).
const LOOK_CONFIG: &str = "ocio_profile_version: 2
environment: {}
roles:
  default: raw
displays:
  d:
    - !<View> {name: v, colorspace: raw}
view_transforms:
  - !<ViewTransform>
    name: vt
    from_scene_reference: !<MatrixTransform> {offset: [0.01, 0.02, 0.03, 0]}
looks:
  - !<Look>
    name: fwd
    process_space: log
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}
  - !<Look>
    name: inv
    process_space: lin
    inverse_transform: !<CDLTransform> {offset: [0.1, 0, 0]}
  - !<Look>
    name: both
    process_space: log
    transform: !<CDLTransform> {sat: 0.8}
    inverse_transform: !<ExponentTransform> {value: 2}
  - !<Look>
    name: none
    process_space: lin
  - !<Look>
    name: data
    process_space: raw
    transform: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}
  - !<Look>
    name: disp
    process_space: disp
    transform: !<RangeTransform> {min_in_value: 0, max_in_value: 1, min_out_value: 0, max_out_value: 2}
  - !<Look>
    name: lost
    process_space: nowhere
    transform: !<CDLTransform> {}
display_colorspaces:
  - !<ColorSpace>
    name: disp
    from_display_reference: !<ExponentTransform> {value: 2.4}
colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: lin
    aliases: [lin_alias]
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<ColorSpace>
    name: log
    from_scene_reference: !<LogTransform> {base: 2}
";

/// The processors of look transforms: sources and destinations (the same, aliases, data,
/// display-referred, missing), single looks, lists of looks, options of fall-back looks,
/// inverse looks, empty names, missing looks and process spaces, with and without the color
/// space conversion, both directions.
#[test]
fn look_transform_processors_match_the_wheel() {
    let config = ocio::Config::create_from_stream(LOOK_CONFIG.as_bytes()).unwrap();
    let ends = [
        ("lin", "log"),
        ("log", "lin"),
        ("lin", "lin_alias"),
        ("raw", "lin"),
        ("lin", "disp"),
        ("disp", "raw"),
        ("missing", "lin"),
        ("lin", "missing"),
    ];
    let looks = [
        "",
        "fwd",
        "+fwd",
        "-fwd",
        "inv",
        "-inv",
        "both",
        "-both",
        "none",
        "data",
        "disp",
        "fwd, inv, -both",
        "-inv, data, +disp",
        "fwd, , inv",
        "lost",
        "absent",
        "absent | fwd",
        "fwd | inv",
        "lost | fwd",
        "fwd:inv",
        "\"fwd\", inv",
        "\"fwd, inv",
    ];
    let mut cases = Vec::new();
    for (i, (src, dst)) in ends.iter().enumerate() {
        for (j, looks) in looks.iter().enumerate() {
            let skip = (i + j) % 3 == 0;
            cases.push(look_case([src, dst, looks], skip, Forward));
            if (i + j) % 2 == 0 {
                cases.push(look_case([src, dst, looks], !skip, Inverse));
            }
        }
    }
    check_processors_in(&cases, Some(&json!({"yaml": LOOK_CONFIG})), &config);
}

/// A config of views of every kind, for the processors of display view transforms (WP 3.2c):
/// a color space alone or with looks, a view transform (scene- or display-referred) with a
/// display color space and looks, named transforms as the view's color space or view
/// transform, a shared view using the display's name, data, and looks of empty names only.
const DISPLAY_VIEW_CONFIG: &str = "ocio_profile_version: 2
environment: {}
roles:
  default: raw
shared_views:
  - !<View> {name: shared, view_transform: vt_scene, display_colorspace: <USE_DISPLAY_NAME>}
displays:
  d:
    - !<View> {name: cs, colorspace: out}
    - !<View> {name: cs_looks, colorspace: out, looks: \"lk_scene, -lk_display\"}
    - !<View> {name: vt, view_transform: vt_scene, display_colorspace: dout}
    - !<View> {name: vt_looks, view_transform: vt_scene, display_colorspace: dout, looks: \"lk_scene | lk_display\"}
    - !<View> {name: vt_display, view_transform: vt_display, display_colorspace: dout}
    - !<View> {name: vt_display_looks, view_transform: vt_display, display_colorspace: dout, looks: lk_display}
    - !<View> {name: nt_cs, colorspace: nt, looks: lk_scene}
    - !<View> {name: nt_vt, view_transform: nt, display_colorspace: dout}
    - !<View> {name: data, colorspace: raw}
    - !<View> {name: empty_looks, colorspace: out, looks: \", \"}
    - !<View> {name: vt_data, view_transform: vt_scene, display_colorspace: ddata}
    - !<Views> [shared]
  dout:
    - !<Views> [shared]
view_transforms:
  - !<ViewTransform>
    name: vt_scene
    from_scene_reference: !<MatrixTransform> {matrix: [2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 1]}
  - !<ViewTransform>
    name: vt_display
    to_display_reference: !<ExponentTransform> {value: 1.5}
looks:
  - !<Look>
    name: lk_scene
    process_space: log
    transform: !<CDLTransform> {slope: [1.1, 1, 0.9]}
  - !<Look>
    name: lk_display
    process_space: dproc
    inverse_transform: !<CDLTransform> {offset: [0.1, 0, 0]}
display_colorspaces:
  - !<ColorSpace>
    name: dout
    from_display_reference: !<ExponentTransform> {value: 2.4}
  - !<ColorSpace>
    name: dproc
    to_display_reference: !<RangeTransform> {min_in_value: 0, max_in_value: 1, min_out_value: 0, max_out_value: 2}
  - !<ColorSpace>
    name: ddata
    isdata: true
    from_display_reference: !<MatrixTransform> {offset: [0, 0, 0.2, 0]}
colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: in
    to_scene_reference: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<ColorSpace>
    name: out
    from_scene_reference: !<LogTransform> {base: 10}
  - !<ColorSpace>
    name: log
    from_scene_reference: !<LogTransform> {base: 2}
named_transforms:
  - !<NamedTransform>
    name: nt
    transform: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}
";

/// The processors of display view transforms: every view of the config from a scene-referred,
/// a data and a missing source, with and without the looks and data bypasses, both directions;
/// a missing display and view.
#[test]
fn display_view_transform_processors_match_the_wheel() {
    let config = ocio::Config::create_from_stream(DISPLAY_VIEW_CONFIG.as_bytes()).unwrap();
    let views = [
        ("d", "cs"),
        ("d", "cs_looks"),
        ("d", "vt"),
        ("d", "vt_looks"),
        ("d", "vt_display"),
        ("d", "vt_display_looks"),
        ("d", "nt_cs"),
        ("d", "nt_vt"),
        ("d", "data"),
        ("d", "empty_looks"),
        ("d", "shared"),
        ("dout", "shared"),
        ("missing", "cs"),
        ("d", "missing"),
        ("d", "vt_data"),
    ];
    let mut cases = Vec::new();
    for (k, (display, view)) in views.iter().enumerate() {
        for src in ["in", "raw", "missing"] {
            for (j, (looks_bypass, data_bypass)) in [(false, true), (true, true), (false, false)]
                .into_iter()
                .enumerate()
            {
                let dir = if (k + j) % 2 == 0 { Forward } else { Inverse };
                cases.push(display_view_case(
                    [src, display, view],
                    looks_bypass,
                    data_bypass,
                    dir,
                ));
            }
        }
    }
    check_processors_in(&cases, Some(&json!({"yaml": DISPLAY_VIEW_CONFIG})), &config);
}
