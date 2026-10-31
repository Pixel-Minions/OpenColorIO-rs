// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary op's CPU renderers: a port of
//! `src/OpenColorIO/ops/gradingprimary/GradingPrimaryOpCPU.h` and `GradingPrimaryOpCPU.cpp` @
//! v2.5.2.
//!
//! [`get_grading_primary_cpu_renderer`] picks one renderer per style and direction. Each holds
//! the op data's dynamic property (for a dynamic one, a copy the CPU processor exposes) and
//! reads its values and their pre-render form once per `apply` (U-81: a set from another
//! thread gives the whole old or the whole new value).
//!
//! # The `OCIO_USE_SSE2` branches
//!
//! Every x86-64 wheel compiles the renderers' `OCIO_USE_SSE2` branches, whatever the CPU and
//! the optimization flags. They compute the four lanes of a pixel, alpha included, then write
//! the input's alpha back over the result (`out[3] = outAlpha`). The port computes the same
//! four lanes (alpha's coefficients are the identity: a brightness or an offset of 0, a
//! contrast, a gamma, an exposure or a slope of 1) and never writes alpha. Alpha's lane still
//! matters: the saturation's luma sums all four lanes, alpha's weighted by 0, so an infinite
//! or NaN alpha makes every color channel NaN, as in the wheel (I-226).
//!
//! `ssePower` is OCIO's approximation ([`sse_power`]), lane by lane. The other operations are
//! one x86 instruction per lane each, written with [`sse_add`], [`sse_sub`], [`sse_mul`],
//! [`sse_min`] and [`sse_max`]: a NaN pixel can meet a NaN coefficient, which infinite
//! parameters make (a brightness of +Inf and -Inf, white and black pivots both +Inf), and the
//! saturation sums the lanes of one pixel, any of which can be NaN.
//!
//! # Operand orders
//!
//! Where two NaNs can meet, the result is the first operand's NaN. The operand orders are the
//! wheels' machine code (`tools/wheel-inspect`, `docs/wheel-inspect.md`), read loop by loop:
//! each `apply` has four loops (with and without saturation, with and without its power), and
//! the compilers ordered some additions and multiplications differently from the source and
//! from each other ([`LoopOrders`]). The Windows wheel (MSVC) and the Linux wheel (GCC) differ
//! in the forward styles' first addition (I-228) and in the saturation's sums; every other
//! operation is in the source's order on both.
//!
//! Not all of these orders can show in an output. The clamp (`ApplyClamp`) turns every NaN
//! into a clamp value (I-227): last in the forward styles, first in the reverse ones, whose
//! operations then meet only the NaNs that infinities make. What can show is a NaN's sign:
//! `ApplyLinContrast` and `ApplyGamma` keep the sign of their input while `ssePower` turns a
//! NaN into 0, so a NaN that an addition gives before them leaves a signed zero; and the
//! reverse styles' last addition, where a pixel's default NaN can meet a coefficient whose
//! pre-render form negated one (`GradingPrimaryPreRender::update`'s inverse offsets, and its
//! inverse log brightness on Linux). The battery (`grading_primary_oracle`) checks, per style:
//! - forward, the first addition: log's brightness, linear's and video's offset, with an
//!   infinite sum (+Inf and -Inf) and a black pivot of 0.0 and -0.0, on both wheels;
//! - reverse, the last addition: linear's and video's inverse offset on both wheels, log's
//!   inverse brightness on Linux only (on Windows its NaN has the pixel's bits, so no output
//!   shows that order);
//! - the contrast's multiplication, `(pix - pivot) * contrast`, with contrasts of 0 and +Inf
//!   (a mutant of its order is killed); its final addition, `+ pivot`, shows in no output
//!   (the mutant of its order survives);
//!
//! The saturation's sums meet only NaNs with the same bits, or none at all: no output shows
//! their orders.
//!
//! # SSE2 fast paths
//!
//! That lane code branches on every operand. Blocks of finite pixels with finite coefficients
//! take the real SSE2 kernels of `grading_primary_op_cpu_sse2` instead, where no NaN can show
//! an order (its `Coefficients::fast_path` says why); any other block takes the lanes.

use std::sync::Arc;

#[cfg(target_arch = "x86_64")]
use super::grading_primary_op_cpu_sse2 as sse2;
use super::grading_primary_op_data::GradingPrimaryOpData;
use crate::dynamic_property::{
    DynamicPropertyGradingPrimaryImplRcPtr, DynamicPropertyRcPtr, PrimaryState,
};
use crate::exception::{Exception, Result};
use crate::math_utils::{sse_add, sse_max, sse_min, sse_mul, sse_sub};
use crate::op::CpuOp;
use crate::open_color_types::{DynamicPropertyType, GradingStyle, TransformDirection};
use crate::sse::{EABS_MASK, ESIGN_MASK, sse_power};
#[cfg(target_arch = "x86_64")]
use crate::sse2::blocks::for_each_block;

/// "Dynamic property type not supported by GradingPrimary." (GradingPrimaryOpCPU.cpp:72 @
/// v2.5.2).
const TYPE_NOT_SUPPORTED: &str = "Dynamic property type not supported by GradingPrimary.";

/// "GradingPrimary property is not dynamic." (GradingPrimaryOpCPU.cpp:75 @ v2.5.2).
const NOT_DYNAMIC: &str = "GradingPrimary property is not dynamic.";

