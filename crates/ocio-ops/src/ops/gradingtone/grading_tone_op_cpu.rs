// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingTone op's CPU renderers: a port of
//! `src/OpenColorIO/ops/gradingtone/GradingToneOpCPU.h` and `GradingToneOpCPU.cpp` @ v2.5.2.
//!
//! [`get_grading_tone_cpu_renderer`] picks one renderer per direction (and the linear style's
//! own pair, WP 5.3c). Each holds the op data's dynamic property (for a dynamic one, a copy the
//! CPU processor exposes) and reads its values and their pre-render form once per `apply`
//! (U-81: a set from another thread gives the whole old or the whole new value).
//!
//! The log and video styles have no `OCIO_USE_SSE2` branch: each tone (midtones, highlights,
//! whites, shadows, blacks, s-contrast) is scalar `float` code, run once per color channel
//! (`float`) and once for the master component on the three color channels together (a
//! `float3` of three `float` lanes, each computed on its own). The renderers copy each pixel
//! and grade its color channels in place; alpha is never written.
//!
//! # Operand orders
//!
//! Where two NaNs can meet, the result is the first operand's NaN. They can: a pixel's NaN
//! meets a NaN coefficient (the pre-render form of extreme finite values overflows to
//! infinities, whose differences are NaN, and `GradingTonePreRender::update` negates some, so
//! a coefficient's NaN can have either sign), or the default NaN an infinity makes (`inf -
//! inf`, `0 * inf`, the square root of a negative number) meets the other sign's.
//!
//! The compilers ordered many additions and multiplications differently from the source, from
//! each other, between the `float` and the `float3` instantiation of a template, and between
//! the three lanes of one `float3` (GCC computes them one after the other, with a different
//! register allocation each). The tables below give, for each operation where two values that
//! can both be NaN meet, which operand each wheel's machine code puts first, read function by
//! function (`tools/wheel-inspect`; Windows `sub_1801f38f0` and its neighbours, which the
//! renderers' `apply`s call, Linux `ComputeHSFwd<float>` and the other symbols). Each table is
//! relative to a canonical form, the C++ text read as `float` code, named in the kernels'
//! comments: `false` is the canonical operand first, `true` the other one ("swapped").
//!
//! MSVC also computes a few roots as `x0 - ((c + c) / (d + b)) * w` where the source has
//! `((-2.f * c) / (d + b)) * w + x0` ([`HsRevOrders::subform`], [`ScRevOrders::subform`]): the
//! same values, but a NaN `x0` comes first.

use std::sync::Arc;

use super::grading_tone::{GradingTone, GradingTonePreRender, RgbmChannel, get_channel_value};
use super::grading_tone_op_data::GradingToneOpData;
use crate::dynamic_property::{DynamicPropertyGradingToneImplRcPtr, DynamicPropertyRcPtr};
use crate::exception::{Exception, Result};
use crate::math_utils::{clamp, sse_add, sse_mul, sse_sub, std_max, std_min};
use crate::op::CpuOp;
use crate::open_color_types::{DynamicPropertyType, GradingStyle, TransformDirection};

/// "Dynamic property type not supported by GradingTone." (GradingToneOpCPU.cpp:76 @ v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by GradingTone.";

/// "GradingTone property is not dynamic." (GradingToneOpCPU.cpp:73 @ v2.5.2).
const NOT_DYNAMIC: &str = "GradingTone property is not dynamic.";

/// A swapped operation: the canonical form's second operand first.
const S: bool = true;

/// `a + b` with `b` first when `swap`.
#[inline]
fn add(swap: bool, a: f32, b: f32) -> f32 {
    if swap { sse_add(b, a) } else { sse_add(a, b) }
}

/// `a * b` with `b` first when `swap`.
#[inline]
fn mul(swap: bool, a: f32, b: f32) -> f32 {
    if swap { sse_mul(b, a) } else { sse_mul(a, b) }
}

/// `a - b`.
#[inline]
fn sub(a: f32, b: f32) -> f32 {
    sse_sub(a, b)
}

/// The square root of a `float`: `sqrtss`, or for a negative argument the C library's `sqrtf`
/// (for `errno`), which gives the same default NaN in both wheels (`Sqrt`, GradingToneOpCPU.cpp:
/// 245-253 @ v2.5.2, and the scalar mids' unqualified `sqrt`).
#[inline]
fn sqrt(x: f32) -> f32 {
    x.sqrt()
}

// ---------------------------------------------------------------------------------------------
// Highlights and shadows.
// ---------------------------------------------------------------------------------------------

/// The control points and slopes of a highlights or shadows curve
/// (`GradingTonePreRender::m_hsX`, `m_hsY`, `m_hsM`).
#[derive(Debug, Clone, Copy)]
struct HsCoefs {
    x0: f32,
    x1: f32,
    x2: f32,
    y0: f32,
    y1: f32,
    y2: f32,
    m0: f32,
    m2: f32,
}

/// The operand orders of `ComputeHSFwd`, whose canonical form is:
///
/// ```text
/// tL = (t - x0) / (x1 - x0);  tR = (t - x1) / (x2 - x1)
/// A = y0 * (1 - tL*tL) [h1];  B = (y1 * tL [h2a]) * tL [h2b]
/// C = ((m0 * (1 - tL) [h3a]) * tL [h3b]) * (x1 - x0) [h3c];  fL = (A + B [h4]) + C [h5]
/// D = (y1 * (1 - tR) [h6a]) * (1 - tR) [h6b];  E = (y2 * (2 - tR) [h7a]) * tR [h7b]
/// F = ((m2 * (tR - 1) [h8a]) * tR [h8b]) * (x2 - x1) [h8c];  fR = (D + E [h9]) + F [h10]
/// r0 = (t - x0) * m0 [h11] + y0 [h12];  r2 = (t - x2) * m2 [h13] + y2 [h14]
/// ```
#[derive(Debug, Clone, Copy, Default)]
struct HsFwdOrders {
    h1: bool,
    h2a: bool,
    h2b: bool,
    h3a: bool,
    h3b: bool,
    h3c: bool,
    h4: bool,
    h5: bool,
    h6a: bool,
    h6b: bool,
    h7a: bool,
    h7b: bool,
    h8a: bool,
    h8b: bool,
    h8c: bool,
    h9: bool,
    h10: bool,
    h11: bool,
    h12: bool,
    h13: bool,
    h14: bool,
}

/// The canonical order everywhere.
const HS_FWD_SRC: HsFwdOrders = HsFwdOrders {
    h1: false,
    h2a: false,
    h2b: false,
    h3a: false,
    h3b: false,
    h3c: false,
    h4: false,
    h5: false,
    h6a: false,
    h6b: false,
    h7a: false,
    h7b: false,
    h8a: false,
    h8b: false,
    h8c: false,
    h9: false,
    h10: false,
    h11: false,
    h12: false,
    h13: false,
    h14: false,
};

/// The operand orders of `ComputeHSRev`, whose canonical form is:
///
/// ```text
/// bL = m0 * (x1 - x0) [r1];  aL = (y1 - y0) - bL;  cL = y0 - t
/// dL = sqrt(bL*bL - (4*aL) * cL [r3b]);  outL = ((-2*cL) / (dL + bL [r5])) * (x1 - x0) [r6] + x0 [r7]
/// P = m2 * (x2 - x1) [r10];  bR = (2*y2 - 2*y1) - P;  aR = (y1 - y2) + P [r12];  cR = y1 - t
/// dR = sqrt(bR*bR - (4*aR) * cR [r13b]);  outR = ((-2*cR) / (dR + bR [r15])) * (x2 - x1) [r16] + x1 [r17]
/// r0 = (t - y0) / m0 + x0 [r18];  r2 = (t - y2) / m2 + x2 [r19]
/// ```
#[derive(Debug, Clone, Copy, Default)]
struct HsRevOrders {
    r1: bool,
    r3b: bool,
    r5: bool,
    r6: bool,
    r7: bool,
    r10: bool,
    r12: bool,
    r13b: bool,
    r15: bool,
    r16: bool,
    r17: bool,
    r18: bool,
    r19: bool,
    /// The roots computed as `x0 - ((c + c) / (d + b)) * w` and `x1 - ...` (MSVC), which put a
    /// NaN `x0` (`x1`) first; `r7` and `r17` are then unused.
    subform: bool,
}

/// The canonical order everywhere.
const HS_REV_SRC: HsRevOrders = HsRevOrders {
    r1: false,
    r3b: false,
    r5: false,
    r6: false,
    r7: false,
    r10: false,
    r12: false,
    r13b: false,
    r15: false,
    r16: false,
    r17: false,
    r18: false,
    r19: false,
    subform: false,
};

/// The orders of one template's `float` instantiation and of each lane of its `float3` one.
#[derive(Debug, Clone, Copy)]
struct Tables<T> {
    /// `ComputeXxx<float>`, for the red, green and blue components.
    scalar: T,
    /// `ComputeXxx<float3>`'s lanes, for the master component.
    f3: [T; 3],
}

/// Windows: `sub_1801f38f0` (`float`) and `sub_1801f3a70` (`float3`), the same orders in
/// every lane.
#[cfg(windows)]
const HS_FWD: Tables<HsFwdOrders> = {
    const W: HsFwdOrders = HsFwdOrders {
        h1: S,
        h2a: S,
        h3a: S,
        h6a: S,
        h7a: S,
        h8a: S,
        h9: S,
        ..HS_FWD_SRC
    };
    Tables {
        scalar: W,
        f3: [W; 3],
    }
};

