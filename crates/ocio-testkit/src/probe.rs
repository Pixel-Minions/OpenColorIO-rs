// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Deterministic probe inputs (PLAN.md §8): every half-float bit pattern, special values,
//! ramps and seeded random values. The same seed gives the same values on every platform.

/// All 65,536 half-float bit patterns, converted exactly to `f32`, in bit-pattern order.
pub fn all_half_values() -> Vec<f32> {
    (0..=u16::MAX)
        .map(|bits| half::f16::from_bits(bits).to_f32())
        .collect()
}

/// Values that exercise edge cases: signed zeros, subnormals, extremes, infinities, NaNs and
/// the neighbours of common breakpoints.
pub fn special_values() -> Vec<f32> {
    let mut v = vec![
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::from_bits(0x007f_ffff),
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::EPSILON,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        -f32::NAN,
        f32::from_bits(0x7fc0_0001),
        f32::from_bits(0x7f80_0001),
        65504.0,
        65520.0,
        -65504.0,
        1e-10,
        1e-6,
        1e10,
        1e30,
    ];
    for x in [0.0f32, 0.18, 0.5, 1.0, 2.0, 10.0, 100.0, 1000.0] {
        for s in [1.0f32, -1.0] {
            let x = x * s;
            v.push(x);
            v.push(f32::from_bits(x.to_bits().wrapping_add(1)));
            if x != 0.0 {
                v.push(f32::from_bits(x.to_bits() - 1));
            }
        }
    }
    v
}

/// `n` evenly spaced values from `lo` to `hi` inclusive, computed in `f64`.
pub fn ramp(n: usize, lo: f64, hi: f64) -> Vec<f32> {
    match n {
        0 => Vec::new(),
        1 => vec![lo as f32],
        _ => (0..n)
            .map(|i| (lo + (hi - lo) * i as f64 / (n - 1) as f64) as f32)
            .collect(),
    }
}

/// A seeded SplitMix64 generator: small, fast and identical everywhere.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator for `seed`.
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform value in `[lo, hi)`, computed in `f64` and rounded to `f32`.
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f32 {
        let unit = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        (lo + (hi - lo) * unit) as f32
    }

    /// A random bit pattern: every `f32`, including NaNs, infinities and subnormals.
    pub fn any_bits(&mut self) -> f32 {
        f32::from_bits((self.next_u64() >> 32) as u32)
    }

    /// A random finite `f32`, uniform over bit patterns.
    pub fn finite_bits(&mut self) -> f32 {
        loop {
            let x = self.any_bits();
            if x.is_finite() {
                return x;
            }
        }
    }
}

/// `n` values uniform in `[lo, hi)` from `seed`.
pub fn uniform(seed: u64, n: usize, lo: f64, hi: f64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    (0..n).map(|_| rng.uniform(lo, hi)).collect()
}

/// Pads a list of channel values to whole RGBA pixels, cycling through `values` so every
/// value is seen in every channel; alpha comes from the same list.
pub fn to_rgba_cycled(values: &[f32]) -> Vec<f32> {
    let n = values.len();
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        for c in 0..4 {
            out.push(values[(i + c * 7919) % n]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_values_are_exact() {
        let v = all_half_values();
        assert_eq!(v.len(), 65536);
        assert_eq!(v[0x3c00].to_bits(), 1.0f32.to_bits());
        assert_eq!(v[0x7bff], 65504.0);
        assert!(v[0x7e00].is_nan());
        assert_eq!(v[0x0001], 2f32.powi(-24));
    }

    #[test]
    fn rng_is_stable() {
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xe220_a839_7b1d_cdaf);
    }
}
