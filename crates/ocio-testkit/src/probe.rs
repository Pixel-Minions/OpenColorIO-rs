// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Deterministic probe inputs (PLAN.md §8): every half-float bit pattern, special values,
//! ramps and seeded random values. The same seed gives the same values on every platform.
//!
//! The oracle test battery (`crate::battery`) draws its inputs from one API here, the
//! [`ProbeSet`]s:
//! - [`ProbeSet::Halves`]: every half-float bit pattern, or every n-th one;
//! - [`ProbeSet::Specials`]: [`specials`], the values that break renderers;
//! - [`ProbeSet::Random`]: seeded values in named ranges ([`RandomRange`]: unit, HDR,
//!   negative, tiny, huge, every bit pattern, ...);
//! - [`ProbeSet::Neighbourhoods`]: every value within ±N ulp, and every bit pattern within ±N
//!   steps, of given points, such as an op's break points ([`neighbourhoods`]);
//! - [`ProbeSet::NanBuffers`]: NaN pixels in buffers of every length from one pixel up
//!   ([`nan_buffers`]), so that each part of a renderer's loops meets them.
//!
//! Value sets become RGBA pixels with every value in every channel ([`to_rgba_cycled`]). The
//! exhaustive tier also sweeps every `f32` bit pattern ([`all_f32_chunk`]).
//!
//! Every generator uses integer arithmetic and IEEE `f64` operations only (no platform math
//! library), so the values are identical on Windows and Linux; the unit tests pin them.

/// All 65,536 half-float bit patterns, converted exactly to `f32`, in bit-pattern order.
pub fn all_half_values() -> Vec<f32> {
    (0..=u16::MAX)
        .map(|bits| half::f16::from_bits(bits).to_f32())
        .collect()
}