/// Linux: `ComputeHSFwd<float>` (0x3d4d20) and `ComputeHSFwd<float3>` (0x3d46c0).
#[cfg(not(windows))]
const HS_FWD: Tables<HsFwdOrders> = Tables {
    scalar: HsFwdOrders {
        h1: S,
        h3a: S,
        h7a: S,
        h8a: S,
        h9: S,
        ..HS_FWD_SRC
    },
    f3: [
        HsFwdOrders {
            h1: S,
            h2a: S,
            h3a: S,
            h4: S,
            h7a: S,
            h8a: S,
            ..HS_FWD_SRC
        },
        HsFwdOrders {
            h1: S,
            h2a: S,
            h3a: S,
            h4: S,
            h5: S,
            h7a: S,
            h8a: S,
            h10: S,
            ..HS_FWD_SRC
        },
        HsFwdOrders {
            h1: S,
            h3a: S,
            h4: S,
            h5: S,
            h7a: S,
            h10: S,
            h14: S,
            ..HS_FWD_SRC
        },
    ],
};

/// Windows: `sub_1801f3e90` (`float`) and `sub_1801f40d0` (`float3`).
#[cfg(windows)]
const HS_REV: Tables<HsRevOrders> = {
    const W: HsRevOrders = HsRevOrders {
        r1: S,
        r10: S,
        subform: true,
        ..HS_REV_SRC
    };
    Tables {
        scalar: W,
        f3: [W; 3],
    }
};

/// Linux: `ComputeHSRev<float>` (0x3d4a30) and `ComputeHSRev<float3>` (0x3d5a00).
#[cfg(not(windows))]
const HS_REV: Tables<HsRevOrders> = {
    const L01: HsRevOrders = HsRevOrders {
        r1: S,
        r3b: S,
        r13b: S,
        r19: S,
        ..HS_REV_SRC
    };
    Tables {
        scalar: HsRevOrders {
            r1: S,
            r5: S,
            r12: S,
            r15: S,
            ..HS_REV_SRC
        },
        f3: [
            L01,
            L01,
            HsRevOrders {
                r1: S,
                r5: S,
                r15: S,
                r19: S,
                ..HS_REV_SRC
            },
        ],
    }
};

/// One value through the highlights or shadows curve: a quadratic on each side of `x1`, lines
/// beyond `x0` and `x2`.
///
/// Port of `ComputeHSFwd` (GradingToneOpCPU.cpp:499-517 @ v2.5.2), for one `float` or one lane
/// of a `float3`.
fn compute_hs_fwd(t: f32, c: &HsCoefs, o: &HsFwdOrders) -> f32 {
    let HsCoefs {
        x0,
        x1,
        x2,
        y0,
        y1,
        y2,
        m0,
        m2,
    } = *c;
    let t_l = sub(t, x0) / sub(x1, x0);
    let t_r = sub(t, x1) / sub(x2, x1);
    let a = mul(o.h1, y0, sub(1.0, sse_mul(t_l, t_l)));
    let b = mul(o.h2b, mul(o.h2a, y1, t_l), t_l);
    let cc = mul(
        o.h3c,
        mul(o.h3b, mul(o.h3a, m0, sub(1.0, t_l)), t_l),
        sub(x1, x0),
    );
    let f_l = add(o.h5, add(o.h4, a, b), cc);
    let one_r = sub(1.0, t_r);
    let d = mul(o.h6b, mul(o.h6a, y1, one_r), one_r);
    let e = mul(o.h7b, mul(o.h7a, y2, sub(2.0, t_r)), t_r);
    let f = mul(
        o.h8c,
        mul(o.h8b, mul(o.h8a, m2, sub(t_r, 1.0)), t_r),
        sub(x2, x1),
    );
    let f_r = add(o.h10, add(o.h9, d, e), f);

    let mut res = if t < x1 { f_l } else { f_r };
    let r0 = add(o.h12, mul(o.h11, sub(t, x0), m0), y0);
    res = if t < x0 { r0 } else { res };
    let r2 = add(o.h14, mul(o.h13, sub(t, x2), m2), y2);
    if t < x2 { res } else { r2 }
}

/// The root of a quadratic segment: `((-2*c) / (d + b)) * w + x`, or MSVC's
/// `x - ((c + c) / (d + b)) * w`.
#[allow(clippy::too_many_arguments)]
fn root(subform: bool, add_first: bool, mul_first: bool, c: f32, den: f32, w: f32, x: f32) -> f32 {
    if subform {
        sub(x, mul(mul_first, sse_add(c, c) / den, w))
    } else {
        add(add_first, mul(mul_first, sse_mul(c, -2.0) / den, w), x)
    }
}

/// One value through the inverse of the highlights or shadows curve.
///
/// Port of `ComputeHSRev` (GradingToneOpCPU.cpp:519-543 @ v2.5.2), for one `float` or one lane
/// of a `float3`.
fn compute_hs_rev(t: f32, c: &HsCoefs, o: &HsRevOrders) -> f32 {
    let HsCoefs {
        x0,
        x1,
        x2,
        y0,
        y1,
        y2,
        m0,
        m2,
    } = *c;
    let w_l = sub(x1, x0);
    let b_l = mul(o.r1, m0, w_l);
    let a_l = sub(sub(y1, y0), b_l);
    let c_l = sub(y0, t);
    let d_l = sqrt(sub(sse_mul(b_l, b_l), mul(o.r3b, sse_mul(4.0, a_l), c_l)));
    let out_l = root(o.subform, o.r7, o.r6, c_l, add(o.r5, d_l, b_l), w_l, x0);

    let w_r = sub(x2, x1);
    let p = mul(o.r10, m2, w_r);
    let b_r = sub(sub(sse_mul(2.0, y2), sse_mul(2.0, y1)), p);
    let a_r = add(o.r12, sub(y1, y2), p);
    let c_r = sub(y1, t);
    let d_r = sqrt(sub(sse_mul(b_r, b_r), mul(o.r13b, sse_mul(4.0, a_r), c_r)));
    let out_r = root(o.subform, o.r17, o.r16, c_r, add(o.r15, d_r, b_r), w_r, x1);

    let mut res = if t < y1 { out_l } else { out_r };
    let r0 = add(o.r18, sub(t, y0) / m0, x0);
    res = if t < y0 { r0 } else { res };
    let r2 = add(o.r19, sub(t, y2) / m2, x2);
    if t < y2 { res } else { r2 }
}

// ---------------------------------------------------------------------------------------------
// Whites and blacks.
// ---------------------------------------------------------------------------------------------

/// The control points, slopes and gain of a whites or blacks curve
/// (`GradingTonePreRender::m_wbX`, `m_wbY`, `m_wbM`, `m_wbGain`).
#[derive(Debug, Clone, Copy)]
struct WbCoefs {
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    m0: f32,
    m1: f32,
    gain: f32,
}

/// The operand orders of `ComputeWBFwd`, whose canonical form is:
///
/// ```text
/// decreasing: tl = (t - x0) / (x1 - x0)
///   res = ((tl * (x1-x0) [w1]) * (((tl*0.5) * (m1-m0) [w2]) + m0 [w3]) [w4]) + y0 [w5]
///   res0 = y0 + (t - x0) * m0 [w6, w7 the addition];  res1 = y1 + (t - x1) * m1 [w8, w9]
/// increasing: t = (t - x0) * gain [w10] + x0 [w11]  (blacks: x1)
///   a = (0.5*(m1-m0)) * (x1-x0) [w12];  b = m0 * (x1-x0) [w13];  c = y0 - t
///   d = sqrt(b*b - (4*a) * c [w14]);  res = ((-2*c) / (d + b [w15])) * (x1-x0) [w16] + x0 [w17]
///   res0 = x0 + (t - y0) / m0 [w18]
///   whites: res = (res - x0) / gain + x0 [w19];  new_y1 = (x1-x0) / gain + x0 [w20]
///     xd = x0 + (x1-x0)*0.99 [w21];  md = m0 + ((xd-x0) * (m1-m0) [w22]) / (x1-x0) [w23]
///     bb = 1/m1 - (2*aa) * x1 [w24];  cc = (new_y1 - bb * x1 [w25]) - (aa * x1 [w26a]) * x1 [w26b]
///     t = (t - x0) / gain + x0 [w27];  res1 = ((aa * t [w28]) + bb [w29]) * t [w30] + cc [w31]
///   blacks: res1 = x1 + (t - y1) / m1 [w32];  res = (res - x1) / gain + x1 [w33]
/// ```
#[derive(Debug, Clone, Copy, Default)]
struct WbFwdOrders {
    w1: bool,
    w2: bool,
    w3: bool,
    w4: bool,
    w5: bool,
    w6: bool,
    w7: bool,
    w8: bool,
    w9: bool,
    w10: bool,
    w11: bool,
    w12: bool,
    w13: bool,
    w14: bool,
    w15: bool,
    w16: bool,
    w17: bool,
    w18: bool,
    w19: bool,
    w20: bool,
    w21: bool,
    w22: bool,
    w23: bool,
    w24: bool,
    w25: bool,
    w26a: bool,
    w26b: bool,
    w27: bool,
    w28: bool,
    w29: bool,
    w30: bool,
    w31: bool,
    w32: bool,
    w33: bool,
}

