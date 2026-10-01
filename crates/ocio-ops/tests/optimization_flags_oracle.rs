// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The optimization flags' values against the wheel's: the CPU processor's cache ID prints the
//! flags it was made with, `" oFlags "` then the number (src/OpenColorIO/CPUProcessor.cpp:
//! 367-372 @ v2.5.2). Each flag and level by its name, and a combination of several.

use core::ffi::c_ulong;

use ocio_ops::open_color_types::OptimizationFlags;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

/// Every flag and level, by upstream's name.
const FLAGS: [(&str, OptimizationFlags); 28] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_IDENTITY", OptimizationFlags::IDENTITY),
    (
        "OPTIMIZATION_IDENTITY_GAMMA",
        OptimizationFlags::IDENTITY_GAMMA,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_CDL",
        OptimizationFlags::PAIR_IDENTITY_CDL,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_EXPOSURE_CONTRAST",
        OptimizationFlags::PAIR_IDENTITY_EXPOSURE_CONTRAST,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_FIXED_FUNCTION",
        OptimizationFlags::PAIR_IDENTITY_FIXED_FUNCTION,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_GAMMA",
        OptimizationFlags::PAIR_IDENTITY_GAMMA,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_LUT1D",
        OptimizationFlags::PAIR_IDENTITY_LUT1D,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_LUT3D",
        OptimizationFlags::PAIR_IDENTITY_LUT3D,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_LOG",
        OptimizationFlags::PAIR_IDENTITY_LOG,
    ),
    (
        "OPTIMIZATION_PAIR_IDENTITY_GRADING",
        OptimizationFlags::PAIR_IDENTITY_GRADING,
    ),
    (
        "OPTIMIZATION_COMP_EXPONENT",
        OptimizationFlags::COMP_EXPONENT,
    ),
    ("OPTIMIZATION_COMP_GAMMA", OptimizationFlags::COMP_GAMMA),
    ("OPTIMIZATION_COMP_MATRIX", OptimizationFlags::COMP_MATRIX),
    ("OPTIMIZATION_COMP_LUT1D", OptimizationFlags::COMP_LUT1D),
    ("OPTIMIZATION_COMP_LUT3D", OptimizationFlags::COMP_LUT3D),
    ("OPTIMIZATION_COMP_RANGE", OptimizationFlags::COMP_RANGE),
    (
        "OPTIMIZATION_COMP_SEPARABLE_PREFIX",
        OptimizationFlags::COMP_SEPARABLE_PREFIX,
    ),
    ("OPTIMIZATION_LUT_INV_FAST", OptimizationFlags::LUT_INV_FAST),
    (
        "OPTIMIZATION_FAST_LOG_EXP_POW",
        OptimizationFlags::FAST_LOG_EXP_POW,
    ),
    ("OPTIMIZATION_SIMPLIFY_OPS", OptimizationFlags::SIMPLIFY_OPS),
    (
        "OPTIMIZATION_NO_DYNAMIC_PROPERTIES",
        OptimizationFlags::NO_DYNAMIC_PROPERTIES,
    ),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
    ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
    ("OPTIMIZATION_VERY_GOOD", OptimizationFlags::VERY_GOOD),
    ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
    ("OPTIMIZATION_DRAFT", OptimizationFlags::DRAFT),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
];

/// The flags the wheel's CPU processor prints for each `optimization` argument (a name, or a
/// list of names that the oracle ORs together).
fn wheel_flags(optimizations: &[Value]) -> Vec<c_ulong> {
    let pixel: Vec<u8> = [0.5f32, 0.25, 0.125, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let calls: Vec<BatchCall<'_>> = optimizations
        .iter()
        .map(|optimization| BatchCall {
            cmd: "cpu_apply",
            args: json!({
                "transform": {
                    "class": "MatrixTransform",
                    "args": {"offset": [0.1, 0.2, 0.3, 0.0]},
                },
                "optimization": optimization,
            }),
            blobs: vec![&pixel],
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| {
            let result = r.unwrap_or_else(|e| panic!("{e}")).result;
            let cpu = result["cpu_cache_id"]
                .as_str()
                .unwrap_or_else(|| panic!("no CPU processor: {result}"));
            let (_, rest) = cpu
                .split_once(" oFlags ")
                .unwrap_or_else(|| panic!("no flags in {cpu:?}"));
            let number = rest.split(' ').next().unwrap();
            number
                .parse()
                .unwrap_or_else(|e| panic!("{number:?} in {cpu:?}: {e}"))
        })
        .collect()
}

#[test]
fn every_flag_has_the_wheels_value() {
    let names: Vec<Value> = FLAGS.iter().map(|(name, _)| json!(name)).collect();
    let wheel = wheel_flags(&names);
    for ((name, flags), wheel) in FLAGS.iter().zip(wheel) {
        assert_eq!(flags.0, wheel, "{name}");
    }
}

#[test]
fn combined_flags_have_the_wheels_value() {
    let lists = [
        vec!["OPTIMIZATION_LOSSLESS", "OPTIMIZATION_COMP_LUT1D"],
        vec!["OPTIMIZATION_IDENTITY", "OPTIMIZATION_COMP_MATRIX"],
        vec![
            "OPTIMIZATION_COMP_SEPARABLE_PREFIX",
            "OPTIMIZATION_LUT_INV_FAST",
            "OPTIMIZATION_NO_DYNAMIC_PROPERTIES",
        ],
    ];
    let wheel = wheel_flags(&lists.iter().map(|list| json!(list)).collect::<Vec<_>>());
    for (list, wheel) in lists.iter().zip(wheel) {
        let port = list
            .iter()
            .map(|name| {
                FLAGS
                    .iter()
                    .find(|(n, _)| n == name)
                    .expect("a known flag")
                    .1
            })
            .fold(OptimizationFlags::NONE, |a, b| a | b);
        assert_eq!(port.0, wheel, "{list:?}");
        // Each of them is set.
        for name in list {
            let flag = FLAGS.iter().find(|(n, _)| n == name).unwrap().1;
            assert!(port.has_flag(flag), "{name} in {list:?}");
        }
    }
}
