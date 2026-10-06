// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 1D LUT's CPU renderers: a port of `src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp` @ v2.5.2.
//!
//! [`get_lut1d_renderer`] picks the renderer of a LUT from an input bit depth to an output bit
//! depth, as upstream's `GetLut1DRenderer` does:
//! - **Lookups.** For integer and half input, a LUT with one entry per code is looked up, not
//!   interpolated ([`Lut1DLookupRenderer`]: the `inBD != BIT_DEPTH_F32` branches of
//!   `Lut1DRenderer` and `Lut1DRendererHalfCode`). The CPU engine renders the first op of a
//!   processor that is a 1D LUT from the processor's input bit depth to F32 this way
//!   (`CreateCPUEngine`, src/OpenColorIO/CPUProcessor.cpp:140-146).
//! - **Float input** ([`Lut1DFloatRenderer`]): a standard domain interpolates between the two
//!   nearest entries (`Lut1DRenderer`); a half domain between the entries of the two halfs
//!   nearest the input (`Lut1DRendererHalfCode`, [`get_edge_float_values`]). Both write any
//!   output bit depth, which the CPU engine asks of the last op of a processor.
//! - **Hue adjust** (`HUE_DW3`: [`Lut1DHueAdjustLookupRenderer`],
//!   [`Lut1DHueAdjustFloatRenderer`]): the same lookups and interpolations, then the middle
//!   channel is set from the input's hue ([`order3`], `adjust_hue`).
//!
//! - **Inverse** ([`InvLut1DRenderer`]): for any input bit depth, each value is inverted
//!   through the forward LUT's interpolation, by a search of its values ([`find_lut_inv`],
//!   [`find_lut_inv_half`]), with or without hue adjust.
//! - **SIMD kernels.** Every x86-64 CPU has SSE2, so upstream's `Lut1DRenderer<BIT_DEPTH_F32,
//!   outBD>` renders every row of more than one pixel with the kernel the CPU dispatches to
//!   ([`lut1d_kernel`]): `Lut1DOpCPU_SSE2.cpp`, `_AVX.cpp`, `_AVX2.cpp` and `_AVX512.cpp`
//!   (`lut1d_op_cpu_sse2`, ...). A row of one pixel takes the scalar loop
//!   ([`get_lut1d_scalar_renderer`]); [`get_lut1d_profile_renderer`] gives any profile.
//!
//! Not here yet, and an error until then: the lookups of a LUT that must first be resampled
//! for the input bit depth (`Compose`, WP 2.1g).

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use super::lut1d_op::NOT_PORTED_COMPOSE;
use super::lut1d_op_cpu_avx::apply_lut_avx;
use super::lut1d_op_cpu_avx2::apply_lut_avx2;
use super::lut1d_op_cpu_avx512::apply_lut_avx512;
use super::lut1d_op_cpu_sse2::apply_lut_sse2;
use super::lut1d_op_data::ComponentProperties;
use super::lut1d_op_data::Lut1DOpData;
use super::{lut1d_op_cpu_avx, lut1d_op_cpu_avx2, lut1d_op_cpu_avx512, lut1d_op_cpu_sse2};
use crate::bit_depth_utils::{
    BitDepthInfo, ChannelType, Converter, F16, F32, Uint8, Uint10, Uint12, Uint16,
};
use crate::cpu_info::CpuInfo;
use crate::exception::{Exception, Result};
use crate::imath_half::{float_to_half, half_to_float};
use crate::math_utils::{
    clamp, lerpf, sanitize_float, sse_add, sse_cvttps_epi32, sse_max, sse_min, sse_mul, std_max,
    std_min,
};
use crate::op::{CpuOp, Pixels, PixelsMut};
use crate::open_color_types::{BitDepth, Lut1DHueAdjust, TransformDirection};
use crate::sse2::PackDepth;

/// A channel value as a LUT index.
///
/// Port of `GetLookupValue` (Lut1DOpCPU.cpp:35-54 @ v2.5.2): an integer code is its own index,
/// a half its bits.
pub(crate) trait LookupIndex: ChannelType {
    fn lookup_index(self) -> usize;
}

impl LookupIndex for u8 {
    fn lookup_index(self) -> usize {
        self as usize
    }
}

impl LookupIndex for u16 {
    fn lookup_index(self) -> usize {
        self as usize
    }
}

impl LookupIndex for half::f16 {
    fn lookup_index(self) -> usize {
        self.to_bits() as usize
    }
}

/// C++'s conversion of a `float` to a channel type, `(T)value` or `OutType(value)`, as both
/// wheels compile it.
pub(crate) trait FromFloat: ChannelType {
    fn from_float(value: f32) -> Self;
}

/// `(uint8_t)value`: both wheels convert with a 32-bit `cvttss2si` and keep the low byte
/// (`Lut1DRendererHalfCode<F16, UINT8>::apply`, Windows 0x180274138, Linux 0x414c15). A value
/// outside `int`'s range, NaN included, gives `INT_MIN`'s low byte, 0; one in range keeps its
/// low byte (docs/improvements.md, I-60).
impl FromFloat for u8 {
    #[inline]
    fn from_float(value: f32) -> u8 {
        sse_cvttps_epi32(value) as u8
    }
}

/// `(uint16_t)value`: a 32-bit `cvttss2si`, keeping the low 16 bits (I-60).
impl FromFloat for u16 {
    #[inline]
    fn from_float(value: f32) -> u16 {
        sse_cvttps_epi32(value) as u16
    }
}

/// `half(value)`: Imath's conversion, to nearest even.
impl FromFloat for half::f16 {
    #[inline]
    fn from_float(value: f32) -> half::f16 {
        half::f16::from_bits(float_to_half(value))
    }
}

impl FromFloat for f32 {
    #[inline]
    fn from_float(value: f32) -> f32 {
        value
    }
}

/// An output bit depth of the renderers, with C++'s conversion of a `float` to its channel
/// type ([`FromFloat`]).
pub(crate) trait LutOutput: Converter + 'static {
    /// The bit depth as the SIMD kernels' RGBA packs name it.
    const PACK: PackDepth;

    /// `OutType(value)`.
    fn from_float(value: f32) -> Self::Type;

    /// The channel value a pack stores as `raw` (an integer, or the bits of a half or float).
    fn from_pack(raw: u32) -> Self::Type;
}

macro_rules! lut_output {
    ($($bd:ty => $pack:ident, |$raw:ident| $from_pack:expr;)*) => {$(
        impl LutOutput for $bd {
            const PACK: PackDepth = PackDepth::$pack;

            #[inline]
            fn from_float(value: f32) -> Self::Type {
                FromFloat::from_float(value)
            }

            #[inline]
            fn from_pack($raw: u32) -> Self::Type {
                $from_pack
            }
        }
    )*};
}

lut_output! {
    Uint8 => Uint8, |raw| raw as u8;
    Uint10 => Uint10, |raw| raw as u16;
    Uint12 => Uint12, |raw| raw as u16;
    Uint16 => Uint16, |raw| raw as u16;
    F16 => F16, |raw| half::f16::from_bits(raw as u16);
    F32 => F32, |raw| f32::from_bits(raw);
}

/// A SIMD kernel of the standard domain's renderer for float input, `m_applyLutFunc`
/// (`Lut1DOpCPU_SSE2.cpp`, `_AVX.cpp`, `_AVX2.cpp`, `_AVX512.cpp`): `linear1D<BIT_DEPTH_F32,
/// outBD>` of each. Upstream runs it on every row of more than one pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lut1DKernel {
    /// `SSE2GetLut1DApplyFunc`.
    Sse2,
    /// `AVXGetLut1DApplyFunc`.
    Avx,
    /// `AVX2GetLut1DApplyFunc`.
    Avx2,
    /// `AVX512GetLut1DApplyFunc`.
    Avx512,
}

impl Lut1DKernel {
    /// One lane of the kernel's `apply_lut_*`: `v` through the table `lut`.
    fn apply_lut(self, lut: &[f32], v: f32, scale: f32, lut_max: f32) -> f32 {
        match self {
            Lut1DKernel::Sse2 => apply_lut_sse2(lut, v, scale, lut_max),
            Lut1DKernel::Avx => apply_lut_avx(lut, v, scale, lut_max),
            Lut1DKernel::Avx2 => apply_lut_avx2(lut, v, scale, lut_max),
            Lut1DKernel::Avx512 => apply_lut_avx512(lut, v, scale, lut_max),
        }
    }

    /// One value of the kernel's RGBA pack `Store`, as raw bits.
    fn store(self, depth: PackDepth, value: f32) -> u32 {
        match self {
            Lut1DKernel::Sse2 => lut1d_op_cpu_sse2::store(depth, value),
            Lut1DKernel::Avx => lut1d_op_cpu_avx::store(depth, value),
            Lut1DKernel::Avx2 => lut1d_op_cpu_avx2::store(depth, value),
            Lut1DKernel::Avx512 => lut1d_op_cpu_avx512::store(depth, value),
        }
    }
}

/// The kernel's `linear1D<BIT_DEPTH_F32, outBD>`, if it has one for `out_bd` on `cpu`: the AVX
/// and AVX2 kernels write halfs only with F16C (`#if OCIO_USE_F16C`, defined in both wheels, and
/// `CPUInfo::hasF16C()`), and give no function otherwise.
///
/// Port of `SSE2GetLut1DApplyFunc`, `AVXGetLut1DApplyFunc`, `AVX2GetLut1DApplyFunc` and
/// `AVX512GetLut1DApplyFunc` for F32 input (Lut1DOpCPU_SSE2.cpp:153-204,
/// Lut1DOpCPU_AVX.cpp:140-195, Lut1DOpCPU_AVX2.cpp:118-173, Lut1DOpCPU_AVX512.cpp:91-146 @
/// v2.5.2).
fn apply_func(kernel: Lut1DKernel, out_bd: BitDepth, cpu: &CpuInfo) -> Option<Lut1DKernel> {
    match (kernel, out_bd) {
        (Lut1DKernel::Avx | Lut1DKernel::Avx2, BitDepth::F16) if !cpu.has_f16c() => None,
        _ => Some(kernel),
    }
}