/// The canonical order everywhere.
const WB_FWD_SRC: WbFwdOrders = WbFwdOrders {
    w1: false,
    w2: false,
    w3: false,
    w4: false,
    w5: false,
    w6: false,
    w7: false,
    w8: false,
    w9: false,
    w10: false,
    w11: false,
    w12: false,
    w13: false,
    w14: false,
    w15: false,
    w16: false,
    w17: false,
    w18: false,
    w19: false,
    w20: false,
    w21: false,
    w22: false,
    w23: false,
    w24: false,
    w25: false,
    w26a: false,
    w26b: false,
    w27: false,
    w28: false,
    w29: false,
    w30: false,
    w31: false,
    w32: false,
    w33: false,
};

/// The operand orders of `ComputeWBRev`, whose canonical form is:
///
/// ```text
/// decreasing: a = (0.5*(m1-m0)) * (x1-x0) [v1];  b = m0 * (x1-x0) [v2];  c = y0 - t
///   d = sqrt(b*b - (4*a) * c [v3]);  res = ((-2*c) / (d + b [v4])) * (x1-x0) [v5] + x0 [v6]
///   res0 = x0 + (t - y0) / m0 [v7];  res1 = x1 + (t - y1) / m1 [v8]
/// increasing: t = (t - x0) * gain [v9] + x0 [v10]  (blacks: x1);  tl = (t - x0) / (x1 - x0)
///   res = ((tl * (x1-x0) [v11]) * (((tl*0.5) * (m1-m0) [v12]) + m0 [v13]) [v14]) + y0 [v15]
///   res0 = y0 + (t - x0) * m0 [v16, v17 the addition]
///   whites: res = (res - x0) / gain + x0 [v18];  new_y1 = (x1-x0) / gain + x0 [v19]
///     xd = x0 + (x1-x0)*0.99 [v20];  md = m0 + ((xd-x0) * (m1-m0) [v21]) / (x1-x0) [v22]
///     bb = 1/m1 - (2*aa) * x1 [v23];  cc = (new_y1 - bb * x1 [v24]) - (aa * x1 [v25a]) * x1 [v25b]
///     t = (t - x0) / gain + x0 [v26];  c2 = cc - t;  d2 = sqrt(bb*bb - (4*aa) * c2 [v28])
///     res1 = (-2*c2) / (d2 + bb [v29]);  brk = (((aa * x1 [v25a]) + bb [v30b]) * x1 [v30c]) + cc [v30d]
///   blacks: res1 = y1 + (t - x1) * m1 [v31, v32 the addition];  res = (res - x1) / gain + x1 [v33]
/// ```
///
/// Both wheels compute `aa * x1` once for `cc` and `brk`.
#[derive(Debug, Clone, Copy, Default)]
struct WbRevOrders {
    v1: bool,
    v2: bool,
    v3: bool,
    v4: bool,
    v5: bool,
    v6: bool,
    v7: bool,
    v8: bool,
    v9: bool,
    v10: bool,
    v11: bool,
    v12: bool,
    v13: bool,
    v14: bool,
    v15: bool,
    v16: bool,
    v17: bool,
    v18: bool,
    v19: bool,
    v20: bool,
    v21: bool,
    v22: bool,
    v23: bool,
    v24: bool,
    v25a: bool,
    v25b: bool,
    v26: bool,
    v28: bool,
    v29: bool,
    v30b: bool,
    v30c: bool,
    v30d: bool,
    v31: bool,
    v32: bool,
    v33: bool,
}

/// The canonical order everywhere.
const WB_REV_SRC: WbRevOrders = WbRevOrders {
    v1: false,
    v2: false,
    v3: false,
    v4: false,
    v5: false,
    v6: false,
    v7: false,
    v8: false,
    v9: false,
    v10: false,
    v11: false,
    v12: false,
    v13: false,
    v14: false,
    v15: false,
    v16: false,
    v17: false,
    v18: false,
    v19: false,
    v20: false,
    v21: false,
    v22: false,
    v23: false,
    v24: false,
    v25a: false,
    v25b: false,
    v26: false,
    v28: false,
    v29: false,
    v30b: false,
    v30c: false,
    v30d: false,
    v31: false,
    v32: false,
    v33: false,
};

/// The orders of the whites and blacks templates: the `float` instantiation's by `isBlack`
/// (GCC compiled the increasing case's two paths differently), and each `float3` lane's.
#[derive(Debug, Clone, Copy)]
struct WbTables<T> {
    /// `ComputeWBXxx<float>`, for whites (`[0]`) and blacks (`[1]`).
    scalar: [T; 2],
    /// `ComputeWBXxx<float3>`'s lanes.
    f3: [T; 3],
}

/// Windows: `sub_1801f4590` (`float`) and `sub_1801f4970` (`float3`). MSVC adds `y0` (`x0`)
/// after choosing between the curve and the line below it, so the `float3` lanes' additions
/// differ in which of the two puts the coefficient first.
#[cfg(windows)]
const WB_FWD: WbTables<WbFwdOrders> = {
    const SCALAR: WbFwdOrders = WbFwdOrders {
        w2: S,
        w4: S,
        w7: S,
        w9: S,
        w13: S,
        w18: S,
        w21: S,
        w23: S,
        w28: S,
        w31: S,
        w32: S,
        ..WB_FWD_SRC
    };
    const F3: WbFwdOrders = WbFwdOrders {
        w1: S,
        w4: S,
        w5: S,
        w9: S,
        w13: S,
        w21: S,
        w23: S,
        w28: S,
        w32: S,
        ..WB_FWD_SRC
    };
    WbTables {
        scalar: [SCALAR; 2],
        f3: [
            WbFwdOrders { w17: S, ..F3 },
            WbFwdOrders { w18: S, ..F3 },
            WbFwdOrders { w18: S, ..F3 },
        ],
    }
};

/// Linux: `ComputeWBFwd<float>` (0x3d3580) and `ComputeWBFwd<float3>` (0x3d6e30).
#[cfg(not(windows))]
const WB_FWD: WbTables<WbFwdOrders> = {
    const SCALAR: WbFwdOrders = WbFwdOrders {
        w1: S,
        w4: S,
        w7: S,
        w9: S,
        w13: S,
        w18: S,
        w19: S,
        w21: S,
        w23: S,
        w25: S,
        w26a: S,
        w32: S,
        ..WB_FWD_SRC
    };
    const F3: WbFwdOrders = WbFwdOrders {
        w7: S,
        w9: S,
        w18: S,
        w21: S,
        w23: S,
        w25: S,
        w26a: S,
        w32: S,
        ..WB_FWD_SRC
    };
    WbTables {
        scalar: [SCALAR; 2],
        f3: [
            WbFwdOrders {
                w4: S,
                w14: S,
                ..F3
            },
            WbFwdOrders {
                w4: S,
                w14: S,
                w30: S,
                ..F3
            },
            WbFwdOrders {
                w1: S,
                w15: S,
                ..F3
            },
        ],
    }
};

/// Windows: `sub_1801f5090` (`float`) and `sub_1801f5440` (`float3`).
#[cfg(windows)]
const WB_REV: WbTables<WbRevOrders> = {
    const SCALAR: WbRevOrders = WbRevOrders {
        v2: S,
        v7: S,
        v8: S,
        v14: S,
        v17: S,
        v20: S,
        v22: S,
        v30b: S,
        v32: S,
        ..WB_REV_SRC
    };
    const F3: WbRevOrders = WbRevOrders {
        v2: S,
        v8: S,
        v11: S,
        v14: S,
        v20: S,
        v22: S,
        v32: S,
        ..WB_REV_SRC
    };
    WbTables {
        scalar: [SCALAR; 2],
        f3: [
            WbRevOrders {
                v6: S,
                v15: S,
                ..F3
            },
            WbRevOrders {
                v7: S,
                v17: S,
                ..F3
            },
            WbRevOrders {
                v7: S,
                v17: S,
                ..F3
            },
        ],
    }
};

/// Linux: `ComputeWBRev<float>` (0x3d3930) and `ComputeWBRev<float3>` (0x3d8810).
#[cfg(not(windows))]
const WB_REV: WbTables<WbRevOrders> = {
    const SCALAR: WbRevOrders = WbRevOrders {
        v2: S,
        v7: S,
        v8: S,
        v14: S,
        v16: S,
        v20: S,
        v22: S,
        v24: S,
        v25a: S,
        v29: S,
        v30c: S,
        v32: S,
        v33: S,
        ..WB_REV_SRC
    };
    const F3: WbRevOrders = WbRevOrders {
        v7: S,
        v8: S,
        v17: S,
        v20: S,
        v24: S,
        v25a: S,
        v25b: S,
        v32: S,
        ..WB_REV_SRC
    };
    WbTables {
        scalar: [
            WbRevOrders { v17: S, ..SCALAR },
            WbRevOrders {
                v11: S,
                v15: S,
                ..SCALAR
            },
        ],
        f3: [
            WbRevOrders {
                v3: S,
                v14: S,
                v28: S,
                ..F3
            },
            WbRevOrders {
                v3: S,
                v14: S,
                v28: S,
                ..F3
            },
            WbRevOrders {
                v4: S,
                v29: S,
                v32: false,
                ..F3
            },
        ],
    }
};