/// Every `stride`-th half-float bit pattern from 0 (all of them for a stride of 1), converted
/// exactly to `f32`, in bit-pattern order. A prime stride such as 61 samples every exponent and
/// sign with varied mantissa bits.
pub fn half_values_every(stride: usize) -> Vec<f32> {
    assert!(stride > 0, "a stride of 0");
    (0..=u16::MAX)
        .step_by(stride)
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

/// The battery's special values: [`special_values`], then
/// - NaNs of both signs, quiet and signalling, with and without payloads;
/// - the smallest and largest subnormals of both signs;
/// - the values within two bit patterns of FLT_MIN, of the fast-math exponent limits (±126,
///   ±128, ±149), of small integers and halves where `sseExp2`'s floor adjusts (0.5, 1.5, 2,
///   3) and of FLT_MAX, with both signs. The two patterns beyond ±FLT_MAX are ±Inf and a
///   signalling NaN.
///
/// Every renderer must get these right. (The S2 spike's oracle tests collected them.)
pub fn specials() -> Vec<f32> {
    let mut v = special_values();
    v.extend(
        [
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
        .map(f32::from_bits),
    );
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

/// A named range of seeded random values.
///
/// Each value takes a fixed number of draws from the generator ([`RandomRange::Finite`] and
/// [`RandomRange::Negative`] draw until the pattern is finite), so a stream of ranges gives the
/// same values on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RandomRange {
    /// Uniform in `[0, 1)`: display-referred values.
    Unit,
    /// Uniform in `[-1, 2)`: the unit range with negatives and overshoot.
    Overshoot,
    /// Uniform in `[-150, 150)`: the domain of the fast-math `exp2`, whose limits are ±126,
    /// ±128 and ±149, and the range of `log2`.
    Exponent,
    /// Uniform in `[-1e6, 1e6)`: scene-linear HDR values of both signs.
    Hdr,
    /// Negative finite values of every magnitude, uniform over bit patterns (and `-0.0`).
    Negative,
    /// Subnormals and zeros of both signs, uniform over bit patterns.
    Tiny,
    /// Finite values from 1e10 to FLT_MAX in magnitude, both signs, uniform over bit patterns.
    Huge,
    /// Every finite value, both signs, uniform over bit patterns.
    Finite,
    /// Every bit pattern: NaN payloads (quiet and signalling) and infinities too.
    AllBits,
}

impl RandomRange {
    /// Every range, in declaration order.
    pub const ALL: [RandomRange; 9] = [
        RandomRange::Unit,
        RandomRange::Overshoot,
        RandomRange::Exponent,
        RandomRange::Hdr,
        RandomRange::Negative,
        RandomRange::Tiny,
        RandomRange::Huge,
        RandomRange::Finite,
        RandomRange::AllBits,
    ];

    /// The range's name, for reports.
    pub fn name(self) -> &'static str {
        match self {
            RandomRange::Unit => "unit",
            RandomRange::Overshoot => "overshoot",
            RandomRange::Exponent => "exponent",
            RandomRange::Hdr => "hdr",
            RandomRange::Negative => "negative",
            RandomRange::Tiny => "tiny",
            RandomRange::Huge => "huge",
            RandomRange::Finite => "finite",
            RandomRange::AllBits => "all-bits",
        }
    }

    /// The next value of this range from `rng`.
    pub fn sample(self, rng: &mut Rng) -> f32 {
        match self {
            RandomRange::Unit => rng.uniform(0.0, 1.0),
            RandomRange::Overshoot => rng.uniform(-1.0, 2.0),
            RandomRange::Exponent => rng.uniform(-150.0, 150.0),
            RandomRange::Hdr => rng.uniform(-1.0e6, 1.0e6),
            RandomRange::Negative => f32::from_bits(rng.finite_bits().to_bits() | 0x8000_0000),
            RandomRange::Tiny => {
                let bits = rng.next_u64();
                f32::from_bits(((bits >> 32) as u32 & 0x8000_0000) | (bits as u32 & 0x007f_ffff))
            }
            RandomRange::Huge => {
                // 0x5015_02f9 is 1e10f32; 0x7f7f_ffff is FLT_MAX.
                const LO: u32 = 0x5015_02f9;
                const SPAN: u64 = (0x7f7f_ffff - LO) as u64 + 1;
                let bits = rng.next_u64();
                let magnitude = LO + ((bits & 0xffff_ffff) % SPAN) as u32;
                f32::from_bits(((bits >> 32) as u32 & 0x8000_0000) | magnitude)
            }
            RandomRange::Finite => rng.finite_bits(),
            RandomRange::AllBits => rng.any_bits(),
        }
    }
}

/// Seeded random values: `count` values of each range, in order, all drawn from one generator
/// seeded with `seed`.
pub fn random(seed: u64, ranges: &[(RandomRange, usize)]) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let mut v = Vec::with_capacity(ranges.iter().map(|(_, n)| n).sum());
    for &(range, count) in ranges {
        v.extend((0..count).map(|_| range.sample(&mut rng)));
    }
    v
}

/// The position of a non-NaN `f32` in value order, with `-0.0` one step below `0.0`, as in
/// `compare::ulp_distance`.
fn order_key(x: f32) -> i64 {
    let bits = x.to_bits();
    let magnitude = i64::from(bits & 0x7fff_ffff);
    if bits >> 31 == 1 {
        -magnitude - 1
    } else {
        magnitude
    }
}

/// The `f32` at an [`order_key`].
fn from_order_key(key: i64) -> f32 {
    if key >= 0 {
        f32::from_bits(key as u32)
    } else {
        f32::from_bits((-key - 1) as u32 | 0x8000_0000)
    }
}

/// For each point, every value within `ulps` units in the last place in value order, and every
/// bit pattern within `ulps` steps.
///
/// In value order, the neighbours of `0.0` are `-0.0` and the subnormals of both signs, those
/// of `1.0` the floats just below and above it, and the steps stop at ±Inf. For finite nonzero
/// points the bit-pattern steps give the same values; around ±0 and ±Inf they add NaNs
/// (`0.0` minus one step is 0xffffffff, `+Inf` plus one is a signalling NaN), which are
/// probes too. A NaN point has no value-order neighbours: its neighbours are the nearby bit
/// patterns, other payloads, some signalling.
///
/// The points come out in order: each point's value-order neighbours from lowest to highest,
/// then the bit patterns they don't already hold.
pub fn neighbourhoods(points: &[f32], ulps: u32) -> Vec<f32> {
    let n = i64::from(ulps);
    let mut v = Vec::with_capacity(points.len() * (2 * ulps as usize + 1));
    for &x in points {
        let start = v.len();
        if !x.is_nan() {
            let key = order_key(x);
            let (lo, hi) = (order_key(f32::NEG_INFINITY), order_key(f32::INFINITY));
            v.extend(((key - n).max(lo)..=(key + n).min(hi)).map(from_order_key));
        }
        for d in -n..=n {
            // Two's complement: adding `d as u32` steps `d` bit patterns, wrapping.
            let bits = x.to_bits().wrapping_add(d as u32);
            if !v[start..].iter().any(|y| y.to_bits() == bits) {
                v.push(f32::from_bits(bits));
            }
        }
    }
    v
}