/// The kernel `Lut1DRenderer` dispatches to on `cpu` for `out_bd`, `m_applyLutFunc`: SSE2's,
/// then AVX's, then AVX2's unless its gathers are slow, then AVX-512's, each replacing the one
/// before, even with none ([`apply_func`]: AVX and AVX2 to F16 without F16C), which leaves the
/// scalar loop; `None` too on a CPU without SSE2. Unlike the hue-adjust renderers' constructor
/// (311-340), this one takes AVX-512 and doesn't check `AVXSlow()`. The `#if OCIO_USE_*` guards
/// are part of `CpuInfo::has_*`.
///
/// Port of the dispatch of `BaseLut1DRenderer::BaseLut1DRenderer(ConstLut1DOpDataRcPtr &)`
/// (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:273-309 @ v2.5.2).
pub fn lut1d_kernel(cpu: &CpuInfo, out_bd: BitDepth) -> Option<Lut1DKernel> {
    let mut kernel = None;
    if cpu.has_sse2() {
        kernel = apply_func(Lut1DKernel::Sse2, out_bd, cpu);
    }
    if cpu.has_avx() {
        kernel = apply_func(Lut1DKernel::Avx, out_bd, cpu);
    }
    if cpu.has_avx2() && !cpu.avx2_slow_gather() {
        kernel = apply_func(Lut1DKernel::Avx2, out_bd, cpu);
    }
    if cpu.has_avx512() {
        kernel = apply_func(Lut1DKernel::Avx512, out_bd, cpu);
    }
    kernel
}

/// `(float)GetBitDepthMaxValue(bd)`.
fn max_value<B: BitDepthInfo>() -> f32 {
    B::MAX_VALUE as f32
}

/// The `L_ADJUST(val)` macro (Lut1DOpCPU.cpp:25-26 @ v2.5.2) up to its cast: for an integer
/// output bit depth, `Clamp(val + 0.5f, 0, outMax)` (NaN becomes 0); for a float one,
/// `SanitizeFloat(val)`.
fn l_adjust(val: f32, is_out_integer: bool, out_max: f32) -> f32 {
    let out_min = 0.0f32;
    if is_out_integer {
        clamp(val + 0.5f32, out_min, out_max)
    } else {
        sanitize_float(val)
    }
}

/// The error of a code of `bit_depth` above `max_index`, past the `entries` of the tables
/// (docs/improvements.md, U-1).
fn past_the_lut(bit_depth: BitDepth, max_index: usize, entries: usize) -> Exception {
    Exception::new(format!(
        "Lut1D: a {} value above {max_index} can't be looked up: upstream reads past the 1D \
         LUT's {entries} entries.",
        crate::open_color_types::bit_depth_to_string(bit_depth),
    ))
}

/// The R, G and B tables of a renderer, `m_tmpLutR`, `m_tmpLutG` and `m_tmpLutB`.
type Tables<T> = [Vec<T>; 3];

/// The tables of a lookup: each of the LUT's values times `outMax`, through `L_ADJUST`, whose
/// cast `(T)` is `cast`.
///
/// Port of `BaseLut1DRenderer::updateData` (Lut1DOpCPU.cpp:374-443 @ v2.5.2), its branch for
/// `isLookup()`, where the LUT needn't be resampled (`!mustResample`): `T` is the table type
/// (the output's, or `float` for hue adjust), `O` the renderer's output bit depth.
fn lookup_tables<T, O: BitDepthInfo>(lut: &Lut1DOpData, cast: fn(f32) -> T) -> Tables<T> {
    let out_max = max_value::<O>();
    // (Used by L_ADJUST macro.)
    let is_out_integer = !O::IS_FLOAT;

    let lut_values = lut.get_array().get_values();
    let dim = lut.get_array().get_length() as usize;
    let mut tables: Tables<T> = [Vec::new(), Vec::new(), Vec::new()];
    // TODO: Would be faster if R, G, B were adjacent in memory?
    for (c, table) in tables.iter_mut().enumerate() {
        *table = (0..dim)
            .map(|i| {
                let val = sse_mul(lut_values[i * 3 + c], out_max);
                cast(l_adjust(val, is_out_integer, out_max))
            })
            .collect();
    }
    tables
}

/// The float tables of float input: `SanitizeFloat(value * outMax)`.
///
/// Port of `BaseLut1DRenderer::updateData` (Lut1DOpCPU.cpp:421-435 @ v2.5.2), its branch for
/// float input.
fn float_tables<O: BitDepthInfo>(lut: &Lut1DOpData) -> Tables<f32> {
    let out_max = max_value::<O>();
    let lut_values = lut.get_array().get_values();
    let dim = lut.get_array().get_length() as usize;
    let mut tables: Tables<f32> = [Vec::new(), Vec::new(), Vec::new()];
    for (c, table) in tables.iter_mut().enumerate() {
        *table = (0..dim)
            .map(|i| sanitize_float(sse_mul(lut_values[i * 3 + c], out_max)))
            .collect();
    }
    tables
}

/// The lookup renderer from `I` codes to `O` values: the forward 1D LUT, standard domain or
/// half domain, of one entry per code of `I`.
///
/// Port of `BaseLut1DRenderer<inBD, outBD>` for `inBD != BIT_DEPTH_F32` (its tables,
/// `updateData`, Lut1DOpCPU.cpp:374-443) and the lookup branches of `Lut1DRenderer::apply` and
/// `Lut1DRendererHalfCode::apply` (Lut1DOpCPU.cpp:501-530, 622-650 @ v2.5.2), which are the
/// same.
pub(crate) struct Lut1DLookupRenderer<I: BitDepthInfo, O: LutOutput> {
    /// `m_tmpLutR`, `m_tmpLutG` and `m_tmpLutB`: each entry times the output's maximum,
    /// through `L_ADJUST`.
    luts: Tables<O::Type>,
    /// `m_alphaScaling`: `(float)maxValue(outBD) / (float)maxValue(inBD)`.
    alpha_scaling: f32,
    input: PhantomData<fn() -> I>,
}

impl<I: BitDepthInfo, O: LutOutput> fmt::Debug for Lut1DLookupRenderer<I, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut1DLookupRenderer")
            .field("in", &I::BIT_DEPTH)
            .field("out", &O::BIT_DEPTH)
            .field("entries", &self.luts[0].len())
            .finish()
    }
}

impl<I: BitDepthInfo, O: LutOutput> Lut1DLookupRenderer<I, O>
where
    I::Type: LookupIndex,
{
    /// The renderer of `lut`, which a lookup with `I` codes may use as it is
    /// (`!mustResample`).
    ///
    /// Port of `BaseLut1DRenderer::updateData` (Lut1DOpCPU.cpp:374-443 @ v2.5.2), its lookup
    /// branch.
    fn new(lut: &Lut1DOpData) -> Self {
        Lut1DLookupRenderer {
            luts: lookup_tables::<O::Type, O>(lut, O::from_float),
            alpha_scaling: max_value::<O>() / max_value::<I>(),
            input: PhantomData,
        }
    }

    /// The largest code the tables hold.
    fn max_index(&self) -> usize {
        self.luts[0].len() - 1
    }

    /// The error of a code past the tables (docs/improvements.md, U-1).
    fn past_the_lut(&self) -> Exception {
        past_the_lut(I::BIT_DEPTH, self.max_index(), self.luts[0].len())
    }

    /// One channel's value: `lutData[GetLookupValue(val)]`.
    ///
    /// Port of `LookupLut::compute` (Lut1DOpCPU.cpp:57-66 @ v2.5.2).
    fn lookup(&self, c: usize, value: I::Type) -> O::Type {
        self.luts[c][value.lookup_index()]
    }

    /// `OutType(in[3] * m_alphaScaling)`.
    fn alpha(&self, value: I::Type) -> O::Type {
        O::from_float(sse_mul(value.to_float(), self.alpha_scaling))
    }
}

impl<I: BitDepthInfo + 'static, O: LutOutput> CpuOp for Lut1DLookupRenderer<I, O>
where
    I::Type: LookupIndex,
{
    /// The CPU engine only runs the renderer between an image of its input bit depth and
    /// F32 values ([`apply_bit_depth`](Self::apply_bit_depth)).
    fn apply(&self, _rgba: &mut [f32]) {
        panic!(
            "{self:?} converts from {:?} codes; it can't work in place on floats",
            I::BIT_DEPTH
        );
    }

    /// Port of `Lut1DRenderer::apply` and `Lut1DRendererHalfCode::apply`, the lookup branch
    /// (Lut1DOpCPU.cpp:501-530, 622-650 @ v2.5.2).
    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());
        let (Some(input), Some(output)) = (
            I::Type::from_pixels(input),
            O::Type::from_pixels_mut(output),
        ) else {
            panic!("{self:?} got {in_name} to {out_name}");
        };
        assert_eq!(
            input.len(),
            output.len(),
            "{self:?}: the pixel counts differ"
        );

        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0)
        {
            out[0] = self.lookup(0, inp[0]);
            out[1] = self.lookup(1, inp[1]);
            out[2] = self.lookup(2, inp[2]);
            out[3] = self.alpha(inp[3]);
        }
    }

    /// The codes of a 10- or 12-bit image past the tables are an error (docs/improvements.md,
    /// U-1), where upstream reads past them.
    fn check_input(&self, input: Pixels<'_>) -> Result<()> {
        let Some(input) = I::Type::from_pixels(input) else {
            return Ok(());
        };
        let max = self.max_index();
        for px in input.as_chunks::<4>().0 {
            if px[..3].iter().any(|v| v.lookup_index() > max) {
                return Err(self.past_the_lut());
            }
        }
        Ok(())
    }

    /// `apply(pixel, pixel, 1)`, as `applyRGB` and `applyRGBA` call it (docs/improvements.md,
    /// I-41), for F32 output, the only one the CPU engine asks of a lookup: the renderer reads
    /// the pixel's first bytes as `I` codes and writes four floats over the same 16 bytes, in
    /// the order each wheel compiled:
    /// - 8-bit input, on both (Windows 0x180270f30, Linux 0x408840), reads each code after it
    ///   stores the float before it;
    /// - 10-, 12-, 16-bit and half input on Windows (0x180271610, folded for the three integer
    ///   depths; 0x180274480 half) does so too; on Linux (0x4083c0, 0x407f40, 0x407ac0,
    ///   0x415550) each code is read one step ahead, before the store of the float before it,
    ///   so only alpha is read after a store.
    fn apply_pixel_in_place(&self, pixel: &mut [f32; 4]) -> Result<()> {
        assert_eq!(
            O::BIT_DEPTH,
            BitDepth::F32,
            "{self:?}: the CPU engine runs a lookup in place to F32 only"
        );
        let mut bytes = [0u8; 16];
        for (k, value) in pixel.iter().enumerate() {
            bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_ne_bytes());
        }

        let size = size_of::<I::Type>();
        let read = |bytes: &[u8; 16], k: usize| I::Type::read_ne(&bytes[k * size..]);
        let write = |bytes: &mut [u8; 16], k: usize, value: O::Type| {
            value.write_ne(&mut bytes[4 * k..]);
        };
        // A code past the tables is an error (docs/improvements.md, U-1); the pixel is left as
        // it was.
        let max = self.max_index();
        let looked_up = |c: usize, code: I::Type| {
            if code.lookup_index() > max {
                return Err(self.past_the_lut());
            }
            Ok(self.lookup(c, code))
        };

        if cfg!(target_os = "windows") || size == 1 {
            for c in 0..3 {
                let value = looked_up(c, read(&bytes, c))?;
                write(&mut bytes, c, value);
            }
        } else {
            let r = read(&bytes, 0);
            let g = read(&bytes, 1);
            write(&mut bytes, 0, looked_up(0, r)?);
            let b = read(&bytes, 2);
            write(&mut bytes, 1, looked_up(1, g)?);
            write(&mut bytes, 2, looked_up(2, b)?);
        }
        let a = read(&bytes, 3);
        write(&mut bytes, 3, self.alpha(a));

        for (k, value) in pixel.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().expect("4 bytes"));
        }
        Ok(())
    }
}