// ---------------------------------------------------------------------------------------------
// Four lanes, as an `__m128`.
// ---------------------------------------------------------------------------------------------

/// The four lanes of an `__m128`: red, green, blue and alpha.
type V4 = [f32; 4];

/// Which of a source operation's two operands the wheel's machine code puts first, where two
/// NaNs can meet: the result is the first one's NaN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum First {
    /// The source's first operand: `_mm_add_ps(a, b)` computed as `a + b`.
    Src,
    /// The source's second operand: `_mm_add_ps(a, b)` computed as `b + a`.
    Swap,
}

use First::{Src, Swap};

/// `_mm_set1_ps(x)`.
fn splat(x: f32) -> V4 {
    [x; 4]
}

/// `_mm_add_ps(a, b)` in the order `first` says.
fn add(first: First, a: V4, b: V4) -> V4 {
    std::array::from_fn(|i| match first {
        Src => sse_add(a[i], b[i]),
        Swap => sse_add(b[i], a[i]),
    })
}

/// `_mm_mul_ps(a, b)` in the order `first` says.
fn mul(first: First, a: V4, b: V4) -> V4 {
    std::array::from_fn(|i| match first {
        Src => sse_mul(a[i], b[i]),
        Swap => sse_mul(b[i], a[i]),
    })
}

/// `_mm_sub_ps(a, b)`.
fn sub(a: V4, b: V4) -> V4 {
    std::array::from_fn(|i| sse_sub(a[i], b[i]))
}

/// `_mm_div_ps(a, b)`: x86 returns the first operand's NaN, as Rust's `/` compiles.
fn div(a: V4, b: V4) -> V4 {
    std::array::from_fn(|i| a[i] / b[i])
}

/// `_mm_and_ps(a, mask)`, with the same mask in every lane.
fn and_bits(a: V4, mask: u32) -> V4 {
    a.map(|x| f32::from_bits(x.to_bits() & mask))
}

/// `_mm_xor_ps(a, b)`.
fn xor(a: V4, b: V4) -> V4 {
    std::array::from_fn(|i| f32::from_bits(a[i].to_bits() ^ b[i].to_bits()))
}

/// `ssePower(x, exp)`, lane by lane ([`sse_power`]).
fn power(x: V4, exp: V4) -> V4 {
    std::array::from_fn(|i| sse_power(x[i], exp[i]))
}

/// `_mm_shuffle_ps(a, a, _MM_SHUFFLE(i3, i2, i1, i0))`: lane `k` of the result is lane `ik`
/// of `a`.
fn shuffle(a: V4, lanes: [usize; 4]) -> V4 {
    lanes.map(|i| a[i])
}

/// The four lanes of the pixel at the start of `px`: `_mm_set_ps(in[3], in[2], in[1], in[0])`.
fn load(px: &[f32; 4]) -> V4 {
    *px
}

/// Writes the color lanes of `p` over the pixel; alpha stays: `_mm_storeu_ps(out, pixel);
/// out[3] = outAlpha;` with `outAlpha = in[3]`.
fn store_rgb(px: &mut [f32; 4], p: V4) {
    px[0] = p[0];
    px[1] = p[1];
    px[2] = p[2];
}

// ---------------------------------------------------------------------------------------------
// The operations of the `OCIO_USE_SSE2` branch.
// ---------------------------------------------------------------------------------------------

/// The operand orders of one loop of an `apply`, where they can matter (two NaNs can meet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoopOrders {
    /// The forward styles' first addition, `_mm_add_ps(pixel, brightness)` or
    /// `_mm_add_ps(pixel, offset)`; the reverse styles' last one,
    /// `_mm_add_ps(pixel, brightnessInv)` or `_mm_add_ps(pixel, offsetInv)`.
    pixel_add: First,
    /// `ApplySaturation`'s `_mm_add_ps(luma, _mm_shuffle_ps(luma, luma, _MM_SHUFFLE(2, 3, 0,
    /// 1)))`.
    luma1: First,
    /// `ApplySaturation`'s `_mm_add_ps(luma, _mm_shuffle_ps(luma, luma, _MM_SHUFFLE(1, 0, 3,
    /// 2)))`.
    luma2: First,
    /// `ApplySaturation`'s `_mm_mul_ps(saturation, _mm_sub_ps(pix, luma))`.
    sat_mul: First,
    /// `ApplySaturation`'s `_mm_add_ps(luma, ...)`.
    sat_add: First,
}

/// One `apply`'s four loops, by whether it applies the saturation and its power (gamma or
/// contrast).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ApplyOrders {
    /// Neither.
    plain: LoopOrders,
    /// The power only.
    power: LoopOrders,
    /// The saturation only.
    sat: LoopOrders,
    /// Both.
    sat_power: LoopOrders,
}

impl ApplyOrders {
    /// The loop's orders.
    fn of(&self, saturation: bool, power: bool) -> &LoopOrders {
        match (saturation, power) {
            (false, false) => &self.plain,
            (false, true) => &self.power,
            (true, false) => &self.sat,
            (true, true) => &self.sat_power,
        }
    }
}

