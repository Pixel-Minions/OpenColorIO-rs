// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 1D LUT's CPU renderers: a port of `src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp` @ v2.5.2,
//! its forward renderers.
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
//!
//! Not here yet, and an error until then:
//! - the SIMD kernels of the standard domain with float input (`Lut1DOpCPU_SSE2.cpp`, `_AVX`,
//!   `_AVX2`, `_AVX512`; WP 2.1c, 2.1d): every x86-64 CPU has SSE2, so upstream's
//!   `Lut1DRenderer<BIT_DEPTH_F32, outBD>` renders every row of more than one pixel with one
//!   of them. [`get_lut1d_renderer`] refuses that renderer ([`NOT_PORTED_SIMD`]) rather than
//!   render those rows with the scalar code; [`get_lut1d_scalar_renderer`] is its scalar
//!   profile, which upstream runs on rows of one pixel;
//! - hue adjust (WP 2.1b), the inverse renderers (WP 2.1f), and the lookups of a LUT that
//!   must first be resampled for the input bit depth (`Compose`, WP 2.1g).

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use super::lut1d_op::{
    NOT_PORTED_COMPOSE, NOT_PORTED_HUE_ADJUST, NOT_PORTED_INVERSE_RENDERER, NOT_PORTED_SIMD,
};
use super::lut1d_op_data::Lut1DOpData;
use crate::bit_depth_utils::{
    BitDepthInfo, ChannelType, Converter, F16, F32, Uint8, Uint10, Uint12, Uint16,
};
use crate::exception::{Exception, Result};
use crate::imath_half::{float_to_half, half_to_float};
use crate::math_utils::{
    clamp, lerpf, sanitize_float, sse_cvttps_epi32, sse_mul, std_max, std_min,
};
use crate::op::{CpuOp, Pixels, PixelsMut};
use crate::open_color_types::{BitDepth, Lut1DHueAdjust, TransformDirection};

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
    /// `OutType(value)`.
    fn from_float(value: f32) -> Self::Type;
}

macro_rules! lut_output {
    ($($bd:ty),*) => {$(
        impl LutOutput for $bd {
            #[inline]
            fn from_float(value: f32) -> Self::Type {
                FromFloat::from_float(value)
            }
        }
    )*};
}

lut_output!(Uint8, Uint10, Uint12, Uint16, F16, F32);

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
        Exception::new(format!(
            "Lut1D: a {} value above {} can't be looked up: upstream reads past the 1D LUT's {} \
             entries.",
            crate::open_color_types::bit_depth_to_string(I::BIT_DEPTH),
            self.max_index(),
            self.luts[0].len()
        ))
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
/// Lut1DOpCPU.cpp:374-443) and the float branches of `Lut1DRenderer::apply` (659-720, the
/// scalar loop) and `Lut1DRendererHalfCode::apply` (531-566 @ v2.5.2).
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
    output: PhantomData<fn() -> O>,
}

impl<O: LutOutput> fmt::Debug for Lut1DFloatRenderer<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(if self.half_code {
            "Lut1DRendererHalfCode"
        } else {
            "Lut1DRenderer (scalar)"
        })
        .field("out", &O::BIT_DEPTH)
        .field("entries", &self.luts[0].len())
        .finish()
    }
}