/// The two half codes around a float, and where the float lies between them.
///
/// Port of `IndexPair` (Lut1DOpCPU.cpp:114-123 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IndexPair {
    pub(crate) val_a: u16,
    pub(crate) val_b: u16,
    pub(crate) fraction: f32,
}

/// Whether half bits are an infinity.
fn half_is_infinity(bits: u16) -> bool {
    bits & 0x7fff == 0x7c00
}

/// `-HALF_MAX` or `HALF_MAX`, as half bits, for the sign of `bits`.
fn half_max_of_sign(bits: u16) -> u16 {
    if bits & 0x8000 != 0 { 0xfbff } else { 0x7bff }
}

/// The half codes `valA` and `valB` that bracket `f_in`, and `f_in`'s fraction of the way from
/// `valA`'s value to `valB`'s: an infinity counts as `±HALF_MAX`, and a NaN fraction (a NaN
/// input, or two equal codes) becomes 0. The codes step through the bit patterns, so `valB`
/// past `0xffff` wraps to 0, as upstream's `unsigned short` does.
///
/// Port of `IndexPair::GetEdgeFloatValues` (Lut1DOpCPU.cpp:568-620 @ v2.5.2).
pub(crate) fn get_edge_float_values(f_in: f32) -> IndexPair {
    // TODO: Could we speed this up (perhaps alternate nan/inf behavior)?
    let mut f_in = f_in;
    let mut half_val = float_to_half(f_in);
    if half_is_infinity(half_val) {
        half_val = half_max_of_sign(half_val);
        f_in = half_to_float(half_val);
    }

    // Convert back to float to compare to fIn
    // and interpolate both values.
    let float_temp = half_to_float(half_val);

    let (val_a, mut val_b);
    // Strict comparison required otherwise negative fractions will occur.
    if float_temp.abs() > f_in.abs() {
        val_b = half_val;
        val_a = val_b.wrapping_sub(1);
    } else {
        val_a = half_val;
        val_b = val_a.wrapping_add(1);

        if half_is_infinity(val_b) {
            val_b = half_max_of_sign(val_b);
            // Necessary to reset fIn too (consider fIn = 65519, it's > HALF_MAX but not Inf).
            f_in = half_to_float(val_b);
        }
    }

    let f_a = half_to_float(val_a);
    let f_b = half_to_float(val_b);

    let mut fraction = (f_in - f_a) / (f_b - f_a);
    if fraction.is_nan() {
        fraction = 0.0;
    }

    IndexPair {
        val_a,
        val_b,
        fraction,
    }
}

/// The renderer of a LUT for float input, to `O` values: a standard domain or a half domain.
///
/// Port of `BaseLut1DRenderer<BIT_DEPTH_F32, outBD>` (its float tables, `updateData`,
/// Lut1DOpCPU.cpp:374-443) and the float branches of `Lut1DRenderer::apply` (652-720: the SIMD
/// kernel for more than one pixel, else the scalar loop) and `Lut1DRendererHalfCode::apply`
/// (531-566 @ v2.5.2).
pub(crate) struct Lut1DFloatRenderer<O: LutOutput> {
    /// `m_tmpLutR`, `m_tmpLutG` and `m_tmpLutB`: `SanitizeFloat(value * outMax)`.
    luts: Tables<f32>,
    /// `m_alphaScaling`: `(float)maxValue(outBD) / 1.0f`.
    alpha_scaling: f32,
    /// `m_step`: `((float)m_dim - 1.0f) / 1.0f`.
    step: f32,
    /// `m_dimMinusOne`: `m_dim - 1.0f`.
    dim_minus_one: f32,
    /// Whether the LUT's domain is the half codes (`Lut1DRendererHalfCode`).
    half_code: bool,
    /// `m_applyLutFunc`, a standard domain's SIMD kernel; `None` for the scalar profile.
    kernel: Option<Lut1DKernel>,
    /// The kernel's `rgb_scale`: `1.0f / (float)maxValue(BIT_DEPTH_F32) * ((float)dim - 1)`.
    kernel_scale: f32,
    output: PhantomData<fn() -> O>,
}

impl<O: LutOutput> fmt::Debug for Lut1DFloatRenderer<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(if self.half_code {
            "Lut1DRendererHalfCode"
        } else {
            "Lut1DRenderer"
        })
        .field("out", &O::BIT_DEPTH)
        .field("kernel", &self.kernel)
        .field("entries", &self.luts[0].len())
        .finish()
    }
}

impl<O: LutOutput> Lut1DFloatRenderer<O> {
    /// Port of `BaseLut1DRenderer::updateData` (Lut1DOpCPU.cpp:374-443 @ v2.5.2) for float
    /// input, with the SIMD kernel the constructor picked (`kernel`, for a standard domain).
    fn new(lut: &Lut1DOpData, kernel: Option<Lut1DKernel>) -> Self {
        let dim = lut.get_array().get_length();
        // The input's maximum, `(float)GetBitDepthMaxValue(BIT_DEPTH_F32)`.
        let in_max = max_value::<F32>();
        Lut1DFloatRenderer {
            luts: float_tables::<O>(lut),
            alpha_scaling: max_value::<O>() / in_max,
            step: (dim as f32 - 1.0f32) / in_max,
            dim_minus_one: dim as f32 - 1.0f32,
            half_code: lut.is_input_half_domain(),
            kernel,
            kernel_scale: 1.0f32 / in_max * (dim as f32 - 1.0f32),
            output: PhantomData,
        }
    }

    /// The kernel for a call of `values` floats: `m_applyLutFunc && numPixels > 1`.
    fn kernel_for(&self, values: usize) -> Option<Lut1DKernel> {
        self.kernel.filter(|_| values / 4 > 1)
    }

    /// The kernel's R, G and B for one pixel: `apply_lut_*` on the three tables, with
    /// `lut_max = (float)dim - 1`.
    fn kernel_rgb(&self, kernel: Lut1DKernel, px: &[f32; 4]) -> [f32; 3] {
        std::array::from_fn(|c| {
            kernel.apply_lut(&self.luts[c], px[c], self.kernel_scale, self.dim_minus_one)
        })
    }

    /// One pixel of the kernel, to another output than F32: R, G and B, and alpha times
    /// `alpha_scale` (`inBD != outBD`), through the pack's `Store`.
    fn kernel_pixel(&self, kernel: Lut1DKernel, px: &[f32; 4]) -> [O::Type; 4] {
        let [r, g, b] = self.kernel_rgb(kernel, px);
        let a = sse_mul(px[3], self.alpha_scaling);
        [r, g, b, a].map(|v| O::from_pack(kernel.store(O::PACK, v)))
    }

    /// `Converter<outBD>::CastValue(in[3] * m_alphaScaling)`.
    fn alpha(&self, value: f32) -> O::Type {
        O::cast_value(sse_mul(value, self.alpha_scaling))
    }

    /// One pixel of a standard domain.
    ///
    /// Port of `Lut1DRenderer::apply`'s scalar loop (Lut1DOpCPU.cpp:659-720 @ v2.5.2).
    fn render(&self, px: &[f32; 4]) -> [O::Type; 4] {
        let mut out = [O::Type::default(); 4];
        for c in 0..3 {
            let idx = sse_mul(self.step, px[c]);

            // NaNs become 0
            let idx = std_min(std_max(0.0f32, idx), self.dim_minus_one);

            let low_idx = idx.floor() as u32;

            // When the idx is exactly equal to an index (e.g. 0,1,2...)
            // then the computation of highIdx is wrong. However,
            // the delta is then equal to zero (e.g. lowIdx-idx),
            // so the highIdx has no impact.
            let high_idx = idx.ceil() as u32;

            // Computing delta relative to high rather than lowIdx
            // to save computing (1-delta) below.
            let delta = high_idx as f32 - idx;

            // Since fraction is in the domain [0, 1), interpolate using 1-fraction
            // in order to avoid cases like -/+Inf * 0. Therefore we never multiply by 0 and
            // thus handle the case where A or B is infinity and return infinity rather than
            // 0*Infinity (which is NaN).
            let lut = &self.luts[c];
            out[c] = O::cast_value(lerpf(lut[high_idx as usize], lut[low_idx as usize], delta));
        }
        out[3] = self.alpha(px[3]);
        out
    }