/// A loop without saturation: only the pixel's addition can be ordered.
const fn no_sat(pixel_add: First) -> LoopOrders {
    LoopOrders {
        pixel_add,
        // Unused.
        luma1: Src,
        luma2: Src,
        sat_mul: Src,
        sat_add: Src,
    }
}

/// A loop with saturation.
const fn with_sat(
    pixel_add: First,
    luma1: First,
    luma2: First,
    sat_mul: First,
    sat_add: First,
) -> LoopOrders {
    LoopOrders {
        pixel_add,
        luma1,
        luma2,
        sat_mul,
        sat_add,
    }
}

/// The Windows wheel's (MSVC) loops of every `apply`: the pixel's addition in the source's
/// order; the saturation's sums with the shuffled luma first, and its multiplication and
/// addition with the difference and the product first.
#[cfg(windows)]
const MSVC: ApplyOrders = ApplyOrders {
    plain: no_sat(Src),
    power: no_sat(Src),
    sat: with_sat(Src, Swap, Swap, Swap, Swap),
    sat_power: with_sat(Src, Swap, Swap, Swap, Swap),
};

/// The Windows wheel's forward log loops (`GradingPrimaryLogFwdOpCPU::apply`, 0x1801d57a0).
#[cfg(windows)]
const LOG_FWD: ApplyOrders = MSVC;

/// The Windows wheel's reverse log loops (`GradingPrimaryLogRevOpCPU::apply`, 0x1801d6080).
#[cfg(windows)]
const LOG_REV: ApplyOrders = MSVC;

/// The Linux wheel's forward log loops (`GradingPrimaryLogFwdOpCPU::apply`, 0x3a9cf0): the
/// brightness first (`brightness + pixel`); with saturation, the luma's first sum with the
/// shuffled luma first where there is no gamma, and the product first in the last addition.
#[cfg(not(windows))]
const LOG_FWD: ApplyOrders = ApplyOrders {
    plain: no_sat(Swap),
    power: no_sat(Swap),
    sat: with_sat(Swap, Swap, Src, Swap, Swap),
    sat_power: with_sat(Swap, Src, Src, Swap, Swap),
};

/// The Linux wheel's reverse log loops (`GradingPrimaryLogRevOpCPU::apply`, 0x3a8ca0): the
/// brightness's addition in the source's order; with saturation, the luma's first sum with
/// the shuffled luma first where there is no gamma, and the last addition with the product
/// first there, the luma first with the gamma.
#[cfg(not(windows))]
const LOG_REV: ApplyOrders = ApplyOrders {
    plain: no_sat(Src),
    power: no_sat(Src),
    sat: with_sat(Src, Swap, Src, Swap, Swap),
    sat_power: with_sat(Src, Src, Src, Swap, Src),
};

/// The Windows wheel's forward linear loops (`GradingPrimaryLinFwdOpCPU::apply`, 0x1801d4730).
#[cfg(windows)]
const LIN_FWD: ApplyOrders = MSVC;

/// The Windows wheel's reverse linear loops (`GradingPrimaryLinRevOpCPU::apply`, 0x1801d4f60).
#[cfg(windows)]
const LIN_REV: ApplyOrders = MSVC;

/// The Windows wheel's forward video loops (`GradingPrimaryVidFwdOpCPU::apply`, 0x1801d6970).
#[cfg(windows)]
const VID_FWD: ApplyOrders = MSVC;

/// The Windows wheel's reverse video loops (`GradingPrimaryVidRevOpCPU::apply`, 0x1801d7220).
#[cfg(windows)]
const VID_REV: ApplyOrders = MSVC;

/// The Linux wheel's forward linear loops (`GradingPrimaryLinFwdOpCPU::apply`, 0x3a8500): the
/// offset first, and the saturation as the forward log style's.
#[cfg(not(windows))]
const LIN_FWD: ApplyOrders = LOG_FWD;

/// The Linux wheel's reverse linear loops (`GradingPrimaryLinRevOpCPU::apply`, 0x3a7b50): as
/// the reverse log style's.
#[cfg(not(windows))]
const LIN_REV: ApplyOrders = LOG_REV;

/// The Linux wheel's forward video loops (`GradingPrimaryVidFwdOpCPU::apply`, 0x3a94c0): as the
/// forward log style's.
#[cfg(not(windows))]
const VID_FWD: ApplyOrders = LOG_FWD;

/// The Linux wheel's reverse video loops (`GradingPrimaryVidRevOpCPU::apply`, 0x3aa530): as
/// the reverse log style's.
#[cfg(not(windows))]
const VID_REV: ApplyOrders = LOG_REV;

/// `ApplyContrast(pix, contrast, pivot)`: `(pix - pivot) * contrast + pivot`, in the source's
/// order on both wheels.
///
/// Port of `ApplyContrast` (GradingPrimaryOpCPU.cpp:130-133 @ v2.5.2).
fn apply_contrast(pix: V4, contrast: V4, pivot: V4) -> V4 {
    add(Src, mul(Src, sub(pix, pivot), contrast), pivot)
}

