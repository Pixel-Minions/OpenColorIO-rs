// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from dragonbox (Copyright 2020-2024 Junekey Jeon), as yaml-cpp 0.9.0 vendors it in
// src/contrib/dragonbox.h, under the Apache License 2.0 with LLVM exceptions; see
// THIRD-PARTY-NOTICES.md.

//! Port of `jkj::dragonbox::to_decimal` as yaml-cpp 0.9.0 vendors it
//! (`src/contrib/dragonbox.h` @ yaml-cpp-0.9.0, an adjusted copy of
//! <https://github.com/jk-jeon/dragonbox> wrapped in the `YAML` namespace). yaml-cpp's
//! `FpToString` (`src/fptostring.cpp:93`) calls `to_decimal(v)` with no policy, so only the
//! default policies are ported (`to_decimal_policy_holder`, dragonbox.h:4068-4081):
//!
//! - `policy::sign::return_sign`: the result carries the sign;
//! - `policy::trailing_zero::remove`: the significand has no trailing decimal zeros;
//! - `policy::decimal_to_binary_rounding::nearest_to_even`: the interval of decimals that
//!   read back as the input is the one of round-to-nearest, ties to even (closed when the
//!   binary significand is even);
//! - `policy::binary_to_decimal_rounding::to_even`: between two equally close shortest
//!   decimals, the even significand;
//! - `policy::cache::full`: the full tables of cached powers of ten, copied verbatim;
//! - `policy::preferred_integer_types::match`: the remainder is the carrier type and the
//!   exponents and shift amounts are `int`.
//!
//! The binary32 path is `to_decimal(float)` (carrier `uint32_t`, cache entries of 64 bits)
//! and the binary64 path `to_decimal(double)` (carrier `uint64_t`, cache entries of 128
//! bits). The other policies, the compressed cache, the directed roundings and
//! `to_decimal_ex` are not reached and not ported.
//!
//! Unsigned C++ arithmetic wraps; the port writes `wrapping_*` wherever upstream computes in
//! an unsigned type, so no operation can panic where C++ would wrap. Upstream's wide
//! multiplications have several implementations (`__int128`, `_umul128`, portable 32-bit
//! halves) that give the same integer; the port uses `u128`.

/// The result of [`to_decimal_f32`] and [`to_decimal_f64`]: `significand * 10^exponent`,
/// negative when `is_negative`.
///
/// Port of `decimal_fp<SignificandType, ExponentType, true, false>` (dragonbox.h:1273-1278),
/// with `ExponentType` the `int` of `preferred_integer_types::match`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalFp<T> {
    /// `significand`.
    pub significand: T,
    /// `exponent`.
    pub exponent: i32,
    /// `is_negative`.
    pub is_negative: bool,
}

/// The shortest decimal that reads back as `x`, the closest one to `x` among those, with
/// trailing zeros removed. `x` must be finite and not zero.
///
/// Port of `jkj::dragonbox::to_decimal(float)` (dragonbox.h:4150-4164) with the default
/// policies (see the module).
pub fn to_decimal_f32(x: f32) -> DecimalFp<u32> {
    binary32::to_decimal(x.to_bits())
}

/// The shortest decimal that reads back as `x`, the closest one to `x` among those, with
/// trailing zeros removed. `x` must be finite and not zero.
///
/// Port of `jkj::dragonbox::to_decimal(double)` (dragonbox.h:4150-4164) with the default
/// policies (see the module).
pub fn to_decimal_f64(x: f64) -> DecimalFp<u64> {
    binary64::to_decimal(x.to_bits())
}

// ---------------------------------------------------------------------------------------------
// Constexpr utilities (dragonbox.h:863-900).
// ---------------------------------------------------------------------------------------------

/// Port of `detail::compute_power<k>(a)` (dragonbox.h:867-881), in `u64`.
const fn compute_power(k: i32, a: u64) -> u64 {
    assert!(k >= 0);
    let mut p: u64 = 1;
    let mut i = 0;
    while i < k {
        p = p.wrapping_mul(a);
        i += 1;
    }
    p
}

/// Port of `detail::count_factors<a>(n)` (dragonbox.h:883-898).
const fn count_factors(a: u64, mut n: u64) -> i32 {
    assert!(a > 1);
    let mut c = 0;
    while n.is_multiple_of(a) {
        n /= a;
        c += 1;
    }
    c
}

// ---------------------------------------------------------------------------------------------
// Fast log computations (dragonbox.h:902-1146).
// ---------------------------------------------------------------------------------------------

/// Port of `log::floor_log2(n)` (dragonbox.h:909-921).
const fn floor_log2(mut n: u64) -> i32 {
    let mut count = -1;
    while n != 0 {
        count += 1;
        n >>= 1;
    }
    count
}

/// One tier of a log approximation: `(e * multiply - subtract) >> shift` is exact for
/// `min_exponent <= e <= max_exponent`.
///
/// Port of the `*_info<tier>` structures (dragonbox.h:974-1144).
struct LogInfo {
    multiply: i32,
    subtract: i32,
    shift: u32,
    min_exponent: i32,
    max_exponent: i32,
}

/// The first tier that covers `[min_exponent, max_exponent]`, applied to `e`.
///
/// Port of `log::compute_impl` and `log::is_in_range` (dragonbox.h:923-972): a range no tier
/// covers is a compile error upstream and a panic in constant evaluation here. Each function's
/// default `ReturnType` (`int_fast8_t`, `int_fast16_t`, `int_fast32_t`) holds every value of
/// the ranges the algorithm uses, so the port returns `i32`. The products stay below 2^31 in
/// every tier's range, so `int_fast32_t` being 64-bit on Linux changes nothing.
const fn log_compute(tiers: &[LogInfo], min_exponent: i32, max_exponent: i32, e: i32) -> i32 {
    let mut tier = 0;
    while tier < tiers.len() {
        let info = &tiers[tier];
        if min_exponent >= info.min_exponent && max_exponent <= info.max_exponent {
            debug_assert!(min_exponent <= e && e <= max_exponent);
            assert!(info.multiply >= 0 && info.subtract >= 0);
            return (e * info.multiply - info.subtract) >> info.shift;
        }
        tier += 1;
    }
    panic!("jkj::dragonbox: exponent range is too wide");
}

/// Port of `log::floor_log10_pow2_info<0..2>` (dragonbox.h:974-1004).
const FLOOR_LOG10_POW2: [LogInfo; 3] = [
    LogInfo {
        multiply: 77,
        subtract: 0,
        shift: 8,
        min_exponent: -102,
        max_exponent: 102,
    },
    LogInfo {
        multiply: 1233,
        subtract: 0,
        shift: 12,
        min_exponent: -425,
        max_exponent: 425,
    },
    LogInfo {
        multiply: 315653,
        subtract: 0,
        shift: 20,
        min_exponent: -2620,
        max_exponent: 2620,
    },
];

/// Port of `log::floor_log10_pow2<min_exponent, max_exponent>(e)` (dragonbox.h:1006-1014).
const fn floor_log10_pow2(min_exponent: i32, max_exponent: i32, e: i32) -> i32 {
    log_compute(&FLOOR_LOG10_POW2, min_exponent, max_exponent, e)
}

/// `floor_log10_pow2(e)` with its default range `[-2620, 2620]` (dragonbox.h:1006-1007).
const fn floor_log10_pow2_default(e: i32) -> i32 {
    floor_log10_pow2(-2620, 2620, e)
}

/// Port of `log::floor_log2_pow10_info<0..2>` (dragonbox.h:1016-1045).
const FLOOR_LOG2_POW10: [LogInfo; 3] = [
    LogInfo {
        multiply: 53,
        subtract: 0,
        shift: 4,
        min_exponent: -15,
        max_exponent: 18,
    },
    LogInfo {
        multiply: 1701,
        subtract: 0,
        shift: 9,
        min_exponent: -58,
        max_exponent: 58,
    },
    LogInfo {
        multiply: 1741647,
        subtract: 0,
        shift: 19,
        min_exponent: -1233,
        max_exponent: 1233,
    },
];

/// Port of `log::floor_log2_pow10<min_exponent, max_exponent>(e)` (dragonbox.h:1047-1055).
const fn floor_log2_pow10(min_exponent: i32, max_exponent: i32, e: i32) -> i32 {
    log_compute(&FLOOR_LOG2_POW10, min_exponent, max_exponent, e)
}

/// Port of `log::floor_log10_pow2_minus_log10_4_over_3_info<0..2>` (dragonbox.h:1057-1087).
const FLOOR_LOG10_POW2_MINUS_LOG10_4_OVER_3: [LogInfo; 3] = [
    LogInfo {
        multiply: 77,
        subtract: 31,
        shift: 8,
        min_exponent: -75,
        max_exponent: 129,
    },
    LogInfo {
        multiply: 19728,
        subtract: 8241,
        shift: 16,
        min_exponent: -424,
        max_exponent: 315,
    },
    LogInfo {
        multiply: 631305,
        subtract: 261663,
        shift: 21,
        min_exponent: -2985,
        max_exponent: 2936,
    },
];

/// Port of `log::floor_log10_pow2_minus_log10_4_over_3<min_exponent, max_exponent>(e)`
/// (dragonbox.h:1089-1098).
const fn floor_log10_pow2_minus_log10_4_over_3(
    min_exponent: i32,
    max_exponent: i32,
    e: i32,
) -> i32 {
    log_compute(
        &FLOOR_LOG10_POW2_MINUS_LOG10_4_OVER_3,
        min_exponent,
        max_exponent,
        e,
    )
}

/// `floor_log10_pow2_minus_log10_4_over_3(e)` with its default range `[-2985, 2936]`
/// (dragonbox.h:1089-1090).
const fn floor_log10_pow2_minus_log10_4_over_3_default(e: i32) -> i32 {
    floor_log10_pow2_minus_log10_4_over_3(-2985, 2936, e)
}

/// Port of `log::floor_log5_pow2_info<0>` (dragonbox.h:1100-1109).
const FLOOR_LOG5_POW2: [LogInfo; 1] = [LogInfo {
    multiply: 225799,
    subtract: 0,
    shift: 19,
    min_exponent: -1831,
    max_exponent: 1831,
}];

/// Port of `log::floor_log5_pow2(e)` with its default range (dragonbox.h:1111-1119).
const fn floor_log5_pow2(e: i32) -> i32 {
    log_compute(&FLOOR_LOG5_POW2, -1831, 1831, e)
}

/// Port of `log::floor_log5_pow2_minus_log5_3_info<0>` (dragonbox.h:1121-1130).
const FLOOR_LOG5_POW2_MINUS_LOG5_3: [LogInfo; 1] = [LogInfo {
    multiply: 451597,
    subtract: 715764,
    shift: 20,
    min_exponent: -3543,
    max_exponent: 2427,
}];

/// Port of `log::floor_log5_pow2_minus_log5_3(e)` with its default range
/// (dragonbox.h:1132-1141).
const fn floor_log5_pow2_minus_log5_3(e: i32) -> i32 {
    log_compute(&FLOOR_LOG5_POW2_MINUS_LOG5_3, -3543, 2427, e)
}

// ---------------------------------------------------------------------------------------------
// Bit operations and wide unsigned arithmetic (dragonbox.h:595-861).
// ---------------------------------------------------------------------------------------------

/// Port of `bits::rotr<32>(n, r)` (dragonbox.h:598-605).
const fn rotr32(n: u32, r: u32) -> u32 {
    let r = r & (32 - 1);
    (n >> r) | (n << ((32 - r) & (32 - 1)))
}

/// Port of `bits::rotr<64>(n, r)` (dragonbox.h:598-605).
const fn rotr64(n: u64, r: u32) -> u64 {
    let r = r & (64 - 1);
    (n >> r) | (n << ((64 - r) & (64 - 1)))
}

/// A 128-bit unsigned integer as two halves.
///
/// Port of `wuint::uint128` (dragonbox.h:629-719).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Uint128 {
    high: u64,
    low: u64,
}

impl Uint128 {
    /// Port of the constructor `uint128(high, low)`.
    const fn new(high: u64, low: u64) -> Self {
        Self { high, low }
    }

    /// Port of `uint128::operator+=(uint_least64_t)` (dragonbox.h:641-717, its
    /// `generic_impl`; the intrinsic paths give the same sum).
    fn add_assign(&mut self, n: u64) {
        let sum = self.low.wrapping_add(n);
        self.high = self.high.wrapping_add(if sum < self.low { 1 } else { 0 });
        self.low = sum;
    }
}

/// Port of `wuint::umul64(x, y)` (dragonbox.h:722-730).
const fn umul64(x: u32, y: u32) -> u64 {
    x as u64 * y as u64
}

/// Port of `wuint::umul128(x, y)` (dragonbox.h:731-775).
const fn umul128(x: u64, y: u64) -> Uint128 {
    let result = x as u128 * y as u128;
    Uint128::new((result >> 64) as u64, result as u64)
}

/// Port of `wuint::umul128_upper64(x, y)` (dragonbox.h:777-818).
const fn umul128_upper64(x: u64, y: u64) -> u64 {
    ((x as u128 * y as u128) >> 64) as u64
}