    /// One pixel of a half domain.
    ///
    /// Port of `Lut1DRendererHalfCode::apply`'s float branch (Lut1DOpCPU.cpp:531-566 @
    /// v2.5.2).
    fn render_half_code(&self, px: &[f32; 4]) -> [O::Type; 4] {
        let mut out = [O::Type::default(); 4];
        for c in 0..3 {
            let inter_vals = get_edge_float_values(px[c]);
            let lut = &self.luts[c];

            // Since fraction is in the domain [0, 1), interpolate using
            // 1-fraction in order to avoid cases like -/+Inf * 0.
            out[c] = O::cast_value(lerpf(
                lut[inter_vals.val_b as usize],
                lut[inter_vals.val_a as usize],
                1.0f32 - inter_vals.fraction,
            ));
        }
        out[3] = self.alpha(px[3]);
        out
    }

    /// The pixel's output values.
    fn render_pixel(&self, px: &[f32; 4]) -> [O::Type; 4] {
        if self.half_code {
            self.render_half_code(px)
        } else {
            self.render(px)
        }
    }
}

impl<O: LutOutput> CpuOp for Lut1DFloatRenderer<O> {
    /// `apply(img, img, numPixels)` on F32 pixels, for an F32 output. With the kernel
    /// (`inBD == outBD`), alpha only moves through the packs: it is never written here.
    fn apply(&self, rgba: &mut [f32]) {
        assert_eq!(
            O::BIT_DEPTH,
            BitDepth::F32,
            "{self:?} writes {:?} values; it can't work in place on floats",
            O::BIT_DEPTH
        );
        if let Some(kernel) = self.kernel_for(rgba.len()) {
            for px in rgba.as_chunks_mut::<4>().0 {
                let rgb = self.kernel_rgb(kernel, px);
                for c in 0..3 {
                    px[c] = O::from_pack(kernel.store(O::PACK, rgb[c])).to_float();
                }
            }
            return;
        }
        for px in rgba.as_chunks_mut::<4>().0 {
            let out = self.render_pixel(px);
            *px = out.map(ChannelType::to_float);
        }
    }

    /// Port of `Lut1DRenderer::apply` and `Lut1DRendererHalfCode::apply`, the float branches
    /// (Lut1DOpCPU.cpp:531-566, 652-720 @ v2.5.2).
    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());
        let Pixels::F32(input) = input else {
            panic!("{self:?} got {in_name} to {out_name}");
        };
        let kernel = self.kernel_for(input.len());
        if kernel.is_some() && O::BIT_DEPTH == BitDepth::F32 {
            // F32 to F32: the input's alpha is the output's; the rest in place.
            let PixelsMut::F32(output) = output else {
                panic!("{self:?} got {in_name} to {out_name}");
            };
            output.copy_from_slice(input);
            self.apply(output);
            return;
        }
        let Some(output) = O::Type::from_pixels_mut(output) else {
            panic!("{self:?} got {in_name} to {out_name}");
        };
        assert_eq!(
            input.len(),
            output.len(),
            "{self:?}: the pixel counts differ"
        );
        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0)
        {
            *out = match kernel {
                Some(kernel) => self.kernel_pixel(kernel, inp),
                None => self.render_pixel(inp),
            };
        }
    }

    /// `apply(pixel, pixel, 1)`, as `applyRGB` and `applyRGBA` call it (docs/improvements.md,
    /// I-41): the renderer reads the pixel's four floats and writes four `O` values over its
    /// first bytes. Each output value is no wider than a float, so it overwrites only floats
    /// already read, whatever the order.
    fn apply_pixel_in_place(&self, pixel: &mut [f32; 4]) -> Result<()> {
        let out = self.render_pixel(pixel);
        let mut bytes = [0u8; 16];
        for (k, value) in pixel.iter().enumerate() {
            bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_ne_bytes());
        }
        let size = size_of::<O::Type>();
        for (k, value) in out.into_iter().enumerate() {
            value.write_ne(&mut bytes[k * size..]);
        }
        for (k, value) in pixel.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().expect("4 bytes"));
        }
        Ok(())
    }
}

/// The indices of the smallest, middle and largest of `rgb`'s values, `(min, mid, max)`,
/// without branches. A comparison with a NaN is false, so `{A, NaN, B}` with `A > B` makes the
/// first two comparisons false and the third true, which the table's `+ 3` maps to `val = 0`.
///
/// Port of `GamutMapUtils::Order3` (Lut1DOpCPU.cpp:723-745 @ v2.5.2).
pub(crate) fn order3(rgb: &[f32; 3]) -> (usize, usize, usize) {
    //                                    0  1  2  3  4  5  6  7  8  (typical val - 3)
    const TABLE: [usize; 12] = [2, 1, 0, 2, 1, 0, 2, 1, 2, 0, 1, 2];

    let val = (i32::from(rgb[0] > rgb[1]) * 5 + i32::from(rgb[1] > rgb[2]) * 4)
        - i32::from(rgb[0] > rgb[2]) * 3
        + 3;
    let val = val as usize;

    let max = TABLE[val];
    let mid = TABLE[val + 1];
    let min = TABLE[val + 2];
    (min, mid, max)
}

/// Hue adjust (`HUE_DW3`): the chroma of `rgb` before the LUT sets the middle value of `rgb2`,
/// the LUT's output, so that the hue is kept.
///
/// `RGB2[mid] = hue_factor * new_chroma + RGB2[min]` (Lut1DOpCPU.cpp:771-788, 803-834,
/// 866-884, 899-907 and 984-986 @ v2.5.2): both wheels multiply `new_chroma * hue_factor`, the
/// reverse of the source (`Lut1DRendererHueAdjust<F32, F32>::apply`, Windows 0x18027a500,
/// Linux 0x415dc3; `Lut1DRendererHalfCodeHueAdjust<F32, F32>::apply`, Windows 0x180278964,
/// Linux 0x445c2d). Both are NaN when the input has a NaN (`hue_factor`) and the LUT gives
/// infinities of one sign (`new_chroma`), and the product is then `new_chroma`'s.
fn adjust_hue(rgb: &[f32; 3], rgb2: &mut [f32; 3]) {
    let (min, mid, max) = order3(rgb);

    let orig_chroma = rgb[max] - rgb[min];
    let hue_factor = if orig_chroma == 0.0 {
        0.0
    } else {
        (rgb[mid] - rgb[min]) / orig_chroma
    };

    let new_chroma = rgb2[max] - rgb2[min];

    rgb2[mid] = sse_add(sse_mul(new_chroma, hue_factor), rgb2[min]);
}

/// The hue-adjust lookup renderer from `I` codes to `O` values.
///
/// Port of `Lut1DRendererHueAdjust<inBD, outBD>` and `Lut1DRendererHalfCodeHueAdjust<inBD,
/// outBD>` for `inBD != BIT_DEPTH_F32`: their tables are `float` (`BaseLut1DRenderer(lut,
/// BIT_DEPTH_F32)` makes `update` call `updateData<float>`), through `L_ADJUST` for `outBD`
/// (Lut1DOpCPU.cpp:155-177, 311-443), and their lookup branches, which are the same
/// (753-798, 848-894 @ v2.5.2).
pub(crate) struct Lut1DHueAdjustLookupRenderer<I: BitDepthInfo, O: LutOutput> {
    /// `m_tmpLutR`, `m_tmpLutG` and `m_tmpLutB`: each entry times the output's maximum,
    /// through `L_ADJUST`, as `float`.
    luts: Tables<f32>,
    /// `m_alphaScaling`: `(float)maxValue(outBD) / (float)maxValue(inBD)`.
    alpha_scaling: f32,
    bit_depths: PhantomData<fn() -> (I, O)>,
}

impl<I: BitDepthInfo, O: LutOutput> fmt::Debug for Lut1DHueAdjustLookupRenderer<I, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut1DHueAdjustLookupRenderer")
            .field("in", &I::BIT_DEPTH)
            .field("out", &O::BIT_DEPTH)
            .field("entries", &self.luts[0].len())
            .finish()
    }
}

impl<I: BitDepthInfo, O: LutOutput> Lut1DHueAdjustLookupRenderer<I, O>
where
    I::Type: LookupIndex,
{
    /// Port of `BaseLut1DRenderer::updateData<float>` (Lut1DOpCPU.cpp:374-443 @ v2.5.2), its
    /// lookup branch, for a hue-adjust renderer.
    fn new(lut: &Lut1DOpData) -> Self {
        Lut1DHueAdjustLookupRenderer {
            luts: lookup_tables::<f32, O>(lut, f32::from_float),
            alpha_scaling: max_value::<O>() / max_value::<I>(),
            bit_depths: PhantomData,
        }
    }

    /// The largest code the tables hold.
    fn max_index(&self) -> usize {
        self.luts[0].len() - 1
    }

    /// One pixel.
    ///
    /// Port of the lookup branches of `Lut1DRendererHalfCodeHueAdjust::apply` and
    /// `Lut1DRendererHueAdjust::apply` (Lut1DOpCPU.cpp:767-798, 862-894 @ v2.5.2).
    fn render(&self, inp: &[I::Type; 4]) -> [O::Type; 4] {
        let rgb = [inp[0].to_float(), inp[1].to_float(), inp[2].to_float()];
        let mut rgb2: [f32; 3] = std::array::from_fn(|c| self.luts[c][inp[c].lookup_index()]);
        adjust_hue(&rgb, &mut rgb2);
        [
            O::from_float(rgb2[0]),
            O::from_float(rgb2[1]),
            O::from_float(rgb2[2]),
            O::from_float(sse_mul(inp[3].to_float(), self.alpha_scaling)),
        ]
    }
}