/// `ApplyLinContrast(pix, contrast, pivot)`: `pix / pivot`, raised to the contrast
/// (`ssePower`) with its sign, times the pivot; the multiplication in the source's order on
/// both wheels.
///
/// Port of `ApplyLinContrast` (GradingPrimaryOpCPU.cpp:135-142 @ v2.5.2).
fn apply_lin_contrast(pix: V4, contrast: V4, pivot: V4) -> V4 {
    let pix = div(pix, pivot);
    let sign_pix = and_bits(pix, ESIGN_MASK);
    let abs_pix = and_bits(pix, EABS_MASK);
    let pix = mul(Src, power(abs_pix, contrast), pivot);
    xor(pix, sign_pix)
}

/// `ApplyGamma(pix, gamma, blackPivot, whitePivot)`: the distance to the black pivot, scaled
/// by the pivots' range, raised to the gamma (`ssePower`) with its sign, scaled back; the
/// multiplication and the addition in the source's order on both wheels.
///
/// Port of `ApplyGamma` (GradingPrimaryOpCPU.cpp:144-153 @ v2.5.2).
fn apply_gamma(pix: V4, gamma: V4, black_pivot: V4, white_pivot: V4) -> V4 {
    let pix = sub(pix, black_pivot);
    let sign_pix = and_bits(pix, ESIGN_MASK);
    let abs_pix = and_bits(pix, EABS_MASK);
    let range = sub(white_pivot, black_pivot);
    let pix = div(abs_pix, range);
    let pix = power(pix, gamma);
    add(Src, mul(Src, xor(pix, sign_pix), range), black_pivot)
}

/// The luma weights, with 0 for alpha.
///
/// Port of `LumaWeights` (GradingPrimaryOpCPU.cpp:155 @ v2.5.2).
const LUMA_WEIGHTS: V4 = [0.2126, 0.7152, 0.0722, 0.0];

/// `ApplySaturation(pix, saturation)`: the luma, the dot product of the four lanes and the
/// weights summed with two shuffles, then `luma + saturation * (pix - luma)`, in the loop's
/// orders.
///
/// Port of `ApplySaturation` (GradingPrimaryOpCPU.cpp:157-170 @ v2.5.2).
fn apply_saturation(pix: V4, saturation: V4, o: &LoopOrders) -> V4 {
    // Compute luma: dot product of pixel values and the luma weights. (The weights are never
    // NaN: the order doesn't matter.)
    let luma = mul(Src, pix, LUMA_WEIGHTS);

    // luma = [ x+y , y+x , z+w , w+z ]
    let luma = add(o.luma1, luma, shuffle(luma, [1, 0, 3, 2]));

    // luma = [ x+y+z+w , y+x+w+z , z+w+x+y , w+z+y+x ]
    let luma = add(o.luma2, luma, shuffle(luma, [2, 3, 0, 1]));

    // Apply saturation
    add(o.sat_add, luma, mul(o.sat_mul, saturation, sub(pix, luma)))
}

/// `ApplyClamp(pix, blackClamp, whiteClamp)`: `_mm_min_ps(_mm_max_ps(pix, blackClamp),
/// whiteClamp)`, so a NaN becomes the black clamp (-Inf when it doesn't clamp, from
/// `NoClampBlack`; I-227), then stays.
///
/// Port of `ApplyClamp` (GradingPrimaryOpCPU.cpp:172-175 @ v2.5.2).
fn apply_clamp(pix: V4, black_clamp: V4, white_clamp: V4) -> V4 {
    std::array::from_fn(|i| sse_min(sse_max(pix[i], black_clamp[i]), white_clamp[i]))
}

// ---------------------------------------------------------------------------------------------
// The renderers.
// ---------------------------------------------------------------------------------------------

/// What every renderer holds: the dynamic property of its values.
///
/// Port of `GradingPrimaryOpCPU` (src/OpenColorIO/ops/gradingprimary/GradingPrimaryOpCPU.cpp:
/// 20-76 @ v2.5.2).
#[derive(Debug)]
struct GradingPrimaryOpCpu {
    /// `m_gp`.
    gp: DynamicPropertyGradingPrimaryImplRcPtr,
}

impl GradingPrimaryOpCpu {
    /// The op data's property, decoupled from the op data by a copy when it is dynamic.
    ///
    /// Port of `GradingPrimaryOpCPU::GradingPrimaryOpCPU` (GradingPrimaryOpCPU.cpp:36-44 @
    /// v2.5.2).
    fn new(gp: &GradingPrimaryOpData) -> Self {
        let mut prop = gp.get_dynamic_property_internal();
        if prop.is_dynamic() {
            prop = prop.create_editable_copy();
        }
        GradingPrimaryOpCpu { gp: prop }
    }

    /// The values and their pre-render form, read once for an `apply`, or `None` when the
    /// values change nothing (`getLocalBypass`): upstream's `apply` then copies its input, and
    /// in place leaves it as it is.
    fn state(&self) -> Option<PrimaryState> {
        let state = *self.gp.state();
        (!state.pre_render.get_local_bypass()).then_some(state)
    }

    /// Port of `GradingPrimaryOpCPU::isDynamic` (GradingPrimaryOpCPU.cpp:46-49 @ v2.5.2).
    fn is_dynamic(&self) -> bool {
        self.gp.is_dynamic()
    }

