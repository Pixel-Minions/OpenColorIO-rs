// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Spike S4: OCIO's three half-float conversions, compared exhaustively.
//!
//! - Imath 3.2.1 ([`ocio_ops::imath_half`]): OCIO's scalar `half` type.
//! - SSE2's software conversion ([`ocio_ops::sse2`]): the SSE2 kernels' F16 packing.
//! - F16C ([`ocio_ops::avx`]): the AVX, AVX2 and AVX-512 kernels' F16 packing.
//!
//! What the tests prove:
//! - each scalar port equals the instructions it ports, for every input: the SSE2 intrinsic
//!   sequence of `SSE2.h`, and the F16C hardware;
//! - SSE2's software conversion equals the F16C hardware, for every input;
//! - the Imath port gives the wheel's results through OCIO's F16 and F32 casts. Those casts
//!   multiply by 1.0, which quiets signaling NaNs, so this checks rounding and every other value
//!   but cannot tell Imath from F16C on signaling NaNs.
//!
//! The differences between the conversions are printed (`--nocapture`) and summarized in
//! `docs/spikes/s4.md`. All 65,536 halves always run. All 2^32 floats run with
//! `OCIO_RS_EXHAUSTIVE=1` (use `--release`); otherwise every 61st bit pattern plus every NaN
//! and infinity.
#![cfg(target_arch = "x86_64")]

use std::collections::BTreeMap;
use std::hint::black_box;

use ocio_ops::{avx, imath_half, sse2};
use ocio_testkit::compare::assert_f32_bits_eq;
use ocio_testkit::oracle::bytes_to_f32;
use ocio_testkit::{Oracle, probe};
use serde_json::json;

/// The class of a float input, for reporting differences.
fn f32_class(x: f32) -> &'static str {
    let bits = x.to_bits() & 0x7fff_ffff;
    match bits {
        0 => "zero",
        0x7f80_0000 => "infinity",
        b if b > 0x7f80_0000 && b & 0x0040_0000 != 0 => "quiet NaN",
        b if b > 0x7f80_0000 => "signaling NaN",
        b if b < 0x0080_0000 => "float denormal",
        b if b <= 0x3300_0000 => "below half denormals (to zero)",
        b if b < 0x3880_0000 => "half denormal range",
        b if b < 0x477f_f000 => "half normal range",
        _ => "overflow to infinity",
    }
}

/// The class of a half input, for reporting differences.
fn half_class(h: u16) -> &'static str {
    match h & 0x7fff {
        0 => "zero",
        0x7c00 => "infinity",
        b if b > 0x7c00 && b & 0x0200 != 0 => "quiet NaN",
        b if b > 0x7c00 => "signaling NaN",
        b if b < 0x0400 => "denormal",
        _ => "normal",
    }
}

/// Differences between pairs of conversions: (pair, input class) -> (count, first examples).
#[derive(Default)]
struct Differences(BTreeMap<(&'static str, &'static str), (u64, Vec<String>)>);

impl Differences {
    fn add(&mut self, pair: &'static str, class: &'static str, example: impl FnOnce() -> String) {
        let entry = self.0.entry((pair, class)).or_default();
        entry.0 += 1;
        if entry.1.len() < 3 {
            entry.1.push(example());
        }
    }

    fn merge(&mut self, other: Differences) {
        for (key, (count, examples)) in other.0 {
            let entry = self.0.entry(key).or_default();
            entry.0 += count;
            for e in examples {
                if entry.1.len() < 3 {
                    entry.1.push(e);
                }
            }
        }
    }

    fn print(&self, title: &str, pairs: &[&str]) {
        println!("{title}");
        for pair in pairs {
            let rows: Vec<_> = self.0.iter().filter(|((p, _), _)| p == pair).collect();
            if rows.is_empty() {
                println!("  {pair}: identical for every input");
            }
            for ((_, class), (count, examples)) in rows {
                println!(
                    "  {pair}: {count} differ ({class}), e.g. {}",
                    examples.join("; ")
                );
            }
        }
    }

    fn failures(&self, pairs: &[&str]) -> Vec<String> {
        self.0
            .iter()
            .filter(|((p, _), _)| pairs.contains(p))
            .map(|((p, c), (n, e))| format!("{p}: {n} differ ({c}), e.g. {}", e.join("; ")))
            .collect()
    }
}

const SSE2_PORT: &str = "SSE2 scalar port vs SSE2 intrinsics";
const F16C_PORT: &str = "F16C scalar port vs F16C hardware";
const IMATH_F16C: &str = "Imath vs F16C";
const IMATH_SSE2: &str = "Imath vs SSE2";
const SSE2_F16C: &str = "SSE2 vs F16C";

