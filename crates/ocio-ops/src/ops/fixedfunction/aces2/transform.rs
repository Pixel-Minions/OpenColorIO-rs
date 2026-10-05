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
    CHROMA_EXPAND_FACT, CHROMA_EXPAND_THR, CUSP_CORNER_COUNT, ChromaCompressParams,
    DISPLAY_CUSP_TOLERANCE, HUE_LIMIT, J_SCALE, JMhParams, L_A, MAX_SORTED_CORNERS,
    REACH_CUSP_TOLERANCE, REFERENCE_LUMINANCE, ResolvedSharedCompressionParameters, SMOOTH_CUSPS,
    SMOOTH_M, SURROUND, SharedCompressionParameters, TOTAL_CORNER_COUNT, Table1D, Table3D,
    ToneScaleParams, Y_B, cam16, f32_to_u32, from_radians_, table_base, to_radians,
};
use super::matrix_lib::{
    F3, M33f, f3_from_f, invert_f33, mult_f_f3, mult_f3_f33, mult_f33_f33, scale_f33,
};
use crate::exception::{Exception, Result};
use crate::math_utils::{lerpf, sse_add, sse_mul, std_max, std_min};
use crate::transforms::builtins::color_matrix_helpers::Primaries;

/// `cuspCornerCount`, as an index.
const CUSP_CORNERS: usize = CUSP_CORNER_COUNT as usize;
/// `totalCornerCount`, as an index.
const TOTAL_CORNERS: usize = TOTAL_CORNER_COUNT as usize;
/// `max_sorted_corners`, as an index.
const MAX_SORTED: usize = MAX_SORTED_CORNERS as usize;

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
/// Port of `RGB_to_Aab` (Transform.cpp:175-187 @ v2.5.2).
pub fn rgb_to_aab(rgb: &F3, p: &JMhParams) -> F3 {
    let rgb_m = mult_f3_f33(rgb, &p.matrix_rgb_to_cam16_c);

    let rgb_a = [
        post_adaptation_cone_response_compression_fwd(rgb_m[0]),
        post_adaptation_cone_response_compression_fwd(rgb_m[1]),
        post_adaptation_cone_response_compression_fwd(rgb_m[2]),
    ];

    mult_f3_f33(&rgb_a, &p.matrix_cone_response_to_aab)
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
/// Port of `JMh_to_Aab(const f3 &, const float &, const float &, const JMhParams &)`
/// (Transform.cpp:210-219 @ v2.5.2).
pub fn jmh_to_aab_with(jmh: &F3, cos_hr: f32, sin_hr: f32, p: &JMhParams) -> F3 {
    let j = jmh[0];
    let m = jmh[1];

    let a_ = j_to_achromatic_n(j, p.inv_cz);
    let a = sse_mul(m, cos_hr);
    let b = sse_mul(m, sin_hr);
    [a_, a, b]
}

/// Port of `JMh_to_Aab(const f3 &, const JMhParams &)` (Transform.cpp:221-229 @ v2.5.2).
pub fn jmh_to_aab(jmh: &F3, p: &JMhParams) -> F3 {
    let h = jmh[2];
    let h_rad = to_radians(h);
    let cos_hr = h_rad.cos();
    let sin_hr = h_rad.sin();

    jmh_to_aab_with(jmh, cos_hr, sin_hr, p)
}

/// Port of `Aab_to_RGB` (Transform.cpp:231-243 @ v2.5.2).
pub fn aab_to_rgb(aab: &F3, p: &JMhParams) -> F3 {
    let rgb_a = mult_f3_f33(aab, &p.matrix_aab_to_cone_response);

    let rgb_m = [
        post_adaptation_cone_response_compression_inv(rgb_a[0]),
        post_adaptation_cone_response_compression_inv(rgb_a[1]),
        post_adaptation_cone_response_compression_inv(rgb_a[2]),
    ];

    mult_f3_f33(&rgb_m, &p.matrix_cam16_c_to_rgb)
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

/// Port of `toe_fwd` (Transform.cpp:335-349 @ v2.5.2).
#[inline]
pub fn toe_fwd(x: f32, limit: f32, k1_in: f32, k2_in: f32) -> f32 {
    if x > limit {
        return x;
    }

    let m = sse_mul;
    let k2 = std_max(k2_in, 0.001);
    let k1 = sse_add(m(k1_in, k1_in), m(k2, k2)).sqrt();
    let k3 = sse_add(limit, k1) / sse_add(limit, k2);

    let minus_b = m(k3, x) - k1;
    let minus_ac = m(m(k2, k3), x); // a is 1.0
    // a is 1.0, mins_b squared == b^2
    m(
        0.5,
        sse_add(
            minus_b,
            sse_add(m(minus_b, minus_b), m(4.0, minus_ac)).sqrt(),
        ),
    )
}

/// Port of `toe_inv` (Transform.cpp:351-362 @ v2.5.2).
#[inline]
pub fn toe_inv(x: f32, limit: f32, k1_in: f32, k2_in: f32) -> f32 {
    if x > limit {
        return x;
    }

    let m = sse_mul;
    let k2 = std_max(k2_in, 0.001);
    let k1 = sse_add(m(k1_in, k1_in), m(k2, k2)).sqrt();
    let k3 = sse_add(limit, k1) / sse_add(limit, k2);
    sse_add(m(x, x), m(k1, x)) / m(k3, sse_add(x, k2))
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

    if mm != 0.0 {
        let nj = j_ts / pr.limit_j_max;
        let snj = std_max(0.0, 1.0 - nj);
        let limit = m(nj.powf(pr.model_gamma_inv), pr.reach_max_m) / mnorm;

        m_cp = m(mm, (j_ts / j).powf(pr.model_gamma_inv));
        m_cp /= mnorm;
        m_cp = limit
            - toe_fwd(
                limit - m_cp,
                limit - 0.001,
                m(snj, pc.sat),
                sse_add(m(nj, nj), pc.sat_thr).sqrt(),
            );
        m_cp = toe_fwd(m_cp, limit, m(nj, pc.compr), snj);
        m_cp = m(m_cp, mnorm);
    }

    [j_ts, m_cp, h]
}

/// The inverse of [`chroma_compress_fwd`], given the original J.
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
        let limit = m(nj.powf(pr.model_gamma_inv), pr.reach_max_m) / mnorm;

        mm = m_cp / mnorm;
        mm = toe_inv(mm, limit, m(nj, pc.compr), snj);
        mm = limit
            - toe_inv(
                limit - mm,
                limit - 0.001,
                m(snj, pc.sat),
                sse_add(m(nj, nj), pc.sat_thr).sqrt(),
            );
        mm = m(mm, mnorm);
        mm = m(mm, (j_ts / j).powf(-pr.model_gamma_inv));
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

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