    /// Port of `GradingPrimaryOpCPU::hasDynamicProperty` (GradingPrimaryOpCPU.cpp:51-59 @
    /// v2.5.2).
    fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        let mut res = false;
        if type_ == DynamicPropertyType::GradingPrimary {
            res = self.gp.is_dynamic();
        }
        res
    }

    /// Port of `GradingPrimaryOpCPU::getDynamicProperty` (GradingPrimaryOpCPU.cpp:61-76 @
    /// v2.5.2).
    fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        if type_ == DynamicPropertyType::GradingPrimary {
            if self.gp.is_dynamic() {
                return Ok(DynamicPropertyRcPtr::GradingPrimary(Arc::clone(&self.gp)));
            }
        } else {
            return Err(Exception::new(TYPE_NOT_SUPPORTED));
        }

        Err(Exception::new(NOT_DYNAMIC))
    }
}

/// Implements [`CpuOp`]'s dynamic property methods with the renderer's base.
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

/// The pixels of `rgba`, four values each.
fn pixels(rgba: &mut [f32]) -> &mut [[f32; 4]] {
    rgba.as_chunks_mut::<4>().0
}

/// Renders `rgba` with `exact`, the lanes, or block by block with `fast`, where the
/// coefficients `c` allow it.
#[cfg(target_arch = "x86_64")]
#[inline]
fn render(
    rgba: &mut [f32],
    c: &sse2::Coefficients,
    fast: impl Fn(&mut [[f32; 4]]),
    exact: impl Fn(&mut [[f32; 4]]),
) {
    if c.fast_path() {
        for_each_block(pixels(rgba), fast, |block, _| exact(block));
    } else {
        exact(pixels(rgba));
    }
}

/// The forward log style: brightness, contrast around the pivot, gamma between the black and
/// white pivots, saturation, clamps.
///
/// Port of `GradingPrimaryLogFwdOpCPU` (src/OpenColorIO/ops/gradingprimary/
/// GradingPrimaryOpCPU.cpp:78-84, 254-431 @ v2.5.2).
#[derive(Debug)]
struct GradingPrimaryLogFwdOpCpu {
    base: GradingPrimaryOpCpu,
}

impl CpuOp for GradingPrimaryLogFwdOpCpu {
    /// Port of `GradingPrimaryLogFwdOpCPU::apply` (GradingPrimaryOpCPU.cpp:259-431 @ v2.5.2),
    /// its `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let Some(state) = self.base.state() else {
            return;
        };
        let v = &state.value;
        let comp = &state.pre_render;

        let is_gamma_identity = comp.is_gamma_identity();

        let [b0, b1, b2] = *comp.get_brightness();
        let brightness = [b0, b1, b2, 0.0];
        let [c0, c1, c2] = *comp.get_contrast();
        let contrast = [c0, c1, c2, 1.0];
        let [g0, g1, g2] = *comp.get_gamma();
        let gamma = [g0, g1, g2, 1.0];
        let pivot = splat(comp.get_pivot() as f32);
        let saturation = splat(v.saturation as f32);
        let black_pivot = splat(v.pivot_black as f32);
        let white_pivot = splat(v.pivot_white as f32);
        let black_clamp = splat(v.clamp_black as f32);
        let white_clamp = splat(v.clamp_white as f32);

        let apply_sat = v.saturation != 1.0;
        let o = LOG_FWD.of(apply_sat, !is_gamma_identity);
        let exact = |block: &mut [[f32; 4]]| {
            for px in block {
                let mut pixel = load(px);

                pixel = add(o.pixel_add, pixel, brightness);
                pixel = apply_contrast(pixel, contrast, pivot);
                if !is_gamma_identity {
                    pixel = apply_gamma(pixel, gamma, black_pivot, white_pivot);
                }
                if apply_sat {
                    pixel = apply_saturation(pixel, saturation, o);
                }
                pixel = apply_clamp(pixel, black_clamp, white_clamp);

                store_rgb(px, pixel);
            }
        };

        #[cfg(target_arch = "x86_64")]
        {
            let c = sse2::Coefficients {
                add: brightness,
                scale: contrast,
                power: gamma,
                pivot: pivot[0],
                saturation: saturation[0],
                black_pivot: black_pivot[0],
                white_pivot: white_pivot[0],
                black_clamp: black_clamp[0],
                lin_contrast: false,
                white_clamp: white_clamp[0],
            };
            let fast = |block: &mut [[f32; 4]]| {
                sse2::render_log_fwd(block, &c, apply_sat, !is_gamma_identity)
            };
            render(rgba, &c, fast, exact);
        }
        #[cfg(not(target_arch = "x86_64"))]
        exact(pixels(rgba));
    }

    base_dynamic_properties!();
}

/// The reverse log style: the forward steps undone in the reverse order, with the inverted
/// values of the pre-render form; no saturation for a saturation of 0.
///
/// Port of `GradingPrimaryLogRevOpCPU` (src/OpenColorIO/ops/gradingprimary/
/// GradingPrimaryOpCPU.cpp:86-92, 433-605 @ v2.5.2).
#[derive(Debug)]
struct GradingPrimaryLogRevOpCpu {
    base: GradingPrimaryOpCpu,
}

