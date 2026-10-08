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
//! `createGroupTransform` comes with `GradingRGBCurveTransform` (the Grading classes wait for
//! Phase 5, D5): their cache IDs and flags are compared, and the port's refusal of their groups
//! is pinned until then.

mod common;

use common::api_cases::BUILTINS_WITH_OPS;
use common::transforms::{self, Case};
use ocio::{
    BuiltinTransform, BuiltinTransformRegistry, Config, OptimizationFlags, Processor,
    TransformDirection,
};
use ocio_testkit::Oracle;
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::processor_ops::{ProcessorDump, ProcessorOpsReply, ProcessorOpsRequest};
use serde_json::json;

/// Whether the processors of `style` hold a `GradingRGBCurve` op: the ACES 1.x output
/// transforms (their tone curves, `ACES_OUTPUT::Generate_tonecurve_ops` and
/// `Generate_hdr_tonecurve_ops`, ACES.cpp:174-384 @ v2.5.2).
fn holds_a_grading_rgb_curve(style: &str) -> bool {
    style.starts_with("ACES-OUTPUT - ") && !style.ends_with("_2.0")
}

/// The bit depths the optimized processors are checked at.
const DEPTHS: [(Depth, Depth); 5] = [
    (Depth::F32, Depth::F32),
    (Depth::Uint8, Depth::F32),
    (Depth::Uint10, Depth::Uint16),
    (Depth::Uint16, Depth::F32),
    (Depth::F16, Depth::F16),
];

/// The entries with ops whose groups the port builds, in the directions `dirs`, as the oracle
/// and the port build them.
fn cases(dirs: &[TransformDirection]) -> Vec<Case> {
    let mut out = Vec::new();
    for style in BUILTINS_WITH_OPS
        .iter()
        .filter(|style| !holds_a_grading_rgb_curve(style))
    {
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

/// A processor's cache ID, `isNoOp`, `hasChannelCrosstalk` and `isDynamic`, the wheel's
/// against the port's; and the port's refusal of its group (see the module's comment).
fn compare_flags_without_group(wheel: &ProcessorDump, port: &Processor) -> Result<(), String> {
    let port_flags = (
        port.cache_id().map_err(|e| e.to_string())?,
        port.is_no_op().map_err(|e| e.to_string())?,
        port.has_channel_crosstalk(),
        port.is_dynamic(),
    );
    let wheel_flags = (
        wheel.cache_id.clone(),
        wheel.is_no_op,
        wheel.has_channel_crosstalk,
        wheel.is_dynamic,
    );
    if port_flags != wheel_flags {
        return Err(format!("wheel {wheel_flags:?}, port {port_flags:?}"));
    }
    match port.create_group_transform() {
        Err(e) if e.message() == "CreateGradingRGBCurveTransform is not ported yet (Phase 3)." => {
            Ok(())
        }
        other => Err(format!("the group: {other:?}")),
    }
}

/// The processors of the ACES 1.x output transforms, in both directions, unoptimized and
/// optimized at the bit depths of [`DEPTHS`] with the default flags: their cache IDs and flags
/// (see the module's comment).
#[test]
fn processors_of_the_aces_1_output_transforms_match_the_wheel_but_their_groups() {
    let mut requests = Vec::new();
    for style in BUILTINS_WITH_OPS
        .iter()
        .filter(|style| holds_a_grading_rgb_curve(style))
    {
        for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
            let mut t = BuiltinTransform::new();
            t.set_style(style).unwrap();
            t.set_direction(dir);
            let spec = json!({"class": "BuiltinTransform", "calls": [
                ["setStyle", style],
                ["setDirection", transforms::direction_spec(dir)],
            ]});
            let mut levels = vec![(Depth::F32, Depth::F32, "OPTIMIZATION_NONE")];
            levels.extend(DEPTHS.iter().map(|&(i, o)| (i, o, "OPTIMIZATION_DEFAULT")));
            for (input, output, flags) in levels {
                let mut request = ProcessorOpsRequest::new(json!({
                    "transform": spec,
                    "direction": transforms::direction_name(TransformDirection::Forward),
                }));
                request.in_bitdepth = Some(input);
                request.out_bitdepth = Some(output);
                request.optimization = Some(json!(flags));
                let port_flags = if flags == "OPTIMIZATION_NONE" {
                    OptimizationFlags::NONE
                } else {
                    OptimizationFlags::DEFAULT
                };
                let label = format!("{style} {dir:?} ({input:?} -> {output:?}, {flags})");
                requests.push((label, t.clone(), input, output, port_flags, request));
            }
        }
    }
    assert_eq!(requests.len(), 16 * 2 * 6);

    let calls: Vec<BatchCall<'_>> = requests.iter().map(|(.., r)| r.call()).collect();
    let config = Config::create_raw().unwrap();
    let mut failures = Vec::new();
    for ((label, t, input, output, flags, _), response) in
        requests.iter().zip(Oracle::get().batch(&calls, true))
    {
        let reply = ProcessorOpsReply::from_response(response.unwrap_or_else(|e| panic!("{e}")));
        let (Some(wheel), Some(wheel_optimized)) = (&reply.processor, &reply.optimized) else {
            failures.push(format!("{label}: the wheel raised {:?}", reply.raised()));
            continue;
        };
        let processor = config
            .processor_in_direction(&t.clone().into(), TransformDirection::Forward)
            .unwrap();
        let optimized = processor
            .optimized_processor_with_bit_depths(
                transforms::port_depth(*input),
                transforms::port_depth(*output),
                *flags,
            )
            .unwrap();
        if let Err(e) = compare_flags_without_group(wheel, &processor) {
            failures.push(format!("{label}: processor: {e}"));
        }
        if let Err(e) = compare_flags_without_group(wheel_optimized, &optimized) {
            failures.push(format!("{label}: optimized processor: {e}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
