// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ACES 2.0 output transform's model: a port of
//! `src/OpenColorIO/ops/fixedfunction/ACES2/Transform.cpp` @ v2.5.2 (and the declarations of
//! `Transform.h`).
//!
//! Everything is in `float`, with the `float` functions of the math library: both wheels call
//! `powf`, `atan2f`, `log10f`, `logf` and `sinf`/`cosf` (GCC merges them into `sincosf`, whose
//! results are `sinf`'s and `cosf`'s), and compute `sqrt` of a `float` as `sqrtss`. The code
//! commented out upstream (the SSE and AVX horizontal sums, Transform.cpp:255-334) isn't
//! compiled, so the port has the scalar sum.

use super::color_lib::{IDENTITY_M33, rgb_to_rgb_f33, rgb_to_xyz_f33, xyz_to_rgb_f33};
use super::common::{
    CAM_NL_OFFSET, CAM_NL_SCALE, CHROMA_COMPRESS, CHROMA_COMPRESS_FACT, CHROMA_EXPAND,
    CHROMA_EXPAND_FACT, CHROMA_EXPAND_THR, COMPRESSION_THRESHOLD, CUSP_CORNER_COUNT,
    CUSP_MID_BLEND, ChromaCompressParams, DISPLAY_CUSP_TOLERANCE, FOCUS_DISTANCE,
    FOCUS_DISTANCE_SCALING, FOCUS_GAIN_BLEND, GAMMA_ACCURACY, GAMMA_MAXIMUM, GAMMA_MINIMUM,
    GAMMA_SEARCH_STEP, GamutCompressParams, HUE_LIMIT, HueDependantGamutParams, J_SCALE, JMhParams,
    L_A, MAX_SORTED_CORNERS, REACH_CUSP_TOLERANCE, REFERENCE_LUMINANCE,
    ResolvedSharedCompressionParameters, SMOOTH_CUSPS, SMOOTH_M, SURROUND,
    SharedCompressionParameters, TOTAL_CORNER_COUNT, Table1D, Table3D, ToneScaleParams, Y_B, cam16,
    f32_to_u32, from_radians_, table_base, to_radians,
};
use super::matrix_lib::{
    F2, F3, M33f, Row, f3_from_f, invert_f33, mult_f_f3, mult_f3_f33, mult_f3_f33_rows,
    mult_f33_f33, scale_f33,
};
use crate::exception::{Exception, Result};
use crate::math_utils::{lerpf, sse_add, sse_mul, sse_sub, std_max, std_min};
use crate::transforms::builtins::color_matrix_helpers::Primaries;

/// `cuspCornerCount`, as an index.
const CUSP_CORNERS: usize = CUSP_CORNER_COUNT as usize;
/// `totalCornerCount`, as an index.
const TOTAL_CORNERS: usize = TOTAL_CORNER_COUNT as usize;
/// `max_sorted_corners`, as an index.
const MAX_SORTED: usize = MAX_SORTED_CORNERS as usize;

/// Whether the platform's wheel is the Windows one (MSVC) rather than the Linux one (GCC).
///
/// Where both operands of an operation can be NaN, the result is the first one's
/// (`math_utils::sse_add`), and the two compilers ordered the operands of the renderers'
/// functions differently, and differently in each inlined copy of a helper. The functions
/// the renderers call follow their wheel's machine code, read with `tools/wheel-inspect`: each
/// names the compiled functions it follows (Linux symbols; Windows `sub_` addresses, which
/// `wheel-inspect disasm 0x...` prints), and `MSVC` picks the platform's order. The values
/// are the source's in every order: only which NaN comes out differs
/// (`docs/improvements.md` I-81).
const MSVC: bool = cfg!(target_os = "windows");

/// `a + b`, or `b + a` where `swap`: the same value, but the first operand's NaN.
#[inline]
fn add_swap(a: f32, b: f32, swap: bool) -> f32 {
    if swap { sse_add(b, a) } else { sse_add(a, b) }
}

/// `a * b`, or `b * a` where `swap`: the same value, but the first operand's NaN.
#[inline]
fn mul_swap(a: f32, b: f32, swap: bool) -> f32 {
    if swap { sse_mul(b, a) } else { sse_mul(a, b) }
}

/// The error where hues that don't compare (NaN) or a degenerate gamut make upstream's hue table
/// code read or write past its arrays (`docs/improvements.md` U-32).
pub const CORNERS_OVERRUN: &str = "ACES 2.0: the gamut's corner hues make the hue table read or \
     write past its arrays: upstream's behaviour is undefined.";

//
// Table lookups
//

/// `lerpf` on each component.
///
/// Port of `lerp(const f3 &, const f3 &, float)` and `lerp(const float[3], const float[3],
/// float)` (src/OpenColorIO/ops/fixedfunction/ACES2/Transform.cpp:20-36 @ v2.5.2).
#[inline]
pub fn lerp(lower: &F3, upper: &F3, t: f32) -> F3 {
    [
        lerpf(lower[0], upper[0], t),
        lerpf(lower[1], upper[1], t),
        lerpf(lower[2], upper[2], t),
    ]
}

/// `(a + b) / 2` in `unsigned int` arithmetic, which wraps.
///
/// Port of `midpoint(unsigned int, unsigned int)` (Transform.cpp:38-41 @ v2.5.2).
#[inline]
pub fn midpoint_u32(a: u32, b: u32) -> u32 {
    a.wrapping_add(b) / 2
}

/// `(a + b) / 2`.
///
/// Port of `midpoint(float, float)` (Transform.cpp:43-46 @ v2.5.2).
#[inline]
pub fn midpoint(a: f32, b: f32) -> f32 {
    sse_add(a, b) / 2.0
}

/// The upper index of the interval of `hues` that holds the hue `h`, by bisection from the
/// position a uniform table would give, within `hue_linearity_search_range` of it.
///
/// Port of `lookup_hue_interval` (Transform.cpp:48-74 @ v2.5.2).
pub fn lookup_hue_interval(h: f32, hues: &Table1D, hue_linearity_search_range: &[i32; 2]) -> u32 {
    // Search the given Table for the interval containing the desired hue
    // Returns the upper index of the interval

    // We can narrow the search range based on the hues being almost uniform
    let mut i = table_base::nominal_hue_position_in_uniform_table(h);
    let mut i_lo = std::cmp::max(
        table_base::LOWER_WRAP_INDEX as i32,
        (i as i32).wrapping_add(hue_linearity_search_range[0]),
    ) as u32; // Should be nominal not lower_wrap?
    let mut i_hi = std::cmp::min(
        table_base::UPPER_WRAP_INDEX as i32,
        (i as i32).wrapping_add(hue_linearity_search_range[1]),
    ) as u32;

    while i_lo.wrapping_add(1) < i_hi {
        if h > hues[i as usize] {
            i_lo = i;
        } else {
            i_hi = i;
        }
        i = midpoint_u32(i_lo, i_hi);
    }

    // TODO: should not be needed if we initialise with the correct lo and hi
    std::cmp::max(1, i_hi)
}

/// `(h - h_lo) / (h_hi - h_lo)`.
///
/// Port of `interpolation_weight(float, float, float)` (Transform.cpp:76-79 @ v2.5.2).
#[inline]
pub fn interpolation_weight(h: f32, h_lo: f32, h_hi: f32) -> f32 {
    (h - h_lo) / (h_hi - h_lo)
}

/// The cusp at `t` between the entries `i_hi - 1` and `i_hi` of the cusp table.
///
/// Port of `cusp_from_table` (Transform.cpp:86-89 @ v2.5.2).
#[inline]
pub fn cusp_from_table(i_hi: u32, t: f32, gt: &Table3D) -> F3 {
    let i_hi = i_hi as usize;
    lerp(&gt[i_hi - 1], &gt[i_hi], t)
}

/// The reach's M at the hue `h`, interpolated in the uniform 1-degree table.
///
/// Port of `reach_m_from_table` (Transform.cpp:91-99 @ v2.5.2).
pub fn reach_m_from_table(h: f32, rt: &Table1D) -> f32 {
    let base = table_base::hue_position_in_uniform_table(h);
    let t = h - base as f32; // NOTE assumes uniform 1 degree 360 spacing
    let i_lo = base.wrapping_add(table_base::FIRST_NOMINAL_INDEX as u32);
    let i_hi = i_lo.wrapping_add(1); // NOTE assumes uniform 1 degree 360 spacing

    lerpf(rt[i_lo as usize], rt[i_hi as usize], t)
}

//
// CAM
//

/// Port of `_post_adaptation_cone_response_compression_fwd` (Transform.cpp:105-110 @ v2.5.2).
#[inline]
pub fn post_adaptation_cone_response_compression_fwd_(rc: f32) -> f32 {
    let f_l_y = rc.powf(0.42);
    f_l_y / sse_add(CAM_NL_OFFSET, f_l_y)
}

/// Port of `_post_adaptation_cone_response_compression_inv` (Transform.cpp:112-118 @ v2.5.2).
#[inline]
pub fn post_adaptation_cone_response_compression_inv_(ra: f32) -> f32 {
    let ra_lim = std_min(ra, 0.99);
    let f_l_y = sse_mul(CAM_NL_OFFSET, ra_lim) / (1.0 - ra_lim);
    f_l_y.powf(1.0 / 0.42)
}

/// The compression of `|v|`, with `v`'s sign.
///
/// Port of `post_adaptation_cone_response_compression_fwd` (Transform.cpp:121-128 @ v2.5.2).
pub fn post_adaptation_cone_response_compression_fwd(v: f32) -> f32 {
    let abs_v = v.abs();
    let ra = post_adaptation_cone_response_compression_fwd_(abs_v);
    // Note that std::copysign(1.f, 0.f) returns 1 but the CTL copysign(1.,0.) returns 0.
    ra.copysign(v)
}

/// The inverse compression of `|v|`, with `v`'s sign.
///
/// Port of `post_adaptation_cone_response_compression_inv` (Transform.cpp:130-135 @ v2.5.2).
pub fn post_adaptation_cone_response_compression_inv(v: f32) -> f32 {
    let abs_v = v.abs();
    let rc = post_adaptation_cone_response_compression_inv_(abs_v);
    rc.copysign(v)
}