/// One value through the whites or blacks curve, or `None` where it changes nothing (a value
/// of 1, or NaN).
///
/// Port of `ComputeWBFwd` (GradingToneOpCPU.cpp:644-705 @ v2.5.2), for one `float` or one lane
/// of a `float3`.
fn compute_wb_fwd(t: f32, is_black: bool, val: f32, c: &WbCoefs, o: &WbFwdOrders) -> Option<f32> {
    let WbCoefs {
        x0,
        x1,
        y0,
        y1,
        m0,
        m1,
        gain,
    } = *c;
    let mtest = if !is_black { val } else { sub(2.0, val) };

    if mtest < 1.0 {
        // Slope is decreasing case.
        let w = sub(x1, x0);
        let tlocal = sub(t, x0) / w;
        let mut res = add(
            o.w5,
            mul(
                o.w4,
                mul(o.w1, tlocal, w),
                add(o.w3, mul(o.w2, sse_mul(tlocal, 0.5), sub(m1, m0)), m0),
            ),
            y0,
        );
        let res0 = add(o.w7, y0, mul(o.w6, sub(t, x0), m0));
        res = if t < x0 { res0 } else { res };
        let res1 = add(o.w9, y1, mul(o.w8, sub(t, x1), m1));
        res = if t < x1 { res } else { res1 };
        Some(res)
    } else if mtest > 1.0 {
        // Slope is increasing case.
        let t = if !is_black {
            add(o.w11, mul(o.w10, sub(t, x0), gain), x0)
        } else {
            add(o.w11, mul(o.w10, sub(t, x1), gain), x1)
        };

        let w = sub(x1, x0);
        let a = mul(o.w12, sse_mul(0.5, sub(m1, m0)), w);
        let b = mul(o.w13, m0, w);

        let c = sub(y0, t);
        let discrim = sqrt(sub(sse_mul(b, b), mul(o.w14, sse_mul(4.0, a), c)));
        let tmp = sse_mul(c, -2.0) / add(o.w15, discrim, b);
        let mut res = add(o.w17, mul(o.w16, tmp, w), x0);
        let res0 = add(o.w18, x0, sub(t, y0) / m0);
        res = if t < y0 { res0 } else { res };

        if !is_black {
            res = add(o.w19, sub(res, x0) / gain, x0);
            // Quadratic extrapolation for better HDR control.
            let new_y1 = add(o.w20, w / gain, x0);
            let xd = add(o.w21, x0, sse_mul(w, 0.99));
            let mut md = add(o.w23, m0, mul(o.w22, sub(xd, x0), sub(m1, m0)) / w);
            md = 1.0 / md;
            let aa = sse_mul(0.5, sub(1.0 / m1, md)) / sub(x1, xd);
            let bb = sub(1.0 / m1, mul(o.w24, sse_add(aa, aa), x1));
            let cc = sub(
                sub(new_y1, mul(o.w25, bb, x1)),
                mul(o.w26b, mul(o.w26a, aa, x1), x1),
            );
            let t = add(o.w27, sub(t, x0) / gain, x0);

            let res1 = add(o.w31, mul(o.w30, add(o.w29, mul(o.w28, aa, t), bb), t), cc);
            res = if t < x1 { res } else { res1 };
        } else {
            let res1 = add(o.w32, x1, sub(t, y1) / m1);
            res = if t < y1 { res } else { res1 };
            res = add(o.w33, sub(res, x1) / gain, x1);
        }
        Some(res)
    } else {
        None
    }
}