/// Port of `wuint::umul192_upper128(x, y)` (dragonbox.h:820-828).
fn umul192_upper128(x: u64, y: Uint128) -> Uint128 {
    let mut r = umul128(x, y.high);
    r.add_assign(umul128_upper64(x, y.low));
    r
}

/// Port of `wuint::umul96_upper64(x, y)` (dragonbox.h:830-845; the 64-bit path).
const fn umul96_upper64(x: u32, y: u64) -> u64 {
    umul128_upper64((x as u64) << 32, y)
}

/// Port of `wuint::umul192_lower128(x, y)` (dragonbox.h:846-854).
const fn umul192_lower128(x: u64, y: Uint128) -> Uint128 {
    let high = x.wrapping_mul(y.high);
    let high_low = umul128(x, y.low);
    Uint128::new(high.wrapping_add(high_low.high), high_low.low)
}

/// Port of `wuint::umul96_lower64(x, y)` (dragonbox.h:856-860).
const fn umul96_lower64(x: u32, y: u64) -> u64 {
    (x as u64).wrapping_mul(y)
}

// ---------------------------------------------------------------------------------------------
// Division by powers of ten (dragonbox.h:1148-1258).
// ---------------------------------------------------------------------------------------------

/// `uint_fast32_t`, the type of `divide_by_pow10_info`'s magic numbers: `unsigned int` with
/// MSVC, `unsigned long` with glibc on x86-64. The values `check_divisibility_and_divide_by_pow10`
/// computes fit in 32 bits, so both give the same results.
#[cfg(windows)]
type UintFast32 = u32;
#[cfg(not(windows))]
type UintFast32 = u64;

/// `(magic_number, shift_amount)` of `div::divide_by_pow10_info<N, UInt>` for a `UInt` of 32
/// or 64 bits (dragonbox.h:1153-1184; the `uint_least8_t`/`uint_least16_t` specializations
/// aren't reached).
const fn divide_by_pow10_info(n: i32) -> (u64, u32) {
    match n {
        1 => (6554, 16),
        2 => (656, 16),
        _ => panic!("no divide_by_pow10_info"),
    }
}

/// Divides `n` by `10^big_n` when `n` is divisible, and says whether it was; `n` must be at
/// most `10^(big_n + 1)`.
///
/// Port of `div::check_divisibility_and_divide_by_pow10<N, UInt>(n)` (dragonbox.h:1186-1204).
// `UintFast32` is already `u64` on Linux.
#[allow(clippy::unnecessary_cast)]
fn check_divisibility_and_divide_by_pow10(big_n: i32, n: u64) -> (bool, u64) {
    debug_assert!(n <= compute_power(big_n + 1, 10));
    let (magic_number, shift_amount) = divide_by_pow10_info(big_n);
    let prod = n.wrapping_mul(magic_number) as UintFast32;
    let mask = ((1 as UintFast32) << shift_amount) - 1;
    let result = (prod & mask) < magic_number as UintFast32;
    (result, (prod >> shift_amount) as u64)
}

/// `n / 10^big_n` for a 32-bit `n` of at most `n_max`.
///
/// Port of `div::divide_by_pow10<N, uint_least32_t, n_max>(n)` (dragonbox.h:1218-1244).
const fn divide_by_pow10_u32(big_n: i32, n_max: u32, n: u32) -> u32 {
    if big_n == 1 && n_max <= 1073741828 {
        (umul64(n, 429496730) >> 32) as u32
    } else if big_n == 2 {
        (umul64(n, 1374389535) >> 37) as u32
    } else {
        n / compute_power(big_n, 10) as u32
    }
}

/// `n / 10^big_n` for a 64-bit `n` of at most `n_max`.
///
/// Port of `div::divide_by_pow10<N, uint_least64_t, n_max>(n)` (dragonbox.h:1218-1244).
const fn divide_by_pow10_u64(big_n: i32, n_max: u64, n: u64) -> u64 {
    if big_n == 1 && n_max <= 4611686018427387908 {
        umul128_upper64(n, 1844674407370955162)
    } else if big_n == 3 && n_max <= 15534100272597517998 {
        umul128_upper64(n, 4722366482869645214) >> 8
    } else {
        n / compute_power(big_n, 10)
    }
}

// ---------------------------------------------------------------------------------------------
// The main algorithm, shared by both formats (dragonbox.h:3170-3245, 3246-3496, 3797-3807).
// ---------------------------------------------------------------------------------------------