/// Port of `Achromatic_n_to_J` (Transform.cpp:137-140 @ v2.5.2).
#[inline]
pub fn achromatic_n_to_j(a: f32, cz: f32) -> f32 {
    sse_mul(J_SCALE, a.powf(cz))
}

/// Port of `J_to_Achromatic_n` (Transform.cpp:142-145 @ v2.5.2).
#[inline]
pub fn j_to_achromatic_n(j: f32, inv_cz: f32) -> f32 {
    sse_mul(j, 1.0 / J_SCALE).powf(inv_cz)
}

// Optimization for achromatic values

/// Port of `_A_to_Y` (Transform.cpp:149-154 @ v2.5.2).
#[inline]
pub fn a_to_y_(a: f32, p: &JMhParams) -> f32 {
    let ra = sse_mul(p.a_w_j, a);
    post_adaptation_cone_response_compression_inv_(ra) / p.f_l_n
}

/// Port of `_J_to_Y` (Transform.cpp:156-159 @ v2.5.2).
#[inline]
pub fn j_to_y_(abs_j: f32, p: &JMhParams) -> f32 {
    a_to_y_(j_to_achromatic_n(abs_j, p.inv_cz), p)
}

/// Port of `_Y_to_J` (Transform.cpp:161-166 @ v2.5.2).
#[inline]
pub fn y_to_j_(abs_y: f32, p: &JMhParams) -> f32 {
    let ra = post_adaptation_cone_response_compression_fwd_(sse_mul(abs_y, p.f_l_n));
    achromatic_n_to_j(sse_mul(ra, p.inv_a_w_j), p.cz)
}

/// The J of the luminance `Y`, with `Y`'s sign.
///
/// Port of `Y_to_J` (Transform.cpp:168-173 @ v2.5.2).
pub fn y_to_j(y: f32, p: &JMhParams) -> f32 {
    let abs_y = y.abs();
    let j = y_to_j_(abs_y, p);
    j.copysign(y)
}

/// RGB to the achromatic and opponent responses.
///
/// The matrices' rows in the wheels' orders (`ACES2::RGB_to_Aab`, Linux `0x34b5f0`, Windows
/// `sub_1802f9170`).
///
/// Port of `RGB_to_Aab` (Transform.cpp:175-187 @ v2.5.2).
pub fn rgb_to_aab(rgb: &F3, p: &JMhParams) -> F3 {
    use Row::{M012, M201, X012, X102};
    let (rows_m, rows_a) = if MSVC {
        ([X102, X102, X102], [X102, X012, X012])
    } else {
        ([M012, M012, X012], [X012, M201, M012])
    };
    let rgb_m = mult_f3_f33_rows(rgb, &p.matrix_rgb_to_cam16_c, rows_m);

    let rgb_a = [
        post_adaptation_cone_response_compression_fwd(rgb_m[0]),
        post_adaptation_cone_response_compression_fwd(rgb_m[1]),
        post_adaptation_cone_response_compression_fwd(rgb_m[2]),
    ];

    mult_f3_f33_rows(&rgb_a, &p.matrix_cone_response_to_aab, rows_a)
}

/// The achromatic and opponent responses to lightness, colourfulness and hue (degrees); 0 for
/// an achromatic response at or below 0.
///
/// Port of `Aab_to_JMh` (Transform.cpp:189-201 @ v2.5.2).
pub fn aab_to_jmh(aab: &F3, p: &JMhParams) -> F3 {
    if aab[0] <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let j = achromatic_n_to_j(aab[0], p.cz);
    let m = sse_add(sse_mul(aab[1], aab[1]), sse_mul(aab[2], aab[2])).sqrt();
    let h_rad = aab[2].atan2(aab[1]);
    let h = from_radians_(h_rad); // Call to unwrapped hue version due to atan2 limits

    [j, m, h]
}

/// Port of `RGB_to_JMh` (Transform.cpp:203-208 @ v2.5.2).
pub fn rgb_to_jmh(rgb: &F3, p: &JMhParams) -> F3 {
    let aab = rgb_to_aab(rgb, p);
    aab_to_jmh(&aab, p)
}

/// JMh to Aab with the hue's cosine and sine given.
///
/// `b` is `M * sin_hr` on Windows and `sin_hr * M` on Linux (`ACES2::JMh_to_Aab`, Linux
/// `0x34b960`, Windows `sub_1802f8e50`).
///
/// Port of `JMh_to_Aab(const f3 &, const float &, const float &, const JMhParams &)`
/// (Transform.cpp:210-219 @ v2.5.2).
pub fn jmh_to_aab_with(jmh: &F3, cos_hr: f32, sin_hr: f32, p: &JMhParams) -> F3 {
    let j = jmh[0];
    let m = jmh[1];

    let a_ = j_to_achromatic_n(j, p.inv_cz);
    let a = sse_mul(m, cos_hr);
    let b = mul_swap(m, sin_hr, !MSVC);
    [a_, a, b]
}

/// JMh to Aab, `a` and `b` as `cos_hr * M` and `sin_hr * M`: the order of both wheels' copy
/// inlined in `JMh_to_RGB` (Linux `0x34bcd0`, Windows `sub_1802f8ed0`), the one the renderers
/// run.
///
/// Port of `JMh_to_Aab(const f3 &, const JMhParams &)` (Transform.cpp:221-229 @ v2.5.2).
pub fn jmh_to_aab(jmh: &F3, p: &JMhParams) -> F3 {
    let h = jmh[2];
    let h_rad = to_radians(h);
    let cos_hr = h_rad.cos();
    let sin_hr = h_rad.sin();

    let m = jmh[1];
    let a_ = j_to_achromatic_n(jmh[0], p.inv_cz);
    [a_, sse_mul(cos_hr, m), sse_mul(sin_hr, m)]
}

/// The matrices' rows in the wheels' orders (`ACES2::Aab_to_RGB`, Linux `0x34ba60`, Windows
/// `sub_1802f8c00`; the copies inlined in `JMh_to_RGB` have the same).
///
/// Port of `Aab_to_RGB` (Transform.cpp:231-243 @ v2.5.2).
pub fn aab_to_rgb(aab: &F3, p: &JMhParams) -> F3 {
    use Row::{M012, M201, X012, X102};
    let (rows_a, rows_m) = if MSVC {
        ([X102, X102, X102], [X102, X102, X102])
    } else {
        ([M012, M012, X012], [X012, M201, M012])
    };
    let rgb_a = mult_f3_f33_rows(aab, &p.matrix_aab_to_cone_response, rows_a);

    let rgb_m = [
        post_adaptation_cone_response_compression_inv(rgb_a[0]),
        post_adaptation_cone_response_compression_inv(rgb_a[1]),
        post_adaptation_cone_response_compression_inv(rgb_a[2]),
    ];

    mult_f3_f33_rows(&rgb_m, &p.matrix_cam16_c_to_rgb, rows_m)
}

/// Port of `JMh_to_RGB` (Transform.cpp:245-250 @ v2.5.2).
pub fn jmh_to_rgb(jmh: &F3, p: &JMhParams) -> F3 {
    let aab = jmh_to_aab(jmh, p);
    aab_to_rgb(&aab, p)
}

//
// Tonescale / Chroma compress
//

/// The chroma compression's normalisation: a weighted sum of the hue's first three
/// harmonics, scaled.
///
/// Port of `chroma_compress_norm` (Transform.cpp:283-333 @ v2.5.2), its scalar sum.
pub fn chroma_compress_norm(cos_hr1: f32, sin_hr1: f32, chroma_compress_scale: f32) -> f32 {
    let m = sse_mul;
    let cos_hr2 = m(m(2.0, cos_hr1), cos_hr1) - 1.0;
    let sin_hr2 = m(m(2.0, cos_hr1), sin_hr1);
    let cos_hr3 = m(m(m(4.0, cos_hr1), cos_hr1), cos_hr1) - m(3.0, cos_hr1);
    let sin_hr3 = m(3.0, sin_hr1) - m(m(m(4.0, sin_hr1), sin_hr1), sin_hr1);

    let trig_angles_hr: [f32; 8] = [
        cos_hr1, cos_hr2, cos_hr3, 0.0, sin_hr1, sin_hr2, sin_hr3, 1.0,
    ];
    // TODO: investigate reordering of the entries so we are summing equal magnitude values first?
    const WEIGHTS: [f32; 8] = [
        11.34072, 16.46899, 7.88380, 0.0, 14.66441, -6.37224, 9.19364, 77.12896,
    ];

    let mut sum = m(WEIGHTS[0], trig_angles_hr[0]);
    for i in [1, 2, 4, 5, 6] {
        sum = sse_add(sum, m(WEIGHTS[i], trig_angles_hr[i]));
    }
    let big_m = sse_add(sum, WEIGHTS[7]);

    m(big_m, chroma_compress_scale) // TODO: is it worth prescaling the above weights?
}

/// The operand orders of a compiled copy of [`toe_fwd`], which differ between the wheels'
/// copies; every copy computes `minus_ac` as `(k3 * k2) * x` and the discriminant as
/// `4 * minus_ac + minus_b * minus_b`.
#[derive(Clone, Copy, Debug)]
pub struct ToeFwdOrder {
    /// `k2 * k2 + k1_in * k1_in`, not `k1_in * k1_in + k2 * k2`.
    pub k2_squared_first: bool,
    /// `(k1 + limit) / (k2 + limit)`, not `(limit + k1) / (limit + k2)`.
    pub k_plus_limit: bool,
    /// `x * k3`, not `k3 * x`.
    pub x_times_k3: bool,
    /// `sqrt(..) + minus_b`, not `minus_b + sqrt(..)`.
    pub root_first: bool,
}

