// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 1D LUT's CPU renderers: a port of the lookup part of
//! `src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp` @ v2.5.2.
//!
//! The CPU engine renders the first op of a processor that is a 1D LUT from the processor's
//! input bit depth to F32 (`CreateCPUEngine`, src/OpenColorIO/CPUProcessor.cpp:140-146). For
//! integer and half input, a LUT with one entry per code is looked up, not interpolated
//! (`Lut1DRenderer` and `Lut1DRendererHalfCode`, the branches for `inBD != BIT_DEPTH_F32`,
//! Lut1DOpCPU.cpp:501-536, 622-650): that is the optimizer's bake, which makes such a LUT
//! (`Lut1DOpData::MakeLookupDomain`).
//!
//! The rest waits for Phase 2 (WP 2.5) and is an error until then ([`NOT_PORTED_F32`],
//! [`NOT_PORTED_COMPOSE`]): float input (interpolation, and its SIMD kernels), a LUT a lookup
//! must first resample (`Compose`), hue adjust, the inverse renderers, and lookups into
//! another output bit depth than F32, which the CPU engine never asks of a first op.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use super::lut1d_op::{NOT_PORTED_COMPOSE, NOT_PORTED_F32};
use super::lut1d_op_data::Lut1DOpData;
use crate::bit_depth_utils::{BitDepthInfo, ChannelType, F16, Uint8, Uint10, Uint12, Uint16};
use crate::exception::{Exception, Result};
use crate::math_utils::{sanitize_float, sse_mul};
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

/// The lookup renderer from `I` codes to F32 values: the forward 1D LUT, standard domain or
/// half domain, of one entry per code of `I`.
///
/// Port of `BaseLut1DRenderer<inBD, BIT_DEPTH_F32>` (its lookup tables, `updateData`,
/// Lut1DOpCPU.cpp:374-443) and the lookup branches of `Lut1DRenderer<inBD, BIT_DEPTH_F32>`
/// and `Lut1DRendererHalfCode<BIT_DEPTH_F16, BIT_DEPTH_F32>::apply` (Lut1DOpCPU.cpp:501-536,
/// 622-650 @ v2.5.2).
pub(crate) struct Lut1DLookupRenderer<I: BitDepthInfo> {
    /// `m_tmpLutR`, `m_tmpLutG` and `m_tmpLutB`: each entry times the output's maximum (1),
    /// sanitized (`L_ADJUST` for a float output).
    luts: [Vec<f32>; 3],
    /// `m_alphaScaling`: `(float)maxValue(F32) / (float)maxValue(inBD)`.
    alpha_scaling: f32,
    input: PhantomData<fn() -> I>,
}

impl<I: BitDepthInfo> fmt::Debug for Lut1DLookupRenderer<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lut1DLookupRenderer")
            .field("in", &I::BIT_DEPTH)
            .field("entries", &self.luts[0].len())
            .finish()
    }
}

impl<I: BitDepthInfo> Lut1DLookupRenderer<I>
where
    I::Type: LookupIndex,
{
    /// The renderer of `lut`, which a lookup with `I` codes may use as it is
    /// (`!mustResample`).
    ///
    /// Port of `BaseLut1DRenderer::updateData` (Lut1DOpCPU.cpp:374-443 @ v2.5.2), its lookup
    /// branch, for a float output.
    fn new(lut: &Lut1DOpData) -> Self {
        // `GetBitDepthMaxValue(BIT_DEPTH_F32)` as a float.
        let out_max = 1.0f32;

        let lut_values = lut.get_array().get_values();
        let dim = lut.get_array().get_length() as usize;
        let mut luts = [vec![0.0f32; dim], vec![0.0f32; dim], vec![0.0f32; dim]];
        // TODO: Would be faster if R, G, B were adjacent in memory?
        for i in 0..dim {
            for (c, table) in luts.iter_mut().enumerate() {
                // `L_ADJUST`: `(T)SanitizeFloat(val)` for a float output.
                table[i] = sanitize_float(sse_mul(lut_values[i * 3 + c], out_max));
            }
        }

        Lut1DLookupRenderer {
            luts,
            alpha_scaling: out_max / I::MAX_VALUE as f32,
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
    fn lookup(&self, c: usize, value: I::Type) -> f32 {
        self.luts[c][value.lookup_index()]
    }
}

impl<I: BitDepthInfo + 'static> CpuOp for Lut1DLookupRenderer<I>
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
    /// (Lut1DOpCPU.cpp:501-536, 622-650 @ v2.5.2).
    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());
        let (Some(input), PixelsMut::F32(output)) = (I::Type::from_pixels(input), output) else {
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
            out[3] = sse_mul(inp[3].to_float(), self.alpha_scaling);
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
    /// I-41): the renderer reads the pixel's first bytes as `I` codes and writes four floats
    /// over the same 16 bytes, in the order each wheel compiled:
    /// - 8-bit input, on both (Windows 0x180270f30, Linux 0x408840), reads each code after it
    ///   stores the float before it;
    /// - 10-, 12-, 16-bit and half input on Windows (0x180271610, folded for the three integer
    ///   depths; 0x180274480 half) does so too; on Linux (0x4083c0, 0x407f40, 0x407ac0,
    ///   0x415550) each code is read one step ahead, before the store of the float before it,
    ///   so only alpha is read after a store.
    fn apply_pixel_in_place(&self, pixel: &mut [f32; 4]) -> Result<()> {
        let mut bytes = [0u8; 16];
        for (k, value) in pixel.iter().enumerate() {
            bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_ne_bytes());
        }

        let size = size_of::<I::Type>();
        let read = |bytes: &[u8; 16], k: usize| I::Type::read_ne(&bytes[k * size..]);
        let write = |bytes: &mut [u8; 16], k: usize, value: f32| {
            bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_ne_bytes());
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
        write(&mut bytes, 3, sse_mul(a.to_float(), self.alpha_scaling));

        for (k, value) in pixel.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[4 * k..4 * k + 4].try_into().expect("4 bytes"));
        }
        Ok(())
    }
}