/// Defines `to_decimal` and `compute_nearest` for the format module it's invoked in, which
/// provides `Carrier`, the `ieee754_binary*` constants, `CACHE`/`CACHE_MIN_K`/`CACHE_MAX_K`,
/// the multiplication traits (`compute_mul`, `compute_delta`, `compute_mul_parity`, the three
/// shorter-interval functions), `divide_by_pow10` and `remove_trailing_zeros`.
macro_rules! dragonbox_impl {
    () => {
        /// `carrier_bits`: the carrier's width.
        const CARRIER_BITS: i32 = Carrier::BITS as i32;

        /// Port of `impl::kappa` (dragonbox.h:3185-3186).
        const KAPPA: i32 = floor_log10_pow2_default(CARRIER_BITS - SIGNIFICAND_BITS - 2) - 1;
        const _: () = assert!(KAPPA >= 1);
        const _: () = assert!(
            CARRIER_BITS >= SIGNIFICAND_BITS + 2 + floor_log2_pow10(-1233, 1233, KAPPA + 1)
        );

        /// Port of `impl::min_k` (dragonbox.h:3194-3196).
        const MIN_K: i32 = min(
            -floor_log10_pow2_minus_log10_4_over_3_default(MAX_EXPONENT - SIGNIFICAND_BITS),
            -floor_log10_pow2_default(MAX_EXPONENT - SIGNIFICAND_BITS) + KAPPA,
        );

        /// Port of `impl::max_k` (dragonbox.h:3200-3203).
        const MAX_K: i32 = max(
            -floor_log10_pow2_minus_log10_4_over_3_default(MIN_EXPONENT - SIGNIFICAND_BITS),
            -floor_log10_pow2_default(MIN_EXPONENT - SIGNIFICAND_BITS) + KAPPA,
        );

        /// `static_assert(min_k >= cache_holder_type::min_k && max_k <= ...)`
        /// (dragonbox.h:3251-3252).
        const _: () = assert!(MIN_K >= CACHE_MIN_K && MAX_K <= CACHE_MAX_K);

        /// Port of `impl::case_shorter_interval_left_endpoint_lower_threshold`
        /// (dragonbox.h:3205).
        const CASE_SHORTER_INTERVAL_LEFT_ENDPOINT_LOWER_THRESHOLD: i32 = 2;
        /// Port of `impl::case_shorter_interval_left_endpoint_upper_threshold`
        /// (dragonbox.h:3206-3211).
        const CASE_SHORTER_INTERVAL_LEFT_ENDPOINT_UPPER_THRESHOLD: i32 = 2 + floor_log2(
            compute_power(
                count_factors(5, (1u64 << (SIGNIFICAND_BITS + 2)) - 1) + 1,
                10,
            ) / 3,
        );
        /// Port of `impl::case_shorter_interval_right_endpoint_lower_threshold`
        /// (dragonbox.h:3213).
        const CASE_SHORTER_INTERVAL_RIGHT_ENDPOINT_LOWER_THRESHOLD: i32 = 0;
        /// Port of `impl::case_shorter_interval_right_endpoint_upper_threshold`
        /// (dragonbox.h:3214-3219).
        const CASE_SHORTER_INTERVAL_RIGHT_ENDPOINT_UPPER_THRESHOLD: i32 = 2 + floor_log2(
            compute_power(
                count_factors(5, (1u64 << (SIGNIFICAND_BITS + 1)) + 1) + 1,
                10,
            ) / 3,
        );

        /// Port of `impl::shorter_interval_tie_lower_threshold` (dragonbox.h:3221-3222).
        const SHORTER_INTERVAL_TIE_LOWER_THRESHOLD: i32 =
            -floor_log5_pow2_minus_log5_3(SIGNIFICAND_BITS + 4) - 2 - SIGNIFICAND_BITS;
        /// Port of `impl::shorter_interval_tie_upper_threshold` (dragonbox.h:3223-3224).
        const SHORTER_INTERVAL_TIE_UPPER_THRESHOLD: i32 =
            -floor_log5_pow2(SIGNIFICAND_BITS + 2) - 2 - SIGNIFICAND_BITS;

        /// Port of `impl::min` (dragonbox.h:3191).
        const fn min(x: i32, y: i32) -> i32 {
            if x < y { x } else { y }
        }

        /// Port of `impl::max` (dragonbox.h:3192).
        const fn max(x: i32, y: i32) -> i32 {
            if x > y { x } else { y }
        }

        /// Port of `ieee754_binary_traits::remove_exponent_bits` (dragonbox.h:398-401).
        const fn remove_exponent_bits(u: Carrier) -> Carrier {
            u & !((((1 as Carrier) << EXPONENT_BITS) - 1) << SIGNIFICAND_BITS)
        }

        /// Port of `ieee754_binary_traits::extract_exponent_bits` (dragonbox.h:385-388).
        const fn extract_exponent_bits(u: Carrier) -> i32 {
            ((u >> SIGNIFICAND_BITS) & (((1 as Carrier) << EXPONENT_BITS) - 1)) as i32
        }

        /// Port of `ieee754_binary_traits::remove_sign_bit_and_shift` (dragonbox.h:404-407).
        const fn remove_sign_bit_and_shift(u: Carrier) -> Carrier {
            (u << 1) & (((((1 as Carrier) << (TOTAL_BITS - 1)) - 1) << 1) | 1)
        }

        /// Port of `ieee754_binary_traits::is_positive` (dragonbox.h:431-433).
        const fn is_positive(u: Carrier) -> bool {
            u < ((1 as Carrier) << (SIGNIFICAND_BITS + EXPONENT_BITS))
        }

        /// Port of `ieee754_binary_traits::is_finite` and `is_nonzero` (dragonbox.h:425-438),
        /// for `to_decimal`'s assertion.
        const fn is_finite_nonzero(u: Carrier) -> bool {
            let finite = extract_exponent_bits(u) != (1 << EXPONENT_BITS) - 1;
            let nonzero = (u & (((1 as Carrier) << (SIGNIFICAND_BITS + EXPONENT_BITS)) - 1)) != 0;
            finite && nonzero
        }

        /// Port of `ieee754_binary_traits::has_even_significand_bits` (dragonbox.h:442-444).
        const fn has_even_significand_bits(u: Carrier) -> bool {
            u % 2 == 0
        }

        /// Port of `policy::cache::full_t::get_cache` (dragonbox.h:2821-2836).
        fn get_cache(k: i32) -> CacheEntry {
            debug_assert!((CACHE_MIN_K..=CACHE_MAX_K).contains(&k));
            CACHE[(k - CACHE_MIN_K) as usize]
        }

        /// Port of `policy::binary_to_decimal_rounding::to_even_t::prefer_round_down`
        /// (dragonbox.h:2779-2787).
        const fn prefer_round_down(significand: Carrier) -> bool {
            significand % 2 != 0
        }

        /// The result with the sign of `s`: port of `policy::sign::return_sign_t::handle_sign`
        /// (dragonbox.h:2336-2344).
        const fn handle_sign(
            s: Carrier,
            significand: Carrier,
            exponent: i32,
        ) -> DecimalFp<Carrier> {
            DecimalFp {
                significand,
                exponent,
                is_negative: !is_positive(s),
            }
        }

        /// Port of `policy::trailing_zero::remove_t::on_trailing_zeros` (dragonbox.h:2368-2381).
        fn on_trailing_zeros(mut significand: Carrier, mut exponent: i32) -> (Carrier, i32) {
            remove_trailing_zeros(&mut significand, &mut exponent);
            (significand, exponent)
        }

        /// Port of `policy::trailing_zero::remove_t::no_trailing_zeros` (dragonbox.h:2383-2388).
        const fn no_trailing_zeros(significand: Carrier, exponent: i32) -> (Carrier, i32) {
            (significand, exponent)
        }

        /// Port of `impl::is_right_endpoint_integer_shorter_interval` (dragonbox.h:3797-3801).
        const fn is_right_endpoint_integer_shorter_interval(binary_exponent: i32) -> bool {
            binary_exponent >= CASE_SHORTER_INTERVAL_RIGHT_ENDPOINT_LOWER_THRESHOLD
                && binary_exponent <= CASE_SHORTER_INTERVAL_RIGHT_ENDPOINT_UPPER_THRESHOLD
        }

        /// Port of `impl::is_left_endpoint_integer_shorter_interval` (dragonbox.h:3803-3807).
        const fn is_left_endpoint_integer_shorter_interval(binary_exponent: i32) -> bool {
            binary_exponent >= CASE_SHORTER_INTERVAL_LEFT_ENDPOINT_LOWER_THRESHOLD
                && binary_exponent <= CASE_SHORTER_INTERVAL_LEFT_ENDPOINT_UPPER_THRESHOLD
        }

        /// Port of `to_decimal(Float)` (dragonbox.h:4150-4164) and `to_decimal_ex`'s dispatch
        /// (4138-4148, 4088-4130) for the default policies: `x`'s bits, which must be finite
        /// and not zero (`assert` upstream, checked in debug builds here).
        pub(super) fn to_decimal(bits: Carrier) -> DecimalFp<Carrier> {
            let exponent_bits = extract_exponent_bits(bits);
            let s = remove_exponent_bits(bits);
            debug_assert!(is_finite_nonzero(bits));
            compute_nearest(s, exponent_bits)
        }

        /// Port of `impl::compute_nearest` (dragonbox.h:3246-3496) with `nearest_to_even`
        /// (its `normal_interval` is `symmetric_boundary{has_even_significand_bits}` and its
        /// `shorter_interval` is `closed`, dragonbox.h:2474-2497), `to_even`, `remove`,
        /// `return_sign`, `full` and `match`.
        fn compute_nearest(s: Carrier, exponent_bits: i32) -> DecimalFp<Carrier> {
            let mut two_fc = remove_sign_bit_and_shift(s);
            let mut binary_exponent = exponent_bits;

            // Is the input a normal number?
            if binary_exponent != 0 {
                binary_exponent += EXPONENT_BIAS - SIGNIFICAND_BITS;

                // Shorter interval case; proceed like Schubfach.
                if two_fc == 0 {
                    // `interval_type::closed`.
                    let include_left_endpoint = true;
                    let include_right_endpoint = true;

                    // Compute k and beta.
                    let minus_k = floor_log10_pow2_minus_log10_4_over_3(
                        MIN_EXPONENT - SIGNIFICAND_BITS,
                        MAX_EXPONENT - SIGNIFICAND_BITS,
                        binary_exponent,
                    );
                    let beta = binary_exponent + floor_log2_pow10(MIN_K, MAX_K, -minus_k);

                    // Compute xi and zi.
                    let cache = get_cache(-minus_k);

                    let mut xi = compute_left_endpoint_for_shorter_interval_case(cache, beta);
                    let mut zi = compute_right_endpoint_for_shorter_interval_case(cache, beta);

                    // If we don't accept the right endpoint and
                    // if the right endpoint is an integer, decrease it.
                    if !include_right_endpoint
                        && is_right_endpoint_integer_shorter_interval(binary_exponent)
                    {
                        zi = zi.wrapping_sub(1);
                    }
                    // If we don't accept the left endpoint or
                    // if the left endpoint is not an integer, increase it.
                    if !include_left_endpoint
                        || !is_left_endpoint_integer_shorter_interval(binary_exponent)
                    {
                        xi = xi.wrapping_add(1);
                    }

                    // Try bigger divisor.
                    let mut decimal_significand = divide_by_pow10(
                        1,
                        ((((1 as Carrier) << (SIGNIFICAND_BITS + 1)) + 1) / 3 + 1) * 20,
                        zi,
                    );

                    // If succeed, remove trailing zeros if necessary and return.
                    if decimal_significand.wrapping_mul(10) >= xi {
                        let (significand, exponent) =
                            on_trailing_zeros(decimal_significand, minus_k + 1);
                        return handle_sign(s, significand, exponent);
                    }

                    // Otherwise, compute the round-up of y.
                    decimal_significand = compute_round_up_for_shorter_interval_case(cache, beta);

                    // When tie occurs, choose one of them according to the rule.
                    if prefer_round_down(decimal_significand)
                        && binary_exponent >= SHORTER_INTERVAL_TIE_LOWER_THRESHOLD
                        && binary_exponent <= SHORTER_INTERVAL_TIE_UPPER_THRESHOLD
                    {
                        decimal_significand = decimal_significand.wrapping_sub(1);
                    } else if decimal_significand < xi {
                        decimal_significand = decimal_significand.wrapping_add(1);
                    }
                    let (significand, exponent) = no_trailing_zeros(decimal_significand, minus_k);
                    return handle_sign(s, significand, exponent);
                }

                // Normal interval case.
                two_fc |= (1 as Carrier) << (SIGNIFICAND_BITS + 1);
            }
            // Is the input a subnormal number?
            else {
                // Normal interval case.
                binary_exponent = MIN_EXPONENT - SIGNIFICAND_BITS;
            }

            //////////////////////////////////////////////////////////////////////
            // Step 1: Schubfach multiplier calculation.
            //////////////////////////////////////////////////////////////////////

            // `interval_type::symmetric_boundary{s.has_even_significand_bits()}`.
            let is_closed = has_even_significand_bits(s);
            let include_left_endpoint = is_closed;
            let include_right_endpoint = is_closed;

            // Compute k and beta.
            let minus_k = floor_log10_pow2(
                MIN_EXPONENT - SIGNIFICAND_BITS,
                MAX_EXPONENT - SIGNIFICAND_BITS,
                binary_exponent,
            ) - KAPPA;
            let cache = get_cache(-minus_k);
            let beta = binary_exponent + floor_log2_pow10(MIN_K, MAX_K, -minus_k);

            // Compute zi and deltai.
            // 10^kappa <= deltai < 10^(kappa + 1)
            let deltai = compute_delta(cache, beta) as Carrier;
            let (z_integer_part, z_is_integer) = compute_mul((two_fc | 1) << beta, cache);

            //////////////////////////////////////////////////////////////////////
            // Step 2: Try larger divisor; remove trailing zeros if necessary.
            //////////////////////////////////////////////////////////////////////

            let big_divisor = compute_power(KAPPA + 1, 10) as Carrier;
            let small_divisor = compute_power(KAPPA, 10) as Carrier;

            // Using an upper bound on zi, we might be able to optimize the division
            // better than the compiler; we are computing zi / big_divisor here.
            let mut decimal_significand = divide_by_pow10(
                KAPPA + 1,
                ((1 as Carrier) << (SIGNIFICAND_BITS + 1))
                    .wrapping_mul(big_divisor)
                    .wrapping_sub(1),
                z_integer_part,
            );
            let mut r = z_integer_part.wrapping_sub(big_divisor.wrapping_mul(decimal_significand));

            'step2: {
                if r < deltai {
                    // Exclude the right endpoint if necessary.
                    if (r | (!z_is_integer) as Carrier | include_right_endpoint as Carrier) == 0 {
                        decimal_significand = decimal_significand.wrapping_sub(1);
                        r = big_divisor;
                        break 'step2;
                    }
                } else if r > deltai {
                    break 'step2;
                } else {
                    // r == deltai; compare fractional parts.
                    let (x_parity, x_is_integer) =
                        compute_mul_parity(two_fc.wrapping_sub(1), cache, beta);

                    if !(x_parity | (x_is_integer & include_left_endpoint)) {
                        break 'step2;
                    }
                }

                // We may need to remove trailing zeros.
                let (significand, exponent) =
                    on_trailing_zeros(decimal_significand, minus_k + KAPPA + 1);
                return handle_sign(s, significand, exponent);
            }

            //////////////////////////////////////////////////////////////////////
            // Step 3: Find the significand with the smaller divisor.
            //////////////////////////////////////////////////////////////////////

            decimal_significand = decimal_significand.wrapping_mul(10);

            // delta is equal to 10^(kappa + elog10(2) - floor(elog10(2))), so dist cannot
            // be larger than r.
            let dist = r.wrapping_sub(deltai / 2).wrapping_add(small_divisor / 2);
            let approx_y_parity = ((dist ^ (small_divisor / 2)) & 1) != 0;

            // Is dist divisible by 10^kappa?
            let (divisible_by_small_divisor, dist) =
                check_divisibility_and_divide_by_pow10(KAPPA, dist as u64);
            let dist = dist as Carrier;

            // Add dist / 10^kappa to the significand.
            decimal_significand = decimal_significand.wrapping_add(dist);

            if divisible_by_small_divisor {
                // Check z^(f) >= epsilon^(f).
                // We have either yi == zi - epsiloni or yi == (zi - epsiloni) - 1,
                // where yi == zi - epsiloni if and only if z^(f) >= epsilon^(f).
                // Since there are only 2 possibilities, we only need to care about the
                // parity. Also, zi and r should have the same parity since the divisor
                // is an even number.
                let (y_parity, y_is_integer) = compute_mul_parity(two_fc, cache, beta);

                if y_parity != approx_y_parity {
                    decimal_significand = decimal_significand.wrapping_sub(1);
                } else {
                    // If z^(f) >= epsilon^(f), we might have a tie
                    // when z^(f) == epsilon^(f), or equivalently, when y is an integer.
                    // For tie-to-up case, we can just choose the upper one.
                    if prefer_round_down(decimal_significand) & y_is_integer {
                        decimal_significand = decimal_significand.wrapping_sub(1);
                    }
                }
            }
            let (significand, exponent) = no_trailing_zeros(decimal_significand, minus_k + KAPPA);
            handle_sign(s, significand, exponent)
        }
    };
}

/// IEEE-754 binary32: `to_decimal(float)`.
mod binary32 {
    use super::*;

    /// The bit carrier of `float` (`default_float_bit_carrier_conversion_traits`,
    /// dragonbox.h:454-487).
    type Carrier = u32;
    /// `cache_holder<ieee754_binary32>::cache_entry_type` (dragonbox.h:1341).
    type CacheEntry = u64;

    /// Port of `ieee754_binary32` (dragonbox.h:337-346).
    const TOTAL_BITS: i32 = 32;
    const SIGNIFICAND_BITS: i32 = 23;
    const EXPONENT_BITS: i32 = 8;
    const MIN_EXPONENT: i32 = -126;
    const MAX_EXPONENT: i32 = 127;
    const EXPONENT_BIAS: i32 = -127;

    /// `cache_holder<ieee754_binary32>::cache_bits` (dragonbox.h:1342).
    const CACHE_BITS: i32 = 64;

    dragonbox_impl!();

    /// Port of `remove_trailing_zeros_traits<remove_t, ieee754_binary32, uint_least32_t>`
    /// (dragonbox.h:2942-2972).
    fn remove_trailing_zeros(significand: &mut u32, exponent: &mut i32) {
        // See https://github.com/jk-jeon/rtz_benchmark.
        // The idea of branchless search below is by reddit users r/pigeon768 and
        // r/TheoreticalDumbass.

        let mut r = rotr32(significand.wrapping_mul(184254097), 4);
        let mut b = r < 429497;
        let mut s = usize::from(b);
        *significand = if b { r } else { *significand };

        r = rotr32(significand.wrapping_mul(42949673), 2);
        b = r < 42949673;
        s = s * 2 + usize::from(b);
        *significand = if b { r } else { *significand };

        r = rotr32(significand.wrapping_mul(1288490189), 1);
        b = r < 429496730;
        s = s * 2 + usize::from(b);
        *significand = if b { r } else { *significand };

        *exponent += s as i32;
    }

    /// `div::divide_by_pow10` for the binary32 carrier.
    const fn divide_by_pow10(big_n: i32, n_max: u32, n: u32) -> u32 {
        divide_by_pow10_u32(big_n, n_max, n)
    }

    /// Port of `multiplication_traits<binary32>::compute_mul` (dragonbox.h:3060-3065).
    const fn compute_mul(u: u32, cache: u64) -> (u32, bool) {
        let r = umul96_upper64(u, cache);
        ((r >> 32) as u32, r as u32 == 0)
    }

    /// Port of `multiplication_traits<binary32>::compute_delta` (dragonbox.h:3067-3071).
    const fn compute_delta(cache: u64, beta: i32) -> u64 {
        cache >> (CACHE_BITS - 1 - beta)
    }

    /// Port of `multiplication_traits<binary32>::compute_mul_parity` (dragonbox.h:3073-3082).
    fn compute_mul_parity(two_f: u32, cache: u64, beta: i32) -> (bool, bool) {
        debug_assert!(beta >= 1);
        debug_assert!(beta <= 32);

        let r = umul96_lower64(two_f, cache);
        (
            ((r >> (64 - beta)) & 1) != 0,
            (0xffffffff & (r >> (32 - beta))) == 0,
        )
    }

    /// Port of `multiplication_traits<binary32>::compute_left_endpoint_for_shorter_interval_case`
    /// (dragonbox.h:3084-3090).
    const fn compute_left_endpoint_for_shorter_interval_case(cache: u64, beta: i32) -> u32 {
        (cache.wrapping_sub(cache >> (SIGNIFICAND_BITS + 2))
            >> (CACHE_BITS - SIGNIFICAND_BITS - 1 - beta)) as u32
    }

    /// Port of `multiplication_traits<binary32>::compute_right_endpoint_for_shorter_interval_case`
    /// (dragonbox.h:3092-3098).
    const fn compute_right_endpoint_for_shorter_interval_case(cache: u64, beta: i32) -> u32 {
        (cache.wrapping_add(cache >> (SIGNIFICAND_BITS + 1))
            >> (CACHE_BITS - SIGNIFICAND_BITS - 1 - beta)) as u32
    }

    /// Port of `multiplication_traits<binary32>::compute_round_up_for_shorter_interval_case`
    /// (dragonbox.h:3100-3107).
    const fn compute_round_up_for_shorter_interval_case(cache: u64, beta: i32) -> u32 {
        ((cache >> (CACHE_BITS - SIGNIFICAND_BITS - 2 - beta)) as u32).wrapping_add(1) / 2
    }