impl<I: BitDepthInfo + 'static, O: LutOutput> CpuOp for Lut1DHueAdjustLookupRenderer<I, O>
where
    I::Type: LookupIndex,
{
    /// The CPU engine only runs the renderer between an image of its input bit depth and
    /// F32 values ([`apply_bit_depth`](Self::apply_bit_depth)).
    fn apply(&self, _rgba: &mut [f32]) {
        panic!(
            "{self:?} converts from {:?} codes; it can't work in place on floats",
            I::BIT_DEPTH
        );
    }

    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());
        let (Some(input), Some(output)) = (
            I::Type::from_pixels(input),
            O::Type::from_pixels_mut(output),
        ) else {
            panic!("{self:?} got {in_name} to {out_name}");
        };
        assert_eq!(
            input.len(),
            output.len(),
            "{self:?}: the pixel counts differ"
        );
        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0)
        {
            *out = self.render(inp);
        }
    }

    /// The codes of a 10- or 12-bit image past the tables are an error (docs/improvements.md,
    /// U-1), where upstream reads past them.
    fn check_input(&self, input: Pixels<'_>) -> Result<()> {
        let Some(input) = I::Type::from_pixels(input) else {
            return Ok(());
        };
        let max = self.max_index();
        for px in input.as_chunks::<4>().0 {
            if px[..3].iter().any(|v| v.lookup_index() > max) {
                return Err(past_the_lut(I::BIT_DEPTH, max, self.luts[0].len()));
            }
        }
        Ok(())
    }

    /// `apply(pixel, pixel, 1)`, as `applyRGB` and `applyRGBA` call it (docs/improvements.md,
    /// I-41), for F32 output, the only one the CPU engine asks of a lookup: the renderer reads
    /// the pixel's first bytes as `I` codes and writes four floats over the same 16 bytes. Both
    /// wheels read the three colour codes first; then, but for 10-, 12- and 16-bit input on
    /// Linux, they store the three floats before they read alpha, which is then a byte of red's
    /// float (8-bit input) or half of green's (16-bit types):
    /// - `Lut1DRendererHueAdjust<UINT8, F32>::apply`: Windows 0x180278d56, Linux 0x4118e5;
    /// - `<UINT10, F32>`, `<UINT12, F32>`, `<UINT16, F32>`: Windows 0x1802790f6 (folded for the
    ///   three); Linux 0x410f9a and 0x41159a, alpha read before the stores;
    /// - `Lut1DRendererHalfCodeHueAdjust<F16, F32>::apply`: Windows 0x18027760b, Linux
    ///   0x421c93 (after the store of red and green).
    fn apply_pixel_in_place(&self, pixel: &mut [f32; 4]) -> Result<()> {
        assert_eq!(
            O::BIT_DEPTH,
            BitDepth::F32,
            "{self:?}: the CPU engine runs a lookup in place to F32 only"
        );
        let mut bytes = [0u8; 16];
        for (k, value) in pixel.iter().enumerate() {
            bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_ne_bytes());
        }
        let size = size_of::<I::Type>();
        let read = |bytes: &[u8; 16], k: usize| I::Type::read_ne(&bytes[k * size..]);
        let codes = [read(&bytes, 0), read(&bytes, 1), read(&bytes, 2)];
        // A code past the tables is an error (docs/improvements.md, U-1); the pixel is left as
        // it was.
        let max = self.max_index();
        if codes.iter().any(|v| v.lookup_index() > max) {
            return Err(past_the_lut(I::BIT_DEPTH, max, self.luts[0].len()));
        }
        let alpha_first =
            cfg!(not(target_os = "windows")) && size == 2 && I::BIT_DEPTH != BitDepth::F16;
        let original_alpha = read(&bytes, 3);

        let rgb = codes.map(ChannelType::to_float);
        let mut rgb2: [f32; 3] = std::array::from_fn(|c| self.luts[c][codes[c].lookup_index()]);
        adjust_hue(&rgb, &mut rgb2);
        for (k, value) in rgb2.into_iter().enumerate() {
            O::from_float(value).write_ne(&mut bytes[4 * k..]);
        }
        let alpha = if alpha_first {
            original_alpha
        } else {
            read(&bytes, 3)
        };
        O::from_float(sse_mul(alpha.to_float(), self.alpha_scaling)).write_ne(&mut bytes[12..]);

        for (k, value) in pixel.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().expect("4 bytes"));
        }
        Ok(())
    }
}

/// The hue-adjust renderer of a LUT for float input, to `O` values: a standard domain or a
/// half domain.
///
/// Port of `Lut1DRendererHueAdjust<BIT_DEPTH_F32, outBD>` and
/// `Lut1DRendererHalfCodeHueAdjust<BIT_DEPTH_F32, outBD>`: their float tables
/// (`BaseLut1DRenderer::updateData<float>`, Lut1DOpCPU.cpp:374-443) and their float branches
/// (799-845, 895-996 @ v2.5.2). The standard domain's is the `#if OCIO_USE_SSE2` branch
/// (909-941), which every x86-64 wheel compiles: it truncates the index rather than taking its
/// floor (the same for the clamped, non-negative indices) and takes the next index up as the
/// high one, so that an input on a node interpolates from that node to the next with a weight
/// of 1.
pub(crate) struct Lut1DHueAdjustFloatRenderer<O: LutOutput> {
    /// `m_tmpLutR`, `m_tmpLutG` and `m_tmpLutB`: `SanitizeFloat(value * outMax)`.
    luts: Tables<f32>,
    /// `m_alphaScaling`: `(float)maxValue(outBD) / 1.0f`.
    alpha_scaling: f32,
    /// `m_step`: `((float)m_dim - 1.0f) / 1.0f`.
    step: f32,
    /// `m_dimMinusOne`: `m_dim - 1.0f`.
    dim_minus_one: f32,
    /// Whether the LUT's domain is the half codes (`Lut1DRendererHalfCodeHueAdjust`).
    half_code: bool,
    output: PhantomData<fn() -> O>,
}

impl<O: LutOutput> fmt::Debug for Lut1DHueAdjustFloatRenderer<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(if self.half_code {
            "Lut1DRendererHalfCodeHueAdjust"
        } else {
            "Lut1DRendererHueAdjust"
        })
        .field("out", &O::BIT_DEPTH)
        .field("entries", &self.luts[0].len())
        .finish()
    }
}

impl<O: LutOutput> Lut1DHueAdjustFloatRenderer<O> {
    /// Port of `BaseLut1DRenderer::updateData<float>` (Lut1DOpCPU.cpp:374-443 @ v2.5.2) for
    /// float input.
    fn new(lut: &Lut1DOpData) -> Self {
        let dim = lut.get_array().get_length();
        let in_max = max_value::<F32>();
        Lut1DHueAdjustFloatRenderer {
            luts: float_tables::<O>(lut),
            alpha_scaling: max_value::<O>() / in_max,
            step: (dim as f32 - 1.0f32) / in_max,
            dim_minus_one: dim as f32 - 1.0f32,
            half_code: lut.is_input_half_domain(),
            output: PhantomData,
        }
    }

    /// The LUT's values for a standard domain, before the hue adjustment.
    ///
    /// Port of `Lut1DRendererHueAdjust::apply`'s float branch, its `OCIO_USE_SSE2` code
    /// (Lut1DOpCPU.cpp:909-941, 978-982 @ v2.5.2): the lanes of `_mm_mul_ps`, `_mm_max_ps`,
    /// `_mm_min_ps`, `_mm_cvttps_epi32` and `_mm_add_ps`, in the source's operand order, which
    /// both wheels keep (Windows 0x18027a42c-0x18027a451, Linux 0x415ce4-0x415d07).
    fn lut_values(&self, rgb: &[f32; 3]) -> [f32; 3] {
        std::array::from_fn(|c| {
            let idx = sse_mul(rgb[c], self.step);

            // _mm_max_ps => NaNs become 0
            let idx = sse_min(sse_max(idx, 0.0f32), self.dim_minus_one);

            // zero < std::floor(idx) < maxIdx
            // SSE => zero < truncate(idx) < maxIdx
            // then clamp to prevent hIdx from falling off the end
            // of the LUT
            let low_idx = sse_cvttps_epi32(idx) as f32;

            // zero < std::ceil(idx) < maxIdx
            // SSE => (lowIdx (already truncated) + 1) < maxIdx
            let high_idx = sse_min(sse_add(low_idx, 1.0f32), self.dim_minus_one);

            // Computing delta relative to high rather than lowIdx
            // to save computing (1-delta) below.
            let delta = high_idx - idx;

            // Since fraction is in the domain [0, 1), interpolate using 1-fraction
            // in order to avoid cases like -/+Inf * 0. Therefore we never multiply by 0 and
            // thus handle the case where A or B is infinity and return infinity rather than
            // 0*Infinity (which is NaN).
            let lut = &self.luts[c];
            lerpf(
                lut[high_idx as u32 as usize],
                lut[low_idx as u32 as usize],
                delta,
            )
        })
    }

    /// The LUT's values for a half domain, before the hue adjustment.
    ///
    /// Port of `Lut1DRendererHalfCodeHueAdjust::apply`'s float branch (Lut1DOpCPU.cpp:808-823
    /// @ v2.5.2).
    fn lut_values_half_code(&self, rgb: &[f32; 3]) -> [f32; 3] {
        std::array::from_fn(|c| {
            let inter_vals = get_edge_float_values(rgb[c]);
            let lut = &self.luts[c];

            // Since fraction is in the domain [0, 1), interpolate using
            // 1-fraction in order to avoid cases like -/+Inf * 0.
            lerpf(
                lut[inter_vals.val_b as usize],
                lut[inter_vals.val_a as usize],
                1.0f32 - inter_vals.fraction,
            )
        })
    }

    /// One pixel.
    ///
    /// Port of the float branches of `Lut1DRendererHalfCodeHueAdjust::apply` and
    /// `Lut1DRendererHueAdjust::apply` (Lut1DOpCPU.cpp:799-845, 895-996 @ v2.5.2).
    fn render_pixel(&self, px: &[f32; 4]) -> [O::Type; 4] {
        let rgb = [px[0], px[1], px[2]];
        let mut rgb2 = if self.half_code {
            self.lut_values_half_code(&rgb)
        } else {
            self.lut_values(&rgb)
        };
        adjust_hue(&rgb, &mut rgb2);
        [
            O::cast_value(rgb2[0]),
            O::cast_value(rgb2[1]),
            O::cast_value(rgb2[2]),
            O::cast_value(sse_mul(px[3], self.alpha_scaling)),
        ]
    }
}