/// Port of `toe_fwd` (Transform.cpp:335-349 @ v2.5.2), in the operand orders `o`.
#[inline]
pub fn toe_fwd(x: f32, limit: f32, k1_in: f32, k2_in: f32, o: ToeFwdOrder) -> f32 {
    if x > limit {
        return x;
    }

    let m = sse_mul;
    let k2 = std_max(k2_in, 0.001);
    let k1 = add_swap(m(k1_in, k1_in), m(k2, k2), o.k2_squared_first).sqrt();
    let k3 = add_swap(limit, k1, o.k_plus_limit) / add_swap(limit, k2, o.k_plus_limit);

    let minus_b = sse_sub(mul_swap(k3, x, o.x_times_k3), k1);
    let minus_ac = m(m(k3, k2), x); // a is 1.0
    // a is 1.0, mins_b squared == b^2
    let root = sse_add(m(minus_ac, 4.0), m(minus_b, minus_b)).sqrt();
    m(add_swap(minus_b, root, o.root_first), 0.5)
}

/// `(k1 + limit) / (k2 + limit)`, `k1 * x + x * x` on Windows (both copies inlined in
/// `chroma_compress_inv`, `sub_1802fa250`), `(k1 + limit) / (limit + k2)`, `x * x + x * k1` on
/// Linux (both copies in `ACES2::chroma_compress_inv`, `0x34c8b0`); both wheels compute
/// `k3 * (k2 + x)`.
///
/// Port of `toe_inv` (Transform.cpp:351-362 @ v2.5.2).
#[inline]
pub fn toe_inv(x: f32, limit: f32, k1_in: f32, k2_in: f32) -> f32 {
    if x > limit {
        return x;
    }

    let m = sse_mul;
    let k2 = std_max(k2_in, 0.001);
    let k1 = sse_add(m(k1_in, k1_in), m(k2, k2)).sqrt();
    let k3 = sse_add(k1, limit) / add_swap(limit, k2, MSVC);
    let numerator = if MSVC {
        sse_add(m(k1, x), m(x, x))
    } else {
        sse_add(m(x, x), m(x, k1))
    };
    numerator / m(k3, sse_add(k2, x))
}

/// The ACES 2.0 tone scale of a luminance, or its inverse.
///
/// Port of `aces_tonescale<inverse>` (Transform.cpp:364-379 @ v2.5.2).
#[inline]
pub fn aces_tonescale<const INVERSE: bool>(y_in: f32, pt: &ToneScaleParams) -> f32 {
    let m = sse_mul;
    if INVERSE {
        let y_ts_norm = y_in / REFERENCE_LUMINANCE;
        // TODO: could eliminate max in the context of the full tonescale
        let z = std_max(0.0, std_min(pt.inverse_limit, y_ts_norm));
        let f = sse_add(z, m(z, sse_add(m(4.0, pt.t_1), z)).sqrt()) / 2.0;
        return pt.s_2 / ((pt.m_2 / f).powf(1.0 / pt.g) - 1.0);
    }

    let f = m(pt.m_2, (y_in / sse_add(y_in, pt.s_2)).powf(pt.g));
    // max prevents -ve values being output also handles division by zero possibility
    m(std_max(0.0, m(f, f) / sse_add(f, pt.t_1)), pt.n_r)
}

/// The tone scale in Y of a J, with J's sign.
///
/// Port of `tonescale<inverse>` (Transform.cpp:381-390 @ v2.5.2).
#[inline]
pub fn tonescale<const INVERSE: bool>(j: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    // Tonescale applied in Y (convert to and from J)
    let j_abs = j.abs();
    let y_in = j_to_y_(j_abs, p);
    let y_out = aces_tonescale::<INVERSE>(y_in, pt);
    let j_out = y_to_j_(y_out, p);
    j_out.copysign(j)
}

/// Port of `tonescale_fwd` (Transform.cpp:392-395 @ v2.5.2).
pub fn tonescale_fwd(j: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    tonescale::<false>(j, p, pt)
}

/// Port of `tonescale_inv` (Transform.cpp:397-400 @ v2.5.2).
pub fn tonescale_inv(j: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    tonescale::<true>(j, p, pt)
}

/// The tone scale of an achromatic response, as a J with A's sign.
///
/// Port of `tonescale_A_to_J<inverse>` (Transform.cpp:403-410 @ v2.5.2).
#[inline]
pub fn tonescale_a_to_j<const INVERSE: bool>(a: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    let y_in = a_to_y_(a, p);
    let y_out = aces_tonescale::<INVERSE>(y_in, pt);
    let j_out = y_to_j_(y_out, p);
    j_out.copysign(a)
}

/// Port of `tonescale_A_to_J_fwd` (Transform.cpp:412-415 @ v2.5.2).
pub fn tonescale_a_to_j_fwd(a: f32, p: &JMhParams, pt: &ToneScaleParams) -> f32 {
    tonescale_a_to_j::<false>(a, p, pt)
}

/// The chroma compression of a JMh, given its tone-scaled J and the hue's normalisation.
///
/// In the wheels' orders (Linux `ACES2::chroma_compress_fwd`, `0x34c4d0`, its two copies of
/// `toe_fwd` inlined; Windows `sub_1802fa060`, which calls `toe_fwd` as `sub_1802fd040`):
/// `limit` is `reachMaxM * pow(..) / Mnorm` on Linux, and the colourfulness `pow(..) * M` on
/// Windows.
///
/// Port of `chroma_compress_fwd` (Transform.cpp:417-439 @ v2.5.2).
pub fn chroma_compress_fwd(
    jmh: &F3,
    j_ts: f32,
    mnorm: f32,
    pr: &ResolvedSharedCompressionParameters,
    pc: &ChromaCompressParams,
) -> F3 {
    let j = jmh[0];
    let mm = jmh[1];
    let h = jmh[2];

    let m = sse_mul;
    let mut m_cp = mm;

    // Windows calls one copy of `toe_fwd`; Linux inlines two.
    const TOE_WINDOWS: ToeFwdOrder = ToeFwdOrder {
        k2_squared_first: true,
        k_plus_limit: true,
        x_times_k3: false,
        root_first: true,
    };
    let (toe_1, toe_2) = if MSVC {
        (TOE_WINDOWS, TOE_WINDOWS)
    } else {
        (
            ToeFwdOrder {
                k2_squared_first: true,
                k_plus_limit: false,
                x_times_k3: true,
                root_first: true,
            },
            ToeFwdOrder {
                k2_squared_first: false,
                k_plus_limit: false,
                x_times_k3: false,
                root_first: false,
            },
        )
    };

    if mm != 0.0 {
        let nj = j_ts / pr.limit_j_max;
        let snj = std_max(0.0, 1.0 - nj);
        let limit = mul_swap(nj.powf(pr.model_gamma_inv), pr.reach_max_m, !MSVC) / mnorm;

        m_cp = mul_swap(mm, (j_ts / j).powf(pr.model_gamma_inv), MSVC);
        m_cp /= mnorm;
        m_cp = sse_sub(
            limit,
            toe_fwd(
                sse_sub(limit, m_cp),
                limit - 0.001,
                m(snj, pc.sat),
                sse_add(m(nj, nj), pc.sat_thr).sqrt(),
                toe_1,
            ),
        );
        m_cp = toe_fwd(m_cp, limit, m(nj, pc.compr), snj, toe_2);
        m_cp = m(m_cp, mnorm);
    }

    [j_ts, m_cp, h]
}

/// The inverse of [`chroma_compress_fwd`], given the original J.
///
/// In the wheels' orders (Linux `ACES2::chroma_compress_inv`, `0x34c8b0`; Windows
/// `sub_1802fa250`; both inline the two copies of `toe_inv`): `limit` is
/// `reachMaxM * pow(..) / Mnorm` and the colourfulness `Mnorm * M` on Linux, the last product
/// `pow(..) * M` on Windows.
///
/// Port of `chroma_compress_inv` (Transform.cpp:441-462 @ v2.5.2).
pub fn chroma_compress_inv(
    jmh: &F3,
    j: f32,
    mnorm: f32,
    pr: &ResolvedSharedCompressionParameters,
    pc: &ChromaCompressParams,
) -> F3 {
    let j_ts = jmh[0];
    let m_cp = jmh[1];
    let h = jmh[2];

    let m = sse_mul;
    let mut mm = m_cp;

    if m_cp != 0.0 {
        let nj = j_ts / pr.limit_j_max;
        let snj = std_max(0.0, 1.0 - nj);
        let limit = mul_swap(nj.powf(pr.model_gamma_inv), pr.reach_max_m, !MSVC) / mnorm;

        mm = m_cp / mnorm;
        mm = toe_inv(mm, limit, m(nj, pc.compr), snj);
        mm = sse_sub(
            limit,
            toe_inv(
                sse_sub(limit, mm),
                limit - 0.001,
                m(snj, pc.sat),
                sse_add(m(nj, nj), pc.sat_thr).sqrt(),
            ),
        );
        mm = mul_swap(mm, mnorm, !MSVC);
        mm = mul_swap(mm, (j_ts / j).powf(-pr.model_gamma_inv), MSVC);
    }

    [j, mm, h]
}

/// The c * z non-linearity of the model: `surround[1] * (1.48 + sqrt(Y_b / 100))`.
///
/// Port of `model_gamma` (Transform.cpp:464-468 @ v2.5.2).
#[inline]
pub fn model_gamma() -> f32 {
    // c * z nonlinearity
    sse_mul(SURROUND[1], 1.48 + (Y_B / REFERENCE_LUMINANCE).sqrt())
}

