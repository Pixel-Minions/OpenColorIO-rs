// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Shared helpers of the oracle tests: the probe pixels, the oracle's CPU processor with fast
//! math on or off, and the exact comparison with a port renderer.
//!
//! The expected values are the wheel's output for the same input bytes; comparisons are
//! bitwise (NaN payloads and signed zeros included).

#![allow(dead_code)] // Each test crate uses a subset.

use std::sync::OnceLock;

use ocio_ops::op::CpuOp;
use ocio_testkit::Oracle;
use ocio_testkit::compare::f32_bits_report;
use ocio_testkit::oracle::f32_to_bytes;
use ocio_testkit::probe::{self, Rng};
use serde_json::{Value, json};

/// The renderer choice: OCIO's fast-math approximations or the math library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Math {
    /// The default processor (`getDefaultCPUProcessor`, `OPTIMIZATION_DEFAULT`), which sets
    /// `OPTIMIZATION_FAST_LOG_EXP_POW`.
    Fast,
    /// `OPTIMIZATION_DEFAULT` without `OPTIMIZATION_FAST_LOG_EXP_POW`.
    Exact,
}

impl Math {
    /// Both choices.
    pub(crate) const BOTH: [Math; 2] = [Math::Fast, Math::Exact];

    /// The `fastLogExpPow` argument of `getCPUOp`.
    pub(crate) fn fast(self) -> bool {
        self == Math::Fast
    }
}

/// `OPTIMIZATION_DEFAULT` is `OPTIMIZATION_VERY_GOOD`: `OPTIMIZATION_LOSSLESS |
/// OPTIMIZATION_COMP_LUT1D | OPTIMIZATION_LUT_INV_FAST | OPTIMIZATION_FAST_LOG_EXP_POW |
/// OPTIMIZATION_COMP_SEPARABLE_PREFIX` (include/OpenColorIO/OpenColorTypes.h:711-722 @ v2.5.2).
/// These are the same flags without `OPTIMIZATION_FAST_LOG_EXP_POW`; the oracle ORs them.
const DEFAULT_WITHOUT_FAST_MATH: [&str; 4] = [
    "OPTIMIZATION_LOSSLESS",
    "OPTIMIZATION_COMP_LUT1D",
    "OPTIMIZATION_LUT_INV_FAST",
    "OPTIMIZATION_COMP_SEPARABLE_PREFIX",
];

/// A transform direction for a transform spec.
pub(crate) fn direction_enum(dir: ocio_ops::open_color_types::TransformDirection) -> Value {
    match dir {
        ocio_ops::open_color_types::TransformDirection::Forward => {
            json!({"enum": "TRANSFORM_DIR_FORWARD"})
        }
        ocio_ops::open_color_types::TransformDirection::Inverse => {
            json!({"enum": "TRANSFORM_DIR_INVERSE"})
        }
    }
}

/// The wheel's F32 RGBA output for `transform` applied to `input` with a raw config.
pub(crate) fn oracle_apply(transform: &Value, math: Math, input: &[f32]) -> Vec<f32> {
    let mut args = json!({ "transform": transform });
    if math == Math::Exact {
        args["optimization"] = json!(DEFAULT_WITHOUT_FAST_MATH);
    }
    let resp = Oracle::get().call("cpu_apply", args, &[&f32_to_bytes(input)]);
    assert!(
        resp.result.get("exception").is_none(),
        "the wheel refused {transform}: {}",
        resp.result
    );
    resp.blob_f32(0)
}

/// Values that each renderer must get right: signed NaN payloads (quiet and signalling), the
/// neighbours of FLT_MIN and of the fast-math range limits (±126, ±128, ±149), and of small
/// integers and halves (where `sseExp2`'s floor adjusts).
fn extra_specials() -> Vec<f32> {
    let mut v: Vec<f32> = [
        0xffc0_0000u32,
        0x7fc1_2345,
        0xffc1_2345,
        0xff80_0001,
        0x7fbf_ffff,
        0xffbf_ffff,
        0xffff_ffff,
        0x7fff_ffff,
        0x0000_0001,
        0x8000_0001,
        0x807f_ffff,
    ]
    .map(f32::from_bits)
    .to_vec();
    for x in [
        f32::MIN_POSITIVE,
        126.0,
        128.0,
        149.0,
        0.5,
        1.5,
        2.0,
        3.0,
        f32::MAX,
    ] {
        for s in [1.0f32, -1.0] {
            let x = s * x;
            for d in -2i32..=2 {
                v.push(f32::from_bits(x.to_bits().wrapping_add_signed(d)));
            }
        }
    }
    v
}