impl<O: LutOutput> Lut1DFloatRenderer<O> {
    /// Port of `BaseLut1DRenderer::updateData` (Lut1DOpCPU.cpp:374-443 @ v2.5.2) for float
    /// input.
    fn new(lut: &Lut1DOpData) -> Self {
        let dim = lut.get_array().get_length();
        // The input's maximum, `(float)GetBitDepthMaxValue(BIT_DEPTH_F32)`.
        let in_max = max_value::<F32>();
        Lut1DFloatRenderer {
            luts: float_tables::<O>(lut),
            alpha_scaling: max_value::<O>() / in_max,
            step: (dim as f32 - 1.0f32) / in_max,
            dim_minus_one: dim as f32 - 1.0f32,
            half_code: lut.is_input_half_domain(),
            output: PhantomData,
        }
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

    /// Port of `Lut1DRenderer::apply` and `Lut1DRendererHalfCode::apply`, the float branches
    /// (Lut1DOpCPU.cpp:531-566, 659-720 @ v2.5.2).
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

/// The renderer of a forward LUT without hue adjust from `I` to `O`: a lookup for integer and
/// half input, which may need the LUT resampled first ([`NOT_PORTED_COMPOSE`]); for float
/// input, a half domain's renderer, or a standard domain's, whose SIMD kernels are Phase 2's
/// ([`NOT_PORTED_SIMD`]).
///
/// Port of the constructors of `Lut1DRenderer<inBD, outBD>` and
/// `Lut1DRendererHalfCode<inBD, outBD>` (`BaseLut1DRenderer::BaseLut1DRenderer`, `update`,
/// `updateData`, Lut1DOpCPU.cpp:273-443 @ v2.5.2).
fn forward_renderer<I: BitDepthInfo + 'static, O: LutOutput>(
    lut: &Lut1DOpData,
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
    if !lut.is_input_half_domain() {
        // `m_applyLutFunc`: `SSE2GetLut1DApplyFunc(BIT_DEPTH_F32, outBD)` at least, on every
        // x86-64 CPU (Lut1DOpCPU.cpp:282-308).
        return Err(Exception::new(NOT_PORTED_SIMD));
    }
    Ok(Arc::new(Lut1DFloatRenderer::<O>::new(lut)))
}

/// The scalar profile of the renderer of a forward standard-domain LUT without hue adjust for
/// F32 input, to `out_bd`: what upstream's `Lut1DRenderer<BIT_DEPTH_F32, outBD>` computes for
/// a row of one pixel, or for any row where no SIMD kernel is dispatched. Until the kernels
/// are ported (WP 2.1c, 2.1d), [`get_lut1d_renderer`] refuses that renderer, and this is how
/// tests reach its scalar code.
///
/// Port of `Lut1DRenderer<BIT_DEPTH_F32, outBD>` without `m_applyLutFunc`
/// (Lut1DOpCPU.cpp:273-443, 659-720 @ v2.5.2).
pub fn get_lut1d_scalar_renderer(lut: &Lut1DOpData, out_bd: BitDepth) -> Result<Arc<dyn CpuOp>> {
    if lut.get_direction() != TransformDirection::Forward
        || lut.get_hue_adjust() != Lut1DHueAdjust::None
        || lut.is_input_half_domain()
    {
        return Err(Exception::new(
            "Lut1D: the scalar profile is the forward standard-domain renderer's, without hue \
             adjust.",
        ));
    }
    fn scalar<O: LutOutput>(lut: &Lut1DOpData) -> Arc<dyn CpuOp> {
        Arc::new(Lut1DFloatRenderer::<O>::new(lut))
    }
    Ok(match out_bd {
        BitDepth::Uint8 => scalar::<Uint8>(lut),
        BitDepth::Uint10 => scalar::<Uint10>(lut),
        BitDepth::Uint12 => scalar::<Uint12>(lut),
        BitDepth::Uint16 => scalar::<Uint16>(lut),
        BitDepth::F16 => scalar::<F16>(lut),
        BitDepth::F32 => scalar::<F32>(lut),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            return Err(Exception::new("Unsupported output bit depth"));
        }
    })
}

/// Port of `GetLut1DRenderer_OutBitDepth` (Lut1DOpCPU.cpp:1657-1695 @ v2.5.2) and
/// `GetForwardLut1DRenderer` (1626-1653).
fn renderer_for<I: BitDepthInfo + 'static, O: LutOutput>(
    lut: &Lut1DOpData,
) -> Result<Arc<dyn CpuOp>>
where
    I::Type: LookupIndex,
{
    match lut.get_direction() {
        TransformDirection::Forward => {
            // NB: Unlike bit-depth, the half domain status of a LUT
            //     may not be changed.
            if lut.get_hue_adjust() == Lut1DHueAdjust::None {
                forward_renderer::<I, O>(lut)
            } else {
                Err(Exception::new(NOT_PORTED_HUE_ADJUST))
            }
        }
        TransformDirection::Inverse => Err(Exception::new(NOT_PORTED_INVERSE_RENDERER)),
    }
}

/// Port of `GetLut1DRenderer_InBitDepth` (Lut1DOpCPU.cpp:1697-1724 @ v2.5.2).
fn renderer_in<I: BitDepthInfo + 'static>(
    lut: &Lut1DOpData,
    out_bd: BitDepth,
) -> Result<Arc<dyn CpuOp>>
where
    I::Type: LookupIndex,
{
    match out_bd {
        BitDepth::Uint8 => renderer_for::<I, Uint8>(lut),
        BitDepth::Uint10 => renderer_for::<I, Uint10>(lut),
        BitDepth::Uint12 => renderer_for::<I, Uint12>(lut),
        BitDepth::Uint16 => renderer_for::<I, Uint16>(lut),
        BitDepth::F16 => renderer_for::<I, F16>(lut),
        BitDepth::F32 => renderer_for::<I, F32>(lut),
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

/// The renderer of `lut` from `in_bd` values to `out_bd` values (module docs).
///
/// Port of `GetLut1DRenderer` (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:1726-1752 @ v2.5.2).
pub fn get_lut1d_renderer(
    lut: &Lut1DOpData,
    in_bd: BitDepth,
    out_bd: BitDepth,
) -> Result<Arc<dyn CpuOp>> {
    match in_bd {
        BitDepth::Uint8 => renderer_in::<Uint8>(lut, out_bd),
        BitDepth::Uint10 => renderer_in::<Uint10>(lut, out_bd),
        BitDepth::Uint12 => renderer_in::<Uint12>(lut, out_bd),
        BitDepth::Uint16 => renderer_in::<Uint16>(lut, out_bd),
        BitDepth::F16 => renderer_in::<F16>(lut, out_bd),
        BitDepth::F32 => renderer_in::<F32>(lut, out_bd),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            Err(Exception::new("Unsupported input bit depth"))
        }
    }
}

#[cfg(test)]
#[path = "lut1d_op_cpu_tests.rs"]
mod tests;