/// Pads a list of channel values to whole pixels of `channels` channels, cycling through
/// `values` so every value is seen in every channel: pixel `i`, channel `c` holds
/// `values[(i + 7919 c) % n]`. There are as many pixels as values.
pub fn to_pixels_cycled(values: &[f32], channels: usize) -> Vec<f32> {
    let n = values.len();
    let mut out = Vec::with_capacity(n * channels);
    for i in 0..n {
        for c in 0..channels {
            out.push(values[(i + c * 7919) % n]);
        }
    }
    out
}

/// Pads a list of channel values to whole RGBA pixels, cycling through `values` so every
/// value is seen in every channel; alpha comes from the same list.
pub fn to_rgba_cycled(values: &[f32]) -> Vec<f32> {
    to_pixels_cycled(values, 4)
}

/// The NaNs of [`nan_buffers`]: negative quiet (the x86 default NaN), negative and positive
/// quiet with a payload, and negative signalling.
pub const NAN_PATTERNS: [u32; 4] = [0xffc0_0000, 0xffc1_2345, 0x7fc1_2345, 0xff80_0001];

/// Two RGBA buffers of each length from 1 to `max_pixels` pixels, made of [`NAN_PATTERNS`]:
/// - `"nan rgb"`: pixel `i` is `[q[i % 4], q[(i + 1) % 4], q[(i + 2) % 4], 0.25]`;
/// - `"nan rgba"`: the same with `q[(i + 3) % 4]` in alpha.
///
/// Where a renderer's coefficients can be NaN, a NaN pixel meets a NaN coefficient, and which
/// NaN comes out depends on the operand order. Every length makes each part of the port's
/// loops meet them (in a release build, a vectorized body and a scalar remainder).
///
/// Returns `(name, pixels)` pairs, by length, `"nan rgb"` first.
pub fn nan_buffers(max_pixels: usize) -> Vec<(String, Vec<f32>)> {
    let q = NAN_PATTERNS.map(f32::from_bits);
    let mut out = Vec::with_capacity(2 * max_pixels);
    for n in 1..=max_pixels {
        for (name, alpha_nan) in [("nan rgb", false), ("nan rgba", true)] {
            let pixels = (0..n)
                .flat_map(|i| {
                    let alpha = if alpha_nan { q[(i + 3) % 4] } else { 0.25 };
                    [q[i % 4], q[(i + 1) % 4], q[(i + 2) % 4], alpha]
                })
                .collect();
            out.push((format!("{name}, {n} pixels"), pixels));
        }
    }
    out
}

/// The number of RGBA pixels that hold every `f32` bit pattern once, four per pixel.
pub const ALL_F32_PIXELS: u64 = 1 << 30;

/// Chunk `index` of the sweep of every `f32` bit pattern: `pixels` RGBA pixels, where pixel `j`
/// of the sweep holds the bit patterns `4j` to `4j + 3`, rotated by `j`: channel `c` holds
/// `4j + (j + c) % 4`. Every pattern appears once, in one channel, and each channel sees a
/// quarter of the patterns with every value of the two lowest bits. `pixels` must divide
/// [`ALL_F32_PIXELS`].
pub fn all_f32_chunk(index: u64, pixels: u64) -> Vec<f32> {
    assert!(
        pixels > 0 && ALL_F32_PIXELS.is_multiple_of(pixels),
        "a chunk of {pixels} pixels does not divide the sweep"
    );
    assert!(
        index < ALL_F32_PIXELS / pixels,
        "chunk {index} is past the sweep"
    );
    let first = index * pixels;
    (first..first + pixels)
        .flat_map(|j| (0..4).map(move |c| f32::from_bits((4 * j + (j + c) % 4) as u32)))
        .collect()
}