impl<O: LutOutput> CpuOp for Lut1DHueAdjustFloatRenderer<O> {
    /// `apply(img, img, numPixels)` on F32 pixels, for an F32 output.
    fn apply(&self, rgba: &mut [f32]) {
        assert_eq!(
            O::BIT_DEPTH,
            BitDepth::F32,
            "{self:?} writes {:?} values; it can't work in place on floats",
            O::BIT_DEPTH
        );
        for px in rgba.as_chunks_mut::<4>().0 {
            let out = self.render_pixel(px);
            *px = out.map(ChannelType::to_float);
        }
    }

    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());
        let (Pixels::F32(input), Some(output)) = (input, O::Type::from_pixels_mut(output)) else {
            panic!("{self:?} got {in_name} to {out_name}");
        };
        assert_eq!(
            input.len(),
            output.len(),
            "{self:?}: the pixel counts differ"
        );
        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0)
        {
            *out = self.render_pixel(inp);
        }
    }

    /// `apply(pixel, pixel, 1)` (docs/improvements.md, I-41): the four floats are read before
    /// any value is written, and each output value is no wider than a float.
    fn apply_pixel_in_place(&self, pixel: &mut [f32; 4]) -> Result<()> {
        let out = self.render_pixel(pixel);
        let mut bytes = [0u8; 16];
        for (k, value) in pixel.iter().enumerate() {
            bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_ne_bytes());
        }
        let size = size_of::<O::Type>();
        for (k, value) in out.into_iter().enumerate() {
            value.write_ne(&mut bytes[k * size..]);
        }
        for (k, value) in pixel.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().expect("4 bytes"));
        }
        Ok(())
    }
}

/// `std::lower_bound(first, last, value)` on `values[first..last]`: the first index whose value
/// does not compare less than `value` (`last` if none), by the halving steps of libstdc++ and
/// the MSVC STL, which decide the result where the values hold NaNs. Both give `first` for a
/// range that ends before it starts (a negative distance), which an inverse LUT's effective
/// domains never are (`Lut1DOpData::initializeFromForward`).
fn lower_bound(values: &[f32], first: usize, last: usize, value: f32) -> usize {
    let mut first = first;
    let mut len = last.saturating_sub(first);
    while len > 0 {
        let half = len / 2;
        let mid = first + half;
        if values[mid] < value {
            first = mid + 1;
            len = len - half - 1;
        } else {
            len = half;
        }
    }
    first
}

/// The shared part of `FindLutInv` and `FindLutInvHalf` (Lut1DOpCPU.cpp:1015-1063, 1077-1122
/// @ v2.5.2): `val` clamped to `lut[start..=end]` (increasing), the index of the entry at or
/// below it from the LUT's start (`totalInds`), and its fraction of the way to the next entry
/// (`delta`).
fn find_lut_inv_bracket(
    lut: &[f32],
    start: usize,
    start_offset: f32,
    end: usize,
    flip_sign: f32,
    val: f32,
) -> (f32, f32) {
    // Note that the LUT data pointed to by start/end must be in increasing order,
    // regardless of whether the original LUT was increasing or decreasing because
    // this function uses std::lower_bound().

    // Clamp the value to the range of the LUT.
    let cv = std_min(std_max(sse_mul(val, flip_sign), lut[start]), lut[end]);

    // std::lower_bound()
    // "Returns an iterator pointing to the first element in the range [first,last)
    // which does not compare less than val (but could be equal)."
    // (NB: This is correct using either end or end+1 since lower_bound will return a
    //  value one greater than the second argument if no values in the array are >= cv.)
    let mut lowbound = lower_bound(lut, start, end, cv);

    // lower_bound() returns first entry >= val so decrement it unless val == *start.
    if lowbound > start {
        lowbound -= 1;
    }

    let mut highbound = lowbound;
    if highbound < end {
        highbound += 1;
    }

    // Delta is the fractional distance of val between the adjacent LUT entries.
    let mut delta = 0.0f32;
    if lut[highbound] > lut[lowbound] {
        // (handle flat spots by leaving delta = 0)
        delta = (cv - lut[lowbound]) / (lut[highbound] - lut[lowbound]);
    }

    // Inds is the index difference from the effective start to lowbound.
    let inds = (lowbound - start) as f32;

    // Correct for the fact that start is not the beginning of the LUT if it
    // starts with a flat spot.
    // (NB: It may seem like lower_bound would automatically find the end of the
    //  flat spot, so start could always simply be the start of the LUT, however
    //  this fails when val equals the flat spot value.)
    let total_inds = inds + start_offset;
    (total_inds, delta)
}

/// The inverse of `val` through a forward LUT's linear interpolation: the input that would
/// give `val`, in index units times `scale`. `lut[start..=end]` is the effective, increasing
/// table (`flip_sign` -1 negates the value for a decreasing LUT); `start_offset` is `start`'s
/// index in the LUT.
///
/// Port of `FindLutInv` (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:1015-1063 @ v2.5.2).
fn find_lut_inv(
    lut: &[f32],
    start: usize,
    start_offset: f32,
    end: usize,
    flip_sign: f32,
    scale: f32,
    val: f32,
) -> f32 {
    let (total_inds, delta) = find_lut_inv_bracket(lut, start, start_offset, end, flip_sign, val);

    // Scale converts from units of [0,dim] to [0,outDepth].
    (total_inds + delta) * scale
}

/// [`find_lut_inv`] for a half domain: the index pair's halfs, interpolated by `delta`.
///
/// Port of `FindLutInvHalf` (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:1077-1135 @ v2.5.2).
fn find_lut_inv_half(
    lut: &[f32],
    start: usize,
    start_offset: f32,
    end: usize,
    flip_sign: f32,
    scale: f32,
    val: f32,
) -> f32 {
    let (total_inds, delta) = find_lut_inv_bracket(lut, start, start_offset, end, flip_sign, val);

    // For a half domain LUT, the entries are not a constant distance apart,
    // so convert the indices (which are half floats) into real floats in order
    // to calculate what distance the delta factor is working over.
    let base = half_to_float(<u16 as FromFloat>::from_float(total_inds));
    let base_plus1 = half_to_float(<u16 as FromFloat>::from_float(total_inds + 1.0));
    let domain = base + delta * (base_plus1 - base);

    // Scale converts from units of [0,dim] to [0,outDepth].
    domain * scale
}

/// The parameters of one color component of an inverse LUT's renderer: indices into the
/// renderer's table `table` where upstream holds pointers into `m_tmpLutR`, `G` or `B`.
///
/// Port of `ComponentParams` (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:179-207 @ v2.5.2).
#[derive(Debug, Clone, Copy)]
struct ComponentParams {
    /// The renderer's table the indices refer to.
    table: usize,
    /// Copy of the pointer to start of effective lutData.
    lut_start: usize,
    /// Difference between real and effective start of lut.
    start_offset: f32,
    /// Copy of the pointer to end of effective lutData.
    lut_end: usize,
    /// lutStart for negative part of half domain LUT.
    neg_lut_start: usize,
    /// startOffset for negative part of half domain LUT.
    neg_start_offset: f32,
    /// lutEnd for negative part of half domain LUT.
    neg_lut_end: usize,
    /// Flip the sign of value to handle decreasing luts.
    flip_sign: f32,
    /// Point of switching from pos to neg of half domain.
    bisect_point: f32,
}

impl ComponentParams {
    /// Port of `ComponentParams::setComponentParams` (Lut1DOpCPU.cpp:1158-1171 @ v2.5.2).
    fn new(properties: &ComponentProperties, table: usize, lut_zero_entry: f32) -> Self {
        ComponentParams {
            table,
            flip_sign: if properties.is_increasing { 1.0 } else { -1.0 },
            bisect_point: lut_zero_entry,
            start_offset: properties.start_domain as f32,
            lut_start: properties.start_domain as usize,
            lut_end: properties.end_domain as usize,
            neg_start_offset: properties.neg_start_domain as f32,
            neg_lut_start: properties.neg_start_domain as usize,
            neg_lut_end: properties.neg_end_domain as usize,
        }
    }
}

/// How an inverse renderer's `apply(pixel, pixel, 1)` orders its reads of the pixel's codes
/// and its stores of the floats over them, as each wheel compiled it (docs/improvements.md,
/// I-41).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InPlaceOrder {
    /// Each code read after the store of the float before it.
    Sequential,
    /// Each code read one step ahead, before the store of the float before it.
    OneAhead,
    /// The three colour codes read first; alpha after the colour floats are stored.
    ColorsFirst,
    /// Every code read before any store.
    AllFirst,
}

/// The renderer of an inverse LUT from `I` values to `O` values: a standard or half domain,
/// with or without hue adjust. It inverts each value through the forward LUT's interpolation
/// ([`find_lut_inv`], [`find_lut_inv_half`]), whatever the input bit depth: an inverse LUT is
/// never looked up.
///
/// Port of `InvLut1DRenderer`, `InvLut1DRendererHalfCode`, `InvLut1DRendererHueAdjust` and
/// `InvLut1DRendererHalfCodeHueAdjust` (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:209-270,
/// 1137-1625 @ v2.5.2).
pub(crate) struct InvLut1DRenderer<I: BitDepthInfo, O: LutOutput> {
    /// `m_tmpLutR`, and for a LUT of three components `m_tmpLutG` and `m_tmpLutB`: the values
    /// times the input's maximum, negated where a channel is decreasing (and, in a half
    /// domain, its negative half the other way), so that each sorts increasing.
    tables: Vec<Vec<f32>>,
    /// `m_paramsR`, `m_paramsG`, `m_paramsB`.
    params: [ComponentParams; 3],
    /// `m_scale`: from index units to the output's units (`outMax / (dim - 1)`, or `outMax`
    /// for a half domain).
    scale: f32,
    /// `m_alphaScaling`: `(float)maxValue(outBD) / (float)maxValue(inBD)`.
    alpha_scaling: f32,
    half_code: bool,
    hue_adjust: bool,
    bit_depths: PhantomData<fn() -> (I, O)>,
}

impl<I: BitDepthInfo, O: LutOutput> fmt::Debug for InvLut1DRenderer<I, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvLut1DRenderer")
            .field("in", &I::BIT_DEPTH)
            .field("out", &O::BIT_DEPTH)
            .field("half_code", &self.half_code)
            .field("hue_adjust", &self.hue_adjust)
            .finish()
    }
}