/// The lookup renderer for `I` codes.
fn lookup<I: BitDepthInfo + 'static>(lut: &Lut1DOpData) -> Arc<dyn CpuOp>
where
    I::Type: LookupIndex,
{
    Arc::new(Lut1DLookupRenderer::<I>::new(lut))
}

/// The renderer of `lut` from `in_bd` codes to `out_bd` values: the lookup of a forward LUT
/// without hue adjust, of one entry per code (or a half domain for half codes), to F32. The
/// others wait for Phase 2 (module docs).
///
/// Port of `GetLut1DRenderer` (src/OpenColorIO/ops/lut1d/Lut1DOpCPU.cpp:1657-1754 @ v2.5.2),
/// `GetLut1DRenderer_InBitDepth`, `GetLut1DRenderer_OutBitDepth` and `GetForwardLut1DRenderer`,
/// for the lookups.
pub fn get_lut1d_renderer(
    lut: &Lut1DOpData,
    in_bd: BitDepth,
    out_bd: BitDepth,
) -> Result<Arc<dyn CpuOp>> {
    match in_bd {
        BitDepth::Uint8
        | BitDepth::Uint10
        | BitDepth::Uint12
        | BitDepth::Uint16
        | BitDepth::F16
        | BitDepth::F32 => {}
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            return Err(Exception::new("Unsupported input bit depth"));
        }
    }
    match out_bd {
        BitDepth::Uint8
        | BitDepth::Uint10
        | BitDepth::Uint12
        | BitDepth::Uint16
        | BitDepth::F16
        | BitDepth::F32 => {}
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            return Err(Exception::new("Unsupported output bit depth"));
        }
    }

    // The inverse renderers, the hue adjust renderers and float input (interpolation) are
    // Phase 2's.
    if lut.get_direction() == TransformDirection::Inverse
        || lut.get_hue_adjust() != Lut1DHueAdjust::None
        || in_bd == BitDepth::F32
    {
        return Err(Exception::new(NOT_PORTED_F32));
    }
    // A lookup into a LUT of another size first resamples it (`Compose`).
    if !lut.may_lookup(in_bd)? {
        return Err(Exception::new(NOT_PORTED_COMPOSE));
    }
    if out_bd != BitDepth::F32 {
        return Err(Exception::new(
            "Lut1D: the 1D LUT lookups to integer or half output are not ported yet (Phase 2, \
             WP 2.5).",
        ));
    }

    Ok(match in_bd {
        BitDepth::Uint8 => lookup::<Uint8>(lut),
        BitDepth::Uint10 => lookup::<Uint10>(lut),
        BitDepth::Uint12 => lookup::<Uint12>(lut),
        BitDepth::Uint16 => lookup::<Uint16>(lut),
        BitDepth::F16 => lookup::<F16>(lut),
        BitDepth::F32 | BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
            unreachable!("checked above")
        }
    })
}

#[cfg(test)]
#[path = "lut1d_op_cpu_tests.rs"]
mod tests;
