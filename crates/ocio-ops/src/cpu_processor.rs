// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CPU processor: a port of `src/OpenColorIO/CPUProcessor.cpp` @ v2.5.2.
//!
//! So far the generic bit-depth conversions, `BitDepthCast` and `CreateGenericBitDepthHelper`,
//! which image packing and the scanline helper call (chunks 1.1d, 1.1e). `CreateCPUEngine`,
//! `CreateScanlineHelper` and the processor itself are chunk 1.2d.

use std::fmt;
use std::hint::black_box;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::bit_depth_utils::{
    BitDepthInfo, ChannelType, Converter, F16, F32, Uint8, Uint10, Uint12, Uint16,
};
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, Pixels, PixelsMut};
use crate::open_color_types::BitDepth;

/// The conversion of RGBA pixels from bit depth `I` to bit depth `O`: each value is scaled by
/// `maxValue(O) / maxValue(I)`, then cast (`Converter<O>::CastValue`). From F32 to F32 it only
/// copies, as upstream specializes it.
///
/// Port of `BitDepthCast<inBD, outBD>` and `BitDepthCast<BIT_DEPTH_F32, BIT_DEPTH_F32>`
/// (src/OpenColorIO/CPUProcessor.cpp:20-66 @ v2.5.2).
pub struct BitDepthCast<I, O> {
    /// `m_scale = float(BitDepthInfo<outBD>::maxValue) / float(BitDepthInfo<inBD>::maxValue)`.
    scale: f32,
    bit_depths: PhantomData<fn() -> (I, O)>,
}

impl<I: BitDepthInfo, O: BitDepthInfo> fmt::Debug for BitDepthCast<I, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitDepthCast")
            .field("in", &I::BIT_DEPTH)
            .field("out", &O::BIT_DEPTH)
            .field("scale", &self.scale)
            .finish()
    }
}

impl<I: BitDepthInfo, O: Converter> BitDepthCast<I, O> {
    /// The conversion from `I` to `O`.
    pub fn new() -> Self {
        BitDepthCast {
            scale: O::MAX_VALUE as f32 / I::MAX_VALUE as f32,
            bit_depths: PhantomData,
        }
    }

    /// Whether this is the F32 to F32 specialization, which copies.
    fn copies() -> bool {
        I::BIT_DEPTH == BitDepth::F32 && O::BIT_DEPTH == BitDepth::F32
    }
}

impl<I: BitDepthInfo, O: Converter> Default for BitDepthCast<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: BitDepthInfo + 'static, O: Converter + 'static> CpuOp for BitDepthCast<I, O> {
    /// In place, `inImg == outImg`: only the F32 to F32 conversion runs so, as the last op of an
    /// F32 image, and it leaves the pixels alone (its `memcpy` needs separate buffers).
    fn apply(&self, _rgba: &mut [f32]) {
        assert!(
            Self::copies(),
            "{self:?} converts between channel types; it can't work in place"
        );
    }

    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());

        if Self::copies() {
            // BitDepthCast<BIT_DEPTH_F32, BIT_DEPTH_F32>: a memcpy, as the buffers differ.
            let (Pixels::F32(input), PixelsMut::F32(output)) = (input, output) else {
                panic!("{self:?} got {in_name} to {out_name}");
            };
            output.copy_from_slice(input);
            return;
        }

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

        // C++ multiplies by `m_scale`, a member it reads at run time, so a scale of 1 still
        // runs the multiply and quiets signaling NaNs; `black_box` keeps LLVM from folding it
        // (spike S4, docs/spikes/s4.md).
        let scale = black_box(self.scale);
        for (out, &value) in output.iter_mut().zip(input) {
            *out = O::cast_value(value.to_float() * scale);
        }
    }
}

/// The conversion between two bit depths, for the ends of a CPU processor's chain that no op
/// absorbs: "Unsupported bit-depth" for the bit depths the CPU processor doesn't take.
///
/// Port of `CreateGenericBitDepthHelper` (src/OpenColorIO/CPUProcessor.cpp:68-120 @ v2.5.2).
pub fn create_generic_bit_depth_helper(
    input: BitDepth,
    output: BitDepth,
) -> Result<Arc<dyn CpuOp>> {
    let unsupported = || Exception::new("Unsupported bit-depth");

    macro_rules! to_output {
        ($in:ty) => {
            match output {
                BitDepth::Uint8 => Arc::new(BitDepthCast::<$in, Uint8>::new()) as Arc<dyn CpuOp>,
                BitDepth::Uint10 => Arc::new(BitDepthCast::<$in, Uint10>::new()),
                BitDepth::Uint12 => Arc::new(BitDepthCast::<$in, Uint12>::new()),
                BitDepth::Uint16 => Arc::new(BitDepthCast::<$in, Uint16>::new()),
                BitDepth::F16 => Arc::new(BitDepthCast::<$in, F16>::new()),
                BitDepth::F32 => Arc::new(BitDepthCast::<$in, F32>::new()),
                BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
                    return Err(unsupported());
                }
            }
        };
    }

    Ok(match input {
        BitDepth::Uint8 => to_output!(Uint8),
        BitDepth::Uint10 => to_output!(Uint10),
        BitDepth::Uint12 => to_output!(Uint12),
        BitDepth::Uint16 => to_output!(Uint16),
        BitDepth::F16 => to_output!(F16),
        BitDepth::F32 => to_output!(F32),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => return Err(unsupported()),
    })
}