impl<I: BitDepthInfo, O: LutOutput> InvLut1DRenderer<I, O> {
    /// Port of `InvLut1DRenderer::updateData` (Lut1DOpCPU.cpp:1181-1243 @ v2.5.2) and
    /// `InvLut1DRendererHalfCode::updateData` (1360-1437).
    fn new(lut: &Lut1DOpData) -> Self {
        let half_code = lut.is_input_half_domain();
        let has_single_lut = lut.has_single_lut();
        let dim = lut.get_array().get_length() as usize;
        let lut_values = lut.get_array().get_values();

        // Get component properties and initialize component parameters structure.
        let properties = [
            *lut.get_red_properties(),
            *lut.get_green_properties(),
            *lut.get_blue_properties(),
        ];
        // The half domain's bisect point is the channel's value at +0, not scaled as the
        // tables are (docs/improvements.md, I-69).
        let zero_entry = |c: usize| if half_code { lut_values[c] } else { 0.0 };
        let params = if has_single_lut {
            // NB: All pointers refer to _tmpLutR.
            let r = ComponentParams::new(&properties[0], 0, zero_entry(0));
            [r, r, r]
        } else {
            std::array::from_fn(|c| ComponentParams::new(&properties[c], c, zero_entry(c)))
        };

        // Fill temporary LUT.
        // Note: Since FindLutInv requires increasing arrays, if the LUT is
        // decreasing we negate the values to obtain the required sort order
        // of smallest to largest.
        let lut_scale = max_value::<I>();
        let tables_count = if has_single_lut { 1 } else { 3 };
        let tables = (0..tables_count)
            .map(|c| {
                let increasing = properties[c].is_increasing;
                (0..dim)
                    .map(|i| {
                        let v = lut_values[i * 3 + c];
                        // (Per above, the LUT must be increasing, so negative half domain is
                        // sign reversed.)
                        let positive = !half_code || i < 32768;
                        if increasing == positive {
                            sse_mul(v, lut_scale)
                        } else {
                            sse_mul(-v, lut_scale)
                        }
                    })
                    .collect()
            })
            .collect();

        let out_max = max_value::<O>();
        InvLut1DRenderer {
            tables,
            params,
            // Converts from index units to inDepth units of the original LUT.
            // (Note that inDepth of the original LUT is outDepth of the inverse LUT.)
            // Note the difference for half domain LUTs, since the distance between
            // between adjacent entries is not constant, we cannot roll it into the
            // scale.
            scale: if half_code {
                out_max
            } else {
                out_max / (dim - 1) as f32
            },
            alpha_scaling: out_max / max_value::<I>(),
            half_code,
            hue_adjust: lut.get_hue_adjust() != Lut1DHueAdjust::None,
            bit_depths: PhantomData,
        }
    }

    /// One component of a standard domain: `FindLutInv` with its parameters.
    fn invert(&self, c: usize, val: f32) -> f32 {
        let p = &self.params[c];
        find_lut_inv(
            &self.tables[p.table],
            p.lut_start,
            p.start_offset,
            p.lut_end,
            p.flip_sign,
            self.scale,
            val,
        )
    }

    /// One component of a half domain: `FindLutInvHalf` on the half of the domain the value's
    /// side of the bisect point gives. Blue's negative half takes red's `flipSign`, as upstream
    /// does (docs/improvements.md, I-67).
    ///
    /// Port of `InvLut1DRendererHalfCode::apply`'s per-channel code
    /// (Lut1DOpCPU.cpp:1460-1532 @ v2.5.2).
    fn invert_half(&self, c: usize, val: f32) -> f32 {
        let p = &self.params[c];
        let is_increasing = p.flip_sign > 0.0;
        let lut = &self.tables[p.table];
        if is_increasing == (val >= p.bisect_point) {
            find_lut_inv_half(
                lut,
                p.lut_start,
                p.start_offset,
                p.lut_end,
                p.flip_sign,
                self.scale,
                val,
            )
        } else {
            let flip_sign = if c == 2 {
                self.params[0].flip_sign
            } else {
                p.flip_sign
            };
            find_lut_inv_half(
                lut,
                p.neg_lut_start,
                p.neg_start_offset,
                p.neg_lut_end,
                -flip_sign,
                self.scale,
                val,
            )
        }
    }

    /// One pixel's output values, from its input values as floats.
    ///
    /// Port of the `apply` of `InvLut1DRenderer` (Lut1DOpCPU.cpp:1245-1286),
    /// `InvLut1DRendererHueAdjust` (1293-1347), `InvLut1DRendererHalfCode` (1439-1540) and
    /// `InvLut1DRendererHalfCodeHueAdjust` (1549-1623 @ v2.5.2).
    fn render(&self, rgba: [f32; 4]) -> [O::Type; 4] {
        let rgb = [rgba[0], rgba[1], rgba[2]];
        let mut rgb2: [f32; 3] = std::array::from_fn(|c| {
            if self.half_code {
                self.invert_half(c, rgb[c])
            } else {
                self.invert(c, rgb[c])
            }
        });
        if self.hue_adjust {
            adjust_hue(&rgb, &mut rgb2);
        }
        [
            O::cast_value(rgb2[0]),
            O::cast_value(rgb2[1]),
            O::cast_value(rgb2[2]),
            O::cast_value(sse_mul(rgba[3], self.alpha_scaling)),
        ]
    }

    /// How the wheel compiled this renderer's `apply(pixel, pixel, 1)` (`InPlaceOrder`):
    /// - `InvLut1DRenderer`: each code after the float before it on Windows (0x18025d830
    ///   8-bit, 0x18025e150 10-, 12- and 16-bit, 0x18025f2e0 half) and for 8 bits on Linux
    ///   (0x40b870); one step ahead for 16-bit types on Linux (0x40bfd0, 0x40c730, 0x40ce90,
    ///   0x41f680);
    /// - the half-domain and hue-adjust renderers: the colour codes first, alpha after the
    ///   colour floats on Windows (0x180260cb0, 0x180261d50, 0x1802636e0; 0x18026c780,
    ///   0x18026da20, 0x18026f480; 0x180266040, 0x180267900), and on Linux for 8 bits
    ///   (0x42e230, 0x414d20, 0x42e460) and for half input with hue adjust (0x423ba0); every
    ///   code first for 16-bit types on Linux (0x430550, 0x432830, 0x434b10, 0x414f20,
    ///   0x415130, 0x415340, 0x430740) and for half input without hue adjust (0x4373c0).
    fn in_place_order(&self) -> InPlaceOrder {
        let size = size_of::<I::Type>();
        let windows = cfg!(target_os = "windows");
        if !self.half_code && !self.hue_adjust {
            if windows || size == 1 {
                InPlaceOrder::Sequential
            } else {
                InPlaceOrder::OneAhead
            }
        } else if windows || size == 1 || (I::BIT_DEPTH == BitDepth::F16 && self.hue_adjust) {
            InPlaceOrder::ColorsFirst
        } else {
            InPlaceOrder::AllFirst
        }
    }
}

impl<I: BitDepthInfo + 'static, O: LutOutput> CpuOp for InvLut1DRenderer<I, O> {
    /// `apply(img, img, numPixels)` on F32 pixels, F32 to F32.
    fn apply(&self, rgba: &mut [f32]) {
        assert!(
            I::BIT_DEPTH == BitDepth::F32 && O::BIT_DEPTH == BitDepth::F32,
            "{self:?} can't work in place on floats"
        );
        for px in rgba.as_chunks_mut::<4>().0 {
            *px = self.render(*px).map(ChannelType::to_float);
        }
    }

    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());
        let (Some(input), Some(output)) = (
            I::Type::from_pixels(input),
            O::Type::from_pixels_mut(output),
        ) else {
            panic!("{self:?} got {in_name} to {out_name}");
        };
        assert_eq!(
            input.len(),
            output.len(),
            "{self:?}: the pixel counts differ"
        );
        for (inp, out) in input
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0)
        {
            *out = self.render(inp.map(ChannelType::to_float));
        }
    }

    /// `apply(pixel, pixel, 1)`, as `applyRGB` and `applyRGBA` call it (docs/improvements.md,
    /// I-41): the renderer reads the pixel's first bytes as `I` values and writes `O` values
    /// over them, in the order the wheel compiled ([`InPlaceOrder`]). The CPU engine runs an
    /// inverse LUT in place only to F32 output, or from F32 input.
    fn apply_pixel_in_place(&self, pixel: &mut [f32; 4]) -> Result<()> {
        let mut bytes = [0u8; 16];
        for (k, value) in pixel.iter().enumerate() {
            bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_ne_bytes());
        }
        let in_size = size_of::<I::Type>();
        let out_size = size_of::<O::Type>();
        let read = |bytes: &[u8; 16], k: usize| I::Type::read_ne(&bytes[k * in_size..]).to_float();
        let write = |bytes: &mut [u8; 16], k: usize, value: O::Type| {
            value.write_ne(&mut bytes[k * out_size..]);
        };

        if in_size >= out_size {
            // Each output value overwrites only values already read.
            let values = [0, 1, 2, 3].map(|k| read(&bytes, k));
            for (k, value) in self.render(values).into_iter().enumerate() {
                write(&mut bytes, k, value);
            }
        } else {
            assert_eq!(
                O::BIT_DEPTH,
                BitDepth::F32,
                "{self:?}: the CPU engine runs an inverse LUT in place to F32 only"
            );
            match self.in_place_order() {
                InPlaceOrder::Sequential | InPlaceOrder::OneAhead => {
                    debug_assert!(!self.half_code && !self.hue_adjust);
                    let one_ahead = self.in_place_order() == InPlaceOrder::OneAhead;
                    let mut next = read(&bytes, 0);
                    for c in 0..3 {
                        let value = next;
                        if one_ahead {
                            next = read(&bytes, c + 1);
                        }
                        write(&mut bytes, c, O::cast_value(self.invert(c, value)));
                        if !one_ahead {
                            next = read(&bytes, c + 1);
                        }
                    }
                    write(
                        &mut bytes,
                        3,
                        O::cast_value(sse_mul(next, self.alpha_scaling)),
                    );
                }
                order @ (InPlaceOrder::ColorsFirst | InPlaceOrder::AllFirst) => {
                    let rgb = [read(&bytes, 0), read(&bytes, 1), read(&bytes, 2)];
                    let early_alpha = read(&bytes, 3);
                    let out = self.render([rgb[0], rgb[1], rgb[2], early_alpha]);
                    for (k, value) in out.into_iter().take(3).enumerate() {
                        write(&mut bytes, k, value);
                    }
                    let alpha = if order == InPlaceOrder::AllFirst {
                        early_alpha
                    } else {
                        read(&bytes, 3)
                    };
                    write(
                        &mut bytes,
                        3,
                        O::cast_value(sse_mul(alpha, self.alpha_scaling)),
                    );
                }
            }
        }

        for (k, value) in pixel.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().expect("4 bytes"));
        }
        Ok(())
    }
}