    /// Port of `cache_holder<ieee754_binary32>::min_k` (dragonbox.h:1343).
    const CACHE_MIN_K: i32 = -31;
    /// Port of `cache_holder<ieee754_binary32>::max_k` (dragonbox.h:1344).
    const CACHE_MAX_K: i32 = 46;
    const _: () = assert!(CACHE.len() == (CACHE_MAX_K - CACHE_MIN_K + 1) as usize);

    /// Port of `cache_holder<ieee754_binary32>::cache` (dragonbox.h:1345-1385), copied
    /// verbatim: `ceil(10^k * 2^(...))` normalized to 64 bits, for `k` from `min_k`.
    pub(super) static CACHE: [u64; 78] = [
        0x81ceb32c4b43fcf5,
        0xa2425ff75e14fc32,
        0xcad2f7f5359a3b3f,
        0xfd87b5f28300ca0e,
        0x9e74d1b791e07e49,
        0xc612062576589ddb,
        0xf79687aed3eec552,
        0x9abe14cd44753b53,
        0xc16d9a0095928a28,
        0xf1c90080baf72cb2,
        0x971da05074da7bef,
        0xbce5086492111aeb,
        0xec1e4a7db69561a6,
        0x9392ee8e921d5d08,
        0xb877aa3236a4b44a,
        0xe69594bec44de15c,
        0x901d7cf73ab0acda,
        0xb424dc35095cd810,
        0xe12e13424bb40e14,
        0x8cbccc096f5088cc,
        0xafebff0bcb24aaff,
        0xdbe6fecebdedd5bf,
        0x89705f4136b4a598,
        0xabcc77118461cefd,
        0xd6bf94d5e57a42bd,
        0x8637bd05af6c69b6,
        0xa7c5ac471b478424,
        0xd1b71758e219652c,
        0x83126e978d4fdf3c,
        0xa3d70a3d70a3d70b,
        0xcccccccccccccccd,
        0x8000000000000000,
        0xa000000000000000,
        0xc800000000000000,
        0xfa00000000000000,
        0x9c40000000000000,
        0xc350000000000000,
        0xf424000000000000,
        0x9896800000000000,
        0xbebc200000000000,
        0xee6b280000000000,
        0x9502f90000000000,
        0xba43b74000000000,
        0xe8d4a51000000000,
        0x9184e72a00000000,
        0xb5e620f480000000,
        0xe35fa931a0000000,
        0x8e1bc9bf04000000,
        0xb1a2bc2ec5000000,
        0xde0b6b3a76400000,
        0x8ac7230489e80000,
        0xad78ebc5ac620000,
        0xd8d726b7177a8000,
        0x878678326eac9000,
        0xa968163f0a57b400,
        0xd3c21bcecceda100,
        0x84595161401484a0,
        0xa56fa5b99019a5c8,
        0xcecb8f27f4200f3a,
        0x813f3978f8940985,
        0xa18f07d736b90be6,
        0xc9f2c9cd04674edf,
        0xfc6f7c4045812297,
        0x9dc5ada82b70b59e,
        0xc5371912364ce306,
        0xf684df56c3e01bc7,
        0x9a130b963a6c115d,
        0xc097ce7bc90715b4,
        0xf0bdc21abb48db21,
        0x96769950b50d88f5,
        0xbc143fa4e250eb32,
        0xeb194f8e1ae525fe,
        0x92efd1b8d0cf37bf,
        0xb7abc627050305ae,
        0xe596b7b0c643c71a,
        0x8f7e32ce7bea5c70,
        0xb35dbf821ae4f38c,
        0xe0352f62a19e306f,
    ];
}

/// IEEE-754 binary64: `to_decimal(double)`.
mod binary64 {
    use super::*;

    /// The bit carrier of `double` (`default_float_bit_carrier_conversion_traits`,
    /// dragonbox.h:454-487).
    type Carrier = u64;
    /// `cache_holder<ieee754_binary64>::cache_entry_type` (dragonbox.h:1397).
    type CacheEntry = Uint128;

    /// Port of `ieee754_binary64` (dragonbox.h:347-356).
    const TOTAL_BITS: i32 = 64;
    const SIGNIFICAND_BITS: i32 = 52;
    const EXPONENT_BITS: i32 = 11;
    const MIN_EXPONENT: i32 = -1022;
    const MAX_EXPONENT: i32 = 1023;
    const EXPONENT_BIAS: i32 = -1023;

    dragonbox_impl!();

    /// Port of `remove_trailing_zeros_traits<remove_t, ieee754_binary64, uint_least64_t>`
    /// (dragonbox.h:2974-3011).
    fn remove_trailing_zeros(significand: &mut u64, exponent: &mut i32) {
        // See https://github.com/jk-jeon/rtz_benchmark.
        // The idea of branchless search below is by reddit users r/pigeon768 and
        // r/TheoreticalDumbass.

        let mut r = rotr64(significand.wrapping_mul(28999941890838049), 8);
        let mut b = r < 184467440738;
        let mut s = usize::from(b);
        *significand = if b { r } else { *significand };

        r = rotr64(significand.wrapping_mul(182622766329724561), 4);
        b = r < 1844674407370956;
        s = s * 2 + usize::from(b);
        *significand = if b { r } else { *significand };

        r = rotr64(significand.wrapping_mul(10330176681277348905), 2);
        b = r < 184467440737095517;
        s = s * 2 + usize::from(b);
        *significand = if b { r } else { *significand };

        // `UINT32_C(14757395258967641293)` upstream: the literal doesn't fit 32 bits, so it
        // is an unsigned 64-bit constant.
        r = rotr64(significand.wrapping_mul(14757395258967641293), 1);
        b = r < 1844674407370955162;
        s = s * 2 + usize::from(b);
        *significand = if b { r } else { *significand };

        *exponent += s as i32;
    }

    /// `div::divide_by_pow10` for the binary64 carrier.
    const fn divide_by_pow10(big_n: i32, n_max: u64, n: u64) -> u64 {
        divide_by_pow10_u64(big_n, n_max, n)
    }

    /// Port of `multiplication_traits<binary64>::compute_mul` (dragonbox.h:3117-3122).
    fn compute_mul(u: u64, cache: Uint128) -> (u64, bool) {
        let r = umul192_upper128(u, cache);
        (r.high, r.low == 0)
    }

    /// Port of `multiplication_traits<binary64>::compute_delta` (dragonbox.h:3124-3129).
    const fn compute_delta(cache: Uint128, beta: i32) -> u64 {
        cache.high >> (TOTAL_BITS - 1 - beta)
    }

    /// Port of `multiplication_traits<binary64>::compute_mul_parity` (dragonbox.h:3131-3141).
    fn compute_mul_parity(two_f: u64, cache: Uint128, beta: i32) -> (bool, bool) {
        debug_assert!(beta >= 1);
        debug_assert!(beta < 64);

        let r = umul192_lower128(two_f, cache);
        // Upstream masks `r.high() << beta` with `UINT64_C(0xffffffffffffffff)` for
        // `uint_least64_t` wider than 64 bits; on `u64` the mask is the identity.
        (
            ((r.high >> (64 - beta)) & 1) != 0,
            ((r.high << beta) | (r.low >> (64 - beta))) == 0,
        )
    }

    /// Port of `multiplication_traits<binary64>::compute_left_endpoint_for_shorter_interval_case`
    /// (dragonbox.h:3143-3149).
    const fn compute_left_endpoint_for_shorter_interval_case(cache: Uint128, beta: i32) -> u64 {
        cache
            .high
            .wrapping_sub(cache.high >> (SIGNIFICAND_BITS + 2))
            >> (TOTAL_BITS - SIGNIFICAND_BITS - 1 - beta)
    }

    /// Port of `multiplication_traits<binary64>::compute_right_endpoint_for_shorter_interval_case`
    /// (dragonbox.h:3151-3157).
    const fn compute_right_endpoint_for_shorter_interval_case(cache: Uint128, beta: i32) -> u64 {
        cache
            .high
            .wrapping_add(cache.high >> (SIGNIFICAND_BITS + 1))
            >> (TOTAL_BITS - SIGNIFICAND_BITS - 1 - beta)
    }

    /// Port of `multiplication_traits<binary64>::compute_round_up_for_shorter_interval_case`
    /// (dragonbox.h:3159-3165).
    const fn compute_round_up_for_shorter_interval_case(cache: Uint128, beta: i32) -> u64 {
        (cache.high >> (TOTAL_BITS - SIGNIFICAND_BITS - 2 - beta)).wrapping_add(1) / 2
    }

    /// Port of `cache_holder<ieee754_binary64>::min_k` (dragonbox.h:1399).
    const CACHE_MIN_K: i32 = -292;
    /// Port of `cache_holder<ieee754_binary64>::max_k` (dragonbox.h:1400).
    const CACHE_MAX_K: i32 = 326;
    const _: () = assert!(CACHE.len() == (CACHE_MAX_K - CACHE_MIN_K + 1) as usize);

