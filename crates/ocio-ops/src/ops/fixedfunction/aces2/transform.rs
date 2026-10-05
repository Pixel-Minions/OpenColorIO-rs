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
    CHROMA_EXPAND_FACT, CHROMA_EXPAND_THR, ChromaCompressParams, J_SCALE, JMhParams, L_A,
    REFERENCE_LUMINANCE, ResolvedSharedCompressionParameters, SURROUND,
    SharedCompressionParameters, Table1D, Table3D, ToneScaleParams, Y_B, cam16, from_radians_,
    table_base, to_radians,
};
use super::matrix_lib::{F3, M33f, f3_from_f, invert_f33, mult_f3_f33, mult_f33_f33, scale_f33};
use crate::exception::Result;
use crate::math_utils::{lerpf, sse_add, sse_mul, std_max, std_min};
use crate::transforms::builtins::color_matrix_helpers::Primaries;

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