/// The hue-adjust renderer of a forward LUT from `I` to `O`: a lookup for integer and half
/// input, which may need the LUT resampled first ([`NOT_PORTED_COMPOSE`]), or the float
/// renderer.
///
/// Port of the constructors of `Lut1DRendererHueAdjust<inBD, outBD>` and
/// `Lut1DRendererHalfCodeHueAdjust<inBD, outBD>` (Lut1DOpCPU.cpp:155-177, 311-443 @ v2.5.2).
/// The SIMD kernel their constructor picks (`m_applyLutFunc`, 320-339) is never called: their
/// `apply` has no SIMD branch.
fn forward_hue_adjust_renderer<I: BitDepthInfo + 'static, O: LutOutput>(
    lut: &Lut1DOpData,
) -> Result<Arc<dyn CpuOp>>
where
    I::Type: LookupIndex,
{
    if I::BIT_DEPTH != BitDepth::F32 {
        if !lut.may_lookup(I::BIT_DEPTH)? {
            return Err(Exception::new(NOT_PORTED_COMPOSE));
        }
        return Ok(Arc::new(Lut1DHueAdjustLookupRenderer::<I, O>::new(lut)));
    }
    Ok(Arc::new(Lut1DHueAdjustFloatRenderer::<O>::new(lut)))
}

/// The renderer of a forward LUT without hue adjust from `I` to `O`: a lookup for integer and
/// half input, which may need the LUT resampled first ([`NOT_PORTED_COMPOSE`]); for float
/// input, a half domain's renderer, or a standard domain's with the SIMD kernel `cpu`
/// dispatches to ([`lut1d_kernel`]).
///
/// Port of the constructors of `Lut1DRenderer<inBD, outBD>` and
/// `Lut1DRendererHalfCode<inBD, outBD>` (`BaseLut1DRenderer::BaseLut1DRenderer`, `update`,
/// `updateData`, Lut1DOpCPU.cpp:273-443 @ v2.5.2).
fn forward_renderer<I: BitDepthInfo + 'static, O: LutOutput>(
    lut: &Lut1DOpData,
    cpu: &CpuInfo,
) -> Result<Arc<dyn CpuOp>>
where
    I::Type: LookupIndex,
{
    if I::BIT_DEPTH != BitDepth::F32 {
        // `isLookup()`: a LUT the lookup can't use as it is is resampled first (`Compose`).
        if !lut.may_lookup(I::BIT_DEPTH)? {
            return Err(Exception::new(NOT_PORTED_COMPOSE));
        }
        return Ok(Arc::new(Lut1DLookupRenderer::<I, O>::new(lut)));
    }
    if lut.is_input_half_domain() {
        // `Lut1DRendererHalfCode::apply` never calls `m_applyLutFunc`.
        return Ok(Arc::new(Lut1DFloatRenderer::<O>::new(lut, None)));
    }
    Ok(Arc::new(Lut1DFloatRenderer::<O>::new(
        lut,
        lut1d_kernel(cpu, O::BIT_DEPTH),
    )))
}

/// The scalar profile of the renderer of a forward standard-domain LUT without hue adjust for
/// F32 input, to `out_bd`: what upstream's `Lut1DRenderer<BIT_DEPTH_F32, outBD>` computes for
/// a row of one pixel, or for any row where no SIMD kernel is dispatched.
///
/// Port of `Lut1DRenderer<BIT_DEPTH_F32, outBD>` without `m_applyLutFunc`
/// (Lut1DOpCPU.cpp:273-443, 659-720 @ v2.5.2).
pub fn get_lut1d_scalar_renderer(lut: &Lut1DOpData, out_bd: BitDepth) -> Result<Arc<dyn CpuOp>> {
    get_lut1d_profile_renderer(lut, out_bd, None)
}

/// The renderer of a forward standard-domain LUT without hue adjust for F32 input, to
/// `out_bd`, with `kernel` as its SIMD kernel whatever this machine dispatches (`None`: the
/// scalar profile): the numeric profiles the battery compares on every machine.
///
/// Port of `Lut1DRenderer<BIT_DEPTH_F32, outBD>` (Lut1DOpCPU.cpp:273-443, 652-720 @ v2.5.2).
pub fn get_lut1d_profile_renderer(
    lut: &Lut1DOpData,
    out_bd: BitDepth,
    kernel: Option<Lut1DKernel>,
) -> Result<Arc<dyn CpuOp>> {
    if lut.get_direction() != TransformDirection::Forward
        || lut.get_hue_adjust() != Lut1DHueAdjust::None
        || lut.is_input_half_domain()
    {
        return Err(Exception::new(
            "Lut1D: the scalar profile is the forward standard-domain renderer's, without hue \
             adjust.",
        ));
    }
    fn profile<O: LutOutput>(lut: &Lut1DOpData, kernel: Option<Lut1DKernel>) -> Arc<dyn CpuOp> {
        Arc::new(Lut1DFloatRenderer::<O>::new(lut, kernel))
    }
    Ok(match out_bd {
        BitDepth::Uint8 => profile::<Uint8>(lut, kernel),
        BitDepth::Uint10 => profile::<Uint10>(lut, kernel),
        BitDepth::Uint12 => profile::<Uint12>(lut, kernel),
        BitDepth::Uint16 => profile::<Uint16>(lut, kernel),
        BitDepth::F16 => profile::<F16>(lut, kernel),
        BitDepth::F32 => profile::<F32>(lut, kernel),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            return Err(Exception::new("Unsupported output bit depth"));
        }
    })
}

/// Port of `GetLut1DRenderer_OutBitDepth` (Lut1DOpCPU.cpp:1657-1695 @ v2.5.2) and
/// `GetForwardLut1DRenderer` (1626-1653). The inverse arm's four renderers are one
/// ([`InvLut1DRenderer`]).
fn renderer_for<I: BitDepthInfo + 'static, O: LutOutput>(
    lut: &Lut1DOpData,
    cpu: &CpuInfo,
) -> Result<Arc<dyn CpuOp>>
where
    I::Type: LookupIndex,
{
    match lut.get_direction() {
        TransformDirection::Forward => {
            // NB: Unlike bit-depth, the half domain status of a LUT
            //     may not be changed.
            if lut.get_hue_adjust() == Lut1DHueAdjust::None {
                forward_renderer::<I, O>(lut, cpu)
            } else {
                forward_hue_adjust_renderer::<I, O>(lut)
            }
        }
        TransformDirection::Inverse => Ok(Arc::new(InvLut1DRenderer::<I, O>::new(lut))),
    }
}

/// Port of `GetLut1DRenderer_InBitDepth` (Lut1DOpCPU.cpp:1697-1724 @ v2.5.2).
fn renderer_in<I: BitDepthInfo + 'static>(
    lut: &Lut1DOpData,
    out_bd: BitDepth,
    cpu: &CpuInfo,
) -> Result<Arc<dyn CpuOp>>
where
    I::Type: LookupIndex,
{
    match out_bd {
        BitDepth::Uint8 => renderer_for::<I, Uint8>(lut, cpu),
        BitDepth::Uint10 => renderer_for::<I, Uint10>(lut, cpu),
        BitDepth::Uint12 => renderer_for::<I, Uint12>(lut, cpu),
        BitDepth::Uint16 => renderer_for::<I, Uint16>(lut, cpu),
        BitDepth::F16 => renderer_for::<I, F16>(lut, cpu),
        BitDepth::F32 => renderer_for::<I, F32>(lut, cpu),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            Err(Exception::new("Unsupported output bit depth"))
        }
    }
}

/// `GetLookupValue(const float &)`: "When instantiating all templates this case is needed. But
/// it will never be used as the 32f is not a lookup case."
///
/// Port of `GetLookupValue(const float &)` (Lut1DOpCPU.cpp:50-55 @ v2.5.2).
impl LookupIndex for f32 {
    fn lookup_index(self) -> usize {
        unreachable!("F32 input is never looked up")
    }
}

/// The renderer of `lut` from `in_bd` values to `out_bd` values (module docs), with the SIMD
/// kernel this machine dispatches to.
///
/// Port of `GetLut1DRenderer` (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:1726-1752 @ v2.5.2).
pub fn get_lut1d_renderer(
    lut: &Lut1DOpData,
    in_bd: BitDepth,
    out_bd: BitDepth,
) -> Result<Arc<dyn CpuOp>> {
    get_lut1d_renderer_for_cpu(lut, in_bd, out_bd, CpuInfo::instance())
}

/// [`get_lut1d_renderer`] with the SIMD kernel `cpu` dispatches to.
pub fn get_lut1d_renderer_for_cpu(
    lut: &Lut1DOpData,
    in_bd: BitDepth,
    out_bd: BitDepth,
    cpu: &CpuInfo,
) -> Result<Arc<dyn CpuOp>> {
    match in_bd {
        BitDepth::Uint8 => renderer_in::<Uint8>(lut, out_bd, cpu),
        BitDepth::Uint10 => renderer_in::<Uint10>(lut, out_bd, cpu),
        BitDepth::Uint12 => renderer_in::<Uint12>(lut, out_bd, cpu),
        BitDepth::Uint16 => renderer_in::<Uint16>(lut, out_bd, cpu),
        BitDepth::F16 => renderer_in::<F16>(lut, out_bd, cpu),
        BitDepth::F32 => renderer_in::<F32>(lut, out_bd, cpu),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            Err(Exception::new("Unsupported input bit depth"))
        }
    }
}

#[cfg(test)]
#[path = "lut1d_op_cpu_tests.rs"]
mod tests;