/// One value through the inverse of the whites or blacks curve, or `None` where it changes
/// nothing.
///
/// Port of `ComputeWBRev` (GradingToneOpCPU.cpp:707-773 @ v2.5.2), for one `float` or one lane
/// of a `float3`.
fn compute_wb_rev(t: f32, is_black: bool, val: f32, c: &WbCoefs, o: &WbRevOrders) -> Option<f32> {
    let WbCoefs {
        x0,
        x1,
        y0,
        y1,
        m0,
        m1,
        gain,
    } = *c;
    let mtest = if !is_black { val } else { sub(2.0, val) };

    if mtest < 1.0 {
        // Slope is decreasing case.
        let w = sub(x1, x0);
        let a = mul(o.v1, sse_mul(0.5, sub(m1, m0)), w);
        let b = mul(o.v2, m0, w);

        let c = sub(y0, t);
        let discrim = sqrt(sub(sse_mul(b, b), mul(o.v3, sse_mul(4.0, a), c)));
        let tmp = sse_mul(c, -2.0) / add(o.v4, discrim, b);
        let mut res = add(o.v6, mul(o.v5, tmp, w), x0);
        let res0 = add(o.v7, x0, sub(t, y0) / m0);
        res = if t < y0 { res0 } else { res };

        let res1 = add(o.v8, x1, sub(t, y1) / m1);
        res = if t < y1 { res } else { res1 };
        Some(res)
    } else if mtest > 1.0 {
        // Slope is increasing case.
        let t = if !is_black {
            add(o.v10, mul(o.v9, sub(t, x0), gain), x0)
        } else {
            add(o.v10, mul(o.v9, sub(t, x1), gain), x1)
        };

        let w = sub(x1, x0);
        let tlocal = sub(t, x0) / w;
        let mut res = add(
            o.v15,
            mul(
                o.v14,
                mul(o.v11, tlocal, w),
                add(o.v13, mul(o.v12, sse_mul(tlocal, 0.5), sub(m1, m0)), m0),
            ),
            y0,
        );
        let res0 = add(o.v17, y0, mul(o.v16, sub(t, x0), m0));
        res = if t < x0 { res0 } else { res };

        if !is_black {
            res = add(o.v18, sub(res, x0) / gain, x0);
            // Quadratic extrapolation for better HDR control.
            let new_y1 = add(o.v19, w / gain, x0);
            let xd = add(o.v20, x0, sse_mul(w, 0.99));
            let mut md = add(o.v22, m0, mul(o.v21, sub(xd, x0), sub(m1, m0)) / w);
            md = 1.0 / md;
            let aa = sse_mul(0.5, sub(1.0 / m1, md)) / sub(x1, xd);
            let bb = sub(1.0 / m1, mul(o.v23, sse_add(aa, aa), x1));
            let aa_x1 = mul(o.v25a, aa, x1);
            let cc = sub(sub(new_y1, mul(o.v24, bb, x1)), mul(o.v25b, aa_x1, x1));
            let t = add(o.v26, sub(t, x0) / gain, x0);

            let c = sub(cc, t);
            let discrim = sqrt(sub(sse_mul(bb, bb), mul(o.v28, sse_mul(4.0, aa), c)));
            let res1 = sse_mul(c, -2.0) / add(o.v29, discrim, bb);
            let brk = add(o.v30d, mul(o.v30c, add(o.v30b, aa_x1, bb), x1), cc);
            res = if t < brk { res } else { res1 };
        } else {
            let res1 = add(o.v32, y1, mul(o.v31, sub(t, x1), m1));
            res = if t < x1 { res } else { res1 };
            res = add(o.v33, sub(res, x1) / gain, x1);
        }
        Some(res)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Midtones.
// ---------------------------------------------------------------------------------------------

/// The control points and slopes of a midtones curve (`GradingTonePreRender::m_midX`,
/// `m_midY`, `m_midM`).
#[derive(Debug, Clone, Copy)]
struct MidCoefs {
    x: [f32; 6],
    y: [f32; 6],
    m: [f32; 6],
}

/// The operand orders of the forward midtones, whose canonical form is, for each segment `k`
/// between `x[k]` and `x[k + 1]`:
///
/// ```text
/// tk = (t - x[k]) / dx;  dx = x[k+1] - x[k];  dm = m[k+1] - m[k]
/// fk = ((tk * dx [a]) * (((tk*0.5) * dm [b]) + m[k] [c]) [d]) + y[k] [e]
/// r0 = (t - x0) * m0 [m6] + y0 [m6s];  r5 = (t - x5) * m5 [m7] + y5 [m7s]
/// ```
///
/// The `float` source writes `y0 + (t - x0) * m0` and `y5 + (t - x5) * m5`; the `float3` one
/// writes the canonical form.
#[derive(Debug, Clone, Copy, Default)]
struct MidsFwdOrders {
    a: [bool; 5],
    b: [bool; 5],
    c: [bool; 5],
    d: [bool; 5],
    e: [bool; 5],
    m6: bool,
    m6s: bool,
    m7: bool,
    m7s: bool,
}

/// The canonical order everywhere.
const MIDS_FWD_SRC: MidsFwdOrders = MidsFwdOrders {
    a: [false; 5],
    b: [false; 5],
    c: [false; 5],
    d: [false; 5],
    e: [false; 5],
    m6: false,
    m6s: false,
    m7: false,
    m7s: false,
};

/// The operand orders of the inverse midtones, whose canonical form is, for each segment `k`
/// between `y[k]` and `y[k + 1]`:
///
/// ```text
/// c = y[k] - t;  dx = x[k+1] - x[k];  b = m[k] * dx [b];  a = (0.5 * (m[k+1] - m[k])) * dx
/// d = sqrt(b*b - (4*a) * c [c]);  res = ((2*c) / ((-d) - b)) * dx [d] + x[k] [e]
///   (the float3 code's denominator is (-b) - d)
/// ends: x0 + (t - y0) / m0 [end, end_l], x5 + (t - y5) / m5 [end_r]
/// ```
#[derive(Debug, Clone, Copy, Default)]
struct MidsRevOrders {
    b: [bool; 5],
    c: [bool; 5],
    d: [bool; 5],
    e: [bool; 5],
    /// The `float` code's ends (both `x0 + (t - y0) / m0`).
    end: bool,
    /// The `float3` code's `outL0`.
    end_l: bool,
    /// The `float3` code's `outR4`.
    end_r: bool,
}

/// The canonical order everywhere.
const MIDS_REV_SRC: MidsRevOrders = MidsRevOrders {
    b: [false; 5],
    c: [false; 5],
    d: [false; 5],
    e: [false; 5],
    end: false,
    end_l: false,
    end_r: false,
};

/// Windows: `sub_1801f79b0`, which holds both the `float` and the `float3` code.
#[cfg(windows)]
const MIDS_FWD: Tables<MidsFwdOrders> = {
    const F3: MidsFwdOrders = MidsFwdOrders {
        a: [S; 5],
        d: [S; 5],
        m6: S,
        ..MIDS_FWD_SRC
    };
    Tables {
        scalar: MidsFwdOrders {
            a: [S, false, false, false, false],
            b: [S; 5],
            d: [S; 5],
            ..MIDS_FWD_SRC
        },
        f3: [F3; 3],
    }
};

/// Linux: `GradingToneFwdOpCPU::mids` (0x3d3e50), which holds both.
#[cfg(not(windows))]
const MIDS_FWD: Tables<MidsFwdOrders> = Tables {
    scalar: MidsFwdOrders {
        a: [S; 5],
        d: [S; 5],
        e: [S, false, false, false, false],
        m6s: S,
        ..MIDS_FWD_SRC
    },
    f3: [
        MidsFwdOrders {
            b: [S, false, false, false, false],
            d: [S; 5],
            ..MIDS_FWD_SRC
        },
        MidsFwdOrders {
            d: [S; 5],
            ..MIDS_FWD_SRC
        },
        MidsFwdOrders {
            a: [S, S, S, S, false],
            b: [false, false, false, false, S],
            d: [S; 5],
            e: [S, false, false, S, false],
            m7: S,
            ..MIDS_FWD_SRC
        },
    ],
};

/// Windows: `sub_1801f8350`, which holds both.
#[cfg(windows)]
const MIDS_REV: Tables<MidsRevOrders> = {
    const F3: MidsRevOrders = MidsRevOrders {
        c: [false, false, false, false, S],
        end_l: S,
        end_r: S,
        ..MIDS_REV_SRC
    };
    Tables {
        scalar: MidsRevOrders {
            end: S,
            ..MIDS_REV_SRC
        },
        f3: [F3; 3],
    }
};

/// Linux: `GradingToneRevOpCPU::mids` (0x3d9420), which holds both.
#[cfg(not(windows))]
const MIDS_REV: Tables<MidsRevOrders> = {
    const L01: MidsRevOrders = MidsRevOrders {
        b: [false, S, S, S, S],
        c: [S; 5],
        end_l: S,
        end_r: S,
        ..MIDS_REV_SRC
    };
    Tables {
        scalar: MidsRevOrders {
            b: [S, false, false, false, false],
            end: S,
            ..MIDS_REV_SRC
        },
        f3: [
            L01,
            L01,
            MidsRevOrders {
                b: [false, S, S, S, S],
                end_l: S,
                end_r: S,
                ..MIDS_REV_SRC
            },
        ],
    }
};

/// The forward midtones curve's segment `k` at `t`.
fn mids_fwd_segment(t: f32, mc: &MidCoefs, k: usize, o: &MidsFwdOrders) -> f32 {
    let MidCoefs { x, y, m } = mc;
    let dx = sub(x[k + 1], x[k]);
    let tk = sub(t, x[k]) / dx;
    add(
        o.e[k],
        mul(
            o.d[k],
            mul(o.a[k], tk, dx),
            add(
                o.c[k],
                mul(o.b[k], sse_mul(tk, 0.5), sub(m[k + 1], m[k])),
                m[k],
            ),
        ),
        y[k],
    )
}

/// One channel through the forward midtones curve.
///
/// Port of the `channel != M` branch of `GradingToneFwdOpCPU::mids` (GradingToneOpCPU.cpp:
/// 295-317 @ v2.5.2).
fn mids_fwd_scalar(t: f32, mc: &MidCoefs, o: &MidsFwdOrders) -> f32 {
    let MidCoefs { x, y, m } = mc;
    let f = |k| mids_fwd_segment(t, mc, k, o);
    let mut res = if t < x[1] { f(0) } else { f(1) };
    if t > x[2] {
        res = f(2);
    }
    if t > x[3] {
        res = f(3);
    }
    if t > x[4] {
        res = f(4);
    }
    if t < x[0] {
        res = add(o.m6s, mul(o.m6, sub(t, x[0]), m[0]), y[0]);
    }
    if t > x[5] {
        res = add(o.m7s, mul(o.m7, sub(t, x[5]), m[5]), y[5]);
    }
    res
}

/// One lane of the master component through the forward midtones curve. The segments are
/// chosen with `setOnLimit` (`t < x ? below : above`), so a value on a knot, or a NaN, takes
/// the segment above it, unlike the `float` code.
///
/// Port of the `channel == M` branch of `GradingToneFwdOpCPU::mids` (GradingToneOpCPU.cpp:
/// 318-347 @ v2.5.2), for one lane.
fn mids_fwd_f3(t: f32, mc: &MidCoefs, o: &MidsFwdOrders) -> f32 {
    let MidCoefs { x, y, m } = mc;
    let f = |k| mids_fwd_segment(t, mc, k, o);
    let f_r4 = add(o.m6s, mul(o.m6, sub(t, x[0]), m[0]), y[0]);
    let f_r5 = add(o.m7s, mul(o.m7, sub(t, x[5]), m[5]), y[5]);

    let mut res = if t < x[1] { f(0) } else { f(1) };
    res = if t < x[2] { res } else { f(2) };
    res = if t < x[3] { res } else { f(3) };
    res = if t < x[4] { res } else { f(4) };
    res = if t < x[0] { f_r4 } else { res };
    if t < x[5] { res } else { f_r5 }
}

/// The inverse midtones curve's segment `k` at `t`: the root of its quadratic, with the
/// `float` code's denominator `(-d) - b` or the `float3` code's `(-b) - d`.
fn mids_rev_segment(t: f32, mc: &MidCoefs, k: usize, f3: bool, o: &MidsRevOrders) -> f32 {
    let MidCoefs { x, y, m } = mc;
    let c = sub(y[k], t);
    let dx = sub(x[k + 1], x[k]);
    let b = mul(o.b[k], m[k], dx);
    let a = sse_mul(sse_mul(0.5, sub(m[k + 1], m[k])), dx);
    let discrim = sqrt(sub(sse_mul(b, b), mul(o.c[k], sse_mul(4.0, a), c)));
    let den = if f3 {
        sub(-b, discrim)
    } else {
        sub(-discrim, b)
    };
    let tmp = sse_mul(2.0, c) / den;
    add(o.e[k], mul(o.d[k], tmp, dx), x[k])
}

/// One channel through the inverse midtones curve. Above `y5` it uses the line below `y0`
/// (`x0 + (t - y0) / m0`), not the one above `y5` as the `float3` code does: an upstream bug,
/// ported (docs/improvements.md, I-241).
///
/// Port of the `channel != M` branch of `GradingToneRevOpCPU::mids` (GradingToneOpCPU.cpp:
/// 377-435 @ v2.5.2).
fn mids_rev_scalar(t: f32, mc: &MidCoefs, o: &MidsRevOrders) -> f32 {
    let MidCoefs { x, y, m } = mc;
    let end = || add(o.end, x[0], sub(t, y[0]) / m[0]);
    if t >= y[5] {
        end()
    } else if t >= y[4] {
        mids_rev_segment(t, mc, 4, false, o)
    } else if t >= y[3] {
        mids_rev_segment(t, mc, 3, false, o)
    } else if t >= y[2] {
        mids_rev_segment(t, mc, 2, false, o)
    } else if t >= y[1] {
        mids_rev_segment(t, mc, 1, false, o)
    } else if t >= y[0] {
        mids_rev_segment(t, mc, 0, false, o)
    } else {
        end()
    }
}

/// One lane of the master component through the inverse midtones curve.
///
/// Port of the `channel == M` branch of `GradingToneRevOpCPU::mids` (GradingToneOpCPU.cpp:
/// 436-495 @ v2.5.2), for one lane.
fn mids_rev_f3(t: f32, mc: &MidCoefs, o: &MidsRevOrders) -> f32 {
    let MidCoefs { x, y, m } = mc;
    let out_r4 = add(o.end_r, x[5], sub(t, y[5]) / m[5]);
    let seg = |k| mids_rev_segment(t, mc, k, true, o);
    let (out_r3, out_r2, out_r, out_m, out_l) = (seg(4), seg(3), seg(2), seg(1), seg(0));
    let out_l0 = add(o.end_l, x[0], sub(t, y[0]) / m[0]);

    let mut res = if t < y[1] { out_l } else { out_m };
    res = if t < y[2] { res } else { out_r };
    res = if t < y[3] { res } else { out_r2 };
    res = if t < y[4] { res } else { out_r3 };
    res = if t < y[0] { out_l0 } else { res };
    if t < y[5] { res } else { out_r4 }
}

// ---------------------------------------------------------------------------------------------
// S-contrast.
// ---------------------------------------------------------------------------------------------

/// The operand orders of the forward s-contrast, whose canonical form is:
///
/// ```text
/// out = (t - pivot) * c [s1] + pivot [s2]
/// top: tR = (t - x1) / dx;  res = ((tR * dx [s3]) * (((tR*0.5) * (m3-m0) [s4]) + m0 [s5]) [s6]) + y1 [s7]
///      res2 = y2 + (t - x2) * m3 [s8, s9 the addition]
/// bottom: res = (... [s10] ... [s11] ... [s12] [s13]) + y1 [s14];  res1 = y1 + (t - x1) * m0 [s15, s16]
/// ```
#[derive(Debug, Clone, Copy, Default)]
struct ScFwdOrders {
    s1: bool,
    s2: bool,
    s3: bool,
    s4: bool,
    s5: bool,
    s6: bool,
    s7: bool,
    s8: bool,
    s9: bool,
    s10: bool,
    s11: bool,
    s12: bool,
    s13: bool,
    s14: bool,
    s15: bool,
    s16: bool,
}

/// The canonical order everywhere.
const SC_FWD_SRC: ScFwdOrders = ScFwdOrders {
    s1: false,
    s2: false,
    s3: false,
    s4: false,
    s5: false,
    s6: false,
    s7: false,
    s8: false,
    s9: false,
    s10: false,
    s11: false,
    s12: false,
    s13: false,
    s14: false,
    s15: false,
    s16: false,
};

/// The operand orders of the inverse s-contrast, whose canonical form is:
///
/// ```text
/// out = (t - pivot) / c + pivot [r1]
/// top: b = m0 * dx [r2];  a = ((m3-m0) * 0.5) * dx;  cc = y1 - t;  d = sqrt(b*b - (4*a) * cc [r3])
///      res = ((dx * (-2*cc) [r4]) / (d + b [r5])) + x1 [r6];  x2 + (t - y2) / m3 [r7]
/// bottom: b [r8];  d [r9];  res = ((dx * (-2*cc) [r10]) / (d + b [r11])) + x1 [r12];  x1 + (t - y1) / m0 [r13]
/// ```
#[derive(Debug, Clone, Copy, Default)]
struct ScRevOrders {
    r1: bool,
    r2: bool,
    r3: bool,
    r4: bool,
    r5: bool,
    r6: bool,
    r7: bool,
    r8: bool,
    r9: bool,
    r10: bool,
    r11: bool,
    r12: bool,
    r13: bool,
    /// The roots computed as `x1 - ((cc + cc) * dx) / (d + b)` (MSVC), which put a NaN `x1`
    /// first; `r6` and `r12` are then unused.
    subform: bool,
}

/// The canonical order everywhere.
const SC_REV_SRC: ScRevOrders = ScRevOrders {
    r1: false,
    r2: false,
    r3: false,
    r4: false,
    r5: false,
    r6: false,
    r7: false,
    r8: false,
    r9: false,
    r10: false,
    r11: false,
    r12: false,
    r13: false,
    subform: false,
};

/// Windows: `sub_1801f8f90`; Linux: `GradingToneFwdOpCPU::scontrast` (0x3d5560); by lane.
#[cfg(windows)]
const SC_FWD: [ScFwdOrders; 3] = {
    const W: ScFwdOrders = ScFwdOrders {
        s3: S,
        s6: S,
        s9: S,
        s10: S,
        s13: S,
        s15: S,
        s16: S,
        ..SC_FWD_SRC
    };
    [W, W, ScFwdOrders { s15: false, ..W }]
};

/// Windows: `sub_1801f8f90`; Linux: `GradingToneFwdOpCPU::scontrast` (0x3d5560); by lane.
#[cfg(not(windows))]
const SC_FWD: [ScFwdOrders; 3] = {
    const L: ScFwdOrders = ScFwdOrders {
        s6: S,
        s9: S,
        s13: S,
        s16: S,
        ..SC_FWD_SRC
    };
    [
        L,
        L,
        ScFwdOrders {
            s1: S,
            s3: S,
            s10: S,
            ..L
        },
    ]
};

/// Windows: `sub_1801f93a0`; Linux: `GradingToneRevOpCPU::scontrast` (0x3d64c0); by lane.
#[cfg(windows)]
const SC_REV: [ScRevOrders; 3] = {
    const W: ScRevOrders = ScRevOrders {
        r4: S,
        r7: S,
        r8: S,
        r10: S,
        r13: S,
        subform: true,
        ..SC_REV_SRC
    };
    [W; 3]
};

/// Windows: `sub_1801f93a0`; Linux: `GradingToneRevOpCPU::scontrast` (0x3d64c0); by lane.
#[cfg(not(windows))]
const SC_REV: [ScRevOrders; 3] = {
    const L: ScRevOrders = ScRevOrders {
        r4: S,
        r7: S,
        r10: S,
        r13: S,
        ..SC_REV_SRC
    };
    [
        ScRevOrders { r3: S, r9: S, ..L },
        ScRevOrders { r3: S, r9: S, ..L },
        ScRevOrders { r5: S, ..L },
    ]
};

/// The s-contrast's slope at the pivot: `1 / (1.8125 - 0.8125 * min(contrast, 1.99))` above
/// 1, `0.28125 + 0.71875 * max(contrast, 0.01)` below, limited to prevent reversals.
///
/// Part of `GradingToneFwdOpCPU::scontrast` (GradingToneOpCPU.cpp:829-834 @ v2.5.2), and of
/// the reverse one (882-887).
fn sc_slope(contrast: f32) -> f32 {
    if contrast > 1.0 {
        1.0 / sub(1.8125, sse_mul(0.8125, std_min(contrast, 1.99)))
    } else {
        sse_add(0.28125, sse_mul(0.71875, std_max(contrast, 0.01)))
    }
}

/// One lane of the color channels through the forward s-contrast.
///
/// Port of the `contrast != 1.` branch of `GradingToneFwdOpCPU::scontrast`
/// (GradingToneOpCPU.cpp:836-876 @ v2.5.2), for one lane.
fn sc_fwd(t: f32, contrast: f32, vpr: &GradingTonePreRender, o: &ScFwdOrders) -> f32 {
    let pivot = vpr.pivot;
    let mut out_color = add(o.s2, mul(o.s1, sub(t, pivot), contrast), pivot);

    // Top end
    {
        let x1 = vpr.sc_x[0][1];
        let x2 = vpr.sc_x[0][2];
        let y1 = vpr.sc_y[0][1];
        let y2 = vpr.sc_y[0][2];
        let m0 = vpr.sc_m[0][0];
        let m3 = vpr.sc_m[0][1];

        let dx = sub(x2, x1);
        let t_r = sub(t, x1) / dx;
        let res = add(
            o.s7,
            mul(
                o.s6,
                mul(o.s3, t_r, dx),
                add(o.s5, mul(o.s4, sse_mul(t_r, 0.5), sub(m3, m0)), m0),
            ),
            y1,
        );
        out_color = if t < x1 { out_color } else { res };

        let res2 = add(o.s9, y2, mul(o.s8, sub(t, x2), m3));
        out_color = if t < x2 { out_color } else { res2 };
    }

    // Bottom end
    {
        let x1 = vpr.sc_x[1][1];
        let x2 = vpr.sc_x[1][2];
        let y1 = vpr.sc_y[1][1];
        let m0 = vpr.sc_m[1][0];
        let m3 = vpr.sc_m[1][1];

        let dx = sub(x2, x1);
        let t_r = sub(t, x1) / dx;
        let res = add(
            o.s14,
            mul(
                o.s13,
                mul(o.s10, t_r, dx),
                add(o.s12, mul(o.s11, sse_mul(t_r, 0.5), sub(m3, m0)), m0),
            ),
            y1,
        );
        out_color = if t < x2 { res } else { out_color };

        let res1 = add(o.s16, y1, mul(o.s15, sub(t, x1), m0));
        out_color = if t < x1 { res1 } else { out_color };
    }
    out_color
}

/// The root of an s-contrast end's quadratic: `((dx * (-2*cc)) / (d + b)) + x1`, or MSVC's
/// `x1 - ((cc + cc) * dx) / (d + b)`.
fn sc_root(
    subform: bool,
    add_first: bool,
    mul_first: bool,
    cc: f32,
    den: f32,
    dx: f32,
    x1: f32,
) -> f32 {
    if subform {
        sub(x1, mul(mul_first, dx, sse_add(cc, cc)) / den)
    } else {
        add(add_first, mul(mul_first, dx, sse_mul(cc, -2.0)) / den, x1)
    }
}

/// One lane of the color channels through the inverse s-contrast.
///
/// Port of the `contrast != 1.` branch of `GradingToneRevOpCPU::scontrast`
/// (GradingToneOpCPU.cpp:889-932 @ v2.5.2), for one lane.
fn sc_rev(t: f32, contrast: f32, vpr: &GradingTonePreRender, o: &ScRevOrders) -> f32 {
    let pivot = vpr.pivot;
    let mut out_color = add(o.r1, sub(t, pivot) / contrast, pivot);

    // Top end
    {
        let x1 = vpr.sc_x[0][1];
        let x2 = vpr.sc_x[0][2];
        let y1 = vpr.sc_y[0][1];
        let y2 = vpr.sc_y[0][2];
        let m0 = vpr.sc_m[0][0];
        let m3 = vpr.sc_m[0][1];

        let dx = sub(x2, x1);
        let b = mul(o.r2, m0, dx);
        let a = sse_mul(sse_mul(sub(m3, m0), 0.5), dx);
        let c = sub(y1, t);
        let discrim = sqrt(sub(sse_mul(b, b), mul(o.r3, sse_mul(4.0, a), c)));
        let res = sc_root(o.subform, o.r6, o.r4, c, add(o.r5, discrim, b), dx, x1);

        out_color = if t < y1 { out_color } else { res };
        let res2 = add(o.r7, x2, sub(t, y2) / m3);
        out_color = if t < y2 { out_color } else { res2 };
    }

    // Bottom end
    {
        let x1 = vpr.sc_x[1][1];
        let x2 = vpr.sc_x[1][2];
        let y1 = vpr.sc_y[1][1];
        let y2 = vpr.sc_y[1][2];
        let m0 = vpr.sc_m[1][0];
        let m3 = vpr.sc_m[1][1];

        let dx = sub(x2, x1);
        let b = mul(o.r8, m0, dx);
        let a = sse_mul(sse_mul(sub(m3, m0), 0.5), dx);
        let c = sub(y1, t);
        let discrim = sqrt(sub(sse_mul(b, b), mul(o.r9, sse_mul(4.0, a), c)));
        let res = sc_root(o.subform, o.r12, o.r10, c, add(o.r11, discrim, b), dx, x1);

        out_color = if t < y2 { res } else { out_color };
        let res1 = add(o.r13, x1, sub(t, y1) / m0);
        out_color = if t < y1 { res1 } else { out_color };
    }
    out_color
}

// ---------------------------------------------------------------------------------------------
// The tones of one pixel.
// ---------------------------------------------------------------------------------------------

/// The midtones of `channel`.
fn mid_coefs(vpr: &GradingTonePreRender, channel: RgbmChannel) -> MidCoefs {
    let c = channel as usize;
    MidCoefs {
        x: vpr.mid_x[c],
        y: vpr.mid_y[c],
        m: vpr.mid_m[c],
    }
}

/// The highlights (`[0]`) or shadows (`[1]`) curve of `channel`.
fn hs_coefs(vpr: &GradingTonePreRender, channel: RgbmChannel, is_shadow: bool) -> HsCoefs {
    let s = usize::from(is_shadow);
    let c = channel as usize;
    let [x0, x1, x2] = vpr.hs_x[s][c];
    let [y0, y1, y2] = vpr.hs_y[s][c];
    let [m0, m2] = vpr.hs_m[s][c];
    HsCoefs {
        x0,
        x1,
        x2,
        y0,
        y1,
        y2,
        m0,
        m2,
    }
}

/// The whites (`[0]`) or blacks (`[1]`) curve of `channel`.
fn wb_coefs(vpr: &GradingTonePreRender, channel: RgbmChannel, is_black: bool) -> WbCoefs {
    let b = usize::from(is_black);
    let c = channel as usize;
    let [x0, x1] = vpr.wb_x[b][c];
    let [y0, y1] = vpr.wb_y[b][c];
    let [m0, m1] = vpr.wb_m[b][c];
    WbCoefs {
        x0,
        x1,
        y0,
        y1,
        m0,
        m1,
        gain: vpr.wb_gain[b][c],
    }
}

/// The midtones' component of `channel`, limited to [0.01, 1.99]: 1 changes nothing.
///
/// Part of `GradingToneFwdOpCPU::mids` (GradingToneOpCPU.cpp:272-274 @ v2.5.2), and of the
/// reverse one (354-356).
fn mid_adj(v: &GradingTone, channel: RgbmChannel) -> f32 {
    clamp(get_channel_value(&v.midtones, channel), 0.01, 1.99)
}

/// Where a component's value selects the curve or its inverse: the highlights' and shadows'
/// value is symmetric around 1 (below 1 the forward curve, above it the reverse one), the
/// highlights' mirrored; 1 changes nothing.
///
/// Part of `GradingToneFwdOpCPU::highlightShadow` (GradingToneOpCPU.cpp:549-556 @ v2.5.2), and
/// of the reverse one (599-605).
fn hs_val(v: &GradingTone, channel: RgbmChannel, is_shadow: bool) -> Option<f32> {
    let mut val = if is_shadow {
        get_channel_value(&v.shadows, channel)
    } else {
        get_channel_value(&v.highlights, channel)
    };
    if !is_shadow {
        val = sub(2.0, val);
    }
    (val != 1.0).then_some(val)
}

/// The whites' or blacks' component of `channel`.
fn wb_val(v: &GradingTone, channel: RgbmChannel, is_black: bool) -> f32 {
    if is_black {
        get_channel_value(&v.blacks, channel)
    } else {
        get_channel_value(&v.whites, channel)
    }
}

/// The forward direction's tones of one pixel, but its clamp.
///
/// Port of `GradingToneFwdOpCPU::mids` (GradingToneOpCPU.cpp:269-349 @ v2.5.2),
/// `highlightShadow` (545-593), `whiteBlack` (775-799) and `scontrast` (827-878).
#[derive(Debug, Clone, Copy)]
struct Fwd;

/// The inverse direction's tones of one pixel, but its clamp.
///
/// Port of `GradingToneRevOpCPU::mids` (GradingToneOpCPU.cpp:351-497 @ v2.5.2),
/// `highlightShadow` (595-642), `whiteBlack` (801-825) and `scontrast` (880-934).
#[derive(Debug, Clone, Copy)]
struct Rev;

impl Fwd {
    fn mids(v: &GradingTone, vpr: &GradingTonePreRender, channel: RgbmChannel, out: &mut [f32; 4]) {
        if mid_adj(v, channel) != 1.0 {
            let mc = mid_coefs(vpr, channel);
            if channel != RgbmChannel::M {
                let c = channel as usize;
                out[c] = mids_fwd_scalar(out[c], &mc, &MIDS_FWD.scalar);
            } else {
                for (i, o) in MIDS_FWD.f3.iter().enumerate() {
                    out[i] = mids_fwd_f3(out[i], &mc, o);
                }
            }
        }
    }

    fn highlight_shadow(
        v: &GradingTone,
        vpr: &GradingTonePreRender,
        channel: RgbmChannel,
        is_shadow: bool,
        out: &mut [f32; 4],
    ) {
        let Some(val) = hs_val(v, channel, is_shadow) else {
            return;
        };
        let hc = hs_coefs(vpr, channel, is_shadow);
        if channel != RgbmChannel::M {
            let c = channel as usize;
            out[c] = if val < 1.0 {
                compute_hs_fwd(out[c], &hc, &HS_FWD.scalar)
            } else {
                compute_hs_rev(out[c], &hc, &HS_REV.scalar)
            };
        } else {
            for (i, o) in out.iter_mut().take(3).enumerate() {
                *o = if val < 1.0 {
                    compute_hs_fwd(*o, &hc, &HS_FWD.f3[i])
                } else {
                    compute_hs_rev(*o, &hc, &HS_REV.f3[i])
                };
            }
        }
    }

    fn white_black(
        v: &GradingTone,
        vpr: &GradingTonePreRender,
        channel: RgbmChannel,
        is_black: bool,
        out: &mut [f32; 4],
    ) {
        let val = wb_val(v, channel, is_black);
        let wc = wb_coefs(vpr, channel, is_black);
        if channel != RgbmChannel::M {
            let c = channel as usize;
            let o = &WB_FWD.scalar[usize::from(is_black)];
            if let Some(res) = compute_wb_fwd(out[c], is_black, val, &wc, o) {
                out[c] = res;
            }
        } else {
            for (i, o) in WB_FWD.f3.iter().enumerate() {
                if let Some(res) = compute_wb_fwd(out[i], is_black, val, &wc, o) {
                    out[i] = res;
                }
            }
        }
    }

    fn scontrast(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut [f32; 4]) {
        let contrast = v.scontrast as f32;
        if contrast != 1.0 {
            let contrast = sc_slope(contrast);
            for (i, o) in SC_FWD.iter().enumerate() {
                out[i] = sc_fwd(out[i], contrast, vpr, o);
            }
        }
    }
}

impl Rev {
    fn mids(v: &GradingTone, vpr: &GradingTonePreRender, channel: RgbmChannel, out: &mut [f32; 4]) {
        if mid_adj(v, channel) != 1.0 {
            let mc = mid_coefs(vpr, channel);
            if channel != RgbmChannel::M {
                let c = channel as usize;
                out[c] = mids_rev_scalar(out[c], &mc, &MIDS_REV.scalar);
            } else {
                for (i, o) in MIDS_REV.f3.iter().enumerate() {
                    out[i] = mids_rev_f3(out[i], &mc, o);
                }
            }
        }
    }

    fn highlight_shadow(
        v: &GradingTone,
        vpr: &GradingTonePreRender,
        channel: RgbmChannel,
        is_shadow: bool,
        out: &mut [f32; 4],
    ) {
        let Some(val) = hs_val(v, channel, is_shadow) else {
            return;
        };
        let hc = hs_coefs(vpr, channel, is_shadow);
        if channel != RgbmChannel::M {
            let c = channel as usize;
            out[c] = if val < 1.0 {
                compute_hs_rev(out[c], &hc, &HS_REV.scalar)
            } else {
                compute_hs_fwd(out[c], &hc, &HS_FWD.scalar)
            };
        } else {
            for (i, o) in out.iter_mut().take(3).enumerate() {
                *o = if val < 1.0 {
                    compute_hs_rev(*o, &hc, &HS_REV.f3[i])
                } else {
                    compute_hs_fwd(*o, &hc, &HS_FWD.f3[i])
                };
            }
        }
    }

    fn white_black(
        v: &GradingTone,
        vpr: &GradingTonePreRender,
        channel: RgbmChannel,
        is_black: bool,
        out: &mut [f32; 4],
    ) {
        let val = wb_val(v, channel, is_black);
        let wc = wb_coefs(vpr, channel, is_black);
        if channel != RgbmChannel::M {
            let c = channel as usize;
            let o = &WB_REV.scalar[usize::from(is_black)];
            if let Some(res) = compute_wb_rev(out[c], is_black, val, &wc, o) {
                out[c] = res;
            }
        } else {
            for (i, o) in WB_REV.f3.iter().enumerate() {
                if let Some(res) = compute_wb_rev(out[i], is_black, val, &wc, o) {
                    out[i] = res;
                }
            }
        }
    }

    fn scontrast(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut [f32; 4]) {
        let contrast = v.scontrast as f32;
        if contrast != 1.0 {
            let contrast = sc_slope(contrast);
            for (i, o) in SC_REV.iter().enumerate() {
                out[i] = sc_rev(out[i], contrast, vpr, o);
            }
        }
    }
}

/// `std::min(out[i], 65504.f)` on the color channels: the grading controls at high values can
/// push values above the largest half float, where they would overflow to infinity. A NaN
/// stays.
///
/// Port of `ClampMaxRGB` (GradingToneOpCPU.cpp:938-946 @ v2.5.2).
fn clamp_max_rgb(out: &mut [f32; 4]) {
    out[0] = std_min(out[0], 65504.0);
    out[1] = std_min(out[1], 65504.0);
    out[2] = std_min(out[2], 65504.0);
}

/// The forward tones of one pixel, in upstream's order, and the clamp.
///
/// Part of `GradingToneFwdOpCPU::apply` (GradingToneOpCPU.cpp:972-999 @ v2.5.2).
fn fwd_pixel(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut [f32; 4]) {
    use RgbmChannel::{B, G, M, R};
    for ch in [R, G, B, M] {
        Fwd::mids(v, vpr, ch, out);
    }
    for ch in [R, G, B, M] {
        Fwd::highlight_shadow(v, vpr, ch, false, out);
    }
    for ch in [R, G, B, M] {
        Fwd::white_black(v, vpr, ch, false, out);
    }
    for ch in [R, G, B, M] {
        Fwd::highlight_shadow(v, vpr, ch, true, out);
    }
    for ch in [R, G, B, M] {
        Fwd::white_black(v, vpr, ch, true, out);
    }
    Fwd::scontrast(v, vpr, out);
    clamp_max_rgb(out);
}

/// The inverse tones of one pixel, in upstream's order, and the clamp.
///
/// Part of `GradingToneRevOpCPU::apply` (GradingToneOpCPU.cpp:1029-1056 @ v2.5.2).
fn rev_pixel(v: &GradingTone, vpr: &GradingTonePreRender, out: &mut [f32; 4]) {
    use RgbmChannel::{B, G, M, R};
    Rev::scontrast(v, vpr, out);
    for ch in [M, R, G, B] {
        Rev::white_black(v, vpr, ch, true, out);
    }
    for ch in [M, R, G, B] {
        Rev::highlight_shadow(v, vpr, ch, true, out);
    }
    for ch in [M, R, G, B] {
        Rev::white_black(v, vpr, ch, false, out);
    }
    for ch in [M, R, G, B] {
        Rev::highlight_shadow(v, vpr, ch, false, out);
    }
    for ch in [M, R, G, B] {
        Rev::mids(v, vpr, ch, out);
    }
    clamp_max_rgb(out);
}

// ---------------------------------------------------------------------------------------------
// The renderers.
// ---------------------------------------------------------------------------------------------

/// What every renderer holds: the dynamic property (the op data's, or for a dynamic one a copy
/// that the CPU processor exposes). Upstream's `m_style` is never read: the renderer's class
/// is the style's.
///
/// Port of `GradingToneOpCPU` (GradingToneOpCPU.cpp:20-77 @ v2.5.2).
#[derive(Debug)]
struct GradingToneOpCpu {
    /// `m_gt`.
    gt: DynamicPropertyGradingToneImplRcPtr,
}

impl GradingToneOpCpu {
    /// Port of `GradingToneOpCPU::GradingToneOpCPU` (GradingToneOpCPU.cpp:37-46 @ v2.5.2).
    fn new(gt: &GradingToneOpData) -> Self {
        let mut prop = gt.get_dynamic_property_internal();
        if prop.is_dynamic() {
            prop = prop.create_editable_copy();
        }
        GradingToneOpCpu { gt: prop }
    }

    /// The values and their pre-render form, read once per `apply`, or `None` when the values
    /// change nothing (`getLocalBypass`).
    fn state(&self) -> Option<(GradingTone, GradingTonePreRender)> {
        let state = *self.gt.state();
        (!state.pre_render.local_bypass).then_some((state.value, state.pre_render))
    }

    /// Port of `GradingToneOpCPU::isDynamic` (GradingToneOpCPU.cpp:48-51 @ v2.5.2).
    fn is_dynamic(&self) -> bool {
        self.gt.is_dynamic()
    }

    /// Port of `GradingToneOpCPU::hasDynamicProperty` (GradingToneOpCPU.cpp:53-61 @ v2.5.2).
    fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        let mut res = false;
        if type_ == DynamicPropertyType::GradingTone {
            res = self.gt.is_dynamic();
        }
        res
    }

    /// Port of `GradingToneOpCPU::getDynamicProperty` (GradingToneOpCPU.cpp:63-77 @ v2.5.2).
    fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        if type_ == DynamicPropertyType::GradingTone {
            if self.gt.is_dynamic() {
                return Ok(DynamicPropertyRcPtr::GradingTone(Arc::clone(&self.gt)));
            } else {
                return Err(Exception::new(NOT_DYNAMIC));
            }
        }
        Err(Exception::new(TYPE_NOT_SUPPORTED))
    }
}