/// `n` as an English ordinal: 1st, 2nd, 3rd, 4th, 11th, 21st, 61st, ...
fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A named set of probe inputs for the battery.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeSet {
    /// Every `stride`-th half-float bit pattern ([`half_values_every`]); all of them for 1.
    Halves {
        /// The step between bit patterns.
        stride: usize,
    },
    /// [`specials`].
    Specials,
    /// Seeded random values ([`random`]).
    Random {
        /// The set's name, for reports.
        name: &'static str,
        /// The generator's seed.
        seed: u64,
        /// The ranges, in order, and how many values each.
        ranges: Vec<(RandomRange, usize)>,
    },
    /// [`neighbourhoods`] of `points`.
    Neighbourhoods {
        /// The points, such as an op's break points.
        points: Vec<f32>,
        /// How many ulps on each side.
        ulps: u32,
    },
    /// Explicit values.
    Values {
        /// The set's name, for reports.
        name: String,
        /// The values.
        values: Vec<f32>,
    },
    /// [`nan_buffers`]: whole buffers of every length from 1 to `max_pixels` pixels.
    NanBuffers {
        /// The longest buffer.
        max_pixels: usize,
    },
}

impl ProbeSet {
    /// The set's name, for reports.
    pub fn name(&self) -> String {
        match self {
            ProbeSet::Halves { stride: 1 } => "all halves".to_string(),
            ProbeSet::Halves { stride } => format!("every {} half", ordinal(*stride)),
            ProbeSet::Specials => "specials".to_string(),
            ProbeSet::Random { name, .. } => format!("random {name}"),
            ProbeSet::Neighbourhoods { ulps, .. } => format!("neighbourhoods of {ulps} ulp"),
            ProbeSet::Values { name, .. } => name.clone(),
            ProbeSet::NanBuffers { max_pixels } => {
                format!("NaN buffers of 1 to {max_pixels} pixels")
            }
        }
    }

    /// The set's RGBA buffers, as `(name, pixels)` pairs: one buffer with every value in every
    /// channel ([`to_rgba_cycled`]), or the whole buffers of [`ProbeSet::NanBuffers`]. An empty
    /// set has no buffers.
    pub fn rgba_buffers(&self) -> Vec<(String, Vec<f32>)> {
        let values = match self {
            ProbeSet::Halves { stride } => half_values_every(*stride),
            ProbeSet::Specials => specials(),
            ProbeSet::Random { seed, ranges, .. } => random(*seed, ranges),
            ProbeSet::Neighbourhoods { points, ulps } => neighbourhoods(points, *ulps),
            ProbeSet::Values { values, .. } => values.clone(),
            ProbeSet::NanBuffers { max_pixels } => return nan_buffers(*max_pixels),
        };
        if values.is_empty() {
            return Vec::new();
        }
        vec![(self.name(), to_rgba_cycled(&values))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xxhash_rust::xxh3::xxh3_64;

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|v| v.to_bits()).collect()
    }

    /// XXH3-64 of the values' little-endian bytes.
    fn digest(values: &[f32]) -> u64 {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        xxh3_64(&bytes)
    }

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

    #[test]
    fn half_samples_step_through_the_bit_patterns() {
        assert_eq!(bits(&half_values_every(1)), bits(&all_half_values()));
        let all = all_half_values();
        let sample = half_values_every(61);
        assert_eq!(sample.len(), 65536usize.div_ceil(61));
        for (i, v) in sample.iter().enumerate() {
            assert_eq!(v.to_bits(), all[i * 61].to_bits());
        }
    }

    #[test]
    fn specials_extend_the_special_values() {
        let base = special_values();
        let v = specials();
        assert_eq!(bits(&v[..base.len()]), bits(&base));
        // 11 patterns, then 9 points x 2 signs x 5 neighbours.
        assert_eq!(v.len(), base.len() + 11 + 9 * 2 * 5);
        for pattern in [
            0xffc0_0000u32,
            0x7fbf_ffff,
            0xff80_0001,
            0x7f80_0000,
            0x7f80_0001,
        ] {
            assert!(v.iter().any(|x| x.to_bits() == pattern), "{pattern:#010x}");
        }
    }

