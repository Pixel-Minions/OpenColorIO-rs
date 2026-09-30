// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Spike S4: OCIO's half-float conversions.
//!
//! - Imath 3.2.1 ([`ocio_ops::imath_half`]): OCIO's scalar `half` type.
//!
//! What the tests prove:
//! - the Imath port gives the wheel's results through OCIO's F16 and F32 casts. Those casts
//!   multiply by 1.0, which quiets signaling NaNs, so this checks rounding and every other value
//!   but cannot tell Imath from F16C on signaling NaNs.

use std::hint::black_box;

use ocio_ops::imath_half;
use ocio_testkit::compare::assert_f32_bits_eq;
use ocio_testkit::oracle::bytes_to_f32;
use ocio_testkit::{Oracle, probe};
use serde_json::json;

/// The wheel's F16 -> F32 cast of every half, through a processor that does nothing else.
///
/// OCIO replaces the identity MatrixTransform with an identity matrix op, so the CPU processor
/// runs `BitDepthCast<F16, F32>` (`out = float(in) * m_scale`, CPUProcessor.cpp:38-41 @ v2.5.2)
/// then `ScaleRenderer` (`out = in * m_scale[i]`, ops/matrix/MatrixOpCPU.cpp:97-100 @ v2.5.2),
/// both with a scale of 1.0f held in memory. Those multiplies quiet signaling NaNs.
#[test]
fn imath_half_to_float_matches_wheel() {
    let halves: Vec<u16> = (0..=u16::MAX).collect();
    let input: Vec<u8> = halves.iter().flat_map(|h| h.to_le_bytes()).collect();
    let resp = Oracle::get().call(
        "cpu_apply",
        json!({
            "transform": {"class": "MatrixTransform"},
            "in_bitdepth": "BIT_DEPTH_F16",
            "out_bitdepth": "BIT_DEPTH_F32",
        }),
        &[&input],
    );
    assert!(resp.result.get("exception").is_none(), "{}", resp.result);
    let expected = bytes_to_f32(&resp.blobs[0]);

    let scale = black_box(1.0f32);
    let actual: Vec<f32> = halves
        .iter()
        .map(|&h| (imath_half::half_to_float(h) * scale) * scale)
        .collect();
    assert_f32_bits_eq("wheel F16 -> F32 cast", &expected, &actual);
}

/// Floats around every half rounding boundary, the specials, and random values.
fn float_to_half_probes() -> Vec<f32> {
    let mut v = Vec::new();
    for h in 0u16..0x7c00 {
        // A half, the midpoint to the next half, and the floats next to the midpoint.
        let lo = imath_half::half_to_float(h);
        let hi = if h == 0x7bff {
            65536.0
        } else {
            imath_half::half_to_float(h + 1)
        };
        let mid = f32::midpoint(lo, hi);
        for x in [
            lo,
            mid,
            f32::from_bits(mid.to_bits() - 1),
            f32::from_bits(mid.to_bits() + 1),
        ] {
            v.push(x);
            v.push(-x);
        }
    }
    for payload in [
        0x00_0001u32,
        0x00_1fff,
        0x00_2000,
        0x00_2001,
        0x1f_e000,
        0x3f_ffff,
        0x40_0000,
        0x40_0001,
        0x40_2000,
        0x7f_ffff,
    ] {
        v.push(f32::from_bits(0x7f80_0000 | payload));
        v.push(f32::from_bits(0xff80_0000 | payload));
    }
    v.extend(probe::special_values());
    let mut rng = probe::Rng::new(0x5345_4834);
    v.extend((0..1 << 20).map(|_| rng.any_bits()));
    v.extend((0..1 << 20).map(|_| rng.uniform(-70000.0, 70000.0)));
    v.extend((0..1 << 18).map(|_| rng.uniform(-1e-4, 1e-4)));
    while v.len() % 4 != 0 {
        v.push(0.0);
    }
    v
}

/// The wheel's F32 -> F16 cast, through a processor that does nothing else: `ScaleRenderer`
/// then `BitDepthCast<F32, F16>` (`out = half(in * m_scale)`), both scaling by 1.0f.
#[test]
fn imath_float_to_half_matches_wheel() {
    let floats = float_to_half_probes();
    let input: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    let resp = Oracle::get().call(
        "cpu_apply",
        json!({
            "transform": {"class": "MatrixTransform"},
            "in_bitdepth": "BIT_DEPTH_F32",
            "out_bitdepth": "BIT_DEPTH_F16",
        }),
        &[&input],
    );
    assert!(resp.result.get("exception").is_none(), "{}", resp.result);

    let expected: Vec<u16> = resp.blobs[0]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    assert_eq!(expected.len(), floats.len());

    let scale = black_box(1.0f32);
    let mismatches: Vec<String> = floats
        .iter()
        .zip(&expected)
        .filter_map(|(&x, &e)| {
            let a = imath_half::float_to_half((x * scale) * scale);
            (a != e).then(|| format!("{:#010x}: wheel {e:#06x}, port {a:#06x}", x.to_bits()))
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "wheel F32 -> F16 cast: {} of {} differ\n{}",
        mismatches.len(),
        floats.len(),
        mismatches[..mismatches.len().min(20)].join("\n")
    );
}