/// The JMh model's matrices and constants for the primaries `prims`: the CAM16 viewing
/// conditions (`L_A`, `Y_b`, a dim surround), the white's adaptation folded into the RGB to
/// CAM16 matrix, and the cone responses' Aab matrix. A singular matrix on the way (degenerate
/// primaries) is refused with `MatrixArray::inverse`'s exception.
///
/// Port of `init_JMhParams` (src/OpenColorIO/ops/fixedfunction/ACES2/Transform.cpp:470-545 @
/// v2.5.2).
pub fn init_jmh_params(prims: &Primaries) -> Result<JMhParams> {
    let m = sse_mul;
    #[rustfmt::skip]
    let base_cone_response_to_aab: M33f = [
        2.0, 1.0, 1.0 / 20.0,
        1.0, -12.0 / 11.0, 1.0 / 11.0,
        1.0 / 9.0, 1.0 / 9.0, -2.0 / 9.0,
    ];

    let matrix_16 = xyz_to_rgb_f33(&cam16::PRIMARIES)?;
    let rgb_to_xyz = rgb_to_xyz_f33(prims)?;
    let xyz_w = mult_f3_f33(&f3_from_f(REFERENCE_LUMINANCE), &rgb_to_xyz);

    let y_w = xyz_w[1];

    let rgb_w = mult_f3_f33(&xyz_w, &matrix_16);

    // Viewing condition dependent parameters
    const K: f32 = 1.0 / (5.0 * L_A + 1.0);
    const K4: f32 = K * K * K * K;
    let f_l = sse_add(
        m(m(0.2, K4), 5.0 * L_A),
        m(m(0.1, (1.0 - K4).powf(2.0)), (5.0 * L_A).powf(1.0 / 3.0)),
    );

    let f_l_n = f_l / REFERENCE_LUMINANCE;
    let cz = model_gamma();
    let inv_cz = 1.0 / cz;

    let d_rgb = [
        m(f_l_n, y_w) / rgb_w[0],
        m(f_l_n, y_w) / rgb_w[1],
        m(f_l_n, y_w) / rgb_w[2],
    ];

    let rgb_wc = [
        m(d_rgb[0], rgb_w[0]),
        m(d_rgb[1], rgb_w[1]),
        m(d_rgb[2], rgb_w[2]),
    ];

    let rgb_aw = [
        post_adaptation_cone_response_compression_fwd(rgb_wc[0]),
        post_adaptation_cone_response_compression_fwd(rgb_wc[1]),
        post_adaptation_cone_response_compression_fwd(rgb_wc[2]),
    ];

    // TODO: this scale maybe ill conditioning the matrix
    let cone_response_to_aab = mult_f33_f33(
        &scale_f33(&IDENTITY_M33, &f3_from_f(CAM_NL_SCALE)),
        &base_cone_response_to_aab,
    );
    let a_w = sse_add(
        sse_add(
            m(cone_response_to_aab[0], rgb_aw[0]),
            m(cone_response_to_aab[1], rgb_aw[1]),
        ),
        m(cone_response_to_aab[2], rgb_aw[2]),
    );
    let a_w_j = post_adaptation_cone_response_compression_fwd_(f_l);
    let inv_a_w_j = 1.0 / a_w_j;

    // Note we are prescaling the CAM16 LMS responses to directly provide for chromatic
    // adaptation.
    let matrix_rgb_to_cam16 = mult_f33_f33(
        &rgb_to_rgb_f33(prims, &cam16::PRIMARIES)?,
        &scale_f33(&IDENTITY_M33, &f3_from_f(REFERENCE_LUMINANCE)),
    );
    let matrix_rgb_to_cam16_c =
        mult_f33_f33(&scale_f33(&IDENTITY_M33, &d_rgb), &matrix_rgb_to_cam16);
    let matrix_cam16_c_to_rgb = invert_f33(&matrix_rgb_to_cam16_c)?;

    let s = |v: f32| m(m(v, 43.0), SURROUND[2]);
    let c = &cone_response_to_aab;
    let matrix_cone_response_to_aab: M33f = [
        c[0] / a_w,
        c[1] / a_w,
        c[2] / a_w,
        s(c[3]),
        s(c[4]),
        s(c[5]),
        s(c[6]),
        s(c[7]),
        s(c[8]),
    ];
    let matrix_aab_to_cone_response = invert_f33(&matrix_cone_response_to_aab)?;

    Ok(JMhParams {
        matrix_rgb_to_cam16_c,
        matrix_cam16_c_to_rgb,
        matrix_cone_response_to_aab,
        matrix_aab_to_cone_response,
        f_l_n,
        cz,
        inv_cz,
        a_w_j,
        inv_a_w_j,
    })
}

/// The RGB corner of the unit cube of cusp `corner`, in the order R, Y, G, C, B, M, so that
/// hues rotate in the correct order.
///
/// Port of `generate_unit_cube_cusp_corners` (Transform.cpp:547-554 @ v2.5.2).
#[inline]
pub fn generate_unit_cube_cusp_corners(corner: u32) -> F3 {
    let n = CUSP_CORNER_COUNT as u32;
    let lit = |shift: u32| f32::from(u8::from(corner.wrapping_add(shift) % n < 3));
    [lit(1), lit(5), lit(3)]
}

/// The limiting gamut's cusp corners in RGB and JMh, in a cycle with the lowest hue at [1],
/// and the wrapped copies of the last and first at [0] and [7].
///
/// Port of `build_limiting_cusp_corners_tables` (Transform.cpp:556-588 @ v2.5.2).
pub fn build_limiting_cusp_corners_tables(
    rgb_corners: &mut [F3; TOTAL_CORNERS],
    jmh_corners: &mut [F3; TOTAL_CORNERS],
    params: &JMhParams,
    peak_luminance: f32,
) {
    // We calculate the RGB and JMh values for the limiting gamut cusp corners
    // They are then arranged into a cycle with the lowest JMh value at [1] to allow for hue
    // wrapping
    let mut temp_rgb_corners = [[0.0f32; 3]; CUSP_CORNERS];
    let mut temp_jmh_corners = [[0.0f32; 3]; CUSP_CORNERS];
    let mut min_index = 0;
    for i in 0..CUSP_CORNERS {
        temp_rgb_corners[i] = mult_f_f3(
            peak_luminance / REFERENCE_LUMINANCE,
            &generate_unit_cube_cusp_corners(i as u32),
        );
        temp_jmh_corners[i] = rgb_to_jmh(&temp_rgb_corners[i], params);
        if temp_jmh_corners[i][2] < temp_jmh_corners[min_index][2] {
            min_index = i;
        }
    }

    // Rotate entries placing lowest at [1] (not [0])
    for i in 0..CUSP_CORNERS {
        rgb_corners[i + 1] = temp_rgb_corners[(i + min_index) % CUSP_CORNERS];
        jmh_corners[i + 1] = temp_jmh_corners[(i + min_index) % CUSP_CORNERS];
    }

    // Copy end elements to create a cycle
    rgb_corners[0] = rgb_corners[CUSP_CORNERS];
    rgb_corners[CUSP_CORNERS + 1] = rgb_corners[1];
    jmh_corners[0] = jmh_corners[CUSP_CORNERS];
    jmh_corners[CUSP_CORNERS + 1] = jmh_corners[1];

    // Wrap the hues, to maintain monotonicity these entries will fall outside [0.0, hue_limit)
    jmh_corners[0][2] -= HUE_LIMIT;
    jmh_corners[CUSP_CORNERS + 1][2] += HUE_LIMIT;
}

/// The reach gamut's corners at the J `limit_j`, in JMh: each unit corner scaled, by
/// bisection on the achromatic response, until its J reaches `limit_j`; in a cycle as
/// [`build_limiting_cusp_corners_tables`] arranges them.
///
/// Port of `find_reach_corners_table` (Transform.cpp:590-644 @ v2.5.2).
pub fn find_reach_corners_table(
    jmh_corners: &mut [F3; TOTAL_CORNERS],
    params: &JMhParams,
    limit_j: f32,
    maximum_source: f32,
) {
    let mut temp_jmh_corners = [[0.0f32; 3]; CUSP_CORNERS];
    let limit_a = j_to_achromatic_n(limit_j, params.inv_cz);

    let mut min_index = 0;
    for i in 0..CUSP_CORNERS {
        let rgb_vector = generate_unit_cube_cusp_corners(i as u32);

        let mut lower = 0.0f32;
        let mut upper = maximum_source;
        while (upper - lower) > REACH_CUSP_TOLERANCE {
            let test = midpoint(lower, upper);
            let test_corner = mult_f_f3(test, &rgb_vector);
            let a = rgb_to_aab(&test_corner, params)[0];
            if a < limit_a {
                lower = test;
            } else {
                upper = test;
            }
            if a == limit_a {
                break;
            }
        }
        temp_jmh_corners[i] = rgb_to_jmh(&mult_f_f3(upper, &rgb_vector), params);

        if temp_jmh_corners[i][2] < temp_jmh_corners[min_index][2] {
            min_index = i;
        }
    }

    // Rotate entries placing lowest at [1] (not [0])
    for i in 0..CUSP_CORNERS {
        jmh_corners[i + 1] = temp_jmh_corners[(i + min_index) % CUSP_CORNERS];
    }

    // Copy end elements to create a cycle
    jmh_corners[0] = jmh_corners[CUSP_CORNERS];
    jmh_corners[CUSP_CORNERS + 1] = jmh_corners[1];

    // Wrap the hues, to maintain monotonicity these entries will fall outside [0.0, hue_limit)
    jmh_corners[0][2] -= HUE_LIMIT;
    jmh_corners[CUSP_CORNERS + 1][2] += HUE_LIMIT;
}

/// The unique hues of the two corner cycles' nominal entries, merged in order; returns how
/// many. Where hues that don't compare (NaN) make upstream read past the corners or write past
/// the hues, the port refuses the parameters ([`CORNERS_OVERRUN`], U-32).
///
/// Port of `extract_sorted_cube_hues` (Transform.cpp:646-681 @ v2.5.2).
pub fn extract_sorted_cube_hues(
    sorted_hues: &mut [f32; MAX_SORTED],
    reach_jmh: &[F3; TOTAL_CORNERS],
    display_jmh: &[F3; TOTAL_CORNERS],
) -> Result<u32> {
    // Basic merge of 2 sorted arrays, extracting the unique hues.
    // Return the count of the unique hues
    let mut idx = 0;
    let mut reach_idx = 1;
    let mut display_idx = 1;
    while reach_idx < CUSP_CORNERS + 1 || display_idx < CUSP_CORNERS + 1 {
        if reach_idx >= TOTAL_CORNERS || display_idx >= TOTAL_CORNERS || idx >= MAX_SORTED {
            return Err(Exception::new(CORNERS_OVERRUN));
        }
        let reach_hue = reach_jmh[reach_idx][2];
        let display_hue = display_jmh[display_idx][2];
        if reach_hue == display_hue {
            sorted_hues[idx] = reach_hue;
            reach_idx += 1;
            display_idx += 1; // When equal consume both
        } else if reach_hue < display_hue {
            sorted_hues[idx] = reach_hue;
            reach_idx += 1;
        } else {
            sorted_hues[idx] = display_hue;
            display_idx += 1;
        }
        idx += 1;
    }
    Ok(idx as u32)
}

