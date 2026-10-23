// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transforms against the wheel beyond their pixels (which
//! `api_battery_oracle.rs` compares): the registry's styles and descriptions, in order; and
//! the processors of the entries whose ops are ported, through `common::transforms`: their
//! cache IDs and the getters of `createGroupTransform`'s ops, every number by its bits, for
//! the processor and the optimized CPU processors at several bit depths, in both directions.
//! A constant that differs from upstream's at the tenth significant digit shows there, where
//! `float` pixels can hide it. Written by the p3-builtins verifier (its killing tests).
//!
//! The ACES 1.x output transforms' processors hold a `GradingRGBCurve` op, whose
//! `createGroupTransform` makes a `GradingRGBCurveTransform` (p5-transforms, 5.5c): their
//! groups are compared as the others'.

mod common;

use common::api_cases::BUILTINS_WITH_OPS;
use common::transforms::{self, Case};
use ocio::{BuiltinTransform, BuiltinTransformRegistry, TransformDirection};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::oracle_values::bytes;
use serde_json::json;

/// The bit depths the optimized processors are checked at.
const DEPTHS: [(Depth, Depth); 5] = [
    (Depth::F32, Depth::F32),
    (Depth::Uint8, Depth::F32),
    (Depth::Uint10, Depth::Uint16),
    (Depth::Uint16, Depth::F32),
    (Depth::F16, Depth::F16),
];

/// The entries with ops, in the directions `dirs`, as the oracle and the port build them.
fn cases(dirs: &[TransformDirection]) -> Vec<Case> {
    let mut out = Vec::new();
    for style in BUILTINS_WITH_OPS {
        for &dir in dirs {
            let mut t = BuiltinTransform::new();
            t.set_style(style).unwrap();
            t.set_direction(dir);
            out.push(Case::new(
                format!("{style} {dir:?}"),
                json!({"class": "BuiltinTransform", "calls": [
                    ["setStyle", style],
                    ["setDirection", transforms::direction_spec(dir)],
                ]}),
                t,
            ));
        }
    }
    out
}

/// The processors of the entries with ops: cache IDs and the group transform's ops.
#[test]
fn processors_of_the_builtin_transforms_match_the_wheel() {
    transforms::check_processors(&cases(&[TransformDirection::Forward]));
}

/// The optimized CPU processors of the entries with ops, in both directions, at several
/// input and output bit depths.
#[test]
fn optimized_processors_of_the_builtin_transforms_match_the_wheel() {
    transforms::check_optimized_processors(
        &cases(&[TransformDirection::Forward, TransformDirection::Inverse]),
        &DEPTHS,
    );
}

/// The registry's 98 styles and descriptions, in order, byte for byte.
#[test]
fn registry_styles_and_descriptions_match_the_wheel() {
    let reply = Oracle::get()
        .call("builtin_transform_names", json!({}), &[])
        .result;
    let wheel = reply["builtins"].as_array().unwrap();
    let registry = BuiltinTransformRegistry::get();
    assert_eq!(wheel.len(), registry.num_builtins());
    let mut failures = Vec::new();
    for (i, entry) in wheel.iter().enumerate() {
        let ws = bytes(&entry[0]);
        let wd = bytes(&entry[1]);
        let ps = registry.builtin_style(i).unwrap().to_vec();
        let pd = registry.builtin_description(i).unwrap().to_vec();
        if ws != ps || wd != pd {
            failures.push(format!(
                "{i}: wheel {:?} / {:?}\n   port {:?} / {:?}",
                String::from_utf8_lossy(&ws),
                String::from_utf8_lossy(&wd),
                String::from_utf8_lossy(&ps),
                String::from_utf8_lossy(&pd)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