/// Compares the four float-to-half conversions on the bit patterns `bits`.
fn compare_float_to_half(bits: &[u32], diffs: &mut Differences) {
    let floats: Vec<f32> = bits.iter().map(|&b| f32::from_bits(b)).collect();
    let mut f16c_hw = vec![0u16; floats.len()];
    avx::hardware::cvtps_ph(&floats, &mut f16c_hw);

    for (block, hw_block) in floats.chunks(4).zip(f16c_hw.chunks(4)) {
        let mut lanes = [0f32; 4];
        lanes[..block.len()].copy_from_slice(block);
        let sse2_hw = sse2::intrinsics::cvtps_ph_x4(lanes);

        for (i, (&x, &hw)) in block.iter().zip(hw_block).enumerate() {
            let imath = imath_half::float_to_half(x);
            let sse2 = sse2::sse2_cvtps_ph(x);
            let f16c = avx::f16c_cvtps_ph(x);
            let class = f32_class(x);
            let show =
                |a: u16, b: u16| move || format!("{:#010x} -> {a:#06x} / {b:#06x}", x.to_bits());
            if sse2 != sse2_hw[i] {
                diffs.add(SSE2_PORT, class, show(sse2, sse2_hw[i]));
            }
            if f16c != hw {
                diffs.add(F16C_PORT, class, show(f16c, hw));
            }
            if imath != hw {
                diffs.add(IMATH_F16C, class, show(imath, hw));
            }
            if imath != sse2_hw[i] {
                diffs.add(IMATH_SSE2, class, show(imath, sse2_hw[i]));
            }
            if sse2_hw[i] != hw {
                diffs.add(SSE2_F16C, class, show(sse2_hw[i], hw));
            }
        }
    }
}

/// Runs `compare_float_to_half` over `ranges` of bit patterns, taking every `stride`-th value,
/// on all cores.
fn scan_floats(ranges: &[(u64, u64)], stride: u64) -> Differences {
    const CHUNK: u64 = 1 << 20;
    let mut jobs = Vec::new();
    for &(start, end) in ranges {
        let mut s = start;
        while s < end {
            jobs.push((s, (s + CHUNK * stride).min(end)));
            s += CHUNK * stride;
        }
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut total = Differences::default();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut diffs = Differences::default();
                    loop {
                        let job = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&(start, end)) = jobs.get(job) else {
                            break;
                        };
                        let bits: Vec<u32> = (start..end)
                            .step_by(stride as usize)
                            .map(|b| b as u32)
                            .collect();
                        compare_float_to_half(&bits, &mut diffs);
                    }
                    diffs
                })
            })
            .collect();
        for worker in workers {
            total.merge(worker.join().expect("worker panicked"));
        }
    });
    total
}

/// Float to half, over every float (`OCIO_RS_EXHAUSTIVE=1`) or a strided subset plus every
/// NaN and infinity.
#[test]
fn float_to_half_conversions() {
    if !avx::hardware::available() {
        println!("skipped: this CPU has no F16C");
        return;
    }
    let exhaustive = std::env::var_os("OCIO_RS_EXHAUSTIVE").is_some_and(|v| v == "1");
    let diffs = if exhaustive {
        scan_floats(&[(0, 1 << 32)], 1)
    } else {
        let mut d = scan_floats(&[(0, 1 << 32)], 61);
        d.merge(scan_floats(
            &[(0x7f80_0000, 0x8000_0000), (0xff80_0000, 1 << 32)],
            1,
        ));
        d
    };
    let scope = if exhaustive {
        "all 2^32 floats"
    } else {
        "every 61st float, plus every NaN and infinity"
    };
    diffs.print(
        &format!("float -> half ({scope}):"),
        &[SSE2_PORT, F16C_PORT, IMATH_F16C, IMATH_SSE2, SSE2_F16C],
    );
    let failures = diffs.failures(&[SSE2_PORT, F16C_PORT, SSE2_F16C]);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Half to float, over all 65,536 halves.
#[test]
fn half_to_float_conversions() {
    if !avx::hardware::available() {
        println!("skipped: this CPU has no F16C");
        return;
    }
    let halves: Vec<u16> = (0..=u16::MAX).collect();
    let mut f16c_hw = vec![0f32; halves.len()];
    avx::hardware::cvtph_ps(&halves, &mut f16c_hw);

    let mut diffs = Differences::default();
    for (block, hw_block) in halves.chunks(4).zip(f16c_hw.chunks(4)) {
        let sse2_hw = sse2::intrinsics::cvtph_ps_x4([block[0], block[1], block[2], block[3]]);
        for (i, (&h, &hw)) in block.iter().zip(hw_block).enumerate() {
            let imath = imath_half::half_to_float(h);
            let sse2 = sse2::sse2_cvtph_ps(h);
            let f16c = avx::f16c_cvtph_ps(h);
            let class = half_class(h);
            let show = |a: f32, b: f32| {
                move || format!("{h:#06x} -> {:#010x} / {:#010x}", a.to_bits(), b.to_bits())
            };
            if sse2.to_bits() != sse2_hw[i].to_bits() {
                diffs.add(SSE2_PORT, class, show(sse2, sse2_hw[i]));
            }
            if f16c.to_bits() != hw.to_bits() {
                diffs.add(F16C_PORT, class, show(f16c, hw));
            }
            if imath.to_bits() != hw.to_bits() {
                diffs.add(IMATH_F16C, class, show(imath, hw));
            }
            if imath.to_bits() != sse2_hw[i].to_bits() {
                diffs.add(IMATH_SSE2, class, show(imath, sse2_hw[i]));
            }
            if sse2_hw[i].to_bits() != hw.to_bits() {
                diffs.add(SSE2_F16C, class, show(sse2_hw[i], hw));
            }
        }
    }
    diffs.print(
        "half -> float (all 65,536 halves):",
        &[SSE2_PORT, F16C_PORT, IMATH_F16C, IMATH_SSE2, SSE2_F16C],
    );
    let failures = diffs.failures(&[SSE2_PORT, F16C_PORT, SSE2_F16C]);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

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