/// `samples` evenly spaced hues from `lower` towards `upper` into `hue_table` from `base`.
/// Where upstream would write past the table, the port refuses the parameters
/// ([`CORNERS_OVERRUN`], U-32).
///
/// Port of `build_hue_sample_interval` (Transform.cpp:683-690 @ v2.5.2).
pub fn build_hue_sample_interval(
    samples: u32,
    lower: f32,
    upper: f32,
    hue_table: &mut Table1D,
    base: u32,
) -> Result<()> {
    if u64::from(base) + u64::from(samples) > table_base::TOTAL_SIZE as u64 {
        return Err(Exception::new(CORNERS_OVERRUN));
    }
    let delta = (upper - lower) / samples as f32;
    for i in 0..samples {
        hue_table[(base + i) as usize] = sse_add(lower, sse_mul(i as f32, delta));
    }
    Ok(())
}

/// The hue table: about one sample per degree, with the sorted unique corner hues among the
/// samples, and the wrapped entries.
///
/// Port of `build_hue_table` (Transform.cpp:692-736 @ v2.5.2), its two `BUG` notes included:
/// where they would make upstream write past the table, the port refuses the parameters
/// ([`CORNERS_OVERRUN`], U-32).
pub fn build_hue_table(
    hue_table: &mut Table1D,
    sorted_hues: &[f32; MAX_SORTED],
    unique_hues: u32,
) -> Result<()> {
    const NOMINAL: u32 = table_base::NOMINAL_SIZE as u32;
    let ideal_spacing = NOMINAL as f32 / HUE_LIMIT;
    let mut samples_count = [0u32; 2 * CUSP_CORNERS + 2];
    let mut last_idx = u32::MAX;
    // Ensure we can always sample at 0.0 hue
    let mut min_index: u32 = if sorted_hues[0] == 0.0 { 0 } else { 1 };
    for hue_idx in 0..unique_hues as usize {
        // BUG: "hue_table.size - 1" will fail if we have multiple hues mapping near the top of
        // the table
        let mut nominal_idx = std::cmp::min(
            std::cmp::max(
                f32_to_u32(sse_mul(sorted_hues[hue_idx], ideal_spacing).round()),
                min_index,
            ),
            NOMINAL - 1,
        );
        if last_idx == nominal_idx {
            // Last two hues should sample at same index, need to adjust them
            // Adjust previous sample down if we can
            if hue_idx > 1
                && samples_count[hue_idx - 2] != samples_count[hue_idx - 1].wrapping_sub(1)
            {
                samples_count[hue_idx - 1] = samples_count[hue_idx - 1].wrapping_sub(1);
            } else {
                nominal_idx = nominal_idx.wrapping_add(1);
            }
        }
        samples_count[hue_idx] = std::cmp::min(nominal_idx, NOMINAL - 1);
        last_idx = nominal_idx;
        min_index = nominal_idx;
    }

    let mut total_samples: u32 = 0;
    // Special cases for ends
    let mut i = 0usize;
    build_hue_sample_interval(
        samples_count[i],
        0.0,
        sorted_hues[i],
        hue_table,
        total_samples.wrapping_add(1),
    )?;
    total_samples = total_samples.wrapping_add(samples_count[i]);
    i += 1;
    while i != unique_hues as usize {
        let samples = samples_count[i].wrapping_sub(samples_count[i - 1]);
        build_hue_sample_interval(
            samples,
            sorted_hues[i - 1],
            sorted_hues[i],
            hue_table,
            total_samples.wrapping_add(1),
        )?;
        total_samples = total_samples.wrapping_add(samples);
        i += 1;
    }
    // BUG: could break if we are unlucky with samples all being used up by this point
    build_hue_sample_interval(
        NOMINAL.wrapping_sub(total_samples),
        sorted_hues[i - 1],
        HUE_LIMIT,
        hue_table,
        total_samples.wrapping_add(1),
    )?;

    hue_table[table_base::LOWER_WRAP_INDEX] = hue_table[table_base::LAST_NOMINAL_INDEX] - HUE_LIMIT;
    hue_table[table_base::UPPER_WRAP_INDEX] =
        sse_add(hue_table[table_base::FIRST_NOMINAL_INDEX], HUE_LIMIT);
    hue_table[table_base::UPPER_WRAP_INDEX + 1] =
        sse_add(hue_table[table_base::FIRST_NOMINAL_INDEX + 1], HUE_LIMIT);
    Ok(())
}

/// The limiting gamut's cusp (J, M) at `hue`: by bisection along the RGB segment between the
/// two cusp corners around the hue. `previous` holds the last segment and position, to
/// resume from there.
///
/// Port of `find_display_cusp_for_hue` (Transform.cpp:738-808 @ v2.5.2).
pub fn find_display_cusp_for_hue(
    hue: f32,
    rgb_corners: &[F3; TOTAL_CORNERS],
    jmh_corners: &[F3; TOTAL_CORNERS],
    params: &JMhParams,
    previous: &mut [f32; 2],
) -> [f32; 2] {
    // This works by finding the required line segment between two of the XYZ cusp corners,
    // then binary searching along the line calculating the JMh of points along the line till
    // we find the required value. All values on the line segments are valid cusp locations.

    let mut upper_corner = 1;
    for (i, corner) in jmh_corners.iter().enumerate().skip(upper_corner) {
        if corner[2] > hue {
            upper_corner = i;
            break;
        }
    }
    let lower_corner = upper_corner - 1;

    // hue should now be within [lower_corner, upper_corner), handle exact match
    if jmh_corners[lower_corner][2] == hue {
        return [jmh_corners[lower_corner][0], jmh_corners[lower_corner][1]];
    }

    // search by lerping between RGB corners for the hue
    let cusp_lower = rgb_corners[lower_corner];
    let cusp_upper = rgb_corners[upper_corner];

    // If we are still on the same segment start from where we left off
    let mut lower_t = if upper_corner as f32 == previous[0] {
        previous[1]
    } else {
        0.0
    };
    let mut upper_t = 1.0f32;

    // There is an edge case where we need to search towards the range when across the [0.0f,
    // hue_limit) boundary each edge needs the directions swapped. This is handled by comparing
    // against the appropriate corner to make sure we are still in the expected range between
    // the lower and upper corner hue limits
    while (upper_t - lower_t) > DISPLAY_CUSP_TOLERANCE {
        let sample_t = midpoint(lower_t, upper_t);
        let sample = lerp(&cusp_lower, &cusp_upper, sample_t);
        let jmh = rgb_to_jmh(&sample, params);
        if jmh[2] < jmh_corners[lower_corner][2] {
            upper_t = sample_t;
        } else if jmh[2] >= jmh_corners[upper_corner][2] {
            lower_t = sample_t;
        } else if jmh[2] > hue {
            upper_t = sample_t;
        } else {
            lower_t = sample_t;
        }
    }

    // Use the midpoint of the final interval for the actual samples
    let sample_t = midpoint(lower_t, upper_t);
    let sample = lerp(&cusp_lower, &cusp_upper, sample_t);
    let jmh = rgb_to_jmh(&sample, params);

    previous[0] = upper_corner as f32;
    previous[1] = sample_t;

    [jmh[0], jmh[1]]
}

/// The cusp table: the limiting gamut's cusp J and smoothed M at each hue of `hue_table`,
/// and the hue; the wrapped entries copied.
///
/// Port of `build_cusp_table` (Transform.cpp:810-834 @ v2.5.2).
pub fn build_cusp_table(
    hue_table: &Table1D,
    rgb_corners: &[F3; TOTAL_CORNERS],
    jmh_corners: &[F3; TOTAL_CORNERS],
    params: &JMhParams,
) -> Table3D {
    let mut previous = [0.0f32, 0.0];
    let mut output_table: Table3D = [[0.0; 3]; table_base::TOTAL_SIZE];
    for i in table_base::FIRST_NOMINAL_INDEX..table_base::UPPER_WRAP_INDEX {
        let hue = hue_table[i];
        let jm = find_display_cusp_for_hue(hue, rgb_corners, jmh_corners, params, &mut previous);
        output_table[i][0] = jm[0];
        output_table[i][1] = sse_mul(jm[1], 1.0 + SMOOTH_M * SMOOTH_CUSPS);
        output_table[i][2] = hue;
    }

    // Copy extra entries to ease the code to handle hues wrapping around
    let (lower, last) = (table_base::LOWER_WRAP_INDEX, table_base::LAST_NOMINAL_INDEX);
    let (upper, first) = (
        table_base::UPPER_WRAP_INDEX,
        table_base::FIRST_NOMINAL_INDEX,
    );
    output_table[lower][0] = output_table[last][0];
    output_table[lower][1] = output_table[last][1];
    output_table[lower][2] = hue_table[lower];
    output_table[upper][0] = output_table[first][0];
    output_table[upper][1] = output_table[first][1];
    output_table[upper][2] = hue_table[upper];
    output_table[upper + 1][0] = output_table[first + 1][0];
    output_table[upper + 1][1] = output_table[first + 1][1];
    output_table[upper + 1][2] = hue_table[upper + 1];
    output_table
}