    /// Port of `cache_holder<ieee754_binary64>::cache` (dragonbox.h:1401-2021), copied
    /// verbatim as `{high, low}` pairs, for `k` from `min_k`.
    pub(super) static CACHE: [Uint128; 619] = [
        Uint128::new(0xff77b1fcbebcdc4f, 0x25e8e89c13bb0f7b),
        Uint128::new(0x9faacf3df73609b1, 0x77b191618c54e9ad),
        Uint128::new(0xc795830d75038c1d, 0xd59df5b9ef6a2418),
        Uint128::new(0xf97ae3d0d2446f25, 0x4b0573286b44ad1e),
        Uint128::new(0x9becce62836ac577, 0x4ee367f9430aec33),
        Uint128::new(0xc2e801fb244576d5, 0x229c41f793cda740),
        Uint128::new(0xf3a20279ed56d48a, 0x6b43527578c11110),
        Uint128::new(0x9845418c345644d6, 0x830a13896b78aaaa),
        Uint128::new(0xbe5691ef416bd60c, 0x23cc986bc656d554),
        Uint128::new(0xedec366b11c6cb8f, 0x2cbfbe86b7ec8aa9),
        Uint128::new(0x94b3a202eb1c3f39, 0x7bf7d71432f3d6aa),
        Uint128::new(0xb9e08a83a5e34f07, 0xdaf5ccd93fb0cc54),
        Uint128::new(0xe858ad248f5c22c9, 0xd1b3400f8f9cff69),
        Uint128::new(0x91376c36d99995be, 0x23100809b9c21fa2),
        Uint128::new(0xb58547448ffffb2d, 0xabd40a0c2832a78b),
        Uint128::new(0xe2e69915b3fff9f9, 0x16c90c8f323f516d),
        Uint128::new(0x8dd01fad907ffc3b, 0xae3da7d97f6792e4),
        Uint128::new(0xb1442798f49ffb4a, 0x99cd11cfdf41779d),
        Uint128::new(0xdd95317f31c7fa1d, 0x40405643d711d584),
        Uint128::new(0x8a7d3eef7f1cfc52, 0x482835ea666b2573),
        Uint128::new(0xad1c8eab5ee43b66, 0xda3243650005eed0),
        Uint128::new(0xd863b256369d4a40, 0x90bed43e40076a83),
        Uint128::new(0x873e4f75e2224e68, 0x5a7744a6e804a292),
        Uint128::new(0xa90de3535aaae202, 0x711515d0a205cb37),
        Uint128::new(0xd3515c2831559a83, 0x0d5a5b44ca873e04),
        Uint128::new(0x8412d9991ed58091, 0xe858790afe9486c3),
        Uint128::new(0xa5178fff668ae0b6, 0x626e974dbe39a873),
        Uint128::new(0xce5d73ff402d98e3, 0xfb0a3d212dc81290),
        Uint128::new(0x80fa687f881c7f8e, 0x7ce66634bc9d0b9a),
        Uint128::new(0xa139029f6a239f72, 0x1c1fffc1ebc44e81),
        Uint128::new(0xc987434744ac874e, 0xa327ffb266b56221),
        Uint128::new(0xfbe9141915d7a922, 0x4bf1ff9f0062baa9),
        Uint128::new(0x9d71ac8fada6c9b5, 0x6f773fc3603db4aa),
        Uint128::new(0xc4ce17b399107c22, 0xcb550fb4384d21d4),
        Uint128::new(0xf6019da07f549b2b, 0x7e2a53a146606a49),
        Uint128::new(0x99c102844f94e0fb, 0x2eda7444cbfc426e),
        Uint128::new(0xc0314325637a1939, 0xfa911155fefb5309),
        Uint128::new(0xf03d93eebc589f88, 0x793555ab7eba27cb),
        Uint128::new(0x96267c7535b763b5, 0x4bc1558b2f3458df),
        Uint128::new(0xbbb01b9283253ca2, 0x9eb1aaedfb016f17),
        Uint128::new(0xea9c227723ee8bcb, 0x465e15a979c1cadd),
        Uint128::new(0x92a1958a7675175f, 0x0bfacd89ec191eca),
        Uint128::new(0xb749faed14125d36, 0xcef980ec671f667c),
        Uint128::new(0xe51c79a85916f484, 0x82b7e12780e7401b),
        Uint128::new(0x8f31cc0937ae58d2, 0xd1b2ecb8b0908811),
        Uint128::new(0xb2fe3f0b8599ef07, 0x861fa7e6dcb4aa16),
        Uint128::new(0xdfbdcece67006ac9, 0x67a791e093e1d49b),
        Uint128::new(0x8bd6a141006042bd, 0xe0c8bb2c5c6d24e1),
        Uint128::new(0xaecc49914078536d, 0x58fae9f773886e19),
        Uint128::new(0xda7f5bf590966848, 0xaf39a475506a899f),
        Uint128::new(0x888f99797a5e012d, 0x6d8406c952429604),
        Uint128::new(0xaab37fd7d8f58178, 0xc8e5087ba6d33b84),
        Uint128::new(0xd5605fcdcf32e1d6, 0xfb1e4a9a90880a65),
        Uint128::new(0x855c3be0a17fcd26, 0x5cf2eea09a550680),
        Uint128::new(0xa6b34ad8c9dfc06f, 0xf42faa48c0ea481f),
        Uint128::new(0xd0601d8efc57b08b, 0xf13b94daf124da27),
        Uint128::new(0x823c12795db6ce57, 0x76c53d08d6b70859),
        Uint128::new(0xa2cb1717b52481ed, 0x54768c4b0c64ca6f),
        Uint128::new(0xcb7ddcdda26da268, 0xa9942f5dcf7dfd0a),
        Uint128::new(0xfe5d54150b090b02, 0xd3f93b35435d7c4d),
        Uint128::new(0x9efa548d26e5a6e1, 0xc47bc5014a1a6db0),
        Uint128::new(0xc6b8e9b0709f109a, 0x359ab6419ca1091c),
        Uint128::new(0xf867241c8cc6d4c0, 0xc30163d203c94b63),
        Uint128::new(0x9b407691d7fc44f8, 0x79e0de63425dcf1e),
        Uint128::new(0xc21094364dfb5636, 0x985915fc12f542e5),
        Uint128::new(0xf294b943e17a2bc4, 0x3e6f5b7b17b2939e),
        Uint128::new(0x979cf3ca6cec5b5a, 0xa705992ceecf9c43),
        Uint128::new(0xbd8430bd08277231, 0x50c6ff782a838354),
        Uint128::new(0xece53cec4a314ebd, 0xa4f8bf5635246429),
        Uint128::new(0x940f4613ae5ed136, 0x871b7795e136be9a),
        Uint128::new(0xb913179899f68584, 0x28e2557b59846e40),
        Uint128::new(0xe757dd7ec07426e5, 0x331aeada2fe589d0),
        Uint128::new(0x9096ea6f3848984f, 0x3ff0d2c85def7622),
        Uint128::new(0xb4bca50b065abe63, 0x0fed077a756b53aa),
        Uint128::new(0xe1ebce4dc7f16dfb, 0xd3e8495912c62895),
        Uint128::new(0x8d3360f09cf6e4bd, 0x64712dd7abbbd95d),
        Uint128::new(0xb080392cc4349dec, 0xbd8d794d96aacfb4),
        Uint128::new(0xdca04777f541c567, 0xecf0d7a0fc5583a1),
        Uint128::new(0x89e42caaf9491b60, 0xf41686c49db57245),
        Uint128::new(0xac5d37d5b79b6239, 0x311c2875c522ced6),
        Uint128::new(0xd77485cb25823ac7, 0x7d633293366b828c),
        Uint128::new(0x86a8d39ef77164bc, 0xae5dff9c02033198),
        Uint128::new(0xa8530886b54dbdeb, 0xd9f57f830283fdfd),
        Uint128::new(0xd267caa862a12d66, 0xd072df63c324fd7c),
        Uint128::new(0x8380dea93da4bc60, 0x4247cb9e59f71e6e),
        Uint128::new(0xa46116538d0deb78, 0x52d9be85f074e609),
        Uint128::new(0xcd795be870516656, 0x67902e276c921f8c),
        Uint128::new(0x806bd9714632dff6, 0x00ba1cd8a3db53b7),
        Uint128::new(0xa086cfcd97bf97f3, 0x80e8a40eccd228a5),
        Uint128::new(0xc8a883c0fdaf7df0, 0x6122cd128006b2ce),
        Uint128::new(0xfad2a4b13d1b5d6c, 0x796b805720085f82),
        Uint128::new(0x9cc3a6eec6311a63, 0xcbe3303674053bb1),
        Uint128::new(0xc3f490aa77bd60fc, 0xbedbfc4411068a9d),
        Uint128::new(0xf4f1b4d515acb93b, 0xee92fb5515482d45),
        Uint128::new(0x991711052d8bf3c5, 0x751bdd152d4d1c4b),
        Uint128::new(0xbf5cd54678eef0b6, 0xd262d45a78a0635e),
        Uint128::new(0xef340a98172aace4, 0x86fb897116c87c35),
        Uint128::new(0x9580869f0e7aac0e, 0xd45d35e6ae3d4da1),
        Uint128::new(0xbae0a846d2195712, 0x8974836059cca10a),
        Uint128::new(0xe998d258869facd7, 0x2bd1a438703fc94c),
        Uint128::new(0x91ff83775423cc06, 0x7b6306a34627ddd0),
        Uint128::new(0xb67f6455292cbf08, 0x1a3bc84c17b1d543),
        Uint128::new(0xe41f3d6a7377eeca, 0x20caba5f1d9e4a94),
        Uint128::new(0x8e938662882af53e, 0x547eb47b7282ee9d),
        Uint128::new(0xb23867fb2a35b28d, 0xe99e619a4f23aa44),
        Uint128::new(0xdec681f9f4c31f31, 0x6405fa00e2ec94d5),
        Uint128::new(0x8b3c113c38f9f37e, 0xde83bc408dd3dd05),
        Uint128::new(0xae0b158b4738705e, 0x9624ab50b148d446),
        Uint128::new(0xd98ddaee19068c76, 0x3badd624dd9b0958),
        Uint128::new(0x87f8a8d4cfa417c9, 0xe54ca5d70a80e5d7),
        Uint128::new(0xa9f6d30a038d1dbc, 0x5e9fcf4ccd211f4d),
        Uint128::new(0xd47487cc8470652b, 0x7647c32000696720),
        Uint128::new(0x84c8d4dfd2c63f3b, 0x29ecd9f40041e074),
        Uint128::new(0xa5fb0a17c777cf09, 0xf468107100525891),
        Uint128::new(0xcf79cc9db955c2cc, 0x7182148d4066eeb5),
        Uint128::new(0x81ac1fe293d599bf, 0xc6f14cd848405531),
        Uint128::new(0xa21727db38cb002f, 0xb8ada00e5a506a7d),
        Uint128::new(0xca9cf1d206fdc03b, 0xa6d90811f0e4851d),
        Uint128::new(0xfd442e4688bd304a, 0x908f4a166d1da664),
        Uint128::new(0x9e4a9cec15763e2e, 0x9a598e4e043287ff),
        Uint128::new(0xc5dd44271ad3cdba, 0x40eff1e1853f29fe),
        Uint128::new(0xf7549530e188c128, 0xd12bee59e68ef47d),
        Uint128::new(0x9a94dd3e8cf578b9, 0x82bb74f8301958cf),
        Uint128::new(0xc13a148e3032d6e7, 0xe36a52363c1faf02),
        Uint128::new(0xf18899b1bc3f8ca1, 0xdc44e6c3cb279ac2),
        Uint128::new(0x96f5600f15a7b7e5, 0x29ab103a5ef8c0ba),
        Uint128::new(0xbcb2b812db11a5de, 0x7415d448f6b6f0e8),
        Uint128::new(0xebdf661791d60f56, 0x111b495b3464ad22),
        Uint128::new(0x936b9fcebb25c995, 0xcab10dd900beec35),
        Uint128::new(0xb84687c269ef3bfb, 0x3d5d514f40eea743),
        Uint128::new(0xe65829b3046b0afa, 0x0cb4a5a3112a5113),
        Uint128::new(0x8ff71a0fe2c2e6dc, 0x47f0e785eaba72ac),
        Uint128::new(0xb3f4e093db73a093, 0x59ed216765690f57),
        Uint128::new(0xe0f218b8d25088b8, 0x306869c13ec3532d),
        Uint128::new(0x8c974f7383725573, 0x1e414218c73a13fc),
        Uint128::new(0xafbd2350644eeacf, 0xe5d1929ef90898fb),
        Uint128::new(0xdbac6c247d62a583, 0xdf45f746b74abf3a),
        Uint128::new(0x894bc396ce5da772, 0x6b8bba8c328eb784),
        Uint128::new(0xab9eb47c81f5114f, 0x066ea92f3f326565),
        Uint128::new(0xd686619ba27255a2, 0xc80a537b0efefebe),
        Uint128::new(0x8613fd0145877585, 0xbd06742ce95f5f37),
        Uint128::new(0xa798fc4196e952e7, 0x2c48113823b73705),
        Uint128::new(0xd17f3b51fca3a7a0, 0xf75a15862ca504c6),
        Uint128::new(0x82ef85133de648c4, 0x9a984d73dbe722fc),
        Uint128::new(0xa3ab66580d5fdaf5, 0xc13e60d0d2e0ebbb),
        Uint128::new(0xcc963fee10b7d1b3, 0x318df905079926a9),
        Uint128::new(0xffbbcfe994e5c61f, 0xfdf17746497f7053),
        Uint128::new(0x9fd561f1fd0f9bd3, 0xfeb6ea8bedefa634),
        Uint128::new(0xc7caba6e7c5382c8, 0xfe64a52ee96b8fc1),
        Uint128::new(0xf9bd690a1b68637b, 0x3dfdce7aa3c673b1),
        Uint128::new(0x9c1661a651213e2d, 0x06bea10ca65c084f),
        Uint128::new(0xc31bfa0fe5698db8, 0x486e494fcff30a63),
        Uint128::new(0xf3e2f893dec3f126, 0x5a89dba3c3efccfb),
        Uint128::new(0x986ddb5c6b3a76b7, 0xf89629465a75e01d),
        Uint128::new(0xbe89523386091465, 0xf6bbb397f1135824),
        Uint128::new(0xee2ba6c0678b597f, 0x746aa07ded582e2d),
        Uint128::new(0x94db483840b717ef, 0xa8c2a44eb4571cdd),
        Uint128::new(0xba121a4650e4ddeb, 0x92f34d62616ce414),
        Uint128::new(0xe896a0d7e51e1566, 0x77b020baf9c81d18),
        Uint128::new(0x915e2486ef32cd60, 0x0ace1474dc1d122f),
        Uint128::new(0xb5b5ada8aaff80b8, 0x0d819992132456bb),
        Uint128::new(0xe3231912d5bf60e6, 0x10e1fff697ed6c6a),
        Uint128::new(0x8df5efabc5979c8f, 0xca8d3ffa1ef463c2),
        Uint128::new(0xb1736b96b6fd83b3, 0xbd308ff8a6b17cb3),
        Uint128::new(0xddd0467c64bce4a0, 0xac7cb3f6d05ddbdf),
        Uint128::new(0x8aa22c0dbef60ee4, 0x6bcdf07a423aa96c),
        Uint128::new(0xad4ab7112eb3929d, 0x86c16c98d2c953c7),
        Uint128::new(0xd89d64d57a607744, 0xe871c7bf077ba8b8),
        Uint128::new(0x87625f056c7c4a8b, 0x11471cd764ad4973),
        Uint128::new(0xa93af6c6c79b5d2d, 0xd598e40d3dd89bd0),
        Uint128::new(0xd389b47879823479, 0x4aff1d108d4ec2c4),
        Uint128::new(0x843610cb4bf160cb, 0xcedf722a585139bb),
        Uint128::new(0xa54394fe1eedb8fe, 0xc2974eb4ee658829),
        Uint128::new(0xce947a3da6a9273e, 0x733d226229feea33),
        Uint128::new(0x811ccc668829b887, 0x0806357d5a3f5260),
        Uint128::new(0xa163ff802a3426a8, 0xca07c2dcb0cf26f8),
        Uint128::new(0xc9bcff6034c13052, 0xfc89b393dd02f0b6),
        Uint128::new(0xfc2c3f3841f17c67, 0xbbac2078d443ace3),
        Uint128::new(0x9d9ba7832936edc0, 0xd54b944b84aa4c0e),
        Uint128::new(0xc5029163f384a931, 0x0a9e795e65d4df12),
        Uint128::new(0xf64335bcf065d37d, 0x4d4617b5ff4a16d6),
        Uint128::new(0x99ea0196163fa42e, 0x504bced1bf8e4e46),
        Uint128::new(0xc06481fb9bcf8d39, 0xe45ec2862f71e1d7),
        Uint128::new(0xf07da27a82c37088, 0x5d767327bb4e5a4d),
        Uint128::new(0x964e858c91ba2655, 0x3a6a07f8d510f870),
        Uint128::new(0xbbe226efb628afea, 0x890489f70a55368c),
        Uint128::new(0xeadab0aba3b2dbe5, 0x2b45ac74ccea842f),
        Uint128::new(0x92c8ae6b464fc96f, 0x3b0b8bc90012929e),
        Uint128::new(0xb77ada0617e3bbcb, 0x09ce6ebb40173745),
        Uint128::new(0xe55990879ddcaabd, 0xcc420a6a101d0516),
        Uint128::new(0x8f57fa54c2a9eab6, 0x9fa946824a12232e),
        Uint128::new(0xb32df8e9f3546564, 0x47939822dc96abfa),
        Uint128::new(0xdff9772470297ebd, 0x59787e2b93bc56f8),
        Uint128::new(0x8bfbea76c619ef36, 0x57eb4edb3c55b65b),
        Uint128::new(0xaefae51477a06b03, 0xede622920b6b23f2),
        Uint128::new(0xdab99e59958885c4, 0xe95fab368e45ecee),
        Uint128::new(0x88b402f7fd75539b, 0x11dbcb0218ebb415),
        Uint128::new(0xaae103b5fcd2a881, 0xd652bdc29f26a11a),
        Uint128::new(0xd59944a37c0752a2, 0x4be76d3346f04960),
        Uint128::new(0x857fcae62d8493a5, 0x6f70a4400c562ddc),
        Uint128::new(0xa6dfbd9fb8e5b88e, 0xcb4ccd500f6bb953),
        Uint128::new(0xd097ad07a71f26b2, 0x7e2000a41346a7a8),
        Uint128::new(0x825ecc24c873782f, 0x8ed400668c0c28c9),
        Uint128::new(0xa2f67f2dfa90563b, 0x728900802f0f32fb),
        Uint128::new(0xcbb41ef979346bca, 0x4f2b40a03ad2ffba),
        Uint128::new(0xfea126b7d78186bc, 0xe2f610c84987bfa9),
        Uint128::new(0x9f24b832e6b0f436, 0x0dd9ca7d2df4d7ca),
        Uint128::new(0xc6ede63fa05d3143, 0x91503d1c79720dbc),
        Uint128::new(0xf8a95fcf88747d94, 0x75a44c6397ce912b),
        Uint128::new(0x9b69dbe1b548ce7c, 0xc986afbe3ee11abb),
        Uint128::new(0xc24452da229b021b, 0xfbe85badce996169),
        Uint128::new(0xf2d56790ab41c2a2, 0xfae27299423fb9c4),
        Uint128::new(0x97c560ba6b0919a5, 0xdccd879fc967d41b),
        Uint128::new(0xbdb6b8e905cb600f, 0x5400e987bbc1c921),
        Uint128::new(0xed246723473e3813, 0x290123e9aab23b69),
        Uint128::new(0x9436c0760c86e30b, 0xf9a0b6720aaf6522),
        Uint128::new(0xb94470938fa89bce, 0xf808e40e8d5b3e6a),
        Uint128::new(0xe7958cb87392c2c2, 0xb60b1d1230b20e05),
        Uint128::new(0x90bd77f3483bb9b9, 0xb1c6f22b5e6f48c3),
        Uint128::new(0xb4ecd5f01a4aa828, 0x1e38aeb6360b1af4),
        Uint128::new(0xe2280b6c20dd5232, 0x25c6da63c38de1b1),
        Uint128::new(0x8d590723948a535f, 0x579c487e5a38ad0f),
        Uint128::new(0xb0af48ec79ace837, 0x2d835a9df0c6d852),
        Uint128::new(0xdcdb1b2798182244, 0xf8e431456cf88e66),
        Uint128::new(0x8a08f0f8bf0f156b, 0x1b8e9ecb641b5900),
        Uint128::new(0xac8b2d36eed2dac5, 0xe272467e3d222f40),
        Uint128::new(0xd7adf884aa879177, 0x5b0ed81dcc6abb10),
        Uint128::new(0x86ccbb52ea94baea, 0x98e947129fc2b4ea),
        Uint128::new(0xa87fea27a539e9a5, 0x3f2398d747b36225),
        Uint128::new(0xd29fe4b18e88640e, 0x8eec7f0d19a03aae),
        Uint128::new(0x83a3eeeef9153e89, 0x1953cf68300424ad),
        Uint128::new(0xa48ceaaab75a8e2b, 0x5fa8c3423c052dd8),
        Uint128::new(0xcdb02555653131b6, 0x3792f412cb06794e),
        Uint128::new(0x808e17555f3ebf11, 0xe2bbd88bbee40bd1),
        Uint128::new(0xa0b19d2ab70e6ed6, 0x5b6aceaeae9d0ec5),
        Uint128::new(0xc8de047564d20a8b, 0xf245825a5a445276),
        Uint128::new(0xfb158592be068d2e, 0xeed6e2f0f0d56713),
        Uint128::new(0x9ced737bb6c4183d, 0x55464dd69685606c),
        Uint128::new(0xc428d05aa4751e4c, 0xaa97e14c3c26b887),
        Uint128::new(0xf53304714d9265df, 0xd53dd99f4b3066a9),
        Uint128::new(0x993fe2c6d07b7fab, 0xe546a8038efe402a),
        Uint128::new(0xbf8fdb78849a5f96, 0xde98520472bdd034),
        Uint128::new(0xef73d256a5c0f77c, 0x963e66858f6d4441),
        Uint128::new(0x95a8637627989aad, 0xdde7001379a44aa9),
        Uint128::new(0xbb127c53b17ec159, 0x5560c018580d5d53),
        Uint128::new(0xe9d71b689dde71af, 0xaab8f01e6e10b4a7),
        Uint128::new(0x9226712162ab070d, 0xcab3961304ca70e9),
        Uint128::new(0xb6b00d69bb55c8d1, 0x3d607b97c5fd0d23),
        Uint128::new(0xe45c10c42a2b3b05, 0x8cb89a7db77c506b),
        Uint128::new(0x8eb98a7a9a5b04e3, 0x77f3608e92adb243),
        Uint128::new(0xb267ed1940f1c61c, 0x55f038b237591ed4),
        Uint128::new(0xdf01e85f912e37a3, 0x6b6c46dec52f6689),
        Uint128::new(0x8b61313bbabce2c6, 0x2323ac4b3b3da016),
        Uint128::new(0xae397d8aa96c1b77, 0xabec975e0a0d081b),
        Uint128::new(0xd9c7dced53c72255, 0x96e7bd358c904a22),
        Uint128::new(0x881cea14545c7575, 0x7e50d64177da2e55),
        Uint128::new(0xaa242499697392d2, 0xdde50bd1d5d0b9ea),
        Uint128::new(0xd4ad2dbfc3d07787, 0x955e4ec64b44e865),
        Uint128::new(0x84ec3c97da624ab4, 0xbd5af13bef0b113f),
        Uint128::new(0xa6274bbdd0fadd61, 0xecb1ad8aeacdd58f),
        Uint128::new(0xcfb11ead453994ba, 0x67de18eda5814af3),
        Uint128::new(0x81ceb32c4b43fcf4, 0x80eacf948770ced8),
        Uint128::new(0xa2425ff75e14fc31, 0xa1258379a94d028e),
        Uint128::new(0xcad2f7f5359a3b3e, 0x096ee45813a04331),
        Uint128::new(0xfd87b5f28300ca0d, 0x8bca9d6e188853fd),
        Uint128::new(0x9e74d1b791e07e48, 0x775ea264cf55347e),
        Uint128::new(0xc612062576589dda, 0x95364afe032a819e),
        Uint128::new(0xf79687aed3eec551, 0x3a83ddbd83f52205),
        Uint128::new(0x9abe14cd44753b52, 0xc4926a9672793543),
        Uint128::new(0xc16d9a0095928a27, 0x75b7053c0f178294),
        Uint128::new(0xf1c90080baf72cb1, 0x5324c68b12dd6339),
        Uint128::new(0x971da05074da7bee, 0xd3f6fc16ebca5e04),
        Uint128::new(0xbce5086492111aea, 0x88f4bb1ca6bcf585),
        Uint128::new(0xec1e4a7db69561a5, 0x2b31e9e3d06c32e6),
        Uint128::new(0x9392ee8e921d5d07, 0x3aff322e62439fd0),
        Uint128::new(0xb877aa3236a4b449, 0x09befeb9fad487c3),
        Uint128::new(0xe69594bec44de15b, 0x4c2ebe687989a9b4),
        Uint128::new(0x901d7cf73ab0acd9, 0x0f9d37014bf60a11),
        Uint128::new(0xb424dc35095cd80f, 0x538484c19ef38c95),
        Uint128::new(0xe12e13424bb40e13, 0x2865a5f206b06fba),
        Uint128::new(0x8cbccc096f5088cb, 0xf93f87b7442e45d4),
        Uint128::new(0xafebff0bcb24aafe, 0xf78f69a51539d749),
        Uint128::new(0xdbe6fecebdedd5be, 0xb573440e5a884d1c),
        Uint128::new(0x89705f4136b4a597, 0x31680a88f8953031),
        Uint128::new(0xabcc77118461cefc, 0xfdc20d2b36ba7c3e),
        Uint128::new(0xd6bf94d5e57a42bc, 0x3d32907604691b4d),
        Uint128::new(0x8637bd05af6c69b5, 0xa63f9a49c2c1b110),
        Uint128::new(0xa7c5ac471b478423, 0x0fcf80dc33721d54),
        Uint128::new(0xd1b71758e219652b, 0xd3c36113404ea4a9),
        Uint128::new(0x83126e978d4fdf3b, 0x645a1cac083126ea),
        Uint128::new(0xa3d70a3d70a3d70a, 0x3d70a3d70a3d70a4),
        Uint128::new(0xcccccccccccccccc, 0xcccccccccccccccd),
        Uint128::new(0x8000000000000000, 0x0000000000000000),
        Uint128::new(0xa000000000000000, 0x0000000000000000),
        Uint128::new(0xc800000000000000, 0x0000000000000000),
        Uint128::new(0xfa00000000000000, 0x0000000000000000),
        Uint128::new(0x9c40000000000000, 0x0000000000000000),
        Uint128::new(0xc350000000000000, 0x0000000000000000),
        Uint128::new(0xf424000000000000, 0x0000000000000000),
        Uint128::new(0x9896800000000000, 0x0000000000000000),
        Uint128::new(0xbebc200000000000, 0x0000000000000000),
        Uint128::new(0xee6b280000000000, 0x0000000000000000),
        Uint128::new(0x9502f90000000000, 0x0000000000000000),
        Uint128::new(0xba43b74000000000, 0x0000000000000000),
        Uint128::new(0xe8d4a51000000000, 0x0000000000000000),
        Uint128::new(0x9184e72a00000000, 0x0000000000000000),
        Uint128::new(0xb5e620f480000000, 0x0000000000000000),
        Uint128::new(0xe35fa931a0000000, 0x0000000000000000),
        Uint128::new(0x8e1bc9bf04000000, 0x0000000000000000),
        Uint128::new(0xb1a2bc2ec5000000, 0x0000000000000000),
        Uint128::new(0xde0b6b3a76400000, 0x0000000000000000),
        Uint128::new(0x8ac7230489e80000, 0x0000000000000000),
        Uint128::new(0xad78ebc5ac620000, 0x0000000000000000),
        Uint128::new(0xd8d726b7177a8000, 0x0000000000000000),
        Uint128::new(0x878678326eac9000, 0x0000000000000000),
        Uint128::new(0xa968163f0a57b400, 0x0000000000000000),
        Uint128::new(0xd3c21bcecceda100, 0x0000000000000000),
        Uint128::new(0x84595161401484a0, 0x0000000000000000),
        Uint128::new(0xa56fa5b99019a5c8, 0x0000000000000000),
        Uint128::new(0xcecb8f27f4200f3a, 0x0000000000000000),
        Uint128::new(0x813f3978f8940984, 0x4000000000000000),
        Uint128::new(0xa18f07d736b90be5, 0x5000000000000000),
        Uint128::new(0xc9f2c9cd04674ede, 0xa400000000000000),
        Uint128::new(0xfc6f7c4045812296, 0x4d00000000000000),
        Uint128::new(0x9dc5ada82b70b59d, 0xf020000000000000),
        Uint128::new(0xc5371912364ce305, 0x6c28000000000000),
        Uint128::new(0xf684df56c3e01bc6, 0xc732000000000000),
        Uint128::new(0x9a130b963a6c115c, 0x3c7f400000000000),
        Uint128::new(0xc097ce7bc90715b3, 0x4b9f100000000000),
        Uint128::new(0xf0bdc21abb48db20, 0x1e86d40000000000),
        Uint128::new(0x96769950b50d88f4, 0x1314448000000000),
        Uint128::new(0xbc143fa4e250eb31, 0x17d955a000000000),
        Uint128::new(0xeb194f8e1ae525fd, 0x5dcfab0800000000),
        Uint128::new(0x92efd1b8d0cf37be, 0x5aa1cae500000000),
        Uint128::new(0xb7abc627050305ad, 0xf14a3d9e40000000),
        Uint128::new(0xe596b7b0c643c719, 0x6d9ccd05d0000000),
        Uint128::new(0x8f7e32ce7bea5c6f, 0xe4820023a2000000),
        Uint128::new(0xb35dbf821ae4f38b, 0xdda2802c8a800000),
        Uint128::new(0xe0352f62a19e306e, 0xd50b2037ad200000),
        Uint128::new(0x8c213d9da502de45, 0x4526f422cc340000),
        Uint128::new(0xaf298d050e4395d6, 0x9670b12b7f410000),
        Uint128::new(0xdaf3f04651d47b4c, 0x3c0cdd765f114000),
        Uint128::new(0x88d8762bf324cd0f, 0xa5880a69fb6ac800),
        Uint128::new(0xab0e93b6efee0053, 0x8eea0d047a457a00),
        Uint128::new(0xd5d238a4abe98068, 0x72a4904598d6d880),
        Uint128::new(0x85a36366eb71f041, 0x47a6da2b7f864750),
        Uint128::new(0xa70c3c40a64e6c51, 0x999090b65f67d924),
        Uint128::new(0xd0cf4b50cfe20765, 0xfff4b4e3f741cf6d),
        Uint128::new(0x82818f1281ed449f, 0xbff8f10e7a8921a5),
        Uint128::new(0xa321f2d7226895c7, 0xaff72d52192b6a0e),
        Uint128::new(0xcbea6f8ceb02bb39, 0x9bf4f8a69f764491),
        Uint128::new(0xfee50b7025c36a08, 0x02f236d04753d5b5),
        Uint128::new(0x9f4f2726179a2245, 0x01d762422c946591),
        Uint128::new(0xc722f0ef9d80aad6, 0x424d3ad2b7b97ef6),
        Uint128::new(0xf8ebad2b84e0d58b, 0xd2e0898765a7deb3),
        Uint128::new(0x9b934c3b330c8577, 0x63cc55f49f88eb30),
        Uint128::new(0xc2781f49ffcfa6d5, 0x3cbf6b71c76b25fc),
        Uint128::new(0xf316271c7fc3908a, 0x8bef464e3945ef7b),
        Uint128::new(0x97edd871cfda3a56, 0x97758bf0e3cbb5ad),
        Uint128::new(0xbde94e8e43d0c8ec, 0x3d52eeed1cbea318),
        Uint128::new(0xed63a231d4c4fb27, 0x4ca7aaa863ee4bde),
        Uint128::new(0x945e455f24fb1cf8, 0x8fe8caa93e74ef6b),
        Uint128::new(0xb975d6b6ee39e436, 0xb3e2fd538e122b45),
        Uint128::new(0xe7d34c64a9c85d44, 0x60dbbca87196b617),
        Uint128::new(0x90e40fbeea1d3a4a, 0xbc8955e946fe31ce),
        Uint128::new(0xb51d13aea4a488dd, 0x6babab6398bdbe42),
        Uint128::new(0xe264589a4dcdab14, 0xc696963c7eed2dd2),
        Uint128::new(0x8d7eb76070a08aec, 0xfc1e1de5cf543ca3),
        Uint128::new(0xb0de65388cc8ada8, 0x3b25a55f43294bcc),
        Uint128::new(0xdd15fe86affad912, 0x49ef0eb713f39ebf),
        Uint128::new(0x8a2dbf142dfcc7ab, 0x6e3569326c784338),
        Uint128::new(0xacb92ed9397bf996, 0x49c2c37f07965405),
        Uint128::new(0xd7e77a8f87daf7fb, 0xdc33745ec97be907),
        Uint128::new(0x86f0ac99b4e8dafd, 0x69a028bb3ded71a4),
        Uint128::new(0xa8acd7c0222311bc, 0xc40832ea0d68ce0d),
        Uint128::new(0xd2d80db02aabd62b, 0xf50a3fa490c30191),
        Uint128::new(0x83c7088e1aab65db, 0x792667c6da79e0fb),
        Uint128::new(0xa4b8cab1a1563f52, 0x577001b891185939),
        Uint128::new(0xcde6fd5e09abcf26, 0xed4c0226b55e6f87),
        Uint128::new(0x80b05e5ac60b6178, 0x544f8158315b05b5),
        Uint128::new(0xa0dc75f1778e39d6, 0x696361ae3db1c722),
        Uint128::new(0xc913936dd571c84c, 0x03bc3a19cd1e38ea),
        Uint128::new(0xfb5878494ace3a5f, 0x04ab48a04065c724),
        Uint128::new(0x9d174b2dcec0e47b, 0x62eb0d64283f9c77),
        Uint128::new(0xc45d1df942711d9a, 0x3ba5d0bd324f8395),
        Uint128::new(0xf5746577930d6500, 0xca8f44ec7ee3647a),
        Uint128::new(0x9968bf6abbe85f20, 0x7e998b13cf4e1ecc),
        Uint128::new(0xbfc2ef456ae276e8, 0x9e3fedd8c321a67f),
        Uint128::new(0xefb3ab16c59b14a2, 0xc5cfe94ef3ea101f),
        Uint128::new(0x95d04aee3b80ece5, 0xbba1f1d158724a13),
        Uint128::new(0xbb445da9ca61281f, 0x2a8a6e45ae8edc98),
        Uint128::new(0xea1575143cf97226, 0xf52d09d71a3293be),
        Uint128::new(0x924d692ca61be758, 0x593c2626705f9c57),
        Uint128::new(0xb6e0c377cfa2e12e, 0x6f8b2fb00c77836d),
        Uint128::new(0xe498f455c38b997a, 0x0b6dfb9c0f956448),
        Uint128::new(0x8edf98b59a373fec, 0x4724bd4189bd5ead),
        Uint128::new(0xb2977ee300c50fe7, 0x58edec91ec2cb658),
        Uint128::new(0xdf3d5e9bc0f653e1, 0x2f2967b66737e3ee),
        Uint128::new(0x8b865b215899f46c, 0xbd79e0d20082ee75),
        Uint128::new(0xae67f1e9aec07187, 0xecd8590680a3aa12),
        Uint128::new(0xda01ee641a708de9, 0xe80e6f4820cc9496),
        Uint128::new(0x884134fe908658b2, 0x3109058d147fdcde),
        Uint128::new(0xaa51823e34a7eede, 0xbd4b46f0599fd416),
        Uint128::new(0xd4e5e2cdc1d1ea96, 0x6c9e18ac7007c91b),
        Uint128::new(0x850fadc09923329e, 0x03e2cf6bc604ddb1),
        Uint128::new(0xa6539930bf6bff45, 0x84db8346b786151d),
        Uint128::new(0xcfe87f7cef46ff16, 0xe612641865679a64),
        Uint128::new(0x81f14fae158c5f6e, 0x4fcb7e8f3f60c07f),
        Uint128::new(0xa26da3999aef7749, 0xe3be5e330f38f09e),
        Uint128::new(0xcb090c8001ab551c, 0x5cadf5bfd3072cc6),
        Uint128::new(0xfdcb4fa002162a63, 0x73d9732fc7c8f7f7),
        Uint128::new(0x9e9f11c4014dda7e, 0x2867e7fddcdd9afb),
        Uint128::new(0xc646d63501a1511d, 0xb281e1fd541501b9),
        Uint128::new(0xf7d88bc24209a565, 0x1f225a7ca91a4227),
        Uint128::new(0x9ae757596946075f, 0x3375788de9b06959),
        Uint128::new(0xc1a12d2fc3978937, 0x0052d6b1641c83af),
        Uint128::new(0xf209787bb47d6b84, 0xc0678c5dbd23a49b),
        Uint128::new(0x9745eb4d50ce6332, 0xf840b7ba963646e1),
        Uint128::new(0xbd176620a501fbff, 0xb650e5a93bc3d899),
        Uint128::new(0xec5d3fa8ce427aff, 0xa3e51f138ab4cebf),
        Uint128::new(0x93ba47c980e98cdf, 0xc66f336c36b10138),
        Uint128::new(0xb8a8d9bbe123f017, 0xb80b0047445d4185),
        Uint128::new(0xe6d3102ad96cec1d, 0xa60dc059157491e6),
        Uint128::new(0x9043ea1ac7e41392, 0x87c89837ad68db30),
        Uint128::new(0xb454e4a179dd1877, 0x29babe4598c311fc),
        Uint128::new(0xe16a1dc9d8545e94, 0xf4296dd6fef3d67b),
        Uint128::new(0x8ce2529e2734bb1d, 0x1899e4a65f58660d),
        Uint128::new(0xb01ae745b101e9e4, 0x5ec05dcff72e7f90),
        Uint128::new(0xdc21a1171d42645d, 0x76707543f4fa1f74),
        Uint128::new(0x899504ae72497eba, 0x6a06494a791c53a9),
        Uint128::new(0xabfa45da0edbde69, 0x0487db9d17636893),
        Uint128::new(0xd6f8d7509292d603, 0x45a9d2845d3c42b7),
        Uint128::new(0x865b86925b9bc5c2, 0x0b8a2392ba45a9b3),
        Uint128::new(0xa7f26836f282b732, 0x8e6cac7768d7141f),
        Uint128::new(0xd1ef0244af2364ff, 0x3207d795430cd927),
        Uint128::new(0x8335616aed761f1f, 0x7f44e6bd49e807b9),
        Uint128::new(0xa402b9c5a8d3a6e7, 0x5f16206c9c6209a7),
        Uint128::new(0xcd036837130890a1, 0x36dba887c37a8c10),
        Uint128::new(0x802221226be55a64, 0xc2494954da2c978a),
        Uint128::new(0xa02aa96b06deb0fd, 0xf2db9baa10b7bd6d),
        Uint128::new(0xc83553c5c8965d3d, 0x6f92829494e5acc8),
        Uint128::new(0xfa42a8b73abbf48c, 0xcb772339ba1f17fa),
        Uint128::new(0x9c69a97284b578d7, 0xff2a760414536efc),
        Uint128::new(0xc38413cf25e2d70d, 0xfef5138519684abb),
        Uint128::new(0xf46518c2ef5b8cd1, 0x7eb258665fc25d6a),
        Uint128::new(0x98bf2f79d5993802, 0xef2f773ffbd97a62),
        Uint128::new(0xbeeefb584aff8603, 0xaafb550ffacfd8fb),
        Uint128::new(0xeeaaba2e5dbf6784, 0x95ba2a53f983cf39),
        Uint128::new(0x952ab45cfa97a0b2, 0xdd945a747bf26184),
        Uint128::new(0xba756174393d88df, 0x94f971119aeef9e5),
        Uint128::new(0xe912b9d1478ceb17, 0x7a37cd5601aab85e),
        Uint128::new(0x91abb422ccb812ee, 0xac62e055c10ab33b),
        Uint128::new(0xb616a12b7fe617aa, 0x577b986b314d600a),
        Uint128::new(0xe39c49765fdf9d94, 0xed5a7e85fda0b80c),
        Uint128::new(0x8e41ade9fbebc27d, 0x14588f13be847308),
        Uint128::new(0xb1d219647ae6b31c, 0x596eb2d8ae258fc9),
        Uint128::new(0xde469fbd99a05fe3, 0x6fca5f8ed9aef3bc),
        Uint128::new(0x8aec23d680043bee, 0x25de7bb9480d5855),
        Uint128::new(0xada72ccc20054ae9, 0xaf561aa79a10ae6b),
        Uint128::new(0xd910f7ff28069da4, 0x1b2ba1518094da05),
        Uint128::new(0x87aa9aff79042286, 0x90fb44d2f05d0843),
        Uint128::new(0xa99541bf57452b28, 0x353a1607ac744a54),
        Uint128::new(0xd3fa922f2d1675f2, 0x42889b8997915ce9),
        Uint128::new(0x847c9b5d7c2e09b7, 0x69956135febada12),
        Uint128::new(0xa59bc234db398c25, 0x43fab9837e699096),
        Uint128::new(0xcf02b2c21207ef2e, 0x94f967e45e03f4bc),
        Uint128::new(0x8161afb94b44f57d, 0x1d1be0eebac278f6),
        Uint128::new(0xa1ba1ba79e1632dc, 0x6462d92a69731733),
        Uint128::new(0xca28a291859bbf93, 0x7d7b8f7503cfdcff),
        Uint128::new(0xfcb2cb35e702af78, 0x5cda735244c3d43f),
        Uint128::new(0x9defbf01b061adab, 0x3a0888136afa64a8),
        Uint128::new(0xc56baec21c7a1916, 0x088aaa1845b8fdd1),
        Uint128::new(0xf6c69a72a3989f5b, 0x8aad549e57273d46),
        Uint128::new(0x9a3c2087a63f6399, 0x36ac54e2f678864c),
        Uint128::new(0xc0cb28a98fcf3c7f, 0x84576a1bb416a7de),
        Uint128::new(0xf0fdf2d3f3c30b9f, 0x656d44a2a11c51d6),
        Uint128::new(0x969eb7c47859e743, 0x9f644ae5a4b1b326),
        Uint128::new(0xbc4665b596706114, 0x873d5d9f0dde1fef),
        Uint128::new(0xeb57ff22fc0c7959, 0xa90cb506d155a7eb),
        Uint128::new(0x9316ff75dd87cbd8, 0x09a7f12442d588f3),
        Uint128::new(0xb7dcbf5354e9bece, 0x0c11ed6d538aeb30),
        Uint128::new(0xe5d3ef282a242e81, 0x8f1668c8a86da5fb),
        Uint128::new(0x8fa475791a569d10, 0xf96e017d694487bd),
        Uint128::new(0xb38d92d760ec4455, 0x37c981dcc395a9ad),
        Uint128::new(0xe070f78d3927556a, 0x85bbe253f47b1418),
        Uint128::new(0x8c469ab843b89562, 0x93956d7478ccec8f),
        Uint128::new(0xaf58416654a6babb, 0x387ac8d1970027b3),
        Uint128::new(0xdb2e51bfe9d0696a, 0x06997b05fcc0319f),
        Uint128::new(0x88fcf317f22241e2, 0x441fece3bdf81f04),
        Uint128::new(0xab3c2fddeeaad25a, 0xd527e81cad7626c4),
        Uint128::new(0xd60b3bd56a5586f1, 0x8a71e223d8d3b075),
        Uint128::new(0x85c7056562757456, 0xf6872d5667844e4a),
        Uint128::new(0xa738c6bebb12d16c, 0xb428f8ac016561dc),
        Uint128::new(0xd106f86e69d785c7, 0xe13336d701beba53),
        Uint128::new(0x82a45b450226b39c, 0xecc0024661173474),
        Uint128::new(0xa34d721642b06084, 0x27f002d7f95d0191),
        Uint128::new(0xcc20ce9bd35c78a5, 0x31ec038df7b441f5),
        Uint128::new(0xff290242c83396ce, 0x7e67047175a15272),
        Uint128::new(0x9f79a169bd203e41, 0x0f0062c6e984d387),
        Uint128::new(0xc75809c42c684dd1, 0x52c07b78a3e60869),
        Uint128::new(0xf92e0c3537826145, 0xa7709a56ccdf8a83),
        Uint128::new(0x9bbcc7a142b17ccb, 0x88a66076400bb692),
        Uint128::new(0xc2abf989935ddbfe, 0x6acff893d00ea436),
        Uint128::new(0xf356f7ebf83552fe, 0x0583f6b8c4124d44),
        Uint128::new(0x98165af37b2153de, 0xc3727a337a8b704b),
        Uint128::new(0xbe1bf1b059e9a8d6, 0x744f18c0592e4c5d),
        Uint128::new(0xeda2ee1c7064130c, 0x1162def06f79df74),
        Uint128::new(0x9485d4d1c63e8be7, 0x8addcb5645ac2ba9),
        Uint128::new(0xb9a74a0637ce2ee1, 0x6d953e2bd7173693),
        Uint128::new(0xe8111c87c5c1ba99, 0xc8fa8db6ccdd0438),
        Uint128::new(0x910ab1d4db9914a0, 0x1d9c9892400a22a3),
        Uint128::new(0xb54d5e4a127f59c8, 0x2503beb6d00cab4c),
        Uint128::new(0xe2a0b5dc971f303a, 0x2e44ae64840fd61e),
        Uint128::new(0x8da471a9de737e24, 0x5ceaecfed289e5d3),
        Uint128::new(0xb10d8e1456105dad, 0x7425a83e872c5f48),
        Uint128::new(0xdd50f1996b947518, 0xd12f124e28f7771a),
        Uint128::new(0x8a5296ffe33cc92f, 0x82bd6b70d99aaa70),
        Uint128::new(0xace73cbfdc0bfb7b, 0x636cc64d1001550c),
        Uint128::new(0xd8210befd30efa5a, 0x3c47f7e05401aa4f),
        Uint128::new(0x8714a775e3e95c78, 0x65acfaec34810a72),
        Uint128::new(0xa8d9d1535ce3b396, 0x7f1839a741a14d0e),
        Uint128::new(0xd31045a8341ca07c, 0x1ede48111209a051),
        Uint128::new(0x83ea2b892091e44d, 0x934aed0aab460433),
        Uint128::new(0xa4e4b66b68b65d60, 0xf81da84d56178540),
        Uint128::new(0xce1de40642e3f4b9, 0x36251260ab9d668f),
        Uint128::new(0x80d2ae83e9ce78f3, 0xc1d72b7c6b42601a),
        Uint128::new(0xa1075a24e4421730, 0xb24cf65b8612f820),
        Uint128::new(0xc94930ae1d529cfc, 0xdee033f26797b628),
        Uint128::new(0xfb9b7cd9a4a7443c, 0x169840ef017da3b2),
        Uint128::new(0x9d412e0806e88aa5, 0x8e1f289560ee864f),
        Uint128::new(0xc491798a08a2ad4e, 0xf1a6f2bab92a27e3),
        Uint128::new(0xf5b5d7ec8acb58a2, 0xae10af696774b1dc),
        Uint128::new(0x9991a6f3d6bf1765, 0xacca6da1e0a8ef2a),
        Uint128::new(0xbff610b0cc6edd3f, 0x17fd090a58d32af4),
        Uint128::new(0xeff394dcff8a948e, 0xddfc4b4cef07f5b1),
        Uint128::new(0x95f83d0a1fb69cd9, 0x4abdaf101564f98f),
        Uint128::new(0xbb764c4ca7a4440f, 0x9d6d1ad41abe37f2),
        Uint128::new(0xea53df5fd18d5513, 0x84c86189216dc5ee),
        Uint128::new(0x92746b9be2f8552c, 0x32fd3cf5b4e49bb5),
        Uint128::new(0xb7118682dbb66a77, 0x3fbc8c33221dc2a2),
        Uint128::new(0xe4d5e82392a40515, 0x0fabaf3feaa5334b),
        Uint128::new(0x8f05b1163ba6832d, 0x29cb4d87f2a7400f),
        Uint128::new(0xb2c71d5bca9023f8, 0x743e20e9ef511013),
        Uint128::new(0xdf78e4b2bd342cf6, 0x914da9246b255417),
        Uint128::new(0x8bab8eefb6409c1a, 0x1ad089b6c2f7548f),
        Uint128::new(0xae9672aba3d0c320, 0xa184ac2473b529b2),
        Uint128::new(0xda3c0f568cc4f3e8, 0xc9e5d72d90a2741f),
        Uint128::new(0x8865899617fb1871, 0x7e2fa67c7a658893),
        Uint128::new(0xaa7eebfb9df9de8d, 0xddbb901b98feeab8),
        Uint128::new(0xd51ea6fa85785631, 0x552a74227f3ea566),
        Uint128::new(0x8533285c936b35de, 0xd53a88958f872760),
        Uint128::new(0xa67ff273b8460356, 0x8a892abaf368f138),
        Uint128::new(0xd01fef10a657842c, 0x2d2b7569b0432d86),
        Uint128::new(0x8213f56a67f6b29b, 0x9c3b29620e29fc74),
        Uint128::new(0xa298f2c501f45f42, 0x8349f3ba91b47b90),
        Uint128::new(0xcb3f2f7642717713, 0x241c70a936219a74),
        Uint128::new(0xfe0efb53d30dd4d7, 0xed238cd383aa0111),
        Uint128::new(0x9ec95d1463e8a506, 0xf4363804324a40ab),
        Uint128::new(0xc67bb4597ce2ce48, 0xb143c6053edcd0d6),
        Uint128::new(0xf81aa16fdc1b81da, 0xdd94b7868e94050b),
        Uint128::new(0x9b10a4e5e9913128, 0xca7cf2b4191c8327),
        Uint128::new(0xc1d4ce1f63f57d72, 0xfd1c2f611f63a3f1),
        Uint128::new(0xf24a01a73cf2dccf, 0xbc633b39673c8ced),
        Uint128::new(0x976e41088617ca01, 0xd5be0503e085d814),
        Uint128::new(0xbd49d14aa79dbc82, 0x4b2d8644d8a74e19),
        Uint128::new(0xec9c459d51852ba2, 0xddf8e7d60ed1219f),
        Uint128::new(0x93e1ab8252f33b45, 0xcabb90e5c942b504),
        Uint128::new(0xb8da1662e7b00a17, 0x3d6a751f3b936244),
        Uint128::new(0xe7109bfba19c0c9d, 0x0cc512670a783ad5),
        Uint128::new(0x906a617d450187e2, 0x27fb2b80668b24c6),
        Uint128::new(0xb484f9dc9641e9da, 0xb1f9f660802dedf7),
        Uint128::new(0xe1a63853bbd26451, 0x5e7873f8a0396974),
        Uint128::new(0x8d07e33455637eb2, 0xdb0b487b6423e1e9),
        Uint128::new(0xb049dc016abc5e5f, 0x91ce1a9a3d2cda63),
        Uint128::new(0xdc5c5301c56b75f7, 0x7641a140cc7810fc),
        Uint128::new(0x89b9b3e11b6329ba, 0xa9e904c87fcb0a9e),
        Uint128::new(0xac2820d9623bf429, 0x546345fa9fbdcd45),
        Uint128::new(0xd732290fbacaf133, 0xa97c177947ad4096),
        Uint128::new(0x867f59a9d4bed6c0, 0x49ed8eabcccc485e),
        Uint128::new(0xa81f301449ee8c70, 0x5c68f256bfff5a75),
        Uint128::new(0xd226fc195c6a2f8c, 0x73832eec6fff3112),
        Uint128::new(0x83585d8fd9c25db7, 0xc831fd53c5ff7eac),
        Uint128::new(0xa42e74f3d032f525, 0xba3e7ca8b77f5e56),
        Uint128::new(0xcd3a1230c43fb26f, 0x28ce1bd2e55f35ec),
        Uint128::new(0x80444b5e7aa7cf85, 0x7980d163cf5b81b4),
        Uint128::new(0xa0555e361951c366, 0xd7e105bcc3326220),
        Uint128::new(0xc86ab5c39fa63440, 0x8dd9472bf3fefaa8),
        Uint128::new(0xfa856334878fc150, 0xb14f98f6f0feb952),
        Uint128::new(0x9c935e00d4b9d8d2, 0x6ed1bf9a569f33d4),
        Uint128::new(0xc3b8358109e84f07, 0x0a862f80ec4700c9),
        Uint128::new(0xf4a642e14c6262c8, 0xcd27bb612758c0fb),
        Uint128::new(0x98e7e9cccfbd7dbd, 0x8038d51cb897789d),
        Uint128::new(0xbf21e44003acdd2c, 0xe0470a63e6bd56c4),
        Uint128::new(0xeeea5d5004981478, 0x1858ccfce06cac75),
        Uint128::new(0x95527a5202df0ccb, 0x0f37801e0c43ebc9),
        Uint128::new(0xbaa718e68396cffd, 0xd30560258f54e6bb),
        Uint128::new(0xe950df20247c83fd, 0x47c6b82ef32a206a),
        Uint128::new(0x91d28b7416cdd27e, 0x4cdc331d57fa5442),
        Uint128::new(0xb6472e511c81471d, 0xe0133fe4adf8e953),
        Uint128::new(0xe3d8f9e563a198e5, 0x58180fddd97723a7),
        Uint128::new(0x8e679c2f5e44ff8f, 0x570f09eaa7ea7649),
        Uint128::new(0xb201833b35d63f73, 0x2cd2cc6551e513db),
        Uint128::new(0xde81e40a034bcf4f, 0xf8077f7ea65e58d2),
        Uint128::new(0x8b112e86420f6191, 0xfb04afaf27faf783),
        Uint128::new(0xadd57a27d29339f6, 0x79c5db9af1f9b564),
        Uint128::new(0xd94ad8b1c7380874, 0x18375281ae7822bd),
        Uint128::new(0x87cec76f1c830548, 0x8f2293910d0b15b6),
        Uint128::new(0xa9c2794ae3a3c69a, 0xb2eb3875504ddb23),
        Uint128::new(0xd433179d9c8cb841, 0x5fa60692a46151ec),
        Uint128::new(0x849feec281d7f328, 0xdbc7c41ba6bcd334),
        Uint128::new(0xa5c7ea73224deff3, 0x12b9b522906c0801),
        Uint128::new(0xcf39e50feae16bef, 0xd768226b34870a01),
        Uint128::new(0x81842f29f2cce375, 0xe6a1158300d46641),
        Uint128::new(0xa1e53af46f801c53, 0x60495ae3c1097fd1),
        Uint128::new(0xca5e89b18b602368, 0x385bb19cb14bdfc5),
        Uint128::new(0xfcf62c1dee382c42, 0x46729e03dd9ed7b6),
        Uint128::new(0x9e19db92b4e31ba9, 0x6c07a2c26a8346d2),
        Uint128::new(0xc5a05277621be293, 0xc7098b7305241886),
        Uint128::new(0xf70867153aa2db38, 0xb8cbee4fc66d1ea8),
    ];
}

#[cfg(test)]
#[path = "dragonbox_tests.rs"]
mod tests;