/// The probe values: all 65,536 halves, the specials, and 1,000,000 seeded random values in
/// several ranges, including negative, subnormal, huge and non-finite values.
pub(crate) fn probe_values() -> &'static [f32] {
    static VALUES: OnceLock<Vec<f32>> = OnceLock::new();
    VALUES.get_or_init(|| {
        let mut v = probe::all_half_values();
        v.extend(probe::special_values());
        v.extend(extra_specials());

        let mut rng = Rng::new(0x5252_0001);
        let mut uniform = |n: usize, lo: f64, hi: f64, out: &mut Vec<f32>| {
            out.extend((0..n).map(|_| rng.uniform(lo, hi)));
        };
        uniform(200_000, 0.0, 1.0, &mut v);
        uniform(200_000, -1.0, 2.0, &mut v);
        uniform(100_000, -150.0, 150.0, &mut v);
        uniform(100_000, -1.0e6, 1.0e6, &mut v);

        let mut rng = Rng::new(0x5252_0002);
        // Every magnitude, both signs.
        v.extend((0..200_000).map(|_| rng.finite_bits()));
        // Subnormals, both signs.
        v.extend((0..100_000).map(|_| {
            let bits = rng.next_u64();
            f32::from_bits(((bits >> 32) as u32 & 0x8000_0000) | (bits as u32 & 0x007f_ffff))
        }));
        // Any bit pattern: NaN payloads and infinities too.
        v.extend((0..100_000).map(|_| rng.any_bits()));
        v
    })
}

/// The probe values as RGBA pixels, each value in each channel.
pub(crate) fn probe_rgba() -> &'static [f32] {
    static RGBA: OnceLock<Vec<f32>> = OnceLock::new();
    RGBA.get_or_init(|| probe::to_rgba_cycled(probe_values()))
}

/// The probe pixels followed by `extra` values (e.g. an op's break points), each in each
/// channel.
pub(crate) fn probe_rgba_with(extra: &[f32]) -> Vec<f32> {
    let mut v = probe_rgba().to_vec();
    if !extra.is_empty() {
        v.extend(probe::to_rgba_cycled(extra));
    }
    v
}

/// Collects the exact comparisons of a test, so a failure reports every mismatching case.
#[derive(Debug, Default)]
pub(crate) struct Checks {
    failures: Vec<String>,
    count: usize,
}

impl Checks {
    /// Compares the wheel's output for `transform` with the port `renderer`, on `input`.
    pub(crate) fn check(
        &mut self,
        label: &str,
        transform: &Value,
        math: Math,
        input: &[f32],
        renderer: &dyn CpuOp,
    ) {
        let expected = oracle_apply(transform, math, input);
        let mut actual = input.to_vec();
        renderer.apply(&mut actual);
        self.count += 1;
        if let Some(report) = f32_bits_report(&expected, &actual, Some(input), 4) {
            self.failures.push(format!(
                "{label} ({math:?}), transform {transform}:\n{report}"
            ));
        }
    }

    /// Records the result of a comparison made by the caller.
    pub(crate) fn record(&mut self, failure: Option<String>) {
        self.count += 1;
        self.failures.extend(failure);
    }

    /// Panics with every failure, if any.
    #[track_caller]
    pub(crate) fn finish(self) {
        assert!(self.count > 0, "no comparisons were made");
        if !self.failures.is_empty() {
            panic!(
                "{} of {} cases differ from the wheel:\n\n{}",
                self.failures.len(),
                self.count,
                self.failures.join("\n")
            );
        }
    }
}
