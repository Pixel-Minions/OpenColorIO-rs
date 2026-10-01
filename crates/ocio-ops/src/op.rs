// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The op model: a port of `src/OpenColorIO/Op.h` and `Op.cpp` @ v2.5.2.
//!
//! So far only the CPU renderer interface, `OpCPU`, is here: the S2 spike's Log and Gamma
//! renderers implement it, and so do the CPU processor's bit-depth conversions. The rest of
//! `Op.h` (`OpData`, `Op`, `OpRcPtrVec`) is WP 1.2c.

use std::fmt::Debug;

/// RGBA pixels in one of the CPU processor's channel types (`BitDepthInfo<BD>::Type`): the
/// input of an op that converts from an image's bit depth.
#[derive(Debug, Clone, Copy)]
pub enum Pixels<'a> {
    /// `uint8_t`: 8-bit images.
    U8(&'a [u8]),
    /// `uint16_t`: 10-, 12- and 16-bit images.
    U16(&'a [u16]),
    /// `half`: F16 images.
    F16(&'a [half::f16]),
    /// `float`: F32 images.
    F32(&'a [f32]),
}

/// RGBA pixels in one of the CPU processor's channel types, to write: the output of an op that
/// converts to an image's bit depth.
#[derive(Debug)]
pub enum PixelsMut<'a> {
    /// `uint8_t`: 8-bit images.
    U8(&'a mut [u8]),
    /// `uint16_t`: 10-, 12- and 16-bit images.
    U16(&'a mut [u16]),
    /// `half`: F16 images.
    F16(&'a mut [half::f16]),
    /// `float`: F32 images.
    F32(&'a mut [f32]),
}

impl Pixels<'_> {
    /// The channel type's name, for messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Pixels::U8(_) => "uint8_t",
            Pixels::U16(_) => "uint16_t",
            Pixels::F16(_) => "half",
            Pixels::F32(_) => "float",
        }
    }
}

impl PixelsMut<'_> {
    /// The channel type's name, for messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            PixelsMut::U8(_) => "uint8_t",
            PixelsMut::U16(_) => "uint16_t",
            PixelsMut::F16(_) => "half",
            PixelsMut::F32(_) => "float",
        }
    }
}

/// A CPU renderer: processes RGBA `f32` pixels.
///
/// Upstream's `OpCPU::apply(inImg, outImg, numPixels)` reads `inImg` and writes `outImg`,
/// which may be the same buffer. Every renderer that works on `f32` pixels reads a whole
/// pixel before writing it, so the port applies them in place: `rgba` holds
/// `numPixels * 4` values, and the caller copies the input first when it has separate
/// buffers (`docs/architecture.md`, "CPU renderers and numeric profiles").
///
/// Port of `OpCPU` (src/OpenColorIO/Op.h:26-50 @ v2.5.2). The dynamic-property accessors
/// come with WP 1.2b.
pub trait CpuOp: Send + Sync + Debug {
    /// Port of `OpCPU::apply`, in place. `rgba.len()` must be a multiple of 4.
    fn apply(&self, rgba: &mut [f32]);

    /// Port of `OpCPU::apply(inImg, outImg, numPixels)` where the images' channel types need
    /// not be `float`: the first op of a CPU processor converts from its input bit depth, and
    /// the last one to its output bit depth (`CreateCPUEngine`,
    /// src/OpenColorIO/CPUProcessor.cpp:122-184 @ v2.5.2). `input` and `output` hold the same
    /// number of values, a multiple of 4.
    ///
    /// The default serves the renderers that process `float` only, which the engine uses at an
    /// end of the chain only when that end is F32: it processes a copy of the input in place,
    /// which gives what upstream's separate buffers give, as [`apply`](Self::apply) reads each
    /// pixel before writing it. Any other pair of types is a bug in the caller, and panics.
    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        match (input, output) {
            (Pixels::F32(input), PixelsMut::F32(output)) => {
                output.copy_from_slice(input);
                self.apply(output);
            }
            (input, output) => panic!(
                "{self:?} processes float pixels only, not {} to {}",
                input.type_name(),
                output.type_name()
            ),
        }
    }
}
