// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio_ops::parse_utils`'s string conversions against the wheel's (ParseUtils.cpp, reached
//! through the oracle's `config_calls` on the module): `BoolFromString`, the enums'
//! `*FromString` with their exceptions, and `EnvironmentModeToString`, for spellings in every
//! case, with surrounding white space, empty, non-ASCII and not UTF-8.

use ocio_ops::open_color_types::{
    Allocation, BitDepth, CdlStyle, EnvironmentMode, NegativeStyle, TransformDirection,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::parse_utils::{
    allocation_from_string, bit_depth_from_string, bool_from_string, cdl_style_from_string,
    environment_mode_from_string, environment_mode_to_string, interpolation_from_string,
    negative_style_from_string, transform_direction_from_string,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{exception, hex, result_bytes};
use serde_json::{Value, json};

/// The spellings: each conversion's own words in several cases, and others.
fn spellings() -> Vec<Vec<u8>> {
    let words = [
        "true",
        "yes",
        "no",
        "false",
        "1",
        "forward",
        "inverse",
        "8ui",
        "10ui",
        "12ui",
        "14ui",
        "16ui",
        "32ui",
        "16f",
        "32f",
        "uniform",
        "lg2",
        "nearest",
        "linear",
        "tetrahedral",
        "best",
        "cubic",
        "default",
        "loadpredefined",
        "loadall",
        "asc",
        "noclamp",
        "mirror",
        "pass_thru",
        "clamp",
        "unknown",
    ];
    let mut out: Vec<Vec<u8>> = Vec::new();
    for w in words {
        out.push(w.as_bytes().to_vec());
        out.push(w.to_uppercase().into_bytes());
        let mut mixed = w.as_bytes().to_vec();
        mixed[0] = mixed[0].to_ascii_uppercase();
        out.push(mixed);
        out.push(format!(" {w}").into_bytes());
        out.push(format!("{w}\t").into_bytes());
    }
    out.extend([
        b"".to_vec(),
        b"noClamp".to_vec(),
        b"LoadAll".to_vec(),
        b"\xc3\x89".to_vec(),
        b"forw\xffard".to_vec(),
    ]);
    out
}

/// The value of an enum the wheel returned, by name.
fn enum_name(v: &Value) -> Option<&str> {
    v["result"]["enum"].as_str()
}

#[test]
fn from_string_conversions_match_the_wheel() {
    let words = spellings();
    let functions = [
        "BoolFromString",
        "TransformDirectionFromString",
        "BitDepthFromString",
        "AllocationFromString",
        "InterpolationFromString",
        "EnvironmentModeFromString",
        "CDLStyleFromString",
        "NegativeStyleFromString",
    ];
    let mut calls = Vec::new();
    for function in functions {
        for w in &words {
            calls.push(json!({"call": function, "on": "OCIO", "args": [{"bytes": hex(w)}]}));
        }
    }
    for mode in [
        "ENV_ENVIRONMENT_UNKNOWN",
        "ENV_ENVIRONMENT_LOAD_PREDEFINED",
        "ENV_ENVIRONMENT_LOAD_ALL",
    ] {
        calls.push(json!({"call": "EnvironmentModeToString", "on": "OCIO",
                          "args": [{"enum": mode}]}));
    }
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "raw", "calls": calls}),
        &[],
    );
    let results = response.result["calls"].as_array().unwrap();
    let mut it = results.iter();
    for function in functions {
        for w in &words {
            let wheel = it.next().unwrap();
            let what = format!("{function}({w:02x?}): {wheel}");
            match function {
                "BoolFromString" => {
                    assert_eq!(json!(bool_from_string(Some(w))), wheel["result"], "{what}")
                }
                "TransformDirectionFromString" => match transform_direction_from_string(Some(w)) {
                    Ok(dir) => assert_eq!(
                        Some(match dir {
                            TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
                            TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
                        }),
                        enum_name(wheel),
                        "{what}"
                    ),
                    Err(e) => check_raised(&e, wheel, &what),
                },
                "BitDepthFromString" => {
                    let name = match bit_depth_from_string(Some(w)) {
                        BitDepth::Unknown => "BIT_DEPTH_UNKNOWN",
                        BitDepth::Uint8 => "BIT_DEPTH_UINT8",
                        BitDepth::Uint10 => "BIT_DEPTH_UINT10",
                        BitDepth::Uint12 => "BIT_DEPTH_UINT12",
                        BitDepth::Uint14 => "BIT_DEPTH_UINT14",
                        BitDepth::Uint16 => "BIT_DEPTH_UINT16",
                        BitDepth::Uint32 => "BIT_DEPTH_UINT32",
                        BitDepth::F16 => "BIT_DEPTH_F16",
                        BitDepth::F32 => "BIT_DEPTH_F32",
                    };
                    assert_eq!(Some(name), enum_name(wheel), "{what}");
                }
                "AllocationFromString" => {
                    let name = match allocation_from_string(Some(w)) {
                        Allocation::Unknown => "ALLOCATION_UNKNOWN",
                        Allocation::Uniform => "ALLOCATION_UNIFORM",
                        Allocation::Lg2 => "ALLOCATION_LG2",
                    };
                    assert_eq!(Some(name), enum_name(wheel), "{what}");
                }
                "InterpolationFromString" => {
                    let name = match interpolation_from_string(Some(w)) {
                        Interpolation::Unknown => "INTERP_UNKNOWN",
                        Interpolation::Nearest => "INTERP_NEAREST",
                        Interpolation::Linear => "INTERP_LINEAR",
                        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
                        Interpolation::Cubic => "INTERP_CUBIC",
                        Interpolation::Default => "INTERP_DEFAULT",
                        Interpolation::Best => "INTERP_BEST",
                    };
                    assert_eq!(Some(name), enum_name(wheel), "{what}");
                }
                "EnvironmentModeFromString" => {
                    let name = match environment_mode_from_string(Some(w)) {
                        EnvironmentMode::Unknown => "ENV_ENVIRONMENT_UNKNOWN",
                        EnvironmentMode::LoadPredefined => "ENV_ENVIRONMENT_LOAD_PREDEFINED",
                        EnvironmentMode::LoadAll => "ENV_ENVIRONMENT_LOAD_ALL",
                    };
                    assert_eq!(Some(name), enum_name(wheel), "{what}");
                }
                "CDLStyleFromString" => match cdl_style_from_string(Some(w)) {
                    Ok(style) => assert_eq!(
                        Some(match style {
                            CdlStyle::Asc => "CDL_ASC",
                            CdlStyle::NoClamp => "CDL_NO_CLAMP",
                        }),
                        enum_name(wheel),
                        "{what}"
                    ),
                    Err(e) => check_raised(&e, wheel, &what),
                },
                "NegativeStyleFromString" => match negative_style_from_string(Some(w)) {
                    Ok(style) => assert_eq!(
                        Some(match style {
                            NegativeStyle::Clamp => "NEGATIVE_CLAMP",
                            NegativeStyle::Mirror => "NEGATIVE_MIRROR",
                            NegativeStyle::PassThru => "NEGATIVE_PASS_THRU",
                            NegativeStyle::Linear => "NEGATIVE_LINEAR",
                        }),
                        enum_name(wheel),
                        "{what}"
                    ),
                    Err(e) => check_raised(&e, wheel, &what),
                },
                _ => unreachable!(),
            }
        }
    }
    for mode in [
        EnvironmentMode::Unknown,
        EnvironmentMode::LoadPredefined,
        EnvironmentMode::LoadAll,
    ] {
        let wheel = it.next().unwrap();
        assert_eq!(
            environment_mode_to_string(mode).as_bytes(),
            result_bytes(wheel),
            "{mode:?}"
        );
    }
}

/// The port's exception against the wheel's, the message's bytes included (a message the
/// binding can't decode comes back as its bytes).
#[track_caller]
fn check_raised(e: &ocio_ops::Exception, wheel: &Value, what: &str) {
    match wheel["undecodable"].as_str() {
        Some(bytes) => assert_eq!(hex(e.what()), bytes, "{what}"),
        None => {
            let (kind, message) = exception(wheel);
            assert_eq!(kind, "Exception", "{what}");
            assert_eq!(e.what(), message, "{what}");
        }
    }
}