impl CpuOp for GradingPrimaryLogRevOpCpu {
    /// Port of `GradingPrimaryLogRevOpCPU::apply` (GradingPrimaryOpCPU.cpp:438-605 @ v2.5.2),
    /// its `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let Some(state) = self.base.state() else {
            return;
        };
        let v = &state.value;
        let comp = &state.pre_render;

        let is_gamma_identity = comp.is_gamma_identity();

        let [b0, b1, b2] = *comp.get_brightness();
        let brightness_inv = [b0, b1, b2, 0.0];
        let [c0, c1, c2] = *comp.get_contrast();
        let contrast_inv = [c0, c1, c2, 1.0];
        let [g0, g1, g2] = *comp.get_gamma();
        let gamma_inv = [g0, g1, g2, 1.0];

        let pivot_black = splat(v.pivot_black as f32);
        let pivot_white = splat(v.pivot_white as f32);
        let clamp_b = splat(v.clamp_black as f32);
        let clamp_w = splat(v.clamp_white as f32);

        let actual_pivot = splat(comp.get_pivot() as f32);

        let apply_sat = v.saturation != 1. && v.saturation != 0.;
        let sat_inv = splat((1. / v.saturation) as f32);
        let o = LOG_REV.of(apply_sat, !is_gamma_identity);
        let exact = |block: &mut [[f32; 4]]| {
            for px in block {
                let mut pixel = load(px);

                pixel = apply_clamp(pixel, clamp_b, clamp_w);
                if apply_sat {
                    pixel = apply_saturation(pixel, sat_inv, o);
                }
                if !is_gamma_identity {
                    pixel = apply_gamma(pixel, gamma_inv, pivot_black, pivot_white);
                }
                pixel = apply_contrast(pixel, contrast_inv, actual_pivot);
                pixel = add(o.pixel_add, pixel, brightness_inv);

                store_rgb(px, pixel);
            }
        };

        #[cfg(target_arch = "x86_64")]
        {
            let c = sse2::Coefficients {
                add: brightness_inv,
                scale: contrast_inv,
                power: gamma_inv,
                pivot: actual_pivot[0],
                saturation: sat_inv[0],
                black_pivot: pivot_black[0],
                white_pivot: pivot_white[0],
                black_clamp: clamp_b[0],
                lin_contrast: false,
                white_clamp: clamp_w[0],
            };
            let fast = |block: &mut [[f32; 4]]| {
                sse2::render_log_rev(block, &c, apply_sat, !is_gamma_identity)
            };
            render(rgba, &c, fast, exact);
        }
        #[cfg(not(target_arch = "x86_64"))]
        exact(pixels(rgba));
    }

    base_dynamic_properties!();
}

/// The forward linear style: offset, exposure, contrast around the pivot, saturation, clamps.
///
/// Port of `GradingPrimaryLinFwdOpCPU` (src/OpenColorIO/ops/gradingprimary/
/// GradingPrimaryOpCPU.cpp:94-100, 607-779 @ v2.5.2).
#[derive(Debug)]
struct GradingPrimaryLinFwdOpCpu {
    base: GradingPrimaryOpCpu,
}

impl CpuOp for GradingPrimaryLinFwdOpCpu {
    /// Port of `GradingPrimaryLinFwdOpCPU::apply` (GradingPrimaryOpCPU.cpp:612-779 @ v2.5.2),
    /// its `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let Some(state) = self.base.state() else {
            return;
        };
        let v = &state.value;
        let comp = &state.pre_render;

        let is_contrast_identity = comp.is_contrast_identity();

        let [o0, o1, o2] = *comp.get_offset();
        let offset = [o0, o1, o2, 0.0];
        let [e0, e1, e2] = *comp.get_exposure();
        let exposure = [e0, e1, e2, 1.0];
        let [c0, c1, c2] = *comp.get_contrast();
        let contrast = [c0, c1, c2, 1.0];
        let pivot = splat(comp.get_pivot() as f32);
        let saturation = splat(v.saturation as f32);
        let clamp_b = splat(v.clamp_black as f32);
        let clamp_w = splat(v.clamp_white as f32);

        let apply_sat = v.saturation != 1.0;
        let o = LIN_FWD.of(apply_sat, !is_contrast_identity);
        let exact = |block: &mut [[f32; 4]]| {
            for px in block {
                let mut pixel = load(px);

                pixel = add(o.pixel_add, pixel, offset);
                pixel = mul(Src, pixel, exposure);
                if !is_contrast_identity {
                    pixel = apply_lin_contrast(pixel, contrast, pivot);
                }
                if apply_sat {
                    pixel = apply_saturation(pixel, saturation, o);
                }
                pixel = apply_clamp(pixel, clamp_b, clamp_w);

                store_rgb(px, pixel);
            }
        };

        #[cfg(target_arch = "x86_64")]
        {
            let c = sse2::Coefficients {
                add: offset,
                scale: exposure,
                power: contrast,
                pivot: pivot[0],
                saturation: saturation[0],
                black_pivot: 0.0,
                white_pivot: 0.0,
                black_clamp: clamp_b[0],
                lin_contrast: true,
                white_clamp: clamp_w[0],
            };
            let fast = |block: &mut [[f32; 4]]| {
                sse2::render_lin_fwd(block, &c, apply_sat, !is_contrast_identity)
            };
            render(rgba, &c, fast, exact);
        }
        #[cfg(not(target_arch = "x86_64"))]
        exact(pixels(rgba));
    }

    base_dynamic_properties!();
}

