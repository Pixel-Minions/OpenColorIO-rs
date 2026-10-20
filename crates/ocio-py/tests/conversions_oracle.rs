// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The module's string-conversion functions (chunk 6.2b, PyTypes.cpp:862-940 @ v2.5.2)
//! against the wheel, through the same step scripts on both sides:
//! - each `*ToString` (and `BitDepthIsFloat`, `BitDepthToInt`, `GetInverseTransformDirection`,
//!   `CombineTransformDirections`) on every value of its enum and on values outside it
//!   (`OCIO.BitDepth(99)`), whose answers are upstream's defaults or exceptions (I-223), and on
//!   arguments of other types (an int, None, a string, another enum);
//! - each `*FromString` on every name the library knows, in other cases, on unknown, empty,
//!   non-ASCII and non-UTF-8 text, text with a NUL, None, bytes and an int, positionally and
//!   by keyword;
//! - `BoolToString` and `BoolFromString` on what pybind11's `bool` and `const char *` casters
//!   take or refuse.
//!
//! py_types_oracle.rs compares the functions' docstrings.
#![cfg(feature = "python")]

mod common;

use common::{call, name, port, text, wheel};
use serde_json::{Value, json};

fn int(v: &str) -> Value {
    json!({ "int": v })
}

fn bytes(hex: &str) -> Value {
    json!({ "bytes": hex })
}

/// The functions that take an enum, with their enum.
const TO_STRING: [(&str, &str); 17] = [
    ("LoggingLevelToString", "LoggingLevel"),
    ("TransformDirectionToString", "TransformDirection"),
    ("GetInverseTransformDirection", "TransformDirection"),
    ("BitDepthToString", "BitDepth"),
    ("BitDepthIsFloat", "BitDepth"),
    ("BitDepthToInt", "BitDepth"),
    ("AllocationToString", "Allocation"),
    ("InterpolationToString", "Interpolation"),
    ("GpuLanguageToString", "GpuLanguage"),
    ("EnvironmentModeToString", "EnvironmentMode"),
    ("CDLStyleToString", "CDLStyle"),
    ("RangeStyleToString", "RangeStyle"),
    ("FixedFunctionStyleToString", "FixedFunctionStyle"),
    ("GradingStyleToString", "GradingStyle"),
    ("ExposureContrastStyleToString", "ExposureContrastStyle"),
    ("NegativeStyleToString", "NegativeStyle"),
    ("CombineTransformDirections", "TransformDirection"),
];

const FROM_STRING: [&str; 14] = [
    "BoolFromString",
    "LoggingLevelFromString",
    "TransformDirectionFromString",
    "BitDepthFromString",
    "AllocationFromString",
    "InterpolationFromString",
    "GpuLanguageFromString",
    "EnvironmentModeFromString",
    "CDLStyleFromString",
    "RangeStyleFromString",
    "FixedFunctionStyleFromString",
    "GradingStyleFromString",
    "ExposureContrastStyleFromString",
    "NegativeStyleFromString",
];

/// The names the library's functions know, and others.
const NAMES: &[&str] = &[
    "true",
    "TRUE",
    "yes",
    "Yes",
    "no",
    "false",
    "0",
    "1",
    "2",
    "3",
    "none",
    "warning",
    "info",
    "debug",
    "unknown",
    "DEBUG",
    "forward",
    "inverse",
    "Forward",
    "INVERSE",
    "8ui",
    "10ui",
    "12ui",
    "14ui",
    "16ui",
    "32ui",
    "16f",
    "32f",
    "32F",
    "uniform",
    "lg2",
    "LG2",
    "nearest",
    "linear",
    "tetrahedral",
    "best",
    "cubic",
    "default",
    "cg",
    "glsl_1.2",
    "glsl_1.3",
    "GLSL_4.0",
    "glsl_vk_4.6",
    "glsl_es_1.0",
    "glsl_es_3.0",
    "hlsl_sm_5.0",
    "hlsl_dx11",
    "osl_1",
    "msl_2",
    "msl_2.0",
    "loadpredefined",
    "LoadAll",
    "asc",
    "noClamp",
    "noclamp",
    "clamp",
    "Clamp",
    "ACES_RedMod03",
    "ACES_RedMod10",
    "ACES_Glow03",
    "ACES_Glow10",
    "ACES_DarkToDim10",
    "ACES_GamutComp13",
    "ACES2_OutputTransform",
    "ACES2_RGB_TO_JMh",
    "ACES2_TonescaleCompress",
    "ACES2_GamutCompress",
    "REC2100_Surround",
    "RGB_TO_HSV",
    "XYZ_TO_xyY",
    "XYZ_TO_uvY",
    "XYZ_TO_LUV",
    "Lin_TO_PQ",
    "Lin_TO_GammaLog",
    "Lin_TO_DoubleLog",
    "RGB_TO_HSY_LIN",
    "RGB_TO_HSY_LOG",
    "RGB_TO_HSY_VID",
    "ACES_GamutMap02",
    "log",
    "video",
    "lin",
    "Video",
    "mirror",
    "pass_thru",
    "PASS_THRU",
    "",
    " linear",
    "x",
    "\u{e9}",
    "LIN\u{c9}AR",
    "linear\u{0}x",
    "a\u{0}",
];