    /// The specials put the signalling NaNs in every channel, alpha included: a renderer that
    /// does arithmetic on a channel it should pass through quiets them.
    #[test]
    fn specials_put_signalling_nans_in_every_channel() {
        let buffers = ProbeSet::Specials.rgba_buffers();
        assert_eq!(buffers.len(), 1);
        let pixels = &buffers[0].1;
        for pattern in [0x7f80_0001u32, 0xff80_0001, 0x7fbf_ffff, 0xffbf_ffff] {
            assert!(f32::from_bits(pattern).is_nan());
            for c in 0..4 {
                assert!(
                    pixels
                        .iter()
                        .skip(c)
                        .step_by(4)
                        .any(|x| x.to_bits() == pattern),
                    "{pattern:#010x} is missing from channel {c}"
                );
            }
        }
    }

    /// Each range draws what its documentation says, from the same generator calls as the
    /// `Rng` methods it names.
    #[test]
    fn random_ranges_hold_their_values() {
        let n = 20_000;
        for range in RandomRange::ALL {
            let v = random(7, &[(range, n)]);
            assert_eq!(v.len(), n);
            let ok = |x: &f32| match range {
                RandomRange::Unit => (0.0..=1.0).contains(x),
                RandomRange::Overshoot => (-1.0..=2.0).contains(x),
                RandomRange::Exponent => (-150.0..=150.0).contains(x),
                RandomRange::Hdr => (-1.0e6..=1.0e6).contains(x),
                RandomRange::Negative => x.is_finite() && x.is_sign_negative(),
                RandomRange::Tiny => x.is_finite() && !x.is_normal(),
                RandomRange::Huge => x.is_finite() && x.abs() >= 1e10,
                RandomRange::Finite => x.is_finite(),
                RandomRange::AllBits => true,
            };
            if let Some(bad) = v.iter().find(|x| !ok(x)) {
                panic!("{}: {bad:e} ({:#010x})", range.name(), bad.to_bits());
            }
            let negatives = v.iter().filter(|x| x.is_sign_negative()).count();
            match range {
                RandomRange::Unit => assert_eq!(negatives, 0),
                RandomRange::Negative => assert_eq!(negatives, n),
                _ => assert!(negatives > 0 && negatives < n, "{}", range.name()),
            }
        }
        // About 1 in 256 bit patterns is a NaN.
        let all = random(7, &[(RandomRange::AllBits, n)]);
        assert!(all.iter().filter(|x| x.is_nan()).count() > n / 512);
    }

    /// A stream draws its ranges in order from one generator, as the S2 oracle tests drew their
    /// probe values.
    #[test]
    fn random_streams_share_one_generator() {
        let v = random(
            0x5252_0001,
            &[(RandomRange::Unit, 100), (RandomRange::Overshoot, 100)],
        );
        let mut rng = Rng::new(0x5252_0001);
        let mut expected: Vec<f32> = (0..100).map(|_| rng.uniform(0.0, 1.0)).collect();
        expected.extend((0..100).map(|_| rng.uniform(-1.0, 2.0)));
        assert_eq!(bits(&v), bits(&expected));

        let v = random(
            0x5252_0002,
            &[
                (RandomRange::Finite, 100),
                (RandomRange::Tiny, 100),
                (RandomRange::AllBits, 100),
            ],
        );
        let mut rng = Rng::new(0x5252_0002);
        let mut expected: Vec<f32> = (0..100).map(|_| rng.finite_bits()).collect();
        expected.extend((0..100).map(|_| {
            let b = rng.next_u64();
            f32::from_bits(((b >> 32) as u32 & 0x8000_0000) | (b as u32 & 0x007f_ffff))
        }));
        expected.extend((0..100).map(|_| rng.any_bits()));
        assert_eq!(bits(&v), bits(&expected));
    }