/// The `OpCPU` overrides every renderer takes from `GradingToneOpCPU`.
macro_rules! base_dynamic_properties {
    () => {
        fn is_dynamic(&self) -> bool {
            self.base.is_dynamic()
        }

        fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
            self.base.has_dynamic_property(type_)
        }

        fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
            self.base.get_dynamic_property(type_)
        }
    };
}

/// The pixels of an RGBA buffer.
fn pixels(rgba: &mut [f32]) -> &mut [[f32; 4]] {
    rgba.as_chunks_mut::<4>().0
}

/// The forward renderer of the log and video styles.
///
/// Port of `GradingToneFwdOpCPU` (GradingToneOpCPU.cpp:79-92, 127-130 @ v2.5.2).
#[derive(Debug)]
struct GradingToneFwdOpCpu {
    base: GradingToneOpCpu,
}

impl CpuOp for GradingToneFwdOpCpu {
    /// Port of `GradingToneFwdOpCPU::apply` (GradingToneOpCPU.cpp:950-1004 @ v2.5.2). Each
    /// pixel is graded in place (upstream copies it to the output first); alpha is never
    /// written.
    fn apply(&self, rgba: &mut [f32]) {
        let Some((v, vpr)) = self.base.state() else {
            return;
        };
        for px in pixels(rgba) {
            fwd_pixel(&v, &vpr, px);
        }
    }