/// The reverse linear style: the forward steps undone in the reverse order. Its loops
/// without saturation both apply the contrast, whether it is the identity or not.
///
/// Port of `GradingPrimaryLinRevOpCPU` (src/OpenColorIO/ops/gradingprimary/
/// GradingPrimaryOpCPU.cpp:102-108, 781-947 @ v2.5.2).
#[derive(Debug)]
struct GradingPrimaryLinRevOpCpu {
    base: GradingPrimaryOpCpu,
}

impl CpuOp for GradingPrimaryLinRevOpCpu {
    /// Port of `GradingPrimaryLinRevOpCPU::apply` (GradingPrimaryOpCPU.cpp:786-947 @ v2.5.2),
    /// its `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let Some(state) = self.base.state() else {
            return;
        };
        let v = &state.value;
        let comp = &state.pre_render;

        let is_contrast_identity = comp.is_contrast_identity();

        let [o0, o1, o2] = *comp.get_offset();
        let offset_inv = [o0, o1, o2, 0.0];
        let [e0, e1, e2] = *comp.get_exposure();
        let exposure_inv = [e0, e1, e2, 1.0];
        let [c0, c1, c2] = *comp.get_contrast();
        let contrast_inv = [c0, c1, c2, 1.0];
        let pivot = splat(comp.get_pivot() as f32);
        let clamp_b = splat(v.clamp_black as f32);
        let clamp_w = splat(v.clamp_white as f32);

        let apply_sat = v.saturation != 1. && v.saturation != 0.;
        let sat_inv = splat((1. / v.saturation) as f32);
        // Without saturation, both of upstream's loops apply the contrast.
        let apply_contrast = !apply_sat || !is_contrast_identity;
        let o = LIN_REV.of(apply_sat, apply_contrast);
        let exact = |block: &mut [[f32; 4]]| {
            for px in block {
                let mut pixel = load(px);

                pixel = apply_clamp(pixel, clamp_b, clamp_w);
                if apply_sat {
                    pixel = apply_saturation(pixel, sat_inv, o);
                }
                if apply_contrast {
                    pixel = apply_lin_contrast(pixel, contrast_inv, pivot);
                }
                pixel = mul(Src, pixel, exposure_inv);
                pixel = add(o.pixel_add, pixel, offset_inv);

                store_rgb(px, pixel);
            }
        };

        #[cfg(target_arch = "x86_64")]
        {
            let c = sse2::Coefficients {
                add: offset_inv,
                scale: exposure_inv,
                power: contrast_inv,
                pivot: pivot[0],
                saturation: sat_inv[0],
                black_pivot: 0.0,
                white_pivot: 0.0,
                black_clamp: clamp_b[0],
                lin_contrast: true,
                white_clamp: clamp_w[0],
            };
            let fast =
                |block: &mut [[f32; 4]]| sse2::render_lin_rev(block, &c, apply_sat, apply_contrast);
            render(rgba, &c, fast, exact);
        }
        #[cfg(not(target_arch = "x86_64"))]
        exact(pixels(rgba));
    }

    base_dynamic_properties!();
}

/// The forward video style: offset (with the lift), slope around the black pivot, gamma
/// between the pivots, saturation, clamps.
///
/// Port of `GradingPrimaryVidFwdOpCPU` (src/OpenColorIO/ops/gradingprimary/
/// GradingPrimaryOpCPU.cpp:110-116, 949-1127 @ v2.5.2).
#[derive(Debug)]
struct GradingPrimaryVidFwdOpCpu {
    base: GradingPrimaryOpCpu,
}

impl CpuOp for GradingPrimaryVidFwdOpCpu {
    /// Port of `GradingPrimaryVidFwdOpCPU::apply` (GradingPrimaryOpCPU.cpp:954-1127 @ v2.5.2),
    /// its `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let Some(state) = self.base.state() else {
            return;
        };
        let v = &state.value;
        let comp = &state.pre_render;

        let is_gamma_identity = comp.is_gamma_identity();

        let [o0, o1, o2] = *comp.get_offset();
        let offset = [o0, o1, o2, 0.0];
        let [s0, s1, s2] = *comp.get_slope();
        let slope = [s0, s1, s2, 1.0];
        let [g0, g1, g2] = *comp.get_gamma();
        let gamma = [g0, g1, g2, 1.0];

        let saturation = splat(v.saturation as f32);
        let pivot_black = splat(v.pivot_black as f32);
        let pivot_white = splat(v.pivot_white as f32);
        let clamp_b = splat(v.clamp_black as f32);
        let clamp_w = splat(v.clamp_white as f32);

        let apply_sat = v.saturation != 1.0;
        let o = VID_FWD.of(apply_sat, !is_gamma_identity);
        let exact = |block: &mut [[f32; 4]]| {
            for px in block {
                let mut pixel = load(px);

                pixel = add(o.pixel_add, pixel, offset);
                pixel = apply_contrast(pixel, slope, pivot_black);
                if !is_gamma_identity {
                    pixel = apply_gamma(pixel, gamma, pivot_black, pivot_white);
                }
                if apply_sat {
                    pixel = apply_saturation(pixel, saturation, o);
                }
                pixel = apply_clamp(pixel, clamp_b, clamp_w);

                store_rgb(px, pixel);
            }
        };

        #[cfg(target_arch = "x86_64")]
        {
            let c = sse2::Coefficients {
                add: offset,
                scale: slope,
                power: gamma,
                pivot: pivot_black[0],
                saturation: saturation[0],
                black_pivot: pivot_black[0],
                white_pivot: pivot_white[0],
                black_clamp: clamp_b[0],
                lin_contrast: false,
                white_clamp: clamp_w[0],
            };
            let fast = |block: &mut [[f32; 4]]| {
                sse2::render_log_fwd(block, &c, apply_sat, !is_gamma_identity)
            };
            render(rgba, &c, fast, exact);
        }
        #[cfg(not(target_arch = "x86_64"))]
        exact(pixels(rgba));
    }

    base_dynamic_properties!();
}