    #[test]
    fn neighbourhoods_hold_value_and_bit_pattern_neighbours() {
        let tiny = f32::from_bits(1);
        // Value order through -0.0, then the NaNs just below +0.0 in bit-pattern order.
        let mut expected = bits(&[-tiny, -0.0, 0.0, tiny, 2.0 * tiny]);
        expected.extend([0xffff_fffe, 0xffff_ffff]);
        assert_eq!(bits(&neighbourhoods(&[0.0], 2)), expected);
        let mut expected = bits(&[-tiny, -0.0, 0.0]);
        expected.push(0x7fff_ffff);
        assert_eq!(bits(&neighbourhoods(&[-0.0], 1)), expected);
        // Finite nonzero points: both orders give the same values.
        let below_one = f32::from_bits(1.0f32.to_bits() - 1);
        let above_one = f32::from_bits(1.0f32.to_bits() + 1);
        assert_eq!(
            bits(&neighbourhoods(&[1.0, -1.0], 1)),
            bits(&[below_one, 1.0, above_one, -above_one, -1.0, -below_one])
        );
        // Value order stops at the infinities; bit patterns go on into the NaNs.
        let mut expected = bits(&[
            f32::from_bits(0x7f7f_fffd),
            f32::from_bits(0x7f7f_fffe),
            f32::MAX,
            f32::INFINITY,
        ]);
        expected.push(0x7f80_0001);
        assert_eq!(bits(&neighbourhoods(&[f32::MAX], 2)), expected);
        let mut expected = bits(&[f32::NEG_INFINITY, f32::MIN]);
        expected.push(0xff80_0001);
        assert_eq!(bits(&neighbourhoods(&[f32::NEG_INFINITY], 1)), expected);
        // A NaN's neighbours are the nearby payloads.
        assert_eq!(
            bits(&neighbourhoods(&[f32::from_bits(0x7fc0_0000)], 1)),
            vec![0x7fbf_ffff, 0x7fc0_0000, 0x7fc0_0001]
        );
    }

    /// The S2 oracle tests probed the bit patterns within 3 steps of each break point; the
    /// neighbourhoods hold them all, and every value within 3 ulps in value order, for points
    /// at zero, the infinities and NaN too.
    #[test]
    fn neighbourhoods_hold_the_s2_bit_pattern_neighbours() {
        let points = [
            0.0,
            -0.0,
            1.0,
            -0.05,
            1e-40,
            f32::MIN_POSITIVE,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(0xffc0_0000),
        ];
        for x in points {
            let held = bits(&neighbourhoods(&[x], 3));
            for d in -3i32..=3 {
                let pattern = x.to_bits().wrapping_add_signed(d);
                assert!(held.contains(&pattern), "{x:e}: {pattern:#010x}");
            }
            if !x.is_nan() {
                for d in -3i64..=3 {
                    let key = order_key(x) + d;
                    if (order_key(f32::NEG_INFINITY)..=order_key(f32::INFINITY)).contains(&key) {
                        let value = from_order_key(key).to_bits();
                        assert!(held.contains(&value), "{x:e}: {value:#010x}");
                    }
                }
            }
        }
    }