/// The hue table and the cusp table: the hues sampled as uniformly as possible while
/// including the corners of the limiting gamut and of the reach gamut at `limit_J_max`.
///
/// Port of `make_uniform_hue_gamut_table` (Transform.cpp:836-855 @ v2.5.2).
pub fn make_uniform_hue_gamut_table(
    reach_params: &JMhParams,
    params: &JMhParams,
    peak_luminance: f32,
    forward_limit: f32,
    sp: &SharedCompressionParameters,
    hue_table: &mut Table1D,
) -> Result<Table3D> {
    let mut reach_jmh_corners = [[0.0f32; 3]; TOTAL_CORNERS];
    let mut limiting_rgb_corners = [[0.0f32; 3]; TOTAL_CORNERS];
    let mut limiting_jmh_corners = [[0.0f32; 3]; TOTAL_CORNERS];
    let mut sorted_hues = [0.0f32; MAX_SORTED];

    find_reach_corners_table(
        &mut reach_jmh_corners,
        reach_params,
        sp.limit_j_max,
        forward_limit,
    );
    build_limiting_cusp_corners_tables(
        &mut limiting_rgb_corners,
        &mut limiting_jmh_corners,
        params,
        peak_luminance,
    );
    let unique_hues =
        extract_sorted_cube_hues(&mut sorted_hues, &reach_jmh_corners, &limiting_jmh_corners)?;
    build_hue_table(hue_table, &sorted_hues, unique_hues)?;
    Ok(build_cusp_table(
        hue_table,
        &limiting_rgb_corners,
        &limiting_jmh_corners,
        params,
    ))
}

/// Port of `any_below_zero` (Transform.cpp:857-860 @ v2.5.2).
#[inline]
pub fn any_below_zero(rgb: &F3) -> bool {
    rgb[0] < 0.0 || rgb[1] < 0.0 || rgb[2] < 0.0
}

/// The reach's M at `limit_J_max` for each degree of hue: the largest M whose RGB in the reach
/// gamut has no negative component, found by stepping then bisecting; the wrapped entries
/// copied.
///
/// Port of `make_reach_m_table` (Transform.cpp:862-910 @ v2.5.2).
pub fn make_reach_m_table(params: &JMhParams, limit_j_max: f32) -> Table1D {
    let mut gamut_reach_table: Table1D = [0.0; table_base::TOTAL_SIZE];

    for i in 0..table_base::NOMINAL_SIZE as u32 {
        let hue = table_base::base_hue_for_position(i);

        const SEARCH_RANGE: f32 = 50.0;
        const SEARCH_MAXIMUM: f32 = 1300.0; // TODO: magic limit
        let mut low = 0.0f32;
        let mut high = low + SEARCH_RANGE;
        let mut outside = false;

        while !outside && high < SEARCH_MAXIMUM {
            let search_jmh = [limit_j_max, high, hue];
            let new_limit_rgb = jmh_to_rgb(&search_jmh, params);
            outside = any_below_zero(&new_limit_rgb);
            if !outside {
                low = high;
                high += SEARCH_RANGE;
            }
        }

        while high - low > 1e-2 {
            let sample_m = sse_add(high, low) / 2.0;
            let search_jmh = [limit_j_max, sample_m, hue];
            let new_limit_rgb = jmh_to_rgb(&search_jmh, params);
            outside = any_below_zero(&new_limit_rgb);
            if outside {
                high = sample_m;
            } else {
                low = sample_m;
            }
        }

        gamut_reach_table[i as usize + table_base::BASE_INDEX] = high;
    }
    gamut_reach_table[table_base::LOWER_WRAP_INDEX] =
        gamut_reach_table[table_base::LAST_NOMINAL_INDEX];
    gamut_reach_table[table_base::UPPER_WRAP_INDEX] =
        gamut_reach_table[table_base::FIRST_NOMINAL_INDEX];
    gamut_reach_table[table_base::UPPER_WRAP_INDEX + 1] =
        gamut_reach_table[table_base::FIRST_NOMINAL_INDEX + 1];

    gamut_reach_table
}

/// Whether a component of `rgb` is above `max_rgb_test_val`: outside the top of the gamut.
///
/// Port of `outside_hull` (Transform.cpp:912-916 @ v2.5.2).
#[inline]
pub fn outside_hull(rgb: &F3, max_rgb_test_val: f32) -> bool {
    // limit value, once we cross this value, we are outside of the top gamut shell
    rgb[0] > max_rgb_test_val || rgb[1] > max_rgb_test_val || rgb[2] > max_rgb_test_val
}

/// The focus gain: `limit_J_max * focus_dist`, raised above the analytical threshold.
///
/// Port of `get_focus_gain` (Transform.cpp:918-930 @ v2.5.2).
#[inline]
pub fn get_focus_gain(j: f32, analytical_threshold: f32, limit_j_max: f32, focus_dist: f32) -> f32 {
    let mut gain = sse_mul(limit_j_max, focus_dist);
    if j > analytical_threshold {
        // Approximate inverse required above threshold due to the introduction of J in the
        // calculation
        let mut gain_adjustment =
            ((limit_j_max - analytical_threshold) / std_max(0.0001, limit_j_max - j)).log10();
        gain_adjustment = sse_add(sse_mul(gain_adjustment, gain_adjustment), 1.0);
        gain = sse_mul(gain, gain_adjustment);
    }
    gain
}

/// The J where the compression line through (J, M) meets the J axis.
///
/// Port of `solve_J_intersect` (Transform.cpp:932-953 @ v2.5.2).
pub fn solve_j_intersect(j: f32, m: f32, focus_j: f32, max_j: f32, slope_gain: f32) -> f32 {
    let mul = sse_mul;
    let m_scaled = m / slope_gain;
    let a = m_scaled / focus_j;

    if j < focus_j {
        let b = 1.0 - m_scaled;
        let c = -j;
        let det = mul(b, b) - mul(mul(4.0, a), c);
        let root = det.sqrt();
        mul(-2.0, c) / sse_add(b, root)
    } else {
        let b = -sse_add(sse_add(1.0, m_scaled), mul(max_j, a));
        let c = sse_add(mul(max_j, m_scaled), j);
        let det = mul(b, b) - mul(mul(4.0, a), c);
        let root = det.sqrt();
        mul(-2.0, c) / (b - root)
    }
}

/// A smooth minimum about the scaled reference, based upon a cubic polynomial.
///
/// Port of `smin_scaled` (Transform.cpp:955-961 @ v2.5.2).
#[inline]
pub fn smin_scaled(a: f32, b: f32, scale_reference: f32) -> f32 {
    let mul = sse_mul;
    let s_scaled = mul(SMOOTH_CUSPS, scale_reference);
    let h = std_max(s_scaled - (a - b).abs(), 0.0) / s_scaled;
    std_min(a, b) - mul(mul(mul(mul(h, h), h), s_scaled), 1.0 / 6.0)
}

/// The slope of the compression vector at the J axis intersection.
///
/// Port of `compute_compression_vector_slope` (Transform.cpp:963-967 @ v2.5.2).
#[inline]
pub fn compute_compression_vector_slope(
    intersect_j: f32,
    focus_j: f32,
    limit_jmax: f32,
    slope_gain: f32,
) -> f32 {
    let direction_scaler = if intersect_j < focus_j {
        intersect_j
    } else {
        limit_jmax - intersect_j
    }; // TODO < vs <=
    sse_mul(direction_scaler, intersect_j - focus_j) / sse_mul(focus_j, slope_gain)
}

/// The approximate M where the line `J = slope * M + J_axis_intersect` meets the boundary
/// `J = J_max * (M / M_max)^(1/inv_gamma)`.
///
/// Port of `estimate_line_and_boundary_intersection_M` (Transform.cpp:969-987 @ v2.5.2).
#[inline]
pub fn estimate_line_and_boundary_intersection_m(
    j_axis_intersect: f32,
    slope: f32,
    inv_gamma: f32,
    j_max: f32,
    m_max: f32,
    j_intersection_reference: f32,
) -> f32 {
    // We calculate a shifted intersection from the original intersection using the inverse
    // of the exponential and the provided reference
    let normalised_j = j_axis_intersect / j_intersection_reference;
    let shifted_intersection = sse_mul(j_intersection_reference, normalised_j.powf(inv_gamma));

    // Now we find the M intersection of two lines
    // line from origin to J,M Max       l1(x) = J/M * x
    // line from J Intersect' with slope l2(x) = slope * x + Intersect'
    sse_mul(shifted_intersection, m_max) / (j_max - sse_mul(slope, m_max))
}

/// The gamut boundary's M along the compression line: a smooth minimum of the lower and the
/// (flipped) upper hulls' intersections.
///
/// Port of `find_gamut_boundary_intersection` (Transform.cpp:989-1004 @ v2.5.2).
pub fn find_gamut_boundary_intersection(
    jm_cusp: &F2,
    j_max: f32,
    gamma_top_inv: f32,
    gamma_bottom_inv: f32,
    j_intersect_source: f32,
    slope: f32,
    j_intersect_cusp: f32,
) -> f32 {
    let m_boundary_lower = estimate_line_and_boundary_intersection_m(
        j_intersect_source,
        slope,
        gamma_bottom_inv,
        jm_cusp[0],
        jm_cusp[1],
        j_intersect_cusp,
    );

    // The upper hull is flipped and thus 'zeroed' at J_max
    // Also note we negate the slope
    let f_j_intersect_cusp = j_max - j_intersect_cusp;
    let f_j_intersect_source = j_max - j_intersect_source;
    let f_jm_cusp_j = j_max - jm_cusp[0];
    let m_boundary_upper = estimate_line_and_boundary_intersection_m(
        f_j_intersect_source,
        -slope,
        gamma_top_inv,
        f_jm_cusp_j,
        jm_cusp[1],
        f_j_intersect_cusp,
    );

    // Smooth minimum between the two calculated values for the M component
    smin_scaled(m_boundary_lower, m_boundary_upper, jm_cusp[1])
}

/// The Reinhard curve, or its inverse.
///
/// Port of `reinhard_remap<invert>` (Transform.cpp:1006-1017 @ v2.5.2).
#[inline]
pub fn reinhard_remap<const INVERT: bool>(scale: f32, nd: f32) -> f32 {
    if INVERT {
        // TODO: given remap_M already tests against proportion do we need this asymptote test
        if nd >= 1.0 {
            return scale;
        }
        return sse_mul(scale, -(nd / (nd - 1.0)));
    }
    sse_mul(scale, nd) / sse_add(1.0, nd)
}