/// The reverse video style: the forward steps undone in the reverse order.
///
/// Port of `GradingPrimaryVidRevOpCPU` (src/OpenColorIO/ops/gradingprimary/
/// GradingPrimaryOpCPU.cpp:118-124, 1129-1297 @ v2.5.2).
#[derive(Debug)]
struct GradingPrimaryVidRevOpCpu {
    base: GradingPrimaryOpCpu,
}

impl CpuOp for GradingPrimaryVidRevOpCpu {
    /// Port of `GradingPrimaryVidRevOpCPU::apply` (GradingPrimaryOpCPU.cpp:1134-1297 @
    /// v2.5.2), its `OCIO_USE_SSE2` branch.
    fn apply(&self, rgba: &mut [f32]) {
        let Some(state) = self.base.state() else {
            return;
        };
        let v = &state.value;
        let comp = &state.pre_render;

        let is_gamma_identity = comp.is_gamma_identity();

        let [o0, o1, o2] = *comp.get_offset();
        let offset_inv = [o0, o1, o2, 0.0];
        let [s0, s1, s2] = *comp.get_slope();
        let slope_inv = [s0, s1, s2, 1.0];
        let [g0, g1, g2] = *comp.get_gamma();
        let gamma_inv = [g0, g1, g2, 1.0];

        let pivot_black = splat(v.pivot_black as f32);
        let pivot_white = splat(v.pivot_white as f32);
        let clamp_b = splat(v.clamp_black as f32);
        let clamp_w = splat(v.clamp_white as f32);

        let apply_sat = v.saturation != 1. && v.saturation != 0.;
        let sat_inv = splat((1. / v.saturation) as f32);
        let o = VID_REV.of(apply_sat, !is_gamma_identity);
        let exact = |block: &mut [[f32; 4]]| {
            for px in block {
                let mut pixel = load(px);

                pixel = apply_clamp(pixel, clamp_b, clamp_w);
                if apply_sat {
                    pixel = apply_saturation(pixel, sat_inv, o);
                }
                if !is_gamma_identity {
                    pixel = apply_gamma(pixel, gamma_inv, pivot_black, pivot_white);
                }
                pixel = apply_contrast(pixel, slope_inv, pivot_black);
                pixel = add(o.pixel_add, pixel, offset_inv);

                store_rgb(px, pixel);
            }
        };

        #[cfg(target_arch = "x86_64")]
        {
            let c = sse2::Coefficients {
                add: offset_inv,
                scale: slope_inv,
                power: gamma_inv,
                pivot: pivot_black[0],
                saturation: sat_inv[0],
                black_pivot: pivot_black[0],
                white_pivot: pivot_white[0],
                black_clamp: clamp_b[0],
                lin_contrast: false,
                white_clamp: clamp_w[0],
            };
            let fast = |block: &mut [[f32; 4]]| {
                sse2::render_log_rev(block, &c, apply_sat, !is_gamma_identity)
            };
            render(rgba, &c, fast, exact);
        }
        #[cfg(not(target_arch = "x86_64"))]
        exact(pixels(rgba));
    }

    base_dynamic_properties!();
}

/// The renderer of `prim`'s style and direction.
///
/// Port of `GetGradingPrimaryCPURenderer` (src/OpenColorIO/ops/gradingprimary/
/// GradingPrimaryOpCPU.cpp:1302-1336 @ v2.5.2). Upstream's "Illegal GradingPrimary
/// direction." can't happen: a direction is forward or inverse.
pub fn get_grading_primary_cpu_renderer(prim: &GradingPrimaryOpData) -> Result<Arc<dyn CpuOp>> {
    let base = GradingPrimaryOpCpu::new(prim);
    Ok(match (prim.get_direction(), prim.get_style()) {
        (TransformDirection::Forward, GradingStyle::Log) => {
            Arc::new(GradingPrimaryLogFwdOpCpu { base })
        }
        (TransformDirection::Forward, GradingStyle::Lin) => {
            Arc::new(GradingPrimaryLinFwdOpCpu { base })
        }
        (TransformDirection::Forward, GradingStyle::Video) => {
            Arc::new(GradingPrimaryVidFwdOpCpu { base })
        }
        (TransformDirection::Inverse, GradingStyle::Log) => {
            Arc::new(GradingPrimaryLogRevOpCpu { base })
        }
        (TransformDirection::Inverse, GradingStyle::Lin) => {
            Arc::new(GradingPrimaryLinRevOpCpu { base })
        }
        (TransformDirection::Inverse, GradingStyle::Video) => {
            Arc::new(GradingPrimaryVidRevOpCpu { base })
        }
    })
}

#[cfg(test)]
#[path = "grading_primary_op_cpu_tests.rs"]
mod tests;