    #[test]
    fn ordinals() {
        let names: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 61, 101, 111, 112]
            .map(ordinal)
            .to_vec();
        assert_eq!(
            names,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "23rd", "61st",
                "101st", "111th", "112th"
            ]
        );
        assert_eq!(ProbeSet::Halves { stride: 61 }.name(), "every 61st half");
    }

    #[test]
    fn cycled_pixels_hold_every_value_in_every_channel() {
        for n in [1usize, 2, 3, 7, 130, 7919, 7920, 20_000] {
            let values: Vec<f32> = (0..n).map(|i| i as f32).collect();
            for channels in [3usize, 4] {
                let pixels = to_pixels_cycled(&values, channels);
                assert_eq!(pixels.len(), n * channels);
                for c in 0..channels {
                    let mut seen: Vec<f32> =
                        pixels.iter().skip(c).step_by(channels).copied().collect();
                    seen.sort_by(f32::total_cmp);
                    assert_eq!(
                        bits(&seen),
                        bits(&values),
                        "n {n}, channel {c} of {channels}"
                    );
                }
            }
        }
        let values = specials();
        assert_eq!(
            bits(&to_rgba_cycled(&values)),
            bits(&to_pixels_cycled(&values, 4))
        );
    }

    #[test]
    fn nan_buffers_cover_every_length() {
        let buffers = nan_buffers(24);
        assert_eq!(buffers.len(), 48);
        let q = NAN_PATTERNS;
        for (k, (name, pixels)) in buffers.iter().enumerate() {
            let n = k / 2 + 1;
            assert_eq!(pixels.len(), 4 * n, "{name}");
            for i in 0..n {
                let px = &pixels[4 * i..4 * i + 4];
                assert_eq!(
                    bits(&px[..3]),
                    vec![q[i % 4], q[(i + 1) % 4], q[(i + 2) % 4]]
                );
                let alpha = if k % 2 == 0 {
                    0.25f32.to_bits()
                } else {
                    q[(i + 3) % 4]
                };
                assert_eq!(px[3].to_bits(), alpha, "{name}");
            }
        }
        assert_eq!(buffers[0].0, "nan rgb, 1 pixels");
        assert_eq!(buffers[47].0, "nan rgba, 24 pixels");
    }

    #[test]
    fn the_f32_sweep_holds_every_bit_pattern_once() {
        let pixels = 1u64 << 20;
        let first = all_f32_chunk(0, pixels);
        assert_eq!(first.len() as u64, pixels * 4);
        // Pixel 0 holds 0, 1, 2, 3; pixel 1 holds 4..=7 rotated by one: 5, 6, 7, 4.
        assert_eq!(bits(&first[..8]), vec![0, 1, 2, 3, 5, 6, 7, 4]);
        // Each chunk holds its patterns once, and each channel every low-bit value.
        let mut seen = bits(&first);
        let low_bits: Vec<Vec<u32>> = (0..4)
            .map(|c| {
                let mut v: Vec<u32> = seen.iter().skip(c).step_by(4).map(|b| b % 4).collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        assert!(low_bits.iter().all(|v| *v == vec![0, 1, 2, 3]));
        seen.sort_unstable();
        assert!(seen.iter().enumerate().all(|(i, &b)| b == i as u32));
        let last = all_f32_chunk(ALL_F32_PIXELS / pixels - 1, pixels);
        let mut last = bits(&last);
        last.sort_unstable();
        assert_eq!(last.first(), Some(&(u32::MAX - (pixels * 4 - 1) as u32)));
        assert_eq!(last.last(), Some(&u32::MAX));
    }

    #[test]
    fn probe_sets_name_and_assemble_their_buffers() {
        let set = ProbeSet::Neighbourhoods {
            points: vec![1.0],
            ulps: 3,
        };
        let buffers = set.rgba_buffers();
        assert_eq!(buffers.len(), 1);
        assert_eq!(buffers[0].0, "neighbourhoods of 3 ulp");
        assert_eq!(buffers[0].1.len(), 7 * 4);
        let empty = ProbeSet::Neighbourhoods {
            points: Vec::new(),
            ulps: 3,
        };
        assert!(empty.rgba_buffers().is_empty());
        assert_eq!(
            ProbeSet::NanBuffers { max_pixels: 3 }.rgba_buffers().len(),
            6
        );
        assert_eq!(ProbeSet::Halves { stride: 1 }.name(), "all halves");
    }

    /// The probe values are the same on every platform: these digests are checked on Windows
    /// and on Rocky Linux 9. They pin the generators' definitions (inputs to the battery), not
    /// any output of the port. Changing a probe set on purpose means updating its digest in the
    /// same commit and saying why.
    #[test]
    fn probe_values_are_identical_on_every_platform() {
        let mut ranges: Vec<(RandomRange, usize)> =
            RandomRange::ALL.iter().map(|&r| (r, 4096)).collect();
        ranges.push((RandomRange::Unit, 1));
        let digests = [
            ("all halves", digest(&all_half_values())),
            ("every 61st half", digest(&half_values_every(61))),
            ("specials", digest(&specials())),
            ("random", digest(&random(0x0b47_7e27, &ranges))),
            (
                "neighbourhoods",
                digest(&neighbourhoods(&[0.0, 1.0, -0.18, 1e-40, f32::MAX], 5)),
            ),
        ];
        let expected = [
            ("all halves", 0xbca4_a3b3_75d9_2669),
            ("every 61st half", 0x790f_f6d5_11dd_1e9f),
            ("specials", 0xd36e_7d82_fe87_5fc3),
            ("random", 0x0e44_d267_2a56_3dc6),
            ("neighbourhoods", 0x1636_2655_412e_6ab6),
        ];
        assert_eq!(digests, expected, "{digests:#018x?}");
    }
}