/// M compressed (or expanded, inverted) between the gamut and the reach boundaries, above a
/// threshold.
///
/// Port of `remap_M<invert>` (Transform.cpp:1019-1039 @ v2.5.2).
#[inline]
pub fn remap_m<const INVERT: bool>(m: f32, gamut_boundary_m: f32, reach_boundary_m: f32) -> f32 {
    let boundary_ratio = gamut_boundary_m / reach_boundary_m;
    let proportion = std_max(boundary_ratio, COMPRESSION_THRESHOLD);
    let threshold = sse_mul(proportion, gamut_boundary_m);

    if m <= threshold || proportion >= 1.0 {
        return m;
    }

    // Translate to place threshold at zero
    let m_offset = m - threshold;
    let gamut_offset = gamut_boundary_m - threshold;
    let reach_offset = reach_boundary_m - threshold;

    let scale = reach_offset / ((reach_offset / gamut_offset) - 1.0);
    let nd = m_offset / scale;

    // shift back to absolute
    sse_add(threshold, reinhard_remap::<INVERT>(scale, nd))
}

/// The gamut compression of a JMh along its compression line, with `Jx` the J that sets the
/// focus gain.
///
/// Port of `compressGamut<invert>` (Transform.cpp:1041-1070 @ v2.5.2).
pub fn compress_gamut<const INVERT: bool>(
    jmh: &F3,
    jx: f32,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
    hdp: &HueDependantGamutParams,
) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let h = jmh[2];

    let slope_gain = get_focus_gain(jx, hdp.analytical_threshold, sr.limit_j_max, p.focus_dist);
    let j_intersect_source = solve_j_intersect(j, m, hdp.focus_j, sr.limit_j_max, slope_gain);
    let gamut_slope = compute_compression_vector_slope(
        j_intersect_source,
        hdp.focus_j,
        sr.limit_j_max,
        slope_gain,
    );

    let j_intersect_cusp = solve_j_intersect(
        hdp.jm_cusp[0],
        hdp.jm_cusp[1],
        hdp.focus_j,
        sr.limit_j_max,
        slope_gain,
    );
    let gamut_boundary_m = find_gamut_boundary_intersection(
        &hdp.jm_cusp,
        sr.limit_j_max,
        hdp.gamma_top_inv,
        hdp.gamma_bottom_inv,
        j_intersect_source,
        gamut_slope,
        j_intersect_cusp,
    );

    if gamut_boundary_m <= 0.0 {
        // TODO: when/why does this happen?
        return [j, 0.0, h];
    }

    let reach_boundary_m = estimate_line_and_boundary_intersection_m(
        j_intersect_source,
        gamut_slope,
        sr.model_gamma_inv,
        sr.limit_j_max,
        sr.reach_max_m,
        sr.limit_j_max,
    );

    let remapped_m = remap_m::<INVERT>(m, gamut_boundary_m, reach_boundary_m);

    [
        sse_add(j_intersect_source, sse_mul(remapped_m, gamut_slope)),
        remapped_m,
        h,
    ]
}

/// The focus J: between the cusp's J and `mid_J`.
///
/// Port of `compute_focusJ` (Transform.cpp:1072-1075 @ v2.5.2).
#[inline]
pub fn compute_focus_j(cusp_j: f32, mid_j: f32, limit_j_max: f32) -> f32 {
    lerpf(
        cusp_j,
        mid_j,
        std_min(1.0, CUSP_MID_BLEND - (cusp_j / limit_j_max)),
    )
}

/// The gamut compression's parameters at `hue`, from the hue and cusp tables.
///
/// Port of `init_HueDependantGamutParams` (Transform.cpp:1077-1091 @ v2.5.2).
pub fn init_hue_dependant_gamut_params(
    hue: f32,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
) -> HueDependantGamutParams {
    let i_hi = lookup_hue_interval(hue, &p.hue_table, &p.hue_linearity_search_range);
    let t = interpolation_weight(
        hue,
        p.hue_table[i_hi as usize - 1],
        p.hue_table[i_hi as usize],
    );
    let cusp = cusp_from_table(i_hi, t, &p.gamut_cusp_table);

    let jm_cusp = [cusp[0], cusp[1]];
    HueDependantGamutParams {
        gamma_bottom_inv: p.lower_hull_gamma_inv,
        jm_cusp,
        gamma_top_inv: cusp[2],
        focus_j: compute_focus_j(jm_cusp[0], p.mid_j, sr.limit_j_max),
        analytical_threshold: lerpf(jm_cusp[0], sr.limit_j_max, FOCUS_GAIN_BLEND),
    }
}

/// The forward gamut compression of a JMh.
///
/// Port of `gamut_compress_fwd` (Transform.cpp:1093-1111 @ v2.5.2).
pub fn gamut_compress_fwd(
    jmh: &F3,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let h = jmh[2];

    if j <= 0.0 {
        // Limit to +ve J values // TODO test this is needed
        return [0.0, 0.0, h];
    }
    if m <= 0.0 || j > sr.limit_j_max {
        // We compress M only so avoid mapping zero
        // Above the expected maximum we explicitly map to 0 M
        return [j, 0.0, h];
    }
    let hdp = init_hue_dependant_gamut_params(h, sr, p);

    compress_gamut::<false>(jmh, jmh[0], sr, p, &hdp)
}

/// The inverse gamut compression of a JMh.
///
/// Port of `gamut_compress_inv` (Transform.cpp:1113-1137 @ v2.5.2).
pub fn gamut_compress_inv(
    jmh: &F3,
    sr: &ResolvedSharedCompressionParameters,
    p: &GamutCompressParams,
) -> F3 {
    let j = jmh[0];
    let m = jmh[1];
    let h = jmh[2];

    if j <= 0.0 {
        // Limit to +ve J values // TODO test this is needed
        return [0.0, 0.0, h];
    }
    if m <= 0.0 || j > sr.limit_j_max {
        // We compress M only so avoid mapping zero
        // Above the expected maximum we explicitly map to 0 M
        return [j, 0.0, h];
    }
    let hdp = init_hue_dependant_gamut_params(h, sr, p);

    let mut jx = j;
    if jx > hdp.analytical_threshold {
        // Approximation above threshold
        jx = compress_gamut::<true>(jmh, jx, sr, p, &hdp)[0];
    }
    compress_gamut::<true>(jmh, jx, sr, p, &hdp)
}

/// `gamma_test_count`.
const GAMMA_TEST_COUNT: usize = 5;

/// A test point of the upper hull's gamma fit.
///
/// Port of `testData` (Transform.cpp:1139-1145 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct TestData {
    /// `testJMh`.
    pub test_jmh: F3,
    /// `J_intersect_source`.
    pub j_intersect_source: f32,
    /// `slope`.
    pub slope: f32,
    /// `J_intersect_cusp`.
    pub j_intersect_cusp: f32,
}

/// The gamma fit's test points between the cusp and `limit_J_max`.
///
/// Port of `generate_gamma_test_data` (Transform.cpp:1147-1169 @ v2.5.2).
pub fn generate_gamma_test_data(
    jm_cusp: &F2,
    hue: f32,
    limit_j_max: f32,
    mid_j: f32,
    focus_dist: f32,
) -> [TestData; GAMMA_TEST_COUNT] {
    const TEST_POSITIONS: [f32; GAMMA_TEST_COUNT] = [0.01, 0.1, 0.5, 0.8, 0.99];
    let analytical_threshold = lerpf(jm_cusp[0], limit_j_max, FOCUS_GAIN_BLEND);
    let focus_j = compute_focus_j(jm_cusp[0], mid_j, limit_j_max);

    TEST_POSITIONS.map(|position| {
        let test_j = lerpf(jm_cusp[0], limit_j_max, position);
        let slope_gain = get_focus_gain(test_j, analytical_threshold, limit_j_max, focus_dist);
        let j_intersect_source =
            solve_j_intersect(test_j, jm_cusp[1], focus_j, limit_j_max, slope_gain);
        TestData {
            test_jmh: [test_j, jm_cusp[1], hue],
            j_intersect_source,
            slope: compute_compression_vector_slope(
                j_intersect_source,
                focus_j,
                limit_j_max,
                slope_gain,
            ),
            j_intersect_cusp: solve_j_intersect(
                jm_cusp[0],
                jm_cusp[1],
                focus_j,
                limit_j_max,
                slope_gain,
            ),
        }
    })
}

/// Whether every test point's boundary estimate with the upper hull's `topGamma_inv` falls
/// outside the limiting gamut's top.
///
/// Port of `evaluate_gamma_fit` (Transform.cpp:1171-1197 @ v2.5.2).
pub fn evaluate_gamma_fit(
    jm_cusp: &F2,
    data: &[TestData; GAMMA_TEST_COUNT],
    top_gamma_inv: f32,
    peak_luminance: f32,
    limit_j_max: f32,
    lower_hull_gamma_inv: f32,
    limit_jmh_params: &JMhParams,
) -> bool {
    let luminance_limit = peak_luminance / REFERENCE_LUMINANCE;
    for test_data in data {
        let approx_limit_m = find_gamut_boundary_intersection(
            jm_cusp,
            limit_j_max,
            top_gamma_inv,
            lower_hull_gamma_inv,
            test_data.j_intersect_source,
            test_data.slope,
            test_data.j_intersect_cusp,
        );
        let approx_limit_j = sse_add(
            test_data.j_intersect_source,
            sse_mul(test_data.slope, approx_limit_m),
        );

        let approximate_jmh = [approx_limit_j, approx_limit_m, test_data.test_jmh[2]];
        let new_limit_rgb = jmh_to_rgb(&approximate_jmh, limit_jmh_params);

        if !outside_hull(&new_limit_rgb, luminance_limit) {
            return false;
        }
    }

    true
}