/// The values each enum is made from: its own and others.
const VALUES: &[&str] = &[
    "-1",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "10",
    "11",
    "12",
    "20",
    "22",
    "23",
    "24",
    "99",
    "254",
    "255",
    "256",
    "2147483647",
    "-2147483648",
];

/// A script's steps.
struct Script {
    steps: Vec<Value>,
}

impl Script {
    fn push(&mut self, step: Value) -> usize {
        self.steps.push(step);
        self.steps.len() - 1
    }
}

fn f(n: &str) -> Value {
    name(&format!("OCIO.{n}"))
}

fn steps() -> Vec<Value> {
    let mut s = Script { steps: vec![] };
    let other_types = [
        int("1"),
        json!({ "none": null }),
        text("x"),
        json!({ "float": "0x3ff0000000000000" }),
    ];
    for (function, e) in TO_STRING {
        let made: Vec<usize> = VALUES
            .iter()
            .map(|v| s.push(call(f(e), json!([int(v)]))))
            .collect();
        for &i in &made {
            if function == "CombineTransformDirections" {
                for &j in &made {
                    s.push(call(f(function), json!([{ "ref": i }, { "ref": j }])));
                }
            } else {
                s.push(call(f(function), json!([{ "ref": i }])));
            }
        }
        for arg in &other_types {
            s.push(call(f(function), json!([arg])));
        }
        s.push(call(
            f(function),
            json!([f("LoggingLevel.LOGGING_LEVEL_INFO")]),
        ));
        s.push(call(f(function), json!([])));
        s.push(json!({ "call": f(function), "args": [], "kwargs": [["x", int("1")]] }));
    }
    for function in FROM_STRING {
        for n in NAMES {
            s.push(call(f(function), json!([text(n)])));
        }
        for arg in [
            json!({ "none": null }),
            bytes("696e666f"),
            bytes("ff"),
            bytes("6c696e65617200"),
            int("1"),
            json!({ "bool": true }),
        ] {
            s.push(call(f(function), json!([arg])));
        }
        s.push(json!({ "call": f(function), "args": [], "kwargs": [["str", text("linear")]] }));
        s.push(json!({ "call": f(function), "args": [], "kwargs": [["style", text("linear")]] }));
    }
    for arg in [
        json!({ "bool": true }),
        json!({ "bool": false }),
        int("1"),
        int("0"),
        json!({ "none": null }),
        json!({ "float": "0x0000000000000000" }),
        text("true"),
    ] {
        s.push(call(f("BoolToString"), json!([arg])));
    }
    s.push(json!({ "call": f("BoolToString"), "args": [], "kwargs": [["value", json!({ "bool": true })]] }));
    s.steps
}

/// The functions answer as the wheel's, step for step.
#[test]
fn conversions_answer_as_the_wheels() {
    let script = json!({ "steps": steps() });
    let ours = port(&script);
    let theirs = wheel(std::slice::from_ref(&script)).remove(0);
    let (ours, theirs) = (
        ours["steps"].as_array().unwrap(),
        theirs["steps"].as_array().unwrap(),
    );
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        assert_eq!(a, b, "step {i}: {}", script["steps"][i]);
    }
    assert_eq!(ours.len(), theirs.len());
}