    base_dynamic_properties!();
}

/// The inverse renderer of the log and video styles.
///
/// Port of `GradingToneRevOpCPU` (GradingToneOpCPU.cpp:102-115, 137-140 @ v2.5.2).
#[derive(Debug)]
struct GradingToneRevOpCpu {
    base: GradingToneOpCpu,
}

impl CpuOp for GradingToneRevOpCpu {
    /// Port of `GradingToneRevOpCPU::apply` (GradingToneOpCPU.cpp:1006-1061 @ v2.5.2).
    fn apply(&self, rgba: &mut [f32]) {
        let Some((v, vpr)) = self.base.state() else {
            return;
        };
        for px in pixels(rgba) {
            rev_pixel(&v, &vpr, px);
        }
    }

    base_dynamic_properties!();
}

/// The renderer of the op data's style and direction.
///
/// Port of `GetGradingToneCPURenderer` (src/OpenColorIO/ops/gradingtone/GradingToneOpCPU.cpp:
/// 1265-1285 @ v2.5.2). The linear style's renderers come with WP 5.3c.
pub fn get_grading_tone_cpu_renderer(tone: &GradingToneOpData) -> Result<Arc<dyn CpuOp>> {
    if tone.get_style() == GradingStyle::Lin {
        return Err(Exception::new(
            "The GradingTone linear renderers are not ported yet (WP 5.3c).",
        ));
    }
    let base = GradingToneOpCpu::new(tone);
    Ok(match tone.get_direction() {
        TransformDirection::Forward => Arc::new(GradingToneFwdOpCpu { base }),
        TransformDirection::Inverse => Arc::new(GradingToneRevOpCpu { base }),
    })
}

#[cfg(test)]
#[path = "grading_tone_op_cpu_tests.rs"]
mod tests;