/// The upper hull's inverse gamma at each hue of the cusp table, into its third column: the
/// smallest gamma (by stepping then bisecting) whose fit puts every test point outside the
/// limiting gamut.
///
/// Port of `make_upper_hull_gamma` (Transform.cpp:1199-1263 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
pub fn make_upper_hull_gamma(
    hue_table: &Table1D,
    gamut_cusp_table: &mut Table3D,
    peak_luminance: f32,
    limit_j_max: f32,
    mid_j: f32,
    focus_dist: f32,
    lower_hull_gamma_inv: f32,
    limit_jmh_params: &JMhParams,
) {
    for i in table_base::FIRST_NOMINAL_INDEX..table_base::UPPER_WRAP_INDEX {
        let hue = hue_table[i];
        let jm_cusp = [gamut_cusp_table[i][0], gamut_cusp_table[i][1]];

        let data = generate_gamma_test_data(&jm_cusp, hue, limit_j_max, mid_j, focus_dist);

        let search_range = GAMMA_SEARCH_STEP;
        let mut low = GAMMA_MINIMUM;
        let mut high = low + search_range;
        let mut outside = false;

        let gamma_fit_predicate = |gamma: f32| {
            evaluate_gamma_fit(
                &jm_cusp,
                &data,
                1.0 / gamma,
                peak_luminance,
                limit_j_max,
                lower_hull_gamma_inv,
                limit_jmh_params,
            )
        };
        while !outside && high < GAMMA_MAXIMUM {
            let gamma_found = gamma_fit_predicate(high);
            if !gamma_found {
                low = high;
                high = sse_add(high, search_range);
            } else {
                outside = true;
            }
        }

        while (high - low) > GAMMA_ACCURACY {
            let test_gamma = midpoint(high, low);
            let gamma_found = gamma_fit_predicate(test_gamma);
            if gamma_found {
                high = test_gamma;
            } else {
                low = test_gamma;
            }
        }
        gamut_cusp_table[i][2] = 1.0 / high;
    }

    // Copy last populated entries to empty spot 'wrapping' entries
    gamut_cusp_table[table_base::LOWER_WRAP_INDEX][2] =
        gamut_cusp_table[table_base::LAST_NOMINAL_INDEX][2];
    gamut_cusp_table[table_base::UPPER_WRAP_INDEX][2] =
        gamut_cusp_table[table_base::FIRST_NOMINAL_INDEX][2];
    gamut_cusp_table[table_base::UPPER_WRAP_INDEX + 1][2] =
        gamut_cusp_table[table_base::FIRST_NOMINAL_INDEX + 1][2];
}

/// The tone scale's parameters for `peakLuminance` nits.
///
/// Port of `init_ToneScaleParams` (Transform.cpp:1265-1315 @ v2.5.2).
pub fn init_tone_scale_params(peak_luminance: f32) -> ToneScaleParams {
    let mul = sse_mul;
    // Preset constants that set the desired behavior for the curve
    let n = peak_luminance;

    let n_r = 100.0f32; // normalized white in nits (what 1.0 should be)
    let g = 1.15f32; // surround / contrast
    let c = 0.18f32; // anchor for 18% grey
    let c_d = 10.013f32; // output luminance of 18% grey (in nits)
    let w_g = 0.14f32; // change in grey between different peak luminance
    let t_1 = 0.04f32; // shadow toe or flare/glare compensation
    let r_hit_min = 128.0f32; // scene-referred value "hitting the roof"
    let r_hit_max = 896.0f32; // scene-referred value "hitting the roof"

    // Calculate output constants
    let r_hit = sse_add(
        r_hit_min,
        mul(
            r_hit_max - r_hit_min,
            (n / n_r).ln() / (10000.0f32 / 100.0).ln(),
        ),
    );
    let m_0 = n / n_r;
    let m_1 = mul(
        0.5,
        sse_add(m_0, mul(m_0, sse_add(m_0, mul(4.0, t_1))).sqrt()),
    );
    let u = ((r_hit / m_1) / sse_add(r_hit / m_1, 1.0)).powf(g);
    let m = m_1 / u;
    let w_i = (n / 100.0).ln() / 2.0f32.ln();
    let c_t = mul(c_d / n_r, sse_add(1.0, mul(w_i, w_g)));
    let g_ip = mul(
        0.5,
        sse_add(c_t, mul(c_t, sse_add(c_t, mul(4.0, t_1))).sqrt()),
    );
    let g_ipp2 = -mul(m_1, (g_ip / m).powf(1.0 / g)) / ((g_ip / m).powf(1.0 / g) - 1.0);
    let w_2 = c / g_ipp2;
    let s_2 = mul(mul(w_2, m_1), REFERENCE_LUMINANCE);
    let u_2 = ((r_hit / m_1) / sse_add(r_hit / m_1, w_2)).powf(g);
    let m_2 = m_1 / u_2;
    let inverse_limit = n / mul(u_2, n_r);
    let forward_limit = mul(8.0, r_hit);
    let log_peak = (n / n_r).log10();

    ToneScaleParams {
        n,
        n_r,
        g,
        t_1,
        c_t,
        s_2,
        u_2,
        m_2,
        forward_limit,
        inverse_limit,
        log_peak,
    }
}

/// The parameters chroma and gamut compression share: `limit_J_max` (the J of the peak
/// luminance), the model's inverse gamma and the reach table.
///
/// Port of `init_SharedCompressionParams` (Transform.cpp:1317-1328 @ v2.5.2).
pub fn init_shared_compression_params(
    peak_luminance: f32,
    input_jmh_params: &JMhParams,
    reach_params: &JMhParams,
) -> SharedCompressionParameters {
    let limit_j_max = y_to_j(peak_luminance, input_jmh_params);
    let model_gamma_inv = 1.0 / model_gamma();

    SharedCompressionParameters {
        limit_j_max,
        model_gamma_inv,
        reach_m_table: make_reach_m_table(reach_params, limit_j_max),
    }
}

/// The shared parameters at `hue`, the reach's M from its table.
///
/// Port of `resolve_CompressionParams` (Transform.cpp:1330-1338 @ v2.5.2).
pub fn resolve_compression_params(
    hue: f32,
    p: &SharedCompressionParameters,
) -> ResolvedSharedCompressionParameters {
    ResolvedSharedCompressionParameters {
        limit_j_max: p.limit_j_max,
        model_gamma_inv: p.model_gamma_inv,
        reach_max_m: reach_m_from_table(hue, &p.reach_m_table),
    }
}

/// The chroma compression's parameters for `peakLuminance` nits.
///
/// Port of `init_ChromaCompressParams` (Transform.cpp:1340-1356 @ v2.5.2).
pub fn init_chroma_compress_params(
    peak_luminance: f32,
    ts_params: &ToneScaleParams,
) -> ChromaCompressParams {
    let mul = sse_mul;
    // Calculated chroma compress variables
    let compr = sse_add(
        CHROMA_COMPRESS,
        mul(CHROMA_COMPRESS * CHROMA_COMPRESS_FACT, ts_params.log_peak),
    );
    let sat = std_max(
        0.2,
        CHROMA_EXPAND - mul(CHROMA_EXPAND * CHROMA_EXPAND_FACT, ts_params.log_peak),
    );
    let sat_thr = CHROMA_EXPAND_THR / ts_params.n;
    let chroma_compress_scale = mul(0.03379, peak_luminance).powf(0.30596) - 0.45135;

    ChromaCompressParams {
        sat,
        sat_thr,
        compr,
        chroma_compress_scale,
    }
}

/// The range around a hue's uniform-table position that holds its interval in the cusp
/// table: the largest deviations from a linear distribution, padded.
///
/// Port of `determine_hue_linearity_search_range` (Transform.cpp:1358-1382 @ v2.5.2).
pub fn determine_hue_linearity_search_range(gamut_cusp_table: &Table3D) -> [i32; 2] {
    // TODO: Padding values are a quick hack to ensure the range encloses the needed range
    const LOWER_PADDING: i32 = 0;
    const UPPER_PADDING: i32 = 1;
    let mut hue_linearity_search_range = [LOWER_PADDING, UPPER_PADDING];
    for (i, entry) in gamut_cusp_table
        .iter()
        .enumerate()
        .take(table_base::UPPER_WRAP_INDEX)
        .skip(table_base::FIRST_NOMINAL_INDEX)
    {
        let pos = table_base::nominal_hue_position_in_uniform_table(entry[2]);
        let delta = (i as i32).wrapping_sub(pos as i32);
        hue_linearity_search_range[0] = std::cmp::min(
            hue_linearity_search_range[0],
            delta.wrapping_add(LOWER_PADDING),
        );
        hue_linearity_search_range[1] = std::cmp::max(
            hue_linearity_search_range[1],
            delta.wrapping_add(UPPER_PADDING),
        );
    }
    hue_linearity_search_range
}

/// The gamut compression's parameters: `mid_J`, the focus distance, the lower hull's inverse
/// gamma, and the hue, cusp and upper hull gamma tables.
///
/// Port of `init_GamutCompressParams` (Transform.cpp:1384-1402 @ v2.5.2).
pub fn init_gamut_compress_params(
    peak_luminance: f32,
    input_jmh_params: &JMhParams,
    limit_jmh_params: &JMhParams,
    ts_params: &ToneScaleParams,
    sh_params: &SharedCompressionParameters,
    reach_params: &JMhParams,
) -> Result<GamutCompressParams> {
    let mul = sse_mul;
    let mid_j = y_to_j(mul(ts_params.c_t, REFERENCE_LUMINANCE), input_jmh_params);

    // Calculated chroma compress variables
    let focus_dist = sse_add(
        FOCUS_DISTANCE,
        mul(FOCUS_DISTANCE * FOCUS_DISTANCE_SCALING, ts_params.log_peak),
    );
    // TODO: name these magic constants
    let lower_hull_gamma_inv = 1.0 / sse_add(1.14, mul(0.07, ts_params.log_peak));

    let mut hue_table: Table1D = [0.0; table_base::TOTAL_SIZE];
    let mut gamut_cusp_table = make_uniform_hue_gamut_table(
        reach_params,
        limit_jmh_params,
        peak_luminance,
        ts_params.forward_limit,
        sh_params,
        &mut hue_table,
    )?;
    let hue_linearity_search_range = determine_hue_linearity_search_range(&gamut_cusp_table);
    make_upper_hull_gamma(
        &hue_table,
        &mut gamut_cusp_table,
        peak_luminance,
        sh_params.limit_j_max,
        mid_j,
        focus_dist,
        lower_hull_gamma_inv,
        limit_jmh_params,
    );
    Ok(GamutCompressParams {
        mid_j,
        focus_dist,
        lower_hull_gamma_inv,
        hue_linearity_search_range,
        hue_table,
        gamut_cusp_table,
    })
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
